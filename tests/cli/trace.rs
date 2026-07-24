use super::*;
use agent_workbench::{
    AdjudicationInput, DesignPackageImport, DesignVersionApproval, NewClosure, NewDesignPackage,
    NewReviewPlan, NewReviewRun, NewTask, NewTaskDerivation, add_closure, add_review_plan,
    add_review_run, add_task, approve_design_version, decide_finding, default_ledger_path,
    derive_task_from_requirement, import_design_package, init_design_package, init_project,
    remediate_work, start_work,
};

#[test]
fn public_cli_rebinds_a_completed_derivation_under_its_closure() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "public completed trace repair", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "implement aggregate behavior",
            priority: "high",
            source: "design",
            work_unit_id: None,
            details: Some("one aggregate task"),
            completion_condition: Some("all completion boundaries hold"),
        },
    )
    .unwrap();
    let package = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "public-completed-rebind",
            title: "Public Completed Rebind",
        },
    )
    .unwrap();
    std::fs::write(
        package.package_path.join("requirements/README.md"),
        r#"## REQ-001: First boundary
```yaml agent-workbench
type: requirement
key: REQ-001
priority: high
surfaces: [cli]
validation: [GATE-001]
status: active
```
First completion boundary.

## REQ-002: Second boundary
```yaml agent-workbench
type: requirement
key: REQ-002
priority: high
surfaces: [cli]
validation: [GATE-001]
status: active
```
Second completion boundary.
"#,
    )
    .unwrap();
    std::fs::write(
        package.package_path.join("validation/gates.md"),
        r#"## GATE-001: Observe aggregate behavior
```yaml agent-workbench
type: validation_gate_template
key: GATE-001
applies_to: [REQ-001, REQ-002]
expected_result: pass
phase: implementation
status: active
```
Observe the aggregate behavior.
"#,
    )
    .unwrap();
    let design = import_design_package(
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
            design_version_id: design.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    let requirement_inventory = ok(
        temp.path(),
        &[
            "requirement",
            "list",
            "--design",
            &design.design_version_id.to_string(),
        ],
    );
    let requirement_id = |key: &str| {
        requirement_inventory
            .lines()
            .find(|line| line.starts_with(&format!("{key} [id=")))
            .and_then(|line| line.split_once("[id=").map(|(_, suffix)| suffix))
            .and_then(|suffix| suffix.split_whitespace().next())
            .and_then(|id| id.parse::<i64>().ok())
            .unwrap_or_else(|| panic!("requirement list did not expose the id for {key}"))
    };
    let first_requirement_id = requirement_id("REQ-001");
    let second_requirement_id = requirement_id("REQ-002");
    let first = derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: design.design_version_id,
            requirement_key: "REQ-001",
            task_id: task.task_id,
            derivation_reason: Some("initial decomposition"),
            checklist_title: Some("aggregate"),
            item_title: Some("first-boundary"),
            completion_condition: Some("first boundary holds"),
        },
    )
    .unwrap();
    let second = derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: design.design_version_id,
            requirement_key: "REQ-002",
            task_id: task.task_id,
            derivation_reason: Some("initial decomposition"),
            checklist_title: Some("aggregate"),
            item_title: Some("second-boundary"),
            completion_condition: Some("second boundary holds"),
        },
    )
    .unwrap();
    let conn = rusqlite::Connection::open(default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update checklist_items set status='closed' where task_id=?1",
        [task.task_id],
    )
    .unwrap();
    conn.execute(
        "update tasks set status='closed' where id=?1",
        [task.task_id],
    )
    .unwrap();
    drop(conn);
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(design.design_version_id),
            review_type: "design_implementation_diff",
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
    let run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some(&format!(
                "review-context:design-implementation-diff:design={}:work={}",
                design.design_version_id, work.work_unit_id
            )),
            prompt_deviations: None,
            result_summary: Some("the completed derivation targets the wrong boundary"),
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
    let added_finding = ok(
        temp.path(),
        &[
            "finding",
            "add",
            "--run",
            &run.review_run_id.to_string(),
            "--type",
            "design_implementation_drift",
            "--severity",
            "high",
            "--description",
            "rebind the completed derivation",
            "--design-requirement",
            &first_requirement_id.to_string(),
            "--task",
            &task.task_id.to_string(),
            "--design-requirement",
            &second_requirement_id.to_string(),
            "--task",
            &task.task_id.to_string(),
        ],
    );
    let finding_id = added_finding
        .lines()
        .find_map(|line| line.strip_prefix("finding_id: "))
        .unwrap()
        .parse::<i64>()
        .unwrap();
    let accepted = decide_finding(
        temp.path(),
        finding_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "accept the exact requirement and task target",
            expected_current: "pending",
        },
    )
    .unwrap();
    let target_inventory = ok(temp.path(), &["finding", "list", "--status", "open"]);
    assert!(target_inventory.contains(&format!(
        "targets: requirement={},task={};requirement={},task={}",
        first_requirement_id, task.task_id, second_requirement_id, task.task_id
    )));
    let closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id,
            design_invariant: "each requirement names its establishing boundary",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some("managed trace derivation"),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("rebind only the selected derivation"),
            tests_or_gates: Some("exact derivation list"),
            verification_plan: Some("independent trace review"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    remediate_work(temp.path(), finding_id).unwrap();

    let design_id = design.design_version_id.to_string();
    let task_id = task.task_id.to_string();
    let item_id = first.checklist_item_id.to_string();
    let closure_id = closure.closure_id.to_string();
    let args = [
        "trace",
        "derivation",
        "rebind",
        "--design",
        &design_id,
        "--requirement",
        "REQ-002",
        "--task",
        &task_id,
        "--checklist-item",
        &item_id,
        "--closure",
        &closure_id,
        "--reason",
        "bind the second requirement to its establishing shared boundary",
    ];
    let rebound = ok(temp.path(), &args);
    assert!(rebound.contains(&format!(
        "task_derivation_id: {}",
        second.task_derivation_id
    )));
    assert!(rebound.contains("idempotent: false"));
    assert!(ok(temp.path(), &args).contains("idempotent: true"));

    let finding_id_text = finding_id.to_string();
    let rejected = aw(
        temp.path(),
        &[
            "finding",
            "decide",
            &finding_id_text,
            "--decision",
            "rejected",
            "--reason",
            "must not discard an applied public rebind",
            "--expected-current",
            &accepted.decision_handle,
        ],
    );
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr)
            .contains("finding_has_active_remediation_effects")
    );
    let listed = ok(temp.path(), &["finding", "list", "--status", "open"]);
    assert!(listed.contains(&format!("{} [run={} ", finding_id, run.review_run_id)));
    assert!(listed.contains(&format!(
        "current_decision_handle: {}",
        accepted.decision_handle
    )));
}
