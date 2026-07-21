use std::path::Path;

use anyhow::Result;
use rusqlite::params;

use crate::db::{open_existing_project, project_id};

pub fn add_decision(root: &Path, input: NewDecision<'_>) -> Result<DecisionOutcome> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    conn.execute(
        r#"
        insert into decisions(
            project_id, decision_key, topic, decision, rationale,
            compatibility_impact, status, authority_refs, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, 'accepted', ?7, current_timestamp)
        "#,
        params![
            project_id,
            input.decision_key,
            input.topic,
            input.decision,
            input.rationale,
            input.compatibility_impact,
            input.authority_refs,
        ],
    )?;

    Ok(DecisionOutcome {
        decision_id: conn.last_insert_rowid(),
    })
}

pub fn list_decisions(root: &Path, query: Option<&str>) -> Result<Vec<DecisionRecord>> {
    list_decisions_filtered(root, DecisionListFilter { query, topic: None })
}

pub fn list_decisions_filtered(
    root: &Path,
    filter: DecisionListFilter<'_>,
) -> Result<Vec<DecisionRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let pattern = filter.query.map(|query| format!("%{query}%"));
    let mut stmt = conn.prepare(
        r#"
        select id, decision_key, topic, decision, rationale, status
        from decisions
        where project_id = ?1
          and status = 'accepted'
          and (?2 is null or topic = ?2)
          and (
            ?3 is null
            or topic like ?3
            or decision like ?3
            or coalesce(decision_key, '') like ?3
          )
        order by id
        "#,
    )?;
    let rows = stmt.query_map(params![project_id, filter.topic, pattern], decision_record)?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

fn decision_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<DecisionRecord> {
    Ok(DecisionRecord {
        id: row.get(0)?,
        decision_key: row.get(1)?,
        topic: row.get(2)?,
        decision: row.get(3)?,
        rationale: row.get(4)?,
        status: row.get(5)?,
    })
}

pub struct NewDecision<'a> {
    pub decision_key: Option<&'a str>,
    pub topic: &'a str,
    pub decision: &'a str,
    pub rationale: Option<&'a str>,
    pub compatibility_impact: Option<&'a str>,
    pub authority_refs: Option<&'a str>,
}

pub struct DecisionListFilter<'a> {
    pub query: Option<&'a str>,
    pub topic: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DecisionOutcome {
    pub decision_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DecisionRecord {
    pub id: i64,
    pub decision_key: Option<String>,
    pub topic: String,
    pub decision: String,
    pub rationale: Option<String>,
    pub status: String,
}
