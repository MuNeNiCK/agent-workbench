use anyhow::{Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::ensure_unscoped_mutation_allowed;

use super::*;

pub(super) fn ensure_no_active_source_correction(
    conn: &rusqlite::Connection,
    operation: &str,
) -> Result<()> {
    ensure_unscoped_mutation_allowed(conn, operation)
}

pub(super) fn build_rescope_report(
    conn: &rusqlite::Connection,
    project_id: i64,
    phase: &StoredPhase,
    target_work_unit_id: i64,
    shared_record_policy: &str,
) -> Result<PhaseRescopeOutcome> {
    let mut blockers = Vec::new();
    let warnings = Vec::new();
    let task_ids = phase_task_ids(conn, phase.id)?;
    if task_ids.is_empty() {
        blockers.push(PhaseRescopeBlocker::new(
            "missing_phase_membership",
            format!("phase {} has no assigned tasks", phase.id),
            format!("agent-workbench phase assign {} --task <task-id>", phase.id),
        ));
    }
    let closed_task_count = count_phase_tasks_not_open(conn, phase.id)?;
    if closed_task_count > 0 {
        blockers.push(PhaseRescopeBlocker::new(
            "closed_records_require_authority",
            format!("{closed_task_count} phase tasks are already closed or accepted"),
            format!(
                "agent-workbench phase trace decide --phase {} --record task:<id> --decision carry|accept --reason \"<reason>\" --authority <authority-id>",
                phase.id
            ),
        ));
    }
    let design_version_count = count_phase_design_versions(conn, phase.id)?;
    if design_version_count > 1 {
        blockers.push(PhaseRescopeBlocker::new(
            "mixed_design_versions",
            format!("phase has {design_version_count} design versions"),
            "split by design version or record explicit trace decisions".to_string(),
        ));
    }
    let active_target_activation_count: i64 = conn.query_row(
        "select count(*) from work_unit_activations where work_unit_id = ?1 and status in ('active', 'suspended')",
        params![target_work_unit_id],
        |row| row.get(0),
    )?;
    if target_work_unit_id != phase.work_unit_id && active_target_activation_count > 0 {
        blockers.push(PhaseRescopeBlocker::new(
            "active_activation_conflict",
            format!("target work unit {target_work_unit_id} has an active or suspended activation"),
            "complete, suspend, or resume/close the conflicting activation first".to_string(),
        ));
    }
    let dependency_count = count_open_cross_phase_dependencies(conn, phase.id)?;
    if dependency_count > 0 {
        blockers.push(PhaseRescopeBlocker::new(
            "unresolved_cross_phase_dependencies",
            format!("{dependency_count} open cross-phase dependencies touch this phase"),
            "agent-workbench phase dependency satisfy <dependency-id> --reason \"<reason>\" --evidence <ref>".to_string(),
        ));
    }
    if shared_record_policy == "require-decisions" {
        for shared in shared_records_missing_decisions(conn, project_id, phase.id)? {
            blockers.push(PhaseRescopeBlocker::new(
                "shared_trace_decision_required",
                format!("{}:{} requires split/carry/accept decision", shared.record_type, shared.record_id),
                format!(
                    "agent-workbench phase trace decide --phase {} --record {}:{} --decision split|carry|accept --reason \"<reason>\" --authority <authority-id>",
                    phase.id, shared.record_type, shared.record_id
                ),
            ));
        }
    }
    let inventory = inventory_lines(conn, project_id, phase.id)?;
    Ok(PhaseRescopeOutcome {
        phase_id: phase.id,
        source_work_unit_id: phase.work_unit_id,
        target_work_unit_id: Some(target_work_unit_id),
        result: if blockers.is_empty() {
            "pass"
        } else {
            "blocked"
        }
        .to_string(),
        inventory,
        blockers,
        warnings,
    })
}

pub(super) fn move_phase_trace_bundle(
    conn: &rusqlite::Connection,
    project_id: i64,
    phase: &StoredPhase,
    target_work_unit_id: i64,
) -> Result<()> {
    let task_ids = phase_task_ids(conn, phase.id)?;
    for task_id in task_ids {
        conn.execute(
            "update tasks set work_unit_id = ?1 where id = ?2",
            params![target_work_unit_id, task_id],
        )?;
        conn.execute(
            "update validation_gates set work_unit_id = ?1 where task_id = ?2 and project_id = ?3",
            params![target_work_unit_id, task_id, project_id],
        )?;
        conn.execute(
            "update coverage_items set work_unit_id = ?1 where task_id = ?2 and project_id = ?3",
            params![target_work_unit_id, task_id, project_id],
        )?;
    }
    move_phase_checklist_items(conn, project_id, phase.id, target_work_unit_id)?;
    split_shared_trace_records(conn, project_id, phase.id, target_work_unit_id)?;
    Ok(())
}

pub(super) fn split_shared_trace_records(
    conn: &rusqlite::Connection,
    project_id: i64,
    phase_id: i64,
    target_work_unit_id: i64,
) -> Result<()> {
    let mut stmt = conn.prepare(
        r#"
        select record_type, record_id
        from work_phase_trace_decisions
        where phase_id = ?1
          and project_id = ?2
          and decision = 'split'
        order by id
        "#,
    )?;
    let rows = stmt.query_map(params![phase_id, project_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut decisions = Vec::new();
    for row in rows {
        decisions.push(row?);
    }
    drop(stmt);

    for (record_type, record_id) in decisions {
        if !phase_trace_record_exists(conn, project_id, phase_id, &record_type, record_id)? {
            continue;
        }
        match record_type.as_str() {
            "coverage_item" => {
                split_shared_coverage_item(conn, project_id, record_id, target_work_unit_id)?
            }
            "review_plan" => {
                split_shared_review_plan(conn, project_id, record_id, target_work_unit_id)?
            }
            "rule_binding" => {
                split_shared_rule_binding(conn, project_id, record_id, target_work_unit_id)?
            }
            "work_record" => {
                split_shared_work_record(conn, project_id, record_id, target_work_unit_id)?
            }
            _ => {}
        }
    }
    Ok(())
}

pub(super) fn split_shared_coverage_item(
    conn: &rusqlite::Connection,
    project_id: i64,
    coverage_item_id: i64,
    target_work_unit_id: i64,
) -> Result<()> {
    conn.execute(
        r#"
        insert into coverage_items(
            project_id, review_scope_id, work_unit_id, design_requirement_id,
            task_id, requirement, runtime_boundary_evidence, ux_boundary_evidence,
            lifecycle_boundary_evidence, tests_or_gates, missing_or_unverified,
            status, created_at
        )
        select
            project_id, review_scope_id, ?1, design_requirement_id,
            null, requirement, runtime_boundary_evidence, ux_boundary_evidence,
            lifecycle_boundary_evidence, tests_or_gates, missing_or_unverified,
            status, current_timestamp
        from coverage_items
        where id = ?2 and project_id = ?3
        "#,
        params![target_work_unit_id, coverage_item_id, project_id],
    )?;
    Ok(())
}

pub(super) fn split_shared_review_plan(
    conn: &rusqlite::Connection,
    project_id: i64,
    review_plan_id: i64,
    target_work_unit_id: i64,
) -> Result<()> {
    conn.execute(
        r#"
        insert into review_plans(
            project_id, work_unit_id, design_version_id, review_type, required,
            stage, scope, clean_condition, stop_condition, review_policy_id,
            review_scope_id, status, created_at
        )
        select
            project_id, ?1, design_version_id, review_type, required,
            stage, scope, clean_condition, stop_condition, review_policy_id,
            review_scope_id, 'open', current_timestamp
        from review_plans
        where id = ?2 and project_id = ?3
        "#,
        params![target_work_unit_id, review_plan_id, project_id],
    )?;
    let split_review_plan_id = conn.last_insert_rowid();
    conn.execute(
        r#"
        insert into review_plan_targets(review_plan_id, target_type, work_unit_id)
        values (?1, 'work_unit', ?2)
        "#,
        params![split_review_plan_id, target_work_unit_id],
    )?;
    let design_version_id: Option<i64> = conn
        .query_row(
            "select design_version_id from review_plans where id = ?1 and project_id = ?2",
            params![split_review_plan_id, project_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    if let Some(design_version_id) = design_version_id {
        conn.execute(
            r#"
            insert into review_plan_targets(review_plan_id, target_type, design_version_id)
            values (?1, 'design_version', ?2)
            "#,
            params![split_review_plan_id, design_version_id],
        )?;
    }
    Ok(())
}

pub(super) fn split_shared_rule_binding(
    conn: &rusqlite::Connection,
    project_id: i64,
    rule_binding_id: i64,
    target_work_unit_id: i64,
) -> Result<()> {
    conn.execute(
        r#"
        insert into rule_bindings(
            project_id, rule_source_type, authority_event_id, user_correction_id,
            command_profile_id, review_policy_id, review_plan_id, work_unit_id,
            validation_gate_id, acceptance_record_id, scope_type, scope_key,
            precedence, status, created_at
        )
        select
            project_id, rule_source_type, authority_event_id, user_correction_id,
            command_profile_id, review_policy_id, review_plan_id, ?1,
            validation_gate_id, acceptance_record_id, scope_type,
            case when scope_type = 'work_unit' then cast(?1 as text) else scope_key end,
            precedence, status, current_timestamp
        from rule_bindings
        where id = ?2 and project_id = ?3
        "#,
        params![target_work_unit_id, rule_binding_id, project_id],
    )?;
    Ok(())
}

pub(super) fn split_shared_work_record(
    conn: &rusqlite::Connection,
    project_id: i64,
    work_record_id: i64,
    target_work_unit_id: i64,
) -> Result<()> {
    conn.execute(
        r#"
        insert into work_records(
            project_id, work_unit_id, topic, work_performed, next_actions,
            notable_operations, export_path, created_at
        )
        select
            project_id, ?1, topic, work_performed, next_actions,
            notable_operations, export_path, current_timestamp
        from work_records
        where id = ?2 and project_id = ?3
        "#,
        params![target_work_unit_id, work_record_id, project_id],
    )?;
    Ok(())
}

pub(super) fn ensure_phase_trace_record_exists(
    conn: &rusqlite::Connection,
    project_id: i64,
    phase_id: i64,
    record_type: &str,
    record_id: i64,
) -> Result<()> {
    if !phase_trace_record_exists(conn, project_id, phase_id, record_type, record_id)? {
        bail!("{record_type}:{record_id} is not part of phase {phase_id} trace inventory");
    }
    Ok(())
}

pub(super) fn phase_trace_record_exists(
    conn: &rusqlite::Connection,
    project_id: i64,
    phase_id: i64,
    record_type: &str,
    record_id: i64,
) -> Result<bool> {
    let exists: i64 = match record_type {
        "task" => conn.query_row(
            r#"
            select exists(
                select 1
                from tasks t
                join work_phase_task_memberships m on m.task_id = t.id
                where m.phase_id = ?1 and t.id = ?2
            )
            "#,
            params![phase_id, record_id],
            |row| row.get(0),
        )?,
        "task_derivation" => conn.query_row(
            r#"
            select exists(
                select 1
                from task_derivations td
                join work_phase_task_memberships m on m.task_id = td.task_id
                where m.phase_id = ?1 and td.id = ?2
            )
            "#,
            params![phase_id, record_id],
            |row| row.get(0),
        )?,
        "checklist_item" => conn.query_row(
            r#"
            select exists(
                select 1
                from checklist_items ci
                join work_phase_task_memberships m on m.task_id = ci.task_id
                where m.phase_id = ?1 and ci.id = ?2
            )
            "#,
            params![phase_id, record_id],
            |row| row.get(0),
        )?,
        "validation_gate" => conn.query_row(
            r#"
            select exists(
                select 1
                from validation_gates vg
                join work_phase_task_memberships m on m.task_id = vg.task_id
                where m.phase_id = ?1 and vg.id = ?2
            )
            "#,
            params![phase_id, record_id],
            |row| row.get(0),
        )?,
        "coverage_item" => conn.query_row(
            r#"
            select exists(
                select 1
                from coverage_items c
                join work_phase_task_memberships m on m.task_id = c.task_id
                where m.phase_id = ?1 and c.id = ?2
            )
            or exists (
                select 1
                from coverage_items c
                join work_phases p on p.id = ?1
                where c.id = ?2
                  and c.project_id = ?3
                  and c.task_id is null
                  and c.work_unit_id = p.work_unit_id
            )
            "#,
            params![phase_id, record_id, project_id],
            |row| row.get(0),
        )?,
        "implementation_evidence" => conn.query_row(
            r#"
            select exists(
                select 1
                from implementation_evidence e
                join work_phase_task_memberships m on m.task_id = e.task_id
                where m.phase_id = ?1 and e.id = ?2
            )
            "#,
            params![phase_id, record_id],
            |row| row.get(0),
        )?,
        "review_plan" => conn.query_row(
            r#"
            select exists(
                select 1
                from work_phase_review_targets pt
                where pt.phase_id = ?1
                  and pt.review_plan_id = ?2
                  and pt.project_id = ?3
            )
            "#,
            params![phase_id, record_id, project_id],
            |row| row.get(0),
        )?,
        "rule_binding" => conn.query_row(
            r#"
            select exists(
                select 1
                from rule_bindings rb
                join work_phases p on p.id = ?1
                where rb.id = ?2
                  and rb.project_id = ?3
                  and rb.work_unit_id = p.work_unit_id
            )
            "#,
            params![phase_id, record_id, project_id],
            |row| row.get(0),
        )?,
        "work_record" => conn.query_row(
            r#"
            select exists(
                select 1
                from work_records wr
                join work_phases p on p.id = ?1
                where wr.id = ?2
                  and wr.project_id = ?3
                  and wr.work_unit_id = p.work_unit_id
            )
            "#,
            params![phase_id, record_id, project_id],
            |row| row.get(0),
        )?,
        _ => 0,
    };
    Ok(exists == 1)
}

pub(super) fn move_phase_checklist_items(
    conn: &rusqlite::Connection,
    project_id: i64,
    phase_id: i64,
    target_work_unit_id: i64,
) -> Result<()> {
    let mut stmt = conn.prepare(
        r#"
        select distinct c.id, c.design_version_id, c.title
        from checklists c
        join checklist_items ci on ci.checklist_id = c.id
        join work_phase_task_memberships m on m.task_id = ci.task_id
        where m.phase_id = ?1
        "#,
    )?;
    let rows = stmt.query_map(params![phase_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut checklist_rows = Vec::new();
    for row in rows {
        checklist_rows.push(row?);
    }
    drop(stmt);
    for (source_checklist_id, design_version_id, title) in checklist_rows {
        conn.execute(
            r#"
            insert into checklists(
                project_id, work_unit_id, design_version_id, title, status, created_at
            )
            values (?1, ?2, ?3, ?4, 'active', current_timestamp)
            "#,
            params![
                project_id,
                target_work_unit_id,
                design_version_id,
                format!("Phase split: {title}"),
            ],
        )?;
        let target_checklist_id = conn.last_insert_rowid();
        conn.execute(
            r#"
            update checklist_items
            set checklist_id = ?1
            where checklist_id = ?2
              and task_id in (
                  select task_id
                  from work_phase_task_memberships
                  where phase_id = ?3
              )
            "#,
            params![target_checklist_id, source_checklist_id, phase_id],
        )?;
    }
    Ok(())
}

pub(super) fn inventory_lines(
    conn: &rusqlite::Connection,
    project_id: i64,
    phase_id: i64,
) -> Result<Vec<String>> {
    let mut lines = Vec::new();
    let mut trace = Vec::new();
    collect_trace_rows(
        conn,
        phase_id,
        "task",
        "select t.id, t.status, t.title from tasks t join work_phase_task_memberships m on m.task_id = t.id where m.phase_id = ?1 order by t.id",
        &mut trace,
    )?;
    collect_trace_rows(
        conn,
        phase_id,
        "task_derivation",
        "select td.id, td.status, dr.requirement_key || ' task=' || td.task_id from task_derivations td join design_requirements dr on dr.id = td.design_requirement_id join work_phase_task_memberships m on m.task_id = td.task_id where m.phase_id = ?1 order by td.id",
        &mut trace,
    )?;
    collect_trace_rows(
        conn,
        phase_id,
        "checklist_item",
        "select ci.id, ci.status, ci.title from checklist_items ci join work_phase_task_memberships m on m.task_id = ci.task_id where m.phase_id = ?1 order by ci.id",
        &mut trace,
    )?;
    collect_trace_rows(
        conn,
        phase_id,
        "validation_gate",
        "select vg.id, vg.status, vg.gate_key from validation_gates vg join work_phase_task_memberships m on m.task_id = vg.task_id where m.phase_id = ?1 order by vg.id",
        &mut trace,
    )?;
    collect_trace_rows(
        conn,
        phase_id,
        "coverage_item",
        "select c.id, c.status, dr.requirement_key from coverage_items c join design_requirements dr on dr.id = c.design_requirement_id join work_phase_task_memberships m on m.task_id = c.task_id where m.phase_id = ?1 order by c.id",
        &mut trace,
    )?;
    collect_trace_rows(
        conn,
        phase_id,
        "implementation_evidence",
        "select e.id, e.evidence_type, e.evidence_type from implementation_evidence e join work_phase_task_memberships m on m.task_id = e.task_id where m.phase_id = ?1 order by e.id",
        &mut trace,
    )?;
    collect_shared_trace_rows(conn, project_id, phase_id, &mut trace)?;
    for record in trace {
        lines.push(format!(
            "{}:{} [{}] {}",
            record.record_type, record.id, record.status, record.label
        ));
    }
    Ok(lines)
}

pub(super) fn collect_shared_trace_rows(
    conn: &rusqlite::Connection,
    project_id: i64,
    phase_id: i64,
    records: &mut Vec<PhaseTraceRecord>,
) -> Result<()> {
    let phase = load_phase(conn, project_id, phase_id)?;
    collect_trace_rows(
        conn,
        phase_id,
        "coverage_item",
        r#"
        select c.id, c.status, dr.requirement_key || ' shared'
        from coverage_items c
        join design_requirements dr on dr.id = c.design_requirement_id
        where c.project_id = (select project_id from work_phases where id = ?1)
          and c.task_id is null
          and c.work_unit_id = (select work_unit_id from work_phases where id = ?1)
        order by c.id
        "#,
        records,
    )?;
    collect_trace_rows(
        conn,
        phase_id,
        "review_plan",
        r#"
        select rp.id, rp.status, rp.review_type || ':' || rp.stage
        from review_plans rp
        join work_phase_review_targets pt on pt.review_plan_id = rp.id
        where pt.phase_id = ?1
        order by rp.id
        "#,
        records,
    )?;
    collect_work_unit_trace_rows(
        conn,
        project_id,
        phase.work_unit_id,
        "rule_binding",
        "select id, status, rule_source_type from rule_bindings where project_id = ?1 and work_unit_id = ?2 order by id",
        records,
    )?;
    collect_work_unit_trace_rows(
        conn,
        project_id,
        phase.work_unit_id,
        "work_record",
        "select id, 'recorded', topic from work_records where project_id = ?1 and work_unit_id = ?2 order by id",
        records,
    )?;
    Ok(())
}

pub(super) fn shared_records_missing_decisions(
    conn: &rusqlite::Connection,
    project_id: i64,
    phase_id: i64,
) -> Result<Vec<SharedRecordRef>> {
    let mut trace = Vec::new();
    collect_shared_trace_rows(conn, project_id, phase_id, &mut trace)?;
    let mut missing = Vec::new();
    for record in trace {
        let has_decision: bool = conn.query_row(
            r#"
            select exists(
                select 1
                from work_phase_trace_decisions
                where phase_id = ?1 and record_type = ?2 and record_id = ?3
            )
            "#,
            params![phase_id, record.record_type, record.id],
            |row| row.get(0),
        )?;
        if !has_decision {
            missing.push(SharedRecordRef {
                record_type: record.record_type,
                record_id: record.id,
            });
        }
    }
    Ok(missing)
}

pub(super) fn attach_trace_decisions(
    conn: &rusqlite::Connection,
    phase_id: i64,
    records: &mut [PhaseTraceRecord],
) -> Result<()> {
    for record in records {
        record.decision = conn
            .query_row(
                r#"
                select decision
                from work_phase_trace_decisions
                where phase_id = ?1 and record_type = ?2 and record_id = ?3
                "#,
                params![phase_id, record.record_type, record.id],
                |row| row.get(0),
            )
            .optional()?;
    }
    Ok(())
}

pub(super) fn collect_trace_rows(
    conn: &rusqlite::Connection,
    phase_id: i64,
    record_type: &str,
    sql: &str,
    output: &mut Vec<PhaseTraceRecord>,
) -> Result<()> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![phase_id], |row| {
        Ok(PhaseTraceRecord {
            record_type: record_type.to_string(),
            id: row.get(0)?,
            status: row.get(1)?,
            label: row.get(2)?,
            decision: None,
        })
    })?;
    for row in rows {
        output.push(row?);
    }
    Ok(())
}

pub(super) fn collect_work_unit_trace_rows(
    conn: &rusqlite::Connection,
    project_id: i64,
    work_unit_id: i64,
    record_type: &str,
    sql: &str,
    output: &mut Vec<PhaseTraceRecord>,
) -> Result<()> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![project_id, work_unit_id], |row| {
        Ok(PhaseTraceRecord {
            record_type: record_type.to_string(),
            id: row.get(0)?,
            status: row.get(1)?,
            label: row.get(2)?,
            decision: None,
        })
    })?;
    for row in rows {
        output.push(row?);
    }
    Ok(())
}
