use anyhow::Result;
use clap::Parser;
use std::env;

mod args;
mod design_flow;
mod gate;
mod memory;
mod planning;
mod records;
mod review_ops;
mod work;

use args::*;

use agent_workbench::{NextAction, init_project, next_action, project_status};
pub(crate) fn run() -> Result<()> {
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
        Command::Work { command } => work::handle(&root, command)?,
        Command::ResumeCheck(args) => work::handle_resume_check(&root, args)?,
        Command::Gate { command } => gate::handle(&root, command)?,
        Command::Correction { command } => memory::handle_correction(&root, command)?,
        Command::Command { command } => memory::handle_command(&root, command)?,
        Command::Git { command } => records::handle_git(&root, command)?,
        Command::Rules { command } => memory::handle_rules(&root, command)?,
        Command::WorkRecord { command } => records::handle_work_record(&root, command)?,
        Command::Repository { command } => records::handle_repository(&root, command)?,
        Command::Task { command } => planning::handle_task(&root, command)?,
        Command::Decision { command } => planning::handle_decision(&root, command)?,
        Command::Design { command } => planning::handle_design(&root, command)?,
        Command::Requirement { command } => planning::handle_requirement(&root, command)?,
        Command::DesignDecision { command } => planning::handle_design_decision(&root, command)?,
        Command::GateTemplate { command } => planning::handle_gate_template(&root, command)?,
        Command::Trace { command } => planning::handle_trace(&root, command)?,
        Command::Evidence { command } => planning::handle_evidence(&root, command)?,
        Command::Coverage { command } => planning::handle_coverage(&root, command)?,
        Command::Review { command } => review_ops::handle_review(&root, command)?,
        Command::Finding { command } => review_ops::handle_finding(&root, command)?,
        Command::Closure { command } => review_ops::handle_closure(&root, command)?,
        Command::Acceptance { command } => review_ops::handle_acceptance(&root, command)?,
        Command::Authority { command } => review_ops::handle_authority(&root, command)?,
        Command::Kpt { command } => review_ops::handle_kpt(&root, command)?,
        Command::Decompose { command } => design_flow::handle_decompose(&root, command)?,
        Command::Checklist { command } => design_flow::handle_checklist(&root, command)?,
        Command::Stale { command } => design_flow::handle_stale(&root, command)?,
        Command::ReviewContext(args) => design_flow::print_review_context(&root, &args)?,
        Command::Export { command } => design_flow::handle_export(&root, command)?,
    }
    Ok(())
}
