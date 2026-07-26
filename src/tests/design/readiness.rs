use super::*;

#[test]
fn close_ready_accepts_current_selected_gate_for_unchanged_carried_requirement() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "implement carried requirement", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "implement cleanup",
            priority: "high",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
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
    let old_derivation = derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: first_import.design_version_id,
            requirement_key: "REQ-001",
            task_id: task.task_id,
            derivation_reason: Some("original decomposition"),
            checklist_title: Some("Original checklist"),
            item_title: Some("Implement original cleanup requirement"),
            completion_condition: Some("cleanup behavior is covered"),
        },
    )
    .unwrap();

    fs::write(
        init.package_path.join("01-introduction-goals.md"),
        "# Introduction And Goals\n\nNon-requirement wording changed.\n",
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
    let current_derivation = derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: second_import.design_version_id,
            requirement_key: "REQ-001",
            task_id: task.task_id,
            derivation_reason: Some("current decomposition"),
            checklist_title: Some("Current checklist"),
            item_title: Some("Implement current cleanup requirement"),
            completion_condition: Some("cleanup behavior is covered"),
        },
    )
    .unwrap();
    let current_gate = select_validation_gate(
        temp.path(),
        ValidationGateSelection {
            design_version_id: second_import.design_version_id,
            gate_key: "GATE-001",
            requirement_key: "REQ-001",
            task_id: task.task_id,
            command: Some("cargo test"),
            command_profile: None,
            timeout: None,
        },
    )
    .unwrap();

    for design_version_id in [
        first_import.design_version_id,
        second_import.design_version_id,
    ] {
        add_implementation_evidence(
            temp.path(),
            NewImplementationEvidence {
                task_id: Some(task.task_id),
                design_version_id: Some(design_version_id),
                requirement_key: Some("REQ-001"),
                evidence_type: "commit",
                commit_sha: Some("abc123"),
                file_path: None,
                line_ref: None,
                symbol: None,
                artifact_path: None,
                note: Some("cleanup implementation evidence"),
            },
        )
        .unwrap();
        add_coverage_item(
            temp.path(),
            NewCoverageItem {
                design_version_id,
                requirement_key: "REQ-001",
                review_scope_id: None,
                work_unit_id: None,
                task_id: Some(task.task_id),
                requirement: "cleanup behavior is connected",
                runtime_boundary_evidence: Some("cleanup path is exercised"),
                ux_boundary_evidence: None,
                lifecycle_boundary_evidence: None,
                tests_or_gates: Some("GATE-001"),
                missing_or_unverified: None,
                status: "covered",
            },
        )
        .unwrap();
    }
    close_checklist_item(temp.path(), old_derivation.checklist_item_id).unwrap();
    close_checklist(temp.path(), old_derivation.checklist_id).unwrap();
    close_checklist_item(temp.path(), current_derivation.checklist_item_id).unwrap();
    close_checklist(temp.path(), current_derivation.checklist_id).unwrap();
    close_task(temp.path(), task.task_id, Some("abc123")).unwrap();
    {
        let db = open_existing_project(temp.path()).unwrap();
        db.execute(
            "delete from coverage_items where design_requirement_id in (select id from design_requirements where design_version_id=?1)",
            params![first_import.design_version_id],
        )
        .unwrap();
    }
    add_validation_run(
        temp.path(),
        NewValidationRun {
            validation_gate_id: current_gate.validation_gate_id,
            command_usage_id: None,
            repository_snapshot_id: None,
            result: "pass",
            command: None,
            classification: None,
            acceptance_record_id: None,
            artifact_path: None,
            artifact_hash: None,
            notes: Some("current validation passed"),
        },
    )
    .unwrap();
    record_close_evidence(temp.path(), work.work_unit_id, work.activation_id);

    let ready = close_ready(temp.path()).unwrap();

    assert!(
        ready.items.iter().any(|item| {
            item.name == "validation_runs_recorded"
                && item.result == "pass"
                && item.details.contains("0 missing selected gates")
        }),
        "{ready:#?}"
    );
    assert!(
        ready.items.iter().any(|item| {
            item.name == "design_trace_closed"
                && item.result == "pass"
                && item.details.contains("0 missing requirement coverage")
        }),
        "{ready:#?}"
    );
}

#[test]
fn implementation_ready_blocks_stale_derivations_and_checklists() {
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
    let import_a = import_design_package(
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
            design_version_id: import_a.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: import_a.design_version_id,
            requirement_key: "REQ-001",
            task_id: task.task_id,
            derivation_reason: Some("design task decomposition"),
            checklist_title: None,
            item_title: None,
            completion_condition: None,
        },
    )
    .unwrap();
    let stale_gate = select_validation_gate(
        temp.path(),
        ValidationGateSelection {
            design_version_id: import_a.design_version_id,
            gate_key: "GATE-001",
            requirement_key: "REQ-001",
            task_id: task.task_id,
            command: Some("cargo test"),
            command_profile: None,
            timeout: None,
        },
    )
    .unwrap();
    {
        let db = open_existing_project(temp.path()).unwrap();
        db.execute(
            "update validation_gates set work_unit_id = null where id = ?1",
            [stale_gate.validation_gate_id],
        )
        .unwrap();
    }
    add_coverage_item(
        temp.path(),
        NewCoverageItem {
            design_version_id: import_a.design_version_id,
            requirement_key: "REQ-001",
            review_scope_id: None,
            work_unit_id: None,
            task_id: Some(task.task_id),
            requirement: "cleanup behavior is connected",
            runtime_boundary_evidence: Some("cleanup path is exercised"),
            ux_boundary_evidence: None,
            lifecycle_boundary_evidence: None,
            tests_or_gates: Some("GATE-001"),
            missing_or_unverified: None,
            status: "covered",
        },
    )
    .unwrap();
    add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(import_a.design_version_id),
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
    fs::write(
        init.package_path.join("requirements").join("README.md"),
        r#"## REQ-001: Preserve cleanup behavior
```yaml agent-workbench
type: requirement
key: REQ-001
revision: 2
priority: high
surfaces: [cli, database]
validation: [GATE-001]
status: active
```

This requirement describes changed cleanup behavior that must be implemented.
"#,
    )
    .unwrap();
    let import_b = import_design_package(
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
            design_version_id: import_b.design_version_id,
            summary: None,
        },
    )
    .unwrap();

    let blocked = implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(import_b.design_version_id),
        },
    )
    .unwrap();

    assert_eq!(blocked.result, "blocked");
    assert!(
        blocked
            .items
            .iter()
            .any(|item| { item.name == "task_derivations_current" && item.result == "fail" })
    );
    assert!(
        blocked
            .items
            .iter()
            .any(|item| { item.name == "checklists_current" && item.result == "fail" })
    );
    assert!(
        blocked
            .items
            .iter()
            .any(|item| { item.name == "validation_gates_current" && item.result == "fail" })
    );
    assert!(
        blocked
            .items
            .iter()
            .any(|item| { item.name == "coverage_items_current" && item.result == "fail" })
    );
    let stale = list_stale_records(temp.path()).unwrap();
    assert!(
        stale
            .iter()
            .any(|record| record.record_type == "task_derivation")
    );
    assert!(stale.iter().any(|record| record.record_type == "checklist"));
    assert!(stale.iter().any(|record| {
        record.record_type == "validation_gate" && record.id == stale_gate.validation_gate_id
    }));
    assert!(
        stale
            .iter()
            .any(|record| record.record_type == "coverage_item")
    );
    {
        let db = open_existing_project(temp.path()).unwrap();
        db.execute(
            "update work_units set status = 'closed' where id = ?1",
            [work.work_unit_id],
        )
        .unwrap();
    }
    let closed_history = implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(import_b.design_version_id),
        },
    )
    .unwrap();
    for item_name in [
        "task_derivations_current",
        "checklists_current",
        "validation_gates_current",
        "coverage_items_current",
    ] {
        assert!(
            closed_history
                .items
                .iter()
                .any(|item| item.name == item_name && item.result == "pass"),
            "{item_name} should ignore stale records owned by closed work: {closed_history:#?}"
        );
    }
    let retained_history = list_stale_records(temp.path()).unwrap();
    for record_type in [
        "task_derivation",
        "checklist",
        "validation_gate",
        "coverage_item",
    ] {
        assert!(
            retained_history
                .iter()
                .any(|record| record.record_type == record_type),
            "{record_type} should remain visible as stale history"
        );
    }
    {
        let db = open_existing_project(temp.path()).unwrap();
        db.execute(
            "update work_units set status = 'open' where id = ?1",
            [work.work_unit_id],
        )
        .unwrap();
    }
    let coverage_item_id = stale
        .iter()
        .find(|record| record.record_type == "coverage_item")
        .map(|record| record.id)
        .unwrap();
    assert!(
        stale
            .iter()
            .any(|record| record.record_type == "review_plan")
    );
    assert!(
        close_stale_record(
            temp.path(),
            StaleRecordDisposition {
                record_type: "coverage_item",
                record_id: coverage_item_id,
                reason: "coverage stale records are accepted rather than closed",
            },
        )
        .is_err()
    );

    let approval_authority_event_id = approval_authority_event(temp.path());
    add_general_acceptance(
        temp.path(),
        NewGeneralAcceptance {
            target: "stale:task_derivation:1",
            acceptance_type: "stale_accepted",
            reason: "user accepted stale derivation while preserving scope",
            approval_authority_event_id,
        },
    )
    .unwrap();
    add_general_acceptance(
        temp.path(),
        NewGeneralAcceptance {
            target: "stale:checklist:1",
            acceptance_type: "stale_accepted",
            reason: "user accepted stale checklist while preserving scope",
            approval_authority_event_id,
        },
    )
    .unwrap();
    let accepted_gate = accept_stale_record(
        temp.path(),
        StaleRecordDisposition {
            record_type: "validation_gate",
            record_id: stale_gate.validation_gate_id,
            reason: "user accepted stale validation gate while preserving scope",
        },
    )
    .unwrap();
    assert_eq!(accepted_gate.record_type, "validation_gate");
    assert_eq!(accepted_gate.record_id, stale_gate.validation_gate_id);
    let accepted_coverage = accept_stale_record(
        temp.path(),
        StaleRecordDisposition {
            record_type: "coverage_item",
            record_id: coverage_item_id,
            reason: "user accepted stale coverage while preserving scope",
        },
    )
    .unwrap();
    assert_eq!(accepted_coverage.record_type, "coverage_item");
    assert_eq!(accepted_coverage.record_id, coverage_item_id);
    assert_eq!(accepted_coverage.status, "stale_accepted");
    let accepted = implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(import_b.design_version_id),
        },
    )
    .unwrap();

    assert!(
        accepted
            .items
            .iter()
            .any(|item| { item.name == "task_derivations_current" && item.result == "pass" })
    );
    assert!(
        accepted
            .items
            .iter()
            .any(|item| { item.name == "checklists_current" && item.result == "pass" })
    );
    assert!(
        accepted
            .items
            .iter()
            .any(|item| { item.name == "validation_gates_current" && item.result == "pass" })
    );
    assert!(
        accepted
            .items
            .iter()
            .any(|item| { item.name == "coverage_items_current" && item.result == "pass" })
    );
}

#[test]
fn design_approval_marks_current_version_and_creates_authority() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
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
    let import = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &init.package_path,
            status: "draft",
        },
    )
    .unwrap();

    let approval = approve_design_version(
        temp.path(),
        DesignVersionApproval {
            design_version_id: import.design_version_id,
            summary: Some("design passed document checks"),
        },
    )
    .unwrap();

    assert_eq!(approval.design_version_id, import.design_version_id);
    assert_eq!(approval.design_package_id, import.design_package_id);
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let approved: (String, i64, Option<String>) = conn
        .query_row(
            "select status, approved_by_authority_event_id, approved_at from design_versions where id = ?1",
            params![import.design_version_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    let package_status: String = conn
        .query_row(
            "select status from design_packages where id = ?1",
            params![import.design_package_id],
            |row| row.get(0),
        )
        .unwrap();
    let authority: (String, String) = conn
        .query_row(
            "select event_type, text_or_summary from authority_events where id = ?1",
            params![approval.authority_event_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(approved.0, "approved");
    assert_eq!(approved.1, approval.authority_event_id);
    assert!(approved.2.is_some());
    assert_eq!(package_status, "approved");
    assert_eq!(authority.0, "design_doc");
    assert_eq!(authority.1, "design passed document checks");
}

#[test]
fn design_ready_passes_after_clean_design_document_review_without_approval() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
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
    let import = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &init.package_path,
            status: "draft",
        },
    )
    .unwrap();

    let blocked = design_ready(
        temp.path(),
        DesignReadyCheck {
            design_version_id: Some(import.design_version_id),
        },
    )
    .unwrap();
    let review_work_unit_id = start_work(temp.path(), "design-ready review", None)
        .unwrap()
        .work_unit_id;
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: review_work_unit_id,
            design_version_id: Some(import.design_version_id),
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
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some(&format!(
                "review-context:design-review:design={}:work={}",
                import.design_version_id, review_work_unit_id
            )),
            prompt_deviations: None,
            result_summary: Some("clean design review"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: Some("test-reviewer"),
            external_agent_id: Some("test-reviewer-1"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("test-reviewer-output"),
        },
    )
    .unwrap();
    let passed = design_ready(
        temp.path(),
        DesignReadyCheck {
            design_version_id: Some(import.design_version_id),
        },
    )
    .unwrap();

    assert_eq!(blocked.result, "blocked");
    assert!(
        blocked
            .items
            .iter()
            .any(|item| item.name == "design_review_clean" && item.result == "fail")
    );
    assert_eq!(passed.result, "pass");
    assert!(passed.items.iter().all(|item| item.result == "pass"));
}

#[test]
fn design_ready_allows_missing_validation_when_requirement_exception_is_accepted() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
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
        requirement_doc_without_validation("REQ-001", "Preserve cleanup behavior", "high"),
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
    let review_work_unit_id = start_work(temp.path(), "design-ready review", None)
        .unwrap()
        .work_unit_id;
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: review_work_unit_id,
            design_version_id: Some(import.design_version_id),
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
    add_clean_review_run(
        temp.path(),
        plan.review_plan_id,
        Some(&format!(
            "review-context:design-review:design={}:work={}",
            import.design_version_id, review_work_unit_id
        )),
        "clean design review",
    );

    let blocked = design_ready(
        temp.path(),
        DesignReadyCheck {
            design_version_id: Some(import.design_version_id),
        },
    )
    .unwrap();
    let approval_authority_event_id = approval_authority_event(temp.path());
    let acceptance = accept_design_exception(
        temp.path(),
        NewDesignExceptionAcceptance {
            design_version_id: Some(import.design_version_id),
            design_package: None,
            target: "requirement:REQ-001",
            acceptance_type: "evidence_gap",
            reason: "validation will be selected after implementation planning",
            approval_authority_event_id,
        },
    )
    .unwrap();
    let passed = design_ready(
        temp.path(),
        DesignReadyCheck {
            design_version_id: Some(import.design_version_id),
        },
    )
    .unwrap();

    assert!(
        blocked
            .items
            .iter()
            .any(|item| item.name == "requirement_validation_defined" && item.result == "fail")
    );
    assert_eq!(acceptance.target_type, "design_requirement");
    assert_eq!(passed.result, "pass");
    assert!(passed.items.iter().all(|item| item.result == "pass"));
}
