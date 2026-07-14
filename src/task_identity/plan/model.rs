use anyhow::{Result, bail};

use crate::identity::{CanonicalValue, canonical_bytes, domain_digest, signed_source_id};

mod payload;
pub(super) use payload::*;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum ArrayKind {
    TaskIdentities,
    Revisions,
    Aliases,
    Memberships,
    Dependencies,
    CompletionClaims,
    Retirements,
    Dispositions,
    Ambiguities,
}

impl ArrayKind {
    pub(super) const ALL: [Self; 9] = [
        Self::TaskIdentities,
        Self::Revisions,
        Self::Aliases,
        Self::Memberships,
        Self::Dependencies,
        Self::CompletionClaims,
        Self::Retirements,
        Self::Dispositions,
        Self::Ambiguities,
    ];

    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::TaskIdentities => "task_identities",
            Self::Revisions => "revisions",
            Self::Aliases => "aliases",
            Self::Memberships => "memberships",
            Self::Dependencies => "dependencies",
            Self::CompletionClaims => "completion_claims",
            Self::Retirements => "retirements",
            Self::Dispositions => "dispositions",
            Self::Ambiguities => "ambiguities",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum RefFamily {
    Requirement,
    Task,
    Checklist,
    ChecklistItem,
    Coverage,
    Phase,
    Membership,
    Dependency,
    Evidence,
    Acceptance,
    ValidationRun,
}

impl RefFamily {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Requirement => "requirement",
            Self::Task => "task",
            Self::Checklist => "checklist",
            Self::ChecklistItem => "checklist_item",
            Self::Coverage => "coverage",
            Self::Phase => "phase",
            Self::Membership => "membership",
            Self::Dependency => "dependency",
            Self::Evidence => "evidence",
            Self::Acceptance => "acceptance",
            Self::ValidationRun => "validation_run",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceRef {
    family: RefFamily,
    identity: RefIdentity,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum RefIdentity {
    One {
        field: &'static str,
        id: i64,
    },
    Requirement {
        design_version_id: i64,
        requirement_id: i64,
    },
    Membership {
        phase_id: i64,
        task_id: i64,
    },
}

impl SourceRef {
    pub(super) fn one(family: RefFamily, field: &'static str, id: i64) -> Result<Self> {
        let expected = match family {
            RefFamily::Task => "task_id",
            RefFamily::Checklist => "checklist_id",
            RefFamily::ChecklistItem => "checklist_item_id",
            RefFamily::Coverage => "coverage_item_id",
            RefFamily::Phase => "phase_id",
            RefFamily::Dependency => "dependency_id",
            RefFamily::Evidence => "evidence_id",
            RefFamily::Acceptance => "acceptance_id",
            RefFamily::ValidationRun => "validation_run_id",
            RefFamily::Requirement | RefFamily::Membership => {
                bail!("compound source reference requires its typed constructor")
            }
        };
        if field != expected {
            bail!("source reference field does not match its closed family")
        }
        signed_source_id(id)?;
        Ok(Self {
            family,
            identity: RefIdentity::One { field, id },
        })
    }

    pub(super) fn requirement(design_version_id: i64, requirement_id: i64) -> Result<Self> {
        signed_source_id(design_version_id)?;
        signed_source_id(requirement_id)?;
        Ok(Self {
            family: RefFamily::Requirement,
            identity: RefIdentity::Requirement {
                design_version_id,
                requirement_id,
            },
        })
    }

    pub(super) fn membership(phase_id: i64, task_id: i64) -> Result<Self> {
        signed_source_id(phase_id)?;
        signed_source_id(task_id)?;
        Ok(Self {
            family: RefFamily::Membership,
            identity: RefIdentity::Membership { phase_id, task_id },
        })
    }

    pub(super) fn value(&self) -> CanonicalValue {
        CanonicalValue::object([
            ("family", CanonicalValue::string(self.family.as_str())),
            ("identity", self.identity_value()),
        ])
    }

    pub(crate) fn task_id(&self) -> Option<i64> {
        match (&self.family, &self.identity) {
            (RefFamily::Task, RefIdentity::One { id, .. }) => Some(*id),
            _ => None,
        }
    }

    pub(crate) fn dependency_id(&self) -> Option<i64> {
        match (&self.family, &self.identity) {
            (RefFamily::Dependency, RefIdentity::One { id, .. }) => Some(*id),
            _ => None,
        }
    }

    pub(crate) fn requirement_id(&self) -> Option<i64> {
        match self.identity {
            RefIdentity::Requirement { requirement_id, .. } => Some(requirement_id),
            _ => None,
        }
    }

    fn identity_value(&self) -> CanonicalValue {
        match self.identity {
            RefIdentity::One { field, id } => {
                CanonicalValue::object([(field, CanonicalValue::string(id.to_string()))])
            }
            RefIdentity::Requirement {
                design_version_id,
                requirement_id,
            } => CanonicalValue::object([
                (
                    "design_version_id",
                    CanonicalValue::string(design_version_id.to_string()),
                ),
                (
                    "requirement_id",
                    CanonicalValue::string(requirement_id.to_string()),
                ),
            ]),
            RefIdentity::Membership { phase_id, task_id } => CanonicalValue::object([
                ("phase_id", CanonicalValue::string(phase_id.to_string())),
                ("task_id", CanonicalValue::string(task_id.to_string())),
            ]),
        }
    }
}

impl Ord for SourceRef {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.family
            .cmp(&other.family)
            .then_with(|| self.identity.cmp(&other.identity))
    }
}

impl PartialOrd for SourceRef {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum TargetKind {
    TaskIdentity,
    Revision,
    Membership,
    Dependency,
    Completion,
}

impl TargetKind {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::TaskIdentity => "task_identity",
            Self::Revision => "revision",
            Self::Membership => "membership",
            Self::Dependency => "dependency",
            Self::Completion => "completion",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Target {
    pub(super) kind: TargetKind,
    pub(super) digest: String,
}

impl Target {
    pub(super) fn value(&self) -> CanonicalValue {
        CanonicalValue::object([
            ("kind", CanonicalValue::string(self.kind.as_str())),
            ("digest", CanonicalValue::string(self.digest.clone())),
        ])
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Classification {
    Mapped,
    Retired,
    Blocked,
}

impl Classification {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Mapped => "mapped",
            Self::Retired => "retired",
            Self::Blocked => "blocked",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Reason {
    Unique,
    DuplicateAlias,
    ChangedRevision,
    RemovedRevision,
    Manual,
    PhaseDeduplicated,
    CompletionCarried,
    ObligationReopened,
    Ambiguous,
    AuthorityMapped,
    AuthorityRetired,
}

impl Reason {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Unique => "unique",
            Self::DuplicateAlias => "duplicate_alias",
            Self::ChangedRevision => "changed_revision",
            Self::RemovedRevision => "removed_revision",
            Self::Manual => "manual",
            Self::PhaseDeduplicated => "phase_deduplicated",
            Self::CompletionCarried => "completion_carried",
            Self::ObligationReopened => "obligation_reopened",
            Self::Ambiguous => "ambiguous",
            Self::AuthorityMapped => "authority_mapped",
            Self::AuthorityRetired => "authority_retired",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum PlanStatus {
    Open,
    Active,
    Blocked,
    Closed,
    Completed,
    OutOfScope,
    Stale,
    Passed,
    Retired,
    Split,
}

impl PlanStatus {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Active => "active",
            Self::Blocked => "blocked",
            Self::Closed => "closed",
            Self::Completed => "completed",
            Self::OutOfScope => "out_of_scope",
            Self::Stale => "stale",
            Self::Passed => "passed",
            Self::Retired => "retired",
            Self::Split => "split",
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Fingerprints {
    pub(super) requirement: Option<String>,
    pub(super) gates: Option<String>,
    pub(super) priority: Option<String>,
    pub(super) status: Option<PlanStatus>,
}

impl Fingerprints {
    fn value(&self) -> CanonicalValue {
        CanonicalValue::object([
            ("requirement", optional_string(&self.requirement)),
            ("gates", optional_string(&self.gates)),
            ("priority", optional_string(&self.priority)),
            (
                "status",
                self.status
                    .map(|status| CanonicalValue::string(status.as_str()))
                    .unwrap_or(CanonicalValue::Null),
            ),
        ])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Element {
    pub(super) array: ArrayKind,
    pub(super) source_refs: Vec<SourceRef>,
    pub(super) target: Option<Target>,
    pub(super) sort_digest: String,
    pub(super) classification: Classification,
    pub(super) reason: Reason,
    pub(super) before: Fingerprints,
    pub(super) after: Fingerprints,
    pub(super) payload: Payload,
}

impl Element {
    pub(super) fn validate(&mut self) -> Result<()> {
        if self.payload.array_kind() != self.array {
            bail!("internal identity plan payload is in the wrong array");
        }
        self.source_refs.sort();
        self.source_refs.dedup();
        if self.source_refs.is_empty() {
            bail!("internal identity plan element has no source references");
        }
        let allowed = matches!(
            (self.array, self.classification, self.reason),
            (
                ArrayKind::TaskIdentities,
                Classification::Mapped,
                Reason::Unique | Reason::Manual
            ) | (
                ArrayKind::Revisions,
                Classification::Mapped,
                Reason::Unique | Reason::Manual | Reason::ChangedRevision
            ) | (
                ArrayKind::Aliases,
                Classification::Mapped,
                Reason::Unique | Reason::Manual | Reason::DuplicateAlias
            ) | (
                ArrayKind::Memberships,
                Classification::Mapped,
                Reason::Unique | Reason::PhaseDeduplicated
            ) | (
                ArrayKind::Dependencies,
                Classification::Mapped,
                Reason::Unique
            ) | (
                ArrayKind::CompletionClaims,
                Classification::Mapped,
                Reason::CompletionCarried | Reason::ObligationReopened
            ) | (
                ArrayKind::Retirements,
                Classification::Retired,
                Reason::RemovedRevision
            ) | (
                ArrayKind::Dispositions,
                Classification::Mapped,
                Reason::AuthorityMapped
            ) | (
                ArrayKind::Dispositions,
                Classification::Retired,
                Reason::AuthorityRetired
            ) | (
                ArrayKind::Ambiguities,
                Classification::Blocked,
                Reason::Ambiguous
            )
        );
        if !allowed {
            bail!(
                "internal identity plan element uses a forbidden array/classification/reason tuple"
            );
        }
        match self.array {
            ArrayKind::Dispositions | ArrayKind::Ambiguities if self.target.is_some() => {
                bail!("internal identity plan disposition or ambiguity has a target")
            }
            ArrayKind::Dispositions | ArrayKind::Ambiguities => {}
            _ if self.target.is_none() => {
                bail!("internal identity plan mapped element lacks a target")
            }
            _ => {}
        }
        validate_digest(&self.sort_digest)?;
        if let Some(target) = &self.target {
            validate_digest(&target.digest)?;
            if self.array != ArrayKind::Dispositions
                && self.array != ArrayKind::Ambiguities
                && self.sort_digest != target.digest
            {
                bail!("internal identity plan sort digest differs from its target");
            }
        }
        Ok(())
    }

    pub(super) fn value(&self) -> CanonicalValue {
        CanonicalValue::object([
            (
                "source_refs",
                CanonicalValue::Array(self.source_refs.iter().map(SourceRef::value).collect()),
            ),
            (
                "target",
                self.target
                    .as_ref()
                    .map(Target::value)
                    .unwrap_or(CanonicalValue::Null),
            ),
            (
                "sort_digest",
                CanonicalValue::string(self.sort_digest.clone()),
            ),
            (
                "classification",
                CanonicalValue::string(self.classification.as_str()),
            ),
            ("reason", CanonicalValue::string(self.reason.as_str())),
            ("before", self.before.value()),
            ("after", self.after.value()),
            ("payload", self.payload.value()),
        ])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Plan {
    pub(super) mode: &'static str,
    pub(super) base_plan_sha256: Option<String>,
    pub(super) recovery_sha256: Option<String>,
    pub(super) owner_digest: String,
    pub(super) component_sha256: String,
    pub(super) source_schema: i64,
    pub(super) source_sha256: String,
    pub(super) elements: Vec<Element>,
}

#[derive(Clone)]
pub(crate) struct OperationalAmbiguity {
    pub(crate) digest: String,
    pub(crate) source_refs: Vec<SourceRef>,
    pub(crate) resolutions: Vec<OperationalResolution>,
    pub(crate) retired_task_ids: Vec<i64>,
    pub(crate) retired_dependency_ids: Vec<i64>,
}

#[derive(Clone)]
pub(crate) struct OperationalResolution {
    pub(crate) digest: String,
    pub(crate) selected_source_refs: Vec<SourceRef>,
    pub(crate) retired_source_refs: Vec<SourceRef>,
}

impl Plan {
    pub(crate) fn ambiguities(&self) -> Vec<OperationalAmbiguity> {
        self.elements
            .iter()
            .filter_map(|element| {
                let Payload::Ambiguity {
                    code, resolutions, ..
                } = &element.payload
                else {
                    return None;
                };
                let dependency_conflict = matches!(
                    code,
                    AmbiguityCode::DependencySelf
                        | AmbiguityCode::DependencyReverse
                        | AmbiguityCode::DependencyState
                );
                Some(OperationalAmbiguity {
                    digest: element.sort_digest.clone(),
                    source_refs: element.source_refs.clone(),
                    resolutions: resolutions
                        .iter()
                        .map(|resolution| OperationalResolution {
                            digest: resolution.digest.clone(),
                            selected_source_refs: resolution.selected_source_refs.clone(),
                            retired_source_refs: resolution.retired_source_refs.clone(),
                        })
                        .collect(),
                    retired_task_ids: if dependency_conflict {
                        Vec::new()
                    } else {
                        element
                            .source_refs
                            .iter()
                            .filter_map(SourceRef::task_id)
                            .collect()
                    },
                    retired_dependency_ids: if dependency_conflict {
                        element
                            .source_refs
                            .iter()
                            .filter_map(SourceRef::dependency_id)
                            .collect()
                    } else {
                        Vec::new()
                    },
                })
            })
            .collect()
    }
    pub(super) fn validate(&mut self) -> Result<()> {
        match self.mode {
            "base" if self.base_plan_sha256.is_none() && self.recovery_sha256.is_none() => {}
            "resolved" if self.base_plan_sha256.is_some() && self.recovery_sha256.is_some() => {}
            _ => bail!("internal identity plan has inconsistent mode bindings"),
        }
        validate_digest(&self.owner_digest)?;
        validate_digest(&self.component_sha256)?;
        validate_digest(&self.source_sha256)?;
        if self.source_schema <= 0 {
            bail!("internal identity plan source schema is invalid");
        }
        for digest in [&self.base_plan_sha256, &self.recovery_sha256]
            .into_iter()
            .flatten()
        {
            validate_digest(digest)?;
        }
        for element in &mut self.elements {
            element.validate()?;
        }
        self.elements.sort_by(|left, right| {
            left.array
                .cmp(&right.array)
                .then(left.sort_digest.cmp(&right.sort_digest))
                .then_with(|| {
                    canonical_bytes(&CanonicalValue::Array(
                        left.source_refs.iter().map(SourceRef::value).collect(),
                    ))
                    .cmp(&canonical_bytes(&CanonicalValue::Array(
                        right.source_refs.iter().map(SourceRef::value).collect(),
                    )))
                })
                .then_with(|| canonical_bytes(&left.value()).cmp(&canonical_bytes(&right.value())))
        });
        self.elements.dedup_by(|left, right| left == right);
        Ok(())
    }

    pub(super) fn value(&self) -> CanonicalValue {
        let arrays = ArrayKind::ALL.map(|kind| {
            CanonicalValue::Array(
                self.elements
                    .iter()
                    .filter(|element| element.array == kind)
                    .map(Element::value)
                    .collect(),
            )
        });
        CanonicalValue::object([
            ("algorithm", CanonicalValue::string("ID-PLAN-v1")),
            ("mode", CanonicalValue::string(self.mode)),
            ("base_plan_sha256", optional_string(&self.base_plan_sha256)),
            ("recovery_sha256", optional_string(&self.recovery_sha256)),
            (
                "scope",
                CanonicalValue::object([
                    (
                        "owner_digest",
                        CanonicalValue::string(self.owner_digest.clone()),
                    ),
                    (
                        "component_sha256",
                        CanonicalValue::string(self.component_sha256.clone()),
                    ),
                ]),
            ),
            ("source_schema", CanonicalValue::Integer(self.source_schema)),
            (
                "source_sha256",
                CanonicalValue::string(self.source_sha256.clone()),
            ),
            (ArrayKind::TaskIdentities.as_str(), arrays[0].clone()),
            (ArrayKind::Revisions.as_str(), arrays[1].clone()),
            (ArrayKind::Aliases.as_str(), arrays[2].clone()),
            (ArrayKind::Memberships.as_str(), arrays[3].clone()),
            (ArrayKind::Dependencies.as_str(), arrays[4].clone()),
            (ArrayKind::CompletionClaims.as_str(), arrays[5].clone()),
            (ArrayKind::Retirements.as_str(), arrays[6].clone()),
            (ArrayKind::Dispositions.as_str(), arrays[7].clone()),
            (ArrayKind::Ambiguities.as_str(), arrays[8].clone()),
        ])
    }

    pub(crate) fn digest(&self) -> String {
        domain_digest(b"AWB-ID-PLAN-v1\0", &self.value())
    }
}

fn optional_string(value: &Option<String>) -> CanonicalValue {
    value
        .as_ref()
        .map(|value| CanonicalValue::string(value.clone()))
        .unwrap_or(CanonicalValue::Null)
}

fn validate_digest(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("internal identity plan digest is not lowercase SHA-256");
    }
    Ok(())
}
