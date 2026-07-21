use super::*;

#[test]
fn interruption_after_remote_effect_reconciles_exactly_without_republishing() {
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
            "assets-after-effect",
        ],
        &[
            ("PATH", &path),
            ("FAKE_GH_STATE", &gh_state),
            ("FAKE_GH_FAIL_AFTER_CREATE", "1"),
        ],
    );
    assert!(!failed.status.success());
    let before = fs::read_dir(Path::new(&gh_state).join("assets"))
        .unwrap()
        .count();

    let status = ok_env(
        temp.path(),
        &["status"],
        &[("PATH", &path), ("FAKE_GH_STATE", &gh_state)],
    );
    assert!(status.contains(&format!("owner: release_candidate:{candidate}")));
    assert!(status.contains("owner_state: source_published_interrupted"));
    let owner = format!("release_candidate:{candidate}");
    let recovery = owner_field(&status, &owner, "owner_next").to_string();
    assert!(recovery.contains("agent-workbench operator release reconcile"));
    assert!(!recovery.contains("agent-workbench release inspect"));
    let next = ok_env(
        temp.path(),
        &["next"],
        &[("PATH", &path), ("FAKE_GH_STATE", &gh_state)],
    );
    assert_eq!(owner_field(&next, &owner, "owner_next"), recovery);

    let reconciled = execute_rendered(
        temp.path(),
        &recovery,
        &[("PATH", &path), ("FAKE_GH_STATE", &gh_state)],
    );
    assert!(reconciled.status.success());
    let reconciled = String::from_utf8(reconciled.stdout).unwrap();
    assert!(
        reconciled.contains("state: assets_published"),
        "{reconciled}"
    );
    assert_eq!(
        fs::read_dir(Path::new(&gh_state).join("assets"))
            .unwrap()
            .count(),
        before
    );
}
