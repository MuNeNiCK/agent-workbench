use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::identity::{
    AmbiguityHandle, AuthorityHandle, BackupHandle, CanonicalValue, DecisionHandle, OwnerHandle,
    PlanHandle, RecoveryHandle, ResolutionHandle, domain_digest, normalize_identifier,
};
use anyhow::{Context, Result, bail};

pub(crate) mod audit;
mod model;
mod projection;
mod store;
mod view;

use model::{
    MigrationAuthority, MigrationDecision, RecoveryEnvelope, authority_value, decision_value,
    recovery_digest,
};
use store::{load_envelope, persist_envelope};

use super::apply::create_verified_backup;
use super::plan::model::{OperationalAmbiguity, OperationalResolution, Plan};
use super::plan::{assembly, owner_handles, owner_plan_digest, owner_plan_handle, resolution};
use super::source::{OwnerSource, SourceSnapshot};
use super::{
    TaskIdentityAuthorityOutput, TaskIdentityAuthorityRequest, TaskIdentityDecisionOutput,
    TaskIdentityDecisionRequest,
};

struct BaseSelection<'a> {
    snapshot: SourceSnapshot,
    owner_index: usize,
    plan_digest: String,
    plan_handle: PlanHandle,
    ambiguity_digest: String,
    resolutions: Vec<OperationalResolution>,
    _marker: std::marker::PhantomData<&'a ()>,
}

pub(super) struct ApplyResolution {
    pub(super) plan_digest: String,
    pub(super) plan_handle: PlanHandle,
    pub(super) mode: &'static str,
    pub(super) retired_task_ids: BTreeSet<i64>,
    pub(super) retired_dependency_ids: BTreeSet<i64>,
    pub(super) selected_requirement_ids: BTreeMap<i64, i64>,
}

pub(super) fn resolve_for_apply(
    root: &Path,
    snapshot: &SourceSnapshot,
    owner: &OwnerSource,
    plan_selector: &str,
) -> Result<ApplyResolution> {
    let requested = PlanHandle::parse(plan_selector)?;
    let base_digest = owner_plan_digest(snapshot, owner)?;
    let base_handle = owner_plan_handle(owner, &base_digest, "base");
    let ambiguities = planned_ambiguities(snapshot, owner)?;
    if ambiguities.is_empty() {
        if requested != base_handle {
            bail!("plan handle is unknown or stale; rerun migration task-history plan");
        }
        return Ok(ApplyResolution {
            plan_digest: base_digest,
            plan_handle: base_handle,
            mode: "base",
            retired_task_ids: BTreeSet::new(),
            retired_dependency_ids: BTreeSet::new(),
            selected_requirement_ids: BTreeMap::new(),
        });
    }
    let envelope = load_envelope(root, &owner.owner_digest)?
        .context("ambiguity_required: record authority and decisions before apply")?;
    validate_envelope_bindings(snapshot, owner, &base_digest, &envelope)?;
    let projection = projection::project(&ambiguities, &envelope)?;
    for ambiguity in &ambiguities {
        envelope
            .decisions
            .iter()
            .find(|decision| decision.ambiguity_digest == ambiguity.digest)
            .context("ambiguity_required: every ambiguity requires a decision before apply")?;
    }
    if envelope.decisions.len() != ambiguities.len() {
        bail!("recovery envelope contains a decision outside the selected base plan");
    }
    let recovery_digest = recovery_digest(&envelope);
    let resolved_plan = build_resolved_plan(snapshot, owner, &recovery_digest, &envelope)?;
    let resolved_digest = resolved_plan.digest();
    let resolved_handle = owner_plan_handle(owner, &resolved_digest, "resolved");
    if requested != resolved_handle {
        bail!("plan handle is unknown or stale; rerun migration task-history plan");
    }
    Ok(ApplyResolution {
        plan_digest: resolved_digest,
        plan_handle: resolved_handle,
        mode: "resolved",
        retired_task_ids: projection.retired_task_ids,
        retired_dependency_ids: projection.retired_dependency_ids,
        selected_requirement_ids: projection.selected_requirement_ids,
    })
}

pub(super) fn record_authority(
    root: &Path,
    request: TaskIdentityAuthorityRequest<'_>,
) -> Result<TaskIdentityAuthorityOutput> {
    if request.provenance != "user_instruction" {
        bail!("task-history authority provenance must be user_instruction");
    }
    let selection = select_base(
        root,
        request.owner_handle,
        request.plan_handle,
        request.ambiguity_handle,
    )?;
    let (action, resolution_digest) = selected_action(
        &selection,
        request.resolution_handle,
        request.retire,
        "authority",
    )?;
    let owner = &selection.snapshot.owners[selection.owner_index];
    let backup = create_verified_backup(
        root,
        owner,
        &selection.plan_digest,
        &selection.snapshot.database_digest,
    )?;
    let mut envelope = load_or_new_envelope(root, &selection)?;
    let mut authority = MigrationAuthority {
        digest: String::new(),
        project_digest: selection.snapshot.project_digest.clone(),
        owner_digest: owner.owner_digest.clone(),
        component_sha256: owner.component_digest.clone(),
        source_sha256: owner.source_digest.clone(),
        base_plan_sha256: selection.plan_digest.clone(),
        ambiguity_digest: selection.ambiguity_digest.clone(),
        action: action.to_string(),
        resolution_digest,
        statement: normalize_identifier(request.statement),
        provenance: request.provenance.to_string(),
        provenance_ref: normalize_identifier(request.provenance_ref),
    };
    authority.digest = domain_digest(
        b"AWB-RECOVERY-AUTHORITY-v1\0",
        &authority_value(&authority, false),
    );
    if let Some(existing) = envelope
        .authorities
        .iter()
        .find(|existing| existing.ambiguity_digest == authority.ambiguity_digest)
    {
        if existing != &authority {
            bail!("ambiguity already has a conflicting migration authority");
        }
    } else {
        envelope.authorities.push(authority.clone());
        envelope.authorities.sort_by(|left, right| {
            left.ambiguity_digest
                .cmp(&right.ambiguity_digest)
                .then(left.digest.cmp(&right.digest))
        });
    }
    let recovery_digest = persist_envelope(root, &envelope)?;
    let authority_handle = authority_handle(owner, &selection.plan_digest, &authority.digest);
    let recovery_handle = recovery_handle(owner, &selection.plan_digest, &recovery_digest);
    let backup_handle = backup_handle(&selection.plan_handle, &backup.digest);
    Ok(TaskIdentityAuthorityOutput {
        classification: "project-internal",
        authority_handle: authority_handle.as_str().to_string(),
        recovery_handle: recovery_handle.as_str().to_string(),
        backup_handle: backup_handle.as_str().to_string(),
    })
}

pub(super) fn record_decision(
    root: &Path,
    request: TaskIdentityDecisionRequest<'_>,
) -> Result<TaskIdentityDecisionOutput> {
    let selection = select_base(
        root,
        request.owner_handle,
        request.plan_handle,
        request.ambiguity_handle,
    )?;
    let (action, resolution_digest) = selected_action(
        &selection,
        request.resolution_handle,
        request.retire,
        "decision",
    )?;
    let owner = &selection.snapshot.owners[selection.owner_index];
    let mut envelope = load_or_new_envelope(root, &selection)?;
    let requested_authority = AuthorityHandle::parse(request.authority_handle)?;
    let authority = envelope
        .authorities
        .iter()
        .find(|authority| {
            authority.ambiguity_digest == selection.ambiguity_digest
                && authority.action == action
                && authority.resolution_digest == resolution_digest
                && authority_handle(owner, &selection.plan_digest, &authority.digest)
                    == requested_authority
        })
        .cloned()
        .context("authority handle does not authorize the selected ambiguity action")?;
    let mut decision = MigrationDecision {
        digest: String::new(),
        project_digest: selection.snapshot.project_digest.clone(),
        owner_digest: owner.owner_digest.clone(),
        component_sha256: owner.component_digest.clone(),
        source_sha256: owner.source_digest.clone(),
        base_plan_sha256: selection.plan_digest.clone(),
        ambiguity_digest: selection.ambiguity_digest.clone(),
        action: action.to_string(),
        resolution_digest,
        authority_digest: authority.digest.clone(),
    };
    decision.digest = domain_digest(
        b"AWB-RECOVERY-DECISION-v1\0",
        &decision_value(&decision, false),
    );
    if let Some(existing) = envelope
        .decisions
        .iter()
        .find(|existing| existing.ambiguity_digest == decision.ambiguity_digest)
    {
        if existing != &decision {
            bail!("ambiguity already has a conflicting migration decision");
        }
    } else {
        envelope.decisions.push(decision.clone());
        envelope.decisions.sort_by(|left, right| {
            left.ambiguity_digest
                .cmp(&right.ambiguity_digest)
                .then(left.digest.cmp(&right.digest))
        });
    }
    let recovery_digest = persist_envelope(root, &envelope)?;
    let resolved_plan =
        build_resolved_plan(&selection.snapshot, owner, &recovery_digest, &envelope)?;
    let decision_handle = DecisionHandle::derive(
        b"AWB-DECISION-HANDLE-v1\0",
        &CanonicalValue::object([
            (
                "base_plan",
                CanonicalValue::string(selection.plan_digest.clone()),
            ),
            ("owner", CanonicalValue::string(owner.owner_digest.clone())),
            (
                "component",
                CanonicalValue::string(owner.component_digest.clone()),
            ),
            (
                "ambiguity",
                CanonicalValue::string(decision.ambiguity_digest),
            ),
            ("decision", CanonicalValue::string(decision.digest)),
        ]),
    );
    let recovery_handle = recovery_handle(owner, &selection.plan_digest, &recovery_digest);
    let json = view::render(&selection, &resolved_plan, &recovery_handle)?;
    Ok(TaskIdentityDecisionOutput {
        classification: "project-internal",
        decision_handle: decision_handle.as_str().to_string(),
        recovery_handle: recovery_handle.as_str().to_string(),
        json,
    })
}

fn select_base<'a>(
    root: &Path,
    owner_selector: &str,
    plan_selector: &str,
    ambiguity_selector: &str,
) -> Result<BaseSelection<'a>> {
    let snapshot = SourceSnapshot::open(root)?;
    let requested_owner = OwnerHandle::parse(owner_selector)?;
    let owner_index = snapshot
        .owners
        .iter()
        .position(|owner| owner_handles(&snapshot, owner).0 == requested_owner)
        .context("owner handle is unknown or stale; rerun migration task-history plan")?;
    let owner = &snapshot.owners[owner_index];
    let plan_digest = owner_plan_digest(&snapshot, owner)?;
    let plan_handle = owner_plan_handle(owner, &plan_digest, "base");
    if PlanHandle::parse(plan_selector)? != plan_handle {
        bail!("plan handle is unknown or stale; rerun migration task-history plan");
    }
    let requested_ambiguity = AmbiguityHandle::parse(ambiguity_selector)?;
    let ambiguity = planned_ambiguities(&snapshot, owner)?
        .into_iter()
        .find(|ambiguity| ambiguity_handle(&plan_digest, &ambiguity.digest) == requested_ambiguity)
        .context("ambiguity handle is unknown or stale; rerun ambiguity-list")?;
    Ok(BaseSelection {
        snapshot,
        owner_index,
        plan_digest,
        plan_handle,
        ambiguity_digest: ambiguity.digest,
        resolutions: ambiguity.resolutions,
        _marker: std::marker::PhantomData,
    })
}

fn load_or_new_envelope(root: &Path, selection: &BaseSelection<'_>) -> Result<RecoveryEnvelope> {
    let owner = &selection.snapshot.owners[selection.owner_index];
    let Some(envelope) = load_envelope(root, &owner.owner_digest)? else {
        return Ok(RecoveryEnvelope {
            algorithm: "AWB-RECOVERY-v1".to_string(),
            project_digest: selection.snapshot.project_digest.clone(),
            owner_digest: owner.owner_digest.clone(),
            component_sha256: owner.component_digest.clone(),
            source_sha256: owner.source_digest.clone(),
            base_plan_sha256: selection.plan_digest.clone(),
            authorities: Vec::new(),
            decisions: Vec::new(),
        });
    };
    validate_envelope_bindings(
        &selection.snapshot,
        owner,
        &selection.plan_digest,
        &envelope,
    )?;
    Ok(envelope)
}

fn validate_envelope_bindings(
    snapshot: &SourceSnapshot,
    owner: &OwnerSource,
    base_plan_digest: &str,
    envelope: &RecoveryEnvelope,
) -> Result<()> {
    if envelope.algorithm != "AWB-RECOVERY-v1"
        || envelope.project_digest != snapshot.project_digest
        || envelope.owner_digest != owner.owner_digest
        || envelope.component_sha256 != owner.component_digest
        || envelope.source_sha256 != owner.source_digest
        || envelope.base_plan_sha256 != base_plan_digest
    {
        bail!("source_drift: recovery envelope does not match the selected base plan");
    }
    Ok(())
}

fn planned_ambiguities(
    snapshot: &SourceSnapshot,
    owner: &OwnerSource,
) -> Result<Vec<OperationalAmbiguity>> {
    Ok(assembly::build(snapshot, owner)?.ambiguities())
}

fn ambiguity_handle(plan_digest: &str, ambiguity_digest: &str) -> AmbiguityHandle {
    let binding = crate::identity::canonical_bytes(&CanonicalValue::string(ambiguity_digest));
    AmbiguityHandle::derive_raw(
        b"AWB-DOMAIN-HANDLE-v1\0",
        &[plan_digest.as_bytes(), b"ambiguity", &binding],
    )
}

fn resolution_handle(plan_digest: &str, resolution_digest: &str) -> ResolutionHandle {
    let binding = crate::identity::canonical_bytes(&CanonicalValue::string(resolution_digest));
    ResolutionHandle::derive_raw(
        b"AWB-DOMAIN-HANDLE-v1\0",
        &[plan_digest.as_bytes(), b"resolution", &binding],
    )
}

fn selected_action(
    selection: &BaseSelection<'_>,
    resolution_selector: Option<&str>,
    retire: bool,
    operation: &str,
) -> Result<(&'static str, Option<String>)> {
    match (resolution_selector, retire) {
        (Some(_), true) | (None, false) => {
            bail!("ambiguity {operation} requires exactly one resolution or --retire")
        }
        (None, true) => Ok(("retire", None)),
        (Some(selector), false) => {
            let requested = ResolutionHandle::parse(selector)?;
            let selected = selection
                .resolutions
                .iter()
                .find(|resolution| {
                    resolution_handle(&selection.plan_digest, &resolution.digest) == requested
                })
                .context("resolution handle is unknown or stale; rerun ambiguity-list")?;
            Ok(("map", Some(selected.digest.clone())))
        }
    }
}

fn build_resolved_plan(
    snapshot: &SourceSnapshot,
    owner: &OwnerSource,
    recovery_digest: &str,
    envelope: &RecoveryEnvelope,
) -> Result<Plan> {
    let base = assembly::build(snapshot, owner)?;
    let ambiguities = base.ambiguities();
    let projection = projection::project(&ambiguities, envelope)?;
    let decisions = envelope
        .decisions
        .iter()
        .map(|decision| resolution::ResolutionDecision {
            ambiguity_digest: decision.ambiguity_digest.clone(),
            action: decision.action.clone(),
            resolution_digest: decision.resolution_digest.clone(),
            authority_digest: decision.authority_digest.clone(),
        })
        .collect::<Vec<_>>();
    resolution::resolve(resolution::ResolvePlan {
        snapshot,
        owner,
        base: &base,
        recovery_digest,
        selected_requirements: &projection.selected_requirement_ids,
        retired_tasks: &projection.retired_task_ids,
        retired_dependencies: &projection.retired_dependency_ids,
        decisions: &decisions,
    })
}

fn authority_handle(owner: &OwnerSource, base_plan: &str, digest: &str) -> AuthorityHandle {
    AuthorityHandle::derive(
        b"AWB-AUTHORITY-HANDLE-v1\0",
        &CanonicalValue::object([
            ("base_plan", CanonicalValue::string(base_plan)),
            ("owner", CanonicalValue::string(owner.owner_digest.clone())),
            (
                "component",
                CanonicalValue::string(owner.component_digest.clone()),
            ),
            ("authority", CanonicalValue::string(digest)),
        ]),
    )
}

fn recovery_handle(owner: &OwnerSource, base_plan: &str, digest: &str) -> RecoveryHandle {
    RecoveryHandle::derive(
        b"AWB-RECOVERY-HANDLE-v1\0",
        &CanonicalValue::object([
            ("base_plan", CanonicalValue::string(base_plan)),
            ("owner", CanonicalValue::string(owner.owner_digest.clone())),
            (
                "component",
                CanonicalValue::string(owner.component_digest.clone()),
            ),
            (
                "source",
                CanonicalValue::string(owner.source_digest.clone()),
            ),
            ("recovery", CanonicalValue::string(digest)),
        ]),
    )
}

fn backup_handle(plan: &PlanHandle, digest: &str) -> BackupHandle {
    BackupHandle::derive(
        b"AWB-BACKUP-HANDLE-v1\0",
        &CanonicalValue::object([
            ("plan", CanonicalValue::string(plan.as_str())),
            ("backup", CanonicalValue::string(digest)),
        ]),
    )
}
