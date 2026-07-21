use super::*;

#[test]
fn open_required_review_finding_blocks_next_action() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "review guarded phase", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "design-review-required",
            review_type: "design_review",
            max_fresh_agents: 1,
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
            result_summary: Some("found design issue"),
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
            finding_type: "design_finding",
            severity: "critical",
            description: "design review blocker must be resolved first",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();

    let status = project_status(temp.path()).unwrap();
    let next = next_action(temp.path()).unwrap();

    assert!(status.phase_blocker.is_none());
    let owner = status.owner_actions.first().unwrap();
    assert_eq!(owner.owner_id, work.work_unit_id);
    assert_eq!(
        owner.blocker_kind.as_deref(),
        Some("required_review_finding")
    );
    assert!(
        owner
            .next_action
            .contains(&format!("finding classify {}", finding.finding_id))
    );
    assert!(matches!(next, NextAction::OwnerActions { .. }));
    classify_finding(temp.path(), finding.finding_id, "valid").unwrap();
    let noneligible_closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: finding.finding_id,
            design_invariant: "design concern remains blocking",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some("docs:create:docs/design-fix.md"),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("create the corrected design note"),
            tests_or_gates: Some("design tests"),
            verification_plan: Some("resume review"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    let status = project_status(temp.path()).unwrap();
    assert!(status.phase_blocker.is_none());
    assert!(status.owner_actions[0].blocker_kind.is_some());
    assert!(status.finding_remediations.is_empty());
    std::fs::create_dir_all(temp.path().join("docs")).unwrap();
    std::fs::write(temp.path().join("docs/design-fix.md"), "premature edit").unwrap();
    assert!(
        begin_correction(temp.path(), noneligible_closure.closure_id)
            .unwrap_err()
            .to_string()
            .contains("changed after closure registration")
    );
    std::fs::remove_file(temp.path().join("docs/design-fix.md")).unwrap();
    begin_correction(temp.path(), noneligible_closure.closure_id).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    assert!(
        conn.execute(
            "insert into correction_tokens(project_id,closure_id,token_ordinal,token_kind,operation,target,status,created_at) values ((select id from projects limit 1),?1,99,'transition','arbitrary-command','x','pending',current_timestamp)",
            params![noneligible_closure.closure_id],
        )
        .is_err()
    );
    assert!(
        conn.execute(
            "insert into correction_tokens(project_id,closure_id,token_ordinal,token_kind,operation,target,status,created_at) values ((select id from projects limit 1),?1,100,'transition','phase-create','x/x/x/x/x/x','pending',current_timestamp)",
            params![noneligible_closure.closure_id],
        )
        .is_err()
    );
    assert!(
        conn.execute(
            "update correction_tokens set status='applied',applied_at=current_timestamp where closure_id=?1 and token_ordinal=1",
            params![noneligible_closure.closure_id],
        )
        .is_err()
    );
    drop(conn);
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update review_plans set status='exhausted' where id=?1",
        params![plan.review_plan_id],
    )
    .unwrap();
    drop(conn);
    let status = project_status(temp.path()).unwrap();
    let decision_blocker = &status.owner_actions[0];
    assert!(
        decision_blocker
            .next_action
            .contains(&format!("review plan waive {}", plan.review_plan_id))
    );
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update review_plans set status='open' where id=?1",
        params![plan.review_plan_id],
    )
    .unwrap();
    drop(conn);
    assert!(
        suspend_work(temp.path(), "must not bypass source correction", "resume")
            .unwrap_err()
            .to_string()
            .contains("source_correction")
    );
    assert!(
        create_phase(
            temp.path(),
            NewWorkPhase {
                work_unit_id: work.work_unit_id,
                design_version_id: None,
                key: "bypass",
                title: "bypass",
                kind: "test",
                order: 1,
                reason: None,
            },
        )
        .unwrap_err()
        .to_string()
        .contains("closure transition apply")
    );
    std::fs::write(temp.path().join("docs/design-fix.md"), "corrected design").unwrap();
    let correcting = project_status(temp.path()).unwrap();
    assert!(correcting.phase_blocker.is_none());
    assert_eq!(correcting.source_corrections.len(), 1);
    let attempt = ready_closure(
        temp.path(),
        ClosureReady {
            closure_id: noneligible_closure.closure_id,
            implementation_evidence: "design conflict resolved",
            tests_or_gates: "design tests pass",
            closed_by_commit: None,
        },
    )
    .unwrap();
    let not_fixed = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some(&attempt.context_ref),
            prompt_deviations: None,
            result_summary: Some("design fix remains incomplete"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: false,
            status: "completed",
            agent_label: Some("design-reviewer"),
            external_agent_id: Some("design-reviewer"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("design-review-output"),
        },
        Some("not_fixed"),
    )
    .unwrap();
    add_finding_verification(
        temp.path(),
        NewFindingVerification {
            review_run_id: not_fixed.review_run_id,
            finding_id: finding.finding_id,
            closure_id: noneligible_closure.closure_id,
            result: "not_fixed",
            notes: None,
        },
    )
    .unwrap();
    adjudicate_verification(
        temp.path(),
        not_fixed.review_run_id,
        finding.finding_id,
        noneligible_closure.closure_id,
        attempt.attempt_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "accept incomplete design correction",
            expected_current: "pending",
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let reopened: (String, String, String) = conn
        .query_row(
            "select s.status,c.status,a.result from correction_sessions s join closures c on c.id=s.closure_id join closure_attempts a on a.closure_id=c.id where s.closure_id=?1 order by a.id desc limit 1",
            params![noneligible_closure.closure_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        reopened,
        ("active".into(), "registered".into(), "not_fixed".into())
    );
    drop(conn);
    std::fs::write(
        temp.path().join("docs/design-fix.md"),
        "corrected design after review",
    )
    .unwrap();
    let second_attempt = ready_closure(
        temp.path(),
        ClosureReady {
            closure_id: noneligible_closure.closure_id,
            implementation_evidence: "design conflict resolved after review",
            tests_or_gates: "design tests pass after review",
            closed_by_commit: None,
        },
    )
    .unwrap();
    let replacement_plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
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
    let replacement_context = render_finding_fix_context(
        temp.path(),
        finding.finding_id,
        noneligible_closure.closure_id,
        second_attempt.attempt_id,
    )
    .unwrap();
    assert!(replacement_context.text.contains(&format!(
        "review_plan_id: {}",
        replacement_plan.review_plan_id
    )));
    let resume = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: replacement_plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some(&second_attempt.context_ref),
            prompt_deviations: None,
            result_summary: Some("verified design fix"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: Some("design-reviewer-2"),
            external_agent_id: Some("design-reviewer-2"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("design-review-output-2"),
        },
        Some("verified"),
    )
    .unwrap();
    add_finding_verification(
        temp.path(),
        NewFindingVerification {
            review_run_id: resume.review_run_id,
            finding_id: finding.finding_id,
            closure_id: noneligible_closure.closure_id,
            result: "verified",
            notes: None,
        },
    )
    .unwrap();
    adjudicate_verification(
        temp.path(),
        resume.review_run_id,
        finding.finding_id,
        noneligible_closure.closure_id,
        second_attempt.attempt_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "accept verified design correction",
            expected_current: "pending",
        },
    )
    .unwrap();
    assert_eq!(
        list_findings(temp.path(), None).unwrap()[0].status,
        "closed"
    );
    let verified_supersession = supersede_closure(
        temp.path(),
        ClosureSupersession {
            closure_id: noneligible_closure.closure_id,
            new_closure: NewClosure {
                finding_id: finding.finding_id,
                design_invariant: "must not replace verified closure",
                design_citations: None,
                implementation_evidence: None,
                affected_surfaces: Some("design"),
                same_invariant_search: None,
                other_violations_found: None,
                fix_plan: Some("none"),
                tests_or_gates: Some("none"),
                verification_plan: Some("none"),
                closed_by_commit: None,
            },
            reason: "must reject terminal supersession",
            authority_event_id: approval_authority_event(temp.path()),
        },
    );
    assert!(verified_supersession.is_err());
}
