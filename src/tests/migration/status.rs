use super::*;
use crate::task_identity::status::{
    ChecklistItemState, ChecklistState, DependencyState, DerivationState, PhaseState,
    RequirementState, TaskState,
};

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
        "insert into schema_migrations(version,applied_at) values(12,current_timestamp)",
        [],
    )
    .unwrap();
    drop(conn);
    assert!(plan_task_identity(unsupported.path(), None).is_err());
}
