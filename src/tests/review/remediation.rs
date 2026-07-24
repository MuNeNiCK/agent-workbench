use super::*;

#[test]
fn typed_close_ready_contract_routes_to_source_correction() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "typed close-ready correction", None).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into work_units(project_id,title,status,started_at) values ((select id from projects limit 1),'unrelated owner','open',current_timestamp)",
        [],
    )
    .unwrap();
    let unrelated_work = conn.last_insert_rowid();
    drop(conn);
    let unrelated_plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: unrelated_work,
            design_version_id: None,
            review_type: "implementation_review",
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
    let unrelated_run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: unrelated_plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("work_unit:2"),
            prompt_deviations: None,
            result_summary: Some("unrelated owner finding"),
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
    let unrelated_finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: unrelated_run.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "unrelated owner review action",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), unrelated_finding.finding_id, "invalid").unwrap();
    let prerequisite = create_phase(
        temp.path(),
        NewWorkPhase {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            key: "prerequisite",
            title: "Prerequisite",
            kind: "implementation",
            order: 1,
            reason: Some("required correction input"),
        },
    )
    .unwrap();
    let dependent = create_phase(
        temp.path(),
        NewWorkPhase {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            key: "dependent",
            title: "Dependent",
            kind: "implementation",
            order: 2,
            reason: Some("requires correction evidence"),
        },
    )
    .unwrap();
    let dependency = add_phase_dependency(
        temp.path(),
        NewPhaseDependency {
            from_phase_id: prerequisite.phase_id,
            to_phase_id: dependent.phase_id,
            dependency_type: "blocks",
            reason: "the correction provides prerequisite evidence",
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
            review_policy_id: None,
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
            result_summary: Some("managed correction required"),
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
    let finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: run.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "correct a typed managed surface",
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
            design_invariant: "typed contracts select source correction",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some(&format!(
                "transition:phase-dependency-satisfy:{}",
                dependency.dependency_id
            )),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("create the declared Markdown file"),
            tests_or_gates: Some("source correction routing"),
            verification_plan: Some("inspect the exact correction"),
            closed_by_commit: None,
        },
    )
    .unwrap();

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let token_count: i64 = conn
        .query_row(
            "select count(*) from correction_tokens where closure_id=?1",
            [closure.closure_id],
            |row| row.get(0),
        )
        .unwrap();
    drop(conn);
    let status = project_status(temp.path()).unwrap();
    assert!(
        status.owner_actions.iter().any(|action| {
            action.next_action
                == format!(
                    "agent-workbench closure correction-begin {}",
                    closure.closure_id
                )
        }),
        "tokens={token_count} actions={:?}",
        status
            .owner_actions
            .iter()
            .map(|action| action.next_action.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        remediate_work(temp.path(), finding.finding_id)
            .unwrap_err()
            .to_string()
            .contains("closure correction-begin")
    );
    let correction = begin_correction(temp.path(), closure.closure_id).unwrap();
    assert_eq!(correction.token_count, 1);
    let selected = project_status(temp.path()).unwrap();
    assert!(selected.owner_actions.iter().any(|action| {
        action.next_action
            == format!(
                "agent-workbench closure transition apply {} --token 1 --evidence <evidence-ref>",
                closure.closure_id
            )
    }));
    let selected_command = format!(
        "agent-workbench closure transition apply {} --token 1 --evidence <evidence-ref>",
        closure.closure_id
    );
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update findings set classification='unclassified' where id=?1",
        [unrelated_finding.finding_id],
    )
    .unwrap();
    drop(conn);
    let unrelated_status = project_status_for(temp.path(), Some(unrelated_work)).unwrap();
    assert_eq!(unrelated_status.owner_actions.len(), 1);
    assert_eq!(
        unrelated_status.owner_actions[0].next_action,
        selected_command
    );
    for rejected in [
        suspend_work(temp.path(), "must not bypass correction", "resume").unwrap_err(),
        interrupt_work(
            temp.path(),
            "must not bypass correction",
            "active correction remains selected",
        )
        .unwrap_err(),
        block_work(
            temp.path(),
            Some(unrelated_work),
            "different owner must not bypass correction",
        )
        .unwrap_err(),
        ready_closure(
            temp.path(),
            ClosureReady {
                closure_id: closure.closure_id,
                implementation_evidence: "premature",
                tests_or_gates: "not run",
                closed_by_commit: None,
            },
        )
        .unwrap_err(),
    ] {
        assert!(rejected.to_string().contains(&selected_command));
    }
    assert!(
        apply_correction_transition(
            temp.path(),
            closure.closure_id,
            2,
            None,
            Some("test:wrong-token"),
        )
        .unwrap_err()
        .to_string()
        .contains("registered correction transition token not found")
    );
    let applied = apply_correction_transition(
        temp.path(),
        closure.closure_id,
        1,
        None,
        Some("test:prerequisite-observed"),
    )
    .unwrap();
    assert!(!applied.idempotent);
    let replayed = apply_correction_transition(
        temp.path(),
        closure.closure_id,
        1,
        None,
        Some("test:prerequisite-observed"),
    )
    .unwrap();
    assert!(replayed.idempotent);
    assert_eq!(replayed.application_id, applied.application_id);
    assert_eq!(replayed.result_ref, applied.result_ref);
}

#[test]
fn generation_26_adopter_can_add_and_supersede_close_ready_source_corrections_after_update() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(
        temp.path(),
        "replace remediation with source correction",
        None,
    )
    .unwrap();
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            review_type: "design_implementation_diff",
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
    let run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("work_unit:1"),
            prompt_deviations: None,
            result_summary: Some("the registered remediation needs a source correction"),
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
            finding_type: "design_implementation_drift",
            severity: "high",
            description: "replace the implementation remediation contract",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    let source_finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: run.review_run_id,
            finding_type: "design_implementation_drift",
            severity: "high",
            description: "register the source correction directly",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), finding.finding_id, "valid").unwrap();
    classify_finding(temp.path(), source_finding.finding_id, "valid").unwrap();
    let original = add_closure(
        temp.path(),
        NewClosure {
            finding_id: finding.finding_id,
            design_invariant: "the implementation follows the current design",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some("src/review.rs"),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("repair the implementation"),
            tests_or_gates: Some("cargo test"),
            verification_plan: Some("independent verification"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    let ledger = default_ledger_path(temp.path());
    let conn = open_ledger(&ledger).unwrap();
    conn.execute(
        "update closures set affected_surfaces='transition:design-decompose:80/1' where id=?1",
        [original.closure_id],
    )
    .unwrap();
    let trigger: String = conn
        .query_row(
            "select sql from sqlite_schema where type='trigger' and name='trg_correction_token_links_insert'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let session_trigger: String = conn
        .query_row(
            "select sql from sqlite_schema where type='trigger' and name='trg_correction_session_links_insert'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let current_clause = "and f.status='open' and f.classification='valid'\n            )";
    let legacy_clause = "and f.status='open' and f.classification='valid'\n                  and not (p.required=1 and p.stage='close-ready'\n                           and p.review_type in ('implementation_review','design_implementation_diff'))\n            )";
    let legacy_trigger = trigger.replacen(current_clause, legacy_clause, 1);
    assert_ne!(legacy_trigger, trigger);
    conn.execute_batch("drop trigger trg_correction_token_links_insert;")
        .unwrap();
    conn.execute_batch(&legacy_trigger).unwrap();
    let current_session_clause = "and f.id=new.finding_id and f.status='open' and f.classification='valid'\n                  and exists(\n                      select 1 from correction_tokens token where token.closure_id=c.id\n                  )";
    let legacy_session_clause = "and f.id=new.finding_id and f.status='open' and f.classification='valid'\n                  and not (p.required=1 and p.stage='close-ready'\n                           and p.review_type in ('implementation_review','design_implementation_diff'))";
    let legacy_session_trigger =
        session_trigger.replacen(current_session_clause, legacy_session_clause, 1);
    assert_ne!(legacy_session_trigger, session_trigger);
    conn.execute_batch("drop trigger trg_correction_session_links_insert;")
        .unwrap();
    conn.execute_batch(&legacy_session_trigger).unwrap();
    conn.execute("delete from schema_migrations where version=27", [])
        .unwrap();
    drop(conn);

    let legacy = rusqlite::Connection::open(&ledger).unwrap();
    let blocked = legacy
        .execute(
            "insert into correction_tokens(project_id,closure_id,token_ordinal,token_kind,operation,target,pre_state,pre_hash,status,created_at) values(1,?1,1,'transition','design-decompose','80/1','checklist_max:0',null,'pending',current_timestamp)",
            [original.closure_id],
        )
        .unwrap_err();
    match blocked {
        rusqlite::Error::SqliteFailure(code, _) => assert_eq!(code.extended_code, 1811),
        other => panic!("unexpected legacy trigger error: {other}"),
    }
    drop(legacy);

    let inspection = crate::inspect_update(temp.path()).unwrap();
    assert_eq!(inspection.status, "ready_to_apply");
    crate::apply_update_operation(
        temp.path(),
        &inspection.inspection_handle,
        &inspection.current_identity,
        "install-source-correction-contracts",
    )
    .unwrap();
    assert_eq!(
        crate::inspect_update(temp.path()).unwrap().status,
        "current"
    );

    let added = add_closure(
        temp.path(),
        NewClosure {
            finding_id: source_finding.finding_id,
            design_invariant: "the corrected design owns the decomposition",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some("docs:create:docs/fix.md"),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("decompose the corrected approved design"),
            tests_or_gates: Some("implementation-ready"),
            verification_plan: Some("independent source-correction verification"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    assert!(added.closure_id > original.closure_id);

    let authority = add_authority_event(
        temp.path(),
        NewAuthorityEvent {
            event_type: "user_instruction",
            source: Some("test-owner"),
            summary: "replace the remediation with the required source correction",
            scope: Some("work-unit:1"),
            precedence: 100,
        },
    )
    .unwrap();
    let replacement = supersede_closure(
        temp.path(),
        ClosureSupersession {
            closure_id: original.closure_id,
            new_closure: NewClosure {
                finding_id: finding.finding_id,
                design_invariant: "the corrected design owns the decomposition",
                design_citations: None,
                implementation_evidence: None,
                affected_surfaces: Some("docs:create:docs/replacement.md"),
                same_invariant_search: None,
                other_violations_found: None,
                fix_plan: Some("decompose the corrected approved design"),
                tests_or_gates: Some("implementation-ready"),
                verification_plan: Some("independent source-correction verification"),
                closed_by_commit: None,
            },
            reason: "the finding requires a source correction rather than code remediation",
            authority_event_id: authority.authority_event_id,
        },
    )
    .unwrap();

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let token: (String, String, String) = conn
        .query_row(
            "select token_kind,operation,target from correction_tokens where closure_id=?1",
            [replacement.closure_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        token,
        (
            "file".to_string(),
            "create".to_string(),
            "docs:docs/replacement.md".to_string()
        )
    );
    drop(conn);
    let correction = begin_correction(temp.path(), replacement.closure_id).unwrap();
    assert_eq!(correction.token_count, 1);
}

#[test]
fn remediation_batch_excludes_typed_correction_but_keeps_nonterminal_acceptance() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "mixed correction owner", None).unwrap();
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
            review_policy_id: None,
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
            result_summary: Some("implementation and managed correction required"),
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
    let implementation = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: run.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "repair implementation code",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    let typed = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: run.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "repair managed documentation",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), implementation.finding_id, "valid").unwrap();
    classify_finding(temp.path(), typed.finding_id, "valid").unwrap();
    let authority = add_authority_event(
        temp.path(),
        NewAuthorityEvent {
            event_type: "user_instruction",
            source: Some("test-owner"),
            summary: "accept an evidence gap without disposing the finding",
            scope: Some("finding remediation"),
            precedence: 100,
        },
    )
    .unwrap();
    add_general_acceptance(
        temp.path(),
        NewGeneralAcceptance {
            target: &format!("finding:{}", implementation.finding_id),
            acceptance_type: "evidence_gap",
            reason: "the finding still requires implementation remediation",
            approval_authority_event_id: authority.authority_event_id,
        },
    )
    .unwrap();
    let implementation_closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: implementation.finding_id,
            design_invariant: "implementation behavior is corrected",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some("src/review.rs"),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("repair implementation"),
            tests_or_gates: Some("cargo test"),
            verification_plan: Some("independent verification"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    let typed_closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: typed.finding_id,
            design_invariant: "managed documentation is corrected",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some("docs:create:docs/mixed-fix.md"),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("create the declared document"),
            tests_or_gates: Some("managed correction routing"),
            verification_plan: Some("inspect the exact correction"),
            closed_by_commit: None,
        },
    )
    .unwrap();

    let status = project_status(temp.path()).unwrap();
    assert!(status.owner_actions[0].next_action.contains(&format!(
        "work remediate --finding {}",
        implementation.finding_id
    )));
    let NextAction::OwnerActions { owners } = next_action(temp.path()).unwrap() else {
        panic!("nonterminal finding acceptance must preserve the owner action");
    };
    assert!(owners[0].next_action.contains(&format!(
        "work remediate --finding {}",
        implementation.finding_id
    )));
    let remediation = remediate_work(temp.path(), implementation.finding_id).unwrap();
    assert_eq!(remediation.binding_count, 1);
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let bindings: (i64, i64) = conn
        .query_row(
            "select count(*),sum(closure_id=?1) from finding_remediation_bindings where work_unit_activation_id=?2",
            params![implementation_closure.closure_id, remediation.activation_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(bindings, (1, 1));
    let typed_tokens: i64 = conn
        .query_row(
            "select count(*) from correction_tokens where closure_id=?1",
            [typed_closure.closure_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(typed_tokens, 1);
    drop(conn);

    add_general_acceptance(
        temp.path(),
        NewGeneralAcceptance {
            target: &format!("finding:{}", implementation.finding_id),
            acceptance_type: "explicit_exception",
            reason: "terminally dispose the implementation finding",
            approval_authority_event_id: authority.authority_event_id,
        },
    )
    .unwrap();
    assert!(
        ready_closure(
            temp.path(),
            ClosureReady {
                closure_id: implementation_closure.closure_id,
                implementation_evidence: "must not be accepted after terminal disposition",
                tests_or_gates: "not run",
                closed_by_commit: None,
            },
        )
        .is_err()
    );
    assert!(
        supersede_closure(
            temp.path(),
            ClosureSupersession {
                closure_id: implementation_closure.closure_id,
                new_closure: NewClosure {
                    finding_id: implementation.finding_id,
                    design_invariant: "must not replace a terminally disposed finding",
                    design_citations: None,
                    implementation_evidence: None,
                    affected_surfaces: Some("docs:create:docs/terminal-finding.md"),
                    same_invariant_search: None,
                    other_violations_found: None,
                    fix_plan: Some("not applicable"),
                    tests_or_gates: Some("not applicable"),
                    verification_plan: Some("not applicable"),
                    closed_by_commit: None,
                },
                reason: "must be rejected",
                authority_event_id: authority.authority_event_id,
            },
        )
        .is_err()
    );
}

#[test]
fn source_correction_rejects_repository_authority_without_publishing_a_closure() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "reject repository source correction", None).unwrap();
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            review_type: "design_review",
            required: true,
            stage: "design-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
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
            result_summary: Some("source correction must remain Markdown-only"),
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
    let finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: run.review_run_id,
            finding_type: "design_finding",
            severity: "high",
            description: "repository authority escaped the source-correction registry",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), finding.finding_id, "valid").unwrap();

    for surface in [
        "repository:create:src/new.rs",
        "repository:edit:src/lib.rs",
        "repository:delete:tests/obsolete.rs",
    ] {
        let error = add_closure(
            temp.path(),
            NewClosure {
                finding_id: finding.finding_id,
                design_invariant: "source correction remains in its declared Markdown registry",
                design_citations: None,
                implementation_evidence: None,
                affected_surfaces: Some(surface),
                same_invariant_search: None,
                other_violations_found: None,
                fix_plan: Some("correct only a declared Markdown surface"),
                tests_or_gates: Some("source correction contract"),
                verification_plan: Some("review the exact Markdown correction"),
                closed_by_commit: None,
            },
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("not source-correction authority"),
            "{surface}: {error:#}"
        );
    }
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    assert_eq!(
        conn.query_row(
            "select count(*) from closures where finding_id=?1",
            [finding.finding_id],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    drop(conn);

    let closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: finding.finding_id,
            design_invariant: "source correction remains in its declared Markdown registry",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some("docs:create:docs/correction.md"),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("publish the declared Markdown correction"),
            tests_or_gates: Some("source correction contract"),
            verification_plan: Some("review the exact Markdown correction"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    assert_eq!(
        conn.query_row(
            "select target from correction_tokens where closure_id=?1",
            [closure.closure_id],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "docs:docs/correction.md"
    );
}

#[test]
fn project_local_verification_claim_remains_advisory_until_owner_adjudication() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "project-local verification", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "project-local-verification",
            review_type: "implementation_review",
            max_fresh_agents: 1,
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
            result_summary: Some("found issue"),
            new_findings_count: 1,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: None,
            external_agent_id: Some("source-reviewer"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("source-review:1"),
        },
    )
    .unwrap();
    let finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: source.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "verify independently",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    decide_finding(
        temp.path(),
        finding.finding_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "finding is valid",
            expected_current: "pending",
        },
    )
    .unwrap();
    let closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: finding.finding_id,
            design_invariant: "verification remains advisory",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some("src/review/orchestration.rs"),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("publish a project-local verification claim"),
            tests_or_gates: Some("cargo test"),
            verification_plan: Some("independent exact-attempt verification"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    remediate_work(temp.path(), finding.finding_id).unwrap();
    let prior_attempt = ready_closure(
        temp.path(),
        ClosureReady {
            closure_id: closure.closure_id,
            implementation_evidence: "fixed",
            tests_or_gates: "tests pass",
            closed_by_commit: None,
        },
    )
    .unwrap();
    let prior_review = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some(&prior_attempt.context_ref),
            prompt_deviations: None,
            result_summary: Some("first attempt is not fixed"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: false,
            status: "completed",
            agent_label: None,
            external_agent_id: Some("prior-verification-reviewer"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("verification-output:prior"),
        },
        Some("not_fixed"),
    )
    .unwrap();
    add_finding_verification(
        temp.path(),
        NewFindingVerification {
            review_run_id: prior_review.review_run_id,
            finding_id: finding.finding_id,
            closure_id: closure.closure_id,
            result: "not_fixed",
            notes: Some("first attempt needs another correction"),
        },
    )
    .unwrap();
    adjudicate_verification(
        temp.path(),
        prior_review.review_run_id,
        finding.finding_id,
        closure.closure_id,
        prior_attempt.attempt_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "owner accepts the not-fixed result",
            expected_current: "pending",
        },
    )
    .unwrap();
    let attempt = ready_closure(
        temp.path(),
        ClosureReady {
            closure_id: closure.closure_id,
            implementation_evidence: "fixed on the next attempt",
            tests_or_gates: "tests pass again",
            closed_by_commit: None,
        },
    )
    .unwrap();
    let provenance = issue_review_provenance(
        temp.path(),
        ReviewProvenanceIssue {
            reviewer_ref: "verification-reviewer",
            review_plan_id: plan.review_plan_id,
            target_context: &attempt.context_ref,
            provenance_kind: "external_agent",
            purpose: "finding_fix_verification",
            source_reference: "verification-output:1",
            idempotency_key: "verification-provenance",
        },
    )
    .unwrap();
    let invocation = request_invocation(
        temp.path(),
        InvocationRequest {
            review_plan_id: plan.review_plan_id,
            target_context: &attempt.context_ref,
            reviewer_ref: "verification-reviewer",
            provenance_handle: &provenance.provenance_handle,
            purpose: "finding_fix_verification",
            idempotency_key: "verification-invocation",
            expected_plan_current: "open",
        },
    )
    .unwrap();
    let mismatched = transition_invocation(
        temp.path(),
        InvocationTransitionRequest {
            invocation_id: invocation.invocation_id,
            expected_current: "requested",
            idempotency_key: "verification-wrong-attempt",
            outcome: InvocationTerminal::CompleteVerification {
                claim: "verified",
                attempt: prior_attempt.attempt_id,
                summary: "wrong prior attempt",
            },
        },
    )
    .unwrap_err();
    assert!(
        mismatched
            .to_string()
            .contains("does not match the invocation target")
    );
    let claim = transition_invocation(
        temp.path(),
        InvocationTransitionRequest {
            invocation_id: invocation.invocation_id,
            expected_current: "requested",
            idempotency_key: "verification-complete",
            outcome: InvocationTerminal::CompleteVerification {
                claim: "verified",
                attempt: attempt.attempt_id,
                summary: "exact attempt verified",
            },
        },
    )
    .unwrap();
    let replay = transition_invocation(
        temp.path(),
        InvocationTransitionRequest {
            invocation_id: invocation.invocation_id,
            expected_current: "requested",
            idempotency_key: "verification-complete",
            outcome: InvocationTerminal::CompleteVerification {
                claim: "verified",
                attempt: attempt.attempt_id,
                summary: "exact attempt verified",
            },
        },
    )
    .unwrap();
    assert_eq!(claim.review_run_id, replay.review_run_id);
    assert!(replay.already_applied);
    assert_eq!(list_findings(temp.path(), Some("open")).unwrap().len(), 1);

    adjudicate_verification(
        temp.path(),
        claim.review_run_id.unwrap(),
        finding.finding_id,
        closure.closure_id,
        attempt.attempt_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "owner accepts the independent claim",
            expected_current: "pending",
        },
    )
    .unwrap();
    assert!(list_findings(temp.path(), Some("open")).unwrap().is_empty());
}

#[test]
fn close_ready_finding_allows_remediation_then_requires_exact_resume_verification() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "remediate implementation", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "remediation-policy",
            review_type: "implementation_review",
            max_fresh_agents: 1,
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
    let fresh = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("work_unit:1"),
            prompt_deviations: None,
            result_summary: Some("found issue"),
            new_findings_count: 1,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: Some("reviewer"),
            external_agent_id: Some("reviewer-1"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:1"),
        },
    )
    .unwrap();
    let finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: fresh.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "fix implementation",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), finding.finding_id, "valid").unwrap();
    let empty_invariant = add_closure(
        temp.path(),
        NewClosure {
            finding_id: finding.finding_id,
            design_invariant: " ",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some("src/review.rs"),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("reject empty invariant"),
            tests_or_gates: Some("cargo test"),
            verification_plan: Some("resume review"),
            closed_by_commit: None,
        },
    );
    assert!(
        empty_invariant
            .unwrap_err()
            .to_string()
            .contains("--invariant")
    );
    let incomplete_contract = |surfaces, fix_plan, tests, verification| {
        add_closure(
            temp.path(),
            NewClosure {
                finding_id: finding.finding_id,
                design_invariant: "implementation is correct",
                design_citations: None,
                implementation_evidence: None,
                affected_surfaces: surfaces,
                same_invariant_search: None,
                other_violations_found: None,
                fix_plan,
                tests_or_gates: tests,
                verification_plan: verification,
                closed_by_commit: None,
            },
        )
    };
    for result in [
        incomplete_contract(None, Some("fix"), Some("test"), Some("verify")),
        incomplete_contract(Some("src"), None, Some("test"), Some("verify")),
        incomplete_contract(Some("src"), Some("fix"), None, Some("verify")),
        incomplete_contract(Some("src"), Some("fix"), Some("test"), None),
    ] {
        assert!(result.is_err());
    }
    let closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: finding.finding_id,
            design_invariant: "implementation is correct",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some("src/review.rs"),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("implement lifecycle"),
            tests_or_gates: Some("cargo test"),
            verification_plan: Some("resume review"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update work_unit_activations set status = 'completed', completed_at = current_timestamp where id = ?1",
        params![work.activation_id],
    )
    .unwrap();
    conn.execute(
        "update work_units set status = 'closed', closed_at = current_timestamp where id = ?1",
        params![work.work_unit_id],
    )
    .unwrap();
    drop(conn);
    let recovery_authority = approval_authority_event(temp.path());
    let reopened = reopen_work(
        temp.path(),
        WorkReopen {
            work_unit_id: work.work_unit_id,
            reason: "verified finding invalidates the old closure",
            reason_type: "closure_invalid",
            authority_event_id: Some(recovery_authority),
            acceptance_record_id: None,
        },
    )
    .unwrap();
    assert!(
        ready_closure(
            temp.path(),
            ClosureReady {
                closure_id: closure.closure_id,
                implementation_evidence: "must bind first",
                tests_or_gates: "not yet",
                closed_by_commit: None,
            },
        )
        .unwrap_err()
        .to_string()
        .contains("work remediate")
    );
    remediate_work(temp.path(), finding.finding_id).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let recovery_state: (String, i64) = conn
        .query_row(
            r#"
            select d.status, count(epoch.id)
            from finding_remediation_recovery_epochs epoch
            join work_unit_dependencies d on d.id = epoch.dependency_id
            where epoch.work_unit_activation_id = ?1
            "#,
            params![reopened.activation_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(recovery_state, ("resolved".to_string(), 1));
    drop(conn);
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into work_units(project_id,title,status,started_at) values ((select id from projects limit 1),'dependency helper','open',current_timestamp)",
        [],
    )
    .unwrap();
    let dependency_work_id = conn.last_insert_rowid();
    conn.execute(
        "insert into work_unit_dependencies(work_unit_id,depends_on_work_unit_id,dependency_type,reason,status,created_at) values (?1,?2,'blocks','exercise dependency scheduling','open',current_timestamp)",
        params![work.work_unit_id, dependency_work_id],
    )
    .unwrap();
    let dependency_id = conn.last_insert_rowid();
    drop(conn);
    let status = project_status(temp.path()).unwrap();
    let dependency_blocker = status
        .owner_actions
        .iter()
        .find(|owner| owner.owner_id == work.work_unit_id)
        .unwrap();
    assert!(
        dependency_blocker
            .next_action
            .contains(&format!("work activate {dependency_work_id}"))
    );
    let dependency_activation = activate_work(
        temp.path(),
        WorkActivate {
            work_unit_id: dependency_work_id,
            design_version_id: None,
            implementation: false,
            reason: Some("resolve selected remediation dependency"),
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let owner_activation_status: String = conn
        .query_row(
            "select status from work_unit_activations where id=?1",
            params![reopened.activation_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(owner_activation_status, "suspended");
    conn.execute(
        "update work_unit_activations set status='abandoned',completed_at=current_timestamp where id=?1",
        params![dependency_activation.activation_id],
    )
    .unwrap();
    conn.execute(
        "update work_units set status='abandoned',closed_at=current_timestamp where id=?1",
        params![dependency_work_id],
    )
    .unwrap();
    conn.execute(
        "update work_unit_activations set status='active',suspended_by_activation_id=null where id=?1",
        params![reopened.activation_id],
    )
    .unwrap();
    conn.execute(
        "update work_unit_dependencies set status='resolved',resolved_at=current_timestamp where id=?1",
        params![dependency_id],
    )
    .unwrap();
    drop(conn);
    let blocking_plan = add_review_plan(
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
    let blocking_run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: blocking_plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("work_unit:1"),
            prompt_deviations: None,
            result_summary: Some("blocking design issue"),
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
    let blocking_finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: blocking_run.review_run_id,
            finding_type: "design_finding",
            severity: "critical",
            description: "mixed blocker takes precedence",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    let mixed_status = project_status(temp.path()).unwrap();
    assert!(mixed_status.phase_blocker.is_none());
    assert!(
        mixed_status.owner_actions[0]
            .next_action
            .contains(&format!("finding classify {}", blocking_finding.finding_id))
    );
    assert!(!mixed_status.finding_remediations.is_empty());
    classify_finding(temp.path(), blocking_finding.finding_id, "invalid").unwrap();
    assert!(project_status(temp.path()).unwrap().phase_blocker.is_none());
    assert!(matches!(
        next_action(temp.path()).unwrap(),
        NextAction::OwnerActions { .. }
    ));
    let attempt = ready_closure(
        temp.path(),
        ClosureReady {
            closure_id: closure.closure_id,
            implementation_evidence: "changed review.rs",
            tests_or_gates: "cargo test passes",
            closed_by_commit: Some("abc123"),
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let ready_evidence: (String, Option<String>, String, String) = conn
        .query_row(
            "select c.tests_or_gates, c.implementation_evidence, a.tests_or_gates, a.implementation_evidence from closures c join closure_attempts a on a.closure_id = c.id where c.id = ?1 and a.id = ?2",
            params![closure.closure_id, attempt.attempt_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        ready_evidence,
        (
            "cargo test".to_string(),
            None,
            "cargo test passes".to_string(),
            "changed review.rs".to_string(),
        )
    );
    drop(conn);
    let context = render_finding_fix_context(
        temp.path(),
        finding.finding_id,
        closure.closure_id,
        attempt.attempt_id,
    )
    .unwrap();
    assert!(context.text.contains("contract_tests_or_gates: cargo test"));
    assert!(
        context
            .text
            .contains("attempt_tests_or_gates: cargo test passes")
    );
    assert!(
        project_status(temp.path()).unwrap().owner_actions[0]
            .blocker_kind
            .is_some()
    );
    let wrong = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some("finding:1"),
            prompt_deviations: None,
            result_summary: Some("verified"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: Some("reviewer"),
            external_agent_id: Some("reviewer-2"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:2"),
        },
        Some("verified"),
    );
    assert!(wrong.is_err());
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update closure_attempts set review_run_high_watermark = 999 where id = ?1",
        params![attempt.attempt_id],
    )
    .unwrap();
    drop(conn);
    let stale_high_watermark = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some(&attempt.context_ref),
            prompt_deviations: None,
            result_summary: Some("stale review"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: Some("reviewer"),
            external_agent_id: Some("reviewer-stale"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:stale"),
        },
        Some("verified"),
    );
    assert!(stale_high_watermark.is_err());
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update closure_attempts set review_run_high_watermark = ?1 where id = ?2",
        params![fresh.review_run_id, attempt.attempt_id],
    )
    .unwrap();
    drop(conn);
    let failed_resume = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some(&attempt.context_ref),
            prompt_deviations: None,
            result_summary: Some("not fixed"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: false,
            status: "completed",
            agent_label: Some("reviewer"),
            external_agent_id: Some("reviewer-2"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:2"),
        },
        Some("not_fixed"),
    )
    .unwrap();
    let conflicting = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some(&attempt.context_ref),
            prompt_deviations: None,
            result_summary: Some("verified"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: Some("reviewer"),
            external_agent_id: Some("reviewer-3"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:3"),
        },
        Some("verified"),
    );
    assert!(
        conflicting
            .unwrap_err()
            .to_string()
            .contains("already has a conflicting resume outcome")
    );
    add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some(&attempt.context_ref),
            prompt_deviations: None,
            result_summary: Some("second reviewer also found it not fixed"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: false,
            status: "completed",
            agent_label: Some("reviewer"),
            external_agent_id: Some("reviewer-3"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:3b"),
        },
        Some("not_fixed"),
    )
    .unwrap();
    let status = project_status(temp.path()).unwrap();
    let blocker = &status.owner_actions[0];
    assert!(blocker.next_action.contains("--result not_fixed"));
    let persisted_result = list_review_runs(temp.path(), Some(plan.review_plan_id))
        .unwrap()
        .into_iter()
        .find(|run| run.id == failed_resume.review_run_id)
        .unwrap()
        .finding_fix_result;
    assert_eq!(persisted_result.as_deref(), Some("not_fixed"));
    add_finding_verification(
        temp.path(),
        NewFindingVerification {
            review_run_id: failed_resume.review_run_id,
            finding_id: finding.finding_id,
            closure_id: closure.closure_id,
            result: "not_fixed",
            notes: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let advisory_state: (Option<String>, String, String) = conn
        .query_row(
            "select a.result,c.status,f.lifecycle_state from closure_attempts a join closures c on c.id=a.closure_id join findings f on f.id=c.finding_id where a.id=?1",
            params![attempt.attempt_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        advisory_state,
        (
            None,
            "ready_for_verification".into(),
            "awaiting_verification".into()
        )
    );
    drop(conn);
    let evidence = adjudicate_verification(
        temp.path(),
        failed_resume.review_run_id,
        finding.finding_id,
        closure.closure_id,
        attempt.attempt_id,
        AdjudicationInput {
            decision: "needs_evidence",
            reason: "request more evidence without applying the claim",
            expected_current: "pending",
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let still_advisory: (Option<String>, String) = conn
        .query_row(
            "select a.result,f.lifecycle_state from closure_attempts a join closures c on c.id=a.closure_id join findings f on f.id=c.finding_id where a.id=?1",
            params![attempt.attempt_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(still_advisory, (None, "awaiting_verification".into()));
    drop(conn);
    let accepted_input = AdjudicationInput {
        decision: "accepted",
        reason: "accept exact not-fixed claim",
        expected_current: &evidence.decision_handle,
    };
    let accepted = adjudicate_verification(
        temp.path(),
        failed_resume.review_run_id,
        finding.finding_id,
        closure.closure_id,
        attempt.attempt_id,
        accepted_input.clone(),
    )
    .unwrap();
    let retry = adjudicate_verification(
        temp.path(),
        failed_resume.review_run_id,
        finding.finding_id,
        closure.closure_id,
        attempt.attempt_id,
        accepted_input,
    )
    .unwrap();
    assert_eq!(retry.decision_handle, accepted.decision_handle);
    let stale = adjudicate_verification(
        temp.path(),
        failed_resume.review_run_id,
        finding.finding_id,
        closure.closure_id,
        attempt.attempt_id,
        AdjudicationInput {
            decision: "rejected",
            reason: "stale verification decision",
            expected_current: "pending",
        },
    )
    .unwrap_err();
    assert!(stale.to_string().contains("expected_current_stale"));
    assert_eq!(list_findings(temp.path(), None).unwrap()[0].status, "open");
    assert!(matches!(
        next_action(temp.path()).unwrap(),
        NextAction::OwnerActions { .. }
    ));
    let retry_attempt = ready_closure(
        temp.path(),
        ClosureReady {
            closure_id: closure.closure_id,
            implementation_evidence: "changed review.rs again",
            tests_or_gates: "cargo test passes after retry",
            closed_by_commit: Some("def456"),
        },
    )
    .unwrap();
    assert_ne!(retry_attempt.attempt_id, attempt.attempt_id);
    assert_eq!(retry_attempt.attempt_number, 2);
    let retry_status = project_status(temp.path()).unwrap();
    let retry_action = &retry_status.owner_actions[0].next_action;
    assert!(retry_action.contains(&retry_attempt.context_ref));
    assert!(!retry_action.contains(&format!("--run {}", failed_resume.review_run_id)));
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let retry_tests: (String, String) = conn
        .query_row(
            "select c.tests_or_gates, a.tests_or_gates from closures c join closure_attempts a on a.closure_id = c.id where c.id = ?1 and a.id = ?2",
            params![closure.closure_id, retry_attempt.attempt_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        retry_tests,
        (
            "cargo test".to_string(),
            "cargo test passes after retry".to_string()
        )
    );
    drop(conn);
    let verified_resume = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some(&retry_attempt.context_ref),
            prompt_deviations: None,
            result_summary: Some("verified after retry"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: Some("reviewer"),
            external_agent_id: Some("reviewer-2"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:4"),
        },
        Some("verified"),
    )
    .unwrap();
    add_finding_verification(
        temp.path(),
        NewFindingVerification {
            review_run_id: verified_resume.review_run_id,
            finding_id: finding.finding_id,
            closure_id: closure.closure_id,
            result: "verified",
            notes: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let advisory_state: (Option<String>, String) = conn
        .query_row(
            "select a.result,f.lifecycle_state from closure_attempts a join closures c on c.id=a.closure_id join findings f on f.id=c.finding_id where a.id=?1",
            params![retry_attempt.attempt_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(advisory_state, (None, "awaiting_verification".into()));
    drop(conn);
    let verified_decision = adjudicate_verification(
        temp.path(),
        verified_resume.review_run_id,
        finding.finding_id,
        closure.closure_id,
        retry_attempt.attempt_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "accept exact verified claim",
            expected_current: "pending",
        },
    )
    .unwrap();
    assert_eq!(
        list_findings(temp.path(), None).unwrap()[0].status,
        "closed"
    );
    let terminal_finding = &list_findings(temp.path(), None).unwrap()[0];
    assert_eq!(terminal_finding.terminal_epoch, Some(1));
    assert_eq!(
        terminal_finding.current_decision_handle.as_deref(),
        Some(verified_decision.decision_handle.as_str())
    );
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        "drop trigger trg_finding_epoch_immutable_delete;
         delete from finding_decision_epochs where finding_id=1;",
    )
    .unwrap();
    drop(conn);
    let inspection = inspect_update(temp.path()).unwrap();
    assert_eq!(inspection.status, "ready_to_apply");
    apply_update(temp.path(), &inspection.current_identity).unwrap();
    let migrated_finding = &list_findings(temp.path(), None).unwrap()[0];
    assert_eq!(migrated_finding.terminal_epoch, Some(1));
    assert_eq!(
        migrated_finding.current_decision_handle.as_deref(),
        Some(verified_decision.decision_handle.as_str())
    );
    assert!(classify_finding(temp.path(), finding.finding_id, "needs_evidence").is_err());
    add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("work_unit:1"),
            prompt_deviations: None,
            result_summary: Some("final clean"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: Some("reviewer"),
            external_agent_id: Some("reviewer-3"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:3"),
        },
    )
    .unwrap();
    assert_eq!(list_review_plans(temp.path()).unwrap()[0].status, "clean");

    let reopened = reopen_finding_epoch(
        temp.path(),
        finding.finding_id,
        1,
        AdjudicationInput {
            decision: "reopened",
            reason: "verified result was later found to be incorrect",
            expected_current: &verified_decision.decision_handle,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let epochs = conn
        .prepare(
            "select epoch_number,status,terminal_decision_id,reopen_decision_id from finding_decision_epochs where finding_id=?1 order by epoch_number",
        )
        .unwrap()
        .query_map(params![finding.finding_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, Option<i64>>(3)?,
            ))
        })
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let verified_decision_id: i64 = conn
        .query_row(
            "select id from owner_decisions where decision_handle=?1",
            params![verified_decision.decision_handle],
            |row| row.get(0),
        )
        .unwrap();
    let reopen_decision_id: i64 = conn
        .query_row(
            "select id from owner_decisions where decision_handle=?1",
            params![reopened.decision_handle],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        epochs,
        vec![
            (1, "terminal".into(), Some(verified_decision_id), None),
            (2, "open".into(), None, Some(reopen_decision_id)),
        ]
    );
    let recovered: (String, String) = conn
        .query_row(
            "select f.lifecycle_state,c.status from findings f join closures c on c.finding_id=f.id where f.id=?1",
            params![finding.finding_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(recovered, ("open".into(), "superseded".into()));
    drop(conn);
    let reconsidered = decide_finding(
        temp.path(),
        finding.finding_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "accept the reopened finding",
            expected_current: "pending",
        },
    )
    .unwrap();
    assert_eq!(
        list_findings(temp.path(), Some("open")).unwrap()[0]
            .current_decision_handle
            .as_deref(),
        Some(reconsidered.decision_handle.as_str())
    );
    decide_finding(
        temp.path(),
        finding.finding_id,
        AdjudicationInput {
            decision: "needs_evidence",
            reason: "use the current handle projected by finding list",
            expected_current: &reconsidered.decision_handle,
        },
    )
    .unwrap();
}

#[test]
fn zero_resume_quota_still_allows_exactly_one_required_attempt_review() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "zero quota remediation", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "zero-resume-quota",
            review_type: "implementation_review",
            max_fresh_agents: 1,
            max_resume_agents: 0,
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
            result_summary: Some("found issue"),
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
    let finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: source.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "zero quota finding",
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
            design_invariant: "required verification still runs",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some("src/review.rs"),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("fix issue"),
            tests_or_gates: Some("cargo test"),
            verification_plan: Some("one required resume"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    remediate_work(temp.path(), finding.finding_id).unwrap();
    let attempt = ready_closure(
        temp.path(),
        ClosureReady {
            closure_id: closure.closure_id,
            implementation_evidence: "fixed",
            tests_or_gates: "tests pass",
            closed_by_commit: None,
        },
    )
    .unwrap();
    let run = || NewReviewRun {
        review_plan_id: plan.review_plan_id,
        run_type: "resume",
        run_purpose: "finding_fix_verification",
        target_ref: Some(attempt.context_ref.as_str()),
        prompt_deviations: None,
        result_summary: Some("verified"),
        new_findings_count: 0,
        carried_findings_checked: 1,
        clean_run: true,
        status: "completed",
        agent_label: Some("reviewer"),
        external_agent_id: None,
        review_provenance: "human_review",
        review_provenance_ref: Some("human-review:1"),
    };
    let verified =
        add_review_run_with_finding_result(temp.path(), run(), Some("verified")).unwrap();
    let exceeded =
        add_review_run_with_finding_result(temp.path(), run(), Some("verified")).unwrap_err();
    assert!(exceeded.to_string().contains("limit exceeded"));
    add_finding_verification(
        temp.path(),
        NewFindingVerification {
            review_run_id: verified.review_run_id,
            finding_id: finding.finding_id,
            closure_id: closure.closure_id,
            result: "verified",
            notes: None,
        },
    )
    .unwrap();
    adjudicate_verification(
        temp.path(),
        verified.review_run_id,
        finding.finding_id,
        closure.closure_id,
        attempt.attempt_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "accept the exact verified attempt",
            expected_current: "pending",
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update review_policies set max_fresh_agents = 0 where id = ?1",
        params![policy.review_policy_id],
    )
    .unwrap();
    drop(conn);
    let fresh_run = || NewReviewRun {
        review_plan_id: plan.review_plan_id,
        run_type: "fresh",
        run_purpose: "new_unbiased_review",
        target_ref: Some("work_unit:1"),
        prompt_deviations: None,
        result_summary: Some("final clean"),
        new_findings_count: 0,
        carried_findings_checked: 0,
        clean_run: true,
        status: "completed",
        agent_label: Some("fresh-reviewer"),
        external_agent_id: Some("fresh-reviewer"),
        review_provenance: "external_agent",
        review_provenance_ref: Some("fresh-review:1"),
    };
    add_review_run(temp.path(), fresh_run()).unwrap();
    let fresh_exceeded = add_review_run(temp.path(), fresh_run()).unwrap_err();
    assert!(fresh_exceeded.to_string().contains("limit exceeded"));
}

#[test]
fn remediation_batch_keeps_one_canonical_finding_after_rebinding_and_blocking() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "ordered remediation batch", None).unwrap();
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
            review_policy_id: None,
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
            result_summary: Some("two ordered findings"),
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
    let first = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: run.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "first remediation",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    let second = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: run.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "second remediation",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), first.finding_id, "valid").unwrap();
    classify_finding(temp.path(), second.finding_id, "valid").unwrap();
    let closure = |finding_id, invariant: &'static str| NewClosure {
        finding_id,
        design_invariant: invariant,
        design_citations: None,
        implementation_evidence: None,
        affected_surfaces: Some("src/review.rs"),
        same_invariant_search: None,
        other_violations_found: None,
        fix_plan: Some("repair the selected implementation surface"),
        tests_or_gates: Some("cargo test"),
        verification_plan: Some("independent verification"),
        closed_by_commit: None,
    };
    let first_closure =
        add_closure(temp.path(), closure(first.finding_id, "first invariant")).unwrap();
    let second_closure =
        add_closure(temp.path(), closure(second.finding_id, "second invariant")).unwrap();
    remediate_work(temp.path(), first.finding_id).unwrap();

    let authority = add_authority_event(
        temp.path(),
        NewAuthorityEvent {
            event_type: "user_instruction",
            source: Some("test-owner"),
            summary: "replace the first remediation contract",
            scope: Some("work-unit:1"),
            precedence: 100,
        },
    )
    .unwrap();
    let replacement = supersede_closure(
        temp.path(),
        ClosureSupersession {
            closure_id: first_closure.closure_id,
            new_closure: closure(first.finding_id, "replacement first invariant"),
            reason: "replace the first contract without changing finding precedence",
            authority_event_id: authority.authority_event_id,
        },
    )
    .unwrap();
    remediate_work(temp.path(), first.finding_id).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let binding_count_before: i64 = conn
        .query_row(
            "select count(*) from finding_remediation_bindings",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        conn.execute(
            r#"
            insert into finding_remediation_bindings(
              project_id,finding_id,closure_id,work_unit_id,
              work_unit_activation_id,created_at
            )
            select project_id,finding_id,closure_id,work_unit_id,
                   work_unit_activation_id,current_timestamp
            from finding_remediation_bindings
            where finding_id=?1 and closure_id=?2
            "#,
            params![first.finding_id, replacement.closure_id],
        )
        .is_err()
    );
    drop(conn);
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let binding_count_after: i64 = conn
        .query_row(
            "select count(*) from finding_remediation_bindings",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(binding_count_after, binding_count_before);
    drop(conn);

    block_work(
        temp.path(),
        Some(work.work_unit_id),
        "pause the remediation batch",
    )
    .unwrap();
    let blocked = project_status_for(temp.path(), Some(work.work_unit_id)).unwrap();
    assert_eq!(
        blocked.owner_actions[0].next_action,
        format!(
            "agent-workbench work unblock {} --reason \"<reason>\"; then agent-workbench work remediate --finding {}",
            work.work_unit_id, first.finding_id
        )
    );
    unblock_work(
        temp.path(),
        Some(work.work_unit_id),
        "continue the canonical remediation",
    )
    .unwrap();
    let selected = project_status_for(temp.path(), Some(work.work_unit_id)).unwrap();
    assert_eq!(
        selected.finding_remediations[0].finding_id,
        first.finding_id
    );
    assert_eq!(
        selected.owner_actions[0].next_action,
        format!(
            "implement the scoped fix, then agent-workbench closure ready {} --evidence \"<evidence>\" --tests \"<tests>\"",
            replacement.closure_id
        )
    );
    let out_of_order = ready_closure(
        temp.path(),
        ClosureReady {
            closure_id: second_closure.closure_id,
            implementation_evidence: "second fix",
            tests_or_gates: "tests pass",
            closed_by_commit: None,
        },
    )
    .unwrap_err();
    assert!(
        out_of_order
            .to_string()
            .contains(&format!("finding {} is selected", first.finding_id))
    );
    ready_closure(
        temp.path(),
        ClosureReady {
            closure_id: replacement.closure_id,
            implementation_evidence: "first fix",
            tests_or_gates: "tests pass",
            closed_by_commit: None,
        },
    )
    .unwrap();
}
