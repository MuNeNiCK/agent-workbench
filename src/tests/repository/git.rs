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
