use anyhow::Result;
use std::path::Path;

use agent_workbench::{
    OperatorReleaseAssemble, OperatorReleaseAuthorityMutation, OperatorReleaseMutation,
    OperatorReleaseSupersession, ReleaseTransitionOutcome, operator_assemble_release,
    operator_inspect_release, operator_publish_release_assets, operator_publish_release_source,
    operator_reconcile_release, operator_retry_release, operator_supersede_release,
    operator_verify_release_remote, operator_withdraw_release,
};

use super::args::*;

pub(super) fn handle(root: &Path, command: OperatorCommand) -> Result<()> {
    match command {
        OperatorCommand::Release { command } => handle_release(root, command),
    }
}

fn handle_release(root: &Path, command: OperatorReleaseCommand) -> Result<()> {
    let outcome = match command {
        OperatorReleaseCommand::Candidate { command } => match command {
            OperatorReleaseCandidateCommand::Assemble(args) => operator_assemble_release(
                root,
                OperatorReleaseAssemble {
                    work_unit_id: args.work_unit_id,
                    version: args.version,
                    reviewed_commit: args.commit,
                    expected_current: args.expected_current,
                    idempotency_key: args.idempotency_key,
                },
            )?,
            OperatorReleaseCandidateCommand::Inspect(args) => {
                operator_inspect_release(root, mutation(args))?
            }
        },
        OperatorReleaseCommand::PublishSource(args) => {
            operator_publish_release_source(root, mutation(args))?
        }
        OperatorReleaseCommand::PublishAssets(args) => {
            operator_publish_release_assets(root, mutation(args))?
        }
        OperatorReleaseCommand::VerifyRemote(args) => {
            operator_verify_release_remote(root, mutation(args))?
        }
        OperatorReleaseCommand::Reconcile(args) => {
            operator_reconcile_release(root, mutation(args))?
        }
        OperatorReleaseCommand::Retry(args) => operator_retry_release(root, mutation(args))?,
        OperatorReleaseCommand::Withdraw(args) => operator_withdraw_release(
            root,
            OperatorReleaseAuthorityMutation {
                candidate: args.candidate,
                expected_current: args.expected_current,
                idempotency_key: args.idempotency_key,
                authority_event_id: args.authority_event_id,
                reason: args.reason,
            },
        )?,
        OperatorReleaseCommand::Supersede(args) => operator_supersede_release(
            root,
            OperatorReleaseSupersession {
                candidate: args.candidate,
                successor: args.by,
                expected_current: args.expected_current,
                idempotency_key: args.idempotency_key,
                authority_event_id: args.authority_event_id,
                reason: args.reason,
            },
        )?,
    };
    print_outcome(&outcome);
    Ok(())
}

fn mutation(args: OperatorReleaseMutationArgs) -> OperatorReleaseMutation {
    OperatorReleaseMutation {
        candidate: args.candidate,
        expected_current: args.expected_current,
        idempotency_key: args.idempotency_key,
    }
}

fn print_outcome(outcome: &ReleaseTransitionOutcome) {
    println!("candidate: {}", outcome.candidate_handle);
    if let Some(work_unit_id) = outcome.work_unit_id {
        println!("work_unit_id: {work_unit_id}");
    }
    println!("state: {}", outcome.state);
    println!("current_revision: {}", outcome.current_revision);
    println!("already_applied: {}", outcome.already_applied);
    println!("next: {}", outcome.next_action);
}
