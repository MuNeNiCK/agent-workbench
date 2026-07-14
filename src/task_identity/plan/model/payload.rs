use crate::identity::CanonicalValue;

use super::{Element, PlanStatus, SourceRef, Target};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum Claim {
    Implementation,
    Validation,
}

impl Claim {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Implementation => "implementation",
            Self::Validation => "validation",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CompletionResult {
    Carry,
    Reopen,
}

impl CompletionResult {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Carry => "carry",
            Self::Reopen => "reopen",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum AmbiguityCode {
    MissingIdentity,
    SameSequenceConflict,
    CrossOwner,
    CrossPackage,
    OpenOpenPhase,
    OpenClosedCompatible,
    MembershipState,
    DependencySelf,
    DependencyReverse,
    DependencyState,
    TerminalDisposition,
    CompletionInvalid,
}

impl AmbiguityCode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::MissingIdentity => "missing_identity",
            Self::SameSequenceConflict => "same_sequence_conflict",
            Self::CrossOwner => "cross_owner",
            Self::CrossPackage => "cross_package",
            Self::OpenOpenPhase => "open_open_phase",
            Self::OpenClosedCompatible => "open_closed_compatible",
            Self::MembershipState => "membership_state",
            Self::DependencySelf => "dependency_self",
            Self::DependencyReverse => "dependency_reverse",
            Self::DependencyState => "dependency_state",
            Self::TerminalDisposition => "terminal_disposition",
            Self::CompletionInvalid => "completion_invalid",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DispositionAction {
    Map,
    Retire,
}

impl DispositionAction {
    pub(crate) const fn as_str(&self) -> &'static str {
        match self {
            Self::Map => "map",
            Self::Retire => "retire",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Coordinate {
    pub(crate) array: super::ArrayKind,
    pub(crate) sort_digest: String,
}

impl Coordinate {
    fn value(&self) -> CanonicalValue {
        CanonicalValue::object([
            ("array", CanonicalValue::string(self.array.as_str())),
            (
                "sort_digest",
                CanonicalValue::string(self.sort_digest.clone()),
            ),
        ])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AddedElement {
    pub(crate) array: super::ArrayKind,
    pub(crate) element: Box<Element>,
}

impl AddedElement {
    fn value(&self) -> CanonicalValue {
        CanonicalValue::object([
            ("array", CanonicalValue::string(self.array.as_str())),
            ("element", self.element.value()),
        ])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Resolution {
    pub(crate) digest: String,
    pub(crate) selected_source_refs: Vec<SourceRef>,
    pub(crate) retired_source_refs: Vec<SourceRef>,
    pub(crate) remove: Vec<Coordinate>,
    pub(crate) add: Vec<AddedElement>,
}

impl Resolution {
    fn value(&self) -> CanonicalValue {
        CanonicalValue::object([
            ("digest", CanonicalValue::string(self.digest.clone())),
            (
                "selected_source_refs",
                CanonicalValue::Array(
                    self.selected_source_refs
                        .iter()
                        .map(SourceRef::value)
                        .collect(),
                ),
            ),
            (
                "retired_source_refs",
                CanonicalValue::Array(
                    self.retired_source_refs
                        .iter()
                        .map(SourceRef::value)
                        .collect(),
                ),
            ),
            (
                "remove",
                CanonicalValue::Array(self.remove.iter().map(Coordinate::value).collect()),
            ),
            (
                "add",
                CanonicalValue::Array(self.add.iter().map(AddedElement::value).collect()),
            ),
        ])
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Payload {
    TaskIdentity {
        identity_key: CanonicalValue,
    },
    Revision {
        identity_digest: String,
        design_sequence: Option<String>,
    },
    Alias {
        historical_task: String,
        revision_digest: String,
    },
    Membership {
        phase: String,
        identity_digest: String,
        state: PlanStatus,
    },
    Dependency {
        from_task: String,
        to_task: String,
        state: PlanStatus,
    },
    Completion {
        revision_digest: String,
        phase: Option<String>,
        claim: Claim,
        result: CompletionResult,
        evidence: Vec<String>,
    },
    Retirement {
        revision_digest: String,
        retired_sequence: String,
    },
    Disposition {
        ambiguity_digest: String,
        action: DispositionAction,
        resolution_digest: Option<String>,
        authority_digest: String,
        selected_source_refs: Vec<SourceRef>,
        retired_source_refs: Vec<SourceRef>,
    },
    Ambiguity {
        code: AmbiguityCode,
        claims: Vec<Claim>,
        candidates: Vec<Target>,
        resolutions: Vec<Resolution>,
    },
}

impl Payload {
    pub(crate) const fn array_kind(&self) -> super::ArrayKind {
        match self {
            Self::TaskIdentity { .. } => super::ArrayKind::TaskIdentities,
            Self::Revision { .. } => super::ArrayKind::Revisions,
            Self::Alias { .. } => super::ArrayKind::Aliases,
            Self::Membership { .. } => super::ArrayKind::Memberships,
            Self::Dependency { .. } => super::ArrayKind::Dependencies,
            Self::Completion { .. } => super::ArrayKind::CompletionClaims,
            Self::Retirement { .. } => super::ArrayKind::Retirements,
            Self::Disposition { .. } => super::ArrayKind::Dispositions,
            Self::Ambiguity { .. } => super::ArrayKind::Ambiguities,
        }
    }

    pub(crate) fn value(&self) -> CanonicalValue {
        match self {
            Self::TaskIdentity { identity_key } => {
                CanonicalValue::object([("identity_key", identity_key.clone())])
            }
            Self::Revision {
                identity_digest,
                design_sequence,
            } => CanonicalValue::object([
                (
                    "identity_digest",
                    CanonicalValue::string(identity_digest.clone()),
                ),
                (
                    "design_sequence",
                    design_sequence
                        .as_ref()
                        .map(|value| CanonicalValue::string(value.clone()))
                        .unwrap_or(CanonicalValue::Null),
                ),
            ]),
            Self::Alias {
                historical_task,
                revision_digest,
            } => CanonicalValue::object([
                (
                    "historical_task",
                    CanonicalValue::string(historical_task.clone()),
                ),
                (
                    "revision_digest",
                    CanonicalValue::string(revision_digest.clone()),
                ),
            ]),
            Self::Membership {
                phase,
                identity_digest,
                state,
            } => CanonicalValue::object([
                ("phase", CanonicalValue::string(phase.clone())),
                (
                    "identity_digest",
                    CanonicalValue::string(identity_digest.clone()),
                ),
                ("state", CanonicalValue::string(state.as_str())),
            ]),
            Self::Dependency {
                from_task,
                to_task,
                state,
            } => CanonicalValue::object([
                ("from_task", CanonicalValue::string(from_task.clone())),
                ("to_task", CanonicalValue::string(to_task.clone())),
                ("state", CanonicalValue::string(state.as_str())),
            ]),
            Self::Completion {
                revision_digest,
                phase,
                claim,
                result,
                evidence,
            } => CanonicalValue::object([
                (
                    "revision_digest",
                    CanonicalValue::string(revision_digest.clone()),
                ),
                (
                    "phase",
                    phase
                        .as_ref()
                        .map(|value| CanonicalValue::string(value.clone()))
                        .unwrap_or(CanonicalValue::Null),
                ),
                ("claim", CanonicalValue::string(claim.as_str())),
                ("result", CanonicalValue::string(result.as_str())),
                (
                    "evidence",
                    CanonicalValue::Array(
                        evidence
                            .iter()
                            .map(|digest| CanonicalValue::string(digest.clone()))
                            .collect(),
                    ),
                ),
            ]),
            Self::Retirement {
                revision_digest,
                retired_sequence,
            } => CanonicalValue::object([
                (
                    "revision_digest",
                    CanonicalValue::string(revision_digest.clone()),
                ),
                (
                    "retired_sequence",
                    CanonicalValue::string(retired_sequence.clone()),
                ),
            ]),
            Self::Disposition {
                ambiguity_digest,
                action,
                resolution_digest,
                authority_digest,
                selected_source_refs,
                retired_source_refs,
            } => CanonicalValue::object([
                (
                    "ambiguity_digest",
                    CanonicalValue::string(ambiguity_digest.clone()),
                ),
                ("action", CanonicalValue::string(action.as_str())),
                (
                    "resolution_digest",
                    resolution_digest
                        .as_ref()
                        .map(|value| CanonicalValue::string(value.clone()))
                        .unwrap_or(CanonicalValue::Null),
                ),
                (
                    "authority_digest",
                    CanonicalValue::string(authority_digest.clone()),
                ),
                (
                    "selected_source_refs",
                    CanonicalValue::Array(
                        selected_source_refs.iter().map(SourceRef::value).collect(),
                    ),
                ),
                (
                    "retired_source_refs",
                    CanonicalValue::Array(
                        retired_source_refs.iter().map(SourceRef::value).collect(),
                    ),
                ),
            ]),
            Self::Ambiguity {
                code,
                claims,
                candidates,
                resolutions,
            } => CanonicalValue::object([
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
                (
                    "resolutions",
                    CanonicalValue::Array(resolutions.iter().map(Resolution::value).collect()),
                ),
            ]),
        }
    }
}
