use super::super::*;

pub fn show_decomposition_plan(
    root: &Path,
    query: DecompositionPlanQuery,
) -> Result<DecompositionPlanRecord> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let id = resolve_current_plan_id(
        &conn,
        project_id,
        query.design_version_id,
        query.work_unit_id,
    )?
    .context("current decomposition plan not found")?;
    load_decomposition_plan(&conn, id)
}

pub(in crate::decomposition) fn load_decomposition_plan(
    conn: &Connection,
    id: i64,
) -> Result<DecompositionPlanRecord> {
    let (
        design_version_id,
        work_unit_id,
        key,
        revision,
        status,
        source_path,
        content_identity,
        document_content,
        issue,
        predecessor_id,
    ) = conn.query_row(
        r#"
            select design_version_id,work_unit_id,plan_key,revision,status,source_path,
                   content_identity,document_content,binding_issue,predecessor_id
            from decomposition_plans where id=?1
            "#,
        [id],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<i64>>(9)?,
            ))
        },
    )?;

    let mut item_statement = conn.prepare(
        r#"
        select item.id,item.item_key,item.title,item.outcome,item.observation,
               item.evidence_owner,item.evidence_kind,slice.slice_key
        from decomposition_items item
        left join decomposition_slices slice on slice.id=item.slice_id
        where item.decomposition_plan_id=?1
        order by item.id
        "#,
    )?;
    let item_rows = item_statement
        .query_map([id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut items = Vec::with_capacity(item_rows.len());
    for (item_id, key, title, outcome, observation, evidence_owner, evidence_kind, slice) in
        item_rows
    {
        items.push(DecompositionItemRecord {
            key,
            title,
            outcome,
            observation,
            evidence_owner,
            evidence_kind,
            slice,
            requirements: string_column(
                conn,
                r#"
                select requirement.requirement_key
                from decomposition_item_requirements link
                join design_requirements requirement on requirement.id=link.design_requirement_id
                where link.decomposition_item_id=?1 order by requirement.requirement_key
                "#,
                (item_id,),
            )?,
            checklist_boundaries: string_column(
                conn,
                "select boundary_key from decomposition_item_checklist_boundaries where decomposition_item_id=?1 order by boundary_order",
                (item_id,),
            )?,
            gates: string_column(
                conn,
                "select gate_key from decomposition_item_gates where decomposition_item_id=?1 order by gate_key",
                (item_id,),
            )?,
        });
    }

    let mut slice_statement = conn.prepare(
        "select id,slice_key,title,slice_order from decomposition_slices where decomposition_plan_id=?1 order by slice_order",
    )?;
    let slice_rows = slice_statement
        .query_map([id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut slices = Vec::with_capacity(slice_rows.len());
    for (slice_id, key, title, order) in slice_rows {
        slices.push(DecompositionSliceRecord {
            key,
            title,
            order,
            depends_on: string_column(
                conn,
                r#"
                select predecessor.slice_key
                from decomposition_slice_dependencies dependency
                join decomposition_slices predecessor on predecessor.id=dependency.predecessor_slice_id
                where dependency.decomposition_plan_id=?1 and dependency.successor_slice_id=?2
                order by predecessor.slice_order
                "#,
                (id, slice_id),
            )?,
        });
    }

    let mut gaps = Vec::new();
    if status == "incomplete" {
        for task in string_column(
            conn,
            r#"
            select distinct cast(task.id as text)
            from task_derivations derivation
            join tasks task on task.id=derivation.task_id
            join design_requirements requirement on requirement.id=derivation.design_requirement_id
            where task.work_unit_id=?1 and requirement.design_version_id=?2
              and derivation.status='active'
              and not exists(
                select 1 from decomposition_migration_sources source
                where source.decomposition_plan_id=?3 and source.source_task_id=task.id
                  and source.mapping_state='exact'
              )
            order by task.id
            "#,
            params![work_unit_id, design_version_id, id],
        )? {
            gaps.push(DecompositionGapRecord {
                endpoint: format!("task:{task}"),
                issue: "explicit item mapping required".to_string(),
            });
        }
        for checklist_item in string_column(
            conn,
            r#"
            select distinct cast(item.id as text)
            from checklist_items item
            join checklists checklist on checklist.id=item.checklist_id
            where checklist.work_unit_id=?1 and checklist.design_version_id=?2
              and item.status in ('open','blocked','closed')
              and not exists(
                select 1 from decomposition_migration_sources source
                where source.decomposition_plan_id=?3
                  and source.source_checklist_item_id=item.id
                  and source.mapping_state='exact'
              )
            order by item.id
            "#,
            params![work_unit_id, design_version_id, id],
        )? {
            gaps.push(DecompositionGapRecord {
                endpoint: format!("checklist-item:{checklist_item}"),
                issue: "explicit checklist-boundary mapping required".to_string(),
            });
        }
        for gate in string_column(
            conn,
            r#"
            select distinct cast(gate.id as text)
            from validation_gates gate
            join design_requirements requirement on requirement.id=gate.design_requirement_id
            where coalesce(gate.work_unit_id,(
                    select task.work_unit_id from tasks task where task.id=gate.task_id
                  ))=?1
              and requirement.design_version_id=?2
              and gate.status in ('active','stale','closed')
              and not exists(
                select 1 from decomposition_reconciliation_gates source
                where source.decomposition_plan_id=?3
                  and source.source_validation_gate_id=gate.id
              )
            order by gate.id
            "#,
            params![work_unit_id, design_version_id, id],
        )? {
            gaps.push(DecompositionGapRecord {
                endpoint: format!("validation-gate:{gate}"),
                issue: "explicit item-gate mapping required".to_string(),
            });
        }
        for phase in string_column(
            conn,
            r#"
            select cast(phase.id as text) from work_phases phase
            where phase.work_unit_id=?1 and phase.design_version_id=?2
              and phase.status!='superseded'
              and not exists(
                select 1 from decomposition_migration_sources source
                where source.decomposition_plan_id=?3 and source.source_phase_id=phase.id
                  and source.mapping_state='exact'
              )
            order by phase.phase_order,phase.id
            "#,
            params![work_unit_id, design_version_id, id],
        )? {
            gaps.push(DecompositionGapRecord {
                endpoint: format!("phase:{phase}"),
                issue: "explicit slice mapping required".to_string(),
            });
        }
        for dependency in string_column(
            conn,
            r#"
            select cast(dependency.id as text)
            from work_phase_dependencies dependency
            join work_phases phase on phase.id=dependency.to_phase_id
            where phase.work_unit_id=?1 and phase.design_version_id=?2
              and dependency.status in ('open','satisfied','accepted')
            order by dependency.id
            "#,
            params![work_unit_id, design_version_id],
        )? {
            gaps.push(DecompositionGapRecord {
                endpoint: format!("phase-dependency:{dependency}"),
                issue: "explicit slice-dependency mapping required".to_string(),
            });
        }
    }
    let current_identity = decomposition_current_identity(conn, id)?;
    let mappings = decomposition_mappings(conn, id)?;
    let shared_bindings = decomposition_shared_bindings(conn, id)?;

    Ok(DecompositionPlanRecord {
        id,
        design_version_id,
        work_unit_id,
        key,
        revision,
        current_identity,
        status,
        predecessor_id,
        source_path,
        content_identity,
        document_content,
        issue,
        items,
        slices,
        gaps,
        mappings,
        shared_bindings,
    })
}

fn decomposition_mappings(
    conn: &Connection,
    plan_id: i64,
) -> Result<Vec<DecompositionMappingRecord>> {
    let mut records = Vec::new();
    let mut push_rows = |category: &str, sql: &str| -> Result<()> {
        let rows = conn
            .prepare(sql)?
            .query_map([plan_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (source_id, target, disposition, effect, reason) in rows {
            let qualification = match (disposition.as_str(), effect.as_deref()) {
                ("retained", Some("preserve")) => "preserved_qualified",
                ("retained", Some("open")) => "open",
                ("new", _) => "open",
                ("retired", _) => "historical_only",
                _ => "unqualified",
            }
            .to_string();
            let observed_handle = projection_handle(
                "owned-mapping",
                &[
                    category.to_string(),
                    source_id.to_string(),
                    target.clone().unwrap_or_else(|| "-".to_string()),
                    disposition.clone(),
                    effect.clone().unwrap_or_else(|| "-".to_string()),
                    reason.unwrap_or_else(|| "-".to_string()),
                ],
            );
            records.push(DecompositionMappingRecord {
                category: category.to_string(),
                source_id,
                target,
                disposition,
                effect,
                qualification,
                observed_handle,
            });
        }
        Ok(())
    };
    push_rows(
        "task",
        r#"
        select mapping.source_task_id,
               case when item.item_key is null then null else 'item:'||item.item_key end,
               mapping.disposition,mapping.effect,mapping.reason
        from decomposition_reconciliation_tasks mapping
        left join decomposition_items item on item.id=mapping.successor_item_id
        where mapping.decomposition_plan_id=?1 order by mapping.source_task_id
        "#,
    )?;
    push_rows(
        "checklist",
        r#"
        select mapping.source_checklist_item_id,
               case when boundary.boundary_key is null then null
                    else 'boundary:'||item.item_key||'/'||boundary.boundary_key end,
               mapping.disposition,mapping.effect,mapping.reason
        from decomposition_reconciliation_checklist_items mapping
        left join decomposition_item_checklist_boundaries boundary
          on boundary.id=mapping.successor_boundary_id
        left join decomposition_items item on item.id=boundary.decomposition_item_id
        where mapping.decomposition_plan_id=?1 order by mapping.source_checklist_item_id
        "#,
    )?;
    push_rows(
        "gate",
        r#"
        select mapping.source_validation_gate_id,
               case when gate.gate_key is null then null
                    else 'gate:'||item.item_key||'/'||gate.gate_key||'@'||coalesce(mapping.resolved_boundary_identity,'unresolved') end,
               mapping.disposition,mapping.effect,mapping.reason
        from decomposition_reconciliation_gates mapping
        left join decomposition_item_gates gate on gate.id=mapping.successor_item_gate_id
        left join decomposition_items item on item.id=gate.decomposition_item_id
        where mapping.decomposition_plan_id=?1 order by mapping.source_validation_gate_id
        "#,
    )?;
    push_rows(
        "phase",
        r#"
        select mapping.source_phase_id,
               case when slice.slice_key is null then null else 'slice:'||slice.slice_key end,
               mapping.disposition,mapping.effect,mapping.reason
        from decomposition_reconciliation_phases mapping
        left join decomposition_slices slice on slice.id=mapping.successor_slice_id
        where mapping.decomposition_plan_id=?1 order by mapping.source_phase_id
        "#,
    )?;
    push_rows(
        "dependency",
        r#"
        select mapping.source_dependency_id,
               case when predecessor.slice_key is null or successor.slice_key is null then null
                    else 'dependency:'||predecessor.slice_key||'->'||successor.slice_key end,
               mapping.disposition,mapping.effect,mapping.reason
        from decomposition_reconciliation_dependencies mapping
        left join decomposition_slice_dependencies dependency
          on dependency.id=mapping.successor_dependency_id
        left join decomposition_slices predecessor on predecessor.id=dependency.predecessor_slice_id
        left join decomposition_slices successor on successor.id=dependency.successor_slice_id
        where mapping.decomposition_plan_id=?1 order by mapping.source_dependency_id
        "#,
    )?;
    let retained_targets = records
        .iter()
        .filter(|record| record.disposition == "retained")
        .filter_map(|record| record.target.clone())
        .collect::<BTreeSet<_>>();
    let mut add_new = |category: &str, target: String| {
        if retained_targets.contains(&target) {
            return;
        }
        let observed_handle = projection_handle(
            "owned-mapping",
            &[
                category.to_string(),
                "0".to_string(),
                target.clone(),
                "new".to_string(),
                "open".to_string(),
                "-".to_string(),
            ],
        );
        records.push(DecompositionMappingRecord {
            category: category.to_string(),
            source_id: 0,
            target: Some(target),
            disposition: "new".to_string(),
            effect: Some("open".to_string()),
            qualification: "open".to_string(),
            observed_handle,
        });
    };
    for target in string_column(
        conn,
        "select 'item:'||item_key from decomposition_items where decomposition_plan_id=?1 order by item_key",
        [plan_id],
    )? {
        add_new("task", target);
    }
    for target in string_column(
        conn,
        r#"
        select 'boundary:'||item.item_key||'/'||boundary.boundary_key
        from decomposition_item_checklist_boundaries boundary
        join decomposition_items item on item.id=boundary.decomposition_item_id
        where item.decomposition_plan_id=?1 order by item.item_key,boundary.boundary_order
        "#,
        [plan_id],
    )? {
        add_new("checklist", target);
    }
    for target in string_column(
        conn,
        r#"
        select 'gate:'||item.item_key||'/'||gate.gate_key||'@new'
        from decomposition_item_gates gate
        join decomposition_items item on item.id=gate.decomposition_item_id
        where item.decomposition_plan_id=?1 order by item.item_key,gate.gate_key
        "#,
        [plan_id],
    )? {
        let retained_prefix = target.trim_end_matches("new");
        if !retained_targets
            .iter()
            .any(|candidate| candidate.starts_with(retained_prefix))
        {
            add_new("gate", target);
        }
    }
    for target in string_column(
        conn,
        "select 'slice:'||slice_key from decomposition_slices where decomposition_plan_id=?1 order by slice_order",
        [plan_id],
    )? {
        add_new("phase", target);
    }
    for target in string_column(
        conn,
        r#"
        select 'dependency:'||predecessor.slice_key||'->'||successor.slice_key
        from decomposition_slice_dependencies dependency
        join decomposition_slices predecessor on predecessor.id=dependency.predecessor_slice_id
        join decomposition_slices successor on successor.id=dependency.successor_slice_id
        where dependency.decomposition_plan_id=?1
        order by predecessor.slice_order,successor.slice_order
        "#,
        [plan_id],
    )? {
        add_new("dependency", target);
    }
    records.sort_by(|left, right| {
        (&left.category, left.source_id, &left.target).cmp(&(
            &right.category,
            right.source_id,
            &right.target,
        ))
    });
    Ok(records)
}

pub(in crate::decomposition) fn decomposition_shared_bindings(
    conn: &Connection,
    plan_id: i64,
) -> Result<Vec<DecompositionSharedBindingRecord>> {
    let (project_id, design_version_id, work_unit_id, revision, content_identity): (
        i64,
        i64,
        i64,
        i64,
        String,
    ) = conn.query_row(
        "select project_id,design_version_id,work_unit_id,revision,content_identity from decomposition_plans where id=?1",
        [plan_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
    )?;
    let context_ref = crate::review_context::decomposition_plan_review_context_ref(
        &DecompositionPlanReviewTarget {
            plan_id,
            revision,
            content_identity: content_identity.clone(),
            current_identity: content_identity,
            projection_identity: None,
            design_version_id,
            work_unit_id,
        },
    );
    let mut records = Vec::new();
    let review_rows = conn
        .prepare(
            r#"
            select run.id,run.review_plan_id,run.status,run.clean_run,run.new_findings_count,
                   coalesce(run.result_summary,''),run.review_provenance,
                   coalesce(run.review_provenance_ref,''),
                   coalesce((select decision.value from review_adjudication_decisions decision
                     where decision.review_run_id=run.id
                       and not exists(select 1 from review_adjudication_decisions newer
                                      where newer.predecessor_id=decision.id)
                     order by decision.id desc limit 1),'pending'),
                   coalesce(run.target_ref,'')
            from review_runs run
            join review_plans owner_plan on owner_plan.id=run.review_plan_id
            where run.project_id=?1
              and (
                run.target_ref=?2
                or (
                  owner_plan.project_id=?1
                  and owner_plan.work_unit_id=?3
                  and owner_plan.design_version_id=?4
                  and owner_plan.review_type='design_task_decomposition'
                )
              )
            order by run.id
            "#,
        )?
        .query_map(
            params![project_id, context_ref, work_unit_id, design_version_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                ))
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (
        id,
        review_plan_id,
        status,
        clean,
        findings,
        summary,
        provenance,
        provenance_ref,
        decision,
        target_ref,
    ) in review_rows
    {
        if target_ref != context_ref
            && crate::review_context::decomposition_plan_id_from_review_context_ref(&target_ref)
                .is_some_and(|target_plan_id| target_plan_id != plan_id)
        {
            continue;
        }
        let trusted = matches!(provenance.as_str(), "external_agent" | "human_review")
            && !provenance_ref.is_empty();
        let qualification = if status == "completed"
            && clean == 1
            && findings == 0
            && trusted
            && decision == "accepted"
        {
            "accepted_clean"
        } else if matches!(status.as_str(), "requested" | "running") || decision == "pending" {
            "pending"
        } else {
            "historical_only"
        };
        records.push(DecompositionSharedBindingRecord {
            kind: "review".to_string(),
            id,
            owner: format!("review-plan:{review_plan_id}"),
            disposition: "shared".to_string(),
            qualification: qualification.to_string(),
            observed_handle: projection_handle(
                "shared-review",
                &[
                    id.to_string(),
                    review_plan_id.to_string(),
                    status,
                    clean.to_string(),
                    findings.to_string(),
                    summary,
                    provenance,
                    provenance_ref,
                    decision,
                    target_ref,
                ],
            ),
        });
    }

    let evidence_rows = conn
        .prepare(
            r#"
            with owned_tasks(id) as (
              select task_id from decomposition_applications where decomposition_plan_id=?1
              union
              select source_task_id
              from decomposition_reconciliation_tasks
              where decomposition_plan_id=?1
              union
              select derivation.task_id
              from task_derivations derivation
              join design_requirements requirement on requirement.id=derivation.design_requirement_id
              join decomposition_plans plan on plan.id=?1
              join tasks task on task.id=derivation.task_id
              where requirement.design_version_id=plan.design_version_id
                and task.work_unit_id=plan.work_unit_id
            ), owned_requirements(id) as (
              select declared.design_requirement_id
              from decomposition_application_requirements application
              join decomposition_item_requirements declared
                on declared.id=application.decomposition_item_requirement_id
              join decomposition_items item on item.id=declared.decomposition_item_id
              where item.decomposition_plan_id=?1
              union
              select link.design_requirement_id
              from decomposition_item_requirements link
              join decomposition_items item on item.id=link.decomposition_item_id
              where item.decomposition_plan_id=?1
              union
              select link.design_requirement_id
              from decomposition_plans plan
              join decomposition_items item on item.decomposition_plan_id=plan.predecessor_id
              join decomposition_item_requirements link on link.decomposition_item_id=item.id
              where plan.id=?1 and plan.predecessor_id is not null
            )
            select evidence.id,evidence.task_id,evidence.design_requirement_id,evidence.evidence_type,
                   coalesce(evidence.repository_id,0),coalesce(evidence.git_commit_id,0),
                   coalesce(evidence.git_file_change_id,0),coalesce(evidence.commit_sha,''),
                   coalesce(evidence.file_path,''),coalesce(evidence.line_ref,''),
                   coalesce(evidence.symbol,''),coalesce(evidence.artifact_path,''),coalesce(evidence.note,'')
            from implementation_evidence evidence
            where evidence.project_id=?2
              and (evidence.task_id in (select id from owned_tasks)
                   or evidence.design_requirement_id in (select id from owned_requirements))
            order by evidence.id
            "#,
        )?
        .query_map(params![plan_id, project_id], |row| {
            let mut values = Vec::new();
            for index in 0..13 {
                values.push(match row.get_ref(index)? {
                    rusqlite::types::ValueRef::Null => "-".to_string(),
                    rusqlite::types::ValueRef::Integer(value) => value.to_string(),
                    rusqlite::types::ValueRef::Real(value) => value.to_string(),
                    rusqlite::types::ValueRef::Text(value) => {
                        String::from_utf8_lossy(value).into_owned()
                    }
                    rusqlite::types::ValueRef::Blob(value) => format!("blob:{}", value.len()),
                });
            }
            Ok(values)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for values in evidence_rows {
        let id = values[0].parse::<i64>()?;
        let task = values[1].parse::<i64>().ok().filter(|value| *value != 0);
        let requirement = values[2].parse::<i64>().ok().filter(|value| *value != 0);
        let qualification = if shared_relation_is_current(conn, plan_id, task, requirement)? {
            "current"
        } else {
            "stale"
        };
        records.push(DecompositionSharedBindingRecord {
            kind: "evidence".to_string(),
            id,
            owner: shared_owner(task, requirement),
            disposition: "shared".to_string(),
            qualification: qualification.to_string(),
            observed_handle: projection_handle("shared-evidence", &values),
        });
    }

    let coverage_rows = conn
        .prepare(
            r#"
            with owned_tasks(id) as (
              select task_id from decomposition_applications where decomposition_plan_id=?1
              union
              select source_task_id
              from decomposition_reconciliation_tasks
              where decomposition_plan_id=?1
              union
              select derivation.task_id
              from task_derivations derivation
              join design_requirements requirement on requirement.id=derivation.design_requirement_id
              join decomposition_plans plan on plan.id=?1
              join tasks task on task.id=derivation.task_id
              where requirement.design_version_id=plan.design_version_id
                and task.work_unit_id=plan.work_unit_id
            ), owned_requirements(id) as (
              select link.design_requirement_id
              from decomposition_item_requirements link
              join decomposition_items item on item.id=link.decomposition_item_id
              where item.decomposition_plan_id=?1
              union
              select link.design_requirement_id
              from decomposition_plans plan
              join decomposition_items item on item.decomposition_plan_id=plan.predecessor_id
              join decomposition_item_requirements link on link.decomposition_item_id=item.id
              where plan.id=?1 and plan.predecessor_id is not null
            )
            select coverage.id,coverage.task_id,coverage.design_requirement_id,coverage.status,
                   coalesce(coverage.review_scope_id,0),coalesce(coverage.work_unit_id,0),
                   coverage.requirement,coalesce(coverage.runtime_boundary_evidence,''),
                   coalesce(coverage.ux_boundary_evidence,''),coalesce(coverage.lifecycle_boundary_evidence,''),
                   coalesce(coverage.tests_or_gates,''),coalesce(coverage.missing_or_unverified,'')
            from coverage_items coverage
            where coverage.project_id=?2
              and (coverage.task_id in (select id from owned_tasks)
                   or coverage.design_requirement_id in (select id from owned_requirements))
            order by coverage.id
            "#,
        )?
        .query_map(params![plan_id, project_id], |row| {
            let mut values = Vec::new();
            for index in 0..12 {
                values.push(match row.get_ref(index)? {
                    rusqlite::types::ValueRef::Null => "-".to_string(),
                    rusqlite::types::ValueRef::Integer(value) => value.to_string(),
                    rusqlite::types::ValueRef::Real(value) => value.to_string(),
                    rusqlite::types::ValueRef::Text(value) => {
                        String::from_utf8_lossy(value).into_owned()
                    }
                    rusqlite::types::ValueRef::Blob(value) => format!("blob:{}", value.len()),
                });
            }
            Ok(values)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for values in coverage_rows {
        let id = values[0].parse::<i64>()?;
        let task = values[1].parse::<i64>().ok().filter(|value| *value != 0);
        let requirement = values[2].parse::<i64>().ok().filter(|value| *value != 0);
        let relation_current = shared_relation_is_current(conn, plan_id, task, requirement)?;
        let qualification = match (values[3].as_str(), relation_current) {
            ("covered", true) => "current",
            ("covered", false) => "recompute_required",
            ("stale", _) => "stale",
            _ => "needs_evidence",
        };
        records.push(DecompositionSharedBindingRecord {
            kind: "coverage".to_string(),
            id,
            owner: shared_owner(task, requirement),
            disposition: "shared".to_string(),
            qualification: qualification.to_string(),
            observed_handle: projection_handle("shared-coverage", &values),
        });
    }
    records.sort_by(|left, right| (&left.kind, left.id).cmp(&(&right.kind, right.id)));
    Ok(records)
}

fn shared_owner(task: Option<i64>, requirement: Option<i64>) -> String {
    match (task, requirement) {
        (Some(task), Some(requirement)) => format!("task:{task}/requirement:{requirement}"),
        (Some(task), None) => format!("task:{task}"),
        (None, Some(requirement)) => format!("requirement:{requirement}"),
        (None, None) => "unbound".to_string(),
    }
}

fn shared_relation_is_current(
    conn: &Connection,
    plan_id: i64,
    task_id: Option<i64>,
    requirement_id: Option<i64>,
) -> Result<bool> {
    if let Some(task_id) = task_id {
        let current_task_relation: bool = conn.query_row(
            r#"
            select exists(
              select 1
              from decomposition_applications application
              where application.decomposition_plan_id=?1 and application.task_id=?2
                and (
                  ?3 is null or exists(
                    select 1
                    from decomposition_item_requirements requirement
                    where requirement.decomposition_item_id=application.decomposition_item_id
                      and requirement.design_requirement_id=?3
                  )
                )
            )
            "#,
            params![plan_id, task_id, requirement_id],
            |row| row.get(0),
        )?;
        if current_task_relation {
            return Ok(true);
        }
        let preserved_source: bool = conn.query_row(
            r#"
            select exists(
              select 1
              from decomposition_reconciliation_tasks mapping
              join decomposition_items item on item.id=mapping.successor_item_id
              where mapping.decomposition_plan_id=?1 and mapping.source_task_id=?2
                and mapping.disposition='retained' and mapping.effect='preserve'
                and (
                  ?3 is null or exists(
                    select 1
                    from design_requirements source
                    join decomposition_item_requirements target_link
                      on target_link.decomposition_item_id=item.id
                    join design_requirements target
                      on target.id=target_link.design_requirement_id
                    where source.id=?3 and source.requirement_key=target.requirement_key
                      and source.requirement_hash=target.requirement_hash
                  )
                )
            )
            "#,
            params![plan_id, task_id, requirement_id],
            |row| row.get(0),
        )?;
        return Ok(preserved_source);
    }
    let Some(requirement_id) = requirement_id else {
        return Ok(false);
    };
    conn.query_row(
        r#"
        select exists(
          select 1
          from design_requirements source
          cross join decomposition_items item
          join decomposition_item_requirements target_link
            on target_link.decomposition_item_id=item.id
          join design_requirements target on target.id=target_link.design_requirement_id
          where item.decomposition_plan_id=?1 and source.id=?2
            and source.requirement_key=target.requirement_key
            and source.requirement_hash=target.requirement_hash
        )
        "#,
        params![plan_id, requirement_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(in crate::decomposition) fn reconciliation_preserves_shared_binding(
    conn: &Connection,
    binding: &DecompositionSharedBindingRecord,
    reconciliation: &PlanReconciliation,
    plan: &PlanDocument,
    design_version_id: i64,
) -> Result<bool> {
    let (task_id, requirement_id): (Option<i64>, Option<i64>) = match binding.kind.as_str() {
        "evidence" => conn.query_row(
            "select task_id,design_requirement_id from implementation_evidence where id=?1",
            [binding.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?,
        "coverage" => conn.query_row(
            "select task_id,design_requirement_id from coverage_items where id=?1",
            [binding.id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?,
        _ => return Ok(false),
    };
    let target_item = match task_id {
        Some(task_id) => {
            let Some(mapping) = reconciliation.tasks.iter().find(|mapping| {
                mapping.source == task_id
                    && mapping.disposition == "retained"
                    && normalized_effect(mapping.effect) == ReconciliationEffect::Preserve
            }) else {
                return Ok(false);
            };
            mapping.item.as_deref()
        }
        None => None,
    };
    if let Some(requirement_id) = requirement_id {
        let (key, source_hash): (String, String) = conn.query_row(
            "select requirement_key,requirement_hash from design_requirements where id=?1",
            [requirement_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let target_hash = conn
            .query_row(
                "select requirement_hash from design_requirements where design_version_id=?1 and requirement_key=?2 and status='active'",
                params![design_version_id, key],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if target_hash.as_deref() != Some(source_hash.as_str())
            || !plan.items.iter().any(|item| {
                target_item.is_none_or(|target| item.key == target)
                    && item
                        .requirements
                        .iter()
                        .any(|requirement| requirement == &key)
            })
        {
            return Ok(false);
        }
    }
    if binding.kind == "coverage" {
        let source_gates = conn
            .prepare(
                r#"
                select gate.id
                from validation_gates gate
                where gate.status!='stale'
                  and (?1 is null or gate.task_id=?1)
                  and (?2 is null or gate.design_requirement_id=?2)
                  and (?1 is not null or ?2 is not null)
                order by gate.id
                "#,
            )?
            .query_map(params![task_id, requirement_id], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if source_gates.iter().any(|source| {
            !reconciliation.gates.iter().any(|mapping| {
                mapping.source == *source
                    && mapping.disposition == "retained"
                    && normalized_effect(mapping.effect) == ReconciliationEffect::Preserve
            })
        }) {
            return Ok(false);
        }
    }
    Ok(task_id.is_some() || requirement_id.is_some())
}

pub(in crate::decomposition) fn projection_handle(domain: &str, values: &[String]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/decomposition-projection/v1\0");
    hasher.update(domain.as_bytes());
    hasher.update(b"\0");
    for value in values {
        hasher.update((value.len() as u64).to_be_bytes());
        hasher.update(value.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn decomposition_current_identity(conn: &Connection, plan_id: i64) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/decomposition-current/v1\0");
    for sql in [
        "select id,project_id,coalesce(work_unit_id,''),design_version_id,plan_key,revision,source_identity,content_identity,status,coalesce(predecessor_id,'') from decomposition_plans where id=?1",
        "select id,item_key,title,details,outcome,observation,evidence_owner,evidence_kind,coalesce(slice_id,''),status from decomposition_items where decomposition_plan_id=?1 order by id",
        "select id,slice_key,title,slice_order from decomposition_slices where decomposition_plan_id=?1 order by slice_order,id",
        "select id,predecessor_slice_id,successor_slice_id from decomposition_slice_dependencies where decomposition_plan_id=?1 order by id",
        r#"select task.id,task.status,coalesce(task.details,''),coalesce(task.completion_condition,'')
           from tasks task
           where task.id in (
             select application.task_id from decomposition_applications application
             where application.decomposition_plan_id=?1
             union
             select distinct derivation.task_id
             from task_derivations derivation
             join design_requirements requirement on requirement.id=derivation.design_requirement_id
             join decomposition_plans plan on plan.design_version_id=requirement.design_version_id
             join tasks derived_task on derived_task.id=derivation.task_id
             where plan.id=?1 and derived_task.work_unit_id=plan.work_unit_id
           ) order by task.id"#,
        "select item.id,item.status,coalesce(item.completion_condition,'') from checklist_items item join checklists checklist on checklist.id=item.checklist_id join decomposition_plans plan on plan.work_unit_id=checklist.work_unit_id and plan.design_version_id=checklist.design_version_id where plan.id=?1 order by item.id",
        r#"select gate.id,gate.status,gate.gate_key,coalesce(gate.task_id,''),coalesce(gate.design_requirement_id,'')
           from validation_gates gate
           where gate.id in (
             select application.validation_gate_id
             from decomposition_application_gates application
             join decomposition_item_gates item_gate
               on item_gate.id=application.decomposition_item_gate_id
             join decomposition_items item on item.id=item_gate.decomposition_item_id
             where item.decomposition_plan_id=?1
             union
             select current.id
             from validation_gates current
             join design_requirements requirement on requirement.id=current.design_requirement_id
             left join tasks task on task.id=current.task_id
             join decomposition_plans plan on plan.design_version_id=requirement.design_version_id
             where plan.id=?1 and coalesce(current.work_unit_id,task.work_unit_id)=plan.work_unit_id
           ) order by gate.id"#,
        "select phase.id,phase.status,phase.phase_key,phase.phase_order from work_phases phase join decomposition_plans plan on plan.work_unit_id=phase.work_unit_id and plan.design_version_id=phase.design_version_id where plan.id=?1 order by phase.id",
        "select dependency.id,dependency.status,dependency.from_phase_id,dependency.to_phase_id,coalesce(dependency.evidence_ref,'') from work_phase_dependencies dependency join work_phases phase on phase.id=dependency.to_phase_id join decomposition_plans plan on plan.work_unit_id=phase.work_unit_id and plan.design_version_id=phase.design_version_id where plan.id=?1 order by dependency.id",
    ] {
        hash_query_rows(conn, sql, plan_id, &mut hasher)?;
    }
    for binding in decomposition_shared_bindings(conn, plan_id)? {
        hasher.update(binding.kind.as_bytes());
        hasher.update(binding.id.to_be_bytes());
        hasher.update(binding.owner.as_bytes());
        hasher.update(binding.disposition.as_bytes());
        hasher.update(binding.qualification.as_bytes());
        hasher.update(binding.observed_handle.as_bytes());
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub(in crate::decomposition) fn validate_reconciliation_scope(
    conn: &Connection,
    project_id: i64,
    next_design_version_id: i64,
    work_unit_id: i64,
    plan: &PlanDocument,
) -> Result<()> {
    let reconciliation = plan
        .reconciliation
        .as_ref()
        .context("Decomposition Plan reconciliation metadata is required")?;
    let (predecessor_design, predecessor_work, predecessor_status): (i64, i64, String) = conn
        .query_row(
            r#"
            select predecessor.design_version_id,predecessor.work_unit_id,predecessor.status
            from decomposition_plans predecessor
            join design_versions predecessor_design on predecessor_design.id=predecessor.design_version_id
            join design_versions next_design_version on next_design_version.id=?3
            where predecessor.id=?1 and predecessor.project_id=?2
              and predecessor_design.design_package_id=next_design_version.design_package_id
            "#,
            params![
                reconciliation.predecessor,
                project_id,
                next_design_version_id
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .context("reconciliation predecessor is outside the selected Design Package")?;
    if predecessor_work != work_unit_id {
        bail!("reconciliation predecessor is outside the selected work owner");
    }
    if !matches!(predecessor_status.as_str(), "applied" | "incomplete") {
        bail!("only the current applied or incomplete predecessor can be reconciled");
    }
    if decomposition_current_identity(conn, reconciliation.predecessor)?
        != reconciliation.expected_current
    {
        bail!("reconciliation predecessor changed; observe it again before retrying");
    }

    let tasks = id_column(
        conn,
        r#"
        select task_id from decomposition_applications where decomposition_plan_id=?1
        union
        select distinct task.id
        from tasks task
        join task_derivations derivation on derivation.task_id=task.id
        join design_requirements requirement on requirement.id=derivation.design_requirement_id
        where task.work_unit_id=?2 and requirement.design_version_id=?3
        order by 1
        "#,
        params![reconciliation.predecessor, work_unit_id, predecessor_design],
    )?;
    let checklist = id_column(
        conn,
        r#"
        select application.checklist_item_id
        from decomposition_application_boundaries application
        join decomposition_item_checklist_boundaries boundary
          on boundary.id=application.decomposition_item_checklist_boundary_id
        join decomposition_items item on item.id=boundary.decomposition_item_id
        where item.decomposition_plan_id=?1
        union
        select checklist_item.id
        from checklist_items checklist_item
        join checklists checklist on checklist.id=checklist_item.checklist_id
        where checklist.work_unit_id=?2 and checklist.design_version_id=?3
        order by 1
        "#,
        params![reconciliation.predecessor, work_unit_id, predecessor_design],
    )?;
    let gates = id_column(
        conn,
        r#"
        select application.validation_gate_id
        from decomposition_application_gates application
        join decomposition_item_gates item_gate on item_gate.id=application.decomposition_item_gate_id
        join decomposition_items item on item.id=item_gate.decomposition_item_id
        where item.decomposition_plan_id=?1
        union
        select gate.id
        from validation_gates gate
        join design_requirements requirement on requirement.id=gate.design_requirement_id
        left join tasks task on task.id=gate.task_id
        where coalesce(gate.work_unit_id,task.work_unit_id)=?2
          and requirement.design_version_id=?3
        order by 1
        "#,
        params![reconciliation.predecessor, work_unit_id, predecessor_design],
    )?;
    let phases = id_column(
        conn,
        r#"
        select phase_id from decomposition_applications where decomposition_plan_id=?1
        union
        select id from work_phases where work_unit_id=?2 and design_version_id=?3
        order by 1
        "#,
        params![reconciliation.predecessor, work_unit_id, predecessor_design],
    )?;
    let dependencies = id_column(
        conn,
        r#"
        select application.work_phase_dependency_id
        from decomposition_application_dependencies application
        join decomposition_slice_dependencies dependency
          on dependency.id=application.decomposition_slice_dependency_id
        where dependency.decomposition_plan_id=?1
        union
        select dependency.id
        from work_phase_dependencies dependency
        join work_phases predecessor on predecessor.id=dependency.from_phase_id
        join work_phases successor on successor.id=dependency.to_phase_id
        where predecessor.work_unit_id=?2 and successor.work_unit_id=?2
          and predecessor.design_version_id=?3 and successor.design_version_id=?3
        order by 1
        "#,
        params![reconciliation.predecessor, work_unit_id, predecessor_design],
    )?;

    require_exact_source_domain(
        "task",
        &tasks,
        reconciliation.tasks.iter().map(|mapping| mapping.source),
    )?;
    require_exact_source_domain(
        "checklist item",
        &checklist,
        reconciliation
            .checklist
            .iter()
            .map(|mapping| mapping.source),
    )?;
    require_exact_source_domain(
        "validation gate",
        &gates,
        reconciliation.gates.iter().map(|mapping| mapping.source),
    )?;
    require_exact_source_domain(
        "phase",
        &phases,
        reconciliation.phases.iter().map(|mapping| mapping.source),
    )?;
    require_exact_source_domain(
        "phase dependency",
        &dependencies,
        reconciliation
            .dependencies
            .iter()
            .map(|mapping| mapping.source),
    )?;

    validate_reconciliation_relationships(conn, reconciliation, plan, &tasks, &phases)?;
    Ok(())
}

fn require_exact_source_domain(
    label: &str,
    actual: &BTreeSet<i64>,
    declared: impl Iterator<Item = i64>,
) -> Result<()> {
    let declared = declared.collect::<BTreeSet<_>>();
    if actual != &declared {
        bail!("reconciliation {label} mappings must cover the exact predecessor domain");
    }
    Ok(())
}

fn validate_reconciliation_relationships(
    conn: &Connection,
    reconciliation: &PlanReconciliation,
    plan: &PlanDocument,
    tasks: &BTreeSet<i64>,
    phases: &BTreeSet<i64>,
) -> Result<()> {
    let task_targets = reconciliation
        .tasks
        .iter()
        .map(|mapping| (mapping.source, mapping.item.as_deref()))
        .collect::<BTreeMap<_, _>>();
    let checklist_targets = reconciliation
        .checklist
        .iter()
        .map(|mapping| (mapping.source, mapping.item.as_deref()))
        .collect::<BTreeMap<_, _>>();
    let gate_targets = reconciliation
        .gates
        .iter()
        .map(|mapping| (mapping.source, mapping.item.as_deref()))
        .collect::<BTreeMap<_, _>>();
    let phase_targets = reconciliation
        .phases
        .iter()
        .map(|mapping| (mapping.source, mapping.slice.as_deref()))
        .collect::<BTreeMap<_, _>>();
    let item_slices = plan
        .items
        .iter()
        .map(|item| (item.key.as_str(), item.slice.as_str()))
        .collect::<BTreeMap<_, _>>();

    let mut checklist_statement =
        conn.prepare("select id,task_id,status from checklist_items where id=?1")?;
    for mapping in &reconciliation.checklist {
        let (source, source_task, state): (i64, i64, String) = checklist_statement
            .query_row([mapping.source], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
        let task_target = *task_targets
            .get(&source_task)
            .context("reconciliation checklist references a foreign task")?;
        if let (Some(task_item), Some(checklist_item)) = (task_target, checklist_targets[&source]) {
            if task_item != checklist_item {
                bail!("retained task and checklist mappings must target the same item");
            }
        } else if task_target.is_some() != checklist_targets[&source].is_some()
            && !matches!(state.as_str(), "closed" | "accepted_out_of_scope")
        {
            bail!("a nonterminal task-checklist relation cannot be retired on only one endpoint");
        }
    }

    let mut gate_statement =
        conn.prepare("select id,task_id,status from validation_gates where id=?1")?;
    for mapping in &reconciliation.gates {
        let (source, source_task, state): (i64, Option<i64>, String) = gate_statement
            .query_row([mapping.source], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?;
        let Some(source_task) = source_task else {
            continue;
        };
        let task_target = *task_targets
            .get(&source_task)
            .context("reconciliation gate references a foreign task")?;
        if let (Some(task_item), Some(gate_item)) = (task_target, gate_targets[&source]) {
            if task_item != gate_item {
                bail!("retained task and validation-gate mappings must target the same item");
            }
        } else if task_target.is_some() != gate_targets[&source].is_some() && state == "active" {
            bail!("an active task-gate relation cannot be retired on only one endpoint");
        }
    }

    let mut memberships =
        conn.prepare("select task_id,phase_id from work_phase_task_memberships where task_id=?1")?;
    for task in tasks {
        let rows = memberships
            .query_map([task], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (task, phase) in rows {
            if !phases.contains(&phase) {
                bail!("reconciliation task membership references a foreign phase");
            }
            let task_target = *task_targets
                .get(&task)
                .context("reconciliation membership references a foreign task")?;
            let phase_target = *phase_targets
                .get(&phase)
                .context("reconciliation membership references a foreign phase")?;
            if let (Some(item), Some(slice)) = (task_target, phase_target)
                && item_slices
                    .get(item)
                    .context("reconciliation membership references an unknown item")?
                    != &slice
            {
                bail!("retained task and phase mappings must preserve slice membership");
            }
        }
    }

    let mut dependency_statement =
        conn.prepare("select from_phase_id,to_phase_id from work_phase_dependencies where id=?1")?;
    for mapping in &reconciliation.dependencies {
        if mapping.disposition != "retained" {
            continue;
        }
        let (from, to): (i64, i64) = dependency_statement
            .query_row([mapping.source], |row| Ok((row.get(0)?, row.get(1)?)))?;
        if phase_targets
            .get(&from)
            .context("reconciliation dependency references a foreign predecessor phase")?
            != &mapping.from.as_deref()
            || phase_targets
                .get(&to)
                .context("reconciliation dependency references a foreign successor phase")?
                != &mapping.to.as_deref()
        {
            bail!("retained dependency endpoints must match retained phase mappings");
        }
    }
    Ok(())
}
