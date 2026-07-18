use super::*;

#[test]
fn reconcile_closes_duplicate_current_derivation() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "duplicate decomposition", None).unwrap();
    let package = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "reconciliation",
            title: "Reconciliation",
        },
    )
    .unwrap();
    fs::write(
        package.package_path.join("requirements/README.md"),
        requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
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
    let review = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(design.design_version_id),
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
    add_clean_review_run(
        temp.path(),
        review.review_plan_id,
        Some(&format!(
            "review-context:design-review:design={}:work={}",
            design.design_version_id, work.work_unit_id
        )),
        "design ready",
    );
    let canonical = decompose_design(
        temp.path(),
        DesignDecomposition {
            design_version_id: design.design_version_id,
            work_unit_id: work.work_unit_id,
            checklist_title: Some("canonical"),
            reason: Some("initial decomposition"),
        },
    )
    .unwrap();
    let duplicate_task = add_task(
        temp.path(),
        NewTask {
            title: "duplicate cleanup task",
            priority: "high",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("duplicate is superseded"),
        },
    )
    .unwrap();
    let duplicate = derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: design.design_version_id,
            requirement_key: "REQ-001",
            task_id: duplicate_task.task_id,
            derivation_reason: Some("legacy duplicate"),
            checklist_title: Some("duplicate"),
            item_title: None,
            completion_condition: None,
        },
    )
    .unwrap();

    let phase = create_phase(
        temp.path(),
        NewWorkPhase {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            key: "implementation",
            title: "Implementation",
            kind: "implementation",
            order: 1,
            reason: Some("canonical task is already scheduled"),
        },
    )
    .unwrap();
    let canonical_task: i64 = open_existing_project(temp.path())
        .unwrap()
        .query_row(
            "select task_id from checklist_items where checklist_id=?1",
            params![canonical.checklist_id],
            |row| row.get(0),
        )
        .unwrap();
    assign_task_to_phase(temp.path(), phase.phase_id, canonical_task).unwrap();

    let conn = open_existing_project(temp.path()).unwrap();
    conn.execute(
        "update validation_gates set status='closed' where task_id=?1",
        params![canonical_task],
    )
    .unwrap();
    let before: i64 = conn
        .query_row(
            "select count(*) from task_derivations td join design_requirements r on r.id=td.design_requirement_id where r.design_version_id=?1 and td.status='active'",
            params![design.design_version_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(before, 2);

    let outcome = crate::traceability::reconcile_design_in(
        &conn,
        1,
        design.design_version_id,
        work.work_unit_id,
        canonical.checklist_id,
        "remove duplicate derivation",
    )
    .unwrap();
    assert_eq!(outcome.checklist_id, canonical.checklist_id);

    let duplicate_state: (String, String, String) = conn
        .query_row(
            "select td.status,ci.status,c.status from task_derivations td join checklist_items ci on ci.id=td.checklist_item_id join checklists c on c.id=ci.checklist_id where td.id=?1",
            params![duplicate.task_derivation_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        duplicate_state,
        (
            "closed".to_string(),
            "closed".to_string(),
            "closed".to_string()
        )
    );
    let remaining: i64 = conn
        .query_row(
            "select count(*) from task_derivations td join design_requirements r on r.id=td.design_requirement_id where r.design_version_id=?1 and td.status='active'",
            params![design.design_version_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(remaining, 1);
    assert_eq!(
        conn.query_row(
            "select count(*) from current_task_validation_gates where task_id=?1",
            params![canonical_task],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row(
            "select phase_id from work_phase_task_memberships where task_id=?1",
            params![canonical_task],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        phase.phase_id
    );
}

#[test]
fn reconcile_retires_predecessor_checklist_without_closing_shared_task_current_gate() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "shared task successor decomposition", None).unwrap();
    let package = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "shared-task-reconciliation",
            title: "Shared Task Reconciliation",
        },
    )
    .unwrap();
    fs::write(
        package.package_path.join("requirements/README.md"),
        requirement_doc("REQ-001", "Preserve one shared task", "high"),
    )
    .unwrap();
    fs::write(
        package.package_path.join("validation/gates.md"),
        validation_gate_doc("GATE-001"),
    )
    .unwrap();

    let predecessor = import_design_package(
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
            design_version_id: predecessor.design_version_id,
            summary: Some("approve predecessor"),
        },
    )
    .unwrap();
    let predecessor_review = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(predecessor.design_version_id),
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
    add_clean_review_run(
        temp.path(),
        predecessor_review.review_plan_id,
        Some(&format!(
            "review-context:design-review:design={}:work={}",
            predecessor.design_version_id, work.work_unit_id
        )),
        "predecessor clean",
    );
    let predecessor_decomposition = decompose_design(
        temp.path(),
        DesignDecomposition {
            design_version_id: predecessor.design_version_id,
            work_unit_id: work.work_unit_id,
            checklist_title: Some("predecessor checklist"),
            reason: Some("predecessor decomposition"),
        },
    )
    .unwrap();
    let predecessor_state = open_existing_project(temp.path()).unwrap();
    let shared_task: i64 = predecessor_state
        .query_row(
            "select task_id from checklist_items where checklist_id=?1",
            params![predecessor_decomposition.checklist_id],
            |row| row.get(0),
        )
        .unwrap();
    let predecessor_requirement: i64 = predecessor_state
        .query_row(
            "select design_requirement_id from checklist_items where checklist_id=?1",
            params![predecessor_decomposition.checklist_id],
            |row| row.get(0),
        )
        .unwrap();
    drop(predecessor_state);
    let phase = create_phase(
        temp.path(),
        NewWorkPhase {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(predecessor.design_version_id),
            key: "implementation",
            title: "Implementation",
            kind: "implementation",
            order: 1,
            reason: Some("schedule shared task before successor refresh"),
        },
    )
    .unwrap();
    assign_task_to_phase(temp.path(), phase.phase_id, shared_task).unwrap();

    fs::write(
        package.package_path.join("01-introduction-goals.md"),
        "# Introduction And Goals\n\nSuccessor package with unchanged requirements.\n",
    )
    .unwrap();
    let successor = import_design_package(
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
            design_version_id: successor.design_version_id,
            summary: Some("approve successor"),
        },
    )
    .unwrap();
    let successor_review = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(successor.design_version_id),
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
    add_clean_review_run(
        temp.path(),
        successor_review.review_plan_id,
        Some(&format!(
            "review-context:design-review:design={}:work={}",
            successor.design_version_id, work.work_unit_id
        )),
        "successor clean",
    );
    let successor_decomposition = decompose_design(
        temp.path(),
        DesignDecomposition {
            design_version_id: successor.design_version_id,
            work_unit_id: work.work_unit_id,
            checklist_title: Some("successor checklist"),
            reason: Some("successor decomposition"),
        },
    )
    .unwrap();

    let conn = open_existing_project(temp.path()).unwrap();
    let (successor_requirement, successor_task): (i64, i64) = conn
        .query_row(
            "select design_requirement_id,task_id from checklist_items where checklist_id=?1",
            params![successor_decomposition.checklist_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(successor_task, shared_task);
    assert_eq!(
        conn.query_row(
            "select count(*) from validation_gates where task_id=?1 and status='active'",
            params![shared_task],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        2
    );

    crate::traceability::reconcile_design_in(
        &conn,
        1,
        successor.design_version_id,
        work.work_unit_id,
        successor_decomposition.checklist_id,
        "retire shared-task predecessor decomposition",
    )
    .unwrap();

    let predecessor_bundle: (String, String, String, String) = conn
        .query_row(
            "select td.status,ci.status,c.status,vg.status from task_derivations td join checklist_items ci on ci.id=td.checklist_item_id join checklists c on c.id=ci.checklist_id join validation_gates vg on vg.task_id=td.task_id and vg.design_requirement_id=td.design_requirement_id where td.design_requirement_id=?1",
            params![predecessor_requirement],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        predecessor_bundle,
        (
            "closed".to_string(),
            "closed".to_string(),
            "closed".to_string(),
            "closed".to_string(),
        )
    );
    let successor_bundle: (String, String, String, i64) = conn
        .query_row(
            "select td.status,ci.status,c.status,(select count(*) from current_task_validation_gates vg where vg.task_id=td.task_id and vg.design_requirement_id=td.design_requirement_id) from task_derivations td join checklist_items ci on ci.id=td.checklist_item_id join checklists c on c.id=ci.checklist_id where td.design_requirement_id=?1",
            params![successor_requirement],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        successor_bundle,
        (
            "active".to_string(),
            "open".to_string(),
            "active".to_string(),
            1,
        )
    );
    assert_eq!(
        conn.query_row(
            "select phase_id from work_phase_task_memberships where task_id=?1",
            params![shared_task],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        phase.phase_id
    );
    drop(conn);
    assert!(list_stale_records(temp.path()).unwrap().is_empty());
}
