use anyhow::Result;
use serde::Serialize;
use serde_json::{Value, json};

use crate::identity::{
    AmbiguityHandle, CanonicalValue, CompletionHandle, DependencyHandle, EntryHandle,
    EvidenceHandle, GateSetHandle, MembershipHandle, PhaseHandle, PlanHandle, PriorityHandle,
    RefHandle, RequirementHandle, ResolutionHandle, RevisionHandle, TaskHandle, canonical_bytes,
};

use super::model::{
    ArrayKind, Element, Fingerprints, Payload, Plan, SourceRef, Target, TargetKind,
};
use super::{OwnerSource, SourceSnapshot, owner_handles};

#[derive(Serialize)]
struct Output {
    view: &'static str,
    plan: View,
}

#[derive(Serialize)]
struct View {
    algorithm: &'static str,
    mode: &'static str,
    base_plan_handle: Option<PlanHandle>,
    recovery_handle: Option<String>,
    scope: Scope,
    plan_handle: PlanHandle,
    task_identities: Vec<Entry>,
    revisions: Vec<Entry>,
    aliases: Vec<Entry>,
    memberships: Vec<Entry>,
    dependencies: Vec<Entry>,
    completion_claims: Vec<Entry>,
    retirements: Vec<Entry>,
    dispositions: Vec<Entry>,
    ambiguities: Vec<Entry>,
}

#[derive(Serialize)]
struct Scope {
    work_unit_id: i64,
    owner_handle: crate::identity::OwnerHandle,
    component_handle: crate::identity::ComponentHandle,
}

#[derive(Serialize)]
struct Entry {
    entry_handle: EntryHandle,
    target_handle: Option<String>,
    classification: &'static str,
    reason: &'static str,
    before: State,
    after: State,
    payload: Value,
}

#[derive(Serialize)]
struct State {
    requirement_handle: Option<RequirementHandle>,
    gate_set_handle: Option<GateSetHandle>,
    priority_handle: Option<PriorityHandle>,
    status: Option<&'static str>,
}

pub(super) fn render(
    snapshot: &SourceSnapshot,
    owner: &OwnerSource,
    plan: &Plan,
) -> Result<String> {
    render_with_recovery(snapshot, owner, plan, None)
}

pub(super) fn render_with_recovery(
    snapshot: &SourceSnapshot,
    owner: &OwnerSource,
    plan: &Plan,
    recovery_handle: Option<String>,
) -> Result<String> {
    let digest = plan.digest();
    let plan_handle = super::owner_plan_handle(owner, &digest, plan.mode);
    let handles = owner_handles(snapshot, owner);
    let mut arrays: [Vec<Entry>; 9] = Default::default();
    for element in &plan.elements {
        arrays[element.array as usize].push(project_element(owner, plan, &digest, element)?);
    }
    serde_json::to_string(&Output {
        view: "owner_plan",
        plan: View {
            algorithm: "ID-PLAN-VIEW-v1",
            mode: plan.mode,
            base_plan_handle: plan
                .base_plan_sha256
                .as_ref()
                .map(|base| super::owner_plan_handle(owner, base, "base")),
            recovery_handle,
            scope: Scope {
                work_unit_id: owner.owner_id,
                owner_handle: handles.0,
                component_handle: handles.1,
            },
            plan_handle,
            task_identities: std::mem::take(&mut arrays[0]),
            revisions: std::mem::take(&mut arrays[1]),
            aliases: std::mem::take(&mut arrays[2]),
            memberships: std::mem::take(&mut arrays[3]),
            dependencies: std::mem::take(&mut arrays[4]),
            completion_claims: std::mem::take(&mut arrays[5]),
            retirements: std::mem::take(&mut arrays[6]),
            dispositions: std::mem::take(&mut arrays[7]),
            ambiguities: std::mem::take(&mut arrays[8]),
        },
    })
    .map_err(Into::into)
}

fn project_element(
    owner: &OwnerSource,
    plan: &Plan,
    digest: &str,
    element: &Element,
) -> Result<Entry> {
    let target_handle = element
        .target
        .as_ref()
        .map(|target| target_handle(digest, target))
        .transpose()?;
    Ok(Entry {
        entry_handle: entry_handle(digest, element.array, &element.sort_digest),
        target_handle,
        classification: element.classification.as_str(),
        reason: element.reason.as_str(),
        before: project_state(digest, &element.before),
        after: project_state(digest, &element.after),
        payload: project_payload(owner, plan, digest, element)?,
    })
}

fn project_payload(
    owner: &OwnerSource,
    plan: &Plan,
    digest: &str,
    element: &Element,
) -> Result<Value> {
    let recovery_scope = plan.base_plan_sha256.as_deref().unwrap_or(digest);
    match &element.payload {
        Payload::TaskIdentity { .. } => Ok(json!({
            "task_handle": required_target::<TaskHandle>(digest, &element.target, TargetKind::TaskIdentity)?,
        })),
        Payload::Revision {
            identity_digest, ..
        } => Ok(json!({
            "task_handle": task_handle(digest, identity_digest),
            "revision_handle": required_target::<RevisionHandle>(digest, &element.target, TargetKind::Revision)?,
        })),
        Payload::Alias {
            revision_digest, ..
        } => Ok(json!({
            "revision_handle": revision_handle(digest, revision_digest),
        })),
        Payload::Membership {
            phase,
            identity_digest,
            state,
        } => Ok(json!({
            "phase_handle": phase_handle(digest, phase),
            "task_handle": task_handle(digest, identity_digest),
            "state": state.as_str(),
        })),
        Payload::Dependency {
            from_task,
            to_task,
            state,
        } => Ok(json!({
            "from_task_handle": task_handle(digest, from_task),
            "to_task_handle": task_handle(digest, to_task),
            "state": state.as_str(),
        })),
        Payload::Completion {
            revision_digest,
            phase,
            claim,
            result,
            evidence,
        } => Ok(json!({
            "revision_handle": revision_handle(digest, revision_digest),
            "phase_handle": phase.as_ref().map(|phase| phase_handle(digest, phase)),
            "claim": claim.as_str(),
            "result": result.as_str(),
            "evidence_handles": evidence.iter().map(|value| semantic_handle::<EvidenceHandle>(digest, "evidence", value)).collect::<Vec<_>>(),
        })),
        Payload::Retirement {
            revision_digest, ..
        } => Ok(json!({
            "revision_handle": revision_handle(digest, revision_digest),
        })),
        Payload::Disposition {
            ambiguity_digest,
            action,
            resolution_digest,
            authority_digest,
            selected_source_refs,
            retired_source_refs,
        } => Ok(json!({
            "ambiguity_handle": ambiguity_handle(recovery_scope, ambiguity_digest),
            "action": action.as_str(),
            "resolution_handle": resolution_digest.as_ref().map(|value| resolution_handle(recovery_scope, value)),
            "authority_handle": authority_handle(owner, plan, authority_digest),
            "selected_ref_handles": selected_source_refs.iter().map(|value| ref_handle(recovery_scope, value)).collect::<Vec<_>>(),
            "retired_ref_handles": retired_source_refs.iter().map(|value| ref_handle(recovery_scope, value)).collect::<Vec<_>>(),
        })),
        Payload::Ambiguity {
            code,
            claims,
            candidates,
            resolutions,
        } => Ok(json!({
            "ambiguity_handle": ambiguity_handle(recovery_scope, &element.sort_digest),
            "code": code.as_str(),
            "claims": claims.iter().map(|claim| claim.as_str()).collect::<Vec<_>>(),
            "candidate_handles": candidates.iter().map(|target| target_handle(digest, target)).collect::<Result<Vec<_>>>()?,
            "resolutions": resolutions.iter().map(|resolution| {
                Ok(json!({
                    "resolution_handle": resolution_handle(recovery_scope, &resolution.digest),
                    "selected_ref_handles": resolution.selected_source_refs.iter().map(|value| ref_handle(recovery_scope, value)).collect::<Vec<_>>(),
                    "retired_ref_handles": resolution.retired_source_refs.iter().map(|value| ref_handle(recovery_scope, value)).collect::<Vec<_>>(),
                    "remove_entry_handles": resolution.remove.iter().map(|coordinate| entry_handle(digest, coordinate.array, &coordinate.sort_digest)).collect::<Vec<_>>(),
                    "add_entries": resolution.add.iter().map(|added| project_element(owner, plan, digest, &added.element)).collect::<Result<Vec<_>>>()?,
                }))
            }).collect::<Result<Vec<_>>>()?,
        })),
    }
}

fn project_state(digest: &str, value: &Fingerprints) -> State {
    State {
        requirement_handle: value
            .requirement
            .as_ref()
            .map(|value| semantic_handle::<RequirementHandle>(digest, "requirement", value)),
        gate_set_handle: value
            .gates
            .as_ref()
            .map(|value| semantic_handle::<GateSetHandle>(digest, "gate_set", value)),
        priority_handle: value
            .priority
            .as_ref()
            .map(|value| semantic_handle::<PriorityHandle>(digest, "priority", value)),
        status: value.status.map(|status| status.as_str()),
    }
}

trait RawHandle {
    fn raw(domain: &[u8], parts: &[&[u8]]) -> Self;
}

macro_rules! raw_handle {
    ($type:ty, $domain:literal) => {
        impl RawHandle for $type {
            fn raw(_: &[u8], parts: &[&[u8]]) -> Self {
                <$type>::derive_raw($domain, parts)
            }
        }
    };
}

raw_handle!(TaskHandle, b"AWB-DOMAIN-HANDLE-v1\0");
raw_handle!(RevisionHandle, b"AWB-DOMAIN-HANDLE-v1\0");

fn required_target<T: RawHandle + Serialize>(
    digest: &str,
    target: &Option<Target>,
    kind: TargetKind,
) -> Result<T> {
    let target = target
        .as_ref()
        .filter(|target| target.kind == kind)
        .ok_or_else(|| anyhow::anyhow!("internal projection target kind mismatch"))?;
    Ok(T::raw(
        b"AWB-DOMAIN-HANDLE-v1\0",
        &[
            digest.as_bytes(),
            kind.as_str().as_bytes(),
            &canonical_bytes(&CanonicalValue::string(target.digest.clone())),
        ],
    ))
}

fn target_handle(digest: &str, target: &Target) -> Result<String> {
    let binding = canonical_bytes(&CanonicalValue::string(target.digest.clone()));
    let parts = &[
        digest.as_bytes(),
        target.kind.as_str().as_bytes(),
        binding.as_slice(),
    ];
    Ok(match target.kind {
        TargetKind::TaskIdentity => TaskHandle::derive_raw(b"AWB-DOMAIN-HANDLE-v1\0", parts)
            .as_str()
            .to_string(),
        TargetKind::Revision => RevisionHandle::derive_raw(b"AWB-DOMAIN-HANDLE-v1\0", parts)
            .as_str()
            .to_string(),
        TargetKind::Membership => MembershipHandle::derive_raw(b"AWB-DOMAIN-HANDLE-v1\0", parts)
            .as_str()
            .to_string(),
        TargetKind::Dependency => DependencyHandle::derive_raw(b"AWB-DOMAIN-HANDLE-v1\0", parts)
            .as_str()
            .to_string(),
        TargetKind::Completion => CompletionHandle::derive_raw(b"AWB-DOMAIN-HANDLE-v1\0", parts)
            .as_str()
            .to_string(),
    })
}

fn domain_handle<T: RawHandle>(digest: &str, kind: &str, value: &str) -> T {
    let binding = canonical_bytes(&CanonicalValue::string(value));
    T::raw(
        b"AWB-DOMAIN-HANDLE-v1\0",
        &[digest.as_bytes(), kind.as_bytes(), &binding],
    )
}

fn semantic_handle<T: RawHandle>(digest: &str, kind: &str, value: &str) -> T {
    T::raw(
        b"AWB-SEMANTIC-HANDLE-v1\0",
        &[digest.as_bytes(), kind.as_bytes(), value.as_bytes()],
    )
}

macro_rules! semantic_raw_handle {
    ($type:ty) => {
        impl RawHandle for $type {
            fn raw(domain: &[u8], parts: &[&[u8]]) -> Self {
                <$type>::derive_raw(domain, parts)
            }
        }
    };
}
semantic_raw_handle!(RequirementHandle);
semantic_raw_handle!(GateSetHandle);
semantic_raw_handle!(PriorityHandle);
semantic_raw_handle!(EvidenceHandle);

fn task_handle(digest: &str, value: &str) -> TaskHandle {
    domain_handle(digest, "task_identity", value)
}
fn revision_handle(digest: &str, value: &str) -> RevisionHandle {
    domain_handle(digest, "revision", value)
}
fn phase_handle(digest: &str, value: &str) -> PhaseHandle {
    let binding = canonical_bytes(&CanonicalValue::string(value));
    PhaseHandle::derive_raw(
        b"AWB-DOMAIN-HANDLE-v1\0",
        &[digest.as_bytes(), b"phase", &binding],
    )
}
fn ambiguity_handle(digest: &str, value: &str) -> AmbiguityHandle {
    let binding = canonical_bytes(&CanonicalValue::string(value));
    AmbiguityHandle::derive_raw(
        b"AWB-DOMAIN-HANDLE-v1\0",
        &[digest.as_bytes(), b"ambiguity", &binding],
    )
}
fn resolution_handle(digest: &str, value: &str) -> ResolutionHandle {
    let binding = canonical_bytes(&CanonicalValue::string(value));
    ResolutionHandle::derive_raw(
        b"AWB-DOMAIN-HANDLE-v1\0",
        &[digest.as_bytes(), b"resolution", &binding],
    )
}
fn ref_handle(digest: &str, value: &SourceRef) -> RefHandle {
    let binding = canonical_bytes(&value.value());
    RefHandle::derive_raw(b"AWB-REF-HANDLE-v1\0", &[digest.as_bytes(), &binding])
}
fn entry_handle(digest: &str, array: ArrayKind, sort_digest: &str) -> EntryHandle {
    EntryHandle::derive_raw(
        b"AWB-ENTRY-HANDLE-v1\0",
        &[
            digest.as_bytes(),
            array.as_str().as_bytes(),
            sort_digest.as_bytes(),
        ],
    )
}
fn authority_handle(
    owner: &OwnerSource,
    plan: &Plan,
    value: &str,
) -> crate::identity::AuthorityHandle {
    let computed_base;
    let base = if let Some(base) = plan.base_plan_sha256.as_deref() {
        base
    } else {
        computed_base = plan.digest();
        &computed_base
    };
    let binding = canonical_bytes(&CanonicalValue::object([
        (
            "owner_digest",
            CanonicalValue::string(owner.owner_digest.clone()),
        ),
        (
            "component_sha256",
            CanonicalValue::string(owner.component_digest.clone()),
        ),
        ("internal_authority_digest", CanonicalValue::string(value)),
    ]));
    crate::identity::AuthorityHandle::derive_raw(
        b"AWB-AUTHORITY-HANDLE-v1\0",
        &[base.as_bytes(), &binding],
    )
}
