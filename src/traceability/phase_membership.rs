use anyhow::{Result, bail};
use rusqlite::{OptionalExtension, params};

pub(super) struct MembershipIdentity<'a> {
    pub project_id: i64,
    pub current_requirement_id: i64,
    pub requirement_key: &'a str,
    pub work_unit_id: i64,
    pub canonical_task_id: i64,
}

pub(super) fn collapse_existing_canonical_membership(
    conn: &rusqlite::Connection,
    identity: &MembershipIdentity<'_>,
) -> Result<bool> {
    let membership_count: i64 = conn.query_row(
        "select count(*) from work_phase_task_memberships where task_id=?1",
        params![identity.canonical_task_id],
        |row| row.get(0),
    )?;
    if membership_count == 0 {
        return Ok(false);
    }
    if membership_count != 1 {
        bail!(
            "canonical phase membership is ambiguous for {}",
            identity.requirement_key
        );
    }
    let phase = conn
        .query_row(
            r#"select p.id,p.status,p.design_version_id,
                      (select design_version_id from design_requirements where id=?4)
               from work_phase_task_memberships m
               join work_phases p on p.id=m.phase_id
               where m.task_id=?1 and m.project_id=?2 and p.project_id=?2
                 and p.work_unit_id=?3"#,
            params![
                identity.canonical_task_id,
                identity.project_id,
                identity.work_unit_id,
                identity.current_requirement_id
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, i64>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((phase_id, phase_status, phase_design_version_id, current_design_version_id)) = phase
    else {
        bail!(
            "canonical task is already phase-assigned for {}",
            identity.requirement_key
        );
    };
    if phase_status == "closed" {
        replace_historical_memberships(conn, identity, phase_id)?;
    } else if phase_design_version_id == Some(current_design_version_id) {
        bail!(
            "canonical task is already phase-assigned for {}",
            identity.requirement_key
        );
    }
    Ok(true)
}

pub(super) fn migrate_incompatible_membership(
    conn: &rusqlite::Connection,
    baseline_design_id: i64,
    identity: &MembershipIdentity<'_>,
) -> Result<()> {
    let mut statement = conn.prepare(
        r#"select distinct p.id from design_requirements r
           join task_derivations td on td.design_requirement_id=r.id join tasks t on t.id=td.task_id
           join checklist_items ci on ci.id=td.checklist_item_id
           join work_phase_task_memberships m on m.task_id=t.id join work_phases p on p.id=m.phase_id
           where r.design_version_id=?1 and r.requirement_key=?2 and t.work_unit_id=?3
             and t.status='closed' and td.status in ('active','closed','stale') and ci.status='closed'
             and p.project_id=?4 and p.work_unit_id=?3 and p.status='closed'
             and p.authority_event_id is null and p.closed_at is not null and m.assigned_at<=p.closed_at
             and (select count(*) from task_derivations x where x.design_requirement_id=r.id)=1
             and (select count(*) from checklist_items x where x.id=td.checklist_item_id and x.task_id=t.id
               and x.design_requirement_id=r.id and x.status='closed')=1
             and (select count(*) from work_phase_task_memberships x where x.task_id=t.id)=1
             and (select count(*) from work_phase_events e where e.phase_id=p.id and e.event_type='closed')=1
             and exists(select 1 from work_phase_events e where e.phase_id=p.id and e.event_type='closed'
               and e.created_at=p.closed_at)"#,
    )?;
    let phases = statement
        .query_map(
            params![
                baseline_design_id,
                identity.requirement_key,
                identity.work_unit_id,
                identity.project_id
            ],
            |row| row.get::<_, i64>(0),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if phases.len() != 1 {
        bail!(
            "incompatible revision phase membership is missing or ambiguous for {}",
            identity.requirement_key
        );
    }
    replace_historical_memberships(conn, identity, phases[0])
}

pub(super) fn replace_historical_memberships(
    conn: &rusqlite::Connection,
    identity: &MembershipIdentity<'_>,
    phase_id: i64,
) -> Result<()> {
    conn.execute(
        r#"delete from work_phase_task_memberships
           where project_id=?1 and phase_id=?2 and task_id!=?3 and task_id in (
             select distinct td.task_id from task_derivations td
             join design_requirements r on r.id=td.design_requirement_id
             join design_versions v on v.id=r.design_version_id
             join tasks t on t.id=td.task_id
             where r.requirement_key=?4 and v.design_package_id=(
               select dv.design_package_id from design_requirements current_r
               join design_versions dv on dv.id=current_r.design_version_id where current_r.id=?5)
               and v.version_number<(select current_v.version_number from design_requirements current_r
                 join design_versions current_v on current_v.id=current_r.design_version_id where current_r.id=?5)
               and t.work_unit_id=?6 and t.status in ('closed','accepted_out_of_scope'))"#,
        params![
            identity.project_id,
            phase_id,
            identity.canonical_task_id,
            identity.requirement_key,
            identity.current_requirement_id,
            identity.work_unit_id
        ],
    )?;
    let canonical_count: i64 = conn.query_row(
        "select count(*) from work_phase_task_memberships where task_id=?1",
        params![identity.canonical_task_id],
        |row| row.get(0),
    )?;
    match canonical_count {
        0 => {
            conn.execute(
                "insert into work_phase_task_memberships(project_id,phase_id,task_id,assigned_at) values(?1,?2,?3,current_timestamp)",
                params![identity.project_id, phase_id, identity.canonical_task_id],
            )?;
        }
        1 => {}
        _ => bail!(
            "canonical phase membership is ambiguous for {}",
            identity.requirement_key
        ),
    }
    Ok(())
}
