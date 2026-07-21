use super::*;
use crate::decomposition::PlanReviewOwnerState;

mod application;
mod dependency_evidence;
mod dependency_state;
mod reconciliation;
mod reconciliation_contract;

fn import_review_and_apply(
    root: &std::path::Path,
    design_version_id: i64,
    work_unit_id: i64,
    plan_path: &std::path::Path,
) -> (
    DecompositionApplicationOutcome,
    DecompositionApplicationOutcome,
) {
    let imported = apply_decomposition_plan(
        root,
        DecompositionApplication {
            design_version_id,
            work_unit_id,
            plan_path: Some(plan_path),
        },
    )
    .unwrap();
    accept_current_plan_review(root, design_version_id, work_unit_id);
    let applied = apply_decomposition_plan(
        root,
        DecompositionApplication {
            design_version_id,
            work_unit_id,
            plan_path: None,
        },
    )
    .unwrap();
    (imported, applied)
}

fn accept_current_plan_review(root: &std::path::Path, design_version_id: i64, work_unit_id: i64) {
    let resolution = resolve_decomposition_plan(
        root,
        DecompositionPlanQuery {
            design_version_id,
            work_unit_id,
        },
    )
    .unwrap();
    let context_ref = resolution.review_owner.unwrap().context_ref;
    let review_plan = add_review_plan(
        root,
        NewReviewPlan {
            work_unit_id,
            design_version_id: Some(design_version_id),
            review_type: "design_task_decomposition",
            required: true,
            stage: "implementation-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    let review_run = add_review_run(
        root,
        NewReviewRun {
            review_plan_id: review_plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some(&context_ref),
            prompt_deviations: None,
            result_summary: Some("the exact Plan is ready"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: Some("independent-plan-reviewer"),
            external_agent_id: Some("independent-plan-reviewer-1"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:exact-plan"),
        },
    )
    .unwrap();
    let pending = resolve_decomposition_plan(
        root,
        DecompositionPlanQuery {
            design_version_id,
            work_unit_id,
        },
    )
    .unwrap();
    let owner = pending.review_owner.unwrap();
    if owner.state != PlanReviewOwnerState::AcceptedClean {
        let expected_current = owner
            .actions
            .into_iter()
            .find(|action| action.contains("--decision accepted"))
            .and_then(|action| {
                action
                    .split("--expected-current ")
                    .nth(1)
                    .and_then(|tail| tail.split_whitespace().next())
                    .map(str::to_string)
            })
            .unwrap();
        adjudicate_review(
            root,
            review_run.review_run_id,
            AdjudicationInput {
                decision: "accepted",
                reason: "accept the exact clean Plan review",
                expected_current: &expected_current,
            },
        )
        .unwrap();
    }
}
