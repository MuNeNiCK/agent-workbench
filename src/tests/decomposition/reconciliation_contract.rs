use super::*;

#[test]
fn reconciliation_document_has_closed_typed_endpoint_mappings() {
    let temp = tempfile::tempdir().unwrap();
    let plans = temp.path().join(".agent-workbench/designs/typed/plans");
    fs::create_dir_all(&plans).unwrap();
    let plan = plans.join("successor.md");
    let document = r#"# Successor

```yaml agent-workbench
type: decomposition_plan
format: 1
key: successor
design_fingerprint: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
items:
  - key: item
    requirements: [REQ]
    title: Item
    details: Details
    completion:
      outcome: Outcome
      observation: Observation
      evidence_owner: work:1
      evidence_kind: validation
      gates: [GATE]
    checklist:
      - key: boundary
        condition: Condition
        evidence_kind: validation
        gates: [GATE]
    slice: slice
slices:
  - key: slice
    title: Slice
    order: 1
    depends_on: []
reconciliation:
  predecessor: 7
  expected_current: bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
  tasks:
    - source: 11
      disposition: retained
      item: item
      effect: open
  checklist:
    - source: 12
      disposition: retained
      item: item
      boundary: boundary
  gates:
    - source: 13
      disposition: retained
      item: item
      gate: GATE
      boundary: retained-source
  phases:
    - source: 14
      disposition: retained
      slice: slice
  dependencies:
    - source: 15
      disposition: retired
      reason: The predecessor edge is no longer required.
```
"#;
    fs::write(&plan, document).unwrap();
    let parsed = crate::decomposition::parse_plan(temp.path(), &plan).unwrap();
    let reconciliation = parsed.document.unwrap().reconciliation.unwrap();
    assert_eq!(reconciliation.predecessor, 7);
    assert_eq!(reconciliation.tasks[0].item.as_deref(), Some("item"));
    assert_eq!(
        reconciliation.tasks[0].effect,
        Some(crate::decomposition::ReconciliationEffect::Open)
    );
    assert_eq!(reconciliation.checklist[0].effect, None);
    assert_eq!(reconciliation.dependencies[0].disposition, "retired");

    fs::write(
        &plan,
        document.replace(
            "disposition: retained\n      item: item",
            "disposition: retained\n      item: missing",
        ),
    )
    .unwrap();
    let error = crate::decomposition::parse_plan(temp.path(), &plan).unwrap_err();
    assert!(error.to_string().contains("unknown item"));

    fs::write(
        &plan,
        document.replace(
            "disposition: retired\n      reason: The predecessor edge is no longer required.",
            "disposition: retired\n      reason: The predecessor edge is no longer required.\n      effect: preserve",
        ),
    )
    .unwrap();
    let error = crate::decomposition::parse_plan(temp.path(), &plan).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("retired dependency mapping forbids lifecycle effect")
    );
}
