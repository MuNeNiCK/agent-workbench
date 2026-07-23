use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{open_existing_project, project_id};
use crate::rules::{RuleBindingInput, insert_rule_binding};

use super::{checklists::*, evidence::*, readiness::*, reconciliation::*, *};

pub fn derive_task_from_requirement(
    root: &Path,
    input: NewTaskDerivation<'_>,
) -> Result<TaskDerivationOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    ensure_no_active_source_correction(&tx, "task derivation")?;
    let requirement = tx
        .query_row(
            r#"
            select id, requirement_key
            from design_requirements
            where project_id = ?1
              and design_version_id = ?2
              and requirement_key = ?3
              and status = 'active'
            "#,
            params![project_id, input.design_version_id, input.requirement_key],
            |row| {
                Ok(ResolvedRequirement {
                    id: row.get(0)?,
                    key: row.get(1)?,
                })
            },
        )
        .optional()?
        .context("active design requirement not found")?;
    let task = tx
        .query_row(
            r#"
            select id, work_unit_id, title, completion_condition
            from tasks
            where id = ?1
            "#,
            params![input.task_id],
            |row| {
                Ok(ResolvedTask {
                    id: row.get(0)?,
                    work_unit_id: row.get(1)?,
                    title: row.get(2)?,
                    completion_condition: row.get(3)?,
                })
            },
        )
        .optional()?
        .context("task not found")?;
    let Some(work_unit_id) = task.work_unit_id else {
        bail!("design-derived task must belong to a work unit");
    };
    let checklist_title = input
        .checklist_title
        .unwrap_or("Design implementation checklist");
    let checklist_id = get_or_create_checklist(
        &tx,
        project_id,
        work_unit_id,
        input.design_version_id,
        checklist_title,
    )?;
    let item_order: i64 = tx.query_row(
        "select coalesce(max(item_order), 0) + 1 from checklist_items where checklist_id = ?1",
        params![checklist_id],
        |row| row.get(0),
    )?;
    let item_title = input
        .item_title
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}: {}", requirement.key, task.title));
    let completion_condition = input
        .completion_condition
        .or(task.completion_condition.as_deref());
    tx.execute(
        r#"
        insert into checklist_items(
            project_id, checklist_id, design_requirement_id, task_id, item_order,
            title, completion_condition, status
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'open')
        "#,
        params![
            project_id,
            checklist_id,
            requirement.id,
            task.id,
            item_order,
            item_title,
            completion_condition,
        ],
    )?;
    let checklist_item_id = tx.last_insert_rowid();
    tx.execute(
        r#"
        insert into task_derivations(
            project_id, design_requirement_id, task_id, checklist_item_id,
            derivation_reason, status, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, 'active', current_timestamp)
        "#,
        params![
            project_id,
            requirement.id,
            task.id,
            checklist_item_id,
            input.derivation_reason,
        ],
    )?;
    let task_derivation_id = tx.last_insert_rowid();
    tx.commit()?;

    Ok(TaskDerivationOutcome {
        task_derivation_id,
        checklist_id,
        checklist_item_id,
        design_requirement_id: requirement.id,
        task_id: task.id,
    })
}

pub fn rebind_task_derivation(
    root: &Path,
    input: TaskDerivationRebind<'_>,
) -> Result<TaskDerivationRebindOutcome> {
    if input.reason.trim().is_empty() {
        bail!("task derivation rebind requires --reason");
    }
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let (derivation_id, previous_item_id, work_unit_id): (i64, i64, i64) = tx
        .query_row(
            r#"
            select derivation.id,derivation.checklist_item_id,task.work_unit_id
            from finding_remediation_bindings binding
            join closures closure on closure.id=binding.closure_id
              and closure.finding_id=binding.finding_id and closure.status='registered'
            join findings finding on finding.id=binding.finding_id
              and finding.status='open' and finding.classification='valid'
              and finding.lifecycle_state in ('open','remediating')
            join review_runs source_run on source_run.id=finding.review_run_id
            join review_plans source_plan on source_plan.id=source_run.review_plan_id
            join work_unit_activations activation on activation.id=binding.work_unit_activation_id
              and activation.status='active'
            join tasks task on task.id=?3 and task.work_unit_id=binding.work_unit_id
            join task_derivations derivation on derivation.task_id=task.id
              and derivation.status='active'
            join design_requirements requirement on requirement.id=derivation.design_requirement_id
            where binding.project_id=?1 and binding.closure_id=?2
              and source_plan.work_unit_id=binding.work_unit_id
              and source_plan.design_version_id=?4
              and source_plan.required=1 and source_plan.stage='close-ready'
              and source_plan.review_type in ('implementation_review','design_implementation_diff')
              and source_plan.status not in ('exhausted','needs_user_decision')
              and not exists(
                select 1 from acceptance_records accepted
                where accepted.finding_id=finding.id and accepted.target_type='finding'
                  and accepted.status='approved'
                  and accepted.acceptance_type in (
                    'accepted_out_of_scope','explicit_exception','classified_failure'
                  )
              )
              and (finding.task_id is null or finding.task_id=task.id)
              and (
                finding.design_requirement_id is null
                or finding.design_requirement_id=requirement.id
              )
              and requirement.design_version_id=?4 and requirement.requirement_key=?5
            "#,
            params![
                project_id,
                input.closure_id,
                input.task_id,
                input.design_version_id,
                input.requirement_key
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .context("active remediation does not authorize this task derivation rebind")?;
    tx.query_row(
        r#"
        select 1
        from checklist_items item
        join checklists checklist on checklist.id=item.checklist_id
        where item.id=?1 and item.project_id=?2 and item.task_id=?3
          and checklist.project_id=?2 and checklist.work_unit_id=?4
          and checklist.design_version_id=?5 and checklist.status='active'
          and (
            (item.status in ('open','blocked')
              and exists(select 1 from tasks where id=?3 and status in ('open','blocked')))
            or
            (item.status='closed'
              and exists(select 1 from tasks where id=?3 and status='closed'))
          )
        "#,
        params![
            input.checklist_item_id,
            project_id,
            input.task_id,
            work_unit_id,
            input.design_version_id
        ],
        |_| Ok(()),
    )
    .optional()?
    .context(
        "target checklist item is outside the remediation owner, design, task, or lifecycle",
    )?;
    let audit_prefix = format!(
        "closure {} rebinds task derivation {} from checklist item ",
        input.closure_id, derivation_id
    );
    let audit_suffix = format!(
        " to checklist item {}: {}",
        input.checklist_item_id,
        input.reason.trim()
    );
    let audit_summary = format!("{audit_prefix}{previous_item_id}{audit_suffix}");
    let existing_audit: bool = tx.query_row(
        r#"
        select exists(
          select 1 from authority_events
          where project_id=?1 and event_type='user_instruction'
            and source='trace derivation rebind'
            and substr(text_or_summary,1,length(?2))=?2
            and substr(text_or_summary,length(text_or_summary)-length(?3)+1)=?3
            and scope=?4 and status='active'
        )
        "#,
        params![
            project_id,
            audit_prefix,
            audit_suffix,
            format!("work-unit:{work_unit_id}")
        ],
        |row| row.get(0),
    )?;
    if existing_audit {
        if previous_item_id != input.checklist_item_id {
            bail!("recorded task derivation rebind no longer matches current state");
        }
        tx.commit()?;
        return Ok(TaskDerivationRebindOutcome {
            task_derivation_id: derivation_id,
            previous_checklist_item_id: previous_item_id,
            checklist_item_id: input.checklist_item_id,
            idempotent: true,
        });
    }
    if previous_item_id != input.checklist_item_id {
        tx.execute(
            "update task_derivations set checklist_item_id=?1 where id=?2",
            params![input.checklist_item_id, derivation_id],
        )?;
    }
    tx.execute(
        r#"
        insert into authority_events(
          project_id,event_type,source,text_or_summary,scope,precedence,status,created_at
        ) values(?1,'user_instruction','trace derivation rebind',?2,?3,100,'active',current_timestamp)
        "#,
        params![
            project_id,
            audit_summary,
            format!("work-unit:{work_unit_id}")
        ],
    )?;
    tx.commit()?;
    Ok(TaskDerivationRebindOutcome {
        task_derivation_id: derivation_id,
        previous_checklist_item_id: previous_item_id,
        checklist_item_id: input.checklist_item_id,
        idempotent: previous_item_id == input.checklist_item_id,
    })
}

pub fn decompose_design(
    root: &Path,
    input: DesignDecomposition<'_>,
) -> Result<DesignDecompositionOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    ensure_no_active_source_correction(&tx, "design decompose")?;
    let outcome = decompose_design_in(&tx, project_id, input)?;
    tx.commit()?;
    Ok(outcome)
}

pub(crate) fn decompose_design_in(
    conn: &rusqlite::Connection,
    project_id: i64,
    input: DesignDecomposition<'_>,
) -> Result<DesignDecompositionOutcome> {
    decompose_design_with_checklist_in(conn, project_id, input, None, true)
}

pub(super) fn decompose_design_with_checklist_in(
    conn: &rusqlite::Connection,
    project_id: i64,
    input: DesignDecomposition<'_>,
    canonical_checklist_id: Option<i64>,
    require_empty: bool,
) -> Result<DesignDecompositionOutcome> {
    if require_empty {
        validate_design_decomposition_in(
            conn,
            project_id,
            input.design_version_id,
            input.work_unit_id,
        )?;
    } else {
        validate_design_decomposition_scope_in(
            conn,
            project_id,
            input.design_version_id,
            input.work_unit_id,
        )?;
    }

    let mut stmt = conn.prepare(
        r#"
        select dr.id, dr.requirement_key, dr.requirement_text, dr.priority
        from design_requirements dr
        where dr.project_id = ?1
          and dr.design_version_id = ?2
          and dr.status = 'active'
          and not exists (
              select 1
              from task_derivations td
              where td.design_requirement_id = dr.id
                and td.status = 'active'
          )
        order by dr.requirement_key
        "#,
    )?;
    let rows = stmt.query_map(params![project_id, input.design_version_id], |row| {
        Ok(RequirementForDecomposition {
            id: row.get(0)?,
            key: row.get(1)?,
            text: row.get(2)?,
            priority: row.get(3)?,
        })
    })?;
    let mut requirements = Vec::new();
    for row in rows {
        requirements.push(row?);
    }
    drop(stmt);

    let checklist_title = input
        .checklist_title
        .unwrap_or("Design implementation checklist");
    let checklist_id = match canonical_checklist_id {
        Some(checklist_id) => checklist_id,
        None => get_or_create_checklist(
            conn,
            project_id,
            input.work_unit_id,
            input.design_version_id,
            checklist_title,
        )?,
    };
    let mut created_tasks = 0;
    let mut created_derivations = 0;
    let mut created_validation_gates = 0;
    for requirement in requirements {
        let task_title = format!(
            "Implement {}: {}",
            requirement.key,
            first_line(&requirement.text)
        );
        let completion_condition = format!(
            "Requirement {} is implemented and validated",
            requirement.key
        );
        let task_id = match reusable_unchanged_baseline_task(
            conn,
            project_id,
            input.work_unit_id,
            requirement.id,
        )? {
            Some(task_id) => task_id,
            None => {
                conn.execute(
                    r#"
                    insert into tasks(
                        work_unit_id, title, priority, status, source,
                        details, completion_condition
                    )
                    values (?1, ?2, ?3, 'open', 'design', ?4, ?5)
                    "#,
                    params![
                        input.work_unit_id,
                        task_title,
                        requirement.priority,
                        requirement.text,
                        completion_condition,
                    ],
                )?;
                created_tasks += 1;
                conn.last_insert_rowid()
            }
        };

        let item_order: i64 = conn.query_row(
            "select coalesce(max(item_order), 0) + 1 from checklist_items where checklist_id = ?1",
            params![checklist_id],
            |row| row.get(0),
        )?;
        conn.execute(
            r#"
            insert into checklist_items(
                project_id, checklist_id, design_requirement_id, task_id,
                item_order, title, completion_condition
            )
            values (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                project_id,
                checklist_id,
                requirement.id,
                task_id,
                item_order,
                task_title,
                completion_condition,
            ],
        )?;
        let checklist_item_id = conn.last_insert_rowid();
        conn.execute(
            r#"
            insert into task_derivations(
                project_id, design_requirement_id, task_id, checklist_item_id,
                derivation_reason, status, created_at
            )
            values (?1, ?2, ?3, ?4, ?5, 'active', current_timestamp)
            "#,
            params![
                project_id,
                requirement.id,
                task_id,
                checklist_item_id,
                input.reason,
            ],
        )?;
        created_derivations += 1;
        let gate_templates = validation_gate_templates_for_requirement(
            conn,
            project_id,
            input.design_version_id,
            requirement.id,
        )?;
        for template in gate_templates {
            conn.execute(
                r#"
                insert into validation_gates(
                    project_id, gate_key, template_id, work_unit_id, task_id,
                    design_requirement_id, command, expected_result,
                    selected_before_edit, status, created_at
                )
                values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, 'active', current_timestamp)
                "#,
                params![
                    project_id,
                    template.gate_key,
                    template.id,
                    input.work_unit_id,
                    task_id,
                    requirement.id,
                    template.command,
                    template.expected_result,
                ],
            )?;
            let validation_gate_id = conn.last_insert_rowid();
            let work_scope = input.work_unit_id.to_string();
            insert_rule_binding(
                conn,
                RuleBindingInput {
                    project_id,
                    rule_source_type: "validation_gate",
                    authority_event_id: None,
                    user_correction_id: None,
                    command_profile_id: None,
                    review_policy_id: None,
                    review_plan_id: None,
                    work_unit_id: Some(input.work_unit_id),
                    validation_gate_id: Some(validation_gate_id),
                    acceptance_record_id: None,
                    scope_type: "work_unit",
                    scope_key: Some(&work_scope),
                    precedence: 62,
                },
            )?;
            created_validation_gates += 1;
        }
    }
    Ok(DesignDecompositionOutcome {
        design_version_id: input.design_version_id,
        work_unit_id: input.work_unit_id,
        checklist_id,
        created_tasks,
        created_derivations,
        created_validation_gates,
    })
}

pub(super) fn reusable_unchanged_baseline_task(
    conn: &rusqlite::Connection,
    project_id: i64,
    work_unit_id: i64,
    current_requirement_id: i64,
) -> Result<Option<i64>> {
    let mut stmt = conn.prepare(
        r#"
        select distinct t.id
        from design_requirements current_r
        join design_versions current_v on current_v.id = current_r.design_version_id
        join design_versions baseline_v
          on baseline_v.design_package_id = current_v.design_package_id
         and baseline_v.version_number = (
             select max(candidate.version_number)
             from design_versions candidate
             where candidate.design_package_id = current_v.design_package_id
               and candidate.version_number < current_v.version_number
               and candidate.status in ('approved', 'superseded')
               and candidate.approved_by_authority_event_id is not null
               and candidate.approved_at is not null
         )
        join design_requirements baseline_r
          on baseline_r.design_version_id = baseline_v.id
         and baseline_r.requirement_key = current_r.requirement_key
         and baseline_r.revision = current_r.revision
         and baseline_r.requirement_hash = current_r.requirement_hash
         and baseline_r.required_surfaces is current_r.required_surfaces
        join task_derivations td
          on td.design_requirement_id = baseline_r.id
         and td.status in ('active', 'stale', 'closed')
        join tasks t on t.id = td.task_id
        where current_r.id = ?1 and current_r.project_id = ?2
          and t.work_unit_id = ?3 and t.status in ('open', 'blocked')
        order by t.id
        "#,
    )?;
    let task_ids = stmt
        .query_map(
            params![current_requirement_id, project_id, work_unit_id],
            |row| row.get::<_, i64>(0),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if task_ids.len() > 1 {
        bail!("unchanged baseline decomposition has ambiguous reusable tasks");
    }
    let Some(task_id) = task_ids.into_iter().next() else {
        return Ok(None);
    };
    conn.execute(
        r#"
        update task_derivations
        set status = 'closed'
        where project_id = ?1 and task_id = ?2 and status = 'active'
          and design_requirement_id in (
              select baseline_r.id
              from design_requirements current_r
              join design_versions current_v on current_v.id = current_r.design_version_id
              join design_requirements baseline_r
                on baseline_r.requirement_key = current_r.requirement_key
              join design_versions baseline_v on baseline_v.id = baseline_r.design_version_id
              where current_r.id = ?3
                and baseline_v.design_package_id = current_v.design_package_id
                and baseline_v.version_number < current_v.version_number
          )
        "#,
        params![project_id, task_id, current_requirement_id],
    )?;
    Ok(Some(task_id))
}
