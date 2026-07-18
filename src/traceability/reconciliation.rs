use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::{decomposition::*, evidence::*, phase_membership::*, *};

mod retirement;

use retirement::retire_historical_decompositions_in;

pub(crate) fn reconcile_design_in(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_version_id: i64,
    work_unit_id: i64,
    canonical_checklist_id: i64,
    reason: &str,
) -> Result<ReconcileDesignOutcome> {
    validate_design_decomposition_scope_in(conn, project_id, design_version_id, work_unit_id)?;
    conn.query_row(
        "select 1 from checklists where id=?1 and project_id=?2 and design_version_id=?3 and work_unit_id=?4 and status='active' and trim(title)!=''",
        params![canonical_checklist_id, project_id, design_version_id, work_unit_id],
        |_| Ok(()),
    )
    .optional()?
    .context("canonical reconciliation checklist is not active for the correction design and owner")?;
    let foreign_canonical_rows: i64 = conn.query_row(
        r#"
        select count(*)
        from checklist_items ci
        left join design_requirements r on r.id=ci.design_requirement_id
        where ci.checklist_id=?1
          and (ci.project_id!=?2 or r.id is null or r.project_id!=?2 or r.design_version_id!=?3 or r.status!='active'
            or exists(select 1 from task_derivations td
              where td.checklist_item_id=ci.id and td.status='active'
                and (td.project_id!=?2 or td.design_requirement_id!=ci.design_requirement_id
                  or td.task_id!=ci.task_id)))
        "#,
        params![canonical_checklist_id, project_id, design_version_id],
        |row| row.get(0),
    )?;
    if foreign_canonical_rows > 0 {
        bail!("canonical reconciliation checklist contains foreign-design rows");
    }

    let canonical_conflicts: i64 = conn.query_row(
        r#"
        select
          (select count(*) from (
            select ci.design_requirement_id
            from checklist_items ci
            join design_requirements r on r.id=ci.design_requirement_id
            left join task_derivations td on td.checklist_item_id=ci.id and td.status='active'
            where ci.checklist_id=?1 and r.design_version_id=?2 and r.status='active'
            group by ci.design_requirement_id having count(td.id)!=1
          ))
          +
          (select count(*)
           from checklist_items ci
           join task_derivations td on td.checklist_item_id=ci.id and td.status='active'
           join design_requirements r on r.id=ci.design_requirement_id
           join tasks t on t.id=ci.task_id
           where ci.checklist_id=?1 and r.design_version_id=?2 and r.status='active'
             and (not ((ci.status in ('open','blocked') and t.status in ('open','blocked'))
                       or (ci.status='closed' and t.status='closed'))
                  or t.work_unit_id!=?3 or td.task_id!=ci.task_id))
        "#,
        params![canonical_checklist_id, design_version_id, work_unit_id],
        |row| row.get(0),
    )?;
    if canonical_conflicts > 0 {
        bail!("canonical reconciliation checklist contains conflicting requirement bundles");
    }
    let canonical_field_conflicts = {
        let mut stmt = conn.prepare(
            r#"
            select r.requirement_key, r.requirement_text, r.priority,
                   t.title, t.details, t.priority, t.completion_condition,
                   ci.item_order, ci.title, ci.completion_condition,
                   (select count(*)
                    from design_requirements ordered
                    where ordered.design_version_id=r.design_version_id
                      and ordered.status='active'
                      and ordered.requirement_key<=r.requirement_key)
            from checklist_items ci
            join task_derivations td on td.checklist_item_id=ci.id and td.status='active'
            join design_requirements r on r.id=ci.design_requirement_id
            join tasks t on t.id=ci.task_id
            where ci.checklist_id=?1 and r.design_version_id=?2 and r.status='active'
            "#,
        )?;
        let rows = stmt.query_map(params![canonical_checklist_id, design_version_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, i64>(10)?,
            ))
        })?;
        let mut conflicts = 0;
        for row in rows {
            let (
                key,
                text,
                priority,
                task_title,
                details,
                task_priority,
                task_completion,
                item_order,
                item_title,
                item_completion,
                expected_order,
            ) = row?;
            let expected_title = format!("Implement {key}: {}", first_line(&text));
            let expected_completion = format!("Requirement {key} is implemented and validated");
            if task_title != expected_title
                || details.as_deref() != Some(text.as_str())
                || task_priority != priority
                || task_completion.as_deref() != Some(expected_completion.as_str())
                || item_order != expected_order
                || item_title != expected_title
                || item_completion.as_deref() != Some(expected_completion.as_str())
            {
                conflicts += 1;
            }
        }
        conflicts
    };
    if canonical_field_conflicts > 0 {
        bail!("canonical reconciliation checklist failed canonical field validation");
    }
    validate_canonical_gate_sources(conn, canonical_checklist_id, design_version_id)?;
    let semantic_conflicts: i64 = conn.query_row(
        r#"
        select count(*)
        from checklist_items ci
        join task_derivations td on td.checklist_item_id=ci.id and td.status='active'
        join design_requirements r on r.id=ci.design_requirement_id
        join tasks t on t.id=ci.task_id
        where ci.checklist_id=?1 and r.design_version_id=?2 and r.status='active'
          and (
            t.source!='design' or trim(coalesce(t.completion_condition,''))=''
            or trim(coalesce(ci.completion_condition,''))=''
            or td.design_requirement_id!=ci.design_requirement_id or td.task_id!=ci.task_id
            or exists(
              select 1 from validation_gates vg
              left join validation_gate_templates gt on gt.id=vg.template_id
              left join validation_gate_template_requirements gm
                on gm.validation_gate_template_id=gt.id and gm.design_requirement_id=r.id
              where vg.task_id=t.id and vg.design_requirement_id=r.id
                and vg.status='active'
                and (vg.project_id!=?3 or vg.work_unit_id!=?4
                  or vg.id not in (select id from current_task_validation_gates)
                  or gt.design_version_id!=?2 or gt.status!='active' or gm.id is null
                  or trim(gt.gate_hash)='' or trim(gt.stage)='' or trim(gt.gate_text)=''
                  or vg.selected_before_edit!=1
                  or vg.gate_key!=gt.gate_key or vg.command is not gt.command
                  or vg.expected_result!=gt.expected_result)
            )
            or exists(
              select 1 from coverage_items c
              where c.task_id=t.id and c.design_requirement_id=r.id
                and (c.project_id!=?3 or c.work_unit_id!=?4
                  or c.status not in ('covered','needs_evidence'))
            )
            or exists(
              select 1 from coverage_items c
              where c.design_requirement_id=r.id and c.work_unit_id=?4
                and c.status in ('covered','needs_evidence') and c.task_id!=t.id
                and not exists(
                  select 1 from task_derivations duplicate_td
                  where duplicate_td.design_requirement_id=r.id
                    and duplicate_td.task_id=c.task_id and duplicate_td.status='active'
                )
            )
            or exists(
              select 1 from acceptance_records ar
              left join authority_events ae on ae.id=ar.approved_by_authority_event_id
              where (ar.task_id=t.id or ar.checklist_item_id=ci.id
                or ar.validation_gate_id in (select id from validation_gates where task_id=t.id and design_requirement_id=r.id)
                or ar.coverage_item_id in (select id from coverage_items where task_id=t.id and design_requirement_id=r.id))
                and (ar.status!='approved' or ae.id is null or ae.project_id!=?3
                  or ae.status!='active' or ae.event_type not in ('user_instruction','policy','design_doc')
                  or (ae.scope!='project' and ae.scope!='work-unit:'||?4
                    and ae.scope!='requirement:'||r.requirement_key))
            )
          )
        "#,
        params![canonical_checklist_id, design_version_id, project_id, work_unit_id],
        |row| row.get(0),
    )?;
    if semantic_conflicts > 0 {
        bail!("canonical reconciliation checklist failed semantic equivalence validation");
    }
    let gate_cardinality_conflicts: i64 = conn.query_row(
        r#"
        select count(*) from checklist_items ci
        join design_requirements r on r.id=ci.design_requirement_id
        where ci.checklist_id=?1 and r.design_version_id=?2
          and (select count(*) from current_task_validation_gates vg
               where vg.task_id=ci.task_id and vg.design_requirement_id=r.id)
              >
              (select count(*) from validation_gate_template_requirements gm
               join validation_gate_templates gt on gt.id=gm.validation_gate_template_id
               where gm.design_requirement_id=r.id and gt.design_version_id=?2 and gt.status='active')
        "#,
        params![canonical_checklist_id, design_version_id],
        |row| row.get(0),
    )?;
    if gate_cardinality_conflicts > 0 {
        bail!("canonical reconciliation checklist has incomplete validation gate bundles");
    }
    let coverage_cardinality_conflicts: i64 = conn.query_row(
        r#"
        select count(*) from checklist_items ci
        join design_requirements r on r.id=ci.design_requirement_id
        where ci.checklist_id=?1 and r.design_version_id=?2
          and (select count(*) from coverage_items c
               where c.task_id=ci.task_id and c.design_requirement_id=r.id
                 and c.status in ('covered','needs_evidence')) > 1
        "#,
        params![canonical_checklist_id, design_version_id],
        |row| row.get(0),
    )?;
    if coverage_cardinality_conflicts > 0 {
        bail!("canonical reconciliation checklist has conflicting coverage bundles");
    }

    let duplicate_item_ids = {
        let mut stmt = conn.prepare(
            r#"
            select distinct ci.id
            from task_derivations td
            join design_requirements r on r.id=td.design_requirement_id
            join tasks t on t.id=td.task_id
            join checklist_items ci on ci.id=td.checklist_item_id
            where r.design_version_id=?1 and r.status='active' and td.status='active'
              and t.work_unit_id=?2 and ci.checklist_id!=?3
            order by ci.id
            "#,
        )?;
        stmt.query_map(
            params![design_version_id, work_unit_id, canonical_checklist_id],
            |row| row.get::<_, i64>(0),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let (shared_gate_nodes, shared_coverage_nodes): (i64, i64) = conn.query_row(
        r#"
        select
          (select count(*) from checklist_items duplicate_ci
           where duplicate_ci.id in (select value from json_each(?1))
             and exists(select 1 from current_task_validation_gates vg
               where vg.task_id=duplicate_ci.task_id
                 and vg.design_requirement_id=duplicate_ci.design_requirement_id
             )
             and exists(select 1 from task_derivations other_td
               where other_td.task_id=duplicate_ci.task_id
                 and other_td.design_requirement_id=duplicate_ci.design_requirement_id
                 and other_td.checklist_item_id!=duplicate_ci.id
                 and other_td.status='active')),
          (select count(*) from checklist_items duplicate_ci
           where duplicate_ci.id in (select value from json_each(?1))
             and exists(select 1 from coverage_items c
               where c.task_id=duplicate_ci.task_id
                 and c.design_requirement_id=duplicate_ci.design_requirement_id
                 and c.status!='stale')
             and exists(select 1 from task_derivations other_td
               where other_td.task_id=duplicate_ci.task_id
                 and other_td.design_requirement_id=duplicate_ci.design_requirement_id
                 and other_td.checklist_item_id!=duplicate_ci.id
                 and other_td.status='active'))
        "#,
        params![format!(
            "[{}]",
            duplicate_item_ids
                .iter()
                .map(i64::to_string)
                .collect::<Vec<_>>()
                .join(",")
        )],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if shared_gate_nodes > 0 || shared_coverage_nodes > 0 {
        bail!("reconciliation duplicate bundle contains shared gate or coverage nodes");
    }
    for item_id in duplicate_item_ids {
        conn.execute(
            "update validation_gates set status='closed' where project_id=?1 and status='active' and task_id=(select task_id from checklist_items where id=?2) and design_requirement_id=(select design_requirement_id from checklist_items where id=?2)",
            params![project_id, item_id],
        )?;
        conn.execute(
            "update coverage_items set status='stale' where project_id=?1 and status!='stale' and task_id=(select task_id from checklist_items where id=?2) and design_requirement_id=(select design_requirement_id from checklist_items where id=?2)",
            params![project_id, item_id],
        )?;
        conn.execute(
            "update task_derivations set status='closed' where project_id=?1 and checklist_item_id=?2 and status='active'",
            params![project_id, item_id],
        )?;
        conn.execute(
            "update checklist_items set status='closed' where project_id=?1 and id=?2 and status in ('open','blocked')",
            params![project_id, item_id],
        )?;
    }
    conn.execute(
        r#"
        update checklists set status='closed'
        where project_id=?1 and design_version_id=?2 and work_unit_id=?3
          and id!=?4 and status='active'
          and not exists(select 1 from checklist_items ci where ci.checklist_id=checklists.id and ci.status in ('open','blocked'))
        "#,
        params![project_id, design_version_id, work_unit_id, canonical_checklist_id],
    )?;
    let residual: i64 = conn.query_row(
        r#"
        select count(*) from task_derivations td
        join design_requirements r on r.id=td.design_requirement_id
        join tasks t on t.id=td.task_id
        left join checklist_items ci on ci.id=td.checklist_item_id
        where r.design_version_id=?1 and r.status='active' and td.status='active'
          and t.work_unit_id=?2 and coalesce(ci.checklist_id,0)!=?3
        "#,
        params![design_version_id, work_unit_id, canonical_checklist_id],
        |row| row.get(0),
    )?;
    if residual > 0 {
        bail!("reconciliation left residual active current derivations");
    }
    let outcome = decompose_design_with_checklist_in(
        conn,
        project_id,
        DesignDecomposition {
            design_version_id,
            work_unit_id,
            checklist_title: None,
            reason: Some(reason),
        },
        Some(canonical_checklist_id),
        false,
    )?;
    let canonical_items = {
        let mut stmt = conn.prepare(
            "select design_requirement_id,task_id from checklist_items where checklist_id=?1 order by item_order",
        )?;
        stmt.query_map(params![canonical_checklist_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (requirement_id, task_id) in canonical_items {
        ensure_validation_gates_for_task(
            conn,
            project_id,
            design_version_id,
            work_unit_id,
            requirement_id,
            task_id,
        )?;
    }
    let completion_inheritances = inherit_closed_phase_memberships_in(
        conn,
        project_id,
        design_version_id,
        work_unit_id,
        canonical_checklist_id,
    )?;
    retire_historical_decompositions_in(conn, project_id, design_version_id, work_unit_id)?;
    Ok(ReconcileDesignOutcome {
        checklist_id: outcome.checklist_id,
        completion_inheritances,
    })
}

#[derive(Debug, Clone)]
pub(crate) struct ReconcileDesignOutcome {
    pub checklist_id: i64,
    pub completion_inheritances: Vec<CompletionInheritance>,
}

#[derive(Debug, Clone)]
pub(crate) struct CompletionInheritance {
    pub current_requirement_id: i64,
    pub source_requirement_id: i64,
    pub source_design_approval_event_id: i64,
    pub source_task_id: i64,
    pub source_checklist_item_id: i64,
    pub source_membership_id: i64,
    pub source_membership_assigned_at: String,
    pub source_phase_id: i64,
    pub source_phase_closed_event_id: i64,
    pub canonical_task_id: i64,
    pub canonical_checklist_item_id: i64,
    pub implementation_evidence_ids: Vec<i64>,
    pub source_coverage_id: i64,
    pub canonical_coverage_id: i64,
    pub gate_mappings: Vec<(i64, i64, i64)>,
}

struct CompletionSourceDiagnostic<'a> {
    baseline_design: i64,
    current_design: i64,
    current_requirement: i64,
    requirement_key: &'a str,
    revision: i64,
    requirement_hash: &'a str,
    required_surfaces: Option<&'a str>,
    work_unit: i64,
}
fn completion_source_rejection_reason(
    conn: &rusqlite::Connection,
    input: &CompletionSourceDiagnostic<'_>,
) -> Result<Option<String>> {
    conn.query_row(
        r#"
        with candidates as (
          select source_r.revision=?4 and source_r.requirement_hash=?5
                   and source_r.required_surfaces is ?6 as compatible, source_r.revision<?4 as successor_revision,
                 not exists(select 1 from validation_gate_template_requirements cm join validation_gate_templates cg
                   on cg.id=cm.validation_gate_template_id and cg.design_version_id=?7 and cg.status='active' where cm.design_requirement_id=?8
                   and not exists(select 1 from validation_gate_template_requirements sm join validation_gate_templates sg
                     on sg.id=sm.validation_gate_template_id and sg.design_version_id=source_r.design_version_id and sg.status='active'
                     where sm.design_requirement_id=source_r.id and sg.gate_key=cg.gate_key and sg.gate_hash=cg.gate_hash))
                 and not exists(select 1 from validation_gate_template_requirements sm join validation_gate_templates sg
                   on sg.id=sm.validation_gate_template_id and sg.design_version_id=source_r.design_version_id and sg.status='active'
                   where sm.design_requirement_id=source_r.id and not exists(select 1 from validation_gate_template_requirements cm
                     join validation_gate_templates cg on cg.id=cm.validation_gate_template_id and cg.design_version_id=?7 and cg.status='active'
                     where cm.design_requirement_id=?8 and cg.gate_key=sg.gate_key and cg.gate_hash=sg.gate_hash)) as gate_compatible,
                 t.status as task_status, td.status as derivation_status,
                 source_ci.status as checklist_status, p.status as phase_status,
                 p.authority_event_id, p.closed_at, m.assigned_at, p.id as phase_id,
                 t.id as task_id, source_r.id as requirement_id,
                 (select count(*) from task_derivations x where x.design_requirement_id=source_r.id) as derivation_count,
                 (select count(*) from checklist_items x where x.id=td.checklist_item_id and x.task_id=t.id
                    and x.design_requirement_id=source_r.id and x.status='closed') as checklist_count,
                 (select count(*) from work_phase_task_memberships x where x.task_id=t.id) as membership_count,
                 (select count(*) from work_phase_events e where e.phase_id=p.id and e.event_type='closed') as close_event_count,
                 exists(select 1 from work_phase_events e where e.phase_id=p.id and e.event_type='closed'
                    and e.created_at=p.closed_at) as exact_close_event,
                 exists(select 1 from implementation_evidence ie where ie.task_id=t.id
                    and ie.design_requirement_id=source_r.id and ie.created_at<=p.closed_at) as evidence,
                 (select count(*) from coverage_items c where c.task_id=t.id
                    and c.design_requirement_id=source_r.id and c.status='covered' and c.created_at<=p.closed_at) as coverage_count
          from design_requirements source_r join task_derivations td on td.design_requirement_id=source_r.id
          join tasks t on t.id=td.task_id join checklist_items source_ci on source_ci.id=td.checklist_item_id
          join work_phase_task_memberships m on m.task_id=t.id join work_phases p on p.id=m.phase_id
          where source_r.design_version_id=?1 and source_r.requirement_key=?2
            and t.work_unit_id=?3 and p.work_unit_id=?3
        )
        select case
          when count(*)=0 then null
          when max(compatible)=0 and max(successor_revision)=1 then 'successor_revision'
          when max(compatible)=0 then 'revision_compatibility'
          when max(gate_compatible)=0 then 'gate_revision_compatibility'
          when max(task_status='closed' and derivation_status in ('active','closed')
            and checklist_status='closed' and derivation_count=1 and checklist_count=1)=0
            then 'task_checklist_lifecycle'
          when max(phase_status='closed' and authority_event_id is null and closed_at is not null
            and assigned_at<=closed_at and membership_count=1 and close_event_count=1
            and exact_close_event)=0 then 'closed_phase_boundary'
          when max(evidence)=0 then 'implementation_evidence'
          when max(coverage_count=1)=0 then 'coverage'
          else 'gate_compatibility_or_validation'
        end
        from candidates
        "#,
        params![
            input.baseline_design,
            input.requirement_key,
            input.work_unit,
            input.revision,
            input.requirement_hash,
            input.required_surfaces,
            input.current_design,
            input.current_requirement,
        ],
        |row| row.get(0),
    )
    .map_err(Into::into)
}
pub(crate) fn inherit_closed_phase_memberships_in(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_version_id: i64,
    work_unit_id: i64,
    canonical_checklist_id: i64,
) -> Result<Vec<CompletionInheritance>> {
    let mut inheritances = Vec::new();
    let current_package_id: i64 = conn.query_row(
        "select design_package_id from design_versions where id=?1 and project_id=?2",
        params![design_version_id, project_id],
        |row| row.get(0),
    )?;
    let mut stmt = conn.prepare(
        r#"
        select r.id, r.requirement_key, r.revision, r.requirement_hash,
               r.required_surfaces, ci.task_id, ci.id
        from checklist_items ci
        join design_requirements r on r.id=ci.design_requirement_id
        where ci.checklist_id=?1 and r.design_version_id=?2 and r.status='active'
        order by r.requirement_key
        "#,
    )?;
    let rows = stmt
        .query_map(params![canonical_checklist_id, design_version_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    for (current_requirement_id, key, revision, hash, surfaces, canonical_task, canonical_item) in
        rows
    {
        let membership_identity = MembershipIdentity {
            project_id,
            current_requirement_id,
            requirement_key: &key,
            work_unit_id,
            canonical_task_id: canonical_task,
        };
        if collapse_existing_canonical_membership(conn, &membership_identity)? {
            continue;
        }
        let selected_baseline: Option<i64> = conn
            .query_row(
                r#"
                select candidate_v.id
                from design_versions candidate_v
                where candidate_v.design_package_id=?1
                  and candidate_v.version_number<(select version_number from design_versions where id=?2)
                  and candidate_v.status in ('approved','superseded')
                  and candidate_v.approved_by_authority_event_id is not null
                  and candidate_v.approved_at is not null
                  and exists(select 1 from authority_events a
                    where a.id=candidate_v.approved_by_authority_event_id and a.project_id=?4 and a.status='active')
                  and (select count(*) from design_requirements r
                    where r.design_version_id=candidate_v.id and r.requirement_key=?3)=1
                order by candidate_v.version_number desc
                limit 1
                "#,
                params![current_package_id, design_version_id, key, project_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(selected_baseline) = selected_baseline else {
            continue;
        };
        let mut source_stmt = conn.prepare(
            r#"
            select distinct t.id, source_ci.id, m.id, p.id, source_r.id,
                   source_v.approved_by_authority_event_id, m.assigned_at,
                   (select e.id from work_phase_events e where e.phase_id=p.id
                    and e.event_type='closed' and e.created_at=p.closed_at)
            from design_requirements source_r join design_versions source_v on source_v.id=source_r.design_version_id
            join task_derivations td on td.design_requirement_id=source_r.id join tasks t on t.id=td.task_id
            join checklist_items source_ci on source_ci.id=td.checklist_item_id
            join work_phase_task_memberships m on m.task_id=t.id join work_phases p on p.id=m.phase_id
            where source_v.design_package_id=?1 and source_v.id=?10
              and source_v.version_number < (select version_number from design_versions where id=?2)
              and source_v.status in ('approved','superseded') and source_v.approved_by_authority_event_id is not null
              and source_v.approved_at is not null
              and exists(select 1 from authority_events approval where approval.id=source_v.approved_by_authority_event_id
                and approval.project_id=?8 and approval.status='active')
              and source_r.requirement_key=?3
              and (select count(*) from design_requirements same_key where same_key.design_version_id=source_v.id
                and same_key.requirement_key=?3)=1
              and source_r.revision=?4 and source_r.requirement_hash=?5 and source_r.required_surfaces is ?6
              and t.work_unit_id=?7 and t.status='closed' and td.status in ('active','closed')
              and source_ci.status='closed'
              and p.project_id=?8 and p.work_unit_id=?7 and p.status='closed'
              and p.authority_event_id is null and p.closed_at is not null and m.assigned_at<=p.closed_at
              and (select count(*) from task_derivations exact_td where exact_td.design_requirement_id=source_r.id)=1
              and (select count(*) from checklist_items exact_ci where exact_ci.id=td.checklist_item_id
                and exact_ci.task_id=t.id and exact_ci.design_requirement_id=source_r.id and exact_ci.status='closed')=1
              and (select count(*) from work_phase_task_memberships exact_m where exact_m.task_id=t.id)=1
              and (select count(*) from work_phase_events e where e.phase_id=p.id and e.event_type='closed')=1
              and exists(select 1 from work_phase_events e where e.phase_id=p.id
                and e.event_type='closed' and e.created_at=p.closed_at)
              and exists(select 1 from implementation_evidence ie where ie.task_id=t.id
                and ie.design_requirement_id=source_r.id and ie.created_at<=p.closed_at)
              and (select count(*) from coverage_items source_c where source_c.task_id=t.id
                and source_c.design_requirement_id=source_r.id and source_c.status='covered'
                and source_c.created_at<=p.closed_at)=1
              and not exists(select 1 from validation_gate_template_requirements current_map
                join validation_gate_templates current_gt on current_gt.id=current_map.validation_gate_template_id
                  and current_gt.design_version_id=?2 and current_gt.status='active'
                where current_map.design_requirement_id=?9 and not exists(
                  select 1 from validation_gate_template_requirements source_map
                    join validation_gate_templates source_gt on source_gt.id=source_map.validation_gate_template_id
                      and source_gt.design_version_id=source_r.design_version_id and source_gt.status='active'
                    where source_map.design_requirement_id=source_r.id
                      and source_gt.gate_key=current_gt.gate_key and source_gt.gate_hash=current_gt.gate_hash))
              and not exists(select 1 from validation_gate_template_requirements source_map
                join validation_gate_templates source_gt on source_gt.id=source_map.validation_gate_template_id
                  and source_gt.design_version_id=source_r.design_version_id and source_gt.status='active'
                where source_map.design_requirement_id=source_r.id and not exists(
                  select 1 from validation_gate_template_requirements current_map
                    join validation_gate_templates current_gt on current_gt.id=current_map.validation_gate_template_id
                      and current_gt.design_version_id=?2 and current_gt.status='active'
                    where current_map.design_requirement_id=?9
                      and current_gt.gate_key=source_gt.gate_key and current_gt.gate_hash=source_gt.gate_hash))
              and not exists(select 1 from validation_gates source_gate
                join validation_gate_templates source_gt on source_gt.id=source_gate.template_id
                where source_gate.task_id=t.id and source_gate.design_requirement_id=source_r.id
                  and source_gate.selected_before_edit=1 and not exists(select 1 from validation_runs latest
                    where latest.id=(select max(candidate.id) from validation_runs candidate
                      where candidate.validation_gate_id=source_gate.id and candidate.created_at<=p.closed_at)
                      and latest.result='pass' and (source_gate.command is null or
                        (latest.command is source_gate.command and latest.command_usage_id is not null
                         and exists(select 1 from command_usages usage
                          where usage.id=latest.command_usage_id and usage.project_id=?8
                            and usage.work_unit_id=?7 and usage.result='pass' and usage.command=source_gate.command)))))
            order by source_v.version_number desc, t.id
            "#,
        )?;
        let sources = source_stmt
            .query_map(
                params![
                    current_package_id,
                    design_version_id,
                    key,
                    revision,
                    hash,
                    surfaces,
                    work_unit_id,
                    project_id,
                    current_requirement_id,
                    selected_baseline
                ],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                        row.get::<_, String>(6)?,
                        row.get::<_, i64>(7)?,
                    ))
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(source_stmt);
        if sources.is_empty() {
            let diagnostic = CompletionSourceDiagnostic {
                baseline_design: selected_baseline,
                current_design: design_version_id,
                current_requirement: current_requirement_id,
                requirement_key: &key,
                revision,
                requirement_hash: &hash,
                required_surfaces: surfaces.as_deref(),
                work_unit: work_unit_id,
            };
            let rejection = completion_source_rejection_reason(conn, &diagnostic)?;
            if matches!(
                rejection.as_deref(),
                Some("successor_revision" | "gate_revision_compatibility")
            ) {
                migrate_incompatible_membership(conn, selected_baseline, &membership_identity)?;
                continue;
            }
            if let Some(reason) = rejection {
                bail!("closed-phase completion source rejected for {key}: {reason}");
            }
            continue;
        }
        if sources.len() != 1 {
            bail!("closed-phase completion inheritance is ambiguous for {key}");
        }
        let (
            source_task,
            source_item,
            source_membership,
            phase_id,
            source_requirement,
            source_approval,
            source_assigned_at,
            source_close_event,
        ) = sources[0].clone();
        let implementation_evidence_ids = {
            let mut evidence = conn.prepare(
                "select id from implementation_evidence where task_id=?1 and design_requirement_id=?2 and created_at<=(select closed_at from work_phases where id=?3) order by id",
            )?;
            evidence
                .query_map(params![source_task, source_requirement, phase_id], |row| {
                    row.get::<_, i64>(0)
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let source_coverage_id: i64 = conn.query_row(
            "select id from coverage_items where task_id=?1 and design_requirement_id=?2 and status='covered' and created_at<=(select closed_at from work_phases where id=?3)",
            params![source_task, source_requirement, phase_id],
            |row| row.get(0),
        )?;
        conn.execute(
            r#"insert into coverage_items(project_id,work_unit_id,design_requirement_id,task_id,
                   requirement,lifecycle_boundary_evidence,tests_or_gates,missing_or_unverified,status,created_at)
               select ?1,?2,?3,?4,requirement_text,'completion inheritance pending',
                   validation_expectation,'source completion mapping pending','needs_evidence',current_timestamp
               from design_requirements where id=?3
                 and not exists(select 1 from coverage_items where task_id=?4 and design_requirement_id=?3)"#,
            params![project_id,work_unit_id,current_requirement_id,canonical_task],
        )?;
        let canonical_coverage_id: i64 = conn.query_row(
            "select id from coverage_items where task_id=?1 and design_requirement_id=?2 and status in ('covered','needs_evidence')",
            params![canonical_task, current_requirement_id],
            |row| row.get(0),
        )?;
        let gate_mappings = {
            let mut gates = conn.prepare(
                r#"select source_gate.id, current_gate.id,
                           (select max(vr.id) from validation_runs vr
                            where vr.validation_gate_id=source_gate.id
                              and vr.created_at<=(select closed_at from work_phases where id=?5))
                    from validation_gates source_gate
                    join validation_gate_templates source_gt on source_gt.id=source_gate.template_id
                    join validation_gates current_gate on current_gate.task_id=?1
                      and current_gate.design_requirement_id=?2 and current_gate.status='active'
                    join validation_gate_templates current_gt on current_gt.id=current_gate.template_id
                      and current_gt.gate_key=source_gt.gate_key and current_gt.gate_hash=source_gt.gate_hash
                    where source_gate.task_id=?3 and source_gate.design_requirement_id=?4
                      and source_gate.selected_before_edit=1 order by source_gate.id"#,
            )?;
            gates
                .query_map(
                    params![
                        canonical_task,
                        current_requirement_id,
                        source_task,
                        source_requirement,
                        phase_id
                    ],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                        ))
                    },
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        replace_historical_memberships(conn, &membership_identity, phase_id)?;
        conn.execute(
            "update tasks set status='closed' where id=?1 and status in ('open','blocked')",
            params![canonical_task],
        )?;
        conn.execute(
            "update checklist_items set status='closed' where id=?1 and status in ('open','blocked')",
            params![canonical_item],
        )?;
        conn.execute(
            "update validation_gates set status='closed' where task_id=?1 and design_requirement_id=?2 and status='active'",
            params![canonical_task, current_requirement_id],
        )?;
        conn.execute(
            "update coverage_items set status='covered', missing_or_unverified=null where task_id=?1 and design_requirement_id=?2 and status='needs_evidence'",
            params![canonical_task, current_requirement_id],
        )?;
        inheritances.push(CompletionInheritance {
            current_requirement_id,
            source_requirement_id: source_requirement,
            source_design_approval_event_id: source_approval,
            source_task_id: source_task,
            source_checklist_item_id: source_item,
            source_membership_id: source_membership,
            source_membership_assigned_at: source_assigned_at,
            source_phase_id: phase_id,
            source_phase_closed_event_id: source_close_event,
            canonical_task_id: canonical_task,
            canonical_checklist_item_id: canonical_item,
            implementation_evidence_ids,
            source_coverage_id,
            canonical_coverage_id,
            gate_mappings,
        });
    }
    Ok(inheritances)
}

#[derive(Deserialize)]
pub(super) struct ReconciledGateMetadata {
    #[serde(rename = "type")]
    record_type: String,
    key: String,
    phase: String,
    expected_result: String,
    #[serde(default)]
    applies_to: Vec<String>,
    #[serde(default)]
    command_template: Option<String>,
    status: String,
}

pub(super) fn validate_canonical_gate_sources(
    conn: &rusqlite::Connection,
    canonical_checklist_id: i64,
    design_version_id: i64,
) -> Result<()> {
    let mut stmt = conn.prepare(
        r#"
        select distinct dv.package_path, df.relative_path, gt.source_section,
               gt.gate_key, gt.gate_hash, gt.stage, gt.command,
               gt.expected_result, gt.requirement_keys, gt.gate_text, gt.status
        from checklist_items ci
        join current_task_validation_gates vg on vg.task_id=ci.task_id
          and vg.design_requirement_id=ci.design_requirement_id
        join validation_gate_templates gt on gt.id=vg.template_id
        join design_files df on df.id=gt.source_design_file_id
        join design_versions dv on dv.id=gt.design_version_id
        where ci.checklist_id=?1 and gt.design_version_id=?2
        "#,
    )?;
    let rows = stmt.query_map(params![canonical_checklist_id, design_version_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, String>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, String>(9)?,
            row.get::<_, String>(10)?,
        ))
    })?;
    for row in rows {
        let (
            package_path,
            relative_path,
            source_section,
            gate_key,
            gate_hash,
            stage,
            command,
            expected_result,
            requirement_keys,
            gate_text,
            status,
        ) = row?;
        let content = fs::read_to_string(Path::new(&package_path).join(relative_path))
            .context("canonical gate source file is unavailable")?;
        let (metadata_text, body) = agent_block_source(&content, &source_section)
            .context("canonical gate source block is unavailable")?;
        let metadata: ReconciledGateMetadata = yaml_serde::from_str(&metadata_text)
            .context("canonical gate source metadata is invalid")?;
        let normalized_stage = match metadata.phase.as_str() {
            "design" => "design-ready",
            "implementation" => "implementation-ready",
            "close" => "close-ready",
            "resume" => "resume-ready",
            other => other,
        };
        let mut hasher = Sha256::new();
        hasher.update(metadata_text.as_bytes());
        hasher.update(b"\0");
        hasher.update(body.trim().as_bytes());
        let source_hash = format!("{:x}", hasher.finalize());
        let source_requirements = if metadata.applies_to.is_empty() {
            None
        } else {
            Some(metadata.applies_to.join(","))
        };
        if metadata.record_type != "validation_gate_template"
            || metadata.key != gate_key
            || source_hash != gate_hash
            || normalized_stage != stage
            || metadata.command_template != command
            || metadata.expected_result != expected_result
            || source_requirements != requirement_keys
            || body.trim() != gate_text
            || metadata.status != status
        {
            bail!("canonical reconciliation gate differs from imported design source");
        }
    }
    Ok(())
}
pub(super) fn agent_block_source(content: &str, source_section: &str) -> Option<(String, String)> {
    let lines: Vec<&str> = content.lines().collect();
    let heading = format!("## {source_section}");
    let index = lines.iter().position(|line| line.trim() == heading)?;
    let fence_start = index + 1;
    if lines.get(fence_start)?.trim() != "```yaml agent-workbench" {
        return None;
    }
    let fence_end = (fence_start + 1..lines.len()).find(|&i| lines[i].trim() == "```")?;
    let body_end = (fence_end + 1..lines.len())
        .find(|&i| lines[i].starts_with("## "))
        .unwrap_or(lines.len());
    Some((
        lines[fence_start + 1..fence_end].join("\n"),
        lines[fence_end + 1..body_end].join("\n"),
    ))
}

pub(super) fn validation_gate_templates_for_requirement(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_version_id: i64,
    design_requirement_id: i64,
) -> Result<Vec<ResolvedGateTemplate>> {
    let mut stmt = conn.prepare(
        r#"
        select g.id, g.gate_key, g.command, g.expected_result
        from validation_gate_templates g
        join validation_gate_template_requirements gr
          on gr.validation_gate_template_id = g.id
        where g.project_id = ?1
          and g.design_version_id = ?2
          and gr.design_requirement_id = ?3
          and g.status = 'active'
        order by g.id
        "#,
    )?;
    let rows = stmt.query_map(
        params![project_id, design_version_id, design_requirement_id],
        |row| {
            Ok(ResolvedGateTemplate {
                id: row.get(0)?,
                gate_key: row.get(1)?,
                command: row.get(2)?,
                expected_result: row.get(3)?,
            })
        },
    )?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}
