pub(in crate::decomposition) struct LifecycleSourceIdentity<'a> {
    pub(in crate::decomposition) operation: &'a str,
    pub(in crate::decomposition) idempotency_key: &'a str,
    pub(in crate::decomposition) expected_current: &'a str,
    pub(in crate::decomposition) predecessor_id: i64,
    pub(in crate::decomposition) revision: i64,
    pub(in crate::decomposition) draft: bool,
    pub(in crate::decomposition) source_path: &'a Path,
    pub(in crate::decomposition) source_identity: &'a str,
}

pub(in crate::decomposition) fn lifecycle_source_identity(
    input: LifecycleSourceIdentity<'_>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/decomposition-lifecycle-source/v1\0");
    hasher.update(input.operation.as_bytes());
    hasher.update(b"\0");
    hasher.update(input.idempotency_key.as_bytes());
    hasher.update(b"\0");
    hasher.update(input.expected_current.as_bytes());
    hasher.update(b"\0");
    hasher.update([u8::from(input.draft)]);
    hasher.update(input.predecessor_id.to_be_bytes());
    hasher.update(input.revision.to_be_bytes());
    hasher.update(input.source_path.as_os_str().as_encoded_bytes());
    hasher.update(b"\0");
    hasher.update(input.source_identity.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(in crate::decomposition) fn validate_document_binding(
    conn: &Connection,
    project_id: i64,
    design_version_id: i64,
    work_unit_id: i64,
    parsed: &ParsedPlan,
) -> Result<()> {
    let document = parsed
        .document
        .as_ref()
        .context("Decomposition Plan metadata is required")?;
    let resolved_work = resolve_work_binding(conn, project_id, design_version_id, document)?;
    if resolve_design_version(conn, project_id, &document.design_fingerprint)? != design_version_id
        || resolved_work.is_some_and(|resolved| resolved != work_unit_id)
        || (resolved_work.is_none() && !document.items.is_empty())
    {
        bail!("Decomposition Plan does not identify the selected design and work owner");
    }
    validate_plan_package_root(conn, project_id, design_version_id, &parsed.design_root)
}

pub(in crate::decomposition) fn empty_draft_plan(
    conn: &Connection,
    project_id: i64,
    design_version_id: i64,
    work_unit_id: i64,
) -> Result<ParsedPlan> {
    let (package_root, design_fingerprint): (String, String) = conn.query_row(
        r#"
        select package.root_path,version.content_hash
        from design_versions version
        join design_packages package on package.id=version.design_package_id
        where version.id=?1 and version.project_id=?2
        "#,
        params![design_version_id, project_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let document = PlanDocument {
        record_type: "decomposition_plan".to_string(),
        format: 1,
        key: format!("work-{work_unit_id}-plan"),
        design_fingerprint,
        work: Some(work_unit_id),
        items: Vec::new(),
        slices: Vec::new(),
        reconciliation: None,
    };
    let content = canonical_plan_content(&document)?;
    parse_owned_plan_content(content, PathBuf::new(), PathBuf::from(package_root))
}

pub(in crate::decomposition) fn parsed_owned_predecessor(
    conn: &Connection,
    project_id: i64,
    predecessor: &DecompositionPlanRecord,
) -> Result<ParsedPlan> {
    let package_root: String = conn.query_row(
        r#"
        select package.root_path
        from design_versions version
        join design_packages package on package.id=version.design_package_id
        where version.id=?1 and version.project_id=?2
        "#,
        params![predecessor.design_version_id, project_id],
        |row| row.get(0),
    )?;
    parse_owned_plan_content(
        predecessor.document_content.clone(),
        predecessor
            .source_path
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_default(),
        PathBuf::from(package_root),
    )
}

use super::super::*;
