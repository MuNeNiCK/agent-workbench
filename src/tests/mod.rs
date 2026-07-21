use super::*;
use crate::db::{CORE_SCHEMA_VERSION, SCHEMA_VERSION, open_ledger};
use rusqlite::params;
use std::fs;

// Library invariants are owned here by product responsibility. Product modules expose only the
// narrow crate-visible seams required to exercise an invariant and do not contain test modules.

mod database;
mod decomposition;
mod design;
mod governance;
mod identity;
mod migration;
mod repository;
mod review;
mod update;
mod work;

fn apply_test_update(root: &std::path::Path) -> UpdateApplyOutcome {
    let inspection = inspect_update(root).unwrap();
    apply_update(root, &inspection.current_identity).unwrap()
}

fn retain_core_storage_only(conn: &rusqlite::Connection) {
    let foreign_keys: i64 = conn
        .pragma_query_value(None, "foreign_keys", |row| row.get(0))
        .unwrap();
    conn.pragma_update(None, "foreign_keys", false).unwrap();
    conn.execute_batch(
        r#"
        drop view if exists correction_decomposition_task_memberships;
        drop trigger if exists trg_decomposition_plan_ingress_links_insert;
        drop trigger if exists trg_decomposition_plan_ingress_immutable_update;
        drop trigger if exists trg_decomposition_plan_ingress_immutable_delete;
        drop table if exists decomposition_plan_ingress_identities;
        drop trigger if exists trg_finding_verification_project_insert;
        drop trigger if exists trg_finding_verification_project_update;
        drop table if exists finding_design_recoveries;
        drop table if exists decomposition_reconciliation_results;
        drop table if exists decomposition_migration_sources;
        drop table if exists decomposition_lineage;
        drop table if exists decomposition_reconciliation_dependencies;
        drop table if exists decomposition_reconciliation_applications;
        drop table if exists decomposition_reconciliation_phases;
        drop table if exists decomposition_reconciliation_gates;
        drop table if exists decomposition_reconciliation_checklist_items;
        drop table if exists decomposition_reconciliation_tasks;
        drop table if exists decomposition_application_dependencies;
        drop table if exists decomposition_application_gates;
        drop table if exists decomposition_application_boundaries;
        drop table if exists decomposition_application_requirements;
        drop table if exists decomposition_applications;
        drop table if exists decomposition_item_checklist_boundary_gates;
        drop table if exists decomposition_item_gates;
        drop table if exists decomposition_item_checklist_boundaries;
        drop table if exists decomposition_item_requirements;
        drop table if exists decomposition_items;
        drop table if exists decomposition_slice_dependencies;
        drop table if exists decomposition_slices;
        drop table if exists decomposition_plans;
        drop trigger if exists trg_release_candidate_boundary_insert;
        drop trigger if exists trg_release_candidate_boundary_update;
        drop trigger if exists trg_release_candidate_boundary_delete;
        drop table if exists release_candidate_boundaries;
        drop table if exists release_candidate_attempts;
        drop table if exists release_candidate_subject_revisions;
        drop table if exists release_candidate_revisions;
        drop table if exists release_candidate_events;
        drop table if exists release_candidate_assets;
        drop table if exists release_candidates;
        drop table if exists update_receipts;
        drop table if exists update_decisions;
        drop table if exists update_operations;
        delete from schema_migrations where version > 13;
        create view correction_decomposition_task_memberships as
        select cast(null as integer) correction_application_id,
               cast(null as integer) task_id
        where 0;
        "#,
    )
    .unwrap();
    conn.execute_batch(crate::db::REVIEW_INTEGRITY_SQL).unwrap();
    conn.pragma_update(None, "foreign_keys", foreign_keys != 0)
        .unwrap();
}

fn add_review_run(
    root: &std::path::Path,
    input: NewReviewRun<'_>,
) -> anyhow::Result<ReviewRunOutcome> {
    let accepted = input.clean_run
        && input.status == "completed"
        && matches!(input.review_provenance, "external_agent" | "human_review")
        && input.review_provenance_ref.is_some();
    let outcome = crate::review::add_review_run(root, input)?;
    if accepted {
        record_accepted_review_claim(root, outcome.review_run_id);
    }
    Ok(outcome)
}

fn record_accepted_review_claim(root: &std::path::Path, run_id: i64) {
    adjudicate_review(
        root,
        run_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "test accepted claim",
            expected_current: "pending",
        },
    )
    .unwrap();
}

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

fn approval_authority_event(root: &std::path::Path) -> i64 {
    add_authority_event(
        root,
        NewAuthorityEvent {
            event_type: "user_instruction",
            source: Some("test-user"),
            summary: "approve exception for test",
            scope: Some("test"),
            precedence: 100,
        },
    )
    .unwrap()
    .authority_event_id
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
