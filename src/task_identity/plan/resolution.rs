use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail};

use crate::identity::domain_digest;

use super::assembly::build;
use super::model::{
    ArrayKind, Classification, Element, Fingerprints, Payload, Plan, PlanStatus, Reason,
};
use super::{OwnerSource, SourceSnapshot};

pub(crate) struct ResolutionDecision {
    pub(crate) ambiguity_digest: String,
    pub(crate) action: String,
    pub(crate) resolution_digest: Option<String>,
    pub(crate) authority_digest: String,
}

pub(crate) struct ResolvePlan<'a> {
    pub(crate) snapshot: &'a SourceSnapshot,
    pub(crate) owner: &'a OwnerSource,
    pub(crate) base: &'a Plan,
    pub(crate) recovery_digest: &'a str,
    pub(crate) selected_requirements: &'a BTreeMap<i64, i64>,
    pub(crate) retired_tasks: &'a BTreeSet<i64>,
    pub(crate) retired_dependencies: &'a BTreeSet<i64>,
    pub(crate) decisions: &'a [ResolutionDecision],
}

pub(crate) fn resolve(input: ResolvePlan<'_>) -> Result<Plan> {
    let ambiguities = input
        .base
        .ambiguities()
        .into_iter()
        .map(|ambiguity| (ambiguity.digest.clone(), ambiguity))
        .collect::<BTreeMap<_, _>>();
    let mut resolved_owner = input.owner.clone();
    resolved_owner
        .tasks
        .retain(|task| !input.retired_tasks.contains(&task.task_id));
    for task in &mut resolved_owner.tasks {
        if let Some(requirement_id) = input.selected_requirements.get(&task.task_id) {
            task.requirements
                .retain(|requirement| requirement.requirement_id == *requirement_id);
        }
    }
    resolved_owner.dependencies.retain(|dependency| {
        !input
            .retired_dependencies
            .contains(&dependency.dependency_id)
    });
    let mut plan = build(input.snapshot, &resolved_owner)?;
    let decided = input
        .decisions
        .iter()
        .map(|decision| decision.ambiguity_digest.as_str())
        .collect::<BTreeSet<_>>();
    plan.elements.retain(|element| {
        element.array != ArrayKind::Ambiguities || !decided.contains(element.sort_digest.as_str())
    });
    for decision in input.decisions {
        let ambiguity = ambiguities
            .get(&decision.ambiguity_digest)
            .context("recovery decision references an unknown base ambiguity")?;
        let resolution = decision
            .resolution_digest
            .as_ref()
            .map(|digest| {
                ambiguity
                    .resolutions
                    .iter()
                    .find(|resolution| &resolution.digest == digest)
                    .context("recovery decision selects an unknown resolution")
            })
            .transpose()?;
        let (action, classification, reason, selected, retired, status) =
            match (decision.action.as_str(), resolution) {
                ("map", Some(resolution)) => (
                    super::model::DispositionAction::Map,
                    Classification::Mapped,
                    Reason::AuthorityMapped,
                    resolution.selected_source_refs.clone(),
                    resolution.retired_source_refs.clone(),
                    None,
                ),
                ("retire", None) => (
                    super::model::DispositionAction::Retire,
                    Classification::Retired,
                    Reason::AuthorityRetired,
                    Vec::new(),
                    ambiguity.source_refs.clone(),
                    Some(PlanStatus::Retired),
                ),
                _ => bail!("recovery decision action and resolution disagree"),
            };
        let payload = Payload::Disposition {
            ambiguity_digest: decision.ambiguity_digest.clone(),
            action,
            resolution_digest: decision.resolution_digest.clone(),
            authority_digest: decision.authority_digest.clone(),
            selected_source_refs: selected,
            retired_source_refs: retired,
        };
        let digest = domain_digest(b"AWB-DISPOSITION-v1\0", &payload.value());
        plan.elements.push(Element {
            array: ArrayKind::Dispositions,
            source_refs: ambiguity.source_refs.clone(),
            target: None,
            sort_digest: digest,
            classification,
            reason,
            before: Fingerprints::default(),
            after: Fingerprints {
                status,
                ..Fingerprints::default()
            },
            payload,
        });
    }
    plan.mode = "resolved";
    plan.base_plan_sha256 = Some(input.base.digest());
    plan.recovery_sha256 = Some(input.recovery_digest.to_string());
    plan.validate()?;
    Ok(plan)
}
