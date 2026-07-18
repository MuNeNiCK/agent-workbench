use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

use crate::db::{open_existing_project, project_id};
use crate::identity::{CanonicalValue, DecisionHandle, domain_digest};

use super::decision_projection_support::*;

fn parse_target_id(value: &str, prefix: &str) -> Result<i64> {
    let id = value
        .strip_prefix(prefix)
        .context("decision target has the wrong tag")?
        .parse::<i64>()
        .context("decision target id is invalid")?;
    if id <= 0 {
        bail!("decision target id must be positive");
    }
    Ok(id)
}

#[derive(Clone, Debug)]
pub struct OwnerDecisionRequest<'a> {
    pub command_kind: &'a str,
    pub owner_ref: &'a str,
    pub target_ref: &'a str,
    pub decision_family: &'a str,
    pub action: &'a str,
    pub decision_value: &'a str,
    pub reason: &'a str,
    pub expected_current: &'a str,
}

#[derive(Clone, Debug)]
pub struct DecisionOutcome {
    pub decision_handle: String,
}

pub fn record_owner_decision(
    root: &Path,
    request: OwnerDecisionRequest<'_>,
) -> Result<DecisionOutcome> {
    let valid_action = matches!(
        (request.decision_family, request.action),
        ("review", "adjudicate" | "correct_terminal")
            | ("finding", "adjudicate" | "dispose" | "reopen")
            | ("verification", "adjudicate")
    );
    if !valid_action {
        bail!("unsupported decision family/action");
    }
    if request.owner_ref.trim().is_empty()
        || request.target_ref.trim().is_empty()
        || request.reason.trim().is_empty()
        || request.expected_current.trim().is_empty()
    {
        bail!("owner decision fields must not be empty");
    }
    let payload = CanonicalValue::object([
        ("command", CanonicalValue::string(request.command_kind)),
        ("owner", CanonicalValue::string(request.owner_ref)),
        ("target", CanonicalValue::string(request.target_ref)),
        ("family", CanonicalValue::string(request.decision_family)),
        ("action", CanonicalValue::string(request.action)),
        ("decision", CanonicalValue::string(request.decision_value)),
        ("reason", CanonicalValue::string(request.reason)),
        (
            "expected_current",
            CanonicalValue::string(request.expected_current),
        ),
    ]);
    let decision = DecisionHandle::derive(b"agent-workbench:owner-decision-v1\0", &payload);
    let payload_digest = domain_digest(b"agent-workbench:owner-decision-payload-v1\0", &payload);
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let project = project_id(&tx)?;
    if let Some(handle) = tx
        .query_row(
            "select decision_handle from owner_decisions where project_id=?1 and decision_handle=?2",
            params![project, decision.as_str()],
            |row| row.get(0),
        )
        .optional()?
    {
        tx.commit()?;
        return Ok(DecisionOutcome { decision_handle: handle });
    }
    let current = expected_current_for_target(&tx, project, &request)?;
    if (request.expected_current == "pending" && current.is_some())
        || (request.expected_current != "pending"
            && current.as_deref() != Some(request.expected_current))
    {
        bail!("expected_current_stale");
    }
    tx.execute(
        r#"insert into owner_decisions(project_id,decision_handle,capability_id,principal_id,owner_ref,target_ref,decision_family,action,decision_value,reason,expected_current,payload_digest,created_at)
           values(?1,?2,null,null,?3,?4,?5,?6,?7,?8,?9,?10,current_timestamp)"#,
        params![project,decision.as_str(),request.owner_ref,request.target_ref,request.decision_family,request.action,request.decision_value,request.reason,request.expected_current,payload_digest],
    )?;
    let owner_decision_id = tx.last_insert_rowid();
    apply_decision_projection(&tx, project, owner_decision_id, &request)?;
    tx.commit()?;
    Ok(DecisionOutcome {
        decision_handle: decision.as_str().to_string(),
    })
}

fn apply_decision_projection(
    conn: &rusqlite::Connection,
    project: i64,
    owner_decision_id: i64,
    request: &OwnerDecisionRequest<'_>,
) -> Result<()> {
    match request.decision_family {
        "review" => {
            if request.action == "correct_terminal" {
                return apply_review_correction(conn, project, owner_decision_id, request);
            }
            if !matches!(
                request.decision_value,
                "accepted" | "rejected" | "needs_evidence"
            ) {
                bail!("review adjudication decision is not allowed");
            }
            let run = parse_target_id(request.target_ref, "review_run:")?;
            let audit_only: i64 = conn.query_row(
                "select exists(select 1 from legacy_claim_audits where project_id=?1 and review_run_id=?2 and reviewer_resolution in ('unbound','ambiguous') and not exists(select 1 from legacy_signed_review_effects s where s.project_id=?1 and s.review_run_id=?2))",
                params![project, run],
                |row| row.get(0),
            )?;
            if audit_only == 1 {
                bail!("legacy_claim_audit_only");
            }
            conn.query_row(
                "select id from review_runs where project_id=?1 and id=?2 and status='completed'",
                params![project, run],
                |row| row.get::<_, i64>(0),
            )
            .context("review adjudication target is not a completed claim")?;
            if request.decision_value == "accepted" && !trusted_review_run(conn, project, run)? {
                bail!("review_claim_provenance_untrusted");
            }
            let predecessor:Option<i64>=conn.query_row("select id from review_adjudication_decisions where project_id=?1 and review_run_id=?2 order by id desc limit 1",params![project,run],|row|row.get(0)).optional()?;
            if let Some(predecessor_id) = predecessor {
                let prior: String = conn.query_row(
                    "select value from review_adjudication_decisions where id=?1",
                    params![predecessor_id],
                    |row| row.get(0),
                )?;
                if prior == request.decision_value {
                    bail!("same_value_supersession");
                }
            }
            conn.execute("insert into review_adjudication_decisions(project_id,owner_decision_id,review_run_id,value,predecessor_id,created_at) values(?1,?2,?3,?4,?5,current_timestamp)",params![project,owner_decision_id,run,request.decision_value,predecessor])?;
            let plan_id: i64 = conn.query_row(
                "select review_plan_id from review_runs where id=?1",
                params![run],
                |row| row.get(0),
            )?;
            if request.decision_value == "accepted" {
                let (required_fresh,required_resume):(i64,i64)=conn.query_row("select pol.required_consecutive_clean_fresh_runs,pol.required_consecutive_clean_resume_runs from review_plans p join review_policies pol on pol.id=p.review_policy_id where p.id=?1",params![plan_id],|row|Ok((row.get(0)?,row.get(1)?)))?;
                let fresh = count_trusted_accepted_runs(conn, project, plan_id, "fresh")?;
                let resume = count_trusted_accepted_runs(conn, project, plan_id, "resume")?;
                if fresh >= required_fresh && resume >= required_resume {
                    conn.execute("update review_plans set status='clean' where project_id=?1 and id=?2 and status in ('open','blocked')",params![project,plan_id])?;
                }
            } else {
                conn.execute("update review_plans set status='blocked' where project_id=?1 and id=?2 and status in ('open','clean')",params![project,plan_id])?;
            }
        }
        "finding" => {
            if request.action == "reopen" {
                return apply_finding_reopen(conn, project, owner_decision_id, request);
            }
            if !matches!(
                request.decision_value,
                "accepted"
                    | "rejected"
                    | "needs_evidence"
                    | "design_conflict"
                    | "deferred"
                    | "authority_disposed"
            ) {
                bail!("finding disposition is not allowed");
            }
            let finding = parse_target_id(request.target_ref, "finding:")?;
            let audit_only: i64 = conn.query_row(
                "select exists(select 1 from findings f join legacy_claim_audits l on l.project_id=f.project_id and l.review_run_id=f.review_run_id where f.project_id=?1 and f.id=?2 and l.reviewer_resolution in ('unbound','ambiguous'))",
                params![project, finding],
                |row| row.get(0),
            )?;
            if audit_only == 1 {
                bail!("legacy_finding_audit_only");
            }
            let current_state: String = conn
                .query_row(
                    "select lifecycle_state from findings where project_id=?1 and id=?2",
                    params![project, finding],
                    |row| row.get(0),
                )
                .context("finding disposition target not found")?;
            let predecessor:Option<i64>=conn.query_row("select id from finding_disposition_decisions where project_id=?1 and finding_id=?2 order by id desc limit 1",params![project,finding],|row|row.get(0)).optional()?;
            let mut invalidate_derived_permissions = false;
            if let Some(predecessor_id) = predecessor {
                let prior: String = conn.query_row(
                    "select value from finding_disposition_decisions where id=?1",
                    params![predecessor_id],
                    |row| row.get(0),
                )?;
                if prior == request.decision_value {
                    bail!("same_value_supersession");
                }
                if matches!(prior.as_str(), "rejected" | "authority_disposed") {
                    bail!("finding_epoch_terminal");
                }
                let closure: i64 = conn.query_row(
                    "select exists(select 1 from closures where project_id=?1 and finding_id=?2)",
                    params![project, finding],
                    |row| row.get(0),
                )?;
                if prior == "accepted" && closure == 1 {
                    invalidate_derived_permissions = true;
                }
            }
            conn.execute("insert into finding_disposition_decisions(project_id,owner_decision_id,finding_id,value,predecessor_id,created_at) values(?1,?2,?3,?4,?5,current_timestamp)",params![project,owner_decision_id,finding,request.decision_value,predecessor])?;
            match request.decision_value {
                "accepted" => {
                    conn.execute("update findings set classification='valid',status='open' where project_id=?1 and id=?2",params![project,finding])?;
                }
                "needs_evidence" => {
                    conn.execute("update findings set classification='needs_evidence',status='open' where project_id=?1 and id=?2",params![project,finding])?;
                }
                "design_conflict" => {
                    conn.execute("update findings set classification='design_conflict',status='open' where project_id=?1 and id=?2",params![project,finding])?;
                }
                "rejected" => {
                    conn.execute("update findings set classification='invalid' where project_id=?1 and id=?2",params![project,finding])?;
                }
                _ => {}
            }
            if invalidate_derived_permissions {
                conn.execute("update closure_attempts set result='superseded',resolved_at=current_timestamp where project_id=?1 and result is null and closure_id in(select id from closures where project_id=?1 and finding_id=?2)",params![project,finding])?;
                conn.execute("update closures set status='superseded',superseded_at=current_timestamp,supersession_reason='finding disposition invalidated derived permission' where project_id=?1 and finding_id=?2 and status!='superseded'",params![project,finding])?;
                conn.execute("update correction_sessions set status='superseded',completed_at=current_timestamp where project_id=?1 and finding_id=?2 and status='active'",params![project,finding])?;
                conn.execute("update correction_tokens set status='superseded' where project_id=?1 and status='pending' and closure_id in(select id from closures where project_id=?1 and finding_id=?2)",params![project,finding])?;
                let activations=conn.prepare("select distinct a.id,a.work_unit_id from finding_remediation_bindings b join work_unit_activations a on a.id=b.work_unit_activation_id where b.project_id=?1 and b.finding_id=?2 and a.status='active'")?.query_map(params![project,finding],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
                for (activation, work) in activations {
                    conn.execute("update work_unit_activations set status='suspended',suspended_at=current_timestamp where project_id=?1 and id=?2 and status='active'",params![project,activation])?;
                    conn.execute("update work_units set status='blocked' where project_id=?1 and id=?2 and status='open'",params![project,work])?;
                    conn.execute("insert into work_unit_events(work_unit_id,work_unit_activation_id,event_type,reason,status_domain,previous_status,next_status,created_at) values(?1,?2,'invalidated','finding disposition invalidated derived permission','activation','active','suspended',current_timestamp)",params![work,activation])?;
                }
            }
            if matches!(request.decision_value, "rejected" | "authority_disposed") {
                conn.execute("update findings set lifecycle_state='closed',status='closed',close_reason=?1 where project_id=?2 and id=?3",params![request.decision_value,project,finding])?;
                conn.execute("insert into finding_lifecycle_events(project_id,finding_id,owner_decision_id,from_state,to_state,effect,created_at) values(?1,?2,?3,?4,'closed',?5,current_timestamp)",params![project,finding,owner_decision_id,current_state,request.decision_value])?;
                let epoch:i64=conn.query_row("select coalesce(max(epoch_number),0)+1 from finding_decision_epochs where project_id=?1 and finding_id=?2",params![project,finding],|row|row.get(0))?;
                conn.execute("insert into finding_decision_epochs(project_id,finding_id,epoch_number,terminal_decision_id,status,created_at) values(?1,?2,?3,?4,'terminal',current_timestamp)",params![project,finding,epoch,owner_decision_id])?;
            } else if invalidate_derived_permissions {
                conn.execute("update findings set lifecycle_state='open',status='open',close_reason=null where project_id=?1 and id=?2",params![project,finding])?;
                conn.execute("insert into finding_lifecycle_events(project_id,finding_id,owner_decision_id,from_state,to_state,effect,created_at) values(?1,?2,?3,?4,'open','derived_permission_invalidated',current_timestamp)",params![project,finding,owner_decision_id,current_state])?;
            }
        }
        "verification" => {
            if !matches!(
                request.decision_value,
                "accepted" | "rejected" | "needs_evidence"
            ) {
                bail!("verification adjudication decision is not allowed");
            }
            let attempt = parse_target_id(request.target_ref, "closure_attempt:")?;
            let (closure_id,finding_id,current_state,claim,review_run):(i64,i64,String,String,i64)=conn.query_row(
                "select a.closure_id,c.finding_id,f.lifecycle_state,v.result,v.review_run_id from closure_attempts a join closures c on c.id=a.closure_id join findings f on f.id=c.finding_id join finding_verifications v on v.closure_attempt_id=a.id where a.project_id=?1 and a.id=?2 order by v.id desc limit 1",
                params![project, attempt],
                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)),
            )
            .context("verification attempt has no completed claim")?;
            if current_state != "awaiting_verification" {
                bail!("verification lifecycle is not current");
            }
            let predecessor:Option<i64>=conn.query_row("select id from verification_adjudication_decisions where project_id=?1 and closure_attempt_id=?2 order by id desc limit 1",params![project,attempt],|row|row.get(0)).optional()?;
            if let Some(predecessor_id) = predecessor {
                let prior: String = conn.query_row(
                    "select value from verification_adjudication_decisions where id=?1",
                    params![predecessor_id],
                    |row| row.get(0),
                )?;
                if prior == request.decision_value {
                    bail!("same_value_supersession");
                }
            }
            conn.execute("insert into verification_adjudication_decisions(project_id,owner_decision_id,closure_attempt_id,value,predecessor_id,created_at) values(?1,?2,?3,?4,?5,current_timestamp)",params![project,owner_decision_id,attempt,request.decision_value,predecessor])?;
            if request.decision_value == "accepted" {
                match claim.as_str() {
                    "verified" => {
                        conn.execute("update findings set lifecycle_state='closed',status='closed',close_reason='verified' where project_id=?1 and id=?2",params![project,finding_id])?;
                        conn.execute(
                            "update closures set status='verified' where project_id=?1 and id=?2",
                            params![project, closure_id],
                        )?;
                    }
                    "not_fixed" | "needs_evidence" => {
                        conn.execute("update findings set lifecycle_state='remediating' where project_id=?1 and id=?2",params![project,finding_id])?;
                        conn.execute(
                            "update closures set status='registered' where project_id=?1 and id=?2",
                            params![project, closure_id],
                        )?;
                        conn.execute("update correction_sessions set status='active',completed_at=null where id=(select max(id) from correction_sessions where project_id=?1 and closure_id=?2 and status='completed')",params![project,closure_id])?;
                    }
                    _ => bail!("unsupported verification claim"),
                }
                conn.execute("update closure_attempts set result=?1,resolved_at=current_timestamp where project_id=?2 and id=?3 and result is null",params![claim,project,attempt])?;
                let next = if claim == "verified" {
                    "closed"
                } else {
                    "remediating"
                };
                conn.execute("insert into finding_lifecycle_events(project_id,finding_id,owner_decision_id,from_state,to_state,effect,created_at) values(?1,?2,?3,?4,?5,?6,current_timestamp)",params![project,finding_id,owner_decision_id,current_state,next,format!("verification_{claim}_accepted")])?;
                let fresh_watermark: i64 = conn.query_row(
                    "select coalesce(max(id),0) from review_runs where project_id=?1",
                    params![project],
                    |row| row.get(0),
                )?;
                conn.execute("update review_plans set fresh_review_after_run_id=?1 where project_id=?2 and id=(select review_plan_id from review_runs where id=?3)",params![fresh_watermark,project,review_run])?;
                crate::review::refresh_plan_for_run(conn, project, review_run)?;
            }
        }
        _ => bail!("unsupported decision family"),
    }
    Ok(())
}

fn trusted_review_run(conn: &rusqlite::Connection, project: i64, run: i64) -> Result<bool> {
    Ok(conn.query_row(
        r#"select
              (trim(coalesce(r.review_provenance_ref,''))!='' and (
                   r.review_provenance='human_review'
                   or (r.review_provenance='external_agent' and exists(
                       select 1 from review_agent_invocations i
                       where i.project_id=r.project_id and i.review_run_id=r.id
                         and trim(coalesce(i.external_agent_id,''))!=''
                   ))
               ))
               or exists(
                   select 1 from legacy_claim_audits l
                   where l.project_id=r.project_id and l.review_run_id=r.id
                     and l.reviewer_resolution='trusted'
               ) or exists(
                   select 1 from legacy_signed_review_effects s
                   where s.project_id=r.project_id and s.review_run_id=r.id
               )
           from review_runs r where r.project_id=?1 and r.id=?2"#,
        params![project, run],
        |row| row.get::<_, i64>(0),
    )? == 1)
}

fn count_trusted_accepted_runs(
    conn: &rusqlite::Connection,
    project: i64,
    plan: i64,
    run_type: &str,
) -> Result<i64> {
    let runs = conn.prepare(
        r#"select r.id from review_runs r
           where r.project_id=?1 and r.review_plan_id=?2 and r.run_type=?3
             and r.status='completed' and r.clean_run=1
             and exists(select 1 from review_adjudication_decisions d
                        where d.review_run_id=r.id and d.value='accepted'
                          and not exists(select 1 from review_adjudication_decisions n where n.predecessor_id=d.id))
           order by r.id"#,
    )?.query_map(params![project, plan, run_type], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut count = 0;
    for run in runs {
        if trusted_review_run(conn, project, run)? {
            count += 1;
        }
    }
    Ok(count)
}
