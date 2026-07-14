use std::fmt::Write;
use std::path::Path;

use anyhow::Result;
use rusqlite::params;

use crate::db::{open_existing_project, project_id};

pub(super) fn render(root: &Path, work_unit_id: i64, output: &mut String) -> Result<()> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;

    writeln!(output, "work_records:")?;
    let mut records = conn.prepare(
        r#"
        select id, topic, work_performed, notable_operations, next_actions
        from work_records
        where project_id = ?1 and work_unit_id = ?2
        order by id
        "#,
    )?;
    let rows = records.query_map(params![project_id, work_unit_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;
    let mut record_count = 0;
    for row in rows {
        let (id, topic, performed, operations, next) = row?;
        record_count += 1;
        writeln!(
            output,
            "- {} topic={} performed={} operations={} next={}",
            id,
            one_line(&topic),
            optional_text(performed.as_deref()),
            optional_text(operations.as_deref()),
            optional_text(next.as_deref())
        )?;
    }
    if record_count == 0 {
        writeln!(output, "- none")?;
    }

    render_commands(&conn, project_id, work_unit_id, output)?;
    render_files(&conn, project_id, work_unit_id, output)?;
    render_commits(&conn, project_id, work_unit_id, output)?;
    render_snapshots(&conn, project_id, work_unit_id, output)?;
    Ok(())
}

fn render_commands(
    conn: &rusqlite::Connection,
    project_id: i64,
    work_unit_id: i64,
    output: &mut String,
) -> Result<()> {
    writeln!(output, "work_record_commands:")?;
    let mut statement = conn.prepare(
        r#"
        select wrc.id, wr.id, wrc.command_usage_id,
               coalesce(wrc.command, cu.command), coalesce(wrc.result, cu.result),
               coalesce(wrc.log_path, cu.log_path), wrc.note
        from work_record_commands wrc
        join work_records wr on wr.id = wrc.work_record_id
        left join command_usages cu on cu.id = wrc.command_usage_id
        where wr.project_id = ?1 and wr.work_unit_id = ?2
        order by wr.id, wrc.id
        "#,
    )?;
    let rows = statement.query_map(params![project_id, work_unit_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, Option<i64>>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    })?;
    let mut count = 0;
    for row in rows {
        let (id, record, usage, command, result, log, note) = row?;
        count += 1;
        writeln!(
            output,
            "- {} record={} usage={} command={} result={} log={} note={}",
            id,
            record,
            optional_id(usage),
            optional_text(command.as_deref()),
            optional_text(result.as_deref()),
            optional_text(log.as_deref()),
            optional_text(note.as_deref())
        )?;
    }
    if count == 0 {
        writeln!(output, "- none")?;
    }
    Ok(())
}

fn render_files(
    conn: &rusqlite::Connection,
    project_id: i64,
    work_unit_id: i64,
    output: &mut String,
) -> Result<()> {
    writeln!(output, "work_record_files:")?;
    let mut statement = conn.prepare(
        r#"
        select wrf.id, wr.id, coalesce(r.name, '-'), wrf.path, wrf.role, wrf.note
        from work_record_files wrf
        join work_records wr on wr.id = wrf.work_record_id
        left join repositories r on r.id = wrf.repository_id
        where wr.project_id = ?1 and wr.work_unit_id = ?2
        order by wr.id, wrf.path, wrf.id
        "#,
    )?;
    let rows = statement.query_map(params![project_id, work_unit_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, Option<String>>(5)?,
        ))
    })?;
    let mut count = 0;
    for row in rows {
        let (id, record, repository, path, role, note) = row?;
        count += 1;
        writeln!(
            output,
            "- {} record={} repository={} role={} path={} note={}",
            id,
            record,
            repository,
            role,
            path,
            optional_text(note.as_deref())
        )?;
    }
    if count == 0 {
        writeln!(output, "- none")?;
    }
    Ok(())
}

fn render_commits(
    conn: &rusqlite::Connection,
    project_id: i64,
    work_unit_id: i64,
    output: &mut String,
) -> Result<()> {
    writeln!(output, "work_record_commits:")?;
    let mut statement = conn.prepare(
        r#"
        select wrc.id, wr.id, wrc.commit_sha, wrc.role, wrc.note
        from work_record_commits wrc
        join work_records wr on wr.id = wrc.work_record_id
        where wr.project_id = ?1 and wr.work_unit_id = ?2
        order by wr.id, wrc.id
        "#,
    )?;
    let rows = statement.query_map(params![project_id, work_unit_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
        ))
    })?;
    let mut count = 0;
    for row in rows {
        let (id, record, commit, role, note) = row?;
        count += 1;
        writeln!(
            output,
            "- {} record={} role={} commit={} note={}",
            id,
            record,
            role,
            optional_text(commit.as_deref()),
            optional_text(note.as_deref())
        )?;
    }
    if count == 0 {
        writeln!(output, "- none")?;
    }
    Ok(())
}

fn render_snapshots(
    conn: &rusqlite::Connection,
    project_id: i64,
    work_unit_id: i64,
    output: &mut String,
) -> Result<()> {
    writeln!(output, "repository_snapshots:")?;
    let mut statement = conn.prepare(
        r#"
        select s.id, r.name, s.work_unit_activation_id, s.head_sha, s.branch,
               s.status_summary, s.is_clean,
               count(distinct d.id), count(distinct case when c.id is not null then d.id end)
        from repository_snapshots s
        join repositories r on r.id = s.repository_id
        join work_unit_activations a on a.id = s.work_unit_activation_id
        left join repository_dirty_entries d on d.repository_snapshot_id = s.id
        left join repository_state_classifications c
          on c.repository_snapshot_id = s.id and c.dirty_entry_id = d.id
        where r.project_id = ?1 and a.work_unit_id = ?2
        group by s.id, r.name, s.work_unit_activation_id, s.head_sha, s.branch,
                 s.status_summary, s.is_clean
        order by s.id
        "#,
    )?;
    let rows = statement.query_map(params![project_id, work_unit_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, i64>(6)? == 1,
            row.get::<_, i64>(7)?,
            row.get::<_, i64>(8)?,
        ))
    })?;
    let mut count = 0;
    for row in rows {
        let (id, repository, activation, head, branch, status, clean, dirty, classified) = row?;
        count += 1;
        writeln!(
            output,
            "- {} repository={} activation={} clean={} head={} branch={} dirty={} classified={} status={}",
            id,
            repository,
            activation,
            clean,
            optional_text(head.as_deref()),
            optional_text(branch.as_deref()),
            dirty,
            classified,
            optional_text(status.as_deref())
        )?;
    }
    if count == 0 {
        writeln!(output, "- none")?;
    }
    Ok(())
}

fn optional_id(value: Option<i64>) -> String {
    value
        .map(|id| id.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn optional_text(value: Option<&str>) -> String {
    value.map(one_line).unwrap_or_else(|| "-".to_string())
}

fn one_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}
