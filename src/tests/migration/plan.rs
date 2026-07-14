use super::*;

fn attach_requirement(root: &std::path::Path, task_ids: &[i64]) {
    let conn = open_ledger(&default_ledger_path(root)).unwrap();
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
        ) values(1,1,1,1,'requirements','task-history',1,'d','Preserve task history',
                 'high','runtime','active',current_timestamp);
        "#,
    )
    .unwrap();
    for task_id in task_ids {
        conn.execute(
            "insert into task_derivations(project_id,design_requirement_id,task_id,status,created_at) values(1,1,?1,'active',current_timestamp)",
            params![task_id],
        )
        .unwrap();
    }
}

fn owner_plan(root: &std::path::Path) -> serde_json::Value {
    let index: serde_json::Value =
        serde_json::from_str(&plan_task_identity(root, None).unwrap().json).unwrap();
    let owner = index["index"]["entries"][0]["owner_handle"]
        .as_str()
        .unwrap();
    serde_json::from_str(&plan_task_identity(root, Some(owner)).unwrap().json).unwrap()
}

#[test]
fn one_identity_in_open_and_blocked_phases_is_quarantined() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work_with_options(
        temp.path(),
        WorkStart {
            title: "phase conflict",
            responsibility: None,
            design_version_id: None,
            implementation: false,
        },
    )
    .unwrap();
    let tasks = ["earlier task", "later task"].map(|title| {
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
    attach_requirement(
        temp.path(),
        &tasks.iter().map(|task| task.task_id).collect::<Vec<_>>(),
    );
    for (order, task) in tasks.iter().enumerate() {
        let phase = create_phase(
            temp.path(),
            NewWorkPhase {
                work_unit_id: work.work_unit_id,
                design_version_id: Some(1),
                key: if order == 0 { "earlier" } else { "later" },
                title: if order == 0 { "Earlier" } else { "Later" },
                kind: "implementation",
                order: order as i64 + 1,
                reason: None,
            },
        )
        .unwrap();
        assign_task_to_phase(temp.path(), phase.phase_id, task.task_id).unwrap();
        if order == 1 {
            let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
            conn.execute(
                "update work_phases set status='blocked' where id=?1",
                params![phase.phase_id],
            )
            .unwrap();
        }
    }

    let plan = owner_plan(temp.path());
    assert!(
        plan["plan"]["ambiguities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["payload"]["code"] == "open_open_phase")
    );
    assert!(plan["plan"]["memberships"].as_array().unwrap().is_empty());
}

#[test]
fn completed_design_task_without_closed_checklist_is_quarantined() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work_with_options(
        temp.path(),
        WorkStart {
            title: "completion conflict",
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
            title: "completed without checklist",
            priority: "high",
            source: "design",
            details: None,
            completion_condition: Some("done"),
        },
    )
    .unwrap();
    attach_requirement(temp.path(), &[task.task_id]);
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "update tasks set status='closed', closed_by_commit='completed-change' where id=?1",
        params![task.task_id],
    )
    .unwrap();
    drop(conn);

    let plan = owner_plan(temp.path());
    assert!(
        plan["plan"]["ambiguities"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["payload"]["code"] == "completion_invalid")
    );
}

#[test]
fn supported_incomplete_checklist_states_do_not_block_planning() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work_with_options(
        temp.path(),
        WorkStart {
            title: "incomplete checklist states",
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
            title: "incomplete design task",
            priority: "high",
            source: "design",
            details: None,
            completion_condition: Some("done"),
        },
    )
    .unwrap();
    attach_requirement(temp.path(), &[task.task_id]);
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into checklists(id,project_id,work_unit_id,design_version_id,title,status,created_at) values(1,1,?1,1,'incomplete','stale',current_timestamp)",
        params![work.work_unit_id],
    )
    .unwrap();
    conn.execute(
        "insert into checklist_items(id,project_id,checklist_id,design_requirement_id,task_id,item_order,title,status) values(1,1,1,1,?1,1,'blocked item','blocked')",
        params![task.task_id],
    )
    .unwrap();
    drop(conn);

    let plan = owner_plan(temp.path());
    let claims = plan["plan"]["completion_claims"].as_array().unwrap();
    assert!(!claims.is_empty());
    assert!(claims.iter().all(|claim| {
        claim["reason"] == "obligation_reopened" && claim["after"]["status"] == "open"
    }));
}
