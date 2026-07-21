use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{Context, Result, bail};
use fs2::FileExt;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::release::{
    NewReleaseCandidate, ReleaseAttemptStart, ReleaseInspection, ReleaseObservation,
    ReleaseSubjectInput, ReleaseTransitionOutcome, assemble_release_candidate,
    finish_release_attempt, inspect_release_candidate, publish_release_assets_with_action,
    publish_release_source_with_action, reconcile_release_candidate,
    record_invalid_release_attempt_rejected, record_release_absent, record_release_reconciliation,
    replay_completed_release_attempt, start_release_attempt, supersede_release_candidate,
    verify_release_locally, verify_release_remotely_with_action, withdraw_release_candidate,
    withdraw_release_candidate_with_action,
};

mod attempt;
mod inspection;
mod lifecycle;
mod transport;
use attempt::*;
pub use inspection::operator_inspect_release;
pub use lifecycle::*;
use transport::*;
pub use transport::{
    operator_assemble_release, operator_publish_release_assets, operator_publish_release_source,
    operator_verify_release_remote,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperatorReleaseAssemble {
    pub work_unit_id: Option<i64>,
    pub version: String,
    pub reviewed_commit: String,
    pub expected_current: String,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperatorReleaseMutation {
    pub candidate: String,
    pub expected_current: String,
    pub idempotency_key: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperatorReleaseAuthorityMutation {
    pub candidate: String,
    pub expected_current: String,
    pub idempotency_key: String,
    pub authority_event_id: i64,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OperatorReleaseSupersession {
    pub candidate: String,
    pub successor: String,
    pub expected_current: String,
    pub idempotency_key: String,
    pub authority_event_id: i64,
    pub reason: String,
}
