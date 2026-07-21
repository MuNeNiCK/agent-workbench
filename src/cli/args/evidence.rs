use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub(crate) enum CorrectionCommand {
    Add(CorrectionAddArgs),
    List(CorrectionListArgs),
}

#[derive(Debug, Args)]
pub(crate) struct CorrectionAddArgs {
    #[arg(long)]
    pub(crate) scope: String,
    #[arg(long = "type", requires_all = ["pattern", "correction"], conflicts_with_all = ["source", "expected_change"])]
    pub(crate) correction_type: Option<String>,
    #[arg(long, requires = "correction_type", conflicts_with = "source")]
    pub(crate) pattern: Option<String>,
    #[arg(long, requires = "correction_type", conflicts_with = "expected_change")]
    pub(crate) correction: Option<String>,
    #[arg(long, requires = "expected_change", conflicts_with_all = ["correction_type", "pattern", "correction", "applies_to"])]
    pub(crate) source: Option<String>,
    #[arg(long, requires = "source", conflicts_with = "correction")]
    pub(crate) expected_change: Option<String>,
    #[arg(long, conflicts_with = "source")]
    pub(crate) applies_to: Option<String>,
    #[arg(long)]
    pub(crate) severity: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct CorrectionListArgs {
    #[arg(long)]
    pub(crate) scope: Option<String>,
    #[arg(long, value_parser = ["active", "superseded", "retired"])]
    pub(crate) status: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum MemoryCommand {
    Fixed {
        #[command(subcommand)]
        command: FixedCommand,
    },
    Prefer(CommandPreferArgs),
    Deprecate(CommandDeprecateArgs),
    Usage {
        #[command(subcommand)]
        command: CommandUsageCommand,
    },
    Deviation {
        #[command(subcommand)]
        command: CommandDeviationCommand,
    },
    List(CommandListArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum FixedCommand {
    Add(CommandFixedAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct CommandFixedAddArgs {
    #[arg(long)]
    pub(crate) name: String,
    #[arg(long = "type")]
    pub(crate) command_type: String,
    #[arg(long)]
    pub(crate) scope: String,
    #[arg(long)]
    pub(crate) command: String,
    #[arg(long)]
    pub(crate) timeout: Option<String>,
    #[arg(long)]
    pub(crate) expected_result: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct CommandPreferArgs {
    #[arg(long)]
    pub(crate) name: String,
    #[arg(long = "type", default_value = "validation")]
    pub(crate) command_type: String,
    #[arg(long, default_value = "project")]
    pub(crate) scope: String,
    #[arg(long)]
    pub(crate) command: String,
    #[arg(long)]
    pub(crate) timeout: Option<String>,
    #[arg(long)]
    pub(crate) expected_result: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct CommandDeprecateArgs {
    #[arg(long)]
    pub(crate) name: String,
    #[arg(long)]
    pub(crate) reason: String,
}

#[derive(Debug, Args)]
pub(crate) struct CommandListArgs {
    #[arg(long = "type")]
    pub(crate) command_type: Option<String>,
    #[arg(long)]
    pub(crate) scope: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum CommandUsageCommand {
    Add(CommandUsageAddArgs),
    List(CommandUsageListArgs),
    Promote(CommandUsagePromoteArgs),
}

#[derive(Debug, Args)]
pub(crate) struct CommandUsageAddArgs {
    #[arg(long)]
    pub(crate) profile: Option<String>,
    #[arg(long)]
    pub(crate) command: Option<String>,
    #[arg(long, default_value = "unknown")]
    pub(crate) result: String,
    #[arg(long)]
    pub(crate) log: Option<String>,
    #[arg(long)]
    pub(crate) work_unit: Option<i64>,
    #[arg(long)]
    pub(crate) snapshot: Option<i64>,
}

#[derive(Debug, Args)]
pub(crate) struct CommandUsageListArgs {
    #[arg(long)]
    pub(crate) profile: Option<String>,
    #[arg(long)]
    pub(crate) work_unit: Option<i64>,
}

#[derive(Debug, Args)]
pub(crate) struct CommandUsagePromoteArgs {
    pub(crate) usage_id: i64,
    #[arg(long)]
    pub(crate) name: String,
    #[arg(long = "type", default_value = "validation")]
    pub(crate) command_type: String,
    #[arg(long, default_value = "project")]
    pub(crate) scope: String,
    #[arg(long, default_value = "preferred")]
    pub(crate) status: String,
    #[arg(long)]
    pub(crate) timeout: Option<String>,
    #[arg(long)]
    pub(crate) expected_result: Option<String>,
    #[arg(long)]
    pub(crate) authority: Option<i64>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum CommandDeviationCommand {
    Add(CommandDeviationAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct CommandDeviationAddArgs {
    #[arg(long)]
    pub(crate) profile: String,
    #[arg(long)]
    pub(crate) usage: Option<i64>,
    #[arg(long)]
    pub(crate) reason: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum RulesCommand {
    Applicable(RulesApplicableArgs),
}

#[derive(Debug, Args)]
pub(crate) struct RulesApplicableArgs {
    #[arg(long)]
    pub(crate) scope: Option<String>,
    #[arg(long)]
    pub(crate) work_unit: Option<i64>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum WorkRecordCommand {
    Create(WorkRecordCreateArgs),
    List(WorkRecordListArgs),
    Export(WorkRecordExportArgs),
    Command {
        #[command(subcommand)]
        command: WorkRecordCommandLinkCommand,
    },
    Commit {
        #[command(subcommand)]
        command: WorkRecordCommitCommand,
    },
    File {
        #[command(subcommand)]
        command: WorkRecordFileCommand,
    },
    Link {
        #[command(subcommand)]
        command: WorkRecordLinkCommand,
    },
}

#[derive(Debug, Args)]
pub(crate) struct WorkRecordCreateArgs {
    #[arg(long)]
    pub(crate) topic: String,
    #[arg(long)]
    pub(crate) work_performed: Option<String>,
    #[arg(long)]
    pub(crate) next_actions: Option<String>,
    #[arg(long)]
    pub(crate) notable_operations: Option<String>,
    #[arg(long)]
    pub(crate) work_unit: Option<i64>,
    #[arg(long, alias = "export-md")]
    pub(crate) export_path: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct WorkRecordListArgs {
    #[arg(long)]
    pub(crate) work_unit: Option<i64>,
}

#[derive(Debug, Args)]
pub(crate) struct WorkRecordExportArgs {
    pub(crate) work_record_id: i64,
    #[arg(long, default_value = "md")]
    pub(crate) format: String,
    #[arg(long)]
    pub(crate) output: PathBuf,
}

#[derive(Debug, Subcommand)]
pub(crate) enum WorkRecordCommandLinkCommand {
    Add(WorkRecordCommandAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct WorkRecordCommandAddArgs {
    pub(crate) work_record_id: Option<i64>,
    #[arg(long = "record")]
    pub(crate) record_id: Option<i64>,
    #[arg(long)]
    pub(crate) usage: Option<i64>,
    #[arg(long)]
    pub(crate) command: Option<String>,
    #[arg(long)]
    pub(crate) result: Option<String>,
    #[arg(long)]
    pub(crate) profile: Option<i64>,
    #[arg(long)]
    pub(crate) log_path: Option<String>,
    #[arg(long)]
    pub(crate) note: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum WorkRecordCommitCommand {
    Add(WorkRecordCommitAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct WorkRecordCommitAddArgs {
    pub(crate) work_record_id: Option<i64>,
    #[arg(long = "record")]
    pub(crate) record_id: Option<i64>,
    #[arg(long)]
    pub(crate) git_commit: Option<i64>,
    #[arg(long, alias = "commit")]
    pub(crate) sha: String,
    #[arg(long, default_value = "referenced")]
    pub(crate) role: String,
    #[arg(long)]
    pub(crate) note: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum WorkRecordFileCommand {
    Add(WorkRecordFileAddArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum WorkRecordLinkCommand {
    Command(WorkRecordCommandAddArgs),
    Commit(WorkRecordCommitAddArgs),
    File(WorkRecordFileAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct WorkRecordFileAddArgs {
    pub(crate) work_record_id: Option<i64>,
    #[arg(long = "record")]
    pub(crate) record_id: Option<i64>,
    #[arg(long)]
    pub(crate) git_file_change: Option<i64>,
    #[arg(long)]
    pub(crate) repository_id: Option<i64>,
    #[arg(long)]
    pub(crate) path: String,
    #[arg(long, default_value = "changed")]
    pub(crate) role: String,
    #[arg(long)]
    pub(crate) note: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum RepositoryCommand {
    Add(RepositoryAddArgs),
    List,
    Snapshot {
        #[command(subcommand)]
        command: RepositorySnapshotCommand,
    },
    Dirty {
        #[command(subcommand)]
        command: RepositoryDirtyCommand,
    },
    Classify {
        #[command(subcommand)]
        command: RepositoryClassifyCommand,
    },
    Commit {
        #[command(subcommand)]
        command: RepositoryCommitCommand,
    },
    File {
        #[command(subcommand)]
        command: RepositoryFileCommand,
    },
    Compare {
        #[command(subcommand)]
        command: RepositoryCompareCommand,
    },
}

#[derive(Debug, Args)]
pub(crate) struct RepositoryAddArgs {
    pub(crate) name: String,
    #[arg(long)]
    pub(crate) path: String,
    #[arg(long)]
    pub(crate) head: Option<String>,
    #[arg(long)]
    pub(crate) status: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum RepositorySnapshotCommand {
    Add(RepositorySnapshotAddArgs),
    List(RepositorySnapshotListArgs),
}

#[derive(Debug, Args)]
pub(crate) struct RepositorySnapshotAddArgs {
    #[arg(long)]
    pub(crate) repository: String,
    #[arg(long)]
    pub(crate) activation: Option<i64>,
    #[arg(long)]
    pub(crate) head: Option<String>,
    #[arg(long)]
    pub(crate) branch: Option<String>,
    #[arg(long)]
    pub(crate) status: Option<String>,
    #[arg(long)]
    pub(crate) clean: bool,
}

#[derive(Debug, Args)]
pub(crate) struct RepositorySnapshotListArgs {
    #[arg(long)]
    pub(crate) repository: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum RepositoryDirtyCommand {
    Add(RepositoryDirtyAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct RepositoryDirtyAddArgs {
    #[arg(long)]
    pub(crate) snapshot: i64,
    #[arg(long)]
    pub(crate) path: String,
    #[arg(long = "type")]
    pub(crate) change_type: String,
    #[arg(long)]
    pub(crate) staged: bool,
    #[arg(long)]
    pub(crate) hash: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum RepositoryClassifyCommand {
    Add(RepositoryClassifyAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct RepositoryClassifyAddArgs {
    #[arg(long)]
    pub(crate) snapshot: i64,
    #[arg(long)]
    pub(crate) dirty_entry: Option<i64>,
    #[arg(long)]
    pub(crate) classification: String,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) acceptance: Option<i64>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum RepositoryCommitCommand {
    Add(RepositoryCommitAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct RepositoryCommitAddArgs {
    #[arg(long)]
    pub(crate) repository: String,
    #[arg(long, alias = "commit")]
    pub(crate) sha: String,
    #[arg(long)]
    pub(crate) short: Option<String>,
    #[arg(long)]
    pub(crate) subject: Option<String>,
    #[arg(long)]
    pub(crate) author_name: Option<String>,
    #[arg(long)]
    pub(crate) author_email: Option<String>,
    #[arg(long)]
    pub(crate) committed_at: Option<String>,
    #[arg(long)]
    pub(crate) parents: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum RepositoryFileCommand {
    Add(RepositoryFileAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct RepositoryFileAddArgs {
    #[arg(long)]
    pub(crate) commit: i64,
    #[arg(long)]
    pub(crate) repository: Option<String>,
    #[arg(long)]
    pub(crate) path: String,
    #[arg(long)]
    pub(crate) old_path: Option<String>,
    #[arg(long = "type")]
    pub(crate) change_type: String,
    #[arg(long)]
    pub(crate) additions: Option<i64>,
    #[arg(long)]
    pub(crate) deletions: Option<i64>,
    #[arg(long)]
    pub(crate) hash: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum RepositoryCompareCommand {
    Add(RepositoryCompareAddArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum GitCommand {
    Commit {
        #[command(subcommand)]
        command: GitCommitCommand,
    },
    Files {
        #[command(subcommand)]
        command: GitFileCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum GitCommitCommand {
    Add(GitCommitAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct GitCommitAddArgs {
    pub(crate) sha_arg: Option<String>,
    #[arg(long, alias = "repo")]
    pub(crate) repository: String,
    #[arg(long, visible_alias = "commit")]
    pub(crate) sha: Option<String>,
    #[arg(long, conflicts_with_all = ["short", "subject", "author_name", "author_email", "committed_at", "parents"])]
    pub(crate) note: Option<String>,
    #[arg(long)]
    pub(crate) short: Option<String>,
    #[arg(long)]
    pub(crate) subject: Option<String>,
    #[arg(long)]
    pub(crate) author_name: Option<String>,
    #[arg(long)]
    pub(crate) author_email: Option<String>,
    #[arg(long)]
    pub(crate) committed_at: Option<String>,
    #[arg(long)]
    pub(crate) parents: Option<String>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum GitFileCommand {
    Add(GitFileAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct GitFileAddArgs {
    #[arg(long, requires = "change_type", conflicts_with = "change")]
    pub(crate) commit: Option<String>,
    #[arg(long)]
    pub(crate) repository: Option<String>,
    #[arg(long)]
    pub(crate) path: String,
    #[arg(long)]
    pub(crate) old_path: Option<String>,
    #[arg(long = "type", requires = "commit", conflicts_with = "change")]
    pub(crate) change_type: Option<String>,
    #[arg(long, requires = "repository", conflicts_with_all = ["commit", "change_type", "old_path", "additions", "deletions", "hash"])]
    pub(crate) change: Option<String>,
    #[arg(long)]
    pub(crate) additions: Option<i64>,
    #[arg(long)]
    pub(crate) deletions: Option<i64>,
    #[arg(long)]
    pub(crate) hash: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct RepositoryCompareAddArgs {
    #[arg(long)]
    pub(crate) base: i64,
    #[arg(long)]
    pub(crate) current: i64,
    #[arg(long = "type")]
    pub(crate) comparison_type: String,
    #[arg(long)]
    pub(crate) head_changed: bool,
    #[arg(long)]
    pub(crate) dirty_changed: bool,
    #[arg(long)]
    pub(crate) nested_changed: bool,
    #[arg(long)]
    pub(crate) result: String,
}
