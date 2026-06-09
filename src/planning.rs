use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{active_activation, open_existing_project, project_id};

pub fn add_task(root: &Path, input: NewTask<'_>) -> Result<TaskOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let work_unit_id = match input.work_unit_id {
        Some(work_unit_id) => Some(work_unit_id),
        None => active_activation(&tx)?.map(|active| active.work_unit_id),
    };

    tx.execute(
        r#"
        insert into tasks(
            work_unit_id, title, priority, status, source, details, completion_condition
        )
        values (?1, ?2, ?3, 'open', ?4, ?5, ?6)
        "#,
        params![
            work_unit_id,
            input.title,
            input.priority,
            input.source,
            input.details,
            input.completion_condition,
        ],
    )?;
    let task_id = tx.last_insert_rowid();
    tx.commit()?;

    Ok(TaskOutcome {
        task_id,
        work_unit_id,
    })
}

pub fn list_tasks(root: &Path, input: TaskListQuery<'_>) -> Result<Vec<TaskRecord>> {
    let conn = open_existing_project(root)?;
    let mut records = Vec::new();

    match (input.status, input.work_unit_id) {
        (Some(status), Some(work_unit_id)) => {
            let mut stmt = conn.prepare(
                r#"
                select id, work_unit_id, title, priority, status, source, closed_by_commit
                from tasks
                where status = ?1 and work_unit_id = ?2
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![status, work_unit_id], task_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        (Some(status), None) => {
            let mut stmt = conn.prepare(
                r#"
                select id, work_unit_id, title, priority, status, source, closed_by_commit
                from tasks
                where status = ?1
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![status], task_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        (None, Some(work_unit_id)) => {
            let mut stmt = conn.prepare(
                r#"
                select id, work_unit_id, title, priority, status, source, closed_by_commit
                from tasks
                where work_unit_id = ?1
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![work_unit_id], task_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        (None, None) => {
            let mut stmt = conn.prepare(
                r#"
                select id, work_unit_id, title, priority, status, source, closed_by_commit
                from tasks
                order by id
                "#,
            )?;
            let rows = stmt.query_map([], task_record)?;
            for row in rows {
                records.push(row?);
            }
        }
    }

    Ok(records)
}

pub fn close_task(root: &Path, task_id: i64, commit: Option<&str>) -> Result<TaskCloseOutcome> {
    let conn = open_existing_project(root)?;
    ensure_design_task_closure_ready(&conn, task_id)?;
    let changed = conn.execute(
        r#"
        update tasks
        set status = 'closed', closed_by_commit = ?1
        where id = ?2 and status != 'closed'
        "#,
        params![commit, task_id],
    )?;
    if changed == 0 {
        bail!("task not found or already closed");
    }

    Ok(TaskCloseOutcome { task_id })
}

fn ensure_design_task_closure_ready(conn: &rusqlite::Connection, task_id: i64) -> Result<()> {
    let active_derivation_count: i64 = conn.query_row(
        "select count(*) from task_derivations where task_id = ?1 and status = 'active'",
        params![task_id],
        |row| row.get(0),
    )?;
    if active_derivation_count == 0 {
        return Ok(());
    }

    let missing_checklist_count: i64 = conn.query_row(
        r#"
        select count(*)
        from task_derivations
        where task_id = ?1
          and status = 'active'
          and checklist_item_id is null
        "#,
        params![task_id],
        |row| row.get(0),
    )?;
    if missing_checklist_count > 0 {
        bail!(
            "cannot close design-derived task; {missing_checklist_count} derivations have no checklist item"
        );
    }

    let missing_completion_count: i64 = conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join tasks t on t.id = td.task_id
        left join checklist_items ci on ci.id = td.checklist_item_id
        where td.task_id = ?1
          and td.status = 'active'
          and coalesce(
            nullif(trim(ci.completion_condition), ''),
            nullif(trim(t.completion_condition), '')
          ) is null
        "#,
        params![task_id],
        |row| row.get(0),
    )?;
    if missing_completion_count > 0 {
        bail!(
            "cannot close design-derived task; {missing_completion_count} derivations have no completion condition"
        );
    }

    let missing_gate_count: i64 = conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        where td.task_id = ?1
          and td.status = 'active'
          and not exists (
            select 1
            from validation_gates vg
            where vg.design_requirement_id = td.design_requirement_id
              and vg.task_id = td.task_id
              and vg.selected_before_edit = 1
              and vg.status = 'active'
          )
        "#,
        params![task_id],
        |row| row.get(0),
    )?;
    if missing_gate_count > 0 {
        bail!(
            "cannot close design-derived task; {missing_gate_count} derivations have no selected validation gate"
        );
    }

    let missing_evidence_count: i64 = conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        where td.task_id = ?1
          and td.status = 'active'
          and not exists (
            select 1
            from implementation_evidence e
            where e.task_id = td.task_id
              and e.design_requirement_id = td.design_requirement_id
          )
        "#,
        params![task_id],
        |row| row.get(0),
    )?;
    if missing_evidence_count > 0 {
        bail!(
            "cannot close design-derived task; {missing_evidence_count} derivations have no implementation evidence"
        );
    }

    let missing_coverage_count: i64 = conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join tasks t on t.id = td.task_id
        where td.task_id = ?1
          and td.status = 'active'
          and not exists (
            select 1
            from coverage_items c
            where c.design_requirement_id = td.design_requirement_id
              and (
                c.task_id = td.task_id
                or (c.task_id is null and c.work_unit_id = t.work_unit_id)
              )
              and (
                c.status = 'covered'
                or (
                  c.status = 'accepted_out_of_scope'
                  and exists (
                    select 1
                    from acceptance_records ar
                    where ar.target_type = 'coverage_item'
                      and ar.coverage_item_id = c.id
                      and ar.acceptance_type = 'accepted_out_of_scope'
                      and ar.status = 'approved'
                  )
                )
              )
          )
        "#,
        params![task_id],
        |row| row.get(0),
    )?;
    if missing_coverage_count > 0 {
        bail!(
            "cannot close design-derived task; {missing_coverage_count} derivations have no covered coverage item"
        );
    }

    Ok(())
}

pub fn accept_task_out_of_scope(
    root: &Path,
    task_id: i64,
    reason: &str,
) -> Result<TaskAcceptanceOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let work_unit_id = tx
        .query_row(
            "select work_unit_id from tasks where id = ?1 and status in ('open', 'blocked')",
            params![task_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()?
        .context("task not found or not open for acceptance")?;

    tx.execute(
        r#"
        insert into authority_events(
            project_id, event_type, source, text_or_summary, scope, precedence,
            status, created_at
        )
        values (?1, 'user_instruction', 'task accept-out-of-scope', ?2, ?3, 100, 'active', current_timestamp)
        "#,
        params![
            project_id,
            format!("accepted task {task_id} out of scope: {reason}"),
            work_unit_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "project".to_string()),
        ],
    )?;
    let authority_event_id = tx.last_insert_rowid();
    tx.execute(
        r#"
        insert into acceptance_records(
            project_id, target_type, task_id, acceptance_type, reason, scope,
            created_by, status, approved_by_authority_event_id, approved_at,
            created_at, review_impact
        )
        values (
            ?1, 'task', ?2, 'accepted_out_of_scope', ?3, ?4,
            'user', 'approved', ?5, current_timestamp, current_timestamp,
            'task accepted out of scope for current work scope'
        )
        "#,
        params![
            project_id,
            task_id,
            reason,
            work_unit_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "project".to_string()),
            authority_event_id,
        ],
    )?;
    let acceptance_record_id = tx.last_insert_rowid();
    let changed = tx.execute(
        r#"
        update tasks
        set status = 'accepted_out_of_scope',
            details = case
                when details is null or details = '' then ?1
                else details || char(10) || 'accepted_out_of_scope: ' || ?1
            end
        where id = ?2 and status in ('open', 'blocked')
        "#,
        params![reason, task_id],
    )?;
    if changed == 0 {
        bail!("task not found or not open for acceptance");
    }
    tx.commit()?;

    Ok(TaskAcceptanceOutcome {
        task_id,
        acceptance_record_id,
        authority_event_id,
    })
}

pub fn add_decision(root: &Path, input: NewDecision<'_>) -> Result<DecisionOutcome> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    conn.execute(
        r#"
        insert into decisions(
            project_id, decision_key, topic, decision, rationale,
            compatibility_impact, status, authority_refs, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, 'accepted', ?7, current_timestamp)
        "#,
        params![
            project_id,
            input.decision_key,
            input.topic,
            input.decision,
            input.rationale,
            input.compatibility_impact,
            input.authority_refs,
        ],
    )?;

    Ok(DecisionOutcome {
        decision_id: conn.last_insert_rowid(),
    })
}

pub fn list_decisions(root: &Path, query: Option<&str>) -> Result<Vec<DecisionRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut records = Vec::new();

    match query {
        Some(query) => {
            let pattern = format!("%{query}%");
            let mut stmt = conn.prepare(
                r#"
                select id, decision_key, topic, decision, rationale, status
                from decisions
                where project_id = ?1
                  and status = 'accepted'
                  and (topic like ?2 or decision like ?2 or coalesce(decision_key, '') like ?2)
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![project_id, pattern], decision_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        None => {
            let mut stmt = conn.prepare(
                r#"
                select id, decision_key, topic, decision, rationale, status
                from decisions
                where project_id = ?1 and status = 'accepted'
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![project_id], decision_record)?;
            for row in rows {
                records.push(row?);
            }
        }
    }

    Ok(records)
}

fn task_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskRecord> {
    Ok(TaskRecord {
        id: row.get(0)?,
        work_unit_id: row.get(1)?,
        title: row.get(2)?,
        priority: row.get(3)?,
        status: row.get(4)?,
        source: row.get(5)?,
        closed_by_commit: row.get(6)?,
    })
}

fn decision_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<DecisionRecord> {
    Ok(DecisionRecord {
        id: row.get(0)?,
        decision_key: row.get(1)?,
        topic: row.get(2)?,
        decision: row.get(3)?,
        rationale: row.get(4)?,
        status: row.get(5)?,
    })
}

pub struct NewTask<'a> {
    pub title: &'a str,
    pub priority: &'a str,
    pub source: &'a str,
    pub work_unit_id: Option<i64>,
    pub details: Option<&'a str>,
    pub completion_condition: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskOutcome {
    pub task_id: i64,
    pub work_unit_id: Option<i64>,
}

pub struct TaskListQuery<'a> {
    pub status: Option<&'a str>,
    pub work_unit_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskRecord {
    pub id: i64,
    pub work_unit_id: Option<i64>,
    pub title: String,
    pub priority: String,
    pub status: String,
    pub source: String,
    pub closed_by_commit: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskCloseOutcome {
    pub task_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskAcceptanceOutcome {
    pub task_id: i64,
    pub acceptance_record_id: i64,
    pub authority_event_id: i64,
}

pub struct NewDecision<'a> {
    pub decision_key: Option<&'a str>,
    pub topic: &'a str,
    pub decision: &'a str,
    pub rationale: Option<&'a str>,
    pub compatibility_impact: Option<&'a str>,
    pub authority_refs: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DecisionOutcome {
    pub decision_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DecisionRecord {
    pub id: i64,
    pub decision_key: Option<String>,
    pub topic: String,
    pub decision: String,
    pub rationale: Option<String>,
    pub status: String,
}
