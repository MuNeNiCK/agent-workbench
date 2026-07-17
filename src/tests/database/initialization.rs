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
        delete from schema_migrations where version in (11, 12);
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
