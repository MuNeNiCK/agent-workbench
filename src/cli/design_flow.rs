use std::fs;
use std::path::Path;

use anyhow::Result;

use super::args::{
    ChecklistCommand, DecomposeCommand, ExportCommand, ReviewContextArgs, StaleCommand,
};
use agent_workbench::{
    CoverageItemListQuery, DesignDecomposition, DesignRequirementListQuery,
    TaskDerivationListQuery, TaskListQuery, decompose_design, list_checklists, list_coverage_items,
    list_design_requirements, list_stale_records, list_task_derivations, list_tasks,
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
    println!("review_context: {}", args.kind);
    if let Some(design_version_id) = args.design_version {
        println!("design_version_id: {design_version_id}");
        let requirements =
            list_design_requirements(root, DesignRequirementListQuery { design_version_id })?;
        println!("requirements:");
        if requirements.is_empty() {
            println!("- none");
        }
        for requirement in requirements {
            let validation = requirement.validation_expectation.as_deref().unwrap_or("-");
            println!(
                "- {} [{}:{} validation={}] {}",
                requirement.requirement_key,
                requirement.priority,
                requirement.status,
                validation,
                requirement.requirement_text.lines().next().unwrap_or("")
            );
        }

        let derivations =
            list_task_derivations(root, TaskDerivationListQuery { design_version_id })?;
        println!("task_derivations:");
        if derivations.is_empty() {
            println!("- none");
        }
        for derivation in derivations {
            println!(
                "- requirement={} task={} [{}] {}",
                derivation.requirement_key,
                derivation.task_id,
                derivation.status,
                derivation.task_title
            );
        }

        let coverage = list_coverage_items(
            root,
            CoverageItemListQuery {
                design_version_id,
                status: None,
            },
        )?;
        println!("coverage_items:");
        if coverage.is_empty() {
            println!("- none");
        }
        for item in &coverage {
            let task = item
                .task_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "-".to_string());
            let tests = item.tests_or_gates.as_deref().unwrap_or("-");
            let gap = item.missing_or_unverified.as_deref().unwrap_or("-");
            println!(
                "- {} coverage={} task={} tests={} gap={} {}",
                item.requirement_key,
                item.status,
                task,
                tests,
                gap,
                item.requirement.lines().next().unwrap_or("")
            );
        }

        println!("known_gaps:");
        let mut printed_gap = false;
        for item in coverage.iter().filter(|item| {
            matches!(
                item.status.as_str(),
                "partial" | "missing_required_surface" | "design_conflict" | "needs_evidence"
            ) || item.missing_or_unverified.is_some()
        }) {
            printed_gap = true;
            let gap = item
                .missing_or_unverified
                .as_deref()
                .unwrap_or("coverage incomplete");
            println!("- coverage:{} [{}] {}", item.id, item.status, gap);
        }
        if !printed_gap {
            println!("- none");
        }
    }
    if let Some(work_unit_id) = args.work_unit {
        println!("work_unit_id: {work_unit_id}");
        let tasks = list_tasks(
            root,
            TaskListQuery {
                status: None,
                work_unit_id: Some(work_unit_id),
            },
        )?;
        println!("tasks:");
        if tasks.is_empty() {
            println!("- none");
        }
        for task in tasks {
            println!(
                "- {} [{}:{}] {}",
                task.id, task.priority, task.status, task.title
            );
        }
    }
    let stale = list_stale_records(root)?;
    println!("stale_records:");
    if stale.is_empty() {
        println!("- none");
    }
    for record in stale {
        println!("- {}:{} {}", record.record_type, record.id, record.label);
    }
    Ok(())
}

fn write_export(path: &std::path::Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
}
