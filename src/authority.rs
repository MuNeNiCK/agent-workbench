use std::path::Path;

use anyhow::Result;
use rusqlite::{OptionalExtension, params};

use crate::db::{open_existing_project, project_id};
use crate::rules::{RuleBindingInput, insert_rule_binding, scope_type_for};

mod decision_projection_support;
mod owner_decisions;

pub use owner_decisions::{
    DecisionOutcome as OwnerDecisionOutcome, OwnerDecisionRequest, record_owner_decision,
};
pub(crate) use owner_decisions::{current_owner_decision, record_owner_decision_in};

pub fn add_authority_event(
    root: &Path,
    input: NewAuthorityEvent<'_>,
) -> Result<AuthorityEventOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let authority_type = authority_type_for_event(input.event_type);
    let path_or_label = input.source.unwrap_or(input.event_type);
    let authority_id = ensure_authority(
        &tx,
        project_id,
        path_or_label,
        authority_type,
        input.scope,
        input.precedence,
        input.summary,
    )?;

    tx.execute(
        r#"
        insert into authority_events(
            project_id, authority_id, event_type, source, text_or_summary, scope, precedence,
            status, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'active', current_timestamp)
        "#,
        params![
            project_id,
            authority_id,
            input.event_type,
            input.source,
            input.summary,
            input.scope,
            input.precedence,
        ],
    )?;
    let authority_event_id = tx.last_insert_rowid();
    insert_rule_binding(
        &tx,
        RuleBindingInput {
            project_id,
            rule_source_type: "authority_event",
            authority_event_id: Some(authority_event_id),
            user_correction_id: None,
            command_profile_id: None,
            review_policy_id: None,
            review_plan_id: None,
            work_unit_id: None,
            validation_gate_id: None,
            acceptance_record_id: None,
            scope_type: scope_type_for(input.scope.unwrap_or("project")),
            scope_key: input.scope.or(Some("project")),
            precedence: input.precedence,
        },
    )?;
    tx.commit()?;

    Ok(AuthorityEventOutcome {
        authority_id,
        authority_event_id,
    })
}

pub fn list_authorities(root: &Path, scope: Option<&str>) -> Result<Vec<AuthorityRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut records = Vec::new();
    match scope {
        Some(scope) => {
            let mut stmt = conn.prepare(
                r#"
                select id, path_or_label, authority_type, scope, precedence, summary, status
                from authorities
                where project_id = ?1 and status = 'active' and scope = ?2
                order by precedence desc, id
                "#,
            )?;
            let rows = stmt.query_map(params![project_id, scope], authority_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        None => {
            let mut stmt = conn.prepare(
                r#"
                select id, path_or_label, authority_type, scope, precedence, summary, status
                from authorities
                where project_id = ?1 and status = 'active'
                order by precedence desc, id
                "#,
            )?;
            let rows = stmt.query_map(params![project_id], authority_record)?;
            for row in rows {
                records.push(row?);
            }
        }
    }
    Ok(records)
}

pub fn list_authority_events(
    root: &Path,
    scope: Option<&str>,
) -> Result<Vec<AuthorityEventRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut records = Vec::new();

    match scope {
        Some(scope) => {
            let mut stmt = conn.prepare(
                r#"
                select id, authority_id, event_type, source, text_or_summary, scope, precedence, status
                from authority_events
                where project_id = ?1 and status = 'active' and scope = ?2
                order by precedence desc, id
                "#,
            )?;
            let rows = stmt.query_map(params![project_id, scope], authority_event_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        None => {
            let mut stmt = conn.prepare(
                r#"
                select id, authority_id, event_type, source, text_or_summary, scope, precedence, status
                from authority_events
                where project_id = ?1 and status = 'active'
                order by precedence desc, id
                "#,
            )?;
            let rows = stmt.query_map(params![project_id], authority_event_record)?;
            for row in rows {
                records.push(row?);
            }
        }
    }

    Ok(records)
}

fn authority_event_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuthorityEventRecord> {
    Ok(AuthorityEventRecord {
        id: row.get(0)?,
        authority_id: row.get(1)?,
        event_type: row.get(2)?,
        source: row.get(3)?,
        summary: row.get(4)?,
        scope: row.get(5)?,
        precedence: row.get(6)?,
        status: row.get(7)?,
    })
}

fn authority_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuthorityRecord> {
    Ok(AuthorityRecord {
        id: row.get(0)?,
        path_or_label: row.get(1)?,
        authority_type: row.get(2)?,
        scope: row.get(3)?,
        precedence: row.get(4)?,
        summary: row.get(5)?,
        status: row.get(6)?,
    })
}

fn ensure_authority(
    conn: &rusqlite::Transaction<'_>,
    project_id: i64,
    path_or_label: &str,
    authority_type: &str,
    scope: Option<&str>,
    precedence: i64,
    summary: &str,
) -> Result<i64> {
    let existing_id = conn
        .query_row(
            r#"
            select id
            from authorities
            where project_id = ?1
              and path_or_label = ?2
              and authority_type = ?3
              and coalesce(scope, 'project') = coalesce(?4, 'project')
            "#,
            params![project_id, path_or_label, authority_type, scope],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    if let Some(id) = existing_id {
        conn.execute(
            r#"
            update authorities
            set precedence = ?1,
                summary = ?2,
                status = 'active',
                updated_at = current_timestamp
            where id = ?3
            "#,
            params![precedence, summary, id],
        )?;
        return Ok(id);
    }
    conn.execute(
        r#"
        insert into authorities(
            project_id, path_or_label, authority_type, scope, precedence,
            summary, status, created_at, updated_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, 'active', current_timestamp, current_timestamp)
        "#,
        params![
            project_id,
            path_or_label,
            authority_type,
            scope,
            precedence,
            summary
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn authority_type_for_event(event_type: &str) -> &'static str {
    match event_type {
        "user_instruction" => "user",
        "design_doc" => "design",
        "validation_result" | "review_result" => "validation",
        _ => "policy",
    }
}

pub struct NewAuthorityEvent<'a> {
    pub event_type: &'a str,
    pub source: Option<&'a str>,
    pub summary: &'a str,
    pub scope: Option<&'a str>,
    pub precedence: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct AuthorityEventOutcome {
    pub authority_id: i64,
    pub authority_event_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct AuthorityRecord {
    pub id: i64,
    pub path_or_label: String,
    pub authority_type: String,
    pub scope: Option<String>,
    pub precedence: i64,
    pub summary: String,
    pub status: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct AuthorityEventRecord {
    pub id: i64,
    pub authority_id: Option<i64>,
    pub event_type: String,
    pub source: Option<String>,
    pub summary: String,
    pub scope: Option<String>,
    pub precedence: i64,
    pub status: String,
}
