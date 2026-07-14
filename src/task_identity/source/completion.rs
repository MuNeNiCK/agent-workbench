use anyhow::Result;
use rusqlite::{Connection, params};

use super::super::status::{ChecklistItemState, ChecklistState};

#[derive(Clone, Debug)]
pub(crate) struct ChecklistSource {
    pub(crate) checklist_id: i64,
    pub(crate) status: ChecklistState,
    pub(crate) items: Vec<ChecklistItemSource>,
}

#[derive(Clone, Debug)]
pub(crate) struct ChecklistItemSource {
    pub(crate) item_id: i64,
    pub(crate) status: ChecklistItemState,
    pub(crate) acceptance_ids: Vec<i64>,
}

pub(super) fn read(conn: &Connection, task_id: i64) -> Result<Vec<ChecklistSource>> {
    let mut stmt = conn.prepare(
        r#"
        select distinct c.id,c.status
        from checklists c
        join checklist_items i on i.checklist_id=c.id
        where i.task_id=?1
        order by c.id
        "#,
    )?;
    let rows = stmt
        .query_map(params![task_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(checklist_id, status)| {
            Ok(ChecklistSource {
                checklist_id,
                status: ChecklistState::parse(&status)?,
                items: read_items(conn, checklist_id, task_id)?,
            })
        })
        .collect()
}

fn read_items(
    conn: &Connection,
    checklist_id: i64,
    task_id: i64,
) -> Result<Vec<ChecklistItemSource>> {
    let mut stmt = conn.prepare(
        r#"
        select id,status
        from checklist_items
        where checklist_id=?1 and task_id=?2
        order by item_order,id
        "#,
    )?;
    let rows = stmt
        .query_map(params![checklist_id, task_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(item_id, status)| {
            Ok(ChecklistItemSource {
                item_id,
                status: ChecklistItemState::parse(&status)?,
                acceptance_ids: read_acceptances(conn, item_id)?,
            })
        })
        .collect()
}

fn read_acceptances(conn: &Connection, item_id: i64) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        r#"
        select id from acceptance_records
        where target_type='checklist_item' and checklist_item_id=?1 and status='approved'
        order by id
        "#,
    )?;
    stmt.query_map(params![item_id], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}
