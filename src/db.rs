use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

pub const LEDGER_DIR: &str = ".agent-workbench";
pub const LEDGER_FILE: &str = "ledger.sqlite";
pub(crate) const SCHEMA_VERSION: i64 = 2;

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

pub(crate) fn open_ledger(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)
        .with_context(|| format!("failed to open ledger {}", path.display()))?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(conn)
}

pub(crate) fn open_existing_project(root: &Path) -> Result<Connection> {
    let ledger_path = default_ledger_path(root);
    if !ledger_path.exists() {
        bail!("project is not initialized; run agent-workbench init");
    }
    open_ledger(&ledger_path)
}

fn migrate(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)?;
    ensure_column(conn, "work_record_forks", "source_git_commit_sha", "text")?;

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

fn ensure_column(conn: &Connection, table: &str, column: &str, definition: &str) -> Result<()> {
    let mut stmt = conn.prepare(&format!("pragma table_info({table})"))?;
    let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for existing in columns {
        if existing? == column {
            return Ok(());
        }
    }

    conn.execute(
        &format!("alter table {table} add column {column} {definition}"),
        [],
    )?;
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

pub(crate) fn project_id(conn: &Connection) -> Result<i64> {
    conn.query_row("select id from projects order by id limit 1", [], |row| {
        row.get(0)
    })
    .context("project row not found; run agent-workbench init")
}

pub(crate) fn max_id(conn: &Connection, table: &str) -> Result<i64> {
    let sql = format!("select coalesce(max(id), 0) from {table}");
    let id = conn.query_row(&sql, [], |row| row.get(0))?;
    Ok(id)
}

pub(crate) fn active_activation(conn: &Connection) -> Result<Option<StoredActivation>> {
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

pub(crate) fn suspended_activation(conn: &Connection) -> Result<Option<StoredActivation>> {
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

pub(crate) fn suspend_snapshot(
    conn: &Connection,
    activation_id: i64,
) -> Result<StoredSuspendSnapshot> {
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

pub(crate) fn insert_event(conn: &Connection, event: NewEvent<'_>) -> Result<i64> {
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

#[derive(Debug)]
pub(crate) struct StoredActivation {
    pub(crate) activation_id: i64,
    pub(crate) project_id: i64,
    pub(crate) work_unit_id: i64,
    pub(crate) stack_depth: i64,
    pub(crate) status: String,
}

#[derive(Debug)]
pub(crate) struct StoredSuspendSnapshot {
    pub(crate) id: i64,
    pub(crate) reason: String,
    pub(crate) next_action: String,
}

pub(crate) struct NewEvent<'a> {
    pub(crate) work_unit_id: i64,
    pub(crate) activation_id: Option<i64>,
    pub(crate) related_activation_id: Option<i64>,
    pub(crate) event_type: &'a str,
    pub(crate) reason: Option<&'a str>,
    pub(crate) status_domain: &'a str,
    pub(crate) previous_status: Option<&'a str>,
    pub(crate) next_status: Option<&'a str>,
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
    source text not null default 'user' check (source in ('user', 'plan', 'review', 'coverage', 'design', 'work_record')),
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

create table if not exists acceptance_records (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    target_type text not null check (target_type in ('task')),
    task_id integer references tasks(id),
    acceptance_type text not null check (acceptance_type in ('accepted_out_of_scope', 'explicit_exception')),
    reason text not null,
    scope text,
    created_by text not null check (created_by in ('user', 'agent', 'system')),
    status text not null check (status in ('proposed', 'approved', 'rejected', 'expired')),
    approved_by_authority_event_id integer references authority_events(id),
    approved_at text,
    created_at text not null,
    review_impact text
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

create table if not exists work_records (
    id integer primary key,
    work_unit_id integer references work_units(id) on delete cascade,
    topic text not null,
    work_performed text,
    next_actions text,
    notable_operations text,
    export_path text,
    created_at text not null
);

create table if not exists work_record_commands (
    id integer primary key,
    work_record_id integer not null references work_records(id) on delete cascade,
    command_usage_id integer references command_usages(id),
    command_profile_id integer references command_profiles(id),
    command text,
    result text,
    log_path text,
    note text
);

create table if not exists work_record_commits (
    id integer primary key,
    work_record_id integer not null references work_records(id) on delete cascade,
    git_commit_id integer,
    commit_sha text,
    role text not null default 'referenced' check (role in ('created', 'referenced', 'validation_base', 'rollback_point')),
    note text
);

create table if not exists work_record_files (
    id integer primary key,
    work_record_id integer not null references work_records(id) on delete cascade,
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
    source_work_record_id integer references work_records(id),
    source_repository_snapshot_id integer,
    source_git_commit_id integer,
    source_git_commit_sha text,
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
