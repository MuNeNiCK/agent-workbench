use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use crate::db::{open_existing_project, project_id};

pub fn add_repository(root: &Path, input: NewRepository<'_>) -> Result<RepositoryOutcome> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    conn.execute(
        r#"
        insert into repositories(
            project_id, name, path, current_head, status_summary, last_checked_at
        )
        values (?1, ?2, ?3, ?4, ?5, current_timestamp)
        "#,
        params![
            project_id,
            input.name,
            input.path,
            input.current_head,
            input.status_summary,
        ],
    )?;

    Ok(RepositoryOutcome {
        repository_id: conn.last_insert_rowid(),
    })
}

pub fn list_repositories(root: &Path) -> Result<Vec<RepositoryRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select id, name, path, current_head, status_summary, last_checked_at
        from repositories
        where project_id = ?1
        order by id
        "#,
    )?;
    let rows = stmt.query_map(params![project_id], |row| {
        Ok(RepositoryRecord {
            id: row.get(0)?,
            name: row.get(1)?,
            path: row.get(2)?,
            current_head: row.get(3)?,
            status_summary: row.get(4)?,
            last_checked_at: row.get(5)?,
        })
    })?;
    collect_rows(rows)
}

pub fn add_repository_snapshot(
    root: &Path,
    input: NewRepositorySnapshot<'_>,
) -> Result<RepositorySnapshotOutcome> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let repository_id = resolve_repository(&conn, project_id, input.repository)?;
    if let Some(activation_id) = input.work_unit_activation_id {
        conn.query_row(
            "select 1 from work_unit_activations where id = ?1 and project_id = ?2",
            params![activation_id, project_id],
            |_| Ok(()),
        )
        .optional()?
        .context("work unit activation not found")?;
    }

    conn.execute(
        r#"
        insert into repository_snapshots(
            repository_id, work_unit_activation_id, head_sha, branch,
            status_summary, is_clean, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, current_timestamp)
        "#,
        params![
            repository_id,
            input.work_unit_activation_id,
            input.head_sha,
            input.branch,
            input.status_summary,
            bool_to_i64(input.is_clean),
        ],
    )?;

    Ok(RepositorySnapshotOutcome {
        repository_snapshot_id: conn.last_insert_rowid(),
        repository_id,
    })
}

pub fn list_repository_snapshots(
    root: &Path,
    repository: Option<&str>,
) -> Result<Vec<RepositorySnapshotRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let repository_id = match repository {
        Some(repository) => Some(resolve_repository(&conn, project_id, repository)?),
        None => None,
    };
    let mut stmt = conn.prepare(
        r#"
        select
            s.id, s.repository_id, r.name, s.work_unit_activation_id,
            s.head_sha, s.branch, s.status_summary, s.is_clean, s.created_at
        from repository_snapshots s
        join repositories r on r.id = s.repository_id
        where r.project_id = ?1 and (?2 is null or s.repository_id = ?2)
        order by s.id
        "#,
    )?;
    let rows = stmt.query_map(params![project_id, repository_id], |row| {
        Ok(RepositorySnapshotRecord {
            id: row.get(0)?,
            repository_id: row.get(1)?,
            repository_name: row.get(2)?,
            work_unit_activation_id: row.get(3)?,
            head_sha: row.get(4)?,
            branch: row.get(5)?,
            status_summary: row.get(6)?,
            is_clean: row.get::<_, i64>(7)? == 1,
            created_at: row.get(8)?,
        })
    })?;
    collect_rows(rows)
}

pub fn add_repository_dirty_entry(
    root: &Path,
    input: NewRepositoryDirtyEntry<'_>,
) -> Result<RepositoryDirtyEntryOutcome> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    ensure_snapshot_project(&conn, project_id, input.repository_snapshot_id)?;
    conn.execute(
        r#"
        insert into repository_dirty_entries(
            repository_snapshot_id, path, change_type, staged, content_hash
        )
        values (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            input.repository_snapshot_id,
            input.path,
            input.change_type,
            bool_to_i64(input.staged),
            input.content_hash,
        ],
    )?;

    Ok(RepositoryDirtyEntryOutcome {
        repository_dirty_entry_id: conn.last_insert_rowid(),
    })
}

pub fn add_repository_state_classification(
    root: &Path,
    input: NewRepositoryStateClassification<'_>,
) -> Result<RepositoryStateClassificationOutcome> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    ensure_snapshot_project(&conn, project_id, input.repository_snapshot_id)?;
    if let Some(dirty_entry_id) = input.dirty_entry_id {
        conn.query_row(
            r#"
            select 1
            from repository_dirty_entries
            where id = ?1 and repository_snapshot_id = ?2
            "#,
            params![dirty_entry_id, input.repository_snapshot_id],
            |_| Ok(()),
        )
        .optional()?
        .context("repository dirty entry not found for snapshot")?;
    }
    conn.execute(
        r#"
        insert into repository_state_classifications(
            repository_snapshot_id, dirty_entry_id, classification,
            reason, acceptance_record_id, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, current_timestamp)
        "#,
        params![
            input.repository_snapshot_id,
            input.dirty_entry_id,
            input.classification,
            input.reason,
            input.acceptance_record_id,
        ],
    )?;

    Ok(RepositoryStateClassificationOutcome {
        repository_state_classification_id: conn.last_insert_rowid(),
    })
}

pub fn add_repository_snapshot_comparison(
    root: &Path,
    input: NewRepositorySnapshotComparison<'_>,
) -> Result<RepositorySnapshotComparisonOutcome> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    ensure_snapshot_project(&conn, project_id, input.base_repository_snapshot_id)?;
    ensure_snapshot_project(&conn, project_id, input.current_repository_snapshot_id)?;
    conn.execute(
        r#"
        insert into repository_snapshot_comparisons(
            base_repository_snapshot_id, current_repository_snapshot_id,
            comparison_type, head_changed, dirty_state_changed,
            nested_repository_changed, result, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, current_timestamp)
        "#,
        params![
            input.base_repository_snapshot_id,
            input.current_repository_snapshot_id,
            input.comparison_type,
            bool_to_i64(input.head_changed),
            bool_to_i64(input.dirty_state_changed),
            bool_to_i64(input.nested_repository_changed),
            input.result,
        ],
    )?;

    Ok(RepositorySnapshotComparisonOutcome {
        repository_snapshot_comparison_id: conn.last_insert_rowid(),
    })
}

pub fn add_git_commit(root: &Path, input: NewGitCommit<'_>) -> Result<GitCommitOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let repository_id = resolve_repository(&tx, project_id, input.repository)?;
    tx.execute(
        r#"
        insert into git_commits(
            repository_id, commit_sha, short_sha, subject, author_name,
            author_email, committed_at, parent_shas, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, current_timestamp)
        "#,
        params![
            repository_id,
            input.commit_sha,
            input.short_sha,
            input.subject,
            input.author_name,
            input.author_email,
            input.committed_at,
            input.parent_shas,
        ],
    )?;
    let git_commit_id = tx.last_insert_rowid();
    backfill_work_record_commits(&tx, project_id, git_commit_id, input.commit_sha)?;
    tx.commit()?;

    Ok(GitCommitOutcome {
        git_commit_id,
        repository_id,
    })
}

pub fn add_git_file_change(
    root: &Path,
    input: NewGitFileChange<'_>,
) -> Result<GitFileChangeOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let commit_repository_id = tx
        .query_row(
            r#"
            select c.repository_id
            from git_commits c
            join repositories r on r.id = c.repository_id
            where c.id = ?1 and r.project_id = ?2
            "#,
            params![input.git_commit_id, project_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .context("git commit not found")?;
    let repository_id = match input.repository {
        Some(repository) => resolve_repository(&tx, project_id, repository)?,
        None => commit_repository_id,
    };
    if repository_id != commit_repository_id {
        bail!("git file change repository must match git commit repository");
    }

    tx.execute(
        r#"
        insert into git_file_changes(
            git_commit_id, repository_id, path, old_path, change_type,
            additions, deletions, content_hash
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        params![
            input.git_commit_id,
            repository_id,
            input.path,
            input.old_path,
            input.change_type,
            input.additions,
            input.deletions,
            input.content_hash,
        ],
    )?;
    let git_file_change_id = tx.last_insert_rowid();
    backfill_work_record_files(
        &tx,
        project_id,
        git_file_change_id,
        repository_id,
        input.path,
    )?;
    tx.commit()?;

    Ok(GitFileChangeOutcome {
        git_file_change_id,
        repository_id,
    })
}

fn backfill_work_record_commits(
    conn: &Connection,
    project_id: i64,
    git_commit_id: i64,
    commit_sha: &str,
) -> Result<()> {
    let matching_commit_count = conn.query_row(
        r#"
        select count(*)
        from git_commits c
        join repositories r on r.id = c.repository_id
        where r.project_id = ?1 and c.commit_sha = ?2
        "#,
        params![project_id, commit_sha],
        |row| row.get::<_, i64>(0),
    )?;
    if matching_commit_count != 1 {
        conn.execute(
            r#"
            update work_record_commits
            set git_commit_id = null,
                auto_linked = 0
            where auto_linked = 1
              and commit_sha = ?1
              and work_record_id in (
                  select id from work_records where project_id = ?2
              )
            "#,
            params![commit_sha, project_id],
        )?;
        return Ok(());
    }

    conn.execute(
        r#"
        update work_record_commits
        set git_commit_id = ?1,
            auto_linked = 1
        where git_commit_id is null
          and commit_sha = ?2
          and work_record_id in (
              select id from work_records where project_id = ?3
          )
        "#,
        params![git_commit_id, commit_sha, project_id],
    )?;
    Ok(())
}

fn backfill_work_record_files(
    conn: &Connection,
    project_id: i64,
    git_file_change_id: i64,
    repository_id: i64,
    path: &str,
) -> Result<()> {
    let matching_path_count = conn.query_row(
        r#"
        select count(*)
        from git_file_changes f
        join repositories r on r.id = f.repository_id
        where r.project_id = ?1 and f.path = ?2
        "#,
        params![project_id, path],
        |row| row.get::<_, i64>(0),
    )?;
    if matching_path_count != 1 {
        conn.execute(
            r#"
            update work_record_files
            set git_file_change_id = null,
                repository_id = null,
                auto_linked = 0
            where auto_linked = 1
              and path = ?1
              and work_record_id in (
                  select id from work_records where project_id = ?2
              )
            "#,
            params![path, project_id],
        )?;
        return Ok(());
    }

    conn.execute(
        r#"
        update work_record_files
        set git_file_change_id = ?1,
            repository_id = ?2,
            auto_linked = 1
        where git_file_change_id is null
          and path = ?3
          and work_record_id in (
              select id from work_records where project_id = ?4
          )
          and (repository_id is null or repository_id = ?2)
        "#,
        params![git_file_change_id, repository_id, path, project_id],
    )?;
    Ok(())
}

fn resolve_repository(conn: &Connection, project_id: i64, repository: &str) -> Result<i64> {
    if let Ok(id) = repository.parse::<i64>() {
        return conn
            .query_row(
                "select id from repositories where project_id = ?1 and id = ?2",
                params![project_id, id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .context("repository not found");
    }

    let mut stmt = conn.prepare(
        r#"
        select id
        from repositories
        where project_id = ?1 and (name = ?2 or path = ?2)
        order by id
        limit 2
        "#,
    )?;
    let rows = stmt.query_map(params![project_id, repository], |row| row.get::<_, i64>(0))?;
    let mut ids = collect_rows(rows)?;
    match ids.len() {
        0 => bail!("repository not found"),
        1 => Ok(ids.remove(0)),
        _ => bail!("repository reference is ambiguous"),
    }
}

fn ensure_snapshot_project(
    conn: &Connection,
    project_id: i64,
    repository_snapshot_id: i64,
) -> Result<()> {
    conn.query_row(
        r#"
        select 1
        from repository_snapshots s
        join repositories r on r.id = s.repository_id
        where s.id = ?1 and r.project_id = ?2
        "#,
        params![repository_snapshot_id, project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("repository snapshot not found")
}

fn collect_rows<T>(
    rows: rusqlite::MappedRows<'_, impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>>,
) -> Result<Vec<T>> {
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

fn bool_to_i64(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

pub struct NewRepository<'a> {
    pub name: &'a str,
    pub path: &'a str,
    pub current_head: Option<&'a str>,
    pub status_summary: Option<&'a str>,
}

pub struct NewRepositorySnapshot<'a> {
    pub repository: &'a str,
    pub work_unit_activation_id: Option<i64>,
    pub head_sha: Option<&'a str>,
    pub branch: Option<&'a str>,
    pub status_summary: Option<&'a str>,
    pub is_clean: bool,
}

pub struct NewRepositoryDirtyEntry<'a> {
    pub repository_snapshot_id: i64,
    pub path: &'a str,
    pub change_type: &'a str,
    pub staged: bool,
    pub content_hash: Option<&'a str>,
}

pub struct NewRepositoryStateClassification<'a> {
    pub repository_snapshot_id: i64,
    pub dirty_entry_id: Option<i64>,
    pub classification: &'a str,
    pub reason: &'a str,
    pub acceptance_record_id: Option<i64>,
}

pub struct NewRepositorySnapshotComparison<'a> {
    pub base_repository_snapshot_id: i64,
    pub current_repository_snapshot_id: i64,
    pub comparison_type: &'a str,
    pub head_changed: bool,
    pub dirty_state_changed: bool,
    pub nested_repository_changed: bool,
    pub result: &'a str,
}

pub struct NewGitCommit<'a> {
    pub repository: &'a str,
    pub commit_sha: &'a str,
    pub short_sha: Option<&'a str>,
    pub subject: Option<&'a str>,
    pub author_name: Option<&'a str>,
    pub author_email: Option<&'a str>,
    pub committed_at: Option<&'a str>,
    pub parent_shas: Option<&'a str>,
}

pub struct NewGitFileChange<'a> {
    pub git_commit_id: i64,
    pub repository: Option<&'a str>,
    pub path: &'a str,
    pub old_path: Option<&'a str>,
    pub change_type: &'a str,
    pub additions: Option<i64>,
    pub deletions: Option<i64>,
    pub content_hash: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RepositoryOutcome {
    pub repository_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RepositorySnapshotOutcome {
    pub repository_snapshot_id: i64,
    pub repository_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RepositoryDirtyEntryOutcome {
    pub repository_dirty_entry_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RepositoryStateClassificationOutcome {
    pub repository_state_classification_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RepositorySnapshotComparisonOutcome {
    pub repository_snapshot_comparison_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct GitCommitOutcome {
    pub git_commit_id: i64,
    pub repository_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct GitFileChangeOutcome {
    pub git_file_change_id: i64,
    pub repository_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RepositoryRecord {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub current_head: Option<String>,
    pub status_summary: Option<String>,
    pub last_checked_at: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct RepositorySnapshotRecord {
    pub id: i64,
    pub repository_id: i64,
    pub repository_name: String,
    pub work_unit_activation_id: Option<i64>,
    pub head_sha: Option<String>,
    pub branch: Option<String>,
    pub status_summary: Option<String>,
    pub is_clean: bool,
    pub created_at: String,
}
