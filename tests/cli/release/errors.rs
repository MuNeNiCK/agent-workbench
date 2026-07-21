use super::*;

#[test]
fn release_errors_return_only_current_public_actions() {
    let temp = tempfile::tempdir().unwrap();
    let commit = release_source(temp.path());
    let work = init_release_project(temp.path(), &commit);
    let assembled = ok(
        temp.path(),
        &[
            "operator",
            "release",
            "candidate",
            "assemble",
            "--work",
            &work.work_unit_id,
            "--version",
            "0.2.0",
            "--commit",
            &commit,
            "--expected-current",
            "absent",
            "--idempotency-key",
            "assemble-error-route",
        ],
    );
    let candidate = field(&assembled, "candidate");
    let assembled_revision = field(&assembled, "current_revision");
    let invalid_reconcile = aw(
        temp.path(),
        &[
            "operator",
            "release",
            "reconcile",
            candidate,
            "--expected-current",
            assembled_revision,
            "--idempotency-key",
            "invalid-reconcile",
        ],
    );
    assert!(!invalid_reconcile.status.success());
    let reconcile_error = String::from_utf8_lossy(&invalid_reconcile.stderr);
    assert!(reconcile_error.contains("next: agent-workbench operator release candidate inspect"));
    let after_reconcile = ok(temp.path(), &["status"]);
    assert!(after_reconcile.contains("owner_state: assembled"));
    assert!(!after_reconcile.contains("owner_state: assembled_interrupted"));

    let authority = ok(
        temp.path(),
        &[
            "authority",
            "event",
            "add",
            "--type",
            "user_instruction",
            "--summary",
            "Exercise invalid withdrawal routing",
            "--scope",
            "project",
        ],
    );
    let authority = field(&authority, "authority_event_id");
    let invalid_withdraw = aw(
        temp.path(),
        &[
            "operator",
            "release",
            "withdraw",
            candidate,
            "--expected-current",
            assembled_revision,
            "--idempotency-key",
            "invalid-withdraw",
            "--authority",
            authority,
            "--reason",
            "This state does not permit withdrawal",
        ],
    );
    assert!(!invalid_withdraw.status.success());
    let withdraw_error = String::from_utf8_lossy(&invalid_withdraw.stderr);
    assert!(withdraw_error.contains("next: agent-workbench operator release candidate inspect"));
    let after_withdraw = ok(temp.path(), &["next"]);
    assert!(after_withdraw.contains("owner_state: assembled"));
    assert!(!after_withdraw.contains("owner_state: assembled_interrupted"));

    let wrong_action = aw(
        temp.path(),
        &[
            "operator",
            "release",
            "publish-source",
            candidate,
            "--expected-current",
            assembled_revision,
            "--idempotency-key",
            "wrong-action",
        ],
    );
    assert!(!wrong_action.status.success());
    let error = String::from_utf8_lossy(&wrong_action.stderr);
    assert!(!error.contains("agent-workbench release inspect"));
    let recovery = error
        .lines()
        .find_map(|line| line.split_once("next: ").map(|(_, action)| action))
        .unwrap_or_else(|| panic!("state error must return its accepted public action: {error}"));
    let recovered = execute_rendered(temp.path(), recovery, &[]);
    assert!(
        recovered.status.success(),
        "{}",
        String::from_utf8_lossy(&recovered.stderr)
    );
    let recovered = String::from_utf8(recovered.stdout).unwrap();

    let stale = aw(
        temp.path(),
        &[
            "operator",
            "release",
            "candidate",
            "inspect",
            candidate,
            "--expected-current",
            assembled_revision,
            "--idempotency-key",
            "stale-route",
        ],
    );
    assert!(!stale.status.success());
    let stale_error = String::from_utf8_lossy(&stale.stderr);
    assert!(stale_error.contains("next: agent-workbench status"));
    assert!(!stale_error.contains("agent-workbench release inspect"));
    let stale_recovery = stale_error
        .lines()
        .find_map(|line| line.split_once("next: ").map(|(_, action)| action))
        .unwrap();
    assert!(
        execute_rendered(temp.path(), stale_recovery, &[])
            .status
            .success()
    );
    assert!(recovered.contains("state: locally_verified"));
}
