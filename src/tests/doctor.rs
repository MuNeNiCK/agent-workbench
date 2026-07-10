use std::path::Path;

use rusqlite::Connection;

use super::*;

fn corrupt_repairable_validation_links(root: &Path) {
    init_project(root).unwrap();
    let conn = open_ledger(&default_ledger_path(root)).unwrap();
    conn.execute_batch(
        r#"
        insert into projects(id, name, root_path, created_at, updated_at)
        values (2, 'legacy-other', '/legacy-other', current_timestamp, current_timestamp);

        insert into work_units(id, project_id, title, status, started_at)
        values (1, 1, 'gate work', 'open', current_timestamp);
        insert into work_units(id, project_id, title, status, started_at)
        values (2, 2, 'legacy wrong work', 'open', current_timestamp);

        insert into tasks(id, title, priority, source, work_unit_id, status)
        values (1, 'gate task', 'high', 'design', 1, 'open');
        insert into tasks(id, title, priority, source, work_unit_id, status)
        values (2, 'legacy wrong task', 'high', 'design', 2, 'open');

        insert into validation_gates(
            id, project_id, gate_key, work_unit_id, task_id,
            expected_result, status, created_at
        ) values (1, 1, 'GATE-LEGACY', 1, 1, 'pass', 'active', current_timestamp);

        insert into repositories(id, project_id, name, path)
        values (1, 1, 'main', '.');
        insert into repositories(id, project_id, name, path)
        values (2, 2, 'legacy', '/legacy-other');
        insert into repository_snapshots(
            id, repository_id, head_sha, status_summary, is_clean, created_at
        ) values (1, 1, 'good', 'clean', 1, current_timestamp);
        insert into repository_snapshots(
            id, repository_id, head_sha, status_summary, is_clean, created_at
        ) values (2, 2, 'legacy', 'clean', 1, current_timestamp);

        insert into command_usages(
            id, project_id, work_unit_id, command, result,
            repository_snapshot_id, created_at
        ) values (1, 2, 2, 'cargo test', 'pass', 2, current_timestamp);

        insert into authority_events(
            project_id, event_type, text_or_summary, precedence, status, created_at
        ) values (1, 'user_instruction', 'approve repair fixture', 100, 'active', current_timestamp);

        drop trigger trg_validation_run_project_insert;
        drop trigger trg_validation_run_project_update;
        insert into validation_runs(
            id, project_id, validation_gate_id, work_unit_id, task_id,
            command_usage_id, repository_snapshot_id, result, created_at
        ) values (1, 2, 1, 2, 2, 1, 2, 'pass', current_timestamp);

        insert into acceptance_records(
            id, project_id, target_type, validation_run_id, acceptance_type,
            reason, created_by, status, approved_by_authority_event_id,
            approved_at, created_at
        ) values (
            1, 2, 'validation_run', 1, 'explicit_exception',
            'legacy accepted validation', 'user', 'approved',
            (select max(id) from authority_events),
            current_timestamp, current_timestamp
        );
        update validation_runs set acceptance_record_id = 1 where id = 1;

        insert into artifacts(
            id, project_id, artifact_type, identity_key, artifact_path,
            validation_run_id, command_usage_id, repository_snapshot_id, created_at
        ) values (
            1, 2, 'validation_output', 'legacy-output', 'legacy.log',
            1, 1, 2, current_timestamp
        );

        drop table validation_link_repair_changes;
        drop table validation_link_repair_runs;
        delete from schema_migrations where version = 8;
        insert into schema_migrations(version, applied_at)
        values (6, current_timestamp);
        "#,
    )
    .unwrap();
}

#[test]
fn doctor_repairs_legacy_validation_links_with_backup_audit_and_idempotence() {
    let temp = tempfile::tempdir().unwrap();
    corrupt_repairable_validation_links(temp.path());

    let normal = project_status(temp.path()).unwrap_err().to_string();
    assert!(normal.contains("doctor validation-links"));
    assert!(normal.contains("doctor validation-links --repair"));

    let diagnosis = diagnose_validation_links(temp.path()).unwrap();
    assert!(diagnosis.repairable);
    assert_eq!(diagnosis.runs.len(), 1);
    assert_eq!(diagnosis.runs[0].validation_run_id, 1);
    assert!(
        diagnosis.runs[0].changes.iter().any(|change| {
            change.entity_type == "artifact" && change.field_name == "project_id"
        })
    );
    assert!(diagnosis.runs[0].changes.iter().any(|change| {
        change.entity_type == "acceptance_record" && change.field_name == "project_id"
    }));

    let repair = repair_validation_links(temp.path()).unwrap();
    assert_eq!(repair.repaired_validation_run_count, 1);
    assert!(repair.change_count >= 9);
    assert!(repair.backup_path.as_ref().unwrap().is_file());
    let backup_conn = Connection::open(repair.backup_path.as_ref().unwrap()).unwrap();
    let backup_run = backup_conn
        .query_row(
            r#"
            select project_id, work_unit_id, task_id, command_usage_id,
                   repository_snapshot_id, command, acceptance_record_id
            from validation_runs where id = 1
            "#,
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                ))
            },
        )
        .unwrap();
    let backup_artifact = backup_conn
        .query_row(
            "select project_id, command_usage_id, repository_snapshot_id from artifacts where id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            },
        )
        .unwrap();
    let backup_acceptance_project: i64 = backup_conn
        .query_row(
            "select project_id from acceptance_records where id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let backup_schema_version: i64 = backup_conn
        .query_row("select max(version) from schema_migrations", [], |row| {
            row.get(0)
        })
        .unwrap();
    let backup_integrity: String = backup_conn
        .query_row("pragma integrity_check(1)", [], |row| row.get(0))
        .unwrap();
    let backup_audit_tables: i64 = backup_conn
        .query_row(
            "select count(*) from sqlite_schema where type = 'table' and name in ('validation_link_repair_runs', 'validation_link_repair_changes')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let backup_repair_runs: i64 = backup_conn
        .query_row(
            "select count(*) from validation_link_repair_runs",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let backup_repair_changes: i64 = backup_conn
        .query_row(
            "select count(*) from validation_link_repair_changes",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        backup_run,
        (2, Some(2), Some(2), Some(1), Some(2), None, Some(1))
    );
    assert_eq!(backup_artifact, (2, Some(1), Some(2)));
    assert_eq!(backup_acceptance_project, 2);
    assert_eq!(backup_schema_version, 6);
    assert_eq!(backup_integrity, "ok");
    assert_eq!(backup_audit_tables, 2);
    assert_eq!(backup_repair_runs, 0);
    assert_eq!(backup_repair_changes, 0);
    drop(backup_conn);

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let repaired = conn
        .query_row(
            r#"
            select project_id, work_unit_id, task_id, command_usage_id,
                   repository_snapshot_id, command, result
            from validation_runs where id = 1
            "#,
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        repaired,
        (
            1,
            Some(1),
            Some(1),
            None,
            None,
            Some("cargo test".to_string()),
            "pass".to_string(),
        )
    );
    let artifact = conn
        .query_row(
            "select project_id, command_usage_id, repository_snapshot_id from artifacts where id = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(artifact, (1, None, None));
    let acceptance_project: i64 = conn
        .query_row(
            "select project_id from acceptance_records where id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(acceptance_project, 1);
    drop(conn);

    assert_eq!(project_status(temp.path()).unwrap().schema_version, Some(8));
    let audit = list_validation_link_audit(temp.path()).unwrap();
    assert_eq!(audit.len(), 1);
    assert_eq!(audit[0].change_count as usize, repair.change_count);
    assert_eq!(
        audit[0].backup_path,
        repair.backup_path.unwrap().display().to_string()
    );

    let second = repair_validation_links(temp.path()).unwrap();
    assert_eq!(second.repaired_validation_run_count, 0);
    assert!(second.backup_path.is_none());
    assert_eq!(list_validation_link_audit(temp.path()).unwrap().len(), 1);

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    assert!(
        conn.execute(
            r#"
            insert into validation_runs(
                id, project_id, validation_gate_id, work_unit_id, task_id,
                result, created_at
            ) values (2, 1, 1, 2, 2, 'pass', current_timestamp)
            "#,
            [],
        )
        .is_err()
    );
    assert!(
        conn.execute(
            "update validation_runs set work_unit_id = 2 where id = 1",
            [],
        )
        .is_err()
    );
    assert!(
        conn.execute(
            "update validation_link_repair_changes set after_value = 'tampered' where id = 1",
            [],
        )
        .is_err()
    );
}

#[test]
fn doctor_refuses_missing_gate_without_mutating_or_backing_up() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let ledger = default_ledger_path(temp.path());
    let conn = Connection::open(&ledger).unwrap();
    conn.pragma_update(None, "foreign_keys", false).unwrap();
    conn.execute(
        r#"
        insert into validation_runs(
            id, project_id, validation_gate_id, result, created_at
        ) values (1, 1, 999, 'pass', current_timestamp)
        "#,
        [],
    )
    .unwrap();
    drop(conn);

    let diagnosis = diagnose_validation_links(temp.path()).unwrap();
    assert!(!diagnosis.repairable);
    assert_eq!(diagnosis.runs.len(), 1);
    assert!(repair_validation_links(temp.path()).is_err());

    let conn = Connection::open(&ledger).unwrap();
    let gate_id: i64 = conn
        .query_row(
            "select validation_gate_id from validation_runs where id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(gate_id, 999);
    let audit_count: i64 = conn
        .query_row(
            "select count(*) from validation_link_repair_runs",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(audit_count, 0);
    assert!(!temp.path().join(".agent-workbench/backups").exists());
}

#[test]
fn doctor_rolls_back_rows_and_audit_when_normal_migration_fails() {
    let temp = tempfile::tempdir().unwrap();
    corrupt_repairable_validation_links(temp.path());
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        delete from schema_migrations where version = 8;
        create trigger fail_schema_8
        before insert on schema_migrations
        when new.version = 8
        begin
            select raise(abort, 'injected schema migration failure');
        end;
        "#,
    )
    .unwrap();
    drop(conn);

    let error = format!("{:#}", repair_validation_links(temp.path()).unwrap_err());
    assert!(error.contains("normal migration failed"), "{error}");
    assert!(error.contains("pre-repair backup:"), "{error}");

    let conn = Connection::open(default_ledger_path(temp.path())).unwrap();
    let project_id: i64 = conn
        .query_row(
            "select project_id from validation_runs where id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(project_id, 2);
    let audit_table_count: i64 = conn
        .query_row(
            "select count(*) from sqlite_schema where type = 'table' and name = 'validation_link_repair_runs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(audit_table_count, 0);
}

#[test]
fn doctor_rolls_back_rows_and_audit_when_final_integrity_validation_fails() {
    let temp = tempfile::tempdir().unwrap();
    corrupt_repairable_validation_links(temp.path());
    let ledger = default_ledger_path(temp.path());
    let conn = Connection::open(&ledger).unwrap();
    conn.pragma_update(None, "foreign_keys", false).unwrap();
    conn.execute_batch(
        r#"
        create table unrelated_integrity_fixture(
            id integer primary key,
            project_id integer not null references projects(id)
        );
        insert into unrelated_integrity_fixture(id, project_id) values (1, 999);
        "#,
    )
    .unwrap();
    drop(conn);

    let error = format!("{:#}", repair_validation_links(temp.path()).unwrap_err());
    assert!(
        error.contains("final integrity validation failed"),
        "{error}"
    );
    assert!(error.contains("pre-repair backup:"), "{error}");

    let conn = Connection::open(&ledger).unwrap();
    let project_id: i64 = conn
        .query_row(
            "select project_id from validation_runs where id = 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(project_id, 2);
    let audit_table_count: i64 = conn
        .query_row(
            "select count(*) from sqlite_schema where type = 'table' and name = 'validation_link_repair_runs'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(audit_table_count, 0);
}

#[test]
fn doctor_retains_compatible_command_usage_and_detaches_conflicting_run_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        insert into work_units(id, project_id, title, status, started_at)
        values (1, 1, 'gate work', 'open', current_timestamp);
        insert into validation_gates(
            id, project_id, gate_key, work_unit_id, expected_result, status, created_at
        ) values (1, 1, 'GATE-SNAPSHOT', 1, 'pass', 'active', current_timestamp);
        insert into repositories(id, project_id, name, path)
        values (1, 1, 'main', '.');
        insert into repository_snapshots(id, repository_id, head_sha, is_clean, created_at)
        values (1, 1, 'command-snapshot', 1, current_timestamp);
        insert into repository_snapshots(id, repository_id, head_sha, is_clean, created_at)
        values (2, 1, 'run-snapshot', 1, current_timestamp);
        insert into command_usages(
            id, project_id, work_unit_id, command, result,
            repository_snapshot_id, created_at
        ) values (1, 1, 1, 'cargo test', 'pass', 1, current_timestamp);
        drop trigger trg_validation_run_project_insert;
        drop trigger trg_validation_run_project_update;
        insert into validation_runs(
            id, project_id, validation_gate_id, work_unit_id,
            command_usage_id, repository_snapshot_id, result, created_at
        ) values (1, 1, 1, 1, 1, 2, 'pass', current_timestamp);
        delete from schema_migrations where version = 8;
        insert into schema_migrations(version, applied_at) values (6, current_timestamp);
        "#,
    )
    .unwrap();
    drop(conn);

    let diagnosis = diagnose_validation_links(temp.path()).unwrap();
    assert!(diagnosis.repairable);
    assert!(
        diagnosis.runs[0]
            .reasons
            .iter()
            .any(|reason| { reason.contains("command usage snapshot") })
    );
    repair_validation_links(temp.path()).unwrap();

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let links = conn
        .query_row(
            "select command_usage_id, repository_snapshot_id from validation_runs where id = 1",
            [],
            |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, Option<i64>>(1)?)),
        )
        .unwrap();
    assert_eq!(links, (Some(1), None));
    drop(conn);

    let second = repair_validation_links(temp.path()).unwrap();
    assert_eq!(second.repaired_validation_run_count, 0);
}

#[test]
fn doctor_refuses_to_move_cross_project_acceptance_authority() {
    let temp = tempfile::tempdir().unwrap();
    corrupt_repairable_validation_links(temp.path());
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update authority_events set project_id = 2 where id = (select approved_by_authority_event_id from acceptance_records where id = 1)",
        [],
    )
    .unwrap();
    drop(conn);

    let diagnosis = diagnose_validation_links(temp.path()).unwrap();
    assert!(!diagnosis.repairable);
    assert!(diagnosis.runs[0].reasons.iter().any(|reason| {
        reason.contains("cross-project approval authority") || reason.contains("unrelated target")
    }));
    assert!(repair_validation_links(temp.path()).is_err());
    assert!(!temp.path().join(".agent-workbench/backups").exists());
}

#[test]
fn doctor_ignores_unknown_dependents_for_healthy_runs_but_refuses_to_repair_them() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        insert into work_units(id, project_id, title, status, started_at)
        values (1, 1, 'gate work', 'open', current_timestamp);
        insert into work_units(id, project_id, title, status, started_at)
        values (2, 1, 'other work', 'open', current_timestamp);
        insert into validation_gates(
            id, project_id, gate_key, work_unit_id, expected_result, status, created_at
        ) values (1, 1, 'GATE-UNKNOWN-DEPENDENT', 1, 'pass', 'active', current_timestamp);
        insert into validation_runs(
            id, project_id, validation_gate_id, work_unit_id, result, created_at
        ) values (1, 1, 1, 1, 'pass', current_timestamp);
        create table extension_validation_links(
            id integer primary key,
            run_id integer not null references validation_runs(id)
        );
        insert into extension_validation_links(id, run_id) values (1, 1);
        "#,
    )
    .unwrap();
    drop(conn);

    assert!(
        diagnose_validation_links(temp.path())
            .unwrap()
            .runs
            .is_empty()
    );

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        drop trigger trg_validation_run_project_update;
        update validation_runs set work_unit_id = 2 where id = 1;
        "#,
    )
    .unwrap();
    drop(conn);

    let diagnosis = diagnose_validation_links(temp.path()).unwrap();
    assert!(!diagnosis.repairable);
    assert!(diagnosis.runs[0].reasons.iter().any(|reason| {
        reason.contains("unknown dependent relation extension_validation_links.run_id")
    }));
    assert!(repair_validation_links(temp.path()).is_err());
}
