use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};

use crate::identity::{CanonicalValue, domain_digest, signed_source_id};

use super::lineage::{ConflictCode, LineagePlan};
use super::model::{
    AmbiguityCode, ArrayKind, Claim, Classification, Element, Fingerprints, Payload, Plan,
    PlanStatus, Reason, RefFamily, Resolution, SourceRef, Target, TargetKind,
};
use super::{OwnerSource, SourceSnapshot};

struct DependencyEdgeGroup {
    states: BTreeMap<PlanStatus, Vec<i64>>,
    task_ids: BTreeSet<i64>,
}

pub(crate) fn build(snapshot: &SourceSnapshot, owner: &OwnerSource) -> Result<Plan> {
    let lineage = super::lineage::analyze(snapshot.project_id, owner)?;
    let mut elements = Vec::new();
    add_lineage(owner, &lineage, &mut elements)?;
    super::retirement::add(&lineage, &mut elements)?;
    let membership_conflicts = super::conflicts::add(owner, &lineage, &mut elements)?;
    add_memberships(owner, &lineage, &membership_conflicts, &mut elements)?;
    add_dependencies(owner, &lineage, &mut elements)?;
    super::completion::add(owner, &lineage, &mut elements)?;
    add_conflicts(&lineage, &mut elements)?;
    let mut plan = Plan {
        mode: "base",
        base_plan_sha256: None,
        recovery_sha256: None,
        owner_digest: owner.owner_digest.clone(),
        component_sha256: owner.component_digest.clone(),
        source_schema: snapshot.schema_version,
        source_sha256: owner.source_digest.clone(),
        elements,
    };
    plan.validate()?;
    Ok(plan)
}

fn add_lineage(
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
        let manual = identity.requirement_ids.is_empty();
        let refs = if manual {
            vec![SourceRef::one(
                RefFamily::Task,
                "task_id",
                identity.revisions[0].source_task_ids[0],
            )?]
        } else {
            identity
                .requirement_ids
                .iter()
                .map(|(version, requirement)| SourceRef::requirement(*version, *requirement))
                .collect::<Result<Vec<_>>>()?
        };
        let current = identity
            .revisions
            .iter()
            .filter(|revision| revision.status == super::super::status::RequirementState::Active)
            .max_by(|left, right| {
                left.design_sequence
                    .cmp(&right.design_sequence)
                    .then(left.revision_digest.cmp(&right.revision_digest))
            })
            .or_else(|| identity.revisions.last())
            .context("task identity lineage has no revision")?;
        let current_status = revision_task_status(current, &tasks);
        elements.push(Element {
            array: ArrayKind::TaskIdentities,
            source_refs: refs,
            target: Some(Target {
                kind: TargetKind::TaskIdentity,
                digest: identity.identity_digest.clone(),
            }),
            sort_digest: identity.identity_digest.clone(),
            classification: Classification::Mapped,
            reason: if manual {
                Reason::Manual
            } else {
                Reason::Unique
            },
            before: Fingerprints::default(),
            after: fingerprints(current, current_status),
            payload: Payload::TaskIdentity {
                identity_key: identity.identity_key.clone(),
            },
        });

        for (index, revision) in identity.revisions.iter().enumerate() {
            let source_refs = if revision.requirement_ids.is_empty() {
                vec![SourceRef::one(
                    RefFamily::Task,
                    "task_id",
                    revision.source_task_ids[0],
                )?]
            } else {
                revision
                    .requirement_ids
                    .iter()
                    .map(|(version, requirement)| SourceRef::requirement(*version, *requirement))
                    .collect::<Result<Vec<_>>>()?
            };
            let status = revision_task_status(revision, &tasks);
            elements.push(Element {
                array: ArrayKind::Revisions,
                source_refs,
                target: Some(Target {
                    kind: TargetKind::Revision,
                    digest: revision.revision_digest.clone(),
                }),
                sort_digest: revision.revision_digest.clone(),
                classification: Classification::Mapped,
                reason: if manual {
                    Reason::Manual
                } else if index == 0 {
                    Reason::Unique
                } else {
                    Reason::ChangedRevision
                },
                before: index
                    .checked_sub(1)
                    .map(|previous| fingerprints(&identity.revisions[previous], status))
                    .unwrap_or_default(),
                after: fingerprints(revision, status),
                payload: Payload::Revision {
                    identity_digest: identity.identity_digest.clone(),
                    design_sequence: revision.design_sequence.map(signed_source_id).transpose()?,
                },
            });
        }
    }

    let revisions = revision_index(lineage);
    for alias in &lineage.aliases {
        let revision = revisions
            .get(alias.revision_digest.as_str())
            .context("alias revision is absent from lineage")?;
        let source = tasks.get(&alias.task_id).context("alias task is absent")?;
        elements.push(Element {
            array: ArrayKind::Aliases,
            source_refs: vec![SourceRef::one(RefFamily::Task, "task_id", alias.task_id)?],
            target: Some(Target {
                kind: TargetKind::Revision,
                digest: alias.revision_digest.clone(),
            }),
            sort_digest: alias.revision_digest.clone(),
            classification: Classification::Mapped,
            reason: if alias.manual {
                Reason::Manual
            } else if alias.duplicate {
                Reason::DuplicateAlias
            } else {
                Reason::Unique
            },
            before: fingerprints(revision, map_task_status(source.status)),
            after: fingerprints(revision, revision_task_status(revision, &tasks)),
            payload: Payload::Alias {
                historical_task: signed_source_id(alias.task_id)?,
                revision_digest: alias.revision_digest.clone(),
            },
        });
    }
    Ok(())
}

fn add_memberships(
    owner: &OwnerSource,
    lineage: &LineagePlan,
    conflicts: &BTreeSet<(i64, String)>,
    elements: &mut Vec<Element>,
) -> Result<()> {
    struct Group {
        phase_id: i64,
        identity_digest: String,
        state: PlanStatus,
        revision_digest: String,
        task_ids: BTreeSet<i64>,
    }
    let aliases = lineage
        .aliases
        .iter()
        .map(|alias| (alias.task_id, alias.revision_digest.as_str()))
        .collect::<BTreeMap<_, _>>();
    let revisions = revision_index(lineage);
    let mut groups = BTreeMap::<(i64, String), Group>::new();
    for task in &owner.tasks {
        let Some(revision_digest) = aliases.get(&task.task_id) else {
            continue;
        };
        let revision = revisions[revision_digest];
        for membership in &task.memberships {
            let key = (membership.phase_id, revision.identity_digest.clone());
            if conflicts.contains(&key) {
                continue;
            }
            let state = map_phase_status(membership.phase_status);
            let group = groups.entry(key).or_insert_with(|| Group {
                phase_id: membership.phase_id,
                identity_digest: revision.identity_digest.clone(),
                state,
                revision_digest: revision.revision_digest.clone(),
                task_ids: BTreeSet::new(),
            });
            if group.state != state
                || (state == PlanStatus::Closed
                    && group.revision_digest != revision.revision_digest)
            {
                continue;
            }
            group.task_ids.insert(task.task_id);
        }
    }
    for group in groups.into_values() {
        let payload = Payload::Membership {
            phase: signed_source_id(group.phase_id)?,
            identity_digest: group.identity_digest.clone(),
            state: group.state,
        };
        let digest = domain_digest(b"AWB-MEMBERSHIP-v1\0", &payload.value());
        let mut refs = vec![SourceRef::one(
            RefFamily::Phase,
            "phase_id",
            group.phase_id,
        )?];
        for task_id in &group.task_ids {
            refs.push(SourceRef::one(RefFamily::Task, "task_id", *task_id)?);
            refs.push(SourceRef::membership(group.phase_id, *task_id)?);
        }
        elements.push(Element {
            array: ArrayKind::Memberships,
            source_refs: refs,
            target: Some(Target {
                kind: TargetKind::Membership,
                digest: digest.clone(),
            }),
            sort_digest: digest,
            classification: Classification::Mapped,
            reason: if group.task_ids.len() > 1 {
                Reason::PhaseDeduplicated
            } else {
                Reason::Unique
            },
            before: Fingerprints {
                status: Some(group.state),
                ..Fingerprints::default()
            },
            after: Fingerprints {
                status: Some(group.state),
                ..Fingerprints::default()
            },
            payload,
        });
    }
    Ok(())
}

fn add_dependencies(
    owner: &OwnerSource,
    lineage: &LineagePlan,
    elements: &mut Vec<Element>,
) -> Result<()> {
    let aliases = lineage
        .aliases
        .iter()
        .map(|alias| (alias.task_id, alias.revision_digest.as_str()))
        .collect::<BTreeMap<_, _>>();
    let revisions = revision_index(lineage);
    let mut phase_identities = BTreeMap::<i64, BTreeSet<String>>::new();
    let mut phase_tasks = BTreeMap::<i64, BTreeSet<i64>>::new();
    for task in &owner.tasks {
        let Some(revision) = aliases
            .get(&task.task_id)
            .and_then(|digest| revisions.get(digest).copied())
        else {
            continue;
        };
        for membership in &task.memberships {
            phase_identities
                .entry(membership.phase_id)
                .or_default()
                .insert(revision.identity_digest.clone());
            phase_tasks
                .entry(membership.phase_id)
                .or_default()
                .insert(task.task_id);
        }
    }
    let mut groups = BTreeMap::<(String, String), DependencyEdgeGroup>::new();
    for dependency in &owner.dependencies {
        let from = phase_identities
            .get(&dependency.from_phase_id)
            .cloned()
            .unwrap_or_default();
        let to = phase_identities
            .get(&dependency.to_phase_id)
            .cloned()
            .unwrap_or_default();
        for from_task in &from {
            for to_task in &to {
                let group = groups
                    .entry((from_task.clone(), to_task.clone()))
                    .or_insert_with(|| DependencyEdgeGroup {
                        states: BTreeMap::new(),
                        task_ids: BTreeSet::new(),
                    });
                group
                    .states
                    .entry(map_dependency_status(dependency.status))
                    .or_default()
                    .push(dependency.dependency_id);
                group.task_ids.extend(
                    phase_tasks
                        .get(&dependency.from_phase_id)
                        .into_iter()
                        .flatten()
                        .chain(
                            phase_tasks
                                .get(&dependency.to_phase_id)
                                .into_iter()
                                .flatten(),
                        )
                        .copied(),
                );
            }
        }
    }
    let mut conflicting = BTreeSet::new();
    for ((from, to), group) in &groups {
        if from == to {
            conflicting.insert((from.clone(), to.clone()));
            elements.push(dependency_ambiguity(group, AmbiguityCode::DependencySelf)?);
            continue;
        }
        if group.states.len() > 1 {
            conflicting.insert((from.clone(), to.clone()));
            elements.push(dependency_ambiguity(group, AmbiguityCode::DependencyState)?);
        }
        let reverse = (to.clone(), from.clone());
        if (from, to) < (&reverse.0, &reverse.1)
            && let Some(reverse_group) = groups.get(&reverse)
        {
            conflicting.insert((from.clone(), to.clone()));
            conflicting.insert(reverse);
            let mut refs = dependency_refs(group)?;
            refs.extend(dependency_refs(reverse_group)?);
            refs.sort();
            refs.dedup();
            elements.push(ambiguity_element(
                refs,
                AmbiguityCode::DependencyReverse,
                Vec::new(),
            ));
        }
    }
    for ((from, to), group) in groups {
        if conflicting.contains(&(from.clone(), to.clone())) {
            continue;
        }
        let source_refs = dependency_refs(&group)?;
        let (state, _dependency_ids) = group
            .states
            .into_iter()
            .next()
            .expect("readable dependency group has one state");
        let payload = Payload::Dependency {
            from_task: from,
            to_task: to,
            state,
        };
        let digest = domain_digest(b"AWB-DEPENDENCY-v1\0", &payload.value());
        elements.push(Element {
            array: ArrayKind::Dependencies,
            source_refs,
            target: Some(Target {
                kind: TargetKind::Dependency,
                digest: digest.clone(),
            }),
            sort_digest: digest,
            classification: Classification::Mapped,
            reason: Reason::Unique,
            before: Fingerprints {
                status: Some(state),
                ..Fingerprints::default()
            },
            after: Fingerprints {
                status: Some(state),
                ..Fingerprints::default()
            },
            payload,
        });
    }
    Ok(())
}

fn dependency_refs(group: &DependencyEdgeGroup) -> Result<Vec<SourceRef>> {
    let mut refs = group
        .states
        .values()
        .flatten()
        .copied()
        .map(|id| SourceRef::one(RefFamily::Dependency, "dependency_id", id))
        .collect::<Result<Vec<_>>>()?;
    refs.extend(
        group
            .task_ids
            .iter()
            .copied()
            .map(|id| SourceRef::one(RefFamily::Task, "task_id", id))
            .collect::<Result<Vec<_>>>()?,
    );
    Ok(refs)
}

fn dependency_ambiguity(group: &DependencyEdgeGroup, code: AmbiguityCode) -> Result<Element> {
    Ok(ambiguity_element(dependency_refs(group)?, code, Vec::new()))
}

pub(super) fn ambiguity_element(
    refs: Vec<SourceRef>,
    code: AmbiguityCode,
    claims: Vec<Claim>,
) -> Element {
    ambiguity_element_with_candidates(refs, code, claims, Vec::new())
}

fn ambiguity_element_with_candidates(
    mut refs: Vec<SourceRef>,
    code: AmbiguityCode,
    claims: Vec<Claim>,
    mut candidates: Vec<Target>,
) -> Element {
    refs.sort();
    refs.dedup();
    candidates.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then(left.digest.cmp(&right.digest))
    });
    candidates.dedup();
    let identity = CanonicalValue::object([
        (
            "source_refs",
            CanonicalValue::Array(refs.iter().map(SourceRef::value).collect()),
        ),
        ("code", CanonicalValue::string(code.as_str())),
        (
            "claims",
            CanonicalValue::Array(
                claims
                    .iter()
                    .map(|claim| CanonicalValue::string(claim.as_str()))
                    .collect(),
            ),
        ),
        (
            "candidates",
            CanonicalValue::Array(candidates.iter().map(Target::value).collect()),
        ),
    ]);
    let digest = domain_digest(b"AWB-AMBIGUITY-v1\0", &identity);
    Element {
        array: ArrayKind::Ambiguities,
        source_refs: refs,
        target: None,
        sort_digest: digest,
        classification: Classification::Blocked,
        reason: Reason::Ambiguous,
        before: Fingerprints::default(),
        after: Fingerprints::default(),
        payload: Payload::Ambiguity {
            code,
            claims,
            candidates,
            resolutions: Vec::new(),
        },
    }
}

fn add_conflicts(lineage: &LineagePlan, elements: &mut Vec<Element>) -> Result<()> {
    for conflict in &lineage.conflicts {
        let mut refs = conflict
            .task_ids
            .iter()
            .map(|id| SourceRef::one(RefFamily::Task, "task_id", *id))
            .collect::<Result<Vec<_>>>()?;
        refs.extend(
            conflict
                .requirement_ids
                .iter()
                .map(|(version, requirement)| SourceRef::requirement(*version, *requirement))
                .collect::<Result<Vec<_>>>()?,
        );
        refs.sort();
        refs.dedup();
        let code = match conflict.code {
            ConflictCode::MissingIdentity => AmbiguityCode::MissingIdentity,
            ConflictCode::SameSequence => AmbiguityCode::SameSequenceConflict,
            ConflictCode::CrossPackage => AmbiguityCode::CrossPackage,
        };
        let claims = vec![Claim::Implementation, Claim::Validation];
        let candidates = conflict
            .candidates
            .iter()
            .map(|(_, digest)| Target {
                kind: TargetKind::Revision,
                digest: digest.clone(),
            })
            .collect();
        let mut element = ambiguity_element_with_candidates(refs, code, claims, candidates);
        let ambiguity_digest = element.sort_digest.clone();
        let all_requirement_refs = conflict
            .requirement_ids
            .iter()
            .map(|(version, requirement)| SourceRef::requirement(*version, *requirement))
            .collect::<Result<Vec<_>>>()?;
        let mut resolutions = Vec::new();
        for ((version, requirement), _) in &conflict.candidates {
            let selected = vec![SourceRef::requirement(*version, *requirement)?];
            let retired = all_requirement_refs
                .iter()
                .filter(|source| source.requirement_id() != Some(*requirement))
                .cloned()
                .collect::<Vec<_>>();
            let value = CanonicalValue::object([
                (
                    "ambiguity_digest",
                    CanonicalValue::string(ambiguity_digest.clone()),
                ),
                (
                    "selected_source_refs",
                    CanonicalValue::Array(selected.iter().map(SourceRef::value).collect()),
                ),
                (
                    "retired_source_refs",
                    CanonicalValue::Array(retired.iter().map(SourceRef::value).collect()),
                ),
                ("remove", CanonicalValue::Array(Vec::new())),
                ("add", CanonicalValue::Array(Vec::new())),
            ]);
            resolutions.push(Resolution {
                digest: domain_digest(b"AWB-RESOLUTION-v1\0", &value),
                selected_source_refs: selected,
                retired_source_refs: retired,
                remove: Vec::new(),
                add: Vec::new(),
            });
        }
        if let Payload::Ambiguity {
            resolutions: target,
            ..
        } = &mut element.payload
        {
            *target = resolutions;
        }
        elements.push(element);
    }
    Ok(())
}

fn revision_index(lineage: &LineagePlan) -> BTreeMap<&str, &super::lineage::Revision> {
    lineage
        .identities
        .iter()
        .flat_map(|identity| &identity.revisions)
        .map(|revision| (revision.revision_digest.as_str(), revision))
        .collect()
}

fn revision_task_status(
    revision: &super::lineage::Revision,
    tasks: &BTreeMap<i64, &super::super::source::TaskSource>,
) -> PlanStatus {
    canonical_task_status(
        &revision
            .source_task_ids
            .iter()
            .filter_map(|task_id| tasks.get(task_id).copied())
            .map(|task| task.status)
            .collect::<Vec<_>>(),
    )
}

fn fingerprints(revision: &super::lineage::Revision, status: PlanStatus) -> Fingerprints {
    Fingerprints {
        requirement: Some(revision.requirement_digest.clone()),
        gates: revision.gate_set_digest.clone(),
        priority: revision.priority_digest.clone(),
        status: Some(status),
    }
}

fn canonical_task_status(statuses: &[super::super::status::TaskState]) -> PlanStatus {
    use super::super::status::TaskState;
    for status in [
        TaskState::Completed,
        TaskState::OutOfScope,
        TaskState::Blocked,
        TaskState::Open,
    ] {
        if statuses.contains(&status) {
            return map_task_status(status);
        }
    }
    PlanStatus::Open
}

fn map_task_status(status: super::super::status::TaskState) -> PlanStatus {
    use super::super::status::TaskState;
    match status {
        TaskState::Open => PlanStatus::Open,
        TaskState::Blocked => PlanStatus::Blocked,
        TaskState::Completed => PlanStatus::Completed,
        TaskState::OutOfScope => PlanStatus::OutOfScope,
    }
}

fn map_phase_status(status: super::super::status::PhaseState) -> PlanStatus {
    use super::super::status::PhaseState;
    match status {
        PhaseState::Open => PlanStatus::Open,
        PhaseState::Blocked => PlanStatus::Blocked,
        PhaseState::Closed => PlanStatus::Closed,
        PhaseState::OutOfScope => PlanStatus::OutOfScope,
        PhaseState::Split => PlanStatus::Split,
    }
}

fn map_dependency_status(status: super::super::status::DependencyState) -> PlanStatus {
    use super::super::status::DependencyState;
    match status {
        DependencyState::Open => PlanStatus::Open,
        DependencyState::Completed => PlanStatus::Completed,
        DependencyState::OutOfScope => PlanStatus::OutOfScope,
    }
}
