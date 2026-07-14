use super::*;

#[test]
fn mediated_design_decomposition_records_complete_owned_alias_graph() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "repair design decomposition", None).unwrap();
    let init = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "storage-lifecycle",
            title: "Storage Lifecycle",
        },
    )
    .unwrap();
    fs::write(
        init.package_path.join("requirements").join("README.md"),
        requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
    )
    .unwrap();
    fs::write(
        init.package_path.join("validation").join("gates.md"),
        validation_gate_doc("GATE-001"),
    )
    .unwrap();
    let imported = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &init.package_path,
            status: "draft",
        },
    )
    .unwrap();
    approve_design_version(
        temp.path(),
        DesignVersionApproval {
            design_version_id: imported.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    let ready_plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(imported.design_version_id),
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
            review_plan_id: ready_plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some(&format!(
                "review-context:design-review:design={}:work={}",
                imported.design_version_id, work.work_unit_id
            )),
            prompt_deviations: None,
            result_summary: Some("design is ready for mediated decomposition"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: Some("design-reviewer"),
            external_agent_id: Some("design-reviewer-1"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:design-ready"),
        },
    )
    .unwrap();
    let correction_plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(imported.design_version_id),
            review_type: "design_review",
            required: true,
            stage: "close-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    let correction_run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: correction_plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("work_unit:1"),
            prompt_deviations: None,
            result_summary: Some("decomposition is missing"),
            new_findings_count: 1,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
            review_provenance: "self_recorded",
            review_provenance_ref: None,
        },
    )
    .unwrap();
    let finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: correction_run.review_run_id,
            finding_type: "design_finding",
            severity: "high",
            description: "create the complete decomposition graph",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), finding.finding_id, "valid").unwrap();
    let surface = format!(
        "transition:design-decompose:{}/{},transition:phase-create:{}/{}/@implementation/implementation/1/implementation,transition:phase-assign:@implementation/@task/REQ-001",
        imported.design_version_id,
        work.work_unit_id,
        work.work_unit_id,
        imported.design_version_id
    );
    let closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: finding.finding_id,
            design_invariant: "all active requirements have an owned trace graph",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some(&surface),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("decompose the approved design"),
            tests_or_gates: Some("GATE-001"),
            verification_plan: Some("resume design review"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    assert!(
        decompose_design(
            temp.path(),
            DesignDecomposition {
                design_version_id: imported.design_version_id,
                work_unit_id: work.work_unit_id,
                checklist_title: None,
                reason: None,
            },
        )
        .unwrap_err()
        .to_string()
        .contains("closure correction-begin")
    );
    begin_correction(temp.path(), closure.closure_id).unwrap();
    assert!(
        decompose_design(
            temp.path(),
            DesignDecomposition {
                design_version_id: imported.design_version_id,
                work_unit_id: work.work_unit_id,
                checklist_title: None,
                reason: None,
            },
        )
        .unwrap_err()
        .to_string()
        .contains("closure transition apply")
    );
    apply_correction_transition(temp.path(), closure.closure_id, 1, None, None).unwrap();
    apply_correction_transition(temp.path(), closure.closure_id, 2, None, None).unwrap();
    apply_correction_transition(temp.path(), closure.closure_id, 3, None, None).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let aliases: Vec<String> = {
        let mut stmt = conn
            .prepare("select alias from correction_transition_aliases order by alias")
            .unwrap();
        stmt.query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };
    assert!(aliases.contains(&"@checklist".to_string()));
    assert!(aliases.contains(&"@task/REQ-001".to_string()));
    assert!(aliases.contains(&"@derivation/REQ-001".to_string()));
    assert!(aliases.contains(&"@checklist-item/REQ-001".to_string()));
    assert!(aliases.contains(&"@coverage/REQ-001".to_string()));
    assert!(aliases.contains(&"@gate/REQ-001/GATE-001".to_string()));
}
