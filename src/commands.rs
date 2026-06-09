use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{active_activation, open_existing_project, project_id};
use crate::rules::{RuleBindingInput, insert_rule_binding, scope_type_for};

pub fn add_fixed_command(root: &Path, input: NewCommandProfile<'_>) -> Result<CommandOutcome> {
    add_command_profile_with_status(root, input, "fixed", "stable", "user")
}

pub fn add_preferred_command(root: &Path, input: NewCommandProfile<'_>) -> Result<CommandOutcome> {
    add_command_profile_with_status(root, input, "preferred", "stable", "user")
}

pub fn deprecate_command_profile(root: &Path, name: &str, reason: &str) -> Result<CommandOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;

    let changed = tx.execute(
        r#"
        update command_profiles
        set status = 'deprecated', expected_result = ?1, updated_at = current_timestamp
        where project_id = ?2 and name = ?3
        "#,
        params![reason, project_id, name],
    )?;
    if changed == 0 {
        bail!("command profile not found");
    }
    let command_profile_id = resolve_command_profile(&tx, project_id, name)?;
    tx.execute(
        "update rule_bindings set status = 'inactive' where command_profile_id = ?1",
        params![command_profile_id],
    )?;
    tx.commit()?;

    Ok(CommandOutcome { command_profile_id })
}

fn add_command_profile_with_status(
    root: &Path,
    input: NewCommandProfile<'_>,
    status: &str,
    stability: &str,
    source: &str,
) -> Result<CommandOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;

    tx.execute(
        r#"
        insert into command_profiles(
            project_id, name, command, command_type, scope, status, stability,
            timeout, expected_result, source, created_at, updated_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                current_timestamp, current_timestamp)
        "#,
        params![
            project_id,
            input.name,
            input.command,
            input.command_type,
            input.scope,
            status,
            stability,
            input.timeout,
            input.expected_result,
            source,
        ],
    )?;
    let command_profile_id = tx.last_insert_rowid();
    insert_rule_binding(
        &tx,
        RuleBindingInput {
            project_id,
            rule_source_type: "command_profile",
            authority_event_id: None,
            user_correction_id: None,
            command_profile_id: Some(command_profile_id),
            work_unit_id: None,
            scope_type: scope_type_for(input.scope),
            scope_key: Some(input.scope),
            precedence: if status == "fixed" { 70 } else { 55 },
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

pub fn add_command_usage(root: &Path, input: NewCommandUsage<'_>) -> Result<CommandUsageOutcome> {
    insert_command_usage(
        root,
        CommandUsageInput {
            profile: input.profile,
            command: input.command,
            result: input.result,
            log_path: input.log_path,
            work_unit_id: input.work_unit_id,
            repository_snapshot_id: None,
        },
    )
}

pub fn add_command_usage_with_repository_snapshot(
    root: &Path,
    input: NewCommandUsageWithRepositorySnapshot<'_>,
) -> Result<CommandUsageOutcome> {
    insert_command_usage(
        root,
        CommandUsageInput {
            profile: input.profile,
            command: input.command,
            result: input.result,
            log_path: input.log_path,
            work_unit_id: input.work_unit_id,
            repository_snapshot_id: input.repository_snapshot_id,
        },
    )
}

fn insert_command_usage(root: &Path, input: CommandUsageInput<'_>) -> Result<CommandUsageOutcome> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let active = active_activation(&conn)?;
    let command_profile_id = match input.profile {
        Some(profile) => Some(resolve_command_profile(&conn, project_id, profile)?),
        None => None,
    };
    let command = match (input.command, command_profile_id) {
        (Some(command), _) => command.to_string(),
        (None, Some(command_profile_id)) => command_for_profile(&conn, command_profile_id)?,
        (None, None) => bail!("--command is required when --profile is omitted"),
    };
    let work_unit_id = input
        .work_unit_id
        .or_else(|| active.as_ref().map(|active| active.work_unit_id));
    let activation_id = active.as_ref().map(|active| active.activation_id);
    if let Some(work_unit_id) = work_unit_id {
        ensure_work_unit_project(&conn, work_unit_id, project_id)?;
    }
    if let (Some(command_profile_id), Some(work_unit_id)) = (command_profile_id, work_unit_id) {
        ensure_profile_matches_work_unit(&conn, command_profile_id, work_unit_id)?;
    }

    conn.execute(
        r#"
        insert into command_usages(
            project_id, command_profile_id, work_unit_id, work_unit_activation_id,
            command, result, log_path, repository_snapshot_id, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, current_timestamp)
        "#,
        params![
            project_id,
            command_profile_id,
            work_unit_id,
            activation_id,
            command,
            input.result,
            input.log_path,
            input.repository_snapshot_id,
        ],
    )?;

    Ok(CommandUsageOutcome {
        command_usage_id: conn.last_insert_rowid(),
        command_profile_id,
        work_unit_id,
    })
}

pub fn list_command_usages(
    root: &Path,
    input: CommandUsageListQuery<'_>,
) -> Result<Vec<CommandUsageRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let command_profile_id = match input.profile {
        Some(profile) => Some(resolve_command_profile(&conn, project_id, profile)?),
        None => None,
    };
    let mut records = Vec::new();

    match (command_profile_id, input.work_unit_id) {
        (Some(command_profile_id), Some(work_unit_id)) => {
            let mut stmt = conn.prepare(COMMAND_USAGE_SELECT_FILTERED)?;
            let rows = stmt.query_map(
                params![project_id, command_profile_id, work_unit_id],
                command_usage_record,
            )?;
            for row in rows {
                records.push(row?);
            }
        }
        (Some(command_profile_id), None) => {
            let mut stmt = conn.prepare(COMMAND_USAGE_SELECT_BY_PROFILE)?;
            let rows = stmt.query_map(
                params![project_id, command_profile_id],
                command_usage_record,
            )?;
            for row in rows {
                records.push(row?);
            }
        }
        (None, Some(work_unit_id)) => {
            let mut stmt = conn.prepare(COMMAND_USAGE_SELECT_BY_WORK_UNIT)?;
            let rows = stmt.query_map(params![project_id, work_unit_id], command_usage_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        (None, None) => {
            let mut stmt = conn.prepare(COMMAND_USAGE_SELECT_ALL)?;
            let rows = stmt.query_map(params![project_id], command_usage_record)?;
            for row in rows {
                records.push(row?);
            }
        }
    }

    Ok(records)
}

pub fn add_command_deviation(
    root: &Path,
    input: NewCommandDeviation<'_>,
) -> Result<CommandDeviationOutcome> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let command_profile_id = resolve_command_profile(&conn, project_id, input.profile)?;
    ensure_fixed_profile(&conn, command_profile_id)?;
    if let Some(command_usage_id) = input.command_usage_id {
        ensure_usage_matches_profile(&conn, command_usage_id, command_profile_id)?;
    }
    let active = active_activation(&conn)?;
    let work_unit_id = active.as_ref().map(|active| active.work_unit_id);

    conn.execute(
        r#"
        insert into command_deviations(
            command_profile_id, command_usage_id, work_unit_id, reason, status, created_at
        )
        values (?1, ?2, ?3, ?4, 'proposed', current_timestamp)
        "#,
        params![
            command_profile_id,
            input.command_usage_id,
            work_unit_id,
            input.reason,
        ],
    )?;

    Ok(CommandDeviationOutcome {
        command_deviation_id: conn.last_insert_rowid(),
        command_profile_id,
        work_unit_id,
    })
}

fn resolve_command_profile(
    conn: &rusqlite::Connection,
    project_id: i64,
    profile: &str,
) -> Result<i64> {
    if let Ok(id) = profile.parse::<i64>() {
        let found = conn
            .query_row(
                "select id from command_profiles where project_id = ?1 and id = ?2",
                params![project_id, id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .context("command profile not found")?;
        return Ok(found);
    }

    conn.query_row(
        "select id from command_profiles where project_id = ?1 and name = ?2",
        params![project_id, profile],
        |row| row.get::<_, i64>(0),
    )
    .optional()?
    .context("command profile not found")
}

fn command_for_profile(conn: &rusqlite::Connection, command_profile_id: i64) -> Result<String> {
    conn.query_row(
        "select command from command_profiles where id = ?1",
        params![command_profile_id],
        |row| row.get::<_, String>(0),
    )
    .optional()?
    .context("command profile not found")
}

fn ensure_fixed_profile(conn: &rusqlite::Connection, command_profile_id: i64) -> Result<()> {
    let status = conn
        .query_row(
            "select status from command_profiles where id = ?1",
            params![command_profile_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .context("command profile not found")?;
    if status != "fixed" {
        bail!("command deviation requires a fixed command profile");
    }
    Ok(())
}

fn ensure_work_unit_project(
    conn: &rusqlite::Connection,
    work_unit_id: i64,
    project_id: i64,
) -> Result<()> {
    conn.query_row(
        "select 1 from work_units where id = ?1 and project_id = ?2",
        params![work_unit_id, project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("work unit not found")
}

fn ensure_profile_matches_work_unit(
    conn: &rusqlite::Connection,
    command_profile_id: i64,
    work_unit_id: i64,
) -> Result<()> {
    conn.query_row(
        r#"
        select 1
        from command_profiles cp
        join work_units w on w.id = ?2
        where cp.id = ?1 and cp.project_id = w.project_id
        "#,
        params![command_profile_id, work_unit_id],
        |_| Ok(()),
    )
    .optional()?
    .context("command profile project must match work unit project")
}

fn ensure_usage_matches_profile(
    conn: &rusqlite::Connection,
    command_usage_id: i64,
    command_profile_id: i64,
) -> Result<()> {
    let usage_profile_id = conn
        .query_row(
            "select command_profile_id from command_usages where id = ?1",
            params![command_usage_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()?
        .context("command usage not found")?;
    if usage_profile_id != Some(command_profile_id) {
        bail!("command usage does not belong to the fixed command profile");
    }
    Ok(())
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

fn command_usage_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<CommandUsageRecord> {
    Ok(CommandUsageRecord {
        id: row.get(0)?,
        command_profile_id: row.get(1)?,
        work_unit_id: row.get(2)?,
        command: row.get(3)?,
        result: row.get(4)?,
        log_path: row.get(5)?,
        created_at: row.get(6)?,
    })
}

const COMMAND_USAGE_SELECT_ALL: &str = r#"
select id, command_profile_id, work_unit_id, command, result, log_path, created_at
from command_usages
where project_id = ?1
order by id
"#;

const COMMAND_USAGE_SELECT_BY_PROFILE: &str = r#"
select id, command_profile_id, work_unit_id, command, result, log_path, created_at
from command_usages
where project_id = ?1 and command_profile_id = ?2
order by id
"#;

const COMMAND_USAGE_SELECT_BY_WORK_UNIT: &str = r#"
select id, command_profile_id, work_unit_id, command, result, log_path, created_at
from command_usages
where project_id = ?1 and work_unit_id = ?2
order by id
"#;

const COMMAND_USAGE_SELECT_FILTERED: &str = r#"
select id, command_profile_id, work_unit_id, command, result, log_path, created_at
from command_usages
where project_id = ?1 and command_profile_id = ?2 and work_unit_id = ?3
order by id
"#;

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

pub struct NewCommandUsage<'a> {
    pub profile: Option<&'a str>,
    pub command: Option<&'a str>,
    pub result: &'a str,
    pub log_path: Option<&'a str>,
    pub work_unit_id: Option<i64>,
}

pub struct NewCommandUsageWithRepositorySnapshot<'a> {
    pub profile: Option<&'a str>,
    pub command: Option<&'a str>,
    pub result: &'a str,
    pub log_path: Option<&'a str>,
    pub work_unit_id: Option<i64>,
    pub repository_snapshot_id: Option<i64>,
}

struct CommandUsageInput<'a> {
    profile: Option<&'a str>,
    command: Option<&'a str>,
    result: &'a str,
    log_path: Option<&'a str>,
    work_unit_id: Option<i64>,
    repository_snapshot_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CommandUsageOutcome {
    pub command_usage_id: i64,
    pub command_profile_id: Option<i64>,
    pub work_unit_id: Option<i64>,
}

pub struct CommandUsageListQuery<'a> {
    pub profile: Option<&'a str>,
    pub work_unit_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CommandUsageRecord {
    pub id: i64,
    pub command_profile_id: Option<i64>,
    pub work_unit_id: Option<i64>,
    pub command: String,
    pub result: String,
    pub log_path: Option<String>,
    pub created_at: String,
}

pub struct NewCommandDeviation<'a> {
    pub profile: &'a str,
    pub command_usage_id: Option<i64>,
    pub reason: &'a str,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CommandDeviationOutcome {
    pub command_deviation_id: i64,
    pub command_profile_id: i64,
    pub work_unit_id: Option<i64>,
}
