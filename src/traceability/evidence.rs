use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{open_existing_project, project_id};
use crate::rules::{RuleBindingInput, insert_rule_binding};

use super::{checklists::*, readiness::*, *};

pub(crate) fn validate_design_decomposition_in(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_version_id: i64,
    work_unit_id: i64,
) -> Result<()> {
    validate_design_decomposition_scope_in(conn, project_id, design_version_id, work_unit_id)?;
    let (_requirements, derived, wrong_owner): (i64, i64, i64) = conn.query_row(
        r#"
        select
          (select count(*) from design_requirements r
           where r.design_version_id=?1 and r.status='active'),
          (select count(*) from task_derivations td
           join design_requirements r on r.id=td.design_requirement_id
           where r.design_version_id=?1 and r.status='active' and td.status='active'),
          (select count(*) from task_derivations td
           join design_requirements r on r.id=td.design_requirement_id
           join tasks t on t.id=td.task_id
           where r.design_version_id=?1 and r.status='active' and td.status='active'
             and t.work_unit_id != ?2)
        "#,
        params![design_version_id, work_unit_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    if wrong_owner > 0 || derived > 0 {
        bail!("design decomposition has partial or conflicting current derivations");
    }
    Ok(())
}

pub(crate) fn validate_design_decomposition_scope_in(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_version_id: i64,
    work_unit_id: i64,
) -> Result<()> {
    conn.query_row(
        "select 1 from work_units where id = ?1 and project_id = ?2 and status = 'open'",
        params![work_unit_id, project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("open work unit not found")?;
    conn.query_row(
        r#"
        select 1
        from design_versions v
        join design_packages p on p.id = v.design_package_id
        where v.id = ?1 and v.project_id = ?2 and v.status = 'approved'
          and p.current_design_version_id = v.id
        "#,
        params![design_version_id, project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("design version is not the current approved design")?;
    ensure_design_ready_for_decomposition(conn, project_id, design_version_id)?;
    Ok(())
}

pub(super) fn ensure_design_ready_for_decomposition(
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

pub(super) fn first_line(text: &str) -> &str {
    text.lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .unwrap_or("design requirement")
}

pub fn list_task_derivations(
    root: &Path,
    input: TaskDerivationListQuery,
) -> Result<Vec<TaskDerivationRecord>> {
    list_task_derivations_filtered(
        root,
        TaskDerivationListFilter {
            design_version_id: Some(input.design_version_id),
            task_id: None,
            work_unit_id: input.work_unit_id,
        },
    )
}

pub fn list_task_derivations_filtered(
    root: &Path,
    input: TaskDerivationListFilter,
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
          and (?2 is null or r.design_version_id = ?2)
          and (?3 is null or td.task_id = ?3)
          and (?4 is null or t.work_unit_id = ?4)
        order by r.requirement_key, td.id
        "#,
    )?;
    let rows = stmt.query_map(
        params![
            project_id,
            input.design_version_id,
            input.task_id,
            input.work_unit_id
        ],
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

pub(super) fn insert_implementation_evidence(
    root: &Path,
    input: ImplementationEvidenceInput<'_>,
) -> Result<ImplementationEvidenceOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    ensure_no_active_source_correction(&tx, "implementation evidence add")?;
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
        require_implementation_evidence_derivation(&tx, design_requirement_id, task_id)?;
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
          and (?5 is null or e.evidence_type = ?5)
        order by e.id
        "#,
    )?;
    let rows = stmt.query_map(
        params![
            project_id,
            input.task_id,
            input.design_version_id,
            input.work_unit_id,
            input.evidence_type
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

pub(super) fn resolve_git_evidence(
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
    ensure_no_active_source_correction(&tx, "validation gate select")?;
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
            from current_task_validation_gates
            where id = ?1 and project_id = ?2
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
        .context("current validation gate not found")?;
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
            vr.artifact_path, vr.artifact_hash, vr.notes, vr.created_at,
            exists(select 1 from validation_link_retirements retirement where retirement.validation_run_id=vr.id)
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
            retired: row.get(15)?,
        })
    })?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}
