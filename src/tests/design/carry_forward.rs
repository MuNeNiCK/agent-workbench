use super::*;

#[test]
fn mediated_task_carry_forward_requires_verified_baseline_and_is_atomic() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "carry unchanged baseline", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "carry cleanup requirement",
            priority: "high",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("cleanup remains validated"),
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
        format!(
            "{}{}",
            validation_gate_doc("GATE-001"),
            validation_gate_doc("GATE-002")
        ),
    )
    .unwrap();
    let baseline = import_design_package(
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
            design_version_id: baseline.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    let baseline_derivation = derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: baseline.design_version_id,
            requirement_key: "REQ-001",
            task_id: task.task_id,
            derivation_reason: Some("verified baseline"),
            checklist_title: Some("Baseline checklist"),
            item_title: None,
            completion_condition: None,
        },
    )
    .unwrap();
    let baseline_gate = select_validation_gate(
        temp.path(),
        ValidationGateSelection {
            design_version_id: baseline.design_version_id,
            gate_key: "GATE-001",
            requirement_key: "REQ-001",
            task_id: task.task_id,
            command: None,
            command_profile: None,
            timeout: None,
        },
    )
    .unwrap();
    let baseline_gate_2 = select_validation_gate(
        temp.path(),
        ValidationGateSelection {
            design_version_id: baseline.design_version_id,
            gate_key: "GATE-002",
            requirement_key: "REQ-001",
            task_id: task.task_id,
            command: None,
            command_profile: None,
            timeout: None,
        },
    )
    .unwrap();
    add_validation_run(
        temp.path(),
        NewValidationRun {
            validation_gate_id: baseline_gate.validation_gate_id,
            command_usage_id: None,
            repository_snapshot_id: None,
            result: "pass",
            command: None,
            classification: None,
            acceptance_record_id: None,
            artifact_path: None,
            artifact_hash: None,
            notes: Some("authoritative baseline pass"),
        },
    )
    .unwrap();
    add_validation_run(
        temp.path(),
        NewValidationRun {
            validation_gate_id: baseline_gate_2.validation_gate_id,
            command_usage_id: None,
            repository_snapshot_id: None,
            result: "pass",
            command: None,
            classification: None,
            acceptance_record_id: None,
            artifact_path: None,
            artifact_hash: None,
            notes: Some("second authoritative baseline pass"),
        },
    )
    .unwrap();
    fs::write(
        init.package_path.join("01-introduction-goals.md"),
        "# Introduction And Goals\n\nUnrelated wording changed.\n",
    )
    .unwrap();
    let current = import_design_package(
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
            design_version_id: current.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    let ready_plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(current.design_version_id),
            review_type: "design_review",
            required: true,
            stage: "design-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: ready_plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some(&format!(
                "review-context:design-review:design={}:work={}",
                current.design_version_id, work.work_unit_id
            )),
            prompt_deviations: None,
            result_summary: Some("unchanged current design is ready for decomposition"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: Some("design-reviewer"),
            external_agent_id: Some("design-reviewer-carry"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:design-ready-carry"),
        },
    )
    .unwrap();
    assert_eq!(
        list_task_derivations(
            temp.path(),
            TaskDerivationListQuery {
                design_version_id: baseline.design_version_id,
                work_unit_id: Some(work.work_unit_id),
            },
        )
        .unwrap()
        .into_iter()
        .find(|record| record.id == baseline_derivation.task_derivation_id)
        .unwrap()
        .status,
        "active"
    );
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(current.design_version_id),
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
    let run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("work_unit:1"),
            prompt_deviations: None,
            result_summary: Some("unchanged baseline should be carried explicitly"),
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
            finding_type: "design_finding",
            severity: "high",
            description: "record verified baseline carry-forward",
            design_requirement_id: None,
            task_id: Some(task.task_id),
        },
    )
    .unwrap();
    classify_finding(temp.path(), finding.finding_id, "valid").unwrap();
    let surface = format!(
        "transition:design-decompose:{}/{},transition:task-accept-out-of-scope:@task/REQ-001",
        current.design_version_id, work.work_unit_id
    );
    let closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: finding.finding_id,
            design_invariant: "unchanged verified baseline is carried with authority",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some(&surface),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("apply the verified carry-forward bundle"),
            tests_or_gates: Some("baseline GATE-001 pass"),
            verification_plan: Some("resume design review"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    begin_correction(temp.path(), closure.closure_id).unwrap();
    apply_correction_transition(temp.path(), closure.closure_id, 1, None, None).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let current_gate: i64 = conn
        .query_row(
            "select record_id from correction_transition_aliases where alias='@gate/REQ-001/GATE-001'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let current_gate_2: i64 = conn
        .query_row(
            "select record_id from correction_transition_aliases where alias='@gate/REQ-001/GATE-002'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let carried_task: i64 = conn
        .query_row(
            "select record_id from correction_transition_aliases where alias='@task/REQ-001'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(carried_task, task.task_id);
    drop(conn);
    let authority = add_authority_event(
        temp.path(),
        NewAuthorityEvent {
            event_type: "user_instruction",
            source: Some("test-user"),
            summary: "carry unchanged verified requirement",
            scope: Some("project"),
            precedence: 100,
        },
    )
    .unwrap();
    for scope in [
        "requirement:REQ-001".to_string(),
        format!("work-unit:{}", work.work_unit_id),
    ] {
        let scoped = add_authority_event(
            temp.path(),
            NewAuthorityEvent {
                event_type: "user_instruction",
                source: Some("test-user"),
                summary: "validate exact carry authority scope",
                scope: Some(&scope),
                precedence: 100,
            },
        )
        .unwrap();
        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        crate::planning::ensure_verified_baseline_carry_forward(
            &conn,
            crate::db::project_id(&conn).unwrap(),
            task.task_id,
            Some(work.work_unit_id),
            scoped.authority_event_id,
        )
        .unwrap();
    }
    let wrong_authority = add_authority_event(
        temp.path(),
        NewAuthorityEvent {
            event_type: "user_instruction",
            source: Some("test-user"),
            summary: "wrong carry authority scope",
            scope: Some("requirement:REQ-999"),
            precedence: 100,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    assert!(
        crate::planning::ensure_verified_baseline_carry_forward(
            &conn,
            crate::db::project_id(&conn).unwrap(),
            task.task_id,
            Some(work.work_unit_id),
            wrong_authority.authority_event_id,
        )
        .unwrap_err()
        .to_string()
        .contains("exact requirement or work unit")
    );
    let current_requirement: (i64, String, Option<String>) = conn
        .query_row(
            "select r.id, r.requirement_hash, r.required_surfaces from design_requirements r where r.design_version_id=?1 and r.requirement_key='REQ-001'",
            params![current.design_version_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    conn.execute(
        "update design_requirements set requirement_hash='changed' where id=?1",
        params![current_requirement.0],
    )
    .unwrap();
    assert!(
        crate::planning::ensure_verified_baseline_carry_forward(
            &conn,
            crate::db::project_id(&conn).unwrap(),
            task.task_id,
            Some(work.work_unit_id),
            authority.authority_event_id,
        )
        .unwrap_err()
        .to_string()
        .contains("normalized hash")
    );
    conn.execute(
        "update design_requirements set requirement_hash=?1, required_surfaces='cli' where id=?2",
        params![current_requirement.1, current_requirement.0],
    )
    .unwrap();
    assert!(
        crate::planning::ensure_verified_baseline_carry_forward(
            &conn,
            crate::db::project_id(&conn).unwrap(),
            task.task_id,
            Some(work.work_unit_id),
            authority.authority_event_id,
        )
        .unwrap_err()
        .to_string()
        .contains("required surfaces")
    );
    conn.execute(
        "update design_requirements set required_surfaces=?1 where id=?2",
        params![current_requirement.2, current_requirement.0],
    )
    .unwrap();
    let current_gate_template: (i64, String) = conn
        .query_row(
            "select id, gate_hash from validation_gate_templates where design_version_id=?1 and gate_key='GATE-002'",
            params![current.design_version_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    conn.execute(
        "update validation_gate_templates set gate_hash='changed' where id=?1",
        params![current_gate_template.0],
    )
    .unwrap();
    assert!(
        crate::planning::ensure_verified_baseline_carry_forward(
            &conn,
            crate::db::project_id(&conn).unwrap(),
            task.task_id,
            Some(work.work_unit_id),
            authority.authority_event_id,
        )
        .unwrap_err()
        .to_string()
        .contains("gate set changed")
    );
    conn.execute(
        "update validation_gate_templates set gate_hash=?1 where id=?2",
        params![current_gate_template.1, current_gate_template.0],
    )
    .unwrap();
    drop(conn);
    add_validation_run(
        temp.path(),
        NewValidationRun {
            validation_gate_id: baseline_gate.validation_gate_id,
            command_usage_id: None,
            repository_snapshot_id: None,
            result: "fail",
            command: None,
            classification: None,
            acceptance_record_id: None,
            artifact_path: None,
            artifact_hash: None,
            notes: Some("latest baseline run must win over the earlier pass"),
        },
    )
    .unwrap();
    assert!(
        apply_correction_transition(
            temp.path(),
            closure.closure_id,
            2,
            Some(authority.authority_event_id),
            None,
        )
        .unwrap_err()
        .to_string()
        .contains("latest authoritative passing run")
    );
    assert_eq!(
        list_tasks(
            temp.path(),
            TaskListQuery {
                status: None,
                work_unit_id: None,
            },
        )
        .unwrap()
        .into_iter()
        .find(|record| record.id == task.task_id)
        .unwrap()
        .status,
        "open"
    );
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let partially_changed: i64 = conn
        .query_row(
            "select (select count(*) from checklist_items where task_id=?1 and status!='open') + (select count(*) from validation_gates where task_id=?1 and design_requirement_id=(select design_requirement_id from task_derivations where task_id=?1 and status='active') and status!='active') + (select count(*) from coverage_items where task_id=?1 and status='accepted_out_of_scope')",
            params![task.task_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(partially_changed, 0);
    drop(conn);
    add_validation_run(
        temp.path(),
        NewValidationRun {
            validation_gate_id: baseline_gate.validation_gate_id,
            command_usage_id: None,
            repository_snapshot_id: None,
            result: "pass",
            command: None,
            classification: None,
            acceptance_record_id: None,
            artifact_path: None,
            artifact_hash: None,
            notes: Some("latest authoritative baseline pass restores eligibility"),
        },
    )
    .unwrap();
    apply_correction_transition(
        temp.path(),
        closure.closure_id,
        2,
        Some(authority.authority_event_id),
        None,
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let state: (String, String, i64, i64) = conn
        .query_row(
            "select t.status, ci.status, (select count(*) from validation_gates where id in (?2,?3) and status='closed'), (select count(*) from coverage_items c join acceptance_records ar on ar.coverage_item_id=c.id and ar.status='approved' where c.task_id=t.id) from tasks t join checklist_items ci on ci.task_id=t.id and ci.checklist_id=(select max(id) from checklists where work_unit_id=?1) where t.id=?4",
            params![work.work_unit_id, current_gate, current_gate_2, task.task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        state,
        (
            "accepted_out_of_scope".to_string(),
            "accepted_out_of_scope".to_string(),
            2,
            1,
        )
    );
    let baseline_state: (String, String) = conn
        .query_row(
            "select (select status from validation_gates where id=?1), (select status from validation_gates where id=?2)",
            params![baseline_gate.validation_gate_id, baseline_gate_2.validation_gate_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(baseline_state, ("active".to_string(), "active".to_string()));
}

#[test]
fn mediated_task_carry_forward_rejects_ambiguous_and_protected_derivations() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "reject ambiguous carry", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "shared task must stay in scope",
            priority: "critical",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("both requirements remain implemented"),
        },
    )
    .unwrap();
    let init = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "protected-carry",
            title: "Protected Carry",
        },
    )
    .unwrap();
    fs::write(
        init.package_path.join("requirements").join("README.md"),
        format!(
            "{}\n{}",
            requirement_doc("REQ-001", "Unchanged baseline candidate", "high"),
            requirement_doc("REQ-020", "Protected source correction", "critical")
        ),
    )
    .unwrap();
    let imported = import_design_package(
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
            design_version_id: imported.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    for requirement_key in ["REQ-001", "REQ-020"] {
        derive_task_from_requirement(
            temp.path(),
            NewTaskDerivation {
                design_version_id: imported.design_version_id,
                requirement_key,
                task_id: task.task_id,
                derivation_reason: Some("supported shared-task derivation"),
                checklist_title: Some("Shared protected checklist"),
                item_title: None,
                completion_condition: None,
            },
        )
        .unwrap();
    }
    let authority = add_authority_event(
        temp.path(),
        NewAuthorityEvent {
            event_type: "user_instruction",
            source: Some("test-user"),
            summary: "attempt ambiguous carry",
            scope: Some("project"),
            precedence: 100,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    assert!(
        crate::planning::ensure_verified_baseline_carry_forward(
            &conn,
            crate::db::project_id(&conn).unwrap(),
            task.task_id,
            Some(work.work_unit_id),
            authority.authority_event_id,
        )
        .unwrap_err()
        .to_string()
        .contains("exactly one active design derivation")
    );
    let status: String = conn
        .query_row(
            "select status from tasks where id=?1",
            params![task.task_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "open");
}
