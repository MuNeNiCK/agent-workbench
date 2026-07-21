use super::*;

#[test]
fn applied_plan_stages_an_owned_successor_before_review_or_reconcile() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let (design, work, plan_path) = setup(root, "GATE-001");
    let imported = ok(
        root,
        &[
            "decomposition",
            "apply",
            &design,
            "--work",
            &work,
            "--plan",
            &plan_path,
        ],
    );
    accept_exact_plan_review(root, &design, &work, field(&imported, "review_context"));
    ok(root, &["decomposition", "apply", &design, "--work", &work]);

    let design_id = design.parse::<i64>().unwrap();
    let work_id = work.parse::<i64>().unwrap();
    let predecessor = show_decomposition_plan(
        root,
        DecompositionPlanQuery {
            design_version_id: design_id,
            work_unit_id: work_id,
        },
    )
    .unwrap();
    let task = list_tasks(
        root,
        TaskListQuery {
            status: None,
            work_unit_id: Some(work_id),
        },
    )
    .unwrap()[0]
        .id;
    let checklist = list_checklists(root, None).unwrap()[0].id;
    let checklist_item = list_checklist_items(
        root,
        ChecklistItemListQuery {
            checklist_id: Some(checklist),
            status: None,
        },
    )
    .unwrap()[0]
        .id;
    let gate = list_validation_gate_context(
        root,
        ValidationGateContextQuery {
            design_version_id: design_id,
            work_unit_id: Some(work_id),
        },
    )
    .unwrap()[0]
        .id;
    let phase = list_phases(root, work_id).unwrap()[0].id;
    let successor_path = ".agent-workbench/designs/black-box-plan/plans/staged.md";
    let source = fs::read_to_string(root.join(&plan_path)).unwrap();
    fs::write(
        root.join(successor_path),
        source
            .replacen("key: black-box-plan", "key: black-box-staged", 1)
            .replace(
                "    depends_on: []\n```",
                &format!(
                    r#"    depends_on: []
reconciliation:
  predecessor: {}
  expected_current: {}
  tasks:
    - {{ source: {task}, disposition: retained, item: "opaque/item:一" }}
  checklist:
    - {{ source: {checklist_item}, disposition: retained, item: "opaque/item:一", boundary: observed }}
  gates:
    - {{ source: {gate}, disposition: retained, item: "opaque/item:一", gate: GATE-001, boundary: retained-source }}
  phases:
    - {{ source: {phase}, disposition: retained, slice: "Slice/一" }}
  dependencies: []
```"#,
                    predecessor.id,
                    "a".repeat(64)
                ),
            ),
    )
    .unwrap();

    let shown = ok(
        root,
        &[
            "decomposition",
            "show",
            "--design-version",
            &design,
            "--work",
            &work,
        ],
    );
    let stage = shown
        .lines()
        .find(|line| {
            line.starts_with("next: agent-workbench decomposition revise ")
                && line.contains(successor_path)
        })
        .unwrap()
        .strip_prefix("next: agent-workbench ")
        .unwrap();
    assert!(!shown.contains(" --dry-run"));
    let args = stage.split_whitespace().collect::<Vec<_>>();
    let staged = ok(root, &args);
    assert_eq!(field(&staged, "status"), "ready");
    assert_eq!(field(&staged, "predecessor"), predecessor.id.to_string());
    let successor_id = field(&staged, "plan").to_string();
    let successor_current = field(&staged, "current_identity").to_string();
    assert_ne!(successor_id, predecessor.id.to_string());
    let first_review_context = field(&staged, "review_context").to_string();
    assert!(first_review_context.contains(&format!("plan={successor_id}:")));
    assert!(first_review_context.contains(":projection="));
    assert_eq!(field(&staged, "successor_projection_identity").len(), 64);
    assert!(staged.contains("projected_owned_effect:"));
    assert!(staged.contains("projected_shared_binding:"));
    let current = show_decomposition_plan(
        root,
        DecompositionPlanQuery {
            design_version_id: design_id,
            work_unit_id: work_id,
        },
    )
    .unwrap();
    assert_eq!(current.id, predecessor.id);
    assert_eq!(current.status, "applied");
    let replay = ok(root, &args);
    assert_eq!(field(&replay, "plan"), successor_id);
    assert_eq!(field(&replay, "idempotent"), "true");

    let revised_source = fs::read_to_string(root.join(successor_path))
        .unwrap()
        .replace("key: black-box-staged", "key: black-box-staged-revision");
    fs::write(root.join(successor_path), revised_source).unwrap();
    let changed = ok(
        root,
        &[
            "decomposition",
            "show",
            "--design-version",
            &design,
            "--work",
            &work,
        ],
    );
    let revise_ready = changed
        .lines()
        .find(|line| {
            line.starts_with(&format!(
                "next: agent-workbench decomposition revise {successor_id} "
            )) && line.contains(successor_path)
        })
        .unwrap()
        .strip_prefix("next: agent-workbench ")
        .unwrap();
    let replacement = ok(root, &revise_ready.split_whitespace().collect::<Vec<_>>());
    let replacement_id = field(&replacement, "plan").to_string();
    assert_ne!(replacement_id, successor_id);
    assert_eq!(field(&replacement, "status"), "ready");
    assert_eq!(
        field(&replacement, "predecessor"),
        predecessor.id.to_string()
    );
    assert_ne!(field(&replacement, "review_context"), first_review_context);
    let shown_after = ok(
        root,
        &[
            "decomposition",
            "show",
            "--design-version",
            &design,
            "--work",
            &work,
        ],
    );
    assert_eq!(field(&shown_after, "plan"), predecessor.id.to_string());
    assert_eq!(field(&shown_after, "status"), "applied");
    assert_eq!(field(&shown_after, "successor_plan"), replacement_id);
    assert_eq!(field(&shown_after, "successor_status"), "ready");

    let raced_source = fs::read_to_string(root.join(successor_path))
        .unwrap()
        .replace(
            "key: black-box-staged-revision",
            "key: black-box-staged-race",
        );
    fs::write(root.join(successor_path), raced_source).unwrap();
    let stale_revision = aw(
        root,
        &[
            "decomposition",
            "revise",
            &successor_id,
            "--plan",
            successor_path,
            "--expected-current",
            &successor_current,
            "--idempotency-key",
            "stale-ready-race-v3",
        ],
    );
    assert!(!stale_revision.status.success());
    let error = String::from_utf8(stale_revision.stderr).unwrap();
    let refresh = error.split("next: agent-workbench ").nth(1).unwrap().trim();
    assert!(refresh.starts_with(&format!("decomposition revise {replacement_id} ")));
    assert!(refresh.contains("--expected-content "));
    let refresh_args = refresh.split_whitespace().collect::<Vec<_>>();
    let refreshed = ok(root, &refresh_args);
    let refreshed_id = field(&refreshed, "plan").to_string();
    assert_ne!(refreshed_id, replacement_id);
    assert_eq!(field(&refreshed, "predecessor"), predecessor.id.to_string());
    assert_eq!(field(&refreshed, "status"), "ready");
    let replayed = ok(root, &refresh_args);
    assert_eq!(field(&replayed, "plan"), refreshed_id);
    assert_eq!(field(&replayed, "idempotent"), "true");
    let resolved = ok(
        root,
        &[
            "decomposition",
            "show",
            "--design-version",
            &design,
            "--work",
            &work,
        ],
    );
    assert_eq!(field(&resolved, "plan"), predecessor.id.to_string());
    assert_eq!(field(&resolved, "successor_plan"), refreshed_id);
}
