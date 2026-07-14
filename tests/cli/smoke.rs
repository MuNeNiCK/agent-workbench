use super::*;

#[test]
fn init_creates_workbench_artifact_directories() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);

    assert!(temp.path().join(".agent-workbench/ledger.sqlite").exists());
    assert!(temp.path().join(".agent-workbench/designs").is_dir());
    assert!(temp.path().join(".agent-workbench/exports").is_dir());
    assert!(temp.path().join(".agent-workbench/logs").is_dir());
}

#[test]
fn design_init_creates_package_under_workbench() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);

    let output = ok(
        temp.path(),
        &[
            "design",
            "init",
            "storage-lifecycle",
            "--title",
            "Storage Lifecycle",
        ],
    );

    let package = temp
        .path()
        .join(".agent-workbench")
        .join("designs")
        .join("storage-lifecycle");
    assert!(output.starts_with("classification: project-internal\n"));
    assert!(output.contains("initialized design package"));
    assert!(!output.contains("path:"));
    assert!(!output.contains(".agent-workbench"));
    assert!(!output.contains(temp.path().to_string_lossy().as_ref()));
    assert!(package.join("design.yaml").exists());
    assert!(package.join("01-introduction-goals.md").exists());
    assert!(package.join("requirements").join("README.md").exists());
    assert!(package.join("validation").join("gates.md").exists());
}

#[test]
fn design_and_acceptance_errors_carry_their_publication_class() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);

    for args in [
        vec!["design", "import", "missing-private-package"],
        vec![
            "acceptance",
            "add",
            "--design",
            "999",
            "--target",
            "requirement:REQ-PRIVATE",
            "--type",
            "explicit_exception",
            "--reason",
            "boundary check",
            "--authority",
            "999",
        ],
    ] {
        let output = aw(temp.path(), &args);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .starts_with("classification: project-internal\n")
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("classification: project-internal")
        );
    }
}
