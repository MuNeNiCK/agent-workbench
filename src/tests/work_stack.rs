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
            runtime_boundary_evidence: None,
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
