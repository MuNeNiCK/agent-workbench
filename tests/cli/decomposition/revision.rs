use super::*;

#[test]
fn incomplete_plan_revises_to_ready_without_losing_terminal_history() {
    let temp = tempfile::tempdir().unwrap();
    let (design, work, valid_plan) = setup(temp.path(), "GATE-001");
    let identity = field(
        &ok(temp.path(), &["design", "inspect", &design]),
        "design_identity",
    )
    .to_string();
    let incomplete_plan = ".agent-workbench/designs/black-box-plan/plans/incomplete-plan.md";
    fs::write(
        temp.path().join(incomplete_plan),
        format!(
            r#"# Incomplete plan

```yaml agent-workbench
type: decomposition_plan
format: 1
key: incomplete-plan
design_fingerprint: {identity}
items: []
slices: []
```
"#
        ),
    )
    .unwrap();
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
            incomplete_plan,
            "--expected-current",
            "absent",
            "--idempotency-key",
            "incomplete-import",
        ],
    );
    assert!(imported.contains("status: incomplete"));
    assert!(imported.contains("issue: decomposition plan requires items and slices"));
    let imported_id = field(&imported, "plan").to_string();
    let imported_current = field(&imported, "current_identity").to_string();

    let validated = ok(
        temp.path(),
        &[
            "decomposition",
            "validate",
            &imported_id,
            "--expected-current",
            &imported_current,
            "--idempotency-key",
            "incomplete-validate",
        ],
    );
    assert!(validated.contains("status: incomplete"));
    assert!(validated.contains(&format!("predecessor: {imported_id}")));
    let incomplete_id = field(&validated, "plan").to_string();
    let incomplete_current = field(&validated, "current_identity").to_string();

    let revised = ok(
        temp.path(),
        &[
            "decomposition",
            "revise",
            &incomplete_id,
            "--plan",
            &valid_plan,
            "--expected-current",
            &incomplete_current,
            "--idempotency-key",
            "incomplete-revise",
        ],
    );
    assert!(revised.contains("status: ready"));
    assert!(revised.contains(&format!("predecessor: {incomplete_id}")));
    assert_eq!(field(&revised, "revision"), "3");
    let ready_id = field(&revised, "plan").to_string();
    let ready_current = field(&revised, "current_identity").to_string();

    let revised_ready = ok(
        temp.path(),
        &[
            "decomposition",
            "revise",
            &ready_id,
            "--plan",
            &valid_plan,
            "--expected-current",
            &ready_current,
            "--idempotency-key",
            "ready-revise",
        ],
    );
    assert!(revised_ready.contains("status: ready"));
    assert!(revised_ready.contains(&format!("predecessor: {ready_id}")));
    assert_eq!(field(&revised_ready, "revision"), "4");
    let corrected_ready_id = field(&revised_ready, "plan").to_string();

    accept_exact_plan_review(
        temp.path(),
        &design,
        &work,
        field(&revised_ready, "review_context"),
    );
    let applied = ok(
        temp.path(),
        &["decomposition", "apply", &design, "--work", &work],
    );
    assert_eq!(field(&applied, "plan"), corrected_ready_id);
    assert!(applied.contains("tasks: 1"));
    assert!(applied.contains("status: applied"));

    let historical_retry = ok(
        temp.path(),
        &[
            "decomposition",
            "validate",
            &imported_id,
            "--expected-current",
            &imported_current,
            "--idempotency-key",
            "incomplete-validate",
        ],
    );
    assert_eq!(field(&historical_retry, "plan"), incomplete_id);
    assert_eq!(field(&historical_retry, "idempotent"), "true");
}
