use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{current_phase_blocker, open_existing_project, project_id};
use crate::design::{NewGeneralAcceptance, add_general_acceptance_in};
use crate::rules::{RuleBindingInput, insert_rule_binding};

use super::{evaluation::*, *};

pub fn start_review_scope(root: &Path, input: NewReviewScope<'_>) -> Result<ReviewScopeOutcome> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let agent_role = agent_role_for_review_type(input.review_type)?;
    conn.execute(
        r#"
        insert into review_scopes(
            project_id, name, review_type, agent_role, user_declared_scope,
            allowed_inputs, forbidden_judgments, expected_output_type,
            exclusions, prompt_template_ref, status, no_findings_streak, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 'open', 0, current_timestamp)
        "#,
        params![
            project_id,
            input.name,
            input.review_type,
            agent_role,
            input.scope,
            input.allowed_inputs,
            input.forbidden_judgments,
            input.expected_output_type,
            input.exclusions,
            input.prompt_template_ref,
        ],
    )?;
    Ok(ReviewScopeOutcome {
        review_scope_id: conn.last_insert_rowid(),
    })
}

pub fn list_review_scopes(root: &Path) -> Result<Vec<ReviewScopeRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select id, name, review_type, agent_role, user_declared_scope, status, no_findings_streak
        from review_scopes
        where project_id = ?1
        order by id
        "#,
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        Ok(ReviewScopeRecord {
            id: row.get(0)?,
            name: row.get(1)?,
            review_type: row.get(2)?,
            agent_role: row.get(3)?,
            scope: row.get(4)?,
            status: row.get(5)?,
            no_findings_streak: row.get(6)?,
        })
    })?;
    collect_rows(rows)
}

pub fn add_review_policy(root: &Path, input: NewReviewPolicy<'_>) -> Result<ReviewPolicyOutcome> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    conn.execute(
        r#"
        insert into review_policies(
            project_id, name, review_type, max_fresh_agents, max_resume_agents,
            max_parallel_agents, required_consecutive_clean_fresh_runs,
            required_consecutive_clean_resume_runs, stop_on_severity,
            allow_resume_review, allow_fresh_review, allow_new_findings_in_resume,
            on_max_agents_exceeded, run_count_scope, default_run_mode, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, current_timestamp)
        "#,
        params![
            project_id,
            input.name,
            input.review_type,
            input.max_fresh_agents,
            input.max_resume_agents,
            input.max_parallel_agents,
            input.required_consecutive_clean_fresh_runs,
            input.required_consecutive_clean_resume_runs,
            input.stop_on_severity,
            bool_to_i64(input.allow_resume_review),
            bool_to_i64(input.allow_fresh_review),
            bool_to_i64(input.allow_new_findings_in_resume),
            input.on_max_agents_exceeded,
            input.run_count_scope,
            input.default_run_mode,
        ],
    )?;
    Ok(ReviewPolicyOutcome {
        review_policy_id: conn.last_insert_rowid(),
    })
}

pub fn list_review_policies(root: &Path) -> Result<Vec<ReviewPolicyRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select
            id, name, review_type, max_fresh_agents, max_resume_agents,
            max_parallel_agents, required_consecutive_clean_fresh_runs,
            required_consecutive_clean_resume_runs, stop_on_severity,
            allow_resume_review, allow_fresh_review, allow_new_findings_in_resume,
            on_max_agents_exceeded, run_count_scope, default_run_mode
        from review_policies
        where project_id = ?1
        order by id
        "#,
    )?;
    let rows = stmt.query_map(params![project_id], review_policy_record)?;
    collect_rows(rows)
}

pub fn add_review_plan(root: &Path, input: NewReviewPlan<'_>) -> Result<ReviewPlanOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    tx.query_row(
        "select id from work_units where id = ?1 and project_id = ?2",
        params![input.work_unit_id, project_id],
        |row| row.get::<_, i64>(0),
    )
    .optional()?
    .context("work unit not found")?;
    let review_policy_id = match input.review_policy_id {
        Some(id) => Some(id),
        None => Some(get_or_create_default_policy(
            &tx,
            project_id,
            input.review_type,
        )?),
    };
    validate_review_plan_references(
        &tx,
        project_id,
        input.review_type,
        review_policy_id,
        input.review_scope_id,
    )?;
    tx.execute(
        r#"
        insert into review_plans(
            project_id, work_unit_id, design_version_id, review_type, required,
            stage, scope, clean_condition, stop_condition, review_policy_id,
            review_scope_id, status, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'open', current_timestamp)
        "#,
        params![
            project_id,
            input.work_unit_id,
            input.design_version_id,
            input.review_type,
            bool_to_i64(input.required),
            input.stage,
            input.scope,
            input.clean_condition,
            input.stop_condition,
            review_policy_id,
            input.review_scope_id,
        ],
    )?;
    let review_plan_id = tx.last_insert_rowid();
    let work_scope = input.work_unit_id.to_string();
    insert_rule_binding(
        &tx,
        RuleBindingInput {
            project_id,
            rule_source_type: "review_policy",
            authority_event_id: None,
            user_correction_id: None,
            command_profile_id: None,
            review_policy_id,
            review_plan_id: Some(review_plan_id),
            work_unit_id: Some(input.work_unit_id),
            validation_gate_id: None,
            acceptance_record_id: None,
            scope_type: "work_unit",
            scope_key: Some(&work_scope),
            precedence: 65,
        },
    )?;
    tx.execute(
        r#"
        insert into review_plan_targets(review_plan_id, target_type, work_unit_id)
        values (?1, 'work_unit', ?2)
        "#,
        params![review_plan_id, input.work_unit_id],
    )?;
    if let Some(design_version_id) = input.design_version_id {
        tx.execute(
            r#"
            insert into review_plan_targets(review_plan_id, target_type, design_version_id)
            values (?1, 'design_version', ?2)
            "#,
            params![review_plan_id, design_version_id],
        )?;
    }
    tx.commit()?;
    Ok(ReviewPlanOutcome {
        review_plan_id,
        review_policy_id,
    })
}

pub fn list_review_plans(root: &Path) -> Result<Vec<ReviewPlanRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select
            id, work_unit_id, design_version_id, review_type, required,
            stage, scope, review_policy_id, review_scope_id, status
        from review_plans
        where project_id = ?1
        order by id
        "#,
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        Ok(ReviewPlanRecord {
            id: row.get(0)?,
            work_unit_id: row.get(1)?,
            design_version_id: row.get(2)?,
            review_type: row.get(3)?,
            required: row.get::<_, i64>(4)? == 1,
            stage: row.get(5)?,
            scope: row.get(6)?,
            review_policy_id: row.get(7)?,
            review_scope_id: row.get(8)?,
            status: row.get(9)?,
        })
    })?;
    collect_rows(rows)
}

pub fn list_review_plan_targets(
    root: &Path,
    review_plan_id: i64,
) -> Result<Vec<ReviewPlanTargetRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select *
        from (
            select
                t.id, t.review_plan_id, t.target_type, t.design_version_id,
                t.design_requirement_id, t.task_id, t.work_unit_id,
                null as phase_id, t.repository_snapshot_id, t.file_path, t.symbol
            from review_plan_targets t
            join review_plans p on p.id = t.review_plan_id
            where t.review_plan_id = ?1 and p.project_id = ?2
            union all
            select
                pt.id, pt.review_plan_id, 'phase', null,
                null, null, null, pt.phase_id, null, null, null
            from work_phase_review_targets pt
            join review_plans p on p.id = pt.review_plan_id
            where pt.review_plan_id = ?1 and p.project_id = ?2
        )
        order by id
        "#,
    )?;
    let rows = stmt.query_map(params![review_plan_id, project_id], |row| {
        Ok(ReviewPlanTargetRecord {
            id: row.get(0)?,
            review_plan_id: row.get(1)?,
            target_type: row.get(2)?,
            design_version_id: row.get(3)?,
            design_requirement_id: row.get(4)?,
            task_id: row.get(5)?,
            work_unit_id: row.get(6)?,
            phase_id: row.get(7)?,
            repository_snapshot_id: row.get(8)?,
            file_path: row.get(9)?,
            symbol: row.get(10)?,
        })
    })?;
    collect_rows(rows)
}

pub fn waive_review_plan(
    root: &Path,
    input: ReviewPlanWaiver<'_>,
) -> Result<ReviewPlanWaiverOutcome> {
    if input.review_plan_id <= 0 {
        bail!("review plan id must be positive");
    }
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let blocker = current_phase_blocker(&tx)?;
    let mut resolver_next = None;
    let selected = if let Some(blocker) = blocker.as_ref() {
        blocker.review_plan_id == Some(input.review_plan_id)
            && blocker
                .next_action
                .contains(&format!("review plan waive {}", input.review_plan_id))
    } else {
        let selected_plan: Option<i64> = tx
            .query_row(
                r#"
                select p.id from review_plans p
                join work_unit_activations active on active.work_unit_id=p.work_unit_id
                  and active.status in ('active','resumed')
                where p.project_id=?1 and p.required=1 and p.status!='clean'
                  and (
                    p.design_version_id is null
                    or exists (
                      select 1
                      from task_derivations td
                      join design_requirements r on r.id=td.design_requirement_id
                      join design_versions v on v.id=r.design_version_id
                      join design_packages package on package.id=v.design_package_id
                      join tasks t on t.id=td.task_id
                      where r.design_version_id=p.design_version_id
                        and t.work_unit_id=p.work_unit_id
                        and td.status in ('active','stale')
                        and package.current_design_version_id=r.design_version_id
                    )
                  )
                  and not exists (
                    select 1 from acceptance_records ar
                    where ar.target_type='review_plan' and ar.review_plan_id=p.id
                      and ar.status='approved'
                      and ar.acceptance_type in ('explicit_exception','stale_accepted')
                  )
                order by case p.stage when 'design-ready' then 0 when 'implementation-ready' then 1 when 'close-ready' then 2 else 3 end, p.id
                limit 1
                "#,
                params![project_id],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(review_plan_id) = selected_plan {
            resolver_next = Some(format!("review plan waive {review_plan_id}"));
        }
        selected_plan == Some(input.review_plan_id)
    };
    if !selected {
        let next = blocker
            .as_ref()
            .map(|blocker| blocker.next_action.clone())
            .or(resolver_next)
            .unwrap_or_else(|| "complete the resolver-selected lifecycle action".to_string());
        bail!("review plan waive is not selected; next: {next}");
    }
    let target = format!("review-plan:{}", input.review_plan_id);
    let outcome = add_general_acceptance_in(
        &tx,
        project_id,
        NewGeneralAcceptance {
            target: &target,
            acceptance_type: "explicit_exception",
            reason: input.reason,
            approval_authority_event_id: input.approval_authority_event_id,
        },
    )?;
    tx.commit()?;
    Ok(ReviewPlanWaiverOutcome {
        review_plan_id: input.review_plan_id,
        acceptance_record_id: outcome.acceptance_record_id,
        authority_event_id: outcome.authority_event_id,
    })
}

pub fn add_review_plan_target(
    root: &Path,
    input: NewReviewPlanTarget<'_>,
) -> Result<ReviewPlanTargetOutcome> {
    validate_review_target_shape(&input)?;
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    conn.query_row(
        "select 1 from review_plans where id = ?1 and project_id = ?2",
        params![input.review_plan_id, project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("review plan not found")?;
    if input.target_type == "phase" {
        let phase_id = input.phase_id.context("phase target requires phase id")?;
        let plan_work_unit_id: i64 = conn.query_row(
            "select work_unit_id from review_plans where id = ?1 and project_id = ?2",
            params![input.review_plan_id, project_id],
            |row| row.get(0),
        )?;
        conn.query_row(
            "select 1 from work_phases where id = ?1 and project_id = ?2 and work_unit_id = ?3",
            params![phase_id, project_id, plan_work_unit_id],
            |_| Ok(()),
        )
        .optional()?
        .context("phase target not found for review plan work unit")?;
        conn.execute(
            r#"
            insert into work_phase_review_targets(
                project_id, review_plan_id, phase_id, created_at
            )
            values (?1, ?2, ?3, current_timestamp)
            "#,
            params![project_id, input.review_plan_id, phase_id],
        )?;
        return Ok(ReviewPlanTargetOutcome {
            review_plan_target_id: conn.last_insert_rowid(),
        });
    }
    conn.execute(
        r#"
        insert into review_plan_targets(
            review_plan_id, target_type, design_version_id, design_requirement_id,
            task_id, work_unit_id, repository_snapshot_id, file_path, symbol
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
        params![
            input.review_plan_id,
            input.target_type,
            input.design_version_id,
            input.design_requirement_id,
            input.task_id,
            input.work_unit_id,
            input.repository_snapshot_id,
            input.file_path,
            input.symbol,
        ],
    )?;
    Ok(ReviewPlanTargetOutcome {
        review_plan_target_id: conn.last_insert_rowid(),
    })
}

pub fn add_review_run(root: &Path, input: NewReviewRun<'_>) -> Result<ReviewRunOutcome> {
    add_review_run_with_finding_result(root, input, None)
}

pub fn add_review_run_with_finding_result(
    root: &Path,
    input: NewReviewRun<'_>,
    finding_fix_result: Option<&str>,
) -> Result<ReviewRunOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let plan = load_review_plan(&tx, project_id, input.review_plan_id)?;
    let policy = load_review_policy(&tx, project_id, plan.review_policy_id)?;
    validate_run_type_purpose(input.run_type, input.run_purpose)?;
    validate_review_run_result(&input)?;
    validate_finding_fix_run(&tx, project_id, &plan, &input, finding_fix_result)?;
    let target = resolve_run_target(&tx, project_id, &plan, input.target_ref)?;
    validate_gate_context_target(&plan, &input, &target)?;
    enforce_run_allowed(&tx, &policy, &plan, input.run_type, input.target_ref)?;
    if input.run_type == "resume"
        && input.new_findings_count > 0
        && !policy.allow_new_findings_in_resume
    {
        bail!("new findings are disabled for resume review by policy");
    }

    tx.execute(
        r#"
        insert into review_runs(
            project_id, review_scope_id, review_plan_id, run_type, run_purpose,
            target_type, design_version_id, design_requirement_id, task_id,
            work_unit_id, phase_id, repository_snapshot_id, file_path, symbol, target_ref,
            prompt_deviations, result_summary, new_findings_count,
            carried_findings_checked, clean_run, status, review_provenance,
            review_provenance_ref, finding_fix_result, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, current_timestamp)
        "#,
        params![
            project_id,
            plan.review_scope_id,
            plan.id,
            input.run_type,
            input.run_purpose,
            target.target_type,
            target.design_version_id,
            target.design_requirement_id,
            target.task_id,
            target.work_unit_id,
            target.phase_id,
            target.repository_snapshot_id,
            target.file_path,
            target.symbol,
            target.target_ref,
            input.prompt_deviations,
            input.result_summary,
            input.new_findings_count,
            input.carried_findings_checked,
            bool_to_i64(input.clean_run),
            input.status,
            input.review_provenance,
            input.review_provenance_ref,
            finding_fix_result,
        ],
    )?;
    let review_run_id = tx.last_insert_rowid();
    let invocation_status = match input.status {
        "completed" => "completed",
        "failed" => "failed",
        "cancelled" => "cancelled",
        "running" => "running",
        _ => "requested",
    };
    tx.execute(
        r#"
        insert into review_agent_invocations(
            project_id, review_plan_id, review_run_id, run_type, agent_label,
            external_agent_id, status, started_at, finished_at
        )
        values (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7,
            case when ?7 in ('running', 'completed', 'failed', 'cancelled') then current_timestamp else null end,
            case when ?7 in ('completed', 'failed', 'cancelled') then current_timestamp else null end
        )
        "#,
        params![
            project_id,
            plan.id,
            review_run_id,
            input.run_type,
            input.agent_label,
            input.external_agent_id,
            invocation_status,
        ],
    )?;
    let plan_status = evaluate_plan_status(&tx, project_id, plan.id, &policy)?;
    tx.commit()?;

    Ok(ReviewRunOutcome {
        review_run_id,
        review_plan_id: plan.id,
        plan_status,
    })
}

pub(super) fn validate_finding_fix_run(
    conn: &rusqlite::Connection,
    project_id: i64,
    plan: &StoredReviewPlan,
    input: &NewReviewRun<'_>,
    finding_fix_result: Option<&str>,
) -> Result<()> {
    let is_finding_fix =
        input.run_type == "resume" && input.run_purpose == "finding_fix_verification";
    if !is_finding_fix {
        if finding_fix_result.is_some() {
            bail!("--finding-result is only valid for resume finding_fix_verification runs");
        }
        return Ok(());
    }
    let result = finding_fix_result.context(
        "resume finding_fix_verification run requires --finding-result verified|not_fixed|needs_evidence",
    )?;
    if !matches!(result, "verified" | "not_fixed" | "needs_evidence") {
        bail!("invalid finding-fix result");
    }
    if input.new_findings_count != 0 || input.carried_findings_checked != 1 {
        bail!("finding-fix resume run requires zero new findings and exactly one carried finding");
    }
    if (result == "verified") != input.clean_run {
        bail!("finding-fix result and clean flag are inconsistent");
    }
    if input.status != "completed" {
        bail!("finding-fix resume run must be completed");
    }
    let trusted = match input.review_provenance {
        "external_agent" => {
            input
                .external_agent_id
                .is_some_and(|value| !value.trim().is_empty())
                && input
                    .review_provenance_ref
                    .is_some_and(|value| !value.trim().is_empty())
        }
        "human_review" => input
            .review_provenance_ref
            .is_some_and(|value| !value.trim().is_empty()),
        _ => false,
    };
    if !trusted {
        bail!("finding-fix resume run requires trusted external-agent or human provenance");
    }
    let target = input
        .target_ref
        .context("finding-fix resume run requires exact context target")?;
    let existing_outcome: Option<String> = conn.query_row(
        "select finding_fix_result from review_runs where review_plan_id = ?1 and run_type = 'resume' and run_purpose = 'finding_fix_verification' and target_ref = ?2 order by id desc limit 1",
        params![plan.id, target],
        |row| row.get(0),
    ).optional()?;
    if existing_outcome
        .as_deref()
        .is_some_and(|value| value != result)
    {
        bail!(
            "finding-fix attempt already has a conflicting resume outcome; all outcomes for one attempt must use the same typed result"
        );
    }
    conn.query_row(
        r#"
        select 1
        from closure_attempts a
        join closures c on c.id = a.closure_id
        join findings f on f.id = c.finding_id
        join review_runs source on source.id = f.review_run_id
        where a.project_id = ?1
          and source.review_plan_id = ?2
          and c.status = 'ready_for_verification'
          and a.result is null
          and ?3 = 'review-context:finding-fix:finding=' || f.id
                    || ':closure=' || c.id || ':attempt=' || a.id
          and coalesce((select max(id) from review_runs), 0) >= a.review_run_high_watermark
        "#,
        params![project_id, plan.id, target],
        |_| Ok(()),
    )
    .optional()?
    .context("finding-fix resume run target is not the current ready attempt")?;
    Ok(())
}

pub fn list_review_runs(root: &Path, review_plan_id: Option<i64>) -> Result<Vec<ReviewRunRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select
            id, review_plan_id, run_type, run_purpose, target_type, target_ref,
            new_findings_count, carried_findings_checked, clean_run, status,
            review_provenance, review_provenance_ref, finding_fix_result
        from review_runs
        where project_id = ?1 and (?2 is null or review_plan_id = ?2)
        order by id
        "#,
    )?;
    let rows = stmt.query_map(params![project_id, review_plan_id], |row| {
        Ok(ReviewRunRecord {
            id: row.get(0)?,
            review_plan_id: row.get(1)?,
            run_type: row.get(2)?,
            run_purpose: row.get(3)?,
            target_type: row.get(4)?,
            target_ref: row.get(5)?,
            new_findings_count: row.get(6)?,
            carried_findings_checked: row.get(7)?,
            clean_run: row.get::<_, i64>(8)? == 1,
            status: row.get(9)?,
            review_provenance: row.get(10)?,
            review_provenance_ref: row.get(11)?,
            finding_fix_result: row.get(12)?,
        })
    })?;
    collect_rows(rows)
}
