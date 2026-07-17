use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{open_existing_project, project_id};
use crate::identity::{
    AssertionHandle, CanonicalValue, CapabilityHandle, DecisionContinuationHandle, DecisionHandle,
    GrantHandle, PrincipalHandle, domain_digest,
};

use super::capability_validation::*;
use super::decision_projection_support::*;
use super::grants::set_contains;
use super::ingress::stored_assertion_payload;
use super::signed_envelope::{
    CborValue, closed_set, digest_reference, encode_value, hex_digest, parse_hex,
    parse_rfc3339_seconds, target_value,
};

#[derive(Clone, Debug)]
pub struct CapabilityIssueRequest<'a> {
    pub principal_handle: &'a str,
    pub owner_grant: &'a str,
    pub assertion_handle: &'a str,
    pub owner_ref: &'a str,
    pub target_ref: &'a str,
    pub role: &'a str,
    pub decision_family: &'a str,
    pub action: &'a str,
    pub design_context: &'a str,
    pub expires_at: &'a str,
}
#[derive(Clone, Debug)]
pub struct CapabilityOutcome {
    pub capability_handle: String,
}

#[derive(Clone, Debug)]
pub struct DecisionRequest<'a> {
    pub command_kind: &'a str,
    pub principal_handle: &'a str,
    pub capability_handle: &'a str,
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

#[derive(Clone, Debug)]
pub struct DecisionContinuationApplyRequest<'a> {
    pub continuation_handle: &'a str,
    pub decision_value: &'a str,
    pub reason: &'a str,
    pub principal_handle: &'a str,
    pub capability_handle: &'a str,
}

#[derive(Clone, Debug)]
pub struct DecisionContinuationOutcome {
    pub continuation_handle: String,
    pub command_kind: String,
    pub owner_ref: String,
    pub target_ref: String,
    pub decision_family: String,
    pub action: String,
    pub expected_current: String,
    pub design_context: String,
    pub status: String,
}

struct StoredCapability {
    id: i64,
    issuer: i64,
    holder: i64,
    owner: String,
    target: String,
    role: String,
    family: String,
    action: String,
    design_context: String,
    status: String,
}

pub(super) struct GrantLineageBinding<'a> {
    pub(super) owner: &'a str,
    pub(super) target: &'a str,
    pub(super) role: &'a str,
    pub(super) family: &'a str,
    pub(super) action: &'a str,
    pub(super) now: i64,
}

pub fn issue_capability(
    root: &Path,
    request: CapabilityIssueRequest<'_>,
) -> Result<CapabilityOutcome> {
    validate_tuple(request.role, request.decision_family, request.action)?;
    if request.design_context.len() != 64
        || !request
            .design_context
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        bail!("design context must be 64 lowercase hex");
    }
    let principal = PrincipalHandle::parse(request.principal_handle)?;
    let grant = GrantHandle::parse(request.owner_grant)?;
    let assertion = AssertionHandle::parse(request.assertion_handle)?;
    let handle = CapabilityHandle::derive(
        b"agent-workbench:decision-capability-v1\0",
        &CanonicalValue::object([
            ("principal", CanonicalValue::string(principal.as_str())),
            ("grant", CanonicalValue::string(grant.as_str())),
            ("assertion", CanonicalValue::string(assertion.as_str())),
            ("owner", CanonicalValue::string(request.owner_ref)),
            ("target", CanonicalValue::string(request.target_ref)),
            ("role", CanonicalValue::string(request.role)),
            ("family", CanonicalValue::string(request.decision_family)),
            ("action", CanonicalValue::string(request.action)),
            ("context", CanonicalValue::string(request.design_context)),
            ("expires", CanonicalValue::string(request.expires_at)),
        ]),
    );
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    let (principal_id, subject_kind, subject_digest): (i64,String,String) = tx
        .query_row(
            "select id,subject_kind,subject_digest from authority_principals where project_id=?1 and principal_handle=?2",
            params![project, principal.as_str()],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?)),
        )
        .context("principal is not resolved")?;
    if request.role == "human_authority" && subject_kind != "human" {
        bail!("human authority capability requires a human principal");
    }
    let (grant_id,grantee,owner,target,roles,families,actions,status,grant_expires):(i64,i64,String,String,String,String,String,String,String)=tx.query_row(
        "select id,grantee_principal_id,owner_ref,maximum_target,roles,decision_families,actions,status,expires_at from owner_decision_grants where project_id=?1 and grant_handle=?2",
        params![project,grant.as_str()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?)),
    ).context("owner grant not found")?;
    if status != "active"
        || grantee != principal_id
        || owner != request.owner_ref
        || (target != "owner_all" && target != request.target_ref)
        || !set_contains(&roles, request.role)
        || !set_contains(&families, request.decision_family)
        || !set_contains(&actions, request.action)
    {
        bail!("capability request exceeds its active owner grant");
    }
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let capability_expires = parse_rfc3339_seconds(request.expires_at, "capability expiry")?;
    let grant_expires = parse_rfc3339_seconds(&grant_expires, "grant expiry")?;
    if capability_expires <= now
        || capability_expires > now.saturating_add(86_400)
        || capability_expires > grant_expires
    {
        bail!("capability expiry exceeds its current grant or 24-hour bound");
    }
    validate_grant_lineage(
        &tx,
        project,
        grant_id,
        GrantLineageBinding {
            owner: request.owner_ref,
            target: request.target_ref,
            role: request.role,
            family: request.decision_family,
            action: request.action,
            now,
        },
    )?;
    if current_design_context(&tx, project, request.target_ref)? != request.design_context {
        bail!("capability design context is not current for its target");
    }
    validate_special_capability_target(&tx, project, &request)?;
    let (assertion_id, payload) = stored_assertion_payload(
        &tx,
        project,
        assertion.as_str(),
        "capability_issue",
        &subject_digest,
    )?;
    if !capability_payload_contains(&payload, &request)? {
        bail!("capability assertion payload mismatch");
    }
    tx.execute(r#"insert into decision_capabilities(project_id,capability_handle,owner_grant_id,issuer_principal_id,holder_principal_id,owner_ref,target_ref,role,decision_family,action,design_context,assertion_id,expires_at,status,created_at)
        values(?1,?2,?3,?4,?4,?5,?6,?7,?8,?9,?10,?11,?12,'active',current_timestamp)"#,
        params![project,handle.as_str(),grant_id,principal_id,request.owner_ref,request.target_ref,request.role,request.decision_family,request.action,request.design_context,assertion_id,request.expires_at])?;
    let capability_id = tx.last_insert_rowid();
    let lineage_digest = grant_lineage_digest(&tx, project, grant_id)?;
    let binding_digest = hex_digest(&encode_value(&capability_payload(&request)?)?);
    tx.execute("insert into capability_issue_audits(project_id,capability_id,assertion_id,owner_grant_id,principal_id,lineage_digest,binding_digest,created_at) values(?1,?2,?3,?4,?5,?6,?7,current_timestamp)",params![project,capability_id,assertion_id,grant_id,principal_id,lineage_digest,binding_digest])?;
    tx.execute("update authority_assertions set consumed_at=current_timestamp where id=?1 and consumed_at is null",params![assertion_id])?;
    tx.commit()?;
    Ok(CapabilityOutcome {
        capability_handle: handle.as_str().to_string(),
    })
}

fn capability_payload(request: &CapabilityIssueRequest<'_>) -> Result<CborValue> {
    let role = closed_set(
        request.role,
        &[
            ("grant_admin", 0),
            ("review_adjudicator", 1),
            ("finding_adjudicator", 2),
            ("verification_adjudicator", 3),
            ("human_authority", 4),
        ],
        "role",
    )?;
    let family = closed_set(
        request.decision_family,
        &[("review", 0), ("finding", 1), ("verification", 2)],
        "family",
    )?;
    let action = closed_set(
        request.action,
        &[
            ("adjudicate", 0),
            ("dispose", 1),
            ("bootstrap_adjudicate", 2),
            ("correct_terminal", 3),
            ("reopen", 4),
        ],
        "action",
    )?;
    let expiry = parse_rfc3339_seconds(request.expires_at, "capability expiry")?;
    Ok(CborValue::Map(std::collections::BTreeMap::from([
        (
            0,
            CborValue::Bytes(
                digest_reference(b"agent-workbench:owner-ref-v1\0", request.owner_ref).to_vec(),
            ),
        ),
        (1, target_value(request.target_ref)?),
        (2, role),
        (3, family),
        (4, action),
        (
            5,
            if expiry >= 0 {
                CborValue::U64(expiry as u64)
            } else {
                CborValue::I64(expiry)
            },
        ),
        (
            6,
            CborValue::Bytes(parse_hex::<32>(request.design_context, "design context")?.to_vec()),
        ),
    ])))
}

fn capability_payload_contains(
    payload: &CborValue,
    request: &CapabilityIssueRequest<'_>,
) -> Result<bool> {
    let CborValue::Map(map) = payload else {
        return Ok(false);
    };
    let requested = capability_payload(request)?;
    let CborValue::Map(req) = requested else {
        unreachable!()
    };
    let target_ok = map.get(&1) == Some(&CborValue::Array(vec![CborValue::U64(0)]))
        || map.get(&1) == req.get(&1);
    let sets_ok = (2..=4).all(|key| match (map.get(&key), req.get(&key)) {
        (Some(CborValue::Array(max)), Some(CborValue::Array(values))) => {
            values.iter().all(|v| max.contains(v))
        }
        _ => false,
    });
    let expiry = |v: Option<&CborValue>| match v {
        Some(CborValue::U64(n)) => i64::try_from(*n).ok(),
        Some(CborValue::I64(n)) => Some(*n),
        _ => None,
    };
    Ok(map.len() == 7
        && map.get(&0) == req.get(&0)
        && target_ok
        && sets_ok
        && expiry(map.get(&5))
            .zip(expiry(req.get(&5)))
            .is_some_and(|(max, value)| max >= value)
        && map.get(&6) == req.get(&6))
}

pub fn present_decision(root: &Path, request: DecisionRequest<'_>) -> Result<DecisionOutcome> {
    present_decision_with_continuation(root, request, None)
}

fn present_decision_with_continuation(
    root: &Path,
    request: DecisionRequest<'_>,
    continuation_handle: Option<&str>,
) -> Result<DecisionOutcome> {
    validate_presentation_syntax(&request)?;
    let capability = CapabilityHandle::parse(request.capability_handle)?;
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    let stored:Option<StoredCapability>=tx.query_row(
        "select id,issuer_principal_id,holder_principal_id,owner_ref,target_ref,role,decision_family,action,design_context,status from decision_capabilities where project_id=?1 and capability_handle=?2",
        params![project,capability.as_str()],|row|Ok(StoredCapability{id:row.get(0)?,issuer:row.get(1)?,holder:row.get(2)?,owner:row.get(3)?,target:row.get(4)?,role:row.get(5)?,family:row.get(6)?,action:row.get(7)?,design_context:row.get(8)?,status:row.get(9)?}),
    ).optional()?;
    let Some(stored) = stored else {
        let digest = security_presentation_digest(&request);
        tx.execute("insert into authority_security_audits(project_id,boundary,presented_handle,presentation_digest,reason,created_at) values(?1,'capability_presentation',?2,?3,'capability_unknown_or_unissued',current_timestamp)",params![project,request.capability_handle,digest])?;
        tx.commit()?;
        bail!("capability_unknown_or_unissued");
    };
    let StoredCapability {
        id: capability_id,
        issuer,
        holder,
        owner,
        target,
        role,
        family,
        action,
        design_context,
        status,
    } = stored;
    if status != "consumed" {
        let changed=tx.execute("update decision_capabilities set status='consumed',consumed_at=current_timestamp where id=?1 and status in ('active','revoked','expired') and consumed_at is null",params![capability_id])?;
        if changed != 1 {
            bail!("capability consumption lost a concurrent race");
        }
    }
    let prior_consumption: Option<(String, String, Option<String>, Option<String>)> = if status
        == "consumed"
    {
        Some(
                tx.query_row(
                    "select presentation_digest,outcome,rejection_reason,decision_handle from capability_consumption_audits where project_id=?1 and capability_id=?2",
                    params![project, capability_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .context("consumed capability has no consumption audit")?,
            )
    } else {
        None
    };
    let attempted = current_design_context(&tx, project, request.target_ref).and_then(|context| {
        presentation_digest(project, &context, &request).map(|digest| (context, digest))
    });
    let (attempted_design_context, presentation_digest) = match attempted {
        Ok(values) => values,
        Err(_) => {
            let digest = security_presentation_digest(&request);
            if let Some((prior_digest, outcome, rejection, decision)) = prior_consumption.as_ref() {
                if prior_digest.as_str() != digest {
                    tx.execute("insert into authority_security_audits(project_id,boundary,presented_handle,presentation_digest,reason,created_at) values(?1,'capability_replay',?2,?3,'capability_consumed_payload_mismatch',current_timestamp)",params![project,request.capability_handle,digest])?;
                    tx.commit()?;
                    bail!("capability_consumed_payload_mismatch");
                }
                if outcome == "accepted" {
                    mark_continuation_applied(&tx, project, continuation_handle)?;
                    tx.commit()?;
                    return Ok(DecisionOutcome {
                        decision_handle: decision
                            .clone()
                            .context("accepted consumption has no decision identity")?,
                    });
                }
                tx.commit()?;
                bail!(
                    "{}",
                    rejection
                        .clone()
                        .unwrap_or_else(|| "capability_consumption_incomplete".into())
                );
            }
            tx.execute("insert into capability_consumption_audits(project_id,capability_id,attempted_principal,attempted_owner,attempted_target,attempted_design_context,attempted_family,attempted_action,presentation_digest,outcome,rejection_reason,attempted_at,completed_at) values(?1,?2,?3,?4,?5,?6,?7,?8,?9,'rejected','invalid_presentation_context',current_timestamp,current_timestamp)",params![project,capability_id,request.principal_handle,request.owner_ref,request.target_ref,design_context,request.decision_family,request.action,digest])?;
            let continuation = DecisionContinuationHandle::derive(
                b"agent-workbench:decision-continuation-v1\0",
                &CanonicalValue::object([
                    ("presentation", CanonicalValue::string(&digest)),
                    (
                        "code",
                        CanonicalValue::string("invalid_presentation_context"),
                    ),
                ]),
            );
            tx.execute("insert or ignore into decision_continuations(project_id,continuation_handle,command_kind,owner_ref,target_ref,decision_family,action,expected_current,design_context,rejection_code,status,created_at) values(?1,?2,?3,?4,?5,?6,?7,?8,?9,'invalid_presentation_context','pending',current_timestamp)",params![project,continuation.as_str(),request.command_kind,request.owner_ref,request.target_ref,request.decision_family,request.action,request.expected_current,design_context])?;
            tx.commit()?;
            bail!(
                "code: invalid_presentation_context\ncontinuation: {}\nrequired_input: decision,reason,principal,capability\nnext: agent-workbench decision continuation show {}",
                continuation.as_str(),
                continuation.as_str()
            );
        }
    };
    if let Some((digest, outcome, rejection, decision)) = prior_consumption {
        if digest != presentation_digest {
            tx.execute("insert into authority_security_audits(project_id,boundary,presented_handle,presentation_digest,reason,created_at) values(?1,'capability_replay',?2,?3,'capability_consumed_payload_mismatch',current_timestamp)",params![project,request.capability_handle,presentation_digest])?;
            tx.commit()?;
            bail!("capability_consumed_payload_mismatch");
        }
        if outcome == "accepted" {
            mark_continuation_applied(&tx, project, continuation_handle)?;
            tx.commit()?;
            return Ok(DecisionOutcome {
                decision_handle: decision
                    .context("accepted consumption has no decision identity")?,
            });
        }
        tx.commit()?;
        bail!(
            "{}",
            rejection.unwrap_or_else(|| "capability_consumption_incomplete".into())
        );
    }
    tx.execute("insert into capability_consumption_audits(project_id,capability_id,attempted_principal,attempted_owner,attempted_target,attempted_design_context,attempted_family,attempted_action,presentation_digest,outcome,attempted_at) values(?1,?2,?3,?4,?5,?6,?7,?8,?9,'pending',current_timestamp)",params![project,capability_id,request.principal_handle,request.owner_ref,request.target_ref,attempted_design_context,request.decision_family,request.action,presentation_digest])?;
    tx.execute_batch("savepoint capability_effect")?;
    let result = (|| -> Result<DecisionOutcome> {
        validate_tuple_for_family(request.decision_family, request.action)?;
        let principal = PrincipalHandle::parse(request.principal_handle)?;
        let principal_id: i64 = tx
            .query_row(
                "select id from authority_principals where project_id=?1 and principal_handle=?2",
                params![project, principal.as_str()],
                |row| row.get(0),
            )
            .context("principal is not resolved")?;
        if issuer != principal_id || holder != principal_id {
            bail!("capability_principal_mismatch");
        }
        if owner != request.owner_ref
            || target != request.target_ref
            || family != request.decision_family
            || action != request.action
        {
            bail!("capability_tuple_mismatch");
        }
        validate_tuple(&role, &family, &action)?;
        let (grant_id, expires): (i64, String) = tx.query_row(
            "select owner_grant_id,expires_at from decision_capabilities where id=?1",
            params![capability_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        if parse_rfc3339_seconds(&expires, "capability expiry")? <= now {
            bail!("capability_expired");
        }
        validate_grant_lineage(
            &tx,
            project,
            grant_id,
            GrantLineageBinding {
                owner: &owner,
                target: &target,
                role: &role,
                family: &family,
                action: &action,
                now,
            },
        )?;
        if status != "active" {
            bail!("capability_not_active");
        }
        if attempted_design_context != design_context {
            bail!("capability_design_context_stale");
        }
        validate_principal_independence(&tx, project, principal_id, &family, &target)?;
        let current = expected_current_for_target(&tx, project, &request)?;
        if (request.expected_current == "pending" && current.is_some())
            || (request.expected_current != "pending"
                && current.as_deref() != Some(request.expected_current))
        {
            bail!("expected_current_stale");
        }
        let payload = decision_payload(&request, principal.as_str());
        let payload_digest =
            domain_digest(b"agent-workbench:owner-decision-payload-v1\0", &payload);
        let decision = DecisionHandle::derive(b"agent-workbench:owner-decision-v1\0", &payload);
        tx.execute(r#"insert into owner_decisions(project_id,decision_handle,capability_id,principal_id,owner_ref,target_ref,decision_family,action,decision_value,reason,expected_current,payload_digest,created_at)
            values(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,current_timestamp)"#,params![project,decision.as_str(),capability_id,principal_id,request.owner_ref,request.target_ref,request.decision_family,request.action,request.decision_value,request.reason,request.expected_current,payload_digest])?;
        let owner_decision_id = tx.last_insert_rowid();
        apply_decision_projection(&tx, project, owner_decision_id, &request)?;
        Ok(DecisionOutcome {
            decision_handle: decision.as_str().to_string(),
        })
    })();
    match result {
        Ok(outcome) => {
            tx.execute_batch("release capability_effect")?;
            tx.execute("update capability_consumption_audits set outcome='accepted',decision_handle=?1,completed_at=current_timestamp where capability_id=?2 and outcome='pending'",params![outcome.decision_handle,capability_id])?;
            mark_continuation_applied(&tx, project, continuation_handle)?;
            tx.commit()?;
            Ok(outcome)
        }
        Err(error) => {
            tx.execute_batch("rollback to capability_effect; release capability_effect")?;
            let reason = typed_rejection_reason(&error);
            tx.execute("update capability_consumption_audits set outcome='rejected',rejection_reason=?1,completed_at=current_timestamp where capability_id=?2 and outcome='pending'",params![reason,capability_id])?;
            let continuation = DecisionContinuationHandle::derive(
                b"agent-workbench:decision-continuation-v1\0",
                &CanonicalValue::object([
                    ("presentation", CanonicalValue::string(&presentation_digest)),
                    ("code", CanonicalValue::string(&reason)),
                ]),
            );
            tx.execute(
                "insert or ignore into decision_continuations(project_id,continuation_handle,command_kind,owner_ref,target_ref,decision_family,action,expected_current,design_context,rejection_code,status,created_at) values(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'pending',current_timestamp)",
                params![project,continuation.as_str(),request.command_kind,request.owner_ref,request.target_ref,request.decision_family,request.action,request.expected_current,attempted_design_context,reason],
            )?;
            tx.commit()?;
            bail!(
                "code: {reason}\ncontinuation: {}\nrequired_input: decision,reason,principal,capability\nnext: agent-workbench decision continuation show {}",
                continuation.as_str(),
                continuation.as_str()
            )
        }
    }
}

fn validate_presentation_syntax(request: &DecisionRequest<'_>) -> Result<()> {
    validate_tuple_for_family(request.decision_family, request.action)?;
    CapabilityHandle::parse(request.capability_handle)?;
    PrincipalHandle::parse(request.principal_handle)?;
    target_value(request.target_ref)?;
    if request.expected_current != "pending" {
        handle_digest(request.expected_current)?;
    }
    Ok(())
}

pub fn show_decision_continuation(
    root: &Path,
    handle: &str,
) -> Result<DecisionContinuationOutcome> {
    let handle = DecisionContinuationHandle::parse(handle)?;
    let conn = open_existing_project(root)?;
    let project = project_id(&conn)?;
    conn.query_row(
        "select continuation_handle,command_kind,owner_ref,target_ref,decision_family,action,expected_current,design_context,status from decision_continuations where project_id=?1 and continuation_handle=?2",
        params![project, handle.as_str()],
        |row| {
            Ok(DecisionContinuationOutcome {
                continuation_handle: row.get(0)?,
                command_kind: row.get(1)?,
                owner_ref: row.get(2)?,
                target_ref: row.get(3)?,
                decision_family: row.get(4)?,
                action: row.get(5)?,
                expected_current: row.get(6)?,
                design_context: row.get(7)?,
                status: row.get(8)?,
            })
        },
    )
    .context("decision continuation not found")
}

pub fn apply_decision_continuation(
    root: &Path,
    request: DecisionContinuationApplyRequest<'_>,
) -> Result<DecisionOutcome> {
    let continuation = show_decision_continuation(root, request.continuation_handle)?;
    if continuation.status != "pending" {
        bail!("decision_continuation_not_pending");
    }
    present_decision_with_continuation(
        root,
        DecisionRequest {
            command_kind: &continuation.command_kind,
            principal_handle: request.principal_handle,
            capability_handle: request.capability_handle,
            owner_ref: &continuation.owner_ref,
            target_ref: &continuation.target_ref,
            decision_family: &continuation.decision_family,
            action: &continuation.action,
            decision_value: request.decision_value,
            reason: request.reason,
            expected_current: &continuation.expected_current,
        },
        Some(request.continuation_handle),
    )
}

fn mark_continuation_applied(
    conn: &rusqlite::Connection,
    project: i64,
    continuation: Option<&str>,
) -> Result<()> {
    let Some(continuation) = continuation else {
        return Ok(());
    };
    let changed=conn.execute("update decision_continuations set status='applied',applied_at=current_timestamp where project_id=?1 and continuation_handle=?2 and status='pending'",params![project,continuation])?;
    if changed != 1 {
        bail!("decision_continuation_not_pending");
    }
    Ok(())
}

fn decision_payload(request: &DecisionRequest<'_>, principal: &str) -> CanonicalValue {
    CanonicalValue::object([
        ("principal", CanonicalValue::string(principal)),
        (
            "capability",
            CanonicalValue::string(request.capability_handle),
        ),
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
    ])
}

fn presentation_digest(
    project: i64,
    design_context: &str,
    request: &DecisionRequest<'_>,
) -> Result<String> {
    let family = match request.decision_family {
        "review" => 0,
        "finding" => 1,
        "verification" => 2,
        _ => u64::MAX,
    };
    let action = match request.action {
        "adjudicate" => 0,
        "dispose" => 1,
        "bootstrap_adjudicate" => 2,
        "correct_terminal" => 3,
        "reopen" => 4,
        _ => u64::MAX,
    };
    let expected = if request.expected_current == "pending" {
        CborValue::Array(vec![CborValue::U64(0)])
    } else {
        let digest = handle_digest(request.expected_current)?;
        CborValue::Array(vec![CborValue::U64(1), CborValue::Bytes(digest.to_vec())])
    };
    let outcome = CborValue::Array(vec![
        CborValue::U64(1),
        CborValue::Text(request.decision_value.to_string()),
    ]);
    let value = CborValue::Map(BTreeMap::from([
        (0, CborValue::U64(1)),
        (1, CborValue::Text(request.command_kind.into())),
        (
            2,
            CborValue::Bytes(
                parse_hex::<32>(
                    request.capability_handle.trim_start_matches("capability_"),
                    "capability",
                )?
                .to_vec(),
            ),
        ),
        (3, CborValue::Text(request.principal_handle.to_string())),
        (4, CborValue::U64(project as u64)),
        (5, CborValue::Text(request.owner_ref.to_string())),
        (6, target_value(request.target_ref)?),
        (
            7,
            CborValue::Bytes(parse_hex::<32>(design_context, "design context")?.to_vec()),
        ),
        (8, CborValue::U64(family)),
        (9, CborValue::U64(action)),
        (10, outcome),
        (
            11,
            CborValue::Bytes(
                parse_hex::<32>(&hex_digest(request.reason.as_bytes()), "reason digest")?.to_vec(),
            ),
        ),
        (12, expected),
    ]));
    Ok(hex_digest(&encode_value(&value)?))
}

fn security_presentation_digest(request: &DecisionRequest<'_>) -> String {
    domain_digest(
        b"agent-workbench:security-presentation-v1\0",
        &CanonicalValue::object([
            (
                "capability",
                CanonicalValue::string(request.capability_handle),
            ),
            (
                "principal",
                CanonicalValue::string(request.principal_handle),
            ),
            ("owner", CanonicalValue::string(request.owner_ref)),
            ("target", CanonicalValue::string(request.target_ref)),
        ]),
    )
}
fn typed_rejection_reason(error: &anyhow::Error) -> String {
    error
        .to_string()
        .split(':')
        .next()
        .unwrap_or("capability_rejected")
        .replace(' ', "_")
}

fn apply_decision_projection(
    conn: &rusqlite::Connection,
    project: i64,
    owner_decision_id: i64,
    request: &DecisionRequest<'_>,
) -> Result<()> {
    match request.decision_family {
        "review" => {
            if request.action == "correct_terminal" {
                return apply_review_correction(conn, project, owner_decision_id, request);
            }
            if request.action == "bootstrap_adjudicate" {
                return apply_bootstrap_adjudication(conn, project, owner_decision_id, request);
            }
            if !matches!(
                request.decision_value,
                "accepted" | "rejected" | "needs_evidence"
            ) {
                bail!("review adjudication decision is not allowed");
            }
            let run = parse_target_id(request.target_ref, "review_run:")?;
            let audit_only: i64 = conn.query_row(
                "select exists(select 1 from legacy_claim_audits where project_id=?1 and review_run_id=?2 and reviewer_resolution in ('unbound','ambiguous'))",
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
                let fresh:i64=conn.query_row("select count(*) from review_runs r where r.review_plan_id=?1 and r.run_type='fresh' and r.status='completed' and r.clean_run=1 and exists(select 1 from review_adjudication_decisions d where d.review_run_id=r.id and d.value='accepted' and not exists(select 1 from review_adjudication_decisions n where n.predecessor_id=d.id))",params![plan_id],|row|row.get(0))?;
                let resume:i64=conn.query_row("select count(*) from review_runs r where r.review_plan_id=?1 and r.run_type='resume' and r.status='completed' and r.clean_run=1 and exists(select 1 from review_adjudication_decisions d where d.review_run_id=r.id and d.value='accepted' and not exists(select 1 from review_adjudication_decisions n where n.predecessor_id=d.id))",params![plan_id],|row|row.get(0))?;
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
                conn.execute("update decision_capabilities set status='revoked' where project_id=?1 and status='active' and (target_ref=?2 or target_ref in(select 'closure_attempt:'||a.id from closure_attempts a join closures c on c.id=a.closure_id where a.project_id=?1 and c.finding_id=?3))",params![project,format!("finding:{finding}"),finding])?;
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
            let (closure_id,finding_id,current_state,claim):(i64,i64,String,String)=conn.query_row(
                "select a.closure_id,c.finding_id,f.lifecycle_state,v.result from closure_attempts a join closures c on c.id=a.closure_id join findings f on f.id=c.finding_id join finding_verifications v on v.closure_attempt_id=a.id where a.project_id=?1 and a.id=?2 order by v.id desc limit 1",
                params![project, attempt],
                |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)),
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
            }
        }
        _ => bail!("unsupported decision family"),
    }
    Ok(())
}
