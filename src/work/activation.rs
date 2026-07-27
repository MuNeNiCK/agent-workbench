use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use crate::db::{
    NewEvent, active_activation, current_phase_blocker, insert_event, open_existing_project,
    project_id, suspended_activation,
};
use crate::rules::{RuleBindingInput, insert_rule_binding};
use crate::traceability::{ImplementationReadyCheck, implementation_ready};

use super::{close_trace::*, forking::*, *};

pub fn start_work(root: &Path, title: &str, responsibility: Option<&str>) -> Result<WorkOutcome> {
    start_work_with_options(
        root,
        WorkStart {
            title,
            responsibility,
            design_version_id: None,
            implementation: false,
        },
    )
}

pub fn remediate_work(root: &Path, finding_id: i64) -> Result<WorkRemediateOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let active_source_correction: bool = tx.query_row(
        "select exists(select 1 from correction_sessions where project_id = ?1 and status = 'active')",
        params![project_id],
        |row| row.get(0),
    )?;
    if active_source_correction {
        bail!("finish the selected source correction before implementation remediation");
    }
    if let Some(blocker) = current_phase_blocker(&tx)? {
        let expected = format!("agent-workbench work remediate --finding {finding_id}");
        if blocker.next_action != expected {
            bail!(
                "work remediate is not the selected action; next: {}",
                blocker.next_action
            );
        }
    }
    let (work_unit_id, closure_id, work_status): (i64, i64, String) = tx
        .query_row(
            r#"
            select p.work_unit_id, c.id, w.status
            from findings f
            join review_runs r on r.id = f.review_run_id
            join review_plans p on p.id = r.review_plan_id
            join work_units w on w.id = p.work_unit_id
            join closures c on c.finding_id = f.id and c.status = 'registered'
            where f.id = ?1 and f.project_id = ?2
              and f.status = 'open' and f.classification = 'valid'
              and p.required = 1
              and (
                (
                  p.stage = 'close-ready'
                  and p.review_type in ('implementation_review', 'design_implementation_diff')
                )
                or (
                  p.stage = 'implementation-ready'
                  and p.review_type = 'implementation_review'
                  and f.finding_type = 'implementation_finding'
                )
              )
              and p.status not in ('exhausted', 'needs_user_decision')
              and not exists(
                select 1 from correction_tokens token where token.closure_id=c.id
              )
              and not exists (
                select 1 from acceptance_records ar
                where ar.finding_id = f.id and ar.target_type = 'finding'
                  and ar.status = 'approved'
                  and ar.acceptance_type in (
                    'accepted_out_of_scope', 'explicit_exception', 'classified_failure'
                  )
              )
            order by c.id desc limit 1
            "#,
            params![finding_id, project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .context("eligible registered remediation finding not found")?;

    if has_stale_design_state_for_work(&tx, work_unit_id)? {
        bail!("stale design blocks remediation; run agent-workbench stale list");
    }
    match work_status.as_str() {
        "open" => {}
        "blocked" => bail!(
            "selected remediation owner is blocked; run agent-workbench work unblock {work_unit_id} --reason \"<reason>\""
        ),
        "closed" | "abandoned" => bail!(
            "selected remediation owner is terminal; record authority, run agent-workbench work reopen {work_unit_id}, then rerun work remediate"
        ),
        _ => bail!("selected remediation owner has unsupported status {work_status}"),
    }

    let active_bound_owner = tx
        .query_row(
            r#"
            select b.work_unit_id, b.work_unit_activation_id
            from finding_remediation_bindings b
            join closures c on c.id = b.closure_id and c.status = 'registered'
            join findings f on f.id = b.finding_id and f.status = 'open' and f.classification = 'valid'
            join work_unit_activations a on a.id = b.work_unit_activation_id and a.status = 'active'
            join review_runs r on r.id = f.review_run_id
            join review_plans p on p.id = r.review_plan_id
            where b.project_id = ?1
              and p.required = 1
              and (
                (
                  p.stage = 'close-ready'
                  and p.review_type in ('implementation_review', 'design_implementation_diff')
                )
                or (
                  p.stage = 'implementation-ready'
                  and p.review_type = 'implementation_review'
                  and f.finding_type = 'implementation_finding'
                )
              )
              and p.status not in ('exhausted', 'needs_user_decision')
              and not exists(
                select 1 from correction_tokens token where token.closure_id=c.id
              )
              and not exists(
                select 1 from acceptance_records accepted
                where accepted.finding_id=f.id and accepted.target_type='finding'
                  and accepted.status='approved'
                  and accepted.acceptance_type in (
                    'accepted_out_of_scope','explicit_exception','classified_failure'
                  )
              )
            order by f.id,c.id,b.id limit 1
            "#,
            params![project_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;
    if let Some((bound_work_unit_id, bound_activation_id)) = active_bound_owner {
        if bound_work_unit_id != work_unit_id {
            bail!("another remediation owner is active; continue its scoped remediation first");
        }
        let already_bound: i64 = tx.query_row(
            "select count(*) from finding_remediation_bindings where finding_id = ?1 and closure_id = ?2 and work_unit_activation_id = ?3",
            params![finding_id, closure_id, bound_activation_id],
            |row| row.get(0),
        )?;
        if already_bound > 0 {
            let unbound_same_owner: i64 = tx.query_row(
                r#"
                select count(*)
                from findings f
                join review_runs r on r.id = f.review_run_id
                join review_plans p on p.id = r.review_plan_id
                join closures c on c.finding_id = f.id and c.status = 'registered'
                where p.work_unit_id = ?1 and f.project_id = ?2
                  and f.status = 'open' and f.classification = 'valid'
                  and p.required = 1
                  and (
                    (
                      p.stage = 'close-ready'
                      and p.review_type in ('implementation_review', 'design_implementation_diff')
                    )
                    or (
                      p.stage = 'implementation-ready'
                      and p.review_type = 'implementation_review'
                      and f.finding_type = 'implementation_finding'
                    )
                  )
                  and p.status not in ('exhausted', 'needs_user_decision')
                  and not exists(
                    select 1 from correction_tokens token where token.closure_id=c.id
                  )
                  and not exists(
                    select 1 from acceptance_records accepted
                    where accepted.finding_id=f.id and accepted.target_type='finding'
                      and accepted.status='approved'
                      and accepted.acceptance_type in (
                        'accepted_out_of_scope','explicit_exception','classified_failure'
                      )
                  )
                  and not exists (
                    select 1 from finding_remediation_bindings b
                    where b.finding_id = f.id and b.closure_id = c.id
                      and b.work_unit_activation_id = ?3
                  )
                "#,
                params![work_unit_id, project_id, bound_activation_id],
                |row| row.get(0),
            )?;
            if unbound_same_owner == 0 {
                return Ok(WorkRemediateOutcome {
                    work_unit_id,
                    activation_id: bound_activation_id,
                    binding_count: 0,
                    idempotent: true,
                });
            }
        }
    }

    if active_bound_owner.is_none() {
        let selected: i64 = tx.query_row(
            r#"
        select f.id
        from findings f
        join review_runs r on r.id = f.review_run_id
        join review_plans p on p.id = r.review_plan_id
        join closures c on c.finding_id = f.id and c.status = 'registered'
        where f.project_id = ?1 and f.status = 'open' and f.classification = 'valid'
          and p.required = 1
          and (
            (
              p.stage = 'close-ready'
              and p.review_type in ('implementation_review', 'design_implementation_diff')
            )
            or (
              p.stage = 'implementation-ready'
              and p.review_type = 'implementation_review'
              and f.finding_type = 'implementation_finding'
            )
          )
          and p.status not in ('exhausted', 'needs_user_decision')
          and not exists(
            select 1 from correction_tokens token where token.closure_id=c.id
          )
          and not exists(
            select 1 from acceptance_records ar
            where ar.finding_id=f.id and ar.target_type='finding'
              and ar.status='approved'
              and ar.acceptance_type in (
                'accepted_out_of_scope','explicit_exception','classified_failure'
              )
          )
        order by
          case when exists (
            select 1 from finding_remediation_bindings prior
            join work_unit_activations pa on pa.id = prior.work_unit_activation_id
            where prior.work_unit_id = p.work_unit_id and pa.status = 'suspended'
              and prior.id = (select max(last.id) from finding_remediation_bindings last where last.work_unit_id = p.work_unit_id)
          ) then 1 else 0 end,
          case when exists (
            select 1 from finding_remediation_bindings prior
            join work_unit_activations pa on pa.id = prior.work_unit_activation_id
            where prior.work_unit_id = p.work_unit_id and pa.status = 'suspended'
              and prior.id = (select max(last.id) from finding_remediation_bindings last where last.work_unit_id = p.work_unit_id)
          ) then coalesce((select max(last.id) from finding_remediation_bindings last where last.work_unit_id = p.work_unit_id), 0) else 0 end,
          f.id, p.work_unit_id
        limit 1
            "#,
            params![project_id],
            |row| row.get(0),
        )?;
        if selected != finding_id {
            bail!(
                "another remediation finding has precedence; run agent-workbench work remediate --finding {selected}"
            );
        }
    }

    let active = active_activation(&tx)?;
    let activation_id = match active {
        Some(active) if active.work_unit_id == work_unit_id => active.activation_id,
        _ => {
            let parent = prepare_parent_frame(
                &tx,
                &format!("remediate finding {finding_id}"),
                &format!("resume after remediation work unit {work_unit_id}"),
            )?;
            tx.execute(
                r#"
                insert into work_unit_activations(
                    project_id, work_unit_id, parent_activation_id, stack_depth,
                    status, activation_reason, opened_at
                ) values (?1, ?2, ?3, ?4, 'active', 'follow_up', current_timestamp)
                "#,
                params![
                    project_id,
                    work_unit_id,
                    parent.as_ref().map(|a| a.activation_id),
                    parent.as_ref().map(|a| a.stack_depth + 1).unwrap_or(0)
                ],
            )?;
            let activation_id = tx.last_insert_rowid();
            if let Some(parent) = parent {
                tx.execute(
                    "update work_unit_activations set suspended_by_activation_id = ?1 where id = ?2",
                    params![activation_id, parent.activation_id],
                )?;
            }
            activation_id
        }
    };

    let recovery_epoch: Option<(i64, i64)> = tx
        .query_row(
            r#"
            select dependency_id, reopened_event_id
            from finding_remediation_recovery_epochs
            where work_unit_id = ?1 and work_unit_activation_id = ?2
              and dependency_id in (
                  select id from work_unit_dependencies where status = 'open'
              )
            order by id desc limit 1
            "#,
            params![work_unit_id, activation_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let blocking_dependencies: i64 = tx.query_row(
        r#"
        select count(*) from work_unit_dependencies d
        where d.work_unit_id = ?1 and d.status = 'open'
          and d.dependency_type in ('blocks', 'invalidates_assumption', 'invalidates_closure')
          and exists(select 1 from work_units dependency_target where dependency_target.id=d.depends_on_work_unit_id and dependency_target.status in ('open','blocked'))
          and d.id != coalesce(?2, -1)
        "#,
        params![work_unit_id, recovery_epoch.map(|epoch| epoch.0)],
        |row| row.get(0),
    )?;
    if blocking_dependencies > 0 {
        bail!("selected remediation owner has an open blocking dependency");
    }

    let mut stmt = tx.prepare(
        r#"
        select f.id, c.id
        from findings f
        join review_runs r on r.id = f.review_run_id
        join review_plans p on p.id = r.review_plan_id
        join closures c on c.finding_id = f.id and c.status = 'registered'
        where p.work_unit_id = ?1 and f.project_id = ?2
          and f.status = 'open' and f.classification = 'valid'
          and p.required = 1
          and (
            (
              p.stage = 'close-ready'
              and p.review_type in ('implementation_review', 'design_implementation_diff')
            )
            or (
              p.stage = 'implementation-ready'
              and p.review_type = 'implementation_review'
              and f.finding_type = 'implementation_finding'
            )
          )
          and p.status not in ('exhausted', 'needs_user_decision')
          and not exists(
            select 1 from correction_tokens token where token.closure_id=c.id
          )
          and not exists(
            select 1 from acceptance_records accepted
            where accepted.finding_id=f.id and accepted.target_type='finding'
              and accepted.status='approved'
              and accepted.acceptance_type in (
                'accepted_out_of_scope','explicit_exception','classified_failure'
              )
          )
        order by f.id
        "#,
    )?;
    let rows = stmt.query_map(params![work_unit_id, project_id], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
    })?;
    let bindings = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    let mut binding_count = 0;
    for (candidate_finding_id, candidate_closure_id) in bindings {
        binding_count += tx.execute(
            r#"
            insert or ignore into finding_remediation_bindings(
                project_id, finding_id, closure_id, work_unit_id,
                work_unit_activation_id, created_at
            ) values (?1, ?2, ?3, ?4, ?5, current_timestamp)
            "#,
            params![
                project_id,
                candidate_finding_id,
                candidate_closure_id,
                work_unit_id,
                activation_id
            ],
        )?;
    }
    if let Some((dependency_id, reopened_event_id)) = recovery_epoch {
        tx.execute(
            r#"
            update work_unit_dependencies
            set status = 'resolved', resolved_at = current_timestamp,
                resolved_by_work_unit_event_id = ?1
            where id = ?2 and work_unit_id = ?3 and depends_on_work_unit_id = ?3
              and dependency_type = 'invalidates_closure' and status = 'open'
            "#,
            params![reopened_event_id, dependency_id, work_unit_id],
        )?;
    }
    tx.commit()?;
    Ok(WorkRemediateOutcome {
        work_unit_id,
        activation_id,
        binding_count: binding_count as i64,
        idempotent: false,
    })
}

pub fn start_work_with_options(root: &Path, input: WorkStart<'_>) -> Result<WorkOutcome> {
    ensure_implementation_intent_start(&input)?;
    if let Some(design_version_id) = input.design_version_id {
        let ready = implementation_ready(
            root,
            ImplementationReadyCheck {
                design_version_id: Some(design_version_id),
            },
        )?;
        if ready.result != "pass" {
            bail!("implementation work start requires implementation-ready to pass");
        }
    }
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    ensure_work_mutation_allowed(&tx, "work start", None)?;

    if active_activation(&tx)?.is_some() {
        bail!("cannot start work while another activation is active");
    }

    tx.execute(
        r#"
        insert into work_units(project_id, title, status, responsibility, started_at)
        values (?1, ?2, 'open', ?3, current_timestamp)
        "#,
        params![project_id, input.title, input.responsibility],
    )?;
    let work_unit_id = tx.last_insert_rowid();
    let work_scope = work_unit_id.to_string();
    if input.responsibility.is_some() {
        insert_rule_binding(
            &tx,
            RuleBindingInput {
                project_id,
                rule_source_type: "work_unit",
                authority_event_id: None,
                user_correction_id: None,
                command_profile_id: None,
                review_policy_id: None,
                review_plan_id: None,
                work_unit_id: Some(work_unit_id),
                validation_gate_id: None,
                acceptance_record_id: None,
                scope_type: "work_unit",
                scope_key: Some(&work_scope),
                precedence: 60,
            },
        )?;
    }

    tx.execute(
        r#"
        insert into work_unit_activations(
            project_id, work_unit_id, stack_depth, status, activation_reason, opened_at
        )
        values (?1, ?2, 0, 'active', 'start', current_timestamp)
        "#,
        params![project_id, work_unit_id],
    )?;
    let activation_id = tx.last_insert_rowid();

    insert_event(
        &tx,
        NewEvent {
            work_unit_id,
            activation_id: Some(activation_id),
            related_activation_id: None,
            event_type: "opened",
            reason: input.responsibility,
            status_domain: "work_unit",
            previous_status: None,
            next_status: Some("open"),
        },
    )?;

    tx.commit()?;

    Ok(WorkOutcome {
        work_unit_id,
        activation_id,
    })
}

pub fn activate_work(root: &Path, input: WorkActivate<'_>) -> Result<WorkOutcome> {
    ensure_implementation_intent_activate(&input)?;
    if let Some(design_version_id) = input.design_version_id {
        let ready = implementation_ready(
            root,
            ImplementationReadyCheck {
                design_version_id: Some(design_version_id),
            },
        )?;
        if ready.result != "pass" {
            bail!("implementation work activation requires implementation-ready to pass");
        }
    }
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let dependency_selected = current_phase_blocker(&tx)?.is_some_and(|blocker| {
        blocker.kind == "finding_remediation_recovery"
            && action_selects_work(&blocker.next_action, "work activate", input.work_unit_id)
    });
    ensure_work_mutation_allowed(
        &tx,
        "work activate",
        Some((input.work_unit_id, "work activate")),
    )?;

    if let Some(design_version_id) = input.design_version_id {
        ensure_work_unit_has_design_scope(&tx, project_id, input.work_unit_id, design_version_id)?;
    }

    if !dependency_selected && let Some(active) = active_activation(&tx)? {
        if active.work_unit_id == input.work_unit_id
            && input.implementation
            && input.design_version_id.is_some()
        {
            tx.commit()?;
            return Ok(WorkOutcome {
                work_unit_id: input.work_unit_id,
                activation_id: active.activation_id,
            });
        }
        bail!("cannot activate work while another activation is active");
    }
    if !dependency_selected && suspended_activation(&tx)?.is_some() {
        bail!(
            "cannot activate open work while a suspended activation exists; run resume-check and work resume"
        );
    }

    tx.query_row(
        "select 1 from work_units where id = ?1 and project_id = ?2 and status = 'open'",
        params![input.work_unit_id, project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("open work unit not found")?;

    if dependency_selected {
        let reason = input
            .reason
            .unwrap_or("schedule selected remediation dependency");
        let parent = prepare_parent_frame(
            &tx,
            reason,
            &format!("resume after dependency work unit {}", input.work_unit_id),
        )?;
        tx.execute(
            r#"
            insert into work_unit_activations(
                project_id, work_unit_id, parent_activation_id, stack_depth,
                status, activation_reason, opened_at
            ) values (?1, ?2, ?3, ?4, 'active', 'follow_up', current_timestamp)
            "#,
            params![
                project_id,
                input.work_unit_id,
                parent.as_ref().map(|activation| activation.activation_id),
                parent
                    .as_ref()
                    .map(|activation| activation.stack_depth + 1)
                    .unwrap_or(0)
            ],
        )?;
        let activation_id = tx.last_insert_rowid();
        if let Some(parent) = &parent {
            tx.execute(
                "update work_unit_activations set suspended_by_activation_id=?1 where id=?2",
                params![activation_id, parent.activation_id],
            )?;
        }
        insert_event(
            &tx,
            NewEvent {
                work_unit_id: input.work_unit_id,
                activation_id: Some(activation_id),
                related_activation_id: parent.as_ref().map(|activation| activation.activation_id),
                event_type: "opened",
                reason: Some(reason),
                status_domain: "activation",
                previous_status: None,
                next_status: Some("active"),
            },
        )?;
        tx.commit()?;
        return Ok(WorkOutcome {
            work_unit_id: input.work_unit_id,
            activation_id,
        });
    }

    let prior_activation_count: i64 = tx.query_row(
        "select count(*) from work_unit_activations where work_unit_id = ?1",
        params![input.work_unit_id],
        |row| row.get(0),
    )?;
    if prior_activation_count > 0 {
        bail!("work unit already has activation history; use resume or reopen flow");
    }

    tx.execute(
        r#"
        insert into work_unit_activations(
            project_id, work_unit_id, stack_depth, status, activation_reason, opened_at
        )
        values (?1, ?2, 0, 'active', 'start', current_timestamp)
        "#,
        params![project_id, input.work_unit_id],
    )?;
    let activation_id = tx.last_insert_rowid();

    insert_event(
        &tx,
        NewEvent {
            work_unit_id: input.work_unit_id,
            activation_id: Some(activation_id),
            related_activation_id: None,
            event_type: "opened",
            reason: input.reason,
            status_domain: "activation",
            previous_status: None,
            next_status: Some("active"),
        },
    )?;
    tx.commit()?;

    Ok(WorkOutcome {
        work_unit_id: input.work_unit_id,
        activation_id,
    })
}

pub(super) fn ensure_implementation_intent_start(input: &WorkStart<'_>) -> Result<()> {
    if reserved_implementation_title(input.title) && !input.implementation {
        bail!(
            "implementation work requires explicit intent; run the design workflow, then use agent-workbench work activate --implementation --design-version <design-version-id> <work-unit-id>"
        );
    }
    if input.design_version_id.is_some() && !input.implementation {
        bail!(
            "design-bound implementation work start requires --implementation and an existing decomposed work unit; use agent-workbench work activate --implementation --design-version <design-version-id> <work-unit-id>"
        );
    }
    if input.implementation && input.design_version_id.is_none() {
        bail!(
            "implementation work start requires --design-version; create or import a design package, then run design-ready, decompose design, and implementation-ready first"
        );
    }
    if input.implementation {
        bail!(
            "design-derived implementation must activate the work unit produced by decompose design; use agent-workbench work activate --implementation --design-version <design-version-id> <work-unit-id>"
        );
    }
    Ok(())
}

pub(super) fn ensure_implementation_intent_activate(input: &WorkActivate<'_>) -> Result<()> {
    if input.design_version_id.is_some() && !input.implementation {
        bail!(
            "design-bound implementation work activation requires --implementation; use agent-workbench work activate --implementation --design-version <design-version-id> <work-unit-id>"
        );
    }
    if input.implementation && input.design_version_id.is_none() {
        bail!("implementation work activation requires --design-version");
    }
    Ok(())
}

pub(super) fn reserved_implementation_title(title: &str) -> bool {
    title.trim().eq_ignore_ascii_case("implementation")
}

pub(super) fn ensure_work_unit_has_design_scope(
    conn: &Connection,
    project_id: i64,
    work_unit_id: i64,
    design_version_id: i64,
) -> Result<()> {
    let owns_design_scope: bool = conn.query_row(
        r#"
        select exists(
            select 1
            from checklists
            where project_id = ?1
              and work_unit_id = ?2
              and design_version_id = ?3
              and status in ('active', 'stale', 'closed')
            union
            select 1
            from task_derivations td
            join design_requirements r on r.id = td.design_requirement_id
            where td.project_id = ?1
              and td.task_id in (
                select t.id
                from tasks t
                join work_units wu on wu.id = t.work_unit_id
                where wu.project_id = ?1
                  and t.work_unit_id = ?2
              )
              and r.design_version_id = ?3
              and td.status in ('active', 'stale', 'closed')
        )
        "#,
        params![project_id, work_unit_id, design_version_id],
        |row| row.get(0),
    )?;
    if !owns_design_scope {
        bail!(
            "implementation work activation requires the target work unit to own design-derived records for the supplied design version"
        );
    }
    Ok(())
}
