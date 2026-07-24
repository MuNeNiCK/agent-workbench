use super::super::*;

#[test]
fn completed_derivation_rebind_is_owned_audited_and_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "repair completed trace", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "implement aggregate behavior",
            priority: "high",
            source: "design",
            work_unit_id: None,
            details: Some("one aggregate task"),
            completion_condition: Some("all completion boundaries hold"),
        },
    )
    .unwrap();
    let package = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "completed-trace-rebind",
            title: "Completed Trace Rebind",
        },
    )
    .unwrap();
    fs::write(
        package.package_path.join("requirements/README.md"),
        format!(
            "{}\n{}",
            requirement_doc("REQ-001", "First completion boundary", "high"),
            requirement_doc("REQ-002", "Second completion boundary", "high")
        ),
    )
    .unwrap();
    fs::write(
        package.package_path.join("validation/gates.md"),
        validation_gate_doc("GATE-001"),
    )
    .unwrap();
    let design = import_design_package(
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
            design_version_id: design.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    let first = derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: design.design_version_id,
            requirement_key: "REQ-001",
            task_id: task.task_id,
            derivation_reason: Some("initial decomposition"),
            checklist_title: Some("aggregate"),
            item_title: Some("first-boundary"),
            completion_condition: Some("first boundary holds"),
        },
    )
    .unwrap();
    let second = derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: design.design_version_id,
            requirement_key: "REQ-002",
            task_id: task.task_id,
            derivation_reason: Some("initial decomposition"),
            checklist_title: Some("aggregate"),
            item_title: Some("second-boundary"),
            completion_condition: Some("second boundary holds"),
        },
    )
    .unwrap();
    let other_task = add_task(
        temp.path(),
        NewTask {
            title: "unrelated completed behavior",
            priority: "high",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("unrelated boundary holds"),
        },
    )
    .unwrap();
    let other = derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: design.design_version_id,
            requirement_key: "REQ-001",
            task_id: other_task.task_id,
            derivation_reason: Some("unrelated decomposition"),
            checklist_title: Some("unrelated"),
            item_title: Some("unrelated-boundary"),
            completion_condition: Some("unrelated boundary holds"),
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update checklist_items set status='closed' where task_id=?1",
        [task.task_id],
    )
    .unwrap();
    conn.execute(
        "update tasks set status='closed' where id=?1",
        [task.task_id],
    )
    .unwrap();
    conn.execute(
        "update checklist_items set status='closed' where task_id=?1",
        [other_task.task_id],
    )
    .unwrap();
    conn.execute(
        "update tasks set status='closed' where id=?1",
        [other_task.task_id],
    )
    .unwrap();
    drop(conn);
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(design.design_version_id),
            review_type: "design_implementation_diff",
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
            target_ref: Some("review-context:design-implementation-diff:design=1:work=1"),
            prompt_deviations: None,
            result_summary: Some("completed derivation targets the wrong boundary"),
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
    let finding = add_finding_with_targets(
        temp.path(),
        NewFinding {
            review_run_id: run.review_run_id,
            finding_type: "design_implementation_drift",
            severity: "high",
            description: "rebind the completed derivation",
            design_requirement_id: None,
            task_id: None,
        },
        &[FindingTargetInput {
            design_requirement_id: Some(second.design_requirement_id),
            task_id: Some(task.task_id),
        }],
    )
    .unwrap();
    classify_finding(temp.path(), finding.finding_id, "valid").unwrap();
    let closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: finding.finding_id,
            design_invariant: "each requirement names its establishing boundary",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some("managed trace derivation"),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("rebind only the selected derivation"),
            tests_or_gates: Some("exact derivation list"),
            verification_plan: Some("independent trace review"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    remediate_work(temp.path(), finding.finding_id).unwrap();

    let rebind_second_to_first = || TaskDerivationRebind {
        design_version_id: design.design_version_id,
        requirement_key: "REQ-002",
        task_id: task.task_id,
        checklist_item_id: first.checklist_item_id,
        closure_id: closure.closure_id,
        reason: "bind the second requirement to its establishing shared boundary",
    };
    let assert_lifecycle_rejected = || {
        assert!(
            rebind_task_derivation(temp.path(), rebind_second_to_first())
                .unwrap_err()
                .to_string()
                .contains(
                    "target checklist item is outside the remediation owner, design, task, or lifecycle"
                )
        );
    };
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update checklists set status='closed' where id=?1",
        [first.checklist_id],
    )
    .unwrap();
    conn.execute("update tasks set status='open' where id=?1", [task.task_id])
        .unwrap();
    drop(conn);
    assert_lifecycle_rejected();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update checklists set status='active' where id=?1",
        [first.checklist_id],
    )
    .unwrap();
    conn.execute(
        "update checklist_items set status='open' where id=?1",
        [first.checklist_item_id],
    )
    .unwrap();
    conn.execute(
        "update tasks set status='blocked' where id=?1",
        [task.task_id],
    )
    .unwrap();
    drop(conn);
    assert_lifecycle_rejected();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update checklist_items set status='blocked' where id=?1",
        [first.checklist_item_id],
    )
    .unwrap();
    conn.execute("update tasks set status='open' where id=?1", [task.task_id])
        .unwrap();
    drop(conn);
    assert_lifecycle_rejected();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update checklists set status='active' where id=?1",
        [first.checklist_id],
    )
    .unwrap();
    conn.execute(
        "update checklist_items set status='closed' where task_id=?1",
        [task.task_id],
    )
    .unwrap();
    conn.execute(
        "update tasks set status='closed' where id=?1",
        [task.task_id],
    )
    .unwrap();
    drop(conn);

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let sealed = conn.execute(
        "insert into finding_targets(project_id,finding_id,ordinal,design_requirement_id,task_id,created_at) select project_id,id,2,?1,null,current_timestamp from findings where id=?2",
        params![
            second.design_requirement_id,
            finding.finding_id
        ],
    );
    assert!(
        sealed
            .unwrap_err()
            .to_string()
            .contains("target set is sealed")
    );
    drop(conn);

    let wrong_requirement = rebind_task_derivation(
        temp.path(),
        TaskDerivationRebind {
            design_version_id: design.design_version_id,
            requirement_key: "REQ-001",
            task_id: task.task_id,
            checklist_item_id: first.checklist_item_id,
            closure_id: closure.closure_id,
            reason: "must not cross the finding requirement boundary",
        },
    )
    .unwrap_err();
    assert!(
        wrong_requirement
            .to_string()
            .contains("does not authorize this task derivation rebind")
    );
    let wrong_task = rebind_task_derivation(
        temp.path(),
        TaskDerivationRebind {
            design_version_id: design.design_version_id,
            requirement_key: "REQ-001",
            task_id: other_task.task_id,
            checklist_item_id: other.checklist_item_id,
            closure_id: closure.closure_id,
            reason: "must not cross the finding task boundary",
        },
    )
    .unwrap_err();
    assert!(
        wrong_task
            .to_string()
            .contains("does not authorize this task derivation rebind")
    );

    let rebound = rebind_task_derivation(temp.path(), rebind_second_to_first()).unwrap();
    assert_eq!(rebound.task_derivation_id, second.task_derivation_id);
    assert_eq!(rebound.previous_checklist_item_id, second.checklist_item_id);
    assert_eq!(rebound.checklist_item_id, first.checklist_item_id);
    assert!(!rebound.idempotent);
    assert!(
        rebind_task_derivation(temp.path(), rebind_second_to_first())
            .unwrap()
            .idempotent
    );

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let stored: (i64, i64, String) = conn
        .query_row(
            r#"
            select derivation.checklist_item_id,count(event.id),max(event.text_or_summary)
            from task_derivations derivation
            join authority_events event
              on event.source='trace derivation rebind'
             and event.scope='work-unit:1'
            where derivation.id=?1
            "#,
            [second.task_derivation_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(stored.0, first.checklist_item_id);
    assert_eq!(stored.1, 1);
    assert!(stored.2.contains(&format!(
        "from checklist item {} to checklist item {}",
        second.checklist_item_id, first.checklist_item_id
    )));
    drop(conn);

    let rejected = decide_finding(
        temp.path(),
        finding.finding_id,
        AdjudicationInput {
            decision: "rejected",
            reason: "must not discard an applied trace repair",
            expected_current: "pending",
        },
    )
    .unwrap_err();
    assert!(
        rejected
            .to_string()
            .contains("finding_has_active_remediation_effects")
    );
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let preserved: (String, String, String, i64, i64) = conn
        .query_row(
            r#"
            select f.classification,f.status,c.status,d.checklist_item_id,
                   count(event.id)
            from findings f
            join closures c on c.finding_id=f.id
            join task_derivations d on d.id=?2
            left join authority_events event
              on event.project_id=f.project_id
             and event.source='trace derivation rebind'
             and event.status='active'
             and event.text_or_summary like
               'closure ' || c.id || ' rebinds task derivation %'
            where f.id=?1
            "#,
            params![finding.finding_id, second.task_derivation_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        preserved,
        (
            "valid".into(),
            "open".into(),
            "registered".into(),
            first.checklist_item_id,
            1
        )
    );
}

#[test]
fn task_derivation_creates_checklist_trace_and_unblocks_implementation_ready() {
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
    let import = import_design_package(
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
            design_version_id: import.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    let blocked = implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(import.design_version_id),
        },
    )
    .unwrap();

    let derivation = derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: import.design_version_id,
            requirement_key: "REQ-001",
            task_id: task.task_id,
            derivation_reason: Some("design task decomposition"),
            checklist_title: None,
            item_title: None,
            completion_condition: None,
        },
    )
    .unwrap();
    assert!(
        accept_task_out_of_scope(temp.path(), task.task_id, "must use verified carry-forward")
            .unwrap_err()
            .to_string()
            .contains("verified baseline proof")
    );
    let blocked_without_gate = implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(import.design_version_id),
        },
    )
    .unwrap();
    let gate = select_validation_gate(
        temp.path(),
        ValidationGateSelection {
            design_version_id: import.design_version_id,
            gate_key: "GATE-001",
            requirement_key: "REQ-001",
            task_id: task.task_id,
            command: None,
            command_profile: None,
            timeout: None,
        },
    )
    .unwrap();
    let implementation_review_plan_id = add_implementation_ready_review_plan(
        temp.path(),
        work.work_unit_id,
        import.design_version_id,
    );
    let missing_context_run = add_clean_review_run_result(
        temp.path(),
        implementation_review_plan_id,
        None,
        "clean decomposition review without context",
    );
    assert!(missing_context_run.is_err());
    let blocked_without_review_context = implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(import.design_version_id),
        },
    )
    .unwrap();
    add_clean_review_run(
        temp.path(),
        implementation_review_plan_id,
        Some(&format!(
            "review-context:design-task-decomposition:design={}:work={}",
            import.design_version_id, work.work_unit_id
        )),
        "clean decomposition review",
    );
    let passed = implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(import.design_version_id),
        },
    )
    .unwrap();
    let close_without_trace = close_task(temp.path(), task.task_id, Some("abc123"));
    let task_only_evidence = add_implementation_evidence(
        temp.path(),
        NewImplementationEvidence {
            task_id: Some(task.task_id),
            design_version_id: None,
            requirement_key: None,
            evidence_type: "commit",
            commit_sha: Some("task-only"),
            file_path: None,
            line_ref: None,
            symbol: None,
            artifact_path: None,
            note: None,
        },
    );
    let design_evidence_before_requirement_link = list_implementation_evidence(
        temp.path(),
        ImplementationEvidenceListQuery {
            task_id: None,
            design_version_id: Some(import.design_version_id),
            work_unit_id: None,
            evidence_type: None,
        },
    )
    .unwrap();
    let superseded_gap = add_coverage_item(
        temp.path(),
        NewCoverageItem {
            design_version_id: import.design_version_id,
            requirement_key: "REQ-001",
            review_scope_id: None,
            work_unit_id: None,
            task_id: Some(task.task_id),
            requirement: "cleanup behavior still needs implementation evidence",
            runtime_boundary_evidence: None,
            ux_boundary_evidence: None,
            lifecycle_boundary_evidence: None,
            tests_or_gates: None,
            missing_or_unverified: Some("implementation evidence required"),
            status: "needs_evidence",
        },
    )
    .unwrap();
    let coverage = add_coverage_item(
        temp.path(),
        NewCoverageItem {
            design_version_id: import.design_version_id,
            requirement_key: "REQ-001",
            review_scope_id: None,
            work_unit_id: None,
            task_id: Some(task.task_id),
            requirement: "cleanup behavior is connected to implementation and tests",
            runtime_boundary_evidence: Some("cleanup path preserves lifecycle behavior"),
            ux_boundary_evidence: None,
            lifecycle_boundary_evidence: Some("storage lifecycle remains intact"),
            tests_or_gates: Some("GATE-001"),
            missing_or_unverified: None,
            status: "covered",
        },
    )
    .unwrap();
    let close_without_requirement_evidence = close_task(temp.path(), task.task_id, Some("abc123"));
    let evidence = add_implementation_evidence(
        temp.path(),
        NewImplementationEvidence {
            task_id: Some(task.task_id),
            design_version_id: Some(import.design_version_id),
            requirement_key: Some("REQ-001"),
            evidence_type: "commit",
            commit_sha: Some("abc123"),
            file_path: None,
            line_ref: None,
            symbol: None,
            artifact_path: None,
            note: None,
        },
    )
    .unwrap();
    let evidence_records = list_implementation_evidence(
        temp.path(),
        ImplementationEvidenceListQuery {
            task_id: None,
            design_version_id: Some(import.design_version_id),
            work_unit_id: None,
            evidence_type: None,
        },
    )
    .unwrap();
    let coverage_records = list_coverage_items(
        temp.path(),
        CoverageItemListQuery {
            design_version_id: import.design_version_id,
            status: Some("covered"),
            work_unit_id: None,
        },
    )
    .unwrap();
    let stale_coverage = list_coverage_items(
        temp.path(),
        CoverageItemListQuery {
            design_version_id: import.design_version_id,
            status: Some("stale"),
            work_unit_id: None,
        },
    )
    .unwrap();
    let review_context = render_review_context(
        temp.path(),
        ReviewContextQuery {
            kind: "implementation-review",
            design_version_id: Some(import.design_version_id),
            work_unit_id: Some(work.work_unit_id),
            phase_id: None,
        },
    )
    .unwrap();
    close_task(temp.path(), task.task_id, Some("abc123")).unwrap();
    let passed_after_close = implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(import.design_version_id),
        },
    )
    .unwrap();
    let checklist_items = list_checklist_items(
        temp.path(),
        ChecklistItemListQuery {
            checklist_id: Some(derivation.checklist_id),
            status: Some("open"),
        },
    )
    .unwrap();
    let premature_checklist_close = close_checklist(temp.path(), derivation.checklist_id);
    let close_blocked_by_checklist = close_ready(temp.path()).unwrap();
    close_checklist_item(temp.path(), derivation.checklist_item_id).unwrap();
    close_checklist(temp.path(), derivation.checklist_id).unwrap();
    let close_blocked_without_reviews = close_ready(temp.path()).unwrap();
    let completed_usage = add_command_usage(
        temp.path(),
        NewCommandUsage {
            profile: None,
            command: Some("manual GATE-001 validation"),
            result: "pass",
            log_path: None,
            work_unit_id: Some(work.work_unit_id),
        },
    )
    .unwrap();
    add_validation_run(
        temp.path(),
        NewValidationRun {
            validation_gate_id: gate.validation_gate_id,
            command_usage_id: Some(completed_usage.command_usage_id),
            repository_snapshot_id: None,
            result: "pass",
            command: None,
            classification: None,
            acceptance_record_id: None,
            artifact_path: None,
            artifact_hash: None,
            notes: Some("design gate passed"),
        },
    )
    .unwrap();
    let close_review_plans =
        add_close_ready_review_plans(temp.path(), work.work_unit_id, import.design_version_id);
    let missing_close_context_runs = add_clean_close_ready_review_runs(
        temp.path(),
        work.work_unit_id,
        import.design_version_id,
        &close_review_plans,
        false,
    );
    assert!(missing_close_context_runs.iter().all(Result::is_err));
    let close_blocked_without_context = close_ready(temp.path()).unwrap();
    add_clean_close_ready_review_runs(
        temp.path(),
        work.work_unit_id,
        import.design_version_id,
        &close_review_plans,
        true,
    );
    record_close_evidence(temp.path(), work.work_unit_id, work.activation_id);
    let close_passed = close_ready(temp.path()).unwrap();
    let records = list_task_derivations(
        temp.path(),
        TaskDerivationListQuery {
            design_version_id: import.design_version_id,
            work_unit_id: None,
        },
    )
    .unwrap();

    assert_eq!(blocked.result, "blocked");
    assert!(
        blocked
            .items
            .iter()
            .any(|item| { item.name == "task_derivations_exist" && item.result == "fail" })
    );
    assert_eq!(derivation.task_id, task.task_id);
    assert_eq!(gate.task_id, task.task_id);
    assert_eq!(blocked_without_gate.result, "blocked");
    assert!(
        blocked_without_gate
            .items
            .iter()
            .any(|item| { item.name == "validation_gates_selected" && item.result == "fail" })
    );
    assert!(blocked_without_review_context.items.iter().any(|item| {
        item.name == "pre_implementation_reviews_clean"
            && item.result == "fail"
            && item
                .detail
                .as_deref()
                .is_some_and(|details| details.contains("missing review-context runs"))
    }));
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].requirement_key, "REQ-001");
    assert_eq!(passed.result, "pass", "{:#?}", passed.items);
    assert!(close_without_trace.is_err());
    assert!(task_only_evidence.is_err());
    assert!(design_evidence_before_requirement_link.is_empty());
    assert!(close_without_requirement_evidence.is_err());
    assert_eq!(evidence.task_id, Some(task.task_id));
    assert_eq!(evidence_records.len(), 1);
    assert_eq!(
        evidence_records[0].requirement_key.as_deref(),
        Some("REQ-001")
    );
    assert_eq!(evidence_records[0].commit_sha.as_deref(), Some("abc123"));
    assert_eq!(passed_after_close.result, "pass");
    assert_eq!(checklist_items.len(), 1);
    assert!(premature_checklist_close.is_err());
    assert!(close_blocked_by_checklist.items.iter().any(|item| {
        item.name == "design_trace_closed"
            && item.result == "fail"
            && item.details.contains("1 open checklist items")
            && item.details.contains("1 active checklists")
    }));
    assert_eq!(close_blocked_without_reviews.result, "blocked");
    assert!(close_blocked_without_reviews.items.iter().any(|item| {
        item.name == "review_plans_clean"
            && item.result == "fail"
            && item.details.contains("design_implementation_diff")
            && item.details.contains("implementation_review")
    }));
    assert!(close_blocked_without_context.items.iter().any(|item| {
        item.name == "review_plans_clean"
            && item.result == "fail"
            && item.details.contains("missing review-context runs")
            && item
                .details
                .contains("missing_context:design-implementation-diff")
            && item
                .details
                .contains("missing_context:implementation-review")
            && item.details.contains(&format!(
                "context_ref:review-context:implementation-review:design={}:work={}",
                import.design_version_id, work.work_unit_id
            ))
    }));
    assert_eq!(close_passed.result, "pass", "{:#?}", close_passed.items);
    assert_eq!(coverage.task_id, Some(task.task_id));
    assert_eq!(coverage_records.len(), 1);
    assert_eq!(coverage_records[0].requirement_key, "REQ-001");
    assert_eq!(coverage_records[0].status, "covered");
    assert_eq!(stale_coverage.len(), 1);
    assert_eq!(stale_coverage[0].id, superseded_gap.coverage_item_id);
    assert!(
        !review_context
            .text
            .contains("implementation evidence required")
    );
    assert!(review_context.text.contains("known_gaps:\n- none"));
    assert!(!review_context.text.contains(&format!(
        "coverage_item:{}",
        superseded_gap.coverage_item_id
    )));
}
