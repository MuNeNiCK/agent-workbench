use super::*;

#[test]
fn v10_to_v11_migration_installs_completion_inheritance_schema() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        drop trigger trg_correction_alias_links_insert;
        drop trigger trg_completion_source_insert;
        drop trigger trg_completion_evidence_insert;
        drop trigger trg_completion_source_immutable_update;
        drop trigger trg_completion_source_immutable_delete;
        drop trigger trg_completion_evidence_immutable_update;
        drop trigger trg_completion_evidence_immutable_delete;
        drop table correction_completion_inheritance_evidence;
        drop table correction_completion_inheritance_sources;
        delete from schema_migrations where version in (11,12);
        insert or ignore into schema_migrations(version, applied_at) values (10, current_timestamp);
        "#,
    )
    .unwrap();
    drop(conn);

    assert_eq!(
        project_status(temp.path()).unwrap().schema_version,
        Some(SCHEMA_VERSION)
    );
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let trigger_sql: String = conn
        .query_row(
            "select sql from sqlite_schema where type='trigger' and name='trg_correction_alias_links_insert'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(trigger_sql.contains("@superseded-task/"));
    assert!(trigger_sql.contains("app.before_state"));
    assert!(trigger_sql.contains("ci.checklist_id!="));
    assert_eq!(
        conn.query_row(
            "select count(*) from sqlite_schema where type='table' and name='correction_application_identity_links'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row(
            "select count(*) from sqlite_schema where type='trigger' and name='trg_correction_identity_link_insert'",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        1
    );
    for table in [
        "correction_completion_inheritance_sources",
        "correction_completion_inheritance_evidence",
    ] {
        assert_eq!(
            conn.query_row(
                "select count(*) from sqlite_schema where type='table' and name=?1",
                params![table],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            1
        );
    }
    assert_eq!(
        conn.query_row(
            "select count(*) from sqlite_schema where type='index' and name in ('idx_completion_evidence_null_canonical','idx_completion_evidence_mapped')",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        2
    );
    for trigger in [
        "trg_completion_source_insert",
        "trg_completion_evidence_insert",
        "trg_completion_source_immutable_update",
        "trg_completion_source_immutable_delete",
        "trg_completion_evidence_immutable_update",
        "trg_completion_evidence_immutable_delete",
    ] {
        assert_eq!(
            conn.query_row(
                "select count(*) from sqlite_schema where type='trigger' and name=?1",
                params![trigger],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            1
        );
    }
    let identity_sql: String = conn
        .query_row(
            "select sql from sqlite_schema where type='table' and name='correction_application_identity_links'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(identity_sql.contains("completion_source"));
}

#[test]
fn current_legacy_inheritance_view_is_rebuilt_atomically_and_once() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let current_view: String = conn
        .query_row(
            "select sql from sqlite_schema where type='view' and name='valid_completion_inheritance_sources'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let legacy_view = current_view.replacen(
        "left join command_usages usage",
        "join command_usages usage",
        1,
    );
    assert_ne!(legacy_view, current_view);
    conn.execute_batch("drop view valid_completion_inheritance_sources")
        .unwrap();
    conn.execute_batch(&legacy_view).unwrap();

    conn.execute_batch("begin").unwrap();
    crate::db::migrate(&conn).unwrap();
    assert_eq!(
        conn.query_row(
            "select sql from sqlite_schema where type='view' and name='valid_completion_inheritance_sources'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        current_view
    );
    conn.execute_batch("rollback").unwrap();
    assert_eq!(
        conn.query_row(
            "select sql from sqlite_schema where type='view' and name='valid_completion_inheritance_sources'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        legacy_view
    );
    drop(conn);

    let conn = crate::db::open_existing_project(temp.path()).unwrap();
    assert_eq!(
        conn.query_row(
            "select sql from sqlite_schema where type='view' and name='valid_completion_inheritance_sources'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        current_view
    );
    let schema_generation: i64 = conn
        .query_row("pragma schema_version", [], |row| row.get(0))
        .unwrap();
    drop(conn);
    let conn = crate::db::open_existing_project(temp.path()).unwrap();
    assert_eq!(
        conn.query_row("pragma schema_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        schema_generation
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
        delete from schema_migrations where version in (11, 12);
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
