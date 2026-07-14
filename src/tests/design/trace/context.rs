use super::super::*;

#[test]
fn review_context_filters_design_trace_by_work_unit() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work_one = start_work(temp.path(), "implement cleanup", None).unwrap();
    let task_one = add_task(
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
        interrupt_work(temp.path(), "implement archival", "parallel design work").unwrap();
    let task_two = add_task(
        temp.path(),
        NewTask {
            title: "archival task",
            priority: "medium",
            source: "design",
            work_unit_id: None,
            details: None,
            completion_condition: Some("archival covered"),
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
            requirement_doc("REQ-002", "Preserve archival behavior", "medium")
        ),
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
    add_implementation_evidence(
        temp.path(),
        NewImplementationEvidence {
            task_id: Some(task_one.task_id),
            design_version_id: Some(import.design_version_id),
            requirement_key: Some("REQ-001"),
            evidence_type: "commit",
            commit_sha: Some("cleanup123"),
            file_path: None,
            line_ref: None,
            symbol: None,
            artifact_path: None,
            note: None,
        },
    )
    .unwrap();
    add_implementation_evidence(
        temp.path(),
        NewImplementationEvidence {
            task_id: None,
            design_version_id: Some(import.design_version_id),
            requirement_key: Some("REQ-001"),
            evidence_type: "artifact",
            commit_sha: None,
            file_path: None,
            line_ref: None,
            symbol: None,
            artifact_path: Some("artifacts/cleanup-review.txt"),
            note: None,
        },
    )
    .unwrap();
    add_implementation_evidence(
        temp.path(),
        NewImplementationEvidence {
            task_id: Some(task_two.task_id),
            design_version_id: Some(import.design_version_id),
            requirement_key: Some("REQ-002"),
            evidence_type: "commit",
            commit_sha: Some("archival456"),
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
            work_unit_id: None,
            task_id: Some(task_one.task_id),
            requirement: "cleanup behavior is covered",
            runtime_boundary_evidence: Some("cleanup runtime path"),
            ux_boundary_evidence: None,
            lifecycle_boundary_evidence: None,
            tests_or_gates: Some("cleanup gate"),
            missing_or_unverified: None,
            status: "covered",
        },
    )
    .unwrap();
    add_coverage_item(
        temp.path(),
        NewCoverageItem {
            design_version_id: import.design_version_id,
            requirement_key: "REQ-002",
            review_scope_id: None,
            work_unit_id: None,
            task_id: Some(task_two.task_id),
            requirement: "archival behavior is covered",
            runtime_boundary_evidence: Some("archival runtime path"),
            ux_boundary_evidence: None,
            lifecycle_boundary_evidence: None,
            tests_or_gates: Some("archival gate"),
            missing_or_unverified: None,
            status: "covered",
        },
    )
    .unwrap();

    let document = render_review_context(
        temp.path(),
        ReviewContextQuery {
            kind: "design-implementation-diff",
            design_version_id: Some(import.design_version_id),
            work_unit_id: Some(work_one.work_unit_id),
            phase_id: None,
        },
    )
    .unwrap();

    assert!(document.text.contains("cleanup task"));
    assert!(document.text.contains("REQ-001"));
    assert!(document.text.contains("cleanup123"));
    assert!(document.text.contains("artifacts/cleanup-review.txt"));
    assert!(document.text.contains("cleanup behavior is covered"));
    assert!(!document.text.contains("archival task"));
    assert!(!document.text.contains("REQ-002"));
    assert!(!document.text.contains("archival456"));
    assert!(!document.text.contains("archival behavior is covered"));
    assert_eq!(work_two.child_work_unit_id, task_two.work_unit_id.unwrap());

    let decomposition_document = render_review_context(
        temp.path(),
        ReviewContextQuery {
            kind: "design-task-decomposition",
            design_version_id: Some(import.design_version_id),
            work_unit_id: Some(work_one.work_unit_id),
            phase_id: None,
        },
    )
    .unwrap();
    assert!(decomposition_document.text.contains("REQ-001"));
    assert!(decomposition_document.text.contains("REQ-002"));
}

#[test]
fn review_context_includes_selected_validation_run_evidence() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "implement cleanup", None).unwrap();
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
    let gate = select_validation_gate(
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
    let run = add_validation_run(
        temp.path(),
        NewValidationRun {
            validation_gate_id: gate.validation_gate_id,
            command_usage_id: None,
            repository_snapshot_id: None,
            result: "pass",
            command: None,
            classification: None,
            acceptance_record_id: None,
            artifact_path: Some("artifacts/gate.log"),
            artifact_hash: Some("hash123"),
            notes: Some("cleanup gate passed"),
        },
    )
    .unwrap();

    let document = render_review_context(
        temp.path(),
        ReviewContextQuery {
            kind: "design-implementation-diff",
            design_version_id: Some(import.design_version_id),
            work_unit_id: Some(work.work_unit_id),
            phase_id: None,
        },
    )
    .unwrap();

    assert!(
        document
            .text
            .contains(&format!("latest_run={}", run.validation_run_id))
    );
    assert!(document.text.contains("latest_result=pass"));
    assert!(document.text.contains("artifact=artifacts/gate.log"));
    assert!(document.text.contains("notes=cleanup gate passed"));
}

#[test]
fn implementation_review_context_includes_owned_work_evidence() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "rename migration workflow", None).unwrap();
    add_repository(
        temp.path(),
        NewRepository {
            name: "main",
            path: ".",
            current_head: Some("abc123"),
            status_summary: Some("clean"),
        },
    )
    .unwrap();
    let record = create_work_record(
        temp.path(),
        NewWorkRecord {
            work_unit_id: Some(work.work_unit_id),
            topic: "task history migration",
            work_performed: Some("renamed the public command and Rust responsibility"),
            next_actions: Some("review the implementation"),
            notable_operations: Some("removed the obsolete term"),
            export_path: None,
        },
    )
    .unwrap();
    let usage = add_command_usage(
        temp.path(),
        NewCommandUsage {
            profile: None,
            command: Some("cargo test"),
            result: "pass",
            log_path: Some("artifacts/test.log"),
            work_unit_id: Some(work.work_unit_id),
        },
    )
    .unwrap();
    add_work_record_command(
        temp.path(),
        NewWorkRecordCommand {
            work_record_id: record.work_record_id,
            command_usage_id: Some(usage.command_usage_id),
            command_profile_id: None,
            command: None,
            result: None,
            log_path: None,
            note: Some("full suite"),
        },
    )
    .unwrap();
    add_work_record_file(
        temp.path(),
        NewWorkRecordFile {
            work_record_id: record.work_record_id,
            path: "src/task_identity.rs",
            role: "changed",
            note: Some("owned implementation"),
        },
    )
    .unwrap();
    add_work_record_commit(
        temp.path(),
        NewWorkRecordCommit {
            work_record_id: record.work_record_id,
            commit_sha: "abc123",
            role: "created",
            note: Some("implementation commit"),
        },
    )
    .unwrap();
    let snapshot = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(work.activation_id),
            head_sha: Some("abc123"),
            branch: Some("main"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();

    let document = render_review_context(
        temp.path(),
        ReviewContextQuery {
            kind: "implementation-review",
            design_version_id: None,
            work_unit_id: Some(work.work_unit_id),
            phase_id: None,
        },
    )
    .unwrap();

    assert!(document.text.contains("work_records:"));
    assert!(document.text.contains("task history migration"));
    assert!(document.text.contains("command=cargo test result=pass"));
    assert!(document.text.contains("path=src/task_identity.rs"));
    assert!(document.text.contains("role=created commit=abc123"));
    assert!(document.text.contains(&format!(
        "repository_snapshots:\n- {}",
        snapshot.repository_snapshot_id
    )));
}
