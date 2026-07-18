use std::path::Path;

use anyhow::{Result, bail};

use agent_workbench::{update_dry_run, update_reset, update_restore};

use super::args::{UpdateArgs, UpdateCommand};

pub(crate) fn handle(root: &Path, args: UpdateArgs) -> Result<()> {
    match (args.dry_run, args.reset, args.command) {
        (true, false, None) => print_plan(&update_dry_run(root)?),
        (false, true, None) => {
            let reason = args.reason.as_deref().unwrap_or_default();
            let outcome = update_reset(root, reason)?;
            print_plan(&outcome.plan);
            println!("backup_handle: {}", outcome.backup_handle);
            println!(
                "result: {}",
                if outcome.plan.already_applied {
                    "already_applied"
                } else {
                    "reset"
                }
            );
        }
        (false, false, Some(UpdateCommand::Restore(restore))) => {
            let outcome = update_restore(root, &restore.backup, &restore.expected_current)?;
            println!("operation_id: {}", outcome.operation_id);
            println!("result_identity: {}", outcome.result_identity);
            println!(
                "recovery_backup_identity: {}",
                outcome.recovery_backup_identity
            );
            println!("receipt: {}", outcome.receipt_path.display());
            println!(
                "result: {}",
                if outcome.already_applied {
                    "already_applied"
                } else {
                    "restored"
                }
            );
        }
        _ => bail!(
            "choose exactly one of update --dry-run, update --reset --reason <reason>, or update restore"
        ),
    }
    Ok(())
}

fn print_plan(plan: &agent_workbench::UpdatePlan) {
    println!("source_schema: {}", plan.source_schema);
    println!("source_profile: {}", plan.source_profile);
    println!("source_identity: {}", plan.source_identity);
    println!("target_schema: {}", plan.target_schema);
    println!("target_profile: {}", plan.target_profile);
    println!("backup: {}", plan.backup_path.display());
    println!("target_rows: schema_metadata=1 projects=1 legacy_ledgers=1 update_audits=1");
    println!("domain_rows_imported: 0");
    for (table, count) in &plan.nonempty_tables {
        println!("source_nonempty: {table}={count}");
    }
    println!("already_applied: {}", plan.already_applied);
}
