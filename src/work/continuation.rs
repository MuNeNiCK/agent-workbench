use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use crate::db::{
    NewEvent, active_activation, current_phase_blocker, insert_event, max_id,
    open_existing_project, project_id,
};

use super::{forking::*, resume_validation::*, *};

pub fn resume_check_basic(root: &Path) -> Result<ResumeCheckOutcome> {
    resume_check(root, "basic")
}

pub fn resume_check(root: &Path, maturity: &str) -> Result<ResumeCheckOutcome> {
    resume_check_for(root, None, maturity)
}

pub fn resume_check_for(
    root: &Path,
    work_unit_id: Option<i64>,
    maturity: &str,
) -> Result<ResumeCheckOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let evaluation = evaluate_resume_ready_for(&tx, work_unit_id, maturity)?;

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
    resume_ready_for(root, None, maturity)
}

pub fn resume_ready_for(
    root: &Path,
    work_unit_id: Option<i64>,
    maturity: &str,
) -> Result<ResumeReadyOutcome> {
    let conn = open_existing_project(root)?;
    match evaluate_resume_ready_for(&conn, work_unit_id, maturity) {
        Ok(evaluation) => Ok(ResumeReadyOutcome {
            work_unit_id: Some(evaluation.work_unit_id),
            activation_id: Some(evaluation.activation_id),
            result: gate_result_for(&evaluation),
            blocking_reason: evaluation.blocking_reason,
            items: evaluation.items,
        }),
        Err(error) if is_resume_target_resolution_error(&error) => Ok(ResumeReadyOutcome {
            work_unit_id: None,
            activation_id: None,
            result: "blocked".to_string(),
            blocking_reason: Some(error.to_string()),
            items: vec![ResumeReadyItem {
                name: "resume_target_suspended".to_string(),
                result: "fail".to_string(),
                blocking_action: Some(
                    error
                        .to_string()
                        .lines()
                        .find_map(|line| line.strip_prefix("next: "))
                        .unwrap_or("suspend or complete current work before resuming")
                        .to_string(),
                ),
                details: error.to_string(),
            }],
        }),
        Err(error) => Err(error),
    }
}

pub(super) fn gate_result_for(evaluation: &ResumeGateEvaluation) -> String {
    if evaluation.resume_result == "allowed" {
        "pass".to_string()
    } else {
        "blocked".to_string()
    }
}

pub(super) fn ensure_resume_check_items_pass(
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

pub(super) fn required_resume_check_items(maturity: &str) -> Result<Vec<&'static str>> {
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

pub(super) fn is_no_resume_target_error(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.to_string() == "no suspended activation to resume")
}

fn is_resume_target_resolution_error(error: &anyhow::Error) -> bool {
    is_no_resume_target_error(error)
        || error
            .chain()
            .any(|cause| cause.to_string().starts_with("resume target unresolved:"))
}

pub fn resume_work(root: &Path, resume_check_id: i64) -> Result<ResumeOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    ensure_work_mutation_allowed(&tx, "work resume", None)?;

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
    let selected_recovery = current_phase_blocker(&tx)?.and_then(|blocker| {
        (blocker.kind == "required_review_finding"
            && blocker.work_unit_id == Some(input.work_unit_id)
            && blocker.next_action.contains("work reopen"))
        .then_some(blocker.finding_id)
        .flatten()
    });
    if selected_recovery.is_some() && input.authority_event_id.is_none() {
        bail!("selected terminal remediation recovery requires --authority");
    }
    ensure_work_mutation_allowed(
        &tx,
        "work reopen",
        Some((input.work_unit_id, "work reopen")),
    )?;
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
    let reopened_event_id = insert_event(
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
    let recovery_dependency_id = tx.last_insert_rowid();
    if let Some(finding_id) = selected_recovery {
        let closure_id: i64 = tx.query_row(
            "select id from closures where finding_id = ?1 and status = 'registered' order by id desc limit 1",
            params![finding_id],
            |row| row.get(0),
        )?;
        tx.execute(
            r#"
            insert into finding_remediation_recovery_epochs(
                project_id, finding_id, closure_id, work_unit_id,
                work_unit_activation_id, dependency_id, reopened_event_id,
                authority_event_id, created_at
            ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, current_timestamp)
            "#,
            params![
                project_id,
                finding_id,
                closure_id,
                input.work_unit_id,
                activation_id,
                recovery_dependency_id,
                reopened_event_id,
                input.authority_event_id.unwrap()
            ],
        )?;
    }
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
    ensure_work_mutation_allowed(&tx, "work follow-up", None)?;

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

pub(super) fn validate_reopen_reason_type(reason_type: &str) -> Result<()> {
    match reason_type {
        "closure_invalid" | "closure_incomplete" | "authority_superseded" => Ok(()),
        _ => bail!(
            "reopen reason type must be closure_invalid, closure_incomplete, or authority_superseded"
        ),
    }
}

pub(super) fn ensure_reopen_authority(
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
