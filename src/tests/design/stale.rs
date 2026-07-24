use super::*;

#[test]
fn stale_close_disposes_selected_validation_gate() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "implement storage lifecycle", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "implement cleanup",
            priority: "high",
            source: "design",
            work_unit_id: None,
            details: None,
            completion_condition: Some("cleanup behavior is covered"),
        },
    )
    .unwrap();
    let init = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "storage-lifecycle",
            title: "Storage Lifecycle",
        },
    )
    .unwrap();
    fs::write(
        init.package_path.join("requirements").join("README.md"),
        requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
    )
    .unwrap();
    fs::write(
        init.package_path.join("validation").join("gates.md"),
        validation_gate_doc("GATE-001"),
    )
    .unwrap();
    let first_import = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &init.package_path,
            status: "draft",
        },
    )
    .unwrap();
    approve_design_version(
        temp.path(),
        DesignVersionApproval {
            design_version_id: first_import.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    let derivation = derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: first_import.design_version_id,
            requirement_key: "REQ-001",
            task_id: task.task_id,
            derivation_reason: None,
            checklist_title: None,
            item_title: None,
            completion_condition: None,
        },
    )
    .unwrap();
    let gate = select_validation_gate(
        temp.path(),
        ValidationGateSelection {
            design_version_id: first_import.design_version_id,
            gate_key: "GATE-001",
            requirement_key: "REQ-001",
            task_id: task.task_id,
            command: None,
            command_profile: None,
            timeout: None,
        },
    )
    .unwrap();
    let make_phase = |key: &str, order: i64| {
        create_phase(
            temp.path(),
            NewWorkPhase {
                work_unit_id: work.work_unit_id,
                design_version_id: Some(first_import.design_version_id),
                key,
                title: key,
                kind: "implementation",
                order,
                reason: None,
            },
        )
        .unwrap()
    };
    let prerequisite_a = make_phase("prerequisite-a", 10);
    let prerequisite_b = make_phase("prerequisite-b", 11);
    let prerequisite_c = make_phase("prerequisite-c", 12);
    let prerequisite_d = make_phase("prerequisite-d", 13);
    let satisfy_dependency = add_phase_dependency(
        temp.path(),
        NewPhaseDependency {
            from_phase_id: prerequisite_a.phase_id,
            to_phase_id: prerequisite_b.phase_id,
            dependency_type: "blocks",
            reason: "satisfy through correction",
        },
    )
    .unwrap();
    let accept_dependency = add_phase_dependency(
        temp.path(),
        NewPhaseDependency {
            from_phase_id: prerequisite_c.phase_id,
            to_phase_id: prerequisite_d.phase_id,
            dependency_type: "requires",
            reason: "accept through correction",
        },
    )
    .unwrap();
    suspend_work(
        temp.path(),
        "verify global stale selection without an active work unit",
        "apply the declared stale transition",
    )
    .unwrap();
    let correction_plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(first_import.design_version_id),
            review_type: "design_review",
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
    let correction_run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: correction_plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("work_unit:1"),
            prompt_deviations: None,
            result_summary: Some("gate will require mediated stale disposal"),
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
    let correction_finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: correction_run.review_run_id,
            finding_type: "design_finding",
            severity: "high",
            description: "dispose the stale gate through the correction contract",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), correction_finding.finding_id, "valid").unwrap();
    let stale_surface = format!(
        "transition:phase-dependency-satisfy:{},transition:phase-dependency-accept:{},transition:phase-create:{}/{}/@foundation/implementation/1/foundation,transition:phase-create:{}/{}/@verification/validation/2/verification,transition:phase-assign:@foundation/{},transition:phase-dependency-add:@foundation/@verification/blocks,transition:stale-close:validation_gate/{}",
        satisfy_dependency.dependency_id,
        accept_dependency.dependency_id,
        work.work_unit_id,
        first_import.design_version_id,
        work.work_unit_id,
        first_import.design_version_id,
        task.task_id,
        gate.validation_gate_id
    );
    let reversed_stale_surfaces = format!(
        "transition:stale-close:validation_gate/{},transition:stale-accept:checklist/{}",
        gate.validation_gate_id, derivation.checklist_id
    );
    assert!(
        add_closure(
            temp.path(),
            NewClosure {
                finding_id: correction_finding.finding_id,
                design_invariant: "stale transitions follow the global tuple",
                design_citations: None,
                implementation_evidence: None,
                affected_surfaces: Some(&reversed_stale_surfaces),
                same_invariant_search: None,
                other_violations_found: None,
                fix_plan: Some("apply stale transitions"),
                tests_or_gates: Some("stale inventory"),
                verification_plan: Some("resume design review"),
                closed_by_commit: None,
            },
        )
        .is_err()
    );
    let correction_closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: correction_finding.finding_id,
            design_invariant: "stale gate is disposed through an audited transition",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some(&stale_surface),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("apply the declared stale transition"),
            tests_or_gates: Some("stale inventory"),
            verification_plan: Some("resume design review"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    begin_correction(temp.path(), correction_closure.closure_id).unwrap();
    assert!(
        apply_correction_transition(temp.path(), correction_closure.closure_id, 1, None, None)
            .unwrap_err()
            .to_string()
            .contains("requires --evidence")
    );
    apply_correction_transition(
        temp.path(),
        correction_closure.closure_id,
        1,
        None,
        Some("validation-run:dependency-satisfied"),
    )
    .unwrap();
    assert!(
        apply_correction_transition(temp.path(), correction_closure.closure_id, 2, None, None)
            .unwrap_err()
            .to_string()
            .contains("requires --authority")
    );
    let dependency_authority = add_authority_event(
        temp.path(),
        NewAuthorityEvent {
            event_type: "user_instruction",
            source: Some("test-user"),
            summary: "accept exact phase dependency",
            scope: Some("project"),
            precedence: 100,
        },
    )
    .unwrap()
    .authority_event_id;
    apply_correction_transition(
        temp.path(),
        correction_closure.closure_id,
        2,
        Some(dependency_authority),
        None,
    )
    .unwrap();
    for token in 3..=6 {
        apply_correction_transition(
            temp.path(),
            correction_closure.closure_id,
            token,
            None,
            None,
        )
        .unwrap();
    }
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let changed_audits: i64 = conn
        .query_row(
            "select count(*) from correction_transition_applications where correction_session_id=(select id from correction_sessions where closure_id=?1) and before_state != after_state",
            params![correction_closure.closure_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(changed_audits, 6);
    drop(conn);
    fs::write(
        init.package_path.join("validation").join("gates.md"),
        validation_gate_doc("GATE-001").replace(
            "Run the project test suite",
            "Run the complete project test suite",
        ),
    )
    .unwrap();
    let second_import = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &init.package_path,
            status: "draft",
        },
    )
    .unwrap();
    approve_design_version(
        temp.path(),
        DesignVersionApproval {
            design_version_id: second_import.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(second_import.design_version_id),
        },
    )
    .unwrap();
    assert!(
        add_task(
            temp.path(),
            NewTask {
                title: "must not cross stale selection",
                priority: "high",
                source: "manual",
                work_unit_id: Some(work.work_unit_id),
                details: None,
                completion_condition: Some("stale is resolved first"),
            },
        )
        .unwrap_err()
        .to_string()
        .contains("selected lifecycle action")
    );
    let status = project_status(temp.path()).unwrap();
    let bootstrap = status
        .owner_actions
        .iter()
        .find(|owner| {
            owner.next_action.contains(&format!(
                "closure transition apply {} --token 7",
                correction_closure.closure_id
            ))
        })
        .unwrap();
    assert!(bootstrap.next_action.contains(&format!(
        "closure transition apply {} --token 7",
        correction_closure.closure_id
    )));
    let transition =
        apply_correction_transition(temp.path(), correction_closure.closure_id, 7, None, None)
            .unwrap();
    assert!(!transition.idempotent);
    let replayed =
        apply_correction_transition(temp.path(), correction_closure.closure_id, 7, None, None)
            .unwrap();
    assert!(replayed.idempotent);
    assert_eq!(replayed.application_id, transition.application_id);
    assert_eq!(replayed.result_ref, transition.result_ref);
    let stale = list_stale_records(temp.path()).unwrap();
    assert!(!stale.iter().any(|record| {
        record.record_type == "validation_gate" && record.id == gate.validation_gate_id
    }));
    let conn = open_ledger(&temp.path().join(".agent-workbench").join("ledger.sqlite")).unwrap();
    let gate_status: String = conn
        .query_row(
            "select status from validation_gates where id = ?1",
            rusqlite::params![gate.validation_gate_id],
            |row| row.get(0),
        )
        .unwrap();
    let acceptance_type: String = conn
        .query_row(
            "select acceptance_type from acceptance_records where target_type = 'stale_record' and stale_record_type = 'validation_gate' and stale_record_id = ?1 order by id desc limit 1",
            rusqlite::params![gate.validation_gate_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(gate_status, "closed");
    assert_eq!(acceptance_type, "stale_accepted");
}
