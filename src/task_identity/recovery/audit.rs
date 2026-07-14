use anyhow::Result;
use serde::Serialize;

use crate::identity::{OwnerHandle, PlanHandle, RecoveryHandle};

use super::{
    SourceSnapshot, load_envelope, owner_handles, owner_plan_digest, owner_plan_handle,
    planned_ambiguities, recovery_digest, recovery_handle, validate_envelope_bindings,
};

#[derive(Serialize)]
pub(crate) struct PendingRecoveryView {
    owner_handle: OwnerHandle,
    base_plan_handle: PlanHandle,
    recovery_handle: RecoveryHandle,
    ambiguity_count: usize,
    authority_count: usize,
    decision_count: usize,
    state: &'static str,
}

pub(crate) fn pending(
    root: &std::path::Path,
    snapshot: &SourceSnapshot,
    selected_owner: Option<&OwnerHandle>,
) -> Result<Vec<PendingRecoveryView>> {
    let mut pending = Vec::new();
    for owner in snapshot.owners.iter().filter(|owner| !owner.migrated) {
        let owner_handle = owner_handles(snapshot, owner).0;
        if selected_owner.is_some_and(|selected| selected != &owner_handle) {
            continue;
        }
        let Some(envelope) = load_envelope(root, &owner.owner_digest)? else {
            continue;
        };
        let base_plan_digest = owner_plan_digest(snapshot, owner)?;
        validate_envelope_bindings(snapshot, owner, &base_plan_digest, &envelope)?;
        let ambiguities = planned_ambiguities(snapshot, owner)?;
        let fully_resolved = envelope.decisions.len() == ambiguities.len()
            && envelope.authorities.len() == ambiguities.len()
            && ambiguities.iter().all(|ambiguity| {
                envelope.decisions.iter().any(|decision| {
                    decision.ambiguity_digest == ambiguity.digest
                        && envelope
                            .authorities
                            .iter()
                            .any(|authority| authority.digest == decision.authority_digest)
                })
            });
        let digest = recovery_digest(&envelope);
        pending.push(PendingRecoveryView {
            owner_handle,
            base_plan_handle: owner_plan_handle(owner, &base_plan_digest, "base"),
            recovery_handle: recovery_handle(owner, &base_plan_digest, &digest),
            ambiguity_count: ambiguities.len(),
            authority_count: envelope.authorities.len(),
            decision_count: envelope.decisions.len(),
            state: if fully_resolved {
                "resolved"
            } else {
                "authority_required"
            },
        });
    }
    pending.sort_by(|left, right| left.owner_handle.cmp(&right.owner_handle));
    Ok(pending)
}
