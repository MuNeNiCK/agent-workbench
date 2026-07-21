use super::*;
use crate::planning::ensure_design_task_closure_ready;

#[test]
fn mediated_design_decomposition_records_complete_owned_alias_graph() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "repair design decomposition", None).unwrap();
    let init = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "storage-lifecycle",
            title: "Storage Lifecycle",
        },
    )
    .unwrap();
    fs::write(
        init.package_path.join("requirements").join("README.md"),
        format!(
            "{}\n{}",
            requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
            requirement_doc("要件/α!?🚀", "Preserve opaque identity", "high")
        ),
    )
    .unwrap();
    fs::write(
        init.package_path.join("validation").join("gates.md"),
        validation_gate_doc("GATE-001"),
    )
    .unwrap();
    let imported = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &init.package_path,
            status: "draft",
        },
    )
    .unwrap();
    approve_design_version(
        temp.path(),
        DesignVersionApproval {
            design_version_id: imported.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    let ready_plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(imported.design_version_id),
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
    add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: ready_plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some(&format!(
                "review-context:design-review:design={}:work={}",
                imported.design_version_id, work.work_unit_id
            )),
            prompt_deviations: None,
            result_summary: Some("design is ready for mediated decomposition"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: Some("design-reviewer"),
            external_agent_id: Some("design-reviewer-1"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:design-ready"),
        },
    )
    .unwrap();
    let correction_plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(imported.design_version_id),
            review_type: "design_task_decomposition",
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
    let correction_run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: correction_plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("work_unit:1"),
            prompt_deviations: None,
            result_summary: Some("decomposition is missing"),
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
            review_run_id: correction_run.review_run_id,
            finding_type: "design_task_gap",
            severity: "high",
            description: "create the complete decomposition graph",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), finding.finding_id, "valid").unwrap();
    let surface = format!(
        "transition:design-decompose:{}/{},transition:phase-create:{}/{}/@implementation/implementation/1/implementation,transition:phase-assign:@implementation/{},transition:phase-assign:@implementation/{}",
        imported.design_version_id,
        work.work_unit_id,
        work.work_unit_id,
        imported.design_version_id,
        crate::review::encode_opaque_task_ref("要件/α!?🚀"),
        crate::review::encode_opaque_task_ref("REQ-001")
    );
    let closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: finding.finding_id,
            design_invariant: "all active requirements have an owned trace graph",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some(&surface),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("decompose the approved design"),
            tests_or_gates: Some("GATE-001"),
            verification_plan: Some("resume design review"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    assert!(
        decompose_design(
            temp.path(),
            DesignDecomposition {
                design_version_id: imported.design_version_id,
                work_unit_id: work.work_unit_id,
                checklist_title: None,
                reason: None,
            },
        )
        .unwrap_err()
        .to_string()
        .contains("closure correction-begin")
    );
    begin_correction(temp.path(), closure.closure_id).unwrap();
    assert!(
        decompose_design(
            temp.path(),
            DesignDecomposition {
                design_version_id: imported.design_version_id,
                work_unit_id: work.work_unit_id,
                checklist_title: None,
                reason: None,
            },
        )
        .unwrap_err()
        .to_string()
        .contains("closure transition apply")
    );
    apply_correction_transition(temp.path(), closure.closure_id, 1, None, None).unwrap();
    apply_correction_transition(temp.path(), closure.closure_id, 2, None, None).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let (project_id, requirement_id, checklist_id): (i64, i64, i64) = conn
        .query_row(
            r#"
            select requirement.project_id,requirement.id,item.checklist_id
            from design_requirements requirement
            join checklist_items item on item.design_requirement_id=requirement.id
            where requirement.design_version_id=?1 and requirement.requirement_key='要件/α!?🚀'
            "#,
            [imported.design_version_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    conn.execute(
        "insert into tasks(work_unit_id,title,priority,status,source) values(?1,'ambiguous opaque identity','medium','open','design')",
        [work.work_unit_id],
    )
    .unwrap();
    let ambiguous_task = conn.last_insert_rowid();
    conn.execute(
        "insert into checklist_items(project_id,checklist_id,design_requirement_id,task_id,item_order,title,status) values(?1,?2,?3,?4,99,'ambiguous opaque identity','open')",
        params![project_id, checklist_id, requirement_id, ambiguous_task],
    )
    .unwrap();
    let ambiguous_item = conn.last_insert_rowid();
    conn.execute(
        "insert into task_derivations(project_id,design_requirement_id,task_id,checklist_item_id,derivation_reason,status,created_at) values(?1,?2,?3,?4,'ambiguity probe','active',current_timestamp)",
        params![project_id, requirement_id, ambiguous_task, ambiguous_item],
    )
    .unwrap();
    let ambiguous_derivation = conn.last_insert_rowid();
    drop(conn);
    let ambiguity =
        apply_correction_transition(temp.path(), closure.closure_id, 3, None, None).unwrap_err();
    assert!(
        ambiguity
            .to_string()
            .contains("matches multiple stable decomposition identities"),
        "{ambiguity:#}"
    );
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    assert_eq!(
        conn.query_row(
            "select count(*) from work_phase_task_memberships",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap(),
        0
    );
    conn.execute(
        "delete from task_derivations where id=?1",
        [ambiguous_derivation],
    )
    .unwrap();
    conn.execute("delete from checklist_items where id=?1", [ambiguous_item])
        .unwrap();
    conn.execute("delete from tasks where id=?1", [ambiguous_task])
        .unwrap();
    drop(conn);
    apply_correction_transition(temp.path(), closure.closure_id, 3, None, None).unwrap();
    apply_correction_transition(temp.path(), closure.closure_id, 4, None, None).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let aliases: Vec<String> = {
        let mut stmt = conn
            .prepare("select alias from correction_transition_aliases order by alias")
            .unwrap();
        stmt.query_map([], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    };
    assert!(aliases.contains(&"@checklist".to_string()));
    assert!(aliases.contains(&"@task/REQ-001".to_string()));
    assert!(aliases.contains(&"@derivation/REQ-001".to_string()));
    assert!(aliases.contains(&"@checklist-item/REQ-001".to_string()));
    assert!(aliases.contains(&"@coverage/REQ-001".to_string()));
    assert!(aliases.contains(&"@gate/REQ-001/GATE-001".to_string()));
    assert!(aliases.contains(&"@task/要件/α!?🚀".to_string()));
    let opaque_task_id: i64 = conn
        .query_row(
            "select record_id from correction_transition_aliases where alias='@task/要件/α!?🚀'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        conn.query_row(
            "select exists(select 1 from work_phase_task_memberships where task_id=?1)",
            [opaque_task_id],
            |row| row.get::<_, bool>(0),
        )
        .unwrap()
    );
    let task_id: i64 = conn
        .query_row(
            "select record_id from correction_transition_aliases where alias='@task/REQ-001'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let original_revision_id: i64 = conn
        .query_row(
            "select alias.task_revision_id from task_revision_aliases alias where alias.historical_task_id=?1",
            [task_id],
            |row| row.get(0),
        )
        .unwrap();
    let original_details: String = conn
        .query_row("select details from tasks where id=?1", [task_id], |row| {
            row.get(0)
        })
        .unwrap();
    drop(conn);
    revise_task_completion(
        temp.path(),
        TaskCompletionRevision {
            task_id,
            closure_id: closure.closure_id,
            design_version_id: imported.design_version_id,
            requirement_key: "REQ-001",
            details: &original_details,
            completion_condition: "Only the completion condition changed and is now observable.",
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let completion_only_revision: i64 = conn
        .query_row(
            "select task_revision_id from task_revision_aliases where historical_task_id=?1",
            [task_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_ne!(completion_only_revision, original_revision_id);
    assert_eq!(
        conn.query_row(
            "select status from task_revisions where id=?1",
            [original_revision_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
        "historical"
    );
    drop(conn);
    let revised = revise_task_completion(
        temp.path(),
        TaskCompletionRevision {
            task_id,
            closure_id: closure.closure_id,
            design_version_id: imported.design_version_id,
            requirement_key: "REQ-001",
            details: "Own only the storage cleanup implementation; consume shared validation without owning it.",
            completion_condition: "The cleanup surface passes GATE-001 with task-local evidence and no sibling task evidence.",
        },
    )
    .unwrap();
    assert_eq!(revised.checklist_items_updated, 1);
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let persisted: (String, String, String) = conn
        .query_row(
            "select t.details,t.completion_condition,ci.completion_condition from tasks t join checklist_items ci on ci.task_id=t.id where t.id=?1",
            params![task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        persisted,
        (
            "Own only the storage cleanup implementation; consume shared validation without owning it.".into(),
            "The cleanup surface passes GATE-001 with task-local evidence and no sibling task evidence.".into(),
            "The cleanup surface passes GATE-001 with task-local evidence and no sibling task evidence.".into(),
        )
    );
    let canonical: (i64, String, i64) = conn
        .query_row(
            r#"
            select alias.task_revision_id,revision.status,
                   (select count(*) from task_revisions current
                    where current.task_identity_id=revision.task_identity_id
                      and current.status='current')
            from task_revision_aliases alias
            join task_revisions revision on revision.id=alias.task_revision_id
            where alias.historical_task_id=?1
            "#,
            [task_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_ne!(canonical.0, completion_only_revision);
    assert_eq!(canonical.1, "current");
    assert_eq!(canonical.2, 1);
    let original_status: String = conn
        .query_row(
            "select status from task_revisions where id=?1",
            [original_revision_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(original_status, "historical");
    let completion_only_status: String = conn
        .query_row(
            "select status from task_revisions where id=?1",
            [completion_only_revision],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(completion_only_status, "historical");
    let phase_id: i64 = conn
        .query_row(
            "select record_id from correction_transition_aliases where alias='@implementation'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    drop(conn);
    let support = add_correction_support_task(
        temp.path(),
        CorrectionSupportTask {
            task: NewTask {
                title: "Inventory the baseline before implementation",
                priority: "critical",
                source: "design",
                work_unit_id: Some(work.work_unit_id),
                details: Some("Own the immutable baseline inventory only."),
                completion_condition: Some("The complete baseline inventory is reviewed."),
            },
            closure_id: closure.closure_id,
            phase_id,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let assigned: bool = conn
        .query_row(
            "select exists(select 1 from work_phase_task_memberships where phase_id=?1 and task_id=?2)",
            params![phase_id, support.task_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(assigned);
    let close_error = ensure_design_task_closure_ready(&conn, support.task_id)
        .unwrap_err()
        .to_string();
    assert!(
        close_error.contains("task-bound implementation evidence is required"),
        "{close_error}"
    );
    drop(conn);
    let attempt = ready_closure(
        temp.path(),
        ClosureReady {
            closure_id: closure.closure_id,
            implementation_evidence: "decomposition and support task are complete",
            tests_or_gates: "support task lifecycle regression",
            closed_by_commit: None,
        },
    )
    .unwrap();
    let verification = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: correction_plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some(&attempt.context_ref),
            prompt_deviations: None,
            result_summary: Some("verified support task correction"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: Some("decomposition-verifier"),
            external_agent_id: Some("decomposition-verifier-2"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:support-task"),
        },
        Some("verified"),
    )
    .unwrap();
    add_finding_verification(
        temp.path(),
        NewFindingVerification {
            review_run_id: verification.review_run_id,
            finding_id: finding.finding_id,
            closure_id: closure.closure_id,
            result: "verified",
            notes: None,
        },
    )
    .unwrap();
    adjudicate_verification(
        temp.path(),
        verification.review_run_id,
        finding.finding_id,
        closure.closure_id,
        attempt.attempt_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "accept the exact support-task correction",
            expected_current: "pending",
        },
    )
    .unwrap();
    let public_error = close_task(temp.path(), support.task_id, None)
        .unwrap_err()
        .to_string();
    assert!(
        public_error.contains("task-bound implementation evidence is required"),
        "{public_error}"
    );
    add_implementation_evidence(
        temp.path(),
        NewImplementationEvidence {
            task_id: Some(support.task_id),
            design_version_id: None,
            requirement_key: None,
            evidence_type: "artifact",
            commit_sha: None,
            file_path: None,
            line_ref: None,
            symbol: None,
            artifact_path: Some("inventory.json"),
            note: Some("reviewed complete baseline inventory"),
        },
    )
    .unwrap();
    close_task(temp.path(), support.task_id, None).unwrap();
}
