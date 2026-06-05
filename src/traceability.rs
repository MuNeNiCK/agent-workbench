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
        "#,
        params![design_package_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn count_stale_checklists(conn: &rusqlite::Connection, design_package_id: i64) -> Result<i64> {
    conn.query_row(
        r#"
        select count(*)
        from checklists c
        join design_versions v on v.id = c.design_version_id
        join design_packages p on p.id = v.design_package_id
        where p.id = ?1
          and c.status = 'active'
          and p.current_design_version_id != c.design_version_id
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
