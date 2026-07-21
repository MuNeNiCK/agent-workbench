use super::super::*;

pub(in crate::decomposition) fn validate_preserve_effects(
    conn: &Connection,
    plan_id: i64,
) -> Result<()> {
    let incompatible_checklist: i64 = conn.query_row(
        r#"
        select count(*)
        from decomposition_reconciliation_checklist_items mapping
        join checklist_items source on source.id=mapping.source_checklist_item_id
        join decomposition_application_boundaries application
          on application.decomposition_item_checklist_boundary_id=mapping.successor_boundary_id
        join checklist_items target on target.id=application.checklist_item_id
        where mapping.decomposition_plan_id=?1 and mapping.effect='preserve'
          and coalesce(source.completion_condition,source.title)
              !=coalesce(target.completion_condition,target.title)
        "#,
        [plan_id],
        |row| row.get(0),
    )?;
    if incompatible_checklist != 0 {
        bail!("preserve checklist effect requires unchanged boundary meaning");
    }

    let incompatible_gates: i64 = conn.query_row(
        r#"
        select count(*)
        from decomposition_reconciliation_gates mapping
        join validation_gates source on source.id=mapping.source_validation_gate_id
        join decomposition_application_gates application
          on application.decomposition_item_gate_id=mapping.successor_item_gate_id
        join validation_gates target on target.id=application.validation_gate_id
        where mapping.decomposition_plan_id=?1 and mapping.effect='preserve'
          and (source.gate_key!=target.gate_key
               or coalesce(source.command,'')!=coalesce(target.command,'')
               or source.expected_result!=target.expected_result
               or coalesce(source.environment,'')!=coalesce(target.environment,'')
               or coalesce(source.timeout,'')!=coalesce(target.timeout,'')
               or coalesce(source.artifact_requirements,'')!=coalesce(target.artifact_requirements,''))
        "#,
        [plan_id],
        |row| row.get(0),
    )?;
    if incompatible_gates != 0 {
        bail!("preserve gate effect requires unchanged validation meaning");
    }

    let incompatible_phases: i64 = conn.query_row(
        r#"
        select count(*)
        from decomposition_reconciliation_phases mapping
        join work_phases source on source.id=mapping.source_phase_id
        join decomposition_items item on item.slice_id=mapping.successor_slice_id
        join decomposition_applications application
          on application.decomposition_plan_id=mapping.decomposition_plan_id
         and application.decomposition_item_id=item.id
        join phase_epochs target on target.id=application.phase_id
        where mapping.decomposition_plan_id=?1 and mapping.effect='preserve'
          and (source.title!=target.title or source.kind!=target.kind)
        "#,
        [plan_id],
        |row| row.get(0),
    )?;
    if incompatible_phases != 0 {
        bail!("preserve phase effect requires unchanged phase meaning");
    }

    let incompatible_dependencies: i64 = conn.query_row(
        r#"
        select count(*)
        from decomposition_reconciliation_dependencies mapping
        join work_phase_dependencies source on source.id=mapping.source_dependency_id
        join decomposition_application_dependencies application
          on application.decomposition_slice_dependency_id=mapping.successor_dependency_id
        join work_phase_dependencies target on target.id=application.work_phase_dependency_id
        join decomposition_slice_dependencies declared
          on declared.id=mapping.successor_dependency_id
        join decomposition_reconciliation_phases from_mapping
          on from_mapping.decomposition_plan_id=mapping.decomposition_plan_id
         and from_mapping.successor_slice_id=declared.predecessor_slice_id
         and from_mapping.disposition='retained'
        join decomposition_reconciliation_phases to_mapping
          on to_mapping.decomposition_plan_id=mapping.decomposition_plan_id
         and to_mapping.successor_slice_id=declared.successor_slice_id
         and to_mapping.disposition='retained'
        where mapping.decomposition_plan_id=?1 and mapping.effect='preserve'
          and (source.dependency_type!=target.dependency_type
               or source.reason!=target.reason
               or (source.status!='open'
                   and (from_mapping.effect!='preserve' or to_mapping.effect!='preserve'))
               or (source.status='satisfied'
                   and coalesce(trim(source.evidence_ref),'')='')
               or (source.status='accepted' and source.authority_event_id is null))
        "#,
        [plan_id],
        |row| row.get(0),
    )?;
    if incompatible_dependencies != 0 {
        bail!(
            "preserve dependency effect requires current successor endpoints and qualifying evidence"
        );
    }
    Ok(())
}

pub(in crate::decomposition) fn retire_reconciliation_task_identities(
    conn: &Connection,
    project_id: i64,
    plan_id: i64,
) -> Result<()> {
    let identities = conn
        .prepare(
            r#"
            select distinct identity.id
            from decomposition_reconciliation_tasks mapping
            join task_revision_aliases alias
              on alias.project_id=mapping.project_id
             and alias.historical_task_id=mapping.source_task_id
            join task_revisions revision on revision.id=alias.task_revision_id
            join task_identities identity on identity.id=revision.task_identity_id
            where mapping.project_id=?1 and mapping.decomposition_plan_id=?2
              and mapping.disposition='retired'
            order by identity.id
            "#,
        )?
        .query_map(params![project_id, plan_id], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let expected: i64 = conn.query_row(
        "select count(*) from decomposition_reconciliation_tasks where project_id=?1 and decomposition_plan_id=?2 and disposition='retired'",
        params![project_id, plan_id],
        |row| row.get(0),
    )?;
    let mapped: i64 = conn.query_row(
        r#"
        select count(*)
        from decomposition_reconciliation_tasks mapping
        join task_revision_aliases alias
          on alias.project_id=mapping.project_id
         and alias.historical_task_id=mapping.source_task_id
        join task_revisions revision on revision.id=alias.task_revision_id
        join task_identities identity on identity.id=revision.task_identity_id
        where mapping.project_id=?1 and mapping.decomposition_plan_id=?2
          and mapping.disposition='retired'
        "#,
        params![project_id, plan_id],
        |row| row.get(0),
    )?;
    if mapped != expected {
        bail!("each retired task mapping requires one canonical identity alias");
    }
    for identity_id in identities {
        let retained_by_successor: bool = conn.query_row(
            r#"
            select exists(
              select 1 from decomposition_applications application
              join task_revision_aliases alias
                on alias.project_id=application.project_id
               and alias.historical_task_id=application.task_id
              join task_revisions revision on revision.id=alias.task_revision_id
              where application.project_id=?1
                and application.decomposition_plan_id=?2
                and revision.task_identity_id=?3
            )
            "#,
            params![project_id, plan_id, identity_id],
            |row| row.get(0),
        )?;
        if retained_by_successor {
            bail!("a retired task identity cannot also be retained by the successor Plan");
        }
        let revisions = conn.execute(
            "update task_revisions set status='retired' where project_id=?1 and task_identity_id=?2 and status='current'",
            params![project_id, identity_id],
        )?;
        let identities = conn.execute(
            "update task_identities set status='retired' where project_id=?1 and id=?2 and status='current'",
            params![project_id, identity_id],
        )?;
        if revisions != 1 || identities != 1 {
            bail!("retired task mapping lacks one current canonical revision");
        }
        conn.execute(
            "update task_phase_memberships set state='out_of_scope' where project_id=?1 and task_identity_id=?2 and state in ('open','blocked')",
            params![project_id, identity_id],
        )?;
        conn.execute(
            "update task_identity_dependencies set state='out_of_scope' where project_id=?1 and state='open' and (from_task_identity_id=?2 or to_task_identity_id=?2)",
            params![project_id, identity_id],
        )?;
    }
    Ok(())
}

pub(in crate::decomposition) fn validate_application_owner(
    conn: &Connection,
    project_id: i64,
    design_version_id: i64,
    work_unit_id: i64,
) -> Result<()> {
    let approved: bool = conn
        .query_row(
            r#"
            select version.status='approved'
                   and version.approved_by_authority_event_id is not null
                   and version.approved_at is not null
                   and package.current_design_version_id=version.id
            from design_versions version
            join design_packages package on package.id=version.design_package_id
            where version.id=?1 and version.project_id=?2
            "#,
            params![design_version_id, project_id],
            |row| row.get(0),
        )
        .optional()?
        .context("selected design version not found")?;
    if !approved {
        bail!("only the exact current approved design version can be decomposed");
    }
    let open: bool = conn.query_row(
        "select exists(select 1 from work_units where id=?1 and project_id=?2 and status in ('open','blocked'))",
        params![work_unit_id, project_id],
        |row| row.get(0),
    )?;
    if !open {
        bail!("open work owner not found");
    }
    Ok(())
}

pub(in crate::decomposition) fn validate_empty_application_target(
    conn: &Connection,
    project_id: i64,
    design_version_id: i64,
    work_unit_id: i64,
) -> Result<()> {
    let occupied: bool = conn.query_row(
        r#"
        select exists(select 1 from checklists where project_id=?1 and design_version_id=?2 and work_unit_id=?3)
            or exists(select 1 from work_phases where project_id=?1 and design_version_id=?2 and work_unit_id=?3 and status!='superseded')
            or exists(
              select 1 from task_derivations derivation
              join tasks task on task.id=derivation.task_id
              join design_requirements requirement on requirement.id=derivation.design_requirement_id
              where requirement.project_id=?1 and requirement.design_version_id=?2
                and task.work_unit_id=?3 and derivation.status='active'
            )
        "#,
        params![project_id, design_version_id, work_unit_id],
        |row| row.get(0),
    )?;
    if occupied {
        bail!("the selected owner has existing decomposition state; reconcile it explicitly");
    }
    Ok(())
}

pub(in crate::decomposition) fn validate_ready_graph(
    conn: &Connection,
    project_id: i64,
    plan_id: i64,
    design_version_id: i64,
) -> Result<()> {
    let active = string_column(
        conn,
        "select requirement_key from design_requirements where project_id=?1 and design_version_id=?2 and status='active' order by requirement_key",
        params![project_id, design_version_id],
    )?;
    let covered = string_column(
        conn,
        r#"
        select distinct requirement.requirement_key
        from decomposition_item_requirements link
        join decomposition_items item on item.id=link.decomposition_item_id
        join design_requirements requirement on requirement.id=link.design_requirement_id
        where item.decomposition_plan_id=?1 order by requirement.requirement_key
        "#,
        [plan_id],
    )?;
    if active.is_empty() || active != covered {
        bail!("Decomposition Plan must cover every current requirement");
    }
    let invalid_gate_count: i64 = conn.query_row(
        r#"
        select count(*) from decomposition_item_gates item_gate
        join decomposition_items item on item.id=item_gate.decomposition_item_id
        where item.decomposition_plan_id=?1 and not exists(
          select 1 from validation_gate_templates template
          join validation_gate_template_requirements template_requirement
            on template_requirement.validation_gate_template_id=template.id
          join decomposition_item_requirements item_requirement
            on item_requirement.design_requirement_id=template_requirement.design_requirement_id
           and item_requirement.decomposition_item_id=item.id
          where template.project_id=?2 and template.design_version_id=?3
            and template.status='active' and template.gate_key=item_gate.gate_key
        )
        "#,
        params![plan_id, project_id, design_version_id],
        |row| row.get(0),
    )?;
    let missing_gate_count: i64 = conn.query_row(
        r#"
        select count(*) from (
          select distinct item.id,template.gate_key
          from decomposition_items item
          join decomposition_item_requirements item_requirement on item_requirement.decomposition_item_id=item.id
          join validation_gate_template_requirements template_requirement
            on template_requirement.design_requirement_id=item_requirement.design_requirement_id
          join validation_gate_templates template on template.id=template_requirement.validation_gate_template_id
          where item.decomposition_plan_id=?1 and template.project_id=?2
            and template.design_version_id=?3 and template.status='active'
            and not exists(
              select 1 from decomposition_item_gates item_gate
              where item_gate.decomposition_item_id=item.id and item_gate.gate_key=template.gate_key
            )
        )
        "#,
        params![plan_id, project_id, design_version_id],
        |row| row.get(0),
    )?;
    if invalid_gate_count != 0 || missing_gate_count != 0 {
        bail!("Decomposition Plan gate coverage is not exact");
    }
    Ok(())
}

pub(in crate::decomposition) fn publish_application(
    conn: &Connection,
    project_id: i64,
    plan_id: i64,
    design_version_id: i64,
    work_unit_id: i64,
    retained_tasks: &BTreeMap<i64, (i64, ReconciliationEffect)>,
) -> Result<()> {
    let plan_key: String = conn.query_row(
        "select plan_key from decomposition_plans where id=?1",
        [plan_id],
        |row| row.get(0),
    )?;
    conn.execute(
        "insert into checklists(project_id,work_unit_id,design_version_id,title,status,created_at) values(?1,?2,?3,?4,'active',current_timestamp)",
        params![project_id, work_unit_id, design_version_id, format!("Decomposition {plan_key}")],
    )?;
    let checklist_id = conn.last_insert_rowid();

    let mut slice_statement = conn.prepare(
        "select id,slice_key,title,slice_order from decomposition_slices where decomposition_plan_id=?1 order by slice_order",
    )?;
    let slices = slice_statement
        .query_map([plan_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let phase_predecessors = conn
        .prepare(
            "select successor_slice_id,source_phase_id from decomposition_reconciliation_phases where decomposition_plan_id=?1 and disposition='retained' order by successor_slice_id,source_phase_id",
        )?
        .query_map([plan_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .try_fold(BTreeMap::new(), |mut predecessors, (slice, source)| {
            if predecessors.insert(slice, source).is_some() {
                bail!("multiple predecessor phases cannot collapse into one successor epoch");
            }
            Ok::<_, anyhow::Error>(predecessors)
        })?;
    let mut phase_ids = BTreeMap::new();
    for (slice_id, key, title, order) in &slices {
        let key_is_occupied: bool = conn.query_row(
            "select exists(select 1 from work_phases where project_id=?1 and work_unit_id=?2 and phase_key=?3)",
            params![project_id, work_unit_id, key],
            |row| row.get(0),
        )?;
        let storage_key =
            key_is_occupied.then(|| format!("successor-plan-{plan_id}-slice-{slice_id}"));
        let phase = crate::phases::create_phase_in(
            conn,
            project_id,
            crate::NewWorkPhase {
                work_unit_id,
                design_version_id: Some(design_version_id),
                key: storage_key.as_deref().unwrap_or(key),
                title,
                kind: "implementation",
                order: *order,
                reason: Some("applied from the current Decomposition Plan"),
            },
        )?;
        conn.execute(
            "update phase_epochs set phase_key=?1,predecessor_epoch_id=?2 where id=?3",
            params![key, phase_predecessors.get(slice_id), phase.phase_id],
        )?;
        phase_ids.insert(*slice_id, phase.phase_id);
    }

    let mut item_statement = conn.prepare(
        "select id,title,details,outcome,slice_id from decomposition_items where decomposition_plan_id=?1 order by id",
    )?;
    let items = item_statement
        .query_map([plan_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut checklist_order = 0_i64;
    for (item_id, title, details, outcome, slice_id) in items {
        let mut requirement_statement = conn.prepare(
            r#"
            select link.id,requirement.id,requirement.priority
            from decomposition_item_requirements link
            join design_requirements requirement on requirement.id=link.design_requirement_id
            where link.decomposition_item_id=?1 order by requirement.requirement_key
            "#,
        )?;
        let requirements = requirement_statement
            .query_map([item_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let priority = requirements
            .iter()
            .map(|(_, _, priority)| priority.as_str())
            .max_by_key(|priority| match *priority {
                "critical" => 4,
                "high" => 3,
                "medium" => 2,
                _ => 1,
            })
            .context("decomposition item has no requirement")?;
        conn.execute(
            r#"
            insert into tasks(work_unit_id,title,priority,status,source,details,completion_condition)
            values(?1,?2,?3,'open','design',?4,?5)
            "#,
            params![work_unit_id, title, priority, details, outcome],
        )?;
        let task_id = conn.last_insert_rowid();

        let mut boundary_statement = conn.prepare(
            r#"
            select id,coalesce(title,boundary_key),condition_text,boundary_order
            from decomposition_item_checklist_boundaries
            where decomposition_item_id=?1 order by boundary_order
            "#,
        )?;
        let boundaries = boundary_statement
            .query_map([item_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let first_requirement_id = requirements[0].1;
        let mut first_checklist_item_id = None;
        for (boundary_id, boundary_title, condition, _boundary_order) in boundaries {
            checklist_order += 1;
            conn.execute(
                r#"
                insert into checklist_items(project_id,checklist_id,design_requirement_id,task_id,item_order,title,completion_condition,status)
                values(?1,?2,?3,?4,?5,?6,?7,'open')
                "#,
                params![
                    project_id,
                    checklist_id,
                    first_requirement_id,
                    task_id,
                    checklist_order,
                    boundary_title,
                    condition
                ],
            )?;
            let checklist_item_id = conn.last_insert_rowid();
            first_checklist_item_id.get_or_insert(checklist_item_id);
            conn.execute(
                "insert into decomposition_application_boundaries(project_id,decomposition_item_checklist_boundary_id,checklist_item_id) values(?1,?2,?3)",
                params![project_id, boundary_id, checklist_item_id],
            )?;
        }
        let first_checklist_item_id =
            first_checklist_item_id.context("decomposition item has no checklist boundary")?;
        for (item_requirement_id, requirement_id, _) in &requirements {
            conn.execute(
                r#"
                insert into task_derivations(project_id,design_requirement_id,task_id,checklist_item_id,derivation_reason,status,created_at)
                values(?1,?2,?3,?4,'applied from the current Decomposition Plan','active',current_timestamp)
                "#,
                params![project_id, requirement_id, task_id, first_checklist_item_id],
            )?;
            let derivation_id = conn.last_insert_rowid();
            conn.execute(
                "insert into decomposition_application_requirements(project_id,decomposition_item_requirement_id,task_derivation_id) values(?1,?2,?3)",
                params![project_id, item_requirement_id, derivation_id],
            )?;
        }

        let mut gate_statement = conn.prepare(
            r#"
            select item_gate.id,template.id,template.gate_key,template.command,
                   template.expected_result,template_requirement.design_requirement_id
            from decomposition_item_gates item_gate
            join validation_gate_templates template
              on template.project_id=?1 and template.design_version_id=?2
             and template.gate_key=item_gate.gate_key and template.status='active'
            join validation_gate_template_requirements template_requirement
              on template_requirement.validation_gate_template_id=template.id
            join decomposition_item_requirements item_requirement
              on item_requirement.decomposition_item_id=item_gate.decomposition_item_id
             and item_requirement.design_requirement_id=template_requirement.design_requirement_id
            where item_gate.decomposition_item_id=?3
            order by item_gate.id,template_requirement.design_requirement_id
            "#,
        )?;
        let gates = gate_statement
            .query_map(params![project_id, design_version_id, item_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (item_gate_id, template_id, gate_key, command, expected_result, requirement_id) in gates
        {
            conn.execute(
                r#"
                insert into validation_gates(project_id,gate_key,template_id,work_unit_id,task_id,design_requirement_id,command,expected_result,selected_before_edit,status,created_at)
                values(?1,?2,?3,?4,?5,?6,?7,?8,1,'active',current_timestamp)
                "#,
                params![project_id, gate_key, template_id, work_unit_id, task_id, requirement_id, command, expected_result],
            )?;
            let validation_gate_id = conn.last_insert_rowid();
            conn.execute(
                "insert into decomposition_application_gates(project_id,decomposition_item_gate_id,validation_gate_id) values(?1,?2,?3)",
                params![project_id, item_gate_id, validation_gate_id],
            )?;
            let work_scope = work_unit_id.to_string();
            crate::rules::insert_rule_binding(
                conn,
                crate::rules::RuleBindingInput {
                    project_id,
                    rule_source_type: "validation_gate",
                    authority_event_id: None,
                    user_correction_id: None,
                    command_profile_id: None,
                    review_policy_id: None,
                    review_plan_id: None,
                    work_unit_id: Some(work_unit_id),
                    validation_gate_id: Some(validation_gate_id),
                    acceptance_record_id: None,
                    scope_type: "work_unit",
                    scope_key: Some(&work_scope),
                    precedence: 62,
                },
            )?;
        }
        let phase_id = phase_ids[&slice_id];
        crate::phases::publish_task_membership_shadow_in(conn, project_id, phase_id, task_id)?;
        crate::task_identity::materialize_decomposition_item(
            conn,
            project_id,
            plan_id,
            item_id,
            task_id,
            phase_id,
            retained_tasks.get(&item_id).copied(),
        )?;
        conn.execute(
            "insert into decomposition_applications(project_id,decomposition_plan_id,decomposition_item_id,task_id,checklist_id,phase_id,applied_at) values(?1,?2,?3,?4,?5,?6,current_timestamp)",
            params![project_id, plan_id, item_id, task_id, checklist_id, phase_id],
        )?;
    }

    let mut dependency_statement = conn.prepare(
        "select id,predecessor_slice_id,successor_slice_id from decomposition_slice_dependencies where decomposition_plan_id=?1 order by id",
    )?;
    let dependencies = dependency_statement
        .query_map([plan_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (dependency_id, predecessor_slice_id, successor_slice_id) in dependencies {
        let outcome = crate::phases::add_phase_dependency_in(
            conn,
            project_id,
            crate::NewPhaseDependency {
                from_phase_id: phase_ids[&predecessor_slice_id],
                to_phase_id: phase_ids[&successor_slice_id],
                dependency_type: "requires",
                reason: "declared by the current Decomposition Plan",
            },
        )?;
        conn.execute(
            "insert into decomposition_application_dependencies(project_id,decomposition_slice_dependency_id,work_phase_dependency_id) values(?1,?2,?3)",
            params![project_id, dependency_id, outcome.dependency_id],
        )?;
    }
    conn.execute(
        "update decomposition_plans set status='applied',binding_issue=null,applied_at=current_timestamp where id=?1 and status='ready'",
        [plan_id],
    )?;
    Ok(())
}
