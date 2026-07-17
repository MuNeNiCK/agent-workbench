use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::identity::{CanonicalValue, domain_digest};

use super::capabilities::{CapabilityIssueRequest, GrantLineageBinding};
use super::decision_projection_support::{
    parse_finding_epoch_target, parse_review_correction_target,
};
use super::grants::set_contains;
use super::signed_envelope::{parse_hex, parse_rfc3339_seconds};

pub(super) fn parse_target_id(value: &str, prefix: &str) -> Result<i64> {
    let raw = value
        .strip_prefix(prefix)
        .context("decision target has the wrong tag")?;
    let id = raw
        .parse::<i64>()
        .context("decision target id is invalid")?;
    if id <= 0 {
        bail!("decision target id must be positive");
    }
    Ok(id)
}

pub(super) fn current_design_context(
    conn: &rusqlite::Connection,
    project: i64,
    target: &str,
) -> Result<String> {
    if target.starts_with("bootstrap_target:") {
        let value = target
            .rsplit(':')
            .next()
            .context("bootstrap target context is missing")?;
        parse_hex::<32>(value, "bootstrap design context")?;
        return Ok(value.to_string());
    }
    let context: Option<String> = if let Some(raw) = target.strip_prefix("review_plan:") {
        conn.query_row("select v.content_hash from review_plans p left join design_versions v on v.id=p.design_version_id where p.project_id=?1 and p.id=?2",params![project,raw.parse::<i64>()?],|row|row.get(0)).optional()?.flatten()
    } else if let Some(raw) = target.strip_prefix("review_run:") {
        conn.query_row("select v.content_hash from review_runs r join review_plans p on p.id=r.review_plan_id left join design_versions v on v.id=p.design_version_id where r.project_id=?1 and r.id=?2",params![project,raw.parse::<i64>()?],|row|row.get(0)).optional()?.flatten()
    } else if let Some(raw) = target.strip_prefix("finding:") {
        conn.query_row("select v.content_hash from findings f join review_runs r on r.id=f.review_run_id join review_plans p on p.id=r.review_plan_id left join design_versions v on v.id=p.design_version_id where f.project_id=?1 and f.id=?2",params![project,raw.parse::<i64>()?],|row|row.get(0)).optional()?.flatten()
    } else if let Some(raw) = target.strip_prefix("closure_attempt:") {
        conn.query_row("select v.content_hash from closure_attempts a join closures c on c.id=a.closure_id join findings f on f.id=c.finding_id join review_runs r on r.id=f.review_run_id join review_plans p on p.id=r.review_plan_id left join design_versions v on v.id=p.design_version_id where a.project_id=?1 and a.id=?2",params![project,raw.parse::<i64>()?],|row|row.get(0)).optional()?.flatten()
    } else {
        None
    };
    Ok(context.unwrap_or_else(|| {
        domain_digest(
            b"agent-workbench:target-design-context-v1\0",
            &CanonicalValue::string(target),
        )
    }))
}

pub(super) fn validate_grant_lineage(
    conn: &rusqlite::Connection,
    project: i64,
    grant_id: i64,
    binding: GrantLineageBinding<'_>,
) -> Result<()> {
    let mut stmt = conn.prepare(
        "with recursive lineage(id,parent_grant_id,owner_ref,maximum_target,roles,decision_families,actions,status,expires_at) as (
           select id,parent_grant_id,owner_ref,maximum_target,roles,decision_families,actions,status,expires_at from owner_decision_grants where project_id=?1 and id=?2
           union all
           select g.id,g.parent_grant_id,g.owner_ref,g.maximum_target,g.roles,g.decision_families,g.actions,g.status,g.expires_at from owner_decision_grants g join lineage l on g.id=l.parent_grant_id where g.project_id=?1
         ) select owner_ref,maximum_target,roles,decision_families,actions,status,expires_at from lineage",
    )?;
    let rows = stmt.query_map(params![project, grant_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
        ))
    })?;
    let mut count = 0;
    for row in rows {
        let (line_owner, scope, roles, families, actions, status, expiry) = row?;
        count += 1;
        if line_owner != binding.owner
            || status != "active"
            || parse_rfc3339_seconds(&expiry, "grant expiry")? <= binding.now
            || !grant_scope_contains(conn, project, &scope, binding.target)?
            || !set_contains(&roles, binding.role)
            || !set_contains(&families, binding.family)
            || !set_contains(&actions, binding.action)
        {
            bail!("grant_lineage_not_current");
        }
    }
    if count == 0 {
        bail!("grant_lineage_missing");
    }
    Ok(())
}

pub(super) fn grant_scope_contains(
    conn: &rusqlite::Connection,
    project: i64,
    scope: &str,
    target: &str,
) -> Result<bool> {
    if scope == "owner_all" || scope == target {
        return Ok(true);
    }
    if let Some(raw) = scope.strip_prefix("review_plan:") {
        let plan: i64 = raw.parse().context("invalid review plan grant scope")?;
        if target.starts_with("review_correction:") {
            let (historical, _) = parse_review_correction_target(target)?;
            return Ok(conn.query_row("select exists(select 1 from owner_decisions od join review_adjudication_decisions d on d.owner_decision_id=od.id join review_runs r on r.id=d.review_run_id where od.project_id=?1 and r.review_plan_id=?2 and od.decision_handle=?3)",params![project,plan,historical],|row|row.get(0))?);
        }
        if let Some(run) = target.strip_prefix("review_run:") {
            return Ok(conn.query_row("select exists(select 1 from review_runs where project_id=?1 and review_plan_id=?2 and id=?3)",params![project,plan,run.parse::<i64>()?],|row|row.get(0))?);
        }
        if let Some(finding) = target.strip_prefix("finding:") {
            return Ok(conn.query_row("select exists(select 1 from findings f join review_runs r on r.id=f.review_run_id where f.project_id=?1 and r.review_plan_id=?2 and f.id=?3)",params![project,plan,finding.parse::<i64>()?],|row|row.get(0))?);
        }
        if let Some(attempt) = target.strip_prefix("closure_attempt:") {
            return Ok(conn.query_row("select exists(select 1 from closure_attempts a join closures c on c.id=a.closure_id join findings f on f.id=c.finding_id join review_runs r on r.id=f.review_run_id where a.project_id=?1 and r.review_plan_id=?2 and a.id=?3)",params![project,plan,attempt.parse::<i64>()?],|row|row.get(0))?);
        }
    }
    if let Some(raw) = scope.strip_prefix("review_run:") {
        let run: i64 = raw.parse().context("invalid review run grant scope")?;
        if target.starts_with("review_correction:") {
            let (historical, _) = parse_review_correction_target(target)?;
            return Ok(conn.query_row("select exists(select 1 from owner_decisions od join review_adjudication_decisions d on d.owner_decision_id=od.id where od.project_id=?1 and d.review_run_id=?2 and od.decision_handle=?3)",params![project,run,historical],|row|row.get(0))?);
        }
        if let Some(finding) = target.strip_prefix("finding:") {
            return Ok(conn.query_row("select exists(select 1 from findings where project_id=?1 and review_run_id=?2 and id=?3)",params![project,run,finding.parse::<i64>()?],|row|row.get(0))?);
        }
        if let Some(attempt) = target.strip_prefix("closure_attempt:") {
            return Ok(conn.query_row("select exists(select 1 from closure_attempts a join closures c on c.id=a.closure_id join findings f on f.id=c.finding_id where a.project_id=?1 and f.review_run_id=?2 and a.id=?3)",params![project,run,attempt.parse::<i64>()?],|row|row.get(0))?);
        }
    }
    if let (Some(raw), Some(attempt)) = (
        scope.strip_prefix("finding:"),
        target.strip_prefix("closure_attempt:"),
    ) {
        return Ok(conn.query_row("select exists(select 1 from closure_attempts a join closures c on c.id=a.closure_id where a.project_id=?1 and c.finding_id=?2 and a.id=?3)",params![project,raw.parse::<i64>()?,attempt.parse::<i64>()?],|row|row.get(0))?);
    }
    if let (Some(raw), true) = (
        scope.strip_prefix("finding:"),
        target.starts_with("finding_epoch:"),
    ) {
        let (finding, _) = parse_finding_epoch_target(target)?;
        return Ok(raw.parse::<i64>()? == finding
            && conn.query_row(
                "select exists(select 1 from findings where project_id=?1 and id=?2)",
                params![project, finding],
                |row| row.get(0),
            )?);
    }
    Ok(false)
}

pub(super) fn grant_lineage_digest(
    conn: &rusqlite::Connection,
    project: i64,
    grant_id: i64,
) -> Result<String> {
    let mut stmt = conn.prepare(
        "with recursive lineage(id,parent_grant_id,grant_handle,depth) as (
           select id,parent_grant_id,grant_handle,0 from owner_decision_grants where project_id=?1 and id=?2
           union all select g.id,g.parent_grant_id,g.grant_handle,l.depth+1 from owner_decision_grants g join lineage l on g.id=l.parent_grant_id where g.project_id=?1
         ) select grant_handle from lineage order by depth desc",
    )?;
    let handles = stmt
        .query_map(params![project, grant_id], |row| row.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(domain_digest(
        b"agent-workbench:grant-lineage-v1\0",
        &CanonicalValue::Array(handles.into_iter().map(CanonicalValue::String).collect()),
    ))
}

pub(super) fn validate_principal_independence(
    conn: &rusqlite::Connection,
    project: i64,
    principal_id: i64,
    family: &str,
    target: &str,
) -> Result<()> {
    let reviewer: Option<i64> = match family {
        "review" if target.starts_with("review_correction:") => {
            let (historical, _) = parse_review_correction_target(target)?;
            let (reviewer, original):(Option<i64>,i64)=conn.query_row(
                "select i.reviewer_principal_id,od.principal_id from owner_decisions od join review_adjudication_decisions d on d.owner_decision_id=od.id left join review_agent_invocations i on i.review_run_id=d.review_run_id where od.project_id=?1 and od.decision_handle=?2",
                params![project,historical],|row|Ok((row.get(0)?,row.get(1)?)),
            ).context("historical review decision not found")?;
            if original == principal_id {
                bail!("correction_original_adjudicator_not_independent");
            }
            reviewer
        }
        "review" if target.starts_with("bootstrap_target:") => {
            let claim:Option<String>=conn.query_row("select claim_handle from authority_bootstrap_targets where project_id=?1 and target_handle=?2 and status='pending'",params![project,target],|row|row.get(0)).optional()?;
            if let Some(run) = claim.and_then(|v| {
                v.strip_prefix("review_run:")
                    .and_then(|v| v.parse::<i64>().ok())
            }) {
                conn.query_row("select reviewer_principal_id from review_agent_invocations where project_id=?1 and review_run_id=?2",params![project,run],|row|row.get(0)).optional()?
            } else {
                return Ok(());
            }
        }
        "review" => {
            let run = parse_target_id(target, "review_run:")?;
            conn.query_row("select i.reviewer_principal_id from review_agent_invocations i where i.project_id=?1 and i.review_run_id=?2",params![project,run],|row|row.get(0)).optional()?
        }
        "finding" if target.starts_with("finding_epoch:") => {
            let (finding, _) = parse_finding_epoch_target(target)?;
            let (reviewer,original):(Option<i64>,i64)=conn.query_row(
                "select i.reviewer_principal_id,od.principal_id from findings f join finding_disposition_decisions d on d.finding_id=f.id join owner_decisions od on od.id=d.owner_decision_id left join review_agent_invocations i on i.review_run_id=f.review_run_id where f.project_id=?1 and f.id=?2 and d.value in ('rejected','authority_disposed') order by d.id desc limit 1",
                params![project,finding],|row|Ok((row.get(0)?,row.get(1)?)),
            ).context("terminal finding decision not found")?;
            if original == principal_id {
                bail!("reopen_original_adjudicator_not_independent");
            }
            reviewer
        }
        "finding" => {
            let finding = parse_target_id(target, "finding:")?;
            conn.query_row("select i.reviewer_principal_id from findings f join review_agent_invocations i on i.review_run_id=f.review_run_id where f.project_id=?1 and f.id=?2",params![project,finding],|row|row.get(0)).optional()?
        }
        "verification" => {
            let attempt = parse_target_id(target, "closure_attempt:")?;
            conn.query_row("select i.reviewer_principal_id from finding_verifications v join review_agent_invocations i on i.review_run_id=v.review_run_id where v.project_id=?1 and v.closure_attempt_id=?2 order by v.id desc limit 1",params![project,attempt],|row|row.get(0)).optional()?
        }
        _ => None,
    };
    let reviewer = reviewer.context("reviewer_principal_missing")?;
    if reviewer == principal_id {
        bail!("reviewer_adjudicator_not_independent");
    }
    Ok(())
}

pub(super) fn validate_special_capability_target(
    conn: &rusqlite::Connection,
    project: i64,
    request: &CapabilityIssueRequest<'_>,
) -> Result<()> {
    match request.action {
        "bootstrap_adjudicate" => {
            if request.role != "human_authority" || request.decision_family != "review" {
                bail!("bootstrap capability tuple is invalid");
            }
            let row:Option<(String,String)>=conn.query_row("select owner_ref,context_digest from authority_bootstrap_targets where project_id=?1 and target_handle=?2 and status='pending'",params![project,request.target_ref],|row|Ok((row.get(0)?,row.get(1)?))).optional()?;
            let (owner, context) = row.context("bootstrap target is not pending")?;
            if owner != request.owner_ref || context != request.design_context {
                bail!("bootstrap target binding mismatch");
            }
        }
        "correct_terminal" => {
            if request.role != "human_authority" || request.decision_family != "review" {
                bail!("review correction capability tuple is invalid");
            }
            let (historical, boundary) = parse_review_correction_target(request.target_ref)?;
            let exists:i64=conn.query_row("select exists(select 1 from owner_decisions od join review_adjudication_decisions d on d.owner_decision_id=od.id join review_boundary_snapshots s on s.historical_owner_decision_id=od.id where od.project_id=?1 and od.decision_handle=?2 and s.boundary_handle=?3 and s.status='current')",params![project,historical,boundary],|row|row.get(0))?;
            if exists != 1 {
                bail!("historical review decision not found");
            }
        }
        "reopen" => {
            if request.role != "human_authority" || request.decision_family != "finding" {
                bail!("finding reopen capability tuple is invalid");
            }
            let (finding, epoch) = parse_finding_epoch_target(request.target_ref)?;
            let closed:i64=conn.query_row("select exists(select 1 from findings f join finding_decision_epochs e on e.finding_id=f.id where f.project_id=?1 and f.id=?2 and f.lifecycle_state='closed' and e.epoch_number=?3 and e.status='terminal')",params![project,finding,epoch],|row|row.get(0))?;
            if closed != 1 {
                bail!("finding epoch is not terminal");
            }
        }
        _ => {}
    }
    Ok(())
}

pub(super) fn validate_tuple(role: &str, family: &str, action: &str) -> Result<()> {
    let allowed = matches!(
        (role, family, action),
        ("review_adjudicator", "review", "adjudicate")
            | ("finding_adjudicator", "finding", "adjudicate")
            | ("verification_adjudicator", "verification", "adjudicate")
            | ("human_authority", "finding", "dispose")
            | ("human_authority", "review", "bootstrap_adjudicate")
            | ("human_authority", "review", "correct_terminal")
            | ("human_authority", "finding", "reopen")
    );
    if !allowed {
        bail!("role, decision family, and action tuple is not allowed");
    }
    Ok(())
}
pub(super) fn validate_tuple_for_family(family: &str, action: &str) -> Result<()> {
    if !matches!(
        (family, action),
        (
            "review",
            "adjudicate" | "bootstrap_adjudicate" | "correct_terminal"
        ) | ("finding", "adjudicate" | "dispose" | "reopen")
            | ("verification", "adjudicate")
    ) {
        bail!("decision family/action tuple is not allowed");
    }
    Ok(())
}
