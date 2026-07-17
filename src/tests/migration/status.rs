use super::*;
use crate::task_identity::status::{
    ChecklistItemState, ChecklistState, DependencyState, DerivationState, PhaseState,
    RequirementState, TaskState,
};

fn schema11_verification_fixture(closure_attempt_id: Option<i64>) -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
        create table review_runs(id integer primary key, project_id integer, status text,
            clean_run integer, new_findings_count integer);
        create table findings(id integer primary key, project_id integer, review_run_id integer,
            status text);
        create table closures(id integer primary key, finding_id integer);
        create table closure_attempts(id integer primary key, closure_id integer);
        create table finding_verifications(id integer primary key, project_id integer,
            finding_id integer, closure_id integer, closure_attempt_id integer, result text);
        create table acceptance_records(project_id integer, finding_id integer, status text);
        insert into review_runs values(1,1,'completed',0,1);
        insert into findings values(1,1,1,'closed');
        insert into closures values(1,1);
        "#,
    )
    .unwrap();
    conn.execute(
        "insert into finding_verifications values(1,1,1,1,?1,'verified')",
        params![closure_attempt_id],
    )
    .unwrap();
    conn
}

#[test]
fn schema11_pre_attempt_verification_is_grandfathered() {
    let conn = schema11_verification_fixture(None);
    crate::db::validate_schema11_invalid_combinations(&conn, 1).unwrap();
}

#[test]
fn schema11_broken_explicit_attempt_link_still_fails_closed() {
    let conn = schema11_verification_fixture(Some(99));
    assert!(
        crate::db::validate_schema11_invalid_combinations(&conn, 1)
            .unwrap_err()
            .to_string()
            .contains("applied_verification_without_exact_attempt")
    );
}

fn assert_profile<T>(
    parse: impl Fn(&str) -> anyhow::Result<T>,
    accepted: &[&str],
    rejected: &[&str],
) {
    for value in accepted {
        assert!(
            parse(value).is_ok(),
            "supported status was rejected: {value}"
        );
    }
    for value in rejected {
        assert!(
            parse(value).is_err(),
            "unknown status was accepted: {value}"
        );
    }
}

#[test]
fn source_status_profiles_accept_only_deployed_values() {
    assert_profile(
        TaskState::parse,
        &["open", "blocked", "closed", "accepted_out_of_scope"],
        &[
            "active",
            "stale",
            "completed",
            "out_of_scope",
            "rejected",
            "abandoned",
        ],
    );
    assert_profile(
        PhaseState::parse,
        &[
            "open",
            "blocked",
            "closed",
            "accepted_out_of_scope",
            "split",
        ],
        &["active", "out_of_scope"],
    );
    assert_profile(
        DependencyState::parse,
        &["open", "satisfied", "accepted"],
        &["completed", "out_of_scope"],
    );
    assert_profile(
        RequirementState::parse,
        &["active", "superseded", "accepted_out_of_scope"],
        &["open", "out_of_scope"],
    );
    assert_profile(
        DerivationState::parse,
        &["active", "stale", "closed"],
        &["open", "completed"],
    );
    assert_profile(
        ChecklistState::parse,
        &["active", "stale", "closed"],
        &["open", "completed"],
    );
    assert_profile(
        ChecklistItemState::parse,
        &["open", "blocked", "closed", "accepted_out_of_scope"],
        &["active", "out_of_scope", "completed"],
    );
}

#[test]
fn source_profile_rejects_family_drift_and_unsupported_versions() {
    let extra = tempfile::tempdir().unwrap();
    init_project(extra.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(extra.path())).unwrap();
    conn.execute("create table unexpected_family(id integer primary key)", [])
        .unwrap();
    drop(conn);
    assert!(
        plan_task_identity(extra.path(), None)
            .unwrap_err()
            .to_string()
            .contains("persisted families")
    );

    let missing = tempfile::tempdir().unwrap();
    init_project(missing.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(missing.path())).unwrap();
    conn.execute("drop table kpt_item_conversions", []).unwrap();
    drop(conn);
    assert!(
        plan_task_identity(missing.path(), None)
            .unwrap_err()
            .to_string()
            .contains("persisted families")
    );

    let unsupported = tempfile::tempdir().unwrap();
    init_project(unsupported.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(unsupported.path())).unwrap();
    conn.execute(
        "insert into schema_migrations(version,applied_at) values(14,current_timestamp)",
        [],
    )
    .unwrap();
    drop(conn);
    assert!(plan_task_identity(unsupported.path(), None).is_err());
}
