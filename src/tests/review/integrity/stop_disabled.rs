use super::*;

#[test]
fn stop_on_severity_none_does_not_block_findings() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "no severity stop", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "no-severity-stop",
            review_type: "implementation_review",
            max_fresh_agents: 2,
            max_resume_agents: 1,
            max_parallel_agents: 1,
            required_consecutive_clean_fresh_runs: 0,
            required_consecutive_clean_resume_runs: 0,
            stop_on_severity: "none",
            allow_resume_review: true,
            allow_fresh_review: true,
            allow_new_findings_in_resume: false,
            on_max_agents_exceeded: "block",
            run_count_scope: "review_plan",
            default_run_mode: "fresh",
        },
    )
    .unwrap();
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            review_type: "implementation_review",
            required: true,
            stage: "close-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: Some(policy.review_policy_id),
            review_scope_id: None,
        },
    )
    .unwrap();
    let run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: None,
            prompt_deviations: None,
            result_summary: None,
            new_findings_count: 1,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
            review_provenance: "self_recorded",
            review_provenance_ref: None,
        },
    )
    .unwrap();
    let finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: run.review_run_id,
            finding_type: "implementation_finding",
            severity: "critical",
            description: "critical but not a stop condition",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();

    let plans = list_review_plans(temp.path()).unwrap();
    assert_eq!(plans[0].status, "blocked");
    let approval_authority_event_id = approval_authority_event(temp.path());
    add_general_acceptance(
        temp.path(),
        NewGeneralAcceptance {
            target: &format!("finding:{}", finding.finding_id),
            acceptance_type: "explicit_exception",
            reason: "user accepted this finding as an explicit exception",
            approval_authority_event_id,
        },
    )
    .unwrap();
    add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: None,
            prompt_deviations: None,
            result_summary: Some("accepted exception checked"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
            review_provenance: "self_recorded",
            review_provenance_ref: None,
        },
    )
    .unwrap();
    let accepted_plans = list_review_plans(temp.path()).unwrap();
    assert_eq!(accepted_plans[0].status, "clean");
}
