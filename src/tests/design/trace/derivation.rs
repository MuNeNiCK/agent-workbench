use super::super::*;

#[test]
fn task_derivation_creates_checklist_trace_and_unblocks_implementation_ready() {
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
    let blocked = implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(import.design_version_id),
        },
    )
    .unwrap();

    let derivation = derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: import.design_version_id,
            requirement_key: "REQ-001",
            task_id: task.task_id,
            derivation_reason: Some("design task decomposition"),
            checklist_title: None,
            item_title: None,
            completion_condition: None,
        },
    )
    .unwrap();
    assert!(
        accept_task_out_of_scope(temp.path(), task.task_id, "must use verified carry-forward")
            .unwrap_err()
            .to_string()
            .contains("verified baseline proof")
    );
    let blocked_without_gate = implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(import.design_version_id),
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
            command: None,
            command_profile: None,
            timeout: None,
        },
    )
    .unwrap();
    let implementation_review_plan_id = add_implementation_ready_review_plan(
        temp.path(),
        work.work_unit_id,
        import.design_version_id,
    );
    let missing_context_run = add_clean_review_run_result(
        temp.path(),
        implementation_review_plan_id,
        None,
        "clean decomposition review without context",
    );
    assert!(missing_context_run.is_err());
    let blocked_without_review_context = implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(import.design_version_id),
        },
    )
    .unwrap();
    add_clean_review_run(
        temp.path(),
        implementation_review_plan_id,
        Some(&format!(
            "review-context:design-task-decomposition:design={}:work={}",
            import.design_version_id, work.work_unit_id
        )),
        "clean decomposition review",
    );
    let passed = implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(import.design_version_id),
        },
    )
    .unwrap();
    let close_without_trace = close_task(temp.path(), task.task_id, Some("abc123"));
    let task_only_evidence = add_implementation_evidence(
        temp.path(),
        NewImplementationEvidence {
            task_id: Some(task.task_id),
            design_version_id: None,
            requirement_key: None,
            evidence_type: "commit",
            commit_sha: Some("task-only"),
            file_path: None,
            line_ref: None,
            symbol: None,
            artifact_path: None,
            note: None,
        },
    );
    let design_evidence_before_requirement_link = list_implementation_evidence(
        temp.path(),
        ImplementationEvidenceListQuery {
            task_id: None,
            design_version_id: Some(import.design_version_id),
            work_unit_id: None,
            evidence_type: None,
        },
    )
    .unwrap();
    let superseded_gap = add_coverage_item(
        temp.path(),
        NewCoverageItem {
            design_version_id: import.design_version_id,
            requirement_key: "REQ-001",
            review_scope_id: None,
            work_unit_id: None,
            task_id: Some(task.task_id),
            requirement: "cleanup behavior still needs implementation evidence",
            runtime_boundary_evidence: None,
            ux_boundary_evidence: None,
            lifecycle_boundary_evidence: None,
            tests_or_gates: None,
            missing_or_unverified: Some("implementation evidence required"),
            status: "needs_evidence",
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
            requirement: "cleanup behavior is connected to implementation and tests",
            runtime_boundary_evidence: Some("cleanup path preserves lifecycle behavior"),
            ux_boundary_evidence: None,
            lifecycle_boundary_evidence: Some("storage lifecycle remains intact"),
            tests_or_gates: Some("GATE-001"),
            missing_or_unverified: None,
            status: "covered",
        },
    )
    .unwrap();
    let close_without_requirement_evidence = close_task(temp.path(), task.task_id, Some("abc123"));
    let evidence = add_implementation_evidence(
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
    let evidence_records = list_implementation_evidence(
        temp.path(),
        ImplementationEvidenceListQuery {
            task_id: None,
            design_version_id: Some(import.design_version_id),
            work_unit_id: None,
            evidence_type: None,
        },
    )
    .unwrap();
    let coverage_records = list_coverage_items(
        temp.path(),
        CoverageItemListQuery {
            design_version_id: import.design_version_id,
            status: Some("covered"),
            work_unit_id: None,
        },
    )
    .unwrap();
    let stale_coverage = list_coverage_items(
        temp.path(),
        CoverageItemListQuery {
            design_version_id: import.design_version_id,
            status: Some("stale"),
            work_unit_id: None,
        },
    )
    .unwrap();
    let review_context = render_review_context(
        temp.path(),
        ReviewContextQuery {
            kind: "implementation-review",
            design_version_id: Some(import.design_version_id),
            work_unit_id: Some(work.work_unit_id),
            phase_id: None,
        },
    )
    .unwrap();
    close_task(temp.path(), task.task_id, Some("abc123")).unwrap();
    let passed_after_close = implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(import.design_version_id),
        },
    )
    .unwrap();
    let checklist_items = list_checklist_items(
        temp.path(),
        ChecklistItemListQuery {
            checklist_id: Some(derivation.checklist_id),
            status: Some("open"),
        },
    )
    .unwrap();
    let premature_checklist_close = close_checklist(temp.path(), derivation.checklist_id);
    let close_blocked_by_checklist = close_ready(temp.path()).unwrap();
    close_checklist_item(temp.path(), derivation.checklist_item_id).unwrap();
    close_checklist(temp.path(), derivation.checklist_id).unwrap();
    let close_blocked_without_reviews = close_ready(temp.path()).unwrap();
    let completed_usage = add_command_usage(
        temp.path(),
        NewCommandUsage {
            profile: None,
            command: Some("manual GATE-001 validation"),
            result: "pass",
            log_path: None,
            work_unit_id: Some(work.work_unit_id),
        },
    )
    .unwrap();
    add_validation_run(
        temp.path(),
        NewValidationRun {
            validation_gate_id: gate.validation_gate_id,
            command_usage_id: Some(completed_usage.command_usage_id),
            repository_snapshot_id: None,
            result: "pass",
            command: None,
            classification: None,
            acceptance_record_id: None,
            artifact_path: None,
            artifact_hash: None,
            notes: Some("design gate passed"),
        },
    )
    .unwrap();
    let close_review_plans =
        add_close_ready_review_plans(temp.path(), work.work_unit_id, import.design_version_id);
    let missing_close_context_runs = add_clean_close_ready_review_runs(
        temp.path(),
        work.work_unit_id,
        import.design_version_id,
        &close_review_plans,
        false,
    );
    assert!(missing_close_context_runs.iter().all(Result::is_err));
    let close_blocked_without_context = close_ready(temp.path()).unwrap();
    add_clean_close_ready_review_runs(
        temp.path(),
        work.work_unit_id,
        import.design_version_id,
        &close_review_plans,
        true,
    );
    record_close_evidence(temp.path(), work.work_unit_id, work.activation_id);
    let close_passed = close_ready(temp.path()).unwrap();
    let records = list_task_derivations(
        temp.path(),
        TaskDerivationListQuery {
            design_version_id: import.design_version_id,
            work_unit_id: None,
        },
    )
    .unwrap();

    assert_eq!(blocked.result, "blocked");
    assert!(
        blocked
            .items
            .iter()
            .any(|item| { item.name == "task_derivations_exist" && item.result == "fail" })
    );
    assert_eq!(derivation.task_id, task.task_id);
    assert_eq!(gate.task_id, task.task_id);
    assert_eq!(blocked_without_gate.result, "blocked");
    assert!(
        blocked_without_gate
            .items
            .iter()
            .any(|item| { item.name == "validation_gates_selected" && item.result == "fail" })
    );
    assert!(blocked_without_review_context.items.iter().any(|item| {
        item.name == "pre_implementation_reviews_clean"
            && item.result == "fail"
            && item
                .detail
                .as_deref()
                .is_some_and(|details| details.contains("missing review-context runs"))
    }));
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].requirement_key, "REQ-001");
    assert_eq!(passed.result, "pass", "{:#?}", passed.items);
    assert!(close_without_trace.is_err());
    assert!(task_only_evidence.is_err());
    assert!(design_evidence_before_requirement_link.is_empty());
    assert!(close_without_requirement_evidence.is_err());
    assert_eq!(evidence.task_id, Some(task.task_id));
    assert_eq!(evidence_records.len(), 1);
    assert_eq!(
        evidence_records[0].requirement_key.as_deref(),
        Some("REQ-001")
    );
    assert_eq!(evidence_records[0].commit_sha.as_deref(), Some("abc123"));
    assert_eq!(passed_after_close.result, "pass");
    assert_eq!(checklist_items.len(), 1);
    assert!(premature_checklist_close.is_err());
    assert!(close_blocked_by_checklist.items.iter().any(|item| {
        item.name == "design_trace_closed"
            && item.result == "fail"
            && item.details.contains("1 open checklist items")
            && item.details.contains("1 active checklists")
    }));
    assert_eq!(close_blocked_without_reviews.result, "blocked");
    assert!(close_blocked_without_reviews.items.iter().any(|item| {
        item.name == "review_plans_clean"
            && item.result == "fail"
            && item.details.contains("design_implementation_diff")
            && item.details.contains("implementation_review")
    }));
    assert!(close_blocked_without_context.items.iter().any(|item| {
        item.name == "review_plans_clean"
            && item.result == "fail"
            && item.details.contains("missing review-context runs")
            && item
                .details
                .contains("missing_context:design-implementation-diff")
            && item
                .details
                .contains("missing_context:implementation-review")
            && item.details.contains(&format!(
                "context_ref:review-context:implementation-review:design={}:work={}",
                import.design_version_id, work.work_unit_id
            ))
    }));
    assert_eq!(close_passed.result, "pass", "{:#?}", close_passed.items);
    assert_eq!(coverage.task_id, Some(task.task_id));
    assert_eq!(coverage_records.len(), 1);
    assert_eq!(coverage_records[0].requirement_key, "REQ-001");
    assert_eq!(coverage_records[0].status, "covered");
    assert_eq!(stale_coverage.len(), 1);
    assert_eq!(stale_coverage[0].id, superseded_gap.coverage_item_id);
    assert!(
        !review_context
            .text
            .contains("implementation evidence required")
    );
    assert!(review_context.text.contains("known_gaps:\n- none"));
    assert!(!review_context.text.contains(&format!(
        "coverage_item:{}",
        superseded_gap.coverage_item_id
    )));
}
