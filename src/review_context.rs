use std::fmt::Write;
use std::path::Path;

use anyhow::Result;

use crate::coverage::{CoverageItemListQuery, list_coverage_items};
use crate::db::{open_existing_project, project_id};
use crate::design::{
    DesignRequirementListQuery, DesignRequirementRecord, list_design_requirements,
};
use crate::planning::{TaskListQuery, list_tasks};
use crate::traceability::{
    ImplementationEvidenceListQuery, ImplementationEvidenceRecord, StaleRecord,
    TaskDerivationListQuery, ValidationGateContextQuery, list_implementation_evidence,
    list_stale_records, list_task_derivations, list_validation_gate_context,
};

pub struct ReviewContextQuery<'a> {
    pub kind: &'a str,
    pub design_version_id: Option<i64>,
    pub work_unit_id: Option<i64>,
}

pub struct ReviewContextDocument {
    pub context_ref: String,
    pub text: String,
}

pub fn review_context_ref(
    kind: &str,
    design_version_id: Option<i64>,
    work_unit_id: Option<i64>,
) -> String {
    let design = design_version_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "-".to_string());
    let work = work_unit_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "-".to_string());
    format!("review-context:{kind}:design={design}:work={work}")
}

pub fn render_review_context(
    root: &Path,
    query: ReviewContextQuery<'_>,
) -> Result<ReviewContextDocument> {
    let context_ref = review_context_ref(query.kind, query.design_version_id, query.work_unit_id);
    let mut output = String::new();
    writeln!(output, "review_context: {}", query.kind)?;
    writeln!(output, "context_ref: {context_ref}")?;

    if let Some(design_version_id) = query.design_version_id {
        writeln!(output, "design_version_id: {design_version_id}")?;
        render_design_context(
            root,
            query.kind,
            design_version_id,
            query.work_unit_id,
            &mut output,
        )?;
    }
    if let Some(work_unit_id) = query.work_unit_id {
        writeln!(output, "work_unit_id: {work_unit_id}")?;
        render_work_context(root, work_unit_id, &mut output)?;
    }
    render_stale_context(root, query.work_unit_id, &mut output)?;

    Ok(ReviewContextDocument {
        context_ref,
        text: output,
    })
}

pub(crate) fn review_plan_has_clean_context_run(
    conn: &rusqlite::Connection,
    review_plan_id: i64,
    kind: &str,
    design_version_id: Option<i64>,
    work_unit_id: Option<i64>,
) -> Result<bool> {
    let context_ref = review_context_ref(kind, design_version_id, work_unit_id);
    conn.query_row(
        r#"
        select exists (
            select 1
            from review_runs
            where review_plan_id = ?1
              and target_ref = ?2
              and run_type = 'fresh'
              and run_purpose = 'new_unbiased_review'
              and clean_run = 1
              and status = 'completed'
        )
        "#,
        rusqlite::params![review_plan_id, context_ref],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(crate) fn required_plans_missing_context_count(
    conn: &rusqlite::Connection,
    project_id: i64,
    stage: &str,
    review_type: &str,
    design_version_id: Option<i64>,
    work_unit_id: Option<i64>,
    kind: &str,
) -> Result<i64> {
    let mut stmt = conn.prepare(
        r#"
        select id, design_version_id, work_unit_id
        from review_plans rp
        where rp.project_id = ?1
          and rp.stage = ?2
          and rp.review_type = ?3
          and rp.required = 1
          and (?4 is null or rp.design_version_id = ?4)
          and (?5 is null or rp.work_unit_id = ?5)
        "#,
    )?;
    let rows = stmt.query_map(
        rusqlite::params![
            project_id,
            stage,
            review_type,
            design_version_id,
            work_unit_id
        ],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<i64>>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        },
    )?;
    let mut missing = 0;
    for row in rows {
        let (review_plan_id, plan_design_version_id, plan_work_unit_id) = row?;
        if !review_plan_has_clean_context_run(
            conn,
            review_plan_id,
            kind,
            plan_design_version_id,
            plan_work_unit_id,
        )? {
            missing += 1;
        }
    }
    Ok(missing)
}

fn render_design_context(
    root: &Path,
    kind: &str,
    design_version_id: i64,
    work_unit_id: Option<i64>,
    output: &mut String,
) -> Result<()> {
    let requirements = list_context_requirements(
        root,
        design_version_id,
        work_unit_id.filter(|_| review_context_kind_is_work_scoped(kind)),
    )?;
    writeln!(output, "requirements:")?;
    if requirements.is_empty() {
        writeln!(output, "- none")?;
    }
    for requirement in requirements {
        let validation = requirement.validation_expectation.as_deref().unwrap_or("-");
        writeln!(
            output,
            "- {} [{}:{} validation={}] {}",
            requirement.requirement_key,
            requirement.priority,
            requirement.status,
            validation,
            requirement.requirement_text.lines().next().unwrap_or("")
        )?;
    }

    let derivations = list_task_derivations(
        root,
        TaskDerivationListQuery {
            design_version_id,
            work_unit_id,
        },
    )?;
    writeln!(output, "task_derivations:")?;
    if derivations.is_empty() {
        writeln!(output, "- none")?;
    }
    for derivation in derivations {
        writeln!(
            output,
            "- requirement={} task={} [{}] {}",
            derivation.requirement_key,
            derivation.task_id,
            derivation.status,
            derivation.task_title
        )?;
    }

    let gates = list_validation_gate_context(
        root,
        ValidationGateContextQuery {
            design_version_id,
            work_unit_id,
        },
    )?;
    writeln!(output, "selected_validation_gates:")?;
    if gates.is_empty() {
        writeln!(output, "- none")?;
    }
    for gate in gates {
        let task = gate
            .task_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string());
        let run = gate
            .latest_run_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string());
        let command_usage = gate
            .latest_command_usage_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string());
        let snapshot = gate
            .latest_repository_snapshot_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string());
        let result = gate.latest_result.as_deref().unwrap_or("-");
        let artifact = gate.latest_artifact_path.as_deref().unwrap_or("-");
        let notes = gate.latest_notes.as_deref().unwrap_or("-");
        writeln!(
            output,
            "- {} requirement={} task={} status={} latest_run={} latest_result={} command_usage={} snapshot={} artifact={} notes={}",
            gate.gate_key,
            gate.requirement_key,
            task,
            gate.status,
            run,
            result,
            command_usage,
            snapshot,
            artifact,
            notes
        )?;
    }

    let evidence = list_implementation_evidence(
        root,
        ImplementationEvidenceListQuery {
            task_id: None,
            design_version_id: Some(design_version_id),
            work_unit_id,
        },
    )?;
    writeln!(output, "implementation_evidence:")?;
    if evidence.is_empty() {
        writeln!(output, "- none")?;
    }
    for item in evidence {
        let requirement = item.requirement_key.as_deref().unwrap_or("-");
        let task = item
            .task_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string());
        writeln!(
            output,
            "- {} type={} requirement={} task={} {}",
            item.id,
            item.evidence_type,
            requirement,
            task,
            evidence_detail(&item)
        )?;
    }

    let coverage = list_coverage_items(
        root,
        CoverageItemListQuery {
            design_version_id,
            status: None,
            work_unit_id,
        },
    )?;
    writeln!(output, "coverage_items:")?;
    if coverage.is_empty() {
        writeln!(output, "- none")?;
    }
    for item in &coverage {
        let task = item
            .task_id
            .map(|id| id.to_string())
            .unwrap_or_else(|| "-".to_string());
        let tests = item.tests_or_gates.as_deref().unwrap_or("-");
        let gap = item.missing_or_unverified.as_deref().unwrap_or("-");
        writeln!(
            output,
            "- {} coverage={} task={} tests={} gap={} {}",
            item.requirement_key,
            item.status,
            task,
            tests,
            gap,
            item.requirement.lines().next().unwrap_or("")
        )?;
    }

    writeln!(output, "known_gaps:")?;
    let mut printed_gap = false;
    for item in coverage.iter().filter(|item| {
        matches!(
            item.status.as_str(),
            "partial" | "missing_required_surface" | "design_conflict" | "needs_evidence"
        ) || item.missing_or_unverified.is_some()
    }) {
        printed_gap = true;
        let gap = item
            .missing_or_unverified
            .as_deref()
            .unwrap_or("coverage incomplete");
        writeln!(output, "- coverage:{} [{}] {}", item.id, item.status, gap)?;
    }
    if !printed_gap {
        writeln!(output, "- none")?;
    }

    Ok(())
}

fn review_context_kind_is_work_scoped(kind: &str) -> bool {
    matches!(kind, "design-implementation-diff" | "implementation-review")
}

fn render_work_context(root: &Path, work_unit_id: i64, output: &mut String) -> Result<()> {
    let tasks = list_tasks(
        root,
        TaskListQuery {
            status: None,
            work_unit_id: Some(work_unit_id),
        },
    )?;
    writeln!(output, "tasks:")?;
    if tasks.is_empty() {
        writeln!(output, "- none")?;
    }
    for task in tasks {
        writeln!(
            output,
            "- {} [{}:{}] {}",
            task.id, task.priority, task.status, task.title
        )?;
    }
    Ok(())
}

fn render_stale_context(root: &Path, work_unit_id: Option<i64>, output: &mut String) -> Result<()> {
    let stale = match work_unit_id {
        Some(work_unit_id) => list_work_stale_records(root, work_unit_id)?,
        None => list_stale_records(root)?,
    };
    writeln!(output, "stale_records:")?;
    if stale.is_empty() {
        writeln!(output, "- none")?;
    }
    for record in stale {
        writeln!(
            output,
            "- {}:{} {}",
            record.record_type, record.id, record.label
        )?;
    }
    Ok(())
}

fn list_context_requirements(
    root: &Path,
    design_version_id: i64,
    work_unit_id: Option<i64>,
) -> Result<Vec<DesignRequirementRecord>> {
    let Some(work_unit_id) = work_unit_id else {
        return list_design_requirements(root, DesignRequirementListQuery { design_version_id });
    };
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        with relevant_requirements as (
            select distinct td.design_requirement_id as id
            from task_derivations td
            join tasks t on t.id = td.task_id
            where td.project_id = ?1
              and t.work_unit_id = ?3
            union
            select distinct vg.design_requirement_id as id
            from validation_gates vg
            left join tasks t on t.id = vg.task_id
            where vg.project_id = ?1
              and coalesce(vg.work_unit_id, t.work_unit_id) = ?3
            union
            select distinct e.design_requirement_id as id
            from implementation_evidence e
            join tasks t on t.id = e.task_id
            where e.project_id = ?1
              and e.design_requirement_id is not null
              and t.work_unit_id = ?3
            union
            select distinct c.design_requirement_id as id
            from coverage_items c
            left join tasks t on t.id = c.task_id
            where c.project_id = ?1
              and coalesce(c.work_unit_id, t.work_unit_id) = ?3
        )
        select
            r.id, r.design_version_id, r.source_design_file_id,
            f.relative_path, r.source_section, r.requirement_key,
            r.revision, r.requirement_text, r.priority,
            r.required_surfaces, r.validation_expectation, r.status
        from design_requirements r
        join design_files f on f.id = r.source_design_file_id
        join relevant_requirements rr on rr.id = r.id
        where r.project_id = ?1
          and r.design_version_id = ?2
        order by r.requirement_key, r.id
        "#,
    )?;
    let rows = stmt.query_map(
        rusqlite::params![project_id, design_version_id, work_unit_id],
        |row| {
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
        },
    )?;
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    Ok(records)
}

fn list_work_stale_records(root: &Path, work_unit_id: i64) -> Result<Vec<StaleRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut records = Vec::new();
    collect_work_stale_rows(
        &conn,
        project_id,
        work_unit_id,
        "task_derivation",
        r#"
        select td.id, dr.requirement_key
        from task_derivations td
        join design_requirements dr on dr.id = td.design_requirement_id
        join tasks t on t.id = td.task_id
        where td.project_id = ?1
          and t.work_unit_id = ?2
          and td.status = 'stale'
        order by td.id
        "#,
        &mut records,
    )?;
    collect_work_stale_rows(
        &conn,
        project_id,
        work_unit_id,
        "checklist",
        r#"
        select c.id, c.title
        from checklists c
        where c.project_id = ?1
          and c.work_unit_id = ?2
          and c.status = 'stale'
        order by c.id
        "#,
        &mut records,
    )?;
    collect_work_stale_rows(
        &conn,
        project_id,
        work_unit_id,
        "validation_gate",
        r#"
        select vg.id, vg.gate_key
        from validation_gates vg
        left join tasks t on t.id = vg.task_id
        where vg.project_id = ?1
          and coalesce(vg.work_unit_id, t.work_unit_id) = ?2
          and vg.status = 'stale'
        order by vg.id
        "#,
        &mut records,
    )?;
    collect_work_stale_rows(
        &conn,
        project_id,
        work_unit_id,
        "coverage_item",
        r#"
        select c.id, dr.requirement_key
        from coverage_items c
        join design_requirements dr on dr.id = c.design_requirement_id
        left join tasks t on t.id = c.task_id
        where c.project_id = ?1
          and coalesce(c.work_unit_id, t.work_unit_id) = ?2
          and c.status = 'stale'
        order by c.id
        "#,
        &mut records,
    )?;
    Ok(records)
}

fn collect_work_stale_rows(
    conn: &rusqlite::Connection,
    project_id: i64,
    work_unit_id: i64,
    record_type: &str,
    sql: &str,
    output: &mut Vec<StaleRecord>,
) -> Result<()> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(rusqlite::params![project_id, work_unit_id], |row| {
        Ok(StaleRecord {
            record_type: record_type.to_string(),
            id: row.get(0)?,
            label: row.get(1)?,
        })
    })?;
    for row in rows {
        output.push(row?);
    }
    Ok(())
}

fn evidence_detail(record: &ImplementationEvidenceRecord) -> String {
    if let Some(commit_sha) = &record.commit_sha {
        return format!("commit={commit_sha}");
    }
    if let Some(file_path) = &record.file_path {
        let line = record
            .line_ref
            .as_ref()
            .map(|value| format!(":{value}"))
            .unwrap_or_default();
        return format!("file={file_path}{line}");
    }
    if let Some(symbol) = &record.symbol {
        return format!("symbol={symbol}");
    }
    if let Some(artifact_path) = &record.artifact_path {
        return format!("artifact={artifact_path}");
    }
    "detail=-".to_string()
}
