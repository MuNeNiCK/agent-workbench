use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{current_phase_blocker, open_existing_project, project_id};

use super::{closure::*, correction_contract::*, correction_state::*, *};

pub fn apply_correction_transition(
    root: &Path,
    closure_id: i64,
    token_ordinal: i64,
    authority_event_id: Option<i64>,
    evidence: Option<&str>,
) -> Result<CorrectionTransitionOutcome> {
    let evidence = evidence.map(str::trim);
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let (token_id, operation, target, status, finding_id): (i64, String, String, String, i64) = tx
        .query_row(
            r#"
            select t.id, t.operation, t.target, t.status, c.finding_id
            from correction_tokens t
            join closures c on c.id = t.closure_id
            where t.closure_id = ?1 and t.token_ordinal = ?2
              and t.token_kind = 'transition' and t.project_id = ?3
              and c.status = 'registered'
            "#,
            params![closure_id, token_ordinal, project_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?
        .context("registered correction transition token not found")?;
    let declared_stale = matches!(operation.as_str(), "stale-accept" | "stale-close");
    let selected_stale = crate::traceability::selected_stale_record_in(&tx, project_id)?;
    let matches_selected_stale = selected_stale
        .as_ref()
        .is_some_and(|(kind, id)| target == format!("{kind}/{id}"));
    if let Some((kind, id)) = selected_stale.as_ref()
        && (!declared_stale || !matches_selected_stale)
    {
        bail!("stale {kind}:{id} is the selected transition");
    }
    if let Some(blocker) = current_phase_blocker(&tx)? {
        let exact_stale_apply = format!(
            "agent-workbench closure transition apply {closure_id} --token {token_ordinal}"
        );
        if !(blocker.kind == "stale_design"
            && declared_stale
            && matches_selected_stale
            && blocker.next_action == exact_stale_apply)
        {
            bail!(
                "closure transition apply is not the selected action; next: {}",
                blocker.next_action
            );
        }
    }
    let mut session_id = tx
        .query_row(
            "select id from correction_sessions where closure_id = ?1 and status = 'active'",
            params![closure_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    if session_id.is_none() {
        if !declared_stale || !matches_selected_stale {
            bail!(
                "correction transition requires closure correction-begin unless it is the selected stale bootstrap token"
            );
        }
        let another_active: bool = tx.query_row(
            "select exists(select 1 from correction_sessions where project_id = ?1 and status = 'active')",
            params![project_id],
            |row| row.get(0),
        )?;
        if another_active {
            bail!("another source correction session is selected");
        }
        if finding_is_remediation_eligible(&tx, project_id, finding_id)? {
            bail!("implementation findings cannot bootstrap source correction");
        }
        let design_root = correction_design_root(&tx, finding_id)?;
        ensure_correction_prestate_unchanged(&tx, root, closure_id, design_root.as_deref())?;
        validate_correction_transition_preflight(&tx, project_id, closure_id, finding_id)?;
        tx.execute(
            "insert into correction_sessions(project_id, finding_id, closure_id, status, created_at) values (?1, ?2, ?3, 'active', current_timestamp)",
            params![project_id, finding_id, closure_id],
        )?;
        session_id = Some(tx.last_insert_rowid());
    }
    let session_id = session_id.unwrap();
    if status == "applied" {
        let (application_id, stored_authority, stored_evidence, result_ref): (
            i64,
            Option<i64>,
            Option<String>,
            String,
        ) = tx.query_row(
            "select id, authority_event_id, evidence_ref, result_ref from correction_transition_applications where correction_token_id = ?1",
            params![token_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        if stored_authority != authority_event_id
            || stored_evidence.as_deref() != evidence.map(str::trim)
        {
            bail!("transition token was already applied with different authority or evidence");
        }
        validate_completion_inheritance_application(&tx, application_id)?;
        return Ok(CorrectionTransitionOutcome {
            closure_id,
            token_ordinal,
            application_id,
            result_ref,
            idempotent: true,
        });
    }
    let selected_transition: i64 = if declared_stale && matches_selected_stale {
        token_ordinal
    } else {
        tx.query_row(
            r#"
        select min(token_ordinal) from correction_tokens
        where closure_id = ?1 and token_kind = 'transition' and status = 'pending'
        "#,
            params![closure_id],
            |row| row.get(0),
        )?
    };
    if token_ordinal != selected_transition {
        bail!(
            "correction transitions must be applied in declared order; selected token is {selected_transition}"
        );
    }
    match operation.as_str() {
        "task-accept-out-of-scope" | "phase-dependency-accept" if authority_event_id.is_none() => {
            bail!("{operation} requires --authority")
        }
        "phase-dependency-satisfy" if evidence.is_none_or(|value| value.trim().is_empty()) => {
            bail!("phase-dependency-satisfy requires --evidence")
        }
        "task-accept-out-of-scope" | "phase-dependency-accept" if evidence.is_some() => {
            bail!("{operation} forbids --evidence")
        }
        "phase-dependency-satisfy" if authority_event_id.is_some() => {
            bail!("phase-dependency-satisfy forbids --authority")
        }
        "task-accept-out-of-scope" | "phase-dependency-accept" | "phase-dependency-satisfy" => {}
        _ if authority_event_id.is_some() || evidence.is_some() => {
            bail!("{operation} does not accept runtime authority or evidence")
        }
        _ => {}
    }

    let (work_unit_id, design_version_id): (i64, Option<i64>) = tx.query_row(
        r#"
        select p.work_unit_id, p.design_version_id
        from closures c
        join findings f on f.id = c.finding_id
        join review_runs r on r.id = f.review_run_id
        join review_plans p on p.id = r.review_plan_id
        where c.id = ?1
        "#,
        params![closure_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let reason = format!("correction closure {closure_id} token {token_ordinal}");
    let before_state = transition_state_snapshot(&tx, work_unit_id)?;
    let mut completion_inheritances = Vec::new();
    let result_ref = match operation.as_str() {
        "design-decompose" => {
            let (design, work) = parse_pair(&target)?;
            if work != work_unit_id {
                bail!("design-decompose target work unit does not own the correction")
            }
            if design_version_id != Some(design) {
                bail!("design-decompose target design is outside the correction review plan")
            }
            let outcome = crate::traceability::decompose_design_in(
                &tx,
                project_id,
                crate::traceability::DesignDecomposition {
                    design_version_id: design,
                    work_unit_id: work,
                    checklist_title: Some("mediated correction decomposition"),
                    reason: Some(&reason),
                },
            )?;
            ensure_mediated_decomposition_coverage(&tx, project_id, work, design)?;
            format!("checklist:{}", outcome.checklist_id)
        }
        "design-reconcile" => {
            let parts = target.split('/').collect::<Vec<_>>();
            let design = parts[0].parse::<i64>()?;
            let work = parts[1].parse::<i64>()?;
            let checklist = parts[2].parse::<i64>()?;
            if work != work_unit_id || design_version_id != Some(design) {
                bail!("design-reconcile target is outside the correction owner or design")
            }
            let outcome = crate::traceability::reconcile_design_in(
                &tx, project_id, design, work, checklist, &reason,
            )?;
            completion_inheritances = outcome.completion_inheritances;
            ensure_mediated_decomposition_coverage(&tx, project_id, work, design)?;
            format!("checklist:{}", outcome.checklist_id)
        }
        "task-accept-out-of-scope" => {
            let (task_id, design_requirement_id) = resolve_task_ref(
                &tx,
                session_id,
                token_ordinal,
                work_unit_id,
                design_version_id,
                &target,
            )?;
            let outcome = if target.starts_with("@task/") {
                crate::planning::accept_task_out_of_scope_in(
                    &tx,
                    project_id,
                    task_id,
                    design_requirement_id,
                    &reason,
                    authority_event_id.unwrap(),
                )?
            } else {
                crate::planning::accept_recovery_task_out_of_scope_in(
                    &tx,
                    project_id,
                    task_id,
                    design_requirement_id,
                    design_version_id.context("correction review has no design")?,
                    &reason,
                    authority_event_id.unwrap(),
                )?
            };
            format!(
                "task:{}:acceptance:{}",
                task_id, outcome.acceptance_record_id
            )
        }
        "phase-create" => {
            let parts = target.split('/').collect::<Vec<_>>();
            if parts.len() != 6 {
                bail!("phase-create target requires work/design/alias/kind/order/key")
            }
            let work = parts[0].parse::<i64>()?;
            let design = parts[1].parse::<i64>()?;
            if work != work_unit_id {
                bail!("phase-create target work unit does not own the correction")
            }
            if design_version_id != Some(design) {
                bail!("phase-create target design is outside the correction review plan")
            }
            let outcome = crate::phases::create_phase_in(
                &tx,
                project_id,
                crate::phases::NewWorkPhase {
                    work_unit_id: work,
                    design_version_id: Some(design),
                    key: parts[5],
                    title: parts[5],
                    kind: parts[3],
                    order: parts[4].parse()?,
                    reason: Some(&reason),
                },
            )?;
            format!("phase:{}", outcome.phase_id)
        }
        "phase-assign" => {
            let (phase_ref, task_ref) = target
                .split_once('/')
                .context("phase-assign target requires phase/task")?;
            let phase_id =
                resolve_phase_ref(&tx, session_id, token_ordinal, work_unit_id, phase_ref)?;
            let (task_id, _) = resolve_task_ref(
                &tx,
                session_id,
                token_ordinal,
                work_unit_id,
                design_version_id,
                task_ref,
            )?;
            if let Some(requirement_key) = task_ref.strip_prefix("@task/") {
                tx.execute(
                    r#"
                    delete from work_phase_task_memberships
                    where phase_id=?5 and task_id!=?1 and task_id in (
                      select distinct predecessor.id
                      from tasks predecessor
                      join task_derivations td on td.task_id=predecessor.id
                      join design_requirements r on r.id=td.design_requirement_id
                      join design_versions v on v.id=r.design_version_id
                      join design_versions current_v on current_v.id=?2
                      where predecessor.work_unit_id=?3
                        and predecessor.status in ('open','blocked')
                        and r.requirement_key=?4
                        and v.design_package_id=current_v.design_package_id
                        and (
                          td.status='active'
                          or (td.status='stale' and exists(
                            select 1 from acceptance_records ar
                            join correction_transition_applications stale_app
                              on stale_app.correction_session_id=?6
                            join correction_tokens stale_token
                              on stale_token.id=stale_app.correction_token_id
                            where ar.target_type='stale_record'
                              and ar.stale_record_type='task_derivation'
                              and ar.stale_record_id=td.id and ar.status='approved'
                              and stale_token.operation='stale-accept'
                              and stale_token.token_ordinal<?7
                              and stale_app.result_ref='stale:task_derivation:'||td.id||':stale_accepted'
                          ))
                          or (td.status='closed' and exists(
                            select 1 from correction_transition_aliases a
                            join correction_transition_applications app on app.id=a.correction_application_id
                            join correction_tokens token on token.id=app.correction_token_id
                            where a.correction_session_id=?6 and a.record_type='task'
                              and a.record_id=predecessor.id
                              and a.alias='@superseded-task/'||predecessor.id
                              and token.operation='design-reconcile'
                              and token.token_ordinal<?7
                          ))
                        )
                    )
                    "#,
                    params![task_id, design_version_id, work_unit_id, requirement_key, phase_id, session_id, token_ordinal],
                )?;
            }
            crate::phases::assign_task_to_phase_in(&tx, project_id, phase_id, task_id)?;
            format!("phase:{phase_id}:task:{task_id}")
        }
        "phase-dependency-add" => {
            let parts = target.split('/').collect::<Vec<_>>();
            if parts.len() != 3 {
                bail!("phase-dependency-add target requires from/to/type")
            }
            let from = resolve_phase_ref(&tx, session_id, token_ordinal, work_unit_id, parts[0])?;
            let to = resolve_phase_ref(&tx, session_id, token_ordinal, work_unit_id, parts[1])?;
            let outcome = crate::phases::add_phase_dependency_in(
                &tx,
                project_id,
                crate::phases::NewPhaseDependency {
                    from_phase_id: from,
                    to_phase_id: to,
                    dependency_type: parts[2],
                    reason: &reason,
                },
            )?;
            format!("phase-dependency:{}", outcome.dependency_id)
        }
        "phase-dependency-satisfy" => {
            let dependency_id = target.parse::<i64>()?;
            ensure_phase_dependency_owner(&tx, dependency_id, work_unit_id)?;
            crate::phases::update_dependency_status_in(
                &tx,
                project_id,
                dependency_id,
                "satisfied",
                &reason,
                Some(evidence.unwrap()),
                None,
            )?;
            format!("phase-dependency:{dependency_id}:satisfied")
        }
        "phase-dependency-accept" => {
            let dependency_id = target.parse::<i64>()?;
            ensure_phase_dependency_owner(&tx, dependency_id, work_unit_id)?;
            ensure_phase_dependency_authority_scope(
                &tx,
                project_id,
                authority_event_id.unwrap(),
                dependency_id,
                work_unit_id,
            )?;
            crate::phases::update_dependency_status_in(
                &tx,
                project_id,
                dependency_id,
                "accepted",
                &reason,
                None,
                Some(authority_event_id.unwrap()),
            )?;
            format!("phase-dependency:{dependency_id}:accepted")
        }
        "stale-accept" | "stale-close" => {
            let (record_type, record_id) = target
                .split_once('/')
                .context("stale target requires type/id")?;
            let input = crate::traceability::StaleRecordDisposition {
                record_type,
                record_id: record_id.parse()?,
                reason: &reason,
            };
            let outcome = crate::traceability::update_stale_record_disposition_in(
                &tx,
                project_id,
                input,
                operation == "stale-close",
            )?;
            format!(
                "stale:{}:{}:{}",
                outcome.record_type, outcome.record_id, outcome.status
            )
        }
        _ => bail!("unsupported correction transition {operation}"),
    };
    let after_state = transition_state_snapshot(&tx, work_unit_id)?;

    tx.execute(
        r#"
        insert into correction_transition_applications(
            project_id, correction_session_id, correction_token_id,
            authority_event_id, evidence_ref, before_state, after_state,
            result_ref, created_at
        ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, current_timestamp)
        "#,
        params![
            project_id,
            session_id,
            token_id,
            authority_event_id,
            evidence,
            before_state,
            after_state,
            result_ref
        ],
    )?;
    let application_id = tx.last_insert_rowid();
    record_completion_inheritances(
        &tx,
        project_id,
        session_id,
        application_id,
        &completion_inheritances,
    )?;
    validate_completion_inheritance_application(&tx, application_id)?;
    record_correction_transition_aliases(
        &tx,
        project_id,
        session_id,
        application_id,
        &operation,
        &target,
        &result_ref,
    )?;
    tx.execute(
        "update correction_tokens set status = 'applied', applied_at = current_timestamp where id = ?1 and status = 'pending'",
        params![token_id],
    )?;
    tx.commit()?;
    Ok(CorrectionTransitionOutcome {
        closure_id,
        token_ordinal,
        application_id,
        result_ref,
        idempotent: false,
    })
}
