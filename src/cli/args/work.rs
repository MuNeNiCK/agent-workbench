use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub(crate) enum DoctorCommand {
    /// Check schema, foreign keys, and storage integrity without mutation.
    Integrity,
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
    /// Resume a suspended activation using an allowed resume check.
    Resume(WorkResumeArgs),
    /// Close the active work unit.
    Close(WorkCloseArgs),
    /// Abandon an open, blocked, or closed work unit.
    Abandon(WorkAbandonArgs),
    /// Reopen a closed or abandoned work unit.
    Reopen(WorkReopenArgs),
    /// Create follow-up work linked to a closed or abandoned work unit.
    FollowUp(WorkFollowUpArgs),
    /// Manage explicit work dependencies.
    Dependency {
        #[command(subcommand)]
        command: WorkDependencyCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum WorkDependencyCommand {
    Add(WorkDependencyAddArgs),
    List(WorkDependencyListArgs),
    Satisfy(WorkDependencySatisfyArgs),
    Accept(WorkDependencyAcceptArgs),
}

#[derive(Debug, Args)]
pub(crate) struct WorkDependencyAddArgs {
    #[arg(long = "from")]
    pub(crate) from_work: i64,
    #[arg(long = "to")]
    pub(crate) to_work: i64,
    #[arg(long = "type")]
    pub(crate) dependency_type: String,
    #[arg(long)]
    pub(crate) reason: String,
}

#[derive(Debug, Args)]
pub(crate) struct WorkDependencyListArgs {
    #[arg(long)]
    pub(crate) work_unit: i64,
}

#[derive(Debug, Args)]
pub(crate) struct WorkDependencySatisfyArgs {
    pub(crate) dependency_id: i64,
    #[arg(long)]
    pub(crate) reason: String,
}

#[derive(Debug, Args)]
pub(crate) struct WorkDependencyAcceptArgs {
    pub(crate) dependency_id: i64,
    #[arg(long)]
    pub(crate) expected_current: String,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) risk: Option<String>,
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
pub(crate) struct WorkReopenArgs {
    pub(crate) work_unit_id: i64,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long, default_value = "closure_invalid")]
    pub(crate) reason_type: String,
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

#[allow(clippy::enum_variant_names)]
#[derive(Debug, Subcommand)]
pub(crate) enum GateCommand {
    /// Check whether the active work unit can close without recording results.
    CloseReady(GateCloseReadyArgs),
    /// Check whether one phase can close without recording results.
    PhaseCloseReady(GatePhaseCloseReadyArgs),
    /// Check whether a suspended activation can resume without recording results.
    ResumeReady(GateResumeReadyArgs),
    /// Check whether an imported design version is ready for implementation planning.
    DesignReady(GateDesignReadyArgs),
    /// Check whether approved design work is decomposed and current.
    ImplementationReady(GateImplementationReadyArgs),
}

#[derive(Debug, Args)]
pub(crate) struct GatePhaseCloseReadyArgs {
    pub(crate) phase_id: i64,
    #[arg(long)]
    pub(crate) dry_run: bool,
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
