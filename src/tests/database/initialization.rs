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
fn schema12_signed_decision_is_preserved_as_inert_audit_during_schema13_migration() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "preserve accepted review", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "preserved-review",
            review_type: "implementation_review",
            max_fresh_agents: 2,
            max_resume_agents: 0,
            max_parallel_agents: 1,
            required_consecutive_clean_fresh_runs: 1,
            required_consecutive_clean_resume_runs: 0,
            stop_on_severity: "none",
            allow_resume_review: false,
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
    let run = crate::review::add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("work_unit:1"),
            prompt_deviations: None,
            result_summary: Some("accepted under schema 12 signing"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
            review_provenance: "self_recorded",
            review_provenance_ref: None,
        },
    )
    .unwrap();
    let stale_plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            review_type: "implementation_review",
            required: false,
            stage: "close-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: Some(policy.review_policy_id),
            review_scope_id: None,
        },
    )
    .unwrap();
    let stale_run = crate::review::add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: stale_plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("work_unit:1"),
            prompt_deviations: None,
            result_summary: Some("stale signed target"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
            review_provenance: "self_recorded",
            review_provenance_ref: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.pragma_update(None, "foreign_keys", false).unwrap();
    conn.execute_batch(
        "drop trigger if exists trg_review_run_target_update;
         drop trigger if exists trg_review_run_plan_target_update;",
    )
    .unwrap();
    conn.execute(
        "update review_runs set target_ref='work_unit:999' where id=?1",
        params![stale_run.review_run_id],
    )
    .unwrap();
    conn.execute_batch(
        r#"
        drop trigger if exists trg_owner_decision_immutable_update;
        drop trigger if exists trg_owner_decision_immutable_delete;
        create table authority_principals(id integer primary key);
        create table decision_capabilities(id integer primary key);
        insert into authority_principals values(41);
        insert into decision_capabilities values(42);
        pragma legacy_alter_table=on;
        alter table owner_decisions rename to owner_decisions_current;
        pragma legacy_alter_table=off;
        create table owner_decisions (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            decision_handle text not null,
            capability_id integer not null unique references decision_capabilities(id),
            principal_id integer not null references authority_principals(id),
            owner_ref text not null,
            target_ref text not null,
            decision_family text not null,
            action text not null,
            decision_value text not null,
            reason text not null,
            expected_current text not null,
            payload_digest text not null check(length(payload_digest)=64),
            created_at text not null,
            unique(project_id, decision_handle)
        );
        insert into owner_decisions
        select * from owner_decisions_current;
        drop table owner_decisions_current;
        delete from schema_migrations where version=13;
        insert into schema_migrations(version,applied_at) values(12,current_timestamp);
        "#,
    )
    .unwrap();
    conn.execute(
        "insert into owner_decisions values(43,1,?1,42,41,?2,?3,'review','adjudicate','accepted','preserve signed history','pending',?4,current_timestamp)",
        params![
            "decision_eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
            format!("work_unit:{}", work.work_unit_id),
            format!("review_run:{}", run.review_run_id),
            "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
        ],
    )
    .unwrap();
    conn.execute(
        "insert into review_adjudication_decisions(project_id,owner_decision_id,review_run_id,value,predecessor_id,created_at) values(1,43,?1,'accepted',null,current_timestamp)",
        params![run.review_run_id],
    )
    .unwrap();
    conn.execute("insert into decision_capabilities values(45)", [])
        .unwrap();
    conn.execute(
        "insert into owner_decisions values(45,1,?1,45,41,?2,?3,'review','adjudicate','accepted','stale signed history','pending',?4,current_timestamp)",
        params![
            "decision_cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
            format!("work_unit:{}", work.work_unit_id),
            format!("review_run:{}", stale_run.review_run_id),
            "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
        ],
    )
    .unwrap();
    conn.execute(
        "insert into review_adjudication_decisions(project_id,owner_decision_id,review_run_id,value,predecessor_id,created_at) values(1,45,?1,'accepted',null,current_timestamp)",
        params![stale_run.review_run_id],
    )
    .unwrap();
    conn.execute("insert into decision_capabilities values(44)", [])
        .unwrap();
    conn.execute(
        "insert into owner_decisions values(44,1,?1,44,41,?2,?3,'review','adjudicate','accepted','conflicting signed history','pending',?4,current_timestamp)",
        params![
            "decision_dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd",
            format!("work_unit:{}", work.work_unit_id),
            format!("review_run:{}", run.review_run_id),
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        ],
    )
    .unwrap();
    conn.execute(
        "insert into review_adjudication_decisions(project_id,owner_decision_id,review_run_id,value,predecessor_id,created_at) values(1,44,?1,'accepted',null,current_timestamp)",
        params![run.review_run_id],
    )
    .unwrap();
    conn.execute(
        "update review_plans set status='clean' where id=?1",
        params![plan.review_plan_id],
    )
    .unwrap();
    conn.execute(
        "insert into legacy_claim_audits(project_id,review_run_id,candidate_kind,content_digest,reviewer_resolution,mapping_row,before_lifecycle,after_lifecycle,created_at) values(1,?1,'clean',?2,'unbound','completed_clean','completed','audit_only',current_timestamp)",
        params![run.review_run_id, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],
    )
    .unwrap();
    conn.execute(
        "insert into legacy_claim_audits(project_id,review_run_id,candidate_kind,content_digest,reviewer_resolution,mapping_row,before_lifecycle,after_lifecycle,created_at) values(1,?1,'clean',?2,'unbound','completed_clean','completed','audit_only',current_timestamp)",
        params![stale_run.review_run_id, "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"],
    )
    .unwrap();
    conn.execute_batch(
        "drop trigger if exists trg_legacy_signed_review_effect_update;
         drop trigger if exists trg_legacy_signed_review_effect_delete;
         drop table legacy_signed_review_effects;
         drop trigger if exists trg_schema_retirement_update;
         drop trigger if exists trg_schema_retirement_delete;
         drop table schema_retirement_records;",
    )
    .unwrap();
    conn.pragma_update(None, "foreign_keys", true).unwrap();
    drop(conn);

    let contradiction = project_status(temp.path()).unwrap_err();
    assert!(
        contradiction
            .to_string()
            .contains("review_adjudication_decisions current heads"),
        "{contradiction:#}"
    );
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let rollback_state: (i64, i64, i64) = conn
        .query_row(
            "select (select max(version) from schema_migrations),(select count(*) from sqlite_schema where type='table' and name='schema_retirement_records'),(select count(*) from sqlite_schema where type='table' and name='legacy_signed_review_effects')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(rollback_state, (12, 0, 0));
    conn.execute_batch(
        "drop trigger if exists trg_review_adjudication_immutable_delete;
         drop trigger if exists trg_owner_decision_immutable_delete;",
    )
    .unwrap();
    conn.execute(
        "delete from review_adjudication_decisions where owner_decision_id=44",
        [],
    )
    .unwrap();
    conn.execute("delete from owner_decisions where id=44", [])
        .unwrap();
    drop(conn);

    let status = project_status(temp.path()).unwrap();
    assert_eq!(status.schema_version, Some(13), "{status:?}");
    assert_eq!(status.project_integrity.result, "clear");
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let preserved: (i64, i64, String, String) = conn
        .query_row(
            "select capability_id,principal_id,decision_value,reason from owner_decisions where id=43",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        preserved,
        (42, 41, "accepted".into(), "preserve signed history".into())
    );
    let nullable: (i64, i64) = conn
        .query_row(
            "select (select [notnull] from pragma_table_info('owner_decisions') where name='capability_id'),(select [notnull] from pragma_table_info('owner_decisions') where name='principal_id')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(nullable, (0, 0));
    let preserved_effect: String = conn
        .query_row(
            "select content_digest from legacy_signed_review_effects where review_run_id=?1",
            params![run.review_run_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(preserved_effect.len(), 64);
    let stale_effects: i64 = conn
        .query_row(
            "select count(*) from legacy_signed_review_effects where review_run_id=?1",
            params![stale_run.review_run_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(stale_effects, 0);
    let original_audit: String = conn
        .query_row(
            "select reviewer_resolution from legacy_claim_audits where review_run_id=?1",
            params![run.review_run_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(original_audit, "unbound");
    let retirement_journal: i64 = conn
        .query_row(
            "select count(*) from schema_retirement_records where source_generation=12",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(retirement_journal, 1);
    drop(conn);

    let rejected = adjudicate_review(
        temp.path(),
        run.review_run_id,
        AdjudicationInput {
            decision: "rejected",
            reason: "exercise preserved predecessor",
            expected_current:
                "decision_eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee",
        },
    )
    .unwrap();
    adjudicate_review(
        temp.path(),
        run.review_run_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "restore preserved acceptance",
            expected_current: &rejected.decision_handle,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let plan_status: String = conn
        .query_row(
            "select status from review_plans where id=?1",
            params![plan.review_plan_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(plan_status, "clean");
    drop(conn);
    assert_eq!(
        project_status(temp.path()).unwrap().schema_version,
        Some(13)
    );
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let exact_retry_counts: (i64, i64) = conn
        .query_row(
            "select (select count(*) from schema_retirement_records where source_generation=12),(select count(*) from legacy_signed_review_effects where review_run_id=?1)",
            params![run.review_run_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(exact_retry_counts, (1, 1));
    drop(conn);
    if let Err(error) = plan_task_identity(temp.path(), None) {
        assert!(
            !error.to_string().contains("persisted families"),
            "schema-12 inert audit tables must be accepted by the schema-13 profile: {error:#}"
        );
    }
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
