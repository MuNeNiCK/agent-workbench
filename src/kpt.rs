use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use crate::db::{open_existing_project, project_id};

pub fn start_kpt_review(root: &Path, input: NewKptReview<'_>) -> Result<KptReviewOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let period_modifier = input.period.map(period_to_sqlite_modifier).transpose()?;
    tx.execute(
        r#"
        insert into kpt_reviews(
            project_id, scope, period_start, period_end, trigger, summary, status, created_at
        )
        values (
            ?1, ?2,
            case when ?3 is null then null else datetime('now', ?3) end,
            case when ?3 is null then null else current_timestamp end,
            'manual', ?4, 'open', current_timestamp
        )
        "#,
        params![project_id, input.scope, period_modifier, input.summary],
    )?;
    let kpt_review_id = tx.last_insert_rowid();

    let generated_item_count = if input
        .from
        .is_some_and(|source| source.split(',').any(|part| part.trim() == "corrections"))
    {
        import_corrections_as_kpt_items(
            &tx,
            kpt_review_id,
            project_id,
            input.scope,
            period_modifier.as_deref(),
        )?
    } else {
        0
    };

    tx.commit()?;

    Ok(KptReviewOutcome {
        kpt_review_id,
        generated_item_count,
    })
}

pub fn list_kpt_reviews(root: &Path, status: Option<&str>) -> Result<Vec<KptReviewRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut records = Vec::new();

    match status {
        Some(status) => {
            let mut stmt = conn.prepare(
                r#"
                select id, scope, summary, status, created_at, closed_at
                from kpt_reviews
                where project_id = ?1 and status = ?2
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![project_id, status], kpt_review_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        None => {
            let mut stmt = conn.prepare(
                r#"
                select id, scope, summary, status, created_at, closed_at
                from kpt_reviews
                where project_id = ?1
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![project_id], kpt_review_record)?;
            for row in rows {
                records.push(row?);
            }
        }
    }

    Ok(records)
}

pub fn add_kpt_item(root: &Path, input: NewKptItem<'_>) -> Result<KptItemOutcome> {
    let conn = open_existing_project(root)?;
    let review_id = match input.kpt_review_id {
        Some(id) => id,
        None => latest_open_kpt_review(&conn)?,
    };

    conn.execute(
        r#"
        insert into kpt_items(
            kpt_review_id, item_type, title, details, severity, proposed_action,
            status, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, 'open', current_timestamp)
        "#,
        params![
            review_id,
            input.item_type,
            input.title,
            input.details,
            input.severity,
            input.proposed_action,
        ],
    )?;

    Ok(KptItemOutcome {
        kpt_item_id: conn.last_insert_rowid(),
        kpt_review_id: review_id,
    })
}

pub fn list_kpt_items(root: &Path, kpt_review_id: Option<i64>) -> Result<Vec<KptItemRecord>> {
    let conn = open_existing_project(root)?;
    let mut records = Vec::new();

    match kpt_review_id {
        Some(kpt_review_id) => {
            let mut stmt = conn.prepare(
                r#"
                select id, kpt_review_id, item_type, title, severity, status, linked_task_id
                from kpt_items
                where kpt_review_id = ?1
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![kpt_review_id], kpt_item_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        None => {
            let mut stmt = conn.prepare(
                r#"
                select id, kpt_review_id, item_type, title, severity, status, linked_task_id
                from kpt_items
                order by id
                "#,
            )?;
            let rows = stmt.query_map([], kpt_item_record)?;
            for row in rows {
                records.push(row?);
            }
        }
    }

    Ok(records)
}

pub fn close_kpt_review(root: &Path, kpt_review_id: i64) -> Result<KptReviewCloseOutcome> {
    let conn = open_existing_project(root)?;
    let changed = conn.execute(
        "update kpt_reviews set status = 'closed', closed_at = current_timestamp where id = ?1 and status = 'open'",
        params![kpt_review_id],
    )?;
    if changed == 0 {
        bail!("kpt review not found or already closed");
    }

    Ok(KptReviewCloseOutcome { kpt_review_id })
}

pub fn convert_kpt_item_to_task(
    root: &Path,
    input: KptItemTaskConversion<'_>,
) -> Result<KptItemConversionOutcome> {
    let conn = open_existing_project(root)?;
    let item = conn
        .query_row(
            r#"
            select id, title, details, proposed_action
            from kpt_items
            where id = ?1 and status in ('open', 'accepted')
            "#,
            params![input.kpt_item_id],
            |row| {
                Ok(StoredKptItem {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    details: row.get(2)?,
                    proposed_action: row.get(3)?,
                })
            },
        )
        .optional()?
        .context("kpt item not found or not convertible")?;
    let task_title = input.task_title.unwrap_or(&item.title);
    let details = input
        .details
        .or(item.proposed_action.as_deref())
        .or(item.details.as_deref());

    conn.execute(
        r#"
        insert into tasks(work_unit_id, title, priority, status, source, details)
        values (?1, ?2, ?3, 'open', 'review', ?4)
        "#,
        params![input.work_unit_id, task_title, input.priority, details],
    )?;
    let task_id = conn.last_insert_rowid();
    conn.execute(
        r#"
        insert into kpt_item_conversions(kpt_item_id, target_type, task_id, created_at)
        values (?1, 'task', ?2, current_timestamp)
        "#,
        params![item.id, task_id],
    )?;
    let conversion_id = conn.last_insert_rowid();
    conn.execute(
        "update kpt_items set status = 'converted_to_task', linked_task_id = ?1 where id = ?2",
        params![task_id, item.id],
    )?;

    Ok(KptItemConversionOutcome {
        kpt_item_conversion_id: conversion_id,
        task_id,
    })
}

fn latest_open_kpt_review(conn: &rusqlite::Connection) -> Result<i64> {
    conn.query_row(
        "select id from kpt_reviews where status = 'open' order by id desc limit 1",
        [],
        |row| row.get::<_, i64>(0),
    )
    .optional()?
    .context("no open kpt review; run kpt start first")
}

fn import_corrections_as_kpt_items(
    conn: &Connection,
    kpt_review_id: i64,
    project_id: i64,
    scope: Option<&str>,
    period_modifier: Option<&str>,
) -> Result<i64> {
    let corrections = corrections_for_kpt(conn, project_id, scope, period_modifier)?;
    for correction in &corrections {
        let title = format!("Repeated correction: {}", correction.mistake_pattern);
        let details = format!(
            "scope: {}\ntype: {}\ncorrection: {}",
            correction.scope, correction.correction_type, correction.correction
        );
        conn.execute(
            r#"
            insert into kpt_items(
                kpt_review_id, item_type, title, details, severity,
                linked_user_correction_id, proposed_action, status, created_at
            )
            values (?1, 'problem', ?2, ?3, ?4, ?5, ?6, 'open', current_timestamp)
            "#,
            params![
                kpt_review_id,
                title,
                details,
                correction.severity,
                correction.id,
                correction.correction,
            ],
        )?;
    }

    Ok(corrections.len() as i64)
}

fn corrections_for_kpt(
    conn: &Connection,
    project_id: i64,
    scope: Option<&str>,
    period_modifier: Option<&str>,
) -> Result<Vec<StoredCorrection>> {
    let mut records = Vec::new();
    match (scope, period_modifier) {
        (Some(scope), Some(period_modifier)) => {
            let mut stmt = conn.prepare(
                r#"
                select id, scope, correction_type, mistake_pattern, correction, severity
                from user_corrections
                where project_id = ?1
                  and status = 'active'
                  and scope = ?2
                  and created_at >= datetime('now', ?3)
                order by id
                "#,
            )?;
            let rows = stmt.query_map(
                params![project_id, scope, period_modifier],
                stored_correction_record,
            )?;
            for row in rows {
                records.push(row?);
            }
        }
        (Some(scope), None) => {
            let mut stmt = conn.prepare(
                r#"
                select id, scope, correction_type, mistake_pattern, correction, severity
                from user_corrections
                where project_id = ?1 and status = 'active' and scope = ?2
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![project_id, scope], stored_correction_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        (None, Some(period_modifier)) => {
            let mut stmt = conn.prepare(
                r#"
                select id, scope, correction_type, mistake_pattern, correction, severity
                from user_corrections
                where project_id = ?1
                  and status = 'active'
                  and created_at >= datetime('now', ?2)
                order by id
                "#,
            )?;
            let rows = stmt.query_map(
                params![project_id, period_modifier],
                stored_correction_record,
            )?;
            for row in rows {
                records.push(row?);
            }
        }
        (None, None) => {
            let mut stmt = conn.prepare(
                r#"
                select id, scope, correction_type, mistake_pattern, correction, severity
                from user_corrections
                where project_id = ?1 and status = 'active'
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![project_id], stored_correction_record)?;
            for row in rows {
                records.push(row?);
            }
        }
    }

    Ok(records)
}

fn period_to_sqlite_modifier(period: &str) -> Result<String> {
    let period = period.trim();
    if let Some(days) = period.strip_suffix('d') {
        let days = days.parse::<u32>()?;
        if days == 0 {
            bail!("period must be greater than zero");
        }
        return Ok(format!("-{days} days"));
    }
    if let Some(hours) = period.strip_suffix('h') {
        let hours = hours.parse::<u32>()?;
        if hours == 0 {
            bail!("period must be greater than zero");
        }
        return Ok(format!("-{hours} hours"));
    }
    bail!("unsupported period; use values like 30d or 12h")
}

fn kpt_review_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<KptReviewRecord> {
    Ok(KptReviewRecord {
        id: row.get(0)?,
        scope: row.get(1)?,
        summary: row.get(2)?,
        status: row.get(3)?,
        created_at: row.get(4)?,
        closed_at: row.get(5)?,
    })
}

fn kpt_item_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<KptItemRecord> {
    Ok(KptItemRecord {
        id: row.get(0)?,
        kpt_review_id: row.get(1)?,
        item_type: row.get(2)?,
        title: row.get(3)?,
        severity: row.get(4)?,
        status: row.get(5)?,
        linked_task_id: row.get(6)?,
    })
}

fn stored_correction_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredCorrection> {
    Ok(StoredCorrection {
        id: row.get(0)?,
        scope: row.get(1)?,
        correction_type: row.get(2)?,
        mistake_pattern: row.get(3)?,
        correction: row.get(4)?,
        severity: row.get(5)?,
    })
}

struct StoredKptItem {
    id: i64,
    title: String,
    details: Option<String>,
    proposed_action: Option<String>,
}

struct StoredCorrection {
    id: i64,
    scope: String,
    correction_type: String,
    mistake_pattern: String,
    correction: String,
    severity: String,
}

pub struct NewKptReview<'a> {
    pub scope: Option<&'a str>,
    pub summary: Option<&'a str>,
    pub from: Option<&'a str>,
    pub period: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptReviewOutcome {
    pub kpt_review_id: i64,
    pub generated_item_count: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptReviewRecord {
    pub id: i64,
    pub scope: Option<String>,
    pub summary: Option<String>,
    pub status: String,
    pub created_at: String,
    pub closed_at: Option<String>,
}

pub struct NewKptItem<'a> {
    pub kpt_review_id: Option<i64>,
    pub item_type: &'a str,
    pub title: &'a str,
    pub details: Option<&'a str>,
    pub severity: &'a str,
    pub proposed_action: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemOutcome {
    pub kpt_item_id: i64,
    pub kpt_review_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemRecord {
    pub id: i64,
    pub kpt_review_id: i64,
    pub item_type: String,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub linked_task_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptReviewCloseOutcome {
    pub kpt_review_id: i64,
}

pub struct KptItemTaskConversion<'a> {
    pub kpt_item_id: i64,
    pub task_title: Option<&'a str>,
    pub details: Option<&'a str>,
    pub priority: &'a str,
    pub work_unit_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemConversionOutcome {
    pub kpt_item_conversion_id: i64,
    pub task_id: i64,
}
