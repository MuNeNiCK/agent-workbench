use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use super::decode_opaque_task_ref;

pub(super) fn record_completion_inheritances(
    conn: &rusqlite::Connection,
    project_id: i64,
    session_id: i64,
    application_id: i64,
    inheritances: &[crate::traceability::CompletionInheritance],
) -> Result<()> {
    for inheritance in inheritances {
        conn.execute(
            r#"insert into correction_completion_inheritance_sources(
                project_id,correction_session_id,correction_application_id,
                current_requirement_id,source_requirement_id,source_design_approval_event_id,
                source_task_id,source_checklist_item_id,source_membership_id,
                source_membership_assigned_at,source_phase_id,source_phase_closed_event_id,
                canonical_task_id,canonical_checklist_item_id,created_at
            ) values (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,current_timestamp)"#,
            params![
                project_id,
                session_id,
                application_id,
                inheritance.current_requirement_id,
                inheritance.source_requirement_id,
                inheritance.source_design_approval_event_id,
                inheritance.source_task_id,
                inheritance.source_checklist_item_id,
                inheritance.source_membership_id,
                inheritance.source_membership_assigned_at,
                inheritance.source_phase_id,
                inheritance.source_phase_closed_event_id,
                inheritance.canonical_task_id,
                inheritance.canonical_checklist_item_id
            ],
        )?;
        let source_id = conn.last_insert_rowid();
        for evidence_id in &inheritance.implementation_evidence_ids {
            conn.execute(
                "insert into correction_completion_inheritance_evidence(project_id,inheritance_source_id,evidence_kind,source_record_id,canonical_record_id,validation_run_id,created_at) values (?1,?2,'implementation_evidence',?3,null,null,current_timestamp)",
                params![project_id,source_id,evidence_id],
            )
            .with_context(|| {
                format!(
                    "completion inheritance implementation evidence rejected: requirement={} evidence={evidence_id}",
                    inheritance.current_requirement_id
                )
            })?;
        }
        conn.execute(
            "insert into correction_completion_inheritance_evidence(project_id,inheritance_source_id,evidence_kind,source_record_id,canonical_record_id,validation_run_id,created_at) values (?1,?2,'coverage_item',?3,?4,null,current_timestamp)",
            params![project_id,source_id,inheritance.source_coverage_id,inheritance.canonical_coverage_id],
        )
        .with_context(|| {
            format!(
                "completion inheritance coverage rejected: requirement={} source={} canonical={}",
                inheritance.current_requirement_id,
                inheritance.source_coverage_id,
                inheritance.canonical_coverage_id
            )
        })?;
        for (source_gate, canonical_gate, validation_run) in &inheritance.gate_mappings {
            conn.execute(
                "insert into correction_completion_inheritance_evidence(project_id,inheritance_source_id,evidence_kind,source_record_id,canonical_record_id,validation_run_id,created_at) values (?1,?2,'validation_gate',?3,?4,?5,current_timestamp)",
                params![project_id,source_id,source_gate,canonical_gate,validation_run],
            )
            .with_context(|| {
                format!(
                    "completion inheritance validation gate rejected: requirement={} source_gate={source_gate} canonical_gate={canonical_gate} validation_run={validation_run}",
                    inheritance.current_requirement_id
                )
            })?;
        }
    }
    Ok(())
}

pub(super) fn validate_completion_inheritance_application(
    conn: &rusqlite::Connection,
    application_id: i64,
) -> Result<()> {
    let mut stmt = conn.prepare(
        "select id,source_task_id,source_requirement_id,source_phase_id,current_requirement_id,canonical_task_id from correction_completion_inheritance_sources where correction_application_id=?1",
    )?;
    let rows = stmt
        .query_map(params![application_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    for (
        source_id,
        source_task,
        source_requirement,
        source_phase,
        current_requirement,
        canonical_task,
    ) in rows
    {
        let currently_valid: bool = conn.query_row(
            "select exists(select 1 from valid_completion_inheritance_sources where id=?1)",
            params![source_id],
            |row| row.get(0),
        )?;
        let (mapped_implementation, eligible_implementation, mapped_coverage, mapped_gates, canonical_gates): (i64,i64,i64,i64,i64) = conn.query_row(
            r#"select
                (select count(*) from correction_completion_inheritance_evidence where inheritance_source_id=?1 and evidence_kind='implementation_evidence'),
                (select count(*) from implementation_evidence where task_id=?2 and design_requirement_id=?3 and created_at<=(select closed_at from work_phases where id=?4)),
                (select count(*) from correction_completion_inheritance_evidence where inheritance_source_id=?1 and evidence_kind='coverage_item'),
                (select count(*) from correction_completion_inheritance_evidence where inheritance_source_id=?1 and evidence_kind='validation_gate'),
                (select count(*) from validation_gates where task_id=?5 and design_requirement_id=?6 and selected_before_edit=1 and status='closed')"#,
            params![source_id,source_task,source_requirement,source_phase,canonical_task,current_requirement],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)),
        )?;
        if !currently_valid
            || eligible_implementation == 0
            || mapped_implementation != eligible_implementation
            || mapped_coverage != 1
            || mapped_gates != canonical_gates
        {
            bail!("completion inheritance evidence set is incomplete");
        }
    }
    Ok(())
}

pub(crate) fn transition_state_snapshot(
    conn: &rusqlite::Connection,
    work_unit_id: i64,
) -> Result<String> {
    let state: (String, String, String, String, String, String, String, String, String, String) = conn.query_row(
        r#"
        select
          coalesce((select group_concat(v,'|') from (select id||':'||status v from tasks where work_unit_id=?1 order by id)),''),
          coalesce((select group_concat(v,'|') from (select m.id||':'||m.phase_id||':'||m.task_id||':'||m.assigned_at v from work_phase_task_memberships m join work_phases p on p.id=m.phase_id where p.work_unit_id=?1 order by m.id)),''),
          coalesce((select group_concat(v,'|') from (select id||':'||status v from checklists where work_unit_id=?1 order by id)),''),
          coalesce((select group_concat(v,'|') from (select td.id||':'||td.status v from task_derivations td join tasks t on t.id=td.task_id where t.work_unit_id=?1 order by td.id)),''),
          coalesce((select group_concat(v,'|') from (select ci.id||':'||ci.status v from checklist_items ci join tasks t on t.id=ci.task_id where t.work_unit_id=?1 order by ci.id)),''),
          coalesce((select group_concat(v,'|') from (select id||':'||status v from validation_gates where work_unit_id=?1 order by id)),''),
          coalesce((select group_concat(v,'|') from (select id||':'||status v from coverage_items where work_unit_id=?1 order by id)),''),
          coalesce((select group_concat(v,'|') from (select id||':'||status v from work_phases where work_unit_id=?1 order by id)),''),
          coalesce((select group_concat(v,'|') from (select d.id||':'||d.status||':'||coalesce(d.evidence_ref,'')||':'||coalesce(d.authority_event_id,0) v from work_phase_dependencies d join work_phases p on p.id=d.from_phase_id where p.work_unit_id=?1 order by d.id)),''),
          coalesce((select group_concat(v,'|') from (select ar.id||':'||ar.target_type||':'||coalesce(ar.task_id,ar.checklist_item_id,ar.validation_gate_id,ar.coverage_item_id,ar.stale_record_id,0)||':'||ar.status||':'||coalesce(ar.approved_by_authority_event_id,0) v from acceptance_records ar left join tasks t on t.id=ar.task_id left join checklist_items ci on ci.id=ar.checklist_item_id left join validation_gates vg on vg.id=ar.validation_gate_id left join coverage_items c on c.id=ar.coverage_item_id where coalesce(t.work_unit_id,vg.work_unit_id,c.work_unit_id,(select work_unit_id from checklists where id=ci.checklist_id),?1)=?1 order by ar.id)),'')
        "#,
        params![work_unit_id],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?,row.get(9)?)),
    )?;
    Ok(format!(
        "tasks=[{}];memberships=[{}];checklists=[{}];derivations=[{}];items=[{}];gates=[{}];coverage=[{}];phases=[{}];phase_dependencies=[{}];acceptances=[{}]",
        state.0, state.1, state.2, state.3, state.4, state.5, state.6, state.7, state.8, state.9
    ))
}

pub(super) fn ensure_mediated_decomposition_coverage(
    conn: &rusqlite::Connection,
    project_id: i64,
    work_unit_id: i64,
    design_version_id: i64,
) -> Result<()> {
    conn.execute(
        r#"
        insert into coverage_items(
            project_id, work_unit_id, design_requirement_id, task_id,
            requirement, lifecycle_boundary_evidence, tests_or_gates,
            missing_or_unverified, status, created_at
        )
        select
            ?1, ?2, r.id, t.id, r.requirement_text,
            'generated by mediated decomposition; implementation evidence pending',
            'selected validation gates pending',
            'implementation and validation evidence required',
            'needs_evidence', current_timestamp
        from task_derivations td
        join tasks t on t.id = td.task_id
        join design_requirements r on r.id = td.design_requirement_id
        where t.work_unit_id = ?2 and r.design_version_id = ?3
          and not exists (
              select 1 from coverage_items c
              where c.task_id = t.id and c.design_requirement_id = r.id
          )
        "#,
        params![project_id, work_unit_id, design_version_id],
    )?;
    Ok(())
}

pub(super) fn record_correction_transition_aliases(
    conn: &rusqlite::Connection,
    project_id: i64,
    session_id: i64,
    application_id: i64,
    operation: &str,
    target: &str,
    result_ref: &str,
) -> Result<()> {
    let mut aliases = Vec::<(String, String, i64)>::new();
    match operation {
        "design-decompose" | "design-reconcile" => {
            let checklist_id = result_ref
                .strip_prefix("checklist:")
                .context("invalid decomposition application result")?
                .parse::<i64>()?;
            aliases.push((
                "@checklist".to_string(),
                "checklist".to_string(),
                checklist_id,
            ));
            let mut stmt = conn.prepare(
                r#"
                select r.requirement_key, ci.id, ci.task_id, td.id, c.id
                from checklist_items ci
                join design_requirements r on r.id = ci.design_requirement_id
                join task_derivations td on td.checklist_item_id = ci.id and td.status='active'
                join coverage_items c on c.task_id = ci.task_id and c.design_requirement_id = r.id
                  and c.status in ('covered','needs_evidence')
                where ci.checklist_id = ?1
                order by r.requirement_key
                "#,
            )?;
            let rows = stmt.query_map(params![checklist_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })?;
            for row in rows {
                let (key, item_id, task_id, derivation_id, coverage_id) = row?;
                aliases.push((format!("@task/{key}"), "task".to_string(), task_id));
                aliases.push((
                    format!("@derivation/{key}"),
                    "task_derivation".to_string(),
                    derivation_id,
                ));
                aliases.push((
                    format!("@checklist-item/{key}"),
                    "checklist_item".to_string(),
                    item_id,
                ));
                aliases.push((
                    format!("@coverage/{key}"),
                    "coverage_item".to_string(),
                    coverage_id,
                ));
                let mut gates = conn.prepare(
                    r#"
                    select vg.id, vg.gate_key
                    from validation_gates vg
                    join validation_gate_templates gt on gt.id=vg.template_id and gt.status='active'
                    join checklist_items ci
                      on ci.task_id = vg.task_id
                     and ci.design_requirement_id = vg.design_requirement_id
                    join checklists checklist on checklist.id=ci.checklist_id
                    where ci.id = ?1 and vg.status='active'
                      and gt.design_version_id=checklist.design_version_id
                    order by vg.id
                    "#,
                )?;
                let gate_rows = gates.query_map(params![item_id], |gate| {
                    Ok((gate.get::<_, i64>(0)?, gate.get::<_, String>(1)?))
                })?;
                for gate in gate_rows {
                    let (gate_id, gate_key) = gate?;
                    aliases.push((
                        format!("@gate/{key}/{gate_key}"),
                        "validation_gate".to_string(),
                        gate_id,
                    ));
                }
            }
            if operation == "design-reconcile" {
                let parts = target.split('/').collect::<Vec<_>>();
                let design = parts[0].parse::<i64>()?;
                let work = parts[1].parse::<i64>()?;
                let mut rejected = conn.prepare(
                    r#"
                    select distinct t.id
                    from task_derivations td
                    join design_requirements r on r.id=td.design_requirement_id
                    join tasks t on t.id=td.task_id
                    join checklist_items ci on ci.id=td.checklist_item_id
                    join correction_transition_applications app on app.id=?4
                    where r.design_version_id=?1 and t.work_unit_id=?2
                      and ci.checklist_id!=?3 and td.status='closed'
                      and ('|'||substr(app.before_state,
                        instr(app.before_state,'derivations=[')+13,
                        instr(app.before_state,'];items=')-(instr(app.before_state,'derivations=[')+13)
                      )||'|') like '%|'||td.id||':active|%'
                    order by t.id
                    "#,
                )?;
                let rejected_rows = rejected
                    .query_map(params![design, work, checklist_id, application_id], |row| {
                        row.get::<_, i64>(0)
                    })?;
                for rejected_task in rejected_rows {
                    let rejected_task = rejected_task?;
                    aliases.push((
                        format!("@superseded-task/{rejected_task}"),
                        "task".to_string(),
                        rejected_task,
                    ));
                }
            }
        }
        "phase-create" => {
            let phase_id = result_ref
                .strip_prefix("phase:")
                .context("invalid phase application result")?
                .parse::<i64>()?;
            let alias = target
                .split('/')
                .nth(2)
                .context("phase-create target has no alias")?;
            aliases.push((alias.to_string(), "phase".to_string(), phase_id));
        }
        "task-accept-out-of-scope" => {
            let parts = result_ref.split(':').collect::<Vec<_>>();
            let task_id = parts
                .get(1)
                .context("invalid task acceptance application result")?
                .parse::<i64>()?;
            aliases.push((
                format!("@accepted-task/{task_id}"),
                "task".to_string(),
                task_id,
            ));
            let (before_state, after_state): (String, String) = conn.query_row(
                "select before_state, after_state from correction_transition_applications where id=?1",
                params![application_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            let mut stmt = conn.prepare(
                r#"
                select 'checklist_item', ci.id from checklist_items ci where ci.task_id=?1
                union all select 'validation_gate', vg.id from validation_gates vg where vg.task_id=?1
                union all select 'coverage_item', c.id from coverage_items c where c.task_id=?1
                "#,
            )?;
            let rows = stmt.query_map(params![task_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (record_type, record_id) = row?;
                let section = match record_type.as_str() {
                    "checklist_item" => "items",
                    "validation_gate" => "gates",
                    "coverage_item" => "coverage",
                    _ => continue,
                };
                let before = snapshot_entries(&before_state, section);
                let after = snapshot_entries(&after_state, section);
                if before.get(&record_id) == after.get(&record_id) {
                    continue;
                }
                aliases.push((
                    format!("@accepted-{record_type}/{record_id}"),
                    record_type,
                    record_id,
                ));
            }
        }
        "phase-dependency-add" => {
            let dependency_id = result_ref
                .strip_prefix("phase-dependency:")
                .context("invalid dependency application result")?
                .parse::<i64>()?;
            aliases.push((
                format!("@dependency/{dependency_id}"),
                "phase_dependency".to_string(),
                dependency_id,
            ));
        }
        _ => {}
    }
    for (alias, record_type, record_id) in &aliases {
        conn.execute(
            r#"
            insert into correction_transition_aliases(
                project_id, correction_session_id, correction_application_id,
                alias, record_type, record_id, created_at
            ) values (?1, ?2, ?3, ?4, ?5, ?6, current_timestamp)
            "#,
            params![
                project_id,
                session_id,
                application_id,
                alias,
                record_type,
                record_id
            ],
        )?;
    }
    record_correction_application_identity_links(
        conn,
        project_id,
        session_id,
        application_id,
        operation,
        &aliases,
    )?;
    Ok(())
}

pub(super) fn record_correction_application_identity_links(
    conn: &rusqlite::Connection,
    project_id: i64,
    session_id: i64,
    application_id: i64,
    operation: &str,
    aliases: &[(String, String, i64)],
) -> Result<()> {
    let (before_state, after_state): (String, String) = conn.query_row(
        "select before_state, after_state from correction_transition_applications where id=?1",
        params![application_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let section_for = |record_type: &str| match record_type {
        "task" => Some("tasks"),
        "checklist" => Some("checklists"),
        "task_derivation" => Some("derivations"),
        "checklist_item" => Some("items"),
        "validation_gate" => Some("gates"),
        "coverage_item" => Some("coverage"),
        "phase" => Some("phases"),
        "phase_dependency" => Some("phase_dependencies"),
        "acceptance_record" => Some("acceptances"),
        _ => None,
    };
    let mut links = std::collections::BTreeSet::<(String, String, i64)>::new();
    for (alias, record_type, record_id) in aliases {
        let existed = section_for(record_type).is_some_and(|section| {
            snapshot_entries(&before_state, section).contains_key(record_id)
        });
        links.insert((
            if alias.starts_with("@superseded-task/") {
                "superseded"
            } else if alias.starts_with("@accepted-") {
                "updated"
            } else if existed {
                "adopted"
            } else {
                "created"
            }
            .to_string(),
            record_type.clone(),
            *record_id,
        ));
    }
    if operation == "design-reconcile" {
        for (section, record_type) in [
            ("checklists", "checklist"),
            ("derivations", "task_derivation"),
            ("items", "checklist_item"),
            ("gates", "validation_gate"),
            ("coverage", "coverage_item"),
        ] {
            let before = snapshot_entries(&before_state, section);
            let after = snapshot_entries(&after_state, section);
            for (record_id, before_value) in &before {
                if after
                    .get(record_id)
                    .is_some_and(|after_value| after_value != before_value)
                {
                    links.insert((
                        "superseded".to_string(),
                        record_type.to_string(),
                        *record_id,
                    ));
                }
            }
        }
        let mut inherited = conn.prepare(
            "select canonical_task_id,canonical_checklist_item_id from correction_completion_inheritance_sources where correction_application_id=?1",
        )?;
        let inherited_rows = inherited.query_map(params![application_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        for inherited_row in inherited_rows {
            let (task_id, item_id) = inherited_row?;
            links.retain(|(_, record_type, record_id)| {
                !((record_type == "task" && *record_id == task_id)
                    || (record_type == "checklist_item" && *record_id == item_id))
            });
            links.insert(("updated".to_string(), "task".to_string(), task_id));
            links.insert(("updated".to_string(), "checklist_item".to_string(), item_id));
        }
        let mut sources = conn.prepare(
            "select source_task_id,source_phase_id from correction_completion_inheritance_sources where correction_application_id=?1",
        )?;
        let source_rows = sources.query_map(params![application_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        for source_row in source_rows {
            let (task_id, phase_id) = source_row?;
            links.insert(("completion_source".to_string(), "task".to_string(), task_id));
            links.insert((
                "completion_source".to_string(),
                "phase".to_string(),
                phase_id,
            ));
        }
        let mut mapped = conn.prepare(
            "select evidence_kind,canonical_record_id from correction_completion_inheritance_evidence evidence join correction_completion_inheritance_sources source on source.id=evidence.inheritance_source_id where source.correction_application_id=?1 and canonical_record_id is not null",
        )?;
        let mapped_rows = mapped.query_map(params![application_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for mapped_row in mapped_rows {
            let (kind, record_id) = mapped_row?;
            let record_type = if kind == "validation_gate" {
                "validation_gate"
            } else {
                "coverage_item"
            };
            links.retain(|(_, existing_type, existing_id)| {
                !(existing_type == record_type && *existing_id == record_id)
            });
            links.insert(("updated".to_string(), record_type.to_string(), record_id));
        }
    }
    let before_memberships = snapshot_entries(&before_state, "memberships");
    let after_memberships = snapshot_entries(&after_state, "memberships");
    for record_id in before_memberships.keys() {
        if !after_memberships.contains_key(record_id) {
            links.insert((
                "membership_removed".to_string(),
                "phase_membership".to_string(),
                *record_id,
            ));
        }
    }
    for record_id in after_memberships.keys() {
        if !before_memberships.contains_key(record_id) {
            links.insert((
                "membership_assigned".to_string(),
                "phase_membership".to_string(),
                *record_id,
            ));
        }
    }
    for (link_kind, record_type, record_id) in links {
        conn.execute(
            r#"
            insert into correction_application_identity_links(
                project_id, correction_session_id, correction_application_id,
                link_kind, record_type, record_id, created_at
            ) values (?1, ?2, ?3, ?4, ?5, ?6, current_timestamp)
            "#,
            params![
                project_id,
                session_id,
                application_id,
                link_kind,
                record_type,
                record_id
            ],
        )?;
    }
    Ok(())
}

pub(super) fn snapshot_entries(
    snapshot: &str,
    section: &str,
) -> std::collections::BTreeMap<i64, String> {
    let prefix = format!("{section}=[");
    let Some(start) = snapshot.find(&prefix).map(|index| index + prefix.len()) else {
        return std::collections::BTreeMap::new();
    };
    let Some(end) = snapshot[start..].find(']').map(|offset| start + offset) else {
        return std::collections::BTreeMap::new();
    };
    snapshot[start..end]
        .split('|')
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| {
            let (id, value) = entry.split_once(':')?;
            Some((id.parse().ok()?, value.to_string()))
        })
        .collect()
}

pub(super) fn parse_pair(target: &str) -> Result<(i64, i64)> {
    let (left, right) = target.split_once('/').context("target requires two ids")?;
    Ok((left.parse()?, right.parse()?))
}

pub(super) fn resolve_task_ref(
    conn: &rusqlite::Connection,
    session_id: i64,
    token_ordinal: i64,
    work_unit_id: i64,
    design_version_id: Option<i64>,
    value: &str,
) -> Result<(i64, i64)> {
    let numeric_id = value.parse::<i64>().ok();
    let opaque_key = decode_opaque_task_ref(value)?;
    if numeric_id.is_none() && opaque_key.is_none() {
        bail!("invalid task reference");
    }
    if let Some(task_id) = numeric_id {
        let mut stmt = conn.prepare(
                r#"
                select distinct t.id, r.id from tasks t
                join task_derivations td on td.task_id = t.id
                join design_requirements r on r.id = td.design_requirement_id
                join design_versions v on v.id=r.design_version_id
                join design_versions current_v on current_v.id=?3
                join design_requirements current_r on current_r.design_version_id=current_v.id
                  and current_r.requirement_key=r.requirement_key
                where t.id = ?1 and t.work_unit_id = ?2
                  and v.design_package_id=current_v.design_package_id
                  and (
                    td.status='active'
                    or (td.status='stale' and exists(
                      select 1 from acceptance_records ar
                      join correction_transition_applications stale_app
                        on stale_app.correction_session_id=?4
                      join correction_tokens stale_token
                        on stale_token.id=stale_app.correction_token_id
                      where ar.target_type='stale_record'
                        and ar.stale_record_type='task_derivation' and ar.stale_record_id=td.id
                        and ar.status='approved'
                        and stale_token.operation='stale-accept'
                        and stale_token.token_ordinal<?5
                        and stale_app.result_ref='stale:task_derivation:'||td.id||':stale_accepted'
                    ))
                    or (td.status='closed' and exists(
                      select 1 from correction_transition_aliases a
                      join correction_transition_applications app on app.id=a.correction_application_id
                      join correction_tokens token on token.id=app.correction_token_id
                      where a.record_type='task' and a.record_id=t.id
                        and a.alias='@superseded-task/'||t.id
                        and token.operation='design-reconcile'
                        and a.correction_session_id=?4 and token.token_ordinal<?5
                    ))
                  )
                order by (r.design_version_id=?3) desc, r.id
                "#,
            )?;
        let candidates = stmt
            .query_map(
                params![
                    task_id,
                    work_unit_id,
                    design_version_id,
                    session_id,
                    token_ordinal
                ],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        return match candidates.as_slice() {
            [candidate] => Ok(*candidate),
            [] => bail!("task id is outside the correction owner or current design"),
            _ => bail!("numeric task disposition has ambiguous eligible derivations"),
        };
    }
    let key = opaque_key.context("task reference has no opaque key")?;
    let mut stmt = conn.prepare(
        r#"
        with eligible(task_id,requirement_id) as (
          select item.task_id,item.design_requirement_id
          from correction_transition_applications application
          join correction_tokens token on token.id=application.correction_token_id
          join checklist_items item
            on application.result_ref='checklist:'||item.checklist_id
          where application.correction_session_id=?1 and token.token_ordinal<?2
            and token.operation in ('design-decompose','design-reconcile')
          union
          select application_item.task_id,item_requirement.design_requirement_id
          from correction_transition_applications application
          join correction_tokens token on token.id=application.correction_token_id
          join decomposition_applications application_item
            on application.result_ref='decomposition-plan:'||application_item.decomposition_plan_id
          join decomposition_item_requirements item_requirement
            on item_requirement.decomposition_item_id=application_item.decomposition_item_id
          where application.correction_session_id=?1 and token.token_ordinal<?2
            and token.operation='decomposition-plan-reconcile'
        )
        select distinct task.id,requirement.id
        from eligible
        join tasks task on task.id=eligible.task_id
        join design_requirements requirement on requirement.id=eligible.requirement_id
        join task_derivations derivation on derivation.task_id=task.id
          and derivation.design_requirement_id=requirement.id and derivation.status='active'
        where task.work_unit_id=?3 and requirement.design_version_id=?4
          and task.status in ('open','blocked') and requirement.requirement_key=?5
        order by task.id,requirement.id
        "#,
    )?;
    let candidates = stmt
        .query_map(
            params![
                session_id,
                token_ordinal,
                work_unit_id,
                design_version_id,
                key
            ],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    match candidates.as_slice() {
        [candidate] => Ok(*candidate),
        [] => bail!("opaque task identity is absent from the preceding design/work decomposition"),
        _ => bail!("opaque task identity matches multiple stable decomposition identities"),
    }
}

pub(super) fn resolve_phase_ref(
    conn: &rusqlite::Connection,
    session_id: i64,
    token_ordinal: i64,
    work_unit_id: i64,
    value: &str,
) -> Result<i64> {
    let numeric_id = value.parse::<i64>().ok();
    let key = value.starts_with('@').then_some(value);
    if numeric_id.is_none() && key.is_none() {
        bail!("invalid phase reference");
    }
    if let Some(phase_id) = numeric_id {
        return conn
            .query_row(
                "select id from work_phases where id=?1 and work_unit_id=?2 and status in ('open','blocked')",
                params![phase_id, work_unit_id],
                |row| row.get(0),
            )
            .optional()?
            .context("numeric phase reference is outside the open correction owner");
    }
    conn.query_row(
        r#"
        select alias.record_id
        from correction_transition_aliases alias
        join correction_transition_applications app on app.id = alias.correction_application_id
        join correction_tokens token on token.id = app.correction_token_id
        where app.correction_session_id = ?1 and token.token_ordinal < ?2
          and alias.record_type = 'phase'
          and alias.alias = ?3
          and exists (
              select 1 from work_phases p
              where p.id = alias.record_id and p.work_unit_id = ?4
          )
        order by alias.id desc limit 1
        "#,
        params![session_id, token_ordinal, key, work_unit_id],
        |row| row.get(0),
    )
    .optional()?
    .context("phase reference was not created by an earlier correction token")
}

pub(super) fn ensure_phase_dependency_owner(
    conn: &rusqlite::Connection,
    dependency_id: i64,
    work_unit_id: i64,
) -> Result<()> {
    conn.query_row(
        r#"
        select 1
        from phase_epoch_dependencies d
        join phase_epochs source on source.id = d.from_phase_epoch_id
        join phase_epochs target on target.id = d.to_phase_epoch_id
        where d.id = ?1 and d.state = 'open'
          and source.work_unit_id = ?2 and target.work_unit_id = ?2
        "#,
        params![dependency_id, work_unit_id],
        |_| Ok(()),
    )
    .optional()?
    .context("open phase dependency is outside the correction work unit")
}

pub(super) fn ensure_phase_dependency_authority_scope(
    conn: &rusqlite::Connection,
    project_id: i64,
    authority_event_id: i64,
    dependency_id: i64,
    work_unit_id: i64,
) -> Result<()> {
    let scope: String = conn
        .query_row(
            "select scope from authority_events where id = ?1 and project_id = ?2 and status = 'active'",
            params![authority_event_id, project_id],
            |row| row.get(0),
        )
        .optional()?
        .context("active same-project authority event not found")?;
    if !matches!(scope.as_str(), "project")
        && scope != format!("phase-dependency:{dependency_id}")
        && scope != format!("work-unit:{work_unit_id}")
    {
        bail!("authority scope does not cover the exact phase dependency or owning work unit");
    }
    Ok(())
}
