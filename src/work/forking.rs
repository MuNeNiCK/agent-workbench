use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use crate::db::{
    NewEvent, StoredActivation, active_activation, current_phase_blocker, insert_event,
    open_existing_project, project_id, suspended_activation,
};

use super::*;

pub fn fork_work(root: &Path, input: NewWorkFork<'_>) -> Result<WorkForkOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    ensure_work_mutation_allowed(&tx, "work fork", None)?;

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

pub(super) fn suspend_active_activation(
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

pub(super) fn csv_query(
    conn: &Connection,
    sql: &str,
    values: &[&dyn rusqlite::ToSql],
) -> Result<String> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(values, |row| row.get::<_, String>(0))?;
    let mut collected = Vec::new();
    for row in rows {
        collected.push(row?);
    }
    Ok(collected.join(","))
}

pub(super) fn snapshot_active_task_ids(conn: &Connection, work_unit_id: i64) -> Result<String> {
    csv_query(
        conn,
        "select cast(id as text) from current_tasks where work_unit_id = ?1 and status = 'open' order by id",
        &[&work_unit_id],
    )
}

pub(super) fn snapshot_selected_gate_id(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<Option<i64>> {
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

pub(super) fn snapshot_authority_refs(conn: &Connection) -> Result<String> {
    csv_query(
        conn,
        "select cast(id as text) || ':' || event_type from authority_events where status = 'active' order by id",
        &[],
    )
}

pub(super) fn snapshot_review_scope_refs(conn: &Connection, work_unit_id: i64) -> Result<String> {
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

pub(super) fn snapshot_repository_heads(conn: &Connection) -> Result<String> {
    csv_query(
        conn,
        "select name || ':' || coalesce(current_head, '') from repositories order by name",
        &[],
    )
}

pub(super) fn snapshot_repository_snapshot_ids(
    conn: &Connection,
    activation_id: i64,
) -> Result<String> {
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

pub(super) fn snapshot_repository_status(conn: &Connection) -> Result<String> {
    csv_query(
        conn,
        "select name || ':' || coalesce(status_summary, '') from repositories order by name",
        &[],
    )
}

pub(super) fn snapshot_dirty_state_summary(
    conn: &Connection,
    activation_id: i64,
) -> Result<String> {
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

pub(super) fn snapshot_open_findings(conn: &Connection, work_unit_id: i64) -> Result<String> {
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

pub(super) fn snapshot_assumptions(conn: &Connection, work_unit_id: i64) -> Result<String> {
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

pub(super) fn snapshot_entries_still_current(stored: &str, current: &str) -> bool {
    stored
        .split(',')
        .filter(|entry| !entry.is_empty())
        .all(|entry| {
            current
                .split(',')
                .any(|current_entry| current_entry == entry)
        })
}

pub(super) fn update_work_unit_lifecycle(
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
    let selected = match event_type {
        "unblocked" => Some((target.work_unit_id, "work unblock")),
        "blocked" => Some((target.work_unit_id, "work block")),
        _ => None,
    };
    ensure_work_mutation_allowed(&tx, event_type, selected)?;
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

pub(super) fn ensure_work_mutation_allowed(
    conn: &Connection,
    operation: &str,
    selected_owner_action: Option<(i64, &str)>,
) -> Result<()> {
    let requested_release: Option<String> = conn
        .query_row(
            r#"
            select candidate.candidate_handle
            from release_candidate_attempts attempt
            join release_candidates candidate on candidate.id=attempt.release_candidate_id
            where attempt.project_id=candidate.project_id and attempt.status='requested'
            order by attempt.id limit 1
            "#,
            [],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(candidate) = requested_release {
        bail!(
            "{operation} is blocked by requested release attempt for {candidate}; next: agent-workbench operator release candidate inspect"
        );
    }
    if let Some(blocker) = current_phase_blocker(conn)? {
        let unrelated_owner = match (blocker.work_unit_id, selected_owner_action) {
            (Some(_), None) => true,
            (Some(blocker_owner), Some((target_owner, command))) => {
                blocker_owner != target_owner
                    && !action_selects_work(&blocker.next_action, command, target_owner)
            }
            _ => false,
        };
        if unrelated_owner {
            return Ok(());
        }
        let selected = selected_owner_action.is_some_and(|(work_unit_id, command)| {
            blocker.next_action.contains(command)
                && (blocker.work_unit_id == Some(work_unit_id)
                    || action_selects_work(&blocker.next_action, command, work_unit_id))
        });
        let implicit_selected_recovery = blocker.kind == "finding_remediation_recovery"
            && blocker
                .next_action
                .contains(&format!("agent-workbench {operation}"));
        if (selected || implicit_selected_recovery)
            && blocker.kind == "finding_remediation_recovery"
        {
            return Ok(());
        }
        if !selected {
            bail!(
                "{operation} is blocked by the selected lifecycle action; next: {}",
                blocker.next_action
            );
        }
    }
    let scoped_change: Option<(String, i64, i64, Option<i64>)> = conn
        .query_row(
            r#"
            select 'finding_remediation', b.finding_id, b.closure_id, b.work_unit_id
            from finding_remediation_bindings b
            join findings f on f.id = b.finding_id and f.status = 'open' and f.classification = 'valid'
            join closures c on c.id = b.closure_id and c.status = 'registered'
            join work_unit_activations a on a.id = b.work_unit_activation_id and a.status = 'active'
            join review_runs r on r.id=f.review_run_id
            join review_plans p on p.id=r.review_plan_id
            where b.project_id = (select id from projects order by id limit 1)
              and p.required=1 and p.stage='close-ready'
              and p.review_type in ('implementation_review','design_implementation_diff')
              and p.status not in ('exhausted','needs_user_decision')
              and not exists(
                select 1 from correction_tokens token where token.closure_id=c.id
              )
              and not exists(
                select 1 from acceptance_records accepted
                where accepted.finding_id=f.id and accepted.target_type='finding'
                  and accepted.status='approved'
                  and accepted.acceptance_type in (
                    'accepted_out_of_scope','explicit_exception','classified_failure'
                  )
              )
            union all
            select 'source_correction', s.finding_id, s.closure_id, null
            from correction_sessions s
            join findings f on f.id = s.finding_id and f.status = 'open' and f.classification = 'valid'
            join closures c on c.id = s.closure_id and c.status = 'registered'
            where s.status = 'active'
              and s.project_id = (select id from projects order by id limit 1)
            order by 2,3
            limit 1
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    if let Some((kind, finding_id, closure_id, work_unit_id)) = scoped_change {
        let permitted_alternate = kind == "finding_remediation"
            && selected_owner_action.is_some_and(|(target, command)| {
                work_unit_id == Some(target)
                    && matches!(
                        command,
                        "work suspend" | "work block" | "work unblock" | "work abandon"
                    )
            });
        if permitted_alternate {
            return Ok(());
        }
        bail!(
            "{operation} is forbidden during {kind} for finding {finding_id}; finish with agent-workbench closure ready {closure_id}"
        );
    }
    Ok(())
}

pub(super) fn action_selects_work(next_action: &str, command: &str, work_unit_id: i64) -> bool {
    let needle = format!("{command} {work_unit_id}");
    next_action.match_indices(&needle).any(|(index, _)| {
        next_action[index + needle.len()..]
            .chars()
            .next()
            .is_none_or(|next| !next.is_ascii_digit())
    })
}

pub(super) fn resolve_lifecycle_work_unit(
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

pub(super) fn lifecycle_activation_for_work(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<Option<i64>> {
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

pub(super) fn prepare_parent_frame(
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

pub(super) fn resolve_fork_source(
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

pub(super) fn fork_reason_code(reason: &str) -> &str {
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

pub(super) fn dependency_type_for_fork_reason(reason: &str) -> &'static str {
    match reason {
        "design_changed" | "agent_drift" | "failed_validation" | "user_requested_redo" => {
            "supersedes"
        }
        "invalid_assumption" => "invalidates_assumption",
        _ => "discovered_by",
    }
}
