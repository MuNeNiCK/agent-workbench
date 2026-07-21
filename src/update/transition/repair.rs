use super::*;

pub(super) fn full_generation(conn: &Connection) -> Result<i64> {
    let has_reset: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='table' and name='schema_metadata')",
        [],
        |row| row.get(0),
    )?;
    if has_reset {
        bail!("full storage cannot contain a recovery header");
    }
    conn.query_row("select max(version) from schema_migrations", [], |row| {
        row.get(0)
    })
    .map_err(Into::into)
}

pub(super) fn observe_current_repair_source(
    conn: &Connection,
    context: &TransitionContext<'_>,
) -> Result<SourceObservation> {
    if full_generation(conn)? != crate::db::SCHEMA_VERSION {
        bail!("registered current repair requires the current storage generation");
    }
    let pending = crate::db::pending_update_change_set(conn)?;
    if pending.is_empty() {
        bail!("current project state has no pending transition");
    }
    let schema = classify_storage_header(conn)?.generation();
    let conservation = conservation::capture_named_product_facts(
        conn,
        &[
            "kpt_reviews",
            "kpt_items",
            "kpt_item_conversions",
            "kpt_item_sources",
            "kpt_rules",
            "kpt_item_dismissals",
        ],
    )?;
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/update-source/v1\0");
    hasher.update(schema.to_be_bytes());
    hasher.update(context.root.as_os_str().as_encoded_bytes());
    hasher.update(b"\0");
    for change in pending {
        hasher.update(change.identity().as_bytes());
        hasher.update(b"\0");
    }
    hasher.update(conservation.digest.as_bytes());
    Ok(SourceObservation {
        descriptor_key: CURRENT_REPAIR_SOURCE.key.to_string(),
        revision: format!("{:x}", hasher.finalize()),
        historical_source: None,
        conservation: Some(conservation),
        plans: Vec::new(),
        derived_bundle_count: 0,
        decomposition_projection: None,
        reconciliation_balance: None,
    })
}

pub(super) fn apply_current_profile_transition(
    conn: &Connection,
    _source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    crate::db::apply_pending_update(conn)?;
    crate::db::migrate(conn)
}

pub(super) fn validate_repaired_current(
    conn: &Connection,
    source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> Result<()> {
    if full_generation(conn)? != crate::db::SCHEMA_VERSION {
        bail!("registered repair did not restore the current storage generation");
    }
    let pending = crate::db::pending_update_change_set(conn)?;
    if !pending.is_empty() {
        bail!(
            "registered repair left undeclared source facts: {}",
            pending
                .iter()
                .map(crate::db::PendingUpdateChange::identity)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    conservation::verify_product_facts(
        conn,
        source
            .conservation
            .as_ref()
            .context("registered repair is missing its KPT conservation snapshot")?,
    )
    .context("registered repair did not conserve KPT product facts")?;
    let violations = conn
        .prepare("select \"table\",rowid,parent from pragma_foreign_key_check order by \"table\",rowid,parent")?
        .query_map([], |row| {
            Ok(format!(
                "{}:{}->{}",
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if !violations.is_empty() {
        bail!(
            "repaired current storage has foreign key violations: {}",
            violations.join(",")
        );
    }
    Ok(())
}

pub(super) fn is_unrepairable_current_change(change: &crate::db::PendingUpdateChange) -> bool {
    matches!(
        change,
        crate::db::PendingUpdateChange::SchemaGeneration { .. }
            | crate::db::PendingUpdateChange::DecompositionPlanHeadNormalization
            | crate::db::PendingUpdateChange::MissingTable(
                "update_operations"
                    | "update_decisions"
                    | "update_receipts"
                    | "release_candidates"
                    | "release_candidate_assets"
                    | "release_candidate_events"
                    | "release_candidate_revisions"
                    | "release_candidate_subject_revisions"
                    | "release_candidate_attempts"
                    | "release_candidate_boundaries"
                    | "decomposition_plans"
                    | "decomposition_slices"
                    | "decomposition_slice_dependencies"
                    | "decomposition_items"
                    | "decomposition_item_requirements"
                    | "decomposition_item_checklist_boundaries"
                    | "decomposition_item_gates"
                    | "decomposition_applications"
                    | "decomposition_lineage"
                    | "decomposition_migration_sources"
            )
    )
}

pub(super) fn is_later_generation_change(change: &crate::db::PendingUpdateChange) -> bool {
    match change {
        crate::db::PendingUpdateChange::SchemaGeneration { source, target } => {
            *source == crate::db::CORE_SCHEMA_VERSION && *target == crate::db::SCHEMA_VERSION
        }
        crate::db::PendingUpdateChange::MissingTable(table) => matches!(
            *table,
            "update_operations"
                | "update_decisions"
                | "update_receipts"
                | "release_candidates"
                | "release_candidate_assets"
                | "release_candidate_events"
                | "release_candidate_revisions"
                | "release_candidate_subject_revisions"
                | "release_candidate_attempts"
                | "release_candidate_boundaries"
                | "decomposition_plans"
                | "decomposition_plan_ingress_identities"
                | "decomposition_slices"
                | "decomposition_slice_dependencies"
                | "decomposition_items"
                | "decomposition_item_requirements"
                | "decomposition_item_checklist_boundaries"
                | "decomposition_item_checklist_boundary_gates"
                | "decomposition_item_gates"
                | "decomposition_applications"
                | "decomposition_application_requirements"
                | "decomposition_application_boundaries"
                | "decomposition_application_gates"
                | "decomposition_application_dependencies"
                | "decomposition_reconciliation_tasks"
                | "decomposition_reconciliation_checklist_items"
                | "decomposition_reconciliation_gates"
                | "decomposition_reconciliation_phases"
                | "decomposition_reconciliation_dependencies"
                | "decomposition_reconciliation_applications"
                | "decomposition_reconciliation_results"
                | "decomposition_lineage"
                | "decomposition_migration_sources"
                | "task_identities"
                | "task_revisions"
                | "task_revision_requirements"
                | "task_revision_aliases"
                | "task_phase_memberships"
                | "task_phase_membership_sources"
                | "task_identity_dependencies"
                | "task_identity_dependency_sources"
                | "task_completion_claims"
                | "task_completion_sources"
                | "task_identity_migration_audits"
                | "phase_epochs"
                | "phase_epoch_sources"
                | "phase_epoch_memberships"
                | "phase_epoch_membership_sources"
                | "phase_epoch_dependencies"
                | "phase_epoch_dependency_sources"
                | "phase_scope_dispositions"
                | "phase_scope_disposition_sources"
        ),
        crate::db::PendingUpdateChange::StructuralProfile(
            crate::db::StructuralProfile::CurrentTasksView
            | crate::db::StructuralProfile::CurrentTaskValidationGatesView
            | crate::db::StructuralProfile::KptLifecycle,
        ) => true,
        _ => false,
    }
}
