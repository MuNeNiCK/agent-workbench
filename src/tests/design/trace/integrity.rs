use super::super::*;

#[test]
fn implementation_ready_rejects_requirement_coverage_from_another_work_unit() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work_one = start_work(temp.path(), "implement cleanup", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "cleanup task",
            priority: "high",
            source: "design",
            work_unit_id: None,
            details: None,
            completion_condition: Some("cleanup covered"),
        },
    )
    .unwrap();
    let work_two = interrupt_work(temp.path(), "unrelated implementation", "parallel work")
        .unwrap()
        .child_work_unit_id;
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
            command: Some("cargo test"),
            command_profile: None,
            timeout: None,
        },
    )
    .unwrap();
    add_clean_implementation_ready_review(
        temp.path(),
        work_one.work_unit_id,
        import.design_version_id,
    );
    add_implementation_evidence(
        temp.path(),
        NewImplementationEvidence {
            task_id: Some(task.task_id),
            design_version_id: Some(import.design_version_id),
            requirement_key: Some("REQ-001"),
            evidence_type: "commit",
            commit_sha: Some("abc123"),
            file_path: None,
            line_ref: None,
            symbol: None,
            artifact_path: None,
            note: None,
        },
    )
    .unwrap();
    add_coverage_item(
        temp.path(),
        NewCoverageItem {
            design_version_id: import.design_version_id,
            requirement_key: "REQ-001",
            review_scope_id: None,
            work_unit_id: Some(work_two),
            task_id: None,
            requirement: "cleanup behavior is covered elsewhere",
            runtime_boundary_evidence: Some("other work runtime path"),
            ux_boundary_evidence: None,
            lifecycle_boundary_evidence: None,
            tests_or_gates: Some("other gate"),
            missing_or_unverified: None,
            status: "covered",
        },
    )
    .unwrap();
    close_task(temp.path(), task.task_id, Some("abc123")).unwrap_err();
    crate::db::open_existing_project(temp.path())
        .unwrap()
        .execute(
            "update tasks set status = 'closed' where id = ?1",
            rusqlite::params![task.task_id],
        )
        .unwrap();

    let blocked = implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(import.design_version_id),
        },
    )
    .unwrap();

    assert!(
        blocked
            .items
            .iter()
            .any(|item| { item.name == "coverage_items_present" && item.result == "fail" })
    );
}

#[test]
fn task_close_rejects_requirement_coverage_from_another_work_unit() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work_one = start_work(temp.path(), "implement cleanup", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "cleanup task",
            priority: "high",
            source: "design",
            work_unit_id: None,
            details: None,
            completion_condition: Some("cleanup covered"),
        },
    )
    .unwrap();
    let work_two =
        interrupt_work(temp.path(), "unrelated implementation", "parallel work").unwrap();
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
            command: Some("cargo test"),
            command_profile: None,
            timeout: None,
        },
    )
    .unwrap();
    add_implementation_evidence(
        temp.path(),
        NewImplementationEvidence {
            task_id: Some(task.task_id),
            design_version_id: Some(import.design_version_id),
            requirement_key: Some("REQ-001"),
            evidence_type: "commit",
            commit_sha: Some("abc123"),
            file_path: None,
            line_ref: None,
            symbol: None,
            artifact_path: None,
            note: None,
        },
    )
    .unwrap();
    add_coverage_item(
        temp.path(),
        NewCoverageItem {
            design_version_id: import.design_version_id,
            requirement_key: "REQ-001",
            review_scope_id: None,
            work_unit_id: Some(work_two.child_work_unit_id),
            task_id: None,
            requirement: "cleanup behavior is covered elsewhere",
            runtime_boundary_evidence: Some("other work runtime path"),
            ux_boundary_evidence: None,
            lifecycle_boundary_evidence: None,
            tests_or_gates: Some("other gate"),
            missing_or_unverified: None,
            status: "covered",
        },
    )
    .unwrap();
    let mismatched_task_work_coverage = add_coverage_item(
        temp.path(),
        NewCoverageItem {
            design_version_id: import.design_version_id,
            requirement_key: "REQ-001",
            review_scope_id: None,
            work_unit_id: Some(work_two.child_work_unit_id),
            task_id: Some(task.task_id),
            requirement: "cleanup behavior is incorrectly attributed",
            runtime_boundary_evidence: Some("wrong work runtime path"),
            ux_boundary_evidence: None,
            lifecycle_boundary_evidence: None,
            tests_or_gates: Some("wrong gate"),
            missing_or_unverified: None,
            status: "covered",
        },
    );

    let wrong_work_coverage = close_task(temp.path(), task.task_id, Some("abc123"));
    add_coverage_item(
        temp.path(),
        NewCoverageItem {
            design_version_id: import.design_version_id,
            requirement_key: "REQ-001",
            review_scope_id: None,
            work_unit_id: Some(work_one.work_unit_id),
            task_id: None,
            requirement: "cleanup behavior is covered in this work",
            runtime_boundary_evidence: Some("cleanup runtime path"),
            ux_boundary_evidence: None,
            lifecycle_boundary_evidence: None,
            tests_or_gates: Some("cleanup gate"),
            missing_or_unverified: None,
            status: "covered",
        },
    )
    .unwrap();
    let same_work_coverage = close_task(temp.path(), task.task_id, Some("abc123"));

    assert!(mismatched_task_work_coverage.is_err());
    assert!(wrong_work_coverage.is_err());
    assert!(same_work_coverage.is_ok());
}

#[test]
fn trace_links_reject_mismatched_requirement_task_pairs() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    start_work(temp.path(), "implement storage lifecycle", None).unwrap();
    let task_one = add_task(
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
    let task_two = add_task(
        temp.path(),
        NewTask {
            title: "implement archival",
            priority: "high",
            source: "design",
            work_unit_id: None,
            details: None,
            completion_condition: Some("archival behavior is covered"),
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
        format!(
            "{}\n{}",
            requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
            requirement_doc("REQ-002", "Preserve archival behavior", "high")
        ),
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
    derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: import.design_version_id,
            requirement_key: "REQ-001",
            task_id: task_one.task_id,
            derivation_reason: None,
            checklist_title: None,
            item_title: None,
            completion_condition: None,
        },
    )
    .unwrap();
    derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: import.design_version_id,
            requirement_key: "REQ-002",
            task_id: task_two.task_id,
            derivation_reason: None,
            checklist_title: None,
            item_title: None,
            completion_condition: None,
        },
    )
    .unwrap();

    let mismatched_evidence = add_implementation_evidence(
        temp.path(),
        NewImplementationEvidence {
            task_id: Some(task_two.task_id),
            design_version_id: Some(import.design_version_id),
            requirement_key: Some("REQ-001"),
            evidence_type: "commit",
            commit_sha: Some("abc123"),
            file_path: None,
            line_ref: None,
            symbol: None,
            artifact_path: None,
            note: None,
        },
    );
    let mismatched_gate = select_validation_gate(
        temp.path(),
        ValidationGateSelection {
            design_version_id: import.design_version_id,
            gate_key: "GATE-001",
            requirement_key: "REQ-001",
            task_id: task_two.task_id,
            command: None,
            command_profile: None,
            timeout: None,
        },
    );
    let mismatched_coverage = add_coverage_item(
        temp.path(),
        NewCoverageItem {
            design_version_id: import.design_version_id,
            requirement_key: "REQ-001",
            review_scope_id: None,
            work_unit_id: None,
            task_id: Some(task_two.task_id),
            requirement: "cleanup behavior is connected",
            runtime_boundary_evidence: None,
            ux_boundary_evidence: None,
            lifecycle_boundary_evidence: None,
            tests_or_gates: Some("GATE-001"),
            missing_or_unverified: None,
            status: "covered",
        },
    );
    let raw_out_of_scope_coverage = add_coverage_item(
        temp.path(),
        NewCoverageItem {
            design_version_id: import.design_version_id,
            requirement_key: "REQ-001",
            review_scope_id: None,
            work_unit_id: None,
            task_id: Some(task_one.task_id),
            requirement: "cleanup behavior is accepted out of scope",
            runtime_boundary_evidence: None,
            ux_boundary_evidence: None,
            lifecycle_boundary_evidence: None,
            tests_or_gates: None,
            missing_or_unverified: Some("not verified"),
            status: "accepted_out_of_scope",
        },
    );

    assert!(mismatched_evidence.is_err());
    assert!(mismatched_gate.is_err());
    assert!(mismatched_coverage.is_err());
    assert!(raw_out_of_scope_coverage.is_err());
}

#[test]
fn approved_coverage_acceptance_can_satisfy_trace_closure() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "implement storage lifecycle", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "implement cleanup",
            priority: "high",
            source: "design",
            work_unit_id: None,
            details: None,
            completion_condition: Some("cleanup behavior is covered or accepted"),
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
    add_implementation_evidence(
        temp.path(),
        NewImplementationEvidence {
            task_id: Some(task.task_id),
            design_version_id: Some(import.design_version_id),
            requirement_key: Some("REQ-001"),
            evidence_type: "commit",
            commit_sha: Some("abc123"),
            file_path: None,
            line_ref: None,
            symbol: None,
            artifact_path: None,
            note: None,
        },
    )
    .unwrap();
    let coverage = add_coverage_item(
        temp.path(),
        NewCoverageItem {
            design_version_id: import.design_version_id,
            requirement_key: "REQ-001",
            review_scope_id: None,
            work_unit_id: None,
            task_id: Some(task.task_id),
            requirement: "cleanup behavior is intentionally out of scope",
            runtime_boundary_evidence: None,
            ux_boundary_evidence: None,
            lifecycle_boundary_evidence: None,
            tests_or_gates: None,
            missing_or_unverified: Some("not applicable to this implementation"),
            status: "partial",
        },
    )
    .unwrap();
    let close_without_acceptance = close_task(temp.path(), task.task_id, Some("abc123"));
    let approval_authority_event_id = approval_authority_event(temp.path());
    let acceptance = accept_design_exception(
        temp.path(),
        NewDesignExceptionAcceptance {
            design_version_id: Some(import.design_version_id),
            design_package: None,
            target: &format!("coverage:{}", coverage.coverage_item_id),
            acceptance_type: "accepted_out_of_scope",
            reason: "coverage is explicitly out of scope for this work",
            approval_authority_event_id,
        },
    )
    .unwrap();
    close_task(temp.path(), task.task_id, Some("abc123")).unwrap();
    add_clean_implementation_ready_review(temp.path(), work.work_unit_id, import.design_version_id);
    let ready = implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(import.design_version_id),
        },
    )
    .unwrap();

    assert!(close_without_acceptance.is_err());
    assert_eq!(acceptance.target_type, "coverage_item");
    assert_eq!(acceptance.coverage_item_id, Some(coverage.coverage_item_id));
    assert_eq!(ready.result, "pass");
}
