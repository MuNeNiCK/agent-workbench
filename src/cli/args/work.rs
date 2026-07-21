use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub(crate) enum DoctorCommand {
    /// Diagnose or repair legacy validation-run links.
    ValidationLinks(DoctorValidationLinksArgs),
}

#[derive(Debug, Args)]
pub(crate) struct DoctorValidationLinksArgs {
    /// Diagnose one exact validation artifact.
    #[arg(long)]
    pub(crate) artifact: Option<String>,
    /// Explicitly run the default read-only diagnosis.
    #[arg(long, conflicts_with_all = ["repair", "audit"])]
    pub(crate) dry_run: bool,
    /// Preserve a recovery point and repair every deterministically repairable link.
    #[arg(long, conflicts_with_all = ["audit", "dry_run"])]
    pub(crate) repair: bool,
    /// List immutable validation-link repair audit records.
    #[arg(long, conflicts_with_all = ["repair", "dry_run"])]
    pub(crate) audit: bool,
    #[command(subcommand)]
    pub(crate) command: Option<DoctorValidationLinkCommand>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum DoctorValidationLinkCommand {
    Repair(DoctorValidationLinkRepairArgs),
    Retire(DoctorValidationLinkRetireArgs),
}

#[derive(Debug, Args)]
pub(crate) struct DoctorValidationLinkRepairArgs {
    pub(crate) artifact_ref: String,
    #[arg(long)]
    pub(crate) project: i64,
    #[arg(long)]
    pub(crate) expected_current: String,
}

#[derive(Debug, Args)]
pub(crate) struct DoctorValidationLinkRetireArgs {
    pub(crate) artifact_ref: String,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) expected_current: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum WorkCommand {
    /// Start a new active work unit.
    Start(WorkStartArgs),
    /// Activate an existing open work unit.
    Activate(WorkActivateArgs),
    /// Enter the audited remediation activation for a valid registered finding.
    Remediate(WorkRemediateArgs),
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
pub(crate) struct WorkRemediateArgs {
    #[arg(long)]
    pub(crate) finding: i64,
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
    /// Close this exact work owner. Omit only when exactly one open owner exists.
    pub(crate) work_unit_id: Option<i64>,
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
    /// Check this exact suspended work owner.
    pub(crate) work_unit_id: Option<i64>,
    #[arg(long, default_value = "basic")]
    pub(crate) maturity: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum GateCommand {
    /// Check whether the active work unit can close without recording results.
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
    /// Check whether a suspended activation can resume without recording results.
    ResumeReady(GateResumeReadyArgs),
    /// Check whether an imported design version is ready for implementation planning.
    DesignReady(GateDesignReadyArgs),
    /// Check whether approved design work is decomposed and current.
    ImplementationReady(GateImplementationReadyArgs),
}

#[derive(Debug, Args)]
pub(crate) struct GateResumeReadyArgs {
    /// Evaluate this exact suspended work owner.
    pub(crate) work_unit_id: Option<i64>,
    #[arg(long, default_value = "basic")]
    pub(crate) maturity: String,
    #[arg(long)]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Args)]
pub(crate) struct GateCloseReadyArgs {
    /// Evaluate this exact work owner instead of the active-work adapter.
    pub(crate) work_unit_id: Option<i64>,
    #[arg(long)]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Args)]
pub(crate) struct StatusArgs {
    /// Restrict the projection to one exact work owner.
    #[arg(long)]
    pub(crate) work: Option<i64>,
}

#[derive(Debug, Args)]
pub(crate) struct NextArgs {
    /// Resolve the next action for one exact work owner.
    #[arg(long)]
    pub(crate) work: Option<i64>,
}

#[derive(Debug, Args)]
pub(crate) struct GateDesignReadyArgs {
    #[arg(value_name = "DESIGN_VERSION", conflicts_with = "design_version")]
    pub(crate) design_version_positional: Option<i64>,
    #[arg(long)]
    pub(crate) design_version: Option<i64>,
    #[arg(long)]
    pub(crate) dry_run: bool,
}

#[derive(Debug, Args)]
pub(crate) struct GateImplementationReadyArgs {
    /// Evaluate the current design bound to this exact work owner.
    #[arg(value_name = "WORK", conflicts_with = "design_version")]
    pub(crate) work_unit_id: Option<i64>,
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
