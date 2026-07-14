use anyhow::Result;
use serde::Serialize;

use crate::identity::{CanonicalValue, ComponentHandle, IndexHandle, OwnerHandle};

use super::{SourceSnapshot, assembly, owner_handles, scope};

#[derive(Serialize)]
struct PlanIndexView {
    algorithm: &'static str,
    index_handle: IndexHandle,
    entries: Vec<PlanIndexEntry>,
}

#[derive(Serialize)]
struct PlanIndexEntry {
    owner_handle: OwnerHandle,
    component_handle: ComponentHandle,
    state: &'static str,
}

#[derive(Serialize)]
struct PlanIndexOutput {
    view: &'static str,
    index: PlanIndexView,
}

pub(crate) fn render(snapshot: &SourceSnapshot) -> Result<String> {
    let mut entries = snapshot
        .owners
        .iter()
        .map(|owner| {
            let plan = assembly::build(snapshot, owner)?;
            let has_ambiguity = !plan.ambiguities().is_empty();
            let handles = owner_handles(snapshot, owner);
            Ok(PlanIndexEntry {
                owner_handle: handles.0,
                component_handle: handles.1,
                state: scope::state(owner, has_ambiguity),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    entries.sort_by(|left, right| left.owner_handle.cmp(&right.owner_handle));
    let index_value = CanonicalValue::Array(
        entries
            .iter()
            .map(|entry| {
                CanonicalValue::object([
                    (
                        "owner_handle",
                        CanonicalValue::string(entry.owner_handle.as_str()),
                    ),
                    (
                        "component_handle",
                        CanonicalValue::string(entry.component_handle.as_str()),
                    ),
                    ("state", CanonicalValue::string(entry.state)),
                ])
            })
            .collect(),
    );
    serde_json::to_string(&PlanIndexOutput {
        view: "owner_index",
        index: PlanIndexView {
            algorithm: "ID-PLAN-INDEX-VIEW-v1",
            index_handle: IndexHandle::derive(b"AWB-PLAN-INDEX-HANDLE-v1\0", &index_value),
            entries,
        },
    })
    .map_err(Into::into)
}
