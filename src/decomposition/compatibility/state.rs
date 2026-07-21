use super::*;

pub(crate) fn uncovered_derived_bundle_count(
    conn: &Connection,
    plans: &[ParsedPlan],
) -> Result<usize> {
    Ok(uncovered_derived_bundles(conn, plans)?.len())
}

pub(crate) fn install_uncovered_derived_bundles(
    conn: &Connection,
    plans: &[ParsedPlan],
) -> Result<()> {
    let project_id = crate::db::project_id(conn)?;
    for (work_unit_id, design_version_id) in uncovered_derived_bundles(conn, plans)? {
        let design_fingerprint: String = conn.query_row(
            "select content_hash from design_versions where id=?1 and project_id=?2",
            params![design_version_id, project_id],
            |row| row.get(0),
        )?;
        let parsed_source = write_derived_bundle_source(
            conn,
            project_id,
            work_unit_id,
            design_version_id,
            &design_fingerprint,
        )?;
        if decomposition_v2_storage(conn)? {
            conn.execute(
                r#"
                insert into decomposition_plans(
                  project_id,work_unit_id,design_version_id,design_package_id,plan_key,revision,source_path,
                  source_identity,document_content,content_identity,source_kind,design_fingerprint,status,binding_issue,created_at
                ) values(?1,?2,?3,(select design_package_id from design_versions where id=?3),'derived-bundle',1,?4,?5,?6,?7,'derived_bundle',?8,'incomplete',
                  'existing decomposition requires an explicit total item mapping',current_timestamp)
                "#,
                params![project_id, work_unit_id, design_version_id,
                    stored_source_path(&parsed_source), parsed_source.source_identity,
                    parsed_source.content, parsed_source.content_identity, design_fingerprint],
            )?;
        } else {
            conn.execute(
                r#"
                insert into decomposition_plans(
                  project_id,work_unit_id,design_version_id,plan_key,revision,source_path,
                  source_identity,source_kind,design_fingerprint,status,binding_issue,created_at
                ) values(?1,?2,?3,'derived-bundle',1,?4,?5,'derived_bundle',?6,'incomplete',
                  'existing decomposition requires an explicit total item mapping',current_timestamp)
                "#,
                params![project_id, work_unit_id, design_version_id,
                    stored_source_path(&parsed_source), parsed_source.source_identity,
                    design_fingerprint],
            )?;
        }
        let plan_id = conn.last_insert_rowid();
        let mut phase_statement = conn.prepare(
            r#"
            select id,title from work_phases
            where project_id=?1 and work_unit_id=?2 and design_version_id=?3
              and status != 'superseded'
            order by phase_order,id
            "#,
        )?;
        let phases = phase_statement
            .query_map(
                params![project_id, work_unit_id, design_version_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut slice_ids = BTreeMap::new();
        for (index, (source_phase_id, title)) in phases.iter().enumerate() {
            conn.execute(
                "insert into decomposition_slices(project_id,decomposition_plan_id,slice_key,title,slice_order) values(?1,?2,?3,?4,?5)",
                params![
                    project_id,
                    plan_id,
                    format!("source-phase-{source_phase_id}"),
                    title,
                    index as i64 + 1
                ],
            )?;
            slice_ids.insert(*source_phase_id, conn.last_insert_rowid());
        }
        let mut task_statement = conn.prepare(
            r#"
            select distinct task.id,task.title,coalesce(task.details,''),
                   coalesce(task.completion_condition,'')
            from tasks task
            join task_derivations derivation on derivation.task_id=task.id
            join design_requirements requirement on requirement.id=derivation.design_requirement_id
            where task.work_unit_id=?1 and requirement.design_version_id=?2
            order by task.id
            "#,
        )?;
        let tasks = task_statement
            .query_map(params![work_unit_id, design_version_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (source_task_id, title, details, completion_condition) in tasks {
            let source_phases = source_phase_ids(conn, source_task_id, &slice_ids)?;
            let source_phase_id = match source_phases.as_slice() {
                [phase] => Some(*phase),
                _ => None,
            };
            let slice_id = source_phase_id.and_then(|phase| slice_ids.get(&phase).copied());
            conn.execute(
                r#"
                insert into decomposition_items(
                  project_id,decomposition_plan_id,item_key,title,details,outcome,observation,
                  evidence_owner,evidence_kind,slice_id,status
                ) values(?1,?2,?3,?4,?5,?6,'',?7,'migration',?8,'open')
                "#,
                params![
                    project_id,
                    plan_id,
                    format!("source-task-{source_task_id}"),
                    title,
                    details,
                    completion_condition,
                    format!("work:{work_unit_id}"),
                    slice_id,
                ],
            )?;
            let item_id = conn.last_insert_rowid();
            let mut requirement_statement = conn.prepare(
                r#"
                select distinct requirement.id
                from task_derivations derivation
                join design_requirements requirement on requirement.id=derivation.design_requirement_id
                where derivation.task_id=?1 and requirement.design_version_id=?2
                order by requirement.id
                "#,
            )?;
            let requirements = requirement_statement
                .query_map(params![source_task_id, design_version_id], |row| {
                    row.get::<_, i64>(0)
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for requirement_id in requirements {
                conn.execute(
                    "insert into decomposition_item_requirements(project_id,decomposition_item_id,design_requirement_id) values(?1,?2,?3)",
                    params![project_id, item_id, requirement_id],
                )?;
            }
            let checklist_items = source_checklist_items(conn, source_task_id)?;
            for (index, (checklist_item_id, checklist_title, condition)) in
                checklist_items.iter().enumerate()
            {
                conn.execute(
                    "insert into decomposition_item_checklist_boundaries(project_id,decomposition_item_id,boundary_key,title,condition_text,evidence_kind,boundary_order) values(?1,?2,?3,?4,?5,'migration',?6)",
                    params![
                        project_id,
                        item_id,
                        format!("source-checklist-{checklist_item_id}"),
                        checklist_title,
                        condition,
                        index as i64 + 1,
                    ],
                )?;
            }
            let mut gate_statement = conn.prepare(
                "select distinct gate_key from validation_gates where task_id=?1 and status!='stale' order by gate_key",
            )?;
            let gates = gate_statement
                .query_map([source_task_id], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for gate in gates {
                conn.execute(
                    "insert into decomposition_item_gates(project_id,decomposition_item_id,gate_key) values(?1,?2,?3)",
                    params![project_id, item_id, gate],
                )?;
            }
            let mapping_state = if source_phases.len() > 1 || checklist_items.len() > 1 {
                "ambiguous"
            } else if source_phases.is_empty() || checklist_items.is_empty() {
                "missing"
            } else {
                "exact"
            };
            let issue = match mapping_state {
                "ambiguous" => Some("multiple source endpoints require an explicit item mapping"),
                "missing" => Some("a required source endpoint has no exact mapping"),
                _ => None,
            };
            if checklist_items.is_empty() {
                conn.execute(
                    "insert into decomposition_migration_sources(project_id,decomposition_plan_id,decomposition_item_id,source_task_id,source_phase_id,mapping_state,issue) values(?1,?2,?3,?4,?5,?6,?7)",
                    params![project_id, plan_id, item_id, source_task_id, source_phase_id, mapping_state, issue],
                )?;
            } else {
                for (checklist_item_id, _, _) in &checklist_items {
                    conn.execute(
                        "insert into decomposition_migration_sources(project_id,decomposition_plan_id,decomposition_item_id,source_task_id,source_checklist_item_id,source_phase_id,mapping_state,issue) values(?1,?2,?3,?4,?5,?6,?7,?8)",
                        params![project_id, plan_id, item_id, source_task_id, checklist_item_id, source_phase_id, mapping_state, issue],
                    )?;
                }
            }
        }
    }
    Ok(())
}
