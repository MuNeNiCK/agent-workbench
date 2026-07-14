use super::*;
use agent_workbench::{
    NewAuthorityEvent, NewFinding, NewReviewPlan, NewReviewPolicy, NewReviewRun, NewTask,
    add_authority_event, add_finding, add_review_plan, add_review_policy, add_review_run, add_task,
    classify_finding, init_project, start_work,
};

#[test]
fn remediation_cli_exposes_ready_supersede_disposition_and_typed_result() {
    let temp = tempfile::tempdir().unwrap();
    let closure_help = ok(temp.path(), &["closure", "--help"]);
    assert!(closure_help.contains("ready"));
    assert!(closure_help.contains("supersede"));

    let finding_help = ok(temp.path(), &["finding", "--help"]);
    assert!(finding_help.contains("accept-out-of-scope"));

    let run_help = ok(temp.path(), &["review", "run", "add", "--help"]);
    assert!(run_help.contains("--finding-result"));

    let context_help = ok(temp.path(), &["review-context", "--help"]);
    assert!(context_help.contains("--finding"));
    assert!(context_help.contains("--closure"));
    assert!(context_help.contains("--attempt"));
}

#[test]
fn acceptance_success_confirmations_do_not_expose_target_ids() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "publication boundary", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            work_unit_id: Some(work.work_unit_id),
            title: "accepted task",
            priority: "medium",
            source: "user",
            details: None,
            completion_condition: Some("accepted"),
        },
    )
    .unwrap();
    let task_output = ok(
        temp.path(),
        &[
            "task",
            "accept-out-of-scope",
            &task.task_id.to_string(),
            "--reason",
            "approved",
        ],
    );
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "publication-review",
            review_type: "implementation_review",
            max_fresh_agents: 2,
            max_resume_agents: 1,
            max_parallel_agents: 1,
            required_consecutive_clean_fresh_runs: 1,
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
            target_ref: Some("publication-boundary"),
            prompt_deviations: None,
            result_summary: Some("one finding"),
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
            severity: "high",
            description: "accepted finding",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), finding.finding_id, "valid").unwrap();
    let authority = add_authority_event(
        temp.path(),
        NewAuthorityEvent {
            event_type: "user_instruction",
            source: Some("test-user"),
            summary: "approve acceptance",
            scope: Some("publication boundary"),
            precedence: 100,
        },
    )
    .unwrap();

    let finding_output = ok(
        temp.path(),
        &[
            "finding",
            "accept-out-of-scope",
            &finding.finding_id.to_string(),
            "--reason",
            "approved",
            "--authority",
            &authority.authority_event_id.to_string(),
        ],
    );
    let plan_output = ok(
        temp.path(),
        &[
            "review",
            "plan",
            "waive",
            &plan.review_plan_id.to_string(),
            "--reason",
            "approved",
            "--authority",
            &authority.authority_event_id.to_string(),
        ],
    );

    assert_eq!(task_output, "accepted task out of scope\n");
    assert_eq!(finding_output, "accepted finding out of scope\n");
    assert_eq!(plan_output, "waived review plan\n");
}
