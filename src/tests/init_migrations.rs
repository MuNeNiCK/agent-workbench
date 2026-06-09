use super::*;

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
    let work_unit_id = conn.last_insert_rowid();
    conn.execute(
        r#"
        insert into review_policies(
            project_id, name, review_type, max_fresh_agents, max_resume_agents,
            max_parallel_agents, required_consecutive_clean_fresh_runs,
            required_consecutive_clean_resume_runs, stop_on_severity,
            allow_resume_review, allow_fresh_review, allow_new_findings_in_resume,
            on_max_agents_exceeded, run_count_scope, default_run_mode, created_at
        )
        values (1, 'migration-policy', 'implementation_review', 2, 1, 1, 1, 0, 'none', 1, 1, 0, 'block', 'review_plan', 'fresh', current_timestamp)
        "#,
        [],
    )
    .unwrap();
    let review_policy_id = conn.last_insert_rowid();
    conn.execute(
        r#"
        insert into review_plans(
            project_id, work_unit_id, review_type, required, stage,
            review_policy_id, status, created_at
        )
        values (1, ?1, 'implementation_review', 1, 'close-ready', ?2, 'open', current_timestamp)
        "#,
        params![work_unit_id, review_policy_id],
    )
    .unwrap();
    let review_plan_id = conn.last_insert_rowid();
    conn.execute(
        "insert into review_plan_targets(review_plan_id, target_type, work_unit_id) values (?1, 'work_unit', ?2)",
        params![review_plan_id, work_unit_id],
    )
    .unwrap();
    let invalid = conn.execute(
        r#"
        insert into review_runs(
            project_id, review_plan_id, run_type, run_purpose, target_type, work_unit_id, target_ref,
            new_findings_count, carried_findings_checked, clean_run, status, created_at
        )
        values (1, ?1, 'fresh', 'finding_fix_verification', 'work_unit', ?2, ?3, 0, 0, 0, 'completed', current_timestamp)
        "#,
        params![
            review_plan_id,
            work_unit_id,
            format!("work_unit:{work_unit_id}"),
        ],
    );
    conn.execute(
        r#"
        insert into review_runs(
            project_id, review_plan_id, run_type, run_purpose, target_type, work_unit_id, target_ref,
            new_findings_count, carried_findings_checked, clean_run, status, created_at
        )
        values (1, ?1, 'fresh', 'new_unbiased_review', 'work_unit', ?2, ?3, 0, 0, 0, 'completed', current_timestamp)
        "#,
        params![
            review_plan_id,
            work_unit_id,
            format!("work_unit:{work_unit_id}"),
        ],
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
fn init_rejects_legacy_review_rows_missing_required_links() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".agent-workbench")).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        pragma foreign_keys = off;

        create table projects (
            id integer primary key,
            name text not null,
            root_path text not null unique,
            created_at text not null,
            updated_at text not null
        );

        create table work_units (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            parent_work_unit_id integer references work_units(id),
            title text not null,
            status text not null default 'open' check (status in ('open', 'blocked', 'closed', 'abandoned')),
            responsibility text,
            in_scope text,
            out_of_scope text,
            interrupt_reason text,
            selected_gate_id integer,
            review_plan_status text,
            started_at text not null,
            closed_at text,
            close_summary text
        );

        create table review_policies (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            name text not null,
            review_type text not null check (review_type in ('design_review', 'design_implementation_diff', 'design_task_decomposition', 'implementation_review', 'general')),
            max_fresh_agents integer not null default 1,
            max_resume_agents integer not null default 1,
            max_parallel_agents integer not null default 1,
            required_consecutive_clean_fresh_runs integer not null default 1,
            required_consecutive_clean_resume_runs integer not null default 1,
            stop_on_severity text not null default 'none' check (stop_on_severity in ('critical', 'high', 'medium', 'low', 'none')),
            allow_resume_review integer not null default 1 check (allow_resume_review in (0, 1)),
            allow_fresh_review integer not null default 1 check (allow_fresh_review in (0, 1)),
            allow_new_findings_in_resume integer not null default 0 check (allow_new_findings_in_resume in (0, 1)),
            on_max_agents_exceeded text not null default 'block' check (on_max_agents_exceeded in ('block', 'accept_with_user_approval', 'mark_exhausted')),
            run_count_scope text not null default 'review_plan' check (run_count_scope in ('review_plan', 'review_scope', 'work_unit')),
            default_run_mode text not null default 'fresh' check (default_run_mode in ('fresh', 'resume')),
            created_at text not null,
            unique(project_id, name)
        );

        create table review_plans (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            work_unit_id integer not null references work_units(id) on delete cascade,
            design_version_id integer references design_versions(id) on delete cascade,
            review_type text not null check (review_type in ('design_review', 'design_implementation_diff', 'design_task_decomposition', 'implementation_review', 'general')),
            required integer not null default 1 check (required in (0, 1)),
            stage text not null check (stage in ('design-ready', 'implementation-ready', 'close-ready', 'resume-ready')),
            scope text,
            clean_condition text,
            stop_condition text,
            review_policy_id integer references review_policies(id),
            review_scope_id integer references review_scopes(id),
            status text not null default 'open' check (status in ('open', 'blocked', 'clean', 'accepted_exception', 'not_required', 'exhausted', 'needs_user_decision')),
            created_at text not null
        );

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

        insert into projects(name, root_path, created_at, updated_at)
        values ('legacy', '/tmp/legacy-awb', current_timestamp, current_timestamp);

        insert into work_units(project_id, title, status, started_at)
        values (1, 'legacy work', 'open', current_timestamp);

        insert into review_plans(
            project_id, work_unit_id, review_type, required, stage,
            review_policy_id, status, created_at
        )
        values (1, 1, 'implementation_review', 1, 'close-ready', null, 'open', current_timestamp);

        insert into review_runs(
            project_id, review_plan_id, run_type, run_purpose,
            target_type, work_unit_id, target_ref,
            new_findings_count, carried_findings_checked, clean_run, status, created_at
        )
        values (1, null, 'resume', 'finding_fix_verification', 'work_unit', 1, 'work_unit:1', 1, 0, 0, 'completed', current_timestamp);

        pragma foreign_keys = on;
        "#,
    )
    .unwrap();
    drop(conn);

    let result = init_project(temp.path());

    assert!(result.is_err());
}

#[test]
fn init_refreshes_artifact_and_repository_snapshot_triggers() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        drop trigger trg_artifact_project_insert;
        drop trigger trg_artifact_project_update;
        drop trigger trg_repository_snapshot_referenced_delete;

        create trigger trg_artifact_project_insert
        before insert on artifacts
        for each row
        begin
            select 1;
        end;

        create trigger trg_artifact_project_update
        before update on artifacts
        for each row
        begin
            select 1;
        end;

        create trigger trg_repository_snapshot_referenced_delete
        before delete on repository_snapshots
        for each row
        begin
            select 1;
        end;
        "#,
    )
    .unwrap();
    drop(conn);

    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let artifact_insert_sql: String = conn
        .query_row(
            "select sql from sqlite_schema where type = 'trigger' and name = 'trg_artifact_project_insert'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let snapshot_delete_sql: String = conn
        .query_row(
            "select sql from sqlite_schema where type = 'trigger' and name = 'trg_repository_snapshot_referenced_delete'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert!(artifact_insert_sql.contains("new.command_usage_id is not"));
    assert!(artifact_insert_sql.contains("new.repository_snapshot_id is not"));
    assert!(snapshot_delete_sql.contains("validation_runs where repository_snapshot_id"));
    assert!(snapshot_delete_sql.contains("artifacts where repository_snapshot_id"));
}

#[test]
fn init_rejects_legacy_artifacts_with_invalid_validation_links() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        insert into work_units(project_id, title, status, started_at)
        values (1, 'legacy validation evidence', 'open', current_timestamp);

        insert into repositories(project_id, name, path, status_summary, last_checked_at)
        values (1, 'main', '.', 'clean', current_timestamp);

        insert into repository_snapshots(
            repository_id, head_sha, branch, status_summary, is_clean, created_at
        )
        values (1, 'abc123', 'master', 'clean', 1, current_timestamp);

        insert into repository_snapshots(
            repository_id, head_sha, branch, status_summary, is_clean, created_at
        )
        values (1, 'def456', 'master', 'clean', 1, current_timestamp);

        insert into command_usages(
            project_id, work_unit_id, command, result, repository_snapshot_id, created_at
        )
        values (1, 1, 'cargo test', 'pass', 1, current_timestamp);

        insert into command_usages(
            project_id, work_unit_id, command, result, repository_snapshot_id, created_at
        )
        values (1, 1, 'cargo test', 'pass', 2, current_timestamp);

        insert into validation_gates(
            project_id, gate_key, work_unit_id, expected_result, status, created_at
        )
        values (1, 'GATE-001', 1, 'pass', 'active', current_timestamp);

        insert into validation_runs(
            project_id, validation_gate_id, work_unit_id, command_usage_id,
            repository_snapshot_id, result, created_at
        )
        values (1, 1, 1, 1, 1, 'pass', current_timestamp);

        drop trigger trg_artifact_project_insert;
        drop trigger trg_artifact_project_update;

        insert into artifacts(
            project_id, artifact_type, identity_key, artifact_path,
            validation_run_id, command_usage_id, repository_snapshot_id, created_at
        )
        values (
            1, 'validation_output', 'mismatched-artifact',
            '.agent-workbench/logs/mismatched.log',
            1, 2, 1, current_timestamp
        );
        "#,
    )
    .unwrap();
    drop(conn);

    let result = init_project(temp.path());

    assert!(result.is_err());
}

#[test]
fn init_rejects_legacy_validation_runs_with_gate_work_mismatch() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        insert into work_units(project_id, title, status, started_at)
        values (1, 'gate work', 'open', current_timestamp);

        insert into work_units(project_id, title, status, started_at)
        values (1, 'wrong validation work', 'open', current_timestamp);

        insert into tasks(title, priority, source, work_unit_id, status)
        values ('gate task', 'high', 'design', 1, 'open');

        insert into tasks(title, priority, source, work_unit_id, status)
        values ('wrong task', 'high', 'design', 2, 'open');

        insert into validation_gates(
            project_id, gate_key, work_unit_id, task_id, expected_result, status, created_at
        )
        values (1, 'GATE-001', 1, 1, 'pass', 'active', current_timestamp);

        drop trigger trg_validation_run_project_insert;
        drop trigger trg_validation_run_project_update;

        insert into validation_runs(
            project_id, validation_gate_id, work_unit_id, task_id, result, created_at
        )
        values (1, 1, 2, 2, 'pass', current_timestamp);
        "#,
    )
    .unwrap();
    drop(conn);

    let result = init_project(temp.path());

    assert!(result.is_err());
}
