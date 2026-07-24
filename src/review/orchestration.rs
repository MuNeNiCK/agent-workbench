use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, Transaction, params};

use crate::db::{open_existing_project, project_id};
use crate::identity::{
    CanonicalValue, InvocationHandle, ReviewProvenanceHandle, ReviewResultItemHandle,
    ReviewResultStageHandle, ReviewResultVersionHandle, domain_digest,
};

use super::FindingTargetInput;

#[derive(Clone, Debug)]
pub struct ReviewProvenanceIssue<'a> {
    pub reviewer_ref: &'a str,
    pub review_plan_id: i64,
    pub target_context: &'a str,
    pub provenance_kind: &'a str,
    pub purpose: &'a str,
    pub source_reference: &'a str,
    pub idempotency_key: &'a str,
}

#[derive(Clone, Debug)]
pub struct ReviewProvenanceOutcome {
    pub provenance_handle: String,
    pub already_recorded: bool,
}

#[derive(Clone, Debug)]
pub struct InvocationRequest<'a> {
    pub review_plan_id: i64,
    pub target_context: &'a str,
    pub reviewer_ref: &'a str,
    pub provenance_handle: &'a str,
    pub purpose: &'a str,
    pub idempotency_key: &'a str,
    pub expected_plan_current: &'a str,
}

#[derive(Clone, Debug)]
pub struct InvocationOutcome {
    pub invocation_id: i64,
    pub invocation_handle: String,
    pub state: String,
    pub review_run_id: Option<i64>,
    pub already_applied: bool,
}

#[derive(Clone, Debug)]
pub struct InvocationTransitionRequest<'a> {
    pub invocation_id: i64,
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

#[derive(Clone, Debug)]
pub struct ResultStageOutcome {
    pub stage_handle: String,
    pub version_handle: String,
    pub state: String,
    pub result_handle: Option<String>,
    pub already_applied: bool,
}

pub struct CreateResultStageRequest<'a> {
    pub invocation_id: i64,
    pub expected_current: &'a str,
    pub idempotency_key: &'a str,
}

pub struct AddResultFindingRequest<'a> {
    pub stage_handle: &'a str,
    pub finding_type: &'a str,
    pub severity: &'a str,
    pub description: &'a str,
    pub requirement: Option<i64>,
    pub task: Option<i64>,
    pub expected_current: &'a str,
    pub idempotency_key: &'a str,
}

pub struct AddResultFindingWithTargetsRequest<'a> {
    pub stage_handle: &'a str,
    pub finding_type: &'a str,
    pub severity: &'a str,
    pub description: &'a str,
    pub targets: &'a [FindingTargetInput],
    pub expected_current: &'a str,
    pub idempotency_key: &'a str,
}

pub struct CompleteResultStageRequest<'a> {
    pub stage_handle: &'a str,
    pub expected_findings: i64,
    pub summary: &'a str,
    pub expected_current: &'a str,
    pub invocation_current: &'a str,
    pub idempotency_key: &'a str,
}

pub struct CancelResultStageRequest<'a> {
    pub stage_handle: &'a str,
    pub reason: &'a str,
    pub expected_current: &'a str,
    pub idempotency_key: &'a str,
}

struct PlanContext {
    id: i64,
    status: String,
    design_version_id: Option<i64>,
    work_unit_id: i64,
    stage: String,
    review_type: String,
    review_scope_id: Option<i64>,
    allow_fresh: bool,
    allow_resume: bool,
    max_fresh: i64,
    max_resume: i64,
    max_parallel: i64,
    run_count_scope: String,
    fresh_review_after_run_id: i64,
}

struct RunPublication<'a> {
    project: i64,
    invocation: i64,
    run_type: &'a str,
    purpose: &'a str,
    summary: &'a str,
    clean: i64,
    findings: i64,
    finding_result: Option<&'a str>,
}

pub fn issue_review_provenance(
    root: &Path,
    request: ReviewProvenanceIssue<'_>,
) -> Result<ReviewProvenanceOutcome> {
    require_text(request.reviewer_ref, "reviewer reference")?;
    require_text(request.target_context, "review target")?;
    require_text(request.source_reference, "review source reference")?;
    require_key(request.idempotency_key)?;
    if !matches!(request.provenance_kind, "external_agent" | "human_review") {
        bail!("review provenance kind must be external_agent or human_review");
    }
    validate_purpose(request.purpose)?;
    let payload = CanonicalValue::object([
        ("plan", CanonicalValue::Integer(request.review_plan_id)),
        ("target", CanonicalValue::string(request.target_context)),
        ("reviewer", CanonicalValue::string(request.reviewer_ref)),
        ("kind", CanonicalValue::string(request.provenance_kind)),
        ("purpose", CanonicalValue::string(request.purpose)),
        (
            "reference",
            CanonicalValue::string(request.source_reference),
        ),
    ]);
    let digest = domain_digest(b"agent-workbench:review-provenance-claim-v1\0", &payload);
    let handle = ReviewProvenanceHandle::derive(
        b"agent-workbench:review-provenance-v1\0",
        &CanonicalValue::object([
            ("payload", payload),
            ("key", CanonicalValue::string(request.idempotency_key)),
        ]),
    );
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    let plan = load_plan(&tx, project, request.review_plan_id)?;
    validate_plan_target(&tx, project, &plan, request.target_context, request.purpose)?;
    let existing: Option<(String, String)> = tx
        .query_row(
            "select provenance_handle,payload_digest from review_provenance_claims where project_id=?1 and review_plan_id=?2 and target_context=?3 and reviewer_ref=?4 and idempotency_key=?5",
            params![project, request.review_plan_id, request.target_context, request.reviewer_ref, request.idempotency_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    if let Some((stored, stored_digest)) = existing {
        if stored != handle.as_str() || stored_digest != digest {
            bail!("review provenance idempotency key was used with a different request");
        }
        return Ok(ReviewProvenanceOutcome {
            provenance_handle: stored,
            already_recorded: true,
        });
    }
    tx.execute(
        "insert into review_provenance_claims(project_id,provenance_handle,reviewer_ref,review_plan_id,target_context,provenance_kind,review_purpose,source_reference,idempotency_key,payload_digest,created_at) values(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,current_timestamp)",
        params![project, handle.as_str(), request.reviewer_ref, request.review_plan_id, request.target_context, request.provenance_kind, request.purpose, request.source_reference, request.idempotency_key, digest],
    )?;
    tx.commit()?;
    Ok(ReviewProvenanceOutcome {
        provenance_handle: handle.as_str().to_string(),
        already_recorded: false,
    })
}

pub fn request_invocation(
    root: &Path,
    request: InvocationRequest<'_>,
) -> Result<InvocationOutcome> {
    if request.expected_plan_current != "open" {
        bail!("invocation request expected plan state must be open");
    }
    require_text(request.reviewer_ref, "reviewer reference")?;
    require_text(request.target_context, "review target")?;
    require_key(request.idempotency_key)?;
    validate_purpose(request.purpose)?;
    ReviewProvenanceHandle::parse(request.provenance_handle)?;
    let payload = CanonicalValue::object([
        ("plan", CanonicalValue::Integer(request.review_plan_id)),
        ("target", CanonicalValue::string(request.target_context)),
        ("reviewer", CanonicalValue::string(request.reviewer_ref)),
        (
            "provenance",
            CanonicalValue::string(request.provenance_handle),
        ),
        ("purpose", CanonicalValue::string(request.purpose)),
    ]);
    let digest = domain_digest(b"agent-workbench:review-invocation-request-v1\0", &payload);
    let handle = InvocationHandle::derive(
        b"agent-workbench:review-invocation-v1\0",
        &CanonicalValue::object([
            ("payload", payload),
            ("key", CanonicalValue::string(request.idempotency_key)),
        ]),
    );
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    let existing: Option<(i64, String, String, String, Option<i64>)> = tx
        .query_row(
            "select id,invocation_handle,status,request_payload_digest,review_run_id from review_agent_invocations where project_id=?1 and review_plan_id=?2 and target_context=?3 and external_agent_id=?4 and request_idempotency_key=?5",
            params![project, request.review_plan_id, request.target_context, request.reviewer_ref, request.idempotency_key],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .optional()?;
    if let Some((id, stored, state, stored_digest, run)) = existing {
        if stored != handle.as_str() || stored_digest != digest {
            bail!("review invocation idempotency key was used with a different request");
        }
        return Ok(InvocationOutcome {
            invocation_id: id,
            invocation_handle: stored,
            state,
            review_run_id: run,
            already_applied: true,
        });
    }
    let plan = load_plan(&tx, project, request.review_plan_id)?;
    validate_plan_target(&tx, project, &plan, request.target_context, request.purpose)?;
    enforce_invocation_limit(&tx, project, &plan, request.purpose, request.target_context)?;
    let provenance: Option<i64> = tx
        .query_row(
            "select id from review_provenance_claims where project_id=?1 and provenance_handle=?2 and reviewer_ref=?3 and review_plan_id=?4 and target_context=?5 and review_purpose=?6",
            params![project, request.provenance_handle, request.reviewer_ref, request.review_plan_id, request.target_context, request.purpose],
            |row| row.get(0),
        )
        .optional()?;
    if provenance.is_none() {
        bail!("review provenance does not bind this reviewer and review target");
    }
    let run_type = if request.purpose == "new_unbiased_review" {
        "fresh"
    } else {
        "resume"
    };
    tx.execute(
        "insert into review_agent_invocations(project_id,review_plan_id,invocation_handle,provenance_handle,target_context,purpose,request_idempotency_key,request_payload_digest,run_type,external_agent_id,status) values(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'requested')",
        params![project, request.review_plan_id, handle.as_str(), request.provenance_handle, request.target_context, request.purpose, request.idempotency_key, digest, run_type, request.reviewer_ref],
    )?;
    let invocation_id = tx.last_insert_rowid();
    tx.commit()?;
    Ok(InvocationOutcome {
        invocation_id,
        invocation_handle: handle.as_str().to_string(),
        state: "requested".to_string(),
        review_run_id: None,
        already_applied: false,
    })
}

pub fn transition_invocation(
    root: &Path,
    request: InvocationTransitionRequest<'_>,
) -> Result<InvocationOutcome> {
    require_key(request.idempotency_key)?;
    let (command, next, claim, verification, attempt, summary, reason) = match request.outcome {
        InvocationTerminal::Start => ("start", "running", None, None, None, None, None),
        InvocationTerminal::CompleteReview { claim, summary } => {
            if !matches!(claim, "clean" | "inconclusive") {
                bail!("review completion claim must be clean or inconclusive");
            }
            require_text(summary, "review summary")?;
            (
                "complete",
                "completed",
                Some(claim),
                None,
                None,
                Some(summary),
                None,
            )
        }
        InvocationTerminal::CompleteVerification {
            claim,
            attempt,
            summary,
        } => {
            if !matches!(claim, "verified" | "not_fixed" | "needs_evidence") || attempt <= 0 {
                bail!("verification completion requires a typed result and positive attempt");
            }
            require_text(summary, "verification summary")?;
            (
                "complete",
                "completed",
                None,
                Some(claim),
                Some(attempt),
                Some(summary),
                None,
            )
        }
        InvocationTerminal::Fail { reason } => {
            require_text(reason, "failure reason")?;
            ("fail", "failed", None, None, None, None, Some(reason))
        }
        InvocationTerminal::Cancel { reason } => {
            require_text(reason, "cancellation reason")?;
            ("cancel", "cancelled", None, None, None, None, Some(reason))
        }
    };
    require_invocation_transition(request.expected_current, next)?;
    let payload = CanonicalValue::object([
        ("invocation", CanonicalValue::Integer(request.invocation_id)),
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
    let digest = domain_digest(b"agent-workbench:review-invocation-event-v1\0", &payload);
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    let (handle, current, purpose, run_type, target_context, existing_run): (
        String,
        String,
        String,
        String,
        String,
        Option<i64>,
    ) = tx
        .query_row(
            "select invocation_handle,status,purpose,run_type,target_context,review_run_id from review_agent_invocations where project_id=?1 and id=?2",
            params![project, request.invocation_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .context("review invocation not found")?;
    let prior: Option<(String, String, Option<i64>)> = tx
        .query_row(
            "select payload_digest,resulting_state,review_run_id from review_invocation_events where project_id=?1 and invocation_id=?2 and command=?3 and idempotency_key=?4",
            params![project, request.invocation_id, command, request.idempotency_key],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    if let Some((stored_digest, state, run)) = prior {
        if stored_digest != digest {
            bail!("review invocation idempotency key was used with a different transition");
        }
        return Ok(InvocationOutcome {
            invocation_id: request.invocation_id,
            invocation_handle: handle,
            state,
            review_run_id: run,
            already_applied: true,
        });
    }
    if current != request.expected_current {
        bail!("review invocation changed; inspect the invocation and retry from its current state");
    }
    if next != "running"
        && tx.query_row(
            "select exists(select 1 from review_result_drafts where project_id=?1 and invocation_id=?2 and status='staging')",
            params![project, request.invocation_id],
            |row| row.get::<_, i64>(0),
        )? == 1
    {
        bail!("review invocation has an active result stage");
    }
    if next == "completed"
        && ((purpose == "new_unbiased_review") != claim.is_some()
            || (purpose == "finding_fix_verification") != verification.is_some())
    {
        bail!("review completion result does not match the invocation purpose");
    }
    let verification_binding = if let (Some(result), Some(attempt_id)) = (verification, attempt) {
        let (closure_id, finding_id): (i64, i64) = tx
            .query_row(
                "select a.closure_id,c.finding_id from closure_attempts a join closures c on c.id=a.closure_id where a.project_id=?1 and a.id=?2",
                params![project, attempt_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .context("verification attempt does not exist")?;
        let expected_target = format!(
            "review-context:finding-fix:finding={finding_id}:closure={closure_id}:attempt={attempt_id}"
        );
        if target_context != expected_target {
            bail!("verification attempt does not match the invocation target");
        }
        Some((result, attempt_id, closure_id, finding_id))
    } else {
        None
    };
    let mut review_run_id = existing_run;
    if next == "completed" {
        review_run_id = Some(insert_review_run_for_invocation(
            &tx,
            RunPublication {
                project,
                invocation: request.invocation_id,
                run_type: &run_type,
                purpose: &purpose,
                summary: summary.context("completed invocation has no summary")?,
                clean: if claim == Some("clean") { 1 } else { 0 },
                findings: 0,
                finding_result: verification,
            },
        )?);
        if let Some((result, attempt_id, closure_id, finding_id)) = verification_binding {
            tx.execute(
                "insert into finding_verifications(project_id,review_run_id,finding_id,closure_id,closure_attempt_id,result,notes,created_at) values(?1,?2,?3,?4,?5,?6,?7,current_timestamp)",
                params![project, review_run_id, finding_id, closure_id, attempt_id, result, summary],
            )?;
        }
    }
    let changed = tx.execute(
        "update review_agent_invocations set status=?1,transition_idempotency_key=?2,claim=?3,verification_claim=?4,closure_attempt_id=?5,result_summary=?6,terminal_reason=?7,review_run_id=?8,started_at=case when ?1='running' then current_timestamp else started_at end,finished_at=case when ?1 in ('completed','failed','cancelled') then current_timestamp else null end where project_id=?9 and id=?10 and status=?11",
        params![next, request.idempotency_key, claim, verification, attempt, summary, reason, review_run_id, project, request.invocation_id, request.expected_current],
    )?;
    if changed != 1 {
        bail!("concurrent review invocation transition lost");
    }
    tx.execute(
        "insert into review_invocation_events(project_id,invocation_id,command,idempotency_key,payload_digest,resulting_state,review_run_id,created_at) values(?1,?2,?3,?4,?5,?6,?7,current_timestamp)",
        params![project, request.invocation_id, command, request.idempotency_key, digest, next, review_run_id],
    )?;
    tx.commit()?;
    Ok(InvocationOutcome {
        invocation_id: request.invocation_id,
        invocation_handle: handle,
        state: next.to_string(),
        review_run_id,
        already_applied: false,
    })
}

pub fn create_result_stage(
    root: &Path,
    request: CreateResultStageRequest<'_>,
) -> Result<ResultStageOutcome> {
    if !matches!(request.expected_current, "requested" | "running") {
        bail!("result stage requires a current requested or running invocation");
    }
    require_key(request.idempotency_key)?;
    let payload = CanonicalValue::object([
        ("invocation", CanonicalValue::Integer(request.invocation_id)),
        ("expected", CanonicalValue::string(request.expected_current)),
    ]);
    let digest = domain_digest(b"agent-workbench:review-result-stage-create-v2\0", &payload);
    let handle = ReviewResultStageHandle::derive(
        b"agent-workbench:review-result-stage-v2\0",
        &CanonicalValue::object([
            ("payload", payload),
            ("key", CanonicalValue::string(request.idempotency_key)),
        ]),
    );
    let version = result_version_handle(handle.as_str(), 0);
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    let existing: Option<(String, String, String, String)> = tx
        .query_row(
            "select draft_handle,version_handle,status,create_payload_digest from review_result_drafts where project_id=?1 and invocation_id=?2 and create_idempotency_key=?3",
            params![project, request.invocation_id, request.idempotency_key],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?;
    if let Some((stored, stored_version, state, stored_digest)) = existing {
        if stored != handle.as_str() || stored_digest != digest {
            bail!("review result idempotency key was used with a different stage request");
        }
        return Ok(ResultStageOutcome {
            stage_handle: stored,
            version_handle: stored_version,
            state,
            result_handle: None,
            already_applied: true,
        });
    }
    let (reviewer, status, purpose): (String, String, String) = tx
        .query_row(
            "select external_agent_id,status,purpose from review_agent_invocations where project_id=?1 and id=?2",
            params![project, request.invocation_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .context("review invocation not found")?;
    if status != request.expected_current || purpose != "new_unbiased_review" {
        bail!("result stage does not bind a current findings invocation");
    }
    tx.execute(
        "insert into review_result_drafts(project_id,draft_handle,invocation_id,reviewer_ref,status,version,version_handle,create_idempotency_key,create_payload_digest,created_at) values(?1,?2,?3,?4,'staging',0,?5,?6,?7,current_timestamp)",
        params![project, handle.as_str(), request.invocation_id, reviewer, version.as_str(), request.idempotency_key, digest],
    )?;
    tx.commit()?;
    Ok(ResultStageOutcome {
        stage_handle: handle.as_str().to_string(),
        version_handle: version.as_str().to_string(),
        state: "staging".to_string(),
        result_handle: None,
        already_applied: false,
    })
}

pub fn add_result_finding(
    root: &Path,
    request: AddResultFindingRequest<'_>,
) -> Result<ResultStageOutcome> {
    let targets = if request.requirement.is_some() || request.task.is_some() {
        vec![FindingTargetInput {
            design_requirement_id: request.requirement,
            task_id: request.task,
        }]
    } else {
        Vec::new()
    };
    add_result_finding_with_targets_in(
        root,
        AddResultFindingWithTargetsRequest {
            stage_handle: request.stage_handle,
            finding_type: request.finding_type,
            severity: request.severity,
            description: request.description,
            targets: &targets,
            expected_current: request.expected_current,
            idempotency_key: request.idempotency_key,
        },
        Some((request.requirement, request.task)),
    )
}

pub fn add_result_finding_with_targets(
    root: &Path,
    request: AddResultFindingWithTargetsRequest<'_>,
) -> Result<ResultStageOutcome> {
    add_result_finding_with_targets_in(root, request, None)
}

fn add_result_finding_with_targets_in(
    root: &Path,
    request: AddResultFindingWithTargetsRequest<'_>,
    legacy_target: Option<(Option<i64>, Option<i64>)>,
) -> Result<ResultStageOutcome> {
    if !matches!(request.severity, "critical" | "high" | "medium" | "low") {
        bail!("invalid staged finding severity");
    }
    validate_finding_targets(request.targets)?;
    require_text(request.description, "finding description")?;
    require_key(request.idempotency_key)?;
    let stage = ReviewResultStageHandle::parse(request.stage_handle)?;
    let (payload, digest_domain, item_domain): (CanonicalValue, &[u8], &[u8]) =
        if let Some((requirement, task)) = legacy_target {
            (
                CanonicalValue::object([
                    ("stage", CanonicalValue::string(stage.as_str())),
                    ("type", CanonicalValue::string(request.finding_type)),
                    ("severity", CanonicalValue::string(request.severity)),
                    ("description", CanonicalValue::string(request.description)),
                    (
                        "requirement",
                        requirement.map_or(CanonicalValue::Null, CanonicalValue::Integer),
                    ),
                    (
                        "task",
                        task.map_or(CanonicalValue::Null, CanonicalValue::Integer),
                    ),
                    ("expected", CanonicalValue::string(request.expected_current)),
                ]),
                b"agent-workbench:review-result-finding-add-v2\0",
                b"agent-workbench:review-result-item-v2\0",
            )
        } else {
            let target_values = request
                .targets
                .iter()
                .map(|target| {
                    CanonicalValue::object([
                        (
                            "requirement",
                            target
                                .design_requirement_id
                                .map_or(CanonicalValue::Null, CanonicalValue::Integer),
                        ),
                        (
                            "task",
                            target
                                .task_id
                                .map_or(CanonicalValue::Null, CanonicalValue::Integer),
                        ),
                    ])
                })
                .collect();
            (
                CanonicalValue::object([
                    ("stage", CanonicalValue::string(stage.as_str())),
                    ("type", CanonicalValue::string(request.finding_type)),
                    ("severity", CanonicalValue::string(request.severity)),
                    ("description", CanonicalValue::string(request.description)),
                    ("targets", CanonicalValue::Array(target_values)),
                    ("expected", CanonicalValue::string(request.expected_current)),
                ]),
                b"agent-workbench:review-result-finding-add-v3\0",
                b"agent-workbench:review-result-item-v3\0",
            )
        };
    let digest = domain_digest(digest_domain, &payload);
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    let (draft_id, invocation_id, status, version, stored_version): (i64, i64, String, i64, String) = tx
        .query_row(
            "select id,invocation_id,status,version,version_handle from review_result_drafts where project_id=?1 and draft_handle=?2",
            params![project, stage.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .context("review result stage not found")?;
    if let Some((stored_digest, result)) = lookup_draft_event(
        &tx,
        project,
        draft_id,
        "finding_add",
        request.idempotency_key,
    )? {
        if stored_digest != digest {
            bail!("review result idempotency key was used with a different finding");
        }
        let item_version: i64 = tx.query_row(
            "select item_version from review_result_draft_items where project_id=?1 and item_handle=?2",
            params![project, result],
            |row| row.get(0),
        )?;
        return Ok(ResultStageOutcome {
            stage_handle: stage.as_str().to_string(),
            version_handle: result_version_handle(stage.as_str(), item_version)
                .as_str()
                .to_string(),
            state: status,
            result_handle: Some(result),
            already_applied: true,
        });
    }
    if status != "staging" || stored_version != request.expected_current {
        bail!("review result stage changed or is terminal");
    }
    validate_staged_finding_type(&tx, project, invocation_id, request.finding_type)?;
    let next = version + 1;
    let item = ReviewResultItemHandle::derive(item_domain, &payload);
    let next_version = result_version_handle(stage.as_str(), next);
    let first_target = request.targets.first().copied();
    tx.execute(
        "insert into review_result_draft_items(project_id,draft_id,item_handle,item_version,finding_type,severity,description,design_requirement_id,task_id,created_at) values(?1,?2,?3,?4,?5,?6,?7,?8,?9,current_timestamp)",
        params![project, draft_id, item.as_str(), next, request.finding_type, request.severity, request.description, first_target.and_then(|target| target.design_requirement_id), first_target.and_then(|target| target.task_id)],
    )?;
    let draft_item_id = tx.last_insert_rowid();
    for (index, target) in request.targets.iter().enumerate() {
        tx.execute(
            "insert into review_result_draft_item_targets(project_id,draft_item_id,ordinal,design_requirement_id,task_id,created_at) values(?1,?2,?3,?4,?5,current_timestamp)",
            params![
                project,
                draft_item_id,
                i64::try_from(index + 1)?,
                target.design_requirement_id,
                target.task_id
            ],
        )?;
    }
    tx.execute(
        "insert into review_result_draft_item_target_seals(draft_item_id,project_id,target_count,created_at) values(?1,?2,?3,current_timestamp)",
        params![
            draft_item_id,
            project,
            i64::try_from(request.targets.len())?
        ],
    )?;
    let changed = tx.execute(
        "update review_result_drafts set version=?1,version_handle=?2 where id=?3 and status='staging' and version=?4",
        params![next, next_version.as_str(), draft_id, version],
    )?;
    if changed != 1 {
        bail!("concurrent review result update lost");
    }
    insert_draft_event(
        &tx,
        project,
        draft_id,
        "finding_add",
        request.idempotency_key,
        &digest,
        item.as_str(),
    )?;
    tx.commit()?;
    Ok(ResultStageOutcome {
        stage_handle: stage.as_str().to_string(),
        version_handle: next_version.as_str().to_string(),
        state: "staging".to_string(),
        result_handle: Some(item.as_str().to_string()),
        already_applied: false,
    })
}

fn validate_finding_targets(targets: &[FindingTargetInput]) -> Result<()> {
    for (index, target) in targets.iter().enumerate() {
        if target.design_requirement_id.is_none() && target.task_id.is_none() {
            bail!("finding target {} is empty", index + 1);
        }
        if targets[..index].contains(target) {
            bail!("finding targets must be unique");
        }
    }
    Ok(())
}

pub fn complete_result_stage(
    root: &Path,
    request: CompleteResultStageRequest<'_>,
) -> Result<ResultStageOutcome> {
    if request.expected_findings <= 0
        || !matches!(request.invocation_current, "requested" | "running")
    {
        bail!("result completion requires findings and a current invocation");
    }
    require_text(request.summary, "review summary")?;
    require_key(request.idempotency_key)?;
    let stage = ReviewResultStageHandle::parse(request.stage_handle)?;
    let payload = CanonicalValue::object([
        ("stage", CanonicalValue::string(stage.as_str())),
        ("count", CanonicalValue::Integer(request.expected_findings)),
        ("summary", CanonicalValue::string(request.summary)),
        ("expected", CanonicalValue::string(request.expected_current)),
        (
            "invocation",
            CanonicalValue::string(request.invocation_current),
        ),
    ]);
    let digest = domain_digest(b"agent-workbench:review-result-complete-v2\0", &payload);
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    let (draft_id, invocation_id, status, stored_version, existing_run): (i64, i64, String, String, Option<i64>) = tx
        .query_row(
            "select id,invocation_id,status,version_handle,review_run_id from review_result_drafts where project_id=?1 and draft_handle=?2",
            params![project, stage.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .context("review result stage not found")?;
    if let Some((stored_digest, result)) =
        lookup_draft_event(&tx, project, draft_id, "complete", request.idempotency_key)?
    {
        if stored_digest != digest {
            bail!("review result idempotency key was used with a different completion");
        }
        return Ok(ResultStageOutcome {
            stage_handle: stage.as_str().to_string(),
            version_handle: stored_version,
            state: status,
            result_handle: Some(result),
            already_applied: true,
        });
    }
    if status != "staging" || stored_version != request.expected_current {
        bail!("review result stage changed or is terminal");
    }
    let (invocation_status, purpose, run_type): (String, String, String) = tx.query_row(
        "select status,purpose,run_type from review_agent_invocations where project_id=?1 and id=?2",
        params![project, invocation_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    if invocation_status != request.invocation_current || purpose != "new_unbiased_review" {
        bail!("review result invocation changed or has the wrong purpose");
    }
    let count: i64 = tx.query_row(
        "select count(*) from review_result_draft_items where draft_id=?1",
        params![draft_id],
        |row| row.get(0),
    )?;
    if count != request.expected_findings {
        bail!("staged finding inventory count changed");
    }
    let run_id = existing_run.unwrap_or(insert_review_run_for_invocation(
        &tx,
        RunPublication {
            project,
            invocation: invocation_id,
            run_type: &run_type,
            purpose: &purpose,
            summary: request.summary,
            clean: 0,
            findings: count,
            finding_result: None,
        },
    )?);
    let draft_items = tx
        .prepare(
            "select id,project_id,finding_type,severity,description,design_requirement_id,task_id from review_result_draft_items where draft_id=?1 order by item_version",
        )?
        .query_map(params![draft_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, Option<i64>>(6)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (draft_item, item_project, finding_type, severity, description, requirement, task) in
        draft_items
    {
        tx.execute(
            "insert into findings(project_id,review_run_id,finding_type,severity,description,design_requirement_id,task_id,created_at) values(?1,?2,?3,?4,?5,?6,?7,current_timestamp)",
            params![
                item_project,
                run_id,
                finding_type,
                severity,
                description,
                requirement,
                task
            ],
        )?;
        let finding_id = tx.last_insert_rowid();
        tx.execute(
            "insert into finding_targets(project_id,finding_id,ordinal,design_requirement_id,task_id,created_at) select project_id,?1,ordinal,design_requirement_id,task_id,current_timestamp from review_result_draft_item_targets where draft_item_id=?2 order by ordinal",
            params![finding_id, draft_item],
        )?;
        let target_count: i64 = tx.query_row(
            "select target_count from review_result_draft_item_target_seals where draft_item_id=?1",
            params![draft_item],
            |row| row.get(0),
        )?;
        tx.execute(
            "insert into finding_target_seals(finding_id,project_id,target_count,created_at) values(?1,?2,?3,current_timestamp)",
            params![finding_id, item_project, target_count],
        )?;
    }
    let changed = tx.execute(
        "update review_agent_invocations set status='completed',claim='findings',result_summary=?1,review_run_id=?2,finished_at=current_timestamp where id=?3 and status=?4",
        params![request.summary, run_id, invocation_id, request.invocation_current],
    )?;
    if changed != 1 {
        bail!("concurrent review invocation completion lost");
    }
    tx.execute(
        "update review_result_drafts set status='completed',review_run_id=?1,completed_at=current_timestamp where id=?2 and status='staging'",
        params![run_id, draft_id],
    )?;
    let result = format!("review_run:{run_id}");
    insert_draft_event(
        &tx,
        project,
        draft_id,
        "complete",
        request.idempotency_key,
        &digest,
        &result,
    )?;
    tx.commit()?;
    Ok(ResultStageOutcome {
        stage_handle: stage.as_str().to_string(),
        version_handle: stored_version,
        state: "completed".to_string(),
        result_handle: Some(result),
        already_applied: false,
    })
}

pub fn cancel_result_stage(
    root: &Path,
    request: CancelResultStageRequest<'_>,
) -> Result<ResultStageOutcome> {
    require_text(request.reason, "result cancellation reason")?;
    require_key(request.idempotency_key)?;
    let stage = ReviewResultStageHandle::parse(request.stage_handle)?;
    let payload = CanonicalValue::object([
        ("stage", CanonicalValue::string(stage.as_str())),
        ("reason", CanonicalValue::string(request.reason)),
        ("expected", CanonicalValue::string(request.expected_current)),
    ]);
    let digest = domain_digest(b"agent-workbench:review-result-cancel-v2\0", &payload);
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    let (draft_id, status, stored_version): (i64, String, String) = tx
        .query_row(
            "select id,status,version_handle from review_result_drafts where project_id=?1 and draft_handle=?2",
            params![project, stage.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .context("review result stage not found")?;
    if let Some((stored_digest, result)) =
        lookup_draft_event(&tx, project, draft_id, "cancel", request.idempotency_key)?
    {
        if stored_digest != digest {
            bail!("review result idempotency key was used with a different cancellation");
        }
        return Ok(ResultStageOutcome {
            stage_handle: stage.as_str().to_string(),
            version_handle: stored_version,
            state: status,
            result_handle: Some(result),
            already_applied: true,
        });
    }
    if status != "staging" || stored_version != request.expected_current {
        bail!("review result stage changed or is terminal");
    }
    tx.execute(
        "update review_result_drafts set status='cancelled',terminal_reason=?1,completed_at=current_timestamp where id=?2 and status='staging'",
        params![request.reason, draft_id],
    )?;
    insert_draft_event(
        &tx,
        project,
        draft_id,
        "cancel",
        request.idempotency_key,
        &digest,
        stage.as_str(),
    )?;
    tx.commit()?;
    Ok(ResultStageOutcome {
        stage_handle: stage.as_str().to_string(),
        version_handle: stored_version,
        state: "cancelled".to_string(),
        result_handle: Some(stage.as_str().to_string()),
        already_applied: false,
    })
}

fn load_plan(tx: &Transaction<'_>, project: i64, plan: i64) -> Result<PlanContext> {
    tx.query_row(
        "select p.id,p.status,p.design_version_id,p.work_unit_id,p.stage,p.review_type,p.review_scope_id,pol.allow_fresh_review,pol.allow_resume_review,pol.max_fresh_agents,pol.max_resume_agents,pol.max_parallel_agents,pol.run_count_scope,p.fresh_review_after_run_id from review_plans p join review_policies pol on pol.id=p.review_policy_id where p.project_id=?1 and p.id=?2",
        params![project, plan],
        |row| {
            Ok(PlanContext {
                id: row.get(0)?,
                status: row.get(1)?,
                design_version_id: row.get(2)?,
                work_unit_id: row.get(3)?,
                stage: row.get(4)?,
                review_type: row.get(5)?,
                review_scope_id: row.get(6)?,
                allow_fresh: row.get::<_, i64>(7)? == 1,
                allow_resume: row.get::<_, i64>(8)? == 1,
                max_fresh: row.get(9)?,
                max_resume: row.get(10)?,
                max_parallel: row.get(11)?,
                run_count_scope: row.get(12)?,
                fresh_review_after_run_id: row.get(13)?,
            })
        },
    )
    .context("review plan not found")
}

fn validate_plan_target(
    tx: &Transaction<'_>,
    project: i64,
    plan: &PlanContext,
    target: &str,
    purpose: &str,
) -> Result<()> {
    match purpose {
        "new_unbiased_review" => {
            if plan.status != "open" {
                bail!("review plan is not current and open");
            }
            if !plan.allow_fresh {
                bail!("review policy disallows fresh review");
            }
            if let (Some(design), Some(kind)) = (
                plan.design_version_id,
                review_context_kind(&plan.stage, &plan.review_type),
            ) {
                let expected = crate::review_context::current_review_context_ref(
                    tx,
                    project,
                    kind,
                    Some(design),
                    Some(plan.work_unit_id),
                    None,
                )?;
                if target != expected {
                    bail!("review target is not the exact current plan context");
                }
            }
        }
        "finding_fix_verification" => {
            if !matches!(plan.status.as_str(), "open" | "blocked") {
                bail!("review plan cannot accept finding verification in its current state");
            }
            if !plan.allow_resume {
                bail!("review policy disallows finding verification");
            }
            let valid: bool = tx.query_row(
                "select exists(select 1 from closure_attempts a join closures c on c.id=a.closure_id join findings f on f.id=c.finding_id join review_runs r on r.id=f.review_run_id join review_plans source on source.id=r.review_plan_id where a.project_id=?1 and source.work_unit_id=?2 and a.result is null and f.lifecycle_state='awaiting_verification' and ?3='review-context:finding-fix:finding='||f.id||':closure='||c.id||':attempt='||a.id)",
                params![project, plan.work_unit_id, target],
                |row| row.get(0),
            )?;
            if !valid {
                bail!("verification target is not an exact current closure attempt");
            }
        }
        _ => bail!("unsupported review purpose"),
    }
    Ok(())
}

fn enforce_invocation_limit(
    tx: &Transaction<'_>,
    _project: i64,
    plan: &PlanContext,
    purpose: &str,
    target: &str,
) -> Result<()> {
    let (run_type, mut limit) = if purpose == "new_unbiased_review" {
        ("fresh", plan.max_fresh)
    } else {
        ("resume", plan.max_resume.max(1))
    };
    let used: i64 = if run_type == "resume" {
        tx.query_row(
            "select count(*) from review_agent_invocations where review_plan_id=?1 and run_type='resume' and target_context=?2",
            params![plan.id, target],
            |row| row.get(0),
        )?
    } else if plan.fresh_review_after_run_id > 0 {
        limit = limit.max(1);
        tx.query_row(
            "select count(*) from review_runs where review_plan_id=?1 and run_type='fresh' and id>?2",
            params![plan.id, plan.fresh_review_after_run_id],
            |row| row.get(0),
        )?
    } else {
        count_invocations_in_scope(tx, plan, run_type, false)?
    };
    if used >= limit {
        bail!("review invocation limit reached; complete the resolver-selected lifecycle action");
    }
    let running = count_invocations_in_scope(tx, plan, "", true)?;
    if running >= plan.max_parallel {
        bail!("maximum parallel review invocations reached");
    }
    Ok(())
}

fn count_invocations_in_scope(
    tx: &Transaction<'_>,
    plan: &PlanContext,
    run_type: &str,
    active_only: bool,
) -> Result<i64> {
    let (scope_sql, scope_id) = match plan.run_count_scope.as_str() {
        "review_scope" => plan
            .review_scope_id
            .map_or(("i.review_plan_id=?1", plan.id), |id| {
                ("p.review_scope_id=?1", id)
            }),
        "work_unit" => ("p.work_unit_id=?1", plan.work_unit_id),
        _ => ("i.review_plan_id=?1", plan.id),
    };
    let run_filter = if run_type.is_empty() {
        ""
    } else if run_type == "fresh" {
        "and i.run_type='fresh'"
    } else {
        "and i.run_type='resume'"
    };
    let state_filter = if active_only {
        "and i.status in ('requested','running')"
    } else {
        ""
    };
    let sql = format!(
        "select count(*) from review_agent_invocations i join review_plans p on p.id=i.review_plan_id where {scope_sql} {run_filter} {state_filter}"
    );
    tx.query_row(&sql, params![scope_id], |row| row.get(0))
        .map_err(Into::into)
}

fn insert_review_run_for_invocation(
    tx: &Transaction<'_>,
    publication: RunPublication<'_>,
) -> Result<i64> {
    let (scope, work, plan, target, provenance_kind, provenance_handle): (
        Option<i64>,
        i64,
        i64,
        String,
        String,
        String,
    ) = tx.query_row(
        "select p.review_scope_id,p.work_unit_id,p.id,i.target_context,pr.provenance_kind,pr.provenance_handle from review_agent_invocations i join review_plans p on p.id=i.review_plan_id join review_provenance_claims pr on pr.project_id=i.project_id and pr.provenance_handle=i.provenance_handle where i.project_id=?1 and i.id=?2",
        params![publication.project, publication.invocation],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
    )?;
    tx.execute(
        "insert into review_runs(project_id,review_scope_id,review_plan_id,run_type,run_purpose,target_type,work_unit_id,target_ref,result_summary,new_findings_count,carried_findings_checked,clean_run,review_provenance,review_provenance_ref,finding_fix_result,status,created_at) values(?1,?2,?3,?4,?5,'work_unit',?6,?7,?8,?9,?10,?11,?12,?13,?14,'completed',current_timestamp)",
        params![publication.project, scope, plan, publication.run_type, publication.purpose, work, target, publication.summary, publication.findings, if publication.run_type == "resume" { 1 } else { 0 }, publication.clean, provenance_kind, provenance_handle, publication.finding_result],
    )?;
    Ok(tx.last_insert_rowid())
}

fn validate_staged_finding_type(
    tx: &Transaction<'_>,
    project: i64,
    invocation: i64,
    finding_type: &str,
) -> Result<()> {
    let review_type: String = tx.query_row(
        "select p.review_type from review_agent_invocations i join review_plans p on p.id=i.review_plan_id where i.project_id=?1 and i.id=?2",
        params![project, invocation],
        |row| row.get(0),
    )?;
    let valid = match review_type.as_str() {
        "design_review" => finding_type == "design_finding",
        "design_task_decomposition" => finding_type == "design_task_gap",
        "design_implementation_diff" => finding_type == "design_implementation_drift",
        "implementation_review" => {
            matches!(finding_type, "implementation_finding" | "coverage_finding")
        }
        _ => false,
    };
    if !valid {
        bail!("finding type does not match the review plan");
    }
    Ok(())
}

fn lookup_draft_event(
    tx: &Transaction<'_>,
    project: i64,
    draft: i64,
    command: &str,
    key: &str,
) -> Result<Option<(String, String)>> {
    tx.query_row(
        "select payload_digest,result_handle from review_result_draft_events where project_id=?1 and draft_id=?2 and command=?3 and idempotency_key=?4",
        params![project, draft, command, key],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()
    .map_err(Into::into)
}

fn insert_draft_event(
    tx: &Transaction<'_>,
    project: i64,
    draft: i64,
    command: &str,
    key: &str,
    digest: &str,
    result: &str,
) -> Result<()> {
    tx.execute(
        "insert into review_result_draft_events(project_id,draft_id,command,idempotency_key,payload_digest,result_handle,created_at) values(?1,?2,?3,?4,?5,?6,current_timestamp)",
        params![project, draft, command, key, digest, result],
    )?;
    Ok(())
}

fn result_version_handle(stage: &str, version: i64) -> ReviewResultVersionHandle {
    ReviewResultVersionHandle::derive(
        b"agent-workbench:review-result-version-v2\0",
        &CanonicalValue::object([
            ("stage", CanonicalValue::string(stage)),
            ("version", CanonicalValue::Integer(version)),
        ]),
    )
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

fn validate_purpose(purpose: &str) -> Result<()> {
    if !matches!(purpose, "new_unbiased_review" | "finding_fix_verification") {
        bail!("review purpose must be new_unbiased_review or finding_fix_verification");
    }
    Ok(())
}

fn require_invocation_transition(current: &str, next: &str) -> Result<()> {
    let valid = matches!(
        (current, next),
        ("requested", "running")
            | ("requested", "completed")
            | ("requested", "failed")
            | ("requested", "cancelled")
            | ("running", "completed")
            | ("running", "failed")
            | ("running", "cancelled")
    );
    if !valid {
        bail!("review invocation transition is not allowed");
    }
    Ok(())
}

fn require_text(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{label} is required");
    }
    Ok(())
}

fn require_key(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 200
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
    {
        bail!("idempotency key must be a non-empty portable token");
    }
    Ok(())
}
