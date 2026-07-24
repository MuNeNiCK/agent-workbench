use std::collections::HashSet;
use std::fmt::Write;
use std::path::Path;

use anyhow::{Result, bail};
use rusqlite::{OptionalExtension, params};
use sha2::{Digest, Sha256};

use crate::coverage::{CoverageItemListQuery, list_coverage_items};
use crate::db::{open_existing_project, project_id};
use crate::design::{
    DesignRequirementListQuery, DesignRequirementRecord, list_design_requirements,
};
use crate::planning::{TaskListQuery, list_tasks};
use crate::traceability::{
    ImplementationEvidenceListQuery, ImplementationEvidenceRecord, StaleRecord,
    TaskDerivationListQuery, ValidationGateContextQuery, list_implementation_evidence,
    list_stale_records, list_task_derivations, list_validation_gate_context,
};

mod work_evidence;

pub struct ReviewContextQuery<'a> {
    pub kind: &'a str,
    pub design_version_id: Option<i64>,
    pub work_unit_id: Option<i64>,
    pub phase_id: Option<i64>,
}

pub struct ReviewContextDocument {
    pub context_ref: String,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecompositionPlanReviewTarget {
    pub plan_id: i64,
    pub revision: i64,
    pub content_identity: String,
    pub current_identity: String,
    pub projection_identity: Option<String>,
    /// The selected current design in the Plan's package lineage. This may be
    /// newer than the design revision that originally authored `plan_id`.
    pub design_version_id: i64,
    pub work_unit_id: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlanReviewOwnerState {
    NoClaim,
    PendingAdjudication,
    AcceptedClean,
    RecoveryRequired,
    FreshReviewRequired,
    RaceConflict,
}

impl std::fmt::Display for PlanReviewOwnerState {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::NoClaim => "no_claim",
            Self::PendingAdjudication => "pending_adjudication",
            Self::AcceptedClean => "accepted_clean",
            Self::RecoveryRequired => "recovery_required",
            Self::FreshReviewRequired => "fresh_review_required",
            Self::RaceConflict => "race_conflict",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlanReviewOwnerResolution {
    pub state: PlanReviewOwnerState,
    pub observed_handle: String,
    pub review_plan_id: Option<i64>,
    pub review_run_id: Option<i64>,
    pub context_ref: String,
    pub actions: Vec<String>,
}

pub(crate) fn decomposition_plan_review_context_ref(
    target: &DecompositionPlanReviewTarget,
) -> String {
    format!(
        "review-context:design-task-decomposition:design={}:work={}:plan={}:revision={}:content={}{}",
        target.design_version_id,
        target.work_unit_id,
        target.plan_id,
        target.revision,
        target.content_identity,
        target
            .projection_identity
            .as_ref()
            .map(|identity| format!(":projection={identity}"))
            .unwrap_or_default()
    )
}

pub(crate) fn decomposition_plan_id_from_review_context_ref(context_ref: &str) -> Option<i64> {
    context_ref
        .strip_prefix("review-context:design-task-decomposition:")?
        .split(':')
        .find_map(|component| component.strip_prefix("plan="))?
        .parse::<i64>()
        .ok()
        .filter(|plan_id| *plan_id > 0)
}

pub(crate) fn current_decomposition_plan_review_target(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_version_id: i64,
    work_unit_id: i64,
) -> Result<Option<DecompositionPlanReviewTarget>> {
    require_current_decomposition_storage(conn)?;
    let stored = conn
        .query_row(
            r#"
        select plan.id, plan.revision, plan.content_identity
        from decomposition_plans plan
        join design_versions plan_version on plan_version.id=plan.design_version_id
        join design_versions selected_version on selected_version.id=?2
        where plan.project_id=?1 and selected_version.project_id=plan.project_id
          and plan_version.design_package_id=selected_version.design_package_id
          and plan.work_unit_id=?3
          and plan.status!='superseded'
        order by plan.revision desc, plan.id desc limit 1
        "#,
            params![project_id, design_version_id, work_unit_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()
        .map_err(anyhow::Error::from)?;
    let Some((plan_id, revision, content_identity)) = stored else {
        return Ok(None);
    };
    let projection_identity = crate::decomposition::decomposition_review_projection_identity(
        conn,
        plan_id,
        design_version_id,
    )?;
    Ok(Some(DecompositionPlanReviewTarget {
        plan_id,
        revision,
        current_identity: content_identity.clone(),
        content_identity,
        projection_identity,
        design_version_id,
        work_unit_id,
    }))
}

pub(crate) fn resolve_decomposition_plan_review_owner(
    conn: &rusqlite::Connection,
    project_id: i64,
    target: &DecompositionPlanReviewTarget,
) -> Result<PlanReviewOwnerResolution> {
    require_current_decomposition_storage(conn)?;
    validate_decomposition_review_target(target)?;
    let context_ref = decomposition_plan_review_context_ref(target);
    let stored: Option<(i64, i64, String)> = conn
        .query_row(
            r#"
            select plan.id,plan.revision,plan.content_identity
            from decomposition_plans plan
            join design_versions plan_version on plan_version.id=plan.design_version_id
            join design_versions selected_version on selected_version.id=?3
            where plan.project_id=?1 and plan.id=?2 and plan.work_unit_id=?4
              and selected_version.project_id=plan.project_id
              and plan_version.design_package_id=selected_version.design_package_id
              and plan.status!='superseded'
            "#,
            params![
                project_id,
                target.plan_id,
                target.design_version_id,
                target.work_unit_id
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let projection_matches = if stored.is_some() {
        crate::decomposition::decomposition_review_projection_identity(
            conn,
            target.plan_id,
            target.design_version_id,
        )? == target.projection_identity
    } else {
        false
    };
    if !projection_matches
        || stored.as_ref().is_none_or(|(_, revision, content)| {
            *revision != target.revision || content != &target.content_identity
        })
    {
        return Ok(plan_review_resolution(
            target,
            PlanReviewOwnerState::RaceConflict,
            None,
            None,
            context_ref,
            vec![format!(
                "agent-workbench decomposition show --design-version {} --work {}",
                target.design_version_id, target.work_unit_id
            )],
            "race",
        ));
    }

    let review_plan: Option<(i64, String, i64, i64)> = conn
        .query_row(
            r#"
            select p.id,p.status,p.fresh_review_after_run_id,
                   policy.required_consecutive_clean_fresh_runs
            from review_plans p
            join review_policies policy on policy.id=p.review_policy_id
            where p.project_id=?1 and p.design_version_id=?2 and p.work_unit_id=?3
              and p.stage='implementation-ready'
              and p.review_type='design_task_decomposition'
              and p.required=1 and p.status not in ('not_required','accepted_exception')
            order by p.id desc limit 1
            "#,
            params![project_id, target.design_version_id, target.work_unit_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    let Some((review_plan_id, plan_status, fresh_after, required_fresh)) = review_plan else {
        let foreign_claim = has_foreign_decomposition_review_claim(conn, project_id, target)?;
        return Ok(plan_review_resolution(
            target,
            if foreign_claim {
                PlanReviewOwnerState::FreshReviewRequired
            } else {
                PlanReviewOwnerState::NoClaim
            },
            None,
            None,
            context_ref,
            vec![format!(
                "agent-workbench review plan add --work-unit {} --type design_task_decomposition --stage implementation-ready --design-version {}",
                target.work_unit_id, target.design_version_id
            )],
            "no-plan",
        ));
    };

    let latest_exact: Option<(i64, i64, i64, String, Option<String>, bool)> = conn
        .query_row(
            r#"
            select r.id,r.clean_run,r.new_findings_count,r.review_provenance,
                   r.review_provenance_ref,
                   exists(select 1 from review_agent_invocations i where i.review_run_id=r.id and coalesce(i.external_agent_id,'')!='')
            from review_runs r
            where r.project_id=?1 and r.review_plan_id=?2 and r.target_ref=?3
              and r.run_type='fresh' and r.run_purpose='new_unbiased_review'
              and r.status='completed' and r.id>?4
            order by r.id desc limit 1
            "#,
            params![project_id, review_plan_id, context_ref, fresh_after],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get::<_, i64>(5)? == 1,
                ))
            },
        )
        .optional()?;
    let Some((run_id, clean, new_findings, provenance, provenance_ref, external_agent)) =
        latest_exact
    else {
        let active_invocation: Option<(i64, String)> = conn
            .query_row(
                "select id,status from review_agent_invocations where project_id=?1 and review_plan_id=?2 and target_context=?3 and purpose='new_unbiased_review' and status in ('requested','running') order by id desc limit 1",
                params![project_id, review_plan_id, context_ref],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((invocation_id, status)) = active_invocation {
            let actions = if status == "requested" {
                vec![format!(
                    "agent-workbench review invocation start {invocation_id} --expected-current requested --idempotency-key plan-review-start-{invocation_id}"
                )]
            } else {
                vec![
                    format!(
                        "agent-workbench review invocation complete {invocation_id} --claim clean --summary \"exact Decomposition Plan review is clean\" --expected-current running --idempotency-key plan-review-clean-{invocation_id}"
                    ),
                    format!(
                        "agent-workbench review invocation complete {invocation_id} --claim inconclusive --summary \"exact Decomposition Plan review is inconclusive\" --expected-current running --idempotency-key plan-review-inconclusive-{invocation_id}"
                    ),
                ]
            };
            return Ok(plan_review_resolution(
                target,
                PlanReviewOwnerState::NoClaim,
                Some(review_plan_id),
                None,
                context_ref,
                actions,
                &status,
            ));
        }
        let has_same_plan_stale: bool = conn.query_row(
            "select exists(select 1 from review_runs where project_id=?1 and review_plan_id=?2 and run_type='fresh' and status='completed')",
            params![project_id, review_plan_id],
            |row| row.get(0),
        )?;
        let has_foreign_or_stale = has_same_plan_stale
            || has_foreign_decomposition_review_claim(conn, project_id, target)?;
        let state = if plan_status == "blocked" {
            PlanReviewOwnerState::RecoveryRequired
        } else if has_foreign_or_stale {
            PlanReviewOwnerState::FreshReviewRequired
        } else {
            PlanReviewOwnerState::NoClaim
        };
        let actions = if plan_status == "blocked" {
            vec![format!(
                "agent-workbench review run list --plan {review_plan_id}"
            )]
        } else if plan_status == "open" {
            fresh_review_actions(target, review_plan_id, &context_ref)
        } else {
            vec![format!(
                "agent-workbench review plan add --work-unit {} --type design_task_decomposition --stage implementation-ready --design-version {}",
                target.work_unit_id, target.design_version_id
            )]
        };
        return Ok(plan_review_resolution(
            target,
            state,
            Some(review_plan_id),
            None,
            context_ref,
            actions,
            &plan_status,
        ));
    };

    let decision: Option<(String, String)> = conn
        .query_row(
            r#"
            select d.value,o.decision_handle
            from review_adjudication_decisions d
            join owner_decisions o on o.id=d.owner_decision_id
            where d.project_id=?1 and d.review_run_id=?2
              and not exists(select 1 from review_adjudication_decisions newer where newer.predecessor_id=d.id)
            order by d.id desc limit 1
            "#,
            params![project_id, run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((decision, decision_handle)) = decision else {
        return Ok(plan_review_resolution(
            target,
            PlanReviewOwnerState::PendingAdjudication,
            Some(review_plan_id),
            Some(run_id),
            context_ref,
            vec![
                format!(
                    "agent-workbench review adjudicate {run_id} --decision accepted --reason \"accepted exact Decomposition Plan review claim\" --expected-current pending"
                ),
                format!(
                    "agent-workbench review adjudicate {run_id} --decision rejected --reason \"rejected exact Decomposition Plan review claim\" --expected-current pending"
                ),
                format!(
                    "agent-workbench review adjudicate {run_id} --decision needs_evidence --reason \"exact Decomposition Plan review claim needs evidence\" --expected-current pending"
                ),
            ],
            "pending",
        ));
    };
    let trusted = match provenance.as_str() {
        "external_agent" => {
            external_agent
                && provenance_ref
                    .as_deref()
                    .is_some_and(|v| !v.trim().is_empty())
        }
        "human_review" => provenance_ref
            .as_deref()
            .is_some_and(|v| !v.trim().is_empty()),
        _ => false,
    };
    let open_findings: bool = conn.query_row(
        r#"
        select exists(
          select 1 from findings f
          join review_runs finding_run on finding_run.id=f.review_run_id
          where f.project_id=?1 and finding_run.review_plan_id=?2
            and f.status='open'
            and f.classification in ('unclassified','valid','design_conflict','needs_evidence')
            and not exists(
              select 1 from acceptance_records accepted
              where accepted.target_type='finding' and accepted.finding_id=f.id
                and accepted.status='approved'
                and accepted.acceptance_type in ('accepted_out_of_scope','explicit_exception','classified_failure')
            )
        )
        "#,
        params![project_id, review_plan_id],
        |row| row.get(0),
    )?;
    let accepted_clean_claims: i64 = conn.query_row(
        r#"
        select count(*) from review_runs accepted_run
        where accepted_run.project_id=?1 and accepted_run.review_plan_id=?2
          and accepted_run.target_ref=?3 and accepted_run.id>?4
          and accepted_run.run_type='fresh'
          and accepted_run.run_purpose='new_unbiased_review'
          and accepted_run.status='completed' and accepted_run.clean_run=1
          and accepted_run.new_findings_count=0
          and exists(
            select 1 from review_adjudication_decisions accepted_decision
            where accepted_decision.review_run_id=accepted_run.id
              and accepted_decision.value='accepted'
              and not exists(
                select 1 from review_adjudication_decisions newer
                where newer.predecessor_id=accepted_decision.id
              )
          )
          and (
            (accepted_run.review_provenance='human_review'
             and coalesce(accepted_run.review_provenance_ref,'')!='')
            or
            (accepted_run.review_provenance='external_agent'
             and coalesce(accepted_run.review_provenance_ref,'')!=''
             and exists(
               select 1 from review_agent_invocations accepted_invocation
               where accepted_invocation.review_run_id=accepted_run.id
                 and coalesce(accepted_invocation.external_agent_id,'')!=''
             ))
          )
        "#,
        params![project_id, review_plan_id, context_ref, fresh_after],
        |row| row.get(0),
    )?;
    let accepted_clean = decision == "accepted"
        && clean == 1
        && new_findings == 0
        && trusted
        && !open_findings
        && accepted_clean_claims >= required_fresh
        && plan_status == "clean";
    let (state, actions) = if accepted_clean {
        (PlanReviewOwnerState::AcceptedClean, Vec::new())
    } else {
        (
            PlanReviewOwnerState::RecoveryRequired,
            vec![format!(
                "agent-workbench review run list --plan {review_plan_id}"
            )],
        )
    };
    Ok(plan_review_resolution(
        target,
        state,
        Some(review_plan_id),
        Some(run_id),
        context_ref,
        actions,
        &decision_handle,
    ))
}

fn require_current_decomposition_storage(conn: &rusqlite::Connection) -> Result<()> {
    if crate::db::project_requires_update(conn)? {
        bail!("project state requires an explicit update; next: agent-workbench update inspect");
    }
    Ok(())
}

pub(crate) fn require_accepted_decomposition_plan_review(
    conn: &rusqlite::Connection,
    project_id: i64,
    target: &DecompositionPlanReviewTarget,
    expected_observed_handle: &str,
) -> Result<()> {
    let resolution = resolve_decomposition_plan_review_owner(conn, project_id, target)?;
    if resolution.observed_handle != expected_observed_handle {
        bail!("Decomposition Plan review state changed; resolve the current Plan again");
    }
    if resolution.state != PlanReviewOwnerState::AcceptedClean {
        bail!("the exact current Decomposition Plan does not have an accepted clean review claim");
    }
    Ok(())
}

fn validate_decomposition_review_target(target: &DecompositionPlanReviewTarget) -> Result<()> {
    if target.plan_id <= 0
        || target.revision <= 0
        || target.design_version_id <= 0
        || target.work_unit_id <= 0
        || target.content_identity.len() != 64
        || target
            .projection_identity
            .as_ref()
            .is_some_and(|identity| identity.len() != 64)
        || target.current_identity.trim().is_empty()
    {
        bail!("exact Decomposition Plan review target is invalid");
    }
    Ok(())
}

fn has_foreign_decomposition_review_claim(
    conn: &rusqlite::Connection,
    project_id: i64,
    target: &DecompositionPlanReviewTarget,
) -> Result<bool> {
    conn.query_row(
        r#"
        select exists(
          select 1 from review_runs prior_run
          join review_plans prior_plan on prior_plan.id=prior_run.review_plan_id
          join design_versions prior_design on prior_design.id=prior_plan.design_version_id
          join design_versions selected_design on selected_design.id=?3
          where prior_run.project_id=?1 and prior_plan.work_unit_id=?2
            and prior_plan.review_type='design_task_decomposition'
            and prior_plan.stage='implementation-ready'
            and prior_design.design_package_id=selected_design.design_package_id
            and prior_plan.design_version_id!=selected_design.id
            and prior_run.run_type='fresh' and prior_run.status='completed'
        )
        "#,
        params![project_id, target.work_unit_id, target.design_version_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn fresh_review_actions(
    target: &DecompositionPlanReviewTarget,
    review_plan_id: i64,
    _context_ref: &str,
) -> Vec<String> {
    vec![
        format!(
            "agent-workbench review-context design-task-decomposition --design-version {} --work-unit {}",
            target.design_version_id, target.work_unit_id
        ),
        format!("agent-workbench review plan context {review_plan_id}"),
    ]
}

fn plan_review_resolution(
    target: &DecompositionPlanReviewTarget,
    state: PlanReviewOwnerState,
    review_plan_id: Option<i64>,
    review_run_id: Option<i64>,
    context_ref: String,
    actions: Vec<String>,
    boundary: &str,
) -> PlanReviewOwnerResolution {
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/decomposition-plan-review-owner/v1\0");
    for value in [
        target.plan_id.to_string(),
        target.revision.to_string(),
        target.content_identity.clone(),
        target.current_identity.clone(),
        target.design_version_id.to_string(),
        target.work_unit_id.to_string(),
        review_plan_id.map_or_else(|| "-".to_string(), |id| id.to_string()),
        review_run_id.map_or_else(|| "-".to_string(), |id| id.to_string()),
        state.to_string(),
        boundary.to_string(),
    ] {
        hasher.update(value.as_bytes());
        hasher.update(b"\0");
    }
    PlanReviewOwnerResolution {
        state,
        observed_handle: format!("plan_review_{:x}", hasher.finalize()),
        review_plan_id,
        review_run_id,
        context_ref,
        actions,
    }
}

pub fn render_finding_fix_context(
    root: &Path,
    finding_id: i64,
    closure_id: i64,
    attempt_id: i64,
) -> Result<ReviewContextDocument> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let context_ref = crate::review::finding_fix_context_ref(finding_id, closure_id, attempt_id);
    let recovery_plan = conn
        .query_row(
            r#"
            select recovery.successor_design_version_id,source_plan.work_unit_id,
                   source_plan.review_type,source_plan.stage,source_plan.scope,
                   source_plan.review_policy_id
            from finding_design_recoveries recovery
            join findings finding on finding.id=recovery.finding_id
            join review_runs source_run on source_run.id=finding.review_run_id
            join review_plans source_plan on source_plan.id=source_run.review_plan_id
            where recovery.project_id=?1 and recovery.finding_id=?2
              and recovery.successor_closure_id=?3
              and recovery.successor_attempt_id=?4
            "#,
            params![project_id, finding_id, closure_id, attempt_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, i64>(5)?,
                ))
            },
        )
        .optional()?;
    if let Some((design, work, review_type, stage, scope, policy)) = recovery_plan {
        let exact_plan_exists: bool = conn.query_row(
            "select exists(select 1 from review_plans where project_id=?1 and work_unit_id=?2 and design_version_id=?3 and review_type=?4 and stage=?5 and coalesce(scope,'')=coalesce(?6,'') and required=1 and status in ('open','clean'))",
            params![project_id, work, design, review_type, stage, scope.as_deref()],
            |row| row.get(0),
        )?;
        if !exact_plan_exists {
            let mut action = format!(
                "agent-workbench review plan add --work-unit {work} --type {review_type} --stage {stage} --design-version {design} --policy {policy}"
            );
            if let Some(scope) = scope {
                action.push_str(" --scope \"");
                action.push_str(&scope.replace('\\', "\\\\").replace('"', "\\\""));
                action.push('"');
            }
            bail!("recovered finding requires an exact successor review plan; next: {action}");
        }
    }
    let text = conn
        .query_row(
            r#"
            select p.id, p.review_type, p.stage, f.description, f.severity,
                   c.design_invariant, c.affected_surfaces, c.fix_plan,
                   c.verification_plan, c.tests_or_gates, a.attempt_number,
                   a.implementation_evidence, a.tests_or_gates,
                   a.closed_by_commit, a.review_run_high_watermark,
                   (select group_concat(
                      'requirement='||coalesce(target.design_requirement_id,'-')||
                      ',task='||coalesce(target.task_id,'-'),
                      ';'
                    ) from finding_targets target
                    where target.finding_id=f.id)
            from closure_attempts a
            join closures c on c.id = a.closure_id
            join findings f on f.id = c.finding_id
            join review_runs r on r.id = f.review_run_id
            join review_plans source_plan on source_plan.id = r.review_plan_id
            join review_plans p on p.id = coalesce(
              (select candidate.id from review_plans candidate
               where candidate.project_id = source_plan.project_id
                 and candidate.work_unit_id = source_plan.work_unit_id
                 and candidate.review_type = source_plan.review_type
                 and candidate.stage = source_plan.stage
                 and candidate.design_version_id is coalesce(
                   (select recovery.successor_design_version_id
                    from finding_design_recoveries recovery
                    where recovery.project_id=f.project_id
                      and recovery.successor_closure_id=c.id),
                   source_plan.design_version_id
                 )
                 and coalesce(candidate.scope, '') = coalesce(source_plan.scope, '')
                 and candidate.required = 1
                 and candidate.status in ('open','clean')
               order by candidate.id desc limit 1),
              source_plan.id
            )
            where f.id = ?1 and c.id = ?2 and a.id = ?3
              and f.project_id = ?4 and c.project_id = ?4 and a.project_id = ?4
            "#,
            rusqlite::params![finding_id, closure_id, attempt_id, project_id],
            |row| {
                Ok(format!(
                    "review_context: finding-fix\ncontext_ref: {context_ref}\nfinding_id: {finding_id}\nclosure_id: {closure_id}\nattempt_id: {attempt_id}\nreview_plan_id: {}\nreview_type: {}\nstage: {}\nseverity: {}\ndescription: {}\ntargets: {}\ninvariant: {}\naffected_surfaces: {}\nfix_plan: {}\nverification_plan: {}\ncontract_tests_or_gates: {}\nattempt_number: {}\nimplementation_evidence: {}\nattempt_tests_or_gates: {}\ncommit: {}\nreview_run_high_watermark: {}\n",
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(15)?.unwrap_or_else(|| "-".to_string()),
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?.unwrap_or_else(|| "-".to_string()),
                    row.get::<_, Option<String>>(7)?.unwrap_or_else(|| "-".to_string()),
                    row.get::<_, Option<String>>(8)?.unwrap_or_else(|| "-".to_string()),
                    row.get::<_, Option<String>>(9)?.unwrap_or_else(|| "-".to_string()),
                    row.get::<_, i64>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, Option<String>>(13)?.unwrap_or_else(|| "-".to_string()),
                    row.get::<_, i64>(14)?,
                ))
            },
        )
        .map_err(anyhow::Error::from)?;
    let text = format!("classification: project-internal\n{text}");
    Ok(ReviewContextDocument { context_ref, text })
}

pub fn review_context_ref(
    kind: &str,
    design_version_id: Option<i64>,
    work_unit_id: Option<i64>,
) -> String {
    review_context_ref_with_phase(kind, design_version_id, work_unit_id, None)
}

pub fn review_context_ref_with_phase(
    kind: &str,
    design_version_id: Option<i64>,
    work_unit_id: Option<i64>,
    phase_id: Option<i64>,
) -> String {
    let design = design_version_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "-".to_string());
    let work = work_unit_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "-".to_string());
    match phase_id {
        Some(phase_id) => {
            format!("review-context:{kind}:design={design}:work={work}:phase={phase_id}")
        }
        None => format!("review-context:{kind}:design={design}:work={work}"),
    }
}

pub(crate) fn current_review_context_ref(
    conn: &rusqlite::Connection,
    project_id: i64,
    kind: &str,
    design_version_id: Option<i64>,
    work_unit_id: Option<i64>,
    phase_id: Option<i64>,
) -> Result<String> {
    if kind == "design-task-decomposition"
        && phase_id.is_none()
        && let (Some(design), Some(work)) = (design_version_id, work_unit_id)
        && let Some(target) =
            current_decomposition_plan_review_target(conn, project_id, design, work)?
    {
        return Ok(decomposition_plan_review_context_ref(&target));
    }
    Ok(review_context_ref_with_phase(
        kind,
        design_version_id,
        work_unit_id,
        phase_id,
    ))
}

pub fn render_review_context(
    root: &Path,
    query: ReviewContextQuery<'_>,
) -> Result<ReviewContextDocument> {
    let exact_plan = if query.kind == "design-task-decomposition" {
        match (query.design_version_id, query.work_unit_id, query.phase_id) {
            (Some(design), Some(work), None) => {
                let conn = open_existing_project(root)?;
                let project = project_id(&conn)?;
                current_decomposition_plan_review_target(&conn, project, design, work)?
            }
            _ => None,
        }
    } else {
        None
    };
    let context_ref = exact_plan.as_ref().map_or_else(
        || {
            review_context_ref_with_phase(
                query.kind,
                query.design_version_id,
                query.work_unit_id,
                query.phase_id,
            )
        },
        decomposition_plan_review_context_ref,
    );
    let mut output = String::new();
    writeln!(output, "classification: project-internal")?;
    writeln!(output, "review_context: {}", query.kind)?;
    writeln!(output, "context_ref: {context_ref}")?;
    if let Some(target) = exact_plan.as_ref() {
        writeln!(output, "decomposition_plan_id: {}", target.plan_id)?;
        writeln!(output, "decomposition_plan_revision: {}", target.revision)?;
        writeln!(
            output,
            "decomposition_plan_content_identity: {}",
            target.content_identity
        )?;
        let conn = open_existing_project(root)?;
        let document: String = conn.query_row(
            "select document_content from decomposition_plans where id=?1 and content_identity=?2",
            params![target.plan_id, target.content_identity],
            |row| row.get(0),
        )?;
        writeln!(output, "decomposition_plan_document:")?;
        for line in document.lines() {
            writeln!(output, "  {line}")?;
        }
    }
    if let Some(phase_id) = query.phase_id {
        render_phase_header(root, phase_id, query.work_unit_id, &mut output)?;
    }

    if let Some(design_version_id) = query.design_version_id {
        writeln!(output, "design_version_id: {design_version_id}")?;
        render_design_context(
            root,
            query.kind,
            design_version_id,
            query.work_unit_id,
            query.phase_id,
            &mut output,
        )?;
    }
    if let Some(work_unit_id) = query.work_unit_id {
        writeln!(output, "work_unit_id: {work_unit_id}")?;
        render_work_context(root, work_unit_id, query.phase_id, &mut output)?;
    }
    render_stale_context(root, query.work_unit_id, query.phase_id, &mut output)?;

    Ok(ReviewContextDocument {
        context_ref,
        text: output,
    })
}

pub(crate) fn review_plan_has_clean_context_run(
    conn: &rusqlite::Connection,
    review_plan_id: i64,
    kind: &str,
    design_version_id: Option<i64>,
    work_unit_id: Option<i64>,
) -> Result<bool> {
    let mut statement = conn.prepare(
        r#"
        select phase_id
        from work_phase_review_targets
        where review_plan_id = ?1
        order by phase_id
        "#,
    )?;
    let rows = statement.query_map([review_plan_id], |row| row.get::<_, i64>(0))?;
    let mut phase_ids = Vec::new();
    for row in rows {
        phase_ids.push(row?);
    }
    if phase_ids.is_empty() {
        let context_ref = if kind == "design-task-decomposition" {
            match (design_version_id, work_unit_id) {
                (Some(design), Some(work)) => {
                    current_decomposition_plan_review_target(conn, project_id(conn)?, design, work)?
                        .as_ref()
                        .map_or_else(
                            || review_context_ref(kind, design_version_id, work_unit_id),
                            decomposition_plan_review_context_ref,
                        )
                }
                _ => review_context_ref(kind, design_version_id, work_unit_id),
            }
        } else {
            review_context_ref(kind, design_version_id, work_unit_id)
        };
        return review_plan_has_clean_target_ref(conn, review_plan_id, &context_ref);
    }
    for phase_id in phase_ids {
        let context_ref =
            review_context_ref_with_phase(kind, design_version_id, work_unit_id, Some(phase_id));
        if !review_plan_has_clean_target_ref(conn, review_plan_id, &context_ref)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn review_plan_has_clean_target_ref(
    conn: &rusqlite::Connection,
    review_plan_id: i64,
    context_ref: &str,
) -> Result<bool> {
    conn.query_row(
        r#"
        select exists (
            select 1
            from review_runs r
            join review_plans p on p.id=r.review_plan_id
            where r.review_plan_id = ?1
              and r.target_ref = ?2
              and r.run_type = 'fresh'
              and r.run_purpose = 'new_unbiased_review'
              and r.id > p.fresh_review_after_run_id
              and r.id = (
                  select max(latest.id)
                  from review_runs latest
                  where latest.review_plan_id=r.review_plan_id
                    and latest.target_ref=r.target_ref
                    and latest.run_type='fresh'
                    and latest.run_purpose='new_unbiased_review'
                    and latest.status='completed'
                    and latest.id > p.fresh_review_after_run_id
              )
              and r.clean_run = 1
              and r.status = 'completed'
              and exists (
                  select 1 from review_adjudication_decisions d
                  where d.review_run_id=r.id and d.value='accepted'
                    and not exists (
                        select 1 from review_adjudication_decisions newer
                        where newer.predecessor_id=d.id
                    )
              )
              and (
                  (
                      r.review_provenance = 'external_agent'
                      and coalesce(r.review_provenance_ref, '') != ''
                      and exists (
                          select 1
                          from review_agent_invocations i
                          where i.review_run_id = r.id
                            and i.external_agent_id is not null
                            and i.external_agent_id != ''
                      )
                  )
                  or (
                      r.review_provenance = 'human_review'
                      and coalesce(r.review_provenance_ref, '') != ''
                  )
              )
        )
        "#,
        rusqlite::params![review_plan_id, context_ref],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(crate) fn required_plans_missing_context_count(
    conn: &rusqlite::Connection,
    project_id: i64,
    stage: &str,
    review_type: &str,
    design_version_id: Option<i64>,
    work_unit_id: Option<i64>,
    kind: &str,
) -> Result<i64> {
    let mut stmt = conn.prepare(
        r#"
        select id, design_version_id, work_unit_id
        from review_plans rp
        where rp.project_id = ?1
          and rp.stage = ?2
          and rp.review_type = ?3
          and rp.required = 1
          and (?4 is null or rp.design_version_id = ?4)
          and (?5 is null or rp.work_unit_id = ?5)
          and (
            ?6 != 'design-task-decomposition'
            or rp.status not in ('accepted_exception','not_required')
            or not exists(
              select 1 from review_plans successor
              where successor.project_id=rp.project_id
                and successor.work_unit_id=rp.work_unit_id
                and successor.design_version_id is rp.design_version_id
                and successor.stage=rp.stage
                and successor.review_type=rp.review_type
                and successor.required=1
                and successor.status not in ('accepted_exception','not_required')
                and successor.id!=rp.id
            )
          )
          and (
            ?6 = 'design-task-decomposition'
            or not exists (
              select 1
              from acceptance_records ar
              where ar.target_type = 'review_plan'
                and ar.review_plan_id = rp.id
                and ar.status = 'approved'
                and ar.acceptance_type in ('explicit_exception', 'stale_accepted')
            )
          )
        "#,
    )?;
    let rows = stmt.query_map(
        rusqlite::params![
            project_id,
            stage,
            review_type,
            design_version_id,
            work_unit_id,
            kind
        ],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        },
    )?;
    let mut missing = 0;
    for row in rows {
        let (review_plan_id, plan_design_version_id, plan_work_unit_id) = row?;
        if !review_plan_has_clean_context_run(
            conn,
            review_plan_id,
            kind,
            plan_design_version_id,
            plan_work_unit_id,
        )? {
            missing += 1;
        }
    }
    Ok(missing)
}

fn render_design_context(
    root: &Path,
    kind: &str,
    design_version_id: i64,
    work_unit_id: Option<i64>,
    phase_id: Option<i64>,
    output: &mut String,
) -> Result<()> {
    let phase_tasks = match phase_id {
        Some(phase_id) => Some(phase_task_set(root, phase_id)?),
        None => None,
    };
    let requirements = if let Some(phase_id) = phase_id {
        list_phase_context_requirements(root, design_version_id, phase_id)?
    } else {
        list_context_requirements(
            root,
            design_version_id,
            work_unit_id.filter(|_| review_context_kind_is_work_scoped(kind)),
        )?
    };
    writeln!(output, "requirements:")?;
    if requirements.is_empty() {
        writeln!(output, "- none")?;
    }
    for requirement in requirements {
        let validation = requirement.validation_expectation.as_deref().unwrap_or("-");
        writeln!(
            output,
            "- {} [{}:{} validation={}] {}",
            requirement.requirement_key,
            requirement.priority,
            requirement.status,
            validation,
            requirement.requirement_text.lines().next().unwrap_or("")
        )?;
    }

    let mut derivations = list_task_derivations(
        root,
        TaskDerivationListQuery {
            design_version_id,
            work_unit_id,
        },
    )?;
    if let Some(tasks) = &phase_tasks {
        derivations.retain(|record| tasks.contains(&record.task_id));
    }
    writeln!(output, "task_derivations:")?;
    if derivations.is_empty() {
        writeln!(output, "- none")?;
    }
    for derivation in derivations {
        writeln!(
            output,
            "- requirement={} task={} [{}] {}",
            derivation.requirement_key,
            derivation.task_id,
            derivation.status,
            derivation.task_title
        )?;
    }

    let mut gates = list_validation_gate_context(
        root,
        ValidationGateContextQuery {
            design_version_id,
            work_unit_id,
        },
    )?;
    if let Some(tasks) = &phase_tasks {
        gates.retain(|record| {
            record
                .task_id
                .is_some_and(|task_id| tasks.contains(&task_id))
        });
    }
    writeln!(output, "selected_validation_gates:")?;
    if gates.is_empty() {
        writeln!(output, "- none")?;
    }
    for gate in gates {
        let task = gate
            .task_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string());
        let run = gate
            .latest_run_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string());
        let command_usage = gate
            .latest_command_usage_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string());
        let snapshot = gate
            .latest_repository_snapshot_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string());
        let result = gate.latest_result.as_deref().unwrap_or("-");
        let artifact = gate.latest_artifact_path.as_deref().unwrap_or("-");
        let notes = gate.latest_notes.as_deref().unwrap_or("-");
        writeln!(
            output,
            "- {} requirement={} task={} status={} latest_run={} latest_result={} command_usage={} snapshot={} artifact={} notes={}",
            gate.gate_key,
            gate.requirement_key,
            task,
            gate.status,
            run,
            result,
            command_usage,
            snapshot,
            artifact,
            notes
        )?;
    }

    let mut evidence = list_implementation_evidence(
        root,
        ImplementationEvidenceListQuery {
            task_id: None,
            design_version_id: Some(design_version_id),
            work_unit_id,
            evidence_type: None,
        },
    )?;
    if let Some(tasks) = &phase_tasks {
        evidence.retain(|record| {
            record
                .task_id
                .is_some_and(|task_id| tasks.contains(&task_id))
        });
    }
    writeln!(output, "implementation_evidence:")?;
    if evidence.is_empty() {
        writeln!(output, "- none")?;
    }
    for item in evidence {
        let requirement = item.requirement_key.as_deref().unwrap_or("-");
        let task = item
            .task_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string());
        writeln!(
            output,
            "- {} type={} requirement={} task={} {}",
            item.id,
            item.evidence_type,
            requirement,
            task,
            evidence_detail(&item)
        )?;
    }

    let mut coverage = list_coverage_items(
        root,
        CoverageItemListQuery {
            design_version_id,
            status: None,
            work_unit_id,
        },
    )?;
    coverage.retain(|record| record.status != "stale");
    if let Some(tasks) = &phase_tasks {
        coverage.retain(|record| match record.task_id {
            Some(task_id) => tasks.contains(&task_id),
            None => true,
        });
    }
    writeln!(output, "coverage_items:")?;
    if coverage.is_empty() {
        writeln!(output, "- none")?;
    }
    for item in &coverage {
        let task = item
            .task_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string());
        let tests = item.tests_or_gates.as_deref().unwrap_or("-");
        let gap = item.missing_or_unverified.as_deref().unwrap_or("-");
        writeln!(
            output,
            "- {} coverage={} task={} tests={} gap={} {}",
            item.requirement_key,
            item.status,
            task,
            tests,
            gap,
            item.requirement.lines().next().unwrap_or("")
        )?;
    }

    writeln!(output, "known_gaps:")?;
    let mut printed_gap = false;
    for item in coverage.iter().filter(|item| {
        matches!(
            item.status.as_str(),
            "partial" | "missing_required_surface" | "design_conflict" | "needs_evidence"
        ) || item.missing_or_unverified.is_some()
    }) {
        printed_gap = true;
        let gap = item
            .missing_or_unverified
            .as_deref()
            .unwrap_or("coverage incomplete");
        writeln!(output, "- coverage:{} [{}] {}", item.id, item.status, gap)?;
    }
    if !printed_gap {
        writeln!(output, "- none")?;
    }

    Ok(())
}

fn review_context_kind_is_work_scoped(kind: &str) -> bool {
    matches!(kind, "design-implementation-diff" | "implementation-review")
}

fn render_work_context(
    root: &Path,
    work_unit_id: i64,
    phase_id: Option<i64>,
    output: &mut String,
) -> Result<()> {
    let mut tasks = list_tasks(
        root,
        TaskListQuery {
            status: None,
            work_unit_id: Some(work_unit_id),
        },
    )?;
    if let Some(phase_id) = phase_id {
        let phase_tasks = phase_task_set(root, phase_id)?;
        tasks.retain(|task| phase_tasks.contains(&task.id));
    }
    writeln!(output, "tasks:")?;
    if tasks.is_empty() {
        writeln!(output, "- none")?;
    }
    for task in tasks {
        writeln!(
            output,
            "- {} [{}:{}] {}",
            task.id, task.priority, task.status, task.title
        )?;
    }
    work_evidence::render(root, work_unit_id, output)?;
    Ok(())
}

fn render_stale_context(
    root: &Path,
    work_unit_id: Option<i64>,
    phase_id: Option<i64>,
    output: &mut String,
) -> Result<()> {
    let stale = match work_unit_id {
        Some(_) if phase_id.is_some() => list_phase_stale_records(root, phase_id.unwrap())?,
        Some(work_unit_id) => list_work_stale_records(root, work_unit_id)?,
        None => list_stale_records(root)?,
    };
    writeln!(output, "stale_records:")?;
    if stale.is_empty() {
        writeln!(output, "- none")?;
    }
    for record in stale {
        writeln!(
            output,
            "- {}:{} {}",
            record.record_type, record.id, record.label
        )?;
    }
    Ok(())
}

fn render_phase_header(
    root: &Path,
    phase_id: i64,
    expected_work_unit_id: Option<i64>,
    output: &mut String,
) -> Result<()> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let (work_unit_id, key, title, status): (i64, String, String, String) = conn.query_row(
        r#"
            select work_unit_id, phase_key, title, status
            from work_phases
            where id = ?1 and project_id = ?2
            "#,
        rusqlite::params![phase_id, project_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    if let Some(expected_work_unit_id) = expected_work_unit_id
        && expected_work_unit_id != work_unit_id
    {
        anyhow::bail!("phase does not belong to requested work unit");
    }
    writeln!(output, "phase_id: {phase_id}")?;
    writeln!(output, "phase_key: {key}")?;
    writeln!(output, "phase_title: {title}")?;
    writeln!(output, "phase_status: {status}")?;
    Ok(())
}

fn phase_task_set(root: &Path, phase_id: i64) -> Result<HashSet<i64>> {
    let conn = open_existing_project(root)?;
    let mut stmt = conn.prepare(
        "select task_id from work_phase_task_memberships where phase_id = ?1 order by task_id",
    )?;
    let rows = stmt.query_map(rusqlite::params![phase_id], |row| row.get(0))?;
    let mut tasks = HashSet::new();
    for row in rows {
        tasks.insert(row?);
    }
    Ok(tasks)
}

fn list_phase_context_requirements(
    root: &Path,
    design_version_id: i64,
    phase_id: i64,
) -> Result<Vec<DesignRequirementRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        with relevant_requirements as (
            select distinct td.design_requirement_id as id
            from task_derivations td
            join work_phase_task_memberships m on m.task_id = td.task_id
            where m.phase_id = ?3
            union
            select distinct vg.design_requirement_id as id
            from validation_gates vg
            join work_phase_task_memberships m on m.task_id = vg.task_id
            where m.phase_id = ?3
            union
            select distinct e.design_requirement_id as id
            from implementation_evidence e
            join work_phase_task_memberships m on m.task_id = e.task_id
            where m.phase_id = ?3 and e.design_requirement_id is not null
            union
            select distinct c.design_requirement_id as id
            from coverage_items c
            join work_phase_task_memberships m on m.task_id = c.task_id
            where m.phase_id = ?3
        )
        select
            r.id, r.design_version_id, r.source_design_file_id,
            f.relative_path, r.source_section, r.requirement_key,
            r.revision, r.requirement_text, r.priority,
            r.required_surfaces, r.validation_expectation, r.status
        from design_requirements r
        join design_files f on f.id = r.source_design_file_id
        join relevant_requirements rr on rr.id = r.id
        where r.project_id = ?1
          and r.design_version_id = ?2
        order by r.requirement_key, r.id
        "#,
    )?;
    let rows = stmt.query_map(
        rusqlite::params![project_id, design_version_id, phase_id],
        |row| {
            Ok(DesignRequirementRecord {
                id: row.get(0)?,
                design_version_id: row.get(1)?,
                source_design_file_id: row.get(2)?,
                source_path: row.get(3)?,
                source_section: row.get(4)?,
                requirement_key: row.get(5)?,
                revision: row.get(6)?,
                requirement_text: row.get(7)?,
                priority: row.get(8)?,
                required_surfaces: row.get(9)?,
                validation_expectation: row.get(10)?,
                status: row.get(11)?,
            })
        },
    )?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

fn list_context_requirements(
    root: &Path,
    design_version_id: i64,
    work_unit_id: Option<i64>,
) -> Result<Vec<DesignRequirementRecord>> {
    let Some(work_unit_id) = work_unit_id else {
        return list_design_requirements(root, DesignRequirementListQuery { design_version_id });
    };
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        with relevant_requirements as (
            select distinct td.design_requirement_id as id
            from task_derivations td
            join tasks t on t.id = td.task_id
            where td.project_id = ?1
              and t.work_unit_id = ?3
            union
            select distinct vg.design_requirement_id as id
            from validation_gates vg
            left join tasks t on t.id = vg.task_id
            where vg.project_id = ?1
              and coalesce(vg.work_unit_id, t.work_unit_id) = ?3
            union
            select distinct e.design_requirement_id as id
            from implementation_evidence e
            join tasks t on t.id = e.task_id
            where e.project_id = ?1
              and e.design_requirement_id is not null
              and t.work_unit_id = ?3
            union
            select distinct c.design_requirement_id as id
            from coverage_items c
            left join tasks t on t.id = c.task_id
            where c.project_id = ?1
              and coalesce(c.work_unit_id, t.work_unit_id) = ?3
        )
        select
            r.id, r.design_version_id, r.source_design_file_id,
            f.relative_path, r.source_section, r.requirement_key,
            r.revision, r.requirement_text, r.priority,
            r.required_surfaces, r.validation_expectation, r.status
        from design_requirements r
        join design_files f on f.id = r.source_design_file_id
        join relevant_requirements rr on rr.id = r.id
        where r.project_id = ?1
          and r.design_version_id = ?2
        order by r.requirement_key, r.id
        "#,
    )?;
    let rows = stmt.query_map(
        rusqlite::params![project_id, design_version_id, work_unit_id],
        |row| {
            Ok(DesignRequirementRecord {
                id: row.get(0)?,
                design_version_id: row.get(1)?,
                source_design_file_id: row.get(2)?,
                source_path: row.get(3)?,
                source_section: row.get(4)?,
                requirement_key: row.get(5)?,
                revision: row.get(6)?,
                requirement_text: row.get(7)?,
                priority: row.get(8)?,
                required_surfaces: row.get(9)?,
                validation_expectation: row.get(10)?,
                status: row.get(11)?,
            })
        },
    )?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

fn list_phase_stale_records(root: &Path, phase_id: i64) -> Result<Vec<StaleRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut records = Vec::new();
    collect_work_stale_rows(
        &conn,
        project_id,
        phase_id,
        "task_derivation",
        r#"
        select td.id, dr.requirement_key
        from task_derivations td
        join design_requirements dr on dr.id = td.design_requirement_id
        join work_phase_task_memberships m on m.task_id = td.task_id
        where m.phase_id = ?2
          and td.project_id = ?1
          and td.status = 'stale'
        order by td.id
        "#,
        &mut records,
    )?;
    collect_work_stale_rows(
        &conn,
        project_id,
        phase_id,
        "validation_gate",
        r#"
        select vg.id, vg.gate_key
        from validation_gates vg
        join work_phase_task_memberships m on m.task_id = vg.task_id
        where m.phase_id = ?2
          and vg.project_id = ?1
          and vg.status = 'stale'
        order by vg.id
        "#,
        &mut records,
    )?;
    collect_work_stale_rows(
        &conn,
        project_id,
        phase_id,
        "coverage_item",
        r#"
        select c.id, dr.requirement_key
        from coverage_items c
        join design_requirements dr on dr.id = c.design_requirement_id
        join work_phase_task_memberships m on m.task_id = c.task_id
        where m.phase_id = ?2
          and c.project_id = ?1
          and c.status = 'stale'
          and not exists (
            select 1 from coverage_items replacement
            where replacement.project_id=c.project_id
              and replacement.design_requirement_id=c.design_requirement_id
              and replacement.task_id is c.task_id
              and replacement.work_unit_id is c.work_unit_id
              and replacement.status!='stale'
              and replacement.id>c.id
          )
        order by c.id
        "#,
        &mut records,
    )?;
    Ok(records)
}

fn list_work_stale_records(root: &Path, work_unit_id: i64) -> Result<Vec<StaleRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut records = Vec::new();
    collect_work_stale_rows(
        &conn,
        project_id,
        work_unit_id,
        "task_derivation",
        r#"
        select td.id, dr.requirement_key
        from task_derivations td
        join design_requirements dr on dr.id = td.design_requirement_id
        join tasks t on t.id = td.task_id
        where td.project_id = ?1
          and t.work_unit_id = ?2
          and td.status = 'stale'
          and not exists (
              select 1
              from acceptance_records ar
              where ar.target_type = 'stale_record'
                and ar.stale_record_type = 'task_derivation'
                and ar.stale_record_id = td.id
                and ar.acceptance_type = 'stale_accepted'
                and ar.status = 'approved'
          )
        order by td.id
        "#,
        &mut records,
    )?;
    collect_work_stale_rows(
        &conn,
        project_id,
        work_unit_id,
        "checklist",
        r#"
        select c.id, c.title
        from checklists c
        where c.project_id = ?1
          and c.work_unit_id = ?2
          and c.status = 'stale'
          and not exists (
              select 1
              from acceptance_records ar
              where ar.target_type = 'stale_record'
                and ar.stale_record_type = 'checklist'
                and ar.stale_record_id = c.id
                and ar.acceptance_type = 'stale_accepted'
                and ar.status = 'approved'
          )
        order by c.id
        "#,
        &mut records,
    )?;
    collect_work_stale_rows(
        &conn,
        project_id,
        work_unit_id,
        "validation_gate",
        r#"
        select vg.id, vg.gate_key
        from validation_gates vg
        left join tasks t on t.id = vg.task_id
        where vg.project_id = ?1
          and coalesce(vg.work_unit_id, t.work_unit_id) = ?2
          and vg.status = 'stale'
          and not exists (
              select 1
              from acceptance_records ar
              where ar.target_type = 'stale_record'
                and ar.stale_record_type = 'validation_gate'
                and ar.stale_record_id = vg.id
                and ar.acceptance_type = 'stale_accepted'
                and ar.status = 'approved'
          )
        order by vg.id
        "#,
        &mut records,
    )?;
    collect_work_stale_rows(
        &conn,
        project_id,
        work_unit_id,
        "coverage_item",
        r#"
        select c.id, dr.requirement_key
        from coverage_items c
        join design_requirements dr on dr.id = c.design_requirement_id
        left join tasks t on t.id = c.task_id
        where c.project_id = ?1
          and coalesce(c.work_unit_id, t.work_unit_id) = ?2
          and c.status = 'stale'
          and not exists (
              select 1 from coverage_items replacement
              where replacement.project_id=c.project_id
                and replacement.design_requirement_id=c.design_requirement_id
                and replacement.task_id is c.task_id
                and replacement.work_unit_id is c.work_unit_id
                and replacement.status!='stale'
                and replacement.id>c.id
          )
          and not exists (
              select 1
              from acceptance_records ar
              where ar.target_type = 'stale_record'
                and ar.stale_record_type = 'coverage_item'
                and ar.stale_record_id = c.id
                and ar.acceptance_type = 'stale_accepted'
                and ar.status = 'approved'
          )
        order by c.id
        "#,
        &mut records,
    )?;
    Ok(records)
}

fn collect_work_stale_rows(
    conn: &rusqlite::Connection,
    project_id: i64,
    work_unit_id: i64,
    record_type: &str,
    sql: &str,
    output: &mut Vec<StaleRecord>,
) -> Result<()> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params![project_id, work_unit_id], |row| {
        Ok(StaleRecord {
            record_type: record_type.to_string(),
            id: row.get(0)?,
            label: row.get(1)?,
        })
    })?;
    for row in rows {
        output.push(row?);
    }
    Ok(())
}

fn evidence_detail(record: &ImplementationEvidenceRecord) -> String {
    if let Some(commit_sha) = &record.commit_sha {
        return format!("commit={commit_sha}");
    }
    if let Some(file_path) = &record.file_path {
        let line = record
            .line_ref
            .as_ref()
            .map(|value| format!(":{value}"))
            .unwrap_or_default();
        return format!("file={file_path}{line}");
    }
    if let Some(symbol) = &record.symbol {
        return format!("symbol={symbol}");
    }
    if let Some(artifact_path) = &record.artifact_path {
        return format!("artifact={artifact_path}");
    }
    "detail=-".to_string()
}
