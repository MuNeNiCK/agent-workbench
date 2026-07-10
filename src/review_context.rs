use std::collections::HashSet;
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
    pub phase_id: Option<i64>,
}

pub struct ReviewContextDocument {
    pub context_ref: String,
    pub text: String,
}

pub fn render_finding_fix_context(
    root: &Path,
    finding_id: i64,
    closure_id: i64,
    attempt_id: i64,
) -> Result<ReviewContextDocument> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let context_ref = crate::review::finding_fix_context_ref(finding_id, closure_id, attempt_id);
    let text = conn
        .query_row(
            r#"
            select p.id, p.review_type, p.stage, f.description, f.severity,
                   c.design_invariant, c.affected_surfaces, c.fix_plan,
                   c.verification_plan, c.tests_or_gates, a.attempt_number,
                   a.implementation_evidence, a.tests_or_gates,
                   a.closed_by_commit, a.review_run_high_watermark
            from closure_attempts a
            join closures c on c.id = a.closure_id
            join findings f on f.id = c.finding_id
            join review_runs r on r.id = f.review_run_id
            join review_plans p on p.id = r.review_plan_id
            where f.id = ?1 and c.id = ?2 and a.id = ?3
              and f.project_id = ?4 and c.project_id = ?4 and a.project_id = ?4
            "#,
            rusqlite::params![finding_id, closure_id, attempt_id, project_id],
            |row| {
                Ok(format!(
                    "review_context: finding-fix\ncontext_ref: {context_ref}\nfinding_id: {finding_id}\nclosure_id: {closure_id}\nattempt_id: {attempt_id}\nreview_plan_id: {}\nreview_type: {}\nstage: {}\nseverity: {}\ndescription: {}\ninvariant: {}\naffected_surfaces: {}\nfix_plan: {}\nverification_plan: {}\ncontract_tests_or_gates: {}\nattempt_number: {}\nimplementation_evidence: {}\nattempt_tests_or_gates: {}\ncommit: {}\nreview_run_high_watermark: {}\n",
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?.unwrap_or_else(|| "-".to_string()),
                    row.get::<_, Option<String>>(7)?.unwrap_or_else(|| "-".to_string()),
                    row.get::<_, Option<String>>(8)?.unwrap_or_else(|| "-".to_string()),
                    row.get::<_, Option<String>>(9)?.unwrap_or_else(|| "-".to_string()),
                    row.get::<_, i64>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, Option<String>>(13)?.unwrap_or_else(|| "-".to_string()),
                    row.get::<_, i64>(14)?,
                ))
            },
        )
        .map_err(anyhow::Error::from)?;
    Ok(ReviewContextDocument { context_ref, text })
}

pub fn review_context_ref(
    kind: &str,
    design_version_id: Option<i64>,
    work_unit_id: Option<i64>,
) -> String {
    review_context_ref_with_phase(kind, design_version_id, work_unit_id, None)
}

pub fn review_context_ref_with_phase(
    kind: &str,
    design_version_id: Option<i64>,
    work_unit_id: Option<i64>,
    phase_id: Option<i64>,
) -> String {
    let design = design_version_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "-".to_string());
    let work = work_unit_id
        .map(|id| id.to_string())
        .unwrap_or_else(|| "-".to_string());
    match phase_id {
        Some(phase_id) => {
            format!("review-context:{kind}:design={design}:work={work}:phase={phase_id}")
        }
        None => format!("review-context:{kind}:design={design}:work={work}"),
    }
}

pub fn render_review_context(
    root: &Path,
    query: ReviewContextQuery<'_>,
) -> Result<ReviewContextDocument> {
    let context_ref = review_context_ref_with_phase(
        query.kind,
        query.design_version_id,
        query.work_unit_id,
        query.phase_id,
    );
    let mut output = String::new();
    writeln!(output, "review_context: {}", query.kind)?;
    writeln!(output, "context_ref: {context_ref}")?;
    if let Some(phase_id) = query.phase_id {
        render_phase_header(root, phase_id, query.work_unit_id, &mut output)?;
    }

    if let Some(design_version_id) = query.design_version_id {
        writeln!(output, "design_version_id: {design_version_id}")?;
        render_design_context(
            root,
            query.kind,
            design_version_id,
            query.work_unit_id,
            query.phase_id,
            &mut output,
        )?;
    }
    if let Some(work_unit_id) = query.work_unit_id {
        writeln!(output, "work_unit_id: {work_unit_id}")?;
        render_work_context(root, work_unit_id, query.phase_id, &mut output)?;
    }
    render_stale_context(root, query.work_unit_id, query.phase_id, &mut output)?;

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
            from review_runs r
            where r.review_plan_id = ?1
              and r.target_ref = ?2
              and r.run_type = 'fresh'
              and r.run_purpose = 'new_unbiased_review'
              and r.clean_run = 1
              and r.status = 'completed'
              and (
                  (
                      r.review_provenance = 'external_agent'
                      and coalesce(r.review_provenance_ref, '') != ''
                      and exists (
                          select 1
                          from review_agent_invocations i
                          where i.review_run_id = r.id
                            and i.external_agent_id is not null
                            and i.external_agent_id != ''
                      )
                  )
                  or (
                      r.review_provenance = 'human_review'
                      and coalesce(r.review_provenance_ref, '') != ''
                  )
              )
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
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'review_plan'
              and ar.review_plan_id = rp.id
              and ar.status = 'approved'
              and ar.acceptance_type in ('explicit_exception', 'stale_accepted')
          )
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
    phase_id: Option<i64>,
    output: &mut String,
) -> Result<()> {
    let phase_tasks = match phase_id {
        Some(phase_id) => Some(phase_task_set(root, phase_id)?),
        None => None,
    };
    let requirements = if let Some(phase_id) = phase_id {
        list_phase_context_requirements(root, design_version_id, phase_id)?
    } else {
        list_context_requirements(
            root,
            design_version_id,
            work_unit_id.filter(|_| review_context_kind_is_work_scoped(kind)),
        )?
    };
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

    let mut derivations = list_task_derivations(
        root,
        TaskDerivationListQuery {
            design_version_id,
            work_unit_id,
        },
    )?;
    if let Some(tasks) = &phase_tasks {
        derivations.retain(|record| tasks.contains(&record.task_id));
    }
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

    let mut gates = list_validation_gate_context(
        root,
        ValidationGateContextQuery {
            design_version_id,
            work_unit_id,
        },
    )?;
    if let Some(tasks) = &phase_tasks {
        gates.retain(|record| {
            record
                .task_id
                .is_some_and(|task_id| tasks.contains(&task_id))
        });
    }
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

    let mut evidence = list_implementation_evidence(
        root,
        ImplementationEvidenceListQuery {
            task_id: None,
            design_version_id: Some(design_version_id),
            work_unit_id,
        },
    )?;
    if let Some(tasks) = &phase_tasks {
        evidence.retain(|record| {
            record
                .task_id
                .is_some_and(|task_id| tasks.contains(&task_id))
        });
    }
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

    let mut coverage = list_coverage_items(
        root,
        CoverageItemListQuery {
            design_version_id,
            status: None,
            work_unit_id,
        },
    )?;
    if let Some(tasks) = &phase_tasks {
        coverage.retain(|record| match record.task_id {
            Some(task_id) => tasks.contains(&task_id),
            None => true,
        });
    }
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

fn render_work_context(
    root: &Path,
    work_unit_id: i64,
    phase_id: Option<i64>,
    output: &mut String,
) -> Result<()> {
    let mut tasks = list_tasks(
        root,
        TaskListQuery {
            status: None,
            work_unit_id: Some(work_unit_id),
        },
    )?;
    if let Some(phase_id) = phase_id {
        let phase_tasks = phase_task_set(root, phase_id)?;
        tasks.retain(|task| phase_tasks.contains(&task.id));
    }
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

fn render_stale_context(
    root: &Path,
    work_unit_id: Option<i64>,
    phase_id: Option<i64>,
    output: &mut String,
) -> Result<()> {
    let stale = match work_unit_id {
        Some(_) if phase_id.is_some() => list_phase_stale_records(root, phase_id.unwrap())?,
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

fn render_phase_header(
    root: &Path,
    phase_id: i64,
    expected_work_unit_id: Option<i64>,
    output: &mut String,
) -> Result<()> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let (work_unit_id, key, title, status): (i64, String, String, String) = conn.query_row(
        r#"
            select work_unit_id, phase_key, title, status
            from work_phases
            where id = ?1 and project_id = ?2
            "#,
        rusqlite::params![phase_id, project_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    if let Some(expected_work_unit_id) = expected_work_unit_id
        && expected_work_unit_id != work_unit_id
    {
        anyhow::bail!("phase does not belong to requested work unit");
    }
    writeln!(output, "phase_id: {phase_id}")?;
    writeln!(output, "phase_key: {key}")?;
    writeln!(output, "phase_title: {title}")?;
    writeln!(output, "phase_status: {status}")?;
    Ok(())
}

fn phase_task_set(root: &Path, phase_id: i64) -> Result<HashSet<i64>> {
    let conn = open_existing_project(root)?;
    let mut stmt = conn.prepare(
        "select task_id from work_phase_task_memberships where phase_id = ?1 order by task_id",
    )?;
    let rows = stmt.query_map(rusqlite::params![phase_id], |row| row.get(0))?;
    let mut tasks = HashSet::new();
    for row in rows {
        tasks.insert(row?);
    }
    Ok(tasks)
}

fn list_phase_context_requirements(
    root: &Path,
    design_version_id: i64,
    phase_id: i64,
) -> Result<Vec<DesignRequirementRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut stmt = conn.prepare(
        r#"
        with relevant_requirements as (
            select distinct td.design_requirement_id as id
            from task_derivations td
            join work_phase_task_memberships m on m.task_id = td.task_id
            where m.phase_id = ?3
            union
            select distinct vg.design_requirement_id as id
            from validation_gates vg
            join work_phase_task_memberships m on m.task_id = vg.task_id
            where m.phase_id = ?3
            union
            select distinct e.design_requirement_id as id
            from implementation_evidence e
            join work_phase_task_memberships m on m.task_id = e.task_id
            where m.phase_id = ?3 and e.design_requirement_id is not null
            union
            select distinct c.design_requirement_id as id
            from coverage_items c
            join work_phase_task_memberships m on m.task_id = c.task_id
            where m.phase_id = ?3
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
        rusqlite::params![project_id, design_version_id, phase_id],
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

fn list_phase_stale_records(root: &Path, phase_id: i64) -> Result<Vec<StaleRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut records = Vec::new();
    collect_work_stale_rows(
        &conn,
        project_id,
        phase_id,
        "task_derivation",
        r#"
        select td.id, dr.requirement_key
        from task_derivations td
        join design_requirements dr on dr.id = td.design_requirement_id
        join work_phase_task_memberships m on m.task_id = td.task_id
        where m.phase_id = ?2
          and td.project_id = ?1
          and td.status = 'stale'
        order by td.id
        "#,
        &mut records,
    )?;
    collect_work_stale_rows(
        &conn,
        project_id,
        phase_id,
        "validation_gate",
        r#"
        select vg.id, vg.gate_key
        from validation_gates vg
        join work_phase_task_memberships m on m.task_id = vg.task_id
        where m.phase_id = ?2
          and vg.project_id = ?1
          and vg.status = 'stale'
        order by vg.id
        "#,
        &mut records,
    )?;
    collect_work_stale_rows(
        &conn,
        project_id,
        phase_id,
        "coverage_item",
        r#"
        select c.id, dr.requirement_key
        from coverage_items c
        join design_requirements dr on dr.id = c.design_requirement_id
        join work_phase_task_memberships m on m.task_id = c.task_id
        where m.phase_id = ?2
          and c.project_id = ?1
          and c.status = 'stale'
        order by c.id
        "#,
        &mut records,
    )?;
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
          and not exists (
              select 1
              from acceptance_records ar
              where ar.target_type = 'stale_record'
                and ar.stale_record_type = 'task_derivation'
                and ar.stale_record_id = td.id
                and ar.acceptance_type = 'stale_accepted'
                and ar.status = 'approved'
          )
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
          and not exists (
              select 1
              from acceptance_records ar
              where ar.target_type = 'stale_record'
                and ar.stale_record_type = 'checklist'
                and ar.stale_record_id = c.id
                and ar.acceptance_type = 'stale_accepted'
                and ar.status = 'approved'
          )
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
          and not exists (
              select 1
              from acceptance_records ar
              where ar.target_type = 'stale_record'
                and ar.stale_record_type = 'validation_gate'
                and ar.stale_record_id = vg.id
                and ar.acceptance_type = 'stale_accepted'
                and ar.status = 'approved'
          )
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
          and not exists (
              select 1
              from acceptance_records ar
              where ar.target_type = 'stale_record'
                and ar.stale_record_type = 'coverage_item'
                and ar.stale_record_id = c.id
                and ar.acceptance_type = 'stale_accepted'
                and ar.status = 'approved'
          )
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
