use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{open_existing_project, project_id};
use crate::review_context::{review_context_ref, review_context_ref_with_phase};

use super::*;

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

pub(super) fn refresh_plan_for_run(
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

pub(super) fn evaluate_plan_status(
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
          and not exists (
            select 1 from legacy_claim_audits l
            where l.project_id=f.project_id and l.review_run_id=f.review_run_id
              and l.reviewer_resolution in ('unbound','ambiguous')
          )
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

pub(super) fn enforce_run_allowed(
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

pub(super) fn consecutive_clean_runs(
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
               ),
               exists (
                   select 1 from review_adjudication_decisions d
                   where d.review_run_id=r.id and d.value='accepted'
                     and not exists(select 1 from review_adjudication_decisions newer where newer.predecessor_id=d.id)
               ),
               exists (
                   select 1 from review_agent_invocations i
                   where i.review_run_id=r.id and i.review_provenance_id is not null
               ) or exists (
                   select 1 from legacy_claim_audits l
                   where l.review_run_id=r.id and l.reviewer_resolution='trusted'
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
            row.get::<_, i64>(4)? == 1,
            row.get::<_, i64>(5)? == 1,
        ))
    })?;
    let mut count = 0;
    for row in rows {
        let (
            clean_run,
            _provenance,
            _provenance_ref,
            _has_external_agent,
            accepted,
            trusted_ingress,
        ) = row?;
        let trusted = required_context.is_none() || trusted_ingress;
        if clean_run == 1 && trusted && accepted {
            count += 1;
        } else {
            break;
        }
    }
    Ok(count)
}

pub(super) fn count_invocations(
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

pub(super) fn validate_review_plan_references(
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

pub(super) fn validate_review_target_shape(input: &NewReviewPlanTarget<'_>) -> Result<()> {
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

pub(super) fn validate_review_run_result(input: &NewReviewRun<'_>) -> Result<()> {
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

pub(super) fn validate_gate_context_target(
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

pub(super) fn validate_review_provenance(
    provenance: &str,
    provenance_ref: Option<&str>,
) -> Result<()> {
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

pub(super) fn trusted_review_provenance(
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

pub(super) fn review_context_kind_for_plan(stage: &str, review_type: &str) -> Option<&'static str> {
    match (stage, review_type) {
        ("design-ready", "design_review") => Some("design-review"),
        ("implementation-ready", "design_task_decomposition") => Some("design-task-decomposition"),
        ("close-ready", "design_implementation_diff") => Some("design-implementation-diff"),
        ("close-ready", "implementation_review") => Some("implementation-review"),
        _ => None,
    }
}

pub(super) fn get_or_create_default_policy(
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

pub(super) fn load_review_plan(
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

pub(super) fn load_review_policy(
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

pub(super) fn resolve_run_target(
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

pub(super) fn parse_structured_target_ref(target_ref: &str) -> Result<Option<ResolvedRunTarget>> {
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

pub(super) fn review_context_phase_id(target_ref: &str) -> Result<Option<i64>> {
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

pub(super) fn ensure_plan_has_target(
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

pub(super) fn ensure_project_row(
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

pub(super) fn agent_role_for_review_type(review_type: &str) -> Result<&'static str> {
    match review_type {
        "design_review" => Ok("design_document_review"),
        "design_task_decomposition" => Ok("design_task_decomposition"),
        "design_implementation_diff" => Ok("design_implementation_diff_review"),
        "implementation_review" => Ok("implementation_review"),
        "general" => Ok("general"),
        _ => bail!("invalid review type"),
    }
}

pub(super) fn ensure_finding_type_matches_review_type(
    finding_type: &str,
    review_type: &str,
) -> Result<()> {
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

pub(super) fn severity_block_filter(stop_on_severity: &str) -> Result<i64> {
    match stop_on_severity {
        "none" => Ok(0),
        "critical" => Ok(4),
        "high" => Ok(3),
        "medium" => Ok(2),
        "low" => Ok(1),
        _ => bail!("invalid stop_on_severity"),
    }
}

pub(super) fn validate_run_type_purpose(run_type: &str, run_purpose: &str) -> Result<()> {
    match (run_type, run_purpose) {
        ("fresh", "new_unbiased_review")
        | ("resume", "finding_fix_verification")
        | ("coverage", "coverage_audit") => Ok(()),
        _ => bail!("invalid review run type and purpose combination"),
    }
}

pub(super) fn review_policy_record(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ReviewPolicyRecord> {
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

pub(super) fn collect_rows<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>>,
) -> Result<Vec<T>> {
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

pub(super) fn bool_to_i64(value: bool) -> i64 {
    if value { 1 } else { 0 }
}
