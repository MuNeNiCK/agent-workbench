use std::path::Path;

use anyhow::{Context, Result};

use super::args::{PhaseCommand, PhaseDependencyCommand, PhaseTraceCommand};
use agent_workbench::*;

pub(crate) fn handle_phase(root: &Path, command: PhaseCommand) -> Result<()> {
    match command {
        PhaseCommand::Create(args) => {
            let outcome = create_phase(
                root,
                NewWorkPhase {
                    work_unit_id: args.work_unit,
                    design_version_id: args.design_version,
                    key: &args.key,
                    title: &args.title,
                    kind: &args.kind,
                    order: args.order,
                    reason: args.reason.as_deref(),
                },
            )?;
            println!("created phase");
            println!("phase_id: {}", outcome.phase_id);
        }
        PhaseCommand::List(args) => {
            let records = list_phases(root, args.work_unit)?;
            if records.is_empty() {
                println!("no phases");
            }
            for record in records {
                print_phase_record(&record);
            }
        }
        PhaseCommand::Show(args) => {
            let record = show_phase(root, args.phase_id)?;
            print_phase_record(&record);
        }
        PhaseCommand::Assign(args) => {
            let outcome = assign_task_to_phase(root, args.phase_id, args.task)?;
            println!("assigned task to phase");
            println!("phase_id: {}", outcome.phase_id);
            println!("task_id: {}", outcome.task_id);
        }
        PhaseCommand::Dependency { command } => match command {
            PhaseDependencyCommand::Add(args) => {
                let outcome = add_phase_dependency(
                    root,
                    NewPhaseDependency {
                        from_phase_id: args.from_phase,
                        to_phase_id: args.to_phase,
                        dependency_type: &args.dependency_type,
                        reason: &args.reason,
                    },
                )?;
                println!("added phase dependency");
                println!("dependency_id: {}", outcome.dependency_id);
            }
            PhaseDependencyCommand::List(args) => {
                let records = list_phase_dependencies(root, args.work_unit)?;
                if records.is_empty() {
                    println!("no phase dependencies");
                }
                for record in records {
                    let evidence = record.evidence_ref.as_deref().unwrap_or("-");
                    let authority = record
                        .authority_event_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    println!(
                        "{} [{} {}] from={}({}) to={}({}) evidence={} authority={} reason={}",
                        record.id,
                        record.dependency_type,
                        record.status,
                        record.from_phase_id,
                        record.from_phase_key,
                        record.to_phase_id,
                        record.to_phase_key,
                        evidence,
                        authority,
                        record.reason
                    );
                }
            }
            PhaseDependencyCommand::Satisfy(args) => {
                let outcome = satisfy_phase_dependency(
                    root,
                    args.dependency_id,
                    &args.reason,
                    &args.evidence,
                )?;
                println!("satisfied phase dependency");
                println!("dependency_id: {}", outcome.dependency_id);
            }
            PhaseDependencyCommand::Accept(args) => {
                let outcome = accept_phase_dependency(
                    root,
                    args.dependency_id,
                    &args.reason,
                    args.authority,
                )?;
                println!("accepted phase dependency");
                println!("dependency_id: {}", outcome.dependency_id);
            }
        },
        PhaseCommand::Trace { command } => match command {
            PhaseTraceCommand::List(args) => {
                print_trace(list_phase_trace(root, args.phase_id)?);
            }
            PhaseTraceCommand::Decide(args) => {
                let (record_type, record_id) = parse_record_ref(&args.record)?;
                let outcome = decide_phase_trace(
                    root,
                    NewPhaseTraceDecision {
                        phase_id: args.phase,
                        record_type,
                        record_id,
                        decision: &args.decision,
                        reason: &args.reason,
                        authority_event_id: args.authority,
                    },
                )?;
                println!("recorded phase trace decision");
                println!("decision_id: {}", outcome.decision_id);
            }
        },
        PhaseCommand::Inventory(args) => {
            let inventory = phase_inventory(root, args.phase_id)?;
            println!("phase_id: {}", inventory.phase_id);
            print_trace(inventory.trace);
        }
        PhaseCommand::Rescope(args) => {
            let outcome = phase_rescope(
                root,
                PhaseRescope {
                    phase_id: args.phase,
                    to_work_unit_id: Some(args.to_work_unit),
                    shared_record_policy: &args.shared_record_policy,
                    dry_run: args.dry_run,
                },
            )?;
            print_rescope_outcome("phase rescope", &outcome, args.dry_run);
        }
        PhaseCommand::Split(args) => {
            let outcome = phase_split(
                root,
                PhaseSplit {
                    phase_id: args.phase_id,
                    title: &args.title,
                    reason: &args.reason,
                    shared_record_policy: &args.shared_record_policy,
                    dry_run: args.dry_run,
                },
            )?;
            print_rescope_outcome("phase split", &outcome, args.dry_run);
        }
        PhaseCommand::CloseReady(args) => {
            let outcome = phase_close_ready(root, args.phase_id)?;
            println!("phase: close-ready");
            println!("dry_run: {}", args.dry_run);
            println!("phase_id: {}", outcome.phase_id);
            if let Some(work_unit_id) = outcome.work_unit_id {
                println!("work_unit_id: {work_unit_id}");
            }
            println!("result: {}", outcome.result);
            for item in outcome.items {
                match item.blocking_action {
                    Some(action) => println!(
                        "{}: {} ({}) next={}",
                        item.name, item.result, item.details, action
                    ),
                    None => println!("{}: {} ({})", item.name, item.result, item.details),
                }
            }
        }
        PhaseCommand::Close(args) => {
            let outcome = close_phase(root, args.phase_id, &args.summary)?;
            println!("closed phase");
            println!("phase_id: {}", outcome.phase_id);
        }
        PhaseCommand::AcceptOutOfScope(args) => {
            let outcome =
                accept_phase_out_of_scope(root, args.phase_id, &args.reason, args.authority)?;
            println!("accepted phase out of scope");
            println!("phase_id: {}", outcome.phase_id);
            println!("authority_event_id: {}", outcome.authority_event_id);
        }
    }
    Ok(())
}

fn print_phase_record(record: &WorkPhaseRecord) {
    let design = record
        .design_version_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "-".to_string());
    let phase_work = record
        .phase_work_unit_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "-".to_string());
    println!(
        "{} [{} order={} kind={} tasks={} work_unit={} phase_work_unit={} design={}] {}",
        record.id,
        record.status,
        record.order,
        record.kind,
        record.task_count,
        record.work_unit_id,
        phase_work,
        design,
        record.title
    );
    println!("key: {}", record.key);
}

fn print_trace(records: Vec<PhaseTraceRecord>) {
    if records.is_empty() {
        println!("no phase trace records");
    }
    for record in records {
        let decision = record.decision.as_deref().unwrap_or("-");
        println!(
            "{}:{} [{} decision={}] {}",
            record.record_type, record.id, record.status, decision, record.label
        );
    }
}

fn print_rescope_outcome(label: &str, outcome: &PhaseRescopeOutcome, dry_run: bool) {
    println!("{label}");
    println!("dry_run: {dry_run}");
    println!("phase_id: {}", outcome.phase_id);
    println!("source_work_unit_id: {}", outcome.source_work_unit_id);
    if let Some(target_work_unit_id) = outcome.target_work_unit_id {
        println!("target_work_unit_id: {target_work_unit_id}");
    }
    println!("result: {}", outcome.result);
    println!("inventory:");
    if outcome.inventory.is_empty() {
        println!("- none");
    }
    for line in &outcome.inventory {
        println!("- {line}");
    }
    println!("blockers:");
    if outcome.blockers.is_empty() {
        println!("- none");
    }
    for blocker in &outcome.blockers {
        println!(
            "- {}: {} | next: {}",
            blocker.kind, blocker.details, blocker.next_action
        );
    }
    println!("warnings:");
    if outcome.warnings.is_empty() {
        println!("- none");
    }
    for warning in &outcome.warnings {
        println!("- {warning}");
    }
}

fn parse_record_ref(value: &str) -> Result<(&str, i64)> {
    let (record_type, id) = value
        .split_once(':')
        .with_context(|| "record must use <type:id>")?;
    Ok((record_type, id.parse()?))
}
