mod authority;
mod commands;
mod coverage;
mod db;
mod design;
mod kpt;
mod planning;
mod records;
mod review;
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
    KptItemCommandProfileConversion, KptItemCommandProfileConversionOutcome,
    KptItemConversionOutcome, KptItemDecisionConversion, KptItemDecisionConversionOutcome,
    KptItemDesignVersionConversion, KptItemDesignVersionConversionOutcome, KptItemOutcome,
    KptItemRecord, KptItemReviewPolicyConversion, KptItemReviewPolicyConversionOutcome,
    KptItemTaskConversion, KptReviewCloseOutcome, KptReviewOutcome, KptReviewRecord, NewKptItem,
    NewKptReview, add_kpt_item, close_kpt_review, convert_kpt_item_to_command_profile,
    convert_kpt_item_to_decision, convert_kpt_item_to_design_version,
    convert_kpt_item_to_review_policy, convert_kpt_item_to_task, list_kpt_items, list_kpt_reviews,
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
pub use review::{
    ClosureOutcome, FindingClassificationOutcome, FindingOutcome, FindingRecord,
    FindingVerificationOutcome, NewClosure, NewFinding, NewFindingVerification, NewReviewPlan,
    NewReviewPolicy, NewReviewRun, NewReviewScope, ReviewPlanOutcome, ReviewPlanRecord,
    ReviewPlanTargetRecord, ReviewPolicyOutcome, ReviewPolicyRecord, ReviewRunOutcome,
    ReviewRunRecord, ReviewScopeOutcome, ReviewScopeRecord, add_closure, add_finding,
    add_finding_verification, add_review_plan, add_review_policy, add_review_run, classify_finding,
    list_findings, list_review_plan_targets, list_review_plans, list_review_policies,
    list_review_runs, list_review_scopes, start_review_scope,
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
    fn init_migrates_existing_kpt_item_status_constraint() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        conn.execute_batch(
            r#"
            pragma foreign_keys = off;

            drop table kpt_item_conversions;
            alter table kpt_items rename to kpt_items_current;

            create table kpt_items (
                id integer primary key,
                kpt_review_id integer not null references kpt_reviews(id) on delete cascade,
                item_type text not null check (item_type in ('keep', 'problem', 'try')),
                title text not null,
                details text,
                severity text not null default 'medium' check (severity in ('critical', 'high', 'medium', 'low')),
                linked_user_correction_id integer references user_corrections(id),
                linked_command_profile_id integer references command_profiles(id),
                linked_review_finding_id integer,
                linked_task_id integer references tasks(id),
                proposed_action text,
                status text not null default 'open' check (status in ('open', 'accepted', 'converted_to_task', 'dismissed')),
                created_at text not null
            );

            insert into kpt_items(
                id, kpt_review_id, item_type, title, details, severity,
                linked_user_correction_id, linked_command_profile_id,
                linked_review_finding_id, linked_task_id, proposed_action, status, created_at
            )
            select
                id, kpt_review_id, item_type, title, details, severity,
                linked_user_correction_id, linked_command_profile_id,
                linked_review_finding_id, linked_task_id, proposed_action, status, created_at
            from kpt_items_current;

            drop table kpt_items_current;

            create table kpt_item_conversions (
                id integer primary key,
                kpt_item_id integer not null references kpt_items(id) on delete cascade,
                target_type text not null check (target_type in ('task', 'command_profile', 'review_policy', 'design_version', 'decision', 'user_correction')),
                task_id integer references tasks(id),
                command_profile_id integer references command_profiles(id),
                review_policy_id integer,
                design_version_id integer,
                decision_id integer references decisions(id),
                user_correction_id integer references user_corrections(id),
                created_at text not null
            );

            pragma foreign_keys = on;
            "#,
        )
        .unwrap();
        drop(conn);

        init_project(temp.path()).unwrap();
        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        conn.execute(
            r#"
            insert into kpt_reviews(project_id, trigger, summary, status, created_at)
            values (1, 'manual', 'migration check', 'open', current_timestamp)
            "#,
            [],
        )
        .unwrap();
        conn.execute(
            r#"
            insert into kpt_items(
                kpt_review_id, item_type, title, severity, status, created_at
            )
            values (1, 'try', 'converted status is generic', 'medium', 'converted', current_timestamp)
            "#,
            [],
        )
        .unwrap();
    }

    #[test]
    fn init_migrates_existing_review_run_type_purpose_constraint() {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        conn.execute_batch(
            r#"
            pragma foreign_keys = off;

            drop table review_agent_invocations;
            drop table finding_verifications;
            drop table findings;
            drop table review_runs;

            create table review_runs (
                id integer primary key,
                project_id integer not null references projects(id) on delete cascade,
                review_scope_id integer references review_scopes(id),
                review_plan_id integer references review_plans(id),
                run_type text not null check (run_type in ('fresh', 'resume', 'coverage')),
                run_purpose text not null check (run_purpose in ('new_unbiased_review', 'finding_fix_verification', 'coverage_audit')),
                target_type text not null check (target_type in ('design_version', 'design_requirement', 'task', 'work_unit', 'repository_snapshot', 'file', 'symbol')),
                design_version_id integer references design_versions(id),
                design_requirement_id integer references design_requirements(id),
                task_id integer references tasks(id),
                work_unit_id integer references work_units(id),
                repository_snapshot_id integer,
                target_ref text,
                prompt_deviations text,
                result_summary text,
                new_findings_count integer not null default 0,
                carried_findings_checked integer not null default 0,
                clean_run integer not null default 0 check (clean_run in (0, 1)),
                status text not null default 'requested' check (status in ('requested', 'running', 'completed', 'failed', 'cancelled')),
                created_at text not null
            );

            create table review_agent_invocations (
                id integer primary key,
                project_id integer not null references projects(id) on delete cascade,
                review_plan_id integer references review_plans(id),
                review_run_id integer references review_runs(id),
                run_type text not null check (run_type in ('fresh', 'resume', 'coverage')),
                agent_label text,
                external_agent_id text,
                status text not null default 'requested' check (status in ('requested', 'running', 'completed', 'failed', 'cancelled')),
                started_at text,
                finished_at text
            );

            create table findings (
                id integer primary key,
                project_id integer not null references projects(id) on delete cascade,
                review_run_id integer not null references review_runs(id) on delete cascade,
                finding_type text not null check (finding_type in ('design_finding', 'design_implementation_drift', 'design_task_gap', 'implementation_finding', 'coverage_finding')),
                severity text not null check (severity in ('critical', 'high', 'medium', 'low')),
                description text not null,
                classification text not null default 'unclassified' check (classification in ('unclassified', 'valid', 'invalid', 'design_conflict', 'needs_evidence')),
                status text not null default 'open' check (status in ('open', 'closed', 'accepted_out_of_scope')),
                design_requirement_id integer references design_requirements(id),
                task_id integer references tasks(id),
                created_at text not null
            );

            create table finding_verifications (
                id integer primary key,
                project_id integer not null references projects(id) on delete cascade,
                review_run_id integer not null references review_runs(id) on delete cascade,
                finding_id integer not null references findings(id) on delete cascade,
                closure_id integer not null references closures(id) on delete cascade,
                result text not null check (result in ('verified', 'not_fixed', 'needs_evidence', 'out_of_scope')),
                notes text,
                created_at text not null,
                unique(review_run_id, finding_id, closure_id)
            );

            pragma foreign_keys = on;
            "#,
        )
        .unwrap();
        drop(conn);

        init_project(temp.path()).unwrap();
        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        let stale_trigger_refs: i64 = conn
            .query_row(
                r#"
                select count(*)
                from sqlite_schema
                where type = 'trigger'
                  and coalesce(sql, '') like '%review_runs_old%'
                "#,
                [],
                |row| row.get(0),
            )
            .unwrap();
        conn.execute(
            "insert into work_units(project_id, title, status, started_at) values (1, 'migration target', 'open', current_timestamp)",
            [],
        )
        .unwrap();
        let invalid = conn.execute(
            r#"
            insert into review_runs(
                project_id, run_type, run_purpose, target_type, work_unit_id, target_ref,
                new_findings_count, carried_findings_checked, clean_run, status, created_at
            )
            values (1, 'fresh', 'finding_fix_verification', 'work_unit', 1, 'work_unit:1', 0, 0, 0, 'completed', current_timestamp)
            "#,
            [],
        );
        conn.execute(
            r#"
            insert into review_runs(
                project_id, run_type, run_purpose, target_type, work_unit_id, target_ref,
                new_findings_count, carried_findings_checked, clean_run, status, created_at
            )
            values (1, 'fresh', 'new_unbiased_review', 'work_unit', 1, 'work_unit:1', 0, 0, 0, 'completed', current_timestamp)
            "#,
            [],
        )
        .unwrap();
        let invalid_update = conn.execute(
            "update review_runs set run_purpose = 'finding_fix_verification' where id = 1",
            [],
        );

        assert_eq!(stale_trigger_refs, 0);
        assert!(invalid.is_err());
        assert!(invalid_update.is_err());
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
    fn trace_aware_resume_blocks_stale_coverage_items() {
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
                derivation_reason: None,
                checklist_title: None,
                item_title: None,
                completion_condition: None,
            },
        )
        .unwrap();
        add_coverage_item(
            temp.path(),
            NewCoverageItem {
                design_version_id: import_a.design_version_id,
                requirement_key: "REQ-001",
                review_scope_id: None,
                work_unit_id: None,
                task_id: Some(task.task_id),
                requirement: "cleanup behavior is connected",
                runtime_boundary_evidence: None,
                ux_boundary_evidence: None,
                lifecycle_boundary_evidence: None,
                tests_or_gates: Some("GATE-001"),
                missing_or_unverified: None,
                status: "covered",
            },
        )
        .unwrap();
        suspend_work(temp.path(), "design changed", "resume after trace check").unwrap();
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

        let check = resume_check(temp.path(), "trace-aware").unwrap();

        assert_eq!(check.result, "blocked");
        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        let design_current_result: String = conn
            .query_row(
                r#"
                select result
                from resume_check_items
                where resume_check_id = ?1 and check_name = 'design_version_current'
                "#,
                params![check.resume_check_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(design_current_result, "fail");
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

        let plan_project_break = conn.execute(
            "update review_plans set work_unit_id = 2 where id = ?1",
            params![plan.review_plan_id],
        );
        let plan_type_break = conn.execute(
            "update review_plans set review_type = 'design_review' where id = ?1",
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
        assert!(policy_type_break.is_err());
        assert!(scope_type_break.is_err());
        assert!(run_project_break.is_err());
        assert!(run_target_update_break.is_err());
        assert!(run_plan_target_update_break.is_err());
        assert!(run_target_insert_break.is_err());
        assert!(run_plan_target_insert_break.is_err());
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
        close_active_work(temp.path(), "switch plans", None).unwrap();
        let second_work = start_work(temp.path(), "second plan", None).unwrap();
        let second_plan = add_review_plan(
            temp.path(),
            NewReviewPlan {
                work_unit_id: second_work.work_unit_id,
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
                severity: "critical",
                description: "critical but not a stop condition",
                design_requirement_id: None,
                task_id: None,
            },
        )
        .unwrap();

        let plans = list_review_plans(temp.path()).unwrap();
        assert_eq!(plans[0].status, "clean");
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
        let raw_out_of_scope_coverage = add_coverage_item(
            temp.path(),
            NewCoverageItem {
                design_version_id: import.design_version_id,
                requirement_key: "REQ-001",
                review_scope_id: None,
                work_unit_id: None,
                task_id: Some(task_one.task_id),
                requirement: "cleanup behavior is accepted out of scope",
                runtime_boundary_evidence: None,
                ux_boundary_evidence: None,
                lifecycle_boundary_evidence: None,
                tests_or_gates: None,
                missing_or_unverified: Some("not verified"),
                status: "accepted_out_of_scope",
            },
        );

        assert!(mismatched_evidence.is_err());
        assert!(mismatched_gate.is_err());
        assert!(mismatched_coverage.is_err());
        assert!(raw_out_of_scope_coverage.is_err());
    }

    #[test]
    fn approved_coverage_acceptance_can_satisfy_trace_closure() {
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
                completion_condition: Some("cleanup behavior is covered or accepted"),
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
        add_implementation_evidence(
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
        let coverage = add_coverage_item(
            temp.path(),
            NewCoverageItem {
                design_version_id: import.design_version_id,
                requirement_key: "REQ-001",
                review_scope_id: None,
                work_unit_id: None,
                task_id: Some(task.task_id),
                requirement: "cleanup behavior is intentionally out of scope",
                runtime_boundary_evidence: None,
                ux_boundary_evidence: None,
                lifecycle_boundary_evidence: None,
                tests_or_gates: None,
                missing_or_unverified: Some("not applicable to this implementation"),
                status: "partial",
            },
        )
        .unwrap();
        let close_without_acceptance = close_task(temp.path(), task.task_id, Some("abc123"));
        let acceptance = accept_design_exception(
            temp.path(),
            NewDesignExceptionAcceptance {
                design_version_id: Some(import.design_version_id),
                design_package: None,
                target: &format!("coverage:{}", coverage.coverage_item_id),
                acceptance_type: "accepted_out_of_scope",
                reason: "coverage is explicitly out of scope for this work",
            },
        )
        .unwrap();
        close_task(temp.path(), task.task_id, Some("abc123")).unwrap();
        let ready = implementation_ready(
            temp.path(),
            ImplementationReadyCheck {
                design_version_id: Some(import.design_version_id),
            },
        )
        .unwrap();

        assert!(close_without_acceptance.is_err());
        assert_eq!(acceptance.target_type, "coverage_item");
        assert_eq!(acceptance.coverage_item_id, Some(coverage.coverage_item_id));
        assert_eq!(ready.result, "pass");
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
    fn implementation_ready_marks_selected_gate_stale_when_template_changes() {
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
        let first_import = import_design_package(
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
                design_version_id: first_import.design_version_id,
                summary: None,
            },
        )
        .unwrap();
        derive_task_from_requirement(
            temp.path(),
            NewTaskDerivation {
                design_version_id: first_import.design_version_id,
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
                design_version_id: first_import.design_version_id,
                gate_key: "GATE-001",
                requirement_key: "REQ-001",
                task_id: task.task_id,
                command: None,
            },
        )
        .unwrap();
        fs::write(
            init.package_path.join("validation").join("gates.md"),
            validation_gate_doc("GATE-001").replace(
                "Run the project test suite",
                "Run the full project test suite",
            ),
        )
        .unwrap();
        let second_import = import_design_package(
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
                design_version_id: second_import.design_version_id,
                summary: None,
            },
        )
        .unwrap();

        let outcome = implementation_ready(
            temp.path(),
            ImplementationReadyCheck {
                design_version_id: Some(second_import.design_version_id),
            },
        )
        .unwrap();

        assert!(
            outcome
                .items
                .iter()
                .any(|item| { item.name == "validation_gates_current" && item.result == "fail" })
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
        add_coverage_item(
            temp.path(),
            NewCoverageItem {
                design_version_id: import_a.design_version_id,
                requirement_key: "REQ-001",
                review_scope_id: None,
                work_unit_id: None,
                task_id: Some(task.task_id),
                requirement: "cleanup behavior is connected",
                runtime_boundary_evidence: None,
                ux_boundary_evidence: None,
                lifecycle_boundary_evidence: None,
                tests_or_gates: Some("GATE-001"),
                missing_or_unverified: None,
                status: "covered",
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
        assert!(
            blocked
                .items
                .iter()
                .any(|item| { item.name == "coverage_items_current" && item.result == "fail" })
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
                details: Some(
                    "cargo fmt && cargo test && cargo clippy --all-targets -- -D warnings",
                ),
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
