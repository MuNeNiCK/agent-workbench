use std::path::Path;

use anyhow::{Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{open_existing_project, project_id};
use crate::review_context::{
    PlanReviewOwnerState, current_decomposition_plan_review_target,
    required_plans_missing_context_count, resolve_decomposition_plan_review_owner,
};

use super::*;

pub fn implementation_ready(
    root: &Path,
    input: ImplementationReadyCheck,
) -> Result<ImplementationReadyOutcome> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut items = Vec::new();
    let Some(version) = resolve_design_version(&conn, project_id, input.design_version_id)? else {
        items.push(ImplementationReadyItem::fail(
            "design_version_exists",
            Some("import a design package first".to_string()),
        ));
        return Ok(ImplementationReadyOutcome::blocked(
            input.design_version_id,
            None,
            "no design version is available",
            items,
        ));
    };
    items.push(ImplementationReadyItem::pass("design_version_exists", None));

    if version.current_design_version_id == Some(version.design_version_id) {
        items.push(ImplementationReadyItem::pass(
            "design_version_current",
            None,
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "design_version_current",
            Some("import or select the current design version".to_string()),
        ));
    }

    if version.status == "approved" && version.approved_by_authority_event_id.is_some() {
        items.push(ImplementationReadyItem::pass(
            "design_version_approved",
            None,
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "design_version_approved",
            Some("approve the design version before implementation starts".to_string()),
        ));
    }

    let missing_derivation_count = count_missing_derivations(&conn, version.design_version_id)?;
    if missing_derivation_count == 0 {
        items.push(ImplementationReadyItem::pass(
            "task_derivations_exist",
            None,
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "task_derivations_exist",
            Some(format!(
                "{missing_derivation_count} active requirements have no task derivation"
            )),
        ));
    }

    let stale_derivation_count = count_stale_task_derivations(&conn, version.design_package_id)?;
    if stale_derivation_count == 0 {
        items.push(ImplementationReadyItem::pass(
            "task_derivations_current",
            None,
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "task_derivations_current",
            Some(format!(
                "{stale_derivation_count} task derivations are stale"
            )),
        ));
    }

    let stale_checklist_count = count_stale_checklists(&conn, version.design_package_id)?;
    if stale_checklist_count == 0 {
        items.push(ImplementationReadyItem::pass("checklists_current", None));
    } else {
        items.push(ImplementationReadyItem::fail(
            "checklists_current",
            Some(format!("{stale_checklist_count} checklists are stale")),
        ));
    }

    let stale_validation_gate_count =
        count_stale_validation_gates(&conn, version.design_package_id)?;
    if stale_validation_gate_count == 0 {
        items.push(ImplementationReadyItem::pass(
            "validation_gates_current",
            None,
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "validation_gates_current",
            Some(format!(
                "{stale_validation_gate_count} validation gates are stale"
            )),
        ));
    }

    let stale_coverage_count = count_stale_coverage_items(&conn, version.design_package_id)?;
    if stale_coverage_count == 0 {
        items.push(ImplementationReadyItem::pass(
            "coverage_items_current",
            None,
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "coverage_items_current",
            Some(format!("{stale_coverage_count} coverage items are stale")),
        ));
    }

    let missing_validation_count =
        count_missing_validation_links(&conn, version.design_version_id)?;
    if missing_validation_count == 0 {
        items.push(ImplementationReadyItem::pass(
            "validation_expectations_linked",
            None,
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "validation_expectations_linked",
            Some(format!(
                "{missing_validation_count} active requirements have no linked validation template"
            )),
        ));
    }

    let missing_selected_gate_count =
        count_missing_selected_gates(&conn, version.design_version_id)?;
    if missing_selected_gate_count == 0 {
        items.push(ImplementationReadyItem::pass(
            "validation_gates_selected",
            None,
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "validation_gates_selected",
            Some(format!(
                "{missing_selected_gate_count} active task derivations have no selected validation gate"
            )),
        ));
    }

    let missing_completion_condition_count =
        count_missing_completion_conditions(&conn, version.design_version_id)?;
    if missing_completion_condition_count == 0 {
        items.push(ImplementationReadyItem::pass(
            "completion_conditions_present",
            None,
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "completion_conditions_present",
            Some(format!(
                "{missing_completion_condition_count} active task derivations have no completion condition"
            )),
        ));
    }

    let missing_evidence_count =
        count_closed_derived_tasks_missing_evidence(&conn, version.design_version_id)?;
    if missing_evidence_count == 0 {
        items.push(ImplementationReadyItem::pass(
            "implementation_evidence_present",
            None,
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "implementation_evidence_present",
            Some(format!(
                "{missing_evidence_count} closed design-derived tasks have no implementation evidence"
            )),
        ));
    }

    let missing_coverage_count =
        count_closed_derived_tasks_missing_coverage(&conn, version.design_version_id)?;
    if missing_coverage_count == 0 {
        items.push(ImplementationReadyItem::pass(
            "coverage_items_present",
            None,
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "coverage_items_present",
            Some(format!(
                "{missing_coverage_count} closed design-derived tasks have no covered coverage item"
            )),
        ));
    }

    let current_plan_reviews = current_decomposition_plan_review_state(
        &conn,
        project_id,
        version.design_package_id,
        version.design_version_id,
    )?;
    if current_plan_reviews.total == 0 {
        // The installed compatibility branch may still be materializing its
        // matching Plan. Its ordinary review gate below remains authoritative.
    } else if current_plan_reviews.applied == current_plan_reviews.total
        && current_plan_reviews.accepted_clean == current_plan_reviews.total
    {
        items.push(ImplementationReadyItem::pass(
            "current_decomposition_plans_reviewed",
            Some(format!(
                "{} current plans are applied with accepted exact reviews",
                current_plan_reviews.total
            )),
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "current_decomposition_plans_reviewed",
            Some(format!(
                "{} current plans, {} applied, {} with accepted exact reviews",
                current_plan_reviews.total,
                current_plan_reviews.applied,
                current_plan_reviews.accepted_clean
            )),
        ));
    }
    if current_plan_reviews.total > 0 {
        let incomplete = current_plan_reviews.total - current_plan_reviews.accepted_clean;
        if incomplete == 0 {
            items.push(ImplementationReadyItem::pass(
                "pre_implementation_reviews_clean",
                Some(format!(
                    "{} current exact decomposition reviews are owner-accepted clean",
                    current_plan_reviews.total
                )),
            ));
        } else {
            items.push(ImplementationReadyItem::fail(
                "pre_implementation_reviews_clean",
                Some(format!(
                    "{} current exact decomposition reviews, {} not owner-accepted clean",
                    current_plan_reviews.total, incomplete
                )),
            ));
        }
    } else {
        let review_state =
            implementation_review_gate_state(&conn, project_id, version.design_version_id)?;
        let decomposition_review_plan_count = required_review_plan_count(
            &conn,
            project_id,
            version.design_version_id,
            "implementation-ready",
            "design_task_decomposition",
        )?;
        if decomposition_review_plan_count == 0 {
            items.push(ImplementationReadyItem::fail(
                "pre_implementation_reviews_clean",
                Some(
                    "add a required implementation-ready design_task_decomposition review plan for this design version",
                ),
            ));
        } else if review_state.incomplete_required_plan_count == 0
            && review_state.missing_context_run_count == 0
            && review_state.unresolved_finding_count == 0
        {
            items.push(ImplementationReadyItem::pass(
                "pre_implementation_reviews_clean",
                Some(format!(
                    "{} required plans, {} missing review-context runs, {} unresolved findings",
                    review_state.required_plan_count,
                    review_state.missing_context_run_count,
                    review_state.unresolved_finding_count
                )),
            ));
        } else {
            items.push(ImplementationReadyItem::fail(
                "pre_implementation_reviews_clean",
                Some(format!(
                    "{} required plans, {} incomplete, {} missing review-context runs, {} unresolved findings",
                    review_state.required_plan_count,
                    review_state.incomplete_required_plan_count,
                    review_state.missing_context_run_count,
                    review_state.unresolved_finding_count
                )),
            ));
        }
    }

    let result = if items.iter().all(|item| item.result == "pass") {
        "pass"
    } else {
        "blocked"
    };
    let blocking_reason = if result == "pass" {
        None
    } else {
        Some("implementation prerequisites are not ready".to_string())
    };

    Ok(ImplementationReadyOutcome {
        result: result.to_string(),
        blocking_reason,
        design_package_id: Some(version.design_package_id),
        design_version_id: Some(version.design_version_id),
        items,
    })
}

/// Resolve the current design from one exact work owner and evaluate the same
/// implementation-readiness contract used by the installed design form.
pub fn implementation_ready_for_work(
    root: &Path,
    work_unit_id: i64,
) -> Result<ImplementationReadyOutcome> {
    let design_version_id = design_version_for_work(root, work_unit_id)?;
    implementation_ready(
        root,
        ImplementationReadyCheck {
            design_version_id: Some(design_version_id),
        },
    )
}

pub fn design_version_for_work(root: &Path, work_unit_id: i64) -> Result<i64> {
    let conn = open_existing_project(root)?;
    let project = project_id(&conn)?;
    let work_exists: bool = conn.query_row(
        "select exists(select 1 from work_units where id=?1 and project_id=?2)",
        params![work_unit_id, project],
        |row| row.get(0),
    )?;
    if !work_exists {
        bail!("work unit {work_unit_id} not found in this project; next: agent-workbench status");
    }
    let mut stmt = conn.prepare(
        r#"
            select distinct candidate.design_version_id
            from (
                select plan.design_version_id
                from decomposition_plans plan
                join design_versions version on version.id=plan.design_version_id
                join design_packages package on package.id=version.design_package_id
                where plan.project_id=?1 and plan.work_unit_id=?2
                  and plan.status != 'superseded'
                  and package.current_design_version_id=plan.design_version_id
                union
                select checklist.design_version_id
                from checklists checklist
                join design_versions version on version.id=checklist.design_version_id
                join design_packages package on package.id=version.design_package_id
                where checklist.project_id=?1 and checklist.work_unit_id=?2
                  and checklist.status in ('active','stale')
                  and package.current_design_version_id=checklist.design_version_id
            ) candidate
            order by candidate.design_version_id
            "#,
    )?;
    let candidates = stmt
        .query_map(params![project, work_unit_id], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    match candidates.as_slice() {
        [design_version_id] => Ok(*design_version_id),
        [] => bail!(
            "work unit {work_unit_id} has no current design binding; next: agent-workbench status --work {work_unit_id}"
        ),
        _ => bail!(
            "work unit {work_unit_id} has multiple current design bindings; next: agent-workbench status --work {work_unit_id}"
        ),
    }
}

struct CurrentPlanReviewState {
    total: i64,
    applied: i64,
    accepted_clean: i64,
}

fn current_decomposition_plan_review_state(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_package_id: i64,
    selected_design_version_id: i64,
) -> Result<CurrentPlanReviewState> {
    let mut statement = conn.prepare(
        r#"
        select plan.work_unit_id,plan.status
        from decomposition_plans plan
        join work_units work on work.id=plan.work_unit_id
        where plan.project_id=?1 and plan.design_package_id=?2
          and plan.status!='superseded' and plan.work_unit_id is not null
          and work.status in ('open','blocked')
          and not exists (
            select 1
            from decomposition_plans newer
            where newer.project_id=plan.project_id
              and newer.design_package_id=plan.design_package_id
              and newer.work_unit_id=plan.work_unit_id
              and newer.status!='superseded'
              and (newer.revision>plan.revision
                   or (newer.revision=plan.revision and newer.id>plan.id))
          )
        order by plan.work_unit_id
        "#,
    )?;
    let plans = statement
        .query_map(params![project_id, design_package_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut state = CurrentPlanReviewState {
        total: plans.len() as i64,
        applied: 0,
        accepted_clean: 0,
    };
    for (work_unit_id, status) in plans {
        if status == "applied" {
            state.applied += 1;
        }
        let Some(target) = current_decomposition_plan_review_target(
            conn,
            project_id,
            selected_design_version_id,
            work_unit_id,
        )?
        else {
            continue;
        };
        let owner = resolve_decomposition_plan_review_owner(conn, project_id, &target)?;
        if owner.state == PlanReviewOwnerState::AcceptedClean {
            state.accepted_clean += 1;
        }
    }
    Ok(state)
}

pub(super) fn required_review_plan_count(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_version_id: i64,
    stage: &str,
    review_type: &str,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from review_plans
        where project_id = ?1
          and design_version_id = ?2
          and stage = ?3
          and review_type = ?4
          and required = 1
        "#,
        params![project_id, design_version_id, stage, review_type],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn implementation_review_gate_state(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_version_id: i64,
) -> Result<ReviewGateState> {
    let required_plan_count = conn.query_row(
        r#"
        select count(*)
        from review_plans
        where project_id = ?1
          and design_version_id = ?2
          and stage = 'implementation-ready'
          and required = 1
        "#,
        params![project_id, design_version_id],
        |row| row.get::<_, i64>(0),
    )?;
    let incomplete_required_plan_count = conn.query_row(
        r#"
        select count(*)
        from review_plans
        where project_id = ?1
          and design_version_id = ?2
          and stage = 'implementation-ready'
          and required = 1
          and status != 'clean'
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'review_plan'
              and ar.review_plan_id = review_plans.id
              and ar.status = 'approved'
              and ar.acceptance_type in ('explicit_exception', 'stale_accepted')
          )
        "#,
        params![project_id, design_version_id],
        |row| row.get::<_, i64>(0),
    )?;
    let unresolved_finding_count = conn.query_row(
        r#"
        select count(*)
        from findings f
        join review_runs rr on rr.id = f.review_run_id
        join review_plans rp on rp.id = rr.review_plan_id
        where rp.project_id = ?1
          and rp.design_version_id = ?2
          and rp.stage in ('design-ready', 'implementation-ready')
          and f.finding_type in ('design_finding', 'design_task_gap')
          and f.status not in ('closed', 'accepted_out_of_scope')
          and f.classification not in ('invalid')
          and not exists(select 1 from legacy_claim_audits l where l.project_id=f.project_id and l.review_run_id=f.review_run_id and l.reviewer_resolution in ('unbound','ambiguous'))
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'finding'
              and ar.finding_id = f.id
              and ar.status = 'approved'
              and ar.acceptance_type in (
                'accepted_out_of_scope', 'explicit_exception', 'classified_failure'
              )
          )
        "#,
        params![project_id, design_version_id],
        |row| row.get::<_, i64>(0),
    )?;
    let missing_context_run_count = required_plans_missing_context_count(
        conn,
        project_id,
        "implementation-ready",
        "design_task_decomposition",
        Some(design_version_id),
        None,
        "design-task-decomposition",
    )?;
    Ok(ReviewGateState {
        required_plan_count,
        incomplete_required_plan_count,
        missing_context_run_count,
        unresolved_finding_count,
    })
}

#[derive(Default)]
pub(super) struct ReviewGateState {
    required_plan_count: i64,
    incomplete_required_plan_count: i64,
    missing_context_run_count: i64,
    unresolved_finding_count: i64,
}

pub(super) fn require_task_derivation(
    conn: &rusqlite::Connection,
    design_requirement_id: i64,
    task_id: i64,
) -> Result<()> {
    let exists = conn
        .query_row(
            r#"
            select 1
            from task_derivations
            where design_requirement_id = ?1
              and task_id = ?2
              and status = 'active'
            "#,
            params![design_requirement_id, task_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some();
    if !exists {
        bail!("task is not actively derived from the design requirement");
    }
    Ok(())
}

pub(super) fn require_implementation_evidence_derivation(
    conn: &rusqlite::Connection,
    design_requirement_id: i64,
    task_id: i64,
) -> Result<()> {
    let exists = conn
        .query_row(
            r#"
            select 1
            from task_derivations td
            join tasks t on t.id = td.task_id
            where td.design_requirement_id = ?1
              and td.task_id = ?2
              and (
                td.status = 'active'
                or (td.status = 'stale' and t.status = 'closed')
              )
            "#,
            params![design_requirement_id, task_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some();
    if !exists {
        bail!(
            "task is not actively derived from the design requirement or a closed task with a stale derivation"
        );
    }
    Ok(())
}

pub(super) fn task_has_active_design_derivation(
    conn: &rusqlite::Connection,
    task_id: i64,
) -> Result<bool> {
    conn.query_row(
        r#"
        select exists (
            select 1
            from task_derivations
            where task_id = ?1
              and status = 'active'
        )
        "#,
        params![task_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn get_or_create_checklist(
    conn: &rusqlite::Connection,
    project_id: i64,
    work_unit_id: i64,
    design_version_id: i64,
    title: &str,
) -> Result<i64> {
    if let Some(id) = conn
        .query_row(
            r#"
            select id
            from checklists
            where project_id = ?1
              and work_unit_id = ?2
              and design_version_id = ?3
              and title = ?4
              and status = 'active'
            order by id desc
            limit 1
            "#,
            params![project_id, work_unit_id, design_version_id, title],
            |row| row.get(0),
        )
        .optional()?
    {
        return Ok(id);
    }
    conn.execute(
        r#"
        insert into checklists(
            project_id, work_unit_id, design_version_id, title, status, created_at
        )
        values (?1, ?2, ?3, ?4, 'active', current_timestamp)
        "#,
        params![project_id, work_unit_id, design_version_id, title],
    )?;
    Ok(conn.last_insert_rowid())
}

pub(super) fn resolve_design_version(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_version_id: Option<i64>,
) -> Result<Option<ResolvedDesignVersion>> {
    match design_version_id {
        Some(id) => conn
            .query_row(
                r#"
                select
                    v.id, v.design_package_id, v.status,
                    v.approved_by_authority_event_id, p.current_design_version_id
                from design_versions v
                join design_packages p on p.id = v.design_package_id
                where v.project_id = ?1 and v.id = ?2
                "#,
                params![project_id, id],
                resolved_design_version,
            )
            .optional()
            .map_err(Into::into),
        None => {
            let current_count: i64 = conn.query_row(
                "select count(*) from design_packages where project_id = ?1 and current_design_version_id is not null",
                params![project_id],
                |row| row.get(0),
            )?;
            if current_count != 1 {
                return Ok(None);
            }
            conn.query_row(
                r#"
                select
                    v.id, v.design_package_id, v.status,
                    v.approved_by_authority_event_id, p.current_design_version_id
                from design_packages p
                join design_versions v on v.id = p.current_design_version_id
                where p.project_id = ?1
                "#,
                params![project_id],
                resolved_design_version,
            )
            .optional()
            .map_err(Into::into)
        }
    }
}

pub(super) fn count_missing_derivations(
    conn: &rusqlite::Connection,
    design_version_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from design_requirements r
        where r.design_version_id = ?1
          and r.status = 'active'
          and not exists (
            select 1
            from task_derivations td
            where td.design_requirement_id = r.id
              and td.status = 'active'
          )
        "#,
        params![design_version_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_stale_task_derivations(
    conn: &rusqlite::Connection,
    design_package_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join design_requirements r on r.id = td.design_requirement_id
        join tasks source_task on source_task.id = td.task_id
        join work_units source_work
          on source_work.id = source_task.work_unit_id
         and source_work.status in ('open', 'blocked')
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where p.id = ?1
          and td.status in ('active', 'stale')
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
          and not exists (
            select 1
            from task_derivations replacement
            join design_requirements replacement_requirement
              on replacement_requirement.id=replacement.design_requirement_id
            join design_versions replacement_version
              on replacement_version.id=replacement_requirement.design_version_id
            join tasks replacement_task on replacement_task.id=replacement.task_id
            where replacement.project_id=td.project_id
              and replacement.status!='stale'
              and replacement_version.status='approved'
              and replacement_requirement.design_version_id=p.current_design_version_id
              and replacement_requirement.requirement_key=r.requirement_key
              and replacement_task.work_unit_id is source_task.work_unit_id
          )
        "#,
        params![design_package_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_stale_checklists(
    conn: &rusqlite::Connection,
    design_package_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(distinct c.id)
        from checklists c
        join work_units source_work
          on source_work.id = c.work_unit_id
         and source_work.status in ('open', 'blocked')
        join checklist_items ci on ci.checklist_id = c.id
        join design_requirements r on r.id = ci.design_requirement_id
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where p.id = ?1
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
        params![design_package_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_stale_validation_gates(
    conn: &rusqlite::Connection,
    design_package_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from validation_gates vg
        join validation_gate_templates gt on gt.id = vg.template_id
        join design_requirements r on r.id = vg.design_requirement_id
        left join tasks source_task on source_task.id = vg.task_id
        join work_units source_work
          on source_work.id = coalesce(vg.work_unit_id, source_task.work_unit_id)
         and source_work.status in ('open', 'blocked')
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where p.id = ?1
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
          and not exists (
            select 1
            from validation_gates replacement
            join design_requirements replacement_requirement
              on replacement_requirement.id=replacement.design_requirement_id
            join design_versions replacement_version
              on replacement_version.id=replacement_requirement.design_version_id
            left join tasks replacement_task on replacement_task.id=replacement.task_id
            where replacement.project_id=vg.project_id
              and replacement.status!='stale'
              and replacement_version.status='approved'
              and replacement_requirement.design_version_id=p.current_design_version_id
              and replacement_requirement.requirement_key=r.requirement_key
              and replacement.gate_key=vg.gate_key
              and coalesce(replacement.work_unit_id,replacement_task.work_unit_id,0)
                  =coalesce(vg.work_unit_id,source_task.work_unit_id,0)
          )
        "#,
        params![design_package_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_stale_coverage_items(
    conn: &rusqlite::Connection,
    design_package_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from coverage_items c
        join design_requirements r on r.id = c.design_requirement_id
        left join tasks source_task on source_task.id = c.task_id
        join work_units source_work
          on source_work.id = coalesce(c.work_unit_id, source_task.work_unit_id)
         and source_work.status in ('open', 'blocked')
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where p.id = ?1
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
        params![design_package_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_missing_validation_links(
    conn: &rusqlite::Connection,
    design_version_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from design_requirements r
        where r.design_version_id = ?1
          and r.status = 'active'
          and (r.validation_expectation is not null and r.validation_expectation != '')
          and not exists (
            select 1
            from validation_gate_template_requirements gr
            join validation_gate_templates g on g.id = gr.validation_gate_template_id
            where gr.design_requirement_id = r.id
              and g.status = 'active'
          )
        "#,
        params![design_version_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_missing_selected_gates(
    conn: &rusqlite::Connection,
    design_version_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join design_requirements r on r.id = td.design_requirement_id
        where r.design_version_id = ?1
          and r.status = 'active'
          and td.status = 'active'
          and not exists (
            select 1
            from current_task_validation_gates vg
            where vg.design_requirement_id = r.id
              and vg.task_id = td.task_id
              and vg.selected_before_edit = 1
          )
        "#,
        params![design_version_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_missing_completion_conditions(
    conn: &rusqlite::Connection,
    design_version_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join design_requirements r on r.id = td.design_requirement_id
        join tasks t on t.id = td.task_id
        left join checklist_items ci on ci.id = td.checklist_item_id
        where r.design_version_id = ?1
          and r.status = 'active'
          and td.status = 'active'
          and coalesce(
            nullif(trim(ci.completion_condition), ''),
            nullif(trim(t.completion_condition), '')
          ) is null
        "#,
        params![design_version_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_closed_derived_tasks_missing_evidence(
    conn: &rusqlite::Connection,
    design_version_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join design_requirements r on r.id = td.design_requirement_id
        join tasks t on t.id = td.task_id
        where r.design_version_id = ?1
          and r.status = 'active'
          and td.status = 'active'
          and t.status = 'closed'
          and not exists (
            select 1
            from implementation_evidence e
            where e.task_id = td.task_id
              and (
                e.design_requirement_id = r.id
                or (
                  e.design_requirement_id is null
                  and not exists (
                    select 1
                    from task_derivations sibling
                    where sibling.task_id = td.task_id
                      and sibling.status = 'active'
                      and sibling.design_requirement_id != r.id
                  )
                )
              )
          )
          and not exists (
            select 1 from correction_completion_inheritance_sources inheritance
            join valid_completion_inheritance_sources valid on valid.id=inheritance.id
            where inheritance.current_requirement_id=r.id
              and inheritance.canonical_task_id=t.id
              and exists (
                select 1 from correction_completion_inheritance_evidence mapped
                where mapped.inheritance_source_id=inheritance.id
                  and mapped.evidence_kind='implementation_evidence'
              )
          )
        "#,
        params![design_version_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_closed_derived_tasks_missing_coverage(
    conn: &rusqlite::Connection,
    design_version_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join design_requirements r on r.id = td.design_requirement_id
        join tasks t on t.id = td.task_id
        where r.design_version_id = ?1
          and r.status = 'active'
          and td.status = 'active'
          and t.status = 'closed'
          and not exists (
            select 1
            from coverage_items c
            where c.design_requirement_id = r.id
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
        params![design_version_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}
