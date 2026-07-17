use clap::{ArgGroup, Args, Subcommand};

#[derive(Debug, Subcommand)]
pub(crate) enum MigrationCommand {
    /// Bind a signed legacy reviewer identity for schema-11 migration.
    Reviewer {
        #[command(subcommand)]
        command: MigrationReviewerCommand,
    },
    /// Preserve task history across revisions and phases.
    #[command(name = "task-history")]
    TaskIdentity {
        #[command(subcommand)]
        command: TaskIdentityCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum MigrationReviewerCommand {
    Bind(MigrationReviewerBindArgs),
}

#[derive(Debug, Args)]
pub(crate) struct MigrationReviewerBindArgs {
    #[arg(long, value_parser = ["signed-envelope-v1"])]
    pub(crate) provider: String,
    #[arg(long)]
    pub(crate) assertion: String,
    #[arg(long)]
    pub(crate) idempotency_key: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum TaskIdentityCommand {
    /// Build a read-only owner index or one owner-scoped migration plan.
    Plan {
        #[arg(long)]
        owner: Option<String>,
    },
    /// List unresolved ambiguities for one exact base plan.
    AmbiguityList(PlanSelection),
    /// Record migration-only user authority for one exact ambiguity action.
    AuthorityRecord(AuthorityRecordArgs),
    /// Bind one authorized ambiguity decision into recovery state.
    AmbiguityDecide(AmbiguityDecisionArgs),
    /// Atomically apply one ambiguity-free resolved owner plan.
    Apply(PlanSelection),
    /// List pending recovery and committed migration audit state.
    Audit {
        #[arg(long)]
        owner: Option<String>,
    },
}

#[derive(Debug, Args)]
pub(crate) struct PlanSelection {
    #[arg(long)]
    pub(crate) owner: String,
    #[arg(long)]
    pub(crate) plan: String,
}

#[derive(Debug, Args)]
#[command(group(ArgGroup::new("action").required(true).multiple(false).args(["resolution", "retire"])))]
pub(crate) struct AuthorityRecordArgs {
    #[arg(long)]
    pub(crate) owner: String,
    #[arg(long)]
    pub(crate) plan: String,
    #[arg(long)]
    pub(crate) ambiguity: String,
    #[arg(long, group = "action")]
    pub(crate) resolution: Option<String>,
    #[arg(long, group = "action")]
    pub(crate) retire: bool,
    #[arg(long)]
    pub(crate) statement: String,
    #[arg(long, value_parser = ["user_instruction"])]
    pub(crate) provenance: String,
    #[arg(long)]
    pub(crate) provenance_ref: String,
}

#[derive(Debug, Args)]
#[command(group(ArgGroup::new("action").required(true).multiple(false).args(["resolution", "retire"])))]
pub(crate) struct AmbiguityDecisionArgs {
    #[arg(long)]
    pub(crate) owner: String,
    #[arg(long)]
    pub(crate) plan: String,
    #[arg(long)]
    pub(crate) ambiguity: String,
    #[arg(long, group = "action")]
    pub(crate) resolution: Option<String>,
    #[arg(long, group = "action")]
    pub(crate) retire: bool,
    #[arg(long)]
    pub(crate) authority: String,
}
