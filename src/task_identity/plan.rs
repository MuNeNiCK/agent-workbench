use anyhow::{Context, Result, bail};

use crate::identity::{CanonicalValue, OwnerHandle, PlanHandle, canonical_bytes};

use super::source::{OwnerSource, SourceSnapshot, TaskSource};

pub(super) mod assembly;
mod completion;
mod conflicts;
mod index;
mod lineage;
pub(super) mod model;
mod projection;
pub(super) mod resolution;
mod retirement;
mod scope;
mod tasks;

pub(crate) use scope::handles as owner_handles;
pub(super) use tasks::{PlannedTask, plan_task, plan_task_requirement};

pub(super) use index::render as render_index;

pub(super) fn render_owner_plan(snapshot: &SourceSnapshot, selector: &str) -> Result<String> {
    let requested = OwnerHandle::parse(selector)?;
    let owner = snapshot
        .owners
        .iter()
        .find(|owner| owner_handles(snapshot, owner).0 == requested)
        .context("owner handle is unknown or stale; rerun migration task-history plan")?;
    if owner.migrated {
        bail!("migration_not_required");
    }
    let plan = assembly::build(snapshot, owner)?;
    projection::render(snapshot, owner, &plan)
}

pub(super) fn render_resolved_plan(
    snapshot: &SourceSnapshot,
    owner: &OwnerSource,
    plan: &model::Plan,
    recovery_handle: String,
) -> Result<String> {
    projection::render_with_recovery(snapshot, owner, plan, Some(recovery_handle))
}

pub(super) fn owner_plan_digest(snapshot: &SourceSnapshot, owner: &OwnerSource) -> Result<String> {
    Ok(assembly::build(snapshot, owner)?.digest())
}

pub(super) fn owner_plan_handle(owner: &OwnerSource, plan_digest: &str, mode: &str) -> PlanHandle {
    let binding = canonical_bytes(&CanonicalValue::object([
        ("owner", CanonicalValue::string(owner.owner_digest.clone())),
        (
            "component",
            CanonicalValue::string(owner.component_digest.clone()),
        ),
        ("mode", CanonicalValue::string(mode)),
    ]));
    PlanHandle::derive_raw(b"AWB-PLAN-HANDLE-v1\0", &[plan_digest.as_bytes(), &binding])
}
