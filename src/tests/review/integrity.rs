use super::*;

#[test]
fn review_integrity_triggers_guard_cross_project_updates() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "project guard", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "project-guard",
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
    let scope = start_review_scope(
        temp.path(),
        NewReviewScope {
            name: "implementation-scope",
            review_type: "implementation_review",
            scope: "implementation only",
            allowed_inputs: None,
            forbidden_judgments: None,
            expected_output_type: None,
            exclusions: None,
            prompt_template_ref: None,
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
            review_scope_id: Some(scope.review_scope_id),
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
            result_summary: Some("found issue"),
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
            description: "guarded finding",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), finding.finding_id, "valid").unwrap();
    let closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: finding.finding_id,
            design_invariant: "project integrity",
            design_citations: None,
            implementation_evidence: Some("abc123"),
            affected_surfaces: Some("src/review.rs"),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("preserve project integrity"),
            tests_or_gates: Some("cargo test"),
            verification_plan: Some("resume review"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into projects(name, root_path, created_at, updated_at) values ('other', '/tmp/other-awb-review', current_timestamp, current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        "insert into work_units(project_id, title, status, started_at) values (2, 'other work', 'open', current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        "insert into work_units(project_id, title, status, started_at) values (1, 'same project other target', 'open', current_timestamp)",
        [],
    )
    .unwrap();
    let same_project_work_unit_id = conn.last_insert_rowid();
    let plan_target_id: i64 = conn
        .query_row(
            "select id from review_plan_targets where review_plan_id = ?1 and target_type = 'work_unit'",
            params![plan.review_plan_id],
            |row| row.get(0),
        )
        .unwrap();

    let plan_project_break = conn.execute(
        "update review_plans set work_unit_id = 2 where id = ?1",
        params![plan.review_plan_id],
    );
    let plan_type_break = conn.execute(
        "update review_plans set review_type = 'design_review' where id = ?1",
        params![plan.review_plan_id],
    );
    let plan_policy_null_break = conn.execute(
        "update review_plans set review_policy_id = null where id = ?1",
        params![plan.review_plan_id],
    );
    let policy_type_break = conn.execute(
        "update review_policies set review_type = 'design_review' where id = ?1",
        params![policy.review_policy_id],
    );
    let scope_type_break = conn.execute(
        "update review_scopes set review_type = 'design_review' where id = ?1",
        params![scope.review_scope_id],
    );
    let run_project_break = conn.execute(
        "update review_runs set project_id = 2 where id = ?1",
        params![run.review_run_id],
    );
    let run_plan_null_break = conn.execute(
        "update review_runs set review_plan_id = null where id = ?1",
        params![run.review_run_id],
    );
    let run_target_update_break = conn.execute(
        "update review_runs set work_unit_id = 2 where id = ?1",
        params![run.review_run_id],
    );
    let run_plan_target_update_break = conn.execute(
        "update review_runs set work_unit_id = ?1, target_ref = ?2 where id = ?3",
        params![
            same_project_work_unit_id,
            format!("work_unit:{same_project_work_unit_id}"),
            run.review_run_id,
        ],
    );
    let run_target_insert_break = conn.execute(
        r#"
        insert into review_runs(
            project_id, review_scope_id, review_plan_id, run_type, run_purpose,
            target_type, work_unit_id, target_ref, new_findings_count,
            carried_findings_checked, clean_run, status, created_at
        )
        values (1, ?1, ?2, 'fresh', 'new_unbiased_review', 'work_unit', 2, 'work_unit:2', 0, 0, 0, 'completed', current_timestamp)
        "#,
        params![scope.review_scope_id, plan.review_plan_id],
    );
    let run_plan_target_insert_break = conn.execute(
        r#"
        insert into review_runs(
            project_id, review_scope_id, review_plan_id, run_type, run_purpose,
            target_type, work_unit_id, target_ref, new_findings_count,
            carried_findings_checked, clean_run, status, created_at
        )
        values (1, ?1, ?2, 'fresh', 'new_unbiased_review', 'work_unit', ?3, ?4, 0, 0, 0, 'completed', current_timestamp)
        "#,
        params![
            scope.review_scope_id,
            plan.review_plan_id,
            same_project_work_unit_id,
            format!("work_unit:{same_project_work_unit_id}"),
        ],
    );
    let run_plan_null_insert_break = conn.execute(
        r#"
        insert into review_runs(
            project_id, review_scope_id, run_type, run_purpose,
            target_type, work_unit_id, target_ref, new_findings_count,
            carried_findings_checked, clean_run, status, created_at
        )
        values (1, ?1, 'fresh', 'new_unbiased_review', 'work_unit', ?2, ?3, 0, 0, 0, 'completed', current_timestamp)
        "#,
        params![
            scope.review_scope_id,
            work.work_unit_id,
            format!("work_unit:{}", work.work_unit_id),
        ],
    );
    let plan_target_update_break = conn.execute(
        "update review_plan_targets set work_unit_id = ?1 where id = ?2",
        params![same_project_work_unit_id, plan_target_id],
    );
    let plan_target_delete_break = conn.execute(
        "delete from review_plan_targets where id = ?1",
        params![plan_target_id],
    );
    let finding_project_break = conn.execute(
        "update findings set project_id = 2 where id = ?1",
        params![finding.finding_id],
    );
    let closure_project_break = conn.execute(
        "update closures set project_id = 2 where id = ?1",
        params![closure.closure_id],
    );

    assert!(plan_project_break.is_err());
    assert!(plan_type_break.is_err());
    assert!(plan_policy_null_break.is_err());
    assert!(policy_type_break.is_err());
    assert!(scope_type_break.is_err());
    assert!(run_project_break.is_err());
    assert!(run_plan_null_break.is_err());
    assert!(run_target_update_break.is_err());
    assert!(run_plan_target_update_break.is_err());
    assert!(run_target_insert_break.is_err());
    assert!(run_plan_target_insert_break.is_err());
    assert!(run_plan_null_insert_break.is_err());
    assert!(plan_target_update_break.is_err());
    assert!(plan_target_delete_break.is_err());
    assert!(finding_project_break.is_err());
    assert!(closure_project_break.is_err());
}

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
    let resume = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some(&attempt.context_ref),
            prompt_deviations: None,
            result_summary: Some("verified design fix"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: Some("design-reviewer"),
            external_agent_id: Some("design-reviewer"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("design-review-output"),
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
        attempt.attempt_id,
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

#[test]
fn review_run_rejects_invalid_type_purpose_pairs() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "type purpose pairs", None).unwrap();
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
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();

    let fresh_fix = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "finding_fix_verification",
            target_ref: None,
            prompt_deviations: None,
            result_summary: None,
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
            review_provenance: "self_recorded",
            review_provenance_ref: None,
        },
    );
    let resume_unbiased = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "new_unbiased_review",
            target_ref: None,
            prompt_deviations: None,
            result_summary: None,
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
            review_provenance: "self_recorded",
            review_provenance_ref: None,
        },
    );

    assert!(fresh_fix.is_err());
    assert!(resume_unbiased.is_err());
}

#[test]
fn stop_on_severity_ignores_lower_severity_findings() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "severity threshold", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "high-only",
            review_type: "implementation_review",
            max_fresh_agents: 1,
            max_resume_agents: 1,
            max_parallel_agents: 1,
            required_consecutive_clean_fresh_runs: 0,
            required_consecutive_clean_resume_runs: 0,
            stop_on_severity: "high",
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
    add_finding(
        temp.path(),
        NewFinding {
            review_run_id: run.review_run_id,
            finding_type: "implementation_finding",
            severity: "low",
            description: "low severity note",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();

    let plans = list_review_plans(temp.path()).unwrap();
    assert_eq!(plans[0].status, "clean");
}

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
