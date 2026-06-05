use std::path::Path;

use anyhow::Result;
use rusqlite::{Connection, params};

use crate::db::{open_existing_project, project_id};

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
            authority_event_id: None,
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

pub fn applicable_rules(root: &Path, input: RuleQuery<'_>) -> Result<Vec<RuleRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut records = Vec::new();
    let scope_key = input.scope_key.unwrap_or("project");

    let mut stmt = conn.prepare(
        r#"
        select rb.id, rb.rule_source_type, rb.scope_type, rb.scope_key, rb.precedence,
               rb.authority_event_id, rb.user_correction_id, rb.command_profile_id, rb.work_unit_id
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
            authority_event_id: row.get(5)?,
            user_correction_id: row.get(6)?,
            command_profile_id: row.get(7)?,
            work_unit_id: row.get(8)?,
        })
    })?;
    for row in rows {
        records.push(row?);
    }

    Ok(records)
}

pub(crate) fn insert_rule_binding(conn: &Connection, input: RuleBindingInput<'_>) -> Result<i64> {
    conn.execute(
        r#"
        insert into rule_bindings(
            project_id, rule_source_type, authority_event_id, user_correction_id,
            command_profile_id, work_unit_id, scope_type, scope_key, precedence,
            status, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'active', current_timestamp)
        "#,
        params![
            input.project_id,
            input.rule_source_type,
            input.authority_event_id,
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

pub(crate) fn scope_type_for(scope: &str) -> &'static str {
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

pub(crate) struct RuleBindingInput<'a> {
    pub(crate) project_id: i64,
    pub(crate) rule_source_type: &'a str,
    pub(crate) authority_event_id: Option<i64>,
    pub(crate) user_correction_id: Option<i64>,
    pub(crate) command_profile_id: Option<i64>,
    pub(crate) work_unit_id: Option<i64>,
    pub(crate) scope_type: &'a str,
    pub(crate) scope_key: Option<&'a str>,
    pub(crate) precedence: i64,
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
    pub authority_event_id: Option<i64>,
    pub user_correction_id: Option<i64>,
    pub command_profile_id: Option<i64>,
    pub work_unit_id: Option<i64>,
}
