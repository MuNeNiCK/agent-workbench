use super::*;

#[test]
fn work_record_commands_reject_cross_project_links() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "project one work", None).unwrap();
    let record = create_work_record(
        temp.path(),
        NewWorkRecord {
            work_unit_id: Some(work.work_unit_id),
            topic: "project one record",
            work_performed: None,
            next_actions: None,
            notable_operations: None,
            export_path: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into projects(name, root_path, created_at, updated_at) values ('other', '/tmp/other-awb-command-link', current_timestamp, current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        "insert into work_units(project_id, title, status, started_at) values (2, 'other work', 'open', current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        r#"
        insert into command_profiles(
            project_id, name, command, command_type, status, stability,
            source, created_at, updated_at
        )
        values (2, 'other-test', 'cargo test', 'test', 'fixed', 'stable', 'user', current_timestamp, current_timestamp)
        "#,
        [],
    )
    .unwrap();
    conn.execute(
        r#"
        insert into command_usages(project_id, command_profile_id, work_unit_id, command, result, created_at)
        values (2, 1, 2, 'cargo test', 'pass', current_timestamp)
        "#,
        [],
    )
    .unwrap();

    let cross_usage = conn.execute(
        r#"
        insert into work_record_commands(work_record_id, command_usage_id)
        values (?1, 1)
        "#,
        params![record.work_record_id],
    );
    let cross_profile = conn.execute(
        r#"
        insert into work_record_commands(work_record_id, command_profile_id, command)
        values (?1, 1, 'cargo test')
        "#,
        params![record.work_record_id],
    );

    assert!(cross_usage.is_err());
    assert!(cross_profile.is_err());
}

#[test]
fn work_fork_rejects_cross_project_record_and_activation_sources() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into projects(name, root_path, created_at, updated_at) values ('other', '/tmp/other-awb-fork-source', current_timestamp, current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        "insert into work_units(project_id, title, status, started_at) values (2, 'other work', 'open', current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        "insert into work_unit_activations(project_id, work_unit_id, status, activation_reason, opened_at) values (2, 1, 'suspended', 'start', current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        "insert into work_records(project_id, work_unit_id, topic, created_at) values (2, 1, 'other record', current_timestamp)",
        [],
    )
    .unwrap();
    drop(conn);

    let cross_record = fork_work(
        temp.path(),
        NewWorkFork {
            title: "bad record fork",
            source: WorkForkSource::Record(1),
            reason: "other",
            discard_policy: "keep_history",
        },
    );
    let cross_activation = fork_work(
        temp.path(),
        NewWorkFork {
            title: "bad activation fork",
            source: WorkForkSource::Activation(1),
            reason: "other",
            discard_policy: "keep_history",
        },
    );

    assert!(cross_record.is_err());
    assert!(cross_activation.is_err());
}

#[test]
fn repository_direct_links_block_repository_delete() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "repository direct links", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "direct repository link",
            priority: "medium",
            source: "user",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("repository link blocks delete"),
        },
    )
    .unwrap();
    let repo = add_repository(
        temp.path(),
        NewRepository {
            name: "main",
            path: ".",
            current_head: None,
            status_summary: None,
        },
    )
    .unwrap();
    let record = create_work_record(
        temp.path(),
        NewWorkRecord {
            work_unit_id: Some(work.work_unit_id),
            topic: "repository file link",
            work_performed: None,
            next_actions: None,
            notable_operations: None,
            export_path: None,
        },
    )
    .unwrap();
    add_work_record_git_file(
        temp.path(),
        NewWorkRecordGitFile {
            work_record_id: record.work_record_id,
            git_file_change_id: None,
            repository_id: Some(repo.repository_id),
            path: "src/lib.rs",
            role: "changed",
            note: None,
        },
    )
    .unwrap();
    add_implementation_evidence_with_git(
        temp.path(),
        NewImplementationEvidenceWithGit {
            task_id: Some(task.task_id),
            design_version_id: None,
            requirement_key: None,
            evidence_type: "file",
            repository_id: Some(repo.repository_id),
            git_commit_id: None,
            git_file_change_id: None,
            commit_sha: None,
            file_path: Some("src/lib.rs"),
            line_ref: None,
            symbol: None,
            artifact_path: None,
            note: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();

    let delete_repository = conn.execute(
        "delete from repositories where id = ?1",
        params![repo.repository_id],
    );

    assert!(delete_repository.is_err());
}

#[test]
fn command_usage_rejects_cross_project_references_without_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into projects(name, root_path, created_at, updated_at) values ('other', '/tmp/other-awb-command-usage', current_timestamp, current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        r#"
        insert into command_profiles(
            project_id, name, command, command_type, status, stability,
            source, created_at, updated_at
        )
        values (1, 'test-command', 'cargo test', 'test', 'fixed', 'stable', 'user', current_timestamp, current_timestamp)
        "#,
        [],
    )
    .unwrap();
    conn.execute(
        "insert into work_units(project_id, title, status, started_at) values (2, 'other work', 'open', current_timestamp)",
        [],
    )
    .unwrap();

    let cross_project_usage = conn.execute(
        r#"
        insert into command_usages(project_id, command_profile_id, work_unit_id, command, result, created_at)
        values (1, 1, 1, 'cargo test', 'pass', current_timestamp)
        "#,
        [],
    );

    assert!(cross_project_usage.is_err());
}

#[test]
fn work_record_forks_require_target_and_one_source() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "fork source rules", None).unwrap();
    suspend_work(temp.path(), "prepare fork", "redo").unwrap();
    let record = create_work_record(
        temp.path(),
        NewWorkRecord {
            work_unit_id: Some(work.work_unit_id),
            topic: "fork source record",
            work_performed: None,
            next_actions: None,
            notable_operations: None,
            export_path: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into work_units(project_id, title, status, started_at) values (1, 'fork target', 'open', current_timestamp)",
        [],
    )
    .unwrap();

    let no_target = conn.execute(
        r#"
        insert into work_record_forks(project_id, fork_reason, discard_policy, status, created_at)
        values (1, 'other', 'keep_history', 'open', current_timestamp)
        "#,
        [],
    );
    let no_source = conn.execute(
        r#"
        insert into work_record_forks(project_id, forked_work_unit_id, fork_reason, discard_policy, status, created_at)
        values (1, 2, 'other', 'keep_history', 'open', current_timestamp)
        "#,
        [],
    );
    let multiple_sources = conn.execute(
        r#"
        insert into work_record_forks(
            project_id, source_work_record_id, source_git_commit_sha,
            forked_work_unit_id, fork_reason, discard_policy, status, created_at
        )
        values (1, ?1, 'abc123', 2, 'other', 'keep_history', 'open', current_timestamp)
        "#,
        params![record.work_record_id],
    );

    assert!(no_target.is_err());
    assert!(no_source.is_err());
    assert!(multiple_sources.is_err());
}

#[test]
fn command_profile_repository_and_empty_work_links_are_db_enforced() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "empty link rules", None).unwrap();
    let record = create_work_record(
        temp.path(),
        NewWorkRecord {
            work_unit_id: Some(work.work_unit_id),
            topic: "empty link record",
            work_performed: None,
            next_actions: None,
            notable_operations: None,
            export_path: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into projects(name, root_path, created_at, updated_at) values ('other', '/tmp/other-awb-profile-repo', current_timestamp, current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        "insert into repositories(project_id, name, path, last_checked_at) values (2, 'other', '../other', current_timestamp)",
        [],
    )
    .unwrap();

    let cross_repository_profile = conn.execute(
        r#"
        insert into command_profiles(
            project_id, repository_id, name, command, command_type,
            status, stability, source, created_at, updated_at
        )
        values (1, 1, 'bad-profile', 'cargo test', 'test', 'fixed', 'stable', 'user', current_timestamp, current_timestamp)
        "#,
        [],
    );
    let empty_command_link = conn.execute(
        "insert into work_record_commands(work_record_id) values (?1)",
        params![record.work_record_id],
    );
    let empty_commit_link = conn.execute(
        "insert into work_record_commits(work_record_id, role) values (?1, 'referenced')",
        params![record.work_record_id],
    );

    assert!(cross_repository_profile.is_err());
    assert!(empty_command_link.is_err());
    assert!(empty_commit_link.is_err());
}

#[test]
fn ledger_rows_require_project_identity_even_without_work_unit_links() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let record = create_work_record(
        temp.path(),
        NewWorkRecord {
            work_unit_id: None,
            topic: "project scoped record",
            work_performed: None,
            next_actions: None,
            notable_operations: None,
            export_path: None,
        },
    )
    .unwrap();
    let usage = add_command_usage(
        temp.path(),
        NewCommandUsage {
            profile: None,
            command: Some("cargo test"),
            result: "pass",
            log_path: None,
            work_unit_id: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into projects(name, root_path, created_at, updated_at) values ('other', '/tmp/other-awb-projectless', current_timestamp, current_timestamp)",
        [],
    )
    .unwrap();

    let projectless_record = conn.execute(
        "insert into work_records(topic, created_at) values ('missing project', current_timestamp)",
        [],
    );
    let projectless_usage = conn.execute(
        "insert into command_usages(command, result, created_at) values ('cargo test', 'pass', current_timestamp)",
        [],
    );
    conn.execute(
        "insert into command_usages(project_id, command, result, created_at) values (2, 'cargo test', 'pass', current_timestamp)",
        [],
    )
    .unwrap();
    let other_project_usage = conn.last_insert_rowid();
    let cross_usage_link = conn.execute(
        "insert into work_record_commands(work_record_id, command_usage_id) values (?1, ?2)",
        params![record.work_record_id, other_project_usage],
    );

    assert!(projectless_record.is_err());
    assert!(projectless_usage.is_err());
    assert!(cross_usage_link.is_err());

    let visible = list_command_usages(
        temp.path(),
        CommandUsageListQuery {
            profile: None,
            work_unit_id: None,
        },
    )
    .unwrap();
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id, usage.command_usage_id);
}

#[test]
fn project_scoped_record_without_work_unit_cannot_fork_another_project() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into projects(name, root_path, created_at, updated_at) values ('other', '/tmp/other-awb-record-fork', current_timestamp, current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        "insert into work_records(project_id, topic, created_at) values (2, 'other record', current_timestamp)",
        [],
    )
    .unwrap();
    drop(conn);

    let cross_record = fork_work(
        temp.path(),
        NewWorkFork {
            title: "bad record fork",
            source: WorkForkSource::Record(1),
            reason: "redo from unrelated project",
            discard_policy: "keep_history",
        },
    );

    assert!(cross_record.is_err());
}

#[test]
fn work_record_operations_reject_records_from_other_projects() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into projects(name, root_path, created_at, updated_at) values ('other', '/tmp/other-awb-record-api', current_timestamp, current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        "insert into work_records(project_id, topic, created_at) values (2, 'other record', current_timestamp)",
        [],
    )
    .unwrap();
    let other_record_id = conn.last_insert_rowid();
    drop(conn);

    let command_link = add_work_record_command(
        temp.path(),
        NewWorkRecordCommand {
            work_record_id: other_record_id,
            command_usage_id: None,
            command_profile_id: None,
            command: Some("cargo test"),
            result: Some("pass"),
            log_path: None,
            note: None,
        },
    );
    let commit_link = add_work_record_commit(
        temp.path(),
        NewWorkRecordCommit {
            work_record_id: other_record_id,
            commit_sha: "abc123",
            role: "referenced",
            note: None,
        },
    );
    let file_link = add_work_record_file(
        temp.path(),
        NewWorkRecordFile {
            work_record_id: other_record_id,
            path: "src/lib.rs",
            role: "changed",
            note: None,
        },
    );
    let exported = export_work_record_markdown(temp.path(), other_record_id);

    assert!(command_link.is_err());
    assert!(commit_link.is_err());
    assert!(file_link.is_err());
    assert!(exported.is_err());
}

#[test]
fn implementation_evidence_rejects_tasks_without_work_units() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into tasks(title, priority, source) values ('detached task', 'medium', 'user')",
        [],
    )
    .unwrap();
    drop(conn);

    let api_result = add_implementation_evidence(
        temp.path(),
        NewImplementationEvidence {
            task_id: Some(1),
            design_version_id: None,
            requirement_key: None,
            evidence_type: "manual_note",
            commit_sha: None,
            file_path: None,
            line_ref: None,
            symbol: None,
            artifact_path: None,
            note: Some("detached tasks cannot own implementation evidence"),
        },
    );
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let direct_result = conn.execute(
        r#"
        insert into implementation_evidence(project_id, task_id, evidence_type, note, created_at)
        values (1, 1, 'manual_note', 'detached task evidence', current_timestamp)
        "#,
        [],
    );

    assert!(api_result.is_err());
    assert!(direct_result.is_err());
}
