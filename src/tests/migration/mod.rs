use super::*;
use crate::db::{default_ledger_path, open_ledger};
use crate::{
    NewPhaseDependency, NewTask, NewWorkPhase, WorkStart, add_phase_dependency, add_task,
    assign_task_to_phase, close_phase, close_task, create_phase, init_project,
    start_work_with_options, suspend_work,
};
use rusqlite::params;

mod apply;
mod plan;
mod recovery;
mod stability;
mod status;

fn migrated_owner_semantics(reverse: bool) -> (Vec<(i64, String)>, Vec<String>, Vec<String>) {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let first = start_work_with_options(
        temp.path(),
        WorkStart {
            title: "first owner",
            responsibility: None,
            design_version_id: None,
            implementation: false,
        },
    )
    .unwrap();
    add_task(
        temp.path(),
        NewTask {
            work_unit_id: Some(first.work_unit_id),
            title: "first task",
            priority: "high",
            source: "user",
            details: None,
            completion_condition: Some("done"),
        },
    )
    .unwrap();
    suspend_work(temp.path(), "owner-order comparison", "create second owner").unwrap();
    let second = start_work_with_options(
        temp.path(),
        WorkStart {
            title: "second owner",
            responsibility: None,
            design_version_id: None,
            implementation: false,
        },
    )
    .unwrap();
    add_task(
        temp.path(),
        NewTask {
            work_unit_id: Some(second.work_unit_id),
            title: "second task",
            priority: "medium",
            source: "user",
            details: None,
            completion_condition: Some("done"),
        },
    )
    .unwrap();
    let index: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), None).unwrap().json).unwrap();
    let mut owners = index["index"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["owner_handle"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    if reverse {
        owners.reverse();
    }
    for owner in owners {
        let plan: serde_json::Value =
            serde_json::from_str(&plan_task_identity(temp.path(), Some(&owner)).unwrap().json)
                .unwrap();
        let plan_handle = plan["plan"]["plan_handle"].as_str().unwrap();
        apply_task_identity(temp.path(), &owner, plan_handle).unwrap();
    }
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let identities = conn
        .prepare("select owner_work_unit_id,identity_digest from task_identities order by owner_work_unit_id,identity_digest")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let revisions = conn
        .prepare("select revision_digest from task_revisions order by revision_digest")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let audits = conn
        .prepare("select plan_digest from task_identity_migration_audits order by owner_digest")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    (identities, revisions, audits)
}
