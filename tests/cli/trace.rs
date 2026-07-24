use super::*;

fn field<'a>(output: &'a str, name: &str) -> &'a str {
    output
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{name}: ")))
        .unwrap()
}

fn exercise_public_cli_completed_derivation_rebind(close_aggregate_checklist: bool) {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let started = ok(
        temp.path(),
        &["work", "start", "public completed trace repair"],
    );
    let work_id = field(&started, "work_unit_id").to_string();
    ok(
        temp.path(),
        &[
            "design",
            "init",
            "public-completed-rebind",
            "--title",
            "Public Completed Rebind",
        ],
    );
    let package = temp
        .path()
        .join(".agent-workbench/designs/public-completed-rebind");
    std::fs::write(
        package.join("requirements/README.md"),
        r#"## REQ-001: First boundary
```yaml agent-workbench
type: requirement
key: REQ-001
priority: high
surfaces: [cli]
validation: [GATE-001]
status: active
```
First completion boundary.

## REQ-002: Second boundary
```yaml agent-workbench
type: requirement
key: REQ-002
priority: high
surfaces: [cli]
validation: [GATE-001]
status: active
```
Second completion boundary.
"#,
    )
    .unwrap();
    std::fs::write(
        package.join("validation/gates.md"),
        r#"## GATE-001: Observe aggregate behavior
```yaml agent-workbench
type: validation_gate_template
key: GATE-001
applies_to: [REQ-001, REQ-002]
expected_result: pass
phase: implementation
status: active
```
Observe the aggregate behavior.
"#,
    )
    .unwrap();
    let imported = ok(
        temp.path(),
        &[
            "design",
            "import",
            ".agent-workbench/designs/public-completed-rebind",
        ],
    );
    let design_id = field(&imported, "design_version_id").to_string();
    let design_identity = field(&imported, "design_identity").to_string();
    ok(temp.path(), &["design", "approve", &design_id]);
    let plans = package.join("plans");
    std::fs::create_dir_all(&plans).unwrap();
    std::fs::write(
        plans.join("plan.md"),
        format!(
            r#"# Public completed rebind plan

```yaml agent-workbench
type: decomposition_plan
format: 1
key: public-completed-rebind-plan
design_fingerprint: {design_identity}
items:
  - key: aggregate
    requirements: [REQ-001, REQ-002]
    title: implement aggregate behavior
    details: one aggregate task
    completion:
      outcome: all completion boundaries hold
      observation: exercise the public command
      evidence_owner: work:{work_id}
      evidence_kind: validation
      gates: [GATE-001]
    checklist:
      - key: first-boundary
        condition: first boundary holds
        evidence_kind: validation
        gates: [GATE-001]
      - key: second-boundary
        condition: second boundary holds
        evidence_kind: validation
        gates: [GATE-001]
    slice: implementation
slices:
  - key: implementation
    title: Implementation
    order: 1
    depends_on: []
```
"#
        ),
    )
    .unwrap();
    let staged = ok(
        temp.path(),
        &[
            "decomposition",
            "apply",
            &design_id,
            "--work",
            &work_id,
            "--plan",
            ".agent-workbench/designs/public-completed-rebind/plans/plan.md",
        ],
    );
    let plan_context = field(&staged, "review_context").to_string();
    let plan = ok(
        temp.path(),
        &[
            "review",
            "plan",
            "add",
            "--work-unit",
            &work_id,
            "--design-version",
            &design_id,
            "--type",
            "design_task_decomposition",
            "--stage",
            "implementation-ready",
            "--required",
        ],
    );
    let plan_id = field(&plan, "review_plan_id").to_string();
    let plan_run = ok(
        temp.path(),
        &[
            "review",
            "run",
            "add",
            "--plan",
            &plan_id,
            "--type",
            "fresh",
            "--purpose",
            "new_unbiased_review",
            "--target",
            &plan_context,
            "--clean",
            "--summary",
            "the exact plan has observable completion boundaries",
            "--agent-label",
            "independent-plan-reviewer",
            "--external-agent-id",
            "independent-plan-reviewer-1",
            "--provenance",
            "external_agent",
            "--provenance-ref",
            "review-output:exact-plan",
        ],
    );
    ok(
        temp.path(),
        &[
            "review",
            "adjudicate",
            field(&plan_run, "review_run_id"),
            "--decision",
            "accepted",
            "--reason",
            "accept the exact clean plan review",
            "--expected-current",
            "pending",
        ],
    );
    ok(
        temp.path(),
        &["decomposition", "apply", &design_id, "--work", &work_id],
    );
    let task_inventory = ok(temp.path(), &["task", "list", "--work-unit", &work_id]);
    let task_id = task_inventory
        .lines()
        .find(|line| line.ends_with("implement aggregate behavior"))
        .and_then(|line| line.split_whitespace().next())
        .unwrap()
        .to_string();
    let requirement_inventory = ok(
        temp.path(),
        &["requirement", "list", "--design", &design_id],
    );
    let requirement_id = |key: &str| {
        requirement_inventory
            .lines()
            .find(|line| line.starts_with(&format!("{key} [id=")))
            .and_then(|line| line.split_once("[id=").map(|(_, suffix)| suffix))
            .and_then(|suffix| suffix.split_whitespace().next())
            .and_then(|id| id.parse::<i64>().ok())
            .unwrap_or_else(|| panic!("requirement list did not expose the id for {key}"))
    };
    let first_requirement_id = requirement_id("REQ-001");
    let second_requirement_id = requirement_id("REQ-002");
    let derivations = ok(
        temp.path(),
        &["trace", "derivation", "list", "--design", &design_id],
    );
    let derivation = |requirement: &str| {
        let line = derivations
            .lines()
            .find(|line| line.contains(&format!("requirement={requirement} ")))
            .unwrap_or_else(|| panic!("missing derivation for {requirement}"));
        let id = line
            .split_whitespace()
            .next()
            .unwrap()
            .parse::<i64>()
            .unwrap();
        let item = line
            .split_whitespace()
            .find_map(|part| part.strip_prefix("checklist_item="))
            .unwrap()
            .parse::<i64>()
            .unwrap();
        (id, item)
    };
    let (_first_derivation_id, first_item_id) = derivation("REQ-001");
    let (second_derivation_id, second_initial_item_id) = derivation("REQ-002");
    assert_eq!(second_initial_item_id, first_item_id);
    let checklist_inventory = ok(temp.path(), &["checklist", "list", "--work", &work_id]);
    let checklist_id = checklist_inventory
        .lines()
        .find(|line| {
            line.split_whitespace()
                .next()
                .is_some_and(|value| value.parse::<i64>().is_ok())
        })
        .and_then(|line| line.split_whitespace().next())
        .unwrap()
        .to_string();
    let checklist_items = ok(temp.path(), &["checklist", "item", "list", &checklist_id]);
    let item_id = |title: &str| {
        checklist_items
            .lines()
            .find(|line| line.contains(&format!("] {title} |")))
            .and_then(|line| line.split_whitespace().next())
            .unwrap_or_else(|| panic!("missing checklist item {title}"))
            .parse::<i64>()
            .unwrap()
    };
    let first_item_id = item_id("first-boundary");
    let second_item_id = item_id("second-boundary");
    for requirement in ["REQ-001", "REQ-002"] {
        let selected = ok(
            temp.path(),
            &[
                "gate",
                "select",
                "--design",
                &design_id,
                "--template",
                "GATE-001",
                "--requirement",
                requirement,
                "--task",
                &task_id,
                "--command",
                "public-test-validation",
            ],
        );
        let gate_id = selected
            .lines()
            .find_map(|line| line.strip_prefix("validation_gate_id: "))
            .unwrap();
        ok(
            temp.path(),
            &[
                "gate",
                "record",
                "--gate",
                gate_id,
                "--result",
                "pass",
                "--command",
                "public-test-validation",
            ],
        );
        ok(
            temp.path(),
            &[
                "evidence",
                "add",
                "--task",
                &task_id,
                "--design",
                &design_id,
                "--requirement",
                requirement,
                "--type",
                "commit",
                "--commit",
                "public-test-commit",
            ],
        );
        ok(
            temp.path(),
            &[
                "coverage",
                "add",
                "--design",
                &design_id,
                "--requirement",
                requirement,
                "--task",
                &task_id,
                "--status",
                "covered",
                "--requirement-text",
                "the public route establishes this requirement",
                "--runtime",
                "public test runtime",
                "--tests-or-gates",
                "GATE-001 passed",
            ],
        );
    }
    ok(
        temp.path(),
        &["checklist", "item", "close", &first_item_id.to_string()],
    );
    ok(
        temp.path(),
        &["checklist", "item", "close", &second_item_id.to_string()],
    );
    if close_aggregate_checklist {
        ok(temp.path(), &["checklist", "close", &checklist_id]);
    }
    ok(temp.path(), &["task", "close", &task_id]);
    let repair_plan = ok(
        temp.path(),
        &[
            "review",
            "plan",
            "add",
            "--work-unit",
            &work_id,
            "--design-version",
            &design_id,
            "--type",
            "design_implementation_diff",
            "--stage",
            "close-ready",
            "--required",
        ],
    );
    let repair_plan_id = field(&repair_plan, "review_plan_id").to_string();
    let repair_run = ok(
        temp.path(),
        &[
            "review",
            "run",
            "add",
            "--plan",
            &repair_plan_id,
            "--type",
            "fresh",
            "--purpose",
            "new_unbiased_review",
            "--target",
            &format!("review-context:design-implementation-diff:design={design_id}:work={work_id}"),
            "--new-findings",
            "1",
            "--summary",
            "the completed derivation targets the wrong boundary",
        ],
    );
    let repair_run_id = field(&repair_run, "review_run_id").to_string();
    let added_finding = ok(
        temp.path(),
        &[
            "finding",
            "add",
            "--run",
            &repair_run_id,
            "--type",
            "design_implementation_drift",
            "--severity",
            "high",
            "--description",
            "rebind the completed derivation",
            "--design-requirement",
            &first_requirement_id.to_string(),
            "--task",
            &task_id,
            "--design-requirement",
            &second_requirement_id.to_string(),
            "--task",
            &task_id,
        ],
    );
    let finding_id = added_finding
        .lines()
        .find_map(|line| line.strip_prefix("finding_id: "))
        .unwrap()
        .parse::<i64>()
        .unwrap();
    let accepted = ok(
        temp.path(),
        &[
            "finding",
            "decide",
            &finding_id.to_string(),
            "--decision",
            "accepted",
            "--reason",
            "accept the exact requirement and task target",
            "--expected-current",
            "pending",
        ],
    );
    let decision_handle = accepted
        .lines()
        .find_map(|line| line.strip_prefix("decision_handle: "))
        .unwrap();
    let target_inventory = ok(temp.path(), &["finding", "list", "--status", "open"]);
    assert!(target_inventory.contains(&format!(
        "targets: requirement={},task={};requirement={},task={}",
        first_requirement_id, task_id, second_requirement_id, task_id
    )));
    let closure = ok(
        temp.path(),
        &[
            "closure",
            "add",
            "--finding",
            &finding_id.to_string(),
            "--invariant",
            "each requirement names its establishing boundary",
            "--surfaces",
            "managed trace derivation",
            "--fix-plan",
            "rebind only the selected derivation",
            "--tests",
            "exact derivation list",
            "--verification",
            "independent trace review",
        ],
    );
    let closure_id = closure
        .lines()
        .find_map(|line| line.strip_prefix("closure_id: "))
        .unwrap()
        .to_string();
    ok(
        temp.path(),
        &["work", "remediate", "--finding", &finding_id.to_string()],
    );

    let item_id = second_item_id.to_string();
    let args = [
        "trace",
        "derivation",
        "rebind",
        "--design",
        &design_id,
        "--requirement",
        "REQ-002",
        "--task",
        &task_id,
        "--checklist-item",
        &item_id,
        "--closure",
        &closure_id,
        "--reason",
        "bind the second requirement to its establishing shared boundary",
    ];
    let rebound = ok(temp.path(), &args);
    assert!(rebound.contains(&format!("task_derivation_id: {}", second_derivation_id)));
    assert!(rebound.contains("idempotent: false"));
    assert!(ok(temp.path(), &args).contains("idempotent: true"));

    let finding_id_text = finding_id.to_string();
    let rejected = aw(
        temp.path(),
        &[
            "finding",
            "decide",
            &finding_id_text,
            "--decision",
            "rejected",
            "--reason",
            "must not discard an applied public rebind",
            "--expected-current",
            decision_handle,
        ],
    );
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr)
            .contains("finding_has_active_remediation_effects")
    );
    let listed = ok(temp.path(), &["finding", "list", "--status", "open"]);
    assert!(listed.contains(&format!("{} [run={} ", finding_id, repair_run_id)));
    assert!(listed.contains(&format!("current_decision_handle: {}", decision_handle)));
}

#[test]
fn public_cli_rebinds_a_completed_derivation_under_its_closure() {
    exercise_public_cli_completed_derivation_rebind(true);
}

#[test]
fn public_cli_rebinds_a_completed_derivation_inside_an_active_aggregate() {
    exercise_public_cli_completed_derivation_rebind(false);
}
