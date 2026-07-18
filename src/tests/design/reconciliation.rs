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

    let conn = open_existing_project(temp.path()).unwrap();
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
}
