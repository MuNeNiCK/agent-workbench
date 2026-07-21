use super::*;

#[test]
fn candidate_matrix_and_pathless_owned_revision_are_resolved_publicly() {
    let temp = tempfile::tempdir().unwrap();
    let (design, work, plan) = setup(temp.path(), "GATE-001");
    let one = ok(
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
    assert_eq!(
        one.matches("next: agent-workbench decomposition import")
            .count(),
        1
    );
    assert!(one.contains("--expected-content"));
    let expected_content = field(&one, "candidate_identity").to_string();
    let original = fs::read_to_string(temp.path().join(&plan)).unwrap();
    fs::write(temp.path().join(&plan), format!("{original}\n")).unwrap();
    let drifted = aw(
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
            "--expected-content",
            &expected_content,
            "--expected-current",
            "absent",
            "--idempotency-key",
            "stale-candidate",
        ],
    );
    assert!(!drifted.status.success());
    fs::write(temp.path().join(&plan), original).unwrap();

    let second = ".agent-workbench/designs/black-box-plan/plans/second.md";
    fs::copy(temp.path().join(&plan), temp.path().join(second)).unwrap();
    let many = ok(
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
    assert_eq!(
        many.matches("next: agent-workbench decomposition import")
            .count(),
        2
    );
    assert!(!many.contains("recommended"));

    fs::remove_file(temp.path().join(&plan)).unwrap();
    fs::remove_file(temp.path().join(second)).unwrap();
    let zero = ok(
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
    assert_eq!(
        zero.matches("next: agent-workbench decomposition import")
            .count(),
        1
    );
    assert!(!zero.contains("--plan"));

    let imported = ok(
        temp.path(),
        &[
            "decomposition",
            "import",
            "--design-version",
            &design,
            "--work",
            &work,
            "--expected-current",
            "absent",
            "--idempotency-key",
            "pathless-import",
        ],
    );
    assert!(imported.contains("status: draft"));
    let plan_id = field(&imported, "plan").to_string();
    let current = field(&imported, "current_identity").to_string();
    let content = field(&imported, "content_identity").to_string();
    let revised = ok(
        temp.path(),
        &[
            "decomposition",
            "revise",
            &plan_id,
            "--expected-current",
            &current,
            "--idempotency-key",
            "pathless-owned-revise",
        ],
    );
    assert!(revised.contains("status: incomplete"));
    assert_eq!(field(&revised, "content_identity"), content);
}
