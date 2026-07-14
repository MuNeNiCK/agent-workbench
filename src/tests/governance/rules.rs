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
fn work_unit_rule_shadows_project_rule_of_same_kind() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "scoped correction", Some("cleanup")).unwrap();
    let project_rule = add_user_correction(
        temp.path(),
        NewUserCorrection {
            scope: "project",
            correction_type: "process",
            mistake_pattern: "generic validation",
            correction: "use project default",
            applies_to: "project",
            severity: "medium",
        },
    )
    .unwrap();
    let work_rule = add_user_correction(
        temp.path(),
        NewUserCorrection {
            scope: &work.work_unit_id.to_string(),
            correction_type: "process",
            mistake_pattern: "generic validation",
            correction: "use work-specific validation",
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
    let winner = rules
        .iter()
        .find(|rule| rule.user_correction_id == Some(work_rule.user_correction_id))
        .unwrap();
    let shadowed = rules
        .iter()
        .find(|rule| rule.user_correction_id == Some(project_rule.user_correction_id))
        .unwrap();

    assert!(winner.shadowed_by_rule_id.is_none());
    assert_eq!(shadowed.shadowed_by_rule_id, Some(winner.id));
}

#[test]
fn current_scope_rules_include_approved_design_package_rules() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    start_work(temp.path(), "design governed work", None).unwrap();
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
    approve_design_version(
        temp.path(),
        DesignVersionApproval {
            design_version_id: imported.design_version_id,
            summary: Some("standing design constraint"),
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
        rule.rule_source_type == "authority_event"
            && rule.scope_type == "design_package"
            && rule.scope_key.as_deref() == Some("storage-lifecycle")
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
