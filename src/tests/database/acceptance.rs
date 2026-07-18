use super::*;

#[test]
fn init_migrates_existing_acceptance_records_shape() {
    let temp = tempfile::tempdir().unwrap();
    let ledger_dir = temp.path().join(".agent-workbench");
    fs::create_dir_all(&ledger_dir).unwrap();
    let ledger_path = ledger_dir.join("ledger.sqlite");
    let conn = rusqlite::Connection::open(&ledger_path).unwrap();
    conn.execute_batch(
        r#"
        create table schema_migrations (
            version integer primary key,
            applied_at text not null
        );
        insert into schema_migrations(version, applied_at)
        values (4, current_timestamp);

        create table acceptance_records (
            id integer primary key,
            project_id integer not null,
            target_type text not null check (target_type in ('task', 'design_requirement', 'validation_gate_template')),
            task_id integer,
            design_requirement_id integer,
            validation_gate_template_id integer,
            acceptance_type text not null check (acceptance_type in ('accepted_out_of_scope', 'explicit_exception')),
            reason text not null,
            scope text,
            created_by text not null,
            status text not null default 'approved' check (status in ('approved', 'revoked')),
            approved_by_authority_event_id integer,
            approved_at text,
            created_at text not null,
            review_impact text,
            check (
                (target_type = 'task' and task_id is not null and design_requirement_id is null and validation_gate_template_id is null)
                or (target_type = 'design_requirement' and task_id is null and design_requirement_id is not null and validation_gate_template_id is null)
                or (target_type = 'validation_gate_template' and task_id is null and design_requirement_id is null and validation_gate_template_id is not null)
            )
        );
        "#,
    )
    .unwrap();
    drop(conn);

    apply_test_update(temp.path());
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        r#"
        insert into acceptance_records(
            project_id, target_type, design_package_key, design_file_path,
            acceptance_type, reason, created_by, status, created_at
        )
        values (
            1, 'design_file', 'oversized-file', '01-introduction-goals.md',
            'explicit_exception', 'oversized import guardrail', 'user',
            'approved', current_timestamp
        )
        "#,
        [],
    )
    .unwrap();
    let status = project_status(temp.path()).unwrap();
    let schema_sql: String = conn
        .query_row(
            "select sql from sqlite_schema where type = 'table' and name = 'acceptance_records'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    conn.execute(
        r#"
        insert into acceptance_records(
            project_id, target_type, design_package_key, design_requirement_key,
            acceptance_type, reason, created_by, status, created_at
        )
        values (
            1, 'design_requirement_key', 'oversized-file', 'REQ-001',
            'explicit_exception', 'proposed oversized requirement', 'agent',
            'proposed', current_timestamp
        )
        "#,
        [],
    )
    .unwrap();

    assert_eq!(status.schema_version, Some(SCHEMA_VERSION));
    assert!(schema_sql.contains("created_by in ('user', 'agent', 'system')"));
    assert!(schema_sql.contains("status in ('proposed', 'approved', 'rejected', 'expired')"));
}

#[test]
fn status_migrates_existing_acceptance_records_shape_without_reinit() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        pragma writable_schema = on;
        update sqlite_schema
        set sql = 'CREATE TABLE acceptance_records (
            id integer primary key,
            project_id integer not null,
            target_type text not null check (target_type in (''task'', ''design_requirement'', ''validation_gate_template'')),
            task_id integer,
            design_requirement_id integer,
            validation_gate_template_id integer,
            acceptance_type text not null check (acceptance_type in (''accepted_out_of_scope'', ''explicit_exception'')),
            reason text not null,
            scope text,
            created_by text not null,
            status text not null default ''approved'' check (status in (''approved'', ''revoked'')),
            approved_by_authority_event_id integer,
            approved_at text,
            created_at text not null,
            review_impact text
        )'
        where type = 'table' and name = 'acceptance_records';
        pragma writable_schema = off;
        "#,
    )
    .unwrap();
    let schema_version: i64 = conn
        .pragma_query_value(None, "schema_version", |row| row.get(0))
        .unwrap();
    conn.pragma_update(None, "schema_version", schema_version + 1)
        .unwrap();
    drop(conn);

    let blocked = project_status(temp.path()).unwrap();
    assert_eq!(blocked.project_integrity.result, "blocked");
    apply_test_update(temp.path());
    let status = project_status(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let schema_sql: String = conn
        .query_row(
            "select sql from sqlite_schema where type = 'table' and name = 'acceptance_records'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert!(status.initialized);
    assert_eq!(status.project_integrity.result, "clear");
    assert!(schema_sql.contains("'design_file'"));
    assert!(schema_sql.contains("'coverage_item'"));
    assert!(schema_sql.contains("'validation_run'"));
    assert!(schema_sql.contains("rule_binding_id"));
}

#[test]
fn init_repairs_acceptance_record_references_rewritten_by_legacy_rename() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        pragma writable_schema = on;
        update sqlite_schema
        set sql = replace(sql, 'references acceptance_records(id)', 'references "acceptance_records_old"(id)')
        where type = 'table'
          and name in ('validation_runs', 'repository_state_classifications');
        pragma writable_schema = off;
        "#,
    )
    .unwrap();
    let schema_version: i64 = conn
        .pragma_query_value(None, "schema_version", |row| row.get(0))
        .unwrap();
    conn.pragma_update(None, "schema_version", schema_version + 1)
        .unwrap();
    drop(conn);

    apply_test_update(temp.path());
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let broken_count: i64 = conn
        .query_row(
            "select count(*) from sqlite_schema where sql like '%acceptance_records_old%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    drop(conn);
    let work = start_work(temp.path(), "classify repository state", None).unwrap();
    add_repository(
        temp.path(),
        NewRepository {
            name: "main",
            path: ".",
            current_head: Some("abc123"),
            status_summary: Some("dirty"),
        },
    )
    .unwrap();
    let snapshot = add_repository_snapshot(
        temp.path(),
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(work.activation_id),
            head_sha: Some("abc123"),
            branch: Some("main"),
            status_summary: Some("dirty"),
            is_clean: false,
        },
    )
    .unwrap();
    let classification = add_repository_state_classification(
        temp.path(),
        NewRepositoryStateClassification {
            repository_snapshot_id: snapshot.repository_snapshot_id,
            dirty_entry_id: None,
            classification: "expected",
            reason: "migration repair keeps classification insert usable",
            acceptance_record_id: None,
        },
    )
    .unwrap();

    assert_eq!(broken_count, 0);
    assert!(classification.repository_state_classification_id > 0);
}
