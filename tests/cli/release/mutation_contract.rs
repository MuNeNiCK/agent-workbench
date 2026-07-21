use super::*;

#[test]
fn release_mutation_is_operator_scoped_and_requires_exact_current_and_idempotency() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let direct = aw(temp.path(), &["release", "assemble"]);
    assert!(!direct.status.success());
    assert!(String::from_utf8_lossy(&direct.stderr).contains("unrecognized subcommand"));

    let missing = aw(
        temp.path(),
        &[
            "operator",
            "release",
            "publish-source",
            "candidate",
            "--expected-current",
            "revision",
        ],
    );
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("--idempotency-key"));
}
