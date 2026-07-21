use super::*;

#[test]
fn legacy_decompose_design_keeps_automatic_task_checklist_and_gate_generation() {
    let temp = tempfile::tempdir().unwrap();
    let (design, work, _) = setup(temp.path(), "GATE-001");
    let review_plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.parse().unwrap(),
            design_version_id: Some(design.parse().unwrap()),
            review_type: "design_review",
            required: true,
            stage: "design-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    let review_run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: review_plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some(&format!(
                "review-context:design-review:design={design}:work={work}"
            )),
            prompt_deviations: None,
            result_summary: Some("the imported design is ready for decomposition"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: Some("independent-reviewer"),
            external_agent_id: Some("independent-reviewer-1"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:legacy-decomposition"),
        },
    )
    .unwrap();
    adjudicate_review(
        temp.path(),
        review_run.review_run_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "accept the exact clean design claim",
            expected_current: "pending",
        },
    )
    .unwrap();

    let decomposed = ok(
        temp.path(),
        &[
            "decompose",
            "design",
            &design,
            "--work",
            &work,
            "--checklist-title",
            "Legacy automatic decomposition",
            "--reason",
            "preserve the established public outcome",
        ],
    );
    assert!(decomposed.contains("decomposed design"));
    assert!(
        ok(temp.path(), &["task", "list", "--work-unit", &work]).contains("Implement REQ-001:")
    );
    assert!(ok(temp.path(), &["checklist", "list"]).contains("Legacy automatic decomposition"));
    assert!(
        ok(temp.path(), &["checklist", "list", "--work", &work])
            .contains("Legacy automatic decomposition")
    );
    assert_eq!(
        ok(temp.path(), &["checklist", "list", "--work", "999"]),
        "classification: project-internal\nno checklists\n"
    );
    let items = ok(temp.path(), &["checklist", "item", "list", "1"]);
    assert!(items.contains("checklist=1"));
    assert!(!items.contains("no checklist items"));
    let derivation = ok(temp.path(), &["trace", "derivation", "list", "--task", "1"]);
    assert!(derivation.contains("REQ-001"));
    assert_eq!(
        ok(
            temp.path(),
            &["trace", "derivation", "list", "--task", "999"]
        ),
        "classification: project-internal\nno task derivations\n"
    );
    assert!(
        ok(
            temp.path(),
            &["trace", "derivation", "list", "--design-version", &design,],
        )
        .contains("REQ-001")
    );
    ok(
        temp.path(),
        &[
            "coverage",
            "add",
            "--design",
            &design,
            "--requirement",
            "REQ-001",
            "--task",
            "1",
            "--work-unit",
            &work,
            "--status",
            "covered",
            "--requirement-text",
            "public behavior",
            "--runtime",
            "observed through the public command",
            "--tests-or-gates",
            "legacy decomposition black-box test",
        ],
    );
    assert!(ok(temp.path(), &["coverage", "list", "--work", &work]).contains("REQ-001"));
    assert!(
        ok(
            temp.path(),
            &["coverage", "list", "--design-version", &design],
        )
        .contains("REQ-001")
    );
    assert_eq!(
        ok(temp.path(), &["coverage", "list", "--work", "999"]),
        "classification: project-internal\nno coverage items\n"
    );
    let readiness = aw(
        temp.path(),
        &[
            "gate",
            "implementation-ready",
            "--design-version",
            &design,
            "--dry-run",
        ],
    );
    assert!(String::from_utf8_lossy(&readiness.stdout).contains("validation_gates_selected: pass"));
}
