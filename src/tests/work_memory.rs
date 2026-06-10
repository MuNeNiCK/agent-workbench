use super::*;

#[test]
fn interrupt_blocks_parent_until_child_is_closed() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let parent = start_work(temp.path(), "parent", None).unwrap();

    let interrupt = interrupt_work(temp.path(), "child", "blocks parent").unwrap();
    let blocked = resume_check_basic(temp.path()).unwrap();
    record_close_evidence(
        temp.path(),
        interrupt.child_work_unit_id,
        interrupt.child_activation_id,
    );
    close_active_work(temp.path(), "child done", None).unwrap();
    let allowed = resume_check_basic(temp.path()).unwrap();
    let resumed = resume_work(temp.path(), allowed.resume_check_id).unwrap();

    assert_eq!(interrupt.parent_work_unit_id, parent.work_unit_id);
    assert_eq!(blocked.result, "blocked");
    assert_eq!(
        blocked.blocking_reason.as_deref(),
        Some("deeper activation frames must be completed or abandoned")
    );
    assert_eq!(allowed.result, "allowed");
    assert_eq!(resumed.activation_id, parent.activation_id);
}

#[test]
fn correction_creates_applicable_rule_binding() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();

    let correction = add_user_correction(
        temp.path(),
        NewUserCorrection {
            scope: "project",
            correction_type: "process",
            mistake_pattern: "write design to README",
            correction: "keep local design under local/",
            applies_to: "project",
            severity: "high",
        },
    )
    .unwrap();

    let corrections = list_user_corrections(temp.path(), Some("project")).unwrap();
    let rules = applicable_rules(
        temp.path(),
        RuleQuery {
            scope_key: Some("project"),
            work_unit_id: None,
        },
    )
    .unwrap();

    assert_eq!(corrections.len(), 1);
    assert_eq!(corrections[0].id, correction.user_correction_id);
    assert_eq!(
        rules
            .iter()
            .find(|rule| rule.user_correction_id == Some(correction.user_correction_id))
            .map(|rule| rule.rule_source_type.as_str()),
        Some("user_correction")
    );
}

#[test]
fn close_ready_allows_approved_repeated_correction_deferral() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    start_work(temp.path(), "apply remembered process", None).unwrap();
    let first = add_user_correction(
        temp.path(),
        NewUserCorrection {
            scope: "project",
            correction_type: "process",
            mistake_pattern: "repeat stale process",
            correction: "record explicit deferral when not addressing a repeated correction",
            applies_to: "project",
            severity: "high",
        },
    )
    .unwrap();
    add_user_correction(
        temp.path(),
        NewUserCorrection {
            scope: "project",
            correction_type: "process",
            mistake_pattern: "miss fixed commands",
            correction: "use fixed command profiles before closing work",
            applies_to: "project",
            severity: "high",
        },
    )
    .unwrap();

    let blocked = close_ready(temp.path()).unwrap();
    let blocked_item = blocked
        .items
        .iter()
        .find(|item| item.name == "corrections_kpt_checked")
        .unwrap();
    assert_eq!(blocked_item.result, "fail");

    let authority = approval_authority_event(temp.path());
    let target = format!("stale:user_correction:{}", first.user_correction_id);
    add_general_acceptance(
        temp.path(),
        NewGeneralAcceptance {
            target: &target,
            acceptance_type: "stale_accepted",
            reason: "user explicitly deferred this remembered correction",
            approval_authority_event_id: authority,
        },
    )
    .unwrap();

    let allowed = close_ready(temp.path()).unwrap();
    let allowed_item = allowed
        .items
        .iter()
        .find(|item| item.name == "corrections_kpt_checked")
        .unwrap();
    assert_eq!(allowed_item.result, "pass");
}

#[test]
fn close_ready_allows_approved_shadowed_rule_conflict() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "rule governed work", None).unwrap();
    let project_rule = add_user_correction(
        temp.path(),
        NewUserCorrection {
            scope: "project",
            correction_type: "process",
            mistake_pattern: "generic workflow",
            correction: "use the project-level workflow",
            applies_to: "project",
            severity: "medium",
        },
    )
    .unwrap();
    add_user_correction(
        temp.path(),
        NewUserCorrection {
            scope: &work.work_unit_id.to_string(),
            correction_type: "process",
            mistake_pattern: "specific workflow",
            correction: "use the work-unit workflow",
            applies_to: "current_work_unit",
            severity: "medium",
        },
    )
    .unwrap();

    let blocked = close_ready(temp.path()).unwrap();
    let blocked_item = blocked
        .items
        .iter()
        .find(|item| item.name == "rules_checked")
        .unwrap();
    assert_eq!(blocked_item.result, "fail");

    let rules = applicable_rules(
        temp.path(),
        RuleQuery {
            scope_key: Some("current"),
            work_unit_id: None,
        },
    )
    .unwrap();
    let shadowed_rule = rules
        .iter()
        .find(|rule| rule.user_correction_id == Some(project_rule.user_correction_id))
        .unwrap();
    assert!(shadowed_rule.shadowed_by_rule_id.is_some());

    let authority = approval_authority_event(temp.path());
    add_general_acceptance(
        temp.path(),
        NewGeneralAcceptance {
            target: &format!("rule:{}", shadowed_rule.id),
            acceptance_type: "explicit_exception",
            reason: "user accepted the work-unit rule overriding the project rule",
            approval_authority_event_id: authority,
        },
    )
    .unwrap();

    let allowed = close_ready(temp.path()).unwrap();
    let allowed_item = allowed
        .items
        .iter()
        .find(|item| item.name == "rules_checked")
        .unwrap();
    assert_eq!(allowed_item.result, "pass");
}

#[test]
fn fixed_command_creates_command_rule_binding() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();

    let command = add_fixed_command(
        temp.path(),
        NewCommandProfile {
            name: "storage-tests",
            command_type: "test",
            scope: "storage",
            command: "cargo test -p storage",
            timeout: Some("120s"),
            expected_result: Some("pass"),
        },
    )
    .unwrap();

    let commands = list_command_profiles(temp.path(), Some("test")).unwrap();
    let rules = applicable_rules(
        temp.path(),
        RuleQuery {
            scope_key: Some("storage"),
            work_unit_id: None,
        },
    )
    .unwrap();

    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].id, command.command_profile_id);
    assert_eq!(commands[0].status, "fixed");
    assert_eq!(
        rules
            .iter()
            .find(|rule| rule.command_profile_id == Some(command.command_profile_id))
            .map(|rule| rule.rule_source_type.as_str()),
        Some("command_profile")
    );
}

#[test]
fn close_ready_requires_only_fixed_commands_applicable_to_work_scope() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    start_work(temp.path(), "storage work", Some("storage")).unwrap();
    add_fixed_command(
        temp.path(),
        NewCommandProfile {
            name: "docs-tests",
            command_type: "test",
            scope: "docs",
            command: "cargo test -p docs",
            timeout: Some("120s"),
            expected_result: Some("pass"),
        },
    )
    .unwrap();

    let ignored = close_ready(temp.path()).unwrap();
    let ignored_item = ignored
        .items
        .iter()
        .find(|item| item.name == "fixed_commands_used")
        .unwrap();
    assert_eq!(ignored_item.result, "pass");
    assert_eq!(
        ignored_item.details,
        "0 fixed command profiles, 0 missing usage or approved deviation"
    );

    add_fixed_command(
        temp.path(),
        NewCommandProfile {
            name: "storage-tests",
            command_type: "test",
            scope: "storage",
            command: "cargo test -p storage",
            timeout: Some("120s"),
            expected_result: Some("pass"),
        },
    )
    .unwrap();

    let required = close_ready(temp.path()).unwrap();
    let required_item = required
        .items
        .iter()
        .find(|item| item.name == "fixed_commands_used")
        .unwrap();
    assert_eq!(required_item.result, "fail");
    assert_eq!(
        required_item.details,
        "1 fixed command profiles, 1 missing usage or approved deviation"
    );
}

#[test]
fn command_usage_and_deviation_attach_to_profile_and_active_work() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "run validation", None).unwrap();
    let profile = add_fixed_command(
        temp.path(),
        NewCommandProfile {
            name: "unit-tests",
            command_type: "test",
            scope: "project",
            command: "cargo test",
            timeout: Some("120s"),
            expected_result: Some("pass"),
        },
    )
    .unwrap();

    let usage = add_command_usage(
        temp.path(),
        NewCommandUsage {
            profile: Some("unit-tests"),
            command: None,
            result: "pass",
            log_path: Some(".agent-workbench/logs/unit-tests.log"),
            work_unit_id: None,
        },
    )
    .unwrap();
    let deviation = add_command_deviation(
        temp.path(),
        NewCommandDeviation {
            profile: "unit-tests",
            command_usage_id: Some(usage.command_usage_id),
            reason: "platform-specific validation path",
        },
    )
    .unwrap();

    assert_eq!(usage.command_profile_id, Some(profile.command_profile_id));
    assert_eq!(usage.work_unit_id, Some(work.work_unit_id));
    assert_eq!(deviation.command_profile_id, profile.command_profile_id);
    assert_eq!(deviation.work_unit_id, Some(work.work_unit_id));

    let usages = list_command_usages(
        temp.path(),
        CommandUsageListQuery {
            profile: Some("unit-tests"),
            work_unit_id: Some(work.work_unit_id),
        },
    )
    .unwrap();
    assert_eq!(usages.len(), 1);
    assert_eq!(usages[0].command, "cargo test");
}

#[test]
fn command_deviation_rejects_usage_from_another_profile() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    add_fixed_command(
        temp.path(),
        NewCommandProfile {
            name: "unit-tests",
            command_type: "test",
            scope: "project",
            command: "cargo test",
            timeout: None,
            expected_result: None,
        },
    )
    .unwrap();
    add_fixed_command(
        temp.path(),
        NewCommandProfile {
            name: "format",
            command_type: "format",
            scope: "project",
            command: "cargo fmt",
            timeout: None,
            expected_result: None,
        },
    )
    .unwrap();
    let usage = add_command_usage(
        temp.path(),
        NewCommandUsage {
            profile: Some("format"),
            command: None,
            result: "pass",
            log_path: None,
            work_unit_id: None,
        },
    )
    .unwrap();

    let deviation = add_command_deviation(
        temp.path(),
        NewCommandDeviation {
            profile: "unit-tests",
            command_usage_id: Some(usage.command_usage_id),
            reason: "wrong profile",
        },
    );

    assert!(deviation.is_err());
}

#[test]
fn work_records_are_created_and_linked_separately() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let started = start_work(temp.path(), "implement work record ledger", None).unwrap();

    let work_record = create_work_record(
        temp.path(),
        NewWorkRecord {
            work_unit_id: None,
            topic: "work record ledger",
            work_performed: Some("created structured records"),
            next_actions: Some("export records"),
            notable_operations: Some("cargo test"),
            export_path: None,
        },
    )
    .unwrap();
    let command = add_work_record_command(
        temp.path(),
        NewWorkRecordCommand {
            work_record_id: work_record.work_record_id,
            command_usage_id: None,
            command_profile_id: None,
            command: Some("cargo test"),
            result: Some("pass"),
            log_path: None,
            note: Some("verification"),
        },
    )
    .unwrap();
    let usage = add_command_usage(
        temp.path(),
        NewCommandUsage {
            profile: None,
            command: Some("cargo fmt"),
            result: "pass",
            log_path: None,
            work_unit_id: Some(started.work_unit_id),
        },
    )
    .unwrap();
    let usage_link = add_work_record_command(
        temp.path(),
        NewWorkRecordCommand {
            work_record_id: work_record.work_record_id,
            command_usage_id: Some(usage.command_usage_id),
            command_profile_id: None,
            command: None,
            result: None,
            log_path: None,
            note: None,
        },
    )
    .unwrap();
    let commit = add_work_record_commit(
        temp.path(),
        NewWorkRecordCommit {
            work_record_id: work_record.work_record_id,
            commit_sha: "abc123",
            role: "created",
            note: None,
        },
    )
    .unwrap();
    let file = add_work_record_file(
        temp.path(),
        NewWorkRecordFile {
            work_record_id: work_record.work_record_id,
            path: "src/lib.rs",
            role: "changed",
            note: None,
        },
    )
    .unwrap();
    let work_records = list_work_records(temp.path(), Some(started.work_unit_id)).unwrap();

    assert_eq!(work_record.work_unit_id, Some(started.work_unit_id));
    assert_eq!(work_records.len(), 1);
    assert_eq!(work_records[0].topic, "work record ledger");
    assert_eq!(command.link_id, 1);
    assert_eq!(usage_link.link_id, 2);
    assert_eq!(commit.link_id, 1);
    assert_eq!(file.link_id, 1);

    let markdown = export_work_record_markdown(temp.path(), work_record.work_record_id).unwrap();
    assert!(markdown.contains("# work record ledger"));
    assert!(markdown.contains("cargo test -> pass"));
    assert!(markdown.contains("cargo fmt -> pass [usage:1]"));
    assert!(markdown.contains("abc123 [created]"));
    assert!(markdown.contains("src/lib.rs [changed]"));
}

#[test]
fn fork_work_from_record_creates_new_active_work_unit() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let started = start_work(temp.path(), "bad attempt", None).unwrap();
    let record = create_work_record(
        temp.path(),
        NewWorkRecord {
            work_unit_id: None,
            topic: "before drift",
            work_performed: Some("partial implementation"),
            next_actions: Some("redo from this point"),
            notable_operations: None,
            export_path: None,
        },
    )
    .unwrap();
    record_close_evidence(temp.path(), started.work_unit_id, started.activation_id);
    close_active_work(temp.path(), "abandon active line before fork", None).unwrap();

    let fork = fork_work(
        temp.path(),
        NewWorkFork {
            title: "redo after drift",
            source: WorkForkSource::Record(record.work_record_id),
            reason: "agent_drift",
            discard_policy: "keep_history",
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let fork_row: (i64, i64, i64, String) = conn
        .query_row(
            r#"
            select source_work_unit_id, source_work_record_id, forked_work_unit_id, fork_reason
            from work_record_forks
            where id = ?1
            "#,
            params![fork.fork_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();

    assert_eq!(fork_row.0, started.work_unit_id);
    assert_eq!(fork_row.1, record.work_record_id);
    assert_eq!(fork_row.2, fork.work_unit_id);
    assert_eq!(fork_row.3, "agent_drift");
    assert_eq!(
        next_action(temp.path()).unwrap(),
        NextAction::ContinueActive {
            work_unit: ActiveWorkUnit {
                id: fork.work_unit_id,
                title: "redo after drift".to_string(),
                design_version_id: None,
            }
        }
    );
}

#[test]
fn fork_work_refuses_to_run_while_work_is_active() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    start_work(temp.path(), "active", None).unwrap();

    let result = fork_work(
        temp.path(),
        NewWorkFork {
            title: "redo",
            source: WorkForkSource::Commit("abc123"),
            reason: "failed_validation",
            discard_policy: "keep_history",
        },
    );

    assert!(result.is_err());
}

#[test]
fn fork_work_rejects_non_default_discard_policy_before_policy_support() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();

    let fork = fork_work(
        temp.path(),
        NewWorkFork {
            title: "redo",
            source: WorkForkSource::Commit("abc123"),
            reason: "failed_validation",
            discard_policy: "mark_abandoned",
        },
    );

    assert!(fork.is_err());
}

#[test]
fn tasks_attach_to_active_work_and_can_close() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "plan work", None).unwrap();

    let task = add_task(
        temp.path(),
        NewTask {
            title: "write task support",
            priority: "high",
            source: "design",
            work_unit_id: None,
            details: Some("task ledger"),
            completion_condition: Some("task can close"),
        },
    )
    .unwrap();
    close_task(temp.path(), task.task_id, Some("abc123")).unwrap();
    let tasks = list_tasks(
        temp.path(),
        TaskListQuery {
            status: Some("closed"),
            work_unit_id: Some(work.work_unit_id),
        },
    )
    .unwrap();

    assert_eq!(task.work_unit_id, Some(work.work_unit_id));
    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].title, "write task support");
    assert_eq!(tasks[0].closed_by_commit.as_deref(), Some("abc123"));
}

#[test]
fn close_work_refuses_open_tasks() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    start_work(temp.path(), "work with task", None).unwrap();
    add_task(
        temp.path(),
        NewTask {
            title: "must finish",
            priority: "medium",
            source: "user",
            work_unit_id: None,
            details: None,
            completion_condition: None,
        },
    )
    .unwrap();

    let closed = close_active_work(temp.path(), "done", None);

    assert!(closed.is_err());
}

#[test]
fn accepted_out_of_scope_task_does_not_block_close() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "work with exception", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "not in scope",
            priority: "medium",
            source: "design",
            work_unit_id: None,
            details: None,
            completion_condition: None,
        },
    )
    .unwrap();

    let acceptance =
        accept_task_out_of_scope(temp.path(), task.task_id, "scoped exception").unwrap();
    record_close_evidence(temp.path(), work.work_unit_id, work.activation_id);
    let closed = close_active_work(temp.path(), "done with exception", None).unwrap();

    assert_eq!(closed.work_unit_id, task.work_unit_id.unwrap());
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let acceptance_status: String = conn
        .query_row(
            "select status from acceptance_records where id = ?1 and task_id = ?2",
            params![acceptance.acceptance_record_id, task.task_id],
            |row| row.get(0),
        )
        .unwrap();
    let authority_count: i64 = conn
        .query_row(
            "select count(*) from authority_events where id = ?1 and event_type = 'user_instruction'",
            params![acceptance.authority_event_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(acceptance_status, "approved");
    assert_eq!(authority_count, 1);
}

#[test]
fn project_scoped_task_can_be_accepted_out_of_scope() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "project scoped task",
            priority: "medium",
            source: "user",
            work_unit_id: None,
            details: None,
            completion_condition: None,
        },
    )
    .unwrap();

    let acceptance =
        accept_task_out_of_scope(temp.path(), task.task_id, "project scope exception").unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let scope: String = conn
        .query_row(
            "select scope from acceptance_records where id = ?1",
            params![acceptance.acceptance_record_id],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(task.work_unit_id, None);
    assert_eq!(scope, "project");
}

#[test]
fn reopen_and_follow_up_create_active_work() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let original = start_work(temp.path(), "closed work", None).unwrap();
    let original_snapshot =
        record_close_evidence(temp.path(), original.work_unit_id, original.activation_id);
    close_active_work(temp.path(), "closed", None).unwrap();

    let authority_event_id = approval_authority_event(temp.path());
    let reopened = reopen_work(
        temp.path(),
        WorkReopen {
            work_unit_id: original.work_unit_id,
            reason: "closure invalid",
            reason_type: "closure_invalid",
            authority_event_id: Some(authority_event_id),
            acceptance_record_id: None,
        },
    )
    .unwrap();
    assert_eq!(reopened.work_unit_id, original.work_unit_id);
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let invalidation_count: i64 = conn
        .query_row(
            "select count(*) from work_unit_dependencies where work_unit_id = ?1 and depends_on_work_unit_id = ?1 and dependency_type = 'invalidates_closure'",
            params![original.work_unit_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(invalidation_count, 1);
    let reopened_snapshot =
        record_close_evidence(temp.path(), reopened.work_unit_id, reopened.activation_id);
    add_repository_snapshot_comparison(
        temp.path(),
        NewRepositorySnapshotComparison {
            base_repository_snapshot_id: original_snapshot.repository_snapshot_id,
            current_repository_snapshot_id: reopened_snapshot.repository_snapshot_id,
            comparison_type: "close",
            head_changed: false,
            dirty_state_changed: false,
            nested_repository_changed: false,
            result: "same",
        },
    )
    .unwrap();
    close_active_work(temp.path(), "closed again", None).unwrap();

    let follow_up = create_follow_up_work(
        temp.path(),
        original.work_unit_id,
        "related follow-up",
        "new related issue",
    )
    .unwrap();

    assert_ne!(follow_up.work_unit_id, original.work_unit_id);
    assert_eq!(follow_up.source_work_unit_id, original.work_unit_id);
    assert_eq!(
        next_action(temp.path()).unwrap(),
        NextAction::ContinueActive {
            work_unit: ActiveWorkUnit {
                id: follow_up.work_unit_id,
                title: "related follow-up".to_string(),
                design_version_id: None,
            }
        }
    );
}

#[test]
fn follow_up_suspends_active_work_and_records_source_event() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let source = start_work(temp.path(), "source", None).unwrap();
    record_close_evidence(temp.path(), source.work_unit_id, source.activation_id);
    close_active_work(temp.path(), "source done", None).unwrap();
    let parent = start_work(temp.path(), "mainline", None).unwrap();

    let follow_up =
        create_follow_up_work(temp.path(), source.work_unit_id, "follow-up", "new issue").unwrap();

    assert_eq!(
        next_action(temp.path()).unwrap(),
        NextAction::ContinueActive {
            work_unit: ActiveWorkUnit {
                id: follow_up.work_unit_id,
                title: "follow-up".to_string(),
                design_version_id: None,
            }
        }
    );
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let parent_status: String = conn
        .query_row(
            "select status from work_unit_activations where id = ?1",
            params![parent.activation_id],
            |row| row.get(0),
        )
        .unwrap();
    let child_frame: (Option<i64>, i64) = conn
        .query_row(
            "select parent_activation_id, stack_depth from work_unit_activations where id = ?1",
            params![follow_up.activation_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let source_follow_up_events: i64 = conn
        .query_row(
            "select count(*) from work_unit_events where work_unit_id = ?1 and event_type = 'follow_up_created'",
            params![source.work_unit_id],
            |row| row.get(0),
        )
        .unwrap();
    let follow_up_opened_events: i64 = conn
        .query_row(
            "select count(*) from work_unit_events where work_unit_id = ?1 and event_type = 'opened'",
            params![follow_up.work_unit_id],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(parent_status, "suspended");
    assert_eq!(child_frame, (Some(parent.activation_id), 1));
    assert_eq!(source_follow_up_events, 1);
    assert_eq!(follow_up_opened_events, 1);
}

#[test]
fn decisions_are_searchable() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();

    let decision = add_decision(
        temp.path(),
        NewDecision {
            decision_key: Some("db.scope"),
            topic: "database",
            decision: "use one sqlite ledger per project",
            rationale: Some("project-local state is enough for MVP"),
            compatibility_impact: None,
            authority_refs: None,
        },
    )
    .unwrap();
    let decisions = list_decisions(temp.path(), Some("sqlite")).unwrap();

    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0].id, decision.decision_id);
    assert_eq!(decisions[0].decision_key.as_deref(), Some("db.scope"));
}
