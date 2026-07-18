use std::io::{self, Write};
use std::path::Path;

use anyhow::Result;

use agent_workbench::{
    ValidationLinkChange, diagnose_validation_links, list_validation_link_audit,
    repair_validation_links_with_backup_notice,
};

use super::args::{DoctorCommand, DoctorValidationLinksArgs};

pub(crate) fn handle(root: &Path, command: DoctorCommand) -> Result<()> {
    match command {
        DoctorCommand::ValidationLinks(args) => handle_validation_links(root, args),
    }
}

fn handle_validation_links(root: &Path, args: DoctorValidationLinksArgs) -> Result<()> {
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
    let diagnosis = diagnose_validation_links(root)?;
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
        println!("run_repairable: {}", run.repairable);
        for reason in run.reasons {
            println!("reason: {reason}");
        }
        for change in &run.changes {
            print_change(change);
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
