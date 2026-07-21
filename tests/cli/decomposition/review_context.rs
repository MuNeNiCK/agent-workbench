use super::*;

#[test]
fn review_context_reports_the_public_update_action_before_resolving_a_plan() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let ledger = temp.path().join(".agent-workbench/ledger.sqlite");
    let conn = Connection::open(ledger).unwrap();
    conn.execute_batch("drop table decomposition_reconciliation_dependencies")
        .unwrap();
    drop(conn);

    let output = aw(
        temp.path(),
        &[
            "review-context",
            "design-task-decomposition",
            "--design-version",
            "1",
            "--work-unit",
            "1",
        ],
    );
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("agent-workbench update inspect"), "{error}");
    assert!(!error.to_lowercase().contains("no such"), "{error}");
}
