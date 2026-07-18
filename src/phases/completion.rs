use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::review_context::review_context_ref_with_phase;

use super::*;

pub(super) fn phase_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<WorkPhaseRecord> {
    Ok(WorkPhaseRecord {
        id: row.get(0)?,
        work_unit_id: row.get(1)?,
        phase_work_unit_id: row.get(2)?,
        design_version_id: row.get(3)?,
        key: row.get(4)?,
        title: row.get(5)?,
        kind: row.get(6)?,
        order: row.get(7)?,
        status: row.get(8)?,
        task_count: row.get(9)?,
    })
}

pub(super) fn load_phase(
    conn: &rusqlite::Connection,
    project_id: i64,
    phase_id: i64,
) -> Result<StoredPhase> {
    conn.query_row(
        r#"
        select id, work_unit_id, phase_work_unit_id, status
        from work_phases
        where id = ?1 and project_id = ?2
        "#,
        params![phase_id, project_id],
        |row| {
            Ok(StoredPhase {
                id: row.get(0)?,
                work_unit_id: row.get(1)?,
                phase_work_unit_id: row.get(2)?,
                status: row.get(3)?,
            })
        },
    )
    .optional()?
    .context("phase not found")
}

pub(super) fn ensure_open_work_unit(
    conn: &rusqlite::Connection,
    project_id: i64,
    work_unit_id: i64,
) -> Result<()> {
    conn.query_row(
        "select 1 from work_units where id = ?1 and project_id = ?2 and status = 'open'",
        params![work_unit_id, project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("open target work unit not found")?;
    Ok(())
}

pub(super) fn ensure_phase_can_move(phase: &StoredPhase) -> Result<()> {
    if !matches!(phase.status.as_str(), "open" | "blocked") {
        bail!(
            "phase must be open or blocked to split or rescope; current status is {}",
            phase.status
        );
    }
    Ok(())
}

pub(super) fn ensure_authority_event(
    conn: &rusqlite::Connection,
    project_id: i64,
    authority_event_id: i64,
) -> Result<()> {
    conn.query_row(
        "select 1 from authority_events where id = ?1 and project_id = ?2 and status = 'active'",
        params![authority_event_id, project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("active authority event not found")?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn insert_phase_event(
    conn: &rusqlite::Connection,
    project_id: i64,
    phase_id: i64,
    event_type: &str,
    reason: Option<&str>,
    authority_event_id: Option<i64>,
    related_task_id: Option<i64>,
    related_work_unit_id: Option<i64>,
    previous_status: Option<&str>,
    next_status: Option<&str>,
) -> Result<()> {
    conn.execute(
        r#"
        insert into work_phase_events(
            project_id, phase_id, event_type, reason, authority_event_id,
            related_task_id, related_work_unit_id, previous_status, next_status,
            created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, current_timestamp)
        "#,
        params![
            project_id,
            phase_id,
            event_type,
            reason,
            authority_event_id,
            related_task_id,
            related_work_unit_id,
            previous_status,
            next_status,
        ],
    )?;
    Ok(())
}

pub(super) fn phase_task_ids(conn: &rusqlite::Connection, phase_id: i64) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(
        "select task_id from work_phase_task_memberships where phase_id = ?1 order by task_id",
    )?;
    let rows = stmt.query_map(params![phase_id], |row| row.get(0))?;
    collect_rows(rows)
}

pub(super) fn phase_task_count(conn: &rusqlite::Connection, phase_id: i64) -> Result<i64> {
    conn.query_row(
        "select count(*) from work_phase_task_memberships where phase_id = ?1",
        params![phase_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_phase_tasks_with_status(
    conn: &rusqlite::Connection,
    phase_id: i64,
    status_a: &str,
    status_b: &str,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from tasks t
        join work_phase_task_memberships m on m.task_id = t.id
        where m.phase_id = ?1 and t.status in (?2, ?3)
        "#,
        params![phase_id, status_a, status_b],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_phase_tasks_not_open(
    conn: &rusqlite::Connection,
    phase_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from tasks t
        join work_phase_task_memberships m on m.task_id = t.id
        where m.phase_id = ?1
          and t.status not in ('open', 'blocked')
          and not exists (
              select 1
              from work_phase_trace_decisions d
              where d.phase_id = m.phase_id
                and d.record_type = 'task'
                and d.record_id = t.id
                and d.decision in ('carry', 'accept')
          )
        "#,
        params![phase_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_phase_design_versions(
    conn: &rusqlite::Connection,
    phase_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(distinct dr.design_version_id)
        from task_derivations td
        join design_requirements dr on dr.id = td.design_requirement_id
        join work_phase_task_memberships m on m.task_id = td.task_id
        where m.phase_id = ?1 and td.status in ('active', 'stale')
        "#,
        params![phase_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_open_cross_phase_dependencies(
    conn: &rusqlite::Connection,
    phase_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from work_phase_dependencies
        where status = 'open'
          and (from_phase_id = ?1 or to_phase_id = ?1)
        "#,
        params![phase_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_open_phase_inbound_dependencies(
    conn: &rusqlite::Connection,
    phase_id: i64,
) -> Result<i64> {
    conn.query_row(
        "select count(*) from work_phase_dependencies where to_phase_id = ?1 and status = 'open'",
        params![phase_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_open_phase_checklist_items(
    conn: &rusqlite::Connection,
    phase_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from checklist_items ci
        join work_phase_task_memberships m on m.task_id = ci.task_id
        where m.phase_id = ?1 and ci.status in ('open', 'blocked')
        "#,
        params![phase_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_phase_validation_gate_blockers(
    conn: &rusqlite::Connection,
    phase_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from validation_gates vg
        join work_phase_task_memberships m on m.task_id = vg.task_id
        where m.phase_id = ?1
          and vg.status = 'active'
          and not exists (
              select 1
              from validation_runs vr
              where vr.validation_gate_id = vg.id
                and (
                    vr.result = 'pass'
                    or vr.acceptance_record_id is not null
                )
          )
        "#,
        params![phase_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_phase_missing_evidence(
    conn: &rusqlite::Connection,
    phase_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join work_phase_task_memberships m on m.task_id = td.task_id
        where m.phase_id = ?1
          and td.status = 'active'
          and not exists (
              select 1
              from implementation_evidence e
              where e.task_id = td.task_id
                and e.design_requirement_id = td.design_requirement_id
          )
          and not exists (
              select 1 from correction_completion_inheritance_sources inheritance
              join valid_completion_inheritance_sources valid on valid.id=inheritance.id
              where inheritance.current_requirement_id=td.design_requirement_id
                and inheritance.canonical_task_id=td.task_id
                and exists (
                    select 1 from correction_completion_inheritance_evidence mapped
                    where mapped.inheritance_source_id=inheritance.id
                      and mapped.evidence_kind='implementation_evidence'
                )
          )
        "#,
        params![phase_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_phase_missing_coverage(
    conn: &rusqlite::Connection,
    phase_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join work_phase_task_memberships m on m.task_id = td.task_id
        where m.phase_id = ?1
          and td.status = 'active'
          and not exists (
              select 1
              from coverage_items c
              where c.design_requirement_id = td.design_requirement_id
                and (
                    c.task_id = td.task_id
                    or (
                        c.task_id is null
                        and c.work_unit_id = (
                            select work_unit_id
                            from work_phases
                            where id = ?1
                        )
                    )
                )
                and c.status = 'covered'
          )
        "#,
        params![phase_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn count_incomplete_phase_reviews(
    conn: &rusqlite::Connection,
    phase_id: i64,
) -> Result<i64> {
    let mut stmt = conn.prepare(
        r#"
        select
            rp.id, rp.review_type, rp.stage, rp.design_version_id, rp.work_unit_id,
            coalesce(pol.required_consecutive_clean_fresh_runs, 1)
        from work_phase_review_targets pt
        join review_plans rp on rp.id = pt.review_plan_id
        left join review_policies pol on pol.id = rp.review_policy_id
        where pt.phase_id = ?1
          and rp.required = 1
          and not exists (
              select 1
              from acceptance_records ar
              where ar.target_type = 'review_plan'
                and ar.review_plan_id = rp.id
                and ar.status = 'approved'
                and ar.acceptance_type in ('explicit_exception', 'stale_accepted')
          )
        "#,
    )?;
    let rows = stmt.query_map(params![phase_id], |row| {
        Ok(PhaseReviewPlan {
            id: row.get(0)?,
            review_type: row.get(1)?,
            stage: row.get(2)?,
            design_version_id: row.get(3)?,
            work_unit_id: row.get(4)?,
            required_clean_fresh_runs: row.get(5)?,
        })
    })?;

    let mut incomplete = 0;
    for row in rows {
        let plan = row?;
        if !phase_review_plan_is_complete(conn, phase_id, &plan)? {
            incomplete += 1;
        }
    }
    Ok(incomplete)
}

pub(crate) fn phase_review_lifecycle_action(
    conn: &rusqlite::Connection,
    work_unit_id: i64,
    only_phase_id: Option<i64>,
) -> Result<Option<PhaseReviewLifecycleAction>> {
    let mut stmt = conn.prepare(
        r#"
        select p.id, rp.id, rp.review_type, rp.stage, rp.status,
               rp.design_version_id, rp.work_unit_id,
               coalesce(pol.required_consecutive_clean_fresh_runs, 1)
        from work_phases p
        join work_phase_review_targets target on target.phase_id=p.id
        join review_plans rp on rp.id=target.review_plan_id
        left join review_policies pol on pol.id=rp.review_policy_id
        where p.work_unit_id=?1 and (?2 is null or p.id=?2) and p.status='open'
          and rp.required=1
          and not exists (
            select 1 from acceptance_records ar
            where ar.target_type='review_plan' and ar.review_plan_id=rp.id
              and ar.status='approved'
              and ar.acceptance_type in ('explicit_exception','stale_accepted')
          )
        order by p.phase_order,p.id,rp.id
        "#,
    )?;
    let rows = stmt
        .query_map(params![work_unit_id, only_phase_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                PhaseReviewPlan {
                    id: row.get(1)?,
                    review_type: row.get(2)?,
                    stage: row.get(3)?,
                    design_version_id: row.get(5)?,
                    work_unit_id: row.get(6)?,
                    required_clean_fresh_runs: row.get(7)?,
                },
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (phase_id, plan, status) in rows {
        if phase_review_plan_is_complete(conn, phase_id, &plan)? {
            continue;
        }
        let sibling_complete = phase_review_sibling_is_complete(conn, phase_id, &plan)?;
        if matches!(status.as_str(), "exhausted" | "needs_user_decision")
            || (status == "blocked" && sibling_complete)
        {
            return Ok(Some(PhaseReviewLifecycleAction {
                phase_id,
                review_plan_id: plan.id,
                review_type: plan.review_type,
                stage: plan.stage,
                action: format!(
                    "agent-workbench review plan waive {} --reason \"<reason>\"",
                    plan.id
                ),
            }));
        }
    }
    Ok(None)
}

fn phase_review_sibling_is_complete(
    conn: &rusqlite::Connection,
    phase_id: i64,
    plan: &PhaseReviewPlan,
) -> Result<bool> {
    let mut stmt = conn.prepare(
        r#"
        select sibling.id,sibling.review_type,sibling.stage,sibling.design_version_id,
               sibling.work_unit_id,coalesce(pol.required_consecutive_clean_fresh_runs,1)
        from work_phase_review_targets target
        join review_plans sibling on sibling.id=target.review_plan_id
        left join review_policies pol on pol.id=sibling.review_policy_id
        where target.phase_id=?1 and sibling.id!=?2 and sibling.required=1
          and sibling.review_type=?3 and sibling.stage=?4
        order by sibling.id desc
        "#,
    )?;
    let siblings = stmt
        .query_map(
            params![phase_id, plan.id, plan.review_type, plan.stage],
            |row| {
                Ok(PhaseReviewPlan {
                    id: row.get(0)?,
                    review_type: row.get(1)?,
                    stage: row.get(2)?,
                    design_version_id: row.get(3)?,
                    work_unit_id: row.get(4)?,
                    required_clean_fresh_runs: row.get(5)?,
                })
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for sibling in siblings {
        if phase_review_plan_is_complete(conn, phase_id, &sibling)? {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn phase_review_plan_is_complete(
    conn: &rusqlite::Connection,
    phase_id: i64,
    plan: &PhaseReviewPlan,
) -> Result<bool> {
    let required = plan.required_clean_fresh_runs.max(1);
    let expected_context = phase_review_context_kind_for_plan(&plan.stage, &plan.review_type)
        .and_then(|kind| {
            plan.design_version_id.map(|design_version_id| {
                review_context_ref_with_phase(
                    kind,
                    Some(design_version_id),
                    Some(plan.work_unit_id),
                    Some(phase_id),
                )
            })
        });
    let mut stmt = conn.prepare(
        r#"
        select r.clean_run, r.target_ref, r.review_provenance, r.review_provenance_ref,
               exists (
                   select 1
                   from review_agent_invocations i
                   where i.review_run_id = r.id
                     and i.external_agent_id is not null
                     and i.external_agent_id != ''
               )
        from review_runs r
        where r.review_plan_id = ?1
          and r.target_type = 'phase'
          and r.phase_id = ?2
          and r.run_type = 'fresh'
          and r.run_purpose = 'new_unbiased_review'
          and r.status = 'completed'
          and r.new_findings_count = 0
        order by r.id desc
        "#,
    )?;
    let rows = stmt.query_map(params![plan.id, phase_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
            row.get::<_, i64>(4)? == 1,
        ))
    })?;
    let mut clean_count = 0;
    for row in rows {
        let (clean_run, target_ref, provenance, provenance_ref, has_external_agent) = row?;
        let context_matches = expected_context
            .as_ref()
            .is_none_or(|expected| target_ref.as_deref() == Some(expected.as_str()));
        if clean_run == 1
            && context_matches
            && phase_review_provenance_is_trusted(
                &provenance,
                provenance_ref.as_deref(),
                has_external_agent,
            )
        {
            clean_count += 1;
            if clean_count >= required {
                return Ok(true);
            }
        } else {
            break;
        }
    }
    Ok(false)
}

pub(super) fn phase_review_context_kind_for_plan(
    stage: &str,
    review_type: &str,
) -> Option<&'static str> {
    match (stage, review_type) {
        ("design-ready", "design_review") => Some("design-review"),
        ("implementation-ready", "design_task_decomposition") => Some("design-task-decomposition"),
        ("close-ready", "design_implementation_diff") => Some("design-implementation-diff"),
        ("close-ready", "implementation_review") => Some("implementation-review"),
        _ => None,
    }
}

pub(super) fn phase_review_provenance_is_trusted(
    provenance: &str,
    provenance_ref: Option<&str>,
    has_external_agent: bool,
) -> bool {
    match provenance {
        "external_agent" => {
            has_external_agent && provenance_ref.is_some_and(|value| !value.trim().is_empty())
        }
        "human_review" => provenance_ref.is_some_and(|value| !value.trim().is_empty()),
        _ => false,
    }
}

pub(super) fn validate_phase_key(key: &str) -> Result<()> {
    if key.trim().is_empty()
        || !key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        bail!("phase key must use lowercase ascii letters, digits, '-' or '_'");
    }
    Ok(())
}

pub(super) fn validate_phase_kind(kind: &str) -> Result<()> {
    if kind.trim().is_empty() {
        bail!("phase kind is required");
    }
    Ok(())
}

pub(super) fn validate_dependency_type(dependency_type: &str) -> Result<()> {
    match dependency_type {
        "blocks" | "requires" => Ok(()),
        _ => bail!("phase dependency type must be blocks or requires"),
    }
}

pub(super) fn validate_shared_record_policy(policy: &str) -> Result<()> {
    match policy {
        "require-decisions" | "carry-shared" => Ok(()),
        _ => bail!("shared record policy must be require-decisions or carry-shared"),
    }
}

pub(super) fn validate_trace_record_type(record_type: &str) -> Result<()> {
    match record_type {
        "task"
        | "task_derivation"
        | "checklist_item"
        | "validation_gate"
        | "coverage_item"
        | "implementation_evidence"
        | "review_plan"
        | "rule_binding"
        | "work_record" => Ok(()),
        _ => bail!("phase trace record type is not supported"),
    }
}

pub(super) fn validate_trace_decision(decision: &str) -> Result<()> {
    match decision {
        "split" | "carry" | "accept" => Ok(()),
        _ => bail!("phase trace decision must be split, carry, or accept"),
    }
}

pub(super) fn collect_rows<T>(rows: impl Iterator<Item = rusqlite::Result<T>>) -> Result<Vec<T>> {
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}
