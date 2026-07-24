use crate::authority::{OwnerDecisionOutcome, OwnerDecisionRequest, record_owner_decision};
use crate::db::{open_existing_project, project_id};
use anyhow::{Context, Result, bail};
use rusqlite::params;
use std::path::Path;

#[derive(Clone, Debug)]
pub struct AdjudicationInput<'a> {
    pub decision: &'a str,
    pub reason: &'a str,
    pub expected_current: &'a str,
}

pub fn adjudicate_owner(
    root: &Path,
    owner: &str,
    target: i64,
    input: AdjudicationInput<'_>,
) -> Result<OwnerDecisionOutcome> {
    if target <= 0 {
        bail!("decision target must be a positive project reference");
    }
    if !matches!(input.decision, "accepted" | "rejected" | "needs_evidence") {
        bail!("unsupported owner decision");
    }
    let conn = open_existing_project(root)?;
    let project = project_id(&conn)?;
    let (work, target_ref) = match owner {
        "review" => {
            let work: i64 = conn
                .query_row(
                    "select p.work_unit_id from review_runs r join review_plans p on p.id=r.review_plan_id where r.project_id=?1 and r.id=?2 and r.status='completed'",
                    params![project, target],
                    |row| row.get(0),
                )
                .context("review claim not found")?;
            (work, format!("review_run:{target}"))
        }
        "finding" => {
            let work: i64 = conn
                .query_row(
                    "select p.work_unit_id from findings f join review_runs r on r.id=f.review_run_id join review_plans p on p.id=r.review_plan_id where f.project_id=?1 and f.id=?2 and r.status='completed' and (select count(*) from findings inventory where inventory.review_run_id=r.id)=r.new_findings_count",
                    params![project, target],
                    |row| row.get(0),
                )
                .context("completed review finding inventory not found")?;
            (work, format!("finding:{target}"))
        }
        "verification" => {
            let work: i64 = conn
                .query_row(
                    "select p.work_unit_id from closure_attempts a join closures c on c.id=a.closure_id join findings f on f.id=c.finding_id join review_runs r on r.id=f.review_run_id join review_plans p on p.id=r.review_plan_id where a.project_id=?1 and a.id=?2",
                    params![project, target],
                    |row| row.get(0),
                )
                .context("verification attempt not found")?;
            (work, format!("closure_attempt:{target}"))
        }
        _ => bail!("unsupported decision owner"),
    };
    record_owner_decision(
        root,
        OwnerDecisionRequest {
            command_kind: "decision adjudicate",
            owner_ref: &format!("work_unit:{work}"),
            target_ref: &target_ref,
            decision_family: owner,
            action: "adjudicate",
            decision_value: input.decision,
            reason: input.reason,
            expected_current: input.expected_current,
        },
    )
}

pub fn adjudicate_review(
    root: &Path,
    run_id: i64,
    input: AdjudicationInput<'_>,
) -> Result<OwnerDecisionOutcome> {
    if !matches!(input.decision, "accepted" | "rejected" | "needs_evidence") {
        bail!("unsupported review decision");
    }
    let conn = open_existing_project(root)?;
    let project = project_id(&conn)?;
    let owner:i64=conn.query_row("select p.work_unit_id from review_runs r join review_plans p on p.id=r.review_plan_id where r.project_id=?1 and r.id=?2 and r.status='completed'",params![project,run_id],|row|row.get(0)).context("review claim not found")?;
    record_owner_decision(
        root,
        OwnerDecisionRequest {
            command_kind: "review adjudicate",
            owner_ref: &format!("work_unit:{owner}"),
            target_ref: &format!("review_run:{run_id}"),
            decision_family: "review",
            action: "adjudicate",
            decision_value: input.decision,
            reason: input.reason,
            expected_current: input.expected_current,
        },
    )
}

pub fn correct_terminal_review(
    root: &Path,
    decision: &str,
    boundary: &str,
    input: AdjudicationInput<'_>,
) -> Result<OwnerDecisionOutcome> {
    if !matches!(input.decision, "accepted" | "rejected" | "needs_evidence") {
        bail!("unsupported review correction outcome");
    }
    let conn = open_existing_project(root)?;
    let project = project_id(&conn)?;
    let owner: i64 = conn.query_row(
        "select p.work_unit_id from owner_decisions od join review_adjudication_decisions d on d.owner_decision_id=od.id join review_runs r on r.id=d.review_run_id join review_plans p on p.id=r.review_plan_id where od.project_id=?1 and od.decision_handle=?2",
        params![project, decision],
        |row| row.get(0),
    ).context("historical review decision not found")?;
    record_owner_decision(
        root,
        OwnerDecisionRequest {
            command_kind: "review correction add",
            owner_ref: &format!("work_unit:{owner}"),
            target_ref: &format!("review_correction:{decision}:{boundary}"),
            decision_family: "review",
            action: "correct_terminal",
            decision_value: input.decision,
            reason: input.reason,
            expected_current: input.expected_current,
        },
    )
}

pub fn reopen_finding_epoch(
    root: &Path,
    finding: i64,
    epoch: i64,
    input: AdjudicationInput<'_>,
) -> Result<OwnerDecisionOutcome> {
    let conn = open_existing_project(root)?;
    let project = project_id(&conn)?;
    let owner:i64=conn.query_row("select p.work_unit_id from findings f join review_runs r on r.id=f.review_run_id join review_plans p on p.id=r.review_plan_id where f.project_id=?1 and f.id=?2 and f.lifecycle_state='closed'",params![project,finding],|row|row.get(0)).context("terminal finding not found")?;
    record_owner_decision(
        root,
        OwnerDecisionRequest {
            command_kind: "finding reopen",
            owner_ref: &format!("work_unit:{owner}"),
            target_ref: &format!("finding_epoch:{finding}:{epoch}"),
            decision_family: "finding",
            action: "reopen",
            decision_value: "reopened",
            reason: input.reason,
            expected_current: input.expected_current,
        },
    )
}
pub fn decide_finding(
    root: &Path,
    finding_id: i64,
    input: AdjudicationInput<'_>,
) -> Result<OwnerDecisionOutcome> {
    if !matches!(
        input.decision,
        "accepted"
            | "rejected"
            | "needs_evidence"
            | "design_conflict"
            | "deferred"
            | "authority_disposed"
    ) {
        bail!("unsupported finding decision");
    }
    let conn = open_existing_project(root)?;
    let project = project_id(&conn)?;
    let owner:i64=conn.query_row("select p.work_unit_id from findings f join review_runs r on r.id=f.review_run_id join review_plans p on p.id=r.review_plan_id where f.project_id=?1 and f.id=?2 and r.status='completed' and (select count(*) from findings inventory where inventory.review_run_id=r.id)=r.new_findings_count",params![project,finding_id],|row|row.get(0)).context("completed review finding inventory not found")?;
    record_owner_decision(
        root,
        OwnerDecisionRequest {
            command_kind: "finding decide",
            owner_ref: &format!("work_unit:{owner}"),
            target_ref: &format!("finding:{finding_id}"),
            decision_family: "finding",
            action: if input.decision == "authority_disposed" {
                "dispose"
            } else {
                "adjudicate"
            },
            decision_value: input.decision,
            reason: input.reason,
            expected_current: input.expected_current,
        },
    )
}
pub fn adjudicate_verification(
    root: &Path,
    run: i64,
    finding: i64,
    closure: i64,
    attempt: i64,
    input: AdjudicationInput<'_>,
) -> Result<OwnerDecisionOutcome> {
    if !matches!(input.decision, "accepted" | "rejected" | "needs_evidence") {
        bail!("unsupported verification decision");
    }
    let conn = open_existing_project(root)?;
    let project = project_id(&conn)?;
    let owner:i64=conn.query_row("select p.work_unit_id from review_runs r join review_plans p on p.id=r.review_plan_id join finding_verifications v on v.review_run_id=r.id join closure_attempts a on a.id=?5 and a.closure_id=?4 where r.project_id=?1 and r.id=?2 and v.finding_id=?3 and v.closure_id=?4",params![project,run,finding,closure,attempt],|row|row.get(0)).context("verification context does not bind one run/finding/closure/attempt")?;
    record_owner_decision(
        root,
        OwnerDecisionRequest {
            command_kind: "verification adjudicate",
            owner_ref: &format!("work_unit:{owner}"),
            target_ref: &format!("closure_attempt:{attempt}"),
            decision_family: "verification",
            action: "adjudicate",
            decision_value: input.decision,
            reason: input.reason,
            expected_current: input.expected_current,
        },
    )
}
