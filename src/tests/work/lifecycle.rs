use super::*;

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
