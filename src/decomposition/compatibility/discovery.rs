use super::*;

pub(super) fn uncovered_derived_bundles(
    conn: &Connection,
    plans: &[ParsedPlan],
) -> Result<Vec<(i64, i64)>> {
    let project_id = crate::db::project_id(conn)?;
    let mut covered = BTreeSet::new();
    for plan in plans {
        let Some(document) = plan.document.as_ref() else {
            continue;
        };
        let design = resolve_design_version(conn, project_id, &document.design_fingerprint)?;
        if let Some(work) = resolve_work_binding(conn, project_id, design, document)? {
            covered.insert((work, design));
        }
    }
    let mut statement = conn.prepare(
        r#"
        select distinct task.work_unit_id,requirement.design_version_id
        from task_derivations derivation
        join tasks task on task.id=derivation.task_id
        join design_requirements requirement on requirement.id=derivation.design_requirement_id
        join work_units work on work.id=task.work_unit_id
        where work.project_id=?1
        order by task.work_unit_id,requirement.design_version_id
        "#,
    )?;
    let bundles = statement
        .query_map([project_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|bundle| !covered.contains(bundle))
        .collect();
    Ok(bundles)
}

pub(super) fn source_phase_ids(
    conn: &Connection,
    task_id: i64,
    available: &BTreeMap<i64, i64>,
) -> Result<Vec<i64>> {
    let mut statement = conn.prepare(
        "select distinct phase_id from work_phase_task_memberships where task_id=?1 order by phase_id",
    )?;
    Ok(statement
        .query_map([task_id], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|phase| available.contains_key(phase))
        .collect())
}

pub(super) fn source_checklist_items(
    conn: &Connection,
    task_id: i64,
) -> Result<Vec<(i64, String, String)>> {
    let mut statement = conn.prepare(
        "select id,title,coalesce(completion_condition,title) from checklist_items where task_id=?1 and status!='accepted_out_of_scope' order by id",
    )?;
    statement
        .query_map([task_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}
