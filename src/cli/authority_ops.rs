use std::path::Path;

use anyhow::Result;

use super::args::*;
use agent_workbench::*;

pub(crate) fn handle_authority(root: &Path, command: AuthorityCommand) -> Result<()> {
    match command {
        AuthorityCommand::Add(args) => {
            let outcome = match (
                args.path.as_deref(),
                args.authority_type.as_deref(),
                args.instruction.as_deref(),
                args.source.as_deref(),
            ) {
                (Some(path), Some(authority_type), None, None) => {
                    let event_type = match authority_type {
                        "design" => "design_doc",
                        other => other,
                    };
                    let summary = args.summary.unwrap_or_else(|| {
                        format!("registered {authority_type} authority at {path}")
                    });
                    add_authority_event(
                        root,
                        NewAuthorityEvent {
                            event_type,
                            source: Some(path),
                            summary: &summary,
                            scope: args.scope.as_deref(),
                            precedence: args.precedence.unwrap_or(90),
                        },
                    )?
                }
                (None, None, Some(instruction), Some(source)) => add_authority_event(
                    root,
                    NewAuthorityEvent {
                        event_type: "user_instruction",
                        source: Some(source),
                        summary: instruction,
                        scope: Some("project"),
                        precedence: 100,
                    },
                )?,
                _ => anyhow::bail!(
                    "authority add requires exactly --path with --type or --instruction with --source"
                ),
            };
            println!("added authority");
            println!("authority_id: {}", outcome.authority_id);
            println!("authority_event_id: {}", outcome.authority_event_id);
        }
        AuthorityCommand::Event { command } => match command {
            AuthorityEventCommand::Add(args) => {
                let outcome = add_authority_event(
                    root,
                    NewAuthorityEvent {
                        event_type: &args.event_type,
                        source: args.source.as_deref(),
                        summary: &args.summary,
                        scope: args.scope.as_deref(),
                        precedence: args.precedence,
                    },
                )?;
                println!("added authority event");
                println!("authority_id: {}", outcome.authority_id);
                println!("authority_event_id: {}", outcome.authority_event_id);
            }
        },
        AuthorityCommand::List(args) => {
            let records = list_authorities(root, args.scope.as_deref())?;
            if records.is_empty() {
                println!("no authorities");
            }
            for record in records {
                let scope = record.scope.as_deref().unwrap_or("-");
                println!(
                    "{} [{} scope={} precedence={}] {}",
                    record.id,
                    record.authority_type,
                    scope,
                    record.precedence,
                    record.path_or_label
                );
            }
        }
    }
    Ok(())
}
