mod authority;
mod commands;
mod coverage;
mod db;
mod design;
mod kpt;
mod planning;
mod records;
mod rules;
mod traceability;
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
pub use coverage::{
    CoverageItemListQuery, CoverageItemOutcome, CoverageItemRecord, NewCoverageItem,
    add_coverage_item, list_coverage_items,
};
pub use db::{
    ActiveWorkUnit, InitOutcome, NextAction, ProjectStatus, default_design_root,
    default_export_root, default_ledger_path, default_log_root, init_project, next_action,
    project_status,
};
pub use design::{
    DesignDecisionListQuery, DesignDecisionRecord, DesignPackageImport, DesignPackageImportOutcome,
    DesignPackageInitOutcome, DesignReadyCheck, DesignReadyItem, DesignReadyOutcome,
    DesignRequirementListQuery, DesignRequirementRecord, DesignVersionApproval,
    DesignVersionApprovalOutcome, NewDesignExceptionAcceptance, NewDesignPackage,
    ValidationGateTemplateListQuery, ValidationGateTemplateRecord, accept_design_exception,
    approve_design_version, design_ready, import_design_package, init_design_package,
    list_design_decisions, list_design_requirements, list_validation_gate_templates,
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
pub use traceability::{
    ImplementationEvidenceListQuery, ImplementationEvidenceOutcome, ImplementationEvidenceRecord,
    ImplementationReadyCheck, ImplementationReadyItem, ImplementationReadyOutcome,
    NewImplementationEvidence, NewTaskDerivation, TaskDerivationListQuery, TaskDerivationOutcome,
    TaskDerivationRecord, ValidationGateSelection, ValidationGateSelectionOutcome,
    add_implementation_evidence, derive_task_from_requirement, implementation_ready,
    list_implementation_evidence, list_task_derivations, select_validation_gate,
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
    fn init_migrates_existing_acceptance_records_shape() {
        let temp = tempfile::tempdir().unwrap();
        let ledger_dir = temp.path().join(".agent-workbench");
        fs::create_dir_all(&ledger_dir).unwrap();
        let ledger_path = ledger_dir.join("ledger.sqlite");
        let conn = rusqlite::Connection::open(&ledger_path).unwrap();
        conn.execute_batch(
            r#"
            create table schema_migrations (
                version integer primary key,
                applied_at text not null
            );
            insert into schema_migrations(version, applied_at)
            values (4, current_timestamp);

            create table acceptance_records (
                id integer primary key,
                project_id integer not null,
                target_type text not null check (target_type in ('task', 'design_requirement', 'validation_gate_template')),
                task_id integer,
                design_requirement_id integer,
                validation_gate_template_id integer,
                acceptance_type text not null check (acceptance_type in ('accepted_out_of_scope', 'explicit_exception')),
                reason text not null,
                scope text,
                created_by text not null,
                status text not null default 'approved' check (status in ('approved', 'revoked')),
                approved_by_authority_event_id integer,
                approved_at text,
                created_at text not null,
                review_impact text,
                check (
                    (target_type = 'task' and task_id is not null and design_requirement_id is null and validation_gate_template_id is null)
                    or (target_type = 'design_requirement' and task_id is null and design_requirement_id is not null and validation_gate_template_id is null)
                    or (target_type = 'validation_gate_template' and task_id is null and design_requirement_id is null and validation_gate_template_id is not null)
                )
            );
            "#,
        )
        .unwrap();
        drop(conn);

        init_project(temp.path()).unwrap();
        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        conn.execute(
            r#"
            insert into acceptance_records(
                project_id, target_type, design_package_key, design_file_path,
                acceptance_type, reason, created_by, status, created_at
            )
            values (
                1, 'design_file', 'oversized-file', '01-introduction-goals.md',
                'explicit_exception', 'oversized import guardrail', 'user',
                'approved', current_timestamp
            )
            "#,
            [],
        )
        .unwrap();
        let status = project_status(temp.path()).unwrap();
        let schema_sql: String = conn
            .query_row(
                "select sql from sqlite_schema where type = 'table' and name = 'acceptance_records'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        conn.execute(
            r#"
            insert into acceptance_records(
                project_id, target_type, design_package_key, design_requirement_key,
                acceptance_type, reason, created_by, status, created_at
            )
            values (
                1, 'design_requirement_key', 'oversized-file', 'REQ-001',
                'explicit_exception', 'proposed oversized requirement', 'agent',
                'proposed', current_timestamp
            )
            "#,
            [],
        )
        .unwrap();

        assert_eq!(status.schema_version, Some(SCHEMA_VERSION));
        assert!(schema_sql.contains("created_by in ('user', 'agent', 'system')"));
        assert!(schema_sql.contains("status in ('proposed', 'approved', 'rejected', 'expired')"));
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
    fn trace_aware_resume_check_evaluates_trace_items() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        start_work(temp.path(), "implement trace gate", None).unwrap();
        suspend_work(temp.path(), "need trace-aware check", "resume trace work").unwrap();

        let check = resume_check(temp.path(), "trace-aware").unwrap();

        assert_eq!(check.result, "allowed");
        assert_eq!(check.blocking_reason, None);
        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        let stored_maturity: String = conn
            .query_row(
                "select maturity from resume_checks where id = ?1",
                params![check.resume_check_id],
                |row| row.get(0),
            )
            .unwrap();
        let trace_passes: i64 = conn
            .query_row(
                r#"
                select count(*)
                from resume_check_items
                where resume_check_id = ?1
                  and check_name in (
                    'design_version_current',
                    'task_derivation_current',
                    'checklist_current',
                    'selected_gate_current'
                  )
                  and result = 'pass'
                "#,
                params![check.resume_check_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored_maturity, "trace-aware");
        assert_eq!(trace_passes, 4);
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
        let short_file_hashes: i64 = conn
            .query_row(
                "select count(*) from design_files where design_version_id = ?1 and length(content_hash) != 64",
                params![import.design_version_id],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(package.0, "storage-lifecycle");
        assert_eq!(package.1, "Storage Lifecycle");
        assert_eq!(package.2, "draft");
        assert_eq!(package.3, import.design_version_id);
        assert_eq!(file_count, 14);
        assert_eq!(short_file_hashes, 0);
    }

    #[test]
    fn design_import_hashes_are_deterministic_and_change_with_content() {
        let temp_a = tempfile::tempdir().unwrap();
        init_project(temp_a.path()).unwrap();
        let init_a = init_design_package(
            temp_a.path(),
            NewDesignPackage {
                design_id: "storage-lifecycle",
                title: "Storage Lifecycle",
            },
        )
        .unwrap();
        fs::write(
            init_a.package_path.join("requirements").join("README.md"),
            requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
        )
        .unwrap();
        let import_a = import_design_package(
            temp_a.path(),
            DesignPackageImport {
                package_path: &init_a.package_path,
                status: "draft",
            },
        )
        .unwrap();
        let conn_a = open_ledger(&default_ledger_path(temp_a.path())).unwrap();
        let requirement_hash_a: String = conn_a
            .query_row(
                "select requirement_hash from design_requirements where design_version_id = ?1",
                params![import_a.design_version_id],
                |row| row.get(0),
            )
            .unwrap();

        let temp_b = tempfile::tempdir().unwrap();
        init_project(temp_b.path()).unwrap();
        let init_b = init_design_package(
            temp_b.path(),
            NewDesignPackage {
                design_id: "storage-lifecycle",
                title: "Storage Lifecycle",
            },
        )
        .unwrap();
        fs::write(
            init_b.package_path.join("requirements").join("README.md"),
            requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
        )
        .unwrap();
        let import_b = import_design_package(
            temp_b.path(),
            DesignPackageImport {
                package_path: &init_b.package_path,
                status: "draft",
            },
        )
        .unwrap();
        let conn_b = open_ledger(&default_ledger_path(temp_b.path())).unwrap();
        let requirement_hash_b: String = conn_b
            .query_row(
                "select requirement_hash from design_requirements where design_version_id = ?1",
                params![import_b.design_version_id],
                |row| row.get(0),
            )
            .unwrap();

        let temp_c = tempfile::tempdir().unwrap();
        init_project(temp_c.path()).unwrap();
        let init_c = init_design_package(
            temp_c.path(),
            NewDesignPackage {
                design_id: "storage-lifecycle",
                title: "Storage Lifecycle",
            },
        )
        .unwrap();
        fs::write(
            init_c.package_path.join("requirements").join("README.md"),
            requirement_doc("REQ-001", "Preserve cleanup behavior", "high")
                .replace("one verifiable behavior", "a different verifiable behavior"),
        )
        .unwrap();
        let import_c = import_design_package(
            temp_c.path(),
            DesignPackageImport {
                package_path: &init_c.package_path,
                status: "draft",
            },
        )
        .unwrap();
        let conn_c = open_ledger(&default_ledger_path(temp_c.path())).unwrap();
        let requirement_hash_c: String = conn_c
            .query_row(
                "select requirement_hash from design_requirements where design_version_id = ?1",
                params![import_c.design_version_id],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(import_a.content_hash, import_b.content_hash);
        assert_eq!(requirement_hash_a, requirement_hash_b);
        assert_ne!(import_a.content_hash, import_c.content_hash);
        assert_ne!(requirement_hash_a, requirement_hash_c);
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
    fn design_import_extracts_decisions_and_validation_gate_templates() {
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
        fs::write(init.package_path.join("09-decisions.md"), decision_doc()).unwrap();
        fs::write(
            init.package_path.join("validation").join("gates.md"),
            validation_gate_doc("GATE-001"),
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
        let decisions = list_design_decisions(
            temp.path(),
            DesignDecisionListQuery {
                design_version_id: import.design_version_id,
            },
        )
        .unwrap();
        let gates = list_validation_gate_templates(
            temp.path(),
            ValidationGateTemplateListQuery {
                design_version_id: import.design_version_id,
            },
        )
        .unwrap();

        assert_eq!(import.decision_count, 1);
        assert_eq!(import.validation_gate_template_count, 1);
        assert_eq!(decisions[0].decision_key, "DEC-001");
        assert_eq!(decisions[0].topic, "Keep project-local ledger");
        assert_eq!(gates[0].gate_key, "GATE-001");
        assert_eq!(gates[0].stage, "implementation-ready");
        assert_eq!(gates[0].expected_result, "pass");
        assert_eq!(gates[0].requirement_keys.as_deref(), Some("REQ-001"));
        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        let linked_count: i64 = conn
            .query_row(
                "select count(*) from validation_gate_template_requirements",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(linked_count, 1);
    }

    #[test]
    fn task_derivation_creates_checklist_trace_and_unblocks_implementation_ready() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        start_work(temp.path(), "implement storage lifecycle", None).unwrap();
        let task = add_task(
            temp.path(),
            NewTask {
                title: "implement cleanup",
                priority: "high",
                source: "design",
                work_unit_id: None,
                details: None,
                completion_condition: Some("cleanup behavior is covered"),
            },
        )
        .unwrap();
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
        fs::write(
            init.package_path.join("validation").join("gates.md"),
            validation_gate_doc("GATE-001"),
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
        approve_design_version(
            temp.path(),
            DesignVersionApproval {
                design_version_id: import.design_version_id,
                summary: None,
            },
        )
        .unwrap();
        let blocked = implementation_ready(
            temp.path(),
            ImplementationReadyCheck {
                design_version_id: Some(import.design_version_id),
            },
        )
        .unwrap();

        let derivation = derive_task_from_requirement(
            temp.path(),
            NewTaskDerivation {
                design_version_id: import.design_version_id,
                requirement_key: "REQ-001",
                task_id: task.task_id,
                derivation_reason: Some("design task decomposition"),
                checklist_title: None,
                item_title: None,
                completion_condition: None,
            },
        )
        .unwrap();
        let blocked_without_gate = implementation_ready(
            temp.path(),
            ImplementationReadyCheck {
                design_version_id: Some(import.design_version_id),
            },
        )
        .unwrap();
        let gate = select_validation_gate(
            temp.path(),
            ValidationGateSelection {
                design_version_id: import.design_version_id,
                gate_key: "GATE-001",
                requirement_key: "REQ-001",
                task_id: task.task_id,
                command: None,
            },
        )
        .unwrap();
        let passed = implementation_ready(
            temp.path(),
            ImplementationReadyCheck {
                design_version_id: Some(import.design_version_id),
            },
        )
        .unwrap();
        let close_without_trace = close_task(temp.path(), task.task_id, Some("abc123"));
        let evidence = add_implementation_evidence(
            temp.path(),
            NewImplementationEvidence {
                task_id: Some(task.task_id),
                design_version_id: Some(import.design_version_id),
                requirement_key: Some("REQ-001"),
                evidence_type: "commit",
                commit_sha: Some("abc123"),
                file_path: None,
                line_ref: None,
                symbol: None,
                artifact_path: None,
                note: None,
            },
        )
        .unwrap();
        let evidence_records = list_implementation_evidence(
            temp.path(),
            ImplementationEvidenceListQuery {
                task_id: Some(task.task_id),
                design_version_id: None,
            },
        )
        .unwrap();
        let coverage = add_coverage_item(
            temp.path(),
            NewCoverageItem {
                design_version_id: import.design_version_id,
                requirement_key: "REQ-001",
                review_scope_id: None,
                work_unit_id: None,
                task_id: Some(task.task_id),
                requirement: "cleanup behavior is connected to implementation and tests",
                runtime_boundary_evidence: Some("cleanup path preserves lifecycle behavior"),
                ux_boundary_evidence: None,
                lifecycle_boundary_evidence: Some("storage lifecycle remains intact"),
                tests_or_gates: Some("GATE-001"),
                missing_or_unverified: None,
                status: "covered",
            },
        )
        .unwrap();
        let coverage_records = list_coverage_items(
            temp.path(),
            CoverageItemListQuery {
                design_version_id: import.design_version_id,
                status: Some("covered"),
            },
        )
        .unwrap();
        close_task(temp.path(), task.task_id, Some("abc123")).unwrap();
        let passed_after_close = implementation_ready(
            temp.path(),
            ImplementationReadyCheck {
                design_version_id: Some(import.design_version_id),
            },
        )
        .unwrap();
        let records = list_task_derivations(
            temp.path(),
            TaskDerivationListQuery {
                design_version_id: import.design_version_id,
            },
        )
        .unwrap();

        assert_eq!(blocked.result, "blocked");
        assert!(
            blocked
                .items
                .iter()
                .any(|item| { item.name == "task_derivations_exist" && item.result == "fail" })
        );
        assert_eq!(derivation.task_id, task.task_id);
        assert_eq!(gate.task_id, task.task_id);
        assert_eq!(blocked_without_gate.result, "blocked");
        assert!(
            blocked_without_gate
                .items
                .iter()
                .any(|item| { item.name == "validation_gates_selected" && item.result == "fail" })
        );
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].requirement_key, "REQ-001");
        assert_eq!(passed.result, "pass");
        assert!(close_without_trace.is_err());
        assert_eq!(evidence.task_id, Some(task.task_id));
        assert_eq!(evidence_records.len(), 1);
        assert_eq!(
            evidence_records[0].requirement_key.as_deref(),
            Some("REQ-001")
        );
        assert_eq!(evidence_records[0].commit_sha.as_deref(), Some("abc123"));
        assert_eq!(passed_after_close.result, "pass");
        assert_eq!(coverage.task_id, Some(task.task_id));
        assert_eq!(coverage_records.len(), 1);
        assert_eq!(coverage_records[0].requirement_key, "REQ-001");
        assert_eq!(coverage_records[0].status, "covered");
    }

    #[test]
    fn trace_links_reject_mismatched_requirement_task_pairs() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        start_work(temp.path(), "implement storage lifecycle", None).unwrap();
        let task_one = add_task(
            temp.path(),
            NewTask {
                title: "implement cleanup",
                priority: "high",
                source: "design",
                work_unit_id: None,
                details: None,
                completion_condition: Some("cleanup behavior is covered"),
            },
        )
        .unwrap();
        let task_two = add_task(
            temp.path(),
            NewTask {
                title: "implement archival",
                priority: "high",
                source: "design",
                work_unit_id: None,
                details: None,
                completion_condition: Some("archival behavior is covered"),
            },
        )
        .unwrap();
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
            format!(
                "{}\n{}",
                requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
                requirement_doc("REQ-002", "Preserve archival behavior", "high")
            ),
        )
        .unwrap();
        fs::write(
            init.package_path.join("validation").join("gates.md"),
            validation_gate_doc("GATE-001"),
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
        derive_task_from_requirement(
            temp.path(),
            NewTaskDerivation {
                design_version_id: import.design_version_id,
                requirement_key: "REQ-001",
                task_id: task_one.task_id,
                derivation_reason: None,
                checklist_title: None,
                item_title: None,
                completion_condition: None,
            },
        )
        .unwrap();
        derive_task_from_requirement(
            temp.path(),
            NewTaskDerivation {
                design_version_id: import.design_version_id,
                requirement_key: "REQ-002",
                task_id: task_two.task_id,
                derivation_reason: None,
                checklist_title: None,
                item_title: None,
                completion_condition: None,
            },
        )
        .unwrap();

        let mismatched_evidence = add_implementation_evidence(
            temp.path(),
            NewImplementationEvidence {
                task_id: Some(task_two.task_id),
                design_version_id: Some(import.design_version_id),
                requirement_key: Some("REQ-001"),
                evidence_type: "commit",
                commit_sha: Some("abc123"),
                file_path: None,
                line_ref: None,
                symbol: None,
                artifact_path: None,
                note: None,
            },
        );
        let mismatched_gate = select_validation_gate(
            temp.path(),
            ValidationGateSelection {
                design_version_id: import.design_version_id,
                gate_key: "GATE-001",
                requirement_key: "REQ-001",
                task_id: task_two.task_id,
                command: None,
            },
        );
        let mismatched_coverage = add_coverage_item(
            temp.path(),
            NewCoverageItem {
                design_version_id: import.design_version_id,
                requirement_key: "REQ-001",
                review_scope_id: None,
                work_unit_id: None,
                task_id: Some(task_two.task_id),
                requirement: "cleanup behavior is connected",
                runtime_boundary_evidence: None,
                ux_boundary_evidence: None,
                lifecycle_boundary_evidence: None,
                tests_or_gates: Some("GATE-001"),
                missing_or_unverified: None,
                status: "covered",
            },
        );

        assert!(mismatched_evidence.is_err());
        assert!(mismatched_gate.is_err());
        assert!(mismatched_coverage.is_err());
    }

    #[test]
    fn implementation_ready_requires_completion_conditions() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        start_work(temp.path(), "implement storage lifecycle", None).unwrap();
        let task = add_task(
            temp.path(),
            NewTask {
                title: "implement cleanup",
                priority: "high",
                source: "design",
                work_unit_id: None,
                details: None,
                completion_condition: None,
            },
        )
        .unwrap();
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
        fs::write(
            init.package_path.join("validation").join("gates.md"),
            validation_gate_doc("GATE-001"),
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
        approve_design_version(
            temp.path(),
            DesignVersionApproval {
                design_version_id: import.design_version_id,
                summary: None,
            },
        )
        .unwrap();
        derive_task_from_requirement(
            temp.path(),
            NewTaskDerivation {
                design_version_id: import.design_version_id,
                requirement_key: "REQ-001",
                task_id: task.task_id,
                derivation_reason: None,
                checklist_title: None,
                item_title: None,
                completion_condition: None,
            },
        )
        .unwrap();
        select_validation_gate(
            temp.path(),
            ValidationGateSelection {
                design_version_id: import.design_version_id,
                gate_key: "GATE-001",
                requirement_key: "REQ-001",
                task_id: task.task_id,
                command: None,
            },
        )
        .unwrap();

        let outcome = implementation_ready(
            temp.path(),
            ImplementationReadyCheck {
                design_version_id: Some(import.design_version_id),
            },
        )
        .unwrap();

        assert_eq!(outcome.result, "blocked");
        assert!(
            outcome.items.iter().any(|item| {
                item.name == "completion_conditions_present" && item.result == "fail"
            })
        );
    }

    #[test]
    fn implementation_ready_blocks_stale_derivations_and_checklists() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        start_work(temp.path(), "implement storage lifecycle", None).unwrap();
        let task = add_task(
            temp.path(),
            NewTask {
                title: "implement cleanup",
                priority: "high",
                source: "design",
                work_unit_id: None,
                details: None,
                completion_condition: Some("cleanup behavior is covered"),
            },
        )
        .unwrap();
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
        fs::write(
            init.package_path.join("validation").join("gates.md"),
            validation_gate_doc("GATE-001"),
        )
        .unwrap();
        let import_a = import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &init.package_path,
                status: "draft",
            },
        )
        .unwrap();
        approve_design_version(
            temp.path(),
            DesignVersionApproval {
                design_version_id: import_a.design_version_id,
                summary: None,
            },
        )
        .unwrap();
        derive_task_from_requirement(
            temp.path(),
            NewTaskDerivation {
                design_version_id: import_a.design_version_id,
                requirement_key: "REQ-001",
                task_id: task.task_id,
                derivation_reason: Some("design task decomposition"),
                checklist_title: None,
                item_title: None,
                completion_condition: None,
            },
        )
        .unwrap();
        fs::write(
            init.package_path.join("requirements").join("README.md"),
            r#"## REQ-001: Preserve cleanup behavior
```yaml agent-workbench
type: requirement
key: REQ-001
revision: 2
priority: high
surfaces: [cli, database]
validation: [GATE-001]
status: active
```

This requirement describes changed cleanup behavior that must be implemented.
"#,
        )
        .unwrap();
        let import_b = import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &init.package_path,
                status: "draft",
            },
        )
        .unwrap();
        approve_design_version(
            temp.path(),
            DesignVersionApproval {
                design_version_id: import_b.design_version_id,
                summary: None,
            },
        )
        .unwrap();

        let blocked = implementation_ready(
            temp.path(),
            ImplementationReadyCheck {
                design_version_id: Some(import_b.design_version_id),
            },
        )
        .unwrap();

        assert_eq!(blocked.result, "blocked");
        assert!(
            blocked
                .items
                .iter()
                .any(|item| { item.name == "task_derivations_current" && item.result == "fail" })
        );
        assert!(
            blocked
                .items
                .iter()
                .any(|item| { item.name == "checklists_current" && item.result == "fail" })
        );
    }

    #[test]
    fn design_exception_acceptance_targets_requirements_and_gate_templates() {
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
        fs::write(
            init.package_path.join("validation").join("gates.md"),
            validation_gate_doc("GATE-001"),
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

        let requirement_acceptance = accept_design_exception(
            temp.path(),
            NewDesignExceptionAcceptance {
                design_version_id: Some(import.design_version_id),
                design_package: None,
                target: "requirement:REQ-001",
                acceptance_type: "accepted_out_of_scope",
                reason: "not needed for current scope",
            },
        )
        .unwrap();
        let gate_acceptance = accept_design_exception(
            temp.path(),
            NewDesignExceptionAcceptance {
                design_version_id: Some(import.design_version_id),
                design_package: None,
                target: "gate:GATE-001",
                acceptance_type: "explicit_exception",
                reason: "manual validation for this draft",
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

        assert_eq!(requirement_acceptance.target_type, "design_requirement");
        assert!(requirement_acceptance.design_requirement_id.is_some());
        assert_eq!(requirements[0].status, "accepted_out_of_scope".to_string());
        assert_eq!(gate_acceptance.target_type, "validation_gate_template");
        assert!(gate_acceptance.validation_gate_template_id.is_some());
    }

    #[test]
    fn design_import_requires_revision_for_changed_requirement_identity() {
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
        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &init.package_path,
                status: "draft",
            },
        )
        .unwrap();

        fs::write(
            init.package_path.join("requirements").join("README.md"),
            requirement_doc("REQ-001", "Preserve cleanup behavior", "high")
                .replace("one verifiable behavior", "a changed verifiable behavior"),
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

        fs::write(
            init.package_path.join("requirements").join("README.md"),
            r#"## REQ-001: Preserve cleanup behavior
```yaml agent-workbench
type: requirement
key: REQ-001
revision: 2
priority: high
surfaces: [cli, database]
validation: [GATE-001]
status: active
```

This requirement describes a changed verifiable behavior that must be implemented.
"#,
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
        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        let supersedes_count: i64 = conn
            .query_row(
                r#"
                select count(*)
                from design_requirements
                where design_version_id = ?1 and supersedes_requirement_id is not null
                "#,
                params![import.design_version_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(supersedes_count, 1);
    }

    #[test]
    fn design_import_accepts_explicit_requirement_supersession_link() {
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
        import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &init.package_path,
                status: "draft",
            },
        )
        .unwrap();
        fs::write(
            init.package_path.join("requirements").join("README.md"),
            r#"## REQ-002: Preserve cleanup behavior with explicit scope
```yaml agent-workbench
type: requirement
key: REQ-002
priority: high
surfaces: [cli, database]
validation: [GATE-001]
supersedes: [REQ-001]
status: active
```

This requirement replaces the previous cleanup behavior with explicit scope.
"#,
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
        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        let supersedes_key: String = conn
            .query_row(
                r#"
                select previous.requirement_key
                from design_requirements current
                join design_requirements previous on previous.id = current.supersedes_requirement_id
                where current.design_version_id = ?1 and current.requirement_key = 'REQ-002'
                "#,
                params![import.design_version_id],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(supersedes_key, "REQ-001");
    }

    #[test]
    fn design_exception_acceptance_allows_pre_import_size_exceptions() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        let file_package = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "oversized-file",
                title: "Oversized File",
            },
        )
        .unwrap();
        fs::write(
            file_package
                .package_path
                .join("requirements")
                .join("README.md"),
            requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
        )
        .unwrap();
        fs::write(
            file_package.package_path.join("01-introduction-goals.md"),
            std::iter::repeat_n("line", 1001)
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .unwrap();
        let file_acceptance = accept_design_exception(
            temp.path(),
            NewDesignExceptionAcceptance {
                design_version_id: None,
                design_package: Some("oversized-file"),
                target: "file:01-introduction-goals.md",
                acceptance_type: "explicit_exception",
                reason: "temporary source document is larger than the import guardrail",
            },
        )
        .unwrap();
        let file_import = import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &file_package.package_path,
                status: "draft",
            },
        )
        .unwrap();

        let requirement_package = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "oversized-requirement",
                title: "Oversized Requirement",
            },
        )
        .unwrap();
        let oversized_body = std::iter::repeat_n("Requirement detail.", 151)
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(
            requirement_package
                .package_path
                .join("requirements")
                .join("README.md"),
            format!(
                r#"## REQ-001: Preserve cleanup behavior
```yaml agent-workbench
type: requirement
key: REQ-001
priority: high
surfaces: [cli, database]
validation: [GATE-001]
status: active
```

{oversized_body}
"#
            ),
        )
        .unwrap();
        let requirement_acceptance = accept_design_exception(
            temp.path(),
            NewDesignExceptionAcceptance {
                design_version_id: None,
                design_package: Some("oversized-requirement"),
                target: "requirement:REQ-001",
                acceptance_type: "explicit_exception",
                reason: "temporary requirement source is larger than the import guardrail",
            },
        )
        .unwrap();
        let requirement_import = import_design_package(
            temp.path(),
            DesignPackageImport {
                package_path: &requirement_package.package_path,
                status: "draft",
            },
        )
        .unwrap();

        assert_eq!(file_acceptance.target_type, "design_file");
        assert_eq!(
            file_acceptance.design_file_path.as_deref(),
            Some("01-introduction-goals.md")
        );
        assert_eq!(file_import.file_count, 14);
        assert_eq!(requirement_acceptance.target_type, "design_requirement_key");
        assert_eq!(
            requirement_acceptance.design_requirement_key.as_deref(),
            Some("REQ-001")
        );
        assert_eq!(requirement_import.requirement_count, 1);
    }

    #[test]
    fn design_import_reports_size_warnings_without_blocking() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        let init = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "size-warning",
                title: "Size Warning",
            },
        )
        .unwrap();
        fs::write(
            init.package_path.join("01-introduction-goals.md"),
            std::iter::repeat_n("line", 501)
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .unwrap();
        let requirement_body = std::iter::repeat_n("Requirement detail.", 81)
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(
            init.package_path.join("requirements").join("README.md"),
            format!(
                r#"## REQ-001: Preserve cleanup behavior
```yaml agent-workbench
type: requirement
key: REQ-001
priority: high
surfaces: [cli, database]
validation: [GATE-001]
status: active
```

{requirement_body}
"#
            ),
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

        assert_eq!(import.warning_count, 2);
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
    fn design_import_rejects_invalid_phase3_design_blocks() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();

        let init = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "missing-field",
                title: "Missing Field",
            },
        )
        .unwrap();
        fs::write(
            init.package_path.join("requirements").join("README.md"),
            r#"## REQ-001: Missing priority
```yaml agent-workbench
type: requirement
key: REQ-001
surfaces: [cli]
validation: [GATE-001]
status: active
```

Body.
"#,
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

        let init = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "invalid-prefix",
                title: "Invalid Prefix",
            },
        )
        .unwrap();
        fs::write(
            init.package_path.join("requirements").join("README.md"),
            requirement_doc("BAD-001", "Bad prefix", "high").replace("## BAD-001", "## REQ-001"),
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

        let init = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "legacy-doc",
                title: "Legacy Doc",
            },
        )
        .unwrap();
        fs::write(
            init.package_path.join("requirements").join("README.md"),
            r#"## R-001: Legacy key
```yaml agent-workbench
type: requirement
key: R-001
priority: high
surfaces: [cli]
validation: [GATE-001]
status: active
```

Body.
"#,
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
    fn design_import_rejects_non_strict_keys_revisions_and_unknown_metadata() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();

        let bad_requirement_key = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "bad-requirement-key",
                title: "Bad Requirement Key",
            },
        )
        .unwrap();
        fs::write(
            bad_requirement_key
                .package_path
                .join("requirements")
                .join("README.md"),
            requirement_doc("REQ-001 extra", "Bad key", "high"),
        )
        .unwrap();
        assert!(
            import_design_package(
                temp.path(),
                DesignPackageImport {
                    package_path: &bad_requirement_key.package_path,
                    status: "draft",
                },
            )
            .is_err()
        );

        let bad_revision = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "bad-revision",
                title: "Bad Revision",
            },
        )
        .unwrap();
        fs::write(
            bad_revision
                .package_path
                .join("requirements")
                .join("README.md"),
            r#"## REQ-001: Bad revision
```yaml agent-workbench
type: requirement
key: REQ-001
revision: 0
priority: high
surfaces: [cli]
validation: [GATE-001]
status: active
```

Body.
"#,
        )
        .unwrap();
        assert!(
            import_design_package(
                temp.path(),
                DesignPackageImport {
                    package_path: &bad_revision.package_path,
                    status: "draft",
                },
            )
            .is_err()
        );

        let unknown_field = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "unknown-field",
                title: "Unknown Field",
            },
        )
        .unwrap();
        fs::write(
            unknown_field
                .package_path
                .join("requirements")
                .join("README.md"),
            r#"## REQ-001: Unknown field
```yaml agent-workbench
type: requirement
key: REQ-001
priority: high
surfaces: [cli]
validation: [GATE-001]
status: active
surafces: [typo]
```

Body.
"#,
        )
        .unwrap();
        assert!(
            import_design_package(
                temp.path(),
                DesignPackageImport {
                    package_path: &unknown_field.package_path,
                    status: "draft",
                },
            )
            .is_err()
        );

        let bad_decision_key = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "bad-decision-key",
                title: "Bad Decision Key",
            },
        )
        .unwrap();
        fs::write(
            bad_decision_key
                .package_path
                .join("requirements")
                .join("README.md"),
            requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
        )
        .unwrap();
        fs::write(
            bad_decision_key.package_path.join("09-decisions.md"),
            decision_doc().replace("DEC-001", "DEC-bad"),
        )
        .unwrap();
        assert!(
            import_design_package(
                temp.path(),
                DesignPackageImport {
                    package_path: &bad_decision_key.package_path,
                    status: "draft",
                },
            )
            .is_err()
        );

        let bad_gate_key = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "bad-gate-key",
                title: "Bad Gate Key",
            },
        )
        .unwrap();
        fs::write(
            bad_gate_key
                .package_path
                .join("requirements")
                .join("README.md"),
            requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
        )
        .unwrap();
        fs::write(
            bad_gate_key
                .package_path
                .join("validation")
                .join("gates.md"),
            validation_gate_doc("GATE-foo"),
        )
        .unwrap();
        assert!(
            import_design_package(
                temp.path(),
                DesignPackageImport {
                    package_path: &bad_gate_key.package_path,
                    status: "draft",
                },
            )
            .is_err()
        );

        let bad_heading = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "bad-heading",
                title: "Bad Heading",
            },
        )
        .unwrap();
        fs::write(
            bad_heading
                .package_path
                .join("requirements")
                .join("README.md"),
            requirement_doc("REQ-001", "Bad heading", "high")
                .replace("## REQ-001:", "## REQ-001-extra:"),
        )
        .unwrap();
        assert!(
            import_design_package(
                temp.path(),
                DesignPackageImport {
                    package_path: &bad_heading.package_path,
                    status: "draft",
                },
            )
            .is_err()
        );

        let bad_heading_level = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "bad-heading-level",
                title: "Bad Heading Level",
            },
        )
        .unwrap();
        fs::write(
            bad_heading_level
                .package_path
                .join("requirements")
                .join("README.md"),
            r#"### REQ-001: Wrong heading level
```yaml agent-workbench
type: requirement
key: REQ-001
priority: high
surfaces: [cli]
validation: [GATE-001]
status: active
```

Body.
"#,
        )
        .unwrap();
        assert!(
            import_design_package(
                temp.path(),
                DesignPackageImport {
                    package_path: &bad_heading_level.package_path,
                    status: "draft",
                },
            )
            .is_err()
        );

        let bad_arc42_block = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "bad-arc42-block",
                title: "Bad Arc42 Block",
            },
        )
        .unwrap();
        fs::write(
            bad_arc42_block.package_path.join("02-constraints.md"),
            r#"## REQ-001: Wrong section
```yaml agent-workbench
type: requirement
key: REQ-001
priority: high
surfaces: [cli]
validation: [GATE-001]
status: active
```

Body.
"#,
        )
        .unwrap();
        assert!(
            import_design_package(
                temp.path(),
                DesignPackageImport {
                    package_path: &bad_arc42_block.package_path,
                    status: "draft",
                },
            )
            .is_err()
        );

        let hidden_bad_block = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "hidden-bad-block",
                title: "Hidden Bad Block",
            },
        )
        .unwrap();
        fs::write(
            hidden_bad_block
                .package_path
                .join("requirements")
                .join("README.md"),
            r#"## BAD-001: Bad hidden block
```yaml agent-workbench
type: requirement
key: BAD-001
priority: high
surfaces: [cli]
validation: [GATE-001]
status: active
```

Body.
"#,
        )
        .unwrap();
        assert!(
            import_design_package(
                temp.path(),
                DesignPackageImport {
                    package_path: &hidden_bad_block.package_path,
                    status: "draft",
                },
            )
            .is_err()
        );
    }

    #[test]
    fn design_import_rejects_manifest_arc42_key_drift() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        let init = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "manifest-typo",
                title: "Manifest Typo",
            },
        )
        .unwrap();
        let manifest_path = init.package_path.join("design.yaml");
        let manifest = fs::read_to_string(&manifest_path).unwrap();
        fs::write(
            &manifest_path,
            manifest.replace("introduction_goals:", "introducton_goals:"),
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
    fn design_import_rejects_duplicate_decisions_gates_and_oversized_files() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();

        let duplicate_requirement = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "duplicate-requirement",
                title: "Duplicate Requirement",
            },
        )
        .unwrap();
        fs::write(
            duplicate_requirement
                .package_path
                .join("requirements")
                .join("README.md"),
            format!(
                "{}\n{}",
                requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
                requirement_doc("REQ-001", "Preserve cleanup behavior again", "high")
            ),
        )
        .unwrap();
        assert!(
            import_design_package(
                temp.path(),
                DesignPackageImport {
                    package_path: &duplicate_requirement.package_path,
                    status: "draft",
                },
            )
            .is_err()
        );

        let duplicate_decision = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "duplicate-decision",
                title: "Duplicate Decision",
            },
        )
        .unwrap();
        fs::write(
            duplicate_decision
                .package_path
                .join("requirements")
                .join("README.md"),
            requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
        )
        .unwrap();
        fs::write(
            duplicate_decision.package_path.join("09-decisions.md"),
            format!("{}\n{}", decision_doc(), decision_doc()),
        )
        .unwrap();
        assert!(
            import_design_package(
                temp.path(),
                DesignPackageImport {
                    package_path: &duplicate_decision.package_path,
                    status: "draft",
                },
            )
            .is_err()
        );

        let duplicate_gate = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "duplicate-gate",
                title: "Duplicate Gate",
            },
        )
        .unwrap();
        fs::write(
            duplicate_gate
                .package_path
                .join("requirements")
                .join("README.md"),
            requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
        )
        .unwrap();
        fs::write(
            duplicate_gate
                .package_path
                .join("validation")
                .join("gates.md"),
            format!(
                "{}\n{}",
                validation_gate_doc("GATE-001"),
                validation_gate_doc("GATE-001")
            ),
        )
        .unwrap();
        assert!(
            import_design_package(
                temp.path(),
                DesignPackageImport {
                    package_path: &duplicate_gate.package_path,
                    status: "draft",
                },
            )
            .is_err()
        );

        let oversized = init_design_package(
            temp.path(),
            NewDesignPackage {
                design_id: "oversized-file",
                title: "Oversized File",
            },
        )
        .unwrap();
        fs::write(
            oversized
                .package_path
                .join("requirements")
                .join("README.md"),
            requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
        )
        .unwrap();
        fs::write(
            oversized.package_path.join("01-introduction-goals.md"),
            std::iter::repeat_n("line", 1001)
                .collect::<Vec<_>>()
                .join("\n"),
        )
        .unwrap();
        assert!(
            import_design_package(
                temp.path(),
                DesignPackageImport {
                    package_path: &oversized.package_path,
                    status: "draft",
                },
            )
            .is_err()
        );
    }

    #[test]
    fn acceptance_records_enforce_single_typed_target() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();

        let missing_target = conn.execute(
            r#"
            insert into acceptance_records(
                project_id, target_type, acceptance_type, reason, created_by,
                status, created_at
            )
            values (1, 'design_requirement', 'explicit_exception', 'missing target',
                    'user', 'approved', current_timestamp)
            "#,
            [],
        );
        assert!(missing_target.is_err());

        let wrong_target = conn.execute(
            r#"
            insert into acceptance_records(
                project_id, target_type, task_id, design_requirement_id,
                acceptance_type, reason, created_by, status, created_at
            )
            values (1, 'task', 999, 999, 'explicit_exception', 'too many targets',
                    'user', 'approved', current_timestamp)
            "#,
            [],
        );
        assert!(wrong_target.is_err());
    }

    #[test]
    fn acceptance_records_enforce_design_target_project_match() {
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
        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        conn.execute(
            r#"
            insert into projects(name, root_path, created_at, updated_at)
            values ('other', '/tmp/agent-workbench-other', current_timestamp, current_timestamp)
            "#,
            [],
        )
        .unwrap();
        let requirement_id: i64 = conn
            .query_row(
                "select id from design_requirements where design_version_id = ?1",
                params![import.design_version_id],
                |row| row.get(0),
            )
            .unwrap();

        let cross_project = conn.execute(
            r#"
            insert into acceptance_records(
                project_id, target_type, design_requirement_id, acceptance_type,
                reason, created_by, status, created_at
            )
            values (2, 'design_requirement', ?1, 'explicit_exception',
                    'wrong project', 'user', 'approved', current_timestamp)
            "#,
            params![requirement_id],
        );

        assert!(cross_project.is_err());
    }

    #[test]
    fn acceptance_records_enforce_task_target_project_match() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        start_work(temp.path(), "scoped work", None).unwrap();
        let task = add_task(
            temp.path(),
            NewTask {
                title: "project-local task",
                priority: "medium",
                source: "user",
                work_unit_id: None,
                details: None,
                completion_condition: None,
            },
        )
        .unwrap();
        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        conn.execute(
            r#"
            insert into projects(name, root_path, created_at, updated_at)
            values ('other', '/tmp/agent-workbench-other', current_timestamp, current_timestamp)
            "#,
            [],
        )
        .unwrap();

        let cross_project = conn.execute(
            r#"
            insert into acceptance_records(
                project_id, target_type, task_id, acceptance_type,
                reason, created_by, status, created_at
            )
            values (2, 'task', ?1, 'accepted_out_of_scope',
                    'wrong project', 'user', 'approved', current_timestamp)
            "#,
            params![task.task_id],
        );

        assert!(cross_project.is_err());
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

    fn decision_doc() -> String {
        r#"## DEC-001: Keep project-local ledger
```yaml agent-workbench
type: decision
key: DEC-001
status: accepted
supersedes: []
```

Use one SQLite ledger per project.
"#
        .to_string()
    }

    fn validation_gate_doc(key: &str) -> String {
        format!(
            r#"## {key}: Unit test command
```yaml agent-workbench
type: validation_gate_template
key: {key}
applies_to: [REQ-001]
expected_result: pass
phase: implementation
status: active
```

Run the project test suite before implementation handoff.
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
