use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

use crate::identity::{CanonicalValue, domain_digest, normalize_document, signed_source_id};

pub(crate) fn materialize_manual_task(
    conn: &Connection,
    project_id: i64,
    task_id: i64,
) -> Result<(i64, i64)> {
    if let Some(existing) = conn
        .query_row(
            r#"
            select revision.task_identity_id,revision.id
            from task_revision_aliases alias
            join task_revisions revision on revision.id=alias.task_revision_id
            where alias.project_id=?1 and alias.historical_task_id=?2
              and revision.status='current'
            "#,
            params![project_id, task_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?
    {
        return Ok(existing);
    }
    let (work_unit_id, title, details): (i64, String, Option<String>) = conn
        .query_row(
            "select work_unit_id,title,details from tasks where id=?1",
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .context("owned task not found")?;
    let identity_key = CanonicalValue::object([
        ("kind", CanonicalValue::string("manual")),
        (
            "project",
            CanonicalValue::string(signed_source_id(project_id)?),
        ),
        (
            "historical_task",
            CanonicalValue::string(signed_source_id(task_id)?),
        ),
    ]);
    let identity_digest = domain_digest(b"AWB-TASK-IDENTITY-v1\0", &identity_key);
    conn.execute(
        "insert into task_identities(project_id,owner_work_unit_id,identity_digest,kind,status,created_at) values(?1,?2,?3,'manual','current',current_timestamp)",
        params![project_id, work_unit_id, identity_digest],
    )?;
    let identity_id = conn.last_insert_rowid();
    let revision_digest = manual_revision_digest(project_id, task_id, &title, details.as_deref())?;
    conn.execute(
        "insert into task_revisions(project_id,task_identity_id,source_design_requirement_id,revision_digest,design_sequence,status,created_at) values(?1,?2,null,?3,null,'current',current_timestamp)",
        params![project_id, identity_id, revision_digest],
    )?;
    let revision_id = conn.last_insert_rowid();
    conn.execute(
        "insert into task_revision_aliases(project_id,task_revision_id,historical_task_id,source_schema,created_at) values(?1,?2,?3,15,current_timestamp)",
        params![project_id, revision_id, task_id],
    )?;
    Ok((identity_id, revision_id))
}

pub(crate) fn revise_manual_task(
    conn: &Connection,
    project_id: i64,
    task_id: i64,
    completion_condition: &str,
) -> Result<i64> {
    let (identity_id, kind, title, details): (i64, String, String, Option<String>) = conn
        .query_row(
            r#"
            select identity.id,identity.kind,task.title,task.details
            from task_revision_aliases alias
            join task_revisions revision on revision.id=alias.task_revision_id
            join task_identities identity on identity.id=revision.task_identity_id
            join tasks task on task.id=alias.historical_task_id
            where alias.project_id=?1 and alias.historical_task_id=?2
            "#,
            params![project_id, task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?
        .context("task correction requires one canonical task identity alias")?;
    if kind != "manual" {
        anyhow::bail!("task correction is not bound to a current Decomposition Plan revision");
    }
    let revision_digest = manual_correction_revision_digest(
        project_id,
        task_id,
        &title,
        details.as_deref(),
        completion_condition,
    )?;
    let existing = conn
        .query_row(
            "select id,status from task_revisions where project_id=?1 and task_identity_id=?2 and revision_digest=?3",
            params![project_id, identity_id, revision_digest],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let revision_id = match existing {
        Some((id, status)) => {
            if status != "current" {
                conn.execute(
                    "update task_revisions set status='historical' where project_id=?1 and task_identity_id=?2 and status='current'",
                    params![project_id, identity_id],
                )?;
                conn.execute(
                    "update task_revisions set status='current' where id=?1",
                    [id],
                )?;
            }
            id
        }
        None => {
            conn.execute(
                "update task_revisions set status='historical' where project_id=?1 and task_identity_id=?2 and status='current'",
                params![project_id, identity_id],
            )?;
            conn.execute(
                "insert into task_revisions(project_id,task_identity_id,source_design_requirement_id,revision_digest,design_sequence,status,created_at) values(?1,?2,null,?3,null,'current',current_timestamp)",
                params![project_id, identity_id, revision_digest],
            )?;
            conn.last_insert_rowid()
        }
    };
    conn.execute(
        "update task_revision_aliases set task_revision_id=?1 where project_id=?2 and historical_task_id=?3",
        params![revision_id, project_id, task_id],
    )?;
    Ok(revision_id)
}

fn manual_correction_revision_digest(
    project_id: i64,
    task_id: i64,
    title: &str,
    details: Option<&str>,
    completion_condition: &str,
) -> Result<String> {
    Ok(domain_digest(
        b"AWB-MANUAL-CORRECTION-REVISION-v1\0",
        &CanonicalValue::object([
            (
                "base_revision",
                CanonicalValue::string(manual_revision_digest(
                    project_id, task_id, title, details,
                )?),
            ),
            (
                "completion_condition",
                CanonicalValue::string(normalize_document(completion_condition)),
            ),
        ]),
    ))
}

fn manual_revision_digest(
    project_id: i64,
    task_id: i64,
    title: &str,
    details: Option<&str>,
) -> Result<String> {
    let identity_key = CanonicalValue::object([
        ("kind", CanonicalValue::string("manual")),
        (
            "project",
            CanonicalValue::string(signed_source_id(project_id)?),
        ),
        (
            "historical_task",
            CanonicalValue::string(signed_source_id(task_id)?),
        ),
    ]);
    Ok(domain_digest(
        b"AWB-MANUAL-REVISION-v1\0",
        &CanonicalValue::object([
            ("identity_key", identity_key),
            (
                "historical_task",
                CanonicalValue::string(signed_source_id(task_id)?),
            ),
            ("title", CanonicalValue::string(normalize_document(title))),
            (
                "description",
                CanonicalValue::string(normalize_document(details.unwrap_or(""))),
            ),
        ]),
    ))
}
