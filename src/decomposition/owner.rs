use super::*;

pub(crate) fn resolve_decomposition_owner(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<Option<DecompositionOwnerResolution>> {
    let project_id = project_id(conn)?;
    let mut statement = conn.prepare(
        r#"
        select distinct package.current_design_version_id
        from decomposition_plans plan
        join design_versions version on version.id=plan.design_version_id
        join design_packages package on package.id=version.design_package_id
        where plan.project_id=?1 and plan.work_unit_id=?2 and plan.status!='superseded'
          and package.current_design_version_id is not null
        order by package.current_design_version_id
        "#,
    )?;
    let designs = statement
        .query_map(params![project_id, work_unit_id], |row| {
            row.get::<_, i64>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let design_version_id = match designs.as_slice() {
        [] => return Ok(None),
        [design] => *design,
        _ => bail!("work has current Decomposition Plans in multiple package lineages"),
    };
    let Some(plan_id) = resolve_current_plan_id(conn, project_id, design_version_id, work_unit_id)?
    else {
        return Ok(None);
    };
    let plan = load_decomposition_plan(conn, plan_id)?;
    let query = DecompositionPlanQuery {
        design_version_id,
        work_unit_id,
    };
    let package_root: String = conn.query_row(
        r#"
        select package.root_path from design_versions version
        join design_packages package on package.id=version.design_package_id
        where version.id=?1
        "#,
        [design_version_id],
        |row| row.get(0),
    )?;
    let project_root = Path::new(&package_root)
        .ancestors()
        .nth(3)
        .context("Design Package root is outside the managed project layout")?;
    let candidates = resolve_plan_candidates(conn, project_root, project_id, &query)?;
    let successor = (plan.status == "applied")
        .then(|| load_ready_successor(conn, project_id, plan.id, design_version_id))
        .transpose()?
        .flatten();
    let review_plan = successor.as_ref().unwrap_or(&plan);
    let review_owner = matches!(review_plan.status.as_str(), "ready" | "applied")
        .then(|| resolve_plan_review_owner(conn, project_id, review_plan, design_version_id))
        .transpose()?;
    let resolution = decomposition_actions(
        conn,
        &query,
        Some(&plan),
        successor.as_ref(),
        &candidates,
        review_owner.as_ref(),
    )?;
    if resolution.actions.is_empty() {
        bail!("Decomposition Plan resolver returned no legal action");
    }
    Ok(Some(DecompositionOwnerResolution {
        plan_id,
        status: plan.status,
        issue: plan.issue,
        actions: resolution.actions,
        blocks_work: resolution.blocks_work,
    }))
}
