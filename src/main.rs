use std::env;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};

use agent_workbench::{NextAction, init_project, next_action, project_status};

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
    }

    Ok(())
}
