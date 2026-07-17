mod adjudication;
mod closure;
mod correction_contract;
mod correction_state;
mod correction_transition;
mod evaluation;
mod findings;
mod invocations;
mod plans;
mod result_stage;
mod state;

pub use adjudication::*;
pub use closure::*;
pub(crate) use correction_contract::*;
pub use correction_transition::*;
pub use evaluation::*;
pub use findings::*;
pub(crate) use invocations::validate_invocation_plan_context;
pub use invocations::*;
pub use plans::*;
pub use result_stage::*;
pub use state::*;

struct StoredReviewPlan {
    id: i64,
    review_policy_id: i64,
    review_scope_id: Option<i64>,
    design_version_id: Option<i64>,
    work_unit_id: i64,
    review_type: String,
    stage: String,
    fresh_review_after_run_id: i64,
}

struct StoredReviewPolicy {
    max_fresh_agents: i64,
    max_resume_agents: i64,
    max_parallel_agents: i64,
    required_consecutive_clean_fresh_runs: i64,
    required_consecutive_clean_resume_runs: i64,
    stop_on_severity: String,
    allow_resume_review: bool,
    allow_fresh_review: bool,
    allow_new_findings_in_resume: bool,
    on_max_agents_exceeded: String,
    run_count_scope: String,
}

struct StoredFinding {
    id: i64,
    classification: String,
    status: String,
}

struct CorrectionToken {
    kind: String,
    operation: String,
    target: String,
}

struct StoredReviewRunPolicy {
    run_type: String,
    review_policy_id: i64,
    review_type: String,
    clean_run: bool,
}

struct StoredReviewRunPurpose {
    run_type: String,
    run_purpose: String,
    finding_fix_result: Option<String>,
    clean_run: bool,
    new_findings_count: i64,
    carried_findings_checked: i64,
    _target_ref: Option<String>,
    review_provenance: String,
    review_provenance_ref: Option<String>,
    has_external_agent: bool,
}

struct ResolvedRunTarget {
    target_type: &'static str,
    design_version_id: Option<i64>,
    design_requirement_id: Option<i64>,
    task_id: Option<i64>,
    work_unit_id: Option<i64>,
    phase_id: Option<i64>,
    repository_snapshot_id: Option<i64>,
    file_path: Option<String>,
    symbol: Option<String>,
    target_ref: String,
}

impl ResolvedRunTarget {
    fn typed_id(target_type: &'static str, id: i64) -> Self {
        Self {
            target_type,
            design_version_id: (target_type == "design_version").then_some(id),
            design_requirement_id: (target_type == "design_requirement").then_some(id),
            task_id: (target_type == "task").then_some(id),
            work_unit_id: (target_type == "work_unit").then_some(id),
            phase_id: (target_type == "phase").then_some(id),
            repository_snapshot_id: (target_type == "repository_snapshot").then_some(id),
            file_path: None,
            symbol: None,
            target_ref: format!("{target_type}:{id}"),
        }
    }

    fn with_ref(mut self, target_ref: &str) -> Self {
        self.target_ref = target_ref.to_string();
        self
    }
}

pub struct NewReviewScope<'a> {
    pub name: &'a str,
    pub review_type: &'a str,
    pub scope: &'a str,
    pub allowed_inputs: Option<&'a str>,
    pub forbidden_judgments: Option<&'a str>,
    pub expected_output_type: Option<&'a str>,
    pub exclusions: Option<&'a str>,
    pub prompt_template_ref: Option<&'a str>,
}

pub struct NewReviewPolicy<'a> {
    pub name: &'a str,
    pub review_type: &'a str,
    pub max_fresh_agents: i64,
    pub max_resume_agents: i64,
    pub max_parallel_agents: i64,
    pub required_consecutive_clean_fresh_runs: i64,
    pub required_consecutive_clean_resume_runs: i64,
    pub stop_on_severity: &'a str,
    pub allow_resume_review: bool,
    pub allow_fresh_review: bool,
    pub allow_new_findings_in_resume: bool,
    pub on_max_agents_exceeded: &'a str,
    pub run_count_scope: &'a str,
    pub default_run_mode: &'a str,
}

pub struct NewReviewPlan<'a> {
    pub work_unit_id: i64,
    pub design_version_id: Option<i64>,
    pub review_type: &'a str,
    pub required: bool,
    pub stage: &'a str,
    pub scope: Option<&'a str>,
    pub clean_condition: Option<&'a str>,
    pub stop_condition: Option<&'a str>,
    pub review_policy_id: Option<i64>,
    pub review_scope_id: Option<i64>,
}

pub struct NewReviewPlanTarget<'a> {
    pub review_plan_id: i64,
    pub target_type: &'a str,
    pub design_version_id: Option<i64>,
    pub design_requirement_id: Option<i64>,
    pub task_id: Option<i64>,
    pub work_unit_id: Option<i64>,
    pub phase_id: Option<i64>,
    pub repository_snapshot_id: Option<i64>,
    pub file_path: Option<&'a str>,
    pub symbol: Option<&'a str>,
}

pub struct ReviewPlanWaiver<'a> {
    pub review_plan_id: i64,
    pub reason: &'a str,
    pub approval_authority_event_id: i64,
}

pub struct NewReviewRun<'a> {
    pub review_plan_id: i64,
    pub run_type: &'a str,
    pub run_purpose: &'a str,
    pub target_ref: Option<&'a str>,
    pub prompt_deviations: Option<&'a str>,
    pub result_summary: Option<&'a str>,
    pub new_findings_count: i64,
    pub carried_findings_checked: i64,
    pub clean_run: bool,
    pub status: &'a str,
    pub agent_label: Option<&'a str>,
    pub external_agent_id: Option<&'a str>,
    pub review_provenance: &'a str,
    pub review_provenance_ref: Option<&'a str>,
}

pub struct NewFinding<'a> {
    pub review_run_id: i64,
    pub finding_type: &'a str,
    pub severity: &'a str,
    pub description: &'a str,
    pub design_requirement_id: Option<i64>,
    pub task_id: Option<i64>,
}

pub struct NewClosure<'a> {
    pub finding_id: i64,
    pub design_invariant: &'a str,
    pub design_citations: Option<&'a str>,
    pub implementation_evidence: Option<&'a str>,
    pub affected_surfaces: Option<&'a str>,
    pub same_invariant_search: Option<&'a str>,
    pub other_violations_found: Option<&'a str>,
    pub fix_plan: Option<&'a str>,
    pub tests_or_gates: Option<&'a str>,
    pub verification_plan: Option<&'a str>,
    pub closed_by_commit: Option<&'a str>,
}

pub struct ClosureReady<'a> {
    pub closure_id: i64,
    pub implementation_evidence: &'a str,
    pub tests_or_gates: &'a str,
    pub closed_by_commit: Option<&'a str>,
}

pub struct ClosureSupersession<'a> {
    pub closure_id: i64,
    pub new_closure: NewClosure<'a>,
    pub reason: &'a str,
    pub authority_event_id: i64,
}

pub struct FindingOutOfScope<'a> {
    pub finding_id: i64,
    pub reason: &'a str,
    pub authority_event_id: i64,
}

pub struct NewFindingVerification<'a> {
    pub review_run_id: i64,
    pub finding_id: i64,
    pub closure_id: i64,
    pub result: &'a str,
    pub notes: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReviewScopeOutcome {
    pub review_scope_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReviewPolicyOutcome {
    pub review_policy_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReviewPlanOutcome {
    pub review_plan_id: i64,
    pub review_policy_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReviewPlanTargetOutcome {
    pub review_plan_target_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReviewPlanWaiverOutcome {
    pub review_plan_id: i64,
    pub acceptance_record_id: i64,
    pub authority_event_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReviewRunOutcome {
    pub review_run_id: i64,
    pub review_agent_invocation_id: i64,
    pub review_plan_id: i64,
    pub plan_status: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct FindingOutcome {
    pub finding_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct FindingClassificationOutcome {
    pub finding_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ClosureOutcome {
    pub closure_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CorrectionBeginOutcome {
    pub closure_id: i64,
    pub session_id: i64,
    pub token_count: i64,
    pub idempotent: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CorrectionTransitionOutcome {
    pub closure_id: i64,
    pub token_ordinal: i64,
    pub application_id: i64,
    pub result_ref: String,
    pub idempotent: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ClosureReadyOutcome {
    pub closure_id: i64,
    pub finding_id: i64,
    pub attempt_id: i64,
    pub attempt_number: i64,
    pub context_ref: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ClosureSupersessionOutcome {
    pub closure_id: i64,
    pub superseded_closure_id: i64,
    pub finding_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct FindingOutOfScopeOutcome {
    pub finding_id: i64,
    pub acceptance_record_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct FindingVerificationOutcome {
    pub finding_verification_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReviewScopeRecord {
    pub id: i64,
    pub name: String,
    pub review_type: String,
    pub agent_role: String,
    pub scope: String,
    pub status: String,
    pub no_findings_streak: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReviewPolicyRecord {
    pub id: i64,
    pub name: String,
    pub review_type: String,
    pub max_fresh_agents: i64,
    pub max_resume_agents: i64,
    pub max_parallel_agents: i64,
    pub required_consecutive_clean_fresh_runs: i64,
    pub required_consecutive_clean_resume_runs: i64,
    pub stop_on_severity: String,
    pub allow_resume_review: bool,
    pub allow_fresh_review: bool,
    pub allow_new_findings_in_resume: bool,
    pub on_max_agents_exceeded: String,
    pub run_count_scope: String,
    pub default_run_mode: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReviewPlanRecord {
    pub id: i64,
    pub work_unit_id: i64,
    pub design_version_id: Option<i64>,
    pub review_type: String,
    pub required: bool,
    pub stage: String,
    pub scope: Option<String>,
    pub review_policy_id: Option<i64>,
    pub review_scope_id: Option<i64>,
    pub status: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReviewPlanTargetRecord {
    pub id: i64,
    pub review_plan_id: i64,
    pub target_type: String,
    pub design_version_id: Option<i64>,
    pub design_requirement_id: Option<i64>,
    pub task_id: Option<i64>,
    pub work_unit_id: Option<i64>,
    pub phase_id: Option<i64>,
    pub repository_snapshot_id: Option<i64>,
    pub file_path: Option<String>,
    pub symbol: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ReviewRunRecord {
    pub id: i64,
    pub review_plan_id: Option<i64>,
    pub run_type: String,
    pub run_purpose: String,
    pub target_type: String,
    pub target_ref: Option<String>,
    pub new_findings_count: i64,
    pub carried_findings_checked: i64,
    pub clean_run: bool,
    pub status: String,
    pub review_provenance: String,
    pub review_provenance_ref: Option<String>,
    pub finding_fix_result: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct FindingRecord {
    pub id: i64,
    pub review_run_id: i64,
    pub finding_type: String,
    pub severity: String,
    pub description: String,
    pub classification: String,
    pub status: String,
}
