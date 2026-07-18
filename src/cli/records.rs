use std::fs;
use std::path::Path;

use anyhow::{Result, bail};

use super::args::*;
use agent_workbench::*;

pub(crate) fn handle_work_record(root: &Path, command: WorkRecordCommand) -> Result<()> {
    match command {
        WorkRecordCommand::Create(args) => {
            let outcome = create_work_record(
                root,
                NewWorkRecord {
                    work_unit_id: args.work_unit,
                    topic: &args.topic,
                    work_performed: args.work_performed.as_deref(),
                    next_actions: args.next_actions.as_deref(),
                    notable_operations: args.notable_operations.as_deref(),
                    export_path: args.export_path.as_deref(),
                },
            )?;
            println!("created work record");
            println!("work_record_id: {}", outcome.work_record_id);
            if let Some(work_unit_id) = outcome.work_unit_id {
                println!("work_unit_id: {work_unit_id}");
            }
        }
        WorkRecordCommand::List(args) => {
            let records = list_work_records(root, args.work_unit)?;
            if records.is_empty() {
                println!("no work records");
            }
            for record in records {
                let work_unit = record
                    .work_unit_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "-".to_string());
                println!("{} [work_unit={}] {}", record.id, work_unit, record.topic);
            }
        }
        WorkRecordCommand::Export(args) => {
            if args.format != "md" {
                anyhow::bail!("only --format md is implemented");
            }
            let markdown = export_work_record_markdown(root, args.work_record_id)?;
            if let Some(parent) = args.output.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&args.output, markdown)?;
            println!("exported work record");
            println!("path: {}", args.output.display());
        }
        WorkRecordCommand::Command { command } => match command {
            WorkRecordCommandLinkCommand::Add(args) => {
                let outcome = add_work_record_command(
                    root,
                    NewWorkRecordCommand {
                        work_record_id: resolve_work_record_arg(
                            args.work_record_id,
                            args.record_id,
                        )?,
                        command_usage_id: args.usage,
                        command_profile_id: args.profile,
                        command: args.command.as_deref(),
                        result: args.result.as_deref(),
                        log_path: args.log_path.as_deref(),
                        note: args.note.as_deref(),
                    },
                )?;
                println!("linked work record command");
                println!("work_record_command_id: {}", outcome.link_id);
            }
        },
        WorkRecordCommand::Commit { command } => match command {
            WorkRecordCommitCommand::Add(args) => {
                let outcome = match args.git_commit {
                    Some(git_commit_id) => add_work_record_git_commit(
                        root,
                        NewWorkRecordGitCommit {
                            work_record_id: resolve_work_record_arg(
                                args.work_record_id,
                                args.record_id,
                            )?,
                            git_commit_id: Some(git_commit_id),
                            commit_sha: &args.sha,
                            role: &args.role,
                            note: args.note.as_deref(),
                        },
                    )?,
                    None => add_work_record_commit(
                        root,
                        NewWorkRecordCommit {
                            work_record_id: resolve_work_record_arg(
                                args.work_record_id,
                                args.record_id,
                            )?,
                            commit_sha: &args.sha,
                            role: &args.role,
                            note: args.note.as_deref(),
                        },
                    )?,
                };
                println!("linked work record commit");
                println!("work_record_commit_id: {}", outcome.link_id);
            }
        },
        WorkRecordCommand::File { command } => match command {
            WorkRecordFileCommand::Add(args) => {
                link_work_record_file(root, args)?;
            }
        },
        WorkRecordCommand::Link { command } => match command {
            WorkRecordLinkCommand::Command(args) => {
                let outcome = add_work_record_command(
                    root,
                    NewWorkRecordCommand {
                        work_record_id: resolve_work_record_arg(
                            args.work_record_id,
                            args.record_id,
                        )?,
                        command_usage_id: args.usage,
                        command_profile_id: args.profile,
                        command: args.command.as_deref(),
                        result: args.result.as_deref(),
                        log_path: args.log_path.as_deref(),
                        note: args.note.as_deref(),
                    },
                )?;
                println!("linked work record command");
                println!("work_record_command_id: {}", outcome.link_id);
            }
            WorkRecordLinkCommand::Commit(args) => {
                let outcome = match args.git_commit {
                    Some(git_commit_id) => add_work_record_git_commit(
                        root,
                        NewWorkRecordGitCommit {
                            work_record_id: resolve_work_record_arg(
                                args.work_record_id,
                                args.record_id,
                            )?,
                            git_commit_id: Some(git_commit_id),
                            commit_sha: &args.sha,
                            role: &args.role,
                            note: args.note.as_deref(),
                        },
                    )?,
                    None => add_work_record_commit(
                        root,
                        NewWorkRecordCommit {
                            work_record_id: resolve_work_record_arg(
                                args.work_record_id,
                                args.record_id,
                            )?,
                            commit_sha: &args.sha,
                            role: &args.role,
                            note: args.note.as_deref(),
                        },
                    )?,
                };
                println!("linked work record commit");
                println!("work_record_commit_id: {}", outcome.link_id);
            }
            WorkRecordLinkCommand::File(args) => {
                link_work_record_file(root, args)?;
            }
        },
    }
    Ok(())
}

fn link_work_record_file(root: &Path, args: WorkRecordFileAddArgs) -> Result<()> {
    let work_record_id = resolve_work_record_arg(args.work_record_id, args.record_id)?;
    let outcome = if args.git_file_change.is_some() || args.repository_id.is_some() {
        add_work_record_git_file(
            root,
            NewWorkRecordGitFile {
                work_record_id,
                git_file_change_id: args.git_file_change,
                repository_id: args.repository_id,
                path: &args.path,
                role: &args.role,
                note: args.note.as_deref(),
            },
        )?
    } else {
        add_work_record_file(
            root,
            NewWorkRecordFile {
                work_record_id,
                path: &args.path,
                role: &args.role,
                note: args.note.as_deref(),
            },
        )?
    };
    println!("linked work record file");
    println!("work_record_file_id: {}", outcome.link_id);
    Ok(())
}

fn resolve_work_record_arg(positional: Option<i64>, flagged: Option<i64>) -> Result<i64> {
    match (positional, flagged) {
        (Some(id), None) | (None, Some(id)) => Ok(id),
        (Some(_), Some(_)) => {
            anyhow::bail!("provide work record id either positionally or with --record, not both")
        }
        (None, None) => anyhow::bail!("work record id is required"),
    }
}

pub(crate) fn handle_repository(root: &Path, command: RepositoryCommand) -> Result<()> {
    match command {
        RepositoryCommand::Add(args) => {
            let outcome = add_repository(
                root,
                NewRepository {
                    name: &args.name,
                    path: &args.path,
                    current_head: args.head.as_deref(),
                    status_summary: args.status.as_deref(),
                },
            )?;
            println!("added repository");
            println!("repository_id: {}", outcome.repository_id);
        }
        RepositoryCommand::List => {
            let records = list_repositories(root)?;
            if records.is_empty() {
                println!("no repositories");
            }
            for record in records {
                let head = record.current_head.as_deref().unwrap_or("-");
                let status = record.status_summary.as_deref().unwrap_or("-");
                println!(
                    "{} [{} head={}] {}",
                    record.id, record.name, head, record.path
                );
                println!("status: {status}");
            }
        }
        RepositoryCommand::Snapshot { command } => match command {
            RepositorySnapshotCommand::Add(args) => {
                let outcome = add_repository_snapshot(
                    root,
                    NewRepositorySnapshot {
                        repository: &args.repository,
                        work_unit_activation_id: args.activation,
                        head_sha: args.head.as_deref(),
                        branch: args.branch.as_deref(),
                        status_summary: args.status.as_deref(),
                        is_clean: args.clean,
                    },
                )?;
                println!("added repository snapshot");
                println!("repository_snapshot_id: {}", outcome.repository_snapshot_id);
                println!("repository_id: {}", outcome.repository_id);
            }
            RepositorySnapshotCommand::List(args) => {
                let records = list_repository_snapshots(root, args.repository.as_deref())?;
                if records.is_empty() {
                    println!("no repository snapshots");
                }
                for record in records {
                    let head = record.head_sha.as_deref().unwrap_or("-");
                    let branch = record.branch.as_deref().unwrap_or("-");
                    let status = record.status_summary.as_deref().unwrap_or("-");
                    println!(
                        "{} [repository={} clean={} branch={} head={}] {}",
                        record.id, record.repository_name, record.is_clean, branch, head, status
                    );
                }
            }
        },
        RepositoryCommand::Dirty { command } => match command {
            RepositoryDirtyCommand::Add(args) => {
                let outcome = add_repository_dirty_entry(
                    root,
                    NewRepositoryDirtyEntry {
                        repository_snapshot_id: args.snapshot,
                        path: &args.path,
                        change_type: &args.change_type,
                        staged: args.staged,
                        content_hash: args.hash.as_deref(),
                    },
                )?;
                println!("added repository dirty entry");
                println!(
                    "repository_dirty_entry_id: {}",
                    outcome.repository_dirty_entry_id
                );
            }
        },
        RepositoryCommand::Classify { command } => match command {
            RepositoryClassifyCommand::Add(args) => {
                let outcome = add_repository_state_classification(
                    root,
                    NewRepositoryStateClassification {
                        repository_snapshot_id: args.snapshot,
                        dirty_entry_id: args.dirty_entry,
                        classification: &args.classification,
                        reason: &args.reason,
                        acceptance_record_id: args.acceptance,
                    },
                )?;
                println!("classified repository state");
                println!(
                    "repository_state_classification_id: {}",
                    outcome.repository_state_classification_id
                );
            }
        },
        RepositoryCommand::Commit { command } => match command {
            RepositoryCommitCommand::Add(args) => {
                let outcome = add_git_commit(
                    root,
                    NewGitCommit {
                        repository: &args.repository,
                        commit_sha: &args.sha,
                        short_sha: args.short.as_deref(),
                        subject: args.subject.as_deref(),
                        author_name: args.author_name.as_deref(),
                        author_email: args.author_email.as_deref(),
                        committed_at: args.committed_at.as_deref(),
                        parent_shas: args.parents.as_deref(),
                    },
                )?;
                println!("added git commit");
                println!("git_commit_id: {}", outcome.git_commit_id);
                println!("repository_id: {}", outcome.repository_id);
            }
        },
        RepositoryCommand::File { command } => match command {
            RepositoryFileCommand::Add(args) => {
                let outcome = add_git_file_change(
                    root,
                    NewGitFileChange {
                        git_commit_id: args.commit,
                        repository: args.repository.as_deref(),
                        path: &args.path,
                        old_path: args.old_path.as_deref(),
                        change_type: &args.change_type,
                        additions: args.additions,
                        deletions: args.deletions,
                        content_hash: args.hash.as_deref(),
                    },
                )?;
                println!("added git file change");
                println!("git_file_change_id: {}", outcome.git_file_change_id);
                println!("repository_id: {}", outcome.repository_id);
            }
        },
        RepositoryCommand::Compare { command } => match command {
            RepositoryCompareCommand::Add(args) => {
                let outcome = add_repository_snapshot_comparison(
                    root,
                    NewRepositorySnapshotComparison {
                        base_repository_snapshot_id: args.base,
                        current_repository_snapshot_id: args.current,
                        comparison_type: &args.comparison_type,
                        head_changed: args.head_changed,
                        dirty_state_changed: args.dirty_changed,
                        nested_repository_changed: args.nested_changed,
                        result: &args.result,
                    },
                )?;
                println!("added repository snapshot comparison");
                println!(
                    "repository_snapshot_comparison_id: {}",
                    outcome.repository_snapshot_comparison_id
                );
            }
        },
    }
    Ok(())
}

pub(crate) fn handle_git(root: &Path, command: GitCommand) -> Result<()> {
    match command {
        GitCommand::Commit { command } => match command {
            GitCommitCommand::Add(args) => {
                let commit_sha =
                    resolve_git_commit_sha(args.sha_arg.as_deref(), args.sha.as_deref())?;
                let outcome = add_git_commit(
                    root,
                    NewGitCommit {
                        repository: &args.repository,
                        commit_sha: &commit_sha,
                        short_sha: args.short.as_deref(),
                        subject: args.subject.as_deref(),
                        author_name: args.author_name.as_deref(),
                        author_email: args.author_email.as_deref(),
                        committed_at: args.committed_at.as_deref(),
                        parent_shas: args.parents.as_deref(),
                    },
                )?;
                println!("added git commit");
                println!("git_commit_id: {}", outcome.git_commit_id);
                println!("repository_id: {}", outcome.repository_id);
            }
        },
        GitCommand::Files { command } => match command {
            GitFileCommand::Add(args) => {
                let git_commit_id = resolve_git_commit_id(root, &args.commit)?;
                let outcome = add_git_file_change(
                    root,
                    NewGitFileChange {
                        git_commit_id,
                        repository: args.repository.as_deref(),
                        path: &args.path,
                        old_path: args.old_path.as_deref(),
                        change_type: &args.change_type,
                        additions: args.additions,
                        deletions: args.deletions,
                        content_hash: args.hash.as_deref(),
                    },
                )?;
                println!("added git file change");
                println!("git_file_change_id: {}", outcome.git_file_change_id);
                println!("repository_id: {}", outcome.repository_id);
            }
        },
    }
    Ok(())
}

fn resolve_git_commit_sha(positional: Option<&str>, flagged: Option<&str>) -> Result<String> {
    match (positional, flagged) {
        (Some(value), None) | (None, Some(value)) => Ok(value.to_string()),
        (Some(_), Some(_)) => {
            bail!("provide commit sha either positionally or with --sha, not both")
        }
        (None, None) => bail!("commit sha is required"),
    }
}
