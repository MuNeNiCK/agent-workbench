use super::super::*;

#[test]
fn design_import_treats_metadata_keys_as_opaque_and_headings_as_presentation() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let init = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "opaque-design-keys",
            title: "Opaque Design Keys",
        },
    )
    .unwrap();
    fs::write(
        init.package_path.join("requirements").join("README.md"),
        r#"## Durable storage behavior
```yaml agent-workbench
type: requirement
key: storage
priority: high
surfaces: [cli, database]
validation: [verify-release]
status: active
```

Storage behavior remains stable across releases.
"#,
    )
    .unwrap();
    fs::write(
        init.package_path.join("09-decisions.md"),
        r#"## Authentication choice
```yaml agent-workbench
type: decision
key: decision.auth
status: accepted
supersedes: []
```

Keep authentication policy explicit.
"#,
    )
    .unwrap();
    fs::write(
        init.package_path.join("validation").join("gates.md"),
        r#"## Candidate verification
```yaml agent-workbench
type: validation_gate_template
key: verify-release
applies_to: [storage]
expected_result: pass
phase: implementation
status: active
```

Verify the candidate release.
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
    let requirements = list_design_requirements(
        temp.path(),
        DesignRequirementListQuery {
            design_version_id: import.design_version_id,
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

    assert_eq!(requirements[0].requirement_key, "storage");
    assert_eq!(
        requirements[0].validation_expectation.as_deref(),
        Some("verify-release")
    );
    assert_eq!(decisions[0].decision_key, "decision.auth");
    assert_eq!(decisions[0].topic, "Authentication choice");
    assert_eq!(gates[0].gate_key, "verify-release");
    assert_eq!(gates[0].requirement_keys.as_deref(), Some("storage"));

    let authority = approval_authority_event(temp.path());
    assert!(
        accept_design_exception(
            temp.path(),
            NewDesignExceptionAcceptance {
                design_version_id: Some(import.design_version_id),
                design_package: None,
                target: "requirement:storage",
                acceptance_type: "accepted_out_of_scope",
                reason: "opaque requirement target",
                approval_authority_event_id: authority,
            },
        )
        .is_ok()
    );
    assert!(
        accept_design_exception(
            temp.path(),
            NewDesignExceptionAcceptance {
                design_version_id: Some(import.design_version_id),
                design_package: None,
                target: "gate:verify-release",
                acceptance_type: "explicit_exception",
                reason: "opaque gate target",
                approval_authority_event_id: authority,
            },
        )
        .is_ok()
    );
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
    assert_eq!(decisions[0].topic, "DEC-001: Keep project-local ledger");
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
        r#"## Initial storage behavior
```yaml agent-workbench
type: requirement
key: storage.v1
priority: high
surfaces: [cli, database]
validation: [verify-release]
status: active
```

This requirement defines the initial storage behavior.
"#,
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
        requirement_doc("storage.v1", "Renamed presentation heading", "high")
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
        r#"## Another presentation heading
```yaml agent-workbench
type: requirement
key: storage.v1
revision: 2
priority: high
surfaces: [cli, database]
validation: [verify-release]
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
        r#"## Initial storage behavior
```yaml agent-workbench
type: requirement
key: storage.v1
priority: high
surfaces: [cli, database]
validation: [verify-release]
status: active
```

This requirement defines the initial storage behavior.
"#,
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
        r#"## Revised storage behavior
```yaml agent-workbench
type: requirement
key: storage.v2
priority: high
surfaces: [cli, database]
validation: [verify-release]
supersedes: [storage.v1]
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
            where current.design_version_id = ?1 and current.requirement_key = ?2
            "#,
            params![import.design_version_id, "storage.v2"],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(supersedes_key, "storage.v1");
}
