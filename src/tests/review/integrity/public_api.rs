use super::*;

#[test]
fn public_review_api_requires_explicit_typed_finding_result() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "typed resume result", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "typed-result",
            review_type: "implementation_review",
            max_fresh_agents: 1,
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
            default_run_mode: "resume",
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
    let error = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some("review-context:finding-fix:finding=1:closure=1:attempt=1"),
            prompt_deviations: None,
            result_summary: None,
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: Some("reviewer"),
            external_agent_id: Some("reviewer"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output"),
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("--finding-result"));
    let untrusted = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some("review-context:finding-fix:finding=1:closure=1:attempt=1"),
            prompt_deviations: None,
            result_summary: Some("claimed verification"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
            review_provenance: "self_recorded",
            review_provenance_ref: None,
        },
        Some("verified"),
    )
    .unwrap_err();
    assert!(untrusted.to_string().contains("trusted"));
    let incomplete_external = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some("review-context:finding-fix:finding=1:closure=1:attempt=1"),
            prompt_deviations: None,
            result_summary: Some("missing provenance ref"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: Some("reviewer"),
            external_agent_id: Some("reviewer"),
            review_provenance: "external_agent",
            review_provenance_ref: None,
        },
        Some("verified"),
    )
    .unwrap_err();
    let incomplete_external = incomplete_external.to_string();
    assert!(
        incomplete_external.contains("--provenance-ref"),
        "{incomplete_external}"
    );
}
