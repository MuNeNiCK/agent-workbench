use super::*;

fn completed_claim(clean: bool) -> (tempfile::TempDir, i64) {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "owner review decision", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "owner-decision-policy",
            review_type: "implementation_review",
            max_fresh_agents: 2,
            max_resume_agents: 0,
            max_parallel_agents: 1,
            required_consecutive_clean_fresh_runs: 1,
            required_consecutive_clean_resume_runs: 0,
            stop_on_severity: "none",
            allow_resume_review: false,
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
    let run = crate::review::add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("work_unit:1"),
            prompt_deviations: None,
            result_summary: Some("review claim"),
            new_findings_count: if clean { 0 } else { 1 },
            carried_findings_checked: 0,
            clean_run: clean,
            status: "completed",
            agent_label: Some("reviewer"),
            external_agent_id: None,
            review_provenance: "human_review",
            review_provenance_ref: Some("review-record:1"),
        },
    )
    .unwrap();
    (temp, run.review_run_id)
}

#[test]
fn owner_review_decision_is_idempotent_and_rejects_stale_supersession() {
    let (temp, run) = completed_claim(true);
    let input = AdjudicationInput {
        decision: "accepted",
        reason: "accept exact claim",
        expected_current: "pending",
    };
    let first = adjudicate_review(temp.path(), run, input.clone()).unwrap();
    let retry = adjudicate_review(temp.path(), run, input).unwrap();
    assert_eq!(retry.decision_handle, first.decision_handle);

    let stale = adjudicate_review(
        temp.path(),
        run,
        AdjudicationInput {
            decision: "rejected",
            reason: "stale competing decision",
            expected_current: "pending",
        },
    )
    .unwrap_err();
    assert!(stale.to_string().contains("expected_current_stale"));

    let evidence = adjudicate_review(
        temp.path(),
        run,
        AdjudicationInput {
            decision: "needs_evidence",
            reason: "request concrete evidence",
            expected_current: &first.decision_handle,
        },
    )
    .unwrap();
    adjudicate_review(
        temp.path(),
        run,
        AdjudicationInput {
            decision: "rejected",
            reason: "evidence did not support claim",
            expected_current: &evidence.decision_handle,
        },
    )
    .unwrap();
}

#[test]
fn concurrent_different_decisions_have_one_winner() {
    let (temp, run) = completed_claim(true);
    let root_a = temp.path().to_path_buf();
    let root_b = root_a.clone();
    let accepted = std::thread::spawn(move || {
        adjudicate_review(
            &root_a,
            run,
            AdjudicationInput {
                decision: "accepted",
                reason: "concurrent acceptance",
                expected_current: "pending",
            },
        )
    });
    let rejected = std::thread::spawn(move || {
        adjudicate_review(
            &root_b,
            run,
            AdjudicationInput {
                decision: "rejected",
                reason: "concurrent rejection",
                expected_current: "pending",
            },
        )
    });
    let outcomes = [accepted.join().unwrap(), rejected.join().unwrap()];
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
    let loser = outcomes.into_iter().find_map(Result::err).unwrap();
    assert!(loser.to_string().contains("expected_current_stale"));
}

#[test]
fn finding_decision_is_the_classification_and_lifecycle_authority() {
    let (temp, run) = completed_claim(false);
    let finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: run,
            finding_type: "implementation_finding",
            severity: "high",
            description: "owner must decide this claim",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    let accepted = decide_finding(
        temp.path(),
        finding.finding_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "finding is valid",
            expected_current: "pending",
        },
    )
    .unwrap();
    let retry = decide_finding(
        temp.path(),
        finding.finding_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "finding is valid",
            expected_current: "pending",
        },
    )
    .unwrap();
    assert_eq!(retry.decision_handle, accepted.decision_handle);
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let accepted_state: (String, String) = conn
        .query_row(
            "select classification,lifecycle_state from findings where id=?1",
            params![finding.finding_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(accepted_state, ("valid".into(), "open".into()));
    drop(conn);

    let rejected = decide_finding(
        temp.path(),
        finding.finding_id,
        AdjudicationInput {
            decision: "rejected",
            reason: "evidence disproved the finding",
            expected_current: &accepted.decision_handle,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let rejected_state: (String, String, String) = conn
        .query_row(
            "select classification,status,lifecycle_state from findings where id=?1",
            params![finding.finding_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        rejected_state,
        ("invalid".into(), "closed".into(), "closed".into())
    );
    assert!(!rejected.decision_handle.is_empty());
}
