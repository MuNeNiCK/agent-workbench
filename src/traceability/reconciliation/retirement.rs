use anyhow::Result;
use rusqlite::params;

pub(super) fn retire_historical_decompositions_in(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_version_id: i64,
    work_unit_id: i64,
) -> Result<()> {
    let historical_tasks = r#"
        select distinct td.task_id
        from task_derivations td
        join design_requirements r on r.id=td.design_requirement_id
        join design_versions v on v.id=r.design_version_id
        join design_versions current_v on current_v.id=?2
        join tasks t on t.id=td.task_id
        where td.project_id=?1 and t.work_unit_id=?3
          and v.design_package_id=current_v.design_package_id
          and v.version_number<current_v.version_number
          and exists (
            select 1
            from design_requirements current_r
            join task_derivations current_td on current_td.design_requirement_id=current_r.id
            join tasks current_t on current_t.id=current_td.task_id
            join work_phase_task_memberships current_m on current_m.task_id=current_t.id
            where current_r.design_version_id=?2
              and current_r.requirement_key=r.requirement_key
              and current_td.status='active'
              and current_t.work_unit_id=?3
          )
    "#;
    conn.execute(
        &format!(
            "update validation_gates set status='closed' where project_id=?1 and status='active' and task_id in ({historical_tasks})"
        ),
        params![project_id, design_version_id, work_unit_id],
    )?;
    conn.execute(
        &format!(
            "update coverage_items set status='stale' where project_id=?1 and status!='stale' and task_id in ({historical_tasks})"
        ),
        params![project_id, design_version_id, work_unit_id],
    )?;
    conn.execute(
        r#"
        update task_derivations set status='closed'
        where project_id=?1 and status in ('active','stale')
          and id in (
            select td.id
            from task_derivations td
            join design_requirements r on r.id=td.design_requirement_id
            join design_versions v on v.id=r.design_version_id
            join design_versions current_v on current_v.id=?2
            join tasks t on t.id=td.task_id
            where td.project_id=?1 and t.work_unit_id=?3
              and v.design_package_id=current_v.design_package_id
              and v.version_number<current_v.version_number
              and exists (
                select 1
                from design_requirements current_r
                join task_derivations current_td on current_td.design_requirement_id=current_r.id
                join tasks current_t on current_t.id=current_td.task_id
                join work_phase_task_memberships current_m on current_m.task_id=current_t.id
                where current_r.design_version_id=?2
                  and current_r.requirement_key=r.requirement_key
                  and current_td.status='active'
                  and current_t.work_unit_id=?3
              )
          )
        "#,
        params![project_id, design_version_id, work_unit_id],
    )?;
    Ok(())
}
