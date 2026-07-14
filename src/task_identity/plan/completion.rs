use std::collections::BTreeMap;

use anyhow::Result;

use crate::identity::domain_digest;

use super::OwnerSource;
use super::lineage::{LineagePlan, Revision};
use super::model::{
    ArrayKind, Claim, Classification, CompletionResult, Element, Fingerprints, Payload, PlanStatus,
    Reason, RefFamily, SourceRef, Target, TargetKind,
};

pub(super) fn add(
    owner: &OwnerSource,
    lineage: &LineagePlan,
    elements: &mut Vec<Element>,
) -> Result<()> {
    let tasks = owner
        .tasks
        .iter()
        .map(|task| (task.task_id, task))
        .collect::<BTreeMap<_, _>>();
    for identity in &lineage.identities {
        for revision in &identity.revisions {
            let mut refs = if revision.requirement_ids.is_empty() {
                Vec::new()
            } else {
                revision
                    .requirement_ids
                    .iter()
                    .map(|(version, requirement)| SourceRef::requirement(*version, *requirement))
                    .collect::<Result<Vec<_>>>()?
            };
            let mut implementation_evidence = Vec::new();
            let mut validation_evidence = Vec::new();
            let mut structural = false;
            for task_id in &revision.source_task_ids {
                let Some(task) = tasks.get(task_id).copied() else {
                    continue;
                };
                refs.push(SourceRef::one(RefFamily::Task, "task_id", *task_id)?);
                structural |= task.status == super::super::status::TaskState::Completed
                    && !task.checklists.is_empty()
                    && task.checklists.iter().all(|checklist| {
                        checklist.status == super::super::status::ChecklistState::Closed
                            && !checklist.items.is_empty()
                            && checklist.items.iter().all(|item| {
                                item.status == super::super::status::ChecklistItemState::Closed
                                    || (item.status
                                        == super::super::status::ChecklistItemState::OutOfScope
                                        && item.acceptance_ids.len() == 1)
                            })
                    });
                for checklist in &task.checklists {
                    refs.push(SourceRef::one(
                        RefFamily::Checklist,
                        "checklist_id",
                        checklist.checklist_id,
                    )?);
                    for item in &checklist.items {
                        refs.push(SourceRef::one(
                            RefFamily::ChecklistItem,
                            "checklist_item_id",
                            item.item_id,
                        )?);
                        for acceptance in &item.acceptance_ids {
                            refs.push(SourceRef::one(
                                RefFamily::Acceptance,
                                "acceptance_id",
                                *acceptance,
                            )?);
                        }
                    }
                }
                for evidence in &task.evidence {
                    let (family, field) = match evidence.kind.as_str() {
                        "implementation" => (RefFamily::Evidence, "evidence_id"),
                        "coverage" => (RefFamily::Coverage, "coverage_item_id"),
                        "validation" => (RefFamily::ValidationRun, "validation_run_id"),
                        _ => continue,
                    };
                    refs.push(SourceRef::one(family, field, evidence.id)?);
                    match evidence.kind.as_str() {
                        "implementation" | "coverage" => {
                            implementation_evidence.push(evidence.digest.clone())
                        }
                        "validation" => validation_evidence.push(evidence.digest.clone()),
                        _ => {}
                    }
                }
            }
            refs.sort();
            refs.dedup();
            implementation_evidence.sort();
            implementation_evidence.dedup();
            validation_evidence.sort();
            validation_evidence.dedup();
            let implementation_carry = structural && !implementation_evidence.is_empty();
            push(
                elements,
                refs.clone(),
                revision,
                Claim::Implementation,
                implementation_carry,
                if implementation_carry {
                    implementation_evidence
                } else {
                    Vec::new()
                },
            );
            if !revision.requirement_ids.is_empty() {
                let validation_carry = !validation_evidence.is_empty();
                push(
                    elements,
                    refs,
                    revision,
                    Claim::Validation,
                    validation_carry,
                    if validation_carry {
                        validation_evidence
                    } else {
                        Vec::new()
                    },
                );
            }
        }
    }
    Ok(())
}

fn push(
    elements: &mut Vec<Element>,
    refs: Vec<SourceRef>,
    revision: &Revision,
    claim: Claim,
    carry: bool,
    evidence: Vec<String>,
) {
    let result = if carry {
        CompletionResult::Carry
    } else {
        CompletionResult::Reopen
    };
    let payload = Payload::Completion {
        revision_digest: revision.revision_digest.clone(),
        phase: None,
        claim,
        result,
        evidence,
    };
    let digest = domain_digest(b"AWB-COMPLETION-v1\0", &payload.value());
    let status = if carry {
        match claim {
            Claim::Implementation => PlanStatus::Completed,
            Claim::Validation => PlanStatus::Passed,
        }
    } else {
        PlanStatus::Open
    };
    elements.push(Element {
        array: ArrayKind::CompletionClaims,
        source_refs: refs,
        target: Some(Target {
            kind: TargetKind::Completion,
            digest: digest.clone(),
        }),
        sort_digest: digest,
        classification: Classification::Mapped,
        reason: if carry {
            Reason::CompletionCarried
        } else {
            Reason::ObligationReopened
        },
        before: Fingerprints {
            status: Some(status),
            ..Fingerprints::default()
        },
        after: Fingerprints {
            status: Some(status),
            ..Fingerprints::default()
        },
        payload,
    });
}
