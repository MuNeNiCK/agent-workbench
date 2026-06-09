use super::*;

#[test]
fn repository_records_snapshots_dirty_entries_and_git_evidence() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();

    let repo = add_repository(
        temp.path(),
        NewRepository {
            name: "main",
            path: ".",
            current_head: Some("abc123"),
            status_summary: Some("dirty"),
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
            content_hash: Some("hash-a"),
        },
    )
    .unwrap();
    let commit = add_git_commit(
        temp.path(),
        NewGitCommit {
            repository: "main",
            commit_sha: "abc123",
            short_sha: Some("abc123"),
            subject: Some("initial"),
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
            additions: Some(2),
            deletions: Some(1),
            content_hash: Some("hash-b"),
        },
    )
    .unwrap();
    let work_record = create_work_record(
        temp.path(),
        NewWorkRecord {
            work_unit_id: None,
            topic: "repository evidence",
            work_performed: None,
            next_actions: None,
            notable_operations: None,
            export_path: None,
        },
    )
    .unwrap();
    add_work_record_git_commit(
        temp.path(),
        NewWorkRecordGitCommit {
            work_record_id: work_record.work_record_id,
            git_commit_id: Some(commit.git_commit_id),
            commit_sha: "abc123",
            role: "created",
            note: None,
        },
    )
    .unwrap();
    add_work_record_git_file(
        temp.path(),
        NewWorkRecordGitFile {
            work_record_id: work_record.work_record_id,
            git_file_change_id: Some(file.git_file_change_id),
            repository_id: None,
            path: "src/lib.rs",
            role: "changed",
            note: None,
        },
    )
    .unwrap();

    let repos = list_repositories(temp.path()).unwrap();
    let snapshots = list_repository_snapshots(temp.path(), Some("main")).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let linked_commit_id: i64 = conn
        .query_row(
            "select git_commit_id from work_record_commits where work_record_id = ?1",
            params![work_record.work_record_id],
            |row| row.get(0),
        )
        .unwrap();
    let linked_file: (i64, i64) = conn
        .query_row(
            "select git_file_change_id, repository_id from work_record_files where work_record_id = ?1",
            params![work_record.work_record_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(repo.repository_id, 1);
    assert_eq!(snapshot.repository_id, repo.repository_id);
    assert_eq!(dirty.repository_dirty_entry_id, 1);
    assert_eq!(commit.repository_id, repo.repository_id);
    assert_eq!(file.repository_id, repo.repository_id);
    assert_eq!(linked_commit_id, commit.git_commit_id);
    assert_eq!(linked_file, (file.git_file_change_id, repo.repository_id));
    assert_eq!(repos[0].name, "main");
    assert_eq!(repos[0].current_head.as_deref(), Some("abc123"));
    assert_eq!(snapshots[0].repository_name, "main");
    assert!(!snapshots[0].is_clean);
}

#[test]
fn git_import_keeps_explicit_work_record_links_when_later_ambiguous() {
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
            name: "nested",
            path: "vendor/lib",
            current_head: None,
            status_summary: None,
        },
    )
    .unwrap();
    let main_commit = add_git_commit(
        temp.path(),
        NewGitCommit {
            repository: "main",
            commit_sha: "abc123",
            short_sha: Some("abc123"),
            subject: Some("main"),
            author_name: None,
            author_email: None,
            committed_at: None,
            parent_shas: None,
        },
    )
    .unwrap();
    let main_file = add_git_file_change(
        temp.path(),
        NewGitFileChange {
            git_commit_id: main_commit.git_commit_id,
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
    let work_record = create_work_record(
        temp.path(),
        NewWorkRecord {
            work_unit_id: None,
            topic: "explicit evidence remains stable",
            work_performed: None,
            next_actions: None,
            notable_operations: None,
            export_path: None,
        },
    )
    .unwrap();
    add_work_record_git_commit(
        temp.path(),
        NewWorkRecordGitCommit {
            work_record_id: work_record.work_record_id,
            git_commit_id: Some(main_commit.git_commit_id),
            commit_sha: "abc123",
            role: "created",
            note: None,
        },
    )
    .unwrap();
    add_work_record_git_file(
        temp.path(),
        NewWorkRecordGitFile {
            work_record_id: work_record.work_record_id,
            git_file_change_id: Some(main_file.git_file_change_id),
            repository_id: None,
            path: "src/lib.rs",
            role: "changed",
            note: None,
        },
    )
    .unwrap();
    let nested_commit = add_git_commit(
        temp.path(),
        NewGitCommit {
            repository: "nested",
            commit_sha: "abc123",
            short_sha: Some("abc123"),
            subject: Some("nested"),
            author_name: None,
            author_email: None,
            committed_at: None,
            parent_shas: None,
        },
    )
    .unwrap();
    add_git_file_change(
        temp.path(),
        NewGitFileChange {
            git_commit_id: nested_commit.git_commit_id,
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

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let linked_commit: (i64, i64) = conn
        .query_row(
            "select git_commit_id, auto_linked from work_record_commits where work_record_id = ?1",
            params![work_record.work_record_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let linked_file: (i64, i64, i64) = conn
        .query_row(
            "select git_file_change_id, repository_id, auto_linked from work_record_files where work_record_id = ?1",
            params![work_record.work_record_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();

    assert_eq!(linked_commit, (main_commit.git_commit_id, 0));
    assert_eq!(
        linked_file,
        (main_file.git_file_change_id, main_file.repository_id, 0)
    );
}

#[test]
fn git_import_keeps_explicit_repository_file_scope_when_later_ambiguous() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let main_repo = add_repository(
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
            name: "nested",
            path: "vendor/lib",
            current_head: None,
            status_summary: None,
        },
    )
    .unwrap();
    let work_record = create_work_record(
        temp.path(),
        NewWorkRecord {
            work_unit_id: None,
            topic: "repository scoped evidence remains stable",
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
            work_record_id: work_record.work_record_id,
            git_file_change_id: None,
            repository_id: Some(main_repo.repository_id),
            path: "src/lib.rs",
            role: "changed",
            note: None,
        },
    )
    .unwrap();
    let main_commit = add_git_commit(
        temp.path(),
        NewGitCommit {
            repository: "main",
            commit_sha: "abc123",
            short_sha: Some("abc123"),
            subject: Some("main"),
            author_name: None,
            author_email: None,
            committed_at: None,
            parent_shas: None,
        },
    )
    .unwrap();
    add_git_file_change(
        temp.path(),
        NewGitFileChange {
            git_commit_id: main_commit.git_commit_id,
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
    let nested_commit = add_git_commit(
        temp.path(),
        NewGitCommit {
            repository: "nested",
            commit_sha: "def456",
            short_sha: Some("def456"),
            subject: Some("nested"),
            author_name: None,
            author_email: None,
            committed_at: None,
            parent_shas: None,
        },
    )
    .unwrap();
    add_git_file_change(
        temp.path(),
        NewGitFileChange {
            git_commit_id: nested_commit.git_commit_id,
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

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let linked: (Option<i64>, Option<i64>, i64, i64) = conn
        .query_row(
            "select git_file_change_id, repository_id, auto_linked, repository_auto_linked from work_record_files where work_record_id = ?1",
            params![work_record.work_record_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();

    assert_eq!(linked, (None, Some(main_repo.repository_id), 0, 0));
}

#[test]
fn git_import_backfills_manual_work_record_links() {
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
    let work_record = create_work_record(
        temp.path(),
        NewWorkRecord {
            work_unit_id: None,
            topic: "manual evidence before git import",
            work_performed: None,
            next_actions: None,
            notable_operations: None,
            export_path: None,
        },
    )
    .unwrap();
    add_work_record_commit(
        temp.path(),
        NewWorkRecordCommit {
            work_record_id: work_record.work_record_id,
            commit_sha: "abc123",
            role: "created",
            note: None,
        },
    )
    .unwrap();
    add_work_record_file(
        temp.path(),
        NewWorkRecordFile {
            work_record_id: work_record.work_record_id,
            path: "src/lib.rs",
            role: "changed",
            note: None,
        },
    )
    .unwrap();

    let commit = add_git_commit(
        temp.path(),
        NewGitCommit {
            repository: "main",
            commit_sha: "abc123",
            short_sha: Some("abc123"),
            subject: Some("backfill"),
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

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let linked_commit_id: i64 = conn
        .query_row(
            "select git_commit_id from work_record_commits where work_record_id = ?1",
            params![work_record.work_record_id],
            |row| row.get(0),
        )
        .unwrap();
    let linked_file: (i64, i64) = conn
        .query_row(
            "select git_file_change_id, repository_id from work_record_files where work_record_id = ?1",
            params![work_record.work_record_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(linked_commit_id, commit.git_commit_id);
    assert_eq!(linked_file, (file.git_file_change_id, file.repository_id));
}

#[test]
fn git_import_does_not_backfill_ambiguous_manual_commit_links() {
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
            name: "nested",
            path: "vendor/lib",
            current_head: None,
            status_summary: None,
        },
    )
    .unwrap();
    add_git_commit(
        temp.path(),
        NewGitCommit {
            repository: "nested",
            commit_sha: "abc123",
            short_sha: Some("abc123"),
            subject: Some("nested"),
            author_name: None,
            author_email: None,
            committed_at: None,
            parent_shas: None,
        },
    )
    .unwrap();
    let work_record = create_work_record(
        temp.path(),
        NewWorkRecord {
            work_unit_id: None,
            topic: "ambiguous manual commit evidence",
            work_performed: None,
            next_actions: None,
            notable_operations: None,
            export_path: None,
        },
    )
    .unwrap();
    add_work_record_commit(
        temp.path(),
        NewWorkRecordCommit {
            work_record_id: work_record.work_record_id,
            commit_sha: "abc123",
            role: "created",
            note: None,
        },
    )
    .unwrap();
    add_git_commit(
        temp.path(),
        NewGitCommit {
            repository: "main",
            commit_sha: "abc123",
            short_sha: Some("abc123"),
            subject: Some("main"),
            author_name: None,
            author_email: None,
            committed_at: None,
            parent_shas: None,
        },
    )
    .unwrap();

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let linked_commit_id: Option<i64> = conn
        .query_row(
            "select git_commit_id from work_record_commits where work_record_id = ?1",
            params![work_record.work_record_id],
            |row| row.get(0),
        )
        .unwrap();

    assert!(linked_commit_id.is_none());
}

#[test]
fn git_import_clears_auto_backfilled_commit_links_when_later_ambiguous() {
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
            name: "nested",
            path: "vendor/lib",
            current_head: None,
            status_summary: None,
        },
    )
    .unwrap();
    let work_record = create_work_record(
        temp.path(),
        NewWorkRecord {
            work_unit_id: None,
            topic: "commit evidence becomes ambiguous",
            work_performed: None,
            next_actions: None,
            notable_operations: None,
            export_path: None,
        },
    )
    .unwrap();
    add_work_record_commit(
        temp.path(),
        NewWorkRecordCommit {
            work_record_id: work_record.work_record_id,
            commit_sha: "abc123",
            role: "created",
            note: None,
        },
    )
    .unwrap();
    add_git_commit(
        temp.path(),
        NewGitCommit {
            repository: "main",
            commit_sha: "abc123",
            short_sha: Some("abc123"),
            subject: Some("main"),
            author_name: None,
            author_email: None,
            committed_at: None,
            parent_shas: None,
        },
    )
    .unwrap();
    add_git_commit(
        temp.path(),
        NewGitCommit {
            repository: "nested",
            commit_sha: "abc123",
            short_sha: Some("abc123"),
            subject: Some("nested"),
            author_name: None,
            author_email: None,
            committed_at: None,
            parent_shas: None,
        },
    )
    .unwrap();

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let linked: (Option<i64>, i64) = conn
        .query_row(
            "select git_commit_id, auto_linked from work_record_commits where work_record_id = ?1",
            params![work_record.work_record_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(linked, (None, 0));
}

#[test]
fn git_import_does_not_backfill_ambiguous_manual_file_links() {
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
            name: "nested",
            path: "vendor/lib",
            current_head: None,
            status_summary: None,
        },
    )
    .unwrap();
    let nested_commit = add_git_commit(
        temp.path(),
        NewGitCommit {
            repository: "nested",
            commit_sha: "def456",
            short_sha: Some("def456"),
            subject: Some("nested"),
            author_name: None,
            author_email: None,
            committed_at: None,
            parent_shas: None,
        },
    )
    .unwrap();
    add_git_file_change(
        temp.path(),
        NewGitFileChange {
            git_commit_id: nested_commit.git_commit_id,
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
    let work_record = create_work_record(
        temp.path(),
        NewWorkRecord {
            work_unit_id: None,
            topic: "ambiguous manual file evidence",
            work_performed: None,
            next_actions: None,
            notable_operations: None,
            export_path: None,
        },
    )
    .unwrap();
    add_work_record_file(
        temp.path(),
        NewWorkRecordFile {
            work_record_id: work_record.work_record_id,
            path: "src/lib.rs",
            role: "changed",
            note: None,
        },
    )
    .unwrap();

    let main_commit = add_git_commit(
        temp.path(),
        NewGitCommit {
            repository: "main",
            commit_sha: "abc123",
            short_sha: Some("abc123"),
            subject: Some("main"),
            author_name: None,
            author_email: None,
            committed_at: None,
            parent_shas: None,
        },
    )
    .unwrap();
    add_git_file_change(
        temp.path(),
        NewGitFileChange {
            git_commit_id: main_commit.git_commit_id,
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

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let linked_file_id: Option<i64> = conn
        .query_row(
            "select git_file_change_id from work_record_files where work_record_id = ?1",
            params![work_record.work_record_id],
            |row| row.get(0),
        )
        .unwrap();

    assert!(linked_file_id.is_none());
}

#[test]
fn git_import_clears_auto_backfilled_file_links_when_later_ambiguous() {
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
            name: "nested",
            path: "vendor/lib",
            current_head: None,
            status_summary: None,
        },
    )
    .unwrap();
    let work_record = create_work_record(
        temp.path(),
        NewWorkRecord {
            work_unit_id: None,
            topic: "file evidence becomes ambiguous",
            work_performed: None,
            next_actions: None,
            notable_operations: None,
            export_path: None,
        },
    )
    .unwrap();
    add_work_record_file(
        temp.path(),
        NewWorkRecordFile {
            work_record_id: work_record.work_record_id,
            path: "src/lib.rs",
            role: "changed",
            note: None,
        },
    )
    .unwrap();
    let main_commit = add_git_commit(
        temp.path(),
        NewGitCommit {
            repository: "main",
            commit_sha: "abc123",
            short_sha: Some("abc123"),
            subject: Some("main"),
            author_name: None,
            author_email: None,
            committed_at: None,
            parent_shas: None,
        },
    )
    .unwrap();
    add_git_file_change(
        temp.path(),
        NewGitFileChange {
            git_commit_id: main_commit.git_commit_id,
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
    let nested_commit = add_git_commit(
        temp.path(),
        NewGitCommit {
            repository: "nested",
            commit_sha: "def456",
            short_sha: Some("def456"),
            subject: Some("nested"),
            author_name: None,
            author_email: None,
            committed_at: None,
            parent_shas: None,
        },
    )
    .unwrap();
    add_git_file_change(
        temp.path(),
        NewGitFileChange {
            git_commit_id: nested_commit.git_commit_id,
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

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let linked: (Option<i64>, Option<i64>, i64) = conn
        .query_row(
            "select git_file_change_id, repository_id, auto_linked from work_record_files where work_record_id = ?1",
            params![work_record.work_record_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();

    assert_eq!(linked, (None, None, 0));
}

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

    let result = init_project(temp.path());

    assert!(result.is_err());
    let error = result.unwrap_err().to_string();
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

    let result = init_project(temp.path());

    assert!(result.is_err());
    let error = result.unwrap_err().to_string();
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

    let result = init_project(temp.path());

    assert!(result.is_err());
    let error = result.unwrap_err().to_string();
    assert!(
        error.contains("work_record_commits contains invalid git links"),
        "{error}"
    );
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

    init_project(temp.path()).unwrap();

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

    init_project(temp.path()).unwrap();

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
