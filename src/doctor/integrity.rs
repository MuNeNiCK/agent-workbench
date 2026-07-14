use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension};

pub(super) fn require_doctor_schema(conn: &Connection) -> Result<()> {
    for table in ["validation_runs", "validation_gates"] {
        if !table_exists(conn, table)? {
            bail!(
                "ledger does not contain required table `{table}`; validation-link doctor cannot infer a repair"
            );
        }
    }
    for column in [
        "project_id",
        "validation_gate_id",
        "work_unit_id",
        "task_id",
        "command_usage_id",
        "repository_snapshot_id",
        "command",
        "acceptance_record_id",
    ] {
        if !table_has_column(conn, "validation_runs", column)? {
            bail!(
                "ledger validation_runs table lacks `{column}`; upgrade through a supported release before using this doctor"
            );
        }
    }
    Ok(())
}

pub(super) fn project_exists(conn: &Connection, id: i64) -> Result<bool> {
    Ok(conn
        .query_row("select 1 from projects where id = ?1", [id], |_| Ok(()))
        .optional()?
        .is_some())
}

pub(super) fn work_unit_project(conn: &Connection, id: i64) -> Result<Option<i64>> {
    conn.query_row(
        "select project_id from work_units where id = ?1",
        [id],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

pub(super) fn task_scope(conn: &Connection, id: i64) -> Result<Option<(i64, Option<i64>)>> {
    conn.query_row(
        r#"
        select w.project_id, t.work_unit_id
        from tasks t join work_units w on w.id = t.work_unit_id
        where t.id = ?1
        "#,
        [id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .map_err(Into::into)
}

pub(super) fn snapshot_project(conn: &Connection, id: i64) -> Result<Option<i64>> {
    conn.query_row(
        r#"
        select r.project_id
        from repository_snapshots s join repositories r on r.id = s.repository_id
        where s.id = ?1
        "#,
        [id],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

pub(super) fn unknown_validation_run_references(
    conn: &Connection,
) -> Result<Vec<(String, String)>> {
    let known = BTreeSet::from([
        ("artifacts".to_string(), "validation_run_id".to_string()),
        (
            "acceptance_records".to_string(),
            "validation_run_id".to_string(),
        ),
    ]);
    let mut statement = conn.prepare(
        "select name from sqlite_schema where type = 'table' and name not like 'sqlite_%' order by name",
    )?;
    let tables = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut unknown = Vec::new();
    for table in tables {
        let pragma = format!("pragma foreign_key_list({})", quote_identifier(&table));
        let mut foreign_keys = conn.prepare(&pragma)?;
        let rows = foreign_keys.query_map([], |row| {
            Ok((row.get::<_, String>(2)?, row.get::<_, String>(3)?))
        })?;
        for (target, from) in rows.collect::<rusqlite::Result<Vec<_>>>()? {
            if target == "validation_runs" && !known.contains(&(table.clone(), from.clone())) {
                unknown.push((table.clone(), from));
            }
        }
    }
    Ok(unknown)
}

pub(super) fn quote_identifier(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

pub(super) fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "select 1 from sqlite_schema where type = 'table' and name = ?1",
            [table],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

pub(super) fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let sql = format!("pragma table_info({})", quote_identifier(table));
    let mut statement = conn.prepare(&sql)?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(columns.iter().any(|candidate| candidate == column))
}

pub(super) fn ensure_audit_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        create table if not exists validation_link_repair_runs (
            id integer primary key,
            backup_path text not null unique,
            repaired_validation_run_count integer not null,
            change_count integer not null,
            created_at text not null
        );
        create table if not exists validation_link_repair_changes (
            id integer primary key,
            repair_run_id integer not null references validation_link_repair_runs(id),
            validation_run_id integer not null,
            entity_type text not null,
            entity_id integer not null,
            field_name text not null,
            before_value text,
            after_value text,
            created_at text not null
        );
        create trigger if not exists trg_validation_link_repair_runs_immutable_update
        before update on validation_link_repair_runs begin
            select raise(abort, 'validation link repair audit is immutable');
        end;
        create trigger if not exists trg_validation_link_repair_runs_immutable_delete
        before delete on validation_link_repair_runs begin
            select raise(abort, 'validation link repair audit is immutable');
        end;
        create trigger if not exists trg_validation_link_repair_changes_immutable_update
        before update on validation_link_repair_changes begin
            select raise(abort, 'validation link repair audit is immutable');
        end;
        create trigger if not exists trg_validation_link_repair_changes_immutable_delete
        before delete on validation_link_repair_changes begin
            select raise(abort, 'validation link repair audit is immutable');
        end;
        "#,
    )?;
    Ok(())
}

pub(super) fn validate_database_integrity(conn: &Connection) -> Result<()> {
    let foreign_key_failure = conn
        .query_row(
            "select \"table\", rowid, parent from pragma_foreign_key_check limit 1",
            [],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    if let Some((table, rowid, parent)) = foreign_key_failure {
        bail!(
            "foreign key check failed for {table} row {} referencing {parent}",
            rowid.map_or_else(|| "<unknown>".to_string(), |value| value.to_string())
        );
    }
    let integrity: String = conn.query_row("pragma integrity_check(1)", [], |row| row.get(0))?;
    if integrity != "ok" {
        bail!("SQLite integrity check failed: {integrity}");
    }
    Ok(())
}

pub(super) fn next_backup_path(root: &Path) -> Result<PathBuf> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before Unix epoch")?
        .as_millis();
    Ok(root.join(".agent-workbench").join("backups").join(format!(
        "validation-links-{millis}-{}.sqlite",
        std::process::id()
    )))
}
