use super::*;

#[test]
fn review_plan_waiver_skips_non_current_design_plan() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "current design waiver routing", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "implement storage",
            priority: "high",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("storage is implemented"),
        },
    )
    .unwrap();
    let package = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "waiver-routing",
            title: "Waiver Routing",
        },
    )
    .unwrap();
    fs::write(
        package.package_path.join("requirements/README.md"),
        requirement_doc("storage", "Preserve storage", "high"),
    )
    .unwrap();
    let first = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &package.package_path,
            status: "draft",
        },
    )
    .unwrap();
    approve_design_version(
        temp.path(),
        DesignVersionApproval {
            design_version_id: first.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: first.design_version_id,
            requirement_key: "storage",
            task_id: task.task_id,
            derivation_reason: None,
            checklist_title: None,
            item_title: None,
            completion_condition: None,
        },
    )
    .unwrap();
    fs::write(
        package.package_path.join("01-introduction-goals.md"),
        "# Introduction And Goals\n\nCurrent revision.\n",
    )
    .unwrap();
    let current = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &package.package_path,
            status: "draft",
        },
    )
    .unwrap();
    approve_design_version(
        temp.path(),
        DesignVersionApproval {
            design_version_id: current.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: current.design_version_id,
            requirement_key: "storage",
            task_id: task.task_id,
            derivation_reason: None,
            checklist_title: None,
            item_title: None,
            completion_condition: None,
        },
    )
    .unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "current-design-waiver",
            review_type: "design_implementation_diff",
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
    let plan = |design_version_id, scope| {
        add_review_plan(
            temp.path(),
            NewReviewPlan {
                work_unit_id: work.work_unit_id,
                design_version_id: Some(design_version_id),
                review_type: "design_implementation_diff",
                required: true,
                stage: "close-ready",
                scope: Some(scope),
                clean_condition: None,
                stop_condition: None,
                review_policy_id: Some(policy.review_policy_id),
                review_scope_id: None,
            },
        )
        .unwrap()
    };
    let obsolete = plan(first.design_version_id, "obsolete design");
    let selected = plan(current.design_version_id, "current design");
    let authority = approval_authority_event(temp.path());

    let waived = waive_review_plan(
        temp.path(),
        ReviewPlanWaiver {
            review_plan_id: selected.review_plan_id,
            reason: "current plan exception",
            approval_authority_event_id: authority,
        },
    )
    .unwrap();
    assert_eq!(waived.review_plan_id, selected.review_plan_id);
    assert!(
        waive_review_plan(
            temp.path(),
            ReviewPlanWaiver {
                review_plan_id: obsolete.review_plan_id,
                reason: "obsolete plan must not be selected",
                approval_authority_event_id: authority,
            },
        )
        .unwrap_err()
        .to_string()
        .contains("review plan waive is not selected")
    );
}

#[test]
fn review_plan_waiver_skips_earlier_clean_plan() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "review waiver routing", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "waiver-routing",
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
    let plan = |scope| {
        add_review_plan(
            temp.path(),
            NewReviewPlan {
                work_unit_id: work.work_unit_id,
                design_version_id: None,
                review_type: "design_review",
                required: true,
                stage: "design-ready",
                scope: Some(scope),
                clean_condition: None,
                stop_condition: None,
                review_policy_id: Some(policy.review_policy_id),
                review_scope_id: None,
            },
        )
        .unwrap()
    };
    let clean = plan("completed review");
    let open = plan("exhausted review");
    add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: clean.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("clean-context"),
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

    let waived = waive_review_plan(
        temp.path(),
        ReviewPlanWaiver {
            review_plan_id: open.review_plan_id,
            reason: "review capacity exhausted",
            approval_authority_event_id: approval_authority_event(temp.path()),
        },
    )
    .unwrap();

    assert_eq!(waived.review_plan_id, open.review_plan_id);
}

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
