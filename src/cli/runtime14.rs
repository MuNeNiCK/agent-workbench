use std::{fs, path::Path};

use anyhow::{Result, bail};

use agent_workbench::{
    accept_dependency14, accept_finding14, accept_repository_change14, activate_work14,
    add_acceptance_schema14, add_closure14, add_command_profile14, add_command_usage14,
    add_correction14, add_coverage_schema14, add_dependency14, add_finding14, add_kpt_item14,
    add_repository_change14, add_repository_commit14, add_repository_comparison14,
    add_repository_snapshot14, add_repository14, add_review_claim14, add_review_plan14,
    add_review_policy14, add_task14, add_typed_evidence14, add_verification_claim14,
    approve_design_schema14, assign_task14, classify_repository_change14,
    close_checklist_item_schema14, close_checklist_schema14, close_kpt14, close_work14,
    create_phase14, create_work_record14, decide_review14, decide_verification14,
    decompose_design_schema14, derive_task_schema14, design_gate_schema14, dispose_stale_schema14,
    except_correction14, finalize_repository_snapshot14, follow_up_work14, import_design_schema14,
    init_design_package_schema14, integrity14, link_correction_requirement14,
    link_correction_validation14, link_work_record14, list_evidence14, list_records14,
    list_relations14, phase_close_ready14, ready_closure14, remediate_finding14,
    render_work_record14, resolve_correction14, resume_check14, resume_work14,
    revoke_acceptance_schema14, satisfy_dependency14, start_kpt14, start_work_for_design14,
    status14, supersede_closure14, suspend_work14, transition_command_profile14,
    transition_kpt_item14, transition_phase14, transition_task14, transition_work14,
    waive_review_plan14, work_close_ready14,
};

use super::args::*;

pub(crate) fn handle(root: &Path, command: Command) -> Result<()> {
    match command {
        Command::Status => print_status(root)?,
        Command::Next => print_next(root)?,
        Command::Work { command } => handle_work(root, command)?,
        Command::ResumeCheck(_) => print_resume_check(root)?,
        Command::Task { command } => handle_task(root, command)?,
        Command::Phase { command } => handle_phase(root, command)?,
        Command::Review { command } => handle_review(root, command)?,
        Command::Correction { command } => handle_correction(root, command)?,
        Command::Kpt { command } => handle_kpt(root, command)?,
        Command::Finding { command } => handle_finding(root, command)?,
        Command::Closure { command } => handle_closure(root, command)?,
        Command::Repository { command } => handle_repository(root, command)?,
        Command::Command { command } => handle_command(root, command)?,
        Command::WorkRecord { command } => handle_work_record(root, command)?,
        Command::Evidence { command } => handle_evidence(root, command)?,
        Command::Rules { .. } => print_records(root, "rule", None)?,
        Command::Design { command } => handle_design(root, command)?,
        Command::Requirement { command } => match command {
            RequirementCommand::List(args) => print_records(
                root,
                "requirement",
                Some(&format!("design_version:{}", args.design)),
            )?,
        },
        Command::DesignDecision { command } => match command {
            DesignDecisionCommand::List(args) => print_records(
                root,
                "design_decision",
                Some(&format!("design_version:{}", args.design)),
            )?,
        },
        Command::GateTemplate { command } => match command {
            GateTemplateCommand::List(args) => print_records(
                root,
                "validation_gate",
                Some(&format!("design_version:{}", args.design)),
            )?,
        },
        Command::Trace { command } => handle_trace(root, command)?,
        Command::Coverage { command } => handle_coverage(root, command)?,
        Command::Decompose { command } => match command {
            DecomposeCommand::Design(args) => print_record(&decompose_design_schema14(
                root,
                args.design_version_id,
                args.work_unit,
                args.checklist_title
                    .as_deref()
                    .unwrap_or("Design implementation"),
            )?),
        },
        Command::Checklist { command } => handle_checklist(root, command)?,
        Command::Acceptance { command } => match command {
            AcceptanceCommand::Add(args) => print_record(&add_acceptance_schema14(
                root,
                &args.target,
                &args.acceptance_type,
                &args.reason,
                args.risk.as_deref(),
            )?),
            AcceptanceCommand::Revoke(args) => print_record(&revoke_acceptance_schema14(
                root,
                args.acceptance_id,
                &args.reason,
            )?),
        },
        Command::Stale { command } => match command {
            StaleCommand::List => print_records(root, "stale_disposition", None)?,
            StaleCommand::Accept(args) => print_decision(&dispose_stale_schema14(
                root,
                args.record_id,
                "accept",
                &args.expected_current,
                &args.reason,
            )?),
            StaleCommand::Close(args) => print_decision(&dispose_stale_schema14(
                root,
                args.record_id,
                "close",
                &args.expected_current,
                &args.reason,
            )?),
        },
        Command::ReviewContext(args) => {
            println!("kind: {}", args.kind);
            print_status(root)?;
            print_records(root, "review_plan", None)?;
            print_records(root, "finding", None)?;
        }
        Command::Export { command } => match command {
            ExportCommand::Design(args) => export_design_view(root, args.design, &args.output)?,
            ExportCommand::Plan(args) => export_plan_view(root, args.design, &args.output)?,
        },
        Command::Gate { command } => handle_gate(root, command)?,
        Command::Doctor { command } => match command {
            DoctorCommand::Integrity => {
                let result = integrity14(root)?;
                println!("quick_check: {}", result.quick_check);
                println!("foreign_key_violations: {}", result.foreign_key_violations);
                println!("manifest_digest: {}", result.manifest_digest);
                println!(
                    "result: {}",
                    if result.quick_check == "ok" && result.foreign_key_violations == 0 {
                        "pass"
                    } else {
                        "blocked"
                    }
                );
            }
        },
        Command::Init | Command::Update(_) => unreachable!(),
    }
    Ok(())
}

fn export_design_view(root: &Path, design: i64, output: &Path) -> Result<()> {
    let mut document = format!("# Design {design}\n\n");
    for record in list_records14(
        root,
        "requirement",
        Some(&format!("design_version:{design}")),
    )? {
        document.push_str(&format!(
            "- {} [{}] revision {}\n",
            record.handle, record.state, record.revision
        ));
    }
    fs::write(output, document)?;
    println!("output: {}", output.display());
    Ok(())
}

fn export_plan_view(root: &Path, design: i64, output: &Path) -> Result<()> {
    let mut document = format!("# Plan for design {design}\n\n");
    for kind in ["task", "phase", "checklist", "review_plan"] {
        for record in list_records14(root, kind, None)? {
            document.push_str(&format!(
                "- {} [{}] revision {}\n",
                record.handle, record.state, record.revision
            ));
        }
    }
    fs::write(output, document)?;
    println!("output: {}", output.display());
    Ok(())
}

fn handle_trace(root: &Path, command: TraceCommand) -> Result<()> {
    match command {
        TraceCommand::DeriveTask(args) => println!(
            "relation_handle: {}",
            derive_task_schema14(root, args.design, &args.requirement, args.task)?
        ),
    }
    Ok(())
}

fn handle_coverage(root: &Path, command: CoverageCommand) -> Result<()> {
    match command {
        CoverageCommand::Add(args) => print_record(&add_coverage_schema14(
            root,
            args.design,
            &args.requirement,
            args.task
                .ok_or_else(|| anyhow::anyhow!("coverage add requires --task"))?,
            &args.status,
            &args.requirement_text,
        )?),
        CoverageCommand::List(_) => print_records(root, "coverage", None)?,
    }
    Ok(())
}

fn handle_checklist(root: &Path, command: ChecklistCommand) -> Result<()> {
    match command {
        ChecklistCommand::List(_) => print_records(root, "checklist", None)?,
        ChecklistCommand::Close(args) => {
            print_record(&close_checklist_schema14(root, args.checklist_id)?)
        }
        ChecklistCommand::Item { command } => match command {
            ChecklistItemCommand::List(_) => print_records(root, "checklist_item", None)?,
            ChecklistItemCommand::Close(args) => print_record(&close_checklist_item_schema14(
                root,
                args.checklist_item_id,
            )?),
        },
    }
    Ok(())
}

fn handle_design(root: &Path, command: DesignCommand) -> Result<()> {
    match command {
        DesignCommand::Init(args) => {
            let path = init_design_package_schema14(
                root,
                &args.design_id,
                args.title.as_deref().unwrap_or(&args.design_id),
            )?;
            println!("package: {}", path.display());
        }
        DesignCommand::Import(args) | DesignCommand::Refresh(args) => print_record(
            &import_design_schema14(root, &args.package_path, &args.status)?,
        ),
        DesignCommand::Approve(args) => print_record(&approve_design_schema14(
            root,
            args.design_version_id,
            args.summary.as_deref().unwrap_or("approved"),
        )?),
    }
    Ok(())
}

fn handle_work_record(root: &Path, command: WorkRecordCommand) -> Result<()> {
    match command {
        WorkRecordCommand::Create(args) => print_record(&create_work_record14(
            root,
            args.work_unit.unwrap_or(active_work_id(root)?),
            &args.topic,
            args.work_performed.as_deref().unwrap_or(""),
        )?),
        WorkRecordCommand::List(_) => print_records(root, "work_record", None)?,
        WorkRecordCommand::Command { command } => match command {
            WorkRecordCommandLinkCommand::Add(args) => print_record(&link_work_record14(
                root,
                args.work_record_id
                    .or(args.record_id)
                    .ok_or_else(|| anyhow::anyhow!("record command add requires work record id"))?,
                &format!(
                    "command_usage:{}",
                    args.usage
                        .ok_or_else(|| anyhow::anyhow!("record command add requires --usage"))?
                ),
            )?),
        },
        WorkRecordCommand::Commit { command } => match command {
            WorkRecordCommitCommand::Add(args) => print_record(&link_work_record14(
                root,
                args.work_record_id
                    .or(args.record_id)
                    .ok_or_else(|| anyhow::anyhow!("record commit add requires work record id"))?,
                &format!(
                    "repository_commit:{}",
                    args.git_commit.ok_or_else(|| anyhow::anyhow!(
                        "record commit add requires --git-commit"
                    ))?
                ),
            )?),
        },
        WorkRecordCommand::File { command } => match command {
            WorkRecordFileCommand::Add(args) => print_record(&link_work_record14(
                root,
                args.work_record_id
                    .or(args.record_id)
                    .ok_or_else(|| anyhow::anyhow!("record file add requires work record id"))?,
                &format!(
                    "repository_change:{}",
                    args.git_file_change.ok_or_else(|| anyhow::anyhow!(
                        "record file add requires --git-file-change"
                    ))?
                ),
            )?),
        },
        WorkRecordCommand::Export(args) => {
            fs::write(
                &args.output,
                render_work_record14(root, args.work_record_id)?,
            )?;
            println!("output: {}", args.output.display());
        }
    }
    Ok(())
}

fn handle_evidence(root: &Path, command: EvidenceCommand) -> Result<()> {
    match command {
        EvidenceCommand::Add(args) => {
            let task = args
                .task
                .ok_or_else(|| anyhow::anyhow!("evidence add requires --task"))?;
            println!(
                "evidence_handle: {}",
                add_typed_evidence14(
                    root,
                    &args.evidence_type,
                    &format!("work:{}", active_work_id(root)?),
                    &format!("task:{task}"),
                    "owner",
                    "recorded",
                    args.artifact.as_deref().or(args.commit.as_deref()),
                    args.note.as_deref(),
                )?
            );
        }
        EvidenceCommand::List(_) => {
            for (handle, kind, result) in list_evidence14(root, None)? {
                println!("evidence_handle: {handle}");
                println!("kind: {kind}");
                println!("result: {result}");
            }
        }
    }
    Ok(())
}

fn handle_command(root: &Path, command: MemoryCommand) -> Result<()> {
    match command {
        MemoryCommand::Add(args) => print_record(&add_command_profile14(
            root,
            active_work_id(root)?,
            &args.name,
            &args.command,
        )?),
        MemoryCommand::List(_) => print_records(root, "command_profile", None)?,
        MemoryCommand::Prefer(args) => print_record(&transition_command_profile14(
            root,
            args.profile_id,
            "prefer",
            &args.reason,
        )?),
        MemoryCommand::Fix(args) => print_record(&transition_command_profile14(
            root,
            args.profile_id,
            "fix",
            &args.reason,
        )?),
        MemoryCommand::Deprecate(args) => print_record(&transition_command_profile14(
            root,
            args.profile_id,
            "deprecate",
            &args.reason,
        )?),
        MemoryCommand::Usage { command } => match command {
            CommandUsageCommand::Add(args) => print_record(&add_command_usage14(
                root,
                args.profile,
                &args.command,
                &args.result,
                &args.output_digest,
            )?),
            CommandUsageCommand::List(_) => print_records(root, "command_usage", None)?,
        },
        MemoryCommand::Deviation { command } => match command {
            CommandDeviationCommand::Add(args) => {
                let profile = if args.profile.starts_with("command_profile:") {
                    args.profile
                } else {
                    format!("command_profile:{}", args.profile)
                };
                let subject = args
                    .usage
                    .map(|id| format!("command_usage:{id}"))
                    .unwrap_or_else(|| profile.clone());
                println!(
                    "evidence_handle: {}",
                    add_typed_evidence14(
                        root,
                        "validation",
                        &profile,
                        &subject,
                        "owner",
                        "deviation",
                        None,
                        Some(&args.reason),
                    )?
                );
            }
        },
    }
    Ok(())
}

fn handle_repository(root: &Path, command: RepositoryCommand) -> Result<()> {
    match command {
        RepositoryCommand::Add(args) => print_record(&add_repository14(
            root,
            active_work_id(root)?,
            &args.name,
            &args.path,
        )?),
        RepositoryCommand::List => print_records(root, "repository", None)?,
        RepositoryCommand::Snapshot { command } => match command {
            RepositorySnapshotCommand::Add(args) => {
                let snapshot = add_repository_snapshot14(
                    root,
                    handle_id(&args.repository, "repository")?,
                    args.head.as_deref().unwrap_or("working-tree"),
                )?;
                let snapshot_id = numeric_suffix(&snapshot.handle)
                    .ok_or_else(|| anyhow::anyhow!("invalid snapshot handle"))?;
                for change in args.changes {
                    let (path, digest) = change
                        .split_once('=')
                        .ok_or_else(|| anyhow::anyhow!("--change requires PATH=DIGEST"))?;
                    add_repository_change14(root, snapshot_id, path, digest)?;
                }
                print_record(&snapshot);
            }
            RepositorySnapshotCommand::List(_) => print_records(root, "repository_snapshot", None)?,
            RepositorySnapshotCommand::Finalize(args) => {
                print_record(&finalize_repository_snapshot14(root, args.snapshot_id)?)
            }
        },
        RepositoryCommand::Classify { command } => match command {
            RepositoryClassifyCommand::Add(args) => {
                let change = args
                    .dirty_entry
                    .ok_or_else(|| anyhow::anyhow!("repository classify requires --dirty-entry"))?;
                if args.accept_exception {
                    print_decision(&accept_repository_change14(
                        root,
                        change,
                        args.expected_current.as_deref().unwrap_or("none"),
                        &args.reason,
                        args.risk.as_deref().unwrap_or("accepted repository risk"),
                    )?);
                } else {
                    print_record(&classify_repository_change14(
                        root,
                        change,
                        &args.classification,
                    )?);
                }
            }
        },
        RepositoryCommand::Commit { command } => match command {
            RepositoryCommitCommand::Add(args) => print_record(&add_repository_commit14(
                root,
                args.snapshot,
                &args.sha,
                &args.content_digest,
            )?),
        },
        RepositoryCommand::File { command } => match command {
            RepositoryFileCommand::Add(args) => {
                let record = add_repository_change14(
                    root,
                    args.snapshot,
                    &args.path,
                    args.hash.as_deref().unwrap_or("unavailable"),
                )?;
                let id = numeric_suffix(&record.handle)
                    .ok_or_else(|| anyhow::anyhow!("invalid repository change handle"))?;
                print_record(&classify_repository_change14(root, id, &args.change_type)?);
            }
        },
        RepositoryCommand::Compare { command } => match command {
            RepositoryCompareCommand::Add(args) => {
                print_record(&add_repository_comparison14(root, args.current, args.base)?)
            }
        },
    }
    Ok(())
}

fn handle_finding(root: &Path, command: FindingCommand) -> Result<()> {
    match command {
        FindingCommand::Add(args) => print_record(&add_finding14(
            root,
            active_work_id(root)?,
            &args.severity,
            &args.description,
        )?),
        FindingCommand::List(_) => print_records(root, "finding", None)?,
        FindingCommand::Verify(args) => print_claim(&add_verification_claim14(
            root,
            args.attempt,
            &args.result,
            &args.producer,
            &args.scope_digest,
            args.notes.as_deref(),
        )?),
        FindingCommand::Decide(args) => print_decision(&decide_verification14(
            root,
            args.finding_id,
            args.closure,
            args.attempt,
            args.claim,
            &args.decision,
            &args.expected_current,
            &args.reason,
        )?),
        FindingCommand::Remediate(args) => {
            let (work, replace) = match (args.work, args.replace_work) {
                (Some(work), None) => (work, false),
                (None, Some(work)) => (work, true),
                _ => bail!("finding remediate requires exactly one of --work or --replace-work"),
            };
            println!(
                "relation_handle: {}",
                remediate_finding14(root, args.finding_id, work, replace)?
            );
        }
        FindingCommand::AcceptOutOfScope(args) => print_decision(&accept_finding14(
            root,
            args.finding_id,
            &args.expected_current,
            &args.reason,
            &args.risk,
        )?),
    }
    Ok(())
}

fn handle_closure(root: &Path, command: ClosureCommand) -> Result<()> {
    match command {
        ClosureCommand::Add(args) => {
            print_record(&add_closure14(root, args.finding, &args.invariant)?)
        }
        ClosureCommand::Ready(args) => print_record(&ready_closure14(
            root,
            args.closure_id,
            &format!("{}; tests={}", args.evidence, args.tests),
        )?),
        ClosureCommand::Supersede(args) => print_record(&supersede_closure14(
            root,
            args.closure_id,
            &args.expected_current,
            &format!(
                "{}; surfaces={}; tests={}",
                args.invariant, args.surfaces, args.tests
            ),
            &args.reason,
        )?),
    }
    Ok(())
}

fn handle_correction(root: &Path, command: CorrectionCommand) -> Result<()> {
    match command {
        CorrectionCommand::Add(args) => print_record(&add_correction14(
            root,
            active_work_id(root)?,
            &args.pattern,
            &args.severity,
            &args.correction,
        )?),
        CorrectionCommand::List(_) => print_records(root, "correction", None)?,
        CorrectionCommand::LinkRequirement(args) => print_record(&link_correction_requirement14(
            root,
            args.correction_id,
            &args.requirement,
        )?),
        CorrectionCommand::LinkValidation(args) => print_record(&link_correction_validation14(
            root,
            args.correction_id,
            &args.usage,
        )?),
        CorrectionCommand::Resolve(args) => print_record(&resolve_correction14(
            root,
            args.correction_id,
            &args.reason,
        )?),
        CorrectionCommand::Except(args) => print_decision(&except_correction14(
            root,
            args.correction_id,
            &args.expected_current,
            &args.reason,
            &args.risk,
        )?),
    }
    Ok(())
}

fn handle_kpt(root: &Path, command: KptCommand) -> Result<()> {
    match command {
        KptCommand::Start(args) => print_record(&start_kpt14(
            root,
            active_work_id(root)?,
            args.summary.as_deref().unwrap_or("KPT review"),
        )?),
        KptCommand::Close(args) => print_record(&close_kpt14(root, args.kpt_review_id)?),
        KptCommand::Item { command } => match command {
            KptItemCommand::Add(args) => print_record(&add_kpt_item14(
                root,
                args.review
                    .ok_or_else(|| anyhow::anyhow!("KPT item requires --review"))?,
                &args.item_type,
                &args.title,
                &args.severity,
            )?),
            KptItemCommand::List(_) => print_records(root, "kpt_item", None)?,
            KptItemCommand::Convert(args) => print_record(&transition_kpt_item14(
                root,
                args.item,
                "convert",
                args.details.as_deref().unwrap_or(&args.target_type),
            )?),
            KptItemCommand::Dismiss(args) => print_record(&transition_kpt_item14(
                root,
                args.item,
                "dismiss",
                &args.reason,
            )?),
        },
    }
    Ok(())
}

fn handle_review(root: &Path, command: ReviewCommand) -> Result<()> {
    match command {
        ReviewCommand::Policy { command } => match command {
            ReviewPolicyCommand::Add(args) => print_record(&add_review_policy14(
                root,
                active_work_id(root)?,
                &args.name,
                args.max_fresh_agents,
            )?),
            ReviewPolicyCommand::List => print_records(root, "review_policy", None)?,
        },
        ReviewCommand::Plan { command } => match command {
            ReviewPlanCommand::Add(args) => {
                let policy = args
                    .policy
                    .ok_or_else(|| anyhow::anyhow!("review plan requires --policy"))?;
                print_record(&add_review_plan14(
                    root,
                    args.work_unit,
                    &args.stage,
                    policy,
                    args.phase,
                    args.required,
                )?);
            }
            ReviewPlanCommand::List => print_records(root, "review_plan", None)?,
            ReviewPlanCommand::Waive(args) => print_decision(&waive_review_plan14(
                root,
                args.review_plan_id,
                &args.expected_current,
                &args.reason,
                args.risk.as_deref(),
            )?),
        },
        ReviewCommand::Run { command } => match command {
            ReviewRunCommand::Add(args) => {
                let outcome = if args.clean {
                    "clean"
                } else {
                    args.finding_result.as_deref().unwrap_or("changes-required")
                };
                let producer = args
                    .external_agent_id
                    .as_deref()
                    .or(args.agent_label.as_deref())
                    .unwrap_or("external-reviewer");
                let scope = args.target.as_deref().unwrap_or(&args.purpose);
                print_claim(&add_review_claim14(
                    root,
                    args.plan,
                    outcome,
                    producer,
                    scope,
                    args.summary.as_deref(),
                )?);
            }
            ReviewRunCommand::List(_) => bail!("use `review plan list` for current review state"),
        },
        ReviewCommand::Decide(args) => print_decision(&decide_review14(
            root,
            args.plan,
            args.claim_id,
            &args.decision,
            &args.expected_current,
            &args.reason,
        )?),
    }
    Ok(())
}

fn print_status(root: &Path) -> Result<()> {
    let status = status14(root)?;
    println!("initialized");
    println!("schema_version: 14");
    println!("open_work_units: {}", status.open_work);
    println!("active_activations: {}", status.active_activations);
    println!("project_integrity: clear");
    for resolution in status.resolutions {
        println!("owner: {}", resolution.owner_handle);
        println!("owner_state: {}", resolution.owner_state);
        println!("state_revision: {}", resolution.state_revision);
        if let Some(blocker) = resolution.blocker {
            println!("owner_blocker_kind: {blocker}");
        }
        println!("legal_actions: {}", resolution.legal_actions.join(", "));
        println!("selected_action: {}", resolution.selected_action);
        println!("next: {}", resolution.selected_action);
    }
    Ok(())
}

fn print_next(root: &Path) -> Result<()> {
    let resolution = status14(root)?
        .resolutions
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("resolver produced no project action"))?;
    println!("owner: {}", resolution.owner_handle);
    println!("owner_state: {}", resolution.owner_state);
    println!("state_revision: {}", resolution.state_revision);
    println!("legal_actions: {}", resolution.legal_actions.join(", "));
    println!("next: {}", resolution.selected_action);
    Ok(())
}

fn handle_work(root: &Path, command: WorkCommand) -> Result<()> {
    match command {
        WorkCommand::Start(args) => print_record(&start_work_for_design14(
            root,
            &args.title,
            args.design_version,
        )?),
        WorkCommand::Activate(args) => print_record(&activate_work14(root, args.work_unit_id)?),
        WorkCommand::Block(args) => {
            let id = args.work_unit_id.unwrap_or(active_work_id(root)?);
            print_record(&transition_work14(root, id, "block", &args.reason)?);
        }
        WorkCommand::Unblock(args) => {
            let id = args.work_unit_id.unwrap_or(single_open_work_id(root)?);
            print_record(&transition_work14(root, id, "unblock", &args.reason)?);
        }
        WorkCommand::Suspend(args) => {
            print_record(&suspend_work14(root, &args.reason, &args.next)?);
        }
        WorkCommand::Resume(_) => print_record(&resume_work14(root)?),
        WorkCommand::Close(args) => print_record(&close_work14(root, &args.summary)?),
        WorkCommand::Abandon(args) => {
            let id = args.work_unit_id.unwrap_or(single_open_work_id(root)?);
            print_record(&transition_work14(root, id, "abandon", &args.reason)?);
        }
        WorkCommand::Reopen(args) => {
            print_record(&transition_work14(
                root,
                args.work_unit_id,
                "reopen",
                &args.reason,
            )?);
        }
        WorkCommand::FollowUp(args) => print_record(&follow_up_work14(
            root,
            args.source_work_unit_id,
            &args.title,
            &args.reason,
        )?),
        WorkCommand::Dependency { command } => match command {
            WorkDependencyCommand::Add(args) => println!(
                "relation_handle: {}",
                add_dependency14(
                    root,
                    "work_dependency",
                    &format!("work:{}", args.from_work),
                    &format!("work:{}", args.to_work),
                    &format!("{}: {}", args.dependency_type, args.reason),
                )?
            ),
            WorkDependencyCommand::List(args) => print_relations(
                root,
                "work_dependency",
                Some(&format!("work:{}", args.work_unit)),
            )?,
            WorkDependencyCommand::Satisfy(args) => println!(
                "relation_handle: {}",
                satisfy_dependency14(
                    root,
                    &format!("work_dependency:{}", args.dependency_id),
                    &args.reason,
                )?
            ),
            WorkDependencyCommand::Accept(args) => print_decision(&accept_dependency14(
                root,
                &format!("work_dependency:{}", args.dependency_id),
                &args.expected_current,
                &args.reason,
                args.risk.as_deref(),
            )?),
        },
    }
    Ok(())
}

fn handle_task(root: &Path, command: TaskCommand) -> Result<()> {
    match command {
        TaskCommand::Add(args) => {
            let work = args.work_unit.unwrap_or(active_work_id(root)?);
            print_record(&add_task14(
                root,
                work,
                &args.title,
                &args.priority,
                args.details.as_deref(),
            )?);
        }
        TaskCommand::List(args) => {
            let owner = args.work_unit.map(|id| format!("work:{id}"));
            for record in list_records14(root, "task", owner.as_deref())? {
                if args
                    .status
                    .as_deref()
                    .is_none_or(|status| status == record.state)
                {
                    print_record(&record);
                }
            }
        }
        TaskCommand::Block(args) => print_record(&transition_task14(
            root,
            args.task_id,
            "block",
            &args.reason,
        )?),
        TaskCommand::Unblock(args) => print_record(&transition_task14(
            root,
            args.task_id,
            "unblock",
            &args.reason,
        )?),
        TaskCommand::Close(args) => {
            print_record(&transition_task14(
                root,
                args.task_id,
                "close",
                "completed",
            )?);
        }
        TaskCommand::AcceptOutOfScope(args) => print_record(&transition_task14(
            root,
            args.task_id,
            "accept-out-of-scope",
            &args.reason,
        )?),
    }
    Ok(())
}

fn handle_phase(root: &Path, command: PhaseCommand) -> Result<()> {
    match command {
        PhaseCommand::Create(args) => print_record(&create_phase14(
            root,
            args.work_unit,
            &args.key,
            &args.title,
            args.order,
        )?),
        PhaseCommand::List(args) => {
            let owner = format!("work:{}", args.work_unit);
            for record in list_records14(root, "phase", Some(&owner))? {
                print_record(&record);
            }
        }
        PhaseCommand::Show(args) => {
            let handle = format!("phase:{}", args.phase_id);
            let record = list_records14(root, "phase", None)?
                .into_iter()
                .find(|record| record.handle == handle)
                .ok_or_else(|| anyhow::anyhow!("phase not found"))?;
            print_record(&record);
        }
        PhaseCommand::Assign(args) => println!(
            "relation_handle: {}",
            assign_task14(root, args.phase_id, args.task)?
        ),
        PhaseCommand::Dependency { command } => match command {
            PhaseDependencyCommand::Add(args) => println!(
                "relation_handle: {}",
                add_dependency14(
                    root,
                    "phase_dependency",
                    &format!("phase:{}", args.from_phase),
                    &format!("phase:{}", args.to_phase),
                    &format!("{}: {}", args.dependency_type, args.reason),
                )?
            ),
            PhaseDependencyCommand::List(_) => print_relations(root, "phase_dependency", None)?,
            PhaseDependencyCommand::Satisfy(args) => println!(
                "relation_handle: {}",
                satisfy_dependency14(
                    root,
                    &format!("phase_dependency:{}", args.dependency_id),
                    &args.reason,
                )?
            ),
            PhaseDependencyCommand::Accept(args) => print_decision(&accept_dependency14(
                root,
                &format!("phase_dependency:{}", args.dependency_id),
                &args.expected_current,
                &args.reason,
                args.risk.as_deref(),
            )?),
        },
        PhaseCommand::CloseReady(args) => {
            let (ready, blockers) = phase_close_ready14(root, args.phase_id)?;
            println!("gate: phase-close-ready");
            println!("result: {}", if ready { "pass" } else { "blocked" });
            for blocker in blockers {
                println!("blocker: {blocker}");
            }
        }
        PhaseCommand::Close(args) => print_record(&transition_phase14(
            root,
            args.phase_id,
            "close",
            &args.summary,
        )?),
        PhaseCommand::AcceptOutOfScope(args) => print_record(&transition_phase14(
            root,
            args.phase_id,
            "accept-out-of-scope",
            &args.reason,
        )?),
    }
    Ok(())
}

fn handle_gate(root: &Path, command: GateCommand) -> Result<()> {
    match command {
        GateCommand::PhaseCloseReady(args) => {
            let (ready, blockers) = phase_close_ready14(root, args.phase_id)?;
            println!("gate: phase-close-ready");
            println!("result: {}", if ready { "pass" } else { "blocked" });
            for blocker in blockers {
                println!("blocker: {blocker}");
            }
        }
        GateCommand::CloseReady(_) => {
            let (ready, blockers) = work_close_ready14(root)?;
            println!("gate: close-ready");
            println!("result: {}", if ready { "pass" } else { "blocked" });
            for blocker in blockers {
                println!("blocker: {blocker}");
            }
        }
        GateCommand::ResumeReady(_) => print_resume_check(root)?,
        GateCommand::DesignReady(args) => print_design_gate(
            root,
            args.design_version
                .ok_or_else(|| anyhow::anyhow!("design-ready requires --design-version"))?,
            false,
        )?,
        GateCommand::ImplementationReady(args) => print_design_gate(
            root,
            args.design_version
                .ok_or_else(|| anyhow::anyhow!("implementation-ready requires --design-version"))?,
            true,
        )?,
    }
    Ok(())
}

fn print_design_gate(root: &Path, design: i64, implementation: bool) -> Result<()> {
    let (ready, blockers) = design_gate_schema14(root, design, implementation)?;
    println!(
        "gate: {}",
        if implementation {
            "implementation-ready"
        } else {
            "design-ready"
        }
    );
    println!("result: {}", if ready { "pass" } else { "blocked" });
    for blocker in blockers {
        println!("blocker: {blocker}");
    }
    Ok(())
}

fn print_resume_check(root: &Path) -> Result<()> {
    let check = resume_check14(root)?;
    println!("snapshot: {}", check.snapshot_handle);
    println!("recorded_digest: {}", check.recorded_digest);
    println!("current_digest: {}", check.current_digest);
    println!("result: {}", check.result);
    for changed in check.changed_components {
        println!("changed: {changed}");
    }
    Ok(())
}

fn print_record(record: &agent_workbench::Record14) {
    println!("handle: {}", record.handle);
    println!("state: {}", record.state);
    println!("revision: {}", record.revision);
}

fn print_records(root: &Path, kind: &str, owner: Option<&str>) -> Result<()> {
    for record in list_records14(root, kind, owner)? {
        print_record(&record);
    }
    Ok(())
}

fn print_relations(root: &Path, kind: &str, source: Option<&str>) -> Result<()> {
    for (handle, state) in list_relations14(root, kind, source)? {
        println!("relation_handle: {handle}");
        println!("state: {state}");
    }
    Ok(())
}

fn print_claim(claim: &agent_workbench::Claim14) {
    println!("claim_handle: {}", claim.handle);
    println!("target_handle: {}", claim.target_handle);
    println!("outcome: {}", claim.outcome);
}

fn print_decision(decision: &agent_workbench::Decision14) {
    println!("decision_handle: {}", decision.handle);
    println!("target_handle: {}", decision.target_handle);
    println!("resulting_state: {}", decision.resulting_state);
}

fn active_work_id(root: &Path) -> Result<i64> {
    let status = status14(root)?;
    status
        .resolutions
        .into_iter()
        .find(|resolution| {
            resolution.owner_handle.starts_with("work:") && resolution.owner_state == "open"
        })
        .and_then(|resolution| numeric_suffix(&resolution.owner_handle))
        .ok_or_else(|| anyhow::anyhow!("no active work owner"))
}

fn single_open_work_id(root: &Path) -> Result<i64> {
    let ids = status14(root)?
        .resolutions
        .into_iter()
        .filter_map(|resolution| numeric_suffix(&resolution.owner_handle))
        .collect::<Vec<_>>();
    if ids.len() != 1 {
        bail!("work owner is missing or ambiguous");
    }
    Ok(ids[0])
}

fn numeric_suffix(handle: &str) -> Option<i64> {
    handle.rsplit_once(':')?.1.parse().ok()
}

fn handle_id(value: &str, kind: &str) -> Result<i64> {
    value
        .parse()
        .ok()
        .or_else(|| {
            value
                .strip_prefix(&format!("{kind}:"))
                .and_then(|id| id.parse().ok())
        })
        .ok_or_else(|| anyhow::anyhow!("invalid {kind} handle"))
}
