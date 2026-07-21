use super::*;

#[test]
fn review_obligations_route_independently_for_multiple_owners() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let first = start_work(temp.path(), "first owner", None).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into work_units(project_id,title,status,started_at) values (1,'second owner','open',current_timestamp)",
        [],
    )
    .unwrap();
    let second_id = conn.last_insert_rowid();
    drop(conn);
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "owner-local-review",
            review_type: "design_review",
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

    for (owner_id, description) in [
        (first.work_unit_id, "first owner finding"),
        (second_id, "second owner finding"),
    ] {
        let plan = add_review_plan(
            temp.path(),
            NewReviewPlan {
                work_unit_id: owner_id,
                design_version_id: None,
                review_type: "design_review",
                required: true,
                stage: "design-ready",
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
                target_ref: Some("HEAD"),
                prompt_deviations: None,
                result_summary: Some(description),
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
        add_finding(
            temp.path(),
            NewFinding {
                review_run_id: run.review_run_id,
                finding_type: "design_finding",
                severity: "critical",
                description,
                design_requirement_id: None,
                task_id: None,
            },
        )
        .unwrap();
    }

    let status = project_status(temp.path()).unwrap();
    assert!(status.phase_blocker.is_none());
    assert_eq!(status.owner_actions.len(), 2);
    assert!(status.owner_actions.iter().all(|owner| owner.schedulable));
    assert!(status.owner_actions.iter().all(|owner| {
        owner.blocker_kind.as_deref() == Some("required_review_finding")
            && owner.next_action.contains("finding classify")
    }));
}
