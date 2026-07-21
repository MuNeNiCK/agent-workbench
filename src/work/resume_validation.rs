use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use crate::db::{StoredActivation, max_id, project_id, suspend_snapshot};

use super::{close_repository::*, close_trace::*, forking::*, *};

pub(super) fn evaluate_resume_ready_for(
    conn: &Connection,
    work_unit_id: Option<i64>,
    maturity: &str,
) -> Result<ResumeGateEvaluation> {
    if !matches!(maturity, "basic" | "trace-aware" | "repo-aware") {
        bail!("unsupported maturity; use basic, trace-aware, or repo-aware");
    }
    let target = resolve_suspended_activation(conn, work_unit_id)?;
    let snapshot = suspend_snapshot(conn, target.activation_id)?;
    let stack_revision = max_id(conn, "work_unit_events")?;
    let authority_high_watermark = max_id(conn, "authority_events")?;

    let deeper_open = conn.query_row(
        r#"
        select count(*)
        from work_unit_activations
        where project_id = ?1
          and stack_depth > ?2
          and status not in ('completed', 'abandoned')
        "#,
        params![target.project_id, target.stack_depth],
        |row| row.get::<_, i64>(0),
    )?;
    let blocking_dependencies = conn.query_row(
        r#"
        select count(*)
        from work_unit_dependencies d
        where d.work_unit_id = ?1
          and d.dependency_type in ('blocks', 'invalidates_assumption', 'invalidates_closure')
          and d.status = 'open'
          and exists(select 1 from work_units dependency_target where dependency_target.id=d.depends_on_work_unit_id and dependency_target.status in ('open','blocked'))
        "#,
        params![target.work_unit_id],
        |row| row.get::<_, i64>(0),
    )?;

    let checks = [
        (
            "resume_target_suspended",
            target.status == "suspended",
            "target activation must be suspended",
        ),
        (
            "snapshot_exists",
            true,
            "suspend snapshot must exist for target activation",
        ),
        (
            "suspend_reason_exists",
            !snapshot.reason.trim().is_empty(),
            "suspend snapshot must include a reason",
        ),
        (
            "next_action_exists",
            !snapshot.next_action.trim().is_empty(),
            "suspend snapshot must include a next action",
        ),
        (
            "deeper_frames_closed",
            deeper_open == 0,
            "deeper activation frames must be completed or abandoned",
        ),
        (
            "blocking_dependencies_clear",
            blocking_dependencies == 0,
            "blocking dependencies must be resolved",
        ),
    ];
    let basic_allowed = checks.iter().all(|(_, pass, _)| *pass);
    let mut blocking_reason = checks
        .iter()
        .find_map(|(_, pass, message)| (!pass).then_some((*message).to_string()));
    let mut items: Vec<_> = checks
        .into_iter()
        .map(|(name, pass, message)| ResumeReadyItem {
            name: name.to_string(),
            result: if pass { "pass" } else { "fail" }.to_string(),
            blocking_action: (!pass).then_some(message.to_string()),
            details: message.to_string(),
        })
        .collect();

    let trace_maturity = matches!(maturity, "trace-aware" | "repo-aware");
    let trace_counts = trace_maturity
        .then(|| trace_resume_counts(conn, target.work_unit_id))
        .transpose()?;
    let mut trace_allowed = true;
    if let Some(trace_counts) = trace_counts {
        let active_tasks_current = snapshot.active_task_ids.as_deref().unwrap_or("")
            == snapshot_active_task_ids(conn, target.work_unit_id)?;
        let current_authority_refs = snapshot_authority_refs(conn)?;
        let authority_refs_current = snapshot_entries_still_current(
            snapshot.authority_refs.as_deref().unwrap_or(""),
            &current_authority_refs,
        );
        let review_scope_refs_current = snapshot.review_scope_refs.as_deref().unwrap_or("")
            == snapshot_review_scope_refs(conn, target.work_unit_id)?;
        let open_findings_current = snapshot.open_findings.as_deref().unwrap_or("")
            == snapshot_open_findings(conn, target.work_unit_id)?;
        for (name, pass, details) in [
            (
                "active_tasks_current",
                active_tasks_current,
                "active task set matches suspend snapshot".to_string(),
            ),
            (
                "authority_refs_current",
                authority_refs_current,
                "authority refs captured at suspend remain active; newer refs are loaded on resume"
                    .to_string(),
            ),
            (
                "review_scope_refs_current",
                review_scope_refs_current,
                "review scope refs match suspend snapshot".to_string(),
            ),
            (
                "open_findings_current",
                open_findings_current,
                "open findings match suspend snapshot".to_string(),
            ),
        ] {
            if !pass {
                trace_allowed = false;
                blocking_reason
                    .get_or_insert_with(|| "trace-aware resume checks failed".to_string());
            }
            items.push(ResumeReadyItem {
                name: name.to_string(),
                result: if pass { "pass" } else { "fail" }.to_string(),
                blocking_action: (!pass).then_some(details.clone()),
                details,
            });
        }
        let stale_design_total =
            trace_counts.stale_design_records + trace_counts.stale_coverage_items;
        let selected_gate_snapshot_current =
            snapshot.selected_gate_id == snapshot_selected_gate_id(conn, target.work_unit_id)?;
        let trace_items = [
            (
                "design_version_current",
                stale_design_total == 0,
                format!(
                    "{} design-derived records and {} coverage items reference changed requirements",
                    trace_counts.stale_design_records, trace_counts.stale_coverage_items
                ),
            ),
            (
                "task_derivation_current",
                trace_counts.stale_task_derivations == 0,
                format!(
                    "{} task derivations reference changed requirements",
                    trace_counts.stale_task_derivations
                ),
            ),
            (
                "checklist_current",
                trace_counts.stale_checklists == 0,
                format!(
                    "{} checklists reference changed requirements",
                    trace_counts.stale_checklists
                ),
            ),
            (
                "selected_gate_current",
                trace_counts.stale_selected_gates == 0 && selected_gate_snapshot_current,
                format!(
                    "{} selected validation gates reference changed requirements; snapshot match={}",
                    trace_counts.stale_selected_gates, selected_gate_snapshot_current
                ),
            ),
        ];
        for (name, pass, details) in trace_items {
            if !pass {
                trace_allowed = false;
                blocking_reason
                    .get_or_insert_with(|| "trace-aware resume checks failed".to_string());
            }
            items.push(ResumeReadyItem {
                name: name.to_string(),
                result: if pass { "pass" } else { "fail" }.to_string(),
                blocking_action: (!pass).then(|| details.clone()),
                details,
            });
        }
        let review_state = review_plan_stage_state(conn, target.work_unit_id, "resume-ready")?;
        let review_pass = review_state.required_plan_count == 0
            || (review_state.incomplete_required_plan_count == 0
                && review_state.stale_target_count == 0);
        if !review_pass {
            trace_allowed = false;
            blocking_reason.get_or_insert_with(|| "trace-aware resume checks failed".to_string());
        }
        items.push(ResumeReadyItem {
            name: "review_plan_current".to_string(),
            result: if review_pass { "pass" } else { "fail" }.to_string(),
            blocking_action: (!review_pass).then_some(
                "complete required resume-ready plans or refresh stale targets".to_string(),
            ),
            details: format!(
                "{} required resume-ready plans, {} incomplete, {} stale targets",
                review_state.required_plan_count,
                review_state.incomplete_required_plan_count,
                review_state.stale_target_count
            ),
        });
    } else {
        let later_items = [
            (
                "active_tasks_current",
                "trace-aware active task snapshot check was not requested",
            ),
            (
                "authority_refs_current",
                "trace-aware authority refs snapshot check was not requested",
            ),
            (
                "review_scope_refs_current",
                "trace-aware review scope refs snapshot check was not requested",
            ),
            (
                "design_version_current",
                "trace-aware design version check was not requested",
            ),
            (
                "task_derivation_current",
                "trace-aware task derivation check was not requested",
            ),
            (
                "checklist_current",
                "trace-aware checklist check was not requested",
            ),
            (
                "selected_gate_current",
                "trace-aware validation gate check was not requested",
            ),
            (
                "review_plan_current",
                "trace-aware review plan check was not requested",
            ),
            (
                "open_findings_current",
                "trace-aware open findings snapshot check was not requested",
            ),
        ];
        items.extend(
            later_items
                .into_iter()
                .map(|(name, details)| ResumeReadyItem {
                    name: name.to_string(),
                    result: "not_checked".to_string(),
                    blocking_action: None,
                    details: details.to_string(),
                }),
        );
    }

    let repo_maturity = maturity == "repo-aware";
    let mut repo_allowed = true;
    let mut repository_snapshot_id = None;
    let mut repository_state_revision = None;
    if repo_maturity {
        let repo_state = repository_resume_state(conn, &target)?;
        repository_snapshot_id = repo_state.latest_current_snapshot_id;
        repository_state_revision = Some(repository_state_revision_for_resume(conn)?);
        let current_repository_heads = snapshot_repository_heads(conn)?;
        let repository_heads_current = snapshot_entries_still_current(
            snapshot.repository_heads.as_deref().unwrap_or(""),
            &current_repository_heads,
        );
        let suspend_repository_snapshot_ids =
            snapshot.repository_snapshot_ids.as_deref().unwrap_or("");
        let current_repository_status = snapshot_repository_status(conn)?;
        let repository_status_current = snapshot_entries_still_current(
            snapshot.repository_status.as_deref().unwrap_or(""),
            &current_repository_status,
        );
        let current_dirty_state_summary = snapshot_dirty_state_summary(conn, target.activation_id)?;
        let dirty_state_summary_current = snapshot_entries_still_current(
            snapshot.dirty_state_summary.as_deref().unwrap_or(""),
            &current_dirty_state_summary,
        );
        let pass = repo_state.repository_count == 0
            || (repo_state.missing_base_snapshot_count == 0
                && repo_state.missing_current_snapshot_count == 0
                && repo_state.missing_comparison_count == 0
                && repo_state.unclassified_comparison_count == 0
                && repo_state.unclassified_dirty_state_count == 0
                && repository_heads_current
                && repository_status_current
                && dirty_state_summary_current);
        if !pass {
            repo_allowed = false;
            blocking_reason.get_or_insert_with(|| "repo-aware resume checks failed".to_string());
        }
        items.push(ResumeReadyItem {
            name: "repository_heads_current".to_string(),
            result: if repository_heads_current {
                "pass"
            } else {
                "fail"
            }
            .to_string(),
            blocking_action: (!repository_heads_current)
                .then_some("record and compare current repository heads".to_string()),
            details: "repository heads match suspend snapshot".to_string(),
        });
        items.push(ResumeReadyItem {
            name: "repository_state_current".to_string(),
            result: if pass { "pass" } else { "fail" }.to_string(),
            blocking_action: (!pass).then_some(
                "record current repository snapshots and classify resume differences".to_string(),
            ),
            details: format!(
                "{} repositories, {} suspend snapshots, {} missing base snapshots, {} missing current snapshots, {} missing comparisons, {} unclassified comparisons, {} unclassified dirty states; suspend snapshot ids={}; status match={}; dirty summary match={}",
                repo_state.repository_count,
                repo_state.base_snapshot_count,
                repo_state.missing_base_snapshot_count,
                repo_state.missing_current_snapshot_count,
                repo_state.missing_comparison_count,
                repo_state.unclassified_comparison_count,
                repo_state.unclassified_dirty_state_count,
                suspend_repository_snapshot_ids,
                repository_status_current,
                dirty_state_summary_current
            ),
        });
    } else {
        items.push(ResumeReadyItem {
            name: "repository_heads_current".to_string(),
            result: "not_checked".to_string(),
            blocking_action: None,
            details: "repo-aware repository head snapshot check was not requested".to_string(),
        });
        items.push(ResumeReadyItem {
            name: "repository_state_current".to_string(),
            result: "not_checked".to_string(),
            blocking_action: None,
            details: "repo-aware repository state check was not requested".to_string(),
        });
    }

    if repo_maturity {
        let invalidated_assumptions = open_assumption_invalidations(conn, target.work_unit_id)?;
        let assumptions_current = snapshot.assumptions.as_deref().unwrap_or("")
            == snapshot_assumptions(conn, target.work_unit_id)?;
        let pass = invalidated_assumptions == 0 && assumptions_current;
        if !pass {
            repo_allowed = false;
            blocking_reason.get_or_insert_with(|| "repo-aware resume checks failed".to_string());
        }
        items.push(ResumeReadyItem {
            name: "assumptions_current".to_string(),
            result: if pass { "pass" } else { "fail" }.to_string(),
            blocking_action: (!pass)
                .then_some("resolve open assumption invalidation dependencies".to_string()),
            details: format!(
                "{invalidated_assumptions} open assumption invalidations; snapshot match={assumptions_current}"
            ),
        });
    } else {
        items.push(ResumeReadyItem {
            name: "assumptions_current".to_string(),
            result: "not_checked".to_string(),
            blocking_action: None,
            details: "repo-aware assumptions check was not requested".to_string(),
        });
    }

    let allowed = basic_allowed && trace_allowed && repo_allowed;
    Ok(ResumeGateEvaluation {
        work_unit_id: target.work_unit_id,
        activation_id: target.activation_id,
        suspend_snapshot_id: snapshot.id,
        resume_result: if allowed { "allowed" } else { "blocked" }.to_string(),
        blocking_reason,
        allowed_next_action: Some(snapshot.next_action),
        authority_high_watermark,
        activation_stack_revision: stack_revision,
        repository_snapshot_id,
        repository_state_revision,
        items,
    })
}

fn resolve_suspended_activation(
    conn: &Connection,
    requested_work_unit_id: Option<i64>,
) -> Result<StoredActivation> {
    let project = project_id(conn)?;
    if let Some(work_unit_id) = requested_work_unit_id {
        let exists = conn.query_row(
            "select exists(select 1 from work_units where id=?1 and project_id=?2)",
            params![work_unit_id, project],
            |row| row.get::<_, bool>(0),
        )?;
        if !exists {
            bail!(
                "resume target unresolved: work unit {work_unit_id} not found; next: agent-workbench status"
            );
        }
    }
    let mut stmt = conn.prepare(
        r#"
        select id,project_id,work_unit_id,stack_depth,status
        from work_unit_activations
        where project_id=?1 and status='suspended'
          and (?2 is null or work_unit_id=?2)
        order by work_unit_id,id
        "#,
    )?;
    let candidates = stmt
        .query_map(params![project, requested_work_unit_id], |row| {
            Ok(StoredActivation {
                activation_id: row.get(0)?,
                project_id: row.get(1)?,
                work_unit_id: row.get(2)?,
                stack_depth: row.get(3)?,
                status: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    match candidates.as_slice() {
        [target] => Ok(StoredActivation {
            activation_id: target.activation_id,
            project_id: target.project_id,
            work_unit_id: target.work_unit_id,
            stack_depth: target.stack_depth,
            status: target.status.clone(),
        }),
        [] => {
            if let Some(work_unit_id) = requested_work_unit_id {
                bail!(
                    "resume target unresolved: work unit {work_unit_id} has no suspended activation; next: agent-workbench status --work {work_unit_id}"
                )
            }
            bail!("no suspended activation to resume")
        }
        _ if requested_work_unit_id.is_some() => {
            let work_unit_id = requested_work_unit_id.expect("checked above");
            bail!(
                "project integrity blocked: work unit {work_unit_id} has multiple suspended activations; next: agent-workbench status --work {work_unit_id}"
            )
        }
        _ => {
            let actions = candidates
                .iter()
                .map(|candidate| {
                    format!(
                        "next: agent-workbench resume-check {} --maturity trace-aware",
                        candidate.work_unit_id
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            bail!(
                "resume target unresolved: {} suspended work owners require an explicit owner\n{actions}",
                candidates.len()
            )
        }
    }
}

pub(super) fn repository_state_revision_for_resume(conn: &Connection) -> Result<i64> {
    Ok([
        max_id(conn, "repository_snapshots")?,
        max_id(conn, "repository_dirty_entries")?,
        max_id(conn, "repository_state_classifications")?,
        max_id(conn, "repository_snapshot_comparisons")?,
    ]
    .into_iter()
    .sum())
}

pub(super) fn repository_resume_state(
    conn: &Connection,
    target: &StoredActivation,
) -> Result<RepositoryResumeState> {
    let repository_count = conn.query_row(
        "select count(*) from repositories where project_id = ?1",
        params![target.project_id],
        |row| row.get::<_, i64>(0),
    )?;
    let mut base_stmt = conn.prepare(
        r#"
        select s.id, s.repository_id
        from repository_snapshots s
        join repositories r on r.id = s.repository_id
        where r.project_id = ?1 and s.work_unit_activation_id = ?2
          and s.id = (
              select max(inner_s.id)
              from repository_snapshots inner_s
              where inner_s.repository_id = s.repository_id
                and inner_s.work_unit_activation_id = ?2
          )
        order by s.id
        "#,
    )?;
    let bases = base_stmt.query_map(params![target.project_id, target.activation_id], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
    })?;
    let mut state = RepositoryResumeState {
        repository_count,
        ..RepositoryResumeState::default()
    };

    for base in bases {
        let (base_snapshot_id, repository_id) = base?;
        state.base_snapshot_count += 1;
        let current = conn
            .query_row(
                r#"
                select id, is_clean
                from repository_snapshots
                where repository_id = ?1 and id > ?2
                order by id desc
                limit 1
                "#,
                params![repository_id, base_snapshot_id],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()?;
        let Some((current_snapshot_id, is_clean)) = current else {
            state.missing_current_snapshot_count += 1;
            continue;
        };
        state.latest_current_snapshot_id = Some(
            state
                .latest_current_snapshot_id
                .map_or(current_snapshot_id, |id| id.max(current_snapshot_id)),
        );
        let comparison = conn
            .query_row(
                r#"
                select result
                from repository_snapshot_comparisons
                where base_repository_snapshot_id = ?1
                  and current_repository_snapshot_id = ?2
                  and comparison_type = 'resume'
                order by id desc
                limit 1
                "#,
                params![base_snapshot_id, current_snapshot_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        match comparison.as_deref() {
            Some("same" | "changed_classified") => {}
            Some("changed_unclassified") => state.unclassified_comparison_count += 1,
            Some(_) => state.unclassified_comparison_count += 1,
            None => state.missing_comparison_count += 1,
        }
        if is_clean == 0 && !repository_snapshot_dirty_state_classified(conn, current_snapshot_id)?
        {
            state.unclassified_dirty_state_count += 1;
        }
    }
    state.missing_base_snapshot_count = repository_count.saturating_sub(state.base_snapshot_count);

    Ok(state)
}

pub(super) fn repository_snapshot_dirty_state_classified(
    conn: &Connection,
    repository_snapshot_id: i64,
) -> Result<bool> {
    let dirty_entry_count = conn.query_row(
        "select count(*) from repository_dirty_entries where repository_snapshot_id = ?1",
        params![repository_snapshot_id],
        |row| row.get::<_, i64>(0),
    )?;
    if dirty_entry_count == 0 {
        return conn
            .query_row(
                r#"
                select 1
                from repository_state_classifications
                where repository_snapshot_id = ?1
                  and dirty_entry_id is null
                  and classification in ('expected', 'unrelated', 'generated', 'accepted_exception')
                limit 1
                "#,
                params![repository_snapshot_id],
                |_| Ok(()),
            )
            .optional()
            .map(|row| row.is_some())
            .map_err(Into::into);
    }

    let unclassified_dirty_entries = conn.query_row(
        r#"
        select count(*)
        from repository_dirty_entries d
        where d.repository_snapshot_id = ?1
          and not exists (
              select 1
              from repository_state_classifications c
              where c.repository_snapshot_id = d.repository_snapshot_id
                and c.dirty_entry_id = d.id
                and c.classification in ('expected', 'unrelated', 'generated', 'accepted_exception')
          )
        "#,
        params![repository_snapshot_id],
        |row| row.get::<_, i64>(0),
    )?;
    Ok(unclassified_dirty_entries == 0)
}

pub(super) fn validation_close_state(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<ValidationCloseState> {
    conn.query_row(
        r#"
        select
            count(*),
            sum(case when latest_result is null then 1 else 0 end),
            sum(case
                when latest_result is not null and latest_result != 'pass' and accepted_failure = 1
                then 1 else 0
            end),
            sum(case
                when latest_result is not null and latest_result != 'pass' and accepted_failure = 0
                then 1 else 0
            end)
        from (
            select
                vg.id,
                (
                    select vr.result
                    from validation_runs vr
                    where vr.validation_gate_id = vg.id
                      and not exists(select 1 from validation_link_retirements retirement where retirement.validation_run_id=vr.id)
                    order by vr.id desc
                    limit 1
                ) as latest_result,
                exists (
                    select 1
                    from validation_runs vr
                    left join acceptance_records run_ar on run_ar.id = vr.acceptance_record_id
                    where vr.validation_gate_id = vg.id
                      and not exists(select 1 from validation_link_retirements retirement where retirement.validation_run_id=vr.id)
                      and (
                        (
                          run_ar.status = 'approved'
                          and run_ar.acceptance_type in ('classified_failure', 'evidence_gap', 'explicit_exception')
                        )
                        or exists (
                          select 1
                          from acceptance_records ar
                          where ar.target_type = 'validation_gate_template'
                            and ar.validation_gate_template_id = vg.template_id
                            and ar.acceptance_type in ('explicit_exception', 'classified_failure', 'evidence_gap')
                            and ar.status = 'approved'
                        )
                      )
                    order by vr.id desc
                    limit 1
                ) as accepted_failure
            from current_task_validation_gates vg
            join design_requirements dr on dr.id = vg.design_requirement_id
            join design_versions dv on dv.id = dr.design_version_id
            join design_packages dp on dp.id = dv.design_package_id
            left join tasks t on t.id = vg.task_id
            where coalesce(vg.work_unit_id, t.work_unit_id) = ?1
              and (vg.task_id is null or t.status != 'accepted_out_of_scope')
              and dp.current_design_version_id = dr.design_version_id
        )
        "#,
        params![work_unit_id],
        |row| {
            Ok(ValidationCloseState {
                selected_gate_count: row.get(0)?,
                missing_run_count: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                accepted_failure_count: row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                unaccepted_failure_count: row.get::<_, Option<i64>>(3)?.unwrap_or(0),
            })
        },
    )
    .map_err(Into::into)
}

pub(super) fn validation_gate_blocker_details_for_work(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        r#"
        select
            gate_id,
            gate_key,
            task_id,
            requirement_key,
            design_version_id,
            latest_result
        from (
            select
                vg.id as gate_id,
                vgt.gate_key as gate_key,
                vg.task_id as task_id,
                dr.requirement_key as requirement_key,
                dr.design_version_id as design_version_id,
                (
                    select vr.result
                    from validation_runs vr
                    where vr.validation_gate_id = vg.id
                      and not exists(select 1 from validation_link_retirements retirement where retirement.validation_run_id=vr.id)
                    order by vr.id desc
                    limit 1
                ) as latest_result,
                exists (
                    select 1
                    from validation_runs vr
                    left join acceptance_records run_ar on run_ar.id = vr.acceptance_record_id
                    where vr.validation_gate_id = vg.id
                      and not exists(select 1 from validation_link_retirements retirement where retirement.validation_run_id=vr.id)
                      and (
                        (
                          run_ar.status = 'approved'
                          and run_ar.acceptance_type in ('classified_failure', 'evidence_gap', 'explicit_exception')
                        )
                        or exists (
                          select 1
                          from acceptance_records ar
                          where ar.target_type = 'validation_gate_template'
                            and ar.validation_gate_template_id = vg.template_id
                            and ar.acceptance_type in ('explicit_exception', 'classified_failure', 'evidence_gap')
                            and ar.status = 'approved'
                        )
                      )
                    order by vr.id desc
                    limit 1
                ) as accepted_failure
            from current_task_validation_gates vg
            join validation_gate_templates vgt on vgt.id = vg.template_id
            join design_requirements dr on dr.id = vg.design_requirement_id
            join design_versions dv on dv.id = dr.design_version_id
            join design_packages dp on dp.id = dv.design_package_id
            left join tasks t on t.id = vg.task_id
            where coalesce(vg.work_unit_id, t.work_unit_id) = ?1
              and (vg.task_id is null or t.status != 'accepted_out_of_scope')
              and dp.current_design_version_id = dr.design_version_id
        )
        where latest_result is null
           or (latest_result != 'pass' and accepted_failure = 0)
        order by gate_id
        "#,
    )?;
    let rows = stmt.query_map(params![work_unit_id], |row| {
        let gate_id: i64 = row.get(0)?;
        let gate_key: String = row.get(1)?;
        let task_id: Option<i64> = row.get(2)?;
        let requirement_key: String = row.get(3)?;
        let design_version_id: i64 = row.get(4)?;
        let latest_result: Option<String> = row.get(5)?;
        let reason = latest_result
            .map(|result| format!("unaccepted_result:{result}"))
            .unwrap_or_else(|| "missing_run".to_string());
        Ok(format!(
            "validation_gate:{gate_id} key:{gate_key} task:{} requirement:{requirement_key} design:{design_version_id} {reason}",
            format_optional_id(task_id)
        ))
    })?;
    collect_rows(rows)
}

pub(super) fn close_process_state(
    conn: &Connection,
    project_id: i64,
    work_unit_id: i64,
) -> Result<CloseProcessState> {
    let work_responsibility: Option<String> = conn
        .query_row(
            "select responsibility from work_units where id = ?1 and project_id = ?2",
            params![work_unit_id, project_id],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    let applicable_rule_count = conn.query_row(
        r#"
        select count(*)
        from rule_bindings
        where project_id = ?1
          and status = 'active'
          and (
              scope_type = 'project'
              or scope_type = 'design_package'
              or work_unit_id = ?2
              or scope_key in ('project', ?3)
              or (?4 is not null and scope_key = ?4)
          )
        "#,
        params![
            project_id,
            work_unit_id,
            work_unit_id.to_string(),
            work_responsibility.as_deref()
        ],
        |row| row.get(0),
    )?;
    let rule_conflict_count = conn.query_row(
        r#"
        select count(*)
        from rule_bindings lower
        where lower.project_id = ?1
          and lower.status = 'active'
          and lower.rule_source_type = 'user_correction'
          and (
              lower.scope_type = 'project'
              or lower.scope_type = 'design_package'
              or lower.work_unit_id = ?2
              or lower.scope_key in ('project', ?3)
              or (?4 is not null and lower.scope_key = ?4)
          )
          and not exists (
              select 1
              from acceptance_records ar
              where ar.project_id = lower.project_id
                and ar.target_type = 'rule_binding'
                and ar.rule_binding_id = lower.id
                and ar.status = 'approved'
                and ar.acceptance_type in ('explicit_exception', 'stale_accepted')
          )
          and exists (
              select 1
              from rule_bindings higher
              where higher.project_id = lower.project_id
                and higher.status = 'active'
                and higher.id != lower.id
                and higher.rule_source_type = lower.rule_source_type
                and (
                    higher.scope_type = 'project'
                    or higher.scope_type = 'design_package'
                    or higher.work_unit_id = ?2
                    or higher.scope_key in ('project', ?3)
                    or (?4 is not null and higher.scope_key = ?4)
                )
                and (
                    higher.scope_key = lower.scope_key
                    or higher.scope_key = 'project'
                    or lower.scope_key = 'project'
                    or higher.work_unit_id = lower.work_unit_id
                    or higher.scope_type = 'work_unit'
                    or lower.scope_type = 'work_unit'
                )
                and (
                    higher.precedence > lower.precedence
                    or (
                        higher.precedence = lower.precedence
                        and case higher.scope_type
                                when 'work_unit' then 4
                                when 'repository' then 3
                                when 'design_package' then 3
                                when 'agent_role' then 3
                                when 'command' then 3
                                when 'review' then 3
                                when 'project' then 1
                                else 2
                            end
                            > case lower.scope_type
                                when 'work_unit' then 4
                                when 'repository' then 3
                                when 'design_package' then 3
                                when 'agent_role' then 3
                                when 'command' then 3
                                when 'review' then 3
                                when 'project' then 1
                                else 2
                            end
                    )
                )
          )
        "#,
        params![
            project_id,
            work_unit_id,
            work_unit_id.to_string(),
            work_responsibility.as_deref()
        ],
        |row| row.get(0),
    )?;
    let fixed_command_count = conn.query_row(
        r#"
        select count(*)
        from command_profiles cp
        where cp.project_id = ?1
          and cp.status = 'fixed'
          and exists (
              select 1
              from rule_bindings rb
              where rb.command_profile_id = cp.id
                and rb.status = 'active'
                and (
                    rb.scope_type = 'project'
                    or rb.work_unit_id = ?2
                    or rb.scope_key in ('project', ?3)
                    or (?4 is not null and rb.scope_key = ?4)
                )
          )
        "#,
        params![
            project_id,
            work_unit_id,
            work_unit_id.to_string(),
            work_responsibility.as_deref()
        ],
        |row| row.get(0),
    )?;
    let missing_fixed_command_usage_count = conn.query_row(
        r#"
        select count(*)
        from command_profiles cp
        where cp.project_id = ?1
          and cp.status = 'fixed'
          and exists (
              select 1
              from rule_bindings rb
              where rb.command_profile_id = cp.id
                and rb.status = 'active'
                and (
                    rb.scope_type = 'project'
                    or rb.work_unit_id = ?2
                    or rb.scope_key in ('project', ?3)
                    or (?4 is not null and rb.scope_key = ?4)
                )
          )
          and not exists (
              select 1
              from command_usages cu
              where cu.command_profile_id = cp.id
                and cu.work_unit_id = ?2
          )
          and not exists (
              select 1
              from command_deviations d
              where d.command_profile_id = cp.id
                and d.work_unit_id = ?2
                and (
                    d.status = 'approved'
                    or exists (
                        select 1
                        from acceptance_records ar
                        where ar.target_type = 'command_deviation'
                          and ar.command_deviation_id = d.id
                          and ar.status = 'approved'
                    )
                )
          )
        "#,
        params![
            project_id,
            work_unit_id,
            work_unit_id.to_string(),
            work_responsibility.as_deref()
        ],
        |row| row.get(0),
    )?;
    let repeated_correction_count = conn.query_row(
        r#"
        select count(*)
        from user_corrections uc
        where uc.project_id = ?1
          and uc.status = 'active'
          and not exists (
              select 1
              from acceptance_records ar
              where ar.project_id = uc.project_id
                and ar.target_type = 'stale_record'
                and ar.stale_record_type = 'user_correction'
                and ar.stale_record_id = uc.id
                and ar.status = 'approved'
                and ar.acceptance_type in ('stale_accepted', 'explicit_exception')
          )
        "#,
        params![project_id],
        |row| row.get(0),
    )?;
    let unsettled_repeated_correction_count = conn.query_row(
        r#"
        select count(*)
        from user_corrections correction
        where correction.project_id=?1 and correction.status='active'
          and not exists (
            select 1 from acceptance_records acceptance
            where acceptance.project_id=correction.project_id
              and acceptance.target_type='stale_record'
              and acceptance.stale_record_type='user_correction'
              and acceptance.stale_record_id=correction.id
              and acceptance.status='approved'
              and acceptance.acceptance_type in ('stale_accepted','explicit_exception')
          )
          and not exists (
            select 1
            from kpt_item_sources source
            join kpt_items item on item.id=source.kpt_item_id
            join kpt_reviews review on review.id=item.kpt_review_id
            where review.project_id=correction.project_id
              and source.source_kind='correction'
              and source.source_identity=cast(correction.id as text)
              and source.source_revision=correction.created_at
              and item.status in ('converted','converted_to_task','dismissed')
          )
        "#,
        params![project_id],
        |row| row.get(0),
    )?;
    let open_kpt_review_count = conn.query_row(
        "select count(*) from kpt_reviews where project_id = ?1 and status = 'open'",
        params![project_id],
        |row| row.get(0),
    )?;
    let unsettled_kpt_item_count = conn.query_row(
        r#"
        select count(*)
        from kpt_items item
        join kpt_reviews review on review.id=item.kpt_review_id
        where review.project_id=?1 and item.status in ('open','accepted')
        "#,
        params![project_id],
        |row| row.get(0),
    )?;
    let work_record_count = conn.query_row(
        "select count(*) from work_records where project_id = ?1 and work_unit_id = ?2",
        params![project_id, work_unit_id],
        |row| row.get(0),
    )?;
    let work_record_evidence_link_count = conn.query_row(
        r#"
        select
            (select count(*) from work_record_commands c join work_records r on r.id = c.work_record_id where r.project_id = ?1 and r.work_unit_id = ?2)
          + (select count(*) from work_record_commits c join work_records r on r.id = c.work_record_id where r.project_id = ?1 and r.work_unit_id = ?2)
          + (select count(*) from work_record_files f join work_records r on r.id = f.work_record_id where r.project_id = ?1 and r.work_unit_id = ?2)
        "#,
        params![project_id, work_unit_id],
        |row| row.get(0),
    )?;
    Ok(CloseProcessState {
        applicable_rule_count,
        rule_conflict_count,
        fixed_command_count,
        missing_fixed_command_usage_count,
        repeated_correction_count,
        unsettled_repeated_correction_count,
        open_kpt_review_count,
        unsettled_kpt_item_count,
        work_record_count,
        work_record_evidence_link_count,
    })
}
