use super::*;

#[test]
fn public_commands_apply_and_inspect_an_explicit_plan() {
    let temp = tempfile::tempdir().unwrap();
    let (design, work, plan) = setup(temp.path(), "GATE-001");
    let imported = ok(
        temp.path(),
        &[
            "decomposition",
            "apply",
            &design,
            "--work",
            &work,
            "--plan",
            &plan,
        ],
    );
    assert!(imported.contains("tasks: 0"));
    assert!(imported.contains("status: ready"));
    assert!(imported.contains("review_owner_state: no_claim"));
    assert!(ok(temp.path(), &["task", "list", "--work-unit", &work]).contains("no tasks"));
    let context_ref = field(&imported, "review_context").to_string();
    accept_exact_plan_review(temp.path(), &design, &work, &context_ref);
    let applied = ok(
        temp.path(),
        &["decomposition", "apply", &design, "--work", &work],
    );
    assert!(applied.contains("tasks: 1"));
    assert!(applied.contains("checklist_items: 1"));
    assert!(applied.contains("phases: 1"));

    let shown = ok(
        temp.path(),
        &[
            "decomposition",
            "show",
            "--design-version",
            &design,
            "--work",
            &work,
        ],
    );
    assert_eq!(field(&shown, "requested_design_version"), design);
    assert_eq!(field(&shown, "requested_work"), work);
    assert!(shown.contains("status: applied"));
    assert!(shown.contains("item: opaque/item:一 slice=Slice/一"));
    assert!(shown.contains("next: agent-workbench gate implementation-ready --design-version"));
    let continuation = shown
        .lines()
        .find_map(|line| {
            line.strip_prefix("next: agent-workbench gate implementation-ready --design-version ")
        })
        .expect("applied decomposition must print the accepted readiness form");
    let readiness = aw(
        temp.path(),
        &[
            "gate",
            "implementation-ready",
            "--design-version",
            continuation,
        ],
    );
    assert!(
        readiness.status.success(),
        "printed decomposition continuation was rejected: {}",
        String::from_utf8_lossy(&readiness.stderr)
    );
    assert!(String::from_utf8_lossy(&readiness.stdout).contains("gate: implementation-ready"));
    let owner_readiness = ok(
        temp.path(),
        &["gate", "implementation-ready", &work, "--dry-run"],
    );
    assert!(owner_readiness.contains(&format!("selected_work_unit_id: {work}")));
    assert!(owner_readiness.contains(&format!("design_version_id: {design}")));
    let positional_design_ready = ok(temp.path(), &["gate", "design-ready", &design, "--dry-run"]);
    assert!(positional_design_ready.contains(&format!("design_version_id: {design}")));
    let public_design = temp.path().join("public-design.md");
    ok(
        temp.path(),
        &[
            "export",
            "design",
            &design,
            "--classification",
            "public",
            "--output",
            public_design.to_str().unwrap(),
        ],
    );
    assert!(
        fs::read_to_string(&public_design)
            .unwrap()
            .starts_with("classification: public\n")
    );
    let sensitive_plan = temp.path().join("sensitive-plan.md");
    ok(
        temp.path(),
        &[
            "export",
            "plan",
            &work,
            "--classification",
            "sensitive",
            "--output",
            sensitive_plan.to_str().unwrap(),
        ],
    );
    assert!(
        fs::read_to_string(&sensitive_plan)
            .unwrap()
            .starts_with("classification: sensitive\n")
    );

    let status = ok(temp.path(), &["status"]);
    assert!(!status.contains("owner_blocker_kind: decomposition_plan_incomplete"));
    assert!(status.contains(&format!("owner_next: continue work unit {work}")));
    let next = ok(temp.path(), &["next"]);
    assert!(!next.contains("owner_blocker_kind: decomposition_plan_incomplete"));
    assert!(next.contains("continue active work unit"));
    assert_eq!(field(&next, "work_unit_id"), work);

    let replay = ok(
        temp.path(),
        &[
            "decomposition",
            "apply",
            &design,
            "--work",
            &work,
            "--plan",
            &plan,
        ],
    );
    assert!(replay.contains("already_applied: true"));

    let root = temp.path();
    let source = fs::read_to_string(root.join(&plan)).unwrap();
    let invalid = source.replace(
        "    depends_on: []\n```",
        r#"    depends_on: []
reconciliation:
  predecessor: 1
  expected_current: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
  tasks:
    - { source: 1, disposition: retained, item: "opaque/item:一", effect: inferred }
  checklist: []
  gates: []
  phases: []
  dependencies: []
```"#,
    );
    let invalid_path = root.join(".agent-workbench/designs/black-box-plan/plans/invalid-effect.md");
    fs::write(&invalid_path, invalid).unwrap();
    let rejected = aw(
        root,
        &[
            "decomposition",
            "reconcile",
            &design,
            "--work",
            &work,
            "--plan",
            ".agent-workbench/designs/black-box-plan/plans/invalid-effect.md",
            "--closure",
            "1",
            "--expected-current",
            &"a".repeat(64),
        ],
    );
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("unknown variant `inferred`"));
    let tasks = ok(root, &["task", "list", "--work-unit", &work]);
    assert_eq!(tasks.matches("Public task").count(), 1);
    let phases = ok(root, &["phase", "list", "--work-unit", &work]);
    assert_eq!(phases.matches("Slice/一").count(), 1);

    let reconciliation = source.replace(
        "    depends_on: []\n```",
        &format!(
            r#"    depends_on: []
reconciliation:
  predecessor: {}
  expected_current: {}
  tasks: []
  checklist: []
  gates: []
  phases: []
  dependencies: []
```"#,
            field(&applied, "plan"),
            field(&shown, "current_identity")
        ),
    );
    let reconciliation_path =
        root.join(".agent-workbench/designs/black-box-plan/plans/no-closure.md");
    fs::write(&reconciliation_path, reconciliation).unwrap();
    let common = [
        "decomposition",
        "reconcile",
        &design,
        "--work",
        &work,
        "--plan",
        ".agent-workbench/designs/black-box-plan/plans/no-closure.md",
        "--closure",
        "999",
        "--expected-current",
        field(&shown, "current_identity"),
    ];
    let mut preview_args = common.to_vec();
    preview_args.push("--dry-run");
    let preview = aw(root, &preview_args);
    let mutation = aw(root, &common);
    assert!(!preview.status.success());
    assert!(!mutation.status.success());
    assert_eq!(preview.stderr, mutation.stderr);
    let tasks = ok(root, &["task", "list", "--work-unit", &work]);
    assert_eq!(tasks.matches("Public task").count(), 1);
    let phases = ok(root, &["phase", "list", "--work-unit", &work]);
    assert_eq!(phases.matches("Slice/一").count(), 1);

    let design_id = design.parse::<i64>().unwrap();
    let work_id = work.parse::<i64>().unwrap();
    let predecessor = show_decomposition_plan(
        root,
        DecompositionPlanQuery {
            design_version_id: design_id,
            work_unit_id: work_id,
        },
    )
    .unwrap();
    let task = list_tasks(
        root,
        TaskListQuery {
            status: None,
            work_unit_id: Some(work_id),
        },
    )
    .unwrap()[0]
        .id;
    let checklist = list_checklists(root, None).unwrap()[0].id;
    let checklist_item = list_checklist_items(
        root,
        ChecklistItemListQuery {
            checklist_id: Some(checklist),
            status: None,
        },
    )
    .unwrap()[0]
        .id;
    let gate = list_validation_gate_context(
        root,
        ValidationGateContextQuery {
            design_version_id: design_id,
            work_unit_id: Some(work_id),
        },
    )
    .unwrap()[0]
        .id;
    let phase = list_phases(root, work_id).unwrap()[0].id;
    let successor_path = ".agent-workbench/designs/black-box-plan/plans/successor.md";
    fs::write(
        root.join(successor_path),
        source
            .replacen("key: black-box-plan", "key: black-box-successor", 1)
            .replace(
                "    depends_on: []\n```",
                &format!(
                r#"    depends_on: []
reconciliation:
  predecessor: {}
  expected_current: {}
  tasks:
    - {{ source: {task}, disposition: retained, item: "opaque/item:一", effect: open }}
  checklist:
    - {{ source: {checklist_item}, disposition: retained, item: "opaque/item:一", boundary: observed, effect: open }}
  gates:
    - {{ source: {gate}, disposition: retained, item: "opaque/item:一", gate: GATE-001, boundary: retained-source, effect: open }}
  phases:
    - {{ source: {phase}, disposition: retained, slice: "Slice/一", effect: open }}
  dependencies: []
```"#,
                predecessor.id, predecessor.current_identity
                ),
            ),
    )
    .unwrap();
    let review_plan = add_review_plan(
        root,
        NewReviewPlan {
            work_unit_id: work_id,
            design_version_id: Some(design_id),
            review_type: "design_task_decomposition",
            required: true,
            stage: "close-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    let review_run = add_review_run(
        root,
        NewReviewRun {
            review_plan_id: review_plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some(&format!("work_unit:{work_id}")),
            prompt_deviations: None,
            result_summary: Some("the Plan requires a corrected successor"),
            new_findings_count: 1,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
            review_provenance: "self_recorded",
            review_provenance_ref: None,
        },
    )
    .unwrap();
    let finding = add_finding(
        root,
        NewFinding {
            review_run_id: review_run.review_run_id,
            finding_type: "design_task_gap",
            severity: "high",
            description: "publish the exact corrected successor",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(root, finding.finding_id, "valid").unwrap();
    let successor_file = Path::new(successor_path)
        .file_name()
        .unwrap()
        .to_string_lossy();
    let surface = format!(
        "design:edit:plans/{successor_file},transition:decomposition-plan-reconcile:{design_id}/{work_id}/{}",
        opaque_component(successor_path)
    );
    let closure = add_closure(
        root,
        NewClosure {
            finding_id: finding.finding_id,
            design_invariant: "preview and commit expose one public result",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some(&surface),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("publish the corrected successor atomically"),
            tests_or_gates: Some("public dry-run and commit"),
            verification_plan: Some("compare public outputs"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    begin_correction(root, closure.closure_id).unwrap();

    let previous_current_identity = predecessor.current_identity.clone();
    let predecessor = show_decomposition_plan(
        root,
        DecompositionPlanQuery {
            design_version_id: design_id,
            work_unit_id: work_id,
        },
    )
    .unwrap();
    let successor_document = fs::read_to_string(root.join(successor_path))
        .unwrap()
        .replace(&previous_current_identity, &predecessor.current_identity);
    fs::write(root.join(successor_path), successor_document).unwrap();

    let successor_resolution = ok(
        root,
        &[
            "decomposition",
            "show",
            "--design-version",
            &design,
            "--work",
            &work,
        ],
    );
    let stage = successor_resolution
        .lines()
        .find(|line| {
            line.starts_with("next: agent-workbench decomposition revise ")
                && line.contains(successor_path)
        })
        .unwrap()
        .strip_prefix("next: agent-workbench ")
        .unwrap();
    let staged = ok(root, &stage.split_whitespace().collect::<Vec<_>>());
    assert_eq!(field(&staged, "successor_status"), "ready");
    assert_eq!(field(&staged, "successor_projection_identity").len(), 64);
    let successor_review_context = field(&staged, "review_context").to_string();
    assert!(successor_review_context.contains(":projection="));
    accept_exact_plan_review(root, &design, &work, &successor_review_context);

    let ledger = root.join(".agent-workbench/ledger.sqlite");
    let ledger_before = fs::read(&ledger).unwrap();
    let preview = ok(
        root,
        &[
            "decomposition",
            "reconcile",
            &design,
            "--work",
            &work,
            "--plan",
            successor_path,
            "--closure",
            &closure.closure_id.to_string(),
            "--expected-current",
            &predecessor.current_identity,
            "--dry-run",
        ],
    );
    assert_eq!(fs::read(&ledger).unwrap(), ledger_before);
    assert!(preview.contains("projected_owned_effect: task"));
    assert!(preview.contains("projected_owned_effect: checklist"));
    assert!(preview.contains("projected_owned_effect: gate"));
    assert!(preview.contains("projected_owned_effect: phase"));
    assert!(preview.contains("projected_shared_binding: review"));
    let execute = field(&preview, "execute");
    let args = execute.split_whitespace().skip(1).collect::<Vec<_>>();
    let committed = ok(root, &args);
    let projection_lines = |output: &str| {
        let mut projection = Vec::new();
        for line in output.lines() {
            if line.starts_with("observed_")
                || line.starts_with("commit_current:")
                || line.starts_with("projected_")
                || line.starts_with("execute:")
            {
                projection.push(line.to_string());
                if line.starts_with("execute:") {
                    break;
                }
            }
        }
        projection
    };
    assert_eq!(projection_lines(&preview), projection_lines(&committed));
    let attempt = ready_closure(
        root,
        ClosureReady {
            closure_id: closure.closure_id,
            implementation_evidence: "the public reconciliation result was published",
            tests_or_gates: "public dry-run and commit",
            closed_by_commit: None,
        },
    )
    .unwrap();
    let verification = add_review_run_with_finding_result(
        root,
        NewReviewRun {
            review_plan_id: review_plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some(&attempt.context_ref),
            prompt_deviations: None,
            result_summary: Some("the public reconciliation result is durable"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: Some("independent-cli-reviewer"),
            external_agent_id: Some("independent-cli-reviewer"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:public-reconciliation-result"),
        },
        Some("verified"),
    )
    .unwrap();
    add_finding_verification(
        root,
        NewFindingVerification {
            review_run_id: verification.review_run_id,
            finding_id: finding.finding_id,
            closure_id: closure.closure_id,
            result: "verified",
            notes: Some("the exact public result is durable"),
        },
    )
    .unwrap();
    adjudicate_verification(
        root,
        verification.review_run_id,
        finding.finding_id,
        closure.closure_id,
        attempt.attempt_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "accept the exact public result verification",
            expected_current: "pending",
        },
    )
    .unwrap();
    let later_evidence = add_implementation_evidence(
        root,
        NewImplementationEvidence {
            task_id: None,
            design_version_id: Some(design_id),
            requirement_key: Some("REQ-001"),
            evidence_type: "artifact",
            commit_sha: None,
            file_path: None,
            line_ref: None,
            symbol: None,
            artifact_path: Some("artifacts/after-first-result.txt"),
            note: Some("added after the first public result"),
        },
    )
    .unwrap();
    let retry = ok(root, &args);
    assert_eq!(projection_lines(&committed), projection_lines(&retry));
    assert_eq!(field(&retry, "idempotent"), "true");
    assert!(!retry.contains(&format!(
        "projected_shared_binding: evidence id={}",
        later_evidence.implementation_evidence_id
    )));
    let shown = ok(
        root,
        &[
            "decomposition",
            "show",
            "--design-version",
            &design,
            "--work",
            &work,
        ],
    );
    assert!(shown.contains("owned_mapping: task"));
    assert!(shown.contains("owned_mapping: checklist"));
    assert!(shown.contains("owned_mapping: gate"));
    assert!(shown.contains("owned_mapping: phase"));
    assert!(
        !shown.contains("next: agent-workbench decomposition revise"),
        "{shown}"
    );
    assert!(
        !shown.contains(&format!("candidate: {successor_path}")),
        "{shown}"
    );
    assert!(!shown.contains(&format!("candidate: {plan}")), "{shown}");

    let current_plan = field(&shown, "plan").to_string();
    let current_identity = field(&shown, "current_identity").to_string();
    let task_mappings = list_tasks(
        root,
        TaskListQuery {
            status: None,
            work_unit_id: Some(work_id),
        },
    )
    .unwrap()
    .into_iter()
    .map(|record| {
        if record.id == task {
            format!(
                "    - {{ source: {}, disposition: retained, item: \"opaque/item:一\", effect: open }}",
                record.id
            )
        } else {
            format!(
                "    - {{ source: {}, disposition: retired, reason: \"The correction task is outside the successor Plan.\" }}",
                record.id
            )
        }
    })
    .collect::<Vec<_>>()
    .join("\n");
    let checklist_mappings = list_checklist_items(
        root,
        ChecklistItemListQuery {
            checklist_id: None,
            status: None,
        },
    )
    .unwrap()
    .into_iter()
    .map(|item| {
        if item.id == checklist_item {
            format!(
                "    - {{ source: {}, disposition: retained, item: \"opaque/item:一\", boundary: observed, effect: open }}",
                item.id
            )
        } else {
            format!(
                "    - {{ source: {}, disposition: retired, reason: \"The correction boundary is outside the successor Plan.\" }}",
                item.id
            )
        }
    })
    .collect::<Vec<_>>()
    .join("\n");
    let mut current_gate_ids = list_validation_gate_context(
        root,
        ValidationGateContextQuery {
            design_version_id: design_id,
            work_unit_id: Some(work_id),
        },
    )
    .unwrap()
    .into_iter()
    .map(|gate| gate.id)
    .collect::<Vec<_>>();
    current_gate_ids.push(gate);
    current_gate_ids.sort_unstable();
    current_gate_ids.dedup();
    let gate_mappings = current_gate_ids
        .into_iter()
        .map(|gate_id| {
            if gate_id == gate {
                format!(
                    "    - {{ source: {gate_id}, disposition: retained, item: \"opaque/item:一\", gate: GATE-001, boundary: retained-source, effect: open }}"
                )
            } else {
                format!(
                    "    - {{ source: {gate_id}, disposition: retired, reason: \"The correction gate is outside the successor Plan.\" }}"
                )
            }
        })
    .collect::<Vec<_>>()
    .join("\n");
    let phase_mappings = list_phases(root, work_id)
        .unwrap()
        .into_iter()
        .map(|record| {
            if record.id == phase {
                format!(
                    "    - {{ source: {}, disposition: retained, slice: \"Slice/一\", effect: open }}",
                    record.id
                )
            } else {
                format!(
                    "    - {{ source: {}, disposition: retired, reason: \"The correction phase is outside the successor Plan.\" }}",
                    record.id
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let changed_source = source
        .replacen("key: black-box-plan", "key: black-box-successor-next", 1)
        .replace(
            "    depends_on: []\n```",
            &format!(
                r#"    depends_on: []
reconciliation:
  predecessor: {current_plan}
  expected_current: {current_identity}
  tasks:
{task_mappings}
  checklist:
{checklist_mappings}
  gates:
{gate_mappings}
  phases:
{phase_mappings}
  dependencies: []
```"#
            ),
        );
    fs::write(root.join(successor_path), changed_source).unwrap();
    let changed = ok(
        root,
        &[
            "decomposition",
            "show",
            "--design-version",
            &design,
            "--work",
            &work,
        ],
    );
    let changed_candidate = changed
        .split(&format!("candidate: {successor_path}\n"))
        .nth(1)
        .expect("changed managed source must become a candidate");
    assert!(
        changed_candidate
            .lines()
            .take(3)
            .any(|line| line == "candidate_ready: true"),
        "{changed}"
    );
    let revise = changed
        .lines()
        .find(|line| {
            line.starts_with(&format!(
                "next: agent-workbench decomposition revise {current_plan} "
            )) && line.contains(successor_path)
        })
        .unwrap_or_else(|| panic!("{changed}"))
        .strip_prefix("next: agent-workbench ")
        .unwrap();
    let staged = ok(root, &revise.split_whitespace().collect::<Vec<_>>());
    assert_eq!(field(&staged, "status"), "ready");

    let continuation_plan = add_review_plan(
        root,
        NewReviewPlan {
            work_unit_id: work_id,
            design_version_id: Some(design_id),
            review_type: "design_task_decomposition",
            required: true,
            stage: "close-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    let continuation_run = add_review_run(
        root,
        NewReviewRun {
            review_plan_id: continuation_plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some(&format!("work_unit:{work_id}")),
            prompt_deviations: None,
            result_summary: Some("the changed Plan requires reconciliation"),
            new_findings_count: 1,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
            review_provenance: "self_recorded",
            review_provenance_ref: None,
        },
    )
    .unwrap();
    let continuation_finding = add_finding(
        root,
        NewFinding {
            review_run_id: continuation_run.review_run_id,
            finding_type: "design_task_gap",
            severity: "high",
            description: "publish the next exact successor",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(root, continuation_finding.finding_id, "valid").unwrap();
    let continuation_closure = add_closure(
        root,
        NewClosure {
            finding_id: continuation_finding.finding_id,
            design_invariant: "the reviewed successor remains executable",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some(&surface),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("reconcile the reviewed successor"),
            tests_or_gates: Some("public successor lifecycle"),
            verification_plan: Some("execute the printed preview"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    begin_correction(root, continuation_closure.closure_id).unwrap();
    let correction_next = ok(root, &["next"]);
    assert!(correction_next.contains(&format!(
        "owner_next: agent-workbench decomposition show --design-version {design} --work {work}"
    )));
    let pre_review = ok(
        root,
        &[
            "decomposition",
            "show",
            "--design-version",
            &design,
            "--work",
            &work,
        ],
    );
    let refresh = pre_review
        .lines()
        .find(|line| {
            line.starts_with("next: agent-workbench decomposition revise ")
                && line.contains(successor_path)
        })
        .unwrap_or_else(|| panic!("{pre_review}"))
        .strip_prefix("next: agent-workbench ")
        .unwrap();
    let restaged = ok(root, &refresh.split_whitespace().collect::<Vec<_>>());
    assert_eq!(field(&restaged, "status"), "ready");
    let add_review = restaged
        .lines()
        .find(|line| {
            line.starts_with("next: agent-workbench review plan add ")
                && line.contains("--type design_task_decomposition")
        })
        .unwrap_or_else(|| panic!("{restaged}"));
    assert!(add_review.ends_with(" --required"));
    let added_review = ok(
        root,
        &add_review
            .strip_prefix("next: agent-workbench ")
            .unwrap()
            .split_whitespace()
            .collect::<Vec<_>>(),
    );
    let review_plan_id = field(&added_review, "review_plan_id").parse().unwrap();
    let review_plans = ok(root, &["review", "plan", "list"]);
    assert!(review_plans.contains(&format!(
        "{review_plan_id} [design_task_decomposition:open required=true]"
    )));
    accept_exact_plan_review_for_plan(root, review_plan_id, field(&restaged, "review_context"));

    let reviewed = ok(
        root,
        &[
            "decomposition",
            "show",
            "--design-version",
            &design,
            "--work",
            &work,
        ],
    );
    let reconcile = reviewed
        .lines()
        .find(|line| {
            line.starts_with("next: agent-workbench decomposition reconcile ")
                && line.contains(successor_path)
                && line.contains(&format!("--closure {}", continuation_closure.closure_id))
                && line.ends_with(" --dry-run")
        })
        .unwrap_or_else(|| panic!("{reviewed}"))
        .strip_prefix("next: agent-workbench ")
        .unwrap();
    let preview = ok(root, &reconcile.split_whitespace().collect::<Vec<_>>());
    assert!(preview.contains("execute: agent-workbench decomposition reconcile "));
}
