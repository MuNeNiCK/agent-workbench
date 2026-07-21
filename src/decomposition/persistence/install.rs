use super::super::*;

pub(in crate::decomposition) fn decomposition_v2_storage(conn: &Connection) -> Result<bool> {
    conn.query_row(
        "select exists(select 1 from pragma_table_info('decomposition_plans') where name='content_identity')",
        [],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(in crate::decomposition) fn reconciliation_v2_storage(conn: &Connection) -> Result<bool> {
    conn.query_row(
        "select exists(select 1 from pragma_table_info('decomposition_reconciliation_gates') where name='boundary_selector')",
        [],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(crate) fn install_discovered_plans(conn: &Connection, plans: &[ParsedPlan]) -> Result<()> {
    let project_id = crate::db::project_id(conn)?;
    for parsed in plans {
        let design_version_id = resolve_plan_design(conn, project_id, parsed)?;
        let work_unit_id = parsed.document.as_ref().map_or(Ok(None), |document| {
            resolve_work_binding(conn, project_id, design_version_id, document)
        })?;
        let has_derived_state = work_unit_id.is_some_and(|work| {
            conn.query_row(
                r#"
                select exists(
                  select 1 from task_derivations derivation
                  join tasks task on task.id=derivation.task_id
                  join design_requirements requirement on requirement.id=derivation.design_requirement_id
                  where task.work_unit_id=?1 and requirement.design_version_id=?2
                )
                "#,
                params![work, design_version_id],
                |row| row.get::<_, bool>(0),
            )
            .unwrap_or(true)
        });
        let (status, binding_issue) = if parsed.document.is_none() {
            (
                "incomplete",
                Some("formal decomposition plan metadata is required"),
            )
        } else {
            match (work_unit_id, has_derived_state) {
                (None, _) => (
                    "incomplete",
                    Some("one exact work owner must be selected before application"),
                ),
                (Some(_), true) => (
                    "incomplete",
                    Some("existing decomposition requires an explicit total item mapping"),
                ),
                (Some(_), false) => ("ready", None),
            }
        };
        let plan_key = parsed.document.as_ref().map_or_else(
            || format!("legacy-document-{}", &parsed.source_identity[..12]),
            |document| document.key.clone(),
        );
        let design_fingerprint = parsed.document.as_ref().map_or_else(
            || design_identity(conn, project_id, design_version_id),
            |document| Ok(document.design_fingerprint.clone()),
        )?;
        if decomposition_v2_storage(conn)? {
            conn.execute(
                r#"
                insert into decomposition_plans(
                  project_id,work_unit_id,design_version_id,design_package_id,plan_key,revision,source_path,
                  source_identity,document_content,content_identity,source_kind,design_fingerprint,status,binding_issue,created_at
                ) values(?1,?2,?3,(select design_package_id from design_versions where id=?3),?4,1,?5,?6,?7,?8,'document',?9,?10,?11,current_timestamp)
                "#,
                params![project_id, work_unit_id, design_version_id, plan_key,
                    stored_source_path(parsed), parsed.source_identity, parsed.content,
                    parsed.content_identity, design_fingerprint, status, binding_issue],
            )?;
        } else {
            conn.execute(
                r#"
                insert into decomposition_plans(
                  project_id,work_unit_id,design_version_id,plan_key,revision,source_path,
                  source_identity,source_kind,design_fingerprint,status,binding_issue,created_at
                ) values(?1,?2,?3,?4,1,?5,?6,'document',?7,?8,?9,current_timestamp)
                "#,
                params![
                    project_id,
                    work_unit_id,
                    design_version_id,
                    plan_key,
                    stored_source_path(parsed),
                    parsed.source_identity,
                    design_fingerprint,
                    status,
                    binding_issue
                ],
            )?;
        }
        let plan_id = conn.last_insert_rowid();
        let Some(document) = parsed.document.as_ref() else {
            continue;
        };
        let mut slice_ids = BTreeMap::new();
        for slice in &document.slices {
            conn.execute(
                "insert into decomposition_slices(project_id,decomposition_plan_id,slice_key,title,slice_order) values(?1,?2,?3,?4,?5)",
                params![project_id, plan_id, slice.key, slice.title, slice.order],
            )?;
            slice_ids.insert(slice.key.as_str(), conn.last_insert_rowid());
        }
        for slice in &document.slices {
            for dependency in &slice.depends_on {
                conn.execute(
                    "insert into decomposition_slice_dependencies(project_id,decomposition_plan_id,predecessor_slice_id,successor_slice_id) values(?1,?2,?3,?4)",
                    params![
                        project_id,
                        plan_id,
                        slice_ids[dependency.as_str()],
                        slice_ids[slice.key.as_str()]
                    ],
                )?;
            }
        }
        for item in &document.items {
            conn.execute(
                r#"
                insert into decomposition_items(
                  project_id,decomposition_plan_id,item_key,title,details,outcome,observation,
                  evidence_owner,evidence_kind,slice_id,status
                ) values(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'open')
                "#,
                params![
                    project_id,
                    plan_id,
                    item.key,
                    item.title,
                    item.details,
                    item.completion.outcome,
                    item.completion.observation,
                    item.completion.evidence_owner,
                    item.completion.evidence_kind,
                    slice_ids[item.slice.as_str()],
                ],
            )?;
            let item_id = conn.last_insert_rowid();
            for requirement_key in &item.requirements {
                let requirement_id = conn
                    .query_row(
                        "select id from design_requirements where project_id=?1 and design_version_id=?2 and requirement_key=?3 and status='active'",
                        params![project_id, design_version_id, requirement_key],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()?
                    .with_context(|| {
                        format!(
                            "decomposition item {} references an unavailable current requirement",
                            item.key
                        )
                    })?;
                conn.execute(
                    "insert into decomposition_item_requirements(project_id,decomposition_item_id,design_requirement_id) values(?1,?2,?3)",
                    params![project_id, item_id, requirement_id],
                )?;
            }
            for (index, boundary) in item.checklist.iter().enumerate() {
                conn.execute(
                    "insert into decomposition_item_checklist_boundaries(project_id,decomposition_item_id,boundary_key,title,condition_text,evidence_kind,boundary_order) values(?1,?2,?3,?3,?4,?5,?6)",
                    params![
                        project_id,
                        item_id,
                        boundary.key,
                        boundary.condition,
                        boundary.evidence_kind,
                        index as i64 + 1,
                    ],
                )?;
                let boundary_id = conn.last_insert_rowid();
                for gate in &boundary.gates {
                    conn.execute(
                        "insert into decomposition_item_checklist_boundary_gates(project_id,decomposition_item_checklist_boundary_id,gate_key) values(?1,?2,?3)",
                        params![project_id, boundary_id, gate],
                    )?;
                }
            }
            let mut gates = item.completion.gates.iter().collect::<BTreeSet<_>>();
            for boundary in &item.checklist {
                gates.extend(boundary.gates.iter());
            }
            for gate in gates {
                conn.execute(
                    "insert into decomposition_item_gates(project_id,decomposition_item_id,gate_key) values(?1,?2,?3)",
                    params![project_id, item_id, gate],
                )?;
            }
        }
    }
    Ok(())
}
