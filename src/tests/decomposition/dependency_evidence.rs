use super::*;

#[test]
fn preserved_dependency_uses_successor_phase_close_evidence() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "preserve dependency epoch", None).unwrap();
    let package = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "preserve-dependency-epoch",
            title: "Preserve Dependency Epoch",
        },
    )
    .unwrap();
    fs::write(
        package.package_path.join("requirements/README.md"),
        requirement_doc("REQ-001", "Preserve dependency qualification", "high"),
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
    let graph = format!(
        r#"items:
  - key: prerequisite-item
    requirements: [REQ-001]
    title: Prerequisite behavior
    details: Preserve the prerequisite behavior.
    completion:
      outcome: The prerequisite behavior remains observable.
      observation: Exercise the prerequisite behavior.
      evidence_owner: work:{}
      evidence_kind: validation
      gates: [GATE-001]
    checklist:
      - key: prerequisite-boundary
        condition: The prerequisite behavior is observed.
        evidence_kind: validation
        gates: [GATE-001]
    slice: prerequisite-slice
  - key: dependent-item
    requirements: [REQ-001]
    title: Dependent behavior
    details: Preserve the dependent behavior.
    completion:
      outcome: The dependent behavior remains observable.
      observation: Exercise the dependent behavior.
      evidence_owner: work:{}
      evidence_kind: validation
      gates: [GATE-001]
    checklist:
      - key: dependent-boundary
        condition: The dependent behavior is observed.
        evidence_kind: validation
        gates: [GATE-001]
    slice: dependent-slice
slices:
  - key: prerequisite-slice
    title: Prerequisite Slice
    order: 1
    depends_on: []
  - key: dependent-slice
    title: Dependent Slice
    order: 2
    depends_on: [prerequisite-slice]"#,
        work.work_unit_id, work.work_unit_id
    );
    let predecessor_path = plans.join("dependency-predecessor.md");
    fs::write(
        &predecessor_path,
        format!(
            "# Dependency predecessor\n\n```yaml agent-workbench\ntype: decomposition_plan\nformat: 1\nkey: dependency-predecessor\ndesign_fingerprint: {}\n{}\n```\n",
            imported.content_hash, graph
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
    let tasks = list_tasks(
        temp.path(),
        TaskListQuery {
            status: None,
            work_unit_id: Some(work.work_unit_id),
        },
    )
    .unwrap();
    let prerequisite_task = tasks
        .iter()
        .find(|task| task.title == "Prerequisite behavior")
        .unwrap()
        .id;
    let dependent_task = tasks
        .iter()
        .find(|task| task.title == "Dependent behavior")
        .unwrap()
        .id;
    let checklist = list_checklists(temp.path(), None).unwrap()[0].id;
    let checklist_items = list_checklist_items(
        temp.path(),
        ChecklistItemListQuery {
            checklist_id: Some(checklist),
            status: None,
        },
    )
    .unwrap();
    let prerequisite_checklist = checklist_items
        .iter()
        .find(|item| item.task_id == prerequisite_task)
        .unwrap()
        .id;
    let dependent_checklist = checklist_items
        .iter()
        .find(|item| item.task_id == dependent_task)
        .unwrap()
        .id;
    let gates = list_validation_gate_context(
        temp.path(),
        ValidationGateContextQuery {
            design_version_id: imported.design_version_id,
            work_unit_id: Some(work.work_unit_id),
        },
    )
    .unwrap();
    let prerequisite_gate = gates
        .iter()
        .find(|gate| gate.task_id == Some(prerequisite_task))
        .unwrap()
        .id;
    let dependent_gate = gates
        .iter()
        .find(|gate| gate.task_id == Some(dependent_task))
        .unwrap()
        .id;
    let phases = list_phases(temp.path(), work.work_unit_id).unwrap();
    let prerequisite = phases
        .iter()
        .find(|phase| phase.key == "prerequisite-slice")
        .unwrap()
        .id;
    let dependent = phases
        .iter()
        .find(|phase| phase.key == "dependent-slice")
        .unwrap()
        .id;
    let dependency = list_phase_dependencies(temp.path(), work.work_unit_id).unwrap()[0].id;
    let predecessor_evidence = format!("phase:{prerequisite}:closed");
    let conn = crate::db::open_existing_project(temp.path()).unwrap();
    conn.execute(
        "update tasks set status='closed',closed_by_commit='qualified-predecessor' where id=?1",
        [prerequisite_task],
    )
    .unwrap();
    conn.execute(
        "update checklist_items set status='closed' where id=?1",
        [prerequisite_checklist],
    )
    .unwrap();
    conn.execute(
        "update validation_gates set status='closed' where id=?1",
        [prerequisite_gate],
    )
    .unwrap();
    conn.execute(
        "update work_phases set status='closed',closed_at=current_timestamp,close_summary='qualified predecessor' where id=?1",
        [prerequisite],
    )
    .unwrap();
    conn.execute(
        "update phase_epochs set state='closed',terminal_at=current_timestamp,terminal_summary='qualified predecessor' where id=?1",
        [prerequisite],
    )
    .unwrap();
    conn.execute(
        "update work_phase_dependencies set status='satisfied',evidence_ref=?1,resolved_at=current_timestamp where id=?2",
        rusqlite::params![predecessor_evidence, dependency],
    )
    .unwrap();
    conn.execute(
        "update phase_epoch_dependencies set state='satisfied',evidence_ref=?1,terminal_at=current_timestamp where id=?2",
        rusqlite::params![predecessor_evidence, dependency],
    )
    .unwrap();
    drop(conn);
    let current = show_decomposition_plan(
        temp.path(),
        DecompositionPlanQuery {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
        },
    )
    .unwrap();
    let successor_path = plans.join("dependency-successor.md");
    fs::write(
        &successor_path,
        format!(
            "# Dependency successor\n\n```yaml agent-workbench\ntype: decomposition_plan\nformat: 1\nkey: dependency-successor\ndesign_fingerprint: {}\n{}\nreconciliation:\n  predecessor: {}\n  expected_current: {}\n  tasks:\n    - {{ source: {}, disposition: retained, item: prerequisite-item, effect: preserve }}\n    - {{ source: {}, disposition: retained, item: dependent-item, effect: preserve }}\n  checklist:\n    - {{ source: {}, disposition: retained, item: prerequisite-item, boundary: prerequisite-boundary, effect: preserve }}\n    - {{ source: {}, disposition: retained, item: dependent-item, boundary: dependent-boundary, effect: preserve }}\n  gates:\n    - {{ source: {}, disposition: retained, item: prerequisite-item, gate: GATE-001, boundary: retained-source, effect: preserve }}\n    - {{ source: {}, disposition: retained, item: dependent-item, gate: GATE-001, boundary: retained-source, effect: preserve }}\n  phases:\n    - {{ source: {}, disposition: retained, slice: prerequisite-slice, effect: preserve }}\n    - {{ source: {}, disposition: retained, slice: dependent-slice, effect: preserve }}\n  dependencies:\n    - {{ source: {}, disposition: retained, from: prerequisite-slice, to: dependent-slice, effect: preserve }}\n```\n",
            imported.content_hash,
            graph,
            predecessor.plan_id,
            current.current_identity,
            prerequisite_task,
            dependent_task,
            prerequisite_checklist,
            dependent_checklist,
            prerequisite_gate,
            dependent_gate,
            prerequisite,
            dependent,
            dependency,
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
            stage: "close-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    let run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: review_plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some(&format!("work_unit:{}", work.work_unit_id)),
            prompt_deviations: None,
            result_summary: Some("the successor dependency requires reconciliation"),
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
            description: "publish a successor dependency with current qualification",
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
    let successor_file_name = successor_path.file_name().unwrap().to_string_lossy();
    let surface = format!(
        "design:edit:plans/{successor_file_name},transition:decomposition-plan-reconcile:{}/{}/{}",
        imported.design_version_id,
        work.work_unit_id,
        crate::review::encode_opaque_component(&successor_project_path)
    );
    let closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: finding.finding_id,
            design_invariant: "successor dependency qualification names the successor epoch",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some(&surface),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("reconcile the compatible successor"),
            tests_or_gates: Some("observe successor dependency evidence"),
            verification_plan: Some("compare predecessor and successor evidence"),
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
    fs::write(
        &successor_path,
        successor_document.replace(
            &format!(
                "    - {{ source: {prerequisite}, disposition: retained, slice: prerequisite-slice, effect: preserve }}"
            ),
            &format!(
                "    - {{ source: {prerequisite}, disposition: retained, slice: prerequisite-slice, effect: open }}"
            ),
        ),
    )
    .unwrap();
    let rejected = reconcile_decomposition_plan(
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
        rejected
            .to_string()
            .contains("current successor endpoints and qualifying evidence"),
        "{rejected:#}"
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
    let staged = revise_decomposition_plan(
        temp.path(),
        DecompositionRevise {
            plan_id: current.id,
            plan_path: &successor_path,
            draft: false,
            expected_current: &current.current_identity,
            idempotency_key: "reviewed-dependency-successor",
        },
    )
    .unwrap();
    assert_eq!(staged.plan.status, "ready");
    accept_current_plan_review(temp.path(), imported.design_version_id, work.work_unit_id);
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
    reconcile_decomposition_plan(
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

    let successor_phases = list_phases(temp.path(), work.work_unit_id).unwrap();
    let successor_prerequisite = successor_phases
        .iter()
        .find(|phase| phase.key == "prerequisite-slice" && phase.id != prerequisite)
        .unwrap();
    assert_eq!(successor_prerequisite.status, "closed");
    let dependencies = list_phase_dependencies(temp.path(), work.work_unit_id).unwrap();
    let predecessor_dependency = dependencies
        .iter()
        .find(|candidate| candidate.id == dependency)
        .unwrap();
    assert_eq!(
        predecessor_dependency.evidence_ref.as_deref(),
        Some(predecessor_evidence.as_str())
    );
    let successor_dependency = dependencies
        .iter()
        .find(|candidate| candidate.id != dependency)
        .unwrap();
    let successor_evidence = format!("phase:{}:closed", successor_prerequisite.id);
    assert_eq!(successor_dependency.status, "satisfied");
    assert_eq!(
        successor_dependency.evidence_ref.as_deref(),
        Some(successor_evidence.as_str())
    );
    assert_ne!(successor_evidence, predecessor_evidence);
}
