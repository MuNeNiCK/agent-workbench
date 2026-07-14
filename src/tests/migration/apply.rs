use super::*;

#[test]
fn manual_owner_plan_applies_with_backup_and_materializes_phase_membership() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work_with_options(
        temp.path(),
        WorkStart {
            title: "identity owner",
            responsibility: None,
            design_version_id: None,
            implementation: false,
        },
    )
    .unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            work_unit_id: Some(work.work_unit_id),
            title: "manual task",
            priority: "high",
            source: "user",
            details: Some("stable manual history"),
            completion_condition: Some("done"),
        },
    )
    .unwrap();
    let phase = create_phase(
        temp.path(),
        NewWorkPhase {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            key: "historical",
            title: "Historical",
            kind: "implementation",
            order: 1,
            reason: None,
        },
    )
    .unwrap();
    assign_task_to_phase(temp.path(), phase.phase_id, task.task_id).unwrap();
    close_task(temp.path(), task.task_id, Some("historical-commit")).unwrap();
    close_phase(temp.path(), phase.phase_id, "historical completion").unwrap();

    let index: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), None).unwrap().json).unwrap();
    let owner = index["index"]["entries"][0]["owner_handle"]
        .as_str()
        .unwrap();
    let plan: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), Some(owner)).unwrap().json).unwrap();
    let plan_handle = plan["plan"]["plan_handle"].as_str().unwrap();
    assert!(plan_handle.starts_with("plan_"));
    assert_eq!(plan_handle.len(), "plan_".len() + 64);
    let ambiguity_view: serde_json::Value = serde_json::from_str(
        &list_task_identity_ambiguities(temp.path(), owner, plan_handle)
            .unwrap()
            .json,
    )
    .unwrap();
    assert!(ambiguity_view["ambiguities"].as_array().unwrap().is_empty());
    let applied = apply_task_identity(temp.path(), owner, plan_handle).unwrap();

    assert_eq!(applied.result, "applied");
    assert!(applied.backup_handle.starts_with("backup_"));
    assert!(applied.audit_handle.starts_with("audit_"));
    let recovery_directory = temp.path().join(".agent-workbench/recovery/task-history");
    let backup_metadata = std::fs::read_dir(&recovery_directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("backup-") && name.ends_with(".json"))
        })
        .unwrap();
    let backup_metadata: serde_json::Value =
        serde_json::from_slice(&std::fs::read(backup_metadata).unwrap()).unwrap();
    for field in [
        "owner_digest",
        "component_sha256",
        "source_sha256",
        "database_sha256",
        "bound_plan_sha256",
        "sqlite_sha256",
    ] {
        assert_eq!(backup_metadata[field].as_str().unwrap().len(), 64);
    }
    let intent_paths = std::fs::read_dir(&recovery_directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("intent-"))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        intent_paths
            .iter()
            .filter(|path| path.to_string_lossy().ends_with("-prepared.json"))
            .count(),
        1
    );
    let committed_path = intent_paths
        .iter()
        .find(|path| path.to_string_lossy().ends_with("-committed.json"))
        .unwrap();
    std::fs::remove_file(committed_path).unwrap();
    let retried = apply_task_identity(temp.path(), owner, plan_handle).unwrap();
    assert_eq!(retried.result, applied.result);
    assert_eq!(retried.backup_handle, applied.backup_handle);
    assert_eq!(retried.audit_handle, applied.audit_handle);
    assert_eq!(
        std::fs::read_dir(&recovery_directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.to_string_lossy().ends_with("-committed.json"))
            .count(),
        1
    );
    let audit: serde_json::Value =
        serde_json::from_str(&audit_task_identity(temp.path(), Some(owner)).unwrap().json).unwrap();
    assert_eq!(audit["records"][0]["result"], "applied");
    assert_eq!(audit["records"][0]["audit_handle"], applied.audit_handle);
    let plan_after_apply = plan_task_identity(temp.path(), Some(owner)).unwrap_err();
    assert!(
        plan_after_apply
            .to_string()
            .contains("migration_not_required")
    );
    let after: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), None).unwrap().json).unwrap();
    assert_eq!(after["index"]["entries"][0]["state"], "migrated");
}

#[test]
fn duplicate_historical_tasks_collapse_to_one_revision_with_all_aliases() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work_with_options(
        temp.path(),
        WorkStart {
            title: "stable identity owner",
            responsibility: None,
            design_version_id: None,
            implementation: false,
        },
    )
    .unwrap();
    let tasks = ["old decomposition", "current decomposition"].map(|title| {
        add_task(
            temp.path(),
            NewTask {
                work_unit_id: Some(work.work_unit_id),
                title,
                priority: "high",
                source: "design",
                details: None,
                completion_condition: Some("done"),
            },
        )
        .unwrap()
    });
    let phase = create_phase(
        temp.path(),
        NewWorkPhase {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            key: "completed",
            title: "Completed",
            kind: "implementation",
            order: 1,
            reason: None,
        },
    )
    .unwrap();
    for task in &tasks {
        assign_task_to_phase(temp.path(), phase.phase_id, task.task_id).unwrap();
        close_task(temp.path(), task.task_id, Some("historical-commit")).unwrap();
    }
    close_phase(temp.path(), phase.phase_id, "completed before migration").unwrap();

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
            insert into design_packages(
              id,project_id,design_key,package_id,title,root_path,format,version,
              status,current_design_version_id,created_at,updated_at
            ) values(1,1,'history','history','Task history','design','agent-workbench-design',1,
                     'approved',1,current_timestamp,current_timestamp);
            insert into design_versions(
              id,project_id,design_package_id,version_number,source_ref,package_hash,
              content_hash,package_path,manifest_path,format,manifest_version,status,
              imported_at,approved_at
            ) values(1,1,1,1,'history-v1','a','b','design','design/design.yaml',
                     'agent-workbench-design',1,'approved',current_timestamp,current_timestamp);
            insert into design_files(
              id,project_id,design_package_id,design_version_id,section_key,relative_path,
              content_hash,line_count
            ) values(1,1,1,1,'requirements','requirements.md','c',1);
            insert into design_requirements(
              id,project_id,design_version_id,source_design_file_id,source_section,
              requirement_key,revision,requirement_hash,requirement_text,priority,
              required_surfaces,status,created_at
            ) values(1,1,1,1,'requirements','stable-key',1,'d','Stable behavior',
                     'high','runtime','active',current_timestamp);
            insert into validation_gate_templates(
              id,project_id,design_version_id,source_design_file_id,source_section,
              gate_key,gate_hash,stage,command,expected_result,requirement_keys,
              gate_text,status,created_at
            ) values(1,1,1,1,'validation','quality-check','e','close-ready',null,
                     'first result','stable-key','Opaque gate body','active',current_timestamp);
            insert into validation_gate_template_requirements(
              id,project_id,validation_gate_template_id,design_requirement_id
            ) values(1,1,1,1);
            "#,
    )
    .unwrap();
    for task in &tasks {
        conn.execute(
                "insert into task_derivations(project_id,design_requirement_id,task_id,status,created_at) values(1,1,?1,'active',current_timestamp)",
                params![task.task_id],
            )
            .unwrap();
    }
    conn.execute(
            "insert into checklists(id,project_id,work_unit_id,design_version_id,title,status,created_at) values(1,1,?1,1,'completion','closed',current_timestamp)",
            params![work.work_unit_id],
        )
        .unwrap();
    conn.execute(
            "insert into checklist_items(id,project_id,checklist_id,design_requirement_id,task_id,item_order,title,status) values(1,1,1,1,?1,1,'verified completion','closed')",
            params![tasks[0].task_id],
        )
        .unwrap();
    conn.execute(
            "insert into acceptance_records(id,project_id,target_type,checklist_item_id,acceptance_type,reason,created_by,status,created_at) values(1,1,'checklist_item',1,'explicit_exception','approved historical evidence','user','approved',current_timestamp)",
            [],
        )
        .unwrap();
    conn.execute(
            "insert into implementation_evidence(id,project_id,task_id,design_requirement_id,evidence_type,note,created_at) values(1,1,?1,1,'test','verified historical output',current_timestamp)",
            params![tasks[0].task_id],
        )
        .unwrap();
    drop(conn);

    let index: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), None).unwrap().json).unwrap();
    let owner = index["index"]["entries"][0]["owner_handle"]
        .as_str()
        .unwrap();
    let plan: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), Some(owner)).unwrap().json).unwrap();
    assert_eq!(plan["plan"]["task_identities"].as_array().unwrap().len(), 1);
    assert_eq!(plan["plan"]["revisions"].as_array().unwrap().len(), 1);
    assert_eq!(plan["plan"]["aliases"].as_array().unwrap().len(), 2);
    assert_eq!(plan["plan"]["memberships"].as_array().unwrap().len(), 1);
    assert_eq!(
        plan["plan"]["completion_claims"].as_array().unwrap().len(),
        2
    );
    let claims = plan["plan"]["completion_claims"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["payload"]["claim"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        claims,
        std::collections::BTreeSet::from(["implementation", "validation"])
    );
    let implementation = plan["plan"]["completion_claims"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["payload"]["claim"] == "implementation")
        .unwrap();
    assert_eq!(implementation["payload"]["result"], "carry");
    assert!(
        implementation["payload"]["evidence_handles"][0]
            .as_str()
            .unwrap()
            .starts_with("evidence_")
    );
    let validation = plan["plan"]["completion_claims"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["payload"]["claim"] == "validation")
        .unwrap();
    assert_eq!(validation["payload"]["result"], "reopen");
    assert!(
        validation["payload"]["evidence_handles"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        plan["plan"]["memberships"][0]["reason"],
        "phase_deduplicated"
    );
    let requirement_handle = plan["plan"]["revisions"][0]["after"]["requirement_handle"]
        .as_str()
        .unwrap()
        .to_string();
    let gate_set_handle = plan["plan"]["revisions"][0]["after"]["gate_set_handle"]
        .as_str()
        .unwrap()
        .to_string();
    let priority_handle = plan["plan"]["revisions"][0]["after"]["priority_handle"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(requirement_handle.starts_with("requirement_"));
    assert!(gate_set_handle.starts_with("gate_set_"));
    assert!(priority_handle.starts_with("priority_"));
    let first_plan_handle = plan["plan"]["plan_handle"].as_str().unwrap().to_string();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update validation_gate_templates set expected_result='second result' where id=1",
        [],
    )
    .unwrap();
    drop(conn);
    let changed_index: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), None).unwrap().json).unwrap();
    let changed_owner = changed_index["index"]["entries"][0]["owner_handle"]
        .as_str()
        .unwrap();
    let changed_plan: serde_json::Value = serde_json::from_str(
        &plan_task_identity(temp.path(), Some(changed_owner))
            .unwrap()
            .json,
    )
    .unwrap();
    let changed_plan_handle = changed_plan["plan"]["plan_handle"].as_str().unwrap();
    assert_ne!(owner, changed_owner);
    assert_ne!(first_plan_handle, changed_plan_handle);
    assert_ne!(
        changed_plan["plan"]["revisions"][0]["after"]["requirement_handle"],
        requirement_handle
    );
    assert_ne!(
        changed_plan["plan"]["revisions"][0]["after"]["priority_handle"],
        priority_handle
    );
    assert_ne!(
        changed_plan["plan"]["revisions"][0]["after"]["gate_set_handle"],
        gate_set_handle
    );
    apply_task_identity(temp.path(), changed_owner, changed_plan_handle).unwrap();

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    for (table, expected) in [
        ("task_identities", 1_i64),
        ("task_revisions", 1),
        ("task_revision_aliases", 2),
        ("task_phase_memberships", 1),
        ("task_phase_membership_sources", 2),
        ("task_completion_claims", 1),
        ("task_completion_sources", 1),
    ] {
        let count: i64 = conn
            .query_row(&format!("select count(*) from {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, expected, "unexpected row count for {table}");
    }
    let open_memberships: i64 = conn
            .query_row(
                "select count(*) from task_phase_memberships where state!='closed' or boundary_revision_id is null",
                [],
                |row| row.get(0),
            )
            .unwrap();
    assert_eq!(open_memberships, 0);
}

#[test]
fn phase_dependency_migrates_to_task_identity_endpoints() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work_with_options(
        temp.path(),
        WorkStart {
            title: "dependency owner",
            responsibility: None,
            design_version_id: None,
            implementation: false,
        },
    )
    .unwrap();
    let tasks = ["source task", "target task"].map(|title| {
        add_task(
            temp.path(),
            NewTask {
                work_unit_id: Some(work.work_unit_id),
                title,
                priority: "high",
                source: "user",
                details: None,
                completion_condition: Some("done"),
            },
        )
        .unwrap()
    });
    let phases = [("source", 1_i64), ("target", 2_i64)].map(|(key, order)| {
        create_phase(
            temp.path(),
            NewWorkPhase {
                work_unit_id: work.work_unit_id,
                design_version_id: None,
                key,
                title: key,
                kind: "implementation",
                order,
                reason: None,
            },
        )
        .unwrap()
    });
    for index in 0..2 {
        assign_task_to_phase(temp.path(), phases[index].phase_id, tasks[index].task_id).unwrap();
    }
    add_phase_dependency(
        temp.path(),
        NewPhaseDependency {
            from_phase_id: phases[0].phase_id,
            to_phase_id: phases[1].phase_id,
            dependency_type: "blocks",
            reason: "ordered migration",
        },
    )
    .unwrap();

    let index: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), None).unwrap().json).unwrap();
    let owner = index["index"]["entries"][0]["owner_handle"]
        .as_str()
        .unwrap();
    let plan: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), Some(owner)).unwrap().json).unwrap();
    assert_eq!(plan["plan"]["dependencies"].as_array().unwrap().len(), 1);
    assert_eq!(plan["plan"]["dependencies"][0]["after"]["status"], "open");
    let plan_handle = plan["plan"]["plan_handle"].as_str().unwrap();
    apply_task_identity(temp.path(), owner, plan_handle).unwrap();

    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let target_count: i64 = conn
        .query_row(
            "select count(*) from task_identity_dependencies",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let source_count: i64 = conn
        .query_row(
            "select count(*) from task_identity_dependency_sources",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!((target_count, source_count), (1, 1));
}

#[test]
fn supported_blocked_and_split_phase_states_are_preserved() {
    for phase_state in ["blocked", "split"] {
        let temp = tempfile::tempdir().unwrap();
        init_project(temp.path()).unwrap();
        let work = start_work_with_options(
            temp.path(),
            WorkStart {
                title: "phase state preservation",
                responsibility: None,
                design_version_id: None,
                implementation: false,
            },
        )
        .unwrap();
        let task = add_task(
            temp.path(),
            NewTask {
                work_unit_id: Some(work.work_unit_id),
                title: "historical phase task",
                priority: "high",
                source: "user",
                details: None,
                completion_condition: Some("preserved"),
            },
        )
        .unwrap();
        let phase = create_phase(
            temp.path(),
            NewWorkPhase {
                work_unit_id: work.work_unit_id,
                design_version_id: None,
                key: "historical",
                title: "Historical",
                kind: "implementation",
                order: 1,
                reason: None,
            },
        )
        .unwrap();
        assign_task_to_phase(temp.path(), phase.phase_id, task.task_id).unwrap();
        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        conn.execute(
            "update work_phases set status=?1 where id=?2",
            params![phase_state, phase.phase_id],
        )
        .unwrap();
        drop(conn);

        let index: serde_json::Value =
            serde_json::from_str(&plan_task_identity(temp.path(), None).unwrap().json).unwrap();
        let owner = index["index"]["entries"][0]["owner_handle"]
            .as_str()
            .unwrap();
        let plan: serde_json::Value =
            serde_json::from_str(&plan_task_identity(temp.path(), Some(owner)).unwrap().json)
                .unwrap();
        assert_eq!(
            plan["plan"]["memberships"][0]["after"]["status"],
            phase_state
        );
        let plan_handle = plan["plan"]["plan_handle"].as_str().unwrap();
        apply_task_identity(temp.path(), owner, plan_handle).unwrap();

        let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
        let stored: String = conn
            .query_row("select state from task_phase_memberships", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(stored, phase_state);
    }
}
