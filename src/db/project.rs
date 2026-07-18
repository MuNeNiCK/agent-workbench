use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

use super::runtime::*;

pub(super) fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    if table_has_column(conn, table, column)? {
        return Ok(());
    }

    conn.execute(
        &format!("alter table {table} add column {column} {definition}"),
        [],
    )?;
    Ok(())
}

pub(super) const PHASE_SCHEMA: &str = r#"
create table if not exists work_phases (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    work_unit_id integer not null references work_units(id) on delete cascade,
    phase_work_unit_id integer references work_units(id) on delete set null,
    design_version_id integer references design_versions(id) on delete set null,
    phase_key text not null,
    title text not null,
    kind text not null,
    phase_order integer not null,
    status text not null default 'open' check (status in ('open', 'blocked', 'closed', 'accepted_out_of_scope', 'split')),
    reason text,
    authority_event_id integer references authority_events(id),
    created_at text not null,
    closed_at text,
    close_summary text,
    unique(project_id, work_unit_id, phase_key)
);

create table if not exists work_phase_task_memberships (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    phase_id integer not null references work_phases(id) on delete cascade,
    task_id integer not null references tasks(id) on delete cascade,
    assigned_at text not null,
    unique(task_id)
);

create table if not exists work_phase_dependencies (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    from_phase_id integer not null references work_phases(id) on delete cascade,
    to_phase_id integer not null references work_phases(id) on delete cascade,
    dependency_type text not null check (dependency_type in ('blocks', 'requires')),
    reason text not null,
    status text not null default 'open' check (status in ('open', 'satisfied', 'accepted')),
    evidence_ref text,
    authority_event_id integer references authority_events(id),
    created_at text not null,
    resolved_at text
);

create table if not exists work_phase_trace_decisions (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    phase_id integer not null references work_phases(id) on delete cascade,
    record_type text not null check (record_type in (
        'task', 'task_derivation', 'checklist_item', 'validation_gate',
        'coverage_item', 'implementation_evidence', 'review_plan',
        'rule_binding', 'work_record'
    )),
    record_id integer not null,
    decision text not null check (decision in ('split', 'carry', 'accept')),
    reason text not null,
    authority_event_id integer not null references authority_events(id),
    created_at text not null,
    unique(phase_id, record_type, record_id)
);

create table if not exists work_phase_review_targets (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    review_plan_id integer not null references review_plans(id) on delete cascade,
    phase_id integer not null references work_phases(id) on delete cascade,
    created_at text not null,
    unique(review_plan_id, phase_id)
);

create table if not exists work_phase_events (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    phase_id integer not null references work_phases(id) on delete cascade,
    event_type text not null check (event_type in (
        'created', 'assigned', 'dependency_added', 'dependency_satisfied',
        'dependency_accepted', 'trace_decided', 'rescope_dry_run',
        'rescoped', 'split', 'closed', 'accepted_out_of_scope'
    )),
    reason text,
    authority_event_id integer references authority_events(id),
    related_task_id integer references tasks(id),
    related_work_unit_id integer references work_units(id),
    previous_status text,
    next_status text,
    created_at text not null
);

create trigger if not exists trg_work_phase_work_unit_project_insert
before insert on work_phases
for each row
when new.project_id != (select project_id from work_units where id = new.work_unit_id)
  or (
      new.phase_work_unit_id is not null
      and new.project_id != (select project_id from work_units where id = new.phase_work_unit_id)
  )
begin
    select raise(abort, 'work phase work units must match project_id');
end;

create trigger if not exists trg_work_phase_membership_project_insert
before insert on work_phase_task_memberships
for each row
when new.project_id != (select project_id from work_phases where id = new.phase_id)
  or new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      new.project_id
  )
begin
    select raise(abort, 'work phase task membership must match project_id');
end;

create trigger if not exists trg_work_phase_dependency_project_insert
before insert on work_phase_dependencies
for each row
when new.project_id != (select project_id from work_phases where id = new.from_phase_id)
  or new.project_id != (select project_id from work_phases where id = new.to_phase_id)
  or (select work_unit_id from work_phases where id = new.from_phase_id)
      != (select work_unit_id from work_phases where id = new.to_phase_id)
begin
    select raise(abort, 'work phase dependency phases must share project and aggregate work unit');
end;

create trigger if not exists trg_work_phase_review_target_project_insert
before insert on work_phase_review_targets
for each row
when new.project_id != (select project_id from review_plans where id = new.review_plan_id)
  or new.project_id != (select project_id from work_phases where id = new.phase_id)
begin
    select raise(abort, 'work phase review target must match project_id');
end;
"#;

pub(super) fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("pragma table_info({table})"))?;
    let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for existing in columns {
        if existing? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    let exists = conn
        .query_row(
            "select 1 from sqlite_schema where type = 'table' and name = ?1",
            params![table],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok(exists)
}

pub(crate) fn ensure_project(conn: &Connection, root: &Path) -> Result<()> {
    let root_path = root
        .canonicalize()
        .unwrap_or_else(|_| root.to_path_buf())
        .display()
        .to_string();
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project");

    conn.execute(
        r#"
        insert into projects(name, root_path, created_at, updated_at)
        select ?1, ?2, current_timestamp, current_timestamp
        where not exists (select 1 from projects where root_path = ?2)
        "#,
        params![name, root_path],
    )?;

    Ok(())
}

pub(crate) fn sync_agents_md_authority(conn: &Connection, root: &Path) -> Result<()> {
    let agents_path = root.join("AGENTS.md");
    if !agents_path.exists() {
        return Ok(());
    }
    let project_id = project_id(conn)?;
    let source = "AGENTS.md";
    let summary = fs::read_to_string(&agents_path)
        .with_context(|| format!("failed to read {}", agents_path.display()))?;
    let authority_id = ensure_authority_row(
        conn,
        project_id,
        source,
        "policy",
        Some("project"),
        70,
        &summary,
    )?;
    let authority_event_id = conn
        .query_row(
            r#"
            select id
            from authority_events
            where project_id = ?1
              and event_type = 'agents'
              and source = ?2
              and status = 'active'
            order by id desc
            limit 1
            "#,
            params![project_id, source],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let authority_event_id = match authority_event_id {
        Some(id) => {
            conn.execute(
                r#"
                update authority_events
                set authority_id = ?1, text_or_summary = ?2
                where id = ?3
                "#,
                params![authority_id, summary, id],
            )?;
            id
        }
        None => {
            conn.execute(
                r#"
                insert into authority_events(
                    project_id, authority_id, event_type, source, text_or_summary, scope,
                    precedence, status, created_at
                )
                values (?1, ?2, 'agents', ?3, ?4, 'project', 70, 'active', current_timestamp)
                "#,
                params![project_id, authority_id, source, summary],
            )?;
            conn.last_insert_rowid()
        }
    };
    conn.execute(
        r#"
        insert into rule_bindings(
            project_id, rule_source_type, authority_event_id, scope_type, scope_key,
            precedence, status, created_at
        )
        select ?1, 'authority_event', ?2, 'project', 'project', 70, 'active', current_timestamp
        where not exists (
            select 1
            from rule_bindings
            where project_id = ?1
              and authority_event_id = ?2
              and status = 'active'
        )
        "#,
        params![project_id, authority_event_id],
    )?;
    Ok(())
}

pub(crate) fn sync_commit_message_policy(conn: &Connection) -> Result<()> {
    let project_id = project_id(conn)?;
    let source = "agent-workbench:commit-message";
    let summary = "Commit subjects must use `prefix: message` and must not contain internal milestone names or the literal review token.";
    let authority_id = ensure_authority_row(
        conn,
        project_id,
        source,
        "policy",
        Some("project"),
        75,
        summary,
    )?;
    let authority_event_id = conn
        .query_row(
            r#"
            select id
            from authority_events
            where project_id = ?1
              and event_type = 'policy'
              and source = ?2
              and status = 'active'
            order by id desc
            limit 1
            "#,
            params![project_id, source],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let authority_event_id = match authority_event_id {
        Some(id) => {
            conn.execute(
                "update authority_events set authority_id = ?1, text_or_summary = ?2 where id = ?3",
                params![authority_id, summary, id],
            )?;
            id
        }
        None => {
            conn.execute(
                r#"
                insert into authority_events(
                    project_id, authority_id, event_type, source, text_or_summary, scope,
                    precedence, status, created_at
                )
                values (?1, ?2, 'policy', ?3, ?4, 'project', 75, 'active', current_timestamp)
                "#,
                params![project_id, authority_id, source, summary],
            )?;
            conn.last_insert_rowid()
        }
    };
    conn.execute(
        r#"
        insert into rule_bindings(
            project_id, rule_source_type, authority_event_id, scope_type, scope_key,
            precedence, status, created_at
        )
        select ?1, 'authority_event', ?2, 'project', 'project', 75, 'active', current_timestamp
        where not exists (
            select 1
            from rule_bindings
            where project_id = ?1
              and authority_event_id = ?2
              and status = 'active'
        )
        "#,
        params![project_id, authority_event_id],
    )?;
    Ok(())
}

pub(super) fn backfill_authorities(conn: &Connection) -> Result<()> {
    conn.execute(
        r#"
        insert into authorities(
            project_id, path_or_label, authority_type, scope, precedence,
            summary, status, created_at, updated_at
        )
        select e.project_id,
               coalesce(e.source, e.event_type),
               case e.event_type
                   when 'user_instruction' then 'user'
                   when 'design_doc' then 'design'
                   when 'validation_result' then 'validation'
                   when 'review_result' then 'validation'
                   else 'policy'
               end,
               e.scope,
               max(e.precedence),
               e.text_or_summary,
               'active',
               current_timestamp,
               current_timestamp
        from authority_events e
        left join authorities a
          on a.project_id = e.project_id
         and a.path_or_label = coalesce(e.source, e.event_type)
         and a.authority_type = case e.event_type
                   when 'user_instruction' then 'user'
                   when 'design_doc' then 'design'
                   when 'validation_result' then 'validation'
                   when 'review_result' then 'validation'
                   else 'policy'
               end
         and coalesce(a.scope, 'project') = coalesce(e.scope, 'project')
        where e.authority_id is null
          and a.id is null
        group by e.project_id, coalesce(e.source, e.event_type), e.event_type, e.scope
        "#,
        [],
    )?;
    conn.execute(
        r#"
        update authority_events
        set authority_id = (
            select a.id
            from authorities a
            where a.project_id = authority_events.project_id
              and a.path_or_label = coalesce(authority_events.source, authority_events.event_type)
              and a.authority_type = case authority_events.event_type
                   when 'user_instruction' then 'user'
                   when 'design_doc' then 'design'
                   when 'validation_result' then 'validation'
                   when 'review_result' then 'validation'
                   else 'policy'
              end
              and coalesce(a.scope, 'project') = coalesce(authority_events.scope, 'project')
            order by a.id desc
            limit 1
        )
        where authority_id is null
        "#,
        [],
    )?;
    Ok(())
}

pub(super) fn ensure_authority_row(
    conn: &Connection,
    project_id: i64,
    path_or_label: &str,
    authority_type: &str,
    scope: Option<&str>,
    precedence: i64,
    summary: &str,
) -> Result<i64> {
    let existing_id = conn
        .query_row(
            r#"
            select id
            from authorities
            where project_id = ?1
              and path_or_label = ?2
              and authority_type = ?3
              and coalesce(scope, 'project') = coalesce(?4, 'project')
            "#,
            params![project_id, path_or_label, authority_type, scope],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    if let Some(id) = existing_id {
        conn.execute(
            r#"
            update authorities
            set precedence = ?1,
                summary = ?2,
                status = 'active',
                updated_at = current_timestamp
            where id = ?3
            "#,
            params![precedence, summary, id],
        )?;
        return Ok(id);
    }
    conn.execute(
        r#"
        insert into authorities(
            project_id, path_or_label, authority_type, scope, precedence,
            summary, status, created_at, updated_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, 'active', current_timestamp, current_timestamp)
        "#,
        params![
            project_id,
            path_or_label,
            authority_type,
            scope,
            precedence,
            summary
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub(super) fn count_rows(conn: &Connection, table: &str, predicate: &str) -> Result<i64> {
    let sql = format!("select count(*) from {table} where {predicate}");
    let count = conn.query_row(&sql, [], |row| row.get(0))?;
    Ok(count)
}
