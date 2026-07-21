use super::*;

#[test]
fn explicit_plan_application_atomically_publishes_the_declared_graph() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "explicit decomposition", None).unwrap();
    let package = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "explicit-plan",
            title: "Explicit Plan",
        },
    )
    .unwrap();
    fs::write(
        package.package_path.join("requirements/README.md"),
        format!(
            "{}\n{}",
            requirement_doc("REQ-001", "Preserve arbitrary behavior", "high"),
            requirement_doc("REQ-002", "Preserve related behavior", "medium")
        ),
    )
    .unwrap();
    fs::write(
        package.package_path.join("validation/gates.md"),
        validation_gate_doc("GATE-001")
            .replace("applies_to: [REQ-001]", "applies_to: [REQ-001, REQ-002]"),
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
    let plan_path = plans.join("explicit.md");
    fs::write(
        &plan_path,
        format!(
            r#"# Explicit plan

```yaml agent-workbench
type: decomposition_plan
format: 1
key: arbitrary-plan
design_fingerprint: {}
items:
  - key: "opaque:item/一"
    requirements: [REQ-001, REQ-002]
    title: Preserve the declared behavior
    details: Implement only the declared behavior.
    completion:
      outcome: The declared behavior is observable.
      observation: Exercise it through the public operation.
      evidence_owner: work:{}
      evidence_kind: validation
      gates: [GATE-001]
    checklist:
      - key: "boundary:一"
        condition: The declared outcome is observed.
        evidence_kind: validation
        gates: [GATE-001]
    slice: "Slice/一"
slices:
  - key: "Slice/一"
    title: Arbitrary Slice
    order: 1
    depends_on: []
```
"#,
            imported.content_hash, work.work_unit_id
        ),
    )
    .unwrap();

    let (staged, applied) = import_review_and_apply(
        temp.path(),
        imported.design_version_id,
        work.work_unit_id,
        &plan_path,
    );
    assert!(!staged.applied);
    assert_eq!(
        (
            applied.task_count,
            applied.checklist_item_count,
            applied.phase_count,
            applied.dependency_count,
            applied.already_applied,
        ),
        (1, 1, 1, 0, false)
    );
    let shown = show_decomposition_plan(
        temp.path(),
        DecompositionPlanQuery {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
        },
    )
    .unwrap();
    assert_eq!(shown.status, "applied");
    assert_eq!(shown.items[0].key, "opaque:item/一");
    assert_eq!(shown.slices[0].key, "Slice/一");
    let resolved = resolve_decomposition_plan(
        temp.path(),
        DecompositionPlanQuery {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
        },
    )
    .unwrap();
    assert_eq!(
        resolved.current.as_ref().map(|plan| plan.id),
        Some(shown.id)
    );
    assert_eq!(
        resolved.actions,
        vec![
            format!(
                "agent-workbench decomposition show --design-version {} --work {}",
                imported.design_version_id, work.work_unit_id
            ),
            format!(
                "agent-workbench gate implementation-ready --design-version {}",
                imported.design_version_id
            )
        ]
    );
    let conn = crate::db::open_existing_project(temp.path()).unwrap();
    let canonical: (i64, i64, i64, i64) = conn
        .query_row(
            r#"
            select
              (select count(*) from task_identities),
              (select count(*) from task_revisions),
              (select count(*) from task_revision_requirements),
              (select count(*) from task_phase_memberships)
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(canonical, (1, 1, 2, 1));
    let task_id: i64 = conn
        .query_row(
            "select task_id from decomposition_applications where decomposition_plan_id=?1",
            [applied.plan_id],
            |row| row.get(0),
        )
        .unwrap();
    let original_revision: i64 = conn
        .query_row(
            "select task_revision_id from task_revision_aliases where historical_task_id=?1",
            [task_id],
            |row| row.get(0),
        )
        .unwrap();
    drop(conn);
    let mut conn = crate::db::open_existing_project(temp.path()).unwrap();
    let tx = conn.transaction().unwrap();
    let project = crate::db::project_id(&tx).unwrap();
    tx.execute(
        "update tasks set details=?1,completion_condition=?2 where id=?3",
        rusqlite::params![
            "Corrected task-local implementation boundary.",
            "The corrected public outcome is observed.",
            task_id
        ],
    )
    .unwrap();
    let revised = crate::task_identity::revise_canonical_task(
        &tx,
        project,
        task_id,
        "Corrected task-local implementation boundary.",
        "The corrected public outcome is observed.",
    )
    .unwrap();
    tx.commit().unwrap();
    assert_ne!(revised, original_revision);
    let conn = crate::db::open_existing_project(temp.path()).unwrap();
    let revision_states = conn
        .prepare(
            "select status from task_revisions where task_identity_id=(select task_identity_id from task_revisions where id=?1) order by id",
        )
        .unwrap()
        .query_map([revised], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(revision_states, vec!["historical", "current"]);
    let alias_revision: i64 = conn
        .query_row(
            "select task_revision_id from task_revision_aliases where historical_task_id=?1",
            [task_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(alias_revision, revised);
    drop(conn);

    let replay = apply_decomposition_plan(
        temp.path(),
        DecompositionApplication {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
            plan_path: Some(&plan_path),
        },
    )
    .unwrap();
    assert_eq!(replay.plan_id, applied.plan_id);
    assert!(replay.already_applied);
}
