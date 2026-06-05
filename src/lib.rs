use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

pub const LEDGER_DIR: &str = ".agent-workbench";
pub const LEDGER_FILE: &str = "ledger.sqlite";
const SCHEMA_VERSION: i64 = 1;

pub fn default_ledger_path(root: &Path) -> PathBuf {
    root.join(LEDGER_DIR).join(LEDGER_FILE)
}

pub fn init_project(root: &Path) -> Result<InitOutcome> {
    let ledger_path = default_ledger_path(root);
    if let Some(parent) = ledger_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create ledger directory {}", parent.display()))?;
    }

    let conn = open_ledger(&ledger_path)?;
    migrate(&conn)?;
    ensure_project(&conn, root)?;

    Ok(InitOutcome { ledger_path })
}

pub fn project_status(root: &Path) -> Result<ProjectStatus> {
    let ledger_path = default_ledger_path(root);
    if !ledger_path.exists() {
        return Ok(ProjectStatus {
            initialized: false,
            ledger_path,
            project_name: None,
            open_work_units: 0,
            active_activations: 0,
            schema_version: None,
        });
    }

    let conn = open_ledger(&ledger_path)?;
    let project_name = conn
        .query_row("select name from projects order by id limit 1", [], |row| {
            row.get::<_, String>(0)
        })
        .optional()?;
    let open_work_units = count_rows(&conn, "work_units", "status in ('open', 'blocked')")?;
    let active_activations = count_rows(&conn, "work_unit_activations", "status = 'active'")?;
    let schema_version = conn
        .query_row(
            "select version from schema_migrations order by version desc limit 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;

    Ok(ProjectStatus {
        initialized: true,
        ledger_path,
        project_name,
        open_work_units,
        active_activations,
        schema_version,
    })
}

pub fn next_action(root: &Path) -> Result<NextAction> {
    let ledger_path = default_ledger_path(root);
    if !ledger_path.exists() {
        return Ok(NextAction::NotInitialized { ledger_path });
    }

    let conn = open_ledger(&ledger_path)?;
    let active = conn
        .query_row(
            r#"
            select w.id, w.title
            from work_unit_activations a
            join work_units w on w.id = a.work_unit_id
            where a.status = 'active'
            order by a.id desc
            limit 1
            "#,
            [],
            |row| {
                Ok(ActiveWorkUnit {
                    id: row.get(0)?,
                    title: row.get(1)?,
                })
            },
        )
        .optional()?;

    Ok(match active {
        Some(work_unit) => NextAction::ContinueActive { work_unit },
        None => NextAction::NoActiveWorkUnit,
    })
}

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
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let target = suspended_activation(&tx)?.context("no suspended activation to resume")?;
    let snapshot = suspend_snapshot(&tx, target.activation_id)?;
    let stack_revision = max_id(&tx, "work_unit_events")?;
    let authority_high_watermark = max_id(&tx, "authority_events")?;

    let deeper_open = tx.query_row(
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
    let blocking_dependencies = tx.query_row(
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
    let allowed = checks.iter().all(|(_, pass, _)| *pass);
    let blocking_reason = checks
        .iter()
        .find_map(|(_, pass, message)| (!pass).then_some(*message));

    tx.execute(
        r#"
        insert into resume_checks(
            work_unit_id, work_unit_activation_id, suspend_snapshot_id, maturity,
            status, result, authority_event_high_watermark, activation_stack_revision,
            allowed_next_action, blocking_reason, created_at
        )
        values (?1, ?2, ?3, 'basic', 'pending', ?4, ?5, ?6, ?7, ?8, current_timestamp)
        "#,
        params![
            target.work_unit_id,
            target.activation_id,
            snapshot.id,
            if allowed { "allowed" } else { "blocked" },
            authority_high_watermark,
            stack_revision,
            if allowed {
                Some(snapshot.next_action.as_str())
            } else {
                None
            },
            blocking_reason,
        ],
    )?;
    let resume_check_id = tx.last_insert_rowid();

    for (name, pass, message) in checks {
        tx.execute(
            r#"
            insert into resume_check_items(
                resume_check_id, check_name, result, blocking_action, details
            )
            values (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                resume_check_id,
                name,
                if pass { "pass" } else { "fail" },
                if pass { None } else { Some(message) },
                message,
            ],
        )?;
    }

    tx.commit()?;

    Ok(ResumeCheckOutcome {
        resume_check_id,
        result: if allowed {
            "allowed".to_string()
        } else {
            "blocked".to_string()
        },
        blocking_reason: blocking_reason.map(str::to_string),
    })
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

pub fn add_user_correction(
    root: &Path,
    input: NewUserCorrection<'_>,
) -> Result<UserCorrectionOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;

    tx.execute(
        r#"
        insert into user_corrections(
            project_id, scope, correction_type, mistake_pattern, correction,
            applies_to, severity, status, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'active', current_timestamp)
        "#,
        params![
            project_id,
            input.scope,
            input.correction_type,
            input.mistake_pattern,
            input.correction,
            input.applies_to,
            input.severity,
        ],
    )?;
    let user_correction_id = tx.last_insert_rowid();
    insert_rule_binding(
        &tx,
        RuleBindingInput {
            project_id,
            rule_source_type: "user_correction",
            user_correction_id: Some(user_correction_id),
            command_profile_id: None,
            work_unit_id: None,
            scope_type: scope_type_for(input.scope),
            scope_key: Some(input.scope),
            precedence: 80,
        },
    )?;
    tx.commit()?;

    Ok(UserCorrectionOutcome { user_correction_id })
}

pub fn list_user_corrections(
    root: &Path,
    scope: Option<&str>,
) -> Result<Vec<UserCorrectionRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut records = Vec::new();

    match scope {
        Some(scope) => {
            let mut stmt = conn.prepare(
                r#"
                select id, scope, correction_type, mistake_pattern, correction, severity
                from user_corrections
                where project_id = ?1 and status = 'active' and scope = ?2
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![project_id, scope], user_correction_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        None => {
            let mut stmt = conn.prepare(
                r#"
                select id, scope, correction_type, mistake_pattern, correction, severity
                from user_corrections
                where project_id = ?1 and status = 'active'
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![project_id], user_correction_record)?;
            for row in rows {
                records.push(row?);
            }
        }
    }

    Ok(records)
}

pub fn add_fixed_command(root: &Path, input: NewCommandProfile<'_>) -> Result<CommandOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;

    tx.execute(
        r#"
        insert into command_profiles(
            project_id, name, command, command_type, scope, status, stability,
            timeout, expected_result, source, created_at, updated_at
        )
        values (?1, ?2, ?3, ?4, ?5, 'fixed', 'stable', ?6, ?7, 'user',
                current_timestamp, current_timestamp)
        "#,
        params![
            project_id,
            input.name,
            input.command,
            input.command_type,
            input.scope,
            input.timeout,
            input.expected_result,
        ],
    )?;
    let command_profile_id = tx.last_insert_rowid();
    insert_rule_binding(
        &tx,
        RuleBindingInput {
            project_id,
            rule_source_type: "command_profile",
            user_correction_id: None,
            command_profile_id: Some(command_profile_id),
            work_unit_id: None,
            scope_type: "command",
            scope_key: Some(input.scope),
            precedence: 70,
        },
    )?;
    tx.commit()?;

    Ok(CommandOutcome { command_profile_id })
}

pub fn list_command_profiles(
    root: &Path,
    command_type: Option<&str>,
) -> Result<Vec<CommandProfileRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut records = Vec::new();

    match command_type {
        Some(command_type) => {
            let mut stmt = conn.prepare(
                r#"
                select id, name, command_type, scope, status, command
                from command_profiles
                where project_id = ?1 and command_type = ?2
                order by name
                "#,
            )?;
            let rows = stmt.query_map(params![project_id, command_type], command_profile_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        None => {
            let mut stmt = conn.prepare(
                r#"
                select id, name, command_type, scope, status, command
                from command_profiles
                where project_id = ?1
                order by name
                "#,
            )?;
            let rows = stmt.query_map(params![project_id], command_profile_record)?;
            for row in rows {
                records.push(row?);
            }
        }
    }

    Ok(records)
}

pub fn applicable_rules(root: &Path, input: RuleQuery<'_>) -> Result<Vec<RuleRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut records = Vec::new();
    let scope_key = input.scope_key.unwrap_or("project");

    let mut stmt = conn.prepare(
        r#"
        select rb.id, rb.rule_source_type, rb.scope_type, rb.scope_key, rb.precedence,
               rb.user_correction_id, rb.command_profile_id, rb.work_unit_id
        from rule_bindings rb
        where rb.project_id = ?1
          and rb.status = 'active'
          and (
            rb.scope_type = 'project'
            or rb.scope_key = ?2
            or (?3 is not null and rb.work_unit_id = ?3)
          )
        order by rb.precedence desc, rb.id asc
        "#,
    )?;
    let rows = stmt.query_map(params![project_id, scope_key, input.work_unit_id], |row| {
        Ok(RuleRecord {
            id: row.get(0)?,
            rule_source_type: row.get(1)?,
            scope_type: row.get(2)?,
            scope_key: row.get(3)?,
            precedence: row.get(4)?,
            user_correction_id: row.get(5)?,
            command_profile_id: row.get(6)?,
            work_unit_id: row.get(7)?,
        })
    })?;
    for row in rows {
        records.push(row?);
    }

    Ok(records)
}

fn open_ledger(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)
        .with_context(|| format!("failed to open ledger {}", path.display()))?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(conn)
}

fn open_existing_project(root: &Path) -> Result<Connection> {
    let ledger_path = default_ledger_path(root);
    if !ledger_path.exists() {
        bail!("project is not initialized; run agent-workbench init");
    }
    open_ledger(&ledger_path)
}

fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)?;

    let current_version = conn
        .query_row(
            "select version from schema_migrations order by version desc limit 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(0);

    if current_version < SCHEMA_VERSION {
        conn.execute(
            "insert into schema_migrations(version, applied_at) values (?1, current_timestamp)",
            params![SCHEMA_VERSION],
        )?;
    }

    Ok(())
}

fn ensure_project(conn: &Connection, root: &Path) -> Result<()> {
    let root_path = root
        .canonicalize()
        .unwrap_or_else(|_| root.to_path_buf())
        .display()
        .to_string();
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project");

    conn.execute(
        r#"
        insert into projects(name, root_path, created_at, updated_at)
        select ?1, ?2, current_timestamp, current_timestamp
        where not exists (select 1 from projects where root_path = ?2)
        "#,
        params![name, root_path],
    )?;

    Ok(())
}

fn count_rows(conn: &Connection, table: &str, predicate: &str) -> Result<i64> {
    let sql = format!("select count(*) from {table} where {predicate}");
    let count = conn.query_row(&sql, [], |row| row.get(0))?;
    Ok(count)
}

fn project_id(conn: &Connection) -> Result<i64> {
    conn.query_row("select id from projects order by id limit 1", [], |row| {
        row.get(0)
    })
    .context("project row not found; run agent-workbench init")
}

fn max_id(conn: &Connection, table: &str) -> Result<i64> {
    let sql = format!("select coalesce(max(id), 0) from {table}");
    let id = conn.query_row(&sql, [], |row| row.get(0))?;
    Ok(id)
}

fn active_activation(conn: &Connection) -> Result<Option<StoredActivation>> {
    conn.query_row(
        r#"
        select a.id, a.project_id, a.work_unit_id, a.stack_depth, a.status
        from work_unit_activations a
        where a.status = 'active'
        order by a.id desc
        limit 1
        "#,
        [],
        stored_activation,
    )
    .optional()
    .map_err(Into::into)
}

fn suspended_activation(conn: &Connection) -> Result<Option<StoredActivation>> {
    conn.query_row(
        r#"
        select a.id, a.project_id, a.work_unit_id, a.stack_depth, a.status
        from work_unit_activations a
        where a.status = 'suspended'
        order by a.stack_depth desc, a.id desc
        limit 1
        "#,
        [],
        stored_activation,
    )
    .optional()
    .map_err(Into::into)
}

fn stored_activation(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredActivation> {
    Ok(StoredActivation {
        activation_id: row.get(0)?,
        project_id: row.get(1)?,
        work_unit_id: row.get(2)?,
        stack_depth: row.get(3)?,
        status: row.get(4)?,
    })
}

fn suspend_snapshot(conn: &Connection, activation_id: i64) -> Result<StoredSuspendSnapshot> {
    conn.query_row(
        r#"
        select id, reason, next_action
        from suspend_snapshots
        where work_unit_activation_id = ?1
        order by id desc
        limit 1
        "#,
        params![activation_id],
        |row| {
            Ok(StoredSuspendSnapshot {
                id: row.get(0)?,
                reason: row.get(1)?,
                next_action: row.get(2)?,
            })
        },
    )
    .optional()?
    .context("suspend snapshot not found")
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

fn insert_event(conn: &Connection, event: NewEvent<'_>) -> Result<i64> {
    conn.execute(
        r#"
        insert into work_unit_events(
            work_unit_id, work_unit_activation_id, related_activation_id,
            event_type, reason, status_domain, previous_status, next_status, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, current_timestamp)
        "#,
        params![
            event.work_unit_id,
            event.activation_id,
            event.related_activation_id,
            event.event_type,
            event.reason,
            event.status_domain,
            event.previous_status,
            event.next_status,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn insert_rule_binding(conn: &Connection, input: RuleBindingInput<'_>) -> Result<i64> {
    conn.execute(
        r#"
        insert into rule_bindings(
            project_id, rule_source_type, user_correction_id, command_profile_id,
            work_unit_id, scope_type, scope_key, precedence, status, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'active', current_timestamp)
        "#,
        params![
            input.project_id,
            input.rule_source_type,
            input.user_correction_id,
            input.command_profile_id,
            input.work_unit_id,
            input.scope_type,
            input.scope_key,
            input.precedence,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn scope_type_for(scope: &str) -> &'static str {
    match scope {
        "project" => "project",
        "repository" => "repository",
        "review" => "review",
        "command" => "command",
        "agent_role" => "agent_role",
        "design_package" => "design_package",
        _ => "work_unit",
    }
}

fn user_correction_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<UserCorrectionRecord> {
    Ok(UserCorrectionRecord {
        id: row.get(0)?,
        scope: row.get(1)?,
        correction_type: row.get(2)?,
        mistake_pattern: row.get(3)?,
        correction: row.get(4)?,
        severity: row.get(5)?,
    })
}

fn command_profile_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<CommandProfileRecord> {
    Ok(CommandProfileRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        command_type: row.get(2)?,
        scope: row.get(3)?,
        status: row.get(4)?,
        command: row.get(5)?,
    })
}

struct NewEvent<'a> {
    work_unit_id: i64,
    activation_id: Option<i64>,
    related_activation_id: Option<i64>,
    event_type: &'a str,
    reason: Option<&'a str>,
    status_domain: &'a str,
    previous_status: Option<&'a str>,
    next_status: Option<&'a str>,
}

#[derive(Debug)]
struct StoredActivation {
    activation_id: i64,
    project_id: i64,
    work_unit_id: i64,
    stack_depth: i64,
    status: String,
}

#[derive(Debug)]
struct StoredSuspendSnapshot {
    id: i64,
    reason: String,
    next_action: String,
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

struct RuleBindingInput<'a> {
    project_id: i64,
    rule_source_type: &'a str,
    user_correction_id: Option<i64>,
    command_profile_id: Option<i64>,
    work_unit_id: Option<i64>,
    scope_type: &'a str,
    scope_key: Option<&'a str>,
    precedence: i64,
}

#[derive(Debug)]
pub struct InitOutcome {
    pub ledger_path: PathBuf,
}

#[derive(Debug)]
pub struct ProjectStatus {
    pub initialized: bool,
    pub ledger_path: PathBuf,
    pub project_name: Option<String>,
    pub open_work_units: i64,
    pub active_activations: i64,
    pub schema_version: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum NextAction {
    NotInitialized { ledger_path: PathBuf },
    NoActiveWorkUnit,
    ContinueActive { work_unit: ActiveWorkUnit },
}

#[derive(Debug, PartialEq, Eq)]
pub struct ActiveWorkUnit {
    pub id: i64,
    pub title: String,
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
pub struct ResumeCheckOutcome {
    pub resume_check_id: i64,
    pub result: String,
    pub blocking_reason: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ResumeOutcome {
    pub work_unit_id: i64,
    pub activation_id: i64,
}

pub struct NewUserCorrection<'a> {
    pub scope: &'a str,
    pub correction_type: &'a str,
    pub mistake_pattern: &'a str,
    pub correction: &'a str,
    pub applies_to: &'a str,
    pub severity: &'a str,
}

#[derive(Debug, PartialEq, Eq)]
pub struct UserCorrectionOutcome {
    pub user_correction_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct UserCorrectionRecord {
    pub id: i64,
    pub scope: String,
    pub correction_type: String,
    pub mistake_pattern: String,
    pub correction: String,
    pub severity: String,
}

pub struct NewCommandProfile<'a> {
    pub name: &'a str,
    pub command_type: &'a str,
    pub scope: &'a str,
    pub command: &'a str,
    pub timeout: Option<&'a str>,
    pub expected_result: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CommandOutcome {
    pub command_profile_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CommandProfileRecord {
    pub id: i64,
    pub name: String,
    pub command_type: String,
    pub scope: Option<String>,
    pub status: String,
    pub command: String,
}

pub struct RuleQuery<'a> {
    pub scope_key: Option<&'a str>,
    pub work_unit_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RuleRecord {
    pub id: i64,
    pub rule_source_type: String,
    pub scope_type: String,
    pub scope_key: Option<String>,
    pub precedence: i64,
    pub user_correction_id: Option<i64>,
    pub command_profile_id: Option<i64>,
    pub work_unit_id: Option<i64>,
}

const SCHEMA: &str = r#"
create table if not exists schema_migrations (
    version integer primary key,
    applied_at text not null
);

create table if not exists projects (
    id integer primary key,
    name text not null,
    root_path text not null unique,
    created_at text not null,
    updated_at text not null
);

create table if not exists authority_events (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    event_type text not null check (event_type in ('user_instruction', 'design_doc', 'agents', 'policy', 'review_result', 'validation_result')),
    source text,
    text_or_summary text not null,
    scope text,
    precedence integer not null default 0,
    supersedes_event_id integer references authority_events(id),
    status text not null default 'active' check (status in ('active', 'inactive', 'superseded')),
    created_at text not null
);

create table if not exists work_units (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    parent_work_unit_id integer references work_units(id),
    title text not null,
    status text not null default 'open' check (status in ('open', 'blocked', 'closed', 'abandoned')),
    responsibility text,
    in_scope text,
    out_of_scope text,
    interrupt_reason text,
    selected_gate_id integer,
    review_plan_status text,
    started_at text not null,
    closed_at text,
    close_summary text
);

create table if not exists work_unit_activations (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    work_unit_id integer not null references work_units(id) on delete cascade,
    parent_activation_id integer references work_unit_activations(id),
    suspended_by_activation_id integer references work_unit_activations(id),
    stack_depth integer not null default 0,
    status text not null check (status in ('active', 'suspended', 'completed', 'abandoned')),
    activation_reason text not null check (activation_reason in ('start', 'interrupt', 'resume', 'reopen', 'follow_up')),
    suspend_snapshot_id integer,
    opened_at text not null,
    suspended_at text,
    completed_at text
);

create unique index if not exists ux_one_active_activation
on work_unit_activations(project_id)
where status = 'active';

create trigger if not exists trg_activation_project_matches_work_unit_insert
before insert on work_unit_activations
for each row
when new.project_id != (select project_id from work_units where id = new.work_unit_id)
begin
    select raise(abort, 'activation project_id must match work unit project_id');
end;

create trigger if not exists trg_activation_project_matches_work_unit_update
before update of project_id, work_unit_id on work_unit_activations
for each row
when new.project_id != (select project_id from work_units where id = new.work_unit_id)
begin
    select raise(abort, 'activation project_id must match work unit project_id');
end;

create table if not exists work_unit_events (
    id integer primary key,
    work_unit_id integer not null references work_units(id) on delete cascade,
    work_unit_activation_id integer references work_unit_activations(id),
    related_activation_id integer references work_unit_activations(id),
    event_type text not null check (event_type in ('opened', 'suspended', 'resumed', 'closed', 'abandoned', 'blocked', 'unblocked', 'reopened', 'invalidated', 'follow_up_created')),
    reason text,
    caused_by_work_unit_id integer references work_units(id),
    authority_event_id integer references authority_events(id),
    status_domain text not null check (status_domain in ('work_unit', 'activation')),
    previous_status text,
    next_status text,
    created_at text not null
);

create table if not exists suspend_snapshots (
    id integer primary key,
    work_unit_activation_id integer not null references work_unit_activations(id) on delete cascade,
    work_unit_id integer not null references work_units(id) on delete cascade,
    reason text not null,
    active_task_ids text,
    next_action text not null,
    selected_gate_id integer,
    authority_refs text,
    review_scope_refs text,
    repository_heads text,
    repository_snapshot_ids text,
    repository_status text,
    dirty_state_summary text,
    open_findings text,
    assumptions text,
    created_at text not null
);

create table if not exists resume_checks (
    id integer primary key,
    work_unit_id integer not null references work_units(id) on delete cascade,
    work_unit_activation_id integer not null references work_unit_activations(id) on delete cascade,
    suspend_snapshot_id integer not null references suspend_snapshots(id) on delete cascade,
    maturity text not null check (maturity in ('basic', 'trace-aware', 'repo-aware')),
    status text not null default 'pending' check (status in ('pending', 'consumed', 'stale')),
    result text not null check (result in ('allowed', 'blocked', 'needs_design_review', 'needs_gate_reselection', 'needs_user_decision')),
    authority_event_high_watermark integer,
    activation_stack_revision integer,
    repository_snapshot_id integer,
    allowed_next_action text,
    blocking_reason text,
    consumed_at text,
    consumed_by_work_unit_event_id integer references work_unit_events(id),
    created_at text not null
);

create table if not exists resume_check_items (
    id integer primary key,
    resume_check_id integer not null references resume_checks(id) on delete cascade,
    check_name text not null check (check_name in ('resume_target_suspended', 'snapshot_exists', 'suspend_reason_exists', 'next_action_exists', 'deeper_frames_closed', 'blocking_dependencies_clear', 'design_version_current', 'task_derivation_current', 'checklist_current', 'selected_gate_current', 'review_plan_current', 'repository_state_current', 'assumptions_current')),
    result text not null check (result in ('pass', 'fail', 'not_checked', 'needs_evidence')),
    evidence_ref text,
    blocking_action text,
    details text
);

create table if not exists work_unit_dependencies (
    id integer primary key,
    work_unit_id integer not null references work_units(id) on delete cascade,
    depends_on_work_unit_id integer not null references work_units(id) on delete cascade,
    dependency_type text not null check (dependency_type in ('blocks', 'discovered_by', 'supersedes', 'invalidates_assumption', 'regression_of', 'invalidates_closure', 'follow_up_of')),
    reason text,
    status text not null default 'open' check (status in ('open', 'resolved')),
    created_at text not null,
    resolved_at text,
    resolved_by_work_unit_event_id integer references work_unit_events(id)
);

create table if not exists tasks (
    id integer primary key,
    work_unit_id integer references work_units(id) on delete cascade,
    title text not null,
    priority text not null default 'medium' check (priority in ('critical', 'high', 'medium', 'low')),
    status text not null default 'open' check (status in ('open', 'blocked', 'closed', 'accepted_out_of_scope')),
    source text not null default 'user' check (source in ('user', 'plan', 'review', 'coverage', 'design', 'handoff')),
    parent_task_id integer references tasks(id),
    details text,
    completion_condition text,
    closed_by_commit text
);

create table if not exists decisions (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decision_key text,
    topic text not null,
    decision text not null,
    rationale text,
    compatibility_impact text,
    status text not null default 'accepted' check (status in ('accepted', 'rejected', 'superseded')),
    authority_refs text,
    created_at text not null
);

create table if not exists rule_bindings (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    rule_source_type text not null check (rule_source_type in ('authority_event', 'user_correction', 'command_profile', 'review_policy', 'work_unit', 'validation_gate', 'acceptance_record', 'skill_default')),
    authority_event_id integer references authority_events(id),
    user_correction_id integer,
    command_profile_id integer,
    review_policy_id integer,
    work_unit_id integer references work_units(id),
    validation_gate_id integer,
    acceptance_record_id integer,
    scope_type text not null check (scope_type in ('project', 'repository', 'design_package', 'work_unit', 'agent_role', 'command', 'review')),
    scope_key text,
    precedence integer not null default 0,
    status text not null default 'active' check (status in ('active', 'shadowed', 'inactive', 'superseded')),
    created_at text not null
);

create table if not exists user_corrections (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    authority_event_id integer references authority_events(id),
    scope text,
    correction_type text not null check (correction_type in ('process', 'design', 'implementation', 'review', 'command', 'communication', 'other')),
    mistake_pattern text not null,
    correction text not null,
    applies_to text not null check (applies_to in ('current_work_unit', 'project', 'repository', 'design_package', 'command_profile', 'agent_role')),
    severity text not null default 'medium' check (severity in ('critical', 'high', 'medium', 'low')),
    status text not null default 'active' check (status in ('active', 'superseded', 'retired')),
    supersedes_user_correction_id integer references user_corrections(id),
    created_at text not null
);

create table if not exists command_profiles (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    repository_id integer,
    name text not null,
    command text not null,
    command_type text not null check (command_type in ('validation', 'test', 'lint', 'format', 'build', 'dev_server', 'review_context', 'export', 'other')),
    scope text,
    status text not null default 'candidate' check (status in ('candidate', 'preferred', 'fixed', 'deprecated', 'blocked')),
    stability text not null default 'context_dependent' check (stability in ('stable', 'context_dependent', 'experimental')),
    working_directory text,
    environment text,
    timeout text,
    expected_result text,
    replaces_command_profile_id integer references command_profiles(id),
    source text not null default 'agent_observed' check (source in ('user', 'agent_observed', 'design', 'validation_gate')),
    created_at text not null,
    updated_at text not null,
    unique(project_id, name)
);

create table if not exists command_usages (
    id integer primary key,
    command_profile_id integer references command_profiles(id),
    work_unit_id integer references work_units(id),
    work_unit_activation_id integer references work_unit_activations(id),
    command text not null,
    result text not null check (result in ('pass', 'fail', 'timeout', 'cancelled', 'unknown')),
    log_path text,
    repository_snapshot_id integer,
    created_at text not null
);

create table if not exists command_deviations (
    id integer primary key,
    command_profile_id integer not null references command_profiles(id) on delete cascade,
    command_usage_id integer references command_usages(id),
    work_unit_id integer references work_units(id),
    reason text not null,
    status text not null default 'proposed' check (status in ('proposed', 'approved', 'rejected')),
    acceptance_record_id integer,
    created_at text not null
);

create table if not exists handoffs (
    id integer primary key,
    work_unit_id integer references work_units(id) on delete cascade,
    topic text not null,
    work_performed text,
    next_actions text,
    notable_operations text,
    export_path text,
    created_at text not null
);

create table if not exists handoff_commands (
    id integer primary key,
    handoff_id integer not null references handoffs(id) on delete cascade,
    command_usage_id integer references command_usages(id),
    command_profile_id integer references command_profiles(id),
    command text,
    result text,
    log_path text,
    note text
);

create table if not exists handoff_commits (
    id integer primary key,
    handoff_id integer not null references handoffs(id) on delete cascade,
    git_commit_id integer,
    commit_sha text,
    role text not null default 'referenced' check (role in ('created', 'referenced', 'validation_base', 'rollback_point')),
    note text
);

create table if not exists handoff_files (
    id integer primary key,
    handoff_id integer not null references handoffs(id) on delete cascade,
    git_file_change_id integer,
    repository_id integer,
    path text not null,
    role text not null default 'changed' check (role in ('changed', 'reviewed', 'generated', 'evidence', 'ignored')),
    note text
);

create table if not exists work_record_forks (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    source_work_unit_id integer references work_units(id),
    source_work_unit_activation_id integer references work_unit_activations(id),
    source_handoff_id integer references handoffs(id),
    source_repository_snapshot_id integer,
    source_git_commit_id integer,
    forked_work_unit_id integer references work_units(id),
    fork_reason text not null check (fork_reason in ('design_changed', 'agent_drift', 'invalid_assumption', 'failed_validation', 'user_requested_redo', 'other')),
    discard_policy text not null default 'keep_history' check (discard_policy in ('keep_history', 'supersede_source', 'mark_abandoned')),
    status text not null default 'open' check (status in ('open', 'closed', 'abandoned')),
    created_by_authority_event_id integer references authority_events(id),
    created_at text not null,
    closed_at text
);

create table if not exists kpt_reviews (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    scope text,
    period_start text,
    period_end text,
    trigger text not null default 'manual' check (trigger in ('manual', 'work_unit_close', 'review_close', 'scheduled')),
    summary text,
    status text not null default 'open' check (status in ('open', 'closed')),
    created_at text not null,
    closed_at text
);

create table if not exists kpt_items (
    id integer primary key,
    kpt_review_id integer not null references kpt_reviews(id) on delete cascade,
    item_type text not null check (item_type in ('keep', 'problem', 'try')),
    title text not null,
    details text,
    severity text not null default 'medium' check (severity in ('critical', 'high', 'medium', 'low')),
    linked_user_correction_id integer references user_corrections(id),
    linked_command_profile_id integer references command_profiles(id),
    linked_review_finding_id integer,
    linked_task_id integer references tasks(id),
    proposed_action text,
    status text not null default 'open' check (status in ('open', 'accepted', 'converted_to_task', 'dismissed')),
    created_at text not null
);

create table if not exists kpt_item_conversions (
    id integer primary key,
    kpt_item_id integer not null references kpt_items(id) on delete cascade,
    target_type text not null check (target_type in ('task', 'command_profile', 'review_policy', 'design_version', 'decision', 'user_correction')),
    task_id integer references tasks(id),
    command_profile_id integer references command_profiles(id),
    review_policy_id integer,
    design_version_id integer,
    decision_id integer references decisions(id),
    user_correction_id integer references user_corrections(id),
    created_at text not null
);
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_creates_ledger_and_project() {
        let temp = tempfile::tempdir().unwrap();

        let outcome = init_project(temp.path()).unwrap();

        assert!(outcome.ledger_path.exists());
        let status = project_status(temp.path()).unwrap();
        assert!(status.initialized);
        assert_eq!(status.schema_version, Some(SCHEMA_VERSION));
        assert_eq!(status.open_work_units, 0);
        assert_eq!(status.active_activations, 0);
    }

    #[test]
    fn status_reports_uninitialized_project() {
        let temp = tempfile::tempdir().unwrap();

        let status = project_status(temp.path()).unwrap();

        assert!(!status.initialized);
        assert!(status.schema_version.is_none());
    }

    #[test]
    fn next_reports_no_active_work_unit_after_init() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();

        let next = next_action(temp.path()).unwrap();

        assert_eq!(next, NextAction::NoActiveWorkUnit);
    }

    #[test]
    fn work_start_creates_active_work_unit() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();

        let started = start_work(temp.path(), "write lifecycle test", Some("test first")).unwrap();
        let next = next_action(temp.path()).unwrap();

        assert_eq!(started.work_unit_id, 1);
        assert_eq!(started.activation_id, 1);
        assert_eq!(
            next,
            NextAction::ContinueActive {
                work_unit: ActiveWorkUnit {
                    id: 1,
                    title: "write lifecycle test".to_string()
                }
            }
        );
    }

    #[test]
    fn work_start_refuses_second_active_activation() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        start_work(temp.path(), "one", None).unwrap();

        let second = start_work(temp.path(), "two", None);

        assert!(second.is_err());
    }

    #[test]
    fn suspend_and_resume_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        let started = start_work(temp.path(), "implement resume", None).unwrap();

        let suspended = suspend_work(
            temp.path(),
            "need to validate assumption",
            "continue implementation",
        )
        .unwrap();
        let check = resume_check_basic(temp.path()).unwrap();
        let resumed = resume_work(temp.path(), check.resume_check_id).unwrap();

        assert_eq!(suspended.work_unit_id, started.work_unit_id);
        assert_eq!(check.result, "allowed");
        assert_eq!(resumed.activation_id, started.activation_id);
        assert!(matches!(
            next_action(temp.path()).unwrap(),
            NextAction::ContinueActive { .. }
        ));
    }

    #[test]
    fn interrupt_blocks_parent_until_child_is_closed() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        let parent = start_work(temp.path(), "parent", None).unwrap();

        let interrupt = interrupt_work(temp.path(), "child", "blocks parent").unwrap();
        let blocked = resume_check_basic(temp.path()).unwrap();
        close_active_work(temp.path(), "child done", None).unwrap();
        let allowed = resume_check_basic(temp.path()).unwrap();
        let resumed = resume_work(temp.path(), allowed.resume_check_id).unwrap();

        assert_eq!(interrupt.parent_work_unit_id, parent.work_unit_id);
        assert_eq!(blocked.result, "blocked");
        assert_eq!(
            blocked.blocking_reason.as_deref(),
            Some("deeper activation frames must be completed or abandoned")
        );
        assert_eq!(allowed.result, "allowed");
        assert_eq!(resumed.activation_id, parent.activation_id);
    }

    #[test]
    fn correction_creates_applicable_rule_binding() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();

        let correction = add_user_correction(
            temp.path(),
            NewUserCorrection {
                scope: "project",
                correction_type: "process",
                mistake_pattern: "write design to README",
                correction: "keep local design under local/",
                applies_to: "project",
                severity: "high",
            },
        )
        .unwrap();

        let corrections = list_user_corrections(temp.path(), Some("project")).unwrap();
        let rules = applicable_rules(
            temp.path(),
            RuleQuery {
                scope_key: Some("project"),
                work_unit_id: None,
            },
        )
        .unwrap();

        assert_eq!(corrections.len(), 1);
        assert_eq!(corrections[0].id, correction.user_correction_id);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].rule_source_type, "user_correction");
        assert_eq!(
            rules[0].user_correction_id,
            Some(correction.user_correction_id)
        );
    }

    #[test]
    fn fixed_command_creates_command_rule_binding() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();

        let command = add_fixed_command(
            temp.path(),
            NewCommandProfile {
                name: "storage-tests",
                command_type: "test",
                scope: "storage",
                command: "cargo test -p storage",
                timeout: Some("120s"),
                expected_result: Some("pass"),
            },
        )
        .unwrap();

        let commands = list_command_profiles(temp.path(), Some("test")).unwrap();
        let rules = applicable_rules(
            temp.path(),
            RuleQuery {
                scope_key: Some("storage"),
                work_unit_id: None,
            },
        )
        .unwrap();

        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].id, command.command_profile_id);
        assert_eq!(commands[0].status, "fixed");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].rule_source_type, "command_profile");
        assert_eq!(
            rules[0].command_profile_id,
            Some(command.command_profile_id)
        );
    }

    #[test]
    fn activation_unique_active_constraint_is_enforced() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        let project_id: i64 = conn
            .query_row("select id from projects limit 1", [], |row| row.get(0))
            .unwrap();
        conn.execute(
            "insert into work_units(project_id, title, status, started_at) values (?1, 'one', 'open', current_timestamp)",
            params![project_id],
        )
        .unwrap();
        conn.execute(
            "insert into work_units(project_id, title, status, started_at) values (?1, 'two', 'open', current_timestamp)",
            params![project_id],
        )
        .unwrap();

        conn.execute(
            "insert into work_unit_activations(project_id, work_unit_id, status, activation_reason, opened_at) values (?1, 1, 'active', 'start', current_timestamp)",
            params![project_id],
        )
        .unwrap();
        let duplicate = conn.execute(
            "insert into work_unit_activations(project_id, work_unit_id, status, activation_reason, opened_at) values (?1, 2, 'active', 'start', current_timestamp)",
            params![project_id],
        );

        assert!(duplicate.is_err());
    }
}
