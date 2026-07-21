use super::super::*;

pub(in crate::decomposition) fn install_reconciliation_mappings(
    conn: &Connection,
    project_id: i64,
    plan_id: i64,
    reconciliation: &PlanReconciliation,
) -> Result<()> {
    let item_ids = keyed_ids(
        conn,
        "select item_key,id from decomposition_items where decomposition_plan_id=?1",
        plan_id,
    )?;
    let slice_ids = keyed_ids(
        conn,
        "select slice_key,id from decomposition_slices where decomposition_plan_id=?1",
        plan_id,
    )?;
    for mapping in &reconciliation.tasks {
        let target = mapping.item.as_deref().map(|key| item_ids[key]);
        conn.execute(
            "insert into decomposition_reconciliation_tasks(project_id,decomposition_plan_id,source_task_id,successor_item_id,disposition,reason,effect) values(?1,?2,?3,?4,?5,?6,?7)",
            params![project_id, plan_id, mapping.source, target, mapping.disposition, mapping.reason, stored_effect(&mapping.disposition, mapping.effect)],
        )?;
    }
    for mapping in &reconciliation.checklist {
        let target = mapping
            .item
            .as_deref()
            .zip(mapping.boundary.as_deref())
            .map(|(item, boundary)| {
                conn.query_row(
                    "select boundary.id from decomposition_item_checklist_boundaries boundary where boundary.decomposition_item_id=?1 and boundary.boundary_key=?2",
                    params![item_ids[item], boundary],
                    |row| row.get::<_, i64>(0),
                )
            })
            .transpose()?;
        conn.execute(
            "insert into decomposition_reconciliation_checklist_items(project_id,decomposition_plan_id,source_checklist_item_id,successor_boundary_id,disposition,reason,effect) values(?1,?2,?3,?4,?5,?6,?7)",
            params![project_id, plan_id, mapping.source, target, mapping.disposition, mapping.reason, stored_effect(&mapping.disposition, mapping.effect)],
        )?;
    }
    for mapping in &reconciliation.gates {
        let target = mapping
            .item
            .as_deref()
            .zip(mapping.gate.as_deref())
            .map(|(item, gate)| {
                conn.query_row(
                    "select item_gate.id from decomposition_item_gates item_gate where item_gate.decomposition_item_id=?1 and item_gate.gate_key=?2",
                    params![item_ids[item], gate],
                    |row| row.get::<_, i64>(0),
                )
            })
            .transpose()?;
        let resolved_boundary_identity =
            if mapping.disposition == "retained" && reconciliation_v2_storage(conn)? {
                Some(resolve_gate_boundary_identity(
                    conn,
                    project_id,
                    mapping.source,
                    mapping
                        .boundary
                        .as_deref()
                        .context("retained gate mapping requires boundary")?,
                )?)
            } else {
                None
            };
        if reconciliation_v2_storage(conn)? {
            conn.execute(
                "insert into decomposition_reconciliation_gates(project_id,decomposition_plan_id,source_validation_gate_id,successor_item_gate_id,disposition,reason,effect,boundary_selector,resolved_boundary_identity) values(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![project_id, plan_id, mapping.source, target, mapping.disposition, mapping.reason, stored_effect(&mapping.disposition, mapping.effect), mapping.boundary, resolved_boundary_identity],
            )?;
        } else {
            conn.execute(
                "insert into decomposition_reconciliation_gates(project_id,decomposition_plan_id,source_validation_gate_id,successor_item_gate_id,disposition,reason) values(?1,?2,?3,?4,?5,?6)",
                params![project_id, plan_id, mapping.source, target, mapping.disposition, mapping.reason],
            )?;
        }
    }
    for mapping in &reconciliation.phases {
        let target = mapping.slice.as_deref().map(|key| slice_ids[key]);
        conn.execute(
            "insert into decomposition_reconciliation_phases(project_id,decomposition_plan_id,source_phase_id,successor_slice_id,disposition,reason,effect) values(?1,?2,?3,?4,?5,?6,?7)",
            params![project_id, plan_id, mapping.source, target, mapping.disposition, mapping.reason, stored_effect(&mapping.disposition, mapping.effect)],
        )?;
    }
    for mapping in &reconciliation.dependencies {
        let target = mapping
            .from
            .as_deref()
            .zip(mapping.to.as_deref())
            .map(|(from, to)| {
                conn.query_row(
                    "select id from decomposition_slice_dependencies where decomposition_plan_id=?1 and predecessor_slice_id=?2 and successor_slice_id=?3",
                    params![plan_id, slice_ids[from], slice_ids[to]],
                    |row| row.get::<_, i64>(0),
                )
            })
            .transpose()?;
        conn.execute(
            "insert into decomposition_reconciliation_dependencies(project_id,decomposition_plan_id,source_dependency_id,successor_dependency_id,disposition,reason,effect) values(?1,?2,?3,?4,?5,?6,?7)",
            params![project_id, plan_id, mapping.source, target, mapping.disposition, mapping.reason, stored_effect(&mapping.disposition, mapping.effect)],
        )?;
    }
    Ok(())
}

pub(in crate::decomposition) fn resolve_gate_boundary_identity(
    conn: &Connection,
    project_id: i64,
    source_gate_id: i64,
    selector: &str,
) -> Result<String> {
    if selector == "retained-source" {
        return crate::update::transition::validation_gate_boundary_identity(conn, source_gate_id);
    }
    require_digest(selector, "successor validation-gate boundary")?;
    let gate_ids = conn
        .prepare("select id from validation_gates where project_id=?1 order by id")?
        .query_map([project_id], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut matches = Vec::new();
    for gate_id in gate_ids {
        if crate::update::transition::validation_gate_boundary_identity(conn, gate_id)? == selector
        {
            matches.push(gate_id);
        }
    }
    if matches.len() != 1 {
        bail!("successor validation-gate boundary must resolve exactly once");
    }
    Ok(selector.to_string())
}

fn keyed_ids(conn: &Connection, sql: &str, plan_id: i64) -> Result<BTreeMap<String, i64>> {
    conn.prepare(sql)?
        .query_map([plan_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()
        .map_err(Into::into)
}

pub(in crate::decomposition) fn retained_task_targets(
    conn: &Connection,
    plan_id: i64,
) -> Result<BTreeMap<i64, (i64, ReconciliationEffect)>> {
    let rows = conn
        .prepare(
            "select successor_item_id,source_task_id,effect from decomposition_reconciliation_tasks where decomposition_plan_id=?1 and disposition='retained' order by successor_item_id,source_task_id",
        )?
        .query_map([plan_id], |row| {
            let effect = match row.get::<_, String>(2)?.as_str() {
                "preserve" => ReconciliationEffect::Preserve,
                "open" => ReconciliationEffect::Open,
                _ => return Err(rusqlite::Error::InvalidQuery),
            };
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, effect))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut targets = BTreeMap::new();
    for (item, source, effect) in rows {
        if targets.insert(item, (source, effect)).is_some() {
            bail!("multiple retained task identities require an explicit merge decision");
        }
    }
    Ok(targets)
}

pub(in crate::decomposition) fn supersede_predecessor_endpoints(
    conn: &Connection,
    predecessor_plan_id: i64,
    successor_plan_id: i64,
) -> Result<()> {
    conn.execute(
        "update decomposition_plans set status='superseded' where id=?1 and status in ('applied','incomplete')",
        [predecessor_plan_id],
    )?;
    conn.execute(
        r#"
        update phase_epochs set state='superseded',terminal_at=current_timestamp,
          terminal_summary='superseded by Decomposition Plan '||?2
        where id in (
          select source_phase_id from decomposition_reconciliation_phases
          where decomposition_plan_id=?2
        ) and state in ('open','blocked')
        "#,
        params![predecessor_plan_id, successor_plan_id],
    )?;
    conn.execute(
        r#"
        update work_phases
        set phase_key='superseded-plan-'||?1||'-phase-'||id
        where id in (
          select source_phase_id from decomposition_reconciliation_phases
          where decomposition_plan_id=?2
        ) and status in ('open','blocked')
        "#,
        params![predecessor_plan_id, successor_plan_id],
    )?;
    conn.execute(
        r#"
        update phase_epoch_memberships set state='superseded',terminal_at=current_timestamp
        where phase_epoch_id in (
          select source_phase_id from decomposition_reconciliation_phases
          where decomposition_plan_id=?1
        ) and state='current'
        "#,
        [successor_plan_id],
    )?;
    conn.execute(
        r#"
        update phase_epoch_dependencies set state='invalidated',terminal_at=current_timestamp
        where id in (
          select source_dependency_id from decomposition_reconciliation_dependencies
          where decomposition_plan_id=?1
        ) and state='open'
        "#,
        [successor_plan_id],
    )?;
    Ok(())
}

pub(in crate::decomposition) fn retire_predecessor_trace_endpoints(
    conn: &Connection,
    predecessor_plan_id: i64,
) -> Result<()> {
    conn.execute(
        r#"
        update checklists set status='stale'
        where status='active' and id in (
          select application.checklist_id
          from decomposition_applications application
          where application.decomposition_plan_id=?1
        )
        "#,
        [predecessor_plan_id],
    )?;
    conn.execute(
        r#"
        update validation_gates set status='stale'
        where status='active' and id in (
          select gate.validation_gate_id
          from decomposition_application_gates gate
          join decomposition_item_gates declared
            on declared.id=gate.decomposition_item_gate_id
          join decomposition_items item on item.id=declared.decomposition_item_id
          where item.decomposition_plan_id=?1
        )
        "#,
        [predecessor_plan_id],
    )?;
    conn.execute(
        r#"
        update task_derivations set status='stale'
        where status='active' and id in (
          select requirement.task_derivation_id
          from decomposition_application_requirements requirement
          join decomposition_item_requirements declared
            on declared.id=requirement.decomposition_item_requirement_id
          join decomposition_items item on item.id=declared.decomposition_item_id
          where item.decomposition_plan_id=?1
        )
        "#,
        [predecessor_plan_id],
    )?;
    Ok(())
}

pub(in crate::decomposition) fn carry_reconciliation_states(
    conn: &Connection,
    project_id: i64,
    plan_id: i64,
) -> Result<()> {
    let task_states = conn
        .prepare(
            r#"
            select application.task_id,source.status
            from decomposition_reconciliation_tasks mapping
            join tasks source on source.id=mapping.source_task_id
            join decomposition_applications application
              on application.decomposition_plan_id=mapping.decomposition_plan_id
             and application.decomposition_item_id=mapping.successor_item_id
            where mapping.decomposition_plan_id=?1 and mapping.disposition='retained'
              and mapping.effect='preserve'
            order by application.task_id
            "#,
        )?
        .query_map([plan_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (task, state) in task_states {
        conn.execute(
            "update tasks set status=?1 where id=?2 and work_unit_id=(select work_unit_id from decomposition_plans where id=?3)",
            params![state, task, plan_id],
        )?;
    }

    let checklist_states = conn
        .prepare(
            r#"
            select application.checklist_item_id,source.status
            from decomposition_reconciliation_checklist_items mapping
            join checklist_items source on source.id=mapping.source_checklist_item_id
            join decomposition_application_boundaries application
              on application.decomposition_item_checklist_boundary_id=mapping.successor_boundary_id
            where mapping.decomposition_plan_id=?1 and mapping.disposition='retained'
              and mapping.effect='preserve'
            order by application.checklist_item_id,source.id
            "#,
        )?
        .query_map([plan_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    apply_uniform_states(
        conn,
        "checklist item",
        "checklist_items",
        "status",
        checklist_states,
    )?;

    let gate_states = conn
        .prepare(
            r#"
            select target.id,source.status
            from decomposition_reconciliation_gates mapping
            join validation_gates source on source.id=mapping.source_validation_gate_id
            join design_requirements source_requirement on source_requirement.id=source.design_requirement_id
            join decomposition_application_gates application
              on application.decomposition_item_gate_id=mapping.successor_item_gate_id
            join validation_gates target on target.id=application.validation_gate_id
            join design_requirements target_requirement on target_requirement.id=target.design_requirement_id
            where mapping.decomposition_plan_id=?1 and mapping.disposition='retained'
              and mapping.effect='preserve'
              and source_requirement.requirement_key=target_requirement.requirement_key
            order by target.id,source.id
            "#,
        )?
        .query_map([plan_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mapped_gate_count = gate_states.len() as i64;
    let expected_gate_count: i64 = conn.query_row(
        "select count(*) from decomposition_reconciliation_gates where decomposition_plan_id=?1 and disposition='retained' and effect='preserve'",
        [plan_id],
        |row| row.get(0),
    )?;
    if mapped_gate_count != expected_gate_count {
        bail!("retained validation gates require one exact requirement-key successor");
    }
    apply_uniform_states(
        conn,
        "validation gate",
        "validation_gates",
        "status",
        gate_states,
    )?;

    let phase_states = conn
        .prepare(
            r#"
            select distinct application.phase_id,source.status
            from decomposition_reconciliation_phases mapping
            join work_phases source on source.id=mapping.source_phase_id
            join decomposition_items item on item.slice_id=mapping.successor_slice_id
            join decomposition_applications application
              on application.decomposition_plan_id=mapping.decomposition_plan_id
             and application.decomposition_item_id=item.id
            where mapping.decomposition_plan_id=?1 and mapping.disposition='retained'
              and mapping.effect='preserve'
            order by application.phase_id,source.id
            "#,
        )?
        .query_map([plan_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let phase_states = uniform_state_map("phase", phase_states)?;
    for (phase, state) in phase_states {
        conn.execute(
            "update work_phases set status=?1,closed_at=case when ?1 in ('closed','accepted_out_of_scope','split') then current_timestamp else null end where id=?2",
            params![state, phase],
        )?;
        let epoch_state = match state.as_str() {
            "open" | "blocked" | "closed" | "split" => state.as_str(),
            "accepted_out_of_scope" => "superseded",
            _ => bail!("unsupported retained phase state"),
        };
        conn.execute(
            "update phase_epochs set state=?1,terminal_at=case when ?1 in ('closed','split','superseded') then current_timestamp else null end where id=?2",
            params![epoch_state, phase],
        )?;
        if state == "accepted_out_of_scope" {
            conn.execute(
                "insert into phase_scope_dispositions(project_id,phase_epoch_id,scope_kind,task_identity_id,state,reason,authority_event_id,created_at) values(?1,?2,'whole_phase',null,'accepted_out_of_scope','retained by explicit Decomposition Plan reconciliation',null,current_timestamp)",
                params![project_id, phase],
            )?;
        }
    }

    let dependency_states = conn
        .prepare(
            r#"
            select application.work_phase_dependency_id,source.status,
                   source.evidence_ref,source.authority_event_id,
                   source.from_phase_id,target.from_phase_id,
                   target_from.status
            from decomposition_reconciliation_dependencies mapping
            join work_phase_dependencies source on source.id=mapping.source_dependency_id
            join decomposition_application_dependencies application
              on application.decomposition_slice_dependency_id=mapping.successor_dependency_id
            join work_phase_dependencies target
              on target.id=application.work_phase_dependency_id
            join work_phases target_from on target_from.id=target.from_phase_id
            where mapping.decomposition_plan_id=?1 and mapping.disposition='retained'
              and mapping.effect='preserve'
            order by application.work_phase_dependency_id,source.id
            "#,
        )?
        .query_map([plan_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<i64>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut seen_dependencies = BTreeSet::new();
    for (
        dependency,
        source_state,
        source_evidence,
        source_authority,
        source_from,
        successor_from,
        successor_from_state,
    ) in dependency_states
    {
        if !seen_dependencies.insert(dependency) {
            bail!("multiple retained dependencies cannot collapse into one successor edge");
        }
        let (state, evidence, authority) = recompute_dependency_state(
            &source_state,
            source_evidence,
            source_authority,
            source_from,
            successor_from,
            &successor_from_state,
        )?;
        conn.execute(
            "update work_phase_dependencies set status=?1,evidence_ref=?2,authority_event_id=?3,resolved_at=case when ?1!='open' then current_timestamp else null end where id=?4",
            params![state, evidence, authority, dependency],
        )?;
        conn.execute(
            "update phase_epoch_dependencies set state=?1,evidence_ref=?2,authority_event_id=?3,terminal_at=case when ?1!='open' then current_timestamp else null end where id=?4",
            params![state, evidence, authority, dependency],
        )?;
    }
    Ok(())
}

pub(crate) fn recompute_dependency_state(
    source_state: &str,
    source_evidence: Option<String>,
    source_authority: Option<i64>,
    source_from: i64,
    successor_from: i64,
    successor_from_state: &str,
) -> Result<(&'static str, Option<String>, Option<i64>)> {
    match source_state {
        "open" => Ok(("open", None, None)),
        "satisfied" => {
            let evidence = source_evidence
                .filter(|evidence| !evidence.trim().is_empty())
                .context("preserve dependency effect requires qualifying evidence")?;
            let predecessor_close = format!("phase:{source_from}:closed");
            if evidence == predecessor_close {
                if successor_from_state != "closed" {
                    bail!("preserve dependency effect cannot reuse obsolete phase-close evidence");
                }
                Ok((
                    "satisfied",
                    Some(format!("phase:{successor_from}:closed")),
                    None,
                ))
            } else {
                Ok(("satisfied", Some(evidence), None))
            }
        }
        "accepted" => Ok((
            "accepted",
            None,
            Some(
                source_authority
                    .context("preserve dependency effect requires qualifying authority")?,
            ),
        )),
        _ => bail!("unsupported retained dependency state"),
    }
}

fn apply_uniform_states(
    conn: &Connection,
    label: &str,
    table: &str,
    column: &str,
    states: Vec<(i64, String)>,
) -> Result<()> {
    for (id, state) in uniform_state_map(label, states)? {
        conn.execute(
            &format!("update {table} set {column}=?1 where id=?2"),
            params![state, id],
        )?;
    }
    Ok(())
}

fn uniform_state_map(label: &str, states: Vec<(i64, String)>) -> Result<BTreeMap<i64, String>> {
    let mut targets = BTreeMap::new();
    for (target, state) in states {
        if targets
            .insert(target, state.clone())
            .is_some_and(|existing| existing != state)
        {
            bail!("retained {label} sources disagree on successor state");
        }
    }
    Ok(targets)
}

pub(in crate::decomposition) fn record_decomposition_lineage(
    conn: &Connection,
    project_id: i64,
    predecessor_plan_id: i64,
    successor_plan_id: i64,
) -> Result<()> {
    let predecessor_items = id_column(
        conn,
        "select id from decomposition_items where decomposition_plan_id=?1 order by id",
        [predecessor_plan_id],
    )?;
    for predecessor_item in predecessor_items {
        let successor_items = id_column(
            conn,
            r#"
            select distinct mapping.successor_item_id
            from decomposition_reconciliation_tasks mapping
            where mapping.decomposition_plan_id=?1 and mapping.disposition='retained'
              and mapping.source_task_id in (
                select application.task_id from decomposition_applications application
                where application.decomposition_plan_id=?2
                  and application.decomposition_item_id=?3
                union
                select migration.source_task_id from decomposition_migration_sources migration
                where migration.decomposition_plan_id=?2
                  and migration.decomposition_item_id=?3
              )
            order by mapping.successor_item_id
            "#,
            params![successor_plan_id, predecessor_plan_id, predecessor_item],
        )?;
        let (successor_item, disposition, reason): (Option<i64>, &str, Option<&str>) =
            match successor_items.len() {
                0 => (
                    None,
                    "retired",
                    Some("all predecessor task endpoints were explicitly retired"),
                ),
                1 => (successor_items.first().copied(), "retained", None),
                _ => bail!(
                    "one predecessor item cannot split across successor items without explicit item lineage"
                ),
            };
        conn.execute(
            "insert into decomposition_lineage(project_id,predecessor_plan_id,predecessor_item_id,successor_plan_id,successor_item_id,disposition,reason) values(?1,?2,?3,?4,?5,?6,?7)",
            params![project_id, predecessor_plan_id, predecessor_item, successor_plan_id, successor_item, disposition, reason],
        )?;
    }
    Ok(())
}

pub(in crate::decomposition) fn hash_query_rows(
    conn: &Connection,
    sql: &str,
    plan_id: i64,
    hasher: &mut Sha256,
) -> Result<()> {
    use rusqlite::types::ValueRef;
    let mut statement = conn.prepare(sql)?;
    let column_count = statement.column_count();
    let mut rows = statement.query([plan_id])?;
    while let Some(row) = rows.next()? {
        hasher.update(b"row\0");
        for index in 0..column_count {
            match row.get_ref(index)? {
                ValueRef::Null => hasher.update(b"null"),
                ValueRef::Integer(value) => hasher.update(value.to_be_bytes()),
                ValueRef::Real(value) => hasher.update(value.to_bits().to_be_bytes()),
                ValueRef::Text(value) | ValueRef::Blob(value) => hasher.update(value),
            }
            hasher.update(b"\0");
        }
    }
    Ok(())
}

pub(in crate::decomposition) fn string_column<P: rusqlite::Params>(
    conn: &Connection,
    sql: &str,
    params: P,
) -> Result<Vec<String>> {
    let mut statement = conn.prepare(sql)?;
    statement
        .query_map(params, |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub(in crate::decomposition) fn id_column<P: rusqlite::Params>(
    conn: &Connection,
    sql: &str,
    params: P,
) -> Result<BTreeSet<i64>> {
    let mut statement = conn.prepare(sql)?;
    statement
        .query_map(params, |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<BTreeSet<_>>>()
        .map_err(Into::into)
}
