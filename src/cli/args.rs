use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "agent-workbench")]
#[command(about = "Structured local workbench for long-running coding-agent work")]
pub(crate) struct Cli {
    #[arg(long, global = true, value_name = "PATH")]
    pub(crate) root: Option<PathBuf>,

    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
#[allow(clippy::enum_variant_names)]
pub(crate) enum Command {
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
    /// Record Git evidence using the design-level CLI spelling.
    Git {
        #[command(subcommand)]
        command: GitCommand,
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
    /// Manage work phases inside aggregate work units.
    Phase {
        #[command(subcommand)]
        command: PhaseCommand,
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
    /// Decompose imported design versions into executable work.
    Decompose {
        #[command(subcommand)]
        command: DecomposeCommand,
    },
    /// List generated checklists.
    Checklist {
        #[command(subcommand)]
        command: ChecklistCommand,
    },
    /// List stale design-derived records.
    Stale {
        #[command(subcommand)]
        command: StaleCommand,
    },
    /// Print focused context for review agents.
    ReviewContext(ReviewContextArgs),
    /// Export human-readable views.
    Export {
        #[command(subcommand)]
        command: ExportCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum WorkCommand {
    /// Start a new active work unit.
    Start(WorkStartArgs),
    /// Activate an existing open work unit.
    Activate(WorkActivateArgs),
    /// Mark an open work unit as blocked.
    Block(WorkBlockArgs),
    /// Mark a blocked work unit as open.
    Unblock(WorkUnblockArgs),
    /// Suspend the active work unit.
    Suspend(WorkSuspendArgs),
    /// Interrupt active work with a child work unit.
    Interrupt(WorkInterruptArgs),
    /// Resume a suspended activation using an allowed resume check.
    Resume(WorkResumeArgs),
    /// Close the active work unit.
    Close(WorkCloseArgs),
    /// Abandon an open, blocked, or closed work unit.
    Abandon(WorkAbandonArgs),
    /// Fork work from a prior record, activation, or commit.
    Fork(WorkForkArgs),
    /// Reopen a closed or abandoned work unit.
    Reopen(WorkReopenArgs),
    /// Create follow-up work linked to a closed or abandoned work unit.
    FollowUp(WorkFollowUpArgs),
}

#[derive(Debug, Args)]
pub(crate) struct WorkStartArgs {
    pub(crate) title: String,
    #[arg(long)]
    pub(crate) responsibility: Option<String>,
    #[arg(long)]
    pub(crate) design_version: Option<i64>,
    #[arg(long)]
    pub(crate) implementation: bool,
}

#[derive(Debug, Args)]
pub(crate) struct WorkActivateArgs {
    pub(crate) work_unit_id: i64,
    #[arg(long)]
    pub(crate) design_version: Option<i64>,
    #[arg(long)]
    pub(crate) implementation: bool,
    #[arg(long)]
    pub(crate) reason: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct WorkBlockArgs {
    pub(crate) work_unit_id: Option<i64>,
    #[arg(long)]
    pub(crate) reason: String,
}

#[derive(Debug, Args)]
pub(crate) struct WorkUnblockArgs {
    pub(crate) work_unit_id: Option<i64>,
    #[arg(long)]
    pub(crate) reason: String,
}

#[derive(Debug, Args)]
pub(crate) struct WorkSuspendArgs {
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) next: String,
}

#[derive(Debug, Args)]
pub(crate) struct WorkInterruptArgs {
    pub(crate) title: String,
    #[arg(long)]
    pub(crate) reason: String,
}

#[derive(Debug, Args)]
pub(crate) struct WorkResumeArgs {
    #[arg(long)]
    pub(crate) check: i64,
}

#[derive(Debug, Args)]
pub(crate) struct WorkCloseArgs {
    #[arg(long)]
    pub(crate) summary: String,
    #[arg(long)]
    pub(crate) commit: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct WorkAbandonArgs {
    pub(crate) work_unit_id: Option<i64>,
    #[arg(long)]
    pub(crate) reason: String,
}

#[derive(Debug, Args)]
pub(crate) struct WorkForkArgs {
    pub(crate) title: String,
    #[arg(long)]
    pub(crate) from_record: Option<i64>,
    #[arg(long)]
    pub(crate) from_activation: Option<i64>,
    #[arg(long)]
    pub(crate) from_commit: Option<String>,
    #[arg(long)]
    pub(crate) from_git_commit_id: Option<i64>,
    #[arg(long)]
    pub(crate) from_snapshot: Option<i64>,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long, default_value = "keep_history")]
    pub(crate) discard_policy: String,
}

#[derive(Debug, Args)]
pub(crate) struct WorkReopenArgs {
    pub(crate) work_unit_id: i64,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long, default_value = "closure_invalid")]
    pub(crate) reason_type: String,
    #[arg(long)]
    pub(crate) authority: Option<i64>,
    #[arg(long)]
    pub(crate) acceptance: Option<i64>,
}

#[derive(Debug, Args)]
pub(crate) struct WorkFollowUpArgs {
    pub(crate) source_work_unit_id: i64,
    pub(crate) title: String,
    #[arg(long)]
    pub(crate) reason: String,
}

#[derive(Debug, Args)]
pub(crate) struct ResumeCheckArgs {
    #[arg(long, default_value = "basic")]
    pub(crate) maturity: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum GateCommand {
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
pub(crate) struct GateResumeReadyArgs {
    #[arg(long, default_value = "basic")]
    pub(crate) maturity: String,
    #[arg(long)]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Args)]
pub(crate) struct GateCloseReadyArgs {
    #[arg(long)]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Args)]
pub(crate) struct GateDesignReadyArgs {
    #[arg(long)]
    pub(crate) design_version: Option<i64>,
    #[arg(long)]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Args)]
pub(crate) struct GateImplementationReadyArgs {
    #[arg(long)]
    pub(crate) design_version: Option<i64>,
    #[arg(long)]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Args)]
pub(crate) struct GateSelectArgs {
    #[arg(long)]
    pub(crate) design: i64,
    #[arg(long)]
    pub(crate) template: String,
    #[arg(long)]
    pub(crate) requirement: String,
    #[arg(long)]
    pub(crate) task: i64,
    #[arg(long)]
    pub(crate) command: Option<String>,
    #[arg(long)]
    pub(crate) command_profile: Option<String>,
    #[arg(long)]
    pub(crate) timeout: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct GateRecordArgs {
    #[arg(long)]
    pub(crate) gate: i64,
    #[arg(long)]
    pub(crate) result: String,
    #[arg(long)]
    pub(crate) usage: Option<i64>,
    #[arg(long)]
    pub(crate) snapshot: Option<i64>,
    #[arg(long)]
    pub(crate) command: Option<String>,
    #[arg(long)]
    pub(crate) acceptance: Option<i64>,
    #[arg(long)]
    pub(crate) artifact: Option<String>,
    #[arg(long)]
    pub(crate) artifact_hash: Option<String>,
    #[arg(long)]
    pub(crate) notes: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum GateRunCommand {
    List(GateRunListArgs),
}

#[derive(Debug, Args)]
pub(crate) struct GateRunListArgs {
    #[arg(long)]
    pub(crate) gate: Option<i64>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum CorrectionCommand {
    Add(CorrectionAddArgs),
    List(CorrectionListArgs),
}

#[derive(Debug, Args)]
pub(crate) struct CorrectionAddArgs {
    #[arg(long)]
    pub(crate) scope: String,
    #[arg(long = "type")]
    pub(crate) correction_type: String,
    #[arg(long)]
    pub(crate) pattern: String,
    #[arg(long)]
    pub(crate) correction: String,
    #[arg(long, default_value = "project")]
    pub(crate) applies_to: String,
    #[arg(long, default_value = "medium")]
    pub(crate) severity: String,
}

#[derive(Debug, Args)]
pub(crate) struct CorrectionListArgs {
    #[arg(long)]
    pub(crate) scope: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum MemoryCommand {
    Fixed {
        #[command(subcommand)]
        command: FixedCommand,
    },
    Prefer(CommandPreferArgs),
    Deprecate(CommandDeprecateArgs),
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
pub(crate) enum FixedCommand {
    Add(CommandFixedAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct CommandFixedAddArgs {
    #[arg(long)]
    pub(crate) name: String,
    #[arg(long = "type")]
    pub(crate) command_type: String,
    #[arg(long)]
    pub(crate) scope: String,
    #[arg(long)]
    pub(crate) command: String,
    #[arg(long)]
    pub(crate) timeout: Option<String>,
    #[arg(long)]
    pub(crate) expected_result: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct CommandPreferArgs {
    #[arg(long)]
    pub(crate) name: String,
    #[arg(long = "type", default_value = "validation")]
    pub(crate) command_type: String,
    #[arg(long, default_value = "project")]
    pub(crate) scope: String,
    #[arg(long)]
    pub(crate) command: String,
    #[arg(long)]
    pub(crate) timeout: Option<String>,
    #[arg(long)]
    pub(crate) expected_result: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct CommandDeprecateArgs {
    #[arg(long)]
    pub(crate) name: String,
    #[arg(long)]
    pub(crate) reason: String,
}

#[derive(Debug, Args)]
pub(crate) struct CommandListArgs {
    #[arg(long = "type")]
    pub(crate) command_type: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum CommandUsageCommand {
    Add(CommandUsageAddArgs),
    List(CommandUsageListArgs),
    Promote(CommandUsagePromoteArgs),
}

#[derive(Debug, Args)]
pub(crate) struct CommandUsageAddArgs {
    #[arg(long)]
    pub(crate) profile: Option<String>,
    #[arg(long)]
    pub(crate) command: Option<String>,
    #[arg(long, default_value = "unknown")]
    pub(crate) result: String,
    #[arg(long)]
    pub(crate) log: Option<String>,
    #[arg(long)]
    pub(crate) work_unit: Option<i64>,
    #[arg(long)]
    pub(crate) snapshot: Option<i64>,
}

#[derive(Debug, Args)]
pub(crate) struct CommandUsageListArgs {
    #[arg(long)]
    pub(crate) profile: Option<String>,
    #[arg(long)]
    pub(crate) work_unit: Option<i64>,
}

#[derive(Debug, Args)]
pub(crate) struct CommandUsagePromoteArgs {
    pub(crate) usage_id: i64,
    #[arg(long)]
    pub(crate) name: String,
    #[arg(long = "type", default_value = "validation")]
    pub(crate) command_type: String,
    #[arg(long, default_value = "project")]
    pub(crate) scope: String,
    #[arg(long, default_value = "preferred")]
    pub(crate) status: String,
    #[arg(long)]
    pub(crate) timeout: Option<String>,
    #[arg(long)]
    pub(crate) expected_result: Option<String>,
    #[arg(long)]
    pub(crate) authority: Option<i64>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum CommandDeviationCommand {
    Add(CommandDeviationAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct CommandDeviationAddArgs {
    #[arg(long)]
    pub(crate) profile: String,
    #[arg(long)]
    pub(crate) usage: Option<i64>,
    #[arg(long)]
    pub(crate) reason: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum RulesCommand {
    Applicable(RulesApplicableArgs),
}

#[derive(Debug, Args)]
pub(crate) struct RulesApplicableArgs {
    #[arg(long)]
    pub(crate) scope: Option<String>,
    #[arg(long)]
    pub(crate) work_unit: Option<i64>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum WorkRecordCommand {
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
    Link {
        #[command(subcommand)]
        command: WorkRecordLinkCommand,
    },
}

#[derive(Debug, Args)]
pub(crate) struct WorkRecordCreateArgs {
    #[arg(long)]
    pub(crate) topic: String,
    #[arg(long)]
    pub(crate) work_performed: Option<String>,
    #[arg(long)]
    pub(crate) next_actions: Option<String>,
    #[arg(long)]
    pub(crate) notable_operations: Option<String>,
    #[arg(long)]
    pub(crate) work_unit: Option<i64>,
    #[arg(long, alias = "export-md")]
    pub(crate) export_path: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct WorkRecordListArgs {
    #[arg(long)]
    pub(crate) work_unit: Option<i64>,
}

#[derive(Debug, Args)]
pub(crate) struct WorkRecordExportArgs {
    pub(crate) work_record_id: i64,
    #[arg(long, default_value = "md")]
    pub(crate) format: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum WorkRecordCommandLinkCommand {
    Add(WorkRecordCommandAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct WorkRecordCommandAddArgs {
    pub(crate) work_record_id: Option<i64>,
    #[arg(long = "record")]
    pub(crate) record_id: Option<i64>,
    #[arg(long)]
    pub(crate) usage: Option<i64>,
    #[arg(long)]
    pub(crate) command: Option<String>,
    #[arg(long)]
    pub(crate) result: Option<String>,
    #[arg(long)]
    pub(crate) profile: Option<i64>,
    #[arg(long)]
    pub(crate) log_path: Option<String>,
    #[arg(long)]
    pub(crate) note: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum WorkRecordCommitCommand {
    Add(WorkRecordCommitAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct WorkRecordCommitAddArgs {
    pub(crate) work_record_id: Option<i64>,
    #[arg(long = "record")]
    pub(crate) record_id: Option<i64>,
    #[arg(long)]
    pub(crate) git_commit: Option<i64>,
    #[arg(long, alias = "commit")]
    pub(crate) sha: String,
    #[arg(long, default_value = "referenced")]
    pub(crate) role: String,
    #[arg(long)]
    pub(crate) note: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum WorkRecordFileCommand {
    Add(WorkRecordFileAddArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum WorkRecordLinkCommand {
    Command(WorkRecordCommandAddArgs),
    Commit(WorkRecordCommitAddArgs),
    File(WorkRecordFileAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct WorkRecordFileAddArgs {
    pub(crate) work_record_id: Option<i64>,
    #[arg(long = "record")]
    pub(crate) record_id: Option<i64>,
    #[arg(long)]
    pub(crate) git_file_change: Option<i64>,
    #[arg(long)]
    pub(crate) repository_id: Option<i64>,
    #[arg(long)]
    pub(crate) path: String,
    #[arg(long, default_value = "changed")]
    pub(crate) role: String,
    #[arg(long)]
    pub(crate) note: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum RepositoryCommand {
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
pub(crate) struct RepositoryAddArgs {
    pub(crate) name: String,
    #[arg(long)]
    pub(crate) path: String,
    #[arg(long)]
    pub(crate) head: Option<String>,
    #[arg(long)]
    pub(crate) status: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum RepositorySnapshotCommand {
    Add(RepositorySnapshotAddArgs),
    List(RepositorySnapshotListArgs),
}

#[derive(Debug, Args)]
pub(crate) struct RepositorySnapshotAddArgs {
    #[arg(long)]
    pub(crate) repository: String,
    #[arg(long)]
    pub(crate) activation: Option<i64>,
    #[arg(long)]
    pub(crate) head: Option<String>,
    #[arg(long)]
    pub(crate) branch: Option<String>,
    #[arg(long)]
    pub(crate) status: Option<String>,
    #[arg(long)]
    pub(crate) clean: bool,
}

#[derive(Debug, Args)]
pub(crate) struct RepositorySnapshotListArgs {
    #[arg(long)]
    pub(crate) repository: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum RepositoryDirtyCommand {
    Add(RepositoryDirtyAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct RepositoryDirtyAddArgs {
    #[arg(long)]
    pub(crate) snapshot: i64,
    #[arg(long)]
    pub(crate) path: String,
    #[arg(long = "type")]
    pub(crate) change_type: String,
    #[arg(long)]
    pub(crate) staged: bool,
    #[arg(long)]
    pub(crate) hash: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum RepositoryClassifyCommand {
    Add(RepositoryClassifyAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct RepositoryClassifyAddArgs {
    #[arg(long)]
    pub(crate) snapshot: i64,
    #[arg(long)]
    pub(crate) dirty_entry: Option<i64>,
    #[arg(long)]
    pub(crate) classification: String,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) acceptance: Option<i64>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum RepositoryCommitCommand {
    Add(RepositoryCommitAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct RepositoryCommitAddArgs {
    #[arg(long)]
    pub(crate) repository: String,
    #[arg(long, alias = "commit")]
    pub(crate) sha: String,
    #[arg(long)]
    pub(crate) short: Option<String>,
    #[arg(long)]
    pub(crate) subject: Option<String>,
    #[arg(long)]
    pub(crate) author_name: Option<String>,
    #[arg(long)]
    pub(crate) author_email: Option<String>,
    #[arg(long)]
    pub(crate) committed_at: Option<String>,
    #[arg(long)]
    pub(crate) parents: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum RepositoryFileCommand {
    Add(RepositoryFileAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct RepositoryFileAddArgs {
    #[arg(long)]
    pub(crate) commit: i64,
    #[arg(long)]
    pub(crate) repository: Option<String>,
    #[arg(long)]
    pub(crate) path: String,
    #[arg(long)]
    pub(crate) old_path: Option<String>,
    #[arg(long = "type")]
    pub(crate) change_type: String,
    #[arg(long)]
    pub(crate) additions: Option<i64>,
    #[arg(long)]
    pub(crate) deletions: Option<i64>,
    #[arg(long)]
    pub(crate) hash: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum RepositoryCompareCommand {
    Add(RepositoryCompareAddArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum GitCommand {
    Commit {
        #[command(subcommand)]
        command: GitCommitCommand,
    },
    Files {
        #[command(subcommand)]
        command: GitFileCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum GitCommitCommand {
    Add(GitCommitAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct GitCommitAddArgs {
    pub(crate) sha_arg: Option<String>,
    #[arg(long, alias = "repo")]
    pub(crate) repository: String,
    #[arg(long, alias = "commit")]
    pub(crate) sha: Option<String>,
    #[arg(long)]
    pub(crate) short: Option<String>,
    #[arg(long)]
    pub(crate) subject: Option<String>,
    #[arg(long)]
    pub(crate) author_name: Option<String>,
    #[arg(long)]
    pub(crate) author_email: Option<String>,
    #[arg(long)]
    pub(crate) committed_at: Option<String>,
    #[arg(long)]
    pub(crate) parents: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum GitFileCommand {
    Add(GitFileAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct GitFileAddArgs {
    #[arg(long)]
    pub(crate) commit: String,
    #[arg(long)]
    pub(crate) repository: Option<String>,
    #[arg(long)]
    pub(crate) path: String,
    #[arg(long)]
    pub(crate) old_path: Option<String>,
    #[arg(long = "type")]
    pub(crate) change_type: String,
    #[arg(long)]
    pub(crate) additions: Option<i64>,
    #[arg(long)]
    pub(crate) deletions: Option<i64>,
    #[arg(long)]
    pub(crate) hash: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct RepositoryCompareAddArgs {
    #[arg(long)]
    pub(crate) base: i64,
    #[arg(long)]
    pub(crate) current: i64,
    #[arg(long = "type")]
    pub(crate) comparison_type: String,
    #[arg(long)]
    pub(crate) head_changed: bool,
    #[arg(long)]
    pub(crate) dirty_changed: bool,
    #[arg(long)]
    pub(crate) nested_changed: bool,
    #[arg(long)]
    pub(crate) result: String,
}

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

#[derive(Debug, Subcommand)]
pub(crate) enum ReviewCommand {
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
    #[arg(long, default_value_t = true)]
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
    #[arg(long)]
    pub(crate) authority: i64,
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
    Add(ReviewRunAddArgs),
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
}

#[derive(Debug, Args)]
pub(crate) struct ReviewRunListArgs {
    #[arg(long)]
    pub(crate) plan: Option<i64>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum FindingCommand {
    Add(FindingAddArgs),
    Classify(FindingClassifyArgs),
    List(FindingListArgs),
    Verify(FindingVerifyArgs),
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
    #[arg(long)]
    pub(crate) classification: String,
}

#[derive(Debug, Args)]
pub(crate) struct FindingListArgs {
    #[arg(long)]
    pub(crate) status: Option<String>,
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
    pub(crate) result: String,
    #[arg(long)]
    pub(crate) notes: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ClosureCommand {
    Add(ClosureAddArgs),
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
pub(crate) enum AuthorityCommand {
    Add(AuthorityAddArgs),
    Event {
        #[command(subcommand)]
        command: AuthorityEventCommand,
    },
    List(AuthorityListArgs),
}

#[derive(Debug, Args)]
pub(crate) struct AuthorityAddArgs {
    #[arg(long)]
    pub(crate) path: String,
    #[arg(long = "type")]
    pub(crate) authority_type: String,
    #[arg(long)]
    pub(crate) scope: Option<String>,
    #[arg(long)]
    pub(crate) summary: Option<String>,
    #[arg(long, default_value_t = 90)]
    pub(crate) precedence: i64,
}

#[derive(Debug, Subcommand)]
pub(crate) enum AuthorityEventCommand {
    Add(AuthorityEventAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct AuthorityEventAddArgs {
    #[arg(long = "type")]
    pub(crate) event_type: String,
    #[arg(long)]
    pub(crate) summary: String,
    #[arg(long)]
    pub(crate) scope: Option<String>,
    #[arg(long)]
    pub(crate) source: Option<String>,
    #[arg(long, default_value_t = 100)]
    pub(crate) precedence: i64,
}

#[derive(Debug, Args)]
pub(crate) struct AuthorityListArgs {
    #[arg(long)]
    pub(crate) scope: Option<String>,
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
    pub(crate) item: i64,
    #[arg(long = "to")]
    pub(crate) target_type: String,
    #[arg(long)]
    pub(crate) title: Option<String>,
    #[arg(long)]
    pub(crate) details: Option<String>,
    #[arg(long, default_value = "medium")]
    pub(crate) priority: String,
    #[arg(long)]
    pub(crate) work_unit: Option<i64>,
    #[arg(long)]
    pub(crate) name: Option<String>,
    #[arg(long)]
    pub(crate) command: Option<String>,
    #[arg(long, default_value = "other")]
    pub(crate) command_type: String,
    #[arg(long)]
    pub(crate) scope: Option<String>,
    #[arg(long, default_value = "candidate")]
    pub(crate) command_status: String,
    #[arg(long, default_value = "context_dependent")]
    pub(crate) stability: String,
    #[arg(long)]
    pub(crate) timeout: Option<String>,
    #[arg(long)]
    pub(crate) expected_result: Option<String>,
    #[arg(long)]
    pub(crate) authority: Option<i64>,
    #[arg(long = "review-type")]
    pub(crate) review_type: Option<String>,
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

#[derive(Debug, Subcommand)]
pub(crate) enum DecomposeCommand {
    Design(DecomposeDesignArgs),
}

#[derive(Debug, Args)]
pub(crate) struct DecomposeDesignArgs {
    pub(crate) design_version_id: i64,
    #[arg(long)]
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
    #[arg(long)]
    pub(crate) status: Option<String>,
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
    #[arg(long)]
    pub(crate) checklist: Option<i64>,
    #[arg(long)]
    pub(crate) status: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct ChecklistItemCloseArgs {
    pub(crate) checklist_item_id: i64,
}

#[derive(Debug, Subcommand)]
pub(crate) enum StaleCommand {
    /// List stale design-derived records.
    List,
    /// Accept a stale record without changing its underlying status.
    Accept(StaleRecordDispositionArgs),
    /// Close a stale record that has a closed lifecycle state.
    Close(StaleRecordDispositionArgs),
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
    pub(crate) design_version: Option<i64>,
    #[arg(long)]
    pub(crate) work_unit: Option<i64>,
    #[arg(long)]
    pub(crate) phase: Option<i64>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ExportCommand {
    Design(ExportDesignArgs),
    Plan(ExportPlanArgs),
}

#[derive(Debug, Args)]
pub(crate) struct ExportDesignArgs {
    #[arg(long)]
    pub(crate) design: i64,
    #[arg(long)]
    pub(crate) output: PathBuf,
}

#[derive(Debug, Args)]
pub(crate) struct ExportPlanArgs {
    #[arg(long)]
    pub(crate) design: i64,
    #[arg(long)]
    pub(crate) output: PathBuf,
}
