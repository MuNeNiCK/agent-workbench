use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

use crate::authority::{OwnerDecisionRequest, current_owner_decision, record_owner_decision_in};
use crate::db::{open_existing_project, project_id};
use crate::identity::{CanonicalValue, DecisionContinuationHandle, domain_digest};

const REQUIRED_INPUTS: &str = "decision,reason";

#[derive(Clone, Debug)]
pub struct NewDecisionContinuation<'a> {
    pub command_kind: &'a str,
    pub owner_ref: &'a str,
    pub target_ref: &'a str,
    pub decision_family: &'a str,
    pub action: &'a str,
    pub expected_current: &'a str,
    pub rejection_code: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionContinuationRecord {
    pub continuation_handle: String,
    pub command_kind: String,
    pub owner_ref: String,
    pub target_ref: String,
    pub decision_family: String,
    pub action: String,
    pub expected_current: String,
    pub context_identity: String,
    pub rejection_code: String,
    pub required_inputs: String,
    pub status: String,
    pub decision_handle: Option<String>,
    pub successor_continuation: Option<String>,
}

#[derive(Clone, Debug)]
pub struct DecisionContinuationApply<'a> {
    pub continuation_handle: &'a str,
    pub decision: &'a str,
    pub reason: &'a str,
    pub expected_current: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionContinuationApplyOutcome {
    pub continuation_handle: String,
    pub status: String,
    pub decision_handle: Option<String>,
    pub successor_continuation: Option<String>,
    pub next_action: String,
    pub idempotent: bool,
}

struct StoredContinuation {
    id: i64,
    continuation_handle: String,
    command_kind: String,
    owner_ref: String,
    target_ref: String,
    decision_family: String,
    action: String,
    expected_current: String,
    status: String,
    applied_payload_digest: Option<String>,
    decision_handle: Option<String>,
    successor_continuation: Option<String>,
}

pub fn add_decision_continuation(
    root: &Path,
    request: NewDecisionContinuation<'_>,
) -> Result<DecisionContinuationRecord> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let project = project_id(&tx)?;
    let (_, handle) = insert_continuation(&tx, project, &request)?;
    let record = load_record(&tx, project, &handle)?;
    tx.commit()?;
    Ok(record)
}

pub fn show_decision_continuation(
    root: &Path,
    continuation_handle: &str,
) -> Result<DecisionContinuationRecord> {
    DecisionContinuationHandle::parse(continuation_handle)?;
    let conn = open_existing_project(root)?;
    let project = project_id(&conn)?;
    load_record(&conn, project, continuation_handle)
}

pub fn apply_decision_continuation(
    root: &Path,
    request: DecisionContinuationApply<'_>,
) -> Result<DecisionContinuationApplyOutcome> {
    DecisionContinuationHandle::parse(request.continuation_handle)?;
    if !matches!(request.decision, "accepted" | "rejected" | "needs_evidence") {
        bail!("continuation_decision_invalid: unsupported owner decision");
    }
    if request.reason.trim().is_empty() {
        bail!("continuation_reason_required: decision reason must not be empty");
    }
    if request.expected_current.trim().is_empty() {
        bail!("continuation_expected_current_required: expected current must not be empty");
    }

    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let project = project_id(&tx)?;
    let stored = load_stored(&tx, project, request.continuation_handle)?;
    if request.expected_current != stored.expected_current {
        bail!(
            "continuation_expected_current_mismatch: supplied expected current does not match the bound continuation"
        );
    }
    let applied_payload_digest = application_digest(&stored, &request);

    if stored.status == "applied" {
        if stored.applied_payload_digest.as_deref() != Some(&applied_payload_digest) {
            bail!(
                "continuation_already_applied: changed input cannot replay an applied continuation"
            );
        }
        let decision_handle = stored
            .decision_handle
            .context("applied continuation has no owner decision")?;
        tx.commit()?;
        return Ok(DecisionContinuationApplyOutcome {
            continuation_handle: stored.continuation_handle,
            status: "applied".to_string(),
            decision_handle: Some(decision_handle),
            successor_continuation: None,
            next_action: "agent-workbench next".to_string(),
            idempotent: true,
        });
    }
    if stored.status == "superseded" {
        let successor = stored
            .successor_continuation
            .context("superseded continuation has no successor")?;
        tx.commit()?;
        return Ok(DecisionContinuationApplyOutcome {
            continuation_handle: stored.continuation_handle,
            status: "superseded".to_string(),
            decision_handle: None,
            successor_continuation: Some(successor.clone()),
            next_action: format!("agent-workbench decision continuation show {successor}"),
            idempotent: true,
        });
    }

    let owner_request = OwnerDecisionRequest {
        command_kind: &stored.command_kind,
        owner_ref: &stored.owner_ref,
        target_ref: &stored.target_ref,
        decision_family: &stored.decision_family,
        action: &stored.action,
        decision_value: request.decision,
        reason: request.reason,
        expected_current: &stored.expected_current,
    };
    let observed_current = current_owner_decision(&tx, project, &owner_request)?
        .unwrap_or_else(|| "pending".to_string());
    if observed_current != stored.expected_current {
        let replacement = NewDecisionContinuation {
            command_kind: &stored.command_kind,
            owner_ref: &stored.owner_ref,
            target_ref: &stored.target_ref,
            decision_family: &stored.decision_family,
            action: &stored.action,
            expected_current: &observed_current,
            rejection_code: "owner_revision_changed",
        };
        let (successor_id, successor_handle) = insert_continuation(&tx, project, &replacement)?;
        let changed = tx.execute(
            "update decision_continuations set status='superseded',superseded_at=current_timestamp,successor_id=?1 where project_id=?2 and id=?3 and status='pending'",
            params![successor_id, project, stored.id],
        )?;
        if changed != 1 {
            bail!("continuation_state_changed: continuation changed while being superseded");
        }
        tx.commit()?;
        return Ok(DecisionContinuationApplyOutcome {
            continuation_handle: stored.continuation_handle,
            status: "superseded".to_string(),
            decision_handle: None,
            successor_continuation: Some(successor_handle.clone()),
            next_action: format!("agent-workbench decision continuation show {successor_handle}"),
            idempotent: false,
        });
    }

    let decision = record_owner_decision_in(&tx, project, owner_request)?;
    let changed = tx.execute(
        "update decision_continuations set status='applied',owner_decision_id=?1,applied_payload_digest=?2,applied_at=current_timestamp where project_id=?3 and id=?4 and status='pending'",
        params![decision.owner_decision_id, applied_payload_digest, project, stored.id],
    )?;
    if changed != 1 {
        bail!("continuation_state_changed: continuation changed while being applied");
    }
    tx.commit()?;
    Ok(DecisionContinuationApplyOutcome {
        continuation_handle: stored.continuation_handle,
        status: "applied".to_string(),
        decision_handle: Some(decision.decision_handle),
        successor_continuation: None,
        next_action: "agent-workbench next".to_string(),
        idempotent: false,
    })
}

fn insert_continuation(
    conn: &rusqlite::Connection,
    project: i64,
    request: &NewDecisionContinuation<'_>,
) -> Result<(i64, String)> {
    validate_continuation_request(request)?;
    let context = context_value(request);
    let context_identity = domain_digest(
        b"agent-workbench:decision-continuation-context-v1\0",
        &context,
    );
    let handle = DecisionContinuationHandle::derive(
        b"agent-workbench:decision-continuation-v1\0",
        &CanonicalValue::object([
            ("context", context),
            (
                "rejection_code",
                CanonicalValue::string(request.rejection_code),
            ),
            ("required_inputs", CanonicalValue::string(REQUIRED_INPUTS)),
        ]),
    );
    conn.execute(
        r#"
        insert or ignore into decision_continuations(
            project_id,continuation_handle,command_kind,owner_ref,target_ref,
            decision_family,action,expected_current,context_identity,rejection_code,
            required_inputs,status,created_at
        ) values(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'pending',current_timestamp)
        "#,
        params![
            project,
            handle.as_str(),
            request.command_kind,
            request.owner_ref,
            request.target_ref,
            request.decision_family,
            request.action,
            request.expected_current,
            context_identity,
            request.rejection_code,
            REQUIRED_INPUTS,
        ],
    )?;
    let id = conn.query_row(
        "select id from decision_continuations where project_id=?1 and continuation_handle=?2",
        params![project, handle.as_str()],
        |row| row.get(0),
    )?;
    Ok((id, handle.as_str().to_string()))
}

fn validate_continuation_request(request: &NewDecisionContinuation<'_>) -> Result<()> {
    if !matches!(
        request.decision_family,
        "review" | "finding" | "verification"
    ) {
        bail!("continuation_owner_invalid: unsupported decision family");
    }
    let valid_action = matches!(
        (request.decision_family, request.action),
        ("review", "adjudicate" | "correct_terminal")
            | ("finding", "adjudicate" | "dispose" | "reopen")
            | ("verification", "adjudicate")
    );
    if !valid_action {
        bail!("continuation_action_invalid: unsupported owner action");
    }
    if [
        request.command_kind,
        request.owner_ref,
        request.target_ref,
        request.expected_current,
        request.rejection_code,
    ]
    .into_iter()
    .any(|value| value.trim().is_empty())
    {
        bail!("continuation_context_incomplete: continuation fields must not be empty");
    }
    Ok(())
}

fn context_value(request: &NewDecisionContinuation<'_>) -> CanonicalValue {
    CanonicalValue::object([
        ("command_kind", CanonicalValue::string(request.command_kind)),
        ("owner", CanonicalValue::string(request.owner_ref)),
        ("target", CanonicalValue::string(request.target_ref)),
        ("family", CanonicalValue::string(request.decision_family)),
        ("action", CanonicalValue::string(request.action)),
        (
            "expected_current",
            CanonicalValue::string(request.expected_current),
        ),
    ])
}

fn application_digest(
    stored: &StoredContinuation,
    request: &DecisionContinuationApply<'_>,
) -> String {
    domain_digest(
        b"agent-workbench:decision-continuation-application-v1\0",
        &CanonicalValue::object([
            (
                "continuation",
                CanonicalValue::string(&stored.continuation_handle),
            ),
            ("decision", CanonicalValue::string(request.decision)),
            ("reason", CanonicalValue::string(request.reason)),
            (
                "expected_current",
                CanonicalValue::string(request.expected_current),
            ),
        ]),
    )
}

fn load_stored(
    conn: &rusqlite::Connection,
    project: i64,
    handle: &str,
) -> Result<StoredContinuation> {
    conn.query_row(
        r#"
        select continuation.id,continuation.continuation_handle,continuation.command_kind,
               continuation.owner_ref,continuation.target_ref,continuation.decision_family,
               continuation.action,continuation.expected_current,continuation.status,
               continuation.applied_payload_digest,decision.decision_handle,
               successor.continuation_handle
        from decision_continuations continuation
        left join owner_decisions decision on decision.id=continuation.owner_decision_id
        left join decision_continuations successor on successor.id=continuation.successor_id
        where continuation.project_id=?1 and continuation.continuation_handle=?2
        "#,
        params![project, handle],
        |row| {
            Ok(StoredContinuation {
                id: row.get(0)?,
                continuation_handle: row.get(1)?,
                command_kind: row.get(2)?,
                owner_ref: row.get(3)?,
                target_ref: row.get(4)?,
                decision_family: row.get(5)?,
                action: row.get(6)?,
                expected_current: row.get(7)?,
                status: row.get(8)?,
                applied_payload_digest: row.get(9)?,
                decision_handle: row.get(10)?,
                successor_continuation: row.get(11)?,
            })
        },
    )
    .optional()?
    .context("decision continuation not found")
}

fn load_record(
    conn: &rusqlite::Connection,
    project: i64,
    handle: &str,
) -> Result<DecisionContinuationRecord> {
    conn.query_row(
        r#"
        select continuation.continuation_handle,continuation.command_kind,
               continuation.owner_ref,continuation.target_ref,continuation.decision_family,
               continuation.action,continuation.expected_current,continuation.context_identity,
               continuation.rejection_code,continuation.required_inputs,continuation.status,
               decision.decision_handle,successor.continuation_handle
        from decision_continuations continuation
        left join owner_decisions decision on decision.id=continuation.owner_decision_id
        left join decision_continuations successor on successor.id=continuation.successor_id
        where continuation.project_id=?1 and continuation.continuation_handle=?2
        "#,
        params![project, handle],
        |row| {
            Ok(DecisionContinuationRecord {
                continuation_handle: row.get(0)?,
                command_kind: row.get(1)?,
                owner_ref: row.get(2)?,
                target_ref: row.get(3)?,
                decision_family: row.get(4)?,
                action: row.get(5)?,
                expected_current: row.get(6)?,
                context_identity: row.get(7)?,
                rejection_code: row.get(8)?,
                required_inputs: row.get(9)?,
                status: row.get(10)?,
                decision_handle: row.get(11)?,
                successor_continuation: row.get(12)?,
            })
        },
    )
    .optional()?
    .context("decision continuation not found")
}
