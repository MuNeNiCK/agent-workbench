use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

use crate::db::{active_activation, open_existing_project, project_id};

pub fn create_work_record(root: &Path, input: NewWorkRecord<'_>) -> Result<WorkRecordOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let work_unit_id = match input.work_unit_id {
        Some(work_unit_id) => Some(work_unit_id),
        None => active_activation(&tx)?.map(|active| active.work_unit_id),
    };
    if let Some(work_unit_id) = work_unit_id {
        tx.query_row(
            "select 1 from work_units where id = ?1 and project_id = ?2",
            params![work_unit_id, project_id],
            |_| Ok(()),
        )
        .optional()?
        .context("work unit not found")?;
    }

    tx.execute(
        r#"
        insert into work_records(
            project_id, work_unit_id, topic, work_performed, next_actions, notable_operations,
            export_path, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, current_timestamp)
        "#,
        params![
            project_id,
            work_unit_id,
            input.topic,
            input.work_performed,
            input.next_actions,
            input.notable_operations,
            input.export_path,
        ],
    )?;
    let work_record_id = tx.last_insert_rowid();
    tx.commit()?;

    Ok(WorkRecordOutcome {
        work_record_id,
        work_unit_id,
    })
}

pub fn list_work_records(root: &Path, work_unit_id: Option<i64>) -> Result<Vec<WorkRecordEntry>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut records = Vec::new();

    match work_unit_id {
        Some(work_unit_id) => {
            let mut stmt = conn.prepare(
                r#"
                select id, work_unit_id, topic, work_performed, next_actions, created_at
                from work_records
                where project_id = ?1 and work_unit_id = ?2
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![project_id, work_unit_id], work_record_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        None => {
            let mut stmt = conn.prepare(
                r#"
                select id, work_unit_id, topic, work_performed, next_actions, created_at
                from work_records
                where project_id = ?1
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![project_id], work_record_record)?;
            for row in rows {
                records.push(row?);
            }
        }
    }

    Ok(records)
}

pub fn add_work_record_command(
    root: &Path,
    input: NewWorkRecordCommand<'_>,
) -> Result<WorkRecordLinkOutcome> {
    let conn = open_existing_project(root)?;
    ensure_work_record_exists(&conn, input.work_record_id)?;
    if input.command_usage_id.is_none() && input.command.is_none() {
        anyhow::bail!("either --usage or --command is required");
    }
    let work_record_project_id = work_record_project_id(&conn, input.work_record_id)?;
    if let Some(command_usage_id) = input.command_usage_id {
        ensure_command_usage_project(&conn, command_usage_id, work_record_project_id)?;
    }
    if let Some(command_profile_id) = input.command_profile_id {
        ensure_command_profile_project(&conn, command_profile_id, work_record_project_id)?;
    }
    conn.execute(
        r#"
        insert into work_record_commands(
            work_record_id, command_usage_id, command_profile_id, command, result, log_path, note
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![
            input.work_record_id,
            input.command_usage_id,
            input.command_profile_id,
            input.command,
            input.result,
            input.log_path,
            input.note,
        ],
    )?;

    Ok(WorkRecordLinkOutcome {
        link_id: conn.last_insert_rowid(),
    })
}

pub fn add_work_record_commit(
    root: &Path,
    input: NewWorkRecordCommit<'_>,
) -> Result<WorkRecordLinkOutcome> {
    let conn = open_existing_project(root)?;
    ensure_work_record_exists(&conn, input.work_record_id)?;
    insert_work_record_commit(
        &conn,
        CommitLinkInput {
            work_record_id: input.work_record_id,
            git_commit_id: None,
            commit_sha: input.commit_sha,
            role: input.role,
            note: input.note,
        },
    )
}

pub fn add_work_record_git_commit(
    root: &Path,
    input: NewWorkRecordGitCommit<'_>,
) -> Result<WorkRecordLinkOutcome> {
    let conn = open_existing_project(root)?;
    ensure_work_record_exists(&conn, input.work_record_id)?;
    if let Some(git_commit_id) = input.git_commit_id {
        ensure_git_commit_matches(&conn, git_commit_id, input.commit_sha)?;
    }
    insert_work_record_commit(
        &conn,
        CommitLinkInput {
            work_record_id: input.work_record_id,
            git_commit_id: input.git_commit_id,
            commit_sha: input.commit_sha,
            role: input.role,
            note: input.note,
        },
    )
}

fn insert_work_record_commit(
    conn: &Connection,
    input: CommitLinkInput<'_>,
) -> Result<WorkRecordLinkOutcome> {
    conn.execute(
        r#"
        insert into work_record_commits(work_record_id, git_commit_id, commit_sha, role, note)
        values (?1, ?2, ?3, ?4, ?5)
        "#,
        params![
            input.work_record_id,
            input.git_commit_id,
            input.commit_sha,
            input.role,
            input.note
        ],
    )?;

    Ok(WorkRecordLinkOutcome {
        link_id: conn.last_insert_rowid(),
    })
}

pub fn add_work_record_file(
    root: &Path,
    input: NewWorkRecordFile<'_>,
) -> Result<WorkRecordLinkOutcome> {
    let conn = open_existing_project(root)?;
    ensure_work_record_exists(&conn, input.work_record_id)?;
    insert_work_record_file(
        &conn,
        FileLinkInput {
            work_record_id: input.work_record_id,
            git_file_change_id: None,
            repository_id: None,
            path: input.path,
            role: input.role,
            note: input.note,
        },
    )
}

pub fn add_work_record_git_file(
    root: &Path,
    input: NewWorkRecordGitFile<'_>,
) -> Result<WorkRecordLinkOutcome> {
    let conn = open_existing_project(root)?;
    ensure_work_record_exists(&conn, input.work_record_id)?;
    let repository_id = match input.git_file_change_id {
        Some(git_file_change_id) => {
            let stored = ensure_git_file_change_matches(&conn, git_file_change_id, input.path)?;
            if let Some(repository_id) = input.repository_id
                && repository_id != stored.repository_id
            {
                anyhow::bail!("work record file repository must match git file change");
            }
            Some(input.repository_id.unwrap_or(stored.repository_id))
        }
        None => input.repository_id,
    };
    if let Some(repository_id) = repository_id {
        ensure_repository_exists(&conn, repository_id)?;
    }
    insert_work_record_file(
        &conn,
        FileLinkInput {
            work_record_id: input.work_record_id,
            git_file_change_id: input.git_file_change_id,
            repository_id,
            path: input.path,
            role: input.role,
            note: input.note,
        },
    )
}

fn insert_work_record_file(
    conn: &Connection,
    input: FileLinkInput<'_>,
) -> Result<WorkRecordLinkOutcome> {
    conn.execute(
        r#"
        insert into work_record_files(
            work_record_id, git_file_change_id, repository_id, path, role, note
        )
        values (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        params![
            input.work_record_id,
            input.git_file_change_id,
            input.repository_id,
            input.path,
            input.role,
            input.note
        ],
    )?;

    Ok(WorkRecordLinkOutcome {
        link_id: conn.last_insert_rowid(),
    })
}

pub fn export_work_record_markdown(root: &Path, work_record_id: i64) -> Result<String> {
    let conn = open_existing_project(root)?;
    let current_project_id = project_id(&conn)?;
    let record = conn
        .query_row(
            r#"
            select id, work_unit_id, topic, work_performed, next_actions,
                   notable_operations, created_at
            from work_records
            where id = ?1 and project_id = ?2
            "#,
            params![work_record_id, current_project_id],
            |row| {
                Ok(StoredWorkRecord {
                    id: row.get(0)?,
                    work_unit_id: row.get(1)?,
                    topic: row.get(2)?,
                    work_performed: row.get(3)?,
                    next_actions: row.get(4)?,
                    notable_operations: row.get(5)?,
                    created_at: row.get(6)?,
                })
            },
        )
        .optional()?
        .context("work record not found")?;
    let commands = work_record_commands(&conn, record.id)?;
    let commits = work_record_commits(&conn, record.id)?;
    let files = work_record_files(&conn, record.id)?;
    let tasks = match record.work_unit_id {
        Some(work_unit_id) => work_record_tasks(&conn, work_unit_id)?,
        None => Vec::new(),
    };

    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", record.topic));
    out.push_str(&format!("- work_record_id: {}\n", record.id));
    out.push_str(&format!("- created_at: {}\n", record.created_at));
    if let Some(work_unit_id) = record.work_unit_id {
        out.push_str(&format!("- work_unit_id: {work_unit_id}\n"));
    }

    push_optional_section(&mut out, "Work Performed", record.work_performed.as_deref());
    push_optional_section(&mut out, "Next Actions", record.next_actions.as_deref());
    push_optional_section(
        &mut out,
        "Notable Operations",
        record.notable_operations.as_deref(),
    );
    push_list_section(&mut out, "Commands", &commands);
    push_list_section(&mut out, "Commits", &commits);
    push_list_section(&mut out, "Files", &files);
    push_list_section(&mut out, "Tasks", &tasks);

    Ok(out)
}

fn work_record_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkRecordEntry> {
    Ok(WorkRecordEntry {
        id: row.get(0)?,
        work_unit_id: row.get(1)?,
        topic: row.get(2)?,
        work_performed: row.get(3)?,
        next_actions: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn ensure_work_record_exists(conn: &Connection, work_record_id: i64) -> Result<()> {
    let current_project_id = project_id(conn)?;
    let exists = conn
        .query_row(
            "select 1 from work_records where id = ?1 and project_id = ?2",
            params![work_record_id, current_project_id],
            |_| Ok(()),
        )
        .optional()?;
    exists.context("work record not found")
}

fn work_record_project_id(conn: &Connection, work_record_id: i64) -> Result<i64> {
    conn.query_row(
        "select project_id from work_records where id = ?1",
        params![work_record_id],
        |row| row.get::<_, i64>(0),
    )
    .optional()?
    .context("work record not found")
}

fn ensure_command_usage_project(
    conn: &Connection,
    command_usage_id: i64,
    work_record_project_id: i64,
) -> Result<()> {
    let usage_project_id = conn
        .query_row(
            r#"
            select project_id
            from command_usages cu
            where cu.id = ?1
            "#,
            params![command_usage_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .context("command usage not found")?;
    if work_record_project_id != usage_project_id {
        anyhow::bail!("work record command usage must match work record project");
    }
    Ok(())
}

fn ensure_command_profile_project(
    conn: &Connection,
    command_profile_id: i64,
    work_record_project_id: i64,
) -> Result<()> {
    let profile_project_id = conn
        .query_row(
            "select project_id from command_profiles where id = ?1",
            params![command_profile_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .context("command profile not found")?;
    if work_record_project_id != profile_project_id {
        anyhow::bail!("work record command profile must match work record project");
    }
    Ok(())
}

fn ensure_git_commit_matches(
    conn: &Connection,
    git_commit_id: i64,
    commit_sha: &str,
) -> Result<()> {
    let current_project_id = project_id(conn)?;
    let stored_sha = conn
        .query_row(
            r#"
            select c.commit_sha
            from git_commits c
            join repositories r on r.id = c.repository_id
            where c.id = ?1 and r.project_id = ?2
            "#,
            params![git_commit_id, current_project_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .context("git commit not found")?;
    if stored_sha != commit_sha {
        anyhow::bail!("work record commit sha must match git commit");
    }
    Ok(())
}

fn ensure_git_file_change_matches(
    conn: &Connection,
    git_file_change_id: i64,
    path: &str,
) -> Result<StoredGitFileChange> {
    let current_project_id = project_id(conn)?;
    let stored = conn
        .query_row(
            r#"
            select f.repository_id, f.path
            from git_file_changes f
            join repositories r on r.id = f.repository_id
            where f.id = ?1 and r.project_id = ?2
            "#,
            params![git_file_change_id, current_project_id],
            |row| {
                Ok(StoredGitFileChange {
                    repository_id: row.get(0)?,
                    path: row.get(1)?,
                })
            },
        )
        .optional()?
        .context("git file change not found")?;
    if stored.path != path {
        anyhow::bail!("work record file path must match git file change");
    }
    Ok(stored)
}

fn ensure_repository_exists(conn: &Connection, repository_id: i64) -> Result<()> {
    let current_project_id = project_id(conn)?;
    conn.query_row(
        "select 1 from repositories where id = ?1 and project_id = ?2",
        params![repository_id, current_project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("repository not found")
}

fn work_record_commands(conn: &Connection, work_record_id: i64) -> Result<Vec<String>> {
    let mut records = Vec::new();
    let mut stmt = conn.prepare(
        r#"
        select
            coalesce(wrc.command, cu.command),
            coalesce(wrc.result, cu.result),
            coalesce(wrc.log_path, cu.log_path),
            wrc.note,
            wrc.command_usage_id
        from work_record_commands wrc
        left join command_usages cu on cu.id = wrc.command_usage_id
        where wrc.work_record_id = ?1
        order by wrc.id
        "#,
    )?;
    let rows = stmt.query_map(params![work_record_id], |row| {
        let command: Option<String> = row.get(0)?;
        let result: Option<String> = row.get(1)?;
        let log_path: Option<String> = row.get(2)?;
        let note: Option<String> = row.get(3)?;
        let command_usage_id: Option<i64> = row.get(4)?;
        let mut text = command.unwrap_or_else(|| "(linked command usage)".to_string());
        if let Some(result) = result {
            text.push_str(&format!(" -> {result}"));
        }
        if let Some(log_path) = log_path {
            text.push_str(&format!(" ({log_path})"));
        }
        if let Some(command_usage_id) = command_usage_id {
            text.push_str(&format!(" [usage:{command_usage_id}]"));
        }
        if let Some(note) = note {
            text.push_str(&format!(" - {note}"));
        }
        Ok(text)
    })?;
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

fn work_record_commits(conn: &Connection, work_record_id: i64) -> Result<Vec<String>> {
    let mut records = Vec::new();
    let mut stmt = conn.prepare(
        r#"
        select commit_sha, role, note
        from work_record_commits
        where work_record_id = ?1
        order by id
        "#,
    )?;
    let rows = stmt.query_map(params![work_record_id], |row| {
        let commit_sha: String = row.get(0)?;
        let role: String = row.get(1)?;
        let note: Option<String> = row.get(2)?;
        Ok(match note {
            Some(note) => format!("{commit_sha} [{role}] - {note}"),
            None => format!("{commit_sha} [{role}]"),
        })
    })?;
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

fn work_record_files(conn: &Connection, work_record_id: i64) -> Result<Vec<String>> {
    let mut records = Vec::new();
    let mut stmt = conn.prepare(
        r#"
        select path, role, note
        from work_record_files
        where work_record_id = ?1
        order by id
        "#,
    )?;
    let rows = stmt.query_map(params![work_record_id], |row| {
        let path: String = row.get(0)?;
        let role: String = row.get(1)?;
        let note: Option<String> = row.get(2)?;
        Ok(match note {
            Some(note) => format!("{path} [{role}] - {note}"),
            None => format!("{path} [{role}]"),
        })
    })?;
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

fn work_record_tasks(conn: &Connection, work_unit_id: i64) -> Result<Vec<String>> {
    let mut records = Vec::new();
    let mut stmt = conn.prepare(
        r#"
        select id, title, priority, status
        from tasks
        where work_unit_id = ?1
        order by id
        "#,
    )?;
    let rows = stmt.query_map(params![work_unit_id], |row| {
        let id: i64 = row.get(0)?;
        let title: String = row.get(1)?;
        let priority: String = row.get(2)?;
        let status: String = row.get(3)?;
        Ok(format!("#{id} [{priority}:{status}] {title}"))
    })?;
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

fn push_optional_section(out: &mut String, title: &str, value: Option<&str>) {
    if let Some(value) = value
        && !value.trim().is_empty()
    {
        out.push_str(&format!("\n## {title}\n\n{value}\n"));
    }
}

fn push_list_section(out: &mut String, title: &str, values: &[String]) {
    if values.is_empty() {
        return;
    }
    out.push_str(&format!("\n## {title}\n\n"));
    for value in values {
        out.push_str(&format!("- {value}\n"));
    }
}

struct StoredWorkRecord {
    id: i64,
    work_unit_id: Option<i64>,
    topic: String,
    work_performed: Option<String>,
    next_actions: Option<String>,
    notable_operations: Option<String>,
    created_at: String,
}

pub struct NewWorkRecord<'a> {
    pub work_unit_id: Option<i64>,
    pub topic: &'a str,
    pub work_performed: Option<&'a str>,
    pub next_actions: Option<&'a str>,
    pub notable_operations: Option<&'a str>,
    pub export_path: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct WorkRecordOutcome {
    pub work_record_id: i64,
    pub work_unit_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct WorkRecordEntry {
    pub id: i64,
    pub work_unit_id: Option<i64>,
    pub topic: String,
    pub work_performed: Option<String>,
    pub next_actions: Option<String>,
    pub created_at: String,
}

pub struct NewWorkRecordCommand<'a> {
    pub work_record_id: i64,
    pub command_usage_id: Option<i64>,
    pub command_profile_id: Option<i64>,
    pub command: Option<&'a str>,
    pub result: Option<&'a str>,
    pub log_path: Option<&'a str>,
    pub note: Option<&'a str>,
}

pub struct NewWorkRecordCommit<'a> {
    pub work_record_id: i64,
    pub commit_sha: &'a str,
    pub role: &'a str,
    pub note: Option<&'a str>,
}

pub struct NewWorkRecordGitCommit<'a> {
    pub work_record_id: i64,
    pub git_commit_id: Option<i64>,
    pub commit_sha: &'a str,
    pub role: &'a str,
    pub note: Option<&'a str>,
}

pub struct NewWorkRecordFile<'a> {
    pub work_record_id: i64,
    pub path: &'a str,
    pub role: &'a str,
    pub note: Option<&'a str>,
}

pub struct NewWorkRecordGitFile<'a> {
    pub work_record_id: i64,
    pub git_file_change_id: Option<i64>,
    pub repository_id: Option<i64>,
    pub path: &'a str,
    pub role: &'a str,
    pub note: Option<&'a str>,
}

struct StoredGitFileChange {
    repository_id: i64,
    path: String,
}

struct CommitLinkInput<'a> {
    work_record_id: i64,
    git_commit_id: Option<i64>,
    commit_sha: &'a str,
    role: &'a str,
    note: Option<&'a str>,
}

struct FileLinkInput<'a> {
    work_record_id: i64,
    git_file_change_id: Option<i64>,
    repository_id: Option<i64>,
    path: &'a str,
    role: &'a str,
    note: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct WorkRecordLinkOutcome {
    pub link_id: i64,
}
