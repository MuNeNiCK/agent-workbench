use super::*;

#[test]
fn interrupted_publication_reconciles_absence_then_retries_only_that_step() {
    let temp = tempfile::tempdir().unwrap();
    let (candidate, source_revision, published) = source_published(temp.path());
    assert!(published.contains("state: source_published"));
    let (path, gh_state) = fake_gh(temp.path());
    let failed = aw_env(
        temp.path(),
        &[
            "operator",
            "release",
            "publish-assets",
            &candidate,
            "--expected-current",
            &source_revision,
            "--idempotency-key",
            "assets-interrupted",
        ],
        &[
            ("PATH", &path),
            ("FAKE_GH_STATE", &gh_state),
            ("FAKE_GH_FAIL_BEFORE_CREATE", "1"),
        ],
    );
    assert!(!failed.status.success());

    let reconciled = ok_env(
        temp.path(),
        &[
            "operator",
            "release",
            "reconcile",
            &candidate,
            "--expected-current",
            &source_revision,
            "--idempotency-key",
            "assets-reconcile-absent",
        ],
        &[("PATH", &path), ("FAKE_GH_STATE", &gh_state)],
    );
    assert!(reconciled.contains("state: source_published"));
    assert!(reconciled.contains("next: agent-workbench operator release retry"));
    let reconciled_revision = field(&reconciled, "current_revision").to_string();

    let retried = ok_env(
        temp.path(),
        &[
            "operator",
            "release",
            "retry",
            &candidate,
            "--expected-current",
            &reconciled_revision,
            "--idempotency-key",
            "assets-retry",
        ],
        &[("PATH", &path), ("FAKE_GH_STATE", &gh_state)],
    );
    assert!(retried.contains("state: assets_published"), "{retried}");

    let exact_retry = ok_env(
        temp.path(),
        &[
            "operator",
            "release",
            "retry",
            &candidate,
            "--expected-current",
            &reconciled_revision,
            "--idempotency-key",
            "assets-retry",
        ],
        &[("PATH", &path), ("FAKE_GH_STATE", &gh_state)],
    );
    assert!(exact_retry.contains("already_applied: true"));
    assert_eq!(
        field(&exact_retry, "current_revision"),
        field(&retried, "current_revision")
    );

    let changed_action = aw_env(
        temp.path(),
        &[
            "operator",
            "release",
            "publish-assets",
            &candidate,
            "--expected-current",
            &reconciled_revision,
            "--idempotency-key",
            "assets-retry",
        ],
        &[("PATH", &path), ("FAKE_GH_STATE", &gh_state)],
    );
    assert!(!changed_action.status.success());
    assert!(
        String::from_utf8_lossy(&changed_action.stderr).contains("bound to a different operation")
    );
}
