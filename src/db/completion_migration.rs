use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

use super::{project::*, runtime::*};

pub(super) fn ensure_completion_inheritance_triggers(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
drop trigger if exists trg_completion_evidence_insert;
create trigger if not exists trg_completion_source_insert
before insert on correction_completion_inheritance_sources
for each row when
    new.project_id!=(select project_id from correction_transition_applications where id=new.correction_application_id)
    or new.correction_session_id!=(select correction_session_id from correction_transition_applications where id=new.correction_application_id)
    or new.project_id!=(select project_id from correction_sessions where id=new.correction_session_id)
    or new.project_id!=(select project_id from design_requirements where id=new.current_requirement_id)
    or new.project_id!=(select project_id from design_requirements where id=new.source_requirement_id)
    or new.project_id!=(select project_id from work_phases where id=new.source_phase_id)
    or new.source_task_id!=(select task_id from checklist_items where id=new.source_checklist_item_id)
    or new.source_requirement_id!=(select design_requirement_id from checklist_items where id=new.source_checklist_item_id)
    or new.source_phase_id!=(select phase_id from work_phase_events where id=new.source_phase_closed_event_id and event_type='closed')
    or new.source_design_approval_event_id!=(select approved_by_authority_event_id from design_versions where id=(select design_version_id from design_requirements where id=new.source_requirement_id))
    or new.canonical_task_id!=(select task_id from checklist_items where id=new.canonical_checklist_item_id)
    or new.current_requirement_id!=(select design_requirement_id from checklist_items where id=new.canonical_checklist_item_id)
    or (select work_unit_id from tasks where id=new.source_task_id)!=(select work_unit_id from tasks where id=new.canonical_task_id)
    or (select work_unit_id from work_phases where id=new.source_phase_id)!=(select work_unit_id from tasks where id=new.canonical_task_id)
    or (select design_package_id from design_versions where id=(select design_version_id from design_requirements where id=new.source_requirement_id))
       !=(select design_package_id from design_versions where id=(select design_version_id from design_requirements where id=new.current_requirement_id))
    or not exists(
       select 1
       from design_requirements old_requirement
       join design_requirements current_requirement on current_requirement.id=new.current_requirement_id
       join design_versions old_version on old_version.id=old_requirement.design_version_id
       join design_versions current_version on current_version.id=current_requirement.design_version_id
       where old_requirement.id=new.source_requirement_id
         and old_version.status in ('approved','superseded')
         and old_version.approved_at is not null
         and old_version.approved_by_authority_event_id=new.source_design_approval_event_id
         and exists(select 1 from authority_events approval
                    where approval.id=old_version.approved_by_authority_event_id
                      and approval.project_id=new.project_id and approval.status='active')
         and old_version.design_package_id=current_version.design_package_id
         and old_version.version_number<current_version.version_number
         and old_version.version_number=(
           select max(candidate_version.version_number)
           from design_versions candidate_version
           where candidate_version.design_package_id=current_version.design_package_id
             and candidate_version.version_number<current_version.version_number
             and candidate_version.status in ('approved','superseded')
             and candidate_version.approved_at is not null
             and candidate_version.approved_by_authority_event_id is not null
             and exists(select 1 from authority_events candidate_approval
                        where candidate_approval.id=candidate_version.approved_by_authority_event_id
                          and candidate_approval.project_id=new.project_id and candidate_approval.status='active')
             and (select count(*) from design_requirements candidate_requirement
                  where candidate_requirement.design_version_id=candidate_version.id
                    and candidate_requirement.requirement_key=current_requirement.requirement_key)=1)
         and old_requirement.requirement_key=current_requirement.requirement_key
         and old_requirement.revision=current_requirement.revision
         and old_requirement.requirement_hash=current_requirement.requirement_hash
         and old_requirement.required_surfaces is current_requirement.required_surfaces
         and not exists(
           select 1 from validation_gate_template_requirements current_map
           join validation_gate_templates current_template on current_template.id=current_map.validation_gate_template_id
             and current_template.status='active'
           where current_map.design_requirement_id=current_requirement.id
             and not exists(
               select 1 from validation_gate_template_requirements old_map
               join validation_gate_templates old_template on old_template.id=old_map.validation_gate_template_id
                 and old_template.status='active'
               where old_map.design_requirement_id=old_requirement.id
                 and old_template.gate_key=current_template.gate_key
                 and old_template.gate_hash=current_template.gate_hash))
         and not exists(
           select 1 from validation_gate_template_requirements old_map
           join validation_gate_templates old_template on old_template.id=old_map.validation_gate_template_id
             and old_template.status='active'
           where old_map.design_requirement_id=old_requirement.id
             and not exists(
               select 1 from validation_gate_template_requirements current_map
               join validation_gate_templates current_template on current_template.id=current_map.validation_gate_template_id
                 and current_template.status='active'
               where current_map.design_requirement_id=current_requirement.id
                 and current_template.gate_key=old_template.gate_key
                 and current_template.gate_hash=old_template.gate_hash))
    )
    or new.source_membership_assigned_at>(select closed_at from work_phases where id=new.source_phase_id)
    or (select created_at from work_phase_events where id=new.source_phase_closed_event_id)!=(select closed_at from work_phases where id=new.source_phase_id)
    or ('|'||(select substr(before_state,instr(before_state,'memberships=[')+13,instr(before_state,'];checklists=')-(instr(before_state,'memberships=[')+13)) from correction_transition_applications where id=new.correction_application_id)||'|')
       not like '%|'||new.source_membership_id||':'||new.source_phase_id||':'||new.source_task_id||':'||new.source_membership_assigned_at||'|%'
    or ('|'||(select substr(after_state,instr(after_state,'memberships=[')+13,instr(after_state,'];checklists=')-(instr(after_state,'memberships=[')+13)) from correction_transition_applications where id=new.correction_application_id)||'|')
       like '%|'||new.source_membership_id||':%'
    or ('|'||(select substr(after_state,instr(after_state,'memberships=[')+13,instr(after_state,'];checklists=')-(instr(after_state,'memberships=[')+13)) from correction_transition_applications where id=new.correction_application_id)||'|')
       not like '%:'||new.source_phase_id||':'||new.canonical_task_id||':%'
    or ('|'||(select substr(before_state,instr(before_state,'tasks=[')+7,instr(before_state,'];memberships=')-(instr(before_state,'tasks=[')+7)) from correction_transition_applications where id=new.correction_application_id)||'|') not like '%|'||new.source_task_id||':closed|%'
    or ('|'||(select substr(after_state,instr(after_state,'tasks=[')+7,instr(after_state,'];memberships=')-(instr(after_state,'tasks=[')+7)) from correction_transition_applications where id=new.correction_application_id)||'|') not like '%|'||new.source_task_id||':closed|%'
    or ('|'||(select substr(before_state,instr(before_state,'phases=[')+8,instr(before_state,'];phase_dependencies=')-(instr(before_state,'phases=[')+8)) from correction_transition_applications where id=new.correction_application_id)||'|') not like '%|'||new.source_phase_id||':closed|%'
    or ('|'||(select substr(after_state,instr(after_state,'phases=[')+8,instr(after_state,'];phase_dependencies=')-(instr(after_state,'phases=[')+8)) from correction_transition_applications where id=new.correction_application_id)||'|') not like '%|'||new.source_phase_id||':closed|%'
begin select raise(abort, 'invalid completion inheritance source ownership'); end;
create trigger if not exists trg_completion_evidence_insert
before insert on correction_completion_inheritance_evidence
for each row when
    new.project_id!=(select project_id from correction_completion_inheritance_sources where id=new.inheritance_source_id)
    or (new.evidence_kind='implementation_evidence' and not exists(
        select 1 from implementation_evidence evidence join correction_completion_inheritance_sources source on source.id=new.inheritance_source_id
        join work_phases phase on phase.id=source.source_phase_id
        where evidence.id=new.source_record_id and evidence.task_id=source.source_task_id and evidence.design_requirement_id=source.source_requirement_id
          and evidence.created_at<=phase.closed_at))
    or (new.evidence_kind='coverage_item' and not exists(
        select 1 from coverage_items old_coverage join coverage_items current_coverage on current_coverage.id=new.canonical_record_id
        join correction_completion_inheritance_sources source on source.id=new.inheritance_source_id
        join work_phases phase on phase.id=source.source_phase_id
        where old_coverage.id=new.source_record_id and old_coverage.task_id=source.source_task_id and old_coverage.design_requirement_id=source.source_requirement_id
          and old_coverage.status='covered' and old_coverage.created_at<=phase.closed_at
          and current_coverage.task_id=source.canonical_task_id and current_coverage.design_requirement_id=source.current_requirement_id))
    or (new.evidence_kind='validation_gate' and not exists(
        select 1 from validation_gates old_gate join validation_gates current_gate on current_gate.id=new.canonical_record_id
        join validation_runs run on run.id=new.validation_run_id and run.validation_gate_id=old_gate.id and run.result='pass'
        join correction_completion_inheritance_sources source on source.id=new.inheritance_source_id
        join validation_gate_templates old_template on old_template.id=old_gate.template_id
        join validation_gate_templates current_template on current_template.id=current_gate.template_id
        join work_phases phase on phase.id=source.source_phase_id
        where old_gate.id=new.source_record_id and old_gate.task_id=source.source_task_id and old_gate.design_requirement_id=source.source_requirement_id
          and current_gate.task_id=source.canonical_task_id and current_gate.design_requirement_id=source.current_requirement_id
          and old_template.gate_key=current_template.gate_key and old_template.gate_hash=current_template.gate_hash
          and run.created_at<=phase.closed_at
          and (old_gate.command is null or (run.command is old_gate.command
            and run.command_usage_id is not null and exists(
              select 1 from command_usages usage
              where usage.id=run.command_usage_id and usage.project_id=source.project_id
                and usage.work_unit_id=(select work_unit_id from tasks where id=source.source_task_id)
                and usage.command=old_gate.command and usage.result='pass')))
          and run.id=(select max(candidate.id) from validation_runs candidate where candidate.validation_gate_id=old_gate.id and candidate.created_at<=phase.closed_at)))
begin select raise(abort, 'invalid completion inheritance evidence ownership'); end;
"#,
    )?;
    Ok(())
}

pub(super) fn migrate_completion_identity_link_kind(conn: &Connection) -> Result<()> {
    let table_sql: Option<String> = conn
        .query_row(
            "select sql from sqlite_schema where type='table' and name='correction_application_identity_links'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    if table_sql
        .as_deref()
        .is_none_or(|sql| sql.contains("completion_source"))
    {
        return Ok(());
    }
    let schema_version: i64 = conn.pragma_query_value(None, "schema_version", |row| row.get(0))?;
    conn.execute_batch(
        r#"pragma writable_schema=on;
        update sqlite_schema set sql=replace(sql,
          '''membership_removed'', ''membership_assigned''',
          '''membership_removed'', ''membership_assigned'', ''completion_source''')
        where type='table' and name='correction_application_identity_links';
        pragma writable_schema=off;"#,
    )?;
    conn.pragma_update(None, "schema_version", schema_version + 1)?;
    Ok(())
}

#[allow(dead_code)]
pub(super) fn backfill_task_acceptance_bundles(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "tasks")? || !table_exists(conn, "checklist_items")? {
        return Ok(());
    }
    let mut stmt = conn.prepare(
        r#"
        select t.id, t.work_unit_id, ar.approved_by_authority_event_id
        from tasks t
        join task_derivations td on td.task_id=t.id and td.status='active'
        join acceptance_records ar on ar.id=(
          select max(latest.id) from acceptance_records latest
          where latest.task_id=t.id and latest.target_type='task'
            and latest.status='approved' and latest.acceptance_type='accepted_out_of_scope'
        )
        where t.status='accepted_out_of_scope'
        group by t.id, t.work_unit_id, ar.approved_by_authority_event_id
        "#,
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, Option<i64>>(2)?,
        ))
    })?;
    let derived_acceptances = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    if !derived_acceptances.is_empty() {
        let project_id = project_id(conn)?;
        for (task_id, work_unit_id, authority_event_id) in derived_acceptances {
            let authority_event_id = authority_event_id.context(
                "design-derived task acceptance migration lacks approved authority provenance",
            )?;
            if let Some(work_unit_id) = work_unit_id {
                conn.execute(
                    "update authority_events set scope=?1 where id=?2 and project_id=?3 and scope=?4",
                    params![
                        format!("work-unit:{work_unit_id}"),
                        authority_event_id,
                        project_id,
                        work_unit_id.to_string()
                    ],
                )?;
            }
            crate::planning::ensure_verified_baseline_carry_forward(
                conn,
                project_id,
                task_id,
                work_unit_id,
                authority_event_id,
            )?;
        }
    }
    conn.execute_batch(
        r#"
        update checklist_items
        set status = 'accepted_out_of_scope'
        where status in ('open', 'blocked')
          and task_id in (select id from tasks where status = 'accepted_out_of_scope');

        update validation_gates
        set status = 'closed'
        where status in ('active', 'stale')
          and task_id in (select id from tasks where status = 'accepted_out_of_scope');

        insert into acceptance_records(
            project_id, target_type, checklist_item_id, acceptance_type, reason,
            scope, created_by, status, approved_by_authority_event_id,
            approved_at, created_at, review_impact
        )
        select ci.project_id, 'checklist_item', ci.id, 'accepted_out_of_scope',
               ar.reason, ar.scope, ar.created_by, 'approved',
               ar.approved_by_authority_event_id, coalesce(ar.approved_at, current_timestamp),
               current_timestamp, 'checklist item repaired from task acceptance authority'
        from checklist_items ci
        join acceptance_records ar on ar.task_id = ci.task_id
          and ar.target_type = 'task' and ar.status = 'approved'
          and ar.acceptance_type = 'accepted_out_of_scope'
        where ci.status = 'accepted_out_of_scope'
          and ar.id = (select max(latest.id) from acceptance_records latest
                       where latest.task_id = ci.task_id and latest.target_type = 'task'
                         and latest.status = 'approved')
          and not exists (select 1 from acceptance_records existing
                          where existing.target_type = 'checklist_item'
                            and existing.checklist_item_id = ci.id and existing.status = 'approved');

        insert into acceptance_records(
            project_id, target_type, validation_gate_id, acceptance_type, reason,
            scope, created_by, status, approved_by_authority_event_id,
            approved_at, created_at, review_impact
        )
        select vg.project_id, 'validation_gate', vg.id, 'accepted_out_of_scope',
               ar.reason, ar.scope, ar.created_by, 'approved',
               ar.approved_by_authority_event_id, coalesce(ar.approved_at, current_timestamp),
               current_timestamp, 'validation gate repaired from task acceptance authority'
        from validation_gates vg
        join acceptance_records ar on ar.task_id = vg.task_id
          and ar.target_type = 'task' and ar.status = 'approved'
          and ar.acceptance_type = 'accepted_out_of_scope'
        where vg.status = 'closed'
          and ar.id = (select max(latest.id) from acceptance_records latest
                       where latest.task_id = vg.task_id and latest.target_type = 'task'
                         and latest.status = 'approved')
          and not exists (select 1 from acceptance_records existing
                          where existing.target_type = 'validation_gate'
                            and existing.validation_gate_id = vg.id and existing.status = 'approved');

        insert into coverage_items(
            project_id, work_unit_id, design_requirement_id, task_id,
            requirement, lifecycle_boundary_evidence, tests_or_gates,
            status, created_at
        )
        select
            w.project_id, t.work_unit_id, td.design_requirement_id, t.id,
            'authority-backed task disposition migration',
            'task acceptance bundle repaired atomically from its approved authority record',
            'validation not claimed; requirement accepted_out_of_scope by authority',
            'accepted_out_of_scope', current_timestamp
        from tasks t
        join work_units w on w.id = t.work_unit_id
        join task_derivations td on td.task_id = t.id
        where t.status = 'accepted_out_of_scope'
          and not exists (
            select 1 from coverage_items c
            where c.task_id = t.id and c.design_requirement_id = td.design_requirement_id
          );

        update coverage_items
        set status = 'accepted_out_of_scope'
        where task_id in (select id from tasks where status = 'accepted_out_of_scope');

        insert into acceptance_records(
            project_id, target_type, coverage_item_id, acceptance_type, reason,
            scope, created_by, status, approved_by_authority_event_id,
            approved_at, created_at, review_impact
        )
        select
            c.project_id, 'coverage_item', c.id, 'accepted_out_of_scope',
            ar.reason, ar.scope, ar.created_by, 'approved',
            ar.approved_by_authority_event_id, coalesce(ar.approved_at, current_timestamp),
            current_timestamp, 'coverage carried by migration with task acceptance authority'
        from coverage_items c
        join acceptance_records ar
          on ar.task_id = c.task_id and ar.target_type = 'task'
         and ar.status = 'approved' and ar.acceptance_type = 'accepted_out_of_scope'
        where c.status = 'accepted_out_of_scope'
          and ar.id = (
              select max(latest.id) from acceptance_records latest
              where latest.task_id = c.task_id and latest.target_type = 'task'
                and latest.status = 'approved'
                and latest.acceptance_type = 'accepted_out_of_scope'
          )
          and not exists (
              select 1 from acceptance_records existing
              where existing.target_type = 'coverage_item'
                and existing.coverage_item_id = c.id and existing.status = 'approved'
          );

        update checklists
        set status = 'closed'
        where status = 'active'
          and not exists (
            select 1 from checklist_items ci
            where ci.checklist_id = checklists.id
              and ci.status in ('open', 'blocked')
          );
        "#,
    )?;
    Ok(())
}
