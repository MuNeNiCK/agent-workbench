use super::super::*;

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
