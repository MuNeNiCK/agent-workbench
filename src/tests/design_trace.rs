use super::*;

#[test]
fn design_init_creates_standard_package_under_workbench() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();

    let outcome = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "storage-lifecycle",
            title: "Storage Lifecycle",
        },
    )
    .unwrap();

    let package = temp
        .path()
        .join(".agent-workbench")
        .join("designs")
        .join("storage-lifecycle");
    assert_eq!(outcome.package_path, package);
    assert!(package.join("design.yaml").exists());
    assert!(package.join("01-introduction-goals.md").exists());
    assert!(package.join("12-glossary.md").exists());
    assert!(package.join("requirements").join("README.md").exists());
    assert!(package.join("validation").join("gates.md").exists());

    let manifest = fs::read_to_string(package.join("design.yaml")).unwrap();
    assert!(manifest.contains(r#"id: "storage-lifecycle""#));
    assert!(manifest.contains(r#"title: "Storage Lifecycle""#));
    assert!(manifest.contains("format: arc42-agent-workbench"));
    assert!(manifest.contains("introduction_goals: 01-introduction-goals.md"));
}

#[test]
fn design_init_rejects_invalid_or_existing_package_id() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();

    assert!(
        init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "Storage",
                title: "Storage",
            },
        )
        .is_err()
    );
    assert!(
        init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "storage/lifecycle",
                title: "Storage",
            },
        )
        .is_err()
    );

    init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "storage-lifecycle",
            title: "Storage",
        },
    )
    .unwrap();
    assert!(
        init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "storage-lifecycle",
                title: "Storage",
            },
        )
        .is_err()
    );
}

#[test]
fn design_import_records_package_version_and_files() {
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

    let import = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &init.package_path,
            status: "draft",
        },
    )
    .unwrap();

    assert_eq!(import.design_package_id, 1);
    assert_eq!(import.design_version_id, 1);
    assert_eq!(import.version_number, 1);
    assert_eq!(import.file_count, 14);
    assert_eq!(import.content_hash.len(), 64);

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let package: (
        String,
        String,
        String,
        String,
        String,
        i64,
        String,
        String,
        i64,
    ) = conn
        .query_row(
            r#"
            select design_key, package_id, title, root_path, format, version,
                   package_hash, status, current_design_version_id
            from design_packages
            where id = ?1
            "#,
            params![import.design_package_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                ))
            },
        )
        .unwrap();
    let version: (String, String, String, Option<String>) = conn
        .query_row(
            r#"
            select source_ref, package_hash, status, approved_at
            from design_versions
            where id = ?1
            "#,
            params![import.design_version_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    let file_count: i64 = conn
        .query_row(
            "select count(*) from design_files where design_version_id = ?1",
            params![import.design_version_id],
            |row| row.get(0),
        )
        .unwrap();
    let short_file_hashes: i64 = conn
        .query_row(
            "select count(*) from design_files where design_version_id = ?1 and length(content_hash) != 64",
            params![import.design_version_id],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(package.0, "storage-lifecycle");
    assert_eq!(package.1, "storage-lifecycle");
    assert_eq!(package.2, "Storage Lifecycle");
    assert!(
        package
            .3
            .ends_with(".agent-workbench/designs/storage-lifecycle")
    );
    assert_eq!(package.4, "arc42-agent-workbench");
    assert_eq!(package.5, 1);
    assert_eq!(package.6, import.content_hash);
    assert_eq!(package.7, "draft");
    assert_eq!(package.8, import.design_version_id);
    assert!(
        version
            .0
            .ends_with(".agent-workbench/designs/storage-lifecycle")
    );
    assert_eq!(version.1, import.content_hash);
    assert_eq!(version.2, "draft");
    assert!(version.3.is_none());
    assert_eq!(file_count, 14);
    assert_eq!(short_file_hashes, 0);
}

#[test]
fn design_import_hashes_are_deterministic_and_change_with_content() {
    let temp_a = tempfile::tempdir().unwrap();
    init_project(temp_a.path()).unwrap();
    let init_a = init_design_package(
        temp_a.path(),
        NewDesignPackage {
            design_id: "storage-lifecycle",
            title: "Storage Lifecycle",
        },
    )
    .unwrap();
    fs::write(
        init_a.package_path.join("requirements").join("README.md"),
        requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
    )
    .unwrap();
    let import_a = import_design_package(
        temp_a.path(),
        DesignPackageImport {
            package_path: &init_a.package_path,
            status: "draft",
        },
    )
    .unwrap();
    let conn_a = open_ledger(&default_ledger_path(temp_a.path())).unwrap();
    let requirement_hash_a: String = conn_a
        .query_row(
            "select requirement_hash from design_requirements where design_version_id = ?1",
            params![import_a.design_version_id],
            |row| row.get(0),
        )
        .unwrap();

    let temp_b = tempfile::tempdir().unwrap();
    init_project(temp_b.path()).unwrap();
    let init_b = init_design_package(
        temp_b.path(),
        NewDesignPackage {
            design_id: "storage-lifecycle",
            title: "Storage Lifecycle",
        },
    )
    .unwrap();
    fs::write(
        init_b.package_path.join("requirements").join("README.md"),
        requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
    )
    .unwrap();
    let import_b = import_design_package(
        temp_b.path(),
        DesignPackageImport {
            package_path: &init_b.package_path,
            status: "draft",
        },
    )
    .unwrap();
    let conn_b = open_ledger(&default_ledger_path(temp_b.path())).unwrap();
    let requirement_hash_b: String = conn_b
        .query_row(
            "select requirement_hash from design_requirements where design_version_id = ?1",
            params![import_b.design_version_id],
            |row| row.get(0),
        )
        .unwrap();

    let temp_c = tempfile::tempdir().unwrap();
    init_project(temp_c.path()).unwrap();
    let init_c = init_design_package(
        temp_c.path(),
        NewDesignPackage {
            design_id: "storage-lifecycle",
            title: "Storage Lifecycle",
        },
    )
    .unwrap();
    fs::write(
        init_c.package_path.join("requirements").join("README.md"),
        requirement_doc("REQ-001", "Preserve cleanup behavior", "high")
            .replace("one verifiable behavior", "a different verifiable behavior"),
    )
    .unwrap();
    let import_c = import_design_package(
        temp_c.path(),
        DesignPackageImport {
            package_path: &init_c.package_path,
            status: "draft",
        },
    )
    .unwrap();
    let conn_c = open_ledger(&default_ledger_path(temp_c.path())).unwrap();
    let requirement_hash_c: String = conn_c
        .query_row(
            "select requirement_hash from design_requirements where design_version_id = ?1",
            params![import_c.design_version_id],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(import_a.content_hash, import_b.content_hash);
    assert_eq!(requirement_hash_a, requirement_hash_b);
    assert_ne!(import_a.content_hash, import_c.content_hash);
    assert_ne!(requirement_hash_a, requirement_hash_c);
}

#[test]
fn design_import_extracts_machine_readable_requirements() {
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
    let requirements = list_design_requirements(
        temp.path(),
        DesignRequirementListQuery {
            design_version_id: import.design_version_id,
        },
    )
    .unwrap();

    assert_eq!(import.requirement_count, 1);
    assert_eq!(requirements.len(), 1);
    assert_eq!(requirements[0].requirement_key, "REQ-001");
    assert_eq!(requirements[0].priority, "high");
    assert_eq!(
        requirements[0].validation_expectation.as_deref(),
        Some("GATE-001")
    );
    assert!(
        requirements[0]
            .requirement_text
            .contains("verifiable behavior")
    );
}

#[test]
fn design_import_extracts_decisions_and_validation_gate_templates() {
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
    fs::write(init.package_path.join("09-decisions.md"), decision_doc()).unwrap();
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
    let decisions = list_design_decisions(
        temp.path(),
        DesignDecisionListQuery {
            design_version_id: import.design_version_id,
        },
    )
    .unwrap();
    let gates = list_validation_gate_templates(
        temp.path(),
        ValidationGateTemplateListQuery {
            design_version_id: import.design_version_id,
        },
    )
    .unwrap();

    assert_eq!(import.decision_count, 1);
    assert_eq!(import.validation_gate_template_count, 1);
    assert_eq!(decisions[0].decision_key, "DEC-001");
    assert_eq!(decisions[0].topic, "Keep project-local ledger");
    assert_eq!(gates[0].gate_key, "GATE-001");
    assert_eq!(gates[0].stage, "implementation-ready");
    assert_eq!(gates[0].expected_result, "pass");
    assert_eq!(gates[0].requirement_keys.as_deref(), Some("REQ-001"));
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let linked_count: i64 = conn
        .query_row(
            "select count(*) from validation_gate_template_requirements",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(linked_count, 1);
}

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
    add_validation_run(
        temp.path(),
        NewValidationRun {
            validation_gate_id: gate.validation_gate_id,
            command_usage_id: None,
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
}

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
    assert_eq!(status.phase_blocker.unwrap().kind, "stale_design");
    assert!(status.finding_remediations.is_empty());
    assert!(matches!(
        next_action(temp.path()).unwrap(),
        NextAction::BlockedPhase {
            blocker: PhaseBlocker { ref kind, .. }
        } if kind == "stale_design"
    ));
}

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

#[test]
fn mediated_design_reconcile_recovers_duplicate_current_derivations() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "recover duplicate decomposition", None).unwrap();
    let init = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "storage-recovery",
            title: "Storage Recovery",
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
    let design = import_design_package(
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
            design_version_id: design.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    let ready = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(design.design_version_id),
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
            review_plan_id: ready.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some(&format!(
                "review-context:design-review:design={}:work={}",
                design.design_version_id, work.work_unit_id
            )),
            prompt_deviations: None,
            result_summary: Some("design ready"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: Some("reviewer"),
            external_agent_id: Some("reviewer-reconcile"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:reconcile-ready"),
        },
    )
    .unwrap();
    let canonical = decompose_design(
        temp.path(),
        DesignDecomposition {
            design_version_id: design.design_version_id,
            work_unit_id: work.work_unit_id,
            checklist_title: Some("canonical"),
            reason: Some("initial decomposition"),
        },
    )
    .unwrap();
    let canonical_task_for_coverage: i64 = open_ledger(&default_ledger_path(temp.path()))
        .unwrap()
        .query_row(
            "select task_id from checklist_items where checklist_id=?1",
            params![canonical.checklist_id],
            |row| row.get(0),
        )
        .unwrap();
    let canonical_coverage = add_coverage_item(
        temp.path(),
        NewCoverageItem {
            design_version_id: design.design_version_id,
            requirement_key: "REQ-001",
            review_scope_id: None,
            work_unit_id: Some(work.work_unit_id),
            task_id: Some(canonical_task_for_coverage),
            requirement: "canonical coverage",
            runtime_boundary_evidence: Some("reconciliation test"),
            ux_boundary_evidence: None,
            lifecycle_boundary_evidence: None,
            tests_or_gates: Some("GATE-001"),
            missing_or_unverified: None,
            status: "covered",
        },
    )
    .unwrap();
    let duplicate_task = add_task(
        temp.path(),
        NewTask {
            title: "duplicate cleanup task",
            priority: "high",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("duplicate is superseded"),
        },
    )
    .unwrap();
    let duplicate = derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: design.design_version_id,
            requirement_key: "REQ-001",
            task_id: duplicate_task.task_id,
            derivation_reason: Some("legacy duplicate"),
            checklist_title: Some("duplicate"),
            item_title: None,
            completion_condition: None,
        },
    )
    .unwrap();
    select_validation_gate(
        temp.path(),
        ValidationGateSelection {
            design_version_id: design.design_version_id,
            gate_key: "GATE-001",
            requirement_key: "REQ-001",
            task_id: duplicate_task.task_id,
            command: None,
            command_profile: None,
            timeout: None,
        },
    )
    .unwrap();
    let duplicate_coverage = add_coverage_item(
        temp.path(),
        NewCoverageItem {
            design_version_id: design.design_version_id,
            requirement_key: "REQ-001",
            review_scope_id: None,
            work_unit_id: Some(work.work_unit_id),
            task_id: Some(duplicate_task.task_id),
            requirement: "duplicate coverage",
            runtime_boundary_evidence: Some("legacy duplicate"),
            ux_boundary_evidence: None,
            lifecycle_boundary_evidence: None,
            tests_or_gates: Some("GATE-001"),
            missing_or_unverified: None,
            status: "covered",
        },
    )
    .unwrap();
    let phase = create_phase(
        temp.path(),
        NewWorkPhase {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(design.design_version_id),
            key: "implementation",
            title: "Implementation",
            kind: "implementation",
            order: 1,
            reason: Some("legacy phase membership"),
        },
    )
    .unwrap();
    assign_task_to_phase(temp.path(), phase.phase_id, duplicate_task.task_id).unwrap();
    let historical_task = add_task(
        temp.path(),
        NewTask {
            title: "pre-existing closed history",
            priority: "high",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("must not be adopted by reconciliation"),
        },
    )
    .unwrap();
    let unrelated_task = add_task(
        temp.path(),
        NewTask {
            title: "unrelated mapping target",
            priority: "low",
            source: "user",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("must not enter the canonical bundle"),
        },
    )
    .unwrap();
    let historical = derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: design.design_version_id,
            requirement_key: "REQ-001",
            task_id: historical_task.task_id,
            derivation_reason: Some("old closed history"),
            checklist_title: Some("historical"),
            item_title: None,
            completion_condition: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update task_derivations set status='closed' where id=?1",
        params![historical.task_derivation_id],
    )
    .unwrap();
    conn.execute(
        "update checklist_items set status='closed' where id=?1",
        params![historical.checklist_item_id],
    )
    .unwrap();
    conn.execute(
        "update checklists set status='closed' where id=?1",
        params![historical.checklist_id],
    )
    .unwrap();
    drop(conn);
    assign_task_to_phase(temp.path(), phase.phase_id, historical_task.task_id).unwrap();
    let stale_task = add_task(
        temp.path(),
        NewTask {
            title: "pre-existing stale accepted task",
            priority: "high",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("pre-existing acceptance is not session ownership"),
        },
    )
    .unwrap();
    let stale = derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: design.design_version_id,
            requirement_key: "REQ-001",
            task_id: stale_task.task_id,
            derivation_reason: Some("pre-existing stale derivation"),
            checklist_title: Some("stale historical"),
            item_title: None,
            completion_condition: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update task_derivations set status='stale' where id=?1",
        params![stale.task_derivation_id],
    )
    .unwrap();
    drop(conn);
    let stale_authority = approval_authority_event(temp.path());
    add_general_acceptance(
        temp.path(),
        NewGeneralAcceptance {
            target: &format!("stale:task_derivation:{}", stale.task_derivation_id),
            acceptance_type: "stale_accepted",
            reason: "approved before the correction session",
            approval_authority_event_id: stale_authority,
        },
    )
    .unwrap();
    assign_task_to_phase(temp.path(), phase.phase_id, stale_task.task_id).unwrap();
    let correction_plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(design.design_version_id),
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
    let run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: correction_plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("work_unit:1"),
            prompt_deviations: None,
            result_summary: Some("duplicate current decomposition"),
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
            finding_type: "design_finding",
            severity: "high",
            description: "reconcile duplicate current decomposition",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), finding.finding_id, "valid").unwrap();
    let surface = format!(
        "transition:design-reconcile:{}/{}/{},transition:stale-accept:coverage_item/{},transition:phase-assign:{}/@task/REQ-001,transition:task-accept-out-of-scope:{}",
        design.design_version_id,
        work.work_unit_id,
        canonical.checklist_id,
        duplicate_coverage.coverage_item_id,
        phase.phase_id,
        duplicate_task.task_id
    );
    let closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: finding.finding_id,
            design_invariant: "one canonical current bundle remains",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some(&surface),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("reconcile canonical checklist"),
            tests_or_gates: Some("GATE-001"),
            verification_plan: Some("resume design review"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    begin_correction(temp.path(), closure.closure_id).unwrap();
    let provenance_authority = add_authority_event(
        temp.path(),
        NewAuthorityEvent {
            event_type: "user_instruction",
            source: Some("test-user"),
            summary: "canonical acceptance provenance",
            scope: Some("project"),
            precedence: 100,
        },
    )
    .unwrap();
    let canonical_task: i64 = open_ledger(&default_ledger_path(temp.path()))
        .unwrap()
        .query_row(
            "select task_id from checklist_items where checklist_id=?1",
            params![canonical.checklist_id],
            |row| row.get(0),
        )
        .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let canonical_gate: i64 = conn
        .query_row(
            "select id from validation_gates where task_id=?1",
            params![canonical_task],
            |row| row.get(0),
        )
        .unwrap();
    let canonical_item: i64 = conn
        .query_row(
            "select id from checklist_items where checklist_id=?1 and task_id=?2",
            params![canonical.checklist_id, canonical_task],
            |row| row.get(0),
        )
        .unwrap();
    let canonical_derivation: i64 = conn
        .query_row(
            "select id from task_derivations where checklist_item_id=?1 and status='active'",
            params![canonical_item],
            |row| row.get(0),
        )
        .unwrap();
    conn.execute(
        "update task_derivations set checklist_item_id=?1 where id=?2",
        params![canonical_item, historical.task_derivation_id],
    )
    .unwrap();
    let (
        canonical_task_title,
        canonical_task_details,
        canonical_task_priority,
        canonical_task_completion,
    ): (String, String, String, String) = conn
        .query_row(
            "select title, details, priority, completion_condition from tasks where id=?1",
            params![canonical_task],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    let (canonical_item_order, canonical_item_title, canonical_item_completion): (
        i64,
        String,
        String,
    ) = conn
        .query_row(
            "select item_order, title, completion_condition from checklist_items where id=?1",
            params![canonical_item],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    let (canonical_template, canonical_gate_hash, canonical_gate_stage): (i64, String, String) = conn
        .query_row(
            "select vg.template_id, gt.gate_hash, gt.stage from validation_gates vg join validation_gate_templates gt on gt.id=vg.template_id where vg.id=?1",
            params![canonical_gate],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    conn.execute(
        r#"insert into acceptance_records(
            project_id, target_type, checklist_item_id, acceptance_type, reason,
            scope, created_by, status, approved_by_authority_event_id, approved_at, created_at
        ) values (1, 'checklist_item', ?1, 'explicit_exception', 'canonical provenance',
            'project', 'user', 'approved', ?2, current_timestamp, current_timestamp)"#,
        params![canonical_item, provenance_authority.authority_event_id],
    )
    .unwrap();
    for (target_type, target_column, target_id) in [
        ("task", "task_id", canonical_task),
        ("validation_gate", "validation_gate_id", canonical_gate),
        (
            "coverage_item",
            "coverage_item_id",
            canonical_coverage.coverage_item_id,
        ),
    ] {
        conn.execute(
            &format!("insert into acceptance_records(project_id, target_type, {target_column}, acceptance_type, reason, scope, created_by, status, approved_by_authority_event_id, approved_at, created_at) values (1, ?1, ?2, 'explicit_exception', 'canonical provenance', 'project', 'user', 'approved', ?3, current_timestamp, current_timestamp)"),
            params![target_type, target_id, provenance_authority.authority_event_id],
        )
        .unwrap();
    }
    conn.execute(
        r#"insert into design_requirements(
            project_id, design_version_id, source_design_file_id, source_section,
            requirement_key, revision, requirement_hash, supersedes_requirement_id,
            requirement_text, priority, required_surfaces, validation_expectation,
            status, created_at)
        select project_id, design_version_id, source_design_file_id, 'REQ-999: matrix',
            'REQ-999', 1, 'matrix-only-requirement', null, 'matrix only', 'low',
            null, null, 'active', current_timestamp
        from design_requirements where design_version_id=?1 and requirement_key='REQ-001'"#,
        params![design.design_version_id],
    )
    .unwrap();
    let matrix_requirement = conn.last_insert_rowid();
    conn.execute(
        r#"insert into design_versions(
            project_id, design_package_id, version_number, source_ref, package_hash,
            content_hash, package_path, manifest_path, format, manifest_version,
            status, imported_at)
        select project_id, design_package_id, version_number+100, 'matrix-foreign',
            'matrix-foreign-package', 'matrix-foreign-content', package_path,
            manifest_path, format, manifest_version, 'draft', current_timestamp
        from design_versions where id=?1"#,
        params![design.design_version_id],
    )
    .unwrap();
    let foreign_design_version = conn.last_insert_rowid();
    conn.execute(
        "update design_requirements set design_version_id=?1 where id=?2",
        params![foreign_design_version, matrix_requirement],
    )
    .unwrap();
    macro_rules! rejects_corruption_atomically {
        ($corrupt:expr, $restore:expr) => {{
            $corrupt.unwrap();
            let before_transition: String = conn.query_row(
                r#"select group_concat(snapshot, '|') from (
                    select 'task:'||id||':'||quote(title)||':'||priority||':'||status||':'||source||':'||quote(details)||':'||quote(completion_condition) snapshot from tasks where work_unit_id=?1
                    union all select 'derivation:'||td.id||':'||td.design_requirement_id||':'||td.task_id||':'||quote(td.checklist_item_id)||':'||td.status from task_derivations td join design_requirements r on r.id=td.design_requirement_id where r.design_version_id=?2
                    union all select 'checklist:'||id||':'||quote(title)||':'||status from checklists where work_unit_id=?1 and design_version_id=?2
                    union all select 'item:'||ci.id||':'||ci.project_id||':'||ci.checklist_id||':'||ci.design_requirement_id||':'||ci.task_id||':'||ci.item_order||':'||quote(ci.title)||':'||quote(ci.completion_condition)||':'||ci.status from checklist_items ci join checklists c on c.id=ci.checklist_id where c.work_unit_id=?1 and c.design_version_id=?2
                    union all select 'gate:'||id||':'||gate_key||':'||quote(template_id)||':'||quote(task_id)||':'||quote(design_requirement_id)||':'||quote(command)||':'||expected_result||':'||selected_before_edit||':'||status from validation_gates where work_unit_id=?1
                    union all select 'coverage:'||id||':'||quote(task_id)||':'||design_requirement_id||':'||status from coverage_items where work_unit_id=?1
                    union all select 'acceptance:'||id||':'||target_type||':'||quote(task_id)||':'||quote(checklist_item_id)||':'||quote(validation_gate_id)||':'||quote(coverage_item_id)||':'||acceptance_type||':'||status||':'||quote(approved_by_authority_event_id) from acceptance_records where project_id=1
                    union all select 'membership:'||phase_id||':'||task_id from work_phase_task_memberships where phase_id=?4
                    union all select 'token:'||id||':'||status from correction_tokens where closure_id=?5
                    union all select 'application:'||id from correction_transition_applications
                    union all select 'alias:'||id||':'||alias from correction_transition_aliases
                    order by snapshot
                )"#,
                params![work.work_unit_id, design.design_version_id, canonical_item, phase.phase_id, closure.closure_id],
                |row| row.get(0),
            ).unwrap();
            assert!(
                apply_correction_transition(temp.path(), closure.closure_id, 1, None, None)
                    .is_err()
            );
            assert_eq!(
                conn.query_row(
                    "select count(*) from correction_transition_applications",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                0
            );
            assert_eq!(
                conn.query_row(
                    "select count(*) from correction_transition_aliases",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                0
            );
            assert_eq!(
                conn.query_row(
                    "select count(*) from correction_tokens where closure_id=?1 and token_ordinal=1 and status='pending'",
                    params![closure.closure_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                1
            );
            assert_eq!(
                conn.query_row(
                    "select status from task_derivations where id=?1",
                    params![duplicate.task_derivation_id],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
                "active"
            );
            assert_eq!(
                conn.query_row(
                    "select count(*) from work_phase_task_memberships where phase_id=?1",
                    params![phase.phase_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                3
            );
            let after_transition: String = conn.query_row(
                r#"select group_concat(snapshot, '|') from (
                    select 'task:'||id||':'||quote(title)||':'||priority||':'||status||':'||source||':'||quote(details)||':'||quote(completion_condition) snapshot from tasks where work_unit_id=?1
                    union all select 'derivation:'||td.id||':'||td.design_requirement_id||':'||td.task_id||':'||quote(td.checklist_item_id)||':'||td.status from task_derivations td join design_requirements r on r.id=td.design_requirement_id where r.design_version_id=?2
                    union all select 'checklist:'||id||':'||quote(title)||':'||status from checklists where work_unit_id=?1 and design_version_id=?2
                    union all select 'item:'||ci.id||':'||ci.project_id||':'||ci.checklist_id||':'||ci.design_requirement_id||':'||ci.task_id||':'||ci.item_order||':'||quote(ci.title)||':'||quote(ci.completion_condition)||':'||ci.status from checklist_items ci join checklists c on c.id=ci.checklist_id where c.work_unit_id=?1 and c.design_version_id=?2
                    union all select 'gate:'||id||':'||gate_key||':'||quote(template_id)||':'||quote(task_id)||':'||quote(design_requirement_id)||':'||quote(command)||':'||expected_result||':'||selected_before_edit||':'||status from validation_gates where work_unit_id=?1
                    union all select 'coverage:'||id||':'||quote(task_id)||':'||design_requirement_id||':'||status from coverage_items where work_unit_id=?1
                    union all select 'acceptance:'||id||':'||target_type||':'||quote(task_id)||':'||quote(checklist_item_id)||':'||quote(validation_gate_id)||':'||quote(coverage_item_id)||':'||acceptance_type||':'||status||':'||quote(approved_by_authority_event_id) from acceptance_records where project_id=1
                    union all select 'membership:'||phase_id||':'||task_id from work_phase_task_memberships where phase_id=?4
                    union all select 'token:'||id||':'||status from correction_tokens where closure_id=?5
                    union all select 'application:'||id from correction_transition_applications
                    union all select 'alias:'||id||':'||alias from correction_transition_aliases
                    order by snapshot
                )"#,
                params![work.work_unit_id, design.design_version_id, canonical_item, phase.phase_id, closure.closure_id],
                |row| row.get(0),
            ).unwrap();
            assert_eq!(after_transition, before_transition);
            $restore.unwrap();
        }};
    }
    rejects_corruption_atomically!(
        conn.execute(
            "update tasks set source='user' where id=?1",
            params![canonical_task]
        ),
        conn.execute(
            "update tasks set source='design' where id=?1",
            params![canonical_task]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update checklist_items set task_id=?1 where id=?2",
            params![historical_task.task_id, canonical_item]
        ),
        conn.execute(
            "update checklist_items set task_id=?1 where id=?2",
            params![canonical_task, canonical_item]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update checklist_items set design_requirement_id=?1 where id=?2",
            params![matrix_requirement, canonical_item]
        ),
        conn.execute(
            "update checklist_items set design_requirement_id=(select design_requirement_id from task_derivations where id=?1) where id=?2",
            params![canonical_derivation, canonical_item]
        )
    );
    conn.execute_batch("drop trigger trg_checklist_item_project_update; pragma foreign_keys=off;")
        .unwrap();
    rejects_corruption_atomically!(
        conn.execute(
            "update checklist_items set project_id=999 where id=?1",
            params![canonical_item]
        ),
        conn.execute(
            "update checklist_items set project_id=1 where id=?1",
            params![canonical_item]
        )
    );
    conn.execute_batch(
        r#"pragma foreign_keys=on;
        create trigger trg_checklist_item_project_update
        before update of project_id, checklist_id, design_requirement_id, task_id on checklist_items
        for each row
        when new.project_id != (select project_id from checklists where id = new.checklist_id)
          or new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
          or new.project_id != coalesce(
              (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
              (select id from projects order by id limit 1)
          )
        begin
            select raise(abort, 'checklist item project_id must match referenced rows');
        end;"#,
    )
    .unwrap();
    conn.execute(
        "update checklist_items set task_id=?1 where id=?2",
        params![canonical_task, duplicate.checklist_item_id],
    )
    .unwrap();
    assert_eq!(
        conn.query_row(
            r#"select count(*) from checklist_items duplicate_ci
               where duplicate_ci.id=?1
                 and exists(select 1 from task_derivations canonical_td
                   where canonical_td.task_id=duplicate_ci.task_id
                     and canonical_td.design_requirement_id=duplicate_ci.design_requirement_id
                     and canonical_td.checklist_item_id!=duplicate_ci.id
                     and canonical_td.status='active')
                 and exists(select 1 from validation_gates vg where vg.task_id=duplicate_ci.task_id and vg.design_requirement_id=duplicate_ci.design_requirement_id and vg.status='active')
                 and exists(select 1 from coverage_items c where c.task_id=duplicate_ci.task_id and c.design_requirement_id=duplicate_ci.design_requirement_id and c.status!='stale')"#,
            params![duplicate.checklist_item_id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1,
        "fixture must share both active gate and live coverage with the canonical bundle"
    );
    let shared_error =
        apply_correction_transition(temp.path(), closure.closure_id, 1, None, None).unwrap_err();
    assert!(
        shared_error
            .to_string()
            .contains("shared gate or coverage nodes")
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update checklist_items set task_id=?1 where id=?2",
            params![canonical_task, duplicate.checklist_item_id]
        ),
        conn.execute(
            "update checklist_items set task_id=?1 where id=?2",
            params![duplicate_task.task_id, duplicate.checklist_item_id]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update task_derivations set checklist_item_id=null where id=?1",
            params![canonical_derivation]
        ),
        conn.execute(
            "update task_derivations set checklist_item_id=?1 where id=?2",
            params![canonical_item, canonical_derivation]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update task_derivations set task_id=?1 where id=?2",
            params![unrelated_task.task_id, canonical_derivation]
        ),
        conn.execute(
            "update task_derivations set task_id=?1 where id=?2",
            params![canonical_task, canonical_derivation]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update task_derivations set design_requirement_id=?1 where id=?2",
            params![matrix_requirement, canonical_derivation]
        ),
        conn.execute(
            "update task_derivations set design_requirement_id=(select design_requirement_id from checklist_items where id=?1) where id=?2",
            params![canonical_item, canonical_derivation]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update tasks set title='corrupt' where id=?1",
            params![canonical_task]
        ),
        conn.execute(
            "update tasks set title=?1 where id=?2",
            params![canonical_task_title, canonical_task]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update tasks set details='corrupt' where id=?1",
            params![canonical_task]
        ),
        conn.execute(
            "update tasks set details=?1 where id=?2",
            params![canonical_task_details, canonical_task]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update tasks set priority='low' where id=?1",
            params![canonical_task]
        ),
        conn.execute(
            "update tasks set priority=?1 where id=?2",
            params![canonical_task_priority, canonical_task]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update tasks set completion_condition='corrupt' where id=?1",
            params![canonical_task]
        ),
        conn.execute(
            "update tasks set completion_condition=?1 where id=?2",
            params![canonical_task_completion, canonical_task]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update checklist_items set item_order=2 where id=?1",
            params![canonical_item]
        ),
        conn.execute(
            "update checklist_items set item_order=?1 where id=?2",
            params![canonical_item_order, canonical_item]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update checklist_items set title='corrupt' where id=?1",
            params![canonical_item]
        ),
        conn.execute(
            "update checklist_items set title=?1 where id=?2",
            params![canonical_item_title, canonical_item]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update checklist_items set completion_condition='corrupt' where id=?1",
            params![canonical_item]
        ),
        conn.execute(
            "update checklist_items set completion_condition=?1 where id=?2",
            params![canonical_item_completion, canonical_item]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update validation_gates set status='closed' where id=?1",
            params![canonical_gate]
        ),
        conn.execute(
            "update validation_gates set status='active' where id=?1",
            params![canonical_gate]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update validation_gates set selected_before_edit=0 where id=?1",
            params![canonical_gate]
        ),
        conn.execute(
            "update validation_gates set selected_before_edit=1 where id=?1",
            params![canonical_gate]
        )
    );
    rejects_corruption_atomically!(
        conn.execute("update validation_gates set expected_result='corrupt' where id=?1", params![canonical_gate]),
        conn.execute("update validation_gates set expected_result=(select expected_result from validation_gate_templates where id=?2) where id=?1", params![canonical_gate, canonical_template])
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update validation_gates set gate_key='GATE-999' where id=?1",
            params![canonical_gate]
        ),
        conn.execute(
            "update validation_gates set gate_key='GATE-001' where id=?1",
            params![canonical_gate]
        )
    );
    rejects_corruption_atomically!(
        conn.execute("update validation_gates set command='corrupt' where id=?1", params![canonical_gate]),
        conn.execute("update validation_gates set command=(select command from validation_gate_templates where id=?2) where id=?1", params![canonical_gate, canonical_template])
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update validation_gates set template_id=null where id=?1",
            params![canonical_gate]
        ),
        conn.execute(
            "update validation_gates set template_id=?1 where id=?2",
            params![canonical_template, canonical_gate]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update validation_gate_templates set gate_hash='different-nonempty-hash' where id=?1",
            params![canonical_template]
        ),
        conn.execute(
            "update validation_gate_templates set gate_hash=?1 where id=?2",
            params![canonical_gate_hash, canonical_template]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update validation_gate_templates set gate_key='GATE-999' where id=?1",
            params![canonical_template]
        ),
        conn.execute(
            "update validation_gate_templates set gate_key='GATE-001' where id=?1",
            params![canonical_template]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update validation_gate_templates set command='corrupt' where id=?1",
            params![canonical_template]
        ),
        conn.execute(
            "update validation_gate_templates set command=null where id=?1",
            params![canonical_template]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update validation_gate_templates set expected_result='fail' where id=?1",
            params![canonical_template]
        ),
        conn.execute(
            "update validation_gate_templates set expected_result='pass' where id=?1",
            params![canonical_template]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update validation_gate_templates set requirement_keys='REQ-999' where id=?1",
            params![canonical_template]
        ),
        conn.execute(
            "update validation_gate_templates set requirement_keys='REQ-001' where id=?1",
            params![canonical_template]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update validation_gate_templates set status='superseded' where id=?1",
            params![canonical_template]
        ),
        conn.execute(
            "update validation_gate_templates set status='active' where id=?1",
            params![canonical_template]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update validation_gate_templates set stage='design-ready' where id=?1",
            params![canonical_template]
        ),
        conn.execute(
            "update validation_gate_templates set stage=?1 where id=?2",
            params![canonical_gate_stage, canonical_template]
        )
    );
    let canonical_gate_text: String = conn
        .query_row(
            "select gate_text from validation_gate_templates where id=?1",
            params![canonical_template],
            |row| row.get(0),
        )
        .unwrap();
    rejects_corruption_atomically!(
        conn.execute(
            "update validation_gate_templates set gate_text='different nonempty text' where id=?1",
            params![canonical_template]
        ),
        conn.execute(
            "update validation_gate_templates set gate_text=?1 where id=?2",
            params![canonical_gate_text, canonical_template]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update coverage_items set status='stale' where id=?1",
            params![canonical_coverage.coverage_item_id]
        ),
        conn.execute(
            "update coverage_items set status='covered' where id=?1",
            params![canonical_coverage.coverage_item_id]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update coverage_items set task_id=?1 where id=?2",
            params![historical_task.task_id, canonical_coverage.coverage_item_id]
        ),
        conn.execute(
            "update coverage_items set task_id=?1 where id=?2",
            params![canonical_task, canonical_coverage.coverage_item_id]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            r#"insert into coverage_items(project_id, review_scope_id, work_unit_id,
                design_requirement_id, task_id, requirement, runtime_boundary_evidence,
                ux_boundary_evidence, lifecycle_boundary_evidence, tests_or_gates,
                missing_or_unverified, status, created_at)
            select project_id, review_scope_id, work_unit_id, design_requirement_id, task_id,
                requirement, runtime_boundary_evidence, ux_boundary_evidence,
                lifecycle_boundary_evidence, tests_or_gates, missing_or_unverified,
                status, current_timestamp from coverage_items where id=?1"#,
            params![canonical_coverage.coverage_item_id]
        ),
        conn.execute("delete from coverage_items where work_unit_id=?1 and design_requirement_id=(select design_requirement_id from coverage_items where id=?2) and task_id=?3 and id!=?2", params![work.work_unit_id, canonical_coverage.coverage_item_id, canonical_task])
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update authority_events set scope='work-unit:999' where id=?1",
            params![provenance_authority.authority_event_id]
        ),
        conn.execute(
            "update authority_events set scope='project' where id=?1",
            params![provenance_authority.authority_event_id]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update acceptance_records set status='proposed' where checklist_item_id=?1",
            params![canonical_item]
        ),
        conn.execute(
            "update acceptance_records set status='approved' where checklist_item_id=?1",
            params![canonical_item]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update acceptance_records set status='rejected' where checklist_item_id=?1",
            params![canonical_item]
        ),
        conn.execute(
            "update acceptance_records set status='approved' where checklist_item_id=?1",
            params![canonical_item]
        )
    );
    rejects_corruption_atomically!(
        conn.execute(
            "update authority_events set event_type='review_result' where id=?1",
            params![provenance_authority.authority_event_id]
        ),
        conn.execute(
            "update authority_events set event_type='user_instruction' where id=?1",
            params![provenance_authority.authority_event_id]
        )
    );
    conn.execute(
        "delete from design_requirements where id=?1",
        params![matrix_requirement],
    )
    .unwrap();
    drop(conn);
    apply_correction_transition(temp.path(), closure.closure_id, 1, None, None).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let (reconcile_application, correction_session): (i64, i64) = conn
        .query_row(
            "select app.id, app.correction_session_id from correction_transition_applications app join correction_tokens token on token.id=app.correction_token_id where token.closure_id=?1 and token.token_ordinal=1",
            params![closure.closure_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert!(
        conn.execute(
            "insert into correction_transition_aliases(project_id, correction_session_id, correction_application_id, alias, record_type, record_id, created_at) values (1, ?1, ?2, ?3, 'task', ?4, current_timestamp)",
            params![correction_session, reconcile_application, format!("@superseded-task/{}", historical_task.task_id), historical_task.task_id],
        )
        .is_err()
    );
    assert!(
        conn.execute(
            "insert into correction_application_identity_links(project_id, correction_session_id, correction_application_id, link_kind, record_type, record_id, created_at) values (1, ?1, ?2, 'adopted', 'task', ?3, current_timestamp)",
            params![correction_session, reconcile_application, unrelated_task.task_id],
        )
        .is_err()
    );
    assert!(
        conn.execute(
            "insert into correction_application_identity_links(project_id, correction_session_id, correction_application_id, link_kind, record_type, record_id, created_at) values (1, ?1, ?2, 'superseded', 'task', ?3, current_timestamp)",
            params![correction_session, reconcile_application, canonical_task],
        )
        .is_err()
    );
    assert!(
        conn.execute(
            "insert into correction_application_identity_links(project_id, correction_session_id, correction_application_id, link_kind, record_type, record_id, created_at) values (1, ?1, ?2, 'adopted', 'task', 999999, current_timestamp)",
            params![correction_session, reconcile_application],
        )
        .is_err()
    );
    assert!(
        conn.execute(
            "insert into correction_application_identity_links(project_id, correction_session_id, correction_application_id, link_kind, record_type, record_id, created_at) values (1, ?1, ?2, 'created', 'task', ?3, current_timestamp)",
            params![correction_session, reconcile_application, canonical_task],
        )
        .is_err()
    );
    assert!(
        conn.execute(
            "insert into correction_application_identity_links(project_id, correction_session_id, correction_application_id, link_kind, record_type, record_id, created_at) values (1, ?1, ?2, 'superseded', 'checklist', ?3, current_timestamp)",
            params![correction_session, reconcile_application, historical.checklist_id],
        )
        .is_err()
    );
    drop(conn);
    apply_correction_transition(temp.path(), closure.closure_id, 2, None, None).unwrap();
    apply_correction_transition(temp.path(), closure.closure_id, 3, None, None).unwrap();
    let authority = add_authority_event(
        temp.path(),
        NewAuthorityEvent {
            event_type: "user_instruction",
            source: Some("test-user"),
            summary: "accept reconciled duplicate task",
            scope: Some("project"),
            precedence: 100,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        r#"insert into design_requirements(
            project_id, design_version_id, source_design_file_id, source_section,
            requirement_key, revision, requirement_hash, supersedes_requirement_id,
            requirement_text, priority, required_surfaces, validation_expectation,
            status, created_at)
        select project_id, design_version_id, source_design_file_id, 'REQ-998: ambiguity',
            'REQ-998', 1, 'ambiguity-requirement', null, 'ambiguity only', 'high',
            null, null, 'active', current_timestamp
        from design_requirements where design_version_id=?1 and requirement_key='REQ-001'"#,
        params![design.design_version_id],
    )
    .unwrap();
    let ambiguous_requirement = conn.last_insert_rowid();
    conn.execute(
        "insert into task_derivations(project_id, design_requirement_id, task_id, checklist_item_id, derivation_reason, status, created_at) values (1, ?1, ?2, null, 'ambiguity fixture', 'closed', current_timestamp)",
        params![ambiguous_requirement, duplicate_task.task_id],
    )
    .unwrap();
    let ambiguous_derivation = conn.last_insert_rowid();
    let applications_before_ambiguity: i64 = conn
        .query_row(
            "select count(*) from correction_transition_applications",
            [],
            |row| row.get(0),
        )
        .unwrap();
    drop(conn);
    assert!(
        apply_correction_transition(
            temp.path(),
            closure.closure_id,
            4,
            Some(authority.authority_event_id),
            None,
        )
        .unwrap_err()
        .to_string()
        .contains("ambiguous eligible derivations")
    );
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    assert_eq!(
        conn.query_row(
            "select count(*) from correction_transition_applications",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        applications_before_ambiguity
    );
    conn.execute(
        "update design_requirements set design_version_id=?1 where id=?2",
        params![foreign_design_version, ambiguous_requirement],
    )
    .unwrap();
    conn.execute(
        "insert into checklist_items(project_id, checklist_id, design_requirement_id, task_id, item_order, title, completion_condition, status) values (1, ?1, ?2, ?3, 999, 'untouched foreign item', 'unchanged', 'open')",
        params![duplicate.checklist_id, ambiguous_requirement, duplicate_task.task_id],
    )
    .unwrap();
    let untouched_item = conn.last_insert_rowid();
    conn.execute(
        "update task_derivations set checklist_item_id=?1 where id=?2",
        params![untouched_item, ambiguous_derivation],
    )
    .unwrap();
    conn.execute(
        r#"insert into validation_gates(project_id, gate_key, template_id, work_unit_id, task_id, design_requirement_id, command, expected_result, selected_before_edit, status, created_at)
            select project_id, 'GATE-UNTOUCHED', template_id, work_unit_id, task_id, ?1, command, expected_result, selected_before_edit, 'closed', current_timestamp
            from validation_gates where task_id=?2 limit 1"#,
        params![ambiguous_requirement, duplicate_task.task_id],
    )
    .unwrap();
    let untouched_gate = conn.last_insert_rowid();
    conn.execute(
        "insert into coverage_items(project_id, work_unit_id, design_requirement_id, task_id, requirement, status, created_at) values (1, ?1, ?2, ?3, 'untouched foreign coverage', 'accepted_out_of_scope', current_timestamp)",
        params![work.work_unit_id, ambiguous_requirement, duplicate_task.task_id],
    )
    .unwrap();
    let untouched_coverage = conn.last_insert_rowid();
    drop(conn);
    apply_correction_transition(
        temp.path(),
        closure.closure_id,
        4,
        Some(authority.authority_event_id),
        None,
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let acceptance_application: i64 = conn
        .query_row(
            "select app.id from correction_transition_applications app join correction_tokens token on token.id=app.correction_token_id where token.closure_id=?1 and token.token_ordinal=4",
            params![closure.closure_id],
            |row| row.get(0),
        )
        .unwrap();
    for (record_type, record_id) in [
        ("checklist_item", untouched_item),
        ("validation_gate", untouched_gate),
        ("coverage_item", untouched_coverage),
    ] {
        assert_eq!(
            conn.query_row(
                "select count(*) from correction_transition_aliases where correction_application_id=?1 and record_type=?2 and record_id=?3",
                params![acceptance_application, record_type, record_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            0
        );
        assert_eq!(
            conn.query_row(
                "select count(*) from correction_application_identity_links where correction_application_id=?1 and record_type=?2 and record_id=?3",
                params![acceptance_application, record_type, record_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            0
        );
    }
    conn.execute(
        "update checklist_items set status='accepted_out_of_scope' where id=?1",
        params![untouched_item],
    )
    .unwrap();
    assert!(
        conn.execute(
            "insert into correction_transition_aliases(project_id, correction_session_id, correction_application_id, alias, record_type, record_id, created_at) values (1, ?1, ?2, ?3, 'checklist_item', ?4, current_timestamp)",
            params![correction_session, acceptance_application, format!("@accepted-checklist_item/{untouched_item}"), untouched_item],
        )
        .is_err()
    );
    let phase_application: i64 = conn
        .query_row(
            "select app.id from correction_transition_applications app join correction_tokens token on token.id=app.correction_token_id where token.closure_id=?1 and token.token_ordinal=3",
            params![closure.closure_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        conn.query_row(
            "select count(*) from correction_application_identity_links where correction_application_id=?1 and link_kind='superseded' and record_type in ('checklist','task_derivation','checklist_item','validation_gate','coverage_item')",
            params![reconcile_application],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        5
    );
    assert_eq!(
        conn.query_row(
            "select count(*) from correction_application_identity_links where correction_application_id=?1 and link_kind='adopted' and record_type in ('checklist','task','task_derivation','checklist_item','validation_gate','coverage_item')",
            params![reconcile_application],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        6
    );
    assert_eq!(
        conn.query_row(
            "select count(*) from correction_application_identity_links where correction_application_id=?1 and link_kind in ('membership_removed','membership_assigned') and record_type='phase_membership'",
            params![phase_application],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        2
    );
    let unchanged_membership: i64 = conn
        .query_row(
            "select id from work_phase_task_memberships where phase_id=?1 and task_id=?2",
            params![phase.phase_id, historical_task.task_id],
            |row| row.get(0),
        )
        .unwrap();
    for forged_kind in ["membership_removed", "membership_assigned"] {
        assert!(
            conn.execute(
                "insert into correction_application_identity_links(project_id, correction_session_id, correction_application_id, link_kind, record_type, record_id, created_at) values (1, ?1, ?2, ?3, 'phase_membership', ?4, current_timestamp)",
                params![correction_session, phase_application, forged_kind, unchanged_membership],
            )
            .is_err()
        );
    }
    assert!(
        conn.execute(
            "insert into correction_application_identity_links(project_id, correction_session_id, correction_application_id, link_kind, record_type, record_id, created_at) values (1, ?1, ?2, 'adopted', 'phase_membership', ?3, current_timestamp)",
            params![correction_session, phase_application, unchanged_membership],
        )
        .is_err()
    );
    assert!(
        conn.execute(
            "insert into correction_application_identity_links(project_id, correction_session_id, correction_application_id, link_kind, record_type, record_id, created_at) values (1, ?1, ?2, 'membership_removed', 'task', ?3, current_timestamp)",
            params![correction_session, phase_application, canonical_task],
        )
        .is_err()
    );
    assert!(
        conn.execute(
            "insert into correction_application_identity_links(project_id, correction_session_id, correction_application_id, link_kind, record_type, record_id, created_at) values (999, ?1, ?2, 'adopted', 'task', ?3, current_timestamp)",
            params![correction_session, reconcile_application, canonical_task],
        )
        .is_err()
    );
    let duplicate_status: String = conn
        .query_row(
            "select status from task_derivations where id=?1",
            params![duplicate.task_derivation_id],
            |row| row.get(0),
        )
        .unwrap();
    let superseded_aliases: i64 = conn
        .query_row(
            "select count(*) from correction_transition_aliases where alias=?1 and record_id=?2",
            params![
                format!("@superseded-task/{}", duplicate_task.task_id),
                duplicate_task.task_id
            ],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(duplicate_status, "closed");
    assert_eq!(superseded_aliases, 1);
    assert_eq!(
        conn.query_row(
            "select count(*) from correction_transition_aliases where correction_application_id=?1 and alias='@derivation/REQ-001' and record_id=?2",
            params![reconcile_application, canonical_derivation],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row(
            "select count(*) from correction_transition_aliases where correction_application_id=?1 and record_type='task_derivation' and record_id=?2",
            params![reconcile_application, historical.task_derivation_id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        0,
        "retained closed canonical history must not be exported as an active alias"
    );
    assert_eq!(
        conn.query_row(
            "select count(*) from correction_transition_aliases where alias=?1",
            params![format!("@superseded-task/{}", historical_task.task_id)],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row(
            "select status from tasks where id=?1",
            params![duplicate_task.task_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "accepted_out_of_scope"
    );
    assert_eq!(
        conn.query_row(
            "select count(*) from work_phase_task_memberships where phase_id=?1 and task_id=?2",
            params![phase.phase_id, canonical_task],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row(
            "select count(*) from work_phase_task_memberships where phase_id=?1 and task_id=?2",
            params![phase.phase_id, historical_task.task_id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row(
            "select count(*) from work_phase_task_memberships where phase_id=?1 and task_id=?2",
            params![phase.phase_id, stale_task.task_id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1,
        "a pre-existing approved stale acceptance is not current-session eligibility"
    );
    assert_eq!(
        conn.query_row(
            "select count(*) from task_derivations td join design_requirements r on r.id=td.design_requirement_id where r.design_version_id=?1 and td.status='active'",
            params![design.design_version_id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
    conn.execute_batch("drop trigger trg_correction_alias_immutable_update;")
        .unwrap();
    conn.execute(
        "update correction_transition_aliases set alias='@numeric-disposition-proof' where correction_application_id=?1 and record_type='task' and record_id=?2",
        params![reconcile_application, duplicate_task.task_id],
    )
    .unwrap();
    assert_eq!(
        conn.query_row(
            "select count(*) from correction_transition_aliases where correction_session_id=?1 and alias like '@superseded-task/%'",
            params![correction_session],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        0,
        "ready drift checks below must be proved by the numeric token branch, not aliases"
    );
    let (acceptance_application, exact_acceptance_result): (i64, String) = conn
        .query_row(
            "select app.id, app.result_ref from correction_transition_applications app join correction_tokens token on token.id=app.correction_token_id where token.closure_id=?1 and token.token_ordinal=4",
            params![closure.closure_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    conn.execute_batch("drop trigger trg_correction_application_links_update;")
        .unwrap();
    conn.execute(
        "update correction_transition_applications set result_ref='task:malformed:acceptance:proof' where id=?1",
        params![acceptance_application],
    )
    .unwrap();
    let attempts_before_malformed: i64 = conn
        .query_row(
            "select count(*) from closure_attempts where closure_id=?1",
            params![closure.closure_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        ready_closure(
            temp.path(),
            ClosureReady {
                closure_id: closure.closure_id,
                implementation_evidence: "malformed proof must reject",
                tests_or_gates: "exact numeric acceptance proof",
                closed_by_commit: None,
            },
        )
        .unwrap_err()
        .to_string()
        .contains("all reconciled duplicate tasks, memberships, and derivations")
    );
    assert_eq!(
        conn.query_row(
            "select count(*) from closure_attempts where closure_id=?1",
            params![closure.closure_id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        attempts_before_malformed
    );
    conn.execute(
        "update correction_transition_applications set result_ref=?1 where id=?2",
        params![exact_acceptance_result, acceptance_application],
    )
    .unwrap();
    conn.execute_batch(
        "create trigger trg_correction_application_links_update before update on correction_transition_applications begin select raise(abort, 'correction transition applications are immutable'); end;",
    )
    .unwrap();
    macro_rules! rejects_ready_drift {
        ($corrupt:expr, $restore:expr) => {{
            $corrupt.unwrap();
            let attempts_before: i64 = conn
                .query_row(
                    "select count(*) from closure_attempts where closure_id=?1",
                    params![closure.closure_id],
                    |row| row.get(0),
                )
                .unwrap();
            let error = ready_closure(
                temp.path(),
                ClosureReady {
                    closure_id: closure.closure_id,
                    implementation_evidence: "drift must reject",
                    tests_or_gates: "recovery postcondition drift",
                    closed_by_commit: None,
                },
            )
            .unwrap_err();
            assert!(
                error
                    .to_string()
                    .contains("all reconciled duplicate tasks, memberships, and derivations")
            );
            assert_eq!(
                conn.query_row(
                    "select count(*) from closure_attempts where closure_id=?1",
                    params![closure.closure_id],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
                attempts_before
            );
            $restore.unwrap();
        }};
    }
    rejects_ready_drift!(
        conn.execute(
            "update tasks set status='open' where id=?1",
            params![duplicate_task.task_id]
        ),
        conn.execute(
            "update tasks set status='accepted_out_of_scope' where id=?1",
            params![duplicate_task.task_id]
        )
    );
    rejects_ready_drift!(
        conn.execute(
            "insert into work_phase_task_memberships(project_id, phase_id, task_id, assigned_at) values (1, ?1, ?2, current_timestamp)",
            params![phase.phase_id, duplicate_task.task_id]
        ),
        conn.execute(
            "delete from work_phase_task_memberships where phase_id=?1 and task_id=?2",
            params![phase.phase_id, duplicate_task.task_id]
        )
    );
    rejects_ready_drift!(
        conn.execute(
            "update task_derivations set status='active' where id=?1",
            params![duplicate.task_derivation_id]
        ),
        conn.execute(
            "update task_derivations set status='closed' where id=?1",
            params![duplicate.task_derivation_id]
        )
    );
    conn.execute(
        "update correction_transition_aliases set alias=?1 where correction_application_id=?2 and record_type='task' and record_id=?3",
        params![format!("@superseded-task/{}", duplicate_task.task_id), reconcile_application, duplicate_task.task_id],
    )
    .unwrap();
    conn.execute_batch(
        "create trigger trg_correction_alias_immutable_update before update on correction_transition_aliases begin select raise(abort, 'correction transition aliases are immutable'); end;",
    )
    .unwrap();
    drop(conn);
    ready_closure(
        temp.path(),
        ClosureReady {
            closure_id: closure.closure_id,
            implementation_evidence: "canonical graph reconciled",
            tests_or_gates: "duplicate task disposition and phase replacement verified",
            closed_by_commit: None,
        },
    )
    .unwrap();
}

#[test]
fn stale_close_disposes_selected_validation_gate() {
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
    let derivation = derive_task_from_requirement(
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
    let make_phase = |key: &str, order: i64| {
        create_phase(
            temp.path(),
            NewWorkPhase {
                work_unit_id: work.work_unit_id,
                design_version_id: Some(first_import.design_version_id),
                key,
                title: key,
                kind: "implementation",
                order,
                reason: None,
            },
        )
        .unwrap()
    };
    let prerequisite_a = make_phase("prerequisite-a", 10);
    let prerequisite_b = make_phase("prerequisite-b", 11);
    let prerequisite_c = make_phase("prerequisite-c", 12);
    let prerequisite_d = make_phase("prerequisite-d", 13);
    let satisfy_dependency = add_phase_dependency(
        temp.path(),
        NewPhaseDependency {
            from_phase_id: prerequisite_a.phase_id,
            to_phase_id: prerequisite_b.phase_id,
            dependency_type: "blocks",
            reason: "satisfy through correction",
        },
    )
    .unwrap();
    let accept_dependency = add_phase_dependency(
        temp.path(),
        NewPhaseDependency {
            from_phase_id: prerequisite_c.phase_id,
            to_phase_id: prerequisite_d.phase_id,
            dependency_type: "requires",
            reason: "accept through correction",
        },
    )
    .unwrap();
    suspend_work(
        temp.path(),
        "verify global stale selection without an active work unit",
        "apply the declared stale transition",
    )
    .unwrap();
    let correction_plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(first_import.design_version_id),
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
            result_summary: Some("gate will require mediated stale disposal"),
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
    let correction_finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: correction_run.review_run_id,
            finding_type: "design_finding",
            severity: "high",
            description: "dispose the stale gate through the correction contract",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), correction_finding.finding_id, "valid").unwrap();
    let stale_surface = format!(
        "transition:phase-dependency-satisfy:{},transition:phase-dependency-accept:{},transition:phase-create:{}/{}/@foundation/implementation/1/foundation,transition:phase-create:{}/{}/@verification/validation/2/verification,transition:phase-assign:@foundation/{},transition:phase-dependency-add:@foundation/@verification/blocks,transition:stale-close:validation_gate/{}",
        satisfy_dependency.dependency_id,
        accept_dependency.dependency_id,
        work.work_unit_id,
        first_import.design_version_id,
        work.work_unit_id,
        first_import.design_version_id,
        task.task_id,
        gate.validation_gate_id
    );
    let reversed_stale_surfaces = format!(
        "transition:stale-close:validation_gate/{},transition:stale-accept:checklist/{}",
        gate.validation_gate_id, derivation.checklist_id
    );
    assert!(
        add_closure(
            temp.path(),
            NewClosure {
                finding_id: correction_finding.finding_id,
                design_invariant: "stale transitions follow the global tuple",
                design_citations: None,
                implementation_evidence: None,
                affected_surfaces: Some(&reversed_stale_surfaces),
                same_invariant_search: None,
                other_violations_found: None,
                fix_plan: Some("apply stale transitions"),
                tests_or_gates: Some("stale inventory"),
                verification_plan: Some("resume design review"),
                closed_by_commit: None,
            },
        )
        .is_err()
    );
    let correction_closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: correction_finding.finding_id,
            design_invariant: "stale gate is disposed through an audited transition",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some(&stale_surface),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("apply the declared stale transition"),
            tests_or_gates: Some("stale inventory"),
            verification_plan: Some("resume design review"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    begin_correction(temp.path(), correction_closure.closure_id).unwrap();
    assert!(
        apply_correction_transition(temp.path(), correction_closure.closure_id, 1, None, None)
            .unwrap_err()
            .to_string()
            .contains("requires --evidence")
    );
    apply_correction_transition(
        temp.path(),
        correction_closure.closure_id,
        1,
        None,
        Some("validation-run:dependency-satisfied"),
    )
    .unwrap();
    assert!(
        apply_correction_transition(temp.path(), correction_closure.closure_id, 2, None, None)
            .unwrap_err()
            .to_string()
            .contains("requires --authority")
    );
    let dependency_authority = add_authority_event(
        temp.path(),
        NewAuthorityEvent {
            event_type: "user_instruction",
            source: Some("test-user"),
            summary: "accept exact phase dependency",
            scope: Some("project"),
            precedence: 100,
        },
    )
    .unwrap()
    .authority_event_id;
    apply_correction_transition(
        temp.path(),
        correction_closure.closure_id,
        2,
        Some(dependency_authority),
        None,
    )
    .unwrap();
    for token in 3..=6 {
        apply_correction_transition(
            temp.path(),
            correction_closure.closure_id,
            token,
            None,
            None,
        )
        .unwrap();
    }
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let changed_audits: i64 = conn
        .query_row(
            "select count(*) from correction_transition_applications where correction_session_id=(select id from correction_sessions where closure_id=?1) and before_state != after_state",
            params![correction_closure.closure_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(changed_audits, 6);
    drop(conn);
    fs::write(
        init.package_path.join("validation").join("gates.md"),
        validation_gate_doc("GATE-001").replace(
            "Run the project test suite",
            "Run the complete project test suite",
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
    implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(second_import.design_version_id),
        },
    )
    .unwrap();
    assert!(
        add_task(
            temp.path(),
            NewTask {
                title: "must not cross stale selection",
                priority: "high",
                source: "manual",
                work_unit_id: Some(work.work_unit_id),
                details: None,
                completion_condition: Some("stale is resolved first"),
            },
        )
        .unwrap_err()
        .to_string()
        .contains("selected lifecycle action")
    );
    let bootstrap = project_status(temp.path()).unwrap().phase_blocker.unwrap();
    assert_eq!(bootstrap.work_unit_id, None);
    assert!(bootstrap.next_action.contains(&format!(
        "closure transition apply {} --token 7",
        correction_closure.closure_id
    )));
    let transition =
        apply_correction_transition(temp.path(), correction_closure.closure_id, 7, None, None)
            .unwrap();
    assert!(!transition.idempotent);
    assert!(
        apply_correction_transition(temp.path(), correction_closure.closure_id, 7, None, None)
            .unwrap_err()
            .to_string()
            .contains("selected transition")
    );
    let stale = list_stale_records(temp.path()).unwrap();
    assert!(!stale.iter().any(|record| {
        record.record_type == "validation_gate" && record.id == gate.validation_gate_id
    }));
    let conn = open_ledger(&temp.path().join(".agent-workbench").join("ledger.sqlite")).unwrap();
    let gate_status: String = conn
        .query_row(
            "select status from validation_gates where id = ?1",
            rusqlite::params![gate.validation_gate_id],
            |row| row.get(0),
        )
        .unwrap();
    let acceptance_type: String = conn
        .query_row(
            "select acceptance_type from acceptance_records where target_type = 'stale_record' and stale_record_type = 'validation_gate' and stale_record_id = ?1 order by id desc limit 1",
            rusqlite::params![gate.validation_gate_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(gate_status, "closed");
    assert_eq!(acceptance_type, "stale_accepted");
}

#[test]
fn mediated_task_carry_forward_requires_verified_baseline_and_is_atomic() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "carry unchanged baseline", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "carry cleanup requirement",
            priority: "high",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("cleanup remains validated"),
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
        format!(
            "{}{}",
            validation_gate_doc("GATE-001"),
            validation_gate_doc("GATE-002")
        ),
    )
    .unwrap();
    let baseline = import_design_package(
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
            design_version_id: baseline.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    let baseline_derivation = derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: baseline.design_version_id,
            requirement_key: "REQ-001",
            task_id: task.task_id,
            derivation_reason: Some("verified baseline"),
            checklist_title: Some("Baseline checklist"),
            item_title: None,
            completion_condition: None,
        },
    )
    .unwrap();
    let baseline_gate = select_validation_gate(
        temp.path(),
        ValidationGateSelection {
            design_version_id: baseline.design_version_id,
            gate_key: "GATE-001",
            requirement_key: "REQ-001",
            task_id: task.task_id,
            command: None,
            command_profile: None,
            timeout: None,
        },
    )
    .unwrap();
    let baseline_gate_2 = select_validation_gate(
        temp.path(),
        ValidationGateSelection {
            design_version_id: baseline.design_version_id,
            gate_key: "GATE-002",
            requirement_key: "REQ-001",
            task_id: task.task_id,
            command: None,
            command_profile: None,
            timeout: None,
        },
    )
    .unwrap();
    add_validation_run(
        temp.path(),
        NewValidationRun {
            validation_gate_id: baseline_gate.validation_gate_id,
            command_usage_id: None,
            repository_snapshot_id: None,
            result: "pass",
            command: None,
            classification: None,
            acceptance_record_id: None,
            artifact_path: None,
            artifact_hash: None,
            notes: Some("authoritative baseline pass"),
        },
    )
    .unwrap();
    add_validation_run(
        temp.path(),
        NewValidationRun {
            validation_gate_id: baseline_gate_2.validation_gate_id,
            command_usage_id: None,
            repository_snapshot_id: None,
            result: "pass",
            command: None,
            classification: None,
            acceptance_record_id: None,
            artifact_path: None,
            artifact_hash: None,
            notes: Some("second authoritative baseline pass"),
        },
    )
    .unwrap();
    fs::write(
        init.package_path.join("01-introduction-goals.md"),
        "# Introduction And Goals\n\nUnrelated wording changed.\n",
    )
    .unwrap();
    let current = import_design_package(
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
            design_version_id: current.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    let ready_plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(current.design_version_id),
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
                current.design_version_id, work.work_unit_id
            )),
            prompt_deviations: None,
            result_summary: Some("unchanged current design is ready for decomposition"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: Some("design-reviewer"),
            external_agent_id: Some("design-reviewer-carry"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:design-ready-carry"),
        },
    )
    .unwrap();
    assert_eq!(
        list_task_derivations(
            temp.path(),
            TaskDerivationListQuery {
                design_version_id: baseline.design_version_id,
                work_unit_id: Some(work.work_unit_id),
            },
        )
        .unwrap()
        .into_iter()
        .find(|record| record.id == baseline_derivation.task_derivation_id)
        .unwrap()
        .status,
        "active"
    );
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(current.design_version_id),
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
    let run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("work_unit:1"),
            prompt_deviations: None,
            result_summary: Some("unchanged baseline should be carried explicitly"),
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
            finding_type: "design_finding",
            severity: "high",
            description: "record verified baseline carry-forward",
            design_requirement_id: None,
            task_id: Some(task.task_id),
        },
    )
    .unwrap();
    classify_finding(temp.path(), finding.finding_id, "valid").unwrap();
    let surface = format!(
        "transition:design-decompose:{}/{},transition:task-accept-out-of-scope:@task/REQ-001",
        current.design_version_id, work.work_unit_id
    );
    let closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: finding.finding_id,
            design_invariant: "unchanged verified baseline is carried with authority",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some(&surface),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("apply the verified carry-forward bundle"),
            tests_or_gates: Some("baseline GATE-001 pass"),
            verification_plan: Some("resume design review"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    begin_correction(temp.path(), closure.closure_id).unwrap();
    apply_correction_transition(temp.path(), closure.closure_id, 1, None, None).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let current_gate: i64 = conn
        .query_row(
            "select record_id from correction_transition_aliases where alias='@gate/REQ-001/GATE-001'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let current_gate_2: i64 = conn
        .query_row(
            "select record_id from correction_transition_aliases where alias='@gate/REQ-001/GATE-002'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let carried_task: i64 = conn
        .query_row(
            "select record_id from correction_transition_aliases where alias='@task/REQ-001'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(carried_task, task.task_id);
    drop(conn);
    let authority = add_authority_event(
        temp.path(),
        NewAuthorityEvent {
            event_type: "user_instruction",
            source: Some("test-user"),
            summary: "carry unchanged verified requirement",
            scope: Some("project"),
            precedence: 100,
        },
    )
    .unwrap();
    for scope in [
        "requirement:REQ-001".to_string(),
        format!("work-unit:{}", work.work_unit_id),
    ] {
        let scoped = add_authority_event(
            temp.path(),
            NewAuthorityEvent {
                event_type: "user_instruction",
                source: Some("test-user"),
                summary: "validate exact carry authority scope",
                scope: Some(&scope),
                precedence: 100,
            },
        )
        .unwrap();
        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        crate::planning::ensure_verified_baseline_carry_forward(
            &conn,
            crate::db::project_id(&conn).unwrap(),
            task.task_id,
            Some(work.work_unit_id),
            scoped.authority_event_id,
        )
        .unwrap();
    }
    let wrong_authority = add_authority_event(
        temp.path(),
        NewAuthorityEvent {
            event_type: "user_instruction",
            source: Some("test-user"),
            summary: "wrong carry authority scope",
            scope: Some("requirement:REQ-999"),
            precedence: 100,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    assert!(
        crate::planning::ensure_verified_baseline_carry_forward(
            &conn,
            crate::db::project_id(&conn).unwrap(),
            task.task_id,
            Some(work.work_unit_id),
            wrong_authority.authority_event_id,
        )
        .unwrap_err()
        .to_string()
        .contains("exact requirement or work unit")
    );
    let current_requirement: (i64, String, Option<String>) = conn
        .query_row(
            "select r.id, r.requirement_hash, r.required_surfaces from design_requirements r where r.design_version_id=?1 and r.requirement_key='REQ-001'",
            params![current.design_version_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    conn.execute(
        "update design_requirements set requirement_hash='changed' where id=?1",
        params![current_requirement.0],
    )
    .unwrap();
    assert!(
        crate::planning::ensure_verified_baseline_carry_forward(
            &conn,
            crate::db::project_id(&conn).unwrap(),
            task.task_id,
            Some(work.work_unit_id),
            authority.authority_event_id,
        )
        .unwrap_err()
        .to_string()
        .contains("normalized hash")
    );
    conn.execute(
        "update design_requirements set requirement_hash=?1, required_surfaces='cli' where id=?2",
        params![current_requirement.1, current_requirement.0],
    )
    .unwrap();
    assert!(
        crate::planning::ensure_verified_baseline_carry_forward(
            &conn,
            crate::db::project_id(&conn).unwrap(),
            task.task_id,
            Some(work.work_unit_id),
            authority.authority_event_id,
        )
        .unwrap_err()
        .to_string()
        .contains("required surfaces")
    );
    conn.execute(
        "update design_requirements set required_surfaces=?1 where id=?2",
        params![current_requirement.2, current_requirement.0],
    )
    .unwrap();
    let current_gate_template: (i64, String) = conn
        .query_row(
            "select id, gate_hash from validation_gate_templates where design_version_id=?1 and gate_key='GATE-002'",
            params![current.design_version_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    conn.execute(
        "update validation_gate_templates set gate_hash='changed' where id=?1",
        params![current_gate_template.0],
    )
    .unwrap();
    assert!(
        crate::planning::ensure_verified_baseline_carry_forward(
            &conn,
            crate::db::project_id(&conn).unwrap(),
            task.task_id,
            Some(work.work_unit_id),
            authority.authority_event_id,
        )
        .unwrap_err()
        .to_string()
        .contains("gate set changed")
    );
    conn.execute(
        "update validation_gate_templates set gate_hash=?1 where id=?2",
        params![current_gate_template.1, current_gate_template.0],
    )
    .unwrap();
    drop(conn);
    add_validation_run(
        temp.path(),
        NewValidationRun {
            validation_gate_id: baseline_gate.validation_gate_id,
            command_usage_id: None,
            repository_snapshot_id: None,
            result: "fail",
            command: None,
            classification: None,
            acceptance_record_id: None,
            artifact_path: None,
            artifact_hash: None,
            notes: Some("latest baseline run must win over the earlier pass"),
        },
    )
    .unwrap();
    assert!(
        apply_correction_transition(
            temp.path(),
            closure.closure_id,
            2,
            Some(authority.authority_event_id),
            None,
        )
        .unwrap_err()
        .to_string()
        .contains("latest authoritative passing run")
    );
    assert_eq!(
        list_tasks(
            temp.path(),
            TaskListQuery {
                status: None,
                work_unit_id: None,
            },
        )
        .unwrap()
        .into_iter()
        .find(|record| record.id == task.task_id)
        .unwrap()
        .status,
        "open"
    );
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let partially_changed: i64 = conn
        .query_row(
            "select (select count(*) from checklist_items where task_id=?1 and status!='open') + (select count(*) from validation_gates where task_id=?1 and design_requirement_id=(select design_requirement_id from task_derivations where task_id=?1 and status='active') and status!='active') + (select count(*) from coverage_items where task_id=?1 and status='accepted_out_of_scope')",
            params![task.task_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(partially_changed, 0);
    drop(conn);
    add_validation_run(
        temp.path(),
        NewValidationRun {
            validation_gate_id: baseline_gate.validation_gate_id,
            command_usage_id: None,
            repository_snapshot_id: None,
            result: "pass",
            command: None,
            classification: None,
            acceptance_record_id: None,
            artifact_path: None,
            artifact_hash: None,
            notes: Some("latest authoritative baseline pass restores eligibility"),
        },
    )
    .unwrap();
    apply_correction_transition(
        temp.path(),
        closure.closure_id,
        2,
        Some(authority.authority_event_id),
        None,
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let state: (String, String, i64, i64) = conn
        .query_row(
            "select t.status, ci.status, (select count(*) from validation_gates where id in (?2,?3) and status='closed'), (select count(*) from coverage_items c join acceptance_records ar on ar.coverage_item_id=c.id and ar.status='approved' where c.task_id=t.id) from tasks t join checklist_items ci on ci.task_id=t.id and ci.checklist_id=(select max(id) from checklists where work_unit_id=?1) where t.id=?4",
            params![work.work_unit_id, current_gate, current_gate_2, task.task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        state,
        (
            "accepted_out_of_scope".to_string(),
            "accepted_out_of_scope".to_string(),
            2,
            1,
        )
    );
    let baseline_state: (String, String) = conn
        .query_row(
            "select (select status from validation_gates where id=?1), (select status from validation_gates where id=?2)",
            params![baseline_gate.validation_gate_id, baseline_gate_2.validation_gate_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(baseline_state, ("active".to_string(), "active".to_string()));
}

#[test]
fn mediated_task_carry_forward_rejects_ambiguous_and_protected_derivations() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "reject ambiguous carry", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "shared task must stay in scope",
            priority: "critical",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("both requirements remain implemented"),
        },
    )
    .unwrap();
    let init = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "protected-carry",
            title: "Protected Carry",
        },
    )
    .unwrap();
    fs::write(
        init.package_path.join("requirements").join("README.md"),
        format!(
            "{}\n{}",
            requirement_doc("REQ-001", "Unchanged baseline candidate", "high"),
            requirement_doc("REQ-020", "Protected source correction", "critical")
        ),
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
    for requirement_key in ["REQ-001", "REQ-020"] {
        derive_task_from_requirement(
            temp.path(),
            NewTaskDerivation {
                design_version_id: imported.design_version_id,
                requirement_key,
                task_id: task.task_id,
                derivation_reason: Some("supported shared-task derivation"),
                checklist_title: Some("Shared protected checklist"),
                item_title: None,
                completion_condition: None,
            },
        )
        .unwrap();
    }
    let authority = add_authority_event(
        temp.path(),
        NewAuthorityEvent {
            event_type: "user_instruction",
            source: Some("test-user"),
            summary: "attempt ambiguous carry",
            scope: Some("project"),
            precedence: 100,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    assert!(
        crate::planning::ensure_verified_baseline_carry_forward(
            &conn,
            crate::db::project_id(&conn).unwrap(),
            task.task_id,
            Some(work.work_unit_id),
            authority.authority_event_id,
        )
        .unwrap_err()
        .to_string()
        .contains("exactly one active design derivation")
    );
    let status: String = conn
        .query_row(
            "select status from tasks where id=?1",
            params![task.task_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "open");
}

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
            .any(|item| { item.name == "coverage_items_current" && item.result == "fail" })
    );
    let stale = list_stale_records(temp.path()).unwrap();
    assert!(
        stale
            .iter()
            .any(|record| record.record_type == "task_derivation")
    );
    assert!(stale.iter().any(|record| record.record_type == "checklist"));
    assert!(
        stale
            .iter()
            .any(|record| record.record_type == "coverage_item")
    );
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
            .any(|item| { item.name == "coverage_items_current" && item.result == "pass" })
    );
}

#[test]
fn design_exception_acceptance_targets_requirements_and_gate_templates() {
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

    let approval_authority_event_id = approval_authority_event(temp.path());
    let requirement_acceptance = accept_design_exception(
        temp.path(),
        NewDesignExceptionAcceptance {
            design_version_id: Some(import.design_version_id),
            design_package: None,
            target: "requirement:REQ-001",
            acceptance_type: "accepted_out_of_scope",
            reason: "not needed for current scope",
            approval_authority_event_id,
        },
    )
    .unwrap();
    let gate_acceptance = accept_design_exception(
        temp.path(),
        NewDesignExceptionAcceptance {
            design_version_id: Some(import.design_version_id),
            design_package: None,
            target: "gate:GATE-001",
            acceptance_type: "explicit_exception",
            reason: "manual validation for this draft",
            approval_authority_event_id,
        },
    )
    .unwrap();
    let requirements = list_design_requirements(
        temp.path(),
        DesignRequirementListQuery {
            design_version_id: import.design_version_id,
        },
    )
    .unwrap();

    assert_eq!(requirement_acceptance.target_type, "design_requirement");
    assert!(requirement_acceptance.design_requirement_id.is_some());
    assert_eq!(requirements[0].status, "accepted_out_of_scope".to_string());
    assert_eq!(gate_acceptance.target_type, "validation_gate_template");
    assert!(gate_acceptance.validation_gate_template_id.is_some());
}

#[test]
fn design_exception_acceptance_rejects_review_result_authority() {
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
    let review_event = add_authority_event(
        temp.path(),
        NewAuthorityEvent {
            event_type: "review_result",
            source: Some("review-agent"),
            summary: "review suggests accepting this exception",
            scope: Some("storage-lifecycle"),
            precedence: 100,
        },
    )
    .unwrap();

    let acceptance = accept_design_exception(
        temp.path(),
        NewDesignExceptionAcceptance {
            design_version_id: Some(import.design_version_id),
            design_package: None,
            target: "requirement:REQ-001",
            acceptance_type: "accepted_out_of_scope",
            reason: "review-only approval must not be enough",
            approval_authority_event_id: review_event.authority_event_id,
        },
    );

    assert!(acceptance.is_err());
}

#[test]
fn design_import_requires_revision_for_changed_requirement_identity() {
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
    import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &init.package_path,
            status: "draft",
        },
    )
    .unwrap();

    fs::write(
        init.package_path.join("requirements").join("README.md"),
        requirement_doc("REQ-001", "Preserve cleanup behavior", "high")
            .replace("one verifiable behavior", "a changed verifiable behavior"),
    )
    .unwrap();
    assert!(
        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &init.package_path,
                status: "draft",
            },
        )
        .is_err()
    );

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

This requirement describes a changed verifiable behavior that must be implemented.
"#,
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
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let supersedes_count: i64 = conn
        .query_row(
            r#"
            select count(*)
            from design_requirements
            where design_version_id = ?1 and supersedes_requirement_id is not null
            "#,
            params![import.design_version_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(supersedes_count, 1);
}

#[test]
fn design_import_accepts_explicit_requirement_supersession_link() {
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
    import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &init.package_path,
            status: "draft",
        },
    )
    .unwrap();
    fs::write(
        init.package_path.join("requirements").join("README.md"),
        r#"## REQ-002: Preserve cleanup behavior with explicit scope
```yaml agent-workbench
type: requirement
key: REQ-002
priority: high
surfaces: [cli, database]
validation: [GATE-001]
supersedes: [REQ-001]
status: active
```

This requirement replaces the previous cleanup behavior with explicit scope.
"#,
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
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let supersedes_key: String = conn
        .query_row(
            r#"
            select previous.requirement_key
            from design_requirements current
            join design_requirements previous on previous.id = current.supersedes_requirement_id
            where current.design_version_id = ?1 and current.requirement_key = 'REQ-002'
            "#,
            params![import.design_version_id],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(supersedes_key, "REQ-001");
}

#[test]
fn design_exception_acceptance_allows_pre_import_size_exceptions() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let file_package = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "oversized-file",
            title: "Oversized File",
        },
    )
    .unwrap();
    fs::write(
        file_package
            .package_path
            .join("requirements")
            .join("README.md"),
        requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
    )
    .unwrap();
    fs::write(
        file_package.package_path.join("01-introduction-goals.md"),
        std::iter::repeat_n("line", 1001)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let approval_authority_event_id = approval_authority_event(temp.path());
    let file_acceptance = accept_design_exception(
        temp.path(),
        NewDesignExceptionAcceptance {
            design_version_id: None,
            design_package: Some("oversized-file"),
            target: "file:01-introduction-goals.md",
            acceptance_type: "explicit_exception",
            reason: "temporary source document is larger than the import guardrail",
            approval_authority_event_id,
        },
    )
    .unwrap();
    let file_import = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &file_package.package_path,
            status: "draft",
        },
    )
    .unwrap();

    let requirement_package = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "oversized-requirement",
            title: "Oversized Requirement",
        },
    )
    .unwrap();
    let oversized_body = std::iter::repeat_n("Requirement detail.", 151)
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        requirement_package
            .package_path
            .join("requirements")
            .join("README.md"),
        format!(
            r#"## REQ-001: Preserve cleanup behavior
```yaml agent-workbench
type: requirement
key: REQ-001
priority: high
surfaces: [cli, database]
validation: [GATE-001]
status: active
```

{oversized_body}
"#
        ),
    )
    .unwrap();
    let requirement_acceptance = accept_design_exception(
        temp.path(),
        NewDesignExceptionAcceptance {
            design_version_id: None,
            design_package: Some("oversized-requirement"),
            target: "requirement:REQ-001",
            acceptance_type: "explicit_exception",
            reason: "temporary requirement source is larger than the import guardrail",
            approval_authority_event_id,
        },
    )
    .unwrap();
    let requirement_import = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &requirement_package.package_path,
            status: "draft",
        },
    )
    .unwrap();

    assert_eq!(file_acceptance.target_type, "design_file");
    assert_eq!(
        file_acceptance.design_file_path.as_deref(),
        Some("01-introduction-goals.md")
    );
    assert_eq!(file_import.file_count, 14);
    assert_eq!(requirement_acceptance.target_type, "design_requirement_key");
    assert_eq!(
        requirement_acceptance.design_requirement_key.as_deref(),
        Some("REQ-001")
    );
    assert_eq!(requirement_import.requirement_count, 1);
}

#[test]
fn design_import_reports_size_warnings_without_blocking() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let init = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "size-warning",
            title: "Size Warning",
        },
    )
    .unwrap();
    fs::write(
        init.package_path.join("01-introduction-goals.md"),
        std::iter::repeat_n("line", 501)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    let requirement_body = std::iter::repeat_n("Requirement detail.", 81)
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        init.package_path.join("requirements").join("README.md"),
        format!(
            r#"## REQ-001: Preserve cleanup behavior
```yaml agent-workbench
type: requirement
key: REQ-001
priority: high
surfaces: [cli, database]
validation: [GATE-001]
status: active
```

{requirement_body}
"#
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

    assert_eq!(import.warning_count, 2);
}

#[test]
fn design_import_rejects_external_or_duplicate_package() {
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
    let external = temp.path().join("external-design");
    fs::create_dir_all(&external).unwrap();

    assert!(
        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &external,
                status: "draft",
            },
        )
        .is_err()
    );

    import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &init.package_path,
            status: "draft",
        },
    )
    .unwrap();
    assert!(
        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &init.package_path,
                status: "draft",
            },
        )
        .is_err()
    );
}

#[test]
fn design_import_rejects_invalid_design_blocks() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();

    let init = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "missing-field",
            title: "Missing Field",
        },
    )
    .unwrap();
    fs::write(
        init.package_path.join("requirements").join("README.md"),
        r#"## REQ-001: Missing priority
```yaml agent-workbench
type: requirement
key: REQ-001
surfaces: [cli]
validation: [GATE-001]
status: active
```

Body.
"#,
    )
    .unwrap();
    assert!(
        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &init.package_path,
                status: "draft",
            },
        )
        .is_err()
    );

    let init = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "invalid-prefix",
            title: "Invalid Prefix",
        },
    )
    .unwrap();
    fs::write(
        init.package_path.join("requirements").join("README.md"),
        requirement_doc("BAD-001", "Bad prefix", "high").replace("## BAD-001", "## REQ-001"),
    )
    .unwrap();
    assert!(
        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &init.package_path,
                status: "draft",
            },
        )
        .is_err()
    );

    let init = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "legacy-doc",
            title: "Legacy Doc",
        },
    )
    .unwrap();
    fs::write(
        init.package_path.join("requirements").join("README.md"),
        r#"## R-001: Legacy key
```yaml agent-workbench
type: requirement
key: R-001
priority: high
surfaces: [cli]
validation: [GATE-001]
status: active
```

Body.
"#,
    )
    .unwrap();
    assert!(
        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &init.package_path,
                status: "draft",
            },
        )
        .is_err()
    );
}

#[test]
fn design_import_rejects_non_strict_keys_revisions_and_unknown_metadata() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();

    let bad_requirement_key = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "bad-requirement-key",
            title: "Bad Requirement Key",
        },
    )
    .unwrap();
    fs::write(
        bad_requirement_key
            .package_path
            .join("requirements")
            .join("README.md"),
        requirement_doc("REQ-001 extra", "Bad key", "high"),
    )
    .unwrap();
    assert!(
        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &bad_requirement_key.package_path,
                status: "draft",
            },
        )
        .is_err()
    );

    let bad_revision = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "bad-revision",
            title: "Bad Revision",
        },
    )
    .unwrap();
    fs::write(
        bad_revision
            .package_path
            .join("requirements")
            .join("README.md"),
        r#"## REQ-001: Bad revision
```yaml agent-workbench
type: requirement
key: REQ-001
revision: 0
priority: high
surfaces: [cli]
validation: [GATE-001]
status: active
```

Body.
"#,
    )
    .unwrap();
    assert!(
        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &bad_revision.package_path,
                status: "draft",
            },
        )
        .is_err()
    );

    let unknown_field = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "unknown-field",
            title: "Unknown Field",
        },
    )
    .unwrap();
    fs::write(
        unknown_field
            .package_path
            .join("requirements")
            .join("README.md"),
        r#"## REQ-001: Unknown field
```yaml agent-workbench
type: requirement
key: REQ-001
priority: high
surfaces: [cli]
validation: [GATE-001]
status: active
surafces: [typo]
```

Body.
"#,
    )
    .unwrap();
    assert!(
        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &unknown_field.package_path,
                status: "draft",
            },
        )
        .is_err()
    );

    let bad_decision_key = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "bad-decision-key",
            title: "Bad Decision Key",
        },
    )
    .unwrap();
    fs::write(
        bad_decision_key
            .package_path
            .join("requirements")
            .join("README.md"),
        requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
    )
    .unwrap();
    fs::write(
        bad_decision_key.package_path.join("09-decisions.md"),
        decision_doc().replace("DEC-001", "DEC-bad"),
    )
    .unwrap();
    assert!(
        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &bad_decision_key.package_path,
                status: "draft",
            },
        )
        .is_err()
    );

    let bad_gate_key = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "bad-gate-key",
            title: "Bad Gate Key",
        },
    )
    .unwrap();
    fs::write(
        bad_gate_key
            .package_path
            .join("requirements")
            .join("README.md"),
        requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
    )
    .unwrap();
    fs::write(
        bad_gate_key
            .package_path
            .join("validation")
            .join("gates.md"),
        validation_gate_doc("GATE-foo"),
    )
    .unwrap();
    assert!(
        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &bad_gate_key.package_path,
                status: "draft",
            },
        )
        .is_err()
    );

    let bad_heading = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "bad-heading",
            title: "Bad Heading",
        },
    )
    .unwrap();
    fs::write(
        bad_heading
            .package_path
            .join("requirements")
            .join("README.md"),
        requirement_doc("REQ-001", "Bad heading", "high")
            .replace("## REQ-001:", "## REQ-001-extra:"),
    )
    .unwrap();
    assert!(
        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &bad_heading.package_path,
                status: "draft",
            },
        )
        .is_err()
    );

    let bad_heading_level = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "bad-heading-level",
            title: "Bad Heading Level",
        },
    )
    .unwrap();
    fs::write(
        bad_heading_level
            .package_path
            .join("requirements")
            .join("README.md"),
        r#"### REQ-001: Wrong heading level
```yaml agent-workbench
type: requirement
key: REQ-001
priority: high
surfaces: [cli]
validation: [GATE-001]
status: active
```

Body.
"#,
    )
    .unwrap();
    assert!(
        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &bad_heading_level.package_path,
                status: "draft",
            },
        )
        .is_err()
    );

    let bad_arc42_block = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "bad-arc42-block",
            title: "Bad Arc42 Block",
        },
    )
    .unwrap();
    fs::write(
        bad_arc42_block.package_path.join("02-constraints.md"),
        r#"## REQ-001: Wrong section
```yaml agent-workbench
type: requirement
key: REQ-001
priority: high
surfaces: [cli]
validation: [GATE-001]
status: active
```

Body.
"#,
    )
    .unwrap();
    assert!(
        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &bad_arc42_block.package_path,
                status: "draft",
            },
        )
        .is_err()
    );

    let hidden_bad_block = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "hidden-bad-block",
            title: "Hidden Bad Block",
        },
    )
    .unwrap();
    fs::write(
        hidden_bad_block
            .package_path
            .join("requirements")
            .join("README.md"),
        r#"## BAD-001: Bad hidden block
```yaml agent-workbench
type: requirement
key: BAD-001
priority: high
surfaces: [cli]
validation: [GATE-001]
status: active
```

Body.
"#,
    )
    .unwrap();
    assert!(
        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &hidden_bad_block.package_path,
                status: "draft",
            },
        )
        .is_err()
    );
}

#[test]
fn design_import_rejects_manifest_arc42_key_drift() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let init = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "manifest-typo",
            title: "Manifest Typo",
        },
    )
    .unwrap();
    let manifest_path = init.package_path.join("design.yaml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        manifest.replace("introduction_goals:", "introducton_goals:"),
    )
    .unwrap();

    assert!(
        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &init.package_path,
                status: "draft",
            },
        )
        .is_err()
    );
}

#[test]
fn design_import_rejects_duplicate_decisions_gates_and_oversized_files() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();

    let duplicate_requirement = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "duplicate-requirement",
            title: "Duplicate Requirement",
        },
    )
    .unwrap();
    fs::write(
        duplicate_requirement
            .package_path
            .join("requirements")
            .join("README.md"),
        format!(
            "{}\n{}",
            requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
            requirement_doc("REQ-001", "Preserve cleanup behavior again", "high")
        ),
    )
    .unwrap();
    assert!(
        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &duplicate_requirement.package_path,
                status: "draft",
            },
        )
        .is_err()
    );

    let duplicate_decision = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "duplicate-decision",
            title: "Duplicate Decision",
        },
    )
    .unwrap();
    fs::write(
        duplicate_decision
            .package_path
            .join("requirements")
            .join("README.md"),
        requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
    )
    .unwrap();
    fs::write(
        duplicate_decision.package_path.join("09-decisions.md"),
        format!("{}\n{}", decision_doc(), decision_doc()),
    )
    .unwrap();
    assert!(
        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &duplicate_decision.package_path,
                status: "draft",
            },
        )
        .is_err()
    );

    let duplicate_gate = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "duplicate-gate",
            title: "Duplicate Gate",
        },
    )
    .unwrap();
    fs::write(
        duplicate_gate
            .package_path
            .join("requirements")
            .join("README.md"),
        requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
    )
    .unwrap();
    fs::write(
        duplicate_gate
            .package_path
            .join("validation")
            .join("gates.md"),
        format!(
            "{}\n{}",
            validation_gate_doc("GATE-001"),
            validation_gate_doc("GATE-001")
        ),
    )
    .unwrap();
    assert!(
        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &duplicate_gate.package_path,
                status: "draft",
            },
        )
        .is_err()
    );

    let oversized = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "oversized-file",
            title: "Oversized File",
        },
    )
    .unwrap();
    fs::write(
        oversized
            .package_path
            .join("requirements")
            .join("README.md"),
        requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
    )
    .unwrap();
    fs::write(
        oversized.package_path.join("01-introduction-goals.md"),
        std::iter::repeat_n("line", 1001)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();
    assert!(
        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &oversized.package_path,
                status: "draft",
            },
        )
        .is_err()
    );
}

#[test]
fn acceptance_records_enforce_single_typed_target() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();

    let missing_target = conn.execute(
        r#"
        insert into acceptance_records(
            project_id, target_type, acceptance_type, reason, created_by,
            status, created_at
        )
        values (1, 'design_requirement', 'explicit_exception', 'missing target',
                'user', 'approved', current_timestamp)
        "#,
        [],
    );
    assert!(missing_target.is_err());

    let wrong_target = conn.execute(
        r#"
        insert into acceptance_records(
            project_id, target_type, task_id, design_requirement_id,
            acceptance_type, reason, created_by, status, created_at
        )
        values (1, 'task', 999, 999, 'explicit_exception', 'too many targets',
                'user', 'approved', current_timestamp)
        "#,
        [],
    );
    assert!(wrong_target.is_err());
}

#[test]
fn acceptance_records_enforce_design_target_project_match() {
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
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        r#"
        insert into projects(name, root_path, created_at, updated_at)
        values ('other', '/tmp/agent-workbench-other', current_timestamp, current_timestamp)
        "#,
        [],
    )
    .unwrap();
    let requirement_id: i64 = conn
        .query_row(
            "select id from design_requirements where design_version_id = ?1",
            params![import.design_version_id],
            |row| row.get(0),
        )
        .unwrap();

    let cross_project = conn.execute(
        r#"
        insert into acceptance_records(
            project_id, target_type, design_requirement_id, acceptance_type,
            reason, created_by, status, created_at
        )
        values (2, 'design_requirement', ?1, 'explicit_exception',
                'wrong project', 'user', 'approved', current_timestamp)
        "#,
        params![requirement_id],
    );

    assert!(cross_project.is_err());
}

#[test]
fn acceptance_records_enforce_task_target_project_match() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    start_work(temp.path(), "scoped work", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "project-local task",
            priority: "medium",
            source: "user",
            work_unit_id: None,
            details: None,
            completion_condition: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        r#"
        insert into projects(name, root_path, created_at, updated_at)
        values ('other', '/tmp/agent-workbench-other', current_timestamp, current_timestamp)
        "#,
        [],
    )
    .unwrap();

    let cross_project = conn.execute(
        r#"
        insert into acceptance_records(
            project_id, target_type, task_id, acceptance_type,
            reason, created_by, status, created_at
        )
        values (2, 'task', ?1, 'accepted_out_of_scope',
                'wrong project', 'user', 'approved', current_timestamp)
        "#,
        params![task.task_id],
    );

    assert!(cross_project.is_err());
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

fn add_clean_implementation_ready_review(
    root: &std::path::Path,
    work_unit_id: i64,
    design_version_id: i64,
) {
    let plan_id = add_implementation_ready_review_plan(root, work_unit_id, design_version_id);
    add_clean_review_run(
        root,
        plan_id,
        Some(&format!(
            "review-context:design-task-decomposition:design={design_version_id}:work={work_unit_id}"
        )),
        "clean decomposition review",
    );
}

fn add_implementation_ready_review_plan(
    root: &std::path::Path,
    work_unit_id: i64,
    design_version_id: i64,
) -> i64 {
    let plan = add_review_plan(
        root,
        NewReviewPlan {
            work_unit_id,
            design_version_id: Some(design_version_id),
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
    plan.review_plan_id
}

fn add_close_ready_review_plans(
    root: &std::path::Path,
    work_unit_id: i64,
    design_version_id: i64,
) -> Vec<(i64, &'static str)> {
    let mut plans = Vec::new();
    for review_type in ["design_implementation_diff", "implementation_review"] {
        let plan = add_review_plan(
            root,
            NewReviewPlan {
                work_unit_id,
                design_version_id: Some(design_version_id),
                review_type,
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
        plans.push((plan.review_plan_id, review_type));
    }
    plans
}

fn add_clean_close_ready_review_runs(
    root: &std::path::Path,
    work_unit_id: i64,
    design_version_id: i64,
    plans: &[(i64, &'static str)],
    use_context: bool,
) -> Vec<anyhow::Result<ReviewRunOutcome>> {
    let mut results = Vec::new();
    for (review_plan_id, review_type) in plans {
        let context_kind = match *review_type {
            "design_implementation_diff" => "design-implementation-diff",
            "implementation_review" => "implementation-review",
            _ => unreachable!(),
        };
        let target_ref = use_context.then(|| {
            format!("review-context:{context_kind}:design={design_version_id}:work={work_unit_id}")
        });
        let result = add_clean_review_run_result(
            root,
            *review_plan_id,
            target_ref.as_deref(),
            "clean close review",
        );
        results.push(result);
    }
    results
}

fn add_clean_review_run(
    root: &std::path::Path,
    review_plan_id: i64,
    target_ref: Option<&str>,
    summary: &str,
) -> i64 {
    add_clean_review_run_result(root, review_plan_id, target_ref, summary)
        .unwrap()
        .review_run_id
}

fn add_clean_review_run_result(
    root: &std::path::Path,
    review_plan_id: i64,
    target_ref: Option<&str>,
    summary: &str,
) -> anyhow::Result<ReviewRunOutcome> {
    add_review_run(
        root,
        NewReviewRun {
            review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref,
            prompt_deviations: None,
            result_summary: Some(summary),
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
}
