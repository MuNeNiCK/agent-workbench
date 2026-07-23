use std::fs;

use super::*;

mod application;
mod atomicity;
mod candidates;
mod cross_design;
mod legacy;
mod lifecycle;
mod lineage;
mod review_context;
mod revision;
mod successor;

fn opaque_component(value: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut encoded = String::new();
    for chunk in value.as_bytes().chunks(3) {
        let first = chunk[0];
        encoded.push(ALPHABET[(first >> 2) as usize] as char);
        encoded.push(
            ALPHABET[((first & 0x03) << 4 | chunk.get(1).copied().unwrap_or(0) >> 4) as usize]
                as char,
        );
        if let Some(second) = chunk.get(1).copied() {
            encoded.push(
                ALPHABET[((second & 0x0f) << 2 | chunk.get(2).copied().unwrap_or(0) >> 6) as usize]
                    as char,
            );
        }
        if let Some(third) = chunk.get(2).copied() {
            encoded.push(ALPHABET[(third & 0x3f) as usize] as char);
        }
    }
    format!("b64:{encoded}")
}
use agent_workbench::{
    AdjudicationInput, ChecklistItemListQuery, ClosureReady, DecompositionPlanQuery, NewClosure,
    NewFinding, NewFindingVerification, NewImplementationEvidence, NewReviewPlan, NewReviewRun,
    TaskListQuery, ValidationGateContextQuery, add_closure, add_finding, add_finding_verification,
    add_implementation_evidence, add_review_plan, add_review_run,
    add_review_run_with_finding_result, adjudicate_review, adjudicate_verification,
    begin_correction, classify_finding, list_checklist_items, list_checklists, list_phases,
    list_tasks, list_validation_gate_context, ready_closure, show_decomposition_plan,
};
use rusqlite::Connection;

fn field<'a>(output: &'a str, name: &str) -> &'a str {
    output
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{name}: ")))
        .unwrap()
}

fn accept_exact_plan_review(root: &Path, design: &str, work: &str, context_ref: &str) {
    let review_plan = add_review_plan(
        root,
        NewReviewPlan {
            work_unit_id: work.parse().unwrap(),
            design_version_id: Some(design.parse().unwrap()),
            review_type: "design_task_decomposition",
            required: true,
            stage: "implementation-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    let review_run = add_review_run(
        root,
        NewReviewRun {
            review_plan_id: review_plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some(context_ref),
            prompt_deviations: None,
            result_summary: Some("the exact Plan has observable completion boundaries"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: Some("independent-plan-reviewer"),
            external_agent_id: Some("independent-plan-reviewer-1"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:exact-plan"),
        },
    )
    .unwrap();
    adjudicate_review(
        root,
        review_run.review_run_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "accept the exact clean Plan review",
            expected_current: "pending",
        },
    )
    .unwrap();
}

fn setup(root: &Path, gate: &str) -> (String, String, String) {
    ok(root, &["init"]);
    let work = ok(root, &["work", "start", "black-box decomposition"]);
    ok(
        root,
        &[
            "design",
            "init",
            "black-box-plan",
            "--title",
            "Black Box Plan",
        ],
    );
    let package = root.join(".agent-workbench/designs/black-box-plan");
    fs::write(
        package.join("requirements/README.md"),
        r#"## REQ-001: Public behavior
```yaml agent-workbench
type: requirement
key: REQ-001
priority: high
surfaces: [cli]
validation: [GATE-001]
status: active
```

The public behavior remains observable.
"#,
    )
    .unwrap();
    fs::write(
        package.join("validation/gates.md"),
        r#"## GATE-001: Public observation
```yaml agent-workbench
type: validation_gate_template
key: GATE-001
applies_to: [REQ-001]
expected_result: pass
phase: implementation
status: active
```

Observe the public operation.
"#,
    )
    .unwrap();
    let imported = ok(
        root,
        &[
            "design",
            "import",
            ".agent-workbench/designs/black-box-plan",
        ],
    );
    let design = field(&imported, "design_version_id").to_string();
    let identity = field(&imported, "design_identity").to_string();
    ok(root, &["design", "approve", &design]);
    let work = field(&work, "work_unit_id").to_string();
    let plans = package.join("plans");
    fs::create_dir_all(&plans).unwrap();
    let plan = plans.join("plan.md");
    fs::write(
        &plan,
        format!(
            r#"# Black-box plan

```yaml agent-workbench
type: decomposition_plan
format: 1
key: black-box-plan
design_fingerprint: {identity}
items:
  - key: "opaque/item:一"
    requirements: [REQ-001]
    title: Public task
    details: Implement the public behavior.
    completion:
      outcome: The public behavior is observable.
      observation: Exercise the public command.
      evidence_owner: work:{work}
      evidence_kind: validation
      gates: [{gate}]
    checklist:
      - key: observed
        condition: The outcome is observed.
        evidence_kind: validation
        gates: [{gate}]
    slice: "Slice/一"
slices:
  - key: "Slice/一"
    title: Public Slice
    order: 1
    depends_on: []
```
"#,
        ),
    )
    .unwrap();
    (
        design,
        work,
        ".agent-workbench/designs/black-box-plan/plans/plan.md".to_string(),
    )
}
