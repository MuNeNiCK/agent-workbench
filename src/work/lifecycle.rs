use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use crate::db::{NewEvent, active_activation, insert_event, open_existing_project, project_id};
use crate::identity::{CanonicalValue, domain_digest};

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
    let work_unit_id = {
        let conn = open_existing_project(root)?;
        active_activation(&conn)?
            .context("no active activation to close")?
            .work_unit_id
    };
    let outcome = close_work(root, Some(work_unit_id), summary, commit)?;
    Ok(CloseOutcome {
        work_unit_id: outcome.work_unit_id,
        activation_id: outcome
            .activation_id
            .context("active-work close completed without its activation")?,
    })
}

/// Close one exact work owner. Omitting the owner is an installed compatibility
/// adapter that succeeds only when exactly one open work owner exists.
pub fn close_work(
    root: &Path,
    work_unit_id: Option<i64>,
    summary: &str,
    commit: Option<&str>,
) -> Result<CloseWorkOutcome> {
    if summary.trim().is_empty() {
        bail!("close summary must not be empty");
    }
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let project = project_id(&tx)?;
    let target = resolve_close_target(&tx, project, work_unit_id, summary, commit)?;
    ensure_work_mutation_allowed(&tx, "work close", Some((target.work_unit_id, "work close")))?;
    if target.status != "open" {
        let next = if target.status == "blocked" {
            format!(
                "agent-workbench work unblock {} --reason \"<reason>\"",
                target.work_unit_id
            )
        } else {
            format!("agent-workbench status --work {}", target.work_unit_id)
        };
        bail!(
            "work unit {} is {}; next: {next}",
            target.work_unit_id,
            target.status
        );
    }
    let activation = current_activation_for_work(&tx, project, target.work_unit_id)?;
    if activation
        .as_ref()
        .is_some_and(|activation| activation.status == "suspended")
    {
        bail!(
            "work unit {} is suspended and was not changed\nnext: agent-workbench resume-check {} --maturity trace-aware\nthen: agent-workbench work resume --check <resume-check-id>",
            target.work_unit_id,
            target.work_unit_id
        );
    }
    let readiness = close_ready_in(&tx, project, target.work_unit_id, activation.as_ref())?;
    if readiness.result != "pass" {
        let reason = readiness
            .blocking_reason
            .as_deref()
            .unwrap_or("close-ready checks failed");
        bail!(
            "cannot close work unit {}; {reason}; next: agent-workbench gate close-ready {} --dry-run",
            target.work_unit_id,
            target.work_unit_id
        );
    }
    let open_tasks = tx.query_row(
        "select count(*) from current_tasks where work_unit_id = ?1 and status in ('open', 'blocked')",
        params![target.work_unit_id],
        |row| row.get::<_, i64>(0),
    )?;
    if open_tasks > 0 {
        bail!("cannot close work unit with open or blocked tasks");
    }
    let close_summary = match commit {
        Some(commit) => format!("{summary}\ncommit: {commit}"),
        None => summary.to_string(),
    };

    let changed = tx.execute(
        "update work_units set status = 'closed', closed_at = current_timestamp, close_summary = ?1 where id = ?2 and project_id=?3 and status='open'",
        params![close_summary, target.work_unit_id, project],
    )?;
    if changed != 1 {
        bail!(
            "work unit {} changed while closing; next: agent-workbench status --work {}",
            target.work_unit_id,
            target.work_unit_id
        );
    }
    if activation.is_some() {
        let changed = tx.execute(
            "update work_unit_activations set status='completed',completed_at=coalesce(completed_at,current_timestamp) where project_id=?1 and work_unit_id=?2 and status in ('active','suspended')",
            params![project, target.work_unit_id],
        )?;
        if changed == 0 {
            bail!(
                "activation changed while closing work unit {}; next: agent-workbench status --work {}",
                target.work_unit_id,
                target.work_unit_id
            );
        }
    }

    let reason = commit
        .map(|commit| format!("{summary}; commit {commit}"))
        .unwrap_or_else(|| summary.to_string());
    let event_id = insert_event(
        &tx,
        NewEvent {
            work_unit_id: target.work_unit_id,
            activation_id: activation.as_ref().map(|item| item.activation_id),
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
        params![event_id, target.work_unit_id],
    )?;

    let boundary_handle = format!(
        "boundary_{}",
        domain_digest(
            b"agent-workbench:work-close-boundary-v1\0",
            &CanonicalValue::object([
                ("work", CanonicalValue::Integer(target.work_unit_id)),
                ("event", CanonicalValue::Integer(event_id))
            ])
        )
    );
    let dependency_rows=tx.prepare("select 'task:'||id||':'||status from current_tasks where work_unit_id=?1 union all select 'gate:'||vg.id||':'||vg.status from validation_gates vg where vg.work_unit_id=?1 and vg.project_id=?2 order by 1")?.query_map(params![target.work_unit_id,project],|row|row.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
    let dependency_digest = domain_digest(
        b"agent-workbench:boundary-dependencies-v1\0",
        &CanonicalValue::Array(
            dependency_rows
                .into_iter()
                .map(CanonicalValue::String)
                .collect(),
        ),
    );
    let decisions=tx.prepare("select od.id,od.decision_handle from owner_decisions od join review_adjudication_decisions d on d.owner_decision_id=od.id join review_runs r on r.id=d.review_run_id join review_plans p on p.id=r.review_plan_id where od.project_id=?1 and p.work_unit_id=?2 and d.value='accepted' and not exists(select 1 from review_adjudication_decisions n where n.predecessor_id=d.id) order by od.id")?.query_map(params![project,target.work_unit_id],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (decision_id, decision_handle) in decisions {
        let snapshot_handle = format!(
            "snapshot_{}",
            domain_digest(
                b"agent-workbench:review-boundary-snapshot-v1\0",
                &CanonicalValue::object([
                    ("boundary", CanonicalValue::string(&boundary_handle)),
                    ("decision", CanonicalValue::string(&decision_handle)),
                    ("dependencies", CanonicalValue::string(&dependency_digest))
                ])
            )
        );
        tx.execute("insert into review_boundary_snapshots(project_id,owner_ref,boundary_handle,snapshot_handle,historical_owner_decision_id,dependency_digest,status,created_at) values(?1,?2,?3,?4,?5,?6,'current',current_timestamp)",params![project,format!("work_unit:{}",target.work_unit_id),boundary_handle,snapshot_handle,decision_id,dependency_digest])?;
    }

    tx.commit()?;

    Ok(CloseWorkOutcome {
        work_unit_id: target.work_unit_id,
        activation_id: activation.map(|item| item.activation_id),
    })
}

fn resolve_close_target(
    conn: &Connection,
    project: i64,
    requested: Option<i64>,
    summary: &str,
    commit: Option<&str>,
) -> Result<LifecycleWorkUnit> {
    if let Some(work_unit_id) = requested {
        return resolve_lifecycle_work_unit(conn, project, Some(work_unit_id));
    }
    let mut stmt = conn
        .prepare("select id from work_units where project_id=?1 and status='open' order by id")?;
    let candidates = stmt
        .query_map(params![project], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    match candidates.as_slice() {
        [work_unit_id] => resolve_lifecycle_work_unit(conn, project, Some(*work_unit_id)),
        [] => bail!("no open work owner can be closed; next: agent-workbench status"),
        _ => {
            let actions = candidates
                .into_iter()
                .map(|work_unit_id| qualified_close_command(work_unit_id, summary, commit))
                .map(|action| format!("next: {action}"))
                .collect::<Vec<_>>()
                .join("\n");
            bail!(
                "work close requires an explicit owner because {} open owners are eligible\n{actions}",
                actions.lines().count()
            )
        }
    }
}

fn qualified_close_command(work_unit_id: i64, summary: &str, commit: Option<&str>) -> String {
    let mut command = format!(
        "agent-workbench work close {work_unit_id} --summary {}",
        shell_quote(summary)
    );
    if let Some(commit) = commit {
        command.push_str(" --commit ");
        command.push_str(&shell_quote(commit));
    }
    command
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn current_activation_for_work(
    conn: &Connection,
    project: i64,
    work_unit_id: i64,
) -> Result<Option<crate::db::StoredActivation>> {
    let mut stmt = conn.prepare(
        r#"
        select id,project_id,work_unit_id,stack_depth,status
        from work_unit_activations
        where work_unit_id=?1 and status in ('active','suspended')
        order by case status when 'active' then 0 else 1 end,
                 stack_depth desc,
                 id desc
        "#,
    )?;
    let activations = stmt
        .query_map(params![work_unit_id], |row| {
            Ok(crate::db::StoredActivation {
                activation_id: row.get(0)?,
                project_id: row.get(1)?,
                work_unit_id: row.get(2)?,
                stack_depth: row.get(3)?,
                status: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let active_count = activations
        .iter()
        .filter(|activation| activation.status == "active")
        .count();
    let suspended_count = activations
        .iter()
        .filter(|activation| activation.status == "suspended")
        .count();
    if active_count > 1
        || (active_count == 0 && suspended_count > 1)
        || activations
            .first()
            .is_some_and(|activation| activation.project_id != project)
    {
        bail!(
            "project integrity blocked: work unit {work_unit_id} has an invalid current activation relation; next: agent-workbench status --work {work_unit_id}"
        );
    }
    Ok(activations.into_iter().next())
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
    close_ready_in(&conn, active.project_id, active.work_unit_id, Some(&active))
}

/// Evaluate close readiness for one exact work owner without consulting an
/// unrelated active owner.
pub fn close_ready_for(root: &Path, work_unit_id: i64) -> Result<CloseReadyOutcome> {
    let conn = open_existing_project(root)?;
    let project = project_id(&conn)?;
    close_ready_for_in(&conn, project, work_unit_id)
}

pub(crate) fn close_ready_for_in(
    conn: &Connection,
    project: i64,
    work_unit_id: i64,
) -> Result<CloseReadyOutcome> {
    let target = resolve_lifecycle_work_unit(conn, project, Some(work_unit_id))?;
    if target.status != "open" {
        let action = if target.status == "blocked" {
            format!("agent-workbench work unblock {work_unit_id} --reason \"<reason>\"")
        } else {
            format!("agent-workbench status --work {work_unit_id}")
        };
        return Ok(blocked_close_ready(
            work_unit_id,
            target.activation_id,
            "work_owner_open",
            action,
            format!("work unit is {}", target.status),
        ));
    }
    let activation = current_activation_for_work(conn, project, work_unit_id)?;
    if activation
        .as_ref()
        .is_some_and(|activation| activation.status == "suspended")
    {
        return Ok(blocked_close_ready(
            work_unit_id,
            activation.as_ref().map(|item| item.activation_id),
            "target_activation_closeable",
            format!("agent-workbench resume-check {work_unit_id} --maturity trace-aware"),
            "target activation is suspended; resume it before closing",
        ));
    }
    close_ready_in(conn, project, work_unit_id, activation.as_ref())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReleaseWorkBoundary {
    pub(crate) work_unit_id: i64,
    pub(crate) activation_id: Option<i64>,
    pub(crate) design_version_id: Option<i64>,
    pub(crate) repository_snapshot_id: i64,
    pub(crate) identity: String,
}

pub(crate) fn resolve_release_work_boundary(
    conn: &Connection,
    project: i64,
    root: &Path,
    requested_work: Option<i64>,
    reviewed_commit: &str,
) -> Result<ReleaseWorkBoundary> {
    if let Some(work_unit_id) = requested_work {
        return release_work_boundary_for(conn, project, root, work_unit_id, reviewed_commit)?
            .with_context(|| {
                format!(
                    "work unit {work_unit_id} is not close-ready for reviewed commit {reviewed_commit}; next: agent-workbench gate close-ready {work_unit_id} --dry-run"
                )
            });
    }

    let work_units = conn
        .prepare(
            "select id from work_units where project_id=?1 and status in ('open','closed') order by id",
        )?
        .query_map(params![project], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut candidates = Vec::new();
    for work_unit_id in work_units {
        if let Some(boundary) =
            release_work_boundary_for(conn, project, root, work_unit_id, reviewed_commit)?
        {
            candidates.push(boundary);
        }
    }
    match candidates.len() {
        1 => Ok(candidates.remove(0)),
        0 => bail!(
            "release assembly found no close-ready work for reviewed commit {reviewed_commit}; next: agent-workbench status"
        ),
        _ => bail!(
            "release assembly work is ambiguous; use one of: {}",
            candidates
                .iter()
                .map(|candidate| format!(
                    "agent-workbench operator release candidate assemble --work {} --version <version> --commit {reviewed_commit} --expected-current absent --idempotency-key <key>",
                    candidate.work_unit_id
                ))
                .collect::<Vec<_>>()
                .join("; ")
        ),
    }
}

pub(crate) fn resolve_release_work_boundary_for_root(
    root: &Path,
    requested_work: Option<i64>,
    reviewed_commit: &str,
) -> Result<ReleaseWorkBoundary> {
    let conn = open_existing_project(root)?;
    let project = project_id(&conn)?;
    resolve_release_work_boundary(&conn, project, root, requested_work, reviewed_commit)
}

fn release_work_boundary_for(
    conn: &Connection,
    project: i64,
    root: &Path,
    work_unit_id: i64,
    reviewed_commit: &str,
) -> Result<Option<ReleaseWorkBoundary>> {
    let status = conn
        .query_row(
            "select status from work_units where id=?1 and project_id=?2",
            params![work_unit_id, project],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(status) = status else {
        return Ok(None);
    };
    let activation = match status.as_str() {
        "open" => current_activation_for_work(conn, project, work_unit_id)?,
        "closed" => conn
            .query_row(
                r#"
                select id,project_id,work_unit_id,stack_depth,status
                from work_unit_activations
                where project_id=?1 and work_unit_id=?2 and status='completed'
                order by id desc limit 1
                "#,
                params![project, work_unit_id],
                |row| {
                    Ok(crate::db::StoredActivation {
                        activation_id: row.get(0)?,
                        project_id: row.get(1)?,
                        work_unit_id: row.get(2)?,
                        stack_depth: row.get(3)?,
                        status: row.get(4)?,
                    })
                },
            )
            .optional()?,
        _ => return Ok(None),
    };
    if activation
        .as_ref()
        .is_some_and(|activation| activation.status == "suspended")
    {
        return Ok(None);
    }
    let readiness = close_ready_in(conn, project, work_unit_id, activation.as_ref())?;
    if readiness.result != "pass" {
        return Ok(None);
    }
    let Some(activation_id) = activation.map(|activation| activation.activation_id) else {
        return Ok(None);
    };
    let canonical_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let source_snapshots = conn
        .prepare(
            r#"
            select snapshot.id,repository.path,snapshot.head_sha,snapshot.is_clean
            from repository_snapshots snapshot
            join repositories repository on repository.id=snapshot.repository_id
            where repository.project_id=?1
              and snapshot.work_unit_activation_id=?2
              and snapshot.id=(
                select max(current.id) from repository_snapshots current
                where current.repository_id=snapshot.repository_id
                  and current.work_unit_activation_id=?2
              )
            order by snapshot.id
            "#,
        )?
        .query_map(params![project, activation_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, i64>(3)? == 1,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|(_, path, _, _)| {
            let path = Path::new(path);
            let joined = if path.is_absolute() {
                path.to_path_buf()
            } else {
                root.join(path)
            };
            std::fs::canonicalize(&joined).unwrap_or(joined) == canonical_root
        })
        .collect::<Vec<_>>();
    let [(repository_snapshot_id, _, Some(head), true)] = source_snapshots.as_slice() else {
        return Ok(None);
    };
    if head != reviewed_commit {
        return Ok(None);
    }

    let design_versions = design_versions_for_work(conn, work_unit_id)?;
    if design_versions.len() > 1 {
        bail!(
            "work unit {work_unit_id} has multiple current design boundaries; next: agent-workbench status --work {work_unit_id}"
        );
    }
    let design_version_id = design_versions.first().copied();
    let exact_rows = release_boundary_rows(conn, project, work_unit_id, activation_id)?;
    let readiness_rows = readiness
        .items
        .into_iter()
        .map(|item| {
            CanonicalValue::object([
                ("name", CanonicalValue::string(item.name)),
                ("result", CanonicalValue::string(item.result)),
            ])
        })
        .collect::<Vec<_>>();
    let identity = domain_digest(
        b"agent-workbench/release-work-boundary/v1\0",
        &CanonicalValue::object([
            ("work", CanonicalValue::Integer(work_unit_id)),
            ("activation", CanonicalValue::Integer(activation_id)),
            (
                "design",
                design_version_id
                    .map(CanonicalValue::Integer)
                    .unwrap_or(CanonicalValue::Null),
            ),
            (
                "repository_snapshot",
                CanonicalValue::Integer(*repository_snapshot_id),
            ),
            ("reviewed_commit", CanonicalValue::string(reviewed_commit)),
            ("readiness", CanonicalValue::Array(readiness_rows)),
            (
                "exact_rows",
                CanonicalValue::Array(exact_rows.into_iter().map(CanonicalValue::String).collect()),
            ),
        ]),
    );
    Ok(Some(ReleaseWorkBoundary {
        work_unit_id,
        activation_id: Some(activation_id),
        design_version_id,
        repository_snapshot_id: *repository_snapshot_id,
        identity,
    }))
}

fn release_boundary_rows(
    conn: &Connection,
    project: i64,
    work_unit_id: i64,
    activation_id: i64,
) -> Result<Vec<String>> {
    let mut statement = conn.prepare(
        r#"
        select value from (
          select 'work:'||id||':'||status||':'||coalesce(responsibility,'') value
          from work_units where id=?2 and project_id=?1
          union all
          select 'activation:'||id||':'||status from work_unit_activations
          where id=?3 and work_unit_id=?2 and project_id=?1
          union all
          select 'task:'||id||':'||status from current_tasks where work_unit_id=?2
          union all
          select 'checklist:'||id||':'||status from checklists where work_unit_id=?2
          union all
          select 'checklist-item:'||item.id||':'||item.status from checklist_items item
          join checklists checklist on checklist.id=item.checklist_id where checklist.work_unit_id=?2
          union all
          select 'gate:'||id||':'||status from validation_gates where work_unit_id=?2
          union all
          select 'validation-run:'||run.id||':'||run.result||':'||coalesce(run.classification,'')
          from validation_runs run where run.work_unit_id=?2
          union all
          select 'review-plan:'||id||':'||status||':'||required from review_plans where work_unit_id=?2
          union all
          select 'review-run:'||run.id||':'||run.status||':'||run.clean_run
          from review_runs run join review_plans plan on plan.id=run.review_plan_id
          where plan.work_unit_id=?2
          union all
          select 'finding:'||finding.id||':'||finding.classification||':'||finding.status||':'||finding.lifecycle_state
          from findings finding join review_runs run on run.id=finding.review_run_id
          join review_plans plan on plan.id=run.review_plan_id where plan.work_unit_id=?2
          union all
          select 'record:'||id from work_records where project_id=?1 and work_unit_id=?2
          union all
          select 'record-command:'||link.id from work_record_commands link
          join work_records record on record.id=link.work_record_id where record.work_unit_id=?2
          union all
          select 'record-commit:'||link.id||':'||coalesce(link.commit_sha,'') from work_record_commits link
          join work_records record on record.id=link.work_record_id where record.work_unit_id=?2
          union all
          select 'record-file:'||link.id from work_record_files link
          join work_records record on record.id=link.work_record_id where record.work_unit_id=?2
          union all
          select 'repository-snapshot:'||snapshot.id||':'||coalesce(snapshot.head_sha,'')||':'||snapshot.is_clean
          from repository_snapshots snapshot where snapshot.work_unit_activation_id=?3
        ) order by value
        "#,
    )?;
    let rows = statement.query_map(params![project, work_unit_id, activation_id], |row| {
        row.get::<_, String>(0)
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn blocked_close_ready(
    work_unit_id: i64,
    activation_id: Option<i64>,
    item_name: &str,
    action: String,
    details: impl Into<String>,
) -> CloseReadyOutcome {
    CloseReadyOutcome {
        work_unit_id: Some(work_unit_id),
        activation_id,
        result: "blocked".to_string(),
        blocking_reason: Some("close-ready checks failed".to_string()),
        items: vec![CloseReadyItem::fail(item_name, &action, details)],
    }
}

fn close_ready_in(
    conn: &Connection,
    project: i64,
    work_unit_id: i64,
    activation: Option<&crate::db::StoredActivation>,
) -> Result<CloseReadyOutcome> {
    let open_tasks = conn.query_row(
        "select count(*) from current_tasks where work_unit_id = ?1 and status in ('open', 'blocked')",
        params![work_unit_id],
        |row| row.get::<_, i64>(0),
    )?;
    let validation = validation_close_state(conn, work_unit_id)?;
    let repository = match activation {
        Some(activation) => repository_close_state(conn, activation)?,
        None => RepositoryCloseState {
            repository_count: 0,
            missing_snapshot_count: 0,
            unclassified_dirty_state_count: 0,
            missing_comparison_count: 0,
            unclassified_comparison_count: 0,
        },
    };
    let (review, review_plan_blocker_details) =
        review_plan_stage_projection(conn, work_unit_id, "close-ready")?;
    let trace = close_trace_state(conn, work_unit_id)?;
    let process = close_process_state(conn, project, work_unit_id)?;
    let missing_close_review_types = missing_required_close_review_types(conn, work_unit_id)?;
    let missing_selected_gate_details = missing_selected_gate_details_for_work(conn, work_unit_id)?;
    let validation_gate_blocker_details =
        validation_gate_blocker_details_for_work(conn, work_unit_id)?;
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
    items.push(if process.unsettled_kpt_item_count > 0 {
        CloseReadyItem::fail(
            "corrections_kpt_checked",
            "convert or dismiss every open or accepted KPT item before closing work",
            format!(
                "{} active corrections, {} open KPT reviews, {} unsettled KPT items",
                process.repeated_correction_count,
                process.open_kpt_review_count,
                process.unsettled_kpt_item_count
            ),
        )
        } else if process.repeated_correction_count < 2
            || process.unsettled_repeated_correction_count == 0
        {
        CloseReadyItem::pass(
            "corrections_kpt_checked",
            format!(
                    "{} active corrections, {} unsettled correction sources, {} open KPT reviews, {} unsettled KPT items",
                    process.repeated_correction_count,
                    process.unsettled_repeated_correction_count,
                    process.open_kpt_review_count,
                    process.unsettled_kpt_item_count
            ),
        )
    } else {
        CloseReadyItem::fail(
            "corrections_kpt_checked",
                "import and settle every repeated active correction through a KPT review",
                format!(
                    "{} active corrections, {} unsettled correction sources, {} open KPT reviews, {} unsettled KPT items",
                    process.repeated_correction_count,
                    process.unsettled_repeated_correction_count,
                    process.open_kpt_review_count,
                process.unsettled_kpt_item_count
            ),
        )
    });
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
        if activation.is_none()
            || (repository.repository_count > 0
            && repository.missing_snapshot_count == 0
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
        work_unit_id: Some(work_unit_id),
        activation_id: activation.map(|item| item.activation_id),
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
