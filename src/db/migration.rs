use std::path::Path;

use crate::identity::{CanonicalValue, domain_digest};
use crate::review_context::review_context_ref;
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
    Ok(conn)
}

pub(crate) fn apply_pending_update(conn: &Connection) -> Result<()> {
    let source_generation = conn
        .query_row(
            "select coalesce(max(version),0) from schema_migrations",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    if (6..=12).contains(&source_generation) {
        let foreign_keys_enabled: i64 =
            conn.pragma_query_value(None, "foreign_keys", |row| row.get(0))?;
        conn.pragma_update(None, "foreign_keys", false)?;
        let migration_result = (|| -> Result<()> {
            let tx = conn.unchecked_transaction()?;
            let project = project_id(&tx)?;
            let digest = super::adjudication_migration::legacy_source_digest(
                &tx,
                project,
                source_generation,
            )?;
            if source_generation == 12 {
                validate_schema12_owner_decisions(&tx, project)?;
            }
            migrate(&tx).with_context(|| {
                format!(
                    "schema-{source_generation} migration failed while installing the current schema"
                )
            })?;
            if source_generation <= 11 {
                tx.execute("insert or ignore into authority_migration_sources(project_id,source_ledger_digest,source_generation,created_at) values(?1,?2,?3,current_timestamp)",params![project,digest,source_generation]).with_context(|| format!("schema-{source_generation} migration failed while recording the source ledger"))?;
                migrate_legacy_review_candidates(&tx).with_context(|| {
                    format!(
                        "schema-{source_generation} migration failed while normalizing legacy review state"
                    )
                })?;
                tx.execute("insert into legacy_adjudication_migrations(project_id,source_ledger_digest,source_generation,completed_at) select project_id,source_ledger_digest,source_generation,current_timestamp from authority_migration_sources where project_id=?1 and source_generation=?2",params![project,source_generation]).with_context(|| format!("schema-{source_generation} migration failed while recording completion"))?;
            } else {
                tx.execute("insert into schema_retirement_records(project_id,source_ledger_digest,source_generation,completed_at) values(?1,?2,?3,current_timestamp)",params![project,digest,source_generation]).context("schema-12 migration failed while recording retirement")?;
            }
            let violations: i64 =
                tx.query_row("select count(*) from pragma_foreign_key_check", [], |row| {
                    row.get(0)
                })?;
            if violations != 0 {
                bail!(
                    "schema-{source_generation} migration produced {violations} foreign key violation(s)"
                );
            }
            tx.commit().with_context(|| {
                format!("schema-{source_generation} migration failed while committing")
            })?;
            Ok(())
        })();
        let restore_result = conn.pragma_update(None, "foreign_keys", foreign_keys_enabled != 0);
        migration_result?;
        restore_result?;
        return Ok(());
    }
    if ledger_needs_migration(conn)? {
        migrate(conn).with_context(|| {
            format!("migration from schema generation {source_generation} failed while installing the current schema")
        })?;
    }
    let pending:i64=conn.query_row("select exists(select 1 from authority_migration_sources s where not exists(select 1 from legacy_adjudication_migrations m where m.project_id=s.project_id and m.source_generation=s.source_generation and m.source_ledger_digest=s.source_ledger_digest))",[],|row|row.get(0)).unwrap_or(0);
    if pending == 1 {
        let tx = conn.unchecked_transaction()?;
        migrate_legacy_review_candidates(&tx)
            .context("pending schema-11 migration failed while normalizing legacy review state")?;
        let project = project_id(&tx)?;
        tx.execute("insert into legacy_adjudication_migrations(project_id,source_ledger_digest,source_generation,completed_at) select s.project_id,s.source_ledger_digest,s.source_generation,current_timestamp from authority_migration_sources s where s.project_id=?1 and not exists(select 1 from legacy_adjudication_migrations m where m.project_id=s.project_id and m.source_generation=s.source_generation)",params![project]).context("pending schema-11 migration failed while recording completion")?;
        tx.commit()
            .context("pending schema-11 migration failed while committing")?;
    }
    Ok(())
}

pub(crate) fn project_requires_update(conn: &Connection) -> Result<bool> {
    Ok(!pending_update_changes(conn)?.is_empty())
}

pub(crate) fn pending_update_changes(conn: &Connection) -> Result<Vec<String>> {
    let mut changes = Vec::new();
    if ledger_needs_migration(conn)? {
        changes.extend(schema_profile_update_reasons(conn)?);
        if changes.is_empty() {
            changes.push("schema_or_profile_update".to_string());
        }
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
        changes.push("legacy_review_normalization".to_string());
    }
    Ok(changes)
}

fn schema_profile_update_reasons(conn: &Connection) -> Result<Vec<String>> {
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
        reasons.push(format!("schema_{version}_to_{SCHEMA_VERSION}"));
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
        "legacy_claim_audits",
        "legacy_signed_review_effects",
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
    ] {
        if !table_exists(conn, table)? {
            reasons.push(format!("missing_table:{table}"));
        }
    }
    let current_task_gates: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='view' and name='current_task_validation_gates')",
        [],
        |row| row.get(0),
    )?;
    if !current_task_gates {
        reasons.push("current_task_validation_gates_view".to_string());
    }
    let inheritance_current: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='view' and name='valid_completion_inheritance_sources' and sql like '%mapping.design_requirement_id=source.current_requirement_id%' and sql like '%candidate_version.version_number%' and sql like '%left join command_usages usage%')",
        [],
        |row| row.get(0),
    )?;
    if !inheritance_current {
        reasons.push("completion_inheritance_view".to_string());
    }
    let correction_current: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='trigger' and name='trg_correction_token_links_insert' and sql like '%phase_dependency_max%') and exists(select 1 from sqlite_schema where type='trigger' and name='trg_completion_source_insert' and sql like '%candidate_version.version_number%')",
        [],
        |row| row.get(0),
    )?;
    if !correction_current {
        reasons.push("correction_transition_profile".to_string());
    }
    let correction_status_current: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='trigger' and name='trg_correction_session_status_update') and exists(select 1 from sqlite_schema where type='trigger' and name='trg_correction_token_status_update')",
        [],
        |row| row.get(0),
    )?;
    if !correction_status_current {
        reasons.push("correction_status_profile".to_string());
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
            reasons.push(format!("trigger_profile:{trigger}"));
        }
    }
    let completion_identity_kind: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='table' and name='correction_application_identity_links' and sql like '%completion_source%')",
        [],
        |row| row.get(0),
    )?;
    if !completion_identity_kind {
        reasons.push("completion_identity_profile".to_string());
    }
    if table_exists(conn, "acceptance_records")?
        && !table_has_column(conn, "acceptance_records", "coverage_item_id")?
    {
        reasons.push("acceptance_coverage_column".to_string());
    }
    let broken_acceptance_refs: i64 = conn.query_row(
        "select count(*) from sqlite_schema where sql like '%acceptance_records_old%'",
        [],
        |row| row.get(0),
    )?;
    if broken_acceptance_refs > 0 {
        reasons.push("acceptance_reference_repair".to_string());
    }
    if table_exists(conn, "acceptance_records")? && acceptance_records_needs_migration(conn)? {
        reasons.push("acceptance_record_profile".to_string());
    }
    if table_exists(conn, "review_runs")?
        && !table_has_column(conn, "review_runs", "review_provenance")?
    {
        reasons.push("review_provenance_column".to_string());
    }
    if table_exists(conn, "closures")?
        && !table_has_column(conn, "closures", "supersession_reason")?
    {
        reasons.push("closure_supersession_column".to_string());
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
    if schema_version < SCHEMA_VERSION {
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
        || !table_exists(conn, "legacy_signed_review_effects")?
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
    run_atomic_schema_migration(conn, migrate_steps)
}

fn run_atomic_schema_migration(
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
        drop trigger if exists trg_owner_decision_immutable_update;
        drop trigger if exists trg_owner_decision_immutable_delete;
        drop view if exists valid_completion_inheritance_sources;
        "#,
    )?;
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
    preserve_signed_review_effects(conn)
        .context("schema migration failed while preserving accepted review history")?;
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

fn preserve_signed_review_effects(conn: &Connection) -> Result<()> {
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
                 and o.capability_id is not null and o.principal_id is not null
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
            b"agent-workbench:preserved-signed-review-effect-v1\0",
            &CanonicalValue::object([
                ("project", CanonicalValue::Integer(project)),
                ("run", CanonicalValue::Integer(run)),
                ("owner_decision", CanonicalValue::Integer(owner_decision)),
            ]),
        );
        conn.execute(
            r#"insert or ignore into legacy_signed_review_effects(
                   project_id,review_run_id,owner_decision_id,content_digest,created_at
               ) values(?1,?2,?3,?4,current_timestamp)"#,
            params![project, run, owner_decision, digest],
        )?;
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
    if table_exists(conn, "owner_decisions")? {
        let capability_required: i64 = conn.query_row(
            "select [notnull] from pragma_table_info('owner_decisions') where name='capability_id'",
            [],
            |row| row.get(0),
        )?;
        if capability_required == 1 {
            conn.execute_batch(
                r#"
                create table owner_decisions_v13 (
                    id integer primary key,
                    project_id integer not null references projects(id) on delete cascade,
                    decision_handle text not null,
                    capability_id integer unique,
                    principal_id integer,
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
                insert into owner_decisions_v13
                select * from owner_decisions;
                drop table owner_decisions;
                alter table owner_decisions_v13 rename to owner_decisions;
                "#,
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod atomic_schema_migration_tests {
    use super::*;

    #[test]
    fn rebuilds_a_referenced_table_atomically_and_restores_foreign_keys() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        conn.execute_batch(
            "create table parent(id integer primary key, value text not null);\
             create table child(id integer primary key, parent_id integer not null references parent(id));\
             insert into parent values(1,'before');\
             insert into child values(1,1);",
        )
        .unwrap();

        run_atomic_schema_migration(&conn, |tx| {
            tx.execute_batch(
                "pragma legacy_alter_table=on;\
                 alter table parent rename to parent_old;\
                 pragma legacy_alter_table=off;\
                 create table parent(id integer primary key, value text not null, added text);\
                 insert into parent(id,value) select id,value from parent_old;\
                 drop table parent_old;",
            )?;
            Ok(())
        })
        .unwrap();

        let foreign_keys: i64 = conn
            .pragma_query_value(None, "foreign_keys", |row| row.get(0))
            .unwrap();
        let violations: i64 = conn
            .query_row("select count(*) from pragma_foreign_key_check", [], |row| {
                row.get(0)
            })
            .unwrap();
        let child_parent: i64 = conn
            .query_row("select parent_id from child where id=1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(foreign_keys, 1);
        assert_eq!(violations, 0);
        assert_eq!(child_parent, 1);
    }

    #[test]
    fn rolls_back_when_a_rebuild_leaves_a_foreign_key_violation() {
        let conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        conn.execute_batch(
            "create table parent(id integer primary key);\
             create table child(id integer primary key, parent_id integer not null references parent(id));\
             insert into parent values(1);\
             insert into child values(1,1);",
        )
        .unwrap();

        let error = run_atomic_schema_migration(&conn, |tx| {
            tx.execute("delete from parent", [])?;
            Ok(())
        })
        .unwrap_err();

        assert!(error.to_string().contains("foreign key violation"));
        assert_eq!(
            conn.query_row("select count(*) from parent", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            conn.pragma_query_value(None, "foreign_keys", |row| row.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
}
