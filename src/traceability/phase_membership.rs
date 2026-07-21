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
            r#"select p.id from work_phase_task_memberships m
               join work_phases p on p.id=m.phase_id
               where m.task_id=?1 and m.project_id=?2 and p.project_id=?2
                 and p.work_unit_id=?3 and p.status='closed'"#,
            params![
                identity.canonical_task_id,
                identity.project_id,
                identity.work_unit_id
            ],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let Some(phase_id) = phase else {
        bail!(
            "canonical task is already phase-assigned for {}",
            identity.requirement_key
        );
    };
    replace_historical_memberships(conn, identity, phase_id)?;
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
        r#"delete from phase_epoch_membership_sources
           where source_membership_id in (
             select m.id from work_phase_task_memberships m
             join task_derivations td on td.task_id=m.task_id
             join design_requirements r on r.id=td.design_requirement_id
             join design_versions v on v.id=r.design_version_id
             join tasks t on t.id=td.task_id
             where m.project_id=?1 and m.phase_id=?2 and m.task_id!=?3
               and r.requirement_key=?4 and v.design_package_id=(
                 select dv.design_package_id from design_requirements current_r
                 join design_versions dv on dv.id=current_r.design_version_id
                 where current_r.id=?5)
               and v.version_number<(select current_v.version_number
                 from design_requirements current_r
                 join design_versions current_v on current_v.id=current_r.design_version_id
                 where current_r.id=?5)
               and t.work_unit_id=?6 and t.status in ('closed','accepted_out_of_scope')
           )"#,
        params![
            identity.project_id,
            phase_id,
            identity.canonical_task_id,
            identity.requirement_key,
            identity.current_requirement_id,
            identity.work_unit_id
        ],
    )?;
    conn.execute(
        r#"delete from task_phase_memberships
           where id in (
             select canonical.task_phase_membership_id
             from task_phase_membership_sources canonical
             where canonical.source_membership_id in (
             select m.id from work_phase_task_memberships m
             join task_derivations td on td.task_id=m.task_id
             join design_requirements r on r.id=td.design_requirement_id
             join design_versions v on v.id=r.design_version_id
             join tasks t on t.id=td.task_id
             where m.project_id=?1 and m.phase_id=?2 and m.task_id!=?3
               and r.requirement_key=?4 and v.design_package_id=(
                 select dv.design_package_id from design_requirements current_r
                 join design_versions dv on dv.id=current_r.design_version_id
                 where current_r.id=?5)
               and v.version_number<(select current_v.version_number
                 from design_requirements current_r
                 join design_versions current_v on current_v.id=current_r.design_version_id
                 where current_r.id=?5)
               and t.work_unit_id=?6 and t.status in ('closed','accepted_out_of_scope')
             )
           )"#,
        params![
            identity.project_id,
            phase_id,
            identity.canonical_task_id,
            identity.requirement_key,
            identity.current_requirement_id,
            identity.work_unit_id
        ],
    )?;
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
    synchronize_canonical_membership(conn, identity, phase_id)?;
    Ok(())
}

fn synchronize_canonical_membership(
    conn: &rusqlite::Connection,
    identity: &MembershipIdentity<'_>,
    phase_id: i64,
) -> Result<()> {
    let (task_identity_id, revision_id) = crate::task_identity::materialize_manual_task(
        conn,
        identity.project_id,
        identity.canonical_task_id,
    )?;
    let (source_membership_id, phase_status): (i64, String) = conn.query_row(
        r#"select membership.id,phase.status
           from work_phase_task_memberships membership
           join work_phases phase on phase.id=membership.phase_id
           where membership.project_id=?1 and membership.phase_id=?2
             and membership.task_id=?3"#,
        params![identity.project_id, phase_id, identity.canonical_task_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let state = match phase_status.as_str() {
        "open" => "open",
        "blocked" => "blocked",
        "closed" => "closed",
        "accepted_out_of_scope" => "out_of_scope",
        "split" => "split",
        _ => bail!("unsupported canonical phase membership state"),
    };
    conn.execute(
        r#"insert into task_phase_memberships(project_id,phase_id,task_identity_id,boundary_revision_id,state,created_at)
           values(?1,?2,?3,?4,?5,current_timestamp)
           on conflict(project_id,phase_id,task_identity_id) do update set
             boundary_revision_id=excluded.boundary_revision_id,
             state=excluded.state,
             created_at=excluded.created_at"#,
        params![
            identity.project_id,
            phase_id,
            task_identity_id,
            (state == "closed").then_some(revision_id),
            state
        ],
    )?;
    let task_membership_id: i64 = conn.query_row(
        "select id from task_phase_memberships where project_id=?1 and phase_id=?2 and task_identity_id=?3",
        params![identity.project_id, phase_id, task_identity_id],
        |row| row.get(0),
    )?;
    conn.execute(
        r#"insert into task_phase_membership_sources(project_id,task_phase_membership_id,source_membership_id,created_at)
           values(?1,?2,?3,current_timestamp)
           on conflict(project_id,source_membership_id) do update set
             task_phase_membership_id=excluded.task_phase_membership_id,
             created_at=excluded.created_at"#,
        params![identity.project_id, task_membership_id, source_membership_id],
    )?;
    let epoch_state = match state {
        "open" | "blocked" => "current",
        other => other,
    };
    conn.execute(
        r#"insert into phase_epoch_memberships(project_id,phase_epoch_id,task_identity_id,boundary_revision_id,state,predecessor_membership_id,created_at,terminal_at)
           values(?1,?2,?3,?4,?5,null,current_timestamp,case when ?5='current' then null else current_timestamp end)
           on conflict(project_id,phase_epoch_id,task_identity_id) do update set
             boundary_revision_id=excluded.boundary_revision_id,
             state=excluded.state,
             terminal_at=excluded.terminal_at"#,
        params![
            identity.project_id,
            phase_id,
            task_identity_id,
            (state == "closed").then_some(revision_id),
            epoch_state
        ],
    )?;
    let epoch_membership_id: i64 = conn.query_row(
        "select id from phase_epoch_memberships where project_id=?1 and phase_epoch_id=?2 and task_identity_id=?3",
        params![identity.project_id, phase_id, task_identity_id],
        |row| row.get(0),
    )?;
    conn.execute(
        r#"insert into phase_epoch_membership_sources(project_id,phase_epoch_membership_id,source_membership_id,source_generation,created_at)
           values(?1,?2,?3,15,current_timestamp)
           on conflict(source_membership_id) do update set
             phase_epoch_membership_id=excluded.phase_epoch_membership_id,
             source_generation=excluded.source_generation,
             created_at=excluded.created_at"#,
        params![identity.project_id, epoch_membership_id, source_membership_id],
    )?;
    Ok(())
}
