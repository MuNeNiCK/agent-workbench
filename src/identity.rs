use std::collections::BTreeMap;
use std::fmt::Write;

use anyhow::{Result, bail};
use serde::Serialize;
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CanonicalValue {
    Null,
    Integer(i64),
    String(String),
    Array(Vec<CanonicalValue>),
    Object(BTreeMap<String, CanonicalValue>),
}

impl CanonicalValue {
    pub(crate) fn object(
        entries: impl IntoIterator<Item = (&'static str, CanonicalValue)>,
    ) -> Self {
        Self::Object(
            entries
                .into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect(),
        )
    }

    pub(crate) fn string(value: impl Into<String>) -> Self {
        Self::String(value.into())
    }
}

pub(crate) fn canonical_bytes(value: &CanonicalValue) -> Vec<u8> {
    let mut output = String::new();
    write_canonical(value, &mut output);
    output.into_bytes()
}

pub(crate) fn domain_digest(domain: &[u8], value: &CanonicalValue) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(canonical_bytes(value));
    format!("{:x}", hasher.finalize())
}

pub(crate) fn normalize_identifier(value: &str) -> String {
    normalize_line_endings(value).nfc().collect()
}

pub(crate) fn normalize_document(value: &str) -> String {
    let normalized: String = normalize_line_endings(value).nfc().collect();
    format!("{}\n", normalized.trim_end_matches('\n'))
}

pub(crate) fn signed_source_id(value: i64) -> Result<String> {
    if value <= 0 {
        bail!("source identity must be a positive signed 64-bit integer");
    }
    Ok(value.to_string())
}

pub(crate) fn unsigned_source_revision(value: i64) -> Result<String> {
    if value < 0 {
        bail!("requirement revision must be non-negative");
    }
    Ok(value.to_string())
}

fn normalize_line_endings(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

fn write_canonical(value: &CanonicalValue, output: &mut String) {
    match value {
        CanonicalValue::Null => output.push_str("null"),
        CanonicalValue::Integer(value) => {
            write!(output, "{value}").expect("writing to String cannot fail")
        }
        CanonicalValue::String(value) => write_json_string(value, output),
        CanonicalValue::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_canonical(value, output);
            }
            output.push(']');
        }
        CanonicalValue::Object(values) => {
            output.push('{');
            for (index, (key, value)) in values.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_json_string(key, output);
                output.push(':');
                write_canonical(value, output);
            }
            output.push('}');
        }
    }
}

fn write_json_string(value: &str, output: &mut String) {
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0c}' => output.push_str("\\f"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            value if value <= '\u{1f}' => {
                write!(output, "\\u{:04x}", value as u32).expect("writing to String cannot fail");
            }
            value => output.push(value),
        }
    }
    output.push('"');
}

macro_rules! typed_handle {
    ($name:ident, $prefix:literal) => {
        #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
        #[serde(transparent)]
        #[allow(dead_code)]
        pub(crate) struct $name(String);

        #[allow(dead_code)]
        impl $name {
            pub(crate) fn derive(domain: &[u8], bindings: &CanonicalValue) -> Self {
                Self(format!("{}{}", $prefix, domain_digest(domain, bindings)))
            }

            pub(crate) fn derive_raw(domain: &[u8], parts: &[&[u8]]) -> Self {
                let mut hasher = Sha256::new();
                hasher.update(domain);
                for part in parts {
                    hasher.update(part);
                }
                Self(format!("{}{:x}", $prefix, hasher.finalize()))
            }

            pub(crate) fn parse(value: &str) -> Result<Self> {
                let digest = value.strip_prefix($prefix).ok_or_else(|| {
                    anyhow::anyhow!("invalid {} handle", $prefix.trim_end_matches('_'))
                })?;
                if digest.len() != 64
                    || !digest
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                {
                    bail!("invalid {} handle", $prefix.trim_end_matches('_'));
                }
                Ok(Self(value.to_string()))
            }

            pub(crate) fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

typed_handle!(PlanHandle, "plan_");
typed_handle!(IndexHandle, "index_");
typed_handle!(RefHandle, "ref_");
typed_handle!(OwnerHandle, "owner_");
typed_handle!(ComponentHandle, "component_");
typed_handle!(TaskHandle, "task_");
typed_handle!(RevisionHandle, "revision_");
typed_handle!(PhaseHandle, "phase_");
typed_handle!(MembershipHandle, "membership_");
typed_handle!(DependencyHandle, "dependency_");
typed_handle!(CompletionHandle, "completion_");
typed_handle!(AmbiguityHandle, "ambiguity_");
typed_handle!(ResolutionHandle, "resolution_");
typed_handle!(AuthorityHandle, "authority_");
typed_handle!(RequirementHandle, "requirement_");
typed_handle!(GateSetHandle, "gate_set_");
typed_handle!(PriorityHandle, "priority_");
typed_handle!(EvidenceHandle, "evidence_");
typed_handle!(EntryHandle, "entry_");
typed_handle!(RecoveryHandle, "recovery_");
typed_handle!(DecisionHandle, "decision_");
typed_handle!(BackupHandle, "backup_");
typed_handle!(AuditHandle, "audit_");
typed_handle!(PrincipalHandle, "principal_");
typed_handle!(AssertionHandle, "assertion_");
typed_handle!(GrantHandle, "grant_");
typed_handle!(CapabilityHandle, "capability_");
typed_handle!(ReviewProvenanceHandle, "review_provenance_");
typed_handle!(InvocationHandle, "invocation_");
typed_handle!(ReviewResultStageHandle, "review_stage_");
typed_handle!(ReviewResultVersionHandle, "review_stage_version_");
typed_handle!(ReviewResultItemHandle, "review_stage_item_");
typed_handle!(LegacyReviewerBindingHandle, "legacy_reviewer_binding_");
typed_handle!(DecisionContinuationHandle, "decision_continuation_");
