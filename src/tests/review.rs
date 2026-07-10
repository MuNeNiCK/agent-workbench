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
            review_provenance: "self_recorded",
            review_provenance_ref: None,
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
            review_provenance: "self_recorded",
            review_provenance_ref: None,
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
            review_provenance: "self_recorded",
            review_provenance_ref: None,
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
            review_provenance: "self_recorded",
            review_provenance_ref: None,
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
            review_provenance: "self_recorded",
            review_provenance_ref: None,
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
            review_provenance: "self_recorded",
            review_provenance_ref: None,
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
            review_provenance: "self_recorded",
            review_provenance_ref: None,
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
fn finding_type_must_match_review_type() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "finding type guard", None).unwrap();
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
            review_policy_id: None,
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

    let mismatched = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: run.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "wrong ledger",
            design_requirement_id: None,
            task_id: None,
        },
    );
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let raw_mismatched = conn.execute(
        r#"
        insert into findings(
            project_id, review_run_id, finding_type, severity, description, created_at
        )
        values (1, ?1, 'implementation_finding', 'high', 'wrong ledger', current_timestamp)
        "#,
        params![run.review_run_id],
    );

    assert!(mismatched.is_err());
    assert!(raw_mismatched.is_err());
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

    assert_eq!(
        status.phase_blocker.as_ref().and_then(|b| b.finding_id),
        Some(finding.finding_id)
    );
    assert_eq!(
        next,
        NextAction::BlockedPhase {
            blocker: PhaseBlocker {
                kind: "required_review_finding".to_string(),
                review_plan_id: Some(plan.review_plan_id),
                work_unit_id: Some(work.work_unit_id),
                review_type: Some("design_review".to_string()),
                stage: Some("design-ready".to_string()),
                review_run_id: Some(run.review_run_id),
                finding_id: Some(finding.finding_id),
                severity: Some("critical".to_string()),
                classification: Some("unclassified".to_string()),
                description: "design review blocker must be resolved first".to_string(),
                next_action: format!(
                    "agent-workbench finding classify {} --classification valid|invalid|design_conflict|needs_evidence",
                    finding.finding_id
                ),
            }
        }
    );
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
    assert!(status.phase_blocker.is_some());
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
    let decision_blocker = project_status(temp.path()).unwrap().phase_blocker.unwrap();
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
            phase_id: None,
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
            review_provenance: "self_recorded",
            review_provenance_ref: None,
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
            phase_id: None,
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
            review_provenance: "self_recorded",
            review_provenance_ref: None,
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

#[test]
fn close_ready_finding_allows_remediation_then_requires_exact_resume_verification() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "remediate implementation", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "remediation-policy",
            review_type: "implementation_review",
            max_fresh_agents: 1,
            max_resume_agents: 2,
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
    let fresh = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("work_unit:1"),
            prompt_deviations: None,
            result_summary: Some("found issue"),
            new_findings_count: 3,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: Some("reviewer"),
            external_agent_id: Some("reviewer-1"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:1"),
        },
    )
    .unwrap();
    let finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: fresh.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "fix implementation",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), finding.finding_id, "valid").unwrap();
    let empty_invariant = add_closure(
        temp.path(),
        NewClosure {
            finding_id: finding.finding_id,
            design_invariant: " ",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some("src/review.rs"),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("reject empty invariant"),
            tests_or_gates: Some("cargo test"),
            verification_plan: Some("resume review"),
            closed_by_commit: None,
        },
    );
    assert!(
        empty_invariant
            .unwrap_err()
            .to_string()
            .contains("--invariant")
    );
    let incomplete_contract = |surfaces, fix_plan, tests, verification| {
        add_closure(
            temp.path(),
            NewClosure {
                finding_id: finding.finding_id,
                design_invariant: "implementation is correct",
                design_citations: None,
                implementation_evidence: None,
                affected_surfaces: surfaces,
                same_invariant_search: None,
                other_violations_found: None,
                fix_plan,
                tests_or_gates: tests,
                verification_plan: verification,
                closed_by_commit: None,
            },
        )
    };
    for result in [
        incomplete_contract(None, Some("fix"), Some("test"), Some("verify")),
        incomplete_contract(Some("src"), None, Some("test"), Some("verify")),
        incomplete_contract(Some("src"), Some("fix"), None, Some("verify")),
        incomplete_contract(Some("src"), Some("fix"), Some("test"), None),
    ] {
        assert!(result.is_err());
    }
    let closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: finding.finding_id,
            design_invariant: "implementation is correct",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some("src/review.rs"),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("implement lifecycle"),
            tests_or_gates: Some("cargo test"),
            verification_plan: Some("resume review"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update work_unit_activations set status = 'completed', completed_at = current_timestamp where id = ?1",
        params![work.activation_id],
    )
    .unwrap();
    conn.execute(
        "update work_units set status = 'closed', closed_at = current_timestamp where id = ?1",
        params![work.work_unit_id],
    )
    .unwrap();
    drop(conn);
    let recovery_authority = approval_authority_event(temp.path());
    let reopened = reopen_work(
        temp.path(),
        WorkReopen {
            work_unit_id: work.work_unit_id,
            reason: "verified finding invalidates the old closure",
            reason_type: "closure_invalid",
            authority_event_id: Some(recovery_authority),
            acceptance_record_id: None,
        },
    )
    .unwrap();
    assert!(
        ready_closure(
            temp.path(),
            ClosureReady {
                closure_id: closure.closure_id,
                implementation_evidence: "must bind first",
                tests_or_gates: "not yet",
                closed_by_commit: None,
            },
        )
        .unwrap_err()
        .to_string()
        .contains("work remediate")
    );
    remediate_work(temp.path(), finding.finding_id).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let recovery_state: (String, i64) = conn
        .query_row(
            r#"
            select d.status, count(epoch.id)
            from finding_remediation_recovery_epochs epoch
            join work_unit_dependencies d on d.id = epoch.dependency_id
            where epoch.work_unit_activation_id = ?1
            "#,
            params![reopened.activation_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(recovery_state, ("resolved".to_string(), 1));
    drop(conn);
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into work_units(project_id,title,status,started_at) values ((select id from projects limit 1),'dependency helper','open',current_timestamp)",
        [],
    )
    .unwrap();
    let dependency_work_id = conn.last_insert_rowid();
    conn.execute(
        "insert into work_unit_dependencies(work_unit_id,depends_on_work_unit_id,dependency_type,reason,status,created_at) values (?1,?2,'blocks','exercise dependency scheduling','open',current_timestamp)",
        params![work.work_unit_id, dependency_work_id],
    )
    .unwrap();
    let dependency_id = conn.last_insert_rowid();
    drop(conn);
    let dependency_blocker = project_status(temp.path()).unwrap().phase_blocker.unwrap();
    assert!(
        dependency_blocker
            .next_action
            .contains(&format!("work activate {dependency_work_id}"))
    );
    let dependency_activation = activate_work(
        temp.path(),
        WorkActivate {
            work_unit_id: dependency_work_id,
            design_version_id: None,
            implementation: false,
            reason: Some("resolve selected remediation dependency"),
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let owner_activation_status: String = conn
        .query_row(
            "select status from work_unit_activations where id=?1",
            params![reopened.activation_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(owner_activation_status, "suspended");
    conn.execute(
        "update work_unit_activations set status='abandoned',completed_at=current_timestamp where id=?1",
        params![dependency_activation.activation_id],
    )
    .unwrap();
    conn.execute(
        "update work_units set status='abandoned',closed_at=current_timestamp where id=?1",
        params![dependency_work_id],
    )
    .unwrap();
    conn.execute(
        "update work_unit_activations set status='active',suspended_by_activation_id=null where id=?1",
        params![reopened.activation_id],
    )
    .unwrap();
    conn.execute(
        "update work_unit_dependencies set status='resolved',resolved_at=current_timestamp where id=?1",
        params![dependency_id],
    )
    .unwrap();
    drop(conn);
    let blocking_plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            review_type: "design_review",
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
    let blocking_run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: blocking_plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("work_unit:1"),
            prompt_deviations: None,
            result_summary: Some("blocking design issue"),
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
    let blocking_finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: blocking_run.review_run_id,
            finding_type: "design_finding",
            severity: "critical",
            description: "mixed blocker takes precedence",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    let mixed_status = project_status(temp.path()).unwrap();
    assert_eq!(
        mixed_status.phase_blocker.unwrap().finding_id,
        Some(blocking_finding.finding_id)
    );
    assert!(mixed_status.finding_remediations.is_empty());
    classify_finding(temp.path(), blocking_finding.finding_id, "invalid").unwrap();
    assert!(project_status(temp.path()).unwrap().phase_blocker.is_none());
    assert!(matches!(
        next_action(temp.path()).unwrap(),
        NextAction::FindingRemediation { .. }
    ));
    let attempt = ready_closure(
        temp.path(),
        ClosureReady {
            closure_id: closure.closure_id,
            implementation_evidence: "changed review.rs",
            tests_or_gates: "cargo test passes",
            closed_by_commit: Some("abc123"),
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let ready_evidence: (String, Option<String>, String, String) = conn
        .query_row(
            "select c.tests_or_gates, c.implementation_evidence, a.tests_or_gates, a.implementation_evidence from closures c join closure_attempts a on a.closure_id = c.id where c.id = ?1 and a.id = ?2",
            params![closure.closure_id, attempt.attempt_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        ready_evidence,
        (
            "cargo test".to_string(),
            None,
            "cargo test passes".to_string(),
            "changed review.rs".to_string(),
        )
    );
    drop(conn);
    let context = render_finding_fix_context(
        temp.path(),
        finding.finding_id,
        closure.closure_id,
        attempt.attempt_id,
    )
    .unwrap();
    assert!(context.text.contains("contract_tests_or_gates: cargo test"));
    assert!(
        context
            .text
            .contains("attempt_tests_or_gates: cargo test passes")
    );
    assert!(project_status(temp.path()).unwrap().phase_blocker.is_some());
    let wrong = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some("finding:1"),
            prompt_deviations: None,
            result_summary: Some("verified"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: Some("reviewer"),
            external_agent_id: Some("reviewer-2"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:2"),
        },
        Some("verified"),
    );
    assert!(wrong.is_err());
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update closure_attempts set review_run_high_watermark = 999 where id = ?1",
        params![attempt.attempt_id],
    )
    .unwrap();
    drop(conn);
    let stale_high_watermark = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some(&attempt.context_ref),
            prompt_deviations: None,
            result_summary: Some("stale review"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: Some("reviewer"),
            external_agent_id: Some("reviewer-stale"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:stale"),
        },
        Some("verified"),
    );
    assert!(stale_high_watermark.is_err());
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update closure_attempts set review_run_high_watermark = ?1 where id = ?2",
        params![fresh.review_run_id, attempt.attempt_id],
    )
    .unwrap();
    drop(conn);
    let failed_resume = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some(&attempt.context_ref),
            prompt_deviations: None,
            result_summary: Some("not fixed"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: false,
            status: "completed",
            agent_label: Some("reviewer"),
            external_agent_id: Some("reviewer-2"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:2"),
        },
        Some("not_fixed"),
    )
    .unwrap();
    let conflicting = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some(&attempt.context_ref),
            prompt_deviations: None,
            result_summary: Some("verified"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: Some("reviewer"),
            external_agent_id: Some("reviewer-3"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:3"),
        },
        Some("verified"),
    );
    assert!(
        conflicting
            .unwrap_err()
            .to_string()
            .contains("already has a conflicting resume outcome")
    );
    add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some(&attempt.context_ref),
            prompt_deviations: None,
            result_summary: Some("second reviewer also found it not fixed"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: false,
            status: "completed",
            agent_label: Some("reviewer"),
            external_agent_id: Some("reviewer-3"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:3b"),
        },
        Some("not_fixed"),
    )
    .unwrap();
    let blocker = project_status(temp.path()).unwrap().phase_blocker.unwrap();
    assert!(blocker.next_action.contains("--result not_fixed"));
    let persisted_result = list_review_runs(temp.path(), Some(plan.review_plan_id))
        .unwrap()
        .into_iter()
        .find(|run| run.id == failed_resume.review_run_id)
        .unwrap()
        .finding_fix_result;
    assert_eq!(persisted_result.as_deref(), Some("not_fixed"));
    add_finding_verification(
        temp.path(),
        NewFindingVerification {
            review_run_id: failed_resume.review_run_id,
            finding_id: finding.finding_id,
            closure_id: closure.closure_id,
            result: "not_fixed",
            notes: None,
        },
    )
    .unwrap();
    assert_eq!(list_findings(temp.path(), None).unwrap()[0].status, "open");
    assert!(matches!(
        next_action(temp.path()).unwrap(),
        NextAction::FindingRemediation { .. }
    ));
    let retry_attempt = ready_closure(
        temp.path(),
        ClosureReady {
            closure_id: closure.closure_id,
            implementation_evidence: "changed review.rs again",
            tests_or_gates: "cargo test passes after retry",
            closed_by_commit: Some("def456"),
        },
    )
    .unwrap();
    assert_ne!(retry_attempt.attempt_id, attempt.attempt_id);
    assert_eq!(retry_attempt.attempt_number, 2);
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let retry_tests: (String, String) = conn
        .query_row(
            "select c.tests_or_gates, a.tests_or_gates from closures c join closure_attempts a on a.closure_id = c.id where c.id = ?1 and a.id = ?2",
            params![closure.closure_id, retry_attempt.attempt_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        retry_tests,
        (
            "cargo test".to_string(),
            "cargo test passes after retry".to_string()
        )
    );
    drop(conn);
    let verified_resume = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some(&retry_attempt.context_ref),
            prompt_deviations: None,
            result_summary: Some("verified after retry"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: Some("reviewer"),
            external_agent_id: Some("reviewer-2"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:4"),
        },
        Some("verified"),
    )
    .unwrap();
    add_finding_verification(
        temp.path(),
        NewFindingVerification {
            review_run_id: verified_resume.review_run_id,
            finding_id: finding.finding_id,
            closure_id: closure.closure_id,
            result: "verified",
            notes: None,
        },
    )
    .unwrap();
    assert_eq!(
        list_findings(temp.path(), None).unwrap()[0].status,
        "closed"
    );
    assert!(classify_finding(temp.path(), finding.finding_id, "needs_evidence").is_err());
    add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("work_unit:1"),
            prompt_deviations: None,
            result_summary: Some("final clean"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: Some("reviewer"),
            external_agent_id: Some("reviewer-3"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:3"),
        },
    )
    .unwrap();
    assert_eq!(list_review_plans(temp.path()).unwrap()[0].status, "clean");
}

#[test]
fn zero_resume_quota_still_allows_exactly_one_required_attempt_review() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "zero quota remediation", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "zero-resume-quota",
            review_type: "implementation_review",
            max_fresh_agents: 1,
            max_resume_agents: 0,
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
    let source = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("work_unit:1"),
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
            review_run_id: source.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "zero quota finding",
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
            design_invariant: "required verification still runs",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some("src/review.rs"),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("fix issue"),
            tests_or_gates: Some("cargo test"),
            verification_plan: Some("one required resume"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    remediate_work(temp.path(), finding.finding_id).unwrap();
    let attempt = ready_closure(
        temp.path(),
        ClosureReady {
            closure_id: closure.closure_id,
            implementation_evidence: "fixed",
            tests_or_gates: "tests pass",
            closed_by_commit: None,
        },
    )
    .unwrap();
    let run = || NewReviewRun {
        review_plan_id: plan.review_plan_id,
        run_type: "resume",
        run_purpose: "finding_fix_verification",
        target_ref: Some(attempt.context_ref.as_str()),
        prompt_deviations: None,
        result_summary: Some("verified"),
        new_findings_count: 0,
        carried_findings_checked: 1,
        clean_run: true,
        status: "completed",
        agent_label: Some("reviewer"),
        external_agent_id: None,
        review_provenance: "human_review",
        review_provenance_ref: Some("human-review:1"),
    };
    let verified =
        add_review_run_with_finding_result(temp.path(), run(), Some("verified")).unwrap();
    let exceeded =
        add_review_run_with_finding_result(temp.path(), run(), Some("verified")).unwrap_err();
    assert!(exceeded.to_string().contains("limit exceeded"));
    add_finding_verification(
        temp.path(),
        NewFindingVerification {
            review_run_id: verified.review_run_id,
            finding_id: finding.finding_id,
            closure_id: closure.closure_id,
            result: "verified",
            notes: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update review_policies set max_fresh_agents = 0 where id = ?1",
        params![policy.review_policy_id],
    )
    .unwrap();
    drop(conn);
    let fresh_run = || NewReviewRun {
        review_plan_id: plan.review_plan_id,
        run_type: "fresh",
        run_purpose: "new_unbiased_review",
        target_ref: Some("work_unit:1"),
        prompt_deviations: None,
        result_summary: Some("final clean"),
        new_findings_count: 0,
        carried_findings_checked: 0,
        clean_run: true,
        status: "completed",
        agent_label: Some("fresh-reviewer"),
        external_agent_id: Some("fresh-reviewer"),
        review_provenance: "external_agent",
        review_provenance_ref: Some("fresh-review:1"),
    };
    add_review_run(temp.path(), fresh_run()).unwrap();
    let fresh_exceeded = add_review_run(temp.path(), fresh_run()).unwrap_err();
    assert!(fresh_exceeded.to_string().contains("limit exceeded"));
}
