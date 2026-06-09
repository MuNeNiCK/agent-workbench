use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{open_existing_project, project_id};
use crate::review_context::review_context_ref;
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
        select
            t.id, t.review_plan_id, t.target_type, t.design_version_id,
            t.design_requirement_id, t.task_id, t.work_unit_id,
            t.repository_snapshot_id, t.file_path, t.symbol
        from review_plan_targets t
        join review_plans p on p.id = t.review_plan_id
        where t.review_plan_id = ?1 and p.project_id = ?2
        order by t.id
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
            repository_snapshot_id: row.get(7)?,
            file_path: row.get(8)?,
            symbol: row.get(9)?,
        })
    })?;
    collect_rows(rows)
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
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let plan = load_review_plan(&tx, project_id, input.review_plan_id)?;
    let policy = load_review_policy(&tx, project_id, plan.review_policy_id)?;
    validate_run_type_purpose(input.run_type, input.run_purpose)?;
    validate_review_run_result(&input)?;
    validate_gate_context_target(&plan, &input)?;
    enforce_run_allowed(&tx, &policy, &plan, input.run_type)?;
    if input.run_type == "resume"
        && input.new_findings_count > 0
        && !policy.allow_new_findings_in_resume
    {
        bail!("new findings are disabled for resume review by policy");
    }
    let target = resolve_run_target(&tx, project_id, &plan, input.target_ref)?;

    tx.execute(
        r#"
        insert into review_runs(
            project_id, review_scope_id, review_plan_id, run_type, run_purpose,
            target_type, design_version_id, design_requirement_id, task_id,
            work_unit_id, repository_snapshot_id, file_path, symbol, target_ref,
            prompt_deviations, result_summary, new_findings_count,
            carried_findings_checked, clean_run, status, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, current_timestamp)
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

pub fn list_review_runs(root: &Path, review_plan_id: Option<i64>) -> Result<Vec<ReviewRunRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select
            id, review_plan_id, run_type, run_purpose, target_type, target_ref,
            new_findings_count, carried_findings_checked, clean_run, status
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
            select r.run_type, p.review_policy_id, r.clean_run
            from review_runs r
            join review_plans p on p.id = r.review_plan_id
            where r.id = ?1 and r.project_id = ?2
            "#,
            params![input.review_run_id, project_id],
            |row| {
                Ok(StoredReviewRunPolicy {
                    run_type: row.get(0)?,
                    review_policy_id: row.get(1)?,
                    clean_run: row.get::<_, i64>(2)? == 1,
                })
            },
        )
        .optional()?
        .context("review run not found")?;
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
    let status = match classification {
        "invalid" => "closed",
        "valid" | "design_conflict" | "needs_evidence" | "unclassified" => "open",
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
    if finding.classification != "valid" {
        bail!("closure requires a valid finding");
    }
    if finding.status != "open" {
        bail!("finding is not open");
    }
    tx.execute(
        r#"
        insert into closures(
            project_id, finding_id, design_invariant, design_citations,
            implementation_evidence, affected_surfaces, same_invariant_search,
            other_violations_found, fix_plan, tests_or_gates,
            verification_plan, closed_by_commit, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, current_timestamp)
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
    tx.commit()?;
    Ok(ClosureOutcome {
        closure_id: conn.last_insert_rowid(),
    })
}

pub fn add_finding_verification(
    root: &Path,
    input: NewFindingVerification<'_>,
) -> Result<FindingVerificationOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let verification_run = tx
        .query_row(
            r#"
            select run_type, run_purpose
            from review_runs
            where id = ?1 and project_id = ?2
            "#,
            params![input.review_run_id, project_id],
            |row| {
                Ok(StoredReviewRunPurpose {
                    run_type: row.get(0)?,
                    run_purpose: row.get(1)?,
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
    tx.query_row(
        r#"
        select 1
        from closures c
        join findings f on f.id = c.finding_id
        join review_runs verifier on verifier.id = ?1 and verifier.project_id = ?5
        join review_runs source_run on source_run.id = f.review_run_id
        where c.id = ?2
          and c.finding_id = ?3
          and c.project_id = ?5
          and f.project_id = ?5
          and f.id = ?3
          and source_run.review_plan_id = verifier.review_plan_id
          and ?4 in ('verified', 'not_fixed', 'needs_evidence', 'out_of_scope')
        "#,
        params![
            input.review_run_id,
            input.closure_id,
            input.finding_id,
            input.result,
            project_id,
        ],
        |_| Ok(()),
    )
    .optional()?
    .context("finding verification target mismatch")?;
    tx.execute(
        r#"
        insert into finding_verifications(
            project_id, review_run_id, finding_id, closure_id, result, notes, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, current_timestamp)
        "#,
        params![
            project_id,
            input.review_run_id,
            input.finding_id,
            input.closure_id,
            input.result,
            input.notes,
        ],
    )?;
    let finding_verification_id = tx.last_insert_rowid();
    if input.result == "verified" {
        tx.execute(
            "update findings set status = 'closed' where id = ?1 and project_id = ?2",
            params![input.finding_id, project_id],
        )?;
    }
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
    let open_blocking_findings: i64 = conn.query_row(
        r#"
        select count(*)
        from findings f
        join review_runs r on r.id = f.review_run_id
        where r.review_plan_id = ?1
          and f.project_id = ?2
          and f.status = 'open'
          and f.classification in ('unclassified', 'valid', 'design_conflict', 'needs_evidence')
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
        params![review_plan_id, project_id],
        |row| row.get(0),
    )?;
    let clean_fresh = consecutive_clean_runs(conn, review_plan_id, "fresh")?;
    let clean_resume = consecutive_clean_runs(conn, review_plan_id, "resume")?;
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
) -> Result<()> {
    if run_type == "fresh" && !policy.allow_fresh_review {
        bail!("fresh review is disabled by policy");
    }
    if run_type == "resume" && !policy.allow_resume_review {
        bail!("resume review is disabled by policy");
    }
    let limit = match run_type {
        "fresh" => policy.max_fresh_agents,
        "resume" => policy.max_resume_agents,
        "coverage" => policy.max_fresh_agents,
        _ => bail!("invalid review run type"),
    };
    let used = count_invocations(conn, policy, plan, run_type, false)?;
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
    review_plan_id: i64,
    run_type: &str,
) -> Result<i64> {
    let mut stmt = conn.prepare(
        r#"
        select clean_run
        from review_runs
        where review_plan_id = ?1
          and run_type = ?2
          and status = 'completed'
          and new_findings_count = 0
        order by id desc
        "#,
    )?;
    let rows = stmt.query_map(params![review_plan_id, run_type], |row| {
        row.get::<_, i64>(0)
    })?;
    let mut count = 0;
    for row in rows {
        if row? == 1 {
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
    Ok(())
}

fn validate_gate_context_target(plan: &StoredReviewPlan, input: &NewReviewRun<'_>) -> Result<()> {
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
    let expected = review_context_ref(kind, Some(design_version_id), Some(plan.work_unit_id));
    if input.target_ref != Some(expected.as_str()) {
        bail!(
            "clean gate review run must use review-context target {expected}; run review-context first and pass context_ref with --target"
        );
    }
    Ok(())
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
               review_type, stage
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
            required_consecutive_clean_resume_runs, allow_resume_review,
            allow_fresh_review, allow_new_findings_in_resume,
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
                allow_resume_review: row.get::<_, i64>(6)? == 1,
                allow_fresh_review: row.get::<_, i64>(7)? == 1,
                allow_new_findings_in_resume: row.get::<_, i64>(8)? == 1,
                on_max_agents_exceeded: row.get(9)?,
                run_count_scope: row.get(10)?,
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
            repository_snapshot_id: None,
            file_path: None,
            symbol: Some(value.to_string()),
            target_ref: value.to_string(),
        })),
        _ => Ok(None),
    }
}

fn ensure_plan_has_target(
    conn: &rusqlite::Connection,
    review_plan_id: i64,
    target: &ResolvedRunTarget,
) -> Result<()> {
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
}

struct StoredReviewPolicy {
    max_fresh_agents: i64,
    max_resume_agents: i64,
    max_parallel_agents: i64,
    required_consecutive_clean_fresh_runs: i64,
    required_consecutive_clean_resume_runs: i64,
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

struct StoredReviewRunPolicy {
    run_type: String,
    review_policy_id: i64,
    clean_run: bool,
}

struct StoredReviewRunPurpose {
    run_type: String,
    run_purpose: String,
}

struct ResolvedRunTarget {
    target_type: &'static str,
    design_version_id: Option<i64>,
    design_requirement_id: Option<i64>,
    task_id: Option<i64>,
    work_unit_id: Option<i64>,
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
    pub repository_snapshot_id: Option<i64>,
    pub file_path: Option<&'a str>,
    pub symbol: Option<&'a str>,
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
