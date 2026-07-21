use super::*;

#[test]
fn public_reconciliation_atomically_replaces_the_current_plan_and_retries_exactly() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "reconcile decomposition", None).unwrap();
    let package = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "reconcile-plan",
            title: "Reconcile Plan",
        },
    )
    .unwrap();
    fs::write(
        package.package_path.join("requirements/README.md"),
        requirement_doc("REQ-001", "Preserve public behavior", "high"),
    )
    .unwrap();
    fs::write(
        package.package_path.join("validation/gates.md"),
        validation_gate_doc("GATE-001"),
    )
    .unwrap();
    let imported = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &package.package_path,
            status: "draft",
        },
    )
    .unwrap();
    approve_design_version(
        temp.path(),
        DesignVersionApproval {
            design_version_id: imported.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    let plans = package.package_path.join("plans");
    fs::create_dir_all(&plans).unwrap();
    let predecessor_path = plans.join("predecessor.md");
    fs::write(
        &predecessor_path,
        format!(
            r#"# Predecessor

```yaml agent-workbench
type: decomposition_plan
format: 1
key: predecessor
design_fingerprint: {}
items:
  - key: old-item
    requirements: [REQ-001]
    title: Preserve behavior
    details: Preserve the observable behavior.
    completion:
      outcome: The behavior remains observable.
      observation: Exercise the public operation.
      evidence_owner: work:{}
      evidence_kind: validation
      gates: [GATE-001]
    checklist:
      - key: old-boundary
        condition: The outcome is observed.
        evidence_kind: validation
        gates: [GATE-001]
    slice: old-slice
  - key: retired-item
    requirements: [REQ-001]
    title: Retire obsolete behavior split
    details: This duplicate split is no longer an executable task.
    completion:
      outcome: The obsolete split is retired without deleting its history.
      observation: Current task queries omit the retired split.
      evidence_owner: work:{}
      evidence_kind: validation
      gates: [GATE-001]
    checklist:
      - key: retired-boundary
        condition: The obsolete split is terminal historical state.
        evidence_kind: validation
        gates: [GATE-001]
    slice: second-slice
slices:
  - key: old-slice
    title: Old Slice
    order: 1
    depends_on: []
  - key: second-slice
    title: Second Slice
    order: 2
    depends_on: [old-slice]
```
"#,
            imported.content_hash, work.work_unit_id, work.work_unit_id
        ),
    )
    .unwrap();
    let (staged_predecessor, predecessor) = import_review_and_apply(
        temp.path(),
        imported.design_version_id,
        work.work_unit_id,
        &predecessor_path,
    );
    assert!(!staged_predecessor.applied);
    let predecessor_tasks = list_tasks(
        temp.path(),
        TaskListQuery {
            status: None,
            work_unit_id: Some(work.work_unit_id),
        },
    )
    .unwrap();
    assert_eq!(predecessor_tasks.len(), 2);
    let task = predecessor_tasks[0].id;
    let retired_task = predecessor_tasks[1].id;
    let checklist = list_checklists(temp.path(), None).unwrap()[0].id;
    let predecessor_items = list_checklist_items(
        temp.path(),
        ChecklistItemListQuery {
            checklist_id: Some(checklist),
            status: None,
        },
    )
    .unwrap();
    assert_eq!(predecessor_items.len(), 2);
    let item = predecessor_items[0].id;
    let retired_item = predecessor_items[1].id;
    let predecessor_gates = list_validation_gate_context(
        temp.path(),
        ValidationGateContextQuery {
            design_version_id: imported.design_version_id,
            work_unit_id: Some(work.work_unit_id),
        },
    )
    .unwrap();
    assert_eq!(predecessor_gates.len(), 2);
    let gate = predecessor_gates[0].id;
    let retired_gate = predecessor_gates[1].id;
    let predecessor_phases = list_phases(temp.path(), work.work_unit_id).unwrap();
    let phase = predecessor_phases
        .iter()
        .find(|candidate| candidate.key == "old-slice")
        .unwrap()
        .id;
    let second_phase = predecessor_phases
        .iter()
        .find(|candidate| candidate.key == "second-slice")
        .unwrap()
        .id;
    let conn = crate::db::open_existing_project(temp.path()).unwrap();
    let dependency: i64 = conn
        .query_row(
            "select id from work_phase_dependencies where from_phase_id=?1 and to_phase_id=?2",
            rusqlite::params![phase, second_phase],
            |row| row.get(0),
        )
        .unwrap();
    conn.execute(
        "update tasks set status='closed',closed_by_commit='terminal-source' where id=?1",
        [task],
    )
    .unwrap();
    conn.execute(
        "update checklist_items set status='closed' where id=?1",
        [item],
    )
    .unwrap();
    conn.execute(
        "update validation_gates set status='closed' where id=?1",
        [gate],
    )
    .unwrap();
    conn.execute(
        "update work_phases set status='closed',closed_at='terminal-source-at',close_summary='terminal source summary' where id=?1",
        [phase],
    )
    .unwrap();
    conn.execute(
        "update phase_epochs set state='closed',terminal_at='terminal-source-at',terminal_summary='terminal source summary' where id=?1",
        [phase],
    )
    .unwrap();
    conn.execute(
        "update work_phase_dependencies set status='satisfied',evidence_ref='terminal-dependency-evidence',resolved_at='terminal-source-at' where id=?1",
        [dependency],
    )
    .unwrap();
    conn.execute(
        "update phase_epoch_dependencies set state='satisfied',evidence_ref='terminal-dependency-evidence',terminal_at='terminal-source-at' where id=?1",
        [dependency],
    )
    .unwrap();
    drop(conn);
    let evidence = add_implementation_evidence(
        temp.path(),
        NewImplementationEvidence {
            task_id: Some(task),
            design_version_id: Some(imported.design_version_id),
            requirement_key: Some("REQ-001"),
            evidence_type: "file",
            commit_sha: None,
            file_path: Some("src/public_behavior.rs"),
            line_ref: None,
            symbol: None,
            artifact_path: None,
            note: Some("observable predecessor behavior"),
        },
    )
    .unwrap();
    let coverage = add_coverage_item(
        temp.path(),
        NewCoverageItem {
            design_version_id: imported.design_version_id,
            requirement_key: "REQ-001",
            review_scope_id: None,
            work_unit_id: Some(work.work_unit_id),
            task_id: Some(task),
            requirement: "Public behavior is covered",
            runtime_boundary_evidence: Some("public operation observed"),
            ux_boundary_evidence: None,
            lifecycle_boundary_evidence: None,
            tests_or_gates: Some("public behavior test"),
            missing_or_unverified: None,
            status: "covered",
        },
    )
    .unwrap();
    let current = show_decomposition_plan(
        temp.path(),
        DecompositionPlanQuery {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
        },
    )
    .unwrap();
    assert!(
        current
            .shared_bindings
            .iter()
            .any(|binding| binding.kind == "evidence"
                && binding.id == evidence.implementation_evidence_id)
    );
    assert!(
        current
            .shared_bindings
            .iter()
            .any(|binding| binding.kind == "coverage" && binding.id == coverage.coverage_item_id)
    );

    let successor_path = plans.join("successor.md");
    fs::write(
        &successor_path,
        format!(
            r#"# Successor

```yaml agent-workbench
type: decomposition_plan
format: 1
key: successor
design_fingerprint: {}
items:
  - key: current-item
    requirements: [REQ-001]
    title: Preserve behavior
    details: Preserve the same observable behavior through a replaceable implementation.
    completion:
      outcome: The behavior remains observable.
      observation: Exercise the public operation.
      evidence_owner: work:{}
      evidence_kind: validation
      gates: [GATE-001]
    checklist:
      - key: current-boundary
        condition: The outcome is observed.
        evidence_kind: validation
        gates: [GATE-001]
    slice: old-slice
  - key: replacement-item
    requirements: [REQ-001]
    title: Replacement behavior
    details: Publish a new successor endpoint without inheriting retired state.
    completion:
      outcome: The replacement behavior is observable.
      observation: Exercise the replacement operation.
      evidence_owner: work:{}
      evidence_kind: validation
      gates: [GATE-001]
    checklist:
      - key: replacement-boundary
        condition: The replacement outcome is observed.
        evidence_kind: validation
        gates: [GATE-001]
    slice: second-slice
slices:
  - key: old-slice
    title: Current Slice
    order: 1
    depends_on: []
  - key: second-slice
    title: Second Slice
    order: 2
    depends_on: [old-slice]
reconciliation:
  predecessor: {}
  expected_current: {}
  tasks:
    - source: {}
      disposition: retained
      item: current-item
      effect: open
    - source: {}
      disposition: retired
      reason: The duplicate task split is no longer part of the executable Plan.
  checklist:
    - source: {}
      disposition: retained
      item: current-item
      boundary: current-boundary
      effect: open
    - source: {}
      disposition: retired
      reason: The duplicate checklist boundary is historical only.
  gates:
    - source: {}
      disposition: retained
      item: current-item
      gate: GATE-001
      boundary: retained-source
      effect: open
    - source: {}
      disposition: retired
      reason: The duplicate gate selection is historical only.
  phases:
    - source: {}
      disposition: retained
      slice: old-slice
      effect: open
    - source: {}
      disposition: retained
      slice: second-slice
      effect: open
  dependencies:
    - source: {}
      disposition: retained
      from: old-slice
      to: second-slice
      effect: open
```
"#,
            imported.content_hash,
            work.work_unit_id,
            work.work_unit_id,
            predecessor.plan_id,
            current.current_identity,
            task,
            retired_task,
            item,
            retired_item,
            gate,
            retired_gate,
            phase,
            second_phase,
            dependency
        ),
    )
    .unwrap();

    let review_plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(imported.design_version_id),
            review_type: "design_task_decomposition",
            required: true,
            stage: "implementation-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    let blocked_context = resolve_decomposition_plan(
        temp.path(),
        DecompositionPlanQuery {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
        },
    )
    .unwrap()
    .review_owner
    .unwrap()
    .context_ref;
    let run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: review_plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some(&blocked_context),
            prompt_deviations: None,
            result_summary: Some("the decomposition requires reconciliation"),
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
        temp.path(),
        NewFinding {
            review_run_id: run.review_run_id,
            finding_type: "design_task_gap",
            severity: "high",
            description: "replace the current decomposition without losing its public behavior",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), finding.finding_id, "valid").unwrap();
    let successor_project_path = successor_path
        .strip_prefix(temp.path())
        .unwrap()
        .to_string_lossy();
    let surface = format!(
        "design:edit:plans/successor.md,transition:decomposition-plan-reconcile:{}/{}/{}",
        imported.design_version_id,
        work.work_unit_id,
        crate::review::encode_opaque_component(&successor_project_path)
    );
    let closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: finding.finding_id,
            design_invariant: "the public decomposition behavior survives replacement",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some(&surface),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("atomically reconcile the current Plan"),
            tests_or_gates: Some("public Plan reconciliation"),
            verification_plan: Some("review the observable successor behavior"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    begin_correction(temp.path(), closure.closure_id).unwrap();

    let previous_current_identity = current.current_identity.clone();
    let current = show_decomposition_plan(
        temp.path(),
        DecompositionPlanQuery {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
        },
    )
    .unwrap();
    let successor_document = fs::read_to_string(&successor_path)
        .unwrap()
        .replace(&previous_current_identity, &current.current_identity);
    fs::write(&successor_path, &successor_document).unwrap();
    let substituted_path = plans.join("substituted-path.md");
    fs::write(&substituted_path, &successor_document).unwrap();
    let substituted = preview_decomposition_reconciliation(
        temp.path(),
        DecompositionReconciliationApplication {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
            plan_path: &substituted_path,
            closure_id: closure.closure_id,
            expected_current: &current.current_identity,
        },
    )
    .unwrap_err();
    assert!(
        substituted
            .to_string()
            .contains("does not authorize this Decomposition Plan path"),
        "{substituted:#}"
    );
    let omitted_document = successor_document.replace(
            &format!(
                "  gates:\n    - source: {gate}\n      disposition: retained\n      item: current-item\n      gate: GATE-001\n      boundary: retained-source\n      effect: open\n    - source: {retired_gate}\n      disposition: retired\n      reason: The duplicate gate selection is historical only.\n"
            ),
            "  gates: []\n",
        );
    fs::write(&successor_path, omitted_document).unwrap();
    let omitted = preview_decomposition_reconciliation(
        temp.path(),
        DecompositionReconciliationApplication {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
            plan_path: &successor_path,
            closure_id: closure.closure_id,
            expected_current: &current.current_identity,
        },
    )
    .unwrap_err();
    assert!(
        omitted.to_string().contains("exact predecessor domain"),
        "{omitted:#}"
    );
    fs::write(&successor_path, &successor_document).unwrap();

    let mismatched = successor_document
        .replace(
            "slices:\n  - key: old-slice",
            r#"  - key: other-item
    requirements: [REQ-001]
    title: Other behavior
    details: A separate successor endpoint.
    completion:
      outcome: The separate behavior is observable.
      observation: Exercise the separate operation.
      evidence_owner: work:1
      evidence_kind: validation
      gates: [GATE-001]
    checklist:
      - key: other-boundary
        condition: The separate outcome is observed.
        evidence_kind: validation
        gates: [GATE-001]
    slice: old-slice
slices:
  - key: old-slice"#,
        )
        .replace(
            &format!(
                "  checklist:\n    - source: {item}\n      disposition: retained\n      item: current-item\n      boundary: current-boundary\n      effect: open\n"
            ),
            &format!(
                "  checklist:\n    - source: {item}\n      disposition: retained\n      item: other-item\n      boundary: other-boundary\n      effect: open\n"
            ),
        );
    fs::write(&successor_path, mismatched).unwrap();
    let mismatched = preview_decomposition_reconciliation(
        temp.path(),
        DecompositionReconciliationApplication {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
            plan_path: &successor_path,
            closure_id: closure.closure_id,
            expected_current: &current.current_identity,
        },
    )
    .unwrap_err();
    assert!(
        mismatched.to_string().contains("must target the same item"),
        "{mismatched:#}"
    );
    fs::write(&successor_path, &successor_document).unwrap();
    assert_eq!(
        show_decomposition_plan(
            temp.path(),
            DecompositionPlanQuery {
                design_version_id: imported.design_version_id,
                work_unit_id: work.work_unit_id,
            },
        )
        .unwrap(),
        current
    );

    let rejected = preview_decomposition_reconciliation(
        temp.path(),
        DecompositionReconciliationApplication {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
            plan_path: &successor_path,
            closure_id: closure.closure_id,
            expected_current: &"f".repeat(64),
        },
    )
    .unwrap_err();
    assert!(
        rejected
            .to_string()
            .contains("command and Plan predecessor identities disagree")
    );
    assert_eq!(
        show_decomposition_plan(
            temp.path(),
            DecompositionPlanQuery {
                design_version_id: imported.design_version_id,
                work_unit_id: work.work_unit_id,
            },
        )
        .unwrap(),
        current
    );

    let qualified_document = successor_document
        .replace(
            "Preserve the same observable behavior through a replaceable implementation.",
            "Preserve the observable behavior.",
        )
        .replacen(
            &format!(
                "    - source: {task}\n      disposition: retained\n      item: current-item\n      effect: open"
            ),
            &format!(
                "    - source: {task}\n      disposition: retained\n      item: current-item\n      effect: preserve"
            ),
            1,
        )
        .replacen(
            &format!(
                "    - source: {gate}\n      disposition: retained\n      item: current-item\n      gate: GATE-001\n      boundary: retained-source\n      effect: open"
            ),
            &format!(
                "    - source: {gate}\n      disposition: retained\n      item: current-item\n      gate: GATE-001\n      boundary: retained-source\n      effect: preserve"
            ),
            1,
        );
    fs::write(&successor_path, qualified_document).unwrap();
    let qualified_preview = preview_decomposition_reconciliation(
        temp.path(),
        DecompositionReconciliationApplication {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
            plan_path: &successor_path,
            closure_id: closure.closure_id,
            expected_current: &current.current_identity,
        },
    )
    .unwrap();
    assert!(
        qualified_preview
            .projection
            .shared_bindings
            .iter()
            .any(|binding| {
                binding.kind == "evidence"
                    && binding.id == evidence.implementation_evidence_id
                    && binding.disposition == "preserved"
                    && binding.qualification == "preserved_qualified"
            })
    );
    assert!(
        qualified_preview
            .projection
            .shared_bindings
            .iter()
            .any(|binding| {
                binding.kind == "coverage"
                    && binding.id == coverage.coverage_item_id
                    && binding.disposition == "preserved"
                    && binding.qualification == "preserved_current"
            })
    );
    fs::write(&successor_path, &successor_document).unwrap();

    fs::write(
        &successor_path,
        successor_document.replace("      effect: open\n", ""),
    )
    .unwrap();
    let incompatible = preview_decomposition_reconciliation(
        temp.path(),
        DecompositionReconciliationApplication {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
            plan_path: &successor_path,
            closure_id: closure.closure_id,
            expected_current: &current.current_identity,
        },
    )
    .unwrap_err();
    assert!(
        incompatible
            .to_string()
            .contains("preserve task effect differs"),
        "{incompatible:#}"
    );
    fs::write(&successor_path, &successor_document).unwrap();
    assert_eq!(
        show_decomposition_plan(
            temp.path(),
            DecompositionPlanQuery {
                design_version_id: imported.design_version_id,
                work_unit_id: work.work_unit_id,
            },
        )
        .unwrap(),
        current
    );

    fs::write(
        &successor_path,
        fs::read_to_string(&predecessor_path).unwrap(),
    )
    .unwrap();
    let ordinary_preview = preview_decomposition_reconciliation(
        temp.path(),
        DecompositionReconciliationApplication {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
            plan_path: &successor_path,
            closure_id: closure.closure_id,
            expected_current: &current.current_identity,
        },
    )
    .unwrap_err();
    assert!(
        ordinary_preview
            .to_string()
            .contains("reconciliation metadata is required")
    );
    fs::write(&successor_path, &successor_document).unwrap();
    let staged = revise_decomposition_plan(
        temp.path(),
        DecompositionRevise {
            plan_id: current.id,
            plan_path: &successor_path,
            draft: false,
            expected_current: &current.current_identity,
            idempotency_key: "reviewed-ready-successor",
        },
    )
    .unwrap();
    assert_eq!(staged.plan.status, "ready");
    assert_eq!(staged.plan.predecessor_id, Some(current.id));
    let staged_conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let staged_ingress: String = staged_conn
        .query_row(
            "select source_identity from decomposition_plan_ingress_identities where plan_id=?1",
            [staged.plan.id],
            |row| row.get(0),
        )
        .unwrap();
    let staged_bytes = fs::read(&successor_path).unwrap();
    assert_eq!(staged_bytes, successor_document.as_bytes());
    let mut staged_hasher = <sha2::Sha256 as sha2::Digest>::new();
    sha2::Digest::update(
        &mut staged_hasher,
        b"agent-workbench/decomposition-plan-source/v1\0",
    );
    sha2::Digest::update(&mut staged_hasher, &staged_bytes);
    assert_eq!(
        staged_ingress,
        format!("{:x}", sha2::Digest::finalize(staged_hasher))
    );
    drop(staged_conn);
    accept_current_plan_review(temp.path(), imported.design_version_id, work.work_unit_id);
    let ledger = temp.path().join(".agent-workbench/ledger.sqlite");
    let ledger_before_drift = fs::read(&ledger).unwrap();
    fs::write(
        &successor_path,
        format!("{successor_document}\npost-review same-path byte drift\n"),
    )
    .unwrap();
    for drifted in [
        preview_decomposition_reconciliation(
            temp.path(),
            DecompositionReconciliationApplication {
                design_version_id: imported.design_version_id,
                work_unit_id: work.work_unit_id,
                plan_path: &successor_path,
                closure_id: closure.closure_id,
                expected_current: &current.current_identity,
            },
        )
        .map(|_| ()),
        reconcile_decomposition_plan(
            temp.path(),
            DecompositionReconciliationApplication {
                design_version_id: imported.design_version_id,
                work_unit_id: work.work_unit_id,
                plan_path: &successor_path,
                closure_id: closure.closure_id,
                expected_current: &current.current_identity,
            },
        )
        .map(|_| ()),
    ] {
        let error = drifted.unwrap_err();
        assert!(
            error
                .to_string()
                .contains("bytes changed after the successor was staged for review"),
            "{error:#}"
        );
        assert_eq!(fs::read(&ledger).unwrap(), ledger_before_drift);
    }
    fs::write(&successor_path, &successor_document).unwrap();
    let ledger_before_preview = fs::read(&ledger).unwrap();
    let preview = preview_decomposition_reconciliation(
        temp.path(),
        DecompositionReconciliationApplication {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
            plan_path: &successor_path,
            closure_id: closure.closure_id,
            expected_current: &current.current_identity,
        },
    )
    .unwrap();
    assert_eq!(fs::read(&ledger).unwrap(), ledger_before_preview);
    assert_eq!(preview.predecessor_plan_id, predecessor.plan_id);
    assert!(!preview.idempotent);
    assert_eq!(preview.projection.endpoint_effects.len(), 12);
    assert!(preview.projection.command.contains(&format!(
        "--expected-current {}",
        preview.projection.commit_current
    )));
    assert!(preview.projection.shared_bindings.iter().any(|binding| {
        binding.kind == "evidence"
            && binding.id == evidence.implementation_evidence_id
            && binding.disposition == "historical"
    }));
    assert!(preview.projection.shared_bindings.iter().any(|binding| {
        binding.kind == "coverage"
            && binding.id == coverage.coverage_item_id
            && binding.disposition == "recompute"
            && binding.qualification == "recompute_required"
    }));
    assert!(preview.projection.shared_bindings.iter().any(|binding| {
        binding.kind == "review"
            && binding.id == run.review_run_id
            && binding.disposition == "historical"
            && binding.qualification == "historical_only"
    }));
    assert!(preview.projection.shared_bindings.iter().any(|binding| {
        binding.kind == "review"
            && binding.id == 0
            && binding.qualification == "fresh_review_required"
    }));

    fs::write(
        &successor_path,
        successor_document.replace(
            "Exercise the public operation.",
            "Exercise the public operation again.",
        ),
    )
    .unwrap();
    let owned_drift = preview_decomposition_reconciliation(
        temp.path(),
        DecompositionReconciliationApplication {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
            plan_path: &successor_path,
            closure_id: closure.closure_id,
            expected_current: &current.current_identity,
        },
    )
    .unwrap_err();
    assert!(
        owned_drift
            .to_string()
            .contains("bytes changed after the successor was staged for review"),
        "{owned_drift:#}"
    );
    assert_eq!(fs::read(&ledger).unwrap(), ledger_before_preview);
    fs::write(&successor_path, &successor_document).unwrap();

    let review_id = current
        .shared_bindings
        .iter()
        .find(|binding| binding.kind == "review")
        .unwrap()
        .id;
    let conn = crate::db::open_existing_project(temp.path()).unwrap();
    conn.execute(
        "update review_runs set result_summary='changed after preview' where id=?1",
        [review_id],
    )
    .unwrap();
    drop(conn);
    let review_drift = reconcile_decomposition_plan(
        temp.path(),
        DecompositionReconciliationApplication {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
            plan_path: &successor_path,
            closure_id: closure.closure_id,
            expected_current: &preview.projection.commit_current,
        },
    )
    .unwrap_err();
    assert!(review_drift.to_string().contains("predecessor changed"));
    let conn = crate::db::open_existing_project(temp.path()).unwrap();
    conn.execute(
        "update review_runs set result_summary='the exact Plan is ready' where id=?1",
        [review_id],
    )
    .unwrap();
    conn.execute(
        "update implementation_evidence set note='changed after preview' where id=?1",
        [evidence.implementation_evidence_id],
    )
    .unwrap();
    drop(conn);
    let evidence_drift = reconcile_decomposition_plan(
        temp.path(),
        DecompositionReconciliationApplication {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
            plan_path: &successor_path,
            closure_id: closure.closure_id,
            expected_current: &preview.projection.commit_current,
        },
    )
    .unwrap_err();
    assert!(evidence_drift.to_string().contains("predecessor changed"));
    let conn = crate::db::open_existing_project(temp.path()).unwrap();
    conn.execute(
        "update implementation_evidence set note='observable predecessor behavior' where id=?1",
        [evidence.implementation_evidence_id],
    )
    .unwrap();
    conn.execute(
        "update coverage_items set status='stale' where id=?1",
        [coverage.coverage_item_id],
    )
    .unwrap();
    drop(conn);
    let coverage_drift = reconcile_decomposition_plan(
        temp.path(),
        DecompositionReconciliationApplication {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
            plan_path: &successor_path,
            closure_id: closure.closure_id,
            expected_current: &preview.projection.commit_current,
        },
    )
    .unwrap_err();
    assert!(coverage_drift.to_string().contains("predecessor changed"));
    let conn = crate::db::open_existing_project(temp.path()).unwrap();
    conn.execute(
        "update coverage_items set status='covered' where id=?1",
        [coverage.coverage_item_id],
    )
    .unwrap();
    conn.execute("update tasks set status='blocked' where id=?1", [task])
        .unwrap();
    drop(conn);
    let predecessor_drift = reconcile_decomposition_plan(
        temp.path(),
        DecompositionReconciliationApplication {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
            plan_path: &successor_path,
            closure_id: closure.closure_id,
            expected_current: &preview.projection.commit_current,
        },
    )
    .unwrap_err();
    assert!(
        predecessor_drift
            .to_string()
            .contains("predecessor changed")
    );
    let conn = crate::db::open_existing_project(temp.path()).unwrap();
    conn.execute("update tasks set status='closed' where id=?1", [task])
        .unwrap();
    drop(conn);
    assert_eq!(
        show_decomposition_plan(
            temp.path(),
            DecompositionPlanQuery {
                design_version_id: imported.design_version_id,
                work_unit_id: work.work_unit_id,
            },
        )
        .unwrap(),
        current
    );

    let applied = reconcile_decomposition_plan(
        temp.path(),
        DecompositionReconciliationApplication {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
            plan_path: &successor_path,
            closure_id: closure.closure_id,
            expected_current: &preview.projection.commit_current,
        },
    )
    .unwrap();
    assert_eq!(applied.projection, preview.projection);
    assert_eq!(applied.predecessor_plan_id, predecessor.plan_id);
    assert!(!applied.idempotent);
    let shown = show_decomposition_plan(
        temp.path(),
        DecompositionPlanQuery {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
        },
    )
    .unwrap();
    assert_eq!(shown.id, applied.plan.plan_id);
    assert_eq!(shown.status, "applied");
    assert_eq!(shown.revision, 2);
    assert_eq!(shown.items[0].key, "current-item");
    let current_tasks = list_tasks(
        temp.path(),
        TaskListQuery {
            status: None,
            work_unit_id: Some(work.work_unit_id),
        },
    )
    .unwrap();
    assert_eq!(current_tasks.len(), 2);
    assert!(
        current_tasks
            .iter()
            .all(|candidate| candidate.status == "open")
    );
    assert!(
        current_tasks
            .iter()
            .all(|candidate| candidate.id != task && candidate.id != retired_task)
    );
    let successor_task = current_tasks
        .iter()
        .find(|candidate| candidate.title == "Preserve behavior")
        .unwrap();
    let conn = crate::db::open_existing_project(temp.path()).unwrap();
    let terminal_source: (String, String, String, String, String, String, String) = conn
        .query_row(
            r#"
            select task.status,item.status,gate.status,phase.status,phase.phase_key,
                   phase.closed_at,epoch.terminal_at
            from tasks task,checklist_items item,validation_gates gate,work_phases phase
            join phase_epochs epoch on epoch.id=phase.id
            where task.id=?1 and item.id=?2 and gate.id=?3 and phase.id=?4
            "#,
            rusqlite::params![task, item, gate, phase],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        terminal_source,
        (
            "closed".into(),
            "closed".into(),
            "closed".into(),
            "closed".into(),
            "old-slice".into(),
            "terminal-source-at".into(),
            "terminal-source-at".into(),
        )
    );
    let dependency_states: (String, String, Option<String>, Option<String>) = conn
        .query_row(
            r#"
            select source.status,source.evidence_ref,target.status,target.evidence_ref
            from work_phase_dependencies source
            join decomposition_reconciliation_dependencies mapping
              on mapping.source_dependency_id=source.id
            join decomposition_application_dependencies application
              on application.decomposition_slice_dependency_id=mapping.successor_dependency_id
            join work_phase_dependencies target on target.id=application.work_phase_dependency_id
            where mapping.decomposition_plan_id=?1
            "#,
            [applied.plan.plan_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        dependency_states,
        (
            "satisfied".into(),
            "terminal-dependency-evidence".into(),
            Some("open".into()),
            None,
        )
    );
    let successor_epoch_dependency: (String, Option<String>, Option<i64>) = conn
        .query_row(
            r#"
            select epoch.state,epoch.evidence_ref,epoch.authority_event_id
            from decomposition_reconciliation_dependencies mapping
            join decomposition_application_dependencies application
              on application.decomposition_slice_dependency_id=mapping.successor_dependency_id
            join phase_epoch_dependencies epoch
              on epoch.id=application.work_phase_dependency_id
            where mapping.decomposition_plan_id=?1
            "#,
            [applied.plan.plan_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(successor_epoch_dependency, ("open".into(), None, None));
    let retired_state: (String, String, i64) = conn
        .query_row(
            r#"
            select identity.status,revision.status,
                   (select count(*) from tasks where id=?1)
            from task_revision_aliases alias
            join task_revisions revision on revision.id=alias.task_revision_id
            join task_identities identity on identity.id=revision.task_identity_id
            where alias.historical_task_id=?1
            "#,
            [retired_task],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(retired_state, ("retired".into(), "retired".into(), 1));
    drop(conn);
    let checklists = list_checklists(temp.path(), None).unwrap();
    assert_eq!(
        checklists
            .iter()
            .filter(|checklist| checklist.status == "active")
            .count(),
        1
    );
    assert_eq!(
        checklists
            .iter()
            .filter(|checklist| checklist.status == "stale")
            .count(),
        1
    );
    let current_gates = list_validation_gate_context(
        temp.path(),
        ValidationGateContextQuery {
            design_version_id: imported.design_version_id,
            work_unit_id: Some(work.work_unit_id),
        },
    )
    .unwrap();
    assert_eq!(current_gates.len(), 2);
    assert!(
        current_gates
            .iter()
            .any(|candidate| candidate.task_id == Some(successor_task.id))
    );
    let current_phases = list_phases(temp.path(), work.work_unit_id)
        .unwrap()
        .into_iter()
        .filter(|phase| phase.status != "superseded")
        .collect::<Vec<_>>();
    assert_eq!(current_phases.len(), 3);
    assert!(current_phases.iter().any(|candidate| candidate.id == phase
        && candidate.key == "old-slice"
        && candidate.status == "closed"));
    assert!(current_phases.iter().any(|candidate| candidate.id != phase
        && candidate.key == "old-slice"
        && candidate.status == "open"));
    assert!(
        current_phases
            .iter()
            .any(|candidate| candidate.key == "second-slice" && candidate.status == "open")
    );
    let conn = crate::db::open_existing_project(temp.path()).unwrap();
    let project = crate::db::project_id(&conn).unwrap();
    assert_eq!(
        crate::traceability::selected_stale_record_in(&conn, project).unwrap(),
        None
    );
    drop(conn);

    let attempt = ready_closure(
        temp.path(),
        ClosureReady {
            closure_id: closure.closure_id,
            implementation_evidence: "the reconciliation result was published atomically",
            tests_or_gates: "public reconciliation behavior",
            closed_by_commit: None,
        },
    )
    .unwrap();
    let verification = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: review_plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some(&attempt.context_ref),
            prompt_deviations: None,
            result_summary: Some("the immutable reconciliation result is verified"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: Some("independent-reconciliation-reviewer"),
            external_agent_id: Some("independent-reconciliation-reviewer"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:immutable-reconciliation-result"),
        },
        Some("verified"),
    )
    .unwrap();
    add_finding_verification(
        temp.path(),
        NewFindingVerification {
            review_run_id: verification.review_run_id,
            finding_id: finding.finding_id,
            closure_id: closure.closure_id,
            result: "verified",
            notes: Some("the exact reconciliation result is durable"),
        },
    )
    .unwrap();
    adjudicate_verification(
        temp.path(),
        verification.review_run_id,
        finding.finding_id,
        closure.closure_id,
        attempt.attempt_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "accept the independent exact-result verification",
            expected_current: "pending",
        },
    )
    .unwrap();
    let later_evidence = add_implementation_evidence(
        temp.path(),
        NewImplementationEvidence {
            task_id: Some(successor_task.id),
            design_version_id: Some(imported.design_version_id),
            requirement_key: Some("REQ-001"),
            evidence_type: "artifact",
            commit_sha: None,
            file_path: None,
            line_ref: None,
            symbol: None,
            artifact_path: Some("artifacts/after-reconciliation.txt"),
            note: Some("evidence created after the first reconciliation result"),
        },
    )
    .unwrap();

    let retry = reconcile_decomposition_plan(
        temp.path(),
        DecompositionReconciliationApplication {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
            plan_path: &successor_path,
            closure_id: closure.closure_id,
            expected_current: &applied.projection.commit_current,
        },
    )
    .unwrap();
    assert_eq!(retry.projection, applied.projection);
    assert!(!retry.projection.shared_bindings.iter().any(|binding| {
        binding.kind == "evidence" && binding.id == later_evidence.implementation_evidence_id
    }));
    assert_eq!(retry.plan.plan_id, applied.plan.plan_id);
    assert_eq!(
        retry.correction_application_id,
        applied.correction_application_id
    );
    assert!(retry.idempotent);
}
