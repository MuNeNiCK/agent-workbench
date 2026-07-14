use super::*;

#[test]
fn status_reports_uninitialized_project() {
    let temp = tempfile::tempdir().unwrap();

    let status = project_status(temp.path()).unwrap();

    assert!(!status.initialized);
    assert!(status.schema_version.is_none());
}

#[test]
fn project_integrity_short_circuits_after_storage_failure() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".agent-workbench")).unwrap();
    fs::write(default_ledger_path(temp.path()), b"not a sqlite database").unwrap();

    let status = project_status(temp.path()).unwrap();

    assert_eq!(status.project_integrity.result, "blocked");
    assert_eq!(status.project_integrity.predicates[0].result, "blocked");
    assert!(
        status.project_integrity.predicates[1..]
            .iter()
            .all(|predicate| predicate.result == "not_evaluated")
    );
    assert!(status.owner_actions.is_empty());
}

#[test]
fn project_integrity_short_circuits_after_unsupported_schema() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute("delete from schema_migrations", []).unwrap();
    conn.execute(
        "insert into schema_migrations(version, applied_at) values (999, current_timestamp)",
        [],
    )
    .unwrap();

    let status = project_status(temp.path()).unwrap();

    assert_eq!(status.project_integrity.predicates[0].result, "clear");
    assert_eq!(status.project_integrity.predicates[1].result, "blocked");
    assert!(
        status.project_integrity.predicates[1]
            .next_action
            .as_deref()
            .is_some_and(|action| action.starts_with("install an agent-workbench version"))
    );
    assert_eq!(
        status.project_integrity.predicates[2].result,
        "not_evaluated"
    );
    assert_eq!(
        status.project_integrity.predicates[3].result,
        "not_evaluated"
    );
}

#[test]
fn project_integrity_gi002_requires_explicit_failed_migration_journal() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        create table migration_apply_journal(
            id integer primary key,
            status text not null check(status in ('incomplete','failed','committed')),
            backup_handle text
        );
        insert into migration_apply_journal(status,backup_handle)
        values('incomplete','backup:pre-migration-abc123');
        "#,
    )
    .unwrap();

    let status = project_status(temp.path()).unwrap();

    assert_eq!(status.project_integrity.predicates[0].result, "clear");
    assert_eq!(status.project_integrity.predicates[1].result, "blocked");
    assert!(
        status.project_integrity.predicates[1]
            .evidence
            .contains("journal proves incomplete application")
    );
    assert_eq!(
        status.project_integrity.predicates[1]
            .next_action
            .as_deref(),
        Some(
            "restore the project-owned pre-migration backup backup:pre-migration-abc123, then run agent-workbench status"
        )
    );
    assert!(
        status.project_integrity.predicates[2..]
            .iter()
            .all(|predicate| predicate.result == "not_evaluated")
    );
}

#[test]
fn project_integrity_gi002_requires_external_restore_without_recorded_backup() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        create table migration_apply_journal(
            id integer primary key,
            status text not null check(status in ('incomplete','failed','committed')),
            backup_handle text
        );
        insert into migration_apply_journal(status,backup_handle) values('failed',null);
        "#,
    )
    .unwrap();

    let status = project_status(temp.path()).unwrap();

    assert_eq!(status.project_integrity.predicates[1].result, "blocked");
    assert_eq!(
        status.project_integrity.predicates[1]
            .next_action
            .as_deref(),
        Some(
            "external-restore-required: no verified pre-migration backup is recorded; then run agent-workbench status"
        )
    );
}

#[test]
fn project_integrity_rejects_activation_before_mutation() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute("delete from schema_migrations", []).unwrap();
    conn.execute(
        "insert into schema_migrations(version, applied_at) values (999, current_timestamp)",
        [],
    )
    .unwrap();

    let error = start_work(temp.path(), "must not be created", None)
        .unwrap_err()
        .to_string();
    let work_count: i64 = conn
        .query_row("select count(*) from work_units", [], |row| row.get(0))
        .unwrap();

    assert!(error.contains("GI-002"));
    assert_eq!(work_count, 0);
}

#[test]
fn project_integrity_common_boundary_rejects_lifecycle_mutation() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "protected owner", None).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute("delete from schema_migrations", []).unwrap();
    conn.execute(
        "insert into schema_migrations(version, applied_at) values (999, current_timestamp)",
        [],
    )
    .unwrap();

    let error = block_work(temp.path(), Some(work.work_unit_id), "must be rejected")
        .unwrap_err()
        .to_string();
    let status: String = conn
        .query_row(
            "select status from work_units where id=?1",
            params![work.work_unit_id],
            |row| row.get(0),
        )
        .unwrap();

    assert!(error.contains("GI-002"));
    assert_eq!(status, "open");
}

#[test]
fn project_integrity_short_circuits_after_project_identity_failure() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute("update projects set root_path='/different-root'", [])
        .unwrap();

    let status = project_status(temp.path()).unwrap();

    assert_eq!(status.project_integrity.predicates[0].result, "clear");
    assert_eq!(status.project_integrity.predicates[1].result, "clear");
    assert_eq!(status.project_integrity.predicates[2].result, "blocked");
    assert_eq!(
        status.project_integrity.predicates[3].result,
        "not_evaluated"
    );
}

#[test]
fn project_integrity_reports_invalid_validation_links_only_at_gi004() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        insert into projects(id,name,root_path,created_at,updated_at)
        values(2,'other','/other',current_timestamp,current_timestamp);
        insert into validation_gates(id,project_id,gate_key,expected_result,status,created_at)
        values(1,1,'opaque-gate','pass','active',current_timestamp);
        drop trigger trg_validation_run_project_insert;
        insert into validation_runs(id,project_id,validation_gate_id,result,created_at)
        values(1,2,1,'pass',current_timestamp);
        "#,
    )
    .unwrap();

    let status = project_status(temp.path()).unwrap();

    assert!(
        status.project_integrity.predicates[..3]
            .iter()
            .all(|predicate| predicate.result == "clear")
    );
    assert_eq!(status.project_integrity.predicates[3].result, "blocked");
    assert_eq!(
        status.project_integrity.predicates[3]
            .next_action
            .as_deref(),
        Some("agent-workbench doctor validation-links")
    );
}
