use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{ensure_unscoped_mutation_allowed, open_existing_project, project_id};
use crate::review_context::review_context_ref_with_phase;

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
    items.push(if incomplete_reviews == 0 {
        PhaseCloseReadyItem::pass(
            "phase_reviews_clean",
            "required phase reviews clean or waived",
        )
    } else {
        PhaseCloseReadyItem::fail(
            "phase_reviews_clean",
            "record clean phase review runs or waive approved exceptions",
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

fn ensure_no_active_source_correction(conn: &rusqlite::Connection, operation: &str) -> Result<()> {
    ensure_unscoped_mutation_allowed(conn, operation)
}

fn build_rescope_report(
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

fn move_phase_trace_bundle(
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

fn split_shared_trace_records(
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

fn split_shared_coverage_item(
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

fn split_shared_review_plan(
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

fn split_shared_rule_binding(
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

fn split_shared_work_record(
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

fn ensure_phase_trace_record_exists(
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

fn phase_trace_record_exists(
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

fn move_phase_checklist_items(
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

fn inventory_lines(
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

fn collect_shared_trace_rows(
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

fn shared_records_missing_decisions(
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

fn attach_trace_decisions(
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

fn collect_trace_rows(
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

fn collect_work_unit_trace_rows(
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

fn phase_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkPhaseRecord> {
    Ok(WorkPhaseRecord {
        id: row.get(0)?,
        work_unit_id: row.get(1)?,
        phase_work_unit_id: row.get(2)?,
        design_version_id: row.get(3)?,
        key: row.get(4)?,
        title: row.get(5)?,
        kind: row.get(6)?,
        order: row.get(7)?,
        status: row.get(8)?,
        task_count: row.get(9)?,
    })
}

fn load_phase(conn: &rusqlite::Connection, project_id: i64, phase_id: i64) -> Result<StoredPhase> {
    conn.query_row(
        r#"
        select id, work_unit_id, phase_work_unit_id, status
        from work_phases
        where id = ?1 and project_id = ?2
        "#,
        params![phase_id, project_id],
        |row| {
            Ok(StoredPhase {
                id: row.get(0)?,
                work_unit_id: row.get(1)?,
                phase_work_unit_id: row.get(2)?,
                status: row.get(3)?,
            })
        },
    )
    .optional()?
    .context("phase not found")
}

fn ensure_open_work_unit(
    conn: &rusqlite::Connection,
    project_id: i64,
    work_unit_id: i64,
) -> Result<()> {
    conn.query_row(
        "select 1 from work_units where id = ?1 and project_id = ?2 and status = 'open'",
        params![work_unit_id, project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("open target work unit not found")?;
    Ok(())
}

fn ensure_phase_can_move(phase: &StoredPhase) -> Result<()> {
    if !matches!(phase.status.as_str(), "open" | "blocked") {
        bail!(
            "phase must be open or blocked to split or rescope; current status is {}",
            phase.status
        );
    }
    Ok(())
}

fn ensure_authority_event(
    conn: &rusqlite::Connection,
    project_id: i64,
    authority_event_id: i64,
) -> Result<()> {
    conn.query_row(
        "select 1 from authority_events where id = ?1 and project_id = ?2 and status = 'active'",
        params![authority_event_id, project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("active authority event not found")?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn insert_phase_event(
    conn: &rusqlite::Connection,
    project_id: i64,
    phase_id: i64,
    event_type: &str,
    reason: Option<&str>,
    authority_event_id: Option<i64>,
    related_task_id: Option<i64>,
    related_work_unit_id: Option<i64>,
    previous_status: Option<&str>,
    next_status: Option<&str>,
) -> Result<()> {
    conn.execute(
        r#"
        insert into work_phase_events(
            project_id, phase_id, event_type, reason, authority_event_id,
            related_task_id, related_work_unit_id, previous_status, next_status,
            created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, current_timestamp)
        "#,
        params![
            project_id,
            phase_id,
            event_type,
            reason,
            authority_event_id,
            related_task_id,
            related_work_unit_id,
            previous_status,
            next_status,
        ],
    )?;
    Ok(())
}

fn phase_task_ids(conn: &rusqlite::Connection, phase_id: i64) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "select task_id from work_phase_task_memberships where phase_id = ?1 order by task_id",
    )?;
    let rows = stmt.query_map(params![phase_id], |row| row.get(0))?;
    collect_rows(rows)
}

fn phase_task_count(conn: &rusqlite::Connection, phase_id: i64) -> Result<i64> {
    conn.query_row(
        "select count(*) from work_phase_task_memberships where phase_id = ?1",
        params![phase_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_phase_tasks_with_status(
    conn: &rusqlite::Connection,
    phase_id: i64,
    status_a: &str,
    status_b: &str,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from tasks t
        join work_phase_task_memberships m on m.task_id = t.id
        where m.phase_id = ?1 and t.status in (?2, ?3)
        "#,
        params![phase_id, status_a, status_b],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_phase_tasks_not_open(conn: &rusqlite::Connection, phase_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from tasks t
        join work_phase_task_memberships m on m.task_id = t.id
        where m.phase_id = ?1
          and t.status not in ('open', 'blocked')
          and not exists (
              select 1
              from work_phase_trace_decisions d
              where d.phase_id = m.phase_id
                and d.record_type = 'task'
                and d.record_id = t.id
                and d.decision in ('carry', 'accept')
          )
        "#,
        params![phase_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_phase_design_versions(conn: &rusqlite::Connection, phase_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        select count(distinct dr.design_version_id)
        from task_derivations td
        join design_requirements dr on dr.id = td.design_requirement_id
        join work_phase_task_memberships m on m.task_id = td.task_id
        where m.phase_id = ?1 and td.status in ('active', 'stale')
        "#,
        params![phase_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_open_cross_phase_dependencies(conn: &rusqlite::Connection, phase_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from work_phase_dependencies
        where status = 'open'
          and (from_phase_id = ?1 or to_phase_id = ?1)
        "#,
        params![phase_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_open_phase_inbound_dependencies(
    conn: &rusqlite::Connection,
    phase_id: i64,
) -> Result<i64> {
    conn.query_row(
        "select count(*) from work_phase_dependencies where to_phase_id = ?1 and status = 'open'",
        params![phase_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_open_phase_checklist_items(conn: &rusqlite::Connection, phase_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from checklist_items ci
        join work_phase_task_memberships m on m.task_id = ci.task_id
        where m.phase_id = ?1 and ci.status in ('open', 'blocked')
        "#,
        params![phase_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_phase_validation_gate_blockers(conn: &rusqlite::Connection, phase_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from validation_gates vg
        join work_phase_task_memberships m on m.task_id = vg.task_id
        where m.phase_id = ?1
          and vg.status = 'active'
          and not exists (
              select 1
              from validation_runs vr
              where vr.validation_gate_id = vg.id
                and (
                    vr.result = 'pass'
                    or vr.acceptance_record_id is not null
                )
          )
        "#,
        params![phase_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_phase_missing_evidence(conn: &rusqlite::Connection, phase_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join work_phase_task_memberships m on m.task_id = td.task_id
        where m.phase_id = ?1
          and td.status = 'active'
          and not exists (
              select 1
              from implementation_evidence e
              where e.task_id = td.task_id
                and e.design_requirement_id = td.design_requirement_id
          )
          and not exists (
              select 1 from correction_completion_inheritance_sources inheritance
              join valid_completion_inheritance_sources valid on valid.id=inheritance.id
              where inheritance.current_requirement_id=td.design_requirement_id
                and inheritance.canonical_task_id=td.task_id
                and exists (
                    select 1 from correction_completion_inheritance_evidence mapped
                    where mapped.inheritance_source_id=inheritance.id
                      and mapped.evidence_kind='implementation_evidence'
                )
          )
        "#,
        params![phase_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_phase_missing_coverage(conn: &rusqlite::Connection, phase_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join work_phase_task_memberships m on m.task_id = td.task_id
        where m.phase_id = ?1
          and td.status = 'active'
          and not exists (
              select 1
              from coverage_items c
              where c.design_requirement_id = td.design_requirement_id
                and (
                    c.task_id = td.task_id
                    or (
                        c.task_id is null
                        and c.work_unit_id = (
                            select work_unit_id
                            from work_phases
                            where id = ?1
                        )
                    )
                )
                and c.status = 'covered'
          )
        "#,
        params![phase_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_incomplete_phase_reviews(conn: &rusqlite::Connection, phase_id: i64) -> Result<i64> {
    let mut stmt = conn.prepare(
        r#"
        select
            rp.id, rp.review_type, rp.stage, rp.design_version_id, rp.work_unit_id,
            coalesce(pol.required_consecutive_clean_fresh_runs, 1)
        from work_phase_review_targets pt
        join review_plans rp on rp.id = pt.review_plan_id
        left join review_policies pol on pol.id = rp.review_policy_id
        where pt.phase_id = ?1
          and rp.required = 1
          and not exists (
              select 1
              from acceptance_records ar
              where ar.target_type = 'review_plan'
                and ar.review_plan_id = rp.id
                and ar.status = 'approved'
                and ar.acceptance_type in ('explicit_exception', 'stale_accepted')
          )
        "#,
    )?;
    let rows = stmt.query_map(params![phase_id], |row| {
        Ok(PhaseReviewPlan {
            id: row.get(0)?,
            review_type: row.get(1)?,
            stage: row.get(2)?,
            design_version_id: row.get(3)?,
            work_unit_id: row.get(4)?,
            required_clean_fresh_runs: row.get(5)?,
        })
    })?;

    let mut incomplete = 0;
    for row in rows {
        let plan = row?;
        if !phase_review_plan_is_complete(conn, phase_id, &plan)? {
            incomplete += 1;
        }
    }
    Ok(incomplete)
}

fn phase_review_plan_is_complete(
    conn: &rusqlite::Connection,
    phase_id: i64,
    plan: &PhaseReviewPlan,
) -> Result<bool> {
    let required = plan.required_clean_fresh_runs.max(1);
    let expected_context = phase_review_context_kind_for_plan(&plan.stage, &plan.review_type)
        .and_then(|kind| {
            plan.design_version_id.map(|design_version_id| {
                review_context_ref_with_phase(
                    kind,
                    Some(design_version_id),
                    Some(plan.work_unit_id),
                    Some(phase_id),
                )
            })
        });
    let mut stmt = conn.prepare(
        r#"
        select r.clean_run, r.target_ref, r.review_provenance, r.review_provenance_ref,
               exists (
                   select 1
                   from review_agent_invocations i
                   where i.review_run_id = r.id
                     and i.external_agent_id is not null
                     and i.external_agent_id != ''
               )
        from review_runs r
        where r.review_plan_id = ?1
          and r.target_type = 'phase'
          and r.phase_id = ?2
          and r.run_type = 'fresh'
          and r.run_purpose = 'new_unbiased_review'
          and r.status = 'completed'
          and r.new_findings_count = 0
        order by r.id desc
        "#,
    )?;
    let rows = stmt.query_map(params![plan.id, phase_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, i64>(4)? == 1,
        ))
    })?;
    let mut clean_count = 0;
    for row in rows {
        let (clean_run, target_ref, provenance, provenance_ref, has_external_agent) = row?;
        let context_matches = expected_context
            .as_ref()
            .is_none_or(|expected| target_ref.as_deref() == Some(expected.as_str()));
        if clean_run == 1
            && context_matches
            && phase_review_provenance_is_trusted(
                &provenance,
                provenance_ref.as_deref(),
                has_external_agent,
            )
        {
            clean_count += 1;
            if clean_count >= required {
                return Ok(true);
            }
        } else {
            break;
        }
    }
    Ok(false)
}

fn phase_review_context_kind_for_plan(stage: &str, review_type: &str) -> Option<&'static str> {
    match (stage, review_type) {
        ("design-ready", "design_review") => Some("design-review"),
        ("implementation-ready", "design_task_decomposition") => Some("design-task-decomposition"),
        ("close-ready", "design_implementation_diff") => Some("design-implementation-diff"),
        ("close-ready", "implementation_review") => Some("implementation-review"),
        _ => None,
    }
}

fn phase_review_provenance_is_trusted(
    provenance: &str,
    provenance_ref: Option<&str>,
    has_external_agent: bool,
) -> bool {
    match provenance {
        "external_agent" => {
            has_external_agent && provenance_ref.is_some_and(|value| !value.trim().is_empty())
        }
        "human_review" => provenance_ref.is_some_and(|value| !value.trim().is_empty()),
        _ => false,
    }
}

fn validate_phase_key(key: &str) -> Result<()> {
    if key.trim().is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        bail!("phase key must use lowercase ascii letters, digits, '-' or '_'");
    }
    Ok(())
}

fn validate_phase_kind(kind: &str) -> Result<()> {
    if kind.trim().is_empty() {
        bail!("phase kind is required");
    }
    Ok(())
}

fn validate_dependency_type(dependency_type: &str) -> Result<()> {
    match dependency_type {
        "blocks" | "requires" => Ok(()),
        _ => bail!("phase dependency type must be blocks or requires"),
    }
}

fn validate_shared_record_policy(policy: &str) -> Result<()> {
    match policy {
        "require-decisions" | "carry-shared" => Ok(()),
        _ => bail!("shared record policy must be require-decisions or carry-shared"),
    }
}

fn validate_trace_record_type(record_type: &str) -> Result<()> {
    match record_type {
        "task"
        | "task_derivation"
        | "checklist_item"
        | "validation_gate"
        | "coverage_item"
        | "implementation_evidence"
        | "review_plan"
        | "rule_binding"
        | "work_record" => Ok(()),
        _ => bail!("phase trace record type is not supported"),
    }
}

fn validate_trace_decision(decision: &str) -> Result<()> {
    match decision {
        "split" | "carry" | "accept" => Ok(()),
        _ => bail!("phase trace decision must be split, carry, or accept"),
    }
}

fn collect_rows<T>(rows: impl Iterator<Item = rusqlite::Result<T>>) -> Result<Vec<T>> {
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

pub struct NewWorkPhase<'a> {
    pub work_unit_id: i64,
    pub design_version_id: Option<i64>,
    pub key: &'a str,
    pub title: &'a str,
    pub kind: &'a str,
    pub order: i64,
    pub reason: Option<&'a str>,
}

pub struct NewPhaseDependency<'a> {
    pub from_phase_id: i64,
    pub to_phase_id: i64,
    pub dependency_type: &'a str,
    pub reason: &'a str,
}

pub struct NewPhaseTraceDecision<'a> {
    pub phase_id: i64,
    pub record_type: &'a str,
    pub record_id: i64,
    pub decision: &'a str,
    pub reason: &'a str,
    pub authority_event_id: i64,
}

pub struct PhaseRescope<'a> {
    pub phase_id: i64,
    pub to_work_unit_id: Option<i64>,
    pub shared_record_policy: &'a str,
    pub dry_run: bool,
}

pub struct PhaseSplit<'a> {
    pub phase_id: i64,
    pub title: &'a str,
    pub reason: &'a str,
    pub shared_record_policy: &'a str,
    pub dry_run: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct WorkPhaseOutcome {
    pub phase_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseTaskOutcome {
    pub phase_id: i64,
    pub task_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseDependencyOutcome {
    pub dependency_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseTraceDecisionOutcome {
    pub decision_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseCloseOutcome {
    pub phase_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseAcceptanceOutcome {
    pub phase_id: i64,
    pub authority_event_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseReviewTargetOutcome {
    pub review_plan_target_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct WorkPhaseRecord {
    pub id: i64,
    pub work_unit_id: i64,
    pub phase_work_unit_id: Option<i64>,
    pub design_version_id: Option<i64>,
    pub key: String,
    pub title: String,
    pub kind: String,
    pub order: i64,
    pub status: String,
    pub task_count: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseDependencyRecord {
    pub id: i64,
    pub from_phase_id: i64,
    pub from_phase_key: String,
    pub to_phase_id: i64,
    pub to_phase_key: String,
    pub dependency_type: String,
    pub status: String,
    pub reason: String,
    pub evidence_ref: Option<String>,
    pub authority_event_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseTraceRecord {
    pub record_type: String,
    pub id: i64,
    pub status: String,
    pub label: String,
    pub decision: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseInventory {
    pub phase_id: i64,
    pub trace: Vec<PhaseTraceRecord>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseRescopeOutcome {
    pub phase_id: i64,
    pub source_work_unit_id: i64,
    pub target_work_unit_id: Option<i64>,
    pub result: String,
    pub inventory: Vec<String>,
    pub blockers: Vec<PhaseRescopeBlocker>,
    pub warnings: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseRescopeBlocker {
    pub kind: String,
    pub details: String,
    pub next_action: String,
}

impl PhaseRescopeBlocker {
    fn new(kind: &str, details: String, next_action: String) -> Self {
        Self {
            kind: kind.to_string(),
            details,
            next_action,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseCloseReadyOutcome {
    pub phase_id: i64,
    pub work_unit_id: Option<i64>,
    pub result: String,
    pub items: Vec<PhaseCloseReadyItem>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseCloseReadyItem {
    pub name: String,
    pub result: String,
    pub blocking_action: Option<String>,
    pub details: String,
}

impl PhaseCloseReadyItem {
    fn pass(name: &str, details: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            result: "pass".to_string(),
            blocking_action: None,
            details: details.into(),
        }
    }

    fn fail(name: &str, action: &str, details: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            result: "fail".to_string(),
            blocking_action: Some(action.to_string()),
            details: details.into(),
        }
    }
}

struct PhaseReviewPlan {
    id: i64,
    review_type: String,
    stage: String,
    design_version_id: Option<i64>,
    work_unit_id: i64,
    required_clean_fresh_runs: i64,
}

#[derive(Debug)]
struct StoredPhase {
    id: i64,
    work_unit_id: i64,
    phase_work_unit_id: Option<i64>,
    status: String,
}

struct SharedRecordRef {
    record_type: String,
    record_id: i64,
}
