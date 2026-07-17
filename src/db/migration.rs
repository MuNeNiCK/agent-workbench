use std::path::Path;

use crate::identity::{CanonicalValue, domain_digest};
use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use super::{
    closure_migration::*, completion_migration::*, integrity_migration::*, legacy_migration::*,
    project::*, schema::SCHEMA_BATCHES, status::*, *,
};

pub(crate) fn open_ledger(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)
        .with_context(|| format!("failed to open ledger {}", path.display()))?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(conn)
}

pub(crate) fn open_existing_project(root: &Path) -> Result<Connection> {
    let ledger_path = default_ledger_path(root);
    if !ledger_path.exists() {
        bail!("project is not initialized; run agent-workbench init");
    }
    let integrity = super::project_integrity::evaluate_project_integrity(root);
    if let Some(error) = integrity.diagnostic_error.as_deref() {
        bail!("project integrity evaluation failed without a global classification: {error}");
    }
    if integrity.status.result == "blocked" {
        let predicate = integrity
            .status
            .predicates
            .iter()
            .find(|predicate| predicate.result == "blocked")
            .context("blocked project integrity has no blocking predicate")?;
        bail!(
            "project integrity {} {} blocks this command: {}; next: {}",
            predicate.code,
            predicate.name,
            predicate.evidence,
            predicate
                .next_action
                .as_deref()
                .unwrap_or("external recovery")
        );
    }
    let conn = integrity
        .connection
        .context("integrity evaluator lost ledger connection")?;
    migrate_if_needed(&conn)?;
    Ok(conn)
}

pub(super) fn migrate_if_needed(conn: &Connection) -> Result<()> {
    let source_generation = conn
        .query_row(
            "select coalesce(max(version),0) from schema_migrations",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    if source_generation == 11 {
        let tx = conn.unchecked_transaction()?;
        let project = project_id(&tx)?;
        let digest =
            super::adjudication_migration::legacy_source_digest(&tx, project, source_generation)?;
        migrate(&tx)?;
        tx.execute("insert or ignore into authority_migration_sources(project_id,source_ledger_digest,source_generation,created_at) values(?1,?2,?3,current_timestamp)",params![project,digest,source_generation])?;
        migrate_legacy_review_candidates(&tx)?;
        tx.execute("insert into legacy_adjudication_migrations(project_id,source_ledger_digest,source_generation,completed_at) select project_id,source_ledger_digest,source_generation,current_timestamp from authority_migration_sources where project_id=?1",params![project])?;
        tx.commit()?;
        return Ok(());
    }
    if ledger_needs_migration(conn)? {
        migrate(conn)?;
    }
    let pending:i64=conn.query_row("select exists(select 1 from authority_migration_sources s where not exists(select 1 from legacy_adjudication_migrations m where m.project_id=s.project_id))",[],|row|row.get(0)).unwrap_or(0);
    if pending == 1 {
        let tx = conn.unchecked_transaction()?;
        migrate_legacy_review_candidates(&tx)?;
        let project = project_id(&tx)?;
        tx.execute("insert into legacy_adjudication_migrations(project_id,source_ledger_digest,source_generation,completed_at) select project_id,source_ledger_digest,source_generation,current_timestamp from authority_migration_sources where project_id=?1",params![project])?;
        tx.commit()?;
    }
    Ok(())
}

pub(crate) fn open_authority_migration_project(root: &Path) -> Result<Connection> {
    let ledger_path = default_ledger_path(root);
    if !ledger_path.exists() {
        bail!("project is not initialized; run agent-workbench init");
    }
    let conn = open_ledger(&ledger_path)?;
    let source_generation = conn
        .query_row(
            "select coalesce(max(version),0) from schema_migrations",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    if source_generation == 11 {
        let tx = conn.unchecked_transaction()?;
        let project = project_id(&tx)?;
        let digest =
            super::adjudication_migration::legacy_source_digest(&tx, project, source_generation)?;
        migrate(&tx)?;
        tx.execute("insert or ignore into authority_migration_sources(project_id,source_ledger_digest,source_generation,created_at) values(?1,?2,?3,current_timestamp)",params![project,digest,source_generation])?;
        tx.commit()?;
    } else {
        migrate(&conn)?;
    }
    Ok(conn)
}

pub(super) fn ledger_needs_migration(conn: &Connection) -> Result<bool> {
    if !table_exists(conn, "schema_migrations")? {
        return Ok(true);
    }
    let schema_version = conn
        .query_row(
            "select version from schema_migrations order by version desc limit 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(0);
    if schema_version < SCHEMA_VERSION {
        return Ok(true);
    }
    if !table_exists(conn, "closure_attempts")?
        || !table_exists(conn, "authority_principals")?
        || !table_exists(conn, "authority_grant_epochs")?
        || !table_exists(conn, "authority_bootstrap_journals")?
        || !table_exists(conn, "review_provenance_records")?
        || !table_exists(conn, "review_adjudication_decisions")?
        || !table_exists(conn, "finding_disposition_decisions")?
        || !table_exists(conn, "verification_adjudication_decisions")?
        || !table_exists(conn, "finding_lifecycle_events")?
        || !table_exists(conn, "decision_continuations")?
        || !table_exists(conn, "review_correction_events")?
        || !table_exists(conn, "review_boundary_snapshots")?
        || !table_exists(conn, "review_correction_recovery_obligations")?
        || !table_exists(conn, "finding_decision_epochs")?
        || !table_exists(conn, "legacy_reviewer_bindings")?
        || !table_exists(conn, "legacy_claim_audits")?
        || !table_exists(conn, "legacy_finding_audits")?
        || !table_exists(conn, "legacy_migration_candidates")?
        || !table_exists(conn, "legacy_migration_candidate_members")?
        || !table_exists(conn, "legacy_migration_edges")?
        || !table_exists(conn, "legacy_migration_projections")?
        || !table_exists(conn, "authority_bootstrap_targets")?
        || !table_exists(conn, "authority_migration_sources")?
        || !table_exists(conn, "legacy_adjudication_migrations")?
        || !table_exists(conn, "owner_decision_grants")?
        || !table_exists(conn, "decision_capabilities")?
        || !table_exists(conn, "owner_decisions")?
        || !table_exists(conn, "finding_remediation_bindings")?
        || !table_exists(conn, "finding_remediation_recovery_epochs")?
        || !table_exists(conn, "correction_sessions")?
        || !table_exists(conn, "correction_tokens")?
        || !table_exists(conn, "correction_transition_aliases")?
        || !table_exists(conn, "correction_application_identity_links")?
        || !table_exists(conn, "correction_completion_inheritance_sources")?
        || !table_exists(conn, "correction_completion_inheritance_evidence")?
    {
        return Ok(true);
    }
    if !table_has_column(conn, "authority_assertions", "envelope_cbor")? {
        return Ok(true);
    }
    let current_task_gates: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='view' and name='current_task_validation_gates')",
        [],
        |row| row.get(0),
    )?;
    if !current_task_gates {
        return Ok(true);
    }
    let valid_inheritance_view: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='view' and name='valid_completion_inheritance_sources' and sql like '%mapping.design_requirement_id=source.current_requirement_id%' and sql like '%candidate_version.version_number%' and sql like '%left join command_usages usage%')",
        [],
        |row| row.get(0),
    )?;
    if !valid_inheritance_view {
        return Ok(true);
    }
    let correction_status_triggers: bool = conn.query_row(
        r#"
        select exists(select 1 from sqlite_schema where type='trigger' and name='trg_correction_session_status_update')
           and exists(select 1 from sqlite_schema where type='trigger' and name='trg_correction_token_status_update')
        "#,
        [],
        |row| row.get(0),
    )?;
    if !correction_status_triggers {
        return Ok(true);
    }
    let correction_semantic_triggers: bool = conn.query_row(
        r#"
        select exists(select 1 from sqlite_schema where type='trigger' and name='trg_correction_token_links_insert' and sql like '%phase_dependency_max%')
           and exists(select 1 from sqlite_schema where type='trigger' and name='trg_correction_application_links_insert' and sql like '%work_phase_task_memberships%')
           and exists(select 1 from sqlite_schema where type='trigger' and name='trg_correction_alias_links_insert' and sql like '%@superseded-task/%')
           and exists(select 1 from sqlite_schema where type='trigger' and name='trg_correction_identity_link_insert' and sql like '%completion_source%')
           and exists(select 1 from sqlite_schema where type='trigger' and name='trg_completion_source_insert' and sql like '%candidate_version.version_number%')
        "#,
        [],
        |row| row.get(0),
    )?;
    if !correction_semantic_triggers {
        return Ok(true);
    }
    let completion_identity_kind: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='table' and name='correction_application_identity_links' and sql like '%completion_source%')",
        [],
        |row| row.get(0),
    )?;
    if !completion_identity_kind {
        return Ok(true);
    }
    if !table_has_column(conn, "acceptance_records", "coverage_item_id")? {
        return Ok(true);
    }
    let incomplete_task_bundles: i64 = conn.query_row(
        r#"
        select exists(
            select 1 from tasks t
            where 0 and t.status = 'accepted_out_of_scope'
              and (
                exists (select 1 from checklist_items ci where ci.task_id = t.id and ci.status in ('open', 'blocked'))
                or exists (select 1 from validation_gates vg where vg.task_id = t.id and vg.status in ('active', 'stale'))
                or exists (
                    select 1 from checklist_items ci where ci.task_id = t.id
                      and not exists (select 1 from acceptance_records ar
                                      where ar.target_type='checklist_item'
                                        and ar.checklist_item_id=ci.id and ar.status='approved')
                )
                or exists (
                    select 1 from validation_gates vg where vg.task_id = t.id
                      and not exists (select 1 from acceptance_records ar
                                      where ar.target_type='validation_gate'
                                        and ar.validation_gate_id=vg.id and ar.status='approved')
                )
                or exists (
                    select 1 from task_derivations td
                    where td.task_id = t.id and not exists (
                        select 1 from coverage_items c
                        where c.task_id = t.id and c.design_requirement_id = td.design_requirement_id
                          and c.status = 'accepted_out_of_scope'
                          and exists (
                              select 1 from acceptance_records ar
                              where ar.target_type = 'coverage_item'
                                and ar.coverage_item_id = c.id
                                and ar.acceptance_type = 'accepted_out_of_scope'
                                and ar.status = 'approved'
                          )
                    )
                )
              )
        )
        "#,
        [],
        |row| row.get(0),
    )?;
    if incomplete_task_bundles > 0 {
        return Ok(true);
    }

    let broken_acceptance_refs: i64 = conn.query_row(
        r#"
        select count(*)
        from sqlite_schema
        where sql like '%acceptance_records_old%'
        "#,
        [],
        |row| row.get(0),
    )?;
    if broken_acceptance_refs > 0 {
        return Ok(true);
    }

    if acceptance_records_needs_migration(conn)? {
        return Ok(true);
    }

    if table_exists(conn, "review_runs")?
        && !table_has_column(conn, "review_runs", "review_provenance")?
    {
        return Ok(true);
    }
    if table_exists(conn, "closures")?
        && !table_has_column(conn, "closures", "supersession_reason")?
    {
        return Ok(true);
    }

    Ok(false)
}

fn execute_schema_batches(conn: &Connection) -> Result<()> {
    for batch in SCHEMA_BATCHES {
        conn.execute_batch(batch)?;
    }
    Ok(())
}

pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    if !conn.is_autocommit() {
        return migrate_steps(conn);
    }
    let transaction = conn.unchecked_transaction()?;
    migrate_steps(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn migrate_steps(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        drop trigger if exists trg_remediation_binding_insert;
        drop trigger if exists trg_remediation_binding_immutable_update;
        drop trigger if exists trg_remediation_binding_immutable_delete;
        drop trigger if exists trg_remediation_recovery_epoch_insert;
        drop trigger if exists trg_remediation_recovery_epoch_immutable_update;
        drop trigger if exists trg_remediation_recovery_epoch_immutable_delete;
        drop trigger if exists trg_correction_session_links_insert;
        drop trigger if exists trg_correction_session_links_update;
        drop trigger if exists trg_correction_session_status_update;
        drop trigger if exists trg_correction_session_immutable_delete;
        drop trigger if exists trg_correction_token_links_insert;
        drop trigger if exists trg_correction_token_links_update;
        drop trigger if exists trg_correction_token_status_update;
        drop trigger if exists trg_correction_token_immutable_delete;
        drop trigger if exists trg_correction_application_links_insert;
        drop trigger if exists trg_correction_application_links_update;
        drop trigger if exists trg_correction_application_immutable_delete;
        drop trigger if exists trg_correction_alias_links_insert;
        drop trigger if exists trg_correction_alias_immutable_update;
        drop trigger if exists trg_correction_alias_immutable_delete;
        drop trigger if exists trg_completion_source_insert;
        drop trigger if exists trg_completion_evidence_insert;
        drop trigger if exists trg_completion_source_immutable_update;
        drop trigger if exists trg_completion_source_immutable_delete;
        drop trigger if exists trg_completion_evidence_immutable_update;
        drop trigger if exists trg_completion_evidence_immutable_delete;
        drop trigger if exists trg_review_adjudication_project_insert;
        drop trigger if exists trg_finding_disposition_project_insert;
        drop trigger if exists trg_verification_adjudication_project_insert;
        drop trigger if exists trg_review_correction_project_insert;
        drop trigger if exists trg_review_boundary_project_insert;
        drop trigger if exists trg_finding_epoch_project_insert;
        drop view if exists valid_completion_inheritance_sources;
        "#,
    )?;
    prepare_acceptance_records_for_schema(conn)?;
    migrate_completion_identity_link_kind(conn)?;
    prepare_review_runs_for_schema(conn)?;
    prepare_review_invocations_for_schema(conn)?;
    prepare_adjudication_for_schema(conn)?;
    prepare_project_scoped_ledger_rows_for_schema(conn)?;
    drop_phase_review_target_reference_triggers(conn)?;
    execute_schema_batches(conn)?;
    ensure_column(
        conn,
        "legacy_migration_candidates",
        "boundary_generation",
        "integer",
    )?;
    ensure_column(
        conn,
        "legacy_migration_candidates",
        "commit_sequence",
        "integer",
    )?;
    conn.execute(
        r#"update validation_gates
           set status='closed'
           where status='active' and task_id in (
             select id from tasks where status in ('closed','accepted_out_of_scope')
           )"#,
        [],
    )?;
    conn.execute_batch(
        r#"
        drop trigger if exists trg_remediation_binding_insert;
        drop trigger if exists trg_remediation_binding_immutable_update;
        drop trigger if exists trg_remediation_binding_immutable_delete;
        drop trigger if exists trg_remediation_recovery_epoch_insert;
        drop trigger if exists trg_remediation_recovery_epoch_immutable_update;
        drop trigger if exists trg_remediation_recovery_epoch_immutable_delete;
        drop trigger if exists trg_correction_session_links_insert;
        drop trigger if exists trg_correction_session_links_update;
        drop trigger if exists trg_correction_session_status_update;
        drop trigger if exists trg_correction_session_immutable_delete;
        drop trigger if exists trg_correction_token_links_insert;
        drop trigger if exists trg_correction_token_links_update;
        drop trigger if exists trg_correction_token_status_update;
        drop trigger if exists trg_correction_token_immutable_delete;
        drop trigger if exists trg_correction_application_links_insert;
        drop trigger if exists trg_correction_application_links_update;
        drop trigger if exists trg_correction_application_immutable_delete;
        drop trigger if exists trg_correction_alias_links_insert;
        drop trigger if exists trg_correction_alias_immutable_update;
        drop trigger if exists trg_correction_alias_immutable_delete;
        "#,
    )?;
    migrate_acceptance_records(conn)?;
    repair_acceptance_record_references(conn)?;
    migrate_repository_snapshot_comparisons(conn)?;
    migrate_kpt_items(conn)?;
    conn.execute_batch(
        "drop trigger if exists trg_review_adjudication_project_insert;
         drop trigger if exists trg_finding_disposition_project_insert;
         drop trigger if exists trg_verification_adjudication_project_insert;
         drop trigger if exists trg_review_correction_project_insert;
         drop trigger if exists trg_review_boundary_project_insert;
         drop trigger if exists trg_finding_epoch_project_insert;",
    )?;
    migrate_review_runs(conn)?;
    migrate_resume_check_items(conn)?;
    validate_project_scoped_ledger_links(conn)?;
    validate_review_required_links(conn)?;
    refresh_review_integrity_triggers(conn)?;
    refresh_ledger_integrity_triggers(conn)?;
    ensure_phase_schema(conn)?;
    migrate_review_runs_phase_targets(conn)?;
    conn.execute_batch(
        "drop trigger if exists trg_verification_adjudication_project_insert;
         drop trigger if exists trg_finding_epoch_project_insert;",
    )?;
    ensure_closure_lifecycle_schema(conn)?;
    ensure_phase_review_target_reference_triggers(conn)?;
    execute_schema_batches(conn)?;
    ensure_phase_review_target_reference_triggers(conn)?;
    ensure_column(conn, "work_record_forks", "source_git_commit_sha", "text")?;
    ensure_column(conn, "work_records", "project_id", "integer")?;
    ensure_column(conn, "command_usages", "project_id", "integer")?;
    ensure_column(conn, "authority_events", "authority_id", "integer")?;
    ensure_column(conn, "rule_bindings", "review_policy_id", "integer")?;
    ensure_column(conn, "rule_bindings", "review_plan_id", "integer")?;
    ensure_column(conn, "rule_bindings", "validation_gate_id", "integer")?;
    ensure_column(conn, "rule_bindings", "acceptance_record_id", "integer")?;
    ensure_column(conn, "design_packages", "package_id", "text")?;
    ensure_column(conn, "design_packages", "root_path", "text")?;
    ensure_column(conn, "design_packages", "format", "text")?;
    ensure_column(conn, "design_packages", "version", "integer")?;
    ensure_column(conn, "design_packages", "package_hash", "text")?;
    ensure_column(conn, "design_versions", "source_ref", "text")?;
    ensure_column(conn, "design_versions", "package_hash", "text")?;
    ensure_column(conn, "design_versions", "approved_at", "text")?;
    ensure_column(
        conn,
        "resume_checks",
        "repository_state_revision",
        "integer",
    )?;
    ensure_column(conn, "review_runs", "file_path", "text")?;
    ensure_column(conn, "review_runs", "symbol", "text")?;
    ensure_column(
        conn,
        "review_runs",
        "review_provenance",
        "text not null default 'self_recorded'",
    )?;
    ensure_column(conn, "review_runs", "review_provenance_ref", "text")?;
    ensure_column(conn, "review_runs", "finding_fix_result", "text")?;
    ensure_column(
        conn,
        "finding_verifications",
        "closure_attempt_id",
        "integer",
    )?;
    ensure_column(
        conn,
        "review_plans",
        "fresh_review_after_run_id",
        "integer not null default 0",
    )?;
    backfill_authorities(conn)?;
    let had_work_record_commit_auto_linked =
        table_has_column(conn, "work_record_commits", "auto_linked")?;
    let had_work_record_file_auto_linked =
        table_has_column(conn, "work_record_files", "auto_linked")?;
    let had_work_record_file_repository_auto_linked =
        table_has_column(conn, "work_record_files", "repository_auto_linked")?;
    ensure_column(
        conn,
        "work_record_commits",
        "auto_linked",
        "integer not null default 0",
    )?;
    ensure_column(
        conn,
        "work_record_files",
        "auto_linked",
        "integer not null default 0",
    )?;
    ensure_column(
        conn,
        "work_record_files",
        "repository_auto_linked",
        "integer not null default 0",
    )?;
    migrate_work_record_auto_link_markers(
        conn,
        had_work_record_commit_auto_linked,
        had_work_record_file_auto_linked,
        had_work_record_file_repository_auto_linked,
    )?;
    ensure_column(conn, "acceptance_records", "design_package_key", "text")?;
    ensure_column(conn, "acceptance_records", "design_file_path", "text")?;
    ensure_column(conn, "acceptance_records", "design_requirement_key", "text")?;
    ensure_column(conn, "acceptance_records", "coverage_item_id", "integer")?;
    ensure_column(conn, "acceptance_records", "finding_id", "integer")?;
    ensure_column(conn, "acceptance_records", "validation_gate_id", "integer")?;
    ensure_column(conn, "acceptance_records", "validation_run_id", "integer")?;
    ensure_column(
        conn,
        "acceptance_records",
        "repository_state_classification_id",
        "integer",
    )?;
    ensure_column(
        conn,
        "acceptance_records",
        "repository_snapshot_comparison_id",
        "integer",
    )?;
    ensure_column(conn, "acceptance_records", "review_plan_id", "integer")?;
    ensure_column(conn, "acceptance_records", "checklist_item_id", "integer")?;
    ensure_column(conn, "acceptance_records", "command_profile_id", "integer")?;
    ensure_column(conn, "acceptance_records", "command_usage_id", "integer")?;
    ensure_column(
        conn,
        "acceptance_records",
        "command_deviation_id",
        "integer",
    )?;
    ensure_column(conn, "acceptance_records", "rule_binding_id", "integer")?;
    ensure_column(conn, "acceptance_records", "stale_record_type", "text")?;
    ensure_column(conn, "acceptance_records", "stale_record_id", "integer")?;
    ensure_column(conn, "validation_runs", "command", "text")?;
    ensure_column(conn, "validation_runs", "classification", "text")?;
    ensure_column(conn, "validation_runs", "acceptance_record_id", "integer")?;
    ensure_column(conn, "acceptance_records", "finding_id", "integer")?;
    ensure_column(conn, "acceptance_records", "validation_gate_id", "integer")?;
    ensure_column(conn, "acceptance_records", "validation_run_id", "integer")?;
    ensure_column(
        conn,
        "acceptance_records",
        "repository_state_classification_id",
        "integer",
    )?;
    ensure_column(
        conn,
        "acceptance_records",
        "repository_snapshot_comparison_id",
        "integer",
    )?;
    ensure_column(conn, "acceptance_records", "review_plan_id", "integer")?;
    ensure_column(conn, "acceptance_records", "checklist_item_id", "integer")?;
    ensure_column(conn, "acceptance_records", "command_profile_id", "integer")?;
    ensure_column(conn, "acceptance_records", "command_usage_id", "integer")?;
    ensure_column(
        conn,
        "acceptance_records",
        "command_deviation_id",
        "integer",
    )?;
    ensure_column(conn, "acceptance_records", "rule_binding_id", "integer")?;
    ensure_column(conn, "acceptance_records", "stale_record_type", "text")?;
    ensure_column(conn, "acceptance_records", "stale_record_id", "integer")?;

    ensure_completion_inheritance_triggers(conn)?;

    let current_version = conn
        .query_row(
            "select version from schema_migrations order by version desc limit 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(0);

    if current_version < SCHEMA_VERSION {
        conn.execute(
            "insert into schema_migrations(version, applied_at) values (?1, current_timestamp)",
            params![SCHEMA_VERSION],
        )?;
    }

    Ok(())
}

fn migrate_legacy_review_candidates(conn: &Connection) -> Result<()> {
    let project: Option<i64> = conn
        .query_row("select id from projects order by id limit 1", [], |row| {
            row.get(0)
        })
        .optional()?;
    let Some(project) = project else {
        return Ok(());
    };
    super::adjudication_migration::validate_schema11_invalid_combinations(conn, project)?;
    normalize_legacy_finding_lifecycle(conn, project)?;
    materialize_legacy_verification_claims(conn, project)?;
    super::adjudication_migration::normalize_schema11_adjudication(conn, project)?;
    let mut stmt=conn.prepare("select r.id,r.clean_run,r.new_findings_count,r.status,coalesce(r.target_ref,''),p.work_unit_id,coalesce(w.status,''),coalesce(v.status,''),r.run_type,r.review_plan_id,coalesce(v.content_hash,'') from review_runs r join review_plans p on p.id=r.review_plan_id join work_units w on w.id=p.work_unit_id left join design_versions v on v.id=p.design_version_id where r.project_id=?1 and r.status='completed' order by r.id")?;
    let rows = stmt
        .query_map(params![project], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, String>(10)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    let mut targets = Vec::new();
    for (
        run,
        clean,
        count,
        status,
        target,
        work,
        _work_status,
        _design_status,
        run_type,
        plan_id,
        design_context,
    ) in rows
    {
        let already_migrated: i64 = conn.query_row(
            "select exists(select 1 from legacy_claim_audits where project_id=?1 and review_run_id=?2)",
            params![project, run],
            |row| row.get(0),
        )?;
        if already_migrated == 1 {
            continue;
        }
        let mut principals=conn.prepare("select distinct reviewer_principal_id from review_agent_invocations where project_id=?1 and review_run_id=?2 and reviewer_principal_id is not null and review_provenance_id is not null")?.query_map(params![project,run],|row|row.get::<_,i64>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let reviewer_digests=conn.prepare("select distinct lower(legacy_source_reviewer_digest) from review_agent_invocations where project_id=?1 and review_run_id=?2 and length(legacy_source_reviewer_digest)=64")?.query_map(params![project,run],|row|row.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        for reviewer_digest in reviewer_digests {
            let bound=conn.prepare("select distinct b.principal_id from legacy_reviewer_bindings b join authority_migration_sources s on s.project_id=b.project_id and s.source_ledger_digest=b.source_ledger_digest and s.source_generation=b.source_generation where b.project_id=?1 and b.source_generation=11 and b.source_reviewer_digest=?2")?.query_map(params![project,reviewer_digest],|row|row.get::<_,i64>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
            principals.extend(bound);
        }
        principals.sort_unstable();
        principals.dedup();
        let resolution = match principals.as_slice() {
            [] => "unbound",
            [_] => "trusted",
            _ => "ambiguous",
        };
        if let [principal] = principals.as_slice() {
            let updated=conn.execute("update review_agent_invocations set reviewer_principal_id=?1,claim=case when claim is null then case when ?2=1 then 'clean' when ?3>0 then 'findings' else 'inconclusive' end else claim end,target_context=coalesce(target_context,?4),purpose=coalesce(purpose,'new_unbiased_review') where project_id=?5 and review_run_id=?6 and reviewer_principal_id is null",params![principal,clean,count,target,project,run])?;
            let exists:i64=conn.query_row("select exists(select 1 from review_agent_invocations where project_id=?1 and review_run_id=?2 and reviewer_principal_id=?3)",params![project,run,principal],|row|row.get(0))?;
            if updated == 0 && exists == 0 {
                conn.execute("insert into review_agent_invocations(project_id,review_plan_id,review_run_id,reviewer_principal_id,target_context,purpose,claim,run_type,agent_label,status,finished_at) values(?1,?2,?3,?4,?5,'new_unbiased_review',case when ?6=1 then 'clean' when ?7>0 then 'findings' else 'inconclusive' end,?8,'schema-11-migration','completed',current_timestamp)",params![project,plan_id,run,principal,target,clean,count,run_type])?;
            }
        }
        let findings: i64 = conn.query_row(
            "select count(*) from findings where project_id=?1 and review_run_id=?2",
            params![project, run],
            |row| row.get(0),
        )?;
        if clean == 1 && (count > 0 || findings > 0) {
            bail!("migration ambiguity: clean_with_findings run {run}");
        }
        let kind = if clean == 1 {
            "clean"
        } else if findings > 0 {
            "findings"
        } else {
            "inconclusive"
        };
        let digest = domain_digest(
            b"agent-workbench:legacy-claim-candidate-v1\0",
            &CanonicalValue::object([
                ("run", CanonicalValue::Integer(run)),
                ("kind", CanonicalValue::string(kind)),
                ("target", CanonicalValue::string(&target)),
            ]),
        );
        conn.execute("insert or ignore into legacy_claim_audits(project_id,review_run_id,candidate_kind,content_digest,reviewer_resolution,mapping_row,before_lifecycle,after_lifecycle,created_at) values(?1,?2,?3,?4,?5,?6,?7,?8,current_timestamp)",params![project,run,kind,digest,resolution,format!("completed_{kind}"),status,if resolution=="trusted"{"pending_adjudication"}else{"audit_only"}])?;
        let mut finding_stmt=conn.prepare("select id,finding_type,severity,description from findings where project_id=?1 and review_run_id=?2 order by id")?;
        let inventory = finding_stmt
            .query_map(params![project, run], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(finding_stmt);
        for (id, ty, severity, description) in inventory {
            let fd = domain_digest(
                b"agent-workbench:legacy-finding-candidate-v1\0",
                &CanonicalValue::object([
                    ("finding", CanonicalValue::Integer(id)),
                    ("type", CanonicalValue::string(ty)),
                    ("severity", CanonicalValue::string(severity)),
                    ("description", CanonicalValue::string(description)),
                ]),
            );
            conn.execute("insert or ignore into legacy_finding_audits(project_id,finding_id,review_run_id,content_digest,created_at) values(?1,?2,?3,?4,current_timestamp)",params![project,id,run,fd])?;
        }
        let boundaries=conn.prepare("with eligible as(select distinct b.candidate_handle,b.boundary_generation,b.commit_sequence from legacy_migration_edges e join legacy_migration_candidates b on b.id=e.source_candidate_id join legacy_migration_candidate_members target on target.candidate_id=e.target_candidate_id where e.project_id=?1 and e.edge_kind='boundary_consumes' and target.source_table='review_runs' and target.source_row_id=?2), greatest as(select max(boundary_generation) generation from eligible), sequence as(select max(commit_sequence) value from eligible where boundary_generation=(select generation from greatest)) select candidate_handle from eligible where boundary_generation=(select generation from greatest) and commit_sequence=(select value from sequence) order by candidate_handle")?.query_map(params![project,run],|row|row.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        if boundaries.len() > 1 {
            bail!("migration ambiguity: conflicting_greatest_completed_boundary");
        }
        let consumed_boundary = boundaries.into_iter().next();
        if clean == 1 && consumed_boundary.is_none() {
            conn.execute(
                "update review_plans set status=?1 where project_id=?2 and id=?3 and status='clean'",
                params![if resolution == "trusted" { "blocked" } else { "open" }, project, plan_id],
            )?;
        }
        if let ("trusted", 1, Some(boundary)) = (resolution, clean, consumed_boundary) {
            let prior_adjudication: i64 = conn.query_row("select exists(select 1 from review_adjudication_decisions where project_id=?1 and review_run_id=?2)",params![project,run],|row|row.get(0))?;
            if prior_adjudication == 1 {
                continue;
            }
            let owner = format!("work_unit:{work}");
            let claim = format!("review_run:{run}");
            let context = if design_context.len() == 64 {
                design_context
            } else {
                domain_digest(
                    b"agent-workbench:legacy-context-v1\0",
                    &CanonicalValue::string(&target),
                )
            };
            targets.push((owner, boundary, claim, context));
        }
    }
    super::adjudication_migration::record_candidate_projections(conn, project)?;
    let existing: i64 = conn.query_row(
        "select count(*) from authority_grant_epochs where project_id=?1",
        params![project],
        |row| row.get(0),
    )?;
    if !targets.is_empty() && existing == 0 {
        targets.sort();
        let encoded_targets = CanonicalValue::Array(
            targets
                .iter()
                .map(|(o, b, c, x)| {
                    CanonicalValue::object([
                        ("owner", CanonicalValue::string(o)),
                        ("boundary", CanonicalValue::string(b)),
                        ("claim", CanonicalValue::string(c)),
                        ("context", CanonicalValue::string(x)),
                    ])
                })
                .collect(),
        );
        let (source_digest,source_generation):(String,i64)=conn.query_row("select source_ledger_digest,source_generation from authority_migration_sources where project_id=?1",params![project],|row|Ok((row.get(0)?,row.get(1)?)))?;
        let bindings=conn.prepare("select binding_handle||':'||payload_digest from legacy_reviewer_bindings where project_id=?1 order by binding_handle")?.query_map(params![project],|row|row.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let resolutions=conn.prepare("select content_digest||':'||reviewer_resolution from legacy_claim_audits where project_id=?1 order by content_digest")?.query_map(params![project],|row|row.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let owners = conn
            .prepare("select id||':'||status from work_units where project_id=?1 order by id")?
            .query_map(params![project], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let projections=conn.prepare("select c.content_digest||':'||p.stratum||':'||p.mapping_row||':'||p.after_lifecycle from legacy_migration_projections p join legacy_migration_candidates c on c.id=p.candidate_id where p.project_id=?1 order by c.content_digest,p.stratum")?.query_map(params![project],|row|row.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let adjudications=conn.prepare("select decision_handle||':'||decision_value from owner_decisions where project_id=?1 order by decision_handle")?.query_map(params![project],|row|row.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let corrections=conn.prepare("select snapshot_handle||':'||status||':'||coalesce(invalidated_at,'') from review_boundary_snapshots where project_id=?1 order by snapshot_handle")?.query_map(params![project],|row|row.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let prior_epochs=conn.prepare("select epoch_digest||':'||status from authority_grant_epochs where project_id=?1 order by epoch_digest")?.query_map(params![project],|row|row.get::<_,String>(0))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let strings = |values: Vec<String>| {
            CanonicalValue::Array(values.into_iter().map(CanonicalValue::String).collect())
        };
        let snapshot = CanonicalValue::object([
            ("source_digest", CanonicalValue::string(&source_digest)),
            (
                "source_generation",
                CanonicalValue::Integer(source_generation),
            ),
            ("bindings", strings(bindings)),
            ("reviewer_resolutions", strings(resolutions)),
            ("owners", strings(owners)),
            ("plan_gate_projections", strings(projections)),
            ("adjudications", strings(adjudications)),
            ("corrections", strings(corrections)),
            ("prior_epochs", strings(prior_epochs)),
            ("targets", encoded_targets),
        ]);
        let epoch_digest = domain_digest(b"agent-workbench:bootstrap-epoch-v2\0", &snapshot);
        let trust = conn.query_row(
            "select coalesce(min(a.trust_digest),?1) from authority_assertions a",
            params!["00".repeat(32)],
            |row| row.get::<_, String>(0),
        )?;
        conn.execute("insert into authority_grant_epochs(project_id,epoch_digest,trust_digest,status,created_at) values(?1,?2,?3,'open',current_timestamp)",params![project,epoch_digest,trust])?;
        let epoch = conn.last_insert_rowid();
        for (owner, boundary, claim, context) in targets {
            let handle =
                format!("bootstrap_target:{project}:{epoch}:{owner}:{boundary}:{claim}:{context}");
            conn.execute("insert into authority_bootstrap_targets(project_id,epoch_id,target_handle,owner_ref,boundary_handle,claim_handle,context_digest,status,created_at) values(?1,?2,?3,?4,?5,?6,?7,'pending',current_timestamp)",params![project,epoch,handle,owner,boundary,claim,context])?;
        }
    }
    Ok(())
}

fn normalize_legacy_finding_lifecycle(conn: &Connection, project: i64) -> Result<()> {
    conn.execute("update findings set lifecycle_state='closed',status='closed',close_reason='legacy_rejected' where project_id=?1 and classification='invalid'",params![project])?;
    conn.execute("update findings set lifecycle_state='closed',close_reason='authority_disposed' where project_id=?1 and status='accepted_out_of_scope'",params![project])?;
    conn.execute("update findings set lifecycle_state='closed',status='closed',close_reason='verified' where project_id=?1 and exists(select 1 from finding_verifications v where v.project_id=findings.project_id and v.finding_id=findings.id and v.result='verified')",params![project])?;
    conn.execute("update findings set lifecycle_state='awaiting_verification',status='open',close_reason=null where project_id=?1 and lifecycle_state!='closed' and exists(select 1 from closures c join closure_attempts a on a.closure_id=c.id where c.project_id=findings.project_id and c.finding_id=findings.id and a.result is null)",params![project])?;
    conn.execute("update findings set lifecycle_state='remediating',status='open',close_reason=null where project_id=?1 and lifecycle_state='open' and exists(select 1 from closures c where c.project_id=findings.project_id and c.finding_id=findings.id and c.status!='superseded')",params![project])?;
    Ok(())
}

fn materialize_legacy_verification_claims(conn: &Connection, project: i64) -> Result<()> {
    conn.execute(
        "insert or ignore into finding_verifications(project_id,review_run_id,finding_id,closure_id,result,notes,created_at,closure_attempt_id) select r.project_id,r.id,f.id,c.id,r.finding_fix_result,'schema-11 immutable verification claim',r.created_at,a.id from review_runs r join findings f on r.target_ref like 'review-context:finding-fix:finding='||f.id||':%' join closures c on c.finding_id=f.id join closure_attempts a on a.closure_id=c.id and r.target_ref='review-context:finding-fix:finding='||f.id||':closure='||c.id||':attempt='||a.id where r.project_id=?1 and r.status='completed' and r.run_type='resume' and r.run_purpose='finding_fix_verification' and r.finding_fix_result in ('verified','not_fixed','needs_evidence')",
        params![project],
    )?;
    Ok(())
}

fn prepare_review_invocations_for_schema(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "review_agent_invocations")? {
        return Ok(());
    }
    for (name, definition) in [
        ("invocation_handle", "text"),
        ("reviewer_principal_id", "integer"),
        ("review_provenance_id", "integer"),
        ("target_context", "text"),
        ("purpose", "text"),
        ("request_idempotency_key", "text"),
        ("request_payload_digest", "text"),
        ("transition_idempotency_key", "text"),
        ("claim", "text"),
        ("verification_claim", "text"),
        ("closure_attempt_id", "integer"),
        ("result_summary", "text"),
        ("terminal_reason", "text"),
        ("legacy_source_reviewer_digest", "text"),
    ] {
        ensure_column(conn, "review_agent_invocations", name, definition)?;
    }
    let legacy_reviewers = conn
        .prepare("select id,external_agent_id from review_agent_invocations where legacy_source_reviewer_digest is null and external_agent_id is not null and trim(external_agent_id)!='' order by id")?
        .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (invocation, source_reference) in legacy_reviewers {
        // This digest identifies only the immutable legacy source reference. It
        // never resolves a principal without a separately signed binding.
        let digest = domain_digest(
            b"agent-workbench:legacy-source-reviewer-reference-v1\0",
            &CanonicalValue::string(&source_reference),
        );
        conn.execute(
            "update review_agent_invocations set legacy_source_reviewer_digest=?1 where id=?2",
            params![digest, invocation],
        )?;
    }
    Ok(())
}

fn prepare_adjudication_for_schema(conn: &Connection) -> Result<()> {
    if table_exists(conn, "findings")? {
        ensure_column(
            conn,
            "findings",
            "lifecycle_state",
            "text not null default 'open'",
        )?;
        ensure_column(conn, "findings", "close_reason", "text")?;
    }
    Ok(())
}
