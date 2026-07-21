use std::path::{Component, Path, PathBuf};

use crate::identity::{CanonicalValue, domain_digest};
use crate::review_context::review_context_ref;
use std::fs::File;
use std::ops::{Deref, DerefMut};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};

use super::{
    closure_migration::*, completion_migration::*, integrity_migration::*, legacy_migration::*,
    project::*, schema::GENERATION_14_SQL, schema::GENERATION_15_APPLICATION_LINK_SQL,
    schema::GENERATION_15_SQL, schema::GENERATION_16_SQL, schema::GENERATION_17_SQL,
    schema::GENERATION_18_SQL, schema::GENERATION_19_SQL, schema::GENERATION_20_SQL,
    schema::GENERATION_21_FINDING_VERIFICATION_SQL, schema::GENERATION_21_SQL,
    schema::GENERATION_22_SQL, schema::GENERATION_23_SQL, schema::GENERATION_24_SQL,
    schema::SCHEMA_BATCHES, status::*, *,
};

pub(crate) fn install_storage_generation_14(conn: &Connection) -> Result<()> {
    conn.execute_batch(GENERATION_14_SQL)?;
    conn.execute(
        "insert or ignore into schema_migrations(version,applied_at) values(14,current_timestamp)",
        [],
    )?;
    Ok(())
}

pub(crate) fn install_storage_generation_15(conn: &Connection) -> Result<()> {
    conn.execute_batch(GENERATION_15_SQL)?;
    conn.execute_batch(GENERATION_15_APPLICATION_LINK_SQL)?;
    conn.execute_batch(crate::task_identity::schema::SQL)?;
    crate::phases::install_phase_epochs(conn)?;
    ensure_closure_lifecycle_schema(conn)?;
    install_correction_decomposition_task_membership_view(conn)?;
    conn.execute(
        "insert or ignore into schema_migrations(version,applied_at) values(15,current_timestamp)",
        [],
    )?;
    Ok(())
}

pub(crate) fn install_storage_generation_16(conn: &Connection) -> Result<()> {
    conn.execute_batch(GENERATION_16_SQL)?;
    crate::release::migrate_release_candidate_revisions(conn)?;
    conn.execute(
        "insert or ignore into schema_migrations(version,applied_at) values(16,current_timestamp)",
        [],
    )?;
    Ok(())
}

pub(crate) fn install_storage_generation_17(conn: &Connection) -> Result<()> {
    // This installer is intentionally adjacent: no current-state repair may synthesize effects.
    for (table, column, definition) in [
        (
            "decomposition_plans",
            "document_content",
            "text not null default ''",
        ),
        (
            "decomposition_plans",
            "content_identity",
            "text not null default ''",
        ),
        (
            "decomposition_plans",
            "design_package_id",
            "integer references design_packages(id)",
        ),
        (
            "decomposition_reconciliation_tasks",
            "effect",
            "text check(effect is null or effect in ('preserve','open'))",
        ),
        (
            "decomposition_reconciliation_checklist_items",
            "effect",
            "text check(effect is null or effect in ('preserve','open'))",
        ),
        (
            "decomposition_reconciliation_gates",
            "effect",
            "text check(effect is null or effect in ('preserve','open'))",
        ),
        (
            "decomposition_reconciliation_gates",
            "boundary_selector",
            "text",
        ),
        (
            "decomposition_reconciliation_gates",
            "resolved_boundary_identity",
            "text",
        ),
        (
            "decomposition_reconciliation_phases",
            "effect",
            "text check(effect is null or effect in ('preserve','open'))",
        ),
        (
            "decomposition_reconciliation_dependencies",
            "effect",
            "text check(effect is null or effect in ('preserve','open'))",
        ),
    ] {
        add_column_if_missing(conn, table, column, definition)?;
    }
    conn.execute_batch(GENERATION_17_SQL)?;
    for (table, column) in [
        ("decomposition_plans", "design_package_id"),
        ("decomposition_plans", "document_content"),
        ("decomposition_plans", "content_identity"),
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
        let installed: bool = conn.query_row(
            "select exists(select 1 from pragma_table_info(?1) where name=?2)",
            params![table, column],
            |row| row.get(0),
        )?;
        if !installed {
            bail!("storage generation 17 target is incomplete");
        }
    }
    conn.execute(
        "insert or ignore into schema_migrations(version,applied_at) values(17,current_timestamp)",
        [],
    )?;
    Ok(())
}

pub(crate) fn install_storage_generation_18(conn: &Connection) -> Result<()> {
    conn.execute_batch(GENERATION_18_SQL)?;
    crate::decomposition::backfill_reconciliation_results(conn)?;
    let incomplete: bool = conn.query_row(
        r#"
        select exists(
          select 1 from decomposition_reconciliation_applications application
          where not exists(
            select 1 from decomposition_reconciliation_results result
            where result.reconciliation_application_id=application.id
          )
        )
        "#,
        [],
        |row| row.get(0),
    )?;
    if incomplete {
        bail!("storage generation 18 target is incomplete");
    }
    conn.execute(
        "insert or ignore into schema_migrations(version,applied_at) values(18,current_timestamp)",
        [],
    )?;
    Ok(())
}

pub(crate) fn install_storage_generation_19(conn: &Connection) -> Result<()> {
    conn.execute_batch(GENERATION_19_SQL)?;
    conn.execute(
        "insert or ignore into schema_migrations(version,applied_at) values(19,current_timestamp)",
        [],
    )?;
    Ok(())
}

pub(crate) fn install_storage_generation_20(conn: &Connection) -> Result<()> {
    let conversion_supports_rule: bool = conn.query_row(
        "select coalesce((select sql like '%''rule''%' and sql like '%kpt_rule_id%' from sqlite_schema where type='table' and name='kpt_item_conversions'),0)",
        [],
        |row| row.get(0),
    )?;
    if !conversion_supports_rule {
        conn.execute_batch(
            r#"
            alter table kpt_item_conversions rename to kpt_item_conversions_generation_19;
            create table kpt_item_conversions (
                id integer primary key,
                kpt_item_id integer not null references kpt_items(id) on delete cascade,
                target_type text not null check(target_type in ('rule','correction','task','command_profile','review_policy','design_version','decision','user_correction')),
                kpt_rule_id integer references kpt_rules(id),
                task_id integer references tasks(id),
                command_profile_id integer references command_profiles(id),
                review_policy_id integer references review_policies(id),
                design_version_id integer references design_versions(id),
                decision_id integer references decisions(id),
                user_correction_id integer references user_corrections(id),
                item_revision text,
                predecessor_handle text,
                request_identity text,
                receipt_identity text,
                current_handle text,
                created_at text not null,
                check (
                    (target_type='rule' and kpt_rule_id is not null and task_id is null and command_profile_id is null and review_policy_id is null and design_version_id is null and decision_id is null and user_correction_id is null)
                    or (target_type in ('correction','user_correction') and kpt_rule_id is null and task_id is null and command_profile_id is null and review_policy_id is null and design_version_id is null and decision_id is null and user_correction_id is not null)
                    or (target_type='task' and kpt_rule_id is null and task_id is not null and command_profile_id is null and review_policy_id is null and design_version_id is null and decision_id is null and user_correction_id is null)
                    or (target_type='command_profile' and kpt_rule_id is null and task_id is null and command_profile_id is not null and review_policy_id is null and design_version_id is null and decision_id is null and user_correction_id is null)
                    or (target_type='review_policy' and kpt_rule_id is null and task_id is null and command_profile_id is null and review_policy_id is not null and design_version_id is null and decision_id is null and user_correction_id is null)
                    or (target_type='design_version' and kpt_rule_id is null and task_id is null and command_profile_id is null and review_policy_id is null and design_version_id is not null and decision_id is null and user_correction_id is null)
                    or (target_type='decision' and kpt_rule_id is null and task_id is null and command_profile_id is null and review_policy_id is null and design_version_id is null and decision_id is not null and user_correction_id is null)
                ),
                check (
                    (item_revision is null and predecessor_handle is null and request_identity is null and receipt_identity is null and current_handle is null)
                    or (item_revision is not null and predecessor_handle is not null and request_identity is not null and receipt_identity is not null and current_handle is not null)
                )
            );
            insert into kpt_item_conversions(
                id,kpt_item_id,target_type,task_id,command_profile_id,review_policy_id,
                design_version_id,decision_id,user_correction_id,created_at
            )
            select id,kpt_item_id,target_type,task_id,command_profile_id,review_policy_id,
                   design_version_id,decision_id,user_correction_id,created_at
            from kpt_item_conversions_generation_19;
            drop table kpt_item_conversions_generation_19;
            "#,
        )?;
    }
    for (column, definition) in [
        ("item_revision", "text"),
        ("predecessor_handle", "text"),
        ("request_identity", "text"),
        ("receipt_identity", "text"),
        ("current_handle", "text"),
    ] {
        add_column_if_missing(conn, "kpt_item_conversions", column, definition)?;
    }
    conn.execute_batch(GENERATION_20_SQL)?;
    conn.execute(
        r#"
        insert or ignore into kpt_item_sources(kpt_item_id,source_kind,source_identity,source_revision,created_at)
        select item.id,
               case when item.linked_user_correction_id is not null then 'correction'
                    when item.linked_review_finding_id is not null then 'finding'
                    else 'legacy-command-profile' end,
               cast(coalesce(item.linked_user_correction_id,item.linked_review_finding_id,item.linked_command_profile_id) as text),
               coalesce(correction.created_at,finding.created_at,profile.created_at,item.created_at),
               item.created_at
        from kpt_items item
        left join user_corrections correction on correction.id=item.linked_user_correction_id
        left join findings finding on finding.id=item.linked_review_finding_id
        left join command_profiles profile on profile.id=item.linked_command_profile_id
        where (item.linked_user_correction_id is not null)
           or (item.linked_review_finding_id is not null)
           or (item.linked_command_profile_id is not null)
        "#,
        [],
    )?;
    conn.execute(
        "insert or ignore into schema_migrations(version,applied_at) values(20,current_timestamp)",
        [],
    )?;
    Ok(())
}

pub(crate) fn install_storage_generation_21(conn: &Connection) -> Result<()> {
    ensure_closure_lifecycle_schema(conn)?;
    conn.execute_batch(GENERATION_21_SQL)?;
    let recovery_aware_verification: bool = conn.query_row(
        "select coalesce((select sql like '%finding_design_recoveries%' from sqlite_schema where type='trigger' and name='trg_finding_verification_project_insert'),0)",
        [],
        |row| row.get(0),
    )?;
    if !recovery_aware_verification {
        conn.execute_batch(
            "drop trigger if exists trg_finding_verification_project_insert;
             drop trigger if exists trg_finding_verification_project_update;",
        )?;
        conn.execute_batch(GENERATION_21_FINDING_VERIFICATION_SQL)?;
    }
    conn.execute(
        "insert or ignore into schema_migrations(version,applied_at) values(21,current_timestamp)",
        [],
    )?;
    Ok(())
}

pub(crate) fn install_storage_generation_22(conn: &Connection) -> Result<()> {
    let pending = inspect_pending_reconciliation_target_migrations(conn)?;
    if !pending.blockers.is_empty() {
        bail!(
            "pending legacy Decomposition Plan reconciliation targets cannot be migrated: {}",
            pending.blockers.join("; ")
        );
    }
    if !pending.changes.is_empty() {
        conn.execute_batch("drop trigger if exists trg_correction_token_links_update;")?;
        for change in &pending.changes {
            let updated = conn.execute(
                "update correction_tokens set target=?1 where id=?2 and status='pending' and operation='decomposition-plan-reconcile' and target=?3",
                params![change.after, change.token_id, change.before],
            )?;
            if updated != 1 {
                bail!(
                    "pending legacy Decomposition Plan reconciliation token {} changed before migration",
                    change.token_id
                );
            }
        }
    }
    ensure_closure_lifecycle_schema(conn)?;
    install_correction_decomposition_task_membership_view(conn)?;
    conn.execute_batch(GENERATION_22_SQL)?;
    backfill_decomposition_plan_ingress_identities(conn)?;
    conn.execute(
        "insert or ignore into schema_migrations(version,applied_at) values(22,current_timestamp)",
        [],
    )?;
    Ok(())
}

pub(crate) fn install_storage_generation_23(conn: &Connection) -> Result<()> {
    let legacy_continuations: bool = table_exists(conn, "decision_continuations")?
        && !conn.query_row(
            "select exists(select 1 from pragma_table_info('decision_continuations') where name='context_identity')",
            [],
            |row| row.get(0),
        )?;
    if legacy_continuations {
        let unbound_applied: i64 = conn.query_row(
            r#"
            select count(*) from decision_continuations continuation
            where continuation.status='applied' and (
              select count(*) from owner_decisions decision
              where decision.project_id=continuation.project_id
                and decision.owner_ref=continuation.owner_ref
                and decision.target_ref=continuation.target_ref
                and decision.decision_family=continuation.decision_family
                and decision.action=continuation.action
                and decision.expected_current=continuation.expected_current
            )!=1
            "#,
            [],
            |row| row.get(0),
        )?;
        if unbound_applied != 0 {
            bail!(
                "{unbound_applied} applied legacy decision continuations cannot be bound to exactly one retained owner decision"
            );
        }
        conn.execute_batch(
            "drop trigger if exists trg_decision_continuation_immutable;
             alter table decision_continuations rename to decision_continuations_generation_22;",
        )?;
    }

    conn.execute_batch(GENERATION_23_SQL)?;
    ensure_completion_inheritance_triggers(conn)?;

    if legacy_continuations {
        conn.execute_batch("drop trigger if exists trg_decision_continuation_insert;")?;
        conn.execute(
            r#"
            insert into decision_continuations(
              id,project_id,continuation_handle,command_kind,owner_ref,target_ref,
              decision_family,action,expected_current,context_identity,design_context,
              rejection_code,required_inputs,status,owner_decision_id,created_at,applied_at
            )
            select legacy.id,legacy.project_id,legacy.continuation_handle,legacy.command_kind,
                   legacy.owner_ref,legacy.target_ref,legacy.decision_family,legacy.action,
                   legacy.expected_current,legacy.design_context,legacy.design_context,
                   legacy.rejection_code,'decision,reason',legacy.status,
                   case when legacy.status='applied' then (
                     select decision.id from owner_decisions decision
                     where decision.project_id=legacy.project_id
                       and decision.owner_ref=legacy.owner_ref
                       and decision.target_ref=legacy.target_ref
                       and decision.decision_family=legacy.decision_family
                       and decision.action=legacy.action
                       and decision.expected_current=legacy.expected_current
                   ) end,
                   legacy.created_at,legacy.applied_at
            from decision_continuations_generation_22 legacy
            order by legacy.id
            "#,
            [],
        )?;
        conn.execute_batch(
            "drop table decision_continuations_generation_22;
             drop trigger if exists trg_decision_continuation_insert;",
        )?;
        conn.execute_batch(GENERATION_23_SQL)?;
    }

    if table_exists(conn, "review_agent_invocations")?
        && conn.query_row(
            "select exists(select 1 from pragma_table_info('review_agent_invocations') where name='legacy_source_reviewer_digest')",
            [],
            |row| row.get::<_, bool>(0),
        )?
    {
        let source = if table_exists(conn, "authority_migration_sources")? {
            conn.query_row(
                "select source_ledger_digest,source_generation from authority_migration_sources order by id limit 1",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?
        } else {
            None
        };
        let (source_ledger_digest, source_generation) = source
            .map(|(digest, generation)| (Some(digest), Some(generation)))
            .unwrap_or((None, None));
        let digests = conn
            .prepare(
                "select distinct legacy_source_reviewer_digest from review_agent_invocations where legacy_source_reviewer_digest is not null and length(legacy_source_reviewer_digest)=64 order by legacy_source_reviewer_digest",
            )?
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let projects = conn
            .prepare("select id from projects order by id")?
            .query_map([], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for project in projects {
            for digest in &digests {
                conn.execute(
                    "insert or ignore into reviewer_migration_sources(project_id,source_reviewer_ref,source_reviewer_digest,source_ledger_digest,source_generation,status,created_at) values(?1,?2,?3,?4,?5,'pending',current_timestamp)",
                    params![
                        project,
                        format!("legacy-reviewer:{digest}"),
                        digest,
                        source_ledger_digest,
                        source_generation,
                    ],
                )?;
            }
        }
    }

    conn.execute(
        "insert or ignore into schema_migrations(version,applied_at) values(23,current_timestamp)",
        [],
    )?;
    Ok(())
}

pub(crate) fn install_storage_generation_24(conn: &Connection) -> Result<()> {
    conn.execute_batch(GENERATION_24_SQL)?;
    conn.execute(
        "insert or ignore into schema_migrations(version,applied_at) values(24,current_timestamp)",
        [],
    )?;
    Ok(())
}

pub(crate) fn install_storage_generation_25(conn: &Connection) -> Result<()> {
    prepare_adjudication_for_schema(conn)?;
    normalize_review_invocation_storage(conn)?;
    execute_schema_batches(conn)?;
    normalize_legacy_review_acceptances(conn)?;
    if table_has_column(conn, "owner_decisions", "capability_id")?
        || table_has_column(conn, "owner_decisions", "principal_id")?
        || table_has_column(conn, "review_agent_invocations", "reviewer_principal_id")?
        || table_has_column(conn, "review_agent_invocations", "review_provenance_id")?
        || table_has_column(
            conn,
            "review_agent_invocations",
            "legacy_source_reviewer_digest",
        )?
    {
        bail!("storage generation 25 target retains retired owner authority columns");
    }
    conn.execute(
        "insert or ignore into schema_migrations(version,applied_at) values(25,current_timestamp)",
        [],
    )?;
    Ok(())
}

fn normalize_review_invocation_storage(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "review_agent_invocations")? {
        return Ok(());
    }
    let mut has_retired_authority = false;
    for column in [
        "reviewer_principal_id",
        "review_provenance_id",
        "legacy_source_reviewer_digest",
    ] {
        has_retired_authority |= table_has_column(conn, "review_agent_invocations", column)?;
    }
    if !has_retired_authority {
        return Ok(());
    }
    conn.execute_batch(
        r#"
        create table review_agent_invocations_without_retired_authority (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            review_plan_id integer references review_plans(id),
            review_run_id integer references review_runs(id),
            invocation_handle text,
            provenance_handle text,
            target_context text,
            purpose text,
            request_idempotency_key text,
            request_payload_digest text,
            transition_idempotency_key text,
            claim text,
            verification_claim text,
            closure_attempt_id integer references closure_attempts(id),
            result_summary text,
            terminal_reason text,
            run_type text not null check (run_type in ('fresh', 'resume', 'coverage')),
            agent_label text,
            external_agent_id text,
            status text not null default 'requested' check (status in ('requested', 'running', 'completed', 'failed', 'cancelled')),
            started_at text,
            finished_at text
        );
        insert into review_agent_invocations_without_retired_authority(
            id,project_id,review_plan_id,review_run_id,invocation_handle,provenance_handle,
            target_context,purpose,request_idempotency_key,request_payload_digest,
            transition_idempotency_key,claim,verification_claim,closure_attempt_id,
            result_summary,terminal_reason,run_type,agent_label,external_agent_id,status,
            started_at,finished_at
        )
        select id,project_id,review_plan_id,review_run_id,invocation_handle,provenance_handle,
               target_context,purpose,request_idempotency_key,request_payload_digest,
               transition_idempotency_key,claim,verification_claim,closure_attempt_id,
               result_summary,terminal_reason,run_type,agent_label,external_agent_id,status,
               started_at,finished_at
        from review_agent_invocations;
        drop table review_agent_invocations;
        alter table review_agent_invocations_without_retired_authority rename to review_agent_invocations;
        "#,
    )?;
    Ok(())
}

fn backfill_decomposition_plan_ingress_identities(conn: &Connection) -> Result<()> {
    let mut statement = conn.prepare(
        r#"
        select plan.id,plan.project_id,plan.document_content,plan.content_identity
        from decomposition_plans plan
        left join decomposition_plan_ingress_identities ingress on ingress.plan_id=plan.id
        where ingress.plan_id is null and plan.document_content is not null
          and plan.document_content!='' and length(plan.content_identity)=64
        order by plan.id
        "#,
    )?;
    let plans = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (plan_id, project_id, content, content_identity) in plans {
        let mut hasher = Sha256::new();
        hasher.update(b"agent-workbench/decomposition-plan-source/v1\0");
        hasher.update(content.as_bytes());
        conn.execute(
            "insert into decomposition_plan_ingress_identities(plan_id,project_id,source_identity,content_identity,created_at) values(?1,?2,?3,?4,current_timestamp)",
            params![plan_id, project_id, format!("{:x}", hasher.finalize()), content_identity],
        )?;
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PendingCorrectionTargetChange {
    pub(crate) token_id: i64,
    pub(crate) before: String,
    pub(crate) after: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PendingCorrectionTargetInspection {
    pub(crate) changes: Vec<PendingCorrectionTargetChange>,
    pub(crate) blockers: Vec<String>,
}

pub(crate) fn inspect_pending_reconciliation_target_migrations(
    conn: &Connection,
) -> Result<PendingCorrectionTargetInspection> {
    let mut statement = conn.prepare(
        r#"
        select token.id,token.project_id,token.closure_id,token.target
        from correction_tokens token
        join closures closure on closure.id=token.closure_id
        where token.token_kind='transition'
          and token.operation='decomposition-plan-reconcile'
          and token.status='pending' and closure.status='registered'
        order by token.id
        "#,
    )?;
    let tokens = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut inspection = PendingCorrectionTargetInspection::default();
    for (token_id, project_id, closure_id, target) in tokens {
        let Some((design_version_id, work_unit_id)) = legacy_reconciliation_owner(&target) else {
            continue;
        };
        let owner = conn
            .query_row(
                r#"
                select package.root_path,project.root_path
                from design_versions version
                join design_packages package on package.id=version.design_package_id
                join projects project on project.id=version.project_id
                join work_units work on work.id=?3 and work.project_id=project.id
                where version.id=?1 and version.project_id=?2
                "#,
                params![design_version_id, project_id, work_unit_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let Some((package_root, project_root)) = owner else {
            inspection.blockers.push(format!(
                "token {token_id} closure {closure_id} has no selected design/work owner"
            ));
            continue;
        };
        let Some(package_relative) = package_project_relative(&project_root, &package_root) else {
            inspection.blockers.push(format!(
                "token {token_id} closure {closure_id} Design Package is outside its project"
            ));
            continue;
        };
        let mut surfaces = conn.prepare(
            r#"
            select target
            from correction_tokens
            where closure_id=?1 and token_kind='file' and operation in ('edit','create')
              and target like 'design:%'
            order by token_ordinal,id
            "#,
        )?;
        let mut candidates = surfaces
            .query_map([closure_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .into_iter()
            .filter_map(|surface| {
                let relative = checked_relative_path(surface.strip_prefix("design:")?)?;
                if relative.extension().and_then(|value| value.to_str()) != Some("md") {
                    return None;
                }
                checked_relative_path(package_relative.join(relative))
            })
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        candidates.sort();
        candidates.dedup();
        let mut plan_matches = Vec::new();
        for candidate in &candidates {
            let matches: bool = conn.query_row(
                r#"
                select exists(
                  select 1 from decomposition_plans
                  where project_id=?1 and design_version_id=?2 and work_unit_id=?3
                    and source_path=?4 and status in ('draft','ready','applied')
                )
                "#,
                params![project_id, design_version_id, work_unit_id, candidate],
                |row| row.get(0),
            )?;
            if matches {
                plan_matches.push(candidate.clone());
            }
        }
        let selected = match (plan_matches.as_slice(), candidates.as_slice()) {
            ([selected], _) => Some(selected.clone()),
            ([], [selected]) => Some(selected.clone()),
            ([], []) => {
                inspection.blockers.push(format!(
                    "token {token_id} closure {closure_id} has no contained same-closure design edit/create Plan path"
                ));
                None
            }
            _ => {
                inspection.blockers.push(format!(
                    "token {token_id} closure {closure_id} has ambiguous contained same-closure design edit/create Plan paths"
                ));
                None
            }
        };
        if let Some(selected) = selected {
            inspection.changes.push(PendingCorrectionTargetChange {
                token_id,
                before: target,
                after: format!(
                    "{design_version_id}/{work_unit_id}/{}",
                    crate::review::encode_opaque_component(&selected)
                ),
            });
        }
    }
    Ok(inspection)
}

fn legacy_reconciliation_owner(target: &str) -> Option<(i64, i64)> {
    let mut components = target.split('/');
    let design = components.next()?;
    let work = components.next()?;
    if components.next().is_some() {
        return None;
    }
    let design_id = design.parse::<i64>().ok()?;
    let work_id = work.parse::<i64>().ok()?;
    (design_id > 0 && work_id > 0 && design_id.to_string() == design && work_id.to_string() == work)
        .then_some((design_id, work_id))
}

fn package_project_relative(project_root: &str, package_root: &str) -> Option<PathBuf> {
    let package = Path::new(package_root);
    let relative = if package.is_absolute() {
        package.strip_prefix(Path::new(project_root)).ok()?
    } else {
        package
    };
    checked_relative_path(relative)
}

fn checked_relative_path(path: impl AsRef<Path>) -> Option<PathBuf> {
    let path = path.as_ref();
    if path.as_os_str().is_empty() || path.is_absolute() {
        return None;
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => normalized.push(value),
            _ => return None,
        }
    }
    (!normalized.as_os_str().is_empty()).then_some(normalized)
}

fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    let exists: bool = conn.query_row(
        "select exists(select 1 from pragma_table_info(?1) where name=?2)",
        params![table, column],
        |row| row.get(0),
    )?;
    if !exists {
        conn.execute_batch(&format!(
            "alter table \"{}\" add column \"{}\" {definition}",
            table.replace('"', "\"\""),
            column.replace('"', "\"\"")
        ))?;
    }
    Ok(())
}

pub(crate) fn open_ledger(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)
        .with_context(|| format!("failed to open ledger {}", path.display()))?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(conn)
}

pub(crate) struct ProjectConnection {
    connection: Connection,
    _update_guard: File,
}

impl Deref for ProjectConnection {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        &self.connection
    }
}

impl DerefMut for ProjectConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.connection
    }
}

pub(crate) fn open_existing_project(root: &Path) -> Result<ProjectConnection> {
    let ledger_path = default_ledger_path(root);
    if !ledger_path.exists() {
        bail!("project is not initialized; run agent-workbench init");
    }
    let update_guard = crate::update::shared_writer_guard(root)?;
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
    Ok(ProjectConnection {
        connection: conn,
        _update_guard: update_guard,
    })
}

pub(crate) fn apply_pending_update(conn: &Connection) -> Result<()> {
    if !conn.is_autocommit() {
        return apply_pending_update_steps(conn);
    }
    let foreign_keys_enabled: i64 =
        conn.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
    conn.pragma_update(None, "foreign_keys", false)?;
    let migration_result = (|| -> Result<()> {
        let tx = conn.unchecked_transaction()?;
        apply_pending_update_steps(&tx)?;
        let violations: i64 =
            tx.query_row("select count(*) from pragma_foreign_key_check", [], |row| {
                row.get(0)
            })?;
        if violations != 0 {
            bail!("update produced {violations} foreign key violation(s)");
        }
        tx.commit().context("update failed while committing")?;
        Ok(())
    })();
    let restore_result = conn.pragma_update(None, "foreign_keys", foreign_keys_enabled != 0);
    migration_result?;
    restore_result?;
    Ok(())
}

pub(crate) fn install_core_from_historical(
    conn: &Connection,
    expected_generation: i64,
    source_ledger_identity: &str,
) -> Result<()> {
    if !(1..CORE_SCHEMA_VERSION).contains(&expected_generation) {
        bail!("historical source generation is outside the core transition boundary");
    }
    if source_ledger_identity.len() != 64
        || !source_ledger_identity
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        bail!("historical source identity is not canonical");
    }
    let actual_generation = conn.query_row(
        "select coalesce(max(version),0) from schema_migrations",
        [],
        |row| row.get::<_, i64>(0),
    )?;
    if actual_generation != expected_generation {
        bail!("historical source changed before its declared transition");
    }
    apply_pending_update_steps(conn)?;
    let project = project_id(conn)?;
    conn.execute(
        "insert or ignore into schema_retirement_records(project_id,source_ledger_digest,source_generation,completed_at) values(?1,?2,?3,current_timestamp)",
        params![project, source_ledger_identity, expected_generation],
    )?;
    let recorded: String = conn.query_row(
        "select source_ledger_digest from schema_retirement_records where project_id=?1 and source_generation=?2",
        params![project, expected_generation],
        |row| row.get(0),
    )?;
    if recorded != source_ledger_identity {
        bail!("historical source retirement conflicts with an existing identity");
    }
    Ok(())
}

fn apply_pending_update_steps(conn: &Connection) -> Result<()> {
    let source_generation = conn
        .query_row(
            "select coalesce(max(version),0) from schema_migrations",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    if (6..=12).contains(&source_generation) {
        let project = project_id(conn)?;
        let digest =
            super::adjudication_migration::legacy_source_digest(conn, project, source_generation)?;
        if source_generation == 12 {
            validate_schema12_owner_decisions(conn, project)?;
        }
        migrate(conn).with_context(|| {
            format!(
                "schema-{source_generation} migration failed while installing the current schema"
            )
        })?;
        if source_generation <= 11 {
            conn.execute("insert or ignore into authority_migration_sources(project_id,source_ledger_digest,source_generation,created_at) values(?1,?2,?3,current_timestamp)",params![project,digest,source_generation]).with_context(|| format!("schema-{source_generation} migration failed while recording the source ledger"))?;
            migrate_legacy_review_candidates(conn).with_context(|| {
                format!(
                    "schema-{source_generation} migration failed while normalizing legacy review state"
                )
            })?;
            conn.execute("insert into legacy_adjudication_migrations(project_id,source_ledger_digest,source_generation,completed_at) select project_id,source_ledger_digest,source_generation,current_timestamp from authority_migration_sources where project_id=?1 and source_generation=?2",params![project,source_generation]).with_context(|| format!("schema-{source_generation} migration failed while recording completion"))?;
        }
        return Ok(());
    }
    if ledger_needs_migration(conn)? {
        migrate(conn).with_context(|| {
            format!("migration from schema generation {source_generation} failed while installing the current schema")
        })?;
    }
    let pending:i64=conn.query_row("select exists(select 1 from authority_migration_sources s where not exists(select 1 from legacy_adjudication_migrations m where m.project_id=s.project_id and m.source_generation=s.source_generation and m.source_ledger_digest=s.source_ledger_digest))",[],|row|row.get(0)).unwrap_or(0);
    if pending == 1 {
        migrate_legacy_review_candidates(conn)
            .context("pending schema-11 migration failed while normalizing legacy review state")?;
        let project = project_id(conn)?;
        conn.execute("insert into legacy_adjudication_migrations(project_id,source_ledger_digest,source_generation,completed_at) select s.project_id,s.source_ledger_digest,s.source_generation,current_timestamp from authority_migration_sources s where s.project_id=?1 and not exists(select 1 from legacy_adjudication_migrations m where m.project_id=s.project_id and m.source_generation=s.source_generation)",params![project]).context("pending schema-11 migration failed while recording completion")?;
    }
    normalize_decomposition_plan_heads(conn)
        .context("pending update failed while normalizing Decomposition Plan heads")?;
    Ok(())
}

pub(crate) fn project_requires_update(conn: &Connection) -> Result<bool> {
    crate::update::connection_requires_update(conn)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PendingUpdateChange {
    SchemaGeneration { source: i64, target: i64 },
    MissingTable(&'static str),
    StructuralProfile(StructuralProfile),
    SchemaOrProfile,
    ReviewPlanSupersessionProjection,
    LegacyReviewNormalization,
    FindingEpochNormalization,
    DecompositionPlanHeadNormalization,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StructuralProfile {
    CurrentTasksView,
    CurrentTaskValidationGatesView,
    CompletionInheritanceView,
    CorrectionTransition,
    CorrectionStatus,
    FindingVerificationPlanReplacement,
    Trigger(&'static str),
    CompletionIdentity,
    AcceptanceCoverageColumn,
    AcceptanceReferenceRepair,
    AcceptanceRecord,
    ReviewProvenanceColumn,
    ClosureSupersessionColumn,
    KptLifecycle,
    PhaseEpochs,
    FindingTypeDomain,
}

impl PendingUpdateChange {
    pub(crate) fn identity(&self) -> String {
        match self {
            Self::SchemaGeneration { source, target } => {
                format!("schema-generation\0{source}\0{target}")
            }
            Self::MissingTable(table) => format!("missing-table\0{table}"),
            Self::StructuralProfile(profile) => profile.identity(),
            Self::SchemaOrProfile => "schema-or-profile".to_string(),
            Self::ReviewPlanSupersessionProjection => {
                "data-normalization\0review-plan-supersession".to_string()
            }
            Self::LegacyReviewNormalization => "data-normalization\0legacy-review".to_string(),
            Self::FindingEpochNormalization => "data-normalization\0finding-epoch".to_string(),
            Self::DecompositionPlanHeadNormalization => {
                "data-normalization\0decomposition-plan-head".to_string()
            }
        }
    }

    fn public_label(&self) -> String {
        match self {
            Self::SchemaGeneration { source, target } => format!("schema_{source}_to_{target}"),
            Self::MissingTable(table) => format!("missing_table:{table}"),
            Self::StructuralProfile(profile) => profile.public_label(),
            Self::SchemaOrProfile => "schema_or_profile_update".to_string(),
            Self::ReviewPlanSupersessionProjection => {
                "review_plan_supersession_projection".to_string()
            }
            Self::LegacyReviewNormalization => "legacy_review_normalization".to_string(),
            Self::FindingEpochNormalization => "finding_epoch_normalization".to_string(),
            Self::DecompositionPlanHeadNormalization => {
                "decomposition_plan_head_normalization".to_string()
            }
        }
    }
}

impl StructuralProfile {
    fn identity(self) -> String {
        match self {
            Self::Trigger(trigger) => format!("structural-profile\0trigger\0{trigger}"),
            Self::CurrentTasksView => "structural-profile\0current-tasks".to_string(),
            Self::CurrentTaskValidationGatesView => {
                "structural-profile\0current-task-validation-gates".to_string()
            }
            Self::CompletionInheritanceView => {
                "structural-profile\0completion-inheritance".to_string()
            }
            Self::CorrectionTransition => "structural-profile\0correction-transition".to_string(),
            Self::CorrectionStatus => "structural-profile\0correction-status".to_string(),
            Self::FindingVerificationPlanReplacement => {
                "structural-profile\0finding-verification-plan-replacement".to_string()
            }
            Self::CompletionIdentity => "structural-profile\0completion-identity".to_string(),
            Self::AcceptanceCoverageColumn => "structural-profile\0acceptance-coverage".to_string(),
            Self::AcceptanceReferenceRepair => {
                "structural-profile\0acceptance-reference".to_string()
            }
            Self::AcceptanceRecord => "structural-profile\0acceptance-record".to_string(),
            Self::ReviewProvenanceColumn => "structural-profile\0review-provenance".to_string(),
            Self::ClosureSupersessionColumn => {
                "structural-profile\0closure-supersession".to_string()
            }
            Self::KptLifecycle => "structural-profile\0kpt-lifecycle".to_string(),
            Self::PhaseEpochs => "structural-profile\0phase-epochs".to_string(),
            Self::FindingTypeDomain => "structural-profile\0finding-type-domain".to_string(),
        }
    }

    fn public_label(self) -> String {
        match self {
            Self::CurrentTasksView => "current_tasks_view".to_string(),
            Self::CurrentTaskValidationGatesView => {
                "current_task_validation_gates_view".to_string()
            }
            Self::CompletionInheritanceView => "completion_inheritance_view".to_string(),
            Self::CorrectionTransition => "correction_transition_profile".to_string(),
            Self::CorrectionStatus => "correction_status_profile".to_string(),
            Self::FindingVerificationPlanReplacement => {
                "finding_verification_plan_replacement_profile".to_string()
            }
            Self::Trigger(trigger) => format!("trigger_profile:{trigger}"),
            Self::CompletionIdentity => "completion_identity_profile".to_string(),
            Self::AcceptanceCoverageColumn => "acceptance_coverage_column".to_string(),
            Self::AcceptanceReferenceRepair => "acceptance_reference_repair".to_string(),
            Self::AcceptanceRecord => "acceptance_record_profile".to_string(),
            Self::ReviewProvenanceColumn => "review_provenance_column".to_string(),
            Self::ClosureSupersessionColumn => "closure_supersession_column".to_string(),
            Self::KptLifecycle => "kpt_lifecycle_profile".to_string(),
            Self::PhaseEpochs => "phase_epoch_profile".to_string(),
            Self::FindingTypeDomain => "finding_type_domain".to_string(),
        }
    }
}

pub(crate) fn pending_update_changes(conn: &Connection) -> Result<Vec<String>> {
    Ok(pending_update_change_set(conn)?
        .iter()
        .map(PendingUpdateChange::public_label)
        .collect())
}

pub(crate) fn pending_update_change_set(conn: &Connection) -> Result<Vec<PendingUpdateChange>> {
    let mut changes = schema_profile_update_changes(conn)?;
    if ledger_needs_migration(conn)? && changes.is_empty() {
        changes.push(PendingUpdateChange::SchemaOrProfile);
    }
    if table_exists(conn, "review_plan_supersessions")? {
        changes.push(PendingUpdateChange::ReviewPlanSupersessionProjection);
    }
    if decomposition_plan_heads_require_normalization(conn)? {
        changes.push(PendingUpdateChange::DecompositionPlanHeadNormalization);
    }
    if !table_exists(conn, "authority_migration_sources")?
        || !table_exists(conn, "legacy_adjudication_migrations")?
    {
        return Ok(changes);
    }
    let pending: bool = conn.query_row(
        r#"
        select exists(
          select 1 from authority_migration_sources source
          where not exists(
            select 1 from legacy_adjudication_migrations applied
            where applied.project_id=source.project_id
              and applied.source_generation=source.source_generation
              and applied.source_ledger_digest=source.source_ledger_digest
          )
        )
        "#,
        [],
        |row| row.get(0),
    )?;
    if pending {
        changes.push(PendingUpdateChange::LegacyReviewNormalization);
    }
    if !table_exists(conn, "finding_decision_epochs")?
        || !table_exists(conn, "verification_adjudication_decisions")?
        || !table_exists(conn, "finding_disposition_decisions")?
        || !table_exists(conn, "closure_attempts")?
        || !table_has_column(conn, "findings", "lifecycle_state")?
        || !table_has_column(conn, "finding_verifications", "closure_attempt_id")?
    {
        return Ok(changes);
    }
    let missing_terminal_epochs: bool = conn.query_row(
        r#"
        select exists(
          select 1 from findings f
          where f.lifecycle_state='closed'
            and not exists(
              select 1 from finding_decision_epochs epoch
              where epoch.project_id=f.project_id and epoch.finding_id=f.id
            )
            and (
              exists(
                select 1 from verification_adjudication_decisions decision
                join closure_attempts attempt on attempt.id=decision.closure_attempt_id
                join closures closure on closure.id=attempt.closure_id
                join finding_verifications verification
                  on verification.closure_attempt_id=attempt.id
                 and verification.result='verified'
                where closure.finding_id=f.id and decision.value='accepted'
                  and not exists(
                    select 1 from verification_adjudication_decisions successor
                    where successor.predecessor_id=decision.id
                  )
              )
              or exists(
                select 1 from finding_disposition_decisions decision
                where decision.finding_id=f.id
                  and decision.value in ('rejected','authority_disposed')
                  and not exists(
                    select 1 from finding_disposition_decisions successor
                    where successor.predecessor_id=decision.id
                  )
              )
            )
        )
        "#,
        [],
        |row| row.get(0),
    )?;
    if missing_terminal_epochs {
        changes.push(PendingUpdateChange::FindingEpochNormalization);
    }
    Ok(changes)
}

fn schema_profile_update_changes(conn: &Connection) -> Result<Vec<PendingUpdateChange>> {
    let mut reasons = Vec::new();
    let version = conn
        .query_row(
            "select version from schema_migrations order by version desc limit 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .unwrap_or(0);
    if version < SCHEMA_VERSION {
        reasons.push(PendingUpdateChange::SchemaGeneration {
            source: version,
            target: SCHEMA_VERSION,
        });
    }
    for table in [
        "closure_attempts",
        "review_adjudication_decisions",
        "finding_disposition_decisions",
        "verification_adjudication_decisions",
        "finding_lifecycle_events",
        "review_correction_events",
        "review_boundary_snapshots",
        "review_correction_recovery_obligations",
        "finding_decision_epochs",
        "review_provenance_claims",
        "review_invocation_events",
        "review_result_drafts",
        "review_result_draft_items",
        "review_result_draft_events",
        "legacy_claim_audits",
        "legacy_review_acceptance_migrations",
        "legacy_finding_audits",
        "legacy_migration_candidates",
        "legacy_migration_candidate_members",
        "legacy_migration_edges",
        "legacy_migration_projections",
        "authority_migration_sources",
        "legacy_adjudication_migrations",
        "owner_decisions",
        "finding_remediation_bindings",
        "finding_remediation_recovery_epochs",
        "correction_sessions",
        "correction_tokens",
        "correction_transition_aliases",
        "correction_application_identity_links",
        "correction_completion_inheritance_sources",
        "correction_completion_inheritance_evidence",
        "update_operations",
        "update_decisions",
        "update_receipts",
        "release_candidates",
        "release_candidate_assets",
        "release_candidate_events",
        "release_candidate_attempts",
        "release_candidate_revisions",
        "release_candidate_subject_revisions",
        "release_candidate_boundaries",
        "decomposition_plans",
        "decomposition_plan_ingress_identities",
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
        "decomposition_reconciliation_results",
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
            reasons.push(PendingUpdateChange::MissingTable(table));
        }
    }
    if !finding_type_domain_current(conn)? {
        reasons.push(PendingUpdateChange::StructuralProfile(
            StructuralProfile::FindingTypeDomain,
        ));
    }
    if table_exists(conn, "phase_epochs")?
        && table_exists(conn, "phase_epoch_sources")?
        && table_exists(conn, "phase_epoch_dependencies")?
        && table_exists(conn, "phase_epoch_memberships")?
        && table_exists(conn, "phase_epoch_membership_sources")?
        && table_exists(conn, "phase_epoch_dependency_sources")?
        && table_exists(conn, "phase_scope_dispositions")?
        && table_exists(conn, "phase_scope_disposition_sources")?
    {
        let phase_epoch_drift: bool = conn.query_row(
            r#"
            select
              exists(
                select 1 from work_phases source
                left join phase_epoch_sources mapping on mapping.source_phase_id=source.id
                left join phase_epochs epoch on epoch.id=mapping.phase_epoch_id
                where mapping.id is null or epoch.id is null
                  or epoch.project_id!=source.project_id
                  or epoch.work_unit_id!=source.work_unit_id
                  or (((epoch.phase_key!=source.phase_key
                        and not (source.phase_key like 'successor-plan-%-slice-%'
                                 and epoch.predecessor_epoch_id is not null))
                       or epoch.state!=case source.status
                            when 'accepted_out_of_scope' then 'superseded'
                            else source.status end)
                      and epoch.state!='superseded')
              )
              or exists(
                select 1 from work_phase_dependencies source
                left join phase_epoch_dependency_sources mapping
                  on mapping.source_dependency_id=source.id
                left join phase_epoch_dependencies dependency
                  on dependency.id=mapping.phase_epoch_dependency_id
                where mapping.id is null or dependency.id is null
                  or dependency.from_phase_epoch_id!=source.from_phase_id
                  or dependency.to_phase_epoch_id!=source.to_phase_id
                  or (dependency.state!=source.status
                      and dependency.state!='invalidated')
              )
              or exists(
                select 1 from task_phase_memberships source
                left join task_phase_membership_sources legacy
                  on legacy.task_phase_membership_id=source.id
                left join phase_epoch_membership_sources mapping
                  on mapping.source_membership_id=legacy.source_membership_id
                left join phase_epoch_memberships membership
                  on membership.id=mapping.phase_epoch_membership_id
                where legacy.id is null or mapping.id is null or membership.id is null
                  or membership.phase_epoch_id!=source.phase_id
                  or membership.task_identity_id!=source.task_identity_id
                  or (membership.state!=case source.state
                        when 'open' then 'current'
                        when 'blocked' then 'current'
                        when 'closed' then 'closed'
                        when 'out_of_scope' then 'out_of_scope'
                        when 'split' then 'split' end
                      and membership.state!='superseded')
              )
              or exists(
                select 1 from work_phases source
                where source.status='accepted_out_of_scope' and not exists(
                  select 1 from phase_scope_dispositions disposition
                  join phase_scope_disposition_sources mapping
                    on mapping.phase_scope_disposition_id=disposition.id
                  where mapping.source_phase_id=source.id
                    and disposition.state='accepted_out_of_scope'
                )
              )
            "#,
            [],
            |row| row.get(0),
        )?;
        if phase_epoch_drift {
            reasons.push(PendingUpdateChange::StructuralProfile(
                StructuralProfile::PhaseEpochs,
            ));
        }
    }
    let kpt_lifecycle_current: bool = conn.query_row(
        r#"
        select
          exists(select 1 from sqlite_schema where type='table' and name='kpt_items'
                 and sql like '%''converted''%'
                 and sql like '%linked_review_finding_id integer references findings%')
          and exists(select 1 from sqlite_schema where type='table' and name='kpt_item_conversions'
                     and sql like '%review_policy_id integer references review_policies%'
                     and sql like '%design_version_id integer references design_versions%'
                     and sql like '%kpt_rule_id%'
                     and sql like '%''correction''%')
          and exists(select 1 from pragma_table_info('kpt_item_conversions') where name='item_revision')
          and exists(select 1 from pragma_table_info('kpt_item_conversions') where name='predecessor_handle')
          and exists(select 1 from pragma_table_info('kpt_item_conversions') where name='request_identity')
          and exists(select 1 from pragma_table_info('kpt_item_conversions') where name='receipt_identity')
          and exists(select 1 from pragma_table_info('kpt_item_conversions') where name='current_handle')
          and exists(select 1 from sqlite_schema where type='table' and name='kpt_item_sources')
          and exists(select 1 from sqlite_schema where type='table' and name='kpt_rules')
          and exists(select 1 from sqlite_schema where type='table' and name='kpt_item_dismissals')
          and exists(select 1 from sqlite_schema where type='trigger' and name='trg_kpt_rule_restricted_update')
          and exists(select 1 from sqlite_schema where type='trigger' and name='trg_kpt_item_conversion_project_insert')
          and exists(select 1 from sqlite_schema where type='trigger' and name='trg_kpt_item_conversion_immutable_delete')
          and exists(select 1 from sqlite_schema where type='trigger' and name='trg_kpt_rule_project_insert')
          and exists(select 1 from sqlite_schema where type='trigger' and name='trg_kpt_dismissal_links_insert')
          and exists(select 1 from sqlite_schema where type='trigger' and name='trg_kpt_item_conversion_receipt_insert')
        "#,
        [],
        |row| row.get(0),
    )?;
    if !kpt_lifecycle_current {
        reasons.push(PendingUpdateChange::StructuralProfile(
            StructuralProfile::KptLifecycle,
        ));
    }
    let current_tasks: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='view' and name='current_tasks' and sql like '%revision.status=''current''%')",
        [],
        |row| row.get(0),
    )?;
    if !current_tasks {
        reasons.push(PendingUpdateChange::StructuralProfile(
            StructuralProfile::CurrentTasksView,
        ));
    }
    let current_task_gates: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='view' and name='current_task_validation_gates' and sql like '%current_tasks%')",
        [],
        |row| row.get(0),
    )?;
    if !current_task_gates {
        reasons.push(PendingUpdateChange::StructuralProfile(
            StructuralProfile::CurrentTaskValidationGatesView,
        ));
    }
    let inheritance_current: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='view' and name='valid_completion_inheritance_sources' and sql like '%mapping.design_requirement_id=source.current_requirement_id%' and sql like '%candidate_version.version_number%' and sql like '%left join command_usages usage%')",
        [],
        |row| row.get(0),
    )?;
    if !inheritance_current {
        reasons.push(PendingUpdateChange::StructuralProfile(
            StructuralProfile::CompletionInheritanceView,
        ));
    }
    let correction_current: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='trigger' and name='trg_correction_token_links_insert' and sql like '%phase_dependency_max%' and sql like '%decomposition-plan-reconcile%') and exists(select 1 from sqlite_schema where type='trigger' and name='trg_correction_application_links_insert' and sql like '%decomposition-plan:%') and exists(select 1 from sqlite_schema where type='trigger' and name='trg_completion_source_insert' and sql like '%candidate_version.version_number%')",
        [],
        |row| row.get(0),
    )?;
    if !correction_current {
        reasons.push(PendingUpdateChange::StructuralProfile(
            StructuralProfile::CorrectionTransition,
        ));
    }
    let correction_status_current: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='trigger' and name='trg_correction_session_status_update') and exists(select 1 from sqlite_schema where type='trigger' and name='trg_correction_token_status_update')",
        [],
        |row| row.get(0),
    )?;
    if !correction_status_current {
        reasons.push(PendingUpdateChange::StructuralProfile(
            StructuralProfile::CorrectionStatus,
        ));
    }
    let verification_plan_replacement_current: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='trigger' and name='trg_finding_verification_project_insert' and sql like '%verifier_plan.work_unit_id%') and exists(select 1 from sqlite_schema where type='trigger' and name='trg_finding_verification_project_update' and sql like '%verifier_plan.work_unit_id%')",
        [],
        |row| row.get(0),
    )?;
    if !verification_plan_replacement_current {
        reasons.push(PendingUpdateChange::StructuralProfile(
            StructuralProfile::FindingVerificationPlanReplacement,
        ));
    }
    for (trigger, marker) in [
        (
            "trg_correction_application_links_insert",
            "work_phase_task_memberships",
        ),
        ("trg_correction_alias_links_insert", "@superseded-task/"),
        ("trg_correction_identity_link_insert", "completion_source"),
    ] {
        let current: bool = conn.query_row(
            "select exists(select 1 from sqlite_schema where type='trigger' and name=?1 and sql like '%'||?2||'%')",
            params![trigger, marker],
            |row| row.get(0),
        )?;
        if !current {
            reasons.push(PendingUpdateChange::StructuralProfile(
                StructuralProfile::Trigger(trigger),
            ));
        }
    }
    let completion_identity_kind: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='table' and name='correction_application_identity_links' and sql like '%completion_source%')",
        [],
        |row| row.get(0),
    )?;
    if !completion_identity_kind {
        reasons.push(PendingUpdateChange::StructuralProfile(
            StructuralProfile::CompletionIdentity,
        ));
    }
    if table_exists(conn, "acceptance_records")?
        && !table_has_column(conn, "acceptance_records", "coverage_item_id")?
    {
        reasons.push(PendingUpdateChange::StructuralProfile(
            StructuralProfile::AcceptanceCoverageColumn,
        ));
    }
    let broken_acceptance_refs: i64 = conn.query_row(
        "select count(*) from sqlite_schema where sql like '%acceptance_records_old%'",
        [],
        |row| row.get(0),
    )?;
    if broken_acceptance_refs > 0 {
        reasons.push(PendingUpdateChange::StructuralProfile(
            StructuralProfile::AcceptanceReferenceRepair,
        ));
    }
    if table_exists(conn, "acceptance_records")? && acceptance_records_needs_migration(conn)? {
        reasons.push(PendingUpdateChange::StructuralProfile(
            StructuralProfile::AcceptanceRecord,
        ));
    }
    if table_exists(conn, "review_runs")?
        && !table_has_column(conn, "review_runs", "review_provenance")?
    {
        reasons.push(PendingUpdateChange::StructuralProfile(
            StructuralProfile::ReviewProvenanceColumn,
        ));
    }
    if table_exists(conn, "closures")?
        && !table_has_column(conn, "closures", "supersession_reason")?
    {
        reasons.push(PendingUpdateChange::StructuralProfile(
            StructuralProfile::ClosureSupersessionColumn,
        ));
    }
    Ok(reasons)
}

fn validate_schema12_owner_decisions(conn: &Connection, project: i64) -> Result<()> {
    for (table, target_column, target_table) in [
        (
            "review_adjudication_decisions",
            "review_run_id",
            "review_runs",
        ),
        ("finding_disposition_decisions", "finding_id", "findings"),
        (
            "verification_adjudication_decisions",
            "closure_attempt_id",
            "closure_attempts",
        ),
    ] {
        let missing_target: Option<String> = conn
            .query_row(
                &format!(
                    "select d.id||':'||d.{target_column} from {table} d left join {target_table} t on t.id=d.{target_column} where d.project_id=?1 and (t.id is null or t.project_id!=d.project_id) order by d.id limit 1"
                ),
                params![project],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(rows) = missing_target {
            bail!("schema-12 decision contradiction: {table} missing target rows {rows}");
        }
        let contradiction: Option<String> = conn
            .query_row(
                &format!(
                    "select cast({target_column} as text)||':'||group_concat(id,',')
                     from {table} d
                     where project_id=?1 and not exists(select 1 from {table} n where n.predecessor_id=d.id)
                     group by {target_column} having count(*)>1 order by {target_column} limit 1"
                ),
                params![project],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(rows) = contradiction {
            bail!("schema-12 decision contradiction: {table} current heads {rows}");
        }
        let broken_link: Option<String> = conn
            .query_row(
                &format!(
                    "select d.id||':'||d.predecessor_id
                     from {table} d left join {table} p on p.id=d.predecessor_id
                     where d.project_id=?1 and d.predecessor_id is not null
                       and (p.id is null or p.project_id!=d.project_id or p.{target_column}!=d.{target_column})
                     order by d.id limit 1"
                ),
                params![project],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(rows) = broken_link {
            bail!("schema-12 decision contradiction: {table} predecessor rows {rows}");
        }
        let row_count: i64 = conn.query_row(
            &format!("select count(*) from {table} where project_id=?1"),
            params![project],
            |row| row.get(0),
        )?;
        let cycle: Option<i64> = conn
            .query_row(
                &format!(
                    "with recursive chain(start,current,next,depth) as (
                         select id,id,predecessor_id,0 from {table} where project_id=?1
                         union all
                         select chain.start,p.id,p.predecessor_id,chain.depth+1
                         from chain join {table} p on p.id=chain.next
                         where chain.next is not null and chain.depth<=?2
                     ) select start from chain where next=start limit 1"
                ),
                params![project, row_count],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(row) = cycle {
            bail!("schema-12 decision contradiction: {table} predecessor cycle at row {row}");
        }
    }

    let decisions = conn
        .prepare(
            "select id,decision_family,action,target_ref,decision_value from owner_decisions where project_id=?1 order by id",
        )?
        .query_map(params![project], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, family, action, target, value) in decisions {
        let count: i64 = match (family.as_str(), action.as_str()) {
            ("review", "adjudicate") => conn.query_row(
                "select count(*) from review_adjudication_decisions where project_id=?1 and owner_decision_id=?2 and value=?3 and ?4='review_run:'||review_run_id",
                params![project,id,value,target], |row| row.get(0))?,
            ("review", "correct_terminal") => conn.query_row(
                "select count(*) from review_correction_events e join owner_decisions historical on historical.id=e.historical_owner_decision_id where e.project_id=?1 and e.owner_decision_id=?2 and e.outcome=?3 and ?4='review_correction:'||historical.decision_handle||':'||e.boundary_handle",
                params![project,id,value,target], |row| row.get(0))?,
            ("finding", "adjudicate" | "dispose") => conn.query_row(
                "select count(*) from finding_disposition_decisions where project_id=?1 and owner_decision_id=?2 and value=?3 and ?4='finding:'||finding_id",
                params![project,id,value,target], |row| row.get(0))?,
            ("finding", "reopen") => conn.query_row(
                "select count(*) from finding_decision_epochs e join finding_decision_epochs prior on prior.project_id=e.project_id and prior.finding_id=e.finding_id and prior.epoch_number=e.epoch_number-1 and prior.status='terminal' and prior.terminal_decision_id is not null where e.project_id=?1 and e.reopen_decision_id=?2 and ?3='finding_epoch:'||e.finding_id||':'||(e.epoch_number-1) and ?4='reopened'",
                params![project,id,target,value], |row| row.get(0))?,
            ("verification", "adjudicate") => conn.query_row(
                "select count(*) from verification_adjudication_decisions where project_id=?1 and owner_decision_id=?2 and value=?3 and ?4='closure_attempt:'||closure_attempt_id",
                params![project,id,value,target], |row| row.get(0))?,
            _ => bail!("schema-12 decision contradiction: owner_decision {id} has unsupported mapping {family}/{action}"),
        };
        if count != 1 {
            bail!(
                "schema-12 decision contradiction: owner_decision {id} maps to {count} projections"
            );
        }
        let foreign_projection_count: i64 = match (family.as_str(), action.as_str()) {
            ("review", "adjudicate") => conn.query_row(
                "select (select count(*) from finding_disposition_decisions where owner_decision_id=?1)+(select count(*) from verification_adjudication_decisions where owner_decision_id=?1)+(select count(*) from review_correction_events where owner_decision_id=?1)+(select count(*) from finding_decision_epochs where terminal_decision_id=?1 or reopen_decision_id=?1)",params![id],|row|row.get(0))?,
            ("review", "correct_terminal") => conn.query_row(
                "select (select count(*) from review_adjudication_decisions where owner_decision_id=?1)+(select count(*) from finding_disposition_decisions where owner_decision_id=?1)+(select count(*) from verification_adjudication_decisions where owner_decision_id=?1)+(select count(*) from finding_decision_epochs where terminal_decision_id=?1 or reopen_decision_id=?1)",params![id],|row|row.get(0))?,
            ("finding", "adjudicate" | "dispose") => conn.query_row(
                "select (select count(*) from review_adjudication_decisions where owner_decision_id=?1)+(select count(*) from verification_adjudication_decisions where owner_decision_id=?1)+(select count(*) from review_correction_events where owner_decision_id=?1)+(select count(*) from finding_decision_epochs where reopen_decision_id=?1)",params![id],|row|row.get(0))?,
            ("finding", "reopen") => conn.query_row(
                "select (select count(*) from review_adjudication_decisions where owner_decision_id=?1)+(select count(*) from finding_disposition_decisions where owner_decision_id=?1)+(select count(*) from verification_adjudication_decisions where owner_decision_id=?1)+(select count(*) from review_correction_events where owner_decision_id=?1)+(select count(*) from finding_decision_epochs where terminal_decision_id=?1)",params![id],|row|row.get(0))?,
            ("verification", "adjudicate") => conn.query_row(
                "select (select count(*) from review_adjudication_decisions where owner_decision_id=?1)+(select count(*) from finding_disposition_decisions where owner_decision_id=?1)+(select count(*) from review_correction_events where owner_decision_id=?1)+(select count(*) from finding_decision_epochs where terminal_decision_id=?1 or reopen_decision_id=?1)",params![id],|row|row.get(0))?,
            _ => 0,
        };
        if foreign_projection_count != 0 {
            bail!(
                "schema-12 decision contradiction: owner_decision {id} has {foreign_projection_count} foreign projection rows"
            );
        }
    }
    let finding_lifecycle: Option<String> = conn.query_row(
        r#"select d.id||':'||f.id from finding_disposition_decisions d
           join findings f on f.id=d.finding_id
           where d.project_id=?1
             and not exists(select 1 from finding_disposition_decisions n where n.predecessor_id=d.id)
             and ((d.value in ('rejected','authority_disposed')
                   and not exists(select 1 from finding_decision_epochs e where e.finding_id=f.id and e.terminal_decision_id=d.owner_decision_id))
                  or (d.value not in ('rejected','authority_disposed')
                      and exists(select 1 from finding_decision_epochs e where e.finding_id=f.id and e.terminal_decision_id=d.owner_decision_id)))
           order by d.id limit 1"#,
        params![project], |row| row.get(0)).optional()?;
    if let Some(rows) = finding_lifecycle {
        bail!("schema-12 decision/lifecycle contradiction: finding decision rows {rows}");
    }
    let verification_lifecycle: Option<String> = conn.query_row(
        r#"select d.id||':'||a.id from verification_adjudication_decisions d
           join closure_attempts a on a.id=d.closure_attempt_id
           left join finding_verifications v on v.id=(select max(id) from finding_verifications where closure_attempt_id=a.id)
           where d.project_id=?1
             and not exists(select 1 from verification_adjudication_decisions n where n.predecessor_id=d.id)
             and ((d.value='accepted' and (v.id is null or a.result is not v.result))
                  or (d.value!='accepted' and a.result is not null))
           order by d.id limit 1"#,
        params![project], |row| row.get(0)).optional()?;
    if let Some(rows) = verification_lifecycle {
        bail!("schema-12 decision/lifecycle contradiction: verification decision rows {rows}");
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
    if schema_version < CORE_SCHEMA_VERSION {
        return Ok(true);
    }
    if !table_exists(conn, "closure_attempts")?
        || !table_exists(conn, "review_adjudication_decisions")?
        || !table_exists(conn, "finding_disposition_decisions")?
        || !table_exists(conn, "verification_adjudication_decisions")?
        || !table_exists(conn, "finding_lifecycle_events")?
        || !table_exists(conn, "review_correction_events")?
        || !table_exists(conn, "review_boundary_snapshots")?
        || !table_exists(conn, "review_correction_recovery_obligations")?
        || !table_exists(conn, "finding_decision_epochs")?
        || !table_exists(conn, "legacy_claim_audits")?
        || !table_exists(conn, "legacy_review_acceptance_migrations")?
        || !table_exists(conn, "legacy_finding_audits")?
        || !table_exists(conn, "legacy_migration_candidates")?
        || !table_exists(conn, "legacy_migration_candidate_members")?
        || !table_exists(conn, "legacy_migration_edges")?
        || !table_exists(conn, "legacy_migration_projections")?
        || !table_exists(conn, "authority_migration_sources")?
        || !table_exists(conn, "legacy_adjudication_migrations")?
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
    let current_tasks: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='view' and name='current_tasks' and sql like '%revision.status=''current''%')",
        [],
        |row| row.get(0),
    )?;
    if !current_tasks {
        return Ok(true);
    }
    let current_task_gates: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='view' and name='current_task_validation_gates' and sql like '%current_tasks%')",
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
    let verification_plan_replacement_triggers: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='trigger' and name='trg_finding_verification_project_insert' and sql like '%verifier_plan.work_unit_id%') and exists(select 1 from sqlite_schema where type='trigger' and name='trg_finding_verification_project_update' and sql like '%verifier_plan.work_unit_id%')",
        [],
        |row| row.get(0),
    )?;
    if !verification_plan_replacement_triggers {
        return Ok(true);
    }
    let correction_semantic_triggers: bool = conn.query_row(
        r#"
        select exists(select 1 from sqlite_schema where type='trigger' and name='trg_correction_token_links_insert' and sql like '%phase_dependency_max%' and sql like '%decomposition-plan-reconcile%')
           and exists(select 1 from sqlite_schema where type='trigger' and name='trg_correction_application_links_insert' and sql like '%work_phase_task_memberships%' and sql like '%decomposition-plan:%')
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

fn finding_type_domain_current(conn: &Connection) -> Result<bool> {
    if !table_exists(conn, "findings")? {
        return Ok(true);
    }
    let sql = conn
        .query_row(
            "select sql from sqlite_schema where type='table' and name='findings'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(sql.is_some_and(|sql| {
        [
            "'validation_finding'",
            "'process_finding'",
            "'security_finding'",
        ]
        .into_iter()
        .all(|value| sql.contains(value))
    }))
}

fn migrate_finding_type_domain(conn: &Connection) -> Result<()> {
    if finding_type_domain_current(conn)? {
        return Ok(());
    }
    let legacy_alter_table: i64 =
        conn.pragma_query_value(None, "legacy_alter_table", |row| row.get(0))?;
    conn.pragma_update(None, "legacy_alter_table", true)?;
    let result = conn.execute_batch(
        r#"
        alter table findings rename to findings_before_type_domain;
        create table findings (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            review_run_id integer not null references review_runs(id) on delete cascade,
            finding_type text not null check (finding_type in ('validation_finding', 'process_finding', 'security_finding', 'design_finding', 'design_implementation_drift', 'design_task_gap', 'implementation_finding', 'coverage_finding')),
            severity text not null check (severity in ('critical', 'high', 'medium', 'low')),
            description text not null,
            classification text not null default 'unclassified' check (classification in ('unclassified', 'valid', 'invalid', 'design_conflict', 'needs_evidence')),
            status text not null default 'open' check (status in ('open', 'closed', 'accepted_out_of_scope')),
            lifecycle_state text not null default 'open' check(lifecycle_state in ('open','remediating','awaiting_verification','closed')),
            close_reason text check(close_reason is null or close_reason in ('verified','rejected','authority_disposed','legacy_rejected')),
            design_requirement_id integer references design_requirements(id),
            task_id integer references tasks(id),
            created_at text not null
        );
        insert into findings(
            id,project_id,review_run_id,finding_type,severity,description,classification,status,
            lifecycle_state,close_reason,design_requirement_id,task_id,created_at
        )
        select
            id,project_id,review_run_id,finding_type,severity,description,classification,status,
            lifecycle_state,close_reason,design_requirement_id,task_id,created_at
        from findings_before_type_domain;
        drop table findings_before_type_domain;
        "#,
    );
    let restore = conn.pragma_update(None, "legacy_alter_table", legacy_alter_table != 0);
    result?;
    restore?;
    Ok(())
}

pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    if !conn.is_autocommit() {
        return migrate_steps(conn);
    }
    run_atomic_schema_migration(conn, migrate_steps)
}

pub(crate) fn run_atomic_schema_migration(
    conn: &Connection,
    operation: impl FnOnce(&Connection) -> Result<()>,
) -> Result<()> {
    let foreign_keys_enabled: i64 =
        conn.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
    conn.pragma_update(None, "foreign_keys", false)?;
    let migration_result = (|| -> Result<()> {
        let transaction = conn.unchecked_transaction()?;
        operation(&transaction)?;
        let foreign_key_violations: i64 =
            transaction.query_row("select count(*) from pragma_foreign_key_check", [], |row| {
                row.get(0)
            })?;
        if foreign_key_violations != 0 {
            bail!("schema migration produced {foreign_key_violations} foreign key violation(s)");
        }
        transaction.commit()?;
        Ok(())
    })();
    let restore_result = conn.pragma_update(None, "foreign_keys", foreign_keys_enabled != 0);
    migration_result?;
    restore_result?;
    Ok(())
}

fn migrate_steps(conn: &Connection) -> Result<()> {
    let starting_generation = if table_exists(conn, "schema_migrations")? {
        conn.query_row(
            "select coalesce(max(version),0) from schema_migrations",
            [],
            |row| row.get::<_, i64>(0),
        )?
    } else {
        0
    };
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
        drop trigger if exists trg_correction_identity_link_insert;
        drop trigger if exists trg_correction_identity_link_immutable_update;
        drop trigger if exists trg_correction_identity_link_immutable_delete;
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
        drop trigger if exists trg_finding_epoch_immutable_update;
        drop trigger if exists trg_owner_decision_immutable_update;
        drop trigger if exists trg_owner_decision_immutable_delete;
        drop trigger if exists trg_decision_continuation_insert;
        drop trigger if exists trg_decision_continuation_update;
        drop trigger if exists trg_decision_continuation_delete;
        drop view if exists valid_completion_inheritance_sources;
        "#,
    )?;
    retire_obsolete_review_plan_supersessions(conn)?;
    prepare_acceptance_records_for_schema(conn)
        .context("schema migration failed while preparing acceptance records")?;
    migrate_completion_identity_link_kind(conn)
        .context("schema migration failed while preparing completion identity links")?;
    prepare_review_runs_for_schema(conn)
        .context("schema migration failed while preparing review runs")?;
    prepare_review_invocations_for_schema(conn)
        .context("schema migration failed while preparing review invocations")?;
    prepare_adjudication_for_schema(conn)
        .context("schema migration failed while preparing adjudication records")?;
    prepare_project_scoped_ledger_rows_for_schema(conn)
        .context("schema migration failed while preparing project-scoped rows")?;
    drop_phase_review_target_reference_triggers(conn)
        .context("schema migration failed while preparing phase review targets")?;
    execute_schema_batches(conn)
        .context("schema migration failed while installing schema batches")?;
    normalize_legacy_review_acceptances(conn)
        .context("schema migration failed while normalizing accepted review history")?;
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
        drop trigger if exists trg_correction_identity_link_insert;
        drop trigger if exists trg_correction_identity_link_immutable_update;
        drop trigger if exists trg_correction_identity_link_immutable_delete;
        "#,
    )?;
    migrate_acceptance_records(conn)
        .context("schema migration failed while migrating acceptance records")?;
    repair_acceptance_record_references(conn)
        .context("schema migration failed while repairing acceptance references")?;
    migrate_repository_snapshot_comparisons(conn)
        .context("schema migration failed while migrating repository comparisons")?;
    migrate_kpt_items(conn).context("schema migration failed while migrating KPT items")?;
    conn.execute_batch(
        "drop trigger if exists trg_review_adjudication_project_insert;
         drop trigger if exists trg_finding_disposition_project_insert;
         drop trigger if exists trg_verification_adjudication_project_insert;
         drop trigger if exists trg_review_correction_project_insert;
         drop trigger if exists trg_review_boundary_project_insert;
        drop trigger if exists trg_finding_epoch_project_insert;",
    )?;
    migrate_review_runs(conn).context("schema migration failed while migrating review runs")?;
    migrate_resume_check_items(conn)
        .context("schema migration failed while migrating resume checks")?;
    validate_project_scoped_ledger_links(conn)?;
    validate_review_required_links(conn)?;
    refresh_review_integrity_triggers(conn)
        .context("schema migration failed while refreshing review integrity triggers")?;
    refresh_ledger_integrity_triggers(conn)
        .context("schema migration failed while refreshing ledger integrity triggers")?;
    ensure_phase_schema(conn).context("schema migration failed while ensuring phase schema")?;
    migrate_review_runs_phase_targets(conn)
        .context("schema migration failed while migrating phase review targets")?;
    conn.execute_batch(
        "drop trigger if exists trg_verification_adjudication_project_insert;
         drop trigger if exists trg_finding_epoch_project_insert;",
    )?;
    ensure_closure_lifecycle_schema(conn)
        .context("schema migration failed while ensuring closure lifecycle schema")?;
    migrate_finding_type_domain(conn)
        .context("schema migration failed while extending the finding type domain")?;
    backfill_terminal_finding_epochs(conn)
        .context("schema migration failed while normalizing terminal finding epochs")?;
    ensure_phase_review_target_reference_triggers(conn)
        .context("schema migration failed while installing phase target references")?;
    execute_schema_batches(conn)
        .context("schema migration failed while reinstalling schema batches")?;
    ensure_phase_review_target_reference_triggers(conn)
        .context("schema migration failed while refreshing phase target references")?;
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

    if current_version < CORE_SCHEMA_VERSION {
        conn.execute(
            "insert into schema_migrations(version, applied_at) values (?1, current_timestamp)",
            params![CORE_SCHEMA_VERSION],
        )?;
    }
    if starting_generation == 0 {
        install_storage_generation_14(conn)?;
        install_storage_generation_15(conn)?;
        install_storage_generation_16(conn)?;
        install_storage_generation_17(conn)?;
        install_storage_generation_18(conn)?;
        install_storage_generation_19(conn)?;
        install_storage_generation_20(conn)?;
        install_storage_generation_21(conn)?;
        install_storage_generation_22(conn)?;
        install_storage_generation_23(conn)?;
        install_storage_generation_24(conn)?;
        install_storage_generation_25(conn)?;
    }
    if current_version >= 15 {
        conn.execute_batch(GENERATION_15_APPLICATION_LINK_SQL)?;
        conn.execute_batch(crate::task_identity::schema::SQL)?;
        crate::phases::install_phase_epochs(conn)?;
        ensure_closure_lifecycle_schema(conn)?;
    }
    if current_version >= 20 {
        install_storage_generation_20(conn)?;
        install_storage_generation_21(conn)?;
        install_storage_generation_22(conn)?;
        install_storage_generation_23(conn)?;
        install_storage_generation_24(conn)?;
        install_storage_generation_25(conn)?;
    }

    normalize_decomposition_plan_heads(conn)
        .context("schema migration failed while normalizing Decomposition Plan heads")?;

    Ok(())
}

fn decomposition_plan_heads_require_normalization(conn: &Connection) -> Result<bool> {
    if !table_exists(conn, "decomposition_plans")? || !table_exists(conn, "design_versions")? {
        return Ok(false);
    }
    conn.query_row(
        r#"
        select exists(
          select 1
          from decomposition_plans older
          join design_versions older_version on older_version.id=older.design_version_id
          where older.work_unit_id is not null
            and older.status!='superseded'
            and exists(
              select 1
              from decomposition_plans newer
              join design_versions newer_version on newer_version.id=newer.design_version_id
              where newer.project_id=older.project_id
                and newer.work_unit_id=older.work_unit_id
                and newer.status!='superseded'
                and (
                  (older.status='applied' and newer.status='applied')
                  or (
                    older.status in ('draft','incomplete','ready')
                    and newer.status in ('draft','incomplete','ready')
                  )
                )
                and newer_version.design_package_id=older_version.design_package_id
                and (
                  newer_version.version_number>older_version.version_number
                  or (
                    newer_version.version_number=older_version.version_number
                    and (
                      newer.revision>older.revision
                      or (newer.revision=older.revision and newer.id>older.id)
                    )
                  )
                )
            )
        )
        "#,
        [],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn normalize_decomposition_plan_heads(conn: &Connection) -> Result<()> {
    if !decomposition_plan_heads_require_normalization(conn)? {
        return Ok(());
    }
    let obsolete = conn
        .prepare(
            r#"
            select older.id,older.project_id,older.work_unit_id,older.design_version_id
            from decomposition_plans older
            join design_versions older_version on older_version.id=older.design_version_id
            where older.work_unit_id is not null
              and older.status!='superseded'
              and exists(
                select 1
                from decomposition_plans newer
                join design_versions newer_version on newer_version.id=newer.design_version_id
                where newer.project_id=older.project_id
                  and newer.work_unit_id=older.work_unit_id
                  and newer.status!='superseded'
                  and (
                    (older.status='applied' and newer.status='applied')
                    or (
                      older.status in ('draft','incomplete','ready')
                      and newer.status in ('draft','incomplete','ready')
                    )
                  )
                  and newer_version.design_package_id=older_version.design_package_id
                  and (
                    newer_version.version_number>older_version.version_number
                    or (
                      newer_version.version_number=older_version.version_number
                      and (
                        newer.revision>older.revision
                        or (newer.revision=older.revision and newer.id>older.id)
                      )
                    )
                  )
              )
            order by older.project_id,older.work_unit_id,older_version.version_number,older.id
            "#,
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    for (plan, project, work, design) in obsolete {
        conn.execute(
            "update decomposition_plans set status='superseded' where id=?1 and status!='superseded'",
            [plan],
        )?;
        conn.execute(
            "update decomposition_items set status='superseded' where decomposition_plan_id=?1 and status='open'",
            [plan],
        )?;
        conn.execute(
            r#"
            update phase_epochs
            set state=case when state in ('open','blocked') then 'superseded' else state end,
                terminal_at=case when state in ('open','blocked')
                                 then coalesce(terminal_at,current_timestamp) else terminal_at end,
                phase_key='superseded-plan-'||?1||'-phase-'||id,
                terminal_summary=case when state in ('open','blocked')
                  then coalesce(terminal_summary,'superseded with Decomposition Plan '||?1||' history')
                  else terminal_summary end
            where project_id=?2 and work_unit_id=?3 and design_version_id=?4
            "#,
            params![plan, project, work, design],
        )?;
        conn.execute(
            r#"
            update phase_epoch_memberships
            set state='superseded',terminal_at=current_timestamp
            where state='current' and phase_epoch_id in (
              select id from phase_epochs
              where project_id=?1 and work_unit_id=?2 and design_version_id=?3
            )
            "#,
            params![project, work, design],
        )?;
        conn.execute(
            r#"
            update phase_epoch_dependencies
            set state='invalidated',terminal_at=current_timestamp
            where state='open' and (
              from_phase_epoch_id in (
                select id from phase_epochs
                where project_id=?1 and work_unit_id=?2 and design_version_id=?3
              )
              or to_phase_epoch_id in (
                select id from phase_epochs
                where project_id=?1 and work_unit_id=?2 and design_version_id=?3
              )
            )
            "#,
            params![project, work, design],
        )?;
        conn.execute(
            r#"
            update work_phases
            set phase_key='superseded-plan-'||?1||'-phase-'||id
            where project_id=?2 and work_unit_id=?3 and design_version_id=?4
              and phase_key not like 'superseded-plan-%'
            "#,
            params![plan, project, work, design],
        )?;
        conn.execute(
            "update checklists set status='stale' where project_id=?1 and work_unit_id=?2 and design_version_id=?3 and status='active'",
            params![project, work, design],
        )?;
        conn.execute(
            r#"
            update task_derivations set status='stale'
            where project_id=?1 and status='active'
              and design_requirement_id in (
                select id from design_requirements where design_version_id=?2
              )
              and task_id in (
                select id from tasks where work_unit_id=?3
              )
            "#,
            params![project, design, work],
        )?;
        conn.execute(
            r#"
            update validation_gates set status='stale'
            where project_id=?1 and work_unit_id=?2 and status='active'
              and (
                design_requirement_id in (
                  select id from design_requirements where design_version_id=?3
                )
                or template_id in (
                  select id from validation_gate_templates where design_version_id=?3
                )
              )
            "#,
            params![project, work, design],
        )?;
    }
    Ok(())
}

fn retire_obsolete_review_plan_supersessions(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "review_plan_supersessions")? {
        return Ok(());
    }
    conn.execute_batch(
        r#"
        insert into acceptance_records(
          project_id,target_type,review_plan_id,acceptance_type,reason,scope,
          created_by,status,approved_by_authority_event_id,approved_at,created_at,review_impact
        )
        select project_id,'review_plan',predecessor_plan_id,'stale_accepted',reason,
               'review-plan-supersession','user','approved',authority_event_id,
               created_at,created_at,'superseded_by_review_plan:'||successor_plan_id
        from review_plan_supersessions old
        where not exists(
          select 1 from acceptance_records current
          where current.target_type='review_plan'
            and current.review_plan_id=old.predecessor_plan_id
            and current.status='approved'
            and current.review_impact='superseded_by_review_plan:'||old.successor_plan_id
        );
        update review_plans set status='not_required'
        where id in(select predecessor_plan_id from review_plan_supersessions);
        drop trigger if exists trg_review_plan_supersession_immutable_update;
        drop trigger if exists trg_review_plan_supersession_immutable_delete;
        drop table review_plan_supersessions;
        "#,
    )?;
    Ok(())
}

fn backfill_terminal_finding_epochs(conn: &Connection) -> Result<()> {
    let terminal_findings = conn
        .prepare(
            r#"
            select f.project_id, f.id,
                   coalesce(
                     (
                       select decision.owner_decision_id
                       from verification_adjudication_decisions decision
                       join closure_attempts attempt on attempt.id=decision.closure_attempt_id
                       join closures closure on closure.id=attempt.closure_id
                       join finding_verifications verification
                         on verification.closure_attempt_id=attempt.id
                        and verification.result='verified'
                       where closure.finding_id=f.id and decision.value='accepted'
                         and not exists(
                           select 1 from verification_adjudication_decisions successor
                           where successor.predecessor_id=decision.id
                         )
                       order by decision.id desc limit 1
                     ),
                     (
                       select decision.owner_decision_id
                       from finding_disposition_decisions decision
                       where decision.finding_id=f.id
                         and decision.value in ('rejected','authority_disposed')
                         and not exists(
                           select 1 from finding_disposition_decisions successor
                           where successor.predecessor_id=decision.id
                         )
                       order by decision.id desc limit 1
                     )
                   )
            from findings f
            where f.lifecycle_state='closed'
              and not exists(
                select 1 from finding_decision_epochs epoch
                where epoch.project_id=f.project_id and epoch.finding_id=f.id
              )
            order by f.project_id,f.id
            "#,
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (project, finding, owner_decision) in terminal_findings {
        let Some(owner_decision) = owner_decision else {
            continue;
        };
        conn.execute(
            "insert into finding_decision_epochs(project_id,finding_id,epoch_number,terminal_decision_id,status,created_at) values(?1,?2,1,?3,'terminal',current_timestamp)",
            params![project, finding, owner_decision],
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
    for (
        run,
        clean,
        count,
        status,
        target,
        _work,
        _work_status,
        _design_status,
        _run_type,
        plan_id,
        _design_context,
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
        let trusted_provenance: i64 = conn.query_row(
            r#"select exists(
                   select 1 from review_runs r
                   where r.project_id=?1 and r.id=?2
                     and trim(coalesce(r.review_provenance_ref,''))!=''
                     and (
                       r.review_provenance='human_review'
                       or (r.review_provenance='external_agent' and exists(
                         select 1 from review_agent_invocations i
                         where i.project_id=r.project_id and i.review_run_id=r.id
                           and trim(coalesce(i.external_agent_id,''))!=''
                       ))
                     )
               )"#,
            params![project, run],
            |row| row.get(0),
        )?;
        let resolution = if trusted_provenance == 1 {
            "trusted"
        } else {
            "unbound"
        };
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
    }
    super::adjudication_migration::record_candidate_projections(conn, project)?;
    Ok(())
}

fn normalize_legacy_review_acceptances(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "owner_decisions")?
        || !table_exists(conn, "review_adjudication_decisions")?
        || !table_exists(conn, "legacy_claim_audits")?
    {
        return Ok(());
    }
    let rows = conn
        .prepare(
            r#"select r.project_id,r.id,o.id,r.target_ref,p.stage,p.review_type,p.design_version_id,p.work_unit_id,
                      coalesce(v.status,''),coalesce(w.status,'')
               from review_runs r
               join review_plans p on p.id=r.review_plan_id
               join work_units w on w.id=p.work_unit_id
               left join design_versions v on v.id=p.design_version_id
               join review_adjudication_decisions d on d.project_id=r.project_id and d.review_run_id=r.id
               join owner_decisions o on o.id=d.owner_decision_id
               where r.status='completed' and p.status='clean' and d.value='accepted'
                 and exists(
                   select 1 from temp.legacy_review_acceptance_sources source
                   where source.owner_decision_id=o.id
                 )
                 and not exists(select 1 from review_adjudication_decisions n where n.predecessor_id=d.id)
               order by r.project_id,r.id"#,
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<i64>>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (
        project,
        run,
        owner_decision,
        target,
        stage,
        review_type,
        design,
        work,
        design_status,
        work_status,
    ) in rows
    {
        let context_kind = match (stage.as_str(), review_type.as_str()) {
            ("design-ready", "design_review") => Some("design-review"),
            ("implementation-ready", "design_task_decomposition") => {
                Some("design-task-decomposition")
            }
            ("close-ready", "design_implementation_diff") => Some("design-implementation-diff"),
            ("close-ready", "implementation_review") => Some("implementation-review"),
            _ => None,
        };
        let current_target = if let (Some(kind), Some(design)) = (context_kind, design) {
            design_status == "approved"
                && target.as_deref()
                    == Some(review_context_ref(kind, Some(design), Some(work)).as_str())
        } else {
            matches!(work_status.as_str(), "open" | "blocked")
                && target.as_deref() == Some(format!("work_unit:{work}").as_str())
        };
        if !current_target {
            continue;
        }
        let digest = domain_digest(
            b"agent-workbench:legacy-review-acceptance-migration-v1\0",
            &CanonicalValue::object([
                ("project", CanonicalValue::Integer(project)),
                ("run", CanonicalValue::Integer(run)),
                ("owner_decision", CanonicalValue::Integer(owner_decision)),
            ]),
        );
        conn.execute(
            r#"insert or ignore into legacy_review_acceptance_migrations(
                   project_id,review_run_id,owner_decision_id,content_digest,created_at
               ) values(?1,?2,?3,?4,current_timestamp)"#,
            params![project, run, owner_decision, digest],
        )?;
        conn.execute(
            "update review_plans set status='blocked' where project_id=?1 and id=(select review_plan_id from review_runs where id=?2) and status='clean'",
            params![project, run],
        )?;
    }
    conn.execute_batch(
        "drop trigger if exists trg_legacy_signed_review_effect_update;
         drop trigger if exists trg_legacy_signed_review_effect_delete;
         drop table if exists legacy_signed_review_effects;
         drop table if exists temp.legacy_review_acceptance_sources;",
    )?;
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
        ("provenance_handle", "text"),
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
        // This digest identifies only the immutable legacy source reference.
        // The current model resolves it through project-local migration provenance.
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
    conn.execute_batch(
        "drop table if exists temp.legacy_review_acceptance_sources;
         create temp table legacy_review_acceptance_sources(
           owner_decision_id integer primary key
         );",
    )?;
    if table_exists(conn, "findings")? {
        ensure_column(
            conn,
            "findings",
            "lifecycle_state",
            "text not null default 'open'",
        )?;
        ensure_column(conn, "findings", "close_reason", "text")?;
    }
    if table_exists(conn, "owner_decisions")? {
        let has_capability = table_has_column(conn, "owner_decisions", "capability_id")?;
        let has_principal = table_has_column(conn, "owner_decisions", "principal_id")?;
        if has_capability && has_principal {
            conn.execute(
                "insert or ignore into temp.legacy_review_acceptance_sources(owner_decision_id) select id from owner_decisions where capability_id is not null and principal_id is not null",
                [],
            )?;
        }
        if table_exists(conn, "legacy_signed_review_effects")? {
            conn.execute(
                "insert or ignore into temp.legacy_review_acceptance_sources(owner_decision_id) select owner_decision_id from legacy_signed_review_effects",
                [],
            )?;
        }
        if has_capability || has_principal {
            conn.execute_batch(
                r#"
                create table owner_decisions_without_retired_authority (
                    id integer primary key,
                    project_id integer not null references projects(id) on delete cascade,
                    decision_handle text not null,
                    owner_ref text not null,
                    target_ref text not null,
                    decision_family text not null,
                    action text not null,
                    decision_value text not null,
                    reason text not null,
                    expected_current text not null,
                    payload_digest text not null check(length(payload_digest)=64),
                    created_at text not null,
                    unique(project_id, decision_handle)
                );
                insert into owner_decisions_without_retired_authority(
                    id,project_id,decision_handle,owner_ref,target_ref,decision_family,action,
                    decision_value,reason,expected_current,payload_digest,created_at
                )
                select id,project_id,decision_handle,owner_ref,target_ref,decision_family,action,
                       decision_value,reason,expected_current,payload_digest,created_at
                from owner_decisions;
                drop table owner_decisions;
                alter table owner_decisions_without_retired_authority rename to owner_decisions;
                "#,
            )?;
        }
    }
    Ok(())
}
