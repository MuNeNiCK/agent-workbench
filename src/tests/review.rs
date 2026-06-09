use super::*;

#[test]
fn review_policy_clean_run_stop_condition_is_enforced() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "review storage design", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "strict-design",
            review_type: "design_review",
            max_fresh_agents: 3,
            max_resume_agents: 1,
            max_parallel_agents: 1,
            required_consecutive_clean_fresh_runs: 2,
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
            name: "storage-design",
            review_type: "design_review",
            scope: "storage design document",
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
            review_type: "design_review",
            required: true,
            stage: "design-ready",
            scope: Some("storage design document"),
            clean_condition: None,
            stop_condition: None,
            review_policy_id: Some(policy.review_policy_id),
            review_scope_id: Some(scope.review_scope_id),
        },
    )
    .unwrap();
    let targets = list_review_plan_targets(temp.path(), plan.review_plan_id).unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].target_type, "work_unit");
    assert_eq!(targets[0].work_unit_id, Some(work.work_unit_id));

    let first = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("HEAD"),
            prompt_deviations: None,
            result_summary: Some("clean"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: Some("agent-a"),
            external_agent_id: None,
        },
    )
    .unwrap();
    let second = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("HEAD"),
            prompt_deviations: None,
            result_summary: Some("clean"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: Some("agent-b"),
            external_agent_id: None,
        },
    )
    .unwrap();
    let plans = list_review_plans(temp.path()).unwrap();

    assert_eq!(first.plan_status, "open");
    assert_eq!(second.plan_status, "clean");
    assert_eq!(plans[0].status, "clean");
}

#[test]
fn review_agent_launch_limit_blocks_extra_runs() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "review implementation", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "single-pass",
            review_type: "implementation_review",
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
    add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("HEAD"),
            prompt_deviations: None,
            result_summary: None,
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
        },
    )
    .unwrap();

    let extra = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("HEAD"),
            prompt_deviations: None,
            result_summary: None,
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
        },
    );

    assert!(extra.is_err());
}

#[test]
fn review_run_rejects_clean_state_with_findings() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "clean state guard", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "clean-state",
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

    let inconsistent = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("HEAD"),
            prompt_deviations: None,
            result_summary: Some("contradictory"),
            new_findings_count: 1,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
        },
    );
    assert!(inconsistent.is_err());

    let clean = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("HEAD"),
            prompt_deviations: None,
            result_summary: Some("clean"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
        },
    )
    .unwrap();
    let finding_on_clean = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: clean.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "cannot attach",
            design_requirement_id: None,
            task_id: None,
        },
    );

    assert!(finding_on_clean.is_err());

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let invalid_update = conn.execute(
        "update review_runs set new_findings_count = 1 where id = ?1",
        params![clean.review_run_id],
    );
    let invalid_insert = conn.execute(
        r#"
        insert into review_runs(
            project_id, review_plan_id, run_type, run_purpose, target_type,
            work_unit_id, target_ref, new_findings_count,
            carried_findings_checked, clean_run, status, created_at
        )
        values (1, ?1, 'fresh', 'new_unbiased_review', 'work_unit', ?2, 'HEAD', 1, 0, 1, 'completed', current_timestamp)
        "#,
        params![plan.review_plan_id, work.work_unit_id],
    );

    assert!(invalid_update.is_err());
    assert!(invalid_insert.is_err());

    let dirty = add_review_run(
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
        },
    )
    .unwrap();
    add_finding(
        temp.path(),
        NewFinding {
            review_run_id: dirty.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "existing finding",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    let invalid_clean_flip = conn.execute(
        "update review_runs set new_findings_count = 0, clean_run = 1 where id = ?1",
        params![dirty.review_run_id],
    );

    assert!(invalid_clean_flip.is_err());
}

#[test]
fn review_plan_rejects_mismatched_policy_and_scope_type() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "type guard", None).unwrap();
    let design_policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "design-only",
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
    let design_scope = start_review_scope(
        temp.path(),
        NewReviewScope {
            name: "design-scope",
            review_type: "design_review",
            scope: "design only",
            allowed_inputs: None,
            forbidden_judgments: None,
            expected_output_type: None,
            exclusions: None,
            prompt_template_ref: None,
        },
    )
    .unwrap();

    let mismatched_policy = add_review_plan(
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
            review_policy_id: Some(design_policy.review_policy_id),
            review_scope_id: None,
        },
    );
    let mismatched_scope = add_review_plan(
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
            review_scope_id: Some(design_scope.review_scope_id),
        },
    );

    assert!(mismatched_policy.is_err());
    assert!(mismatched_scope.is_err());
}

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
            affected_surfaces: None,
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: None,
            tests_or_gates: None,
            verification_plan: None,
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
fn resume_verification_closes_valid_findings() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "review finding fix", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "resume-required",
            review_type: "implementation_review",
            max_fresh_agents: 1,
            max_resume_agents: 2,
            max_parallel_agents: 1,
            required_consecutive_clean_fresh_runs: 0,
            required_consecutive_clean_resume_runs: 1,
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
    let fresh = add_review_run(
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
        },
    )
    .unwrap();
    let finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: fresh.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "missing error handling",
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
            design_invariant: "errors are surfaced",
            design_citations: None,
            implementation_evidence: Some("abc123"),
            affected_surfaces: Some("cli"),
            same_invariant_search: Some("checked"),
            other_violations_found: Some("none"),
            fix_plan: Some("return errors"),
            tests_or_gates: Some("cargo test"),
            verification_plan: Some("resume review"),
            closed_by_commit: Some("abc123"),
        },
    )
    .unwrap();
    let fresh_verification = add_finding_verification(
        temp.path(),
        NewFindingVerification {
            review_run_id: fresh.review_run_id,
            finding_id: finding.finding_id,
            closure_id: closure.closure_id,
            result: "verified",
            notes: None,
        },
    );
    assert!(fresh_verification.is_err());
    let resume = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some("HEAD"),
            prompt_deviations: None,
            result_summary: Some("verified"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
        },
    )
    .unwrap();
    add_finding_verification(
        temp.path(),
        NewFindingVerification {
            review_run_id: resume.review_run_id,
            finding_id: finding.finding_id,
            closure_id: closure.closure_id,
            result: "verified",
            notes: None,
        },
    )
    .unwrap();
    let findings = list_findings(temp.path(), None).unwrap();
    let plans = list_review_plans(temp.path()).unwrap();

    assert_eq!(findings[0].status, "closed");
    assert_eq!(plans[0].status, "clean");
}

#[test]
fn finding_verification_rejects_unrelated_closure() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "review two findings", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "closure-integrity",
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
    let fresh = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: None,
            prompt_deviations: None,
            result_summary: None,
            new_findings_count: 2,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
        },
    )
    .unwrap();
    let first = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: fresh.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "first finding",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    let second = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: fresh.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "second finding",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), first.finding_id, "valid").unwrap();
    classify_finding(temp.path(), second.finding_id, "valid").unwrap();
    let first_closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: first.finding_id,
            design_invariant: "first invariant",
            design_citations: None,
            implementation_evidence: Some("abc123"),
            affected_surfaces: None,
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: None,
            tests_or_gates: None,
            verification_plan: None,
            closed_by_commit: None,
        },
    )
    .unwrap();
    let resume = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: None,
            prompt_deviations: None,
            result_summary: None,
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
        },
    )
    .unwrap();

    let mismatch = add_finding_verification(
        temp.path(),
        NewFindingVerification {
            review_run_id: resume.review_run_id,
            finding_id: second.finding_id,
            closure_id: first_closure.closure_id,
            result: "verified",
            notes: None,
        },
    );

    assert!(mismatch.is_err());
}

#[test]
fn finding_verification_update_preserves_scope_constraints() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "verification update guard", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "verification-update",
            review_type: "implementation_review",
            max_fresh_agents: 2,
            max_resume_agents: 2,
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
    let fresh = add_review_run(
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
        },
    )
    .unwrap();
    let finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: fresh.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "update guarded finding",
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
            design_invariant: "update invariant",
            design_citations: None,
            implementation_evidence: Some("abc123"),
            affected_surfaces: None,
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: None,
            tests_or_gates: None,
            verification_plan: None,
            closed_by_commit: None,
        },
    )
    .unwrap();
    let resume = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: None,
            prompt_deviations: None,
            result_summary: None,
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
        },
    )
    .unwrap();
    add_finding_verification(
        temp.path(),
        NewFindingVerification {
            review_run_id: resume.review_run_id,
            finding_id: finding.finding_id,
            closure_id: closure.closure_id,
            result: "not_fixed",
            notes: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();

    let update_to_fresh = conn.execute(
        "update finding_verifications set review_run_id = ?1 where id = 1",
        params![fresh.review_run_id],
    );

    assert!(update_to_fresh.is_err());
}

#[test]
fn finding_verification_rejects_different_plan_finding() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let first_work = start_work(temp.path(), "first plan", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "same-plan-verification",
            review_type: "implementation_review",
            max_fresh_agents: 2,
            max_resume_agents: 2,
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
    let first_plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: first_work.work_unit_id,
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
    let first_run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: first_plan.review_plan_id,
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
        },
    )
    .unwrap();
    let finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: first_run.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "first plan finding",
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
            design_invariant: "same plan invariant",
            design_citations: None,
            implementation_evidence: Some("abc123"),
            affected_surfaces: None,
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: None,
            tests_or_gates: None,
            verification_plan: None,
            closed_by_commit: None,
        },
    )
    .unwrap();
    let second_work = interrupt_work(temp.path(), "second plan", "verify different plan").unwrap();
    let second_plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: second_work.child_work_unit_id,
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
    let second_resume = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: second_plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: None,
            prompt_deviations: None,
            result_summary: None,
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
        },
    )
    .unwrap();

    let cross_plan = add_finding_verification(
        temp.path(),
        NewFindingVerification {
            review_run_id: second_resume.review_run_id,
            finding_id: finding.finding_id,
            closure_id: closure.closure_id,
            result: "verified",
            notes: None,
        },
    );

    assert!(cross_plan.is_err());
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
        },
    );

    assert!(fresh_fix.is_err());
    assert!(resume_unbiased.is_err());
}

#[test]
fn resume_policy_blocks_new_findings_when_disallowed() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "verify known finding only", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "resume-no-new",
            review_type: "implementation_review",
            max_fresh_agents: 1,
            max_resume_agents: 2,
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

    let resume_with_count = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: None,
            prompt_deviations: None,
            result_summary: None,
            new_findings_count: 1,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
        },
    );
    assert!(resume_with_count.is_err());

    let resume = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: None,
            prompt_deviations: None,
            result_summary: None,
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
        },
    )
    .unwrap();
    let finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: resume.review_run_id,
            finding_type: "implementation_finding",
            severity: "medium",
            description: "new resume finding",
            design_requirement_id: None,
            task_id: None,
        },
    );
    assert!(finding.is_err());

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let direct_resume_count_insert = conn.execute(
        r#"
        insert into review_runs(
            project_id, review_plan_id, run_type, run_purpose, target_type,
            work_unit_id, target_ref, new_findings_count,
            carried_findings_checked, clean_run, status, created_at
        )
        values (1, ?1, 'resume', 'finding_fix_verification', 'work_unit', ?2, ?3, 1, 0, 0, 'completed', current_timestamp)
        "#,
        params![
            plan.review_plan_id,
            work.work_unit_id,
            format!("work_unit:{}", work.work_unit_id),
        ],
    );
    let direct_resume_count_update = conn.execute(
        "update review_runs set new_findings_count = 1 where id = ?1",
        params![resume.review_run_id],
    );
    let direct_resume_finding_insert = conn.execute(
        r#"
        insert into findings(
            project_id, review_run_id, finding_type, severity,
            description, classification, status, created_at
        )
        values (1, ?1, 'implementation_finding', 'medium', 'direct resume finding', 'unclassified', 'open', current_timestamp)
        "#,
        params![resume.review_run_id],
    );

    assert!(direct_resume_count_insert.is_err());
    assert!(direct_resume_count_update.is_err());
    assert!(direct_resume_finding_insert.is_err());

    let permissive_policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "resume-allows-new",
            review_type: "implementation_review",
            max_fresh_agents: 1,
            max_resume_agents: 3,
            max_parallel_agents: 1,
            required_consecutive_clean_fresh_runs: 0,
            required_consecutive_clean_resume_runs: 0,
            stop_on_severity: "none",
            allow_resume_review: true,
            allow_fresh_review: true,
            allow_new_findings_in_resume: true,
            on_max_agents_exceeded: "block",
            run_count_scope: "review_plan",
            default_run_mode: "resume",
        },
    )
    .unwrap();
    let permissive_plan = add_review_plan(
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
            review_policy_id: Some(permissive_policy.review_policy_id),
            review_scope_id: None,
        },
    )
    .unwrap();
    let counted_resume = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: permissive_plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: None,
            prompt_deviations: None,
            result_summary: Some("allowed count"),
            new_findings_count: 1,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
        },
    )
    .unwrap();
    let actual_resume = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: permissive_plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: None,
            prompt_deviations: None,
            result_summary: Some("allowed finding"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
        },
    )
    .unwrap();
    add_finding(
        temp.path(),
        NewFinding {
            review_run_id: actual_resume.review_run_id,
            finding_type: "implementation_finding",
            severity: "medium",
            description: "allowed resume finding",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    let policy_tighten_with_actual_finding = conn.execute(
        "update review_policies set allow_new_findings_in_resume = 0 where id = ?1",
        params![permissive_policy.review_policy_id],
    );
    let plan_policy_swap_with_count = conn.execute(
        "update review_plans set review_policy_id = ?1 where id = ?2",
        params![policy.review_policy_id, permissive_plan.review_plan_id],
    );
    let run_plan_swap_with_count = conn.execute(
        "update review_runs set review_plan_id = ?1 where id = ?2",
        params![plan.review_plan_id, counted_resume.review_run_id],
    );

    assert!(policy_tighten_with_actual_finding.is_err());
    assert!(plan_policy_swap_with_count.is_err());
    assert!(run_plan_swap_with_count.is_err());

    conn.execute_batch(
        r#"
        drop trigger trg_review_policy_resume_findings_update;
        create trigger trg_review_policy_resume_findings_update
        before update of allow_new_findings_in_resume on review_policies
        for each row
        when new.allow_new_findings_in_resume = 0
          and exists (
              select 1
              from review_plans p
              join review_runs r on r.review_plan_id = p.id
              where p.review_policy_id = old.id
                and r.run_type = 'resume'
                and r.new_findings_count > 0
          )
        begin
            select raise(abort, 'old weak trigger');
        end;
        "#,
    )
    .unwrap();
    drop(conn);
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let refreshed_trigger_sql: String = conn
        .query_row(
            r#"
            select sql
            from sqlite_schema
            where type = 'trigger'
              and name = 'trg_review_policy_resume_findings_update'
            "#,
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert!(refreshed_trigger_sql.contains("left join findings"));
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
    assert_eq!(plans[0].status, "blocked");
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
    add_general_acceptance(
        temp.path(),
        NewGeneralAcceptance {
            target: &format!("finding:{}", finding.finding_id),
            acceptance_type: "explicit_exception",
            reason: "user accepted this finding as an explicit exception",
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
        },
    )
    .unwrap();
    let accepted_plans = list_review_plans(temp.path()).unwrap();
    assert_eq!(accepted_plans[0].status, "clean");
}

#[test]
fn review_plan_targets_reject_cross_project_targets() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "target integrity", None).unwrap();
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
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into projects(name, root_path, created_at, updated_at) values ('other', '/tmp/other-awb-target', current_timestamp, current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        "insert into work_units(project_id, title, status, started_at) values (2, 'other work', 'open', current_timestamp)",
        [],
    )
    .unwrap();

    let cross_project = conn.execute(
        r#"
        insert into review_plan_targets(review_plan_id, target_type, work_unit_id)
        values (?1, 'work_unit', 2)
        "#,
        params![plan.review_plan_id],
    );
    let repository_snapshot_target = conn.execute(
        r#"
        insert into review_plan_targets(review_plan_id, target_type, repository_snapshot_id)
        values (?1, 'repository_snapshot', 1)
        "#,
        params![plan.review_plan_id],
    );

    assert!(cross_project.is_err());
    assert!(repository_snapshot_target.is_err());
}

#[test]
fn review_plan_targets_can_drive_typed_review_run_targets() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "typed targets", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "targeted task",
            priority: "medium",
            source: "user",
            work_unit_id: None,
            details: None,
            completion_condition: Some("target is reviewed"),
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
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    let target = add_review_plan_target(
        temp.path(),
        NewReviewPlanTarget {
            review_plan_id: plan.review_plan_id,
            target_type: "task",
            design_version_id: None,
            design_requirement_id: None,
            task_id: Some(task.task_id),
            work_unit_id: None,
            repository_snapshot_id: None,
            file_path: None,
            symbol: None,
        },
    )
    .unwrap();

    let run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some(&format!("task:{}", task.task_id)),
            prompt_deviations: None,
            result_summary: Some("task target clean"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
        },
    )
    .unwrap();
    let records = list_review_plan_targets(temp.path(), plan.review_plan_id).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let (target_type, task_id): (String, i64) = conn
        .query_row(
            "select target_type, task_id from review_runs where id = ?1",
            params![run.review_run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(records.len(), 2);
    assert!(
        records
            .iter()
            .any(|record| record.id == target.review_plan_target_id)
    );
    assert_eq!(target_type, "task");
    assert_eq!(task_id, task.task_id);
}

#[test]
fn file_review_run_targets_are_stored_in_typed_columns() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "file target", None).unwrap();
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
    add_review_plan_target(
        temp.path(),
        NewReviewPlanTarget {
            review_plan_id: plan.review_plan_id,
            target_type: "file",
            design_version_id: None,
            design_requirement_id: None,
            task_id: None,
            work_unit_id: None,
            repository_snapshot_id: None,
            file_path: Some("src/lib.rs"),
            symbol: None,
        },
    )
    .unwrap();
    let run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("file:src/lib.rs"),
            prompt_deviations: None,
            result_summary: Some("file clean"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let (target_type, file_path, target_ref): (String, String, String) = conn
        .query_row(
            "select target_type, file_path, target_ref from review_runs where id = ?1",
            params![run.review_run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();

    assert_eq!(target_type, "file");
    assert_eq!(file_path, "src/lib.rs");
    assert_eq!(target_ref, "file:src/lib.rs");
}
