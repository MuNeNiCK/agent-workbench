use super::*;

#[test]
fn work_lifecycle_commands_block_unblock_and_abandon() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(temp.path(), &["work", "start", "lifecycle"]);

    let blocked = ok(
        temp.path(),
        &["work", "block", "--reason", "waiting for decision"],
    );
    let unblocked = ok(
        temp.path(),
        &["work", "unblock", "1", "--reason", "decision recorded"],
    );
    let abandoned = ok(
        temp.path(),
        &["work", "abandon", "1", "--reason", "restart from fork"],
    );
    assert!(blocked.contains("blocked work unit"));
    assert!(blocked.contains("status: blocked"));
    assert!(unblocked.contains("unblocked work unit"));
    assert!(unblocked.contains("previous_status: blocked"));
    assert!(abandoned.contains("abandoned work unit"));
    assert!(abandoned.contains("status: abandoned"));
}

#[test]
fn gate_resume_ready_defaults_to_read_only_and_reports_blocked() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);

    let output = ok(temp.path(), &["gate", "resume-ready"]);
    assert!(output.contains("gate: resume-ready"));
    assert!(output.contains("dry_run: true"));
    assert!(output.contains("result: blocked"));
    assert!(output.contains("blocking_reason: no suspended activation to resume"));
}

#[test]
fn owner_qualified_status_next_readiness_and_close_never_select_a_sibling() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(temp.path(), &["work", "start", "first owner"]);
    ok(
        temp.path(),
        &[
            "work",
            "suspend",
            "--reason",
            "exercise explicit owner routing",
            "--next",
            "resume first owner",
        ],
    );
    ok(temp.path(), &["work", "start", "second owner"]);

    let first_status = ok(temp.path(), &["status", "--work", "1"]);
    assert!(first_status.contains("selected_work_unit_id: 1"));
    assert!(first_status.contains("owner: work_unit:1"));
    assert!(!first_status.contains("owner: work_unit:2"));
    assert!(first_status.contains("resume-check 1 --maturity trace-aware"));

    let first_next = ok(temp.path(), &["next", "--work", "1"]);
    assert!(first_next.contains("owner: work_unit:1"));
    assert!(!first_next.contains("owner: work_unit:2"));
    assert!(first_next.contains("resume-check 1 --maturity trace-aware"));

    let suspended_ready = ok(temp.path(), &["gate", "close-ready", "1", "--dry-run"]);
    assert!(suspended_ready.contains("work_unit_id: 1"));
    assert!(suspended_ready.contains("result: blocked"));
    assert!(suspended_ready.contains("agent-workbench resume-check 1 --maturity trace-aware"));

    let ambiguous = aw(temp.path(), &["work", "close", "--summary", "done"]);
    assert!(!ambiguous.status.success());
    let ambiguous = String::from_utf8_lossy(&ambiguous.stderr);
    assert!(ambiguous.contains("requires an explicit owner"));
    assert!(ambiguous.contains("agent-workbench work close 1 --summary 'done'"));
    assert!(ambiguous.contains("agent-workbench work close 2 --summary 'done'"));

    let rejected_suspended = aw(
        temp.path(),
        &["work", "close", "1", "--summary", "wrong owner"],
    );
    assert!(!rejected_suspended.status.success());
    assert!(
        String::from_utf8_lossy(&rejected_suspended.stderr)
            .contains("agent-workbench resume-check 1 --maturity trace-aware")
    );

    ok(
        temp.path(),
        &[
            "record",
            "create",
            "--topic",
            "complete",
            "--work-unit",
            "2",
        ],
    );
    ok(
        temp.path(),
        &[
            "repository",
            "add",
            "workspace",
            "--path",
            temp.path().to_str().unwrap(),
        ],
    );
    ok(
        temp.path(),
        &[
            "repository",
            "snapshot",
            "add",
            "--repository",
            "workspace",
            "--activation",
            "2",
            "--clean",
        ],
    );
    let second_ready = ok(temp.path(), &["gate", "close-ready", "2", "--dry-run"]);
    assert!(second_ready.contains("work_unit_id: 2"));
    assert!(second_ready.contains("result: pass"));
    let closed = ok(
        temp.path(),
        &["work", "close", "2", "--summary", "second complete"],
    );
    assert!(closed.contains("work_unit_id: 2"));
    assert!(closed.contains("activation_effect: completed"));

    let terminal = ok(temp.path(), &["next", "--work", "2"]);
    assert!(terminal.contains("selected work unit is terminal"));
    assert!(terminal.contains("work_unit_id: 2"));
    assert!(terminal.contains("status: closed"));
    let first_still_current = ok(temp.path(), &["status", "--work", "1"]);
    assert!(first_still_current.contains("open_work_units: 1"));
    assert!(first_still_current.contains("owner: work_unit:1"));
    assert!(!first_still_current.contains("owner: work_unit:2"));
}
