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
    assert!(output.contains("result: blocked"));
    assert!(output.contains("trace-aware checks are not implemented yet"));

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
