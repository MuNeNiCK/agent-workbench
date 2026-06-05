use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

pub const LEDGER_DIR: &str = ".agent-workbench";
pub const LEDGER_FILE: &str = "ledger.sqlite";
pub const DESIGN_DIR: &str = "designs";
pub const EXPORT_DIR: &str = "exports";
pub const LOG_DIR: &str = "logs";
pub(crate) const SCHEMA_VERSION: i64 = 4;

pub fn default_ledger_path(root: &Path) -> PathBuf {
    root.join(LEDGER_DIR).join(LEDGER_FILE)
}

pub fn default_design_root(root: &Path) -> PathBuf {
    root.join(LEDGER_DIR).join(DESIGN_DIR)
}

pub fn default_export_root(root: &Path) -> PathBuf {
    root.join(LEDGER_DIR).join(EXPORT_DIR)
}

pub fn default_log_root(root: &Path) -> PathBuf {
    root.join(LEDGER_DIR).join(LOG_DIR)
}

pub fn init_project(root: &Path) -> Result<InitOutcome> {
    let ledger_path = default_ledger_path(root);
    if let Some(parent) = ledger_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create ledger directory {}", parent.display()))?;
    }
    for directory in [
        default_design_root(root),
        default_export_root(root),
        default_log_root(root),
    ] {
        fs::create_dir_all(&directory)
            .with_context(|| format!("failed to create directory {}", directory.display()))?;
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
    prepare_acceptance_records_for_schema(conn)?;
    conn.execute_batch(SCHEMA)?;
    migrate_acceptance_records(conn)?;
    migrate_kpt_items(conn)?;
    migrate_review_runs(conn)?;
    conn.execute_batch(SCHEMA)?;
    ensure_column(conn, "work_record_forks", "source_git_commit_sha", "text")?;
    ensure_column(conn, "acceptance_records", "design_package_key", "text")?;
    ensure_column(conn, "acceptance_records", "design_file_path", "text")?;
    ensure_column(conn, "acceptance_records", "design_requirement_key", "text")?;
    ensure_column(conn, "acceptance_records", "coverage_item_id", "integer")?;

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

fn prepare_acceptance_records_for_schema(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "acceptance_records")? {
        return Ok(());
    }
    ensure_column(conn, "acceptance_records", "design_package_key", "text")?;
    ensure_column(conn, "acceptance_records", "design_file_path", "text")?;
    ensure_column(conn, "acceptance_records", "design_requirement_key", "text")?;
    ensure_column(conn, "acceptance_records", "coverage_item_id", "integer")?;
    Ok(())
}

fn migrate_acceptance_records(conn: &Connection) -> Result<()> {
    let table_sql = conn
        .query_row(
            "select sql from sqlite_schema where type = 'table' and name = 'acceptance_records'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(table_sql) = table_sql else {
        return Ok(());
    };
    if table_sql.contains("'design_file'")
        && table_sql.contains("'coverage_item'")
        && table_sql.contains("'design_requirement_key'")
        && table_has_column(conn, "acceptance_records", "design_package_key")?
        && table_has_column(conn, "acceptance_records", "design_file_path")?
        && table_has_column(conn, "acceptance_records", "design_requirement_key")?
        && table_has_column(conn, "acceptance_records", "coverage_item_id")?
    {
        return Ok(());
    }

    conn.execute_batch(
        r#"
        drop trigger if exists trg_acceptance_design_requirement_project_insert;
        drop trigger if exists trg_acceptance_design_requirement_project_update;
        drop trigger if exists trg_acceptance_task_project_insert;
        drop trigger if exists trg_acceptance_task_project_update;
        drop trigger if exists trg_acceptance_validation_gate_template_project_insert;
        drop trigger if exists trg_acceptance_validation_gate_template_project_update;
        alter table acceptance_records rename to acceptance_records_old;

        create table acceptance_records (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            target_type text not null check (target_type in ('task', 'design_requirement', 'validation_gate_template', 'design_file', 'design_requirement_key', 'coverage_item')),
            task_id integer references tasks(id),
            design_requirement_id integer references design_requirements(id),
            validation_gate_template_id integer references validation_gate_templates(id),
            coverage_item_id integer references coverage_items(id),
            design_package_key text,
            design_file_path text,
            design_requirement_key text,
            acceptance_type text not null check (acceptance_type in ('accepted_out_of_scope', 'explicit_exception')),
            reason text not null,
            scope text,
            created_by text not null check (created_by in ('user', 'agent', 'system')),
            status text not null check (status in ('proposed', 'approved', 'rejected', 'expired')),
            approved_by_authority_event_id integer references authority_events(id),
            approved_at text,
            created_at text not null,
            review_impact text,
            check (
                (target_type = 'task' and task_id is not null and design_requirement_id is null and validation_gate_template_id is null and coverage_item_id is null and design_package_key is null and design_file_path is null and design_requirement_key is null)
                or (target_type = 'design_requirement' and task_id is null and design_requirement_id is not null and validation_gate_template_id is null and coverage_item_id is null and design_package_key is null and design_file_path is null and design_requirement_key is null)
                or (target_type = 'validation_gate_template' and task_id is null and design_requirement_id is null and validation_gate_template_id is not null and coverage_item_id is null and design_package_key is null and design_file_path is null and design_requirement_key is null)
                or (target_type = 'coverage_item' and task_id is null and design_requirement_id is null and validation_gate_template_id is null and coverage_item_id is not null and design_package_key is null and design_file_path is null and design_requirement_key is null)
                or (target_type = 'design_file' and task_id is null and design_requirement_id is null and validation_gate_template_id is null and coverage_item_id is null and design_package_key is not null and design_file_path is not null and design_requirement_key is null)
                or (target_type = 'design_requirement_key' and task_id is null and design_requirement_id is null and validation_gate_template_id is null and coverage_item_id is null and design_package_key is not null and design_file_path is null and design_requirement_key is not null)
            )
        );

        insert into acceptance_records(
            id, project_id, target_type, task_id, design_requirement_id,
            validation_gate_template_id, coverage_item_id, acceptance_type, reason, scope,
            created_by, status, approved_by_authority_event_id, approved_at,
            created_at, review_impact
        )
        select
            id, project_id, target_type, task_id, design_requirement_id,
            validation_gate_template_id, coverage_item_id, acceptance_type, reason, scope,
            case
                when created_by in ('user', 'agent', 'system') then created_by
                else 'system'
            end,
            case
                when status in ('proposed', 'approved', 'rejected', 'expired') then status
                when status = 'revoked' then 'rejected'
                else 'approved'
            end,
            approved_by_authority_event_id, approved_at,
            created_at, review_impact
        from acceptance_records_old;

        drop table acceptance_records_old;
        "#,
    )?;
    Ok(())
}

fn migrate_kpt_items(conn: &Connection) -> Result<()> {
    let table_sql = conn
        .query_row(
            "select sql from sqlite_schema where type = 'table' and name = 'kpt_items'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(table_sql) = table_sql else {
        return Ok(());
    };
    let conversion_sql = conn
        .query_row(
            "select sql from sqlite_schema where type = 'table' and name = 'kpt_item_conversions'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let conversion_sql = conversion_sql.unwrap_or_default();
    if table_sql.contains("'converted'")
        && table_sql.contains("linked_review_finding_id integer references findings")
        && conversion_sql.contains("review_policy_id integer references review_policies")
        && conversion_sql.contains("design_version_id integer references design_versions")
        && conversion_sql.contains("target_type = 'task'")
    {
        return Ok(());
    }

    conn.execute_batch(
        r#"
        pragma foreign_keys = off;

        alter table kpt_item_conversions rename to kpt_item_conversions_old;
        alter table kpt_items rename to kpt_items_old;

        create table kpt_items (
            id integer primary key,
            kpt_review_id integer not null references kpt_reviews(id) on delete cascade,
            item_type text not null check (item_type in ('keep', 'problem', 'try')),
            title text not null,
            details text,
            severity text not null default 'medium' check (severity in ('critical', 'high', 'medium', 'low')),
            linked_user_correction_id integer references user_corrections(id),
            linked_command_profile_id integer references command_profiles(id),
            linked_review_finding_id integer references findings(id),
            linked_task_id integer references tasks(id),
            proposed_action text,
            status text not null default 'open' check (status in ('open', 'accepted', 'converted', 'converted_to_task', 'dismissed')),
            created_at text not null
        );

        insert into kpt_items(
            id, kpt_review_id, item_type, title, details, severity,
            linked_user_correction_id, linked_command_profile_id,
            linked_review_finding_id, linked_task_id, proposed_action, status, created_at
        )
        select
            id, kpt_review_id, item_type, title, details, severity,
            linked_user_correction_id, linked_command_profile_id,
            linked_review_finding_id, linked_task_id, proposed_action, status, created_at
        from kpt_items_old;

        create table kpt_item_conversions (
            id integer primary key,
            kpt_item_id integer not null references kpt_items(id) on delete cascade,
            target_type text not null check (target_type in ('task', 'command_profile', 'review_policy', 'design_version', 'decision', 'user_correction')),
            task_id integer references tasks(id),
            command_profile_id integer references command_profiles(id),
            review_policy_id integer references review_policies(id),
            design_version_id integer references design_versions(id),
            decision_id integer references decisions(id),
            user_correction_id integer references user_corrections(id),
            created_at text not null,
            check (
                (target_type = 'task' and task_id is not null and command_profile_id is null and review_policy_id is null and design_version_id is null and decision_id is null and user_correction_id is null)
                or (target_type = 'command_profile' and task_id is null and command_profile_id is not null and review_policy_id is null and design_version_id is null and decision_id is null and user_correction_id is null)
                or (target_type = 'review_policy' and task_id is null and command_profile_id is null and review_policy_id is not null and design_version_id is null and decision_id is null and user_correction_id is null)
                or (target_type = 'design_version' and task_id is null and command_profile_id is null and review_policy_id is null and design_version_id is not null and decision_id is null and user_correction_id is null)
                or (target_type = 'decision' and task_id is null and command_profile_id is null and review_policy_id is null and design_version_id is null and decision_id is not null and user_correction_id is null)
                or (target_type = 'user_correction' and task_id is null and command_profile_id is null and review_policy_id is null and design_version_id is null and decision_id is null and user_correction_id is not null)
            )
        );

        insert into kpt_item_conversions(
            id, kpt_item_id, target_type, task_id, command_profile_id,
            review_policy_id, design_version_id, decision_id, user_correction_id, created_at
        )
        select
            id, kpt_item_id, target_type, task_id, command_profile_id,
            review_policy_id, design_version_id, decision_id, user_correction_id, created_at
        from kpt_item_conversions_old;

        drop table kpt_item_conversions_old;
        drop table kpt_items_old;

        pragma foreign_keys = on;
        "#,
    )?;

    Ok(())
}

fn migrate_review_runs(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "review_runs")? {
        return Ok(());
    }

    let invalid_count: i64 = conn.query_row(
        r#"
        select count(*)
        from review_runs
        where not (
            (run_type = 'fresh' and run_purpose = 'new_unbiased_review')
            or (run_type = 'resume' and run_purpose = 'finding_fix_verification')
            or (run_type = 'coverage' and run_purpose = 'coverage_audit')
        )
        "#,
        [],
        |row| row.get(0),
    )?;
    if invalid_count > 0 {
        bail!("review_runs contains invalid run_type/run_purpose combinations");
    }
    conn.execute_batch(
        r#"
        create trigger if not exists trg_review_run_type_purpose_insert
        before insert on review_runs
        for each row
        when not (
            (new.run_type = 'fresh' and new.run_purpose = 'new_unbiased_review')
            or (new.run_type = 'resume' and new.run_purpose = 'finding_fix_verification')
            or (new.run_type = 'coverage' and new.run_purpose = 'coverage_audit')
        )
        begin
            select raise(abort, 'review run type must match purpose');
        end;

        create trigger if not exists trg_review_run_type_purpose_update
        before update of run_type, run_purpose on review_runs
        for each row
        when not (
            (new.run_type = 'fresh' and new.run_purpose = 'new_unbiased_review')
            or (new.run_type = 'resume' and new.run_purpose = 'finding_fix_verification')
            or (new.run_type = 'coverage' and new.run_purpose = 'coverage_audit')
        )
        begin
            select raise(abort, 'review run type must match purpose');
        end;
        "#,
    )?;

    Ok(())
}

fn ensure_column(conn: &Connection, table: &str, column: &str, definition: &str) -> Result<()> {
    if table_has_column(conn, table, column)? {
        return Ok(());
    }

    conn.execute(
        &format!("alter table {table} add column {column} {definition}"),
        [],
    )?;
    Ok(())
}

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("pragma table_info({table})"))?;
    let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for existing in columns {
        if existing? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    let exists = conn
        .query_row(
            "select 1 from sqlite_schema where type = 'table' and name = ?1",
            params![table],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok(exists)
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
    target_type text not null check (target_type in ('task', 'design_requirement', 'validation_gate_template', 'design_file', 'design_requirement_key', 'coverage_item')),
    task_id integer references tasks(id),
    design_requirement_id integer references design_requirements(id),
    validation_gate_template_id integer references validation_gate_templates(id),
    coverage_item_id integer references coverage_items(id),
    design_package_key text,
    design_file_path text,
    design_requirement_key text,
    acceptance_type text not null check (acceptance_type in ('accepted_out_of_scope', 'explicit_exception')),
    reason text not null,
    scope text,
    created_by text not null check (created_by in ('user', 'agent', 'system')),
    status text not null check (status in ('proposed', 'approved', 'rejected', 'expired')),
    approved_by_authority_event_id integer references authority_events(id),
    approved_at text,
    created_at text not null,
    review_impact text,
    check (
        (target_type = 'task' and task_id is not null and design_requirement_id is null and validation_gate_template_id is null and coverage_item_id is null and design_package_key is null and design_file_path is null and design_requirement_key is null)
        or (target_type = 'design_requirement' and task_id is null and design_requirement_id is not null and validation_gate_template_id is null and coverage_item_id is null and design_package_key is null and design_file_path is null and design_requirement_key is null)
        or (target_type = 'validation_gate_template' and task_id is null and design_requirement_id is null and validation_gate_template_id is not null and coverage_item_id is null and design_package_key is null and design_file_path is null and design_requirement_key is null)
        or (target_type = 'coverage_item' and task_id is null and design_requirement_id is null and validation_gate_template_id is null and coverage_item_id is not null and design_package_key is null and design_file_path is null and design_requirement_key is null)
        or (target_type = 'design_file' and task_id is null and design_requirement_id is null and validation_gate_template_id is null and coverage_item_id is null and design_package_key is not null and design_file_path is not null and design_requirement_key is null)
        or (target_type = 'design_requirement_key' and task_id is null and design_requirement_id is null and validation_gate_template_id is null and coverage_item_id is null and design_package_key is not null and design_file_path is null and design_requirement_key is not null)
    )
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

create table if not exists design_packages (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    design_key text not null,
    title text not null,
    status text not null default 'draft' check (status in ('draft', 'imported', 'approved', 'superseded', 'archived')),
    current_design_version_id integer,
    created_at text not null,
    updated_at text not null,
    unique(project_id, design_key)
);

create table if not exists design_versions (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    design_package_id integer not null references design_packages(id) on delete cascade,
    version_number integer not null,
    content_hash text not null,
    package_path text not null,
    manifest_path text not null,
    format text not null,
    manifest_version integer not null,
    status text not null default 'draft' check (status in ('draft', 'imported', 'approved', 'superseded', 'rejected')),
    imported_at text not null,
    approved_by_authority_event_id integer references authority_events(id),
    unique(design_package_id, version_number),
    unique(design_package_id, content_hash)
);

create table if not exists design_files (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    design_package_id integer not null references design_packages(id) on delete cascade,
    design_version_id integer not null references design_versions(id) on delete cascade,
    section_key text not null,
    relative_path text not null,
    content_hash text not null,
    line_count integer not null,
    unique(design_version_id, relative_path)
);

create table if not exists design_requirements (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    design_version_id integer not null references design_versions(id) on delete cascade,
    source_design_file_id integer not null references design_files(id) on delete cascade,
    source_section text not null,
    requirement_key text not null,
    revision integer not null default 1,
    requirement_hash text not null,
    supersedes_requirement_id integer references design_requirements(id),
    requirement_text text not null,
    priority text not null check (priority in ('critical', 'high', 'medium', 'low')),
    required_surfaces text,
    validation_expectation text,
    status text not null check (status in ('active', 'superseded', 'accepted_out_of_scope')),
    created_at text not null,
    unique(design_version_id, requirement_key)
);

create table if not exists design_decisions (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    design_version_id integer not null references design_versions(id) on delete cascade,
    source_design_file_id integer not null references design_files(id) on delete cascade,
    source_section text not null,
    decision_key text not null,
    decision_hash text not null,
    topic text not null,
    decision_text text not null,
    rationale text,
    supersedes_decision_keys text,
    status text not null check (status in ('accepted', 'rejected', 'superseded')),
    created_at text not null,
    unique(design_version_id, decision_key)
);

create table if not exists validation_gate_templates (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    design_version_id integer not null references design_versions(id) on delete cascade,
    source_design_file_id integer not null references design_files(id) on delete cascade,
    source_section text not null,
    gate_key text not null,
    gate_hash text not null,
    stage text not null check (stage in ('design-ready', 'implementation-ready', 'close-ready', 'resume-ready')),
    command text,
    expected_result text not null,
    requirement_keys text,
    gate_text text not null,
    status text not null check (status in ('active', 'superseded', 'accepted_out_of_scope')),
    created_at text not null,
    unique(design_version_id, gate_key)
);

create table if not exists validation_gate_template_requirements (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    validation_gate_template_id integer not null references validation_gate_templates(id) on delete cascade,
    design_requirement_id integer not null references design_requirements(id) on delete cascade,
    unique(validation_gate_template_id, design_requirement_id)
);

create table if not exists checklists (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    work_unit_id integer not null references work_units(id) on delete cascade,
    design_version_id integer not null references design_versions(id) on delete cascade,
    title text not null,
    status text not null default 'active' check (status in ('active', 'stale', 'closed')),
    created_by_review_run_id integer,
    created_at text not null
);

create table if not exists checklist_items (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    checklist_id integer not null references checklists(id) on delete cascade,
    design_requirement_id integer not null references design_requirements(id) on delete cascade,
    task_id integer not null references tasks(id) on delete cascade,
    item_order integer not null,
    title text not null,
    completion_condition text,
    status text not null default 'open' check (status in ('open', 'blocked', 'closed', 'accepted_out_of_scope')),
    unique(checklist_id, item_order)
);

create table if not exists task_derivations (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    design_requirement_id integer not null references design_requirements(id) on delete cascade,
    task_id integer not null references tasks(id) on delete cascade,
    checklist_item_id integer references checklist_items(id) on delete set null,
    derivation_reason text,
    generated_by_review_run_id integer,
    status text not null default 'active' check (status in ('active', 'stale', 'closed')),
    created_at text not null,
    unique(design_requirement_id, task_id)
);

create table if not exists validation_gates (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    gate_key text not null,
    template_id integer references validation_gate_templates(id),
    work_unit_id integer references work_units(id) on delete cascade,
    task_id integer references tasks(id) on delete cascade,
    design_requirement_id integer references design_requirements(id) on delete cascade,
    command_profile_id integer references command_profiles(id),
    command text,
    expected_result text not null,
    environment text,
    timeout text,
    artifact_requirements text,
    selected_before_edit integer not null default 1 check (selected_before_edit in (0, 1)),
    status text not null default 'active' check (status in ('active', 'stale', 'closed')),
    created_at text not null
);

create table if not exists implementation_evidence (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    task_id integer references tasks(id) on delete cascade,
    design_requirement_id integer references design_requirements(id) on delete cascade,
    evidence_type text not null check (evidence_type in ('commit', 'file', 'symbol', 'test', 'artifact', 'command_output', 'manual_note')),
    repository_id integer,
    git_commit_id integer,
    git_file_change_id integer,
    commit_sha text,
    file_path text,
    line_ref text,
    symbol text,
    artifact_path text,
    note text,
    created_at text not null,
    check (task_id is not null or design_requirement_id is not null)
);

create table if not exists coverage_items (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    review_scope_id integer,
    work_unit_id integer references work_units(id) on delete cascade,
    design_requirement_id integer not null references design_requirements(id) on delete cascade,
    task_id integer references tasks(id) on delete cascade,
    requirement text not null,
    runtime_boundary_evidence text,
    ux_boundary_evidence text,
    lifecycle_boundary_evidence text,
    tests_or_gates text,
    missing_or_unverified text,
    status text not null check (status in ('covered', 'partial', 'missing_required_surface', 'design_conflict', 'accepted_out_of_scope', 'needs_evidence')),
    created_at text not null
);

create trigger if not exists trg_gate_template_requirement_project_insert
before insert on validation_gate_template_requirements
for each row
when new.project_id != (select project_id from validation_gate_templates where id = new.validation_gate_template_id)
  or new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
begin
    select raise(abort, 'validation gate template requirement project_id must match referenced rows');
end;

create trigger if not exists trg_gate_template_requirement_project_update
before update of project_id, validation_gate_template_id, design_requirement_id on validation_gate_template_requirements
for each row
when new.project_id != (select project_id from validation_gate_templates where id = new.validation_gate_template_id)
  or new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
begin
    select raise(abort, 'validation gate template requirement project_id must match referenced rows');
end;

create trigger if not exists trg_checklist_project_insert
before insert on checklists
for each row
when new.project_id != (select project_id from work_units where id = new.work_unit_id)
  or new.project_id != (select project_id from design_versions where id = new.design_version_id)
begin
    select raise(abort, 'checklist project_id must match referenced rows');
end;

create trigger if not exists trg_checklist_project_update
before update of project_id, work_unit_id, design_version_id on checklists
for each row
when new.project_id != (select project_id from work_units where id = new.work_unit_id)
  or new.project_id != (select project_id from design_versions where id = new.design_version_id)
begin
    select raise(abort, 'checklist project_id must match referenced rows');
end;

create trigger if not exists trg_checklist_item_project_insert
before insert on checklist_items
for each row
when new.project_id != (select project_id from checklists where id = new.checklist_id)
  or new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
  or new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  )
begin
    select raise(abort, 'checklist item project_id must match referenced rows');
end;

create trigger if not exists trg_checklist_item_project_update
before update of project_id, checklist_id, design_requirement_id, task_id on checklist_items
for each row
when new.project_id != (select project_id from checklists where id = new.checklist_id)
  or new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
  or new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  )
begin
    select raise(abort, 'checklist item project_id must match referenced rows');
end;

create trigger if not exists trg_task_derivation_project_insert
before insert on task_derivations
for each row
when new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
  or new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  )
  or (new.checklist_item_id is not null and new.project_id != (select project_id from checklist_items where id = new.checklist_item_id))
begin
    select raise(abort, 'task derivation project_id must match referenced rows');
end;

create trigger if not exists trg_task_derivation_project_update
before update of project_id, design_requirement_id, task_id, checklist_item_id on task_derivations
for each row
when new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
  or new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  )
  or (new.checklist_item_id is not null and new.project_id != (select project_id from checklist_items where id = new.checklist_item_id))
begin
    select raise(abort, 'task derivation project_id must match referenced rows');
end;

create trigger if not exists trg_validation_gate_project_insert
before insert on validation_gates
for each row
when (new.template_id is not null and new.project_id != (select project_id from validation_gate_templates where id = new.template_id))
  or (new.work_unit_id is not null and new.project_id != (select project_id from work_units where id = new.work_unit_id))
  or (new.task_id is not null and new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
  or (new.design_requirement_id is not null and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id))
begin
    select raise(abort, 'validation gate project_id must match referenced rows');
end;

create trigger if not exists trg_validation_gate_project_update
before update of project_id, template_id, work_unit_id, task_id, design_requirement_id on validation_gates
for each row
when (new.template_id is not null and new.project_id != (select project_id from validation_gate_templates where id = new.template_id))
  or (new.work_unit_id is not null and new.project_id != (select project_id from work_units where id = new.work_unit_id))
  or (new.task_id is not null and new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
  or (new.design_requirement_id is not null and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id))
begin
    select raise(abort, 'validation gate project_id must match referenced rows');
end;

create trigger if not exists trg_implementation_evidence_project_insert
before insert on implementation_evidence
for each row
when (new.task_id is not null and new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
  or (new.design_requirement_id is not null and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id))
begin
    select raise(abort, 'implementation evidence project_id must match referenced rows');
end;

create trigger if not exists trg_implementation_evidence_project_update
before update of project_id, task_id, design_requirement_id on implementation_evidence
for each row
when (new.task_id is not null and new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
  or (new.design_requirement_id is not null and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id))
begin
    select raise(abort, 'implementation evidence project_id must match referenced rows');
end;

create trigger if not exists trg_coverage_item_project_insert
before insert on coverage_items
for each row
when (new.work_unit_id is not null and new.project_id != (select project_id from work_units where id = new.work_unit_id))
  or new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
  or (new.task_id is not null and new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
begin
    select raise(abort, 'coverage item project_id must match referenced rows');
end;

create trigger if not exists trg_coverage_item_project_update
before update of project_id, work_unit_id, design_requirement_id, task_id on coverage_items
for each row
when (new.work_unit_id is not null and new.project_id != (select project_id from work_units where id = new.work_unit_id))
  or new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
  or (new.task_id is not null and new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
begin
    select raise(abort, 'coverage item project_id must match referenced rows');
end;

create trigger if not exists trg_acceptance_design_requirement_project_insert
before insert on acceptance_records
for each row
when new.target_type = 'design_requirement'
 and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
begin
    select raise(abort, 'acceptance project_id must match design requirement project_id');
end;

create trigger if not exists trg_acceptance_design_requirement_project_update
before update of project_id, target_type, design_requirement_id on acceptance_records
for each row
when new.target_type = 'design_requirement'
 and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
begin
    select raise(abort, 'acceptance project_id must match design requirement project_id');
end;

create trigger if not exists trg_acceptance_task_project_insert
before insert on acceptance_records
for each row
when new.target_type = 'task'
 and new.project_id != coalesce(
     (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
     (select id from projects order by id limit 1)
 )
begin
    select raise(abort, 'acceptance project_id must match task project_id');
end;

create trigger if not exists trg_acceptance_task_project_update
before update of project_id, target_type, task_id on acceptance_records
for each row
when new.target_type = 'task'
 and new.project_id != coalesce(
     (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
     (select id from projects order by id limit 1)
 )
begin
    select raise(abort, 'acceptance project_id must match task project_id');
end;

create trigger if not exists trg_acceptance_validation_gate_template_project_insert
before insert on acceptance_records
for each row
when new.target_type = 'validation_gate_template'
 and new.project_id != (select project_id from validation_gate_templates where id = new.validation_gate_template_id)
begin
    select raise(abort, 'acceptance project_id must match validation gate template project_id');
end;

create trigger if not exists trg_acceptance_validation_gate_template_project_update
before update of project_id, target_type, validation_gate_template_id on acceptance_records
for each row
when new.target_type = 'validation_gate_template'
 and new.project_id != (select project_id from validation_gate_templates where id = new.validation_gate_template_id)
begin
    select raise(abort, 'acceptance project_id must match validation gate template project_id');
end;

create trigger if not exists trg_acceptance_coverage_item_project_insert
before insert on acceptance_records
for each row
when new.target_type = 'coverage_item'
 and new.project_id != (select project_id from coverage_items where id = new.coverage_item_id)
begin
    select raise(abort, 'acceptance project_id must match coverage item project_id');
end;

create trigger if not exists trg_acceptance_coverage_item_project_update
before update of project_id, target_type, coverage_item_id on acceptance_records
for each row
when new.target_type = 'coverage_item'
 and new.project_id != (select project_id from coverage_items where id = new.coverage_item_id)
begin
    select raise(abort, 'acceptance project_id must match coverage item project_id');
end;

create table if not exists review_policies (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    name text not null,
    review_type text not null check (review_type in ('design_review', 'design_implementation_diff', 'design_task_decomposition', 'implementation_review', 'general')),
    max_fresh_agents integer not null default 1,
    max_resume_agents integer not null default 1,
    max_parallel_agents integer not null default 1,
    required_consecutive_clean_fresh_runs integer not null default 1,
    required_consecutive_clean_resume_runs integer not null default 1,
    stop_on_severity text not null default 'none' check (stop_on_severity in ('critical', 'high', 'medium', 'low', 'none')),
    allow_resume_review integer not null default 1 check (allow_resume_review in (0, 1)),
    allow_fresh_review integer not null default 1 check (allow_fresh_review in (0, 1)),
    allow_new_findings_in_resume integer not null default 0 check (allow_new_findings_in_resume in (0, 1)),
    on_max_agents_exceeded text not null default 'block' check (on_max_agents_exceeded in ('block', 'accept_with_user_approval', 'mark_exhausted')),
    run_count_scope text not null default 'review_plan' check (run_count_scope in ('review_plan', 'review_scope', 'work_unit')),
    default_run_mode text not null default 'fresh' check (default_run_mode in ('fresh', 'resume')),
    created_at text not null,
    unique(project_id, name)
);

create table if not exists review_scopes (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    name text not null,
    review_type text not null check (review_type in ('design_review', 'design_implementation_diff', 'design_task_decomposition', 'implementation_review', 'general')),
    agent_role text not null check (agent_role in ('general', 'design_document_review', 'design_task_decomposition', 'design_implementation_diff_review', 'implementation_review')),
    user_declared_scope text not null,
    allowed_inputs text,
    forbidden_judgments text,
    expected_output_type text,
    exclusions text,
    prompt_template_ref text,
    status text not null default 'open' check (status in ('open', 'blocked', 'clean', 'closed')),
    no_findings_streak integer not null default 0,
    created_at text not null,
    unique(project_id, name)
);

create table if not exists review_plans (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    work_unit_id integer not null references work_units(id) on delete cascade,
    design_version_id integer references design_versions(id) on delete cascade,
    review_type text not null check (review_type in ('design_review', 'design_implementation_diff', 'design_task_decomposition', 'implementation_review', 'general')),
    required integer not null default 1 check (required in (0, 1)),
    stage text not null check (stage in ('design-ready', 'implementation-ready', 'close-ready', 'resume-ready')),
    scope text,
    clean_condition text,
    stop_condition text,
    review_policy_id integer references review_policies(id),
    review_scope_id integer references review_scopes(id),
    status text not null default 'open' check (status in ('open', 'blocked', 'clean', 'accepted_exception', 'not_required', 'exhausted', 'needs_user_decision')),
    created_at text not null
);

create trigger if not exists trg_review_policy_referenced_update
before update of project_id, review_type on review_policies
for each row
when exists (
    select 1
    from review_plans p
    where p.review_policy_id = old.id
      and (p.project_id != new.project_id or p.review_type != new.review_type)
)
begin
    select raise(abort, 'review policy update would break referenced review plans');
end;

create trigger if not exists trg_review_scope_referenced_update
before update of project_id, review_type on review_scopes
for each row
when exists (
    select 1
    from review_plans p
    where p.review_scope_id = old.id
      and (p.project_id != new.project_id or p.review_type != new.review_type)
)
or exists (
    select 1
    from review_runs r
    where r.review_scope_id = old.id
      and r.project_id != new.project_id
)
begin
    select raise(abort, 'review scope update would break referenced review plans or runs');
end;

create table if not exists review_plan_targets (
    id integer primary key,
    review_plan_id integer not null references review_plans(id) on delete cascade,
    target_type text not null check (target_type in ('design_version', 'design_requirement', 'task', 'work_unit', 'repository_snapshot', 'file', 'symbol')),
    design_version_id integer references design_versions(id),
    design_requirement_id integer references design_requirements(id),
    task_id integer references tasks(id),
    work_unit_id integer references work_units(id),
    repository_snapshot_id integer,
    file_path text,
    symbol text,
    check (
        (target_type = 'design_version' and design_version_id is not null and design_requirement_id is null and task_id is null and work_unit_id is null and repository_snapshot_id is null and file_path is null and symbol is null)
        or (target_type = 'design_requirement' and design_version_id is null and design_requirement_id is not null and task_id is null and work_unit_id is null and repository_snapshot_id is null and file_path is null and symbol is null)
        or (target_type = 'task' and design_version_id is null and design_requirement_id is null and task_id is not null and work_unit_id is null and repository_snapshot_id is null and file_path is null and symbol is null)
        or (target_type = 'work_unit' and design_version_id is null and design_requirement_id is null and task_id is null and work_unit_id is not null and repository_snapshot_id is null and file_path is null and symbol is null)
        or (target_type = 'repository_snapshot' and design_version_id is null and design_requirement_id is null and task_id is null and work_unit_id is null and repository_snapshot_id is not null and file_path is null and symbol is null)
        or (target_type = 'file' and design_version_id is null and design_requirement_id is null and task_id is null and work_unit_id is null and repository_snapshot_id is null and file_path is not null and symbol is null)
        or (target_type = 'symbol' and design_version_id is null and design_requirement_id is null and task_id is null and work_unit_id is null and repository_snapshot_id is null and file_path is null and symbol is not null)
    )
);

create table if not exists review_runs (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    review_scope_id integer references review_scopes(id),
    review_plan_id integer references review_plans(id),
    run_type text not null check (run_type in ('fresh', 'resume', 'coverage')),
    run_purpose text not null check (run_purpose in ('new_unbiased_review', 'finding_fix_verification', 'coverage_audit')),
    target_type text not null check (target_type in ('design_version', 'design_requirement', 'task', 'work_unit', 'repository_snapshot', 'file', 'symbol')),
    design_version_id integer references design_versions(id),
    design_requirement_id integer references design_requirements(id),
    task_id integer references tasks(id),
    work_unit_id integer references work_units(id),
    repository_snapshot_id integer,
    target_ref text,
    prompt_deviations text,
    result_summary text,
    new_findings_count integer not null default 0 check (new_findings_count >= 0),
    carried_findings_checked integer not null default 0 check (carried_findings_checked >= 0),
    clean_run integer not null default 0 check (clean_run in (0, 1)),
    status text not null default 'requested' check (status in ('requested', 'running', 'completed', 'failed', 'cancelled')),
    created_at text not null,
    check (
        (run_type = 'fresh' and run_purpose = 'new_unbiased_review')
        or (run_type = 'resume' and run_purpose = 'finding_fix_verification')
        or (run_type = 'coverage' and run_purpose = 'coverage_audit')
    ),
    check (
        clean_run = 0
        or (status = 'completed' and new_findings_count = 0)
    )
);

create table if not exists review_agent_invocations (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    review_plan_id integer references review_plans(id),
    review_run_id integer references review_runs(id),
    run_type text not null check (run_type in ('fresh', 'resume', 'coverage')),
    agent_label text,
    external_agent_id text,
    status text not null default 'requested' check (status in ('requested', 'running', 'completed', 'failed', 'cancelled')),
    started_at text,
    finished_at text
);

create table if not exists findings (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    review_run_id integer not null references review_runs(id) on delete cascade,
    finding_type text not null check (finding_type in ('design_finding', 'design_implementation_drift', 'design_task_gap', 'implementation_finding', 'coverage_finding')),
    severity text not null check (severity in ('critical', 'high', 'medium', 'low')),
    description text not null,
    classification text not null default 'unclassified' check (classification in ('unclassified', 'valid', 'invalid', 'design_conflict', 'needs_evidence')),
    status text not null default 'open' check (status in ('open', 'closed', 'accepted_out_of_scope')),
    design_requirement_id integer references design_requirements(id),
    task_id integer references tasks(id),
    created_at text not null
);

create table if not exists closures (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    finding_id integer not null references findings(id) on delete cascade,
    design_invariant text not null,
    design_citations text,
    implementation_evidence text,
    affected_surfaces text,
    same_invariant_search text,
    other_violations_found text,
    fix_plan text,
    tests_or_gates text,
    verification_plan text,
    closed_by_commit text,
    created_at text not null
);

create table if not exists finding_verifications (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    review_run_id integer not null references review_runs(id) on delete cascade,
    finding_id integer not null references findings(id) on delete cascade,
    closure_id integer not null references closures(id) on delete cascade,
    result text not null check (result in ('verified', 'not_fixed', 'needs_evidence', 'out_of_scope')),
    notes text,
    created_at text not null,
    unique(review_run_id, finding_id, closure_id)
);

create trigger if not exists trg_review_plan_project_insert
before insert on review_plans
for each row
when new.project_id != (select project_id from work_units where id = new.work_unit_id)
  or (new.design_version_id is not null and new.project_id != (select project_id from design_versions where id = new.design_version_id))
  or (new.review_policy_id is not null and new.project_id != (select project_id from review_policies where id = new.review_policy_id))
  or (new.review_scope_id is not null and new.project_id != (select project_id from review_scopes where id = new.review_scope_id))
begin
    select raise(abort, 'review plan project_id must match referenced rows');
end;

create trigger if not exists trg_review_plan_project_update
before update of project_id, work_unit_id, design_version_id, review_policy_id, review_scope_id on review_plans
for each row
when new.project_id != (select project_id from work_units where id = new.work_unit_id)
  or (new.design_version_id is not null and new.project_id != (select project_id from design_versions where id = new.design_version_id))
  or (new.review_policy_id is not null and new.project_id != (select project_id from review_policies where id = new.review_policy_id))
  or (new.review_scope_id is not null and new.project_id != (select project_id from review_scopes where id = new.review_scope_id))
begin
    select raise(abort, 'review plan project_id must match referenced rows');
end;

create trigger if not exists trg_review_plan_type_insert
before insert on review_plans
for each row
when (new.review_policy_id is not null and new.review_type != (select review_type from review_policies where id = new.review_policy_id))
  or (new.review_scope_id is not null and new.review_type != (select review_type from review_scopes where id = new.review_scope_id))
begin
    select raise(abort, 'review plan type must match policy and scope type');
end;

create trigger if not exists trg_review_plan_type_update
before update of review_type, review_policy_id, review_scope_id on review_plans
for each row
when (new.review_policy_id is not null and new.review_type != (select review_type from review_policies where id = new.review_policy_id))
  or (new.review_scope_id is not null and new.review_type != (select review_type from review_scopes where id = new.review_scope_id))
begin
    select raise(abort, 'review plan type must match policy and scope type');
end;

create trigger if not exists trg_review_run_project_insert
before insert on review_runs
for each row
when (new.review_scope_id is not null and new.project_id != (select project_id from review_scopes where id = new.review_scope_id))
  or (new.review_plan_id is not null and new.project_id != (select project_id from review_plans where id = new.review_plan_id))
begin
    select raise(abort, 'review run project_id must match referenced rows');
end;

create trigger if not exists trg_review_run_target_insert
before insert on review_runs
for each row
when (new.target_type = 'design_version' and not (
        new.design_version_id is not null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is null
        and new.repository_snapshot_id is null
        and new.project_id = (select project_id from design_versions where id = new.design_version_id)
    ))
  or (new.target_type = 'design_requirement' and not (
        new.design_version_id is null
        and new.design_requirement_id is not null
        and new.task_id is null
        and new.work_unit_id is null
        and new.repository_snapshot_id is null
        and new.project_id = (select project_id from design_requirements where id = new.design_requirement_id)
    ))
  or (new.target_type = 'task' and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is not null
        and new.work_unit_id is null
        and new.repository_snapshot_id is null
        and new.project_id = coalesce(
            (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
            (select id from projects order by id limit 1)
        )
    ))
  or (new.target_type = 'work_unit' and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is not null
        and new.repository_snapshot_id is null
        and new.project_id = (select project_id from work_units where id = new.work_unit_id)
    ))
  or (new.target_type = 'repository_snapshot' and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is null
        and new.repository_snapshot_id is not null
    ))
  or (new.target_type in ('file', 'symbol') and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is null
        and new.repository_snapshot_id is null
        and new.target_ref is not null
    ))
begin
    select raise(abort, 'review run target must match target_type and project_id');
end;

create trigger if not exists trg_review_run_project_update
before update of project_id, review_scope_id, review_plan_id on review_runs
for each row
when (new.review_scope_id is not null and new.project_id != (select project_id from review_scopes where id = new.review_scope_id))
  or (new.review_plan_id is not null and new.project_id != (select project_id from review_plans where id = new.review_plan_id))
begin
    select raise(abort, 'review run project_id must match referenced rows');
end;

create trigger if not exists trg_review_run_target_update
before update of project_id, target_type, design_version_id, design_requirement_id, task_id, work_unit_id, repository_snapshot_id, target_ref on review_runs
for each row
when (new.target_type = 'design_version' and not (
        new.design_version_id is not null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is null
        and new.repository_snapshot_id is null
        and new.project_id = (select project_id from design_versions where id = new.design_version_id)
    ))
  or (new.target_type = 'design_requirement' and not (
        new.design_version_id is null
        and new.design_requirement_id is not null
        and new.task_id is null
        and new.work_unit_id is null
        and new.repository_snapshot_id is null
        and new.project_id = (select project_id from design_requirements where id = new.design_requirement_id)
    ))
  or (new.target_type = 'task' and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is not null
        and new.work_unit_id is null
        and new.repository_snapshot_id is null
        and new.project_id = coalesce(
            (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
            (select id from projects order by id limit 1)
        )
    ))
  or (new.target_type = 'work_unit' and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is not null
        and new.repository_snapshot_id is null
        and new.project_id = (select project_id from work_units where id = new.work_unit_id)
    ))
  or (new.target_type = 'repository_snapshot' and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is null
        and new.repository_snapshot_id is not null
    ))
  or (new.target_type in ('file', 'symbol') and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is null
        and new.repository_snapshot_id is null
        and new.target_ref is not null
    ))
begin
    select raise(abort, 'review run target must match target_type and project_id');
end;

create trigger if not exists trg_review_run_type_purpose_insert
before insert on review_runs
for each row
when not (
    (new.run_type = 'fresh' and new.run_purpose = 'new_unbiased_review')
    or (new.run_type = 'resume' and new.run_purpose = 'finding_fix_verification')
    or (new.run_type = 'coverage' and new.run_purpose = 'coverage_audit')
)
begin
    select raise(abort, 'review run type must match purpose');
end;

create trigger if not exists trg_review_run_type_purpose_update
before update of run_type, run_purpose on review_runs
for each row
when not (
    (new.run_type = 'fresh' and new.run_purpose = 'new_unbiased_review')
    or (new.run_type = 'resume' and new.run_purpose = 'finding_fix_verification')
    or (new.run_type = 'coverage' and new.run_purpose = 'coverage_audit')
)
begin
    select raise(abort, 'review run type must match purpose');
end;

create trigger if not exists trg_review_run_result_insert
before insert on review_runs
for each row
when new.new_findings_count < 0
  or new.carried_findings_checked < 0
  or (new.clean_run = 1 and (new.status != 'completed' or new.new_findings_count != 0))
begin
    select raise(abort, 'review run result is inconsistent');
end;

create trigger if not exists trg_review_run_result_update
before update of new_findings_count, carried_findings_checked, clean_run, status on review_runs
for each row
when new.new_findings_count < 0
  or new.carried_findings_checked < 0
  or (new.clean_run = 1 and (new.status != 'completed' or new.new_findings_count != 0))
  or (new.clean_run = 1 and exists (
      select 1 from findings where review_run_id = new.id
  ))
begin
    select raise(abort, 'review run result is inconsistent');
end;

create trigger if not exists trg_review_plan_target_project_insert
before insert on review_plan_targets
for each row
when (new.target_type = 'design_version' and (select project_id from review_plans where id = new.review_plan_id) != (select project_id from design_versions where id = new.design_version_id))
  or (new.target_type = 'design_requirement' and (select project_id from review_plans where id = new.review_plan_id) != (select project_id from design_requirements where id = new.design_requirement_id))
  or (new.target_type = 'task' and (select project_id from review_plans where id = new.review_plan_id) != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
  or (new.target_type = 'work_unit' and (select project_id from review_plans where id = new.review_plan_id) != (select project_id from work_units where id = new.work_unit_id))
  or new.target_type = 'repository_snapshot'
begin
    select raise(abort, 'review plan target project_id must match review plan project_id');
end;

create trigger if not exists trg_review_plan_target_project_update
before update of review_plan_id, target_type, design_version_id, design_requirement_id, task_id, work_unit_id, repository_snapshot_id on review_plan_targets
for each row
when (new.target_type = 'design_version' and (select project_id from review_plans where id = new.review_plan_id) != (select project_id from design_versions where id = new.design_version_id))
  or (new.target_type = 'design_requirement' and (select project_id from review_plans where id = new.review_plan_id) != (select project_id from design_requirements where id = new.design_requirement_id))
  or (new.target_type = 'task' and (select project_id from review_plans where id = new.review_plan_id) != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
  or (new.target_type = 'work_unit' and (select project_id from review_plans where id = new.review_plan_id) != (select project_id from work_units where id = new.work_unit_id))
  or new.target_type = 'repository_snapshot'
begin
    select raise(abort, 'review plan target project_id must match review plan project_id');
end;

create trigger if not exists trg_finding_project_insert
before insert on findings
for each row
when new.project_id != (select project_id from review_runs where id = new.review_run_id)
  or (new.design_requirement_id is not null and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id))
  or (new.task_id is not null and new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
begin
    select raise(abort, 'finding project_id must match referenced rows');
end;

create trigger if not exists trg_finding_project_update
before update of project_id, review_run_id, design_requirement_id, task_id on findings
for each row
when new.project_id != (select project_id from review_runs where id = new.review_run_id)
  or (new.design_requirement_id is not null and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id))
  or (new.task_id is not null and new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
begin
    select raise(abort, 'finding project_id must match referenced rows');
end;

create trigger if not exists trg_finding_clean_run_insert
before insert on findings
for each row
when (select clean_run from review_runs where id = new.review_run_id) = 1
begin
    select raise(abort, 'cannot attach finding to clean review run');
end;

create trigger if not exists trg_finding_clean_run_update
before update of review_run_id on findings
for each row
when (select clean_run from review_runs where id = new.review_run_id) = 1
begin
    select raise(abort, 'cannot attach finding to clean review run');
end;

create trigger if not exists trg_closure_project_insert
before insert on closures
for each row
when new.project_id != (select project_id from findings where id = new.finding_id)
begin
    select raise(abort, 'closure project_id must match finding project_id');
end;

create trigger if not exists trg_closure_project_update
before update of project_id, finding_id on closures
for each row
when new.project_id != (select project_id from findings where id = new.finding_id)
begin
    select raise(abort, 'closure project_id must match finding project_id');
end;

create trigger if not exists trg_finding_verification_project_insert
before insert on finding_verifications
for each row
when new.project_id != (select project_id from review_runs where id = new.review_run_id)
  or new.project_id != (select project_id from findings where id = new.finding_id)
  or new.project_id != (select project_id from closures where id = new.closure_id)
  or new.finding_id != (select finding_id from closures where id = new.closure_id)
  or (select run_type from review_runs where id = new.review_run_id) != 'resume'
  or (select run_purpose from review_runs where id = new.review_run_id) != 'finding_fix_verification'
  or (select review_plan_id from review_runs where id = new.review_run_id) != (
      select source_run.review_plan_id
      from findings f
      join review_runs source_run on source_run.id = f.review_run_id
      where f.id = new.finding_id
  )
begin
    select raise(abort, 'finding verification project_id must match referenced rows');
end;

create trigger if not exists trg_finding_verification_project_update
before update of project_id, review_run_id, finding_id, closure_id on finding_verifications
for each row
when new.project_id != (select project_id from review_runs where id = new.review_run_id)
  or new.project_id != (select project_id from findings where id = new.finding_id)
  or new.project_id != (select project_id from closures where id = new.closure_id)
  or new.finding_id != (select finding_id from closures where id = new.closure_id)
  or (select run_type from review_runs where id = new.review_run_id) != 'resume'
  or (select run_purpose from review_runs where id = new.review_run_id) != 'finding_fix_verification'
  or (select review_plan_id from review_runs where id = new.review_run_id) != (
      select source_run.review_plan_id
      from findings f
      join review_runs source_run on source_run.id = f.review_run_id
      where f.id = new.finding_id
  )
begin
    select raise(abort, 'finding verification project_id must match referenced rows');
end;

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
    linked_review_finding_id integer references findings(id),
    linked_task_id integer references tasks(id),
    proposed_action text,
    status text not null default 'open' check (status in ('open', 'accepted', 'converted', 'converted_to_task', 'dismissed')),
    created_at text not null
);

create table if not exists kpt_item_conversions (
    id integer primary key,
    kpt_item_id integer not null references kpt_items(id) on delete cascade,
    target_type text not null check (target_type in ('task', 'command_profile', 'review_policy', 'design_version', 'decision', 'user_correction')),
    task_id integer references tasks(id),
    command_profile_id integer references command_profiles(id),
    review_policy_id integer references review_policies(id),
    design_version_id integer references design_versions(id),
    decision_id integer references decisions(id),
    user_correction_id integer references user_corrections(id),
    created_at text not null,
    check (
        (target_type = 'task' and task_id is not null and command_profile_id is null and review_policy_id is null and design_version_id is null and decision_id is null and user_correction_id is null)
        or (target_type = 'command_profile' and task_id is null and command_profile_id is not null and review_policy_id is null and design_version_id is null and decision_id is null and user_correction_id is null)
        or (target_type = 'review_policy' and task_id is null and command_profile_id is null and review_policy_id is not null and design_version_id is null and decision_id is null and user_correction_id is null)
        or (target_type = 'design_version' and task_id is null and command_profile_id is null and review_policy_id is null and design_version_id is not null and decision_id is null and user_correction_id is null)
        or (target_type = 'decision' and task_id is null and command_profile_id is null and review_policy_id is null and design_version_id is null and decision_id is not null and user_correction_id is null)
        or (target_type = 'user_correction' and task_id is null and command_profile_id is null and review_policy_id is null and design_version_id is null and decision_id is null and user_correction_id is not null)
    )
);

create trigger if not exists trg_kpt_item_conversion_project_insert
before insert on kpt_item_conversions
for each row
when (new.target_type = 'task' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
  or (new.target_type = 'command_profile' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from command_profiles where id = new.command_profile_id))
  or (new.target_type = 'review_policy' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from review_policies where id = new.review_policy_id))
  or (new.target_type = 'design_version' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from design_versions where id = new.design_version_id))
  or (new.target_type = 'decision' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from decisions where id = new.decision_id))
  or (new.target_type = 'user_correction' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from user_corrections where id = new.user_correction_id))
begin
    select raise(abort, 'kpt item conversion project_id must match target project_id');
end;

create trigger if not exists trg_kpt_item_conversion_project_update
before update of kpt_item_id, target_type, task_id, command_profile_id, review_policy_id, design_version_id, decision_id, user_correction_id on kpt_item_conversions
for each row
when (new.target_type = 'task' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
  or (new.target_type = 'command_profile' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from command_profiles where id = new.command_profile_id))
  or (new.target_type = 'review_policy' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from review_policies where id = new.review_policy_id))
  or (new.target_type = 'design_version' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from design_versions where id = new.design_version_id))
  or (new.target_type = 'decision' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from decisions where id = new.decision_id))
  or (new.target_type = 'user_correction' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from user_corrections where id = new.user_correction_id))
begin
    select raise(abort, 'kpt item conversion project_id must match target project_id');
end;
"#;
