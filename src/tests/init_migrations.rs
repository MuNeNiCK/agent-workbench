use super::*;

#[test]
fn init_creates_ledger_and_project() {
    let temp = tempfile::tempdir().unwrap();

    let outcome = init_project(temp.path()).unwrap();

    assert!(outcome.ledger_path.exists());
    assert!(default_design_root(temp.path()).exists());
    assert!(default_export_root(temp.path()).exists());
    assert!(default_log_root(temp.path()).exists());
    let status = project_status(temp.path()).unwrap();
    assert!(status.initialized);
    assert_eq!(status.schema_version, Some(SCHEMA_VERSION));
    assert_eq!(status.open_work_units, 0);
    assert_eq!(status.active_activations, 0);
}

#[test]
fn next_action_migrates_schema_before_querying_lifecycle_state() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        drop trigger if exists trg_remediation_binding_insert;
        drop trigger if exists trg_remediation_recovery_epoch_insert;
        drop trigger if exists trg_correction_session_links_insert;
        drop trigger if exists trg_correction_session_status_update;
        drop trigger if exists trg_correction_token_links_insert;
        drop table closure_attempts;
        alter table closures drop column status;
        delete from schema_migrations where version = 9;
        insert or ignore into schema_migrations(version, applied_at) values (7, current_timestamp);
        "#,
    )
    .unwrap();
    drop(conn);

    let action = next_action(temp.path()).unwrap();
    assert_eq!(action, NextAction::NoOpenWorkUnit);
    assert_eq!(
        project_status(temp.path()).unwrap().schema_version,
        Some(SCHEMA_VERSION)
    );
}

#[test]
fn repeated_init_preserves_ready_closure_and_single_pending_attempt() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "migration idempotence", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "migration-review",
            review_type: "implementation_review",
            max_fresh_agents: 1,
            max_resume_agents: 1,
            max_parallel_agents: 1,
            required_consecutive_clean_fresh_runs: 0,
            required_consecutive_clean_resume_runs: 0,
            stop_on_severity: "none",
            allow_resume_review: true,
            allow_fresh_review: true,
            allow_new_findings_in_resume: false,
            on_max_agents_exceeded: "block",
            run_count_scope: "review_plan",
            default_run_mode: "fresh",
        },
    )
    .unwrap();
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            review_type: "implementation_review",
            required: true,
            stage: "close-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: Some(policy.review_policy_id),
            review_scope_id: None,
        },
    )
    .unwrap();
    let run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("work_unit:1"),
            prompt_deviations: None,
            result_summary: None,
            new_findings_count: 2,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
            review_provenance: "self_recorded",
            review_provenance_ref: None,
        },
    )
    .unwrap();
    let finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: run.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "migration finding",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), finding.finding_id, "valid").unwrap();
    let closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: finding.finding_id,
            design_invariant: "fixed",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some("src/db.rs"),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("fix migration"),
            tests_or_gates: Some("cargo test"),
            verification_plan: Some("resume"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    let disposed_finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: run.review_run_id,
            finding_type: "implementation_finding",
            severity: "medium",
            description: "legacy disposed finding",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), disposed_finding.finding_id, "valid").unwrap();
    let disposed_closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: disposed_finding.finding_id,
            design_invariant: "disposition is terminal",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some("src/review.rs"),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("accept outside scope"),
            tests_or_gates: Some("cargo test"),
            verification_plan: Some("authority disposition"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    remediate_work(temp.path(), finding.finding_id).unwrap();
    ready_closure(
        temp.path(),
        ClosureReady {
            closure_id: closure.closure_id,
            implementation_evidence: "patched",
            tests_or_gates: "tests pass",
            closed_by_commit: None,
        },
    )
    .unwrap();
    let authority_event_id = approval_authority_event(temp.path());
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        r#"
        insert into acceptance_records(
            project_id, target_type, finding_id, acceptance_type, reason, scope,
            created_by, status, approved_by_authority_event_id, approved_at,
            created_at, review_impact
        ) values (
            1, 'finding', ?1, 'accepted_out_of_scope', 'legacy approved disposition',
            'migration fixture', 'user', 'approved', ?2, current_timestamp,
            current_timestamp, 'legacy migration fixture'
        )
        "#,
        params![disposed_finding.finding_id, authority_event_id],
    )
    .unwrap();
    conn.execute(
        r#"
        insert into closure_attempts(
            project_id, closure_id, attempt_number, implementation_evidence,
            tests_or_gates, review_run_high_watermark, created_at
        ) values (1, ?1, 1, 'legacy scope review', 'legacy scope gate', 0, current_timestamp)
        "#,
        params![disposed_closure.closure_id],
    )
    .unwrap();
    let disposed_attempt_id = conn.last_insert_rowid();
    conn.execute(
        "update findings set status = 'open' where id = ?1",
        params![disposed_finding.finding_id],
    )
    .unwrap();
    conn.execute(
        "update closures set status = 'registered' where id = ?1",
        params![disposed_closure.closure_id],
    )
    .unwrap();
    conn.execute(
        "update closure_attempts set result = null, resolved_at = null where id = ?1",
        params![disposed_attempt_id],
    )
    .unwrap();
    drop(conn);

    init_project(temp.path()).unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let status: String = conn
        .query_row(
            "select status from closures where id = ?1",
            params![closure.closure_id],
            |row| row.get(0),
        )
        .unwrap();
    let pending: i64 = conn
        .query_row(
            "select count(*) from closure_attempts where closure_id = ?1 and result is null",
            params![closure.closure_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(status, "ready_for_verification");
    assert_eq!(pending, 1);
    let disposed_state: (String, String, String) = conn
        .query_row(
            r#"
            select f.status, c.status, a.result
            from findings f
            join closures c on c.finding_id = f.id
            join closure_attempts a on a.closure_id = c.id
            where f.id = ?1 and c.id = ?2 and a.id = ?3
            "#,
            params![
                disposed_finding.finding_id,
                disposed_closure.closure_id,
                disposed_attempt_id
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        disposed_state,
        (
            "accepted_out_of_scope".to_string(),
            "superseded".to_string(),
            "superseded".to_string()
        )
    );
}

#[test]
fn v6_closure_normalization_handles_multiple_incomplete_noneligible_and_unauthorized_history() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "v6 closure matrix", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "v6-matrix",
            review_type: "implementation_review",
            max_fresh_agents: 1,
            max_resume_agents: 1,
            max_parallel_agents: 1,
            required_consecutive_clean_fresh_runs: 0,
            required_consecutive_clean_resume_runs: 0,
            stop_on_severity: "none",
            allow_resume_review: true,
            allow_fresh_review: true,
            allow_new_findings_in_resume: false,
            on_max_agents_exceeded: "block",
            run_count_scope: "review_plan",
            default_run_mode: "fresh",
        },
    )
    .unwrap();
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            review_type: "implementation_review",
            required: true,
            stage: "close-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: Some(policy.review_policy_id),
            review_scope_id: None,
        },
    )
    .unwrap();
    let source = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("work_unit:1"),
            prompt_deviations: None,
            result_summary: Some("legacy findings"),
            new_findings_count: 3,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
            review_provenance: "self_recorded",
            review_provenance_ref: None,
        },
    )
    .unwrap();
    let add_eligible_finding = |description| {
        let finding = add_finding(
            temp.path(),
            NewFinding {
                review_run_id: source.review_run_id,
                finding_type: "implementation_finding",
                severity: "high",
                description,
                design_requirement_id: None,
                task_id: None,
            },
        )
        .unwrap();
        classify_finding(temp.path(), finding.finding_id, "valid").unwrap();
        let closure = add_closure(
            temp.path(),
            NewClosure {
                finding_id: finding.finding_id,
                design_invariant: "legacy invariant",
                design_citations: None,
                implementation_evidence: None,
                affected_surfaces: Some("src/review.rs"),
                same_invariant_search: None,
                other_violations_found: None,
                fix_plan: Some("legacy fix"),
                tests_or_gates: Some("cargo test"),
                verification_plan: Some("legacy resume"),
                closed_by_commit: None,
            },
        )
        .unwrap();
        (finding, closure)
    };
    let (multiple_finding, older_closure) = add_eligible_finding("multiple legacy closures");
    let (out_of_scope_history, history_closure) =
        add_eligible_finding("unauthorized out-of-scope history");
    let (verified_history, verified_older_closure) =
        add_eligible_finding("multiple verified legacy closures");
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into closures(project_id, finding_id, design_invariant, status, created_at) values (1, ?1, 'latest incomplete closure', 'registered', current_timestamp)",
        params![multiple_finding.finding_id],
    )
    .unwrap();
    let incomplete_closure_id = conn.last_insert_rowid();
    conn.execute(
        r#"
        insert into closures(
            project_id, finding_id, design_invariant, affected_surfaces,
            fix_plan, tests_or_gates, verification_plan, status, created_at
        ) values (1, ?1, 'newer verified closure', 'src/review.rs',
                  'newer fix', 'cargo test', 'resume', 'registered', current_timestamp)
        "#,
        params![verified_history.finding_id],
    )
    .unwrap();
    let verified_newer_closure_id = conn.last_insert_rowid();
    for closure_id in [verified_older_closure.closure_id, verified_newer_closure_id] {
        conn.execute(
            r#"
            insert into review_runs(
                project_id, review_plan_id, run_type, run_purpose, target_type,
                work_unit_id, target_ref, new_findings_count, carried_findings_checked,
                clean_run, status, review_provenance, created_at
            ) values (1, ?1, 'resume', 'finding_fix_verification', 'work_unit', ?2,
                      'finding:legacy-verified', 0, 1, 1, 'completed',
                      'self_recorded', current_timestamp)
            "#,
            params![plan.review_plan_id, work.work_unit_id],
        )
        .unwrap();
        let verifier_id = conn.last_insert_rowid();
        conn.execute(
            r#"
            insert into finding_verifications(
                project_id, review_run_id, finding_id, closure_id, result, created_at
            ) values (1, ?1, ?2, ?3, 'verified', current_timestamp)
            "#,
            params![verifier_id, verified_history.finding_id, closure_id],
        )
        .unwrap();
    }
    conn.execute(
        "update findings set status = 'closed' where id = ?1",
        params![verified_history.finding_id],
    )
    .unwrap();
    conn.execute(
        r#"
        insert into review_runs(
            project_id, review_plan_id, run_type, run_purpose, target_type,
            work_unit_id, target_ref, new_findings_count, carried_findings_checked,
            clean_run, status, review_provenance, created_at
        ) values (1, ?1, 'resume', 'finding_fix_verification', 'work_unit', ?2,
                  'finding:legacy-out-of-scope', 0, 1, 0, 'completed',
                  'self_recorded', current_timestamp)
        "#,
        params![plan.review_plan_id, work.work_unit_id],
    )
    .unwrap();
    let legacy_resume_id = conn.last_insert_rowid();
    conn.execute(
        r#"
        insert into finding_verifications(
            project_id, review_run_id, finding_id, closure_id, result, created_at
        ) values (1, ?1, ?2, ?3, 'out_of_scope', current_timestamp)
        "#,
        params![
            legacy_resume_id,
            out_of_scope_history.finding_id,
            history_closure.closure_id
        ],
    )
    .unwrap();
    conn.execute(
        "update findings set status = 'closed' where id = ?1",
        params![out_of_scope_history.finding_id],
    )
    .unwrap();
    drop(conn);

    let design_plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            review_type: "design_review",
            required: true,
            stage: "close-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    let design_run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: design_plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("work_unit:1"),
            prompt_deviations: None,
            result_summary: Some("legacy design finding"),
            new_findings_count: 1,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
            review_provenance: "self_recorded",
            review_provenance_ref: None,
        },
    )
    .unwrap();
    let noneligible_finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: design_run.review_run_id,
            finding_type: "design_finding",
            severity: "high",
            description: "noneligible legacy closure",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), noneligible_finding.finding_id, "valid").unwrap();
    let noneligible_closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: noneligible_finding.finding_id,
            design_invariant: "design finding stays blocking",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some("docs:create:docs/legacy.md"),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("correct the documented design source"),
            tests_or_gates: Some("review the corrected source"),
            verification_plan: Some("resume the design review"),
            closed_by_commit: None,
        },
    )
    .unwrap();

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update closures set affected_surfaces = null, fix_plan = null, tests_or_gates = null, verification_plan = null where id = ?1",
        params![history_closure.closure_id],
    )
    .unwrap();
    conn.execute_batch(
        r#"
        drop trigger if exists trg_remediation_binding_insert;
        drop trigger if exists trg_remediation_recovery_epoch_insert;
        drop trigger if exists trg_correction_session_links_insert;
        drop trigger if exists trg_correction_session_status_update;
        drop trigger if exists trg_correction_token_links_insert;
        drop table closure_attempts;
        alter table closures drop column status;
        delete from schema_migrations where version = 9;
        insert or ignore into schema_migrations(version, applied_at) values (7, current_timestamp);
        "#,
    )
    .unwrap();
    drop(conn);

    init_project(temp.path()).unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let older_status: String = conn
        .query_row(
            "select status from closures where id = ?1",
            params![older_closure.closure_id],
            |row| row.get(0),
        )
        .unwrap();
    let incomplete_status: String = conn
        .query_row(
            "select status from closures where id = ?1",
            params![incomplete_closure_id],
            |row| row.get(0),
        )
        .unwrap();
    let history_state: (String, String) = conn
        .query_row(
            "select f.status, c.status from findings f join closures c on c.finding_id = f.id where f.id = ?1 and c.id = ?2",
            params![out_of_scope_history.finding_id, history_closure.closure_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let noneligible_status: String = conn
        .query_row(
            "select status from closures where id = ?1",
            params![noneligible_closure.closure_id],
            |row| row.get(0),
        )
        .unwrap();
    let verified_states: (String, String, String) = conn
        .query_row(
            r#"
            select f.status, older.status, newer.status
            from findings f
            join closures older on older.id = ?2
            join closures newer on newer.id = ?3
            where f.id = ?1
            "#,
            params![
                verified_history.finding_id,
                verified_older_closure.closure_id,
                verified_newer_closure_id
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(older_status, "superseded");
    assert_eq!(incomplete_status, "incomplete");
    assert_eq!(
        history_state,
        ("open".to_string(), "incomplete".to_string())
    );
    assert_eq!(noneligible_status, "registered");
    assert_eq!(
        verified_states,
        (
            "closed".to_string(),
            "superseded".to_string(),
            "verified".to_string()
        )
    );
}

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

    init_project(temp.path()).unwrap();
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

    project_status(temp.path()).unwrap();
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

#[test]
fn init_migrates_existing_kpt_item_status_constraint() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        pragma foreign_keys = off;

        drop table kpt_item_conversions;
        alter table kpt_items rename to kpt_items_current;

        create table kpt_items (
            id integer primary key,
            kpt_review_id integer not null references kpt_reviews(id) on delete cascade,
            item_type text not null check (item_type in ('keep', 'problem', 'try')),
            title text not null,
            details text,
            severity text not null default 'medium' check (severity in ('critical', 'high', 'medium', 'low')),
            linked_user_correction_id integer references user_corrections(id),
            linked_command_profile_id integer references command_profiles(id),
            linked_review_finding_id integer,
            linked_task_id integer references tasks(id),
            proposed_action text,
            status text not null default 'open' check (status in ('open', 'accepted', 'converted_to_task', 'dismissed')),
            created_at text not null
        );

        insert into kpt_items(
            id, kpt_review_id, item_type, title, details, severity,
            linked_user_correction_id, linked_command_profile_id,
            linked_review_finding_id, linked_task_id, proposed_action, status, created_at
        )
        select
            id, kpt_review_id, item_type, title, details, severity,
            linked_user_correction_id, linked_command_profile_id,
            linked_review_finding_id, linked_task_id, proposed_action, status, created_at
        from kpt_items_current;

        drop table kpt_items_current;

        create table kpt_item_conversions (
            id integer primary key,
            kpt_item_id integer not null references kpt_items(id) on delete cascade,
            target_type text not null check (target_type in ('task', 'command_profile', 'review_policy', 'design_version', 'decision', 'user_correction')),
            task_id integer references tasks(id),
            command_profile_id integer references command_profiles(id),
            review_policy_id integer,
            design_version_id integer,
            decision_id integer references decisions(id),
            user_correction_id integer references user_corrections(id),
            created_at text not null
        );

        pragma foreign_keys = on;
        "#,
    )
    .unwrap();
    drop(conn);

    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        r#"
        insert into kpt_reviews(project_id, trigger, summary, status, created_at)
        values (1, 'manual', 'migration check', 'open', current_timestamp)
        "#,
        [],
    )
    .unwrap();
    conn.execute(
        r#"
        insert into kpt_items(
            kpt_review_id, item_type, title, severity, status, created_at
        )
        values (1, 'try', 'converted status is generic', 'medium', 'converted', current_timestamp)
        "#,
        [],
    )
    .unwrap();
}

#[test]
fn init_migrates_existing_review_run_type_purpose_constraint() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        pragma foreign_keys = off;

        drop table review_agent_invocations;
        drop table finding_verifications;
        drop table findings;
        drop table review_runs;

        create table review_runs (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            review_scope_id integer references review_scopes(id),
            review_plan_id integer references review_plans(id),
            run_type text not null check (run_type in ('fresh', 'resume', 'coverage')),
            run_purpose text not null check (run_purpose in ('new_unbiased_review', 'finding_fix_verification', 'coverage_audit')),
            target_type text not null check (target_type in ('design_version', 'design_requirement', 'task', 'work_unit', 'repository_snapshot', 'file', 'symbol')),
            design_version_id integer references design_versions(id),
            design_requirement_id integer references design_requirements(id),
            task_id integer references tasks(id),
            work_unit_id integer references work_units(id),
            repository_snapshot_id integer,
            target_ref text,
            prompt_deviations text,
            result_summary text,
            new_findings_count integer not null default 0,
            carried_findings_checked integer not null default 0,
            clean_run integer not null default 0 check (clean_run in (0, 1)),
            status text not null default 'requested' check (status in ('requested', 'running', 'completed', 'failed', 'cancelled')),
            created_at text not null
        );

        create table review_agent_invocations (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            review_plan_id integer references review_plans(id),
            review_run_id integer references review_runs(id),
            run_type text not null check (run_type in ('fresh', 'resume', 'coverage')),
            agent_label text,
            external_agent_id text,
            status text not null default 'requested' check (status in ('requested', 'running', 'completed', 'failed', 'cancelled')),
            started_at text,
            finished_at text
        );

        create table findings (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            review_run_id integer not null references review_runs(id) on delete cascade,
            finding_type text not null check (finding_type in ('design_finding', 'design_implementation_drift', 'design_task_gap', 'implementation_finding', 'coverage_finding')),
            severity text not null check (severity in ('critical', 'high', 'medium', 'low')),
            description text not null,
            classification text not null default 'unclassified' check (classification in ('unclassified', 'valid', 'invalid', 'design_conflict', 'needs_evidence')),
            status text not null default 'open' check (status in ('open', 'closed', 'accepted_out_of_scope')),
            design_requirement_id integer references design_requirements(id),
            task_id integer references tasks(id),
            created_at text not null
        );

        create table finding_verifications (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            review_run_id integer not null references review_runs(id) on delete cascade,
            finding_id integer not null references findings(id) on delete cascade,
            closure_id integer not null references closures(id) on delete cascade,
            result text not null check (result in ('verified', 'not_fixed', 'needs_evidence', 'out_of_scope')),
            notes text,
            created_at text not null,
            unique(review_run_id, finding_id, closure_id)
        );

        pragma foreign_keys = on;
        "#,
    )
    .unwrap();
    drop(conn);

    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let stale_trigger_refs: i64 = conn
        .query_row(
            r#"
            select count(*)
            from sqlite_schema
            where type = 'trigger'
              and coalesce(sql, '') like '%review_runs_old%'
            "#,
            [],
            |row| row.get(0),
        )
        .unwrap();
    conn.execute(
        "insert into work_units(project_id, title, status, started_at) values (1, 'migration target', 'open', current_timestamp)",
        [],
    )
    .unwrap();
    let work_unit_id = conn.last_insert_rowid();
    conn.execute(
        r#"
        insert into review_policies(
            project_id, name, review_type, max_fresh_agents, max_resume_agents,
            max_parallel_agents, required_consecutive_clean_fresh_runs,
            required_consecutive_clean_resume_runs, stop_on_severity,
            allow_resume_review, allow_fresh_review, allow_new_findings_in_resume,
            on_max_agents_exceeded, run_count_scope, default_run_mode, created_at
        )
        values (1, 'migration-policy', 'implementation_review', 2, 1, 1, 1, 0, 'none', 1, 1, 0, 'block', 'review_plan', 'fresh', current_timestamp)
        "#,
        [],
    )
    .unwrap();
    let review_policy_id = conn.last_insert_rowid();
    conn.execute(
        r#"
        insert into review_plans(
            project_id, work_unit_id, review_type, required, stage,
            review_policy_id, status, created_at
        )
        values (1, ?1, 'implementation_review', 1, 'close-ready', ?2, 'open', current_timestamp)
        "#,
        params![work_unit_id, review_policy_id],
    )
    .unwrap();
    let review_plan_id = conn.last_insert_rowid();
    conn.execute(
        "insert into review_plan_targets(review_plan_id, target_type, work_unit_id) values (?1, 'work_unit', ?2)",
        params![review_plan_id, work_unit_id],
    )
    .unwrap();
    let invalid = conn.execute(
        r#"
        insert into review_runs(
            project_id, review_plan_id, run_type, run_purpose, target_type, work_unit_id, target_ref,
            new_findings_count, carried_findings_checked, clean_run, status, created_at
        )
        values (1, ?1, 'fresh', 'finding_fix_verification', 'work_unit', ?2, ?3, 0, 0, 0, 'completed', current_timestamp)
        "#,
        params![
            review_plan_id,
            work_unit_id,
            format!("work_unit:{work_unit_id}"),
        ],
    );
    conn.execute(
        r#"
        insert into review_runs(
            project_id, review_plan_id, run_type, run_purpose, target_type, work_unit_id, target_ref,
            new_findings_count, carried_findings_checked, clean_run, status, created_at
        )
        values (1, ?1, 'fresh', 'new_unbiased_review', 'work_unit', ?2, ?3, 0, 0, 0, 'completed', current_timestamp)
        "#,
        params![
            review_plan_id,
            work_unit_id,
            format!("work_unit:{work_unit_id}"),
        ],
    )
    .unwrap();
    let invalid_update = conn.execute(
        "update review_runs set run_purpose = 'finding_fix_verification' where id = 1",
        [],
    );

    assert_eq!(stale_trigger_refs, 0);
    assert!(invalid.is_err());
    assert!(invalid_update.is_err());
}

#[test]
fn init_rejects_legacy_review_rows_missing_required_links() {
    let temp = tempfile::tempdir().unwrap();
    fs::create_dir_all(temp.path().join(".agent-workbench")).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        pragma foreign_keys = off;

        create table projects (
            id integer primary key,
            name text not null,
            root_path text not null unique,
            created_at text not null,
            updated_at text not null
        );

        create table work_units (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            parent_work_unit_id integer references work_units(id),
            title text not null,
            status text not null default 'open' check (status in ('open', 'blocked', 'closed', 'abandoned')),
            responsibility text,
            in_scope text,
            out_of_scope text,
            interrupt_reason text,
            selected_gate_id integer,
            review_plan_status text,
            started_at text not null,
            closed_at text,
            close_summary text
        );

        create table review_policies (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            name text not null,
            review_type text not null check (review_type in ('design_review', 'design_implementation_diff', 'design_task_decomposition', 'implementation_review', 'general')),
            max_fresh_agents integer not null default 1,
            max_resume_agents integer not null default 1,
            max_parallel_agents integer not null default 1,
            required_consecutive_clean_fresh_runs integer not null default 1,
            required_consecutive_clean_resume_runs integer not null default 1,
            stop_on_severity text not null default 'none' check (stop_on_severity in ('critical', 'high', 'medium', 'low', 'none')),
            allow_resume_review integer not null default 1 check (allow_resume_review in (0, 1)),
            allow_fresh_review integer not null default 1 check (allow_fresh_review in (0, 1)),
            allow_new_findings_in_resume integer not null default 0 check (allow_new_findings_in_resume in (0, 1)),
            on_max_agents_exceeded text not null default 'block' check (on_max_agents_exceeded in ('block', 'accept_with_user_approval', 'mark_exhausted')),
            run_count_scope text not null default 'review_plan' check (run_count_scope in ('review_plan', 'review_scope', 'work_unit')),
            default_run_mode text not null default 'fresh' check (default_run_mode in ('fresh', 'resume')),
            created_at text not null,
            unique(project_id, name)
        );

        create table review_plans (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            work_unit_id integer not null references work_units(id) on delete cascade,
            design_version_id integer references design_versions(id) on delete cascade,
            review_type text not null check (review_type in ('design_review', 'design_implementation_diff', 'design_task_decomposition', 'implementation_review', 'general')),
            required integer not null default 1 check (required in (0, 1)),
            stage text not null check (stage in ('design-ready', 'implementation-ready', 'close-ready', 'resume-ready')),
            scope text,
            clean_condition text,
            stop_condition text,
            review_policy_id integer references review_policies(id),
            review_scope_id integer references review_scopes(id),
            status text not null default 'open' check (status in ('open', 'blocked', 'clean', 'accepted_exception', 'not_required', 'exhausted', 'needs_user_decision')),
            created_at text not null
        );

        create table review_runs (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            review_scope_id integer references review_scopes(id),
            review_plan_id integer references review_plans(id),
            run_type text not null check (run_type in ('fresh', 'resume', 'coverage')),
            run_purpose text not null check (run_purpose in ('new_unbiased_review', 'finding_fix_verification', 'coverage_audit')),
            target_type text not null check (target_type in ('design_version', 'design_requirement', 'task', 'work_unit', 'repository_snapshot', 'file', 'symbol')),
            design_version_id integer references design_versions(id),
            design_requirement_id integer references design_requirements(id),
            task_id integer references tasks(id),
            work_unit_id integer references work_units(id),
            repository_snapshot_id integer,
            target_ref text,
            prompt_deviations text,
            result_summary text,
            new_findings_count integer not null default 0,
            carried_findings_checked integer not null default 0,
            clean_run integer not null default 0 check (clean_run in (0, 1)),
            status text not null default 'requested' check (status in ('requested', 'running', 'completed', 'failed', 'cancelled')),
            created_at text not null
        );

        insert into projects(name, root_path, created_at, updated_at)
        values ('legacy', '/tmp/legacy-awb', current_timestamp, current_timestamp);

        insert into work_units(project_id, title, status, started_at)
        values (1, 'legacy work', 'open', current_timestamp);

        insert into review_plans(
            project_id, work_unit_id, review_type, required, stage,
            review_policy_id, status, created_at
        )
        values (1, 1, 'implementation_review', 1, 'close-ready', null, 'open', current_timestamp);

        insert into review_runs(
            project_id, review_plan_id, run_type, run_purpose,
            target_type, work_unit_id, target_ref,
            new_findings_count, carried_findings_checked, clean_run, status, created_at
        )
        values (1, null, 'resume', 'finding_fix_verification', 'work_unit', 1, 'work_unit:1', 1, 0, 0, 'completed', current_timestamp);

        pragma foreign_keys = on;
        "#,
    )
    .unwrap();
    drop(conn);

    let result = init_project(temp.path());

    assert!(result.is_err());
}

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
