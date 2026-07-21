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
    let first_task = add_task(
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

    let second = interrupt_work(
        temp.path(),
        "second owner",
        "exercise parent owner topology",
    )
    .unwrap();
    let child_tasks = ["second task", "second owner mutation"].map(|title| {
        add_task(
            temp.path(),
            NewTask {
                work_unit_id: Some(second.child_work_unit_id),
                title,
                priority: "medium",
                source: "user",
                details: None,
                completion_condition: Some("done"),
            },
        )
        .unwrap()
    });
    let after: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), None).unwrap().json).unwrap();
    let entries = after["index"]["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry["work_unit_id"].as_i64().unwrap())
            .collect::<std::collections::BTreeSet<_>>(),
        std::collections::BTreeSet::from([first.work_unit_id, second.child_work_unit_id])
    );
    let plan_for = |work_unit_id| {
        let owner = entries
            .iter()
            .find(|entry| entry["work_unit_id"] == work_unit_id)
            .unwrap()["owner_handle"]
            .as_str()
            .unwrap()
            .to_string();
        let plan = serde_json::from_str::<serde_json::Value>(
            &plan_task_identity(temp.path(), Some(&owner)).unwrap().json,
        )
        .unwrap();
        (owner, plan)
    };
    let (parent_owner, parent_plan) = plan_for(first.work_unit_id);
    let (child_owner, child_plan) = plan_for(second.child_work_unit_id);
    assert_eq!(parent_plan["plan"]["aliases"].as_array().unwrap().len(), 1);
    assert_eq!(child_plan["plan"]["aliases"].as_array().unwrap().len(), 2);
    for (owner, plan) in [(&parent_owner, &parent_plan), (&child_owner, &child_plan)] {
        apply_task_identity(
            temp.path(),
            owner,
            plan["plan"]["plan_handle"].as_str().unwrap(),
        )
        .unwrap();
    }
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let owned_aliases = |owner| {
        conn.prepare(
            "select alias.historical_task_id from task_revision_aliases alias join task_revisions revision on revision.id=alias.task_revision_id join task_identities identity on identity.id=revision.task_identity_id where identity.owner_work_unit_id=?1 order by alias.historical_task_id",
        )
        .unwrap()
        .query_map([owner], |row| row.get::<_, i64>(0))
        .unwrap()
        .collect::<rusqlite::Result<std::collections::BTreeSet<_>>>()
        .unwrap()
    };
    assert_eq!(
        owned_aliases(first.work_unit_id),
        std::collections::BTreeSet::from([first_task.task_id])
    );
    assert_eq!(
        owned_aliases(second.child_work_unit_id),
        child_tasks
            .iter()
            .map(|task| task.task_id)
            .collect::<std::collections::BTreeSet<_>>()
    );
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
