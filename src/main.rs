use std::env;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};

use agent_workbench::{
    NewCommandProfile, NewUserCorrection, NextAction, RuleQuery, add_fixed_command,
    add_user_correction, applicable_rules, close_active_work, init_project, interrupt_work,
    list_command_profiles, list_user_corrections, next_action, project_status, resume_check_basic,
    resume_work, start_work, suspend_work,
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
struct ResumeCheckArgs {
    #[arg(long, default_value = "basic")]
    maturity: String,
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
                let scope = args.scope.as_deref().filter(|scope| *scope != "current");
                let records = applicable_rules(
                    &root,
                    RuleQuery {
                        scope_key: scope,
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
    }

    Ok(())
}
