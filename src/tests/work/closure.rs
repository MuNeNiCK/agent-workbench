use super::*;

#[test]
fn close_ready_requires_validation_runs_for_selected_gates() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "close ready validation", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "implement cleanup",
            priority: "high",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("validation run is recorded"),
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
    derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: import.design_version_id,
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
            design_version_id: import.design_version_id,
            gate_key: "GATE-001",
            requirement_key: "REQ-001",
            task_id: task.task_id,
            command: Some("cargo test"),
            command_profile: None,
            timeout: None,
        },
    )
    .unwrap();
    add_repository(
        temp.path(),
        NewRepository {
            name: "main",
            path: ".",
            current_head: Some("abc123"),
            status_summary: Some("clean"),
        },
    )
    .unwrap();
    add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(work.activation_id),
            head_sha: Some("abc123"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();

    let missing = close_ready(temp.path()).unwrap();
    add_validation_run(
        temp.path(),
        NewValidationRun {
            validation_gate_id: gate.validation_gate_id,
            command_usage_id: None,
            repository_snapshot_id: None,
            result: "pass",
            command: None,
            classification: None,
            acceptance_record_id: None,
            artifact_path: None,
            artifact_hash: None,
            notes: Some("validation passed"),
        },
    )
    .unwrap();
    let recorded = close_ready(temp.path()).unwrap();

    assert!(
        missing
            .items
            .iter()
            .any(|item| item.name == "validation_runs_recorded" && item.result == "fail")
    );
    assert!(
        recorded
            .items
            .iter()
            .any(|item| item.name == "validation_runs_recorded" && item.result == "pass")
    );
}

#[test]
fn close_ready_names_derivations_missing_selected_gates() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "missing selected gate detail", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "implement cleanup",
            priority: "high",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("selected gate is visible in close-ready"),
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
    let derivation = derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: import.design_version_id,
            requirement_key: "REQ-001",
            task_id: task.task_id,
            derivation_reason: None,
            checklist_title: None,
            item_title: None,
            completion_condition: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update tasks set status = 'closed', closed_by_commit = 'abc123' where id = ?1",
        params![task.task_id],
    )
    .unwrap();

    let blocked = close_ready(temp.path()).unwrap();
    let item = blocked
        .items
        .iter()
        .find(|item| item.name == "validation_runs_recorded")
        .unwrap();

    assert_eq!(item.result, "fail");
    assert!(item.details.contains("1 missing selected gates"));
    assert!(
        item.details.contains(&format!(
            "task_derivation:{}",
            derivation.task_derivation_id
        )),
        "{item:#?}"
    );
    assert!(
        item.details.contains(&format!("task:{}", task.task_id)),
        "{item:#?}"
    );
    assert!(item.details.contains("requirement:REQ-001"), "{item:#?}");
    assert!(
        item.details
            .contains(&format!("design:{}", import.design_version_id)),
        "{item:#?}"
    );

    conn.execute(
        "update tasks set status = 'accepted_out_of_scope' where id = ?1",
        params![task.task_id],
    )
    .unwrap();
    drop(conn);
    select_validation_gate(
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
    let accepted = close_ready(temp.path()).unwrap();
    assert!(
        accepted
            .items
            .iter()
            .any(|item| item.name == "validation_runs_recorded" && item.result == "pass")
    );
    assert!(
        accepted
            .items
            .iter()
            .any(|item| item.name == "review_plans_clean" && item.result == "pass")
    );
}

#[test]
fn close_ready_allows_explicitly_accepted_validation_failures() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "accepted validation failure", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "implement cleanup",
            priority: "high",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("validation failure is accepted"),
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
    derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: import.design_version_id,
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
            design_version_id: import.design_version_id,
            gate_key: "GATE-001",
            requirement_key: "REQ-001",
            task_id: task.task_id,
            command: Some("cargo test"),
            command_profile: None,
            timeout: None,
        },
    )
    .unwrap();
    add_validation_run(
        temp.path(),
        NewValidationRun {
            validation_gate_id: gate.validation_gate_id,
            command_usage_id: None,
            repository_snapshot_id: None,
            result: "fail",
            command: None,
            classification: None,
            acceptance_record_id: None,
            artifact_path: None,
            artifact_hash: None,
            notes: Some("known external failure"),
        },
    )
    .unwrap();

    let blocked = close_ready(temp.path()).unwrap();
    let approval_authority_event_id = approval_authority_event(temp.path());
    accept_design_exception(
        temp.path(),
        NewDesignExceptionAcceptance {
            design_version_id: Some(import.design_version_id),
            design_package: None,
            target: "gate:GATE-001",
            acceptance_type: "explicit_exception",
            reason: "known external failure accepted by user",
            approval_authority_event_id,
        },
    )
    .unwrap();
    let accepted = close_ready(temp.path()).unwrap();

    assert!(
        blocked
            .items
            .iter()
            .any(|item| item.name == "validation_runs_recorded" && item.result == "fail")
    );
    assert!(
        accepted
            .items
            .iter()
            .any(|item| item.name == "validation_runs_recorded" && item.result == "pass")
    );
}

#[test]
fn close_ready_requires_required_close_plans_to_be_clean() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "close plan work", None).unwrap();
    record_close_prerequisites(temp.path(), &work);
    record_clean_repository_snapshot(temp.path(), &work);
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            review_type: "implementation_review",
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

    let blocked = close_ready(temp.path()).unwrap();
    add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: None,
            prompt_deviations: None,
            result_summary: Some("clean"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: None,
            external_agent_id: Some("test-reviewer"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("test-output"),
        },
    )
    .unwrap();
    let allowed = close_ready(temp.path()).unwrap();

    assert_eq!(blocked.result, "blocked");
    assert!(blocked.items.iter().any(|item| {
        item.name == "review_plans_clean"
            && item.result == "fail"
            && item
                .details
                .contains(&format!("review_plan:{}", plan.review_plan_id))
            && item.details.contains("type:implementation_review")
            && item.details.contains("status:open")
    }));
    assert_eq!(allowed.result, "pass");
    assert!(
        allowed
            .items
            .iter()
            .any(|item| item.name == "review_plans_clean" && item.result == "pass")
    );
}

#[test]
fn close_ready_ignores_retained_superseded_review_history_with_a_bounded_projection() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "bounded close review projection", None).unwrap();
    record_close_prerequisites(temp.path(), &work);
    record_clean_repository_snapshot(temp.path(), &work);
    let task = add_task(
        temp.path(),
        NewTask {
            title: "retained coverage projection",
            priority: "high",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("accepted coverage remains current"),
        },
    )
    .unwrap();
    let design = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "bounded-close-history",
            title: "Bounded Close History",
        },
    )
    .unwrap();
    fs::write(
        design.package_path.join("requirements").join("README.md"),
        requirement_doc("REQ-001", "Retain current coverage", "high"),
    )
    .unwrap();
    let imported = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &design.package_path,
            status: "draft",
        },
    )
    .unwrap();
    derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: imported.design_version_id,
            requirement_key: "REQ-001",
            task_id: task.task_id,
            derivation_reason: None,
            checklist_title: None,
            item_title: None,
            completion_condition: None,
        },
    )
    .unwrap();
    let coverage = add_coverage_item(
        temp.path(),
        NewCoverageItem {
            design_version_id: imported.design_version_id,
            requirement_key: "REQ-001",
            review_scope_id: None,
            work_unit_id: None,
            task_id: Some(task.task_id),
            requirement: "coverage is explicitly out of scope",
            runtime_boundary_evidence: None,
            ux_boundary_evidence: None,
            lifecycle_boundary_evidence: None,
            tests_or_gates: None,
            missing_or_unverified: Some("not applicable"),
            status: "partial",
        },
    )
    .unwrap();
    let current = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            review_type: "implementation_review",
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
    let authority_event_id = approval_authority_event(temp.path());

    let mut conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let review_policy_id: i64 = conn
        .query_row(
            "select review_policy_id from review_plans where id = ?1",
            params![current.review_plan_id],
            |row| row.get(0),
        )
        .unwrap();
    let tx = conn.transaction().unwrap();
    let mut first_historical_plan_id = None;
    for ordinal in 0..512 {
        tx.execute(
            r#"
            insert into review_plans(
                project_id, work_unit_id, review_type, required, stage, scope,
                clean_condition, stop_condition, review_policy_id, status, created_at
            ) values(1, ?1, 'implementation_review', 1, 'close-ready', ?2,
                     'clean', 'block', ?3, 'not_required', current_timestamp)
            "#,
            params![
                work.work_unit_id,
                format!("retained historical plan {ordinal}"),
                review_policy_id
            ],
        )
        .unwrap();
        let historical_plan_id = tx.last_insert_rowid();
        first_historical_plan_id.get_or_insert(historical_plan_id);
        tx.execute(
            r#"
            insert into review_plan_targets(review_plan_id, target_type, work_unit_id)
            values(?1, 'work_unit', ?2)
            "#,
            params![historical_plan_id, work.work_unit_id],
        )
        .unwrap();
        tx.execute(
            r#"
            insert into acceptance_records(
                project_id, target_type, review_plan_id, acceptance_type, reason, scope,
                created_by, status, approved_by_authority_event_id, approved_at, created_at,
                review_impact
            ) values(1, 'review_plan', ?1, 'stale_accepted', 'superseded',
                     'review-plan-supersession', 'user', 'approved', ?2,
                     current_timestamp, current_timestamp, ?3)
            "#,
            params![
                historical_plan_id,
                authority_event_id,
                format!("superseded_by_review_plan:{}", current.review_plan_id)
            ],
        )
        .unwrap();
    }
    tx.execute(
        "update tasks set status = 'closed', closed_by_commit = 'abc123' where id = ?1",
        params![task.task_id],
    )
    .unwrap();
    tx.commit().unwrap();
    drop(conn);
    accept_design_exception(
        temp.path(),
        NewDesignExceptionAcceptance {
            design_version_id: Some(imported.design_version_id),
            design_package: None,
            target: &format!("coverage:{}", coverage.coverage_item_id),
            acceptance_type: "accepted_out_of_scope",
            reason: "coverage is outside this work's runtime boundary",
            approval_authority_event_id: authority_event_id,
        },
    )
    .unwrap();

    let first = close_ready(temp.path()).unwrap();
    let second = close_ready(temp.path()).unwrap();

    assert_eq!(first, second);
    let review = first
        .items
        .iter()
        .find(|item| item.name == "review_plans_clean")
        .unwrap();
    assert!(review.details.starts_with("1 required close-ready plans"));
    assert!(
        review
            .details
            .contains(&format!("review_plan:{}", current.review_plan_id))
    );
    assert!(!review.details.contains(&format!(
        "review_plan:{}",
        first_historical_plan_id.unwrap()
    )));
    let trace = first
        .items
        .iter()
        .find(|item| item.name == "design_trace_closed")
        .unwrap();
    assert!(trace.details.contains("0 missing task coverage"));
    assert!(trace.details.contains("0 missing requirement coverage"));
}

#[test]
fn close_ready_requires_close_repository_comparisons_for_changed_snapshots() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "close comparison work", None).unwrap();
    record_close_prerequisites(temp.path(), &work);
    add_repository(
        temp.path(),
        NewRepository {
            name: "main",
            path: ".",
            current_head: Some("abc123"),
            status_summary: Some("clean"),
        },
    )
    .unwrap();
    let base = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: None,
            head_sha: Some("abc123"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    let current = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(work.activation_id),
            head_sha: Some("def456"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();

    let blocked = close_ready(temp.path()).unwrap();
    add_repository_snapshot_comparison(
        temp.path(),
        NewRepositorySnapshotComparison {
            base_repository_snapshot_id: base.repository_snapshot_id,
            current_repository_snapshot_id: current.repository_snapshot_id,
            comparison_type: "close",
            head_changed: true,
            dirty_state_changed: false,
            nested_repository_changed: false,
            result: "changed_classified",
        },
    )
    .unwrap();
    let allowed = close_ready(temp.path()).unwrap();

    assert_eq!(blocked.result, "blocked");
    assert!(
        blocked
            .items
            .iter()
            .any(|item| item.name == "repository_state_recorded" && item.result == "fail")
    );
    assert_eq!(allowed.result, "pass");
    assert!(
        allowed
            .items
            .iter()
            .any(|item| item.name == "repository_state_recorded" && item.result == "pass")
    );
}

#[test]
fn close_ready_uses_pre_activation_repository_snapshot_as_comparison_base() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "close comparison baseline", None).unwrap();
    record_close_prerequisites(temp.path(), &work);
    add_repository(
        temp.path(),
        NewRepository {
            name: "main",
            path: ".",
            current_head: Some("abc123"),
            status_summary: Some("clean"),
        },
    )
    .unwrap();
    let baseline = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: None,
            head_sha: Some("abc123"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    let active_intermediate = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(work.activation_id),
            head_sha: Some("def456"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    let active_latest = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(work.activation_id),
            head_sha: Some("def456"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    add_repository_snapshot_comparison(
        temp.path(),
        NewRepositorySnapshotComparison {
            base_repository_snapshot_id: active_intermediate.repository_snapshot_id,
            current_repository_snapshot_id: active_latest.repository_snapshot_id,
            comparison_type: "close",
            head_changed: false,
            dirty_state_changed: false,
            nested_repository_changed: false,
            result: "same",
        },
    )
    .unwrap();

    let blocked = close_ready(temp.path()).unwrap();
    add_repository_snapshot_comparison(
        temp.path(),
        NewRepositorySnapshotComparison {
            base_repository_snapshot_id: baseline.repository_snapshot_id,
            current_repository_snapshot_id: active_latest.repository_snapshot_id,
            comparison_type: "close",
            head_changed: true,
            dirty_state_changed: false,
            nested_repository_changed: false,
            result: "changed_classified",
        },
    )
    .unwrap();
    let allowed = close_ready(temp.path()).unwrap();

    assert_eq!(blocked.result, "blocked");
    assert!(
        blocked
            .items
            .iter()
            .any(|item| item.name == "repository_state_recorded" && item.result == "fail")
    );
    assert_eq!(allowed.result, "pass");
    assert!(
        allowed
            .items
            .iter()
            .any(|item| item.name == "repository_state_recorded" && item.result == "pass")
    );
}

#[test]
fn close_ready_ignores_interrupted_child_repository_snapshots_as_baseline() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let parent = start_work(temp.path(), "parent close baseline", None).unwrap();
    record_close_prerequisites(temp.path(), &parent);
    add_repository(
        temp.path(),
        NewRepository {
            name: "main",
            path: ".",
            current_head: Some("abc123"),
            status_summary: Some("clean"),
        },
    )
    .unwrap();
    let baseline = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: None,
            head_sha: Some("abc123"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    let child = interrupt_work(temp.path(), "child work", "check interruption").unwrap();
    let child_snapshot = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(child.child_activation_id),
            head_sha: Some("child456"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    add_repository_snapshot_comparison(
        temp.path(),
        NewRepositorySnapshotComparison {
            base_repository_snapshot_id: baseline.repository_snapshot_id,
            current_repository_snapshot_id: child_snapshot.repository_snapshot_id,
            comparison_type: "close",
            head_changed: true,
            dirty_state_changed: false,
            nested_repository_changed: false,
            result: "changed_classified",
        },
    )
    .unwrap();
    create_work_record(
        temp.path(),
        NewWorkRecord {
            work_unit_id: Some(child.child_work_unit_id),
            topic: "child close evidence",
            work_performed: Some("recorded child close prerequisites"),
            next_actions: None,
            notable_operations: None,
            export_path: None,
        },
    )
    .unwrap();
    close_active_work(temp.path(), "child complete", None).unwrap();
    let check = resume_check(temp.path(), "basic").unwrap();
    resume_work(temp.path(), check.resume_check_id).unwrap();
    let parent_current = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(parent.activation_id),
            head_sha: Some("parent789"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    add_repository_snapshot_comparison(
        temp.path(),
        NewRepositorySnapshotComparison {
            base_repository_snapshot_id: child_snapshot.repository_snapshot_id,
            current_repository_snapshot_id: parent_current.repository_snapshot_id,
            comparison_type: "close",
            head_changed: true,
            dirty_state_changed: false,
            nested_repository_changed: false,
            result: "changed_classified",
        },
    )
    .unwrap();

    let blocked = close_ready(temp.path()).unwrap();
    add_repository_snapshot_comparison(
        temp.path(),
        NewRepositorySnapshotComparison {
            base_repository_snapshot_id: baseline.repository_snapshot_id,
            current_repository_snapshot_id: parent_current.repository_snapshot_id,
            comparison_type: "close",
            head_changed: true,
            dirty_state_changed: false,
            nested_repository_changed: false,
            result: "changed_classified",
        },
    )
    .unwrap();
    let allowed = close_ready(temp.path()).unwrap();

    assert_eq!(blocked.result, "blocked");
    assert!(
        blocked
            .items
            .iter()
            .any(|item| item.name == "repository_state_recorded" && item.result == "fail")
    );
    assert_eq!(allowed.result, "pass");
    assert!(
        allowed
            .items
            .iter()
            .any(|item| item.name == "repository_state_recorded" && item.result == "pass")
    );
}

#[test]
fn linked_commit_subject_vocabulary_does_not_affect_close_readiness() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "commit policy work", None).unwrap();
    let record = create_work_record(
        temp.path(),
        NewWorkRecord {
            work_unit_id: Some(work.work_unit_id),
            topic: "commit evidence",
            work_performed: Some("recorded commit evidence"),
            next_actions: None,
            notable_operations: None,
            export_path: None,
        },
    )
    .unwrap();
    add_repository(
        temp.path(),
        NewRepository {
            name: "main",
            path: ".",
            current_head: Some("abc123"),
            status_summary: Some("clean"),
        },
    )
    .unwrap();
    let valid = add_git_commit(
        temp.path(),
        NewGitCommit {
            repository: "main",
            commit_sha: "abc123",
            short_sha: Some("abc123"),
            subject: Some("fix: valid message"),
            author_name: None,
            author_email: None,
            committed_at: None,
            parent_shas: None,
        },
    )
    .unwrap();
    add_work_record_git_commit(
        temp.path(),
        NewWorkRecordGitCommit {
            work_record_id: record.work_record_id,
            git_commit_id: Some(valid.git_commit_id),
            commit_sha: "abc123",
            role: "created",
            note: None,
        },
    )
    .unwrap();

    let allowed = close_ready(temp.path()).unwrap();

    for (sha, subject) in [
        ("def456", "review"),
        ("ghi789", "Phase 42 review: 日本語の目的を記録"),
    ] {
        let commit = add_git_commit(
            temp.path(),
            NewGitCommit {
                repository: "main",
                commit_sha: sha,
                short_sha: Some(sha),
                subject: Some(subject),
                author_name: None,
                author_email: None,
                committed_at: None,
                parent_shas: None,
            },
        )
        .unwrap();
        add_work_record_git_commit(
            temp.path(),
            NewWorkRecordGitCommit {
                work_record_id: record.work_record_id,
                git_commit_id: Some(commit.git_commit_id),
                commit_sha: sha,
                role: "created",
                note: None,
            },
        )
        .unwrap();
    }
    let after = close_ready(temp.path()).unwrap();

    assert_eq!(allowed.result, after.result);
    assert!(
        after
            .items
            .iter()
            .all(|item| item.name != "commit_messages_checked")
    );
    assert!(after.items.iter().any(|item| {
        item.name == "work_record_recorded" && item.details.contains("3 linked evidence rows")
    }));
}
