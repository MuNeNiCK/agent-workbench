use anyhow::{Result, bail};
use clap::{CommandFactory, Parser};
use std::env;

mod args;
mod authority_ops;
mod classification;
mod design_flow;
mod doctor;
mod gate;
mod memory;
mod migration;
mod phases;
mod planning;
mod records;
mod release_ops;
mod review_ops;
mod work;

use args::*;

use agent_workbench::{
    NextAction, OwnerAction, PhaseBlocker, ProjectIntegrityStatus, UpdateDecisionAuthority,
    UpdateRecoveryAuthorityInput, apply_update, apply_update_operation,
    decide_update_with_authority, init_project_with_name, inspect_update, next_action_for,
    project_status_for, record_update_recovery_authority, restore_update, restore_update_operation,
};

fn reject_with_inspection_actions(reason: &str, next_actions: &[String]) -> Result<()> {
    let actions = next_actions
        .iter()
        .map(|action| format!("next: {action}"))
        .collect::<Vec<_>>()
        .join("\n");
    bail!("{reason}\n{actions}")
}

fn project_public_update_result<T>(
    root: &std::path::Path,
    operation: &str,
    expected_current: &str,
    retry: &str,
    result: Result<T>,
) -> Result<T> {
    result.map_err(|error| {
        let message = error.to_string();
        let is_public_contract_error = message.lines().any(|line| line.starts_with("next: "))
            || message.contains("; run agent-workbench ")
            || [
                "inspection handle ",
                "backup handle ",
                "expected current identity ",
                "idempotency key ",
            ]
            .iter()
            .any(|prefix| message.starts_with(prefix));
        if is_public_contract_error {
            return error;
        }
        match inspect_update(root) {
            Ok(inspection) => {
                let actions = if inspection.current_identity == expected_current {
                    inspection.next_actions
                } else {
                    vec![retry.to_string()]
                };
                anyhow::anyhow!(
                    "update {operation} could not be completed\nupdate_status: {}\n{}",
                    inspection.status,
                    actions
                        .iter()
                        .map(|action| format!("next: {action}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                )
            }
            Err(_) => anyhow::anyhow!(
                "update {operation} could not be completed\nupdate_status: owner_input_required\nnext: agent-workbench update inspect"
            ),
        }
    })
}

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
            Command::Init { name } => {
                init_project_with_name(&root, name.as_deref())?;
                println!("initialized project state");
            }
            Command::Update { command } => match command {
                UpdateCommand::Inspect => {
                    let inspection = inspect_update(&root)?;
                    println!("inspection_handle: {}", inspection.inspection_handle);
                    println!("current_identity: {}", inspection.current_identity);
                    println!("update_status: {}", inspection.status);
                    println!("update_required: {}", inspection.status != "current");
                    for capability in inspection.preserved_capabilities {
                        println!("preserved_capability: {capability}");
                    }
                    for backup in inspection.restorable_backups {
                        println!("backup: {backup}");
                    }
                    for choice in inspection.decision_choices {
                        println!("decision_choice: {choice}");
                    }
                    for next in inspection.next_actions {
                        println!("next: {next}");
                    }
                }
                UpdateCommand::AuthorityRecord(args) => {
                    let outcome = record_update_recovery_authority(
                        &root,
                        UpdateRecoveryAuthorityInput {
                            inspection_handle: &args.inspection_handle,
                            choice: &args.choice,
                            statement: &args.statement,
                            provenance: &args.provenance,
                            provenance_ref: &args.provenance_ref,
                            expected_current: &args.expected_current,
                            idempotency_key: &args.idempotency_key,
                        },
                    )?;
                    println!("authority_handle: {}", outcome.authority_handle);
                    println!("already_recorded: {}", outcome.already_recorded);
                    println!("next: {}", outcome.next_action);
                }
                UpdateCommand::Decide(args) => {
                    let authority = match (args.authority, args.recovery_authority.as_deref()) {
                        (Some(authority), None) => UpdateDecisionAuthority::ProjectEvent(authority),
                        (None, Some(authority)) => UpdateDecisionAuthority::Recovery(authority),
                        _ => unreachable!("clap enforces the authority sum"),
                    };
                    let outcome = decide_update_with_authority(
                        &root,
                        &args.inspection_handle,
                        &args.choice,
                        authority,
                        &args.reason,
                        &args.expected_current,
                    )?;
                    println!("inspection_handle: {}", outcome.inspection_handle);
                    println!("decision_handle: {}", outcome.decision_handle);
                    println!("already_applied: {}", outcome.already_applied);
                    println!("next: {}", outcome.next_action);
                }
                UpdateCommand::Apply(args) => {
                    let outcome = match (&args.inspection_handle, &args.idempotency_key) {
                        (Some(inspection_handle), Some(idempotency_key)) => {
                            let retry = format!(
                                "agent-workbench update apply {inspection_handle} --expected-current {} --idempotency-key {idempotency_key}",
                                args.expected_current
                            );
                            project_public_update_result(
                                &root,
                                "apply",
                                &args.expected_current,
                                &retry,
                                apply_update_operation(
                                    &root,
                                    inspection_handle,
                                    &args.expected_current,
                                    idempotency_key,
                                ),
                            )?
                        }
                        (None, None) => {
                            let retry = format!(
                                "agent-workbench update apply --expected-current {}",
                                args.expected_current
                            );
                            project_public_update_result(
                                &root,
                                "apply",
                                &args.expected_current,
                                &retry,
                                apply_update(&root, &args.expected_current),
                            )?
                        }
                        _ => {
                            let inspection = inspect_update(&root)?;
                            reject_with_inspection_actions(
                                "update apply requires either the established form or the complete inspected form",
                                &inspection.next_actions,
                            )?;
                            unreachable!()
                        }
                    };
                    println!("operation_handle: {}", outcome.operation_handle);
                    println!("source_identity: {}", outcome.source_identity);
                    println!("result_identity: {}", outcome.result_identity);
                    println!("backup_identity: {}", outcome.backup_identity);
                    println!("already_applied: {}", outcome.already_applied);
                }
                UpdateCommand::Restore(args) => {
                    let outcome = if let Some(idempotency_key) = &args.idempotency_key {
                        let retry = format!(
                            "agent-workbench update restore --backup {} --expected-current {} --idempotency-key {idempotency_key}",
                            args.backup, args.expected_current
                        );
                        project_public_update_result(
                            &root,
                            "restore",
                            &args.expected_current,
                            &retry,
                            restore_update_operation(
                                &root,
                                &args.backup,
                                &args.expected_current,
                                idempotency_key,
                            ),
                        )?
                    } else {
                        let retry = format!(
                            "agent-workbench update restore --backup {} --expected-current {}",
                            args.backup, args.expected_current
                        );
                        project_public_update_result(
                            &root,
                            "restore",
                            &args.expected_current,
                            &retry,
                            restore_update(&root, &args.backup, &args.expected_current),
                        )?
                    };
                    println!("operation_handle: {}", outcome.operation_handle);
                    println!("restored_identity: {}", outcome.restored_identity);
                    println!(
                        "recovery_backup_identity: {}",
                        outcome.recovery_backup_identity
                    );
                    println!("already_applied: {}", outcome.already_applied);
                }
            },
            Command::Operator { command } => release_ops::handle(&root, command)?,
            Command::Status(args) => {
                let status = project_status_for(&root, args.work)?;
                if !status.initialized {
                    println!("not initialized");
                    println!("next: agent-workbench init");
                } else {
                    println!("initialized");
                    if let Some(work_unit_id) = args.work {
                        println!("selected_work_unit_id: {work_unit_id}");
                    }
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
            Command::Next(args) => match next_action_for(&root, args.work)? {
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
                NextAction::SelectedWorkTerminal {
                    work_unit_id,
                    status,
                } => {
                    println!("selected work unit is terminal");
                    println!("work_unit_id: {work_unit_id}");
                    println!("status: {status}");
                    println!("next: none");
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
            Command::Verification { command } => review_ops::handle_verification(&root, command)?,
            Command::Closure { command } => review_ops::handle_closure(&root, command)?,
            Command::Acceptance { command } => review_ops::handle_acceptance(&root, command)?,
            Command::Authority { command } => authority_ops::handle_authority(&root, command)?,
            Command::Kpt { command } => review_ops::handle_kpt(&root, command)?,
            Command::Decompose { command } => design_flow::handle_decompose(&root, command)?,
            // Keep every Decomposition entry on the shared typed resolver and renderer.
            Command::Decomposition { command } => {
                design_flow::handle_decomposition(&root, command)?
            }
            Command::Checklist { command } => design_flow::handle_checklist(&root, command)?,
            Command::Stale { command } => design_flow::handle_stale(&root, command)?,
            Command::ReviewContext(args) => design_flow::print_review_context(&root, &args)?,
            Command::Export { command } => design_flow::handle_export(&root, command)?,
            Command::Help(args) => print_route_help(args)?,
        }
        Ok(())
    })();

    result.map_err(|error| classification::classify_error(error, &root, publication))
}

fn print_route_help(args: HelpArgs) -> Result<()> {
    let parts = match args.route {
        Some(route) => route
            .split(|character: char| character == '/' || character.is_whitespace())
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>(),
        None => args.route_parts,
    };
    let mut command = Cli::command();
    let mut selected = &mut command;
    for part in &parts {
        selected = selected
            .find_subcommand_mut(part)
            .ok_or_else(|| anyhow::anyhow!("unknown help route: {}", parts.join(" ")))?;
    }
    selected.set_bin_name(format!("agent-workbench {}", parts.join(" ")));
    selected.print_long_help()?;
    println!();
    Ok(())
}

fn print_project_integrity(integrity: &ProjectIntegrityStatus) {
    println!("project_integrity: {}", integrity.result);
    if integrity.result == "blocked" {
        let action = integrity
            .predicates
            .iter()
            .find(|predicate| predicate.result == "blocked")
            .and_then(|predicate| predicate.next_action.as_deref())
            .unwrap_or("agent-workbench doctor validation-links");
        println!("next: {action}");
    }
}

fn print_owner_actions(owners: &[OwnerAction]) {
    println!("owner_action_count: {}", owners.len());
    for owner in owners {
        println!(
            "owner: {}:{}",
            owner.owner_type,
            owner
                .owner_handle
                .as_deref()
                .map(str::to_string)
                .unwrap_or_else(|| owner.owner_id.to_string())
        );
        println!("owner_state: {}", owner.state);
        println!("owner_schedulable: {}", owner.schedulable);
        if let Some(kind) = owner.blocker_kind.as_deref() {
            println!("owner_blocker_kind: {kind}");
        }
        for action in &owner.next_actions {
            if action.contains("review-context:") {
                println!("owner_next: agent-workbench finding list --status open");
            } else {
                println!("owner_next: {action}");
            }
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
    println!("description: {}", correction.description);
    println!("affected_surfaces: {}", correction.affected_surfaces);
    println!("fix_plan: {}", correction.fix_plan);
    println!("tests_or_gates: {}", correction.tests_or_gates);
    println!("verification_plan: {}", correction.verification_plan);
    println!("next: {}", correction.next_action);
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
