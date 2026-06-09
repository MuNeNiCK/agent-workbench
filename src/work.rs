use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use crate::db::{
    NewEvent, StoredActivation, active_activation, insert_event, max_id, open_existing_project,
    project_id, suspend_snapshot, suspended_activation,
};

pub fn start_work(root: &Path, title: &str, responsibility: Option<&str>) -> Result<WorkOutcome> {
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
        params![project_id, title, responsibility],
    )?;
    let work_unit_id = tx.last_insert_rowid();

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
            reason: responsibility,
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
    items.push(
        if validation.missing_run_count == 0 && validation.unaccepted_failure_count == 0 {
            CloseReadyItem::pass(
                "validation_runs_recorded",
                format!(
                    "{} selected gates, {} missing runs, {} accepted failures, {} unaccepted failures",
                    validation.selected_gate_count,
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
                    "{} selected gates, {} missing runs, {} accepted failures, {} unaccepted failures",
                    validation.selected_gate_count,
                    validation.missing_run_count,
                    validation.accepted_failure_count,
                    validation.unaccepted_failure_count
                ),
            )
        },
    );
    items.push(
        if repository.repository_count == 0
            || (repository.missing_snapshot_count == 0
                && repository.unclassified_dirty_state_count == 0
                && repository.missing_comparison_count == 0
                && repository.unclassified_comparison_count == 0)
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
        if review.required_plan_count == 0
            || (review.incomplete_required_plan_count == 0 && review.stale_target_count == 0)
        {
            CloseReadyItem::pass(
                "review_plans_clean",
                format!(
                    "{} required close-ready plans, {} incomplete, {} stale targets",
                    review.required_plan_count,
                    review.incomplete_required_plan_count,
                    review.stale_target_count
                ),
            )
        } else {
            CloseReadyItem::fail(
                "review_plans_clean",
                "complete required close-ready plans or refresh stale targets",
                format!(
                    "{} required close-ready plans, {} incomplete, {} stale targets",
                    review.required_plan_count,
                    review.incomplete_required_plan_count,
                    review.stale_target_count
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
            repository_snapshot_id, allowed_next_action, blocking_reason, created_at
        )
        values (?1, ?2, ?3, ?4, 'pending', ?5, ?6, ?7, ?8, ?9, ?10, current_timestamp)
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
                   authority_event_high_watermark, activation_stack_revision
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
                })
            },
        )
        .optional()?
        .context("resume check not found")?;

    if check.status != "pending" || check.result != "allowed" {
        bail!("resume check must be pending and allowed");
    }
    if active_activation(&tx)?.is_some() {
        bail!("cannot resume while another activation is active");
    }
    if max_id(&tx, "authority_events")? != check.authority_event_high_watermark.unwrap_or(0)
        || max_id(&tx, "work_unit_events")? != check.activation_stack_revision.unwrap_or(0)
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

pub fn reopen_work(root: &Path, work_unit_id: i64, reason: &str) -> Result<WorkOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;

    let status = tx
        .query_row(
            "select status from work_units where id = ?1 and project_id = ?2",
            params![work_unit_id, project_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .context("work unit not found")?;
    if status != "closed" && status != "abandoned" {
        bail!("only closed or abandoned work units can be reopened");
    }

    let parent = prepare_parent_frame(
        &tx,
        reason,
        &format!("resume after reopening work unit {work_unit_id}"),
    )?;

    tx.execute(
        "update work_units set status = 'open', closed_at = null where id = ?1",
        params![work_unit_id],
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
            work_unit_id,
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
            work_unit_id,
            activation_id: Some(activation_id),
            related_activation_id: parent.as_ref().map(|activation| activation.activation_id),
            event_type: "reopened",
            reason: Some(reason),
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
        values (?1, ?1, 'invalidates_closure', ?2, 'resolved', current_timestamp)
        "#,
        params![work_unit_id, reason],
    )?;
    tx.commit()?;

    Ok(WorkOutcome {
        work_unit_id,
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
    tx.commit()?;

    Ok(FollowUpOutcome {
        source_work_unit_id,
        work_unit_id: follow_up_work_unit_id,
        activation_id,
    })
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
            work_unit_activation_id, work_unit_id, reason, next_action, created_at
        )
        values (?1, ?2, ?3, ?4, current_timestamp)
        "#,
        params![
            active.activation_id,
            active.work_unit_id,
            reason,
            next_action
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
        let stale_design_total =
            trace_counts.stale_design_records + trace_counts.stale_coverage_items;
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
                trace_counts.stale_selected_gates == 0,
                format!(
                    "{} selected validation gates reference changed requirements",
                    trace_counts.stale_selected_gates
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
    if repo_maturity {
        let repo_state = repository_resume_state(conn, &target)?;
        repository_snapshot_id = repo_state.latest_current_snapshot_id;
        let pass = repo_state.repository_count == 0
            || (repo_state.missing_base_snapshot_count == 0
                && repo_state.missing_current_snapshot_count == 0
                && repo_state.missing_comparison_count == 0
                && repo_state.unclassified_comparison_count == 0
                && repo_state.unclassified_dirty_state_count == 0);
        if !pass {
            repo_allowed = false;
            blocking_reason.get_or_insert_with(|| "repo-aware resume checks failed".to_string());
        }
        items.push(ResumeReadyItem {
            name: "repository_state_current".to_string(),
            result: if pass { "pass" } else { "fail" }.to_string(),
            blocking_action: (!pass).then_some(
                "record current repository snapshots and classify resume differences".to_string(),
            ),
            details: format!(
                "{} repositories, {} suspend snapshots, {} missing base snapshots, {} missing current snapshots, {} missing comparisons, {} unclassified comparisons, {} unclassified dirty states",
                repo_state.repository_count,
                repo_state.base_snapshot_count,
                repo_state.missing_base_snapshot_count,
                repo_state.missing_current_snapshot_count,
                repo_state.missing_comparison_count,
                repo_state.unclassified_comparison_count,
                repo_state.unclassified_dirty_state_count
            ),
        });
    } else {
        items.push(ResumeReadyItem {
            name: "repository_state_current".to_string(),
            result: "not_checked".to_string(),
            blocking_action: None,
            details: "repo-aware repository state check was not requested".to_string(),
        });
    }

    if repo_maturity {
        let invalidated_assumptions = open_assumption_invalidations(conn, target.work_unit_id)?;
        let pass = invalidated_assumptions == 0;
        if !pass {
            repo_allowed = false;
            blocking_reason.get_or_insert_with(|| "repo-aware resume checks failed".to_string());
        }
        items.push(ResumeReadyItem {
            name: "assumptions_current".to_string(),
            result: if pass { "pass" } else { "fail" }.to_string(),
            blocking_action: (!pass)
                .then_some("resolve open assumption invalidation dependencies".to_string()),
            details: format!("{invalidated_assumptions} open assumption invalidations"),
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
        items,
    })
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
                    from acceptance_records ar
                    where ar.target_type = 'validation_gate_template'
                      and ar.validation_gate_template_id = vg.template_id
                      and ar.acceptance_type = 'explicit_exception'
                      and ar.status = 'approved'
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
        select id, status
        from review_plans
        where work_unit_id = ?1
          and stage = ?2
          and required = 1
        order by id
        "#,
    )?;
    let rows = stmt.query_map(params![work_unit_id, stage], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut state = ReviewPlanStageState::default();
    for row in rows {
        let (review_plan_id, status) = row?;
        state.required_plan_count += 1;
        if !matches!(
            status.as_str(),
            "clean" | "accepted_exception" | "not_required"
        ) {
            state.incomplete_required_plan_count += 1;
        }
        state.stale_target_count += stale_review_plan_target_count(conn, review_plan_id)?;
    }
    Ok(state)
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
          and c.status = 'active'
          and p.current_design_version_id != r.design_version_id
          and not exists (
            select 1
            from design_requirements current_r
            where current_r.design_version_id = p.current_design_version_id
              and current_r.requirement_key = r.requirement_key
              and current_r.requirement_hash = r.requirement_hash
              and current_r.status = 'active'
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
          and vg.status = 'active'
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
    items: Vec<ResumeReadyItem>,
}

struct TraceResumeCounts {
    stale_design_records: i64,
    stale_task_derivations: i64,
    stale_checklists: i64,
    stale_selected_gates: i64,
    stale_coverage_items: i64,
}

struct ValidationCloseState {
    selected_gate_count: i64,
    missing_run_count: i64,
    accepted_failure_count: i64,
    unaccepted_failure_count: i64,
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
    stale_target_count: i64,
}

struct ReviewPlanTargetForResume {
    target_type: String,
    design_version_id: Option<i64>,
    design_requirement_id: Option<i64>,
    repository_snapshot_id: Option<i64>,
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
