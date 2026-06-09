use std::fs;
use std::path::Path;

use anyhow::Result;

use super::args::{
    ChecklistCommand, DecomposeCommand, ExportCommand, ReviewContextArgs, StaleCommand,
};
use agent_workbench::{
    DesignDecomposition, DesignRequirementListQuery, ReviewContextQuery, TaskDerivationListQuery,
    decompose_design, list_checklists, list_design_requirements, list_stale_records,
    list_task_derivations, render_review_context,
};

pub(crate) fn handle_decompose(root: &Path, command: DecomposeCommand) -> Result<()> {
    match command {
        DecomposeCommand::Design(args) => {
            let outcome = decompose_design(
                root,
                DesignDecomposition {
                    design_version_id: args.design_version_id,
                    work_unit_id: args.work_unit,
                    checklist_title: args.checklist_title.as_deref(),
                    reason: args.reason.as_deref(),
                },
            )?;
            println!("decomposed design");
            println!("design_version_id: {}", outcome.design_version_id);
            println!("work_unit_id: {}", outcome.work_unit_id);
            println!("checklist_id: {}", outcome.checklist_id);
            println!("created_tasks: {}", outcome.created_tasks);
            println!("created_derivations: {}", outcome.created_derivations);
        }
    }
    Ok(())
}

pub(crate) fn handle_checklist(root: &Path, command: ChecklistCommand) -> Result<()> {
    match command {
        ChecklistCommand::List(args) => {
            let records = list_checklists(root, args.status.as_deref())?;
            if records.is_empty() {
                println!("no checklists");
            }
            for record in records {
                println!(
                    "{} [work_unit={} design_version={} {} {}/{}] {}",
                    record.id,
                    record.work_unit_id,
                    record.design_version_id,
                    record.status,
                    record.closed_count,
                    record.item_count,
                    record.title
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn handle_stale(root: &Path, command: StaleCommand) -> Result<()> {
    match command {
        StaleCommand::List => {
            let records = list_stale_records(root)?;
            if records.is_empty() {
                println!("no stale records");
            }
            for record in records {
                println!("{}:{} {}", record.record_type, record.id, record.label);
            }
        }
    }
    Ok(())
}

pub(crate) fn handle_export(root: &Path, command: ExportCommand) -> Result<()> {
    match command {
        ExportCommand::Design(args) => {
            let records = list_design_requirements(
                root,
                DesignRequirementListQuery {
                    design_version_id: args.design,
                },
            )?;
            let mut output = format!("# Design {}\n\n## Requirements\n\n", args.design);
            for record in records {
                output.push_str(&format!(
                    "- {} [{}:{}] {}\n",
                    record.requirement_key,
                    record.priority,
                    record.status,
                    record.requirement_text.lines().next().unwrap_or("")
                ));
            }
            write_export(&args.output, &output)?;
            println!("exported design");
            println!("path: {}", args.output.display());
        }
        ExportCommand::Plan(args) => {
            let records = list_task_derivations(
                root,
                TaskDerivationListQuery {
                    design_version_id: args.design,
                    work_unit_id: None,
                },
            )?;
            let mut output = format!("# Implementation Plan {}\n\n", args.design);
            for record in records {
                let checklist = record
                    .checklist_item_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "-".to_string());
                output.push_str(&format!(
                    "- requirement={} task={} checklist_item={} [{}] {}\n",
                    record.requirement_key,
                    record.task_id,
                    checklist,
                    record.status,
                    record.task_title
                ));
            }
            write_export(&args.output, &output)?;
            println!("exported plan");
            println!("path: {}", args.output.display());
        }
    }
    Ok(())
}

pub(crate) fn print_review_context(root: &std::path::Path, args: &ReviewContextArgs) -> Result<()> {
    let document = render_review_context(
        root,
        ReviewContextQuery {
            kind: &args.kind,
            design_version_id: args.design_version,
            work_unit_id: args.work_unit,
        },
    )?;
    print!("{}", document.text);
    Ok(())
}

fn write_export(path: &std::path::Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
}
