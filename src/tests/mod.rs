use super::*;
use crate::db::{SCHEMA_VERSION, open_ledger};
use rusqlite::params;
use std::fs;

mod design_trace;
mod init_migrations;
mod repository_ledger;
mod review;
mod rules_kpt;
mod work_memory;
mod work_stack;

fn requirement_doc(key: &str, title: &str, priority: &str) -> String {
    format!(
        r#"## {key}: {title}
```yaml agent-workbench
type: requirement
key: {key}
priority: {priority}
surfaces: [cli, database]
validation: [GATE-001]
status: active
```

This requirement describes one verifiable behavior that must be implemented.
"#
    )
}

fn requirement_doc_without_validation(key: &str, title: &str, priority: &str) -> String {
    format!(
        r#"## {key}: {title}
```yaml agent-workbench
type: requirement
key: {key}
priority: {priority}
surfaces: [cli, database]
status: active
```

This requirement describes behavior whose validation is intentionally unresolved.
"#
    )
}

fn decision_doc() -> String {
    r#"## DEC-001: Keep project-local ledger
```yaml agent-workbench
type: decision
key: DEC-001
status: accepted
supersedes: []
```

Use one SQLite ledger per project.
"#
    .to_string()
}

fn validation_gate_doc(key: &str) -> String {
    format!(
        r#"## {key}: Unit test command
```yaml agent-workbench
type: validation_gate_template
key: {key}
applies_to: [REQ-001]
expected_result: pass
phase: implementation
status: active
```

Run the project test suite before implementation handoff.
"#
    )
}

fn record_close_evidence(
    root: &std::path::Path,
    work_unit_id: i64,
    activation_id: i64,
) -> RepositorySnapshotOutcome {
    create_work_record(
        root,
        NewWorkRecord {
            work_unit_id: Some(work_unit_id),
            topic: "close evidence",
            work_performed: Some("recorded close readiness evidence"),
            next_actions: None,
            notable_operations: None,
            export_path: None,
        },
    )
    .unwrap();
    if list_repositories(root)
        .unwrap()
        .iter()
        .all(|repo| repo.name != "main")
    {
        add_repository(
            root,
            NewRepository {
                name: "main",
                path: ".",
                current_head: Some("abc123"),
                status_summary: Some("clean"),
            },
        )
        .unwrap();
    }
    add_repository_snapshot(
        root,
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(activation_id),
            head_sha: Some("abc123"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap()
}
