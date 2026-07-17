use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub(crate) enum TaskCommand {
    Add(TaskAddArgs),
    List(TaskListArgs),
    Close(TaskCloseArgs),
    AcceptOutOfScope(TaskAcceptOutOfScopeArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum PhaseCommand {
    Create(PhaseCreateArgs),
    List(PhaseListArgs),
    Show(PhaseShowArgs),
    Assign(PhaseAssignArgs),
    Dependency {
        #[command(subcommand)]
        command: PhaseDependencyCommand,
    },
    Trace {
        #[command(subcommand)]
        command: PhaseTraceCommand,
    },
    Inventory(PhaseInventoryArgs),
    Rescope(PhaseRescopeArgs),
    Split(PhaseSplitArgs),
    CloseReady(PhaseCloseReadyArgs),
    Close(PhaseCloseArgs),
    AcceptOutOfScope(PhaseAcceptOutOfScopeArgs),
}

#[derive(Debug, Args)]
pub(crate) struct PhaseCreateArgs {
    #[arg(long)]
    pub(crate) work_unit: i64,
    #[arg(long)]
    pub(crate) design_version: Option<i64>,
    #[arg(long)]
    pub(crate) key: String,
    #[arg(long)]
    pub(crate) title: String,
    #[arg(long, default_value = "milestone")]
    pub(crate) kind: String,
    #[arg(long = "order")]
    pub(crate) order: i64,
    #[arg(long)]
    pub(crate) reason: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct PhaseListArgs {
    #[arg(long)]
    pub(crate) work_unit: i64,
}

#[derive(Debug, Args)]
pub(crate) struct PhaseShowArgs {
    pub(crate) phase_id: i64,
}

#[derive(Debug, Args)]
pub(crate) struct PhaseAssignArgs {
    pub(crate) phase_id: i64,
    #[arg(long)]
    pub(crate) task: i64,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PhaseDependencyCommand {
    Add(PhaseDependencyAddArgs),
    List(PhaseDependencyListArgs),
    Satisfy(PhaseDependencySatisfyArgs),
    Accept(PhaseDependencyAcceptArgs),
}

#[derive(Debug, Args)]
pub(crate) struct PhaseDependencyAddArgs {
    #[arg(long = "from")]
    pub(crate) from_phase: i64,
    #[arg(long = "to")]
    pub(crate) to_phase: i64,
    #[arg(long = "type")]
    pub(crate) dependency_type: String,
    #[arg(long)]
    pub(crate) reason: String,
}

#[derive(Debug, Args)]
pub(crate) struct PhaseDependencyListArgs {
    #[arg(long)]
    pub(crate) work_unit: i64,
}

#[derive(Debug, Args)]
pub(crate) struct PhaseDependencySatisfyArgs {
    pub(crate) dependency_id: i64,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) evidence: String,
}

#[derive(Debug, Args)]
pub(crate) struct PhaseDependencyAcceptArgs {
    pub(crate) dependency_id: i64,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) authority: i64,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PhaseTraceCommand {
    List(PhaseTraceListArgs),
    Decide(PhaseTraceDecideArgs),
}

#[derive(Debug, Args)]
pub(crate) struct PhaseTraceListArgs {
    pub(crate) phase_id: i64,
}

#[derive(Debug, Args)]
pub(crate) struct PhaseTraceDecideArgs {
    #[arg(long)]
    pub(crate) phase: i64,
    #[arg(long)]
    pub(crate) record: String,
    #[arg(long)]
    pub(crate) decision: String,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) authority: i64,
}

#[derive(Debug, Args)]
pub(crate) struct PhaseInventoryArgs {
    pub(crate) phase_id: i64,
}

#[derive(Debug, Args)]
pub(crate) struct PhaseRescopeArgs {
    #[arg(long)]
    pub(crate) phase: i64,
    #[arg(long)]
    pub(crate) to_work_unit: i64,
    #[arg(long, default_value = "require-decisions")]
    pub(crate) shared_record_policy: String,
    #[arg(long)]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PhaseSplitArgs {
    pub(crate) phase_id: i64,
    #[arg(long)]
    pub(crate) title: String,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long, default_value = "require-decisions")]
    pub(crate) shared_record_policy: String,
    #[arg(long)]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PhaseCloseReadyArgs {
    pub(crate) phase_id: i64,
    #[arg(long)]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Args)]
pub(crate) struct PhaseCloseArgs {
    pub(crate) phase_id: i64,
    #[arg(long)]
    pub(crate) summary: String,
}

#[derive(Debug, Args)]
pub(crate) struct PhaseAcceptOutOfScopeArgs {
    pub(crate) phase_id: i64,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) authority: i64,
}

#[derive(Debug, Args)]
pub(crate) struct TaskAddArgs {
    pub(crate) title: String,
    #[arg(long, default_value = "medium")]
    pub(crate) priority: String,
    #[arg(long, default_value = "user")]
    pub(crate) source: String,
    #[arg(long)]
    pub(crate) work_unit: Option<i64>,
    #[arg(long)]
    pub(crate) details: Option<String>,
    #[arg(long)]
    pub(crate) completion_condition: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct TaskListArgs {
    #[arg(long)]
    pub(crate) status: Option<String>,
    #[arg(long)]
    pub(crate) work_unit: Option<i64>,
}

#[derive(Debug, Args)]
pub(crate) struct TaskCloseArgs {
    pub(crate) task_id: i64,
    #[arg(long)]
    pub(crate) commit: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct TaskAcceptOutOfScopeArgs {
    pub(crate) task_id: i64,
    #[arg(long)]
    pub(crate) reason: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum DecisionCommand {
    Add(DecisionAddArgs),
    List(DecisionListArgs),
    Search(DecisionSearchArgs),
    Capability {
        #[command(subcommand)]
        command: DecisionCapabilityCommand,
    },
    Adjudicate(DecisionAdjudicateArgs),
    Continuation {
        #[command(subcommand)]
        command: DecisionContinuationCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum DecisionContinuationCommand {
    Show(DecisionContinuationShowArgs),
    Apply(DecisionContinuationApplyArgs),
}

#[derive(Debug, Args)]
pub(crate) struct DecisionContinuationShowArgs {
    pub(crate) handle: String,
}

#[derive(Debug, Args)]
pub(crate) struct DecisionContinuationApplyArgs {
    pub(crate) handle: String,
    #[arg(long)]
    pub(crate) decision: String,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) principal: String,
    #[arg(long)]
    pub(crate) capability: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum DecisionCapabilityCommand {
    Issue(DecisionCapabilityIssueArgs),
}

#[derive(Debug, Args)]
pub(crate) struct DecisionCapabilityIssueArgs {
    #[arg(long)]
    pub(crate) principal: String,
    #[arg(long)]
    pub(crate) owner: String,
    #[arg(long)]
    pub(crate) target: String,
    #[arg(long)]
    pub(crate) role: String,
    #[arg(long)]
    pub(crate) decision_family: String,
    #[arg(long)]
    pub(crate) action: String,
    #[arg(long)]
    pub(crate) design_context: String,
    #[arg(long)]
    pub(crate) expires: String,
    #[arg(long)]
    pub(crate) issuer_assertion: String,
    #[arg(long)]
    pub(crate) owner_grant: String,
}

#[derive(Debug, Args)]
pub(crate) struct DecisionAdjudicateArgs {
    #[arg(long)]
    pub(crate) principal: String,
    #[arg(long)]
    pub(crate) capability: String,
    #[arg(long)]
    pub(crate) owner: String,
    #[arg(long)]
    pub(crate) target: String,
    #[arg(long)]
    pub(crate) decision_family: String,
    #[arg(long)]
    pub(crate) action: String,
    #[arg(long)]
    pub(crate) decision: String,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) expected_current: String,
}

#[derive(Debug, Args)]
pub(crate) struct DecisionAddArgs {
    #[arg(long)]
    pub(crate) topic: String,
    #[arg(long)]
    pub(crate) decision: String,
    #[arg(long)]
    pub(crate) key: Option<String>,
    #[arg(long)]
    pub(crate) rationale: Option<String>,
    #[arg(long)]
    pub(crate) compatibility_impact: Option<String>,
    #[arg(long)]
    pub(crate) authority_refs: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct DecisionListArgs {
    #[arg(long)]
    pub(crate) query: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct DecisionSearchArgs {
    pub(crate) query: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum DesignCommand {
    Init(DesignInitArgs),
    Import(DesignImportArgs),
    Refresh(DesignImportArgs),
    Approve(DesignApproveArgs),
}

#[derive(Debug, Args)]
pub(crate) struct DesignInitArgs {
    pub(crate) design_id: String,
    #[arg(long)]
    pub(crate) title: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct DesignImportArgs {
    pub(crate) package_path: PathBuf,
    #[arg(long, default_value = "draft")]
    pub(crate) status: String,
}

#[derive(Debug, Args)]
pub(crate) struct DesignApproveArgs {
    pub(crate) design_version_id: i64,
    #[arg(long)]
    pub(crate) summary: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum RequirementCommand {
    List(RequirementListArgs),
}

#[derive(Debug, Args)]
pub(crate) struct RequirementListArgs {
    #[arg(long)]
    pub(crate) design: i64,
}

#[derive(Debug, Subcommand)]
pub(crate) enum DesignDecisionCommand {
    List(DesignDecisionListArgs),
}

#[derive(Debug, Args)]
pub(crate) struct DesignDecisionListArgs {
    #[arg(long)]
    pub(crate) design: i64,
}

#[derive(Debug, Subcommand)]
pub(crate) enum GateTemplateCommand {
    List(GateTemplateListArgs),
}

#[derive(Debug, Args)]
pub(crate) struct GateTemplateListArgs {
    #[arg(long)]
    pub(crate) design: i64,
}

#[derive(Debug, Subcommand)]
pub(crate) enum TraceCommand {
    DeriveTask(TraceDeriveTaskArgs),
    Derivation {
        #[command(subcommand)]
        command: TraceDerivationCommand,
    },
}

#[derive(Debug, Args)]
pub(crate) struct TraceDeriveTaskArgs {
    #[arg(long)]
    pub(crate) design: i64,
    #[arg(long)]
    pub(crate) requirement: String,
    #[arg(long)]
    pub(crate) task: i64,
    #[arg(long)]
    pub(crate) reason: Option<String>,
    #[arg(long)]
    pub(crate) checklist_title: Option<String>,
    #[arg(long)]
    pub(crate) item_title: Option<String>,
    #[arg(long)]
    pub(crate) completion_condition: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum TraceDerivationCommand {
    List(TraceDerivationListArgs),
}

#[derive(Debug, Args)]
pub(crate) struct TraceDerivationListArgs {
    #[arg(long)]
    pub(crate) design: i64,
}

#[derive(Debug, Subcommand)]
pub(crate) enum EvidenceCommand {
    Add(Box<EvidenceAddArgs>),
    List(EvidenceListArgs),
}

#[derive(Debug, Args)]
pub(crate) struct EvidenceAddArgs {
    #[arg(long)]
    pub(crate) task: Option<i64>,
    #[arg(long)]
    pub(crate) design: Option<i64>,
    #[arg(long)]
    pub(crate) requirement: Option<String>,
    #[arg(long = "type")]
    pub(crate) evidence_type: String,
    #[arg(long)]
    pub(crate) repository_id: Option<i64>,
    #[arg(long)]
    pub(crate) git_commit_id: Option<i64>,
    #[arg(long)]
    pub(crate) git_file_change_id: Option<i64>,
    #[arg(long)]
    pub(crate) commit: Option<String>,
    #[arg(long)]
    pub(crate) file: Option<String>,
    #[arg(long)]
    pub(crate) line: Option<String>,
    #[arg(long)]
    pub(crate) symbol: Option<String>,
    #[arg(long)]
    pub(crate) artifact: Option<String>,
    #[arg(long)]
    pub(crate) note: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct EvidenceListArgs {
    #[arg(long)]
    pub(crate) task: Option<i64>,
    #[arg(long)]
    pub(crate) design: Option<i64>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum CoverageCommand {
    Add(CoverageAddArgs),
    List(CoverageListArgs),
}

#[derive(Debug, Args)]
pub(crate) struct CoverageAddArgs {
    #[arg(long)]
    pub(crate) design: i64,
    #[arg(long)]
    pub(crate) requirement: String,
    #[arg(long)]
    pub(crate) task: Option<i64>,
    #[arg(long)]
    pub(crate) work_unit: Option<i64>,
    #[arg(long)]
    pub(crate) status: String,
    #[arg(long)]
    pub(crate) requirement_text: String,
    #[arg(long)]
    pub(crate) runtime: Option<String>,
    #[arg(long)]
    pub(crate) ux: Option<String>,
    #[arg(long)]
    pub(crate) lifecycle: Option<String>,
    #[arg(long)]
    pub(crate) tests_or_gates: Option<String>,
    #[arg(long)]
    pub(crate) missing: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct CoverageListArgs {
    #[arg(long)]
    pub(crate) design: i64,
    #[arg(long)]
    pub(crate) status: Option<String>,
}
