use std::collections::BTreeMap;

use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use super::super::source::{SourceSnapshot, TaskSource};
use super::super::status::PhaseState;

pub(super) fn materialize(
    conn: &Connection,
    snapshot: &SourceSnapshot,
    task: &TaskSource,
    task_identity_id: i64,
    revision_id: i64,
    membership_ids: &mut BTreeMap<(i64, i64), (i64, Option<i64>, String)>,
) -> Result<()> {
    for membership in &task.memberships {
        let predecessor_epoch_membership = conn
            .query_row(
                r#"
                select epoch.id,epoch.task_identity_id,identity.status
                from phase_epoch_membership_sources source
                join phase_epoch_memberships epoch on epoch.id=source.phase_epoch_membership_id
                join task_identities identity on identity.id=epoch.task_identity_id
                where source.project_id=?1 and source.source_membership_id=?2
                "#,
                params![snapshot.project_id, membership.membership_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        let state = membership.phase_status.as_str();
        let boundary_revision_id = (state == "closed").then_some(revision_id);
        let key = (membership.phase_id, task_identity_id);
        let task_membership_id = match membership_ids.get(&key) {
            Some((id, existing_boundary, existing_state)) => {
                if existing_boundary != &boundary_revision_id || existing_state != state {
                    bail!("membership_state: aliases disagree on phase boundary state");
                }
                *id
            }
            None => {
                let id = match conn
                    .query_row(
                        "select id,boundary_revision_id,state from task_phase_memberships where project_id=?1 and phase_id=?2 and task_identity_id=?3",
                        params![snapshot.project_id, membership.phase_id, task_identity_id],
                        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Option<i64>>(1)?, row.get::<_, String>(2)?)),
                    )
                    .optional()?
                {
                    Some((id, existing_boundary, existing_state))
                        if existing_boundary == boundary_revision_id && existing_state == state => id,
                    Some((id, _, _)) => {
                        conn.execute(
                            "update task_phase_memberships set boundary_revision_id=?1,state=?2,created_at=current_timestamp where id=?3",
                            params![boundary_revision_id, state, id],
                        )?;
                        id
                    }
                    None => {
                        conn.execute(
                            "insert into task_phase_memberships(project_id,phase_id,task_identity_id,boundary_revision_id,state,created_at) values(?1,?2,?3,?4,?5,current_timestamp)",
                            params![
                                snapshot.project_id,
                                membership.phase_id,
                                task_identity_id,
                                boundary_revision_id,
                                state,
                            ],
                        )?;
                        conn.last_insert_rowid()
                    }
                };
                membership_ids.insert(key, (id, boundary_revision_id, state.to_string()));
                id
            }
        };
        let linked_membership = conn
            .query_row(
                r#"
                select source.task_phase_membership_id,identity.status
                from task_phase_membership_sources source
                join task_phase_memberships canonical
                  on canonical.id=source.task_phase_membership_id
                join task_identities identity on identity.id=canonical.task_identity_id
                where source.project_id=?1 and source.source_membership_id=?2
                "#,
                params![snapshot.project_id, membership.membership_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        match linked_membership {
            None => {
                conn.execute(
                    "insert into task_phase_membership_sources(project_id,task_phase_membership_id,source_membership_id,created_at) values(?1,?2,?3,current_timestamp)",
                    params![snapshot.project_id, task_membership_id, membership.membership_id],
                )?;
            }
            Some((linked, _)) if linked == task_membership_id => {}
            Some((_, status)) if status == "retired" => {
                conn.execute(
                    "update task_phase_membership_sources set task_phase_membership_id=?1,created_at=current_timestamp where project_id=?2 and source_membership_id=?3",
                    params![task_membership_id, snapshot.project_id, membership.membership_id],
                )?;
            }
            Some(_) => bail!("membership_state: source points to another canonical membership"),
        }
        let epoch_state = if membership.phase_status == PhaseState::Superseded {
            "superseded"
        } else {
            match state {
                "open" | "blocked" => "current",
                "closed" => "closed",
                "out_of_scope" => "out_of_scope",
                "split" => "split",
                _ => bail!("membership_state: unsupported phase epoch state"),
            }
        };
        let predecessor_id = predecessor_epoch_membership
            .as_ref()
            .filter(|(_, predecessor_identity, status)| {
                *predecessor_identity != task_identity_id && status == "retired"
            })
            .map(|(id, _, _)| *id);
        if let Some(predecessor_id) = predecessor_id {
            conn.execute(
                "update phase_epoch_memberships set state='superseded',terminal_at=current_timestamp where id=?1 and state!='superseded'",
                [predecessor_id],
            )?;
        }
        conn.execute(
            r#"insert into phase_epoch_memberships(project_id,phase_epoch_id,task_identity_id,boundary_revision_id,state,predecessor_membership_id,created_at,terminal_at)
               values(?1,?2,?3,?4,?5,?6,current_timestamp,case when ?5='current' then null else current_timestamp end)
               on conflict(project_id,phase_epoch_id,task_identity_id) do update set
                 boundary_revision_id=excluded.boundary_revision_id,
                 state=excluded.state,
                 predecessor_membership_id=coalesce(phase_epoch_memberships.predecessor_membership_id,excluded.predecessor_membership_id),
                 terminal_at=excluded.terminal_at"#,
            params![
                snapshot.project_id,
                membership.phase_id,
                task_identity_id,
                boundary_revision_id,
                epoch_state,
                predecessor_id,
            ],
        )?;
        let phase_epoch_membership_id: i64 = conn.query_row(
            "select id from phase_epoch_memberships where project_id=?1 and phase_epoch_id=?2 and task_identity_id=?3",
            params![snapshot.project_id, membership.phase_id, task_identity_id],
            |row| row.get(0),
        )?;
        conn.execute(
            r#"insert into phase_epoch_membership_sources(project_id,phase_epoch_membership_id,source_membership_id,source_generation,created_at)
               values(?1,?2,?3,?4,current_timestamp)
               on conflict(source_membership_id) do update set
                 phase_epoch_membership_id=excluded.phase_epoch_membership_id,
                 source_generation=excluded.source_generation,
                 created_at=excluded.created_at"#,
            params![
                snapshot.project_id,
                phase_epoch_membership_id,
                membership.membership_id,
                snapshot.schema_version,
            ],
        )?;
    }
    Ok(())
}
