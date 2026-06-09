use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{open_existing_project, project_id};
use crate::review_context::required_plans_missing_context_count;
use crate::rules::{RuleBindingInput, insert_rule_binding};

pub fn derive_task_from_requirement(
    root: &Path,
    input: NewTaskDerivation<'_>,
) -> Result<TaskDerivationOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let requirement = tx
        .query_row(
            r#"
            select id, requirement_key
            from design_requirements
            where project_id = ?1
              and design_version_id = ?2
              and requirement_key = ?3
              and status = 'active'
            "#,
            params![project_id, input.design_version_id, input.requirement_key],
            |row| {
                Ok(ResolvedRequirement {
                    id: row.get(0)?,
                    key: row.get(1)?,
                })
            },
        )
        .optional()?
        .context("active design requirement not found")?;
    let task = tx
        .query_row(
            r#"
            select id, work_unit_id, title, completion_condition
            from tasks
            where id = ?1
            "#,
            params![input.task_id],
            |row| {
                Ok(ResolvedTask {
                    id: row.get(0)?,
                    work_unit_id: row.get(1)?,
                    title: row.get(2)?,
                    completion_condition: row.get(3)?,
                })
            },
        )
        .optional()?
        .context("task not found")?;
    let Some(work_unit_id) = task.work_unit_id else {
        bail!("design-derived task must belong to a work unit");
    };
    let checklist_title = input
        .checklist_title
        .unwrap_or("Design implementation checklist");
    let checklist_id = get_or_create_checklist(
        &tx,
        project_id,
        work_unit_id,
        input.design_version_id,
        checklist_title,
    )?;
    let item_order: i64 = tx.query_row(
        "select coalesce(max(item_order), 0) + 1 from checklist_items where checklist_id = ?1",
        params![checklist_id],
        |row| row.get(0),
    )?;
    let item_title = input
        .item_title
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}: {}", requirement.key, task.title));
    let completion_condition = input
        .completion_condition
        .or(task.completion_condition.as_deref());
    tx.execute(
        r#"
        insert into checklist_items(
            project_id, checklist_id, design_requirement_id, task_id, item_order,
            title, completion_condition, status
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'open')
        "#,
        params![
            project_id,
            checklist_id,
            requirement.id,
            task.id,
            item_order,
            item_title,
            completion_condition,
        ],
    )?;
    let checklist_item_id = tx.last_insert_rowid();
    tx.execute(
        r#"
        insert into task_derivations(
            project_id, design_requirement_id, task_id, checklist_item_id,
            derivation_reason, status, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, 'active', current_timestamp)
        "#,
        params![
            project_id,
            requirement.id,
            task.id,
            checklist_item_id,
            input.derivation_reason,
        ],
    )?;
    let task_derivation_id = tx.last_insert_rowid();
    tx.commit()?;

    Ok(TaskDerivationOutcome {
        task_derivation_id,
        checklist_id,
        checklist_item_id,
        design_requirement_id: requirement.id,
        task_id: task.id,
    })
}

pub fn decompose_design(
    root: &Path,
    input: DesignDecomposition<'_>,
) -> Result<DesignDecompositionOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    tx.query_row(
        "select 1 from work_units where id = ?1 and project_id = ?2",
        params![input.work_unit_id, project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("work unit not found")?;
    tx.query_row(
        "select 1 from design_versions where id = ?1 and project_id = ?2",
        params![input.design_version_id, project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("design version not found")?;
    ensure_design_ready_for_decomposition(&tx, project_id, input.design_version_id)?;

    let mut stmt = tx.prepare(
        r#"
        select dr.id, dr.requirement_key, dr.requirement_text, dr.priority
        from design_requirements dr
        where dr.project_id = ?1
          and dr.design_version_id = ?2
          and dr.status = 'active'
          and not exists (
              select 1
              from task_derivations td
              where td.design_requirement_id = dr.id
                and td.status = 'active'
          )
        order by dr.requirement_key
        "#,
    )?;
    let rows = stmt.query_map(params![project_id, input.design_version_id], |row| {
        Ok(RequirementForDecomposition {
            id: row.get(0)?,
            key: row.get(1)?,
            text: row.get(2)?,
            priority: row.get(3)?,
        })
    })?;
    let mut requirements = Vec::new();
    for row in rows {
        requirements.push(row?);
    }
    drop(stmt);

    let checklist_title = input
        .checklist_title
        .unwrap_or("Design implementation checklist");
    let checklist_id = get_or_create_checklist(
        &tx,
        project_id,
        input.work_unit_id,
        input.design_version_id,
        checklist_title,
    )?;
    let mut created_tasks = 0;
    let mut created_derivations = 0;
    let mut created_validation_gates = 0;
    for requirement in requirements {
        let task_title = format!(
            "Implement {}: {}",
            requirement.key,
            first_line(&requirement.text)
        );
        let completion_condition = format!(
            "Requirement {} is implemented and validated",
            requirement.key
        );
        tx.execute(
            r#"
            insert into tasks(
                work_unit_id, title, priority, status, source,
                details, completion_condition
            )
            values (?1, ?2, ?3, 'open', 'design', ?4, ?5)
            "#,
            params![
                input.work_unit_id,
                task_title,
                requirement.priority,
                requirement.text,
                completion_condition,
            ],
        )?;
        let task_id = tx.last_insert_rowid();
        created_tasks += 1;

        let item_order: i64 = tx.query_row(
            "select coalesce(max(item_order), 0) + 1 from checklist_items where checklist_id = ?1",
            params![checklist_id],
            |row| row.get(0),
        )?;
        tx.execute(
            r#"
            insert into checklist_items(
                project_id, checklist_id, design_requirement_id, task_id,
                item_order, title, completion_condition
            )
            values (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                project_id,
                checklist_id,
                requirement.id,
                task_id,
                item_order,
                task_title,
                completion_condition,
            ],
        )?;
        let checklist_item_id = tx.last_insert_rowid();
        tx.execute(
            r#"
            insert into task_derivations(
                project_id, design_requirement_id, task_id, checklist_item_id,
                derivation_reason, status, created_at
            )
            values (?1, ?2, ?3, ?4, ?5, 'active', current_timestamp)
            "#,
            params![
                project_id,
                requirement.id,
                task_id,
                checklist_item_id,
                input.reason,
            ],
        )?;
        created_derivations += 1;
        let gate_templates = validation_gate_templates_for_requirement(
            &tx,
            project_id,
            input.design_version_id,
            requirement.id,
        )?;
        for template in gate_templates {
            tx.execute(
                r#"
                insert into validation_gates(
                    project_id, gate_key, template_id, work_unit_id, task_id,
                    design_requirement_id, command, expected_result,
                    selected_before_edit, status, created_at
                )
                values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, 'active', current_timestamp)
                "#,
                params![
                    project_id,
                    template.gate_key,
                    template.id,
                    input.work_unit_id,
                    task_id,
                    requirement.id,
                    template.command,
                    template.expected_result,
                ],
            )?;
            let validation_gate_id = tx.last_insert_rowid();
            let work_scope = input.work_unit_id.to_string();
            insert_rule_binding(
                &tx,
                RuleBindingInput {
                    project_id,
                    rule_source_type: "validation_gate",
                    authority_event_id: None,
                    user_correction_id: None,
                    command_profile_id: None,
                    review_policy_id: None,
                    review_plan_id: None,
                    work_unit_id: Some(input.work_unit_id),
                    validation_gate_id: Some(validation_gate_id),
                    acceptance_record_id: None,
                    scope_type: "work_unit",
                    scope_key: Some(&work_scope),
                    precedence: 62,
                },
            )?;
            created_validation_gates += 1;
        }
    }
    tx.commit()?;

    Ok(DesignDecompositionOutcome {
        design_version_id: input.design_version_id,
        work_unit_id: input.work_unit_id,
        checklist_id,
        created_tasks,
        created_derivations,
        created_validation_gates,
    })
}

fn validation_gate_templates_for_requirement(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_version_id: i64,
    design_requirement_id: i64,
) -> Result<Vec<ResolvedGateTemplate>> {
    let mut stmt = conn.prepare(
        r#"
        select g.id, g.gate_key, g.command, g.expected_result
        from validation_gate_templates g
        join validation_gate_template_requirements gr
          on gr.validation_gate_template_id = g.id
        where g.project_id = ?1
          and g.design_version_id = ?2
          and gr.design_requirement_id = ?3
          and g.status = 'active'
        order by g.id
        "#,
    )?;
    let rows = stmt.query_map(
        params![project_id, design_version_id, design_requirement_id],
        |row| {
            Ok(ResolvedGateTemplate {
                id: row.get(0)?,
                gate_key: row.get(1)?,
                command: row.get(2)?,
                expected_result: row.get(3)?,
            })
        },
    )?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

pub fn list_checklists(root: &Path, status: Option<&str>) -> Result<Vec<ChecklistRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select c.id, c.work_unit_id, c.design_version_id, c.title, c.status,
               count(ci.id) as item_count,
               sum(case when ci.status = 'closed' then 1 else 0 end) as closed_count
        from checklists c
        left join checklist_items ci on ci.checklist_id = c.id
        where c.project_id = ?1
          and (?2 is null or c.status = ?2)
        group by c.id, c.work_unit_id, c.design_version_id, c.title, c.status
        order by c.id
        "#,
    )?;
    let rows = stmt.query_map(params![project_id, status], |row| {
        Ok(ChecklistRecord {
            id: row.get(0)?,
            work_unit_id: row.get(1)?,
            design_version_id: row.get(2)?,
            title: row.get(3)?,
            status: row.get(4)?,
            item_count: row.get(5)?,
            closed_count: row.get(6)?,
        })
    })?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

pub fn list_stale_records(root: &Path) -> Result<Vec<StaleRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut records = Vec::new();
    collect_stale_rows(
        &conn,
        project_id,
        "task_derivation",
        r#"
        select td.id, dr.requirement_key
        from task_derivations td
        join design_requirements dr on dr.id = td.design_requirement_id
        where td.project_id = ?1 and td.status = 'stale'
        order by td.id
        "#,
        &mut records,
    )?;
    collect_stale_rows(
        &conn,
        project_id,
        "checklist",
        r#"
        select c.id, c.title
        from checklists c
        where c.project_id = ?1 and c.status = 'stale'
        order by c.id
        "#,
        &mut records,
    )?;
    collect_stale_rows(
        &conn,
        project_id,
        "validation_gate",
        r#"
        select vg.id, vg.gate_key
        from validation_gates vg
        where vg.project_id = ?1 and vg.status = 'stale'
        order by vg.id
        "#,
        &mut records,
    )?;
    collect_stale_rows(
        &conn,
        project_id,
        "coverage_item",
        r#"
        select c.id, dr.requirement_key
        from coverage_items c
        join design_requirements dr on dr.id = c.design_requirement_id
        where c.project_id = ?1 and c.status = 'stale'
        order by c.id
        "#,
        &mut records,
    )?;
    collect_stale_rows(
        &conn,
        project_id,
        "review_plan",
        r#"
        select rp.id, rp.review_type || ':' || rp.stage
        from review_plans rp
        join design_versions v on v.id = rp.design_version_id
        join design_packages p on p.id = v.design_package_id
        where rp.project_id = ?1
          and rp.status = 'blocked'
          and p.current_design_version_id != rp.design_version_id
        order by rp.id
        "#,
        &mut records,
    )?;
    Ok(records)
}

pub fn list_validation_gate_context(
    root: &Path,
    input: ValidationGateContextQuery,
) -> Result<Vec<ValidationGateContextRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select
            vg.id, vg.gate_key, r.requirement_key, vg.task_id, vg.status,
            latest.id, latest.command_usage_id, latest.repository_snapshot_id,
            latest.result, latest.artifact_path, latest.notes
        from validation_gates vg
        join design_requirements r on r.id = vg.design_requirement_id
        left join tasks t on t.id = vg.task_id
        left join validation_runs latest on latest.id = (
            select vr.id
            from validation_runs vr
            where vr.validation_gate_id = vg.id
            order by vr.id desc
            limit 1
        )
        where vg.project_id = ?1
          and r.design_version_id = ?2
          and (?3 is null or coalesce(vg.work_unit_id, t.work_unit_id) = ?3)
        order by r.requirement_key, vg.id
        "#,
    )?;
    let rows = stmt.query_map(
        params![project_id, input.design_version_id, input.work_unit_id],
        |row| {
            Ok(ValidationGateContextRecord {
                id: row.get(0)?,
                gate_key: row.get(1)?,
                requirement_key: row.get(2)?,
                task_id: row.get(3)?,
                status: row.get(4)?,
                latest_run_id: row.get(5)?,
                latest_command_usage_id: row.get(6)?,
                latest_repository_snapshot_id: row.get(7)?,
                latest_result: row.get(8)?,
                latest_artifact_path: row.get(9)?,
                latest_notes: row.get(10)?,
            })
        },
    )?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

fn collect_stale_rows(
    conn: &rusqlite::Connection,
    project_id: i64,
    record_type: &str,
    sql: &str,
    records: &mut Vec<StaleRecord>,
) -> Result<()> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![project_id], |row| {
        Ok(StaleRecord {
            record_type: record_type.to_string(),
            id: row.get(0)?,
            label: row.get(1)?,
        })
    })?;
    for row in rows {
        records.push(row?);
    }
    Ok(())
}

fn ensure_design_ready_for_decomposition(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_version_id: i64,
) -> Result<()> {
    let ready: bool = conn.query_row(
        r#"
        select exists(
            select 1
            from review_plans p
            where p.project_id = ?1
              and p.design_version_id = ?2
              and p.review_type = 'design_review'
              and p.stage = 'design-ready'
              and p.required = 1
              and p.status = 'clean'
        )
        "#,
        params![project_id, design_version_id],
        |row| row.get(0),
    )?;
    if !ready {
        bail!("design decomposition requires a clean design-ready review plan");
    }
    Ok(())
}

fn first_line(text: &str) -> &str {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .unwrap_or("design requirement")
}

pub fn list_task_derivations(
    root: &Path,
    input: TaskDerivationListQuery,
) -> Result<Vec<TaskDerivationRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select
            td.id, r.requirement_key, td.task_id, t.title,
            td.checklist_item_id, ci.title, td.status
        from task_derivations td
        join design_requirements r on r.id = td.design_requirement_id
        join tasks t on t.id = td.task_id
        left join checklist_items ci on ci.id = td.checklist_item_id
        where td.project_id = ?1
          and r.design_version_id = ?2
          and (?3 is null or t.work_unit_id = ?3)
        order by r.requirement_key, td.id
        "#,
    )?;
    let rows = stmt.query_map(
        params![project_id, input.design_version_id, input.work_unit_id],
        |row| {
            Ok(TaskDerivationRecord {
                id: row.get(0)?,
                requirement_key: row.get(1)?,
                task_id: row.get(2)?,
                task_title: row.get(3)?,
                checklist_item_id: row.get(4)?,
                checklist_item_title: row.get(5)?,
                status: row.get(6)?,
            })
        },
    )?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

pub fn add_implementation_evidence(
    root: &Path,
    input: NewImplementationEvidence<'_>,
) -> Result<ImplementationEvidenceOutcome> {
    insert_implementation_evidence(
        root,
        ImplementationEvidenceInput {
            task_id: input.task_id,
            design_version_id: input.design_version_id,
            requirement_key: input.requirement_key,
            evidence_type: input.evidence_type,
            repository_id: None,
            git_commit_id: None,
            git_file_change_id: None,
            commit_sha: input.commit_sha,
            file_path: input.file_path,
            line_ref: input.line_ref,
            symbol: input.symbol,
            artifact_path: input.artifact_path,
            note: input.note,
        },
    )
}

pub fn add_implementation_evidence_with_git(
    root: &Path,
    input: NewImplementationEvidenceWithGit<'_>,
) -> Result<ImplementationEvidenceOutcome> {
    insert_implementation_evidence(
        root,
        ImplementationEvidenceInput {
            task_id: input.task_id,
            design_version_id: input.design_version_id,
            requirement_key: input.requirement_key,
            evidence_type: input.evidence_type,
            repository_id: input.repository_id,
            git_commit_id: input.git_commit_id,
            git_file_change_id: input.git_file_change_id,
            commit_sha: input.commit_sha,
            file_path: input.file_path,
            line_ref: input.line_ref,
            symbol: input.symbol,
            artifact_path: input.artifact_path,
            note: input.note,
        },
    )
}

fn insert_implementation_evidence(
    root: &Path,
    input: ImplementationEvidenceInput<'_>,
) -> Result<ImplementationEvidenceOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let task_id = match input.task_id {
        Some(id) => {
            tx.query_row(
                r#"
                select 1
                from tasks t
                join work_units w on w.id = t.work_unit_id
                where t.id = ?1 and w.project_id = ?2
                "#,
                params![id, project_id],
                |_| Ok(()),
            )
            .optional()?
            .context("implementation evidence task must belong to a work unit in this project")?;
            Some(id)
        }
        None => None,
    };
    let design_requirement_id = match (input.design_version_id, input.requirement_key) {
        (Some(design_version_id), Some(requirement_key)) => Some(
            tx.query_row(
                r#"
                select id
                from design_requirements
                where project_id = ?1
                  and design_version_id = ?2
                  and requirement_key = ?3
                  and status = 'active'
                "#,
                params![project_id, design_version_id, requirement_key],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .context("active design requirement not found")?,
        ),
        (None, None) => None,
        _ => bail!("--design and --requirement must be provided together"),
    };
    if task_id.is_none() && design_requirement_id.is_none() {
        bail!("implementation evidence requires --task or --design with --requirement");
    }
    if let Some(task_id) = task_id
        && design_requirement_id.is_none()
        && task_has_active_design_derivation(&tx, task_id)?
    {
        bail!("design-derived implementation evidence requires --design and --requirement");
    }
    if let (Some(task_id), Some(design_requirement_id)) = (task_id, design_requirement_id) {
        require_task_derivation(&tx, design_requirement_id, task_id)?;
    }
    let git = resolve_git_evidence(
        &tx,
        project_id,
        input.repository_id,
        input.git_commit_id,
        input.git_file_change_id,
        input.commit_sha,
        input.file_path,
    )?;
    let has_evidence_reference = git.git_commit_id.is_some()
        || git.git_file_change_id.is_some()
        || git.commit_sha.is_some()
        || git.file_path.is_some()
        || input.symbol.is_some_and(|value| !value.trim().is_empty())
        || input
            .artifact_path
            .is_some_and(|value| !value.trim().is_empty());
    if !has_evidence_reference {
        bail!("implementation evidence requires commit, file, symbol, or artifact reference");
    }

    tx.execute(
        r#"
        insert into implementation_evidence(
            project_id, task_id, design_requirement_id, evidence_type,
            repository_id, git_commit_id, git_file_change_id, commit_sha,
            file_path, line_ref, symbol, artifact_path, note, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, current_timestamp)
        "#,
        params![
            project_id,
            task_id,
            design_requirement_id,
            input.evidence_type,
            git.repository_id,
            git.git_commit_id,
            git.git_file_change_id,
            git.commit_sha,
            git.file_path,
            input.line_ref,
            input.symbol,
            input.artifact_path,
            input.note,
        ],
    )?;
    let implementation_evidence_id = tx.last_insert_rowid();
    tx.commit()?;

    Ok(ImplementationEvidenceOutcome {
        implementation_evidence_id,
        task_id,
        design_requirement_id,
    })
}

pub fn list_implementation_evidence(
    root: &Path,
    input: ImplementationEvidenceListQuery,
) -> Result<Vec<ImplementationEvidenceRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select
            e.id, e.task_id, r.requirement_key, e.evidence_type,
            e.commit_sha, e.file_path, e.line_ref, e.symbol, e.artifact_path, e.note
        from implementation_evidence e
        left join design_requirements r on r.id = e.design_requirement_id
        left join tasks t on t.id = e.task_id
        where e.project_id = ?1
          and (?2 is null or e.task_id = ?2)
          and (?3 is null or r.design_version_id = ?3)
          and (
            ?4 is null
            or t.work_unit_id = ?4
            or (
              e.task_id is null
              and exists (
                select 1
                from task_derivations td
                join tasks dt on dt.id = td.task_id
                where td.design_requirement_id = e.design_requirement_id
                  and td.status = 'active'
                  and dt.work_unit_id = ?4
              )
            )
          )
        order by e.id
        "#,
    )?;
    let rows = stmt.query_map(
        params![
            project_id,
            input.task_id,
            input.design_version_id,
            input.work_unit_id
        ],
        |row| {
            Ok(ImplementationEvidenceRecord {
                id: row.get(0)?,
                task_id: row.get(1)?,
                requirement_key: row.get(2)?,
                evidence_type: row.get(3)?,
                commit_sha: row.get(4)?,
                file_path: row.get(5)?,
                line_ref: row.get(6)?,
                symbol: row.get(7)?,
                artifact_path: row.get(8)?,
                note: row.get(9)?,
            })
        },
    )?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

fn resolve_git_evidence(
    conn: &rusqlite::Connection,
    project_id: i64,
    repository_id: Option<i64>,
    git_commit_id: Option<i64>,
    git_file_change_id: Option<i64>,
    commit_sha: Option<&str>,
    file_path: Option<&str>,
) -> Result<ResolvedGitEvidence> {
    let mut resolved_repository_id = repository_id;
    let mut resolved_commit_sha = commit_sha.map(str::to_string);
    let mut resolved_file_path = file_path.map(str::to_string);

    if let Some(repository_id) = resolved_repository_id {
        conn.query_row(
            "select 1 from repositories where id = ?1 and project_id = ?2",
            params![repository_id, project_id],
            |_| Ok(()),
        )
        .optional()?
        .context("repository not found")?;
    }

    if let Some(git_commit_id) = git_commit_id {
        let commit = conn
            .query_row(
                r#"
                select c.repository_id, c.commit_sha
                from git_commits c
                join repositories r on r.id = c.repository_id
                where c.id = ?1 and r.project_id = ?2
                "#,
                params![git_commit_id, project_id],
                |row| {
                    Ok(ResolvedCommit {
                        repository_id: row.get(0)?,
                        commit_sha: row.get(1)?,
                    })
                },
            )
            .optional()?
            .context("git commit not found")?;
        if let Some(existing_repository_id) = resolved_repository_id
            && existing_repository_id != commit.repository_id
        {
            bail!("implementation evidence repository must match git commit");
        }
        if let Some(existing_commit_sha) = &resolved_commit_sha
            && existing_commit_sha != &commit.commit_sha
        {
            bail!("implementation evidence commit sha must match git commit");
        }
        resolved_repository_id = Some(commit.repository_id);
        resolved_commit_sha = Some(commit.commit_sha);
    }

    if let Some(git_file_change_id) = git_file_change_id {
        let file = conn
            .query_row(
                r#"
                select f.repository_id, f.git_commit_id, f.path, c.commit_sha
                from git_file_changes f
                join git_commits c on c.id = f.git_commit_id
                join repositories r on r.id = f.repository_id
                where f.id = ?1 and r.project_id = ?2
                "#,
                params![git_file_change_id, project_id],
                |row| {
                    Ok(ResolvedFileChange {
                        repository_id: row.get(0)?,
                        git_commit_id: row.get(1)?,
                        path: row.get(2)?,
                        commit_sha: row.get(3)?,
                    })
                },
            )
            .optional()?
            .context("git file change not found")?;
        if let Some(existing_repository_id) = resolved_repository_id
            && existing_repository_id != file.repository_id
        {
            bail!("implementation evidence repository must match git file change");
        }
        if let Some(existing_git_commit_id) = git_commit_id
            && existing_git_commit_id != file.git_commit_id
        {
            bail!("implementation evidence git file change must match git commit");
        }
        if let Some(existing_commit_sha) = &resolved_commit_sha
            && existing_commit_sha != &file.commit_sha
        {
            bail!("implementation evidence commit sha must match git file change");
        }
        if let Some(existing_file_path) = &resolved_file_path
            && existing_file_path != &file.path
        {
            bail!("implementation evidence file path must match git file change");
        }
        resolved_repository_id = Some(file.repository_id);
        resolved_commit_sha = Some(file.commit_sha);
        resolved_file_path = Some(file.path);
    }

    Ok(ResolvedGitEvidence {
        repository_id: resolved_repository_id,
        git_commit_id,
        git_file_change_id,
        commit_sha: resolved_commit_sha,
        file_path: resolved_file_path,
    })
}

pub fn select_validation_gate(
    root: &Path,
    input: ValidationGateSelection<'_>,
) -> Result<ValidationGateSelectionOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let template = tx
        .query_row(
            r#"
            select id, gate_key, command, expected_result
            from validation_gate_templates
            where project_id = ?1
              and design_version_id = ?2
              and gate_key = ?3
              and status = 'active'
            "#,
            params![project_id, input.design_version_id, input.gate_key],
            |row| {
                Ok(ResolvedGateTemplate {
                    id: row.get(0)?,
                    gate_key: row.get(1)?,
                    command: row.get(2)?,
                    expected_result: row.get(3)?,
                })
            },
        )
        .optional()?
        .context("active validation gate template not found")?;
    let requirement_id: i64 = tx
        .query_row(
            r#"
            select r.id
            from design_requirements r
            join validation_gate_template_requirements gr
              on gr.design_requirement_id = r.id
            where r.project_id = ?1
              and r.design_version_id = ?2
              and r.requirement_key = ?3
              and gr.validation_gate_template_id = ?4
              and r.status = 'active'
            "#,
            params![
                project_id,
                input.design_version_id,
                input.requirement_key,
                template.id
            ],
            |row| row.get(0),
        )
        .optional()?
        .context("active requirement is not covered by the validation gate template")?;
    let task_work_unit_id: Option<i64> = tx
        .query_row(
            "select work_unit_id from tasks where id = ?1",
            params![input.task_id],
            |row| row.get(0),
        )
        .optional()?
        .context("task not found")?;
    require_task_derivation(&tx, requirement_id, input.task_id)?;
    let (command_profile_id, profile_command) = match input.command_profile {
        Some(profile) => {
            let (id, command): (i64, String) = tx
                .query_row(
                    r#"
                    select id, command
                    from command_profiles
                    where project_id = ?1
                      and (name = ?2 or cast(id as text) = ?2)
                      and status in ('candidate', 'preferred', 'fixed')
                    "#,
                    params![project_id, profile],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()?
                .context("command profile not found")?;
            (Some(id), Some(command))
        }
        None => (None, None),
    };
    let command = input
        .command
        .map(str::to_string)
        .or(profile_command)
        .or_else(|| template.command.clone());
    tx.execute(
        r#"
        insert into validation_gates(
            project_id, gate_key, template_id, work_unit_id, task_id,
            design_requirement_id, command_profile_id, command, expected_result, timeout,
            selected_before_edit, status, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1, 'active', current_timestamp)
        "#,
        params![
            project_id,
            template.gate_key,
            template.id,
            task_work_unit_id,
            input.task_id,
            requirement_id,
            command_profile_id,
            command,
            template.expected_result,
            input.timeout,
        ],
    )?;
    let validation_gate_id = tx.last_insert_rowid();
    if let Some(work_unit_id) = task_work_unit_id {
        let work_scope = work_unit_id.to_string();
        insert_rule_binding(
            &tx,
            RuleBindingInput {
                project_id,
                rule_source_type: "validation_gate",
                authority_event_id: None,
                user_correction_id: None,
                command_profile_id: None,
                review_policy_id: None,
                review_plan_id: None,
                work_unit_id: Some(work_unit_id),
                validation_gate_id: Some(validation_gate_id),
                acceptance_record_id: None,
                scope_type: "work_unit",
                scope_key: Some(&work_scope),
                precedence: 62,
            },
        )?;
    }
    tx.commit()?;

    Ok(ValidationGateSelectionOutcome {
        validation_gate_id,
        validation_gate_template_id: template.id,
        design_requirement_id: requirement_id,
        task_id: input.task_id,
    })
}

pub fn add_validation_run(
    root: &Path,
    input: NewValidationRun<'_>,
) -> Result<ValidationRunOutcome> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let gate = conn
        .query_row(
            r#"
            select work_unit_id, task_id
            from validation_gates
            where id = ?1 and project_id = ?2 and status = 'active'
            "#,
            params![input.validation_gate_id, project_id],
            |row| {
                Ok(ResolvedValidationGate {
                    work_unit_id: row.get(0)?,
                    task_id: row.get(1)?,
                })
            },
        )
        .optional()?
        .context("active validation gate not found")?;
    if let Some(command_usage_id) = input.command_usage_id {
        conn.query_row(
            r#"
            select 1
            from command_usages
            where id = ?1
              and project_id = ?2
              and (
                  work_unit_id is null
                  or work_unit_id is ?3
              )
            "#,
            params![command_usage_id, project_id, gate.work_unit_id],
            |_| Ok(()),
        )
        .optional()?
        .context("command usage not found for validation gate scope")?;
    }
    if let Some(repository_snapshot_id) = input.repository_snapshot_id {
        conn.query_row(
            r#"
            select 1
            from repository_snapshots s
            join repositories r on r.id = s.repository_id
            where s.id = ?1 and r.project_id = ?2
            "#,
            params![repository_snapshot_id, project_id],
            |_| Ok(()),
        )
        .optional()?
        .context("repository snapshot not found")?;
    }
    if let Some(acceptance_record_id) = input.acceptance_record_id {
        conn.query_row(
            "select 1 from acceptance_records where id = ?1 and project_id = ?2 and status = 'approved'",
            params![acceptance_record_id, project_id],
            |_| Ok(()),
        )
        .optional()?
        .context("approved acceptance record not found")?;
    }

    conn.execute(
        r#"
        insert into validation_runs(
            project_id, validation_gate_id, work_unit_id, task_id,
            command_usage_id, repository_snapshot_id, result,
            command, classification, acceptance_record_id,
            artifact_path, artifact_hash, notes, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, current_timestamp)
        "#,
        params![
            project_id,
            input.validation_gate_id,
            gate.work_unit_id,
            gate.task_id,
            input.command_usage_id,
            input.repository_snapshot_id,
            input.result,
            input.command,
            input.classification,
            input.acceptance_record_id,
            input.artifact_path,
            input.artifact_hash,
            input.notes,
        ],
    )?;
    let validation_run_id = conn.last_insert_rowid();
    if input.artifact_path.is_some() || input.artifact_hash.is_some() {
        let identity_key = input
            .artifact_hash
            .or(input.artifact_path)
            .context("artifact identity requires a path or hash")?;
        conn.execute(
            r#"
            insert into artifacts(
                project_id, artifact_type, identity_key, artifact_path,
                artifact_hash, validation_run_id, command_usage_id,
                repository_snapshot_id, created_at
            )
            values (?1, 'validation_output', ?2, ?3, ?4, ?5, ?6, ?7, current_timestamp)
            "#,
            params![
                project_id,
                identity_key,
                input.artifact_path,
                input.artifact_hash,
                validation_run_id,
                input.command_usage_id,
                input.repository_snapshot_id,
            ],
        )?;
    }

    Ok(ValidationRunOutcome {
        validation_run_id,
        validation_gate_id: input.validation_gate_id,
        work_unit_id: gate.work_unit_id,
        task_id: gate.task_id,
    })
}

pub fn list_validation_runs(
    root: &Path,
    input: ValidationRunListQuery,
) -> Result<Vec<ValidationRunRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select
            vr.id, vr.validation_gate_id, vg.gate_key, vr.work_unit_id,
            vr.task_id, vr.command_usage_id, vr.repository_snapshot_id,
            vr.result, vr.command, vr.classification, vr.acceptance_record_id,
            vr.artifact_path, vr.artifact_hash, vr.notes, vr.created_at
        from validation_runs vr
        join validation_gates vg on vg.id = vr.validation_gate_id
        where vr.project_id = ?1
          and (?2 is null or vr.validation_gate_id = ?2)
        order by vr.id
        "#,
    )?;
    let rows = stmt.query_map(params![project_id, input.validation_gate_id], |row| {
        Ok(ValidationRunRecord {
            id: row.get(0)?,
            validation_gate_id: row.get(1)?,
            gate_key: row.get(2)?,
            work_unit_id: row.get(3)?,
            task_id: row.get(4)?,
            command_usage_id: row.get(5)?,
            repository_snapshot_id: row.get(6)?,
            result: row.get(7)?,
            command: row.get(8)?,
            classification: row.get(9)?,
            acceptance_record_id: row.get(10)?,
            artifact_path: row.get(11)?,
            artifact_hash: row.get(12)?,
            notes: row.get(13)?,
            created_at: row.get(14)?,
        })
    })?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

pub fn implementation_ready(
    root: &Path,
    input: ImplementationReadyCheck,
) -> Result<ImplementationReadyOutcome> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut items = Vec::new();
    let Some(version) = resolve_design_version(&conn, project_id, input.design_version_id)? else {
        items.push(ImplementationReadyItem::fail(
            "design_version_exists",
            Some("import a design package first".to_string()),
        ));
        return Ok(ImplementationReadyOutcome::blocked(
            input.design_version_id,
            None,
            "no design version is available",
            items,
        ));
    };
    items.push(ImplementationReadyItem::pass("design_version_exists", None));

    if version.current_design_version_id == Some(version.design_version_id) {
        items.push(ImplementationReadyItem::pass(
            "design_version_current",
            None,
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "design_version_current",
            Some("import or select the current design version".to_string()),
        ));
    }

    if version.status == "approved" && version.approved_by_authority_event_id.is_some() {
        items.push(ImplementationReadyItem::pass(
            "design_version_approved",
            None,
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "design_version_approved",
            Some("approve the design version before implementation starts".to_string()),
        ));
    }

    let missing_derivation_count = count_missing_derivations(&conn, version.design_version_id)?;
    if missing_derivation_count == 0 {
        items.push(ImplementationReadyItem::pass(
            "task_derivations_exist",
            None,
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "task_derivations_exist",
            Some(format!(
                "{missing_derivation_count} active requirements have no task derivation"
            )),
        ));
    }

    let stale_derivation_count = count_stale_task_derivations(&conn, version.design_package_id)?;
    if stale_derivation_count == 0 {
        items.push(ImplementationReadyItem::pass(
            "task_derivations_current",
            None,
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "task_derivations_current",
            Some(format!(
                "{stale_derivation_count} task derivations are stale"
            )),
        ));
    }

    let stale_checklist_count = count_stale_checklists(&conn, version.design_package_id)?;
    if stale_checklist_count == 0 {
        items.push(ImplementationReadyItem::pass("checklists_current", None));
    } else {
        items.push(ImplementationReadyItem::fail(
            "checklists_current",
            Some(format!("{stale_checklist_count} checklists are stale")),
        ));
    }

    let stale_validation_gate_count =
        count_stale_validation_gates(&conn, version.design_package_id)?;
    if stale_validation_gate_count == 0 {
        items.push(ImplementationReadyItem::pass(
            "validation_gates_current",
            None,
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "validation_gates_current",
            Some(format!(
                "{stale_validation_gate_count} validation gates are stale"
            )),
        ));
    }

    let stale_coverage_count = count_stale_coverage_items(&conn, version.design_package_id)?;
    if stale_coverage_count == 0 {
        items.push(ImplementationReadyItem::pass(
            "coverage_items_current",
            None,
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "coverage_items_current",
            Some(format!("{stale_coverage_count} coverage items are stale")),
        ));
    }

    let missing_validation_count =
        count_missing_validation_links(&conn, version.design_version_id)?;
    if missing_validation_count == 0 {
        items.push(ImplementationReadyItem::pass(
            "validation_expectations_linked",
            None,
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "validation_expectations_linked",
            Some(format!(
                "{missing_validation_count} active requirements have no linked validation template"
            )),
        ));
    }

    let missing_selected_gate_count =
        count_missing_selected_gates(&conn, version.design_version_id)?;
    if missing_selected_gate_count == 0 {
        items.push(ImplementationReadyItem::pass(
            "validation_gates_selected",
            None,
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "validation_gates_selected",
            Some(format!(
                "{missing_selected_gate_count} active task derivations have no selected validation gate"
            )),
        ));
    }

    let missing_completion_condition_count =
        count_missing_completion_conditions(&conn, version.design_version_id)?;
    if missing_completion_condition_count == 0 {
        items.push(ImplementationReadyItem::pass(
            "completion_conditions_present",
            None,
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "completion_conditions_present",
            Some(format!(
                "{missing_completion_condition_count} active task derivations have no completion condition"
            )),
        ));
    }

    let missing_evidence_count =
        count_closed_derived_tasks_missing_evidence(&conn, version.design_version_id)?;
    if missing_evidence_count == 0 {
        items.push(ImplementationReadyItem::pass(
            "implementation_evidence_present",
            None,
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "implementation_evidence_present",
            Some(format!(
                "{missing_evidence_count} closed design-derived tasks have no implementation evidence"
            )),
        ));
    }

    let missing_coverage_count =
        count_closed_derived_tasks_missing_coverage(&conn, version.design_version_id)?;
    if missing_coverage_count == 0 {
        items.push(ImplementationReadyItem::pass(
            "coverage_items_present",
            None,
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "coverage_items_present",
            Some(format!(
                "{missing_coverage_count} closed design-derived tasks have no covered coverage item"
            )),
        ));
    }

    let review_state =
        implementation_review_gate_state(&conn, project_id, version.design_version_id)?;
    let decomposition_review_plan_count = required_review_plan_count(
        &conn,
        project_id,
        version.design_version_id,
        "implementation-ready",
        "design_task_decomposition",
    )?;
    if decomposition_review_plan_count == 0 {
        items.push(ImplementationReadyItem::fail(
            "pre_implementation_reviews_clean",
            Some(
                "add a required implementation-ready design_task_decomposition review plan for this design version",
            ),
        ));
    } else if review_state.incomplete_required_plan_count == 0
        && review_state.missing_context_run_count == 0
        && review_state.unresolved_finding_count == 0
    {
        items.push(ImplementationReadyItem::pass(
            "pre_implementation_reviews_clean",
            Some(format!(
                "{} required plans, {} missing review-context runs, {} unresolved findings",
                review_state.required_plan_count,
                review_state.missing_context_run_count,
                review_state.unresolved_finding_count
            )),
        ));
    } else {
        items.push(ImplementationReadyItem::fail(
            "pre_implementation_reviews_clean",
            Some(format!(
                "{} required plans, {} incomplete, {} missing review-context runs, {} unresolved findings",
                review_state.required_plan_count,
                review_state.incomplete_required_plan_count,
                review_state.missing_context_run_count,
                review_state.unresolved_finding_count
            )),
        ));
    }

    let result = if items.iter().all(|item| item.result == "pass") {
        "pass"
    } else {
        "blocked"
    };
    let blocking_reason = if result == "pass" {
        None
    } else {
        Some("implementation prerequisites are not ready".to_string())
    };

    Ok(ImplementationReadyOutcome {
        result: result.to_string(),
        blocking_reason,
        design_package_id: Some(version.design_package_id),
        design_version_id: Some(version.design_version_id),
        items,
    })
}

fn required_review_plan_count(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_version_id: i64,
    stage: &str,
    review_type: &str,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from review_plans
        where project_id = ?1
          and design_version_id = ?2
          and stage = ?3
          and review_type = ?4
          and required = 1
        "#,
        params![project_id, design_version_id, stage, review_type],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn implementation_review_gate_state(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_version_id: i64,
) -> Result<ReviewGateState> {
    let required_plan_count = conn.query_row(
        r#"
        select count(*)
        from review_plans
        where project_id = ?1
          and design_version_id = ?2
          and stage = 'implementation-ready'
          and required = 1
        "#,
        params![project_id, design_version_id],
        |row| row.get::<_, i64>(0),
    )?;
    let incomplete_required_plan_count = conn.query_row(
        r#"
        select count(*)
        from review_plans
        where project_id = ?1
          and design_version_id = ?2
          and stage = 'implementation-ready'
          and required = 1
          and status != 'clean'
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'review_plan'
              and ar.review_plan_id = review_plans.id
              and ar.status = 'approved'
              and ar.acceptance_type in ('explicit_exception', 'stale_accepted')
          )
        "#,
        params![project_id, design_version_id],
        |row| row.get::<_, i64>(0),
    )?;
    let unresolved_finding_count = conn.query_row(
        r#"
        select count(*)
        from findings f
        join review_runs rr on rr.id = f.review_run_id
        join review_plans rp on rp.id = rr.review_plan_id
        where rp.project_id = ?1
          and rp.design_version_id = ?2
          and rp.stage in ('design-ready', 'implementation-ready')
          and f.finding_type in ('design_finding', 'design_task_gap')
          and f.status not in ('closed', 'accepted_out_of_scope')
          and f.classification not in ('invalid')
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'finding'
              and ar.finding_id = f.id
              and ar.status = 'approved'
              and ar.acceptance_type in (
                'accepted_out_of_scope', 'explicit_exception', 'classified_failure'
              )
          )
        "#,
        params![project_id, design_version_id],
        |row| row.get::<_, i64>(0),
    )?;
    let missing_context_run_count = required_plans_missing_context_count(
        conn,
        project_id,
        "implementation-ready",
        "design_task_decomposition",
        Some(design_version_id),
        None,
        "design-task-decomposition",
    )?;
    Ok(ReviewGateState {
        required_plan_count,
        incomplete_required_plan_count,
        missing_context_run_count,
        unresolved_finding_count,
    })
}

#[derive(Default)]
struct ReviewGateState {
    required_plan_count: i64,
    incomplete_required_plan_count: i64,
    missing_context_run_count: i64,
    unresolved_finding_count: i64,
}

fn require_task_derivation(
    conn: &rusqlite::Connection,
    design_requirement_id: i64,
    task_id: i64,
) -> Result<()> {
    let exists = conn
        .query_row(
            r#"
            select 1
            from task_derivations
            where design_requirement_id = ?1
              and task_id = ?2
              and status = 'active'
            "#,
            params![design_requirement_id, task_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some();
    if !exists {
        bail!("task is not actively derived from the design requirement");
    }
    Ok(())
}

fn task_has_active_design_derivation(conn: &rusqlite::Connection, task_id: i64) -> Result<bool> {
    conn.query_row(
        r#"
        select exists (
            select 1
            from task_derivations
            where task_id = ?1
              and status = 'active'
        )
        "#,
        params![task_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn get_or_create_checklist(
    conn: &rusqlite::Connection,
    project_id: i64,
    work_unit_id: i64,
    design_version_id: i64,
    title: &str,
) -> Result<i64> {
    if let Some(id) = conn
        .query_row(
            r#"
            select id
            from checklists
            where project_id = ?1
              and work_unit_id = ?2
              and design_version_id = ?3
              and title = ?4
              and status = 'active'
            order by id desc
            limit 1
            "#,
            params![project_id, work_unit_id, design_version_id, title],
            |row| row.get(0),
        )
        .optional()?
    {
        return Ok(id);
    }
    conn.execute(
        r#"
        insert into checklists(
            project_id, work_unit_id, design_version_id, title, status, created_at
        )
        values (?1, ?2, ?3, ?4, 'active', current_timestamp)
        "#,
        params![project_id, work_unit_id, design_version_id, title],
    )?;
    Ok(conn.last_insert_rowid())
}

fn resolve_design_version(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_version_id: Option<i64>,
) -> Result<Option<ResolvedDesignVersion>> {
    match design_version_id {
        Some(id) => conn
            .query_row(
                r#"
                select
                    v.id, v.design_package_id, v.status,
                    v.approved_by_authority_event_id, p.current_design_version_id
                from design_versions v
                join design_packages p on p.id = v.design_package_id
                where v.project_id = ?1 and v.id = ?2
                "#,
                params![project_id, id],
                resolved_design_version,
            )
            .optional()
            .map_err(Into::into),
        None => {
            let current_count: i64 = conn.query_row(
                "select count(*) from design_packages where project_id = ?1 and current_design_version_id is not null",
                params![project_id],
                |row| row.get(0),
            )?;
            if current_count != 1 {
                return Ok(None);
            }
            conn.query_row(
                r#"
                select
                    v.id, v.design_package_id, v.status,
                    v.approved_by_authority_event_id, p.current_design_version_id
                from design_packages p
                join design_versions v on v.id = p.current_design_version_id
                where p.project_id = ?1
                "#,
                params![project_id],
                resolved_design_version,
            )
            .optional()
            .map_err(Into::into)
        }
    }
}

fn count_missing_derivations(conn: &rusqlite::Connection, design_version_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from design_requirements r
        where r.design_version_id = ?1
          and r.status = 'active'
          and not exists (
            select 1
            from task_derivations td
            where td.design_requirement_id = r.id
              and td.status = 'active'
          )
        "#,
        params![design_version_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_stale_task_derivations(
    conn: &rusqlite::Connection,
    design_package_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join design_requirements r on r.id = td.design_requirement_id
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where p.id = ?1
          and td.status in ('active', 'stale')
          and p.current_design_version_id != r.design_version_id
          and not exists (
            select 1
            from design_requirements current_r
            where current_r.design_version_id = p.current_design_version_id
              and current_r.requirement_key = r.requirement_key
              and current_r.requirement_hash = r.requirement_hash
              and current_r.status = 'active'
          )
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'stale_record'
              and ar.stale_record_type = 'task_derivation'
              and ar.stale_record_id = td.id
              and ar.acceptance_type = 'stale_accepted'
              and ar.status = 'approved'
          )
        "#,
        params![design_package_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_stale_checklists(conn: &rusqlite::Connection, design_package_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        select count(distinct c.id)
        from checklists c
        join checklist_items ci on ci.checklist_id = c.id
        join design_requirements r on r.id = ci.design_requirement_id
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where p.id = ?1
          and c.status in ('active', 'stale')
          and p.current_design_version_id != r.design_version_id
          and not exists (
            select 1
            from design_requirements current_r
            where current_r.design_version_id = p.current_design_version_id
              and current_r.requirement_key = r.requirement_key
              and current_r.requirement_hash = r.requirement_hash
              and current_r.status = 'active'
          )
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'stale_record'
              and ar.stale_record_type = 'checklist'
              and ar.stale_record_id = c.id
              and ar.acceptance_type = 'stale_accepted'
              and ar.status = 'approved'
          )
        "#,
        params![design_package_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_stale_validation_gates(
    conn: &rusqlite::Connection,
    design_package_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from validation_gates vg
        join validation_gate_templates gt on gt.id = vg.template_id
        join design_requirements r on r.id = vg.design_requirement_id
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where p.id = ?1
          and vg.status in ('active', 'stale')
          and (p.current_design_version_id != r.design_version_id
               or p.current_design_version_id != gt.design_version_id)
          and (
            not exists (
              select 1
              from design_requirements current_r
              where current_r.design_version_id = p.current_design_version_id
                and current_r.requirement_key = r.requirement_key
                and current_r.requirement_hash = r.requirement_hash
                and current_r.status = 'active'
            )
            or not exists (
              select 1
              from validation_gate_templates current_gt
              where current_gt.design_version_id = p.current_design_version_id
                and current_gt.gate_key = gt.gate_key
                and current_gt.gate_hash = gt.gate_hash
                and current_gt.status = 'active'
            )
          )
          and not exists (
            select 1
            from acceptance_records ar
            where (
                (
                  ar.target_type = 'validation_gate'
                  and ar.validation_gate_id = vg.id
                )
                or (
                  ar.target_type = 'stale_record'
                  and ar.stale_record_type = 'validation_gate'
                  and ar.stale_record_id = vg.id
                )
              )
              and ar.acceptance_type = 'stale_accepted'
              and ar.status = 'approved'
          )
        "#,
        params![design_package_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_stale_coverage_items(conn: &rusqlite::Connection, design_package_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from coverage_items c
        join design_requirements r on r.id = c.design_requirement_id
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where p.id = ?1
          and p.current_design_version_id != r.design_version_id
          and not exists (
            select 1
            from design_requirements current_r
            where current_r.design_version_id = p.current_design_version_id
              and current_r.requirement_key = r.requirement_key
              and current_r.requirement_hash = r.requirement_hash
              and current_r.status = 'active'
          )
          and not exists (
            select 1
            from acceptance_records ar
            where (
                (
                  ar.target_type = 'coverage_item'
                  and ar.coverage_item_id = c.id
                )
                or (
                  ar.target_type = 'stale_record'
                  and ar.stale_record_type = 'coverage_item'
                  and ar.stale_record_id = c.id
                )
              )
              and ar.acceptance_type = 'stale_accepted'
              and ar.status = 'approved'
          )
        "#,
        params![design_package_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_missing_validation_links(
    conn: &rusqlite::Connection,
    design_version_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from design_requirements r
        where r.design_version_id = ?1
          and r.status = 'active'
          and (r.validation_expectation is not null and r.validation_expectation != '')
          and not exists (
            select 1
            from validation_gate_template_requirements gr
            join validation_gate_templates g on g.id = gr.validation_gate_template_id
            where gr.design_requirement_id = r.id
              and g.status = 'active'
          )
        "#,
        params![design_version_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_missing_selected_gates(
    conn: &rusqlite::Connection,
    design_version_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join design_requirements r on r.id = td.design_requirement_id
        where r.design_version_id = ?1
          and r.status = 'active'
          and td.status = 'active'
          and not exists (
            select 1
            from validation_gates vg
            where vg.design_requirement_id = r.id
              and vg.task_id = td.task_id
              and vg.selected_before_edit = 1
              and vg.status = 'active'
          )
        "#,
        params![design_version_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_missing_completion_conditions(
    conn: &rusqlite::Connection,
    design_version_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join design_requirements r on r.id = td.design_requirement_id
        join tasks t on t.id = td.task_id
        left join checklist_items ci on ci.id = td.checklist_item_id
        where r.design_version_id = ?1
          and r.status = 'active'
          and td.status = 'active'
          and coalesce(
            nullif(trim(ci.completion_condition), ''),
            nullif(trim(t.completion_condition), '')
          ) is null
        "#,
        params![design_version_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_closed_derived_tasks_missing_evidence(
    conn: &rusqlite::Connection,
    design_version_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join design_requirements r on r.id = td.design_requirement_id
        join tasks t on t.id = td.task_id
        where r.design_version_id = ?1
          and r.status = 'active'
          and td.status = 'active'
          and t.status = 'closed'
          and not exists (
            select 1
            from implementation_evidence e
            where e.task_id = td.task_id
              and (
                e.design_requirement_id = r.id
                or (
                  e.design_requirement_id is null
                  and not exists (
                    select 1
                    from task_derivations sibling
                    where sibling.task_id = td.task_id
                      and sibling.status = 'active'
                      and sibling.design_requirement_id != r.id
                  )
                )
              )
          )
        "#,
        params![design_version_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_closed_derived_tasks_missing_coverage(
    conn: &rusqlite::Connection,
    design_version_id: i64,
) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from task_derivations td
        join design_requirements r on r.id = td.design_requirement_id
        join tasks t on t.id = td.task_id
        where r.design_version_id = ?1
          and r.status = 'active'
          and td.status = 'active'
          and t.status = 'closed'
          and not exists (
            select 1
            from coverage_items c
            where c.design_requirement_id = r.id
              and (
                c.task_id = td.task_id
                or (c.task_id is null and c.work_unit_id = t.work_unit_id)
              )
              and (
                c.status = 'covered'
                or (
                  c.status = 'accepted_out_of_scope'
                  and exists (
                    select 1
                    from acceptance_records ar
                    where ar.target_type = 'coverage_item'
                      and ar.coverage_item_id = c.id
                      and ar.acceptance_type = 'accepted_out_of_scope'
                      and ar.status = 'approved'
                  )
                )
              )
          )
        "#,
        params![design_version_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn resolved_design_version(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResolvedDesignVersion> {
    Ok(ResolvedDesignVersion {
        design_version_id: row.get(0)?,
        design_package_id: row.get(1)?,
        status: row.get(2)?,
        approved_by_authority_event_id: row.get(3)?,
        current_design_version_id: row.get(4)?,
    })
}

struct ResolvedRequirement {
    id: i64,
    key: String,
}

struct RequirementForDecomposition {
    id: i64,
    key: String,
    text: String,
    priority: String,
}

struct ResolvedTask {
    id: i64,
    work_unit_id: Option<i64>,
    title: String,
    completion_condition: Option<String>,
}

struct ResolvedGateTemplate {
    id: i64,
    gate_key: String,
    command: Option<String>,
    expected_result: String,
}

struct ResolvedValidationGate {
    work_unit_id: Option<i64>,
    task_id: Option<i64>,
}

struct ResolvedDesignVersion {
    design_version_id: i64,
    design_package_id: i64,
    status: String,
    approved_by_authority_event_id: Option<i64>,
    current_design_version_id: Option<i64>,
}

pub struct NewTaskDerivation<'a> {
    pub design_version_id: i64,
    pub requirement_key: &'a str,
    pub task_id: i64,
    pub derivation_reason: Option<&'a str>,
    pub checklist_title: Option<&'a str>,
    pub item_title: Option<&'a str>,
    pub completion_condition: Option<&'a str>,
}

pub struct DesignDecomposition<'a> {
    pub design_version_id: i64,
    pub work_unit_id: i64,
    pub checklist_title: Option<&'a str>,
    pub reason: Option<&'a str>,
}

pub struct TaskDerivationListQuery {
    pub design_version_id: i64,
    pub work_unit_id: Option<i64>,
}

pub struct ImplementationReadyCheck {
    pub design_version_id: Option<i64>,
}

pub struct ValidationGateSelection<'a> {
    pub design_version_id: i64,
    pub gate_key: &'a str,
    pub requirement_key: &'a str,
    pub task_id: i64,
    pub command: Option<&'a str>,
    pub command_profile: Option<&'a str>,
    pub timeout: Option<&'a str>,
}

pub struct NewValidationRun<'a> {
    pub validation_gate_id: i64,
    pub command_usage_id: Option<i64>,
    pub repository_snapshot_id: Option<i64>,
    pub result: &'a str,
    pub command: Option<&'a str>,
    pub classification: Option<&'a str>,
    pub acceptance_record_id: Option<i64>,
    pub artifact_path: Option<&'a str>,
    pub artifact_hash: Option<&'a str>,
    pub notes: Option<&'a str>,
}

pub struct ValidationRunListQuery {
    pub validation_gate_id: Option<i64>,
}

pub struct NewImplementationEvidence<'a> {
    pub task_id: Option<i64>,
    pub design_version_id: Option<i64>,
    pub requirement_key: Option<&'a str>,
    pub evidence_type: &'a str,
    pub commit_sha: Option<&'a str>,
    pub file_path: Option<&'a str>,
    pub line_ref: Option<&'a str>,
    pub symbol: Option<&'a str>,
    pub artifact_path: Option<&'a str>,
    pub note: Option<&'a str>,
}

pub struct NewImplementationEvidenceWithGit<'a> {
    pub task_id: Option<i64>,
    pub design_version_id: Option<i64>,
    pub requirement_key: Option<&'a str>,
    pub evidence_type: &'a str,
    pub repository_id: Option<i64>,
    pub git_commit_id: Option<i64>,
    pub git_file_change_id: Option<i64>,
    pub commit_sha: Option<&'a str>,
    pub file_path: Option<&'a str>,
    pub line_ref: Option<&'a str>,
    pub symbol: Option<&'a str>,
    pub artifact_path: Option<&'a str>,
    pub note: Option<&'a str>,
}

struct ImplementationEvidenceInput<'a> {
    task_id: Option<i64>,
    design_version_id: Option<i64>,
    requirement_key: Option<&'a str>,
    evidence_type: &'a str,
    repository_id: Option<i64>,
    git_commit_id: Option<i64>,
    git_file_change_id: Option<i64>,
    commit_sha: Option<&'a str>,
    file_path: Option<&'a str>,
    line_ref: Option<&'a str>,
    symbol: Option<&'a str>,
    artifact_path: Option<&'a str>,
    note: Option<&'a str>,
}

struct ResolvedGitEvidence {
    repository_id: Option<i64>,
    git_commit_id: Option<i64>,
    git_file_change_id: Option<i64>,
    commit_sha: Option<String>,
    file_path: Option<String>,
}

struct ResolvedCommit {
    repository_id: i64,
    commit_sha: String,
}

struct ResolvedFileChange {
    repository_id: i64,
    git_commit_id: i64,
    path: String,
    commit_sha: String,
}

pub struct ImplementationEvidenceListQuery {
    pub task_id: Option<i64>,
    pub design_version_id: Option<i64>,
    pub work_unit_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskDerivationOutcome {
    pub task_derivation_id: i64,
    pub checklist_id: i64,
    pub checklist_item_id: i64,
    pub design_requirement_id: i64,
    pub task_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskDerivationRecord {
    pub id: i64,
    pub requirement_key: String,
    pub task_id: i64,
    pub task_title: String,
    pub checklist_item_id: Option<i64>,
    pub checklist_item_title: Option<String>,
    pub status: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DesignDecompositionOutcome {
    pub design_version_id: i64,
    pub work_unit_id: i64,
    pub checklist_id: i64,
    pub created_tasks: i64,
    pub created_derivations: i64,
    pub created_validation_gates: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ChecklistRecord {
    pub id: i64,
    pub work_unit_id: i64,
    pub design_version_id: i64,
    pub title: String,
    pub status: String,
    pub item_count: i64,
    pub closed_count: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct StaleRecord {
    pub record_type: String,
    pub id: i64,
    pub label: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ValidationGateContextQuery {
    pub design_version_id: i64,
    pub work_unit_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ValidationGateContextRecord {
    pub id: i64,
    pub gate_key: String,
    pub requirement_key: String,
    pub task_id: Option<i64>,
    pub status: String,
    pub latest_run_id: Option<i64>,
    pub latest_command_usage_id: Option<i64>,
    pub latest_repository_snapshot_id: Option<i64>,
    pub latest_result: Option<String>,
    pub latest_artifact_path: Option<String>,
    pub latest_notes: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ImplementationEvidenceOutcome {
    pub implementation_evidence_id: i64,
    pub task_id: Option<i64>,
    pub design_requirement_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ImplementationEvidenceRecord {
    pub id: i64,
    pub task_id: Option<i64>,
    pub requirement_key: Option<String>,
    pub evidence_type: String,
    pub commit_sha: Option<String>,
    pub file_path: Option<String>,
    pub line_ref: Option<String>,
    pub symbol: Option<String>,
    pub artifact_path: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ValidationGateSelectionOutcome {
    pub validation_gate_id: i64,
    pub validation_gate_template_id: i64,
    pub design_requirement_id: i64,
    pub task_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ValidationRunOutcome {
    pub validation_run_id: i64,
    pub validation_gate_id: i64,
    pub work_unit_id: Option<i64>,
    pub task_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ValidationRunRecord {
    pub id: i64,
    pub validation_gate_id: i64,
    pub gate_key: String,
    pub work_unit_id: Option<i64>,
    pub task_id: Option<i64>,
    pub command_usage_id: Option<i64>,
    pub repository_snapshot_id: Option<i64>,
    pub result: String,
    pub command: Option<String>,
    pub classification: Option<String>,
    pub acceptance_record_id: Option<i64>,
    pub artifact_path: Option<String>,
    pub artifact_hash: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ImplementationReadyOutcome {
    pub result: String,
    pub blocking_reason: Option<String>,
    pub design_package_id: Option<i64>,
    pub design_version_id: Option<i64>,
    pub items: Vec<ImplementationReadyItem>,
}

impl ImplementationReadyOutcome {
    fn blocked(
        requested_design_version_id: Option<i64>,
        design_package_id: Option<i64>,
        reason: &str,
        items: Vec<ImplementationReadyItem>,
    ) -> Self {
        Self {
            result: "blocked".to_string(),
            blocking_reason: Some(reason.to_string()),
            design_package_id,
            design_version_id: requested_design_version_id,
            items,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ImplementationReadyItem {
    pub name: String,
    pub result: String,
    pub detail: Option<String>,
}

impl ImplementationReadyItem {
    fn pass(name: &str, detail: Option<String>) -> Self {
        Self {
            name: name.to_string(),
            result: "pass".to_string(),
            detail,
        }
    }

    fn fail<S: Into<String>>(name: &str, detail: Option<S>) -> Self {
        Self {
            name: name.to_string(),
            result: "fail".to_string(),
            detail: detail.map(Into::into),
        }
    }
}
