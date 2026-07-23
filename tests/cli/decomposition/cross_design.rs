use super::*;
use agent_workbench::{
    DesignPackageImport, DesignVersionApproval, NewDesignPackage, add_review_plan, add_review_run,
    approve_design_version, import_design_package, init_design_package,
};

#[test]
fn public_cli_resolves_generated_checklist_across_design_versions() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let work = ok(temp.path(), &["work", "start", "cross-design correction"]);
    let work_id = field(&work, "work_unit_id").parse::<i64>().unwrap();
    let package = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "cross-design-correction",
            title: "Cross Design Correction",
        },
    )
    .unwrap();
    fs::write(
        package.package_path.join("requirements/README.md"),
        r#"## REQ-001: Preserve behavior
```yaml agent-workbench
type: requirement
key: REQ-001
revision: 1
priority: high
surfaces: [cli]
validation: [GATE-001]
status: active
```

Preserve the observable behavior.
"#,
    )
    .unwrap();
    fs::write(
        package.package_path.join("validation/gates.md"),
        r#"## GATE-001: Observe behavior
```yaml agent-workbench
type: validation_gate_template
key: GATE-001
applies_to: [REQ-001]
expected_result: pass
phase: implementation
status: active
```

Observe the public behavior.
"#,
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
            work_unit_id: work_id,
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
            target_ref: Some(&format!(
                "review-context:design-task-decomposition:design={}:work={work_id}",
                predecessor.design_version_id
            )),
            prompt_deviations: None,
            result_summary: Some("a successor design must own the corrected decomposition"),
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
            description: "publish the successor decomposition and reconcile it",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), finding.finding_id, "valid").unwrap();

    let successor_requirement =
        fs::read_to_string(package.package_path.join("requirements/README.md"))
            .unwrap()
            .replace("revision: 1", "revision: 2")
            .replace(
                "Preserve the observable behavior.",
                "Preserve the observable behavior through the successor.",
            );
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
    let successor_review = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work_id,
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
    let successor_run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: successor_review.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some(&format!(
                "review-context:design-review:design={}:work={work_id}",
                successor.design_version_id
            )),
            prompt_deviations: None,
            result_summary: Some("the successor design is ready"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: Some("successor-reviewer"),
            external_agent_id: Some("successor-reviewer"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review:successor"),
        },
    )
    .unwrap();
    adjudicate_review(
        temp.path(),
        successor_run.review_run_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "accept the clean successor design review",
            expected_current: "pending",
        },
    )
    .unwrap();
    let surfaces = format!(
        "transition:design-decompose:{}/{work_id},transition:design-reconcile:{}/{work_id}/@checklist",
        successor.design_version_id, successor.design_version_id
    );
    let finding_id = finding.finding_id.to_string();
    let closure_output = ok(
        temp.path(),
        &[
            "closure",
            "add",
            "--finding",
            &finding_id,
            "--invariant",
            "the approved successor owns its generated canonical checklist",
            "--surfaces",
            &surfaces,
            "--fix-plan",
            "decompose and reconcile the approved successor",
            "--tests",
            "public correction transition lifecycle",
            "--verification",
            "inspect the exact successor trace",
        ],
    );
    let closure_id = field(&closure_output, "closure_id").to_string();
    let status_before = ok(temp.path(), &["status", "--work", &work_id.to_string()]);
    assert!(
        status_before.contains(&format!(
            "agent-workbench closure correction-begin {closure_id}"
        )),
        "unexpected status before correction: {status_before}"
    );
    let next_before = ok(temp.path(), &["next", "--work", &work_id.to_string()]);
    assert!(
        next_before.contains(&format!(
            "agent-workbench closure correction-begin {closure_id}"
        )),
        "unexpected next before correction: {next_before}"
    );
    let begun = ok(temp.path(), &["closure", "correction-begin", &closure_id]);
    assert!(begun.contains("token_count: 2"));
    let status_after = ok(temp.path(), &["status", "--work", &work_id.to_string()]);
    assert!(
        status_after.contains(&format!(
            "agent-workbench closure transition apply {closure_id} --token 1"
        )),
        "unexpected status after correction begin: {status_after}"
    );
    let next_after = ok(temp.path(), &["next", "--work", &work_id.to_string()]);
    assert!(
        next_after.contains(&format!(
            "agent-workbench closure transition apply {closure_id} --token 1"
        )),
        "unexpected next after correction begin: {next_after}"
    );
    let decomposed = ok(
        temp.path(),
        &[
            "closure",
            "transition",
            "apply",
            &closure_id,
            "--token",
            "1",
        ],
    );
    assert!(decomposed.contains("result_ref: checklist:"));
    let reconciled = ok(
        temp.path(),
        &[
            "closure",
            "transition",
            "apply",
            &closure_id,
            "--token",
            "2",
        ],
    );
    assert!(reconciled.contains("result_ref: checklist:"));
}
