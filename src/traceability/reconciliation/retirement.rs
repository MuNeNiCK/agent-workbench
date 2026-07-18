use anyhow::Result;
use rusqlite::params;

pub(super) fn retire_historical_decompositions_in(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_version_id: i64,
    work_unit_id: i64,
) -> Result<()> {
    let historical_requirements = r#"
        select distinct r.id
        from design_requirements r
        join design_versions v on v.id=r.design_version_id
        join design_versions current_v on current_v.id=?2
        where r.project_id=?1
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
              and current_r.revision=r.revision
              and current_r.requirement_hash=r.requirement_hash
              and current_r.required_surfaces is r.required_surfaces
              and not exists (
                select 1
                from design_requirements intervening_r
                join design_versions intervening_v on intervening_v.id=intervening_r.design_version_id
                where intervening_v.design_package_id=v.design_package_id
                  and intervening_v.version_number>v.version_number
                  and intervening_v.version_number<current_v.version_number
                  and intervening_r.requirement_key=r.requirement_key
                  and (intervening_r.revision!=current_r.revision
                    or intervening_r.requirement_hash!=current_r.requirement_hash
                    or intervening_r.required_surfaces is not current_r.required_surfaces)
              )
              and current_td.status='active'
              and current_t.work_unit_id=?3
          )
    "#;
    conn.execute(
        &format!(
            "update validation_gates set status='closed' where project_id=?1 and status='active' and design_requirement_id in ({historical_requirements})"
        ),
        params![project_id, design_version_id, work_unit_id],
    )?;
    conn.execute(
        &format!(
            "update coverage_items set status='stale' where project_id=?1 and status!='stale' and design_requirement_id in ({historical_requirements})"
        ),
        params![project_id, design_version_id, work_unit_id],
    )?;
    conn.execute(
        &format!(
            r#"
        update task_derivations set status='closed'
        where project_id=?1 and status in ('active','stale')
          and design_requirement_id in ({historical_requirements})
        "#
        ),
        params![project_id, design_version_id, work_unit_id],
    )?;
    conn.execute(
        &format!(
            r#"
        update checklist_items set status='closed'
        where project_id=?1 and status in ('open','blocked')
          and design_requirement_id in ({historical_requirements})
        "#
        ),
        params![project_id, design_version_id, work_unit_id],
    )?;
    conn.execute(
        &format!(
            r#"
        update checklists set status='closed'
        where project_id=?1 and work_unit_id=?3 and status in ('active','stale')
          and design_version_id in (
            select historical_v.id
            from design_versions historical_v
            join design_versions current_v on current_v.id=?2
            where historical_v.design_package_id=current_v.design_package_id
              and historical_v.version_number<current_v.version_number
          )
          and exists (
            select 1 from checklist_items retired_ci
            where retired_ci.checklist_id=checklists.id
              and retired_ci.design_requirement_id in ({historical_requirements})
          )
          and not exists (
            select 1 from checklist_items ci
            where ci.checklist_id=checklists.id and ci.status in ('open','blocked')
          )
        "#
        ),
        params![project_id, design_version_id, work_unit_id],
    )?;
    Ok(())
}
