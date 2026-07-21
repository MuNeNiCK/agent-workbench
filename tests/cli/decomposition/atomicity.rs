use super::*;

#[test]
fn rejected_plan_application_publishes_no_partial_work() {
    let temp = tempfile::tempdir().unwrap();
    let (design, work, plan) = setup(temp.path(), "UNKNOWN-GATE");
    let imported = aw(
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
    assert!(imported.status.success());
    let imported_output = String::from_utf8(imported.stdout).unwrap();
    assert!(imported_output.contains("status: ready"));
    let context_ref = field(&imported_output, "review_context").to_string();
    accept_exact_plan_review(temp.path(), &design, &work, &context_ref);
    let rejected = aw(
        temp.path(),
        &["decomposition", "apply", &design, "--work", &work],
    );
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("gate coverage is not exact"));
    assert!(ok(temp.path(), &["task", "list", "--work-unit", &work]).contains("no tasks"));
    assert!(ok(temp.path(), &["phase", "list", "--work-unit", &work]).contains("no phases"));
    assert!(ok(temp.path(), &["checklist", "list"]).contains("no checklists"));
    let absent = aw(
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
    assert!(absent.status.success());
    let absent = String::from_utf8(absent.stdout).unwrap();
    assert!(absent.contains("status: ready"));
    assert!(absent.contains("review_owner_state: accepted_clean"));
}
