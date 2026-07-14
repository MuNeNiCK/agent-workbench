use serde::{Deserialize, Serialize};

use crate::identity::{CanonicalValue, domain_digest};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct RecoveryEnvelope {
    pub(super) algorithm: String,
    pub(super) project_digest: String,
    pub(super) owner_digest: String,
    pub(super) component_sha256: String,
    pub(super) source_sha256: String,
    pub(super) base_plan_sha256: String,
    pub(super) authorities: Vec<MigrationAuthority>,
    pub(super) decisions: Vec<MigrationDecision>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct MigrationAuthority {
    pub(super) digest: String,
    pub(super) project_digest: String,
    pub(super) owner_digest: String,
    pub(super) component_sha256: String,
    pub(super) source_sha256: String,
    pub(super) base_plan_sha256: String,
    pub(super) ambiguity_digest: String,
    pub(super) action: String,
    pub(super) resolution_digest: Option<String>,
    pub(super) statement: String,
    pub(super) provenance: String,
    pub(super) provenance_ref: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct MigrationDecision {
    pub(super) digest: String,
    pub(super) project_digest: String,
    pub(super) owner_digest: String,
    pub(super) component_sha256: String,
    pub(super) source_sha256: String,
    pub(super) base_plan_sha256: String,
    pub(super) ambiguity_digest: String,
    pub(super) action: String,
    pub(super) resolution_digest: Option<String>,
    pub(super) authority_digest: String,
}

pub(super) fn recovery_digest(envelope: &RecoveryEnvelope) -> String {
    domain_digest(b"AWB-RECOVERY-v1\0", &envelope_value(envelope))
}

pub(super) fn authority_value(
    authority: &MigrationAuthority,
    include_digest: bool,
) -> CanonicalValue {
    let mut entries = vec![
        ("action", CanonicalValue::string(authority.action.clone())),
        (
            "ambiguity_digest",
            CanonicalValue::string(authority.ambiguity_digest.clone()),
        ),
        (
            "base_plan_sha256",
            CanonicalValue::string(authority.base_plan_sha256.clone()),
        ),
        (
            "component_sha256",
            CanonicalValue::string(authority.component_sha256.clone()),
        ),
        (
            "owner_digest",
            CanonicalValue::string(authority.owner_digest.clone()),
        ),
        (
            "project_digest",
            CanonicalValue::string(authority.project_digest.clone()),
        ),
        (
            "provenance",
            CanonicalValue::string(authority.provenance.clone()),
        ),
        (
            "provenance_ref",
            CanonicalValue::string(authority.provenance_ref.clone()),
        ),
        (
            "resolution_digest",
            authority
                .resolution_digest
                .clone()
                .map(CanonicalValue::string)
                .unwrap_or(CanonicalValue::Null),
        ),
        (
            "source_sha256",
            CanonicalValue::string(authority.source_sha256.clone()),
        ),
        (
            "statement",
            CanonicalValue::string(authority.statement.clone()),
        ),
    ];
    if include_digest {
        entries.push(("digest", CanonicalValue::string(authority.digest.clone())));
    }
    CanonicalValue::object(entries)
}

pub(super) fn decision_value(decision: &MigrationDecision, include_digest: bool) -> CanonicalValue {
    let mut entries = vec![
        ("action", CanonicalValue::string(decision.action.clone())),
        (
            "ambiguity_digest",
            CanonicalValue::string(decision.ambiguity_digest.clone()),
        ),
        (
            "authority_digest",
            CanonicalValue::string(decision.authority_digest.clone()),
        ),
        (
            "base_plan_sha256",
            CanonicalValue::string(decision.base_plan_sha256.clone()),
        ),
        (
            "component_sha256",
            CanonicalValue::string(decision.component_sha256.clone()),
        ),
        (
            "owner_digest",
            CanonicalValue::string(decision.owner_digest.clone()),
        ),
        (
            "project_digest",
            CanonicalValue::string(decision.project_digest.clone()),
        ),
        (
            "resolution_digest",
            decision
                .resolution_digest
                .clone()
                .map(CanonicalValue::string)
                .unwrap_or(CanonicalValue::Null),
        ),
        (
            "source_sha256",
            CanonicalValue::string(decision.source_sha256.clone()),
        ),
    ];
    if include_digest {
        entries.push(("digest", CanonicalValue::string(decision.digest.clone())));
    }
    CanonicalValue::object(entries)
}

pub(super) fn envelope_value(envelope: &RecoveryEnvelope) -> CanonicalValue {
    CanonicalValue::object([
        (
            "algorithm",
            CanonicalValue::string(envelope.algorithm.clone()),
        ),
        (
            "project_digest",
            CanonicalValue::string(envelope.project_digest.clone()),
        ),
        (
            "owner_digest",
            CanonicalValue::string(envelope.owner_digest.clone()),
        ),
        (
            "component_sha256",
            CanonicalValue::string(envelope.component_sha256.clone()),
        ),
        (
            "source_sha256",
            CanonicalValue::string(envelope.source_sha256.clone()),
        ),
        (
            "base_plan_sha256",
            CanonicalValue::string(envelope.base_plan_sha256.clone()),
        ),
        (
            "authorities",
            CanonicalValue::Array(
                envelope
                    .authorities
                    .iter()
                    .map(|authority| authority_value(authority, true))
                    .collect(),
            ),
        ),
        (
            "decisions",
            CanonicalValue::Array(
                envelope
                    .decisions
                    .iter()
                    .map(|decision| decision_value(decision, true))
                    .collect(),
            ),
        ),
    ])
}
