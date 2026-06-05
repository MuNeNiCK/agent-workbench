use std::path::Path;

use anyhow::Result;
use rusqlite::params;

use crate::db::{open_existing_project, project_id};
use crate::rules::{RuleBindingInput, insert_rule_binding, scope_type_for};

pub fn add_authority_event(
    root: &Path,
    input: NewAuthorityEvent<'_>,
) -> Result<AuthorityEventOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;

    tx.execute(
        r#"
        insert into authority_events(
            project_id, event_type, source, text_or_summary, scope, precedence,
            status, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, 'active', current_timestamp)
        "#,
        params![
            project_id,
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
            work_unit_id: None,
            scope_type: scope_type_for(input.scope.unwrap_or("project")),
            scope_key: input.scope.or(Some("project")),
            precedence: input.precedence,
        },
    )?;
    tx.commit()?;

    Ok(AuthorityEventOutcome { authority_event_id })
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
                select id, event_type, source, text_or_summary, scope, precedence, status
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
                select id, event_type, source, text_or_summary, scope, precedence, status
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
        event_type: row.get(1)?,
        source: row.get(2)?,
        summary: row.get(3)?,
        scope: row.get(4)?,
        precedence: row.get(5)?,
        status: row.get(6)?,
    })
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
    pub authority_event_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct AuthorityEventRecord {
    pub id: i64,
    pub event_type: String,
    pub source: Option<String>,
    pub summary: String,
    pub scope: Option<String>,
    pub precedence: i64,
    pub status: String,
}
