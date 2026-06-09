use std::path::Path;

use anyhow::Result;

use super::args::{GateCommand, GateRunCommand};
use agent_workbench::{
    DesignReadyCheck, ImplementationReadyCheck, NewValidationRun, ValidationGateSelection,
    ValidationRunListQuery, add_validation_run, close_ready, design_ready, implementation_ready,
    list_validation_runs, resume_ready, select_validation_gate,
};

pub(crate) fn handle(root: &Path, command: GateCommand) -> Result<()> {
    match command {
        GateCommand::CloseReady(args) => {
            let _ = args.dry_run;
            let outcome = close_ready(root)?;
            println!("gate: close-ready");
            println!("dry_run: true");
            if let Some(work_unit_id) = outcome.work_unit_id {
                println!("work_unit_id: {work_unit_id}");
            }
            if let Some(activation_id) = outcome.activation_id {
                println!("activation_id: {activation_id}");
            }
            println!("result: {}", outcome.result);
            if let Some(reason) = outcome.blocking_reason {
                println!("blocking_reason: {reason}");
            }
            for item in outcome.items {
                match item.blocking_action {
                    Some(action) => println!(
                        "{}: {} ({}) [{}]",
                        item.name, item.result, action, item.details
                    ),
                    None => println!("{}: {} [{}]", item.name, item.result, item.details),
                }
            }
        }
        GateCommand::Select(args) => {
            let outcome = select_validation_gate(
                root,
                ValidationGateSelection {
                    design_version_id: args.design,
                    gate_key: &args.template,
                    requirement_key: &args.requirement,
                    task_id: args.task,
                    command: args.command.as_deref(),
                },
            )?;
            println!("selected validation gate");
            println!("validation_gate_id: {}", outcome.validation_gate_id);
            println!(
                "validation_gate_template_id: {}",
                outcome.validation_gate_template_id
            );
            println!("design_requirement_id: {}", outcome.design_requirement_id);
            println!("task_id: {}", outcome.task_id);
        }
        GateCommand::Record(args) => {
            let outcome = add_validation_run(
                root,
                NewValidationRun {
                    validation_gate_id: args.gate,
                    command_usage_id: args.usage,
                    repository_snapshot_id: args.snapshot,
                    result: &args.result,
                    command: args.command.as_deref(),
                    classification: None,
                    acceptance_record_id: args.acceptance,
                    artifact_path: args.artifact.as_deref(),
                    artifact_hash: args.artifact_hash.as_deref(),
                    notes: args.notes.as_deref(),
                },
            )?;
            println!("recorded validation run");
            println!("validation_run_id: {}", outcome.validation_run_id);
            println!("validation_gate_id: {}", outcome.validation_gate_id);
            if let Some(work_unit_id) = outcome.work_unit_id {
                println!("work_unit_id: {work_unit_id}");
            }
            if let Some(task_id) = outcome.task_id {
                println!("task_id: {task_id}");
            }
        }
        GateCommand::Run { command } => match command {
            GateRunCommand::List(args) => {
                let records = list_validation_runs(
                    root,
                    ValidationRunListQuery {
                        validation_gate_id: args.gate,
                    },
                )?;
                if records.is_empty() {
                    println!("no validation runs");
                }
                for record in records {
                    let usage = record
                        .command_usage_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    let snapshot = record
                        .repository_snapshot_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    let artifact = record.artifact_path.as_deref().unwrap_or("-");
                    println!(
                        "{} [gate={} {}:{}] usage={} snapshot={} artifact={}",
                        record.id,
                        record.validation_gate_id,
                        record.gate_key,
                        record.result,
                        usage,
                        snapshot,
                        artifact
                    );
                }
            }
        },
        GateCommand::ResumeReady(args) => {
            let outcome = resume_ready(root, &args.maturity)?;
            println!("gate: resume-ready");
            println!("maturity: {}", args.maturity);
            println!("dry_run: true");
            if let Some(work_unit_id) = outcome.work_unit_id {
                println!("work_unit_id: {work_unit_id}");
            }
            if let Some(activation_id) = outcome.activation_id {
                println!("activation_id: {activation_id}");
            }
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
        GateCommand::DesignReady(args) => {
            let outcome = design_ready(
                root,
                DesignReadyCheck {
                    design_version_id: args.design_version,
                },
            )?;
            println!("gate: design-ready");
            println!("dry_run: true");
            if let Some(design_package_id) = outcome.design_package_id {
                println!("design_package_id: {design_package_id}");
            }
            if let Some(design_version_id) = outcome.design_version_id {
                println!("design_version_id: {design_version_id}");
            }
            println!("result: {}", outcome.result);
            if let Some(reason) = outcome.blocking_reason {
                println!("blocking_reason: {reason}");
            }
            for item in outcome.items {
                match item.detail {
                    Some(detail) => println!("{}: {} ({})", item.name, item.result, detail),
                    None => println!("{}: {}", item.name, item.result),
                }
            }
        }
        GateCommand::ImplementationReady(args) => {
            let outcome = implementation_ready(
                root,
                ImplementationReadyCheck {
                    design_version_id: args.design_version,
                },
            )?;
            println!("gate: implementation-ready");
            println!("dry_run: true");
            if let Some(design_package_id) = outcome.design_package_id {
                println!("design_package_id: {design_package_id}");
            }
            if let Some(design_version_id) = outcome.design_version_id {
                println!("design_version_id: {design_version_id}");
            }
            println!("result: {}", outcome.result);
            if let Some(reason) = outcome.blocking_reason {
                println!("blocking_reason: {reason}");
            }
            for item in outcome.items {
                match item.detail {
                    Some(detail) => println!("{}: {} ({})", item.name, item.result, detail),
                    None => println!("{}: {}", item.name, item.result),
                }
            }
        }
    }
    Ok(())
}
