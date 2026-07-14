use std::path::Path;

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
    if ledger_needs_migration(conn)? {
        migrate(conn)?;
    }
    Ok(())
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
        drop view if exists valid_completion_inheritance_sources;
        "#,
    )?;
    prepare_acceptance_records_for_schema(conn)?;
    migrate_completion_identity_link_kind(conn)?;
    prepare_review_runs_for_schema(conn)?;
    prepare_project_scoped_ledger_rows_for_schema(conn)?;
    drop_phase_review_target_reference_triggers(conn)?;
    execute_schema_batches(conn)?;
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
    migrate_review_runs(conn)?;
    migrate_resume_check_items(conn)?;
    validate_project_scoped_ledger_links(conn)?;
    validate_review_required_links(conn)?;
    refresh_review_integrity_triggers(conn)?;
    refresh_ledger_integrity_triggers(conn)?;
    ensure_phase_schema(conn)?;
    migrate_review_runs_phase_targets(conn)?;
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
