use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{active_activation, open_existing_project, project_id};
use crate::rules::{RuleBindingInput, insert_rule_binding};

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
            authority_event_id: None,
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

pub fn add_command_usage(root: &Path, input: NewCommandUsage<'_>) -> Result<CommandUsageOutcome> {
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

    conn.execute(
        r#"
        insert into command_usages(
            command_profile_id, work_unit_id, work_unit_activation_id,
            command, result, log_path, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, current_timestamp)
        "#,
        params![
            command_profile_id,
            work_unit_id,
            activation_id,
            command,
            input.result,
            input.log_path,
        ],
    )?;

    Ok(CommandUsageOutcome {
        command_usage_id: conn.last_insert_rowid(),
        command_profile_id,
        work_unit_id,
    })
}

pub fn add_command_deviation(
    root: &Path,
    input: NewCommandDeviation<'_>,
) -> Result<CommandDeviationOutcome> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let command_profile_id = resolve_command_profile(&conn, project_id, input.profile)?;
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

#[derive(Debug, PartialEq, Eq)]
pub struct CommandUsageOutcome {
    pub command_usage_id: i64,
    pub command_profile_id: Option<i64>,
    pub work_unit_id: Option<i64>,
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
