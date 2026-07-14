use std::collections::BTreeMap;

use anyhow::{Result, bail};
use rusqlite::{Connection, params};

use super::super::source::{SourceSnapshot, TaskSource};

pub(super) fn materialize(
    conn: &Connection,
    snapshot: &SourceSnapshot,
    task: &TaskSource,
    task_identity_id: i64,
    revision_id: i64,
    membership_ids: &mut BTreeMap<(i64, i64), (i64, Option<i64>, String)>,
) -> Result<()> {
    for membership in &task.memberships {
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
                let id = conn.last_insert_rowid();
                membership_ids.insert(key, (id, boundary_revision_id, state.to_string()));
                id
            }
        };
        conn.execute(
            "insert into task_phase_membership_sources(project_id,task_phase_membership_id,source_membership_id,created_at) values(?1,?2,?3,current_timestamp)",
            params![
                snapshot.project_id,
                task_membership_id,
                membership.membership_id
            ],
        )?;
    }
    Ok(())
}
