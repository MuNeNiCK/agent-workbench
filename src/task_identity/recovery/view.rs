use anyhow::Result;

use crate::identity::RecoveryHandle;

use super::BaseSelection;
use crate::task_identity::plan::model::Plan;

pub(super) fn render(
    selection: &BaseSelection<'_>,
    plan: &Plan,
    recovery_handle: &RecoveryHandle,
) -> Result<String> {
    let owner = &selection.snapshot.owners[selection.owner_index];
    super::super::plan::render_resolved_plan(
        &selection.snapshot,
        owner,
        plan,
        recovery_handle.as_str().to_string(),
    )
}
