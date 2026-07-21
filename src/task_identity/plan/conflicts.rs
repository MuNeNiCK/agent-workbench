use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;

use super::super::source::{OwnerSource, TaskSource};
use super::super::status::{ChecklistItemState, ChecklistState, PhaseState, TaskState};

struct MembershipRecord {
    phase_id: i64,
    task_id: i64,
    identity_digest: String,
    revision_digest: String,
    state: PhaseState,
}
use super::lineage::LineagePlan;
use super::model::{AmbiguityCode, Claim, Element, RefFamily, SourceRef};

pub(super) fn add(
    owner: &OwnerSource,
    lineage: &LineagePlan,
    elements: &mut Vec<Element>,
) -> Result<BTreeSet<(i64, String)>> {
    add_owner_conflict(owner, elements)?;
    add_terminal_conflicts(owner, lineage, elements)?;
    add_completion_conflicts(owner, lineage, elements)?;
    add_membership_conflicts(owner, lineage, elements)
}

fn add_membership_conflicts(
    owner: &OwnerSource,
    lineage: &LineagePlan,
    elements: &mut Vec<Element>,
) -> Result<BTreeSet<(i64, String)>> {
    let revisions = lineage
        .identities
        .iter()
        .flat_map(|identity| &identity.revisions)
        .map(|revision| (revision.revision_digest.as_str(), revision))
        .collect::<BTreeMap<_, _>>();
    let aliases = lineage
        .aliases
        .iter()
        .map(|alias| (alias.task_id, alias.revision_digest.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut records = Vec::new();
    for task in &owner.tasks {
        let Some(revision) = aliases
            .get(&task.task_id)
            .and_then(|digest| revisions.get(digest).copied())
        else {
            continue;
        };
        for membership in &task.memberships {
            records.push(MembershipRecord {
                phase_id: membership.phase_id,
                task_id: task.task_id,
                identity_digest: revision.identity_digest.clone(),
                revision_digest: revision.revision_digest.clone(),
                state: membership.phase_status,
            });
        }
    }

    let mut conflicts = BTreeSet::new();
    let mut groups = BTreeMap::<(i64, String), Vec<&MembershipRecord>>::new();
    for record in &records {
        groups
            .entry((record.phase_id, record.identity_digest.clone()))
            .or_default()
            .push(record);
    }
    for (key, group) in &groups {
        if group
            .iter()
            .map(|record| record.state)
            .collect::<BTreeSet<_>>()
            .len()
            > 1
        {
            conflicts.insert(key.clone());
            elements.push(super::assembly::ambiguity_element(
                membership_refs(group)?,
                AmbiguityCode::MembershipState,
                Vec::new(),
            ));
        }
    }

    let mut by_identity = BTreeMap::<String, Vec<&MembershipRecord>>::new();
    for record in &records {
        by_identity
            .entry(record.identity_digest.clone())
            .or_default()
            .push(record);
    }
    for group in by_identity.values() {
        let open_phases = group
            .iter()
            .filter(|record| record.state.is_live())
            .map(|record| record.phase_id)
            .collect::<BTreeSet<_>>();
        if open_phases.len() > 1 {
            let involved = group
                .iter()
                .filter(|record| record.state.is_live())
                .copied()
                .collect::<Vec<_>>();
            conflicts.extend(
                involved
                    .iter()
                    .map(|record| (record.phase_id, record.identity_digest.clone())),
            );
            elements.push(super::assembly::ambiguity_element(
                membership_refs(&involved)?,
                AmbiguityCode::OpenOpenPhase,
                Vec::new(),
            ));
        }
        let revisions = group
            .iter()
            .map(|record| record.revision_digest.as_str())
            .collect::<BTreeSet<_>>();
        for revision in revisions {
            let involved = group
                .iter()
                .filter(|record| record.revision_digest == revision)
                .copied()
                .collect::<Vec<_>>();
            let states = involved
                .iter()
                .map(|record| record.state)
                .collect::<BTreeSet<_>>();
            if states.iter().any(|state| state.is_live()) && states.contains(&PhaseState::Closed) {
                conflicts.extend(
                    involved
                        .iter()
                        .map(|record| (record.phase_id, record.identity_digest.clone())),
                );
                elements.push(super::assembly::ambiguity_element(
                    membership_refs(&involved)?,
                    AmbiguityCode::OpenClosedCompatible,
                    Vec::new(),
                ));
            }
        }
    }
    Ok(conflicts)
}

fn membership_refs(records: &[&MembershipRecord]) -> Result<Vec<SourceRef>> {
    let mut refs = Vec::new();
    for record in records {
        refs.push(SourceRef::one(
            RefFamily::Phase,
            "phase_id",
            record.phase_id,
        )?);
        refs.push(SourceRef::one(RefFamily::Task, "task_id", record.task_id)?);
        refs.push(SourceRef::membership(record.phase_id, record.task_id)?);
    }
    refs.sort();
    refs.dedup();
    Ok(refs)
}

fn add_owner_conflict(owner: &OwnerSource, elements: &mut Vec<Element>) -> Result<()> {
    if !owner.owner_conflict {
        return Ok(());
    }
    if owner.tasks.is_empty() {
        return Ok(());
    }
    let refs = owner
        .tasks
        .iter()
        .map(|task| SourceRef::one(RefFamily::Task, "task_id", task.task_id))
        .collect::<Result<Vec<_>>>()?;
    elements.push(super::assembly::ambiguity_element(
        refs,
        AmbiguityCode::CrossOwner,
        vec![Claim::Implementation, Claim::Validation],
    ));
    Ok(())
}

fn add_terminal_conflicts(
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
            let terminal = revision
                .source_task_ids
                .iter()
                .filter_map(|task_id| tasks.get(task_id))
                .map(|task| task.status)
                .filter(is_terminal)
                .collect::<BTreeSet<_>>();
            if terminal.len() < 2 {
                continue;
            }
            let mut refs = revision
                .source_task_ids
                .iter()
                .map(|task_id| SourceRef::one(RefFamily::Task, "task_id", *task_id))
                .collect::<Result<Vec<_>>>()?;
            refs.extend(
                revision
                    .requirement_ids
                    .iter()
                    .map(|(version, requirement)| SourceRef::requirement(*version, *requirement))
                    .collect::<Result<Vec<_>>>()?,
            );
            elements.push(super::assembly::ambiguity_element(
                refs,
                AmbiguityCode::TerminalDisposition,
                vec![Claim::Implementation],
            ));
        }
    }
    Ok(())
}

fn add_completion_conflicts(
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
            if revision.requirement_ids.is_empty() {
                continue;
            }
            let completed = revision
                .source_task_ids
                .iter()
                .filter_map(|task_id| tasks.get(task_id).copied())
                .filter(|task| task.status == TaskState::Completed)
                .collect::<Vec<_>>();
            if completed.is_empty() || completed.iter().any(|task| structurally_complete(task)) {
                continue;
            }
            let mut refs = revision
                .requirement_ids
                .iter()
                .map(|(version, requirement)| SourceRef::requirement(*version, *requirement))
                .collect::<Result<Vec<_>>>()?;
            for task in completed {
                refs.push(SourceRef::one(RefFamily::Task, "task_id", task.task_id)?);
                append_completion_refs(task, &mut refs)?;
            }
            elements.push(super::assembly::ambiguity_element(
                refs,
                AmbiguityCode::CompletionInvalid,
                vec![Claim::Implementation],
            ));
        }
    }
    Ok(())
}

fn append_completion_refs(task: &TaskSource, refs: &mut Vec<SourceRef>) -> Result<()> {
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
    Ok(())
}

fn structurally_complete(task: &TaskSource) -> bool {
    if task.status != TaskState::Completed {
        return false;
    }
    !task.checklists.is_empty()
        && task.checklists.iter().all(|checklist| {
            checklist.status == ChecklistState::Closed
                && !checklist.items.is_empty()
                && checklist.items.iter().all(|item| {
                    item.status == ChecklistItemState::Closed
                        || (item.status == ChecklistItemState::OutOfScope
                            && item.acceptance_ids.len() == 1)
                })
        })
}

fn is_terminal(status: &TaskState) -> bool {
    matches!(status, TaskState::Completed | TaskState::OutOfScope)
}
