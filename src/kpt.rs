use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{open_existing_project, project_id};

pub fn start_kpt_review(root: &Path, input: NewKptReview<'_>) -> Result<KptReviewOutcome> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    conn.execute(
        r#"
        insert into kpt_reviews(project_id, scope, trigger, summary, status, created_at)
        values (?1, ?2, 'manual', ?3, 'open', current_timestamp)
        "#,
        params![project_id, input.scope, input.summary],
    )?;

    Ok(KptReviewOutcome {
        kpt_review_id: conn.last_insert_rowid(),
    })
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

struct StoredKptItem {
    id: i64,
    title: String,
    details: Option<String>,
    proposed_action: Option<String>,
}

pub struct NewKptReview<'a> {
    pub scope: Option<&'a str>,
    pub summary: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptReviewOutcome {
    pub kpt_review_id: i64,
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
