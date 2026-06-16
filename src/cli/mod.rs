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

use agent_workbench::{NextAction, PhaseBlocker, init_project, next_action, project_status};
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
                if let Some(blocker) = status.phase_blocker {
                    println!("phase_blocked: true");
                    print_phase_blocker(&blocker);
                } else {
                    println!("phase_blocked: false");
                }
            }
        }
        Command::Next => match next_action(&root)? {
            NextAction::NotInitialized { ledger_path } => {
                println!("not initialized");
                println!("ledger: {}", ledger_path.display());
                println!("next: agent-workbench init");
            }
            NextAction::BlockedPhase { blocker } => {
                println!("blocked phase");
                print_phase_blocker(&blocker);
            }
            NextAction::NoOpenWorkUnit => {
                println!("no open work unit");
                println!("next: agent-workbench work start <title>");
            }
            NextAction::ResumeSuspended { work_unit } => {
                println!("suspended work unit");
                println!("work_unit_id: {}", work_unit.id);
                println!("title: {}", work_unit.title);
                if let Some(design_version_id) = work_unit.design_version_id {
                    println!("design_version_id: {design_version_id}");
                }
                println!("next: agent-workbench resume-check --maturity trace-aware");
                println!("then: agent-workbench work resume --check <resume-check-id>");
            }
            NextAction::ActivateOpen { work_unit } => {
                println!("open inactive work unit");
                println!("work_unit_id: {}", work_unit.id);
                println!("title: {}", work_unit.title);
                match work_unit.design_version_id {
                    Some(design_version_id) => {
                        println!("design_version_id: {design_version_id}");
                        println!(
                            "next: agent-workbench work activate --implementation --design-version {} {}",
                            design_version_id, work_unit.id
                        );
                    }
                    None => println!("next: agent-workbench work activate {}", work_unit.id),
                }
            }
            NextAction::ContinueActive { work_unit } => {
                println!("continue active work unit");
                println!("work_unit_id: {}", work_unit.id);
                println!("title: {}", work_unit.title);
                if let Some(design_version_id) = work_unit.design_version_id {
                    println!("design_version_id: {design_version_id}");
                }
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

fn print_phase_blocker(blocker: &PhaseBlocker) {
    println!("blocker_kind: {}", blocker.kind);
    if let Some(work_unit_id) = blocker.work_unit_id {
        println!("work_unit_id: {work_unit_id}");
    }
    if let Some(review_plan_id) = blocker.review_plan_id {
        println!("review_plan_id: {review_plan_id}");
    }
    if let Some(review_run_id) = blocker.review_run_id {
        println!("review_run_id: {review_run_id}");
    }
    if let Some(finding_id) = blocker.finding_id {
        println!("finding_id: {finding_id}");
    }
    if let Some(review_type) = blocker.review_type.as_deref() {
        println!("review_type: {review_type}");
    }
    if let Some(stage) = blocker.stage.as_deref() {
        println!("stage: {stage}");
    }
    if let Some(severity) = blocker.severity.as_deref() {
        println!("severity: {severity}");
    }
    if let Some(classification) = blocker.classification.as_deref() {
        println!("classification: {classification}");
    }
    println!("description: {}", blocker.description);
    println!("next: {}", blocker.next_action);
}
