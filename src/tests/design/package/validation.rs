use super::super::*;

#[test]
fn design_import_rejects_non_markdown_entries_before_creating_a_version() {
    for (design_id, manifest_entry, replacement) in [
        (
            "json-arc42",
            "introduction_goals: 01-introduction-goals.md",
            "introduction_goals: source/introduction.json",
        ),
        (
            "json-requirement",
            "  - requirements/README.md",
            "  - source/requirement.json",
        ),
        (
            "json-validation",
            "  - validation/gates.md",
            "  - source/gates.json",
        ),
        (
            "uppercase-markdown-extension",
            "introduction_goals: 01-introduction-goals.md",
            "introduction_goals: source/introduction.MD",
        ),
    ] {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        let init = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id,
                title: "Markdown Contract",
            },
        )
        .unwrap();
        fs::create_dir_all(init.package_path.join("source")).unwrap();
        fs::write(
            init.package_path
                .join(replacement.split_whitespace().last().unwrap()),
            "{}\n",
        )
        .unwrap();
        let manifest_path = init.package_path.join("design.yaml");
        let manifest = fs::read_to_string(&manifest_path).unwrap();
        fs::write(
            &manifest_path,
            manifest.replace(manifest_entry, replacement),
        )
        .unwrap();

        let error = import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &init.package_path,
                status: "draft",
            },
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("design manifest paths must name Markdown files ending in .md"),
            "unexpected error for {design_id}: {error}"
        );

        fs::write(&manifest_path, manifest).unwrap();
        let valid = import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &init.package_path,
                status: "draft",
            },
        )
        .unwrap();
        assert_eq!(valid.version_number, 1);
    }
}

#[test]
fn design_import_rejects_non_regular_markdown_manifest_entry() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let init = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "markdown-directory",
            title: "Markdown Directory",
        },
    )
    .unwrap();
    fs::create_dir(init.package_path.join("source.md")).unwrap();
    let manifest_path = init.package_path.join("design.yaml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    fs::write(
        &manifest_path,
        manifest.replace(
            "introduction_goals: 01-introduction-goals.md",
            "introduction_goals: source.md",
        ),
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
}

#[test]
fn design_import_rejects_empty_keys_revisions_and_unknown_metadata() {
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
        r#"## Empty key
```yaml agent-workbench
type: requirement
key: ""
priority: high
surfaces: [cli]
validation: [verify-release]
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
