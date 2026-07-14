use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::params;

use crate::db::{NewEvent, active_activation, insert_event, open_existing_project, project_id};

use super::{close_repository::*, close_trace::*, forking::*, resume_validation::*, *};

pub fn block_work(
    root: &Path,
    work_unit_id: Option<i64>,
    reason: &str,
) -> Result<WorkStatusOutcome> {
    update_work_unit_lifecycle(root, work_unit_id, "open", "blocked", "blocked", reason)
}

pub fn unblock_work(
    root: &Path,
    work_unit_id: Option<i64>,
    reason: &str,
) -> Result<WorkStatusOutcome> {
    update_work_unit_lifecycle(root, work_unit_id, "blocked", "open", "unblocked", reason)
}

pub fn abandon_work(
    root: &Path,
    work_unit_id: Option<i64>,
    reason: &str,
) -> Result<WorkStatusOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let target = resolve_lifecycle_work_unit(&tx, project_id, work_unit_id)?;
    ensure_work_mutation_allowed(
        &tx,
        "work abandon",
        Some((target.work_unit_id, "work abandon")),
    )?;
    let previous_status = target.status;
    if !matches!(previous_status.as_str(), "open" | "blocked" | "closed") {
        bail!("only open, blocked, or closed work units can be abandoned");
    }

    tx.execute(
        "update work_units set status = 'abandoned', closed_at = current_timestamp, close_summary = ?1 where id = ?2",
        params![reason, target.work_unit_id],
    )?;
    tx.execute(
        r#"
        update work_unit_activations
        set status = 'abandoned', completed_at = current_timestamp
        where work_unit_id = ?1 and status in ('active', 'suspended')
        "#,
        params![target.work_unit_id],
    )?;
    let event_id = insert_event(
        &tx,
        NewEvent {
            work_unit_id: target.work_unit_id,
            activation_id: target.activation_id,
            related_activation_id: None,
            event_type: "abandoned",
            reason: Some(reason),
            status_domain: "work_unit",
            previous_status: Some(&previous_status),
            next_status: Some("abandoned"),
        },
    )?;
    tx.execute(
        r#"
        update work_unit_dependencies
        set status = 'resolved', resolved_at = current_timestamp, resolved_by_work_unit_event_id = ?1
        where depends_on_work_unit_id = ?2 and status = 'open'
        "#,
        params![event_id, target.work_unit_id],
    )?;
    tx.commit()?;

    Ok(WorkStatusOutcome {
        work_unit_id: target.work_unit_id,
        activation_id: target.activation_id,
        previous_status,
        status: "abandoned".to_string(),
    })
}

pub fn suspend_work(root: &Path, reason: &str, next_action: &str) -> Result<SuspendOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let active = active_activation(&tx)?.context("no active activation to suspend")?;
    ensure_work_mutation_allowed(
        &tx,
        "work suspend",
        Some((active.work_unit_id, "work suspend")),
    )?;
    let snapshot_id = suspend_active_activation(&tx, &active, reason, next_action)?;
    tx.commit()?;

    Ok(SuspendOutcome {
        work_unit_id: active.work_unit_id,
        activation_id: active.activation_id,
        suspend_snapshot_id: snapshot_id,
    })
}

pub fn interrupt_work(root: &Path, title: &str, reason: &str) -> Result<InterruptOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    ensure_work_mutation_allowed(&tx, "work interrupt", None)?;
    let parent = active_activation(&tx)?.context("no active activation to interrupt")?;
    let next_action = format!("resume work unit {}", parent.work_unit_id);
    let parent_snapshot_id = suspend_active_activation(&tx, &parent, reason, &next_action)?;

    tx.execute(
        r#"
        insert into work_units(
            project_id, parent_work_unit_id, title, status, interrupt_reason, started_at
        )
        values (?1, ?2, ?3, 'open', ?4, current_timestamp)
        "#,
        params![project_id, parent.work_unit_id, title, reason],
    )?;
    let child_work_unit_id = tx.last_insert_rowid();

    tx.execute(
        r#"
        insert into work_unit_activations(
            project_id, work_unit_id, parent_activation_id, stack_depth, status,
            activation_reason, opened_at
        )
        values (?1, ?2, ?3, ?4, 'active', 'interrupt', current_timestamp)
        "#,
        params![
            project_id,
            child_work_unit_id,
            parent.activation_id,
            parent.stack_depth + 1
        ],
    )?;
    let child_activation_id = tx.last_insert_rowid();

    tx.execute(
        "update work_unit_activations set suspended_by_activation_id = ?1 where id = ?2",
        params![child_activation_id, parent.activation_id],
    )?;

    insert_event(
        &tx,
        NewEvent {
            work_unit_id: child_work_unit_id,
            activation_id: Some(child_activation_id),
            related_activation_id: Some(parent.activation_id),
            event_type: "opened",
            reason: Some(reason),
            status_domain: "work_unit",
            previous_status: None,
            next_status: Some("open"),
        },
    )?;

    tx.execute(
        r#"
        insert into work_unit_dependencies(
            work_unit_id, depends_on_work_unit_id, dependency_type, reason,
            status, created_at
        )
        values (?1, ?2, 'blocks', ?3, 'open', current_timestamp)
        "#,
        params![parent.work_unit_id, child_work_unit_id, reason],
    )?;

    tx.commit()?;

    Ok(InterruptOutcome {
        parent_work_unit_id: parent.work_unit_id,
        parent_activation_id: parent.activation_id,
        parent_suspend_snapshot_id: parent_snapshot_id,
        child_work_unit_id,
        child_activation_id,
    })
}

pub fn close_active_work(root: &Path, summary: &str, commit: Option<&str>) -> Result<CloseOutcome> {
    {
        let conn = open_existing_project(root)?;
        ensure_work_mutation_allowed(&conn, "work close", None)?;
    }
    let readiness = close_ready(root)?;
    if readiness.result != "pass" {
        let reason = readiness
            .blocking_reason
            .as_deref()
            .unwrap_or("close-ready checks failed");
        bail!("cannot close work unit; {reason}");
    }

    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let active = active_activation(&tx)?.context("no active activation to close")?;
    let open_tasks = tx.query_row(
        "select count(*) from tasks where work_unit_id = ?1 and status in ('open', 'blocked')",
        params![active.work_unit_id],
        |row| row.get::<_, i64>(0),
    )?;
    if open_tasks > 0 {
        bail!("cannot close work unit with open or blocked tasks");
    }
    let close_summary = match commit {
        Some(commit) => format!("{summary}\ncommit: {commit}"),
        None => summary.to_string(),
    };

    tx.execute(
        "update work_units set status = 'closed', closed_at = current_timestamp, close_summary = ?1 where id = ?2",
        params![close_summary, active.work_unit_id],
    )?;
    tx.execute(
        "update work_unit_activations set status = 'completed', completed_at = current_timestamp where work_unit_id = ?1 and status in ('active', 'suspended')",
        params![active.work_unit_id],
    )?;

    let reason = commit
        .map(|commit| format!("{summary}; commit {commit}"))
        .unwrap_or_else(|| summary.to_string());
    let event_id = insert_event(
        &tx,
        NewEvent {
            work_unit_id: active.work_unit_id,
            activation_id: Some(active.activation_id),
            related_activation_id: None,
            event_type: "closed",
            reason: Some(&reason),
            status_domain: "work_unit",
            previous_status: Some("open"),
            next_status: Some("closed"),
        },
    )?;

    tx.execute(
        r#"
        update work_unit_dependencies
        set status = 'resolved', resolved_at = current_timestamp, resolved_by_work_unit_event_id = ?1
        where depends_on_work_unit_id = ?2 and status = 'open'
        "#,
        params![event_id, active.work_unit_id],
    )?;

    tx.commit()?;

    Ok(CloseOutcome {
        work_unit_id: active.work_unit_id,
        activation_id: active.activation_id,
    })
}

pub fn close_ready(root: &Path) -> Result<CloseReadyOutcome> {
    let conn = open_existing_project(root)?;
    let Some(active) = active_activation(&conn)? else {
        return Ok(CloseReadyOutcome {
            work_unit_id: None,
            activation_id: None,
            result: "blocked".to_string(),
            blocking_reason: Some("no active work unit to close".to_string()),
            items: vec![CloseReadyItem::fail(
                "active_work_exists",
                "start or resume work before checking close readiness",
                "no active work unit to close",
            )],
        });
    };

    let open_tasks = conn.query_row(
        "select count(*) from tasks where work_unit_id = ?1 and status in ('open', 'blocked')",
        params![active.work_unit_id],
        |row| row.get::<_, i64>(0),
    )?;
    let validation = validation_close_state(&conn, active.work_unit_id)?;
    let repository = repository_close_state(&conn, &active)?;
    let review = review_plan_stage_state(&conn, active.work_unit_id, "close-ready")?;
    let trace = close_trace_state(&conn, active.work_unit_id)?;
    let process = close_process_state(&conn, active.project_id, active.work_unit_id)?;
    let missing_close_review_types =
        missing_required_close_review_types(&conn, active.work_unit_id)?;
    let missing_selected_gate_details =
        missing_selected_gate_details_for_work(&conn, active.work_unit_id)?;
    let validation_gate_blocker_details =
        validation_gate_blocker_details_for_work(&conn, active.work_unit_id)?;
    let review_plan_blocker_details =
        review_plan_blocker_details_for_stage(&conn, active.work_unit_id, "close-ready")?;
    let mut items = Vec::new();
    items.push(if open_tasks == 0 {
        CloseReadyItem::pass(
            "open_tasks_closed",
            format!("{open_tasks} open or blocked tasks"),
        )
    } else {
        CloseReadyItem::fail(
            "open_tasks_closed",
            "close or accept all open tasks before closing work",
            format!("{open_tasks} open or blocked tasks"),
        )
    });
    items.push(if process.rule_conflict_count == 0 {
        CloseReadyItem::pass(
            "rules_checked",
            format!(
                "{} applicable rules, {} shadowed conflicts",
                process.applicable_rule_count, process.rule_conflict_count
            ),
        )
    } else {
        CloseReadyItem::fail(
            "rules_checked",
            "resolve or accept shadowed rule conflicts before closing work",
            format!(
                "{} applicable rules, {} shadowed conflicts",
                process.applicable_rule_count, process.rule_conflict_count
            ),
        )
    });
    items.push(if process.missing_fixed_command_usage_count == 0 {
        CloseReadyItem::pass(
            "fixed_commands_used",
            format!(
                "{} fixed command profiles, {} missing usage or approved deviation",
                process.fixed_command_count, process.missing_fixed_command_usage_count
            ),
        )
    } else {
        CloseReadyItem::fail(
            "fixed_commands_used",
            "record fixed command usage or approve a command deviation before closing work",
            format!(
                "{} fixed command profiles, {} missing usage or approved deviation",
                process.fixed_command_count, process.missing_fixed_command_usage_count
            ),
        )
    });
    items.push(if process.invalid_commit_message_count == 0 {
        CloseReadyItem::pass(
            "commit_messages_checked",
            format!(
                "{} invalid linked commit messages",
                process.invalid_commit_message_count
            ),
        )
    } else {
        CloseReadyItem::fail(
            "commit_messages_checked",
            "link only commits with prefix messages and without forbidden internal terms",
            format!(
                "{} invalid linked commit messages",
                process.invalid_commit_message_count
            ),
        )
    });
    items.push(
        if process.repeated_correction_count < 2 || process.open_kpt_review_count > 0 {
            CloseReadyItem::pass(
                "corrections_kpt_checked",
                format!(
                    "{} active corrections, {} open KPT reviews",
                    process.repeated_correction_count, process.open_kpt_review_count
                ),
            )
        } else {
            CloseReadyItem::fail(
                "corrections_kpt_checked",
                "open or record a KPT review for repeated active corrections",
                format!(
                    "{} active corrections, {} open KPT reviews",
                    process.repeated_correction_count, process.open_kpt_review_count
                ),
            )
        },
    );
    items.push(if process.work_record_count > 0 {
        CloseReadyItem::pass(
            "work_record_recorded",
            format!(
                "{} work records, {} linked evidence rows",
                process.work_record_count, process.work_record_evidence_link_count
            ),
        )
    } else {
        CloseReadyItem::fail(
            "work_record_recorded",
            "create a work record before closing work",
            "no work record exists for the active work unit",
        )
    });
    items.push(
        if trace.missing_validation_gate_count == 0
            && validation.missing_run_count == 0
            && validation.unaccepted_failure_count == 0
        {
            CloseReadyItem::pass(
                "validation_runs_recorded",
                format!(
                    "{} selected gates, {} missing selected gates, {} missing runs, {} accepted failures, {} unaccepted failures",
                    validation.selected_gate_count,
                    trace.missing_validation_gate_count,
                    validation.missing_run_count,
                    validation.accepted_failure_count,
                    validation.unaccepted_failure_count
                ),
            )
        } else {
            let details = append_detail_list(
                format!(
                    "{} selected gates, {} missing selected gates, {} missing runs, {} accepted failures, {} unaccepted failures",
                    validation.selected_gate_count,
                    trace.missing_validation_gate_count,
                    validation.missing_run_count,
                    validation.accepted_failure_count,
                    validation.unaccepted_failure_count
                ),
                "missing selected gate derivations",
                &missing_selected_gate_details,
            );
            let details = append_detail_list(
                details,
                "validation gate run blockers",
                &validation_gate_blocker_details,
            );
            CloseReadyItem::fail(
                "validation_runs_recorded",
                "record passing validation runs or classify the remaining failures",
                details,
            )
        },
    );
    items.push(
        if repository.repository_count > 0
            && repository.missing_snapshot_count == 0
                && repository.unclassified_dirty_state_count == 0
                && repository.missing_comparison_count == 0
                && repository.unclassified_comparison_count == 0
        {
            CloseReadyItem::pass(
                "repository_state_recorded",
                format!(
                    "{} repositories, {} missing active snapshots, {} unclassified dirty states, {} missing comparisons, {} unclassified comparisons",
                    repository.repository_count,
                    repository.missing_snapshot_count,
                    repository.unclassified_dirty_state_count,
                    repository.missing_comparison_count,
                    repository.unclassified_comparison_count
                ),
            )
        } else {
            CloseReadyItem::fail(
                "repository_state_recorded",
                "record active repository snapshots and classify dirty state before closing work",
                format!(
                    "{} repositories, {} missing active snapshots, {} unclassified dirty states, {} missing comparisons, {} unclassified comparisons",
                    repository.repository_count,
                    repository.missing_snapshot_count,
                    repository.unclassified_dirty_state_count,
                    repository.missing_comparison_count,
                    repository.unclassified_comparison_count
                ),
            )
        },
    );
    items.push(
        if trace.missing_evidence_count == 0
            && trace.missing_coverage_count == 0
            && trace.missing_requirement_coverage_count == 0
            && trace.open_checklist_item_count == 0
            && trace.active_checklist_count == 0
        {
            CloseReadyItem::pass(
                "design_trace_closed",
                format!(
                    "{} active requirements, {} design-derived tasks, {} missing evidence, {} missing task coverage, {} missing requirement coverage, {} open checklist items, {} active checklists",
                    trace.active_requirement_count,
                    trace.derived_task_count,
                    trace.missing_evidence_count,
                    trace.missing_coverage_count,
                    trace.missing_requirement_coverage_count,
                    trace.open_checklist_item_count,
                    trace.active_checklist_count
                ),
            )
        } else {
            CloseReadyItem::fail(
                "design_trace_closed",
                "record implementation evidence and coverage, then close checklist items and checklists",
                format!(
                    "{} active requirements, {} design-derived tasks, {} missing evidence, {} missing task coverage, {} missing requirement coverage, {} open checklist items, {} active checklists",
                    trace.active_requirement_count,
                    trace.derived_task_count,
                    trace.missing_evidence_count,
                    trace.missing_coverage_count,
                    trace.missing_requirement_coverage_count,
                    trace.open_checklist_item_count,
                    trace.active_checklist_count
                ),
            )
        },
    );
    items.push(
        if trace.derived_task_count > 0 && !missing_close_review_types.is_empty() {
            let details = append_detail_list(
                format!(
                    "{} required close-ready plans, {} incomplete, {} stale targets, {} missing review-context runs, missing types: {}",
                    review.required_plan_count,
                    review.incomplete_required_plan_count,
                    review.stale_target_count,
                    review.missing_context_run_count,
                    missing_close_review_types.join(", ")
                ),
                "review plan blockers",
                &review_plan_blocker_details,
            );
            CloseReadyItem::fail(
                "review_plans_clean",
                "add required close-ready review plans for design-derived work",
                details,
            )
        } else if review.incomplete_required_plan_count == 0
            && review.stale_target_count == 0
            && review.missing_context_run_count == 0
        {
            CloseReadyItem::pass(
                "review_plans_clean",
                format!(
                    "{} required close-ready plans, {} incomplete, {} stale targets, {} missing review-context runs",
                    review.required_plan_count,
                    review.incomplete_required_plan_count,
                    review.stale_target_count,
                    review.missing_context_run_count
                ),
            )
        } else {
            let details = append_detail_list(
                format!(
                    "{} required close-ready plans, {} incomplete, {} stale targets, {} missing review-context runs",
                    review.required_plan_count,
                    review.incomplete_required_plan_count,
                    review.stale_target_count,
                    review.missing_context_run_count
                ),
                "review plan blockers",
                &review_plan_blocker_details,
            );
            CloseReadyItem::fail(
                "review_plans_clean",
                "complete required close-ready plans, refresh stale targets, or waive an approved exception with review plan waive",
                details,
            )
        },
    );

    let blocking_reason = items
        .iter()
        .find_map(|item| item.blocking_action.clone())
        .map(|_| "close-ready checks failed".to_string());
    Ok(CloseReadyOutcome {
        work_unit_id: Some(active.work_unit_id),
        activation_id: Some(active.activation_id),
        result: if blocking_reason.is_none() {
            "pass"
        } else {
            "blocked"
        }
        .to_string(),
        blocking_reason,
        items,
    })
}
