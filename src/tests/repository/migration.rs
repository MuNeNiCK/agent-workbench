use super::*;

#[test]
fn init_rejects_legacy_cross_project_work_record_links() {
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

        create table projects (
            id integer primary key,
            name text not null,
            root_path text not null,
            created_at text not null,
            updated_at text not null
        );
        insert into projects(id, name, root_path, created_at, updated_at)
        values
            (1, 'main', '/tmp/main-awb-legacy-link', current_timestamp, current_timestamp),
            (2, 'other', '/tmp/other-awb-legacy-link', current_timestamp, current_timestamp);

        create table work_units (
            id integer primary key,
            project_id integer not null,
            title text not null,
            status text not null,
            started_at text
        );
        insert into work_units(id, project_id, title, status, started_at)
        values (1, 2, 'other work', 'open', current_timestamp);

        create table command_profiles (
            id integer primary key,
            project_id integer not null,
            repository_id integer,
            name text not null,
            command text not null,
            command_type text not null,
            scope text,
            status text not null,
            stability text not null,
            working_directory text,
            environment text,
            timeout text,
            expected_result text,
            replaces_command_profile_id integer,
            source text not null,
            created_at text not null,
            updated_at text not null
        );

        create table command_usages (
            id integer primary key,
            command_profile_id integer,
            work_unit_id integer,
            work_unit_activation_id integer,
            command text not null,
            result text not null,
            log_path text,
            repository_snapshot_id integer,
            created_at text not null
        );
        insert into command_usages(id, work_unit_id, command, result, created_at)
        values (1, 1, 'cargo test', 'pass', current_timestamp);

        create table work_records (
            id integer primary key,
            work_unit_id integer,
            topic text not null,
            work_performed text,
            next_actions text,
            notable_operations text,
            export_path text,
            created_at text not null
        );
        insert into work_records(id, work_unit_id, topic, created_at)
        values (1, null, 'legacy detached record', current_timestamp);

        create table work_record_commands (
            id integer primary key,
            work_record_id integer not null,
            command_usage_id integer,
            command_profile_id integer,
            command text,
            result text,
            log_path text,
            note text
        );
        insert into work_record_commands(id, work_record_id, command_usage_id)
        values (1, 1, 1);
        "#,
    )
    .unwrap();
    drop(conn);

    let inspection = inspect_update(temp.path()).unwrap();
    let result = apply_update(temp.path(), &inspection.current_identity);

    assert!(result.is_err());
    let error = format!("{:#}", result.unwrap_err());
    assert!(
        error.contains("work_record_commands contains cross-project links"),
        "{error}"
    );
}

#[test]
fn init_rejects_legacy_links_with_missing_repository_parent_rows() {
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

        create table projects (
            id integer primary key,
            name text not null,
            root_path text not null,
            created_at text not null,
            updated_at text not null
        );
        insert into projects(id, name, root_path, created_at, updated_at)
        values (1, 'main', '/tmp/main-awb-missing-repo', current_timestamp, current_timestamp);

        create table repository_snapshots (
            id integer primary key,
            repository_id integer not null,
            is_clean integer not null,
            created_at text not null
        );
        insert into repository_snapshots(id, repository_id, is_clean, created_at)
        values (1, 999, 1, current_timestamp);

        create table command_usages (
            id integer primary key,
            command_profile_id integer,
            work_unit_id integer,
            work_unit_activation_id integer,
            command text not null,
            result text not null,
            log_path text,
            repository_snapshot_id integer,
            created_at text not null
        );
        insert into command_usages(id, command, result, repository_snapshot_id, created_at)
        values (1, 'cargo test', 'pass', 1, current_timestamp);
        "#,
    )
    .unwrap();
    drop(conn);

    let inspection = inspect_update(temp.path()).unwrap();
    let result = apply_update(temp.path(), &inspection.current_identity);

    assert!(result.is_err());
    let error = format!("{:#}", result.unwrap_err());
    assert!(
        error.contains("command_usages contains rows without a valid project_id"),
        "{error}"
    );

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

        create table projects (
            id integer primary key,
            name text not null,
            root_path text not null,
            created_at text not null,
            updated_at text not null
        );
        insert into projects(id, name, root_path, created_at, updated_at)
        values (1, 'main', '/tmp/main-awb-missing-commit-repo', current_timestamp, current_timestamp);

        create table work_records (
            id integer primary key,
            work_unit_id integer,
            topic text not null,
            work_performed text,
            next_actions text,
            notable_operations text,
            export_path text,
            created_at text not null
        );
        insert into work_records(id, topic, created_at)
        values (1, 'legacy record', current_timestamp);

        create table git_commits (
            id integer primary key,
            repository_id integer not null,
            commit_sha text not null,
            created_at text not null
        );
        insert into git_commits(id, repository_id, commit_sha, created_at)
        values (1, 999, 'abc123', current_timestamp);

        create table work_record_commits (
            id integer primary key,
            work_record_id integer not null,
            git_commit_id integer,
            commit_sha text,
            role text not null,
            note text
        );
        insert into work_record_commits(id, work_record_id, git_commit_id, commit_sha, role)
        values (1, 1, 1, 'abc123', 'referenced');
        "#,
    )
    .unwrap();
    drop(conn);

    let inspection = inspect_update(temp.path()).unwrap();
    let result = apply_update(temp.path(), &inspection.current_identity);

    assert!(result.is_err());
    let error = format!("{:#}", result.unwrap_err());
    assert!(
        error.contains("work_record_commits contains invalid git links"),
        "{error}"
    );
}

#[test]
fn init_marks_pre_marker_work_record_git_links_as_auto_linked() {
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

        create table projects (
            id integer primary key,
            name text not null,
            root_path text not null,
            created_at text not null,
            updated_at text not null
        );
        insert into projects(id, name, root_path, created_at, updated_at)
        values (1, 'main', '/tmp/main-awb-auto-link-marker', current_timestamp, current_timestamp);

        create table repositories (
            id integer primary key,
            project_id integer not null,
            name text not null,
            path text not null,
            status_summary text,
            last_checked_at text not null
        );
        insert into repositories(id, project_id, name, path, status_summary, last_checked_at)
        values (1, 1, 'main', '.', 'clean', current_timestamp);

        create table git_commits (
            id integer primary key,
            repository_id integer not null,
            commit_sha text not null,
            short_sha text,
            subject text,
            author_name text,
            author_email text,
            committed_at text,
            parent_shas text,
            imported_at text not null
        );
        insert into git_commits(id, repository_id, commit_sha, short_sha, subject, imported_at)
        values (1, 1, 'abc123', 'abc123', 'legacy', current_timestamp);

        create table git_file_changes (
            id integer primary key,
            git_commit_id integer not null,
            repository_id integer not null,
            path text not null,
            old_path text,
            change_type text not null,
            additions integer,
            deletions integer,
            content_hash text
        );
        insert into git_file_changes(id, git_commit_id, repository_id, path, change_type)
        values (1, 1, 1, 'src/lib.rs', 'modified');

        create table work_records (
            id integer primary key,
            project_id integer,
            work_unit_id integer,
            topic text not null,
            work_performed text,
            next_actions text,
            notable_operations text,
            export_path text,
            created_at text not null
        );
        insert into work_records(id, project_id, work_unit_id, topic, created_at)
        values (1, 1, null, 'legacy linked record', current_timestamp);

        create table work_record_commits (
            id integer primary key,
            work_record_id integer not null,
            git_commit_id integer,
            commit_sha text,
            role text not null,
            note text
        );
        insert into work_record_commits(id, work_record_id, git_commit_id, commit_sha, role)
        values (1, 1, 1, 'abc123', 'created');

        create table work_record_files (
            id integer primary key,
            work_record_id integer not null,
            git_file_change_id integer,
            repository_id integer,
            path text not null,
            role text not null,
            note text
        );
        insert into work_record_files(id, work_record_id, git_file_change_id, repository_id, path, role)
        values (1, 1, 1, 1, 'src/lib.rs', 'changed');
        "#,
    )
    .unwrap();
    drop(conn);

    apply_test_update(temp.path());

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let commit_auto_linked: i64 = conn
        .query_row(
            "select auto_linked from work_record_commits where id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let file_markers: (i64, i64) = conn
        .query_row(
            "select auto_linked, repository_auto_linked from work_record_files where id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(commit_auto_linked, 1);
    assert_eq!(file_markers, (1, 1));
}

#[test]
fn init_preserves_intermediate_auto_linked_repository_scope() {
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

        create table projects (
            id integer primary key,
            name text not null,
            root_path text not null,
            created_at text not null,
            updated_at text not null
        );
        insert into projects(id, name, root_path, created_at, updated_at)
        values (1, 'main', '/tmp/main-awb-intermediate-marker', current_timestamp, current_timestamp);

        create table repositories (
            id integer primary key,
            project_id integer not null,
            name text not null,
            path text not null,
            status_summary text,
            last_checked_at text not null
        );
        insert into repositories(id, project_id, name, path, status_summary, last_checked_at)
        values (1, 1, 'main', '.', 'clean', current_timestamp);

        create table git_commits (
            id integer primary key,
            repository_id integer not null,
            commit_sha text not null,
            short_sha text,
            subject text,
            author_name text,
            author_email text,
            committed_at text,
            parent_shas text,
            imported_at text not null
        );
        insert into git_commits(id, repository_id, commit_sha, short_sha, subject, imported_at)
        values (1, 1, 'abc123', 'abc123', 'intermediate', current_timestamp);

        create table git_file_changes (
            id integer primary key,
            git_commit_id integer not null,
            repository_id integer not null,
            path text not null,
            old_path text,
            change_type text not null,
            additions integer,
            deletions integer,
            content_hash text
        );
        insert into git_file_changes(id, git_commit_id, repository_id, path, change_type)
        values (1, 1, 1, 'src/lib.rs', 'modified');

        create table work_records (
            id integer primary key,
            project_id integer,
            work_unit_id integer,
            topic text not null,
            work_performed text,
            next_actions text,
            notable_operations text,
            export_path text,
            created_at text not null
        );
        insert into work_records(id, project_id, work_unit_id, topic, created_at)
        values (1, 1, null, 'intermediate linked record', current_timestamp);

        create table work_record_files (
            id integer primary key,
            work_record_id integer not null,
            git_file_change_id integer,
            repository_id integer,
            path text not null,
            role text not null,
            note text,
            auto_linked integer not null default 0
        );
        insert into work_record_files(
            id, work_record_id, git_file_change_id, repository_id, path, role, auto_linked
        )
        values (1, 1, 1, 1, 'src/lib.rs', 'changed', 1);
        "#,
    )
    .unwrap();
    drop(conn);

    apply_test_update(temp.path());

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let markers: (i64, i64) = conn
        .query_row(
            "select auto_linked, repository_auto_linked from work_record_files where id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(markers, (1, 0));
}

#[test]
fn fork_work_normalizes_freeform_reason_to_other_code() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "freeform fork reason", None).unwrap();
    let record = create_work_record(
        temp.path(),
        NewWorkRecord {
            work_unit_id: Some(work.work_unit_id),
            topic: "fork source",
            work_performed: None,
            next_actions: None,
            notable_operations: None,
            export_path: None,
        },
    )
    .unwrap();
    suspend_work(temp.path(), "pause before fork", "fork from record").unwrap();

    let fork = fork_work(
        temp.path(),
        NewWorkFork {
            title: "redo branch",
            source: WorkForkSource::Record(record.work_record_id),
            reason: "redo from bad implementation",
            discard_policy: "keep_history",
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let stored_reason: String = conn
        .query_row(
            "select fork_reason from work_record_forks where id = ?1",
            params![fork.fork_id],
            |row| row.get(0),
        )
        .unwrap();
    let work_reason: String = conn
        .query_row(
            "select interrupt_reason from work_units where id = ?1",
            params![fork.work_unit_id],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(stored_reason, "other");
    assert_eq!(work_reason, "redo from bad implementation");
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

#[test]
fn validation_runs_record_gate_results_and_enforce_project_links() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "validation run work", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "validate cleanup",
            priority: "high",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("validation run is recorded"),
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
    derive_task_from_requirement(
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
    let gate = select_validation_gate(
        temp.path(),
        ValidationGateSelection {
            design_version_id: import.design_version_id,
            gate_key: "GATE-001",
            requirement_key: "REQ-001",
            task_id: task.task_id,
            command: Some("cargo test"),
            command_profile: None,
            timeout: None,
        },
    )
    .unwrap();
    add_repository(
        temp.path(),
        NewRepository {
            name: "main",
            path: ".",
            current_head: Some("abc123"),
            status_summary: Some("clean"),
        },
    )
    .unwrap();
    let snapshot = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(work.activation_id),
            head_sha: Some("abc123"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    let usage = add_command_usage_with_repository_snapshot(
        temp.path(),
        NewCommandUsageWithRepositorySnapshot {
            profile: None,
            command: Some("cargo test"),
            result: "pass",
            log_path: Some(".agent-workbench/logs/cargo-test.log"),
            work_unit_id: Some(work.work_unit_id),
            repository_snapshot_id: Some(snapshot.repository_snapshot_id),
        },
    )
    .unwrap();
    let other_snapshot = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(work.activation_id),
            head_sha: Some("def456"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    let other_usage = add_command_usage_with_repository_snapshot(
        temp.path(),
        NewCommandUsageWithRepositorySnapshot {
            profile: None,
            command: Some("cargo test"),
            result: "pass",
            log_path: Some(".agent-workbench/logs/other-test.log"),
            work_unit_id: Some(work.work_unit_id),
            repository_snapshot_id: Some(other_snapshot.repository_snapshot_id),
        },
    )
    .unwrap();

    let run = add_validation_run(
        temp.path(),
        NewValidationRun {
            validation_gate_id: gate.validation_gate_id,
            command_usage_id: Some(usage.command_usage_id),
            repository_snapshot_id: Some(snapshot.repository_snapshot_id),
            result: "pass",
            command: None,
            classification: None,
            acceptance_record_id: None,
            artifact_path: Some(".agent-workbench/logs/cargo-test.log"),
            artifact_hash: Some("sha256:abc"),
            notes: Some("full test suite passed"),
        },
    )
    .unwrap();
    let records = list_validation_runs(
        temp.path(),
        ValidationRunListQuery {
            validation_gate_id: Some(gate.validation_gate_id),
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let artifact: (String, String, String, i64, i64, i64) = conn
        .query_row(
            r#"
            select artifact_type, identity_key, artifact_path,
                   validation_run_id, command_usage_id, repository_snapshot_id
            from artifacts
            where validation_run_id = ?1
            "#,
            params![run.validation_run_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .unwrap();
    conn.execute(
        "insert into projects(name, root_path, created_at, updated_at) values ('other', '/tmp/other-awb-validation-run', current_timestamp, current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        "insert into work_units(project_id, title, status, started_at) values (2, 'other work', 'open', current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        "insert into validation_gates(project_id, gate_key, work_unit_id, task_id, expected_result, status, created_at) values (2, 'OTHER-GATE', 2, null, 'pass', 'active', current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        "insert into work_units(project_id, title, status, started_at) values (1, 'same project other work', 'open', current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        "insert into tasks(title, priority, source, work_unit_id, status) values ('other task', 'medium', 'user', (select max(id) from work_units where project_id = 1), 'open')",
        [],
    )
    .unwrap();
    conn.execute(
        r#"
        insert into command_usages(
            project_id, work_unit_id, command, result, repository_snapshot_id, created_at
        )
        values (
            1,
            (select max(id) from work_units where project_id = 1),
            'cargo test',
            'pass',
            ?1,
            current_timestamp
        )
        "#,
        params![snapshot.repository_snapshot_id],
    )
    .unwrap();
    let wrong_work_usage_id = conn.last_insert_rowid();
    let same_project_wrong_work_run = conn.execute(
        r#"
        insert into validation_runs(
            project_id, validation_gate_id, work_unit_id, task_id, command_usage_id,
            repository_snapshot_id, result, created_at
        )
        values (
            1, ?1,
            (select max(id) from work_units where project_id = 1),
            (select max(id) from tasks),
            ?2, ?3, 'pass', current_timestamp
        )
        "#,
        params![
            gate.validation_gate_id,
            usage.command_usage_id,
            snapshot.repository_snapshot_id
        ],
    );
    let wrong_work_usage_run = add_validation_run(
        temp.path(),
        NewValidationRun {
            validation_gate_id: gate.validation_gate_id,
            command_usage_id: Some(wrong_work_usage_id),
            repository_snapshot_id: Some(snapshot.repository_snapshot_id),
            result: "pass",
            command: None,
            classification: None,
            acceptance_record_id: None,
            artifact_path: None,
            artifact_hash: None,
            notes: Some("wrong work unit command usage"),
        },
    );
    let wrong_work_usage_direct_run = conn.execute(
        r#"
        insert into validation_runs(
            project_id, validation_gate_id, work_unit_id, task_id, command_usage_id,
            repository_snapshot_id, result, created_at
        )
        values (1, ?1, ?2, ?3, ?4, ?5, 'pass', current_timestamp)
        "#,
        params![
            gate.validation_gate_id,
            work.work_unit_id,
            task.task_id,
            wrong_work_usage_id,
            snapshot.repository_snapshot_id
        ],
    );
    let mismatched_usage_artifact = conn.execute(
        r#"
        insert into artifacts(
            project_id, artifact_type, identity_key, artifact_path,
            validation_run_id, command_usage_id, repository_snapshot_id, created_at
        )
        values (1, 'validation_output', 'usage-mismatch', 'usage-mismatch.log', ?1, ?2, ?3, current_timestamp)
        "#,
        params![
            run.validation_run_id,
            other_usage.command_usage_id,
            snapshot.repository_snapshot_id
        ],
    );
    let mismatched_snapshot_artifact = conn.execute(
        r#"
        insert into artifacts(
            project_id, artifact_type, identity_key, artifact_path,
            validation_run_id, command_usage_id, repository_snapshot_id, created_at
        )
        values (1, 'validation_output', 'snapshot-mismatch', 'snapshot-mismatch.log', ?1, ?2, ?3, current_timestamp)
        "#,
        params![
            run.validation_run_id,
            usage.command_usage_id,
            other_snapshot.repository_snapshot_id
        ],
    );
    let validation_snapshot = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(work.activation_id),
            head_sha: Some("ghi789"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    add_validation_run(
        temp.path(),
        NewValidationRun {
            validation_gate_id: gate.validation_gate_id,
            command_usage_id: None,
            repository_snapshot_id: Some(validation_snapshot.repository_snapshot_id),
            result: "pass",
            command: None,
            classification: None,
            acceptance_record_id: None,
            artifact_path: None,
            artifact_hash: None,
            notes: Some("snapshot-only validation"),
        },
    )
    .unwrap();
    let artifact_snapshot = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(work.activation_id),
            head_sha: Some("jkl012"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    conn.execute(
        r#"
        insert into artifacts(
            project_id, artifact_type, identity_key, artifact_path,
            repository_snapshot_id, created_at
        )
        values (1, 'other', 'manual-artifact', 'manual-artifact.log', ?1, current_timestamp)
        "#,
        params![artifact_snapshot.repository_snapshot_id],
    )
    .unwrap();
    let cross_project_run = conn.execute(
        r#"
        insert into validation_runs(
            project_id, validation_gate_id, work_unit_id, command_usage_id,
            repository_snapshot_id, result, created_at
        )
        values (1, 2, ?1, ?2, ?3, 'pass', current_timestamp)
        "#,
        params![
            work.work_unit_id,
            usage.command_usage_id,
            snapshot.repository_snapshot_id
        ],
    );

    assert_eq!(run.work_unit_id, Some(work.work_unit_id));
    assert_eq!(run.task_id, Some(task.task_id));
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].command_usage_id, Some(usage.command_usage_id));
    assert_eq!(
        records[0].repository_snapshot_id,
        Some(snapshot.repository_snapshot_id)
    );
    assert_eq!(records[0].result, "pass");
    assert_eq!(
        records[0].artifact_path.as_deref(),
        Some(".agent-workbench/logs/cargo-test.log")
    );
    assert_eq!(
        artifact,
        (
            "validation_output".to_string(),
            "sha256:abc".to_string(),
            ".agent-workbench/logs/cargo-test.log".to_string(),
            run.validation_run_id,
            usage.command_usage_id,
            snapshot.repository_snapshot_id
        )
    );
    assert!(mismatched_usage_artifact.is_err());
    assert!(mismatched_snapshot_artifact.is_err());
    assert!(same_project_wrong_work_run.is_err());
    assert!(wrong_work_usage_run.is_err());
    assert!(wrong_work_usage_direct_run.is_err());
    assert!(
        conn.execute(
            "delete from repository_snapshots where id = ?1",
            params![validation_snapshot.repository_snapshot_id],
        )
        .is_err()
    );
    assert!(
        conn.execute(
            "delete from repository_snapshots where id = ?1",
            params![artifact_snapshot.repository_snapshot_id],
        )
        .is_err()
    );
    assert!(cross_project_run.is_err());
}
