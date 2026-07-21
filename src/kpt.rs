mod lifecycle;
mod lifecycle_state;
mod sources;

pub use lifecycle::*;
use lifecycle_state::*;
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
    pub details: Option<String>,
    pub proposed_action: Option<String>,
    pub current_handle: String,
    pub legal_actions: Vec<String>,
    pub conversion: Option<KptItemConversionRecord>,
    pub dismissal: Option<KptItemDismissalReceipt>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KptItemConversionTarget {
    Rule(i64),
    Correction(i64),
    Task(i64),
    CommandProfile(i64),
    ReviewPolicy(i64),
    Decision(i64),
    DesignVersion(i64),
}

impl KptItemConversionTarget {
    pub fn target_type(&self) -> &'static str {
        match self {
            Self::Rule(_) => "rule",
            Self::Correction(_) => "correction",
            Self::Task(_) => "task",
            Self::CommandProfile(_) => "command-profile",
            Self::ReviewPolicy(_) => "review-policy",
            Self::Decision(_) => "decision",
            Self::DesignVersion(_) => "design-version",
        }
    }

    pub fn target_id(&self) -> i64 {
        match self {
            Self::Rule(id)
            | Self::Correction(id)
            | Self::Task(id)
            | Self::CommandProfile(id)
            | Self::ReviewPolicy(id)
            | Self::Decision(id)
            | Self::DesignVersion(id) => *id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KptItemConversionReceipt {
    pub kpt_item_conversion_id: i64,
    pub kpt_item_id: i64,
    pub item_revision: String,
    pub target: KptItemConversionTarget,
    pub predecessor_handle: String,
    pub request_identity: String,
    pub receipt_identity: String,
    pub current_handle: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KptItemConversionRecord {
    pub kpt_item_conversion_id: i64,
    pub target: KptItemConversionTarget,
    pub receipt: Option<KptItemConversionReceipt>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KptConversionAlreadyCommitted {
    pub record: KptItemConversionRecord,
    pub current_handle: String,
    pub next: String,
}

impl std::fmt::Display for KptConversionAlreadyCommitted {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(formatter, "conversion_already_committed")?;
        writeln!(
            formatter,
            "kpt_item_conversion_id: {}",
            self.record.kpt_item_conversion_id
        )?;
        writeln!(
            formatter,
            "target_type: {}",
            self.record.target.target_type()
        )?;
        writeln!(formatter, "target_id: {}", self.record.target.target_id())?;
        if let Some(receipt) = &self.record.receipt {
            writeln!(
                formatter,
                "conversion.item_revision: {}",
                receipt.item_revision
            )?;
            writeln!(
                formatter,
                "conversion.predecessor: {}",
                receipt.predecessor_handle
            )?;
            writeln!(
                formatter,
                "conversion.request: {}",
                receipt.request_identity
            )?;
            writeln!(
                formatter,
                "conversion.receipt: {}",
                receipt.receipt_identity
            )?;
        }
        writeln!(formatter, "current: {}", self.current_handle)?;
        write!(formatter, "next: {}", self.next)
    }
}

impl std::error::Error for KptConversionAlreadyCommitted {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KptItemSourceBinding {
    pub source_kind: String,
    pub source_identity: String,
    pub source_revision: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KptItemDismissalReceipt {
    pub kpt_item_id: i64,
    pub item_revision: String,
    pub source: Option<KptItemSourceBinding>,
    pub review_revision: String,
    pub review_status: String,
    pub authority_event_id: i64,
    pub reason: String,
    pub predecessor_handle: String,
    pub decision_handle: String,
    pub current_handle: String,
    pub replay_identity: String,
}

pub struct KptItemDismissalRequest<'a> {
    pub kpt_item_id: i64,
    pub authority_event_id: i64,
    pub reason: &'a str,
    pub expected_current: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KptItemDismissalOutcome {
    Dismissed(KptItemDismissalReceipt),
    Existing(KptItemDismissalReceipt),
    InputInvalid {
        field: String,
        next: String,
    },
    AuthorityInvalid {
        authority_event_id: i64,
        required_scope: String,
        next: String,
    },
    StateChanged {
        expected: String,
        observed: String,
        next: String,
    },
    ItemTerminal {
        state: String,
        current: String,
        next: String,
    },
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

pub struct KptItemRuleConversion<'a> {
    pub kpt_item_id: i64,
    pub scope: Option<&'a str>,
    pub title: Option<&'a str>,
    pub body: Option<&'a str>,
}

pub struct KptItemCorrectionConversion<'a> {
    pub kpt_item_id: i64,
    pub scope: Option<&'a str>,
    pub source_label: Option<&'a str>,
    pub expected_change: Option<&'a str>,
    pub severity: &'a str,
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
    pub receipt: KptItemConversionReceipt,
    pub already_applied: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemRuleConversionOutcome {
    pub kpt_item_conversion_id: i64,
    pub kpt_rule_id: i64,
    pub receipt: KptItemConversionReceipt,
    pub already_applied: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemCorrectionConversionOutcome {
    pub kpt_item_conversion_id: i64,
    pub user_correction_id: i64,
    pub receipt: KptItemConversionReceipt,
    pub already_applied: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemReviewPolicyConversionOutcome {
    pub kpt_item_conversion_id: i64,
    pub review_policy_id: i64,
    pub receipt: KptItemConversionReceipt,
    pub already_applied: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemCommandProfileConversionOutcome {
    pub kpt_item_conversion_id: i64,
    pub command_profile_id: i64,
    pub receipt: KptItemConversionReceipt,
    pub already_applied: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemDecisionConversionOutcome {
    pub kpt_item_conversion_id: i64,
    pub decision_id: i64,
    pub receipt: KptItemConversionReceipt,
    pub already_applied: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemDesignVersionConversionOutcome {
    pub kpt_item_conversion_id: i64,
    pub design_version_id: i64,
    pub receipt: KptItemConversionReceipt,
    pub already_applied: bool,
}
