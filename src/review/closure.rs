use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{current_phase_blocker, open_existing_project, project_id};

use super::{correction_contract::*, *};

pub fn ready_closure(root: &Path, input: ClosureReady<'_>) -> Result<ClosureReadyOutcome> {
    require_text(
        Some(input.implementation_evidence),
        "closure ready requires --evidence",
    )?;
    require_text(Some(input.tests_or_gates), "closure ready requires --tests")?;
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    if let Some(blocker) = current_phase_blocker(&tx)? {
        let selected_ready = blocker
            .next_action
            .starts_with("agent-workbench closure ready")
            && blocker.next_action.contains(&input.closure_id.to_string());
        let stale_design_recovery: bool = if blocker.kind == "stale_design" {
            tx.query_row(
                r#"
                select exists(
                  select 1 from correction_sessions session
                  where session.closure_id=?1 and session.status='active'
                    and exists(
                      select 1 from correction_tokens applied
                      where applied.closure_id=session.closure_id
                        and applied.token_kind='transition'
                        and applied.operation in ('design-decompose','design-reconcile','decomposition-plan-reconcile')
                        and applied.status='applied'
                    )
                    and not exists(
                      select 1 from correction_tokens pending
                      where pending.closure_id=session.closure_id
                        and pending.token_kind='transition'
                        and pending.status='pending'
                    )
                )
                "#,
                params![input.closure_id],
                |row| row.get(0),
            )?
        } else {
            false
        };
        let selected_source_correction: bool = if blocker.kind == "source_correction"
            && blocker
                .next_action
                .contains(&format!("closure ready {}", input.closure_id))
        {
            tx.query_row(
                r#"
                select exists(
                  select 1 from correction_sessions session
                  join closures closure on closure.id=session.closure_id
                  where session.closure_id=?1 and session.status='active'
                    and closure.finding_id=?2
                )
                "#,
                params![input.closure_id, blocker.finding_id],
                |row| row.get(0),
            )?
        } else {
            false
        };
        if !selected_ready && !stale_design_recovery && !selected_source_correction {
            bail!(
                "closure ready is not the selected action; next: {}",
                blocker.next_action
            );
        }
    }
    let (finding_id, status): (i64, String) = tx
        .query_row(
            r#"
            select c.finding_id, c.status
            from closures c join findings f on f.id = c.finding_id
            where c.id = ?1 and c.project_id = ?2
              and f.status = 'open' and f.classification = 'valid'
              and not exists(
                select 1 from acceptance_records accepted
                where accepted.finding_id=f.id and accepted.target_type='finding'
                  and accepted.status='approved'
                  and accepted.acceptance_type in (
                    'accepted_out_of_scope','explicit_exception','classified_failure'
                  )
              )
            "#,
            params![input.closure_id, project_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .context("open valid closure not found")?;
    if status != "registered" {
        bail!("closure ready requires a registered closure");
    }
    let eligible_owner: Option<i64> = tx
        .query_row(
            r#"
            select p.work_unit_id
            from findings f
            join review_runs r on r.id = f.review_run_id
            join review_plans p on p.id = r.review_plan_id
            where f.id = ?1 and p.project_id = ?2 and p.required = 1
              and p.stage = 'close-ready'
              and p.review_type in ('implementation_review', 'design_implementation_diff')
              and p.status not in ('exhausted', 'needs_user_decision')
              and not exists(
                select 1 from acceptance_records accepted
                where accepted.finding_id=f.id and accepted.target_type='finding'
                  and accepted.status='approved'
                  and accepted.acceptance_type in (
                    'accepted_out_of_scope','explicit_exception','classified_failure'
                  )
              )
              and not exists(
                select 1 from correction_tokens token where token.closure_id=?3
              )
            "#,
            params![finding_id, project_id, input.closure_id],
            |row| row.get(0),
        )
        .optional()?;
    let mut correction_session_id = None;
    if let Some(work_unit_id) = eligible_owner {
        let selected_finding_id: i64 = tx.query_row(
            r#"
            select min(b.finding_id)
            from finding_remediation_bindings b
            join findings selected_f on selected_f.id = b.finding_id
              and selected_f.status = 'open' and selected_f.classification = 'valid'
            join closures selected_c on selected_c.id = b.closure_id
              and selected_c.status = 'registered'
            join work_unit_activations selected_a on selected_a.id = b.work_unit_activation_id
              and selected_a.status = 'active'
            join review_runs selected_r on selected_r.id = selected_f.review_run_id
            join review_plans selected_p on selected_p.id = selected_r.review_plan_id
            where b.work_unit_id = ?1 and b.project_id = ?2
              and selected_p.required = 1 and selected_p.stage = 'close-ready'
              and selected_p.review_type in ('implementation_review', 'design_implementation_diff')
              and selected_p.status not in ('exhausted', 'needs_user_decision')
              and not exists(
                select 1 from correction_tokens token where token.closure_id=selected_c.id
              )
              and not exists(
                select 1 from acceptance_records accepted
                where accepted.finding_id=selected_f.id
                  and accepted.target_type='finding' and accepted.status='approved'
                  and accepted.acceptance_type in (
                    'accepted_out_of_scope','explicit_exception','classified_failure'
                  )
              )
            "#,
            params![work_unit_id, project_id],
            |row| row.get(0),
        )?;
        if selected_finding_id != finding_id {
            bail!(
                "closure ready targets finding {finding_id}, but finding {selected_finding_id} is selected"
            );
        }
        let bound: i64 = tx.query_row(
            r#"
            select count(*)
            from finding_remediation_bindings b
            join work_unit_activations a on a.id = b.work_unit_activation_id
            join work_units w on w.id = b.work_unit_id
            where b.finding_id = ?1 and b.closure_id = ?2
              and b.work_unit_id = ?3 and b.project_id = ?4
              and a.status = 'active' and a.work_unit_id = b.work_unit_id
              and w.status = 'open'
            "#,
            params![finding_id, input.closure_id, work_unit_id, project_id],
            |row| row.get(0),
        )?;
        if bound == 0 {
            bail!(
                "closure ready requires active audited remediation; run agent-workbench work remediate --finding {finding_id}"
            );
        }
    } else {
        let session_id: i64 = tx
            .query_row(
                "select id from correction_sessions where closure_id = ?1 and status = 'active' order by id desc limit 1",
                params![input.closure_id],
                |row| row.get(0),
            )
            .optional()?
            .with_context(|| {
                format!(
                    "closure ready requires an active correction session; run agent-workbench closure correction-begin {}",
                    input.closure_id
                )
            })?;
        correction_session_id = Some(session_id);
        let pending_transitions: i64 = tx.query_row(
            "select count(*) from correction_tokens where closure_id = ?1 and token_kind = 'transition' and status != 'applied'",
            params![input.closure_id],
            |row| row.get(0),
        )?;
        if pending_transitions > 0 {
            let token: i64 = tx.query_row(
                "select min(token_ordinal) from correction_tokens where closure_id = ?1 and token_kind = 'transition' and status = 'pending'",
                params![input.closure_id],
                |row| row.get(0),
            )?;
            bail!(
                "closure ready requires the selected transition: agent-workbench closure transition apply {} --token {}",
                input.closure_id,
                token
            );
        }
        let recovery_residuals: i64 = tx.query_row(
            r#"
            select
              (select count(*)
               from correction_transition_aliases a
               join tasks t on t.id=a.record_id
               where a.correction_session_id=?1 and a.alias like '@superseded-task/%'
                 and (t.status in ('open','blocked')
                   or exists(select 1 from work_phase_task_memberships m where m.task_id=t.id)
                   or exists(select 1 from task_derivations td where td.task_id=t.id and td.status='active')))
              +
              (select count(*)
               from correction_tokens token
               join task_derivations td
               join design_requirements r on r.id=td.design_requirement_id
               join tasks t on t.id=td.task_id
               left join checklist_items ci on ci.id=td.checklist_item_id
               where token.closure_id=?2 and token.operation='design-reconcile'
                 and token.status='applied' and td.status='active'
                 and r.design_version_id=cast(json_extract('["'||replace(token.target,'/','","')||'"]','$[0]') as integer)
                 and t.work_unit_id=cast(json_extract('["'||replace(token.target,'/','","')||'"]','$[1]') as integer)
                 and coalesce(ci.checklist_id,0)!=cast(json_extract('["'||replace(token.target,'/','","')||'"]','$[2]') as integer))
              +
              (select count(*)
               from correction_tokens token
               join correction_transition_applications app
                 on app.correction_token_id=token.id and app.correction_session_id=?1
               join tasks t on t.id=cast(token.target as integer)
               left join acceptance_records ar
                 on ar.task_id=t.id and ar.status='approved'
                and ar.acceptance_type='accepted_out_of_scope'
                and ar.approved_by_authority_event_id=app.authority_event_id
                and app.result_ref='task:'||t.id||':acceptance:'||ar.id
               where token.closure_id=?2 and token.operation='task-accept-out-of-scope'
                 and token.status='applied' and token.target not glob '*[^0-9]*'
                 and (ar.id is null or t.status!='accepted_out_of_scope'
                   or exists(select 1 from work_phase_task_memberships m where m.task_id=t.id)
                   or exists(select 1 from task_derivations td where td.task_id=t.id and td.status='active')))
            "#,
            params![session_id, input.closure_id],
            |row| row.get(0),
        )?;
        if recovery_residuals > 0 {
            bail!(
                "closure ready requires all reconciled duplicate tasks, memberships, and derivations to be dispositioned"
            );
        }
        let design_root: Option<String> = tx
            .query_row(
                r#"
                select dp.root_path
                from closures c
                join findings f on f.id = c.finding_id
                join review_runs r on r.id = f.review_run_id
                join review_plans p on p.id = r.review_plan_id
                left join design_versions dv on dv.id = p.design_version_id
                left join design_packages dp on dp.id = dv.design_package_id
                where c.id = ?1
                "#,
                params![input.closure_id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        let mut stmt = tx.prepare(
            "select operation, target from correction_tokens where closure_id = ?1 and token_kind = 'file' order by token_ordinal",
        )?;
        let rows = stmt.query_map(params![input.closure_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let file_tokens = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        let mut incomplete_surfaces = Vec::new();
        for (operation, target) in file_tokens {
            let token = CorrectionToken {
                kind: "file".to_string(),
                operation: operation.clone(),
                target: target.clone(),
            };
            let path = correction_file_path(root, design_root.as_deref(), &token)?;
            let pre_hash = effective_file_pre_hash(&tx, input.closure_id, &operation, &target)?;
            match operation.as_str() {
                "create" if !path.is_file() => {
                    incomplete_surfaces.push(format!(
                        "created correction surface is still absent: {}",
                        path.display()
                    ));
                }
                "delete" if path.exists() => {
                    incomplete_surfaces.push(format!(
                        "deleted correction surface still exists: {}",
                        path.display()
                    ));
                }
                "edit" if !path.is_file() => {
                    incomplete_surfaces.push(format!(
                        "edited correction surface is not a regular file: {}",
                        path.display()
                    ));
                }
                "edit" if Some(file_sha256(&path)?) == pre_hash => {
                    incomplete_surfaces.push(format!(
                        "edited correction surface is unchanged: {}",
                        path.display()
                    ));
                }
                _ => {}
            }
        }
        if !incomplete_surfaces.is_empty() {
            bail!(
                "correction surfaces are incomplete:\n{}",
                incomplete_surfaces.join("\n")
            );
        }
    }
    let high_watermark: i64 =
        tx.query_row("select coalesce(max(id), 0) from review_runs", [], |row| {
            row.get(0)
        })?;
    let attempt_number: i64 = tx.query_row(
        "select coalesce(max(attempt_number), 0) + 1 from closure_attempts where closure_id = ?1",
        params![input.closure_id],
        |row| row.get(0),
    )?;
    tx.execute(
        r#"
        insert into closure_attempts(
            project_id, closure_id, attempt_number, implementation_evidence,
            tests_or_gates, closed_by_commit, review_run_high_watermark, created_at
        ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, current_timestamp)
        "#,
        params![
            project_id,
            input.closure_id,
            attempt_number,
            input.implementation_evidence,
            input.tests_or_gates,
            input.closed_by_commit,
            high_watermark,
        ],
    )?;
    let attempt_id = tx.last_insert_rowid();
    tx.execute(
        "update closures set status = 'ready_for_verification' where id = ?1",
        params![input.closure_id],
    )?;
    let prior_lifecycle: String = tx.query_row(
        "select lifecycle_state from findings where project_id=?1 and id=?2",
        params![project_id, finding_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "update findings set lifecycle_state='awaiting_verification' where project_id=?1 and id=?2",
        params![project_id, finding_id],
    )?;
    tx.execute(
        "insert into finding_lifecycle_events(project_id,finding_id,owner_decision_id,from_state,to_state,effect,created_at) values(?1,?2,null,?3,'awaiting_verification','closure_ready',current_timestamp)",
        params![project_id, finding_id, prior_lifecycle],
    )?;
    if let Some(session_id) = correction_session_id {
        tx.execute(
            "update correction_sessions set status = 'completed', completed_at = current_timestamp where id = ?1",
            params![session_id],
        )?;
    }
    tx.commit()?;
    Ok(ClosureReadyOutcome {
        closure_id: input.closure_id,
        finding_id,
        attempt_id,
        attempt_number,
        context_ref: finding_fix_context_ref(finding_id, input.closure_id, attempt_id),
    })
}

pub fn supersede_closure(
    root: &Path,
    input: ClosureSupersession<'_>,
) -> Result<ClosureSupersessionOutcome> {
    require_text(Some(input.reason), "closure supersede requires --reason")?;
    require_text(
        Some(input.new_closure.design_invariant),
        "closure supersede requires --invariant",
    )?;
    require_text(
        input.new_closure.affected_surfaces,
        "closure supersede requires --surfaces",
    )?;
    require_text(
        input.new_closure.fix_plan,
        "closure supersede requires --fix-plan",
    )?;
    require_text(
        input.new_closure.tests_or_gates,
        "closure supersede requires --tests",
    )?;
    require_text(
        input.new_closure.verification_plan,
        "closure supersede requires --verification",
    )?;
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    ensure_active_acceptance_authority(&tx, project_id, input.authority_event_id)?;
    let finding_id: i64 = tx
        .query_row(
            r#"
            select c.finding_id from closures c join findings f on f.id = c.finding_id
            where c.id = ?1 and c.project_id = ?2
              and c.status in ('registered', 'incomplete')
              and f.status = 'open' and f.classification = 'valid'
              and not exists(
                select 1 from acceptance_records accepted
                where accepted.finding_id=f.id and accepted.target_type='finding'
                  and accepted.status='approved'
                  and accepted.acceptance_type in (
                    'accepted_out_of_scope','explicit_exception','classified_failure'
                  )
              )
            "#,
            params![input.closure_id, project_id],
            |row| row.get(0),
        )
        .optional()?
        .context("current registered or incomplete closure not found")?;
    ensure_review_finding_target(&tx, finding_id, "closure supersede")?;
    if let Some(blocker) = current_phase_blocker(&tx)? {
        let selected_active_correction = blocker.next_action.starts_with(&format!(
            "agent-workbench closure transition apply {} --token ",
            input.closure_id
        )) || blocker.kind == "source_correction"
            && blocker.finding_id == Some(finding_id)
            && blocker
                .next_action
                .contains(&format!("closure ready {}", input.closure_id));
        let selected_contract_action = blocker
            .next_action
            .starts_with("agent-workbench closure supersede")
            || blocker
                .next_action
                .starts_with("agent-workbench work remediate")
            || blocker
                .next_action
                .starts_with("agent-workbench closure correction-begin")
            || selected_active_correction;
        let selected_stale_recovery = if blocker.kind == "stale_design" {
            let selected = crate::traceability::selected_stale_record_in(&tx, project_id)?;
            let expected_operation = blocker
                .next_action
                .starts_with("agent-workbench stale accept ")
                .then_some("stale-accept")
                .or_else(|| {
                    blocker
                        .next_action
                        .starts_with("agent-workbench stale close ")
                        .then_some("stale-close")
                });
            match (
                selected,
                expected_operation,
                input.new_closure.affected_surfaces,
            ) {
                (Some((kind, id)), Some(operation), Some(surfaces)) => {
                    let target = format!("{kind}/{id}");
                    parse_correction_tokens(surfaces)?.iter().any(|token| {
                        token.kind == "transition"
                            && token.operation == operation
                            && token.target == target
                    })
                }
                _ => false,
            }
        } else {
            false
        };
        if !selected_contract_action && !selected_stale_recovery {
            bail!(
                "closure supersede is not selected; next: {}",
                blocker.next_action
            );
        }
    }
    let surfaces = input.new_closure.affected_surfaces.unwrap();
    let source_correction = declares_typed_correction(surfaces);
    let eligible =
        finding_is_remediation_eligible(&tx, project_id, finding_id)? && !source_correction;
    if source_correction || !eligible {
        parse_correction_tokens(input.new_closure.affected_surfaces.unwrap())?;
    }
    tx.execute(
        r#"
        insert into closures(
            project_id, finding_id, design_invariant, design_citations,
            implementation_evidence, affected_surfaces, same_invariant_search,
            other_violations_found, fix_plan, tests_or_gates,
            verification_plan, closed_by_commit, status, created_at
        ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'registered', current_timestamp)
        "#,
        params![
            project_id, finding_id, input.new_closure.design_invariant,
            input.new_closure.design_citations, input.new_closure.implementation_evidence,
            input.new_closure.affected_surfaces, input.new_closure.same_invariant_search,
            input.new_closure.other_violations_found, input.new_closure.fix_plan,
            input.new_closure.tests_or_gates, input.new_closure.verification_plan,
            input.new_closure.closed_by_commit,
        ],
    )?;
    let new_closure_id = tx.last_insert_rowid();
    if source_correction || !eligible {
        let design_root = correction_design_root(&tx, finding_id)?;
        record_correction_tokens(
            &tx,
            root,
            project_id,
            new_closure_id,
            input.new_closure.affected_surfaces.unwrap(),
            design_root.as_deref(),
        )?;
    }
    tx.execute(
        "update closures set status = 'superseded', superseded_by_closure_id = ?1, superseded_at = current_timestamp, superseded_by_authority_event_id = ?2, supersession_reason = ?3 where id = ?4",
        params![new_closure_id, input.authority_event_id, input.reason, input.closure_id],
    )?;
    tx.execute(
        "update correction_tokens set status='superseded' where closure_id=?1 and status='pending'",
        params![input.closure_id],
    )?;
    tx.execute(
        "update correction_sessions set status = 'superseded', completed_at = current_timestamp where closure_id = ?1 and status = 'active'",
        params![input.closure_id],
    )?;
    tx.commit()?;
    Ok(ClosureSupersessionOutcome {
        closure_id: new_closure_id,
        superseded_closure_id: input.closure_id,
        finding_id,
    })
}

pub fn accept_finding_out_of_scope(
    root: &Path,
    input: FindingOutOfScope<'_>,
) -> Result<FindingOutOfScopeOutcome> {
    require_text(
        Some(input.reason),
        "out-of-scope disposition requires --reason",
    )?;
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    ensure_review_finding_target(&tx, input.finding_id, "finding accept-out-of-scope")?;
    ensure_active_acceptance_authority(&tx, project_id, input.authority_event_id)?;
    let review_plan_id: i64 = tx
        .query_row(
            r#"
            select r.review_plan_id from findings f join review_runs r on r.id = f.review_run_id
            where f.id = ?1 and f.project_id = ?2 and f.status = 'open'
            "#,
            params![input.finding_id, project_id],
            |row| row.get(0),
        )
        .optional()?
        .context("open finding not found")?;
    tx.execute(
        r#"
        insert into acceptance_records(
            project_id, target_type, finding_id, acceptance_type, reason,
            created_by, status, approved_by_authority_event_id,
            approved_at, created_at
        ) values (?1, 'finding', ?2, 'accepted_out_of_scope', ?3,
                  'user', 'approved', ?4, current_timestamp, current_timestamp)
        "#,
        params![
            project_id,
            input.finding_id,
            input.reason,
            input.authority_event_id
        ],
    )?;
    let acceptance_record_id = tx.last_insert_rowid();
    tx.execute(
        "update closure_attempts set result = 'superseded', resolved_at = current_timestamp where result is null and closure_id in (select id from closures where finding_id = ?1 and status = 'ready_for_verification')",
        params![input.finding_id],
    )?;
    tx.execute(
        "update closures set status = 'superseded', superseded_at = current_timestamp, superseded_by_authority_event_id = ?1 where finding_id = ?2 and status != 'superseded'",
        params![input.authority_event_id, input.finding_id],
    )?;
    tx.execute(
        "update correction_sessions set status = 'superseded', completed_at = current_timestamp where finding_id = ?1 and status = 'active'",
        params![input.finding_id],
    )?;
    tx.execute(
        "update findings set status = 'accepted_out_of_scope' where id = ?1",
        params![input.finding_id],
    )?;
    let owner_work_unit_id: i64 = tx.query_row(
        r#"
        select p.work_unit_id from findings f
        join review_runs r on r.id = f.review_run_id
        join review_plans p on p.id = r.review_plan_id
        where f.id = ?1
        "#,
        params![input.finding_id],
        |row| row.get(0),
    )?;
    let surviving_candidates: i64 = tx.query_row(
        r#"
        select count(*)
        from findings f
        join review_runs r on r.id = f.review_run_id
        join review_plans p on p.id = r.review_plan_id
        join closures c on c.finding_id = f.id and c.status = 'registered'
        where p.work_unit_id = ?1 and f.status = 'open' and f.classification = 'valid'
          and p.required = 1 and p.stage = 'close-ready'
          and p.review_type in ('implementation_review', 'design_implementation_diff')
        "#,
        params![owner_work_unit_id],
        |row| row.get(0),
    )?;
    if surviving_candidates == 0 {
        tx.execute(
            r#"
            update work_unit_dependencies
            set status = 'resolved', resolved_at = current_timestamp,
                resolved_by_work_unit_event_id = (
                    select epoch.reopened_event_id
                    from finding_remediation_recovery_epochs epoch
                    where epoch.dependency_id = work_unit_dependencies.id
                    order by epoch.id desc limit 1
                )
            where status = 'open' and id in (
                select epoch.dependency_id
                from finding_remediation_recovery_epochs epoch
                where epoch.work_unit_id = ?1
            )
            "#,
            params![owner_work_unit_id],
        )?;
    }
    let watermark: i64 =
        tx.query_row("select coalesce(max(id), 0) from review_runs", [], |row| {
            row.get(0)
        })?;
    tx.execute(
        "update review_plans set fresh_review_after_run_id = ?1 where id = ?2",
        params![watermark, review_plan_id],
    )?;
    tx.commit()?;
    Ok(FindingOutOfScopeOutcome {
        finding_id: input.finding_id,
        acceptance_record_id,
    })
}

pub fn finding_fix_context_ref(finding_id: i64, closure_id: i64, attempt_id: i64) -> String {
    format!(
        "review-context:finding-fix:finding={finding_id}:closure={closure_id}:attempt={attempt_id}"
    )
}

pub(super) fn ensure_active_acceptance_authority(
    conn: &rusqlite::Connection,
    project_id: i64,
    authority_event_id: i64,
) -> Result<()> {
    let valid: bool = conn.query_row(
        r#"
        select exists(select 1 from authority_events
                      where id = ?1 and project_id = ?2 and status = 'active'
                        and event_type in ('user_instruction', 'policy', 'design_doc'))
        "#,
        params![authority_event_id, project_id],
        |row| row.get(0),
    )?;
    if !valid {
        bail!("operation requires an active user, policy, or design authority event");
    }
    Ok(())
}

pub(super) fn ensure_review_finding_target(
    conn: &rusqlite::Connection,
    finding_id: i64,
    operation: &str,
) -> Result<()> {
    let published: bool = conn.query_row(
        "select exists(select 1 from findings f join review_runs r on r.id=f.review_run_id where f.id=?1 and r.status='completed' and (select count(*) from findings inventory where inventory.review_run_id=r.id)=r.new_findings_count)",
        [finding_id],
        |row| row.get(0),
    )?;
    if !published {
        bail!("{operation} requires a completed review finding inventory");
    }
    let terminally_accepted: bool = conn.query_row(
        r#"
        select exists(
          select 1 from acceptance_records accepted
          where accepted.finding_id=?1 and accepted.target_type='finding'
            and accepted.status='approved'
            and accepted.acceptance_type in (
              'accepted_out_of_scope','explicit_exception','classified_failure'
            )
        )
        "#,
        [finding_id],
        |row| row.get(0),
    )?;
    if terminally_accepted {
        bail!("{operation} requires a finding without a terminal acceptance");
    }
    if let Some(blocker) = current_phase_blocker(conn)? {
        let same_selected_finding =
            blocker.kind == "required_review_finding" && blocker.finding_id == Some(finding_id);
        let same_selected_correction = blocker.kind == "source_correction"
            && operation == "closure supersede"
            && blocker.finding_id == Some(finding_id);
        let same_owner_stale_recovery = if blocker.kind == "stale_design"
            && matches!(operation, "closure add" | "closure supersede")
        {
            if let Some(work_unit_id) = blocker.work_unit_id {
                conn.query_row(
                    "select exists(select 1 from findings f join review_runs r on r.id=f.review_run_id join review_plans p on p.id=r.review_plan_id where f.id=?1 and p.work_unit_id=?2 and f.status='open' and f.classification='valid')",
                    params![finding_id, work_unit_id],
                    |row| row.get::<_, bool>(0),
                )?
            } else {
                false
            }
        } else {
            false
        };
        if !same_selected_finding && !same_selected_correction && !same_owner_stale_recovery {
            bail!(
                "{operation} is not allowed under the selected resolver action; next: {}",
                blocker.next_action
            );
        }
        return Ok(());
    }
    let (target_is_active, selected_active_finding): (bool, Option<i64>) = conn.query_row(
        r#"
        with active_scopes(finding_id) as (
          select b.finding_id
          from finding_remediation_bindings b
          join findings f on f.id = b.finding_id and f.status = 'open' and f.classification = 'valid'
          join closures c on c.id = b.closure_id and c.status = 'registered'
          join work_unit_activations a on a.id = b.work_unit_activation_id and a.status = 'active'
          join review_runs r on r.id=f.review_run_id
          join review_plans p on p.id=r.review_plan_id
          where p.required=1 and p.stage='close-ready'
            and p.review_type in ('implementation_review','design_implementation_diff')
            and p.status not in ('exhausted','needs_user_decision')
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
          union
          select s.finding_id
          from correction_sessions s
          join findings f on f.id = s.finding_id and f.status = 'open' and f.classification = 'valid'
          join closures c on c.id = s.closure_id and c.status = 'registered'
          where s.status = 'active'
            and not exists(
              select 1 from acceptance_records accepted
              where accepted.finding_id=f.id and accepted.target_type='finding'
                and accepted.status='approved'
                and accepted.acceptance_type in (
                  'accepted_out_of_scope','explicit_exception','classified_failure'
                )
            )
        )
        select exists(select 1 from active_scopes where finding_id=?1),
               (select min(finding_id) from active_scopes)
        "#,
        [finding_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if selected_active_finding.is_some() && !target_is_active {
        bail!(
            "{operation} targets finding {finding_id}, but active scoped finding {selected_active_finding:?} is selected"
        );
    }
    Ok(())
}

pub(super) fn finding_is_remediation_eligible(
    conn: &rusqlite::Connection,
    project_id: i64,
    finding_id: i64,
) -> Result<bool> {
    conn.query_row(
        r#"
        select exists(
            select 1
            from findings f
            join review_runs r on r.id = f.review_run_id
            join review_plans p on p.id = r.review_plan_id
            where f.id = ?1 and f.project_id = ?2
              and p.required = 1 and p.stage = 'close-ready'
              and p.review_type in ('implementation_review', 'design_implementation_diff')
              and p.status not in ('exhausted', 'needs_user_decision')
              and not exists(
                select 1 from acceptance_records accepted
                where accepted.finding_id=f.id and accepted.target_type='finding'
                  and accepted.status='approved'
                  and accepted.acceptance_type in (
                    'accepted_out_of_scope','explicit_exception','classified_failure'
                  )
              )
        )
        "#,
        params![finding_id, project_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn require_text(value: Option<&str>, message: &str) -> Result<()> {
    if value.is_none_or(|value| value.trim().is_empty()) {
        bail!(message.to_string());
    }
    Ok(())
}

pub fn add_finding_verification(
    root: &Path,
    input: NewFindingVerification<'_>,
) -> Result<FindingVerificationOutcome> {
    insert_finding_verification(
        root,
        FindingVerificationInput {
            review_run_id: input.review_run_id,
            finding_id: input.finding_id,
            closure_id: input.closure_id,
            closure_attempt_id: None,
            result: input.result,
            notes: input.notes,
        },
    )
}

pub fn add_finding_verification_for_attempt(
    root: &Path,
    input: NewFindingVerificationForAttempt<'_>,
) -> Result<FindingVerificationOutcome> {
    insert_finding_verification(
        root,
        FindingVerificationInput {
            review_run_id: input.review_run_id,
            finding_id: input.finding_id,
            closure_id: input.closure_id,
            closure_attempt_id: Some(input.closure_attempt_id),
            result: input.result,
            notes: input.notes,
        },
    )
}

struct FindingVerificationInput<'a> {
    review_run_id: i64,
    finding_id: i64,
    closure_id: i64,
    closure_attempt_id: Option<i64>,
    result: &'a str,
    notes: Option<&'a str>,
}

fn insert_finding_verification(
    root: &Path,
    input: FindingVerificationInput<'_>,
) -> Result<FindingVerificationOutcome> {
    if !matches!(input.result, "verified" | "not_fixed" | "needs_evidence") {
        bail!(
            "finding verification result must be verified|not_fixed|needs_evidence; use finding accept-out-of-scope for authority disposition"
        );
    }
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let verification_run = tx
        .query_row(
            r#"
            select run_type, run_purpose, finding_fix_result, clean_run,
                   new_findings_count, carried_findings_checked, target_ref,
                   review_provenance, review_provenance_ref,
                   exists(select 1 from review_agent_invocations i
                          where i.review_run_id = review_runs.id
                            and coalesce(i.external_agent_id, '') != '')
            from review_runs
            where id = ?1 and project_id = ?2
            "#,
            params![input.review_run_id, project_id],
            |row| {
                Ok(StoredReviewRunPurpose {
                    run_type: row.get(0)?,
                    run_purpose: row.get(1)?,
                    finding_fix_result: row.get(2)?,
                    clean_run: row.get::<_, i64>(3)? == 1,
                    new_findings_count: row.get(4)?,
                    carried_findings_checked: row.get(5)?,
                    _target_ref: row.get(6)?,
                    review_provenance: row.get(7)?,
                    review_provenance_ref: row.get(8)?,
                    has_external_agent: row.get::<_, i64>(9)? == 1,
                })
            },
        )
        .optional()?
        .context("review run not found")?;
    if verification_run.run_type != "resume"
        || verification_run.run_purpose != "finding_fix_verification"
    {
        bail!("finding verification requires a resume finding_fix_verification run");
    }
    if verification_run.finding_fix_result.as_deref() != Some(input.result)
        || verification_run.new_findings_count != 0
        || verification_run.carried_findings_checked != 1
        || (input.result == "verified") != verification_run.clean_run
    {
        bail!("finding verification result is inconsistent with the resume review outcome");
    }
    let trusted = match verification_run.review_provenance.as_str() {
        "external_agent" => {
            verification_run.has_external_agent
                && verification_run
                    .review_provenance_ref
                    .as_deref()
                    .is_some_and(|v| !v.trim().is_empty())
        }
        "human_review" => verification_run
            .review_provenance_ref
            .as_deref()
            .is_some_and(|v| !v.trim().is_empty()),
        _ => false,
    };
    if !trusted {
        bail!("finding verification requires trusted review provenance");
    }
    let attempt_id: i64 = tx
        .query_row(
            r#"
        select a.id
        from closures c
        join closure_attempts a on a.closure_id = c.id and a.result is null
        join findings f on f.id = c.finding_id
        join review_runs verifier on verifier.id = ?1 and verifier.project_id = ?5
        join review_runs source_run on source_run.id = f.review_run_id
        join review_plans source_plan on source_plan.id = source_run.review_plan_id
        join review_plans verifier_plan on verifier_plan.id = verifier.review_plan_id
        where c.id = ?2
          and c.finding_id = ?3
          and c.project_id = ?5
          and f.project_id = ?5
          and f.id = ?3
          and verifier_plan.work_unit_id = source_plan.work_unit_id
          and verifier_plan.review_type = source_plan.review_type
          and verifier_plan.stage = source_plan.stage
          and (
            exists(
              select 1 from finding_design_recoveries recovery
              where recovery.project_id=?5
                and recovery.successor_closure_id=c.id
                and recovery.successor_attempt_id=a.id
                and recovery.successor_design_version_id=verifier_plan.design_version_id
            )
            or (
              not exists(
                select 1 from finding_design_recoveries recovery
                where recovery.project_id=?5 and recovery.successor_closure_id=c.id
              )
              and (
                verifier_plan.design_version_id is source_plan.design_version_id
                or exists(
                  select 1 from design_versions source_design
                  join design_versions verifier_design
                    on verifier_design.design_package_id=source_design.design_package_id
                  where source_design.id=source_plan.design_version_id
                    and verifier_design.id=verifier_plan.design_version_id
                    and verifier_design.version_number>=source_design.version_number
                    and verifier_design.status='approved'
                )
              )
            )
          )
          and coalesce(verifier_plan.scope, '') = coalesce(source_plan.scope, '')
          and c.status = 'ready_for_verification'
          and verifier.id > a.review_run_high_watermark
          and verifier.target_ref = 'review-context:finding-fix:finding=' || f.id
                    || ':closure=' || c.id || ':attempt=' || a.id
          and (?6 is null or a.id = ?6)
        "#,
            params![
                input.review_run_id,
                input.closure_id,
                input.finding_id,
                input.result,
                project_id,
                input.closure_attempt_id,
            ],
            |row| row.get(0),
        )
        .optional()?
        .context("finding verification target mismatch")?;
    tx.execute(
        r#"
        insert into finding_verifications(
            project_id, review_run_id, finding_id, closure_id, closure_attempt_id,
            result, notes, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, current_timestamp)
        "#,
        params![
            project_id,
            input.review_run_id,
            input.finding_id,
            input.closure_id,
            attempt_id,
            input.result,
            input.notes,
        ],
    )?;
    let finding_verification_id = tx.last_insert_rowid();
    tx.commit()?;
    Ok(FindingVerificationOutcome {
        finding_verification_id,
    })
}
