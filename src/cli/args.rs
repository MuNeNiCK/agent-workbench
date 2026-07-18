use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod evidence;
mod planning;
mod review;
mod work;

pub(crate) use evidence::*;
pub(crate) use planning::*;
pub(crate) use review::*;
pub(crate) use work::*;

#[derive(Debug, Parser)]
#[command(name = "agent-workbench")]
#[command(about = "Structured local workbench for long-running coding-agent work")]
#[command(version)]
pub(crate) struct Cli {
    #[arg(long, global = true, value_name = "PATH")]
    pub(crate) root: Option<PathBuf>,

    #[command(subcommand)]
    pub(crate) command: Command,
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Initialize managed project state.
    Init,
    /// Explicitly inspect, reset, or restore the project ledger.
    Update(UpdateArgs),
    /// Print public project status.
    Status,
    /// Print the next suggested action.
    Next,
    /// Diagnose project-state integrity without changing it.
    Doctor {
        #[command(subcommand)]
        command: DoctorCommand,
    },
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
    /// Manage project tasks.
    Task {
        #[command(subcommand)]
        command: TaskCommand,
    },
    /// Manage work phases inside aggregate work units.
    Phase {
        #[command(subcommand)]
        command: PhaseCommand,
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

#[derive(Debug, clap::Args)]
pub(crate) struct UpdateArgs {
    /// Print the supported reset plan without changing any file.
    #[arg(long, conflicts_with = "reset")]
    pub(crate) dry_run: bool,
    /// Replace a supported schema-13 ledger with a fresh schema-14 ledger.
    #[arg(long, conflicts_with = "dry_run")]
    pub(crate) reset: bool,
    /// Required reason stored in the reset audit.
    #[arg(long, requires = "reset")]
    pub(crate) reason: Option<String>,
    #[command(subcommand)]
    pub(crate) command: Option<UpdateCommand>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum UpdateCommand {
    /// Restore a verified content-addressed backup byte-for-byte.
    Restore(UpdateRestoreArgs),
}

#[derive(Debug, clap::Args)]
pub(crate) struct UpdateRestoreArgs {
    #[arg(long)]
    pub(crate) backup: String,
    #[arg(long)]
    pub(crate) expected_current: String,
}
