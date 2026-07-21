use std::io::{self, Write};
use std::path::Path;

use anyhow::Result;

use agent_workbench::{
    ValidationLinkArtifactOutcome, ValidationLinkChange, diagnose_validation_link,
    diagnose_validation_links, list_validation_link_audit, repair_validation_link,
    repair_validation_links_with_backup_notice, retire_validation_link,
};

use super::args::{DoctorCommand, DoctorValidationLinkCommand, DoctorValidationLinksArgs};

pub(crate) fn handle(root: &Path, command: DoctorCommand) -> Result<()> {
    match command {
        DoctorCommand::ValidationLinks(args) => handle_validation_links(root, args),
    }
}

fn handle_validation_links(root: &Path, args: DoctorValidationLinksArgs) -> Result<()> {
    if let Some(command) = args.command {
        if args.artifact.is_some() || args.dry_run || args.repair || args.audit {
            anyhow::bail!(
                "validation_link_mode_conflict: explicit repair/retire cannot be combined with diagnosis or compatibility flags"
            );
        }
        let outcome = match command {
            DoctorValidationLinkCommand::Repair(input) => repair_validation_link(
                root,
                &input.artifact_ref,
                input.project,
                &input.expected_current,
            )?,
            DoctorValidationLinkCommand::Retire(input) => retire_validation_link(
                root,
                &input.artifact_ref,
                &input.reason,
                &input.expected_current,
            )?,
        };
        print_artifact_outcome(&outcome);
        return Ok(());
    }
    if args.audit {
        let runs = list_validation_link_audit(root)?;
        println!("validation_link_repair_runs: {}", runs.len());
        for run in runs {
            println!("repair_run_id: {}", run.repair_run_id);
            println!("backup: {}", run.backup_path);
            println!(
                "repaired_validation_runs: {}",
                run.repaired_validation_run_count
            );
            println!("change_count: {}", run.change_count);
            println!("created_at: {}", run.created_at);
            for change in run.changes {
                print_change(&ValidationLinkChange {
                    validation_run_id: change.validation_run_id,
                    entity_type: change.entity_type,
                    entity_id: change.entity_id,
                    field_name: change.field_name,
                    before_value: change.before_value,
                    after_value: change.after_value,
                });
            }
        }
        return Ok(());
    }

    if args.repair {
        let outcome = repair_validation_links_with_backup_notice(root, |path| {
            println!("backup: {}", path.display());
            io::stdout().flush()?;
            Ok(())
        })?;
        if outcome.repaired_validation_run_count == 0 {
            println!("validation_links: clean");
            println!("repair_status: no_changes");
        } else {
            println!("validation_links: repaired");
            println!("repair_status: committed");
            println!(
                "repair_run_id: {}",
                outcome.repair_run_id.unwrap_or_default()
            );
            println!(
                "repaired_validation_runs: {}",
                outcome.repaired_validation_run_count
            );
            println!("change_count: {}", outcome.change_count);
            println!("migration: pass");
            println!("integrity_validation: pass");
        }
        println!("next: agent-workbench status");
        return Ok(());
    }

    let _explicit_dry_run = args.dry_run;
    let diagnosis = match args.artifact.as_deref() {
        Some(artifact) => diagnose_validation_link(root, artifact)?,
        None => diagnose_validation_links(root)?,
    };
    println!("dry_run: true");
    if diagnosis.runs.is_empty() {
        println!("validation_links: clean");
        println!("invalid_validation_runs: 0");
        return Ok(());
    }
    println!("validation_links: invalid");
    println!("invalid_validation_runs: {}", diagnosis.runs.len());
    println!("repairable: {}", diagnosis.repairable);
    for run in diagnosis.runs {
        println!("validation_run_id: {}", run.validation_run_id);
        println!("artifact: {}", run.artifact_ref);
        if let Some(project) = run.expected_project_id {
            println!("expected_project: {project}");
        }
        println!("expected_current: {}", run.current_revision);
        println!("run_repairable: {}", run.repairable);
        for reason in run.reasons {
            println!("reason: {reason}");
        }
        for change in &run.changes {
            print_change(change);
        }
        if !run.repairable {
            println!("required_input: reason");
        }
        for action in run.legal_actions {
            println!("next: {action}");
        }
    }
    if diagnosis.repairable {
        println!("next: agent-workbench doctor validation-links --repair");
    } else {
        println!(
            "next: resolve the reported authority or required-link conflict, then rerun agent-workbench doctor validation-links"
        );
    }
    Ok(())
}

fn print_artifact_outcome(outcome: &ValidationLinkArtifactOutcome) {
    println!("artifact: {}", outcome.artifact_ref);
    println!("validation_run_id: {}", outcome.validation_run_id);
    println!("operation: {}", outcome.operation);
    println!("result_current: {}", outcome.result_current);
    if let Some(repair) = outcome.repair_run_id {
        println!("repair_run_id: {repair}");
    }
    if let Some(retirement) = outcome.retirement_id {
        println!("retirement_id: {retirement}");
    }
    if let Some(backup) = &outcome.backup_path {
        println!("backup: {}", backup.display());
    }
    println!("idempotent: {}", outcome.idempotent);
    println!("next: agent-workbench status");
}

fn print_change(change: &ValidationLinkChange) {
    println!(
        "change: validation_run={} {}:{} {} {} -> {}",
        change.validation_run_id,
        change.entity_type,
        change.entity_id,
        change.field_name,
        display_value(change.before_value.as_deref()),
        display_value(change.after_value.as_deref())
    );
}

fn display_value(value: Option<&str>) -> &str {
    value.unwrap_or("<null>")
}
