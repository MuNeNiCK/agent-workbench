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
