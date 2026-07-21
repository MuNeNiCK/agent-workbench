use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use sha2::{Digest, Sha256};

use crate::db::{open_existing_project, project_id};

mod attempt;
mod integrity;
mod lifecycle;
mod store;
pub(crate) use attempt::{
    finish_release_attempt, replay_completed_release_attempt, start_release_attempt,
};
pub(crate) use integrity::{
    migrate_release_candidate_revisions, validate_release_candidate_lineage,
};
pub(crate) use lifecycle::{
    active_release_candidates, publish_release_assets_with_action,
    publish_release_source_with_action, record_invalid_release_attempt_rejected,
    record_release_absent, record_release_reconciliation, verify_release_remotely_with_action,
    withdraw_release_candidate_with_action,
};
pub use lifecycle::{
    inspect_release_candidate, reconcile_release_candidate, verify_release_locally,
    withdraw_release_candidate,
};
pub(crate) use store::withdrawal_state;
use store::*;
pub use store::{assemble_release_candidate, supersede_release_candidate};

const REQUIRED_SUBJECTS: &[(&str, &str)] = &[
    ("local", "package-version"),
    ("local", "lockfile"),
    ("local", "binary-version"),
    ("local", "wrapper"),
    ("local", "skill"),
    ("local", "license"),
    ("local", "source-archive"),
    ("local", "release-notes"),
    ("source", "tag"),
    ("release", "release"),
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseSubjectInput {
    pub kind: String,
    pub name: String,
    pub expected_identity: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseObservation {
    pub name: String,
    pub identity: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewReleaseCandidate {
    pub work_unit_id: Option<i64>,
    pub version: String,
    pub reviewed_commit: String,
    pub idempotency_key: String,
    pub subjects: Vec<ReleaseSubjectInput>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseSubjectRecord {
    pub kind: String,
    pub name: String,
    pub expected_identity: String,
    pub local_identity: Option<String>,
    pub requested_identity: Option<String>,
    pub observed_identity: Option<String>,
    pub downloaded_identity: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseInspection {
    pub candidate_handle: String,
    pub version: String,
    pub reviewed_commit: String,
    pub work_unit_id: Option<i64>,
    pub state: String,
    pub(crate) stage: String,
    pub current_revision: String,
    pub subjects: Vec<ReleaseSubjectRecord>,
    pub next_action: String,
    pub(crate) withdrawal_requested_identity: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseTransitionOutcome {
    pub candidate_handle: String,
    pub work_unit_id: Option<i64>,
    pub current_revision: String,
    pub state: String,
    pub already_applied: bool,
    pub next_action: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ActiveReleaseCandidate {
    pub(crate) candidate_id: i64,
    pub(crate) candidate_handle: String,
    pub(crate) version: String,
    pub(crate) state: String,
    pub(crate) description: String,
    pub(crate) next_action: String,
}

#[derive(Clone, Debug)]
struct CurrentRelease {
    candidate_id: i64,
    candidate_handle: String,
    work_unit_id: Option<i64>,
    revision_id: i64,
    revision_handle: String,
    revision: i64,
    state: String,
    stage: String,
    action: String,
    reason: Option<String>,
    subjects: Vec<ReleaseSubjectRecord>,
}

pub(crate) enum ReleaseAttemptStart {
    Ready { attempt_id: i64, resumed: bool },
    Completed(ReleaseTransitionOutcome),
}
