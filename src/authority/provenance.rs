use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{open_authority_migration_project, open_existing_project, project_id};
use crate::identity::{
    AssertionHandle, CanonicalValue, LegacyReviewerBindingHandle, PrincipalHandle,
    ReviewProvenanceHandle, domain_digest,
};

use super::ingress::stored_assertion_payload;
use super::signed_envelope::{CborValue, digest_reference, parse_hex};

#[derive(Clone, Debug)]
pub struct ReviewProvenanceIssueRequest<'a> {
    pub principal_handle: &'a str,
    pub assertion_handle: &'a str,
    pub review_plan_id: i64,
    pub target_context: &'a str,
    pub provenance_kind: &'a str,
    pub review_purpose: &'a str,
    pub reference_digest: &'a str,
    pub idempotency_key: &'a str,
}

pub struct LegacyReviewerBindingRequest<'a> {
    pub assertion_handle: &'a str,
    pub idempotency_key: &'a str,
}
pub struct LegacyReviewerBindingOutcome {
    pub binding_handle: String,
}
pub fn bind_legacy_reviewer(
    root: &Path,
    request: LegacyReviewerBindingRequest<'_>,
) -> Result<LegacyReviewerBindingOutcome> {
    if request.idempotency_key.is_empty() {
        bail!("migration reviewer binding idempotency key is empty");
    }
    let assertion = AssertionHandle::parse(request.assertion_handle)?;
    let mut conn = open_authority_migration_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    let existing:Option<(String,String)>=tx.query_row(
        "select b.binding_handle,a.assertion_digest from legacy_reviewer_bindings b join authority_assertions a on a.id=b.assertion_id where b.project_id=?1 and b.idempotency_key=?2",
        params![project,request.idempotency_key],|row|Ok((row.get(0)?,row.get(1)?)),
    ).optional()?;
    if let Some((handle, assertion_digest)) = existing {
        if assertion_digest != assertion.as_str().trim_start_matches("assertion_") {
            bail!("migration reviewer binding idempotency payload mismatch");
        }
        return Ok(LegacyReviewerBindingOutcome {
            binding_handle: handle,
        });
    }
    let committed: i64 = tx.query_row(
        "select exists(select 1 from legacy_adjudication_migrations where project_id=?1)",
        params![project],
        |row| row.get(0),
    )?;
    if committed == 1 {
        bail!("legacy reviewer binding is closed after migration commit");
    }
    let (subject,kind):(String,String)=tx.query_row("select subject_digest,subject_kind from authority_assertions where project_id=?1 and assertion_digest=?2",params![project,assertion.as_str().trim_start_matches("assertion_")],|row|Ok((row.get(0)?,row.get(1)?))).context("legacy reviewer assertion is not imported")?;
    let principal = PrincipalHandle::derive(
        b"agent-workbench:principal-v1\0",
        &CanonicalValue::object([
            ("provider", CanonicalValue::string("signed-envelope-v1")),
            ("subject_kind", CanonicalValue::string(&kind)),
            ("subject_digest", CanonicalValue::string(&subject)),
        ]),
    );
    tx.execute("insert or ignore into authority_principals(project_id,principal_handle,provider,subject_kind,subject_digest,created_at) values(?1,?2,'signed-envelope-v1',?3,?4,current_timestamp)",params![project,principal.as_str(),kind,subject])?;
    let principal_id: i64 = tx.query_row(
        "select id from authority_principals where project_id=?1 and principal_handle=?2",
        params![project, principal.as_str()],
        |row| row.get(0),
    )?;
    let (assertion_id, payload) = stored_assertion_payload(
        &tx,
        project,
        assertion.as_str(),
        "legacy_reviewer_binding",
        &subject,
    )?;
    let CborValue::Map(map) = payload else {
        bail!("legacy reviewer binding payload must be a map")
    };
    let source = match map.get(&0) {
        Some(CborValue::Bytes(v)) if v.len() == 32 => super::signed_envelope::hex_digest(v),
        _ => bail!("legacy source ledger digest is invalid"),
    };
    let generation = match map.get(&1) {
        Some(CborValue::U64(v)) => i64::try_from(*v)?,
        _ => bail!("legacy source generation is invalid"),
    };
    let reviewer = match map.get(&2) {
        Some(CborValue::Bytes(v)) if v.len() == 32 => super::signed_envelope::hex_digest(v),
        _ => bail!("legacy source reviewer digest is invalid"),
    };
    let source_matches:i64=tx.query_row("select exists(select 1 from authority_migration_sources where project_id=?1 and source_ledger_digest=?2 and source_generation=?3)",params![project,source,generation],|row|row.get(0))?;
    if source_matches != 1 {
        bail!("legacy reviewer binding source snapshot mismatch");
    }
    let payload_digest = domain_digest(
        b"agent-workbench:legacy-reviewer-binding-payload-v1\0",
        &CanonicalValue::object([
            ("source", CanonicalValue::string(&source)),
            ("generation", CanonicalValue::Integer(generation)),
            ("reviewer", CanonicalValue::string(&reviewer)),
            ("principal", CanonicalValue::string(principal.as_str())),
        ]),
    );
    let handle = LegacyReviewerBindingHandle::derive(
        b"agent-workbench:legacy-reviewer-binding-v1\0",
        &CanonicalValue::object([("payload", CanonicalValue::string(&payload_digest))]),
    );
    tx.execute("insert into legacy_reviewer_bindings(project_id,source_ledger_digest,source_generation,source_reviewer_digest,principal_id,assertion_id,binding_handle,idempotency_key,payload_digest,created_at) values(?1,?2,?3,?4,?5,?6,?7,?8,?9,current_timestamp)",params![project,source,generation,reviewer,principal_id,assertion_id,handle.as_str(),request.idempotency_key,payload_digest])?;
    tx.execute("update authority_assertions set consumed_at=current_timestamp where id=?1 and consumed_at is null",params![assertion_id])?;
    tx.commit()?;
    Ok(LegacyReviewerBindingOutcome {
        binding_handle: handle.as_str().into(),
    })
}

#[derive(Clone, Debug)]
pub struct ReviewProvenanceOutcome {
    pub provenance_handle: String,
}

pub fn issue_review_provenance(
    root: &Path,
    request: ReviewProvenanceIssueRequest<'_>,
) -> Result<ReviewProvenanceOutcome> {
    let principal = PrincipalHandle::parse(request.principal_handle)?;
    let assertion = AssertionHandle::parse(request.assertion_handle)?;
    if request.review_plan_id <= 0 || request.idempotency_key.is_empty() {
        bail!("review provenance requires a positive plan and nonempty idempotency key");
    }
    let kind_code = match request.provenance_kind {
        "human_review" => 0,
        "external_agent" => 1,
        "service_review" => 2,
        _ => bail!("unsupported provenance kind"),
    };
    let purpose_code = match request.review_purpose {
        "new_unbiased_review" => 0,
        "finding_fix_verification" => 1,
        _ => bail!("unsupported review purpose"),
    };
    let reference = parse_hex::<32>(request.reference_digest, "reference digest")?;
    let payload_value = CanonicalValue::object([
        ("assertion", CanonicalValue::string(assertion.as_str())),
        ("plan", CanonicalValue::Integer(request.review_plan_id)),
        ("target", CanonicalValue::string(request.target_context)),
        ("kind", CanonicalValue::string(request.provenance_kind)),
        ("purpose", CanonicalValue::string(request.review_purpose)),
        (
            "reference",
            CanonicalValue::string(request.reference_digest),
        ),
    ]);
    let payload_digest = domain_digest(
        b"agent-workbench:review-provenance-issue-v1\0",
        &payload_value,
    );
    let handle =
        ReviewProvenanceHandle::derive(b"agent-workbench:review-provenance-v1\0", &payload_value);
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    let existing:Option<(String,String)>=tx.query_row("select provenance_handle,payload_digest from review_provenance_records where project_id=?1 and principal_id=(select id from authority_principals where project_id=?1 and principal_handle=?2) and idempotency_key=?3",params![project,principal.as_str(),request.idempotency_key],|row|Ok((row.get(0)?,row.get(1)?))).optional()?;
    if let Some((stored, stored_digest)) = existing {
        if stored_digest != payload_digest {
            bail!("review provenance idempotency payload mismatch");
        }
        return Ok(ReviewProvenanceOutcome {
            provenance_handle: stored,
        });
    }
    let (principal_id,subject_kind,subject_digest):(i64,String,String)=tx.query_row("select id,subject_kind,subject_digest from authority_principals where project_id=?1 and principal_handle=?2",params![project,principal.as_str()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).context("review provenance principal is not resolved")?;
    if !matches!(
        (subject_kind.as_str(), request.provenance_kind),
        ("human", "human_review") | ("agent", "external_agent") | ("service", "service_review")
    ) {
        bail!("principal subject kind does not match provenance kind");
    }
    crate::review::validate_invocation_plan_context(
        &tx,
        project,
        request.review_plan_id,
        request.target_context,
        request.review_purpose,
    )?;
    let (assertion_id, payload) = stored_assertion_payload(
        &tx,
        project,
        assertion.as_str(),
        "review_provenance",
        &subject_digest,
    )?;
    let expected = CborValue::Map(BTreeMap::from([
        (
            0,
            CborValue::Bytes(
                digest_reference(
                    b"agent-workbench:review-plan-ref-v1\0",
                    &request.review_plan_id.to_string(),
                )
                .to_vec(),
            ),
        ),
        (
            1,
            CborValue::Bytes(
                digest_reference(
                    b"agent-workbench:review-context-ref-v1\0",
                    request.target_context,
                )
                .to_vec(),
            ),
        ),
        (2, CborValue::U64(kind_code)),
        (3, CborValue::U64(purpose_code)),
        (4, CborValue::Bytes(reference.to_vec())),
    ]));
    if payload != expected {
        bail!("review provenance assertion payload mismatch");
    }
    tx.execute("insert into review_provenance_records(project_id,provenance_handle,principal_id,assertion_id,review_plan_id,target_context,provenance_kind,review_purpose,reference_digest,idempotency_key,payload_digest,created_at) values(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,current_timestamp)",params![project,handle.as_str(),principal_id,assertion_id,request.review_plan_id,request.target_context,request.provenance_kind,request.review_purpose,request.reference_digest,request.idempotency_key,payload_digest])?;
    tx.execute("update authority_assertions set consumed_at=current_timestamp where id=?1 and consumed_at is null",params![assertion_id])?;
    tx.commit()?;
    Ok(ReviewProvenanceOutcome {
        provenance_handle: handle.as_str().to_string(),
    })
}
