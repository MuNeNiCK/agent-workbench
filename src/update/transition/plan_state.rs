use super::*;

pub(super) fn backfill_reconciliation_gate_boundaries(conn: &Connection) -> Result<()> {
    let rows = conn
        .prepare(
            r#"
            select id,source_validation_gate_id
            from decomposition_reconciliation_gates
            where disposition='retained'
            order by id
            "#,
        )?
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (mapping_id, gate_id) in rows {
        let identity = validation_gate_boundary_identity(conn, gate_id)?;
        conn.execute(
            r#"
            update decomposition_reconciliation_gates
            set boundary_selector='retained-source',resolved_boundary_identity=?1
            where id=?2 and disposition='retained'
            "#,
            rusqlite::params![identity, mapping_id],
        )?;
    }
    Ok(())
}

pub(super) fn validate_reconciliation_gate_boundaries(conn: &Connection) -> Result<()> {
    let rows = conn
        .prepare(
            r#"
            select source_validation_gate_id,boundary_selector,resolved_boundary_identity
            from decomposition_reconciliation_gates
            where disposition='retained'
            order by id
            "#,
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (source_gate, selector, resolved) in rows {
        if selector == "retained-source" {
            if resolved != validation_gate_boundary_identity(conn, source_gate)? {
                bail!("retained-source does not conserve its exact validation boundary");
            }
            continue;
        }
        if selector.len() != 64
            || selector != resolved
            || !selector
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            bail!("explicit successor validation boundary is not one exact opaque identity");
        }
        let gate_ids = conn
            .prepare("select id from validation_gates order by id")?
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut matches = 0;
        for gate_id in gate_ids {
            matches += i64::from(validation_gate_boundary_identity(conn, gate_id)? == selector);
        }
        if matches != 1 {
            bail!("explicit successor validation boundary must resolve exactly once");
        }
    }
    Ok(())
}

pub(crate) fn validation_gate_boundary_identity(conn: &Connection, gate_id: i64) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/validation-gate-boundary/v1\0");
    let mut statement = conn.prepare(
        r#"
        select id,project_id,gate_key,coalesce(template_id,''),coalesce(work_unit_id,''),
               coalesce(task_id,''),coalesce(design_requirement_id,''),
               coalesce(command_profile_id,''),coalesce(command,''),expected_result,
               coalesce(environment,''),coalesce(timeout,''),
               coalesce(artifact_requirements,''),selected_before_edit
        from validation_gates where id=?1
        "#,
    )?;
    let mut rows = statement.query([gate_id])?;
    let row = rows
        .next()?
        .context("retained reconciliation gate has no source validation boundary")?;
    for index in 0..14 {
        hash_value_ref(&mut hasher, row.get_ref(index)?);
    }
    if rows.next()?.is_some() {
        bail!("retained reconciliation gate source boundary is ambiguous");
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub(super) fn hash_value_ref(hasher: &mut Sha256, value: ValueRef<'_>) {
    match value {
        ValueRef::Null => hasher.update(b"null\0"),
        ValueRef::Integer(value) => {
            hasher.update(b"integer\0");
            hasher.update(value.to_be_bytes());
        }
        ValueRef::Real(value) => {
            hasher.update(b"real\0");
            hasher.update(value.to_bits().to_be_bytes());
        }
        ValueRef::Text(value) => {
            hasher.update(b"text\0");
            hasher.update((value.len() as u64).to_be_bytes());
            hasher.update(value);
        }
        ValueRef::Blob(value) => {
            hasher.update(b"blob\0");
            hasher.update((value.len() as u64).to_be_bytes());
            hasher.update(value);
        }
    }
    hasher.update(b"\0");
}

pub(super) fn backfill_owned_plan_documents(conn: &Connection) -> Result<()> {
    let plans = conn
        .prepare("select id from decomposition_plans order by id")?
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for plan_id in plans {
        let content = canonical_owned_plan_document(conn, plan_id)?;
        let identity = crate::decomposition::plan_content_identity(&content);
        conn.execute(
            "update decomposition_plans set document_content=?1,content_identity=?2 where id=?3",
            rusqlite::params![content, identity, plan_id],
        )?;
    }
    Ok(())
}

pub(super) fn validate_owned_plan_documents(conn: &Connection) -> Result<()> {
    let plans = conn
        .prepare(
            "select id,document_content,content_identity,design_package_id from decomposition_plans order by id",
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<i64>>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (plan_id, content, identity, package) in plans {
        let content = content.context("target Plan revision has no owned document content")?;
        let identity = identity.context("target Plan revision has no content identity")?;
        if package.is_none()
            || identity != crate::decomposition::plan_content_identity(&content)
            || content != canonical_owned_plan_document(conn, plan_id)?
        {
            bail!("target Plan revision does not own its canonical durable document");
        }
    }
    Ok(())
}

pub(super) fn validate_owned_plan_content_identities(conn: &Connection) -> Result<()> {
    let plans = conn
        .prepare(
            "select document_content,content_identity,design_package_id from decomposition_plans order by id",
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (content, identity, package) in plans {
        let canonical_document = decode_canonical_plan_document(&content)?;
        if content.is_empty()
            || package.is_none()
            || identity != crate::decomposition::plan_content_identity(&content)
            || crate::decomposition::canonical_plan_content(&canonical_document)? != content
        {
            bail!("current Plan revision has mismatched owned content or lineage");
        }
    }
    Ok(())
}

pub(super) fn decode_canonical_plan_document(
    content: &str,
) -> Result<crate::decomposition::PlanDocument> {
    let metadata = content
        .strip_prefix("# Decomposition Plan\n\n```yaml agent-workbench\n")
        .and_then(|body| body.strip_suffix("\n```\n"))
        .context("owned Decomposition Plan content is not canonical managed content")?;
    serde_json::from_str(metadata).context("owned Decomposition Plan metadata is invalid")
}

type StoredPlanHeader = (
    String,
    String,
    Option<i64>,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<i64>,
);

pub(super) fn canonical_owned_plan_document(conn: &Connection, plan_id: i64) -> Result<String> {
    let (key, design_fingerprint, work, status, source_kind, source_path, issue, predecessor):
        StoredPlanHeader = conn.query_row(
        r#"
        select plan_key,design_fingerprint,work_unit_id,status,source_kind,source_path,
               binding_issue,predecessor_id
        from decomposition_plans where id=?1
        "#,
        [plan_id],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
            ))
        },
    )?;
    let slices = canonical_slices(conn, plan_id)?;
    let items = canonical_items(conn, plan_id)?;
    let reconciliation = canonical_reconciliation(conn, plan_id, predecessor)?;
    let document = json!({
        "type": "decomposition_plan",
        "format": 1,
        "key": key,
        "design_fingerprint": design_fingerprint,
        "work": work,
        "items": items,
        "slices": slices,
        "reconciliation": reconciliation,
    });
    // The durable state/provenance fields above deliberately participate in
    // synthesis eligibility, while the managed document remains the same
    // canonical public representation used by ordinary decomposition parsing.
    let _synthesis_provenance = (status, source_kind, source_path, issue);
    let typed: crate::decomposition::PlanDocument = serde_json::from_value(document)?;
    crate::decomposition::canonical_plan_content(&typed)
}

pub(super) fn canonical_slices(conn: &Connection, plan_id: i64) -> Result<Vec<Value>> {
    let rows = conn
        .prepare(
            "select id,slice_key,title,slice_order from decomposition_slices where decomposition_plan_id=?1 order by slice_order,id",
        )?
        .query_map([plan_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, i64>(3)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(id, key, title, order)| {
            let depends_on = conn
                .prepare(
                    r#"
                    select source.slice_key
                    from decomposition_slice_dependencies dependency
                    join decomposition_slices source on source.id=dependency.predecessor_slice_id
                    where dependency.decomposition_plan_id=?1
                      and dependency.successor_slice_id=?2
                    order by source.slice_order,source.id
                    "#,
                )?
                .query_map(rusqlite::params![plan_id, id], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            Ok(json!({"key":key,"title":title,"order":order,"depends_on":depends_on}))
        })
        .collect()
}

pub(super) fn canonical_items(conn: &Connection, plan_id: i64) -> Result<Vec<Value>> {
    let rows = conn
        .prepare(
            r#"
            select item.id,item.item_key,item.title,item.details,item.outcome,item.observation,
                   item.evidence_owner,item.evidence_kind,coalesce(slice.slice_key,'')
            from decomposition_items item
            left join decomposition_slices slice on slice.id=item.slice_id
            where item.decomposition_plan_id=?1 order by item.id
            "#,
        )?
        .query_map([plan_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(id,key,title,details,outcome,observation,owner,kind,slice)| {
            let requirements = string_values(conn,
                "select requirement.requirement_key from decomposition_item_requirements link join design_requirements requirement on requirement.id=link.design_requirement_id where link.decomposition_item_id=?1 order by requirement.requirement_key", id)?;
            let gates = string_values(conn,
                "select gate_key from decomposition_item_gates where decomposition_item_id=?1 order by gate_key", id)?;
            let boundary_rows = conn.prepare(
                "select id,boundary_key,condition_text,evidence_kind from decomposition_item_checklist_boundaries where decomposition_item_id=?1 order by boundary_order,id")?
                .query_map([id], |row| Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let mut checklist = Vec::new();
            for (boundary_id,boundary,condition,evidence_kind) in boundary_rows {
                let boundary_gates = string_values(conn,
                    "select gate_key from decomposition_item_checklist_boundary_gates where decomposition_item_checklist_boundary_id=?1 order by gate_key", boundary_id)?;
                checklist.push(json!({"key":boundary,"condition":condition,"evidence_kind":evidence_kind,"gates":boundary_gates}));
            }
            Ok(json!({
                "key":key,"requirements":requirements,"title":title,"details":details,
                "completion":{"outcome":outcome,"observation":observation,"evidence_owner":owner,"evidence_kind":kind,"gates":gates},
                "checklist":checklist,"slice":slice
            }))
        })
        .collect()
}

pub(super) fn string_values(conn: &Connection, sql: &str, id: i64) -> Result<Vec<String>> {
    conn.prepare(sql)?
        .query_map([id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub(super) fn canonical_reconciliation(
    conn: &Connection,
    plan_id: i64,
    predecessor: Option<i64>,
) -> Result<Option<Value>> {
    let mapping_count: i64 = conn.query_row(
        r#"
        select
          (select count(*) from decomposition_reconciliation_tasks where decomposition_plan_id=?1)+
          (select count(*) from decomposition_reconciliation_checklist_items where decomposition_plan_id=?1)+
          (select count(*) from decomposition_reconciliation_gates where decomposition_plan_id=?1)+
          (select count(*) from decomposition_reconciliation_phases where decomposition_plan_id=?1)+
          (select count(*) from decomposition_reconciliation_dependencies where decomposition_plan_id=?1)
        "#,
        [plan_id],
        |row| row.get(0),
    )?;
    if mapping_count == 0 {
        return Ok(None);
    }
    let predecessor = predecessor.context("reconciliation mappings have no predecessor Plan")?;
    let expected_current = conn
        .query_row(
            "select expected_current from decomposition_reconciliation_applications where successor_plan_id=?1",
            [plan_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .unwrap_or_else(|| "0".repeat(64));
    Ok(Some(json!({
        "predecessor": predecessor,
        "expected_current": expected_current,
        "tasks": canonical_task_mappings(conn, plan_id)?,
        "checklist": canonical_checklist_mappings(conn, plan_id)?,
        "gates": canonical_gate_mappings(conn, plan_id)?,
        "phases": canonical_phase_mappings(conn, plan_id)?,
        "dependencies": canonical_dependency_mappings(conn, plan_id)?,
    })))
}

pub(super) fn canonical_task_mappings(conn: &Connection, plan_id: i64) -> Result<Vec<Value>> {
    conn.prepare(
        r#"
        select mapping.source_task_id,mapping.disposition,item.item_key,mapping.reason,mapping.effect
        from decomposition_reconciliation_tasks mapping
        left join decomposition_items item on item.id=mapping.successor_item_id
        where mapping.decomposition_plan_id=?1 order by mapping.source_task_id
        "#,
    )?
    .query_map([plan_id], |row| {
        Ok(json!({"source":row.get::<_,i64>(0)?,"disposition":row.get::<_,String>(1)?,
            "item":row.get::<_,Option<String>>(2)?,"reason":row.get::<_,Option<String>>(3)?,
            "effect":row.get::<_,Option<String>>(4)?}))
    })?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(Into::into)
}

pub(super) fn canonical_checklist_mappings(conn: &Connection, plan_id: i64) -> Result<Vec<Value>> {
    conn.prepare(
        r#"
        select mapping.source_checklist_item_id,mapping.disposition,item.item_key,
               boundary.boundary_key,mapping.reason,mapping.effect
        from decomposition_reconciliation_checklist_items mapping
        left join decomposition_item_checklist_boundaries boundary on boundary.id=mapping.successor_boundary_id
        left join decomposition_items item on item.id=boundary.decomposition_item_id
        where mapping.decomposition_plan_id=?1 order by mapping.source_checklist_item_id
        "#,
    )?
    .query_map([plan_id], |row| {
        Ok(json!({"source":row.get::<_,i64>(0)?,"disposition":row.get::<_,String>(1)?,
            "item":row.get::<_,Option<String>>(2)?,"boundary":row.get::<_,Option<String>>(3)?,
            "reason":row.get::<_,Option<String>>(4)?,"effect":row.get::<_,Option<String>>(5)?}))
    })?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(Into::into)
}

pub(super) fn canonical_gate_mappings(conn: &Connection, plan_id: i64) -> Result<Vec<Value>> {
    conn.prepare(
        r#"
        select mapping.source_validation_gate_id,mapping.disposition,item.item_key,item_gate.gate_key,
               mapping.boundary_selector,mapping.resolved_boundary_identity,mapping.reason,mapping.effect
        from decomposition_reconciliation_gates mapping
        left join decomposition_item_gates item_gate on item_gate.id=mapping.successor_item_gate_id
        left join decomposition_items item on item.id=item_gate.decomposition_item_id
        where mapping.decomposition_plan_id=?1 order by mapping.source_validation_gate_id
        "#,
    )?
    .query_map([plan_id], |row| {
        Ok(json!({"source":row.get::<_,i64>(0)?,"disposition":row.get::<_,String>(1)?,
            "item":row.get::<_,Option<String>>(2)?,"gate":row.get::<_,Option<String>>(3)?,
            "boundary":row.get::<_,Option<String>>(4)?,
            "reason":row.get::<_,Option<String>>(6)?,"effect":row.get::<_,Option<String>>(7)?}))
    })?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(Into::into)
}

pub(super) fn canonical_phase_mappings(conn: &Connection, plan_id: i64) -> Result<Vec<Value>> {
    conn.prepare(
        r#"
        select mapping.source_phase_id,mapping.disposition,slice.slice_key,mapping.reason,mapping.effect
        from decomposition_reconciliation_phases mapping
        left join decomposition_slices slice on slice.id=mapping.successor_slice_id
        where mapping.decomposition_plan_id=?1 order by mapping.source_phase_id
        "#,
    )?
    .query_map([plan_id], |row| {
        Ok(json!({"source":row.get::<_,i64>(0)?,"disposition":row.get::<_,String>(1)?,
            "slice":row.get::<_,Option<String>>(2)?,"reason":row.get::<_,Option<String>>(3)?,
            "effect":row.get::<_,Option<String>>(4)?}))
    })?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(Into::into)
}

pub(super) fn canonical_dependency_mappings(conn: &Connection, plan_id: i64) -> Result<Vec<Value>> {
    conn.prepare(
        r#"
        select mapping.source_dependency_id,mapping.disposition,source.slice_key,target.slice_key,
               mapping.reason,mapping.effect
        from decomposition_reconciliation_dependencies mapping
        left join decomposition_slice_dependencies dependency on dependency.id=mapping.successor_dependency_id
        left join decomposition_slices source on source.id=dependency.predecessor_slice_id
        left join decomposition_slices target on target.id=dependency.successor_slice_id
        where mapping.decomposition_plan_id=?1 order by mapping.source_dependency_id
        "#,
    )?
    .query_map([plan_id], |row| {
        Ok(json!({"source":row.get::<_,i64>(0)?,"disposition":row.get::<_,String>(1)?,
            "from":row.get::<_,Option<String>>(2)?,"to":row.get::<_,Option<String>>(3)?,
            "reason":row.get::<_,Option<String>>(4)?,"effect":row.get::<_,Option<String>>(5)?}))
    })?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(Into::into)
}

pub(super) fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    conn.query_row(
        "select exists(select 1 from pragma_table_info(?1) where name=?2)",
        [table, column],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn reconciliation_projection_digest(conn: &Connection) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/reconciliation-v1-projection/v1\0");
    for table in std::iter::once("decomposition_plans").chain(RECONCILIATION_MAPPING_TABLES) {
        let mut columns = conn
            .prepare("select name from pragma_table_info(?1) order by cid")?
            .query_map([table], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        columns.retain(|column| {
            !matches!(
                column.as_str(),
                "document_content"
                    | "content_identity"
                    | "design_package_id"
                    | "effect"
                    | "boundary_selector"
                    | "resolved_boundary_identity"
            )
        });
        if columns.is_empty() {
            bail!("reconciliation mapping has no conserved fields");
        }
        hasher.update(table.as_bytes());
        hasher.update(b"\0");
        let projection = columns
            .iter()
            .map(|column| quote_identifier(column))
            .collect::<Vec<_>>()
            .join(",");
        let mut statement = conn.prepare(&format!(
            "select {projection} from {} order by {projection}",
            quote_identifier(table)
        ))?;
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            hasher.update(b"row\0");
            for index in 0..columns.len() {
                match row.get_ref(index)? {
                    ValueRef::Null => hasher.update(b"null\0"),
                    ValueRef::Integer(value) => {
                        hasher.update(b"integer\0");
                        hasher.update(value.to_be_bytes());
                    }
                    ValueRef::Real(value) => {
                        hasher.update(b"real\0");
                        hasher.update(value.to_bits().to_be_bytes());
                    }
                    ValueRef::Text(value) => {
                        hasher.update(b"text\0");
                        hasher.update((value.len() as u64).to_be_bytes());
                        hasher.update(value);
                    }
                    ValueRef::Blob(value) => {
                        hasher.update(b"blob\0");
                        hasher.update((value.len() as u64).to_be_bytes());
                        hasher.update(value);
                    }
                }
                hasher.update(b"\0");
            }
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub(super) fn reconciliation_balance(conn: &Connection) -> Result<ReconciliationBalance> {
    fn read(conn: &Connection, table: &str) -> Result<MappingBalance> {
        conn.query_row(
            &format!(
                "select sum(disposition='retained'),sum(disposition='retired') from {}",
                quote_identifier(table)
            ),
            [],
            |row| {
                Ok(MappingBalance {
                    retained: row.get::<_, Option<i64>>(0)?.unwrap_or(0),
                    retired: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                })
            },
        )
        .map_err(Into::into)
    }

    Ok(ReconciliationBalance {
        tasks: read(conn, "decomposition_reconciliation_tasks")?,
        checklist_items: read(conn, "decomposition_reconciliation_checklist_items")?,
        gates: read(conn, "decomposition_reconciliation_gates")?,
        phases: read(conn, "decomposition_reconciliation_phases")?,
        dependencies: read(conn, "decomposition_reconciliation_dependencies")?,
    })
}
