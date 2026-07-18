use std::path::Path;

use anyhow::Result;

use super::args::*;
use agent_workbench::*;

pub(crate) fn handle_correction(root: &Path, command: CorrectionCommand) -> Result<()> {
    match command {
        CorrectionCommand::Add(args) => {
            let outcome = add_user_correction(
                root,
                NewUserCorrection {
                    scope: &args.scope,
                    correction_type: &args.correction_type,
                    mistake_pattern: &args.pattern,
                    correction: &args.correction,
                    applies_to: &args.applies_to,
                    severity: &args.severity,
                },
            )?;
            println!("added correction");
            println!("user_correction_id: {}", outcome.user_correction_id);
        }
        CorrectionCommand::List(args) => {
            let records = list_user_corrections(root, args.scope.as_deref())?;
            if records.is_empty() {
                println!("no corrections");
            }
            for record in records {
                println!(
                    "{} [{}:{}] {} -> {}",
                    record.id,
                    record.scope,
                    record.severity,
                    record.mistake_pattern,
                    record.correction
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn handle_command(root: &Path, command: MemoryCommand) -> Result<()> {
    match command {
        MemoryCommand::Fixed { command } => match command {
            FixedCommand::Add(args) => {
                let outcome = add_fixed_command(
                    root,
                    NewCommandProfile {
                        name: &args.name,
                        command_type: &args.command_type,
                        scope: &args.scope,
                        command: &args.command,
                        timeout: args.timeout.as_deref(),
                        expected_result: args.expected_result.as_deref(),
                    },
                )?;
                println!("added fixed command");
                println!("command_profile_id: {}", outcome.command_profile_id);
            }
        },
        MemoryCommand::Prefer(args) => {
            let outcome = add_preferred_command(
                root,
                NewCommandProfile {
                    name: &args.name,
                    command_type: &args.command_type,
                    scope: &args.scope,
                    command: &args.command,
                    timeout: args.timeout.as_deref(),
                    expected_result: args.expected_result.as_deref(),
                },
            )?;
            println!("added preferred command");
            println!("command_profile_id: {}", outcome.command_profile_id);
        }
        MemoryCommand::Deprecate(args) => {
            let outcome = deprecate_command_profile(root, &args.name, &args.reason)?;
            println!("deprecated command");
            println!("command_profile_id: {}", outcome.command_profile_id);
        }
        MemoryCommand::Usage { command } => match command {
            CommandUsageCommand::Add(args) => {
                let outcome = match args.snapshot {
                    Some(snapshot_id) => add_command_usage_with_repository_snapshot(
                        root,
                        NewCommandUsageWithRepositorySnapshot {
                            profile: args.profile.as_deref(),
                            command: args.command.as_deref(),
                            result: &args.result,
                            log_path: args.log.as_deref(),
                            work_unit_id: args.work_unit,
                            repository_snapshot_id: Some(snapshot_id),
                        },
                    )?,
                    None => add_command_usage(
                        root,
                        NewCommandUsage {
                            profile: args.profile.as_deref(),
                            command: args.command.as_deref(),
                            result: &args.result,
                            log_path: args.log.as_deref(),
                            work_unit_id: args.work_unit,
                        },
                    )?,
                };
                println!("recorded command usage");
                println!("command_usage_id: {}", outcome.command_usage_id);
                if let Some(command_profile_id) = outcome.command_profile_id {
                    println!("command_profile_id: {command_profile_id}");
                }
                if let Some(work_unit_id) = outcome.work_unit_id {
                    println!("work_unit_id: {work_unit_id}");
                }
            }
            CommandUsageCommand::List(args) => {
                let records = list_command_usages(
                    root,
                    CommandUsageListQuery {
                        profile: args.profile.as_deref(),
                        work_unit_id: args.work_unit,
                    },
                )?;
                if records.is_empty() {
                    println!("no command usages");
                }
                for record in records {
                    let profile = record
                        .command_profile_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    let work_unit = record
                        .work_unit_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    println!(
                        "{} [profile={} work_unit={} {}] {}",
                        record.id, profile, work_unit, record.result, record.command
                    );
                }
            }
            CommandUsageCommand::Promote(args) => {
                let outcome = promote_command_usage(
                    root,
                    NewCommandPromotion {
                        command_usage_id: args.usage_id,
                        name: &args.name,
                        command_type: &args.command_type,
                        scope: &args.scope,
                        status: &args.status,
                        timeout: args.timeout.as_deref(),
                        expected_result: args.expected_result.as_deref(),
                        authority_event_id: args.authority,
                    },
                )?;
                println!("promoted command usage");
                println!("command_profile_id: {}", outcome.command_profile_id);
                println!("status: {}", args.status);
            }
        },
        MemoryCommand::Deviation { command } => match command {
            CommandDeviationCommand::Add(args) => {
                let outcome = add_command_deviation(
                    root,
                    NewCommandDeviation {
                        profile: &args.profile,
                        command_usage_id: args.usage,
                        reason: &args.reason,
                    },
                )?;
                println!("recorded command deviation");
                println!("command_deviation_id: {}", outcome.command_deviation_id);
                println!("command_profile_id: {}", outcome.command_profile_id);
                if let Some(work_unit_id) = outcome.work_unit_id {
                    println!("work_unit_id: {work_unit_id}");
                }
            }
        },
        MemoryCommand::List(args) => {
            let records = list_command_profiles(root, args.command_type.as_deref())?;
            if records.is_empty() {
                println!("no command profiles");
            }
            for record in records {
                println!(
                    "{} [{}:{}] {} = {}",
                    record.id, record.command_type, record.status, record.name, record.command
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn handle_rules(root: &Path, command: RulesCommand) -> Result<()> {
    match command {
        RulesCommand::Applicable(args) => {
            let records = applicable_rules(
                root,
                RuleQuery {
                    scope_key: args.scope.as_deref(),
                    work_unit_id: args.work_unit,
                },
            )?;
            if records.is_empty() {
                println!("no applicable rules");
            }
            for record in records {
                let shadowed = record
                    .shadowed_by_rule_id
                    .map(|id| format!(" shadowed_by={id}"))
                    .unwrap_or_default();
                let authority = record
                    .authority_event_id
                    .map(|id| format!(" authority_event_id={id}"))
                    .unwrap_or_default();
                let scope_key = record
                    .scope_key
                    .as_ref()
                    .map(|key| format!(" scope_key={key}"))
                    .unwrap_or_default();
                let user_correction = record
                    .user_correction_id
                    .map(|id| format!(" user_correction_id={id}"))
                    .unwrap_or_default();
                let command_profile = record
                    .command_profile_id
                    .map(|id| format!(" command_profile_id={id}"))
                    .unwrap_or_default();
                let review_policy = record
                    .review_policy_id
                    .map(|id| format!(" review_policy_id={id}"))
                    .unwrap_or_default();
                let review_plan = record
                    .review_plan_id
                    .map(|id| format!(" review_plan_id={id}"))
                    .unwrap_or_default();
                let work_unit = record
                    .work_unit_id
                    .map(|id| format!(" work_unit_id={id}"))
                    .unwrap_or_default();
                let validation_gate = record
                    .validation_gate_id
                    .map(|id| format!(" validation_gate_id={id}"))
                    .unwrap_or_default();
                let acceptance_record = record
                    .acceptance_record_id
                    .map(|id| format!(" acceptance_record_id={id}"))
                    .unwrap_or_default();
                println!(
                    "{} [{}:{} precedence={}]{}{}{}{}{}{}{}{}{}{}",
                    record.id,
                    record.rule_source_type,
                    record.scope_type,
                    record.precedence,
                    scope_key,
                    authority,
                    user_correction,
                    command_profile,
                    review_policy,
                    review_plan,
                    work_unit,
                    validation_gate,
                    acceptance_record,
                    shadowed
                );
            }
        }
    }
    Ok(())
}
