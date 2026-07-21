use super::*;
use agent_workbench::default_ledger_path;
use rusqlite::Connection;

fn field<'a>(output: &'a str, key: &str) -> &'a str {
    output
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}: ")))
        .unwrap_or_else(|| panic!("missing {key} in:\n{output}"))
}

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

#[test]
fn reviewer_migration_binds_only_a_known_pending_source_with_local_authority() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let family = ok(temp.path(), &["migration", "reviewer", "--help"]);
    assert!(family.contains("bind"));
    let bind_help = ok(temp.path(), &["migration", "reviewer", "bind", "--help"]);
    for flag in [
        "--agent-label",
        "--external-agent-id",
        "--provenance-ref",
        "--authority",
    ] {
        assert!(bind_help.contains(flag));
    }
    for removed in ["signature", "principal", "capability", "trust"] {
        assert!(!bind_help.contains(removed));
    }

    let digest = "a".repeat(64);
    let source = format!("legacy-reviewer:{digest}");
    let conn = Connection::open(default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into reviewer_migration_sources(project_id,source_reviewer_ref,source_reviewer_digest,status,created_at) values(1,?1,?2,'pending',current_timestamp)",
        [&source, &digest],
    )
    .unwrap();
    drop(conn);

    let authority = ok(
        temp.path(),
        &[
            "authority",
            "event",
            "add",
            "--type",
            "user_instruction",
            "--summary",
            "bind retained reviewer provenance",
            "--scope",
            "project",
        ],
    );
    let authority = field(&authority, "authority_event_id");
    let args = [
        "migration",
        "reviewer",
        "bind",
        source.as_str(),
        "--agent-label",
        "independent-reviewer",
        "--external-agent-id",
        "agent:reviewer-7",
        "--provenance-ref",
        "review-output:retained-7",
        "--authority",
        authority,
    ];
    let bound = ok(temp.path(), &args);
    assert!(bound.contains("binding_handle: reviewer_binding_"));
    assert!(bound.contains("status: bound"));
    assert!(bound.contains("idempotent: false"));
    let replay = ok(temp.path(), &args);
    assert_eq!(
        field(&bound, "binding_handle"),
        field(&replay, "binding_handle")
    );
    assert!(replay.contains("idempotent: true"));

    let changed = aw(
        temp.path(),
        &[
            "migration",
            "reviewer",
            "bind",
            &source,
            "--agent-label",
            "substituted-reviewer",
            "--external-agent-id",
            "agent:reviewer-7",
            "--provenance-ref",
            "review-output:retained-7",
            "--authority",
            authority,
        ],
    );
    assert!(!changed.status.success());
    assert!(
        String::from_utf8_lossy(&changed.stderr)
            .contains("reviewer_migration_source_already_bound")
    );

    let unknown = aw(
        temp.path(),
        &[
            "migration",
            "reviewer",
            "bind",
            &format!("legacy-reviewer:{}", "b".repeat(64)),
            "--agent-label",
            "unknown-reviewer",
            "--external-agent-id",
            "agent:unknown",
            "--provenance-ref",
            "review-output:unknown",
            "--authority",
            authority,
        ],
    );
    assert!(!unknown.status.success());
    assert!(
        String::from_utf8_lossy(&unknown.stderr).contains("reviewer_migration_source_not_found")
    );
}
