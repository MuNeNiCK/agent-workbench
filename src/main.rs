use std::env;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};

use agent_workbench::{
    CommandUsageListQuery, KptItemTaskConversion, NewAuthorityEvent, NewCommandDeviation,
    NewCommandProfile, NewCommandUsage, NewDecision, NewKptItem, NewKptReview, NewTask,
    NewUserCorrection, NewWorkFork, NewWorkRecord, NewWorkRecordCommand, NewWorkRecordCommit,
    NewWorkRecordFile, NextAction, RuleQuery, TaskListQuery, WorkForkSource,
    accept_task_out_of_scope, add_authority_event, add_command_deviation, add_command_usage,
    add_decision, add_fixed_command, add_kpt_item, add_task, add_user_correction,
    add_work_record_command, add_work_record_commit, add_work_record_file, applicable_rules,
    close_active_work, close_kpt_review, close_task, convert_kpt_item_to_task,
    create_follow_up_work, create_work_record, export_work_record_markdown, fork_work,
    init_project, interrupt_work, list_authority_events, list_command_profiles,
    list_command_usages, list_decisions, list_kpt_items, list_kpt_reviews, list_tasks,
    list_user_corrections, list_work_records, next_action, project_status, reopen_work,
    resume_check_basic, resume_ready_basic, resume_work, start_kpt_review, start_work,
    suspend_work,
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
    /// Check whether a suspended activation can resume without writing ledger rows.
    ResumeReady(GateResumeReadyArgs),
}

#[derive(Debug, Args)]
struct GateResumeReadyArgs {
    #[arg(long, default_value = "basic")]
    maturity: String,
    #[arg(long)]
    dry_run: bool,
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
    #[arg(long)]
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
    command: String,
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
    path: String,
    #[arg(long, default_value = "changed")]
    role: String,
    #[arg(long)]
    note: Option<String>,
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
    Convert(KptItemConvertArgs),
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
                ]
                .into_iter()
                .filter(|selected| *selected)
                .count();
                if source_count != 1 {
                    anyhow::bail!(
                        "exactly one of --from-record, --from-activation, or --from-commit is required"
                    );
                }

                let source = match (
                    args.from_record,
                    args.from_activation,
                    args.from_commit.as_deref(),
                ) {
                    (Some(id), None, None) => WorkForkSource::Record(id),
                    (None, Some(id), None) => WorkForkSource::Activation(id),
                    (None, None, Some(sha)) => WorkForkSource::Commit(sha),
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
            if args.maturity != "basic" {
                anyhow::bail!("only --maturity basic is implemented");
            }
            let outcome = resume_check_basic(&root)?;
            println!("resume_check_id: {}", outcome.resume_check_id);
            println!("result: {}", outcome.result);
            if let Some(reason) = outcome.blocking_reason {
                println!("blocking_reason: {reason}");
            }
        }
        Command::Gate { command } => match command {
            GateCommand::ResumeReady(args) => {
                if args.maturity != "basic" {
                    anyhow::bail!("only --maturity basic is implemented");
                }
                let outcome = resume_ready_basic(&root)?;
                println!("gate: resume-ready");
                println!("maturity: {}", args.maturity);
                println!("dry_run: true");
                println!("work_unit_id: {}", outcome.work_unit_id);
                println!("activation_id: {}", outcome.activation_id);
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
                    let outcome = add_command_usage(
                        &root,
                        NewCommandUsage {
                            profile: args.profile.as_deref(),
                            command: args.command.as_deref(),
                            result: &args.result,
                            log_path: args.log.as_deref(),
                            work_unit_id: args.work_unit,
                        },
                    )?;
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
                            command_profile_id: args.profile,
                            command: &args.command,
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
                    let outcome = add_work_record_commit(
                        &root,
                        NewWorkRecordCommit {
                            work_record_id: args.work_record_id,
                            commit_sha: &args.sha,
                            role: &args.role,
                            note: args.note.as_deref(),
                        },
                    )?;
                    println!("linked work record commit");
                    println!("work_record_commit_id: {}", outcome.link_id);
                }
            },
            WorkRecordCommand::File { command } => match command {
                WorkRecordFileCommand::Add(args) => {
                    let outcome = add_work_record_file(
                        &root,
                        NewWorkRecordFile {
                            work_record_id: args.work_record_id,
                            path: &args.path,
                            role: &args.role,
                            note: args.note.as_deref(),
                        },
                    )?;
                    println!("linked work record file");
                    println!("work_record_file_id: {}", outcome.link_id);
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
                KptItemCommand::Convert(args) => {
                    if args.target_type != "task" {
                        anyhow::bail!("only --to task is implemented");
                    }
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
