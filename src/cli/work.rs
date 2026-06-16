use std::path::Path;

use anyhow::Result;

use super::args::{ResumeCheckArgs, WorkCommand};
use agent_workbench::{
    NewWorkFork, WorkActivate, WorkForkSource, WorkReopen, WorkStart, abandon_work, activate_work,
    block_work, close_active_work, create_follow_up_work, fork_work, interrupt_work, reopen_work,
    resume_check, resume_work, start_work_with_options, suspend_work, unblock_work,
};

pub(crate) fn handle(root: &Path, command: WorkCommand) -> Result<()> {
    match command {
        WorkCommand::Start(args) => {
            let outcome = if let Some(design_version_id) = args.design_version {
                start_work_with_options(
                    root,
                    WorkStart {
                        title: &args.title,
                        responsibility: args.responsibility.as_deref(),
                        design_version_id: Some(design_version_id),
                        implementation: args.implementation,
                    },
                )?
            } else {
                start_work_with_options(
                    root,
                    WorkStart {
                        title: &args.title,
                        responsibility: args.responsibility.as_deref(),
                        design_version_id: None,
                        implementation: args.implementation,
                    },
                )?
            };
            println!("started work unit");
            println!("work_unit_id: {}", outcome.work_unit_id);
            println!("activation_id: {}", outcome.activation_id);
        }
        WorkCommand::Activate(args) => {
            let outcome = activate_work(
                root,
                WorkActivate {
                    work_unit_id: args.work_unit_id,
                    design_version_id: args.design_version,
                    implementation: args.implementation,
                    reason: args.reason.as_deref(),
                },
            )?;
            println!("activated work unit");
            println!("work_unit_id: {}", outcome.work_unit_id);
            println!("activation_id: {}", outcome.activation_id);
        }
        WorkCommand::Block(args) => {
            let outcome = block_work(root, args.work_unit_id, &args.reason)?;
            println!("blocked work unit");
            println!("work_unit_id: {}", outcome.work_unit_id);
            if let Some(activation_id) = outcome.activation_id {
                println!("activation_id: {activation_id}");
            }
            println!("previous_status: {}", outcome.previous_status);
            println!("status: {}", outcome.status);
        }
        WorkCommand::Unblock(args) => {
            let outcome = unblock_work(root, args.work_unit_id, &args.reason)?;
            println!("unblocked work unit");
            println!("work_unit_id: {}", outcome.work_unit_id);
            if let Some(activation_id) = outcome.activation_id {
                println!("activation_id: {activation_id}");
            }
            println!("previous_status: {}", outcome.previous_status);
            println!("status: {}", outcome.status);
        }
        WorkCommand::Suspend(args) => {
            let outcome = suspend_work(root, &args.reason, &args.next)?;
            println!("suspended work unit");
            println!("work_unit_id: {}", outcome.work_unit_id);
            println!("activation_id: {}", outcome.activation_id);
            println!("suspend_snapshot_id: {}", outcome.suspend_snapshot_id);
        }
        WorkCommand::Interrupt(args) => {
            let outcome = interrupt_work(root, &args.title, &args.reason)?;
            println!("interrupted active work");
            println!("parent_work_unit_id: {}", outcome.parent_work_unit_id);
            println!("parent_activation_id: {}", outcome.parent_activation_id);
            println!(
                "parent_suspend_snapshot_id: {}",
                outcome.parent_suspend_snapshot_id
            );
            println!("child_work_unit_id: {}", outcome.child_work_unit_id);
            println!("child_activation_id: {}", outcome.child_activation_id);
        }
        WorkCommand::Resume(args) => {
            let outcome = resume_work(root, args.check)?;
            println!("resumed work unit");
            println!("work_unit_id: {}", outcome.work_unit_id);
            println!("activation_id: {}", outcome.activation_id);
        }
        WorkCommand::Close(args) => {
            let outcome = close_active_work(root, &args.summary, args.commit.as_deref())?;
            println!("closed work unit");
            println!("work_unit_id: {}", outcome.work_unit_id);
            println!("activation_id: {}", outcome.activation_id);
        }
        WorkCommand::Abandon(args) => {
            let outcome = abandon_work(root, args.work_unit_id, &args.reason)?;
            println!("abandoned work unit");
            println!("work_unit_id: {}", outcome.work_unit_id);
            if let Some(activation_id) = outcome.activation_id {
                println!("activation_id: {activation_id}");
            }
            println!("previous_status: {}", outcome.previous_status);
            println!("status: {}", outcome.status);
        }
        WorkCommand::Fork(args) => {
            let source_count = [
                args.from_record.is_some(),
                args.from_activation.is_some(),
                args.from_commit.is_some(),
                args.from_git_commit_id.is_some(),
                args.from_snapshot.is_some(),
            ]
            .into_iter()
            .filter(|selected| *selected)
            .count();
            if source_count != 1 {
                anyhow::bail!(
                    "exactly one of --from-record, --from-activation, --from-commit, --from-git-commit-id, or --from-snapshot is required"
                );
            }

            let source = match (
                args.from_record,
                args.from_activation,
                args.from_commit.as_deref(),
                args.from_git_commit_id,
                args.from_snapshot,
            ) {
                (Some(id), None, None, None, None) => WorkForkSource::Record(id),
                (None, Some(id), None, None, None) => WorkForkSource::Activation(id),
                (None, None, Some(sha), None, None) => WorkForkSource::Commit(sha),
                (None, None, None, Some(id), None) => WorkForkSource::GitCommit(id),
                (None, None, None, None, Some(id)) => WorkForkSource::RepositorySnapshot(id),
                _ => unreachable!("source count checked above"),
            };
            let outcome = fork_work(
                root,
                NewWorkFork {
                    title: &args.title,
                    source,
                    reason: &args.reason,
                    discard_policy: &args.discard_policy,
                },
            )?;
            println!("forked work");
            println!("fork_id: {}", outcome.fork_id);
            println!("work_unit_id: {}", outcome.work_unit_id);
            println!("activation_id: {}", outcome.activation_id);
        }
        WorkCommand::Reopen(args) => {
            let outcome = reopen_work(
                root,
                WorkReopen {
                    work_unit_id: args.work_unit_id,
                    reason: &args.reason,
                    reason_type: &args.reason_type,
                    authority_event_id: args.authority,
                    acceptance_record_id: args.acceptance,
                },
            )?;
            println!("reopened work unit");
            println!("work_unit_id: {}", outcome.work_unit_id);
            println!("activation_id: {}", outcome.activation_id);
        }
        WorkCommand::FollowUp(args) => {
            let outcome =
                create_follow_up_work(root, args.source_work_unit_id, &args.title, &args.reason)?;
            println!("created follow-up work unit");
            println!("source_work_unit_id: {}", outcome.source_work_unit_id);
            println!("work_unit_id: {}", outcome.work_unit_id);
            println!("activation_id: {}", outcome.activation_id);
        }
    }
    Ok(())
}

pub(crate) fn handle_resume_check(root: &Path, args: ResumeCheckArgs) -> Result<()> {
    let outcome = resume_check(root, &args.maturity)?;
    println!("resume_check_id: {}", outcome.resume_check_id);
    println!("result: {}", outcome.result);
    if let Some(reason) = outcome.blocking_reason {
        println!("blocking_reason: {reason}");
    }
    Ok(())
}
