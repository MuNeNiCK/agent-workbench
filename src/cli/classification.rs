use std::path::Path;

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum PublicationClass {
    Public,
    ProjectInternal,
}

impl PublicationClass {
    pub(super) const fn label(self) -> Option<&'static str> {
        match self {
            Self::Public => None,
            Self::ProjectInternal => Some("project-internal"),
        }
    }
}

use super::args::*;

pub(super) fn publication_class(command: &Command) -> PublicationClass {
    let internal = match command {
        Command::Doctor { .. }
        | Command::Migration { .. }
        | Command::ReviewContext(_)
        | Command::Design { .. }
        | Command::Acceptance { .. }
        | Command::Decompose { .. }
        | Command::Checklist { .. }
        | Command::Stale { .. }
        | Command::Export { .. }
        | Command::Trace { .. }
        | Command::Evidence { .. }
        | Command::Coverage { .. } => true,
        Command::Gate { command } => gate(command),
        Command::Correction {
            command: CorrectionCommand::List(_),
        }
        | Command::Rules { .. }
        | Command::Requirement { .. }
        | Command::DesignDecision { .. }
        | Command::GateTemplate { .. } => true,
        Command::Command { command } => memory(command),
        Command::WorkRecord { command } => record(command),
        Command::Repository { command } => repository(command),
        Command::Task {
            command: TaskCommand::List(_),
        } => true,
        Command::Phase { command } => phase(command),
        Command::Decision { command } => {
            matches!(
                command,
                DecisionCommand::List(_) | DecisionCommand::Search(_)
            )
        }
        Command::Review { command } => review(command),
        Command::Finding {
            command: FindingCommand::List(_),
        }
        | Command::Closure {
            command: ClosureCommand::Ready(_),
        }
        | Command::Authority {
            command: AuthorityCommand::List(_),
        } => true,
        Command::Kpt { command } => kpt(command),
        _ => false,
    };
    if internal {
        PublicationClass::ProjectInternal
    } else {
        PublicationClass::Public
    }
}

pub(super) fn classify_error(
    error: anyhow::Error,
    root: &Path,
    class: PublicationClass,
) -> anyhow::Error {
    if class == PublicationClass::ProjectInternal {
        return anyhow::anyhow!("classification: project-internal\n{error:#}");
    }
    let mut message = format!("{error:#}");
    if let Some(root) = root.to_str().filter(|root| !root.is_empty()) {
        message = message.replace(root, "<project>");
    }
    for (private, public) in [
        (".agent-workbench", "managed project state"),
        ("ledger.sqlite", "managed project state"),
        ("Ledger", "Project state"),
        ("ledger", "project state"),
        ("SQLite", "state store"),
        ("sqlite", "state store"),
    ] {
        message = message.replace(private, public);
    }
    anyhow::anyhow!(message)
}

fn gate(command: &GateCommand) -> bool {
    matches!(
        command,
        GateCommand::CloseReady(_)
            | GateCommand::ResumeReady(_)
            | GateCommand::DesignReady(_)
            | GateCommand::ImplementationReady(_)
            | GateCommand::Run {
                command: GateRunCommand::List(_)
            }
    )
}

fn memory(command: &MemoryCommand) -> bool {
    matches!(
        command,
        MemoryCommand::List(_)
            | MemoryCommand::Usage {
                command: CommandUsageCommand::List(_)
            }
    )
}

fn record(command: &WorkRecordCommand) -> bool {
    matches!(
        command,
        WorkRecordCommand::List(_) | WorkRecordCommand::Export(_)
    )
}

fn repository(command: &RepositoryCommand) -> bool {
    matches!(
        command,
        RepositoryCommand::List
            | RepositoryCommand::Snapshot {
                command: RepositorySnapshotCommand::List(_)
            }
    )
}

fn phase(command: &PhaseCommand) -> bool {
    match command {
        PhaseCommand::List(_)
        | PhaseCommand::Show(_)
        | PhaseCommand::Dependency {
            command: PhaseDependencyCommand::List(_),
        }
        | PhaseCommand::Trace {
            command: PhaseTraceCommand::List(_),
        }
        | PhaseCommand::Inventory(_)
        | PhaseCommand::CloseReady(_) => true,
        PhaseCommand::Rescope(args) => args.dry_run,
        PhaseCommand::Split(args) => args.dry_run,
        _ => false,
    }
}

fn review(command: &ReviewCommand) -> bool {
    matches!(
        command,
        ReviewCommand::Scope {
            command: ReviewScopeCommand::List
        } | ReviewCommand::Policy {
            command: ReviewPolicyCommand::List
        } | ReviewCommand::Plan {
            command: ReviewPlanCommand::List | ReviewPlanCommand::Context(_)
        } | ReviewCommand::Run {
            command: ReviewRunCommand::List(_)
        }
    )
}

fn kpt(command: &KptCommand) -> bool {
    matches!(
        command,
        KptCommand::List(_)
            | KptCommand::Item {
                command: KptItemCommand::List(_)
            }
    )
}
