use std::path::PathBuf;

use clap::{Args, Subcommand, ValueEnum};

#[derive(Debug, Subcommand)]
pub(crate) enum ReviewCommand {
    Adjudicate(ReviewAdjudicateArgs),
    Correction {
        #[command(subcommand)]
        command: ReviewCorrectionCommand,
    },
    Provenance {
        #[command(subcommand)]
        command: ReviewProvenanceCommand,
    },
    Invocation {
        #[command(subcommand)]
        command: ReviewInvocationCommand,
    },
    Result {
        #[command(subcommand)]
        command: ReviewResultCommand,
    },
    Scope {
        #[command(subcommand)]
        command: ReviewScopeCommand,
    },
    Policy {
        #[command(subcommand)]
        command: ReviewPolicyCommand,
    },
    Plan {
        #[command(subcommand)]
        command: ReviewPlanCommand,
    },
    Run {
        #[command(subcommand)]
        command: ReviewRunCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum ReviewProvenanceCommand {
    Issue(ReviewProvenanceIssueArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ReviewProvenanceIssueArgs {
    #[arg(long)]
    pub(crate) reviewer: String,
    #[arg(long)]
    pub(crate) plan: i64,
    #[arg(long)]
    pub(crate) target: String,
    #[arg(long, default_value = "external_agent")]
    pub(crate) kind: String,
    #[arg(long)]
    pub(crate) purpose: String,
    #[arg(long)]
    pub(crate) reference: String,
    #[arg(long)]
    pub(crate) idempotency_key: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ReviewInvocationCommand {
    Request(ReviewInvocationRequestArgs),
    Start(ReviewInvocationStartArgs),
    Complete(ReviewInvocationCompleteArgs),
    Fail(ReviewInvocationEndArgs),
    Cancel(ReviewInvocationEndArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ReviewInvocationRequestArgs {
    #[arg(long)]
    pub(crate) plan: i64,
    #[arg(long)]
    pub(crate) target: String,
    #[arg(long)]
    pub(crate) reviewer: String,
    #[arg(long)]
    pub(crate) provenance: String,
    #[arg(long)]
    pub(crate) purpose: String,
    #[arg(long)]
    pub(crate) idempotency_key: String,
    #[arg(long, default_value = "open")]
    pub(crate) expected_plan_current: String,
}

#[derive(Debug, Args)]
pub(crate) struct ReviewInvocationStartArgs {
    pub(crate) invocation_id: i64,
    #[arg(long)]
    pub(crate) expected_current: String,
    #[arg(long)]
    pub(crate) idempotency_key: String,
}

#[derive(Debug, Args)]
pub(crate) struct ReviewInvocationCompleteArgs {
    pub(crate) invocation_id: i64,
    #[arg(long)]
    pub(crate) claim: Option<String>,
    #[arg(long)]
    pub(crate) verification_claim: Option<String>,
    #[arg(long)]
    pub(crate) attempt: Option<i64>,
    #[arg(long)]
    pub(crate) summary: String,
    #[arg(long)]
    pub(crate) expected_current: String,
    #[arg(long)]
    pub(crate) idempotency_key: String,
}

#[derive(Debug, Args)]
pub(crate) struct ReviewInvocationEndArgs {
    pub(crate) invocation_id: i64,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) expected_current: String,
    #[arg(long)]
    pub(crate) idempotency_key: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ReviewResultCommand {
    Stage(ReviewResultStageArgs),
    FindingAdd(ReviewResultFindingAddArgs),
    Complete(ReviewResultCompleteArgs),
    Cancel(ReviewResultCancelArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ReviewResultStageArgs {
    pub(crate) invocation_id: i64,
    #[arg(long)]
    pub(crate) expected_current: String,
    #[arg(long)]
    pub(crate) idempotency_key: String,
}

#[derive(Debug, Args)]
pub(crate) struct ReviewResultFindingAddArgs {
    pub(crate) stage_handle: String,
    #[arg(long = "type")]
    pub(crate) finding_type: String,
    #[arg(long)]
    pub(crate) severity: String,
    #[arg(long)]
    pub(crate) description: String,
    #[arg(long)]
    pub(crate) requirement: Option<i64>,
    #[arg(long)]
    pub(crate) task: Option<i64>,
    #[arg(long)]
    pub(crate) expected_current: String,
    #[arg(long)]
    pub(crate) idempotency_key: String,
}

#[derive(Debug, Args)]
pub(crate) struct ReviewResultCompleteArgs {
    pub(crate) stage_handle: String,
    #[arg(long)]
    pub(crate) expected_findings: i64,
    #[arg(long)]
    pub(crate) summary: String,
    #[arg(long)]
    pub(crate) expected_current: String,
    #[arg(long)]
    pub(crate) invocation_current: String,
    #[arg(long)]
    pub(crate) idempotency_key: String,
}

#[derive(Debug, Args)]
pub(crate) struct ReviewResultCancelArgs {
    pub(crate) stage_handle: String,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) expected_current: String,
    #[arg(long)]
    pub(crate) idempotency_key: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ReviewCorrectionCommand {
    Add(ReviewCorrectionAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ReviewCorrectionAddArgs {
    #[arg(long)]
    pub(crate) decision: String,
    #[arg(long)]
    pub(crate) boundary: String,
    #[arg(long)]
    pub(crate) outcome: String,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) expected_boundary_current: String,
}

#[derive(Debug, Args)]
pub(crate) struct ReviewAdjudicateArgs {
    pub(crate) run_id: i64,
    #[arg(long)]
    pub(crate) decision: String,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) expected_current: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ReviewScopeCommand {
    Start(ReviewScopeStartArgs),
    List,
}

#[derive(Debug, Args)]
pub(crate) struct ReviewScopeStartArgs {
    pub(crate) name: String,
    #[arg(long = "type", default_value = "general")]
    pub(crate) review_type: String,
    #[arg(long)]
    pub(crate) scope: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ReviewPolicyCommand {
    Add(ReviewPolicyAddArgs),
    List,
}

#[derive(Debug, Args)]
pub(crate) struct ReviewPolicyAddArgs {
    #[arg(long)]
    pub(crate) name: String,
    #[arg(long = "type")]
    pub(crate) review_type: String,
    #[arg(long, default_value_t = 1)]
    pub(crate) fresh_clean: i64,
    #[arg(long, default_value_t = 0)]
    pub(crate) resume_clean: i64,
    #[arg(long, default_value_t = 1)]
    pub(crate) max_fresh_agents: i64,
    #[arg(long, default_value_t = 1)]
    pub(crate) max_resume_agents: i64,
    #[arg(long, default_value_t = 1)]
    pub(crate) max_parallel_agents: i64,
    #[arg(long, default_value = "none")]
    pub(crate) stop_on_severity: String,
    #[arg(long, default_value_t = false)]
    pub(crate) allow_new_findings_in_resume: bool,
    #[arg(long, default_value = "review_plan")]
    pub(crate) run_count_scope: String,
    #[arg(long, default_value = "fresh")]
    pub(crate) default_run_mode: String,
    #[arg(long, default_value = "block")]
    pub(crate) on_max_agents_exceeded: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ReviewPlanCommand {
    Add(ReviewPlanAddArgs),
    List,
    Context(ReviewPlanContextArgs),
    /// Record an approved exception for a required review plan.
    Waive(ReviewPlanWaiveArgs),
    /// Replace an obsolete required plan with a compatible successor.
    Supersede(ReviewPlanSupersedeArgs),
    Target {
        #[command(subcommand)]
        command: ReviewPlanTargetCommand,
    },
}

#[derive(Debug, Args)]
pub(crate) struct ReviewPlanAddArgs {
    #[arg(long)]
    pub(crate) work_unit: i64,
    #[arg(long = "type")]
    pub(crate) review_type: String,
    #[arg(long)]
    pub(crate) stage: String,
    #[arg(long)]
    pub(crate) design_version: Option<i64>,
    #[arg(long)]
    pub(crate) scope: Option<String>,
    #[arg(long)]
    pub(crate) policy: Option<i64>,
    #[arg(long)]
    pub(crate) review_scope: Option<i64>,
    #[arg(long)]
    pub(crate) required: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ReviewPlanContextArgs {
    pub(crate) review_plan_id: i64,
}

#[derive(Debug, Args)]
pub(crate) struct ReviewPlanWaiveArgs {
    pub(crate) review_plan_id: i64,
    #[arg(long)]
    pub(crate) reason: String,
}

#[derive(Debug, Args)]
pub(crate) struct ReviewPlanSupersedeArgs {
    pub(crate) review_plan_id: i64,
    #[arg(long = "by")]
    pub(crate) successor_plan_id: i64,
    #[arg(long)]
    pub(crate) authority: i64,
    #[arg(long)]
    pub(crate) reason: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ReviewPlanTargetCommand {
    Add(ReviewPlanTargetAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ReviewPlanTargetAddArgs {
    #[arg(long)]
    pub(crate) plan: i64,
    #[arg(long = "type")]
    pub(crate) target_type: String,
    #[arg(long)]
    pub(crate) design_version: Option<i64>,
    #[arg(long)]
    pub(crate) design_requirement: Option<i64>,
    #[arg(long)]
    pub(crate) task: Option<i64>,
    #[arg(long)]
    pub(crate) work_unit: Option<i64>,
    #[arg(long)]
    pub(crate) phase: Option<i64>,
    #[arg(long)]
    pub(crate) repository_snapshot: Option<i64>,
    #[arg(long)]
    pub(crate) file: Option<String>,
    #[arg(long)]
    pub(crate) symbol: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ReviewRunCommand {
    Add(Box<ReviewRunAddArgs>),
    List(ReviewRunListArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ReviewRunAddArgs {
    #[arg(long)]
    pub(crate) plan: i64,
    #[arg(long = "type")]
    pub(crate) run_type: String,
    #[arg(long)]
    pub(crate) purpose: String,
    #[arg(long)]
    pub(crate) target: Option<String>,
    #[arg(long, default_value = "completed")]
    pub(crate) status: String,
    #[arg(long)]
    pub(crate) clean: bool,
    #[arg(long, default_value_t = 0)]
    pub(crate) new_findings: i64,
    #[arg(long, default_value_t = 0)]
    pub(crate) carried_findings: i64,
    #[arg(long)]
    pub(crate) summary: Option<String>,
    #[arg(long)]
    pub(crate) agent_label: Option<String>,
    #[arg(long)]
    pub(crate) external_agent_id: Option<String>,
    #[arg(long, default_value = "self_recorded")]
    pub(crate) provenance: String,
    #[arg(long)]
    pub(crate) provenance_ref: Option<String>,
    #[arg(long)]
    pub(crate) finding_result: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct ReviewRunListArgs {
    #[arg(long)]
    pub(crate) plan: Option<i64>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum FindingCommand {
    Decide(FindingDecideArgs),
    Reopen(FindingReopenArgs),
    Recover(FindingRecoverArgs),
    Add(FindingAddArgs),
    Classify(FindingClassifyArgs),
    List(FindingListArgs),
    Verify(FindingVerifyArgs),
    /// Accept an open finding out of scope using recorded authority.
    AcceptOutOfScope(FindingAcceptOutOfScopeArgs),
}

#[derive(Debug, Args)]
pub(crate) struct FindingRecoverArgs {
    pub(crate) finding_id: i64,
    #[arg(long)]
    pub(crate) epoch: i64,
    #[arg(long)]
    pub(crate) evidence: String,
    #[arg(long)]
    pub(crate) authority: i64,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) package_current: String,
    #[arg(long)]
    pub(crate) expected_current: String,
    #[arg(long)]
    pub(crate) idempotency_key: String,
}

#[derive(Debug, Args)]
pub(crate) struct FindingReopenArgs {
    pub(crate) finding_id: i64,
    #[arg(long)]
    pub(crate) epoch: i64,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) expected_current: String,
}

#[derive(Debug, Args)]
pub(crate) struct FindingDecideArgs {
    pub(crate) finding_id: i64,
    #[arg(long)]
    pub(crate) decision: String,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) expected_current: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum VerificationCommand {
    Adjudicate(VerificationAdjudicateArgs),
}
#[derive(Debug, Args)]
pub(crate) struct VerificationAdjudicateArgs {
    #[arg(long)]
    pub(crate) run: i64,
    #[arg(long)]
    pub(crate) finding: i64,
    #[arg(long)]
    pub(crate) closure: i64,
    #[arg(long)]
    pub(crate) attempt: i64,
    #[arg(long)]
    pub(crate) decision: String,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) expected_current: String,
}

#[derive(Debug, Args)]
pub(crate) struct FindingAcceptOutOfScopeArgs {
    pub(crate) finding_id: i64,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) authority: i64,
}

#[derive(Debug, Args)]
pub(crate) struct FindingAddArgs {
    #[arg(long)]
    pub(crate) run: i64,
    #[arg(long = "type")]
    pub(crate) finding_type: String,
    #[arg(long)]
    pub(crate) severity: String,
    #[arg(long)]
    pub(crate) description: String,
    #[arg(long)]
    pub(crate) design_requirement: Option<i64>,
    #[arg(long)]
    pub(crate) task: Option<i64>,
}

#[derive(Debug, Args)]
pub(crate) struct FindingClassifyArgs {
    pub(crate) finding_id: i64,
    #[arg(long, conflicts_with = "decision")]
    pub(crate) classification: Option<String>,
    #[arg(long, value_parser = ["accepted", "rejected"], requires_all = ["reason", "expected_current"], conflicts_with = "classification")]
    pub(crate) decision: Option<String>,
    #[arg(long, requires = "decision")]
    pub(crate) reason: Option<String>,
    #[arg(long, requires = "decision")]
    pub(crate) expected_current: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct FindingListArgs {
    #[arg(long, value_parser = ["open", "closed"])]
    pub(crate) status: Option<String>,
    #[arg(long)]
    pub(crate) run: Option<i64>,
}

#[derive(Debug, Args)]
pub(crate) struct FindingVerifyArgs {
    #[arg(long)]
    pub(crate) run: i64,
    #[arg(long)]
    pub(crate) finding: i64,
    #[arg(long)]
    pub(crate) closure: i64,
    #[arg(long)]
    pub(crate) attempt: Option<i64>,
    #[arg(long)]
    pub(crate) result: String,
    #[arg(long)]
    pub(crate) notes: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ClosureCommand {
    Add(ClosureAddArgs),
    CorrectionBegin(ClosureCorrectionBeginArgs),
    Transition {
        #[command(subcommand)]
        command: ClosureTransitionCommand,
    },
    Ready(ClosureReadyArgs),
    Supersede(ClosureSupersedeArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum ClosureTransitionCommand {
    Apply(ClosureTransitionApplyArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ClosureTransitionApplyArgs {
    pub(crate) closure_id: i64,
    #[arg(long)]
    pub(crate) token: i64,
    #[arg(long)]
    pub(crate) authority: Option<i64>,
    #[arg(long)]
    pub(crate) evidence: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct ClosureCorrectionBeginArgs {
    pub(crate) closure_id: i64,
}

#[derive(Debug, Args)]
pub(crate) struct ClosureReadyArgs {
    pub(crate) closure_id: i64,
    #[arg(long)]
    pub(crate) evidence: String,
    #[arg(long)]
    pub(crate) tests: String,
    #[arg(long)]
    pub(crate) commit: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct ClosureSupersedeArgs {
    pub(crate) closure_id: i64,
    #[arg(long)]
    pub(crate) invariant: String,
    #[arg(long)]
    pub(crate) surfaces: String,
    #[arg(long)]
    pub(crate) fix_plan: String,
    #[arg(long)]
    pub(crate) tests: String,
    #[arg(long)]
    pub(crate) verification: String,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) authority: i64,
    #[arg(long)]
    pub(crate) citations: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct ClosureAddArgs {
    #[arg(long)]
    pub(crate) finding: i64,
    #[arg(long)]
    pub(crate) invariant: String,
    #[arg(long)]
    pub(crate) citations: Option<String>,
    #[arg(long)]
    pub(crate) evidence: Option<String>,
    #[arg(long)]
    pub(crate) surfaces: Option<String>,
    #[arg(long)]
    pub(crate) search: Option<String>,
    #[arg(long)]
    pub(crate) other_violations: Option<String>,
    #[arg(long)]
    pub(crate) fix_plan: Option<String>,
    #[arg(long)]
    pub(crate) tests: Option<String>,
    #[arg(long)]
    pub(crate) verification: Option<String>,
    #[arg(long)]
    pub(crate) commit: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum AcceptanceCommand {
    Add(AcceptanceAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct AcceptanceAddArgs {
    #[arg(long)]
    pub(crate) design: Option<i64>,
    #[arg(long)]
    pub(crate) package: Option<String>,
    #[arg(long)]
    pub(crate) target: String,
    #[arg(long = "type")]
    pub(crate) acceptance_type: String,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) authority: i64,
}

#[derive(Debug, Subcommand)]
pub(crate) enum KptCommand {
    Start(KptStartArgs),
    List(KptListArgs),
    Close(KptCloseArgs),
    Item {
        #[command(subcommand)]
        command: KptItemCommand,
    },
}

#[derive(Debug, Args)]
pub(crate) struct KptStartArgs {
    #[arg(long)]
    pub(crate) scope: Option<String>,
    #[arg(long)]
    pub(crate) summary: Option<String>,
    #[arg(long = "from")]
    pub(crate) from: Option<String>,
    #[arg(long)]
    pub(crate) period: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct KptListArgs {
    #[arg(long)]
    pub(crate) status: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct KptCloseArgs {
    pub(crate) kpt_review_id: i64,
}

#[derive(Debug, Subcommand)]
pub(crate) enum KptItemCommand {
    Add(KptItemAddArgs),
    List(KptItemListArgs),
    Convert(Box<KptItemConvertArgs>),
    Dismiss(KptItemDismissArgs),
}

#[derive(Debug, Args)]
pub(crate) struct KptItemDismissArgs {
    pub(crate) item: i64,
    #[arg(long)]
    pub(crate) authority: i64,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) expected_current: String,
}

#[derive(Debug, Args)]
pub(crate) struct KptItemAddArgs {
    #[arg(long = "type")]
    pub(crate) item_type: String,
    #[arg(long)]
    pub(crate) title: String,
    #[arg(long)]
    pub(crate) review: Option<i64>,
    #[arg(long)]
    pub(crate) details: Option<String>,
    #[arg(long, default_value = "medium")]
    pub(crate) severity: String,
    #[arg(long)]
    pub(crate) proposed_action: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct KptItemListArgs {
    #[arg(long)]
    pub(crate) review: Option<i64>,
}

#[derive(Debug, Args)]
pub(crate) struct KptItemConvertArgs {
    #[arg(long)]
    pub(crate) item: Option<i64>,
    #[arg(long = "to", value_enum)]
    pub(crate) target_type: KptConversionTargetArg,
    #[arg(long)]
    pub(crate) title: Option<String>,
    #[arg(long)]
    pub(crate) details: Option<String>,
    #[arg(long)]
    pub(crate) priority: Option<String>,
    #[arg(long)]
    pub(crate) work_unit: Option<i64>,
    #[arg(long)]
    pub(crate) name: Option<String>,
    #[arg(long)]
    pub(crate) command: Option<String>,
    #[arg(long)]
    pub(crate) command_type: Option<String>,
    #[arg(long)]
    pub(crate) scope: Option<String>,
    #[arg(long)]
    pub(crate) command_status: Option<String>,
    #[arg(long)]
    pub(crate) stability: Option<String>,
    #[arg(long)]
    pub(crate) timeout: Option<String>,
    #[arg(long)]
    pub(crate) expected_result: Option<String>,
    #[arg(long)]
    pub(crate) authority: Option<i64>,
    #[arg(long = "review-type")]
    pub(crate) review_type: Option<String>,
    #[arg(long)]
    pub(crate) fresh_clean: Option<i64>,
    #[arg(long)]
    pub(crate) resume_clean: Option<i64>,
    #[arg(long)]
    pub(crate) max_fresh_agents: Option<i64>,
    #[arg(long)]
    pub(crate) max_resume_agents: Option<i64>,
    #[arg(long)]
    pub(crate) max_parallel_agents: Option<i64>,
    #[arg(long)]
    pub(crate) stop_on_severity: Option<String>,
    #[arg(long, default_value_t = false)]
    pub(crate) allow_new_findings_in_resume: bool,
    #[arg(long)]
    pub(crate) run_count_scope: Option<String>,
    #[arg(long)]
    pub(crate) default_run_mode: Option<String>,
    #[arg(long)]
    pub(crate) on_max_agents_exceeded: Option<String>,
    #[arg(long)]
    pub(crate) decision_key: Option<String>,
    #[arg(long)]
    pub(crate) rationale: Option<String>,
    #[arg(long)]
    pub(crate) compatibility_impact: Option<String>,
    #[arg(long)]
    pub(crate) authority_refs: Option<String>,
    #[arg(long)]
    pub(crate) design_version: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum KptConversionTargetArg {
    Rule,
    Correction,
    Task,
    #[value(alias = "command_profile")]
    CommandProfile,
    #[value(alias = "review_policy")]
    ReviewPolicy,
    Decision,
    #[value(alias = "design_version")]
    DesignVersion,
}

impl KptConversionTargetArg {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::Correction => "correction",
            Self::Task => "task",
            Self::CommandProfile => "command-profile",
            Self::ReviewPolicy => "review-policy",
            Self::Decision => "decision",
            Self::DesignVersion => "design-version",
        }
    }
}

#[derive(Debug, Subcommand)]
pub(crate) enum DecomposeCommand {
    Design(DecomposeDesignArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum DecompositionCommand {
    /// Register the first immutable plan revision for a design-package/work slot.
    Import(DecompositionImportArgs),
    /// Revalidate one exact draft or incomplete plan revision.
    Validate(DecompositionValidateArgs),
    /// Publish a project-owned document-backed successor revision without replacing an applied Plan.
    Revise(DecompositionReviseArgs),
    Apply(DecompositionApplyArgs),
    Show(DecompositionShowArgs),
    Reconcile(DecompositionReconcileArgs),
}

#[derive(Debug, Args)]
pub(crate) struct DecompositionImportArgs {
    #[arg(long = "design-version", alias = "design")]
    pub(crate) design_version_id: i64,
    #[arg(long = "work", alias = "work-unit")]
    pub(crate) work_unit_id: i64,
    #[arg(long)]
    pub(crate) plan: Option<PathBuf>,
    #[arg(long)]
    pub(crate) expected_content: Option<String>,
    #[arg(long)]
    pub(crate) draft: bool,
    #[arg(long)]
    pub(crate) expected_current: String,
    #[arg(long)]
    pub(crate) idempotency_key: String,
}

#[derive(Debug, Args)]
pub(crate) struct DecompositionValidateArgs {
    pub(crate) plan_id: i64,
    #[arg(long)]
    pub(crate) expected_current: String,
    #[arg(long)]
    pub(crate) idempotency_key: String,
}

#[derive(Debug, Args)]
pub(crate) struct DecompositionReviseArgs {
    pub(crate) plan_id: i64,
    #[arg(long)]
    pub(crate) plan: Option<PathBuf>,
    #[arg(long)]
    pub(crate) expected_content: Option<String>,
    #[arg(long)]
    pub(crate) draft: bool,
    #[arg(long)]
    pub(crate) expected_current: String,
    #[arg(long)]
    pub(crate) idempotency_key: String,
}

#[derive(Debug, Args)]
pub(crate) struct DecompositionApplyArgs {
    pub(crate) design_version_id: i64,
    #[arg(long = "work", alias = "work-unit")]
    pub(crate) work_unit_id: i64,
    #[arg(long)]
    pub(crate) plan: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub(crate) struct DecompositionShowArgs {
    #[arg(long = "design-version", alias = "design")]
    pub(crate) design_version_id: i64,
    #[arg(long = "work", alias = "work-unit")]
    pub(crate) work_unit_id: i64,
}

#[derive(Debug, Args)]
pub(crate) struct DecompositionReconcileArgs {
    pub(crate) design_version_id: i64,
    #[arg(long = "work", alias = "work-unit")]
    pub(crate) work_unit_id: i64,
    #[arg(long)]
    pub(crate) plan: PathBuf,
    #[arg(long)]
    pub(crate) closure: i64,
    #[arg(long)]
    pub(crate) expected_current: String,
    #[arg(long)]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Args)]
pub(crate) struct DecomposeDesignArgs {
    pub(crate) design_version_id: i64,
    #[arg(long = "work", visible_alias = "work-unit")]
    pub(crate) work_unit: i64,
    #[arg(long)]
    pub(crate) checklist_title: Option<String>,
    #[arg(long)]
    pub(crate) reason: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ChecklistCommand {
    List(ChecklistListArgs),
    Close(ChecklistCloseArgs),
    Item {
        #[command(subcommand)]
        command: ChecklistItemCommand,
    },
}

#[derive(Debug, Args)]
pub(crate) struct ChecklistListArgs {
    #[arg(long, value_parser = ["active", "stale", "closed"])]
    pub(crate) status: Option<String>,
    #[arg(long)]
    pub(crate) work: Option<i64>,
}

#[derive(Debug, Args)]
pub(crate) struct ChecklistCloseArgs {
    pub(crate) checklist_id: i64,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ChecklistItemCommand {
    List(ChecklistItemListArgs),
    Close(ChecklistItemCloseArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ChecklistItemListArgs {
    #[arg(value_name = "CHECKLIST", conflicts_with = "checklist")]
    pub(crate) checklist_positional: Option<i64>,
    #[arg(long)]
    pub(crate) checklist: Option<i64>,
    #[arg(long, value_parser = ["open", "blocked", "closed", "accepted_out_of_scope"])]
    pub(crate) status: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct ChecklistItemCloseArgs {
    pub(crate) checklist_item_id: i64,
}

#[derive(Debug, Subcommand)]
pub(crate) enum StaleCommand {
    /// List stale design-derived records.
    List(StaleListArgs),
    /// Accept a stale record without changing its underlying status.
    Accept(StaleRecordDispositionArgs),
    /// Close a stale record that has a closed lifecycle state.
    Close(StaleRecordDispositionArgs),
}

#[derive(Debug, Args)]
pub(crate) struct StaleListArgs {
    #[arg(long = "type", value_parser = [
        "task_derivation",
        "checklist",
        "validation_gate",
        "coverage_item",
        "review_plan"
    ])]
    pub(crate) record_type: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct StaleRecordDispositionArgs {
    pub(crate) record_type: String,
    pub(crate) record_id: i64,
    #[arg(long)]
    pub(crate) reason: String,
}

#[derive(Debug, Args)]
pub(crate) struct ReviewContextArgs {
    pub(crate) kind: String,
    #[arg(long)]
    pub(crate) design_version: Option<String>,
    #[arg(long)]
    pub(crate) work_unit: Option<i64>,
    #[arg(long)]
    pub(crate) phase: Option<i64>,
    #[arg(long)]
    pub(crate) finding: Option<i64>,
    #[arg(long)]
    pub(crate) closure: Option<i64>,
    #[arg(long)]
    pub(crate) attempt: Option<i64>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ExportCommand {
    Design(ExportDesignArgs),
    Plan(ExportPlanArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ExportDesignArgs {
    #[arg(value_name = "DESIGN_VERSION", conflicts_with = "design")]
    pub(crate) design_positional: Option<i64>,
    #[arg(long, conflicts_with = "design_positional")]
    pub(crate) design: Option<i64>,
    #[arg(long, value_parser = ["public", "project-internal", "sensitive"])]
    pub(crate) classification: Option<String>,
    #[arg(long)]
    pub(crate) output: PathBuf,
}

#[derive(Debug, Args)]
pub(crate) struct ExportPlanArgs {
    #[arg(value_name = "WORK", conflicts_with = "design")]
    pub(crate) work_positional: Option<i64>,
    #[arg(long, conflicts_with = "work_positional")]
    pub(crate) design: Option<i64>,
    #[arg(long, value_parser = ["public", "project-internal", "sensitive"])]
    pub(crate) classification: Option<String>,
    #[arg(long)]
    pub(crate) output: PathBuf,
}
