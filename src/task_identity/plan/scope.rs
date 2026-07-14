use crate::identity::{ComponentHandle, OwnerHandle};

use super::{OwnerSource, SourceSnapshot};

pub(super) fn state(owner: &OwnerSource, has_ambiguity: bool) -> &'static str {
    if owner.migrated {
        "migrated"
    } else if owner.tasks.is_empty() {
        "legacy_safe"
    } else if owner.owner_conflict || has_ambiguity {
        "ambiguity_required"
    } else {
        "migration_required"
    }
}

pub(crate) fn handles(
    snapshot: &SourceSnapshot,
    owner: &OwnerSource,
) -> (OwnerHandle, ComponentHandle) {
    (
        OwnerHandle::derive_raw(
            b"AWB-OWNER-HANDLE-v1\0",
            &[
                snapshot.project_digest.as_bytes(),
                owner.source_digest.as_bytes(),
                owner.owner_digest.as_bytes(),
            ],
        ),
        ComponentHandle::derive_raw(
            b"AWB-COMPONENT-HANDLE-v1\0",
            &[
                snapshot.project_digest.as_bytes(),
                owner.source_digest.as_bytes(),
                owner.component_digest.as_bytes(),
            ],
        ),
    )
}
