use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};

use super::super::source::{OwnerSource, RequirementSource};
use super::super::status::{DerivationState, RequirementState};
use super::{PlannedTask, plan_task_requirement};
use crate::identity::CanonicalValue;

#[derive(Clone)]
pub(super) struct TaskLineage {
    pub(super) identity_key: CanonicalValue,
    pub(super) identity_digest: String,
    pub(super) revisions: Vec<Revision>,
    pub(super) requirement_ids: Vec<(i64, i64)>,
}

#[derive(Clone)]
pub(super) struct Revision {
    pub(super) identity_digest: String,
    pub(super) revision_digest: String,
    pub(super) design_sequence: Option<i64>,
    pub(super) requirement_digest: String,
    pub(super) gate_set_digest: Option<String>,
    pub(super) priority_digest: Option<String>,
    pub(super) requirement_ids: Vec<(i64, i64)>,
    pub(super) status: RequirementState,
    pub(super) source_task_ids: Vec<i64>,
}

#[derive(Clone)]
pub(super) struct Alias {
    pub(super) task_id: i64,
    pub(super) revision_digest: String,
    pub(super) duplicate: bool,
    pub(super) manual: bool,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum ConflictCode {
    MissingIdentity,
    SameSequence,
    CrossPackage,
}

#[derive(Clone)]
pub(super) struct Conflict {
    pub(super) code: ConflictCode,
    pub(super) task_ids: Vec<i64>,
    pub(super) requirement_ids: Vec<(i64, i64)>,
    pub(super) candidates: Vec<((i64, i64), String)>,
}

pub(super) struct LineagePlan {
    pub(super) identities: Vec<TaskLineage>,
    pub(super) aliases: Vec<Alias>,
    pub(super) conflicts: Vec<Conflict>,
}

struct Candidate<'a> {
    task_id: i64,
    source: &'a RequirementSource,
    planned: PlannedTask,
}

pub(super) fn analyze(project_id: i64, owner: &OwnerSource) -> Result<LineagePlan> {
    let mut candidates = Vec::new();
    let mut manuals = Vec::new();
    for task in &owner.tasks {
        if task.requirements.is_empty() {
            manuals.push((task, super::plan_task(project_id, owner, task)?));
            continue;
        }
        for source in &task.requirements {
            candidates.push(Candidate {
                task_id: task.task_id,
                source,
                planned: plan_task_requirement(project_id, owner, source)?,
            });
        }
    }

    let mut conflicts = Vec::new();
    let mut task_identities_by_source = BTreeMap::<i64, BTreeSet<String>>::new();
    for candidate in &candidates {
        task_identities_by_source
            .entry(candidate.task_id)
            .or_default()
            .insert(candidate.planned.identity_digest.clone());
    }
    for (task_id, identities) in &task_identities_by_source {
        if identities.len() > 1 {
            let involved = candidates
                .iter()
                .filter(|candidate| candidate.task_id == *task_id)
                .map(requirement_identity)
                .collect();
            conflicts.push(Conflict {
                code: ConflictCode::CrossPackage,
                task_ids: vec![*task_id],
                requirement_ids: involved,
                candidates: candidates
                    .iter()
                    .filter(|candidate| candidate.task_id == *task_id)
                    .map(|candidate| {
                        (
                            requirement_identity(candidate),
                            candidate.planned.revision_digest.clone(),
                        )
                    })
                    .collect(),
            });
        }
    }

    let mut grouped = BTreeMap::<String, Vec<&Candidate<'_>>>::new();
    for candidate in &candidates {
        grouped
            .entry(candidate.planned.identity_digest.clone())
            .or_default()
            .push(candidate);
    }

    let mut identities = Vec::new();
    for (identity_digest, mut group) in grouped {
        group.sort_by(candidate_order);
        let mut by_sequence = BTreeMap::<i64, Vec<&Candidate<'_>>>::new();
        for candidate in &group {
            let Some(sequence) = candidate.planned.design_sequence else {
                conflicts.push(Conflict {
                    code: ConflictCode::MissingIdentity,
                    task_ids: vec![candidate.task_id],
                    requirement_ids: vec![requirement_identity(candidate)],
                    candidates: Vec::new(),
                });
                continue;
            };
            by_sequence.entry(sequence).or_default().push(candidate);
        }

        let mut revisions = Vec::new();
        for sequence_group in by_sequence.values() {
            let distinct = sequence_group
                .iter()
                .map(|candidate| {
                    (
                        candidate.planned.revision_digest.as_str(),
                        candidate.planned.requirement_digest.as_str(),
                        candidate.planned.gate_set_digest.as_deref(),
                        candidate.planned.priority_digest.as_deref(),
                        candidate.source.design_version_id,
                        candidate.source.revision,
                    )
                })
                .collect::<BTreeSet<_>>();
            if distinct.len() > 1 {
                conflicts.push(Conflict {
                    code: ConflictCode::SameSequence,
                    task_ids: unique_sorted(
                        sequence_group.iter().map(|candidate| candidate.task_id),
                    ),
                    requirement_ids: unique_sorted(
                        sequence_group
                            .iter()
                            .map(|candidate| requirement_identity(candidate)),
                    ),
                    candidates: unique_sorted(sequence_group.iter().map(|candidate| {
                        (
                            requirement_identity(candidate),
                            candidate.planned.revision_digest.clone(),
                        )
                    })),
                });
                continue;
            }
            let first = sequence_group[0];
            let mut task_ids =
                unique_sorted(sequence_group.iter().map(|candidate| candidate.task_id));
            task_ids.sort_unstable();
            revisions.push(Revision {
                identity_digest: identity_digest.clone(),
                revision_digest: first.planned.revision_digest.clone(),
                design_sequence: first.planned.design_sequence,
                requirement_digest: first.planned.requirement_digest.clone(),
                gate_set_digest: first.planned.gate_set_digest.clone(),
                priority_digest: first.planned.priority_digest.clone(),
                requirement_ids: unique_sorted(
                    sequence_group
                        .iter()
                        .map(|candidate| requirement_identity(candidate)),
                ),
                status: combined_requirement_status(sequence_group)?,
                source_task_ids: task_ids,
            });
        }
        revisions.sort_by(|left, right| {
            left.design_sequence
                .cmp(&right.design_sequence)
                .then(left.revision_digest.cmp(&right.revision_digest))
        });
        let mut requirement_ids = unique_sorted(
            group
                .iter()
                .map(|candidate| requirement_identity(candidate)),
        );
        requirement_ids.sort_unstable();
        identities.push(TaskLineage {
            identity_key: group[0].planned.identity_key.clone(),
            identity_digest,
            revisions,
            requirement_ids,
        });
    }

    let mut aliases = Vec::new();
    for task in &owner.tasks {
        if let Some((_, planned)) = manuals
            .iter()
            .find(|(source, _)| source.task_id == task.task_id)
        {
            aliases.push(Alias {
                task_id: task.task_id,
                revision_digest: planned.revision_digest.clone(),
                duplicate: false,
                manual: true,
            });
            identities.push(TaskLineage {
                identity_key: planned.identity_key.clone(),
                identity_digest: planned.identity_digest.clone(),
                revisions: vec![Revision {
                    identity_digest: planned.identity_digest.clone(),
                    revision_digest: planned.revision_digest.clone(),
                    design_sequence: None,
                    requirement_digest: planned.requirement_digest.clone(),
                    gate_set_digest: None,
                    priority_digest: None,
                    requirement_ids: Vec::new(),
                    status: RequirementState::Active,
                    source_task_ids: vec![task.task_id],
                }],
                requirement_ids: Vec::new(),
            });
            continue;
        }
        if task_identities_by_source
            .get(&task.task_id)
            .is_some_and(|values| values.len() != 1)
        {
            continue;
        }
        let mut linked = candidates
            .iter()
            .filter(|candidate| candidate.task_id == task.task_id)
            .collect::<Vec<_>>();
        linked.sort_by(candidate_order);
        let selected = linked
            .iter()
            .rev()
            .find(|candidate| {
                candidate.source.derivation_status != DerivationState::Stale
                    && candidate.source.status == RequirementState::Active
            })
            .or_else(|| linked.last())
            .copied();
        if let Some(selected) = selected {
            aliases.push(Alias {
                task_id: task.task_id,
                revision_digest: selected.planned.revision_digest.clone(),
                duplicate: false,
                manual: false,
            });
        }
    }
    let mut counts = BTreeMap::<String, usize>::new();
    for alias in &aliases {
        *counts.entry(alias.revision_digest.clone()).or_default() += 1;
    }
    for alias in &mut aliases {
        alias.duplicate = counts[&alias.revision_digest] > 1;
    }
    identities.sort_by(|left, right| left.identity_digest.cmp(&right.identity_digest));
    aliases.sort_by_key(|alias| alias.task_id);
    conflicts.sort_by(|left, right| {
        left.code
            .cmp(&right.code)
            .then(left.task_ids.cmp(&right.task_ids))
            .then(left.requirement_ids.cmp(&right.requirement_ids))
            .then(left.candidates.cmp(&right.candidates))
    });
    conflicts.dedup_by(|left, right| {
        left.code == right.code
            && left.task_ids == right.task_ids
            && left.requirement_ids == right.requirement_ids
            && left.candidates == right.candidates
    });
    Ok(LineagePlan {
        identities,
        aliases,
        conflicts,
    })
}

fn candidate_order(left: &&Candidate<'_>, right: &&Candidate<'_>) -> std::cmp::Ordering {
    left.planned
        .design_sequence
        .cmp(&right.planned.design_sequence)
        .then(left.source.revision.cmp(&right.source.revision))
        .then(
            left.planned
                .revision_digest
                .cmp(&right.planned.revision_digest),
        )
        .then(left.source.requirement_id.cmp(&right.source.requirement_id))
}

fn requirement_identity(candidate: &Candidate<'_>) -> (i64, i64) {
    (
        candidate.source.design_version_id,
        candidate.source.requirement_id,
    )
}

fn combined_requirement_status(group: &[&Candidate<'_>]) -> Result<RequirementState> {
    let statuses = group
        .iter()
        .map(|candidate| candidate.source.status)
        .collect::<BTreeSet<_>>();
    if statuses.len() != 1 {
        bail!("same_sequence_conflict: byte-identical requirements disagree on status");
    }
    Ok(*statuses.iter().next().expect("nonempty sequence group"))
}

fn unique_sorted<T: Ord>(values: impl Iterator<Item = T>) -> Vec<T> {
    values.collect::<BTreeSet<_>>().into_iter().collect()
}
