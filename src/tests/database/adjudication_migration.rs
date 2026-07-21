use crate::db::{
    find_migration_candidate, insert_migration_candidate, insert_migration_edge_for_members,
    insert_shared_migration_member_for_first_root,
};
use rusqlite::Connection;

fn fixture() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
            "create table roots(id integer primary key, state text not null);\
             create table authority_events(id integer primary key, summary text not null);\
             create table legacy_migration_candidates(\
               id integer primary key, project_id integer not null, candidate_kind text not null,\
               candidate_handle text not null, base_digest text not null, content_digest text not null,\
               created_at text not null, unique(project_id,candidate_handle));\
             create table legacy_migration_candidate_members(\
               id integer primary key, project_id integer not null, candidate_id integer not null,\
               source_table text not null, source_row_id integer not null, member_digest text not null,\
               created_at text not null, unique(project_id,source_table,source_row_id));\
             create table legacy_migration_edges(\
               id integer primary key, project_id integer not null, edge_kind text not null,\
               source_candidate_id integer not null, target_candidate_id integer not null,\
               edge_digest text not null, created_at text not null,\
               unique(project_id,edge_kind,source_candidate_id,target_candidate_id));\
             insert into roots values(1,'open'),(2,'open');\
             insert into authority_events values(1,'shared authority');",
        )
        .unwrap();
    insert_migration_candidate(&conn, 1, "roots", 1, "work_owner", "open").unwrap();
    insert_migration_candidate(&conn, 1, "roots", 2, "work_owner", "open").unwrap();
    conn
}

#[test]
fn shared_authority_is_assigned_once_in_deterministic_first_root() {
    let conn = fixture();
    insert_shared_migration_member_for_first_root(&conn, 1, "roots", 1, "authority_events", 1)
        .unwrap();
    insert_shared_migration_member_for_first_root(&conn, 1, "roots", 2, "authority_events", 1)
        .unwrap();

    let memberships: i64 = conn
            .query_row(
                "select count(*) from legacy_migration_candidate_members where project_id=1 and source_table='authority_events' and source_row_id=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
    let first_candidate = find_migration_candidate(&conn, 1, "roots", 1).unwrap().0;
    let assigned_candidate: i64 = conn
            .query_row(
                "select candidate_id from legacy_migration_candidate_members where project_id=1 and source_table='authority_events' and source_row_id=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
    assert_eq!(memberships, 1);
    assert_eq!(assigned_candidate, first_candidate);
}

#[test]
fn repeated_source_relations_collapse_to_one_normalized_edge() {
    let conn = fixture();
    insert_migration_edge_for_members(&conn, 1, "work_depends_on", "roots", 1, "roots", 2).unwrap();
    insert_migration_edge_for_members(&conn, 1, "work_depends_on", "roots", 1, "roots", 2).unwrap();
    let edges: i64 = conn
        .query_row("select count(*) from legacy_migration_edges", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(edges, 1);
}
