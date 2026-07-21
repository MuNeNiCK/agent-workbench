use anyhow::{Result, bail};
use rusqlite::{Connection, params};

pub(crate) const SQL: &str = r#"
create table if not exists phase_epochs (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    work_unit_id integer not null references work_units(id) on delete cascade,
    phase_work_unit_id integer references work_units(id) on delete set null,
    design_version_id integer references design_versions(id) on delete set null,
    phase_key text not null,
    title text not null,
    kind text not null,
    phase_order integer not null,
    state text not null check(state in ('open','blocked','closed','split','superseded')),
    predecessor_epoch_id integer references phase_epochs(id),
    reason text,
    authority_event_id integer references authority_events(id),
    created_at text not null,
    terminal_at text,
    terminal_summary text
);

create unique index if not exists phase_epoch_current_design_key
on phase_epochs(project_id,work_unit_id,design_version_id,phase_key)
where design_version_id is not null and state!='superseded';

create unique index if not exists phase_epoch_current_manual_key
on phase_epochs(project_id,work_unit_id,phase_key)
where design_version_id is null and state!='superseded';

create table if not exists phase_epoch_sources (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    phase_epoch_id integer not null unique references phase_epochs(id),
    source_phase_id integer not null unique references work_phases(id),
    source_generation integer not null,
    created_at text not null
);

create table if not exists phase_epoch_memberships (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    phase_epoch_id integer not null references phase_epochs(id),
    task_identity_id integer not null references task_identities(id),
    boundary_revision_id integer references task_revisions(id),
    state text not null check(state in ('current','closed','out_of_scope','split','superseded')),
    predecessor_membership_id integer references phase_epoch_memberships(id),
    created_at text not null,
    terminal_at text,
    unique(project_id,phase_epoch_id,task_identity_id)
);

create unique index if not exists phase_epoch_membership_current_task
on phase_epoch_memberships(project_id,task_identity_id) where state='current';

create table if not exists phase_epoch_membership_sources (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    phase_epoch_membership_id integer not null references phase_epoch_memberships(id),
    source_membership_id integer not null unique references work_phase_task_memberships(id),
    source_generation integer not null,
    created_at text not null
);

create table if not exists phase_epoch_dependencies (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    from_phase_epoch_id integer not null references phase_epochs(id),
    to_phase_epoch_id integer not null references phase_epochs(id),
    dependency_type text not null check(dependency_type in ('blocks','requires')),
    reason text not null,
    state text not null check(state in ('open','satisfied','accepted','invalidated')),
    evidence_ref text,
    authority_event_id integer references authority_events(id),
    created_at text not null,
    terminal_at text,
    unique(project_id,from_phase_epoch_id,to_phase_epoch_id,dependency_type)
);

create table if not exists phase_epoch_dependency_sources (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    phase_epoch_dependency_id integer not null unique references phase_epoch_dependencies(id),
    source_dependency_id integer not null unique references work_phase_dependencies(id),
    source_generation integer not null,
    created_at text not null
);

create table if not exists phase_scope_dispositions (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    phase_epoch_id integer not null references phase_epochs(id),
    scope_kind text not null check(scope_kind in ('whole_phase','task_identity')),
    task_identity_id integer references task_identities(id),
    state text not null check(state in ('open','accepted_out_of_scope','retired')),
    reason text not null,
    authority_event_id integer references authority_events(id),
    created_at text not null,
    check((scope_kind='whole_phase' and task_identity_id is null)
       or (scope_kind='task_identity' and task_identity_id is not null)),
    unique(project_id,phase_epoch_id,scope_kind,task_identity_id)
);

create unique index if not exists phase_scope_whole_phase_unique
on phase_scope_dispositions(project_id,phase_epoch_id)
where scope_kind='whole_phase';

create unique index if not exists phase_scope_task_identity_unique
on phase_scope_dispositions(project_id,phase_epoch_id,task_identity_id)
where scope_kind='task_identity';

create table if not exists phase_scope_disposition_sources (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    phase_scope_disposition_id integer not null unique references phase_scope_dispositions(id),
    source_phase_id integer not null references work_phases(id),
    source_generation integer not null,
    created_at text not null
);
"#;

pub(crate) fn install_phase_epochs(conn: &Connection) -> Result<()> {
    conn.execute_batch(SQL)?;
    conn.execute_batch(
        r#"
        insert or ignore into phase_epochs(
          id,project_id,work_unit_id,phase_work_unit_id,design_version_id,phase_key,
          title,kind,phase_order,state,predecessor_epoch_id,reason,authority_event_id,
          created_at,terminal_at,terminal_summary
        )
        select id,project_id,work_unit_id,phase_work_unit_id,design_version_id,phase_key,
               title,kind,phase_order,
               case status
                 when 'accepted_out_of_scope' then 'superseded'
                 else status
               end,
               null,reason,authority_event_id,created_at,closed_at,close_summary
        from work_phases;

        insert or ignore into phase_epoch_sources(
          project_id,phase_epoch_id,source_phase_id,source_generation,created_at
        )
        select project_id,id,id,15,current_timestamp from work_phases;

        insert or ignore into phase_epoch_dependencies(
          id,project_id,from_phase_epoch_id,to_phase_epoch_id,dependency_type,reason,
          state,evidence_ref,authority_event_id,created_at,terminal_at
        )
        select id,project_id,from_phase_id,to_phase_id,dependency_type,reason,status,
               evidence_ref,authority_event_id,created_at,resolved_at
        from work_phase_dependencies;

        insert or ignore into phase_epoch_dependency_sources(
          project_id,phase_epoch_dependency_id,source_dependency_id,source_generation,created_at
        )
        select project_id,id,id,15,current_timestamp from work_phase_dependencies;

        insert or ignore into phase_epoch_memberships(
          id,project_id,phase_epoch_id,task_identity_id,boundary_revision_id,state,
          predecessor_membership_id,created_at,terminal_at
        )
        select membership.id,membership.project_id,membership.phase_id,
               membership.task_identity_id,membership.boundary_revision_id,
               case membership.state
                 when 'open' then 'current'
                 when 'blocked' then 'current'
                 when 'closed' then 'closed'
                 when 'out_of_scope' then 'out_of_scope'
                 when 'split' then 'split'
               end,
               null,membership.created_at,
               case when membership.state in ('closed','out_of_scope','split')
                    then membership.created_at else null end
        from task_phase_memberships membership;

        insert or ignore into phase_epoch_membership_sources(
          project_id,phase_epoch_membership_id,source_membership_id,source_generation,created_at
        )
        select source.project_id,source.task_phase_membership_id,
               source.source_membership_id,15,source.created_at
        from task_phase_membership_sources source;

        insert or ignore into phase_scope_dispositions(
          project_id,phase_epoch_id,scope_kind,task_identity_id,state,reason,
          authority_event_id,created_at
        )
        select project_id,id,'whole_phase',null,'accepted_out_of_scope',
               coalesce(reason,'legacy whole-phase out-of-scope disposition'),
               authority_event_id,coalesce(closed_at,created_at)
        from work_phases where status='accepted_out_of_scope';
        "#,
    )?;
    let dispositions = conn
        .prepare(
            r#"
            select disposition.project_id,disposition.id,disposition.phase_epoch_id
            from phase_scope_dispositions disposition
            where disposition.scope_kind='whole_phase'
              and not exists(
                select 1 from phase_scope_disposition_sources source
                where source.phase_scope_disposition_id=disposition.id
              )
            order by disposition.id
            "#,
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (project, disposition, source_phase) in dispositions {
        conn.execute(
            "insert into phase_scope_disposition_sources(project_id,phase_scope_disposition_id,source_phase_id,source_generation,created_at) values(?1,?2,?3,15,current_timestamp)",
            params![project, disposition, source_phase],
        )?;
    }
    let missing_phases: i64 = conn.query_row(
        "select count(*) from work_phases source left join phase_epoch_sources mapping on mapping.source_phase_id=source.id where mapping.id is null",
        [],
        |row| row.get(0),
    )?;
    let missing_dependencies: i64 = conn.query_row(
        "select count(*) from work_phase_dependencies source left join phase_epoch_dependency_sources mapping on mapping.source_dependency_id=source.id where mapping.id is null",
        [],
        |row| row.get(0),
    )?;
    let missing_memberships: i64 = conn.query_row(
        "select count(*) from task_phase_membership_sources source left join phase_epoch_membership_sources mapping on mapping.source_membership_id=source.source_membership_id where mapping.id is null",
        [],
        |row| row.get(0),
    )?;
    if missing_phases != 0 || missing_dependencies != 0 || missing_memberships != 0 {
        bail!("phase epoch migration did not conserve every source endpoint");
    }
    Ok(())
}
