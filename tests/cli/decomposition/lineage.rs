use super::*;

#[test]
fn show_and_apply_share_the_package_lineage_current_slot() {
    let temp = tempfile::tempdir().unwrap();
    let (design, work, plan) = setup(temp.path(), "GATE-001");
    let imported = ok(
        temp.path(),
        &[
            "decomposition",
            "apply",
            &design,
            "--work",
            &work,
            "--plan",
            &plan,
        ],
    );
    let plan_id = field(&imported, "plan").to_string();
    let context_ref = field(&imported, "review_context").to_string();
    accept_exact_plan_review(temp.path(), &design, &work, &context_ref);
    let applied = ok(
        temp.path(),
        &["decomposition", "apply", &design, "--work", &work],
    );
    assert!(applied.contains("status: applied"));

    let requirement = temp
        .path()
        .join(".agent-workbench/designs/black-box-plan/requirements/README.md");
    let changed = fs::read_to_string(&requirement)
        .unwrap()
        .replace("key: REQ-001\n", "key: REQ-001\nrevision: 2\n")
        .replace(
            "The public behavior remains observable.",
            "The public behavior remains observable through the successor design.",
        );
    fs::write(&requirement, changed).unwrap();
    let refreshed = ok(
        temp.path(),
        &[
            "design",
            "refresh",
            ".agent-workbench/designs/black-box-plan",
        ],
    );
    let successor = field(&refreshed, "design_version_id").to_string();
    ok(temp.path(), &["design", "approve", &successor]);

    let shown = ok(
        temp.path(),
        &[
            "decomposition",
            "show",
            "--design-version",
            &successor,
            "--work",
            &work,
        ],
    );
    assert_eq!(field(&shown, "plan"), plan_id);
    assert_eq!(field(&shown, "design_version"), design);
    assert!(shown.contains("status: applied"));
    assert!(shown.contains("review_owner_state: fresh_review_required"));

    let replay = ok(
        temp.path(),
        &["decomposition", "apply", &successor, "--work", &work],
    );
    assert_eq!(field(&replay, "plan"), plan_id);
    assert_eq!(field(&replay, "already_applied"), "true");
    assert!(replay.contains("review_owner_state: fresh_review_required"));
}
