use super::*;

#[test]
fn public_plan_lifecycle_uses_one_current_slot_and_executable_actions() {
    let temp = tempfile::tempdir().unwrap();
    let (design, work, plan) = setup(temp.path(), "GATE-001");

    let imported = ok(
        temp.path(),
        &[
            "decomposition",
            "import",
            "--design-version",
            &design,
            "--work",
            &work,
            "--plan",
            &plan,
            "--draft",
            "--expected-current",
            "absent",
            "--idempotency-key",
            "lifecycle-import",
        ],
    );
    assert!(imported.contains("status: draft"));
    let draft_id = field(&imported, "plan").to_string();
    let draft_current = field(&imported, "current_identity").to_string();
    let import_replay = ok(
        temp.path(),
        &[
            "decomposition",
            "import",
            "--design-version",
            &design,
            "--work",
            &work,
            "--plan",
            &plan,
            "--draft",
            "--expected-current",
            "absent",
            "--idempotency-key",
            "lifecycle-import",
        ],
    );
    assert_eq!(field(&import_replay, "plan"), draft_id);
    assert_eq!(field(&import_replay, "idempotent"), "true");
    let changed_import_key = aw(
        temp.path(),
        &[
            "decomposition",
            "import",
            "--design-version",
            &design,
            "--work",
            &work,
            "--plan",
            &plan,
            "--draft",
            "--expected-current",
            "absent",
            "--idempotency-key",
            "lifecycle-import-other",
        ],
    );
    assert!(!changed_import_key.status.success());
    let after_changed_key = ok(
        temp.path(),
        &[
            "decomposition",
            "show",
            "--design-version",
            &design,
            "--work",
            &work,
        ],
    );
    assert_eq!(field(&after_changed_key, "plan"), draft_id);
    assert_eq!(field(&after_changed_key, "current_identity"), draft_current);

    let shown = ok(
        temp.path(),
        &[
            "decomposition",
            "show",
            "--design-version",
            &design,
            "--work",
            &work,
        ],
    );
    let validate_action = format!(
        "next: agent-workbench decomposition validate {draft_id} --expected-current {draft_current}"
    );
    let revise_action = format!(
        "next: agent-workbench decomposition revise {draft_id} --expected-current {draft_current}"
    );
    assert!(shown.contains(&validate_action));
    assert!(shown.contains(&revise_action));
    let status = ok(temp.path(), &["status"]);
    let next = ok(temp.path(), &["next"]);
    assert!(status.contains(&validate_action[6..]));
    assert!(next.contains(&validate_action[6..]));
    assert!(status.contains(&revise_action[6..]));
    assert!(next.contains(&revise_action[6..]));

    let stale = aw(
        temp.path(),
        &[
            "decomposition",
            "validate",
            &draft_id,
            "--expected-current",
            &"a".repeat(64),
            "--idempotency-key",
            "lifecycle-stale",
        ],
    );
    assert!(!stale.status.success());
    let unchanged = ok(
        temp.path(),
        &[
            "decomposition",
            "show",
            "--design-version",
            &design,
            "--work",
            &work,
        ],
    );
    assert_eq!(field(&unchanged, "plan"), draft_id);
    assert_eq!(field(&unchanged, "current_identity"), draft_current);

    let validated = ok(
        temp.path(),
        &[
            "decomposition",
            "validate",
            &draft_id,
            "--expected-current",
            &draft_current,
            "--idempotency-key",
            "lifecycle-validate",
        ],
    );
    assert!(validated.contains("status: ready"));
    assert!(validated.contains(&format!("predecessor: {draft_id}")));
    let ready_id = field(&validated, "plan").to_string();

    let replay = ok(
        temp.path(),
        &[
            "decomposition",
            "validate",
            &draft_id,
            "--expected-current",
            &draft_current,
            "--idempotency-key",
            "lifecycle-validate",
        ],
    );
    assert_eq!(field(&replay, "plan"), ready_id);
    assert_eq!(field(&replay, "idempotent"), "true");
    let changed_validate_key = aw(
        temp.path(),
        &[
            "decomposition",
            "validate",
            &draft_id,
            "--expected-current",
            &draft_current,
            "--idempotency-key",
            "lifecycle-validate-other",
        ],
    );
    assert!(!changed_validate_key.status.success());
    let changed_validate_error = String::from_utf8_lossy(&changed_validate_key.stderr);
    assert!(changed_validate_error.contains("retry payload differs"));
    assert!(changed_validate_error.contains(&format!("successor: {ready_id}")));
    assert!(changed_validate_error.contains("current:"));
    assert!(!changed_validate_error.contains("next: agent-workbench decomposition apply"));

    let ready = ok(
        temp.path(),
        &[
            "decomposition",
            "show",
            "--design-version",
            &design,
            "--work",
            &work,
        ],
    );
    assert!(ready.contains("status: ready"));
    assert!(ready.contains("review_owner_state: no_claim"));
    assert!(!ready.contains("next: agent-workbench decomposition apply"));
    let context_ref = field(&ready, "review_context").to_string();
    accept_exact_plan_review(temp.path(), &design, &work, &context_ref);
    let accepted = ok(
        temp.path(),
        &[
            "decomposition",
            "show",
            "--design-version",
            &design,
            "--work",
            &work,
        ],
    );
    assert!(accepted.contains("review_owner_state: accepted_clean"));
    assert!(accepted.contains(&format!(
        "next: agent-workbench decomposition apply {design} --work {work}"
    )));
    let applied = ok(
        temp.path(),
        &["decomposition", "apply", &design, "--work", &work],
    );
    assert!(applied.contains("already_applied: false"));
    let final_state = ok(
        temp.path(),
        &[
            "decomposition",
            "show",
            "--design-version",
            &design,
            "--work",
            &work,
        ],
    );
    assert_eq!(field(&final_state, "plan"), ready_id);
    assert!(final_state.contains(&format!(
        "next: agent-workbench gate implementation-ready --design-version {design}"
    )));
}
