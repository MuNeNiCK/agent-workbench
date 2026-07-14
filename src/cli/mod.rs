use anyhow::Result;
use clap::Parser;
use std::env;

mod args;
mod classification;
mod design_flow;
mod doctor;
mod gate;
mod memory;
mod migration;
mod phases;
mod planning;
mod records;
mod review_ops;
mod work;

use args::*;

use agent_workbench::{
    NextAction, OwnerAction, PhaseBlocker, ProjectIntegrityStatus, init_project, next_action,
    project_status,
};
pub(crate) fn run() -> Result<()> {
    let cli = Cli::parse();
    let root = match cli.root {
        Some(root) => root,
        None => env::current_dir()?,
    };

    let publication = classification::publication_class(&cli.command);
    if let Some(label) = publication.label() {
        println!("classification: {label}");
    }

    let result: Result<()> = (|| {
        match cli.command {
            Command::Init => {
                init_project(&root)?;
                println!("initialized project state");
            }
            Command::Status => {
                let status = project_status(&root)?;
                if !status.initialized {
                    println!("not initialized");
                    println!("next: agent-workbench init");
                } else {
                    println!("initialized");
                    println!("open_work_units: {}", status.open_work_units);
                    println!("active_activations: {}", status.active_activations);
                    print_project_integrity(&status.project_integrity);
                    if status.project_integrity.result == "blocked" {
                        println!("project_blocked: true");
                        return Ok(());
                    }
                    println!("project_blocked: false");
                    if let Some(blocker) = status.phase_blocker {
                        println!("phase_blocked: true");
                        print_phase_blocker(&blocker);
                    } else {
                        println!("phase_blocked: false");
                        if !status.finding_remediations.is_empty() {
                            println!("finding_remediation: true");
                            println!(
                                "finding_remediation_count: {}",
                                status.finding_remediations.len()
                            );
                            for remediation in &status.finding_remediations {
                                print_finding_remediation_summary(remediation);
                            }
                        } else if !status.source_corrections.is_empty() {
                            println!("finding_remediation: false");
                            println!("source_correction: true");
                            println!(
                                "source_correction_count: {}",
                                status.source_corrections.len()
                            );
                            for correction in &status.source_corrections {
                                print_source_correction_summary(correction);
                            }
                        } else {
                            println!("finding_remediation: false");
                            println!("source_correction: false");
                        }
                        print_owner_actions(&status.owner_actions);
                    }
                }
            }
            Command::Next => match next_action(&root)? {
                NextAction::NotInitialized { ledger_path } => {
                    let _ = ledger_path;
                    println!("not initialized");
                    println!("next: agent-workbench init");
                }
                NextAction::BlockedPhase { blocker } => {
                    println!("blocked phase");
                    print_phase_blocker(&blocker);
                }
                NextAction::ProjectIntegrityBlocked { integrity } => {
                    println!("project integrity blocked");
                    print_project_integrity(&integrity);
                }
                NextAction::OwnerActions { owners } => {
                    println!("owner actions");
                    print_owner_actions(&owners);
                }
                NextAction::FindingRemediation { remediations } => {
                    println!("finding remediation");
                    println!("finding_remediation_count: {}", remediations.len());
                    for remediation in &remediations {
                        print_finding_remediation_summary(remediation);
                    }
                }
                NextAction::SourceCorrection { corrections } => {
                    println!("source correction");
                    println!("source_correction_count: {}", corrections.len());
                    for correction in &corrections {
                        print_source_correction_summary(correction);
                    }
                }
                NextAction::NoOpenWorkUnit => {
                    println!("no open work unit");
                    println!("next: agent-workbench work start <title>");
                }
                NextAction::ResumeSuspended { work_unit } => {
                    println!("suspended work unit");
                    println!("work_unit_id: {}", work_unit.id);
                    print_next_phase(&work_unit);
                    println!("next: agent-workbench resume-check --maturity trace-aware");
                    println!("then: agent-workbench work resume --check <resume-check-id>");
                }
                NextAction::ActivateOpen { work_unit } => {
                    println!("open inactive work unit");
                    println!("work_unit_id: {}", work_unit.id);
                    match work_unit.design_version_id {
                        Some(design_version_id) => {
                            println!(
                                "next: agent-workbench work activate --implementation --design-version {} {}",
                                design_version_id, work_unit.id
                            );
                        }
                        None => println!("next: agent-workbench work activate {}", work_unit.id),
                    }
                    print_next_phase(&work_unit);
                }
                NextAction::ContinueActive { work_unit } => {
                    println!("continue active work unit");
                    println!("work_unit_id: {}", work_unit.id);
                    print_next_phase(&work_unit);
                }
            },
            Command::Doctor { command } => doctor::handle(&root, command)?,
            Command::Migration { command } => migration::handle(&root, command)?,
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
            Command::Phase { command } => phases::handle_phase(&root, command)?,
            Command::Decision { command } => planning::handle_decision(&root, command)?,
            Command::Design { command } => planning::handle_design(&root, command)?,
            Command::Requirement { command } => planning::handle_requirement(&root, command)?,
            Command::DesignDecision { command } => {
                planning::handle_design_decision(&root, command)?
            }
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
    })();

    result.map_err(|error| classification::classify_error(error, &root, publication))
}

fn print_project_integrity(integrity: &ProjectIntegrityStatus) {
    println!("project_integrity: {}", integrity.result);
    if integrity.result == "blocked" {
        println!("inspect: agent-workbench doctor validation-links");
    }
}

fn print_owner_actions(owners: &[OwnerAction]) {
    println!("owner_action_count: {}", owners.len());
    for owner in owners {
        println!("owner: {}:{}", owner.owner_type, owner.owner_id);
        println!("owner_state: {}", owner.state);
        println!("owner_schedulable: {}", owner.schedulable);
        if let Some(kind) = owner.blocker_kind.as_deref() {
            println!("owner_blocker_kind: {kind}");
        }
        if owner.next_action.contains("review-context:") {
            println!("owner_next: agent-workbench finding list --status open");
        } else {
            println!("owner_next: {}", owner.next_action);
        }
    }
}

fn print_finding_remediation_summary(remediation: &agent_workbench::FindingRemediation) {
    println!("work_unit_id: {}", remediation.work_unit_id);
    println!("finding_id: {}", remediation.finding_id);
    println!("closure_id: {}", remediation.closure_id);
    println!("inspect: agent-workbench finding list --status open");
}

fn print_source_correction_summary(correction: &agent_workbench::SourceCorrection) {
    println!("work_unit_id: {}", correction.work_unit_id);
    println!("finding_id: {}", correction.finding_id);
    println!("closure_id: {}", correction.closure_id);
    println!("inspect: agent-workbench finding list --status open");
}

fn print_next_phase(work_unit: &agent_workbench::ActiveWorkUnit) {
    if let Some(phase_id) = work_unit.next_phase_id {
        println!("next_phase_id: {phase_id}");
    }
}

fn print_phase_blocker(blocker: &PhaseBlocker) {
    println!("blocker_kind: {}", blocker.kind);
    if let Some(work_unit_id) = blocker.work_unit_id {
        println!("work_unit_id: {work_unit_id}");
    }
    if let Some(finding_id) = blocker.finding_id {
        println!("finding_id: {finding_id}");
    }
    println!("next: {}", blocker.next_action);
}
