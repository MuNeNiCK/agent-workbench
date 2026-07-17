use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::params;

use super::ingress::stored_assertion_payload;
use super::signed_envelope::{
    CborValue, closed_set, digest_reference, parse_rfc3339_seconds, target_value,
};
use crate::db::{open_existing_project, project_id};
use crate::identity::{AssertionHandle, CanonicalValue, GrantHandle, PrincipalHandle};

#[derive(Clone, Debug)]
pub struct RootGrantRequest<'a> {
    pub principal_handle: &'a str,
    pub assertion_handle: &'a str,
    pub owner_ref: &'a str,
    pub maximum_target: &'a str,
    pub roles: &'a str,
    pub decision_families: &'a str,
    pub actions: &'a str,
    pub maximum_depth: i64,
    pub expires_at: &'a str,
}

#[derive(Clone, Debug)]
pub struct GrantOutcome {
    pub grant_handle: String,
}

#[derive(Clone, Debug)]
pub struct DelegateGrantRequest<'a> {
    pub parent_grant: &'a str,
    pub grantor_principal: &'a str,
    pub grantee_principal: &'a str,
    pub assertion_handle: &'a str,
    pub target_scope: &'a str,
    pub roles: &'a str,
    pub decision_families: &'a str,
    pub actions: &'a str,
    pub delegation_depth: i64,
    pub expires_at: &'a str,
}

pub fn issue_root_grant(root: &Path, request: RootGrantRequest<'_>) -> Result<GrantOutcome> {
    validate_set(
        request.roles,
        &[
            "grant_admin",
            "review_adjudicator",
            "finding_adjudicator",
            "verification_adjudicator",
            "human_authority",
        ],
    )?;
    validate_set(
        request.decision_families,
        &["review", "finding", "verification"],
    )?;
    validate_set(
        request.actions,
        &[
            "adjudicate",
            "dispose",
            "bootstrap_adjudicate",
            "correct_terminal",
            "reopen",
        ],
    )?;
    if request.maximum_depth < 0 {
        bail!("maximum depth must be non-negative");
    }
    let principal = PrincipalHandle::parse(request.principal_handle)?;
    let assertion = AssertionHandle::parse(request.assertion_handle)?;
    let handle = GrantHandle::derive(
        b"agent-workbench:owner-decision-grant-v1\0",
        &CanonicalValue::object([
            ("principal", CanonicalValue::string(principal.as_str())),
            ("assertion", CanonicalValue::string(assertion.as_str())),
            ("owner", CanonicalValue::string(request.owner_ref)),
            ("target", CanonicalValue::string(request.maximum_target)),
            (
                "roles",
                CanonicalValue::string(normalized_set(request.roles)),
            ),
            (
                "families",
                CanonicalValue::string(normalized_set(request.decision_families)),
            ),
            (
                "actions",
                CanonicalValue::string(normalized_set(request.actions)),
            ),
            ("depth", CanonicalValue::Integer(request.maximum_depth)),
            ("expires", CanonicalValue::string(request.expires_at)),
        ]),
    );
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    let (principal_id, subject_digest): (i64,String) = tx.query_row(
        "select id,subject_digest from authority_principals where project_id=?1 and principal_handle=?2 and subject_kind='human'",
        params![project,principal.as_str()], |row| Ok((row.get(0)?, row.get(1)?)),
    ).context("root grant requires a canonical human principal")?;
    let (assertion_id, payload) = stored_assertion_payload(
        &tx,
        project,
        assertion.as_str(),
        "root_grant",
        &subject_digest,
    )?;
    if !root_payload_contains(&payload, &request)? {
        bail!("root grant assertion payload mismatch");
    }
    tx.execute(
        r#"insert into owner_decision_grants(project_id,grant_handle,owner_ref,grantor_principal_id,grantee_principal_id,maximum_target,roles,decision_families,actions,maximum_depth,expires_at,assertion_id,status,created_at)
           values(?1,?2,?3,?4,?4,?5,?6,?7,?8,?9,?10,?11,'active',current_timestamp)"#,
        params![project,handle.as_str(),request.owner_ref,principal_id,request.maximum_target,normalized_set(request.roles),normalized_set(request.decision_families),normalized_set(request.actions),request.maximum_depth,request.expires_at,assertion_id],
    )?;
    tx.execute("update authority_assertions set consumed_at=current_timestamp where id=?1 and consumed_at is null", params![assertion_id])?;
    tx.commit()?;
    Ok(GrantOutcome {
        grant_handle: handle.as_str().to_string(),
    })
}

pub fn delegate_grant(root: &Path, request: DelegateGrantRequest<'_>) -> Result<GrantOutcome> {
    let parent = GrantHandle::parse(request.parent_grant)?;
    let grantor = PrincipalHandle::parse(request.grantor_principal)?;
    let grantee = PrincipalHandle::parse(request.grantee_principal)?;
    let assertion = AssertionHandle::parse(request.assertion_handle)?;
    let roles = validate_set(
        request.roles,
        &[
            "grant_admin",
            "review_adjudicator",
            "finding_adjudicator",
            "verification_adjudicator",
            "human_authority",
        ],
    )?;
    let families = validate_set(
        request.decision_families,
        &["review", "finding", "verification"],
    )?;
    let actions = validate_set(
        request.actions,
        &[
            "adjudicate",
            "dispose",
            "bootstrap_adjudicate",
            "correct_terminal",
            "reopen",
        ],
    )?;
    if request.delegation_depth < 0 {
        bail!("delegation depth must be non-negative");
    }
    let child_expires = parse_rfc3339_seconds(request.expires_at, "grant expiry")?;
    if child_expires <= time::OffsetDateTime::now_utc().unix_timestamp() {
        bail!("delegated grant must have a future expiry");
    }
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    let (grantor_id,grantor_subject):(i64,String)=tx.query_row("select id,subject_digest from authority_principals where project_id=?1 and principal_handle=?2",params![project,grantor.as_str()],|row|Ok((row.get(0)?,row.get(1)?))).context("grantor principal is not resolved")?;
    let (grantee_id,grantee_kind,grantee_subject):(i64,String,String)=tx.query_row("select id,subject_kind,subject_digest from authority_principals where project_id=?1 and principal_handle=?2",params![project,grantee.as_str()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).context("grantee principal is not resolved")?;
    let (parent_id,owner,parent_grantee,parent_target,parent_roles,parent_families,parent_actions,parent_depth,parent_expiry,status):(i64,String,i64,String,String,String,String,i64,String,String)=tx.query_row(
        "select id,owner_ref,grantee_principal_id,maximum_target,roles,decision_families,actions,maximum_depth,expires_at,status from owner_decision_grants where project_id=?1 and grant_handle=?2",
        params![project,parent.as_str()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?,row.get(9)?))).context("parent grant not found")?;
    let parent_expiry = parse_rfc3339_seconds(&parent_expiry, "parent grant expiry")?;
    if status != "active"
        || parent_grantee != grantor_id
        || !set_contains(&parent_roles, "grant_admin")
        || parent_depth <= 0
        || request.delegation_depth >= parent_depth
        || child_expires > parent_expiry
    {
        bail!("parent grant cannot delegate this child");
    }
    if parent_target != "owner_all"
        && parent_target != request.target_scope
        && !grant_scope_contains(&tx, project, &parent_target, request.target_scope)?
    {
        bail!("delegated target is outside parent scope");
    }
    if !roles.iter().all(|value| set_contains(&parent_roles, value))
        || !families
            .iter()
            .all(|value| set_contains(&parent_families, value))
        || !actions
            .iter()
            .all(|value| set_contains(&parent_actions, value))
    {
        bail!("delegated grant widens parent sets");
    }
    let mut lineage=tx.prepare("with recursive l(id,parent_grant_id,owner_ref,maximum_target,roles,decision_families,actions,status,expires_at) as (select id,parent_grant_id,owner_ref,maximum_target,roles,decision_families,actions,status,expires_at from owner_decision_grants where project_id=?1 and id=?2 union all select g.id,g.parent_grant_id,g.owner_ref,g.maximum_target,g.roles,g.decision_families,g.actions,g.status,g.expires_at from owner_decision_grants g join l on g.id=l.parent_grant_id where g.project_id=?1) select owner_ref,maximum_target,roles,decision_families,actions,status,expires_at from l")?;
    let ancestors = lineage.query_map(params![project, parent_id], |row| {
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
    for ancestor in ancestors {
        let (a_owner, a_target, a_roles, a_families, a_actions, a_status, a_expiry) = ancestor?;
        if a_owner != owner
            || a_status != "active"
            || parse_rfc3339_seconds(&a_expiry, "ancestor expiry")?
                <= time::OffsetDateTime::now_utc().unix_timestamp()
            || !grant_scope_contains(&tx, project, &a_target, request.target_scope)?
            || !roles.iter().all(|v| set_contains(&a_roles, v))
            || !families.iter().all(|v| set_contains(&a_families, v))
            || !actions.iter().all(|v| set_contains(&a_actions, v))
        {
            bail!("grant ancestor lineage is not effective for delegation");
        }
    }
    drop(lineage);
    let (assertion_id, payload) = stored_assertion_payload(
        &tx,
        project,
        assertion.as_str(),
        "grant_delegate",
        &grantor_subject,
    )?;
    let kind = match grantee_kind.as_str() {
        "human" => 0,
        "agent" => 1,
        "service" => 2,
        _ => bail!("unsupported grantee kind"),
    };
    let expected = CborValue::Map(std::collections::BTreeMap::from([
        (
            0,
            CborValue::Bytes(digest_reference(b"agent-workbench:owner-ref-v1\0", &owner).to_vec()),
        ),
        (
            1,
            CborValue::Bytes(
                digest_reference(b"agent-workbench:grant-ref-v1\0", parent.as_str()).to_vec(),
            ),
        ),
        (
            2,
            super::signed_envelope::subject_value(
                kind,
                super::signed_envelope::parse_hex(&grantee_subject, "grantee subject")?,
            ),
        ),
        (3, target_value(request.target_scope)?),
        (
            4,
            closed_set(
                request.roles,
                &[
                    ("grant_admin", 0),
                    ("review_adjudicator", 1),
                    ("finding_adjudicator", 2),
                    ("verification_adjudicator", 3),
                    ("human_authority", 4),
                ],
                "role",
            )?,
        ),
        (
            5,
            closed_set(
                request.decision_families,
                &[("review", 0), ("finding", 1), ("verification", 2)],
                "family",
            )?,
        ),
        (
            6,
            closed_set(
                request.actions,
                &[
                    ("adjudicate", 0),
                    ("dispose", 1),
                    ("bootstrap_adjudicate", 2),
                    ("correct_terminal", 3),
                    ("reopen", 4),
                ],
                "action",
            )?,
        ),
        (7, CborValue::U64(request.delegation_depth as u64)),
        (8, timestamp_value(request.expires_at, "grant expiry")?),
    ]));
    if payload != expected {
        bail!("grant delegation assertion payload mismatch");
    }
    let handle = GrantHandle::derive(
        b"agent-workbench:owner-decision-grant-v1\0",
        &CanonicalValue::object([
            ("parent", CanonicalValue::string(parent.as_str())),
            ("grantor", CanonicalValue::string(grantor.as_str())),
            ("grantee", CanonicalValue::string(grantee.as_str())),
            ("assertion", CanonicalValue::string(assertion.as_str())),
        ]),
    );
    tx.execute(r#"insert into owner_decision_grants(project_id,grant_handle,parent_grant_id,owner_ref,grantor_principal_id,grantee_principal_id,maximum_target,roles,decision_families,actions,maximum_depth,expires_at,assertion_id,status,created_at)
      values(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,'active',current_timestamp)"#,params![project,handle.as_str(),parent_id,owner,grantor_id,grantee_id,request.target_scope,normalized_set(request.roles),normalized_set(request.decision_families),normalized_set(request.actions),request.delegation_depth,request.expires_at,assertion_id])?;
    tx.execute("update authority_assertions set consumed_at=current_timestamp where id=?1 and consumed_at is null",params![assertion_id])?;
    tx.commit()?;
    Ok(GrantOutcome {
        grant_handle: handle.as_str().to_string(),
    })
}

fn root_payload_contains(payload: &CborValue, request: &RootGrantRequest<'_>) -> Result<bool> {
    let CborValue::Map(map) = payload else {
        return Ok(false);
    };
    let owner = CborValue::Bytes(
        digest_reference(b"agent-workbench:owner-ref-v1\0", request.owner_ref).to_vec(),
    );
    let target = target_value(request.maximum_target)?;
    let roles = closed_set(
        request.roles,
        &[
            ("grant_admin", 0),
            ("review_adjudicator", 1),
            ("finding_adjudicator", 2),
            ("verification_adjudicator", 3),
            ("human_authority", 4),
        ],
        "role",
    )?;
    let families = closed_set(
        request.decision_families,
        &[("review", 0), ("finding", 1), ("verification", 2)],
        "family",
    )?;
    let actions = closed_set(
        request.actions,
        &[
            ("adjudicate", 0),
            ("dispose", 1),
            ("bootstrap_adjudicate", 2),
            ("correct_terminal", 3),
            ("reopen", 4),
        ],
        "action",
    )?;
    let target_ok = map.get(&1) == Some(&CborValue::Array(vec![CborValue::U64(0)]))
        || map.get(&1) == Some(&target);
    let depth_ok =
        matches!(map.get(&5),Some(CborValue::U64(value)) if *value>=request.maximum_depth as u64);
    let ceiling = timestamp_value(request.expires_at, "grant expiry")?;
    let expiry_ok = timestamp_at_least(map.get(&6), &ceiling);
    Ok(map.len() == 7
        && map.get(&0) == Some(&owner)
        && target_ok
        && cbor_set_contains(map.get(&2), &roles)
        && cbor_set_contains(map.get(&3), &families)
        && cbor_set_contains(map.get(&4), &actions)
        && depth_ok
        && expiry_ok)
}

fn cbor_set_contains(maximum: Option<&CborValue>, requested: &CborValue) -> bool {
    match (maximum, requested) {
        (Some(CborValue::Array(max)), CborValue::Array(req)) => req.iter().all(|v| max.contains(v)),
        _ => false,
    }
}
fn timestamp_at_least(maximum: Option<&CborValue>, requested: &CborValue) -> bool {
    let value = |v: &CborValue| match v {
        CborValue::U64(n) => i64::try_from(*n).ok(),
        CborValue::I64(n) => Some(*n),
        _ => None,
    };
    maximum
        .and_then(value)
        .zip(value(requested))
        .is_some_and(|(max, req)| max >= req)
}

fn grant_scope_contains(
    conn: &rusqlite::Connection,
    project: i64,
    scope: &str,
    target: &str,
) -> Result<bool> {
    if scope == "owner_all" || scope == target {
        return Ok(true);
    }
    if let Some(raw) = scope.strip_prefix("review_plan:") {
        let plan = raw.parse::<i64>()?;
        if let Some(run) = target.strip_prefix("review_run:") {
            return Ok(conn.query_row("select exists(select 1 from review_runs where project_id=?1 and review_plan_id=?2 and id=?3)",params![project,plan,run.parse::<i64>()?],|r|r.get(0))?);
        }
        if let Some(finding) = target.strip_prefix("finding:") {
            return Ok(conn.query_row("select exists(select 1 from findings f join review_runs r on r.id=f.review_run_id where f.project_id=?1 and r.review_plan_id=?2 and f.id=?3)",params![project,plan,finding.parse::<i64>()?],|r|r.get(0))?);
        }
        if let Some(attempt) = target.strip_prefix("closure_attempt:") {
            return Ok(conn.query_row("select exists(select 1 from closure_attempts a join closures c on c.id=a.closure_id join findings f on f.id=c.finding_id join review_runs r on r.id=f.review_run_id where a.project_id=?1 and r.review_plan_id=?2 and a.id=?3)",params![project,plan,attempt.parse::<i64>()?],|r|r.get(0))?);
        }
    }
    if let Some(raw) = scope.strip_prefix("review_run:") {
        let run = raw.parse::<i64>()?;
        if let Some(finding) = target.strip_prefix("finding:") {
            return Ok(conn.query_row("select exists(select 1 from findings where project_id=?1 and review_run_id=?2 and id=?3)",params![project,run,finding.parse::<i64>()?],|r|r.get(0))?);
        }
        if let Some(attempt) = target.strip_prefix("closure_attempt:") {
            return Ok(conn.query_row("select exists(select 1 from closure_attempts a join closures c on c.id=a.closure_id join findings f on f.id=c.finding_id where a.project_id=?1 and f.review_run_id=?2 and a.id=?3)",params![project,run,attempt.parse::<i64>()?],|r|r.get(0))?);
        }
    }
    if let (Some(raw), Some(attempt)) = (
        scope.strip_prefix("finding:"),
        target.strip_prefix("closure_attempt:"),
    ) {
        return Ok(conn.query_row("select exists(select 1 from closure_attempts a join closures c on c.id=a.closure_id where a.project_id=?1 and c.finding_id=?2 and a.id=?3)",params![project,raw.parse::<i64>()?,attempt.parse::<i64>()?],|r|r.get(0))?);
    }
    Ok(false)
}

fn timestamp_value(value: &str, label: &str) -> Result<CborValue> {
    let value = parse_rfc3339_seconds(value, label)?;
    Ok(if value >= 0 {
        CborValue::U64(value as u64)
    } else {
        CborValue::I64(value)
    })
}

pub fn revoke_grant(root: &Path, grant_handle: &str, assertion_handle: &str) -> Result<()> {
    let grant = GrantHandle::parse(grant_handle)?;
    let assertion = AssertionHandle::parse(assertion_handle)?;
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    let assertion_id: i64 = tx.query_row(
        "select id from authority_assertions where project_id=?1 and assertion_digest=?2 and purpose='grant_revoke' and consumed_at is null",
        params![project,assertion.as_str().trim_start_matches("assertion_")], |row| row.get(0),
    ).context("grant revoke assertion is unavailable")?;
    let changed = tx.execute(
        "update owner_decision_grants set status='revoked',revoked_at=current_timestamp where project_id=?1 and grant_handle=?2 and status='active'",
        params![project,grant.as_str()],
    )?;
    if changed != 1 {
        bail!("grant is not current active");
    }
    tx.execute("update decision_capabilities set status='revoked' where owner_grant_id in (select id from owner_decision_grants where project_id=?1 and grant_handle=?2) and status='active'",params![project,grant.as_str()])?;
    tx.execute(
        "update authority_assertions set consumed_at=current_timestamp where id=?1",
        params![assertion_id],
    )?;
    tx.commit()?;
    Ok(())
}

pub fn revoke_grant_as(
    root: &Path,
    grant_handle: &str,
    grantor_handle: &str,
    assertion_handle: &str,
    reason: &str,
    expected_current: &str,
) -> Result<()> {
    if expected_current != "active" || reason.is_empty() {
        bail!("grant revoke requires a reason and expected-current active");
    }
    let grant = GrantHandle::parse(grant_handle)?;
    let grantor = PrincipalHandle::parse(grantor_handle)?;
    let assertion = AssertionHandle::parse(assertion_handle)?;
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    let (grant_id,owner,status):(i64,String,String)=tx.query_row("select id,owner_ref,status from owner_decision_grants where project_id=?1 and grant_handle=?2",params![project,grant.as_str()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).context("grant not found")?;
    let (grantor_id,subject):(i64,String)=tx.query_row("select id,subject_digest from authority_principals where project_id=?1 and principal_handle=?2",params![project,grantor.as_str()],|row|Ok((row.get(0)?,row.get(1)?))).context("grantor principal is not resolved")?;
    let authorized:i64=tx.query_row("with recursive lineage(id,parent_grant_id,grantor_principal_id,grantee_principal_id,roles,status,expires_at,depth) as (
      select id,parent_grant_id,grantor_principal_id,grantee_principal_id,roles,status,expires_at,0 from owner_decision_grants where project_id=?1 and id=?2
      union all select g.id,g.parent_grant_id,g.grantor_principal_id,g.grantee_principal_id,g.roles,g.status,g.expires_at,l.depth+1 from owner_decision_grants g join lineage l on g.id=l.parent_grant_id where g.project_id=?1)
      select exists(select 1 from lineage where (depth=0 and grantor_principal_id=?3) or (depth>0 and grantee_principal_id=?3 and instr(','||roles||',',',grant_admin,')>0 and status='active'))",
      params![project,grant_id,grantor_id],|row|row.get(0))?;
    if status != "active" || authorized != 1 {
        bail!("grantor is not authorized to revoke the current active grant");
    }
    let (assertion_id, payload) =
        stored_assertion_payload(&tx, project, assertion.as_str(), "grant_revoke", &subject)?;
    let CborValue::Map(map) = payload else {
        bail!("grant revoke payload must be a map");
    };
    let expected_owner =
        CborValue::Bytes(digest_reference(b"agent-workbench:owner-ref-v1\0", &owner).to_vec());
    let expected_grant = CborValue::Bytes(
        digest_reference(b"agent-workbench:grant-ref-v1\0", grant.as_str()).to_vec(),
    );
    if map.len() != 4
        || map.get(&0) != Some(&expected_owner)
        || map.get(&1) != Some(&expected_grant)
        || map.get(&2)
            != Some(&CborValue::Bytes(
                super::signed_envelope::parse_hex::<32>(
                    &super::signed_envelope::hex_digest(reason.as_bytes()),
                    "reason digest",
                )?
                .to_vec(),
            ))
        || map.get(&3) != Some(&CborValue::U64(0))
    {
        bail!("grant revoke assertion payload mismatch");
    }
    tx.execute("update owner_decision_grants set status='revoked',revoked_at=current_timestamp where id=?1 and status='active'",params![grant_id])?;
    tx.execute("with recursive descendants(id) as (select ?1 union all select g.id from owner_decision_grants g join descendants d on g.parent_grant_id=d.id where g.project_id=?2) update decision_capabilities set status='revoked' where owner_grant_id in descendants and status='active'",params![grant_id,project])?;
    tx.execute("update authority_assertions set consumed_at=current_timestamp where id=?1 and consumed_at is null",params![assertion_id])?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn validate_set(value: &str, allowed: &[&str]) -> Result<BTreeSet<String>> {
    let values = value
        .split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    if values.is_empty() || values.iter().any(|item| !allowed.contains(&item.as_str())) {
        bail!("set contains an unsupported or empty value");
    }
    Ok(values)
}
pub(crate) fn normalized_set(value: &str) -> String {
    value
        .split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(",")
}
pub(crate) fn set_contains(stored: &str, value: &str) -> bool {
    stored.split(',').any(|item| item == value)
}
