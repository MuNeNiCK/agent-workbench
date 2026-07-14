use super::*;

#[test]
fn ambiguous_owner_requires_bound_authority_and_decision() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work_with_options(
        temp.path(),
        WorkStart {
            title: "ambiguous identity owner",
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
            title: "ambiguous historical task",
            priority: "high",
            source: "design",
            details: None,
            completion_condition: Some("resolved"),
        },
    )
    .unwrap();
    let second_task = add_task(
        temp.path(),
        NewTask {
            work_unit_id: Some(work.work_unit_id),
            title: "second ambiguous historical task",
            priority: "high",
            source: "design",
            details: None,
            completion_condition: Some("resolved"),
        },
    )
    .unwrap();
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
            ) values
              (1,1,1,1,'requirements','R-A',1,'d','A','high','runtime','active',current_timestamp),
              (2,1,1,1,'requirements','R-B',1,'e','B','high','runtime','active',current_timestamp),
              (3,1,1,1,'requirements','R-C',1,'f','C','high','runtime','active',current_timestamp),
              (4,1,1,1,'requirements','R-D',1,'g','D','high','runtime','active',current_timestamp);
            "#,
    )
    .unwrap();
    for requirement_id in [1_i64, 2] {
        conn.execute(
                "insert into task_derivations(project_id,design_requirement_id,task_id,status,created_at) values(1,?1,?2,'active',current_timestamp)",
                params![requirement_id, task.task_id],
            )
            .unwrap();
    }
    for requirement_id in [3_i64, 4] {
        conn.execute(
                "insert into task_derivations(project_id,design_requirement_id,task_id,status,created_at) values(1,?1,?2,'active',current_timestamp)",
                params![requirement_id, second_task.task_id],
            )
            .unwrap();
    }
    drop(conn);

    let index: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), None).unwrap().json).unwrap();
    assert_eq!(index["index"]["entries"][0]["state"], "ambiguity_required");
    let owner = index["index"]["entries"][0]["owner_handle"]
        .as_str()
        .unwrap();
    let plan: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), Some(owner)).unwrap().json).unwrap();
    let plan_handle = plan["plan"]["plan_handle"].as_str().unwrap();
    let ambiguity = plan["plan"]["ambiguities"][0]["payload"]["ambiguity_handle"]
        .as_str()
        .unwrap();
    let resolution =
        plan["plan"]["ambiguities"][0]["payload"]["resolutions"][0]["resolution_handle"]
            .as_str()
            .unwrap();
    let authority = record_task_identity_authority(
        temp.path(),
        TaskIdentityAuthorityRequest {
            owner_handle: owner,
            plan_handle,
            ambiguity_handle: ambiguity,
            resolution_handle: Some(resolution),
            retire: false,
            statement: "select the authoritative historical mapping",
            provenance: "user_instruction",
            provenance_ref: "recorded-authority",
        },
    )
    .unwrap();
    let pending: serde_json::Value =
        serde_json::from_str(&audit_task_identity(temp.path(), Some(owner)).unwrap().json).unwrap();
    assert_eq!(pending["pending_recoveries"][0]["authority_count"], 1);
    assert_eq!(pending["pending_recoveries"][0]["decision_count"], 0);
    assert_eq!(pending["pending_recoveries"][0]["ambiguity_count"], 2);
    assert_eq!(
        pending["pending_recoveries"][0]["state"],
        "authority_required"
    );
    let decision = decide_task_identity_ambiguity(
        temp.path(),
        TaskIdentityDecisionRequest {
            owner_handle: owner,
            plan_handle,
            ambiguity_handle: ambiguity,
            resolution_handle: Some(resolution),
            retire: false,
            authority_handle: &authority.authority_handle,
        },
    )
    .unwrap();
    let resolved_audit: serde_json::Value =
        serde_json::from_str(&audit_task_identity(temp.path(), Some(owner)).unwrap().json).unwrap();
    assert_eq!(
        resolved_audit["pending_recoveries"][0]["state"],
        "authority_required"
    );
    let resolved: serde_json::Value = serde_json::from_str(&decision.json).unwrap();
    assert_eq!(resolved["plan"]["mode"], "resolved");
    assert_eq!(resolved["plan"]["ambiguities"].as_array().unwrap().len(), 1);
    assert_eq!(
        resolved["plan"]["dispositions"].as_array().unwrap().len(),
        1
    );
    assert_eq!(
        resolved["plan"]["dispositions"][0]["payload"]["action"],
        "map"
    );
    let second_ambiguity = resolved["plan"]["ambiguities"][0]["payload"]["ambiguity_handle"]
        .as_str()
        .unwrap();
    let second_resolution =
        resolved["plan"]["ambiguities"][0]["payload"]["resolutions"][0]["resolution_handle"]
            .as_str()
            .unwrap();
    let second_authority = record_task_identity_authority(
        temp.path(),
        TaskIdentityAuthorityRequest {
            owner_handle: owner,
            plan_handle,
            ambiguity_handle: second_ambiguity,
            resolution_handle: Some(second_resolution),
            retire: false,
            statement: "select the second authoritative historical mapping",
            provenance: "user_instruction",
            provenance_ref: "recorded-authority-two",
        },
    )
    .unwrap();
    let second_decision = decide_task_identity_ambiguity(
        temp.path(),
        TaskIdentityDecisionRequest {
            owner_handle: owner,
            plan_handle,
            ambiguity_handle: second_ambiguity,
            resolution_handle: Some(second_resolution),
            retire: false,
            authority_handle: &second_authority.authority_handle,
        },
    )
    .unwrap();
    let resolved: serde_json::Value = serde_json::from_str(&second_decision.json).unwrap();
    assert!(
        resolved["plan"]["ambiguities"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        resolved["plan"]["dispositions"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        resolved["plan"]["task_identities"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(resolved["plan"]["revisions"].as_array().unwrap().len(), 2);
    let resolved_handle = resolved["plan"]["plan_handle"].as_str().unwrap();
    let final_pending: serde_json::Value =
        serde_json::from_str(&audit_task_identity(temp.path(), Some(owner)).unwrap().json).unwrap();
    assert_eq!(final_pending["pending_recoveries"][0]["state"], "resolved");
    assert_eq!(final_pending["pending_recoveries"][0]["ambiguity_count"], 2);
    assert_eq!(final_pending["pending_recoveries"][0]["decision_count"], 2);
    let applied = apply_task_identity(temp.path(), owner, resolved_handle).unwrap();
    assert_eq!(applied.result, "applied");
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let materialized: (i64, i64) = conn
        .query_row(
            "select count(*),count(source_design_requirement_id) from task_revisions",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(materialized, (2, 2));
    drop(conn);
    let audit: serde_json::Value =
        serde_json::from_str(&audit_task_identity(temp.path(), Some(owner)).unwrap().json).unwrap();
    assert_eq!(audit["records"][0]["plan_handle"], resolved_handle);
}

#[test]
fn contradictory_dependencies_require_authorized_retirement() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work_with_options(
        temp.path(),
        WorkStart {
            title: "dependency ambiguity owner",
            responsibility: None,
            design_version_id: None,
            implementation: false,
        },
    )
    .unwrap();
    let tasks = ["left task", "right task"].map(|title| {
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
    let phases = [("left", 1_i64), ("right", 2_i64)].map(|(key, order)| {
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
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    for (from_phase_id, to_phase_id) in [
        (phases[0].phase_id, phases[1].phase_id),
        (phases[1].phase_id, phases[0].phase_id),
    ] {
        conn.execute(
                "insert into work_phase_dependencies(project_id,from_phase_id,to_phase_id,dependency_type,reason,status,created_at) values(1,?1,?2,'blocks','conflicting historical dependency','open',current_timestamp)",
                params![from_phase_id, to_phase_id],
            )
            .unwrap();
    }
    drop(conn);

    let index: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), None).unwrap().json).unwrap();
    let owner = index["index"]["entries"][0]["owner_handle"]
        .as_str()
        .unwrap();
    let plan: serde_json::Value =
        serde_json::from_str(&plan_task_identity(temp.path(), Some(owner)).unwrap().json).unwrap();
    assert_eq!(index["index"]["entries"][0]["state"], "ambiguity_required");
    assert!(plan["plan"]["dependencies"].as_array().unwrap().is_empty());
    let plan_handle = plan["plan"]["plan_handle"].as_str().unwrap();
    let ambiguity = plan["plan"]["ambiguities"][0]["payload"]["ambiguity_handle"]
        .as_str()
        .unwrap();
    let authority = record_task_identity_authority(
        temp.path(),
        TaskIdentityAuthorityRequest {
            owner_handle: owner,
            plan_handle,
            ambiguity_handle: ambiguity,
            resolution_handle: None,
            retire: true,
            statement: "retire contradictory historical dependencies",
            provenance: "user_instruction",
            provenance_ref: "dependency-history-authority",
        },
    )
    .unwrap();
    let decision = decide_task_identity_ambiguity(
        temp.path(),
        TaskIdentityDecisionRequest {
            owner_handle: owner,
            plan_handle,
            ambiguity_handle: ambiguity,
            resolution_handle: None,
            retire: true,
            authority_handle: &authority.authority_handle,
        },
    )
    .unwrap();
    let resolved: serde_json::Value = serde_json::from_str(&decision.json).unwrap();
    let resolved_handle = resolved["plan"]["plan_handle"].as_str().unwrap();
    apply_task_identity(temp.path(), owner, resolved_handle).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let dependencies: i64 = conn
        .query_row(
            "select count(*) from task_identity_dependencies",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(dependencies, 0);
}
