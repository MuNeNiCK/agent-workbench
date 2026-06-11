use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use crate::db::{
    NewEvent, StoredActivation, active_activation, insert_event, max_id, open_existing_project,
    project_id, suspend_snapshot, suspended_activation,
};
use crate::review_context::review_plan_has_clean_context_run;
use crate::rules::{RuleBindingInput, insert_rule_binding};
use crate::traceability::{ImplementationReadyCheck, implementation_ready};

pub fn start_work(root: &Path, title: &str, responsibility: Option<&str>) -> Result<WorkOutcome> {
    start_work_with_options(
        root,
        WorkStart {
            title,
            responsibility,
            design_version_id: None,
        },
    )
}

pub fn start_work_with_options(root: &Path, input: WorkStart<'_>) -> Result<WorkOutcome> {
    if let Some(design_version_id) = input.design_version_id {
        let ready = implementation_ready(
            root,
            ImplementationReadyCheck {
                design_version_id: Some(design_version_id),
            },
        )?;
        if ready.result != "pass" {
            bail!("implementation work start requires implementation-ready to pass");
        }
    }
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;

    if active_activation(&tx)?.is_some() {
        bail!("cannot start work while another activation is active");
    }

    tx.execute(
        r#"
        insert into work_units(project_id, title, status, responsibility, started_at)
        values (?1, ?2, 'open', ?3, current_timestamp)
        "#,
        params![project_id, input.title, input.responsibility],
    )?;
    let work_unit_id = tx.last_insert_rowid();
    let work_scope = work_unit_id.to_string();
    if input.responsibility.is_some() {
        insert_rule_binding(
            &tx,
            RuleBindingInput {
                project_id,
                rule_source_type: "work_unit",
                authority_event_id: None,
                user_correction_id: None,
                command_profile_id: None,
                review_policy_id: None,
                review_plan_id: None,
                work_unit_id: Some(work_unit_id),
                validation_gate_id: None,
                acceptance_record_id: None,
                scope_type: "work_unit",
                scope_key: Some(&work_scope),
                precedence: 60,
            },
        )?;
    }

    tx.execute(
        r#"
        insert into work_unit_activations(
            project_id, work_unit_id, stack_depth, status, activation_reason, opened_at
        )
        values (?1, ?2, 0, 'active', 'start', current_timestamp)
        "#,
        params![project_id, work_unit_id],
    )?;
    let activation_id = tx.last_insert_rowid();

    insert_event(
        &tx,
        NewEvent {
            work_unit_id,
            activation_id: Some(activation_id),
            related_activation_id: None,
            event_type: "opened",
            reason: input.responsibility,
            status_domain: "work_unit",
            previous_status: None,
            next_status: Some("open"),
        },
    )?;

    tx.commit()?;

    Ok(WorkOutcome {
        work_unit_id,
        activation_id,
    })
}

pub fn activate_work(root: &Path, input: WorkActivate<'_>) -> Result<WorkOutcome> {
    if let Some(design_version_id) = input.design_version_id {
        let ready = implementation_ready(
            root,
            ImplementationReadyCheck {
                design_version_id: Some(design_version_id),
            },
        )?;
        if ready.result != "pass" {
            bail!("implementation work activation requires implementation-ready to pass");
        }
    }
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;

    if active_activation(&tx)?.is_some() {
        bail!("cannot activate work while another activation is active");
    }
    if suspended_activation(&tx)?.is_some() {
        bail!(
            "cannot activate open work while a suspended activation exists; run resume-check and work resume"
        );
    }

    tx.query_row(
        "select 1 from work_units where id = ?1 and project_id = ?2 and status = 'open'",
        params![input.work_unit_id, project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("open work unit not found")?;

    let prior_activation_count: i64 = tx.query_row(
        "select count(*) from work_unit_activations where work_unit_id = ?1",
        params![input.work_unit_id],
        |row| row.get(0),
    )?;
    if prior_activation_count > 0 {
        bail!("work unit already has activation history; use resume or reopen flow");
    }

    tx.execute(
        r#"
        insert into work_unit_activations(
            project_id, work_unit_id, stack_depth, status, activation_reason, opened_at
        )
        values (?1, ?2, 0, 'active', 'start', current_timestamp)
        "#,
        params![project_id, input.work_unit_id],
    )?;
    let activation_id = tx.last_insert_rowid();

    insert_event(
        &tx,
        NewEvent {
            work_unit_id: input.work_unit_id,
            activation_id: Some(activation_id),
            related_activation_id: None,
            event_type: "opened",
            reason: input.reason,
            status_domain: "activation",
            previous_status: None,
            next_status: Some("active"),
        },
    )?;
    tx.commit()?;

    Ok(WorkOutcome {
        work_unit_id: input.work_unit_id,
        activation_id,
    })
}

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
        "update work_unit_activations set status = 'completed', completed_at = current_timestamp where id = ?1",
        params![active.activation_id],
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
            && (trace.derived_task_count == 0 || validation.selected_gate_count > 0)
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
            CloseReadyItem::fail(
                "validation_runs_recorded",
                "record passing validation runs or classify the remaining failures",
                format!(
                    "{} selected gates, {} missing selected gates, {} missing runs, {} accepted failures, {} unaccepted failures",
                    validation.selected_gate_count,
                    trace.missing_validation_gate_count,
                    validation.missing_run_count,
                    validation.accepted_failure_count,
                    validation.unaccepted_failure_count
                ),
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
            CloseReadyItem::fail(
                "review_plans_clean",
                "add required close-ready review plans for design-derived work",
                format!(
                    "{} required close-ready plans, {} incomplete, {} stale targets, {} missing review-context runs, missing types: {}",
                    review.required_plan_count,
                    review.incomplete_required_plan_count,
                    review.stale_target_count,
                    review.missing_context_run_count,
                    missing_close_review_types.join(", ")
                ),
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
            CloseReadyItem::fail(
                "review_plans_clean",
                "complete required close-ready plans, refresh stale targets, or waive an approved exception with review plan waive",
                format!(
                    "{} required close-ready plans, {} incomplete, {} stale targets, {} missing review-context runs",
                    review.required_plan_count,
                    review.incomplete_required_plan_count,
                    review.stale_target_count,
                    review.missing_context_run_count
                ),
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

pub fn resume_check_basic(root: &Path) -> Result<ResumeCheckOutcome> {
    resume_check(root, "basic")
}

pub fn resume_check(root: &Path, maturity: &str) -> Result<ResumeCheckOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let evaluation = evaluate_resume_ready(&tx, maturity)?;

    tx.execute(
        r#"
        insert into resume_checks(
            work_unit_id, work_unit_activation_id, suspend_snapshot_id, maturity,
            status, result, authority_event_high_watermark, activation_stack_revision,
            repository_snapshot_id, repository_state_revision, allowed_next_action,
            blocking_reason, created_at
        )
        values (?1, ?2, ?3, ?4, 'pending', ?5, ?6, ?7, ?8, ?9, ?10, ?11, current_timestamp)
        "#,
        params![
            evaluation.work_unit_id,
            evaluation.activation_id,
            evaluation.suspend_snapshot_id,
            maturity,
            evaluation.resume_result,
            evaluation.authority_high_watermark,
            evaluation.activation_stack_revision,
            evaluation.repository_snapshot_id,
            evaluation.repository_state_revision,
            if evaluation.resume_result == "allowed" {
                evaluation.allowed_next_action.as_deref()
            } else {
                None
            },
            evaluation.blocking_reason.as_deref(),
        ],
    )?;
    let resume_check_id = tx.last_insert_rowid();

    for item in &evaluation.items {
        tx.execute(
            r#"
            insert into resume_check_items(
                resume_check_id, check_name, result, blocking_action, details
            )
            values (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                resume_check_id,
                item.name,
                item.result,
                item.blocking_action.as_deref(),
                item.details,
            ],
        )?;
    }

    tx.commit()?;

    Ok(ResumeCheckOutcome {
        resume_check_id,
        result: evaluation.resume_result,
        blocking_reason: evaluation.blocking_reason,
    })
}

pub fn resume_ready_basic(root: &Path) -> Result<ResumeReadyOutcome> {
    resume_ready(root, "basic")
}

pub fn resume_ready(root: &Path, maturity: &str) -> Result<ResumeReadyOutcome> {
    let conn = open_existing_project(root)?;
    match evaluate_resume_ready(&conn, maturity) {
        Ok(evaluation) => Ok(ResumeReadyOutcome {
            work_unit_id: Some(evaluation.work_unit_id),
            activation_id: Some(evaluation.activation_id),
            result: gate_result_for(&evaluation),
            blocking_reason: evaluation.blocking_reason,
            items: evaluation.items,
        }),
        Err(error) if is_no_resume_target_error(&error) => Ok(ResumeReadyOutcome {
            work_unit_id: None,
            activation_id: None,
            result: "blocked".to_string(),
            blocking_reason: Some("no suspended activation to resume".to_string()),
            items: vec![ResumeReadyItem {
                name: "resume_target_suspended".to_string(),
                result: "fail".to_string(),
                blocking_action: Some(
                    "suspend or complete current work before resuming".to_string(),
                ),
                details: "no suspended activation to resume".to_string(),
            }],
        }),
        Err(error) => Err(error),
    }
}

fn gate_result_for(evaluation: &ResumeGateEvaluation) -> String {
    if evaluation.resume_result == "allowed" {
        "pass".to_string()
    } else {
        "blocked".to_string()
    }
}

fn ensure_resume_check_items_pass(
    conn: &Connection,
    resume_check_id: i64,
    maturity: &str,
) -> Result<()> {
    for item_name in required_resume_check_items(maturity)? {
        let result = conn
            .query_row(
                r#"
                select result
                from resume_check_items
                where resume_check_id = ?1 and check_name = ?2
                order by id desc
                limit 1
                "#,
                params![resume_check_id, item_name],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .with_context(|| format!("resume check is missing required item {item_name}"))?;
        if result != "pass" {
            bail!("resume check item {item_name} is not pass");
        }
    }
    Ok(())
}

fn required_resume_check_items(maturity: &str) -> Result<Vec<&'static str>> {
    let mut items = vec![
        "resume_target_suspended",
        "snapshot_exists",
        "suspend_reason_exists",
        "next_action_exists",
        "deeper_frames_closed",
        "blocking_dependencies_clear",
    ];
    match maturity {
        "basic" => {}
        "trace-aware" => items.extend([
            "active_tasks_current",
            "authority_refs_current",
            "review_scope_refs_current",
            "design_version_current",
            "task_derivation_current",
            "checklist_current",
            "selected_gate_current",
            "review_plan_current",
            "open_findings_current",
        ]),
        "repo-aware" => items.extend([
            "active_tasks_current",
            "authority_refs_current",
            "review_scope_refs_current",
            "design_version_current",
            "task_derivation_current",
            "checklist_current",
            "selected_gate_current",
            "review_plan_current",
            "open_findings_current",
            "repository_heads_current",
            "repository_state_current",
            "assumptions_current",
        ]),
        _ => bail!("unsupported maturity; use basic, trace-aware, or repo-aware"),
    }
    Ok(items)
}

fn is_no_resume_target_error(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.to_string() == "no suspended activation to resume")
}

pub fn resume_work(root: &Path, resume_check_id: i64) -> Result<ResumeOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;

    let check = tx
        .query_row(
            r#"
            select id, work_unit_id, work_unit_activation_id, result, status,
                   authority_event_high_watermark, activation_stack_revision,
                   maturity, repository_snapshot_id, repository_state_revision
            from resume_checks
            where id = ?1
            "#,
            params![resume_check_id],
            |row| {
                Ok(StoredResumeCheck {
                    id: row.get(0)?,
                    work_unit_id: row.get(1)?,
                    activation_id: row.get(2)?,
                    result: row.get(3)?,
                    status: row.get(4)?,
                    authority_event_high_watermark: row.get(5)?,
                    activation_stack_revision: row.get(6)?,
                    maturity: row.get(7)?,
                    repository_snapshot_id: row.get(8)?,
                    repository_state_revision: row.get(9)?,
                })
            },
        )
        .optional()?
        .context("resume check not found")?;

    if check.status != "pending" || check.result != "allowed" {
        bail!("resume check must be pending and allowed");
    }
    ensure_resume_check_items_pass(&tx, check.id, &check.maturity)?;
    if active_activation(&tx)?.is_some() {
        bail!("cannot resume while another activation is active");
    }
    let repository_snapshot_changed = match (check.maturity.as_str(), check.repository_snapshot_id)
    {
        ("repo-aware", Some(repository_snapshot_id)) => {
            max_id(&tx, "repository_snapshots")? != repository_snapshot_id
        }
        _ => false,
    };
    let repository_state_changed = match (check.maturity.as_str(), check.repository_state_revision)
    {
        ("repo-aware", Some(repository_state_revision)) => {
            repository_state_revision_for_resume(&tx)? != repository_state_revision
        }
        _ => false,
    };
    if max_id(&tx, "authority_events")? != check.authority_event_high_watermark.unwrap_or(0)
        || max_id(&tx, "work_unit_events")? != check.activation_stack_revision.unwrap_or(0)
        || repository_snapshot_changed
        || repository_state_changed
    {
        tx.execute(
            "update resume_checks set status = 'stale' where id = ?1",
            params![check.id],
        )?;
        tx.commit()?;
        bail!("resume check is stale");
    }

    let status: String = tx.query_row(
        "select status from work_unit_activations where id = ?1",
        params![check.activation_id],
        |row| row.get(0),
    )?;
    if status != "suspended" {
        bail!("resume target activation is not suspended");
    }

    tx.execute(
        "update work_unit_activations set status = 'active' where id = ?1",
        params![check.activation_id],
    )?;
    let event_id = insert_event(
        &tx,
        NewEvent {
            work_unit_id: check.work_unit_id,
            activation_id: Some(check.activation_id),
            related_activation_id: None,
            event_type: "resumed",
            reason: Some("resume check allowed"),
            status_domain: "activation",
            previous_status: Some("suspended"),
            next_status: Some("active"),
        },
    )?;
    tx.execute(
        "update resume_checks set status = 'consumed', consumed_at = current_timestamp, consumed_by_work_unit_event_id = ?1 where id = ?2",
        params![event_id, check.id],
    )?;
    tx.commit()?;

    Ok(ResumeOutcome {
        work_unit_id: check.work_unit_id,
        activation_id: check.activation_id,
    })
}

pub fn reopen_work(root: &Path, input: WorkReopen<'_>) -> Result<WorkOutcome> {
    validate_reopen_reason_type(input.reason_type)?;
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    ensure_reopen_authority(
        &tx,
        project_id,
        input.authority_event_id,
        input.acceptance_record_id,
    )?;

    let status = tx
        .query_row(
            "select status from work_units where id = ?1 and project_id = ?2",
            params![input.work_unit_id, project_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .context("work unit not found")?;
    if status != "closed" && status != "abandoned" {
        bail!("only closed or abandoned work units can be reopened");
    }

    let parent = prepare_parent_frame(
        &tx,
        input.reason,
        &format!("resume after reopening work unit {}", input.work_unit_id),
    )?;

    tx.execute(
        "update work_units set status = 'open', closed_at = null where id = ?1",
        params![input.work_unit_id],
    )?;
    tx.execute(
        r#"
        insert into work_unit_activations(
            project_id, work_unit_id, parent_activation_id, stack_depth, status,
            activation_reason, opened_at
        )
        values (?1, ?2, ?3, ?4, 'active', 'reopen', current_timestamp)
        "#,
        params![
            project_id,
            input.work_unit_id,
            parent.as_ref().map(|activation| activation.activation_id),
            parent
                .as_ref()
                .map(|activation| activation.stack_depth + 1)
                .unwrap_or(0)
        ],
    )?;
    let activation_id = tx.last_insert_rowid();
    if let Some(parent) = &parent {
        tx.execute(
            "update work_unit_activations set suspended_by_activation_id = ?1 where id = ?2",
            params![activation_id, parent.activation_id],
        )?;
    }
    insert_event(
        &tx,
        NewEvent {
            work_unit_id: input.work_unit_id,
            activation_id: Some(activation_id),
            related_activation_id: parent.as_ref().map(|activation| activation.activation_id),
            event_type: "reopened",
            reason: Some(input.reason),
            status_domain: "work_unit",
            previous_status: Some(&status),
            next_status: Some("open"),
        },
    )?;
    tx.execute(
        r#"
        insert into work_unit_dependencies(
            work_unit_id, depends_on_work_unit_id, dependency_type, reason,
            status, created_at
        )
        values (?1, ?1, 'invalidates_closure', ?2, 'open', current_timestamp)
        "#,
        params![input.work_unit_id, input.reason],
    )?;
    if let Some(parent) = &parent {
        tx.execute(
            r#"
            insert into work_unit_dependencies(
                work_unit_id, depends_on_work_unit_id, dependency_type, reason,
                status, created_at
            )
            values (?1, ?2, 'blocks', ?3, 'open', current_timestamp)
            "#,
            params![parent.work_unit_id, input.work_unit_id, input.reason],
        )?;
    }
    tx.commit()?;

    Ok(WorkOutcome {
        work_unit_id: input.work_unit_id,
        activation_id,
    })
}

pub fn create_follow_up_work(
    root: &Path,
    source_work_unit_id: i64,
    title: &str,
    reason: &str,
) -> Result<FollowUpOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;

    let source_status = tx
        .query_row(
            "select status from work_units where id = ?1 and project_id = ?2",
            params![source_work_unit_id, project_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .context("source work unit not found")?;
    if source_status != "closed" && source_status != "abandoned" {
        bail!("follow-up source must be closed or abandoned");
    }

    let parent = prepare_parent_frame(
        &tx,
        reason,
        &format!("resume after follow-up for work unit {source_work_unit_id}"),
    )?;

    tx.execute(
        r#"
        insert into work_units(
            project_id, parent_work_unit_id, title, status, responsibility,
            interrupt_reason, started_at
        )
        values (?1, ?2, ?3, 'open', 'follow-up work', ?4, current_timestamp)
        "#,
        params![project_id, source_work_unit_id, title, reason],
    )?;
    let follow_up_work_unit_id = tx.last_insert_rowid();
    tx.execute(
        r#"
        insert into work_unit_activations(
            project_id, work_unit_id, parent_activation_id, stack_depth, status,
            activation_reason, opened_at
        )
        values (?1, ?2, ?3, ?4, 'active', 'follow_up', current_timestamp)
        "#,
        params![
            project_id,
            follow_up_work_unit_id,
            parent.as_ref().map(|activation| activation.activation_id),
            parent
                .as_ref()
                .map(|activation| activation.stack_depth + 1)
                .unwrap_or(0)
        ],
    )?;
    let activation_id = tx.last_insert_rowid();
    if let Some(parent) = &parent {
        tx.execute(
            "update work_unit_activations set suspended_by_activation_id = ?1 where id = ?2",
            params![activation_id, parent.activation_id],
        )?;
    }
    insert_event(
        &tx,
        NewEvent {
            work_unit_id: source_work_unit_id,
            activation_id: None,
            related_activation_id: Some(activation_id),
            event_type: "follow_up_created",
            reason: Some(reason),
            status_domain: "work_unit",
            previous_status: Some(&source_status),
            next_status: Some(&source_status),
        },
    )?;
    insert_event(
        &tx,
        NewEvent {
            work_unit_id: follow_up_work_unit_id,
            activation_id: Some(activation_id),
            related_activation_id: parent.as_ref().map(|activation| activation.activation_id),
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
        values (?1, ?2, 'follow_up_of', ?3, 'resolved', current_timestamp)
        "#,
        params![follow_up_work_unit_id, source_work_unit_id, reason],
    )?;
    if let Some(parent) = &parent {
        tx.execute(
            r#"
            insert into work_unit_dependencies(
                work_unit_id, depends_on_work_unit_id, dependency_type, reason,
                status, created_at
            )
            values (?1, ?2, 'blocks', ?3, 'open', current_timestamp)
            "#,
            params![parent.work_unit_id, follow_up_work_unit_id, reason],
        )?;
    }
    tx.commit()?;

    Ok(FollowUpOutcome {
        source_work_unit_id,
        work_unit_id: follow_up_work_unit_id,
        activation_id,
    })
}

fn validate_reopen_reason_type(reason_type: &str) -> Result<()> {
    match reason_type {
        "closure_invalid" | "closure_incomplete" | "authority_superseded" => Ok(()),
        _ => bail!(
            "reopen reason type must be closure_invalid, closure_incomplete, or authority_superseded"
        ),
    }
}

fn ensure_reopen_authority(
    conn: &Connection,
    project_id: i64,
    authority_event_id: Option<i64>,
    acceptance_record_id: Option<i64>,
) -> Result<()> {
    match (authority_event_id, acceptance_record_id) {
        (Some(_), Some(_)) | (None, None) => {
            bail!("work reopen requires exactly one of --authority or --acceptance")
        }
        (Some(authority_event_id), None) => {
            let allowed: bool = conn.query_row(
                r#"
                select exists(
                    select 1
                    from authority_events
                    where id = ?1
                      and project_id = ?2
                      and status = 'active'
                      and event_type in ('user_instruction', 'policy', 'design_doc')
                )
                "#,
                params![authority_event_id, project_id],
                |row| row.get(0),
            )?;
            if !allowed {
                bail!("work reopen requires active user, policy, or design authority");
            }
        }
        (None, Some(acceptance_record_id)) => {
            let allowed: bool = conn.query_row(
                r#"
                select exists(
                    select 1
                    from acceptance_records
                    where id = ?1
                      and project_id = ?2
                      and status = 'approved'
                      and acceptance_type in ('explicit_exception', 'stale_accepted')
                )
                "#,
                params![acceptance_record_id, project_id],
                |row| row.get(0),
            )?;
            if !allowed {
                bail!("work reopen requires an approved acceptance record");
            }
        }
    }
    Ok(())
}

pub fn fork_work(root: &Path, input: NewWorkFork<'_>) -> Result<WorkForkOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;

    if active_activation(&tx)?.is_some() {
        bail!("cannot fork work while another activation is active");
    }
    if input.discard_policy != "keep_history" {
        bail!("only --discard-policy keep_history is implemented currently");
    }

    let source = resolve_fork_source(&tx, project_id, input.source)?;
    let fork_reason = fork_reason_code(input.reason);
    tx.execute(
        r#"
        insert into work_units(
            project_id, parent_work_unit_id, title, status, responsibility,
            interrupt_reason, started_at
        )
        values (?1, ?2, ?3, 'open', ?4, ?5, current_timestamp)
        "#,
        params![
            project_id,
            source.source_work_unit_id,
            input.title,
            Some("forked work"),
            input.reason,
        ],
    )?;
    let forked_work_unit_id = tx.last_insert_rowid();

    tx.execute(
        r#"
        insert into work_unit_activations(
            project_id, work_unit_id, stack_depth, status, activation_reason, opened_at
        )
        values (?1, ?2, 0, 'active', 'start', current_timestamp)
        "#,
        params![project_id, forked_work_unit_id],
    )?;
    let activation_id = tx.last_insert_rowid();

    insert_event(
        &tx,
        NewEvent {
            work_unit_id: forked_work_unit_id,
            activation_id: Some(activation_id),
            related_activation_id: source.source_work_unit_activation_id,
            event_type: "opened",
            reason: Some(input.reason),
            status_domain: "work_unit",
            previous_status: None,
            next_status: Some("open"),
        },
    )?;

    tx.execute(
        r#"
        insert into work_record_forks(
            project_id, source_work_unit_id, source_work_unit_activation_id,
            source_work_record_id, source_repository_snapshot_id,
            source_git_commit_id, source_git_commit_sha, forked_work_unit_id,
            fork_reason, discard_policy, status, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'open', current_timestamp)
        "#,
        params![
            project_id,
            source.source_work_unit_id,
            source.source_work_unit_activation_id,
            source.source_work_record_id,
            source.source_repository_snapshot_id,
            source.source_git_commit_id,
            source.source_git_commit_sha,
            forked_work_unit_id,
            fork_reason,
            input.discard_policy,
        ],
    )?;
    let fork_id = tx.last_insert_rowid();
    if let Some(source_work_unit_id) = source.source_work_unit_id {
        tx.execute(
            r#"
            insert into work_unit_dependencies(
                work_unit_id, depends_on_work_unit_id, dependency_type, reason,
                status, created_at
            )
            values (?1, ?2, ?3, ?4, 'open', current_timestamp)
            "#,
            params![
                forked_work_unit_id,
                source_work_unit_id,
                dependency_type_for_fork_reason(fork_reason),
                input.reason,
            ],
        )?;
    }
    tx.commit()?;

    Ok(WorkForkOutcome {
        fork_id,
        work_unit_id: forked_work_unit_id,
        activation_id,
    })
}

fn suspend_active_activation(
    conn: &Connection,
    active: &StoredActivation,
    reason: &str,
    next_action: &str,
) -> Result<i64> {
    if reason.trim().is_empty() {
        bail!("suspend reason is required");
    }
    if next_action.trim().is_empty() {
        bail!("suspend next action is required");
    }

    conn.execute(
        "update work_unit_activations set status = 'suspended', suspended_at = current_timestamp where id = ?1",
        params![active.activation_id],
    )?;
    conn.execute(
        r#"
        insert into suspend_snapshots(
            work_unit_activation_id, work_unit_id, reason, active_task_ids, next_action,
            selected_gate_id, authority_refs, review_scope_refs, repository_heads,
            repository_snapshot_ids, repository_status, dirty_state_summary,
            open_findings, assumptions, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, current_timestamp)
        "#,
        params![
            active.activation_id,
            active.work_unit_id,
            reason,
            snapshot_active_task_ids(conn, active.work_unit_id)?,
            next_action,
            snapshot_selected_gate_id(conn, active.work_unit_id)?,
            snapshot_authority_refs(conn)?,
            snapshot_review_scope_refs(conn, active.work_unit_id)?,
            snapshot_repository_heads(conn)?,
            snapshot_repository_snapshot_ids(conn, active.activation_id)?,
            snapshot_repository_status(conn)?,
            snapshot_dirty_state_summary(conn, active.activation_id)?,
            snapshot_open_findings(conn, active.work_unit_id)?,
            snapshot_assumptions(conn, active.work_unit_id)?,
        ],
    )?;
    let snapshot_id = conn.last_insert_rowid();
    conn.execute(
        "update work_unit_activations set suspend_snapshot_id = ?1 where id = ?2",
        params![snapshot_id, active.activation_id],
    )?;
    insert_event(
        conn,
        NewEvent {
            work_unit_id: active.work_unit_id,
            activation_id: Some(active.activation_id),
            related_activation_id: None,
            event_type: "suspended",
            reason: Some(reason),
            status_domain: "activation",
            previous_status: Some("active"),
            next_status: Some("suspended"),
        },
    )?;

    Ok(snapshot_id)
}

fn csv_query(conn: &Connection, sql: &str, values: &[&dyn rusqlite::ToSql]) -> Result<String> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(values, |row| row.get::<_, String>(0))?;
    let mut collected = Vec::new();
    for row in rows {
        collected.push(row?);
    }
    Ok(collected.join(","))
}

fn snapshot_active_task_ids(conn: &Connection, work_unit_id: i64) -> Result<String> {
    csv_query(
        conn,
        "select cast(id as text) from tasks where work_unit_id = ?1 and status = 'open' order by id",
        &[&work_unit_id],
    )
}

fn snapshot_selected_gate_id(conn: &Connection, work_unit_id: i64) -> Result<Option<i64>> {
    conn.query_row(
        r#"
        select id
        from validation_gates
        where work_unit_id = ?1 and status = 'selected'
        order by id desc
        limit 1
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn snapshot_authority_refs(conn: &Connection) -> Result<String> {
    csv_query(
        conn,
        "select cast(id as text) || ':' || event_type from authority_events where status = 'active' order by id",
        &[],
    )
}

fn snapshot_review_scope_refs(conn: &Connection, work_unit_id: i64) -> Result<String> {
    csv_query(
        conn,
        r#"
        select cast(coalesce(review_scope_id, 0) as text) || ':' || review_type || ':' || status
        from review_plans
        where work_unit_id = ?1 and stage != 'resume-ready'
        order by id
        "#,
        &[&work_unit_id],
    )
}

fn snapshot_repository_heads(conn: &Connection) -> Result<String> {
    csv_query(
        conn,
        "select name || ':' || coalesce(current_head, '') from repositories order by name",
        &[],
    )
}

fn snapshot_repository_snapshot_ids(conn: &Connection, activation_id: i64) -> Result<String> {
    csv_query(
        conn,
        r#"
        select cast(id as text)
        from repository_snapshots
        where work_unit_activation_id = ?1
        order by id
        "#,
        &[&activation_id],
    )
}

fn snapshot_repository_status(conn: &Connection) -> Result<String> {
    csv_query(
        conn,
        "select name || ':' || coalesce(status_summary, '') from repositories order by name",
        &[],
    )
}

fn snapshot_dirty_state_summary(conn: &Connection, activation_id: i64) -> Result<String> {
    csv_query(
        conn,
        r#"
        select coalesce(s.status_summary, '') || ':' || count(d.id)
        from repository_snapshots s
        left join repository_dirty_entries d on d.repository_snapshot_id = s.id
        where s.work_unit_activation_id = ?1
        group by s.id
        order by s.id
        "#,
        &[&activation_id],
    )
}

fn snapshot_open_findings(conn: &Connection, work_unit_id: i64) -> Result<String> {
    csv_query(
        conn,
        r#"
        select cast(f.id as text) || ':' || f.finding_type || ':' || f.severity
        from findings f
        join review_runs r on r.id = f.review_run_id
        join review_plans p on p.id = r.review_plan_id
        where p.work_unit_id = ?1 and f.status = 'open'
        order by f.id
        "#,
        &[&work_unit_id],
    )
}

fn snapshot_assumptions(conn: &Connection, work_unit_id: i64) -> Result<String> {
    csv_query(
        conn,
        r#"
        select cast(id as text) || ':' || status
        from work_unit_dependencies
        where work_unit_id = ?1 and dependency_type = 'invalidates_assumption'
        order by id
        "#,
        &[&work_unit_id],
    )
}

fn snapshot_entries_still_current(stored: &str, current: &str) -> bool {
    stored
        .split(',')
        .filter(|entry| !entry.is_empty())
        .all(|entry| {
            current
                .split(',')
                .any(|current_entry| current_entry == entry)
        })
}

fn update_work_unit_lifecycle(
    root: &Path,
    work_unit_id: Option<i64>,
    required_status: &str,
    next_status: &str,
    event_type: &'static str,
    reason: &str,
) -> Result<WorkStatusOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let target = resolve_lifecycle_work_unit(&tx, project_id, work_unit_id)?;
    let previous_status = target.status;
    if previous_status != required_status {
        bail!("work unit must be {required_status} before {event_type}");
    }
    tx.execute(
        "update work_units set status = ?1 where id = ?2",
        params![next_status, target.work_unit_id],
    )?;
    insert_event(
        &tx,
        NewEvent {
            work_unit_id: target.work_unit_id,
            activation_id: target.activation_id,
            related_activation_id: None,
            event_type,
            reason: Some(reason),
            status_domain: "work_unit",
            previous_status: Some(&previous_status),
            next_status: Some(next_status),
        },
    )?;
    tx.commit()?;

    Ok(WorkStatusOutcome {
        work_unit_id: target.work_unit_id,
        activation_id: target.activation_id,
        previous_status,
        status: next_status.to_string(),
    })
}

fn resolve_lifecycle_work_unit(
    conn: &Connection,
    project_id: i64,
    work_unit_id: Option<i64>,
) -> Result<LifecycleWorkUnit> {
    match work_unit_id {
        Some(id) => {
            let status = conn
                .query_row(
                    "select status from work_units where id = ?1 and project_id = ?2",
                    params![id, project_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .context("work unit not found")?;
            Ok(LifecycleWorkUnit {
                work_unit_id: id,
                activation_id: lifecycle_activation_for_work(conn, id)?,
                status,
            })
        }
        None => {
            if let Some(active) = active_activation(conn)? {
                let status = conn.query_row(
                    "select status from work_units where id = ?1",
                    params![active.work_unit_id],
                    |row| row.get::<_, String>(0),
                )?;
                return Ok(LifecycleWorkUnit {
                    work_unit_id: active.work_unit_id,
                    activation_id: Some(active.activation_id),
                    status,
                });
            }
            let suspended =
                suspended_activation(conn)?.context("no active or suspended work unit")?;
            let status = conn.query_row(
                "select status from work_units where id = ?1",
                params![suspended.work_unit_id],
                |row| row.get::<_, String>(0),
            )?;
            Ok(LifecycleWorkUnit {
                work_unit_id: suspended.work_unit_id,
                activation_id: Some(suspended.activation_id),
                status,
            })
        }
    }
}

fn lifecycle_activation_for_work(conn: &Connection, work_unit_id: i64) -> Result<Option<i64>> {
    conn.query_row(
        r#"
        select id
        from work_unit_activations
        where work_unit_id = ?1 and status in ('active', 'suspended')
        order by case status when 'active' then 0 else 1 end, stack_depth desc, id desc
        limit 1
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn prepare_parent_frame(
    conn: &Connection,
    reason: &str,
    next_action: &str,
) -> Result<Option<StoredActivation>> {
    if let Some(active) = active_activation(conn)? {
        suspend_active_activation(conn, &active, reason, next_action)?;
        return Ok(Some(active));
    }

    suspended_activation(conn)
}

fn resolve_fork_source(
    conn: &Connection,
    project_id: i64,
    source: WorkForkSource<'_>,
) -> Result<StoredForkSource> {
    match source {
        WorkForkSource::Record(work_record_id) => {
            let source_work_unit_id = conn
                .query_row(
                    r#"
                    select wr.work_unit_id
                    from work_records wr
                    where wr.id = ?1 and wr.project_id = ?2
                    "#,
                    params![work_record_id, project_id],
                    |row| row.get::<_, Option<i64>>(0),
                )
                .optional()?
                .context("source work record not found")?;

            Ok(StoredForkSource {
                source_work_unit_id,
                source_work_unit_activation_id: None,
                source_work_record_id: Some(work_record_id),
                source_repository_snapshot_id: None,
                source_git_commit_id: None,
                source_git_commit_sha: None,
            })
        }
        WorkForkSource::Activation(activation_id) => {
            let work_unit_id = conn
                .query_row(
                    "select work_unit_id from work_unit_activations where id = ?1 and project_id = ?2",
                    params![activation_id, project_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .context("source activation not found")?;

            Ok(StoredForkSource {
                source_work_unit_id: Some(work_unit_id),
                source_work_unit_activation_id: Some(activation_id),
                source_work_record_id: None,
                source_repository_snapshot_id: None,
                source_git_commit_id: None,
                source_git_commit_sha: None,
            })
        }
        WorkForkSource::Commit(commit_sha) => {
            let git_commit_id = conn
                .query_row(
                    r#"
                    select c.id
                    from git_commits c
                    join repositories r on r.id = c.repository_id
                    where c.commit_sha = ?1 and r.project_id = ?2
                    order by c.id
                    limit 1
                    "#,
                    params![commit_sha, project_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            Ok(StoredForkSource {
                source_work_unit_id: None,
                source_work_unit_activation_id: None,
                source_work_record_id: None,
                source_repository_snapshot_id: None,
                source_git_commit_id: git_commit_id,
                source_git_commit_sha: Some(commit_sha.to_string()),
            })
        }
        WorkForkSource::GitCommit(git_commit_id) => {
            let commit_sha = conn
                .query_row(
                    r#"
                    select c.commit_sha
                    from git_commits c
                    join repositories r on r.id = c.repository_id
                    where c.id = ?1 and r.project_id = ?2
                    "#,
                    params![git_commit_id, project_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?
                .context("source git commit not found")?;
            Ok(StoredForkSource {
                source_work_unit_id: None,
                source_work_unit_activation_id: None,
                source_work_record_id: None,
                source_repository_snapshot_id: None,
                source_git_commit_id: Some(git_commit_id),
                source_git_commit_sha: Some(commit_sha),
            })
        }
        WorkForkSource::RepositorySnapshot(repository_snapshot_id) => {
            conn.query_row(
                r#"
                select 1
                from repository_snapshots s
                join repositories r on r.id = s.repository_id
                where s.id = ?1 and r.project_id = ?2
                "#,
                params![repository_snapshot_id, project_id],
                |_| Ok(()),
            )
            .optional()?
            .context("source repository snapshot not found")?;
            Ok(StoredForkSource {
                source_work_unit_id: None,
                source_work_unit_activation_id: None,
                source_work_record_id: None,
                source_repository_snapshot_id: Some(repository_snapshot_id),
                source_git_commit_id: None,
                source_git_commit_sha: None,
            })
        }
    }
}

fn fork_reason_code(reason: &str) -> &str {
    match reason {
        "design_changed"
        | "agent_drift"
        | "invalid_assumption"
        | "failed_validation"
        | "user_requested_redo"
        | "other" => reason,
        _ => "other",
    }
}

fn dependency_type_for_fork_reason(reason: &str) -> &'static str {
    match reason {
        "design_changed" | "agent_drift" | "failed_validation" | "user_requested_redo" => {
            "supersedes"
        }
        "invalid_assumption" => "invalidates_assumption",
        _ => "discovered_by",
    }
}

fn evaluate_resume_ready(conn: &Connection, maturity: &str) -> Result<ResumeGateEvaluation> {
    if !matches!(maturity, "basic" | "trace-aware" | "repo-aware") {
        bail!("unsupported maturity; use basic, trace-aware, or repo-aware");
    }
    let target = suspended_activation(conn)?.context("no suspended activation to resume")?;
    let snapshot = suspend_snapshot(conn, target.activation_id)?;
    let stack_revision = max_id(conn, "work_unit_events")?;
    let authority_high_watermark = max_id(conn, "authority_events")?;

    let deeper_open = conn.query_row(
        r#"
        select count(*)
        from work_unit_activations
        where project_id = ?1
          and stack_depth > ?2
          and status not in ('completed', 'abandoned')
        "#,
        params![target.project_id, target.stack_depth],
        |row| row.get::<_, i64>(0),
    )?;
    let blocking_dependencies = conn.query_row(
        r#"
        select count(*)
        from work_unit_dependencies
        where work_unit_id = ?1
          and dependency_type in ('blocks', 'invalidates_assumption', 'invalidates_closure')
          and status = 'open'
        "#,
        params![target.work_unit_id],
        |row| row.get::<_, i64>(0),
    )?;

    let checks = [
        (
            "resume_target_suspended",
            target.status == "suspended",
            "target activation must be suspended",
        ),
        (
            "snapshot_exists",
            true,
            "suspend snapshot must exist for target activation",
        ),
        (
            "suspend_reason_exists",
            !snapshot.reason.trim().is_empty(),
            "suspend snapshot must include a reason",
        ),
        (
            "next_action_exists",
            !snapshot.next_action.trim().is_empty(),
            "suspend snapshot must include a next action",
        ),
        (
            "deeper_frames_closed",
            deeper_open == 0,
            "deeper activation frames must be completed or abandoned",
        ),
        (
            "blocking_dependencies_clear",
            blocking_dependencies == 0,
            "blocking dependencies must be resolved",
        ),
    ];
    let basic_allowed = checks.iter().all(|(_, pass, _)| *pass);
    let mut blocking_reason = checks
        .iter()
        .find_map(|(_, pass, message)| (!pass).then_some((*message).to_string()));
    let mut items: Vec<_> = checks
        .into_iter()
        .map(|(name, pass, message)| ResumeReadyItem {
            name: name.to_string(),
            result: if pass { "pass" } else { "fail" }.to_string(),
            blocking_action: (!pass).then_some(message.to_string()),
            details: message.to_string(),
        })
        .collect();

    let trace_maturity = matches!(maturity, "trace-aware" | "repo-aware");
    let trace_counts = trace_maturity
        .then(|| trace_resume_counts(conn, target.work_unit_id))
        .transpose()?;
    let mut trace_allowed = true;
    if let Some(trace_counts) = trace_counts {
        let active_tasks_current = snapshot.active_task_ids.as_deref().unwrap_or("")
            == snapshot_active_task_ids(conn, target.work_unit_id)?;
        let authority_refs_current =
            snapshot.authority_refs.as_deref().unwrap_or("") == snapshot_authority_refs(conn)?;
        let review_scope_refs_current = snapshot.review_scope_refs.as_deref().unwrap_or("")
            == snapshot_review_scope_refs(conn, target.work_unit_id)?;
        let open_findings_current = snapshot.open_findings.as_deref().unwrap_or("")
            == snapshot_open_findings(conn, target.work_unit_id)?;
        for (name, pass, details) in [
            (
                "active_tasks_current",
                active_tasks_current,
                "active task set matches suspend snapshot".to_string(),
            ),
            (
                "authority_refs_current",
                authority_refs_current,
                "active authority refs match suspend snapshot".to_string(),
            ),
            (
                "review_scope_refs_current",
                review_scope_refs_current,
                "review scope refs match suspend snapshot".to_string(),
            ),
            (
                "open_findings_current",
                open_findings_current,
                "open findings match suspend snapshot".to_string(),
            ),
        ] {
            if !pass {
                trace_allowed = false;
                blocking_reason
                    .get_or_insert_with(|| "trace-aware resume checks failed".to_string());
            }
            items.push(ResumeReadyItem {
                name: name.to_string(),
                result: if pass { "pass" } else { "fail" }.to_string(),
                blocking_action: (!pass).then_some(details.clone()),
                details,
            });
        }
        let stale_design_total =
            trace_counts.stale_design_records + trace_counts.stale_coverage_items;
        let selected_gate_snapshot_current =
            snapshot.selected_gate_id == snapshot_selected_gate_id(conn, target.work_unit_id)?;
        let trace_items = [
            (
                "design_version_current",
                stale_design_total == 0,
                format!(
                    "{} design-derived records and {} coverage items reference changed requirements",
                    trace_counts.stale_design_records, trace_counts.stale_coverage_items
                ),
            ),
            (
                "task_derivation_current",
                trace_counts.stale_task_derivations == 0,
                format!(
                    "{} task derivations reference changed requirements",
                    trace_counts.stale_task_derivations
                ),
            ),
            (
                "checklist_current",
                trace_counts.stale_checklists == 0,
                format!(
                    "{} checklists reference changed requirements",
                    trace_counts.stale_checklists
                ),
            ),
            (
                "selected_gate_current",
                trace_counts.stale_selected_gates == 0 && selected_gate_snapshot_current,
                format!(
                    "{} selected validation gates reference changed requirements; snapshot match={}",
                    trace_counts.stale_selected_gates, selected_gate_snapshot_current
                ),
            ),
        ];
        for (name, pass, details) in trace_items {
            if !pass {
                trace_allowed = false;
                blocking_reason
                    .get_or_insert_with(|| "trace-aware resume checks failed".to_string());
            }
            items.push(ResumeReadyItem {
                name: name.to_string(),
                result: if pass { "pass" } else { "fail" }.to_string(),
                blocking_action: (!pass).then(|| details.clone()),
                details,
            });
        }
        let review_state = review_plan_stage_state(conn, target.work_unit_id, "resume-ready")?;
        let review_pass = review_state.required_plan_count == 0
            || (review_state.incomplete_required_plan_count == 0
                && review_state.stale_target_count == 0);
        if !review_pass {
            trace_allowed = false;
            blocking_reason.get_or_insert_with(|| "trace-aware resume checks failed".to_string());
        }
        items.push(ResumeReadyItem {
            name: "review_plan_current".to_string(),
            result: if review_pass { "pass" } else { "fail" }.to_string(),
            blocking_action: (!review_pass).then_some(
                "complete required resume-ready plans or refresh stale targets".to_string(),
            ),
            details: format!(
                "{} required resume-ready plans, {} incomplete, {} stale targets",
                review_state.required_plan_count,
                review_state.incomplete_required_plan_count,
                review_state.stale_target_count
            ),
        });
    } else {
        let later_items = [
            (
                "active_tasks_current",
                "trace-aware active task snapshot check was not requested",
            ),
            (
                "authority_refs_current",
                "trace-aware authority refs snapshot check was not requested",
            ),
            (
                "review_scope_refs_current",
                "trace-aware review scope refs snapshot check was not requested",
            ),
            (
                "design_version_current",
                "trace-aware design version check was not requested",
            ),
            (
                "task_derivation_current",
                "trace-aware task derivation check was not requested",
            ),
            (
                "checklist_current",
                "trace-aware checklist check was not requested",
            ),
            (
                "selected_gate_current",
                "trace-aware validation gate check was not requested",
            ),
            (
                "review_plan_current",
                "trace-aware review plan check was not requested",
            ),
            (
                "open_findings_current",
                "trace-aware open findings snapshot check was not requested",
            ),
        ];
        items.extend(
            later_items
                .into_iter()
                .map(|(name, details)| ResumeReadyItem {
                    name: name.to_string(),
                    result: "not_checked".to_string(),
                    blocking_action: None,
                    details: details.to_string(),
                }),
        );
    }

    let repo_maturity = maturity == "repo-aware";
    let mut repo_allowed = true;
    let mut repository_snapshot_id = None;
    let mut repository_state_revision = None;
    if repo_maturity {
        let repo_state = repository_resume_state(conn, &target)?;
        repository_snapshot_id = repo_state.latest_current_snapshot_id;
        repository_state_revision = Some(repository_state_revision_for_resume(conn)?);
        let current_repository_heads = snapshot_repository_heads(conn)?;
        let repository_heads_current = snapshot_entries_still_current(
            snapshot.repository_heads.as_deref().unwrap_or(""),
            &current_repository_heads,
        );
        let suspend_repository_snapshot_ids =
            snapshot.repository_snapshot_ids.as_deref().unwrap_or("");
        let current_repository_status = snapshot_repository_status(conn)?;
        let repository_status_current = snapshot_entries_still_current(
            snapshot.repository_status.as_deref().unwrap_or(""),
            &current_repository_status,
        );
        let current_dirty_state_summary = snapshot_dirty_state_summary(conn, target.activation_id)?;
        let dirty_state_summary_current = snapshot_entries_still_current(
            snapshot.dirty_state_summary.as_deref().unwrap_or(""),
            &current_dirty_state_summary,
        );
        let pass = repo_state.repository_count == 0
            || (repo_state.missing_base_snapshot_count == 0
                && repo_state.missing_current_snapshot_count == 0
                && repo_state.missing_comparison_count == 0
                && repo_state.unclassified_comparison_count == 0
                && repo_state.unclassified_dirty_state_count == 0
                && repository_heads_current
                && repository_status_current
                && dirty_state_summary_current);
        if !pass {
            repo_allowed = false;
            blocking_reason.get_or_insert_with(|| "repo-aware resume checks failed".to_string());
        }
        items.push(ResumeReadyItem {
            name: "repository_heads_current".to_string(),
            result: if repository_heads_current {
                "pass"
            } else {
                "fail"
            }
            .to_string(),
            blocking_action: (!repository_heads_current)
                .then_some("record and compare current repository heads".to_string()),
            details: "repository heads match suspend snapshot".to_string(),
        });
        items.push(ResumeReadyItem {
            name: "repository_state_current".to_string(),
            result: if pass { "pass" } else { "fail" }.to_string(),
            blocking_action: (!pass).then_some(
                "record current repository snapshots and classify resume differences".to_string(),
            ),
            details: format!(
                "{} repositories, {} suspend snapshots, {} missing base snapshots, {} missing current snapshots, {} missing comparisons, {} unclassified comparisons, {} unclassified dirty states; suspend snapshot ids={}; status match={}; dirty summary match={}",
                repo_state.repository_count,
                repo_state.base_snapshot_count,
                repo_state.missing_base_snapshot_count,
                repo_state.missing_current_snapshot_count,
                repo_state.missing_comparison_count,
                repo_state.unclassified_comparison_count,
                repo_state.unclassified_dirty_state_count,
                suspend_repository_snapshot_ids,
                repository_status_current,
                dirty_state_summary_current
            ),
        });
    } else {
        items.push(ResumeReadyItem {
            name: "repository_heads_current".to_string(),
            result: "not_checked".to_string(),
            blocking_action: None,
            details: "repo-aware repository head snapshot check was not requested".to_string(),
        });
        items.push(ResumeReadyItem {
            name: "repository_state_current".to_string(),
            result: "not_checked".to_string(),
            blocking_action: None,
            details: "repo-aware repository state check was not requested".to_string(),
        });
    }

    if repo_maturity {
        let invalidated_assumptions = open_assumption_invalidations(conn, target.work_unit_id)?;
        let assumptions_current = snapshot.assumptions.as_deref().unwrap_or("")
            == snapshot_assumptions(conn, target.work_unit_id)?;
        let pass = invalidated_assumptions == 0 && assumptions_current;
        if !pass {
            repo_allowed = false;
            blocking_reason.get_or_insert_with(|| "repo-aware resume checks failed".to_string());
        }
        items.push(ResumeReadyItem {
            name: "assumptions_current".to_string(),
            result: if pass { "pass" } else { "fail" }.to_string(),
            blocking_action: (!pass)
                .then_some("resolve open assumption invalidation dependencies".to_string()),
            details: format!(
                "{invalidated_assumptions} open assumption invalidations; snapshot match={assumptions_current}"
            ),
        });
    } else {
        items.push(ResumeReadyItem {
            name: "assumptions_current".to_string(),
            result: "not_checked".to_string(),
            blocking_action: None,
            details: "repo-aware assumptions check was not requested".to_string(),
        });
    }

    let allowed = basic_allowed && trace_allowed && repo_allowed;
    Ok(ResumeGateEvaluation {
        work_unit_id: target.work_unit_id,
        activation_id: target.activation_id,
        suspend_snapshot_id: snapshot.id,
        resume_result: if allowed { "allowed" } else { "blocked" }.to_string(),
        blocking_reason,
        allowed_next_action: Some(snapshot.next_action),
        authority_high_watermark,
        activation_stack_revision: stack_revision,
        repository_snapshot_id,
        repository_state_revision,
        items,
    })
}

fn repository_state_revision_for_resume(conn: &Connection) -> Result<i64> {
    Ok([
        max_id(conn, "repository_snapshots")?,
        max_id(conn, "repository_dirty_entries")?,
        max_id(conn, "repository_state_classifications")?,
        max_id(conn, "repository_snapshot_comparisons")?,
    ]
    .into_iter()
    .sum())
}

fn repository_resume_state(
    conn: &Connection,
    target: &StoredActivation,
) -> Result<RepositoryResumeState> {
    let repository_count = conn.query_row(
        "select count(*) from repositories where project_id = ?1",
        params![target.project_id],
        |row| row.get::<_, i64>(0),
    )?;
    let mut base_stmt = conn.prepare(
        r#"
        select s.id, s.repository_id
        from repository_snapshots s
        join repositories r on r.id = s.repository_id
        where r.project_id = ?1 and s.work_unit_activation_id = ?2
          and s.id = (
              select max(inner_s.id)
              from repository_snapshots inner_s
              where inner_s.repository_id = s.repository_id
                and inner_s.work_unit_activation_id = ?2
          )
        order by s.id
        "#,
    )?;
    let bases = base_stmt.query_map(params![target.project_id, target.activation_id], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut state = RepositoryResumeState {
        repository_count,
        ..RepositoryResumeState::default()
    };

    for base in bases {
        let (base_snapshot_id, repository_id) = base?;
        state.base_snapshot_count += 1;
        let current = conn
            .query_row(
                r#"
                select id, is_clean
                from repository_snapshots
                where repository_id = ?1 and id > ?2
                order by id desc
                limit 1
                "#,
                params![repository_id, base_snapshot_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        let Some((current_snapshot_id, is_clean)) = current else {
            state.missing_current_snapshot_count += 1;
            continue;
        };
        state.latest_current_snapshot_id = Some(
            state
                .latest_current_snapshot_id
                .map_or(current_snapshot_id, |id| id.max(current_snapshot_id)),
        );
        let comparison = conn
            .query_row(
                r#"
                select result
                from repository_snapshot_comparisons
                where base_repository_snapshot_id = ?1
                  and current_repository_snapshot_id = ?2
                  and comparison_type = 'resume'
                order by id desc
                limit 1
                "#,
                params![base_snapshot_id, current_snapshot_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        match comparison.as_deref() {
            Some("same" | "changed_classified") => {}
            Some("changed_unclassified") => state.unclassified_comparison_count += 1,
            Some(_) => state.unclassified_comparison_count += 1,
            None => state.missing_comparison_count += 1,
        }
        if is_clean == 0 && !repository_snapshot_dirty_state_classified(conn, current_snapshot_id)?
        {
            state.unclassified_dirty_state_count += 1;
        }
    }
    state.missing_base_snapshot_count = repository_count.saturating_sub(state.base_snapshot_count);

    Ok(state)
}

fn repository_snapshot_dirty_state_classified(
    conn: &Connection,
    repository_snapshot_id: i64,
) -> Result<bool> {
    let dirty_entry_count = conn.query_row(
        "select count(*) from repository_dirty_entries where repository_snapshot_id = ?1",
        params![repository_snapshot_id],
        |row| row.get::<_, i64>(0),
    )?;
    if dirty_entry_count == 0 {
        return conn
            .query_row(
                r#"
                select 1
                from repository_state_classifications
                where repository_snapshot_id = ?1
                  and dirty_entry_id is null
                  and classification in ('expected', 'unrelated', 'generated', 'accepted_exception')
                limit 1
                "#,
                params![repository_snapshot_id],
                |_| Ok(()),
            )
            .optional()
            .map(|row| row.is_some())
            .map_err(Into::into);
    }

    let unclassified_dirty_entries = conn.query_row(
        r#"
        select count(*)
        from repository_dirty_entries d
        where d.repository_snapshot_id = ?1
          and not exists (
              select 1
              from repository_state_classifications c
              where c.repository_snapshot_id = d.repository_snapshot_id
                and c.dirty_entry_id = d.id
                and c.classification in ('expected', 'unrelated', 'generated', 'accepted_exception')
          )
        "#,
        params![repository_snapshot_id],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(unclassified_dirty_entries == 0)
}

fn validation_close_state(conn: &Connection, work_unit_id: i64) -> Result<ValidationCloseState> {
    conn.query_row(
        r#"
        select
            count(*),
            sum(case when latest_result is null then 1 else 0 end),
            sum(case
                when latest_result is not null and latest_result != 'pass' and accepted_failure = 1
                then 1 else 0
            end),
            sum(case
                when latest_result is not null and latest_result != 'pass' and accepted_failure = 0
                then 1 else 0
            end)
        from (
            select
                vg.id,
                (
                    select vr.result
                    from validation_runs vr
                    where vr.validation_gate_id = vg.id
                    order by vr.id desc
                    limit 1
                ) as latest_result,
                exists (
                    select 1
                    from validation_runs vr
                    left join acceptance_records run_ar on run_ar.id = vr.acceptance_record_id
                    where vr.validation_gate_id = vg.id
                      and (
                        (
                          run_ar.status = 'approved'
                          and run_ar.acceptance_type in ('classified_failure', 'evidence_gap', 'explicit_exception')
                        )
                        or exists (
                          select 1
                          from acceptance_records ar
                          where ar.target_type = 'validation_gate_template'
                            and ar.validation_gate_template_id = vg.template_id
                            and ar.acceptance_type in ('explicit_exception', 'classified_failure', 'evidence_gap')
                            and ar.status = 'approved'
                        )
                      )
                    order by vr.id desc
                    limit 1
                ) as accepted_failure
            from validation_gates vg
            left join tasks t on t.id = vg.task_id
            where vg.status = 'active'
              and coalesce(vg.work_unit_id, t.work_unit_id) = ?1
        )
        "#,
        params![work_unit_id],
        |row| {
            Ok(ValidationCloseState {
                selected_gate_count: row.get(0)?,
                missing_run_count: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                accepted_failure_count: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                unaccepted_failure_count: row.get::<_, Option<i64>>(3)?.unwrap_or(0),
            })
        },
    )
    .map_err(Into::into)
}

fn close_process_state(
    conn: &Connection,
    project_id: i64,
    work_unit_id: i64,
) -> Result<CloseProcessState> {
    let work_responsibility: Option<String> = conn
        .query_row(
            "select responsibility from work_units where id = ?1 and project_id = ?2",
            params![work_unit_id, project_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    let applicable_rule_count = conn.query_row(
        r#"
        select count(*)
        from rule_bindings
        where project_id = ?1
          and status = 'active'
          and (
              scope_type = 'project'
              or scope_type = 'design_package'
              or work_unit_id = ?2
              or scope_key in ('project', ?3)
              or (?4 is not null and scope_key = ?4)
          )
        "#,
        params![
            project_id,
            work_unit_id,
            work_unit_id.to_string(),
            work_responsibility.as_deref()
        ],
        |row| row.get(0),
    )?;
    let rule_conflict_count = conn.query_row(
        r#"
        select count(*)
        from rule_bindings lower
        where lower.project_id = ?1
          and lower.status = 'active'
          and lower.rule_source_type = 'user_correction'
          and (
              lower.scope_type = 'project'
              or lower.scope_type = 'design_package'
              or lower.work_unit_id = ?2
              or lower.scope_key in ('project', ?3)
              or (?4 is not null and lower.scope_key = ?4)
          )
          and not exists (
              select 1
              from acceptance_records ar
              where ar.project_id = lower.project_id
                and ar.target_type = 'rule_binding'
                and ar.rule_binding_id = lower.id
                and ar.status = 'approved'
                and ar.acceptance_type in ('explicit_exception', 'stale_accepted')
          )
          and exists (
              select 1
              from rule_bindings higher
              where higher.project_id = lower.project_id
                and higher.status = 'active'
                and higher.id != lower.id
                and higher.rule_source_type = lower.rule_source_type
                and (
                    higher.scope_type = 'project'
                    or higher.scope_type = 'design_package'
                    or higher.work_unit_id = ?2
                    or higher.scope_key in ('project', ?3)
                    or (?4 is not null and higher.scope_key = ?4)
                )
                and (
                    higher.scope_key = lower.scope_key
                    or higher.scope_key = 'project'
                    or lower.scope_key = 'project'
                    or higher.work_unit_id = lower.work_unit_id
                    or higher.scope_type = 'work_unit'
                    or lower.scope_type = 'work_unit'
                )
                and (
                    higher.precedence > lower.precedence
                    or (
                        higher.precedence = lower.precedence
                        and case higher.scope_type
                                when 'work_unit' then 4
                                when 'repository' then 3
                                when 'design_package' then 3
                                when 'agent_role' then 3
                                when 'command' then 3
                                when 'review' then 3
                                when 'project' then 1
                                else 2
                            end
                            > case lower.scope_type
                                when 'work_unit' then 4
                                when 'repository' then 3
                                when 'design_package' then 3
                                when 'agent_role' then 3
                                when 'command' then 3
                                when 'review' then 3
                                when 'project' then 1
                                else 2
                            end
                    )
                )
          )
        "#,
        params![
            project_id,
            work_unit_id,
            work_unit_id.to_string(),
            work_responsibility.as_deref()
        ],
        |row| row.get(0),
    )?;
    let fixed_command_count = conn.query_row(
        r#"
        select count(*)
        from command_profiles cp
        where cp.project_id = ?1
          and cp.status = 'fixed'
          and exists (
              select 1
              from rule_bindings rb
              where rb.command_profile_id = cp.id
                and rb.status = 'active'
                and (
                    rb.scope_type = 'project'
                    or rb.work_unit_id = ?2
                    or rb.scope_key in ('project', ?3)
                    or (?4 is not null and rb.scope_key = ?4)
                )
          )
        "#,
        params![
            project_id,
            work_unit_id,
            work_unit_id.to_string(),
            work_responsibility.as_deref()
        ],
        |row| row.get(0),
    )?;
    let missing_fixed_command_usage_count = conn.query_row(
        r#"
        select count(*)
        from command_profiles cp
        where cp.project_id = ?1
          and cp.status = 'fixed'
          and exists (
              select 1
              from rule_bindings rb
              where rb.command_profile_id = cp.id
                and rb.status = 'active'
                and (
                    rb.scope_type = 'project'
                    or rb.work_unit_id = ?2
                    or rb.scope_key in ('project', ?3)
                    or (?4 is not null and rb.scope_key = ?4)
                )
          )
          and not exists (
              select 1
              from command_usages cu
              where cu.command_profile_id = cp.id
                and cu.work_unit_id = ?2
          )
          and not exists (
              select 1
              from command_deviations d
              where d.command_profile_id = cp.id
                and d.work_unit_id = ?2
                and (
                    d.status = 'approved'
                    or exists (
                        select 1
                        from acceptance_records ar
                        where ar.target_type = 'command_deviation'
                          and ar.command_deviation_id = d.id
                          and ar.status = 'approved'
                    )
                )
          )
        "#,
        params![
            project_id,
            work_unit_id,
            work_unit_id.to_string(),
            work_responsibility.as_deref()
        ],
        |row| row.get(0),
    )?;
    let repeated_correction_count = conn.query_row(
        r#"
        select count(*)
        from user_corrections uc
        where uc.project_id = ?1
          and uc.status = 'active'
          and not exists (
              select 1
              from acceptance_records ar
              where ar.project_id = uc.project_id
                and ar.target_type = 'stale_record'
                and ar.stale_record_type = 'user_correction'
                and ar.stale_record_id = uc.id
                and ar.status = 'approved'
                and ar.acceptance_type in ('stale_accepted', 'explicit_exception')
          )
        "#,
        params![project_id],
        |row| row.get(0),
    )?;
    let open_kpt_review_count = conn.query_row(
        "select count(*) from kpt_reviews where project_id = ?1 and status = 'open'",
        params![project_id],
        |row| row.get(0),
    )?;
    let work_record_count = conn.query_row(
        "select count(*) from work_records where project_id = ?1 and work_unit_id = ?2",
        params![project_id, work_unit_id],
        |row| row.get(0),
    )?;
    let work_record_evidence_link_count = conn.query_row(
        r#"
        select
            (select count(*) from work_record_commands c join work_records r on r.id = c.work_record_id where r.project_id = ?1 and r.work_unit_id = ?2)
          + (select count(*) from work_record_commits c join work_records r on r.id = c.work_record_id where r.project_id = ?1 and r.work_unit_id = ?2)
          + (select count(*) from work_record_files f join work_records r on r.id = f.work_record_id where r.project_id = ?1 and r.work_unit_id = ?2)
        "#,
        params![project_id, work_unit_id],
        |row| row.get(0),
    )?;
    let invalid_commit_message_count = conn.query_row(
        r#"
        select count(*)
        from work_record_commits wrc
        join work_records wr on wr.id = wrc.work_record_id
        join git_commits gc on gc.id = wrc.git_commit_id
        where wr.project_id = ?1
          and wr.work_unit_id = ?2
          and (
              instr(gc.subject, ': ') = 0
              or lower(gc.subject) = 'review'
              or lower(gc.subject) like 'review:%'
              or lower(gc.subject) like 'review %'
              or lower(gc.subject) like '% review'
              or lower(gc.subject) like '% review %'
              or lower(gc.subject) glob '*' || char(112,104,97,115,101) || '[0-9]*'
              or lower(gc.subject) glob '*' || char(112,104,97,115,101) || ' [0-9]*'
          )
        "#,
        params![project_id, work_unit_id],
        |row| row.get(0),
    )?;
    Ok(CloseProcessState {
        applicable_rule_count,
        rule_conflict_count,
        fixed_command_count,
        missing_fixed_command_usage_count,
        invalid_commit_message_count,
        repeated_correction_count,
        open_kpt_review_count,
        work_record_count,
        work_record_evidence_link_count,
    })
}

fn repository_close_state(
    conn: &Connection,
    active: &StoredActivation,
) -> Result<RepositoryCloseState> {
    let repository_count = conn.query_row(
        "select count(*) from repositories where project_id = ?1",
        params![active.project_id],
        |row| row.get::<_, i64>(0),
    )?;
    let active_snapshot_count = conn.query_row(
        r#"
        select count(distinct s.repository_id)
        from repository_snapshots s
        join repositories r on r.id = s.repository_id
        where r.project_id = ?1 and s.work_unit_activation_id = ?2
        "#,
        params![active.project_id, active.activation_id],
        |row| row.get::<_, i64>(0),
    )?;
    let mut stmt = conn.prepare(
        r#"
        select s.id, s.repository_id, s.is_clean
        from repository_snapshots s
        join repositories r on r.id = s.repository_id
        where r.project_id = ?1
          and s.work_unit_activation_id = ?2
          and s.id = (
              select max(inner_s.id)
              from repository_snapshots inner_s
              where inner_s.repository_id = s.repository_id
                and inner_s.work_unit_activation_id = ?2
          )
        "#,
    )?;
    let snapshots = stmt.query_map(params![active.project_id, active.activation_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;
    let mut unclassified_dirty_state_count = 0;
    let mut missing_comparison_count = 0;
    let mut unclassified_comparison_count = 0;
    for snapshot in snapshots {
        let (repository_snapshot_id, repository_id, is_clean) = snapshot?;
        if is_clean == 0
            && !repository_snapshot_dirty_state_classified(conn, repository_snapshot_id)?
        {
            unclassified_dirty_state_count += 1;
        }
        if let Some(base_snapshot_id) = previous_repository_snapshot(
            conn,
            repository_id,
            repository_snapshot_id,
            active.activation_id,
        )? {
            match close_repository_snapshot_comparison(
                conn,
                base_snapshot_id,
                repository_snapshot_id,
            )?
            .as_deref()
            {
                Some("same" | "changed_classified") => {}
                Some("changed_unclassified") | Some(_) => unclassified_comparison_count += 1,
                None => missing_comparison_count += 1,
            }
        }
    }

    Ok(RepositoryCloseState {
        repository_count,
        missing_snapshot_count: repository_count.saturating_sub(active_snapshot_count),
        unclassified_dirty_state_count,
        missing_comparison_count,
        unclassified_comparison_count,
    })
}

fn previous_repository_snapshot(
    conn: &Connection,
    repository_id: i64,
    repository_snapshot_id: i64,
    active_activation_id: i64,
) -> Result<Option<i64>> {
    conn.query_row(
        r#"
        select max(s.id)
        from repository_snapshots s
        where s.repository_id = ?1
          and s.id < ?2
          and (
              s.work_unit_activation_id is null
              or s.work_unit_activation_id < ?3
          )
        "#,
        params![repository_id, repository_snapshot_id, active_activation_id],
        |row| row.get::<_, Option<i64>>(0),
    )
    .map_err(Into::into)
}

fn close_repository_snapshot_comparison(
    conn: &Connection,
    base_snapshot_id: i64,
    current_snapshot_id: i64,
) -> Result<Option<String>> {
    conn.query_row(
        r#"
        select result
        from repository_snapshot_comparisons
        where base_repository_snapshot_id = ?1
          and current_repository_snapshot_id = ?2
          and comparison_type = 'close'
        order by id desc
        limit 1
        "#,
        params![base_snapshot_id, current_snapshot_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn review_plan_stage_state(
    conn: &Connection,
    work_unit_id: i64,
    stage: &str,
) -> Result<ReviewPlanStageState> {
    let mut stmt = conn.prepare(
        r#"
        select id, status, review_type, design_version_id, work_unit_id
        from review_plans
        where work_unit_id = ?1
          and stage = ?2
          and required = 1
        order by id
        "#,
    )?;
    let rows = stmt.query_map(params![work_unit_id, stage], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<i64>>(3)?,
            row.get::<_, Option<i64>>(4)?,
        ))
    })?;
    let mut state = ReviewPlanStageState::default();
    for row in rows {
        let (review_plan_id, status, review_type, design_version_id, plan_work_unit_id) = row?;
        state.required_plan_count += 1;
        let accepted = review_plan_accepted(conn, review_plan_id)?;
        if status != "clean" && !accepted {
            state.incomplete_required_plan_count += 1;
        }
        if let Some(kind) = review_context_kind_for_plan(stage, &review_type)
            && design_version_id.is_some()
            && !accepted
            && !review_plan_has_clean_context_run(
                conn,
                review_plan_id,
                kind,
                design_version_id,
                plan_work_unit_id,
            )?
        {
            state.missing_context_run_count += 1;
        }
        if !accepted {
            state.stale_target_count += stale_review_plan_target_count(conn, review_plan_id)?;
        }
    }
    Ok(state)
}

fn review_plan_accepted(conn: &Connection, review_plan_id: i64) -> Result<bool> {
    conn.query_row(
        r#"
        select exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'review_plan'
              and ar.review_plan_id = ?1
              and ar.status = 'approved'
              and ar.acceptance_type in ('explicit_exception', 'stale_accepted')
        )
        "#,
        params![review_plan_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn review_context_kind_for_plan(stage: &str, review_type: &str) -> Option<&'static str> {
    match (stage, review_type) {
        ("close-ready", "design_implementation_diff") => Some("design-implementation-diff"),
        ("close-ready", "implementation_review") => Some("implementation-review"),
        _ => None,
    }
}

fn stale_review_plan_target_count(conn: &Connection, review_plan_id: i64) -> Result<i64> {
    let mut stmt = conn.prepare(
        r#"
        select target_type, design_version_id, design_requirement_id, repository_snapshot_id
        from review_plan_targets
        where review_plan_id = ?1
        "#,
    )?;
    let rows = stmt.query_map(params![review_plan_id], |row| {
        Ok(ReviewPlanTargetForResume {
            target_type: row.get(0)?,
            design_version_id: row.get(1)?,
            design_requirement_id: row.get(2)?,
            repository_snapshot_id: row.get(3)?,
        })
    })?;
    let mut stale = 0;
    for row in rows {
        if review_plan_target_stale(conn, row?)? {
            stale += 1;
        }
    }
    Ok(stale)
}

fn review_plan_target_stale(conn: &Connection, target: ReviewPlanTargetForResume) -> Result<bool> {
    match target.target_type.as_str() {
        "design_version" => match target.design_version_id {
            Some(design_version_id) => design_version_stale(conn, design_version_id),
            None => Ok(true),
        },
        "design_requirement" => match target.design_requirement_id {
            Some(design_requirement_id) => design_requirement_stale(conn, design_requirement_id),
            None => Ok(true),
        },
        "repository_snapshot" => match target.repository_snapshot_id {
            Some(repository_snapshot_id) => {
                repository_snapshot_target_stale(conn, repository_snapshot_id)
            }
            None => Ok(true),
        },
        _ => Ok(false),
    }
}

fn design_version_stale(conn: &Connection, design_version_id: i64) -> Result<bool> {
    let current_id = conn
        .query_row(
            r#"
            select p.current_design_version_id
            from design_versions v
            join design_packages p on p.id = v.design_package_id
            where v.id = ?1
            "#,
            params![design_version_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    Ok(current_id != Some(design_version_id))
}

fn design_requirement_stale(conn: &Connection, design_requirement_id: i64) -> Result<bool> {
    conn.query_row(
        r#"
        select not exists (
            select 1
            from design_requirements old_r
            join design_versions old_v on old_v.id = old_r.design_version_id
            join design_packages p on p.id = old_v.design_package_id
            join design_requirements current_r
              on current_r.design_version_id = p.current_design_version_id
             and current_r.requirement_key = old_r.requirement_key
             and current_r.requirement_hash = old_r.requirement_hash
             and current_r.status = 'active'
            where old_r.id = ?1
        )
        "#,
        params![design_requirement_id],
        |row| row.get::<_, bool>(0),
    )
    .map_err(Into::into)
}

fn repository_snapshot_target_stale(
    conn: &Connection,
    repository_snapshot_id: i64,
) -> Result<bool> {
    let Some((repository_id, latest_snapshot_id)) = conn
        .query_row(
            r#"
            select s.repository_id, (
                select max(current_s.id)
                from repository_snapshots current_s
                where current_s.repository_id = s.repository_id
            )
            from repository_snapshots s
            where s.id = ?1
            "#,
            params![repository_snapshot_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?
    else {
        return Ok(true);
    };
    let _ = repository_id;
    if latest_snapshot_id == repository_snapshot_id {
        return Ok(false);
    }
    let classified_comparison = conn
        .query_row(
            r#"
            select 1
            from repository_snapshot_comparisons
            where base_repository_snapshot_id = ?1
              and current_repository_snapshot_id = ?2
              and comparison_type = 'resume'
              and result in ('same', 'changed_classified')
            order by id desc
            limit 1
            "#,
            params![repository_snapshot_id, latest_snapshot_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok(!classified_comparison)
}

fn open_assumption_invalidations(conn: &Connection, work_unit_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from work_unit_dependencies
        where work_unit_id = ?1
          and dependency_type = 'invalidates_assumption'
          and status = 'open'
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn trace_resume_counts(conn: &Connection, work_unit_id: i64) -> Result<TraceResumeCounts> {
    Ok(TraceResumeCounts {
        stale_design_records: count_stale_design_records_for_work(conn, work_unit_id)?,
        stale_task_derivations: count_stale_task_derivations_for_work(conn, work_unit_id)?,
        stale_checklists: count_stale_checklists_for_work(conn, work_unit_id)?,
        stale_selected_gates: count_stale_selected_gates_for_work(conn, work_unit_id)?,
        stale_coverage_items: count_stale_coverage_items_for_work(conn, work_unit_id)?,
    })
}

fn close_trace_state(conn: &Connection, work_unit_id: i64) -> Result<CloseTraceState> {
    Ok(CloseTraceState {
        active_requirement_count: count_active_requirements_for_work(conn, work_unit_id)?,
        derived_task_count: count_design_derived_tasks_for_work(conn, work_unit_id)?,
        missing_evidence_count: count_closed_derived_tasks_missing_evidence_for_work(
            conn,
            work_unit_id,
        )?,
        missing_coverage_count: count_closed_derived_tasks_missing_coverage_for_work(
            conn,
            work_unit_id,
        )?,
        missing_requirement_coverage_count: count_active_requirements_missing_coverage_for_work(
            conn,
            work_unit_id,
        )?,
        missing_validation_gate_count: count_derived_tasks_missing_selected_gate_for_work(
            conn,
            work_unit_id,
        )?,
        open_checklist_item_count: count_open_checklist_items_for_work(conn, work_unit_id)?,
        active_checklist_count: count_active_checklists_for_work(conn, work_unit_id)?,
    })
}

fn missing_required_close_review_types(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<Vec<String>> {
    let mut missing = Vec::new();
    for design_version_id in design_versions_for_work(conn, work_unit_id)? {
        for review_type in ["design_implementation_diff", "implementation_review"] {
            let count: i64 = conn.query_row(
                r#"
                select count(*)
                from review_plans
                where work_unit_id = ?1
                  and design_version_id = ?2
                  and stage = 'close-ready'
                  and review_type = ?3
                  and required = 1
                "#,
                params![work_unit_id, design_version_id, review_type],
                |row| row.get(0),
            )?;
            if count == 0 {
                missing.push(format!("{review_type}@design:{design_version_id}"));
            }
        }
    }
    Ok(missing)
}

fn design_versions_for_work(conn: &Connection, work_unit_id: i64) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        r#"
        select distinct r.design_version_id
        from task_derivations td
        join tasks t on t.id = td.task_id
        join design_requirements r on r.id = td.design_requirement_id
        where t.work_unit_id = ?1
          and td.status in ('active', 'stale')
        order by r.design_version_id
        "#,
    )?;
    let rows = stmt.query_map(params![work_unit_id], |row| row.get(0))?;
    let mut design_version_ids = Vec::new();
    for row in rows {
        design_version_ids.push(row?);
    }
    Ok(design_version_ids)
}

fn count_design_derived_tasks_for_work(conn: &Connection, work_unit_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        select count(distinct td.task_id)
        from task_derivations td
        join tasks t on t.id = td.task_id
        where t.work_unit_id = ?1
          and td.status in ('active', 'stale')
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_active_requirements_for_work(conn: &Connection, work_unit_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        with relevant_requirements as (
            select distinct r.id
            from task_derivations td
            join tasks t on t.id = td.task_id
            join design_requirements r on r.id = td.design_requirement_id
            where t.work_unit_id = ?1 and td.status = 'active'
            union
            select distinct r.id
            from validation_gates vg
            join design_requirements r on r.id = vg.design_requirement_id
            left join tasks t on t.id = vg.task_id
            where coalesce(vg.work_unit_id, t.work_unit_id) = ?1
              and vg.status = 'active'
        )
        select count(*)
        from design_requirements r
        join relevant_requirements rr on rr.id = r.id
        where r.status = 'active'
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_active_requirements_missing_coverage_for_work(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        with relevant_requirements as (
            select distinct r.id
            from task_derivations td
            join tasks t on t.id = td.task_id
            join design_requirements r on r.id = td.design_requirement_id
            where t.work_unit_id = ?1 and td.status = 'active'
            union
            select distinct r.id
            from validation_gates vg
            join design_requirements r on r.id = vg.design_requirement_id
            left join tasks t on t.id = vg.task_id
            where coalesce(vg.work_unit_id, t.work_unit_id) = ?1
              and vg.status = 'active'
        )
        select count(*)
        from design_requirements r
        join relevant_requirements rr on rr.id = r.id
        where r.status = 'active'
          and not exists (
            select 1
            from coverage_items c
            left join tasks ct on ct.id = c.task_id
            where c.design_requirement_id = r.id
              and (
                ct.work_unit_id = ?1
                or (c.task_id is null and c.work_unit_id = ?1)
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
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_derived_tasks_missing_selected_gate_for_work(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join tasks t on t.id = td.task_id
        join design_requirements r on r.id = td.design_requirement_id
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where t.work_unit_id = ?1
          and td.status in ('active', 'stale')
          and t.status in ('closed', 'accepted_out_of_scope')
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'stale_record'
              and ar.stale_record_type = 'task_derivation'
              and ar.stale_record_id = td.id
              and ar.acceptance_type = 'stale_accepted'
              and ar.status = 'approved'
          )
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
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_open_checklist_items_for_work(conn: &Connection, work_unit_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from checklist_items ci
        join checklists c on c.id = ci.checklist_id
        where c.work_unit_id = ?1
          and c.status = 'active'
          and ci.status in ('open', 'blocked')
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_active_checklists_for_work(conn: &Connection, work_unit_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from checklists
        where work_unit_id = ?1
          and status = 'active'
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_closed_derived_tasks_missing_evidence_for_work(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join tasks t on t.id = td.task_id
        where t.work_unit_id = ?1
          and td.status in ('active', 'stale')
          and t.status = 'closed'
          and not exists (
            select 1
            from implementation_evidence e
            where e.task_id = td.task_id
              and e.design_requirement_id = td.design_requirement_id
          )
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_closed_derived_tasks_missing_coverage_for_work(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join tasks t on t.id = td.task_id
        where t.work_unit_id = ?1
          and td.status = 'active'
          and t.status = 'closed'
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
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_stale_design_records_for_work(conn: &Connection, work_unit_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        select count(distinct r.id)
        from task_derivations td
        join tasks t on t.id = td.task_id
        join design_requirements r on r.id = td.design_requirement_id
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where t.work_unit_id = ?1
          and td.status = 'active'
          and p.current_design_version_id != r.design_version_id
          and not exists (
            select 1
            from design_requirements current_r
            where current_r.design_version_id = p.current_design_version_id
              and current_r.requirement_key = r.requirement_key
              and current_r.requirement_hash = r.requirement_hash
              and current_r.status = 'active'
          )
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'stale_record'
              and ar.stale_record_type = 'task_derivation'
              and ar.stale_record_id = td.id
              and ar.acceptance_type = 'stale_accepted'
              and ar.status = 'approved'
          )
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_stale_task_derivations_for_work(conn: &Connection, work_unit_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join tasks t on t.id = td.task_id
        join design_requirements r on r.id = td.design_requirement_id
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where t.work_unit_id = ?1
          and td.status = 'active'
          and p.current_design_version_id != r.design_version_id
          and not exists (
            select 1
            from design_requirements current_r
            where current_r.design_version_id = p.current_design_version_id
              and current_r.requirement_key = r.requirement_key
              and current_r.requirement_hash = r.requirement_hash
              and current_r.status = 'active'
          )
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'stale_record'
              and ar.stale_record_type = 'task_derivation'
              and ar.stale_record_id = td.id
              and ar.acceptance_type = 'stale_accepted'
              and ar.status = 'approved'
          )
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_stale_checklists_for_work(conn: &Connection, work_unit_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        select count(distinct c.id)
        from checklists c
        join checklist_items ci on ci.checklist_id = c.id
        join design_requirements r on r.id = ci.design_requirement_id
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where c.work_unit_id = ?1
          and c.status in ('active', 'stale')
          and p.current_design_version_id != r.design_version_id
          and not exists (
            select 1
            from design_requirements current_r
            where current_r.design_version_id = p.current_design_version_id
              and current_r.requirement_key = r.requirement_key
              and current_r.requirement_hash = r.requirement_hash
              and current_r.status = 'active'
          )
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'stale_record'
              and ar.stale_record_type = 'checklist'
              and ar.stale_record_id = c.id
              and ar.acceptance_type = 'stale_accepted'
              and ar.status = 'approved'
          )
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_stale_selected_gates_for_work(conn: &Connection, work_unit_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from validation_gates vg
        join validation_gate_templates gt on gt.id = vg.template_id
        join design_requirements r on r.id = vg.design_requirement_id
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        left join tasks t on t.id = vg.task_id
        where coalesce(vg.work_unit_id, t.work_unit_id) = ?1
          and vg.status in ('active', 'stale')
          and (p.current_design_version_id != r.design_version_id
               or p.current_design_version_id != gt.design_version_id)
          and (
            not exists (
              select 1
              from design_requirements current_r
              where current_r.design_version_id = p.current_design_version_id
                and current_r.requirement_key = r.requirement_key
                and current_r.requirement_hash = r.requirement_hash
                and current_r.status = 'active'
            )
            or not exists (
              select 1
              from validation_gate_templates current_gt
              where current_gt.design_version_id = p.current_design_version_id
                and current_gt.gate_key = gt.gate_key
                and current_gt.gate_hash = gt.gate_hash
                and current_gt.status = 'active'
            )
          )
          and not exists (
            select 1
            from acceptance_records ar
            where (
                (
                  ar.target_type = 'validation_gate'
                  and ar.validation_gate_id = vg.id
                )
                or (
                  ar.target_type = 'stale_record'
                  and ar.stale_record_type = 'validation_gate'
                  and ar.stale_record_id = vg.id
                )
              )
              and ar.acceptance_type = 'stale_accepted'
              and ar.status = 'approved'
          )
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_stale_coverage_items_for_work(conn: &Connection, work_unit_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from coverage_items c
        join design_requirements r on r.id = c.design_requirement_id
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        left join tasks t on t.id = c.task_id
        where coalesce(c.work_unit_id, t.work_unit_id) = ?1
          and p.current_design_version_id != r.design_version_id
          and not exists (
            select 1
            from design_requirements current_r
            where current_r.design_version_id = p.current_design_version_id
              and current_r.requirement_key = r.requirement_key
              and current_r.requirement_hash = r.requirement_hash
              and current_r.status = 'active'
          )
          and not exists (
            select 1
            from acceptance_records ar
            where (
                (
                  ar.target_type = 'coverage_item'
                  and ar.coverage_item_id = c.id
                )
                or (
                  ar.target_type = 'stale_record'
                  and ar.stale_record_type = 'coverage_item'
                  and ar.stale_record_id = c.id
                )
              )
              and ar.acceptance_type = 'stale_accepted'
              and ar.status = 'approved'
          )
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

#[derive(Debug)]
struct StoredResumeCheck {
    id: i64,
    work_unit_id: i64,
    activation_id: i64,
    result: String,
    status: String,
    authority_event_high_watermark: Option<i64>,
    activation_stack_revision: Option<i64>,
    maturity: String,
    repository_snapshot_id: Option<i64>,
    repository_state_revision: Option<i64>,
}

struct ResumeGateEvaluation {
    work_unit_id: i64,
    activation_id: i64,
    suspend_snapshot_id: i64,
    resume_result: String,
    blocking_reason: Option<String>,
    allowed_next_action: Option<String>,
    authority_high_watermark: i64,
    activation_stack_revision: i64,
    repository_snapshot_id: Option<i64>,
    repository_state_revision: Option<i64>,
    items: Vec<ResumeReadyItem>,
}

struct TraceResumeCounts {
    stale_design_records: i64,
    stale_task_derivations: i64,
    stale_checklists: i64,
    stale_selected_gates: i64,
    stale_coverage_items: i64,
}

struct CloseTraceState {
    active_requirement_count: i64,
    derived_task_count: i64,
    missing_evidence_count: i64,
    missing_coverage_count: i64,
    missing_requirement_coverage_count: i64,
    missing_validation_gate_count: i64,
    open_checklist_item_count: i64,
    active_checklist_count: i64,
}

struct ValidationCloseState {
    selected_gate_count: i64,
    missing_run_count: i64,
    accepted_failure_count: i64,
    unaccepted_failure_count: i64,
}

struct CloseProcessState {
    applicable_rule_count: i64,
    rule_conflict_count: i64,
    fixed_command_count: i64,
    missing_fixed_command_usage_count: i64,
    invalid_commit_message_count: i64,
    repeated_correction_count: i64,
    open_kpt_review_count: i64,
    work_record_count: i64,
    work_record_evidence_link_count: i64,
}

struct RepositoryCloseState {
    repository_count: i64,
    missing_snapshot_count: i64,
    unclassified_dirty_state_count: i64,
    missing_comparison_count: i64,
    unclassified_comparison_count: i64,
}

#[derive(Default)]
struct ReviewPlanStageState {
    required_plan_count: i64,
    incomplete_required_plan_count: i64,
    missing_context_run_count: i64,
    stale_target_count: i64,
}

struct ReviewPlanTargetForResume {
    target_type: String,
    design_version_id: Option<i64>,
    design_requirement_id: Option<i64>,
    repository_snapshot_id: Option<i64>,
}

struct LifecycleWorkUnit {
    work_unit_id: i64,
    activation_id: Option<i64>,
    status: String,
}

#[derive(Default)]
struct RepositoryResumeState {
    repository_count: i64,
    base_snapshot_count: i64,
    missing_base_snapshot_count: i64,
    missing_current_snapshot_count: i64,
    missing_comparison_count: i64,
    unclassified_comparison_count: i64,
    unclassified_dirty_state_count: i64,
    latest_current_snapshot_id: Option<i64>,
}

struct StoredForkSource {
    source_work_unit_id: Option<i64>,
    source_work_unit_activation_id: Option<i64>,
    source_work_record_id: Option<i64>,
    source_repository_snapshot_id: Option<i64>,
    source_git_commit_id: Option<i64>,
    source_git_commit_sha: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct WorkOutcome {
    pub work_unit_id: i64,
    pub activation_id: i64,
}

pub struct WorkStart<'a> {
    pub title: &'a str,
    pub responsibility: Option<&'a str>,
    pub design_version_id: Option<i64>,
}

pub struct WorkActivate<'a> {
    pub work_unit_id: i64,
    pub design_version_id: Option<i64>,
    pub reason: Option<&'a str>,
}

pub struct WorkReopen<'a> {
    pub work_unit_id: i64,
    pub reason: &'a str,
    pub reason_type: &'a str,
    pub authority_event_id: Option<i64>,
    pub acceptance_record_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct WorkStatusOutcome {
    pub work_unit_id: i64,
    pub activation_id: Option<i64>,
    pub previous_status: String,
    pub status: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SuspendOutcome {
    pub work_unit_id: i64,
    pub activation_id: i64,
    pub suspend_snapshot_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct InterruptOutcome {
    pub parent_work_unit_id: i64,
    pub parent_activation_id: i64,
    pub parent_suspend_snapshot_id: i64,
    pub child_work_unit_id: i64,
    pub child_activation_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CloseOutcome {
    pub work_unit_id: i64,
    pub activation_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CloseReadyOutcome {
    pub work_unit_id: Option<i64>,
    pub activation_id: Option<i64>,
    pub result: String,
    pub blocking_reason: Option<String>,
    pub items: Vec<CloseReadyItem>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CloseReadyItem {
    pub name: String,
    pub result: String,
    pub blocking_action: Option<String>,
    pub details: String,
}

impl CloseReadyItem {
    fn pass(name: &str, details: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            result: "pass".to_string(),
            blocking_action: None,
            details: details.into(),
        }
    }

    fn fail(name: &str, blocking_action: &str, details: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            result: "fail".to_string(),
            blocking_action: Some(blocking_action.to_string()),
            details: details.into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct FollowUpOutcome {
    pub source_work_unit_id: i64,
    pub work_unit_id: i64,
    pub activation_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ResumeCheckOutcome {
    pub resume_check_id: i64,
    pub result: String,
    pub blocking_reason: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ResumeReadyOutcome {
    pub work_unit_id: Option<i64>,
    pub activation_id: Option<i64>,
    pub result: String,
    pub blocking_reason: Option<String>,
    pub items: Vec<ResumeReadyItem>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ResumeReadyItem {
    pub name: String,
    pub result: String,
    pub blocking_action: Option<String>,
    pub details: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ResumeOutcome {
    pub work_unit_id: i64,
    pub activation_id: i64,
}

pub struct NewWorkFork<'a> {
    pub title: &'a str,
    pub source: WorkForkSource<'a>,
    pub reason: &'a str,
    pub discard_policy: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkForkSource<'a> {
    Record(i64),
    Activation(i64),
    Commit(&'a str),
    GitCommit(i64),
    RepositorySnapshot(i64),
}

#[derive(Debug, PartialEq, Eq)]
pub struct WorkForkOutcome {
    pub fork_id: i64,
    pub work_unit_id: i64,
    pub activation_id: i64,
}
