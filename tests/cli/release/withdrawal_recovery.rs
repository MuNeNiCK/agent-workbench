use super::*;

#[test]
fn withdrawal_recovery_observes_absent_and_exact_notice_effects() {
    let before_effect = tempfile::tempdir().unwrap();
    let (candidate, revision, published) = source_published(before_effect.path());
    assert!(published.contains("state: source_published"));
    let (path, gh_state) = fake_gh(before_effect.path());
    let authority = ok(
        before_effect.path(),
        &[
            "authority",
            "event",
            "add",
            "--type",
            "user_instruction",
            "--summary",
            "Withdraw only after observing the exact notice",
            "--scope",
            "project",
        ],
    );
    let authority = field(&authority, "authority_event_id").to_string();
    let withdraw_args = [
        "operator",
        "release",
        "withdraw",
        &candidate,
        "--expected-current",
        &revision,
        "--idempotency-key",
        "withdraw-before-effect",
        "--authority",
        &authority,
        "--reason",
        "The candidate must not remain published",
    ];
    let failed = aw_env(
        before_effect.path(),
        &withdraw_args,
        &[
            ("PATH", &path),
            ("FAKE_GH_STATE", &gh_state),
            ("FAKE_GH_FAIL_BEFORE_CREATE", "1"),
        ],
    );
    assert!(!failed.status.success());
    assert!(!Path::new(&gh_state).join("release.json").exists());

    let status = ok(before_effect.path(), &["status"]);
    assert!(status.contains("owner_state: source_published_interrupted"));
    let owner = format!("release_candidate:{candidate}");
    let reconcile = owner_field(&status, &owner, "owner_next").to_string();
    assert!(reconcile.contains(" operator release reconcile "));
    let observed_absent = execute_rendered(
        before_effect.path(),
        &reconcile,
        &[("PATH", &path), ("FAKE_GH_STATE", &gh_state)],
    );
    assert!(observed_absent.status.success());
    let observed_absent = String::from_utf8(observed_absent.stdout).unwrap();
    assert!(observed_absent.contains("state: source_published"));
    assert!(observed_absent.contains("next: agent-workbench operator release retry"));
    assert!(!Path::new(&gh_state).join("release.json").exists());

    let retry =
        owner_field(&ok(before_effect.path(), &["status"]), &owner, "owner_next").to_string();
    assert!(retry.contains(" operator release retry "));
    let completed = execute_rendered(
        before_effect.path(),
        &retry,
        &[("PATH", &path), ("FAKE_GH_STATE", &gh_state)],
    );
    assert!(
        completed.status.success(),
        "{}",
        String::from_utf8_lossy(&completed.stderr)
    );
    assert!(String::from_utf8_lossy(&completed.stdout).contains("state: withdrawn"));
    assert!(Path::new(&gh_state).join("assets/WITHDRAWN.txt").is_file());

    let after_effect = tempfile::tempdir().unwrap();
    let (candidate, revision, published) = source_published(after_effect.path());
    assert!(published.contains("state: source_published"));
    let (path, gh_state) = fake_gh(after_effect.path());
    let authority = ok(
        after_effect.path(),
        &[
            "authority",
            "event",
            "add",
            "--type",
            "user_instruction",
            "--summary",
            "Recover an observed withdrawal",
            "--scope",
            "project",
        ],
    );
    let authority = field(&authority, "authority_event_id").to_string();
    let withdraw_args = [
        "operator",
        "release",
        "withdraw",
        &candidate,
        "--expected-current",
        &revision,
        "--idempotency-key",
        "withdraw-after-effect",
        "--authority",
        &authority,
        "--reason",
        "Observe the existing notice without republishing",
    ];
    let failed = aw_env(
        after_effect.path(),
        &withdraw_args,
        &[
            ("PATH", &path),
            ("FAKE_GH_STATE", &gh_state),
            ("FAKE_GH_FAIL_AFTER_CREATE", "1"),
        ],
    );
    assert!(!failed.status.success());
    let notice = Path::new(&gh_state).join("assets/WITHDRAWN.txt");
    let observed_bytes = fs::read(&notice).unwrap();
    let completed = ok_env(
        after_effect.path(),
        &withdraw_args,
        &[("PATH", &path), ("FAKE_GH_STATE", &gh_state)],
    );
    assert!(completed.contains("state: withdrawn"));
    assert!(completed.contains("already_applied: false"));
    assert_eq!(fs::read(notice).unwrap(), observed_bytes);

    let conflicting_effect = tempfile::tempdir().unwrap();
    let (candidate, revision, published) = source_published(conflicting_effect.path());
    assert!(published.contains("state: source_published"));
    let (path, gh_state) = fake_gh(conflicting_effect.path());
    let authority = ok(
        conflicting_effect.path(),
        &[
            "authority",
            "event",
            "add",
            "--type",
            "user_instruction",
            "--summary",
            "Block withdrawal until the exact external notice is observed",
            "--scope",
            "project",
        ],
    );
    let authority = field(&authority, "authority_event_id").to_string();
    let withdraw_args = [
        "operator",
        "release",
        "withdraw",
        &candidate,
        "--expected-current",
        &revision,
        "--idempotency-key",
        "withdraw-conflicting-effect",
        "--authority",
        &authority,
        "--reason",
        "Only the requested withdrawal notice may complete the transition",
    ];
    let failed = aw_env(
        conflicting_effect.path(),
        &withdraw_args,
        &[
            ("PATH", &path),
            ("FAKE_GH_STATE", &gh_state),
            ("FAKE_GH_FAIL_AFTER_CREATE", "1"),
        ],
    );
    assert!(!failed.status.success());
    let remote_notice = Path::new(&gh_state).join("assets/WITHDRAWN.txt");
    fs::write(&remote_notice, "different withdrawal notice\n").unwrap();

    let owner = format!("release_candidate:{candidate}");
    let first_reconcile = owner_field(
        &ok(conflicting_effect.path(), &["status"]),
        &owner,
        "owner_next",
    )
    .to_string();
    let conflict = execute_rendered(
        conflicting_effect.path(),
        &first_reconcile,
        &[("PATH", &path), ("FAKE_GH_STATE", &gh_state)],
    );
    assert!(conflict.status.success());
    assert!(String::from_utf8_lossy(&conflict.stdout).contains("state: withdrawal_conflict"));
    let status = ok(conflicting_effect.path(), &["status"]);
    assert!(status.contains("owner_state: withdrawal_conflict"));
    let second_reconcile = owner_field(&status, &owner, "owner_next").to_string();
    assert!(second_reconcile.contains(" operator release reconcile "));
    let repeated = execute_rendered(
        conflicting_effect.path(),
        &second_reconcile,
        &[("PATH", &path), ("FAKE_GH_STATE", &gh_state)],
    );
    assert!(repeated.status.success());
    assert!(String::from_utf8_lossy(&repeated.stdout).contains("state: withdrawal_conflict"));

    let local_notice = conflicting_effect
        .path()
        .join(".agent-workbench/release-candidates")
        .join(&candidate)
        .join("WITHDRAWN.txt");
    fs::copy(local_notice, &remote_notice).unwrap();
    let final_reconcile = owner_field(
        &ok(conflicting_effect.path(), &["status"]),
        &owner,
        "owner_next",
    )
    .to_string();
    let withdrawn = execute_rendered(
        conflicting_effect.path(),
        &final_reconcile,
        &[("PATH", &path), ("FAKE_GH_STATE", &gh_state)],
    );
    assert!(
        withdrawn.status.success(),
        "{}",
        String::from_utf8_lossy(&withdrawn.stderr)
    );
    assert!(String::from_utf8_lossy(&withdrawn.stdout).contains("state: withdrawn"));
}
