use std::env;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};

use agent_workbench::{
    CommandUsageListQuery, CoverageItemListQuery, DesignDecisionListQuery, DesignPackageImport,
    DesignReadyCheck, DesignRequirementListQuery, DesignVersionApproval,
    ImplementationEvidenceListQuery, ImplementationEvidenceRecord, ImplementationReadyCheck,
    KptItemCommandProfileConversion, KptItemDecisionConversion, KptItemDesignVersionConversion,
    KptItemReviewPolicyConversion, KptItemTaskConversion, NewAuthorityEvent, NewClosure,
    NewCommandDeviation, NewCommandProfile, NewCommandUsage, NewCommandUsageWithRepositorySnapshot,
    NewCoverageItem, NewDecision, NewDesignExceptionAcceptance, NewDesignPackage, NewFinding,
    NewFindingVerification, NewGitCommit, NewGitFileChange, NewImplementationEvidence,
    NewImplementationEvidenceWithGit, NewKptItem, NewKptReview, NewRepository,
    NewRepositoryDirtyEntry, NewRepositorySnapshot, NewRepositorySnapshotComparison,
    NewRepositoryStateClassification, NewReviewPlan, NewReviewPolicy, NewReviewRun, NewReviewScope,
    NewTask, NewTaskDerivation, NewUserCorrection, NewValidationRun, NewWorkFork, NewWorkRecord,
    NewWorkRecordCommand, NewWorkRecordCommit, NewWorkRecordFile, NewWorkRecordGitCommit,
    NewWorkRecordGitFile, NextAction, RuleQuery, TaskDerivationListQuery, TaskListQuery,
    ValidationGateSelection, ValidationGateTemplateListQuery, ValidationRunListQuery,
    WorkForkSource, accept_design_exception, accept_task_out_of_scope, add_authority_event,
    add_closure, add_command_deviation, add_command_usage,
    add_command_usage_with_repository_snapshot, add_coverage_item, add_decision, add_finding,
    add_finding_verification, add_fixed_command, add_git_commit, add_git_file_change,
    add_implementation_evidence, add_implementation_evidence_with_git, add_kpt_item,
    add_repository, add_repository_dirty_entry, add_repository_snapshot,
    add_repository_snapshot_comparison, add_repository_state_classification, add_review_plan,
    add_review_policy, add_review_run, add_task, add_user_correction, add_validation_run,
    add_work_record_command, add_work_record_commit, add_work_record_file,
    add_work_record_git_commit, add_work_record_git_file, applicable_rules, approve_design_version,
    classify_finding, close_active_work, close_kpt_review, close_ready, close_task,
    convert_kpt_item_to_command_profile, convert_kpt_item_to_decision,
    convert_kpt_item_to_design_version, convert_kpt_item_to_review_policy,
    convert_kpt_item_to_task, create_follow_up_work, create_work_record,
    derive_task_from_requirement, design_ready, export_work_record_markdown, fork_work,
    implementation_ready, import_design_package, init_design_package, init_project, interrupt_work,
    list_authority_events, list_command_profiles, list_command_usages, list_coverage_items,
    list_decisions, list_design_decisions, list_design_requirements, list_findings,
    list_implementation_evidence, list_kpt_items, list_kpt_reviews, list_repositories,
    list_repository_snapshots, list_review_plan_targets, list_review_plans, list_review_policies,
    list_review_runs, list_review_scopes, list_task_derivations, list_tasks, list_user_corrections,
    list_validation_gate_templates, list_validation_runs, list_work_records, next_action,
    project_status, reopen_work, resume_check, resume_ready, resume_work, select_validation_gate,
    start_kpt_review, start_review_scope, start_work, suspend_work,
};

#[derive(Debug, Parser)]
#[command(name = "agent-workbench")]
#[command(about = "Structured local workbench for long-running coding-agent work")]
struct Cli {
    #[arg(long, global = true, value_name = "PATH")]
    root: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
#[allow(clippy::enum_variant_names)]
enum Command {
    /// Initialize the project-local ledger.
    Init,
    /// Print project ledger status.
    Status,
    /// Print the next suggested action.
    Next,
    /// Manage work units and activation state.
    Work {
        #[command(subcommand)]
        command: WorkCommand,
    },
    /// Record a basic resume check for the latest suspended activation.
    ResumeCheck(ResumeCheckArgs),
    /// Run read-only gates.
    Gate {
        #[command(subcommand)]
        command: GateCommand,
    },
    /// Record or list user corrections.
    Correction {
        #[command(subcommand)]
        command: CorrectionCommand,
    },
    /// Record or list reusable project commands.
    Command {
        #[command(subcommand)]
        command: MemoryCommand,
    },
    /// Query applicable rules.
    Rules {
        #[command(subcommand)]
        command: RulesCommand,
    },
    /// Record and link structured work records.
    #[command(name = "record")]
    WorkRecord {
        #[command(subcommand)]
        command: WorkRecordCommand,
    },
    /// Record repository state, snapshots, and Git evidence.
    Repository {
        #[command(subcommand)]
        command: RepositoryCommand,
    },
    /// Manage task ledger entries.
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    /// Manage project decisions.
    Decision {
        #[command(subcommand)]
        command: DecisionCommand,
    },
    /// Manage design package drafts.
    Design {
        #[command(subcommand)]
        command: DesignCommand,
    },
    /// List design requirements imported from a design version.
    Requirement {
        #[command(subcommand)]
        command: RequirementCommand,
    },
    /// List design decisions imported from a design version.
    DesignDecision {
        #[command(subcommand)]
        command: DesignDecisionCommand,
    },
    /// List validation gate templates imported from a design version.
    GateTemplate {
        #[command(subcommand)]
        command: GateTemplateCommand,
    },
    /// Manage design traceability from requirements to work.
    Trace {
        #[command(subcommand)]
        command: TraceCommand,
    },
    /// Record or list implementation evidence.
    Evidence {
        #[command(subcommand)]
        command: EvidenceCommand,
    },
    /// Record or list design implementation coverage.
    Coverage {
        #[command(subcommand)]
        command: CoverageCommand,
    },
    /// Manage review scopes, policies, plans, and runs.
    Review {
        #[command(subcommand)]
        command: ReviewCommand,
    },
    /// Manage findings recorded from review runs.
    Finding {
        #[command(subcommand)]
        command: FindingCommand,
    },
    /// Manage finding closure records.
    Closure {
        #[command(subcommand)]
        command: ClosureCommand,
    },
    /// Accept explicit design exceptions.
    Acceptance {
        #[command(subcommand)]
        command: AcceptanceCommand,
    },
    /// Manage authority events.
    Authority {
        #[command(subcommand)]
        command: AuthorityCommand,
    },
    /// Manage KPT reviews.
    Kpt {
        #[command(subcommand)]
        command: KptCommand,
    },
}

#[derive(Debug, Subcommand)]
enum WorkCommand {
    /// Start a new active work unit.
    Start(WorkStartArgs),
    /// Suspend the active work unit.
    Suspend(WorkSuspendArgs),
    /// Interrupt active work with a child work unit.
    Interrupt(WorkInterruptArgs),
    /// Resume a suspended activation using an allowed resume check.
    Resume(WorkResumeArgs),
    /// Close the active work unit.
    Close(WorkCloseArgs),
    /// Fork work from a prior record, activation, or commit.
    Fork(WorkForkArgs),
    /// Reopen a closed or abandoned work unit.
    Reopen(WorkReopenArgs),
    /// Create follow-up work linked to a closed or abandoned work unit.
    FollowUp(WorkFollowUpArgs),
}

#[derive(Debug, Args)]
struct WorkStartArgs {
    title: String,
    #[arg(long)]
    responsibility: Option<String>,
}

#[derive(Debug, Args)]
struct WorkSuspendArgs {
    #[arg(long)]
    reason: String,
    #[arg(long)]
    next: String,
}

#[derive(Debug, Args)]
struct WorkInterruptArgs {
    title: String,
    #[arg(long)]
    reason: String,
}

#[derive(Debug, Args)]
struct WorkResumeArgs {
    #[arg(long)]
    check: i64,
}

#[derive(Debug, Args)]
struct WorkCloseArgs {
    #[arg(long)]
    summary: String,
    #[arg(long)]
    commit: Option<String>,
}

#[derive(Debug, Args)]
struct WorkForkArgs {
    title: String,
    #[arg(long)]
    from_record: Option<i64>,
    #[arg(long)]
    from_activation: Option<i64>,
    #[arg(long)]
    from_commit: Option<String>,
    #[arg(long)]
    from_git_commit_id: Option<i64>,
    #[arg(long)]
    from_snapshot: Option<i64>,
    #[arg(long)]
    reason: String,
    #[arg(long, default_value = "keep_history")]
    discard_policy: String,
}

#[derive(Debug, Args)]
struct WorkReopenArgs {
    work_unit_id: i64,
    #[arg(long)]
    reason: String,
}

#[derive(Debug, Args)]
struct WorkFollowUpArgs {
    source_work_unit_id: i64,
    title: String,
    #[arg(long)]
    reason: String,
}

#[derive(Debug, Args)]
struct ResumeCheckArgs {
    #[arg(long, default_value = "basic")]
    maturity: String,
}

#[derive(Debug, Subcommand)]
enum GateCommand {
    /// Check whether the active work unit can close without writing ledger rows.
    CloseReady(GateCloseReadyArgs),
    /// Select a validation gate from an imported template.
    Select(GateSelectArgs),
    /// Record a validation result for a selected gate.
    Record(GateRecordArgs),
    /// List recorded validation runs.
    Run {
        #[command(subcommand)]
        command: GateRunCommand,
    },
    /// Check whether a suspended activation can resume without writing ledger rows.
    ResumeReady(GateResumeReadyArgs),
    /// Check whether an imported design version is ready for implementation planning.
    DesignReady(GateDesignReadyArgs),
    /// Check whether approved design work is decomposed and current.
    ImplementationReady(GateImplementationReadyArgs),
}

#[derive(Debug, Args)]
struct GateResumeReadyArgs {
    #[arg(long, default_value = "basic")]
    maturity: String,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct GateCloseReadyArgs {
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct GateDesignReadyArgs {
    #[arg(long)]
    design_version: Option<i64>,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct GateImplementationReadyArgs {
    #[arg(long)]
    design_version: Option<i64>,
    #[arg(long)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct GateSelectArgs {
    #[arg(long)]
    design: i64,
    #[arg(long)]
    template: String,
    #[arg(long)]
    requirement: String,
    #[arg(long)]
    task: i64,
    #[arg(long)]
    command: Option<String>,
}

#[derive(Debug, Args)]
struct GateRecordArgs {
    #[arg(long)]
    gate: i64,
    #[arg(long)]
    result: String,
    #[arg(long)]
    usage: Option<i64>,
    #[arg(long)]
    snapshot: Option<i64>,
    #[arg(long)]
    artifact: Option<String>,
    #[arg(long)]
    artifact_hash: Option<String>,
    #[arg(long)]
    notes: Option<String>,
}

#[derive(Debug, Subcommand)]
enum GateRunCommand {
    List(GateRunListArgs),
}

#[derive(Debug, Args)]
struct GateRunListArgs {
    #[arg(long)]
    gate: Option<i64>,
}

#[derive(Debug, Subcommand)]
enum CorrectionCommand {
    Add(CorrectionAddArgs),
    List(CorrectionListArgs),
}

#[derive(Debug, Args)]
struct CorrectionAddArgs {
    #[arg(long)]
    scope: String,
    #[arg(long = "type")]
    correction_type: String,
    #[arg(long)]
    pattern: String,
    #[arg(long)]
    correction: String,
    #[arg(long, default_value = "project")]
    applies_to: String,
    #[arg(long, default_value = "medium")]
    severity: String,
}

#[derive(Debug, Args)]
struct CorrectionListArgs {
    #[arg(long)]
    scope: Option<String>,
}

#[derive(Debug, Subcommand)]
enum MemoryCommand {
    Fixed {
        #[command(subcommand)]
        command: FixedCommand,
    },
    Usage {
        #[command(subcommand)]
        command: CommandUsageCommand,
    },
    Deviation {
        #[command(subcommand)]
        command: CommandDeviationCommand,
    },
    List(CommandListArgs),
}

#[derive(Debug, Subcommand)]
enum FixedCommand {
    Add(CommandFixedAddArgs),
}

#[derive(Debug, Args)]
struct CommandFixedAddArgs {
    #[arg(long)]
    name: String,
    #[arg(long = "type")]
    command_type: String,
    #[arg(long)]
    scope: String,
    #[arg(long)]
    command: String,
    #[arg(long)]
    timeout: Option<String>,
    #[arg(long)]
    expected_result: Option<String>,
}

#[derive(Debug, Args)]
struct CommandListArgs {
    #[arg(long = "type")]
    command_type: Option<String>,
}

#[derive(Debug, Subcommand)]
enum CommandUsageCommand {
    Add(CommandUsageAddArgs),
    List(CommandUsageListArgs),
}

#[derive(Debug, Args)]
struct CommandUsageAddArgs {
    #[arg(long)]
    profile: Option<String>,
    #[arg(long)]
    command: Option<String>,
    #[arg(long, default_value = "unknown")]
    result: String,
    #[arg(long)]
    log: Option<String>,
    #[arg(long)]
    work_unit: Option<i64>,
    #[arg(long)]
    snapshot: Option<i64>,
}

#[derive(Debug, Args)]
struct CommandUsageListArgs {
    #[arg(long)]
    profile: Option<String>,
    #[arg(long)]
    work_unit: Option<i64>,
}

#[derive(Debug, Subcommand)]
enum CommandDeviationCommand {
    Add(CommandDeviationAddArgs),
}

#[derive(Debug, Args)]
struct CommandDeviationAddArgs {
    #[arg(long)]
    profile: String,
    #[arg(long)]
    usage: Option<i64>,
    #[arg(long)]
    reason: String,
}

#[derive(Debug, Subcommand)]
enum RulesCommand {
    Applicable(RulesApplicableArgs),
}

#[derive(Debug, Args)]
struct RulesApplicableArgs {
    #[arg(long)]
    scope: Option<String>,
    #[arg(long)]
    work_unit: Option<i64>,
}

#[derive(Debug, Subcommand)]
enum WorkRecordCommand {
    Create(WorkRecordCreateArgs),
    List(WorkRecordListArgs),
    Export(WorkRecordExportArgs),
    Command {
        #[command(subcommand)]
        command: WorkRecordCommandLinkCommand,
    },
    Commit {
        #[command(subcommand)]
        command: WorkRecordCommitCommand,
    },
    File {
        #[command(subcommand)]
        command: WorkRecordFileCommand,
    },
}

#[derive(Debug, Args)]
struct WorkRecordCreateArgs {
    #[arg(long)]
    topic: String,
    #[arg(long)]
    work_performed: Option<String>,
    #[arg(long)]
    next_actions: Option<String>,
    #[arg(long)]
    notable_operations: Option<String>,
    #[arg(long)]
    work_unit: Option<i64>,
    #[arg(long, alias = "export-md")]
    export_path: Option<String>,
}

#[derive(Debug, Args)]
struct WorkRecordListArgs {
    #[arg(long)]
    work_unit: Option<i64>,
}

#[derive(Debug, Args)]
struct WorkRecordExportArgs {
    work_record_id: i64,
    #[arg(long, default_value = "md")]
    format: String,
}

#[derive(Debug, Subcommand)]
enum WorkRecordCommandLinkCommand {
    Add(WorkRecordCommandAddArgs),
}

#[derive(Debug, Args)]
struct WorkRecordCommandAddArgs {
    work_record_id: i64,
    #[arg(long)]
    usage: Option<i64>,
    #[arg(long)]
    command: Option<String>,
    #[arg(long)]
    result: Option<String>,
    #[arg(long)]
    profile: Option<i64>,
    #[arg(long)]
    log_path: Option<String>,
    #[arg(long)]
    note: Option<String>,
}

#[derive(Debug, Subcommand)]
enum WorkRecordCommitCommand {
    Add(WorkRecordCommitAddArgs),
}

#[derive(Debug, Args)]
struct WorkRecordCommitAddArgs {
    work_record_id: i64,
    #[arg(long)]
    git_commit: Option<i64>,
    #[arg(long, alias = "commit")]
    sha: String,
    #[arg(long, default_value = "referenced")]
    role: String,
    #[arg(long)]
    note: Option<String>,
}

#[derive(Debug, Subcommand)]
enum WorkRecordFileCommand {
    Add(WorkRecordFileAddArgs),
}

#[derive(Debug, Args)]
struct WorkRecordFileAddArgs {
    work_record_id: i64,
    #[arg(long)]
    git_file_change: Option<i64>,
    #[arg(long)]
    repository_id: Option<i64>,
    #[arg(long)]
    path: String,
    #[arg(long, default_value = "changed")]
    role: String,
    #[arg(long)]
    note: Option<String>,
}

#[derive(Debug, Subcommand)]
enum RepositoryCommand {
    Add(RepositoryAddArgs),
    List,
    Snapshot {
        #[command(subcommand)]
        command: RepositorySnapshotCommand,
    },
    Dirty {
        #[command(subcommand)]
        command: RepositoryDirtyCommand,
    },
    Classify {
        #[command(subcommand)]
        command: RepositoryClassifyCommand,
    },
    Commit {
        #[command(subcommand)]
        command: RepositoryCommitCommand,
    },
    File {
        #[command(subcommand)]
        command: RepositoryFileCommand,
    },
    Compare {
        #[command(subcommand)]
        command: RepositoryCompareCommand,
    },
}

#[derive(Debug, Args)]
struct RepositoryAddArgs {
    name: String,
    #[arg(long)]
    path: String,
    #[arg(long)]
    head: Option<String>,
    #[arg(long)]
    status: Option<String>,
}

#[derive(Debug, Subcommand)]
enum RepositorySnapshotCommand {
    Add(RepositorySnapshotAddArgs),
    List(RepositorySnapshotListArgs),
}

#[derive(Debug, Args)]
struct RepositorySnapshotAddArgs {
    #[arg(long)]
    repository: String,
    #[arg(long)]
    activation: Option<i64>,
    #[arg(long)]
    head: Option<String>,
    #[arg(long)]
    branch: Option<String>,
    #[arg(long)]
    status: Option<String>,
    #[arg(long)]
    clean: bool,
}

#[derive(Debug, Args)]
struct RepositorySnapshotListArgs {
    #[arg(long)]
    repository: Option<String>,
}

#[derive(Debug, Subcommand)]
enum RepositoryDirtyCommand {
    Add(RepositoryDirtyAddArgs),
}

#[derive(Debug, Args)]
struct RepositoryDirtyAddArgs {
    #[arg(long)]
    snapshot: i64,
    #[arg(long)]
    path: String,
    #[arg(long = "type")]
    change_type: String,
    #[arg(long)]
    staged: bool,
    #[arg(long)]
    hash: Option<String>,
}

#[derive(Debug, Subcommand)]
enum RepositoryClassifyCommand {
    Add(RepositoryClassifyAddArgs),
}

#[derive(Debug, Args)]
struct RepositoryClassifyAddArgs {
    #[arg(long)]
    snapshot: i64,
    #[arg(long)]
    dirty_entry: Option<i64>,
    #[arg(long)]
    classification: String,
    #[arg(long)]
    reason: String,
    #[arg(long)]
    acceptance: Option<i64>,
}

#[derive(Debug, Subcommand)]
enum RepositoryCommitCommand {
    Add(RepositoryCommitAddArgs),
}

#[derive(Debug, Args)]
struct RepositoryCommitAddArgs {
    #[arg(long)]
    repository: String,
    #[arg(long, alias = "commit")]
    sha: String,
    #[arg(long)]
    short: Option<String>,
    #[arg(long)]
    subject: Option<String>,
    #[arg(long)]
    author_name: Option<String>,
    #[arg(long)]
    author_email: Option<String>,
    #[arg(long)]
    committed_at: Option<String>,
    #[arg(long)]
    parents: Option<String>,
}

#[derive(Debug, Subcommand)]
enum RepositoryFileCommand {
    Add(RepositoryFileAddArgs),
}

#[derive(Debug, Args)]
struct RepositoryFileAddArgs {
    #[arg(long)]
    commit: i64,
    #[arg(long)]
    repository: Option<String>,
    #[arg(long)]
    path: String,
    #[arg(long)]
    old_path: Option<String>,
    #[arg(long = "type")]
    change_type: String,
    #[arg(long)]
    additions: Option<i64>,
    #[arg(long)]
    deletions: Option<i64>,
    #[arg(long)]
    hash: Option<String>,
}

#[derive(Debug, Subcommand)]
enum RepositoryCompareCommand {
    Add(RepositoryCompareAddArgs),
}

#[derive(Debug, Args)]
struct RepositoryCompareAddArgs {
    #[arg(long)]
    base: i64,
    #[arg(long)]
    current: i64,
    #[arg(long = "type")]
    comparison_type: String,
    #[arg(long)]
    head_changed: bool,
    #[arg(long)]
    dirty_changed: bool,
    #[arg(long)]
    nested_changed: bool,
    #[arg(long)]
    result: String,
}

#[derive(Debug, Subcommand)]
enum TaskCommand {
    Add(TaskAddArgs),
    List(TaskListArgs),
    Close(TaskCloseArgs),
    AcceptOutOfScope(TaskAcceptOutOfScopeArgs),
}

#[derive(Debug, Args)]
struct TaskAddArgs {
    title: String,
    #[arg(long, default_value = "medium")]
    priority: String,
    #[arg(long, default_value = "user")]
    source: String,
    #[arg(long)]
    work_unit: Option<i64>,
    #[arg(long)]
    details: Option<String>,
    #[arg(long)]
    completion_condition: Option<String>,
}

#[derive(Debug, Args)]
struct TaskListArgs {
    #[arg(long)]
    status: Option<String>,
    #[arg(long)]
    work_unit: Option<i64>,
}

#[derive(Debug, Args)]
struct TaskCloseArgs {
    task_id: i64,
    #[arg(long)]
    commit: Option<String>,
}

#[derive(Debug, Args)]
struct TaskAcceptOutOfScopeArgs {
    task_id: i64,
    #[arg(long)]
    reason: String,
}

#[derive(Debug, Subcommand)]
enum DecisionCommand {
    Add(DecisionAddArgs),
    List(DecisionListArgs),
    Search(DecisionSearchArgs),
}

#[derive(Debug, Args)]
struct DecisionAddArgs {
    #[arg(long)]
    topic: String,
    #[arg(long)]
    decision: String,
    #[arg(long)]
    key: Option<String>,
    #[arg(long)]
    rationale: Option<String>,
    #[arg(long)]
    compatibility_impact: Option<String>,
    #[arg(long)]
    authority_refs: Option<String>,
}

#[derive(Debug, Args)]
struct DecisionListArgs {
    #[arg(long)]
    query: Option<String>,
}

#[derive(Debug, Args)]
struct DecisionSearchArgs {
    query: String,
}

#[derive(Debug, Subcommand)]
enum DesignCommand {
    Init(DesignInitArgs),
    Import(DesignImportArgs),
    Approve(DesignApproveArgs),
}

#[derive(Debug, Args)]
struct DesignInitArgs {
    design_id: String,
    #[arg(long)]
    title: Option<String>,
}

#[derive(Debug, Args)]
struct DesignImportArgs {
    package_path: PathBuf,
    #[arg(long, default_value = "draft")]
    status: String,
}

#[derive(Debug, Args)]
struct DesignApproveArgs {
    design_version_id: i64,
    #[arg(long)]
    summary: Option<String>,
}

#[derive(Debug, Subcommand)]
enum RequirementCommand {
    List(RequirementListArgs),
}

#[derive(Debug, Args)]
struct RequirementListArgs {
    #[arg(long)]
    design: i64,
}

#[derive(Debug, Subcommand)]
enum DesignDecisionCommand {
    List(DesignDecisionListArgs),
}

#[derive(Debug, Args)]
struct DesignDecisionListArgs {
    #[arg(long)]
    design: i64,
}

#[derive(Debug, Subcommand)]
enum GateTemplateCommand {
    List(GateTemplateListArgs),
}

#[derive(Debug, Args)]
struct GateTemplateListArgs {
    #[arg(long)]
    design: i64,
}

#[derive(Debug, Subcommand)]
enum TraceCommand {
    DeriveTask(TraceDeriveTaskArgs),
    Derivation {
        #[command(subcommand)]
        command: TraceDerivationCommand,
    },
}

#[derive(Debug, Args)]
struct TraceDeriveTaskArgs {
    #[arg(long)]
    design: i64,
    #[arg(long)]
    requirement: String,
    #[arg(long)]
    task: i64,
    #[arg(long)]
    reason: Option<String>,
    #[arg(long)]
    checklist_title: Option<String>,
    #[arg(long)]
    item_title: Option<String>,
    #[arg(long)]
    completion_condition: Option<String>,
}

#[derive(Debug, Subcommand)]
enum TraceDerivationCommand {
    List(TraceDerivationListArgs),
}

#[derive(Debug, Args)]
struct TraceDerivationListArgs {
    #[arg(long)]
    design: i64,
}

#[derive(Debug, Subcommand)]
enum EvidenceCommand {
    Add(Box<EvidenceAddArgs>),
    List(EvidenceListArgs),
}

#[derive(Debug, Args)]
struct EvidenceAddArgs {
    #[arg(long)]
    task: Option<i64>,
    #[arg(long)]
    design: Option<i64>,
    #[arg(long)]
    requirement: Option<String>,
    #[arg(long = "type")]
    evidence_type: String,
    #[arg(long)]
    repository_id: Option<i64>,
    #[arg(long)]
    git_commit_id: Option<i64>,
    #[arg(long)]
    git_file_change_id: Option<i64>,
    #[arg(long)]
    commit: Option<String>,
    #[arg(long)]
    file: Option<String>,
    #[arg(long)]
    line: Option<String>,
    #[arg(long)]
    symbol: Option<String>,
    #[arg(long)]
    artifact: Option<String>,
    #[arg(long)]
    note: Option<String>,
}

#[derive(Debug, Args)]
struct EvidenceListArgs {
    #[arg(long)]
    task: Option<i64>,
    #[arg(long)]
    design: Option<i64>,
}

#[derive(Debug, Subcommand)]
enum CoverageCommand {
    Add(CoverageAddArgs),
    List(CoverageListArgs),
}

#[derive(Debug, Args)]
struct CoverageAddArgs {
    #[arg(long)]
    design: i64,
    #[arg(long)]
    requirement: String,
    #[arg(long)]
    task: Option<i64>,
    #[arg(long)]
    work_unit: Option<i64>,
    #[arg(long)]
    status: String,
    #[arg(long)]
    requirement_text: String,
    #[arg(long)]
    runtime: Option<String>,
    #[arg(long)]
    ux: Option<String>,
    #[arg(long)]
    lifecycle: Option<String>,
    #[arg(long)]
    tests_or_gates: Option<String>,
    #[arg(long)]
    missing: Option<String>,
}

#[derive(Debug, Args)]
struct CoverageListArgs {
    #[arg(long)]
    design: i64,
    #[arg(long)]
    status: Option<String>,
}

#[derive(Debug, Subcommand)]
enum ReviewCommand {
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
enum ReviewScopeCommand {
    Start(ReviewScopeStartArgs),
    List,
}

#[derive(Debug, Args)]
struct ReviewScopeStartArgs {
    name: String,
    #[arg(long = "type", default_value = "general")]
    review_type: String,
    #[arg(long)]
    scope: String,
}

#[derive(Debug, Subcommand)]
enum ReviewPolicyCommand {
    Add(ReviewPolicyAddArgs),
    List,
}

#[derive(Debug, Args)]
struct ReviewPolicyAddArgs {
    #[arg(long)]
    name: String,
    #[arg(long = "type")]
    review_type: String,
    #[arg(long, default_value_t = 1)]
    fresh_clean: i64,
    #[arg(long, default_value_t = 0)]
    resume_clean: i64,
    #[arg(long, default_value_t = 1)]
    max_fresh_agents: i64,
    #[arg(long, default_value_t = 1)]
    max_resume_agents: i64,
    #[arg(long, default_value_t = 1)]
    max_parallel_agents: i64,
    #[arg(long, default_value = "none")]
    stop_on_severity: String,
    #[arg(long, default_value_t = false)]
    allow_new_findings_in_resume: bool,
    #[arg(long, default_value = "review_plan")]
    run_count_scope: String,
    #[arg(long, default_value = "fresh")]
    default_run_mode: String,
    #[arg(long, default_value = "block")]
    on_max_agents_exceeded: String,
}

#[derive(Debug, Subcommand)]
enum ReviewPlanCommand {
    Add(ReviewPlanAddArgs),
    List,
    Context(ReviewPlanContextArgs),
}

#[derive(Debug, Args)]
struct ReviewPlanAddArgs {
    #[arg(long)]
    work_unit: i64,
    #[arg(long = "type")]
    review_type: String,
    #[arg(long)]
    stage: String,
    #[arg(long)]
    design_version: Option<i64>,
    #[arg(long)]
    scope: Option<String>,
    #[arg(long)]
    policy: Option<i64>,
    #[arg(long)]
    review_scope: Option<i64>,
    #[arg(long, default_value_t = true)]
    required: bool,
}

#[derive(Debug, Args)]
struct ReviewPlanContextArgs {
    review_plan_id: i64,
}

#[derive(Debug, Subcommand)]
enum ReviewRunCommand {
    Add(ReviewRunAddArgs),
    List(ReviewRunListArgs),
}

#[derive(Debug, Args)]
struct ReviewRunAddArgs {
    #[arg(long)]
    plan: i64,
    #[arg(long = "type")]
    run_type: String,
    #[arg(long)]
    purpose: String,
    #[arg(long)]
    target: Option<String>,
    #[arg(long, default_value = "completed")]
    status: String,
    #[arg(long)]
    clean: bool,
    #[arg(long, default_value_t = 0)]
    new_findings: i64,
    #[arg(long, default_value_t = 0)]
    carried_findings: i64,
    #[arg(long)]
    summary: Option<String>,
    #[arg(long)]
    agent_label: Option<String>,
    #[arg(long)]
    external_agent_id: Option<String>,
}

#[derive(Debug, Args)]
struct ReviewRunListArgs {
    #[arg(long)]
    plan: Option<i64>,
}

#[derive(Debug, Subcommand)]
enum FindingCommand {
    Add(FindingAddArgs),
    Classify(FindingClassifyArgs),
    List(FindingListArgs),
    Verify(FindingVerifyArgs),
}

#[derive(Debug, Args)]
struct FindingAddArgs {
    #[arg(long)]
    run: i64,
    #[arg(long = "type")]
    finding_type: String,
    #[arg(long)]
    severity: String,
    #[arg(long)]
    description: String,
    #[arg(long)]
    design_requirement: Option<i64>,
    #[arg(long)]
    task: Option<i64>,
}

#[derive(Debug, Args)]
struct FindingClassifyArgs {
    finding_id: i64,
    #[arg(long)]
    classification: String,
}

#[derive(Debug, Args)]
struct FindingListArgs {
    #[arg(long)]
    status: Option<String>,
}

#[derive(Debug, Args)]
struct FindingVerifyArgs {
    #[arg(long)]
    run: i64,
    #[arg(long)]
    finding: i64,
    #[arg(long)]
    closure: i64,
    #[arg(long)]
    result: String,
    #[arg(long)]
    notes: Option<String>,
}

#[derive(Debug, Subcommand)]
enum ClosureCommand {
    Add(ClosureAddArgs),
}

#[derive(Debug, Args)]
struct ClosureAddArgs {
    #[arg(long)]
    finding: i64,
    #[arg(long)]
    invariant: String,
    #[arg(long)]
    citations: Option<String>,
    #[arg(long)]
    evidence: Option<String>,
    #[arg(long)]
    surfaces: Option<String>,
    #[arg(long)]
    search: Option<String>,
    #[arg(long)]
    other_violations: Option<String>,
    #[arg(long)]
    fix_plan: Option<String>,
    #[arg(long)]
    tests: Option<String>,
    #[arg(long)]
    verification: Option<String>,
    #[arg(long)]
    commit: Option<String>,
}

#[derive(Debug, Subcommand)]
enum AcceptanceCommand {
    Add(AcceptanceAddArgs),
}

#[derive(Debug, Args)]
struct AcceptanceAddArgs {
    #[arg(long)]
    design: Option<i64>,
    #[arg(long)]
    package: Option<String>,
    #[arg(long)]
    target: String,
    #[arg(long = "type")]
    acceptance_type: String,
    #[arg(long)]
    reason: String,
}

#[derive(Debug, Subcommand)]
enum AuthorityCommand {
    Event {
        #[command(subcommand)]
        command: AuthorityEventCommand,
    },
    List(AuthorityListArgs),
}

#[derive(Debug, Subcommand)]
enum AuthorityEventCommand {
    Add(AuthorityEventAddArgs),
}

#[derive(Debug, Args)]
struct AuthorityEventAddArgs {
    #[arg(long = "type")]
    event_type: String,
    #[arg(long)]
    summary: String,
    #[arg(long)]
    scope: Option<String>,
    #[arg(long)]
    source: Option<String>,
    #[arg(long, default_value_t = 100)]
    precedence: i64,
}

#[derive(Debug, Args)]
struct AuthorityListArgs {
    #[arg(long)]
    scope: Option<String>,
}

#[derive(Debug, Subcommand)]
enum KptCommand {
    Start(KptStartArgs),
    List(KptListArgs),
    Close(KptCloseArgs),
    Item {
        #[command(subcommand)]
        command: KptItemCommand,
    },
}

#[derive(Debug, Args)]
struct KptStartArgs {
    #[arg(long)]
    scope: Option<String>,
    #[arg(long)]
    summary: Option<String>,
    #[arg(long = "from")]
    from: Option<String>,
    #[arg(long)]
    period: Option<String>,
}

#[derive(Debug, Args)]
struct KptListArgs {
    #[arg(long)]
    status: Option<String>,
}

#[derive(Debug, Args)]
struct KptCloseArgs {
    kpt_review_id: i64,
}

#[derive(Debug, Subcommand)]
enum KptItemCommand {
    Add(KptItemAddArgs),
    List(KptItemListArgs),
    Convert(Box<KptItemConvertArgs>),
}

#[derive(Debug, Args)]
struct KptItemAddArgs {
    #[arg(long = "type")]
    item_type: String,
    #[arg(long)]
    title: String,
    #[arg(long)]
    review: Option<i64>,
    #[arg(long)]
    details: Option<String>,
    #[arg(long, default_value = "medium")]
    severity: String,
    #[arg(long)]
    proposed_action: Option<String>,
}

#[derive(Debug, Args)]
struct KptItemListArgs {
    #[arg(long)]
    review: Option<i64>,
}

#[derive(Debug, Args)]
struct KptItemConvertArgs {
    #[arg(long)]
    item: i64,
    #[arg(long = "to")]
    target_type: String,
    #[arg(long)]
    title: Option<String>,
    #[arg(long)]
    details: Option<String>,
    #[arg(long, default_value = "medium")]
    priority: String,
    #[arg(long)]
    work_unit: Option<i64>,
    #[arg(long)]
    name: Option<String>,
    #[arg(long)]
    command: Option<String>,
    #[arg(long, default_value = "other")]
    command_type: String,
    #[arg(long)]
    scope: Option<String>,
    #[arg(long, default_value = "candidate")]
    command_status: String,
    #[arg(long, default_value = "context_dependent")]
    stability: String,
    #[arg(long)]
    timeout: Option<String>,
    #[arg(long)]
    expected_result: Option<String>,
    #[arg(long = "review-type")]
    review_type: Option<String>,
    #[arg(long, default_value_t = 1)]
    fresh_clean: i64,
    #[arg(long, default_value_t = 0)]
    resume_clean: i64,
    #[arg(long, default_value_t = 1)]
    max_fresh_agents: i64,
    #[arg(long, default_value_t = 1)]
    max_resume_agents: i64,
    #[arg(long, default_value_t = 1)]
    max_parallel_agents: i64,
    #[arg(long, default_value = "none")]
    stop_on_severity: String,
    #[arg(long, default_value_t = false)]
    allow_new_findings_in_resume: bool,
    #[arg(long, default_value = "review_plan")]
    run_count_scope: String,
    #[arg(long, default_value = "fresh")]
    default_run_mode: String,
    #[arg(long, default_value = "block")]
    on_max_agents_exceeded: String,
    #[arg(long)]
    decision_key: Option<String>,
    #[arg(long)]
    rationale: Option<String>,
    #[arg(long)]
    compatibility_impact: Option<String>,
    #[arg(long)]
    authority_refs: Option<String>,
    #[arg(long)]
    design_version: Option<i64>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = match cli.root {
        Some(root) => root,
        None => env::current_dir()?,
    };

    match cli.command {
        Command::Init => {
            let outcome = init_project(&root)?;
            println!("initialized ledger: {}", outcome.ledger_path.display());
        }
        Command::Status => {
            let status = project_status(&root)?;
            if !status.initialized {
                println!("not initialized");
                println!("ledger: {}", status.ledger_path.display());
                println!("next: agent-workbench init");
            } else {
                println!("initialized");
                println!("ledger: {}", status.ledger_path.display());
                if let Some(name) = status.project_name {
                    println!("project: {name}");
                }
                if let Some(version) = status.schema_version {
                    println!("schema_version: {version}");
                }
                println!("open_work_units: {}", status.open_work_units);
                println!("active_activations: {}", status.active_activations);
            }
        }
        Command::Next => match next_action(&root)? {
            NextAction::NotInitialized { ledger_path } => {
                println!("not initialized");
                println!("ledger: {}", ledger_path.display());
                println!("next: agent-workbench init");
            }
            NextAction::NoActiveWorkUnit => {
                println!("no active work unit");
                println!("next: agent-workbench work start <title>");
            }
            NextAction::ContinueActive { work_unit } => {
                println!("continue active work unit");
                println!("work_unit_id: {}", work_unit.id);
                println!("title: {}", work_unit.title);
            }
        },
        Command::Work { command } => match command {
            WorkCommand::Start(args) => {
                let outcome = start_work(&root, &args.title, args.responsibility.as_deref())?;
                println!("started work unit");
                println!("work_unit_id: {}", outcome.work_unit_id);
                println!("activation_id: {}", outcome.activation_id);
            }
            WorkCommand::Suspend(args) => {
                let outcome = suspend_work(&root, &args.reason, &args.next)?;
                println!("suspended work unit");
                println!("work_unit_id: {}", outcome.work_unit_id);
                println!("activation_id: {}", outcome.activation_id);
                println!("suspend_snapshot_id: {}", outcome.suspend_snapshot_id);
            }
            WorkCommand::Interrupt(args) => {
                let outcome = interrupt_work(&root, &args.title, &args.reason)?;
                println!("interrupted active work");
                println!("parent_work_unit_id: {}", outcome.parent_work_unit_id);
                println!("parent_activation_id: {}", outcome.parent_activation_id);
                println!(
                    "parent_suspend_snapshot_id: {}",
                    outcome.parent_suspend_snapshot_id
                );
                println!("child_work_unit_id: {}", outcome.child_work_unit_id);
                println!("child_activation_id: {}", outcome.child_activation_id);
            }
            WorkCommand::Resume(args) => {
                let outcome = resume_work(&root, args.check)?;
                println!("resumed work unit");
                println!("work_unit_id: {}", outcome.work_unit_id);
                println!("activation_id: {}", outcome.activation_id);
            }
            WorkCommand::Close(args) => {
                let outcome = close_active_work(&root, &args.summary, args.commit.as_deref())?;
                println!("closed work unit");
                println!("work_unit_id: {}", outcome.work_unit_id);
                println!("activation_id: {}", outcome.activation_id);
            }
            WorkCommand::Fork(args) => {
                let source_count = [
                    args.from_record.is_some(),
                    args.from_activation.is_some(),
                    args.from_commit.is_some(),
                    args.from_git_commit_id.is_some(),
                    args.from_snapshot.is_some(),
                ]
                .into_iter()
                .filter(|selected| *selected)
                .count();
                if source_count != 1 {
                    anyhow::bail!(
                        "exactly one of --from-record, --from-activation, --from-commit, --from-git-commit-id, or --from-snapshot is required"
                    );
                }

                let source = match (
                    args.from_record,
                    args.from_activation,
                    args.from_commit.as_deref(),
                    args.from_git_commit_id,
                    args.from_snapshot,
                ) {
                    (Some(id), None, None, None, None) => WorkForkSource::Record(id),
                    (None, Some(id), None, None, None) => WorkForkSource::Activation(id),
                    (None, None, Some(sha), None, None) => WorkForkSource::Commit(sha),
                    (None, None, None, Some(id), None) => WorkForkSource::GitCommit(id),
                    (None, None, None, None, Some(id)) => WorkForkSource::RepositorySnapshot(id),
                    _ => unreachable!("source count checked above"),
                };
                let outcome = fork_work(
                    &root,
                    NewWorkFork {
                        title: &args.title,
                        source,
                        reason: &args.reason,
                        discard_policy: &args.discard_policy,
                    },
                )?;
                println!("forked work");
                println!("fork_id: {}", outcome.fork_id);
                println!("work_unit_id: {}", outcome.work_unit_id);
                println!("activation_id: {}", outcome.activation_id);
            }
            WorkCommand::Reopen(args) => {
                let outcome = reopen_work(&root, args.work_unit_id, &args.reason)?;
                println!("reopened work unit");
                println!("work_unit_id: {}", outcome.work_unit_id);
                println!("activation_id: {}", outcome.activation_id);
            }
            WorkCommand::FollowUp(args) => {
                let outcome = create_follow_up_work(
                    &root,
                    args.source_work_unit_id,
                    &args.title,
                    &args.reason,
                )?;
                println!("created follow-up work unit");
                println!("source_work_unit_id: {}", outcome.source_work_unit_id);
                println!("work_unit_id: {}", outcome.work_unit_id);
                println!("activation_id: {}", outcome.activation_id);
            }
        },
        Command::ResumeCheck(args) => {
            let outcome = resume_check(&root, &args.maturity)?;
            println!("resume_check_id: {}", outcome.resume_check_id);
            println!("result: {}", outcome.result);
            if let Some(reason) = outcome.blocking_reason {
                println!("blocking_reason: {reason}");
            }
        }
        Command::Gate { command } => match command {
            GateCommand::CloseReady(args) => {
                if !args.dry_run {
                    anyhow::bail!("gate close-ready is read-only; pass --dry-run");
                }
                let outcome = close_ready(&root)?;
                println!("gate: close-ready");
                println!("dry_run: true");
                if let Some(work_unit_id) = outcome.work_unit_id {
                    println!("work_unit_id: {work_unit_id}");
                }
                if let Some(activation_id) = outcome.activation_id {
                    println!("activation_id: {activation_id}");
                }
                println!("result: {}", outcome.result);
                if let Some(reason) = outcome.blocking_reason {
                    println!("blocking_reason: {reason}");
                }
                for item in outcome.items {
                    match item.blocking_action {
                        Some(action) => println!("{}: {} ({})", item.name, item.result, action),
                        None => println!("{}: {}", item.name, item.result),
                    }
                }
            }
            GateCommand::Select(args) => {
                let outcome = select_validation_gate(
                    &root,
                    ValidationGateSelection {
                        design_version_id: args.design,
                        gate_key: &args.template,
                        requirement_key: &args.requirement,
                        task_id: args.task,
                        command: args.command.as_deref(),
                    },
                )?;
                println!("selected validation gate");
                println!("validation_gate_id: {}", outcome.validation_gate_id);
                println!(
                    "validation_gate_template_id: {}",
                    outcome.validation_gate_template_id
                );
                println!("design_requirement_id: {}", outcome.design_requirement_id);
                println!("task_id: {}", outcome.task_id);
            }
            GateCommand::Record(args) => {
                let outcome = add_validation_run(
                    &root,
                    NewValidationRun {
                        validation_gate_id: args.gate,
                        command_usage_id: args.usage,
                        repository_snapshot_id: args.snapshot,
                        result: &args.result,
                        artifact_path: args.artifact.as_deref(),
                        artifact_hash: args.artifact_hash.as_deref(),
                        notes: args.notes.as_deref(),
                    },
                )?;
                println!("recorded validation run");
                println!("validation_run_id: {}", outcome.validation_run_id);
                println!("validation_gate_id: {}", outcome.validation_gate_id);
                if let Some(work_unit_id) = outcome.work_unit_id {
                    println!("work_unit_id: {work_unit_id}");
                }
                if let Some(task_id) = outcome.task_id {
                    println!("task_id: {task_id}");
                }
            }
            GateCommand::Run { command } => match command {
                GateRunCommand::List(args) => {
                    let records = list_validation_runs(
                        &root,
                        ValidationRunListQuery {
                            validation_gate_id: args.gate,
                        },
                    )?;
                    if records.is_empty() {
                        println!("no validation runs");
                    }
                    for record in records {
                        let usage = record
                            .command_usage_id
                            .map(|id| id.to_string())
                            .unwrap_or_else(|| "-".to_string());
                        let snapshot = record
                            .repository_snapshot_id
                            .map(|id| id.to_string())
                            .unwrap_or_else(|| "-".to_string());
                        let artifact = record.artifact_path.as_deref().unwrap_or("-");
                        println!(
                            "{} [gate={} {}:{}] usage={} snapshot={} artifact={}",
                            record.id,
                            record.validation_gate_id,
                            record.gate_key,
                            record.result,
                            usage,
                            snapshot,
                            artifact
                        );
                    }
                }
            },
            GateCommand::ResumeReady(args) => {
                if !args.dry_run {
                    anyhow::bail!("gate resume-ready is read-only; pass --dry-run");
                }
                let outcome = resume_ready(&root, &args.maturity)?;
                println!("gate: resume-ready");
                println!("maturity: {}", args.maturity);
                println!("dry_run: true");
                if let Some(work_unit_id) = outcome.work_unit_id {
                    println!("work_unit_id: {work_unit_id}");
                }
                if let Some(activation_id) = outcome.activation_id {
                    println!("activation_id: {activation_id}");
                }
                println!("result: {}", outcome.result);
                if let Some(reason) = outcome.blocking_reason {
                    println!("blocking_reason: {reason}");
                }
                for item in outcome.items {
                    match item.blocking_action {
                        Some(action) => {
                            println!("{}: {} ({})", item.name, item.result, action);
                        }
                        None => {
                            println!("{}: {}", item.name, item.result);
                        }
                    }
                }
            }
            GateCommand::DesignReady(args) => {
                if !args.dry_run {
                    anyhow::bail!("gate design-ready is read-only; pass --dry-run");
                }
                let outcome = design_ready(
                    &root,
                    DesignReadyCheck {
                        design_version_id: args.design_version,
                    },
                )?;
                println!("gate: design-ready");
                println!("dry_run: true");
                if let Some(design_package_id) = outcome.design_package_id {
                    println!("design_package_id: {design_package_id}");
                }
                if let Some(design_version_id) = outcome.design_version_id {
                    println!("design_version_id: {design_version_id}");
                }
                println!("result: {}", outcome.result);
                if let Some(reason) = outcome.blocking_reason {
                    println!("blocking_reason: {reason}");
                }
                for item in outcome.items {
                    match item.detail {
                        Some(detail) => println!("{}: {} ({})", item.name, item.result, detail),
                        None => println!("{}: {}", item.name, item.result),
                    }
                }
            }
            GateCommand::ImplementationReady(args) => {
                if !args.dry_run {
                    anyhow::bail!("gate implementation-ready is read-only; pass --dry-run");
                }
                let outcome = implementation_ready(
                    &root,
                    ImplementationReadyCheck {
                        design_version_id: args.design_version,
                    },
                )?;
                println!("gate: implementation-ready");
                println!("dry_run: true");
                if let Some(design_package_id) = outcome.design_package_id {
                    println!("design_package_id: {design_package_id}");
                }
                if let Some(design_version_id) = outcome.design_version_id {
                    println!("design_version_id: {design_version_id}");
                }
                println!("result: {}", outcome.result);
                if let Some(reason) = outcome.blocking_reason {
                    println!("blocking_reason: {reason}");
                }
                for item in outcome.items {
                    match item.detail {
                        Some(detail) => println!("{}: {} ({})", item.name, item.result, detail),
                        None => println!("{}: {}", item.name, item.result),
                    }
                }
            }
        },
        Command::Correction { command } => match command {
            CorrectionCommand::Add(args) => {
                let outcome = add_user_correction(
                    &root,
                    NewUserCorrection {
                        scope: &args.scope,
                        correction_type: &args.correction_type,
                        mistake_pattern: &args.pattern,
                        correction: &args.correction,
                        applies_to: &args.applies_to,
                        severity: &args.severity,
                    },
                )?;
                println!("added correction");
                println!("user_correction_id: {}", outcome.user_correction_id);
            }
            CorrectionCommand::List(args) => {
                let records = list_user_corrections(&root, args.scope.as_deref())?;
                if records.is_empty() {
                    println!("no corrections");
                }
                for record in records {
                    println!(
                        "{} [{}:{}] {} -> {}",
                        record.id,
                        record.scope,
                        record.severity,
                        record.mistake_pattern,
                        record.correction
                    );
                }
            }
        },
        Command::Command { command } => match command {
            MemoryCommand::Fixed { command } => match command {
                FixedCommand::Add(args) => {
                    let outcome = add_fixed_command(
                        &root,
                        NewCommandProfile {
                            name: &args.name,
                            command_type: &args.command_type,
                            scope: &args.scope,
                            command: &args.command,
                            timeout: args.timeout.as_deref(),
                            expected_result: args.expected_result.as_deref(),
                        },
                    )?;
                    println!("added fixed command");
                    println!("command_profile_id: {}", outcome.command_profile_id);
                }
            },
            MemoryCommand::Usage { command } => match command {
                CommandUsageCommand::Add(args) => {
                    let outcome = match args.snapshot {
                        Some(snapshot_id) => add_command_usage_with_repository_snapshot(
                            &root,
                            NewCommandUsageWithRepositorySnapshot {
                                profile: args.profile.as_deref(),
                                command: args.command.as_deref(),
                                result: &args.result,
                                log_path: args.log.as_deref(),
                                work_unit_id: args.work_unit,
                                repository_snapshot_id: Some(snapshot_id),
                            },
                        )?,
                        None => add_command_usage(
                            &root,
                            NewCommandUsage {
                                profile: args.profile.as_deref(),
                                command: args.command.as_deref(),
                                result: &args.result,
                                log_path: args.log.as_deref(),
                                work_unit_id: args.work_unit,
                            },
                        )?,
                    };
                    println!("recorded command usage");
                    println!("command_usage_id: {}", outcome.command_usage_id);
                    if let Some(command_profile_id) = outcome.command_profile_id {
                        println!("command_profile_id: {command_profile_id}");
                    }
                    if let Some(work_unit_id) = outcome.work_unit_id {
                        println!("work_unit_id: {work_unit_id}");
                    }
                }
                CommandUsageCommand::List(args) => {
                    let records = list_command_usages(
                        &root,
                        CommandUsageListQuery {
                            profile: args.profile.as_deref(),
                            work_unit_id: args.work_unit,
                        },
                    )?;
                    if records.is_empty() {
                        println!("no command usages");
                    }
                    for record in records {
                        let profile = record
                            .command_profile_id
                            .map(|id| id.to_string())
                            .unwrap_or_else(|| "-".to_string());
                        let work_unit = record
                            .work_unit_id
                            .map(|id| id.to_string())
                            .unwrap_or_else(|| "-".to_string());
                        println!(
                            "{} [profile={} work_unit={} {}] {}",
                            record.id, profile, work_unit, record.result, record.command
                        );
                    }
                }
            },
            MemoryCommand::Deviation { command } => match command {
                CommandDeviationCommand::Add(args) => {
                    let outcome = add_command_deviation(
                        &root,
                        NewCommandDeviation {
                            profile: &args.profile,
                            command_usage_id: args.usage,
                            reason: &args.reason,
                        },
                    )?;
                    println!("recorded command deviation");
                    println!("command_deviation_id: {}", outcome.command_deviation_id);
                    println!("command_profile_id: {}", outcome.command_profile_id);
                    if let Some(work_unit_id) = outcome.work_unit_id {
                        println!("work_unit_id: {work_unit_id}");
                    }
                }
            },
            MemoryCommand::List(args) => {
                let records = list_command_profiles(&root, args.command_type.as_deref())?;
                if records.is_empty() {
                    println!("no command profiles");
                }
                for record in records {
                    println!(
                        "{} [{}:{}] {} = {}",
                        record.id, record.command_type, record.status, record.name, record.command
                    );
                }
            }
        },
        Command::Rules { command } => match command {
            RulesCommand::Applicable(args) => {
                let records = applicable_rules(
                    &root,
                    RuleQuery {
                        scope_key: args.scope.as_deref(),
                        work_unit_id: args.work_unit,
                    },
                )?;
                if records.is_empty() {
                    println!("no applicable rules");
                }
                for record in records {
                    println!(
                        "{} [{}:{} precedence={}]",
                        record.id, record.rule_source_type, record.scope_type, record.precedence
                    );
                }
            }
        },
        Command::WorkRecord { command } => match command {
            WorkRecordCommand::Create(args) => {
                let outcome = create_work_record(
                    &root,
                    NewWorkRecord {
                        work_unit_id: args.work_unit,
                        topic: &args.topic,
                        work_performed: args.work_performed.as_deref(),
                        next_actions: args.next_actions.as_deref(),
                        notable_operations: args.notable_operations.as_deref(),
                        export_path: args.export_path.as_deref(),
                    },
                )?;
                println!("created work record");
                println!("work_record_id: {}", outcome.work_record_id);
                if let Some(work_unit_id) = outcome.work_unit_id {
                    println!("work_unit_id: {work_unit_id}");
                }
            }
            WorkRecordCommand::List(args) => {
                let records = list_work_records(&root, args.work_unit)?;
                if records.is_empty() {
                    println!("no work records");
                }
                for record in records {
                    let work_unit = record
                        .work_unit_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    println!("{} [work_unit={}] {}", record.id, work_unit, record.topic);
                }
            }
            WorkRecordCommand::Export(args) => {
                if args.format != "md" {
                    anyhow::bail!("only --format md is implemented");
                }
                print!(
                    "{}",
                    export_work_record_markdown(&root, args.work_record_id)?
                );
            }
            WorkRecordCommand::Command { command } => match command {
                WorkRecordCommandLinkCommand::Add(args) => {
                    let outcome = add_work_record_command(
                        &root,
                        NewWorkRecordCommand {
                            work_record_id: args.work_record_id,
                            command_usage_id: args.usage,
                            command_profile_id: args.profile,
                            command: args.command.as_deref(),
                            result: args.result.as_deref(),
                            log_path: args.log_path.as_deref(),
                            note: args.note.as_deref(),
                        },
                    )?;
                    println!("linked work record command");
                    println!("work_record_command_id: {}", outcome.link_id);
                }
            },
            WorkRecordCommand::Commit { command } => match command {
                WorkRecordCommitCommand::Add(args) => {
                    let outcome = match args.git_commit {
                        Some(git_commit_id) => add_work_record_git_commit(
                            &root,
                            NewWorkRecordGitCommit {
                                work_record_id: args.work_record_id,
                                git_commit_id: Some(git_commit_id),
                                commit_sha: &args.sha,
                                role: &args.role,
                                note: args.note.as_deref(),
                            },
                        )?,
                        None => add_work_record_commit(
                            &root,
                            NewWorkRecordCommit {
                                work_record_id: args.work_record_id,
                                commit_sha: &args.sha,
                                role: &args.role,
                                note: args.note.as_deref(),
                            },
                        )?,
                    };
                    println!("linked work record commit");
                    println!("work_record_commit_id: {}", outcome.link_id);
                }
            },
            WorkRecordCommand::File { command } => match command {
                WorkRecordFileCommand::Add(args) => {
                    let outcome = if args.git_file_change.is_some() || args.repository_id.is_some()
                    {
                        add_work_record_git_file(
                            &root,
                            NewWorkRecordGitFile {
                                work_record_id: args.work_record_id,
                                git_file_change_id: args.git_file_change,
                                repository_id: args.repository_id,
                                path: &args.path,
                                role: &args.role,
                                note: args.note.as_deref(),
                            },
                        )?
                    } else {
                        add_work_record_file(
                            &root,
                            NewWorkRecordFile {
                                work_record_id: args.work_record_id,
                                path: &args.path,
                                role: &args.role,
                                note: args.note.as_deref(),
                            },
                        )?
                    };
                    println!("linked work record file");
                    println!("work_record_file_id: {}", outcome.link_id);
                }
            },
        },
        Command::Repository { command } => match command {
            RepositoryCommand::Add(args) => {
                let outcome = add_repository(
                    &root,
                    NewRepository {
                        name: &args.name,
                        path: &args.path,
                        current_head: args.head.as_deref(),
                        status_summary: args.status.as_deref(),
                    },
                )?;
                println!("added repository");
                println!("repository_id: {}", outcome.repository_id);
            }
            RepositoryCommand::List => {
                let records = list_repositories(&root)?;
                if records.is_empty() {
                    println!("no repositories");
                }
                for record in records {
                    let head = record.current_head.as_deref().unwrap_or("-");
                    let status = record.status_summary.as_deref().unwrap_or("-");
                    println!(
                        "{} [{} head={}] {}",
                        record.id, record.name, head, record.path
                    );
                    println!("status: {status}");
                }
            }
            RepositoryCommand::Snapshot { command } => match command {
                RepositorySnapshotCommand::Add(args) => {
                    let outcome = add_repository_snapshot(
                        &root,
                        NewRepositorySnapshot {
                            repository: &args.repository,
                            work_unit_activation_id: args.activation,
                            head_sha: args.head.as_deref(),
                            branch: args.branch.as_deref(),
                            status_summary: args.status.as_deref(),
                            is_clean: args.clean,
                        },
                    )?;
                    println!("added repository snapshot");
                    println!("repository_snapshot_id: {}", outcome.repository_snapshot_id);
                    println!("repository_id: {}", outcome.repository_id);
                }
                RepositorySnapshotCommand::List(args) => {
                    let records = list_repository_snapshots(&root, args.repository.as_deref())?;
                    if records.is_empty() {
                        println!("no repository snapshots");
                    }
                    for record in records {
                        let head = record.head_sha.as_deref().unwrap_or("-");
                        let branch = record.branch.as_deref().unwrap_or("-");
                        let status = record.status_summary.as_deref().unwrap_or("-");
                        println!(
                            "{} [repository={} clean={} branch={} head={}] {}",
                            record.id,
                            record.repository_name,
                            record.is_clean,
                            branch,
                            head,
                            status
                        );
                    }
                }
            },
            RepositoryCommand::Dirty { command } => match command {
                RepositoryDirtyCommand::Add(args) => {
                    let outcome = add_repository_dirty_entry(
                        &root,
                        NewRepositoryDirtyEntry {
                            repository_snapshot_id: args.snapshot,
                            path: &args.path,
                            change_type: &args.change_type,
                            staged: args.staged,
                            content_hash: args.hash.as_deref(),
                        },
                    )?;
                    println!("added repository dirty entry");
                    println!(
                        "repository_dirty_entry_id: {}",
                        outcome.repository_dirty_entry_id
                    );
                }
            },
            RepositoryCommand::Classify { command } => match command {
                RepositoryClassifyCommand::Add(args) => {
                    let outcome = add_repository_state_classification(
                        &root,
                        NewRepositoryStateClassification {
                            repository_snapshot_id: args.snapshot,
                            dirty_entry_id: args.dirty_entry,
                            classification: &args.classification,
                            reason: &args.reason,
                            acceptance_record_id: args.acceptance,
                        },
                    )?;
                    println!("classified repository state");
                    println!(
                        "repository_state_classification_id: {}",
                        outcome.repository_state_classification_id
                    );
                }
            },
            RepositoryCommand::Commit { command } => match command {
                RepositoryCommitCommand::Add(args) => {
                    let outcome = add_git_commit(
                        &root,
                        NewGitCommit {
                            repository: &args.repository,
                            commit_sha: &args.sha,
                            short_sha: args.short.as_deref(),
                            subject: args.subject.as_deref(),
                            author_name: args.author_name.as_deref(),
                            author_email: args.author_email.as_deref(),
                            committed_at: args.committed_at.as_deref(),
                            parent_shas: args.parents.as_deref(),
                        },
                    )?;
                    println!("added git commit");
                    println!("git_commit_id: {}", outcome.git_commit_id);
                    println!("repository_id: {}", outcome.repository_id);
                }
            },
            RepositoryCommand::File { command } => match command {
                RepositoryFileCommand::Add(args) => {
                    let outcome = add_git_file_change(
                        &root,
                        NewGitFileChange {
                            git_commit_id: args.commit,
                            repository: args.repository.as_deref(),
                            path: &args.path,
                            old_path: args.old_path.as_deref(),
                            change_type: &args.change_type,
                            additions: args.additions,
                            deletions: args.deletions,
                            content_hash: args.hash.as_deref(),
                        },
                    )?;
                    println!("added git file change");
                    println!("git_file_change_id: {}", outcome.git_file_change_id);
                    println!("repository_id: {}", outcome.repository_id);
                }
            },
            RepositoryCommand::Compare { command } => match command {
                RepositoryCompareCommand::Add(args) => {
                    let outcome = add_repository_snapshot_comparison(
                        &root,
                        NewRepositorySnapshotComparison {
                            base_repository_snapshot_id: args.base,
                            current_repository_snapshot_id: args.current,
                            comparison_type: &args.comparison_type,
                            head_changed: args.head_changed,
                            dirty_state_changed: args.dirty_changed,
                            nested_repository_changed: args.nested_changed,
                            result: &args.result,
                        },
                    )?;
                    println!("added repository snapshot comparison");
                    println!(
                        "repository_snapshot_comparison_id: {}",
                        outcome.repository_snapshot_comparison_id
                    );
                }
            },
        },
        Command::Task { command } => match command {
            TaskCommand::Add(args) => {
                let outcome = add_task(
                    &root,
                    NewTask {
                        title: &args.title,
                        priority: &args.priority,
                        source: &args.source,
                        work_unit_id: args.work_unit,
                        details: args.details.as_deref(),
                        completion_condition: args.completion_condition.as_deref(),
                    },
                )?;
                println!("added task");
                println!("task_id: {}", outcome.task_id);
                if let Some(work_unit_id) = outcome.work_unit_id {
                    println!("work_unit_id: {work_unit_id}");
                }
            }
            TaskCommand::List(args) => {
                let records = list_tasks(
                    &root,
                    TaskListQuery {
                        status: args.status.as_deref(),
                        work_unit_id: args.work_unit,
                    },
                )?;
                if records.is_empty() {
                    println!("no tasks");
                }
                for record in records {
                    let work_unit = record
                        .work_unit_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    println!(
                        "{} [work_unit={} {}:{}] {}",
                        record.id, work_unit, record.priority, record.status, record.title
                    );
                }
            }
            TaskCommand::Close(args) => {
                let outcome = close_task(&root, args.task_id, args.commit.as_deref())?;
                println!("closed task");
                println!("task_id: {}", outcome.task_id);
            }
            TaskCommand::AcceptOutOfScope(args) => {
                let outcome = accept_task_out_of_scope(&root, args.task_id, &args.reason)?;
                println!("accepted task out of scope");
                println!("task_id: {}", outcome.task_id);
                println!("acceptance_record_id: {}", outcome.acceptance_record_id);
                println!("authority_event_id: {}", outcome.authority_event_id);
            }
        },
        Command::Decision { command } => match command {
            DecisionCommand::Add(args) => {
                let outcome = add_decision(
                    &root,
                    NewDecision {
                        decision_key: args.key.as_deref(),
                        topic: &args.topic,
                        decision: &args.decision,
                        rationale: args.rationale.as_deref(),
                        compatibility_impact: args.compatibility_impact.as_deref(),
                        authority_refs: args.authority_refs.as_deref(),
                    },
                )?;
                println!("added decision");
                println!("decision_id: {}", outcome.decision_id);
            }
            DecisionCommand::List(args) => {
                print_decisions(list_decisions(&root, args.query.as_deref())?);
            }
            DecisionCommand::Search(args) => {
                print_decisions(list_decisions(&root, Some(&args.query))?);
            }
        },
        Command::Design { command } => match command {
            DesignCommand::Init(args) => {
                let title = args.title.as_deref().unwrap_or(&args.design_id);
                let outcome = init_design_package(
                    &root,
                    NewDesignPackage {
                        design_id: &args.design_id,
                        title,
                    },
                )?;
                println!("initialized design package");
                println!("path: {}", outcome.package_path.display());
            }
            DesignCommand::Import(args) => {
                let outcome = import_design_package(
                    &root,
                    DesignPackageImport {
                        package_path: &args.package_path,
                        status: &args.status,
                    },
                )?;
                println!("imported design package");
                println!("design_package_id: {}", outcome.design_package_id);
                println!("design_version_id: {}", outcome.design_version_id);
                println!("version_number: {}", outcome.version_number);
                println!("file_count: {}", outcome.file_count);
                println!("requirement_count: {}", outcome.requirement_count);
                println!("decision_count: {}", outcome.decision_count);
                println!(
                    "validation_gate_template_count: {}",
                    outcome.validation_gate_template_count
                );
                println!("warning_count: {}", outcome.warning_count);
                println!("content_hash: {}", outcome.content_hash);
            }
            DesignCommand::Approve(args) => {
                let outcome = approve_design_version(
                    &root,
                    DesignVersionApproval {
                        design_version_id: args.design_version_id,
                        summary: args.summary.as_deref(),
                    },
                )?;
                println!("approved design version");
                println!("design_package_id: {}", outcome.design_package_id);
                println!("design_version_id: {}", outcome.design_version_id);
                println!("authority_event_id: {}", outcome.authority_event_id);
            }
        },
        Command::Requirement { command } => match command {
            RequirementCommand::List(args) => {
                let records = list_design_requirements(
                    &root,
                    DesignRequirementListQuery {
                        design_version_id: args.design,
                    },
                )?;
                if records.is_empty() {
                    println!("no requirements");
                }
                for record in records {
                    println!(
                        "{} [{}:{} rev={}] {} ({})",
                        record.requirement_key,
                        record.priority,
                        record.status,
                        record.revision,
                        record.source_section,
                        record.source_path
                    );
                }
            }
        },
        Command::DesignDecision { command } => match command {
            DesignDecisionCommand::List(args) => {
                let records = list_design_decisions(
                    &root,
                    DesignDecisionListQuery {
                        design_version_id: args.design,
                    },
                )?;
                if records.is_empty() {
                    println!("no design decisions");
                }
                for record in records {
                    println!(
                        "{} [{}:{}] {} ({})",
                        record.decision_key,
                        record.topic,
                        record.status,
                        record.source_section,
                        record.source_path
                    );
                }
            }
        },
        Command::GateTemplate { command } => match command {
            GateTemplateCommand::List(args) => {
                let records = list_validation_gate_templates(
                    &root,
                    ValidationGateTemplateListQuery {
                        design_version_id: args.design,
                    },
                )?;
                if records.is_empty() {
                    println!("no validation gate templates");
                }
                for record in records {
                    let command = record.command.as_deref().unwrap_or("-");
                    println!(
                        "{} [{}:{} expected={} command={}] {} ({})",
                        record.gate_key,
                        record.stage,
                        record.status,
                        record.expected_result,
                        command,
                        record.source_section,
                        record.source_path
                    );
                }
            }
        },
        Command::Trace { command } => match command {
            TraceCommand::DeriveTask(args) => {
                let outcome = derive_task_from_requirement(
                    &root,
                    NewTaskDerivation {
                        design_version_id: args.design,
                        requirement_key: &args.requirement,
                        task_id: args.task,
                        derivation_reason: args.reason.as_deref(),
                        checklist_title: args.checklist_title.as_deref(),
                        item_title: args.item_title.as_deref(),
                        completion_condition: args.completion_condition.as_deref(),
                    },
                )?;
                println!("derived task from requirement");
                println!("task_derivation_id: {}", outcome.task_derivation_id);
                println!("checklist_id: {}", outcome.checklist_id);
                println!("checklist_item_id: {}", outcome.checklist_item_id);
                println!("design_requirement_id: {}", outcome.design_requirement_id);
                println!("task_id: {}", outcome.task_id);
            }
            TraceCommand::Derivation { command } => match command {
                TraceDerivationCommand::List(args) => {
                    let records = list_task_derivations(
                        &root,
                        TaskDerivationListQuery {
                            design_version_id: args.design,
                        },
                    )?;
                    if records.is_empty() {
                        println!("no task derivations");
                    }
                    for record in records {
                        let checklist_item = record
                            .checklist_item_id
                            .map(|id| id.to_string())
                            .unwrap_or_else(|| "-".to_string());
                        println!(
                            "{} [{}] requirement={} task={} checklist_item={} {}",
                            record.id,
                            record.status,
                            record.requirement_key,
                            record.task_id,
                            checklist_item,
                            record.task_title
                        );
                    }
                }
            },
        },
        Command::Evidence { command } => match command {
            EvidenceCommand::Add(args) => {
                let outcome = if args.repository_id.is_some()
                    || args.git_commit_id.is_some()
                    || args.git_file_change_id.is_some()
                {
                    add_implementation_evidence_with_git(
                        &root,
                        NewImplementationEvidenceWithGit {
                            task_id: args.task,
                            design_version_id: args.design,
                            requirement_key: args.requirement.as_deref(),
                            evidence_type: &args.evidence_type,
                            repository_id: args.repository_id,
                            git_commit_id: args.git_commit_id,
                            git_file_change_id: args.git_file_change_id,
                            commit_sha: args.commit.as_deref(),
                            file_path: args.file.as_deref(),
                            line_ref: args.line.as_deref(),
                            symbol: args.symbol.as_deref(),
                            artifact_path: args.artifact.as_deref(),
                            note: args.note.as_deref(),
                        },
                    )?
                } else {
                    add_implementation_evidence(
                        &root,
                        NewImplementationEvidence {
                            task_id: args.task,
                            design_version_id: args.design,
                            requirement_key: args.requirement.as_deref(),
                            evidence_type: &args.evidence_type,
                            commit_sha: args.commit.as_deref(),
                            file_path: args.file.as_deref(),
                            line_ref: args.line.as_deref(),
                            symbol: args.symbol.as_deref(),
                            artifact_path: args.artifact.as_deref(),
                            note: args.note.as_deref(),
                        },
                    )?
                };
                println!("added implementation evidence");
                println!(
                    "implementation_evidence_id: {}",
                    outcome.implementation_evidence_id
                );
                if let Some(task_id) = outcome.task_id {
                    println!("task_id: {task_id}");
                }
                if let Some(design_requirement_id) = outcome.design_requirement_id {
                    println!("design_requirement_id: {design_requirement_id}");
                }
            }
            EvidenceCommand::List(args) => {
                let records = list_implementation_evidence(
                    &root,
                    ImplementationEvidenceListQuery {
                        task_id: args.task,
                        design_version_id: args.design,
                    },
                )?;
                if records.is_empty() {
                    println!("no implementation evidence");
                }
                for record in records {
                    let task = record
                        .task_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    let requirement = record.requirement_key.as_deref().unwrap_or("-");
                    let detail = evidence_detail(&record);
                    println!(
                        "{} [{}] task={} requirement={} {}",
                        record.id, record.evidence_type, task, requirement, detail
                    );
                }
            }
        },
        Command::Coverage { command } => match command {
            CoverageCommand::Add(args) => {
                let outcome = add_coverage_item(
                    &root,
                    NewCoverageItem {
                        design_version_id: args.design,
                        requirement_key: &args.requirement,
                        review_scope_id: None,
                        work_unit_id: args.work_unit,
                        task_id: args.task,
                        requirement: &args.requirement_text,
                        runtime_boundary_evidence: args.runtime.as_deref(),
                        ux_boundary_evidence: args.ux.as_deref(),
                        lifecycle_boundary_evidence: args.lifecycle.as_deref(),
                        tests_or_gates: args.tests_or_gates.as_deref(),
                        missing_or_unverified: args.missing.as_deref(),
                        status: &args.status,
                    },
                )?;
                println!("added coverage item");
                println!("coverage_item_id: {}", outcome.coverage_item_id);
                println!("design_requirement_id: {}", outcome.design_requirement_id);
                if let Some(work_unit_id) = outcome.work_unit_id {
                    println!("work_unit_id: {work_unit_id}");
                }
                if let Some(task_id) = outcome.task_id {
                    println!("task_id: {task_id}");
                }
            }
            CoverageCommand::List(args) => {
                let records = list_coverage_items(
                    &root,
                    CoverageItemListQuery {
                        design_version_id: args.design,
                        status: args.status.as_deref(),
                    },
                )?;
                if records.is_empty() {
                    println!("no coverage items");
                }
                for record in records {
                    let work_unit = record
                        .work_unit_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    let task = record
                        .task_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    let detail = record
                        .missing_or_unverified
                        .as_deref()
                        .or(record.tests_or_gates.as_deref())
                        .unwrap_or("-");
                    println!(
                        "{} [{}] requirement={} work_unit={} task={} {}",
                        record.id, record.status, record.requirement_key, work_unit, task, detail
                    );
                }
            }
        },
        Command::Review { command } => match command {
            ReviewCommand::Scope { command } => match command {
                ReviewScopeCommand::Start(args) => {
                    let outcome = start_review_scope(
                        &root,
                        NewReviewScope {
                            name: &args.name,
                            review_type: &args.review_type,
                            scope: &args.scope,
                            allowed_inputs: None,
                            forbidden_judgments: None,
                            expected_output_type: None,
                            exclusions: None,
                            prompt_template_ref: None,
                        },
                    )?;
                    println!("started review scope");
                    println!("review_scope_id: {}", outcome.review_scope_id);
                }
                ReviewScopeCommand::List => {
                    let records = list_review_scopes(&root)?;
                    if records.is_empty() {
                        println!("no review scopes");
                    }
                    for record in records {
                        println!(
                            "{} [{}:{} role={} streak={}] {}",
                            record.id,
                            record.review_type,
                            record.status,
                            record.agent_role,
                            record.no_findings_streak,
                            record.name
                        );
                    }
                }
            },
            ReviewCommand::Policy { command } => match command {
                ReviewPolicyCommand::Add(args) => {
                    let outcome = add_review_policy(
                        &root,
                        NewReviewPolicy {
                            name: &args.name,
                            review_type: &args.review_type,
                            max_fresh_agents: args.max_fresh_agents,
                            max_resume_agents: args.max_resume_agents,
                            max_parallel_agents: args.max_parallel_agents,
                            required_consecutive_clean_fresh_runs: args.fresh_clean,
                            required_consecutive_clean_resume_runs: args.resume_clean,
                            stop_on_severity: &args.stop_on_severity,
                            allow_resume_review: true,
                            allow_fresh_review: true,
                            allow_new_findings_in_resume: args.allow_new_findings_in_resume,
                            on_max_agents_exceeded: &args.on_max_agents_exceeded,
                            run_count_scope: &args.run_count_scope,
                            default_run_mode: &args.default_run_mode,
                        },
                    )?;
                    println!("added review policy");
                    println!("review_policy_id: {}", outcome.review_policy_id);
                }
                ReviewPolicyCommand::List => {
                    let records = list_review_policies(&root)?;
                    if records.is_empty() {
                        println!("no review policies");
                    }
                    for record in records {
                        println!(
                            "{} [{} fresh_clean={} resume_clean={} max_fresh={} max_resume={} max_parallel={} resume_new={} count_scope={} default_mode={}] {}",
                            record.id,
                            record.review_type,
                            record.required_consecutive_clean_fresh_runs,
                            record.required_consecutive_clean_resume_runs,
                            record.max_fresh_agents,
                            record.max_resume_agents,
                            record.max_parallel_agents,
                            record.allow_new_findings_in_resume,
                            record.run_count_scope,
                            record.default_run_mode,
                            record.name
                        );
                    }
                }
            },
            ReviewCommand::Plan { command } => match command {
                ReviewPlanCommand::Add(args) => {
                    let outcome = add_review_plan(
                        &root,
                        NewReviewPlan {
                            work_unit_id: args.work_unit,
                            design_version_id: args.design_version,
                            review_type: &args.review_type,
                            required: args.required,
                            stage: &args.stage,
                            scope: args.scope.as_deref(),
                            clean_condition: None,
                            stop_condition: None,
                            review_policy_id: args.policy,
                            review_scope_id: args.review_scope,
                        },
                    )?;
                    println!("added review plan");
                    println!("review_plan_id: {}", outcome.review_plan_id);
                    if let Some(review_policy_id) = outcome.review_policy_id {
                        println!("review_policy_id: {review_policy_id}");
                    }
                }
                ReviewPlanCommand::List => {
                    let records = list_review_plans(&root)?;
                    if records.is_empty() {
                        println!("no review plans");
                    }
                    for record in records {
                        println!(
                            "{} [{}:{} required={}] work_unit={} stage={}",
                            record.id,
                            record.review_type,
                            record.status,
                            record.required,
                            record.work_unit_id,
                            record.stage
                        );
                    }
                }
                ReviewPlanCommand::Context(args) => {
                    let targets = list_review_plan_targets(&root, args.review_plan_id)?;
                    println!("review_plan_id: {}", args.review_plan_id);
                    if targets.is_empty() {
                        println!("no review plan targets");
                    }
                    for target in targets {
                        println!(
                            "target {} [{}] {}",
                            target.id,
                            target.target_type,
                            review_target_detail(&target)
                        );
                    }
                }
            },
            ReviewCommand::Run { command } => match command {
                ReviewRunCommand::Add(args) => {
                    let outcome = add_review_run(
                        &root,
                        NewReviewRun {
                            review_plan_id: args.plan,
                            run_type: &args.run_type,
                            run_purpose: &args.purpose,
                            target_ref: args.target.as_deref(),
                            prompt_deviations: None,
                            result_summary: args.summary.as_deref(),
                            new_findings_count: args.new_findings,
                            carried_findings_checked: args.carried_findings,
                            clean_run: args.clean,
                            status: &args.status,
                            agent_label: args.agent_label.as_deref(),
                            external_agent_id: args.external_agent_id.as_deref(),
                        },
                    )?;
                    println!("added review run");
                    println!("review_run_id: {}", outcome.review_run_id);
                    println!(
                        "review_agent_invocation_id: {}",
                        outcome.review_agent_invocation_id
                    );
                    println!("review_plan_id: {}", outcome.review_plan_id);
                    println!("plan_status: {}", outcome.plan_status);
                }
                ReviewRunCommand::List(args) => {
                    let records = list_review_runs(&root, args.plan)?;
                    if records.is_empty() {
                        println!("no review runs");
                    }
                    for record in records {
                        let target = record.target_ref.as_deref().unwrap_or("-");
                        println!(
                            "{} [plan={} {}:{} clean={}] target={}",
                            record.id,
                            record
                                .review_plan_id
                                .map(|id| id.to_string())
                                .unwrap_or_else(|| "-".to_string()),
                            record.run_type,
                            record.status,
                            record.clean_run,
                            target
                        );
                    }
                }
            },
        },
        Command::Finding { command } => match command {
            FindingCommand::Add(args) => {
                let outcome = add_finding(
                    &root,
                    NewFinding {
                        review_run_id: args.run,
                        finding_type: &args.finding_type,
                        severity: &args.severity,
                        description: &args.description,
                        design_requirement_id: args.design_requirement,
                        task_id: args.task,
                    },
                )?;
                println!("added finding");
                println!("finding_id: {}", outcome.finding_id);
            }
            FindingCommand::Classify(args) => {
                let outcome = classify_finding(&root, args.finding_id, &args.classification)?;
                println!("classified finding");
                println!("finding_id: {}", outcome.finding_id);
            }
            FindingCommand::List(args) => {
                let records = list_findings(&root, args.status.as_deref())?;
                if records.is_empty() {
                    println!("no findings");
                }
                for record in records {
                    println!(
                        "{} [run={} {}:{} {}] {}",
                        record.id,
                        record.review_run_id,
                        record.finding_type,
                        record.severity,
                        record.status,
                        record.description
                    );
                }
            }
            FindingCommand::Verify(args) => {
                let outcome = add_finding_verification(
                    &root,
                    NewFindingVerification {
                        review_run_id: args.run,
                        finding_id: args.finding,
                        closure_id: args.closure,
                        result: &args.result,
                        notes: args.notes.as_deref(),
                    },
                )?;
                println!("added finding verification");
                println!(
                    "finding_verification_id: {}",
                    outcome.finding_verification_id
                );
            }
        },
        Command::Closure { command } => match command {
            ClosureCommand::Add(args) => {
                let outcome = add_closure(
                    &root,
                    NewClosure {
                        finding_id: args.finding,
                        design_invariant: &args.invariant,
                        design_citations: args.citations.as_deref(),
                        implementation_evidence: args.evidence.as_deref(),
                        affected_surfaces: args.surfaces.as_deref(),
                        same_invariant_search: args.search.as_deref(),
                        other_violations_found: args.other_violations.as_deref(),
                        fix_plan: args.fix_plan.as_deref(),
                        tests_or_gates: args.tests.as_deref(),
                        verification_plan: args.verification.as_deref(),
                        closed_by_commit: args.commit.as_deref(),
                    },
                )?;
                println!("added closure");
                println!("closure_id: {}", outcome.closure_id);
            }
        },
        Command::Acceptance { command } => match command {
            AcceptanceCommand::Add(args) => {
                let outcome = accept_design_exception(
                    &root,
                    NewDesignExceptionAcceptance {
                        design_version_id: args.design,
                        design_package: args.package.as_deref(),
                        target: &args.target,
                        acceptance_type: &args.acceptance_type,
                        reason: &args.reason,
                    },
                )?;
                println!("accepted design exception");
                println!("acceptance_record_id: {}", outcome.acceptance_record_id);
                println!("authority_event_id: {}", outcome.authority_event_id);
                println!("target_type: {}", outcome.target_type);
                if let Some(design_requirement_id) = outcome.design_requirement_id {
                    println!("design_requirement_id: {design_requirement_id}");
                }
                if let Some(validation_gate_template_id) = outcome.validation_gate_template_id {
                    println!("validation_gate_template_id: {validation_gate_template_id}");
                }
                if let Some(coverage_item_id) = outcome.coverage_item_id {
                    println!("coverage_item_id: {coverage_item_id}");
                }
                if let Some(design_package_key) = outcome.design_package_key {
                    println!("design_package_key: {design_package_key}");
                }
                if let Some(design_file_path) = outcome.design_file_path {
                    println!("design_file_path: {design_file_path}");
                }
                if let Some(design_requirement_key) = outcome.design_requirement_key {
                    println!("design_requirement_key: {design_requirement_key}");
                }
            }
        },
        Command::Authority { command } => match command {
            AuthorityCommand::Event { command } => match command {
                AuthorityEventCommand::Add(args) => {
                    let outcome = add_authority_event(
                        &root,
                        NewAuthorityEvent {
                            event_type: &args.event_type,
                            source: args.source.as_deref(),
                            summary: &args.summary,
                            scope: args.scope.as_deref(),
                            precedence: args.precedence,
                        },
                    )?;
                    println!("added authority event");
                    println!("authority_event_id: {}", outcome.authority_event_id);
                }
            },
            AuthorityCommand::List(args) => {
                let records = list_authority_events(&root, args.scope.as_deref())?;
                if records.is_empty() {
                    println!("no authority events");
                }
                for record in records {
                    let scope = record.scope.as_deref().unwrap_or("-");
                    println!(
                        "{} [{} scope={} precedence={}] {}",
                        record.id, record.event_type, scope, record.precedence, record.summary
                    );
                }
            }
        },
        Command::Kpt { command } => match command {
            KptCommand::Start(args) => {
                let outcome = start_kpt_review(
                    &root,
                    NewKptReview {
                        scope: args.scope.as_deref(),
                        summary: args.summary.as_deref(),
                        from: args.from.as_deref(),
                        period: args.period.as_deref(),
                    },
                )?;
                println!("started kpt review");
                println!("kpt_review_id: {}", outcome.kpt_review_id);
                println!("generated_item_count: {}", outcome.generated_item_count);
            }
            KptCommand::List(args) => {
                let records = list_kpt_reviews(&root, args.status.as_deref())?;
                if records.is_empty() {
                    println!("no kpt reviews");
                }
                for record in records {
                    let scope = record.scope.as_deref().unwrap_or("-");
                    let summary = record.summary.as_deref().unwrap_or("");
                    println!(
                        "{} [scope={} {}] {}",
                        record.id, scope, record.status, summary
                    );
                }
            }
            KptCommand::Close(args) => {
                let outcome = close_kpt_review(&root, args.kpt_review_id)?;
                println!("closed kpt review");
                println!("kpt_review_id: {}", outcome.kpt_review_id);
            }
            KptCommand::Item { command } => match command {
                KptItemCommand::Add(args) => {
                    let outcome = add_kpt_item(
                        &root,
                        NewKptItem {
                            kpt_review_id: args.review,
                            item_type: &args.item_type,
                            title: &args.title,
                            details: args.details.as_deref(),
                            severity: &args.severity,
                            proposed_action: args.proposed_action.as_deref(),
                        },
                    )?;
                    println!("added kpt item");
                    println!("kpt_item_id: {}", outcome.kpt_item_id);
                    println!("kpt_review_id: {}", outcome.kpt_review_id);
                }
                KptItemCommand::List(args) => {
                    let records = list_kpt_items(&root, args.review)?;
                    if records.is_empty() {
                        println!("no kpt items");
                    }
                    for record in records {
                        let task = record
                            .linked_task_id
                            .map(|id| id.to_string())
                            .unwrap_or_else(|| "-".to_string());
                        println!(
                            "{} [review={} {}:{} task={}] {}",
                            record.id,
                            record.kpt_review_id,
                            record.item_type,
                            record.status,
                            task,
                            record.title
                        );
                    }
                }
                KptItemCommand::Convert(args) => match args.target_type.as_str() {
                    "task" => {
                        let outcome = convert_kpt_item_to_task(
                            &root,
                            KptItemTaskConversion {
                                kpt_item_id: args.item,
                                task_title: args.title.as_deref(),
                                details: args.details.as_deref(),
                                priority: &args.priority,
                                work_unit_id: args.work_unit,
                            },
                        )?;
                        println!("converted kpt item");
                        println!("kpt_item_conversion_id: {}", outcome.kpt_item_conversion_id);
                        println!("task_id: {}", outcome.task_id);
                    }
                    "review-policy" | "review_policy" => {
                        let review_type = args
                            .review_type
                            .as_deref()
                            .ok_or_else(|| anyhow::anyhow!("--review-type is required"))?;
                        let outcome = convert_kpt_item_to_review_policy(
                            &root,
                            KptItemReviewPolicyConversion {
                                kpt_item_id: args.item,
                                name: args.name.as_deref().or(args.title.as_deref()),
                                review_type,
                                max_fresh_agents: args.max_fresh_agents,
                                max_resume_agents: args.max_resume_agents,
                                max_parallel_agents: args.max_parallel_agents,
                                required_consecutive_clean_fresh_runs: args.fresh_clean,
                                required_consecutive_clean_resume_runs: args.resume_clean,
                                stop_on_severity: &args.stop_on_severity,
                                allow_new_findings_in_resume: args.allow_new_findings_in_resume,
                                run_count_scope: &args.run_count_scope,
                                default_run_mode: &args.default_run_mode,
                                on_max_agents_exceeded: &args.on_max_agents_exceeded,
                            },
                        )?;
                        println!("converted kpt item");
                        println!("kpt_item_conversion_id: {}", outcome.kpt_item_conversion_id);
                        println!("review_policy_id: {}", outcome.review_policy_id);
                    }
                    "command-profile" | "command_profile" => {
                        let outcome = convert_kpt_item_to_command_profile(
                            &root,
                            KptItemCommandProfileConversion {
                                kpt_item_id: args.item,
                                name: args.name.as_deref().or(args.title.as_deref()),
                                command: args.command.as_deref().or(args.details.as_deref()),
                                command_type: &args.command_type,
                                scope: args.scope.as_deref(),
                                status: &args.command_status,
                                stability: &args.stability,
                                timeout: args.timeout.as_deref(),
                                expected_result: args.expected_result.as_deref(),
                            },
                        )?;
                        println!("converted kpt item");
                        println!("kpt_item_conversion_id: {}", outcome.kpt_item_conversion_id);
                        println!("command_profile_id: {}", outcome.command_profile_id);
                    }
                    "decision" => {
                        let outcome = convert_kpt_item_to_decision(
                            &root,
                            KptItemDecisionConversion {
                                kpt_item_id: args.item,
                                decision_key: args.decision_key.as_deref(),
                                topic: args.title.as_deref(),
                                decision: args.details.as_deref(),
                                rationale: args.rationale.as_deref(),
                                compatibility_impact: args.compatibility_impact.as_deref(),
                                authority_refs: args.authority_refs.as_deref(),
                            },
                        )?;
                        println!("converted kpt item");
                        println!("kpt_item_conversion_id: {}", outcome.kpt_item_conversion_id);
                        println!("decision_id: {}", outcome.decision_id);
                    }
                    "design-version" | "design_version" => {
                        let design_version_id = args
                            .design_version
                            .ok_or_else(|| anyhow::anyhow!("--design-version is required"))?;
                        let outcome = convert_kpt_item_to_design_version(
                            &root,
                            KptItemDesignVersionConversion {
                                kpt_item_id: args.item,
                                design_version_id,
                            },
                        )?;
                        println!("converted kpt item");
                        println!("kpt_item_conversion_id: {}", outcome.kpt_item_conversion_id);
                        println!("design_version_id: {}", outcome.design_version_id);
                    }
                    other => anyhow::bail!("unsupported kpt item conversion target: {other}"),
                },
            },
        },
    }

    Ok(())
}

fn print_decisions(records: Vec<agent_workbench::DecisionRecord>) {
    if records.is_empty() {
        println!("no decisions");
    }
    for record in records {
        let key = record.decision_key.as_deref().unwrap_or("-");
        println!(
            "{} [{}:{}] {}",
            record.id, key, record.status, record.decision
        );
    }
}

fn review_target_detail(target: &agent_workbench::ReviewPlanTargetRecord) -> String {
    if let Some(id) = target.design_version_id {
        return format!("design_version_id={id}");
    }
    if let Some(id) = target.design_requirement_id {
        return format!("design_requirement_id={id}");
    }
    if let Some(id) = target.task_id {
        return format!("task_id={id}");
    }
    if let Some(id) = target.work_unit_id {
        return format!("work_unit_id={id}");
    }
    if let Some(id) = target.repository_snapshot_id {
        return format!("repository_snapshot_id={id}");
    }
    if let Some(path) = &target.file_path {
        return format!("file_path={path}");
    }
    if let Some(symbol) = &target.symbol {
        return format!("symbol={symbol}");
    }
    "-".to_string()
}

fn evidence_detail(record: &ImplementationEvidenceRecord) -> String {
    if let Some(commit_sha) = &record.commit_sha {
        return format!("commit={commit_sha}");
    }
    if let Some(file_path) = &record.file_path {
        let line = record
            .line_ref
            .as_ref()
            .map(|value| format!(":{value}"))
            .unwrap_or_default();
        return format!("file={file_path}{line}");
    }
    if let Some(symbol) = &record.symbol {
        return format!("symbol={symbol}");
    }
    if let Some(artifact_path) = &record.artifact_path {
        return format!("artifact={artifact_path}");
    }
    record
        .note
        .as_ref()
        .map(|note| format!("note={note}"))
        .unwrap_or_else(|| "detail=-".to_string())
}
