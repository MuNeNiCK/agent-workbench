use super::*;

#[test]
fn owner_handle_is_isolated_from_other_owner_changes() {
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
    suspend_work(temp.path(), "owner isolation setup", "create second owner").unwrap();
    let before: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), None).unwrap().json).unwrap();
    let stable_owner = before["index"]["entries"][0]["owner_handle"]
        .as_str()
        .unwrap()
        .to_string();

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
    for title in ["second task", "second owner mutation"] {
        add_task(
            temp.path(),
            NewTask {
                work_unit_id: Some(second.work_unit_id),
                title,
                priority: "medium",
                source: "user",
                details: None,
                completion_condition: Some("done"),
            },
        )
        .unwrap();
    }
    let after: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), None).unwrap().json).unwrap();
    assert!(
        after["index"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|entry| entry["owner_handle"] == stable_owner)
    );
    plan_task_identity(temp.path(), Some(&stable_owner)).unwrap();
}

#[test]
fn wal_checkpoint_does_not_change_owner_or_plan_handles() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.pragma_update(None, "journal_mode", "wal").unwrap();
    drop(conn);
    let work = start_work_with_options(
        temp.path(),
        WorkStart {
            title: "wal owner",
            responsibility: None,
            design_version_id: None,
            implementation: false,
        },
    )
    .unwrap();
    add_task(
        temp.path(),
        NewTask {
            work_unit_id: Some(work.work_unit_id),
            title: "wal task",
            priority: "high",
            source: "user",
            details: None,
            completion_condition: Some("done"),
        },
    )
    .unwrap();
    let before_index: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), None).unwrap().json).unwrap();
    let owner = before_index["index"]["entries"][0]["owner_handle"]
        .as_str()
        .unwrap()
        .to_string();
    let before_plan: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), Some(&owner)).unwrap().json).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch("pragma wal_checkpoint(truncate)")
        .unwrap();
    drop(conn);
    let after_index: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), None).unwrap().json).unwrap();
    let after_plan: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), Some(&owner)).unwrap().json).unwrap();
    assert_eq!(
        before_index["index"]["entries"][0]["owner_handle"],
        after_index["index"]["entries"][0]["owner_handle"]
    );
    assert_eq!(
        before_plan["plan"]["plan_handle"],
        after_plan["plan"]["plan_handle"]
    );
}

#[test]
fn unrelated_views_and_triggers_do_not_change_migration_handles() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work_with_options(
        temp.path(),
        WorkStart {
            title: "schema-independent owner",
            responsibility: None,
            design_version_id: None,
            implementation: false,
        },
    )
    .unwrap();
    add_task(
        temp.path(),
        NewTask {
            work_unit_id: Some(work.work_unit_id),
            title: "schema-independent task",
            priority: "high",
            source: "user",
            details: None,
            completion_condition: Some("done"),
        },
    )
    .unwrap();
    let before_index: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), None).unwrap().json).unwrap();
    let owner = before_index["index"]["entries"][0]["owner_handle"]
        .as_str()
        .unwrap();
    let before_plan: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), Some(owner)).unwrap().json).unwrap();

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        create view unrelated_task_titles as select id,title from tasks;
        create trigger unrelated_task_readback after update of title on tasks
        begin select new.id; end;
        "#,
    )
    .unwrap();
    drop(conn);

    let after_index: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), None).unwrap().json).unwrap();
    let after_plan: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), Some(owner)).unwrap().json).unwrap();
    assert_eq!(
        before_index["index"]["entries"][0]["owner_handle"],
        after_index["index"]["entries"][0]["owner_handle"]
    );
    assert_eq!(
        before_plan["plan"]["plan_handle"],
        after_plan["plan"]["plan_handle"]
    );
}

#[test]
fn owner_application_order_is_commutative() {
    let (forward_identities, forward_revisions, forward_audits) = migrated_owner_semantics(false);
    let (reverse_identities, reverse_revisions, reverse_audits) = migrated_owner_semantics(true);
    assert_eq!(forward_identities, reverse_identities);
    assert_eq!(forward_revisions, reverse_revisions);
    assert_eq!(forward_audits.len(), reverse_audits.len());
    assert_eq!(forward_audits.len(), 2);
}
