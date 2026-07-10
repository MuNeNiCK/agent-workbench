use std::fs;
use std::path::Path;

use anyhow::Result;

use super::args::{
    ChecklistCommand, ChecklistItemCommand, DecomposeCommand, ExportCommand, ReviewContextArgs,
    StaleCommand,
};
use agent_workbench::{
    ChecklistItemListQuery, DesignDecomposition, DesignRequirementListQuery, ReviewContextQuery,
    StaleRecordDisposition, TaskDerivationListQuery, accept_stale_record, close_checklist,
    close_checklist_item, close_stale_record, decompose_design, list_checklist_items,
    list_checklists, list_design_requirements, list_stale_records, list_task_derivations,
    render_finding_fix_context, render_review_context,
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
            println!(
                "created_validation_gates: {}",
                outcome.created_validation_gates
            );
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
        ChecklistCommand::Close(args) => {
            let outcome = close_checklist(root, args.checklist_id)?;
            println!("closed checklist");
            println!("checklist_id: {}", outcome.checklist_id);
        }
        ChecklistCommand::Item { command } => match command {
            ChecklistItemCommand::List(args) => {
                let records = list_checklist_items(
                    root,
                    ChecklistItemListQuery {
                        checklist_id: args.checklist,
                        status: args.status.as_deref(),
                    },
                )?;
                if records.is_empty() {
                    println!("no checklist items");
                }
                for record in records {
                    let condition = record.completion_condition.as_deref().unwrap_or("-");
                    println!(
                        "{} [checklist={} work_unit={} design_version={} requirement={} task={} order={} {}] {} | completion={}",
                        record.id,
                        record.checklist_id,
                        record.work_unit_id,
                        record.design_version_id,
                        record.requirement_key,
                        record.task_id,
                        record.item_order,
                        record.status,
                        record.title,
                        condition
                    );
                }
            }
            ChecklistItemCommand::Close(args) => {
                let outcome = close_checklist_item(root, args.checklist_item_id)?;
                println!("closed checklist item");
                println!("checklist_item_id: {}", outcome.checklist_item_id);
            }
        },
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
        StaleCommand::Accept(args) => {
            let outcome = accept_stale_record(
                root,
                StaleRecordDisposition {
                    record_type: &args.record_type,
                    record_id: args.record_id,
                    reason: &args.reason,
                },
            )?;
            println!("accepted stale record");
            println!("record_type: {}", outcome.record_type);
            println!("record_id: {}", outcome.record_id);
            println!("acceptance_record_id: {}", outcome.acceptance_record_id);
            println!("authority_event_id: {}", outcome.authority_event_id);
        }
        StaleCommand::Close(args) => {
            let outcome = close_stale_record(
                root,
                StaleRecordDisposition {
                    record_type: &args.record_type,
                    record_id: args.record_id,
                    reason: &args.reason,
                },
            )?;
            println!("closed stale record");
            println!("record_type: {}", outcome.record_type);
            println!("record_id: {}", outcome.record_id);
            println!("status: {}", outcome.status);
            println!("acceptance_record_id: {}", outcome.acceptance_record_id);
            println!("authority_event_id: {}", outcome.authority_event_id);
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
    if args.kind == "finding-fix" {
        let finding = args
            .finding
            .ok_or_else(|| anyhow::anyhow!("finding-fix context requires --finding"))?;
        let closure = args
            .closure
            .ok_or_else(|| anyhow::anyhow!("finding-fix context requires --closure"))?;
        let attempt = args
            .attempt
            .ok_or_else(|| anyhow::anyhow!("finding-fix context requires --attempt"))?;
        let document = render_finding_fix_context(root, finding, closure, attempt)?;
        print!("{}", document.text);
        return Ok(());
    }
    let document = render_review_context(
        root,
        ReviewContextQuery {
            kind: &args.kind,
            design_version_id: args.design_version,
            work_unit_id: args.work_unit,
            phase_id: args.phase,
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
