use std::path::Path;

use super::*;

#[test]
fn explicit_close_without_activation_leaves_unrelated_active_owner_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let active = start_work(temp.path(), "unrelated active owner", None).unwrap();
    let inactive_work_unit_id = {
        let conn = crate::db::open_existing_project(temp.path()).unwrap();
        let project = crate::db::project_id(&conn).unwrap();
        conn.execute(
            "insert into work_units(project_id,title,status,started_at) values(?1,'migrated inactive owner','open',current_timestamp)",
            rusqlite::params![project],
        )
        .unwrap();
        conn.last_insert_rowid()
    };
    create_work_record(
        temp.path(),
        NewWorkRecord {
            work_unit_id: Some(inactive_work_unit_id),
            topic: "inactive owner completed",
            work_performed: Some("validated the owner without an activation"),
            next_actions: None,
            notable_operations: None,
            export_path: None,
        },
    )
    .unwrap();

    let ready = close_ready_for(temp.path(), inactive_work_unit_id).unwrap();
    assert_eq!(ready.result, "pass");
    assert_eq!(ready.activation_id, None);
    let closed = close_work(
        temp.path(),
        Some(inactive_work_unit_id),
        "inactive owner complete",
        None,
    )
    .unwrap();
    assert_eq!(closed.work_unit_id, inactive_work_unit_id);
    assert_eq!(closed.activation_id, None);

    let conn = crate::db::open_existing_project(temp.path()).unwrap();
    let inactive_status: String = conn
        .query_row(
            "select status from work_units where id=?1",
            [inactive_work_unit_id],
            |row| row.get(0),
        )
        .unwrap();
    let active_status: String = conn
        .query_row(
            "select status from work_unit_activations where id=?1",
            [active.activation_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(inactive_status, "closed");
    assert_eq!(active_status, "active");
}

#[test]
fn incomplete_decomposition_is_the_owner_action_and_cannot_select_a_phase() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "incomplete decomposition owner", None).unwrap();
    let package = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "incomplete-owner",
            title: "Incomplete Owner",
        },
    )
    .unwrap();
    fs::write(
        package.package_path.join("requirements/README.md"),
        requirement_doc_without_validation("REQ", "Public obligation", "high"),
    )
    .unwrap();
    let design = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &package.package_path,
            status: "draft",
        },
    )
    .unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "existing obligation",
            priority: "high",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
            details: Some("preserve behavior"),
            completion_condition: Some("behavior observed"),
        },
    )
    .unwrap();
    derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: design.design_version_id,
            requirement_key: "REQ",
            task_id: task.task_id,
            derivation_reason: Some("existing decomposition"),
            checklist_title: Some("existing checklist"),
            item_title: Some("existing boundary"),
            completion_condition: Some("behavior observed"),
        },
    )
    .unwrap();
    let phase = create_phase(
        temp.path(),
        NewWorkPhase {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(design.design_version_id),
            key: "existing",
            title: "Existing",
            kind: "implementation",
            order: 1,
            reason: Some("existing schedule"),
        },
    )
    .unwrap();
    assign_task_to_phase(temp.path(), phase.phase_id, task.task_id).unwrap();
    let conn = crate::db::open_existing_project(temp.path()).unwrap();
    crate::decomposition::install_uncovered_derived_bundles(&conn, &[]).unwrap();
    drop(conn);

    let NextAction::OwnerActions { owners } = next_action(temp.path()).unwrap() else {
        panic!("incomplete decomposition must be selected as an owner action");
    };
    let owner = owners
        .iter()
        .find(|owner| owner.owner_id == work.work_unit_id)
        .unwrap();
    assert_eq!(
        owner.blocker_kind.as_deref(),
        Some("decomposition_plan_incomplete")
    );
    assert!(
        owner
            .next_action
            .starts_with("agent-workbench decomposition validate ")
    );
    assert_eq!(owner.next_actions.len(), 2);
    assert_eq!(owner.next_actions[0], owner.next_action);
    assert!(owner.next_actions[1].starts_with("agent-workbench decomposition revise "));
    assert!(!owner.next_actions[1].contains(" --plan "));
    assert!(!owner.next_actions[1].ends_with(" --help"));

    let current = show_decomposition_plan(
        temp.path(),
        DecompositionPlanQuery {
            design_version_id: design.design_version_id,
            work_unit_id: work.work_unit_id,
        },
    )
    .unwrap();
    let source_path = current.source_path.as_deref().unwrap();
    assert!(temp.path().join(source_path).is_file());
    let revised = revise_decomposition_plan(
        temp.path(),
        DecompositionRevise {
            plan_id: current.id,
            plan_path: Path::new(source_path),
            draft: false,
            expected_current: &current.current_identity,
            idempotency_key: &format!("revise-{}-{}", current.id, current.revision),
        },
    )
    .unwrap();
    assert_eq!(revised.plan.predecessor_id, Some(current.id));
    assert_eq!(revised.plan.status, "incomplete");
}

#[test]
fn owner_routing_ignores_replacement_backed_stale_coverage() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "coverage replacement", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "covered task",
            priority: "high",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("coverage is current"),
        },
    )
    .unwrap();
    let package = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "coverage-routing",
            title: "Coverage Routing",
        },
    )
    .unwrap();
    fs::write(
        package.package_path.join("requirements/README.md"),
        requirement_doc("storage", "Preserve storage behavior", "high"),
    )
    .unwrap();
    let design = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &package.package_path,
            status: "draft",
        },
    )
    .unwrap();
    derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: design.design_version_id,
            requirement_key: "storage",
            task_id: task.task_id,
            derivation_reason: None,
            checklist_title: None,
            item_title: None,
            completion_condition: None,
        },
    )
    .unwrap();
    let predecessor = add_coverage_item(
        temp.path(),
        NewCoverageItem {
            design_version_id: design.design_version_id,
            requirement_key: "storage",
            review_scope_id: None,
            work_unit_id: Some(work.work_unit_id),
            task_id: Some(task.task_id),
            requirement: "storage coverage pending",
            runtime_boundary_evidence: None,
            ux_boundary_evidence: None,
            lifecycle_boundary_evidence: None,
            tests_or_gates: None,
            missing_or_unverified: Some("evidence required"),
            status: "needs_evidence",
        },
    )
    .unwrap();
    add_coverage_item(
        temp.path(),
        NewCoverageItem {
            design_version_id: design.design_version_id,
            requirement_key: "storage",
            review_scope_id: None,
            work_unit_id: Some(work.work_unit_id),
            task_id: Some(task.task_id),
            requirement: "storage coverage complete",
            runtime_boundary_evidence: Some("storage runtime covered"),
            ux_boundary_evidence: None,
            lifecycle_boundary_evidence: None,
            tests_or_gates: Some("storage gate"),
            missing_or_unverified: None,
            status: "covered",
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let requirement_id: i64 = conn
        .query_row(
            "select id from design_requirements where design_version_id=?1 and requirement_key='storage'",
            [design.design_version_id],
            |row| row.get(0),
        )
        .unwrap();
    let mut stale_gate_id = None;
    for status in ["stale", "active"] {
        conn.execute(
            "insert into validation_gates(project_id,gate_key,work_unit_id,task_id,design_requirement_id,expected_result,status,created_at) values(1,'storage-gate',?1,?2,?3,'storage remains observable',?4,current_timestamp)",
            params![work.work_unit_id, task.task_id, requirement_id, status],
        )
        .unwrap();
        if status == "stale" {
            stale_gate_id = Some(conn.last_insert_rowid());
        }
    }
    drop(conn);
    let stale_gate_id = stale_gate_id.unwrap();

    let unapproved = next_action(temp.path()).unwrap();
    assert!(matches!(
        unapproved,
        NextAction::OwnerActions { ref owners }
            if owners.iter().any(|owner| {
                owner.owner_id == work.work_unit_id
                    && owner.blocker_kind.as_deref() == Some("stale_design")
                    && owner.next_action.contains(&format!("stale accept validation_gate {stale_gate_id}"))
            })
    ));
    assert!(
        list_stale_records(temp.path())
            .unwrap()
            .iter()
            .any(|record| {
                record.record_type == "validation_gate" && record.id == stale_gate_id
            })
    );

    approve_design_version(
        temp.path(),
        DesignVersionApproval {
            design_version_id: design.design_version_id,
            summary: None,
        },
    )
    .unwrap();

    let next = next_action(temp.path()).unwrap();
    assert_eq!(
        next,
        NextAction::ContinueActive {
            work_unit: ActiveWorkUnit {
                id: work.work_unit_id,
                title: "coverage replacement".to_string(),
                design_version_id: Some(design.design_version_id),
                next_phase_id: None,
                next_phase_key: None,
                next_phase_title: None,
            }
        },
        "replacement-backed stale coverage item {} must not enter owner routing",
        predecessor.coverage_item_id
    );
    assert!(
        !list_stale_records(temp.path())
            .unwrap()
            .iter()
            .any(|record| {
                record.record_type == "validation_gate" && record.id == stale_gate_id
            })
    );
    let readiness = implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(design.design_version_id),
        },
    )
    .unwrap();
    assert!(
        readiness
            .items
            .iter()
            .any(|item| { item.name == "validation_gates_current" && item.result == "pass" })
    );
}

#[test]
fn owner_routing_ignores_replacement_backed_stale_task_derivation() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "derivation replacement", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            title: "replacement-backed task",
            priority: "high",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
            details: None,
            completion_condition: Some("derivation is current"),
        },
    )
    .unwrap();
    let package = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "derivation-routing",
            title: "Derivation Routing",
        },
    )
    .unwrap();
    fs::write(
        package.package_path.join("requirements/README.md"),
        requirement_doc("storage", "Preserve storage behavior", "high"),
    )
    .unwrap();
    let design = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &package.package_path,
            status: "draft",
        },
    )
    .unwrap();
    let predecessor = derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: design.design_version_id,
            requirement_key: "storage",
            task_id: task.task_id,
            derivation_reason: None,
            checklist_title: None,
            item_title: None,
            completion_condition: None,
        },
    )
    .unwrap();
    fs::write(
        package.package_path.join("requirements/README.md"),
        requirement_doc("storage", "Preserve updated storage behavior", "high"),
    )
    .unwrap();
    let successor = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &package.package_path,
            status: "draft",
        },
    )
    .unwrap();
    derive_task_from_requirement(
        temp.path(),
        NewTaskDerivation {
            design_version_id: successor.design_version_id,
            requirement_key: "storage",
            task_id: task.task_id,
            derivation_reason: Some("current replacement"),
            checklist_title: None,
            item_title: None,
            completion_condition: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update task_derivations set status='stale' where id=?1",
        [predecessor.task_derivation_id],
    )
    .unwrap();
    let project = crate::db::project_id(&conn).unwrap();
    assert_eq!(
        crate::traceability::selected_stale_record_in(&conn, project).unwrap(),
        Some((
            "task_derivation".to_string(),
            predecessor.task_derivation_id
        ))
    );
    drop(conn);
    assert!(
        list_stale_records(temp.path())
            .unwrap()
            .iter()
            .any(|record| {
                record.record_type == "task_derivation"
                    && record.id == predecessor.task_derivation_id
            })
    );
    let unapproved_readiness = implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(successor.design_version_id),
        },
    )
    .unwrap();
    assert!(
        unapproved_readiness
            .items
            .iter()
            .any(|item| { item.name == "design_version_approved" && item.result == "fail" }),
        "{:#?}",
        unapproved_readiness.items
    );

    approve_design_version(
        temp.path(),
        DesignVersionApproval {
            design_version_id: successor.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    assert_ne!(
        crate::traceability::selected_stale_record_in(&conn, project).unwrap(),
        Some((
            "task_derivation".to_string(),
            predecessor.task_derivation_id
        ))
    );
    drop(conn);
    assert!(
        !list_stale_records(temp.path())
            .unwrap()
            .iter()
            .any(|record| {
                record.record_type == "task_derivation"
                    && record.id == predecessor.task_derivation_id
            })
    );
    let readiness = implementation_ready(
        temp.path(),
        ImplementationReadyCheck {
            design_version_id: Some(successor.design_version_id),
        },
    )
    .unwrap();
    assert!(
        readiness
            .items
            .iter()
            .any(|item| { item.name == "task_derivations_current" && item.result == "pass" })
    );
}

#[test]
fn owner_local_blocker_does_not_suppress_unrelated_schedulable_owner() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        insert into work_units(id,project_id,title,status,started_at)
        values(1,1,'blocked owner','blocked',current_timestamp);
        insert into work_units(id,project_id,title,status,started_at)
        values(2,1,'ready owner','open',current_timestamp);
        "#,
    )
    .unwrap();

    let status = project_status(temp.path()).unwrap();

    assert!(status.phase_blocker.is_none());
    assert_eq!(status.project_integrity.result, "clear");
    assert_eq!(status.owner_actions.len(), 2);
    assert_eq!(
        status.owner_actions[0].blocker_kind.as_deref(),
        Some("blocked_work_unit")
    );
    assert!(status.owner_actions[0].schedulable);
    assert!(status.owner_actions[1].schedulable);
    assert_eq!(status.owner_actions[1].owner_id, 2);
}

#[test]
fn next_reports_no_active_work_unit_after_init() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();

    let next = next_action(temp.path()).unwrap();

    assert_eq!(next, NextAction::NoOpenWorkUnit);
}

#[test]
fn work_start_creates_active_work_unit() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();

    let started = start_work(temp.path(), "write lifecycle test", Some("test first")).unwrap();
    let next = next_action(temp.path()).unwrap();

    assert_eq!(started.work_unit_id, 1);
    assert_eq!(started.activation_id, 1);
    assert_eq!(
        next,
        NextAction::ContinueActive {
            work_unit: ActiveWorkUnit {
                id: 1,
                title: "write lifecycle test".to_string(),
                design_version_id: None,
                next_phase_id: None,
                next_phase_key: None,
                next_phase_title: None,
            }
        }
    );
}

#[test]
fn work_start_with_design_version_requires_existing_design_work_activation() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
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
        requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
    )
    .unwrap();
    let import = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &init.package_path,
            status: "draft",
        },
    )
    .unwrap();

    let started = start_work_with_options(
        temp.path(),
        WorkStart {
            title: "implement design",
            responsibility: None,
            design_version_id: Some(import.design_version_id),
            implementation: true,
        },
    );
    let next = next_action(temp.path()).unwrap();
    let error = started.unwrap_err().to_string();

    assert!(error.contains("design-derived implementation must activate the work unit"));
    assert_eq!(next, NextAction::NoOpenWorkUnit);
}

#[test]
fn work_activate_existing_open_unit_after_planning() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into work_units(project_id, title, status, started_at) values (1, 'planned implementation', 'open', current_timestamp)",
        [],
    )
    .unwrap();

    assert_eq!(
        next_action(temp.path()).unwrap(),
        NextAction::ActivateOpen {
            work_unit: ActiveWorkUnit {
                id: 1,
                title: "planned implementation".to_string(),
                design_version_id: None,
                next_phase_id: None,
                next_phase_key: None,
                next_phase_title: None,
            }
        }
    );
    let activated = activate_work(
        temp.path(),
        WorkActivate {
            work_unit_id: 1,
            design_version_id: None,
            implementation: false,
            reason: Some("implementation-ready passed"),
        },
    )
    .unwrap();

    assert_eq!(activated.work_unit_id, 1);
    assert_eq!(activated.activation_id, 1);
    assert!(matches!(
        next_action(temp.path()).unwrap(),
        NextAction::ContinueActive { .. }
    ));
}

#[test]
fn next_reports_design_version_for_open_inactive_planned_work() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
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
        requirement_doc("REQ-001", "Preserve cleanup behavior", "high"),
    )
    .unwrap();
    let import = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &init.package_path,
            status: "draft",
        },
    )
    .unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into work_units(project_id, title, status, started_at) values (1, 'planned design implementation', 'open', current_timestamp)",
        [],
    )
    .unwrap();
    conn.execute(
        "insert into checklists(project_id, work_unit_id, design_version_id, title, status, created_at) values (1, 1, ?1, 'REQ-001 implementation checklist', 'active', current_timestamp)",
        params![import.design_version_id],
    )
    .unwrap();

    assert_eq!(
        next_action(temp.path()).unwrap(),
        NextAction::ActivateOpen {
            work_unit: ActiveWorkUnit {
                id: 1,
                title: "planned design implementation".to_string(),
                design_version_id: Some(import.design_version_id),
                next_phase_id: None,
                next_phase_key: None,
                next_phase_title: None,
            }
        }
    );
}

#[test]
fn work_start_refuses_second_active_activation() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    start_work(temp.path(), "one", None).unwrap();

    let second = start_work(temp.path(), "two", None);

    assert!(second.is_err());
}

#[test]
fn work_block_unblock_and_abandon_record_lifecycle_events() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let started = start_work(temp.path(), "lifecycle work", None).unwrap();

    let blocked = block_work(temp.path(), None, "waiting for user decision").unwrap();
    let unblocked =
        unblock_work(temp.path(), Some(started.work_unit_id), "decision recorded").unwrap();
    let abandoned =
        abandon_work(temp.path(), Some(started.work_unit_id), "redo from fork").unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let work_status: String = conn
        .query_row(
            "select status from work_units where id = ?1",
            params![started.work_unit_id],
            |row| row.get(0),
        )
        .unwrap();
    let activation_status: String = conn
        .query_row(
            "select status from work_unit_activations where id = ?1",
            params![started.activation_id],
            |row| row.get(0),
        )
        .unwrap();
    let events: Vec<String> = {
        let mut stmt = conn
            .prepare("select event_type from work_unit_events where work_unit_id = ?1 order by id")
            .unwrap();
        stmt.query_map(params![started.work_unit_id], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };

    assert_eq!(blocked.previous_status, "open");
    assert_eq!(blocked.status, "blocked");
    assert_eq!(unblocked.previous_status, "blocked");
    assert_eq!(unblocked.status, "open");
    assert_eq!(abandoned.previous_status, "open");
    assert_eq!(abandoned.status, "abandoned");
    assert_eq!(work_status, "abandoned");
    assert_eq!(activation_status, "abandoned");
    assert_eq!(
        events,
        vec![
            "opened".to_string(),
            "blocked".to_string(),
            "unblocked".to_string(),
            "abandoned".to_string()
        ]
    );
}

#[test]
fn abandoning_interrupted_child_allows_parent_resume_check() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let parent = start_work(temp.path(), "parent", None).unwrap();
    let interrupt = interrupt_work(temp.path(), "child", "blocks parent").unwrap();

    let blocked = resume_check_basic(temp.path()).unwrap();
    let abandoned = abandon_work(temp.path(), None, "child no longer needed").unwrap();
    let allowed = resume_check_basic(temp.path()).unwrap();
    let resumed = resume_work(temp.path(), allowed.resume_check_id).unwrap();

    assert_eq!(blocked.result, "blocked");
    assert_eq!(abandoned.work_unit_id, interrupt.child_work_unit_id);
    assert_eq!(abandoned.activation_id, Some(interrupt.child_activation_id));
    assert_eq!(allowed.result, "allowed");
    assert_eq!(resumed.activation_id, parent.activation_id);
}

#[test]
fn suspend_and_resume_round_trip() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let started = start_work(temp.path(), "implement resume", None).unwrap();

    let suspended = suspend_work(
        temp.path(),
        "need to validate assumption",
        "continue implementation",
    )
    .unwrap();
    assert!(matches!(
        next_action(temp.path()).unwrap(),
        NextAction::ResumeSuspended { .. }
    ));
    let check = resume_check_basic(temp.path()).unwrap();
    let resumed = resume_work(temp.path(), check.resume_check_id).unwrap();

    assert_eq!(suspended.work_unit_id, started.work_unit_id);
    assert_eq!(check.result, "allowed");
    assert_eq!(resumed.activation_id, started.activation_id);
    assert!(matches!(
        next_action(temp.path()).unwrap(),
        NextAction::ContinueActive { .. }
    ));
}

#[test]
fn trace_aware_resume_loads_new_authority_without_invalidating_the_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let started = start_work(temp.path(), "resume with newer user direction", None).unwrap();
    suspend_work(
        temp.path(),
        "await direction",
        "continue with current rules",
    )
    .unwrap();
    add_authority_event(
        temp.path(),
        NewAuthorityEvent {
            event_type: "user_instruction",
            source: Some("test"),
            summary: "new direction recorded while work is suspended",
            scope: Some("project"),
            precedence: 100,
        },
    )
    .unwrap();

    let check = resume_check(temp.path(), "trace-aware").unwrap();
    assert_eq!(check.result, "allowed");
    let resumed = resume_work(temp.path(), check.resume_check_id).unwrap();
    assert_eq!(resumed.activation_id, started.activation_id);
}
