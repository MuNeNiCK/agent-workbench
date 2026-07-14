use super::super::*;

#[test]
fn implementation_ready_requires_completion_conditions() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    start_work(temp.path(), "implement storage lifecycle", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "implement cleanup",
            priority: "high",
            source: "design",
            work_unit_id: None,
            details: None,
            completion_condition: None,
        },
    )
    .unwrap();
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
    let import = import_design_package(
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
            design_version_id: import.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: import.design_version_id,
            requirement_key: "REQ-001",
            task_id: task.task_id,
            derivation_reason: None,
            checklist_title: None,
            item_title: None,
            completion_condition: None,
        },
    )
    .unwrap();
    select_validation_gate(
        temp.path(),
        ValidationGateSelection {
            design_version_id: import.design_version_id,
            gate_key: "GATE-001",
            requirement_key: "REQ-001",
            task_id: task.task_id,
            command: None,
            command_profile: None,
            timeout: None,
        },
    )
    .unwrap();

    let outcome = implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(import.design_version_id),
        },
    )
    .unwrap();

    assert_eq!(outcome.result, "blocked");
    assert!(
        outcome
            .items
            .iter()
            .any(|item| { item.name == "completion_conditions_present" && item.result == "fail" })
    );
}

#[test]
fn implementation_ready_marks_selected_gate_stale_when_template_changes() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    start_work(temp.path(), "implement storage lifecycle", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "implement cleanup",
            priority: "high",
            source: "design",
            work_unit_id: None,
            details: None,
            completion_condition: Some("cleanup behavior is covered"),
        },
    )
    .unwrap();
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
    let first_import = import_design_package(
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
            design_version_id: first_import.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: first_import.design_version_id,
            requirement_key: "REQ-001",
            task_id: task.task_id,
            derivation_reason: None,
            checklist_title: None,
            item_title: None,
            completion_condition: None,
        },
    )
    .unwrap();
    let gate = select_validation_gate(
        temp.path(),
        ValidationGateSelection {
            design_version_id: first_import.design_version_id,
            gate_key: "GATE-001",
            requirement_key: "REQ-001",
            task_id: task.task_id,
            command: None,
            command_profile: None,
            timeout: None,
        },
    )
    .unwrap();
    fs::write(
        init.package_path.join("validation").join("gates.md"),
        validation_gate_doc("GATE-001").replace(
            "Run the project test suite",
            "Run the full project test suite",
        ),
    )
    .unwrap();
    let second_import = import_design_package(
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
            design_version_id: second_import.design_version_id,
            summary: None,
        },
    )
    .unwrap();

    let outcome = implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(second_import.design_version_id),
        },
    )
    .unwrap();

    assert!(
        outcome
            .items
            .iter()
            .any(|item| { item.name == "validation_gates_current" && item.result == "fail" })
    );
    let stale = list_stale_records(temp.path()).unwrap();
    assert!(stale.iter().any(|record| {
        record.record_type == "validation_gate" && record.id == gate.validation_gate_id
    }));
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: 1,
            design_version_id: Some(first_import.design_version_id),
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
            target_ref: Some("work_unit:1"),
            prompt_deviations: None,
            result_summary: Some("found implementation drift"),
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
            review_run_id: run.review_run_id,
            finding_type: "design_implementation_drift",
            severity: "high",
            description: "fix after stale design",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    let blocked_classification = classify_finding(temp.path(), finding.finding_id, "valid");
    assert!(
        blocked_classification
            .unwrap_err()
            .to_string()
            .contains("stale accept")
    );

    let status = project_status(temp.path()).unwrap();
    assert!(status.phase_blocker.is_none());
    assert_eq!(
        status.owner_actions[0].blocker_kind.as_deref(),
        Some("stale_design")
    );
    assert!(status.finding_remediations.is_empty());
    assert!(matches!(
        next_action(temp.path()).unwrap(),
        NextAction::OwnerActions { ref owners }
            if owners.iter().any(|owner| owner.blocker_kind.as_deref() == Some("stale_design"))
    ));
}
