use super::super::*;

#[allow(clippy::too_many_arguments)]
pub(in crate::decomposition) fn insert_lifecycle_plan(
    conn: &Connection,
    project_id: i64,
    design_version_id: i64,
    work_unit_id: i64,
    parsed: &ParsedPlan,
    ingress_source_identity: &str,
    revision: i64,
    predecessor_id: Option<i64>,
    status: &str,
    issue: Option<&str>,
) -> Result<i64> {
    let document = parsed
        .document
        .as_ref()
        .context("Decomposition Plan metadata is required")?;
    if validate_plan(document).is_ok() {
        let predecessor_key = predecessor_id
            .map(|predecessor_id| {
                conn.query_row(
                    "select plan_key from decomposition_plans where id=?1",
                    [predecessor_id],
                    |row| row.get::<_, String>(0),
                )
                .map(|key| (predecessor_id, key))
            })
            .transpose()?;
        if let Some((predecessor_id, _)) = predecessor_key.as_ref() {
            conn.execute(
                "update decomposition_plans set plan_key=?1 where id=?2",
                params![format!("__superseded__{predecessor_id}"), predecessor_id],
            )?;
        }
        install_discovered_plans(conn, std::slice::from_ref(parsed))?;
        let plan_id = plan_by_source_identity(conn, project_id, &parsed.source_identity)?
            .context("installed Decomposition Plan is unavailable")?;
        conn.execute(
            r#"
            update decomposition_plans
            set revision=?1,predecessor_id=?2,status=?3,binding_issue=?4
            where id=?5
            "#,
            params![revision, predecessor_id, status, issue, plan_id],
        )?;
        record_ingress_identity(
            conn,
            project_id,
            plan_id,
            ingress_source_identity,
            &parsed.content_identity,
        )?;
        if let Some((predecessor_id, predecessor_key)) = predecessor_key {
            conn.execute(
                "update decomposition_plans set plan_key=?1 where id=?2",
                params![predecessor_key, predecessor_id],
            )?;
        }
        return Ok(plan_id);
    }
    conn.execute(
        r#"
        insert into decomposition_plans(
          project_id,work_unit_id,design_version_id,design_package_id,plan_key,revision,source_path,
          source_identity,document_content,content_identity,source_kind,design_fingerprint,status,binding_issue,predecessor_id,
          created_at
        ) values(?1,?2,?3,(select design_package_id from design_versions where id=?3),?4,?5,?6,?7,?8,?9,'document',?10,?11,?12,?13,current_timestamp)
        "#,
        params![
            project_id,
            work_unit_id,
            design_version_id,
            document.key,
            revision,
            stored_source_path(parsed),
            parsed.source_identity,
            parsed.content,
            parsed.content_identity,
            document.design_fingerprint,
            status,
            issue,
            predecessor_id,
        ],
    )?;
    let plan_id = conn.last_insert_rowid();
    record_ingress_identity(
        conn,
        project_id,
        plan_id,
        ingress_source_identity,
        &parsed.content_identity,
    )?;
    Ok(plan_id)
}

fn record_ingress_identity(
    conn: &Connection,
    project_id: i64,
    plan_id: i64,
    source_identity: &str,
    content_identity: &str,
) -> Result<()> {
    conn.execute(
        r#"
        insert or ignore into decomposition_plan_ingress_identities(
          plan_id,project_id,source_identity,content_identity,created_at
        ) values(?1,?2,?3,?4,current_timestamp)
        "#,
        params![plan_id, project_id, source_identity, content_identity],
    )?;
    let exact: bool = conn.query_row(
        "select source_identity=?1 and content_identity=?2 from decomposition_plan_ingress_identities where plan_id=?3 and project_id=?4",
        params![source_identity, content_identity, plan_id, project_id],
        |row| row.get(0),
    )?;
    if !exact {
        bail!("Decomposition Plan ingress identity belongs to different staged bytes");
    }
    Ok(())
}
