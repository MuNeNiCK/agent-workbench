use super::*;
use agent_workbench::default_ledger_path;
use rusqlite::Connection;

fn field<'a>(output: &'a str, key: &str) -> &'a str {
    output
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}: ")))
        .unwrap_or_else(|| panic!("missing {key} in:\n{output}"))
}

#[test]
fn validation_link_artifact_repair_and_retirement_are_exact_owner_operations() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let conn = Connection::open(default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        insert into projects(id,name,root_path,created_at,updated_at)
        values(2,'foreign','/foreign',current_timestamp,current_timestamp);
        insert into work_units(id,project_id,title,status,started_at)
        values(1,1,'expected work','open',current_timestamp);
        insert into work_units(id,project_id,title,status,started_at)
        values(2,2,'foreign work','open',current_timestamp);
        insert into work_units(id,project_id,title,status,started_at)
        values(3,1,'wrong local work','open',current_timestamp);
        insert into validation_gates(id,project_id,gate_key,work_unit_id,expected_result,status,created_at)
        values(1,1,'GATE-RELINK',1,'pass','active',current_timestamp);
        insert into validation_gates(id,project_id,gate_key,work_unit_id,expected_result,status,created_at)
        values(2,1,'GATE-RETIRE',1,'pass','active',current_timestamp);
        drop trigger trg_validation_run_project_insert;
        drop trigger trg_validation_run_project_update;
        insert into validation_runs(id,project_id,validation_gate_id,work_unit_id,result,created_at)
        values(1,2,1,2,'pass',current_timestamp);
        insert into validation_runs(id,project_id,validation_gate_id,work_unit_id,result,created_at)
        values(2,1,2,3,'pass',current_timestamp);
        create table extension_validation_links(
          id integer primary key,
          validation_run_id integer references validation_runs(id)
        );
        insert into extension_validation_links(id,validation_run_id) values(1,2);
        "#,
    )
    .unwrap();
    drop(conn);

    let relink_diagnosis = ok(
        temp.path(),
        &[
            "doctor",
            "validation-links",
            "--artifact",
            "validation-run:1",
        ],
    );
    assert!(relink_diagnosis.contains("run_repairable: true"));
    let relink_current = field(&relink_diagnosis, "expected_current").to_string();
    assert!(relink_diagnosis.contains(&format!(
        "next: agent-workbench doctor validation-links repair validation-run:1 --project 1 --expected-current {relink_current}"
    )));
    let repaired = ok(
        temp.path(),
        &[
            "doctor",
            "validation-links",
            "repair",
            "validation-run:1",
            "--project",
            "1",
            "--expected-current",
            &relink_current,
        ],
    );
    assert!(repaired.contains("operation: relink"));
    assert!(repaired.contains("idempotent: false"));
    assert!(repaired.contains("backup: "));
    let replay = ok(
        temp.path(),
        &[
            "doctor",
            "validation-links",
            "repair",
            "validation-run:1",
            "--project",
            "1",
            "--expected-current",
            &relink_current,
        ],
    );
    assert_eq!(
        field(&repaired, "repair_run_id"),
        field(&replay, "repair_run_id")
    );
    assert!(replay.contains("idempotent: true"));

    let retire_diagnosis = ok(
        temp.path(),
        &[
            "doctor",
            "validation-links",
            "--artifact",
            "validation-run:2",
        ],
    );
    assert!(retire_diagnosis.contains("run_repairable: false"));
    assert!(retire_diagnosis.contains("required_input: reason"));
    assert!(
        retire_diagnosis.contains("next: agent-workbench doctor validation-links retire --help")
    );
    assert!(!retire_diagnosis.contains("<reason>"));
    let retire_current = field(&retire_diagnosis, "expected_current").to_string();
    let retired = ok(
        temp.path(),
        &[
            "doctor",
            "validation-links",
            "retire",
            "validation-run:2",
            "--reason",
            "unknown dependent makes safe relink impossible",
            "--expected-current",
            &retire_current,
        ],
    );
    assert!(retired.contains("operation: retire"));
    assert!(retired.contains("retirement_id: "));
    let runs = ok(temp.path(), &["gate", "run", "list", "--gate", "2"]);
    assert!(runs.contains("GATE-RETIRE:pass:retired"));

    let conn = Connection::open(default_ledger_path(temp.path())).unwrap();
    let repaired_scope: (i64, i64) = conn
        .query_row(
            "select project_id,work_unit_id from validation_runs where id=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(repaired_scope, (1, 1));
    let retired_count: i64 = conn
        .query_row(
            "select count(*) from validation_link_retirements where validation_run_id=2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(retired_count, 1);
}
