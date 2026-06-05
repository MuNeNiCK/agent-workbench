use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{open_existing_project, project_id};

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
        where td.project_id = ?1 and r.design_version_id = ?2
        order by r.requirement_key, td.id
        "#,
    )?;
    let rows = stmt.query_map(params![project_id, input.design_version_id], |row| {
        Ok(TaskDerivationRecord {
            id: row.get(0)?,
            requirement_key: row.get(1)?,
            task_id: row.get(2)?,
            task_title: row.get(3)?,
            checklist_item_id: row.get(4)?,
            checklist_item_title: row.get(5)?,
            status: row.get(6)?,
        })
    })?;
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
        where e.project_id = ?1
          and (?2 is null or e.task_id = ?2)
          and (?3 is null or r.design_version_id = ?3)
        order by e.id
        "#,
    )?;
    let rows = stmt.query_map(
        params![project_id, input.task_id, input.design_version_id],
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
    let command = input.command.or(template.command.as_deref());
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
            task_work_unit_id,
            input.task_id,
            requirement_id,
            command,
            template.expected_result,
        ],
    )?;
    let validation_gate_id = tx.last_insert_rowid();
    tx.commit()?;

    Ok(ValidationGateSelectionOutcome {
        validation_gate_id,
        validation_gate_template_id: template.id,
        design_requirement_id: requirement_id,
        task_id: input.task_id,
    })
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
          and td.status = 'active'
          and p.current_design_version_id != r.design_version_id
          and not exists (
            select 1
            from design_requirements current_r
            where current_r.design_version_id = p.current_design_version_id
              and current_r.requirement_key = r.requirement_key
              and current_r.requirement_hash = r.requirement_hash
              and current_r.status = 'active'
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
          and c.status = 'active'
          and p.current_design_version_id != r.design_version_id
          and not exists (
            select 1
            from design_requirements current_r
            where current_r.design_version_id = p.current_design_version_id
              and current_r.requirement_key = r.requirement_key
              and current_r.requirement_hash = r.requirement_hash
              and current_r.status = 'active'
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
          and vg.status = 'active'
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
              and (c.task_id = td.task_id or c.task_id is null)
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

pub struct TaskDerivationListQuery {
    pub design_version_id: i64,
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

    fn fail(name: &str, detail: Option<String>) -> Self {
        Self {
            name: name.to_string(),
            result: "fail".to_string(),
            detail,
        }
    }
}
