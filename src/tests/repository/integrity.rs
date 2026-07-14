use super::*;

#[test]
fn repository_snapshot_comparisons_require_one_repository() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    add_repository(
        temp.path(),
        NewRepository {
            name: "main",
            path: ".",
            current_head: None,
            status_summary: None,
        },
    )
    .unwrap();
    add_repository(
        temp.path(),
        NewRepository {
            name: "submodule",
            path: "vendor/submodule",
            current_head: None,
            status_summary: None,
        },
    )
    .unwrap();
    let first = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: None,
            head_sha: Some("a"),
            branch: None,
            status_summary: None,
            is_clean: true,
        },
    )
    .unwrap();
    let second = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: None,
            head_sha: Some("b"),
            branch: None,
            status_summary: None,
            is_clean: true,
        },
    )
    .unwrap();
    let other = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "submodule",
            work_unit_activation_id: None,
            head_sha: Some("c"),
            branch: None,
            status_summary: None,
            is_clean: true,
        },
    )
    .unwrap();

    let valid = add_repository_snapshot_comparison(
        temp.path(),
        NewRepositorySnapshotComparison {
            base_repository_snapshot_id: first.repository_snapshot_id,
            current_repository_snapshot_id: second.repository_snapshot_id,
            comparison_type: "resume",
            head_changed: true,
            dirty_state_changed: false,
            nested_repository_changed: false,
            result: "changed_classified",
        },
    );
    let invalid = add_repository_snapshot_comparison(
        temp.path(),
        NewRepositorySnapshotComparison {
            base_repository_snapshot_id: first.repository_snapshot_id,
            current_repository_snapshot_id: other.repository_snapshot_id,
            comparison_type: "resume",
            head_changed: true,
            dirty_state_changed: false,
            nested_repository_changed: false,
            result: "changed_unclassified",
        },
    );

    assert!(valid.is_ok());
    assert!(invalid.is_err());
}

#[test]
fn repository_targets_can_attach_to_review_plan() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "implement repo ledger", None).unwrap();
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
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    add_repository(
        temp.path(),
        NewRepository {
            name: "main",
            path: ".",
            current_head: None,
            status_summary: None,
        },
    )
    .unwrap();
    let snapshot = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(work.activation_id),
            head_sha: Some("abc"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();

    let same_project = conn.execute(
        r#"
        insert into review_plan_targets(review_plan_id, target_type, repository_snapshot_id)
        values (?1, 'repository_snapshot', ?2)
        "#,
        params![plan.review_plan_id, snapshot.repository_snapshot_id],
    );

    assert!(same_project.is_ok());
}

#[test]
fn repository_integrity_rejects_cross_project_links() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    add_repository(
        temp.path(),
        NewRepository {
            name: "main",
            path: ".",
            current_head: None,
            status_summary: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into projects(name, root_path, created_at, updated_at) values ('other', '/tmp/other-awb-repo', current_timestamp, current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        "insert into work_units(project_id, title, status, started_at) values (2, 'other', 'open', current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        "insert into work_unit_activations(project_id, work_unit_id, status, activation_reason, opened_at) values (2, 1, 'suspended', 'start', current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        "insert into repositories(project_id, name, path, current_head, status_summary, last_checked_at) values (2, 'other', '../other', null, null, current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        "insert into git_commits(repository_id, commit_sha, created_at) values (1, 'abc', current_timestamp)",
        [],
    )
    .unwrap();

    let cross_activation = conn.execute(
        r#"
        insert into repository_snapshots(
            repository_id, work_unit_activation_id, head_sha, is_clean, created_at
        )
        values (1, 1, 'abc', 1, current_timestamp)
        "#,
        [],
    );
    let cross_file_change = conn.execute(
        r#"
        insert into git_file_changes(
            git_commit_id, repository_id, path, change_type
        )
        values (1, 2, 'src/lib.rs', 'modified')
        "#,
        [],
    );

    assert!(cross_activation.is_err());
    assert!(cross_file_change.is_err());
}

#[test]
fn repository_work_record_git_links_are_db_enforced() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    add_repository(
        temp.path(),
        NewRepository {
            name: "main",
            path: ".",
            current_head: None,
            status_summary: None,
        },
    )
    .unwrap();
    let commit = add_git_commit(
        temp.path(),
        NewGitCommit {
            repository: "main",
            commit_sha: "abc123",
            short_sha: None,
            subject: None,
            author_name: None,
            author_email: None,
            committed_at: None,
            parent_shas: None,
        },
    )
    .unwrap();
    let file = add_git_file_change(
        temp.path(),
        NewGitFileChange {
            git_commit_id: commit.git_commit_id,
            repository: None,
            path: "src/lib.rs",
            old_path: None,
            change_type: "modified",
            additions: None,
            deletions: None,
            content_hash: None,
        },
    )
    .unwrap();
    let record = create_work_record(
        temp.path(),
        NewWorkRecord {
            work_unit_id: None,
            topic: "db enforced git links",
            work_performed: None,
            next_actions: None,
            notable_operations: None,
            export_path: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();

    let dangling_commit = conn.execute(
        r#"
        insert into work_record_commits(work_record_id, git_commit_id, commit_sha, role)
        values (?1, 999, 'abc123', 'created')
        "#,
        params![record.work_record_id],
    );
    let mismatched_sha = conn.execute(
        r#"
        insert into work_record_commits(work_record_id, git_commit_id, commit_sha, role)
        values (?1, ?2, 'different', 'created')
        "#,
        params![record.work_record_id, commit.git_commit_id],
    );
    let missing_file_repository = conn.execute(
        r#"
        insert into work_record_files(work_record_id, git_file_change_id, path, role)
        values (?1, ?2, 'src/lib.rs', 'changed')
        "#,
        params![record.work_record_id, file.git_file_change_id],
    );
    let mismatched_file_path = conn.execute(
        r#"
        insert into work_record_files(work_record_id, git_file_change_id, repository_id, path, role)
        values (?1, ?2, 1, 'src/main.rs', 'changed')
        "#,
        params![record.work_record_id, file.git_file_change_id],
    );

    assert!(dangling_commit.is_err());
    assert!(mismatched_sha.is_err());
    assert!(missing_file_repository.is_err());
    assert!(mismatched_file_path.is_err());
}

#[test]
fn repository_snapshot_references_are_db_enforced() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "snapshot references", None).unwrap();
    let suspended = suspend_work(temp.path(), "capture state", "resume").unwrap();
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            review_type: "implementation_review",
            required: true,
            stage: "resume-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    add_repository(
        temp.path(),
        NewRepository {
            name: "main",
            path: ".",
            current_head: None,
            status_summary: None,
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
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        r#"
        insert into review_plan_targets(review_plan_id, target_type, repository_snapshot_id)
        values (?1, 'repository_snapshot', ?2)
        "#,
        params![plan.review_plan_id, snapshot.repository_snapshot_id],
    )
    .unwrap();
    conn.execute(
        r#"
        insert into resume_checks(
            work_unit_id, work_unit_activation_id, suspend_snapshot_id, maturity,
            result, repository_snapshot_id, created_at
        )
        values (?1, ?2, ?3, 'repo-aware', 'allowed', ?4, current_timestamp)
        "#,
        params![
            suspended.work_unit_id,
            suspended.activation_id,
            suspended.suspend_snapshot_id,
            snapshot.repository_snapshot_id
        ],
    )
    .unwrap();

    let delete_snapshot = conn.execute(
        "delete from repository_snapshots where id = ?1",
        params![snapshot.repository_snapshot_id],
    );

    assert!(delete_snapshot.is_err());
}

#[test]
fn repository_classification_acceptance_project_is_db_enforced() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    add_repository(
        temp.path(),
        NewRepository {
            name: "main",
            path: ".",
            current_head: None,
            status_summary: None,
        },
    )
    .unwrap();
    let snapshot = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: None,
            head_sha: Some("abc123"),
            branch: None,
            status_summary: None,
            is_clean: false,
        },
    )
    .unwrap();
    let dirty = add_repository_dirty_entry(
        temp.path(),
        NewRepositoryDirtyEntry {
            repository_snapshot_id: snapshot.repository_snapshot_id,
            path: "src/lib.rs",
            change_type: "modified",
            staged: false,
            content_hash: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into projects(name, root_path, created_at, updated_at) values ('other', '/tmp/other-awb-classification', current_timestamp, current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        r#"
        insert into acceptance_records(
            project_id, target_type, design_package_key, design_file_path,
            acceptance_type, reason, created_by, status, created_at
        )
        values (
            2, 'design_file', 'other', '01-other.md',
            'explicit_exception', 'other project exception', 'user',
            'approved', current_timestamp
        )
        "#,
        [],
    )
    .unwrap();

    let cross_project_acceptance = conn.execute(
        r#"
        insert into repository_state_classifications(
            repository_snapshot_id, dirty_entry_id, classification,
            reason, acceptance_record_id, created_at
        )
        values (?1, ?2, 'accepted_exception', 'accepted elsewhere', 1, current_timestamp)
        "#,
        params![
            snapshot.repository_snapshot_id,
            dirty.repository_dirty_entry_id
        ],
    );

    assert!(cross_project_acceptance.is_err());
}

#[test]
fn repository_state_classification_records_snapshot_and_dirty_entry() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    add_repository(
        temp.path(),
        NewRepository {
            name: "main",
            path: ".",
            current_head: None,
            status_summary: None,
        },
    )
    .unwrap();
    let snapshot = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: None,
            head_sha: Some("abc123"),
            branch: Some("master"),
            status_summary: Some("M src/lib.rs"),
            is_clean: false,
        },
    )
    .unwrap();
    let dirty = add_repository_dirty_entry(
        temp.path(),
        NewRepositoryDirtyEntry {
            repository_snapshot_id: snapshot.repository_snapshot_id,
            path: "src/lib.rs",
            change_type: "modified",
            staged: false,
            content_hash: None,
        },
    )
    .unwrap();

    let classification = add_repository_state_classification(
        temp.path(),
        NewRepositoryStateClassification {
            repository_snapshot_id: snapshot.repository_snapshot_id,
            dirty_entry_id: Some(dirty.repository_dirty_entry_id),
            classification: "expected",
            reason: "implementation edit",
            acceptance_record_id: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let stored: (i64, i64, String, String) = conn
        .query_row(
            r#"
            select repository_snapshot_id, dirty_entry_id, classification, reason
            from repository_state_classifications
            where id = ?1
            "#,
            params![classification.repository_state_classification_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();

    assert_eq!(
        stored,
        (
            snapshot.repository_snapshot_id,
            dirty.repository_dirty_entry_id,
            "expected".to_string(),
            "implementation edit".to_string()
        )
    );
}

#[test]
fn repository_classification_acceptance_type_is_db_enforced() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    add_repository(
        temp.path(),
        NewRepository {
            name: "main",
            path: ".",
            current_head: None,
            status_summary: None,
        },
    )
    .unwrap();
    let snapshot = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: None,
            head_sha: Some("abc123"),
            branch: None,
            status_summary: None,
            is_clean: false,
        },
    )
    .unwrap();
    let dirty = add_repository_dirty_entry(
        temp.path(),
        NewRepositoryDirtyEntry {
            repository_snapshot_id: snapshot.repository_snapshot_id,
            path: "src/lib.rs",
            change_type: "modified",
            staged: false,
            content_hash: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        r#"
        insert into acceptance_records(
            project_id, target_type, design_package_key, design_file_path,
            acceptance_type, reason, created_by, status, created_at
        )
        values (
            1, 'design_file', 'main', '01-main.md',
            'explicit_exception', 'generated file', 'user',
            'approved', current_timestamp
        )
        "#,
        [],
    )
    .unwrap();

    let missing_acceptance = conn.execute(
        r#"
        insert into repository_state_classifications(
            repository_snapshot_id, dirty_entry_id, classification, reason, created_at
        )
        values (?1, ?2, 'accepted_exception', 'missing acceptance', current_timestamp)
        "#,
        params![
            snapshot.repository_snapshot_id,
            dirty.repository_dirty_entry_id
        ],
    );
    let unexpected_acceptance = conn.execute(
        r#"
        insert into repository_state_classifications(
            repository_snapshot_id, dirty_entry_id, classification,
            reason, acceptance_record_id, created_at
        )
        values (?1, ?2, 'expected', 'not an exception', 1, current_timestamp)
        "#,
        params![
            snapshot.repository_snapshot_id,
            dirty.repository_dirty_entry_id
        ],
    );

    assert!(missing_acceptance.is_err());
    assert!(unexpected_acceptance.is_err());
}

#[test]
fn repository_links_cover_commands_evidence_and_forks() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "git linked work", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "wire git evidence",
            priority: "medium",
            source: "user",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("evidence links to git ids"),
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
    let commit = add_git_commit(
        temp.path(),
        NewGitCommit {
            repository: "main",
            commit_sha: "abc123",
            short_sha: Some("abc123"),
            subject: Some("wire git evidence"),
            author_name: None,
            author_email: None,
            committed_at: None,
            parent_shas: None,
        },
    )
    .unwrap();
    let file = add_git_file_change(
        temp.path(),
        NewGitFileChange {
            git_commit_id: commit.git_commit_id,
            repository: None,
            path: "src/lib.rs",
            old_path: None,
            change_type: "modified",
            additions: Some(1),
            deletions: Some(0),
            content_hash: None,
        },
    )
    .unwrap();
    let usage = add_command_usage_with_repository_snapshot(
        temp.path(),
        NewCommandUsageWithRepositorySnapshot {
            profile: None,
            command: Some("cargo test"),
            result: "pass",
            log_path: None,
            work_unit_id: Some(work.work_unit_id),
            repository_snapshot_id: Some(snapshot.repository_snapshot_id),
        },
    )
    .unwrap();
    let evidence = add_implementation_evidence_with_git(
        temp.path(),
        NewImplementationEvidenceWithGit {
            task_id: Some(task.task_id),
            design_version_id: None,
            requirement_key: None,
            evidence_type: "file",
            repository_id: None,
            git_commit_id: Some(commit.git_commit_id),
            git_file_change_id: Some(file.git_file_change_id),
            commit_sha: None,
            file_path: None,
            line_ref: None,
            symbol: None,
            artifact_path: None,
            note: None,
        },
    )
    .unwrap();
    suspend_work(temp.path(), "fork from current state", "redo").unwrap();
    let fork = fork_work(
        temp.path(),
        NewWorkFork {
            title: "redo from snapshot",
            source: WorkForkSource::RepositorySnapshot(snapshot.repository_snapshot_id),
            reason: "user_requested_redo",
            discard_policy: "keep_history",
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let stored_usage_snapshot: i64 = conn
        .query_row(
            "select repository_snapshot_id from command_usages where id = ?1",
            params![usage.command_usage_id],
            |row| row.get(0),
        )
        .unwrap();
    let stored_evidence: (i64, i64, i64, String, String) = conn
        .query_row(
            r#"
            select repository_id, git_commit_id, git_file_change_id, commit_sha, file_path
            from implementation_evidence
            where id = ?1
            "#,
            params![evidence.implementation_evidence_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    let stored_fork_snapshot: i64 = conn
        .query_row(
            "select source_repository_snapshot_id from work_record_forks where id = ?1",
            params![fork.fork_id],
            |row| row.get(0),
        )
        .unwrap();

    assert_eq!(stored_usage_snapshot, snapshot.repository_snapshot_id);
    assert_eq!(
        stored_evidence,
        (
            commit.repository_id,
            commit.git_commit_id,
            file.git_file_change_id,
            "abc123".to_string(),
            "src/lib.rs".to_string()
        )
    );
    assert_eq!(stored_fork_snapshot, snapshot.repository_snapshot_id);
}

#[test]
fn repository_links_reject_invalid_direct_sql_for_commands_evidence_and_forks() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "direct sql protection", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "protect git links",
            priority: "medium",
            source: "user",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("invalid direct SQL is rejected"),
        },
    )
    .unwrap();
    add_repository(
        temp.path(),
        NewRepository {
            name: "main",
            path: ".",
            current_head: None,
            status_summary: None,
        },
    )
    .unwrap();
    let commit = add_git_commit(
        temp.path(),
        NewGitCommit {
            repository: "main",
            commit_sha: "abc123",
            short_sha: None,
            subject: None,
            author_name: None,
            author_email: None,
            committed_at: None,
            parent_shas: None,
        },
    )
    .unwrap();
    let file = add_git_file_change(
        temp.path(),
        NewGitFileChange {
            git_commit_id: commit.git_commit_id,
            repository: None,
            path: "src/lib.rs",
            old_path: None,
            change_type: "modified",
            additions: None,
            deletions: None,
            content_hash: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();

    let dangling_usage_snapshot = conn.execute(
        r#"
        insert into command_usages(project_id, command, result, repository_snapshot_id, created_at)
        values (1, 'cargo test', 'pass', 999, current_timestamp)
        "#,
        [],
    );
    let mismatched_evidence_file = conn.execute(
        r#"
        insert into implementation_evidence(
            project_id, task_id, evidence_type, repository_id,
            git_commit_id, git_file_change_id, commit_sha, file_path, created_at
        )
        values (1, ?1, 'file', 1, ?2, ?3, 'abc123', 'src/main.rs', current_timestamp)
        "#,
        params![task.task_id, commit.git_commit_id, file.git_file_change_id],
    );
    let dangling_fork_commit = conn.execute(
        r#"
        insert into work_record_forks(
            project_id, source_git_commit_id, source_git_commit_sha,
            fork_reason, discard_policy, status, created_at
        )
        values (1, 999, 'missing', 'other', 'keep_history', 'open', current_timestamp)
        "#,
        [],
    );

    assert!(dangling_usage_snapshot.is_err());
    assert!(mismatched_evidence_file.is_err());
    assert!(dangling_fork_commit.is_err());
}
