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
