use anyhow::Result;
use rusqlite::{Connection, params};

use super::*;

pub(super) fn trace_resume_counts(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<TraceResumeCounts> {
    Ok(TraceResumeCounts {
        stale_design_records: count_stale_design_records_for_work(conn, work_unit_id)?,
        stale_task_derivations: count_stale_task_derivations_for_work(conn, work_unit_id)?,
        stale_checklists: count_stale_checklists_for_work(conn, work_unit_id)?,
        stale_selected_gates: count_stale_selected_gates_for_work(conn, work_unit_id)?,
        stale_coverage_items: count_stale_coverage_items_for_work(conn, work_unit_id)?,
    })
}

pub(crate) fn has_stale_design_state_for_work(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<bool> {
    let counts = trace_resume_counts(conn, work_unit_id)?;
    Ok(counts.stale_design_records > 0
        || counts.stale_task_derivations > 0
        || counts.stale_checklists > 0
        || counts.stale_selected_gates > 0
        || counts.stale_coverage_items > 0)
}

pub(super) fn close_trace_state(conn: &Connection, work_unit_id: i64) -> Result<CloseTraceState> {
    Ok(CloseTraceState {
        active_requirement_count: count_active_requirements_for_work(conn, work_unit_id)?,
        derived_task_count: count_design_derived_tasks_for_work(conn, work_unit_id)?,
        missing_evidence_count: count_closed_derived_tasks_missing_evidence_for_work(
            conn,
            work_unit_id,
        )?,
        missing_coverage_count: count_closed_derived_tasks_missing_coverage_for_work(
            conn,
            work_unit_id,
        )?,
        missing_requirement_coverage_count: count_active_requirements_missing_coverage_for_work(
            conn,
            work_unit_id,
        )?,
        missing_validation_gate_count: count_derived_tasks_missing_selected_gate_for_work(
            conn,
            work_unit_id,
        )?,
        open_checklist_item_count: count_open_checklist_items_for_work(conn, work_unit_id)?,
        active_checklist_count: count_active_checklists_for_work(conn, work_unit_id)?,
    })
}

pub(super) fn missing_required_close_review_types(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<Vec<String>> {
    let mut missing = Vec::new();
    for design_version_id in design_versions_for_work(conn, work_unit_id)? {
        for review_type in ["design_implementation_diff", "implementation_review"] {
            let count: i64 = conn.query_row(
                r#"
                select count(*)
                from review_plans
                where work_unit_id = ?1
                  and design_version_id = ?2
                  and stage = 'close-ready'
                  and review_type = ?3
                  and required = 1
                  and not exists (
                    select 1
                    from work_phase_review_targets wprt
                    where wprt.review_plan_id = review_plans.id
                  )
                "#,
                params![work_unit_id, design_version_id, review_type],
                |row| row.get(0),
            )?;
            if count == 0 {
                missing.push(format!("{review_type}@design:{design_version_id}"));
            }
        }
    }
    Ok(missing)
}

pub(super) fn design_versions_for_work(conn: &Connection, work_unit_id: i64) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        r#"
        select distinct r.design_version_id
        from task_derivations td
        join tasks t on t.id = td.task_id
        join design_requirements r on r.id = td.design_requirement_id
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where t.work_unit_id = ?1
          and td.status in ('active', 'stale')
          and t.status = 'closed'
          and p.current_design_version_id = r.design_version_id
        order by r.design_version_id
        "#,
    )?;
    let rows = stmt.query_map(params![work_unit_id], |row| row.get(0))?;
    let mut design_version_ids = Vec::new();
    for row in rows {
        design_version_ids.push(row?);
    }
    Ok(design_version_ids)
}

pub(super) fn count_design_derived_tasks_for_work(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(distinct td.task_id)
        from task_derivations td
        join tasks t on t.id = td.task_id
        join design_requirements r on r.id = td.design_requirement_id
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where t.work_unit_id = ?1
          and td.status in ('active', 'stale')
          and p.current_design_version_id = r.design_version_id
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_active_requirements_for_work(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        with relevant_requirements as (
            select distinct r.id
            from task_derivations td
            join tasks t on t.id = td.task_id
            join design_requirements r on r.id = td.design_requirement_id
            join design_versions v on v.id = r.design_version_id
            join design_packages p on p.id = v.design_package_id
            where t.work_unit_id = ?1 and td.status = 'active'
              and p.current_design_version_id = r.design_version_id
            union
            select distinct r.id
            from current_task_validation_gates vg
            join design_requirements r on r.id = vg.design_requirement_id
            join design_versions v on v.id = r.design_version_id
            join design_packages p on p.id = v.design_package_id
            left join tasks t on t.id = vg.task_id
            where coalesce(vg.work_unit_id, t.work_unit_id) = ?1
              and p.current_design_version_id = r.design_version_id
        )
        select count(*)
        from design_requirements r
        join relevant_requirements rr on rr.id = r.id
        where r.status = 'active'
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_active_requirements_missing_coverage_for_work(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        with relevant_requirements as (
            select distinct r.id
            from task_derivations td
            join tasks t on t.id = td.task_id
            join design_requirements r on r.id = td.design_requirement_id
            join design_versions v on v.id = r.design_version_id
            join design_packages p on p.id = v.design_package_id
            where t.work_unit_id = ?1 and td.status = 'active'
              and p.current_design_version_id = r.design_version_id
            union
            select distinct r.id
            from current_task_validation_gates vg
            join design_requirements r on r.id = vg.design_requirement_id
            join design_versions v on v.id = r.design_version_id
            join design_packages p on p.id = v.design_package_id
            left join tasks t on t.id = vg.task_id
            where coalesce(vg.work_unit_id, t.work_unit_id) = ?1
              and p.current_design_version_id = r.design_version_id
        )
        select count(*)
        from design_requirements r
        join relevant_requirements rr on rr.id = r.id
        where r.status = 'active'
          and not exists (
            select 1
            from coverage_items c
            join design_requirements covered_r on covered_r.id = c.design_requirement_id
            join design_versions covered_v on covered_v.id = covered_r.design_version_id
            join design_versions required_v on required_v.id = r.design_version_id
            left join tasks ct on ct.id = c.task_id
            where (
                c.design_requirement_id = r.id
                or (
                  covered_v.design_package_id = required_v.design_package_id
                  and covered_r.requirement_key = r.requirement_key
                  and covered_r.requirement_hash = r.requirement_hash
                )
              )
              and (
                ct.work_unit_id = ?1
                or (c.task_id is null and c.work_unit_id = ?1)
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
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_derived_tasks_missing_selected_gate_for_work(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join tasks t on t.id = td.task_id
        join design_requirements r on r.id = td.design_requirement_id
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where t.work_unit_id = ?1
          and td.status in ('active', 'stale')
          and t.status = 'closed'
          and p.current_design_version_id = r.design_version_id
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'stale_record'
              and ar.stale_record_type = 'task_derivation'
              and ar.stale_record_id = td.id
              and ar.acceptance_type = 'stale_accepted'
              and ar.status = 'approved'
          )
          and not exists (
            select 1
            from current_task_validation_gates vg
            where (
                vg.design_requirement_id = td.design_requirement_id
                or exists (
                    select 1
                    from design_requirements current_r
                    where current_r.id = vg.design_requirement_id
                      and current_r.design_version_id = p.current_design_version_id
                      and current_r.requirement_key = r.requirement_key
                      and current_r.requirement_hash = r.requirement_hash
                      and current_r.status = 'active'
                )
              )
              and vg.task_id = td.task_id
              and vg.selected_before_edit = 1
          )
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn missing_selected_gate_details_for_work(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        r#"
        select
            td.id,
            td.task_id,
            t.status,
            r.requirement_key,
            r.design_version_id,
            p.current_design_version_id
        from task_derivations td
        join tasks t on t.id = td.task_id
        join design_requirements r on r.id = td.design_requirement_id
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where t.work_unit_id = ?1
          and td.status in ('active', 'stale')
          and t.status = 'closed'
          and p.current_design_version_id = r.design_version_id
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'stale_record'
              and ar.stale_record_type = 'task_derivation'
              and ar.stale_record_id = td.id
              and ar.acceptance_type = 'stale_accepted'
              and ar.status = 'approved'
          )
          and not exists (
            select 1
            from current_task_validation_gates vg
            where (
                vg.design_requirement_id = td.design_requirement_id
                or exists (
                    select 1
                    from design_requirements current_r
                    where current_r.id = vg.design_requirement_id
                      and current_r.design_version_id = p.current_design_version_id
                      and current_r.requirement_key = r.requirement_key
                      and current_r.requirement_hash = r.requirement_hash
                      and current_r.status = 'active'
                )
              )
              and vg.task_id = td.task_id
              and vg.selected_before_edit = 1
          )
        order by td.id
        "#,
    )?;
    let rows = stmt.query_map(params![work_unit_id], |row| {
        let derivation_id: i64 = row.get(0)?;
        let task_id: i64 = row.get(1)?;
        let task_status: String = row.get(2)?;
        let requirement_key: String = row.get(3)?;
        let design_version_id: i64 = row.get(4)?;
        let current_design_version_id: Option<i64> = row.get(5)?;
        Ok(format!(
            "task_derivation:{derivation_id} task:{task_id} task_status:{task_status} requirement:{requirement_key} design:{design_version_id} current_design:{}",
            format_optional_id(current_design_version_id)
        ))
    })?;
    collect_rows(rows)
}

pub(super) fn count_open_checklist_items_for_work(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from checklist_items ci
        join checklists c on c.id = ci.checklist_id
        where c.work_unit_id = ?1
          and c.status = 'active'
          and ci.status in ('open', 'blocked')
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_active_checklists_for_work(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from checklists
        where work_unit_id = ?1
          and status = 'active'
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_closed_derived_tasks_missing_evidence_for_work(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join tasks t on t.id = td.task_id
        join design_requirements r on r.id = td.design_requirement_id
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where t.work_unit_id = ?1
          and td.status in ('active', 'stale')
          and t.status = 'closed'
          and p.current_design_version_id = r.design_version_id
          and not exists (
            select 1
            from implementation_evidence e
            where e.task_id = td.task_id
              and e.design_requirement_id = td.design_requirement_id
          )
          and not exists (
            select 1
            from correction_completion_inheritance_sources inheritance
            join valid_completion_inheritance_sources valid on valid.id=inheritance.id
            where inheritance.current_requirement_id=r.id
              and inheritance.canonical_task_id=t.id
              and exists (
                select 1
                from correction_completion_inheritance_evidence mapped
                where mapped.inheritance_source_id=inheritance.id
                  and mapped.evidence_kind='implementation_evidence'
              )
          )
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_closed_derived_tasks_missing_coverage_for_work(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join tasks t on t.id = td.task_id
        join design_requirements required_r on required_r.id = td.design_requirement_id
        join design_versions required_v on required_v.id = required_r.design_version_id
        join design_packages p on p.id = required_v.design_package_id
        where t.work_unit_id = ?1
          and td.status = 'active'
          and t.status = 'closed'
          and p.current_design_version_id = required_r.design_version_id
          and not exists (
            select 1
            from coverage_items c
            join design_requirements covered_r on covered_r.id = c.design_requirement_id
            join design_versions covered_v on covered_v.id = covered_r.design_version_id
            where (
                c.design_requirement_id = td.design_requirement_id
                or (
                  covered_v.design_package_id = required_v.design_package_id
                  and covered_r.requirement_key = required_r.requirement_key
                  and covered_r.requirement_hash = required_r.requirement_hash
                )
              )
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
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_stale_design_records_for_work(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(distinct r.id)
        from task_derivations td
        join tasks t on t.id = td.task_id
        join design_requirements r on r.id = td.design_requirement_id
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where t.work_unit_id = ?1
          and td.status = 'active'
          and p.current_design_version_id != r.design_version_id
          and not exists (
            select 1
            from design_requirements current_r
            where current_r.design_version_id = p.current_design_version_id
              and current_r.requirement_key = r.requirement_key
              and current_r.requirement_hash = r.requirement_hash
              and current_r.status = 'active'
          )
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'stale_record'
              and ar.stale_record_type = 'task_derivation'
              and ar.stale_record_id = td.id
              and ar.acceptance_type = 'stale_accepted'
              and ar.status = 'approved'
          )
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_stale_task_derivations_for_work(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join tasks t on t.id = td.task_id
        join design_requirements r on r.id = td.design_requirement_id
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where t.work_unit_id = ?1
          and td.status = 'active'
          and p.current_design_version_id != r.design_version_id
          and not exists (
            select 1
            from design_requirements current_r
            where current_r.design_version_id = p.current_design_version_id
              and current_r.requirement_key = r.requirement_key
              and current_r.requirement_hash = r.requirement_hash
              and current_r.status = 'active'
          )
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'stale_record'
              and ar.stale_record_type = 'task_derivation'
              and ar.stale_record_id = td.id
              and ar.acceptance_type = 'stale_accepted'
              and ar.status = 'approved'
          )
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_stale_checklists_for_work(conn: &Connection, work_unit_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        select count(distinct c.id)
        from checklists c
        join checklist_items ci on ci.checklist_id = c.id
        join design_requirements r on r.id = ci.design_requirement_id
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where c.work_unit_id = ?1
          and c.status in ('active', 'stale')
          and p.current_design_version_id != r.design_version_id
          and not exists (
            select 1
            from design_requirements current_r
            where current_r.design_version_id = p.current_design_version_id
              and current_r.requirement_key = r.requirement_key
              and current_r.requirement_hash = r.requirement_hash
              and current_r.status = 'active'
          )
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'stale_record'
              and ar.stale_record_type = 'checklist'
              and ar.stale_record_id = c.id
              and ar.acceptance_type = 'stale_accepted'
              and ar.status = 'approved'
          )
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_stale_selected_gates_for_work(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from current_task_validation_gates vg
        join validation_gate_templates gt on gt.id = vg.template_id
        join design_requirements r on r.id = vg.design_requirement_id
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        left join tasks t on t.id = vg.task_id
        where coalesce(vg.work_unit_id, t.work_unit_id) = ?1
          and vg.status in ('active', 'stale')
          and (p.current_design_version_id != r.design_version_id
               or p.current_design_version_id != gt.design_version_id)
          and (
            not exists (
              select 1
              from design_requirements current_r
              where current_r.design_version_id = p.current_design_version_id
                and current_r.requirement_key = r.requirement_key
                and current_r.requirement_hash = r.requirement_hash
                and current_r.status = 'active'
            )
            or not exists (
              select 1
              from validation_gate_templates current_gt
              where current_gt.design_version_id = p.current_design_version_id
                and current_gt.gate_key = gt.gate_key
                and current_gt.gate_hash = gt.gate_hash
                and current_gt.status = 'active'
            )
          )
          and not exists (
            select 1
            from acceptance_records ar
            where (
                (
                  ar.target_type = 'validation_gate'
                  and ar.validation_gate_id = vg.id
                )
                or (
                  ar.target_type = 'stale_record'
                  and ar.stale_record_type = 'validation_gate'
                  and ar.stale_record_id = vg.id
                )
              )
              and ar.acceptance_type = 'stale_accepted'
              and ar.status = 'approved'
          )
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_stale_coverage_items_for_work(
    conn: &Connection,
    work_unit_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from coverage_items c
        join design_requirements r on r.id = c.design_requirement_id
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        left join tasks t on t.id = c.task_id
        where coalesce(c.work_unit_id, t.work_unit_id) = ?1
          and c.status != 'accepted_out_of_scope'
          and p.current_design_version_id != r.design_version_id
          and not exists (
            select 1
            from design_requirements current_r
            where current_r.design_version_id = p.current_design_version_id
              and current_r.requirement_key = r.requirement_key
              and current_r.requirement_hash = r.requirement_hash
              and current_r.status = 'active'
          )
          and not exists (
            select 1
            from acceptance_records ar
            where (
                (
                  ar.target_type = 'coverage_item'
                  and ar.coverage_item_id = c.id
                )
                or (
                  ar.target_type = 'stale_record'
                  and ar.stale_record_type = 'coverage_item'
                  and ar.stale_record_id = c.id
                )
              )
              and ar.acceptance_type = 'stale_accepted'
              and ar.status = 'approved'
          )
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}
