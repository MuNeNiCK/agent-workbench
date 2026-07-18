mod completion;
mod operations;
mod rescope;

pub(crate) use completion::phase_review_lifecycle_action;
use completion::*;
pub use operations::*;
use rescope::*;

pub struct NewWorkPhase<'a> {
    pub work_unit_id: i64,
    pub design_version_id: Option<i64>,
    pub key: &'a str,
    pub title: &'a str,
    pub kind: &'a str,
    pub order: i64,
    pub reason: Option<&'a str>,
}

pub struct NewPhaseDependency<'a> {
    pub from_phase_id: i64,
    pub to_phase_id: i64,
    pub dependency_type: &'a str,
    pub reason: &'a str,
}

pub struct NewPhaseTraceDecision<'a> {
    pub phase_id: i64,
    pub record_type: &'a str,
    pub record_id: i64,
    pub decision: &'a str,
    pub reason: &'a str,
    pub authority_event_id: i64,
}

pub struct PhaseRescope<'a> {
    pub phase_id: i64,
    pub to_work_unit_id: Option<i64>,
    pub shared_record_policy: &'a str,
    pub dry_run: bool,
}

pub struct PhaseSplit<'a> {
    pub phase_id: i64,
    pub title: &'a str,
    pub reason: &'a str,
    pub shared_record_policy: &'a str,
    pub dry_run: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct WorkPhaseOutcome {
    pub phase_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseTaskOutcome {
    pub phase_id: i64,
    pub task_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseDependencyOutcome {
    pub dependency_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseTraceDecisionOutcome {
    pub decision_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseCloseOutcome {
    pub phase_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseAcceptanceOutcome {
    pub phase_id: i64,
    pub authority_event_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseReviewTargetOutcome {
    pub review_plan_target_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct WorkPhaseRecord {
    pub id: i64,
    pub work_unit_id: i64,
    pub phase_work_unit_id: Option<i64>,
    pub design_version_id: Option<i64>,
    pub key: String,
    pub title: String,
    pub kind: String,
    pub order: i64,
    pub status: String,
    pub task_count: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseDependencyRecord {
    pub id: i64,
    pub from_phase_id: i64,
    pub from_phase_key: String,
    pub to_phase_id: i64,
    pub to_phase_key: String,
    pub dependency_type: String,
    pub status: String,
    pub reason: String,
    pub evidence_ref: Option<String>,
    pub authority_event_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseTraceRecord {
    pub record_type: String,
    pub id: i64,
    pub status: String,
    pub label: String,
    pub decision: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseInventory {
    pub phase_id: i64,
    pub trace: Vec<PhaseTraceRecord>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseRescopeOutcome {
    pub phase_id: i64,
    pub source_work_unit_id: i64,
    pub target_work_unit_id: Option<i64>,
    pub result: String,
    pub inventory: Vec<String>,
    pub blockers: Vec<PhaseRescopeBlocker>,
    pub warnings: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseRescopeBlocker {
    pub kind: String,
    pub details: String,
    pub next_action: String,
}

impl PhaseRescopeBlocker {
    fn new(kind: &str, details: String, next_action: String) -> Self {
        Self {
            kind: kind.to_string(),
            details,
            next_action,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseCloseReadyOutcome {
    pub phase_id: i64,
    pub work_unit_id: Option<i64>,
    pub result: String,
    pub items: Vec<PhaseCloseReadyItem>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PhaseCloseReadyItem {
    pub name: String,
    pub result: String,
    pub blocking_action: Option<String>,
    pub details: String,
}

impl PhaseCloseReadyItem {
    fn pass(name: &str, details: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            result: "pass".to_string(),
            blocking_action: None,
            details: details.into(),
        }
    }

    fn fail(name: &str, action: &str, details: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            result: "fail".to_string(),
            blocking_action: Some(action.to_string()),
            details: details.into(),
        }
    }
}

struct PhaseReviewPlan {
    id: i64,
    review_type: String,
    stage: String,
    design_version_id: Option<i64>,
    work_unit_id: i64,
    required_clean_fresh_runs: i64,
}

pub(crate) struct PhaseReviewLifecycleAction {
    pub(crate) phase_id: i64,
    pub(crate) review_plan_id: i64,
    pub(crate) review_type: String,
    pub(crate) stage: String,
    pub(crate) action: String,
}

#[derive(Debug)]
struct StoredPhase {
    id: i64,
    work_unit_id: i64,
    phase_work_unit_id: Option<i64>,
    status: String,
}

struct SharedRecordRef {
    record_type: String,
    record_id: i64,
}
