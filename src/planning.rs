use std::path::Path;

use anyhow::{Result, bail};
use rusqlite::params;

use crate::db::{active_activation, open_existing_project, project_id};

pub fn add_task(root: &Path, input: NewTask<'_>) -> Result<TaskOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let work_unit_id = match input.work_unit_id {
        Some(work_unit_id) => Some(work_unit_id),
        None => active_activation(&tx)?.map(|active| active.work_unit_id),
    };

    tx.execute(
        r#"
        insert into tasks(
            work_unit_id, title, priority, status, source, details, completion_condition
        )
        values (?1, ?2, ?3, 'open', ?4, ?5, ?6)
        "#,
        params![
            work_unit_id,
            input.title,
            input.priority,
            input.source,
            input.details,
            input.completion_condition,
        ],
    )?;
    let task_id = tx.last_insert_rowid();
    tx.commit()?;

    Ok(TaskOutcome {
        task_id,
        work_unit_id,
    })
}

pub fn list_tasks(root: &Path, input: TaskListQuery<'_>) -> Result<Vec<TaskRecord>> {
    let conn = open_existing_project(root)?;
    let mut records = Vec::new();

    match (input.status, input.work_unit_id) {
        (Some(status), Some(work_unit_id)) => {
            let mut stmt = conn.prepare(
                r#"
                select id, work_unit_id, title, priority, status, source, closed_by_commit
                from tasks
                where status = ?1 and work_unit_id = ?2
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![status, work_unit_id], task_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        (Some(status), None) => {
            let mut stmt = conn.prepare(
                r#"
                select id, work_unit_id, title, priority, status, source, closed_by_commit
                from tasks
                where status = ?1
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![status], task_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        (None, Some(work_unit_id)) => {
            let mut stmt = conn.prepare(
                r#"
                select id, work_unit_id, title, priority, status, source, closed_by_commit
                from tasks
                where work_unit_id = ?1
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![work_unit_id], task_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        (None, None) => {
            let mut stmt = conn.prepare(
                r#"
                select id, work_unit_id, title, priority, status, source, closed_by_commit
                from tasks
                order by id
                "#,
            )?;
            let rows = stmt.query_map([], task_record)?;
            for row in rows {
                records.push(row?);
            }
        }
    }

    Ok(records)
}

pub fn close_task(root: &Path, task_id: i64, commit: Option<&str>) -> Result<TaskCloseOutcome> {
    let conn = open_existing_project(root)?;
    let changed = conn.execute(
        r#"
        update tasks
        set status = 'closed', closed_by_commit = ?1
        where id = ?2 and status != 'closed'
        "#,
        params![commit, task_id],
    )?;
    if changed == 0 {
        bail!("task not found or already closed");
    }

    Ok(TaskCloseOutcome { task_id })
}

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
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut records = Vec::new();

    match query {
        Some(query) => {
            let pattern = format!("%{query}%");
            let mut stmt = conn.prepare(
                r#"
                select id, decision_key, topic, decision, rationale, status
                from decisions
                where project_id = ?1
                  and status = 'accepted'
                  and (topic like ?2 or decision like ?2 or coalesce(decision_key, '') like ?2)
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![project_id, pattern], decision_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        None => {
            let mut stmt = conn.prepare(
                r#"
                select id, decision_key, topic, decision, rationale, status
                from decisions
                where project_id = ?1 and status = 'accepted'
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![project_id], decision_record)?;
            for row in rows {
                records.push(row?);
            }
        }
    }

    Ok(records)
}

fn task_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskRecord> {
    Ok(TaskRecord {
        id: row.get(0)?,
        work_unit_id: row.get(1)?,
        title: row.get(2)?,
        priority: row.get(3)?,
        status: row.get(4)?,
        source: row.get(5)?,
        closed_by_commit: row.get(6)?,
    })
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

pub struct NewTask<'a> {
    pub title: &'a str,
    pub priority: &'a str,
    pub source: &'a str,
    pub work_unit_id: Option<i64>,
    pub details: Option<&'a str>,
    pub completion_condition: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskOutcome {
    pub task_id: i64,
    pub work_unit_id: Option<i64>,
}

pub struct TaskListQuery<'a> {
    pub status: Option<&'a str>,
    pub work_unit_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskRecord {
    pub id: i64,
    pub work_unit_id: Option<i64>,
    pub title: String,
    pub priority: String,
    pub status: String,
    pub source: String,
    pub closed_by_commit: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskCloseOutcome {
    pub task_id: i64,
}

pub struct NewDecision<'a> {
    pub decision_key: Option<&'a str>,
    pub topic: &'a str,
    pub decision: &'a str,
    pub rationale: Option<&'a str>,
    pub compatibility_impact: Option<&'a str>,
    pub authority_refs: Option<&'a str>,
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
