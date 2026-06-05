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
struct DesignManifest {
    id: String,
    title: String,
    format: String,
    version: i64,
    arc42: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    requirements: Vec<String>,
    #[serde(default)]
    validation: Vec<String>,
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
