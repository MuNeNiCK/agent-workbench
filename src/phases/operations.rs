use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{open_existing_project, project_id};

use super::*;

pub fn create_phase(root: &Path, input: NewWorkPhase<'_>) -> Result<WorkPhaseOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    ensure_no_active_source_correction(&tx, "phase create")?;
    let outcome = create_phase_in(&tx, project_id, input)?;
    tx.commit()?;
    Ok(outcome)
}

pub(crate) fn create_phase_in(
    conn: &rusqlite::Connection,
    project_id: i64,
    input: NewWorkPhase<'_>,
) -> Result<WorkPhaseOutcome> {
    validate_phase_key(input.key)?;
    validate_phase_kind(input.kind)?;
    conn.query_row(
        "select 1 from work_units where id = ?1 and project_id = ?2 and status in ('open', 'blocked')",
        params![input.work_unit_id, project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("open work unit not found")?;
    if let Some(design_version_id) = input.design_version_id {
        conn.query_row(
            "select 1 from design_versions where id = ?1 and project_id = ?2",
            params![design_version_id, project_id],
            |_| Ok(()),
        )
        .optional()?
        .context("design version not found")?;
    }
    conn.execute(
        r#"
        insert into work_phases(
            project_id, work_unit_id, design_version_id, phase_key, title,
            kind, phase_order, status, reason, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'open', ?8, current_timestamp)
        "#,
        params![
            project_id,
            input.work_unit_id,
            input.design_version_id,
            input.key,
            input.title,
            input.kind,
            input.order,
            input.reason,
        ],
    )?;
    let phase_id = conn.last_insert_rowid();
    insert_phase_event(
        conn,
        project_id,
        phase_id,
        "created",
        input.reason,
        None,
        None,
        None,
        None,
        Some("open"),
    )?;
    Ok(WorkPhaseOutcome { phase_id })
}

pub fn list_phases(root: &Path, work_unit_id: i64) -> Result<Vec<WorkPhaseRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select
            p.id, p.work_unit_id, p.phase_work_unit_id, p.design_version_id,
            p.phase_key, p.title, p.kind, p.phase_order, p.status,
            count(m.task_id)
        from work_phases p
        left join work_phase_task_memberships m on m.phase_id = p.id
        where p.project_id = ?1 and p.work_unit_id = ?2
        group by p.id
        order by p.phase_order, p.id
        "#,
    )?;
    let rows = stmt.query_map(params![project_id, work_unit_id], phase_record)?;
    collect_rows(rows)
}

pub fn show_phase(root: &Path, phase_id: i64) -> Result<WorkPhaseRecord> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    conn.query_row(
        r#"
        select
            p.id, p.work_unit_id, p.phase_work_unit_id, p.design_version_id,
            p.phase_key, p.title, p.kind, p.phase_order, p.status,
            count(m.task_id)
        from work_phases p
        left join work_phase_task_memberships m on m.phase_id = p.id
        where p.project_id = ?1 and p.id = ?2
        group by p.id
        "#,
        params![project_id, phase_id],
        phase_record,
    )
    .optional()?
    .context("phase not found")
}

pub fn assign_task_to_phase(root: &Path, phase_id: i64, task_id: i64) -> Result<PhaseTaskOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    ensure_no_active_source_correction(&tx, "phase assign")?;
    let outcome = assign_task_to_phase_in(&tx, project_id, phase_id, task_id)?;
    tx.commit()?;
    Ok(outcome)
}

pub(crate) fn assign_task_to_phase_in(
    conn: &rusqlite::Connection,
    project_id: i64,
    phase_id: i64,
    task_id: i64,
) -> Result<PhaseTaskOutcome> {
    let phase = load_phase(conn, project_id, phase_id)?;
    let task_work_unit_id: Option<i64> = conn
        .query_row(
            "select work_unit_id from tasks where id = ?1",
            params![task_id],
            |row| row.get(0),
        )
        .optional()?
        .context("task not found")?;
    let allowed_child = phase.phase_work_unit_id;
    if task_work_unit_id != Some(phase.work_unit_id) && task_work_unit_id != allowed_child {
        bail!("phase assignment task must belong to the phase aggregate or phase work unit");
    }
    conn.execute(
        r#"
        insert into work_phase_task_memberships(project_id, phase_id, task_id, assigned_at)
        values (?1, ?2, ?3, current_timestamp)
        on conflict(task_id) do update set
            phase_id = excluded.phase_id,
            project_id = excluded.project_id,
            assigned_at = excluded.assigned_at
        "#,
        params![project_id, phase_id, task_id],
    )?;
    insert_phase_event(
        conn,
        project_id,
        phase_id,
        "assigned",
        Some("assigned task to phase"),
        None,
        Some(task_id),
        None,
        None,
        None,
    )?;
    Ok(PhaseTaskOutcome { phase_id, task_id })
}

pub fn add_phase_dependency(
    root: &Path,
    input: NewPhaseDependency<'_>,
) -> Result<PhaseDependencyOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    ensure_no_active_source_correction(&tx, "phase dependency add")?;
    let outcome = add_phase_dependency_in(&tx, project_id, input)?;
    tx.commit()?;
    Ok(outcome)
}

pub(crate) fn add_phase_dependency_in(
    conn: &rusqlite::Connection,
    project_id: i64,
    input: NewPhaseDependency<'_>,
) -> Result<PhaseDependencyOutcome> {
    validate_dependency_type(input.dependency_type)?;
    let from = load_phase(conn, project_id, input.from_phase_id)?;
    let to = load_phase(conn, project_id, input.to_phase_id)?;
    if from.work_unit_id != to.work_unit_id {
        bail!("phase dependencies must stay inside one aggregate work unit");
    }
    conn.execute(
        r#"
        insert into work_phase_dependencies(
            project_id, from_phase_id, to_phase_id, dependency_type,
            reason, status, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, 'open', current_timestamp)
        "#,
        params![
            project_id,
            input.from_phase_id,
            input.to_phase_id,
            input.dependency_type,
            input.reason,
        ],
    )?;
    let dependency_id = conn.last_insert_rowid();
    insert_phase_event(
        conn,
        project_id,
        input.to_phase_id,
        "dependency_added",
        Some(input.reason),
        None,
        None,
        None,
        None,
        None,
    )?;
    Ok(PhaseDependencyOutcome { dependency_id })
}

pub fn list_phase_dependencies(
    root: &Path,
    work_unit_id: i64,
) -> Result<Vec<PhaseDependencyRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select
            d.id, d.from_phase_id, fp.phase_key, d.to_phase_id, tp.phase_key,
            d.dependency_type, d.status, d.reason, d.evidence_ref,
            d.authority_event_id
        from work_phase_dependencies d
        join work_phases fp on fp.id = d.from_phase_id
        join work_phases tp on tp.id = d.to_phase_id
        where d.project_id = ?1 and fp.work_unit_id = ?2
        order by d.id
        "#,
    )?;
    let rows = stmt.query_map(params![project_id, work_unit_id], |row| {
        Ok(PhaseDependencyRecord {
            id: row.get(0)?,
            from_phase_id: row.get(1)?,
            from_phase_key: row.get(2)?,
            to_phase_id: row.get(3)?,
            to_phase_key: row.get(4)?,
            dependency_type: row.get(5)?,
            status: row.get(6)?,
            reason: row.get(7)?,
            evidence_ref: row.get(8)?,
            authority_event_id: row.get(9)?,
        })
    })?;
    collect_rows(rows)
}

pub fn satisfy_phase_dependency(
    root: &Path,
    dependency_id: i64,
    reason: &str,
    evidence_ref: &str,
) -> Result<PhaseDependencyOutcome> {
    update_dependency_status(
        root,
        dependency_id,
        "satisfied",
        reason,
        Some(evidence_ref),
        None,
    )
}

pub fn accept_phase_dependency(
    root: &Path,
    dependency_id: i64,
    reason: &str,
    authority_event_id: i64,
) -> Result<PhaseDependencyOutcome> {
    update_dependency_status(
        root,
        dependency_id,
        "accepted",
        reason,
        None,
        Some(authority_event_id),
    )
}

pub fn list_phase_trace(root: &Path, phase_id: i64) -> Result<Vec<PhaseTraceRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    load_phase(&conn, project_id, phase_id)?;
    let mut records = Vec::new();
    collect_trace_rows(
        &conn,
        phase_id,
        "task",
        r#"
        select t.id, t.status, t.title
        from tasks t
        join work_phase_task_memberships m on m.task_id = t.id
        where m.phase_id = ?1
        order by t.id
        "#,
        &mut records,
    )?;
    collect_trace_rows(
        &conn,
        phase_id,
        "task_derivation",
        r#"
        select td.id, td.status, dr.requirement_key || ' task=' || td.task_id
        from task_derivations td
        join design_requirements dr on dr.id = td.design_requirement_id
        join work_phase_task_memberships m on m.task_id = td.task_id
        where m.phase_id = ?1
        order by td.id
        "#,
        &mut records,
    )?;
    collect_trace_rows(
        &conn,
        phase_id,
        "checklist_item",
        r#"
        select ci.id, ci.status, ci.title
        from checklist_items ci
        join work_phase_task_memberships m on m.task_id = ci.task_id
        where m.phase_id = ?1
        order by ci.id
        "#,
        &mut records,
    )?;
    collect_trace_rows(
        &conn,
        phase_id,
        "validation_gate",
        r#"
        select vg.id, vg.status, vg.gate_key || coalesce(' task=' || vg.task_id, '')
        from validation_gates vg
        join work_phase_task_memberships m on m.task_id = vg.task_id
        where m.phase_id = ?1
        order by vg.id
        "#,
        &mut records,
    )?;
    collect_trace_rows(
        &conn,
        phase_id,
        "coverage_item",
        r#"
        select c.id, c.status, dr.requirement_key || coalesce(' task=' || c.task_id, '')
        from coverage_items c
        join design_requirements dr on dr.id = c.design_requirement_id
        join work_phase_task_memberships m on m.task_id = c.task_id
        where m.phase_id = ?1
        order by c.id
        "#,
        &mut records,
    )?;
    collect_trace_rows(
        &conn,
        phase_id,
        "implementation_evidence",
        r#"
        select e.id, e.evidence_type, e.evidence_type || coalesce(' task=' || e.task_id, '')
        from implementation_evidence e
        join work_phase_task_memberships m on m.task_id = e.task_id
        where m.phase_id = ?1
        order by e.id
        "#,
        &mut records,
    )?;
    collect_shared_trace_rows(&conn, project_id, phase_id, &mut records)?;
    attach_trace_decisions(&conn, phase_id, &mut records)?;
    Ok(records)
}

pub fn decide_phase_trace(
    root: &Path,
    input: NewPhaseTraceDecision<'_>,
) -> Result<PhaseTraceDecisionOutcome> {
    validate_trace_record_type(input.record_type)?;
    validate_trace_decision(input.decision)?;
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    load_phase(&tx, project_id, input.phase_id)?;
    ensure_phase_trace_record_exists(
        &tx,
        project_id,
        input.phase_id,
        input.record_type,
        input.record_id,
    )?;
    ensure_authority_event(&tx, project_id, input.authority_event_id)?;
    tx.execute(
        r#"
        insert into work_phase_trace_decisions(
            project_id, phase_id, record_type, record_id, decision,
            reason, authority_event_id, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, current_timestamp)
        on conflict(phase_id, record_type, record_id) do update set
            decision = excluded.decision,
            reason = excluded.reason,
            authority_event_id = excluded.authority_event_id,
            created_at = excluded.created_at
        "#,
        params![
            project_id,
            input.phase_id,
            input.record_type,
            input.record_id,
            input.decision,
            input.reason,
            input.authority_event_id,
        ],
    )?;
    let decision_id = tx.query_row(
        r#"
        select id from work_phase_trace_decisions
        where phase_id = ?1 and record_type = ?2 and record_id = ?3
        "#,
        params![input.phase_id, input.record_type, input.record_id],
        |row| row.get(0),
    )?;
    insert_phase_event(
        &tx,
        project_id,
        input.phase_id,
        "trace_decided",
        Some(input.reason),
        Some(input.authority_event_id),
        None,
        None,
        None,
        None,
    )?;
    tx.commit()?;
    Ok(PhaseTraceDecisionOutcome { decision_id })
}

pub fn phase_inventory(root: &Path, phase_id: i64) -> Result<PhaseInventory> {
    let trace = list_phase_trace(root, phase_id)?;
    Ok(PhaseInventory { phase_id, trace })
}

pub fn phase_rescope(root: &Path, input: PhaseRescope<'_>) -> Result<PhaseRescopeOutcome> {
    validate_shared_record_policy(input.shared_record_policy)?;
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    ensure_no_active_source_correction(&tx, "phase rescope")?;
    let phase = load_phase(&tx, project_id, input.phase_id)?;
    ensure_phase_can_move(&phase)?;
    let target_work_unit_id = input
        .to_work_unit_id
        .context("rescope requires target work unit")?;
    ensure_open_work_unit(&tx, project_id, target_work_unit_id)?;
    let report = build_rescope_report(
        &tx,
        project_id,
        &phase,
        target_work_unit_id,
        input.shared_record_policy,
    )?;
    if input.dry_run {
        insert_phase_event(
            &tx,
            project_id,
            phase.id,
            "rescope_dry_run",
            Some("phase rescope dry-run"),
            None,
            None,
            Some(target_work_unit_id),
            None,
            None,
        )?;
        tx.commit()?;
        return Ok(report);
    }
    if !report.blockers.is_empty() {
        bail!("phase rescope has blockers; run --dry-run and resolve printed next commands");
    }
    move_phase_trace_bundle(&tx, project_id, &phase, target_work_unit_id)?;
    tx.execute(
        "update work_phases set phase_work_unit_id = ?1, status = 'split' where id = ?2",
        params![target_work_unit_id, phase.id],
    )?;
    insert_phase_event(
        &tx,
        project_id,
        phase.id,
        "rescoped",
        Some("phase rescoped to work unit"),
        None,
        None,
        Some(target_work_unit_id),
        Some(&phase.status),
        Some("split"),
    )?;
    tx.commit()?;
    Ok(report)
}

pub fn phase_split(root: &Path, input: PhaseSplit<'_>) -> Result<PhaseRescopeOutcome> {
    validate_shared_record_policy(input.shared_record_policy)?;
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    ensure_no_active_source_correction(&tx, "phase split")?;
    let phase = load_phase(&tx, project_id, input.phase_id)?;
    ensure_phase_can_move(&phase)?;
    let report = build_rescope_report(
        &tx,
        project_id,
        &phase,
        phase.work_unit_id,
        input.shared_record_policy,
    )?;
    if input.dry_run {
        insert_phase_event(
            &tx,
            project_id,
            phase.id,
            "rescope_dry_run",
            Some(input.reason),
            None,
            None,
            None,
            None,
            None,
        )?;
        tx.commit()?;
        return Ok(report);
    }
    if !report.blockers.is_empty() {
        bail!("phase split has blockers; run --dry-run and resolve printed next commands");
    }
    tx.execute(
        r#"
        insert into work_units(
            project_id, parent_work_unit_id, title, status, responsibility,
            interrupt_reason, started_at
        )
        values (?1, ?2, ?3, 'open', 'phase split work', ?4, current_timestamp)
        "#,
        params![project_id, phase.work_unit_id, input.title, input.reason],
    )?;
    let child_work_unit_id = tx.last_insert_rowid();
    move_phase_trace_bundle(&tx, project_id, &phase, child_work_unit_id)?;
    tx.execute(
        "update work_phases set phase_work_unit_id = ?1, status = 'split' where id = ?2",
        params![child_work_unit_id, phase.id],
    )?;
    insert_phase_event(
        &tx,
        project_id,
        phase.id,
        "split",
        Some(input.reason),
        None,
        None,
        Some(child_work_unit_id),
        Some(&phase.status),
        Some("split"),
    )?;
    tx.commit()?;
    let mut report = report;
    report.target_work_unit_id = Some(child_work_unit_id);
    Ok(report)
}

pub fn phase_close_ready(root: &Path, phase_id: i64) -> Result<PhaseCloseReadyOutcome> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let phase = load_phase(&conn, project_id, phase_id)?;
    let mut items = Vec::new();
    let task_count = phase_task_count(&conn, phase_id)?;
    let open_task_count = count_phase_tasks_with_status(&conn, phase_id, "open", "blocked")?;
    items.push(if open_task_count == 0 {
        PhaseCloseReadyItem::pass("phase_tasks_closed", format!("{task_count} phase tasks"))
    } else {
        PhaseCloseReadyItem::fail(
            "phase_tasks_closed",
            "close phase tasks or run phase accept-out-of-scope",
            format!("{open_task_count} open or blocked phase tasks"),
        )
    });
    let open_checklist_items = count_open_phase_checklist_items(&conn, phase_id)?;
    items.push(if open_checklist_items == 0 {
        PhaseCloseReadyItem::pass("checklist_items_closed", "phase checklist items closed")
    } else {
        PhaseCloseReadyItem::fail(
            "checklist_items_closed",
            "close checklist items for phase tasks",
            format!("{open_checklist_items} open or blocked checklist items"),
        )
    });
    let missing_gate_runs = count_phase_validation_gate_blockers(&conn, phase_id)?;
    items.push(if missing_gate_runs == 0 {
        PhaseCloseReadyItem::pass("validation_runs_recorded", "phase validation gates pass")
    } else {
        PhaseCloseReadyItem::fail(
            "validation_runs_recorded",
            "record passing validation runs or accepted failures for phase gates",
            format!("{missing_gate_runs} phase validation gates lack passing or accepted runs"),
        )
    });
    let missing_evidence = count_phase_missing_evidence(&conn, phase_id)?;
    let missing_coverage = count_phase_missing_coverage(&conn, phase_id)?;
    items.push(if missing_evidence == 0 && missing_coverage == 0 {
        PhaseCloseReadyItem::pass(
            "evidence_and_coverage_present",
            "phase trace coverage complete",
        )
    } else {
        PhaseCloseReadyItem::fail(
            "evidence_and_coverage_present",
            "record implementation evidence and coverage for phase tasks",
            format!("{missing_evidence} missing evidence, {missing_coverage} missing coverage"),
        )
    });
    let incomplete_reviews = count_incomplete_phase_reviews(&conn, phase_id)?;
    let review_action = phase_review_lifecycle_action(&conn, phase.work_unit_id, Some(phase_id))?;
    items.push(if incomplete_reviews == 0 {
        PhaseCloseReadyItem::pass(
            "phase_reviews_clean",
            "required phase reviews clean or waived",
        )
    } else {
        PhaseCloseReadyItem::fail(
            "phase_reviews_clean",
            review_action
                .as_ref()
                .map(|action| action.action.as_str())
                .unwrap_or("record a clean phase review run for the printed review context"),
            format!("{incomplete_reviews} required phase review plans incomplete"),
        )
    });
    let dependency_blockers = count_open_phase_inbound_dependencies(&conn, phase_id)?;
    items.push(if dependency_blockers == 0 {
        PhaseCloseReadyItem::pass("dependencies_clear", "phase dependencies satisfied")
    } else {
        PhaseCloseReadyItem::fail(
            "dependencies_clear",
            "satisfy or accept phase dependencies before closing phase",
            format!("{dependency_blockers} open inbound phase dependencies"),
        )
    });
    let result = if phase.status == "accepted_out_of_scope"
        || items.iter().all(|item| item.result == "pass")
    {
        "pass"
    } else {
        "blocked"
    };
    Ok(PhaseCloseReadyOutcome {
        phase_id,
        work_unit_id: Some(phase.work_unit_id),
        result: result.to_string(),
        items,
    })
}

pub fn close_phase(root: &Path, phase_id: i64, summary: &str) -> Result<PhaseCloseOutcome> {
    let ready = phase_close_ready(root, phase_id)?;
    if ready.result != "pass" {
        bail!("cannot close phase; phase close-ready is blocked");
    }
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    ensure_no_active_source_correction(&tx, "phase close")?;
    let phase = load_phase(&tx, project_id, phase_id)?;
    if phase.status == "closed" {
        bail!("phase is already closed");
    }
    if phase.status != "accepted_out_of_scope"
        && (count_phase_tasks_with_status(&tx, phase_id, "open", "blocked")? > 0
            || count_open_phase_checklist_items(&tx, phase_id)? > 0
            || count_phase_validation_gate_blockers(&tx, phase_id)? > 0
            || count_phase_missing_evidence(&tx, phase_id)? > 0
            || count_phase_missing_coverage(&tx, phase_id)? > 0
            || count_incomplete_phase_reviews(&tx, phase_id)? > 0
            || count_open_phase_inbound_dependencies(&tx, phase_id)? > 0)
    {
        bail!("cannot close phase; phase close-ready changed before commit");
    }
    tx.execute(
        "update work_phases set status = 'closed', closed_at = current_timestamp, close_summary = ?1 where id = ?2",
        params![summary, phase_id],
    )?;
    tx.execute(
        r#"
        update work_phase_dependencies
        set status = 'satisfied', resolved_at = current_timestamp, evidence_ref = ?1
        where from_phase_id = ?2 and status = 'open'
        "#,
        params![format!("phase:{phase_id}:closed"), phase_id],
    )?;
    insert_phase_event(
        &tx,
        project_id,
        phase_id,
        "closed",
        Some(summary),
        None,
        None,
        None,
        Some(&phase.status),
        Some("closed"),
    )?;
    tx.commit()?;
    Ok(PhaseCloseOutcome { phase_id })
}

pub fn accept_phase_out_of_scope(
    root: &Path,
    phase_id: i64,
    reason: &str,
    authority_event_id: i64,
) -> Result<PhaseAcceptanceOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    ensure_no_active_source_correction(&tx, "phase accept-out-of-scope")?;
    let phase = load_phase(&tx, project_id, phase_id)?;
    ensure_authority_event(&tx, project_id, authority_event_id)?;
    let changed = tx.execute(
        r#"
        update work_phases
        set status = 'accepted_out_of_scope', authority_event_id = ?1,
            closed_at = current_timestamp, close_summary = ?2
        where id = ?3 and status in ('open', 'blocked')
        "#,
        params![authority_event_id, reason, phase_id],
    )?;
    if changed == 0 {
        bail!(
            "phase must be open or blocked to accept out of scope; current status is {}",
            phase.status
        );
    }
    insert_phase_event(
        &tx,
        project_id,
        phase_id,
        "accepted_out_of_scope",
        Some(reason),
        Some(authority_event_id),
        None,
        None,
        Some(&phase.status),
        Some("accepted_out_of_scope"),
    )?;
    tx.commit()?;
    Ok(PhaseAcceptanceOutcome {
        phase_id,
        authority_event_id,
    })
}

pub fn add_phase_review_target(
    root: &Path,
    review_plan_id: i64,
    phase_id: i64,
) -> Result<PhaseReviewTargetOutcome> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let phase = load_phase(&conn, project_id, phase_id)?;
    conn.query_row(
        "select 1 from review_plans where id = ?1 and project_id = ?2 and work_unit_id = ?3",
        params![review_plan_id, project_id, phase.work_unit_id],
        |_| Ok(()),
    )
    .optional()?
    .context("review plan not found for phase work unit")?;
    conn.execute(
        r#"
        insert into work_phase_review_targets(project_id, review_plan_id, phase_id, created_at)
        values (?1, ?2, ?3, current_timestamp)
        "#,
        params![project_id, review_plan_id, phase_id],
    )?;
    Ok(PhaseReviewTargetOutcome {
        review_plan_target_id: conn.last_insert_rowid(),
    })
}

fn update_dependency_status(
    root: &Path,
    dependency_id: i64,
    status: &str,
    reason: &str,
    evidence_ref: Option<&str>,
    authority_event_id: Option<i64>,
) -> Result<PhaseDependencyOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    ensure_no_active_source_correction(&tx, "phase dependency transition")?;
    let outcome = update_dependency_status_in(
        &tx,
        project_id,
        dependency_id,
        status,
        reason,
        evidence_ref,
        authority_event_id,
    )?;
    tx.commit()?;
    Ok(outcome)
}

pub(crate) fn update_dependency_status_in(
    conn: &rusqlite::Connection,
    project_id: i64,
    dependency_id: i64,
    status: &str,
    reason: &str,
    evidence_ref: Option<&str>,
    authority_event_id: Option<i64>,
) -> Result<PhaseDependencyOutcome> {
    if let Some(authority_event_id) = authority_event_id {
        ensure_authority_event(conn, project_id, authority_event_id)?;
    }
    let phase_id: i64 = conn
        .query_row(
            r#"
            select to_phase_id
            from work_phase_dependencies
            where id = ?1 and project_id = ?2 and status = 'open'
            "#,
            params![dependency_id, project_id],
            |row| row.get(0),
        )
        .optional()?
        .context("open phase dependency not found")?;
    conn.execute(
        r#"
        update work_phase_dependencies
        set status = ?1, reason = ?2, evidence_ref = ?3,
            authority_event_id = ?4, resolved_at = current_timestamp
        where id = ?5 and project_id = ?6
        "#,
        params![
            status,
            reason,
            evidence_ref,
            authority_event_id,
            dependency_id,
            project_id
        ],
    )?;
    insert_phase_event(
        conn,
        project_id,
        phase_id,
        if status == "accepted" {
            "dependency_accepted"
        } else {
            "dependency_satisfied"
        },
        Some(reason),
        authority_event_id,
        None,
        None,
        None,
        None,
    )?;
    Ok(PhaseDependencyOutcome { dependency_id })
}
