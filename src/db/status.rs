use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use super::{migration::*, owner_routing::*, project::*, project_integrity::*, runtime::*, *};

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
    init_project_with_name(root, None)
}

pub fn init_project_with_name(root: &Path, requested_name: Option<&str>) -> Result<InitOutcome> {
    if requested_name.is_some_and(|name| name.trim().is_empty()) {
        bail!("project name must not be empty");
    }
    let ledger_path = default_ledger_path(root);
    if let Some(parent) = ledger_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create ledger directory {}", parent.display()))?;
    }
    let _update_guard = crate::update::exclusive_writer_guard(root)?;
    for directory in [
        default_design_root(root),
        default_export_root(root),
        default_log_root(root),
    ] {
        fs::create_dir_all(&directory)
            .with_context(|| format!("failed to create directory {}", directory.display()))?;
    }

    let existing = ledger_path
        .metadata()
        .is_ok_and(|metadata| metadata.len() > 0);
    let conn = open_ledger(&ledger_path)?;
    if existing && project_requires_update(&conn)? {
        anyhow::bail!(
            "existing project state requires an explicit update; run agent-workbench update inspect"
        );
    }
    migrate(&conn)?;
    ensure_project(&conn, root)?;
    if let Some(requested_name) = requested_name {
        let project = project_id(&conn)?;
        let current: String =
            conn.query_row("select name from projects where id=?1", [project], |row| {
                row.get(0)
            })?;
        if existing && current != requested_name {
            bail!("project is already initialized with a different name");
        }
        conn.execute(
            "update projects set name=?1,updated_at=current_timestamp where id=?2",
            params![requested_name, project],
        )?;
    }
    sync_agents_md_authority(&conn, root)?;
    sync_commit_message_policy(&conn)?;

    Ok(InitOutcome { ledger_path })
}

pub fn project_status(root: &Path) -> Result<ProjectStatus> {
    project_status_for(root, None)
}

pub fn project_status_for(root: &Path, work_unit_id: Option<i64>) -> Result<ProjectStatus> {
    let ledger_path = default_ledger_path(root);
    if !ledger_path.exists() {
        return Ok(ProjectStatus {
            initialized: false,
            ledger_path,
            project_name: None,
            open_work_units: 0,
            active_activations: 0,
            schema_version: None,
            project_integrity: ProjectIntegrityStatus {
                result: "not_initialized".to_string(),
                predicates: Vec::new(),
            },
            phase_blocker: None,
            owner_actions: Vec::new(),
            finding_remediations: Vec::new(),
            source_corrections: Vec::new(),
        });
    }

    let integrity = evaluate_project_integrity(root);
    if let Some(error) = integrity.diagnostic_error.as_deref() {
        anyhow::bail!(
            "project integrity evaluation failed without a global classification: {error}"
        );
    }
    if integrity.status.result == "blocked" {
        return Ok(ProjectStatus {
            initialized: true,
            ledger_path,
            project_name: None,
            open_work_units: 0,
            active_activations: 0,
            schema_version: integrity.schema_version,
            project_integrity: integrity.status,
            phase_blocker: None,
            owner_actions: Vec::new(),
            finding_remediations: Vec::new(),
            source_corrections: Vec::new(),
        });
    }
    let conn = integrity
        .connection
        .context("integrity evaluator lost ledger connection")?;
    let project_name = conn
        .query_row("select name from projects order by id limit 1", [], |row| {
            row.get::<_, String>(0)
        })
        .optional()?;
    if let Some(work_unit_id) = work_unit_id {
        ensure_project_work_exists(&conn, work_unit_id)?;
    }
    let open_work_units = match work_unit_id {
        Some(work_unit_id) => conn.query_row(
            "select count(*) from work_units where id=?1 and status in ('open','blocked')",
            params![work_unit_id],
            |row| row.get(0),
        )?,
        None => count_rows(&conn, "work_units", "status in ('open', 'blocked')")?,
    };
    let active_activations = match work_unit_id {
        Some(work_unit_id) => conn.query_row(
            "select count(*) from work_unit_activations where work_unit_id=?1 and status='active'",
            params![work_unit_id],
            |row| row.get(0),
        )?,
        None => count_rows(&conn, "work_unit_activations", "status = 'active'")?,
    };
    let schema_version = conn
        .query_row(
            "select version from schema_migrations order by version desc limit 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let mut finding_remediations = current_finding_remediations(&conn)?;
    let mut source_corrections = current_source_corrections(&conn)?;
    let mut owner_actions = resolved_owner_actions(&conn)?;
    if let Some(work_unit_id) = work_unit_id {
        finding_remediations.retain(|item| item.work_unit_id == work_unit_id);
        source_corrections.retain(|item| item.work_unit_id == work_unit_id);
        owner_actions
            .retain(|owner| owner.owner_type == "work_unit" && owner.owner_id == work_unit_id);
    }

    Ok(ProjectStatus {
        initialized: true,
        ledger_path,
        project_name,
        open_work_units,
        active_activations,
        schema_version,
        project_integrity: integrity.status,
        phase_blocker: None,
        owner_actions,
        finding_remediations,
        source_corrections,
    })
}

pub fn next_action(root: &Path) -> Result<NextAction> {
    next_action_for(root, None)
}

pub fn next_action_for(root: &Path, work_unit_id: Option<i64>) -> Result<NextAction> {
    let ledger_path = default_ledger_path(root);
    if !ledger_path.exists() {
        return Ok(NextAction::NotInitialized { ledger_path });
    }

    let integrity = evaluate_project_integrity(root);
    if let Some(error) = integrity.diagnostic_error.as_deref() {
        anyhow::bail!(
            "project integrity evaluation failed without a global classification: {error}"
        );
    }
    if integrity.status.result == "blocked" {
        return Ok(NextAction::ProjectIntegrityBlocked {
            integrity: integrity.status,
        });
    }
    let conn = integrity
        .connection
        .context("integrity evaluator lost ledger connection")?;
    let owners = resolved_owner_actions(&conn)?;
    if let Some(work_unit_id) = work_unit_id {
        let status = ensure_project_work_exists(&conn, work_unit_id)?;
        if matches!(status.as_str(), "closed" | "abandoned") {
            return Ok(NextAction::SelectedWorkTerminal {
                work_unit_id,
                status,
            });
        }
        let owner = owners
            .into_iter()
            .find(|owner| owner.owner_type == "work_unit" && owner.owner_id == work_unit_id)
            .with_context(|| {
                format!("resolver did not return the selected work owner {work_unit_id}")
            })?;
        return Ok(NextAction::OwnerActions {
            owners: vec![owner],
        });
    }
    if owners.len() > 1 || owners.iter().any(|owner| owner.blocker_kind.is_some()) {
        return Ok(NextAction::OwnerActions { owners });
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

fn ensure_project_work_exists(conn: &Connection, work_unit_id: i64) -> Result<String> {
    let project = project_id(conn)?;
    conn.query_row(
        "select status from work_units where id=?1 and project_id=?2",
        params![work_unit_id, project],
        |row| row.get(0),
    )
    .optional()?
    .with_context(|| format!("work unit {work_unit_id} not found in this project"))
}

fn resolved_owner_actions(conn: &Connection) -> Result<Vec<OwnerAction>> {
    let owners = current_owner_actions(conn)?;
    let mut work_owners = HashSet::new();
    if owners
        .iter()
        .filter(|owner| owner.owner_type == "work_unit")
        .any(|owner| !work_owners.insert(owner.owner_id))
    {
        bail!("owner resolution returned more than one result for the same work");
    }
    Ok(owners)
}

pub(super) fn current_finding_remediations(conn: &Connection) -> Result<Vec<FindingRemediation>> {
    let project_id = project_id(conn)?;
    let mut stmt = conn.prepare(
        r#"
        select p.id, p.work_unit_id, f.id, c.id, f.description,
               coalesce(c.affected_surfaces, '-'), coalesce(c.fix_plan, '-'),
               c.design_invariant, c.tests_or_gates, c.verification_plan
        from review_plans p
        join review_runs r on r.review_plan_id = p.id
        join findings f on f.review_run_id = r.id
        join closures c on c.finding_id = f.id and c.status = 'registered'
        join work_unit_activations a on a.work_unit_id = p.work_unit_id and a.status = 'active'
        join work_units w on w.id = p.work_unit_id and w.status = 'open'
        join finding_remediation_bindings b
          on b.finding_id = f.id and b.closure_id = c.id
         and b.work_unit_id = p.work_unit_id and b.work_unit_activation_id = a.id
        where p.project_id = ?1 and p.required = 1 and p.stage = 'close-ready'
          and p.review_type in ('implementation_review', 'design_implementation_diff')
          and not exists(
            select 1 from correction_tokens token where token.closure_id=c.id
          )
          and f.status = 'open' and f.classification = 'valid'
          and not exists (
              select 1 from work_unit_dependencies d
              where d.work_unit_id = p.work_unit_id and d.status = 'open'
                and d.dependency_type in ('blocks', 'invalidates_assumption', 'invalidates_closure')
                and exists(select 1 from work_units dependency_target where dependency_target.id=d.depends_on_work_unit_id and dependency_target.status in ('open','blocked'))
          )
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
                design_invariant: row.get(7)?,
                tests_or_gates: row.get(8)?,
                verification_plan: row.get(9)?,
                next_action: format!("implement the scoped fix, then agent-workbench closure ready {closure_id} --evidence \"<evidence>\" --tests \"<tests>\""),
            })
        },
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub(super) fn current_source_corrections(conn: &Connection) -> Result<Vec<SourceCorrection>> {
    let project_id = project_id(conn)?;
    let mut stmt = conn.prepare(
        r#"
        select p.id, p.work_unit_id, f.id, c.id, s.id, f.description,
               c.affected_surfaces, c.fix_plan, c.design_invariant,
               c.tests_or_gates, c.verification_plan,
               (select min(token_ordinal) from correction_tokens t
                where t.closure_id = c.id and t.token_kind = 'transition' and t.status = 'pending'),
               (select operation from correction_tokens t
                where t.closure_id = c.id and t.token_kind = 'transition' and t.status = 'pending'
                order by token_ordinal limit 1)
        from correction_sessions s
        join closures c on c.id = s.closure_id and c.status = 'registered'
        join findings f on f.id = s.finding_id and f.status = 'open' and f.classification = 'valid'
        join review_runs r on r.id = f.review_run_id
        join review_plans p on p.id = r.review_plan_id
        where s.project_id = ?1 and s.status = 'active'
        order by f.id
        "#,
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        let closure_id = row.get::<_, i64>(3)?;
        let pending_token = row.get::<_, Option<i64>>(11)?;
        let pending_operation = row.get::<_, Option<String>>(12)?;
        let next_action = if let (Some(token), Some(operation)) = (pending_token, pending_operation)
        {
            let runtime = match operation.as_str() {
                "task-accept-out-of-scope" | "phase-dependency-accept" => {
                    " --authority <authority-event-id>"
                }
                "phase-dependency-satisfy" => " --evidence <evidence-ref>",
                _ => "",
            };
            format!(
                "agent-workbench closure transition apply {closure_id} --token {token}{runtime}"
            )
        } else {
            format!(
                "apply only the typed file correction contract, then agent-workbench closure ready {closure_id} --evidence \"<evidence>\" --tests \"<tests>\""
            )
        };
        Ok(SourceCorrection {
            review_plan_id: row.get(0)?,
            work_unit_id: row.get(1)?,
            finding_id: row.get(2)?,
            closure_id,
            correction_session_id: row.get(4)?,
            description: row.get(5)?,
            affected_surfaces: row.get(6)?,
            fix_plan: row.get(7)?,
            design_invariant: row.get(8)?,
            tests_or_gates: row.get(9)?,
            verification_plan: row.get(10)?,
            next_action,
        })
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub(super) fn attach_next_phase(
    conn: &Connection,
    mut work_unit: ActiveWorkUnit,
) -> Result<ActiveWorkUnit> {
    let next_phase = conn
        .query_row(
            r#"
            select p.id, p.phase_key, p.title
            from phase_epochs p
            where p.work_unit_id = ?1
              and p.state in ('open', 'blocked')
              and (
                exists(
                  select 1 from decomposition_applications application
                  join decomposition_plans plan on plan.id=application.decomposition_plan_id
                  where application.phase_id=p.id and plan.status='applied'
                )
                or not exists(
                  select 1 from decomposition_applications application
                  where application.phase_id=p.id
                  union all
                  select 1 from decomposition_migration_sources migration
                  where migration.source_phase_id=p.id
                )
              )
              and not exists (
                  select 1
                  from phase_epoch_dependencies d
                  where d.to_phase_epoch_id = p.id
                    and d.state = 'open'
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

pub(crate) fn current_phase_blocker(conn: &Connection) -> Result<Option<PhaseBlocker>> {
    let project_id = project_id(conn)?;
    let selected_stale =
        crate::traceability::selected_stale_record_with_owner_in(conn, project_id)?;
    if selected_stale.is_some() {
        let stale_transition = if let Some((kind, record_id, _)) = selected_stale.as_ref() {
            conn.query_row(
                r#"
                select token.closure_id, token.token_ordinal
                from correction_tokens token
                join closures c on c.id = token.closure_id and c.status = 'registered'
                join findings f on f.id = c.finding_id and f.status = 'open' and f.classification = 'valid'
                where token.status = 'pending' and token.token_kind = 'transition'
                  and token.operation in ('stale-accept', 'stale-close')
                  and token.target = ?1
                order by token.closure_id, token.token_ordinal limit 1
                "#,
                params![format!("{kind}/{record_id}")],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?
        } else {
            None
        };
        let next_action = stale_transition.map_or_else(
            || {
                let (kind, record_id, _) = selected_stale.as_ref().unwrap();
                format!("agent-workbench stale accept {kind} {record_id} --reason \"<reason>\"")
            },
            |(closure_id, token)| {
                format!("agent-workbench closure transition apply {closure_id} --token {token}")
            },
        );
        return Ok(Some(PhaseBlocker {
            kind: "stale_design".to_string(),
            review_plan_id: None,
            work_unit_id: selected_stale.map(|(_, _, work_unit_id)| work_unit_id),
            review_type: None,
            stage: None,
            review_run_id: None,
            finding_id: None,
            severity: Some("critical".to_string()),
            classification: None,
            description: "stale design-derived state blocks implementation and scoped remediation"
                .to_string(),
            next_action,
        }));
    }
    let active_remediation: Option<(i64, i64, i64, String, String, i64)> = conn
        .query_row(
            r#"
            select f.id, c.id, p.work_unit_id, w.status, p.status,
                   (select count(*) from work_unit_dependencies d
                    where d.work_unit_id = p.work_unit_id and d.status = 'open'
                      and d.dependency_type in ('blocks', 'invalidates_assumption', 'invalidates_closure')
                      and exists(select 1 from work_units dependency_target where dependency_target.id=d.depends_on_work_unit_id and dependency_target.status in ('open','blocked')))
            from finding_remediation_bindings b
            join findings f on f.id = b.finding_id and f.status = 'open' and f.classification = 'valid'
            join closures c on c.id = b.closure_id and c.status = 'registered'
            join work_unit_activations a on a.id = b.work_unit_activation_id and a.status = 'active'
            join review_runs r on r.id = f.review_run_id
            join review_plans p on p.id = r.review_plan_id
            join work_units w on w.id = p.work_unit_id
            where b.project_id = ?1
            order by b.id limit 1
            "#,
            params![project_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()?;
    let active_source_correction: bool = conn.query_row(
        r#"
        select exists(
            select 1 from correction_sessions s
            join findings f on f.id = s.finding_id and f.status = 'open' and f.classification = 'valid'
            join closures c on c.id = s.closure_id and c.status = 'registered'
            where s.project_id = ?1 and s.status = 'active'
        )
        "#,
        params![project_id],
        |row| row.get(0),
    )?;
    let mut review_blocker = conn
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
                      and a.result is null
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
                      and a.result is null
                      and rr.id > a.review_run_high_watermark
                    order by rr.id desc
                    limit 1
                ),
                (select c.status from closures c where c.finding_id = f.id and c.status != 'superseded' order by c.id desc limit 1),
                (select status from work_units where id = p.work_unit_id),
                p.status,
                p.required,
                exists(
                    select 1 from acceptance_records plan_acceptance
                    where plan_acceptance.target_type = 'review_plan'
                      and plan_acceptance.review_plan_id = p.id
                      and plan_acceptance.status = 'approved'
                ),
                not exists(
                    select 1
                    from closures correction_closure
                    join correction_tokens correction_token
                      on correction_token.closure_id=correction_closure.id
                    where correction_closure.finding_id=f.id
                      and correction_closure.status='registered'
                )
            from review_plans p
            join review_runs r on r.review_plan_id = p.id
            join findings f on f.review_run_id = r.id
            where p.project_id = ?1
              and r.project_id = ?1
              and f.project_id = ?1
              and f.status = 'open'
              and f.classification in ('unclassified', 'valid', 'design_conflict', 'needs_evidence')
              and not exists(select 1 from legacy_claim_audits l where l.project_id=f.project_id and l.review_run_id=f.review_run_id and l.reviewer_resolution in ('unbound','ambiguous'))
              and not (
                  f.classification = 'valid'
                  and p.stage = 'close-ready'
                  and p.review_type in ('implementation_review', 'design_implementation_diff')
                  and exists (
                      select 1 from closures c
                      join finding_remediation_bindings b
                        on b.finding_id = f.id and b.closure_id = c.id
                       and b.work_unit_id = p.work_unit_id
                      join work_unit_activations a
                        on a.id = b.work_unit_activation_id
                       and a.work_unit_id = p.work_unit_id
                      join work_units w on w.id = p.work_unit_id and w.status = 'open'
                      where c.finding_id = f.id and c.status = 'registered'
                        and a.status = 'active'
                        and p.status not in ('exhausted', 'needs_user_decision')
                        and not exists (
                          select 1 from acceptance_records plan_acceptance
                          where plan_acceptance.target_type = 'review_plan'
                            and plan_acceptance.review_plan_id = p.id
                            and plan_acceptance.status = 'approved'
                        )
                        and not exists (
                          select 1 from work_unit_dependencies d
                          where d.work_unit_id = p.work_unit_id and d.status = 'open'
                            and d.dependency_type in ('blocks', 'invalidates_assumption', 'invalidates_closure')
                            and exists(select 1 from work_units dependency_target where dependency_target.id=d.depends_on_work_unit_id and dependency_target.status in ('open','blocked'))
                        )
                  )
              )
              and not (
                  f.classification = 'valid'
                  and p.stage = 'close-ready'
                  and p.review_type in ('implementation_review', 'design_implementation_diff')
                  and exists (
                      select 1
                      from closures current_c
                      where current_c.finding_id = f.id and current_c.status = 'registered'
                  )
                  and exists (
                      select 1
                      from finding_remediation_bindings selected_b
                      join work_unit_activations selected_a
                        on selected_a.id = selected_b.work_unit_activation_id
                       and selected_a.status = 'active'
                      where selected_b.project_id = ?1
                        and selected_b.work_unit_id != p.work_unit_id
                  )
              )
              and not (
                  f.classification = 'valid'
                  and exists (
                      select 1 from closures current_c
                      where current_c.finding_id = f.id and current_c.status = 'registered'
                  )
                  and exists (
                      select 1 from correction_sessions selected_s
                      where selected_s.project_id = ?1 and selected_s.status = 'active'
                        and selected_s.finding_id != f.id
                  )
              )
              and not (
                  not (
                    p.required = 1 and p.stage = 'close-ready'
                    and p.review_type in ('implementation_review', 'design_implementation_diff')
                  )
                  and p.required = 1
                  and p.status not in ('exhausted', 'needs_user_decision')
                  and not exists (
                    select 1 from acceptance_records plan_acceptance
                    where plan_acceptance.target_type = 'review_plan'
                      and plan_acceptance.review_plan_id = p.id
                      and plan_acceptance.status = 'approved'
                  )
                  and exists (
                    select 1
                    from closures c
                    join correction_sessions s on s.closure_id = c.id and s.status = 'active'
                    where c.finding_id = f.id and c.status = 'registered'
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
                case
                    when f.classification != 'valid' then 1
                    when p.status in ('exhausted', 'needs_user_decision') then 2
                    when p.required = 0 or exists (
                        select 1 from acceptance_records plan_acceptance
                        where plan_acceptance.target_type = 'review_plan'
                          and plan_acceptance.review_plan_id = p.id
                          and plan_acceptance.status = 'approved'
                    ) then 3
                    when not exists (
                        select 1 from closures c where c.finding_id = f.id and c.status != 'superseded'
                    ) then 4
                    when exists (
                        select 1 from closures c where c.finding_id = f.id and c.status = 'incomplete'
                    ) then 5
                    when exists (
                        select 1 from closures c where c.finding_id = f.id and c.status = 'ready_for_verification'
                    ) then 6
                    when not (p.required = 1 and p.stage = 'close-ready'
                              and p.review_type in ('implementation_review', 'design_implementation_diff'))
                         then 7
                    else 8
                end,
                case p.stage
                    when 'design-ready' then 1
                    when 'implementation-ready' then 2
                    when 'close-ready' then 3
                    when 'resume-ready' then 4
                    else 5
                end,
                case when
                    f.classification = 'valid'
                    and p.stage = 'close-ready'
                    and p.review_type in ('implementation_review', 'design_implementation_diff')
                    and exists (
                        select 1 from finding_remediation_bindings prior
                        join work_unit_activations prior_a on prior_a.id = prior.work_unit_activation_id
                        where prior.work_unit_id = p.work_unit_id and prior_a.status = 'suspended'
                          and prior.id = (
                              select max(last.id) from finding_remediation_bindings last
                              where last.work_unit_id = p.work_unit_id
                          )
                    ) then 1 else 0 end,
                case when
                    f.classification = 'valid'
                    and p.stage = 'close-ready'
                    and p.review_type in ('implementation_review', 'design_implementation_diff')
                    and exists (
                        select 1 from finding_remediation_bindings prior
                        join work_unit_activations prior_a on prior_a.id = prior.work_unit_activation_id
                        where prior.work_unit_id = p.work_unit_id and prior_a.status = 'suspended'
                          and prior.id = (
                              select max(last.id) from finding_remediation_bindings last
                              where last.work_unit_id = p.work_unit_id
                          )
                    ) then coalesce((
                        select max(last.id) from finding_remediation_bindings last
                        where last.work_unit_id = p.work_unit_id
                    ), 0) else 0 end,
                f.id,
                p.work_unit_id,
                coalesce((select max(c.id) from closures c where c.finding_id = f.id), 0),
                p.id,
                r.id
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
                let work_status = row.get::<_, String>(14)?;
                let plan_status = row.get::<_, String>(15)?;
                let plan_required = row.get::<_, bool>(16)?;
                let plan_accepted = row.get::<_, bool>(17)?;
                let implementation_surface = row.get::<_, bool>(18)?;
                let review_type = row.get::<_, String>(2)?;
                let stage = row.get::<_, String>(3)?;
                let implementation_eligible = stage == "close-ready"
                    && matches!(
                        review_type.as_str(),
                        "implementation_review" | "design_implementation_diff"
                    )
                    && implementation_surface;
                Ok(PhaseBlocker {
                    kind: "required_review_finding".to_string(),
                    review_plan_id: Some(review_plan_id),
                    work_unit_id: Some(row.get(1)?),
                    review_type: Some(review_type),
                    stage: Some(stage),
                    review_run_id: Some(row.get(4)?),
                    finding_id: Some(finding_id),
                    severity: Some(row.get(6)?),
                    classification: Some(classification.clone()),
                    description: row.get(8)?,
                    next_action: finding_next_action(FindingActionState {
                        finding_id,
                        review_plan_id,
                        closure_id,
                        closure_status: closure_status.as_deref(),
                        attempt_id,
                        verification: verification_run_id
                            .map(|run_id| (run_id, verification_result.as_deref())),
                        classification: &classification,
                        implementation_eligible,
                        work_unit_id: row.get(1)?,
                        work_status: &work_status,
                        plan_status: &plan_status,
                        plan_required,
                        plan_accepted,
                    }),
                })
            },
        )
        .optional()?;
    if let Some(blocker) = review_blocker.as_mut()
        && blocker
            .next_action
            .starts_with("agent-workbench work remediate")
        && let (Some(work_unit_id), Some(finding_id)) = (blocker.work_unit_id, blocker.finding_id)
        && let Some(action) = remediation_dependency_action(conn, work_unit_id, finding_id)?
    {
        blocker.kind = "finding_remediation_recovery".to_string();
        blocker.description =
            "remediation owner is dormant behind an ordinary work dependency".to_string();
        blocker.next_action = action;
    }
    if let (
        Some(blocker),
        Some((finding_id, closure_id, work_unit_id, work_status, _, dependencies)),
    ) = (review_blocker.as_mut(), active_remediation.as_ref())
        && blocker.finding_id == Some(*finding_id)
        && (*dependencies > 0 || work_status != "open")
    {
        blocker.kind = "finding_remediation_recovery".to_string();
        blocker.description =
            format!("active remediation for closure {closure_id} is dormant behind an owner guard");
        blocker.next_action = if work_status == "blocked" {
            format!("agent-workbench work unblock {work_unit_id} --reason \"<reason>\"")
        } else if work_status != "open" {
            format!(
                "agent-workbench authority event add --type user_instruction --summary \"recover remediation owner {work_unit_id}\" --scope \"work-unit:{work_unit_id}\"; then agent-workbench work reopen {work_unit_id} --reason \"recover finding {finding_id}\" --reason-type closure_invalid --authority <authority-event-id>; then agent-workbench work remediate --finding {finding_id}"
            )
        } else {
            remediation_dependency_action(conn, *work_unit_id, *finding_id)?
                .unwrap_or_else(|| format!("agent-workbench work remediate --finding {finding_id}"))
        };
    }
    if review_blocker.is_some() {
        return Ok(review_blocker);
    }
    if let Some((finding_id, closure_id, work_unit_id, work_status, plan_status, dependencies)) =
        active_remediation
    {
        let next_action = if matches!(plan_status.as_str(), "exhausted" | "needs_user_decision") {
            format!(
                "record authority and waive the exhausted review plan before continuing finding {finding_id}"
            )
        } else if work_status == "blocked" {
            format!(
                "agent-workbench work unblock {work_unit_id} --reason \"<reason>\"; then continue finding {finding_id}"
            )
        } else if work_status != "open" {
            format!(
                "agent-workbench authority event add --type user_instruction --summary \"recover remediation owner {work_unit_id}\" --scope \"work-unit:{work_unit_id}\"; then agent-workbench work reopen {work_unit_id} --reason \"recover finding {finding_id}\" --reason-type closure_invalid --authority <authority-event-id>; then agent-workbench work remediate --finding {finding_id}"
            )
        } else if dependencies > 0 {
            remediation_dependency_action(conn, work_unit_id, finding_id)?
                .unwrap_or_else(|| format!("agent-workbench work remediate --finding {finding_id}"))
        } else {
            return Ok(None);
        };
        return Ok(Some(PhaseBlocker {
            kind: "finding_remediation_recovery".to_string(),
            review_plan_id: None,
            work_unit_id: Some(work_unit_id),
            review_type: Some("implementation_review".to_string()),
            stage: Some("close-ready".to_string()),
            review_run_id: None,
            finding_id: Some(finding_id),
            severity: Some("high".to_string()),
            classification: Some("valid".to_string()),
            description: format!(
                "active remediation for closure {closure_id} requires lifecycle recovery"
            ),
            next_action,
        }));
    }
    if active_source_correction {
        return Ok(None);
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
