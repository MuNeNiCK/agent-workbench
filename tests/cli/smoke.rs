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
fn init_name_is_persisted_and_cannot_be_silently_replaced() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init", "--name", "named-project"]);
    ok(temp.path(), &["init", "--name", "named-project"]);

    let changed = aw(temp.path(), &["init", "--name", "different-project"]);
    assert!(!changed.status.success());
    assert!(
        String::from_utf8_lossy(&changed.stderr)
            .contains("project is already initialized with a different name")
    );
}

#[test]
fn preferred_governance_and_evidence_adapters_dispatch_to_existing_owners() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);

    let authority = ok(
        temp.path(),
        &[
            "authority",
            "add",
            "--instruction",
            "keep public behavior observable",
            "--source",
            "user:conversation",
        ],
    );
    assert!(authority.contains("added authority"));
    let authorities = ok(temp.path(), &["authority", "list"]);
    assert!(authorities.contains("user:conversation"));

    let correction = ok(
        temp.path(),
        &[
            "correction",
            "add",
            "--source",
            "opaque owner selection",
            "--scope",
            "project",
            "--severity",
            "high",
            "--expected-change",
            "require an explicit owner",
        ],
    );
    assert!(correction.contains("added correction"));
    let corrections = ok(temp.path(), &["correction", "list", "--status", "active"]);
    assert!(corrections.contains("opaque owner selection -> require an explicit owner"));

    ok(temp.path(), &["work", "start", "evidence owner"]);
    ok(temp.path(), &["task", "add", "observable behavior"]);
    ok(
        temp.path(),
        &[
            "evidence",
            "add",
            "--task",
            "1",
            "--type",
            "artifact",
            "--artifact",
            "/tmp/agent-workbench-observable-proof",
        ],
    );
    let selected = ok(
        temp.path(),
        &[
            "evidence",
            "list",
            "--owner",
            "work_unit:1",
            "--kind",
            "artifact",
        ],
    );
    assert!(selected.contains("[artifact] task=1"));
    let excluded = ok(
        temp.path(),
        &[
            "evidence",
            "list",
            "--owner",
            "work_unit:1",
            "--kind",
            "file",
        ],
    );
    assert!(excluded.contains("no implementation evidence"));
    let invalid_owner = aw(temp.path(), &["evidence", "list", "--owner", "unknown:1"]);
    assert!(!invalid_owner.status.success());
    assert!(
        String::from_utf8_lossy(&invalid_owner.stderr).contains("evidence owner must be task:<id>")
    );
    ok(
        temp.path(),
        &[
            "repository",
            "add",
            "git-alias-owner",
            "--path",
            temp.path().to_str().unwrap(),
            "--head",
            "abc123",
        ],
    );
    let commit = ok(
        temp.path(),
        &[
            "git",
            "commit",
            "add",
            "--repository",
            "git-alias-owner",
            "--commit",
            "abc123",
            "--note",
            "preferred short form",
        ],
    );
    assert!(commit.contains("added git commit"));
    let changed = ok(
        temp.path(),
        &[
            "git",
            "files",
            "add",
            "--repository",
            "git-alias-owner",
            "--path",
            "src/lib.rs",
            "--change",
            "modified",
        ],
    );
    assert!(changed.contains("added git file change"));
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
fn design_import_and_refresh_reject_non_markdown_manifest_entries() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(
        temp.path(),
        &[
            "design",
            "init",
            "markdown-only",
            "--title",
            "Markdown Only",
        ],
    );
    let package = temp.path().join(".agent-workbench/designs/markdown-only");
    std::fs::create_dir(package.join("source")).unwrap();
    std::fs::write(package.join("source/decision.json"), "{}\n").unwrap();
    let manifest_path = package.join("design.yaml");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap();
    std::fs::write(
        &manifest_path,
        manifest.replace(
            "decisions: 09-decisions.md",
            "decisions: source/decision.json",
        ),
    )
    .unwrap();

    for command in ["import", "refresh"] {
        let output = aw(
            temp.path(),
            &[
                "design",
                command,
                ".agent-workbench/designs/markdown-only",
                "--status",
                "draft",
            ],
        );
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("design manifest paths must name Markdown files ending in .md")
        );
    }
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

#[test]
fn declared_governance_filters_select_exact_owner_fields() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);

    for (topic, decision) in [
        ("release", "retain boundary"),
        ("storage", "migrate atomically"),
    ] {
        ok(
            temp.path(),
            &["decision", "add", "--topic", topic, "--decision", decision],
        );
    }
    let decisions = ok(temp.path(), &["decision", "list", "--topic", "release"]);
    assert!(decisions.contains("retain boundary"));
    assert!(!decisions.contains("migrate atomically"));

    for (name, scope, command) in [
        ("project-check", "project", "cargo test"),
        ("release-check", "release", "cargo package"),
    ] {
        ok(
            temp.path(),
            &[
                "command",
                "fixed",
                "add",
                "--name",
                name,
                "--type",
                "validation",
                "--scope",
                scope,
                "--command",
                command,
            ],
        );
    }
    let commands = ok(temp.path(), &["command", "list", "--scope", "release"]);
    assert!(commands.contains("release-check"));
    assert!(commands.contains("scope=release"));
    assert!(!commands.contains("project-check"));

    ok(
        temp.path(),
        &[
            "correction",
            "add",
            "--scope",
            "project",
            "--type",
            "process",
            "--pattern",
            "implicit migration",
            "--correction",
            "use update",
        ],
    );
    let corrections = ok(
        temp.path(),
        &[
            "correction",
            "list",
            "--status",
            "active",
            "--scope",
            "project",
        ],
    );
    assert!(corrections.contains("implicit migration"));
    assert!(corrections.contains(":active]"));
    assert_eq!(
        ok(temp.path(), &["correction", "list", "--status", "retired"]),
        "classification: project-internal\nno corrections\n"
    );

    let invalid = aw(temp.path(), &["correction", "list", "--status", "invented"]);
    assert!(!invalid.status.success());
}
