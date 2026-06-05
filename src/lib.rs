mod authority;
mod commands;
mod db;
mod design;
mod kpt;
mod planning;
mod records;
mod rules;
mod work;

pub use authority::{
    AuthorityEventOutcome, AuthorityEventRecord, NewAuthorityEvent, add_authority_event,
    list_authority_events,
};
pub use commands::{
    CommandDeviationOutcome, CommandOutcome, CommandProfileRecord, CommandUsageListQuery,
    CommandUsageOutcome, CommandUsageRecord, NewCommandDeviation, NewCommandProfile,
    NewCommandUsage, add_command_deviation, add_command_usage, add_fixed_command,
    list_command_profiles, list_command_usages,
};
pub use db::{
    ActiveWorkUnit, InitOutcome, NextAction, ProjectStatus, default_design_root,
    default_export_root, default_ledger_path, default_log_root, init_project, next_action,
    project_status,
};
pub use design::{
    DesignPackageImport, DesignPackageImportOutcome, DesignPackageInitOutcome, DesignReadyCheck,
    DesignReadyItem, DesignReadyOutcome, DesignRequirementListQuery, DesignRequirementRecord,
    DesignVersionApproval, DesignVersionApprovalOutcome, NewDesignPackage, approve_design_version,
    design_ready, import_design_package, init_design_package, list_design_requirements,
};
pub use kpt::{
    KptItemConversionOutcome, KptItemOutcome, KptItemRecord, KptItemTaskConversion,
    KptReviewCloseOutcome, KptReviewOutcome, KptReviewRecord, NewKptItem, NewKptReview,
    add_kpt_item, close_kpt_review, convert_kpt_item_to_task, list_kpt_items, list_kpt_reviews,
    start_kpt_review,
};
pub use planning::{
    DecisionOutcome, DecisionRecord, NewDecision, NewTask, TaskAcceptanceOutcome, TaskCloseOutcome,
    TaskListQuery, TaskOutcome, TaskRecord, accept_task_out_of_scope, add_decision, add_task,
    close_task, list_decisions, list_tasks,
};
pub use records::{
    NewWorkRecord, NewWorkRecordCommand, NewWorkRecordCommit, NewWorkRecordFile, WorkRecordEntry,
    WorkRecordLinkOutcome, WorkRecordOutcome, add_work_record_command, add_work_record_commit,
    add_work_record_file, create_work_record, export_work_record_markdown, list_work_records,
};
pub use rules::{
    NewUserCorrection, RuleQuery, RuleRecord, UserCorrectionOutcome, UserCorrectionRecord,
    add_user_correction, applicable_rules, list_user_corrections,
};
pub use work::{
    CloseOutcome, FollowUpOutcome, InterruptOutcome, NewWorkFork, ResumeCheckOutcome,
    ResumeOutcome, ResumeReadyItem, ResumeReadyOutcome, SuspendOutcome, WorkForkOutcome,
    WorkForkSource, WorkOutcome, close_active_work, create_follow_up_work, fork_work,
    interrupt_work, reopen_work, resume_check, resume_check_basic, resume_ready,
    resume_ready_basic, resume_work, start_work, suspend_work,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{SCHEMA_VERSION, open_ledger};
    use rusqlite::params;
    use std::fs;

    #[test]
    fn init_creates_ledger_and_project() {
        let temp = tempfile::tempdir().unwrap();

        let outcome = init_project(temp.path()).unwrap();

        assert!(outcome.ledger_path.exists());
        assert!(default_design_root(temp.path()).exists());
        assert!(default_export_root(temp.path()).exists());
        assert!(default_log_root(temp.path()).exists());
        let status = project_status(temp.path()).unwrap();
        assert!(status.initialized);
        assert_eq!(status.schema_version, Some(SCHEMA_VERSION));
        assert_eq!(status.open_work_units, 0);
        assert_eq!(status.active_activations, 0);
    }

    #[test]
    fn status_reports_uninitialized_project() {
        let temp = tempfile::tempdir().unwrap();

        let status = project_status(temp.path()).unwrap();

        assert!(!status.initialized);
        assert!(status.schema_version.is_none());
    }

    #[test]
    fn next_reports_no_active_work_unit_after_init() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();

        let next = next_action(temp.path()).unwrap();

        assert_eq!(next, NextAction::NoActiveWorkUnit);
    }

    #[test]
    fn work_start_creates_active_work_unit() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();

        let started = start_work(temp.path(), "write lifecycle test", Some("test first")).unwrap();
        let next = next_action(temp.path()).unwrap();

        assert_eq!(started.work_unit_id, 1);
        assert_eq!(started.activation_id, 1);
        assert_eq!(
            next,
            NextAction::ContinueActive {
                work_unit: ActiveWorkUnit {
                    id: 1,
                    title: "write lifecycle test".to_string()
                }
            }
        );
    }

    #[test]
    fn work_start_refuses_second_active_activation() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        start_work(temp.path(), "one", None).unwrap();

        let second = start_work(temp.path(), "two", None);

        assert!(second.is_err());
    }

    #[test]
    fn suspend_and_resume_round_trip() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        let started = start_work(temp.path(), "implement resume", None).unwrap();

        let suspended = suspend_work(
            temp.path(),
            "need to validate assumption",
            "continue implementation",
        )
        .unwrap();
        let check = resume_check_basic(temp.path()).unwrap();
        let resumed = resume_work(temp.path(), check.resume_check_id).unwrap();

        assert_eq!(suspended.work_unit_id, started.work_unit_id);
        assert_eq!(check.result, "allowed");
        assert_eq!(resumed.activation_id, started.activation_id);
        assert!(matches!(
            next_action(temp.path()).unwrap(),
            NextAction::ContinueActive { .. }
        ));
    }

    #[test]
    fn resume_ready_dry_run_does_not_record_check() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        start_work(temp.path(), "implement resume gate", None).unwrap();
        suspend_work(temp.path(), "interrupt complete", "resume gate work").unwrap();

        let outcome = resume_ready_basic(temp.path()).unwrap();

        assert_eq!(outcome.result, "pass");
        assert!(
            outcome
                .items
                .iter()
                .filter(|item| item.result == "pass")
                .count()
                >= 6
        );
        assert!(
            outcome
                .items
                .iter()
                .any(|item| item.result == "not_checked")
        );
        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        let count: i64 = conn
            .query_row("select count(*) from resume_checks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn resume_ready_without_target_returns_blocked_gate_result() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();

        let outcome = resume_ready_basic(temp.path()).unwrap();

        assert_eq!(outcome.result, "blocked");
        assert_eq!(
            outcome.blocking_reason.as_deref(),
            Some("no suspended activation to resume")
        );
        assert_eq!(outcome.work_unit_id, None);
        assert_eq!(outcome.activation_id, None);
    }

    #[test]
    fn trace_aware_resume_check_records_not_checked_items() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        start_work(temp.path(), "implement trace gate", None).unwrap();
        suspend_work(temp.path(), "need trace-aware check", "resume trace work").unwrap();

        let check = resume_check(temp.path(), "trace-aware").unwrap();

        assert_eq!(check.result, "blocked");
        assert_eq!(
            check.blocking_reason.as_deref(),
            Some("trace-aware checks are not implemented yet")
        );
        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        let stored_maturity: String = conn
            .query_row(
                "select maturity from resume_checks where id = ?1",
                params![check.resume_check_id],
                |row| row.get(0),
            )
            .unwrap();
        let not_checked: i64 = conn
            .query_row(
                "select count(*) from resume_check_items where resume_check_id = ?1 and result = 'not_checked'",
                params![check.resume_check_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored_maturity, "trace-aware");
        assert!(not_checked > 0);
    }

    #[test]
    fn interrupt_blocks_parent_until_child_is_closed() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        let parent = start_work(temp.path(), "parent", None).unwrap();

        let interrupt = interrupt_work(temp.path(), "child", "blocks parent").unwrap();
        let blocked = resume_check_basic(temp.path()).unwrap();
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
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].rule_source_type, "user_correction");
        assert_eq!(
            rules[0].user_correction_id,
            Some(correction.user_correction_id)
        );
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
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].rule_source_type, "command_profile");
        assert_eq!(
            rules[0].command_profile_id,
            Some(command.command_profile_id)
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

        let markdown =
            export_work_record_markdown(temp.path(), work_record.work_record_id).unwrap();
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
        start_work(temp.path(), "work with exception", None).unwrap();
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
    fn reopen_and_follow_up_create_active_work() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        let original = start_work(temp.path(), "closed work", None).unwrap();
        close_active_work(temp.path(), "closed", None).unwrap();

        let reopened = reopen_work(temp.path(), original.work_unit_id, "closure invalid").unwrap();
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
                }
            }
        );
    }

    #[test]
    fn follow_up_suspends_active_work_and_records_source_event() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        let source = start_work(temp.path(), "source", None).unwrap();
        close_active_work(temp.path(), "source done", None).unwrap();
        let parent = start_work(temp.path(), "mainline", None).unwrap();

        let follow_up =
            create_follow_up_work(temp.path(), source.work_unit_id, "follow-up", "new issue")
                .unwrap();

        assert_eq!(
            next_action(temp.path()).unwrap(),
            NextAction::ContinueActive {
                work_unit: ActiveWorkUnit {
                    id: follow_up.work_unit_id,
                    title: "follow-up".to_string(),
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

    #[test]
    fn design_init_creates_standard_package_under_workbench() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();

        let outcome = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "storage-lifecycle",
                title: "Storage Lifecycle",
            },
        )
        .unwrap();

        let package = temp
            .path()
            .join(".agent-workbench")
            .join("designs")
            .join("storage-lifecycle");
        assert_eq!(outcome.package_path, package);
        assert!(package.join("design.yaml").exists());
        assert!(package.join("01-introduction-goals.md").exists());
        assert!(package.join("12-glossary.md").exists());
        assert!(package.join("requirements").join("README.md").exists());
        assert!(package.join("validation").join("gates.md").exists());

        let manifest = fs::read_to_string(package.join("design.yaml")).unwrap();
        assert!(manifest.contains(r#"id: "storage-lifecycle""#));
        assert!(manifest.contains(r#"title: "Storage Lifecycle""#));
        assert!(manifest.contains("format: arc42-agent-workbench"));
        assert!(manifest.contains("introduction_goals: 01-introduction-goals.md"));
    }

    #[test]
    fn design_init_rejects_invalid_or_existing_package_id() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();

        assert!(
            init_design_package(
                temp.path(),
                NewDesignPackage {
                    design_id: "Storage",
                    title: "Storage",
                },
            )
            .is_err()
        );
        assert!(
            init_design_package(
                temp.path(),
                NewDesignPackage {
                    design_id: "storage/lifecycle",
                    title: "Storage",
                },
            )
            .is_err()
        );

        init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "storage-lifecycle",
                title: "Storage",
            },
        )
        .unwrap();
        assert!(
            init_design_package(
                temp.path(),
                NewDesignPackage {
                    design_id: "storage-lifecycle",
                    title: "Storage",
                },
            )
            .is_err()
        );
    }

    #[test]
    fn design_import_records_package_version_and_files() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        let init = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "storage-lifecycle",
                title: "Storage Lifecycle",
            },
        )
        .unwrap();

        let import = import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &init.package_path,
                status: "draft",
            },
        )
        .unwrap();

        assert_eq!(import.design_package_id, 1);
        assert_eq!(import.design_version_id, 1);
        assert_eq!(import.version_number, 1);
        assert_eq!(import.file_count, 14);
        assert_eq!(import.content_hash.len(), 64);

        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        let package: (String, String, String, i64) = conn
            .query_row(
                r#"
                select design_key, title, status, current_design_version_id
                from design_packages
                where id = ?1
                "#,
                params![import.design_package_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        let file_count: i64 = conn
            .query_row(
                "select count(*) from design_files where design_version_id = ?1",
                params![import.design_version_id],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(package.0, "storage-lifecycle");
        assert_eq!(package.1, "Storage Lifecycle");
        assert_eq!(package.2, "draft");
        assert_eq!(package.3, import.design_version_id);
        assert_eq!(file_count, 14);
    }

    #[test]
    fn design_import_extracts_machine_readable_requirements() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        let init = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "storage-lifecycle",
                title: "Storage Lifecycle",
            },
        )
        .unwrap();
        fs::write(
            init.package_path.join("requirements").join("README.md"),
            requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
        )
        .unwrap();

        let import = import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &init.package_path,
                status: "draft",
            },
        )
        .unwrap();
        let requirements = list_design_requirements(
            temp.path(),
            DesignRequirementListQuery {
                design_version_id: import.design_version_id,
            },
        )
        .unwrap();

        assert_eq!(import.requirement_count, 1);
        assert_eq!(requirements.len(), 1);
        assert_eq!(requirements[0].requirement_key, "REQ-001");
        assert_eq!(requirements[0].priority, "high");
        assert_eq!(
            requirements[0].validation_expectation.as_deref(),
            Some("GATE-001")
        );
        assert!(
            requirements[0]
                .requirement_text
                .contains("verifiable behavior")
        );
    }

    #[test]
    fn design_import_rejects_external_or_duplicate_package() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        let init = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "storage-lifecycle",
                title: "Storage Lifecycle",
            },
        )
        .unwrap();
        let external = temp.path().join("external-design");
        fs::create_dir_all(&external).unwrap();

        assert!(
            import_design_package(
                temp.path(),
                DesignPackageImport {
                    package_path: &external,
                    status: "draft",
                },
            )
            .is_err()
        );

        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &init.package_path,
                status: "draft",
            },
        )
        .unwrap();
        assert!(
            import_design_package(
                temp.path(),
                DesignPackageImport {
                    package_path: &init.package_path,
                    status: "draft",
                },
            )
            .is_err()
        );
    }

    #[test]
    fn design_approval_marks_current_version_and_creates_authority() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        let init = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "storage-lifecycle",
                title: "Storage Lifecycle",
            },
        )
        .unwrap();
        fs::write(
            init.package_path.join("requirements").join("README.md"),
            requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
        )
        .unwrap();
        let import = import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &init.package_path,
                status: "draft",
            },
        )
        .unwrap();

        let approval = approve_design_version(
            temp.path(),
            DesignVersionApproval {
                design_version_id: import.design_version_id,
                summary: Some("design passed document checks"),
            },
        )
        .unwrap();

        assert_eq!(approval.design_version_id, import.design_version_id);
        assert_eq!(approval.design_package_id, import.design_package_id);
        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        let approved: (String, i64) = conn
            .query_row(
                "select status, approved_by_authority_event_id from design_versions where id = ?1",
                params![import.design_version_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        let package_status: String = conn
            .query_row(
                "select status from design_packages where id = ?1",
                params![import.design_package_id],
                |row| row.get(0),
            )
            .unwrap();
        let authority: (String, String) = conn
            .query_row(
                "select event_type, text_or_summary from authority_events where id = ?1",
                params![approval.authority_event_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(
            approved,
            ("approved".to_string(), approval.authority_event_id)
        );
        assert_eq!(package_status, "approved");
        assert_eq!(authority.0, "design_doc");
        assert_eq!(authority.1, "design passed document checks");
    }

    #[test]
    fn design_ready_blocks_until_current_version_is_approved() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        let init = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "storage-lifecycle",
                title: "Storage Lifecycle",
            },
        )
        .unwrap();
        fs::write(
            init.package_path.join("requirements").join("README.md"),
            requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
        )
        .unwrap();
        let import = import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &init.package_path,
                status: "draft",
            },
        )
        .unwrap();

        let blocked = design_ready(
            temp.path(),
            DesignReadyCheck {
                design_version_id: Some(import.design_version_id),
            },
        )
        .unwrap();
        approve_design_version(
            temp.path(),
            DesignVersionApproval {
                design_version_id: import.design_version_id,
                summary: None,
            },
        )
        .unwrap();
        let passed = design_ready(
            temp.path(),
            DesignReadyCheck {
                design_version_id: Some(import.design_version_id),
            },
        )
        .unwrap();

        assert_eq!(blocked.result, "blocked");
        assert!(
            blocked
                .items
                .iter()
                .any(|item| item.name == "design_version_approved" && item.result == "fail")
        );
        assert_eq!(passed.result, "pass");
        assert!(passed.items.iter().all(|item| item.result == "pass"));
    }

    fn requirement_doc(key: &str, title: &str, priority: &str) -> String {
        format!(
            r#"## {key}: {title}
```yaml agent-workbench
type: requirement
key: {key}
priority: {priority}
surfaces: [cli, database]
validation: [GATE-001]
status: active
```

This requirement describes one verifiable behavior that must be implemented.
"#
        )
    }

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
        let rules = applicable_rules(
            temp.path(),
            RuleQuery {
                scope_key: Some("project"),
                work_unit_id: None,
            },
        )
        .unwrap();

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, authority.authority_event_id);
        assert_eq!(rules.len(), 1);
        assert_eq!(
            rules[0].authority_event_id,
            Some(authority.authority_event_id)
        );
    }

    #[test]
    fn current_scope_rules_include_active_work_unit_rules() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        let work = start_work(temp.path(), "scoped work", None).unwrap();
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
    fn activation_unique_active_constraint_is_enforced() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        let project_id: i64 = conn
            .query_row("select id from projects limit 1", [], |row| row.get(0))
            .unwrap();
        conn.execute(
            "insert into work_units(project_id, title, status, started_at) values (?1, 'one', 'open', current_timestamp)",
            params![project_id],
        )
        .unwrap();
        conn.execute(
            "insert into work_units(project_id, title, status, started_at) values (?1, 'two', 'open', current_timestamp)",
            params![project_id],
        )
        .unwrap();

        conn.execute(
            "insert into work_unit_activations(project_id, work_unit_id, status, activation_reason, opened_at) values (?1, 1, 'active', 'start', current_timestamp)",
            params![project_id],
        )
        .unwrap();
        let duplicate = conn.execute(
            "insert into work_unit_activations(project_id, work_unit_id, status, activation_reason, opened_at) values (?1, 2, 'active', 'start', current_timestamp)",
            params![project_id],
        );

        assert!(duplicate.is_err());
    }
}
