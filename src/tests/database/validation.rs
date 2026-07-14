use super::*;

#[test]
fn init_rejects_legacy_artifacts_with_invalid_validation_links() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        insert into work_units(project_id, title, status, started_at)
        values (1, 'legacy validation evidence', 'open', current_timestamp);

        insert into repositories(project_id, name, path, status_summary, last_checked_at)
        values (1, 'main', '.', 'clean', current_timestamp);

        insert into repository_snapshots(
            repository_id, head_sha, branch, status_summary, is_clean, created_at
        )
        values (1, 'abc123', 'master', 'clean', 1, current_timestamp);

        insert into repository_snapshots(
            repository_id, head_sha, branch, status_summary, is_clean, created_at
        )
        values (1, 'def456', 'master', 'clean', 1, current_timestamp);

        insert into command_usages(
            project_id, work_unit_id, command, result, repository_snapshot_id, created_at
        )
        values (1, 1, 'cargo test', 'pass', 1, current_timestamp);

        insert into command_usages(
            project_id, work_unit_id, command, result, repository_snapshot_id, created_at
        )
        values (1, 1, 'cargo test', 'pass', 2, current_timestamp);

        insert into validation_gates(
            project_id, gate_key, work_unit_id, expected_result, status, created_at
        )
        values (1, 'GATE-001', 1, 'pass', 'active', current_timestamp);

        insert into validation_runs(
            project_id, validation_gate_id, work_unit_id, command_usage_id,
            repository_snapshot_id, result, created_at
        )
        values (1, 1, 1, 1, 1, 'pass', current_timestamp);

        drop trigger trg_artifact_project_insert;
        drop trigger trg_artifact_project_update;

        insert into artifacts(
            project_id, artifact_type, identity_key, artifact_path,
            validation_run_id, command_usage_id, repository_snapshot_id, created_at
        )
        values (
            1, 'validation_output', 'mismatched-artifact',
            '.agent-workbench/logs/mismatched.log',
            1, 2, 1, current_timestamp
        );
        "#,
    )
    .unwrap();
    drop(conn);

    let result = init_project(temp.path());

    assert!(result.is_err());
}

#[test]
fn init_rejects_legacy_validation_runs_with_gate_work_mismatch() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        insert into work_units(project_id, title, status, started_at)
        values (1, 'gate work', 'open', current_timestamp);

        insert into work_units(project_id, title, status, started_at)
        values (1, 'wrong validation work', 'open', current_timestamp);

        insert into tasks(title, priority, source, work_unit_id, status)
        values ('gate task', 'high', 'design', 1, 'open');

        insert into tasks(title, priority, source, work_unit_id, status)
        values ('wrong task', 'high', 'design', 2, 'open');

        insert into validation_gates(
            project_id, gate_key, work_unit_id, task_id, expected_result, status, created_at
        )
        values (1, 'GATE-001', 1, 1, 'pass', 'active', current_timestamp);

        drop trigger trg_validation_run_project_insert;
        drop trigger trg_validation_run_project_update;

        insert into validation_runs(
            project_id, validation_gate_id, work_unit_id, task_id, result, created_at
        )
        values (1, 1, 2, 2, 'pass', current_timestamp);
        "#,
    )
    .unwrap();
    drop(conn);

    let result = init_project(temp.path());

    assert!(result.is_err());
}
