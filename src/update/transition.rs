use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, types::ValueRef};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::Path;

mod conservation;
mod edges;
mod plan_state;
mod repair;
use edges::*;
pub(crate) use plan_state::*;
use repair::*;

pub(crate) fn semantic_storage_identity(conn: &Connection) -> Result<String> {
    conservation::semantic_ledger_identity(conn)
}

pub(crate) type ObserveSource =
    fn(&Connection, &TransitionContext<'_>) -> Result<SourceObservation>;
pub(crate) type ApplyTransition =
    fn(&Connection, &SourceObservation, &TransitionContext<'_>) -> Result<()>;
pub(crate) type ValidateTarget =
    fn(&Connection, &SourceObservation, &TransitionContext<'_>) -> Result<()>;

#[derive(Clone, Copy, Debug)]
pub(crate) struct TransitionContext<'a> {
    pub(crate) root: &'a Path,
}

pub(crate) const RESET_SCHEMA_GENERATION: i64 = 14;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StorageHeader {
    Full { generation: i64 },
    Reset { generation: i64 },
}

impl StorageHeader {
    fn generation(self) -> i64 {
        match self {
            Self::Full { generation } | Self::Reset { generation } => generation,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum UpdateRoute {
    Current,
    #[allow(dead_code)]
    CoreNormalization {
        source_revision: String,
    },
    RegisteredPath {
        source_generation: i64,
        source_descriptor: String,
        source_revision: String,
    },
    CurrentRepair {
        source_revision: String,
    },
    RecoveryRequired,
    UnsupportedSource,
}

impl UpdateRoute {
    pub(crate) fn requires_change(&self) -> bool {
        !matches!(self, Self::Current)
    }
}

pub(crate) fn preserved_capability_classes(
    conn: &Connection,
    route: &UpdateRoute,
) -> Result<Vec<String>> {
    if matches!(route, UpdateRoute::RecoveryRequired) {
        return Ok(Vec::new());
    }
    if matches!(route, UpdateRoute::UnsupportedSource) {
        return Ok(Vec::new());
    }
    let generation = classify_storage_header(conn)?.generation();
    let mut classes = vec![
        "project-and-repository".to_string(),
        "work-and-planning".to_string(),
        "review-and-evidence".to_string(),
    ];
    if generation >= 2 {
        classes.extend(
            ["commands-and-records", "owner-decisions-and-governance"]
                .into_iter()
                .map(str::to_string),
        );
    }
    if generation >= 3 {
        classes.push("design-and-traceability".to_string());
    }
    if generation >= 6 {
        classes.push("phases-and-dependencies".to_string());
    }
    Ok(classes)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StateDescriptor {
    pub(crate) key: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceObservation {
    pub(crate) descriptor_key: String,
    pub(crate) revision: String,
    pub(crate) historical_source: Option<HistoricalSource>,
    pub(crate) conservation: Option<conservation::ConservationSnapshot>,
    pub(crate) plans: Vec<crate::decomposition::ParsedPlan>,
    pub(crate) derived_bundle_count: usize,
    pub(crate) decomposition_projection: Option<String>,
    pub(crate) reconciliation_balance: Option<ReconciliationBalance>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HistoricalSource {
    generation: i64,
    ledger_identity: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MappingBalance {
    pub(crate) retained: i64,
    pub(crate) retired: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ReconciliationBalance {
    pub(crate) tasks: MappingBalance,
    pub(crate) checklist_items: MappingBalance,
    pub(crate) gates: MappingBalance,
    pub(crate) phases: MappingBalance,
    pub(crate) dependencies: MappingBalance,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TransitionEdge {
    pub(crate) key: &'static str,
    pub(crate) source: StateDescriptor,
    pub(crate) target: StateDescriptor,
    pub(crate) observe_source: ObserveSource,
    pub(crate) apply: ApplyTransition,
    pub(crate) validate_target: ValidateTarget,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TransitionReceipt {
    pub(crate) edge_key: String,
    pub(crate) source_revision: String,
    pub(crate) target_descriptor: String,
}

const HISTORICAL_GENERATION_4: StateDescriptor = StateDescriptor {
    key: "historical-storage-generation-4",
};
const HISTORICAL_GENERATION_6: StateDescriptor = StateDescriptor {
    key: "historical-storage-generation-6",
};
const HISTORICAL_GENERATION_7: StateDescriptor = StateDescriptor {
    key: "historical-storage-generation-7",
};
const HISTORICAL_GENERATION_8: StateDescriptor = StateDescriptor {
    key: "historical-storage-generation-8",
};
const HISTORICAL_GENERATION_9: StateDescriptor = StateDescriptor {
    key: "historical-storage-generation-9",
};
const HISTORICAL_GENERATION_10: StateDescriptor = StateDescriptor {
    key: "historical-storage-generation-10",
};
const HISTORICAL_GENERATION_11: StateDescriptor = StateDescriptor {
    key: "historical-storage-generation-11",
};
const HISTORICAL_GENERATION_12: StateDescriptor = StateDescriptor {
    key: "historical-storage-generation-12",
};
const GENERATION_13: StateDescriptor = StateDescriptor {
    key: "full-storage-with-owner-history",
};
const GENERATION_13_REPAIR_SOURCE: StateDescriptor = StateDescriptor {
    key: "repairable-core-storage",
};
const GENERATION_14: StateDescriptor = StateDescriptor {
    key: "full-storage-with-update-and-release-state",
};
const GENERATION_15: StateDescriptor = StateDescriptor {
    key: "full-storage-with-explicit-decomposition",
};
const GENERATION_16: StateDescriptor = StateDescriptor {
    key: "full-storage-with-release-lifecycle",
};
const GENERATION_17: StateDescriptor = StateDescriptor {
    key: "full-storage-with-explicit-reconciliation-effects",
};
const GENERATION_18: StateDescriptor = StateDescriptor {
    key: "full-storage-with-immutable-reconciliation-results",
};
const GENERATION_19: StateDescriptor = StateDescriptor {
    key: "full-storage-with-ready-reconciliation-successor",
};
const GENERATION_20: StateDescriptor = StateDescriptor {
    key: "full-storage-with-complete-kpt-lifecycle",
};
const GENERATION_21: StateDescriptor = StateDescriptor {
    key: "full-storage-with-terminal-design-recovery",
};
const GENERATION_22: StateDescriptor = StateDescriptor {
    key: "full-storage-with-opaque-correction-identities",
};
const GENERATION_23: StateDescriptor = StateDescriptor {
    key: "full-storage-with-public-owner-recovery",
};
const GENERATION_24: StateDescriptor = StateDescriptor {
    key: "full-storage-with-release-work-boundaries",
};
const GENERATION_25: StateDescriptor = StateDescriptor {
    key: "full-storage-with-project-local-owner-decisions",
};
const CURRENT_REPAIR_SOURCE: StateDescriptor = StateDescriptor {
    key: "current-storage-with-registered-repair",
};
const REPAIRED_CURRENT: StateDescriptor = StateDescriptor {
    key: "repaired-current-storage",
};

const GENERATION_14_TABLES: &[&str] = &[
    "update_operations",
    "update_decisions",
    "update_receipts",
    "release_candidates",
    "release_candidate_assets",
    "release_candidate_events",
];

const GENERATION_15_TABLES: &[&str] = &[
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
];

const GENERATION_16_TABLES: &[&str] = &[
    "release_candidate_revisions",
    "release_candidate_subject_revisions",
    "release_candidate_attempts",
];

pub(crate) fn classify_storage_header(conn: &Connection) -> Result<StorageHeader> {
    let (has_migrations, has_unknown_metadata): (bool, bool) = conn.query_row(
        r#"
        select
          exists(select 1 from sqlite_schema where type='table' and name='schema_migrations'),
          exists(select 1 from sqlite_schema where type='table' and name='schema_metadata')
        "#,
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    match (has_migrations, has_unknown_metadata) {
        (true, true) => bail!("ledger has contradictory storage headers"),
        (false, false) => bail!("ledger has no registered storage header"),
        (false, true) => {
            let (rows, singleton, generation): (i64, Option<i64>, Option<i64>) = conn
                .query_row(
                    "select count(*),min(singleton),max(schema_version) from schema_metadata",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .context("recovery storage header cannot be decoded")?;
            let generation = generation.context("recovery storage header has no generation")?;
            if rows != 1 || singleton != Some(1) || generation != RESET_SCHEMA_GENERATION {
                bail!("recovery storage header is not a registered source state");
            }
            let (projects, unexpected_families): (i64, i64) = conn.query_row(
                r#"
                select
                  (select count(*) from projects),
                  (select count(*) from sqlite_schema
                   where type='table' and name not like 'sqlite_%'
                     and name not in ('projects','schema_metadata'))
                "#,
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            if projects != 1 || unexpected_families != 0 {
                bail!("recovery storage does not satisfy its registered structural contract");
            }
            Ok(StorageHeader::Reset { generation })
        }
        (true, false) => {
            let (rows, generation): (i64, Option<i64>) = conn
                .query_row(
                    "select count(*),max(version) from schema_migrations",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .context("full storage header cannot be decoded")?;
            let generation = generation
                .filter(|generation| *generation > 0)
                .filter(|_| rows > 0)
                .context("full storage header has no positive generation")?;
            if generation > crate::db::SCHEMA_VERSION {
                bail!(
                    "ledger schema generation {generation} is newer than supported generation {}",
                    crate::db::SCHEMA_VERSION
                );
            }
            Ok(StorageHeader::Full { generation })
        }
    }
}

pub(crate) fn classify_update_route(conn: &Connection, root: &Path) -> Result<UpdateRoute> {
    validate_transition_registry()?;
    let header = classify_storage_header(conn)?;
    if matches!(header, StorageHeader::Reset { .. }) {
        return Ok(UpdateRoute::RecoveryRequired);
    }
    let generation = header.generation();
    if generation == crate::db::SCHEMA_VERSION {
        let pending = crate::db::pending_update_change_set(conn)?;
        if pending.is_empty() {
            validate_current_generation_24(conn)?;
            return Ok(UpdateRoute::Current);
        }
        if pending.iter().any(is_unrepairable_current_change) {
            bail!("current storage does not satisfy its registered structural contract");
        }
        return Ok(UpdateRoute::CurrentRepair {
            source_revision: observe_current_repair_source(conn, &TransitionContext { root })?
                .revision,
        });
    }
    let context = TransitionContext { root };
    for (source_generation, observer) in [
        (23, observe_generation_23 as ObserveSource),
        (22, observe_generation_22 as ObserveSource),
        (21, observe_generation_21 as ObserveSource),
        (20, observe_generation_20 as ObserveSource),
        (19, observe_generation_19 as ObserveSource),
        (18, observe_generation_18 as ObserveSource),
        (17, observe_generation_17 as ObserveSource),
        (16, observe_generation_16 as ObserveSource),
        (15, observe_generation_15 as ObserveSource),
        (14, observe_generation_14 as ObserveSource),
        (13, observe_generation_13 as ObserveSource),
    ] {
        if let Ok(source) = observer(conn, &context) {
            return Ok(UpdateRoute::RegisteredPath {
                source_generation,
                source_descriptor: source.descriptor_key,
                source_revision: source.revision,
            });
        }
    }
    if generation == crate::db::CORE_SCHEMA_VERSION
        && let Ok(source) = observe_generation_13_repair(conn, &context)
    {
        return Ok(UpdateRoute::RegisteredPath {
            source_generation: generation,
            source_descriptor: source.descriptor_key,
            source_revision: source.revision,
        });
    }
    if historical_descriptor(generation).is_some()
        && let Ok(source) = observe_historical_source(conn, &context)
    {
        return Ok(UpdateRoute::RegisteredPath {
            source_generation: generation,
            source_descriptor: source.descriptor_key,
            source_revision: source.revision,
        });
    }
    Ok(UpdateRoute::UnsupportedSource)
}

fn validate_current_generation_17(conn: &Connection) -> Result<()> {
    validate_generation_16_release_contract(conn)?;
    validate_generation_16_reconciliation_shape(conn)?;
    for column in ["document_content", "content_identity", "design_package_id"] {
        if !column_exists(conn, "decomposition_plans", column)? {
            bail!("current storage is missing an owned Decomposition Plan field");
        }
    }
    let invalid_packages: i64 = conn.query_row(
        r#"
        select count(*) from decomposition_plans plan
        join design_versions version on version.id=plan.design_version_id
        where plan.design_package_id!=version.design_package_id
           or plan.project_id!=version.project_id
        "#,
        [],
        |row| row.get(0),
    )?;
    if invalid_packages != 0 {
        bail!("current Decomposition Plan revision has a foreign package lineage");
    }
    for table in RECONCILIATION_MAPPING_TABLES {
        if !column_exists(conn, table, "effect")? {
            bail!("current storage is missing a reconciliation effect field");
        }
    }
    for column in ["boundary_selector", "resolved_boundary_identity"] {
        if !column_exists(conn, "decomposition_reconciliation_gates", column)? {
            bail!("current storage is missing an exact validation-boundary field");
        }
    }
    let invalid_effects: i64 = RECONCILIATION_MAPPING_TABLES
        .iter()
        .map(|table| {
            conn.query_row(
                &format!(
                    "select count(*) from {} where (disposition='retained' and effect not in ('preserve','open')) or (disposition='retired' and effect is not null)",
                    quote_identifier(table)
                ),
                [],
                |row| row.get::<_, i64>(0),
            )
        })
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .sum();
    if invalid_effects != 0 {
        bail!("current reconciliation effects are not a closed total mapping");
    }
    validate_owned_plan_content_identities(conn)?;
    let invalid_boundaries: i64 = conn.query_row(
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
    if invalid_boundaries != 0 {
        bail!("current reconciliation gates do not own exact validation boundaries");
    }
    validate_reconciliation_gate_boundaries(conn)?;
    Ok(())
}

fn validate_current_generation_18(conn: &Connection) -> Result<()> {
    validate_current_generation_17(conn)?;
    if !table_exists(conn, "decomposition_reconciliation_results")? {
        bail!("current storage is missing immutable reconciliation results");
    }
    crate::decomposition::validate_reconciliation_results(conn)
}

fn validate_current_generation_19(conn: &Connection) -> Result<()> {
    validate_current_generation_18(conn)?;
    for index in [
        "decomposition_plan_applied_package_work_unique",
        "decomposition_plan_editable_package_work_unique",
    ] {
        let exists: bool = conn.query_row(
            "select exists(select 1 from sqlite_master where type='index' and name=?1)",
            [index],
            |row| row.get(0),
        )?;
        if !exists {
            bail!("current storage is missing a Decomposition Plan owner constraint");
        }
    }
    Ok(())
}

fn validate_current_generation_20(conn: &Connection) -> Result<()> {
    validate_current_generation_19(conn)?;
    for table in ["kpt_item_sources", "kpt_rules", "kpt_item_dismissals"] {
        if !table_exists(conn, table)? {
            bail!("current storage is missing a KPT lifecycle relation");
        }
    }
    let conversion_supports_all_targets: bool = conn.query_row(
        "select coalesce((select sql like '%''rule''%' and sql like '%''correction''%' and sql like '%kpt_rule_id%' from sqlite_schema where type='table' and name='kpt_item_conversions'),0) and exists(select 1 from pragma_table_info('kpt_item_conversions') where name='request_identity') and exists(select 1 from pragma_table_info('kpt_item_conversions') where name='receipt_identity') and exists(select 1 from pragma_table_info('kpt_item_conversions') where name='current_handle')",
        [],
        |row| row.get(0),
    )?;
    if !conversion_supports_all_targets {
        bail!("current storage does not support the complete KPT conversion sum");
    }
    Ok(())
}

fn validate_current_generation_21(conn: &Connection) -> Result<()> {
    validate_current_generation_20(conn)?;
    if !table_exists(conn, "finding_design_recoveries")? {
        bail!("current storage is missing terminal design recovery receipts");
    }
    let columns = conn
        .prepare(
            "select name,upper(type),\"notnull\",pk from pragma_table_info('finding_design_recoveries') order by cid",
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let expected_columns = [
        ("id", "INTEGER", 0, 1),
        ("project_id", "INTEGER", 1, 0),
        ("recovery_handle", "TEXT", 1, 0),
        ("finding_id", "INTEGER", 1, 0),
        ("terminal_epoch", "INTEGER", 1, 0),
        ("source_closure_id", "INTEGER", 1, 0),
        ("source_session_id", "INTEGER", 1, 0),
        ("source_attempt_id", "INTEGER", 1, 0),
        ("authority_event_id", "INTEGER", 1, 0),
        ("evidence", "TEXT", 1, 0),
        ("reason", "TEXT", 1, 0),
        ("package_current", "TEXT", 1, 0),
        ("expected_current", "TEXT", 1, 0),
        ("idempotency_key", "TEXT", 1, 0),
        ("payload_digest", "TEXT", 1, 0),
        ("postcondition_digest", "TEXT", 1, 0),
        ("successor_design_version_id", "INTEGER", 1, 0),
        ("successor_alias", "TEXT", 1, 0),
        ("successor_closure_id", "INTEGER", 1, 0),
        ("successor_session_id", "INTEGER", 1, 0),
        ("successor_attempt_id", "INTEGER", 1, 0),
        ("successor_epoch_decision_id", "INTEGER", 1, 0),
        ("created_at", "TEXT", 1, 0),
    ];
    let expected_columns = expected_columns
        .into_iter()
        .map(|(name, kind, not_null, primary_key)| {
            (name.to_string(), kind.to_string(), not_null, primary_key)
        })
        .collect::<Vec<_>>();
    if columns != expected_columns {
        bail!("terminal design recovery receipt columns do not match the registered structure");
    }

    let foreign_keys = conn
        .prepare(
            "select \"from\",\"table\",\"to\" from pragma_foreign_key_list('finding_design_recoveries') order by \"from\",\"table\",\"to\"",
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut expected_foreign_keys = [
        ("project_id", "projects", "id"),
        ("finding_id", "findings", "id"),
        ("source_closure_id", "closures", "id"),
        ("source_session_id", "correction_sessions", "id"),
        ("source_attempt_id", "closure_attempts", "id"),
        ("authority_event_id", "authority_events", "id"),
        ("successor_design_version_id", "design_versions", "id"),
        ("successor_closure_id", "closures", "id"),
        ("successor_session_id", "correction_sessions", "id"),
        ("successor_attempt_id", "closure_attempts", "id"),
        ("successor_epoch_decision_id", "owner_decisions", "id"),
    ]
    .into_iter()
    .map(|(from, table, to)| (from.to_string(), table.to_string(), to.to_string()))
    .collect::<Vec<_>>();
    expected_foreign_keys.sort();
    if foreign_keys != expected_foreign_keys {
        bail!(
            "terminal design recovery receipt foreign keys do not match the registered structure"
        );
    }

    for unique_columns in [
        &["project_id", "recovery_handle"][..],
        &["project_id", "idempotency_key"],
        &["project_id", "finding_id", "terminal_epoch"],
        &["project_id", "successor_alias"],
        &["project_id", "successor_design_version_id"],
        &["project_id", "successor_closure_id"],
        &["project_id", "successor_session_id"],
        &["project_id", "successor_attempt_id"],
        &["successor_epoch_decision_id"],
    ] {
        if !has_unique_index(conn, "finding_design_recoveries", unique_columns)? {
            bail!(
                "terminal design recovery receipt is missing unique identity ({})",
                unique_columns.join(",")
            );
        }
    }

    let table_sql = schema_object_sql(conn, "table", "finding_design_recoveries")?;
    require_sql_tokens(
        "terminal design recovery receipt",
        &table_sql,
        &[
            "check(length(payload_digest)=64)",
            "check(length(postcondition_digest)=64)",
        ],
    )?;
    validate_recovery_trigger_behavior(conn)?;
    validate_verification_trigger_behavior(conn)?;
    Ok(())
}

fn validate_current_generation_22(conn: &Connection) -> Result<()> {
    validate_current_generation_21(conn)?;
    validate_opaque_correction_trigger_behavior(conn)?;
    validate_correction_decomposition_membership_view(conn)?;
    validate_pending_reconciliation_targets(conn)
}

fn validate_current_generation_23(conn: &Connection) -> Result<()> {
    validate_current_generation_22(conn)?;
    for table in [
        "decision_continuations",
        "reviewer_migration_sources",
        "reviewer_migration_bindings",
        "validation_link_repair_runs",
        "validation_link_repair_changes",
        "validation_link_retirements",
        "validation_link_repair_receipts",
    ] {
        if !table_exists(conn, table)? {
            bail!("current storage is missing a public owner recovery relation: {table}");
        }
    }
    for column in [
        "context_identity",
        "required_inputs",
        "owner_decision_id",
        "successor_id",
    ] {
        if !column_exists(conn, "decision_continuations", column)? {
            bail!("current storage is missing a decision continuation field: {column}");
        }
    }
    let invalid_continuations: i64 = conn.query_row(
        r#"
        select count(*) from decision_continuations continuation
        where (continuation.status='pending' and (
                 continuation.owner_decision_id is not null or continuation.successor_id is not null
                 or continuation.applied_at is not null or continuation.superseded_at is not null))
           or (continuation.status='applied' and (
                 continuation.owner_decision_id is null or continuation.successor_id is not null
                 or continuation.applied_at is null or continuation.superseded_at is not null))
           or (continuation.status='superseded' and (
                 continuation.owner_decision_id is not null or continuation.successor_id is null
                 or continuation.applied_at is not null or continuation.superseded_at is null))
           or (continuation.owner_decision_id is not null and not exists(
                 select 1 from owner_decisions decision
                 where decision.id=continuation.owner_decision_id
                   and decision.project_id=continuation.project_id))
           or (continuation.successor_id is not null and not exists(
                 select 1 from decision_continuations successor
                 where successor.id=continuation.successor_id
                   and successor.project_id=continuation.project_id))
        "#,
        [],
        |row| row.get(0),
    )?;
    if invalid_continuations != 0 {
        bail!("current storage contains an invalid decision continuation transition");
    }
    let invalid_reviewer_sources: i64 = conn.query_row(
        r#"
        select count(*) from reviewer_migration_sources source
        where (source.status='pending' and source.binding_id is not null)
           or (source.status='bound' and not exists(
                 select 1 from reviewer_migration_bindings binding
                 where binding.id=source.binding_id and binding.source_id=source.id
                   and binding.project_id=source.project_id))
           or (source.status='retired' and source.retired_at is null)
        "#,
        [],
        |row| row.get(0),
    )?;
    if invalid_reviewer_sources != 0 {
        bail!("current storage contains an invalid reviewer migration source");
    }
    Ok(())
}

fn validate_current_generation_24(conn: &Connection) -> Result<()> {
    validate_current_generation_23(conn)?;
    if !table_exists(conn, "release_candidate_boundaries")? {
        bail!("current storage is missing release candidate work boundaries");
    }
    for column in [
        "project_id",
        "release_candidate_id",
        "work_unit_id",
        "activation_id",
        "design_version_id",
        "repository_snapshot_id",
        "reviewed_commit",
        "boundary_identity",
    ] {
        if !column_exists(conn, "release_candidate_boundaries", column)? {
            bail!("current storage is missing a release boundary field: {column}");
        }
    }
    if !has_unique_index(
        conn,
        "release_candidate_boundaries",
        &["release_candidate_id"],
    )? {
        bail!("release candidate work boundary is not unique per candidate");
    }
    let invalid: i64 = conn.query_row(
        r#"
        select count(*)
        from release_candidate_boundaries boundary
        left join release_candidates candidate on candidate.id=boundary.release_candidate_id
        left join work_units work on work.id=boundary.work_unit_id
        left join work_unit_activations activation on activation.id=boundary.activation_id
        left join design_versions version on version.id=boundary.design_version_id
        left join design_packages package on package.id=version.design_package_id
        left join repository_snapshots snapshot on snapshot.id=boundary.repository_snapshot_id
        left join repositories repository on repository.id=snapshot.repository_id
        where candidate.id is null or candidate.project_id!=boundary.project_id
           or candidate.reviewed_commit!=boundary.reviewed_commit
           or work.id is null or work.project_id!=boundary.project_id
           or (boundary.activation_id is not null and (
                 activation.id is null or activation.project_id!=boundary.project_id
                 or activation.work_unit_id!=boundary.work_unit_id))
           or (boundary.design_version_id is not null and (
                 version.id is null or package.project_id!=boundary.project_id))
           or snapshot.id is null or repository.project_id!=boundary.project_id
           or snapshot.head_sha!=boundary.reviewed_commit
           or (boundary.activation_id is not null
               and snapshot.work_unit_activation_id!=boundary.activation_id)
           or length(boundary.boundary_identity)!=64
        "#,
        [],
        |row| row.get(0),
    )?;
    if invalid != 0 {
        bail!("current storage contains {invalid} invalid release candidate work boundaries");
    }
    for trigger in [
        "trg_release_candidate_boundary_insert",
        "trg_release_candidate_boundary_update",
        "trg_release_candidate_boundary_delete",
    ] {
        let present: bool = conn.query_row(
            "select exists(select 1 from sqlite_schema where type='trigger' and name=?1)",
            [trigger],
            |row| row.get(0),
        )?;
        if !present {
            bail!("current storage is missing a release boundary guard: {trigger}");
        }
    }
    Ok(())
}

fn validate_current_generation_25(conn: &Connection) -> Result<()> {
    validate_current_generation_24(conn)?;
    if column_exists(conn, "owner_decisions", "capability_id")?
        || column_exists(conn, "owner_decisions", "principal_id")?
        || column_exists(conn, "review_agent_invocations", "reviewer_principal_id")?
        || column_exists(conn, "review_agent_invocations", "review_provenance_id")?
        || column_exists(
            conn,
            "review_agent_invocations",
            "legacy_source_reviewer_digest",
        )?
    {
        bail!("current storage retains retired owner authority fields");
    }
    Ok(())
}

fn validate_correction_decomposition_membership_view(conn: &Connection) -> Result<()> {
    let ingress = schema_object_sql(conn, "table", "decomposition_plan_ingress_identities")?;
    require_sql_tokens(
        "Decomposition Plan ingress identity",
        &ingress,
        &[
            "plan_id integer primary key",
            "source_identity text not null",
            "content_identity text not null",
            "check(length(source_identity)=64)",
            "check(length(content_identity)=64)",
        ],
    )?;
    for trigger in [
        "trg_decomposition_plan_ingress_links_insert",
        "trg_decomposition_plan_ingress_immutable_update",
        "trg_decomposition_plan_ingress_immutable_delete",
    ] {
        schema_object_sql(conn, "trigger", trigger)?;
    }
    let invalid_ingress: bool = conn.query_row(
        r#"
        select exists(
          select 1
          from decomposition_plan_ingress_identities ingress
          left join decomposition_plans plan on plan.id=ingress.plan_id
          where plan.id is null or plan.project_id!=ingress.project_id
            or plan.content_identity!=ingress.content_identity
        )
        "#,
        [],
        |row| row.get(0),
    )?;
    if invalid_ingress {
        bail!("Decomposition Plan ingress identity is not bound to its exact Plan");
    }
    let view = schema_object_sql(conn, "view", "correction_decomposition_task_memberships")?;
    require_sql_tokens(
        "correction decomposition task membership view",
        &view,
        &[
            "correction_transition_applications",
            "decomposition_plans",
            "decomposition_applications",
            "decomposition-plan:",
            "application.task_id",
        ],
    )?;
    let probe = Connection::open_in_memory()?;
    probe.execute_batch(
        r#"
        create table correction_transition_applications(id integer,result_ref text);
        create table decomposition_plans(id integer);
        create table decomposition_applications(decomposition_plan_id integer,task_id integer);
        insert into correction_transition_applications values(41,'decomposition-plan:71');
        insert into correction_transition_applications values(42,'unrelated:71');
        insert into decomposition_plans values(71),(72);
        insert into decomposition_applications values(71,101),(71,102),(72,103);
        "#,
    )?;
    probe.execute_batch(&view)?;
    let memberships = probe
        .prepare(
            "select correction_application_id,task_id from correction_decomposition_task_memberships order by correction_application_id,task_id",
        )?
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if memberships != [(41, 101), (41, 102)] {
        bail!("correction decomposition task membership view is not backed by Plan applications");
    }
    Ok(())
}

fn validate_pending_reconciliation_targets(conn: &Connection) -> Result<()> {
    let pending = crate::db::inspect_pending_reconciliation_target_migrations(conn)?;
    if !pending.changes.is_empty() || !pending.blockers.is_empty() {
        bail!("current storage retains a pending legacy Decomposition Plan reconciliation target");
    }
    let targets = conn
        .prepare(
            "select token.target from correction_tokens token join closures closure on closure.id=token.closure_id where token.token_kind='transition' and token.operation='decomposition-plan-reconcile' and token.status='pending' and closure.status='registered' order by token.id",
        )?
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for target in targets {
        let parts = target.split('/').collect::<Vec<_>>();
        let valid_owner = parts.len() == 3
            && parts[0]
                .parse::<i64>()
                .is_ok_and(|value| value > 0 && value.to_string() == parts[0])
            && parts[1]
                .parse::<i64>()
                .is_ok_and(|value| value > 0 && value.to_string() == parts[1]);
        let valid_path = valid_owner
            && crate::review::decode_opaque_component(parts[2], "Decomposition Plan path")
                .ok()
                .is_some_and(|path| {
                    let path = Path::new(&path);
                    !path.is_absolute()
                        && path.extension().and_then(|value| value.to_str()) == Some("md")
                        && path
                            .components()
                            .all(|component| matches!(component, std::path::Component::Normal(_)))
                });
        if !valid_path {
            bail!(
                "current storage contains an invalid pending Decomposition Plan reconciliation target"
            );
        }
    }
    Ok(())
}

fn validate_opaque_correction_trigger_behavior(conn: &Connection) -> Result<()> {
    let correction = schema_object_sql(conn, "trigger", "trg_correction_token_links_insert")?;
    let reconciliation = schema_object_sql(
        conn,
        "trigger",
        "trg_decomposition_reconciliation_application_links_insert",
    )?;
    let probe = Connection::open_in_memory()?;
    probe.execute_batch(
        r#"
        create table closures(id integer,project_id integer,finding_id integer,status text);
        create table findings(id integer,review_run_id integer,status text,classification text);
        create table review_runs(id integer,review_plan_id integer);
        create table review_plans(id integer,required integer,stage text,review_type text);
        create table checklists(id integer);
        create table work_phases(id integer);
        create table work_phase_dependencies(id integer);
        create table correction_tokens(
          id integer primary key,project_id integer,closure_id integer,token_ordinal integer,
          token_kind text,operation text,target text,pre_state text,pre_hash text,
          status text,created_at text,applied_at text
        );
        insert into review_plans values(1,0,'implementation-ready','design_task_decomposition');
        insert into review_runs values(1,1);
        insert into findings values(1,1,'open','valid');
        insert into closures values(1,1,1,'registered');
        "#,
    )?;
    probe.execute_batch(&correction)?;
    probe
        .execute(
            "insert into correction_tokens values(1,1,1,1,'transition','decomposition-plan-reconcile','1/1/b64:cGxhbnMveC5tZA',null,null,'pending',current_timestamp,null)",
            [],
        )
        .context("opaque correction trigger rejected a valid Plan path")?;
    probe
        .execute(
            "insert into correction_tokens values(2,1,1,2,'transition','task-accept-out-of-scope','@task/b64:44GT44KT44Gr44Gh44Gv',null,null,'pending',current_timestamp,null)",
            [],
        )
        .context("opaque correction trigger rejected a valid Unicode task identity")?;
    probe
        .execute(
            "insert into correction_tokens values(3,1,1,3,'transition','phase-assign','@phase/@task/b64:44GT44KT44Gr44Gh44Gv',null,null,'pending',current_timestamp,null)",
            [],
        )
        .context("opaque correction trigger rejected a valid phase task identity")?;
    require_probe_failure(
        probe.execute(
            "insert into correction_tokens values(4,1,1,4,'transition','decomposition-plan-reconcile','1/1',null,null,'pending',current_timestamp,null)",
            [],
        ),
        "opaque correction trigger accepted the obsolete design/work Plan target",
    )?;
    require_probe_failure(
        probe.execute(
            "insert into correction_tokens values(5,1,1,5,'transition','task-accept-out-of-scope','@task/b64:YQ==',null,null,'pending',current_timestamp,null)",
            [],
        ),
        "opaque correction trigger accepted padded base64url",
    )?;

    probe.execute_batch(
        r#"
        create table correction_transition_applications(
          id integer,project_id integer,correction_token_id integer,result_ref text
        );
        create table decomposition_plans(
          id integer,project_id integer,status text,predecessor_id integer,
          design_version_id integer,work_unit_id integer,source_identity text
        );
        create table decomposition_reconciliation_applications(
          project_id integer,correction_application_id integer,correction_token_id integer,
          predecessor_plan_id integer,successor_plan_id integer,source_identity text
        );
        insert into decomposition_plans values(1,1,'superseded',null,1,1,'old');
        insert into decomposition_plans values(2,1,'applied',1,1,1,'post-edit-digest');
        insert into correction_transition_applications values(1,1,1,'decomposition-plan:2');
        "#,
    )?;
    probe.execute_batch(&reconciliation)?;
    probe
        .execute(
            "insert into decomposition_reconciliation_applications values(1,1,1,1,2,'post-edit-digest')",
            [],
        )
        .context("reconciliation trigger rejected the authorized path and post-edit digest")?;
    probe.execute(
        "insert into correction_transition_applications values(2,1,1,'decomposition-plan:2')",
        [],
    )?;
    require_probe_failure(
        probe.execute(
            "insert into decomposition_reconciliation_applications values(1,2,1,1,2,'substituted-digest')",
            [],
        ),
        "reconciliation trigger accepted a substituted post-edit digest",
    )?;
    probe.execute(
        "insert into correction_tokens values(6,1,1,6,'transition','decomposition-plan-reconcile','2/1/b64:cGxhbnMveC5tZA',null,null,'pending',current_timestamp,null)",
        [],
    )?;
    probe.execute(
        "insert into correction_transition_applications values(3,1,6,'decomposition-plan:2')",
        [],
    )?;
    require_probe_failure(
        probe.execute(
            "insert into decomposition_reconciliation_applications values(1,3,6,1,2,'post-edit-digest')",
            [],
        ),
        "reconciliation trigger accepted a substituted design/work owner",
    )
}

fn schema_object_sql(conn: &Connection, object_type: &str, name: &str) -> Result<String> {
    conn.query_row(
        "select sql from sqlite_schema where type=?1 and name=?2",
        [object_type, name],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(|| anyhow::anyhow!("registered {object_type} {name} is missing"))
}

fn require_sql_tokens(label: &str, sql: &str, tokens: &[&str]) -> Result<()> {
    let normalized = sql.to_ascii_lowercase();
    if let Some(token) = tokens
        .iter()
        .find(|token| !normalized.contains(&token.to_ascii_lowercase()))
    {
        bail!("registered {label} does not enforce required structure: {token}");
    }
    Ok(())
}

fn validate_recovery_trigger_behavior(conn: &Connection) -> Result<()> {
    let update = schema_object_sql(
        conn,
        "trigger",
        "trg_finding_design_recovery_immutable_update",
    )?;
    let delete = schema_object_sql(
        conn,
        "trigger",
        "trg_finding_design_recovery_immutable_delete",
    )?;
    let insert = schema_object_sql(
        conn,
        "trigger",
        "trg_finding_design_recovery_project_insert",
    )?;
    let probe = Connection::open_in_memory()?;
    probe.execute_batch(
        r#"
        create table findings(id integer,project_id integer);
        create table closures(id integer,project_id integer,finding_id integer);
        create table correction_sessions(id integer,project_id integer,finding_id integer,closure_id integer);
        create table closure_attempts(id integer,project_id integer,closure_id integer);
        create table authority_events(id integer,project_id integer);
        create table design_versions(id integer,project_id integer);
        create table owner_decisions(id integer,project_id integer,decision_family text,action text,decision_value text);
        create table finding_decision_epochs(project_id integer,finding_id integer,epoch_number integer,reopen_decision_id integer);
        create table finding_design_recoveries(
          project_id integer,finding_id integer,terminal_epoch integer,
          source_closure_id integer,source_session_id integer,source_attempt_id integer,
          authority_event_id integer,successor_design_version_id integer,
          successor_closure_id integer,successor_session_id integer,
          successor_attempt_id integer,successor_epoch_decision_id integer
        );
        insert into findings values(1,1);
        insert into closures values(1,1,1),(2,1,1);
        insert into correction_sessions values(1,1,1,1),(2,1,1,2);
        insert into closure_attempts values(1,1,1),(2,1,2);
        insert into authority_events values(1,1),(2,2);
        insert into design_versions values(1,1);
        insert into owner_decisions values(1,1,'finding','reopen','reopened');
        insert into finding_decision_epochs values(1,1,2,1);
        "#,
    )?;
    probe.execute_batch(&update)?;
    probe.execute_batch(&delete)?;
    probe.execute_batch(&insert)?;
    probe
        .execute(
            "insert into finding_design_recoveries values(1,1,1,1,1,1,1,1,2,2,2,1)",
            [],
        )
        .context("terminal recovery trigger rejected a valid receipt")?;
    require_probe_failure(
        probe.execute("update finding_design_recoveries set terminal_epoch=2", []),
        "terminal recovery update trigger accepted mutation",
    )?;
    require_probe_failure(
        probe.execute("delete from finding_design_recoveries", []),
        "terminal recovery delete trigger accepted mutation",
    )?;
    require_probe_failure(
        probe.execute(
            "insert into finding_design_recoveries values(1,1,1,1,1,1,2,1,2,2,2,1)",
            [],
        ),
        "terminal recovery project trigger accepted a cross-project receipt",
    )
}

fn validate_verification_trigger_behavior(conn: &Connection) -> Result<()> {
    let insert = schema_object_sql(conn, "trigger", "trg_finding_verification_project_insert")?;
    let update = schema_object_sql(conn, "trigger", "trg_finding_verification_project_update")?;
    let probe = Connection::open_in_memory()?;
    probe.execute_batch(
        r#"
        create table review_plans(id integer,work_unit_id integer,review_type text,stage text,design_version_id integer,scope text);
        create table review_runs(id integer,project_id integer,review_plan_id integer,run_type text,run_purpose text);
        create table findings(id integer,project_id integer,review_run_id integer);
        create table closures(id integer,project_id integer,finding_id integer);
        create table design_versions(id integer,design_package_id integer,version_number integer,status text);
        create table finding_design_recoveries(project_id integer,successor_closure_id integer,successor_attempt_id integer,successor_design_version_id integer);
        create table finding_verifications(project_id integer,review_run_id integer,finding_id integer,closure_id integer,closure_attempt_id integer,result text);
        insert into review_plans values(1,1,'design_review','design-ready',1,null),(2,1,'design_review','design-ready',2,null);
        insert into review_runs values(1,1,1,'fresh','new_unbiased_review'),(2,1,2,'resume','finding_fix_verification');
        insert into findings values(1,1,1);
        insert into closures values(1,1,1);
        insert into design_versions values(1,1,1,'approved'),(2,1,2,'draft');
        insert into finding_design_recoveries values(1,1,1,2);
        "#,
    )?;
    probe.execute_batch(&insert)?;
    probe.execute_batch(&update)?;
    probe
        .execute(
            "insert into finding_verifications values(1,2,1,1,1,'verified')",
            [],
        )
        .context("finding verification trigger rejected a valid successor verification")?;
    require_probe_failure(
        probe.execute(
            "insert into finding_verifications values(1,2,1,1,2,'verified')",
            [],
        ),
        "finding verification insert trigger accepted a stale successor attempt",
    )?;
    require_probe_failure(
        probe.execute(
            "update finding_verifications set closure_attempt_id=2 where closure_attempt_id=1",
            [],
        ),
        "finding verification update trigger accepted a stale successor attempt",
    )
}

fn require_probe_failure<T>(result: rusqlite::Result<T>, message: &str) -> Result<()> {
    if result.is_ok() {
        bail!(message.to_string());
    }
    Ok(())
}

fn has_unique_index(conn: &Connection, table: &str, expected: &[&str]) -> Result<bool> {
    let indexes = conn
        .prepare("select name from pragma_index_list(?1) where \"unique\"=1")?
        .query_map([table], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for index in indexes {
        let columns = conn
            .prepare("select name from pragma_index_info(?1) order by seqno")?
            .query_map([index], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if columns
            .iter()
            .map(String::as_str)
            .eq(expected.iter().copied())
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn validate_transition_registry() -> Result<()> {
    validate_edge(&current_repair_edge())?;
    let edges = registered_storage_edges();
    for edge in &edges {
        validate_edge(edge)?;
    }
    for source_generation in supported_storage_generations() {
        let path = registered_storage_path(source_generation)?;
        if source_generation != crate::db::SCHEMA_VERSION
            && path.last().map(|edge| edge.target.key) != Some(GENERATION_25.key)
        {
            bail!("storage transition registry does not form a complete adjacent path");
        }
    }
    let repair_path = registered_storage_path_from_descriptor(GENERATION_13_REPAIR_SOURCE.key)?;
    if repair_path.last().map(|edge| edge.target.key) != Some(GENERATION_25.key) {
        bail!("registered core repair does not compose to the current target");
    }
    Ok(())
}

pub(crate) fn registered_storage_edges() -> Vec<TransitionEdge> {
    let mut edges = historical_descriptors()
        .into_iter()
        .map(|(_, source)| TransitionEdge {
            key: "install-deployed-core-storage",
            source,
            target: GENERATION_13,
            observe_source: observe_historical_source,
            apply: apply_historical_source,
            validate_target: validate_historical_target,
        })
        .collect::<Vec<_>>();
    edges.push(generation_13_repair_edge());
    edges.extend([
        generation_13_to_14_edge(),
        generation_14_to_15_edge(),
        generation_15_to_16_edge(),
        generation_16_to_17_edge(),
        generation_17_to_18_edge(),
        generation_18_to_19_edge(),
        generation_19_to_20_edge(),
        generation_20_to_21_edge(),
        generation_21_to_22_edge(),
        generation_22_to_23_edge(),
        generation_23_to_24_edge(),
        generation_24_to_25_edge(),
    ]);
    edges
}

pub(crate) fn registered_storage_path(source_generation: i64) -> Result<Vec<TransitionEdge>> {
    let source_key = match source_generation {
        13 => GENERATION_13.key,
        14 => GENERATION_14.key,
        15 => GENERATION_15.key,
        16 => GENERATION_16.key,
        17 => GENERATION_17.key,
        18 => GENERATION_18.key,
        19 => GENERATION_19.key,
        20 => GENERATION_20.key,
        21 => GENERATION_21.key,
        22 => GENERATION_22.key,
        23 => GENERATION_23.key,
        24 => GENERATION_24.key,
        25 => GENERATION_25.key,
        generation => historical_descriptor(generation)
            .map(|descriptor| descriptor.key)
            .context("storage header has no registered source descriptor")?,
    };
    registered_storage_path_from_descriptor(source_key)
}

fn registered_storage_path_from_descriptor(source_key: &str) -> Result<Vec<TransitionEdge>> {
    let edges = registered_storage_edges();
    let mut states = edges
        .iter()
        .flat_map(|edge| [edge.source, edge.target])
        .collect::<Vec<_>>();
    states.sort_by_key(|state| state.key);
    states.dedup_by_key(|state| state.key);
    Ok(
        resolve_path(&states, &edges, source_key, GENERATION_25.key)?
            .into_iter()
            .copied()
            .collect(),
    )
}

pub(crate) fn apply_update_route(
    conn: &Connection,
    route: &UpdateRoute,
    root: &Path,
) -> Result<()> {
    match route {
        UpdateRoute::Current => {
            if classify_update_route(conn, root)? != UpdateRoute::Current {
                bail!("classified update source changed before application");
            }
        }
        UpdateRoute::RegisteredPath {
            source_descriptor,
            source_revision,
            ..
        } => {
            let foreign_keys_enabled: i64 =
                conn.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
            conn.pragma_update(None, "foreign_keys", false)?;
            let context = TransitionContext { root };
            let transition = (|| -> Result<()> {
                for (index, edge) in registered_storage_path_from_descriptor(source_descriptor)?
                    .into_iter()
                    .enumerate()
                {
                    let expected = if index == 0 {
                        source_revision.to_string()
                    } else {
                        (edge.observe_source)(conn, &context)?.revision
                    };
                    execute_adjacent(conn, &edge, &expected, &context)?;
                }
                Ok(())
            })();
            let restore = conn.pragma_update(None, "foreign_keys", foreign_keys_enabled != 0);
            transition?;
            restore?;
        }
        UpdateRoute::CoreNormalization { .. } => {
            bail!("synthetic storage normalization is not a registered update route");
        }
        UpdateRoute::CurrentRepair { source_revision } => {
            let foreign_keys_enabled: i64 =
                conn.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
            conn.pragma_update(None, "foreign_keys", false)?;
            let transition = execute_adjacent(
                conn,
                &current_repair_edge(),
                source_revision,
                &TransitionContext { root },
            );
            let restore = conn.pragma_update(None, "foreign_keys", foreign_keys_enabled != 0);
            transition?;
            restore?;
        }
        UpdateRoute::RecoveryRequired => {
            bail!("a verified recovery source is required before update can be applied");
        }
        UpdateRoute::UnsupportedSource => {
            bail!("the project state has no registered no-mutation update path");
        }
    }
    Ok(())
}

pub(crate) fn resolve_path<'a>(
    states: &[StateDescriptor],
    edges: &'a [TransitionEdge],
    source_key: &str,
    target_key: &str,
) -> Result<Vec<&'a TransitionEdge>> {
    let source = unique_state(states, source_key)?;
    let target = unique_state(states, target_key)?;
    if source == target {
        return Ok(Vec::new());
    }

    let mut current = source;
    let mut path = Vec::new();
    while current != target {
        let candidates = edges
            .iter()
            .filter(|edge| edge.source == current)
            .collect::<Vec<_>>();
        let edge = match candidates.as_slice() {
            [edge] => *edge,
            [] => bail!(
                "no declared adjacent transition leaves source descriptor {}",
                current.key
            ),
            _ => bail!(
                "more than one declared transition leaves source descriptor {}",
                current.key
            ),
        };
        validate_edge(edge)?;
        current = edge.target;
        path.push(edge);
        if path.len() > states.len() {
            bail!("state transition registry contains a cycle");
        }
    }
    Ok(path)
}

pub(crate) fn execute_adjacent(
    conn: &Connection,
    edge: &TransitionEdge,
    expected_source_revision: &str,
    context: &TransitionContext<'_>,
) -> Result<TransitionReceipt> {
    validate_edge(edge)?;
    if expected_source_revision.trim().is_empty() {
        bail!("expected source revision is required");
    }
    let transaction = conn.unchecked_transaction()?;
    let observation = (edge.observe_source)(&transaction, context)
        .with_context(|| format!("failed to classify source for transition {}", edge.key))?;
    if observation.descriptor_key != edge.source.key {
        bail!(
            "transition {} expected source descriptor {}, found {}",
            edge.key,
            edge.source.key,
            observation.descriptor_key
        );
    }
    if observation.revision != expected_source_revision {
        bail!(
            "source state changed: expected {}, found {}",
            expected_source_revision,
            observation.revision
        );
    }
    (edge.apply)(&transaction, &observation, context)
        .with_context(|| format!("transition {} did not apply", edge.key))?;
    (edge.validate_target)(&transaction, &observation, context)
        .with_context(|| format!("transition {} target validation failed", edge.key))?;
    transaction.commit()?;
    Ok(TransitionReceipt {
        edge_key: edge.key.to_string(),
        source_revision: observation.revision,
        target_descriptor: edge.target.key.to_string(),
    })
}

fn unique_state(states: &[StateDescriptor], key: &str) -> Result<StateDescriptor> {
    let matches = states
        .iter()
        .filter(|state| state.key == key)
        .copied()
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [state] => Ok(*state),
        [] => bail!("state descriptor is not registered: {key}"),
        _ => bail!("state descriptor is registered more than once: {key}"),
    }
}

fn validate_edge(edge: &TransitionEdge) -> Result<()> {
    if edge.key.trim().is_empty() {
        bail!("transition key is required");
    }
    if edge.source.key == edge.target.key {
        bail!("transition {} must cross a descriptor boundary", edge.key);
    }
    Ok(())
}

fn current_repair_edge() -> TransitionEdge {
    TransitionEdge {
        key: "apply-registered-current-repair",
        source: CURRENT_REPAIR_SOURCE,
        target: REPAIRED_CURRENT,
        observe_source: observe_current_repair_source,
        apply: apply_current_profile_transition,
        validate_target: validate_repaired_current,
    }
}

fn historical_descriptors() -> [(i64, StateDescriptor); 8] {
    [
        (4, HISTORICAL_GENERATION_4),
        (6, HISTORICAL_GENERATION_6),
        (7, HISTORICAL_GENERATION_7),
        (8, HISTORICAL_GENERATION_8),
        (9, HISTORICAL_GENERATION_9),
        (10, HISTORICAL_GENERATION_10),
        (11, HISTORICAL_GENERATION_11),
        (12, HISTORICAL_GENERATION_12),
    ]
}

fn generation_13_repair_edge() -> TransitionEdge {
    TransitionEdge {
        key: "repair-deployed-core-storage",
        source: GENERATION_13_REPAIR_SOURCE,
        target: GENERATION_13,
        observe_source: observe_generation_13_repair,
        apply: apply_generation_13_repair,
        validate_target: validate_generation_13_repair,
    }
}

fn supported_storage_generations() -> impl Iterator<Item = i64> {
    historical_descriptors()
        .into_iter()
        .map(|(generation, _)| generation)
        .chain(crate::db::CORE_SCHEMA_VERSION..=crate::db::SCHEMA_VERSION)
}

#[cfg(test)]
pub(crate) fn registered_historical_generations() -> Vec<i64> {
    historical_descriptors()
        .into_iter()
        .map(|(generation, _)| generation)
        .collect()
}

fn historical_descriptor(generation: i64) -> Option<StateDescriptor> {
    historical_descriptors()
        .into_iter()
        .find_map(|(candidate, descriptor)| (candidate == generation).then_some(descriptor))
}

pub(crate) fn generation_13_to_14_edge() -> TransitionEdge {
    TransitionEdge {
        key: "install-update-and-release-state",
        source: GENERATION_13,
        target: GENERATION_14,
        observe_source: observe_generation_13,
        apply: apply_generation_14,
        validate_target: validate_generation_14,
    }
}

pub(crate) fn generation_14_to_15_edge() -> TransitionEdge {
    TransitionEdge {
        key: "install-explicit-decomposition-state",
        source: GENERATION_14,
        target: GENERATION_15,
        observe_source: observe_generation_14,
        apply: apply_generation_15,
        validate_target: validate_generation_15,
    }
}

pub(crate) fn generation_15_to_16_edge() -> TransitionEdge {
    TransitionEdge {
        key: "install-release-candidate-lifecycle",
        source: GENERATION_15,
        target: GENERATION_16,
        observe_source: observe_generation_15,
        apply: apply_generation_16,
        validate_target: validate_generation_16,
    }
}

pub(crate) fn generation_16_to_17_edge() -> TransitionEdge {
    TransitionEdge {
        key: "install-explicit-reconciliation-effects",
        source: GENERATION_16,
        target: GENERATION_17,
        observe_source: observe_generation_16,
        apply: apply_generation_17,
        validate_target: validate_generation_17,
    }
}

pub(crate) fn generation_17_to_18_edge() -> TransitionEdge {
    TransitionEdge {
        key: "install-immutable-reconciliation-results",
        source: GENERATION_17,
        target: GENERATION_18,
        observe_source: observe_generation_17,
        apply: apply_generation_18,
        validate_target: validate_generation_18,
    }
}

pub(crate) fn generation_18_to_19_edge() -> TransitionEdge {
    TransitionEdge {
        key: "install-ready-reconciliation-successor",
        source: GENERATION_18,
        target: GENERATION_19,
        observe_source: observe_generation_18,
        apply: apply_generation_19,
        validate_target: validate_generation_19,
    }
}

pub(crate) fn generation_19_to_20_edge() -> TransitionEdge {
    TransitionEdge {
        key: "install-complete-kpt-lifecycle",
        source: GENERATION_19,
        target: GENERATION_20,
        observe_source: observe_generation_19,
        apply: apply_generation_20,
        validate_target: validate_generation_20,
    }
}

pub(crate) fn generation_20_to_21_edge() -> TransitionEdge {
    TransitionEdge {
        key: "install-terminal-design-recovery",
        source: GENERATION_20,
        target: GENERATION_21,
        observe_source: observe_generation_20,
        apply: apply_generation_21,
        validate_target: validate_generation_21,
    }
}

pub(crate) fn generation_21_to_22_edge() -> TransitionEdge {
    TransitionEdge {
        key: "install-opaque-correction-identities",
        source: GENERATION_21,
        target: GENERATION_22,
        observe_source: observe_generation_21,
        apply: apply_generation_22,
        validate_target: validate_generation_22,
    }
}

pub(crate) fn generation_22_to_23_edge() -> TransitionEdge {
    TransitionEdge {
        key: "install-public-owner-recovery",
        source: GENERATION_22,
        target: GENERATION_23,
        observe_source: observe_generation_22,
        apply: apply_generation_23,
        validate_target: validate_generation_23,
    }
}

pub(crate) fn generation_23_to_24_edge() -> TransitionEdge {
    TransitionEdge {
        key: "install-release-work-boundaries",
        source: GENERATION_23,
        target: GENERATION_24,
        observe_source: observe_generation_23,
        apply: apply_generation_24,
        validate_target: validate_generation_24,
    }
}

pub(crate) fn generation_24_to_25_edge() -> TransitionEdge {
    TransitionEdge {
        key: "retire-owner-signing-authority",
        source: GENERATION_24,
        target: GENERATION_25,
        observe_source: observe_generation_24,
        apply: apply_generation_25,
        validate_target: validate_generation_25,
    }
}
