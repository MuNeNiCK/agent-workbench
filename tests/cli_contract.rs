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
fn design_approve_allows_design_ready_gate_to_pass() {
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
    assert!(blocked.contains("design_version_approved: fail"));

    let approved = ok(
        temp.path(),
        &[
            "design",
            "approve",
            "1",
            "--summary",
            "design passed document checks",
        ],
    );
    assert!(approved.contains("approved design version"));
    assert!(approved.contains("authority_event_id: 1"));

    let passed = ok(
        temp.path(),
        &["gate", "design-ready", "--design-version", "1", "--dry-run"],
    );
    assert!(passed.contains("result: pass"));
    assert!(passed.contains("design_version_approved: pass"));

    let conn = conn(temp.path());
    let approved_by: i64 = conn
        .query_row(
            "select approved_by_authority_event_id from design_versions where id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(approved_by, 1);
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
            "--tests-or-gates",
            "GATE-001",
        ],
    );
    let coverage_list = ok(
        temp.path(),
        &["coverage", "list", "--design", "1", "--status", "covered"],
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
        ],
    );

    assert!(derivation.contains("derived task from requirement"));
    assert!(derivation.contains("task_derivation_id: 1"));
    assert!(list.contains("requirement=REQ-001 task=1"));
    assert!(without_gate.contains("validation_gates_selected: fail"));
    assert!(selected.contains("selected validation gate"));
    assert!(selected.contains("validation_gate_id: 1"));
    assert!(passed.contains("result: pass"));
    assert!(passed.contains("task_derivations_exist: pass"));
    assert!(passed.contains("validation_gates_selected: pass"));
    assert!(close_without_trace.contains("cannot close design-derived task"));
    assert!(evidence.contains("added implementation evidence"));
    assert!(evidence.contains("implementation_evidence_id: 1"));
    assert!(evidence_list.contains("1 [commit] task=1 requirement=REQ-001 commit=abc123"));
    assert!(coverage.contains("added coverage item"));
    assert!(coverage.contains("coverage_item_id: 1"));
    assert!(coverage_list.contains("1 [covered] requirement=REQ-001"));
    assert!(raw_out_of_scope_coverage.contains("requires an approved acceptance record"));
    assert!(passed_after_close.contains("result: pass"));
    assert!(passed_after_close.contains("implementation_evidence_present: pass"));
    assert!(passed_after_close.contains("coverage_items_present: pass"));
    assert!(partial_coverage.contains("coverage_item_id: 2"));
    assert!(coverage_acceptance.contains("target_type: coverage_item"));
    assert!(coverage_acceptance.contains("coverage_item_id: 2"));
}

#[test]
fn gate_resume_ready_requires_dry_run_and_reports_blocked() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);

    let error = err(temp.path(), &["gate", "resume-ready"]);
    assert!(error.contains("pass --dry-run"));

    let output = ok(temp.path(), &["gate", "resume-ready", "--dry-run"]);
    assert!(output.contains("gate: resume-ready"));
    assert!(output.contains("dry_run: true"));
    assert!(output.contains("result: blocked"));
    assert!(output.contains("blocking_reason: no suspended activation to resume"));
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
    ok(temp.path(), &["work", "close", "--summary", "source done"]);

    ok(
        temp.path(),
        &["work", "reopen", "1", "--reason", "closure invalid"],
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
    assert!(output.contains("authority_event_id: 1"));

    let status: String = conn(temp.path())
        .query_row(
            "select status from acceptance_records where id = 1 and task_id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "approved");
}
