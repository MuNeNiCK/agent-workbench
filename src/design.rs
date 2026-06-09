use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::db::{default_design_root, open_existing_project, project_id};
use crate::review_context::required_plans_missing_context_count;
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

const ARC42_KEYS: &[&str] = &[
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
        "draft" | "imported" | "approved" | "superseded" | "archived" => {}
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

fn mark_stale_links_for_design_version(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_package_id: i64,
    current_design_version_id: i64,
) -> Result<()> {
    let stale_requirement = r#"
        select 1
        from design_requirements old_req
        join design_versions old_version on old_version.id = old_req.design_version_id
        where old_req.id = design_requirements.id
          and old_req.project_id = ?1
          and old_version.design_package_id = ?2
          and old_req.design_version_id != ?3
          and not exists (
              select 1
              from design_requirements current_req
              where current_req.project_id = old_req.project_id
                and current_req.design_version_id = ?3
                and current_req.requirement_key = old_req.requirement_key
                and current_req.requirement_hash = old_req.requirement_hash
                and current_req.status = 'active'
          )
    "#;
    conn.execute(
        &format!(
            r#"
            update task_derivations
            set status = 'stale'
            where project_id = ?1
              and status = 'active'
              and exists (
                  select 1
                  from design_requirements
                  where design_requirements.id = task_derivations.design_requirement_id
                    and exists ({stale_requirement})
              )
            "#
        ),
        params![project_id, design_package_id, current_design_version_id],
    )?;
    conn.execute(
        &format!(
            r#"
            update checklists
            set status = 'stale'
            where project_id = ?1
              and status = 'active'
              and exists (
                  select 1
                  from checklist_items item
                  join design_requirements on design_requirements.id = item.design_requirement_id
                  where item.checklist_id = checklists.id
                    and exists ({stale_requirement})
              )
            "#
        ),
        params![project_id, design_package_id, current_design_version_id],
    )?;
    conn.execute(
        &format!(
            r#"
            update coverage_items
            set status = 'stale'
            where project_id = ?1
              and status != 'stale'
              and exists (
                  select 1
                  from design_requirements
                  where design_requirements.id = coverage_items.design_requirement_id
                    and exists ({stale_requirement})
              )
            "#
        ),
        params![project_id, design_package_id, current_design_version_id],
    )?;
    conn.execute(
        r#"
        update validation_gates
        set status = 'stale'
        where project_id = ?1
          and status = 'active'
          and (
              exists (
                  select 1
                  from validation_gate_templates old_gate
                  join design_versions old_version on old_version.id = old_gate.design_version_id
                  where old_gate.id = validation_gates.template_id
                    and old_gate.project_id = ?1
                    and old_version.design_package_id = ?2
                    and old_gate.design_version_id != ?3
                    and not exists (
                        select 1
                        from validation_gate_templates current_gate
                        where current_gate.project_id = old_gate.project_id
                          and current_gate.design_version_id = ?3
                          and current_gate.gate_key = old_gate.gate_key
                          and current_gate.gate_hash = old_gate.gate_hash
                          and current_gate.status = 'active'
                    )
              )
              or exists (
                  select 1
                  from design_requirements old_req
                  join design_versions old_version on old_version.id = old_req.design_version_id
                  where old_req.id = validation_gates.design_requirement_id
                    and old_req.project_id = ?1
                    and old_version.design_package_id = ?2
                    and old_req.design_version_id != ?3
                    and not exists (
                        select 1
                        from design_requirements current_req
                        where current_req.project_id = old_req.project_id
                          and current_req.design_version_id = ?3
                          and current_req.requirement_key = old_req.requirement_key
                          and current_req.requirement_hash = old_req.requirement_hash
                          and current_req.status = 'active'
                    )
              )
          )
        "#,
        params![project_id, design_package_id, current_design_version_id],
    )?;
    conn.execute(
        r#"
        update review_plans
        set status = 'blocked'
        where project_id = ?1
          and status = 'open'
          and design_version_id in (
              select id
              from design_versions
              where design_package_id = ?2 and id != ?3
          )
        "#,
        params![project_id, design_package_id, current_design_version_id],
    )?;
    Ok(())
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
    match (input.design_version_id, input.design_package) {
        (Some(_), None) | (None, Some(_)) => {}
        _ => bail!("provide exactly one of design_version_id or design_package"),
    }
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let target = match (input.design_version_id, input.design_package) {
        (Some(design_version_id), None) => {
            resolve_design_acceptance_target(&tx, project_id, design_version_id, input.target)?
        }
        (None, Some(design_package)) => {
            resolve_pre_import_design_acceptance_target(design_package, input.target)?
        }
        _ => unreachable!("validated above"),
    };
    let scope = match (input.design_version_id, input.design_package) {
        (Some(design_version_id), None) => design_version_id.to_string(),
        (None, Some(design_package)) => design_package.to_string(),
        _ => unreachable!("validated above"),
    };

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
                "accepted design exception for {} on {}: {}",
                input.target, scope, input.reason
            ),
            scope,
        ],
    )?;
    let authority_event_id = tx.last_insert_rowid();
    tx.execute(
        r#"
        insert into acceptance_records(
            project_id, target_type, task_id, design_requirement_id,
            validation_gate_template_id, coverage_item_id, design_package_key,
            design_file_path, design_requirement_key, acceptance_type, reason, scope,
            created_by, status, approved_by_authority_event_id, approved_at,
            created_at, review_impact
        )
        values (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
            'user', 'approved', ?13, current_timestamp,
            current_timestamp, 'design exception accepted for current design scope'
        )
        "#,
        params![
            project_id,
            target.target_type,
            target.task_id,
            target.design_requirement_id,
            target.validation_gate_template_id,
            target.coverage_item_id,
            target.design_package_key,
            target.design_file_path,
            target.design_requirement_key,
            input.acceptance_type,
            input.reason,
            scope,
            authority_event_id,
        ],
    )?;
    let acceptance_record_id = tx.last_insert_rowid();
    if input.acceptance_type == "accepted_out_of_scope" {
        match target.target_type {
            "design_requirement" => {
                tx.execute(
                    "update design_requirements set status = 'accepted_out_of_scope' where id = ?1",
                    params![target.design_requirement_id],
                )?;
            }
            "validation_gate_template" => {
                tx.execute(
                    "update validation_gate_templates set status = 'accepted_out_of_scope' where id = ?1",
                    params![target.validation_gate_template_id],
                )?;
            }
            "coverage_item" => {
                tx.execute(
                    "update coverage_items set status = 'accepted_out_of_scope' where id = ?1",
                    params![target.coverage_item_id],
                )?;
            }
            "design_file" | "design_requirement_key" => {}
            _ => unreachable!("target type resolved above"),
        }
    }
    tx.commit()?;

    Ok(DesignExceptionAcceptanceOutcome {
        acceptance_record_id,
        authority_event_id,
        target_type: target.target_type.to_string(),
        design_requirement_id: target.design_requirement_id,
        validation_gate_template_id: target.validation_gate_template_id,
        coverage_item_id: target.coverage_item_id,
        design_package_key: target.design_package_key,
        design_file_path: target.design_file_path,
        design_requirement_key: target.design_requirement_key,
    })
}

pub fn add_general_acceptance(
    root: &Path,
    input: NewGeneralAcceptance<'_>,
) -> Result<GeneralAcceptanceOutcome> {
    validate_acceptance_type(input.acceptance_type)?;
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let target = resolve_general_acceptance_target(&tx, project_id, input.target)?;
    tx.execute(
        r#"
        insert into authority_events(
            project_id, event_type, source, text_or_summary, scope, precedence,
            status, created_at
        )
        values (?1, 'user_instruction', 'acceptance', ?2, ?3, 100, 'active', current_timestamp)
        "#,
        params![
            project_id,
            format!(
                "accepted {} for {}: {}",
                input.acceptance_type, input.target, input.reason
            ),
            input.target,
        ],
    )?;
    let authority_event_id = tx.last_insert_rowid();
    tx.execute(
        r#"
        insert into acceptance_records(
            project_id, target_type, task_id, finding_id, validation_gate_id,
            validation_run_id, repository_state_classification_id,
            repository_snapshot_comparison_id, review_plan_id, checklist_item_id,
            command_profile_id, command_usage_id, command_deviation_id,
            stale_record_type, stale_record_id, acceptance_type, reason, scope,
            created_by, status, approved_by_authority_event_id, approved_at,
            created_at, review_impact
        )
        values (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
            ?16, ?17, ?18, 'user', 'approved', ?19, current_timestamp,
            current_timestamp, 'general acceptance recorded for current workflow'
        )
        "#,
        params![
            project_id,
            target.target_type,
            target.task_id,
            target.finding_id,
            target.validation_gate_id,
            target.validation_run_id,
            target.repository_state_classification_id,
            target.repository_snapshot_comparison_id,
            target.review_plan_id,
            target.checklist_item_id,
            target.command_profile_id,
            target.command_usage_id,
            target.command_deviation_id,
            target.stale_record_type,
            target.stale_record_id,
            input.acceptance_type,
            input.reason,
            input.target,
            authority_event_id,
        ],
    )?;
    let acceptance_record_id = tx.last_insert_rowid();
    tx.commit()?;
    Ok(GeneralAcceptanceOutcome {
        acceptance_record_id,
        authority_event_id,
        target_type: target.target_type.to_string(),
    })
}

fn resolve_general_acceptance_target(
    conn: &rusqlite::Connection,
    project_id: i64,
    target: &str,
) -> Result<ResolvedGeneralAcceptanceTarget> {
    let Some((kind, raw_id)) = target.split_once(':') else {
        bail!("general acceptance target must be kind:<id>");
    };
    if kind == "stale" {
        let Some((stale_type, stale_id)) = raw_id.split_once(':') else {
            bail!("stale acceptance target must be stale:<record-type>:<id>");
        };
        return Ok(ResolvedGeneralAcceptanceTarget {
            target_type: "stale_record",
            stale_record_type: Some(stale_type.to_string()),
            stale_record_id: Some(parse_positive_i64(stale_id, "stale record id")?),
            ..ResolvedGeneralAcceptanceTarget::new("stale_record")
        });
    }
    let id = parse_positive_i64(raw_id, "acceptance target id")?;
    match kind {
        "task" => {
            ensure_project_row(conn, "tasks", "work_units", "work_unit_id", id, project_id)?;
            Ok(ResolvedGeneralAcceptanceTarget {
                task_id: Some(id),
                ..ResolvedGeneralAcceptanceTarget::new("task")
            })
        }
        "finding" => {
            ensure_direct_project_row(conn, "findings", id, project_id)?;
            Ok(ResolvedGeneralAcceptanceTarget {
                finding_id: Some(id),
                ..ResolvedGeneralAcceptanceTarget::new("finding")
            })
        }
        "validation-gate" => {
            ensure_direct_project_row(conn, "validation_gates", id, project_id)?;
            Ok(ResolvedGeneralAcceptanceTarget {
                validation_gate_id: Some(id),
                ..ResolvedGeneralAcceptanceTarget::new("validation_gate")
            })
        }
        "validation-run" => {
            ensure_direct_project_row(conn, "validation_runs", id, project_id)?;
            Ok(ResolvedGeneralAcceptanceTarget {
                validation_run_id: Some(id),
                ..ResolvedGeneralAcceptanceTarget::new("validation_run")
            })
        }
        "repository-state" => Ok(ResolvedGeneralAcceptanceTarget {
            repository_state_classification_id: {
                ensure_repository_state_classification_project(conn, id, project_id)?;
                Some(id)
            },
            ..ResolvedGeneralAcceptanceTarget::new("repository_state_classification")
        }),
        "repository-comparison" => Ok(ResolvedGeneralAcceptanceTarget {
            repository_snapshot_comparison_id: {
                ensure_repository_snapshot_comparison_project(conn, id, project_id)?;
                Some(id)
            },
            ..ResolvedGeneralAcceptanceTarget::new("repository_snapshot_comparison")
        }),
        "review-plan" => {
            ensure_direct_project_row(conn, "review_plans", id, project_id)?;
            Ok(ResolvedGeneralAcceptanceTarget {
                review_plan_id: Some(id),
                ..ResolvedGeneralAcceptanceTarget::new("review_plan")
            })
        }
        "checklist-item" => {
            ensure_checklist_item_project(conn, id, project_id)?;
            Ok(ResolvedGeneralAcceptanceTarget {
                checklist_item_id: Some(id),
                ..ResolvedGeneralAcceptanceTarget::new("checklist_item")
            })
        }
        "command-profile" => {
            ensure_direct_project_row(conn, "command_profiles", id, project_id)?;
            Ok(ResolvedGeneralAcceptanceTarget {
                command_profile_id: Some(id),
                ..ResolvedGeneralAcceptanceTarget::new("command_profile")
            })
        }
        "command-usage" => {
            ensure_direct_project_row(conn, "command_usages", id, project_id)?;
            Ok(ResolvedGeneralAcceptanceTarget {
                command_usage_id: Some(id),
                ..ResolvedGeneralAcceptanceTarget::new("command_usage")
            })
        }
        "command-deviation" => {
            ensure_command_deviation_project(conn, id, project_id)?;
            Ok(ResolvedGeneralAcceptanceTarget {
                command_deviation_id: Some(id),
                ..ResolvedGeneralAcceptanceTarget::new("command_deviation")
            })
        }
        _ => bail!("unsupported general acceptance target kind: {kind}"),
    }
}

fn ensure_direct_project_row(
    conn: &rusqlite::Connection,
    table: &str,
    id: i64,
    project_id: i64,
) -> Result<()> {
    let sql = format!("select 1 from {table} where id = ?1 and project_id = ?2");
    conn.query_row(&sql, params![id, project_id], |_| Ok(()))
        .optional()?
        .with_context(|| format!("{table} row not found for project"))?;
    Ok(())
}

fn ensure_project_row(
    conn: &rusqlite::Connection,
    table: &str,
    project_table: &str,
    project_fk: &str,
    id: i64,
    project_id: i64,
) -> Result<()> {
    let sql = format!(
        "select 1 from {table} child join {project_table} owner on owner.id = child.{project_fk} where child.id = ?1 and owner.project_id = ?2"
    );
    conn.query_row(&sql, params![id, project_id], |_| Ok(()))
        .optional()?
        .with_context(|| format!("{table} row not found for project"))?;
    Ok(())
}

fn ensure_repository_state_classification_project(
    conn: &rusqlite::Connection,
    id: i64,
    project_id: i64,
) -> Result<()> {
    conn.query_row(
        r#"
        select 1
        from repository_state_classifications c
        join repository_snapshots s on s.id = c.repository_snapshot_id
        join repositories r on r.id = s.repository_id
        where c.id = ?1 and r.project_id = ?2
        "#,
        params![id, project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("repository state classification not found for project")?;
    Ok(())
}

fn ensure_repository_snapshot_comparison_project(
    conn: &rusqlite::Connection,
    id: i64,
    project_id: i64,
) -> Result<()> {
    conn.query_row(
        r#"
        select 1
        from repository_snapshot_comparisons c
        join repository_snapshots base on base.id = c.base_repository_snapshot_id
        join repositories base_repo on base_repo.id = base.repository_id
        join repository_snapshots current on current.id = c.current_repository_snapshot_id
        join repositories current_repo on current_repo.id = current.repository_id
        where c.id = ?1
          and base_repo.project_id = ?2
          and current_repo.project_id = ?2
        "#,
        params![id, project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("repository snapshot comparison not found for project")?;
    Ok(())
}

fn ensure_checklist_item_project(
    conn: &rusqlite::Connection,
    id: i64,
    project_id: i64,
) -> Result<()> {
    conn.query_row(
        r#"
        select 1
        from checklist_items item
        join checklists c on c.id = item.checklist_id
        where item.id = ?1 and c.project_id = ?2
        "#,
        params![id, project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("checklist item not found for project")?;
    Ok(())
}

fn ensure_command_deviation_project(
    conn: &rusqlite::Connection,
    id: i64,
    project_id: i64,
) -> Result<()> {
    conn.query_row(
        r#"
        select 1
        from command_deviations d
        join command_profiles p on p.id = d.command_profile_id
        where d.id = ?1 and p.project_id = ?2
        "#,
        params![id, project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("command deviation not found for project")?;
    Ok(())
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
            review_policy_id: None,
            review_plan_id: None,
            work_unit_id: None,
            validation_gate_id: None,
            acceptance_record_id: None,
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
        from design_requirements r
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where r.design_version_id = ?1
          and r.status = 'active'
          and (r.validation_expectation is null or r.validation_expectation = '')
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'design_requirement'
              and ar.design_requirement_id = r.id
              and ar.status = 'approved'
              and ar.acceptance_type in ('explicit_exception', 'evidence_gap')
          )
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'design_requirement_key'
              and ar.design_package_key = p.design_key
              and ar.design_requirement_key = r.requirement_key
              and ar.status = 'approved'
              and ar.acceptance_type in ('explicit_exception', 'evidence_gap')
          )
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

    let review_state = design_review_gate_state(&conn, project_id, version.design_version_id)?;
    if review_state.required_plan_count == 0 {
        items.push(DesignReadyItem::fail(
            "design_review_clean",
            Some("add a required design-ready design_review plan for this design version"),
        ));
    } else if review_state.incomplete_required_plan_count == 0
        && review_state.missing_context_run_count == 0
        && review_state.unresolved_finding_count == 0
    {
        items.push(DesignReadyItem::pass(
            "design_review_clean",
            Some(format!(
                "{} required plans, {} missing review-context runs, {} unresolved findings",
                review_state.required_plan_count,
                review_state.missing_context_run_count,
                review_state.unresolved_finding_count
            )),
        ));
    } else {
        items.push(DesignReadyItem::fail(
            "design_review_clean",
            Some(format!(
                "{} required plans, {} incomplete, {} missing review-context runs, {} unresolved findings",
                review_state.required_plan_count,
                review_state.incomplete_required_plan_count,
                review_state.missing_context_run_count,
                review_state.unresolved_finding_count
            )),
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

fn design_review_gate_state(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_version_id: i64,
) -> Result<ReviewGateState> {
    let required_plan_count = conn.query_row(
        r#"
        select count(*)
        from review_plans
        where project_id = ?1
          and design_version_id = ?2
          and stage = 'design-ready'
          and review_type = 'design_review'
          and required = 1
        "#,
        params![project_id, design_version_id],
        |row| row.get::<_, i64>(0),
    )?;
    let incomplete_required_plan_count = conn.query_row(
        r#"
        select count(*)
        from review_plans
        where project_id = ?1
          and design_version_id = ?2
          and stage = 'design-ready'
          and review_type = 'design_review'
          and required = 1
          and status != 'clean'
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'review_plan'
              and ar.review_plan_id = review_plans.id
              and ar.status = 'approved'
              and ar.acceptance_type in ('explicit_exception', 'stale_accepted')
          )
        "#,
        params![project_id, design_version_id],
        |row| row.get::<_, i64>(0),
    )?;
    let unresolved_finding_count = conn.query_row(
        r#"
        select count(*)
        from findings f
        join review_runs rr on rr.id = f.review_run_id
        join review_plans rp on rp.id = rr.review_plan_id
        where rp.project_id = ?1
          and rp.design_version_id = ?2
          and rp.stage = 'design-ready'
          and rp.review_type = 'design_review'
          and f.status not in ('closed', 'accepted_out_of_scope')
          and f.classification not in ('invalid')
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'finding'
              and ar.finding_id = f.id
              and ar.status = 'approved'
              and ar.acceptance_type in (
                'accepted_out_of_scope', 'explicit_exception', 'classified_failure'
              )
          )
        "#,
        params![project_id, design_version_id],
        |row| row.get::<_, i64>(0),
    )?;
    let missing_context_run_count = required_plans_missing_context_count(
        conn,
        project_id,
        "design-ready",
        "design_review",
        Some(design_version_id),
        None,
        "design-review",
    )?;
    Ok(ReviewGateState {
        required_plan_count,
        incomplete_required_plan_count,
        missing_context_run_count,
        unresolved_finding_count,
    })
}

#[derive(Default)]
struct ReviewGateState {
    required_plan_count: i64,
    incomplete_required_plan_count: i64,
    missing_context_run_count: i64,
    unresolved_finding_count: i64,
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
            v.id, v.design_package_id, v.status, p.design_key, p.current_design_version_id
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
                    v.id, v.design_package_id, v.status, p.design_key, p.current_design_version_id
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
) -> Result<ResolvedDesignAcceptanceTarget> {
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
        return Ok(ResolvedDesignAcceptanceTarget {
            target_type: "design_requirement",
            task_id: None,
            design_requirement_id: Some(id),
            validation_gate_template_id: None,
            coverage_item_id: None,
            design_package_key: None,
            design_file_path: None,
            design_requirement_key: None,
        });
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
        return Ok(ResolvedDesignAcceptanceTarget {
            target_type: "validation_gate_template",
            task_id: None,
            design_requirement_id: None,
            validation_gate_template_id: Some(id),
            coverage_item_id: None,
            design_package_key: None,
            design_file_path: None,
            design_requirement_key: None,
        });
    }
    if let Some(coverage_id) = target.strip_prefix("coverage:") {
        let coverage_id = parse_positive_i64(coverage_id, "coverage target id")?;
        let id = conn
            .query_row(
                r#"
                select c.id
                from coverage_items c
                join design_requirements r on r.id = c.design_requirement_id
                where c.project_id = ?1
                  and r.design_version_id = ?2
                  and c.id = ?3
                "#,
                params![project_id, design_version_id, coverage_id],
                |row| row.get(0),
            )
            .optional()?
            .context("coverage item target not found")?;
        return Ok(ResolvedDesignAcceptanceTarget {
            target_type: "coverage_item",
            task_id: None,
            design_requirement_id: None,
            validation_gate_template_id: None,
            coverage_item_id: Some(id),
            design_package_key: None,
            design_file_path: None,
            design_requirement_key: None,
        });
    }
    unreachable!("target was validated above")
}

fn resolve_pre_import_design_acceptance_target(
    design_package: &str,
    target: &str,
) -> Result<ResolvedDesignAcceptanceTarget> {
    validate_design_id(design_package)?;
    if let Some(relative_path) = target.strip_prefix("file:") {
        if relative_path.is_empty() {
            bail!("acceptance file target path is required");
        }
        validate_relative_manifest_path(relative_path)?;
        return Ok(ResolvedDesignAcceptanceTarget {
            target_type: "design_file",
            task_id: None,
            design_requirement_id: None,
            validation_gate_template_id: None,
            coverage_item_id: None,
            design_package_key: Some(design_package.to_string()),
            design_file_path: Some(relative_path.to_string()),
            design_requirement_key: None,
        });
    }
    if let Some(requirement_key) = target.strip_prefix("requirement:") {
        if !valid_design_key(requirement_key, "REQ") {
            bail!("acceptance requirement target key must match REQ-<positive-number>");
        }
        return Ok(ResolvedDesignAcceptanceTarget {
            target_type: "design_requirement_key",
            task_id: None,
            design_requirement_id: None,
            validation_gate_template_id: None,
            coverage_item_id: None,
            design_package_key: Some(design_package.to_string()),
            design_file_path: None,
            design_requirement_key: Some(requirement_key.to_string()),
        });
    }
    bail!("package-scoped acceptance target must be file:<path> or requirement:<key>");
}

fn validate_design_acceptance_type_target(target: &str) -> Result<()> {
    if target
        .strip_prefix("requirement:")
        .is_some_and(|key| valid_design_key(key, "REQ"))
        || target
            .strip_prefix("gate:")
            .is_some_and(|key| valid_design_key(key, "GATE"))
        || target
            .strip_prefix("coverage:")
            .is_some_and(|id| parse_positive_i64(id, "coverage target id").is_ok())
    {
        return Ok(());
    }
    bail!("acceptance target must be requirement:<key>, gate:<key>, or coverage:<id>");
}

fn validate_design_acceptance_type(acceptance_type: &str) -> Result<()> {
    validate_acceptance_type(acceptance_type)
}

fn validate_acceptance_type(acceptance_type: &str) -> Result<()> {
    match acceptance_type {
        "accepted_out_of_scope"
        | "explicit_exception"
        | "evidence_gap"
        | "classified_failure"
        | "stale_accepted" => Ok(()),
        _ => bail!(
            "acceptance type must be accepted_out_of_scope, explicit_exception, evidence_gap, classified_failure, or stale_accepted"
        ),
    }
}

fn parse_positive_i64(value: &str, label: &str) -> Result<i64> {
    let id: i64 = value
        .parse()
        .with_context(|| format!("{label} must be a positive integer"))?;
    if id <= 0 {
        bail!("{label} must be a positive integer");
    }
    Ok(id)
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

fn validate_arc42_manifest_keys(manifest: &DesignManifest) -> Result<()> {
    let required: BTreeSet<&str> = ARC42_KEYS.iter().copied().collect();
    let actual: BTreeSet<&str> = manifest.arc42.keys().map(String::as_str).collect();
    let missing: Vec<&str> = required.difference(&actual).copied().collect();
    let unknown: Vec<&str> = actual.difference(&required).copied().collect();
    if !missing.is_empty() || !unknown.is_empty() {
        bail!(
            "design manifest arc42 keys must exactly match required sections; missing=[{}] unknown=[{}]",
            missing.join(","),
            unknown.join(",")
        );
    }
    Ok(())
}

fn design_file_exception_exists(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_key: &str,
    relative_path: &str,
) -> Result<bool> {
    let count: i64 = conn.query_row(
        r#"
        select count(*)
        from acceptance_records
        where project_id = ?1
          and target_type = 'design_file'
          and design_package_key = ?2
          and design_file_path = ?3
          and acceptance_type = 'explicit_exception'
          and status = 'approved'
        "#,
        params![project_id, design_key, relative_path],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn design_requirement_key_exception_exists(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_key: &str,
    requirement_key: &str,
) -> Result<bool> {
    let count: i64 = conn.query_row(
        r#"
        select count(*)
        from acceptance_records
        where project_id = ?1
          and target_type = 'design_requirement_key'
          and design_package_key = ?2
          and design_requirement_key = ?3
          and acceptance_type = 'explicit_exception'
          and status = 'approved'
        "#,
        params![project_id, design_key, requirement_key],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn validate_requirement_version_transition(
    conn: &rusqlite::Connection,
    design_package_id: i64,
    requirement: &ExtractedDesignRequirement,
) -> Result<Option<i64>> {
    if !requirement.supersedes_requirement_keys.is_empty() {
        if requirement.supersedes_requirement_keys.len() > 1 {
            bail!("requirement import currently supports one supersedes link");
        }
        let superseded_key = &requirement.supersedes_requirement_keys[0];
        let superseded_id = latest_requirement_id_by_key(conn, design_package_id, superseded_key)?
            .with_context(|| format!("superseded requirement not found: {superseded_key}"))?;
        return Ok(Some(superseded_id));
    }

    let previous: Option<(i64, i64, String)> = conn
        .query_row(
            r#"
            select r.id, r.revision, r.requirement_hash
            from design_requirements r
            join design_versions v on v.id = r.design_version_id
            where v.design_package_id = ?1 and r.requirement_key = ?2
            order by v.version_number desc, r.revision desc, r.id desc
            limit 1
            "#,
            params![design_package_id, requirement.requirement_key],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((previous_id, previous_revision, previous_hash)) = previous else {
        return Ok(None);
    };
    if previous_hash == requirement.requirement_hash {
        return Ok(None);
    }
    if requirement.revision > previous_revision {
        return Ok(Some(previous_id));
    }
    bail!(
        "requirement {} changed without increasing revision",
        requirement.requirement_key
    );
}

fn insert_validation_gate_template_requirements(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_version_id: i64,
    validation_gate_template_id: i64,
    requirement_keys: &Option<String>,
) -> Result<()> {
    let Some(requirement_keys) = requirement_keys else {
        return Ok(());
    };
    for requirement_key in requirement_keys.split(',').filter(|key| !key.is_empty()) {
        let design_requirement_id: i64 = conn
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
            .with_context(|| {
                format!(
                    "validation gate applies_to references unknown requirement: {requirement_key}"
                )
            })?;
        conn.execute(
            r#"
            insert into validation_gate_template_requirements(
                project_id, validation_gate_template_id, design_requirement_id
            )
            values (?1, ?2, ?3)
            "#,
            params![
                project_id,
                validation_gate_template_id,
                design_requirement_id
            ],
        )?;
    }
    Ok(())
}

fn latest_requirement_id_by_key(
    conn: &rusqlite::Connection,
    design_package_id: i64,
    requirement_key: &str,
) -> Result<Option<i64>> {
    conn.query_row(
        r#"
        select r.id
        from design_requirements r
        join design_versions v on v.id = r.design_version_id
        where v.design_package_id = ?1 and r.requirement_key = ?2
        order by v.version_number desc, r.revision desc, r.id desc
        limit 1
        "#,
        params![design_package_id, requirement_key],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

fn stored_design_version_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredDesignVersion> {
    Ok(StoredDesignVersion {
        design_version_id: row.get(0)?,
        design_package_id: row.get(1)?,
        status: row.get(2)?,
        design_key: row.get(3)?,
        current_design_version_id: row.get(4)?,
    })
}

fn extract_design_requirements(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_key: &str,
    content: &str,
    file: &ImportedDesignFile,
) -> Result<Vec<ExtractedDesignRequirement>> {
    reject_legacy_headings(content, file, &["R-"], "REQ-")?;
    reject_invalid_agent_blocks(content, file, "requirement", "REQ")?;
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
        let warning_count = usize::from(body_line_count > 80);
        if body_line_count > 150
            && !design_requirement_key_exception_exists(
                conn,
                project_id,
                design_key,
                &metadata.key,
            )?
        {
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
            supersedes_requirement_keys: metadata.supersedes,
            status: metadata.status,
            warning_count,
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
    for validation_key in &metadata.validation {
        if !valid_design_key(validation_key, "GATE") {
            bail!("requirement validation keys must match GATE-<positive-number>");
        }
    }
    for superseded_key in &metadata.supersedes {
        if !valid_design_key(superseded_key, "REQ") {
            bail!("superseded requirement key must match REQ-<positive-number>");
        }
    }
    Ok(())
}

fn extract_design_decisions(
    content: &str,
    file: &ImportedDesignFile,
) -> Result<Vec<ExtractedDesignDecision>> {
    reject_legacy_headings(content, file, &["D-"], "DEC-")?;
    reject_invalid_agent_blocks(content, file, "decision", "DEC")?;
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
    reject_invalid_agent_blocks(content, file, "validation_gate_template", "GATE")?;
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

fn validate_agent_blocks_for_file(file: &ImportedDesignFile) -> Result<()> {
    let lines: Vec<&str> = file.content.lines().collect();
    reject_agent_workbench_blocks_without_level_two_heading(&lines, file)?;
    for block in agent_workbench_header_blocks(&lines, file)? {
        let metadata: AgentBlockHeaderMetadata = yaml_serde::from_str(&block.metadata_text)
            .with_context(|| {
                format!(
                    "failed to parse agent-workbench metadata for {} in {}",
                    block.source_section, file.relative_path
                )
            })?;
        let (key_prefix, allowed_section) = match metadata.record_type.as_str() {
            "requirement" => ("REQ", "requirements"),
            "decision" => ("DEC", "arc42.decisions"),
            "validation_gate_template" => ("GATE", "validation"),
            _ => bail!(
                "unknown agent-workbench metadata type {} in {}",
                metadata.record_type,
                file.relative_path
            ),
        };
        if !valid_design_key(&metadata.key, key_prefix) {
            bail!(
                "agent-workbench metadata key must match {}-<positive-number>",
                key_prefix
            );
        }
        if !heading_key_matches(&block.source_section, &metadata.key) {
            bail!("agent-workbench heading must start with metadata key");
        }
        if file.section_key != allowed_section {
            bail!(
                "agent-workbench metadata type {} is not allowed in {}",
                metadata.record_type,
                file.relative_path
            );
        }
    }
    Ok(())
}

fn reject_invalid_agent_blocks(
    content: &str,
    file: &ImportedDesignFile,
    expected_type: &str,
    key_prefix: &str,
) -> Result<()> {
    let lines: Vec<&str> = content.lines().collect();
    reject_agent_workbench_blocks_without_level_two_heading(&lines, file)?;
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index];
        if !line.starts_with("## ") {
            index += 1;
            continue;
        }
        let source_section = line.trim_start_matches("## ").trim().to_string();
        let fence_start = index + 1;
        if lines.get(fence_start).map(|line| line.trim()) != Some("```yaml agent-workbench") {
            index += 1;
            continue;
        }
        let mut fence_end = fence_start + 1;
        while fence_end < lines.len() && lines[fence_end].trim() != "```" {
            fence_end += 1;
        }
        if fence_end == lines.len() {
            bail!(
                "agent-workbench block {} in {} has an unterminated yaml block",
                source_section,
                file.relative_path
            );
        }
        let metadata_text = lines[fence_start + 1..fence_end].join("\n");
        let metadata: AgentBlockHeaderMetadata = yaml_serde::from_str(&metadata_text)
            .with_context(|| {
                format!(
                    "failed to parse agent-workbench metadata for {} in {}",
                    source_section, file.relative_path
                )
            })?;
        if metadata.record_type != expected_type {
            bail!(
                "unexpected agent-workbench metadata type {} in {}; expected {}",
                metadata.record_type,
                file.relative_path,
                expected_type
            );
        }
        if !valid_design_key(&metadata.key, key_prefix) {
            bail!(
                "agent-workbench metadata key must match {}-<positive-number>",
                key_prefix
            );
        }
        if !heading_key_matches(&source_section, &metadata.key) {
            bail!("agent-workbench heading must start with metadata key");
        }
        index = fence_end + 1;
    }
    Ok(())
}

fn agent_workbench_header_blocks(
    lines: &[&str],
    file: &ImportedDesignFile,
) -> Result<Vec<ExtractedBlock>> {
    let mut blocks = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index];
        if !line.starts_with("## ") {
            index += 1;
            continue;
        }
        let source_section = line.trim_start_matches("## ").trim().to_string();
        let fence_start = index + 1;
        if lines.get(fence_start).map(|line| line.trim()) != Some("```yaml agent-workbench") {
            index += 1;
            continue;
        }
        let mut fence_end = fence_start + 1;
        while fence_end < lines.len() && lines[fence_end].trim() != "```" {
            fence_end += 1;
        }
        if fence_end == lines.len() {
            bail!(
                "agent-workbench block {} in {} has an unterminated yaml block",
                source_section,
                file.relative_path
            );
        }
        blocks.push(ExtractedBlock {
            source_section,
            metadata_text: lines[fence_start + 1..fence_end].join("\n"),
            body: String::new(),
        });
        index = fence_end + 1;
    }
    Ok(blocks)
}

fn reject_agent_workbench_blocks_without_level_two_heading(
    lines: &[&str],
    file: &ImportedDesignFile,
) -> Result<()> {
    for (index, line) in lines.iter().enumerate() {
        if line.trim() != "```yaml agent-workbench" {
            continue;
        }
        let Some(previous_line) = index
            .checked_sub(1)
            .and_then(|previous| lines.get(previous))
        else {
            bail!(
                "yaml agent-workbench block in {} must follow a level-two heading",
                file.relative_path
            );
        };
        if !previous_line.starts_with("## ") {
            bail!(
                "yaml agent-workbench block in {} must follow a level-two heading",
                file.relative_path
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
    #[serde(default)]
    supersedes: Vec<String>,
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

#[derive(Debug, Deserialize)]
struct AgentBlockHeaderMetadata {
    #[serde(rename = "type")]
    record_type: String,
    key: String,
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
    supersedes_requirement_keys: Vec<String>,
    status: String,
    warning_count: usize,
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
    design_key: String,
    current_design_version_id: Option<i64>,
}

struct ResolvedDesignAcceptanceTarget {
    target_type: &'static str,
    task_id: Option<i64>,
    design_requirement_id: Option<i64>,
    validation_gate_template_id: Option<i64>,
    coverage_item_id: Option<i64>,
    design_package_key: Option<String>,
    design_file_path: Option<String>,
    design_requirement_key: Option<String>,
}

struct ResolvedGeneralAcceptanceTarget {
    target_type: &'static str,
    task_id: Option<i64>,
    finding_id: Option<i64>,
    validation_gate_id: Option<i64>,
    validation_run_id: Option<i64>,
    repository_state_classification_id: Option<i64>,
    repository_snapshot_comparison_id: Option<i64>,
    review_plan_id: Option<i64>,
    checklist_item_id: Option<i64>,
    command_profile_id: Option<i64>,
    command_usage_id: Option<i64>,
    command_deviation_id: Option<i64>,
    stale_record_type: Option<String>,
    stale_record_id: Option<i64>,
}

impl ResolvedGeneralAcceptanceTarget {
    fn new(target_type: &'static str) -> Self {
        Self {
            target_type,
            task_id: None,
            finding_id: None,
            validation_gate_id: None,
            validation_run_id: None,
            repository_state_classification_id: None,
            repository_snapshot_comparison_id: None,
            review_plan_id: None,
            checklist_item_id: None,
            command_profile_id: None,
            command_usage_id: None,
            command_deviation_id: None,
            stale_record_type: None,
            stale_record_id: None,
        }
    }
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
    pub design_version_id: Option<i64>,
    pub design_package: Option<&'a str>,
    pub target: &'a str,
    pub acceptance_type: &'a str,
    pub reason: &'a str,
}

pub struct NewGeneralAcceptance<'a> {
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
    pub warning_count: usize,
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
    pub coverage_item_id: Option<i64>,
    pub design_package_key: Option<String>,
    pub design_file_path: Option<String>,
    pub design_requirement_key: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct GeneralAcceptanceOutcome {
    pub acceptance_record_id: i64,
    pub authority_event_id: i64,
    pub target_type: String,
}

impl DesignReadyItem {
    fn pass(name: &str, detail: Option<String>) -> Self {
        Self {
            name: name.to_string(),
            result: "pass".to_string(),
            detail,
        }
    }

    fn fail<S: Into<String>>(name: &str, detail: Option<S>) -> Self {
        Self {
            name: name.to_string(),
            result: "fail".to_string(),
            detail: detail.map(Into::into),
        }
    }
}
