use super::*;

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
