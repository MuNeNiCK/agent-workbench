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
    assert_eq!(passed.result, "pass");
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
    }));
    assert_eq!(close_passed.result, "pass");
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
    assert!(
        stale
            .iter()
            .any(|record| record.record_type == "review_plan")
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
    add_general_acceptance(
        temp.path(),
        NewGeneralAcceptance {
            target: "stale:coverage_item:1",
            acceptance_type: "stale_accepted",
            reason: "user accepted stale coverage while preserving scope",
            approval_authority_event_id,
        },
    )
    .unwrap();
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
            agent_label: None,
            external_agent_id: None,
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
            agent_label: None,
            external_agent_id: None,
        },
    )
}
