use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{
    active_activation, ensure_unscoped_mutation_allowed, open_existing_project, project_id,
};

pub fn add_task(root: &Path, input: NewTask<'_>) -> Result<TaskOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    ensure_no_active_source_correction(&tx, "task add")?;
    let work_unit_id = match input.work_unit_id {
        Some(work_unit_id) => Some(work_unit_id),
        None => active_activation(&tx)?.map(|active| active.work_unit_id),
    };

    tx.execute(
        r#"
        insert into tasks(
            work_unit_id, title, priority, status, source, details, completion_condition
        )
        values (?1, ?2, ?3, 'open', ?4, ?5, ?6)
        "#,
        params![
            work_unit_id,
            input.title,
            input.priority,
            input.source,
            input.details,
            input.completion_condition,
        ],
    )?;
    let task_id = tx.last_insert_rowid();
    if work_unit_id.is_some() {
        crate::task_identity::materialize_manual_task(&tx, project, task_id)?;
    }
    tx.commit()?;

    Ok(TaskOutcome {
        task_id,
        work_unit_id,
    })
}

pub fn add_correction_support_task(
    root: &Path,
    input: CorrectionSupportTask<'_>,
) -> Result<TaskOutcome> {
    let work_unit_id = input
        .task
        .work_unit_id
        .context("correction support task requires --work-unit")?;
    if input
        .task
        .details
        .is_none_or(|value| value.trim().is_empty())
        || input
            .task
            .completion_condition
            .is_none_or(|value| value.trim().is_empty())
    {
        bail!("correction support task requires non-empty details and completion condition");
    }
    let mut db = open_existing_project(root)?;
    let tx = db.transaction()?;
    let project = project_id(&tx)?;
    tx.query_row(
        r#"
        select 1
        from correction_sessions session
        join closures c on c.id=session.closure_id and c.finding_id=session.finding_id
        join findings f on f.id=session.finding_id
        join review_runs source_run on source_run.id=f.review_run_id
        join review_plans source_plan on source_plan.id=source_run.review_plan_id
        join work_phases phase on phase.id=?4
        where session.project_id=?1 and session.closure_id=?2
          and session.status in ('active','completed') and c.status='registered'
          and f.status='open' and f.lifecycle_state in ('open','remediating')
          and f.finding_type='design_task_gap'
          and source_plan.work_unit_id=?3 and phase.work_unit_id=?3
          and phase.design_version_id=source_plan.design_version_id
          and phase.status in ('open','blocked')
        "#,
        params![project, input.closure_id, work_unit_id, input.phase_id],
        |_| Ok(()),
    )
    .optional()?
    .context("active correction does not authorize this support task and phase")?;
    tx.execute(
        r#"
        insert into tasks(
            work_unit_id,title,priority,status,source,details,completion_condition
        ) values(?1,?2,?3,'open',?4,?5,?6)
        "#,
        params![
            work_unit_id,
            input.task.title,
            input.task.priority,
            input.task.source,
            input.task.details,
            input.task.completion_condition
        ],
    )?;
    let task_id = tx.last_insert_rowid();
    crate::task_identity::materialize_manual_task(&tx, project, task_id)?;
    crate::phases::assign_task_to_phase_in(&tx, project, input.phase_id, task_id)?;
    tx.execute(
        r#"
        insert into authority_events(
            project_id,event_type,source,text_or_summary,scope,precedence,status,created_at
        ) values(?1,'user_instruction','task add correction support',?2,?3,100,'active',current_timestamp)
        "#,
        params![
            project,
            format!(
                "support task {task_id} added to phase {} under correction closure {}",
                input.phase_id, input.closure_id
            ),
            format!("work-unit:{work_unit_id}")
        ],
    )?;
    tx.commit()?;
    Ok(TaskOutcome {
        task_id,
        work_unit_id: Some(work_unit_id),
    })
}

pub fn revise_task_completion(
    root: &Path,
    input: TaskCompletionRevision<'_>,
) -> Result<TaskCompletionRevisionOutcome> {
    if input.details.trim().is_empty() || input.completion_condition.trim().is_empty() {
        bail!("task details and completion condition must be non-empty");
    }
    let mut db = open_existing_project(root)?;
    let tx = db.transaction()?;
    let project = project_id(&tx)?;
    let work_unit_id: i64 = tx
        .query_row(
            r#"
            select distinct t.work_unit_id
            from correction_sessions session
            join closures c on c.id=session.closure_id and c.finding_id=session.finding_id
            join findings f on f.id=session.finding_id
            join review_runs source_run on source_run.id=f.review_run_id
            join review_plans source_plan on source_plan.id=source_run.review_plan_id
            join tasks t on t.id=?2
            join task_derivations td on td.task_id=t.id and td.status='active'
            join design_requirements requirement on requirement.id=td.design_requirement_id
            where session.project_id=?1 and session.closure_id=?3
              and session.status in ('active','completed')
              and c.status='registered'
              and f.status='open' and f.lifecycle_state in ('open','remediating')
              and f.finding_type='design_task_gap'
              and t.status in ('open','blocked')
              and t.work_unit_id=source_plan.work_unit_id
              and requirement.design_version_id=source_plan.design_version_id
              and requirement.design_version_id=?4
              and requirement.requirement_key=?5
            "#,
            params![
                project,
                input.task_id,
                input.closure_id,
                input.design_version_id,
                input.requirement_key
            ],
            |row| row.get(0),
        )
        .optional()?
        .context("active correction does not authorize this task completion revision")?;
    tx.execute(
        "update tasks set details=?1,completion_condition=?2 where id=?3",
        params![
            input.details.trim(),
            input.completion_condition.trim(),
            input.task_id
        ],
    )?;
    let checklist_items_updated = tx.execute(
        r#"
        update checklist_items
        set completion_condition=?1
        where task_id=?2 and status in ('open','blocked')
          and exists (
            select 1 from task_derivations td
            where td.checklist_item_id=checklist_items.id
              and td.task_id=?2 and td.status='active'
          )
        "#,
        params![input.completion_condition.trim(), input.task_id],
    )? as i64;
    if checklist_items_updated == 0 {
        bail!("task has no active managed checklist item to revise");
    }
    crate::task_identity::revise_canonical_task(
        &tx,
        project,
        input.task_id,
        input.details.trim(),
        input.completion_condition.trim(),
    )?;
    tx.execute(
        r#"
        insert into authority_events(
            project_id,event_type,source,text_or_summary,scope,precedence,status,created_at
        ) values(?1,'user_instruction','trace derive-task revise-completion',?2,?3,100,'active',current_timestamp)
        "#,
        params![
            project,
            format!(
                "task {} completion revised under correction closure {}",
                input.task_id, input.closure_id
            ),
            format!("work-unit:{work_unit_id}")
        ],
    )?;
    tx.commit()?;
    Ok(TaskCompletionRevisionOutcome {
        task_id: input.task_id,
        checklist_items_updated,
    })
}

pub fn list_tasks(root: &Path, input: TaskListQuery<'_>) -> Result<Vec<TaskRecord>> {
    let conn = open_existing_project(root)?;
    let mut records = Vec::new();

    match (input.status, input.work_unit_id) {
        (Some(status), Some(work_unit_id)) => {
            let mut stmt = conn.prepare(
                r#"
                select id, work_unit_id, title, priority, status, source, closed_by_commit
                from current_tasks
                where status = ?1 and work_unit_id = ?2
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![status, work_unit_id], task_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        (Some(status), None) => {
            let mut stmt = conn.prepare(
                r#"
                select id, work_unit_id, title, priority, status, source, closed_by_commit
                from current_tasks
                where status = ?1
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![status], task_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        (None, Some(work_unit_id)) => {
            let mut stmt = conn.prepare(
                r#"
                select id, work_unit_id, title, priority, status, source, closed_by_commit
                from current_tasks
                where work_unit_id = ?1
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![work_unit_id], task_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        (None, None) => {
            let mut stmt = conn.prepare(
                r#"
                select id, work_unit_id, title, priority, status, source, closed_by_commit
                from current_tasks
                order by id
                "#,
            )?;
            let rows = stmt.query_map([], task_record)?;
            for row in rows {
                records.push(row?);
            }
        }
    }

    Ok(records)
}

pub fn close_task(root: &Path, task_id: i64, commit: Option<&str>) -> Result<TaskCloseOutcome> {
    let mut db = open_existing_project(root)?;
    let conn = db.transaction()?;
    ensure_no_active_source_correction(&conn, "task close")?;
    ensure_design_task_closure_ready(&conn, task_id)?;
    let changed = conn.execute(
        r#"
        update tasks
        set status = 'closed', closed_by_commit = ?1
        where id = ?2 and status != 'closed'
        "#,
        params![commit, task_id],
    )?;
    if changed == 0 {
        bail!("task not found or already closed");
    }
    conn.execute(
        "update validation_gates set status='closed' where task_id=?1 and status='active'",
        params![task_id],
    )?;
    conn.commit()?;

    Ok(TaskCloseOutcome { task_id })
}

pub(crate) fn ensure_design_task_closure_ready(
    conn: &rusqlite::Connection,
    task_id: i64,
) -> Result<()> {
    let active_derivation_count: i64 = conn.query_row(
        "select count(*) from task_derivations where task_id = ?1 and status = 'active'",
        params![task_id],
        |row| row.get(0),
    )?;
    if active_derivation_count == 0 {
        let design_support_contract: bool = conn.query_row(
            r#"
            select exists(
              select 1 from tasks t
              join work_phase_task_memberships membership on membership.task_id=t.id
              join work_phases phase on phase.id=membership.phase_id
              where t.id=?1 and t.source='design'
                and t.status in ('open','blocked')
                and nullif(trim(t.details),'') is not null
                and nullif(trim(t.completion_condition),'') is not null
                and phase.design_version_id is not null
            )
            "#,
            params![task_id],
            |row| row.get(0),
        )?;
        if design_support_contract {
            let evidence_count: i64 = conn.query_row(
                "select count(*) from implementation_evidence where task_id=?1",
                params![task_id],
                |row| row.get(0),
            )?;
            if evidence_count == 0 {
                bail!(
                    "cannot close design support task; task-bound implementation evidence is required"
                );
            }
        }
        return Ok(());
    }

    let missing_checklist_count: i64 = conn.query_row(
        r#"
        select count(*)
        from task_derivations
        where task_id = ?1
          and status = 'active'
          and checklist_item_id is null
        "#,
        params![task_id],
        |row| row.get(0),
    )?;
    if missing_checklist_count > 0 {
        bail!(
            "cannot close design-derived task; {missing_checklist_count} derivations have no checklist item"
        );
    }

    let missing_completion_count: i64 = conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join tasks t on t.id = td.task_id
        left join checklist_items ci on ci.id = td.checklist_item_id
        where td.task_id = ?1
          and td.status = 'active'
          and coalesce(
            nullif(trim(ci.completion_condition), ''),
            nullif(trim(t.completion_condition), '')
          ) is null
        "#,
        params![task_id],
        |row| row.get(0),
    )?;
    if missing_completion_count > 0 {
        bail!(
            "cannot close design-derived task; {missing_completion_count} derivations have no completion condition"
        );
    }

    let missing_gate_count: i64 = conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join design_requirements r on r.id = td.design_requirement_id
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where td.task_id = ?1
          and td.status = 'active'
          and not exists (
            select 1
            from validation_gates vg
            where (
                vg.design_requirement_id = td.design_requirement_id
                or exists (
                    select 1
                    from design_requirements current_r
                    where current_r.id = vg.design_requirement_id
                      and current_r.design_version_id = p.current_design_version_id
                      and current_r.requirement_key = r.requirement_key
                      and current_r.requirement_hash = r.requirement_hash
                      and current_r.status = 'active'
                )
              )
              and vg.task_id = td.task_id
              and vg.selected_before_edit = 1
              and vg.status = 'active'
          )
        "#,
        params![task_id],
        |row| row.get(0),
    )?;
    if missing_gate_count > 0 {
        bail!(
            "cannot close design-derived task; {missing_gate_count} derivations have no selected validation gate"
        );
    }

    let missing_evidence_count: i64 = conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        where td.task_id = ?1
          and td.status = 'active'
          and not exists (
            select 1
            from implementation_evidence e
            where e.task_id = td.task_id
              and e.design_requirement_id = td.design_requirement_id
          )
        "#,
        params![task_id],
        |row| row.get(0),
    )?;
    if missing_evidence_count > 0 {
        bail!(
            "cannot close design-derived task; {missing_evidence_count} derivations have no implementation evidence"
        );
    }

    let missing_coverage_count: i64 = conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join tasks t on t.id = td.task_id
        where td.task_id = ?1
          and td.status = 'active'
          and not exists (
            select 1
            from coverage_items c
            where c.design_requirement_id = td.design_requirement_id
              and (
                c.task_id = td.task_id
                or (c.task_id is null and c.work_unit_id = t.work_unit_id)
              )
              and (
                c.status = 'covered'
                or (
                  c.status = 'accepted_out_of_scope'
                  and exists (
                    select 1
                    from acceptance_records ar
                    where ar.target_type = 'coverage_item'
                      and ar.coverage_item_id = c.id
                      and ar.acceptance_type = 'accepted_out_of_scope'
                      and ar.status = 'approved'
                  )
                )
              )
          )
        "#,
        params![task_id],
        |row| row.get(0),
    )?;
    if missing_coverage_count > 0 {
        bail!(
            "cannot close design-derived task; {missing_coverage_count} derivations have no covered coverage item"
        );
    }

    Ok(())
}

pub fn accept_task_out_of_scope(
    root: &Path,
    task_id: i64,
    reason: &str,
) -> Result<TaskAcceptanceOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    ensure_no_active_source_correction(&tx, "task accept-out-of-scope")?;
    let work_unit_id = open_task_work_unit(&tx, task_id)?;
    let design_derived: bool = tx.query_row(
        "select exists(select 1 from task_derivations where task_id=?1 and status='active')",
        params![task_id],
        |row| row.get(0),
    )?;
    if design_derived {
        bail!(
            "design-derived task acceptance requires a declared task-accept-out-of-scope correction transition with verified baseline proof"
        );
    }

    tx.execute(
        r#"
        insert into authority_events(
            project_id, event_type, source, text_or_summary, scope, precedence,
            status, created_at
        )
        values (?1, 'user_instruction', 'task accept-out-of-scope', ?2, ?3, 100, 'active', current_timestamp)
        "#,
        params![
            project_id,
            format!("accepted task {task_id} out of scope: {reason}"),
            work_unit_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "project".to_string()),
        ],
    )?;
    let authority_event_id = tx.last_insert_rowid();
    let acceptance_record_id = apply_task_acceptance_bundle(
        &tx,
        project_id,
        task_id,
        None,
        work_unit_id,
        reason,
        authority_event_id,
    )?;
    tx.commit()?;

    Ok(TaskAcceptanceOutcome {
        task_id,
        acceptance_record_id,
        authority_event_id,
    })
}

pub(crate) fn accept_task_out_of_scope_in(
    conn: &rusqlite::Connection,
    project_id: i64,
    task_id: i64,
    design_requirement_id: i64,
    reason: &str,
    authority_event_id: i64,
) -> Result<TaskAcceptanceOutcome> {
    let valid_authority: bool = conn.query_row(
        r#"
        select exists(
            select 1 from authority_events
            where id = ?1 and project_id = ?2 and status = 'active'
              and event_type in ('user_instruction', 'policy', 'design_doc')
        )
        "#,
        params![authority_event_id, project_id],
        |row| row.get(0),
    )?;
    if !valid_authority {
        bail!("task acceptance requires active user, policy, or design authority");
    }
    let work_unit_id = open_task_work_unit(conn, task_id)?;
    let design_requirement_id = ensure_verified_baseline_carry_forward_for_requirement(
        conn,
        project_id,
        task_id,
        work_unit_id,
        authority_event_id,
        Some(design_requirement_id),
    )?;
    let acceptance_record_id = apply_task_acceptance_bundle(
        conn,
        project_id,
        task_id,
        Some(design_requirement_id),
        work_unit_id,
        reason,
        authority_event_id,
    )?;
    Ok(TaskAcceptanceOutcome {
        task_id,
        acceptance_record_id,
        authority_event_id,
    })
}

pub(crate) fn accept_recovery_task_out_of_scope_in(
    conn: &rusqlite::Connection,
    project_id: i64,
    task_id: i64,
    design_requirement_id: i64,
    current_design_version_id: i64,
    reason: &str,
    authority_event_id: i64,
) -> Result<TaskAcceptanceOutcome> {
    let work_unit_id = open_task_work_unit(conn, task_id)?;
    let (key, eligible): (String, bool) = conn.query_row(
        r#"
        select current_r.requirement_key,
          exists(
            select 1
            from task_derivations td
            join design_requirements r on r.id=td.design_requirement_id
            join design_versions v on v.id=r.design_version_id
            join design_versions current_v on current_v.id=current_r.design_version_id
            where td.task_id=?1 and r.id=?2
              and v.design_package_id=current_v.design_package_id
              and r.requirement_key=current_r.requirement_key
              and (
                td.status='active'
                or (td.status='stale' and exists(
                  select 1 from acceptance_records ar
                  where ar.target_type='stale_record'
                    and ar.stale_record_type='task_derivation'
                    and ar.stale_record_id=td.id and ar.status='approved'
                ))
                or (td.status='closed' and exists(
                  select 1 from correction_transition_aliases a
                  join correction_transition_applications app on app.id=a.correction_application_id
                  join correction_tokens token on token.id=app.correction_token_id
                  where a.record_type='task' and a.record_id=?1
                    and a.alias='@superseded-task/'||?1
                    and token.operation='design-reconcile'
                ))
              )
          )
        from design_requirements current_r
        where current_r.design_version_id=?3
          and current_r.requirement_key=(select requirement_key from design_requirements where id=?2)
        "#,
        params![task_id, design_requirement_id, current_design_version_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if !eligible {
        bail!("task is not an eligible predecessor or reconciled duplicate");
    }
    let scope: String = conn
        .query_row(
            "select scope from authority_events where id=?1 and project_id=?2 and status='active' and event_type in ('user_instruction','policy','design_doc')",
            params![authority_event_id, project_id],
            |row| row.get(0),
        )
        .optional()?
        .context("task acceptance requires active user, policy, or design authority")?;
    let work_scope = work_unit_id.map(|id| format!("work-unit:{id}"));
    if scope != "project"
        && scope != format!("requirement:{key}")
        && work_scope.as_deref() != Some(scope.as_str())
    {
        bail!("recovery task authority scope does not cover the exact requirement or work unit");
    }
    let acceptance_record_id = apply_task_acceptance_bundle(
        conn,
        project_id,
        task_id,
        Some(design_requirement_id),
        work_unit_id,
        reason,
        authority_event_id,
    )?;
    Ok(TaskAcceptanceOutcome {
        task_id,
        acceptance_record_id,
        authority_event_id,
    })
}

pub(crate) fn ensure_verified_baseline_carry_forward(
    conn: &rusqlite::Connection,
    project_id: i64,
    task_id: i64,
    work_unit_id: Option<i64>,
    authority_event_id: i64,
) -> Result<i64> {
    ensure_verified_baseline_carry_forward_for_requirement(
        conn,
        project_id,
        task_id,
        work_unit_id,
        authority_event_id,
        None,
    )
}

fn ensure_verified_baseline_carry_forward_for_requirement(
    conn: &rusqlite::Connection,
    project_id: i64,
    task_id: i64,
    work_unit_id: Option<i64>,
    authority_event_id: i64,
    expected_requirement_id: Option<i64>,
) -> Result<i64> {
    let active_derivations: i64 = conn.query_row(
        "select count(*) from task_derivations where task_id=?1 and status='active'",
        params![task_id],
        |row| row.get(0),
    )?;
    if active_derivations != 1 {
        bail!("mediated task acceptance requires exactly one active design derivation");
    }
    let (
        requirement_id,
        design_version_id,
        package_id,
        version_number,
        key,
        revision,
        hash,
        surfaces,
    ): (i64, i64, i64, i64, String, i64, String, Option<String>) = conn
        .query_row(
            r#"
            select r.id, r.design_version_id, v.design_package_id, v.version_number,
                   r.requirement_key, r.revision, r.requirement_hash, r.required_surfaces
            from task_derivations td
            join design_requirements r on r.id = td.design_requirement_id
            join design_versions v on v.id = r.design_version_id
            where td.task_id = ?1 and td.status = 'active'
            "#,
            params![task_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                ))
            },
        )
        .optional()?
        .context("mediated task acceptance requires one active design derivation")?;
    if expected_requirement_id.is_some_and(|expected| expected != requirement_id) {
        bail!("task correction alias does not identify the task's sole active requirement");
    }
    let (
        baseline_version_id,
        baseline_requirement_id,
        baseline_revision,
        baseline_hash,
        baseline_surfaces,
    ): (i64, i64, i64, String, Option<String>) = conn
        .query_row(
            r#"
            select v.id, r.id, r.revision, r.requirement_hash, r.required_surfaces
            from design_versions v
            join design_requirements r on r.design_version_id = v.id and r.requirement_key = ?1
            where v.design_package_id = ?2 and v.version_number < ?3
              and v.status in ('approved', 'superseded')
              and v.approved_by_authority_event_id is not null and v.approved_at is not null
            order by v.version_number desc limit 1
            "#,
            params![key, package_id, version_number],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?
        .context("no immutable approved preceding design baseline")?;
    if revision != baseline_revision || hash != baseline_hash || surfaces != baseline_surfaces {
        bail!("requirement revision, normalized hash, or required surfaces changed from baseline");
    }
    let gate_drift: bool = conn.query_row(
        r#"
        select exists(
          select gate_key, gate_hash from validation_gate_templates current_g
          join validation_gate_template_requirements current_map
            on current_map.validation_gate_template_id = current_g.id
          where current_g.design_version_id = ?1 and current_map.design_requirement_id = ?2
          except
          select gate_key, gate_hash from validation_gate_templates baseline_g
          join validation_gate_template_requirements baseline_map
            on baseline_map.validation_gate_template_id = baseline_g.id
          where baseline_g.design_version_id = ?3 and baseline_map.design_requirement_id = ?4
        ) or exists(
          select gate_key, gate_hash from validation_gate_templates baseline_g
          join validation_gate_template_requirements baseline_map
            on baseline_map.validation_gate_template_id = baseline_g.id
          where baseline_g.design_version_id = ?3 and baseline_map.design_requirement_id = ?4
          except
          select gate_key, gate_hash from validation_gate_templates current_g
          join validation_gate_template_requirements current_map
            on current_map.validation_gate_template_id = current_g.id
          where current_g.design_version_id = ?1 and current_map.design_requirement_id = ?2
        )
        "#,
        params![
            design_version_id,
            requirement_id,
            baseline_version_id,
            baseline_requirement_id
        ],
        |row| row.get(0),
    )?;
    if gate_drift {
        bail!("required validation gate set changed from baseline");
    }
    let unverified_baseline_gates: i64 = conn.query_row(
        r#"
        select count(*)
        from validation_gate_templates g
        join validation_gate_template_requirements m on m.validation_gate_template_id = g.id
        where g.design_version_id = ?1 and m.design_requirement_id = ?2
          and not exists (
              select 1 from validation_gates selected
              where selected.template_id = g.id
                and selected.id = (
                    select max(latest_gate.id) from validation_gates latest_gate
                    where latest_gate.template_id = g.id
                )
                and (
                    select result from validation_runs run
                    where run.validation_gate_id = selected.id
                      and not exists(select 1 from validation_link_retirements retirement where retirement.validation_run_id=run.id)
                    order by run.id desc limit 1
                ) = 'pass'
          )
        "#,
        params![baseline_version_id, baseline_requirement_id],
        |row| row.get(0),
    )?;
    if unverified_baseline_gates > 0 {
        bail!("baseline validation gates lack a latest authoritative passing run");
    }
    let scope: String = conn.query_row(
        "select scope from authority_events where id = ?1 and project_id = ?2 and status = 'active'",
        params![authority_event_id, project_id],
        |row| row.get(0),
    )?;
    let work_scope = work_unit_id.map(|id| format!("work-unit:{id}"));
    if scope != "project"
        && scope != format!("requirement:{key}")
        && work_scope.as_deref() != Some(scope.as_str())
    {
        bail!(
            "task carry-forward authority scope does not cover the exact requirement or work unit"
        );
    }
    Ok(requirement_id)
}

fn ensure_no_active_source_correction(conn: &rusqlite::Connection, operation: &str) -> Result<()> {
    ensure_unscoped_mutation_allowed(conn, operation)
}

fn open_task_work_unit(conn: &rusqlite::Connection, task_id: i64) -> Result<Option<i64>> {
    conn.query_row(
        "select work_unit_id from tasks where id = ?1 and status in ('open', 'blocked')",
        params![task_id],
        |row| row.get(0),
    )
    .optional()?
    .context("task not found or not open for acceptance")
}

fn apply_task_acceptance_bundle(
    conn: &rusqlite::Connection,
    project_id: i64,
    task_id: i64,
    design_requirement_id: Option<i64>,
    work_unit_id: Option<i64>,
    reason: &str,
    authority_event_id: i64,
) -> Result<i64> {
    conn.execute(
        r#"
        insert into acceptance_records(
            project_id, target_type, task_id, acceptance_type, reason, scope,
            created_by, status, approved_by_authority_event_id, approved_at,
            created_at, review_impact
        )
        values (
            ?1, 'task', ?2, 'accepted_out_of_scope', ?3, ?4,
            'user', 'approved', ?5, current_timestamp, current_timestamp,
            'task accepted out of scope for current work scope'
        )
        "#,
        params![
            project_id,
            task_id,
            reason,
            work_unit_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "project".to_string()),
            authority_event_id,
        ],
    )?;
    let acceptance_record_id = conn.last_insert_rowid();
    let changed = conn.execute(
        r#"
        update tasks
        set status = 'accepted_out_of_scope',
            details = case
                when details is null or details = '' then ?1
                else details || char(10) || 'accepted_out_of_scope: ' || ?1
            end
        where id = ?2 and status in ('open', 'blocked')
        "#,
        params![reason, task_id],
    )?;
    if changed == 0 {
        bail!("task not found or not open for acceptance");
    }
    conn.execute(
        "update checklist_items set status = 'accepted_out_of_scope' where task_id = ?1 and design_requirement_id = ?2 and status in ('open', 'blocked')",
        params![task_id, design_requirement_id],
    )?;
    conn.execute(
        r#"
        insert into acceptance_records(
            project_id, target_type, checklist_item_id, acceptance_type, reason,
            scope, created_by, status, approved_by_authority_event_id,
            approved_at, created_at, review_impact
        )
        select ?1, 'checklist_item', ci.id, 'accepted_out_of_scope', ?2, ?3,
               'user', 'approved', ?4, current_timestamp, current_timestamp,
               'checklist item carried with task disposition'
        from checklist_items ci
        where ci.task_id = ?5 and ci.design_requirement_id = ?6
          and ci.status = 'accepted_out_of_scope'
          and not exists (
              select 1 from acceptance_records ar
              where ar.target_type = 'checklist_item' and ar.checklist_item_id = ci.id
                and ar.status = 'approved'
          )
        "#,
        params![
            project_id,
            reason,
            work_unit_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "project".to_string()),
            authority_event_id,
            task_id,
            design_requirement_id
        ],
    )?;
    conn.execute(
        "update validation_gates set status = 'closed' where task_id = ?1 and design_requirement_id = ?2 and status in ('active', 'stale')",
        params![task_id, design_requirement_id],
    )?;
    conn.execute(
        r#"
        insert into acceptance_records(
            project_id, target_type, validation_gate_id, acceptance_type, reason,
            scope, created_by, status, approved_by_authority_event_id,
            approved_at, created_at, review_impact
        )
        select ?1, 'validation_gate', vg.id, 'accepted_out_of_scope', ?2, ?3,
               'user', 'approved', ?4, current_timestamp, current_timestamp,
               'validation gate carried with task disposition'
        from validation_gates vg
        where vg.task_id = ?5 and vg.design_requirement_id = ?6
          and vg.status = 'closed'
          and not exists (
              select 1 from acceptance_records ar
              where ar.target_type = 'validation_gate' and ar.validation_gate_id = vg.id
                and ar.status = 'approved'
          )
        "#,
        params![
            project_id,
            reason,
            work_unit_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "project".to_string()),
            authority_event_id,
            task_id,
            design_requirement_id
        ],
    )?;
    conn.execute(
        r#"
        insert into coverage_items(
            project_id, work_unit_id, design_requirement_id, task_id,
            requirement, lifecycle_boundary_evidence, tests_or_gates,
            status, created_at
        )
        select
            ?1, ?2, td.design_requirement_id, ?3,
            'authority-backed unchanged baseline carry-forward',
            'task accepted_out_of_scope; generated trace bundle carried atomically',
            'covered by authority-backed task disposition',
            'accepted_out_of_scope', current_timestamp
        from task_derivations td
        where td.task_id = ?3 and td.design_requirement_id = ?4
          and not exists (
              select 1 from coverage_items c
              where c.task_id = ?3 and c.design_requirement_id = td.design_requirement_id
          )
        "#,
        params![project_id, work_unit_id, task_id, design_requirement_id],
    )?;
    conn.execute(
        "update coverage_items set status = 'accepted_out_of_scope' where task_id = ?1 and design_requirement_id = ?2",
        params![task_id, design_requirement_id],
    )?;
    conn.execute(
        r#"
        insert into acceptance_records(
            project_id, target_type, coverage_item_id, acceptance_type, reason,
            scope, created_by, status, approved_by_authority_event_id,
            approved_at, created_at, review_impact
        )
        select
            ?1, 'coverage_item', c.id, 'accepted_out_of_scope', ?2,
            ?3, 'user', 'approved', ?4, current_timestamp, current_timestamp,
            'coverage carried with authority-backed task acceptance'
        from coverage_items c
        where c.task_id = ?5 and c.design_requirement_id = ?6
          and not exists (
              select 1 from acceptance_records ar
              where ar.target_type = 'coverage_item' and ar.coverage_item_id = c.id
                and ar.status = 'approved'
          )
        "#,
        params![
            project_id,
            reason,
            work_unit_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "project".to_string()),
            authority_event_id,
            task_id,
            design_requirement_id
        ],
    )?;
    conn.execute(
        r#"
        update checklists
        set status = 'closed'
        where status = 'active'
          and exists (
              select 1 from checklist_items ci
              where ci.checklist_id = checklists.id and ci.task_id = ?1
                and ci.design_requirement_id = ?2
          )
          and not exists (
              select 1 from checklist_items ci
              where ci.checklist_id = checklists.id and ci.status in ('open', 'blocked')
          )
        "#,
        params![task_id, design_requirement_id],
    )?;
    Ok(acceptance_record_id)
}

fn task_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskRecord> {
    Ok(TaskRecord {
        id: row.get(0)?,
        work_unit_id: row.get(1)?,
        title: row.get(2)?,
        priority: row.get(3)?,
        status: row.get(4)?,
        source: row.get(5)?,
        closed_by_commit: row.get(6)?,
    })
}

pub struct NewTask<'a> {
    pub title: &'a str,
    pub priority: &'a str,
    pub source: &'a str,
    pub work_unit_id: Option<i64>,
    pub details: Option<&'a str>,
    pub completion_condition: Option<&'a str>,
}

pub struct CorrectionSupportTask<'a> {
    pub task: NewTask<'a>,
    pub closure_id: i64,
    pub phase_id: i64,
}

pub struct TaskCompletionRevision<'a> {
    pub task_id: i64,
    pub closure_id: i64,
    pub design_version_id: i64,
    pub requirement_key: &'a str,
    pub details: &'a str,
    pub completion_condition: &'a str,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskCompletionRevisionOutcome {
    pub task_id: i64,
    pub checklist_items_updated: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskOutcome {
    pub task_id: i64,
    pub work_unit_id: Option<i64>,
}

pub struct TaskListQuery<'a> {
    pub status: Option<&'a str>,
    pub work_unit_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskRecord {
    pub id: i64,
    pub work_unit_id: Option<i64>,
    pub title: String,
    pub priority: String,
    pub status: String,
    pub source: String,
    pub closed_by_commit: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskCloseOutcome {
    pub task_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskAcceptanceOutcome {
    pub task_id: i64,
    pub acceptance_record_id: i64,
    pub authority_event_id: i64,
}
