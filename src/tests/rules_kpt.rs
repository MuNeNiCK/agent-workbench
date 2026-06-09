use super::*;

#[test]
fn authority_events_create_applicable_rule_bindings() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();

    let authority = add_authority_event(
        temp.path(),
        NewAuthorityEvent {
            event_type: "user_instruction",
            source: Some("chat"),
            summary: "do not store local design in tracked README",
            scope: Some("project"),
            precedence: 100,
        },
    )
    .unwrap();
    let events = list_authority_events(temp.path(), Some("project")).unwrap();
    let authorities = list_authorities(temp.path(), Some("project")).unwrap();
    let rules = applicable_rules(
        temp.path(),
        RuleQuery {
            scope_key: Some("project"),
            work_unit_id: None,
        },
    )
    .unwrap();

    assert!(
        events
            .iter()
            .any(|event| event.id == authority.authority_event_id)
    );
    assert!(
        authorities
            .iter()
            .any(|record| record.id == authority.authority_id
                && record.authority_type == "user"
                && record.path_or_label == "chat")
    );
    assert!(
        rules
            .iter()
            .any(|rule| rule.authority_event_id == Some(authority.authority_event_id))
    );
}

#[test]
fn current_scope_rules_include_active_work_unit_rules() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "scoped work", Some("storage")).unwrap();
    let scope = work.work_unit_id.to_string();
    let correction = add_user_correction(
        temp.path(),
        NewUserCorrection {
            scope: &scope,
            correction_type: "process",
            mistake_pattern: "skip scoped rule",
            correction: "load active work rules",
            applies_to: "current_work_unit",
            severity: "medium",
        },
    )
    .unwrap();

    let rules = applicable_rules(
        temp.path(),
        RuleQuery {
            scope_key: Some("current"),
            work_unit_id: None,
        },
    )
    .unwrap();

    assert!(
        rules
            .iter()
            .any(|rule| rule.user_correction_id == Some(correction.user_correction_id))
    );
    assert!(rules.iter().any(|rule| {
        rule.rule_source_type == "work_unit" && rule.work_unit_id == Some(work.work_unit_id)
    }));
}

#[test]
fn current_scope_rules_include_active_work_responsibility_scope() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    start_work(temp.path(), "cleanup work", Some("cleanup")).unwrap();
    let command = add_fixed_command(
        temp.path(),
        NewCommandProfile {
            name: "cleanup-test",
            command_type: "validation",
            scope: "cleanup",
            command: "cargo test cleanup",
            timeout: Some("120s"),
            expected_result: Some("pass"),
        },
    )
    .unwrap();

    let current_rules = applicable_rules(
        temp.path(),
        RuleQuery {
            scope_key: Some("current"),
            work_unit_id: None,
        },
    )
    .unwrap();

    assert!(current_rules.iter().any(|rule| {
        rule.command_profile_id == Some(command.command_profile_id)
            && rule.scope_key.as_deref() == Some("cleanup")
    }));
}

#[test]
fn unrelated_project_rules_are_not_shadowed() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let first = add_user_correction(
        temp.path(),
        NewUserCorrection {
            scope: "project",
            correction_type: "process",
            mistake_pattern: "skip status",
            correction: "check status first",
            applies_to: "project",
            severity: "medium",
        },
    )
    .unwrap();
    let second = add_user_correction(
        temp.path(),
        NewUserCorrection {
            scope: "project",
            correction_type: "command",
            mistake_pattern: "vary command",
            correction: "use fixed command",
            applies_to: "project",
            severity: "medium",
        },
    )
    .unwrap();

    let rules = applicable_rules(
        temp.path(),
        RuleQuery {
            scope_key: Some("project"),
            work_unit_id: None,
        },
    )
    .unwrap();

    assert!(rules.iter().any(|rule| {
        rule.user_correction_id == Some(first.user_correction_id)
            && rule.shadowed_by_rule_id.is_none()
    }));
    assert!(rules.iter().any(|rule| {
        rule.user_correction_id == Some(second.user_correction_id)
            && rule.shadowed_by_rule_id.is_none()
    }));
}

#[test]
fn current_scope_rules_include_review_policy_and_validation_gate_rules() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "design linked work", Some("storage")).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "implement storage",
            priority: "high",
            source: "design",
            work_unit_id: None,
            details: None,
            completion_condition: Some("storage behavior is covered"),
        },
    )
    .unwrap();
    let design = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "storage-lifecycle",
            title: "Storage Lifecycle",
        },
    )
    .unwrap();
    fs::write(
        design.package_path.join("requirements").join("README.md"),
        requirement_doc("REQ-001", "Preserve storage behavior", "high"),
    )
    .unwrap();
    fs::write(
        design.package_path.join("validation").join("gates.md"),
        validation_gate_doc("GATE-001"),
    )
    .unwrap();
    let imported = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &design.package_path,
            status: "draft",
        },
    )
    .unwrap();
    derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: imported.design_version_id,
            requirement_key: "REQ-001",
            task_id: task.task_id,
            derivation_reason: None,
            checklist_title: None,
            item_title: None,
            completion_condition: None,
        },
    )
    .unwrap();
    let gate = select_validation_gate(
        temp.path(),
        ValidationGateSelection {
            design_version_id: imported.design_version_id,
            gate_key: "GATE-001",
            requirement_key: "REQ-001",
            task_id: task.task_id,
            command: None,
            command_profile: None,
            timeout: None,
        },
    )
    .unwrap();
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(imported.design_version_id),
            review_type: "design_implementation_diff",
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

    let rules = applicable_rules(
        temp.path(),
        RuleQuery {
            scope_key: Some("current"),
            work_unit_id: None,
        },
    )
    .unwrap();

    assert!(rules.iter().any(|rule| {
        rule.rule_source_type == "validation_gate"
            && rule.validation_gate_id == Some(gate.validation_gate_id)
    }));
    assert!(rules.iter().any(|rule| {
        rule.rule_source_type == "review_policy"
            && rule.review_policy_id == plan.review_policy_id
            && rule.review_plan_id == Some(plan.review_plan_id)
    }));
}

#[test]
fn kpt_item_can_convert_to_task() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();

    let review = start_kpt_review(
        temp.path(),
        NewKptReview {
            scope: Some("project"),
            summary: Some("monthly review"),
            from: None,
            period: None,
        },
    )
    .unwrap();
    let item = add_kpt_item(
        temp.path(),
        NewKptItem {
            kpt_review_id: Some(review.kpt_review_id),
            item_type: "try",
            title: "stabilize validation command",
            details: Some("command drift keeps happening"),
            severity: "high",
            proposed_action: Some("fix command profile"),
        },
    )
    .unwrap();
    let conversion = convert_kpt_item_to_task(
        temp.path(),
        KptItemTaskConversion {
            kpt_item_id: item.kpt_item_id,
            task_title: None,
            details: None,
            priority: "high",
            work_unit_id: None,
        },
    )
    .unwrap();
    let tasks = list_tasks(
        temp.path(),
        TaskListQuery {
            status: Some("open"),
            work_unit_id: None,
        },
    )
    .unwrap();

    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].id, conversion.task_id);
    assert_eq!(tasks[0].title, "stabilize validation command");

    let reviews = list_kpt_reviews(temp.path(), Some("open")).unwrap();
    let items = list_kpt_items(temp.path(), Some(review.kpt_review_id)).unwrap();
    assert_eq!(reviews.len(), 1);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].status, "converted");
    assert_eq!(items[0].linked_task_id, Some(conversion.task_id));
}

#[test]
fn kpt_review_can_import_user_corrections() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    add_user_correction(
        temp.path(),
        NewUserCorrection {
            scope: "project",
            correction_type: "process",
            mistake_pattern: "validation command drifts",
            correction: "use fixed validation command",
            applies_to: "project",
            severity: "high",
        },
    )
    .unwrap();

    let review = start_kpt_review(
        temp.path(),
        NewKptReview {
            scope: Some("project"),
            summary: Some("process review"),
            from: Some("corrections"),
            period: Some("30d"),
        },
    )
    .unwrap();

    let items = list_kpt_items(temp.path(), Some(review.kpt_review_id)).unwrap();
    assert_eq!(review.generated_item_count, 1);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].item_type, "problem");
    assert_eq!(
        items[0].title,
        "Repeated correction: validation command drifts"
    );
}

#[test]
fn kpt_review_rejects_unknown_sources() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();

    let result = start_kpt_review(
        temp.path(),
        NewKptReview {
            scope: Some("project"),
            summary: Some("bad source"),
            from: Some("unknown-source"),
            period: None,
        },
    );

    assert!(result.is_err());
}

#[test]
fn kpt_review_can_import_command_drift() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    add_fixed_command(
        temp.path(),
        NewCommandProfile {
            name: "validation",
            command_type: "test",
            scope: "project",
            command: "cargo test",
            timeout: None,
            expected_result: Some("pass"),
        },
    )
    .unwrap();
    let usage = add_command_usage(
        temp.path(),
        NewCommandUsage {
            profile: Some("validation"),
            command: None,
            result: "fail",
            log_path: None,
            work_unit_id: None,
        },
    )
    .unwrap();
    add_command_deviation(
        temp.path(),
        NewCommandDeviation {
            profile: "validation",
            command_usage_id: Some(usage.command_usage_id),
            reason: "workspace needs nextest command",
        },
    )
    .unwrap();

    let kpt = start_kpt_review(
        temp.path(),
        NewKptReview {
            scope: Some("project"),
            summary: Some("command drift"),
            from: Some("commands"),
            period: None,
        },
    )
    .unwrap();

    let items = list_kpt_items(temp.path(), Some(kpt.kpt_review_id)).unwrap();
    assert_eq!(kpt.generated_item_count, 1);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].item_type, "problem");
    assert_eq!(items[0].title, "Command drift: validation");

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let linked_profile: i64 = conn
        .query_row(
            "select linked_command_profile_id from kpt_items where id = ?1",
            params![items[0].id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(linked_profile, 1);
}

#[test]
fn kpt_review_can_import_review_and_work_outcomes() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "triage outcomes", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "outcome-import",
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
    add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("HEAD"),
            prompt_deviations: None,
            result_summary: Some("not clean"),
            new_findings_count: 1,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
        },
    )
    .unwrap();
    create_work_record(
        temp.path(),
        NewWorkRecord {
            work_unit_id: Some(work.work_unit_id),
            topic: "outcome follow-up",
            work_performed: Some("captured pending work"),
            next_actions: Some("convert missing check into task"),
            notable_operations: Some("cargo test"),
            export_path: None,
        },
    )
    .unwrap();

    let kpt = start_kpt_review(
        temp.path(),
        NewKptReview {
            scope: Some("project"),
            summary: Some("outcome import"),
            from: Some("review-runs,work-records"),
            period: None,
        },
    )
    .unwrap();

    let items = list_kpt_items(temp.path(), Some(kpt.kpt_review_id)).unwrap();
    assert_eq!(kpt.generated_item_count, 2);
    assert_eq!(items.len(), 2);
    assert!(
        items
            .iter()
            .any(|item| item.title == "Review outcome: fresh 1")
    );
    assert!(
        items
            .iter()
            .any(|item| item.title == "Work outcome: outcome follow-up")
    );
}

#[test]
fn kpt_review_can_import_open_findings() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "fix error handling", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "implementation-quality",
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
            stage: "implementation-ready",
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
            result_summary: Some("found a gap"),
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
            severity: "high",
            description: "missing error context",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();

    let kpt = start_kpt_review(
        temp.path(),
        NewKptReview {
            scope: Some("project"),
            summary: Some("finding triage"),
            from: Some("findings"),
            period: None,
        },
    )
    .unwrap();

    let items = list_kpt_items(temp.path(), Some(kpt.kpt_review_id)).unwrap();
    assert_eq!(kpt.generated_item_count, 1);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].item_type, "problem");
    assert_eq!(items[0].severity, "high");
    assert_eq!(items[0].title, "Review finding: missing error context");

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let linked_finding: i64 = conn
        .query_row(
            "select linked_review_finding_id from kpt_items where id = ?1",
            params![items[0].id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(linked_finding, 1);
}

#[test]
fn kpt_review_period_filters_findings() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "old finding", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "finding-period",
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
            target_ref: Some("HEAD"),
            prompt_deviations: None,
            result_summary: Some("old finding"),
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
            description: "old finding",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update findings set created_at = datetime('now', '-90 days') where id = ?1",
        params![finding.finding_id],
    )
    .unwrap();

    let kpt = start_kpt_review(
        temp.path(),
        NewKptReview {
            scope: Some("project"),
            summary: Some("recent findings only"),
            from: Some("findings"),
            period: Some("30d"),
        },
    )
    .unwrap();

    let items = list_kpt_items(temp.path(), Some(kpt.kpt_review_id)).unwrap();
    assert_eq!(kpt.generated_item_count, 0);
    assert!(items.is_empty());
}

#[test]
fn kpt_item_can_convert_to_fixed_command_profile() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let kpt = start_kpt_review(
        temp.path(),
        NewKptReview {
            scope: Some("project"),
            summary: Some("command profile"),
            from: None,
            period: None,
        },
    )
    .unwrap();
    let item = add_kpt_item(
        temp.path(),
        NewKptItem {
            kpt_review_id: Some(kpt.kpt_review_id),
            item_type: "try",
            title: "stable validation command",
            details: Some("cargo test --workspace"),
            severity: "medium",
            proposed_action: None,
        },
    )
    .unwrap();

    let conversion = convert_kpt_item_to_command_profile(
        temp.path(),
        KptItemCommandProfileConversion {
            kpt_item_id: item.kpt_item_id,
            name: Some("workspace-tests"),
            command: None,
            command_type: "test",
            scope: None,
            status: "fixed",
            stability: "stable",
            timeout: Some("120s"),
            expected_result: Some("pass"),
        },
    )
    .unwrap();

    let commands = list_command_profiles(temp.path(), Some("test")).unwrap();
    let rules = applicable_rules(
        temp.path(),
        RuleQuery {
            scope_key: Some("project"),
            work_unit_id: None,
        },
    )
    .unwrap();
    let items = list_kpt_items(temp.path(), Some(kpt.kpt_review_id)).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let conversion_target: (String, i64) = conn
        .query_row(
            "select target_type, command_profile_id from kpt_item_conversions where id = ?1",
            params![conversion.kpt_item_conversion_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].id, conversion.command_profile_id);
    assert_eq!(commands[0].name, "workspace-tests");
    assert_eq!(commands[0].command, "cargo test --workspace");
    assert_eq!(commands[0].status, "fixed");
    assert!(
        rules
            .iter()
            .any(|rule| rule.command_profile_id == Some(conversion.command_profile_id))
    );
    assert_eq!(items[0].status, "converted");
    assert_eq!(conversion_target, ("command_profile".to_string(), 1));
}

#[test]
fn kpt_item_can_convert_to_policy_decision_and_design_version() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let review = start_kpt_review(
        temp.path(),
        NewKptReview {
            scope: Some("project"),
            summary: Some("process tuning"),
            from: None,
            period: None,
        },
    )
    .unwrap();
    let policy_item = add_kpt_item(
        temp.path(),
        NewKptItem {
            kpt_review_id: Some(review.kpt_review_id),
            item_type: "try",
            title: "require two clean implementation passes",
            details: Some("one clean pass missed regressions"),
            severity: "medium",
            proposed_action: Some("tighten implementation quality checks"),
        },
    )
    .unwrap();
    let decision_item = add_kpt_item(
        temp.path(),
        NewKptItem {
            kpt_review_id: Some(review.kpt_review_id),
            item_type: "keep",
            title: "fixed validation command",
            details: Some("cargo fmt && cargo test && cargo clippy --all-targets -- -D warnings"),
            severity: "medium",
            proposed_action: None,
        },
    )
    .unwrap();
    let design_item = add_kpt_item(
        temp.path(),
        NewKptItem {
            kpt_review_id: Some(review.kpt_review_id),
            item_type: "problem",
            title: "design package needs explicit trace gate",
            details: None,
            severity: "high",
            proposed_action: None,
        },
    )
    .unwrap();
    let init = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "trace-gates",
            title: "Trace Gates",
        },
    )
    .unwrap();
    fs::write(
        init.package_path.join("requirements").join("README.md"),
        requirement_doc("REQ-001", "Preserve trace gate behavior", "high"),
    )
    .unwrap();
    fs::write(
        init.package_path.join("validation").join("gates.md"),
        validation_gate_doc("GATE-001"),
    )
    .unwrap();
    let imported = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &init.package_path,
            status: "draft",
        },
    )
    .unwrap();

    let policy = convert_kpt_item_to_review_policy(
        temp.path(),
        KptItemReviewPolicyConversion {
            kpt_item_id: policy_item.kpt_item_id,
            name: Some("two-clean-implementation-passes"),
            review_type: "implementation_review",
            max_fresh_agents: 2,
            max_resume_agents: 1,
            max_parallel_agents: 1,
            required_consecutive_clean_fresh_runs: 2,
            required_consecutive_clean_resume_runs: 0,
            stop_on_severity: "none",
            allow_new_findings_in_resume: true,
            run_count_scope: "work_unit",
            default_run_mode: "resume",
            on_max_agents_exceeded: "block",
        },
    )
    .unwrap();
    let decision = convert_kpt_item_to_decision(
        temp.path(),
        KptItemDecisionConversion {
            kpt_item_id: decision_item.kpt_item_id,
            decision_key: Some("DEC-KPT-001"),
            topic: Some("validation command"),
            decision: None,
            rationale: Some("avoid command drift"),
            compatibility_impact: None,
            authority_refs: None,
        },
    )
    .unwrap();
    let design = convert_kpt_item_to_design_version(
        temp.path(),
        KptItemDesignVersionConversion {
            kpt_item_id: design_item.kpt_item_id,
            design_version_id: imported.design_version_id,
        },
    )
    .unwrap();

    assert_eq!(policy.review_policy_id, 1);
    assert_eq!(decision.decision_id, 1);
    assert_eq!(design.design_version_id, imported.design_version_id);

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let policy_settings: (i64, String, String) = conn
        .query_row(
            r#"
            select allow_new_findings_in_resume, run_count_scope, default_run_mode
            from review_policies
            where id = ?1
            "#,
            params![policy.review_policy_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    let conversion_count: i64 = conn
        .query_row("select count(*) from kpt_item_conversions", [], |row| {
            row.get(0)
        })
        .unwrap();
    let converted_count: i64 = conn
        .query_row(
            "select count(*) from kpt_items where status = 'converted'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        policy_settings,
        (1, "work_unit".to_string(), "resume".to_string())
    );
    assert_eq!(conversion_count, 3);
    assert_eq!(converted_count, 3);
}

#[test]
fn kpt_item_conversions_enforce_typed_targets() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let review = start_kpt_review(
        temp.path(),
        NewKptReview {
            scope: Some("project"),
            summary: Some("typed conversion"),
            from: None,
            period: None,
        },
    )
    .unwrap();
    let item = add_kpt_item(
        temp.path(),
        NewKptItem {
            kpt_review_id: Some(review.kpt_review_id),
            item_type: "try",
            title: "typed target",
            details: None,
            severity: "medium",
            proposed_action: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();

    let missing_target = conn.execute(
        r#"
        insert into kpt_item_conversions(kpt_item_id, target_type, created_at)
        values (?1, 'decision', current_timestamp)
        "#,
        params![item.kpt_item_id],
    );
    let wrong_target = conn.execute(
        r#"
        insert into kpt_item_conversions(kpt_item_id, target_type, task_id, decision_id, created_at)
        values (?1, 'task', 1, 1, current_timestamp)
        "#,
        params![item.kpt_item_id],
    );

    assert!(missing_target.is_err());
    assert!(wrong_target.is_err());
}

#[test]
fn kpt_item_conversions_reject_cross_project_targets() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let review = start_kpt_review(
        temp.path(),
        NewKptReview {
            scope: Some("project"),
            summary: Some("project scoped kpt"),
            from: None,
            period: None,
        },
    )
    .unwrap();
    let item = add_kpt_item(
        temp.path(),
        NewKptItem {
            kpt_review_id: Some(review.kpt_review_id),
            item_type: "try",
            title: "cross project conversion",
            details: None,
            severity: "medium",
            proposed_action: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into projects(name, root_path, created_at, updated_at) values ('other', '/tmp/other-awb-kpt', current_timestamp, current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        r#"
        insert into review_policies(
            project_id, name, review_type, max_fresh_agents, max_resume_agents,
            max_parallel_agents, required_consecutive_clean_fresh_runs,
            required_consecutive_clean_resume_runs, stop_on_severity,
            allow_resume_review, allow_fresh_review, allow_new_findings_in_resume,
            on_max_agents_exceeded, run_count_scope, default_run_mode, created_at
        )
        values (2, 'other-policy', 'implementation_review', 1, 1, 1, 1, 0, 'none', 1, 1, 0, 'block', 'review_plan', 'fresh', current_timestamp)
        "#,
        [],
    )
    .unwrap();

    let cross_project = conn.execute(
        r#"
        insert into kpt_item_conversions(kpt_item_id, target_type, review_policy_id, created_at)
        values (?1, 'review_policy', 1, current_timestamp)
        "#,
        params![item.kpt_item_id],
    );

    assert!(cross_project.is_err());
}
