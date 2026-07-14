use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use crate::db::StoredActivation;
use crate::review_context::review_plan_has_clean_context_run;

use super::{resume_validation::*, *};

pub(super) fn repository_close_state(
    conn: &Connection,
    active: &StoredActivation,
) -> Result<RepositoryCloseState> {
    let repository_count = conn.query_row(
        "select count(*) from repositories where project_id = ?1",
        params![active.project_id],
        |row| row.get::<_, i64>(0),
    )?;
    let active_snapshot_count = conn.query_row(
        r#"
        select count(distinct s.repository_id)
        from repository_snapshots s
        join repositories r on r.id = s.repository_id
        where r.project_id = ?1 and s.work_unit_activation_id = ?2
        "#,
        params![active.project_id, active.activation_id],
        |row| row.get::<_, i64>(0),
    )?;
    let mut stmt = conn.prepare(
        r#"
        select s.id, s.repository_id, s.is_clean
        from repository_snapshots s
        join repositories r on r.id = s.repository_id
        where r.project_id = ?1
          and s.work_unit_activation_id = ?2
          and s.id = (
              select max(inner_s.id)
              from repository_snapshots inner_s
              where inner_s.repository_id = s.repository_id
                and inner_s.work_unit_activation_id = ?2
          )
        "#,
    )?;
    let snapshots = stmt.query_map(params![active.project_id, active.activation_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;
    let mut unclassified_dirty_state_count = 0;
    let mut missing_comparison_count = 0;
    let mut unclassified_comparison_count = 0;
    for snapshot in snapshots {
        let (repository_snapshot_id, repository_id, is_clean) = snapshot?;
        if is_clean == 0
            && !repository_snapshot_dirty_state_classified(conn, repository_snapshot_id)?
        {
            unclassified_dirty_state_count += 1;
        }
        if let Some(base_snapshot_id) = previous_repository_snapshot(
            conn,
            repository_id,
            repository_snapshot_id,
            active.activation_id,
        )? {
            match close_repository_snapshot_comparison(
                conn,
                base_snapshot_id,
                repository_snapshot_id,
            )?
            .as_deref()
            {
                Some("same" | "changed_classified") => {}
                Some("changed_unclassified") | Some(_) => unclassified_comparison_count += 1,
                None => missing_comparison_count += 1,
            }
        }
    }

    Ok(RepositoryCloseState {
        repository_count,
        missing_snapshot_count: repository_count.saturating_sub(active_snapshot_count),
        unclassified_dirty_state_count,
        missing_comparison_count,
        unclassified_comparison_count,
    })
}

pub(super) fn previous_repository_snapshot(
    conn: &Connection,
    repository_id: i64,
    repository_snapshot_id: i64,
    active_activation_id: i64,
) -> Result<Option<i64>> {
    conn.query_row(
        r#"
        select max(s.id)
        from repository_snapshots s
        where s.repository_id = ?1
          and s.id < ?2
          and (
              s.work_unit_activation_id is null
              or s.work_unit_activation_id < ?3
          )
        "#,
        params![repository_id, repository_snapshot_id, active_activation_id],
        |row| row.get::<_, Option<i64>>(0),
    )
    .map_err(Into::into)
}

pub(super) fn close_repository_snapshot_comparison(
    conn: &Connection,
    base_snapshot_id: i64,
    current_snapshot_id: i64,
) -> Result<Option<String>> {
    conn.query_row(
        r#"
        select result
        from repository_snapshot_comparisons
        where base_repository_snapshot_id = ?1
          and current_repository_snapshot_id = ?2
          and comparison_type = 'close'
        order by id desc
        limit 1
        "#,
        params![base_snapshot_id, current_snapshot_id],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

pub(super) fn review_plan_stage_state(
    conn: &Connection,
    work_unit_id: i64,
    stage: &str,
) -> Result<ReviewPlanStageState> {
    let mut stmt = conn.prepare(
        r#"
        select id, status, review_type, design_version_id, work_unit_id
        from review_plans
        where work_unit_id = ?1
          and stage = ?2
          and required = 1
          and (
            design_version_id is null
            or exists (
              select 1
              from task_derivations td
              join design_requirements r on r.id=td.design_requirement_id
              join design_versions v on v.id=r.design_version_id
              join design_packages p on p.id=v.design_package_id
              join tasks t on t.id=td.task_id
              where r.design_version_id=review_plans.design_version_id
                and t.work_unit_id=review_plans.work_unit_id
                and td.status in ('active','stale')
                and p.current_design_version_id=r.design_version_id
            )
          )
        order by id
        "#,
    )?;
    let rows = stmt.query_map(params![work_unit_id, stage], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<i64>>(3)?,
            row.get::<_, Option<i64>>(4)?,
        ))
    })?;
    let mut state = ReviewPlanStageState::default();
    for row in rows {
        let (review_plan_id, status, review_type, design_version_id, plan_work_unit_id) = row?;
        state.required_plan_count += 1;
        let accepted = review_plan_accepted(conn, review_plan_id)?;
        if status != "clean" && !accepted {
            state.incomplete_required_plan_count += 1;
        }
        if let Some(kind) = review_context_kind_for_plan(stage, &review_type)
            && design_version_id.is_some()
            && !accepted
            && !review_plan_has_clean_context_run(
                conn,
                review_plan_id,
                kind,
                design_version_id,
                plan_work_unit_id,
            )?
        {
            state.missing_context_run_count += 1;
        }
        if !accepted {
            state.stale_target_count += stale_review_plan_target_count(conn, review_plan_id)?;
        }
    }
    Ok(state)
}

pub(super) fn review_plan_blocker_details_for_stage(
    conn: &Connection,
    work_unit_id: i64,
    stage: &str,
) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        r#"
        select id, status, review_type, design_version_id, work_unit_id
        from review_plans
        where work_unit_id = ?1
          and stage = ?2
          and required = 1
          and (
            design_version_id is null
            or exists (
              select 1
              from task_derivations td
              join design_requirements r on r.id=td.design_requirement_id
              join design_versions v on v.id=r.design_version_id
              join design_packages p on p.id=v.design_package_id
              join tasks t on t.id=td.task_id
              where r.design_version_id=review_plans.design_version_id
                and t.work_unit_id=review_plans.work_unit_id
                and td.status in ('active','stale')
                and p.current_design_version_id=r.design_version_id
            )
          )
        order by id
        "#,
    )?;
    let rows = stmt.query_map(params![work_unit_id, stage], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<i64>>(3)?,
            row.get::<_, Option<i64>>(4)?,
        ))
    })?;
    let mut details = Vec::new();
    for row in rows {
        let (review_plan_id, status, review_type, design_version_id, plan_work_unit_id) = row?;
        if review_plan_accepted(conn, review_plan_id)? {
            continue;
        }
        if status != "clean" {
            details.push(format!(
                "review_plan:{review_plan_id} type:{review_type} status:{status} design:{} incomplete",
                format_optional_id(design_version_id)
            ));
        }
        if let Some(kind) = review_context_kind_for_plan(stage, &review_type)
            && design_version_id.is_some()
            && !review_plan_has_clean_context_run(
                conn,
                review_plan_id,
                kind,
                design_version_id,
                plan_work_unit_id,
            )?
        {
            details.push(format!(
                "review_plan:{review_plan_id} type:{review_type} missing_context:{kind} context_ref:review-context:{kind}:design={}:work={}",
                format_optional_id(design_version_id),
                format_optional_id(plan_work_unit_id)
            ));
        }
        let stale_targets = stale_review_plan_target_count(conn, review_plan_id)?;
        if stale_targets > 0 {
            details.push(format!(
                "review_plan:{review_plan_id} type:{review_type} stale_targets:{stale_targets}"
            ));
        }
    }
    Ok(details)
}

pub(super) fn review_plan_accepted(conn: &Connection, review_plan_id: i64) -> Result<bool> {
    conn.query_row(
        r#"
        select exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'review_plan'
              and ar.review_plan_id = ?1
              and ar.status = 'approved'
              and ar.acceptance_type in ('explicit_exception', 'stale_accepted')
        )
        "#,
        params![review_plan_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn review_context_kind_for_plan(stage: &str, review_type: &str) -> Option<&'static str> {
    match (stage, review_type) {
        ("close-ready", "design_implementation_diff") => Some("design-implementation-diff"),
        ("close-ready", "implementation_review") => Some("implementation-review"),
        _ => None,
    }
}

pub(super) fn stale_review_plan_target_count(
    conn: &Connection,
    review_plan_id: i64,
) -> Result<i64> {
    let mut stmt = conn.prepare(
        r#"
        select target_type, design_version_id, design_requirement_id, repository_snapshot_id
        from review_plan_targets
        where review_plan_id = ?1
        "#,
    )?;
    let rows = stmt.query_map(params![review_plan_id], |row| {
        Ok(ReviewPlanTargetForResume {
            target_type: row.get(0)?,
            design_version_id: row.get(1)?,
            design_requirement_id: row.get(2)?,
            repository_snapshot_id: row.get(3)?,
        })
    })?;
    let mut stale = 0;
    for row in rows {
        if review_plan_target_stale(conn, row?)? {
            stale += 1;
        }
    }
    Ok(stale)
}

pub(super) fn review_plan_target_stale(
    conn: &Connection,
    target: ReviewPlanTargetForResume,
) -> Result<bool> {
    match target.target_type.as_str() {
        "design_version" => match target.design_version_id {
            Some(design_version_id) => design_version_stale(conn, design_version_id),
            None => Ok(true),
        },
        "design_requirement" => match target.design_requirement_id {
            Some(design_requirement_id) => design_requirement_stale(conn, design_requirement_id),
            None => Ok(true),
        },
        "repository_snapshot" => match target.repository_snapshot_id {
            Some(repository_snapshot_id) => {
                repository_snapshot_target_stale(conn, repository_snapshot_id)
            }
            None => Ok(true),
        },
        _ => Ok(false),
    }
}

pub(super) fn design_version_stale(conn: &Connection, design_version_id: i64) -> Result<bool> {
    let current_id = conn
        .query_row(
            r#"
            select p.current_design_version_id
            from design_versions v
            join design_packages p on p.id = v.design_package_id
            where v.id = ?1
            "#,
            params![design_version_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    Ok(current_id != Some(design_version_id))
}

pub(super) fn design_requirement_stale(
    conn: &Connection,
    design_requirement_id: i64,
) -> Result<bool> {
    conn.query_row(
        r#"
        select not exists (
            select 1
            from design_requirements old_r
            join design_versions old_v on old_v.id = old_r.design_version_id
            join design_packages p on p.id = old_v.design_package_id
            join design_requirements current_r
              on current_r.design_version_id = p.current_design_version_id
             and current_r.requirement_key = old_r.requirement_key
             and current_r.requirement_hash = old_r.requirement_hash
             and current_r.status = 'active'
            where old_r.id = ?1
        )
        "#,
        params![design_requirement_id],
        |row| row.get::<_, bool>(0),
    )
    .map_err(Into::into)
}

pub(super) fn repository_snapshot_target_stale(
    conn: &Connection,
    repository_snapshot_id: i64,
) -> Result<bool> {
    let Some((repository_id, latest_snapshot_id)) = conn
        .query_row(
            r#"
            select s.repository_id, (
                select max(current_s.id)
                from repository_snapshots current_s
                where current_s.repository_id = s.repository_id
            )
            from repository_snapshots s
            where s.id = ?1
            "#,
            params![repository_snapshot_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?
    else {
        return Ok(true);
    };
    let _ = repository_id;
    if latest_snapshot_id == repository_snapshot_id {
        return Ok(false);
    }
    let classified_comparison = conn
        .query_row(
            r#"
            select 1
            from repository_snapshot_comparisons
            where base_repository_snapshot_id = ?1
              and current_repository_snapshot_id = ?2
              and comparison_type = 'resume'
              and result in ('same', 'changed_classified')
            order by id desc
            limit 1
            "#,
            params![repository_snapshot_id, latest_snapshot_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok(!classified_comparison)
}

pub(super) fn open_assumption_invalidations(conn: &Connection, work_unit_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from work_unit_dependencies
        where work_unit_id = ?1
          and dependency_type = 'invalidates_assumption'
          and status = 'open'
        "#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}
