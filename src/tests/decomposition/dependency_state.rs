#[test]
fn dependency_recomputation_has_closed_evidence_and_authority_branches() {
    use crate::decomposition::recompute_dependency_state;

    assert_eq!(
        recompute_dependency_state("open", Some("obsolete".into()), Some(7), 11, 21, "open")
            .unwrap(),
        ("open", None, None)
    );
    assert_eq!(
        recompute_dependency_state(
            "satisfied",
            Some("evidence:independent".into()),
            None,
            11,
            21,
            "open",
        )
        .unwrap(),
        ("satisfied", Some("evidence:independent".into()), None)
    );
    assert_eq!(
        recompute_dependency_state("accepted", None, Some(7), 11, 21, "open").unwrap(),
        ("accepted", None, Some(7))
    );
    assert!(
        recompute_dependency_state(
            "satisfied",
            Some("phase:11:closed".into()),
            None,
            11,
            21,
            "open",
        )
        .unwrap_err()
        .to_string()
        .contains("cannot reuse obsolete phase-close evidence")
    );
}
