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
            external_agent_id: None,
            review_provenance: "self_recorded",
            review_provenance_ref: None,
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
fn close_ready_blocks_invalid_linked_commit_messages() {
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

    let invalid = add_git_commit(
        temp.path(),
        NewGitCommit {
            repository: "main",
            commit_sha: "def456",
            short_sha: Some("def456"),
            subject: Some("fix: review feedback"),
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
            git_commit_id: Some(invalid.git_commit_id),
            commit_sha: "def456",
            role: "created",
            note: None,
        },
    )
    .unwrap();
    let blocked = close_ready(temp.path()).unwrap();

    assert!(
        allowed
            .items
            .iter()
            .any(|item| item.name == "commit_messages_checked" && item.result == "pass")
    );
    assert!(
        blocked
            .items
            .iter()
            .any(|item| item.name == "commit_messages_checked" && item.result == "fail")
    );
}
