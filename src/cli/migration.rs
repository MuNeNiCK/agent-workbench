use std::path::Path;

use anyhow::Result;

use agent_workbench::{
    TaskIdentityAuthorityRequest, TaskIdentityDecisionRequest, apply_task_identity,
    audit_task_identity, decide_task_identity_ambiguity, list_task_identity_ambiguities,
    plan_task_identity, record_task_identity_authority,
};

use super::args::{MigrationCommand, TaskIdentityCommand};

pub(crate) fn handle(root: &Path, command: MigrationCommand) -> Result<()> {
    match command {
        MigrationCommand::TaskIdentity { command } => handle_task_identity(root, command),
    }
}

fn handle_task_identity(root: &Path, command: TaskIdentityCommand) -> Result<()> {
    match command {
        TaskIdentityCommand::Plan { owner } => {
            let output = plan_task_identity(root, owner.as_deref())?;
            println!("{}", output.json);
            Ok(())
        }
        TaskIdentityCommand::Apply(selection) => {
            let output = apply_task_identity(root, &selection.owner, &selection.plan)?;
            println!("result: {}", output.result);
            println!("backup_handle: {}", output.backup_handle);
            println!("audit_handle: {}", output.audit_handle);
            Ok(())
        }
        TaskIdentityCommand::Audit { owner } => {
            let output = audit_task_identity(root, owner.as_deref())?;
            println!("{}", output.json);
            Ok(())
        }
        TaskIdentityCommand::AmbiguityList(selection) => {
            let output = list_task_identity_ambiguities(root, &selection.owner, &selection.plan)?;
            println!("{}", output.json);
            Ok(())
        }
        TaskIdentityCommand::AuthorityRecord(args) => {
            let output = record_task_identity_authority(
                root,
                TaskIdentityAuthorityRequest {
                    owner_handle: &args.owner,
                    plan_handle: &args.plan,
                    ambiguity_handle: &args.ambiguity,
                    resolution_handle: args.resolution.as_deref(),
                    retire: args.retire,
                    statement: &args.statement,
                    provenance: &args.provenance,
                    provenance_ref: &args.provenance_ref,
                },
            )?;
            println!("authority_handle: {}", output.authority_handle);
            println!("recovery_handle: {}", output.recovery_handle);
            println!("backup_handle: {}", output.backup_handle);
            Ok(())
        }
        TaskIdentityCommand::AmbiguityDecide(args) => {
            let output = decide_task_identity_ambiguity(
                root,
                TaskIdentityDecisionRequest {
                    owner_handle: &args.owner,
                    plan_handle: &args.plan,
                    ambiguity_handle: &args.ambiguity,
                    resolution_handle: args.resolution.as_deref(),
                    retire: args.retire,
                    authority_handle: &args.authority,
                },
            )?;
            println!("decision_handle: {}", output.decision_handle);
            println!("recovery_handle: {}", output.recovery_handle);
            println!("{}", output.json);
            Ok(())
        }
    }
}
