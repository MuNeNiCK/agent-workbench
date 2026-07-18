use super::*;
use sha2::{Digest, Sha256};

#[test]
fn reconciliation_inherits_completed_membership_from_closed_phase() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "closed phase migration", None).unwrap();
    let package = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "closed-phase-migration",
            title: "Closed Phase Migration",
        },
    )
    .unwrap();
    fs::write(
        package.package_path.join("requirements").join("README.md"),
        requirement_doc("REQ-001", "Preserve completed behavior", "high"),
    )
    .unwrap();
    fs::write(
        package.package_path.join("validation").join("gates.md"),
        validation_gate_doc("GATE-001"),
    )
    .unwrap();
    let old = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &package.package_path,
            status: "draft",
        },
    )
    .unwrap();
    approve_design_version(
        temp.path(),
        DesignVersionApproval {
            design_version_id: old.design_version_id,
            summary: Some("approved old design"),
        },
    )
    .unwrap();
    let old_task = add_task(
        temp.path(),
        NewTask {
            title: "completed old task",
            priority: "high",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("completed"),
        },
    )
    .unwrap();
    let old_trace = derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: old.design_version_id,
            requirement_key: "REQ-001",
            task_id: old_task.task_id,
            derivation_reason: Some("old completion"),
            checklist_title: Some("old checklist"),
            item_title: None,
            completion_condition: Some("completed"),
        },
    )
    .unwrap();
    let phase = create_phase(
        temp.path(),
        NewWorkPhase {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(old.design_version_id),
            key: "completed",
            title: "Completed",
            kind: "implementation",
            order: 1,
            reason: Some("fixture"),
        },
    )
    .unwrap();
    assign_task_to_phase(temp.path(), phase.phase_id, old_task.task_id).unwrap();
    let old_gate = select_validation_gate(
        temp.path(),
        ValidationGateSelection {
            design_version_id: old.design_version_id,
            gate_key: "GATE-001",
            requirement_key: "REQ-001",
            task_id: old_task.task_id,
            command: None,
            command_profile: None,
            timeout: None,
        },
    )
    .unwrap();
    let inherited_evidence = add_implementation_evidence(
        temp.path(),
        NewImplementationEvidence {
            task_id: Some(old_task.task_id),
            design_version_id: Some(old.design_version_id),
            requirement_key: Some("REQ-001"),
            evidence_type: "commit",
            commit_sha: Some("completed-source"),
            file_path: None,
            line_ref: None,
            symbol: None,
            artifact_path: None,
            note: None,
        },
    )
    .unwrap();
    let inherited_coverage = add_coverage_item(
        temp.path(),
        NewCoverageItem {
            design_version_id: old.design_version_id,
            requirement_key: "REQ-001",
            review_scope_id: None,
            work_unit_id: Some(work.work_unit_id),
            task_id: Some(old_task.task_id),
            requirement: "completed source coverage",
            runtime_boundary_evidence: Some("covered before phase close"),
            ux_boundary_evidence: None,
            lifecycle_boundary_evidence: None,
            tests_or_gates: Some("GATE-001"),
            missing_or_unverified: None,
            status: "covered",
        },
    )
    .unwrap();
    let inherited_run = add_validation_run(
        temp.path(),
        NewValidationRun {
            validation_gate_id: old_gate.validation_gate_id,
            command_usage_id: None,
            repository_snapshot_id: None,
            result: "pass",
            command: None,
            classification: None,
            acceptance_record_id: None,
            artifact_path: None,
            artifact_hash: None,
            notes: Some("passed before close"),
        },
    )
    .unwrap();
    {
        let db = open_existing_project(temp.path()).unwrap();
        db.execute(
            "update tasks set status='closed' where id=?1",
            params![old_task.task_id],
        )
        .unwrap();
        db.execute(
            "update checklist_items set status='closed' where id=?1",
            params![old_trace.checklist_item_id],
        )
        .unwrap();
        db.execute(
            "update work_phases set status='closed', closed_at=current_timestamp, close_summary='fixture complete' where id=?1",
            params![phase.phase_id],
        )
        .unwrap();
        db.execute(
            "insert into work_phase_events(project_id,phase_id,event_type,reason,next_status,created_at) values (1,?1,'closed','fixture','closed',(select closed_at from work_phases where id=?1))",
            params![phase.phase_id],
        )
        .unwrap();
    }
    let historical_alias_task = {
        let db = open_existing_project(temp.path()).unwrap();
        db.execute(
            r#"insert into design_versions(
                 project_id,design_package_id,version_number,source_ref,package_hash,content_hash,
                 package_path,manifest_path,format,manifest_version,status,imported_at,
                 approved_by_authority_event_id,approved_at)
               select project_id,design_package_id,0,source_ref||'-historical-alias',package_hash||'-historical-alias',
                 content_hash||'-historical-alias',package_path,manifest_path,format,manifest_version,'superseded',
                 imported_at,approved_by_authority_event_id,approved_at
               from design_versions where id=?1"#,
            params![old.design_version_id],
        )
        .unwrap();
        let alias_version = db.last_insert_rowid();
        db.execute(
            r#"insert into design_files(project_id,design_package_id,design_version_id,section_key,
                 relative_path,content_hash,line_count)
               select project_id,design_package_id,?1,section_key,relative_path,content_hash,line_count
               from design_files where design_version_id=?2 and section_key='requirements' limit 1"#,
            params![alias_version, old.design_version_id],
        )
        .unwrap();
        let alias_file = db.last_insert_rowid();
        db.execute(
            r#"insert into design_requirements(project_id,design_version_id,source_design_file_id,
                 source_section,requirement_key,revision,requirement_hash,requirement_text,priority,
                 required_surfaces,validation_expectation,status,created_at)
               select project_id,?1,?2,source_section,requirement_key,revision,requirement_hash,
                 requirement_text,priority,required_surfaces,validation_expectation,'active',created_at
               from design_requirements where id=?3"#,
            params![alias_version, alias_file, old_trace.design_requirement_id],
        )
        .unwrap();
        let alias_requirement = db.last_insert_rowid();
        db.execute(
            "insert into tasks(work_unit_id,title,priority,status,source,completion_condition) values(?1,'historical terminal alias','high','accepted_out_of_scope','design','historical alias')",
            params![work.work_unit_id],
        )
        .unwrap();
        let alias_task = db.last_insert_rowid();
        db.execute(
            "insert into checklists(project_id,work_unit_id,design_version_id,title,status,created_at) values(1,?1,?2,'historical alias checklist','closed',current_timestamp)",
            params![work.work_unit_id, alias_version],
        )
        .unwrap();
        let alias_checklist = db.last_insert_rowid();
        db.execute(
            "insert into checklist_items(project_id,checklist_id,design_requirement_id,task_id,item_order,title,completion_condition,status) values(1,?1,?2,?3,1,'historical alias','historical alias','closed')",
            params![alias_checklist, alias_requirement, alias_task],
        )
        .unwrap();
        let alias_item = db.last_insert_rowid();
        db.execute(
            "insert into task_derivations(project_id,design_requirement_id,task_id,checklist_item_id,derivation_reason,status,created_at) values(1,?1,?2,?3,'historical alias','closed',current_timestamp)",
            params![alias_requirement, alias_task, alias_item],
        )
        .unwrap();
        db.execute(
            "insert into work_phase_task_memberships(project_id,phase_id,task_id,assigned_at) values(1,?1,?2,(select closed_at from work_phases where id=?1))",
            params![phase.phase_id, alias_task],
        )
        .unwrap();
        alias_task
    };
    fs::write(
        package.package_path.join("01-introduction-goals.md"),
        "# Introduction And Goals\n\nA refreshed package with unchanged requirements.\n",
    )
    .unwrap();
    let current = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &package.package_path,
            status: "draft",
        },
    )
    .unwrap();
    approve_design_version(
        temp.path(),
        DesignVersionApproval {
            design_version_id: current.design_version_id,
            summary: Some("approved refreshed design"),
        },
    )
    .unwrap();
    let review = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(current.design_version_id),
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
    add_clean_review_run(
        temp.path(),
        review.review_plan_id,
        Some(&format!(
            "review-context:design-review:design={}:work={}",
            current.design_version_id, work.work_unit_id
        )),
        "clean",
    );
    let decomposition = decompose_design(
        temp.path(),
        DesignDecomposition {
            design_version_id: current.design_version_id,
            work_unit_id: work.work_unit_id,
            checklist_title: Some("current checklist"),
            reason: Some("fixture"),
        },
    )
    .unwrap();
    let db = open_existing_project(temp.path()).unwrap();
    let canonical_task: i64 = db
        .query_row(
            "select task_id from checklist_items where checklist_id=?1",
            params![decomposition.checklist_id],
            |row| row.get(0),
        )
        .unwrap();
    drop(db);
    let conflicting_phase = create_phase(
        temp.path(),
        NewWorkPhase {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(current.design_version_id),
            key: "conflicting-current",
            title: "Conflicting current",
            kind: "implementation",
            order: 2,
            reason: Some("atomic rejection fixture"),
        },
    )
    .unwrap();
    let db = open_existing_project(temp.path()).unwrap();

    // With no older fallback candidate, a completed selected baseline whose semantics
    // differ from current must still reject instead of being treated as "nothing to inherit".
    let old_requirement_hash: String = db
        .query_row(
            "select requirement_hash from design_requirements where id=?1",
            params![old_trace.design_requirement_id],
            |row| row.get(0),
        )
        .unwrap();
    db.execute(
        "update design_requirements set requirement_hash=requirement_hash||'-mismatch' where id=?1",
        params![old_trace.design_requirement_id],
    )
    .unwrap();
    let before_selected_mismatch = reconciliation_state_snapshot(&db, work.work_unit_id);
    let selected_mismatch = crate::traceability::reconcile_design_in(
        &db,
        1,
        current.design_version_id,
        work.work_unit_id,
        decomposition.checklist_id,
        "selected-baseline mismatch fixture",
    )
    .unwrap_err();
    assert!(
        selected_mismatch
            .to_string()
            .contains("revision_compatibility")
    );
    assert_eq!(
        reconciliation_state_snapshot(&db, work.work_unit_id),
        before_selected_mismatch
    );
    db.execute(
        "update design_requirements set requirement_hash=?1 where id=?2",
        params![old_requirement_hash, old_trace.design_requirement_id],
    )
    .unwrap();

    let assert_membership_only = |reason| {
        let outcome = crate::traceability::reconcile_design_in(
            &db,
            1,
            current.design_version_id,
            work.work_unit_id,
            decomposition.checklist_id,
            reason,
        )
        .unwrap();
        assert!(outcome.completion_inheritances.is_empty());
        assert_eq!(
            db.query_row(
                "select task_id from work_phase_task_memberships where phase_id=?1",
                params![phase.phase_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            canonical_task
        );
        assert_eq!(
            db.query_row(
                "select status from tasks where id=?1",
                params![canonical_task],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "open"
        );
        assert_eq!(
            db.query_row(
                "select status from work_phases where id=?1",
                params![phase.phase_id],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
            "closed"
        );
    };

    db.execute_batch("savepoint successor_revision_migration")
        .unwrap();
    db.execute(
        "update design_requirements set revision=revision+1, requirement_hash=requirement_hash||'-successor' where design_version_id=?1 and requirement_key='REQ-001'",
        params![current.design_version_id],
    )
    .unwrap();
    db.execute(
        "update task_derivations set status='stale' where id=?1",
        params![old_trace.task_derivation_id],
    )
    .unwrap();
    assert_membership_only("successor revision membership migration");
    db.execute_batch(
        "rollback to successor_revision_migration; release successor_revision_migration",
    )
    .unwrap();

    // Select the greatest-lower approved design baseline before considering whether its
    // implementation can be inherited.  A non-qualifying intervening version must reject;
    // it must never fall back to the older completed implementation.
    db.execute(
        "update design_versions set version_number=3 where id=?1",
        params![current.design_version_id],
    )
    .unwrap();
    db.execute(
        r#"
        insert into design_versions(
            project_id,design_package_id,version_number,source_ref,package_hash,content_hash,
            package_path,manifest_path,format,manifest_version,status,imported_at,
            approved_by_authority_event_id,approved_at
        )
        select project_id,design_package_id,2,source_ref,package_hash||'-intervening',
               content_hash||'-intervening',package_path,manifest_path,format,manifest_version,
               'superseded',imported_at,approved_by_authority_event_id,approved_at
        from design_versions where id=?1
        "#,
        params![old.design_version_id],
    )
    .unwrap();
    let intervening_version = db.last_insert_rowid();
    db.execute(
        r#"
        insert into design_files(
            project_id,design_package_id,design_version_id,section_key,relative_path,
            content_hash,line_count
        )
        select project_id,design_package_id,?1,section_key,relative_path,
               content_hash||'-intervening',line_count
        from design_files where design_version_id=?2 and section_key='requirements'
        "#,
        params![intervening_version, old.design_version_id],
    )
    .unwrap();
    let intervening_file = db.last_insert_rowid();
    db.execute(
        r#"
        insert into design_requirements(
            project_id,design_version_id,source_design_file_id,source_section,requirement_key,
            revision,requirement_hash,requirement_text,priority,required_surfaces,
            validation_expectation,status,created_at
        )
        select project_id,?1,?2,source_section,requirement_key,revision,
               requirement_hash||'-changed',requirement_text||' changed','high',
               required_surfaces,validation_expectation,'active',created_at
        from design_requirements where design_version_id=?3 and requirement_key='REQ-001'
        "#,
        params![intervening_version, intervening_file, old.design_version_id],
    )
    .unwrap();
    let before_no_fallback = reconciliation_state_snapshot(&db, work.work_unit_id);
    let no_fallback = crate::traceability::reconcile_design_in(
        &db,
        1,
        current.design_version_id,
        work.work_unit_id,
        decomposition.checklist_id,
        "greatest-lower no-fallback fixture",
    )
    .unwrap();
    assert!(no_fallback.completion_inheritances.is_empty());
    assert_eq!(
        reconciliation_state_snapshot(&db, work.work_unit_id),
        before_no_fallback
    );
    db.execute(
        "delete from design_versions where id=?1",
        params![intervening_version],
    )
    .unwrap();

    let diagnostic_cases = [
        (
            format!(
                "update implementation_evidence set created_at=datetime((select closed_at from work_phases where id={}),'+1 day') where id={}",
                phase.phase_id, inherited_evidence.implementation_evidence_id
            ),
            format!(
                "update implementation_evidence set created_at=(select closed_at from work_phases where id={}) where id={}",
                phase.phase_id, inherited_evidence.implementation_evidence_id
            ),
            "implementation_evidence",
        ),
        (
            format!(
                "update coverage_items set status='needs_evidence' where id={}",
                inherited_coverage.coverage_item_id
            ),
            format!(
                "update coverage_items set status='covered' where id={}",
                inherited_coverage.coverage_item_id
            ),
            "coverage",
        ),
        (
            format!(
                "update validation_runs set result='fail' where id={}",
                inherited_run.validation_run_id
            ),
            format!(
                "update validation_runs set result='pass' where id={}",
                inherited_run.validation_run_id
            ),
            "gate_compatibility_or_validation",
        ),
    ];
    for (mutate, restore, expected_reason) in diagnostic_cases {
        db.execute_batch(&mutate).unwrap();
        let before = reconciliation_state_snapshot(&db, work.work_unit_id);
        let error = crate::traceability::reconcile_design_in(
            &db,
            1,
            current.design_version_id,
            work.work_unit_id,
            decomposition.checklist_id,
            "typed diagnostic fixture",
        )
        .unwrap_err();
        assert!(error.to_string().contains(expected_reason), "{error:#}");
        assert_eq!(
            reconciliation_state_snapshot(&db, work.work_unit_id),
            before
        );
        db.execute_batch(&restore).unwrap();
    }

    db.execute_batch("savepoint commanded_gate_evidence")
        .unwrap();
    let commanded_metadata = "type: validation_gate_template\nkey: GATE-001\napplies_to: [REQ-001]\nexpected_result: pass\nphase: implementation\ncommand_template: cargo test --locked\nstatus: active";
    let gate_body = "Run the project test suite before implementation handoff.";
    let commanded_document = format!(
        "## GATE-001: Unit test command\n```yaml agent-workbench\n{commanded_metadata}\n```\n\n{gate_body}\n"
    );
    fs::write(
        package.package_path.join("validation").join("gates.md"),
        commanded_document,
    )
    .unwrap();
    let mut commanded_hasher = Sha256::new();
    commanded_hasher.update(commanded_metadata.as_bytes());
    commanded_hasher.update(b"\0");
    commanded_hasher.update(gate_body.as_bytes());
    let commanded_hash = format!("{:x}", commanded_hasher.finalize());
    db.execute(
        "insert into command_usages(project_id,work_unit_id,command,result,created_at) values(1,?1,'cargo test --locked','pass',current_timestamp)",
        params![work.work_unit_id],
    )
    .unwrap();
    let commanded_usage = db.last_insert_rowid();
    db.execute(
        "update validation_gate_templates set command='cargo test --locked',gate_hash=?1 where design_version_id in (?2,?3) and gate_key='GATE-001'",
        params![commanded_hash, old.design_version_id, current.design_version_id],
    )
    .unwrap();
    db.execute(
        "update validation_gates set command='cargo test --locked' where id=?1 or task_id=?2",
        params![old_gate.validation_gate_id, canonical_task],
    )
    .unwrap();
    db.execute(
        "update validation_runs set command='cargo test --locked',command_usage_id=?1 where id=?2",
        params![commanded_usage, inherited_run.validation_run_id],
    )
    .unwrap();

    db.execute_batch("savepoint commanded_gate_success")
        .unwrap();
    let commanded = crate::traceability::reconcile_design_in(
        &db,
        1,
        current.design_version_id,
        work.work_unit_id,
        decomposition.checklist_id,
        "commanded gate evidence",
    )
    .unwrap();
    assert_eq!(commanded.completion_inheritances.len(), 1);
    db.execute_batch("rollback to commanded_gate_success; release commanded_gate_success")
        .unwrap();

    let commanded_rejections = [
        (
            format!(
                "update validation_runs set command_usage_id=null where id={}",
                inherited_run.validation_run_id
            ),
            format!(
                "update validation_runs set command_usage_id={} where id={}",
                commanded_usage, inherited_run.validation_run_id
            ),
        ),
        (
            format!("update command_usages set work_unit_id=null where id={commanded_usage}"),
            format!(
                "update command_usages set work_unit_id={} where id={commanded_usage}",
                work.work_unit_id
            ),
        ),
        (
            format!(
                "update validation_runs set command='cargo check' where id={}",
                inherited_run.validation_run_id
            ),
            format!(
                "update validation_runs set command='cargo test --locked' where id={}",
                inherited_run.validation_run_id
            ),
        ),
        (
            format!("update command_usages set command='cargo check' where id={commanded_usage}"),
            format!(
                "update command_usages set command='cargo test --locked' where id={commanded_usage}"
            ),
        ),
        (
            format!("update command_usages set result='fail' where id={commanded_usage}"),
            format!("update command_usages set result='pass' where id={commanded_usage}"),
        ),
    ];
    for (case, (mutate, restore)) in commanded_rejections.into_iter().enumerate() {
        db.execute_batch(&mutate).unwrap();
        let before = reconciliation_state_snapshot(&db, work.work_unit_id);
        let outcome = crate::traceability::reconcile_design_in(
            &db,
            1,
            current.design_version_id,
            work.work_unit_id,
            decomposition.checklist_id,
            "invalid commanded gate evidence",
        );
        let error = match outcome {
            Ok(_) => panic!("commanded rejection case {case} was accepted"),
            Err(error) => error,
        };
        assert!(
            error
                .to_string()
                .contains("gate_compatibility_or_validation"),
            "{error:#}"
        );
        assert_eq!(
            reconciliation_state_snapshot(&db, work.work_unit_id),
            before
        );
        db.execute_batch(&restore).unwrap();
    }
    db.execute_batch("rollback to commanded_gate_evidence; release commanded_gate_evidence")
        .unwrap();
    fs::write(
        package.package_path.join("validation").join("gates.md"),
        validation_gate_doc("GATE-001"),
    )
    .unwrap();

    db.execute(
        "insert into work_phase_task_memberships(project_id,phase_id,task_id,assigned_at) values(1,?1,?2,current_timestamp)",
        params![conflicting_phase.phase_id, canonical_task],
    )
    .unwrap();
    let before_conflict = reconciliation_state_snapshot(&db, work.work_unit_id);
    let conflict = crate::traceability::reconcile_design_in(
        &db,
        1,
        current.design_version_id,
        work.work_unit_id,
        decomposition.checklist_id,
        "conflicting reconciliation fixture",
    )
    .unwrap_err();
    assert!(
        conflict.to_string().contains("already phase-assigned"),
        "{conflict:#}"
    );
    assert_eq!(
        reconciliation_state_snapshot(&db, work.work_unit_id),
        before_conflict
    );
    db.execute(
        "delete from work_phase_task_memberships where task_id=?1",
        params![canonical_task],
    )
    .unwrap();
    let before_duplicate_membership = reconciliation_state_snapshot(&db, work.work_unit_id);
    assert!(db
        .execute(
            "insert into work_phase_task_memberships(project_id,phase_id,task_id,assigned_at) values (1,?1,?2,current_timestamp)",
            params![conflicting_phase.phase_id, old_task.task_id],
        )
        .is_err());
    assert_eq!(
        reconciliation_state_snapshot(&db, work.work_unit_id),
        before_duplicate_membership
    );
    let lifecycle_cases = [
        (
            format!(
                "insert into tasks(work_unit_id,title,priority,status,source) values ({},'ambiguous derivation','high','open','design'); insert into task_derivations(project_id,design_requirement_id,task_id,status,created_at) values (1,{},last_insert_rowid(),'active',current_timestamp)",
                work.work_unit_id, old_trace.design_requirement_id
            ),
            "delete from tasks where title='ambiguous derivation'".to_string(),
            "task_checklist_lifecycle",
        ),
        (
            format!(
                "update tasks set status='open' where id={}",
                old_task.task_id
            ),
            format!(
                "update tasks set status='closed' where id={}",
                old_task.task_id
            ),
            "task_checklist_lifecycle",
        ),
        (
            format!(
                "update task_derivations set status='stale' where id={}",
                old_trace.task_derivation_id
            ),
            format!(
                "update task_derivations set status='active' where id={}",
                old_trace.task_derivation_id
            ),
            "task_checklist_lifecycle",
        ),
        (
            format!(
                "update checklist_items set status='open' where id={}",
                old_trace.checklist_item_id
            ),
            format!(
                "update checklist_items set status='closed' where id={}",
                old_trace.checklist_item_id
            ),
            "task_checklist_lifecycle",
        ),
        (
            format!(
                "update work_phases set status='accepted_out_of_scope' where id={}",
                phase.phase_id
            ),
            format!(
                "update work_phases set status='closed' where id={}",
                phase.phase_id
            ),
            "closed_phase_boundary",
        ),
        (
            format!(
                "update work_phase_task_memberships set assigned_at=datetime((select closed_at from work_phases where id={}),'+1 day') where task_id={}",
                phase.phase_id, old_task.task_id
            ),
            format!(
                "update work_phase_task_memberships set assigned_at=(select closed_at from work_phases where id={}) where task_id={}",
                phase.phase_id, old_task.task_id
            ),
            "closed_phase_boundary",
        ),
    ];
    let state_snapshot = || reconciliation_state_snapshot(&db, work.work_unit_id);
    for (mutate, restore, expected_reason) in lifecycle_cases {
        db.execute_batch(&mutate).unwrap();
        let before = state_snapshot();
        let error = crate::traceability::reconcile_design_in(
            &db,
            1,
            current.design_version_id,
            work.work_unit_id,
            decomposition.checklist_id,
            "lifecycle rejection fixture",
        )
        .unwrap_err();
        assert!(error.to_string().contains(expected_reason), "{error:#}");
        let after = state_snapshot();
        assert_eq!(after, before);
        db.execute_batch(&restore).unwrap();
    }
    let close_event_id: i64 = db
        .query_row(
            "select id from work_phase_events where phase_id=?1 and event_type='closed'",
            params![phase.phase_id],
            |row| row.get(0),
        )
        .unwrap();
    let closed_at: String = db
        .query_row(
            "select closed_at from work_phases where id=?1",
            params![phase.phase_id],
            |row| row.get(0),
        )
        .unwrap();
    let boundary_cases = [
        (
            format!(
                "insert into work_phase_events(project_id,phase_id,event_type,reason,next_status,created_at) values (1,{},'closed','duplicate','closed',datetime('{}','+1 day'))",
                phase.phase_id, closed_at
            ),
            "delete from work_phase_events where reason='duplicate'".to_string(),
        ),
        (
            format!(
                "update work_phase_events set created_at=datetime('{}','+1 day') where id={}",
                closed_at, close_event_id
            ),
            format!(
                "update work_phase_events set created_at='{}' where id={}",
                closed_at, close_event_id
            ),
        ),
        (
            format!(
                "update work_phases set closed_at=null where id={}",
                phase.phase_id
            ),
            format!(
                "update work_phases set closed_at='{}' where id={}",
                closed_at, phase.phase_id
            ),
        ),
    ];
    for (mutate, restore) in boundary_cases {
        db.execute_batch(&mutate).unwrap();
        let before = state_snapshot();
        let error = crate::traceability::reconcile_design_in(
            &db,
            1,
            current.design_version_id,
            work.work_unit_id,
            decomposition.checklist_id,
            "boundary rejection fixture",
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("closed_phase_boundary"),
            "{error:#}"
        );
        assert_eq!(state_snapshot(), before);
        db.execute_batch(&restore).unwrap();
    }
    db.execute(
        "delete from work_phase_events where phase_id=?1 and event_type='closed'",
        params![phase.phase_id],
    )
    .unwrap();
    let before_missing_event = state_snapshot();
    assert!(
        crate::traceability::reconcile_design_in(
            &db,
            1,
            current.design_version_id,
            work.work_unit_id,
            decomposition.checklist_id,
            "missing close event fixture",
        )
        .is_err()
    );
    assert_eq!(state_snapshot(), before_missing_event);
    db.execute(
        "insert into work_phase_events(project_id,phase_id,event_type,reason,next_status,created_at) values (1,?1,'closed','restored','closed',(select closed_at from work_phases where id=?1))",
        params![phase.phase_id],
    )
    .unwrap();
    let reconciled = crate::traceability::reconcile_design_in(
        &db,
        1,
        current.design_version_id,
        work.work_unit_id,
        decomposition.checklist_id,
        "mediated reconciliation fixture",
    )
    .unwrap();
    assert_eq!(reconciled.completion_inheritances.len(), 1);
    assert_eq!(
        db.query_row(
            "select status from task_derivations where id=?1",
            params![old_trace.task_derivation_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "closed"
    );
    assert_eq!(
        db.query_row(
            "select status from validation_gates where id=?1",
            params![old_gate.validation_gate_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "closed"
    );
    assert_eq!(
        db.query_row(
            "select status from coverage_items where id=?1",
            params![inherited_coverage.coverage_item_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "stale"
    );
    assert_eq!(
        db.query_row(
            "select task_id from work_phase_task_memberships where phase_id=?1",
            params![phase.phase_id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        canonical_task
    );
    assert_eq!(
        db.query_row(
            "select status from tasks where id=?1",
            params![canonical_task],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "closed"
    );
    let replay = crate::traceability::reconcile_design_in(
        &db,
        1,
        current.design_version_id,
        work.work_unit_id,
        decomposition.checklist_id,
        "idempotent canonical membership replay",
    )
    .unwrap();
    assert!(replay.completion_inheritances.is_empty());
    assert_eq!(
        db.query_row(
            "select status from work_phases where id=?1",
            params![phase.phase_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "closed"
    );
    assert_eq!(
        db.query_row(
            "select count(*) from work_phase_task_memberships where task_id=?1",
            params![historical_alias_task],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "select status from tasks where id=?1",
            params![historical_alias_task],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "accepted_out_of_scope"
    );
    drop(db);

    let readiness = implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(current.design_version_id),
        },
    )
    .unwrap();
    assert!(
        readiness
            .items
            .iter()
            .any(|item| { item.name == "validation_gates_selected" && item.result == "pass" })
    );
}
