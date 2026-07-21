use std::fs;
use std::path::Path;

use anyhow::Result;

use super::args::{
    ChecklistCommand, ChecklistItemCommand, DecomposeCommand, DecompositionCommand, ExportCommand,
    ReviewContextArgs, StaleCommand,
};
use agent_workbench::{
    ChecklistItemListQuery, ChecklistListFilter, DecompositionApplication, DecompositionImport,
    DecompositionPlanQuery, DecompositionPlanRecord, DecompositionPlanResolution,
    DecompositionReconciliationApplication, DecompositionRevise, DecompositionValidate,
    DesignDecomposition, DesignRequirementListQuery, ReviewContextQuery, StaleRecordDisposition,
    TaskDerivationListFilter, accept_stale_record, apply_decomposition_plan, close_checklist,
    close_checklist_item, close_stale_record, decompose_design, design_version_for_work,
    import_decomposition_plan, list_checklist_items, list_checklists_filtered,
    list_design_requirements, list_stale_records_filtered, list_task_derivations_filtered,
    preview_decomposition_reconciliation, reconcile_decomposition_plan, render_finding_fix_context,
    render_review_context, resolve_decomposition_plan, resolve_design_version_ref,
    validate_decomposition_plan,
};

pub(crate) fn handle_decompose(root: &Path, command: DecomposeCommand) -> Result<()> {
    match command {
        DecomposeCommand::Design(args) => {
            decompose_design(
                root,
                DesignDecomposition {
                    design_version_id: args.design_version_id,
                    work_unit_id: args.work_unit,
                    checklist_title: args.checklist_title.as_deref(),
                    reason: args.reason.as_deref(),
                },
            )?;
            println!("decomposed design");
        }
    }
    Ok(())
}

pub(crate) fn handle_decomposition(root: &Path, command: DecompositionCommand) -> Result<()> {
    match command {
        DecompositionCommand::Import(args) => {
            let outcome = import_decomposition_plan(
                root,
                DecompositionImport {
                    design_version_id: args.design_version_id,
                    work_unit_id: args.work_unit_id,
                    plan_path: args.plan.as_deref(),
                    expected_content: args.expected_content.as_deref(),
                    draft: args.draft,
                    expected_current: &args.expected_current,
                    idempotency_key: &args.idempotency_key,
                },
            )?;
            println!("imported decomposition plan");
            print_plan(&outcome.plan);
            println!("idempotent: {}", outcome.idempotent);
            print_current_resolution(root, &outcome.plan)?;
        }
        DecompositionCommand::Validate(args) => {
            let outcome = validate_decomposition_plan(
                root,
                DecompositionValidate {
                    plan_id: args.plan_id,
                    expected_current: &args.expected_current,
                    idempotency_key: &args.idempotency_key,
                },
            )?;
            println!("validated decomposition plan");
            print_plan(&outcome.plan);
            println!("idempotent: {}", outcome.idempotent);
            print_current_resolution(root, &outcome.plan)?;
        }
        DecompositionCommand::Revise(args) => {
            let outcome = DecompositionRevise::execute_request(
                root,
                args.plan_id,
                args.plan.as_deref(),
                args.expected_content.as_deref(),
                args.draft,
                &args.expected_current,
                &args.idempotency_key,
            )?;
            println!("revised decomposition plan");
            print_plan(&outcome.plan);
            println!("idempotent: {}", outcome.idempotent);
            print_current_resolution(root, &outcome.plan)?;
        }
        DecompositionCommand::Apply(args) => {
            let outcome = apply_decomposition_plan(
                root,
                DecompositionApplication {
                    design_version_id: args.design_version_id,
                    work_unit_id: args.work_unit_id,
                    plan_path: args.plan.as_deref(),
                },
            )?;
            if outcome.applied {
                println!("applied decomposition plan");
            } else {
                println!("resolved decomposition apply without publication");
            }
            println!("plan: {}", outcome.plan_id);
            println!("tasks: {}", outcome.task_count);
            println!("checklist_items: {}", outcome.checklist_item_count);
            println!("phases: {}", outcome.phase_count);
            println!("dependencies: {}", outcome.dependency_count);
            println!("already_applied: {}", outcome.already_applied);
            let resolution = resolve_decomposition_plan(
                root,
                DecompositionPlanQuery {
                    design_version_id: args.design_version_id,
                    work_unit_id: args.work_unit_id,
                },
            )?;
            print_resolution(&resolution);
        }
        DecompositionCommand::Show(args) => {
            let resolution = resolve_decomposition_plan(
                root,
                DecompositionPlanQuery {
                    design_version_id: args.design_version_id,
                    work_unit_id: args.work_unit_id,
                },
            )?;
            print_resolution(&resolution);
        }
        DecompositionCommand::Reconcile(args) => {
            if args.dry_run {
                let outcome = preview_decomposition_reconciliation(
                    root,
                    DecompositionReconciliationApplication {
                        design_version_id: args.design_version_id,
                        work_unit_id: args.work_unit_id,
                        plan_path: &args.plan,
                        closure_id: args.closure,
                        expected_current: &args.expected_current,
                    },
                )?;
                println!("decomposition reconciliation preview");
                println!("predecessor: {}", outcome.predecessor_plan_id);
                println!("closure: {}", outcome.closure_id);
                println!("token: {}", outcome.token_ordinal);
                println!("tasks: {}", outcome.plan.task_count);
                println!("checklist_items: {}", outcome.plan.checklist_item_count);
                println!("phases: {}", outcome.plan.phase_count);
                println!("dependencies: {}", outcome.plan.dependency_count);
                println!("idempotent: {}", outcome.idempotent);
                print_reconciliation_projection(&outcome);
                let resolution = resolve_decomposition_plan(
                    root,
                    DecompositionPlanQuery {
                        design_version_id: args.design_version_id,
                        work_unit_id: args.work_unit_id,
                    },
                )?;
                print_resolution(&resolution);
                return Ok(());
            }
            let outcome = reconcile_decomposition_plan(
                root,
                DecompositionReconciliationApplication {
                    design_version_id: args.design_version_id,
                    work_unit_id: args.work_unit_id,
                    plan_path: &args.plan,
                    closure_id: args.closure,
                    expected_current: &args.expected_current,
                },
            )?;
            println!("reconciled decomposition plan");
            println!("plan: {}", outcome.plan.plan_id);
            println!("predecessor: {}", outcome.predecessor_plan_id);
            println!("closure: {}", outcome.closure_id);
            println!("token: {}", outcome.token_ordinal);
            println!("tasks: {}", outcome.plan.task_count);
            println!("checklist_items: {}", outcome.plan.checklist_item_count);
            println!("phases: {}", outcome.plan.phase_count);
            println!("dependencies: {}", outcome.plan.dependency_count);
            println!("idempotent: {}", outcome.idempotent);
            print_reconciliation_projection(&outcome);
        }
    }
    Ok(())
}

fn print_reconciliation_projection(outcome: &agent_workbench::DecompositionReconciliationOutcome) {
    let projection = &outcome.projection;
    println!("projection_identity: {}", projection.projection_identity);
    println!("observed_predecessor: {}", projection.observed_predecessor);
    println!("observed_document: {}", projection.observed_document);
    println!("observed_correction: {}", projection.observed_correction);
    println!("observed_shared: {}", projection.observed_shared);
    println!("commit_current: {}", projection.commit_current);
    for effect in &projection.endpoint_effects {
        println!(
            "projected_owned_effect: {} source={} target={} disposition={} effect={} qualification={} observed={}",
            effect.category,
            effect.source_id,
            effect.target.as_deref().unwrap_or("-"),
            effect.disposition,
            effect.effect.as_deref().unwrap_or("-"),
            effect.qualification,
            effect.observed_handle
        );
    }
    for binding in &projection.shared_bindings {
        println!(
            "projected_shared_binding: {} id={} owner={} disposition={} qualification={} observed={}",
            binding.kind,
            binding.id,
            binding.owner,
            binding.disposition,
            binding.qualification,
            binding.observed_handle
        );
    }
    println!("execute: {}", projection.command);
}

fn print_resolution(resolution: &DecompositionPlanResolution) {
    println!("requested_design_version: {}", resolution.design_version_id);
    println!("requested_work: {}", resolution.work_unit_id);
    match resolution.current.as_ref() {
        Some(plan) => print_plan(plan),
        None => println!("status: absent"),
    }
    if let Some(plan) = resolution.successor.as_ref() {
        println!("successor_plan: {}", plan.id);
        println!("successor_revision: {}", plan.revision);
        println!("successor_status: {}", plan.status);
        println!("successor_current_identity: {}", plan.current_identity);
        println!("successor_content_identity: {}", plan.content_identity);
    }
    if let Some(projection) = resolution.successor_projection.as_ref() {
        println!(
            "successor_projection_identity: {}",
            projection.projection_identity
        );
        for effect in &projection.endpoint_effects {
            println!(
                "projected_owned_effect: {} source={} target={} disposition={} effect={} qualification={} observed={}",
                effect.category,
                effect.source_id,
                effect.target.as_deref().unwrap_or("-"),
                effect.disposition,
                effect.effect.as_deref().unwrap_or("-"),
                effect.qualification,
                effect.observed_handle
            );
        }
        for binding in &projection.shared_bindings {
            println!(
                "projected_shared_binding: {} id={} owner={} disposition={} qualification={} observed={}",
                binding.kind,
                binding.id,
                binding.owner,
                binding.disposition,
                binding.qualification,
                binding.observed_handle
            );
        }
    }
    for candidate in &resolution.candidates {
        println!("candidate: {}", candidate.source_path);
        println!("candidate_identity: {}", candidate.ingress_identity);
        println!("candidate_ready: {}", candidate.structurally_ready);
        if let Some(issue) = candidate.issue.as_deref() {
            println!("candidate_issue: {issue}");
        }
    }
    if let Some(owner) = resolution.review_owner.as_ref() {
        println!("review_owner_state: {}", owner.state);
        println!("review_owner_current: {}", owner.observed_handle);
        println!("review_context: {}", owner.context_ref);
    }
    for action in &resolution.actions {
        println!("next: {action}");
    }
}

fn print_current_resolution(root: &Path, plan: &DecompositionPlanRecord) -> Result<()> {
    let resolution = resolve_decomposition_plan(
        root,
        DecompositionPlanQuery {
            design_version_id: plan.design_version_id,
            work_unit_id: plan.work_unit_id,
        },
    )?;
    print_resolution(&resolution);
    Ok(())
}

fn print_plan(plan: &DecompositionPlanRecord) {
    println!("plan: {}", plan.id);
    println!("key: {}", plan.key);
    println!("design_version: {}", plan.design_version_id);
    println!("work: {}", plan.work_unit_id);
    println!("revision: {}", plan.revision);
    println!("current_identity: {}", plan.current_identity);
    println!("content_identity: {}", plan.content_identity);
    println!("status: {}", plan.status);
    if let Some(predecessor) = plan.predecessor_id {
        println!("predecessor: {predecessor}");
    }
    if let Some(issue) = plan.issue.as_deref() {
        println!("issue: {issue}");
    }
    for slice in &plan.slices {
        println!(
            "slice: {} order={} depends_on={}",
            slice.key,
            slice.order,
            slice.depends_on.join(",")
        );
    }
    for item in &plan.items {
        println!(
            "item: {} slice={}",
            item.key,
            item.slice.as_deref().unwrap_or("-")
        );
        println!("item_requirements: {}", item.requirements.join(","));
        println!("item_checklist: {}", item.checklist_boundaries.join(","));
        println!("item_gates: {}", item.gates.join(","));
    }
    for gap in &plan.gaps {
        println!("gap: {} | {}", gap.endpoint, gap.issue);
    }
    for mapping in &plan.mappings {
        println!(
            "owned_mapping: {} source={} target={} disposition={} effect={} qualification={} observed={}",
            mapping.category,
            mapping.source_id,
            mapping.target.as_deref().unwrap_or("-"),
            mapping.disposition,
            mapping.effect.as_deref().unwrap_or("-"),
            mapping.qualification,
            mapping.observed_handle
        );
    }
    for binding in &plan.shared_bindings {
        println!(
            "shared_binding: {} id={} owner={} disposition={} qualification={} observed={}",
            binding.kind,
            binding.id,
            binding.owner,
            binding.disposition,
            binding.qualification,
            binding.observed_handle
        );
    }
}

pub(crate) fn handle_checklist(root: &Path, command: ChecklistCommand) -> Result<()> {
    match command {
        ChecklistCommand::List(args) => {
            let records = list_checklists_filtered(
                root,
                ChecklistListFilter {
                    status: args.status.as_deref(),
                    work_unit_id: args.work,
                },
            )?;
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
            close_checklist(root, args.checklist_id)?;
            println!("closed checklist");
        }
        ChecklistCommand::Item { command } => match command {
            ChecklistItemCommand::List(args) => {
                let records = list_checklist_items(
                    root,
                    ChecklistItemListQuery {
                        checklist_id: args.checklist_positional.or(args.checklist),
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
                close_checklist_item(root, args.checklist_item_id)?;
                println!("closed checklist item");
            }
        },
    }
    Ok(())
}

pub(crate) fn handle_stale(root: &Path, command: StaleCommand) -> Result<()> {
    match command {
        StaleCommand::List(args) => {
            let records = list_stale_records_filtered(root, args.record_type.as_deref())?;
            if records.is_empty() {
                println!("no stale records");
            }
            for record in records {
                println!("{}:{} {}", record.record_type, record.id, record.label);
            }
        }
        StaleCommand::Accept(args) => {
            accept_stale_record(
                root,
                StaleRecordDisposition {
                    record_type: &args.record_type,
                    record_id: args.record_id,
                    reason: &args.reason,
                },
            )?;
            println!("accepted stale record");
        }
        StaleCommand::Close(args) => {
            close_stale_record(
                root,
                StaleRecordDisposition {
                    record_type: &args.record_type,
                    record_id: args.record_id,
                    reason: &args.reason,
                },
            )?;
            println!("closed stale record");
        }
    }
    Ok(())
}

pub(crate) fn handle_export(root: &Path, command: ExportCommand) -> Result<()> {
    match command {
        ExportCommand::Design(args) => {
            let (design_version_id, classification) = match (
                args.design,
                args.design_positional,
                args.classification.as_deref(),
            ) {
                (Some(design), None, None) => (design, "project-internal"),
                (None, Some(design), Some(classification)) => (design, classification),
                _ => anyhow::bail!(
                    "export design requires either installed --design or positional design with --classification"
                ),
            };
            let records =
                list_design_requirements(root, DesignRequirementListQuery { design_version_id })?;
            let mut output = format!(
                "classification: {classification}\n\n# Design {design_version_id}\n\n## Requirements\n\n"
            );
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
        }
        ExportCommand::Plan(args) => {
            let (design_version_id, work_unit_id, classification) = match (
                args.design,
                args.work_positional,
                args.classification.as_deref(),
            ) {
                (Some(design), None, None) => (design, None, "project-internal"),
                (None, Some(work), Some(classification)) => (
                    design_version_for_work(root, work)?,
                    Some(work),
                    classification,
                ),
                _ => anyhow::bail!(
                    "export plan requires either installed --design or positional work with --classification"
                ),
            };
            let records = list_task_derivations_filtered(
                root,
                TaskDerivationListFilter {
                    design_version_id: Some(design_version_id),
                    task_id: None,
                    work_unit_id,
                },
            )?;
            let mut output = format!(
                "classification: {classification}\n\n# Implementation Plan {design_version_id}\n\n"
            );
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
        print_review_document(&document.text);
        return Ok(());
    }
    let design_version_id = args
        .design_version
        .as_deref()
        .map(|reference| resolve_design_version_ref(root, reference))
        .transpose()?;
    let document = render_review_context(
        root,
        ReviewContextQuery {
            kind: &args.kind,
            design_version_id,
            work_unit_id: args.work_unit,
            phase_id: args.phase,
        },
    )?;
    print_review_document(&document.text);
    Ok(())
}

fn print_review_document(text: &str) {
    print!(
        "{}",
        text.strip_prefix("classification: project-internal\n")
            .unwrap_or(text)
    );
}

fn write_export(path: &std::path::Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)?;
    Ok(())
}
