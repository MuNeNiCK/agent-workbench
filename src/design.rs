use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::db::{default_design_root, open_existing_project, project_id};
use crate::rules::{RuleBindingInput, insert_rule_binding};

const ARC42_FILES: &[(&str, &str)] = &[
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
        "draft" | "imported" | "approved" | "superseded" | "archived" => {}
        _ => bail!("invalid design manifest status: {}", manifest.status),
    }
    for dependency in &manifest.depends_on {
        validate_design_id(dependency)?;
    }

    let mut design_files = manifest.design_files();
    design_files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    if design_files.is_empty() {
        bail!("design package has no importable design files");
    }

    let mut package_hasher = Sha256::new();
    package_hasher.update(manifest_text.as_bytes());
    let mut imported_files = Vec::with_capacity(design_files.len());
    for design_file in design_files {
        validate_relative_manifest_path(&design_file.relative_path)?;
        let path = package_path.join(&design_file.relative_path);
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let line_count = line_count(&content);
        if line_count > 1000 {
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
            project_id, design_key, title, status, created_at, updated_at
        )
        values (?1, ?2, ?3, ?4, current_timestamp, current_timestamp)
        on conflict(project_id, design_key) do update set
            title = excluded.title,
            status = excluded.status,
            updated_at = current_timestamp
        "#,
        params![project_id, manifest.id, manifest.title, input.status],
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
            project_id, design_package_id, version_number, content_hash,
            package_path, manifest_path, format, manifest_version, status, imported_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, current_timestamp)
        "#,
        params![
            project_id,
            design_package_id,
            version_number,
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
        if file.section_key == "requirements" {
            let requirements = extract_design_requirements(&file.content, file)?;
            requirement_count += requirements.len();
            for requirement in requirements {
                tx.execute(
                    r#"
                    insert into design_requirements(
                        project_id, design_version_id, source_design_file_id,
                        source_section, requirement_key, revision, requirement_hash,
                        requirement_text, priority, required_surfaces,
                        validation_expectation, status, created_at
                    )
                    values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, current_timestamp)
                    "#,
                    params![
                        project_id,
                        design_version_id,
                        design_file_id,
                        requirement.source_section,
                        requirement.requirement_key,
                        requirement.revision,
                        requirement.requirement_hash,
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
            }
        }
    }
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
    })
}

pub fn list_design_requirements(
    root: &Path,
    input: DesignRequirementListQuery,
) -> Result<Vec<DesignRequirementRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select
            r.id, r.design_version_id, r.source_design_file_id,
            f.relative_path, r.source_section, r.requirement_key,
            r.revision, r.requirement_text, r.priority,
            r.required_surfaces, r.validation_expectation, r.status
        from design_requirements r
        join design_files f on f.id = r.source_design_file_id
        where r.project_id = ?1 and r.design_version_id = ?2
        order by r.requirement_key
        "#,
    )?;
    let rows = stmt.query_map(params![project_id, input.design_version_id], |row| {
        Ok(DesignRequirementRecord {
            id: row.get(0)?,
            design_version_id: row.get(1)?,
            source_design_file_id: row.get(2)?,
            source_path: row.get(3)?,
            source_section: row.get(4)?,
            requirement_key: row.get(5)?,
            revision: row.get(6)?,
            requirement_text: row.get(7)?,
            priority: row.get(8)?,
            required_surfaces: row.get(9)?,
            validation_expectation: row.get(10)?,
            status: row.get(11)?,
        })
    })?;

    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

pub fn list_design_decisions(
    root: &Path,
    input: DesignDecisionListQuery,
) -> Result<Vec<DesignDecisionRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select
            d.id, d.design_version_id, d.source_design_file_id,
            f.relative_path, d.source_section, d.decision_key,
            d.topic, d.decision_text, d.rationale, d.supersedes_decision_keys, d.status
        from design_decisions d
        join design_files f on f.id = d.source_design_file_id
        where d.project_id = ?1 and d.design_version_id = ?2
        order by d.decision_key
        "#,
    )?;
    let rows = stmt.query_map(params![project_id, input.design_version_id], |row| {
        Ok(DesignDecisionRecord {
            id: row.get(0)?,
            design_version_id: row.get(1)?,
            source_design_file_id: row.get(2)?,
            source_path: row.get(3)?,
            source_section: row.get(4)?,
            decision_key: row.get(5)?,
            topic: row.get(6)?,
            decision_text: row.get(7)?,
            rationale: row.get(8)?,
            supersedes_decision_keys: row.get(9)?,
            status: row.get(10)?,
        })
    })?;

    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

pub fn list_validation_gate_templates(
    root: &Path,
    input: ValidationGateTemplateListQuery,
) -> Result<Vec<ValidationGateTemplateRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select
            g.id, g.design_version_id, g.source_design_file_id,
            f.relative_path, g.source_section, g.gate_key,
            g.stage, g.command, g.expected_result, g.requirement_keys, g.gate_text, g.status
        from validation_gate_templates g
        join design_files f on f.id = g.source_design_file_id
        where g.project_id = ?1 and g.design_version_id = ?2
        order by g.gate_key
        "#,
    )?;
    let rows = stmt.query_map(params![project_id, input.design_version_id], |row| {
        Ok(ValidationGateTemplateRecord {
            id: row.get(0)?,
            design_version_id: row.get(1)?,
            source_design_file_id: row.get(2)?,
            source_path: row.get(3)?,
            source_section: row.get(4)?,
            gate_key: row.get(5)?,
            stage: row.get(6)?,
            command: row.get(7)?,
            expected_result: row.get(8)?,
            requirement_keys: row.get(9)?,
            gate_text: row.get(10)?,
            status: row.get(11)?,
        })
    })?;

    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

pub fn accept_design_exception(
    root: &Path,
    input: NewDesignExceptionAcceptance<'_>,
) -> Result<DesignExceptionAcceptanceOutcome> {
    validate_design_acceptance_type(input.acceptance_type)?;
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let (target_type, design_requirement_id, validation_gate_template_id) =
        resolve_design_acceptance_target(&tx, project_id, input.design_version_id, input.target)?;

    tx.execute(
        r#"
        insert into authority_events(
            project_id, event_type, source, text_or_summary, scope, precedence,
            status, created_at
        )
        values (?1, 'user_instruction', 'design acceptance', ?2, ?3, 100, 'active', current_timestamp)
        "#,
        params![
            project_id,
            format!(
                "accepted design exception for {} on design version {}: {}",
                input.target, input.design_version_id, input.reason
            ),
            input.design_version_id.to_string(),
        ],
    )?;
    let authority_event_id = tx.last_insert_rowid();
    tx.execute(
        r#"
        insert into acceptance_records(
            project_id, target_type, design_requirement_id,
            validation_gate_template_id, acceptance_type, reason, scope,
            created_by, status, approved_by_authority_event_id, approved_at,
            created_at, review_impact
        )
        values (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7,
            'user', 'approved', ?8, current_timestamp,
            current_timestamp, 'design exception accepted for current design scope'
        )
        "#,
        params![
            project_id,
            target_type,
            design_requirement_id,
            validation_gate_template_id,
            input.acceptance_type,
            input.reason,
            input.design_version_id.to_string(),
            authority_event_id,
        ],
    )?;
    let acceptance_record_id = tx.last_insert_rowid();
    if input.acceptance_type == "accepted_out_of_scope" {
        match target_type {
            "design_requirement" => {
                tx.execute(
                    "update design_requirements set status = 'accepted_out_of_scope' where id = ?1",
                    params![design_requirement_id],
                )?;
            }
            "validation_gate_template" => {
                tx.execute(
                    "update validation_gate_templates set status = 'accepted_out_of_scope' where id = ?1",
                    params![validation_gate_template_id],
                )?;
            }
            _ => unreachable!("target type resolved above"),
        }
    }
    tx.commit()?;

    Ok(DesignExceptionAcceptanceOutcome {
        acceptance_record_id,
        authority_event_id,
        target_type: target_type.to_string(),
        design_requirement_id,
        validation_gate_template_id,
    })
}

pub fn approve_design_version(
    root: &Path,
    input: DesignVersionApproval<'_>,
) -> Result<DesignVersionApprovalOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let version = stored_design_version(&tx, project_id, input.design_version_id)?
        .context("design version not found")?;
    if version.current_design_version_id != Some(version.design_version_id) {
        bail!("only the current design version can be approved");
    }
    if version.status == "approved" {
        bail!("design version is already approved");
    }

    let summary = input.summary.map(str::to_string).unwrap_or_else(|| {
        format!(
            "approved design version {} for {}",
            version.design_version_id, version.design_key
        )
    });
    let source = format!("design_version:{}", version.design_version_id);
    tx.execute(
        r#"
        insert into authority_events(
            project_id, event_type, source, text_or_summary, scope, precedence,
            status, created_at
        )
        values (?1, 'design_doc', ?2, ?3, ?4, ?5, 'active', current_timestamp)
        "#,
        params![project_id, source, summary, version.design_key, 90],
    )?;
    let authority_event_id = tx.last_insert_rowid();
    insert_rule_binding(
        &tx,
        RuleBindingInput {
            project_id,
            rule_source_type: "authority_event",
            authority_event_id: Some(authority_event_id),
            user_correction_id: None,
            command_profile_id: None,
            work_unit_id: None,
            scope_type: "design_package",
            scope_key: Some(&version.design_key),
            precedence: 90,
        },
    )?;
    tx.execute(
        r#"
        update design_versions
        set status = 'approved', approved_by_authority_event_id = ?1
        where id = ?2
        "#,
        params![authority_event_id, version.design_version_id],
    )?;
    tx.execute(
        r#"
        update design_packages
        set status = 'approved', updated_at = current_timestamp
        where id = ?1
        "#,
        params![version.design_package_id],
    )?;
    tx.commit()?;

    Ok(DesignVersionApprovalOutcome {
        design_package_id: version.design_package_id,
        design_version_id: version.design_version_id,
        authority_event_id,
    })
}

pub fn design_ready(root: &Path, input: DesignReadyCheck) -> Result<DesignReadyOutcome> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut items = Vec::new();

    let version = match resolve_design_version_for_gate(&conn, project_id, input.design_version_id)?
    {
        Some(version) => {
            items.push(DesignReadyItem::pass("design_version_exists", None));
            version
        }
        None => {
            items.push(DesignReadyItem::fail(
                "design_version_exists",
                Some("import a design package first"),
            ));
            return Ok(DesignReadyOutcome::blocked(
                input.design_version_id,
                None,
                "no design version is available",
                items,
            ));
        }
    };

    if version.current_design_version_id == Some(version.design_version_id) {
        items.push(DesignReadyItem::pass("design_version_current", None));
    } else {
        items.push(DesignReadyItem::fail(
            "design_version_current",
            Some("import or select the current design version"),
        ));
    }

    let file_count: i64 = conn.query_row(
        "select count(*) from design_files where design_version_id = ?1",
        params![version.design_version_id],
        |row| row.get(0),
    )?;
    if file_count > 0 {
        items.push(DesignReadyItem::pass(
            "design_files_imported",
            Some(format!("{file_count} files")),
        ));
    } else {
        items.push(DesignReadyItem::fail(
            "design_files_imported",
            Some("imported design version has no files"),
        ));
    }

    let active_requirement_count: i64 = conn.query_row(
        "select count(*) from design_requirements where design_version_id = ?1 and status = 'active'",
        params![version.design_version_id],
        |row| row.get(0),
    )?;
    if active_requirement_count > 0 {
        items.push(DesignReadyItem::pass(
            "active_requirements_extracted",
            Some(format!("{active_requirement_count} requirements")),
        ));
    } else {
        items.push(DesignReadyItem::fail(
            "active_requirements_extracted",
            Some("add requirement records to requirements/*.md"),
        ));
    }

    let missing_validation_count: i64 = conn.query_row(
        r#"
        select count(*)
        from design_requirements
        where design_version_id = ?1
          and status = 'active'
          and (validation_expectation is null or validation_expectation = '')
        "#,
        params![version.design_version_id],
        |row| row.get(0),
    )?;
    if missing_validation_count == 0 {
        items.push(DesignReadyItem::pass(
            "requirement_validation_defined",
            None,
        ));
    } else {
        items.push(DesignReadyItem::fail(
            "requirement_validation_defined",
            Some("every active requirement needs validation metadata"),
        ));
    }

    if version.status == "approved" && version.approved_by_authority_event_id.is_some() {
        items.push(DesignReadyItem::pass("design_version_approved", None));
    } else {
        items.push(DesignReadyItem::fail(
            "design_version_approved",
            Some("approve the design version before implementation planning"),
        ));
    }

    let result = if items.iter().all(|item| item.result == "pass") {
        "pass"
    } else {
        "blocked"
    };
    let blocking_reason = if result == "pass" {
        None
    } else {
        Some("design version is not ready".to_string())
    };
    Ok(DesignReadyOutcome {
        result: result.to_string(),
        blocking_reason,
        design_package_id: Some(version.design_package_id),
        design_version_id: Some(version.design_version_id),
        items,
    })
}

fn validate_design_id(design_id: &str) -> Result<()> {
    if design_id.is_empty() {
        bail!("design id is required");
    }
    let valid = design_id
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if !valid {
        bail!("design id must contain only lowercase letters, digits, and hyphens");
    }
    Ok(())
}

fn validate_import_status(status: &str) -> Result<()> {
    match status {
        "draft" | "imported" => Ok(()),
        _ => bail!("design import status must be draft or imported"),
    }
}

fn resolve_package_path(root: &Path, package_path: &Path) -> Result<PathBuf> {
    let path = if package_path.is_absolute() {
        package_path.to_path_buf()
    } else {
        root.join(package_path)
    };
    path.canonicalize()
        .with_context(|| format!("failed to resolve {}", path.display()))
}

fn ensure_package_path_is_under_design_root(
    root: &Path,
    design_root: &Path,
    package_path: &Path,
) -> Result<()> {
    let canonical_root = root
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", root.display()))?;
    let canonical_design_root = design_root
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", design_root.display()))?;
    if !package_path.starts_with(&canonical_design_root) {
        bail!(
            "design import source must be under {}",
            canonical_design_root
                .strip_prefix(&canonical_root)
                .unwrap_or(&canonical_design_root)
                .display()
        );
    }
    Ok(())
}

fn validate_relative_manifest_path(path: &str) -> Result<()> {
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        bail!("design manifest paths must be relative package paths");
    }
    Ok(())
}

fn stored_design_version(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_version_id: i64,
) -> Result<Option<StoredDesignVersion>> {
    conn.query_row(
        r#"
        select
            v.id, v.design_package_id, v.status, v.approved_by_authority_event_id,
            p.design_key, p.current_design_version_id
        from design_versions v
        join design_packages p on p.id = v.design_package_id
        where v.project_id = ?1 and v.id = ?2
        "#,
        params![project_id, design_version_id],
        stored_design_version_row,
    )
    .optional()
    .map_err(Into::into)
}

fn resolve_design_version_for_gate(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_version_id: Option<i64>,
) -> Result<Option<StoredDesignVersion>> {
    match design_version_id {
        Some(id) => stored_design_version(conn, project_id, id),
        None => {
            let current_count: i64 = conn.query_row(
                "select count(*) from design_packages where project_id = ?1 and current_design_version_id is not null",
                params![project_id],
                |row| row.get(0),
            )?;
            if current_count != 1 {
                return Ok(None);
            }
            conn.query_row(
                r#"
                select
                    v.id, v.design_package_id, v.status, v.approved_by_authority_event_id,
                    p.design_key, p.current_design_version_id
                from design_packages p
                join design_versions v on v.id = p.current_design_version_id
                where p.project_id = ?1
                "#,
                params![project_id],
                stored_design_version_row,
            )
            .optional()
            .map_err(Into::into)
        }
    }
}

fn resolve_design_acceptance_target(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_version_id: i64,
    target: &str,
) -> Result<(&'static str, Option<i64>, Option<i64>)> {
    validate_design_acceptance_type_target(target)?;
    if let Some(requirement_key) = target.strip_prefix("requirement:") {
        let id = conn
            .query_row(
                r#"
                select id
                from design_requirements
                where project_id = ?1 and design_version_id = ?2 and requirement_key = ?3
                "#,
                params![project_id, design_version_id, requirement_key],
                |row| row.get(0),
            )
            .optional()?
            .context("design requirement target not found")?;
        return Ok(("design_requirement", Some(id), None));
    }
    if let Some(gate_key) = target.strip_prefix("gate:") {
        let id = conn
            .query_row(
                r#"
                select id
                from validation_gate_templates
                where project_id = ?1 and design_version_id = ?2 and gate_key = ?3
                "#,
                params![project_id, design_version_id, gate_key],
                |row| row.get(0),
            )
            .optional()?
            .context("validation gate template target not found")?;
        return Ok(("validation_gate_template", None, Some(id)));
    }
    unreachable!("target was validated above")
}

fn validate_design_acceptance_type_target(target: &str) -> Result<()> {
    if target
        .strip_prefix("requirement:")
        .is_some_and(|key| valid_design_key(key, "REQ"))
        || target
            .strip_prefix("gate:")
            .is_some_and(|key| valid_design_key(key, "GATE"))
    {
        return Ok(());
    }
    bail!("acceptance target must be requirement:<key> or gate:<key>");
}

fn validate_design_acceptance_type(acceptance_type: &str) -> Result<()> {
    match acceptance_type {
        "accepted_out_of_scope" | "explicit_exception" => Ok(()),
        _ => bail!("acceptance type must be accepted_out_of_scope or explicit_exception"),
    }
}

fn valid_design_key(value: &str, prefix: &str) -> bool {
    let expected_prefix = format!("{prefix}-");
    let Some(number) = value.strip_prefix(&expected_prefix) else {
        return false;
    };
    !number.is_empty()
        && number.bytes().all(|byte| byte.is_ascii_digit())
        && number.bytes().any(|byte| byte != b'0')
}

fn heading_key_matches(source_section: &str, key: &str) -> bool {
    source_section
        .split_once(':')
        .map(|(heading_key, _)| heading_key.trim() == key)
        .unwrap_or_else(|| source_section.trim() == key)
}

fn title_from_section(source_section: &str, key: &str) -> String {
    source_section
        .split_once(':')
        .map(|(_, title)| title.trim())
        .filter(|title| !title.is_empty())
        .unwrap_or(key)
        .to_string()
}

fn normalize_validation_phase(phase: &str) -> Result<String> {
    match phase {
        "design-ready" | "implementation-ready" | "close-ready" | "resume-ready" => {
            Ok(phase.to_string())
        }
        "design" => Ok("design-ready".to_string()),
        "implementation" => Ok("implementation-ready".to_string()),
        "close" => Ok("close-ready".to_string()),
        "resume" => Ok("resume-ready".to_string()),
        _ => bail!("invalid validation gate phase: {phase}"),
    }
}

fn validate_expected_result(expected_result: &str) -> Result<()> {
    match expected_result {
        "pass" | "blocked" | "needs_evidence" | "fail" => Ok(()),
        _ => bail!("invalid validation gate expected_result: {expected_result}"),
    }
}

fn stored_design_version_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredDesignVersion> {
    Ok(StoredDesignVersion {
        design_version_id: row.get(0)?,
        design_package_id: row.get(1)?,
        status: row.get(2)?,
        approved_by_authority_event_id: row.get(3)?,
        design_key: row.get(4)?,
        current_design_version_id: row.get(5)?,
    })
}

fn extract_design_requirements(
    content: &str,
    file: &ImportedDesignFile,
) -> Result<Vec<ExtractedDesignRequirement>> {
    reject_legacy_headings(content, file, &["R-"], "REQ-")?;
    let lines: Vec<&str> = content.lines().collect();
    let mut requirements = Vec::new();
    let mut seen_keys = BTreeSet::new();
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index];
        if !line.starts_with("## ") {
            index += 1;
            continue;
        }
        let source_section = line.trim_start_matches("## ").trim().to_string();
        if !source_section.starts_with("REQ-") {
            index += 1;
            continue;
        }
        let fence_start = index + 1;
        if lines.get(fence_start).map(|line| line.trim()) != Some("```yaml agent-workbench") {
            bail!(
                "requirement {} in {} must be followed by a yaml agent-workbench block",
                source_section,
                file.relative_path
            );
        }
        let mut fence_end = fence_start + 1;
        while fence_end < lines.len() && lines[fence_end].trim() != "```" {
            fence_end += 1;
        }
        if fence_end == lines.len() {
            bail!(
                "requirement {} in {} has an unterminated yaml block",
                source_section,
                file.relative_path
            );
        }
        let metadata_text = lines[fence_start + 1..fence_end].join("\n");
        let metadata: RequirementMetadata =
            yaml_serde::from_str(&metadata_text).with_context(|| {
                format!(
                    "failed to parse requirement metadata for {} in {}",
                    source_section, file.relative_path
                )
            })?;
        validate_requirement_metadata(&source_section, &metadata, &mut seen_keys)?;

        let mut body_end = fence_end + 1;
        while body_end < lines.len() && !lines[body_end].starts_with("## ") {
            body_end += 1;
        }
        let body = lines[fence_end + 1..body_end].join("\n").trim().to_string();
        if body.is_empty() {
            bail!(
                "requirement {} in {} must have body text",
                metadata.key,
                file.relative_path
            );
        }
        let body_line_count = body.lines().count();
        if body_line_count > 150 {
            bail!(
                "requirement {} exceeds 150 lines in {}",
                metadata.key,
                file.relative_path
            );
        }

        let mut hasher = Sha256::new();
        hasher.update(metadata_text.as_bytes());
        hasher.update(b"\0");
        hasher.update(body.as_bytes());
        let digest = hasher.finalize();
        let revision = metadata.revision.unwrap_or(1);
        if revision <= 0 {
            bail!("requirement revision must be positive");
        }
        requirements.push(ExtractedDesignRequirement {
            source_section,
            requirement_key: metadata.key,
            revision,
            requirement_hash: hex_digest(&digest),
            requirement_text: body,
            priority: metadata.priority,
            required_surfaces: join_metadata_list(&metadata.surfaces),
            validation_expectation: join_metadata_list(&metadata.validation),
            status: metadata.status,
        });
        index = body_end;
    }
    Ok(requirements)
}

fn validate_requirement_metadata(
    source_section: &str,
    metadata: &RequirementMetadata,
    seen_keys: &mut BTreeSet<String>,
) -> Result<()> {
    if metadata.record_type != "requirement" {
        bail!("requirement metadata type must be requirement");
    }
    if !valid_design_key(&metadata.key, "REQ") {
        bail!("requirement key must match REQ-<positive-number>");
    }
    if !heading_key_matches(source_section, &metadata.key) {
        bail!("requirement heading must start with metadata key");
    }
    if !seen_keys.insert(metadata.key.clone()) {
        bail!("duplicate requirement key: {}", metadata.key);
    }
    match metadata.priority.as_str() {
        "critical" | "high" | "medium" | "low" => {}
        _ => bail!("invalid requirement priority: {}", metadata.priority),
    }
    match metadata.status.as_str() {
        "active" | "superseded" | "accepted_out_of_scope" => {}
        _ => bail!("invalid requirement status: {}", metadata.status),
    }
    if metadata.validation.is_empty() && metadata.status == "active" {
        bail!("active requirement must declare validation metadata");
    }
    Ok(())
}

fn extract_design_decisions(
    content: &str,
    file: &ImportedDesignFile,
) -> Result<Vec<ExtractedDesignDecision>> {
    reject_legacy_headings(content, file, &["D-"], "DEC-")?;
    let blocks = extract_agent_workbench_blocks(content, file, "DEC-", "decision")?;
    let mut decisions = Vec::with_capacity(blocks.len());
    let mut seen_keys = BTreeSet::new();
    for block in blocks {
        let metadata: DecisionMetadata =
            yaml_serde::from_str(&block.metadata_text).with_context(|| {
                format!(
                    "failed to parse decision metadata for {} in {}",
                    block.source_section, file.relative_path
                )
            })?;
        validate_decision_metadata(&block.source_section, &metadata, &mut seen_keys)?;
        let body = block.body.trim().to_string();
        if body.is_empty() {
            bail!(
                "decision {} in {} must have body text",
                metadata.key,
                file.relative_path
            );
        }
        if body.lines().count() > 150 {
            bail!(
                "decision {} exceeds 150 lines in {}",
                metadata.key,
                file.relative_path
            );
        }
        let mut hasher = Sha256::new();
        hasher.update(block.metadata_text.as_bytes());
        hasher.update(b"\0");
        hasher.update(body.as_bytes());
        let digest = hasher.finalize();
        decisions.push(ExtractedDesignDecision {
            topic: title_from_section(&block.source_section, &metadata.key),
            source_section: block.source_section,
            decision_key: metadata.key,
            decision_hash: hex_digest(&digest),
            decision_text: body,
            rationale: None,
            supersedes_decision_keys: join_metadata_list(&metadata.supersedes),
            status: metadata.status,
        });
    }
    Ok(decisions)
}

fn validate_decision_metadata(
    source_section: &str,
    metadata: &DecisionMetadata,
    seen_keys: &mut BTreeSet<String>,
) -> Result<()> {
    if metadata.record_type != "decision" {
        bail!("decision metadata type must be decision");
    }
    if !valid_design_key(&metadata.key, "DEC") {
        bail!("decision key must match DEC-<positive-number>");
    }
    if !heading_key_matches(source_section, &metadata.key) {
        bail!("decision heading must start with metadata key");
    }
    if !seen_keys.insert(metadata.key.clone()) {
        bail!("duplicate decision key: {}", metadata.key);
    }
    for superseded_key in &metadata.supersedes {
        if !valid_design_key(superseded_key, "DEC") {
            bail!("superseded decision key must match DEC-<positive-number>");
        }
    }
    match metadata.status.as_str() {
        "accepted" | "rejected" | "superseded" => {}
        _ => bail!("invalid decision status: {}", metadata.status),
    }
    Ok(())
}

fn extract_validation_gate_templates(
    content: &str,
    file: &ImportedDesignFile,
) -> Result<Vec<ExtractedValidationGateTemplate>> {
    reject_legacy_headings(content, file, &["VG-", "VAL-"], "GATE-")?;
    let blocks = extract_agent_workbench_blocks(content, file, "GATE-", "validation gate")?;
    let mut templates = Vec::with_capacity(blocks.len());
    let mut seen_keys = BTreeSet::new();
    for block in blocks {
        let metadata: ValidationGateTemplateMetadata = yaml_serde::from_str(&block.metadata_text)
            .with_context(|| {
            format!(
                "failed to parse validation gate metadata for {} in {}",
                block.source_section, file.relative_path
            )
        })?;
        validate_validation_gate_template_metadata(
            &block.source_section,
            &metadata,
            &mut seen_keys,
        )?;
        let body = block.body.trim().to_string();
        if body.is_empty() {
            bail!(
                "validation gate {} in {} must have body text",
                metadata.key,
                file.relative_path
            );
        }
        if body.lines().count() > 150 {
            bail!(
                "validation gate {} exceeds 150 lines in {}",
                metadata.key,
                file.relative_path
            );
        }
        let mut hasher = Sha256::new();
        hasher.update(block.metadata_text.as_bytes());
        hasher.update(b"\0");
        hasher.update(body.as_bytes());
        let digest = hasher.finalize();
        templates.push(ExtractedValidationGateTemplate {
            source_section: block.source_section,
            gate_key: metadata.key,
            gate_hash: hex_digest(&digest),
            stage: normalize_validation_phase(&metadata.phase)?,
            command: metadata.command_template,
            expected_result: metadata.expected_result,
            requirement_keys: join_metadata_list(&metadata.applies_to),
            gate_text: body,
            status: metadata.status,
        });
    }
    Ok(templates)
}

fn validate_validation_gate_template_metadata(
    source_section: &str,
    metadata: &ValidationGateTemplateMetadata,
    seen_keys: &mut BTreeSet<String>,
) -> Result<()> {
    if metadata.record_type != "validation_gate_template" {
        bail!("validation gate metadata type must be validation_gate_template");
    }
    if !valid_design_key(&metadata.key, "GATE") {
        bail!("validation gate key must match GATE-<positive-number>");
    }
    if !heading_key_matches(source_section, &metadata.key) {
        bail!("validation gate heading must start with metadata key");
    }
    if !seen_keys.insert(metadata.key.clone()) {
        bail!("duplicate validation gate key: {}", metadata.key);
    }
    normalize_validation_phase(&metadata.phase)?;
    validate_expected_result(&metadata.expected_result)?;
    match metadata.status.as_str() {
        "active" | "superseded" | "accepted_out_of_scope" => {}
        _ => bail!("invalid validation gate status: {}", metadata.status),
    }
    for requirement_key in &metadata.applies_to {
        if !valid_design_key(requirement_key, "REQ") {
            bail!("validation gate applies_to keys must match REQ-<positive-number>");
        }
    }
    if metadata.applies_to.is_empty() && metadata.status == "active" {
        bail!("active validation gate must declare applies_to metadata");
    }
    Ok(())
}

fn reject_legacy_headings(
    content: &str,
    file: &ImportedDesignFile,
    legacy_prefixes: &[&str],
    required_prefix: &str,
) -> Result<()> {
    for line in content.lines() {
        if !line.starts_with("## ") {
            continue;
        }
        let heading = line.trim_start_matches("## ").trim();
        if legacy_prefixes
            .iter()
            .any(|prefix| heading.starts_with(prefix))
        {
            bail!(
                "unsupported legacy design heading {} in {}; use {} keys",
                heading,
                file.relative_path,
                required_prefix
            );
        }
    }
    Ok(())
}

fn extract_agent_workbench_blocks(
    content: &str,
    file: &ImportedDesignFile,
    heading_prefix: &str,
    label: &str,
) -> Result<Vec<ExtractedBlock>> {
    let lines: Vec<&str> = content.lines().collect();
    let mut blocks = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index];
        if !line.starts_with("## ") {
            index += 1;
            continue;
        }
        let source_section = line.trim_start_matches("## ").trim().to_string();
        if !source_section.starts_with(heading_prefix) {
            index += 1;
            continue;
        }
        let fence_start = index + 1;
        if lines.get(fence_start).map(|line| line.trim()) != Some("```yaml agent-workbench") {
            bail!(
                "{} {} in {} must be followed by a yaml agent-workbench block",
                label,
                source_section,
                file.relative_path
            );
        }
        let mut fence_end = fence_start + 1;
        while fence_end < lines.len() && lines[fence_end].trim() != "```" {
            fence_end += 1;
        }
        if fence_end == lines.len() {
            bail!(
                "{} {} in {} has an unterminated yaml block",
                label,
                source_section,
                file.relative_path
            );
        }
        let mut body_end = fence_end + 1;
        while body_end < lines.len() && !lines[body_end].starts_with("## ") {
            body_end += 1;
        }
        blocks.push(ExtractedBlock {
            source_section,
            metadata_text: lines[fence_start + 1..fence_end].join("\n"),
            body: lines[fence_end + 1..body_end].join("\n"),
        });
        index = body_end;
    }
    Ok(blocks)
}

fn join_metadata_list(values: &[String]) -> Option<String> {
    if values.is_empty() {
        None
    } else {
        Some(values.join(","))
    }
}

fn design_manifest(design_id: &str, title: &str) -> String {
    let design_id = yaml_string(design_id);
    let title = yaml_string(title);
    format!(
        r#"id: {design_id}
title: {title}
format: arc42-agent-workbench
version: 1
status: draft

arc42:
  introduction_goals: 01-introduction-goals.md
  constraints: 02-constraints.md
  context_scope: 03-context-scope.md
  solution_strategy: 04-solution-strategy.md
  building_blocks: 05-building-blocks.md
  runtime_view: 06-runtime-view.md
  deployment_view: 07-deployment-view.md
  crosscutting_concepts: 08-crosscutting-concepts.md
  decisions: 09-decisions.md
  quality_requirements: 10-quality-requirements.md
  risks_technical_debt: 11-risks-technical-debt.md
  glossary: 12-glossary.md

requirements:
  - requirements/README.md

validation:
  - validation/gates.md

depends_on: []
"#
    )
}

fn yaml_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    format!("\"{escaped}\"")
}

fn line_count(content: &str) -> i64 {
    content.lines().count().try_into().unwrap_or(i64::MAX)
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    hex_digest(&digest)
}

fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn markdown_stub(title: &str) -> String {
    format!("# {title}\n\n")
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DesignManifest {
    id: String,
    title: String,
    format: String,
    version: i64,
    status: String,
    arc42: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    requirements: Vec<String>,
    #[serde(default)]
    validation: Vec<String>,
    #[serde(default)]
    depends_on: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequirementMetadata {
    #[serde(rename = "type")]
    record_type: String,
    key: String,
    #[serde(default)]
    revision: Option<i64>,
    priority: String,
    #[serde(default)]
    surfaces: Vec<String>,
    #[serde(default)]
    validation: Vec<String>,
    status: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionMetadata {
    #[serde(rename = "type")]
    record_type: String,
    key: String,
    status: String,
    #[serde(default)]
    supersedes: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidationGateTemplateMetadata {
    #[serde(rename = "type")]
    record_type: String,
    key: String,
    phase: String,
    expected_result: String,
    #[serde(default)]
    applies_to: Vec<String>,
    #[serde(default)]
    command_template: Option<String>,
    status: String,
}

impl DesignManifest {
    fn design_files(&self) -> Vec<ManifestDesignFile> {
        let mut files = Vec::new();
        files.extend(
            self.arc42
                .iter()
                .map(|(section_key, relative_path)| ManifestDesignFile {
                    section_key: format!("arc42.{section_key}"),
                    relative_path: relative_path.clone(),
                }),
        );
        files.extend(
            self.requirements
                .iter()
                .map(|relative_path| ManifestDesignFile {
                    section_key: "requirements".to_string(),
                    relative_path: relative_path.clone(),
                }),
        );
        files.extend(
            self.validation
                .iter()
                .map(|relative_path| ManifestDesignFile {
                    section_key: "validation".to_string(),
                    relative_path: relative_path.clone(),
                }),
        );
        files
    }
}

struct ManifestDesignFile {
    section_key: String,
    relative_path: String,
}

struct ImportedDesignFile {
    section_key: String,
    relative_path: String,
    content_hash: String,
    line_count: i64,
    content: String,
}

struct ExtractedDesignRequirement {
    source_section: String,
    requirement_key: String,
    revision: i64,
    requirement_hash: String,
    requirement_text: String,
    priority: String,
    required_surfaces: Option<String>,
    validation_expectation: Option<String>,
    status: String,
}

struct ExtractedDesignDecision {
    source_section: String,
    decision_key: String,
    decision_hash: String,
    topic: String,
    decision_text: String,
    rationale: Option<String>,
    supersedes_decision_keys: Option<String>,
    status: String,
}

struct ExtractedValidationGateTemplate {
    source_section: String,
    gate_key: String,
    gate_hash: String,
    stage: String,
    command: Option<String>,
    expected_result: String,
    requirement_keys: Option<String>,
    gate_text: String,
    status: String,
}

struct ExtractedBlock {
    source_section: String,
    metadata_text: String,
    body: String,
}

struct StoredDesignVersion {
    design_version_id: i64,
    design_package_id: i64,
    status: String,
    approved_by_authority_event_id: Option<i64>,
    design_key: String,
    current_design_version_id: Option<i64>,
}

pub struct NewDesignPackage<'a> {
    pub design_id: &'a str,
    pub title: &'a str,
}

pub struct DesignPackageImport<'a> {
    pub package_path: &'a Path,
    pub status: &'a str,
}

pub struct DesignVersionApproval<'a> {
    pub design_version_id: i64,
    pub summary: Option<&'a str>,
}

pub struct DesignReadyCheck {
    pub design_version_id: Option<i64>,
}

pub struct DesignRequirementListQuery {
    pub design_version_id: i64,
}

pub struct DesignDecisionListQuery {
    pub design_version_id: i64,
}

pub struct ValidationGateTemplateListQuery {
    pub design_version_id: i64,
}

pub struct NewDesignExceptionAcceptance<'a> {
    pub design_version_id: i64,
    pub target: &'a str,
    pub acceptance_type: &'a str,
    pub reason: &'a str,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DesignPackageInitOutcome {
    pub package_path: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DesignPackageImportOutcome {
    pub design_package_id: i64,
    pub design_version_id: i64,
    pub version_number: i64,
    pub content_hash: String,
    pub file_count: usize,
    pub requirement_count: usize,
    pub decision_count: usize,
    pub validation_gate_template_count: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DesignVersionApprovalOutcome {
    pub design_package_id: i64,
    pub design_version_id: i64,
    pub authority_event_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DesignReadyOutcome {
    pub result: String,
    pub blocking_reason: Option<String>,
    pub design_package_id: Option<i64>,
    pub design_version_id: Option<i64>,
    pub items: Vec<DesignReadyItem>,
}

impl DesignReadyOutcome {
    fn blocked(
        requested_design_version_id: Option<i64>,
        design_package_id: Option<i64>,
        reason: &str,
        items: Vec<DesignReadyItem>,
    ) -> Self {
        Self {
            result: "blocked".to_string(),
            blocking_reason: Some(reason.to_string()),
            design_package_id,
            design_version_id: requested_design_version_id,
            items,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct DesignReadyItem {
    pub name: String,
    pub result: String,
    pub detail: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DesignRequirementRecord {
    pub id: i64,
    pub design_version_id: i64,
    pub source_design_file_id: i64,
    pub source_path: String,
    pub source_section: String,
    pub requirement_key: String,
    pub revision: i64,
    pub requirement_text: String,
    pub priority: String,
    pub required_surfaces: Option<String>,
    pub validation_expectation: Option<String>,
    pub status: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DesignDecisionRecord {
    pub id: i64,
    pub design_version_id: i64,
    pub source_design_file_id: i64,
    pub source_path: String,
    pub source_section: String,
    pub decision_key: String,
    pub topic: String,
    pub decision_text: String,
    pub rationale: Option<String>,
    pub supersedes_decision_keys: Option<String>,
    pub status: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ValidationGateTemplateRecord {
    pub id: i64,
    pub design_version_id: i64,
    pub source_design_file_id: i64,
    pub source_path: String,
    pub source_section: String,
    pub gate_key: String,
    pub stage: String,
    pub command: Option<String>,
    pub expected_result: String,
    pub requirement_keys: Option<String>,
    pub gate_text: String,
    pub status: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DesignExceptionAcceptanceOutcome {
    pub acceptance_record_id: i64,
    pub authority_event_id: i64,
    pub target_type: String,
    pub design_requirement_id: Option<i64>,
    pub validation_gate_template_id: Option<i64>,
}

impl DesignReadyItem {
    fn pass(name: &str, detail: Option<String>) -> Self {
        Self {
            name: name.to_string(),
            result: "pass".to_string(),
            detail,
        }
    }

    fn fail(name: &str, detail: Option<&str>) -> Self {
        Self {
            name: name.to_string(),
            result: "fail".to_string(),
            detail: detail.map(str::to_string),
        }
    }
}
