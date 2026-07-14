mod lifecycle;
mod sources;

pub use lifecycle::*;
use sources::*;

pub struct NewKptReview<'a> {
    pub scope: Option<&'a str>,
    pub summary: Option<&'a str>,
    pub from: Option<&'a str>,
    pub period: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptReviewOutcome {
    pub kpt_review_id: i64,
    pub generated_item_count: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptReviewRecord {
    pub id: i64,
    pub scope: Option<String>,
    pub summary: Option<String>,
    pub status: String,
    pub created_at: String,
    pub closed_at: Option<String>,
}

pub struct NewKptItem<'a> {
    pub kpt_review_id: Option<i64>,
    pub item_type: &'a str,
    pub title: &'a str,
    pub details: Option<&'a str>,
    pub severity: &'a str,
    pub proposed_action: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemOutcome {
    pub kpt_item_id: i64,
    pub kpt_review_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemRecord {
    pub id: i64,
    pub kpt_review_id: i64,
    pub item_type: String,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub linked_task_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptReviewCloseOutcome {
    pub kpt_review_id: i64,
}

pub struct KptItemTaskConversion<'a> {
    pub kpt_item_id: i64,
    pub task_title: Option<&'a str>,
    pub details: Option<&'a str>,
    pub priority: &'a str,
    pub work_unit_id: Option<i64>,
}

pub struct KptItemReviewPolicyConversion<'a> {
    pub kpt_item_id: i64,
    pub name: Option<&'a str>,
    pub review_type: &'a str,
    pub max_fresh_agents: i64,
    pub max_resume_agents: i64,
    pub max_parallel_agents: i64,
    pub required_consecutive_clean_fresh_runs: i64,
    pub required_consecutive_clean_resume_runs: i64,
    pub stop_on_severity: &'a str,
    pub allow_new_findings_in_resume: bool,
    pub run_count_scope: &'a str,
    pub default_run_mode: &'a str,
    pub on_max_agents_exceeded: &'a str,
}

pub struct KptItemCommandProfileConversion<'a> {
    pub kpt_item_id: i64,
    pub name: Option<&'a str>,
    pub command: Option<&'a str>,
    pub command_type: &'a str,
    pub scope: Option<&'a str>,
    pub status: &'a str,
    pub stability: &'a str,
    pub timeout: Option<&'a str>,
    pub expected_result: Option<&'a str>,
    pub authority_event_id: Option<i64>,
}

pub struct KptItemDecisionConversion<'a> {
    pub kpt_item_id: i64,
    pub decision_key: Option<&'a str>,
    pub topic: Option<&'a str>,
    pub decision: Option<&'a str>,
    pub rationale: Option<&'a str>,
    pub compatibility_impact: Option<&'a str>,
    pub authority_refs: Option<&'a str>,
}

pub struct KptItemDesignVersionConversion {
    pub kpt_item_id: i64,
    pub design_version_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemConversionOutcome {
    pub kpt_item_conversion_id: i64,
    pub task_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemReviewPolicyConversionOutcome {
    pub kpt_item_conversion_id: i64,
    pub review_policy_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemCommandProfileConversionOutcome {
    pub kpt_item_conversion_id: i64,
    pub command_profile_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemDecisionConversionOutcome {
    pub kpt_item_conversion_id: i64,
    pub decision_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemDesignVersionConversionOutcome {
    pub kpt_item_conversion_id: i64,
    pub design_version_id: i64,
}
