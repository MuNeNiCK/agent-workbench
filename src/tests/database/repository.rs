use super::*;

#[test]
fn init_refreshes_artifact_and_repository_snapshot_triggers() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        drop trigger trg_artifact_project_insert;
        drop trigger trg_artifact_project_update;
        drop trigger trg_repository_snapshot_referenced_delete;

        create trigger trg_artifact_project_insert
        before insert on artifacts
        for each row
        begin
            select 1;
        end;

        create trigger trg_artifact_project_update
        before update on artifacts
        for each row
        begin
            select 1;
        end;

        create trigger trg_repository_snapshot_referenced_delete
        before delete on repository_snapshots
        for each row
        begin
            select 1;
        end;
        "#,
    )
    .unwrap();
    drop(conn);

    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let artifact_insert_sql: String = conn
        .query_row(
            "select sql from sqlite_schema where type = 'trigger' and name = 'trg_artifact_project_insert'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let snapshot_delete_sql: String = conn
        .query_row(
            "select sql from sqlite_schema where type = 'trigger' and name = 'trg_repository_snapshot_referenced_delete'",
            [],
            |row| row.get(0),
        )
        .unwrap();

    assert!(artifact_insert_sql.contains("new.command_usage_id is not"));
    assert!(artifact_insert_sql.contains("new.repository_snapshot_id is not"));
    assert!(snapshot_delete_sql.contains("validation_runs where repository_snapshot_id"));
    assert!(snapshot_delete_sql.contains("artifacts where repository_snapshot_id"));
}
