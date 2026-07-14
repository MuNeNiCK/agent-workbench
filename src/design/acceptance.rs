use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{open_existing_project, project_id};

use super::{validation::*, *};

pub(super) fn mark_stale_links_for_design_version(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_package_id: i64,
    current_design_version_id: i64,
) -> Result<()> {
    let stale_requirement = r#"
        select 1
        from design_requirements old_req
        join design_versions old_version on old_version.id = old_req.design_version_id
        where old_req.id = design_requirements.id
          and old_req.project_id = ?1
          and old_version.design_package_id = ?2
          and old_req.design_version_id != ?3
          and not exists (
              select 1
              from design_requirements current_req
              where current_req.project_id = old_req.project_id
                and current_req.design_version_id = ?3
                and current_req.requirement_key = old_req.requirement_key
                and current_req.requirement_hash = old_req.requirement_hash
                and current_req.status = 'active'
          )
    "#;
    conn.execute(
        &format!(
            r#"
            update task_derivations
            set status = 'stale'
            where project_id = ?1
              and status = 'active'
              and exists (
                  select 1
                  from design_requirements
                  where design_requirements.id = task_derivations.design_requirement_id
                    and exists ({stale_requirement})
              )
            "#
        ),
        params![project_id, design_package_id, current_design_version_id],
    )?;
    conn.execute(
        &format!(
            r#"
            update checklists
            set status = 'stale'
            where project_id = ?1
              and status = 'active'
              and exists (
                  select 1
                  from checklist_items item
                  join design_requirements on design_requirements.id = item.design_requirement_id
                  where item.checklist_id = checklists.id
                    and exists ({stale_requirement})
              )
            "#
        ),
        params![project_id, design_package_id, current_design_version_id],
    )?;
    conn.execute(
        &format!(
            r#"
            update coverage_items
            set status = 'stale'
            where project_id = ?1
              and status != 'stale'
              and exists (
                  select 1
                  from design_requirements
                  where design_requirements.id = coverage_items.design_requirement_id
                    and exists ({stale_requirement})
              )
            "#
        ),
        params![project_id, design_package_id, current_design_version_id],
    )?;
    conn.execute(
        r#"
        update validation_gates
        set status = 'stale'
        where project_id = ?1
          and status = 'active'
          and (
              exists (
                  select 1
                  from validation_gate_templates old_gate
                  join design_versions old_version on old_version.id = old_gate.design_version_id
                  where old_gate.id = validation_gates.template_id
                    and old_gate.project_id = ?1
                    and old_version.design_package_id = ?2
                    and old_gate.design_version_id != ?3
                    and not exists (
                        select 1
                        from validation_gate_templates current_gate
                        where current_gate.project_id = old_gate.project_id
                          and current_gate.design_version_id = ?3
                          and current_gate.gate_key = old_gate.gate_key
                          and current_gate.gate_hash = old_gate.gate_hash
                          and current_gate.status = 'active'
                    )
              )
              or exists (
                  select 1
                  from design_requirements old_req
                  join design_versions old_version on old_version.id = old_req.design_version_id
                  where old_req.id = validation_gates.design_requirement_id
                    and old_req.project_id = ?1
                    and old_version.design_package_id = ?2
                    and old_req.design_version_id != ?3
                    and not exists (
                        select 1
                        from design_requirements current_req
                        where current_req.project_id = old_req.project_id
                          and current_req.design_version_id = ?3
                          and current_req.requirement_key = old_req.requirement_key
                          and current_req.requirement_hash = old_req.requirement_hash
                          and current_req.status = 'active'
                    )
              )
          )
        "#,
        params![project_id, design_package_id, current_design_version_id],
    )?;
    conn.execute(
        r#"
        update review_plans
        set status = 'blocked'
        where project_id = ?1
          and status = 'open'
          and design_version_id in (
              select id
              from design_versions
              where design_package_id = ?2 and id != ?3
          )
        "#,
        params![project_id, design_package_id, current_design_version_id],
    )?;
    Ok(())
}

pub fn list_design_requirements(
    root: &Path,
    input: DesignRequirementListQuery,
) -> Result<Vec<DesignRequirementRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select
            r.id, r.design_version_id, r.source_design_file_id,
            f.relative_path, r.source_section, r.requirement_key,
            r.revision, r.requirement_text, r.priority,
            r.required_surfaces, r.validation_expectation, r.status
        from design_requirements r
        join design_files f on f.id = r.source_design_file_id
        where r.project_id = ?1 and r.design_version_id = ?2
        order by r.requirement_key
        "#,
    )?;
    let rows = stmt.query_map(params![project_id, input.design_version_id], |row| {
        Ok(DesignRequirementRecord {
            id: row.get(0)?,
            design_version_id: row.get(1)?,
            source_design_file_id: row.get(2)?,
            source_path: row.get(3)?,
            source_section: row.get(4)?,
            requirement_key: row.get(5)?,
            revision: row.get(6)?,
            requirement_text: row.get(7)?,
            priority: row.get(8)?,
            required_surfaces: row.get(9)?,
            validation_expectation: row.get(10)?,
            status: row.get(11)?,
        })
    })?;

    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

pub fn list_design_decisions(
    root: &Path,
    input: DesignDecisionListQuery,
) -> Result<Vec<DesignDecisionRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select
            d.id, d.design_version_id, d.source_design_file_id,
            f.relative_path, d.source_section, d.decision_key,
            d.topic, d.decision_text, d.rationale, d.supersedes_decision_keys, d.status
        from design_decisions d
        join design_files f on f.id = d.source_design_file_id
        where d.project_id = ?1 and d.design_version_id = ?2
        order by d.decision_key
        "#,
    )?;
    let rows = stmt.query_map(params![project_id, input.design_version_id], |row| {
        Ok(DesignDecisionRecord {
            id: row.get(0)?,
            design_version_id: row.get(1)?,
            source_design_file_id: row.get(2)?,
            source_path: row.get(3)?,
            source_section: row.get(4)?,
            decision_key: row.get(5)?,
            topic: row.get(6)?,
            decision_text: row.get(7)?,
            rationale: row.get(8)?,
            supersedes_decision_keys: row.get(9)?,
            status: row.get(10)?,
        })
    })?;

    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

pub fn list_validation_gate_templates(
    root: &Path,
    input: ValidationGateTemplateListQuery,
) -> Result<Vec<ValidationGateTemplateRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select
            g.id, g.design_version_id, g.source_design_file_id,
            f.relative_path, g.source_section, g.gate_key,
            g.stage, g.command, g.expected_result, g.requirement_keys, g.gate_text, g.status
        from validation_gate_templates g
        join design_files f on f.id = g.source_design_file_id
        where g.project_id = ?1 and g.design_version_id = ?2
        order by g.gate_key
        "#,
    )?;
    let rows = stmt.query_map(params![project_id, input.design_version_id], |row| {
        Ok(ValidationGateTemplateRecord {
            id: row.get(0)?,
            design_version_id: row.get(1)?,
            source_design_file_id: row.get(2)?,
            source_path: row.get(3)?,
            source_section: row.get(4)?,
            gate_key: row.get(5)?,
            stage: row.get(6)?,
            command: row.get(7)?,
            expected_result: row.get(8)?,
            requirement_keys: row.get(9)?,
            gate_text: row.get(10)?,
            status: row.get(11)?,
        })
    })?;

    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

pub fn accept_design_exception(
    root: &Path,
    input: NewDesignExceptionAcceptance<'_>,
) -> Result<DesignExceptionAcceptanceOutcome> {
    validate_design_acceptance_type(input.acceptance_type)?;
    match (input.design_version_id, input.design_package) {
        (Some(_), None) | (None, Some(_)) => {}
        _ => bail!("provide exactly one of design_version_id or design_package"),
    }
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    ensure_active_authority_event(&tx, project_id, input.approval_authority_event_id)?;
    let target = match (input.design_version_id, input.design_package) {
        (Some(design_version_id), None) => {
            resolve_design_acceptance_target(&tx, project_id, design_version_id, input.target)?
        }
        (None, Some(design_package)) => {
            resolve_pre_import_design_acceptance_target(design_package, input.target)?
        }
        _ => unreachable!("validated above"),
    };
    let scope = match (input.design_version_id, input.design_package) {
        (Some(design_version_id), None) => design_version_id.to_string(),
        (None, Some(design_package)) => design_package.to_string(),
        _ => unreachable!("validated above"),
    };

    tx.execute(
        r#"
        insert into acceptance_records(
            project_id, target_type, task_id, design_requirement_id,
            validation_gate_template_id, coverage_item_id, design_package_key,
            design_file_path, design_requirement_key, acceptance_type, reason, scope,
            created_by, status, approved_by_authority_event_id, approved_at,
            created_at, review_impact
        )
        values (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
            'user', 'approved', ?13, current_timestamp,
            current_timestamp, 'design exception accepted for current design scope'
        )
        "#,
        params![
            project_id,
            target.target_type,
            target.task_id,
            target.design_requirement_id,
            target.validation_gate_template_id,
            target.coverage_item_id,
            target.design_package_key,
            target.design_file_path,
            target.design_requirement_key,
            input.acceptance_type,
            input.reason,
            scope,
            input.approval_authority_event_id,
        ],
    )?;
    let acceptance_record_id = tx.last_insert_rowid();
    if input.acceptance_type == "accepted_out_of_scope" {
        match target.target_type {
            "design_requirement" => {
                tx.execute(
                    "update design_requirements set status = 'accepted_out_of_scope' where id = ?1",
                    params![target.design_requirement_id],
                )?;
            }
            "validation_gate_template" => {
                tx.execute(
                    "update validation_gate_templates set status = 'accepted_out_of_scope' where id = ?1",
                    params![target.validation_gate_template_id],
                )?;
            }
            "coverage_item" => {
                tx.execute(
                    "update coverage_items set status = 'accepted_out_of_scope' where id = ?1",
                    params![target.coverage_item_id],
                )?;
            }
            "design_file" | "design_requirement_key" => {}
            _ => unreachable!("target type resolved above"),
        }
    }
    tx.commit()?;

    Ok(DesignExceptionAcceptanceOutcome {
        acceptance_record_id,
        authority_event_id: input.approval_authority_event_id,
        target_type: target.target_type.to_string(),
        design_requirement_id: target.design_requirement_id,
        validation_gate_template_id: target.validation_gate_template_id,
        coverage_item_id: target.coverage_item_id,
        design_package_key: target.design_package_key,
        design_file_path: target.design_file_path,
        design_requirement_key: target.design_requirement_key,
    })
}

pub fn add_general_acceptance(
    root: &Path,
    input: NewGeneralAcceptance<'_>,
) -> Result<GeneralAcceptanceOutcome> {
    validate_acceptance_type(input.acceptance_type)?;
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let outcome = add_general_acceptance_in(&tx, project_id, input)?;
    tx.commit()?;
    Ok(outcome)
}

pub(crate) fn add_general_acceptance_in(
    conn: &rusqlite::Connection,
    project_id: i64,
    input: NewGeneralAcceptance<'_>,
) -> Result<GeneralAcceptanceOutcome> {
    ensure_active_authority_event(conn, project_id, input.approval_authority_event_id)?;
    let target = resolve_general_acceptance_target(conn, project_id, input.target)?;
    conn.execute(
        r#"
        insert into acceptance_records(
            project_id, target_type, task_id, finding_id, validation_gate_id,
            validation_run_id, repository_state_classification_id,
            repository_snapshot_comparison_id, review_plan_id, checklist_item_id,
            command_profile_id, command_usage_id, command_deviation_id,
            rule_binding_id, stale_record_type, stale_record_id,
            acceptance_type, reason, scope,
            created_by, status, approved_by_authority_event_id, approved_at,
            created_at, review_impact
        )
        values (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
            ?16, ?17, ?18, ?19, 'user', 'approved', ?20, current_timestamp,
            current_timestamp, 'general acceptance recorded for current workflow'
        )
        "#,
        params![
            project_id,
            target.target_type,
            target.task_id,
            target.finding_id,
            target.validation_gate_id,
            target.validation_run_id,
            target.repository_state_classification_id,
            target.repository_snapshot_comparison_id,
            target.review_plan_id,
            target.checklist_item_id,
            target.command_profile_id,
            target.command_usage_id,
            target.command_deviation_id,
            target.rule_binding_id,
            target.stale_record_type,
            target.stale_record_id,
            input.acceptance_type,
            input.reason,
            input.target,
            input.approval_authority_event_id,
        ],
    )?;
    let acceptance_record_id = conn.last_insert_rowid();
    Ok(GeneralAcceptanceOutcome {
        acceptance_record_id,
        authority_event_id: input.approval_authority_event_id,
        target_type: target.target_type.to_string(),
    })
}

pub(super) fn resolve_general_acceptance_target(
    conn: &rusqlite::Connection,
    project_id: i64,
    target: &str,
) -> Result<ResolvedGeneralAcceptanceTarget> {
    let Some((kind, raw_id)) = target.split_once(':') else {
        bail!("general acceptance target must be kind:<id>");
    };
    if kind == "stale" {
        let Some((stale_type, stale_id)) = raw_id.split_once(':') else {
            bail!("stale acceptance target must be stale:<record-type>:<id>");
        };
        return Ok(ResolvedGeneralAcceptanceTarget {
            target_type: "stale_record",
            stale_record_type: Some(stale_type.to_string()),
            stale_record_id: Some(parse_positive_i64(stale_id, "stale record id")?),
            ..ResolvedGeneralAcceptanceTarget::new("stale_record")
        });
    }
    let id = parse_positive_i64(raw_id, "acceptance target id")?;
    match kind {
        "task" => {
            ensure_project_row(conn, "tasks", "work_units", "work_unit_id", id, project_id)?;
            Ok(ResolvedGeneralAcceptanceTarget {
                task_id: Some(id),
                ..ResolvedGeneralAcceptanceTarget::new("task")
            })
        }
        "finding" => {
            ensure_direct_project_row(conn, "findings", id, project_id)?;
            Ok(ResolvedGeneralAcceptanceTarget {
                finding_id: Some(id),
                ..ResolvedGeneralAcceptanceTarget::new("finding")
            })
        }
        "validation-gate" => {
            ensure_direct_project_row(conn, "validation_gates", id, project_id)?;
            Ok(ResolvedGeneralAcceptanceTarget {
                validation_gate_id: Some(id),
                ..ResolvedGeneralAcceptanceTarget::new("validation_gate")
            })
        }
        "validation-run" => {
            ensure_direct_project_row(conn, "validation_runs", id, project_id)?;
            Ok(ResolvedGeneralAcceptanceTarget {
                validation_run_id: Some(id),
                ..ResolvedGeneralAcceptanceTarget::new("validation_run")
            })
        }
        "repository-state" => Ok(ResolvedGeneralAcceptanceTarget {
            repository_state_classification_id: {
                ensure_repository_state_classification_project(conn, id, project_id)?;
                Some(id)
            },
            ..ResolvedGeneralAcceptanceTarget::new("repository_state_classification")
        }),
        "repository-comparison" => Ok(ResolvedGeneralAcceptanceTarget {
            repository_snapshot_comparison_id: {
                ensure_repository_snapshot_comparison_project(conn, id, project_id)?;
                Some(id)
            },
            ..ResolvedGeneralAcceptanceTarget::new("repository_snapshot_comparison")
        }),
        "review-plan" => {
            ensure_direct_project_row(conn, "review_plans", id, project_id)?;
            Ok(ResolvedGeneralAcceptanceTarget {
                review_plan_id: Some(id),
                ..ResolvedGeneralAcceptanceTarget::new("review_plan")
            })
        }
        "checklist-item" => {
            ensure_checklist_item_project(conn, id, project_id)?;
            Ok(ResolvedGeneralAcceptanceTarget {
                checklist_item_id: Some(id),
                ..ResolvedGeneralAcceptanceTarget::new("checklist_item")
            })
        }
        "command-profile" => {
            ensure_direct_project_row(conn, "command_profiles", id, project_id)?;
            Ok(ResolvedGeneralAcceptanceTarget {
                command_profile_id: Some(id),
                ..ResolvedGeneralAcceptanceTarget::new("command_profile")
            })
        }
        "command-usage" => {
            ensure_direct_project_row(conn, "command_usages", id, project_id)?;
            Ok(ResolvedGeneralAcceptanceTarget {
                command_usage_id: Some(id),
                ..ResolvedGeneralAcceptanceTarget::new("command_usage")
            })
        }
        "command-deviation" => {
            ensure_command_deviation_project(conn, id, project_id)?;
            Ok(ResolvedGeneralAcceptanceTarget {
                command_deviation_id: Some(id),
                ..ResolvedGeneralAcceptanceTarget::new("command_deviation")
            })
        }
        "rule" | "rule-binding" => {
            ensure_direct_project_row(conn, "rule_bindings", id, project_id)?;
            Ok(ResolvedGeneralAcceptanceTarget {
                rule_binding_id: Some(id),
                ..ResolvedGeneralAcceptanceTarget::new("rule_binding")
            })
        }
        _ => bail!("unsupported general acceptance target kind: {kind}"),
    }
}

pub(super) fn ensure_direct_project_row(
    conn: &rusqlite::Connection,
    table: &str,
    id: i64,
    project_id: i64,
) -> Result<()> {
    let sql = format!("select 1 from {table} where id = ?1 and project_id = ?2");
    conn.query_row(&sql, params![id, project_id], |_| Ok(()))
        .optional()?
        .with_context(|| format!("{table} row not found for project"))?;
    Ok(())
}

pub(super) fn ensure_active_authority_event(
    conn: &rusqlite::Connection,
    project_id: i64,
    authority_event_id: i64,
) -> Result<()> {
    let exists: bool = conn
        .query_row(
            r#"
            select exists(
                select 1
                from authority_events
                where id = ?1
                  and project_id = ?2
                  and status = 'active'
                  and event_type in ('user_instruction', 'policy', 'design_doc')
            )
            "#,
            params![authority_event_id, project_id],
            |row| row.get(0),
        )
        .context("failed to validate acceptance authority event")?;
    if !exists {
        bail!(
            "acceptance approval requires an active user, policy, or design authority event from this project"
        );
    }
    Ok(())
}

pub(super) fn ensure_project_row(
    conn: &rusqlite::Connection,
    table: &str,
    project_table: &str,
    project_fk: &str,
    id: i64,
    project_id: i64,
) -> Result<()> {
    let sql = format!(
        "select 1 from {table} child join {project_table} owner on owner.id = child.{project_fk} where child.id = ?1 and owner.project_id = ?2"
    );
    conn.query_row(&sql, params![id, project_id], |_| Ok(()))
        .optional()?
        .with_context(|| format!("{table} row not found for project"))?;
    Ok(())
}

pub(super) fn ensure_repository_state_classification_project(
    conn: &rusqlite::Connection,
    id: i64,
    project_id: i64,
) -> Result<()> {
    conn.query_row(
        r#"
        select 1
        from repository_state_classifications c
        join repository_snapshots s on s.id = c.repository_snapshot_id
        join repositories r on r.id = s.repository_id
        where c.id = ?1 and r.project_id = ?2
        "#,
        params![id, project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("repository state classification not found for project")?;
    Ok(())
}

pub(super) fn ensure_repository_snapshot_comparison_project(
    conn: &rusqlite::Connection,
    id: i64,
    project_id: i64,
) -> Result<()> {
    conn.query_row(
        r#"
        select 1
        from repository_snapshot_comparisons c
        join repository_snapshots base on base.id = c.base_repository_snapshot_id
        join repositories base_repo on base_repo.id = base.repository_id
        join repository_snapshots current on current.id = c.current_repository_snapshot_id
        join repositories current_repo on current_repo.id = current.repository_id
        where c.id = ?1
          and base_repo.project_id = ?2
          and current_repo.project_id = ?2
        "#,
        params![id, project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("repository snapshot comparison not found for project")?;
    Ok(())
}

pub(super) fn ensure_checklist_item_project(
    conn: &rusqlite::Connection,
    id: i64,
    project_id: i64,
) -> Result<()> {
    conn.query_row(
        r#"
        select 1
        from checklist_items item
        join checklists c on c.id = item.checklist_id
        where item.id = ?1 and c.project_id = ?2
        "#,
        params![id, project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("checklist item not found for project")?;
    Ok(())
}

pub(super) fn ensure_command_deviation_project(
    conn: &rusqlite::Connection,
    id: i64,
    project_id: i64,
) -> Result<()> {
    conn.query_row(
        r#"
        select 1
        from command_deviations d
        join command_profiles p on p.id = d.command_profile_id
        where d.id = ?1 and p.project_id = ?2
        "#,
        params![id, project_id],
        |_| Ok(()),
    )
    .optional()?
    .context("command deviation not found for project")?;
    Ok(())
}
