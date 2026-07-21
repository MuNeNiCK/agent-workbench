use crate::db::run_atomic_schema_migration;
use rusqlite::Connection;

#[test]
fn rebuilds_a_referenced_table_atomically_and_restores_foreign_keys() {
    let conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", true).unwrap();
    conn.execute_batch(
            "create table parent(id integer primary key, value text not null);\
             create table child(id integer primary key, parent_id integer not null references parent(id));\
             insert into parent values(1,'before');\
             insert into child values(1,1);",
        )
        .unwrap();

    run_atomic_schema_migration(&conn, |tx| {
        tx.execute_batch(
            "pragma legacy_alter_table=on;\
                 alter table parent rename to parent_old;\
                 pragma legacy_alter_table=off;\
                 create table parent(id integer primary key, value text not null, added text);\
                 insert into parent(id,value) select id,value from parent_old;\
                 drop table parent_old;",
        )?;
        Ok(())
    })
    .unwrap();

    let foreign_keys: i64 = conn
        .pragma_query_value(None, "foreign_keys", |row| row.get(0))
        .unwrap();
    let violations: i64 = conn
        .query_row("select count(*) from pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .unwrap();
    let child_parent: i64 = conn
        .query_row("select parent_id from child where id=1", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(foreign_keys, 1);
    assert_eq!(violations, 0);
    assert_eq!(child_parent, 1);
}

#[test]
fn rolls_back_when_a_rebuild_leaves_a_foreign_key_violation() {
    let conn = Connection::open_in_memory().unwrap();
    conn.pragma_update(None, "foreign_keys", true).unwrap();
    conn.execute_batch(
            "create table parent(id integer primary key);\
             create table child(id integer primary key, parent_id integer not null references parent(id));\
             insert into parent values(1);\
             insert into child values(1,1);",
        )
        .unwrap();

    let error = run_atomic_schema_migration(&conn, |tx| {
        tx.execute("delete from parent", [])?;
        Ok(())
    })
    .unwrap_err();

    assert!(error.to_string().contains("foreign key violation"));
    assert_eq!(
        conn.query_row("select count(*) from parent", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        conn.pragma_query_value(None, "foreign_keys", |row| row.get::<_, i64>(0))
            .unwrap(),
        1
    );
}
