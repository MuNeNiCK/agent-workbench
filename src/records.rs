use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

use crate::db::{active_activation, open_existing_project};

pub fn create_work_record(root: &Path, input: NewWorkRecord<'_>) -> Result<WorkRecordOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let work_unit_id = match input.work_unit_id {
        Some(work_unit_id) => Some(work_unit_id),
        None => active_activation(&tx)?.map(|active| active.work_unit_id),
    };

    tx.execute(
        r#"
        insert into work_records(
            work_unit_id, topic, work_performed, next_actions, notable_operations,
            export_path, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, current_timestamp)
        "#,
        params![
            work_unit_id,
            input.topic,
            input.work_performed,
            input.next_actions,
            input.notable_operations,
            input.export_path,
        ],
    )?;
    let work_record_id = tx.last_insert_rowid();
    tx.commit()?;

    Ok(WorkRecordOutcome {
        work_record_id,
        work_unit_id,
    })
}

pub fn list_work_records(root: &Path, work_unit_id: Option<i64>) -> Result<Vec<WorkRecordEntry>> {
    let conn = open_existing_project(root)?;
    let mut records = Vec::new();

    match work_unit_id {
        Some(work_unit_id) => {
            let mut stmt = conn.prepare(
                r#"
                select id, work_unit_id, topic, work_performed, next_actions, created_at
                from work_records
                where work_unit_id = ?1
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![work_unit_id], work_record_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        None => {
            let mut stmt = conn.prepare(
                r#"
                select id, work_unit_id, topic, work_performed, next_actions, created_at
                from work_records
                order by id
                "#,
            )?;
            let rows = stmt.query_map([], work_record_record)?;
            for row in rows {
                records.push(row?);
            }
        }
    }

    Ok(records)
}

pub fn add_work_record_command(
    root: &Path,
    input: NewWorkRecordCommand<'_>,
) -> Result<WorkRecordLinkOutcome> {
    let conn = open_existing_project(root)?;
    ensure_work_record_exists(&conn, input.work_record_id)?;
    conn.execute(
        r#"
        insert into work_record_commands(
            work_record_id, command_profile_id, command, result, log_path, note
        )
        values (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        params![
            input.work_record_id,
            input.command_profile_id,
            input.command,
            input.result,
            input.log_path,
            input.note,
        ],
    )?;

    Ok(WorkRecordLinkOutcome {
        link_id: conn.last_insert_rowid(),
    })
}

pub fn add_work_record_commit(
    root: &Path,
    input: NewWorkRecordCommit<'_>,
) -> Result<WorkRecordLinkOutcome> {
    let conn = open_existing_project(root)?;
    ensure_work_record_exists(&conn, input.work_record_id)?;
    conn.execute(
        r#"
        insert into work_record_commits(work_record_id, commit_sha, role, note)
        values (?1, ?2, ?3, ?4)
        "#,
        params![
            input.work_record_id,
            input.commit_sha,
            input.role,
            input.note
        ],
    )?;

    Ok(WorkRecordLinkOutcome {
        link_id: conn.last_insert_rowid(),
    })
}

pub fn add_work_record_file(
    root: &Path,
    input: NewWorkRecordFile<'_>,
) -> Result<WorkRecordLinkOutcome> {
    let conn = open_existing_project(root)?;
    ensure_work_record_exists(&conn, input.work_record_id)?;
    conn.execute(
        r#"
        insert into work_record_files(work_record_id, path, role, note)
        values (?1, ?2, ?3, ?4)
        "#,
        params![input.work_record_id, input.path, input.role, input.note],
    )?;

    Ok(WorkRecordLinkOutcome {
        link_id: conn.last_insert_rowid(),
    })
}

fn work_record_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkRecordEntry> {
    Ok(WorkRecordEntry {
        id: row.get(0)?,
        work_unit_id: row.get(1)?,
        topic: row.get(2)?,
        work_performed: row.get(3)?,
        next_actions: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn ensure_work_record_exists(conn: &Connection, work_record_id: i64) -> Result<()> {
    let exists = conn
        .query_row(
            "select 1 from work_records where id = ?1",
            params![work_record_id],
            |_| Ok(()),
        )
        .optional()?;
    exists.context("work record not found")
}

pub struct NewWorkRecord<'a> {
    pub work_unit_id: Option<i64>,
    pub topic: &'a str,
    pub work_performed: Option<&'a str>,
    pub next_actions: Option<&'a str>,
    pub notable_operations: Option<&'a str>,
    pub export_path: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct WorkRecordOutcome {
    pub work_record_id: i64,
    pub work_unit_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct WorkRecordEntry {
    pub id: i64,
    pub work_unit_id: Option<i64>,
    pub topic: String,
    pub work_performed: Option<String>,
    pub next_actions: Option<String>,
    pub created_at: String,
}

pub struct NewWorkRecordCommand<'a> {
    pub work_record_id: i64,
    pub command_profile_id: Option<i64>,
    pub command: &'a str,
    pub result: Option<&'a str>,
    pub log_path: Option<&'a str>,
    pub note: Option<&'a str>,
}

pub struct NewWorkRecordCommit<'a> {
    pub work_record_id: i64,
    pub commit_sha: &'a str,
    pub role: &'a str,
    pub note: Option<&'a str>,
}

pub struct NewWorkRecordFile<'a> {
    pub work_record_id: i64,
    pub path: &'a str,
    pub role: &'a str,
    pub note: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct WorkRecordLinkOutcome {
    pub link_id: i64,
}
