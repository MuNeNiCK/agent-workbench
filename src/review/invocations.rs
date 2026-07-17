use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{open_existing_project, project_id};
use crate::identity::{CanonicalValue, InvocationHandle, PrincipalHandle, domain_digest};

use super::{InvocationState, invocation_transition};

#[derive(Clone, Debug)]
pub struct InvocationRequest<'a> {
    pub review_plan_id: i64,
    pub target_context: &'a str,
    pub reviewer: &'a str,
    pub idempotency_key: &'a str,
    pub provenance: &'a str,
    pub purpose: &'a str,
    pub expected_plan_current: &'a str,
}
#[derive(Clone, Debug)]
pub struct InvocationOutcome {
    pub invocation_id: i64,
    pub invocation_handle: String,
    pub state: String,
}

pub fn request_invocation(
    root: &Path,
    request: InvocationRequest<'_>,
) -> Result<InvocationOutcome> {
    if request.expected_plan_current != "open" || request.idempotency_key.is_empty() {
        bail!(
            "invocation request requires expected-plan-current open and a nonempty idempotency key"
        );
    }
    if !matches!(
        request.purpose,
        "new_unbiased_review" | "finding_fix_verification"
    ) {
        bail!("unsupported invocation purpose");
    }
    let reviewer = PrincipalHandle::parse(request.reviewer)?;
    let payload = CanonicalValue::object([
        ("plan", CanonicalValue::Integer(request.review_plan_id)),
        ("target", CanonicalValue::string(request.target_context)),
        ("reviewer", CanonicalValue::string(reviewer.as_str())),
        ("provenance", CanonicalValue::string(request.provenance)),
        ("purpose", CanonicalValue::string(request.purpose)),
        ("key", CanonicalValue::string(request.idempotency_key)),
    ]);
    let handle = InvocationHandle::derive(b"agent-workbench:review-invocation-v1\0", &payload);
    let request_payload_digest =
        domain_digest(b"agent-workbench:review-invocation-request-v1\0", &payload);
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    let (principal_id,): (i64,) = tx
        .query_row(
            "select id from authority_principals where project_id=?1 and principal_handle=?2",
            params![project, reviewer.as_str()],
            |row| Ok((row.get(0)?,)),
        )
        .context("invocation reviewer principal is not resolved")?;
    let existing:Option<(i64,String,String,String)>=tx.query_row("select id,invocation_handle,status,request_payload_digest from review_agent_invocations where project_id=?1 and review_plan_id=?2 and target_context=?3 and reviewer_principal_id=?4 and request_idempotency_key=?5",params![project,request.review_plan_id,request.target_context,principal_id,request.idempotency_key],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?;
    if let Some((id, stored, state, stored_digest)) = existing {
        if stored_digest != request_payload_digest || stored != handle.as_str() {
            bail!("invocation_request_idempotency_payload_mismatch");
        }
        return Ok(InvocationOutcome {
            invocation_id: id,
            invocation_handle: stored,
            state,
        });
    }
    let (provenance_id,plan_id,target,purpose):(i64,i64,String,String)=tx.query_row("select id,review_plan_id,target_context,review_purpose from review_provenance_records where project_id=?1 and provenance_handle=?2 and principal_id=?3",params![project,request.provenance,principal_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).context("trusted provenance does not bind this reviewer")?;
    if plan_id != request.review_plan_id
        || target != request.target_context
        || purpose != request.purpose
    {
        bail!("trusted provenance does not bind invocation context");
    }
    validate_invocation_plan_context(
        &tx,
        project,
        request.review_plan_id,
        request.target_context,
        request.purpose,
    )?;
    let plan_status: String = tx
        .query_row(
            "select status from review_plans where project_id=?1 and id=?2",
            params![project, request.review_plan_id],
            |row| row.get(0),
        )
        .context("review plan not found")?;
    if plan_status != "open" {
        bail!("review plan is not current open");
    }
    tx.execute("insert into review_agent_invocations(project_id,review_plan_id,invocation_handle,reviewer_principal_id,review_provenance_id,target_context,purpose,request_idempotency_key,request_payload_digest,run_type,status) values(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'requested')",params![project,request.review_plan_id,handle.as_str(),principal_id,provenance_id,request.target_context,request.purpose,request.idempotency_key,request_payload_digest,if request.purpose=="new_unbiased_review"{"fresh"}else{"resume"}])?;
    let id = tx.last_insert_rowid();
    tx.commit()?;
    Ok(InvocationOutcome {
        invocation_id: id,
        invocation_handle: handle.as_str().to_string(),
        state: "requested".into(),
    })
}

pub(crate) fn validate_invocation_plan_context(
    conn: &rusqlite::Connection,
    project: i64,
    plan_id: i64,
    target: &str,
    purpose: &str,
) -> Result<()> {
    let (status,design,work,stage,review_type,allow_fresh,allow_resume,max_fresh,max_resume):(String,Option<i64>,i64,String,String,i64,i64,i64,i64)=conn.query_row("select p.status,p.design_version_id,p.work_unit_id,p.stage,p.review_type,pol.allow_fresh_review,pol.allow_resume_review,pol.max_fresh_agents,pol.max_resume_agents from review_plans p join review_policies pol on pol.id=p.review_policy_id where p.project_id=?1 and p.id=?2",params![project,plan_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?))).context("review plan not found")?;
    if status != "open" {
        bail!("review plan is not current open");
    }
    match purpose {
        "new_unbiased_review" => {
            if allow_fresh != 1 {
                bail!("review policy disallows fresh review");
            }
            let active:i64=conn.query_row("select count(*) from review_agent_invocations where project_id=?1 and review_plan_id=?2 and run_type='fresh' and status in ('requested','running')",params![project,plan_id],|row|row.get(0))?;
            if active >= max_fresh {
                bail!("review policy fresh invocation limit reached");
            }
            if let (Some(design), Some(kind)) = (design, review_context_kind(&stage, &review_type))
            {
                let expected =
                    crate::review_context::review_context_ref(kind, Some(design), Some(work));
                if target != expected {
                    bail!("invocation target does not match the current plan review context");
                }
            } else if target.is_empty() {
                bail!("invocation target context is empty");
            }
        }
        "finding_fix_verification" => {
            if allow_resume != 1 {
                bail!("review policy disallows resume verification");
            }
            let active:i64=conn.query_row("select count(*) from review_agent_invocations where project_id=?1 and review_plan_id=?2 and run_type='resume' and status in ('requested','running')",params![project,plan_id],|row|row.get(0))?;
            if active >= max_resume.max(1) {
                bail!("review policy resume invocation limit reached");
            }
            let valid:i64=conn.query_row("select exists(select 1 from closure_attempts a join closures c on c.id=a.closure_id join findings f on f.id=c.finding_id where a.project_id=?1 and a.result is null and f.lifecycle_state='awaiting_verification' and ?2='review-context:finding-fix:finding='||f.id||':closure='||c.id||':attempt='||a.id)",params![project,target],|row|row.get(0))?;
            if valid != 1 {
                bail!("verification invocation target is not an exact current closure attempt");
            }
        }
        _ => bail!("unsupported review purpose"),
    }
    Ok(())
}

fn review_context_kind(stage: &str, review_type: &str) -> Option<&'static str> {
    match (stage, review_type) {
        ("design-ready", "design_review") => Some("design-review"),
        ("implementation-ready", "design_task_decomposition") => Some("design-task-decomposition"),
        ("close-ready", "design_implementation_diff") => Some("design-implementation-diff"),
        ("close-ready", "implementation_review") => Some("implementation-review"),
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub struct InvocationTransitionRequest<'a> {
    pub invocation_id: i64,
    pub principal: &'a str,
    pub expected_current: &'a str,
    pub idempotency_key: &'a str,
    pub outcome: InvocationTerminal<'a>,
}
#[derive(Clone, Debug)]
pub enum InvocationTerminal<'a> {
    Start,
    CompleteReview {
        claim: &'a str,
        summary: &'a str,
    },
    CompleteVerification {
        claim: &'a str,
        attempt: i64,
        summary: &'a str,
    },
    Fail {
        reason: &'a str,
    },
    Cancel {
        reason: &'a str,
    },
}

pub fn transition_invocation(
    root: &Path,
    request: InvocationTransitionRequest<'_>,
) -> Result<InvocationOutcome> {
    let principal = PrincipalHandle::parse(request.principal)?;
    let expected = InvocationState::parse(request.expected_current)?;
    let (next, claim, verification, attempt, summary, reason, command) = match request.outcome {
        InvocationTerminal::Start => (
            InvocationState::Running,
            None,
            None,
            None,
            None,
            None,
            "start",
        ),
        InvocationTerminal::CompleteReview { claim, summary } => {
            if !matches!(claim, "clean" | "inconclusive") {
                bail!("review completion claim must be clean or inconclusive");
            }
            (
                InvocationState::Completed,
                Some(claim),
                None,
                None,
                Some(summary),
                None,
                "complete",
            )
        }
        InvocationTerminal::CompleteVerification {
            claim,
            attempt,
            summary,
        } => {
            if !matches!(claim, "verified" | "not_fixed" | "needs_evidence") || attempt <= 0 {
                bail!("verification completion requires a closed claim and positive attempt");
            }
            (
                InvocationState::Completed,
                None,
                Some(claim),
                Some(attempt),
                Some(summary),
                None,
                "complete",
            )
        }
        InvocationTerminal::Fail { reason } => (
            InvocationState::Failed,
            None,
            None,
            None,
            None,
            Some(reason),
            "fail",
        ),
        InvocationTerminal::Cancel { reason } => (
            InvocationState::Cancelled,
            None,
            None,
            None,
            None,
            Some(reason),
            "cancel",
        ),
    };
    invocation_transition(expected, next)?;
    if request.idempotency_key.is_empty() {
        bail!("transition idempotency key must not be empty");
    }
    let transition_payload = CanonicalValue::object([
        ("invocation", CanonicalValue::Integer(request.invocation_id)),
        ("principal", CanonicalValue::string(principal.as_str())),
        ("expected", CanonicalValue::string(request.expected_current)),
        ("command", CanonicalValue::string(command)),
        (
            "claim",
            claim.map_or(CanonicalValue::Null, CanonicalValue::string),
        ),
        (
            "verification",
            verification.map_or(CanonicalValue::Null, CanonicalValue::string),
        ),
        (
            "attempt",
            attempt.map_or(CanonicalValue::Null, CanonicalValue::Integer),
        ),
        (
            "summary",
            summary.map_or(CanonicalValue::Null, CanonicalValue::string),
        ),
        (
            "reason",
            reason.map_or(CanonicalValue::Null, CanonicalValue::string),
        ),
    ]);
    let transition_digest = domain_digest(
        b"agent-workbench:review-invocation-transition-v1\0",
        &transition_payload,
    );
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    let principal_id: i64 = tx
        .query_row(
            "select id from authority_principals where project_id=?1 and principal_handle=?2",
            params![project, principal.as_str()],
            |row| row.get(0),
        )
        .context("invocation principal is not resolved")?;
    let (handle,holder,current,purpose):(String,i64,String,String)=tx.query_row("select invocation_handle,reviewer_principal_id,status,purpose from review_agent_invocations where project_id=?1 and id=?2",params![project,request.invocation_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).context("review invocation not found")?;
    let prior:Option<(String,String)>=tx.query_row("select payload_digest,resulting_state from review_invocation_transition_audits where project_id=?1 and command=?2 and invocation_id=?3 and principal_id=?4 and idempotency_key=?5",params![project,command,request.invocation_id,principal_id,request.idempotency_key],|row|Ok((row.get(0)?,row.get(1)?))).optional()?;
    if let Some((digest, state)) = prior {
        if digest != transition_digest {
            bail!("invocation_transition_idempotency_payload_mismatch");
        }
        return Ok(InvocationOutcome {
            invocation_id: request.invocation_id,
            invocation_handle: handle,
            state,
        });
    }
    if holder != principal_id {
        bail!("invocation principal mismatch");
    }
    if current != expected.as_str() {
        bail!("invocation expected-current is stale");
    }
    if next != InvocationState::Running
        && tx.query_row("select exists(select 1 from review_result_stages where project_id=?1 and invocation_id=?2 and status='staging')",params![project,request.invocation_id],|row|row.get::<_,i64>(0))? == 1
    {
        bail!("active_result_stage");
    }
    if next == InvocationState::Completed
        && ((purpose == "new_unbiased_review") != claim.is_some()
            || (purpose == "finding_fix_verification") != verification.is_some())
    {
        bail!("completion kind does not match invocation purpose");
    }
    let changed=tx.execute("update review_agent_invocations set status=?1,transition_idempotency_key=?2,claim=?3,verification_claim=?4,closure_attempt_id=?5,result_summary=?6,terminal_reason=?7,started_at=case when ?1='running' then current_timestamp else started_at end,finished_at=case when ?1 in ('completed','failed','cancelled') then current_timestamp else null end where project_id=?8 and id=?9 and status=?10",params![next.as_str(),request.idempotency_key,claim,verification,attempt,summary,reason,project,request.invocation_id,expected.as_str()])?;
    if changed != 1 {
        bail!("concurrent invocation transition lost");
    }
    let mut created_run_id = None;
    if next == InvocationState::Completed {
        let (scope_id,work_unit_id,provenance_kind,provenance_handle):(Option<i64>,i64,String,String)=tx.query_row(
            "select p.review_scope_id,p.work_unit_id,pr.provenance_kind,pr.provenance_handle from review_plans p join review_provenance_records pr on pr.id=(select review_provenance_id from review_agent_invocations where id=?1) where p.id=(select review_plan_id from review_agent_invocations where id=?1)",
            params![request.invocation_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)))?;
        let clean = if claim == Some("clean") { 1 } else { 0 };
        tx.execute("insert into review_runs(project_id,review_scope_id,review_plan_id,run_type,run_purpose,target_type,work_unit_id,target_ref,result_summary,new_findings_count,carried_findings_checked,clean_run,review_provenance,review_provenance_ref,status,created_at) select project_id,?1,review_plan_id,run_type,purpose,'work_unit',?2,target_context,?3,0,0,?4,?5,?6,'completed',current_timestamp from review_agent_invocations where id=?7",
            params![scope_id,work_unit_id,summary,clean,if provenance_kind=="human_review"{"human_review"}else{"external_agent"},provenance_handle,request.invocation_id])?;
        let run_id = tx.last_insert_rowid();
        created_run_id = Some(run_id);
        tx.execute(
            "update review_agent_invocations set review_run_id=?1 where id=?2",
            params![run_id, request.invocation_id],
        )?;
        if let (Some(verification), Some(attempt)) = (verification, attempt) {
            let (closure_id,finding_id):(i64,i64)=tx.query_row("select a.closure_id,c.finding_id from closure_attempts a join closures c on c.id=a.closure_id where a.project_id=?1 and a.id=?2",params![project,attempt],|row|Ok((row.get(0)?,row.get(1)?))).context("verification attempt does not exist")?;
            tx.execute("insert into finding_verifications(project_id,review_run_id,finding_id,closure_id,closure_attempt_id,result,notes,created_at) values(?1,?2,?3,?4,?5,?6,?7,current_timestamp)",params![project,run_id,finding_id,closure_id,attempt,verification,summary])?;
        }
    }
    tx.execute("insert into review_invocation_transition_audits(project_id,invocation_id,principal_id,command,idempotency_key,payload_digest,resulting_state,review_run_id,created_at) values(?1,?2,?3,?4,?5,?6,?7,?8,current_timestamp)",params![project,request.invocation_id,principal_id,command,request.idempotency_key,transition_digest,next.as_str(),created_run_id])?;
    tx.commit()?;
    Ok(InvocationOutcome {
        invocation_id: request.invocation_id,
        invocation_handle: handle,
        state: next.as_str().into(),
    })
}
