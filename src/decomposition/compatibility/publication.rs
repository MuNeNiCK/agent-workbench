use super::*;

pub(super) fn write_derived_bundle_source(
    conn: &Connection,
    project_id: i64,
    work_unit_id: i64,
    design_version_id: i64,
    design_fingerprint: &str,
) -> Result<ParsedPlan> {
    let package_root: String = conn.query_row(
        r#"
        select package.root_path
        from design_versions version
        join design_packages package on package.id=version.design_package_id
        where version.id=?1 and version.project_id=?2
        "#,
        params![design_version_id, project_id],
        |row| row.get(0),
    )?;
    let package_root = PathBuf::from(package_root);
    let project_root = package_root
        .ancestors()
        .nth(3)
        .context("Design Package root is outside the managed project layout")?;
    let requirements = string_column(
        conn,
        "select requirement_key from design_requirements where project_id=?1 and design_version_id=?2 and status='active' order by requirement_key",
        params![project_id, design_version_id],
    )?;
    let document = PlanDocument {
        record_type: "decomposition_plan".to_string(),
        format: 1,
        key: format!("migrated-work-{work_unit_id}"),
        design_fingerprint: design_fingerprint.to_string(),
        work: Some(work_unit_id),
        items: vec![PlanItem {
            key: "complete-migrated-mapping".to_string(),
            requirements,
            title: "Complete the migrated decomposition mapping".to_string(),
            details: "Replace this migration worksheet with explicit observable boundaries and endpoint mappings.".to_string(),
            completion: PlanCompletion {
                outcome: String::new(),
                observation: String::new(),
                evidence_owner: format!("work:{work_unit_id}"),
                evidence_kind: String::new(),
                gates: Vec::new(),
            },
            checklist: Vec::new(),
            slice: "migration".to_string(),
        }],
        slices: vec![PlanSlice {
            key: "migration".to_string(),
            title: "Migration reconciliation".to_string(),
            order: 1,
            depends_on: Vec::new(),
        }],
        reconciliation: None,
    };
    let yaml = yaml_serde::to_string(&document)
        .context("failed to serialize the migrated Decomposition Plan")?;
    let content = format!(
        "# Migrated Decomposition Plan\n\nComplete this project-owned worksheet, then use the exact `decomposition revise` or closure-bound `decomposition reconcile` action.\n\n```yaml agent-workbench\n{yaml}```\n"
    );
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let content_identity = format!("{:x}", hasher.finalize());
    let plans_dir = package_root.join("plans");
    fs::create_dir_all(&plans_dir)
        .with_context(|| format!("failed to create {}", plans_dir.display()))?;
    let path = plans_dir.join(format!(
        "migrated-work-{work_unit_id}-{}.md",
        &content_identity[..12]
    ));
    if path.exists() {
        let existing = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if existing != content {
            bail!("migrated Decomposition Plan path has different content");
        }
    } else {
        let temporary = plans_dir.join(format!(
            ".migrated-work-{work_unit_id}-{}.tmp",
            &content_identity[..12]
        ));
        fs::write(&temporary, &content)
            .with_context(|| format!("failed to write {}", temporary.display()))?;
        fs::rename(&temporary, &path)
            .with_context(|| format!("failed to publish {}", path.display()))?;
    }
    parse_plan_unvalidated(project_root, &path)
}
