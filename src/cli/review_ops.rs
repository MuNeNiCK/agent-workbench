use std::path::Path;

use anyhow::Result;

use super::args::*;
use agent_workbench::*;

pub(crate) fn handle_review(root: &Path, command: ReviewCommand) -> Result<()> {
    match command {
        ReviewCommand::Provenance { command } => match command {
            ReviewProvenanceCommand::Issue(args) => {
                let outcome = issue_review_provenance(
                    root,
                    ReviewProvenanceIssue {
                        reviewer_ref: &args.reviewer,
                        review_plan_id: args.plan,
                        target_context: &args.target,
                        provenance_kind: &args.kind,
                        purpose: &args.purpose,
                        source_reference: &args.reference,
                        idempotency_key: &args.idempotency_key,
                    },
                )?;
                println!("provenance_handle: {}", outcome.provenance_handle);
                println!("already_recorded: {}", outcome.already_recorded);
            }
        },
        ReviewCommand::Invocation { command } => {
            let outcome = match command {
                ReviewInvocationCommand::Request(args) => request_invocation(
                    root,
                    InvocationRequest {
                        review_plan_id: args.plan,
                        target_context: &args.target,
                        reviewer_ref: &args.reviewer,
                        provenance_handle: &args.provenance,
                        purpose: &args.purpose,
                        idempotency_key: &args.idempotency_key,
                        expected_plan_current: &args.expected_plan_current,
                    },
                )?,
                ReviewInvocationCommand::Start(args) => transition_invocation(
                    root,
                    InvocationTransitionRequest {
                        invocation_id: args.invocation_id,
                        expected_current: &args.expected_current,
                        idempotency_key: &args.idempotency_key,
                        outcome: InvocationTerminal::Start,
                    },
                )?,
                ReviewInvocationCommand::Complete(args) => {
                    let outcome = match (
                        args.claim.as_deref(),
                        args.verification_claim.as_deref(),
                        args.attempt,
                    ) {
                        (Some(claim), None, None) => InvocationTerminal::CompleteReview {
                            claim,
                            summary: &args.summary,
                        },
                        (None, Some(claim), Some(attempt)) => {
                            InvocationTerminal::CompleteVerification {
                                claim,
                                attempt,
                                summary: &args.summary,
                            }
                        }
                        _ => anyhow::bail!(
                            "complete requires either --claim, or --verification-claim with --attempt"
                        ),
                    };
                    transition_invocation(
                        root,
                        InvocationTransitionRequest {
                            invocation_id: args.invocation_id,
                            expected_current: &args.expected_current,
                            idempotency_key: &args.idempotency_key,
                            outcome,
                        },
                    )?
                }
                ReviewInvocationCommand::Fail(args) => transition_invocation(
                    root,
                    InvocationTransitionRequest {
                        invocation_id: args.invocation_id,
                        expected_current: &args.expected_current,
                        idempotency_key: &args.idempotency_key,
                        outcome: InvocationTerminal::Fail {
                            reason: &args.reason,
                        },
                    },
                )?,
                ReviewInvocationCommand::Cancel(args) => transition_invocation(
                    root,
                    InvocationTransitionRequest {
                        invocation_id: args.invocation_id,
                        expected_current: &args.expected_current,
                        idempotency_key: &args.idempotency_key,
                        outcome: InvocationTerminal::Cancel {
                            reason: &args.reason,
                        },
                    },
                )?,
            };
            println!("invocation_id: {}", outcome.invocation_id);
            println!("invocation_handle: {}", outcome.invocation_handle);
            println!("invocation_state: {}", outcome.state);
            if let Some(run) = outcome.review_run_id {
                println!("review_run_id: {run}");
            }
            println!("already_applied: {}", outcome.already_applied);
        }
        ReviewCommand::Result { command } => {
            let outcome = match command {
                ReviewResultCommand::Stage(args) => create_result_stage(
                    root,
                    CreateResultStageRequest {
                        invocation_id: args.invocation_id,
                        expected_current: &args.expected_current,
                        idempotency_key: &args.idempotency_key,
                    },
                )?,
                ReviewResultCommand::FindingAdd(args) => add_result_finding(
                    root,
                    AddResultFindingRequest {
                        stage_handle: &args.stage_handle,
                        finding_type: &args.finding_type,
                        severity: &args.severity,
                        description: &args.description,
                        requirement: args.requirement,
                        task: args.task,
                        expected_current: &args.expected_current,
                        idempotency_key: &args.idempotency_key,
                    },
                )?,
                ReviewResultCommand::Complete(args) => complete_result_stage(
                    root,
                    CompleteResultStageRequest {
                        stage_handle: &args.stage_handle,
                        expected_findings: args.expected_findings,
                        summary: &args.summary,
                        expected_current: &args.expected_current,
                        invocation_current: &args.invocation_current,
                        idempotency_key: &args.idempotency_key,
                    },
                )?,
                ReviewResultCommand::Cancel(args) => cancel_result_stage(
                    root,
                    CancelResultStageRequest {
                        stage_handle: &args.stage_handle,
                        reason: &args.reason,
                        expected_current: &args.expected_current,
                        idempotency_key: &args.idempotency_key,
                    },
                )?,
            };
            println!("stage_handle: {}", outcome.stage_handle);
            println!("version_handle: {}", outcome.version_handle);
            println!("stage_state: {}", outcome.state);
            if let Some(result) = outcome.result_handle {
                println!("result_handle: {result}");
            }
            println!("already_applied: {}", outcome.already_applied);
        }
        ReviewCommand::Correction { command } => match command {
            ReviewCorrectionCommand::Add(args) => {
                let outcome = correct_terminal_review(
                    root,
                    &args.decision,
                    &args.boundary,
                    AdjudicationInput {
                        decision: &args.outcome,
                        reason: &args.reason,
                        expected_current: &args.expected_boundary_current,
                    },
                )?;
                println!("decision_handle: {}", outcome.decision_handle);
            }
        },
        ReviewCommand::Adjudicate(args) => {
            let outcome = adjudicate_review(
                root,
                args.run_id,
                AdjudicationInput {
                    decision: &args.decision,
                    reason: &args.reason,
                    expected_current: &args.expected_current,
                },
            )?;
            println!("decision_handle: {}", outcome.decision_handle);
        }
        ReviewCommand::Scope { command } => match command {
            ReviewScopeCommand::Start(args) => {
                let defaults = review_role_defaults(&args.review_type);
                let outcome = start_review_scope(
                    root,
                    NewReviewScope {
                        name: &args.name,
                        review_type: &args.review_type,
                        scope: &args.scope,
                        allowed_inputs: Some(defaults.allowed_inputs),
                        forbidden_judgments: Some(defaults.forbidden_judgments),
                        expected_output_type: Some(defaults.expected_output_type),
                        exclusions: None,
                        prompt_template_ref: None,
                    },
                )?;
                println!("started review scope");
                println!("review_scope_id: {}", outcome.review_scope_id);
            }
            ReviewScopeCommand::List => {
                let records = list_review_scopes(root)?;
                if records.is_empty() {
                    println!("no review scopes");
                }
                for record in records {
                    println!(
                        "{} [{}:{} role={} streak={}] {}",
                        record.id,
                        record.review_type,
                        record.status,
                        record.agent_role,
                        record.no_findings_streak,
                        record.name
                    );
                }
            }
        },
        ReviewCommand::Policy { command } => match command {
            ReviewPolicyCommand::Add(args) => {
                let outcome = add_review_policy(
                    root,
                    NewReviewPolicy {
                        name: &args.name,
                        review_type: &args.review_type,
                        max_fresh_agents: args.max_fresh_agents,
                        max_resume_agents: args.max_resume_agents,
                        max_parallel_agents: args.max_parallel_agents,
                        required_consecutive_clean_fresh_runs: args.fresh_clean,
                        required_consecutive_clean_resume_runs: args.resume_clean,
                        stop_on_severity: &args.stop_on_severity,
                        allow_resume_review: true,
                        allow_fresh_review: true,
                        allow_new_findings_in_resume: args.allow_new_findings_in_resume,
                        on_max_agents_exceeded: &args.on_max_agents_exceeded,
                        run_count_scope: &args.run_count_scope,
                        default_run_mode: &args.default_run_mode,
                    },
                )?;
                println!("added review policy");
                println!("review_policy_id: {}", outcome.review_policy_id);
            }
            ReviewPolicyCommand::List => {
                let records = list_review_policies(root)?;
                if records.is_empty() {
                    println!("no review policies");
                }
                for record in records {
                    println!(
                        "{} [{} fresh_clean={} resume_clean={} max_fresh={} max_resume={} max_parallel={} resume_new={} count_scope={} default_mode={}] {}",
                        record.id,
                        record.review_type,
                        record.required_consecutive_clean_fresh_runs,
                        record.required_consecutive_clean_resume_runs,
                        record.max_fresh_agents,
                        record.max_resume_agents,
                        record.max_parallel_agents,
                        record.allow_new_findings_in_resume,
                        record.run_count_scope,
                        record.default_run_mode,
                        record.name
                    );
                }
            }
        },
        ReviewCommand::Plan { command } => match command {
            ReviewPlanCommand::Add(args) => {
                let outcome = add_review_plan(
                    root,
                    NewReviewPlan {
                        work_unit_id: args.work_unit,
                        design_version_id: args.design_version,
                        review_type: &args.review_type,
                        required: args.required,
                        stage: &args.stage,
                        scope: args.scope.as_deref(),
                        clean_condition: None,
                        stop_condition: None,
                        review_policy_id: args.policy,
                        review_scope_id: args.review_scope,
                    },
                )?;
                println!("added review plan");
                println!("review_plan_id: {}", outcome.review_plan_id);
                if let Some(review_policy_id) = outcome.review_policy_id {
                    println!("review_policy_id: {review_policy_id}");
                }
            }
            ReviewPlanCommand::List => {
                let records = list_review_plans(root)?;
                if records.is_empty() {
                    println!("no review plans");
                }
                for record in records {
                    println!(
                        "{} [{}:{} required={}] work_unit={} stage={}",
                        record.id,
                        record.review_type,
                        record.status,
                        record.required,
                        record.work_unit_id,
                        record.stage
                    );
                }
            }
            ReviewPlanCommand::Context(args) => {
                let targets = list_review_plan_targets(root, args.review_plan_id)?;
                println!("review_plan_id: {}", args.review_plan_id);
                if targets.is_empty() {
                    println!("no review plan targets");
                }
                for target in targets {
                    println!(
                        "target {} [{}] {}",
                        target.id,
                        target.target_type,
                        review_target_detail(&target)
                    );
                }
            }
            ReviewPlanCommand::Waive(args) => {
                waive_review_plan(
                    root,
                    ReviewPlanWaiver {
                        review_plan_id: args.review_plan_id,
                        reason: &args.reason,
                    },
                )?;
                println!("waived review plan");
            }
            ReviewPlanCommand::Supersede(args) => {
                let outcome = supersede_review_plan(
                    root,
                    ReviewPlanSupersession {
                        predecessor_plan_id: args.review_plan_id,
                        successor_plan_id: args.successor_plan_id,
                        authority_event_id: args.authority,
                        reason: &args.reason,
                    },
                )?;
                println!("superseded review plan");
                println!("predecessor_plan_id: {}", outcome.predecessor_plan_id);
                println!("successor_plan_id: {}", outcome.successor_plan_id);
            }
            ReviewPlanCommand::Target { command } => match command {
                ReviewPlanTargetCommand::Add(args) => {
                    let outcome = add_review_plan_target(
                        root,
                        NewReviewPlanTarget {
                            review_plan_id: args.plan,
                            target_type: &args.target_type,
                            design_version_id: args.design_version,
                            design_requirement_id: args.design_requirement,
                            task_id: args.task,
                            work_unit_id: args.work_unit,
                            phase_id: args.phase,
                            repository_snapshot_id: args.repository_snapshot,
                            file_path: args.file.as_deref(),
                            symbol: args.symbol.as_deref(),
                        },
                    )?;
                    println!("added review plan target");
                    println!("review_plan_target_id: {}", outcome.review_plan_target_id);
                }
            },
        },
        ReviewCommand::Run { command } => match command {
            ReviewRunCommand::Add(args) => {
                let outcome = add_review_run_with_finding_result(
                    root,
                    NewReviewRun {
                        review_plan_id: args.plan,
                        run_type: &args.run_type,
                        run_purpose: &args.purpose,
                        target_ref: args.target.as_deref(),
                        prompt_deviations: None,
                        result_summary: args.summary.as_deref(),
                        new_findings_count: args.new_findings,
                        carried_findings_checked: args.carried_findings,
                        clean_run: args.clean,
                        status: &args.status,
                        agent_label: args.agent_label.as_deref(),
                        external_agent_id: args.external_agent_id.as_deref(),
                        review_provenance: &args.provenance,
                        review_provenance_ref: args.provenance_ref.as_deref(),
                    },
                    args.finding_result.as_deref(),
                )?;
                println!("added review run");
                println!("review_run_id: {}", outcome.review_run_id);
                println!("review_plan_id: {}", outcome.review_plan_id);
                println!("plan_status: {}", outcome.plan_status);
            }
            ReviewRunCommand::List(args) => {
                let records = list_review_runs(root, args.plan)?;
                if records.is_empty() {
                    println!("no review runs");
                }
                for record in records {
                    let target = record.target_ref.as_deref().unwrap_or("-");
                    let finding_result = record.finding_fix_result.as_deref().unwrap_or("-");
                    println!(
                        "{} [plan={} {}:{} clean={} provenance={} finding_result={}] target={}",
                        record.id,
                        record
                            .review_plan_id
                            .map(|id| id.to_string())
                            .unwrap_or_else(|| "-".to_string()),
                        record.run_type,
                        record.status,
                        record.clean_run,
                        record.review_provenance,
                        finding_result,
                        target
                    );
                }
            }
        },
    }
    Ok(())
}

fn review_role_defaults(review_type: &str) -> ReviewRoleDefaults {
    match review_type {
        "design_review" => ReviewRoleDefaults {
            allowed_inputs: "design documents, requirements, accepted decisions, explicit non-goals, user-declared scope",
            forbidden_judgments: "do not implement fixes; do not rely on implementation behavior as proof; do not narrow user-declared scope",
            expected_output_type: "design_finding",
        },
        "design_task_decomposition" => ReviewRoleDefaults {
            allowed_inputs: "reviewed design documents, design review results, accepted decisions, existing task and checklist state",
            forbidden_judgments: "do not change the design; do not skip required design surfaces; do not mark tasks complete because they exist",
            expected_output_type: "design_task_decomposition",
        },
        "design_implementation_diff" => ReviewRoleDefaults {
            allowed_inputs: "design documents, implementation, tests, generated artifacts, public CLI/API/runtime/lifecycle surfaces, accepted decisions",
            forbidden_judgments: "do not redesign except for design_conflict; do not report language-style concerns unrelated to the design contract",
            expected_output_type: "design_implementation_drift",
        },
        "implementation_review" => ReviewRoleDefaults {
            allowed_inputs: "implementation code, tests, package conventions, language norms, security and error-handling paths",
            forbidden_judgments: "do not require behavior only because a design document says so; keep design coverage findings separate",
            expected_output_type: "implementation_finding",
        },
        _ => ReviewRoleDefaults {
            allowed_inputs: "managed project state, active work unit, applicable rules, implementation context",
            forbidden_judgments: "do not bypass active work-unit rules or review-specific gates",
            expected_output_type: "general",
        },
    }
}

struct ReviewRoleDefaults {
    allowed_inputs: &'static str,
    forbidden_judgments: &'static str,
    expected_output_type: &'static str,
}

pub(crate) fn handle_finding(root: &Path, command: FindingCommand) -> Result<()> {
    match command {
        FindingCommand::Reopen(args) => {
            let outcome = reopen_finding_epoch(
                root,
                args.finding_id,
                args.epoch,
                AdjudicationInput {
                    decision: "reopened",
                    reason: &args.reason,
                    expected_current: &args.expected_current,
                },
            )?;
            println!("decision_handle: {}", outcome.decision_handle);
        }
        FindingCommand::Recover(args) => {
            let outcome = recover_finding_design(
                root,
                FindingDesignRecovery {
                    finding_id: args.finding_id,
                    terminal_epoch: args.epoch,
                    evidence: &args.evidence,
                    authority_event_id: args.authority,
                    reason: &args.reason,
                    package_current: &args.package_current,
                    expected_current: &args.expected_current,
                    idempotency_key: &args.idempotency_key,
                },
            )?;
            println!("recovered terminal design finding");
            println!("recovery_handle: {}", outcome.recovery_handle);
            println!("finding_id: {}", outcome.finding_id);
            println!("terminal_epoch: {}", outcome.terminal_epoch);
            println!("source_closure_id: {}", outcome.source_closure_id);
            println!("source_session_id: {}", outcome.source_session_id);
            println!("source_attempt_id: {}", outcome.source_attempt_id);
            println!(
                "corrected_design_version_id: {}",
                outcome.corrected_design_version_id
            );
            println!("corrected_design_ref: {}", outcome.corrected_design_ref);
            println!("successor_closure_id: {}", outcome.successor_closure_id);
            println!("successor_session_id: {}", outcome.successor_session_id);
            println!("successor_attempt_id: {}", outcome.successor_attempt_id);
            println!("context_ref: {}", outcome.context_ref);
            println!("next: {}", outcome.next_action);
            println!("idempotent: {}", outcome.idempotent);
            println!("converged: {}", outcome.converged);
        }
        FindingCommand::Decide(args) => {
            let outcome = decide_finding(
                root,
                args.finding_id,
                AdjudicationInput {
                    decision: &args.decision,
                    reason: &args.reason,
                    expected_current: &args.expected_current,
                },
            )?;
            println!("decision_handle: {}", outcome.decision_handle);
        }
        FindingCommand::Add(args) => {
            let outcome = add_finding(
                root,
                NewFinding {
                    review_run_id: args.run,
                    finding_type: &args.finding_type,
                    severity: &args.severity,
                    description: &args.description,
                    design_requirement_id: args.design_requirement,
                    task_id: args.task,
                },
            )?;
            println!("added finding");
            println!("finding_id: {}", outcome.finding_id);
        }
        FindingCommand::Classify(args) => {
            match (
                args.classification.as_deref(),
                args.decision.as_deref(),
                args.reason.as_deref(),
                args.expected_current.as_deref(),
            ) {
                (Some(classification), None, None, None) => {
                    let outcome = classify_finding(root, args.finding_id, classification)?;
                    println!(
                        "classification_result: {}",
                        if outcome.existing {
                            "existing"
                        } else {
                            "classified"
                        }
                    );
                    println!("finding_id: {}", outcome.finding_id);
                    println!("classification: {}", outcome.classification);
                    println!("status: {}", outcome.status);
                }
                (None, Some(decision), Some(reason), Some(expected_current)) => {
                    let outcome = decide_finding(
                        root,
                        args.finding_id,
                        AdjudicationInput {
                            decision,
                            reason,
                            expected_current,
                        },
                    )?;
                    println!("decision_handle: {}", outcome.decision_handle);
                }
                _ => anyhow::bail!(
                    "finding classify requires exactly --classification or the complete --decision/--reason/--expected-current form"
                ),
            }
        }
        FindingCommand::List(args) => {
            let records = list_findings_filtered(
                root,
                FindingListFilter {
                    status: args.status.as_deref(),
                    review_run_id: args.run,
                },
            )?;
            let status = project_status(root)?;
            if records.is_empty() {
                println!("no findings");
            }
            for record in records {
                println!(
                    "{} [run={} {}:{} {}] {}",
                    record.id,
                    record.review_run_id,
                    record.finding_type,
                    record.severity,
                    record.status,
                    record.description
                );
                if let Some(handle) = record.current_decision_handle.as_deref() {
                    println!("current_decision_handle: {handle}");
                }
                if record.status == "closed"
                    && let (Some(epoch), Some(handle)) = (
                        record.terminal_epoch,
                        record.current_decision_handle.as_deref(),
                    )
                {
                    println!("terminal_epoch: {epoch}");
                    println!(
                        "reopen: agent-workbench finding reopen {} --epoch {} --reason <reason> --expected-current {}",
                        record.id, epoch, handle
                    );
                }
                if let Some(remediation) = status
                    .finding_remediations
                    .iter()
                    .find(|item| item.finding_id == record.id)
                {
                    println!("closure_id: {}", remediation.closure_id);
                    println!("affected_surfaces: {}", remediation.affected_surfaces);
                    println!("fix_plan: {}", remediation.fix_plan);
                    println!("design_invariant: {}", remediation.design_invariant);
                    println!("tests_or_gates: {}", remediation.tests_or_gates);
                    println!("verification_plan: {}", remediation.verification_plan);
                    println!("next: {}", remediation.next_action);
                }
                if let Some(correction) = status
                    .source_corrections
                    .iter()
                    .find(|item| item.finding_id == record.id)
                {
                    println!("closure_id: {}", correction.closure_id);
                    println!("affected_surfaces: {}", correction.affected_surfaces);
                    println!("fix_plan: {}", correction.fix_plan);
                    println!("design_invariant: {}", correction.design_invariant);
                    println!("tests_or_gates: {}", correction.tests_or_gates);
                    println!("verification_plan: {}", correction.verification_plan);
                    println!("next: {}", correction.next_action);
                }
            }
        }
        FindingCommand::Verify(args) => {
            let outcome = match args.attempt {
                Some(closure_attempt_id) => add_finding_verification_for_attempt(
                    root,
                    NewFindingVerificationForAttempt {
                        review_run_id: args.run,
                        finding_id: args.finding,
                        closure_id: args.closure,
                        closure_attempt_id,
                        result: &args.result,
                        notes: args.notes.as_deref(),
                    },
                )?,
                None => add_finding_verification(
                    root,
                    NewFindingVerification {
                        review_run_id: args.run,
                        finding_id: args.finding,
                        closure_id: args.closure,
                        result: &args.result,
                        notes: args.notes.as_deref(),
                    },
                )?,
            };
            println!("added finding verification");
            println!(
                "finding_verification_id: {}",
                outcome.finding_verification_id
            );
        }
        FindingCommand::AcceptOutOfScope(args) => {
            accept_finding_out_of_scope(
                root,
                FindingOutOfScope {
                    finding_id: args.finding_id,
                    reason: &args.reason,
                    authority_event_id: args.authority,
                },
            )?;
            println!("accepted finding out of scope");
        }
    }
    Ok(())
}

pub(crate) fn handle_verification(root: &Path, command: VerificationCommand) -> Result<()> {
    match command {
        VerificationCommand::Adjudicate(args) => {
            let outcome = adjudicate_verification(
                root,
                args.run,
                args.finding,
                args.closure,
                args.attempt,
                AdjudicationInput {
                    decision: &args.decision,
                    reason: &args.reason,
                    expected_current: &args.expected_current,
                },
            )?;
            println!("decision_handle: {}", outcome.decision_handle);
        }
    }
    Ok(())
}

pub(crate) fn handle_closure(root: &Path, command: ClosureCommand) -> Result<()> {
    match command {
        ClosureCommand::Add(args) => {
            let outcome = add_closure(
                root,
                NewClosure {
                    finding_id: args.finding,
                    design_invariant: &args.invariant,
                    design_citations: args.citations.as_deref(),
                    implementation_evidence: args.evidence.as_deref(),
                    affected_surfaces: args.surfaces.as_deref(),
                    same_invariant_search: args.search.as_deref(),
                    other_violations_found: args.other_violations.as_deref(),
                    fix_plan: args.fix_plan.as_deref(),
                    tests_or_gates: args.tests.as_deref(),
                    verification_plan: args.verification.as_deref(),
                    closed_by_commit: args.commit.as_deref(),
                },
            )?;
            println!("added closure");
            println!("closure_id: {}", outcome.closure_id);
        }
        ClosureCommand::CorrectionBegin(args) => {
            let outcome = begin_correction(root, args.closure_id)?;
            println!("began correction session");
            println!("closure_id: {}", outcome.closure_id);
            println!("correction_session_id: {}", outcome.session_id);
            println!("token_count: {}", outcome.token_count);
            println!("idempotent: {}", outcome.idempotent);
        }
        ClosureCommand::Transition { command } => match command {
            ClosureTransitionCommand::Apply(args) => {
                let outcome = apply_correction_transition(
                    root,
                    args.closure_id,
                    args.token,
                    args.authority,
                    args.evidence.as_deref(),
                )?;
                println!("applied correction transition");
                println!("closure_id: {}", outcome.closure_id);
                println!("token_ordinal: {}", outcome.token_ordinal);
                println!("application_id: {}", outcome.application_id);
                println!("result_ref: {}", outcome.result_ref);
                println!("idempotent: {}", outcome.idempotent);
            }
        },
        ClosureCommand::Ready(args) => {
            let outcome = ready_closure(
                root,
                ClosureReady {
                    closure_id: args.closure_id,
                    implementation_evidence: &args.evidence,
                    tests_or_gates: &args.tests,
                    closed_by_commit: args.commit.as_deref(),
                },
            )?;
            println!("closure ready for verification");
            println!("closure_id: {}", outcome.closure_id);
            println!("finding_id: {}", outcome.finding_id);
            println!("attempt_id: {}", outcome.attempt_id);
            println!("attempt_number: {}", outcome.attempt_number);
            println!("context_ref: {}", outcome.context_ref);
        }
        ClosureCommand::Supersede(args) => {
            let outcome = supersede_closure(
                root,
                ClosureSupersession {
                    closure_id: args.closure_id,
                    new_closure: NewClosure {
                        finding_id: 0,
                        design_invariant: &args.invariant,
                        design_citations: args.citations.as_deref(),
                        implementation_evidence: None,
                        affected_surfaces: Some(&args.surfaces),
                        same_invariant_search: None,
                        other_violations_found: None,
                        fix_plan: Some(&args.fix_plan),
                        tests_or_gates: Some(&args.tests),
                        verification_plan: Some(&args.verification),
                        closed_by_commit: None,
                    },
                    reason: &args.reason,
                    authority_event_id: args.authority,
                },
            )?;
            println!("superseded closure");
            println!("superseded_closure_id: {}", outcome.superseded_closure_id);
            println!("closure_id: {}", outcome.closure_id);
            println!("finding_id: {}", outcome.finding_id);
        }
    }
    Ok(())
}

pub(crate) fn handle_acceptance(root: &Path, command: AcceptanceCommand) -> Result<()> {
    match command {
        AcceptanceCommand::Add(args) => {
            if args.design.is_some() || args.package.is_some() {
                accept_design_exception(
                    root,
                    NewDesignExceptionAcceptance {
                        design_version_id: args.design,
                        design_package: args.package.as_deref(),
                        target: &args.target,
                        acceptance_type: &args.acceptance_type,
                        reason: &args.reason,
                        approval_authority_event_id: args.authority,
                    },
                )?;
                println!("accepted design exception");
            } else {
                add_general_acceptance(
                    root,
                    NewGeneralAcceptance {
                        target: &args.target,
                        acceptance_type: &args.acceptance_type,
                        reason: &args.reason,
                        approval_authority_event_id: args.authority,
                    },
                )?;
                println!("accepted workflow exception");
            }
        }
    }
    Ok(())
}

pub(crate) fn handle_kpt(root: &Path, command: KptCommand) -> Result<()> {
    match command {
        KptCommand::Start(args) => {
            let outcome = start_kpt_review(
                root,
                NewKptReview {
                    scope: args.scope.as_deref(),
                    summary: args.summary.as_deref(),
                    from: args.from.as_deref(),
                    period: args.period.as_deref(),
                },
            )?;
            println!("started kpt review");
            println!("kpt_review_id: {}", outcome.kpt_review_id);
            println!("generated_item_count: {}", outcome.generated_item_count);
        }
        KptCommand::List(args) => {
            let records = list_kpt_reviews(root, args.status.as_deref())?;
            if records.is_empty() {
                println!("no kpt reviews");
            }
            for record in records {
                let scope = record.scope.as_deref().unwrap_or("-");
                let summary = record.summary.as_deref().unwrap_or("");
                println!(
                    "{} [scope={} {}] {}",
                    record.id, scope, record.status, summary
                );
            }
        }
        KptCommand::Close(args) => {
            let outcome = close_kpt_review(root, args.kpt_review_id)?;
            println!("closed kpt review");
            println!("kpt_review_id: {}", outcome.kpt_review_id);
        }
        KptCommand::Item { command } => match command {
            KptItemCommand::Add(args) => {
                let outcome = add_kpt_item(
                    root,
                    NewKptItem {
                        kpt_review_id: args.review,
                        item_type: &args.item_type,
                        title: &args.title,
                        details: args.details.as_deref(),
                        severity: &args.severity,
                        proposed_action: args.proposed_action.as_deref(),
                    },
                )?;
                println!("added kpt item");
                println!("kpt_item_id: {}", outcome.kpt_item_id);
                println!("kpt_review_id: {}", outcome.kpt_review_id);
            }
            KptItemCommand::List(args) => {
                let records = list_kpt_items(root, args.review)?;
                if records.is_empty() {
                    println!("no kpt items");
                }
                for record in records {
                    let task = record
                        .linked_task_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    println!(
                        "{} [review={} {}:{} task={}] {}",
                        record.id,
                        record.kpt_review_id,
                        record.item_type,
                        record.status,
                        task,
                        record.title
                    );
                    println!("current: {}", record.current_handle);
                    println!("details: {}", record.details.as_deref().unwrap_or("-"));
                    println!(
                        "proposed_action: {}",
                        record.proposed_action.as_deref().unwrap_or("-")
                    );
                    if !record.legal_actions.is_empty() {
                        println!("legal_actions: {}", record.legal_actions.join(","));
                    }
                    if let Some(conversion) = &record.conversion {
                        print_kpt_conversion_record(conversion);
                    }
                    if let Some(receipt) = &record.dismissal {
                        print_kpt_dismissal_receipt(receipt);
                    }
                }
            }
            KptItemCommand::Convert(args) => {
                validate_kpt_conversion_operands(&args)?;
                let item_id = match args.item {
                    Some(item_id) => item_id,
                    None if args.target_type == KptConversionTargetArg::CommandProfile
                        && args.command_status.as_deref() == Some("fixed") =>
                    {
                        resolve_fixed_kpt_item(root, args.authority)?
                    }
                    None => anyhow::bail!("--item is required for this KPT conversion target"),
                };
                match args.target_type {
                    KptConversionTargetArg::Rule => {
                        let outcome = convert_kpt_item_to_rule(
                            root,
                            KptItemRuleConversion {
                                kpt_item_id: item_id,
                                scope: args.scope.as_deref(),
                                title: args.title.as_deref(),
                                body: args.details.as_deref(),
                            },
                        )?;
                        print_kpt_conversion_result(outcome.already_applied, &outcome.receipt);
                        println!("kpt_rule_id: {}", outcome.kpt_rule_id);
                    }
                    KptConversionTargetArg::Correction => {
                        let outcome = convert_kpt_item_to_correction(
                            root,
                            KptItemCorrectionConversion {
                                kpt_item_id: item_id,
                                scope: args.scope.as_deref(),
                                source_label: args.title.as_deref(),
                                expected_change: args.details.as_deref(),
                                severity: args.priority.as_deref().unwrap_or("medium"),
                            },
                        )?;
                        print_kpt_conversion_result(outcome.already_applied, &outcome.receipt);
                        println!("user_correction_id: {}", outcome.user_correction_id);
                    }
                    KptConversionTargetArg::Task => {
                        let outcome = convert_kpt_item_to_task(
                            root,
                            KptItemTaskConversion {
                                kpt_item_id: item_id,
                                task_title: args.title.as_deref(),
                                details: args.details.as_deref(),
                                priority: args.priority.as_deref().unwrap_or("medium"),
                                work_unit_id: args.work_unit,
                            },
                        )?;
                        print_kpt_conversion_result(outcome.already_applied, &outcome.receipt);
                        println!("task_id: {}", outcome.task_id);
                    }
                    KptConversionTargetArg::ReviewPolicy => {
                        let review_type = args
                            .review_type
                            .as_deref()
                            .ok_or_else(|| anyhow::anyhow!("--review-type is required"))?;
                        let outcome = convert_kpt_item_to_review_policy(
                            root,
                            KptItemReviewPolicyConversion {
                                kpt_item_id: item_id,
                                name: args.name.as_deref().or(args.title.as_deref()),
                                review_type,
                                max_fresh_agents: args.max_fresh_agents.unwrap_or(1),
                                max_resume_agents: args.max_resume_agents.unwrap_or(1),
                                max_parallel_agents: args.max_parallel_agents.unwrap_or(1),
                                required_consecutive_clean_fresh_runs: args
                                    .fresh_clean
                                    .unwrap_or(1),
                                required_consecutive_clean_resume_runs: args
                                    .resume_clean
                                    .unwrap_or(0),
                                stop_on_severity: args
                                    .stop_on_severity
                                    .as_deref()
                                    .unwrap_or("none"),
                                allow_new_findings_in_resume: args.allow_new_findings_in_resume,
                                run_count_scope: args
                                    .run_count_scope
                                    .as_deref()
                                    .unwrap_or("review_plan"),
                                default_run_mode: args
                                    .default_run_mode
                                    .as_deref()
                                    .unwrap_or("fresh"),
                                on_max_agents_exceeded: args
                                    .on_max_agents_exceeded
                                    .as_deref()
                                    .unwrap_or("block"),
                            },
                        )?;
                        print_kpt_conversion_result(outcome.already_applied, &outcome.receipt);
                        println!("review_policy_id: {}", outcome.review_policy_id);
                    }
                    KptConversionTargetArg::CommandProfile => {
                        let outcome = convert_kpt_item_to_command_profile(
                            root,
                            KptItemCommandProfileConversion {
                                kpt_item_id: item_id,
                                name: args.name.as_deref().or(args.title.as_deref()),
                                command: args.command.as_deref().or(args.details.as_deref()),
                                command_type: args.command_type.as_deref().unwrap_or("other"),
                                scope: args.scope.as_deref(),
                                status: args.command_status.as_deref().unwrap_or("candidate"),
                                stability: args.stability.as_deref().unwrap_or("context_dependent"),
                                timeout: args.timeout.as_deref(),
                                expected_result: args.expected_result.as_deref(),
                                authority_event_id: args.authority,
                            },
                        )?;
                        print_kpt_conversion_result(outcome.already_applied, &outcome.receipt);
                        println!("command_profile_id: {}", outcome.command_profile_id);
                    }
                    KptConversionTargetArg::Decision => {
                        let outcome = convert_kpt_item_to_decision(
                            root,
                            KptItemDecisionConversion {
                                kpt_item_id: item_id,
                                decision_key: args.decision_key.as_deref(),
                                topic: args.title.as_deref(),
                                decision: args.details.as_deref(),
                                rationale: args.rationale.as_deref(),
                                compatibility_impact: args.compatibility_impact.as_deref(),
                                authority_refs: args.authority_refs.as_deref(),
                            },
                        )?;
                        print_kpt_conversion_result(outcome.already_applied, &outcome.receipt);
                        println!("decision_id: {}", outcome.decision_id);
                    }
                    KptConversionTargetArg::DesignVersion => {
                        let design_version_id = args
                            .design_version
                            .ok_or_else(|| anyhow::anyhow!("--design-version is required"))?;
                        let outcome = convert_kpt_item_to_design_version(
                            root,
                            KptItemDesignVersionConversion {
                                kpt_item_id: item_id,
                                design_version_id,
                            },
                        )?;
                        print_kpt_conversion_result(outcome.already_applied, &outcome.receipt);
                        println!("design_version_id: {}", outcome.design_version_id);
                    }
                }
            }
            KptItemCommand::Dismiss(args) => {
                let outcome = dismiss_kpt_item(
                    root,
                    KptItemDismissalRequest {
                        kpt_item_id: args.item,
                        authority_event_id: args.authority,
                        reason: &args.reason,
                        expected_current: &args.expected_current,
                    },
                )?;
                match outcome {
                    KptItemDismissalOutcome::Dismissed(receipt) => {
                        println!("dismissed kpt item");
                        print_kpt_dismissal_receipt(&receipt);
                    }
                    KptItemDismissalOutcome::Existing(receipt) => {
                        println!("kpt item dismissal already exists");
                        print_kpt_dismissal_receipt(&receipt);
                    }
                    KptItemDismissalOutcome::InputInvalid { field, next } => {
                        anyhow::bail!("input_invalid: {field}\nnext: {next}")
                    }
                    KptItemDismissalOutcome::AuthorityInvalid {
                        authority_event_id,
                        required_scope,
                        next,
                    } => anyhow::bail!(
                        "authority_invalid: {authority_event_id}; required_scope: {required_scope}\nnext: {next}"
                    ),
                    KptItemDismissalOutcome::StateChanged {
                        expected,
                        observed,
                        next,
                    } => {
                        anyhow::bail!(
                            "state_changed: expected {expected}, observed {observed}\nnext: {next}"
                        )
                    }
                    KptItemDismissalOutcome::ItemTerminal {
                        state,
                        current,
                        next,
                    } => {
                        anyhow::bail!("item_terminal: {state}; current: {current}\nnext: {next}")
                    }
                }
            }
        },
    }
    Ok(())
}

fn print_kpt_dismissal_receipt(receipt: &KptItemDismissalReceipt) {
    println!("dismissal.item_revision: {}", receipt.item_revision);
    match &receipt.source {
        Some(source) => println!(
            "dismissal.source: exact({},{},{})",
            source.source_kind, source.source_identity, source.source_revision
        ),
        None => println!("dismissal.source: none"),
    }
    println!("dismissal.review_revision: {}", receipt.review_revision);
    println!("dismissal.review_status: {}", receipt.review_status);
    println!(
        "dismissal.authority_event_id: {}",
        receipt.authority_event_id
    );
    println!("dismissal.reason: {}", receipt.reason);
    println!("dismissal.predecessor: {}", receipt.predecessor_handle);
    println!("dismissal.decision: {}", receipt.decision_handle);
    println!("dismissal.current: {}", receipt.current_handle);
    println!("dismissal.replay: {}", receipt.replay_identity);
}

fn print_kpt_conversion_result(already_applied: bool, receipt: &KptItemConversionReceipt) {
    if already_applied {
        println!("kpt item conversion already exists");
    } else {
        println!("converted kpt item");
    }
    println!("kpt_item_conversion_id: {}", receipt.kpt_item_conversion_id);
    print_kpt_conversion_receipt(receipt);
}

fn print_kpt_conversion_record(record: &KptItemConversionRecord) {
    match &record.receipt {
        Some(receipt) => print_kpt_conversion_receipt(receipt),
        None => {
            println!(
                "conversion.target: {}({})",
                record.target.target_type(),
                record.target.target_id()
            );
            println!("conversion.receipt: legacy-absent");
        }
    }
}

fn print_kpt_conversion_receipt(receipt: &KptItemConversionReceipt) {
    println!(
        "conversion.target: {}({})",
        receipt.target.target_type(),
        receipt.target.target_id()
    );
    println!("conversion.item_revision: {}", receipt.item_revision);
    println!("conversion.predecessor: {}", receipt.predecessor_handle);
    println!("conversion.request: {}", receipt.request_identity);
    println!("conversion.receipt: {}", receipt.receipt_identity);
    println!("conversion.current: {}", receipt.current_handle);
}

fn resolve_fixed_kpt_item(root: &Path, authority: Option<i64>) -> Result<i64> {
    let reviews = list_kpt_reviews(root, Some("open"))?;
    if reviews.len() != 1 {
        let actions = reviews
            .iter()
            .map(|review| format!("agent-workbench kpt item list --review {}", review.id))
            .collect::<Vec<_>>();
        anyhow::bail!(
            "fixed command conversion without --item requires exactly one open KPT review; found {}{}",
            reviews.len(),
            actions
                .iter()
                .map(|action| format!("\nnext: {action}"))
                .collect::<String>()
        );
    }
    let items = list_kpt_items(root, Some(reviews[0].id))?
        .into_iter()
        .filter(|item| matches!(item.status.as_str(), "open" | "accepted"))
        .collect::<Vec<_>>();
    if items.len() != 1 {
        let authority = authority
            .map(|id| format!(" --authority {id}"))
            .unwrap_or_default();
        anyhow::bail!(
            "fixed command conversion without --item requires exactly one eligible item; found {}{}",
            items.len(),
            items
                .iter()
                .map(|item| format!(
                    "\nnext: agent-workbench kpt item convert --item {} --to command-profile --command-status fixed{}",
                    item.id, authority
                ))
                .collect::<String>()
        );
    }
    Ok(items[0].id)
}

fn validate_kpt_conversion_operands(args: &KptItemConvertArgs) -> Result<()> {
    let target = args.target_type.as_str();
    let present = [
        ("--title", args.title.is_some()),
        ("--details", args.details.is_some()),
        ("--priority", args.priority.is_some()),
        ("--work-unit", args.work_unit.is_some()),
        ("--name", args.name.is_some()),
        ("--command", args.command.is_some()),
        ("--command-type", args.command_type.is_some()),
        ("--scope", args.scope.is_some()),
        ("--command-status", args.command_status.is_some()),
        ("--stability", args.stability.is_some()),
        ("--timeout", args.timeout.is_some()),
        ("--expected-result", args.expected_result.is_some()),
        ("--authority", args.authority.is_some()),
        ("--review-type", args.review_type.is_some()),
        ("--fresh-clean", args.fresh_clean.is_some()),
        ("--resume-clean", args.resume_clean.is_some()),
        ("--max-fresh-agents", args.max_fresh_agents.is_some()),
        ("--max-resume-agents", args.max_resume_agents.is_some()),
        ("--max-parallel-agents", args.max_parallel_agents.is_some()),
        ("--stop-on-severity", args.stop_on_severity.is_some()),
        (
            "--allow-new-findings-in-resume",
            args.allow_new_findings_in_resume,
        ),
        ("--run-count-scope", args.run_count_scope.is_some()),
        ("--default-run-mode", args.default_run_mode.is_some()),
        (
            "--on-max-agents-exceeded",
            args.on_max_agents_exceeded.is_some(),
        ),
        ("--decision-key", args.decision_key.is_some()),
        ("--rationale", args.rationale.is_some()),
        (
            "--compatibility-impact",
            args.compatibility_impact.is_some(),
        ),
        ("--authority-refs", args.authority_refs.is_some()),
        ("--design-version", args.design_version.is_some()),
    ];
    let allowed: &[&str] = match target {
        "rule" => &["--title", "--details", "--scope"],
        "correction" => &["--title", "--details", "--scope", "--priority"],
        "task" => &["--title", "--details", "--priority", "--work-unit"],
        "command-profile" => &[
            "--title",
            "--details",
            "--name",
            "--command",
            "--command-type",
            "--scope",
            "--command-status",
            "--stability",
            "--timeout",
            "--expected-result",
            "--authority",
        ],
        "review-policy" => &[
            "--title",
            "--name",
            "--review-type",
            "--fresh-clean",
            "--resume-clean",
            "--max-fresh-agents",
            "--max-resume-agents",
            "--max-parallel-agents",
            "--stop-on-severity",
            "--allow-new-findings-in-resume",
            "--run-count-scope",
            "--default-run-mode",
            "--on-max-agents-exceeded",
        ],
        "decision" => &[
            "--title",
            "--details",
            "--decision-key",
            "--rationale",
            "--compatibility-impact",
            "--authority-refs",
        ],
        "design-version" => &["--design-version"],
        _ => unreachable!("KPT conversion target is a closed value enum"),
    };
    if let Some((operand, _)) = present
        .iter()
        .find(|(operand, is_present)| *is_present && !allowed.contains(operand))
    {
        anyhow::bail!("unexpected operand {operand} for KPT conversion target {target}");
    }
    if target == "command-profile"
        && args.authority.is_some()
        && args.command_status.as_deref().unwrap_or("candidate") != "fixed"
    {
        anyhow::bail!("--authority is valid only for a fixed command-profile conversion");
    }
    Ok(())
}

fn review_target_detail(target: &agent_workbench::ReviewPlanTargetRecord) -> String {
    if let Some(id) = target.design_version_id {
        return format!("design_version_id={id}");
    }
    if let Some(id) = target.design_requirement_id {
        return format!("design_requirement_id={id}");
    }
    if let Some(id) = target.task_id {
        return format!("task_id={id}");
    }
    if let Some(id) = target.work_unit_id {
        return format!("work_unit_id={id}");
    }
    if let Some(id) = target.phase_id {
        return format!("phase_id={id}");
    }
    if let Some(id) = target.repository_snapshot_id {
        return format!("repository_snapshot_id={id}");
    }
    if let Some(path) = &target.file_path {
        return format!("file_path={path}");
    }
    if let Some(symbol) = &target.symbol {
        return format!("symbol={symbol}");
    }
    "-".to_string()
}
