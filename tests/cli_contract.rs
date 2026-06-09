use std::path::Path;
use std::process::{Command, Output};

use rusqlite::Connection;

fn aw(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_agent-workbench"))
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .expect("failed to run agent-workbench")
}

fn ok(root: &Path, args: &[&str]) -> String {
    let output = aw(root, args);
    assert!(
        output.status.success(),
        "command failed: {:?}\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout must be utf-8")
}

fn err(root: &Path, args: &[&str]) -> String {
    let output = aw(root, args);
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: {:?}\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stderr).expect("stderr must be utf-8")
}

fn conn(root: &Path) -> Connection {
    Connection::open(root.join(".agent-workbench").join("ledger.sqlite"))
        .expect("failed to open ledger")
}

fn cli_approval_authority_event(root: &Path) -> String {
    ok(
        root,
        &[
            "authority",
            "event",
            "add",
            "--type",
            "user_instruction",
            "--summary",
            "approve exception for test",
            "--source",
            "test-user",
        ],
    );
    conn(root)
        .query_row(
            "select id from authority_events order by id desc limit 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
        .to_string()
}

fn cli_record_close_evidence(root: &Path, work_unit_id: i64, activation_id: i64) -> i64 {
    ok(
        root,
        &[
            "record",
            "create",
            "--work-unit",
            &work_unit_id.to_string(),
            "--topic",
            "close evidence",
            "--work-performed",
            "recorded close readiness evidence",
        ],
    );
    let repository_count: i64 = conn(root)
        .query_row(
            "select count(*) from repositories where name = 'main'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    if repository_count == 0 {
        ok(
            root,
            &[
                "repository",
                "add",
                "main",
                "--path",
                ".",
                "--head",
                "abc123",
                "--status",
                "clean",
            ],
        );
    }
    ok(
        root,
        &[
            "repository",
            "snapshot",
            "add",
            "--repository",
            "main",
            "--activation",
            &activation_id.to_string(),
            "--head",
            "abc123",
            "--branch",
            "master",
            "--status",
            "clean",
            "--clean",
        ],
    );
    conn(root)
        .query_row(
            "select max(id) from repository_snapshots where work_unit_activation_id = ?1",
            [activation_id],
            |row| row.get(0),
        )
        .unwrap()
}

fn write_requirement(root: &Path) {
    std::fs::write(
        root.join(".agent-workbench")
            .join("designs")
            .join("storage-lifecycle")
            .join("requirements")
            .join("README.md"),
        r#"## REQ-001: Preserve cleanup behavior
```yaml agent-workbench
type: requirement
key: REQ-001
priority: high
surfaces: [cli, database]
validation: [GATE-001]
status: active
```

This requirement describes one verifiable behavior that must be implemented.
"#,
    )
    .unwrap();
}

fn write_decision(root: &Path) {
    std::fs::write(
        root.join(".agent-workbench")
            .join("designs")
            .join("storage-lifecycle")
            .join("09-decisions.md"),
        r#"## DEC-001: Keep project-local ledger
```yaml agent-workbench
type: decision
key: DEC-001
status: accepted
supersedes: []
```

Use one SQLite ledger per project.
"#,
    )
    .unwrap();
}

fn write_gate_template(root: &Path) {
    std::fs::write(
        root.join(".agent-workbench")
            .join("designs")
            .join("storage-lifecycle")
            .join("validation")
            .join("gates.md"),
        r#"## GATE-001: Unit test command
```yaml agent-workbench
type: validation_gate_template
key: GATE-001
applies_to: [REQ-001]
expected_result: pass
phase: implementation
status: active
```

Run the project test suite before implementation handoff.
"#,
    )
    .unwrap();
}

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
fn init_imports_agents_md_as_project_rule() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(
        temp.path().join("AGENTS.md"),
        "# Agent Instructions\n\nUse focused validation commands.\n",
    )
    .unwrap();

    ok(temp.path(), &["init"]);
    let rules = ok(temp.path(), &["rules", "applicable", "--scope", "project"]);
    let conn = conn(temp.path());
    let event: (String, String, String) = conn
        .query_row(
            "select event_type, source, text_or_summary from authority_events where id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    let authority: (String, String, String) = conn
        .query_row(
            "select path_or_label, authority_type, summary from authorities where id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();

    assert!(rules.contains("authority_event_id=1"));
    assert_eq!(event.0, "agents");
    assert_eq!(event.1, "AGENTS.md");
    assert!(event.2.contains("Use focused validation commands."));
    assert_eq!(authority.0, "AGENTS.md");
    assert_eq!(authority.1, "policy");
    assert!(authority.2.contains("Use focused validation commands."));
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
    assert!(output.contains("initialized design package"));
    assert!(output.contains(".agent-workbench/designs/storage-lifecycle"));
    assert!(package.join("design.yaml").exists());
    assert!(package.join("01-introduction-goals.md").exists());
    assert!(package.join("requirements").join("README.md").exists());
    assert!(package.join("validation").join("gates.md").exists());
}

#[test]
fn design_import_records_design_version() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(
        temp.path(),
        &[
            "design",
            "init",
            "storage-lifecycle",
            "--title",
            "Storage Lifecycle",
        ],
    );
    write_requirement(temp.path());
    write_decision(temp.path());
    write_gate_template(temp.path());

    let output = ok(
        temp.path(),
        &[
            "design",
            "import",
            ".agent-workbench/designs/storage-lifecycle",
            "--status",
            "draft",
        ],
    );

    assert!(output.contains("imported design package"));
    assert!(output.contains("design_package_id: 1"));
    assert!(output.contains("design_version_id: 1"));
    assert!(output.contains("file_count: 14"));
    assert!(output.contains("requirement_count: 1"));
    assert!(output.contains("decision_count: 1"));
    assert!(output.contains("validation_gate_template_count: 1"));

    let conn = conn(temp.path());
    let current_version: i64 = conn
        .query_row(
            "select current_design_version_id from design_packages where design_key = 'storage-lifecycle'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let file_count: i64 = conn
        .query_row(
            "select count(*) from design_files where design_version_id = ?1",
            [current_version],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(current_version, 1);
    assert_eq!(file_count, 14);
    let requirement_count: i64 = conn
        .query_row(
            "select count(*) from design_requirements where design_version_id = ?1",
            [current_version],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(requirement_count, 1);
}

#[test]
fn repository_commands_record_state_and_git_evidence() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);

    let repo = ok(
        temp.path(),
        &[
            "repository",
            "add",
            "main",
            "--path",
            ".",
            "--head",
            "abc123",
            "--status",
            "dirty",
        ],
    );
    let snapshot = ok(
        temp.path(),
        &[
            "repository",
            "snapshot",
            "add",
            "--repository",
            "main",
            "--head",
            "abc123",
            "--branch",
            "master",
            "--status",
            "M src/lib.rs",
        ],
    );
    let dirty = ok(
        temp.path(),
        &[
            "repository",
            "dirty",
            "add",
            "--snapshot",
            "1",
            "--path",
            "src/lib.rs",
            "--type",
            "modified",
        ],
    );
    let comparison = ok(
        temp.path(),
        &[
            "repository",
            "compare",
            "add",
            "--base",
            "1",
            "--current",
            "1",
            "--type",
            "review",
            "--result",
            "same",
        ],
    );
    let commit = ok(
        temp.path(),
        &[
            "repository",
            "commit",
            "add",
            "--repository",
            "main",
            "--sha",
            "abc123",
            "--subject",
            "initial",
        ],
    );
    let file = ok(
        temp.path(),
        &[
            "repository",
            "file",
            "add",
            "--commit",
            "1",
            "--path",
            "src/lib.rs",
            "--type",
            "modified",
            "--additions",
            "1",
            "--deletions",
            "0",
        ],
    );
    let record = ok(
        temp.path(),
        &[
            "record",
            "create",
            "--topic",
            "repository evidence",
            "--work-performed",
            "recorded repository state",
        ],
    );
    let record_commit = ok(
        temp.path(),
        &[
            "record",
            "commit",
            "add",
            "1",
            "--git-commit",
            "1",
            "--sha",
            "abc123",
            "--role",
            "created",
        ],
    );
    let record_file = ok(
        temp.path(),
        &[
            "record",
            "file",
            "add",
            "1",
            "--git-file-change",
            "1",
            "--path",
            "src/lib.rs",
            "--role",
            "changed",
        ],
    );
    let work = ok(temp.path(), &["work", "start", "git linked work"]);
    let task = ok(
        temp.path(),
        &[
            "task",
            "add",
            "wire git evidence",
            "--work-unit",
            "1",
            "--completion-condition",
            "evidence links to git ids",
        ],
    );
    let usage = ok(
        temp.path(),
        &[
            "command",
            "usage",
            "add",
            "--command",
            "cargo test",
            "--result",
            "pass",
            "--work-unit",
            "1",
            "--snapshot",
            "1",
        ],
    );
    let evidence = ok(
        temp.path(),
        &[
            "evidence",
            "add",
            "--task",
            "1",
            "--type",
            "file",
            "--git-commit-id",
            "1",
            "--git-file-change-id",
            "1",
        ],
    );
    ok(
        temp.path(),
        &[
            "work",
            "suspend",
            "--reason",
            "fork from snapshot",
            "--next",
            "redo",
        ],
    );
    let fork = ok(
        temp.path(),
        &[
            "work",
            "fork",
            "redo from snapshot",
            "--from-snapshot",
            "1",
            "--reason",
            "user_requested_redo",
        ],
    );
    let list = ok(temp.path(), &["repository", "list"]);
    let snapshots = ok(temp.path(), &["repository", "snapshot", "list"]);

    assert!(repo.contains("repository_id: 1"));
    assert!(snapshot.contains("repository_snapshot_id: 1"));
    assert!(dirty.contains("repository_dirty_entry_id: 1"));
    assert!(comparison.contains("repository_snapshot_comparison_id: 1"));
    assert!(commit.contains("git_commit_id: 1"));
    assert!(file.contains("git_file_change_id: 1"));
    assert!(record.contains("work_record_id: 1"));
    assert!(record_commit.contains("work_record_commit_id: 1"));
    assert!(record_file.contains("work_record_file_id: 1"));
    assert!(work.contains("work_unit_id: 1"));
    assert!(task.contains("task_id: 1"));
    assert!(usage.contains("command_usage_id: 1"));
    assert!(evidence.contains("implementation_evidence_id: 1"));
    assert!(fork.contains("fork_id: 1"));
    assert!(list.contains("main"));
    assert!(snapshots.contains("repository=main"));

    let conn = conn(temp.path());
    let usage_snapshot: i64 = conn
        .query_row(
            "select repository_snapshot_id from command_usages where id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let evidence_links: (i64, i64, i64) = conn
        .query_row(
            "select repository_id, git_commit_id, git_file_change_id from implementation_evidence where id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    let fork_snapshot: i64 = conn
        .query_row(
            "select source_repository_snapshot_id from work_record_forks where id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(usage_snapshot, 1);
    assert_eq!(evidence_links, (1, 1, 1));
    assert_eq!(fork_snapshot, 1);
}

#[test]
fn design_import_lists_decisions_and_gate_templates() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(
        temp.path(),
        &[
            "design",
            "init",
            "storage-lifecycle",
            "--title",
            "Storage Lifecycle",
        ],
    );
    write_requirement(temp.path());
    write_decision(temp.path());
    write_gate_template(temp.path());
    ok(
        temp.path(),
        &[
            "design",
            "import",
            ".agent-workbench/designs/storage-lifecycle",
            "--status",
            "draft",
        ],
    );

    let decisions = ok(temp.path(), &["design-decision", "list", "--design", "1"]);
    assert!(decisions.contains("DEC-001 [Keep project-local ledger:accepted]"));
    assert!(decisions.contains("09-decisions.md"));

    let gates = ok(temp.path(), &["gate-template", "list", "--design", "1"]);
    assert!(gates.contains("GATE-001 [implementation-ready:active expected=pass command=-]"));
    assert!(gates.contains("validation/gates.md"));
}

#[test]
fn acceptance_add_records_design_exception() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(
        temp.path(),
        &[
            "design",
            "init",
            "storage-lifecycle",
            "--title",
            "Storage Lifecycle",
        ],
    );
    write_requirement(temp.path());
    write_gate_template(temp.path());
    ok(
        temp.path(),
        &[
            "design",
            "import",
            ".agent-workbench/designs/storage-lifecycle",
            "--status",
            "draft",
        ],
    );

    let authority = cli_approval_authority_event(temp.path());
    let output = ok(
        temp.path(),
        &[
            "acceptance",
            "add",
            "--design",
            "1",
            "--target",
            "requirement:REQ-001",
            "--type",
            "accepted_out_of_scope",
            "--reason",
            "not needed for current scope",
            "--authority",
            &authority,
        ],
    );

    assert!(output.contains("accepted design exception"));
    assert!(output.contains("target_type: design_requirement"));
    assert!(output.contains("design_requirement_id: 1"));
}

#[test]
fn acceptance_add_records_package_scoped_design_exception() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(
        temp.path(),
        &[
            "design",
            "init",
            "oversized-file",
            "--title",
            "Oversized File",
        ],
    );
    std::fs::write(
        temp.path()
            .join(".agent-workbench")
            .join("designs")
            .join("oversized-file")
            .join("01-introduction-goals.md"),
        std::iter::repeat_n("line", 1001)
            .collect::<Vec<_>>()
            .join("\n"),
    )
    .unwrap();

    let authority = cli_approval_authority_event(temp.path());
    let output = ok(
        temp.path(),
        &[
            "acceptance",
            "add",
            "--package",
            "oversized-file",
            "--target",
            "file:01-introduction-goals.md",
            "--type",
            "explicit_exception",
            "--reason",
            "temporary source document is larger than the import guardrail",
            "--authority",
            &authority,
        ],
    );
    let imported = ok(
        temp.path(),
        &[
            "design",
            "import",
            ".agent-workbench/designs/oversized-file",
            "--status",
            "draft",
        ],
    );

    assert!(output.contains("target_type: design_file"));
    assert!(output.contains("design_package_key: oversized-file"));
    assert!(output.contains("design_file_path: 01-introduction-goals.md"));
    assert!(imported.contains("imported design package"));
}

#[test]
fn requirement_list_prints_imported_requirements() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(
        temp.path(),
        &[
            "design",
            "init",
            "storage-lifecycle",
            "--title",
            "Storage Lifecycle",
        ],
    );
    write_requirement(temp.path());
    ok(
        temp.path(),
        &[
            "design",
            "import",
            ".agent-workbench/designs/storage-lifecycle",
            "--status",
            "draft",
        ],
    );

    let output = ok(temp.path(), &["requirement", "list", "--design", "1"]);

    assert!(output.contains("REQ-001"));
    assert!(output.contains("[high:active rev=1]"));
    assert!(output.contains("requirements/README.md"));
}

#[test]
fn clean_design_document_review_allows_design_ready_gate_to_pass() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(
        temp.path(),
        &[
            "design",
            "init",
            "storage-lifecycle",
            "--title",
            "Storage Lifecycle",
        ],
    );
    write_requirement(temp.path());
    ok(
        temp.path(),
        &[
            "design",
            "import",
            ".agent-workbench/designs/storage-lifecycle",
            "--status",
            "draft",
        ],
    );

    let blocked = ok(
        temp.path(),
        &["gate", "design-ready", "--design-version", "1", "--dry-run"],
    );
    assert!(blocked.contains("gate: design-ready"));
    assert!(blocked.contains("result: blocked"));
    assert!(blocked.contains("design_review_clean: fail"));

    ok(temp.path(), &["work", "start", "design document review"]);
    ok(
        temp.path(),
        &[
            "review",
            "plan",
            "add",
            "--work-unit",
            "1",
            "--type",
            "design_review",
            "--stage",
            "design-ready",
            "--design-version",
            "1",
        ],
    );
    let missing_review_context_target = err(
        temp.path(),
        &[
            "review",
            "run",
            "add",
            "--plan",
            "1",
            "--type",
            "fresh",
            "--purpose",
            "new_unbiased_review",
            "--clean",
            "--summary",
            "clean decomposition review without context",
        ],
    );
    ok(
        temp.path(),
        &[
            "review",
            "run",
            "add",
            "--plan",
            "1",
            "--type",
            "fresh",
            "--purpose",
            "new_unbiased_review",
            "--target",
            "review-context:design-review:design=1:work=1",
            "--clean",
            "--summary",
            "clean design review",
        ],
    );
    let passed = ok(
        temp.path(),
        &["gate", "design-ready", "--design-version", "1", "--dry-run"],
    );
    assert!(missing_review_context_target.contains("must use review-context target"));
    assert!(passed.contains("result: pass"));
    assert!(passed.contains("design_review_clean: pass"));
}

#[test]
fn trace_derivation_allows_implementation_ready_gate_to_pass() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(temp.path(), &["work", "start", "implementation"]);
    ok(
        temp.path(),
        &[
            "task",
            "add",
            "implement cleanup",
            "--priority",
            "high",
            "--source",
            "design",
            "--completion-condition",
            "cleanup behavior is covered",
        ],
    );
    ok(
        temp.path(),
        &[
            "design",
            "init",
            "storage-lifecycle",
            "--title",
            "Storage Lifecycle",
        ],
    );
    write_requirement(temp.path());
    write_gate_template(temp.path());
    ok(
        temp.path(),
        &[
            "design",
            "import",
            ".agent-workbench/designs/storage-lifecycle",
            "--status",
            "draft",
        ],
    );
    ok(
        temp.path(),
        &[
            "design",
            "approve",
            "1",
            "--summary",
            "design passed document checks",
        ],
    );
    let blocked = ok(
        temp.path(),
        &[
            "gate",
            "implementation-ready",
            "--design-version",
            "1",
            "--dry-run",
        ],
    );
    assert!(blocked.contains("result: blocked"));
    assert!(blocked.contains("task_derivations_exist: fail"));

    let derivation = ok(
        temp.path(),
        &[
            "trace",
            "derive-task",
            "--design",
            "1",
            "--requirement",
            "REQ-001",
            "--task",
            "1",
            "--reason",
            "design task decomposition",
        ],
    );
    let list = ok(
        temp.path(),
        &["trace", "derivation", "list", "--design", "1"],
    );
    let without_gate = ok(
        temp.path(),
        &[
            "gate",
            "implementation-ready",
            "--design-version",
            "1",
            "--dry-run",
        ],
    );
    let selected = ok(
        temp.path(),
        &[
            "gate",
            "select",
            "--design",
            "1",
            "--template",
            "GATE-001",
            "--requirement",
            "REQ-001",
            "--task",
            "1",
        ],
    );
    ok(
        temp.path(),
        &[
            "review",
            "plan",
            "add",
            "--work-unit",
            "1",
            "--type",
            "design_task_decomposition",
            "--stage",
            "implementation-ready",
            "--design-version",
            "1",
        ],
    );
    let missing_review_context_target = err(
        temp.path(),
        &[
            "review",
            "run",
            "add",
            "--plan",
            "1",
            "--type",
            "fresh",
            "--purpose",
            "new_unbiased_review",
            "--clean",
            "--summary",
            "clean decomposition review without context",
        ],
    );
    ok(
        temp.path(),
        &[
            "review",
            "run",
            "add",
            "--plan",
            "1",
            "--type",
            "fresh",
            "--purpose",
            "new_unbiased_review",
            "--target",
            "review-context:design-task-decomposition:design=1:work=1",
            "--clean",
            "--summary",
            "clean decomposition review",
        ],
    );
    let passed = ok(
        temp.path(),
        &[
            "gate",
            "implementation-ready",
            "--design-version",
            "1",
            "--dry-run",
        ],
    );
    let close_without_trace = err(temp.path(), &["task", "close", "1", "--commit", "abc123"]);
    let empty_evidence = err(
        temp.path(),
        &[
            "evidence",
            "add",
            "--task",
            "1",
            "--design",
            "1",
            "--requirement",
            "REQ-001",
            "--type",
            "commit",
        ],
    );
    let evidence = ok(
        temp.path(),
        &[
            "evidence",
            "add",
            "--task",
            "1",
            "--design",
            "1",
            "--requirement",
            "REQ-001",
            "--type",
            "commit",
            "--commit",
            "abc123",
        ],
    );
    let evidence_list = ok(temp.path(), &["evidence", "list", "--task", "1"]);
    let coverage = ok(
        temp.path(),
        &[
            "coverage",
            "add",
            "--design",
            "1",
            "--requirement",
            "REQ-001",
            "--task",
            "1",
            "--status",
            "covered",
            "--requirement-text",
            "cleanup behavior is connected",
            "--runtime",
            "cleanup path is exercised",
            "--tests-or-gates",
            "GATE-001",
        ],
    );
    let incomplete_covered_coverage = err(
        temp.path(),
        &[
            "coverage",
            "add",
            "--design",
            "1",
            "--requirement",
            "REQ-001",
            "--task",
            "1",
            "--status",
            "covered",
            "--requirement-text",
            "cleanup behavior is asserted without boundary evidence",
            "--tests-or-gates",
            "GATE-001",
        ],
    );
    let coverage_list = ok(
        temp.path(),
        &["coverage", "list", "--design", "1", "--status", "covered"],
    );
    let review_context = ok(
        temp.path(),
        &[
            "review-context",
            "design-implementation-diff",
            "--design-version",
            "1",
            "--work-unit",
            "1",
        ],
    );
    let raw_out_of_scope_coverage = err(
        temp.path(),
        &[
            "coverage",
            "add",
            "--design",
            "1",
            "--requirement",
            "REQ-001",
            "--task",
            "1",
            "--status",
            "accepted_out_of_scope",
            "--requirement-text",
            "cleanup behavior is out of scope",
        ],
    );
    ok(temp.path(), &["task", "close", "1", "--commit", "abc123"]);
    let passed_after_close = ok(
        temp.path(),
        &[
            "gate",
            "implementation-ready",
            "--design-version",
            "1",
            "--dry-run",
        ],
    );
    let partial_coverage = ok(
        temp.path(),
        &[
            "coverage",
            "add",
            "--design",
            "1",
            "--requirement",
            "REQ-001",
            "--task",
            "1",
            "--status",
            "partial",
            "--requirement-text",
            "cleanup behavior is intentionally out of scope",
            "--missing",
            "not applicable",
        ],
    );
    let authority = cli_approval_authority_event(temp.path());
    let coverage_acceptance = ok(
        temp.path(),
        &[
            "acceptance",
            "add",
            "--design",
            "1",
            "--target",
            "coverage:2",
            "--type",
            "accepted_out_of_scope",
            "--reason",
            "coverage is explicitly out of scope",
            "--authority",
            &authority,
        ],
    );

    assert!(derivation.contains("derived task from requirement"));
    assert!(derivation.contains("task_derivation_id: 1"));
    assert!(list.contains("requirement=REQ-001 task=1"));
    assert!(without_gate.contains("validation_gates_selected: fail"));
    assert!(selected.contains("selected validation gate"));
    assert!(selected.contains("validation_gate_id: 1"));
    assert!(missing_review_context_target.contains("must use review-context target"));
    assert!(passed.contains("result: pass"));
    assert!(passed.contains("task_derivations_exist: pass"));
    assert!(passed.contains("validation_gates_selected: pass"));
    assert!(close_without_trace.contains("cannot close design-derived task"));
    assert!(empty_evidence.contains("requires commit, file, symbol, or artifact reference"));
    assert!(evidence.contains("added implementation evidence"));
    assert!(evidence.contains("implementation_evidence_id: 1"));
    assert!(evidence_list.contains("1 [commit] task=1 requirement=REQ-001 commit=abc123"));
    assert!(coverage.contains("added coverage item"));
    assert!(coverage.contains("coverage_item_id: 1"));
    assert!(coverage_list.contains("1 [covered] requirement=REQ-001"));
    assert!(incomplete_covered_coverage.contains("requires boundary evidence"));
    assert!(review_context.contains("requirements:"));
    assert!(review_context.contains("task_derivations:"));
    assert!(review_context.contains("selected_validation_gates:"));
    assert!(review_context.contains("latest_run="));
    assert!(review_context.contains("command_usage="));
    assert!(review_context.contains("snapshot="));
    assert!(review_context.contains("implementation_evidence:"));
    assert!(review_context.contains("coverage_items:"));
    assert!(review_context.contains("known_gaps:"));
    assert!(review_context.contains("stale_records:"));
    assert!(raw_out_of_scope_coverage.contains("requires an approved acceptance record"));
    assert!(passed_after_close.contains("result: pass"));
    assert!(passed_after_close.contains("implementation_evidence_present: pass"));
    assert!(passed_after_close.contains("coverage_items_present: pass"));
    assert!(partial_coverage.contains("coverage_item_id: 2"));
    assert!(coverage_acceptance.contains("target_type: coverage_item"));
    assert!(coverage_acceptance.contains("coverage_item_id: 2"));
}

#[test]
fn work_start_with_design_version_requires_implementation_ready_gate() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(
        temp.path(),
        &[
            "design",
            "init",
            "storage-lifecycle",
            "--title",
            "Storage Lifecycle",
        ],
    );
    write_requirement(temp.path());
    ok(
        temp.path(),
        &[
            "design",
            "import",
            ".agent-workbench/designs/storage-lifecycle",
            "--status",
            "draft",
        ],
    );

    let blocked = err(
        temp.path(),
        &["work", "start", "implement design", "--design-version", "1"],
    );
    let work_count: i64 = conn(temp.path())
        .query_row("select count(*) from work_units", [], |row| row.get(0))
        .unwrap();

    assert!(blocked.contains("implementation-ready"));
    assert_eq!(work_count, 0);
}

#[test]
fn decompose_design_requires_clean_design_ready_plan() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(temp.path(), &["work", "start", "implementation"]);
    ok(
        temp.path(),
        &[
            "design",
            "init",
            "storage-lifecycle",
            "--title",
            "Storage Lifecycle",
        ],
    );
    write_requirement(temp.path());
    write_gate_template(temp.path());
    ok(
        temp.path(),
        &[
            "design",
            "import",
            ".agent-workbench/designs/storage-lifecycle",
            "--status",
            "draft",
        ],
    );

    let blocked = err(
        temp.path(),
        &["decompose", "design", "1", "--work-unit", "1"],
    );
    ok(
        temp.path(),
        &[
            "review",
            "plan",
            "add",
            "--work-unit",
            "1",
            "--type",
            "design_review",
            "--stage",
            "design-ready",
            "--design-version",
            "1",
            "--required",
        ],
    );
    ok(
        temp.path(),
        &[
            "review",
            "run",
            "add",
            "--plan",
            "1",
            "--type",
            "fresh",
            "--purpose",
            "new_unbiased_review",
            "--target",
            "review-context:design-review:design=1:work=1",
            "--clean",
        ],
    );
    let decomposed = ok(
        temp.path(),
        &["decompose", "design", "1", "--work-unit", "1"],
    );

    assert!(blocked.contains("requires a clean design-ready review plan"));
    assert!(decomposed.contains("decomposed design"));
}

#[test]
fn gate_record_links_validation_run_to_command_usage_and_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(temp.path(), &["work", "start", "implementation"]);
    ok(
        temp.path(),
        &[
            "task",
            "add",
            "implement cleanup",
            "--priority",
            "high",
            "--source",
            "design",
            "--completion-condition",
            "cleanup behavior is covered",
        ],
    );
    ok(
        temp.path(),
        &[
            "design",
            "init",
            "storage-lifecycle",
            "--title",
            "Storage Lifecycle",
        ],
    );
    write_requirement(temp.path());
    write_gate_template(temp.path());
    ok(
        temp.path(),
        &[
            "design",
            "import",
            ".agent-workbench/designs/storage-lifecycle",
            "--status",
            "draft",
        ],
    );
    ok(
        temp.path(),
        &[
            "trace",
            "derive-task",
            "--design",
            "1",
            "--requirement",
            "REQ-001",
            "--task",
            "1",
        ],
    );
    ok(
        temp.path(),
        &[
            "gate",
            "select",
            "--design",
            "1",
            "--template",
            "GATE-001",
            "--requirement",
            "REQ-001",
            "--task",
            "1",
            "--command",
            "cargo test",
        ],
    );
    ok(
        temp.path(),
        &[
            "repository",
            "add",
            "main",
            "--path",
            ".",
            "--head",
            "abc123",
            "--status",
            "clean",
        ],
    );
    ok(
        temp.path(),
        &[
            "repository",
            "snapshot",
            "add",
            "--repository",
            "main",
            "--head",
            "abc123",
            "--branch",
            "master",
            "--status",
            "clean",
            "--clean",
        ],
    );
    ok(
        temp.path(),
        &[
            "command",
            "usage",
            "add",
            "--command",
            "cargo test",
            "--result",
            "pass",
            "--work-unit",
            "1",
            "--snapshot",
            "1",
            "--log",
            ".agent-workbench/logs/cargo-test.log",
        ],
    );

    let record = ok(
        temp.path(),
        &[
            "gate",
            "record",
            "--gate",
            "1",
            "--result",
            "pass",
            "--usage",
            "1",
            "--snapshot",
            "1",
            "--artifact",
            ".agent-workbench/logs/cargo-test.log",
            "--artifact-hash",
            "sha256:abc",
            "--notes",
            "full test suite passed",
        ],
    );
    let list = ok(temp.path(), &["gate", "run", "list", "--gate", "1"]);
    let conn = conn(temp.path());
    let linked: (i64, i64, String) = conn
        .query_row(
            "select command_usage_id, repository_snapshot_id, result from validation_runs where id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    let artifact: (String, String, i64) = conn
        .query_row(
            "select artifact_type, identity_key, validation_run_id from artifacts where id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();

    assert!(record.contains("recorded validation run"));
    assert!(record.contains("validation_run_id: 1"));
    assert!(record.contains("work_unit_id: 1"));
    assert!(record.contains("task_id: 1"));
    assert!(list.contains("1 [gate=1 GATE-001:pass] usage=1 snapshot=1"));
    assert_eq!(linked, (1, 1, "pass".to_string()));
    assert_eq!(
        artifact,
        ("validation_output".to_string(), "sha256:abc".to_string(), 1)
    );
}

#[test]
fn gate_select_can_record_command_profile_and_timeout() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(temp.path(), &["work", "start", "implementation"]);
    ok(
        temp.path(),
        &[
            "task",
            "add",
            "implement cleanup",
            "--priority",
            "high",
            "--source",
            "design",
            "--completion-condition",
            "cleanup behavior is covered",
        ],
    );
    ok(
        temp.path(),
        &[
            "design",
            "init",
            "storage-lifecycle",
            "--title",
            "Storage Lifecycle",
        ],
    );
    write_requirement(temp.path());
    write_gate_template(temp.path());
    ok(
        temp.path(),
        &[
            "design",
            "import",
            ".agent-workbench/designs/storage-lifecycle",
            "--status",
            "draft",
        ],
    );
    ok(
        temp.path(),
        &[
            "trace",
            "derive-task",
            "--design",
            "1",
            "--requirement",
            "REQ-001",
            "--task",
            "1",
        ],
    );
    ok(
        temp.path(),
        &[
            "command",
            "prefer",
            "--name",
            "cleanup-test",
            "--type",
            "validation",
            "--command",
            "cargo test cleanup",
            "--timeout",
            "120s",
        ],
    );

    let selected = ok(
        temp.path(),
        &[
            "gate",
            "select",
            "--design",
            "1",
            "--template",
            "GATE-001",
            "--requirement",
            "REQ-001",
            "--task",
            "1",
            "--command-profile",
            "cleanup-test",
            "--timeout",
            "180s",
        ],
    );
    let row: (Option<i64>, Option<String>, Option<String>) = conn(temp.path())
        .query_row(
            "select command_profile_id, command, timeout from validation_gates where id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();

    assert!(selected.contains("selected validation gate"));
    assert_eq!(
        row,
        (
            Some(1),
            Some("cargo test cleanup".to_string()),
            Some("180s".to_string())
        )
    );
}

#[test]
fn gate_resume_ready_defaults_to_read_only_and_reports_blocked() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);

    let output = ok(temp.path(), &["gate", "resume-ready"]);
    assert!(output.contains("gate: resume-ready"));
    assert!(output.contains("dry_run: true"));
    assert!(output.contains("result: blocked"));
    assert!(output.contains("blocking_reason: no suspended activation to resume"));
}

#[test]
fn gate_close_ready_reports_active_work_readiness() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);

    ok(temp.path(), &["work", "start", "close ready"]);
    cli_record_close_evidence(temp.path(), 1, 1);

    let output = ok(temp.path(), &["gate", "close-ready"]);

    assert!(output.contains("gate: close-ready"));
    assert!(output.contains("result: pass"));
    assert!(output.contains("open_tasks_closed: pass ["));
    assert!(output.contains("validation_runs_recorded: pass ["));
    assert!(output.contains("repository_state_recorded: pass ["));
    assert!(output.contains("review_plans_clean: pass ["));
}

#[test]
fn work_lifecycle_commands_block_unblock_and_abandon() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(temp.path(), &["work", "start", "lifecycle"]);

    let blocked = ok(
        temp.path(),
        &["work", "block", "--reason", "waiting for decision"],
    );
    let unblocked = ok(
        temp.path(),
        &["work", "unblock", "1", "--reason", "decision recorded"],
    );
    let abandoned = ok(
        temp.path(),
        &["work", "abandon", "1", "--reason", "restart from fork"],
    );
    let status: (String, String) = conn(temp.path())
        .query_row(
            r#"
            select w.status, a.status
            from work_units w
            join work_unit_activations a on a.work_unit_id = w.id
            where w.id = 1
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert!(blocked.contains("blocked work unit"));
    assert!(blocked.contains("status: blocked"));
    assert!(unblocked.contains("unblocked work unit"));
    assert!(unblocked.contains("previous_status: blocked"));
    assert!(abandoned.contains("abandoned work unit"));
    assert!(abandoned.contains("status: abandoned"));
    assert_eq!(status, ("abandoned".to_string(), "abandoned".to_string()));
}

#[test]
fn gate_resume_ready_repo_aware_uses_repository_comparisons() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(temp.path(), &["work", "start", "repo aware resume"]);
    ok(
        temp.path(),
        &[
            "repository",
            "add",
            "main",
            "--path",
            ".",
            "--head",
            "abc123",
            "--status",
            "clean",
        ],
    );
    ok(
        temp.path(),
        &[
            "repository",
            "snapshot",
            "add",
            "--repository",
            "main",
            "--activation",
            "1",
            "--head",
            "abc123",
            "--branch",
            "master",
            "--status",
            "clean",
            "--clean",
        ],
    );
    ok(
        temp.path(),
        &[
            "work",
            "suspend",
            "--reason",
            "interrupt",
            "--next",
            "resume repo work",
        ],
    );
    ok(
        temp.path(),
        &[
            "repository",
            "snapshot",
            "add",
            "--repository",
            "main",
            "--head",
            "abc123",
            "--branch",
            "master",
            "--status",
            "M src/lib.rs",
        ],
    );
    ok(
        temp.path(),
        &[
            "repository",
            "dirty",
            "add",
            "--snapshot",
            "2",
            "--path",
            "src/lib.rs",
            "--type",
            "modified",
        ],
    );

    let blocked = ok(
        temp.path(),
        &[
            "gate",
            "resume-ready",
            "--maturity",
            "repo-aware",
            "--dry-run",
        ],
    );
    ok(
        temp.path(),
        &[
            "repository",
            "compare",
            "add",
            "--base",
            "1",
            "--current",
            "2",
            "--type",
            "resume",
            "--dirty-changed",
            "--result",
            "changed_classified",
        ],
    );
    let still_blocked = ok(
        temp.path(),
        &[
            "gate",
            "resume-ready",
            "--maturity",
            "repo-aware",
            "--dry-run",
        ],
    );
    let classification = ok(
        temp.path(),
        &[
            "repository",
            "classify",
            "add",
            "--snapshot",
            "2",
            "--dirty-entry",
            "1",
            "--classification",
            "expected",
            "--reason",
            "implementation edit",
        ],
    );
    let passed = ok(
        temp.path(),
        &[
            "gate",
            "resume-ready",
            "--maturity",
            "repo-aware",
            "--dry-run",
        ],
    );

    assert!(blocked.contains("result: blocked"));
    assert!(blocked.contains("repository_state_current: fail"));
    assert!(still_blocked.contains("result: blocked"));
    assert!(still_blocked.contains("repository_state_current: fail"));
    assert!(classification.contains("repository_state_classification_id: 1"));
    assert!(passed.contains("maturity: repo-aware"));
    assert!(passed.contains("result: pass"));
    assert!(passed.contains("repository_state_current: pass"));
}

#[test]
fn resume_check_records_requested_maturity() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(temp.path(), &["work", "start", "trace gate"]);
    ok(
        temp.path(),
        &[
            "work",
            "suspend",
            "--reason",
            "need trace check",
            "--next",
            "resume trace work",
        ],
    );

    let output = ok(temp.path(), &["resume-check", "--maturity", "trace-aware"]);
    assert!(output.contains("result: allowed"));

    let saved: String = conn(temp.path())
        .query_row(
            "select maturity from resume_checks where id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(saved, "trace-aware");
}

#[test]
fn reopen_and_follow_up_preserve_stack_and_dependencies() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(temp.path(), &["work", "start", "source"]);
    let source_snapshot = cli_record_close_evidence(temp.path(), 1, 1);
    ok(temp.path(), &["work", "close", "--summary", "source done"]);

    let authority = cli_approval_authority_event(temp.path());
    ok(
        temp.path(),
        &[
            "work",
            "reopen",
            "1",
            "--reason",
            "closure invalid",
            "--authority",
            &authority,
        ],
    );
    let reopened_activation_id: i64 = conn(temp.path())
        .query_row(
            "select id from work_unit_activations where status = 'active'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let reopened_snapshot = cli_record_close_evidence(temp.path(), 1, reopened_activation_id);
    ok(
        temp.path(),
        &[
            "repository",
            "compare",
            "add",
            "--base",
            &source_snapshot.to_string(),
            "--current",
            &reopened_snapshot.to_string(),
            "--type",
            "close",
            "--result",
            "same",
        ],
    );
    ok(
        temp.path(),
        &["work", "close", "--summary", "source redone"],
    );

    ok(temp.path(), &["work", "start", "mainline"]);
    let output = ok(
        temp.path(),
        &[
            "work",
            "follow-up",
            "1",
            "follow-up",
            "--reason",
            "new issue",
        ],
    );
    assert!(output.contains("created follow-up work unit"));
    assert!(output.contains("source_work_unit_id: 1"));

    let conn = conn(temp.path());
    let invalidates_closure: i64 = conn
        .query_row(
            "select count(*) from work_unit_dependencies where work_unit_id = 1 and depends_on_work_unit_id = 1 and dependency_type = 'invalidates_closure'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let follow_up_of: i64 = conn
        .query_row(
            r#"
            select count(*)
            from work_unit_dependencies d
            join work_units w on w.id = d.work_unit_id
            where w.title = 'follow-up'
              and d.depends_on_work_unit_id = 1
              and d.dependency_type = 'follow_up_of'
            "#,
            [],
            |row| row.get(0),
        )
        .unwrap();
    let mainline_status: String = conn
        .query_row(
            r#"
            select a.status
            from work_unit_activations a
            join work_units w on w.id = a.work_unit_id
            where w.title = 'mainline'
            "#,
            [],
            |row| row.get(0),
        )
        .unwrap();
    let follow_up_frame: (Option<i64>, i64) = conn
        .query_row(
            r#"
            select a.parent_activation_id, a.stack_depth
            from work_unit_activations a
            join work_units w on w.id = a.work_unit_id
            where w.title = 'follow-up'
            "#,
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert_eq!(invalidates_closure, 1);
    assert_eq!(follow_up_of, 1);
    assert_eq!(mainline_status, "suspended");
    assert_eq!(follow_up_frame.1, 1);
    assert!(follow_up_frame.0.is_some());
}

#[test]
fn fork_rejects_non_default_discard_policy() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);

    let error = err(
        temp.path(),
        &[
            "work",
            "fork",
            "redo",
            "--from-commit",
            "abc123",
            "--reason",
            "failed_validation",
            "--discard-policy",
            "mark_abandoned",
        ],
    );
    assert!(error.contains("keep_history"));
}

#[test]
fn task_accept_out_of_scope_creates_acceptance_record() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(temp.path(), &["work", "start", "scoped work"]);
    ok(temp.path(), &["task", "add", "outside scope"]);

    let output = ok(
        temp.path(),
        &[
            "task",
            "accept-out-of-scope",
            "1",
            "--reason",
            "not needed now",
        ],
    );
    assert!(output.contains("accepted task out of scope"));
    assert!(output.contains("acceptance_record_id: 1"));
    assert!(output.contains("authority_event_id: "));

    let status: String = conn(temp.path())
        .query_row(
            "select status from acceptance_records where id = 1 and task_id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "approved");
}

#[test]
fn review_flow_records_policy_runs_findings_and_verification() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(temp.path(), &["work", "start", "reviewed work"]);

    let policy = ok(
        temp.path(),
        &[
            "review",
            "policy",
            "add",
            "--name",
            "strict-impl",
            "--type",
            "implementation_review",
            "--fresh-clean",
            "0",
            "--resume-clean",
            "1",
            "--max-fresh-agents",
            "1",
            "--max-resume-agents",
            "2",
            "--allow-new-findings-in-resume",
            "--run-count-scope",
            "work_unit",
            "--default-run-mode",
            "resume",
        ],
    );
    let scope = ok(
        temp.path(),
        &[
            "review",
            "scope",
            "start",
            "impl-scope",
            "--type",
            "implementation_review",
            "--scope",
            "implementation quality",
        ],
    );
    let plan = ok(
        temp.path(),
        &[
            "review",
            "plan",
            "add",
            "--work-unit",
            "1",
            "--type",
            "implementation_review",
            "--stage",
            "close-ready",
            "--policy",
            "1",
            "--review-scope",
            "1",
        ],
    );
    let context = ok(temp.path(), &["review", "plan", "context", "1"]);
    let fresh_run = ok(
        temp.path(),
        &[
            "review",
            "run",
            "add",
            "--plan",
            "1",
            "--type",
            "fresh",
            "--purpose",
            "new_unbiased_review",
            "--target",
            "HEAD",
            "--new-findings",
            "1",
            "--summary",
            "found issue",
        ],
    );
    let finding = ok(
        temp.path(),
        &[
            "finding",
            "add",
            "--run",
            "1",
            "--type",
            "implementation_finding",
            "--severity",
            "high",
            "--description",
            "missing error handling",
        ],
    );
    let kpt = ok(
        temp.path(),
        &[
            "kpt",
            "start",
            "--summary",
            "triage open findings",
            "--from",
            "findings",
        ],
    );
    let classified = ok(
        temp.path(),
        &["finding", "classify", "1", "--classification", "valid"],
    );
    let closure = ok(
        temp.path(),
        &[
            "closure",
            "add",
            "--finding",
            "1",
            "--invariant",
            "errors are surfaced",
            "--evidence",
            "abc123",
            "--tests",
            "cargo test",
        ],
    );
    let resume_run = ok(
        temp.path(),
        &[
            "review",
            "run",
            "add",
            "--plan",
            "1",
            "--type",
            "resume",
            "--purpose",
            "finding_fix_verification",
            "--target",
            "HEAD",
            "--clean",
            "--carried-findings",
            "1",
            "--summary",
            "verified",
        ],
    );
    let verification = ok(
        temp.path(),
        &[
            "finding",
            "verify",
            "--run",
            "2",
            "--finding",
            "1",
            "--closure",
            "1",
            "--result",
            "verified",
        ],
    );
    let plans = ok(temp.path(), &["review", "plan", "list"]);
    let policies = ok(temp.path(), &["review", "policy", "list"]);
    let findings = ok(temp.path(), &["finding", "list"]);

    assert!(policy.contains("review_policy_id: 1"));
    assert!(policies.contains("resume_new=true count_scope=work_unit default_mode=resume"));
    assert!(scope.contains("review_scope_id: 1"));
    assert!(plan.contains("review_plan_id: 1"));
    assert!(context.contains("target 1 [work_unit] work_unit_id=1"));
    assert!(fresh_run.contains("plan_status: open"));
    assert!(finding.contains("finding_id: 1"));
    assert!(kpt.contains("generated_item_count: 1"));
    assert!(classified.contains("classified finding"));
    assert!(closure.contains("closure_id: 1"));
    assert!(resume_run.contains("review_run_id: 2"));
    assert!(verification.contains("finding_verification_id: 1"));
    assert!(plans.contains("1 [implementation_review:clean required=true]"));
    assert!(findings.contains("1 [run=1 implementation_finding:high closed]"));
}

#[test]
fn skill_references_use_executable_review_and_fork_examples() {
    let review_recipes =
        std::fs::read_to_string("skills/agent-workbench/references/review-recipes.md").unwrap();
    let interruption_recovery =
        std::fs::read_to_string("skills/agent-workbench/references/interruption-recovery.md")
            .unwrap();
    let cli_workflow =
        std::fs::read_to_string("skills/agent-workbench/references/cli-workflow.md").unwrap();

    assert!(review_recipes.contains("finding add --run <review-run-id>"));
    assert!(review_recipes.contains("--description \"<description>\""));
    assert!(review_recipes.contains("--result verified"));
    assert!(!review_recipes.contains("--summary \"<summary>\""));
    assert!(!review_recipes.contains("--result fixed"));
    assert!(interruption_recovery.contains("--reason design_changed"));
    assert!(interruption_recovery.contains("Known fork reasons are `design_changed`"));
    assert!(!interruption_recovery.contains("--reason design_change\n"));
    assert!(
        cli_workflow
            .contains("gate implementation-ready --design-version <design-version-id> --dry-run")
    );
}

#[test]
fn kpt_item_convert_can_create_fixed_command_profile() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(
        temp.path(),
        &[
            "kpt",
            "start",
            "--scope",
            "project",
            "--summary",
            "command tuning",
        ],
    );
    ok(
        temp.path(),
        &[
            "kpt",
            "item",
            "add",
            "--type",
            "try",
            "--title",
            "stable validation command",
            "--details",
            "cargo test --workspace",
        ],
    );
    let authority = cli_approval_authority_event(temp.path());

    let converted = ok(
        temp.path(),
        &[
            "kpt",
            "item",
            "convert",
            "--item",
            "1",
            "--to",
            "command-profile",
            "--name",
            "workspace-tests",
            "--command-type",
            "test",
            "--scope",
            "project",
            "--command-status",
            "fixed",
            "--stability",
            "stable",
            "--expected-result",
            "pass",
            "--authority",
            &authority,
        ],
    );
    let commands = ok(temp.path(), &["command", "list", "--type", "test"]);
    let rules = ok(temp.path(), &["rules", "applicable", "--scope", "project"]);
    let usage = ok(
        temp.path(),
        &[
            "command",
            "usage",
            "add",
            "--command",
            "cargo test -p agent-workbench",
            "--result",
            "pass",
        ],
    );
    let promoted_preferred = ok(
        temp.path(),
        &[
            "command",
            "usage",
            "promote",
            "1",
            "--name",
            "focused-tests",
            "--type",
            "test",
            "--scope",
            "project",
        ],
    );
    let user_authority = ok(
        temp.path(),
        &[
            "authority",
            "event",
            "add",
            "--type",
            "user_instruction",
            "--scope",
            "project",
            "--summary",
            "fix the validation command",
        ],
    );
    let authority_id: i64 = conn(temp.path())
        .query_row(
            "select id from authority_events where event_type = 'user_instruction' order by id desc limit 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let authority_id = authority_id.to_string();
    let promoted_fixed = ok(
        temp.path(),
        &[
            "command",
            "usage",
            "promote",
            "1",
            "--name",
            "fixed-focused-tests",
            "--type",
            "test",
            "--scope",
            "project",
            "--status",
            "fixed",
            "--authority",
            &authority_id,
        ],
    );

    assert!(converted.contains("command_profile_id: 1"));
    assert!(commands.contains("1 [test:fixed] workspace-tests = cargo test --workspace"));
    assert!(rules.contains("[command_profile:project precedence=70]"));
    assert!(usage.contains("command_usage_id: 1"));
    assert!(promoted_preferred.contains("command_profile_id: 2"));
    assert!(promoted_preferred.contains("status: preferred"));
    assert!(user_authority.contains("authority_event_id: "));
    assert!(promoted_fixed.contains("command_profile_id: 3"));
    assert!(promoted_fixed.contains("status: fixed"));
}

#[test]
fn cli_git_import_backfills_manual_work_record_links() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(
        temp.path(),
        &[
            "repository",
            "add",
            "main",
            "--path",
            ".",
            "--status",
            "clean",
        ],
    );
    ok(
        temp.path(),
        &[
            "record",
            "create",
            "--topic",
            "manual evidence before git import",
        ],
    );
    ok(
        temp.path(),
        &[
            "record", "commit", "add", "1", "--sha", "abc123", "--role", "created",
        ],
    );
    ok(
        temp.path(),
        &[
            "record",
            "file",
            "add",
            "1",
            "--path",
            "src/lib.rs",
            "--role",
            "changed",
        ],
    );

    let commit = ok(
        temp.path(),
        &[
            "repository",
            "commit",
            "add",
            "--repository",
            "main",
            "--sha",
            "abc123",
            "--short",
            "abc123",
            "--subject",
            "backfill",
        ],
    );
    let file = ok(
        temp.path(),
        &[
            "repository",
            "file",
            "add",
            "--commit",
            "1",
            "--path",
            "src/lib.rs",
            "--type",
            "modified",
        ],
    );

    let conn = conn(temp.path());
    let linked_commit_id: i64 = conn
        .query_row(
            "select git_commit_id from work_record_commits where work_record_id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let linked_file: (i64, i64) = conn
        .query_row(
            "select git_file_change_id, repository_id from work_record_files where work_record_id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();

    assert!(commit.contains("git_commit_id: 1"));
    assert!(file.contains("git_file_change_id: 1"));
    assert_eq!(linked_commit_id, 1);
    assert_eq!(linked_file, (1, 1));
}

#[test]
fn design_cli_aliases_record_commands_authority_git_and_links() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);

    let preferred = ok(
        temp.path(),
        &[
            "command",
            "prefer",
            "--name",
            "workspace-tests",
            "--type",
            "test",
            "--scope",
            "project",
            "--command",
            "cargo test --workspace",
        ],
    );
    let deprecated = ok(
        temp.path(),
        &[
            "command",
            "deprecate",
            "--name",
            "workspace-tests",
            "--reason",
            "too broad for this work",
        ],
    );
    let authority = ok(
        temp.path(),
        &[
            "authority",
            "add",
            "--path",
            ".agent-workbench/designs/storage-lifecycle/design.yaml",
            "--type",
            "design",
            "--scope",
            "project",
        ],
    );
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
            "old instruction",
            "--correction",
            "follow the authority event",
        ],
    );
    let rules = ok(temp.path(), &["rules", "applicable", "--scope", "project"]);
    ok(
        temp.path(),
        &[
            "repository",
            "add",
            "main",
            "--path",
            ".",
            "--status",
            "clean",
        ],
    );
    ok(
        temp.path(),
        &["record", "create", "--topic", "linked aliases"],
    );
    let git_commit = ok(
        temp.path(),
        &[
            "git",
            "commit",
            "add",
            "abc123",
            "--repo",
            "main",
            "--subject",
            "alias import",
        ],
    );
    let git_file = ok(
        temp.path(),
        &[
            "git",
            "files",
            "add",
            "--commit",
            "abc123",
            "--path",
            "src/lib.rs",
            "--type",
            "modified",
        ],
    );
    let linked_commit = ok(
        temp.path(),
        &[
            "record",
            "link",
            "commit",
            "--record",
            "1",
            "--git-commit",
            "1",
            "--sha",
            "abc123",
        ],
    );
    let linked_file = ok(
        temp.path(),
        &[
            "record",
            "link",
            "file",
            "--record",
            "1",
            "--git-file-change",
            "1",
            "--repository-id",
            "1",
            "--path",
            "src/lib.rs",
        ],
    );

    assert!(preferred.contains("command_profile_id: 1"));
    assert!(deprecated.contains("command_profile_id: 1"));
    assert!(authority.contains("authority_id: "));
    assert!(authority.contains("authority_event_id: "));
    assert!(rules.contains("user_correction"));
    assert!(git_commit.contains("git_commit_id: 1"));
    assert!(git_file.contains("git_file_change_id: 1"));
    assert!(linked_commit.contains("work_record_commit_id: 1"));
    assert!(linked_file.contains("work_record_file_id: 1"));
}

#[test]
fn work_resume_rejects_allowed_check_without_required_items() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(temp.path(), &["work", "start", "parent"]);
    ok(
        temp.path(),
        &[
            "work",
            "suspend",
            "--reason",
            "interrupt",
            "--next",
            "resume parent",
        ],
    );
    let conn = conn(temp.path());
    conn.execute(
        r#"
        insert into resume_checks(
            work_unit_id, work_unit_activation_id, suspend_snapshot_id, maturity,
            status, result, authority_event_high_watermark, activation_stack_revision,
            allowed_next_action, created_at
        )
        values (1, 1, 1, 'basic', 'pending', 'allowed', 0, 2,
                'resume parent', current_timestamp)
        "#,
        [],
    )
    .unwrap();

    let stderr = err(temp.path(), &["work", "resume", "--check", "1"]);

    assert!(stderr.contains("missing required item resume_target_suspended"));
}
