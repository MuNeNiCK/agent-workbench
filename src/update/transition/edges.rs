use super::*;

pub(super) fn observe_historical_source(
    conn: &Connection,
    context: &TransitionContext<'_>,
) -> Result<SourceObservation> {
    let generation = full_generation(conn)?;
    let descriptor = historical_descriptor(generation)
        .context("storage header has no registered historical descriptor")?;
    if !table_exists(conn, "projects")? {
        bail!("historical source is missing its project aggregate");
    }
    let project_count: i64 =
        conn.query_row("select count(*) from projects", [], |row| row.get(0))?;
    if project_count == 0 {
        bail!("historical source has no project aggregate");
    }
    let ledger_identity = conservation::semantic_ledger_identity(conn)?;
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/historical-storage-source/v1\0");
    hasher.update(generation.to_be_bytes());
    hasher.update(context.root.as_os_str().as_encoded_bytes());
    hasher.update(b"\0");
    hasher.update(ledger_identity.as_bytes());
    Ok(SourceObservation {
        descriptor_key: descriptor.key.to_string(),
        revision: format!("{:x}", hasher.finalize()),
        historical_source: Some(HistoricalSource {
            generation,
            ledger_identity,
        }),
        conservation: None,
        plans: Vec::new(),
        derived_bundle_count: 0,
        decomposition_projection: None,
        reconciliation_balance: None,
    })
}

pub(super) fn apply_historical_source(
    conn: &Connection,
    source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    let historical = source
        .historical_source
        .as_ref()
        .context("historical source observation is missing")?;
    crate::db::install_core_from_historical(
        conn,
        historical.generation,
        &historical.ledger_identity,
    )
}

pub(super) fn validate_historical_target(
    conn: &Connection,
    source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    let historical = source
        .historical_source
        .as_ref()
        .context("historical source observation is missing")?;
    if full_generation(conn)? != crate::db::CORE_SCHEMA_VERSION {
        bail!("historical transition did not reach its declared target");
    }
    let unexpected = crate::db::pending_update_change_set(conn)?
        .into_iter()
        .filter(|change| !is_later_generation_change(change))
        .collect::<Vec<_>>();
    if !unexpected.is_empty() {
        bail!("historical transition left an incomplete core structural contract");
    }
    let retirement: Option<String> = conn
        .query_row(
            "select source_ledger_digest from schema_retirement_records where source_generation=?1",
            [historical.generation],
            |row| row.get(0),
        )
        .optional()?;
    if retirement.as_deref() != Some(historical.ledger_identity.as_str()) {
        bail!("historical transition did not record its exact retired source identity");
    }
    let violations: i64 =
        conn.query_row("select count(*) from pragma_foreign_key_check", [], |row| {
            row.get(0)
        })?;
    if violations != 0 {
        bail!("historical transition produced foreign key violations");
    }
    Ok(())
}

pub(super) fn observe_generation_13_repair(
    conn: &Connection,
    context: &TransitionContext<'_>,
) -> Result<SourceObservation> {
    if full_generation(conn)? != crate::db::CORE_SCHEMA_VERSION {
        bail!("core repair source has a different storage header");
    }
    let pending_base_changes = crate::db::pending_update_change_set(conn)?
        .into_iter()
        .filter(|change| !is_later_generation_change(change))
        .collect::<Vec<_>>();
    if pending_base_changes.is_empty() {
        bail!("core repair source has no declared repair");
    }
    let ledger_identity = conservation::semantic_ledger_identity(conn)?;
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/core-repair-source/v1\0");
    hasher.update(context.root.as_os_str().as_encoded_bytes());
    hasher.update(b"\0");
    hasher.update(ledger_identity.as_bytes());
    for change in pending_base_changes {
        hasher.update(change.identity().as_bytes());
        hasher.update(b"\0");
    }
    Ok(SourceObservation {
        descriptor_key: GENERATION_13_REPAIR_SOURCE.key.to_string(),
        revision: format!("{:x}", hasher.finalize()),
        historical_source: None,
        conservation: None,
        plans: Vec::new(),
        derived_bundle_count: 0,
        decomposition_projection: None,
        reconciliation_balance: None,
    })
}

pub(super) fn apply_generation_13_repair(
    conn: &Connection,
    _source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    crate::db::apply_pending_update(conn)
}

pub(super) fn validate_generation_13_repair(
    conn: &Connection,
    _source: &SourceObservation,
    context: &TransitionContext<'_>,
) -> Result<()> {
    observe_generation_13(conn, context)?;
    let violations: i64 =
        conn.query_row("select count(*) from pragma_foreign_key_check", [], |row| {
            row.get(0)
        })?;
    if violations != 0 {
        bail!("core repair produced foreign key violations");
    }
    Ok(())
}

pub(super) fn observe_generation_13(
    conn: &Connection,
    context: &TransitionContext<'_>,
) -> Result<SourceObservation> {
    for table in [
        "projects",
        "work_units",
        "validation_runs",
        "design_versions",
    ] {
        if !table_exists(conn, table)? {
            bail!("source is missing the deployed core aggregate family");
        }
    }
    if all_tables_exist(conn, GENERATION_14_TABLES)? {
        bail!("source already satisfies the next structural descriptor");
    }
    let pending_base_changes = crate::db::pending_update_change_set(conn)?
        .into_iter()
        .filter(|change| !is_later_generation_change(change))
        .collect::<Vec<_>>();
    if !pending_base_changes.is_empty() {
        bail!(
            "source has not reached the deployed core structural descriptor: {}",
            pending_base_changes
                .iter()
                .map(crate::db::PendingUpdateChange::identity)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    let mut exclusions = vec!["schema_migrations"];
    exclusions.extend_from_slice(GENERATION_14_TABLES);
    let conservation = conservation::capture_product_facts(conn, &exclusions)?;
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/storage-source/v1\0");
    hasher.update(13_i64.to_be_bytes());
    hasher.update(context.root.as_os_str().as_encoded_bytes());
    hasher.update(b"\0");
    hasher.update(conservation.digest.as_bytes());
    Ok(SourceObservation {
        descriptor_key: GENERATION_13.key.to_string(),
        revision: format!("{:x}", hasher.finalize()),
        historical_source: None,
        conservation: Some(conservation),
        plans: Vec::new(),
        derived_bundle_count: 0,
        decomposition_projection: None,
        reconciliation_balance: None,
    })
}

pub(super) fn apply_generation_14(
    conn: &Connection,
    _source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    crate::db::install_storage_generation_14(conn)
}

pub(super) fn validate_generation_14(
    conn: &Connection,
    source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    for table in GENERATION_14_TABLES {
        if !table_exists(conn, table)? {
            bail!("target storage is missing a required aggregate family");
        }
    }
    let snapshot = source
        .conservation
        .as_ref()
        .context("source conservation observation is missing")?;
    conservation::verify_product_facts(conn, snapshot)?;
    Ok(())
}

pub(super) fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    conn.query_row(
        "select exists(select 1 from sqlite_schema where type='table' and name=?1)",
        [table],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn all_tables_exist(conn: &Connection, tables: &[&str]) -> Result<bool> {
    for table in tables {
        if !table_exists(conn, table)? {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn generation_17_fields_complete(conn: &Connection) -> Result<bool> {
    for (table, column) in [
        ("decomposition_plans", "document_content"),
        ("decomposition_plans", "content_identity"),
        ("decomposition_plans", "design_package_id"),
        ("decomposition_reconciliation_tasks", "effect"),
        ("decomposition_reconciliation_checklist_items", "effect"),
        ("decomposition_reconciliation_gates", "effect"),
        ("decomposition_reconciliation_gates", "boundary_selector"),
        (
            "decomposition_reconciliation_gates",
            "resolved_boundary_identity",
        ),
        ("decomposition_reconciliation_phases", "effect"),
        ("decomposition_reconciliation_dependencies", "effect"),
    ] {
        if !column_exists(conn, table, column)? {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

pub(super) fn observe_generation_14(
    conn: &Connection,
    context: &TransitionContext<'_>,
) -> Result<SourceObservation> {
    if !all_tables_exist(conn, GENERATION_14_TABLES)? {
        bail!("source is missing a required aggregate family");
    }
    if all_tables_exist(conn, GENERATION_15_TABLES)? {
        bail!("source already satisfies the next structural descriptor");
    }
    let plans = crate::decomposition::discover_plans(context.root)?;
    let derived_bundle_count = crate::decomposition::uncovered_derived_bundle_count(conn, &plans)?;
    let mut exclusions = vec!["schema_migrations"];
    exclusions.extend_from_slice(GENERATION_15_TABLES);
    let conservation = conservation::capture_product_facts(conn, &exclusions)?;
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/storage-source/v1\0");
    hasher.update(14_i64.to_be_bytes());
    hasher.update(context.root.as_os_str().as_encoded_bytes());
    hasher.update(b"\0");
    hasher.update(conservation.digest.as_bytes());
    for plan in &plans {
        hasher.update(plan.source_identity.as_bytes());
        hasher.update(b"\0");
    }
    hasher.update((derived_bundle_count as u64).to_be_bytes());
    Ok(SourceObservation {
        descriptor_key: GENERATION_14.key.to_string(),
        revision: format!("{:x}", hasher.finalize()),
        historical_source: None,
        conservation: Some(conservation),
        plans,
        derived_bundle_count,
        decomposition_projection: None,
        reconciliation_balance: None,
    })
}

pub(super) fn apply_generation_15(
    conn: &Connection,
    source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    crate::db::install_storage_generation_15(conn)?;
    crate::decomposition::install_discovered_plans(conn, &source.plans)?;
    crate::decomposition::install_uncovered_derived_bundles(conn, &source.plans)
}

pub(super) fn validate_generation_15(
    conn: &Connection,
    source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    for table in [
        "decomposition_plans",
        "decomposition_slices",
        "decomposition_slice_dependencies",
        "decomposition_items",
        "decomposition_item_requirements",
        "decomposition_item_checklist_boundaries",
        "decomposition_item_checklist_boundary_gates",
        "decomposition_item_gates",
        "decomposition_applications",
        "decomposition_application_requirements",
        "decomposition_application_boundaries",
        "decomposition_application_gates",
        "decomposition_application_dependencies",
        "decomposition_reconciliation_tasks",
        "decomposition_reconciliation_checklist_items",
        "decomposition_reconciliation_gates",
        "decomposition_reconciliation_phases",
        "decomposition_reconciliation_dependencies",
        "decomposition_reconciliation_applications",
        "decomposition_lineage",
        "decomposition_migration_sources",
        "task_identities",
        "task_revisions",
        "task_revision_requirements",
        "task_revision_aliases",
        "task_phase_memberships",
        "task_phase_membership_sources",
        "task_identity_dependencies",
        "task_identity_dependency_sources",
        "task_completion_claims",
        "task_completion_sources",
        "task_identity_migration_audits",
        "phase_epochs",
        "phase_epoch_sources",
        "phase_epoch_memberships",
        "phase_epoch_membership_sources",
        "phase_epoch_dependencies",
        "phase_epoch_dependency_sources",
        "phase_scope_dispositions",
        "phase_scope_disposition_sources",
    ] {
        if !table_exists(conn, table)? {
            bail!("target storage is missing a required aggregate family");
        }
    }
    let snapshot = source
        .conservation
        .as_ref()
        .context("source conservation observation is missing")?;
    conservation::verify_product_facts(conn, snapshot)?;
    let plan_count: i64 =
        conn.query_row("select count(*) from decomposition_plans", [], |row| {
            row.get(0)
        })?;
    if plan_count != (source.plans.len() + source.derived_bundle_count) as i64 {
        bail!("target decomposition plan count is not conserved");
    }
    for plan in &source.plans {
        let (status, items, slices, boundaries, boundary_gates): (String, i64, i64, i64, i64) = conn.query_row(
            r#"
            select plan.status,
              (select count(*) from decomposition_items item where item.decomposition_plan_id=plan.id),
              (select count(*) from decomposition_slices slice where slice.decomposition_plan_id=plan.id),
              (select count(*) from decomposition_item_checklist_boundaries boundary
               join decomposition_items item on item.id=boundary.decomposition_item_id
               where item.decomposition_plan_id=plan.id),
              (select count(*) from decomposition_item_checklist_boundary_gates gate
               join decomposition_item_checklist_boundaries boundary
                 on boundary.id=gate.decomposition_item_checklist_boundary_id
               join decomposition_items item on item.id=boundary.decomposition_item_id
               where item.decomposition_plan_id=plan.id)
            from decomposition_plans plan where plan.source_identity=?1
            "#,
            [&plan.source_identity],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )?;
        let Some(document) = plan.document.as_ref() else {
            if status != "incomplete"
                || items != 0
                || slices != 0
                || boundaries != 0
                || boundary_gates != 0
            {
                bail!("legacy decomposition document was not preserved as incomplete");
            }
            continue;
        };
        let expected_boundaries = document
            .items
            .iter()
            .map(|item| item.checklist.len() as i64)
            .sum::<i64>();
        let expected_boundary_gates = document
            .items
            .iter()
            .flat_map(|item| &item.checklist)
            .map(|boundary| boundary.gates.len() as i64)
            .sum::<i64>();
        if items != document.items.len() as i64
            || slices != document.slices.len() as i64
            || boundaries != expected_boundaries
            || boundary_gates != expected_boundary_gates
        {
            bail!("target decomposition graph is incomplete");
        }
    }
    Ok(())
}

pub(super) fn observe_generation_15(
    conn: &Connection,
    context: &TransitionContext<'_>,
) -> Result<SourceObservation> {
    if !all_tables_exist(conn, GENERATION_15_TABLES)? {
        bail!("source is missing a required aggregate family");
    }
    if all_tables_exist(conn, GENERATION_16_TABLES)? {
        bail!("source already satisfies the next structural descriptor");
    }
    let mut exclusions = vec!["schema_migrations"];
    exclusions.extend_from_slice(GENERATION_16_TABLES);
    let conservation = conservation::capture_product_facts(conn, &exclusions)?;
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/storage-source/v1\0");
    hasher.update(15_i64.to_be_bytes());
    hasher.update(context.root.as_os_str().as_encoded_bytes());
    hasher.update(b"\0");
    hasher.update(conservation.digest.as_bytes());
    Ok(SourceObservation {
        descriptor_key: GENERATION_15.key.to_string(),
        revision: format!("{:x}", hasher.finalize()),
        historical_source: None,
        conservation: Some(conservation),
        plans: Vec::new(),
        derived_bundle_count: 0,
        decomposition_projection: None,
        reconciliation_balance: None,
    })
}

pub(super) fn apply_generation_16(
    conn: &Connection,
    _source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    crate::db::install_storage_generation_16(conn)
}

pub(super) fn validate_generation_16(
    conn: &Connection,
    source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    for table in [
        "release_candidate_revisions",
        "release_candidate_subject_revisions",
        "release_candidate_attempts",
    ] {
        if !table_exists(conn, table)? {
            bail!("target storage is missing a required aggregate family");
        }
    }
    let snapshot = source
        .conservation
        .as_ref()
        .context("source conservation observation is missing")?;
    conservation::verify_product_facts(conn, snapshot)?;
    validate_generation_16_release_contract(conn)?;
    let (candidates, current_revisions, revisions_without_subjects): (i64, i64, i64) = conn
        .query_row(
            r#"
            select
              (select count(*) from release_candidates),
              (select count(*) from release_candidate_revisions where head_state='current'),
              (select count(*) from release_candidate_revisions revision
               where not exists(
                 select 1 from release_candidate_subject_revisions subject
                 where subject.release_candidate_revision_id=revision.id
               ))
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
    if candidates != current_revisions || revisions_without_subjects != 0 {
        bail!("release candidate lifecycle migration is incomplete");
    }
    Ok(())
}

pub(super) const RECONCILIATION_MAPPING_TABLES: [&str; 5] = [
    "decomposition_reconciliation_tasks",
    "decomposition_reconciliation_checklist_items",
    "decomposition_reconciliation_gates",
    "decomposition_reconciliation_phases",
    "decomposition_reconciliation_dependencies",
];

// These five tables are the complete owned decomposition endpoint mapping domain.

pub(super) fn observe_generation_16(
    conn: &Connection,
    context: &TransitionContext<'_>,
) -> Result<SourceObservation> {
    validate_generation_16_update_structure(conn)?;
    if !all_tables_exist(conn, GENERATION_16_TABLES)? {
        bail!("source is missing the deployed release lifecycle family");
    }
    if generation_17_fields_complete(conn)? {
        bail!("source already satisfies the next structural descriptor");
    }
    let conservation = conservation::capture_product_facts(
        conn,
        &[
            "schema_migrations",
            "decomposition_plans",
            "decomposition_reconciliation_tasks",
            "decomposition_reconciliation_checklist_items",
            "decomposition_reconciliation_gates",
            "decomposition_reconciliation_phases",
            "decomposition_reconciliation_dependencies",
        ],
    )?;
    let decomposition_projection = reconciliation_projection_digest(conn)?;
    let reconciliation_balance = reconciliation_balance(conn)?;
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/storage-source/v1\0");
    hasher.update(16_i64.to_be_bytes());
    hasher.update(context.root.as_os_str().as_encoded_bytes());
    hasher.update(b"\0");
    hasher.update(conservation.digest.as_bytes());
    hasher.update(b"\0");
    hasher.update(decomposition_projection.as_bytes());
    Ok(SourceObservation {
        descriptor_key: GENERATION_16.key.to_string(),
        revision: format!("{:x}", hasher.finalize()),
        historical_source: None,
        conservation: Some(conservation),
        plans: Vec::new(),
        derived_bundle_count: 0,
        decomposition_projection: Some(decomposition_projection),
        reconciliation_balance: Some(reconciliation_balance),
    })
}

pub(super) fn validate_generation_16_update_structure(conn: &Connection) -> Result<()> {
    // Source recognition is deliberately structural. Row-level product policy is
    // validated after the staged transition and must not turn a known deployed
    // shape into an unrelated recovery choice.
    for table in [
        "design_versions",
        "validation_gates",
        "decomposition_plans",
        "decomposition_reconciliation_tasks",
        "decomposition_reconciliation_checklist_items",
        "decomposition_reconciliation_gates",
        "decomposition_reconciliation_phases",
        "decomposition_reconciliation_dependencies",
    ] {
        if !table_exists(conn, table)? {
            bail!("source is missing a required deployed aggregate family");
        }
    }
    Ok(())
}

pub(super) fn validate_generation_16_release_contract(conn: &Connection) -> Result<()> {
    for table in [
        "release_candidates",
        "release_candidate_revisions",
        "release_candidate_subject_revisions",
        "release_candidate_attempts",
    ] {
        if !table_exists(conn, table)? {
            bail!("source is missing a deployed release aggregate family");
        }
    }
    let invalid_heads: i64 = conn.query_row(
        r#"
        select count(*) from release_candidates candidate
        where (select count(*) from release_candidate_revisions revision
               where revision.release_candidate_id=candidate.id and revision.head_state='current') != 1
        "#,
        [],
        |row| row.get(0),
    )?;
    let revisions_without_subjects: i64 = conn.query_row(
        r#"
        select count(*) from release_candidate_revisions revision
        where not exists(
          select 1 from release_candidate_subject_revisions subject
          where subject.release_candidate_revision_id=revision.id
        )
        "#,
        [],
        |row| row.get(0),
    )?;
    let incompatible_subjects: i64 = conn.query_row(
        r#"
        select count(*)
        from release_candidate_revisions revision
        join release_candidates candidate on candidate.id=revision.release_candidate_id
        where revision.project_id!=candidate.project_id
           or exists(
             select 1 from release_candidate_subject_revisions subject
             where subject.release_candidate_revision_id=revision.id
               and subject.project_id!=revision.project_id
           )
           or exists(
             select 1 from release_candidate_assets asset
             where asset.release_candidate_id=candidate.id
               and not exists(
                 select 1 from release_candidate_subject_revisions subject
                 where subject.release_candidate_revision_id=revision.id
                   and subject.subject_name=asset.asset_name
                   and subject.expected_identity=asset.expected_identity
               )
           )
        "#,
        [],
        |row| row.get(0),
    )?;
    crate::release::validate_release_candidate_lineage(conn)?;
    let invalid_lineage_revisions: i64 = conn.query_row(
        r#"
        select count(*)
        from release_candidates candidate
        left join release_candidate_revisions current
          on current.release_candidate_id=candidate.id and current.head_state='current'
        where current.id is null
           or candidate.status!=current.state
           or (candidate.status='superseded' and (
                 current.stage!='terminal'
                 or current.action!='supersede'
              ))
           or (candidate.status!='superseded' and current.action='supersede')
        "#,
        [],
        |row| row.get(0),
    )?;
    let invalid_attempts: i64 = conn.query_row(
        r#"
        select count(*)
        from release_candidate_attempts attempt
        left join release_candidate_revisions expected
          on expected.release_candidate_id=attempt.release_candidate_id
         and expected.revision_handle=attempt.expected_current
        left join release_candidate_revisions result
          on result.release_candidate_id=attempt.release_candidate_id
         and result.revision_handle=attempt.result_revision_handle
        where (attempt.status='requested' and attempt.result_revision_handle is not null)
           or (attempt.status='completed' and
               (attempt.observed_identity is null
                or length(trim(attempt.observed_identity))=0
                or attempt.result_revision_handle is null
                or expected.id is null or result.id is null
                or result.predecessor_id is not expected.id
                or (result.state='withdrawn' and (
                  attempt.action not in ('withdraw','retry','reconcile')
                  or result.action not in ('withdraw','reconcile')
                  or attempt.requested_identity!=attempt.observed_identity
                  or length(attempt.requested_identity)!=64
                  or attempt.requested_identity glob '*[^0-9a-f]*'
                ))
                or (result.action!=attempt.action and not (
                  result.action='reconcile' and exists(
                    select 1 from release_candidate_attempts resolution
                    where resolution.release_candidate_id=attempt.release_candidate_id
                      and resolution.action='reconcile'
                      and resolution.status='completed'
                      and resolution.result_revision_handle=result.revision_handle
                  )
                ))
                or exists(
                  select 1 from release_candidate_subject_revisions expected_subject
                  where expected_subject.release_candidate_revision_id=expected.id
                    and not exists(
                      select 1 from release_candidate_subject_revisions result_subject
                      where result_subject.release_candidate_revision_id=result.id
                        and result_subject.subject_kind=expected_subject.subject_kind
                        and result_subject.subject_name=expected_subject.subject_name
                    )
                )))
           or (attempt.status='requested' and attempt.observed_identity is not null
               and length(trim(attempt.observed_identity))=0)
        "#,
        [],
        |row| row.get(0),
    )?;
    if invalid_heads != 0
        || revisions_without_subjects != 0
        || incompatible_subjects != 0
        || invalid_lineage_revisions != 0
        || invalid_attempts != 0
    {
        bail!("source does not satisfy the deployed release revision and attempt contract");
    }
    Ok(())
}

pub(super) fn validate_generation_16_reconciliation_shape(conn: &Connection) -> Result<()> {
    let duplicate_slots: i64 = conn.query_row(
        r#"
        select count(*) from (
          select plan.project_id,version.design_package_id,plan.work_unit_id
          from decomposition_plans plan
          join design_versions version on version.id=plan.design_version_id
          where plan.status!='superseded' and plan.work_unit_id is not null
          group by plan.project_id,version.design_package_id,plan.work_unit_id
          having sum(case when plan.status='applied' then 1 else 0 end)>1
              or sum(case when plan.status in ('draft','incomplete','ready') then 1 else 0 end)>1
        )
        "#,
        [],
        |row| row.get(0),
    )?;
    if duplicate_slots != 0 {
        bail!("source Decomposition Plan package-lineage slot is ambiguous");
    }
    let invalid_owned_relations: i64 = conn.query_row(
        r#"
        select
          (select count(*) from decomposition_items item
           join decomposition_slices slice on slice.id=item.slice_id
           where slice.decomposition_plan_id!=item.decomposition_plan_id)
        + (select count(*) from decomposition_slice_dependencies dependency
           join decomposition_slices source on source.id=dependency.predecessor_slice_id
           join decomposition_slices target on target.id=dependency.successor_slice_id
           where source.decomposition_plan_id!=dependency.decomposition_plan_id
              or target.decomposition_plan_id!=dependency.decomposition_plan_id)
        + (select count(*) from decomposition_applications application
           join decomposition_items item on item.id=application.decomposition_item_id
           where item.decomposition_plan_id!=application.decomposition_plan_id)
        + (select count(*) from decomposition_migration_sources source
           join decomposition_items item on item.id=source.decomposition_item_id
           where item.decomposition_plan_id!=source.decomposition_plan_id)
        + (select count(*) from decomposition_lineage lineage
           join decomposition_items predecessor on predecessor.id=lineage.predecessor_item_id
           left join decomposition_items successor on successor.id=lineage.successor_item_id
           where predecessor.decomposition_plan_id!=lineage.predecessor_plan_id
              or (successor.id is not null and successor.decomposition_plan_id!=lineage.successor_plan_id))
        + (select count(*) from decomposition_reconciliation_applications application
           join decomposition_plans successor on successor.id=application.successor_plan_id
           where successor.predecessor_id is not application.predecessor_plan_id)
        "#,
        [],
        |row| row.get(0),
    )?;
    if invalid_owned_relations != 0 {
        bail!("source Decomposition Plan aggregate has a foreign or contradictory relation");
    }
    let mappings = [
        ("decomposition_reconciliation_tasks", "successor_item_id"),
        (
            "decomposition_reconciliation_checklist_items",
            "successor_boundary_id",
        ),
        (
            "decomposition_reconciliation_gates",
            "successor_item_gate_id",
        ),
        ("decomposition_reconciliation_phases", "successor_slice_id"),
        (
            "decomposition_reconciliation_dependencies",
            "successor_dependency_id",
        ),
    ];
    for (table, target) in mappings {
        let invalid: i64 = conn.query_row(
            &format!(
                "select count(*) from {} where (disposition='retained' and ({} is null or reason is not null)) or (disposition='retired' and ({} is not null or nullif(trim(reason),'') is null))",
                quote_identifier(table),
                quote_identifier(target),
                quote_identifier(target),
            ),
            [],
            |row| row.get(0),
        )?;
        if invalid != 0 {
            bail!("source reconciliation mapping is partial or contradictory");
        }
    }
    let foreign_key_violations: i64 =
        conn.query_row("select count(*) from pragma_foreign_key_check", [], |row| {
            row.get(0)
        })?;
    if foreign_key_violations != 0 {
        bail!("source reconciliation mapping has a foreign endpoint");
    }
    Ok(())
}

pub(super) fn apply_generation_17(
    conn: &Connection,
    _source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    crate::db::install_storage_generation_17(conn)?;
    backfill_reconciliation_gate_boundaries(conn)?;
    backfill_owned_plan_documents(conn)
}

pub(super) fn validate_generation_17(
    conn: &Connection,
    source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    if full_generation(conn)? != 17 {
        bail!("target storage generation is not the declared successor");
    }
    validate_generation_16_release_contract(conn)?;
    for table in RECONCILIATION_MAPPING_TABLES {
        if !column_exists(conn, table, "effect")? {
            bail!("target storage is missing a reconciliation effect field");
        }
        let invalid: i64 = conn.query_row(
            &format!(
                "select count(*) from {} where (disposition='retained' and effect not in ('preserve','open')) or (disposition='retired' and effect is not null)",
                quote_identifier(table)
            ),
            [],
            |row| row.get(0),
        )?;
        let inferred_open: i64 = conn.query_row(
            &format!(
                "select count(*) from {} where effect='open'",
                quote_identifier(table)
            ),
            [],
            |row| row.get(0),
        )?;
        if invalid != 0 || inferred_open != 0 {
            bail!("target reconciliation effects do not satisfy the declared mapping");
        }
    }
    validate_owned_plan_documents(conn)?;
    let invalid_gate_boundaries: i64 = conn.query_row(
        r#"
        select count(*) from decomposition_reconciliation_gates
        where (disposition='retained' and
               (boundary_selector is null or resolved_boundary_identity is null))
           or (disposition='retired' and
               (boundary_selector is not null or resolved_boundary_identity is not null))
        "#,
        [],
        |row| row.get(0),
    )?;
    if invalid_gate_boundaries != 0 {
        bail!("target reconciliation gates do not own exact validation boundaries");
    }
    validate_reconciliation_gate_boundaries(conn)?;
    validate_current_generation_17(conn)?;
    let expected_projection = source
        .decomposition_projection
        .as_ref()
        .context("source reconciliation projection is missing")?;
    if reconciliation_projection_digest(conn)? != *expected_projection {
        bail!("reconciliation history was not conserved while adding effects");
    }
    if reconciliation_balance(conn)?
        != source
            .reconciliation_balance
            .context("source reconciliation balance is missing")?
    {
        bail!("reconciliation mapping balance was not conserved while adding effects");
    }
    let snapshot = source
        .conservation
        .as_ref()
        .context("source conservation observation is missing")?;
    conservation::verify_product_facts(conn, snapshot)?;
    Ok(())
}

pub(super) fn observe_generation_17(
    conn: &Connection,
    context: &TransitionContext<'_>,
) -> Result<SourceObservation> {
    if full_generation(conn)? != 17 {
        bail!("source is not the declared predecessor storage generation");
    }
    validate_current_generation_17(conn)?;
    if table_exists(conn, "decomposition_reconciliation_results")? {
        bail!("source already satisfies the next structural descriptor");
    }
    let conservation = conservation::capture_product_facts(
        conn,
        &["schema_migrations", "decomposition_reconciliation_results"],
    )?;
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/storage-source/v1\0");
    hasher.update(17_i64.to_be_bytes());
    hasher.update(context.root.as_os_str().as_encoded_bytes());
    hasher.update(b"\0");
    hasher.update(conservation.digest.as_bytes());
    Ok(SourceObservation {
        descriptor_key: GENERATION_17.key.to_string(),
        revision: format!("{:x}", hasher.finalize()),
        historical_source: None,
        conservation: Some(conservation),
        plans: Vec::new(),
        derived_bundle_count: 0,
        decomposition_projection: None,
        reconciliation_balance: None,
    })
}

pub(super) fn apply_generation_18(
    conn: &Connection,
    _source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    crate::db::install_storage_generation_18(conn)
}

pub(super) fn validate_generation_18(
    conn: &Connection,
    source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    if full_generation(conn)? != 18 {
        bail!("target storage generation is not the declared successor");
    }
    validate_current_generation_18(conn)?;
    let snapshot = source
        .conservation
        .as_ref()
        .context("source conservation observation is missing")?;
    conservation::verify_product_facts(conn, snapshot)
}

pub(super) fn observe_generation_18(
    conn: &Connection,
    context: &TransitionContext<'_>,
) -> Result<SourceObservation> {
    if full_generation(conn)? != 18 {
        bail!("source is not the declared predecessor storage generation");
    }
    validate_current_generation_18(conn)?;
    let conservation = conservation::capture_product_facts(conn, &["schema_migrations"])?;
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/storage-source/v1\0");
    hasher.update(18_i64.to_be_bytes());
    hasher.update(context.root.as_os_str().as_encoded_bytes());
    hasher.update(b"\0");
    hasher.update(conservation.digest.as_bytes());
    Ok(SourceObservation {
        descriptor_key: GENERATION_18.key.to_string(),
        revision: format!("{:x}", hasher.finalize()),
        historical_source: None,
        conservation: Some(conservation),
        plans: Vec::new(),
        derived_bundle_count: 0,
        decomposition_projection: None,
        reconciliation_balance: None,
    })
}

pub(super) fn apply_generation_19(
    conn: &Connection,
    _source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    crate::db::install_storage_generation_19(conn)
}

pub(super) fn validate_generation_19(
    conn: &Connection,
    source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    if full_generation(conn)? != 19 {
        bail!("target storage generation is not the declared successor");
    }
    validate_current_generation_19(conn)?;
    let snapshot = source
        .conservation
        .as_ref()
        .context("source conservation observation is missing")?;
    conservation::verify_product_facts(conn, snapshot)
}

pub(super) fn observe_generation_19(
    conn: &Connection,
    context: &TransitionContext<'_>,
) -> Result<SourceObservation> {
    if full_generation(conn)? != 19 {
        bail!("source is not the declared predecessor storage generation");
    }
    validate_current_generation_19(conn)?;
    let conservation = conservation::capture_product_facts(conn, &["schema_migrations"])?;
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/storage-source/v1\0");
    hasher.update(19_i64.to_be_bytes());
    hasher.update(context.root.as_os_str().as_encoded_bytes());
    hasher.update(b"\0");
    hasher.update(conservation.digest.as_bytes());
    Ok(SourceObservation {
        descriptor_key: GENERATION_19.key.to_string(),
        revision: format!("{:x}", hasher.finalize()),
        historical_source: None,
        conservation: Some(conservation),
        plans: Vec::new(),
        derived_bundle_count: 0,
        decomposition_projection: None,
        reconciliation_balance: None,
    })
}

pub(super) fn apply_generation_20(
    conn: &Connection,
    _source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    crate::db::install_storage_generation_20(conn)
}

pub(super) fn validate_generation_20(
    conn: &Connection,
    source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    if full_generation(conn)? != 20 {
        bail!("target storage generation is not the declared successor");
    }
    validate_current_generation_20(conn)?;
    let snapshot = source
        .conservation
        .as_ref()
        .context("source conservation observation is missing")?;
    conservation::verify_product_facts(conn, snapshot)
}

pub(super) fn observe_generation_20(
    conn: &Connection,
    context: &TransitionContext<'_>,
) -> Result<SourceObservation> {
    if full_generation(conn)? != 20 {
        bail!("source storage generation is not 20");
    }
    validate_current_generation_20(conn)?;
    if table_exists(conn, "finding_design_recoveries")? {
        bail!("generation-20 source contains an undeclared terminal recovery relation");
    }
    let conservation = conservation::capture_product_facts(conn, &["schema_migrations"])?;
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/storage-source/v1\0");
    hasher.update(20_i64.to_be_bytes());
    hasher.update(context.root.as_os_str().as_encoded_bytes());
    hasher.update(b"\0");
    hasher.update(conservation.digest.as_bytes());
    Ok(SourceObservation {
        descriptor_key: GENERATION_20.key.to_string(),
        revision: format!("{:x}", hasher.finalize()),
        historical_source: None,
        conservation: Some(conservation),
        plans: Vec::new(),
        derived_bundle_count: 0,
        decomposition_projection: None,
        reconciliation_balance: None,
    })
}

pub(super) fn apply_generation_21(
    conn: &Connection,
    _source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    crate::db::install_storage_generation_21(conn)
}

pub(super) fn validate_generation_21(
    conn: &Connection,
    source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    if full_generation(conn)? != 21 {
        bail!("target storage generation is not the declared successor");
    }
    validate_current_generation_21(conn)?;
    let snapshot = source
        .conservation
        .as_ref()
        .context("source conservation observation is missing")?;
    conservation::verify_product_facts(conn, snapshot)
}

pub(super) fn observe_generation_21(
    conn: &Connection,
    context: &TransitionContext<'_>,
) -> Result<SourceObservation> {
    if full_generation(conn)? != 21 {
        bail!("source storage generation is not 21");
    }
    validate_current_generation_21(conn)?;
    let target_changes = crate::db::inspect_pending_reconciliation_target_migrations(conn)?;
    let conservation = conservation::capture_product_facts_with_text_mutations(
        conn,
        &["schema_migrations"],
        target_changes
            .changes
            .into_iter()
            .map(|change| conservation::DeclaredTextMutation {
                table: "correction_tokens".to_string(),
                key_column: "id".to_string(),
                key_value: change.token_id,
                column: "target".to_string(),
                before: change.before,
                after: change.after,
            })
            .collect(),
    )?;
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/storage-source/v1\0");
    hasher.update(21_i64.to_be_bytes());
    hasher.update(context.root.as_os_str().as_encoded_bytes());
    hasher.update(b"\0");
    hasher.update(conservation.digest.as_bytes());
    Ok(SourceObservation {
        descriptor_key: GENERATION_21.key.to_string(),
        revision: format!("{:x}", hasher.finalize()),
        historical_source: None,
        conservation: Some(conservation),
        plans: Vec::new(),
        derived_bundle_count: 0,
        decomposition_projection: None,
        reconciliation_balance: None,
    })
}

pub(super) fn apply_generation_22(
    conn: &Connection,
    _source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    crate::db::install_storage_generation_22(conn)
}

pub(super) fn validate_generation_22(
    conn: &Connection,
    source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    if full_generation(conn)? != 22 {
        bail!("target storage generation is not the declared successor");
    }
    validate_current_generation_22(conn)?;
    let snapshot = source
        .conservation
        .as_ref()
        .context("source conservation observation is missing")?;
    conservation::verify_product_facts(conn, snapshot)
}

pub(super) fn observe_generation_22(
    conn: &Connection,
    context: &TransitionContext<'_>,
) -> Result<SourceObservation> {
    if full_generation(conn)? != 22 {
        bail!("source storage generation is not 22");
    }
    validate_current_generation_22(conn)?;
    let conservation = conservation::capture_product_facts(conn, &["schema_migrations"])?;
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/storage-source/v1\0");
    hasher.update(22_i64.to_be_bytes());
    hasher.update(context.root.as_os_str().as_encoded_bytes());
    hasher.update(b"\0");
    hasher.update(conservation.digest.as_bytes());
    Ok(SourceObservation {
        descriptor_key: GENERATION_22.key.to_string(),
        revision: format!("{:x}", hasher.finalize()),
        historical_source: None,
        conservation: Some(conservation),
        plans: Vec::new(),
        derived_bundle_count: 0,
        decomposition_projection: None,
        reconciliation_balance: None,
    })
}

pub(super) fn apply_generation_23(
    conn: &Connection,
    _source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    crate::db::install_storage_generation_23(conn)
}

pub(super) fn validate_generation_23(
    conn: &Connection,
    source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    if full_generation(conn)? != 23 {
        bail!("target storage generation is not the declared successor");
    }
    validate_current_generation_23(conn)?;
    let snapshot = source
        .conservation
        .as_ref()
        .context("source conservation observation is missing")?;
    conservation::verify_product_facts(conn, snapshot)
}

pub(super) fn observe_generation_23(
    conn: &Connection,
    context: &TransitionContext<'_>,
) -> Result<SourceObservation> {
    if full_generation(conn)? != 23 {
        bail!("source storage generation is not 23");
    }
    validate_current_generation_23(conn)?;
    let conservation = conservation::capture_product_facts(conn, &["schema_migrations"])?;
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/storage-source/v1\0");
    hasher.update(23_i64.to_be_bytes());
    hasher.update(context.root.as_os_str().as_encoded_bytes());
    hasher.update(b"\0");
    hasher.update(conservation.digest.as_bytes());
    Ok(SourceObservation {
        descriptor_key: GENERATION_23.key.to_string(),
        revision: format!("{:x}", hasher.finalize()),
        historical_source: None,
        conservation: Some(conservation),
        plans: Vec::new(),
        derived_bundle_count: 0,
        decomposition_projection: None,
        reconciliation_balance: None,
    })
}

pub(super) fn apply_generation_24(
    conn: &Connection,
    _source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    crate::db::install_storage_generation_24(conn)
}

pub(super) fn validate_generation_24(
    conn: &Connection,
    source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    if full_generation(conn)? != 24 {
        bail!("target storage generation is not the declared successor");
    }
    validate_current_generation_24(conn)?;
    let snapshot = source
        .conservation
        .as_ref()
        .context("source conservation observation is missing")?;
    conservation::verify_product_facts(conn, snapshot)
}

pub(super) fn observe_generation_24(
    conn: &Connection,
    context: &TransitionContext<'_>,
) -> Result<SourceObservation> {
    if full_generation(conn)? != 24 {
        bail!("source storage generation is not 24");
    }
    validate_current_generation_24(conn)?;
    let conservation = conservation::capture_product_facts(
        conn,
        &[
            "schema_migrations",
            "owner_decisions",
            "review_agent_invocations",
            "legacy_review_acceptance_migrations",
            "legacy_signed_review_effects",
        ],
    )?;
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/storage-source/v1\0");
    hasher.update(24_i64.to_be_bytes());
    hasher.update(context.root.as_os_str().as_encoded_bytes());
    hasher.update(b"\0");
    hasher.update(conservation.digest.as_bytes());
    Ok(SourceObservation {
        descriptor_key: GENERATION_24.key.to_string(),
        revision: format!("{:x}", hasher.finalize()),
        historical_source: None,
        conservation: Some(conservation),
        plans: Vec::new(),
        derived_bundle_count: 0,
        decomposition_projection: None,
        reconciliation_balance: None,
    })
}

pub(super) fn apply_generation_25(
    conn: &Connection,
    _source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    crate::db::install_storage_generation_25(conn)
}

pub(super) fn validate_generation_25(
    conn: &Connection,
    source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    if full_generation(conn)? != 25 {
        bail!("target storage generation is not the declared successor");
    }
    validate_current_generation_25(conn)?;
    let snapshot = source
        .conservation
        .as_ref()
        .context("source conservation observation is missing")?;
    conservation::verify_product_facts(conn, snapshot)
}

pub(super) fn observe_generation_25(
    conn: &Connection,
    context: &TransitionContext<'_>,
) -> Result<SourceObservation> {
    if full_generation(conn)? != 25 {
        bail!("source storage generation is not 25");
    }
    validate_current_generation_25(conn)?;
    let count_mutations = conn
        .prepare(
            r#"
            select run.id,run.new_findings_count,count(finding.id)
            from review_runs run
            left join findings finding on finding.review_run_id=run.id
            where run.status='completed'
               or run.status in ('requested','running')
            group by run.id
            having run.new_findings_count!=count(finding.id)
               and (
                   run.status='completed'
                   or (
                       run.status in ('requested','running')
                       and count(finding.id)>0
                       and count(finding.id)>=run.new_findings_count
                   )
               )
            order by run.id
            "#,
        )?
        .query_map([], |row| {
            Ok(conservation::DeclaredIntegerMutation {
                table: "review_runs".to_string(),
                key_column: "id".to_string(),
                key_value: row.get(0)?,
                column: "new_findings_count".to_string(),
                before: row.get(1)?,
                after: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut status_mutations = conn
        .prepare(
            r#"
            select run.id,run.status
            from review_runs run
            where run.status in ('requested','running')
              and (select count(*) from findings where review_run_id=run.id)>0
              and (select count(*) from findings where review_run_id=run.id)
                    >=run.new_findings_count
            order by run.id
            "#,
        )?
        .query_map([], |row| {
            Ok(conservation::DeclaredTextMutation {
                table: "review_runs".to_string(),
                key_column: "id".to_string(),
                key_value: row.get(0)?,
                column: "status".to_string(),
                before: row.get(1)?,
                after: "completed".to_string(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let invocation_status_mutations = conn
        .prepare(
            r#"
            select invocation.id,invocation.status
            from review_agent_invocations invocation
            join review_runs run on run.id=invocation.review_run_id
            where invocation.status in ('requested','running')
              and run.status in ('requested','running')
              and (select count(*) from findings where review_run_id=run.id)>0
              and (select count(*) from findings where review_run_id=run.id)
                    >=run.new_findings_count
            order by invocation.id
            "#,
        )?
        .query_map([], |row| {
            Ok(conservation::DeclaredTextMutation {
                table: "review_agent_invocations".to_string(),
                key_column: "id".to_string(),
                key_value: row.get(0)?,
                column: "status".to_string(),
                before: row.get(1)?,
                after: "completed".to_string(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    status_mutations.extend(invocation_status_mutations);
    let conservation = conservation::capture_product_facts_with_mutations(
        conn,
        &["schema_migrations"],
        status_mutations,
        count_mutations,
    )?;
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/storage-source/v1\0");
    hasher.update(25_i64.to_be_bytes());
    hasher.update(context.root.as_os_str().as_encoded_bytes());
    hasher.update(b"\0");
    hasher.update(conservation.digest.as_bytes());
    Ok(SourceObservation {
        descriptor_key: GENERATION_25.key.to_string(),
        revision: format!("{:x}", hasher.finalize()),
        historical_source: None,
        conservation: Some(conservation),
        plans: Vec::new(),
        derived_bundle_count: 0,
        decomposition_projection: None,
        reconciliation_balance: None,
    })
}

pub(super) fn apply_generation_26(
    conn: &Connection,
    _source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    crate::db::install_storage_generation_26(conn)
}

pub(super) fn validate_generation_26(
    conn: &Connection,
    source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    if full_generation(conn)? != 26 {
        bail!("target storage generation is not the declared successor");
    }
    validate_current_generation_26(conn)?;
    let snapshot = source
        .conservation
        .as_ref()
        .context("source conservation observation is missing")?;
    conservation::verify_product_facts(conn, snapshot)
}
