use anyhow::Result;
use rusqlite::{Connection, params};

pub(super) fn migration_applied(conn: &Connection, owner_digest: &str) -> Result<bool> {
    let table_exists: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='table' and name='task_identity_migration_audits')",
        [],
        |row| row.get(0),
    )?;
    if !table_exists {
        return Ok(false);
    }
    conn.query_row(
        "select exists(select 1 from task_identity_migration_audits where owner_digest=?1 and status='applied')",
        params![owner_digest],
        |row| row.get(0),
    )
    .map_err(Into::into)
}
