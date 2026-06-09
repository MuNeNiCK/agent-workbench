use std::path::Path;

use anyhow::Result;

use super::args::*;
use agent_workbench::*;

pub(crate) fn handle_review(root: &Path, command: ReviewCommand) -> Result<()> {
    match command {
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
        },
        ReviewCommand::Run { command } => match command {
            ReviewRunCommand::Add(args) => {
                let outcome = add_review_run(
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
                    },
                )?;
                println!("added review run");
                println!("review_run_id: {}", outcome.review_run_id);
                println!(
                    "review_agent_invocation_id: {}",
                    outcome.review_agent_invocation_id
                );
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
                    println!(
                        "{} [plan={} {}:{} clean={}] target={}",
                        record.id,
                        record
                            .review_plan_id
                            .map(|id| id.to_string())
                            .unwrap_or_else(|| "-".to_string()),
                        record.run_type,
                        record.status,
                        record.clean_run,
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
            allowed_inputs: "project ledger, active work unit, applicable rules, implementation context",
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
            let outcome = classify_finding(root, args.finding_id, &args.classification)?;
            println!("classified finding");
            println!("finding_id: {}", outcome.finding_id);
        }
        FindingCommand::List(args) => {
            let records = list_findings(root, args.status.as_deref())?;
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
            }
        }
        FindingCommand::Verify(args) => {
            let outcome = add_finding_verification(
                root,
                NewFindingVerification {
                    review_run_id: args.run,
                    finding_id: args.finding,
                    closure_id: args.closure,
                    result: &args.result,
                    notes: args.notes.as_deref(),
                },
            )?;
            println!("added finding verification");
            println!(
                "finding_verification_id: {}",
                outcome.finding_verification_id
            );
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
    }
    Ok(())
}

pub(crate) fn handle_acceptance(root: &Path, command: AcceptanceCommand) -> Result<()> {
    match command {
        AcceptanceCommand::Add(args) => {
            if args.design.is_some() || args.package.is_some() {
                let outcome = accept_design_exception(
                    root,
                    NewDesignExceptionAcceptance {
                        design_version_id: args.design,
                        design_package: args.package.as_deref(),
                        target: &args.target,
                        acceptance_type: &args.acceptance_type,
                        reason: &args.reason,
                    },
                )?;
                println!("accepted design exception");
                println!("acceptance_record_id: {}", outcome.acceptance_record_id);
                println!("authority_event_id: {}", outcome.authority_event_id);
                println!("target_type: {}", outcome.target_type);
                if let Some(design_requirement_id) = outcome.design_requirement_id {
                    println!("design_requirement_id: {design_requirement_id}");
                }
                if let Some(validation_gate_template_id) = outcome.validation_gate_template_id {
                    println!("validation_gate_template_id: {validation_gate_template_id}");
                }
                if let Some(coverage_item_id) = outcome.coverage_item_id {
                    println!("coverage_item_id: {coverage_item_id}");
                }
                if let Some(design_package_key) = outcome.design_package_key {
                    println!("design_package_key: {design_package_key}");
                }
                if let Some(design_file_path) = outcome.design_file_path {
                    println!("design_file_path: {design_file_path}");
                }
                if let Some(design_requirement_key) = outcome.design_requirement_key {
                    println!("design_requirement_key: {design_requirement_key}");
                }
            } else {
                let outcome = add_general_acceptance(
                    root,
                    NewGeneralAcceptance {
                        target: &args.target,
                        acceptance_type: &args.acceptance_type,
                        reason: &args.reason,
                    },
                )?;
                println!("accepted workflow exception");
                println!("acceptance_record_id: {}", outcome.acceptance_record_id);
                println!("authority_event_id: {}", outcome.authority_event_id);
                println!("target_type: {}", outcome.target_type);
            }
        }
    }
    Ok(())
}

pub(crate) fn handle_authority(root: &Path, command: AuthorityCommand) -> Result<()> {
    match command {
        AuthorityCommand::Add(args) => {
            let event_type = match args.authority_type.as_str() {
                "design" => "design_doc",
                other => other,
            };
            let summary = args.summary.unwrap_or_else(|| {
                format!(
                    "registered {} authority at {}",
                    args.authority_type, args.path
                )
            });
            let outcome = add_authority_event(
                root,
                NewAuthorityEvent {
                    event_type,
                    source: Some(&args.path),
                    summary: &summary,
                    scope: args.scope.as_deref(),
                    precedence: args.precedence,
                },
            )?;
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
                }
            }
            KptItemCommand::Convert(args) => match args.target_type.as_str() {
                "task" => {
                    let outcome = convert_kpt_item_to_task(
                        root,
                        KptItemTaskConversion {
                            kpt_item_id: args.item,
                            task_title: args.title.as_deref(),
                            details: args.details.as_deref(),
                            priority: &args.priority,
                            work_unit_id: args.work_unit,
                        },
                    )?;
                    println!("converted kpt item");
                    println!("kpt_item_conversion_id: {}", outcome.kpt_item_conversion_id);
                    println!("task_id: {}", outcome.task_id);
                }
                "review-policy" | "review_policy" => {
                    let review_type = args
                        .review_type
                        .as_deref()
                        .ok_or_else(|| anyhow::anyhow!("--review-type is required"))?;
                    let outcome = convert_kpt_item_to_review_policy(
                        root,
                        KptItemReviewPolicyConversion {
                            kpt_item_id: args.item,
                            name: args.name.as_deref().or(args.title.as_deref()),
                            review_type,
                            max_fresh_agents: args.max_fresh_agents,
                            max_resume_agents: args.max_resume_agents,
                            max_parallel_agents: args.max_parallel_agents,
                            required_consecutive_clean_fresh_runs: args.fresh_clean,
                            required_consecutive_clean_resume_runs: args.resume_clean,
                            stop_on_severity: &args.stop_on_severity,
                            allow_new_findings_in_resume: args.allow_new_findings_in_resume,
                            run_count_scope: &args.run_count_scope,
                            default_run_mode: &args.default_run_mode,
                            on_max_agents_exceeded: &args.on_max_agents_exceeded,
                        },
                    )?;
                    println!("converted kpt item");
                    println!("kpt_item_conversion_id: {}", outcome.kpt_item_conversion_id);
                    println!("review_policy_id: {}", outcome.review_policy_id);
                }
                "command-profile" | "command_profile" => {
                    let outcome = convert_kpt_item_to_command_profile(
                        root,
                        KptItemCommandProfileConversion {
                            kpt_item_id: args.item,
                            name: args.name.as_deref().or(args.title.as_deref()),
                            command: args.command.as_deref().or(args.details.as_deref()),
                            command_type: &args.command_type,
                            scope: args.scope.as_deref(),
                            status: &args.command_status,
                            stability: &args.stability,
                            timeout: args.timeout.as_deref(),
                            expected_result: args.expected_result.as_deref(),
                        },
                    )?;
                    println!("converted kpt item");
                    println!("kpt_item_conversion_id: {}", outcome.kpt_item_conversion_id);
                    println!("command_profile_id: {}", outcome.command_profile_id);
                }
                "decision" => {
                    let outcome = convert_kpt_item_to_decision(
                        root,
                        KptItemDecisionConversion {
                            kpt_item_id: args.item,
                            decision_key: args.decision_key.as_deref(),
                            topic: args.title.as_deref(),
                            decision: args.details.as_deref(),
                            rationale: args.rationale.as_deref(),
                            compatibility_impact: args.compatibility_impact.as_deref(),
                            authority_refs: args.authority_refs.as_deref(),
                        },
                    )?;
                    println!("converted kpt item");
                    println!("kpt_item_conversion_id: {}", outcome.kpt_item_conversion_id);
                    println!("decision_id: {}", outcome.decision_id);
                }
                "design-version" | "design_version" => {
                    let design_version_id = args
                        .design_version
                        .ok_or_else(|| anyhow::anyhow!("--design-version is required"))?;
                    let outcome = convert_kpt_item_to_design_version(
                        root,
                        KptItemDesignVersionConversion {
                            kpt_item_id: args.item,
                            design_version_id,
                        },
                    )?;
                    println!("converted kpt item");
                    println!("kpt_item_conversion_id: {}", outcome.kpt_item_conversion_id);
                    println!("design_version_id: {}", outcome.design_version_id);
                }
                other => anyhow::bail!("unsupported kpt item conversion target: {other}"),
            },
        },
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
