use anyhow::Result;

use crate::identity::{
    CanonicalValue, canonical_bytes, domain_digest, normalize_document, normalize_identifier,
    signed_source_id, unsigned_source_revision,
};

use super::super::source::RequirementSource;
use super::{OwnerSource, TaskSource};

#[derive(Clone)]
pub(crate) struct PlannedTask {
    pub(crate) identity_key: CanonicalValue,
    pub(crate) identity_digest: String,
    pub(crate) revision_digest: String,
    pub(crate) design_sequence: Option<i64>,
    pub(crate) requirement_digest: String,
    pub(crate) gate_set_digest: Option<String>,
    pub(crate) priority_digest: Option<String>,
    pub(crate) source_requirement_id: Option<i64>,
    pub(crate) ambiguity: bool,
}

pub(crate) fn plan_task(
    project_id: i64,
    owner: &OwnerSource,
    task: &TaskSource,
) -> Result<PlannedTask> {
    if task.requirements.is_empty() {
        return plan_manual_task(project_id, task);
    }
    let mut candidates = task
        .requirements
        .iter()
        .map(|requirement| {
            Ok((
                requirement,
                plan_design_task(project_id, owner, requirement)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    candidates.sort_by(|(left_source, left), (right_source, right)| {
        left_source
            .design_sequence
            .cmp(&right_source.design_sequence)
            .then(left_source.revision.cmp(&right_source.revision))
            .then(left.revision_digest.cmp(&right.revision_digest))
    });
    let identities = candidates
        .iter()
        .map(|(_, candidate)| candidate.identity_digest.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let same_sequence_conflict = candidates.windows(2).any(|pair| {
        pair[0].0.design_sequence == pair[1].0.design_sequence
            && pair[0].1.revision_digest != pair[1].1.revision_digest
    });
    if identities.len() != 1 || same_sequence_conflict {
        return Ok(ambiguous());
    }
    let current = candidates
        .iter()
        .rev()
        .find(|(source, _)| {
            source.derivation_status != super::super::status::DerivationState::Stale
                && source.status == super::super::status::RequirementState::Active
        })
        .or_else(|| candidates.last())
        .expect("nonempty requirement candidates");
    Ok(current.1.clone())
}

fn ambiguous() -> PlannedTask {
    PlannedTask {
        identity_key: CanonicalValue::Null,
        identity_digest: String::new(),
        revision_digest: String::new(),
        design_sequence: None,
        requirement_digest: String::new(),
        gate_set_digest: None,
        priority_digest: None,
        source_requirement_id: None,
        ambiguity: true,
    }
}

pub(crate) fn plan_task_requirement(
    project_id: i64,
    owner: &OwnerSource,
    requirement: &RequirementSource,
) -> Result<PlannedTask> {
    plan_design_task(project_id, owner, requirement)
}

fn plan_design_task(
    project_id: i64,
    owner: &OwnerSource,
    requirement: &RequirementSource,
) -> Result<PlannedTask> {
    let identity_key = CanonicalValue::object([
        ("kind", CanonicalValue::string("design")),
        (
            "project",
            CanonicalValue::string(signed_source_id(project_id)?),
        ),
        (
            "package",
            CanonicalValue::string(signed_source_id(requirement.design_package_id)?),
        ),
        (
            "work",
            CanonicalValue::string(signed_source_id(owner.owner_id)?),
        ),
        (
            "requirement_key",
            CanonicalValue::string(normalize_identifier(&requirement.requirement_key)),
        ),
    ]);
    let identity_digest = domain_digest(b"AWB-TASK-IDENTITY-v1\0", &identity_key);
    let (heading, body) = requirement_text_parts(&requirement.requirement_text);
    let surfaces = parse_surfaces(requirement.surfaces.as_deref())?
        .into_iter()
        .map(CanonicalValue::string)
        .collect::<Vec<_>>();
    let normalized_heading = normalize_document(heading);
    let normalized_body = normalize_document(body);
    let normalized_priority = normalize_identifier(&requirement.priority);
    let requirement_semantics = CanonicalValue::object([
        (
            "heading",
            CanonicalValue::string(normalized_heading.clone()),
        ),
        ("body", CanonicalValue::string(normalized_body.clone())),
        (
            "priority",
            CanonicalValue::string(normalized_priority.clone()),
        ),
        ("surfaces", CanonicalValue::Array(surfaces.clone())),
    ]);
    let requirement_digest = domain_digest(
        b"AWB-REQ-v1\0",
        &CanonicalValue::object([
            ("heading", CanonicalValue::string(normalized_heading)),
            ("body", CanonicalValue::string(normalized_body)),
            ("surfaces", CanonicalValue::Array(surfaces)),
        ]),
    );
    let priority_digest = domain_digest(
        b"AWB-PRIORITY-v1\0",
        &CanonicalValue::object([("priority", CanonicalValue::string(normalized_priority))]),
    );
    let mut gates = requirement
        .gates
        .iter()
        .map(|gate| {
            let value = CanonicalValue::object([
                (
                    "key",
                    CanonicalValue::string(normalize_identifier(&gate.key)),
                ),
                (
                    "expected",
                    CanonicalValue::string(normalize_document(&gate.expected)),
                ),
                (
                    "phase",
                    CanonicalValue::string(normalize_identifier(&gate.stage)),
                ),
                (
                    "body",
                    CanonicalValue::string(normalize_document(&gate.body)),
                ),
            ]);
            (canonical_bytes(&value), value)
        })
        .collect::<Vec<_>>();
    gates.sort_by(|left, right| left.0.cmp(&right.0));
    gates.dedup_by(|left, right| left.0 == right.0);
    let mut gate_digests = gates
        .iter()
        .map(|(_, gate)| domain_digest(b"AWB-GATE-v1\0", gate))
        .collect::<Vec<_>>();
    gate_digests.sort();
    gate_digests.dedup();
    let gate_set_digest = domain_digest(
        b"AWB-GATES-v1\0",
        &CanonicalValue::Array(
            gate_digests
                .into_iter()
                .map(CanonicalValue::string)
                .collect(),
        ),
    );
    let revision = CanonicalValue::object([
        ("identity_key", identity_key.clone()),
        (
            "design_sequence",
            CanonicalValue::string(signed_source_id(requirement.design_sequence)?),
        ),
        (
            "design_version",
            CanonicalValue::string(signed_source_id(requirement.design_version_id)?),
        ),
        (
            "requirement_revision",
            CanonicalValue::string(unsigned_source_revision(requirement.revision)?),
        ),
        ("requirement", requirement_semantics),
        (
            "gates",
            CanonicalValue::Array(gates.into_iter().map(|(_, value)| value).collect()),
        ),
    ]);
    Ok(PlannedTask {
        identity_key,
        identity_digest,
        revision_digest: domain_digest(b"AWB-REVISION-v1\0", &revision),
        design_sequence: Some(requirement.design_sequence),
        requirement_digest,
        gate_set_digest: Some(gate_set_digest),
        priority_digest: Some(priority_digest),
        source_requirement_id: Some(requirement.requirement_id),
        ambiguity: false,
    })
}

fn plan_manual_task(project_id: i64, task: &TaskSource) -> Result<PlannedTask> {
    let identity_key = CanonicalValue::object([
        ("kind", CanonicalValue::string("manual")),
        (
            "project",
            CanonicalValue::string(signed_source_id(project_id)?),
        ),
        (
            "historical_task",
            CanonicalValue::string(signed_source_id(task.task_id)?),
        ),
    ]);
    let identity_digest = domain_digest(b"AWB-TASK-IDENTITY-v1\0", &identity_key);
    let revision = CanonicalValue::object([
        ("identity_key", identity_key.clone()),
        (
            "historical_task",
            CanonicalValue::string(signed_source_id(task.task_id)?),
        ),
        (
            "title",
            CanonicalValue::string(normalize_document(&task.title)),
        ),
        (
            "description",
            CanonicalValue::string(normalize_document(task.details.as_deref().unwrap_or(""))),
        ),
    ]);
    let revision_digest = domain_digest(b"AWB-MANUAL-REVISION-v1\0", &revision);
    Ok(PlannedTask {
        identity_key,
        identity_digest,
        requirement_digest: revision_digest.clone(),
        revision_digest,
        design_sequence: None,
        gate_set_digest: None,
        priority_digest: None,
        source_requirement_id: None,
        ambiguity: false,
    })
}

fn requirement_text_parts(value: &str) -> (&str, &str) {
    value.split_once('\n').unwrap_or((value, ""))
}

fn parse_surfaces(value: Option<&str>) -> Result<Vec<String>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    if value.is_empty() {
        return Ok(Vec::new());
    }
    let mut surfaces = value
        .split(',')
        .map(|surface| {
            if surface.is_empty() || surface != surface.trim() {
                anyhow::bail!("unreadable_source: surface is not canonical");
            }
            Ok(normalize_identifier(surface))
        })
        .collect::<Result<Vec<_>>>()?;
    surfaces.sort();
    surfaces.dedup();
    Ok(surfaces)
}
