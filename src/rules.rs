use std::path::Path;

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use crate::db::{active_activation, open_existing_project, project_id};

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
            review_policy_id: None,
            review_plan_id: None,
            work_unit_id: None,
            validation_gate_id: None,
            acceptance_record_id: None,
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

pub fn applicable_rules(root: &Path, input: RuleQuery<'_>) -> Result<Vec<RuleRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut records = Vec::new();
    let active_work_unit_id = if matches!(input.scope_key, Some("current")) {
        active_activation(&conn)?.map(|active| active.work_unit_id)
    } else {
        None
    };
    let work_unit_id = input.work_unit_id.or(active_work_unit_id);
    let responsibility = match (input.scope_key, work_unit_id) {
        (Some("current"), Some(work_unit_id)) => conn
            .query_row(
                "select responsibility from work_units where id = ?1 and project_id = ?2",
                params![work_unit_id, project_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten(),
        _ => None,
    };
    let resolved_scope_key = match (input.scope_key, work_unit_id) {
        (Some("current"), Some(work_unit_id)) => Some(work_unit_id.to_string()),
        (Some(scope_key), _) => Some(scope_key.to_string()),
        (None, _) => Some("project".to_string()),
    };
    let scope_key = resolved_scope_key.as_deref().unwrap_or("project");

    let mut stmt = conn.prepare(
        r#"
        select rb.id, rb.rule_source_type, rb.scope_type, rb.scope_key, rb.precedence,
               rb.authority_event_id, rb.user_correction_id, rb.command_profile_id,
               rb.review_policy_id, rb.review_plan_id, rb.work_unit_id,
               rb.validation_gate_id, rb.acceptance_record_id
        from rule_bindings rb
        where rb.project_id = ?1
          and rb.status = 'active'
          and (
            rb.scope_type = 'project'
            or rb.scope_key = ?2
            or (?3 is not null and rb.work_unit_id = ?3)
            or (?4 is not null and rb.scope_key = ?4)
          )
        order by rb.precedence desc, rb.id asc
        "#,
    )?;
    let rows = stmt.query_map(
        params![
            project_id,
            scope_key,
            work_unit_id,
            responsibility.as_deref()
        ],
        |row| {
            Ok(RuleRecord {
                id: row.get(0)?,
                rule_source_type: row.get(1)?,
                scope_type: row.get(2)?,
                scope_key: row.get(3)?,
                precedence: row.get(4)?,
                authority_event_id: row.get(5)?,
                user_correction_id: row.get(6)?,
                command_profile_id: row.get(7)?,
                review_policy_id: row.get(8)?,
                review_plan_id: row.get(9)?,
                work_unit_id: row.get(10)?,
                validation_gate_id: row.get(11)?,
                acceptance_record_id: row.get(12)?,
                shadowed_by_rule_id: None,
            })
        },
    )?;
    for row in rows {
        records.push(row?);
    }
    annotate_shadowed_rules(&mut records);

    Ok(records)
}

fn annotate_shadowed_rules(records: &mut [RuleRecord]) {
    for index in 0..records.len() {
        let winner_id = records
            .iter()
            .find(|candidate| {
                candidate.id != records[index].id
                    && candidate.rule_source_type == records[index].rule_source_type
                    && rule_identity(candidate) == rule_identity(&records[index])
                    && candidate.scope_type == records[index].scope_type
                    && candidate.scope_key == records[index].scope_key
                    && candidate.precedence > records[index].precedence
            })
            .map(|winner| winner.id);
        if let Some(winner_id) = winner_id {
            records[index].shadowed_by_rule_id = Some(winner_id);
        }
    }
}

fn rule_identity(record: &RuleRecord) -> Option<i64> {
    record
        .authority_event_id
        .or(record.user_correction_id)
        .or(record.command_profile_id)
        .or(record.review_policy_id)
        .or(record.review_plan_id)
        .or(record.work_unit_id)
        .or(record.validation_gate_id)
        .or(record.acceptance_record_id)
}

pub(crate) fn insert_rule_binding(conn: &Connection, input: RuleBindingInput<'_>) -> Result<i64> {
    conn.execute(
        r#"
        insert into rule_bindings(
            project_id, rule_source_type, authority_event_id, user_correction_id,
            command_profile_id, review_policy_id, review_plan_id, work_unit_id,
            validation_gate_id, acceptance_record_id, scope_type, scope_key,
            precedence, status, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 'active', current_timestamp)
        "#,
        params![
            input.project_id,
            input.rule_source_type,
            input.authority_event_id,
            input.user_correction_id,
            input.command_profile_id,
            input.review_policy_id,
            input.review_plan_id,
            input.work_unit_id,
            input.validation_gate_id,
            input.acceptance_record_id,
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

pub(crate) struct RuleBindingInput<'a> {
    pub(crate) project_id: i64,
    pub(crate) rule_source_type: &'a str,
    pub(crate) authority_event_id: Option<i64>,
    pub(crate) user_correction_id: Option<i64>,
    pub(crate) command_profile_id: Option<i64>,
    pub(crate) review_policy_id: Option<i64>,
    pub(crate) review_plan_id: Option<i64>,
    pub(crate) work_unit_id: Option<i64>,
    pub(crate) validation_gate_id: Option<i64>,
    pub(crate) acceptance_record_id: Option<i64>,
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
    pub review_policy_id: Option<i64>,
    pub review_plan_id: Option<i64>,
    pub work_unit_id: Option<i64>,
    pub validation_gate_id: Option<i64>,
    pub acceptance_record_id: Option<i64>,
    pub shadowed_by_rule_id: Option<i64>,
}
