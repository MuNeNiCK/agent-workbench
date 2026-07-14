use super::*;

#[test]
fn task_history_migration_has_recursive_help_and_stays_explicit() {
    let temp = tempfile::tempdir().unwrap();
    let migration = ok(temp.path(), &["migration", "--help"]);
    assert!(migration.contains("task-history"));
    let family = ok(temp.path(), &["migration", "task-history", "--help"]);
    for command in [
        "plan",
        "ambiguity-list",
        "authority-record",
        "ambiguity-decide",
        "apply",
        "audit",
    ] {
        assert!(family.contains(command));
        ok(
            temp.path(),
            &["migration", "task-history", command, "--help"],
        );
    }
    let apply = ok(
        temp.path(),
        &["migration", "task-history", "apply", "--help"],
    );
    assert!(apply.contains("--owner <OWNER>"));
    assert!(apply.contains("--plan <PLAN>"));
    assert!(!apply.contains("plan-hash"));

    ok(temp.path(), &["init"]);
    ok(temp.path(), &["status"]);
    let work = ok(temp.path(), &["work", "start", "migration owner"])
        .lines()
        .find_map(|line| line.strip_prefix("work_unit_id: "))
        .unwrap()
        .to_string();
    let task = ok(
        temp.path(),
        &[
            "task",
            "add",
            "--work-unit",
            &work,
            "--completion-condition",
            "done",
            "historical task",
        ],
    )
    .lines()
    .find_map(|line| line.strip_prefix("task_id: "))
    .unwrap()
    .to_string();
    let phase = ok(
        temp.path(),
        &[
            "phase",
            "create",
            "--work-unit",
            &work,
            "--key",
            "historical",
            "--title",
            "Historical",
            "--order",
            "1",
        ],
    )
    .lines()
    .find_map(|line| line.strip_prefix("phase_id: "))
    .unwrap()
    .to_string();
    ok(temp.path(), &["phase", "assign", "--task", &task, &phase]);
    let index = ok(temp.path(), &["migration", "task-history", "plan"]);
    let index: serde_json::Value = serde_json::from_str(index.lines().last().unwrap()).unwrap();
    let owner = index["index"]["entries"][0]["owner_handle"]
        .as_str()
        .unwrap();
    let plan = ok(
        temp.path(),
        &["migration", "task-history", "plan", "--owner", owner],
    );
    let plan: serde_json::Value = serde_json::from_str(plan.lines().last().unwrap()).unwrap();
    let plan_handle = plan["plan"]["plan_handle"].as_str().unwrap();
    let applied = ok(
        temp.path(),
        &[
            "migration",
            "task-history",
            "apply",
            "--owner",
            owner,
            "--plan",
            plan_handle,
        ],
    );
    assert!(applied.contains("result: applied"));
    assert!(applied.contains("backup_handle: backup_"));
    assert!(applied.contains("audit_handle: audit_"));
    let audit = ok(
        temp.path(),
        &["migration", "task-history", "audit", "--owner", owner],
    );
    let audit: serde_json::Value = serde_json::from_str(audit.lines().last().unwrap()).unwrap();
    assert_eq!(audit["records"][0]["result"], "applied");
}
