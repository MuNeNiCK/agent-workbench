mod activation;
mod close_repository;
mod close_trace;
mod continuation;
mod forking;
mod lifecycle;
mod resume_validation;

use anyhow::Result;

pub use activation::*;
pub use continuation::*;
pub use forking::*;
pub use lifecycle::*;

#[derive(Debug)]
struct StoredResumeCheck {
    id: i64,
    work_unit_id: i64,
    activation_id: i64,
    result: String,
    status: String,
    authority_event_high_watermark: Option<i64>,
    activation_stack_revision: Option<i64>,
    maturity: String,
    repository_snapshot_id: Option<i64>,
    repository_state_revision: Option<i64>,
}

struct ResumeGateEvaluation {
    work_unit_id: i64,
    activation_id: i64,
    suspend_snapshot_id: i64,
    resume_result: String,
    blocking_reason: Option<String>,
    allowed_next_action: Option<String>,
    authority_high_watermark: i64,
    activation_stack_revision: i64,
    repository_snapshot_id: Option<i64>,
    repository_state_revision: Option<i64>,
    items: Vec<ResumeReadyItem>,
}

struct TraceResumeCounts {
    stale_design_records: i64,
    stale_task_derivations: i64,
    stale_checklists: i64,
    stale_selected_gates: i64,
    stale_coverage_items: i64,
}

struct CloseTraceState {
    active_requirement_count: i64,
    derived_task_count: i64,
    missing_evidence_count: i64,
    missing_coverage_count: i64,
    missing_requirement_coverage_count: i64,
    missing_validation_gate_count: i64,
    open_checklist_item_count: i64,
    active_checklist_count: i64,
}

struct ValidationCloseState {
    selected_gate_count: i64,
    missing_run_count: i64,
    accepted_failure_count: i64,
    unaccepted_failure_count: i64,
}

struct CloseProcessState {
    applicable_rule_count: i64,
    rule_conflict_count: i64,
    fixed_command_count: i64,
    missing_fixed_command_usage_count: i64,
    repeated_correction_count: i64,
    unsettled_repeated_correction_count: i64,
    open_kpt_review_count: i64,
    unsettled_kpt_item_count: i64,
    work_record_count: i64,
    work_record_evidence_link_count: i64,
}

struct RepositoryCloseState {
    repository_count: i64,
    missing_snapshot_count: i64,
    unclassified_dirty_state_count: i64,
    missing_comparison_count: i64,
    unclassified_comparison_count: i64,
}

#[derive(Default)]
struct ReviewPlanStageState {
    required_plan_count: i64,
    incomplete_required_plan_count: i64,
    missing_context_run_count: i64,
    stale_target_count: i64,
}

struct ReviewPlanTargetForResume {
    target_type: String,
    design_version_id: Option<i64>,
    design_requirement_id: Option<i64>,
    repository_snapshot_id: Option<i64>,
}

struct LifecycleWorkUnit {
    work_unit_id: i64,
    activation_id: Option<i64>,
    status: String,
}

#[derive(Default)]
struct RepositoryResumeState {
    repository_count: i64,
    base_snapshot_count: i64,
    missing_base_snapshot_count: i64,
    missing_current_snapshot_count: i64,
    missing_comparison_count: i64,
    unclassified_comparison_count: i64,
    unclassified_dirty_state_count: i64,
    latest_current_snapshot_id: Option<i64>,
}

struct StoredForkSource {
    source_work_unit_id: Option<i64>,
    source_work_unit_activation_id: Option<i64>,
    source_work_record_id: Option<i64>,
    source_repository_snapshot_id: Option<i64>,
    source_git_commit_id: Option<i64>,
    source_git_commit_sha: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct WorkOutcome {
    pub work_unit_id: i64,
    pub activation_id: i64,
}

pub struct WorkStart<'a> {
    pub title: &'a str,
    pub responsibility: Option<&'a str>,
    pub design_version_id: Option<i64>,
    pub implementation: bool,
}

pub struct WorkActivate<'a> {
    pub work_unit_id: i64,
    pub design_version_id: Option<i64>,
    pub implementation: bool,
    pub reason: Option<&'a str>,
}

pub struct WorkReopen<'a> {
    pub work_unit_id: i64,
    pub reason: &'a str,
    pub reason_type: &'a str,
    pub authority_event_id: Option<i64>,
    pub acceptance_record_id: Option<i64>,
}

pub struct WorkRemediate {
    pub finding_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct WorkRemediateOutcome {
    pub work_unit_id: i64,
    pub activation_id: i64,
    pub binding_count: i64,
    pub idempotent: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct WorkStatusOutcome {
    pub work_unit_id: i64,
    pub activation_id: Option<i64>,
    pub previous_status: String,
    pub status: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SuspendOutcome {
    pub work_unit_id: i64,
    pub activation_id: i64,
    pub suspend_snapshot_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct InterruptOutcome {
    pub parent_work_unit_id: i64,
    pub parent_activation_id: i64,
    pub parent_suspend_snapshot_id: i64,
    pub child_work_unit_id: i64,
    pub child_activation_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CloseOutcome {
    pub work_unit_id: i64,
    pub activation_id: i64,
}

/// Result of the canonical owner-qualified close operation.
///
/// `activation_id` is absent when the open work had no current activation. The
/// legacy `close_active_work` adapter retains its activation-required result.
#[derive(Debug, PartialEq, Eq)]
pub struct CloseWorkOutcome {
    pub work_unit_id: i64,
    pub activation_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CloseReadyOutcome {
    pub work_unit_id: Option<i64>,
    pub activation_id: Option<i64>,
    pub result: String,
    pub blocking_reason: Option<String>,
    pub items: Vec<CloseReadyItem>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CloseReadyItem {
    pub name: String,
    pub result: String,
    pub blocking_action: Option<String>,
    pub details: String,
}

impl CloseReadyItem {
    fn pass(name: &str, details: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            result: "pass".to_string(),
            blocking_action: None,
            details: details.into(),
        }
    }

    fn fail(name: &str, blocking_action: &str, details: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            result: "fail".to_string(),
            blocking_action: Some(blocking_action.to_string()),
            details: details.into(),
        }
    }
}

fn append_detail_list(base: String, label: &str, details: &[String]) -> String {
    if details.is_empty() {
        return base;
    }
    let shown = details.iter().take(20).cloned().collect::<Vec<_>>();
    let suffix = if details.len() > shown.len() {
        format!(", ... +{} more", details.len() - shown.len())
    } else {
        String::new()
    };
    format!("{base}; {label}: {}{suffix}", shown.join(", "))
}

fn format_optional_id(id: Option<i64>) -> String {
    id.map(|id| id.to_string())
        .unwrap_or_else(|| "-".to_string())
}

fn collect_rows<T, F>(rows: rusqlite::MappedRows<'_, F>) -> Result<Vec<T>>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
{
    let mut values = Vec::new();
    for row in rows {
        values.push(row?);
    }
    Ok(values)
}

#[derive(Debug, PartialEq, Eq)]
pub struct FollowUpOutcome {
    pub source_work_unit_id: i64,
    pub work_unit_id: i64,
    pub activation_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ResumeCheckOutcome {
    pub resume_check_id: i64,
    pub result: String,
    pub blocking_reason: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ResumeReadyOutcome {
    pub work_unit_id: Option<i64>,
    pub activation_id: Option<i64>,
    pub result: String,
    pub blocking_reason: Option<String>,
    pub items: Vec<ResumeReadyItem>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ResumeReadyItem {
    pub name: String,
    pub result: String,
    pub blocking_action: Option<String>,
    pub details: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ResumeOutcome {
    pub work_unit_id: i64,
    pub activation_id: i64,
}

pub struct NewWorkFork<'a> {
    pub title: &'a str,
    pub source: WorkForkSource<'a>,
    pub reason: &'a str,
    pub discard_policy: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkForkSource<'a> {
    Record(i64),
    Activation(i64),
    Commit(&'a str),
    GitCommit(i64),
    RepositorySnapshot(i64),
}

#[derive(Debug, PartialEq, Eq)]
pub struct WorkForkOutcome {
    pub fork_id: i64,
    pub work_unit_id: i64,
    pub activation_id: i64,
}
