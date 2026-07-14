use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result, bail};

use super::RecoveryEnvelope;
use crate::task_identity::plan::model::OperationalAmbiguity;

pub(super) struct ResolutionProjection {
    pub(super) retired_task_ids: BTreeSet<i64>,
    pub(super) retired_dependency_ids: BTreeSet<i64>,
    pub(super) selected_requirement_ids: BTreeMap<i64, i64>,
}

pub(super) fn project(
    ambiguities: &[OperationalAmbiguity],
    envelope: &RecoveryEnvelope,
) -> Result<ResolutionProjection> {
    let mut retired_task_ids = BTreeSet::new();
    let mut retired_dependency_ids = BTreeSet::new();
    let mut selected_requirement_ids = BTreeMap::new();
    for decision in &envelope.decisions {
        let ambiguity = ambiguities
            .iter()
            .find(|ambiguity| ambiguity.digest == decision.ambiguity_digest)
            .context("recovery envelope contains a decision outside the selected base plan")?;
        match (
            decision.action.as_str(),
            decision.resolution_digest.as_deref(),
        ) {
            ("retire", None) => {
                retired_task_ids.extend(ambiguity.retired_task_ids.iter().copied());
                retired_dependency_ids.extend(ambiguity.retired_dependency_ids.iter().copied());
            }
            ("map", Some(selected)) => {
                let resolution = ambiguity
                    .resolutions
                    .iter()
                    .find(|resolution| resolution.digest == selected)
                    .context("resolved plan selects an unknown ambiguity candidate")?;
                let task_ids = ambiguity
                    .source_refs
                    .iter()
                    .filter_map(|source| source.task_id())
                    .collect::<BTreeSet<_>>();
                let requirement_ids = resolution
                    .selected_source_refs
                    .iter()
                    .filter_map(|source| source.requirement_id())
                    .collect::<BTreeSet<_>>();
                let task_ids = task_ids.into_iter().collect::<Vec<_>>();
                let [task_id] = task_ids.as_slice() else {
                    bail!("map resolution does not identify exactly one historical task")
                };
                let requirement_ids = requirement_ids.into_iter().collect::<Vec<_>>();
                let [requirement_id] = requirement_ids.as_slice() else {
                    bail!("map resolution does not identify exactly one requirement")
                };
                selected_requirement_ids.insert(*task_id, *requirement_id);
            }
            _ => bail!("resolved plan contains an unsupported ambiguity action"),
        }
    }
    Ok(ResolutionProjection {
        retired_task_ids,
        retired_dependency_ids,
        selected_requirement_ids,
    })
}
