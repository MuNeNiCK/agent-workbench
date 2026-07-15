use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use super::{package::*, *};

pub(super) fn validate_design_id(design_id: &str) -> Result<()> {
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

pub(super) fn validate_import_status(status: &str) -> Result<()> {
    match status {
        "draft" | "reviewed" => Ok(()),
        _ => bail!("design import status must be draft or reviewed"),
    }
}

pub(super) fn resolve_package_path(root: &Path, package_path: &Path) -> Result<PathBuf> {
    let path = if package_path.is_absolute() {
        package_path.to_path_buf()
    } else {
        root.join(package_path)
    };
    path.canonicalize()
        .with_context(|| format!("failed to resolve {}", path.display()))
}

pub(super) fn ensure_package_path_is_under_design_root(
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

pub(super) fn validate_relative_manifest_path(path: &str) -> Result<()> {
    let path = Path::new(path);
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        bail!("design manifest paths must be relative package paths");
    }
    if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
        bail!("design manifest paths must name Markdown files ending in .md");
    }
    Ok(())
}

pub(super) fn stored_design_version(
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

pub(super) fn resolve_design_version_for_gate(
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

pub(super) fn resolve_design_acceptance_target(
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

pub(super) fn resolve_pre_import_design_acceptance_target(
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
        validate_opaque_key(requirement_key, "acceptance requirement target")?;
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

pub(super) fn validate_design_acceptance_type_target(target: &str) -> Result<()> {
    if target
        .strip_prefix("requirement:")
        .is_some_and(valid_opaque_key)
        || target.strip_prefix("gate:").is_some_and(valid_opaque_key)
        || target
            .strip_prefix("coverage:")
            .is_some_and(|id| parse_positive_i64(id, "coverage target id").is_ok())
    {
        return Ok(());
    }
    bail!("acceptance target must be requirement:<key>, gate:<key>, or coverage:<id>");
}

pub(super) fn validate_design_acceptance_type(acceptance_type: &str) -> Result<()> {
    validate_acceptance_type(acceptance_type)
}

pub(super) fn validate_acceptance_type(acceptance_type: &str) -> Result<()> {
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

pub(super) fn parse_positive_i64(value: &str, label: &str) -> Result<i64> {
    let id: i64 = value
        .parse()
        .with_context(|| format!("{label} must be a positive integer"))?;
    if id <= 0 {
        bail!("{label} must be a positive integer");
    }
    Ok(id)
}

pub(super) fn validate_opaque_key(value: &str, label: &str) -> Result<()> {
    if !valid_opaque_key(value) {
        bail!("{label} key must not be empty");
    }
    Ok(())
}

fn valid_opaque_key(value: &str) -> bool {
    !value.trim().is_empty()
}

pub(super) fn title_from_section(source_section: &str) -> String {
    source_section.trim().to_string()
}

pub(super) fn normalize_validation_phase(phase: &str) -> Result<String> {
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

pub(super) fn validate_expected_result(expected_result: &str) -> Result<()> {
    match expected_result {
        "pass" | "blocked" | "needs_evidence" | "fail" => Ok(()),
        _ => bail!("invalid validation gate expected_result: {expected_result}"),
    }
}

pub(super) fn validate_arc42_manifest_keys(manifest: &DesignManifest) -> Result<()> {
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

pub(super) fn design_file_exception_exists(
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

pub(super) fn design_requirement_key_exception_exists(
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

pub(super) fn validate_requirement_version_transition(
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

pub(super) fn insert_validation_gate_template_requirements(
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

pub(super) fn latest_requirement_id_by_key(
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

pub(super) fn stored_design_version_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredDesignVersion> {
    Ok(StoredDesignVersion {
        design_version_id: row.get(0)?,
        design_package_id: row.get(1)?,
        status: row.get(2)?,
        design_key: row.get(3)?,
        current_design_version_id: row.get(4)?,
    })
}
