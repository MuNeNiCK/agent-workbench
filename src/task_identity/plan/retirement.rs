use anyhow::Result;

use crate::identity::signed_source_id;

use super::super::status::RequirementState;
use super::lineage::LineagePlan;
use super::model::{
    ArrayKind, Classification, Element, Fingerprints, Payload, PlanStatus, Reason, SourceRef,
    Target, TargetKind,
};

pub(super) fn add(lineage: &LineagePlan, elements: &mut Vec<Element>) -> Result<()> {
    for identity in &lineage.identities {
        for revision in &identity.revisions {
            if revision.status == RequirementState::Active {
                continue;
            }
            let Some(sequence) = revision.design_sequence else {
                continue;
            };
            let Some(retired_sequence) = identity
                .revisions
                .iter()
                .filter_map(|candidate| candidate.design_sequence)
                .filter(|candidate| *candidate > sequence)
                .min()
            else {
                continue;
            };
            let source_refs = revision
                .requirement_ids
                .iter()
                .map(|(version, requirement)| SourceRef::requirement(*version, *requirement))
                .collect::<Result<Vec<_>>>()?;
            let payload = Payload::Retirement {
                revision_digest: revision.revision_digest.clone(),
                retired_sequence: signed_source_id(retired_sequence)?,
            };
            elements.push(Element {
                array: ArrayKind::Retirements,
                source_refs,
                target: Some(Target {
                    kind: TargetKind::Revision,
                    digest: revision.revision_digest.clone(),
                }),
                sort_digest: revision.revision_digest.clone(),
                classification: Classification::Retired,
                reason: Reason::RemovedRevision,
                before: Fingerprints {
                    requirement: Some(revision.requirement_digest.clone()),
                    gates: revision.gate_set_digest.clone(),
                    priority: revision.priority_digest.clone(),
                    status: Some(match revision.status {
                        RequirementState::Active => PlanStatus::Active,
                        RequirementState::Superseded => PlanStatus::Stale,
                        RequirementState::OutOfScope => PlanStatus::OutOfScope,
                    }),
                },
                after: Fingerprints {
                    status: Some(PlanStatus::Retired),
                    ..Fingerprints::default()
                },
                payload,
            });
        }
    }
    Ok(())
}
