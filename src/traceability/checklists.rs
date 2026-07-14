use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{ensure_unscoped_mutation_allowed, open_existing_project, project_id};

use super::*;

pub fn list_checklists(root: &Path, status: Option<&str>) -> Result<Vec<ChecklistRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select c.id, c.work_unit_id, c.design_version_id, c.title, c.status,
               count(ci.id) as item_count,
               sum(case when ci.status = 'closed' then 1 else 0 end) as closed_count
        from checklists c
        left join checklist_items ci on ci.checklist_id = c.id
        where c.project_id = ?1
          and (?2 is null or c.status = ?2)
        group by c.id, c.work_unit_id, c.design_version_id, c.title, c.status
        order by c.id
        "#,
    )?;
    let rows = stmt.query_map(params![project_id, status], |row| {
        Ok(ChecklistRecord {
            id: row.get(0)?,
            work_unit_id: row.get(1)?,
            design_version_id: row.get(2)?,
            title: row.get(3)?,
            status: row.get(4)?,
            item_count: row.get(5)?,
            closed_count: row.get(6)?,
        })
    })?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

pub fn list_checklist_items(
    root: &Path,
    input: ChecklistItemListQuery<'_>,
) -> Result<Vec<ChecklistItemRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select ci.id, ci.checklist_id, c.work_unit_id, c.design_version_id,
               ci.design_requirement_id, r.requirement_key, ci.task_id, ci.item_order,
               ci.title, ci.completion_condition, ci.status
        from checklist_items ci
        join checklists c on c.id = ci.checklist_id
        join design_requirements r on r.id = ci.design_requirement_id
        where ci.project_id = ?1
          and (?2 is null or ci.checklist_id = ?2)
          and (?3 is null or ci.status = ?3)
        order by ci.checklist_id, ci.item_order, ci.id
        "#,
    )?;
    let rows = stmt.query_map(
        params![project_id, input.checklist_id, input.status],
        |row| {
            Ok(ChecklistItemRecord {
                id: row.get(0)?,
                checklist_id: row.get(1)?,
                work_unit_id: row.get(2)?,
                design_version_id: row.get(3)?,
                design_requirement_id: row.get(4)?,
                requirement_key: row.get(5)?,
                task_id: row.get(6)?,
                item_order: row.get(7)?,
                title: row.get(8)?,
                completion_condition: row.get(9)?,
                status: row.get(10)?,
            })
        },
    )?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

pub fn close_checklist_item(root: &Path, checklist_item_id: i64) -> Result<ChecklistItemOutcome> {
    let mut db = open_existing_project(root)?;
    let conn = db.transaction()?;
    ensure_no_active_source_correction(&conn, "checklist item close")?;
    let project_id = project_id(&conn)?;
    let changed = conn.execute(
        r#"
        update checklist_items
        set status = 'closed'
        where id = ?1
          and project_id = ?2
          and status in ('open', 'blocked')
        "#,
        params![checklist_item_id, project_id],
    )?;
    if changed == 0 {
        bail!("checklist item not found or not closeable");
    }
    conn.commit()?;

    Ok(ChecklistItemOutcome { checklist_item_id })
}

pub fn close_checklist(root: &Path, checklist_id: i64) -> Result<ChecklistOutcome> {
    let mut db = open_existing_project(root)?;
    let conn = db.transaction()?;
    ensure_no_active_source_correction(&conn, "checklist close")?;
    let project_id = project_id(&conn)?;
    let status = conn
        .query_row(
            "select status from checklists where id = ?1 and project_id = ?2",
            params![checklist_id, project_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .context("checklist not found")?;
    if status == "stale" {
        bail!("cannot close stale checklist with checklist close; use stale close checklist");
    }
    if status == "closed" {
        bail!("checklist already closed");
    }

    let open_item_count: i64 = conn.query_row(
        r#"
        select count(*)
        from checklist_items
        where checklist_id = ?1
          and project_id = ?2
          and status in ('open', 'blocked')
        "#,
        params![checklist_id, project_id],
        |row| row.get(0),
    )?;
    if open_item_count > 0 {
        bail!(
            "cannot close checklist; {open_item_count} checklist items are still open or blocked"
        );
    }

    conn.execute(
        "update checklists set status = 'closed' where id = ?1 and project_id = ?2 and status = 'active'",
        params![checklist_id, project_id],
    )?;

    conn.commit()?;
    Ok(ChecklistOutcome { checklist_id })
}

pub fn list_stale_records(root: &Path) -> Result<Vec<StaleRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut records = Vec::new();
    collect_stale_rows(
        &conn,
        project_id,
        "task_derivation",
        r#"
        select td.id, dr.requirement_key
        from task_derivations td
        join design_requirements dr on dr.id = td.design_requirement_id
        where td.project_id = ?1 and td.status = 'stale'
          and not exists (
              select 1
              from acceptance_records ar
              where ar.target_type = 'stale_record'
                and ar.stale_record_type = 'task_derivation'
                and ar.stale_record_id = td.id
                and ar.acceptance_type = 'stale_accepted'
                and ar.status = 'approved'
          )
        order by td.id
        "#,
        &mut records,
    )?;
    collect_stale_rows(
        &conn,
        project_id,
        "checklist",
        r#"
        select c.id, c.title
        from checklists c
        join checklist_items ci on ci.checklist_id = c.id
        join design_requirements dr on dr.id = ci.design_requirement_id
        join design_versions v on v.id = dr.design_version_id
        join design_packages p on p.id = v.design_package_id
        where c.project_id = ?1 and c.status in ('active', 'stale')
          and (c.status = 'stale' or (
            p.current_design_version_id != dr.design_version_id
            and not exists (
              select 1 from design_requirements current_r
              where current_r.design_version_id = p.current_design_version_id
                and current_r.requirement_key = dr.requirement_key
                and current_r.requirement_hash = dr.requirement_hash
                and current_r.status = 'active'
            )
          ))
          and not exists (
              select 1
              from acceptance_records ar
              where ar.target_type = 'stale_record'
                and ar.stale_record_type = 'checklist'
                and ar.stale_record_id = c.id
                and ar.acceptance_type = 'stale_accepted'
                and ar.status = 'approved'
          )
        group by c.id, c.title
        order by c.id
        "#,
        &mut records,
    )?;
    collect_stale_rows(
        &conn,
        project_id,
        "validation_gate",
        r#"
        select vg.id, vg.gate_key
        from validation_gates vg
        where vg.project_id = ?1 and vg.status = 'stale'
          and not exists (
              select 1
              from acceptance_records ar
              where ar.target_type = 'stale_record'
                and ar.stale_record_type = 'validation_gate'
                and ar.stale_record_id = vg.id
                and ar.acceptance_type = 'stale_accepted'
                and ar.status = 'approved'
          )
        order by vg.id
        "#,
        &mut records,
    )?;
    collect_stale_rows(
        &conn,
        project_id,
        "coverage_item",
        r#"
        select c.id, dr.requirement_key
        from coverage_items c
        join design_requirements dr on dr.id = c.design_requirement_id
        join design_versions v on v.id = dr.design_version_id
        join design_packages p on p.id = v.design_package_id
        where c.project_id = ?1 and c.status != 'accepted_out_of_scope'
          and ((c.status = 'stale' and not exists (
            select 1 from coverage_items replacement
            where replacement.project_id=c.project_id
              and replacement.design_requirement_id=c.design_requirement_id
              and replacement.task_id is c.task_id
              and replacement.work_unit_id is c.work_unit_id
              and replacement.status!='stale'
              and replacement.id>c.id
          )) or (
            p.current_design_version_id != dr.design_version_id
            and not exists (
              select 1 from design_requirements current_r
              where current_r.design_version_id = p.current_design_version_id
                and current_r.requirement_key = dr.requirement_key
                and current_r.requirement_hash = dr.requirement_hash
                and current_r.status = 'active'
            )
          ))
          and not exists (
              select 1
              from acceptance_records ar
              where ar.target_type = 'stale_record'
                and ar.stale_record_type = 'coverage_item'
                and ar.stale_record_id = c.id
                and ar.acceptance_type = 'stale_accepted'
                and ar.status = 'approved'
          )
        order by c.id
        "#,
        &mut records,
    )?;
    collect_stale_rows(
        &conn,
        project_id,
        "review_plan",
        r#"
        select rp.id, rp.review_type || ':' || rp.stage
        from review_plans rp
        join design_versions v on v.id = rp.design_version_id
        join design_packages p on p.id = v.design_package_id
        where rp.project_id = ?1
          and rp.status = 'blocked'
          and p.current_design_version_id != rp.design_version_id
          and not exists (
              select 1
              from acceptance_records ar
              where ar.target_type = 'stale_record'
                and ar.stale_record_type = 'review_plan'
                and ar.stale_record_id = rp.id
                and ar.acceptance_type = 'stale_accepted'
                and ar.status = 'approved'
          )
        order by rp.id
        "#,
        &mut records,
    )?;
    Ok(records)
}

pub fn accept_stale_record(
    root: &Path,
    input: StaleRecordDisposition<'_>,
) -> Result<StaleRecordDispositionOutcome> {
    update_stale_record_disposition(root, input, false)
}

pub fn close_stale_record(
    root: &Path,
    input: StaleRecordDisposition<'_>,
) -> Result<StaleRecordDispositionOutcome> {
    update_stale_record_disposition(root, input, true)
}

pub(super) fn update_stale_record_disposition(
    root: &Path,
    input: StaleRecordDisposition<'_>,
    close: bool,
) -> Result<StaleRecordDispositionOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let selected = selected_stale_record_in(&tx, project_id)?
        .context("no unresolved stale record is selected")?;
    if selected != (input.record_type.to_string(), input.record_id) {
        bail!(
            "stale disposition must follow the global tuple; selected {}:{}",
            selected.0,
            selected.1
        );
    }
    let owned_by_active_session: bool = tx.query_row(
        r#"
        select exists(
            select 1 from correction_tokens token
            join correction_sessions session on session.closure_id = token.closure_id
            where session.status = 'active' and token.status = 'pending'
              and token.token_kind = 'transition'
              and token.operation in ('stale-accept', 'stale-close')
              and token.target = ?1
        )
        "#,
        params![format!("{}/{}", input.record_type, input.record_id)],
        |row| row.get(0),
    )?;
    if owned_by_active_session {
        bail!("selected stale record is owned by closure transition apply");
    }
    let outcome = update_stale_record_disposition_in(&tx, project_id, input, close)?;
    tx.commit()?;
    Ok(outcome)
}

pub(crate) fn selected_stale_record_in(
    conn: &rusqlite::Connection,
    project_id: i64,
) -> Result<Option<(String, i64)>> {
    conn.query_row(
        r#"
        select kind, record_id from (
          select 0 kind_rank, 'task_derivation' kind, td.id record_id,
                 r.design_version_id design_id, coalesce(t.work_unit_id, 0) work_id
          from task_derivations td
          join design_requirements r on r.id = td.design_requirement_id
          join tasks t on t.id = td.task_id
          where td.project_id = ?1 and td.status = 'stale'
            and not exists (select 1 from acceptance_records ar where ar.target_type='stale_record' and ar.stale_record_type='task_derivation' and ar.stale_record_id=td.id and ar.status='approved')
          union all
          select 1, 'checklist', c.id, c.design_version_id, c.work_unit_id
          from checklists c
          join checklist_items ci on ci.checklist_id=c.id
          join design_requirements r on r.id=ci.design_requirement_id
          join design_versions v on v.id=r.design_version_id
          join design_packages p on p.id=v.design_package_id
          where c.project_id=?1 and c.status in ('active','stale')
            and (c.status='stale' or (
              p.current_design_version_id != r.design_version_id
              and not exists (
              select 1 from design_requirements current_r
              where current_r.design_version_id=p.current_design_version_id
                and current_r.requirement_key=r.requirement_key
                and current_r.requirement_hash=r.requirement_hash
                and current_r.status='active'
              )
            ))
            and not exists (select 1 from acceptance_records ar where ar.target_type='stale_record' and ar.stale_record_type='checklist' and ar.stale_record_id=c.id and ar.status='approved')
          union all
          select 2, 'validation_gate', vg.id, coalesce(r.design_version_id,0), coalesce(vg.work_unit_id,t.work_unit_id,0)
          from validation_gates vg left join design_requirements r on r.id=vg.design_requirement_id left join tasks t on t.id=vg.task_id
          where vg.project_id=?1 and vg.status='stale'
            and not exists (select 1 from acceptance_records ar where ar.target_type='stale_record' and ar.stale_record_type='validation_gate' and ar.stale_record_id=vg.id and ar.status='approved')
          union all
          select 3, 'coverage_item', c.id, r.design_version_id, coalesce(c.work_unit_id,t.work_unit_id,0)
          from coverage_items c join design_requirements r on r.id=c.design_requirement_id left join tasks t on t.id=c.task_id
          join design_versions v on v.id=r.design_version_id
          join design_packages p on p.id=v.design_package_id
          where c.project_id=?1 and c.status!='accepted_out_of_scope'
            and ((c.status='stale' and not exists (
              select 1 from coverage_items replacement
              where replacement.project_id=c.project_id
                and replacement.design_requirement_id=c.design_requirement_id
                and replacement.task_id is c.task_id
                and replacement.work_unit_id is c.work_unit_id
                and replacement.status!='stale'
                and replacement.id>c.id
            )) or (
              p.current_design_version_id != r.design_version_id
              and not exists (
              select 1 from design_requirements current_r
              where current_r.design_version_id=p.current_design_version_id
                and current_r.requirement_key=r.requirement_key
                and current_r.requirement_hash=r.requirement_hash
                and current_r.status='active'
              )
            ))
            and not exists (select 1 from acceptance_records ar where ar.target_type='stale_record' and ar.stale_record_type='coverage_item' and ar.stale_record_id=c.id and ar.status='approved')
          union all
          select 4, 'review_plan', rp.id, coalesce(rp.design_version_id,0), rp.work_unit_id
          from review_plans rp left join design_versions v on v.id=rp.design_version_id left join design_packages p on p.id=v.design_package_id
          where rp.project_id=?1 and rp.status='blocked' and p.current_design_version_id != rp.design_version_id
            and not exists (select 1 from acceptance_records ar where ar.target_type='stale_record' and ar.stale_record_type='review_plan' and ar.stale_record_id=rp.id and ar.status='approved')
        ) ordered
        order by kind_rank, design_id, work_id, record_id limit 1
        "#,
        params![project_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .map_err(Into::into)
}

pub(crate) fn update_stale_record_disposition_in(
    conn: &rusqlite::Connection,
    project_id: i64,
    input: StaleRecordDisposition<'_>,
    close: bool,
) -> Result<StaleRecordDispositionOutcome> {
    validate_stale_record_type(input.record_type)?;
    if close {
        validate_closeable_stale_record_type(input.record_type)?;
    }
    let label = ensure_stale_record(conn, project_id, input.record_type, input.record_id)?;

    conn.execute(
        r#"
        insert into authority_events(
            project_id, event_type, source, text_or_summary, scope, precedence,
            status, created_at
        )
        values (?1, 'user_instruction', 'stale disposition', ?2, 'project', 100, 'active', current_timestamp)
        "#,
        params![
            project_id,
            format!(
                "{} stale {}:{}: {}",
                if close { "closed" } else { "accepted" },
                input.record_type,
                input.record_id,
                input.reason
            ),
        ],
    )?;
    let authority_event_id = conn.last_insert_rowid();

    conn.execute(
        r#"
        insert into acceptance_records(
            project_id, target_type, stale_record_type, stale_record_id,
            acceptance_type, reason, scope, created_by, status,
            approved_by_authority_event_id, approved_at, created_at, review_impact
        )
        values (
            ?1, 'stale_record', ?2, ?3, 'stale_accepted', ?4, ?5,
            'user', 'approved', ?6, current_timestamp, current_timestamp,
            'stale record disposition recorded through stale command'
        )
        "#,
        params![
            project_id,
            input.record_type,
            input.record_id,
            input.reason,
            format!("stale:{}:{}", input.record_type, input.record_id),
            authority_event_id,
        ],
    )?;
    let acceptance_record_id = conn.last_insert_rowid();

    let status = if close {
        close_stale_record_row(conn, project_id, input.record_type, input.record_id)?;
        "closed"
    } else {
        "stale_accepted"
    };

    Ok(StaleRecordDispositionOutcome {
        record_type: input.record_type.to_string(),
        record_id: input.record_id,
        label,
        status: status.to_string(),
        acceptance_record_id,
        authority_event_id,
    })
}

pub(super) fn ensure_no_active_source_correction(
    conn: &rusqlite::Connection,
    operation: &str,
) -> Result<()> {
    ensure_unscoped_mutation_allowed(conn, operation)
}

pub(super) fn validate_stale_record_type(record_type: &str) -> Result<()> {
    match record_type {
        "task_derivation" | "checklist" | "validation_gate" | "coverage_item" | "review_plan" => {
            Ok(())
        }
        _ => bail!(
            "stale record type must be task_derivation, checklist, validation_gate, coverage_item, or review_plan"
        ),
    }
}

pub(super) fn validate_closeable_stale_record_type(record_type: &str) -> Result<()> {
    match record_type {
        "task_derivation" | "checklist" | "validation_gate" => Ok(()),
        "coverage_item" | "review_plan" => bail!(
            "stale {record_type} cannot be closed; use stale accept {record_type} <id> --reason <reason>"
        ),
        _ => validate_stale_record_type(record_type),
    }
}

pub(super) fn ensure_stale_record(
    conn: &rusqlite::Connection,
    project_id: i64,
    record_type: &str,
    record_id: i64,
) -> Result<String> {
    let sql = match record_type {
        "task_derivation" => {
            r#"
            select dr.requirement_key
            from task_derivations td
            join design_requirements dr on dr.id = td.design_requirement_id
            where td.project_id = ?1 and td.id = ?2 and td.status = 'stale'
            "#
        }
        "checklist" => {
            r#"
            select checklists.title
            from checklists
            join checklist_items ci on ci.checklist_id = checklists.id
            join design_requirements dr on dr.id = ci.design_requirement_id
            join design_versions v on v.id = dr.design_version_id
            join design_packages p on p.id = v.design_package_id
            where checklists.project_id = ?1 and checklists.id = ?2
              and checklists.status in ('active', 'stale')
              and (checklists.status = 'stale' or (
                p.current_design_version_id != dr.design_version_id
                and not exists (
                select 1 from design_requirements current_r
                where current_r.design_version_id = p.current_design_version_id
                  and current_r.requirement_key = dr.requirement_key
                  and current_r.requirement_hash = dr.requirement_hash
                  and current_r.status = 'active'
                )
              ))
            limit 1
            "#
        }
        "validation_gate" => {
            r#"
            select gate_key
            from validation_gates
            where project_id = ?1 and id = ?2 and status = 'stale'
            "#
        }
        "coverage_item" => {
            r#"
            select dr.requirement_key
            from coverage_items c
            join design_requirements dr on dr.id = c.design_requirement_id
            join design_versions v on v.id = dr.design_version_id
            join design_packages p on p.id = v.design_package_id
            where c.project_id = ?1 and c.id = ?2
              and c.status != 'accepted_out_of_scope'
              and ((c.status = 'stale' and not exists (
                select 1 from coverage_items replacement
                where replacement.project_id=c.project_id
                  and replacement.design_requirement_id=c.design_requirement_id
                  and replacement.task_id is c.task_id
                  and replacement.work_unit_id is c.work_unit_id
                  and replacement.status!='stale'
                  and replacement.id>c.id
              )) or (
                p.current_design_version_id != dr.design_version_id
                and not exists (
                select 1 from design_requirements current_r
                where current_r.design_version_id = p.current_design_version_id
                  and current_r.requirement_key = dr.requirement_key
                  and current_r.requirement_hash = dr.requirement_hash
                  and current_r.status = 'active'
                )
              ))
            "#
        }
        "review_plan" => {
            r#"
            select rp.review_type || ':' || rp.stage
            from review_plans rp
            join design_versions v on v.id = rp.design_version_id
            join design_packages p on p.id = v.design_package_id
            where rp.project_id = ?1
              and rp.id = ?2
              and rp.status = 'blocked'
              and p.current_design_version_id != rp.design_version_id
            "#
        }
        _ => unreachable!("validated stale record type"),
    };
    conn.query_row(sql, params![project_id, record_id], |row| row.get(0))
        .optional()?
        .with_context(|| format!("stale {record_type} record not found: {record_id}"))
}

pub(super) fn close_stale_record_row(
    conn: &rusqlite::Connection,
    project_id: i64,
    record_type: &str,
    record_id: i64,
) -> Result<()> {
    let (table, id_column) = match record_type {
        "task_derivation" => ("task_derivations", "id"),
        "checklist" => ("checklists", "id"),
        "validation_gate" => ("validation_gates", "id"),
        _ => unreachable!("validated closeable stale record type"),
    };
    let changed = conn.execute(
        &format!(
            "update {table} set status = 'closed' where project_id = ?1 and {id_column} = ?2 and status = 'stale'"
        ),
        params![project_id, record_id],
    )?;
    if changed == 0 {
        bail!("stale {record_type} record not found or not closeable: {record_id}");
    }
    Ok(())
}

pub fn list_validation_gate_context(
    root: &Path,
    input: ValidationGateContextQuery,
) -> Result<Vec<ValidationGateContextRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select
            vg.id, vg.gate_key, r.requirement_key, vg.task_id, vg.status,
            latest.id, latest.command_usage_id, latest.repository_snapshot_id,
            latest.result, latest.artifact_path, latest.notes
        from validation_gates vg
        join design_requirements r on r.id = vg.design_requirement_id
        left join tasks t on t.id = vg.task_id
        left join validation_runs latest on latest.id = (
            select vr.id
            from validation_runs vr
            where vr.validation_gate_id = vg.id
            order by vr.id desc
            limit 1
        )
        where vg.project_id = ?1
          and r.design_version_id = ?2
          and (?3 is null or coalesce(vg.work_unit_id, t.work_unit_id) = ?3)
        order by r.requirement_key, vg.id
        "#,
    )?;
    let rows = stmt.query_map(
        params![project_id, input.design_version_id, input.work_unit_id],
        |row| {
            Ok(ValidationGateContextRecord {
                id: row.get(0)?,
                gate_key: row.get(1)?,
                requirement_key: row.get(2)?,
                task_id: row.get(3)?,
                status: row.get(4)?,
                latest_run_id: row.get(5)?,
                latest_command_usage_id: row.get(6)?,
                latest_repository_snapshot_id: row.get(7)?,
                latest_result: row.get(8)?,
                latest_artifact_path: row.get(9)?,
                latest_notes: row.get(10)?,
            })
        },
    )?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

pub(super) fn collect_stale_rows(
    conn: &rusqlite::Connection,
    project_id: i64,
    record_type: &str,
    sql: &str,
    records: &mut Vec<StaleRecord>,
) -> Result<()> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![project_id], |row| {
        Ok(StaleRecord {
            record_type: record_type.to_string(),
            id: row.get(0)?,
            label: row.get(1)?,
        })
    })?;
    for row in rows {
        records.push(row?);
    }
    Ok(())
}
