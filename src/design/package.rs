use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};
use sha2::{Digest, Sha256};

use crate::db::{default_design_root, open_existing_project, project_id};

use super::{acceptance::*, parsing::*, validation::*, *};

pub(super) const ARC42_FILES: &[(&str, &str)] = &[
    ("01-introduction-goals.md", "Introduction And Goals"),
    ("02-constraints.md", "Constraints"),
    ("03-context-scope.md", "Context And Scope"),
    ("04-solution-strategy.md", "Solution Strategy"),
    ("05-building-blocks.md", "Building Blocks"),
    ("06-runtime-view.md", "Runtime View"),
    ("07-deployment-view.md", "Deployment View"),
    ("08-crosscutting-concepts.md", "Crosscutting Concepts"),
    ("09-decisions.md", "Decisions"),
    ("10-quality-requirements.md", "Quality Requirements"),
    ("11-risks-technical-debt.md", "Risks And Technical Debt"),
    ("12-glossary.md", "Glossary"),
];

pub(super) const ARC42_KEYS: &[&str] = &[
    "introduction_goals",
    "constraints",
    "context_scope",
    "solution_strategy",
    "building_blocks",
    "runtime_view",
    "deployment_view",
    "crosscutting_concepts",
    "decisions",
    "quality_requirements",
    "risks_technical_debt",
    "glossary",
];

pub fn init_design_package(
    root: &Path,
    input: NewDesignPackage<'_>,
) -> Result<DesignPackageInitOutcome> {
    validate_design_id(input.design_id)?;
    open_existing_project(root)?;

    let design_root = default_design_root(root);
    let package_path = design_root.join(input.design_id);
    if package_path.exists() {
        bail!("design package already exists");
    }

    fs::create_dir_all(package_path.join("requirements"))
        .with_context(|| format!("failed to create {}", package_path.display()))?;
    fs::create_dir_all(package_path.join("validation"))
        .with_context(|| format!("failed to create {}", package_path.display()))?;
    fs::create_dir_all(package_path.join("notes"))
        .with_context(|| format!("failed to create {}", package_path.display()))?;

    fs::write(
        package_path.join("design.yaml"),
        design_manifest(input.design_id, input.title),
    )
    .with_context(|| {
        format!(
            "failed to write {}",
            package_path.join("design.yaml").display()
        )
    })?;
    for (file, title) in ARC42_FILES {
        fs::write(package_path.join(file), markdown_stub(title))
            .with_context(|| format!("failed to write {}", package_path.join(file).display()))?;
    }
    fs::write(
        package_path.join("requirements").join("README.md"),
        markdown_stub("Requirements"),
    )
    .with_context(|| {
        format!(
            "failed to write {}",
            package_path
                .join("requirements")
                .join("README.md")
                .display()
        )
    })?;
    fs::write(
        package_path.join("validation").join("gates.md"),
        markdown_stub("Validation Gates"),
    )
    .with_context(|| {
        format!(
            "failed to write {}",
            package_path.join("validation").join("gates.md").display()
        )
    })?;

    Ok(DesignPackageInitOutcome { package_path })
}

pub fn import_design_package(
    root: &Path,
    input: DesignPackageImport<'_>,
) -> Result<DesignPackageImportOutcome> {
    validate_import_status(input.status)?;
    let mut conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let design_root = default_design_root(root);
    let package_path = resolve_package_path(root, input.package_path)?;
    ensure_package_path_is_under_design_root(root, &design_root, &package_path)?;

    let manifest_path = package_path.join("design.yaml");
    let manifest_text = fs::read_to_string(&manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest: DesignManifest = yaml_serde::from_str(&manifest_text)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    validate_design_id(&manifest.id)?;
    if manifest.id
        != package_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
    {
        bail!("design manifest id must match the package directory name");
    }
    if manifest.format != "arc42-agent-workbench" {
        bail!("unsupported design format: {}", manifest.format);
    }
    if manifest.version != 1 {
        bail!("unsupported design manifest version: {}", manifest.version);
    }
    match manifest.status.as_str() {
        "draft" | "reviewed" | "approved" | "superseded" => {}
        _ => bail!("invalid design manifest status: {}", manifest.status),
    }
    for dependency in &manifest.depends_on {
        validate_design_id(dependency)?;
    }
    validate_arc42_manifest_keys(&manifest)?;

    let mut design_files = manifest.design_files();
    design_files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    if design_files.is_empty() {
        bail!("design package has no importable design files");
    }

    let mut package_hasher = Sha256::new();
    package_hasher.update(manifest_text.as_bytes());
    let mut imported_files = Vec::with_capacity(design_files.len());
    let mut warning_count = 0usize;
    for design_file in design_files {
        validate_relative_manifest_path(&design_file.relative_path)?;
        let path = package_path.join(&design_file.relative_path);
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let line_count = line_count(&content);
        if line_count > 500 {
            warning_count += 1;
        }
        if line_count > 1000
            && !design_file_exception_exists(
                &conn,
                project_id,
                &manifest.id,
                &design_file.relative_path,
            )?
        {
            bail!(
                "design file exceeds 1000 lines: {}",
                design_file.relative_path
            );
        }
        let content_hash = sha256_hex(content.as_bytes());
        package_hasher.update(design_file.section_key.as_bytes());
        package_hasher.update(b"\0");
        package_hasher.update(design_file.relative_path.as_bytes());
        package_hasher.update(b"\0");
        package_hasher.update(content.as_bytes());
        imported_files.push(ImportedDesignFile {
            section_key: design_file.section_key,
            relative_path: design_file.relative_path,
            content_hash,
            line_count,
            content,
        });
    }
    let package_digest = package_hasher.finalize();
    let content_hash = hex_digest(&package_digest);

    let tx = conn.transaction()?;
    tx.execute(
        r#"
        insert into design_packages(
            project_id, design_key, package_id, title, root_path, format, version,
            package_hash, status, created_at, updated_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, current_timestamp, current_timestamp)
        on conflict(project_id, design_key) do update set
            title = excluded.title,
            package_id = excluded.package_id,
            root_path = excluded.root_path,
            format = excluded.format,
            version = excluded.version,
            package_hash = excluded.package_hash,
            status = excluded.status,
            updated_at = current_timestamp
        "#,
        params![
            project_id,
            manifest.id,
            manifest.id,
            manifest.title,
            display_path(&package_path),
            manifest.format,
            manifest.version,
            content_hash,
            input.status,
        ],
    )?;
    let design_package_id: i64 = tx.query_row(
        "select id from design_packages where project_id = ?1 and design_key = ?2",
        params![project_id, manifest.id],
        |row| row.get(0),
    )?;
    let existing_version: Option<i64> = tx
        .query_row(
            "select id from design_versions where design_package_id = ?1 and content_hash = ?2",
            params![design_package_id, content_hash],
            |row| row.get(0),
        )
        .optional()?;
    if existing_version.is_some() {
        bail!("design package content has already been imported");
    }
    let version_number: i64 = tx.query_row(
        "select coalesce(max(version_number), 0) + 1 from design_versions where design_package_id = ?1",
        params![design_package_id],
        |row| row.get(0),
    )?;
    tx.execute(
        r#"
        insert into design_versions(
            project_id, design_package_id, version_number, source_ref, package_hash,
            content_hash, package_path, manifest_path, format, manifest_version,
            status, imported_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, current_timestamp)
        "#,
        params![
            project_id,
            design_package_id,
            version_number,
            display_path(&package_path),
            content_hash,
            content_hash,
            display_path(&package_path),
            display_path(&manifest_path),
            manifest.format,
            manifest.version,
            input.status,
        ],
    )?;
    let design_version_id = tx.last_insert_rowid();
    let mut requirement_count = 0usize;
    let mut decision_count = 0usize;
    let mut validation_gate_template_count = 0usize;
    for file in &imported_files {
        tx.execute(
            r#"
            insert into design_files(
                project_id, design_package_id, design_version_id, section_key,
                relative_path, content_hash, line_count
            )
            values (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                project_id,
                design_package_id,
                design_version_id,
                file.section_key,
                file.relative_path,
                file.content_hash,
                file.line_count,
            ],
        )?;
        let design_file_id = tx.last_insert_rowid();
        validate_agent_blocks_for_file(file)?;
        if file.section_key == "requirements" {
            let requirements =
                extract_design_requirements(&tx, project_id, &manifest.id, &file.content, file)?;
            requirement_count += requirements.len();
            for requirement in requirements {
                warning_count += requirement.warning_count;
                let supersedes_requirement_id =
                    validate_requirement_version_transition(&tx, design_package_id, &requirement)?;
                tx.execute(
                    r#"
                    insert into design_requirements(
                        project_id, design_version_id, source_design_file_id,
                        source_section, requirement_key, revision, requirement_hash,
                        supersedes_requirement_id, requirement_text, priority, required_surfaces,
                        validation_expectation, status, created_at
                    )
                    values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, current_timestamp)
                    "#,
                    params![
                        project_id,
                        design_version_id,
                        design_file_id,
                        requirement.source_section,
                        requirement.requirement_key,
                        requirement.revision,
                        requirement.requirement_hash,
                        supersedes_requirement_id,
                        requirement.requirement_text,
                        requirement.priority,
                        requirement.required_surfaces,
                        requirement.validation_expectation,
                        requirement.status,
                    ],
                )?;
            }
        } else if file.section_key == "arc42.decisions" {
            let decisions = extract_design_decisions(&file.content, file)?;
            decision_count += decisions.len();
            for decision in decisions {
                tx.execute(
                    r#"
                    insert into design_decisions(
                        project_id, design_version_id, source_design_file_id,
                        source_section, decision_key, decision_hash, topic,
                        decision_text, rationale, supersedes_decision_keys, status, created_at
                    )
                    values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, current_timestamp)
                    "#,
                    params![
                        project_id,
                        design_version_id,
                        design_file_id,
                        decision.source_section,
                        decision.decision_key,
                        decision.decision_hash,
                        decision.topic,
                        decision.decision_text,
                        decision.rationale,
                        decision.supersedes_decision_keys,
                        decision.status,
                    ],
                )?;
            }
        } else if file.section_key == "validation" {
            let templates = extract_validation_gate_templates(&file.content, file)?;
            validation_gate_template_count += templates.len();
            for template in templates {
                tx.execute(
                    r#"
                    insert into validation_gate_templates(
                        project_id, design_version_id, source_design_file_id,
                        source_section, gate_key, gate_hash, stage, command,
                        expected_result, requirement_keys, gate_text, status, created_at
                    )
                    values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, current_timestamp)
                    "#,
                    params![
                        project_id,
                        design_version_id,
                        design_file_id,
                        template.source_section,
                        template.gate_key,
                        template.gate_hash,
                        template.stage,
                        template.command,
                        template.expected_result,
                        template.requirement_keys,
                        template.gate_text,
                        template.status,
                    ],
                )?;
                let validation_gate_template_id = tx.last_insert_rowid();
                insert_validation_gate_template_requirements(
                    &tx,
                    project_id,
                    design_version_id,
                    validation_gate_template_id,
                    &template.requirement_keys,
                )?;
            }
        }
    }
    mark_stale_links_for_design_version(&tx, project_id, design_package_id, design_version_id)?;
    tx.execute(
        r#"
        update design_packages
        set current_design_version_id = ?1, updated_at = current_timestamp
        where id = ?2
        "#,
        params![design_version_id, design_package_id],
    )?;
    tx.commit()?;

    Ok(DesignPackageImportOutcome {
        design_package_id,
        design_version_id,
        version_number,
        content_hash,
        file_count: imported_files.len(),
        requirement_count,
        decision_count,
        validation_gate_template_count,
        warning_count,
    })
}
