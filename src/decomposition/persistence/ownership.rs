use super::super::*;

pub(in crate::decomposition) fn resolve_design_version(
    conn: &Connection,
    project_id: i64,
    fingerprint: &str,
) -> Result<i64> {
    let mut statement = conn.prepare(
        "select id from design_versions where project_id=?1 and (content_hash=?2 or package_hash=?2) order by id",
    )?;
    let matches = statement
        .query_map(params![project_id, fingerprint], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    match matches.as_slice() {
        [design] => Ok(*design),
        [] => bail!("decomposition plan design identity is unavailable"),
        _ => bail!("decomposition plan design identity is ambiguous"),
    }
}

pub(in crate::decomposition) fn resolve_plan_design(
    conn: &Connection,
    project_id: i64,
    plan: &ParsedPlan,
) -> Result<i64> {
    if let Some(document) = &plan.document {
        return resolve_design_version(conn, project_id, &document.design_fingerprint);
    }
    let root = plan.design_root.to_string_lossy();
    conn.query_row(
        r#"
        select coalesce(package.current_design_version_id,
                        (select max(candidate.id) from design_versions candidate
                         where candidate.design_package_id=package.id))
        from design_packages package
        where package.project_id=?1 and package.root_path=?2
        "#,
        params![project_id, root.as_ref()],
        |row| row.get::<_, Option<i64>>(0),
    )
    .optional()?
    .flatten()
    .context("legacy decomposition document has no owning design version")
}

pub(in crate::decomposition) fn design_identity(
    conn: &Connection,
    project_id: i64,
    design_version_id: i64,
) -> Result<String> {
    conn.query_row(
        "select content_hash from design_versions where id=?1 and project_id=?2",
        params![design_version_id, project_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(in crate::decomposition) fn resolve_work_binding(
    conn: &Connection,
    project_id: i64,
    _design_version_id: i64,
    document: &PlanDocument,
) -> Result<Option<i64>> {
    if let Some(work_id) = document.work {
        if work_id <= 0 {
            return Ok(None);
        }
        return work_binding_exists(conn, project_id, work_id);
    }
    let owners = document
        .items
        .iter()
        .map(|item| item.completion.evidence_owner.as_str())
        .collect::<BTreeSet<_>>();
    let owners = owners.into_iter().collect::<Vec<_>>();
    let [owner] = owners.as_slice() else {
        return Ok(None);
    };
    let Some(work_id) = owner
        .strip_prefix("work:")
        .and_then(|value| value.parse::<i64>().ok())
    else {
        return Ok(None);
    };
    work_binding_exists(conn, project_id, work_id)
}

pub(in crate::decomposition) fn work_binding_exists(
    conn: &Connection,
    project_id: i64,
    work_id: i64,
) -> Result<Option<i64>> {
    let exists: bool = conn.query_row(
        r#"
        select exists(
          select 1 from work_units work
          where work.id=?1 and work.project_id=?2 and work.status in ('open','blocked')
        )
        "#,
        params![work_id, project_id],
        |row| row.get(0),
    )?;
    Ok(exists.then_some(work_id))
}
