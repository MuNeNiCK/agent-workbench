use super::*;

#[test]
fn corrected_design_version_can_resolve_a_predecessor_design_finding() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "successor design correction", None).unwrap();
    suspend_work(
        temp.path(),
        "perform the design correction without an active implementation owner",
        "resume only after the correction is independently verified",
    )
    .unwrap();
    let package = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "successor-correction",
            title: "Successor Correction",
        },
    )
    .unwrap();
    fs::write(
        package.package_path.join("requirements/README.md"),
        requirement_doc("REQ-001", "Preserve the predecessor outcome", "high"),
    )
    .unwrap();
    fs::write(
        package.package_path.join("validation/gates.md"),
        validation_gate_doc("GATE-001"),
    )
    .unwrap();
    let predecessor = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &package.package_path,
            status: "draft",
        },
    )
    .unwrap();
    approve_design_version(
        temp.path(),
        DesignVersionApproval {
            design_version_id: predecessor.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    let source_plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(predecessor.design_version_id),
            review_type: "design_task_decomposition",
            required: true,
            stage: "implementation-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    let source_run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: source_plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("review-context:design-task-decomposition:design=1:work=1"),
            prompt_deviations: None,
            result_summary: Some("the predecessor decomposition is incomplete"),
            new_findings_count: 1,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: Some("predecessor-reviewer"),
            external_agent_id: Some("predecessor-reviewer"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review:predecessor"),
        },
    )
    .unwrap();
    let finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: source_run.review_run_id,
            finding_type: "design_task_gap",
            severity: "high",
            description: "publish a corrected successor decomposition",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), finding.finding_id, "valid").unwrap();

    let successor_requirement = requirement_doc(
        "REQ-001",
        "Preserve the predecessor outcome with an observable boundary",
        "high",
    )
    .replacen("revision: 1", "revision: 2", 1);
    fs::write(
        package.package_path.join("requirements/README.md"),
        successor_requirement,
    )
    .unwrap();
    let successor = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &package.package_path,
            status: "draft",
        },
    )
    .unwrap();
    approve_design_version(
        temp.path(),
        DesignVersionApproval {
            design_version_id: successor.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    let corrected_design_review = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(successor.design_version_id),
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
    add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: corrected_design_review.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some(&format!(
                "review-context:design-review:design={}:work={}",
                successor.design_version_id, work.work_unit_id
            )),
            prompt_deviations: None,
            result_summary: Some("successor design is ready"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: Some("successor-design-reviewer"),
            external_agent_id: Some("successor-design-reviewer"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review:successor-design"),
        },
    )
    .unwrap();
    let surfaces = format!(
        "transition:design-decompose:{}/{}",
        successor.design_version_id, work.work_unit_id
    );
    let closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: finding.finding_id,
            design_invariant: "a successor design can replace a deficient predecessor decomposition",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some(&surfaces),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("decompose the approved successor design"),
            tests_or_gates: Some("successor correction workflow"),
            verification_plan: Some("verify through an equivalent successor review plan"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    begin_correction(temp.path(), closure.closure_id).unwrap();
    let replacement_surfaces = format!(
        "transition:stale-accept:review_plan/{},{}",
        source_plan.review_plan_id, surfaces
    );
    let supersession = supersede_closure(
        temp.path(),
        ClosureSupersession {
            closure_id: closure.closure_id,
            new_closure: NewClosure {
                finding_id: finding.finding_id,
                design_invariant:
                    "only the current successor decomposition may authorize implementation",
                design_citations: None,
                implementation_evidence: None,
                affected_surfaces: Some(&replacement_surfaces),
                same_invariant_search: None,
                other_violations_found: None,
                fix_plan: Some("replace the active predecessor correction contract"),
                tests_or_gates: Some("successor correction workflow"),
                verification_plan: Some("verify the replacement correction independently"),
                closed_by_commit: None,
            },
            reason: "the active correction contract must be replaceable before its pending transition runs",
            authority_event_id: approval_authority_event(temp.path()),
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let retired: (String, String, String) = conn
        .query_row(
            "select c.status,s.status,t.status from closures c join correction_sessions s on s.closure_id=c.id join correction_tokens t on t.closure_id=c.id where c.id=?1",
            params![closure.closure_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        retired,
        (
            "superseded".into(),
            "superseded".into(),
            "superseded".into()
        )
    );
    drop(conn);
    let closure_id = supersession.closure_id;
    let selected = project_status(temp.path()).unwrap();
    assert!(selected.owner_actions.iter().any(|action| {
        action.next_action
            == format!("agent-workbench closure transition apply {closure_id} --token 1")
    }));
    apply_correction_transition(temp.path(), closure_id, 1, None, None).unwrap();
    apply_correction_transition(temp.path(), closure_id, 2, None, None).unwrap();
    let successor_plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(successor.design_version_id),
            review_type: "design_task_decomposition",
            required: true,
            stage: "implementation-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    supersede_review_plan(
        temp.path(),
        ReviewPlanSupersession {
            predecessor_plan_id: source_plan.review_plan_id,
            successor_plan_id: successor_plan.review_plan_id,
            authority_event_id: approval_authority_event(temp.path()),
            reason: "the approved successor design replaces the obsolete review scope",
        },
    )
    .unwrap();
    assert_eq!(
        list_review_plans(temp.path()).unwrap()[0].status,
        "superseded"
    );
    let attempt = ready_closure(
        temp.path(),
        ClosureReady {
            closure_id,
            implementation_evidence: "successor decomposition applied",
            tests_or_gates: "successor workflow passed",
            closed_by_commit: None,
        },
    )
    .unwrap();
    let verification = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: successor_plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some(&attempt.context_ref),
            prompt_deviations: None,
            result_summary: Some("successor design resolves the predecessor finding"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: Some("successor-reviewer"),
            external_agent_id: Some("successor-reviewer"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review:successor"),
        },
        Some("verified"),
    )
    .unwrap();
    add_finding_verification(
        temp.path(),
        NewFindingVerification {
            review_run_id: verification.review_run_id,
            finding_id: finding.finding_id,
            closure_id,
            result: "verified",
            notes: None,
        },
    )
    .unwrap();
    adjudicate_verification(
        temp.path(),
        verification.review_run_id,
        finding.finding_id,
        closure_id,
        attempt.attempt_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "accept the independently verified successor correction",
            expected_current: "pending",
        },
    )
    .unwrap();
    assert_eq!(
        list_findings(temp.path(), None).unwrap()[0].status,
        "closed"
    );
}
