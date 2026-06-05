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
            allowed_next_action, blocking_reason, created_at
        )
        values (?1, ?2, ?3, ?4, 'pending', ?5, ?6, ?7, ?8, ?9, current_timestamp)
        "#,
        params![
            evaluation.work_unit_id,
            evaluation.activation_id,
            evaluation.suspend_snapshot_id,
            maturity,
            evaluation.resume_result,
            evaluation.authority_high_watermark,
            evaluation.activation_stack_revision,
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

    let source = resolve_fork_source(&tx, input.source)?;
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
            source_work_record_id, source_git_commit_sha, forked_work_unit_id,
            fork_reason, discard_policy, status, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'open', current_timestamp)
        "#,
        params![
            project_id,
            source.source_work_unit_id,
            source.source_work_unit_activation_id,
            source.source_work_record_id,
            source.source_git_commit_sha,
            forked_work_unit_id,
            input.reason,
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

fn resolve_fork_source(conn: &Connection, source: WorkForkSource<'_>) -> Result<StoredForkSource> {
    match source {
        WorkForkSource::Record(work_record_id) => {
            let source_work_unit_id = conn
                .query_row(
                    "select work_unit_id from work_records where id = ?1",
                    params![work_record_id],
                    |row| row.get::<_, Option<i64>>(0),
                )
                .optional()?
                .context("source work record not found")?;

            Ok(StoredForkSource {
                source_work_unit_id,
                source_work_unit_activation_id: None,
                source_work_record_id: Some(work_record_id),
                source_git_commit_sha: None,
            })
        }
        WorkForkSource::Activation(activation_id) => {
            let work_unit_id = conn
                .query_row(
                    "select work_unit_id from work_unit_activations where id = ?1",
                    params![activation_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .context("source activation not found")?;

            Ok(StoredForkSource {
                source_work_unit_id: Some(work_unit_id),
                source_work_unit_activation_id: Some(activation_id),
                source_work_record_id: None,
                source_git_commit_sha: None,
            })
        }
        WorkForkSource::Commit(commit_sha) => Ok(StoredForkSource {
            source_work_unit_id: None,
            source_work_unit_activation_id: None,
            source_work_record_id: None,
            source_git_commit_sha: Some(commit_sha.to_string()),
        }),
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

    let later_items = [
        (
            "design_version_current",
            "trace-aware design version check is not implemented yet",
        ),
        (
            "task_derivation_current",
            "trace-aware task derivation check is not implemented yet",
        ),
        (
            "checklist_current",
            "trace-aware checklist check is not implemented yet",
        ),
        (
            "selected_gate_current",
            "trace-aware validation gate check is not implemented yet",
        ),
        (
            "review_plan_current",
            "trace-aware review plan check is not implemented yet",
        ),
        (
            "repository_state_current",
            "repo-aware repository state check is not implemented yet",
        ),
        (
            "assumptions_current",
            "repo-aware assumptions check is not implemented yet",
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

    if maturity != "basic" {
        blocking_reason.get_or_insert_with(|| format!("{maturity} checks are not implemented yet"));
    }
    let allowed = basic_allowed && maturity == "basic";
    Ok(ResumeGateEvaluation {
        work_unit_id: target.work_unit_id,
        activation_id: target.activation_id,
        suspend_snapshot_id: snapshot.id,
        resume_result: if allowed { "allowed" } else { "blocked" }.to_string(),
        blocking_reason,
        allowed_next_action: Some(snapshot.next_action),
        authority_high_watermark,
        activation_stack_revision: stack_revision,
        items,
    })
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
    items: Vec<ResumeReadyItem>,
}

struct StoredForkSource {
    source_work_unit_id: Option<i64>,
    source_work_unit_activation_id: Option<i64>,
    source_work_record_id: Option<i64>,
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
}

#[derive(Debug, PartialEq, Eq)]
pub struct WorkForkOutcome {
    pub fork_id: i64,
    pub work_unit_id: i64,
    pub activation_id: i64,
}
