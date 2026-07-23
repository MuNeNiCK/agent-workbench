use std::path::Path;

use anyhow::Result;

use super::args::*;
use agent_workbench::*;

pub(crate) fn handle_task(root: &Path, command: TaskCommand) -> Result<()> {
    match command {
        TaskCommand::Add(args) => {
            let task = NewTask {
                title: &args.title,
                priority: &args.priority,
                source: &args.source,
                work_unit_id: args.work_unit,
                details: args.details.as_deref(),
                completion_condition: args.completion_condition.as_deref(),
            };
            let outcome = match (args.under_correction_closure, args.phase) {
                (Some(closure_id), Some(phase_id)) => add_correction_support_task(
                    root,
                    CorrectionSupportTask {
                        task,
                        closure_id,
                        phase_id,
                    },
                )?,
                (None, None) => add_task(root, task)?,
                _ => unreachable!("clap requires correction closure and phase together"),
            };
            println!("added task");
            println!("task_id: {}", outcome.task_id);
            if let Some(work_unit_id) = outcome.work_unit_id {
                println!("work_unit_id: {work_unit_id}");
            }
        }
        TaskCommand::List(args) => {
            let records = list_tasks(
                root,
                TaskListQuery {
                    status: args.status.as_deref(),
                    work_unit_id: args.work_unit,
                },
            )?;
            if records.is_empty() {
                println!("no tasks");
            }
            for record in records {
                let work_unit = record
                    .work_unit_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "-".to_string());
                println!(
                    "{} [work_unit={} {}:{}] {}",
                    record.id, work_unit, record.priority, record.status, record.title
                );
            }
        }
        TaskCommand::Close(args) => {
            let outcome = close_task(root, args.task_id, args.commit.as_deref())?;
            println!("closed task");
            println!("task_id: {}", outcome.task_id);
        }
        TaskCommand::AcceptOutOfScope(args) => {
            accept_task_out_of_scope(root, args.task_id, &args.reason)?;
            println!("accepted task out of scope");
        }
    }
    Ok(())
}

pub(crate) fn handle_decision(root: &Path, command: DecisionCommand) -> Result<()> {
    match command {
        DecisionCommand::Add(args) => {
            let outcome = add_decision(
                root,
                NewDecision {
                    decision_key: args.key.as_deref(),
                    topic: &args.topic,
                    decision: &args.decision,
                    rationale: args.rationale.as_deref(),
                    compatibility_impact: args.compatibility_impact.as_deref(),
                    authority_refs: args.authority_refs.as_deref(),
                },
            )?;
            println!("added decision");
            println!("decision_id: {}", outcome.decision_id);
        }
        DecisionCommand::Adjudicate(args) => {
            let outcome = adjudicate_owner(
                root,
                &args.owner,
                args.target,
                AdjudicationInput {
                    decision: &args.decision,
                    reason: &args.reason,
                    expected_current: &args.expected_current,
                },
            )?;
            println!("decision_handle: {}", outcome.decision_handle);
        }
        DecisionCommand::Continuation { command } => match command {
            DecisionContinuationCommand::Show(args) => {
                let record = show_decision_continuation(root, &args.continuation_handle)?;
                println!("continuation: {}", record.continuation_handle);
                println!("status: {}", record.status);
                println!("owner: {}", record.owner_ref);
                println!("target: {}", record.target_ref);
                println!("decision_family: {}", record.decision_family);
                println!("action: {}", record.action);
                println!("expected_current: {}", record.expected_current);
                println!("context_identity: {}", record.context_identity);
                println!("code: {}", record.rejection_code);
                println!("required_input: {}", record.required_inputs);
                if let Some(decision) = record.decision_handle {
                    println!("decision_handle: {decision}");
                }
                if let Some(successor) = record.successor_continuation {
                    println!("successor_continuation: {successor}");
                    println!("next: agent-workbench decision continuation show {successor}");
                }
            }
            DecisionContinuationCommand::Apply(args) => {
                let outcome = apply_decision_continuation(
                    root,
                    DecisionContinuationApply {
                        continuation_handle: &args.continuation_handle,
                        decision: &args.decision,
                        reason: &args.reason,
                        expected_current: &args.expected_current,
                    },
                )?;
                println!("continuation: {}", outcome.continuation_handle);
                println!("status: {}", outcome.status);
                if let Some(decision) = outcome.decision_handle {
                    println!("decision_handle: {decision}");
                }
                if let Some(successor) = outcome.successor_continuation {
                    println!("successor_continuation: {successor}");
                }
                println!("idempotent: {}", outcome.idempotent);
                println!("next: {}", outcome.next_action);
            }
        },
        DecisionCommand::List(args) => {
            print_decisions(list_decisions_filtered(
                root,
                DecisionListFilter {
                    query: args.query.as_deref(),
                    topic: args.topic.as_deref(),
                },
            )?);
        }
        DecisionCommand::Search(args) => {
            print_decisions(list_decisions(root, Some(&args.query))?);
        }
    }
    Ok(())
}

pub(crate) fn handle_design(root: &Path, command: DesignCommand) -> Result<()> {
    match command {
        DesignCommand::Init(args) => {
            let title = args.title.as_deref().unwrap_or(&args.design_id);
            init_design_package(
                root,
                NewDesignPackage {
                    design_id: &args.design_id,
                    title,
                },
            )?;
            println!("initialized design package");
        }
        DesignCommand::Import(args) => {
            let outcome = import_design_package(
                root,
                DesignPackageImport {
                    package_path: &args.package_path,
                    status: &args.status,
                },
            )?;
            println!("imported design package");
            println!("design_version_id: {}", outcome.design_version_id);
            println!("design_identity: {}", outcome.content_hash);
        }
        DesignCommand::Refresh(args) => {
            let outcome = import_design_package(
                root,
                DesignPackageImport {
                    package_path: &args.package_path,
                    status: &args.status,
                },
            )?;
            println!("refreshed design package");
            println!("design_version_id: {}", outcome.design_version_id);
            println!("design_identity: {}", outcome.content_hash);
        }
        DesignCommand::Inspect(args) => {
            let outcome = inspect_design_version_ref(root, &args.design_version_ref)?;
            println!("design_version_id: {}", outcome.design_version_id);
            println!("version_number: {}", outcome.version_number);
            println!("status: {}", outcome.status);
            println!("current: {}", outcome.current);
            println!("design_identity: {}", outcome.design_identity);
        }
        DesignCommand::Approve(args) => {
            approve_design_version(
                root,
                DesignVersionApproval {
                    design_version_id: args.design_version_id,
                    summary: args.summary.as_deref(),
                },
            )?;
            println!("approved design version");
        }
    }
    Ok(())
}

pub(crate) fn handle_requirement(root: &Path, command: RequirementCommand) -> Result<()> {
    match command {
        RequirementCommand::List(args) => {
            let records = list_design_requirements(
                root,
                DesignRequirementListQuery {
                    design_version_id: args.design,
                },
            )?;
            if records.is_empty() {
                println!("no requirements");
            }
            for record in records {
                println!(
                    "{} [{}:{} rev={}] {} ({})",
                    record.requirement_key,
                    record.priority,
                    record.status,
                    record.revision,
                    record.source_section,
                    record.source_path
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn handle_design_decision(root: &Path, command: DesignDecisionCommand) -> Result<()> {
    match command {
        DesignDecisionCommand::List(args) => {
            let records = list_design_decisions(
                root,
                DesignDecisionListQuery {
                    design_version_id: args.design,
                },
            )?;
            if records.is_empty() {
                println!("no design decisions");
            }
            for record in records {
                println!(
                    "{} [{}:{}] {} ({})",
                    record.decision_key,
                    record.topic,
                    record.status,
                    record.source_section,
                    record.source_path
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn handle_gate_template(root: &Path, command: GateTemplateCommand) -> Result<()> {
    match command {
        GateTemplateCommand::List(args) => {
            let records = list_validation_gate_templates(
                root,
                ValidationGateTemplateListQuery {
                    design_version_id: args.design,
                },
            )?;
            if records.is_empty() {
                println!("no validation gate templates");
            }
            for record in records {
                let command = record.command.as_deref().unwrap_or("-");
                println!(
                    "{} [{}:{} expected={} command={}] {} ({})",
                    record.gate_key,
                    record.stage,
                    record.status,
                    record.expected_result,
                    command,
                    record.source_section,
                    record.source_path
                );
            }
        }
    }
    Ok(())
}

pub(crate) fn handle_trace(root: &Path, command: TraceCommand) -> Result<()> {
    match command {
        TraceCommand::DeriveTask(args) => {
            if let Some(closure_id) = args.revise_completion_under_closure {
                let details = args.details.as_deref().ok_or_else(|| {
                    anyhow::anyhow!("--details is required when revising completion")
                })?;
                let completion_condition =
                    args.completion_condition.as_deref().ok_or_else(|| {
                        anyhow::anyhow!(
                            "--completion-condition is required when revising completion"
                        )
                    })?;
                let outcome = revise_task_completion(
                    root,
                    TaskCompletionRevision {
                        task_id: args.task,
                        closure_id,
                        design_version_id: args.design,
                        requirement_key: &args.requirement,
                        details,
                        completion_condition,
                    },
                )?;
                println!("revised task derivation completion");
                println!("task_id: {}", outcome.task_id);
                println!(
                    "checklist_items_updated: {}",
                    outcome.checklist_items_updated
                );
                return Ok(());
            }
            derive_task_from_requirement(
                root,
                NewTaskDerivation {
                    design_version_id: args.design,
                    requirement_key: &args.requirement,
                    task_id: args.task,
                    derivation_reason: args.reason.as_deref(),
                    checklist_title: args.checklist_title.as_deref(),
                    item_title: args.item_title.as_deref(),
                    completion_condition: args.completion_condition.as_deref(),
                },
            )?;
            println!("derived task from requirement");
        }
        TraceCommand::Derivation { command } => match command {
            TraceDerivationCommand::List(args) => {
                let records = list_task_derivations_filtered(
                    root,
                    TaskDerivationListFilter {
                        design_version_id: args.design.or(args.design_version),
                        task_id: args.task,
                        work_unit_id: None,
                    },
                )?;
                if records.is_empty() {
                    println!("no task derivations");
                }
                for record in records {
                    let checklist_item = record
                        .checklist_item_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    println!(
                        "{} [{}] requirement={} task={} checklist_item={} {}",
                        record.id,
                        record.status,
                        record.requirement_key,
                        record.task_id,
                        checklist_item,
                        record.task_title
                    );
                }
            }
            TraceDerivationCommand::Rebind(args) => {
                let outcome = rebind_task_derivation(
                    root,
                    TaskDerivationRebind {
                        design_version_id: args.design,
                        requirement_key: &args.requirement,
                        task_id: args.task,
                        checklist_item_id: args.checklist_item,
                        closure_id: args.closure,
                        reason: &args.reason,
                    },
                )?;
                println!("rebound task derivation");
                println!("task_derivation_id: {}", outcome.task_derivation_id);
                println!(
                    "previous_checklist_item_id: {}",
                    outcome.previous_checklist_item_id
                );
                println!("checklist_item_id: {}", outcome.checklist_item_id);
                println!("idempotent: {}", outcome.idempotent);
            }
        },
    }
    Ok(())
}

pub(crate) fn handle_evidence(root: &Path, command: EvidenceCommand) -> Result<()> {
    match command {
        EvidenceCommand::Add(args) => {
            if args.repository_id.is_some()
                || args.git_commit_id.is_some()
                || args.git_file_change_id.is_some()
            {
                add_implementation_evidence_with_git(
                    root,
                    NewImplementationEvidenceWithGit {
                        task_id: args.task,
                        design_version_id: args.design,
                        requirement_key: args.requirement.as_deref(),
                        evidence_type: &args.evidence_type,
                        repository_id: args.repository_id,
                        git_commit_id: args.git_commit_id,
                        git_file_change_id: args.git_file_change_id,
                        commit_sha: args.commit.as_deref(),
                        file_path: args.file.as_deref(),
                        line_ref: args.line.as_deref(),
                        symbol: args.symbol.as_deref(),
                        artifact_path: args.artifact.as_deref(),
                        note: args.note.as_deref(),
                    },
                )?
            } else {
                add_implementation_evidence(
                    root,
                    NewImplementationEvidence {
                        task_id: args.task,
                        design_version_id: args.design,
                        requirement_key: args.requirement.as_deref(),
                        evidence_type: &args.evidence_type,
                        commit_sha: args.commit.as_deref(),
                        file_path: args.file.as_deref(),
                        line_ref: args.line.as_deref(),
                        symbol: args.symbol.as_deref(),
                        artifact_path: args.artifact.as_deref(),
                        note: args.note.as_deref(),
                    },
                )?
            };
            println!("added implementation evidence");
        }
        EvidenceCommand::List(args) => {
            let (owner_task, owner_design, owner_work) = match args.owner.as_deref() {
                None => (None, None, None),
                Some(owner) => parse_evidence_owner(owner)?,
            };
            let records = list_implementation_evidence(
                root,
                ImplementationEvidenceListQuery {
                    task_id: args.task.or(owner_task),
                    design_version_id: args.design.or(owner_design),
                    work_unit_id: owner_work,
                    evidence_type: args.kind,
                },
            )?;
            if records.is_empty() {
                println!("no implementation evidence");
            }
            for record in records {
                let task = record
                    .task_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "-".to_string());
                let requirement = record.requirement_key.as_deref().unwrap_or("-");
                let detail = evidence_detail(&record);
                println!(
                    "{} [{}] task={} requirement={} {}",
                    record.id, record.evidence_type, task, requirement, detail
                );
            }
        }
    }
    Ok(())
}

fn parse_evidence_owner(owner: &str) -> Result<(Option<i64>, Option<i64>, Option<i64>)> {
    const HELP: &str = "evidence owner must be task:<id>, design_version:<id>, or work_unit:<id>";
    let (kind, id) = owner.split_once(':').ok_or_else(|| anyhow::anyhow!(HELP))?;
    let id = id.parse::<i64>().map_err(|_| anyhow::anyhow!(HELP))?;
    if id <= 0 {
        anyhow::bail!(HELP);
    }
    match kind {
        "task" => Ok((Some(id), None, None)),
        "design" | "design_version" => Ok((None, Some(id), None)),
        "work" | "work_unit" => Ok((None, None, Some(id))),
        _ => anyhow::bail!(HELP),
    }
}

pub(crate) fn handle_coverage(root: &Path, command: CoverageCommand) -> Result<()> {
    match command {
        CoverageCommand::Add(args) => {
            add_coverage_item(
                root,
                NewCoverageItem {
                    design_version_id: args.design,
                    requirement_key: &args.requirement,
                    review_scope_id: None,
                    work_unit_id: args.work_unit,
                    task_id: args.task,
                    requirement: &args.requirement_text,
                    runtime_boundary_evidence: args.runtime.as_deref(),
                    ux_boundary_evidence: args.ux.as_deref(),
                    lifecycle_boundary_evidence: args.lifecycle.as_deref(),
                    tests_or_gates: args.tests_or_gates.as_deref(),
                    missing_or_unverified: args.missing.as_deref(),
                    status: &args.status,
                },
            )?;
            println!("added coverage item");
        }
        CoverageCommand::List(args) => {
            let records = list_coverage_items_filtered(
                root,
                CoverageItemListFilter {
                    design_version_id: args.design.or(args.design_version),
                    status: args.status.as_deref(),
                    work_unit_id: args.work,
                },
            )?;
            if records.is_empty() {
                println!("no coverage items");
            }
            for record in records {
                let work_unit = record
                    .work_unit_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "-".to_string());
                let task = record
                    .task_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "-".to_string());
                let detail = record
                    .missing_or_unverified
                    .as_deref()
                    .or(record.tests_or_gates.as_deref())
                    .unwrap_or("-");
                println!(
                    "{} [{}] requirement={} work_unit={} task={} {}",
                    record.id, record.status, record.requirement_key, work_unit, task, detail
                );
            }
        }
    }
    Ok(())
}

fn print_decisions(records: Vec<agent_workbench::DecisionRecord>) {
    if records.is_empty() {
        println!("no decisions");
    }
    for record in records {
        let key = record.decision_key.as_deref().unwrap_or("-");
        println!(
            "{} [{}:{}] {}",
            record.id, key, record.status, record.decision
        );
    }
}

fn evidence_detail(record: &ImplementationEvidenceRecord) -> String {
    if let Some(commit_sha) = &record.commit_sha {
        return format!("commit={commit_sha}");
    }
    if let Some(file_path) = &record.file_path {
        let line = record
            .line_ref
            .as_ref()
            .map(|value| format!(":{value}"))
            .unwrap_or_default();
        return format!("file={file_path}{line}");
    }
    if let Some(symbol) = &record.symbol {
        return format!("symbol={symbol}");
    }
    if let Some(artifact_path) = &record.artifact_path {
        return format!("artifact={artifact_path}");
    }
    record
        .note
        .as_ref()
        .map(|note| format!("note={note}"))
        .unwrap_or_else(|| "detail=-".to_string())
}
