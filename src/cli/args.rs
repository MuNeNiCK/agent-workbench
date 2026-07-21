use std::path::PathBuf;

use clap::{Parser, Subcommand};

mod authority;
mod evidence;
mod migration;
mod planning;
mod review;
mod work;

pub(crate) use authority::*;
pub(crate) use evidence::*;
pub(crate) use migration::*;
pub(crate) use planning::*;
pub(crate) use review::*;
pub(crate) use work::*;

#[derive(Debug, Parser)]
#[command(name = "agent-workbench")]
#[command(about = "Structured local workbench for long-running coding-agent work")]
#[command(version, disable_help_subcommand = true)]
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
    Init {
        #[arg(long)]
        name: Option<String>,
    },
    /// Inspect or restore versioned project state.
    Update {
        #[command(subcommand)]
        command: UpdateCommand,
    },
    /// Run explicitly operator-scoped workflows.
    Operator {
        #[command(subcommand)]
        command: OperatorCommand,
    },
    /// Print public project status.
    Status(StatusArgs),
    /// Print the next suggested action.
    Next(NextArgs),
    /// Diagnose and repair supported compatibility problems.
    Doctor {
        #[command(subcommand)]
        command: DoctorCommand,
    },
    /// Migrate supported historical project state.
    Migration {
        #[command(subcommand)]
        command: MigrationCommand,
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
    /// Adjudicate immutable verification claims.
    Verification {
        #[command(subcommand)]
        command: VerificationCommand,
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
    /// Inspect, preview, and apply Decomposition Plans.
    Decomposition {
        #[command(subcommand)]
        command: DecompositionCommand,
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
    /// Print help for a variadic route or one --route value.
    Help(HelpArgs),
}

#[derive(Debug, clap::Args)]
pub(crate) struct HelpArgs {
    #[arg(value_name = "ROUTE_PART", conflicts_with = "route")]
    pub(crate) route_parts: Vec<String>,
    #[arg(long, conflicts_with = "route_parts")]
    pub(crate) route: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum UpdateCommand {
    /// Print the current identity and available content-addressed backups.
    Inspect,
    /// Record project-local authority for one recovery choice.
    AuthorityRecord(UpdateAuthorityRecordArgs),
    /// Record one owner choice offered by an update inspection.
    Decide(UpdateDecideArgs),
    /// Apply the pending project update after inspecting current identity.
    Apply(UpdateApplyArgs),
    /// Atomically restore a verified content-addressed backup.
    Restore(UpdateRestoreArgs),
}

#[derive(Debug, clap::Args)]
pub(crate) struct UpdateAuthorityRecordArgs {
    pub(crate) inspection_handle: String,
    #[arg(long)]
    pub(crate) choice: String,
    #[arg(long)]
    pub(crate) statement: String,
    #[arg(long, value_parser = ["user_instruction"])]
    pub(crate) provenance: String,
    #[arg(long)]
    pub(crate) provenance_ref: String,
    #[arg(long)]
    pub(crate) expected_current: String,
    #[arg(long)]
    pub(crate) idempotency_key: String,
}

#[derive(Debug, clap::Args)]
#[command(group(clap::ArgGroup::new("authority_kind").required(true).multiple(false).args(["authority", "recovery_authority"])))]
pub(crate) struct UpdateDecideArgs {
    pub(crate) inspection_handle: String,
    #[arg(long)]
    pub(crate) choice: String,
    #[arg(long)]
    pub(crate) authority: Option<i64>,
    #[arg(long)]
    pub(crate) recovery_authority: Option<String>,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) expected_current: String,
}

#[derive(Debug, clap::Args)]
pub(crate) struct UpdateApplyArgs {
    pub(crate) inspection_handle: Option<String>,
    #[arg(long)]
    pub(crate) expected_current: String,
    #[arg(long)]
    pub(crate) idempotency_key: Option<String>,
}

#[derive(Debug, clap::Args)]
pub(crate) struct UpdateRestoreArgs {
    #[arg(long)]
    pub(crate) backup: String,
    #[arg(long)]
    pub(crate) expected_current: String,
    #[arg(long)]
    pub(crate) idempotency_key: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum OperatorCommand {
    /// Assemble, publish, verify, and recover one release candidate owner.
    Release {
        #[command(subcommand)]
        command: OperatorReleaseCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum OperatorReleaseCommand {
    /// Assemble or inspect candidate-local release bytes.
    Candidate {
        #[command(subcommand)]
        command: OperatorReleaseCandidateCommand,
    },
    /// Create and push the exact annotated tag without overwriting remote history.
    PublishSource(OperatorReleaseMutationArgs),
    /// Create the remote release and publish only the inspected asset manifest.
    PublishAssets(OperatorReleaseMutationArgs),
    /// Download the complete remote asset set and verify its identities.
    VerifyRemote(OperatorReleaseMutationArgs),
    /// Probe an interrupted or conflicting external publication step.
    Reconcile(OperatorReleaseMutationArgs),
    /// Retry only the absent step selected by the candidate resolver.
    Retry(OperatorReleaseMutationArgs),
    /// Publish a non-destructive withdrawal notice and retain remote history.
    Withdraw(OperatorReleaseAuthorityArgs),
    /// Link an incomplete candidate to an explicitly authorized successor.
    Supersede(OperatorReleaseSupersedeArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum OperatorReleaseCandidateCommand {
    /// Build and bind the complete candidate from the reviewed source commit.
    Assemble(OperatorReleaseAssembleArgs),
    /// Reinspect the assembled files and record local identity equality.
    Inspect(OperatorReleaseMutationArgs),
}

#[derive(Debug, clap::Args)]
pub(crate) struct OperatorReleaseAssembleArgs {
    #[arg(long = "work")]
    pub(crate) work_unit_id: Option<i64>,
    #[arg(long)]
    pub(crate) version: String,
    #[arg(long = "commit")]
    pub(crate) commit: String,
    #[arg(long)]
    pub(crate) expected_current: String,
    #[arg(long)]
    pub(crate) idempotency_key: String,
}

#[derive(Debug, clap::Args)]
pub(crate) struct OperatorReleaseMutationArgs {
    pub(crate) candidate: String,
    #[arg(long)]
    pub(crate) expected_current: String,
    #[arg(long)]
    pub(crate) idempotency_key: String,
}

#[derive(Debug, clap::Args)]
pub(crate) struct OperatorReleaseAuthorityArgs {
    pub(crate) candidate: String,
    #[arg(long)]
    pub(crate) expected_current: String,
    #[arg(long)]
    pub(crate) idempotency_key: String,
    #[arg(long = "authority")]
    pub(crate) authority_event_id: i64,
    #[arg(long)]
    pub(crate) reason: String,
}

#[derive(Debug, clap::Args)]
pub(crate) struct OperatorReleaseSupersedeArgs {
    pub(crate) candidate: String,
    #[arg(long)]
    pub(crate) expected_current: String,
    #[arg(long)]
    pub(crate) idempotency_key: String,
    #[arg(long)]
    pub(crate) by: String,
    #[arg(long = "authority")]
    pub(crate) authority_event_id: i64,
    #[arg(long)]
    pub(crate) reason: String,
}
