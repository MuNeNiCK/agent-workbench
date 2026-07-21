use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::db::{open_existing_project, project_id};
use crate::review_context::{
    DecompositionPlanReviewTarget, require_accepted_decomposition_plan_review,
    resolve_decomposition_plan_review_owner,
};
pub use crate::review_context::{PlanReviewOwnerResolution, PlanReviewOwnerState};

mod application;
mod compatibility;
mod document;
mod owner;
mod persistence;
mod reconciliation;
mod state;
pub use application::apply_decomposition_plan;
use application::*;
pub(crate) use compatibility::{install_uncovered_derived_bundles, uncovered_derived_bundle_count};
pub(crate) use document::*;
pub(crate) use owner::resolve_decomposition_owner;
pub(crate) use persistence::install_discovered_plans;
use persistence::*;
pub use reconciliation::*;
#[cfg(test)]
pub(crate) use state::recompute_dependency_state;
pub use state::show_decomposition_plan;
use state::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecompositionPlanQuery {
    pub design_version_id: i64,
    pub work_unit_id: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecompositionPlanRecord {
    pub id: i64,
    pub design_version_id: i64,
    pub work_unit_id: i64,
    pub key: String,
    pub revision: i64,
    pub current_identity: String,
    pub status: String,
    pub predecessor_id: Option<i64>,
    pub source_path: Option<String>,
    pub content_identity: String,
    pub document_content: String,
    pub issue: Option<String>,
    pub items: Vec<DecompositionItemRecord>,
    pub slices: Vec<DecompositionSliceRecord>,
    pub gaps: Vec<DecompositionGapRecord>,
    pub mappings: Vec<DecompositionMappingRecord>,
    pub shared_bindings: Vec<DecompositionSharedBindingRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecompositionPlanCandidate {
    pub source_path: String,
    pub ingress_identity: String,
    pub content_identity: String,
    pub managed_content_identity: String,
    pub structurally_ready: bool,
    pub issue: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecompositionPlanResolution {
    pub design_version_id: i64,
    pub work_unit_id: i64,
    pub current: Option<DecompositionPlanRecord>,
    pub successor: Option<DecompositionPlanRecord>,
    pub successor_projection: Option<DecompositionReconciliationProjection>,
    pub candidates: Vec<DecompositionPlanCandidate>,
    pub review_owner: Option<PlanReviewOwnerResolution>,
    pub actions: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecompositionGapRecord {
    pub endpoint: String,
    pub issue: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecompositionItemRecord {
    pub key: String,
    pub title: String,
    pub outcome: String,
    pub observation: String,
    pub evidence_owner: String,
    pub evidence_kind: String,
    pub slice: Option<String>,
    pub requirements: Vec<String>,
    pub checklist_boundaries: Vec<String>,
    pub gates: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecompositionSliceRecord {
    pub key: String,
    pub title: String,
    pub order: i64,
    pub depends_on: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DecompositionMappingRecord {
    pub category: String,
    pub source_id: i64,
    pub target: Option<String>,
    pub disposition: String,
    pub effect: Option<String>,
    pub qualification: String,
    pub observed_handle: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DecompositionSharedBindingRecord {
    pub kind: String,
    pub id: i64,
    pub owner: String,
    pub disposition: String,
    pub qualification: String,
    pub observed_handle: String,
}

pub struct DecompositionApplication<'a> {
    pub design_version_id: i64,
    pub work_unit_id: i64,
    pub plan_path: Option<&'a Path>,
}

pub struct DecompositionImport<'a> {
    pub design_version_id: i64,
    pub work_unit_id: i64,
    pub plan_path: Option<&'a Path>,
    pub expected_content: Option<&'a str>,
    pub draft: bool,
    pub expected_current: &'a str,
    pub idempotency_key: &'a str,
}

pub struct DecompositionValidate<'a> {
    pub plan_id: i64,
    pub expected_current: &'a str,
    pub idempotency_key: &'a str,
}

pub struct DecompositionRevise<'a> {
    pub plan_id: i64,
    pub plan_path: &'a Path,
    pub draft: bool,
    pub expected_current: &'a str,
    pub idempotency_key: &'a str,
}

struct DecompositionReviseRequest<'a> {
    pub plan_id: i64,
    pub plan_path: Option<&'a Path>,
    pub expected_content: Option<&'a str>,
    pub draft: bool,
    pub expected_current: &'a str,
    pub idempotency_key: &'a str,
}

impl DecompositionRevise<'_> {
    #[allow(clippy::too_many_arguments)]
    pub fn execute_request(
        root: &Path,
        plan_id: i64,
        plan_path: Option<&Path>,
        expected_content: Option<&str>,
        draft: bool,
        expected_current: &str,
        idempotency_key: &str,
    ) -> Result<DecompositionPlanTransitionOutcome> {
        revise_decomposition_plan_request(
            root,
            DecompositionReviseRequest {
                plan_id,
                plan_path,
                expected_content,
                draft,
                expected_current,
                idempotency_key,
            },
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecompositionPlanTransitionOutcome {
    pub plan: DecompositionPlanRecord,
    pub idempotent: bool,
}

pub(crate) struct DecompositionOwnerResolution {
    pub(crate) plan_id: i64,
    pub(crate) status: String,
    pub(crate) issue: Option<String>,
    pub(crate) actions: Vec<String>,
    pub(crate) blocks_work: bool,
}

struct DecompositionActionResolution {
    actions: Vec<String>,
    blocks_work: bool,
}

pub struct DecompositionReconciliationApplication<'a> {
    pub design_version_id: i64,
    pub work_unit_id: i64,
    pub plan_path: &'a Path,
    pub closure_id: i64,
    pub expected_current: &'a str,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DecompositionReconciliationOutcome {
    pub plan: DecompositionApplicationOutcome,
    pub predecessor_plan_id: i64,
    pub closure_id: i64,
    pub token_ordinal: i64,
    pub correction_application_id: i64,
    pub idempotent: bool,
    pub projection: DecompositionReconciliationProjection,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DecompositionReconciliationProjection {
    pub endpoint_effects: Vec<DecompositionMappingRecord>,
    pub shared_bindings: Vec<DecompositionSharedBindingRecord>,
    #[serde(default)]
    pub projection_identity: String,
    pub observed_predecessor: String,
    pub observed_document: String,
    pub observed_correction: String,
    pub observed_shared: String,
    pub commit_current: String,
    pub command: String,
}

struct PendingDecompositionReconciliation {
    parsed: ParsedPlan,
    project_id: i64,
    session_id: i64,
    token_id: i64,
    token_ordinal: i64,
    payload_identity: String,
    projection: DecompositionReconciliationProjection,
}

struct StoredReconciliationApplication {
    project: i64,
    successor: i64,
    predecessor: i64,
    correction_application: i64,
    closure: i64,
    session: i64,
    token: i64,
    token_ordinal: i64,
    operation: String,
    target: String,
    work: i64,
    design: Option<i64>,
    source_identity: String,
    source_path: Option<String>,
    content_identity: String,
    content: String,
}

enum DecompositionReconciliationResolution {
    Retry(Box<DecompositionReconciliationOutcome>),
    Pending(Box<PendingDecompositionReconciliation>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct GateMeaning {
    key: String,
    command: Option<String>,
    expected: String,
    environment: Option<String>,
    timeout: Option<String>,
    artifacts: Option<String>,
    requirement: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DecompositionApplicationOutcome {
    pub plan_id: i64,
    pub task_count: i64,
    pub checklist_item_count: i64,
    pub phase_count: i64,
    pub dependency_count: i64,
    pub already_applied: bool,
    pub applied: bool,
}

pub fn resolve_decomposition_plan(
    root: &Path,
    query: DecompositionPlanQuery,
) -> Result<DecompositionPlanResolution> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    validate_application_owner(
        &conn,
        project_id,
        query.design_version_id,
        query.work_unit_id,
    )?;
    resolve_decomposition_slot(&conn, root, project_id, query)
}

pub fn import_decomposition_plan(
    root: &Path,
    input: DecompositionImport<'_>,
) -> Result<DecompositionPlanTransitionOutcome> {
    require_key(input.idempotency_key, "decomposition idempotency key")?;
    if input.expected_current != "absent" {
        bail!("decomposition import requires --expected-current absent");
    }
    let preflighted = input
        .plan_path
        .map(|path| parse_plan_unvalidated(root, path))
        .transpose()?;
    if let (Some(expected), Some(parsed)) = (input.expected_content, preflighted.as_ref()) {
        require_digest(expected, "decomposition expected content")?;
        if parsed.source_identity != expected {
            bail!("Decomposition Plan ingress content changed before import");
        }
    } else if input.expected_content.is_some() {
        bail!("pathless decomposition import forbids --expected-content");
    }
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    validate_application_owner(&tx, project_id, input.design_version_id, input.work_unit_id)?;
    let mut parsed = match preflighted {
        Some(parsed) => parsed,
        None => empty_draft_plan(&tx, project_id, input.design_version_id, input.work_unit_id)?,
    };
    let document = parsed
        .document
        .clone()
        .context("Decomposition Plan metadata is required for import")?;
    validate_plan_header(&document)?;
    let draft = input.draft || input.plan_path.is_none();
    let ingress_identity = parsed.source_identity.clone();
    parsed.source_identity = lifecycle_source_identity(LifecycleSourceIdentity {
        operation: "import",
        idempotency_key: input.idempotency_key,
        expected_current: input.expected_current,
        predecessor_id: 0,
        revision: 1,
        draft,
        source_path: &parsed.source_path,
        source_identity: &ingress_identity,
    });
    validate_document_binding(
        &tx,
        project_id,
        input.design_version_id,
        input.work_unit_id,
        &parsed,
    )?;
    if let Some(existing) = plan_by_source_identity(&tx, project_id, &parsed.source_identity)? {
        let plan = load_decomposition_plan(&tx, existing)?;
        tx.commit()?;
        return Ok(DecompositionPlanTransitionOutcome {
            plan,
            idempotent: true,
        });
    }
    if resolve_current_plan_id(&tx, project_id, input.design_version_id, input.work_unit_id)?
        .is_some()
    {
        bail!("the selected package and work already have a current Decomposition Plan");
    }
    let issue = validate_plan(&document)
        .err()
        .map(|error| error.to_string());
    let status = if draft {
        "draft"
    } else if issue.is_some() {
        "incomplete"
    } else {
        "ready"
    };
    let plan_id = insert_lifecycle_plan(
        &tx,
        project_id,
        input.design_version_id,
        input.work_unit_id,
        &parsed,
        &ingress_identity,
        1,
        None,
        status,
        issue.as_deref(),
    )?;
    let plan = load_decomposition_plan(&tx, plan_id)?;
    tx.commit()?;
    Ok(DecompositionPlanTransitionOutcome {
        plan,
        idempotent: false,
    })
}

pub fn validate_decomposition_plan(
    root: &Path,
    input: DecompositionValidate<'_>,
) -> Result<DecompositionPlanTransitionOutcome> {
    require_key(input.idempotency_key, "decomposition idempotency key")?;
    require_digest(input.expected_current, "decomposition expected current")?;
    transition_decomposition_plan(
        root,
        input.plan_id,
        None,
        None,
        false,
        input.expected_current,
        input.idempotency_key,
    )
}

pub fn revise_decomposition_plan(
    root: &Path,
    input: DecompositionRevise<'_>,
) -> Result<DecompositionPlanTransitionOutcome> {
    revise_decomposition_plan_request(
        root,
        DecompositionReviseRequest {
            plan_id: input.plan_id,
            plan_path: Some(input.plan_path),
            expected_content: None,
            draft: input.draft,
            expected_current: input.expected_current,
            idempotency_key: input.idempotency_key,
        },
    )
}

fn revise_decomposition_plan_request(
    root: &Path,
    input: DecompositionReviseRequest<'_>,
) -> Result<DecompositionPlanTransitionOutcome> {
    require_key(input.idempotency_key, "decomposition idempotency key")?;
    require_digest(input.expected_current, "decomposition expected current")?;
    transition_decomposition_plan(
        root,
        input.plan_id,
        input.plan_path,
        input.expected_content,
        input.draft,
        input.expected_current,
        input.idempotency_key,
    )
}
