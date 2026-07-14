use super::*;

#[test]
fn review_plan_targets_reject_cross_project_targets() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "target integrity", None).unwrap();
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
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into projects(name, root_path, created_at, updated_at) values ('other', '/tmp/other-awb-target', current_timestamp, current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        "insert into work_units(project_id, title, status, started_at) values (2, 'other work', 'open', current_timestamp)",
        [],
    )
    .unwrap();

    let cross_project = conn.execute(
        r#"
        insert into review_plan_targets(review_plan_id, target_type, work_unit_id)
        values (?1, 'work_unit', 2)
        "#,
        params![plan.review_plan_id],
    );
    let repository_snapshot_target = conn.execute(
        r#"
        insert into review_plan_targets(review_plan_id, target_type, repository_snapshot_id)
        values (?1, 'repository_snapshot', 1)
        "#,
        params![plan.review_plan_id],
    );

    assert!(cross_project.is_err());
    assert!(repository_snapshot_target.is_err());
}

#[test]
fn review_plan_targets_can_drive_typed_review_run_targets() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "typed targets", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "targeted task",
            priority: "medium",
            source: "user",
            work_unit_id: None,
            details: None,
            completion_condition: Some("target is reviewed"),
        },
    )
    .unwrap();
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
    let target = add_review_plan_target(
        temp.path(),
        NewReviewPlanTarget {
            review_plan_id: plan.review_plan_id,
            target_type: "task",
            design_version_id: None,
            design_requirement_id: None,
            task_id: Some(task.task_id),
            work_unit_id: None,
            phase_id: None,
            repository_snapshot_id: None,
            file_path: None,
            symbol: None,
        },
    )
    .unwrap();

    let run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some(&format!("task:{}", task.task_id)),
            prompt_deviations: None,
            result_summary: Some("task target clean"),
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
    let records = list_review_plan_targets(temp.path(), plan.review_plan_id).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let (target_type, task_id): (String, i64) = conn
        .query_row(
            "select target_type, task_id from review_runs where id = ?1",
            params![run.review_run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(records.len(), 2);
    assert!(
        records
            .iter()
            .any(|record| record.id == target.review_plan_target_id)
    );
    assert_eq!(target_type, "task");
    assert_eq!(task_id, task.task_id);
}

#[test]
fn phase_targeted_plan_uses_its_phase_review_context() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "phase review context", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "phase task",
            priority: "medium",
            source: "user",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("phase is reviewed"),
        },
    )
    .unwrap();
    let phase = create_phase(
        temp.path(),
        NewWorkPhase {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            key: "reviewed-phase",
            title: "Reviewed Phase",
            kind: "milestone",
            order: 1,
            reason: None,
        },
    )
    .unwrap();
    assign_task_to_phase(temp.path(), phase.phase_id, task.task_id).unwrap();
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            review_type: "implementation_review",
            required: true,
            stage: "close-ready",
            scope: Some("one phase"),
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    add_review_plan_target(
        temp.path(),
        NewReviewPlanTarget {
            review_plan_id: plan.review_plan_id,
            target_type: "phase",
            design_version_id: None,
            design_requirement_id: None,
            task_id: None,
            work_unit_id: None,
            phase_id: Some(phase.phase_id),
            repository_snapshot_id: None,
            file_path: None,
            symbol: None,
        },
    )
    .unwrap();
    add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("review-context:implementation-review:design=-:work=1:phase=1"),
            prompt_deviations: None,
            result_summary: Some("phase clean"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: Some("phase-reviewer"),
            external_agent_id: Some("phase-reviewer"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("phase-review-output"),
        },
    )
    .unwrap();

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let context_ref = crate::review_context::review_context_ref_with_phase(
        "implementation-review",
        None,
        Some(work.work_unit_id),
        Some(phase.phase_id),
    );
    assert_eq!(
        context_ref,
        "review-context:implementation-review:design=-:work=1:phase=1"
    );
    assert!(
        crate::review_context::review_plan_has_clean_context_run(
            &conn,
            plan.review_plan_id,
            "implementation-review",
            None,
            Some(work.work_unit_id),
        )
        .unwrap()
    );
}

#[test]
fn file_review_run_targets_are_stored_in_typed_columns() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "file target", None).unwrap();
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
    add_review_plan_target(
        temp.path(),
        NewReviewPlanTarget {
            review_plan_id: plan.review_plan_id,
            target_type: "file",
            design_version_id: None,
            design_requirement_id: None,
            task_id: None,
            work_unit_id: None,
            phase_id: None,
            repository_snapshot_id: None,
            file_path: Some("src/lib.rs"),
            symbol: None,
        },
    )
    .unwrap();
    let run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("file:src/lib.rs"),
            prompt_deviations: None,
            result_summary: Some("file clean"),
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
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let (target_type, file_path, target_ref): (String, String, String) = conn
        .query_row(
            "select target_type, file_path, target_ref from review_runs where id = ?1",
            params![run.review_run_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();

    assert_eq!(target_type, "file");
    assert_eq!(file_path, "src/lib.rs");
    assert_eq!(target_ref, "file:src/lib.rs");
}
