use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{ensure_unscoped_mutation_allowed, open_existing_project, project_id};

pub fn add_coverage_item(root: &Path, input: NewCoverageItem<'_>) -> Result<CoverageItemOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    ensure_unscoped_mutation_allowed(&tx, "coverage add")?;
    if input.status == "accepted_out_of_scope" {
        bail!("coverage accepted_out_of_scope requires an approved acceptance record");
    }
    if input.status == "covered" {
        let has_boundary_evidence = [
            input.runtime_boundary_evidence,
            input.ux_boundary_evidence,
            input.lifecycle_boundary_evidence,
        ]
        .into_iter()
        .flatten()
        .any(|value| !value.trim().is_empty());
        let has_tests_or_gates = input
            .tests_or_gates
            .is_some_and(|value| !value.trim().is_empty());
        if !has_boundary_evidence || !has_tests_or_gates {
            bail!("covered coverage requires boundary evidence and tests_or_gates");
        }
    }
    let design_requirement_id = tx
        .query_row(
            r#"
            select id
            from design_requirements
            where project_id = ?1
              and design_version_id = ?2
              and requirement_key = ?3
              and status = 'active'
            "#,
            params![project_id, input.design_version_id, input.requirement_key],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .context("active design requirement not found")?;
    let task_work_unit_id = match input.task_id {
        Some(task_id) => tx
            .query_row(
                "select work_unit_id from tasks where id = ?1",
                params![task_id],
                |row| row.get::<_, Option<i64>>(0),
            )
            .optional()?
            .context("task not found")?,
        None => None,
    };
    if let (Some(input_work_unit_id), Some(task_work_unit_id)) =
        (input.work_unit_id, task_work_unit_id)
        && input_work_unit_id != task_work_unit_id
    {
        bail!("coverage work unit must match task work unit");
    };
    let work_unit_id = input.work_unit_id.or(task_work_unit_id);
    if let Some(task_id) = input.task_id {
        let derived = tx
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
        if !derived {
            bail!("task is not actively derived from the design requirement");
        }
    }

    tx.execute(
        r#"
        update coverage_items
        set status = 'stale'
        where project_id = ?1
          and design_requirement_id = ?2
          and task_id is ?3
          and work_unit_id is ?4
          and status != 'stale'
        "#,
        params![
            project_id,
            design_requirement_id,
            input.task_id,
            work_unit_id
        ],
    )?;

    tx.execute(
        r#"
        insert into coverage_items(
            project_id, review_scope_id, work_unit_id, design_requirement_id, task_id,
            requirement, runtime_boundary_evidence, ux_boundary_evidence,
            lifecycle_boundary_evidence, tests_or_gates, missing_or_unverified,
            status, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, current_timestamp)
        "#,
        params![
            project_id,
            input.review_scope_id,
            work_unit_id,
            design_requirement_id,
            input.task_id,
            input.requirement,
            input.runtime_boundary_evidence,
            input.ux_boundary_evidence,
            input.lifecycle_boundary_evidence,
            input.tests_or_gates,
            input.missing_or_unverified,
            input.status,
        ],
    )?;
    let coverage_item_id = tx.last_insert_rowid();
    tx.commit()?;

    Ok(CoverageItemOutcome {
        coverage_item_id,
        work_unit_id,
        design_requirement_id,
        task_id: input.task_id,
    })
}

pub fn list_coverage_items(
    root: &Path,
    input: CoverageItemListQuery<'_>,
) -> Result<Vec<CoverageItemRecord>> {
    list_coverage_items_filtered(
        root,
        CoverageItemListFilter {
            design_version_id: Some(input.design_version_id),
            status: input.status,
            work_unit_id: input.work_unit_id,
        },
    )
}

pub fn list_coverage_items_filtered(
    root: &Path,
    input: CoverageItemListFilter<'_>,
) -> Result<Vec<CoverageItemRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        select
            c.id, c.work_unit_id, c.task_id, r.requirement_key, c.requirement,
            c.status, c.tests_or_gates, c.missing_or_unverified
        from coverage_items c
        join design_requirements r on r.id = c.design_requirement_id
        left join tasks t on t.id = c.task_id
        where c.project_id = ?1
          and (?2 is null or r.design_version_id = ?2)
          and (?3 is null or c.status = ?3)
          and (?4 is null or coalesce(c.work_unit_id, t.work_unit_id) = ?4)
        order by r.requirement_key, c.id
        "#,
    )?;
    let rows = stmt.query_map(
        params![
            project_id,
            input.design_version_id,
            input.status,
            input.work_unit_id
        ],
        |row| {
            Ok(CoverageItemRecord {
                id: row.get(0)?,
                work_unit_id: row.get(1)?,
                task_id: row.get(2)?,
                requirement_key: row.get(3)?,
                requirement: row.get(4)?,
                status: row.get(5)?,
                tests_or_gates: row.get(6)?,
                missing_or_unverified: row.get(7)?,
            })
        },
    )?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

pub struct NewCoverageItem<'a> {
    pub design_version_id: i64,
    pub requirement_key: &'a str,
    pub review_scope_id: Option<i64>,
    pub work_unit_id: Option<i64>,
    pub task_id: Option<i64>,
    pub requirement: &'a str,
    pub runtime_boundary_evidence: Option<&'a str>,
    pub ux_boundary_evidence: Option<&'a str>,
    pub lifecycle_boundary_evidence: Option<&'a str>,
    pub tests_or_gates: Option<&'a str>,
    pub missing_or_unverified: Option<&'a str>,
    pub status: &'a str,
}

pub struct CoverageItemListQuery<'a> {
    pub design_version_id: i64,
    pub status: Option<&'a str>,
    pub work_unit_id: Option<i64>,
}

pub struct CoverageItemListFilter<'a> {
    pub design_version_id: Option<i64>,
    pub status: Option<&'a str>,
    pub work_unit_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CoverageItemOutcome {
    pub coverage_item_id: i64,
    pub work_unit_id: Option<i64>,
    pub design_requirement_id: i64,
    pub task_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CoverageItemRecord {
    pub id: i64,
    pub work_unit_id: Option<i64>,
    pub task_id: Option<i64>,
    pub requirement_key: String,
    pub requirement: String,
    pub status: String,
    pub tests_or_gates: Option<String>,
    pub missing_or_unverified: Option<String>,
}
