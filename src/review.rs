use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};
use sha2::{Digest, Sha256};

use crate::db::{current_phase_blocker, open_existing_project, project_id};
use crate::design::{NewGeneralAcceptance, add_general_acceptance_in};
use crate::review_context::{review_context_ref, review_context_ref_with_phase};
use crate::rules::{RuleBindingInput, insert_rule_binding};

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
                join work_unit_activations active on active.work_unit_id=p.work_unit_id and active.status='active'
                where p.project_id=?1 and p.required=1 and p.status!='clean'
                  and not exists (select 1 from acceptance_records ar where ar.target_type='review_plan' and ar.review_plan_id=p.id and ar.status='approved')
                  and not exists (select 1 from correction_sessions s where s.project_id=p.project_id and s.status='active')
                  and not exists (select 1 from finding_remediation_bindings b join work_unit_activations a on a.id=b.work_unit_activation_id and a.status='active' where b.project_id=p.project_id)
                order by case p.stage when 'design-ready' then 0 when 'implementation-ready' then 1 when 'close-ready' then 2 else 3 end, p.id
                limit 1
                "#,
                params![project_id],
                |row| row.get(0),
            )
            .optional()?;
        selected_plan == Some(input.review_plan_id)
    };
    if !selected {
        let next = blocker
            .as_ref()
            .map(|blocker| blocker.next_action.as_str())
            .unwrap_or("complete the resolver-selected lifecycle action");
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
    let review_agent_invocation_id = tx.last_insert_rowid();
    let plan_status = evaluate_plan_status(&tx, project_id, plan.id, &policy)?;
    tx.commit()?;

    Ok(ReviewRunOutcome {
        review_run_id,
        review_agent_invocation_id,
        review_plan_id: plan.id,
        plan_status,
    })
}

fn validate_finding_fix_run(
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

pub fn add_finding(root: &Path, input: NewFinding<'_>) -> Result<FindingOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let run = tx
        .query_row(
            r#"
            select r.run_type, p.review_policy_id, p.review_type, r.clean_run
            from review_runs r
            join review_plans p on p.id = r.review_plan_id
            where r.id = ?1 and r.project_id = ?2
            "#,
            params![input.review_run_id, project_id],
            |row| {
                Ok(StoredReviewRunPolicy {
                    run_type: row.get(0)?,
                    review_policy_id: row.get(1)?,
                    review_type: row.get(2)?,
                    clean_run: row.get::<_, i64>(3)? == 1,
                })
            },
        )
        .optional()?
        .context("review run not found")?;
    ensure_finding_type_matches_review_type(input.finding_type, &run.review_type)?;
    if run.clean_run {
        bail!("cannot add finding to a clean review run");
    }
    let policy = load_review_policy(&tx, project_id, run.review_policy_id)?;
    if run.run_type == "resume" && !policy.allow_new_findings_in_resume {
        bail!("new findings are disabled for resume review by policy");
    }
    tx.query_row(
        "select id from review_runs where id = ?1 and project_id = ?2",
        params![input.review_run_id, project_id],
        |row| row.get::<_, i64>(0),
    )
    .optional()?
    .context("review run not found")?;
    tx.execute(
        r#"
        insert into findings(
            project_id, review_run_id, finding_type, severity, description,
            classification, status, design_requirement_id, task_id, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, 'unclassified', 'open', ?6, ?7, current_timestamp)
        "#,
        params![
            project_id,
            input.review_run_id,
            input.finding_type,
            input.severity,
            input.description,
            input.design_requirement_id,
            input.task_id,
        ],
    )?;
    let finding_id = tx.last_insert_rowid();
    refresh_plan_for_run(&tx, project_id, input.review_run_id)?;
    tx.commit()?;
    Ok(FindingOutcome { finding_id })
}

pub fn classify_finding(
    root: &Path,
    finding_id: i64,
    classification: &str,
) -> Result<FindingClassificationOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let current_status: String = tx
        .query_row(
            "select status from findings where id = ?1 and project_id = ?2",
            params![finding_id, project_id],
            |row| row.get(0),
        )
        .optional()?
        .context("finding not found")?;
    ensure_review_finding_target(&tx, finding_id, "finding classify")?;
    if let Some(blocker) = current_phase_blocker(&tx)? {
        let expected = format!("agent-workbench finding classify {finding_id}");
        if !blocker.next_action.starts_with(&expected) {
            bail!(
                "finding classify is not selected; next: {}",
                blocker.next_action
            );
        }
    }
    if matches!(current_status.as_str(), "closed" | "accepted_out_of_scope") {
        bail!("terminal finding cannot be reclassified without an explicit authority transition");
    }
    let current_classification: String = tx.query_row(
        "select classification from findings where id = ?1 and project_id = ?2",
        params![finding_id, project_id],
        |row| row.get(0),
    )?;
    if current_classification == classification {
        return Ok(FindingClassificationOutcome { finding_id });
    }
    if current_classification == "valid" && classification != "valid" {
        bail!(
            "a valid finding cannot be reclassified to bypass closure, remediation, and verification"
        );
    }
    let status = match classification {
        "invalid" => "closed",
        "valid" => "open",
        "design_conflict" | "needs_evidence" | "unclassified" => "open",
        _ => bail!("invalid finding classification"),
    };
    let changed = tx.execute(
        r#"
        update findings
        set classification = ?1, status = ?2
        where id = ?3 and project_id = ?4
        "#,
        params![classification, status, finding_id, project_id],
    )?;
    if changed == 0 {
        bail!("finding not found");
    }
    let review_run_id: i64 = tx.query_row(
        "select review_run_id from findings where id = ?1",
        params![finding_id],
        |row| row.get(0),
    )?;
    refresh_plan_for_run(&tx, project_id, review_run_id)?;
    tx.commit()?;
    Ok(FindingClassificationOutcome { finding_id })
}

pub fn add_closure(root: &Path, input: NewClosure<'_>) -> Result<ClosureOutcome> {
    require_text(Some(input.design_invariant), "closure requires --invariant")?;
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let finding = tx
        .query_row(
            "select id, classification, status from findings where id = ?1 and project_id = ?2",
            params![input.finding_id, project_id],
            |row| {
                Ok(StoredFinding {
                    id: row.get(0)?,
                    classification: row.get(1)?,
                    status: row.get(2)?,
                })
            },
        )
        .optional()?
        .context("finding not found")?;
    ensure_review_finding_target(&tx, finding.id, "closure add")?;
    if let Some(blocker) = current_phase_blocker(&tx)? {
        let expected = format!("agent-workbench closure add --finding {}", finding.id);
        if !blocker.next_action.starts_with(&expected) {
            bail!("closure add is not selected; next: {}", blocker.next_action);
        }
    }
    if finding.classification != "valid" {
        bail!("closure requires a valid finding");
    }
    if finding.status != "open" {
        bail!("finding is not open");
    }
    let current_exists: bool = tx.query_row(
        "select exists(select 1 from closures where finding_id = ?1 and status != 'superseded')",
        params![finding.id],
        |row| row.get(0),
    )?;
    if current_exists {
        bail!(
            "finding already has a current closure; use closure supersede when the contract must change"
        );
    }
    let eligible = finding_is_remediation_eligible(&tx, project_id, finding.id)?;
    if eligible {
        require_text(
            input.affected_surfaces,
            "eligible closure requires --surfaces",
        )?;
        require_text(input.fix_plan, "eligible closure requires --fix-plan")?;
        require_text(input.tests_or_gates, "eligible closure requires --tests")?;
        require_text(
            input.verification_plan,
            "eligible closure requires --verification",
        )?;
    } else {
        require_text(
            input.affected_surfaces,
            "source correction closure requires --surfaces",
        )?;
        require_text(
            input.fix_plan,
            "source correction closure requires --fix-plan",
        )?;
        require_text(
            input.tests_or_gates,
            "source correction closure requires --tests",
        )?;
        require_text(
            input.verification_plan,
            "source correction closure requires --verification",
        )?;
        parse_correction_tokens(input.affected_surfaces.unwrap())?;
    }
    tx.execute(
        r#"
        insert into closures(
            project_id, finding_id, design_invariant, design_citations,
            implementation_evidence, affected_surfaces, same_invariant_search,
            other_violations_found, fix_plan, tests_or_gates,
            verification_plan, closed_by_commit, status, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'registered', current_timestamp)
        "#,
        params![
            project_id,
            finding.id,
            input.design_invariant,
            input.design_citations,
            input.implementation_evidence,
            input.affected_surfaces,
            input.same_invariant_search,
            input.other_violations_found,
            input.fix_plan,
            input.tests_or_gates,
            input.verification_plan,
            input.closed_by_commit,
        ],
    )?;
    let closure_id = tx.last_insert_rowid();
    if !eligible {
        let design_root = correction_design_root(&tx, finding.id)?;
        record_correction_tokens(
            &tx,
            root,
            project_id,
            closure_id,
            input.affected_surfaces.unwrap(),
            design_root.as_deref(),
        )?;
    }
    tx.commit()?;
    Ok(ClosureOutcome { closure_id })
}

pub fn begin_correction(root: &Path, closure_id: i64) -> Result<CorrectionBeginOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let selected_active_closure: Option<i64> = tx
        .query_row(
            "select closure_id from correction_sessions where project_id = ?1 and status = 'active' order by id limit 1",
            params![project_id],
            |row| row.get(0),
        )
        .optional()?;
    if selected_active_closure.is_some_and(|selected| selected != closure_id) {
        bail!(
            "another source correction session is selected; finish closure {} first",
            selected_active_closure.unwrap()
        );
    }
    if let Some(blocker) = current_phase_blocker(&tx)? {
        let expected = format!("agent-workbench closure correction-begin {closure_id}");
        if blocker.next_action != expected {
            bail!(
                "closure correction-begin is not the selected action; next: {}",
                blocker.next_action
            );
        }
    }
    let (finding_id, surfaces, eligible, design_root): (i64, String, bool, Option<String>) = tx
        .query_row(
            r#"
            select c.finding_id, c.affected_surfaces,
                   p.required = 1 and p.stage = 'close-ready'
                     and p.review_type in ('implementation_review', 'design_implementation_diff'),
                   dp.root_path
            from closures c
            join findings f on f.id = c.finding_id
            join review_runs r on r.id = f.review_run_id
            join review_plans p on p.id = r.review_plan_id
            left join design_versions dv on dv.id = p.design_version_id
            left join design_packages dp on dp.id = dv.design_package_id
            where c.id = ?1 and c.project_id = ?2 and c.status = 'registered'
              and f.status = 'open' and f.classification = 'valid'
            "#,
            params![closure_id, project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?
        .context("registered correction closure not found")?;
    if eligible {
        bail!("implementation findings use agent-workbench work remediate");
    }
    if let Some(session_id) = tx
        .query_row(
            "select id from correction_sessions where closure_id = ?1 and status = 'active'",
            params![closure_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
    {
        let token_count = tx.query_row(
            "select count(*) from correction_tokens where closure_id = ?1",
            params![closure_id],
            |row| row.get(0),
        )?;
        return Ok(CorrectionBeginOutcome {
            closure_id,
            session_id,
            token_count,
            idempotent: true,
        });
    }
    let mut token_count: i64 = tx.query_row(
        "select count(*) from correction_tokens where closure_id = ?1",
        params![closure_id],
        |row| row.get(0),
    )?;
    if token_count == 0 {
        token_count = record_correction_tokens(
            &tx,
            root,
            project_id,
            closure_id,
            &surfaces,
            design_root.as_deref(),
        )?;
    } else {
        ensure_correction_prestate_unchanged(&tx, root, closure_id, design_root.as_deref())?;
    }
    validate_correction_transition_preflight(&tx, project_id, closure_id, finding_id)?;
    tx.execute(
        r#"
        insert into correction_sessions(project_id, finding_id, closure_id, status, created_at)
        values (?1, ?2, ?3, 'active', current_timestamp)
        "#,
        params![project_id, finding_id, closure_id],
    )?;
    let session_id = tx.last_insert_rowid();
    tx.commit()?;
    Ok(CorrectionBeginOutcome {
        closure_id,
        session_id,
        token_count,
        idempotent: false,
    })
}

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
        "task-accept-out-of-scope" => {
            let (task_id, design_requirement_id) = resolve_task_ref(
                &tx,
                session_id,
                token_ordinal,
                work_unit_id,
                design_version_id,
                &target,
            )?;
            let outcome = crate::planning::accept_task_out_of_scope_in(
                &tx,
                project_id,
                task_id,
                design_requirement_id,
                &reason,
                authority_event_id.unwrap(),
            )?;
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

fn transition_state_snapshot(conn: &rusqlite::Connection, work_unit_id: i64) -> Result<String> {
    let state: (String, String, String, String, String, String, String, String, String, String) = conn.query_row(
        r#"
        select
          coalesce((select group_concat(v,'|') from (select id||':'||status v from tasks where work_unit_id=?1 order by id)),''),
          coalesce((select group_concat(v,'|') from (select m.id||':'||m.phase_id||':'||m.task_id v from work_phase_task_memberships m join work_phases p on p.id=m.phase_id where p.work_unit_id=?1 order by m.id)),''),
          coalesce((select group_concat(v,'|') from (select id||':'||status v from checklists where work_unit_id=?1 order by id)),''),
          coalesce((select group_concat(v,'|') from (select td.id||':'||td.status v from task_derivations td join tasks t on t.id=td.task_id where t.work_unit_id=?1 order by td.id)),''),
          coalesce((select group_concat(v,'|') from (select ci.id||':'||ci.status v from checklist_items ci join tasks t on t.id=ci.task_id where t.work_unit_id=?1 order by ci.id)),''),
          coalesce((select group_concat(v,'|') from (select id||':'||status v from validation_gates where work_unit_id=?1 order by id)),''),
          coalesce((select group_concat(v,'|') from (select id||':'||status v from coverage_items where work_unit_id=?1 order by id)),''),
          coalesce((select group_concat(v,'|') from (select id||':'||status v from work_phases where work_unit_id=?1 order by id)),''),
          coalesce((select group_concat(v,'|') from (select d.id||':'||d.status||':'||coalesce(d.evidence_ref,'')||':'||coalesce(d.authority_event_id,0) v from work_phase_dependencies d join work_phases p on p.id=d.from_phase_id where p.work_unit_id=?1 order by d.id)),''),
          coalesce((select group_concat(v,'|') from (select ar.id||':'||ar.target_type||':'||coalesce(ar.task_id,ar.checklist_item_id,ar.validation_gate_id,ar.coverage_item_id,ar.stale_record_id,0)||':'||ar.status||':'||coalesce(ar.approved_by_authority_event_id,0) v from acceptance_records ar left join tasks t on t.id=ar.task_id left join checklist_items ci on ci.id=ar.checklist_item_id left join validation_gates vg on vg.id=ar.validation_gate_id left join coverage_items c on c.id=ar.coverage_item_id where coalesce(t.work_unit_id,vg.work_unit_id,c.work_unit_id,(select work_unit_id from checklists where id=ci.checklist_id),?1)=?1 order by ar.id)),'')
        "#,
        params![work_unit_id],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?,row.get(9)?)),
    )?;
    Ok(format!(
        "tasks=[{}];memberships=[{}];checklists=[{}];derivations=[{}];items=[{}];gates=[{}];coverage=[{}];phases=[{}];phase_dependencies=[{}];acceptances=[{}]",
        state.0, state.1, state.2, state.3, state.4, state.5, state.6, state.7, state.8, state.9
    ))
}

fn ensure_mediated_decomposition_coverage(
    conn: &rusqlite::Connection,
    project_id: i64,
    work_unit_id: i64,
    design_version_id: i64,
) -> Result<()> {
    conn.execute(
        r#"
        insert into coverage_items(
            project_id, work_unit_id, design_requirement_id, task_id,
            requirement, lifecycle_boundary_evidence, tests_or_gates,
            missing_or_unverified, status, created_at
        )
        select
            ?1, ?2, r.id, t.id, r.requirement_text,
            'generated by mediated decomposition; implementation evidence pending',
            'selected validation gates pending',
            'implementation and validation evidence required',
            'needs_evidence', current_timestamp
        from task_derivations td
        join tasks t on t.id = td.task_id
        join design_requirements r on r.id = td.design_requirement_id
        where t.work_unit_id = ?2 and r.design_version_id = ?3
          and not exists (
              select 1 from coverage_items c
              where c.task_id = t.id and c.design_requirement_id = r.id
          )
        "#,
        params![project_id, work_unit_id, design_version_id],
    )?;
    Ok(())
}

fn record_correction_transition_aliases(
    conn: &rusqlite::Connection,
    project_id: i64,
    session_id: i64,
    application_id: i64,
    operation: &str,
    target: &str,
    result_ref: &str,
) -> Result<()> {
    let mut aliases = Vec::<(String, String, i64)>::new();
    match operation {
        "design-decompose" => {
            let checklist_id = result_ref
                .strip_prefix("checklist:")
                .context("invalid decomposition application result")?
                .parse::<i64>()?;
            aliases.push((
                "@checklist".to_string(),
                "checklist".to_string(),
                checklist_id,
            ));
            let mut stmt = conn.prepare(
                r#"
                select r.requirement_key, ci.id, ci.task_id, td.id, c.id
                from checklist_items ci
                join design_requirements r on r.id = ci.design_requirement_id
                join task_derivations td on td.checklist_item_id = ci.id
                join coverage_items c on c.task_id = ci.task_id and c.design_requirement_id = r.id
                where ci.checklist_id = ?1
                order by r.requirement_key
                "#,
            )?;
            let rows = stmt.query_map(params![checklist_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })?;
            for row in rows {
                let (key, item_id, task_id, derivation_id, coverage_id) = row?;
                aliases.push((format!("@task/{key}"), "task".to_string(), task_id));
                aliases.push((
                    format!("@derivation/{key}"),
                    "task_derivation".to_string(),
                    derivation_id,
                ));
                aliases.push((
                    format!("@checklist-item/{key}"),
                    "checklist_item".to_string(),
                    item_id,
                ));
                aliases.push((
                    format!("@coverage/{key}"),
                    "coverage_item".to_string(),
                    coverage_id,
                ));
                let mut gates = conn.prepare(
                    r#"
                    select vg.id, vg.gate_key
                    from validation_gates vg
                    join checklist_items ci
                      on ci.task_id = vg.task_id
                     and ci.design_requirement_id = vg.design_requirement_id
                    where ci.id = ?1
                    order by vg.id
                    "#,
                )?;
                let gate_rows = gates.query_map(params![item_id], |gate| {
                    Ok((gate.get::<_, i64>(0)?, gate.get::<_, String>(1)?))
                })?;
                for gate in gate_rows {
                    let (gate_id, gate_key) = gate?;
                    aliases.push((
                        format!("@gate/{key}/{gate_key}"),
                        "validation_gate".to_string(),
                        gate_id,
                    ));
                }
            }
        }
        "phase-create" => {
            let phase_id = result_ref
                .strip_prefix("phase:")
                .context("invalid phase application result")?
                .parse::<i64>()?;
            let alias = target
                .split('/')
                .nth(2)
                .context("phase-create target has no alias")?;
            aliases.push((alias.to_string(), "phase".to_string(), phase_id));
        }
        "task-accept-out-of-scope" => {
            let parts = result_ref.split(':').collect::<Vec<_>>();
            let task_id = parts
                .get(1)
                .context("invalid task acceptance application result")?
                .parse::<i64>()?;
            aliases.push((
                format!("@accepted-task/{task_id}"),
                "task".to_string(),
                task_id,
            ));
            let mut stmt = conn.prepare(
                r#"
                select 'checklist_item', ci.id from checklist_items ci where ci.task_id=?1
                union all select 'validation_gate', vg.id from validation_gates vg where vg.task_id=?1
                union all select 'coverage_item', c.id from coverage_items c where c.task_id=?1
                "#,
            )?;
            let rows = stmt.query_map(params![task_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (record_type, record_id) = row?;
                aliases.push((
                    format!("@accepted-{record_type}/{record_id}"),
                    record_type,
                    record_id,
                ));
            }
        }
        "phase-dependency-add" => {
            let dependency_id = result_ref
                .strip_prefix("phase-dependency:")
                .context("invalid dependency application result")?
                .parse::<i64>()?;
            aliases.push((
                format!("@dependency/{dependency_id}"),
                "phase_dependency".to_string(),
                dependency_id,
            ));
        }
        _ => {}
    }
    for (alias, record_type, record_id) in aliases {
        conn.execute(
            r#"
            insert into correction_transition_aliases(
                project_id, correction_session_id, correction_application_id,
                alias, record_type, record_id, created_at
            ) values (?1, ?2, ?3, ?4, ?5, ?6, current_timestamp)
            "#,
            params![
                project_id,
                session_id,
                application_id,
                alias,
                record_type,
                record_id
            ],
        )?;
    }
    Ok(())
}

fn parse_pair(target: &str) -> Result<(i64, i64)> {
    let (left, right) = target.split_once('/').context("target requires two ids")?;
    Ok((left.parse()?, right.parse()?))
}

fn resolve_task_ref(
    conn: &rusqlite::Connection,
    session_id: i64,
    token_ordinal: i64,
    work_unit_id: i64,
    design_version_id: Option<i64>,
    value: &str,
) -> Result<(i64, i64)> {
    let numeric_id = value.parse::<i64>().ok();
    let key = value.strip_prefix("@task/");
    if numeric_id.is_none() && key.is_none() {
        bail!("invalid task reference");
    }
    if let Some(task_id) = numeric_id {
        return conn
            .query_row(
                r#"
                select t.id, r.id from tasks t
                join task_derivations td on td.task_id = t.id and td.status = 'active'
                join design_requirements r on r.id = td.design_requirement_id
                where t.id = ?1 and t.work_unit_id = ?2 and r.design_version_id = ?3
                "#,
                params![task_id, work_unit_id, design_version_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .context("task id is outside the correction owner or current design");
    }
    conn.query_row(
        r#"
        select alias.record_id, r.id
        from correction_transition_aliases alias
        join correction_transition_applications app on app.id = alias.correction_application_id
        join correction_tokens token on token.id = app.correction_token_id
        join tasks t on t.id = alias.record_id
        join task_derivations td on td.task_id = t.id and td.status = 'active'
        join design_requirements r on r.id = td.design_requirement_id
        where app.correction_session_id = ?1 and token.token_ordinal < ?2
          and alias.record_type = 'task'
          and (?3 is null or alias.record_id = ?3)
          and (?4 is null or alias.alias = '@task/' || ?4)
          and t.work_unit_id = ?5 and r.design_version_id = ?6
          and (?4 is null or r.requirement_key = ?4)
        order by alias.id desc, r.id limit 1
        "#,
        params![
            session_id,
            token_ordinal,
            numeric_id,
            key,
            work_unit_id,
            design_version_id
        ],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()?
    .context("task reference was not created or adopted by an earlier correction token")
}

fn resolve_phase_ref(
    conn: &rusqlite::Connection,
    session_id: i64,
    token_ordinal: i64,
    work_unit_id: i64,
    value: &str,
) -> Result<i64> {
    let numeric_id = value.parse::<i64>().ok();
    let key = value.starts_with('@').then_some(value);
    if numeric_id.is_none() && key.is_none() {
        bail!("invalid phase reference");
    }
    if let Some(phase_id) = numeric_id {
        return conn
            .query_row(
                "select id from work_phases where id=?1 and work_unit_id=?2 and status in ('open','blocked')",
                params![phase_id, work_unit_id],
                |row| row.get(0),
            )
            .optional()?
            .context("numeric phase reference is outside the open correction owner");
    }
    conn.query_row(
        r#"
        select alias.record_id
        from correction_transition_aliases alias
        join correction_transition_applications app on app.id = alias.correction_application_id
        join correction_tokens token on token.id = app.correction_token_id
        where app.correction_session_id = ?1 and token.token_ordinal < ?2
          and alias.record_type = 'phase'
          and alias.alias = ?3
          and exists (
              select 1 from work_phases p
              where p.id = alias.record_id and p.work_unit_id = ?4
          )
        order by alias.id desc limit 1
        "#,
        params![session_id, token_ordinal, key, work_unit_id],
        |row| row.get(0),
    )
    .optional()?
    .context("phase reference was not created by an earlier correction token")
}

fn ensure_phase_dependency_owner(
    conn: &rusqlite::Connection,
    dependency_id: i64,
    work_unit_id: i64,
) -> Result<()> {
    conn.query_row(
        r#"
        select 1
        from work_phase_dependencies d
        join work_phases source on source.id = d.from_phase_id
        join work_phases target on target.id = d.to_phase_id
        where d.id = ?1 and d.status = 'open'
          and source.work_unit_id = ?2 and target.work_unit_id = ?2
        "#,
        params![dependency_id, work_unit_id],
        |_| Ok(()),
    )
    .optional()?
    .context("open phase dependency is outside the correction work unit")
}

fn ensure_phase_dependency_authority_scope(
    conn: &rusqlite::Connection,
    project_id: i64,
    authority_event_id: i64,
    dependency_id: i64,
    work_unit_id: i64,
) -> Result<()> {
    let scope: String = conn
        .query_row(
            "select scope from authority_events where id = ?1 and project_id = ?2 and status = 'active'",
            params![authority_event_id, project_id],
            |row| row.get(0),
        )
        .optional()?
        .context("active same-project authority event not found")?;
    if !matches!(scope.as_str(), "project")
        && scope != format!("phase-dependency:{dependency_id}")
        && scope != format!("work-unit:{work_unit_id}")
    {
        bail!("authority scope does not cover the exact phase dependency or owning work unit");
    }
    Ok(())
}

fn parse_correction_tokens(surfaces: &str) -> Result<Vec<CorrectionToken>> {
    let mut parsed = Vec::new();
    let mut phase_aliases = Vec::<String>::new();
    let mut has_decomposition = false;
    let mut transition_effects = HashSet::<String>::new();
    for raw in surfaces.split(',') {
        let token = raw.trim();
        if token.is_empty() {
            bail!("correction surfaces contain an empty token");
        }
        if let Some(rest) = token.strip_prefix("transition:") {
            let (verb, target) = rest
                .split_once(':')
                .context("transition token requires transition:<verb>:<target>")?;
            if !matches!(
                verb,
                "design-decompose"
                    | "task-accept-out-of-scope"
                    | "phase-create"
                    | "phase-assign"
                    | "phase-dependency-add"
                    | "phase-dependency-satisfy"
                    | "phase-dependency-accept"
                    | "stale-accept"
                    | "stale-close"
            ) || target.trim().is_empty()
            {
                bail!("unsupported correction transition token: {token}");
            }
            validate_correction_transition_target(verb, target, has_decomposition, &phase_aliases)?;
            let effect_key = if verb == "phase-create" {
                let parts = target.split('/').collect::<Vec<_>>();
                format!(
                    "phase-create:{}/{}/{}/{}/{}",
                    parts[0], parts[1], parts[3], parts[4], parts[5]
                )
            } else {
                format!("{verb}:{target}")
            };
            if !transition_effects.insert(effect_key) {
                bail!("duplicate correction transition effect is not allowed");
            }
            if verb == "design-decompose" {
                has_decomposition = true;
            }
            if verb == "phase-create" {
                phase_aliases.push(target.split('/').nth(2).unwrap().to_string());
            }
            parsed.push(CorrectionToken {
                kind: "transition".to_string(),
                operation: verb.to_string(),
                target: target.to_string(),
            });
            continue;
        }
        let mut parts = token.splitn(3, ':');
        let kind = parts.next().unwrap_or_default();
        let operation = parts.next().unwrap_or_default();
        let target = parts.next().unwrap_or_default();
        if !matches!(kind, "design" | "plan" | "docs" | "workflow")
            || !matches!(operation, "edit" | "create" | "delete")
            || target.is_empty()
            || !target.ends_with(".md")
            || target.starts_with('/')
            || target.contains('\\')
            || target
                .split('/')
                .any(|part| part.is_empty() || matches!(part, "." | ".."))
            || !target
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '/'))
        {
            bail!("invalid typed correction surface: {token}");
        }
        match kind {
            "plan" if !target.starts_with("plans/") => bail!("plan surface must be below plans/"),
            "docs" if target != "README.md" && !target.starts_with("docs/") => {
                bail!("docs surface must be README.md or below docs/")
            }
            "workflow"
                if !target.starts_with(".agents/skills/agent-workbench/")
                    && !target.starts_with("skills/agent-workbench/") =>
            {
                bail!("workflow surface must be inside the Agent Workbench skill")
            }
            _ => {}
        }
        parsed.push(CorrectionToken {
            kind: "file".to_string(),
            operation: operation.to_string(),
            target: format!("{kind}:{target}"),
        });
    }
    Ok(parsed)
}

fn validate_correction_transition_target(
    verb: &str,
    target: &str,
    has_decomposition: bool,
    phase_aliases: &[String],
) -> Result<()> {
    let positive = |value: &str| -> Result<i64> {
        let parsed = value.parse::<i64>()?;
        if parsed <= 0 || value != parsed.to_string() {
            bail!("transition ids and order must be positive")
        }
        Ok(parsed)
    };
    let valid_phase_ref = |value: &str| {
        phase_aliases.iter().any(|alias| alias == value)
            || value.parse::<i64>().is_ok_and(|id| id > 0)
    };
    match verb {
        "design-decompose" => {
            let parts = target.split('/').collect::<Vec<_>>();
            if parts.len() != 2 {
                bail!("design-decompose target requires design/work")
            }
            positive(parts[0])?;
            positive(parts[1])?;
        }
        "task-accept-out-of-scope" => {
            if target.starts_with("@task/") {
                let key = target.trim_start_matches("@task/");
                if !has_decomposition
                    || key.is_empty()
                    || !key
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
                {
                    bail!("task alias requires an earlier design-decompose token")
                }
            } else {
                positive(target)?;
            }
        }
        "phase-create" => {
            let parts = target.split('/').collect::<Vec<_>>();
            if parts.len() != 6 {
                bail!("phase-create target requires work/design/alias/kind/order/key")
            }
            positive(parts[0])?;
            positive(parts[1])?;
            positive(parts[4])?;
            let alias_key = parts[2].strip_prefix('@').unwrap_or_default();
            if alias_key.is_empty()
                || !alias_key.chars().all(|ch| {
                    ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '-' | '_')
                })
                || parts[3].trim().is_empty()
                || parts[5].is_empty()
                || !parts[5].chars().all(|ch| {
                    ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '-' | '_')
                })
            {
                bail!("phase-create alias, kind, or key is invalid")
            }
            if phase_aliases.iter().any(|alias| alias == parts[2]) {
                bail!("phase-create alias is duplicated")
            }
        }
        "phase-assign" => {
            let (phase, task) = target
                .split_once('/')
                .context("phase-assign target requires phase/task")?;
            if !valid_phase_ref(phase) {
                bail!("phase assignment requires an earlier same-closure phase alias")
            }
            if task.starts_with("@task/") {
                let key = task.trim_start_matches("@task/");
                if !has_decomposition
                    || key.is_empty()
                    || !key
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
                {
                    bail!("task alias requires an earlier design-decompose token")
                }
            } else {
                positive(task)?;
            }
        }
        "phase-dependency-add" => {
            let parts = target.split('/').collect::<Vec<_>>();
            if parts.len() != 3
                || !valid_phase_ref(parts[0])
                || !valid_phase_ref(parts[1])
                || !matches!(parts[2], "blocks" | "requires")
            {
                bail!("phase dependency requires earlier phase aliases and blocks|requires")
            }
        }
        "phase-dependency-satisfy" | "phase-dependency-accept" => {
            positive(target)?;
        }
        "stale-accept" | "stale-close" => {
            let (kind, id) = target
                .split_once('/')
                .context("stale target requires type/id")?;
            if !matches!(
                kind,
                "task_derivation"
                    | "checklist"
                    | "validation_gate"
                    | "coverage_item"
                    | "review_plan"
            ) {
                bail!("invalid stale record type")
            }
            positive(id)?;
        }
        _ => bail!("unsupported correction transition {verb}"),
    }
    Ok(())
}

pub(crate) fn validate_correction_surfaces(surfaces: &str) -> Result<()> {
    let tokens = parse_correction_tokens(surfaces)?;
    if tokens.is_empty() {
        bail!("correction contract has no typed surfaces");
    }
    Ok(())
}

fn correction_design_root(conn: &rusqlite::Connection, finding_id: i64) -> Result<Option<String>> {
    conn.query_row(
        r#"
        select dp.root_path
        from findings f
        join review_runs r on r.id = f.review_run_id
        join review_plans p on p.id = r.review_plan_id
        left join design_versions dv on dv.id = p.design_version_id
        left join design_packages dp on dp.id = dv.design_package_id
        where f.id = ?1
        "#,
        params![finding_id],
        |row| row.get(0),
    )
    .optional()
    .map(|value| value.flatten())
    .map_err(Into::into)
}

fn stale_contract_tuple(
    conn: &rusqlite::Connection,
    project_id: i64,
    kind: &str,
    record_id: i64,
) -> Result<(i64, i64, i64, i64)> {
    let (rank, design_id, work_id) = match kind {
        "task_derivation" => conn.query_row(
            r#"select 0, r.design_version_id, coalesce(t.work_unit_id,0)
               from task_derivations td join design_requirements r on r.id=td.design_requirement_id
               join tasks t on t.id=td.task_id where td.id=?1 and td.project_id=?2"#,
            params![record_id, project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ),
        "checklist" => conn.query_row(
            "select 1, design_version_id, work_unit_id from checklists where id=?1 and project_id=?2",
            params![record_id, project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ),
        "validation_gate" => conn.query_row(
            r#"select 2, coalesce(r.design_version_id,0), coalesce(vg.work_unit_id,t.work_unit_id,0)
               from validation_gates vg left join design_requirements r on r.id=vg.design_requirement_id
               left join tasks t on t.id=vg.task_id where vg.id=?1 and vg.project_id=?2"#,
            params![record_id, project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ),
        "coverage_item" => conn.query_row(
            r#"select 3, r.design_version_id, coalesce(c.work_unit_id,t.work_unit_id,0)
               from coverage_items c join design_requirements r on r.id=c.design_requirement_id
               left join tasks t on t.id=c.task_id where c.id=?1 and c.project_id=?2"#,
            params![record_id, project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ),
        "review_plan" => conn.query_row(
            "select 4, coalesce(design_version_id,0), work_unit_id from review_plans where id=?1 and project_id=?2",
            params![record_id, project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ),
        _ => bail!("invalid stale record type"),
    }
    .optional()?
    .context("declared stale record does not exist in this project")?;
    Ok((rank, design_id, work_id, record_id))
}

fn validate_declared_stale_order(
    conn: &rusqlite::Connection,
    project_id: i64,
    tokens: &[CorrectionToken],
) -> Result<()> {
    let mut previous = None;
    for token in tokens.iter().filter(|token| {
        token.kind == "transition"
            && matches!(token.operation.as_str(), "stale-accept" | "stale-close")
    }) {
        let (kind, record_id) = token
            .target
            .split_once('/')
            .context("stale target requires type/id")?;
        let tuple = stale_contract_tuple(conn, project_id, kind, record_id.parse()?)?;
        if previous.is_some_and(|prior| tuple <= prior) {
            bail!("declared stale transition tokens must be in ascending global tuple order");
        }
        previous = Some(tuple);
    }
    Ok(())
}

fn validate_correction_transition_preflight(
    conn: &rusqlite::Connection,
    project_id: i64,
    closure_id: i64,
    finding_id: i64,
) -> Result<()> {
    let (work_unit_id, design_version_id): (i64, Option<i64>) = conn.query_row(
        r#"select p.work_unit_id, p.design_version_id
           from findings f join review_runs r on r.id=f.review_run_id
           join review_plans p on p.id=r.review_plan_id where f.id=?1"#,
        params![finding_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let mut stmt = conn.prepare(
        "select operation, target from correction_tokens where closure_id=?1 and token_kind='transition' order by token_ordinal",
    )?;
    let rows = stmt.query_map(params![closure_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let transitions = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    for (operation, target) in transitions {
        match operation.as_str() {
            "design-decompose" => {
                let (design, work) = parse_pair(&target)?;
                if work != work_unit_id || design_version_id != Some(design) {
                    bail!("design-decompose target is outside the correction owner or design");
                }
                crate::traceability::validate_design_decomposition_in(
                    conn, project_id, design, work,
                )?;
            }
            "task-accept-out-of-scope" if !target.starts_with("@task/") => {
                let task_id = target.parse::<i64>()?;
                conn.query_row(
                    r#"select 1 from tasks t
                       where t.id=?1 and t.work_unit_id=?2 and t.status in ('open','blocked')
                         and (?3 is null or exists(
                           select 1 from task_derivations td join design_requirements r on r.id=td.design_requirement_id
                           where td.task_id=t.id and td.status='active' and r.design_version_id=?3
                         ))"#,
                    params![task_id, work_unit_id, design_version_id],
                    |_| Ok(()),
                )
                .optional()?
                .context("task transition target is outside the open correction owner/design")?;
            }
            "phase-create" => {
                let parts = target.split('/').collect::<Vec<_>>();
                if parts.len() != 6 {
                    bail!("phase-create target requires work/design/alias/kind/order/key");
                }
                let work = parts[0].parse::<i64>()?;
                let design = parts[1].parse::<i64>()?;
                if work != work_unit_id || design_version_id != Some(design) {
                    bail!("phase-create target is outside the correction owner or design");
                }
            }
            "phase-assign" => {
                let (phase, task) = target
                    .split_once('/')
                    .context("phase-assign target requires phase/task")?;
                if !phase.starts_with('@') {
                    resolve_phase_ref(conn, 0, 0, work_unit_id, phase)?;
                }
                if !task.starts_with("@task/") {
                    let task_id = task.parse::<i64>()?;
                    conn.query_row(
                        "select 1 from tasks where id=?1 and work_unit_id=?2 and status in ('open','blocked')",
                        params![task_id, work_unit_id],
                        |_| Ok(()),
                    )
                    .optional()?
                    .context("phase assignment task is outside the correction owner")?;
                }
            }
            "phase-dependency-add" => {
                let parts = target.split('/').collect::<Vec<_>>();
                if parts.len() != 3 {
                    bail!("phase-dependency-add target requires from/to/type");
                }
                for phase in &parts[..2] {
                    if !phase.starts_with('@') {
                        resolve_phase_ref(conn, 0, 0, work_unit_id, phase)?;
                    }
                }
            }
            "phase-dependency-satisfy" | "phase-dependency-accept" => {
                ensure_phase_dependency_owner(conn, target.parse()?, work_unit_id)?;
            }
            "stale-accept" | "stale-close" => {
                let (kind, id) = target
                    .split_once('/')
                    .context("stale target requires type/id")?;
                stale_contract_tuple(conn, project_id, kind, id.parse()?)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn record_correction_tokens(
    conn: &rusqlite::Connection,
    root: &Path,
    project_id: i64,
    closure_id: i64,
    surfaces: &str,
    design_root: Option<&str>,
) -> Result<i64> {
    let tokens = parse_correction_tokens(surfaces)?;
    if tokens.is_empty() {
        bail!("correction contract has no typed surfaces");
    }
    validate_declared_stale_order(conn, project_id, &tokens)?;
    for (index, token) in tokens.iter().enumerate() {
        let (pre_state, pre_hash) = match token.kind.as_str() {
            "file" => {
                let path = correction_file_path(root, design_root, token)?;
                let exists = path.is_file();
                match token.operation.as_str() {
                    "edit" | "delete" if !exists => bail!(
                        "{} requires an existing regular file: {}",
                        token.operation,
                        path.display()
                    ),
                    "create" if exists => {
                        bail!("create requires an absent target: {}", path.display())
                    }
                    _ => {}
                }
                (
                    Some(if exists { "file" } else { "absent" }.to_string()),
                    exists.then(|| file_sha256(&path)).transpose()?,
                )
            }
            "transition" => (transition_pre_state(conn, &token.operation)?, None),
            _ => unreachable!(),
        };
        conn.execute(
            r#"
            insert into correction_tokens(
                project_id, closure_id, token_ordinal, token_kind, operation,
                target, pre_state, pre_hash, status, created_at
            ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending', current_timestamp)
            "#,
            params![
                project_id,
                closure_id,
                index as i64 + 1,
                token.kind,
                token.operation,
                token.target,
                pre_state,
                pre_hash
            ],
        )?;
    }
    Ok(tokens.len() as i64)
}

fn transition_pre_state(conn: &rusqlite::Connection, operation: &str) -> Result<Option<String>> {
    let table = match operation {
        "design-decompose" => Some(("checklist_max", "checklists")),
        "phase-create" => Some(("phase_max", "work_phases")),
        "phase-dependency-add" => Some(("phase_dependency_max", "work_phase_dependencies")),
        _ => None,
    };
    table
        .map(|(label, table)| {
            let max_id: i64 = conn.query_row(
                &format!("select coalesce(max(id),0) from {table}"),
                [],
                |row| row.get(0),
            )?;
            Ok(format!("{label}:{max_id}"))
        })
        .transpose()
}

fn ensure_correction_prestate_unchanged(
    conn: &rusqlite::Connection,
    root: &Path,
    closure_id: i64,
    design_root: Option<&str>,
) -> Result<()> {
    let mut stmt = conn.prepare(
        "select token_kind, operation, target, pre_state, pre_hash from correction_tokens where closure_id = ?1 order by token_ordinal",
    )?;
    let rows = stmt.query_map(params![closure_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;
    let stored = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    for (token_kind, operation, target, pre_state, pre_hash) in stored {
        if token_kind == "transition" {
            let current = transition_pre_state(conn, &operation)?;
            if current != pre_state {
                bail!(
                    "correction transition pre-state changed after closure registration; supersede the closure before correction-begin: {operation}:{target}"
                );
            }
            continue;
        }
        let token = CorrectionToken {
            kind: token_kind,
            operation,
            target,
        };
        let path = correction_file_path(root, design_root, &token)?;
        let state = if path.is_file() { "file" } else { "absent" };
        let hash = path.is_file().then(|| file_sha256(&path)).transpose()?;
        if pre_state.as_deref() != Some(state) || pre_hash != hash {
            bail!(
                "correction source changed after closure registration; supersede the closure before correction-begin: {}",
                path.display()
            );
        }
    }
    Ok(())
}

fn correction_file_path(
    root: &Path,
    design_root: Option<&str>,
    token: &CorrectionToken,
) -> Result<PathBuf> {
    let (kind, target) = token
        .target
        .split_once(':')
        .context("invalid stored file token")?;
    let base = match kind {
        "design" | "plan" => root.join(design_root.context(
            "design and plan correction surfaces require an exact imported design package",
        )?),
        "docs" | "workflow" => root.to_path_buf(),
        _ => bail!("invalid stored correction file kind"),
    };
    let canonical_base = base
        .canonicalize()
        .with_context(|| format!("correction surface root does not exist: {}", base.display()))?;
    let path = base.join(target);
    let containment = if path.exists() {
        path.canonicalize()?
    } else {
        let mut parent = path.parent().context("correction target has no parent")?;
        while !parent.exists() {
            parent = parent
                .parent()
                .context("correction target has no existing parent")?;
        }
        parent.canonicalize()?
    };
    if !containment.starts_with(&canonical_base) {
        bail!("correction surface escapes its allowed root");
    }
    Ok(path)
}

fn file_sha256(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

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
        if !selected_ready {
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
            "#,
            params![finding_id, project_id],
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
            where b.work_unit_id = ?1 and b.project_id = ?2
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
            "select operation, target, pre_hash from correction_tokens where closure_id = ?1 and token_kind = 'file' order by token_ordinal",
        )?;
        let rows = stmt.query_map(params![input.closure_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;
        let file_tokens = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);
        for (operation, target, pre_hash) in file_tokens {
            let token = CorrectionToken {
                kind: "file".to_string(),
                operation: operation.clone(),
                target,
            };
            let path = correction_file_path(root, design_root.as_deref(), &token)?;
            match operation.as_str() {
                "create" if !path.is_file() => {
                    bail!(
                        "created correction surface is still absent: {}",
                        path.display()
                    )
                }
                "delete" if path.exists() => {
                    bail!(
                        "deleted correction surface still exists: {}",
                        path.display()
                    )
                }
                "edit" if !path.is_file() => {
                    bail!(
                        "edited correction surface is not a regular file: {}",
                        path.display()
                    )
                }
                "edit" if Some(file_sha256(&path)?) == pre_hash => {
                    bail!("edited correction surface is unchanged: {}", path.display())
                }
                _ => {}
            }
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
            "#,
            params![input.closure_id, project_id],
            |row| row.get(0),
        )
        .optional()?
        .context("current registered or incomplete closure not found")?;
    ensure_review_finding_target(&tx, finding_id, "closure supersede")?;
    if let Some(blocker) = current_phase_blocker(&tx)? {
        let selected_contract_action = blocker
            .next_action
            .starts_with("agent-workbench closure supersede")
            || blocker
                .next_action
                .starts_with("agent-workbench work remediate")
            || blocker
                .next_action
                .starts_with("agent-workbench closure correction-begin");
        if !selected_contract_action {
            bail!(
                "closure supersede is not selected; next: {}",
                blocker.next_action
            );
        }
    }
    let eligible = finding_is_remediation_eligible(&tx, project_id, finding_id)?;
    if !eligible {
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
    if !eligible {
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

fn ensure_active_acceptance_authority(
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

fn ensure_review_finding_target(
    conn: &rusqlite::Connection,
    finding_id: i64,
    operation: &str,
) -> Result<()> {
    if let Some(blocker) = current_phase_blocker(conn)? {
        let same_selected_finding =
            blocker.kind == "required_review_finding" && blocker.finding_id == Some(finding_id);
        if !same_selected_finding {
            bail!(
                "{operation} is not allowed under the selected resolver action; next: {}",
                blocker.next_action
            );
        }
        return Ok(());
    }
    let selected_active_finding: Option<i64> = conn.query_row(
        r#"
        with active_scopes(finding_id) as (
          select b.finding_id
          from finding_remediation_bindings b
          join findings f on f.id = b.finding_id and f.status = 'open' and f.classification = 'valid'
          join closures c on c.id = b.closure_id and c.status = 'registered'
          join work_unit_activations a on a.id = b.work_unit_activation_id and a.status = 'active'
          union
          select s.finding_id
          from correction_sessions s
          join findings f on f.id = s.finding_id and f.status = 'open' and f.classification = 'valid'
          join closures c on c.id = s.closure_id and c.status = 'registered'
          where s.status = 'active'
        )
        select min(finding_id) from active_scopes
        "#,
        [],
        |row| row.get(0),
    )?;
    if selected_active_finding.is_some_and(|selected| selected != finding_id) {
        bail!(
            "{operation} targets finding {finding_id}, but active scoped finding {selected_active_finding:?} is selected"
        );
    }
    Ok(())
}

fn finding_is_remediation_eligible(
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
        )
        "#,
        params![finding_id, project_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn require_text(value: Option<&str>, message: &str) -> Result<()> {
    if value.is_none_or(|value| value.trim().is_empty()) {
        bail!(message.to_string());
    }
    Ok(())
}

pub fn add_finding_verification(
    root: &Path,
    input: NewFindingVerification<'_>,
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
        where c.id = ?2
          and c.finding_id = ?3
          and c.project_id = ?5
          and f.project_id = ?5
          and f.id = ?3
          and source_run.review_plan_id = verifier.review_plan_id
          and c.status = 'ready_for_verification'
          and verifier.id > a.review_run_high_watermark
          and verifier.target_ref = 'review-context:finding-fix:finding=' || f.id
                    || ':closure=' || c.id || ':attempt=' || a.id
        "#,
            params![
                input.review_run_id,
                input.closure_id,
                input.finding_id,
                input.result,
                project_id,
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
    tx.execute(
        "update closure_attempts set result = ?1, resolved_at = current_timestamp where id = ?2",
        params![input.result, attempt_id],
    )?;
    if input.result == "verified" {
        tx.execute(
            "update findings set status = 'closed' where id = ?1 and project_id = ?2",
            params![input.finding_id, project_id],
        )?;
        tx.execute(
            "update closures set status = 'verified' where id = ?1",
            params![input.closure_id],
        )?;
    } else {
        tx.execute(
            "update closures set status = 'registered' where id = ?1",
            params![input.closure_id],
        )?;
        tx.execute(
            "update correction_sessions set status = 'active', completed_at = null where id = (select max(id) from correction_sessions where closure_id = ?1 and status = 'completed')",
            params![input.closure_id],
        )?;
    }
    let fresh_watermark: i64 =
        tx.query_row("select coalesce(max(id), 0) from review_runs", [], |row| {
            row.get(0)
        })?;
    tx.execute(
        "update review_plans set fresh_review_after_run_id = ?1 where id = (select review_plan_id from review_runs where id = ?2)",
        params![fresh_watermark, input.review_run_id],
    )?;
    refresh_plan_for_run(&tx, project_id, input.review_run_id)?;
    tx.commit()?;
    Ok(FindingVerificationOutcome {
        finding_verification_id,
    })
}

pub fn list_findings(root: &Path, status: Option<&str>) -> Result<Vec<FindingRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select id, review_run_id, finding_type, severity, description, classification, status
        from findings
        where project_id = ?1 and (?2 is null or status = ?2)
        order by id
        "#,
    )?;
    let rows = stmt.query_map(params![project_id, status], |row| {
        Ok(FindingRecord {
            id: row.get(0)?,
            review_run_id: row.get(1)?,
            finding_type: row.get(2)?,
            severity: row.get(3)?,
            description: row.get(4)?,
            classification: row.get(5)?,
            status: row.get(6)?,
        })
    })?;
    collect_rows(rows)
}

fn refresh_plan_for_run(
    conn: &rusqlite::Connection,
    project_id: i64,
    review_run_id: i64,
) -> Result<()> {
    let plan_id: i64 = conn.query_row(
        "select review_plan_id from review_runs where id = ?1 and project_id = ?2",
        params![review_run_id, project_id],
        |row| row.get(0),
    )?;
    let review_policy_id: i64 = conn.query_row(
        "select review_policy_id from review_plans where id = ?1 and project_id = ?2",
        params![plan_id, project_id],
        |row| row.get(0),
    )?;
    let policy = load_review_policy(conn, project_id, review_policy_id)?;
    evaluate_plan_status(conn, project_id, plan_id, &policy)?;
    Ok(())
}

fn evaluate_plan_status(
    conn: &rusqlite::Connection,
    project_id: i64,
    review_plan_id: i64,
    policy: &StoredReviewPolicy,
) -> Result<String> {
    let severity_filter = severity_block_filter(&policy.stop_on_severity)?;
    let open_blocking_findings: i64 = conn.query_row(
        r#"
        select count(*)
        from findings f
        join review_runs r on r.id = f.review_run_id
        where r.review_plan_id = ?1
          and f.project_id = ?2
          and f.status = 'open'
          and f.classification in ('unclassified', 'valid', 'design_conflict', 'needs_evidence')
          and (
              ?3 = 0
              or case f.severity
                    when 'critical' then 4
                    when 'high' then 3
                    when 'medium' then 2
                    when 'low' then 1
                 end >= ?3
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
        "#,
        params![review_plan_id, project_id, severity_filter],
        |row| row.get(0),
    )?;
    let plan = load_review_plan(conn, project_id, review_plan_id)?;
    let clean_fresh = consecutive_clean_runs(conn, &plan, "fresh")?;
    let clean_resume = consecutive_clean_runs(conn, &plan, "resume")?;
    let status = if open_blocking_findings > 0 {
        "blocked"
    } else if clean_fresh >= policy.required_consecutive_clean_fresh_runs
        && clean_resume >= policy.required_consecutive_clean_resume_runs
    {
        "clean"
    } else {
        "open"
    };
    conn.execute(
        "update review_plans set status = ?1 where id = ?2 and project_id = ?3",
        params![status, review_plan_id, project_id],
    )?;
    if status == "clean" {
        conn.execute(
            r#"
            update review_scopes
            set status = 'clean', no_findings_streak = no_findings_streak + 1
            where id = (select review_scope_id from review_plans where id = ?1)
            "#,
            params![review_plan_id],
        )?;
    }
    Ok(status.to_string())
}

fn enforce_run_allowed(
    conn: &rusqlite::Connection,
    policy: &StoredReviewPolicy,
    plan: &StoredReviewPlan,
    run_type: &str,
    target_ref: Option<&str>,
) -> Result<()> {
    if run_type == "fresh" && !policy.allow_fresh_review {
        bail!("fresh review is disabled by policy");
    }
    if run_type == "resume" && !policy.allow_resume_review {
        bail!("resume review is disabled by policy");
    }
    let mut limit = match run_type {
        "fresh" => policy.max_fresh_agents,
        "resume" => policy.max_resume_agents,
        "coverage" => policy.max_fresh_agents,
        _ => bail!("invalid review run type"),
    };
    let used = if run_type == "resume"
        && target_ref.is_some_and(|target| target.starts_with("review-context:finding-fix:"))
    {
        limit = limit.max(1);
        conn.query_row(
            "select count(*) from review_runs where review_plan_id = ?1 and run_type = 'resume' and target_ref = ?2",
            params![plan.id, target_ref],
            |row| row.get(0),
        )?
    } else if run_type == "fresh" && plan.fresh_review_after_run_id > 0 {
        limit = limit.max(1);
        conn.query_row(
            "select count(*) from review_runs where review_plan_id = ?1 and run_type = 'fresh' and id > ?2",
            params![plan.id, plan.fresh_review_after_run_id],
            |row| row.get(0),
        )?
    } else {
        count_invocations(conn, policy, plan, run_type, false)?
    };
    if used >= limit {
        match policy.on_max_agents_exceeded.as_str() {
            "mark_exhausted" => {
                conn.execute(
                    "update review_plans set status = 'exhausted' where id = ?1",
                    params![plan.id],
                )?;
                bail!("review agent limit exceeded; review plan marked exhausted");
            }
            "accept_with_user_approval" => {
                conn.execute(
                    "update review_plans set status = 'needs_user_decision' where id = ?1",
                    params![plan.id],
                )?;
                bail!("review agent limit exceeded; user approval is required");
            }
            _ => bail!("review agent limit exceeded"),
        }
    }
    let running = count_invocations(conn, policy, plan, "", true)?;
    if running >= policy.max_parallel_agents {
        bail!("max parallel review agents exceeded");
    }
    Ok(())
}

fn consecutive_clean_runs(
    conn: &rusqlite::Connection,
    plan: &StoredReviewPlan,
    run_type: &str,
) -> Result<i64> {
    let required_context =
        review_context_kind_for_plan(&plan.stage, &plan.review_type).and_then(|kind| {
            plan.design_version_id.map(|design_version_id| {
                review_context_ref(kind, Some(design_version_id), Some(plan.work_unit_id))
            })
        });
    let mut stmt = conn.prepare(
        r#"
        select r.clean_run, r.review_provenance, r.review_provenance_ref,
               exists (
                   select 1
                   from review_agent_invocations i
                   where i.review_run_id = r.id
                     and i.external_agent_id is not null
                     and i.external_agent_id != ''
               )
        from review_runs
        r
        where r.review_plan_id = ?1
          and r.run_type = ?2
          and (?3 = 0 or r.id > ?3)
          and r.status = 'completed'
          and r.new_findings_count = 0
        order by id desc
        "#,
    )?;
    let fresh_boundary = if run_type == "fresh" {
        plan.fresh_review_after_run_id
    } else {
        0
    };
    let rows = stmt.query_map(params![plan.id, run_type, fresh_boundary], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, i64>(3)? == 1,
        ))
    })?;
    let mut count = 0;
    for row in rows {
        let (clean_run, provenance, provenance_ref, has_external_agent) = row?;
        let trusted = required_context.is_none()
            || trusted_review_provenance(
                &provenance,
                provenance_ref.as_deref(),
                has_external_agent,
            );
        if clean_run == 1 && trusted {
            count += 1;
        } else {
            break;
        }
    }
    Ok(count)
}

fn count_invocations(
    conn: &rusqlite::Connection,
    policy: &StoredReviewPolicy,
    plan: &StoredReviewPlan,
    run_type: &str,
    active_only: bool,
) -> Result<i64> {
    let status_filter = if active_only {
        "and i.status in ('requested', 'running')"
    } else {
        ""
    };
    let run_type_filter = if run_type.is_empty() {
        "".to_string()
    } else {
        format!("and run_type = '{run_type}'")
    };
    let (scope_sql, scope_id) = match policy.run_count_scope.as_str() {
        "review_scope" => match plan.review_scope_id {
            Some(review_scope_id) => ("p.review_scope_id = ?1", review_scope_id),
            None => ("i.review_plan_id = ?1", plan.id),
        },
        "work_unit" => ("p.work_unit_id = ?1", plan.work_unit_id),
        _ => ("i.review_plan_id = ?1", plan.id),
    };
    let sql = format!(
        r#"
        select count(*)
        from review_agent_invocations i
        join review_plans p on p.id = i.review_plan_id
        where {scope_sql} {run_type_filter} {status_filter}
        "#
    );
    conn.query_row(&sql, params![scope_id], |row| row.get(0))
        .map_err(Into::into)
}

fn validate_review_plan_references(
    conn: &rusqlite::Connection,
    project_id: i64,
    review_type: &str,
    review_policy_id: Option<i64>,
    review_scope_id: Option<i64>,
) -> Result<()> {
    if let Some(review_policy_id) = review_policy_id {
        let policy_type: String = conn
            .query_row(
                "select review_type from review_policies where id = ?1 and project_id = ?2",
                params![review_policy_id, project_id],
                |row| row.get(0),
            )
            .optional()?
            .context("review policy not found")?;
        if policy_type != review_type {
            bail!("review policy type must match review plan type");
        }
    }
    if let Some(review_scope_id) = review_scope_id {
        let scope_type: String = conn
            .query_row(
                "select review_type from review_scopes where id = ?1 and project_id = ?2",
                params![review_scope_id, project_id],
                |row| row.get(0),
            )
            .optional()?
            .context("review scope not found")?;
        if scope_type != review_type {
            bail!("review scope type must match review plan type");
        }
    }
    Ok(())
}

fn validate_review_target_shape(input: &NewReviewPlanTarget<'_>) -> Result<()> {
    let present_count = [
        input.design_version_id.is_some(),
        input.design_requirement_id.is_some(),
        input.task_id.is_some(),
        input.work_unit_id.is_some(),
        input.phase_id.is_some(),
        input.repository_snapshot_id.is_some(),
        input.file_path.is_some(),
        input.symbol.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count();
    if present_count != 1 {
        bail!("review plan target requires exactly one typed target value");
    }
    let valid = match input.target_type {
        "design_version" => input.design_version_id.is_some(),
        "design_requirement" => input.design_requirement_id.is_some(),
        "task" => input.task_id.is_some(),
        "work_unit" => input.work_unit_id.is_some(),
        "phase" => input.phase_id.is_some(),
        "repository_snapshot" => input.repository_snapshot_id.is_some(),
        "file" => input
            .file_path
            .is_some_and(|value| !value.trim().is_empty()),
        "symbol" => input.symbol.is_some_and(|value| !value.trim().is_empty()),
        _ => false,
    };
    if !valid {
        bail!("review plan target value must match target type");
    }
    Ok(())
}

fn validate_review_run_result(input: &NewReviewRun<'_>) -> Result<()> {
    if input.new_findings_count < 0 || input.carried_findings_checked < 0 {
        bail!("review run counters must be non-negative");
    }
    if input.clean_run && input.new_findings_count > 0 {
        bail!("clean review run cannot report new findings");
    }
    if input.clean_run && input.status != "completed" {
        bail!("clean review run must be completed");
    }
    validate_review_provenance(input.review_provenance, input.review_provenance_ref)?;
    Ok(())
}

fn validate_gate_context_target(
    plan: &StoredReviewPlan,
    input: &NewReviewRun<'_>,
    target: &ResolvedRunTarget,
) -> Result<()> {
    if !input.clean_run
        || input.status != "completed"
        || input.run_type != "fresh"
        || input.run_purpose != "new_unbiased_review"
    {
        return Ok(());
    }
    let Some(kind) = review_context_kind_for_plan(&plan.stage, &plan.review_type) else {
        return Ok(());
    };
    let Some(design_version_id) = plan.design_version_id else {
        return Ok(());
    };
    let expected = if let Some(phase_id) = target.phase_id {
        review_context_ref_with_phase(
            kind,
            Some(design_version_id),
            Some(plan.work_unit_id),
            Some(phase_id),
        )
    } else {
        review_context_ref(kind, Some(design_version_id), Some(plan.work_unit_id))
    };
    if input.target_ref != Some(expected.as_str()) {
        bail!(
            "clean gate review run must use review-context target {expected}; run review-context first and pass context_ref with --target"
        );
    }
    let has_external_agent = input
        .external_agent_id
        .is_some_and(|external_agent_id| !external_agent_id.trim().is_empty());
    if !trusted_review_provenance(
        input.review_provenance,
        input.review_provenance_ref,
        has_external_agent,
    ) {
        bail!(
            "clean gate review run requires trusted review provenance; pass --provenance external_agent --external-agent-id <id> --provenance-ref <review-output-ref>, or --provenance human_review --provenance-ref <review-output-ref>"
        );
    }
    Ok(())
}

fn validate_review_provenance(provenance: &str, provenance_ref: Option<&str>) -> Result<()> {
    match provenance {
        "self_recorded" => Ok(()),
        "external_agent" | "human_review" => {
            if provenance_ref.is_none_or(|value| value.trim().is_empty()) {
                bail!("{provenance} review provenance requires --provenance-ref");
            }
            Ok(())
        }
        _ => bail!("review provenance must be self_recorded, external_agent, or human_review"),
    }
}

fn trusted_review_provenance(
    provenance: &str,
    provenance_ref: Option<&str>,
    has_external_agent: bool,
) -> bool {
    let has_ref = provenance_ref.is_some_and(|value| !value.trim().is_empty());
    match provenance {
        "external_agent" => has_external_agent && has_ref,
        "human_review" => has_ref,
        _ => false,
    }
}

fn review_context_kind_for_plan(stage: &str, review_type: &str) -> Option<&'static str> {
    match (stage, review_type) {
        ("design-ready", "design_review") => Some("design-review"),
        ("implementation-ready", "design_task_decomposition") => Some("design-task-decomposition"),
        ("close-ready", "design_implementation_diff") => Some("design-implementation-diff"),
        ("close-ready", "implementation_review") => Some("implementation-review"),
        _ => None,
    }
}

fn get_or_create_default_policy(
    conn: &rusqlite::Connection,
    project_id: i64,
    review_type: &str,
) -> Result<i64> {
    let name = format!("default-{review_type}");
    if let Some(id) = conn
        .query_row(
            "select id from review_policies where project_id = ?1 and name = ?2",
            params![project_id, name],
            |row| row.get(0),
        )
        .optional()?
    {
        return Ok(id);
    }
    conn.execute(
        r#"
        insert into review_policies(
            project_id, name, review_type, max_fresh_agents, max_resume_agents,
            max_parallel_agents, required_consecutive_clean_fresh_runs,
            required_consecutive_clean_resume_runs, stop_on_severity,
            allow_resume_review, allow_fresh_review, allow_new_findings_in_resume,
            on_max_agents_exceeded, run_count_scope, default_run_mode, created_at
        )
        values (?1, ?2, ?3, 1, 1, 1, 1, 0, 'none', 1, 1, 0, 'block', 'review_plan', 'fresh', current_timestamp)
        "#,
        params![project_id, name, review_type],
    )?;
    Ok(conn.last_insert_rowid())
}

fn load_review_plan(
    conn: &rusqlite::Connection,
    project_id: i64,
    review_plan_id: i64,
) -> Result<StoredReviewPlan> {
    conn.query_row(
        r#"
        select id, review_policy_id, review_scope_id, design_version_id, work_unit_id,
               review_type, stage, coalesce(fresh_review_after_run_id, 0)
        from review_plans
        where id = ?1 and project_id = ?2
        "#,
        params![review_plan_id, project_id],
        |row| {
            Ok(StoredReviewPlan {
                id: row.get(0)?,
                review_policy_id: row.get(1)?,
                review_scope_id: row.get(2)?,
                design_version_id: row.get(3)?,
                work_unit_id: row.get(4)?,
                review_type: row.get(5)?,
                stage: row.get(6)?,
                fresh_review_after_run_id: row.get(7)?,
            })
        },
    )
    .optional()?
    .context("review plan not found")
}

fn load_review_policy(
    conn: &rusqlite::Connection,
    project_id: i64,
    review_policy_id: i64,
) -> Result<StoredReviewPolicy> {
    conn.query_row(
        r#"
        select
            id, max_fresh_agents, max_resume_agents, max_parallel_agents,
            required_consecutive_clean_fresh_runs,
            required_consecutive_clean_resume_runs, stop_on_severity,
            allow_resume_review, allow_fresh_review, allow_new_findings_in_resume,
            on_max_agents_exceeded, run_count_scope
        from review_policies
        where id = ?1 and project_id = ?2
        "#,
        params![review_policy_id, project_id],
        |row| {
            Ok(StoredReviewPolicy {
                max_fresh_agents: row.get(1)?,
                max_resume_agents: row.get(2)?,
                max_parallel_agents: row.get(3)?,
                required_consecutive_clean_fresh_runs: row.get(4)?,
                required_consecutive_clean_resume_runs: row.get(5)?,
                stop_on_severity: row.get(6)?,
                allow_resume_review: row.get::<_, i64>(7)? == 1,
                allow_fresh_review: row.get::<_, i64>(8)? == 1,
                allow_new_findings_in_resume: row.get::<_, i64>(9)? == 1,
                on_max_agents_exceeded: row.get(10)?,
                run_count_scope: row.get(11)?,
            })
        },
    )
    .optional()?
    .context("review policy not found")
}

fn resolve_run_target(
    conn: &rusqlite::Connection,
    project_id: i64,
    plan: &StoredReviewPlan,
    explicit_target_ref: Option<&str>,
) -> Result<ResolvedRunTarget> {
    if let Some(target_ref) = explicit_target_ref
        && let Some(target) = parse_structured_target_ref(target_ref)?
    {
        ensure_plan_has_target(conn, plan.id, &target)?;
        return Ok(target.with_ref(target_ref));
    }
    if let Some(design_version_id) = plan.design_version_id {
        ensure_project_row(conn, "design_versions", design_version_id, project_id)?;
        Ok(ResolvedRunTarget {
            target_type: "design_version",
            design_version_id: Some(design_version_id),
            design_requirement_id: None,
            task_id: None,
            work_unit_id: None,
            phase_id: None,
            repository_snapshot_id: None,
            file_path: None,
            symbol: None,
            target_ref: explicit_target_ref
                .map(str::to_string)
                .unwrap_or_else(|| format!("design_version:{design_version_id}")),
        })
    } else {
        ensure_project_row(conn, "work_units", plan.work_unit_id, project_id)?;
        Ok(ResolvedRunTarget {
            target_type: "work_unit",
            design_version_id: None,
            design_requirement_id: None,
            task_id: None,
            work_unit_id: Some(plan.work_unit_id),
            phase_id: None,
            repository_snapshot_id: None,
            file_path: None,
            symbol: None,
            target_ref: explicit_target_ref
                .map(str::to_string)
                .unwrap_or_else(|| format!("work_unit:{}", plan.work_unit_id)),
        })
    }
}

fn parse_structured_target_ref(target_ref: &str) -> Result<Option<ResolvedRunTarget>> {
    if let Some(phase_id) = review_context_phase_id(target_ref)? {
        return Ok(Some(ResolvedRunTarget::typed_id("phase", phase_id)));
    }
    let Some((target_type, value)) = target_ref.split_once(':') else {
        return Ok(None);
    };
    match target_type {
        "design_version" => Ok(Some(ResolvedRunTarget::typed_id(
            "design_version",
            value.parse()?,
        ))),
        "design_requirement" => Ok(Some(ResolvedRunTarget::typed_id(
            "design_requirement",
            value.parse()?,
        ))),
        "task" => Ok(Some(ResolvedRunTarget::typed_id("task", value.parse()?))),
        "work_unit" => Ok(Some(ResolvedRunTarget::typed_id(
            "work_unit",
            value.parse()?,
        ))),
        "phase" => Ok(Some(ResolvedRunTarget::typed_id("phase", value.parse()?))),
        "repository_snapshot" => Ok(Some(ResolvedRunTarget::typed_id(
            "repository_snapshot",
            value.parse()?,
        ))),
        "file" => Ok(Some(ResolvedRunTarget {
            target_type: "file",
            design_version_id: None,
            design_requirement_id: None,
            task_id: None,
            work_unit_id: None,
            phase_id: None,
            repository_snapshot_id: None,
            file_path: Some(value.to_string()),
            symbol: None,
            target_ref: value.to_string(),
        })),
        "symbol" => Ok(Some(ResolvedRunTarget {
            target_type: "symbol",
            design_version_id: None,
            design_requirement_id: None,
            task_id: None,
            work_unit_id: None,
            phase_id: None,
            repository_snapshot_id: None,
            file_path: None,
            symbol: Some(value.to_string()),
            target_ref: value.to_string(),
        })),
        _ => Ok(None),
    }
}

fn review_context_phase_id(target_ref: &str) -> Result<Option<i64>> {
    if !target_ref.starts_with("review-context:") {
        return Ok(None);
    }
    for part in target_ref.split(':') {
        if let Some(value) = part.strip_prefix("phase=") {
            return Ok(Some(value.parse()?));
        }
    }
    Ok(None)
}

fn ensure_plan_has_target(
    conn: &rusqlite::Connection,
    review_plan_id: i64,
    target: &ResolvedRunTarget,
) -> Result<()> {
    if target.target_type == "phase" {
        conn.query_row(
            r#"
            select 1
            from work_phase_review_targets
            where review_plan_id = ?1
              and phase_id = ?2
            "#,
            params![review_plan_id, target.phase_id],
            |_| Ok(()),
        )
        .optional()?
        .context("review run target must be included in review plan targets")?;
        return Ok(());
    }

    conn.query_row(
        r#"
        select 1
        from review_plan_targets
        where review_plan_id = ?1
          and target_type = ?2
          and coalesce(design_version_id, -1) = coalesce(?3, -1)
          and coalesce(design_requirement_id, -1) = coalesce(?4, -1)
          and coalesce(task_id, -1) = coalesce(?5, -1)
          and coalesce(work_unit_id, -1) = coalesce(?6, -1)
          and coalesce(repository_snapshot_id, -1) = coalesce(?7, -1)
          and coalesce(file_path, '') = coalesce(?8, '')
          and coalesce(symbol, '') = coalesce(?9, '')
        "#,
        params![
            review_plan_id,
            target.target_type,
            target.design_version_id,
            target.design_requirement_id,
            target.task_id,
            target.work_unit_id,
            target.repository_snapshot_id,
            target.file_path.as_deref(),
            target.symbol.as_deref(),
        ],
        |_| Ok(()),
    )
    .optional()?
    .context("review run target is not registered on the review plan")?;
    Ok(())
}

fn ensure_project_row(
    conn: &rusqlite::Connection,
    table: &str,
    id: i64,
    project_id: i64,
) -> Result<()> {
    let sql = format!("select 1 from {table} where id = ?1 and project_id = ?2");
    conn.query_row(&sql, params![id, project_id], |_| Ok(()))
        .optional()?
        .context("review run target not found for project")?;
    Ok(())
}

fn agent_role_for_review_type(review_type: &str) -> Result<&'static str> {
    match review_type {
        "design_review" => Ok("design_document_review"),
        "design_task_decomposition" => Ok("design_task_decomposition"),
        "design_implementation_diff" => Ok("design_implementation_diff_review"),
        "implementation_review" => Ok("implementation_review"),
        "general" => Ok("general"),
        _ => bail!("invalid review type"),
    }
}

fn ensure_finding_type_matches_review_type(finding_type: &str, review_type: &str) -> Result<()> {
    let allowed = match review_type {
        "design_review" => matches!(finding_type, "design_finding"),
        "design_implementation_diff" => matches!(finding_type, "design_implementation_drift"),
        "design_task_decomposition" => matches!(finding_type, "design_task_gap"),
        "implementation_review" => {
            matches!(finding_type, "implementation_finding" | "coverage_finding")
        }
        "general" => matches!(
            finding_type,
            "design_finding"
                | "design_implementation_drift"
                | "design_task_gap"
                | "implementation_finding"
                | "coverage_finding"
        ),
        _ => false,
    };
    if !allowed {
        bail!("finding type does not match review type");
    }
    Ok(())
}

fn severity_block_filter(stop_on_severity: &str) -> Result<i64> {
    match stop_on_severity {
        "none" => Ok(0),
        "critical" => Ok(4),
        "high" => Ok(3),
        "medium" => Ok(2),
        "low" => Ok(1),
        _ => bail!("invalid stop_on_severity"),
    }
}

fn validate_run_type_purpose(run_type: &str, run_purpose: &str) -> Result<()> {
    match (run_type, run_purpose) {
        ("fresh", "new_unbiased_review")
        | ("resume", "finding_fix_verification")
        | ("coverage", "coverage_audit") => Ok(()),
        _ => bail!("invalid review run type and purpose combination"),
    }
}

fn review_policy_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReviewPolicyRecord> {
    Ok(ReviewPolicyRecord {
        id: row.get(0)?,
        name: row.get(1)?,
        review_type: row.get(2)?,
        max_fresh_agents: row.get(3)?,
        max_resume_agents: row.get(4)?,
        max_parallel_agents: row.get(5)?,
        required_consecutive_clean_fresh_runs: row.get(6)?,
        required_consecutive_clean_resume_runs: row.get(7)?,
        stop_on_severity: row.get(8)?,
        allow_resume_review: row.get::<_, i64>(9)? == 1,
        allow_fresh_review: row.get::<_, i64>(10)? == 1,
        allow_new_findings_in_resume: row.get::<_, i64>(11)? == 1,
        on_max_agents_exceeded: row.get(12)?,
        run_count_scope: row.get(13)?,
        default_run_mode: row.get(14)?,
    })
}

fn collect_rows<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>>,
) -> Result<Vec<T>> {
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

fn bool_to_i64(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

struct StoredReviewPlan {
    id: i64,
    review_policy_id: i64,
    review_scope_id: Option<i64>,
    design_version_id: Option<i64>,
    work_unit_id: i64,
    review_type: String,
    stage: String,
    fresh_review_after_run_id: i64,
}

struct StoredReviewPolicy {
    max_fresh_agents: i64,
    max_resume_agents: i64,
    max_parallel_agents: i64,
    required_consecutive_clean_fresh_runs: i64,
    required_consecutive_clean_resume_runs: i64,
    stop_on_severity: String,
    allow_resume_review: bool,
    allow_fresh_review: bool,
    allow_new_findings_in_resume: bool,
    on_max_agents_exceeded: String,
    run_count_scope: String,
}

struct StoredFinding {
    id: i64,
    classification: String,
    status: String,
}

struct CorrectionToken {
    kind: String,
    operation: String,
    target: String,
}

struct StoredReviewRunPolicy {
    run_type: String,
    review_policy_id: i64,
    review_type: String,
    clean_run: bool,
}

struct StoredReviewRunPurpose {
    run_type: String,
    run_purpose: String,
    finding_fix_result: Option<String>,
    clean_run: bool,
    new_findings_count: i64,
    carried_findings_checked: i64,
    _target_ref: Option<String>,
    review_provenance: String,
    review_provenance_ref: Option<String>,
    has_external_agent: bool,
}

struct ResolvedRunTarget {
    target_type: &'static str,
    design_version_id: Option<i64>,
    design_requirement_id: Option<i64>,
    task_id: Option<i64>,
    work_unit_id: Option<i64>,
    phase_id: Option<i64>,
    repository_snapshot_id: Option<i64>,
    file_path: Option<String>,
    symbol: Option<String>,
    target_ref: String,
}

impl ResolvedRunTarget {
    fn typed_id(target_type: &'static str, id: i64) -> Self {
        Self {
            target_type,
            design_version_id: (target_type == "design_version").then_some(id),
            design_requirement_id: (target_type == "design_requirement").then_some(id),
            task_id: (target_type == "task").then_some(id),
            work_unit_id: (target_type == "work_unit").then_some(id),
            phase_id: (target_type == "phase").then_some(id),
            repository_snapshot_id: (target_type == "repository_snapshot").then_some(id),
            file_path: None,
            symbol: None,
            target_ref: format!("{target_type}:{id}"),
        }
    }

    fn with_ref(mut self, target_ref: &str) -> Self {
        self.target_ref = target_ref.to_string();
        self
    }
}

pub struct NewReviewScope<'a> {
    pub name: &'a str,
    pub review_type: &'a str,
    pub scope: &'a str,
    pub allowed_inputs: Option<&'a str>,
    pub forbidden_judgments: Option<&'a str>,
    pub expected_output_type: Option<&'a str>,
    pub exclusions: Option<&'a str>,
    pub prompt_template_ref: Option<&'a str>,
}

pub struct NewReviewPolicy<'a> {
    pub name: &'a str,
    pub review_type: &'a str,
    pub max_fresh_agents: i64,
    pub max_resume_agents: i64,
    pub max_parallel_agents: i64,
    pub required_consecutive_clean_fresh_runs: i64,
    pub required_consecutive_clean_resume_runs: i64,
    pub stop_on_severity: &'a str,
    pub allow_resume_review: bool,
    pub allow_fresh_review: bool,
    pub allow_new_findings_in_resume: bool,
    pub on_max_agents_exceeded: &'a str,
    pub run_count_scope: &'a str,
    pub default_run_mode: &'a str,
}

pub struct NewReviewPlan<'a> {
    pub work_unit_id: i64,
    pub design_version_id: Option<i64>,
    pub review_type: &'a str,
    pub required: bool,
    pub stage: &'a str,
    pub scope: Option<&'a str>,
    pub clean_condition: Option<&'a str>,
    pub stop_condition: Option<&'a str>,
    pub review_policy_id: Option<i64>,
    pub review_scope_id: Option<i64>,
}

pub struct NewReviewPlanTarget<'a> {
    pub review_plan_id: i64,
    pub target_type: &'a str,
    pub design_version_id: Option<i64>,
    pub design_requirement_id: Option<i64>,
    pub task_id: Option<i64>,
    pub work_unit_id: Option<i64>,
    pub phase_id: Option<i64>,
    pub repository_snapshot_id: Option<i64>,
    pub file_path: Option<&'a str>,
    pub symbol: Option<&'a str>,
}

pub struct ReviewPlanWaiver<'a> {
    pub review_plan_id: i64,
    pub reason: &'a str,
    pub approval_authority_event_id: i64,
}

pub struct NewReviewRun<'a> {
    pub review_plan_id: i64,
    pub run_type: &'a str,
    pub run_purpose: &'a str,
    pub target_ref: Option<&'a str>,
    pub prompt_deviations: Option<&'a str>,
    pub result_summary: Option<&'a str>,
    pub new_findings_count: i64,
    pub carried_findings_checked: i64,
    pub clean_run: bool,
    pub status: &'a str,
    pub agent_label: Option<&'a str>,
    pub external_agent_id: Option<&'a str>,
    pub review_provenance: &'a str,
    pub review_provenance_ref: Option<&'a str>,
}

pub struct NewFinding<'a> {
    pub review_run_id: i64,
    pub finding_type: &'a str,
    pub severity: &'a str,
    pub description: &'a str,
    pub design_requirement_id: Option<i64>,
    pub task_id: Option<i64>,
}

pub struct NewClosure<'a> {
    pub finding_id: i64,
    pub design_invariant: &'a str,
    pub design_citations: Option<&'a str>,
    pub implementation_evidence: Option<&'a str>,
    pub affected_surfaces: Option<&'a str>,
    pub same_invariant_search: Option<&'a str>,
    pub other_violations_found: Option<&'a str>,
    pub fix_plan: Option<&'a str>,
    pub tests_or_gates: Option<&'a str>,
    pub verification_plan: Option<&'a str>,
    pub closed_by_commit: Option<&'a str>,
}

pub struct ClosureReady<'a> {
    pub closure_id: i64,
    pub implementation_evidence: &'a str,
    pub tests_or_gates: &'a str,
    pub closed_by_commit: Option<&'a str>,
}

pub struct ClosureSupersession<'a> {
    pub closure_id: i64,
    pub new_closure: NewClosure<'a>,
    pub reason: &'a str,
    pub authority_event_id: i64,
}

pub struct FindingOutOfScope<'a> {
    pub finding_id: i64,
    pub reason: &'a str,
    pub authority_event_id: i64,
}

pub struct NewFindingVerification<'a> {
    pub review_run_id: i64,
    pub finding_id: i64,
    pub closure_id: i64,
    pub result: &'a str,
    pub notes: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReviewScopeOutcome {
    pub review_scope_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReviewPolicyOutcome {
    pub review_policy_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReviewPlanOutcome {
    pub review_plan_id: i64,
    pub review_policy_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReviewPlanTargetOutcome {
    pub review_plan_target_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReviewPlanWaiverOutcome {
    pub review_plan_id: i64,
    pub acceptance_record_id: i64,
    pub authority_event_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReviewRunOutcome {
    pub review_run_id: i64,
    pub review_agent_invocation_id: i64,
    pub review_plan_id: i64,
    pub plan_status: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct FindingOutcome {
    pub finding_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct FindingClassificationOutcome {
    pub finding_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ClosureOutcome {
    pub closure_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CorrectionBeginOutcome {
    pub closure_id: i64,
    pub session_id: i64,
    pub token_count: i64,
    pub idempotent: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CorrectionTransitionOutcome {
    pub closure_id: i64,
    pub token_ordinal: i64,
    pub application_id: i64,
    pub result_ref: String,
    pub idempotent: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ClosureReadyOutcome {
    pub closure_id: i64,
    pub finding_id: i64,
    pub attempt_id: i64,
    pub attempt_number: i64,
    pub context_ref: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ClosureSupersessionOutcome {
    pub closure_id: i64,
    pub superseded_closure_id: i64,
    pub finding_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct FindingOutOfScopeOutcome {
    pub finding_id: i64,
    pub acceptance_record_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct FindingVerificationOutcome {
    pub finding_verification_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReviewScopeRecord {
    pub id: i64,
    pub name: String,
    pub review_type: String,
    pub agent_role: String,
    pub scope: String,
    pub status: String,
    pub no_findings_streak: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReviewPolicyRecord {
    pub id: i64,
    pub name: String,
    pub review_type: String,
    pub max_fresh_agents: i64,
    pub max_resume_agents: i64,
    pub max_parallel_agents: i64,
    pub required_consecutive_clean_fresh_runs: i64,
    pub required_consecutive_clean_resume_runs: i64,
    pub stop_on_severity: String,
    pub allow_resume_review: bool,
    pub allow_fresh_review: bool,
    pub allow_new_findings_in_resume: bool,
    pub on_max_agents_exceeded: String,
    pub run_count_scope: String,
    pub default_run_mode: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReviewPlanRecord {
    pub id: i64,
    pub work_unit_id: i64,
    pub design_version_id: Option<i64>,
    pub review_type: String,
    pub required: bool,
    pub stage: String,
    pub scope: Option<String>,
    pub review_policy_id: Option<i64>,
    pub review_scope_id: Option<i64>,
    pub status: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReviewPlanTargetRecord {
    pub id: i64,
    pub review_plan_id: i64,
    pub target_type: String,
    pub design_version_id: Option<i64>,
    pub design_requirement_id: Option<i64>,
    pub task_id: Option<i64>,
    pub work_unit_id: Option<i64>,
    pub phase_id: Option<i64>,
    pub repository_snapshot_id: Option<i64>,
    pub file_path: Option<String>,
    pub symbol: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReviewRunRecord {
    pub id: i64,
    pub review_plan_id: Option<i64>,
    pub run_type: String,
    pub run_purpose: String,
    pub target_type: String,
    pub target_ref: Option<String>,
    pub new_findings_count: i64,
    pub carried_findings_checked: i64,
    pub clean_run: bool,
    pub status: String,
    pub review_provenance: String,
    pub review_provenance_ref: Option<String>,
    pub finding_fix_result: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct FindingRecord {
    pub id: i64,
    pub review_run_id: i64,
    pub finding_type: String,
    pub severity: String,
    pub description: String,
    pub classification: String,
    pub status: String,
}
