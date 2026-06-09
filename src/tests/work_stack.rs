use super::*;

#[test]
fn status_reports_uninitialized_project() {
    let temp = tempfile::tempdir().unwrap();

    let status = project_status(temp.path()).unwrap();

    assert!(!status.initialized);
    assert!(status.schema_version.is_none());
}

#[test]
fn next_reports_no_active_work_unit_after_init() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();

    let next = next_action(temp.path()).unwrap();

    assert_eq!(next, NextAction::NoActiveWorkUnit);
}

#[test]
fn work_start_creates_active_work_unit() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();

    let started = start_work(temp.path(), "write lifecycle test", Some("test first")).unwrap();
    let next = next_action(temp.path()).unwrap();

    assert_eq!(started.work_unit_id, 1);
    assert_eq!(started.activation_id, 1);
    assert_eq!(
        next,
        NextAction::ContinueActive {
            work_unit: ActiveWorkUnit {
                id: 1,
                title: "write lifecycle test".to_string()
            }
        }
    );
}

#[test]
fn work_start_refuses_second_active_activation() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    start_work(temp.path(), "one", None).unwrap();

    let second = start_work(temp.path(), "two", None);

    assert!(second.is_err());
}

#[test]
fn suspend_and_resume_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let started = start_work(temp.path(), "implement resume", None).unwrap();

    let suspended = suspend_work(
        temp.path(),
        "need to validate assumption",
        "continue implementation",
    )
    .unwrap();
    let check = resume_check_basic(temp.path()).unwrap();
    let resumed = resume_work(temp.path(), check.resume_check_id).unwrap();

    assert_eq!(suspended.work_unit_id, started.work_unit_id);
    assert_eq!(check.result, "allowed");
    assert_eq!(resumed.activation_id, started.activation_id);
    assert!(matches!(
        next_action(temp.path()).unwrap(),
        NextAction::ContinueActive { .. }
    ));
}

#[test]
fn resume_ready_dry_run_does_not_record_check() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    start_work(temp.path(), "implement resume gate", None).unwrap();
    suspend_work(temp.path(), "interrupt complete", "resume gate work").unwrap();

    let outcome = resume_ready_basic(temp.path()).unwrap();

    assert_eq!(outcome.result, "pass");
    assert!(
        outcome
            .items
            .iter()
            .filter(|item| item.result == "pass")
            .count()
            >= 6
    );
    assert!(
        outcome
            .items
            .iter()
            .any(|item| item.result == "not_checked")
    );
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let count: i64 = conn
        .query_row("select count(*) from resume_checks", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn resume_ready_without_target_returns_blocked_gate_result() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();

    let outcome = resume_ready_basic(temp.path()).unwrap();

    assert_eq!(outcome.result, "blocked");
    assert_eq!(
        outcome.blocking_reason.as_deref(),
        Some("no suspended activation to resume")
    );
    assert_eq!(outcome.work_unit_id, None);
    assert_eq!(outcome.activation_id, None);
}

#[test]
fn trace_aware_resume_check_evaluates_trace_items() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    start_work(temp.path(), "implement trace gate", None).unwrap();
    suspend_work(temp.path(), "need trace-aware check", "resume trace work").unwrap();

    let check = resume_check(temp.path(), "trace-aware").unwrap();

    assert_eq!(check.result, "allowed");
    assert_eq!(check.blocking_reason, None);
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let stored_maturity: String = conn
        .query_row(
            "select maturity from resume_checks where id = ?1",
            params![check.resume_check_id],
            |row| row.get(0),
        )
        .unwrap();
    let trace_passes: i64 = conn
        .query_row(
            r#"
            select count(*)
            from resume_check_items
            where resume_check_id = ?1
              and check_name in (
                'design_version_current',
                'task_derivation_current',
                'checklist_current',
                'selected_gate_current'
              )
              and result = 'pass'
            "#,
            params![check.resume_check_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stored_maturity, "trace-aware");
    assert_eq!(trace_passes, 4);
}

#[test]
fn trace_aware_resume_blocks_stale_coverage_items() {
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
            derivation_reason: None,
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
    suspend_work(temp.path(), "design changed", "resume after trace check").unwrap();
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

    let check = resume_check(temp.path(), "trace-aware").unwrap();

    assert_eq!(check.result, "blocked");
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let design_current_result: String = conn
        .query_row(
            r#"
            select result
            from resume_check_items
            where resume_check_id = ?1 and check_name = 'design_version_current'
            "#,
            params![check.resume_check_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(design_current_result, "fail");
}

#[test]
fn repo_aware_resume_requires_classified_repository_comparison() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "repo aware resume", None).unwrap();
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
    let base = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(work.activation_id),
            head_sha: Some("abc123"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    suspend_work(
        temp.path(),
        "pause with repository state",
        "resume repo work",
    )
    .unwrap();
    let current = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: None,
            head_sha: Some("abc123"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();

    let blocked = resume_ready(temp.path(), "repo-aware").unwrap();
    add_repository_snapshot_comparison(
        temp.path(),
        NewRepositorySnapshotComparison {
            base_repository_snapshot_id: base.repository_snapshot_id,
            current_repository_snapshot_id: current.repository_snapshot_id,
            comparison_type: "resume",
            head_changed: false,
            dirty_state_changed: false,
            nested_repository_changed: false,
            result: "same",
        },
    )
    .unwrap();
    let check = resume_check(temp.path(), "repo-aware").unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let stored_snapshot_id: i64 = conn
        .query_row(
            "select repository_snapshot_id from resume_checks where id = ?1",
            params![check.resume_check_id],
            |row| row.get(0),
        )
        .unwrap();
    let repository_item: (String, String) = conn
        .query_row(
            "select result, details from resume_check_items where resume_check_id = ?1 and check_name = 'repository_state_current'",
            params![check.resume_check_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(blocked.result, "blocked");
    assert_eq!(
        blocked.blocking_reason.as_deref(),
        Some("repo-aware resume checks failed")
    );
    assert!(
        blocked
            .items
            .iter()
            .any(|item| item.name == "repository_state_current" && item.result == "fail")
    );
    assert_eq!(check.result, "allowed");
    assert_eq!(stored_snapshot_id, current.repository_snapshot_id);
    assert_eq!(repository_item.0, "pass");
    assert!(repository_item.1.contains("0 missing comparisons"));
}

#[test]
fn repo_aware_resume_requires_snapshots_for_all_repositories() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "repo aware nested resume", None).unwrap();
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
    add_repository(
        temp.path(),
        NewRepository {
            name: "nested",
            path: "vendor/lib",
            current_head: Some("def456"),
            status_summary: Some("clean"),
        },
    )
    .unwrap();
    let main_base = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(work.activation_id),
            head_sha: Some("abc123"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    suspend_work(
        temp.path(),
        "pause with partial repository state",
        "resume repo work",
    )
    .unwrap();
    let main_current = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: None,
            head_sha: Some("abc123"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    add_repository_snapshot_comparison(
        temp.path(),
        NewRepositorySnapshotComparison {
            base_repository_snapshot_id: main_base.repository_snapshot_id,
            current_repository_snapshot_id: main_current.repository_snapshot_id,
            comparison_type: "resume",
            head_changed: false,
            dirty_state_changed: false,
            nested_repository_changed: false,
            result: "same",
        },
    )
    .unwrap();

    let blocked = resume_ready(temp.path(), "repo-aware").unwrap();

    let nested_base = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "nested",
            work_unit_activation_id: Some(work.activation_id),
            head_sha: Some("def456"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    let nested_current = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "nested",
            work_unit_activation_id: None,
            head_sha: Some("def456"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    add_repository_snapshot_comparison(
        temp.path(),
        NewRepositorySnapshotComparison {
            base_repository_snapshot_id: nested_base.repository_snapshot_id,
            current_repository_snapshot_id: nested_current.repository_snapshot_id,
            comparison_type: "resume",
            head_changed: false,
            dirty_state_changed: false,
            nested_repository_changed: false,
            result: "same",
        },
    )
    .unwrap();
    let allowed = resume_ready(temp.path(), "repo-aware").unwrap();

    assert_eq!(blocked.result, "blocked");
    assert!(
        blocked
            .items
            .iter()
            .any(|item| item.name == "repository_state_current"
                && item.result == "fail"
                && item.details.contains("1 missing base snapshots"))
    );
    assert_eq!(allowed.result, "pass");
    assert!(
        allowed
            .items
            .iter()
            .any(|item| item.name == "repository_state_current"
                && item.result == "pass"
                && item.details.contains("0 missing base snapshots"))
    );
}

#[test]
fn repo_aware_resume_counts_base_snapshots_by_repository() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "repo aware duplicate base", None).unwrap();
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
    add_repository(
        temp.path(),
        NewRepository {
            name: "nested",
            path: "vendor/lib",
            current_head: Some("def456"),
            status_summary: Some("clean"),
        },
    )
    .unwrap();
    add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(work.activation_id),
            head_sha: Some("abc123"),
            branch: Some("master"),
            status_summary: Some("early clean"),
            is_clean: true,
        },
    )
    .unwrap();
    let main_base = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(work.activation_id),
            head_sha: Some("abc123"),
            branch: Some("master"),
            status_summary: Some("latest clean"),
            is_clean: true,
        },
    )
    .unwrap();
    suspend_work(
        temp.path(),
        "pause with duplicate repository snapshots",
        "resume repo work",
    )
    .unwrap();
    let main_current = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: None,
            head_sha: Some("abc123"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    add_repository_snapshot_comparison(
        temp.path(),
        NewRepositorySnapshotComparison {
            base_repository_snapshot_id: main_base.repository_snapshot_id,
            current_repository_snapshot_id: main_current.repository_snapshot_id,
            comparison_type: "resume",
            head_changed: false,
            dirty_state_changed: false,
            nested_repository_changed: false,
            result: "same",
        },
    )
    .unwrap();

    let blocked = resume_ready(temp.path(), "repo-aware").unwrap();

    assert_eq!(blocked.result, "blocked");
    assert!(
        blocked
            .items
            .iter()
            .any(|item| item.name == "repository_state_current"
                && item.result == "fail"
                && item.details.contains("1 suspend snapshots")
                && item.details.contains("1 missing base snapshots"))
    );
}

#[test]
fn close_ready_requires_validation_runs_for_selected_gates() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "close ready validation", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "implement cleanup",
            priority: "high",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("validation run is recorded"),
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
        },
    )
    .unwrap();
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
    add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(work.activation_id),
            head_sha: Some("abc123"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();

    let missing = close_ready(temp.path()).unwrap();
    add_validation_run(
        temp.path(),
        NewValidationRun {
            validation_gate_id: gate.validation_gate_id,
            command_usage_id: None,
            repository_snapshot_id: None,
            result: "pass",
            artifact_path: None,
            artifact_hash: None,
            notes: Some("validation passed"),
        },
    )
    .unwrap();
    let recorded = close_ready(temp.path()).unwrap();

    assert!(
        missing
            .items
            .iter()
            .any(|item| item.name == "validation_runs_recorded" && item.result == "fail")
    );
    assert!(
        recorded
            .items
            .iter()
            .any(|item| item.name == "validation_runs_recorded" && item.result == "pass")
    );
}

#[test]
fn close_ready_allows_explicitly_accepted_validation_failures() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "accepted validation failure", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "implement cleanup",
            priority: "high",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("validation failure is accepted"),
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
        },
    )
    .unwrap();
    add_validation_run(
        temp.path(),
        NewValidationRun {
            validation_gate_id: gate.validation_gate_id,
            command_usage_id: None,
            repository_snapshot_id: None,
            result: "fail",
            artifact_path: None,
            artifact_hash: None,
            notes: Some("known external failure"),
        },
    )
    .unwrap();

    let blocked = close_ready(temp.path()).unwrap();
    accept_design_exception(
        temp.path(),
        NewDesignExceptionAcceptance {
            design_version_id: Some(import.design_version_id),
            design_package: None,
            target: "gate:GATE-001",
            acceptance_type: "explicit_exception",
            reason: "known external failure accepted by user",
        },
    )
    .unwrap();
    let accepted = close_ready(temp.path()).unwrap();

    assert!(
        blocked
            .items
            .iter()
            .any(|item| item.name == "validation_runs_recorded" && item.result == "fail")
    );
    assert!(
        accepted
            .items
            .iter()
            .any(|item| item.name == "validation_runs_recorded" && item.result == "pass")
    );
}

#[test]
fn close_ready_requires_required_close_plans_to_be_clean() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "close plan work", None).unwrap();
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            review_type: "implementation_review",
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

    let blocked = close_ready(temp.path()).unwrap();
    add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: None,
            prompt_deviations: None,
            result_summary: Some("clean"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
        },
    )
    .unwrap();
    let allowed = close_ready(temp.path()).unwrap();

    assert_eq!(blocked.result, "blocked");
    assert!(
        blocked
            .items
            .iter()
            .any(|item| item.name == "review_plans_clean" && item.result == "fail")
    );
    assert_eq!(allowed.result, "pass");
    assert!(
        allowed
            .items
            .iter()
            .any(|item| item.name == "review_plans_clean" && item.result == "pass")
    );
}

#[test]
fn close_ready_requires_close_repository_comparisons_for_changed_snapshots() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "close comparison work", None).unwrap();
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
    let base = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: None,
            head_sha: Some("abc123"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    let current = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(work.activation_id),
            head_sha: Some("def456"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();

    let blocked = close_ready(temp.path()).unwrap();
    add_repository_snapshot_comparison(
        temp.path(),
        NewRepositorySnapshotComparison {
            base_repository_snapshot_id: base.repository_snapshot_id,
            current_repository_snapshot_id: current.repository_snapshot_id,
            comparison_type: "close",
            head_changed: true,
            dirty_state_changed: false,
            nested_repository_changed: false,
            result: "changed_classified",
        },
    )
    .unwrap();
    let allowed = close_ready(temp.path()).unwrap();

    assert_eq!(blocked.result, "blocked");
    assert!(
        blocked
            .items
            .iter()
            .any(|item| item.name == "repository_state_recorded" && item.result == "fail")
    );
    assert_eq!(allowed.result, "pass");
    assert!(
        allowed
            .items
            .iter()
            .any(|item| item.name == "repository_state_recorded" && item.result == "pass")
    );
}

#[test]
fn close_ready_uses_pre_activation_repository_snapshot_as_comparison_base() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "close comparison baseline", None).unwrap();
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
    let baseline = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: None,
            head_sha: Some("abc123"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    let active_intermediate = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(work.activation_id),
            head_sha: Some("def456"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    let active_latest = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(work.activation_id),
            head_sha: Some("def456"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    add_repository_snapshot_comparison(
        temp.path(),
        NewRepositorySnapshotComparison {
            base_repository_snapshot_id: active_intermediate.repository_snapshot_id,
            current_repository_snapshot_id: active_latest.repository_snapshot_id,
            comparison_type: "close",
            head_changed: false,
            dirty_state_changed: false,
            nested_repository_changed: false,
            result: "same",
        },
    )
    .unwrap();

    let blocked = close_ready(temp.path()).unwrap();
    add_repository_snapshot_comparison(
        temp.path(),
        NewRepositorySnapshotComparison {
            base_repository_snapshot_id: baseline.repository_snapshot_id,
            current_repository_snapshot_id: active_latest.repository_snapshot_id,
            comparison_type: "close",
            head_changed: true,
            dirty_state_changed: false,
            nested_repository_changed: false,
            result: "changed_classified",
        },
    )
    .unwrap();
    let allowed = close_ready(temp.path()).unwrap();

    assert_eq!(blocked.result, "blocked");
    assert!(
        blocked
            .items
            .iter()
            .any(|item| item.name == "repository_state_recorded" && item.result == "fail")
    );
    assert_eq!(allowed.result, "pass");
    assert!(
        allowed
            .items
            .iter()
            .any(|item| item.name == "repository_state_recorded" && item.result == "pass")
    );
}

#[test]
fn close_ready_ignores_interrupted_child_repository_snapshots_as_baseline() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let parent = start_work(temp.path(), "parent close baseline", None).unwrap();
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
    let baseline = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: None,
            head_sha: Some("abc123"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    let child = interrupt_work(temp.path(), "child work", "check interruption").unwrap();
    let child_snapshot = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(child.child_activation_id),
            head_sha: Some("child456"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    add_repository_snapshot_comparison(
        temp.path(),
        NewRepositorySnapshotComparison {
            base_repository_snapshot_id: baseline.repository_snapshot_id,
            current_repository_snapshot_id: child_snapshot.repository_snapshot_id,
            comparison_type: "close",
            head_changed: true,
            dirty_state_changed: false,
            nested_repository_changed: false,
            result: "changed_classified",
        },
    )
    .unwrap();
    close_active_work(temp.path(), "child complete", None).unwrap();
    let check = resume_check(temp.path(), "basic").unwrap();
    resume_work(temp.path(), check.resume_check_id).unwrap();
    let parent_current = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(parent.activation_id),
            head_sha: Some("parent789"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    add_repository_snapshot_comparison(
        temp.path(),
        NewRepositorySnapshotComparison {
            base_repository_snapshot_id: child_snapshot.repository_snapshot_id,
            current_repository_snapshot_id: parent_current.repository_snapshot_id,
            comparison_type: "close",
            head_changed: true,
            dirty_state_changed: false,
            nested_repository_changed: false,
            result: "changed_classified",
        },
    )
    .unwrap();

    let blocked = close_ready(temp.path()).unwrap();
    add_repository_snapshot_comparison(
        temp.path(),
        NewRepositorySnapshotComparison {
            base_repository_snapshot_id: baseline.repository_snapshot_id,
            current_repository_snapshot_id: parent_current.repository_snapshot_id,
            comparison_type: "close",
            head_changed: true,
            dirty_state_changed: false,
            nested_repository_changed: false,
            result: "changed_classified",
        },
    )
    .unwrap();
    let allowed = close_ready(temp.path()).unwrap();

    assert_eq!(blocked.result, "blocked");
    assert!(
        blocked
            .items
            .iter()
            .any(|item| item.name == "repository_state_recorded" && item.result == "fail")
    );
    assert_eq!(allowed.result, "pass");
    assert!(
        allowed
            .items
            .iter()
            .any(|item| item.name == "repository_state_recorded" && item.result == "pass")
    );
}

#[test]
fn trace_aware_resume_requires_required_resume_plans_to_be_current() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "resume plan work", None).unwrap();
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            review_type: "implementation_review",
            required: true,
            stage: "resume-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    suspend_work(temp.path(), "pause for plan", "resume after plan").unwrap();

    let blocked = resume_ready(temp.path(), "trace-aware").unwrap();
    add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: None,
            prompt_deviations: None,
            result_summary: Some("clean"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
        },
    )
    .unwrap();
    let allowed = resume_ready(temp.path(), "trace-aware").unwrap();

    assert_eq!(blocked.result, "blocked");
    assert!(
        blocked
            .items
            .iter()
            .any(|item| item.name == "review_plan_current" && item.result == "fail")
    );
    assert_eq!(allowed.result, "pass");
    assert!(
        allowed
            .items
            .iter()
            .any(|item| item.name == "review_plan_current" && item.result == "pass")
    );
}

#[test]
fn repo_aware_resume_reports_open_assumption_invalidations() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "assumption work", None).unwrap();
    suspend_work(
        temp.path(),
        "pause with assumption",
        "resume after assumption check",
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into work_units(project_id, title, status, started_at) values (1, 'invalidator', 'open', current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        r#"
        insert into work_unit_dependencies(
            work_unit_id, depends_on_work_unit_id, dependency_type, reason, status, created_at
        )
        values (?1, 2, 'invalidates_assumption', 'assumption no longer holds', 'open', current_timestamp)
        "#,
        params![work.work_unit_id],
    )
    .unwrap();
    drop(conn);

    let blocked = resume_ready(temp.path(), "repo-aware").unwrap();

    assert_eq!(blocked.result, "blocked");
    assert!(
        blocked
            .items
            .iter()
            .any(|item| item.name == "assumptions_current" && item.result == "fail")
    );
}
