use super::*;

#[test]
fn partial_publication_conflicts_and_withdrawal_preserves_remote_assets() {
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
            "assets-partial",
        ],
        &[
            ("PATH", &path),
            ("FAKE_GH_STATE", &gh_state),
            ("FAKE_GH_PARTIAL_CREATE", "1"),
        ],
    );
    assert!(!failed.status.success());
    let assets_dir = Path::new(&gh_state).join("assets");
    let retained = fs::read_dir(&assets_dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let retained_bytes = fs::read(&retained).unwrap();

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
            "assets-reconcile-partial",
        ],
        &[("PATH", &path), ("FAKE_GH_STATE", &gh_state)],
    );
    assert!(reconciled.contains("state: asset_conflict"), "{reconciled}");
    assert_eq!(fs::read(&retained).unwrap(), retained_bytes);

    let authority = ok(
        temp.path(),
        &[
            "authority",
            "event",
            "add",
            "--type",
            "user_instruction",
            "--summary",
            "Withdraw the partial candidate without deleting remote history",
            "--scope",
            "project",
        ],
    );
    let authority = field(&authority, "authority_event_id").to_string();
    let conflict_revision = field(&reconciled, "current_revision").to_string();
    let withdrawn = ok_env(
        temp.path(),
        &[
            "operator",
            "release",
            "withdraw",
            &candidate,
            "--expected-current",
            &conflict_revision,
            "--idempotency-key",
            "withdraw-partial",
            "--authority",
            &authority,
            "--reason",
            "Partial remote publication cannot be completed safely",
        ],
        &[("PATH", &path), ("FAKE_GH_STATE", &gh_state)],
    );
    assert!(withdrawn.contains("state: withdrawn"), "{withdrawn}");
    assert_eq!(fs::read(&retained).unwrap(), retained_bytes);
    assert!(assets_dir.join("WITHDRAWN.txt").is_file());
}
