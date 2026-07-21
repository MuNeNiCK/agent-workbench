use super::*;

#[test]
fn terminal_design_recovery_publishes_one_atomic_successor_and_replays_exactly() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "terminal design recovery", None).unwrap();
    let package = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "terminal-recovery",
            title: "Terminal Recovery",
        },
    )
    .unwrap();
    let source_design = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &package.package_path,
            status: "draft",
        },
    )
    .unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "terminal-recovery-review",
            review_type: "design_review",
            max_fresh_agents: 2,
            max_resume_agents: 2,
            max_parallel_agents: 1,
            required_consecutive_clean_fresh_runs: 1,
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
            design_version_id: Some(source_design.design_version_id),
            review_type: "design_review",
            required: true,
            stage: "design-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: Some(policy.review_policy_id),
            review_scope_id: None,
        },
    )
    .unwrap();
    let source_run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some(&format!(
                "review-context:design-review:design={}:work={}",
                source_design.design_version_id, work.work_unit_id
            )),
            prompt_deviations: None,
            result_summary: Some("design correction required"),
            new_findings_count: 1,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: Some("source-reviewer"),
            external_agent_id: Some("source-reviewer"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("source-review-output"),
        },
    )
    .unwrap();
    let finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: source_run.review_run_id,
            finding_type: "design_finding",
            severity: "critical",
            description: "corrected package must publish a successor",
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
            design_invariant: "corrected design has an immutable successor",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some("design:edit:01-introduction-goals.md"),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("correct the design package"),
            tests_or_gates: Some("design package validation"),
            verification_plan: Some("fresh exact-context verification"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    let session = begin_correction(temp.path(), closure.closure_id).unwrap();
    let edited = package.package_path.join("01-introduction-goals.md");
    let mut text = std::fs::read_to_string(&edited).unwrap();
    text.push_str("\nCorrected terminal recovery boundary.\n");
    std::fs::write(&edited, text).unwrap();
    let attempt = ready_closure(
        temp.path(),
        ClosureReady {
            closure_id: closure.closure_id,
            implementation_evidence: "corrected design source",
            tests_or_gates: "design package validation passed",
            closed_by_commit: None,
        },
    )
    .unwrap();
    let verification_run = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some(&attempt.context_ref),
            prompt_deviations: None,
            result_summary: Some("corrected design verified"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: Some("verification-reviewer"),
            external_agent_id: Some("verification-reviewer"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("verification-output"),
        },
        Some("verified"),
    )
    .unwrap();
    add_finding_verification(
        temp.path(),
        NewFindingVerification {
            review_run_id: verification_run.review_run_id,
            finding_id: finding.finding_id,
            closure_id: closure.closure_id,
            result: "verified",
            notes: None,
        },
    )
    .unwrap();
    let terminal = adjudicate_verification(
        temp.path(),
        verification_run.review_run_id,
        finding.finding_id,
        closure.closure_id,
        attempt.attempt_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "accept terminal correction verification",
            expected_current: "pending",
        },
    )
    .unwrap();
    let authority = add_authority_event(
        temp.path(),
        NewAuthorityEvent {
            event_type: "user_instruction",
            source: Some("test-owner"),
            summary: "recover terminal design publication",
            scope: Some("project"),
            precedence: 100,
        },
    )
    .unwrap();
    let counts = || {
        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        conn.query_row(
            "select (select count(*) from design_versions),(select count(*) from closures),(select count(*) from correction_sessions),(select count(*) from correction_tokens),(select count(*) from closure_attempts),(select count(*) from finding_decision_epochs),(select count(*) from finding_design_recoveries)",
            [],
            |row| Ok((row.get::<_, i64>(0)?,row.get::<_, i64>(1)?,row.get::<_, i64>(2)?,row.get::<_, i64>(3)?,row.get::<_, i64>(4)?,row.get::<_, i64>(5)?,row.get::<_, i64>(6)?)),
        )
        .unwrap()
    };
    let terminal_counts = counts();
    let stale_package = recover_finding_design(
        temp.path(),
        FindingDesignRecovery {
            finding_id: finding.finding_id,
            terminal_epoch: 1,
            evidence: "corrected package requires successor publication",
            authority_event_id: authority.authority_event_id,
            reason: "publish the corrected successor atomically",
            package_current: &"0".repeat(64),
            expected_current: &terminal.decision_handle,
            idempotency_key: "terminal-recovery-stale-package",
        },
    )
    .unwrap_err();
    assert!(stale_package.to_string().contains("package_current_stale"));
    assert_eq!(counts(), terminal_counts);

    let manifest_path = package.package_path.join("design.yaml");
    let manifest = std::fs::read_to_string(&manifest_path).unwrap();
    std::fs::write(&manifest_path, "not: [valid").unwrap();
    let invalid_package = recover_finding_design(
        temp.path(),
        FindingDesignRecovery {
            finding_id: finding.finding_id,
            terminal_epoch: 1,
            evidence: "corrected package requires successor publication",
            authority_event_id: authority.authority_event_id,
            reason: "publish the corrected successor atomically",
            package_current: &source_design.content_hash,
            expected_current: &terminal.decision_handle,
            idempotency_key: "terminal-recovery-invalid-package",
        },
    )
    .unwrap_err();
    let invalid_message = invalid_package.to_string();
    assert!(
        invalid_message.contains("corrected Design Package is invalid"),
        "unexpected invalid package diagnostic: {invalid_message}"
    );
    assert!(invalid_message.contains("next: agent-workbench design inspect"));
    std::fs::write(&manifest_path, manifest).unwrap();
    assert_eq!(counts(), terminal_counts);

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        "create trigger fail_recovery_receipt before insert on finding_design_recoveries begin select raise(abort,'injected recovery durability failure'); end;",
    )
    .unwrap();
    drop(conn);
    let failed_publish = recover_finding_design(
        temp.path(),
        FindingDesignRecovery {
            finding_id: finding.finding_id,
            terminal_epoch: 1,
            evidence: "corrected package requires successor publication",
            authority_event_id: authority.authority_event_id,
            reason: "publish the corrected successor atomically",
            package_current: &source_design.content_hash,
            expected_current: &terminal.decision_handle,
            idempotency_key: "terminal-recovery-1",
        },
    )
    .unwrap_err();
    assert!(
        failed_publish
            .to_string()
            .contains("injected recovery durability failure")
    );
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch("drop trigger fail_recovery_receipt;")
        .unwrap();
    drop(conn);
    assert_eq!(counts(), terminal_counts);

    let request = FindingDesignRecovery {
        finding_id: finding.finding_id,
        terminal_epoch: 1,
        evidence: "corrected package requires successor publication",
        authority_event_id: authority.authority_event_id,
        reason: "publish the corrected successor atomically",
        package_current: &source_design.content_hash,
        expected_current: &terminal.decision_handle,
        idempotency_key: "terminal-recovery-1",
    };
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let mut racers = Vec::new();
    for _ in 0..2 {
        let root = temp.path().to_path_buf();
        let barrier = barrier.clone();
        let package_current = source_design.content_hash.clone();
        let expected_current = terminal.decision_handle.clone();
        let finding_id = finding.finding_id;
        let authority_event_id = authority.authority_event_id;
        racers.push(std::thread::spawn(move || {
            barrier.wait();
            recover_finding_design(
                &root,
                FindingDesignRecovery {
                    finding_id,
                    terminal_epoch: 1,
                    evidence: "corrected package requires successor publication",
                    authority_event_id,
                    reason: "publish the corrected successor atomically",
                    package_current: &package_current,
                    expected_current: &expected_current,
                    idempotency_key: "terminal-recovery-1",
                },
            )
            .unwrap()
        }));
    }
    let first = racers.remove(0).join().unwrap();
    let second = racers.remove(0).join().unwrap();
    assert_ne!(first.idempotent, second.idempotent);
    assert_eq!(first.recovery_handle, second.recovery_handle);
    assert_eq!(
        first.corrected_design_version_id,
        second.corrected_design_version_id
    );
    let recovered = if first.idempotent { second } else { first };
    assert!(!recovered.idempotent);
    assert!(!recovered.converged);
    assert_ne!(
        recovered.corrected_design_version_id,
        source_design.design_version_id
    );
    assert!(recovered.corrected_design_ref.starts_with("revision_"));
    assert_eq!(
        inspect_design_version(temp.path(), recovered.corrected_design_version_id)
            .unwrap()
            .design_version_id,
        recovered.corrected_design_version_id
    );
    assert_eq!(
        inspect_design_version_ref(temp.path(), &recovered.corrected_design_ref)
            .unwrap()
            .design_version_id,
        recovered.corrected_design_version_id
    );
    let missing_successor_plan = render_finding_fix_context(
        temp.path(),
        finding.finding_id,
        recovered.successor_closure_id,
        recovered.successor_attempt_id,
    )
    .err()
    .expect("recovered context must require an exact successor plan");
    let missing_successor_plan = missing_successor_plan.to_string();
    assert!(missing_successor_plan.contains("exact successor review plan"));
    assert!(missing_successor_plan.contains(&format!(
        "agent-workbench review plan add --work-unit {} --type design_review --stage design-ready --design-version {} --policy {}",
        work.work_unit_id, recovered.corrected_design_version_id, policy.review_policy_id
    )));

    let competing = recover_finding_design(
        temp.path(),
        FindingDesignRecovery {
            finding_id: finding.finding_id,
            terminal_epoch: 1,
            evidence: "a concurrent owner supplied different evidence",
            authority_event_id: authority.authority_event_id,
            reason: "converge on the already-published terminal recovery",
            package_current: &source_design.content_hash,
            expected_current: &terminal.decision_handle,
            idempotency_key: "terminal-recovery-competing-key",
        },
    )
    .unwrap();
    assert!(!competing.idempotent);
    assert!(competing.converged);
    assert_eq!(competing.recovery_handle, recovered.recovery_handle);
    assert_eq!(
        competing.corrected_design_version_id,
        recovered.corrected_design_version_id
    );
    assert_eq!(competing.next_action, recovered.next_action);

    let recovered_counts = counts();
    let corrected_bytes = std::fs::read(&edited).unwrap();
    let mut drifted_bytes = corrected_bytes.clone();
    drifted_bytes.extend_from_slice(b"\npost-recovery drift\n");
    std::fs::write(&edited, drifted_bytes).unwrap();
    let drifted_replay = recover_finding_design(temp.path(), request.clone()).unwrap_err();
    assert!(
        drifted_replay
            .to_string()
            .contains("recovery_postconditions_changed")
    );
    assert_eq!(counts(), recovered_counts);
    std::fs::write(&edited, corrected_bytes).unwrap();

    let stale_verifier = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some(&recovered.context_ref),
            prompt_deviations: None,
            result_summary: Some("stale design version must not verify the successor"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: Some("stale-verifier"),
            external_agent_id: Some("stale-verifier"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("stale-verification-output"),
        },
        Some("verified"),
    )
    .unwrap_err();
    assert!(
        stale_verifier
            .to_string()
            .contains("finding-fix resume run target is not the current ready attempt")
    );

    let successor_plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(recovered.corrected_design_version_id),
            review_type: "design_review",
            required: true,
            stage: "design-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: Some(policy.review_policy_id),
            review_scope_id: None,
        },
    )
    .unwrap();
    let successor_context = render_finding_fix_context(
        temp.path(),
        finding.finding_id,
        recovered.successor_closure_id,
        recovered.successor_attempt_id,
    )
    .unwrap();
    assert!(successor_context.text.contains(&format!(
        "review_plan_id: {}",
        successor_plan.review_plan_id
    )));
    let successor_verifier = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: successor_plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some(&recovered.context_ref),
            prompt_deviations: None,
            result_summary: Some("exact successor design verified"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: Some("successor-verifier"),
            external_agent_id: Some("successor-verifier"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("successor-verification-output"),
        },
        Some("verified"),
    )
    .unwrap();
    add_finding_verification(
        temp.path(),
        NewFindingVerification {
            review_run_id: successor_verifier.review_run_id,
            finding_id: finding.finding_id,
            closure_id: recovered.successor_closure_id,
            result: "verified",
            notes: None,
        },
    )
    .unwrap();

    let replayed = recover_finding_design(temp.path(), request.clone()).unwrap();
    assert!(replayed.idempotent);
    assert_eq!(
        replayed.next_action,
        "agent-workbench verification adjudicate --help"
    );
    assert_eq!(replayed.recovery_handle, recovered.recovery_handle);
    assert_eq!(
        replayed.corrected_design_version_id,
        recovered.corrected_design_version_id
    );
    assert_eq!(
        replayed.successor_attempt_id,
        recovered.successor_attempt_id
    );
    let rejected_claim = adjudicate_verification(
        temp.path(),
        successor_verifier.review_run_id,
        finding.finding_id,
        recovered.successor_closure_id,
        recovered.successor_attempt_id,
        AdjudicationInput {
            decision: "rejected",
            reason: "request another owner review before adoption",
            expected_current: "pending",
        },
    )
    .unwrap();
    let adjudicated_replay = recover_finding_design(temp.path(), request.clone()).unwrap();
    assert_eq!(adjudicated_replay.next_action, "agent-workbench next");
    adjudicate_verification(
        temp.path(),
        successor_verifier.review_run_id,
        finding.finding_id,
        recovered.successor_closure_id,
        recovered.successor_attempt_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "accept exact successor verification",
            expected_current: &rejected_claim.decision_handle,
        },
    )
    .unwrap();
    let terminal_replay = recover_finding_design(temp.path(), request).unwrap();
    assert!(terminal_replay.idempotent);
    assert_eq!(
        terminal_replay.next_action,
        format!(
            "agent-workbench design inspect {}",
            recovered.corrected_design_ref
        )
    );
    let terminal_convergence = recover_finding_design(
        temp.path(),
        FindingDesignRecovery {
            finding_id: finding.finding_id,
            terminal_epoch: 1,
            evidence: "late competing recovery",
            authority_event_id: authority.authority_event_id,
            reason: "converge after successor verification",
            package_current: &source_design.content_hash,
            expected_current: &terminal.decision_handle,
            idempotency_key: "terminal-recovery-late-competitor",
        },
    )
    .unwrap();
    assert!(terminal_convergence.converged);
    assert_eq!(
        terminal_convergence.next_action,
        terminal_replay.next_action
    );
    let changed = recover_finding_design(
        temp.path(),
        FindingDesignRecovery {
            finding_id: finding.finding_id,
            terminal_epoch: 1,
            evidence: "changed replay payload",
            authority_event_id: authority.authority_event_id,
            reason: "publish the corrected successor atomically",
            package_current: &source_design.content_hash,
            expected_current: &terminal.decision_handle,
            idempotency_key: "terminal-recovery-1",
        },
    )
    .unwrap_err();
    assert!(
        changed
            .to_string()
            .contains("idempotency_key_payload_mismatch")
    );

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let predecessor: (String, String, String, i64) = conn
        .query_row(
            "select c.status,s.status,a.result,(select count(*) from finding_verifications where closure_id=c.id) from closures c join correction_sessions s on s.closure_id=c.id join closure_attempts a on a.id=?2 where c.id=?1",
            params![closure.closure_id, attempt.attempt_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        predecessor,
        ("verified".into(), "completed".into(), "verified".into(), 1)
    );
    let successor: (String, String, Option<String>, i64, i64) = conn
        .query_row(
            "select c.status,s.status,a.result,(select count(*) from correction_tokens where closure_id=c.id and status='applied'),(select count(*) from finding_design_recoveries where successor_closure_id=c.id) from closures c join correction_sessions s on s.closure_id=c.id join closure_attempts a on a.id=?2 where c.id=?1",
            params![recovered.successor_closure_id, recovered.successor_attempt_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .unwrap();
    assert_eq!(
        successor,
        (
            "verified".into(),
            "completed".into(),
            Some("verified".into()),
            1,
            1
        )
    );
    let successor_verifications: i64 = conn
        .query_row(
            "select count(*) from finding_verifications where closure_id=?1",
            [recovered.successor_closure_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(successor_verifications, 1);
    let successor_epoch: (i64, String, i64, String, String, String, i64) = conn
        .query_row(
            r#"
            select epoch.epoch_number,epoch.status,epoch.reopen_decision_id,
                   decision.action,decision.decision_value,decision.target_ref,
                   recovery.successor_epoch_decision_id
            from finding_decision_epochs epoch
            join owner_decisions decision on decision.id=epoch.reopen_decision_id
            join finding_design_recoveries recovery
              on recovery.successor_epoch_decision_id=decision.id
            where epoch.finding_id=?1 and epoch.epoch_number=2
            "#,
            [finding.finding_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(successor_epoch.0, 2);
    assert_eq!(successor_epoch.1, "terminal");
    assert_eq!(successor_epoch.2, successor_epoch.6);
    assert_eq!(successor_epoch.3, "reopen");
    assert_eq!(successor_epoch.4, "reopened");
    assert_eq!(
        successor_epoch.5,
        format!("finding_epoch:{}:1", finding.finding_id)
    );
    assert_eq!(session.session_id, 1);
}

#[test]
fn generation_21_rejects_partial_relations_and_missing_integrity_triggers() {
    let partial = tempfile::tempdir().unwrap();
    init_project(partial.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(partial.path())).unwrap();
    conn.execute_batch(
        r#"
        drop trigger trg_finding_verification_project_insert;
        drop trigger trg_finding_verification_project_update;
        drop table finding_design_recoveries;
        delete from schema_migrations where version>=21;
        create table finding_design_recoveries(id integer primary key);
        "#,
    )
    .unwrap();
    drop(conn);
    let inspection = inspect_update(partial.path()).unwrap();
    assert_ne!(inspection.status, "ready_to_apply");
    assert!(
        inspection
            .next_actions
            .iter()
            .all(|action| !action.starts_with("agent-workbench update apply"))
    );

    let missing_trigger = tempfile::tempdir().unwrap();
    init_project(missing_trigger.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(missing_trigger.path())).unwrap();
    conn.execute_batch("drop trigger trg_finding_design_recovery_project_insert;")
        .unwrap();
    drop(conn);
    let inspection = inspect_update(missing_trigger.path()).unwrap();
    assert_ne!(inspection.status, "current");
    assert!(!inspection.next_actions.is_empty());

    let ineffective_trigger = tempfile::tempdir().unwrap();
    init_project(ineffective_trigger.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(ineffective_trigger.path())).unwrap();
    conn.execute_batch(
        r#"
        drop trigger trg_finding_design_recovery_project_insert;
        create trigger trg_finding_design_recovery_project_insert
        before insert on finding_design_recoveries begin
          select 'new.finding_id new.source_closure_id new.source_session_id new.source_attempt_id new.authority_event_id new.successor_design_version_id new.successor_closure_id new.successor_session_id new.successor_attempt_id new.successor_epoch_decision_id finding_decision_epochs raise(abort';
        end;
        "#,
    )
    .unwrap();
    drop(conn);
    let inspection = inspect_update(ineffective_trigger.path()).unwrap();
    assert_ne!(inspection.status, "current");
    assert!(!inspection.next_actions.is_empty());
}
