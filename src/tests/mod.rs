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
