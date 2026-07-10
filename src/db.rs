use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use crate::work::has_stale_design_state_for_work;

pub const LEDGER_DIR: &str = ".agent-workbench";
pub const LEDGER_FILE: &str = "ledger.sqlite";
pub const DESIGN_DIR: &str = "designs";
pub const EXPORT_DIR: &str = "exports";
pub const LOG_DIR: &str = "logs";
pub(crate) const SCHEMA_VERSION: i64 = 8;

pub fn default_ledger_path(root: &Path) -> PathBuf {
    root.join(LEDGER_DIR).join(LEDGER_FILE)
}

pub fn default_design_root(root: &Path) -> PathBuf {
    root.join(LEDGER_DIR).join(DESIGN_DIR)
}

pub fn default_export_root(root: &Path) -> PathBuf {
    root.join(LEDGER_DIR).join(EXPORT_DIR)
}

pub fn default_log_root(root: &Path) -> PathBuf {
    root.join(LEDGER_DIR).join(LOG_DIR)
}

pub fn init_project(root: &Path) -> Result<InitOutcome> {
    let ledger_path = default_ledger_path(root);
    if let Some(parent) = ledger_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create ledger directory {}", parent.display()))?;
    }
    for directory in [
        default_design_root(root),
        default_export_root(root),
        default_log_root(root),
    ] {
        fs::create_dir_all(&directory)
            .with_context(|| format!("failed to create directory {}", directory.display()))?;
    }

    let conn = open_ledger(&ledger_path)?;
    migrate(&conn)?;
    ensure_project(&conn, root)?;
    sync_agents_md_authority(&conn, root)?;
    sync_commit_message_policy(&conn)?;

    Ok(InitOutcome { ledger_path })
}

pub fn project_status(root: &Path) -> Result<ProjectStatus> {
    let ledger_path = default_ledger_path(root);
    if !ledger_path.exists() {
        return Ok(ProjectStatus {
            initialized: false,
            ledger_path,
            project_name: None,
            open_work_units: 0,
            active_activations: 0,
            schema_version: None,
            phase_blocker: None,
            finding_remediations: Vec::new(),
        });
    }

    let conn = open_ledger(&ledger_path)?;
    migrate_if_needed(&conn)?;
    let project_name = conn
        .query_row("select name from projects order by id limit 1", [], |row| {
            row.get::<_, String>(0)
        })
        .optional()?;
    let open_work_units = count_rows(&conn, "work_units", "status in ('open', 'blocked')")?;
    let active_activations = count_rows(&conn, "work_unit_activations", "status = 'active'")?;
    let schema_version = conn
        .query_row(
            "select version from schema_migrations order by version desc limit 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let phase_blocker = current_phase_blocker(&conn)?;
    let finding_remediations = if phase_blocker.is_none() {
        current_finding_remediations(&conn)?
    } else {
        Vec::new()
    };

    Ok(ProjectStatus {
        initialized: true,
        ledger_path,
        project_name,
        open_work_units,
        active_activations,
        schema_version,
        phase_blocker,
        finding_remediations,
    })
}

pub fn next_action(root: &Path) -> Result<NextAction> {
    let ledger_path = default_ledger_path(root);
    if !ledger_path.exists() {
        return Ok(NextAction::NotInitialized { ledger_path });
    }

    let conn = open_ledger(&ledger_path)?;
    migrate_if_needed(&conn)?;
    if let Some(blocker) = current_phase_blocker(&conn)? {
        return Ok(NextAction::BlockedPhase { blocker });
    }
    let remediations = current_finding_remediations(&conn)?;
    if !remediations.is_empty() {
        return Ok(NextAction::FindingRemediation { remediations });
    }

    let active = conn
        .query_row(
            r#"
            select w.id, w.title,
                   (
                       select max(c.design_version_id)
                       from checklists c
                       where c.work_unit_id = w.id
                         and c.status in ('active', 'stale')
                   ) as design_version_id
            from work_unit_activations a
            join work_units w on w.id = a.work_unit_id
            where a.status = 'active'
            order by a.id desc
            limit 1
            "#,
            [],
            |row| {
                Ok(ActiveWorkUnit {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    design_version_id: row.get(2)?,
                    next_phase_id: None,
                    next_phase_key: None,
                    next_phase_title: None,
                })
            },
        )
        .optional()?;

    if let Some(work_unit) = active {
        return Ok(NextAction::ContinueActive {
            work_unit: attach_next_phase(&conn, work_unit)?,
        });
    }

    let suspended = conn
        .query_row(
            r#"
            select w.id, w.title,
                   (
                       select max(c.design_version_id)
                       from checklists c
                       where c.work_unit_id = w.id
                         and c.status in ('active', 'stale')
                   ) as design_version_id
            from work_unit_activations a
            join work_units w on w.id = a.work_unit_id
            where a.status = 'suspended'
              and w.status in ('open', 'blocked')
            order by a.id desc
            limit 1
            "#,
            [],
            |row| {
                Ok(ActiveWorkUnit {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    design_version_id: row.get(2)?,
                    next_phase_id: None,
                    next_phase_key: None,
                    next_phase_title: None,
                })
            },
        )
        .optional()?;
    if let Some(work_unit) = suspended {
        return Ok(NextAction::ResumeSuspended {
            work_unit: attach_next_phase(&conn, work_unit)?,
        });
    }

    let inactive = conn
        .query_row(
            r#"
            select w.id, w.title,
                   (
                       select max(c.design_version_id)
                       from checklists c
                       where c.work_unit_id = w.id
                         and c.status in ('active', 'stale')
                   ) as design_version_id
            from work_units w
            where w.status = 'open'
              and not exists (
                  select 1
                  from work_unit_activations a
                  where a.work_unit_id = w.id
                    and a.status in ('active', 'suspended')
              )
            order by w.id
            limit 1
            "#,
            [],
            |row| {
                Ok(ActiveWorkUnit {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    design_version_id: row.get(2)?,
                    next_phase_id: None,
                    next_phase_key: None,
                    next_phase_title: None,
                })
            },
        )
        .optional()?;

    Ok(match inactive {
        Some(work_unit) => NextAction::ActivateOpen {
            work_unit: attach_next_phase(&conn, work_unit)?,
        },
        None => NextAction::NoOpenWorkUnit,
    })
}

fn current_finding_remediations(conn: &Connection) -> Result<Vec<FindingRemediation>> {
    let project_id = project_id(conn)?;
    let mut stmt = conn.prepare(
        r#"
        select p.id, p.work_unit_id, f.id, c.id, f.description,
               coalesce(c.affected_surfaces, '-'), coalesce(c.fix_plan, '-')
        from review_plans p
        join review_runs r on r.review_plan_id = p.id
        join findings f on f.review_run_id = r.id
        join closures c on c.finding_id = f.id and c.status = 'registered'
        join work_unit_activations a on a.work_unit_id = p.work_unit_id and a.status = 'active'
        where p.project_id = ?1 and p.required = 1 and p.stage = 'close-ready'
          and p.review_type in ('implementation_review', 'design_implementation_diff')
          and f.status = 'open' and f.classification = 'valid'
          and not exists (
              select 1 from acceptance_records ar
              where ar.finding_id = f.id and ar.target_type = 'finding'
                and ar.status = 'approved'
                and ar.acceptance_type in ('accepted_out_of_scope', 'explicit_exception', 'classified_failure')
          )
        order by f.id
        "#,
    )?;
    let rows = stmt.query_map(params![project_id],
        |row| {
            let closure_id = row.get(3)?;
            Ok(FindingRemediation {
                review_plan_id: row.get(0)?,
                work_unit_id: row.get(1)?,
                finding_id: row.get(2)?,
                closure_id,
                description: row.get(4)?,
                affected_surfaces: row.get(5)?,
                fix_plan: row.get(6)?,
                next_action: format!("implement the scoped fix, then agent-workbench closure ready {closure_id} --evidence \"<evidence>\" --tests \"<tests>\""),
            })
        },
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn attach_next_phase(conn: &Connection, mut work_unit: ActiveWorkUnit) -> Result<ActiveWorkUnit> {
    let next_phase = conn
        .query_row(
            r#"
            select p.id, p.phase_key, p.title
            from work_phases p
            where p.work_unit_id = ?1
              and p.status in ('open', 'blocked')
              and not exists (
                  select 1
                  from work_phase_dependencies d
                  where d.to_phase_id = p.id
                    and d.status = 'open'
              )
            order by p.phase_order, p.id
            limit 1
            "#,
            params![work_unit.id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?;
    if let Some((id, key, title)) = next_phase {
        work_unit.next_phase_id = Some(id);
        work_unit.next_phase_key = Some(key);
        work_unit.next_phase_title = Some(title);
    }
    Ok(work_unit)
}

fn current_phase_blocker(conn: &Connection) -> Result<Option<PhaseBlocker>> {
    let project_id = project_id(conn)?;
    let active_work_unit_id = conn
        .query_row(
            "select work_unit_id from work_unit_activations where project_id = ?1 and status = 'active' order by id desc limit 1",
            params![project_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    if let Some(work_unit_id) = active_work_unit_id
        && has_stale_design_state_for_work(conn, work_unit_id)?
    {
        return Ok(Some(PhaseBlocker {
            kind: "stale_design".to_string(),
            review_plan_id: None,
            work_unit_id: Some(work_unit_id),
            review_type: None,
            stage: None,
            review_run_id: None,
            finding_id: None,
            severity: Some("critical".to_string()),
            classification: None,
            description: "stale design-derived state blocks implementation and scoped remediation"
                .to_string(),
            next_action:
                "agent-workbench stale list; resolve each stale record before implementation"
                    .to_string(),
        }));
    }
    let review_blocker = conn
        .query_row(
            r#"
            select
                p.id,
                p.work_unit_id,
                p.review_type,
                p.stage,
                r.id,
                f.id,
                f.severity,
                f.classification,
                f.description,
                (
                    select c.id
                    from closures c
                    where c.finding_id = f.id
                      and c.project_id = f.project_id
                      and c.status != 'superseded'
                    order by c.id desc
                    limit 1
                ),
                (
                    select a.id
                    from closure_attempts a
                    join closures c on c.id = a.closure_id
                    where c.finding_id = f.id and a.result is null
                    order by a.id desc limit 1
                ),
                (
                    select rr.id
                    from review_runs rr
                    join closure_attempts a
                      on rr.target_ref = 'review-context:finding-fix:finding=' || f.id
                         || ':closure=' || a.closure_id || ':attempt=' || a.id
                    where rr.review_plan_id = p.id
                      and rr.project_id = p.project_id
                      and rr.run_type = 'resume'
                      and rr.run_purpose = 'finding_fix_verification'
                      and rr.id > a.review_run_high_watermark
                    order by rr.id desc
                    limit 1
                ),
                (
                    select rr.finding_fix_result
                    from review_runs rr
                    join closure_attempts a
                      on rr.target_ref = 'review-context:finding-fix:finding=' || f.id
                         || ':closure=' || a.closure_id || ':attempt=' || a.id
                    where rr.review_plan_id = p.id
                      and rr.project_id = p.project_id
                      and rr.run_type = 'resume'
                      and rr.run_purpose = 'finding_fix_verification'
                      and rr.id > a.review_run_high_watermark
                    order by rr.id desc
                    limit 1
                ),
                (select c.status from closures c where c.finding_id = f.id and c.status != 'superseded' order by c.id desc limit 1)
            from review_plans p
            join review_runs r on r.review_plan_id = p.id
            join findings f on f.review_run_id = r.id
            where p.required = 1
              and p.project_id = ?1
              and r.project_id = ?1
              and f.project_id = ?1
              and f.status = 'open'
              and f.classification in ('unclassified', 'valid', 'design_conflict', 'needs_evidence')
              and not (
                  f.classification = 'valid'
                  and p.stage = 'close-ready'
                  and p.review_type in ('implementation_review', 'design_implementation_diff')
                  and exists (
                      select 1 from closures c
                      join work_unit_activations a on a.work_unit_id = p.work_unit_id
                      where c.finding_id = f.id and c.status = 'registered'
                        and a.status = 'active'
                  )
              )
              and not exists (
                select 1
                from acceptance_records ar
                where ar.target_type = 'finding'
                  and ar.finding_id = f.id
                  and ar.status = 'approved'
                  and ar.acceptance_type in (
                    'accepted_out_of_scope', 'explicit_exception', 'classified_failure'
                  )
              )
            order by
                case p.stage
                    when 'design-ready' then 1
                    when 'implementation-ready' then 2
                    when 'close-ready' then 3
                    when 'resume-ready' then 4
                    else 5
                end,
                f.id
            limit 1
            "#,
            params![project_id],
            |row| {
                let finding_id = row.get(5)?;
                let classification: String = row.get(7)?;
                let review_plan_id = row.get(0)?;
                let closure_id = row.get(9)?;
                let attempt_id = row.get(10)?;
                let verification_run_id: Option<i64> = row.get(11)?;
                let verification_result = row.get::<_, Option<String>>(12)?;
                let closure_status = row.get::<_, Option<String>>(13)?;
                Ok(PhaseBlocker {
                    kind: "required_review_finding".to_string(),
                    review_plan_id: Some(review_plan_id),
                    work_unit_id: Some(row.get(1)?),
                    review_type: Some(row.get(2)?),
                    stage: Some(row.get(3)?),
                    review_run_id: Some(row.get(4)?),
                    finding_id: Some(finding_id),
                    severity: Some(row.get(6)?),
                    classification: Some(classification.clone()),
                    description: row.get(8)?,
                    next_action: finding_next_action(
                        finding_id,
                        review_plan_id,
                        closure_id,
                        closure_status.as_deref(),
                        attempt_id,
                        verification_run_id
                            .map(|run_id| (run_id, verification_result.as_deref())),
                        &classification,
                    ),
                })
            },
        )
        .optional()?;
    if review_blocker.is_some() {
        return Ok(review_blocker);
    }

    conn.query_row(
        r#"
        select w.id, w.title
        from work_units w
        where w.status = 'blocked'
          and w.project_id = ?1
        order by w.id
        limit 1
        "#,
        params![project_id],
        |row| {
            let work_unit_id = row.get(0)?;
            let title: String = row.get(1)?;
            Ok(PhaseBlocker {
                kind: "blocked_work_unit".to_string(),
                review_plan_id: None,
                work_unit_id: Some(work_unit_id),
                review_type: None,
                stage: None,
                review_run_id: None,
                finding_id: None,
                severity: None,
                classification: None,
                description: format!("work unit {work_unit_id} is blocked: {title}"),
                next_action: format!(
                    "resolve the blocker, then run agent-workbench work unblock {work_unit_id} --reason \"<reason>\""
                ),
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn finding_next_action(
    finding_id: i64,
    review_plan_id: i64,
    closure_id: Option<i64>,
    closure_status: Option<&str>,
    attempt_id: Option<i64>,
    verification: Option<(i64, Option<&str>)>,
    classification: &str,
) -> String {
    match classification {
        "unclassified" => format!(
            "agent-workbench finding classify {finding_id} --classification valid|invalid|design_conflict|needs_evidence"
        ),
        "valid" | "design_conflict" | "needs_evidence" => {
            match (closure_id, closure_status, attempt_id, verification) {
                (
                    Some(closure_id),
                    Some("ready_for_verification"),
                    Some(_),
                    Some((run_id, finding_result)),
                ) => {
                    let result = finding_result.unwrap_or("<missing-finding-result>");
                    format!(
                        "agent-workbench finding verify --run {run_id} --finding {finding_id} --closure {closure_id} --result {result}"
                    )
                }
                (Some(closure_id), Some("ready_for_verification"), Some(attempt_id), None) => {
                    let context = format!(
                        "review-context:finding-fix:finding={finding_id}:closure={closure_id}:attempt={attempt_id}"
                    );
                    format!(
                        "agent-workbench review-context finding-fix --finding {finding_id} --closure {closure_id} --attempt {attempt_id}; then agent-workbench review run add --plan {review_plan_id} --type resume --purpose finding_fix_verification --target {context} --finding-result verified|not_fixed|needs_evidence --carried-findings 1 --provenance external_agent --external-agent-id <id> --provenance-ref <ref>"
                    )
                }
                (Some(closure_id), Some("registered"), _, _) => format!(
                    "correct the finding source, then agent-workbench closure ready {closure_id} --evidence \"<evidence>\" --tests \"<tests>\""
                ),
                (Some(closure_id), Some("incomplete"), _, _) => format!(
                    "agent-workbench closure supersede {closure_id} --invariant \"<invariant>\" --surfaces \"<surfaces>\" --fix-plan \"<plan>\" --tests \"<tests>\" --verification \"<plan>\" --reason \"<reason>\" --authority <authority-event-id>"
                ),
                (None, _, _, _) => format!(
                    "agent-workbench closure add --finding {finding_id} --invariant \"<invariant>\""
                ),
                _ => format!("resolve closure state for finding {finding_id}"),
            }
        }
        _ => format!("resolve finding {finding_id}"),
    }
}

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
    let conn = open_ledger(&ledger_path)?;
    migrate_if_needed(&conn)?;
    Ok(conn)
}

fn migrate_if_needed(conn: &Connection) -> Result<()> {
    if ledger_needs_migration(conn)? {
        migrate(conn)?;
    }
    Ok(())
}

fn ledger_needs_migration(conn: &Connection) -> Result<bool> {
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

pub(crate) fn migrate(conn: &Connection) -> Result<()> {
    prepare_acceptance_records_for_schema(conn)?;
    prepare_review_runs_for_schema(conn)?;
    prepare_project_scoped_ledger_rows_for_schema(conn)?;
    drop_phase_review_target_reference_triggers(conn)?;
    conn.execute_batch(SCHEMA)?;
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
    ensure_closure_lifecycle_schema(conn)?;
    migrate_review_runs_phase_targets(conn)?;
    ensure_phase_review_target_reference_triggers(conn)?;
    conn.execute_batch(SCHEMA)?;
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

fn ensure_closure_lifecycle_schema(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "closures")? {
        return Ok(());
    }
    ensure_column(
        conn,
        "closures",
        "status",
        "text not null default 'registered'",
    )?;
    ensure_column(conn, "closures", "superseded_by_closure_id", "integer")?;
    ensure_column(conn, "closures", "superseded_at", "text")?;
    ensure_column(conn, "closures", "supersession_reason", "text")?;
    ensure_column(
        conn,
        "closures",
        "superseded_by_authority_event_id",
        "integer",
    )?;
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
    conn.execute_batch(
        r#"
        create table if not exists closure_attempts (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            closure_id integer not null references closures(id) on delete cascade,
            attempt_number integer not null,
            implementation_evidence text not null,
            tests_or_gates text not null,
            closed_by_commit text,
            review_run_high_watermark integer not null default 0,
            result text check (result in ('verified', 'not_fixed', 'needs_evidence', 'superseded')),
            created_at text not null,
            resolved_at text,
            unique(closure_id, attempt_number)
        );
        "#,
    )?;

    // Preserve verified legacy history first. For other findings, only the
    // greatest-id closure remains current.
    conn.execute_batch(
        r#"
        update closures set status = 'superseded'
        where exists (
            select 1 from finding_verifications fv
            where fv.finding_id = closures.finding_id and fv.result = 'verified'
        );

        update closures set status = 'verified'
        where id = (
            select fv.closure_id from finding_verifications fv
            where fv.finding_id = closures.finding_id and fv.result = 'verified'
            order by fv.id desc limit 1
        );

        update closures set status = 'superseded'
        where not exists (
            select 1 from finding_verifications fv
            where fv.finding_id = closures.finding_id and fv.result = 'verified'
        )
        and id != (select max(c2.id) from closures c2 where c2.finding_id = closures.finding_id);

        update findings set status = 'accepted_out_of_scope'
        where exists (
            select 1 from acceptance_records ar
            where ar.finding_id = findings.id
              and ar.target_type = 'finding'
              and ar.acceptance_type = 'accepted_out_of_scope'
              and ar.status = 'approved'
        );

        update closure_attempts set result = 'superseded', resolved_at = coalesce(resolved_at, current_timestamp)
        where result is null and closure_id in (
            select c.id from closures c
            join findings f on f.id = c.finding_id
            where f.status = 'accepted_out_of_scope'
        );

        update closures set status = 'superseded'
        where finding_id in (
            select id from findings where status = 'accepted_out_of_scope'
        );

        update findings set status = 'open'
        where classification = 'valid'
          and status = 'closed'
          and not exists (
              select 1 from finding_verifications fv
              where fv.finding_id = findings.id and fv.result = 'verified'
          )
          and not exists (
              select 1 from acceptance_records ar
              where ar.finding_id = findings.id
                and ar.target_type = 'finding'
                and ar.acceptance_type = 'accepted_out_of_scope'
                and ar.status = 'approved'
          );

        update closures
        set status = case
            when exists (
                select 1
                from findings f
                join review_runs r on r.id = f.review_run_id
                join review_plans p on p.id = r.review_plan_id
                where f.id = closures.finding_id
                  and p.required = 1
                  and p.stage = 'close-ready'
                  and p.review_type in ('implementation_review', 'design_implementation_diff')
            )
            and (
                coalesce(trim(affected_surfaces), '') = ''
                or coalesce(trim(fix_plan), '') = ''
                or coalesce(trim(tests_or_gates), '') = ''
                or coalesce(trim(verification_plan), '') = ''
            ) then 'incomplete'
            else 'registered'
        end
        where id = (select max(c2.id) from closures c2 where c2.finding_id = closures.finding_id)
          and exists (
              select 1 from findings f
              where f.id = closures.finding_id
                and f.status = 'open' and f.classification = 'valid'
          )
          and not exists (
              select 1 from closure_attempts a
              where a.closure_id = closures.id and a.result is null
          )
          and not exists (
              select 1 from finding_verifications fv
              where fv.finding_id = closures.finding_id and fv.result = 'verified'
          );

        update findings set status = 'closed'
        where exists (
            select 1 from finding_verifications fv
            where fv.finding_id = findings.id and fv.result = 'verified'
        );

        update findings set status = 'accepted_out_of_scope'
        where exists (
            select 1 from acceptance_records ar
            where ar.finding_id = findings.id
              and ar.target_type = 'finding'
              and ar.acceptance_type = 'accepted_out_of_scope'
              and ar.status = 'approved'
        );

        update findings set status = 'open'
        where classification = 'valid'
          and status = 'closed'
          and not exists (
              select 1 from finding_verifications fv
              where fv.finding_id = findings.id and fv.result = 'verified'
          )
          and not exists (
              select 1 from acceptance_records ar
              where ar.finding_id = findings.id
                and ar.target_type = 'finding'
                and ar.acceptance_type = 'accepted_out_of_scope'
                and ar.status = 'approved'
          );
        "#,
    )?;
    Ok(())
}

fn validate_review_required_links(conn: &Connection) -> Result<()> {
    if table_exists(conn, "review_plans")? {
        let missing_policy_count: i64 = conn.query_row(
            "select count(*) from review_plans where review_policy_id is null",
            [],
            |row| row.get(0),
        )?;
        if missing_policy_count > 0 {
            bail!("review_plans contains rows without review_policy_id");
        }
    }
    if table_exists(conn, "review_runs")? {
        let missing_plan_count: i64 = conn.query_row(
            "select count(*) from review_runs where review_plan_id is null",
            [],
            |row| row.get(0),
        )?;
        if missing_plan_count > 0 {
            bail!("review_runs contains rows without review_plan_id");
        }
    }
    Ok(())
}

fn refresh_review_integrity_triggers(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        drop trigger if exists trg_review_policy_referenced_update;
        drop trigger if exists trg_review_policy_resume_findings_update;
        drop trigger if exists trg_review_scope_referenced_update;
        drop trigger if exists trg_review_plan_policy_required_insert;
        drop trigger if exists trg_review_plan_policy_required_update;
        drop trigger if exists trg_review_plan_project_insert;
        drop trigger if exists trg_review_plan_project_update;
        drop trigger if exists trg_review_plan_type_insert;
        drop trigger if exists trg_review_plan_type_update;
        drop trigger if exists trg_review_plan_resume_policy_update;
        drop trigger if exists trg_review_run_plan_required_insert;
        drop trigger if exists trg_review_run_plan_required_update;
        drop trigger if exists trg_review_run_project_insert;
        drop trigger if exists trg_review_run_target_insert;
        drop trigger if exists trg_review_run_project_update;
        drop trigger if exists trg_review_run_target_update;
        drop trigger if exists trg_review_run_plan_target_insert;
        drop trigger if exists trg_review_run_plan_target_update;
        drop trigger if exists trg_review_run_type_purpose_insert;
        drop trigger if exists trg_review_run_type_purpose_update;
        drop trigger if exists trg_review_run_resume_policy_insert;
        drop trigger if exists trg_review_run_resume_policy_update;
        drop trigger if exists trg_review_run_result_insert;
        drop trigger if exists trg_review_run_result_update;
        drop trigger if exists trg_review_plan_target_project_insert;
        drop trigger if exists trg_review_plan_target_project_update;
        drop trigger if exists trg_review_plan_target_referenced_update;
        drop trigger if exists trg_review_plan_target_referenced_delete;
        drop trigger if exists trg_work_phase_review_target_referenced_update;
        drop trigger if exists trg_work_phase_review_target_referenced_delete;
        drop trigger if exists trg_finding_project_insert;
        drop trigger if exists trg_finding_project_update;
        drop trigger if exists trg_finding_clean_run_insert;
        drop trigger if exists trg_finding_clean_run_update;
        drop trigger if exists trg_finding_resume_policy_insert;
        drop trigger if exists trg_finding_resume_policy_update;
        drop trigger if exists trg_finding_review_type_insert;
        drop trigger if exists trg_finding_review_type_update;
        drop trigger if exists trg_closure_project_insert;
        drop trigger if exists trg_closure_project_update;
        drop trigger if exists trg_finding_verification_project_insert;
        drop trigger if exists trg_finding_verification_project_update;
        "#,
    )?;
    Ok(())
}

fn drop_phase_review_target_reference_triggers(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        drop trigger if exists trg_work_phase_review_target_referenced_update;
        drop trigger if exists trg_work_phase_review_target_referenced_delete;
        "#,
    )?;
    Ok(())
}

fn prepare_acceptance_records_for_schema(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "acceptance_records")? {
        return Ok(());
    }
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
    Ok(())
}

fn prepare_review_runs_for_schema(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "review_runs")? {
        return Ok(());
    }
    ensure_column(
        conn,
        "review_runs",
        "review_provenance",
        "text not null default 'self_recorded'",
    )?;
    ensure_column(conn, "review_runs", "review_provenance_ref", "text")?;
    Ok(())
}

fn prepare_project_scoped_ledger_rows_for_schema(conn: &Connection) -> Result<()> {
    if table_exists(conn, "work_records")? {
        ensure_column(conn, "work_records", "project_id", "integer")?;
        if table_exists(conn, "work_units")? {
            conn.execute(
                r#"
                update work_records
                set project_id = (select project_id from work_units where id = work_records.work_unit_id)
                where project_id is null and work_unit_id is not null
                "#,
                [],
            )?;
        }
        if table_exists(conn, "projects")? {
            conn.execute(
                r#"
                update work_records
                set project_id = (select id from projects order by id limit 1)
                where project_id is null
                "#,
                [],
            )?;
        }
    }
    if table_exists(conn, "command_usages")? {
        ensure_column(conn, "command_usages", "project_id", "integer")?;
        if table_exists(conn, "command_profiles")? {
            conn.execute(
                r#"
                update command_usages
                set project_id = (select project_id from command_profiles where id = command_usages.command_profile_id)
                where project_id is null and command_profile_id is not null
                "#,
                [],
            )?;
        }
        if table_exists(conn, "work_units")? {
            conn.execute(
                r#"
                update command_usages
                set project_id = (select project_id from work_units where id = command_usages.work_unit_id)
                where project_id is null and work_unit_id is not null
                "#,
                [],
            )?;
        }
        if table_exists(conn, "work_unit_activations")? {
            conn.execute(
                r#"
                update command_usages
                set project_id = (select project_id from work_unit_activations where id = command_usages.work_unit_activation_id)
                where project_id is null and work_unit_activation_id is not null
                "#,
                [],
            )?;
        }
        if table_exists(conn, "repository_snapshots")? && table_exists(conn, "repositories")? {
            conn.execute(
                r#"
                update command_usages
                set project_id = (
                    select r.project_id
                    from repository_snapshots s
                    join repositories r on r.id = s.repository_id
                    where s.id = command_usages.repository_snapshot_id
                )
                where project_id is null and repository_snapshot_id is not null
                "#,
                [],
            )?;
        }
        if table_exists(conn, "projects")? {
            conn.execute(
                r#"
                update command_usages
                set project_id = (select id from projects order by id limit 1)
                where project_id is null
                "#,
                [],
            )?;
        }
    }
    Ok(())
}

fn validate_project_scoped_ledger_links(conn: &Connection) -> Result<()> {
    reject_invalid_rows(
        conn,
        "work_records",
        r#"
        select count(*)
        from work_records wr
        left join work_units w on w.id = wr.work_unit_id
        where wr.project_id is null
           or not exists (select 1 from projects where id = wr.project_id)
           or (wr.work_unit_id is not null and (w.id is null or wr.project_id != w.project_id))
        "#,
        "work_records contains rows without a valid project_id",
    )?;
    reject_invalid_rows(
        conn,
        "command_usages",
        r#"
        select count(*)
        from command_usages cu
        left join command_profiles cp on cp.id = cu.command_profile_id
        left join work_units w on w.id = cu.work_unit_id
        left join work_unit_activations a on a.id = cu.work_unit_activation_id
        left join repository_snapshots s on s.id = cu.repository_snapshot_id
        left join repositories sr on sr.id = s.repository_id
        where cu.project_id is null
           or not exists (select 1 from projects where id = cu.project_id)
           or (cu.command_profile_id is not null and (cp.id is null or cu.project_id != cp.project_id))
           or (cu.work_unit_id is not null and (w.id is null or cu.project_id != w.project_id))
           or (cu.work_unit_activation_id is not null and (a.id is null or cu.project_id != a.project_id))
           or (
               cu.repository_snapshot_id is not null
               and (s.id is null or sr.id is null or cu.project_id != sr.project_id)
           )
        "#,
        "command_usages contains rows without a valid project_id",
    )?;
    reject_invalid_rows(
        conn,
        "validation_runs",
        r#"
        select count(*)
        from validation_runs vr
        left join validation_gates vg on vg.id = vr.validation_gate_id
        left join work_units w on w.id = vr.work_unit_id
        left join tasks t on t.id = vr.task_id
        left join command_usages cu on cu.id = vr.command_usage_id
        left join repository_snapshots s on s.id = vr.repository_snapshot_id
        left join repositories sr on sr.id = s.repository_id
        where vr.project_id is null
           or not exists (select 1 from projects where id = vr.project_id)
           or vg.id is null
           or vr.project_id != vg.project_id
           or vr.work_unit_id is not vg.work_unit_id
           or vr.task_id is not vg.task_id
           or (vr.work_unit_id is not null and (w.id is null or vr.project_id != w.project_id))
           or (
               vr.task_id is not null
               and (
                   t.id is null
                   or t.work_unit_id is null
                   or vr.project_id != (
                       select project_id from work_units where id = t.work_unit_id
                   )
               )
           )
           or (vr.command_usage_id is not null and (cu.id is null or vr.project_id != cu.project_id))
           or (
               vr.command_usage_id is not null
               and cu.work_unit_id is not null
               and cu.work_unit_id is not vr.work_unit_id
           )
           or (
               vr.repository_snapshot_id is not null
               and (s.id is null or sr.id is null or vr.project_id != sr.project_id)
           )
           or (
               vr.command_usage_id is not null
               and vr.repository_snapshot_id is not null
               and cu.repository_snapshot_id is not null
               and vr.repository_snapshot_id != cu.repository_snapshot_id
           )
        "#,
        "validation_runs contains invalid project links; run `agent-workbench doctor validation-links`, then `agent-workbench doctor validation-links --repair`",
    )?;
    reject_invalid_rows(
        conn,
        "artifacts",
        r#"
        select count(*)
        from artifacts a
        left join validation_runs vr on vr.id = a.validation_run_id
        left join command_usages cu on cu.id = a.command_usage_id
        left join repository_snapshots s on s.id = a.repository_snapshot_id
        left join repositories sr on sr.id = s.repository_id
        where a.project_id is null
           or not exists (select 1 from projects where id = a.project_id)
           or (a.validation_run_id is not null and (vr.id is null or a.project_id != vr.project_id))
           or (a.command_usage_id is not null and (cu.id is null or a.project_id != cu.project_id))
           or (
               a.repository_snapshot_id is not null
               and (s.id is null or sr.id is null or a.project_id != sr.project_id)
           )
           or (
               a.validation_run_id is not null
               and a.command_usage_id is not vr.command_usage_id
           )
           or (
               a.validation_run_id is not null
               and a.repository_snapshot_id is not vr.repository_snapshot_id
           )
        "#,
        "artifacts contains invalid validation links",
    )?;
    reject_invalid_rows(
        conn,
        "work_record_commands",
        r#"
        select count(*)
        from work_record_commands wrc
        left join work_records wr on wr.id = wrc.work_record_id
        left join command_usages cu on cu.id = wrc.command_usage_id
        left join command_profiles cp on cp.id = wrc.command_profile_id
        where wr.id is null
           or (wrc.command_usage_id is not null and (cu.id is null or wr.project_id != cu.project_id))
           or (wrc.command_profile_id is not null and (cp.id is null or wr.project_id != cp.project_id))
        "#,
        "work_record_commands contains cross-project links",
    )?;
    reject_invalid_rows(
        conn,
        "work_record_commits",
        r#"
        select count(*)
        from work_record_commits wrc
        left join work_records wr on wr.id = wrc.work_record_id
        left join git_commits gc on gc.id = wrc.git_commit_id
        left join repositories r on r.id = gc.repository_id
        where wr.id is null
           or (
               wrc.git_commit_id is not null
               and (
                   gc.id is null
                   or r.id is null
                   or wrc.commit_sha is null
                   or wrc.commit_sha != gc.commit_sha
                   or wr.project_id != r.project_id
               )
           )
        "#,
        "work_record_commits contains invalid git links",
    )?;
    reject_invalid_rows(
        conn,
        "work_record_files",
        r#"
        select count(*)
        from work_record_files wrf
        left join work_records wr on wr.id = wrf.work_record_id
        left join repositories r on r.id = wrf.repository_id
        left join git_file_changes gf on gf.id = wrf.git_file_change_id
        where wr.id is null
           or (wrf.repository_id is not null and (r.id is null or wr.project_id != r.project_id))
           or (
               wrf.git_file_change_id is not null
               and (
                   gf.id is null
                   or wrf.repository_id is null
                   or wrf.repository_id != gf.repository_id
                   or wrf.path != gf.path
               )
           )
        "#,
        "work_record_files contains invalid repository links",
    )?;
    reject_invalid_rows(
        conn,
        "work_record_forks",
        r#"
        select count(*)
        from work_record_forks f
        left join work_units forked on forked.id = f.forked_work_unit_id
        left join work_units source_w on source_w.id = f.source_work_unit_id
        left join work_unit_activations source_a on source_a.id = f.source_work_unit_activation_id
        left join work_records source_r on source_r.id = f.source_work_record_id
        left join repository_snapshots source_s on source_s.id = f.source_repository_snapshot_id
        left join repositories source_sr on source_sr.id = source_s.repository_id
        left join git_commits source_gc on source_gc.id = f.source_git_commit_id
        left join repositories source_gr on source_gr.id = source_gc.repository_id
        where f.project_id is null
           or forked.id is null
           or f.project_id != forked.project_id
           or (f.source_work_unit_id is not null and (source_w.id is null or f.project_id != source_w.project_id))
           or (f.source_work_unit_activation_id is not null and (source_a.id is null or f.project_id != source_a.project_id))
           or (f.source_work_record_id is not null and (source_r.id is null or f.project_id != source_r.project_id))
           or (
               f.source_repository_snapshot_id is not null
               and (source_s.id is null or source_sr.id is null or f.project_id != source_sr.project_id)
           )
           or (
               f.source_git_commit_id is not null
               and (
                   source_gc.id is null
                   or source_gr.id is null
                   or f.project_id != source_gr.project_id
                   or (f.source_git_commit_sha is not null and f.source_git_commit_sha != source_gc.commit_sha)
               )
           )
        "#,
        "work_record_forks contains invalid project links",
    )?;
    Ok(())
}

fn reject_invalid_rows(
    conn: &Connection,
    table: &str,
    sql: &str,
    message: &'static str,
) -> Result<()> {
    if !table_exists(conn, table)? {
        return Ok(());
    }
    let count: i64 = conn.query_row(sql, [], |row| row.get(0))?;
    if count > 0 {
        bail!("{message}");
    }
    Ok(())
}

fn migrate_work_record_auto_link_markers(
    conn: &Connection,
    had_work_record_commit_auto_linked: bool,
    had_work_record_file_auto_linked: bool,
    had_work_record_file_repository_auto_linked: bool,
) -> Result<()> {
    if !had_work_record_commit_auto_linked {
        conn.execute(
            r#"
            update work_record_commits
            set auto_linked = 1
            where git_commit_id is not null
            "#,
            [],
        )?;
    }
    if !had_work_record_file_auto_linked {
        conn.execute(
            r#"
            update work_record_files
            set auto_linked = 1
            where git_file_change_id is not null
            "#,
            [],
        )?;
    }
    if !had_work_record_file_repository_auto_linked {
        conn.execute(
            r#"
            update work_record_files
            set repository_auto_linked = 1
            where git_file_change_id is not null
              and ?1 = 0
            "#,
            params![i64::from(had_work_record_file_auto_linked)],
        )?;
    }

    Ok(())
}

fn refresh_ledger_integrity_triggers(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        drop trigger if exists trg_command_usage_project_insert;
        drop trigger if exists trg_command_usage_project_update;
        drop trigger if exists trg_command_usage_repository_snapshot_insert;
        drop trigger if exists trg_command_usage_repository_snapshot_update;
        drop trigger if exists trg_work_record_project_insert;
        drop trigger if exists trg_work_record_project_update;
        drop trigger if exists trg_work_record_command_project_insert;
        drop trigger if exists trg_work_record_command_project_update;
        drop trigger if exists trg_work_record_commit_git_insert;
        drop trigger if exists trg_work_record_commit_git_update;
        drop trigger if exists trg_work_record_file_git_insert;
        drop trigger if exists trg_work_record_file_git_update;
        drop trigger if exists trg_work_record_fork_repository_git_insert;
        drop trigger if exists trg_work_record_fork_repository_git_update;
        drop trigger if exists trg_implementation_evidence_project_insert;
        drop trigger if exists trg_implementation_evidence_project_update;
        drop trigger if exists trg_validation_run_project_insert;
        drop trigger if exists trg_validation_run_project_update;
        drop trigger if exists trg_artifact_project_insert;
        drop trigger if exists trg_artifact_project_update;
        drop trigger if exists trg_repository_snapshot_referenced_delete;
        drop trigger if exists trg_acceptance_design_requirement_project_insert;
        drop trigger if exists trg_acceptance_design_requirement_project_update;
        drop trigger if exists trg_acceptance_task_project_insert;
        drop trigger if exists trg_acceptance_task_project_update;
        drop trigger if exists trg_acceptance_validation_gate_template_project_insert;
        drop trigger if exists trg_acceptance_validation_gate_template_project_update;
        drop trigger if exists trg_acceptance_coverage_item_project_insert;
        drop trigger if exists trg_acceptance_coverage_item_project_update;
        drop trigger if exists trg_acceptance_general_project_insert;
        drop trigger if exists trg_acceptance_general_project_update;
        drop trigger if exists trg_repository_state_classification_acceptance_insert;
        drop trigger if exists trg_repository_state_classification_acceptance_update;
        "#,
    )?;
    Ok(())
}

fn migrate_acceptance_records(conn: &Connection) -> Result<()> {
    let table_sql = conn
        .query_row(
            "select sql from sqlite_schema where type = 'table' and name = 'acceptance_records'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(table_sql) = table_sql else {
        return Ok(());
    };
    if acceptance_records_schema_current(conn, &table_sql)? {
        return Ok(());
    }

    conn.execute_batch(
        r#"
        drop trigger if exists trg_acceptance_design_requirement_project_insert;
        drop trigger if exists trg_acceptance_design_requirement_project_update;
        drop trigger if exists trg_acceptance_task_project_insert;
        drop trigger if exists trg_acceptance_task_project_update;
        drop trigger if exists trg_acceptance_validation_gate_template_project_insert;
        drop trigger if exists trg_acceptance_validation_gate_template_project_update;
        drop trigger if exists trg_acceptance_coverage_item_project_insert;
        drop trigger if exists trg_acceptance_coverage_item_project_update;
        drop trigger if exists trg_acceptance_general_project_insert;
        drop trigger if exists trg_acceptance_general_project_update;
        drop trigger if exists trg_repository_state_classification_acceptance_insert;
        drop trigger if exists trg_repository_state_classification_acceptance_update;
        pragma legacy_alter_table = on;
        alter table acceptance_records rename to acceptance_records_old;
        pragma legacy_alter_table = off;

        create table acceptance_records (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            target_type text not null check (target_type in (
                'task', 'design_requirement', 'validation_gate_template', 'design_file',
                'design_requirement_key', 'coverage_item', 'finding', 'validation_gate',
                'validation_run', 'repository_state_classification',
                'repository_snapshot_comparison', 'review_plan', 'checklist_item',
                'command_profile', 'command_usage', 'command_deviation',
                'rule_binding', 'stale_record'
            )),
            task_id integer references tasks(id),
            design_requirement_id integer references design_requirements(id),
            validation_gate_template_id integer references validation_gate_templates(id),
            coverage_item_id integer references coverage_items(id),
            finding_id integer references findings(id),
            validation_gate_id integer references validation_gates(id),
            validation_run_id integer references validation_runs(id),
            repository_state_classification_id integer references repository_state_classifications(id),
            repository_snapshot_comparison_id integer references repository_snapshot_comparisons(id),
            review_plan_id integer references review_plans(id),
            checklist_item_id integer references checklist_items(id),
            command_profile_id integer references command_profiles(id),
            command_usage_id integer references command_usages(id),
            command_deviation_id integer references command_deviations(id),
            rule_binding_id integer references rule_bindings(id),
            stale_record_type text,
            stale_record_id integer,
            design_package_key text,
            design_file_path text,
            design_requirement_key text,
            acceptance_type text not null check (acceptance_type in (
                'accepted_out_of_scope', 'explicit_exception', 'evidence_gap',
                'classified_failure', 'stale_accepted'
            )),
            reason text not null,
            scope text,
            created_by text not null check (created_by in ('user', 'agent', 'system')),
            status text not null check (status in ('proposed', 'approved', 'rejected', 'expired')),
            approved_by_authority_event_id integer references authority_events(id),
            approved_at text,
            created_at text not null,
            review_impact text,
            check (
                (
                    (case when task_id is not null then 1 else 0 end) +
                    (case when design_requirement_id is not null then 1 else 0 end) +
                    (case when validation_gate_template_id is not null then 1 else 0 end) +
                    (case when coverage_item_id is not null then 1 else 0 end) +
                    (case when finding_id is not null then 1 else 0 end) +
                    (case when validation_gate_id is not null then 1 else 0 end) +
                    (case when validation_run_id is not null then 1 else 0 end) +
                    (case when repository_state_classification_id is not null then 1 else 0 end) +
                    (case when repository_snapshot_comparison_id is not null then 1 else 0 end) +
                    (case when review_plan_id is not null then 1 else 0 end) +
                    (case when checklist_item_id is not null then 1 else 0 end) +
                    (case when command_profile_id is not null then 1 else 0 end) +
                    (case when command_usage_id is not null then 1 else 0 end) +
                    (case when command_deviation_id is not null then 1 else 0 end) +
                    (case when rule_binding_id is not null then 1 else 0 end) +
                    (case when design_package_key is not null and design_file_path is not null and design_requirement_key is null then 1 else 0 end) +
                    (case when design_package_key is not null and design_requirement_key is not null and design_file_path is null then 1 else 0 end) +
                    (case when stale_record_type is not null and stale_record_id is not null then 1 else 0 end)
                ) = 1
                and (
                    (target_type = 'task' and task_id is not null)
                    or (target_type = 'design_requirement' and design_requirement_id is not null)
                    or (target_type = 'validation_gate_template' and validation_gate_template_id is not null)
                    or (target_type = 'coverage_item' and coverage_item_id is not null)
                    or (target_type = 'finding' and finding_id is not null)
                    or (target_type = 'validation_gate' and validation_gate_id is not null)
                    or (target_type = 'validation_run' and validation_run_id is not null)
                    or (target_type = 'repository_state_classification' and repository_state_classification_id is not null)
                    or (target_type = 'repository_snapshot_comparison' and repository_snapshot_comparison_id is not null)
                    or (target_type = 'review_plan' and review_plan_id is not null)
                    or (target_type = 'checklist_item' and checklist_item_id is not null)
                    or (target_type = 'command_profile' and command_profile_id is not null)
                    or (target_type = 'command_usage' and command_usage_id is not null)
                    or (target_type = 'command_deviation' and command_deviation_id is not null)
                    or (target_type = 'rule_binding' and rule_binding_id is not null)
                    or (target_type = 'design_file' and design_package_key is not null and design_file_path is not null)
                    or (target_type = 'design_requirement_key' and design_package_key is not null and design_requirement_key is not null)
                    or (target_type = 'stale_record' and stale_record_type is not null and stale_record_id is not null)
                )
            )
        );

        insert into acceptance_records(
            id, project_id, target_type, task_id, design_requirement_id,
            validation_gate_template_id, coverage_item_id, finding_id, validation_gate_id,
            validation_run_id, repository_state_classification_id,
            repository_snapshot_comparison_id, review_plan_id, checklist_item_id,
            command_profile_id, command_usage_id, command_deviation_id,
            rule_binding_id, stale_record_type, stale_record_id, design_package_key,
            design_file_path, design_requirement_key,
            acceptance_type, reason, scope, created_by, status,
            approved_by_authority_event_id, approved_at, created_at, review_impact
        )
        select
            id, project_id, target_type, task_id, design_requirement_id,
            validation_gate_template_id, coverage_item_id, finding_id, validation_gate_id,
            validation_run_id, repository_state_classification_id,
            repository_snapshot_comparison_id, review_plan_id, checklist_item_id,
            command_profile_id, command_usage_id, command_deviation_id, null,
            stale_record_type, stale_record_id, design_package_key, design_file_path,
            design_requirement_key,
            acceptance_type, reason, scope,
            case
                when created_by in ('user', 'agent', 'system') then created_by
                else 'system'
            end,
            case
                when status in ('proposed', 'approved', 'rejected', 'expired') then status
                when status = 'revoked' then 'rejected'
                else 'approved'
            end,
            approved_by_authority_event_id, approved_at,
            created_at, review_impact
        from acceptance_records_old;

        drop table acceptance_records_old;
        "#,
    )?;
    Ok(())
}

fn acceptance_records_needs_migration(conn: &Connection) -> Result<bool> {
    let table_sql = conn
        .query_row(
            "select sql from sqlite_schema where type = 'table' and name = 'acceptance_records'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(table_sql) = table_sql else {
        return Ok(false);
    };
    Ok(!acceptance_records_schema_current(conn, &table_sql)?)
}

fn acceptance_records_schema_current(conn: &Connection, table_sql: &str) -> Result<bool> {
    Ok(table_sql.contains("'design_file'")
        && table_sql.contains("'coverage_item'")
        && table_sql.contains("'design_requirement_key'")
        && table_sql.contains("'validation_run'")
        && table_sql.contains("'rule_binding'")
        && table_sql.contains("'evidence_gap'")
        && table_has_column(conn, "acceptance_records", "design_package_key")?
        && table_has_column(conn, "acceptance_records", "design_file_path")?
        && table_has_column(conn, "acceptance_records", "design_requirement_key")?
        && table_has_column(conn, "acceptance_records", "coverage_item_id")?
        && table_has_column(conn, "acceptance_records", "validation_run_id")?
        && table_has_column(conn, "acceptance_records", "rule_binding_id")?)
}

fn repair_acceptance_record_references(conn: &Connection) -> Result<()> {
    let broken_reference_count: i64 = conn.query_row(
        r#"
        select count(*)
        from sqlite_schema
        where sql like '%acceptance_records_old%'
        "#,
        [],
        |row| row.get(0),
    )?;
    if broken_reference_count == 0 {
        return Ok(());
    }

    let schema_version: i64 = conn.pragma_query_value(None, "schema_version", |row| row.get(0))?;
    conn.execute_batch(
        r#"
        pragma writable_schema = on;
        update sqlite_schema
        set sql = replace(replace(sql, '"acceptance_records_old"', 'acceptance_records'), 'acceptance_records_old', 'acceptance_records')
        where sql like '%acceptance_records_old%';
        pragma writable_schema = off;
        "#,
    )?;
    conn.pragma_update(None, "schema_version", schema_version + 1)?;
    Ok(())
}

fn migrate_repository_snapshot_comparisons(conn: &Connection) -> Result<()> {
    let table_sql = conn
        .query_row(
            "select sql from sqlite_schema where type = 'table' and name = 'repository_snapshot_comparisons'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(table_sql) = table_sql else {
        return Ok(());
    };
    if table_sql.contains("'review'") && !table_sql.contains("'inspection'") {
        return Ok(());
    }

    conn.pragma_update(None, "foreign_keys", false)?;
    conn.execute_batch(
        r#"
        create table repository_snapshot_comparisons_new (
            id integer primary key,
            base_repository_snapshot_id integer not null references repository_snapshots(id) on delete cascade,
            current_repository_snapshot_id integer not null references repository_snapshots(id) on delete cascade,
            comparison_type text not null check (comparison_type in ('resume', 'close', 'validation', 'review')),
            head_changed integer not null check (head_changed in (0, 1)),
            dirty_state_changed integer not null check (dirty_state_changed in (0, 1)),
            nested_repository_changed integer not null default 0 check (nested_repository_changed in (0, 1)),
            result text not null check (result in ('same', 'changed_classified', 'changed_unclassified')),
            created_at text not null
        );

        insert into repository_snapshot_comparisons_new(
            id, base_repository_snapshot_id, current_repository_snapshot_id,
            comparison_type, head_changed, dirty_state_changed,
            nested_repository_changed, result, created_at
        )
        select
            id, base_repository_snapshot_id, current_repository_snapshot_id,
            case when comparison_type = 'inspection' then 'review' else comparison_type end,
            head_changed, dirty_state_changed, nested_repository_changed, result, created_at
        from repository_snapshot_comparisons;

        drop table repository_snapshot_comparisons;
        alter table repository_snapshot_comparisons_new rename to repository_snapshot_comparisons;
        "#,
    )?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(())
}

fn migrate_kpt_items(conn: &Connection) -> Result<()> {
    let table_sql = conn
        .query_row(
            "select sql from sqlite_schema where type = 'table' and name = 'kpt_items'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(table_sql) = table_sql else {
        return Ok(());
    };
    let conversion_sql = conn
        .query_row(
            "select sql from sqlite_schema where type = 'table' and name = 'kpt_item_conversions'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let conversion_sql = conversion_sql.unwrap_or_default();
    if table_sql.contains("'converted'")
        && table_sql.contains("linked_review_finding_id integer references findings")
        && conversion_sql.contains("review_policy_id integer references review_policies")
        && conversion_sql.contains("design_version_id integer references design_versions")
        && conversion_sql.contains("target_type = 'task'")
    {
        return Ok(());
    }

    conn.execute_batch(
        r#"
        pragma foreign_keys = off;

        alter table kpt_item_conversions rename to kpt_item_conversions_old;
        alter table kpt_items rename to kpt_items_old;

        create table kpt_items (
            id integer primary key,
            kpt_review_id integer not null references kpt_reviews(id) on delete cascade,
            item_type text not null check (item_type in ('keep', 'problem', 'try')),
            title text not null,
            details text,
            severity text not null default 'medium' check (severity in ('critical', 'high', 'medium', 'low')),
            linked_user_correction_id integer references user_corrections(id),
            linked_command_profile_id integer references command_profiles(id),
            linked_review_finding_id integer references findings(id),
            linked_task_id integer references tasks(id),
            proposed_action text,
            status text not null default 'open' check (status in ('open', 'accepted', 'converted', 'converted_to_task', 'dismissed')),
            created_at text not null
        );

        insert into kpt_items(
            id, kpt_review_id, item_type, title, details, severity,
            linked_user_correction_id, linked_command_profile_id,
            linked_review_finding_id, linked_task_id, proposed_action, status, created_at
        )
        select
            id, kpt_review_id, item_type, title, details, severity,
            linked_user_correction_id, linked_command_profile_id,
            linked_review_finding_id, linked_task_id, proposed_action, status, created_at
        from kpt_items_old;

        create table kpt_item_conversions (
            id integer primary key,
            kpt_item_id integer not null references kpt_items(id) on delete cascade,
            target_type text not null check (target_type in ('task', 'command_profile', 'review_policy', 'design_version', 'decision', 'user_correction')),
            task_id integer references tasks(id),
            command_profile_id integer references command_profiles(id),
            review_policy_id integer references review_policies(id),
            design_version_id integer references design_versions(id),
            decision_id integer references decisions(id),
            user_correction_id integer references user_corrections(id),
            created_at text not null,
            check (
                (target_type = 'task' and task_id is not null and command_profile_id is null and review_policy_id is null and design_version_id is null and decision_id is null and user_correction_id is null)
                or (target_type = 'command_profile' and task_id is null and command_profile_id is not null and review_policy_id is null and design_version_id is null and decision_id is null and user_correction_id is null)
                or (target_type = 'review_policy' and task_id is null and command_profile_id is null and review_policy_id is not null and design_version_id is null and decision_id is null and user_correction_id is null)
                or (target_type = 'design_version' and task_id is null and command_profile_id is null and review_policy_id is null and design_version_id is not null and decision_id is null and user_correction_id is null)
                or (target_type = 'decision' and task_id is null and command_profile_id is null and review_policy_id is null and design_version_id is null and decision_id is not null and user_correction_id is null)
                or (target_type = 'user_correction' and task_id is null and command_profile_id is null and review_policy_id is null and design_version_id is null and decision_id is null and user_correction_id is not null)
            )
        );

        insert into kpt_item_conversions(
            id, kpt_item_id, target_type, task_id, command_profile_id,
            review_policy_id, design_version_id, decision_id, user_correction_id, created_at
        )
        select
            id, kpt_item_id, target_type, task_id, command_profile_id,
            review_policy_id, design_version_id, decision_id, user_correction_id, created_at
        from kpt_item_conversions_old;

        drop table kpt_item_conversions_old;
        drop table kpt_items_old;

        pragma foreign_keys = on;
        "#,
    )?;

    Ok(())
}

fn migrate_review_runs(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "review_runs")? {
        return Ok(());
    }

    let invalid_count: i64 = conn.query_row(
        r#"
        select count(*)
        from review_runs
        where not (
            (run_type = 'fresh' and run_purpose = 'new_unbiased_review')
            or (run_type = 'resume' and run_purpose = 'finding_fix_verification')
            or (run_type = 'coverage' and run_purpose = 'coverage_audit')
        )
        "#,
        [],
        |row| row.get(0),
    )?;
    if invalid_count > 0 {
        bail!("review_runs contains invalid run_type/run_purpose combinations");
    }
    conn.execute_batch(
        r#"
        create trigger if not exists trg_review_run_type_purpose_insert
        before insert on review_runs
        for each row
        when not (
            (new.run_type = 'fresh' and new.run_purpose = 'new_unbiased_review')
            or (new.run_type = 'resume' and new.run_purpose = 'finding_fix_verification')
            or (new.run_type = 'coverage' and new.run_purpose = 'coverage_audit')
        )
        begin
            select raise(abort, 'review run type must match purpose');
        end;

        create trigger if not exists trg_review_run_type_purpose_update
        before update of run_type, run_purpose on review_runs
        for each row
        when not (
            (new.run_type = 'fresh' and new.run_purpose = 'new_unbiased_review')
            or (new.run_type = 'resume' and new.run_purpose = 'finding_fix_verification')
            or (new.run_type = 'coverage' and new.run_purpose = 'coverage_audit')
        )
        begin
            select raise(abort, 'review run type must match purpose');
        end;
        "#,
    )?;

    Ok(())
}

fn migrate_review_runs_phase_targets(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "review_runs")? {
        return Ok(());
    }

    ensure_column(conn, "review_runs", "file_path", "text")?;
    ensure_column(conn, "review_runs", "symbol", "text")?;
    ensure_column(
        conn,
        "review_runs",
        "review_provenance",
        "text not null default 'self_recorded'",
    )?;
    ensure_column(conn, "review_runs", "review_provenance_ref", "text")?;
    ensure_column(conn, "review_runs", "phase_id", "integer")?;

    let table_sql: String = conn.query_row(
        "select sql from sqlite_schema where type = 'table' and name = 'review_runs'",
        [],
        |row| row.get(0),
    )?;
    if table_sql.contains("'phase'") {
        return Ok(());
    }

    conn.execute_batch(
        r#"
        pragma foreign_keys = off;

        create table review_runs_new (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            review_scope_id integer references review_scopes(id),
            review_plan_id integer not null references review_plans(id),
            run_type text not null check (run_type in ('fresh', 'resume', 'coverage')),
            run_purpose text not null check (run_purpose in ('new_unbiased_review', 'finding_fix_verification', 'coverage_audit')),
            target_type text not null check (target_type in ('design_version', 'design_requirement', 'task', 'work_unit', 'phase', 'repository_snapshot', 'file', 'symbol')),
            design_version_id integer references design_versions(id),
            design_requirement_id integer references design_requirements(id),
            task_id integer references tasks(id),
            work_unit_id integer references work_units(id),
            phase_id integer references work_phases(id),
            repository_snapshot_id integer,
            file_path text,
            symbol text,
            target_ref text,
            prompt_deviations text,
            result_summary text,
            new_findings_count integer not null default 0 check (new_findings_count >= 0),
            carried_findings_checked integer not null default 0 check (carried_findings_checked >= 0),
            clean_run integer not null default 0 check (clean_run in (0, 1)),
            review_provenance text not null default 'self_recorded' check (review_provenance in ('self_recorded', 'external_agent', 'human_review')),
            review_provenance_ref text,
            status text not null default 'requested' check (status in ('requested', 'running', 'completed', 'failed', 'cancelled')),
            created_at text not null,
            check (
                (run_type = 'fresh' and run_purpose = 'new_unbiased_review')
                or (run_type = 'resume' and run_purpose = 'finding_fix_verification')
                or (run_type = 'coverage' and run_purpose = 'coverage_audit')
            ),
            check (
                clean_run = 0
                or (status = 'completed' and new_findings_count = 0)
            )
        );

        insert into review_runs_new(
            id, project_id, review_scope_id, review_plan_id, run_type, run_purpose,
            target_type, design_version_id, design_requirement_id, task_id,
            work_unit_id, phase_id, repository_snapshot_id, file_path, symbol,
            target_ref, prompt_deviations, result_summary, new_findings_count,
            carried_findings_checked, clean_run, review_provenance,
            review_provenance_ref, status, created_at
        )
        select
            id, project_id, review_scope_id, review_plan_id, run_type, run_purpose,
            target_type, design_version_id, design_requirement_id, task_id,
            work_unit_id, phase_id, repository_snapshot_id, file_path, symbol,
            target_ref, prompt_deviations, result_summary, new_findings_count,
            carried_findings_checked, clean_run, review_provenance,
            review_provenance_ref, status, created_at
        from review_runs;

        drop table review_runs;
        alter table review_runs_new rename to review_runs;

        pragma foreign_keys = on;
        "#,
    )?;

    Ok(())
}

fn migrate_resume_check_items(conn: &Connection) -> Result<()> {
    let table_sql = conn
        .query_row(
            "select sql from sqlite_schema where type = 'table' and name = 'resume_check_items'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(table_sql) = table_sql else {
        return Ok(());
    };
    if table_sql.contains("'active_tasks_current'")
        && table_sql.contains("'repository_heads_current'")
    {
        return Ok(());
    }

    conn.execute_batch(
        r#"
        pragma foreign_keys = off;

        alter table resume_check_items rename to resume_check_items_old;

        create table resume_check_items (
            id integer primary key,
            resume_check_id integer not null references resume_checks(id) on delete cascade,
            check_name text not null check (check_name in ('resume_target_suspended', 'snapshot_exists', 'suspend_reason_exists', 'next_action_exists', 'deeper_frames_closed', 'blocking_dependencies_clear', 'active_tasks_current', 'authority_refs_current', 'review_scope_refs_current', 'design_version_current', 'task_derivation_current', 'checklist_current', 'selected_gate_current', 'review_plan_current', 'open_findings_current', 'repository_heads_current', 'repository_state_current', 'assumptions_current')),
            result text not null check (result in ('pass', 'fail', 'not_checked', 'needs_evidence')),
            evidence_ref text,
            blocking_action text,
            details text
        );

        insert into resume_check_items(
            id, resume_check_id, check_name, result, evidence_ref, blocking_action, details
        )
        select id, resume_check_id, check_name, result, evidence_ref, blocking_action, details
        from resume_check_items_old;

        drop table resume_check_items_old;

        pragma foreign_keys = on;
        "#,
    )?;

    Ok(())
}

fn ensure_phase_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(PHASE_SCHEMA)?;
    Ok(())
}

fn ensure_phase_review_target_reference_triggers(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "work_phase_review_targets")? || !table_exists(conn, "review_runs")? {
        return Ok(());
    }
    conn.execute_batch(
        r#"
        drop trigger if exists trg_work_phase_review_target_referenced_update;
        drop trigger if exists trg_work_phase_review_target_referenced_delete;

        create trigger trg_work_phase_review_target_referenced_update
        before update of review_plan_id, phase_id on work_phase_review_targets
        for each row
        when exists (
            select 1
            from review_runs r
            where r.review_plan_id = old.review_plan_id
              and r.target_type = 'phase'
              and r.phase_id = old.phase_id
        )
        begin
            select raise(abort, 'work phase review target is referenced by review runs');
        end;

        create trigger trg_work_phase_review_target_referenced_delete
        before delete on work_phase_review_targets
        for each row
        when exists (
            select 1
            from review_runs r
            where r.review_plan_id = old.review_plan_id
              and r.target_type = 'phase'
              and r.phase_id = old.phase_id
        )
        begin
            select raise(abort, 'work phase review target is referenced by review runs');
        end;
        "#,
    )?;
    Ok(())
}

fn ensure_column(conn: &Connection, table: &str, column: &str, definition: &str) -> Result<()> {
    if table_has_column(conn, table, column)? {
        return Ok(());
    }

    conn.execute(
        &format!("alter table {table} add column {column} {definition}"),
        [],
    )?;
    Ok(())
}

const PHASE_SCHEMA: &str = r#"
create table if not exists work_phases (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    work_unit_id integer not null references work_units(id) on delete cascade,
    phase_work_unit_id integer references work_units(id) on delete set null,
    design_version_id integer references design_versions(id) on delete set null,
    phase_key text not null,
    title text not null,
    kind text not null,
    phase_order integer not null,
    status text not null default 'open' check (status in ('open', 'blocked', 'closed', 'accepted_out_of_scope', 'split')),
    reason text,
    authority_event_id integer references authority_events(id),
    created_at text not null,
    closed_at text,
    close_summary text,
    unique(project_id, work_unit_id, phase_key)
);

create table if not exists work_phase_task_memberships (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    phase_id integer not null references work_phases(id) on delete cascade,
    task_id integer not null references tasks(id) on delete cascade,
    assigned_at text not null,
    unique(task_id)
);

create table if not exists work_phase_dependencies (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    from_phase_id integer not null references work_phases(id) on delete cascade,
    to_phase_id integer not null references work_phases(id) on delete cascade,
    dependency_type text not null check (dependency_type in ('blocks', 'requires')),
    reason text not null,
    status text not null default 'open' check (status in ('open', 'satisfied', 'accepted')),
    evidence_ref text,
    authority_event_id integer references authority_events(id),
    created_at text not null,
    resolved_at text
);

create table if not exists work_phase_trace_decisions (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    phase_id integer not null references work_phases(id) on delete cascade,
    record_type text not null check (record_type in (
        'task', 'task_derivation', 'checklist_item', 'validation_gate',
        'coverage_item', 'implementation_evidence', 'review_plan',
        'rule_binding', 'work_record'
    )),
    record_id integer not null,
    decision text not null check (decision in ('split', 'carry', 'accept')),
    reason text not null,
    authority_event_id integer not null references authority_events(id),
    created_at text not null,
    unique(phase_id, record_type, record_id)
);

create table if not exists work_phase_review_targets (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    review_plan_id integer not null references review_plans(id) on delete cascade,
    phase_id integer not null references work_phases(id) on delete cascade,
    created_at text not null,
    unique(review_plan_id, phase_id)
);

create table if not exists work_phase_events (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    phase_id integer not null references work_phases(id) on delete cascade,
    event_type text not null check (event_type in (
        'created', 'assigned', 'dependency_added', 'dependency_satisfied',
        'dependency_accepted', 'trace_decided', 'rescope_dry_run',
        'rescoped', 'split', 'closed', 'accepted_out_of_scope'
    )),
    reason text,
    authority_event_id integer references authority_events(id),
    related_task_id integer references tasks(id),
    related_work_unit_id integer references work_units(id),
    previous_status text,
    next_status text,
    created_at text not null
);

create trigger if not exists trg_work_phase_work_unit_project_insert
before insert on work_phases
for each row
when new.project_id != (select project_id from work_units where id = new.work_unit_id)
  or (
      new.phase_work_unit_id is not null
      and new.project_id != (select project_id from work_units where id = new.phase_work_unit_id)
  )
begin
    select raise(abort, 'work phase work units must match project_id');
end;

create trigger if not exists trg_work_phase_membership_project_insert
before insert on work_phase_task_memberships
for each row
when new.project_id != (select project_id from work_phases where id = new.phase_id)
  or new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      new.project_id
  )
begin
    select raise(abort, 'work phase task membership must match project_id');
end;

create trigger if not exists trg_work_phase_dependency_project_insert
before insert on work_phase_dependencies
for each row
when new.project_id != (select project_id from work_phases where id = new.from_phase_id)
  or new.project_id != (select project_id from work_phases where id = new.to_phase_id)
  or (select work_unit_id from work_phases where id = new.from_phase_id)
      != (select work_unit_id from work_phases where id = new.to_phase_id)
begin
    select raise(abort, 'work phase dependency phases must share project and aggregate work unit');
end;

create trigger if not exists trg_work_phase_review_target_project_insert
before insert on work_phase_review_targets
for each row
when new.project_id != (select project_id from review_plans where id = new.review_plan_id)
  or new.project_id != (select project_id from work_phases where id = new.phase_id)
begin
    select raise(abort, 'work phase review target must match project_id');
end;
"#;

fn table_has_column(conn: &Connection, table: &str, column: &str) -> Result<bool> {
    let mut stmt = conn.prepare(&format!("pragma table_info({table})"))?;
    let columns = stmt.query_map([], |row| row.get::<_, String>(1))?;
    for existing in columns {
        if existing? == column {
            return Ok(true);
        }
    }
    Ok(false)
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool> {
    let exists = conn
        .query_row(
            "select 1 from sqlite_schema where type = 'table' and name = ?1",
            params![table],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok(exists)
}

fn ensure_project(conn: &Connection, root: &Path) -> Result<()> {
    let root_path = root
        .canonicalize()
        .unwrap_or_else(|_| root.to_path_buf())
        .display()
        .to_string();
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project");

    conn.execute(
        r#"
        insert into projects(name, root_path, created_at, updated_at)
        select ?1, ?2, current_timestamp, current_timestamp
        where not exists (select 1 from projects where root_path = ?2)
        "#,
        params![name, root_path],
    )?;

    Ok(())
}

fn sync_agents_md_authority(conn: &Connection, root: &Path) -> Result<()> {
    let agents_path = root.join("AGENTS.md");
    if !agents_path.exists() {
        return Ok(());
    }
    let project_id = project_id(conn)?;
    let source = "AGENTS.md";
    let summary = fs::read_to_string(&agents_path)
        .with_context(|| format!("failed to read {}", agents_path.display()))?;
    let authority_id = ensure_authority_row(
        conn,
        project_id,
        source,
        "policy",
        Some("project"),
        70,
        &summary,
    )?;
    let authority_event_id = conn
        .query_row(
            r#"
            select id
            from authority_events
            where project_id = ?1
              and event_type = 'agents'
              and source = ?2
              and status = 'active'
            order by id desc
            limit 1
            "#,
            params![project_id, source],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let authority_event_id = match authority_event_id {
        Some(id) => {
            conn.execute(
                r#"
                update authority_events
                set authority_id = ?1, text_or_summary = ?2
                where id = ?3
                "#,
                params![authority_id, summary, id],
            )?;
            id
        }
        None => {
            conn.execute(
                r#"
                insert into authority_events(
                    project_id, authority_id, event_type, source, text_or_summary, scope,
                    precedence, status, created_at
                )
                values (?1, ?2, 'agents', ?3, ?4, 'project', 70, 'active', current_timestamp)
                "#,
                params![project_id, authority_id, source, summary],
            )?;
            conn.last_insert_rowid()
        }
    };
    conn.execute(
        r#"
        insert into rule_bindings(
            project_id, rule_source_type, authority_event_id, scope_type, scope_key,
            precedence, status, created_at
        )
        select ?1, 'authority_event', ?2, 'project', 'project', 70, 'active', current_timestamp
        where not exists (
            select 1
            from rule_bindings
            where project_id = ?1
              and authority_event_id = ?2
              and status = 'active'
        )
        "#,
        params![project_id, authority_event_id],
    )?;
    Ok(())
}

fn sync_commit_message_policy(conn: &Connection) -> Result<()> {
    let project_id = project_id(conn)?;
    let source = "agent-workbench:commit-message";
    let summary = "Commit subjects must use `prefix: message` and must not contain internal milestone names or the literal review token.";
    let authority_id = ensure_authority_row(
        conn,
        project_id,
        source,
        "policy",
        Some("project"),
        75,
        summary,
    )?;
    let authority_event_id = conn
        .query_row(
            r#"
            select id
            from authority_events
            where project_id = ?1
              and event_type = 'policy'
              and source = ?2
              and status = 'active'
            order by id desc
            limit 1
            "#,
            params![project_id, source],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let authority_event_id = match authority_event_id {
        Some(id) => {
            conn.execute(
                "update authority_events set authority_id = ?1, text_or_summary = ?2 where id = ?3",
                params![authority_id, summary, id],
            )?;
            id
        }
        None => {
            conn.execute(
                r#"
                insert into authority_events(
                    project_id, authority_id, event_type, source, text_or_summary, scope,
                    precedence, status, created_at
                )
                values (?1, ?2, 'policy', ?3, ?4, 'project', 75, 'active', current_timestamp)
                "#,
                params![project_id, authority_id, source, summary],
            )?;
            conn.last_insert_rowid()
        }
    };
    conn.execute(
        r#"
        insert into rule_bindings(
            project_id, rule_source_type, authority_event_id, scope_type, scope_key,
            precedence, status, created_at
        )
        select ?1, 'authority_event', ?2, 'project', 'project', 75, 'active', current_timestamp
        where not exists (
            select 1
            from rule_bindings
            where project_id = ?1
              and authority_event_id = ?2
              and status = 'active'
        )
        "#,
        params![project_id, authority_event_id],
    )?;
    Ok(())
}

fn backfill_authorities(conn: &Connection) -> Result<()> {
    conn.execute(
        r#"
        insert into authorities(
            project_id, path_or_label, authority_type, scope, precedence,
            summary, status, created_at, updated_at
        )
        select e.project_id,
               coalesce(e.source, e.event_type),
               case e.event_type
                   when 'user_instruction' then 'user'
                   when 'design_doc' then 'design'
                   when 'validation_result' then 'validation'
                   when 'review_result' then 'validation'
                   else 'policy'
               end,
               e.scope,
               max(e.precedence),
               e.text_or_summary,
               'active',
               current_timestamp,
               current_timestamp
        from authority_events e
        left join authorities a
          on a.project_id = e.project_id
         and a.path_or_label = coalesce(e.source, e.event_type)
         and a.authority_type = case e.event_type
                   when 'user_instruction' then 'user'
                   when 'design_doc' then 'design'
                   when 'validation_result' then 'validation'
                   when 'review_result' then 'validation'
                   else 'policy'
               end
         and coalesce(a.scope, 'project') = coalesce(e.scope, 'project')
        where e.authority_id is null
          and a.id is null
        group by e.project_id, coalesce(e.source, e.event_type), e.event_type, e.scope
        "#,
        [],
    )?;
    conn.execute(
        r#"
        update authority_events
        set authority_id = (
            select a.id
            from authorities a
            where a.project_id = authority_events.project_id
              and a.path_or_label = coalesce(authority_events.source, authority_events.event_type)
              and a.authority_type = case authority_events.event_type
                   when 'user_instruction' then 'user'
                   when 'design_doc' then 'design'
                   when 'validation_result' then 'validation'
                   when 'review_result' then 'validation'
                   else 'policy'
              end
              and coalesce(a.scope, 'project') = coalesce(authority_events.scope, 'project')
            order by a.id desc
            limit 1
        )
        where authority_id is null
        "#,
        [],
    )?;
    Ok(())
}

fn ensure_authority_row(
    conn: &Connection,
    project_id: i64,
    path_or_label: &str,
    authority_type: &str,
    scope: Option<&str>,
    precedence: i64,
    summary: &str,
) -> Result<i64> {
    let existing_id = conn
        .query_row(
            r#"
            select id
            from authorities
            where project_id = ?1
              and path_or_label = ?2
              and authority_type = ?3
              and coalesce(scope, 'project') = coalesce(?4, 'project')
            "#,
            params![project_id, path_or_label, authority_type, scope],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    if let Some(id) = existing_id {
        conn.execute(
            r#"
            update authorities
            set precedence = ?1,
                summary = ?2,
                status = 'active',
                updated_at = current_timestamp
            where id = ?3
            "#,
            params![precedence, summary, id],
        )?;
        return Ok(id);
    }
    conn.execute(
        r#"
        insert into authorities(
            project_id, path_or_label, authority_type, scope, precedence,
            summary, status, created_at, updated_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, 'active', current_timestamp, current_timestamp)
        "#,
        params![
            project_id,
            path_or_label,
            authority_type,
            scope,
            precedence,
            summary
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn count_rows(conn: &Connection, table: &str, predicate: &str) -> Result<i64> {
    let sql = format!("select count(*) from {table} where {predicate}");
    let count = conn.query_row(&sql, [], |row| row.get(0))?;
    Ok(count)
}

pub(crate) fn project_id(conn: &Connection) -> Result<i64> {
    conn.query_row("select id from projects order by id limit 1", [], |row| {
        row.get(0)
    })
    .context("project row not found; run agent-workbench init")
}

pub(crate) fn max_id(conn: &Connection, table: &str) -> Result<i64> {
    let sql = format!("select coalesce(max(id), 0) from {table}");
    let id = conn.query_row(&sql, [], |row| row.get(0))?;
    Ok(id)
}

pub(crate) fn active_activation(conn: &Connection) -> Result<Option<StoredActivation>> {
    conn.query_row(
        r#"
        select a.id, a.project_id, a.work_unit_id, a.stack_depth, a.status
        from work_unit_activations a
        where a.status = 'active'
        order by a.id desc
        limit 1
        "#,
        [],
        stored_activation,
    )
    .optional()
    .map_err(Into::into)
}

pub(crate) fn suspended_activation(conn: &Connection) -> Result<Option<StoredActivation>> {
    conn.query_row(
        r#"
        select a.id, a.project_id, a.work_unit_id, a.stack_depth, a.status
        from work_unit_activations a
        where a.status = 'suspended'
        order by a.stack_depth desc, a.id desc
        limit 1
        "#,
        [],
        stored_activation,
    )
    .optional()
    .map_err(Into::into)
}

fn stored_activation(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredActivation> {
    Ok(StoredActivation {
        activation_id: row.get(0)?,
        project_id: row.get(1)?,
        work_unit_id: row.get(2)?,
        stack_depth: row.get(3)?,
        status: row.get(4)?,
    })
}

pub(crate) fn suspend_snapshot(
    conn: &Connection,
    activation_id: i64,
) -> Result<StoredSuspendSnapshot> {
    conn.query_row(
        r#"
        select id, reason, active_task_ids, next_action, selected_gate_id,
               authority_refs, review_scope_refs, repository_heads,
               repository_snapshot_ids, repository_status, dirty_state_summary,
               open_findings, assumptions
        from suspend_snapshots
        where work_unit_activation_id = ?1
        order by id desc
        limit 1
        "#,
        params![activation_id],
        |row| {
            Ok(StoredSuspendSnapshot {
                id: row.get(0)?,
                reason: row.get(1)?,
                active_task_ids: row.get(2)?,
                next_action: row.get(3)?,
                selected_gate_id: row.get(4)?,
                authority_refs: row.get(5)?,
                review_scope_refs: row.get(6)?,
                repository_heads: row.get(7)?,
                repository_snapshot_ids: row.get(8)?,
                repository_status: row.get(9)?,
                dirty_state_summary: row.get(10)?,
                open_findings: row.get(11)?,
                assumptions: row.get(12)?,
            })
        },
    )
    .optional()?
    .context("suspend snapshot not found")
}

pub(crate) fn insert_event(conn: &Connection, event: NewEvent<'_>) -> Result<i64> {
    conn.execute(
        r#"
        insert into work_unit_events(
            work_unit_id, work_unit_activation_id, related_activation_id,
            event_type, reason, status_domain, previous_status, next_status, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, current_timestamp)
        "#,
        params![
            event.work_unit_id,
            event.activation_id,
            event.related_activation_id,
            event.event_type,
            event.reason,
            event.status_domain,
            event.previous_status,
            event.next_status,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

#[derive(Debug)]
pub(crate) struct StoredActivation {
    pub(crate) activation_id: i64,
    pub(crate) project_id: i64,
    pub(crate) work_unit_id: i64,
    pub(crate) stack_depth: i64,
    pub(crate) status: String,
}

#[derive(Debug)]
pub(crate) struct StoredSuspendSnapshot {
    pub(crate) id: i64,
    pub(crate) reason: String,
    pub(crate) active_task_ids: Option<String>,
    pub(crate) next_action: String,
    pub(crate) selected_gate_id: Option<i64>,
    pub(crate) authority_refs: Option<String>,
    pub(crate) review_scope_refs: Option<String>,
    pub(crate) repository_heads: Option<String>,
    pub(crate) repository_snapshot_ids: Option<String>,
    pub(crate) repository_status: Option<String>,
    pub(crate) dirty_state_summary: Option<String>,
    pub(crate) open_findings: Option<String>,
    pub(crate) assumptions: Option<String>,
}

pub(crate) struct NewEvent<'a> {
    pub(crate) work_unit_id: i64,
    pub(crate) activation_id: Option<i64>,
    pub(crate) related_activation_id: Option<i64>,
    pub(crate) event_type: &'a str,
    pub(crate) reason: Option<&'a str>,
    pub(crate) status_domain: &'a str,
    pub(crate) previous_status: Option<&'a str>,
    pub(crate) next_status: Option<&'a str>,
}

#[derive(Debug)]
pub struct InitOutcome {
    pub ledger_path: PathBuf,
}

#[derive(Debug)]
pub struct ProjectStatus {
    pub initialized: bool,
    pub ledger_path: PathBuf,
    pub project_name: Option<String>,
    pub open_work_units: i64,
    pub active_activations: i64,
    pub schema_version: Option<i64>,
    pub phase_blocker: Option<PhaseBlocker>,
    pub finding_remediations: Vec<FindingRemediation>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum NextAction {
    NotInitialized {
        ledger_path: PathBuf,
    },
    BlockedPhase {
        blocker: PhaseBlocker,
    },
    FindingRemediation {
        remediations: Vec<FindingRemediation>,
    },
    NoOpenWorkUnit,
    ResumeSuspended {
        work_unit: ActiveWorkUnit,
    },
    ActivateOpen {
        work_unit: ActiveWorkUnit,
    },
    ContinueActive {
        work_unit: ActiveWorkUnit,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindingRemediation {
    pub review_plan_id: i64,
    pub work_unit_id: i64,
    pub finding_id: i64,
    pub closure_id: i64,
    pub description: String,
    pub affected_surfaces: String,
    pub fix_plan: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseBlocker {
    pub kind: String,
    pub review_plan_id: Option<i64>,
    pub work_unit_id: Option<i64>,
    pub review_type: Option<String>,
    pub stage: Option<String>,
    pub review_run_id: Option<i64>,
    pub finding_id: Option<i64>,
    pub severity: Option<String>,
    pub classification: Option<String>,
    pub description: String,
    pub next_action: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ActiveWorkUnit {
    pub id: i64,
    pub title: String,
    pub design_version_id: Option<i64>,
    pub next_phase_id: Option<i64>,
    pub next_phase_key: Option<String>,
    pub next_phase_title: Option<String>,
}

const SCHEMA: &str = r#"
create table if not exists schema_migrations (
    version integer primary key,
    applied_at text not null
);

create table if not exists validation_link_repair_runs (
    id integer primary key,
    backup_path text not null unique,
    repaired_validation_run_count integer not null,
    change_count integer not null,
    created_at text not null
);

create table if not exists validation_link_repair_changes (
    id integer primary key,
    repair_run_id integer not null references validation_link_repair_runs(id),
    validation_run_id integer not null,
    entity_type text not null,
    entity_id integer not null,
    field_name text not null,
    before_value text,
    after_value text,
    created_at text not null
);

create trigger if not exists trg_validation_link_repair_runs_immutable_update
before update on validation_link_repair_runs
begin
    select raise(abort, 'validation link repair audit is immutable');
end;

create trigger if not exists trg_validation_link_repair_runs_immutable_delete
before delete on validation_link_repair_runs
begin
    select raise(abort, 'validation link repair audit is immutable');
end;

create trigger if not exists trg_validation_link_repair_changes_immutable_update
before update on validation_link_repair_changes
begin
    select raise(abort, 'validation link repair audit is immutable');
end;

create trigger if not exists trg_validation_link_repair_changes_immutable_delete
before delete on validation_link_repair_changes
begin
    select raise(abort, 'validation link repair audit is immutable');
end;

create table if not exists projects (
    id integer primary key,
    name text not null,
    root_path text not null unique,
    created_at text not null,
    updated_at text not null
);

create table if not exists repositories (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    name text not null,
    path text not null,
    current_head text,
    status_summary text,
    last_checked_at text,
    unique(project_id, name),
    unique(project_id, path)
);

create table if not exists repository_snapshots (
    id integer primary key,
    repository_id integer not null references repositories(id) on delete cascade,
    work_unit_activation_id integer references work_unit_activations(id),
    head_sha text,
    branch text,
    status_summary text,
    is_clean integer not null check (is_clean in (0, 1)),
    created_at text not null
);

create table if not exists repository_dirty_entries (
    id integer primary key,
    repository_snapshot_id integer not null references repository_snapshots(id) on delete cascade,
    path text not null,
    change_type text not null check (change_type in ('modified', 'added', 'deleted', 'renamed', 'untracked', 'ignored')),
    staged integer not null default 0 check (staged in (0, 1)),
    content_hash text
);

create table if not exists repository_state_classifications (
    id integer primary key,
    repository_snapshot_id integer not null references repository_snapshots(id) on delete cascade,
    dirty_entry_id integer references repository_dirty_entries(id) on delete cascade,
    classification text not null check (classification in ('expected', 'unrelated', 'generated', 'requires_action', 'accepted_exception')),
    reason text not null,
    acceptance_record_id integer references acceptance_records(id),
    created_at text not null
);

create table if not exists repository_snapshot_comparisons (
    id integer primary key,
    base_repository_snapshot_id integer not null references repository_snapshots(id) on delete cascade,
    current_repository_snapshot_id integer not null references repository_snapshots(id) on delete cascade,
    comparison_type text not null check (comparison_type in ('resume', 'close', 'validation', 'review')),
    head_changed integer not null check (head_changed in (0, 1)),
    dirty_state_changed integer not null check (dirty_state_changed in (0, 1)),
    nested_repository_changed integer not null default 0 check (nested_repository_changed in (0, 1)),
    result text not null check (result in ('same', 'changed_classified', 'changed_unclassified')),
    created_at text not null
);

create table if not exists git_commits (
    id integer primary key,
    repository_id integer not null references repositories(id) on delete cascade,
    commit_sha text not null,
    short_sha text,
    subject text,
    author_name text,
    author_email text,
    committed_at text,
    parent_shas text,
    created_at text not null,
    unique(repository_id, commit_sha)
);

create table if not exists git_file_changes (
    id integer primary key,
    git_commit_id integer not null references git_commits(id) on delete cascade,
    repository_id integer not null references repositories(id) on delete cascade,
    path text not null,
    old_path text,
    change_type text not null check (change_type in ('added', 'modified', 'deleted', 'renamed', 'copied')),
    additions integer,
    deletions integer,
    content_hash text
);

create trigger if not exists trg_repository_snapshot_activation_project_insert
before insert on repository_snapshots
for each row
when new.work_unit_activation_id is not null
  and (select project_id from repositories where id = new.repository_id)
      != (select project_id from work_unit_activations where id = new.work_unit_activation_id)
begin
    select raise(abort, 'repository snapshot activation must match repository project_id');
end;

create trigger if not exists trg_repository_snapshot_activation_project_update
before update of repository_id, work_unit_activation_id on repository_snapshots
for each row
when new.work_unit_activation_id is not null
  and (select project_id from repositories where id = new.repository_id)
      != (select project_id from work_unit_activations where id = new.work_unit_activation_id)
begin
    select raise(abort, 'repository snapshot activation must match repository project_id');
end;

create trigger if not exists trg_repository_state_classification_dirty_insert
before insert on repository_state_classifications
for each row
when new.dirty_entry_id is not null
  and new.repository_snapshot_id != (
      select repository_snapshot_id
      from repository_dirty_entries
      where id = new.dirty_entry_id
  )
begin
    select raise(abort, 'repository state classification dirty entry must match snapshot');
end;

create trigger if not exists trg_repository_state_classification_dirty_update
before update of repository_snapshot_id, dirty_entry_id on repository_state_classifications
for each row
when new.dirty_entry_id is not null
  and new.repository_snapshot_id != (
      select repository_snapshot_id
      from repository_dirty_entries
      where id = new.dirty_entry_id
  )
begin
    select raise(abort, 'repository state classification dirty entry must match snapshot');
end;

create trigger if not exists trg_repository_snapshot_comparison_repository_insert
before insert on repository_snapshot_comparisons
for each row
when (select repository_id from repository_snapshots where id = new.base_repository_snapshot_id)
  != (select repository_id from repository_snapshots where id = new.current_repository_snapshot_id)
begin
    select raise(abort, 'repository snapshot comparison requires one repository');
end;

create trigger if not exists trg_repository_snapshot_comparison_repository_update
before update of base_repository_snapshot_id, current_repository_snapshot_id on repository_snapshot_comparisons
for each row
when (select repository_id from repository_snapshots where id = new.base_repository_snapshot_id)
  != (select repository_id from repository_snapshots where id = new.current_repository_snapshot_id)
begin
    select raise(abort, 'repository snapshot comparison requires one repository');
end;

create trigger if not exists trg_git_file_change_repository_insert
before insert on git_file_changes
for each row
when new.repository_id != (select repository_id from git_commits where id = new.git_commit_id)
begin
    select raise(abort, 'git file change repository must match git commit repository');
end;

create trigger if not exists trg_git_file_change_repository_update
before update of git_commit_id, repository_id on git_file_changes
for each row
when new.repository_id != (select repository_id from git_commits where id = new.git_commit_id)
begin
    select raise(abort, 'git file change repository must match git commit repository');
end;

create table if not exists authorities (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    path_or_label text not null,
    authority_type text not null check (authority_type in ('user', 'design', 'spec', 'plan', 'policy', 'validation')),
    scope text,
    precedence integer not null default 0,
    summary text not null,
    status text not null default 'active' check (status in ('active', 'inactive', 'superseded')),
    created_at text not null,
    updated_at text not null
);

create unique index if not exists ux_authorities_identity
on authorities(project_id, path_or_label, authority_type, coalesce(scope, 'project'));

create table if not exists authority_events (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    authority_id integer references authorities(id),
    event_type text not null check (event_type in ('user_instruction', 'design_doc', 'agents', 'policy', 'review_result', 'validation_result')),
    source text,
    text_or_summary text not null,
    scope text,
    precedence integer not null default 0,
    supersedes_event_id integer references authority_events(id),
    status text not null default 'active' check (status in ('active', 'inactive', 'superseded')),
    created_at text not null
);

create trigger if not exists trg_authority_event_authority_project_insert
before insert on authority_events
for each row
when new.authority_id is not null
 and new.project_id != (select project_id from authorities where id = new.authority_id)
begin
    select raise(abort, 'authority event authority must match project_id');
end;

create trigger if not exists trg_authority_event_authority_project_update
before update of project_id, authority_id on authority_events
for each row
when new.authority_id is not null
 and new.project_id != (select project_id from authorities where id = new.authority_id)
begin
    select raise(abort, 'authority event authority must match project_id');
end;

create table if not exists work_units (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    parent_work_unit_id integer references work_units(id),
    title text not null,
    status text not null default 'open' check (status in ('open', 'blocked', 'closed', 'abandoned')),
    responsibility text,
    in_scope text,
    out_of_scope text,
    interrupt_reason text,
    selected_gate_id integer,
    review_plan_status text,
    started_at text not null,
    closed_at text,
    close_summary text
);

create table if not exists work_unit_activations (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    work_unit_id integer not null references work_units(id) on delete cascade,
    parent_activation_id integer references work_unit_activations(id),
    suspended_by_activation_id integer references work_unit_activations(id),
    stack_depth integer not null default 0,
    status text not null check (status in ('active', 'suspended', 'completed', 'abandoned')),
    activation_reason text not null check (activation_reason in ('start', 'interrupt', 'resume', 'reopen', 'follow_up')),
    suspend_snapshot_id integer,
    opened_at text not null,
    suspended_at text,
    completed_at text
);

create unique index if not exists ux_one_active_activation
on work_unit_activations(project_id)
where status = 'active';

create trigger if not exists trg_activation_project_matches_work_unit_insert
before insert on work_unit_activations
for each row
when new.project_id != (select project_id from work_units where id = new.work_unit_id)
begin
    select raise(abort, 'activation project_id must match work unit project_id');
end;

create trigger if not exists trg_activation_project_matches_work_unit_update
before update of project_id, work_unit_id on work_unit_activations
for each row
when new.project_id != (select project_id from work_units where id = new.work_unit_id)
begin
    select raise(abort, 'activation project_id must match work unit project_id');
end;

create table if not exists work_unit_events (
    id integer primary key,
    work_unit_id integer not null references work_units(id) on delete cascade,
    work_unit_activation_id integer references work_unit_activations(id),
    related_activation_id integer references work_unit_activations(id),
    event_type text not null check (event_type in ('opened', 'suspended', 'resumed', 'closed', 'abandoned', 'blocked', 'unblocked', 'reopened', 'invalidated', 'follow_up_created')),
    reason text,
    caused_by_work_unit_id integer references work_units(id),
    authority_event_id integer references authority_events(id),
    status_domain text not null check (status_domain in ('work_unit', 'activation')),
    previous_status text,
    next_status text,
    created_at text not null
);

create table if not exists suspend_snapshots (
    id integer primary key,
    work_unit_activation_id integer not null references work_unit_activations(id) on delete cascade,
    work_unit_id integer not null references work_units(id) on delete cascade,
    reason text not null,
    active_task_ids text,
    next_action text not null,
    selected_gate_id integer,
    authority_refs text,
    review_scope_refs text,
    repository_heads text,
    repository_snapshot_ids text,
    repository_status text,
    dirty_state_summary text,
    open_findings text,
    assumptions text,
    created_at text not null
);

create table if not exists resume_checks (
    id integer primary key,
    work_unit_id integer not null references work_units(id) on delete cascade,
    work_unit_activation_id integer not null references work_unit_activations(id) on delete cascade,
    suspend_snapshot_id integer not null references suspend_snapshots(id) on delete cascade,
    maturity text not null check (maturity in ('basic', 'trace-aware', 'repo-aware')),
    status text not null default 'pending' check (status in ('pending', 'consumed', 'stale')),
    result text not null check (result in ('allowed', 'blocked', 'needs_design_review', 'needs_gate_reselection', 'needs_user_decision')),
    authority_event_high_watermark integer,
    activation_stack_revision integer,
    repository_snapshot_id integer,
    repository_state_revision integer,
    allowed_next_action text,
    blocking_reason text,
    consumed_at text,
    consumed_by_work_unit_event_id integer references work_unit_events(id),
    created_at text not null
);

create table if not exists resume_check_items (
    id integer primary key,
    resume_check_id integer not null references resume_checks(id) on delete cascade,
    check_name text not null check (check_name in ('resume_target_suspended', 'snapshot_exists', 'suspend_reason_exists', 'next_action_exists', 'deeper_frames_closed', 'blocking_dependencies_clear', 'active_tasks_current', 'authority_refs_current', 'review_scope_refs_current', 'design_version_current', 'task_derivation_current', 'checklist_current', 'selected_gate_current', 'review_plan_current', 'open_findings_current', 'repository_heads_current', 'repository_state_current', 'assumptions_current')),
    result text not null check (result in ('pass', 'fail', 'not_checked', 'needs_evidence')),
    evidence_ref text,
    blocking_action text,
    details text
);

create trigger if not exists trg_resume_check_repository_snapshot_insert
before insert on resume_checks
for each row
when new.repository_snapshot_id is not null
  and (
      not exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
      or (select project_id from work_units where id = new.work_unit_id) != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
  )
begin
    select raise(abort, 'resume check repository snapshot must match work unit project_id');
end;

create trigger if not exists trg_resume_check_repository_snapshot_update
before update of work_unit_id, repository_snapshot_id on resume_checks
for each row
when new.repository_snapshot_id is not null
  and (
      not exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
      or (select project_id from work_units where id = new.work_unit_id) != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
  )
begin
    select raise(abort, 'resume check repository snapshot must match work unit project_id');
end;

create table if not exists work_unit_dependencies (
    id integer primary key,
    work_unit_id integer not null references work_units(id) on delete cascade,
    depends_on_work_unit_id integer not null references work_units(id) on delete cascade,
    dependency_type text not null check (dependency_type in ('blocks', 'discovered_by', 'supersedes', 'invalidates_assumption', 'regression_of', 'invalidates_closure', 'follow_up_of')),
    reason text,
    status text not null default 'open' check (status in ('open', 'resolved')),
    created_at text not null,
    resolved_at text,
    resolved_by_work_unit_event_id integer references work_unit_events(id)
);

create table if not exists tasks (
    id integer primary key,
    work_unit_id integer references work_units(id) on delete cascade,
    title text not null,
    priority text not null default 'medium' check (priority in ('critical', 'high', 'medium', 'low')),
    status text not null default 'open' check (status in ('open', 'blocked', 'closed', 'accepted_out_of_scope')),
    source text not null default 'user' check (source in ('user', 'plan', 'review', 'coverage', 'design', 'work_record')),
    parent_task_id integer references tasks(id),
    details text,
    completion_condition text,
    closed_by_commit text
);

create table if not exists decisions (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decision_key text,
    topic text not null,
    decision text not null,
    rationale text,
    compatibility_impact text,
    status text not null default 'accepted' check (status in ('accepted', 'rejected', 'superseded')),
    authority_refs text,
    created_at text not null
);

create table if not exists acceptance_records (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    target_type text not null check (target_type in (
        'task', 'design_requirement', 'validation_gate_template', 'design_file',
        'design_requirement_key', 'coverage_item', 'finding', 'validation_gate',
        'validation_run', 'repository_state_classification',
        'repository_snapshot_comparison', 'review_plan', 'checklist_item',
        'command_profile', 'command_usage', 'command_deviation',
        'rule_binding', 'stale_record'
    )),
    task_id integer references tasks(id),
    design_requirement_id integer references design_requirements(id),
    validation_gate_template_id integer references validation_gate_templates(id),
    coverage_item_id integer references coverage_items(id),
    finding_id integer references findings(id),
    validation_gate_id integer references validation_gates(id),
    validation_run_id integer references validation_runs(id),
    repository_state_classification_id integer references repository_state_classifications(id),
    repository_snapshot_comparison_id integer references repository_snapshot_comparisons(id),
    review_plan_id integer references review_plans(id),
    checklist_item_id integer references checklist_items(id),
    command_profile_id integer references command_profiles(id),
    command_usage_id integer references command_usages(id),
    command_deviation_id integer references command_deviations(id),
    rule_binding_id integer references rule_bindings(id),
    stale_record_type text,
    stale_record_id integer,
    design_package_key text,
    design_file_path text,
    design_requirement_key text,
    acceptance_type text not null check (acceptance_type in (
        'accepted_out_of_scope', 'explicit_exception', 'evidence_gap',
        'classified_failure', 'stale_accepted'
    )),
    reason text not null,
    scope text,
    created_by text not null check (created_by in ('user', 'agent', 'system')),
    status text not null check (status in ('proposed', 'approved', 'rejected', 'expired')),
    approved_by_authority_event_id integer references authority_events(id),
    approved_at text,
    created_at text not null,
    review_impact text,
    check (
        (
            (case when task_id is not null then 1 else 0 end) +
            (case when design_requirement_id is not null then 1 else 0 end) +
            (case when validation_gate_template_id is not null then 1 else 0 end) +
            (case when coverage_item_id is not null then 1 else 0 end) +
            (case when finding_id is not null then 1 else 0 end) +
            (case when validation_gate_id is not null then 1 else 0 end) +
            (case when validation_run_id is not null then 1 else 0 end) +
            (case when repository_state_classification_id is not null then 1 else 0 end) +
            (case when repository_snapshot_comparison_id is not null then 1 else 0 end) +
            (case when review_plan_id is not null then 1 else 0 end) +
            (case when checklist_item_id is not null then 1 else 0 end) +
            (case when command_profile_id is not null then 1 else 0 end) +
            (case when command_usage_id is not null then 1 else 0 end) +
            (case when command_deviation_id is not null then 1 else 0 end) +
            (case when rule_binding_id is not null then 1 else 0 end) +
            (case when design_package_key is not null and design_file_path is not null and design_requirement_key is null then 1 else 0 end) +
            (case when design_package_key is not null and design_requirement_key is not null and design_file_path is null then 1 else 0 end) +
            (case when stale_record_type is not null and stale_record_id is not null then 1 else 0 end)
        ) = 1
        and (
            (target_type = 'task' and task_id is not null)
            or (target_type = 'design_requirement' and design_requirement_id is not null)
            or (target_type = 'validation_gate_template' and validation_gate_template_id is not null)
            or (target_type = 'coverage_item' and coverage_item_id is not null)
            or (target_type = 'finding' and finding_id is not null)
            or (target_type = 'validation_gate' and validation_gate_id is not null)
            or (target_type = 'validation_run' and validation_run_id is not null)
            or (target_type = 'repository_state_classification' and repository_state_classification_id is not null)
            or (target_type = 'repository_snapshot_comparison' and repository_snapshot_comparison_id is not null)
            or (target_type = 'review_plan' and review_plan_id is not null)
            or (target_type = 'checklist_item' and checklist_item_id is not null)
            or (target_type = 'command_profile' and command_profile_id is not null)
            or (target_type = 'command_usage' and command_usage_id is not null)
            or (target_type = 'command_deviation' and command_deviation_id is not null)
            or (target_type = 'rule_binding' and rule_binding_id is not null)
            or (target_type = 'design_file' and design_package_key is not null and design_file_path is not null)
            or (target_type = 'design_requirement_key' and design_package_key is not null and design_requirement_key is not null)
            or (target_type = 'stale_record' and stale_record_type is not null and stale_record_id is not null)
        )
    )
);

create trigger if not exists trg_repository_state_classification_acceptance_insert
before insert on repository_state_classifications
for each row
when (new.classification = 'accepted_exception' and new.acceptance_record_id is null)
  or (new.classification != 'accepted_exception' and new.acceptance_record_id is not null)
  or (new.acceptance_record_id is not null and (
      not exists (select 1 from acceptance_records where id = new.acceptance_record_id)
      or (select project_id from acceptance_records where id = new.acceptance_record_id) != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
  ))
begin
    select raise(abort, 'repository state classification acceptance must match snapshot project_id');
end;

create trigger if not exists trg_repository_state_classification_acceptance_update
before update of repository_snapshot_id, classification, acceptance_record_id on repository_state_classifications
for each row
when (new.classification = 'accepted_exception' and new.acceptance_record_id is null)
  or (new.classification != 'accepted_exception' and new.acceptance_record_id is not null)
  or (new.acceptance_record_id is not null and (
      not exists (select 1 from acceptance_records where id = new.acceptance_record_id)
      or (select project_id from acceptance_records where id = new.acceptance_record_id) != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
  ))
begin
    select raise(abort, 'repository state classification acceptance must match snapshot project_id');
end;

create table if not exists rule_bindings (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    rule_source_type text not null check (rule_source_type in ('authority_event', 'user_correction', 'command_profile', 'review_policy', 'work_unit', 'validation_gate', 'acceptance_record', 'skill_default')),
    authority_event_id integer references authority_events(id),
    user_correction_id integer,
    command_profile_id integer,
    review_policy_id integer,
    review_plan_id integer references review_plans(id),
    work_unit_id integer references work_units(id),
    validation_gate_id integer,
    acceptance_record_id integer,
    scope_type text not null check (scope_type in ('project', 'repository', 'design_package', 'work_unit', 'agent_role', 'command', 'review')),
    scope_key text,
    precedence integer not null default 0,
    status text not null default 'active' check (status in ('active', 'shadowed', 'inactive', 'superseded')),
    created_at text not null
);

create table if not exists user_corrections (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    authority_event_id integer references authority_events(id),
    scope text,
    correction_type text not null check (correction_type in ('process', 'design', 'implementation', 'review', 'command', 'communication', 'other')),
    mistake_pattern text not null,
    correction text not null,
    applies_to text not null check (applies_to in ('current_work_unit', 'project', 'repository', 'design_package', 'command_profile', 'agent_role')),
    severity text not null default 'medium' check (severity in ('critical', 'high', 'medium', 'low')),
    status text not null default 'active' check (status in ('active', 'superseded', 'retired')),
    supersedes_user_correction_id integer references user_corrections(id),
    created_at text not null
);

create table if not exists command_profiles (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    repository_id integer,
    name text not null,
    command text not null,
    command_type text not null check (command_type in ('validation', 'test', 'lint', 'format', 'build', 'dev_server', 'review_context', 'export', 'other')),
    scope text,
    status text not null default 'candidate' check (status in ('candidate', 'preferred', 'fixed', 'deprecated', 'blocked')),
    stability text not null default 'context_dependent' check (stability in ('stable', 'context_dependent', 'experimental')),
    working_directory text,
    environment text,
    timeout text,
    expected_result text,
    replaces_command_profile_id integer references command_profiles(id),
    source text not null default 'agent_observed' check (source in ('user', 'agent_observed', 'design', 'validation_gate')),
    created_at text not null,
    updated_at text not null,
    unique(project_id, name)
);

create trigger if not exists trg_command_profile_repository_insert
before insert on command_profiles
for each row
when new.repository_id is not null
  and (
      not exists (select 1 from repositories where id = new.repository_id)
      or new.project_id != (select project_id from repositories where id = new.repository_id)
  )
begin
    select raise(abort, 'command profile repository must match project_id');
end;

create trigger if not exists trg_command_profile_repository_update
before update of project_id, repository_id on command_profiles
for each row
when new.repository_id is not null
  and (
      not exists (select 1 from repositories where id = new.repository_id)
      or new.project_id != (select project_id from repositories where id = new.repository_id)
  )
begin
    select raise(abort, 'command profile repository must match project_id');
end;

create table if not exists command_usages (
    id integer primary key,
    project_id integer references projects(id) on delete cascade,
    command_profile_id integer references command_profiles(id),
    work_unit_id integer references work_units(id),
    work_unit_activation_id integer references work_unit_activations(id),
    command text not null,
    result text not null check (result in ('pass', 'fail', 'timeout', 'cancelled', 'unknown')),
    log_path text,
    repository_snapshot_id integer,
    created_at text not null
);

create trigger if not exists trg_command_usage_project_insert
before insert on command_usages
for each row
when new.project_id is null
  or not exists (select 1 from projects where id = new.project_id)
  or (new.command_profile_id is not null and not exists (
      select 1 from command_profiles where id = new.command_profile_id
  ))
  or (new.work_unit_id is not null and not exists (
      select 1 from work_units where id = new.work_unit_id
  ))
  or (new.work_unit_activation_id is not null and not exists (
      select 1 from work_unit_activations where id = new.work_unit_activation_id
  ))
  or (
      new.command_profile_id is not null
      and new.project_id != (select project_id from command_profiles where id = new.command_profile_id)
  )
  or (
      new.work_unit_id is not null
      and new.project_id != (select project_id from work_units where id = new.work_unit_id)
  )
  or (
      new.work_unit_activation_id is not null
      and new.project_id != (select project_id from work_unit_activations where id = new.work_unit_activation_id)
  )
  or (
      new.command_profile_id is not null
      and new.work_unit_id is not null
      and (select project_id from command_profiles where id = new.command_profile_id) != (
          select project_id from work_units where id = new.work_unit_id
      )
  )
  or (
      new.command_profile_id is not null
      and new.work_unit_activation_id is not null
      and (select project_id from command_profiles where id = new.command_profile_id) != (
          select project_id from work_unit_activations where id = new.work_unit_activation_id
      )
  )
  or (
      new.work_unit_id is not null
      and new.work_unit_activation_id is not null
      and (select project_id from work_units where id = new.work_unit_id) != (
          select project_id from work_unit_activations where id = new.work_unit_activation_id
      )
  )
begin
    select raise(abort, 'command usage references must match project');
end;

create trigger if not exists trg_command_usage_project_update
before update of project_id, command_profile_id, work_unit_id, work_unit_activation_id on command_usages
for each row
when new.project_id is null
  or not exists (select 1 from projects where id = new.project_id)
  or (new.command_profile_id is not null and not exists (
      select 1 from command_profiles where id = new.command_profile_id
  ))
  or (new.work_unit_id is not null and not exists (
      select 1 from work_units where id = new.work_unit_id
  ))
  or (new.work_unit_activation_id is not null and not exists (
      select 1 from work_unit_activations where id = new.work_unit_activation_id
  ))
  or (
      new.command_profile_id is not null
      and new.project_id != (select project_id from command_profiles where id = new.command_profile_id)
  )
  or (
      new.work_unit_id is not null
      and new.project_id != (select project_id from work_units where id = new.work_unit_id)
  )
  or (
      new.work_unit_activation_id is not null
      and new.project_id != (select project_id from work_unit_activations where id = new.work_unit_activation_id)
  )
  or (
      new.command_profile_id is not null
      and new.work_unit_id is not null
      and (select project_id from command_profiles where id = new.command_profile_id) != (
          select project_id from work_units where id = new.work_unit_id
      )
  )
  or (
      new.command_profile_id is not null
      and new.work_unit_activation_id is not null
      and (select project_id from command_profiles where id = new.command_profile_id) != (
          select project_id from work_unit_activations where id = new.work_unit_activation_id
      )
  )
  or (
      new.work_unit_id is not null
      and new.work_unit_activation_id is not null
      and (select project_id from work_units where id = new.work_unit_id) != (
          select project_id from work_unit_activations where id = new.work_unit_activation_id
      )
  )
begin
    select raise(abort, 'command usage references must match project');
end;

create table if not exists command_deviations (
    id integer primary key,
    command_profile_id integer not null references command_profiles(id) on delete cascade,
    command_usage_id integer references command_usages(id),
    work_unit_id integer references work_units(id),
    reason text not null,
    status text not null default 'proposed' check (status in ('proposed', 'approved', 'rejected')),
    acceptance_record_id integer,
    created_at text not null
);

create trigger if not exists trg_command_usage_repository_snapshot_insert
before insert on command_usages
for each row
when new.repository_snapshot_id is not null
  and (
      not exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
      or new.project_id != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
      or (
          new.command_profile_id is not null
          and (select project_id from command_profiles where id = new.command_profile_id) != (
              select r.project_id
              from repository_snapshots s
              join repositories r on r.id = s.repository_id
              where s.id = new.repository_snapshot_id
          )
      )
      or (
          new.work_unit_id is not null
          and (select project_id from work_units where id = new.work_unit_id) != (
              select r.project_id
              from repository_snapshots s
              join repositories r on r.id = s.repository_id
              where s.id = new.repository_snapshot_id
          )
      )
      or (
          new.work_unit_activation_id is not null
          and (select project_id from work_unit_activations where id = new.work_unit_activation_id) != (
              select r.project_id
              from repository_snapshots s
              join repositories r on r.id = s.repository_id
              where s.id = new.repository_snapshot_id
          )
      )
  )
begin
    select raise(abort, 'command usage repository snapshot must match referenced project');
end;

create trigger if not exists trg_command_usage_repository_snapshot_update
before update of project_id, command_profile_id, work_unit_id, work_unit_activation_id, repository_snapshot_id on command_usages
for each row
when new.repository_snapshot_id is not null
  and (
      not exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
      or new.project_id != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
      or (
          new.command_profile_id is not null
          and (select project_id from command_profiles where id = new.command_profile_id) != (
              select r.project_id
              from repository_snapshots s
              join repositories r on r.id = s.repository_id
              where s.id = new.repository_snapshot_id
          )
      )
      or (
          new.work_unit_id is not null
          and (select project_id from work_units where id = new.work_unit_id) != (
              select r.project_id
              from repository_snapshots s
              join repositories r on r.id = s.repository_id
              where s.id = new.repository_snapshot_id
          )
      )
      or (
          new.work_unit_activation_id is not null
          and (select project_id from work_unit_activations where id = new.work_unit_activation_id) != (
              select r.project_id
              from repository_snapshots s
              join repositories r on r.id = s.repository_id
              where s.id = new.repository_snapshot_id
          )
      )
  )
begin
    select raise(abort, 'command usage repository snapshot must match referenced project');
end;

create table if not exists work_records (
    id integer primary key,
    project_id integer references projects(id) on delete cascade,
    work_unit_id integer references work_units(id) on delete cascade,
    topic text not null,
    work_performed text,
    next_actions text,
    notable_operations text,
    export_path text,
    created_at text not null
);

create trigger if not exists trg_work_record_project_insert
before insert on work_records
for each row
when new.project_id is null
  or not exists (select 1 from projects where id = new.project_id)
  or (
      new.work_unit_id is not null
      and (
          not exists (select 1 from work_units where id = new.work_unit_id)
          or new.project_id != (select project_id from work_units where id = new.work_unit_id)
      )
  )
begin
    select raise(abort, 'work record project_id must match referenced work unit');
end;

create trigger if not exists trg_work_record_project_update
before update of project_id, work_unit_id on work_records
for each row
when new.project_id is null
  or not exists (select 1 from projects where id = new.project_id)
  or (
      new.work_unit_id is not null
      and (
          not exists (select 1 from work_units where id = new.work_unit_id)
          or new.project_id != (select project_id from work_units where id = new.work_unit_id)
      )
  )
begin
    select raise(abort, 'work record project_id must match referenced work unit');
end;

create table if not exists work_record_commands (
    id integer primary key,
    work_record_id integer not null references work_records(id) on delete cascade,
    command_usage_id integer references command_usages(id),
    command_profile_id integer references command_profiles(id),
    command text,
    result text,
    log_path text,
    note text
);

create trigger if not exists trg_work_record_command_required_insert
before insert on work_record_commands
for each row
when new.command_usage_id is null and new.command is null
begin
    select raise(abort, 'work record command requires command_usage_id or command');
end;

create trigger if not exists trg_work_record_command_required_update
before update of command_usage_id, command on work_record_commands
for each row
when new.command_usage_id is null and new.command is null
begin
    select raise(abort, 'work record command requires command_usage_id or command');
end;

create table if not exists work_record_commits (
    id integer primary key,
    work_record_id integer not null references work_records(id) on delete cascade,
    git_commit_id integer,
    commit_sha text,
    role text not null default 'referenced' check (role in ('created', 'referenced', 'validation_base', 'rollback_point')),
    note text,
    auto_linked integer not null default 0 check (auto_linked in (0, 1))
);

create trigger if not exists trg_work_record_commit_required_insert
before insert on work_record_commits
for each row
when new.commit_sha is null
begin
    select raise(abort, 'work record commit requires commit_sha');
end;

create trigger if not exists trg_work_record_commit_required_update
before update of commit_sha on work_record_commits
for each row
when new.commit_sha is null
begin
    select raise(abort, 'work record commit requires commit_sha');
end;

create table if not exists work_record_files (
    id integer primary key,
    work_record_id integer not null references work_records(id) on delete cascade,
    git_file_change_id integer,
    repository_id integer,
    path text not null,
    role text not null default 'changed' check (role in ('changed', 'reviewed', 'generated', 'evidence', 'ignored')),
    note text,
    auto_linked integer not null default 0 check (auto_linked in (0, 1)),
    repository_auto_linked integer not null default 0 check (repository_auto_linked in (0, 1))
);

create trigger if not exists trg_work_record_command_project_insert
before insert on work_record_commands
for each row
when (new.command_usage_id is not null and (
      not exists (select 1 from command_usages where id = new.command_usage_id)
      or (select project_id from command_usages where id = new.command_usage_id) != (
          select project_id from work_records where id = new.work_record_id
      )
  ))
  or (new.command_profile_id is not null and (
      not exists (select 1 from command_profiles where id = new.command_profile_id)
      or (select project_id from command_profiles where id = new.command_profile_id) != (
          select project_id from work_records where id = new.work_record_id
      )
  ))
begin
    select raise(abort, 'work record command must match referenced project');
end;

create trigger if not exists trg_work_record_command_project_update
before update of work_record_id, command_usage_id, command_profile_id on work_record_commands
for each row
when (new.command_usage_id is not null and (
      not exists (select 1 from command_usages where id = new.command_usage_id)
      or (select project_id from command_usages where id = new.command_usage_id) != (
          select project_id from work_records where id = new.work_record_id
      )
  ))
  or (new.command_profile_id is not null and (
      not exists (select 1 from command_profiles where id = new.command_profile_id)
      or (select project_id from command_profiles where id = new.command_profile_id) != (
          select project_id from work_records where id = new.work_record_id
      )
  ))
begin
    select raise(abort, 'work record command must match referenced project');
end;

create trigger if not exists trg_work_record_commit_git_insert
before insert on work_record_commits
for each row
when new.git_commit_id is not null
  and (
      not exists (select 1 from git_commits where id = new.git_commit_id)
      or new.commit_sha is null
      or new.commit_sha != (select commit_sha from git_commits where id = new.git_commit_id)
      or (select project_id from work_records where id = new.work_record_id) != (
          select r.project_id
          from git_commits c
          join repositories r on r.id = c.repository_id
          where c.id = new.git_commit_id
      )
  )
begin
    select raise(abort, 'work record commit must match git commit');
end;

create trigger if not exists trg_work_record_commit_git_update
before update of work_record_id, git_commit_id, commit_sha on work_record_commits
for each row
when new.git_commit_id is not null
  and (
      not exists (select 1 from git_commits where id = new.git_commit_id)
      or new.commit_sha is null
      or new.commit_sha != (select commit_sha from git_commits where id = new.git_commit_id)
      or (select project_id from work_records where id = new.work_record_id) != (
          select r.project_id
          from git_commits c
          join repositories r on r.id = c.repository_id
          where c.id = new.git_commit_id
      )
  )
begin
    select raise(abort, 'work record commit must match git commit');
end;

create trigger if not exists trg_work_record_file_git_insert
before insert on work_record_files
for each row
when (new.repository_id is not null and not exists (select 1 from repositories where id = new.repository_id))
  or (
      new.git_file_change_id is not null
      and (
          new.repository_id is null
          or not exists (select 1 from git_file_changes where id = new.git_file_change_id)
          or new.repository_id != (select repository_id from git_file_changes where id = new.git_file_change_id)
          or new.path != (select path from git_file_changes where id = new.git_file_change_id)
      )
  )
  or (
      new.repository_id is not null
      and (select project_id from work_records where id = new.work_record_id) != (
          select project_id from repositories where id = new.repository_id
      )
  )
begin
    select raise(abort, 'work record file must match repository or git file change');
end;

create trigger if not exists trg_work_record_file_git_update
before update of work_record_id, git_file_change_id, repository_id, path on work_record_files
for each row
when (new.repository_id is not null and not exists (select 1 from repositories where id = new.repository_id))
  or (
      new.git_file_change_id is not null
      and (
          new.repository_id is null
          or not exists (select 1 from git_file_changes where id = new.git_file_change_id)
          or new.repository_id != (select repository_id from git_file_changes where id = new.git_file_change_id)
          or new.path != (select path from git_file_changes where id = new.git_file_change_id)
      )
  )
  or (
      new.repository_id is not null
      and (select project_id from work_records where id = new.work_record_id) != (
          select project_id from repositories where id = new.repository_id
      )
  )
begin
    select raise(abort, 'work record file must match repository or git file change');
end;

create table if not exists work_record_forks (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    source_work_unit_id integer references work_units(id),
    source_work_unit_activation_id integer references work_unit_activations(id),
    source_work_record_id integer references work_records(id),
    source_repository_snapshot_id integer,
    source_git_commit_id integer,
    source_git_commit_sha text,
    forked_work_unit_id integer references work_units(id),
    fork_reason text not null check (fork_reason in ('design_changed', 'agent_drift', 'invalid_assumption', 'failed_validation', 'user_requested_redo', 'other')),
    discard_policy text not null default 'keep_history' check (discard_policy in ('keep_history', 'supersede_source', 'mark_abandoned')),
    status text not null default 'open' check (status in ('open', 'closed', 'abandoned')),
    created_by_authority_event_id integer references authority_events(id),
    created_at text not null,
    closed_at text
);

create trigger if not exists trg_work_record_fork_repository_git_insert
before insert on work_record_forks
for each row
when new.forked_work_unit_id is null
  or not exists (select 1 from work_units where id = new.forked_work_unit_id)
  or (
      (case when new.source_work_unit_activation_id is not null then 1 else 0 end)
      + (case when new.source_work_record_id is not null then 1 else 0 end)
      + (case when new.source_repository_snapshot_id is not null then 1 else 0 end)
      + (case when new.source_git_commit_id is not null or new.source_git_commit_sha is not null then 1 else 0 end)
  ) != 1
  or new.project_id != (select project_id from work_units where id = new.forked_work_unit_id)
  or (new.source_work_unit_id is not null and new.project_id != (
      select project_id from work_units where id = new.source_work_unit_id
  ))
  or (new.source_work_unit_activation_id is not null and new.project_id != (
      select project_id from work_unit_activations where id = new.source_work_unit_activation_id
  ))
  or (new.source_work_record_id is not null and (
      not exists (select 1 from work_records where id = new.source_work_record_id)
      or new.project_id != (
          select project_id from work_records where id = new.source_work_record_id
      )
  ))
  or (new.source_repository_snapshot_id is not null and (
      not exists (select 1 from repository_snapshots where id = new.source_repository_snapshot_id)
      or new.project_id != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.source_repository_snapshot_id
      )
  ))
  or (new.source_git_commit_id is not null and (
      not exists (select 1 from git_commits where id = new.source_git_commit_id)
      or new.project_id != (
          select r.project_id
          from git_commits c
          join repositories r on r.id = c.repository_id
          where c.id = new.source_git_commit_id
      )
      or (new.source_git_commit_sha is not null and new.source_git_commit_sha != (
          select commit_sha from git_commits where id = new.source_git_commit_id
      ))
  ))
begin
    select raise(abort, 'work record fork repository and git sources must match project');
end;

create trigger if not exists trg_work_record_fork_repository_git_update
before update of project_id, source_work_unit_id, source_work_unit_activation_id, source_work_record_id, source_repository_snapshot_id, source_git_commit_id, source_git_commit_sha, forked_work_unit_id on work_record_forks
for each row
when new.forked_work_unit_id is null
  or not exists (select 1 from work_units where id = new.forked_work_unit_id)
  or (
      (case when new.source_work_unit_activation_id is not null then 1 else 0 end)
      + (case when new.source_work_record_id is not null then 1 else 0 end)
      + (case when new.source_repository_snapshot_id is not null then 1 else 0 end)
      + (case when new.source_git_commit_id is not null or new.source_git_commit_sha is not null then 1 else 0 end)
  ) != 1
  or new.project_id != (select project_id from work_units where id = new.forked_work_unit_id)
  or (new.source_work_unit_id is not null and new.project_id != (
      select project_id from work_units where id = new.source_work_unit_id
  ))
  or (new.source_work_unit_activation_id is not null and new.project_id != (
      select project_id from work_unit_activations where id = new.source_work_unit_activation_id
  ))
  or (new.source_work_record_id is not null and (
      not exists (select 1 from work_records where id = new.source_work_record_id)
      or new.project_id != (
          select project_id from work_records where id = new.source_work_record_id
      )
  ))
  or (new.source_repository_snapshot_id is not null and (
      not exists (select 1 from repository_snapshots where id = new.source_repository_snapshot_id)
      or new.project_id != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.source_repository_snapshot_id
      )
  ))
  or (new.source_git_commit_id is not null and (
      not exists (select 1 from git_commits where id = new.source_git_commit_id)
      or new.project_id != (
          select r.project_id
          from git_commits c
          join repositories r on r.id = c.repository_id
          where c.id = new.source_git_commit_id
      )
      or (new.source_git_commit_sha is not null and new.source_git_commit_sha != (
          select commit_sha from git_commits where id = new.source_git_commit_id
      ))
  ))
begin
    select raise(abort, 'work record fork repository and git sources must match project');
end;

create table if not exists design_packages (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    design_key text not null,
    package_id text not null,
    title text not null,
    root_path text not null,
    format text not null,
    version integer not null,
    package_hash text,
    status text not null default 'draft' check (status in ('draft', 'reviewed', 'approved', 'superseded')),
    current_design_version_id integer,
    created_at text not null,
    updated_at text not null,
    unique(project_id, design_key),
    unique(project_id, package_id)
);

create table if not exists design_versions (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    design_package_id integer not null references design_packages(id) on delete cascade,
    version_number integer not null,
    source_ref text not null,
    package_hash text not null,
    content_hash text not null,
    package_path text not null,
    manifest_path text not null,
    format text not null,
    manifest_version integer not null,
    status text not null default 'draft' check (status in ('draft', 'reviewed', 'approved', 'superseded')),
    imported_at text not null,
    approved_by_authority_event_id integer references authority_events(id),
    approved_at text,
    unique(design_package_id, version_number),
    unique(design_package_id, content_hash)
);

create table if not exists design_files (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    design_package_id integer not null references design_packages(id) on delete cascade,
    design_version_id integer not null references design_versions(id) on delete cascade,
    section_key text not null,
    relative_path text not null,
    content_hash text not null,
    line_count integer not null,
    unique(design_version_id, relative_path)
);

create table if not exists design_requirements (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    design_version_id integer not null references design_versions(id) on delete cascade,
    source_design_file_id integer not null references design_files(id) on delete cascade,
    source_section text not null,
    requirement_key text not null,
    revision integer not null default 1,
    requirement_hash text not null,
    supersedes_requirement_id integer references design_requirements(id),
    requirement_text text not null,
    priority text not null check (priority in ('critical', 'high', 'medium', 'low')),
    required_surfaces text,
    validation_expectation text,
    status text not null check (status in ('active', 'superseded', 'accepted_out_of_scope')),
    created_at text not null,
    unique(design_version_id, requirement_key)
);

create table if not exists design_decisions (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    design_version_id integer not null references design_versions(id) on delete cascade,
    source_design_file_id integer not null references design_files(id) on delete cascade,
    source_section text not null,
    decision_key text not null,
    decision_hash text not null,
    topic text not null,
    decision_text text not null,
    rationale text,
    supersedes_decision_keys text,
    status text not null check (status in ('accepted', 'rejected', 'superseded')),
    created_at text not null,
    unique(design_version_id, decision_key)
);

create table if not exists validation_gate_templates (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    design_version_id integer not null references design_versions(id) on delete cascade,
    source_design_file_id integer not null references design_files(id) on delete cascade,
    source_section text not null,
    gate_key text not null,
    gate_hash text not null,
    stage text not null check (stage in ('design-ready', 'implementation-ready', 'close-ready', 'resume-ready')),
    command text,
    expected_result text not null,
    requirement_keys text,
    gate_text text not null,
    status text not null check (status in ('active', 'superseded', 'accepted_out_of_scope')),
    created_at text not null,
    unique(design_version_id, gate_key)
);

create table if not exists validation_gate_template_requirements (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    validation_gate_template_id integer not null references validation_gate_templates(id) on delete cascade,
    design_requirement_id integer not null references design_requirements(id) on delete cascade,
    unique(validation_gate_template_id, design_requirement_id)
);

create table if not exists checklists (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    work_unit_id integer not null references work_units(id) on delete cascade,
    design_version_id integer not null references design_versions(id) on delete cascade,
    title text not null,
    status text not null default 'active' check (status in ('active', 'stale', 'closed')),
    created_by_review_run_id integer,
    created_at text not null
);

create table if not exists checklist_items (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    checklist_id integer not null references checklists(id) on delete cascade,
    design_requirement_id integer not null references design_requirements(id) on delete cascade,
    task_id integer not null references tasks(id) on delete cascade,
    item_order integer not null,
    title text not null,
    completion_condition text,
    status text not null default 'open' check (status in ('open', 'blocked', 'closed', 'accepted_out_of_scope')),
    unique(checklist_id, item_order)
);

create table if not exists task_derivations (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    design_requirement_id integer not null references design_requirements(id) on delete cascade,
    task_id integer not null references tasks(id) on delete cascade,
    checklist_item_id integer references checklist_items(id) on delete set null,
    derivation_reason text,
    generated_by_review_run_id integer,
    status text not null default 'active' check (status in ('active', 'stale', 'closed')),
    created_at text not null,
    unique(design_requirement_id, task_id)
);

create table if not exists validation_gates (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    gate_key text not null,
    template_id integer references validation_gate_templates(id),
    work_unit_id integer references work_units(id) on delete cascade,
    task_id integer references tasks(id) on delete cascade,
    design_requirement_id integer references design_requirements(id) on delete cascade,
    command_profile_id integer references command_profiles(id),
    command text,
    expected_result text not null,
    environment text,
    timeout text,
    artifact_requirements text,
    selected_before_edit integer not null default 1 check (selected_before_edit in (0, 1)),
    status text not null default 'active' check (status in ('active', 'stale', 'closed')),
    created_at text not null
);

create table if not exists validation_runs (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    validation_gate_id integer not null references validation_gates(id) on delete cascade,
    work_unit_id integer references work_units(id) on delete cascade,
    task_id integer references tasks(id) on delete cascade,
    command_usage_id integer references command_usages(id),
    repository_snapshot_id integer,
    result text not null check (result in (
        'pass', 'fail', 'timeout', 'cancelled', 'unknown',
        'expected_red', 'oom', 'non_strict_observation', 'evidence_gap'
    )),
    command text,
    classification text check (classification in (
        'none', 'classified_failure', 'evidence_gap', 'accepted_exception'
    )),
    acceptance_record_id integer references acceptance_records(id),
    artifact_path text,
    artifact_hash text,
    notes text,
    created_at text not null
);

create table if not exists artifacts (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    artifact_type text not null check (artifact_type in ('validation_output', 'test_report', 'build_output', 'generated_file', 'other')),
    identity_key text not null,
    artifact_path text,
    artifact_hash text,
    validation_run_id integer references validation_runs(id) on delete cascade,
    command_usage_id integer references command_usages(id),
    repository_snapshot_id integer,
    created_at text not null,
    check (artifact_path is not null or artifact_hash is not null)
);

create trigger if not exists trg_validation_run_project_insert
before insert on validation_runs
for each row
when new.project_id != (select project_id from validation_gates where id = new.validation_gate_id)
  or new.work_unit_id is not (select work_unit_id from validation_gates where id = new.validation_gate_id)
  or new.task_id is not (select task_id from validation_gates where id = new.validation_gate_id)
  or (new.work_unit_id is not null and new.project_id != (select project_id from work_units where id = new.work_unit_id))
  or (new.task_id is not null and (
      not exists (select 1 from tasks where id = new.task_id)
      or (select work_unit_id from tasks where id = new.task_id) is null
      or new.project_id != (
          select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)
      )
  ))
  or (new.command_usage_id is not null and (
      not exists (select 1 from command_usages where id = new.command_usage_id)
      or new.project_id != (select project_id from command_usages where id = new.command_usage_id)
      or (
          (select work_unit_id from command_usages where id = new.command_usage_id) is not null
          and (select work_unit_id from command_usages where id = new.command_usage_id) is not new.work_unit_id
      )
  ))
  or (new.repository_snapshot_id is not null and (
      not exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
      or new.project_id != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
  ))
  or (
      new.command_usage_id is not null
      and new.repository_snapshot_id is not null
      and (select repository_snapshot_id from command_usages where id = new.command_usage_id) is not null
      and new.repository_snapshot_id != (select repository_snapshot_id from command_usages where id = new.command_usage_id)
  )
begin
    select raise(abort, 'validation run project_id must match referenced rows');
end;

create trigger if not exists trg_validation_run_project_update
before update of project_id, validation_gate_id, work_unit_id, task_id, command_usage_id, repository_snapshot_id on validation_runs
for each row
when new.project_id != (select project_id from validation_gates where id = new.validation_gate_id)
  or new.work_unit_id is not (select work_unit_id from validation_gates where id = new.validation_gate_id)
  or new.task_id is not (select task_id from validation_gates where id = new.validation_gate_id)
  or (new.work_unit_id is not null and new.project_id != (select project_id from work_units where id = new.work_unit_id))
  or (new.task_id is not null and (
      not exists (select 1 from tasks where id = new.task_id)
      or (select work_unit_id from tasks where id = new.task_id) is null
      or new.project_id != (
          select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)
      )
  ))
  or (new.command_usage_id is not null and (
      not exists (select 1 from command_usages where id = new.command_usage_id)
      or new.project_id != (select project_id from command_usages where id = new.command_usage_id)
      or (
          (select work_unit_id from command_usages where id = new.command_usage_id) is not null
          and (select work_unit_id from command_usages where id = new.command_usage_id) is not new.work_unit_id
      )
  ))
  or (new.repository_snapshot_id is not null and (
      not exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
      or new.project_id != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
  ))
  or (
      new.command_usage_id is not null
      and new.repository_snapshot_id is not null
      and (select repository_snapshot_id from command_usages where id = new.command_usage_id) is not null
      and new.repository_snapshot_id != (select repository_snapshot_id from command_usages where id = new.command_usage_id)
  )
begin
    select raise(abort, 'validation run project_id must match referenced rows');
end;

create trigger if not exists trg_artifact_project_insert
before insert on artifacts
for each row
when (new.validation_run_id is not null and (
      not exists (select 1 from validation_runs where id = new.validation_run_id)
      or new.project_id != (select project_id from validation_runs where id = new.validation_run_id)
  ))
  or (new.command_usage_id is not null and (
      not exists (select 1 from command_usages where id = new.command_usage_id)
      or new.project_id != (select project_id from command_usages where id = new.command_usage_id)
  ))
  or (new.repository_snapshot_id is not null and (
      not exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
      or new.project_id != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
  ))
  or (
      new.validation_run_id is not null
      and new.command_usage_id is not (
          select command_usage_id from validation_runs where id = new.validation_run_id
      )
  )
  or (
      new.validation_run_id is not null
      and new.repository_snapshot_id is not (
          select repository_snapshot_id from validation_runs where id = new.validation_run_id
      )
  )
begin
    select raise(abort, 'artifact project_id must match referenced rows');
end;

create trigger if not exists trg_artifact_project_update
before update of project_id, validation_run_id, command_usage_id, repository_snapshot_id on artifacts
for each row
when (new.validation_run_id is not null and (
      not exists (select 1 from validation_runs where id = new.validation_run_id)
      or new.project_id != (select project_id from validation_runs where id = new.validation_run_id)
  ))
  or (new.command_usage_id is not null and (
      not exists (select 1 from command_usages where id = new.command_usage_id)
      or new.project_id != (select project_id from command_usages where id = new.command_usage_id)
  ))
  or (new.repository_snapshot_id is not null and (
      not exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
      or new.project_id != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
  ))
  or (
      new.validation_run_id is not null
      and new.command_usage_id is not (
          select command_usage_id from validation_runs where id = new.validation_run_id
      )
  )
  or (
      new.validation_run_id is not null
      and new.repository_snapshot_id is not (
          select repository_snapshot_id from validation_runs where id = new.validation_run_id
      )
  )
begin
    select raise(abort, 'artifact project_id must match referenced rows');
end;

create table if not exists implementation_evidence (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    task_id integer references tasks(id) on delete cascade,
    design_requirement_id integer references design_requirements(id) on delete cascade,
    evidence_type text not null check (evidence_type in ('commit', 'file', 'symbol', 'test', 'artifact', 'command_output', 'manual_note')),
    repository_id integer,
    git_commit_id integer,
    git_file_change_id integer,
    commit_sha text,
    file_path text,
    line_ref text,
    symbol text,
    artifact_path text,
    note text,
    created_at text not null,
    check (task_id is not null or design_requirement_id is not null)
);

create trigger if not exists trg_implementation_evidence_git_insert
before insert on implementation_evidence
for each row
when (new.repository_id is not null and (
      not exists (select 1 from repositories where id = new.repository_id)
      or new.project_id != (select project_id from repositories where id = new.repository_id)
  ))
  or (new.git_commit_id is not null and (
      not exists (select 1 from git_commits where id = new.git_commit_id)
      or new.project_id != (
          select r.project_id
          from git_commits c
          join repositories r on r.id = c.repository_id
          where c.id = new.git_commit_id
      )
      or (new.repository_id is not null and new.repository_id != (
          select repository_id from git_commits where id = new.git_commit_id
      ))
      or (new.commit_sha is not null and new.commit_sha != (
          select commit_sha from git_commits where id = new.git_commit_id
      ))
  ))
  or (new.git_file_change_id is not null and (
      not exists (select 1 from git_file_changes where id = new.git_file_change_id)
      or new.project_id != (
          select r.project_id
          from git_file_changes f
          join repositories r on r.id = f.repository_id
          where f.id = new.git_file_change_id
      )
      or (new.repository_id is not null and new.repository_id != (
          select repository_id from git_file_changes where id = new.git_file_change_id
      ))
      or (new.git_commit_id is not null and new.git_commit_id != (
          select git_commit_id from git_file_changes where id = new.git_file_change_id
      ))
      or (new.file_path is not null and new.file_path != (
          select path from git_file_changes where id = new.git_file_change_id
      ))
  ))
begin
    select raise(abort, 'implementation evidence git links must match project and paths');
end;

create trigger if not exists trg_implementation_evidence_git_update
before update of project_id, repository_id, git_commit_id, git_file_change_id, commit_sha, file_path on implementation_evidence
for each row
when (new.repository_id is not null and (
      not exists (select 1 from repositories where id = new.repository_id)
      or new.project_id != (select project_id from repositories where id = new.repository_id)
  ))
  or (new.git_commit_id is not null and (
      not exists (select 1 from git_commits where id = new.git_commit_id)
      or new.project_id != (
          select r.project_id
          from git_commits c
          join repositories r on r.id = c.repository_id
          where c.id = new.git_commit_id
      )
      or (new.repository_id is not null and new.repository_id != (
          select repository_id from git_commits where id = new.git_commit_id
      ))
      or (new.commit_sha is not null and new.commit_sha != (
          select commit_sha from git_commits where id = new.git_commit_id
      ))
  ))
  or (new.git_file_change_id is not null and (
      not exists (select 1 from git_file_changes where id = new.git_file_change_id)
      or new.project_id != (
          select r.project_id
          from git_file_changes f
          join repositories r on r.id = f.repository_id
          where f.id = new.git_file_change_id
      )
      or (new.repository_id is not null and new.repository_id != (
          select repository_id from git_file_changes where id = new.git_file_change_id
      ))
      or (new.git_commit_id is not null and new.git_commit_id != (
          select git_commit_id from git_file_changes where id = new.git_file_change_id
      ))
      or (new.file_path is not null and new.file_path != (
          select path from git_file_changes where id = new.git_file_change_id
      ))
  ))
begin
    select raise(abort, 'implementation evidence git links must match project and paths');
end;

create table if not exists coverage_items (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    review_scope_id integer,
    work_unit_id integer references work_units(id) on delete cascade,
    design_requirement_id integer not null references design_requirements(id) on delete cascade,
    task_id integer references tasks(id) on delete cascade,
    requirement text not null,
    runtime_boundary_evidence text,
    ux_boundary_evidence text,
    lifecycle_boundary_evidence text,
    tests_or_gates text,
    missing_or_unverified text,
    status text not null check (status in ('covered', 'partial', 'missing_required_surface', 'design_conflict', 'accepted_out_of_scope', 'needs_evidence', 'stale')),
    created_at text not null
);

create trigger if not exists trg_gate_template_requirement_project_insert
before insert on validation_gate_template_requirements
for each row
when new.project_id != (select project_id from validation_gate_templates where id = new.validation_gate_template_id)
  or new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
begin
    select raise(abort, 'validation gate template requirement project_id must match referenced rows');
end;

create trigger if not exists trg_gate_template_requirement_project_update
before update of project_id, validation_gate_template_id, design_requirement_id on validation_gate_template_requirements
for each row
when new.project_id != (select project_id from validation_gate_templates where id = new.validation_gate_template_id)
  or new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
begin
    select raise(abort, 'validation gate template requirement project_id must match referenced rows');
end;

create trigger if not exists trg_checklist_project_insert
before insert on checklists
for each row
when new.project_id != (select project_id from work_units where id = new.work_unit_id)
  or new.project_id != (select project_id from design_versions where id = new.design_version_id)
begin
    select raise(abort, 'checklist project_id must match referenced rows');
end;

create trigger if not exists trg_checklist_project_update
before update of project_id, work_unit_id, design_version_id on checklists
for each row
when new.project_id != (select project_id from work_units where id = new.work_unit_id)
  or new.project_id != (select project_id from design_versions where id = new.design_version_id)
begin
    select raise(abort, 'checklist project_id must match referenced rows');
end;

create trigger if not exists trg_checklist_item_project_insert
before insert on checklist_items
for each row
when new.project_id != (select project_id from checklists where id = new.checklist_id)
  or new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
  or new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  )
begin
    select raise(abort, 'checklist item project_id must match referenced rows');
end;

create trigger if not exists trg_checklist_item_project_update
before update of project_id, checklist_id, design_requirement_id, task_id on checklist_items
for each row
when new.project_id != (select project_id from checklists where id = new.checklist_id)
  or new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
  or new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  )
begin
    select raise(abort, 'checklist item project_id must match referenced rows');
end;

create trigger if not exists trg_task_derivation_project_insert
before insert on task_derivations
for each row
when new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
  or new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  )
  or (new.checklist_item_id is not null and new.project_id != (select project_id from checklist_items where id = new.checklist_item_id))
begin
    select raise(abort, 'task derivation project_id must match referenced rows');
end;

create trigger if not exists trg_task_derivation_project_update
before update of project_id, design_requirement_id, task_id, checklist_item_id on task_derivations
for each row
when new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
  or new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  )
  or (new.checklist_item_id is not null and new.project_id != (select project_id from checklist_items where id = new.checklist_item_id))
begin
    select raise(abort, 'task derivation project_id must match referenced rows');
end;

create trigger if not exists trg_validation_gate_project_insert
before insert on validation_gates
for each row
when (new.template_id is not null and new.project_id != (select project_id from validation_gate_templates where id = new.template_id))
  or (new.work_unit_id is not null and new.project_id != (select project_id from work_units where id = new.work_unit_id))
  or (new.task_id is not null and new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
  or (new.design_requirement_id is not null and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id))
begin
    select raise(abort, 'validation gate project_id must match referenced rows');
end;

create trigger if not exists trg_validation_gate_project_update
before update of project_id, template_id, work_unit_id, task_id, design_requirement_id on validation_gates
for each row
when (new.template_id is not null and new.project_id != (select project_id from validation_gate_templates where id = new.template_id))
  or (new.work_unit_id is not null and new.project_id != (select project_id from work_units where id = new.work_unit_id))
  or (new.task_id is not null and new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
  or (new.design_requirement_id is not null and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id))
begin
    select raise(abort, 'validation gate project_id must match referenced rows');
end;

create trigger if not exists trg_implementation_evidence_project_insert
before insert on implementation_evidence
for each row
when (new.task_id is not null and (
      not exists (select 1 from tasks where id = new.task_id)
      or (select work_unit_id from tasks where id = new.task_id) is null
      or new.project_id != (
          select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)
      )
  ))
  or (new.design_requirement_id is not null and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id))
begin
    select raise(abort, 'implementation evidence project_id must match referenced rows');
end;

create trigger if not exists trg_implementation_evidence_project_update
before update of project_id, task_id, design_requirement_id on implementation_evidence
for each row
when (new.task_id is not null and (
      not exists (select 1 from tasks where id = new.task_id)
      or (select work_unit_id from tasks where id = new.task_id) is null
      or new.project_id != (
          select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)
      )
  ))
  or (new.design_requirement_id is not null and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id))
begin
    select raise(abort, 'implementation evidence project_id must match referenced rows');
end;

create trigger if not exists trg_coverage_item_project_insert
before insert on coverage_items
for each row
when (new.work_unit_id is not null and new.project_id != (select project_id from work_units where id = new.work_unit_id))
  or new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
  or (new.task_id is not null and new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
begin
    select raise(abort, 'coverage item project_id must match referenced rows');
end;

create trigger if not exists trg_coverage_item_project_update
before update of project_id, work_unit_id, design_requirement_id, task_id on coverage_items
for each row
when (new.work_unit_id is not null and new.project_id != (select project_id from work_units where id = new.work_unit_id))
  or new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
  or (new.task_id is not null and new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
begin
    select raise(abort, 'coverage item project_id must match referenced rows');
end;

create trigger if not exists trg_acceptance_design_requirement_project_insert
before insert on acceptance_records
for each row
when new.target_type = 'design_requirement'
 and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
begin
    select raise(abort, 'acceptance project_id must match design requirement project_id');
end;

create trigger if not exists trg_acceptance_design_requirement_project_update
before update of project_id, target_type, design_requirement_id on acceptance_records
for each row
when new.target_type = 'design_requirement'
 and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
begin
    select raise(abort, 'acceptance project_id must match design requirement project_id');
end;

create trigger if not exists trg_acceptance_task_project_insert
before insert on acceptance_records
for each row
when new.target_type = 'task'
 and new.project_id != coalesce(
     (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
     (select id from projects order by id limit 1)
 )
begin
    select raise(abort, 'acceptance project_id must match task project_id');
end;

create trigger if not exists trg_acceptance_task_project_update
before update of project_id, target_type, task_id on acceptance_records
for each row
when new.target_type = 'task'
 and new.project_id != coalesce(
     (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
     (select id from projects order by id limit 1)
 )
begin
    select raise(abort, 'acceptance project_id must match task project_id');
end;

create trigger if not exists trg_acceptance_validation_gate_template_project_insert
before insert on acceptance_records
for each row
when new.target_type = 'validation_gate_template'
 and new.project_id != (select project_id from validation_gate_templates where id = new.validation_gate_template_id)
begin
    select raise(abort, 'acceptance project_id must match validation gate template project_id');
end;

create trigger if not exists trg_acceptance_validation_gate_template_project_update
before update of project_id, target_type, validation_gate_template_id on acceptance_records
for each row
when new.target_type = 'validation_gate_template'
 and new.project_id != (select project_id from validation_gate_templates where id = new.validation_gate_template_id)
begin
    select raise(abort, 'acceptance project_id must match validation gate template project_id');
end;

create trigger if not exists trg_acceptance_coverage_item_project_insert
before insert on acceptance_records
for each row
when new.target_type = 'coverage_item'
 and new.project_id != (select project_id from coverage_items where id = new.coverage_item_id)
begin
    select raise(abort, 'acceptance project_id must match coverage item project_id');
end;

create trigger if not exists trg_acceptance_coverage_item_project_update
before update of project_id, target_type, coverage_item_id on acceptance_records
for each row
when new.target_type = 'coverage_item'
 and new.project_id != (select project_id from coverage_items where id = new.coverage_item_id)
begin
    select raise(abort, 'acceptance project_id must match coverage item project_id');
end;

create table if not exists review_policies (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    name text not null,
    review_type text not null check (review_type in ('design_review', 'design_implementation_diff', 'design_task_decomposition', 'implementation_review', 'general')),
    max_fresh_agents integer not null default 1,
    max_resume_agents integer not null default 1,
    max_parallel_agents integer not null default 1,
    required_consecutive_clean_fresh_runs integer not null default 1,
    required_consecutive_clean_resume_runs integer not null default 1,
    stop_on_severity text not null default 'none' check (stop_on_severity in ('critical', 'high', 'medium', 'low', 'none')),
    allow_resume_review integer not null default 1 check (allow_resume_review in (0, 1)),
    allow_fresh_review integer not null default 1 check (allow_fresh_review in (0, 1)),
    allow_new_findings_in_resume integer not null default 0 check (allow_new_findings_in_resume in (0, 1)),
    on_max_agents_exceeded text not null default 'block' check (on_max_agents_exceeded in ('block', 'accept_with_user_approval', 'mark_exhausted')),
    run_count_scope text not null default 'review_plan' check (run_count_scope in ('review_plan', 'review_scope', 'work_unit')),
    default_run_mode text not null default 'fresh' check (default_run_mode in ('fresh', 'resume')),
    created_at text not null,
    unique(project_id, name)
);

create table if not exists review_scopes (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    name text not null,
    review_type text not null check (review_type in ('design_review', 'design_implementation_diff', 'design_task_decomposition', 'implementation_review', 'general')),
    agent_role text not null check (agent_role in ('general', 'design_document_review', 'design_task_decomposition', 'design_implementation_diff_review', 'implementation_review')),
    user_declared_scope text not null,
    allowed_inputs text,
    forbidden_judgments text,
    expected_output_type text,
    exclusions text,
    prompt_template_ref text,
    status text not null default 'open' check (status in ('open', 'blocked', 'clean', 'closed')),
    no_findings_streak integer not null default 0,
    created_at text not null,
    unique(project_id, name)
);

create table if not exists review_plans (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    work_unit_id integer not null references work_units(id) on delete cascade,
    design_version_id integer references design_versions(id) on delete cascade,
    review_type text not null check (review_type in ('design_review', 'design_implementation_diff', 'design_task_decomposition', 'implementation_review', 'general')),
    required integer not null default 1 check (required in (0, 1)),
    stage text not null check (stage in ('design-ready', 'implementation-ready', 'close-ready', 'resume-ready')),
    scope text,
    clean_condition text,
    stop_condition text,
    review_policy_id integer not null references review_policies(id),
    review_scope_id integer references review_scopes(id),
    status text not null default 'open' check (status in ('open', 'blocked', 'clean', 'accepted_exception', 'not_required', 'exhausted', 'needs_user_decision')),
    created_at text not null
);

create trigger if not exists trg_review_policy_referenced_update
before update of project_id, review_type on review_policies
for each row
when exists (
    select 1
    from review_plans p
    where p.review_policy_id = old.id
      and (p.project_id != new.project_id or p.review_type != new.review_type)
)
begin
    select raise(abort, 'review policy update would break referenced review plans');
end;

create trigger if not exists trg_review_policy_resume_findings_update
before update of allow_new_findings_in_resume on review_policies
for each row
when new.allow_new_findings_in_resume = 0
  and exists (
      select 1
      from review_plans p
      join review_runs r on r.review_plan_id = p.id
      left join findings f on f.review_run_id = r.id
      where p.review_policy_id = old.id
        and r.run_type = 'resume'
        and (r.new_findings_count > 0 or f.id is not null)
  )
begin
    select raise(abort, 'review policy update would conflict with resume findings');
end;

create trigger if not exists trg_review_scope_referenced_update
before update of project_id, review_type on review_scopes
for each row
when exists (
    select 1
    from review_plans p
    where p.review_scope_id = old.id
      and (p.project_id != new.project_id or p.review_type != new.review_type)
)
or exists (
    select 1
    from review_runs r
    where r.review_scope_id = old.id
      and r.project_id != new.project_id
)
begin
    select raise(abort, 'review scope update would break referenced review plans or runs');
end;

create table if not exists review_plan_targets (
    id integer primary key,
    review_plan_id integer not null references review_plans(id) on delete cascade,
    target_type text not null check (target_type in ('design_version', 'design_requirement', 'task', 'work_unit', 'phase', 'repository_snapshot', 'file', 'symbol')),
    design_version_id integer references design_versions(id),
    design_requirement_id integer references design_requirements(id),
    task_id integer references tasks(id),
    work_unit_id integer references work_units(id),
    phase_id integer references work_phases(id),
    repository_snapshot_id integer,
    file_path text,
    symbol text,
    check (
        (target_type = 'design_version' and design_version_id is not null and design_requirement_id is null and task_id is null and work_unit_id is null and repository_snapshot_id is null and file_path is null and symbol is null)
        or (target_type = 'design_requirement' and design_version_id is null and design_requirement_id is not null and task_id is null and work_unit_id is null and repository_snapshot_id is null and file_path is null and symbol is null)
        or (target_type = 'task' and design_version_id is null and design_requirement_id is null and task_id is not null and work_unit_id is null and repository_snapshot_id is null and file_path is null and symbol is null)
        or (target_type = 'work_unit' and design_version_id is null and design_requirement_id is null and task_id is null and work_unit_id is not null and repository_snapshot_id is null and file_path is null and symbol is null)
        or (target_type = 'repository_snapshot' and design_version_id is null and design_requirement_id is null and task_id is null and work_unit_id is null and repository_snapshot_id is not null and file_path is null and symbol is null)
        or (target_type = 'file' and design_version_id is null and design_requirement_id is null and task_id is null and work_unit_id is null and repository_snapshot_id is null and file_path is not null and symbol is null)
        or (target_type = 'symbol' and design_version_id is null and design_requirement_id is null and task_id is null and work_unit_id is null and repository_snapshot_id is null and file_path is null and symbol is not null)
    )
);

create table if not exists review_runs (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    review_scope_id integer references review_scopes(id),
    review_plan_id integer not null references review_plans(id),
    run_type text not null check (run_type in ('fresh', 'resume', 'coverage')),
    run_purpose text not null check (run_purpose in ('new_unbiased_review', 'finding_fix_verification', 'coverage_audit')),
    target_type text not null check (target_type in ('design_version', 'design_requirement', 'task', 'work_unit', 'repository_snapshot', 'file', 'symbol')),
    design_version_id integer references design_versions(id),
    design_requirement_id integer references design_requirements(id),
    task_id integer references tasks(id),
    work_unit_id integer references work_units(id),
    repository_snapshot_id integer,
    file_path text,
    symbol text,
    target_ref text,
    prompt_deviations text,
    result_summary text,
    new_findings_count integer not null default 0 check (new_findings_count >= 0),
    carried_findings_checked integer not null default 0 check (carried_findings_checked >= 0),
    clean_run integer not null default 0 check (clean_run in (0, 1)),
    review_provenance text not null default 'self_recorded' check (review_provenance in ('self_recorded', 'external_agent', 'human_review')),
    review_provenance_ref text,
    status text not null default 'requested' check (status in ('requested', 'running', 'completed', 'failed', 'cancelled')),
    created_at text not null,
    check (
        (run_type = 'fresh' and run_purpose = 'new_unbiased_review')
        or (run_type = 'resume' and run_purpose = 'finding_fix_verification')
        or (run_type = 'coverage' and run_purpose = 'coverage_audit')
    ),
    check (
        clean_run = 0
        or (status = 'completed' and new_findings_count = 0)
    )
);

create table if not exists review_agent_invocations (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    review_plan_id integer references review_plans(id),
    review_run_id integer references review_runs(id),
    run_type text not null check (run_type in ('fresh', 'resume', 'coverage')),
    agent_label text,
    external_agent_id text,
    status text not null default 'requested' check (status in ('requested', 'running', 'completed', 'failed', 'cancelled')),
    started_at text,
    finished_at text
);

create table if not exists findings (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    review_run_id integer not null references review_runs(id) on delete cascade,
    finding_type text not null check (finding_type in ('design_finding', 'design_implementation_drift', 'design_task_gap', 'implementation_finding', 'coverage_finding')),
    severity text not null check (severity in ('critical', 'high', 'medium', 'low')),
    description text not null,
    classification text not null default 'unclassified' check (classification in ('unclassified', 'valid', 'invalid', 'design_conflict', 'needs_evidence')),
    status text not null default 'open' check (status in ('open', 'closed', 'accepted_out_of_scope')),
    design_requirement_id integer references design_requirements(id),
    task_id integer references tasks(id),
    created_at text not null
);

create trigger if not exists trg_finding_review_type_insert
before insert on findings
for each row
when not exists (
    select 1
    from review_runs r
    join review_plans p on p.id = r.review_plan_id
    where r.id = new.review_run_id
      and (
          (p.review_type = 'design_review' and new.finding_type = 'design_finding')
          or (p.review_type = 'design_implementation_diff' and new.finding_type = 'design_implementation_drift')
          or (p.review_type = 'design_task_decomposition' and new.finding_type = 'design_task_gap')
          or (p.review_type = 'implementation_review' and new.finding_type in ('implementation_finding', 'coverage_finding'))
          or p.review_type = 'general'
      )
)
begin
    select raise(abort, 'finding type must match review type');
end;

create trigger if not exists trg_finding_review_type_update
before update of review_run_id, finding_type on findings
for each row
when not exists (
    select 1
    from review_runs r
    join review_plans p on p.id = r.review_plan_id
    where r.id = new.review_run_id
      and (
          (p.review_type = 'design_review' and new.finding_type = 'design_finding')
          or (p.review_type = 'design_implementation_diff' and new.finding_type = 'design_implementation_drift')
          or (p.review_type = 'design_task_decomposition' and new.finding_type = 'design_task_gap')
          or (p.review_type = 'implementation_review' and new.finding_type in ('implementation_finding', 'coverage_finding'))
          or p.review_type = 'general'
      )
)
begin
    select raise(abort, 'finding type must match review type');
end;

create table if not exists closures (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    finding_id integer not null references findings(id) on delete cascade,
    design_invariant text not null,
    design_citations text,
    implementation_evidence text,
    affected_surfaces text,
    same_invariant_search text,
    other_violations_found text,
    fix_plan text,
    tests_or_gates text,
    verification_plan text,
    closed_by_commit text,
    created_at text not null
);

create table if not exists finding_verifications (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    review_run_id integer not null references review_runs(id) on delete cascade,
    finding_id integer not null references findings(id) on delete cascade,
    closure_id integer not null references closures(id) on delete cascade,
    result text not null check (result in ('verified', 'not_fixed', 'needs_evidence', 'out_of_scope')),
    notes text,
    created_at text not null,
    unique(review_run_id, finding_id, closure_id)
);

create trigger if not exists trg_review_plan_policy_required_insert
before insert on review_plans
for each row
when new.review_policy_id is null
begin
    select raise(abort, 'review plan requires review policy');
end;

create trigger if not exists trg_review_plan_policy_required_update
before update of review_policy_id on review_plans
for each row
when new.review_policy_id is null
begin
    select raise(abort, 'review plan requires review policy');
end;

create trigger if not exists trg_review_plan_project_insert
before insert on review_plans
for each row
when new.project_id != (select project_id from work_units where id = new.work_unit_id)
  or (new.design_version_id is not null and new.project_id != (select project_id from design_versions where id = new.design_version_id))
  or (new.review_policy_id is not null and new.project_id != (select project_id from review_policies where id = new.review_policy_id))
  or (new.review_scope_id is not null and new.project_id != (select project_id from review_scopes where id = new.review_scope_id))
begin
    select raise(abort, 'review plan project_id must match referenced rows');
end;

create trigger if not exists trg_review_plan_project_update
before update of project_id, work_unit_id, design_version_id, review_policy_id, review_scope_id on review_plans
for each row
when new.project_id != (select project_id from work_units where id = new.work_unit_id)
  or (new.design_version_id is not null and new.project_id != (select project_id from design_versions where id = new.design_version_id))
  or (new.review_policy_id is not null and new.project_id != (select project_id from review_policies where id = new.review_policy_id))
  or (new.review_scope_id is not null and new.project_id != (select project_id from review_scopes where id = new.review_scope_id))
begin
    select raise(abort, 'review plan project_id must match referenced rows');
end;

create trigger if not exists trg_review_plan_type_insert
before insert on review_plans
for each row
when (new.review_policy_id is not null and new.review_type != (select review_type from review_policies where id = new.review_policy_id))
  or (new.review_scope_id is not null and new.review_type != (select review_type from review_scopes where id = new.review_scope_id))
begin
    select raise(abort, 'review plan type must match policy and scope type');
end;

create trigger if not exists trg_review_plan_type_update
before update of review_type, review_policy_id, review_scope_id on review_plans
for each row
when (new.review_policy_id is not null and new.review_type != (select review_type from review_policies where id = new.review_policy_id))
  or (new.review_scope_id is not null and new.review_type != (select review_type from review_scopes where id = new.review_scope_id))
begin
    select raise(abort, 'review plan type must match policy and scope type');
end;

create trigger if not exists trg_review_plan_resume_policy_update
before update of review_policy_id on review_plans
for each row
when (select allow_new_findings_in_resume from review_policies where id = new.review_policy_id) = 0
  and exists (
      select 1
      from review_runs r
      left join findings f on f.review_run_id = r.id
      where r.review_plan_id = new.id
        and r.run_type = 'resume'
        and (r.new_findings_count > 0 or f.id is not null)
  )
begin
    select raise(abort, 'review plan policy update would conflict with resume findings');
end;

create trigger if not exists trg_review_run_plan_required_insert
before insert on review_runs
for each row
when new.review_plan_id is null
begin
    select raise(abort, 'review run requires review plan');
end;

create trigger if not exists trg_review_run_plan_required_update
before update of review_plan_id on review_runs
for each row
when new.review_plan_id is null
begin
    select raise(abort, 'review run requires review plan');
end;

create trigger if not exists trg_review_run_project_insert
before insert on review_runs
for each row
when (new.review_scope_id is not null and new.project_id != (select project_id from review_scopes where id = new.review_scope_id))
  or (new.review_plan_id is not null and new.project_id != (select project_id from review_plans where id = new.review_plan_id))
begin
    select raise(abort, 'review run project_id must match referenced rows');
end;

create trigger if not exists trg_review_run_target_insert
before insert on review_runs
for each row
when (new.target_type = 'design_version' and not (
        new.design_version_id is not null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is null
        and new.phase_id is null
        and new.repository_snapshot_id is null
        and new.project_id = (select project_id from design_versions where id = new.design_version_id)
    ))
  or (new.target_type = 'design_requirement' and not (
        new.design_version_id is null
        and new.design_requirement_id is not null
        and new.task_id is null
        and new.work_unit_id is null
        and new.phase_id is null
        and new.repository_snapshot_id is null
        and new.project_id = (select project_id from design_requirements where id = new.design_requirement_id)
    ))
  or (new.target_type = 'task' and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is not null
        and new.work_unit_id is null
        and new.phase_id is null
        and new.repository_snapshot_id is null
        and new.project_id = coalesce(
            (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
            (select id from projects order by id limit 1)
        )
    ))
  or (new.target_type = 'work_unit' and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is not null
        and new.phase_id is null
        and new.repository_snapshot_id is null
        and new.project_id = (select project_id from work_units where id = new.work_unit_id)
    ))
  or (new.target_type = 'phase' and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is null
        and new.phase_id is not null
        and new.repository_snapshot_id is null
        and new.file_path is null
        and new.symbol is null
        and new.project_id = (select project_id from work_phases where id = new.phase_id)
    ))
  or (new.target_type = 'repository_snapshot' and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is null
        and new.phase_id is null
        and new.repository_snapshot_id is not null
        and exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
        and new.project_id = (
            select r.project_id
            from repository_snapshots s
            join repositories r on r.id = s.repository_id
            where s.id = new.repository_snapshot_id
        )
    ))
  or (new.target_type in ('file', 'symbol') and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is null
        and new.phase_id is null
        and new.repository_snapshot_id is null
        and (
            (new.target_type = 'file' and new.file_path is not null and new.symbol is null)
            or (new.target_type = 'symbol' and new.file_path is null and new.symbol is not null)
        )
    ))
begin
    select raise(abort, 'review run target must match target_type and project_id');
end;

create trigger if not exists trg_review_run_project_update
before update of project_id, review_scope_id, review_plan_id on review_runs
for each row
when (new.review_scope_id is not null and new.project_id != (select project_id from review_scopes where id = new.review_scope_id))
  or (new.review_plan_id is not null and new.project_id != (select project_id from review_plans where id = new.review_plan_id))
begin
    select raise(abort, 'review run project_id must match referenced rows');
end;

create trigger if not exists trg_review_run_target_update
before update of project_id, target_type, design_version_id, design_requirement_id, task_id, work_unit_id, phase_id, repository_snapshot_id, file_path, symbol, target_ref on review_runs
for each row
when (new.target_type = 'design_version' and not (
        new.design_version_id is not null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is null
        and new.phase_id is null
        and new.repository_snapshot_id is null
        and new.project_id = (select project_id from design_versions where id = new.design_version_id)
    ))
  or (new.target_type = 'design_requirement' and not (
        new.design_version_id is null
        and new.design_requirement_id is not null
        and new.task_id is null
        and new.work_unit_id is null
        and new.phase_id is null
        and new.repository_snapshot_id is null
        and new.project_id = (select project_id from design_requirements where id = new.design_requirement_id)
    ))
  or (new.target_type = 'task' and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is not null
        and new.work_unit_id is null
        and new.phase_id is null
        and new.repository_snapshot_id is null
        and new.project_id = coalesce(
            (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
            (select id from projects order by id limit 1)
        )
    ))
  or (new.target_type = 'work_unit' and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is not null
        and new.phase_id is null
        and new.repository_snapshot_id is null
        and new.project_id = (select project_id from work_units where id = new.work_unit_id)
    ))
  or (new.target_type = 'phase' and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is null
        and new.phase_id is not null
        and new.repository_snapshot_id is null
        and new.file_path is null
        and new.symbol is null
        and new.project_id = (select project_id from work_phases where id = new.phase_id)
    ))
  or (new.target_type = 'repository_snapshot' and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is null
        and new.phase_id is null
        and new.repository_snapshot_id is not null
        and exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
        and new.project_id = (
            select r.project_id
            from repository_snapshots s
            join repositories r on r.id = s.repository_id
            where s.id = new.repository_snapshot_id
        )
    ))
  or (new.target_type in ('file', 'symbol') and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is null
        and new.phase_id is null
        and new.repository_snapshot_id is null
        and (
            (new.target_type = 'file' and new.file_path is not null and new.symbol is null)
            or (new.target_type = 'symbol' and new.file_path is null and new.symbol is not null)
        )
    ))
begin
    select raise(abort, 'review run target must match target_type and project_id');
end;

create trigger if not exists trg_review_run_plan_target_insert
before insert on review_runs
for each row
when new.review_plan_id is not null
  and (
      (
          new.target_type != 'phase'
          and not exists (
              select 1
              from review_plan_targets t
              where t.review_plan_id = new.review_plan_id
                and (
                    (new.target_type = 'design_version' and t.target_type = 'design_version' and t.design_version_id = new.design_version_id)
                    or (new.target_type = 'design_requirement' and t.target_type = 'design_requirement' and t.design_requirement_id = new.design_requirement_id)
                    or (new.target_type = 'task' and t.target_type = 'task' and t.task_id = new.task_id)
                    or (new.target_type = 'work_unit' and t.target_type = 'work_unit' and t.work_unit_id = new.work_unit_id)
                    or (new.target_type = 'repository_snapshot' and t.target_type = 'repository_snapshot' and t.repository_snapshot_id = new.repository_snapshot_id)
                    or (new.target_type = 'file' and t.target_type = 'file' and t.file_path = new.file_path)
                    or (new.target_type = 'symbol' and t.target_type = 'symbol' and t.symbol = new.symbol)
                )
          )
      )
      or (
          new.target_type = 'phase'
          and not exists (
              select 1
              from work_phase_review_targets t
              where t.review_plan_id = new.review_plan_id
                and t.phase_id = new.phase_id
          )
      )
  )
begin
    select raise(abort, 'review run target must be included in review plan targets');
end;

create trigger if not exists trg_review_run_plan_target_update
before update of review_plan_id, target_type, design_version_id, design_requirement_id, task_id, work_unit_id, phase_id, repository_snapshot_id, file_path, symbol, target_ref on review_runs
for each row
when new.review_plan_id is not null
  and (
      (
          new.target_type != 'phase'
          and not exists (
              select 1
              from review_plan_targets t
              where t.review_plan_id = new.review_plan_id
                and (
                    (new.target_type = 'design_version' and t.target_type = 'design_version' and t.design_version_id = new.design_version_id)
                    or (new.target_type = 'design_requirement' and t.target_type = 'design_requirement' and t.design_requirement_id = new.design_requirement_id)
                    or (new.target_type = 'task' and t.target_type = 'task' and t.task_id = new.task_id)
                    or (new.target_type = 'work_unit' and t.target_type = 'work_unit' and t.work_unit_id = new.work_unit_id)
                    or (new.target_type = 'repository_snapshot' and t.target_type = 'repository_snapshot' and t.repository_snapshot_id = new.repository_snapshot_id)
                    or (new.target_type = 'file' and t.target_type = 'file' and t.file_path = new.file_path)
                    or (new.target_type = 'symbol' and t.target_type = 'symbol' and t.symbol = new.symbol)
                )
          )
      )
      or (
          new.target_type = 'phase'
          and not exists (
              select 1
              from work_phase_review_targets t
              where t.review_plan_id = new.review_plan_id
                and t.phase_id = new.phase_id
          )
      )
  )
begin
    select raise(abort, 'review run target must be included in review plan targets');
end;

create trigger if not exists trg_review_run_type_purpose_insert
before insert on review_runs
for each row
when not (
    (new.run_type = 'fresh' and new.run_purpose = 'new_unbiased_review')
    or (new.run_type = 'resume' and new.run_purpose = 'finding_fix_verification')
    or (new.run_type = 'coverage' and new.run_purpose = 'coverage_audit')
)
begin
    select raise(abort, 'review run type must match purpose');
end;

create trigger if not exists trg_review_run_type_purpose_update
before update of run_type, run_purpose on review_runs
for each row
when not (
    (new.run_type = 'fresh' and new.run_purpose = 'new_unbiased_review')
    or (new.run_type = 'resume' and new.run_purpose = 'finding_fix_verification')
    or (new.run_type = 'coverage' and new.run_purpose = 'coverage_audit')
)
begin
    select raise(abort, 'review run type must match purpose');
end;

create trigger if not exists trg_review_run_resume_policy_insert
before insert on review_runs
for each row
when new.run_type = 'resume'
  and new.new_findings_count > 0
  and (select allow_new_findings_in_resume
       from review_policies
       where id = (select review_policy_id from review_plans where id = new.review_plan_id)) = 0
begin
    select raise(abort, 'new findings are disabled for resume review by policy');
end;

create trigger if not exists trg_review_run_resume_policy_update
before update of review_plan_id, run_type, new_findings_count on review_runs
for each row
when new.run_type = 'resume'
  and (select allow_new_findings_in_resume
       from review_policies
       where id = (select review_policy_id from review_plans where id = new.review_plan_id)) = 0
  and (
      new.new_findings_count > 0
      or exists (select 1 from findings where review_run_id = new.id)
  )
begin
    select raise(abort, 'new findings are disabled for resume review by policy');
end;

create trigger if not exists trg_review_run_result_insert
before insert on review_runs
for each row
when new.new_findings_count < 0
  or new.carried_findings_checked < 0
  or (new.clean_run = 1 and (new.status != 'completed' or new.new_findings_count != 0))
  or new.review_provenance not in ('self_recorded', 'external_agent', 'human_review')
  or (new.review_provenance in ('external_agent', 'human_review') and coalesce(new.review_provenance_ref, '') = '')
begin
    select raise(abort, 'review run result is inconsistent');
end;

create trigger if not exists trg_review_run_result_update
before update of new_findings_count, carried_findings_checked, clean_run, status, review_provenance, review_provenance_ref on review_runs
for each row
when new.new_findings_count < 0
  or new.carried_findings_checked < 0
  or (new.clean_run = 1 and (new.status != 'completed' or new.new_findings_count != 0))
  or new.review_provenance not in ('self_recorded', 'external_agent', 'human_review')
  or (new.review_provenance in ('external_agent', 'human_review') and coalesce(new.review_provenance_ref, '') = '')
  or (new.clean_run = 1 and exists (
      select 1 from findings where review_run_id = new.id
  ))
begin
    select raise(abort, 'review run result is inconsistent');
end;

create trigger if not exists trg_review_plan_target_project_insert
before insert on review_plan_targets
for each row
when (new.target_type = 'design_version' and (select project_id from review_plans where id = new.review_plan_id) != (select project_id from design_versions where id = new.design_version_id))
  or (new.target_type = 'design_requirement' and (select project_id from review_plans where id = new.review_plan_id) != (select project_id from design_requirements where id = new.design_requirement_id))
  or (new.target_type = 'task' and (select project_id from review_plans where id = new.review_plan_id) != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
  or (new.target_type = 'work_unit' and (select project_id from review_plans where id = new.review_plan_id) != (select project_id from work_units where id = new.work_unit_id))
  or (new.target_type = 'repository_snapshot' and (
      not exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
      or (select project_id from review_plans where id = new.review_plan_id) != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
  ))
begin
    select raise(abort, 'review plan target project_id must match review plan project_id');
end;

create trigger if not exists trg_review_plan_target_project_update
before update of review_plan_id, target_type, design_version_id, design_requirement_id, task_id, work_unit_id, repository_snapshot_id on review_plan_targets
for each row
when (new.target_type = 'design_version' and (select project_id from review_plans where id = new.review_plan_id) != (select project_id from design_versions where id = new.design_version_id))
  or (new.target_type = 'design_requirement' and (select project_id from review_plans where id = new.review_plan_id) != (select project_id from design_requirements where id = new.design_requirement_id))
  or (new.target_type = 'task' and (select project_id from review_plans where id = new.review_plan_id) != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
  or (new.target_type = 'work_unit' and (select project_id from review_plans where id = new.review_plan_id) != (select project_id from work_units where id = new.work_unit_id))
  or (new.target_type = 'repository_snapshot' and (
      not exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
      or (select project_id from review_plans where id = new.review_plan_id) != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
  ))
begin
    select raise(abort, 'review plan target project_id must match review plan project_id');
end;

create trigger if not exists trg_review_plan_target_referenced_update
before update of review_plan_id, target_type, design_version_id, design_requirement_id, task_id, work_unit_id, repository_snapshot_id, file_path, symbol on review_plan_targets
for each row
when exists (
    select 1
    from review_runs r
    where r.review_plan_id = old.review_plan_id
      and (
          (r.target_type = 'design_version' and old.target_type = 'design_version' and r.design_version_id = old.design_version_id)
          or (r.target_type = 'design_requirement' and old.target_type = 'design_requirement' and r.design_requirement_id = old.design_requirement_id)
          or (r.target_type = 'task' and old.target_type = 'task' and r.task_id = old.task_id)
          or (r.target_type = 'work_unit' and old.target_type = 'work_unit' and r.work_unit_id = old.work_unit_id)
          or (r.target_type = 'repository_snapshot' and old.target_type = 'repository_snapshot' and r.repository_snapshot_id = old.repository_snapshot_id)
          or (r.target_type = 'file' and old.target_type = 'file' and r.target_ref = old.file_path)
          or (r.target_type = 'symbol' and old.target_type = 'symbol' and r.target_ref = old.symbol)
      )
)
begin
    select raise(abort, 'cannot update review plan target referenced by review runs');
end;

create trigger if not exists trg_review_plan_target_referenced_delete
before delete on review_plan_targets
for each row
when exists (
    select 1
    from review_runs r
    where r.review_plan_id = old.review_plan_id
      and (
          (r.target_type = 'design_version' and old.target_type = 'design_version' and r.design_version_id = old.design_version_id)
          or (r.target_type = 'design_requirement' and old.target_type = 'design_requirement' and r.design_requirement_id = old.design_requirement_id)
          or (r.target_type = 'task' and old.target_type = 'task' and r.task_id = old.task_id)
          or (r.target_type = 'work_unit' and old.target_type = 'work_unit' and r.work_unit_id = old.work_unit_id)
          or (r.target_type = 'repository_snapshot' and old.target_type = 'repository_snapshot' and r.repository_snapshot_id = old.repository_snapshot_id)
          or (r.target_type = 'file' and old.target_type = 'file' and r.target_ref = old.file_path)
          or (r.target_type = 'symbol' and old.target_type = 'symbol' and r.target_ref = old.symbol)
      )
)
begin
    select raise(abort, 'cannot delete review plan target referenced by review runs');
end;

create trigger if not exists trg_finding_project_insert
before insert on findings
for each row
when new.project_id != (select project_id from review_runs where id = new.review_run_id)
  or (new.design_requirement_id is not null and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id))
  or (new.task_id is not null and new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
begin
    select raise(abort, 'finding project_id must match referenced rows');
end;

create trigger if not exists trg_finding_project_update
before update of project_id, review_run_id, design_requirement_id, task_id on findings
for each row
when new.project_id != (select project_id from review_runs where id = new.review_run_id)
  or (new.design_requirement_id is not null and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id))
  or (new.task_id is not null and new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
begin
    select raise(abort, 'finding project_id must match referenced rows');
end;

create trigger if not exists trg_finding_clean_run_insert
before insert on findings
for each row
when (select clean_run from review_runs where id = new.review_run_id) = 1
begin
    select raise(abort, 'cannot attach finding to clean review run');
end;

create trigger if not exists trg_finding_clean_run_update
before update of review_run_id on findings
for each row
when (select clean_run from review_runs where id = new.review_run_id) = 1
begin
    select raise(abort, 'cannot attach finding to clean review run');
end;

create trigger if not exists trg_finding_resume_policy_insert
before insert on findings
for each row
when exists (
    select 1
    from review_runs r
    join review_plans p on p.id = r.review_plan_id
    join review_policies rp on rp.id = p.review_policy_id
    where r.id = new.review_run_id
      and r.run_type = 'resume'
      and rp.allow_new_findings_in_resume = 0
)
begin
    select raise(abort, 'new findings are disabled for resume review by policy');
end;

create trigger if not exists trg_finding_resume_policy_update
before update of review_run_id on findings
for each row
when exists (
    select 1
    from review_runs r
    join review_plans p on p.id = r.review_plan_id
    join review_policies rp on rp.id = p.review_policy_id
    where r.id = new.review_run_id
      and r.run_type = 'resume'
      and rp.allow_new_findings_in_resume = 0
)
begin
    select raise(abort, 'new findings are disabled for resume review by policy');
end;

create trigger if not exists trg_closure_project_insert
before insert on closures
for each row
when new.project_id != (select project_id from findings where id = new.finding_id)
begin
    select raise(abort, 'closure project_id must match finding project_id');
end;

create trigger if not exists trg_closure_project_update
before update of project_id, finding_id on closures
for each row
when new.project_id != (select project_id from findings where id = new.finding_id)
begin
    select raise(abort, 'closure project_id must match finding project_id');
end;

create trigger if not exists trg_finding_verification_project_insert
before insert on finding_verifications
for each row
when new.project_id != (select project_id from review_runs where id = new.review_run_id)
  or new.project_id != (select project_id from findings where id = new.finding_id)
  or new.project_id != (select project_id from closures where id = new.closure_id)
  or new.finding_id != (select finding_id from closures where id = new.closure_id)
  or (select run_type from review_runs where id = new.review_run_id) != 'resume'
  or (select run_purpose from review_runs where id = new.review_run_id) != 'finding_fix_verification'
  or (select review_plan_id from review_runs where id = new.review_run_id) != (
      select source_run.review_plan_id
      from findings f
      join review_runs source_run on source_run.id = f.review_run_id
      where f.id = new.finding_id
  )
begin
    select raise(abort, 'finding verification project_id must match referenced rows');
end;

create trigger if not exists trg_finding_verification_project_update
before update of project_id, review_run_id, finding_id, closure_id on finding_verifications
for each row
when new.project_id != (select project_id from review_runs where id = new.review_run_id)
  or new.project_id != (select project_id from findings where id = new.finding_id)
  or new.project_id != (select project_id from closures where id = new.closure_id)
  or new.finding_id != (select finding_id from closures where id = new.closure_id)
  or (select run_type from review_runs where id = new.review_run_id) != 'resume'
  or (select run_purpose from review_runs where id = new.review_run_id) != 'finding_fix_verification'
  or (select review_plan_id from review_runs where id = new.review_run_id) != (
      select source_run.review_plan_id
      from findings f
      join review_runs source_run on source_run.id = f.review_run_id
      where f.id = new.finding_id
  )
begin
    select raise(abort, 'finding verification project_id must match referenced rows');
end;

create trigger if not exists trg_acceptance_general_project_insert
before insert on acceptance_records
for each row
when (new.target_type = 'finding' and new.project_id != (select project_id from findings where id = new.finding_id))
  or (new.target_type = 'validation_gate' and new.project_id != (select project_id from validation_gates where id = new.validation_gate_id))
  or (new.target_type = 'validation_run' and new.project_id != (select project_id from validation_runs where id = new.validation_run_id))
  or (new.target_type = 'repository_state_classification' and new.project_id != (
      select r.project_id
      from repository_state_classifications c
      join repository_snapshots s on s.id = c.repository_snapshot_id
      join repositories r on r.id = s.repository_id
      where c.id = new.repository_state_classification_id
  ))
  or (new.target_type = 'repository_snapshot_comparison' and (
      new.project_id != (
          select base_repo.project_id
          from repository_snapshot_comparisons c
          join repository_snapshots base on base.id = c.base_repository_snapshot_id
          join repositories base_repo on base_repo.id = base.repository_id
          where c.id = new.repository_snapshot_comparison_id
      )
      or new.project_id != (
          select current_repo.project_id
          from repository_snapshot_comparisons c
          join repository_snapshots current on current.id = c.current_repository_snapshot_id
          join repositories current_repo on current_repo.id = current.repository_id
          where c.id = new.repository_snapshot_comparison_id
      )
  ))
  or (new.target_type = 'review_plan' and new.project_id != (select project_id from review_plans where id = new.review_plan_id))
  or (new.target_type = 'checklist_item' and new.project_id != (select project_id from checklist_items where id = new.checklist_item_id))
  or (new.target_type = 'command_profile' and new.project_id != (select project_id from command_profiles where id = new.command_profile_id))
  or (new.target_type = 'command_usage' and new.project_id != (select project_id from command_usages where id = new.command_usage_id))
  or (new.target_type = 'command_deviation' and new.project_id != (
      select p.project_id
      from command_deviations d
      join command_profiles p on p.id = d.command_profile_id
      where d.id = new.command_deviation_id
  ))
  or (new.target_type = 'rule_binding' and new.project_id != (
      select project_id from rule_bindings where id = new.rule_binding_id
  ))
begin
    select raise(abort, 'acceptance project_id must match general target project_id');
end;

create trigger if not exists trg_acceptance_general_project_update
before update of project_id, target_type, finding_id, validation_gate_id, validation_run_id, repository_state_classification_id, repository_snapshot_comparison_id, review_plan_id, checklist_item_id, command_profile_id, command_usage_id, command_deviation_id, rule_binding_id on acceptance_records
for each row
when (new.target_type = 'finding' and new.project_id != (select project_id from findings where id = new.finding_id))
  or (new.target_type = 'validation_gate' and new.project_id != (select project_id from validation_gates where id = new.validation_gate_id))
  or (new.target_type = 'validation_run' and new.project_id != (select project_id from validation_runs where id = new.validation_run_id))
  or (new.target_type = 'repository_state_classification' and new.project_id != (
      select r.project_id
      from repository_state_classifications c
      join repository_snapshots s on s.id = c.repository_snapshot_id
      join repositories r on r.id = s.repository_id
      where c.id = new.repository_state_classification_id
  ))
  or (new.target_type = 'repository_snapshot_comparison' and (
      new.project_id != (
          select base_repo.project_id
          from repository_snapshot_comparisons c
          join repository_snapshots base on base.id = c.base_repository_snapshot_id
          join repositories base_repo on base_repo.id = base.repository_id
          where c.id = new.repository_snapshot_comparison_id
      )
      or new.project_id != (
          select current_repo.project_id
          from repository_snapshot_comparisons c
          join repository_snapshots current on current.id = c.current_repository_snapshot_id
          join repositories current_repo on current_repo.id = current.repository_id
          where c.id = new.repository_snapshot_comparison_id
      )
  ))
  or (new.target_type = 'review_plan' and new.project_id != (select project_id from review_plans where id = new.review_plan_id))
  or (new.target_type = 'checklist_item' and new.project_id != (select project_id from checklist_items where id = new.checklist_item_id))
  or (new.target_type = 'command_profile' and new.project_id != (select project_id from command_profiles where id = new.command_profile_id))
  or (new.target_type = 'command_usage' and new.project_id != (select project_id from command_usages where id = new.command_usage_id))
  or (new.target_type = 'command_deviation' and new.project_id != (
      select p.project_id
      from command_deviations d
      join command_profiles p on p.id = d.command_profile_id
      where d.id = new.command_deviation_id
  ))
  or (new.target_type = 'rule_binding' and new.project_id != (
      select project_id from rule_bindings where id = new.rule_binding_id
  ))
begin
    select raise(abort, 'acceptance project_id must match general target project_id');
end;

create table if not exists kpt_reviews (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    scope text,
    period_start text,
    period_end text,
    trigger text not null default 'manual' check (trigger in ('manual', 'work_unit_close', 'review_close', 'scheduled')),
    summary text,
    status text not null default 'open' check (status in ('open', 'closed')),
    created_at text not null,
    closed_at text
);

create table if not exists kpt_items (
    id integer primary key,
    kpt_review_id integer not null references kpt_reviews(id) on delete cascade,
    item_type text not null check (item_type in ('keep', 'problem', 'try')),
    title text not null,
    details text,
    severity text not null default 'medium' check (severity in ('critical', 'high', 'medium', 'low')),
    linked_user_correction_id integer references user_corrections(id),
    linked_command_profile_id integer references command_profiles(id),
    linked_review_finding_id integer references findings(id),
    linked_task_id integer references tasks(id),
    proposed_action text,
    status text not null default 'open' check (status in ('open', 'accepted', 'converted', 'converted_to_task', 'dismissed')),
    created_at text not null
);

create table if not exists kpt_item_conversions (
    id integer primary key,
    kpt_item_id integer not null references kpt_items(id) on delete cascade,
    target_type text not null check (target_type in ('task', 'command_profile', 'review_policy', 'design_version', 'decision', 'user_correction')),
    task_id integer references tasks(id),
    command_profile_id integer references command_profiles(id),
    review_policy_id integer references review_policies(id),
    design_version_id integer references design_versions(id),
    decision_id integer references decisions(id),
    user_correction_id integer references user_corrections(id),
    created_at text not null,
    check (
        (target_type = 'task' and task_id is not null and command_profile_id is null and review_policy_id is null and design_version_id is null and decision_id is null and user_correction_id is null)
        or (target_type = 'command_profile' and task_id is null and command_profile_id is not null and review_policy_id is null and design_version_id is null and decision_id is null and user_correction_id is null)
        or (target_type = 'review_policy' and task_id is null and command_profile_id is null and review_policy_id is not null and design_version_id is null and decision_id is null and user_correction_id is null)
        or (target_type = 'design_version' and task_id is null and command_profile_id is null and review_policy_id is null and design_version_id is not null and decision_id is null and user_correction_id is null)
        or (target_type = 'decision' and task_id is null and command_profile_id is null and review_policy_id is null and design_version_id is null and decision_id is not null and user_correction_id is null)
        or (target_type = 'user_correction' and task_id is null and command_profile_id is null and review_policy_id is null and design_version_id is null and decision_id is null and user_correction_id is not null)
    )
);

create trigger if not exists trg_kpt_item_conversion_project_insert
before insert on kpt_item_conversions
for each row
when (new.target_type = 'task' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
  or (new.target_type = 'command_profile' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from command_profiles where id = new.command_profile_id))
  or (new.target_type = 'review_policy' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from review_policies where id = new.review_policy_id))
  or (new.target_type = 'design_version' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from design_versions where id = new.design_version_id))
  or (new.target_type = 'decision' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from decisions where id = new.decision_id))
  or (new.target_type = 'user_correction' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from user_corrections where id = new.user_correction_id))
begin
    select raise(abort, 'kpt item conversion project_id must match target project_id');
end;

create trigger if not exists trg_kpt_item_conversion_project_update
before update of kpt_item_id, target_type, task_id, command_profile_id, review_policy_id, design_version_id, decision_id, user_correction_id on kpt_item_conversions
for each row
when (new.target_type = 'task' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
  or (new.target_type = 'command_profile' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from command_profiles where id = new.command_profile_id))
  or (new.target_type = 'review_policy' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from review_policies where id = new.review_policy_id))
  or (new.target_type = 'design_version' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from design_versions where id = new.design_version_id))
  or (new.target_type = 'decision' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from decisions where id = new.decision_id))
  or (new.target_type = 'user_correction' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from user_corrections where id = new.user_correction_id))
begin
    select raise(abort, 'kpt item conversion project_id must match target project_id');
end;

create trigger if not exists trg_repository_snapshot_referenced_delete
before delete on repository_snapshots
for each row
when exists (select 1 from resume_checks where repository_snapshot_id = old.id)
  or exists (select 1 from command_usages where repository_snapshot_id = old.id)
  or exists (select 1 from validation_runs where repository_snapshot_id = old.id)
  or exists (select 1 from artifacts where repository_snapshot_id = old.id)
  or exists (select 1 from review_plan_targets where repository_snapshot_id = old.id)
  or exists (select 1 from review_runs where repository_snapshot_id = old.id)
  or exists (select 1 from work_record_forks where source_repository_snapshot_id = old.id)
begin
    select raise(abort, 'cannot delete repository snapshot referenced by ledger rows');
end;

create trigger if not exists trg_repository_referenced_delete
before delete on repositories
for each row
when exists (select 1 from repository_snapshots where repository_id = old.id)
  or exists (select 1 from git_commits where repository_id = old.id)
  or exists (select 1 from git_file_changes where repository_id = old.id)
  or exists (select 1 from command_profiles where repository_id = old.id)
  or exists (select 1 from work_record_files where repository_id = old.id)
  or exists (select 1 from implementation_evidence where repository_id = old.id)
begin
    select raise(abort, 'cannot delete repository referenced by ledger rows');
end;

create trigger if not exists trg_git_commit_referenced_delete
before delete on git_commits
for each row
when exists (select 1 from work_record_commits where git_commit_id = old.id)
  or exists (select 1 from work_record_forks where source_git_commit_id = old.id)
  or exists (select 1 from implementation_evidence where git_commit_id = old.id)
begin
    select raise(abort, 'cannot delete git commit referenced by ledger rows');
end;

create trigger if not exists trg_git_file_change_referenced_delete
before delete on git_file_changes
for each row
when exists (select 1 from work_record_files where git_file_change_id = old.id)
  or exists (select 1 from implementation_evidence where git_file_change_id = old.id)
begin
    select raise(abort, 'cannot delete git file change referenced by ledger rows');
end;
"#;
