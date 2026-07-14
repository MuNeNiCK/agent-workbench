mod integrity;

use integrity::*;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, DatabaseName, OptionalExtension, TransactionBehavior, params};

use crate::db::{default_ledger_path, migrate, open_ledger};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationLinkChange {
    pub validation_run_id: i64,
    pub entity_type: String,
    pub entity_id: i64,
    pub field_name: String,
    pub before_value: Option<String>,
    pub after_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationLinkRunDiagnosis {
    pub validation_run_id: i64,
    pub repairable: bool,
    pub reasons: Vec<String>,
    pub changes: Vec<ValidationLinkChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationLinkDiagnosis {
    pub repairable: bool,
    pub runs: Vec<ValidationLinkRunDiagnosis>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationLinkRepairOutcome {
    pub repair_run_id: Option<i64>,
    pub backup_path: Option<PathBuf>,
    pub repaired_validation_run_count: usize,
    pub change_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationLinkAuditChange {
    pub validation_run_id: i64,
    pub entity_type: String,
    pub entity_id: i64,
    pub field_name: String,
    pub before_value: Option<String>,
    pub after_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationLinkAuditRun {
    pub repair_run_id: i64,
    pub backup_path: String,
    pub repaired_validation_run_count: i64,
    pub change_count: i64,
    pub created_at: String,
    pub changes: Vec<ValidationLinkAuditChange>,
}

#[derive(Debug, Clone)]
struct RunRow {
    id: i64,
    project_id: Option<i64>,
    validation_gate_id: i64,
    work_unit_id: Option<i64>,
    task_id: Option<i64>,
    command_usage_id: Option<i64>,
    repository_snapshot_id: Option<i64>,
    command: Option<String>,
    acceptance_record_id: Option<i64>,
}

#[derive(Debug, Clone)]
struct GateRow {
    project_id: i64,
    work_unit_id: Option<i64>,
    task_id: Option<i64>,
}

#[derive(Debug, Clone)]
struct CommandUsageRow {
    project_id: Option<i64>,
    work_unit_id: Option<i64>,
    repository_snapshot_id: Option<i64>,
    command: String,
}

#[derive(Debug, Clone)]
struct PlannedArtifact {
    id: i64,
    project_id: i64,
    command_usage_id: Option<i64>,
    repository_snapshot_id: Option<i64>,
}

#[derive(Debug, Clone)]
struct PlannedRun {
    id: i64,
    project_id: i64,
    work_unit_id: Option<i64>,
    task_id: Option<i64>,
    command_usage_id: Option<i64>,
    repository_snapshot_id: Option<i64>,
    command: Option<String>,
    artifacts: Vec<PlannedArtifact>,
    acceptance_projects: BTreeMap<i64, i64>,
    diagnosis: ValidationLinkRunDiagnosis,
}

#[derive(Debug)]
struct ScanOutcome {
    diagnosis: ValidationLinkDiagnosis,
    plans: Vec<PlannedRun>,
}

pub fn diagnose_validation_links(root: &Path) -> Result<ValidationLinkDiagnosis> {
    let ledger_path = require_ledger(root)?;
    let conn = open_ledger(&ledger_path)?;
    Ok(scan_validation_links(&conn)?.diagnosis)
}

pub fn repair_validation_links(root: &Path) -> Result<ValidationLinkRepairOutcome> {
    repair_validation_links_with_backup_notice(root, |_| Ok(()))
}

pub fn repair_validation_links_with_backup_notice<F>(
    root: &Path,
    backup_notice: F,
) -> Result<ValidationLinkRepairOutcome>
where
    F: FnOnce(&Path) -> Result<()>,
{
    let ledger_path = require_ledger(root)?;
    let mut conn = open_ledger(&ledger_path)?;

    let initial = scan_validation_links(&conn)?;
    if initial.diagnosis.runs.is_empty() {
        return Ok(ValidationLinkRepairOutcome {
            repair_run_id: None,
            backup_path: None,
            repaired_validation_run_count: 0,
            change_count: 0,
        });
    }
    require_repairable(&initial.diagnosis)?;

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let scan = scan_validation_links(&tx)?;
    if scan.diagnosis.runs.is_empty() {
        tx.rollback()?;
        return Ok(ValidationLinkRepairOutcome {
            repair_run_id: None,
            backup_path: None,
            repaired_validation_run_count: 0,
            change_count: 0,
        });
    }
    require_repairable(&scan.diagnosis)?;

    let backup_path = next_backup_path(root)?;
    if let Some(parent) = backup_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create backup directory {}", parent.display()))?;
    }
    // The repair transaction holds SQLite's reserved writer lock. A distinct
    // read connection can still take a stable snapshot, while no competing
    // writer can change the source between backup and the first mutation.
    let backup_source = open_ledger(&ledger_path)?;
    backup_source
        .backup(DatabaseName::Main, &backup_path, None)
        .with_context(|| format!("failed to create backup {}", backup_path.display()))?;
    drop(backup_source);
    backup_notice(&backup_path)
        .context("failed to report pre-repair backup; no mutation was made")?;

    let repaired_validation_run_count = scan.plans.len();
    let changes = scan
        .plans
        .iter()
        .flat_map(|plan| plan.diagnosis.changes.iter().cloned())
        .collect::<Vec<_>>();

    let repair_result = (|| -> Result<i64> {
        ensure_audit_schema(&tx)?;
        for plan in &scan.plans {
            tx.execute(
                r#"
                update validation_runs
                set project_id = ?1,
                    work_unit_id = ?2,
                    task_id = ?3,
                    command_usage_id = ?4,
                    repository_snapshot_id = ?5,
                    command = ?6
                where id = ?7
                "#,
                params![
                    plan.project_id,
                    plan.work_unit_id,
                    plan.task_id,
                    plan.command_usage_id,
                    plan.repository_snapshot_id,
                    plan.command,
                    plan.id,
                ],
            )?;

            for (acceptance_id, project_id) in &plan.acceptance_projects {
                tx.execute(
                    "update acceptance_records set project_id = ?1 where id = ?2",
                    params![project_id, acceptance_id],
                )?;
            }
            for artifact in &plan.artifacts {
                tx.execute(
                    r#"
                    update artifacts
                    set project_id = ?1,
                        command_usage_id = ?2,
                        repository_snapshot_id = ?3
                    where id = ?4
                    "#,
                    params![
                        artifact.project_id,
                        artifact.command_usage_id,
                        artifact.repository_snapshot_id,
                        artifact.id,
                    ],
                )?;
            }
        }

        tx.execute(
            r#"
            insert into validation_link_repair_runs(
                backup_path, repaired_validation_run_count, change_count, created_at
            ) values (?1, ?2, ?3, current_timestamp)
            "#,
            params![
                backup_path.display().to_string(),
                repaired_validation_run_count as i64,
                changes.len() as i64,
            ],
        )?;
        let repair_run_id = tx.last_insert_rowid();
        for change in &changes {
            tx.execute(
                r#"
                insert into validation_link_repair_changes(
                    repair_run_id, validation_run_id, entity_type, entity_id,
                    field_name, before_value, after_value, created_at
                ) values (?1, ?2, ?3, ?4, ?5, ?6, ?7, current_timestamp)
                "#,
                params![
                    repair_run_id,
                    change.validation_run_id,
                    change.entity_type,
                    change.entity_id,
                    change.field_name,
                    change.before_value,
                    change.after_value,
                ],
            )?;
        }

        migrate(&tx).context("normal migration failed")?;
        validate_database_integrity(&tx).context("final integrity validation failed")?;
        Ok(repair_run_id)
    })();
    let repair_run_id = repair_result.with_context(|| {
        format!(
            "validation-link repair was rolled back; pre-repair backup: {}",
            backup_path.display()
        )
    })?;
    tx.commit().with_context(|| {
        format!(
            "validation-link repair commit failed and was rolled back; pre-repair backup: {}",
            backup_path.display()
        )
    })?;

    Ok(ValidationLinkRepairOutcome {
        repair_run_id: Some(repair_run_id),
        backup_path: Some(backup_path),
        repaired_validation_run_count,
        change_count: changes.len(),
    })
}

pub fn list_validation_link_audit(root: &Path) -> Result<Vec<ValidationLinkAuditRun>> {
    let ledger_path = require_ledger(root)?;
    let conn = open_ledger(&ledger_path)?;
    if !table_exists(&conn, "validation_link_repair_runs")? {
        return Ok(Vec::new());
    }
    let mut statement = conn.prepare(
        r#"
        select id, backup_path, repaired_validation_run_count, change_count, created_at
        from validation_link_repair_runs
        order by id
        "#,
    )?;
    let rows = statement.query_map([], |row| {
        Ok(ValidationLinkAuditRun {
            repair_run_id: row.get(0)?,
            backup_path: row.get(1)?,
            repaired_validation_run_count: row.get(2)?,
            change_count: row.get(3)?,
            created_at: row.get(4)?,
            changes: Vec::new(),
        })
    })?;
    let mut runs = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    for run in &mut runs {
        let mut changes = conn.prepare(
            r#"
            select validation_run_id, entity_type, entity_id, field_name,
                   before_value, after_value
            from validation_link_repair_changes
            where repair_run_id = ?1
            order by id
            "#,
        )?;
        run.changes = changes
            .query_map([run.repair_run_id], |row| {
                Ok(ValidationLinkAuditChange {
                    validation_run_id: row.get(0)?,
                    entity_type: row.get(1)?,
                    entity_id: row.get(2)?,
                    field_name: row.get(3)?,
                    before_value: row.get(4)?,
                    after_value: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
    }
    Ok(runs)
}

fn require_ledger(root: &Path) -> Result<PathBuf> {
    let ledger_path = default_ledger_path(root);
    if !ledger_path.exists() {
        bail!("project is not initialized; run agent-workbench init");
    }
    Ok(ledger_path)
}

fn require_repairable(diagnosis: &ValidationLinkDiagnosis) -> Result<()> {
    if diagnosis.repairable {
        return Ok(());
    }
    let ids = diagnosis
        .runs
        .iter()
        .filter(|run| !run.repairable)
        .map(|run| run.validation_run_id.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    bail!(
        "validation-link repair refused: unrepairable validation runs: {ids}; run `agent-workbench doctor validation-links` for reasons"
    )
}

fn scan_validation_links(conn: &Connection) -> Result<ScanOutcome> {
    require_doctor_schema(conn)?;
    let unknown_references = unknown_validation_run_references(conn)?;
    let mut statement = conn.prepare(
        r#"
        select id, project_id, validation_gate_id, work_unit_id, task_id,
               command_usage_id, repository_snapshot_id, command,
               acceptance_record_id
        from validation_runs
        order by id
        "#,
    )?;
    let rows = statement.query_map([], |row| {
        Ok(RunRow {
            id: row.get(0)?,
            project_id: row.get(1)?,
            validation_gate_id: row.get(2)?,
            work_unit_id: row.get(3)?,
            task_id: row.get(4)?,
            command_usage_id: row.get(5)?,
            repository_snapshot_id: row.get(6)?,
            command: row.get(7)?,
            acceptance_record_id: row.get(8)?,
        })
    })?;

    let mut plans = Vec::new();
    for run in rows.collect::<rusqlite::Result<Vec<_>>>()? {
        if let Some(plan) = plan_run(conn, run, &unknown_references)? {
            plans.push(plan);
        }
    }

    let mut desired_acceptance_projects = BTreeMap::<i64, (i64, i64)>::new();
    let mut conflicts = Vec::new();
    for plan in &plans {
        for (acceptance_id, project_id) in &plan.acceptance_projects {
            if let Some((prior_project, prior_run)) =
                desired_acceptance_projects.insert(*acceptance_id, (*project_id, plan.id))
                && prior_project != *project_id
            {
                conflicts.push((*acceptance_id, prior_run, plan.id));
            }
        }
    }
    for (acceptance_id, first_run, second_run) in conflicts {
        for plan in &mut plans {
            if plan.id == first_run || plan.id == second_run {
                plan.diagnosis.repairable = false;
                plan.diagnosis.reasons.push(format!(
                    "acceptance_record:{acceptance_id} is shared by runs requiring different target projects"
                ));
            }
        }
    }

    let runs = plans
        .iter()
        .map(|plan| plan.diagnosis.clone())
        .collect::<Vec<_>>();
    let repairable = runs.iter().all(|run| run.repairable);
    Ok(ScanOutcome {
        diagnosis: ValidationLinkDiagnosis { repairable, runs },
        plans,
    })
}

fn plan_run(
    conn: &Connection,
    run: RunRow,
    unknown_references: &[(String, String)],
) -> Result<Option<PlannedRun>> {
    let mut repairable = true;
    let mut reasons = Vec::new();
    let mut changes = BTreeMap::<(String, i64, String), ValidationLinkChange>::new();
    let gate = conn
        .query_row(
            "select project_id, work_unit_id, task_id from validation_gates where id = ?1",
            [run.validation_gate_id],
            |row| {
                Ok(GateRow {
                    project_id: row.get(0)?,
                    work_unit_id: row.get(1)?,
                    task_id: row.get(2)?,
                })
            },
        )
        .optional()?;
    let Some(gate) = gate else {
        return Ok(Some(PlannedRun {
            id: run.id,
            project_id: run.project_id.unwrap_or_default(),
            work_unit_id: run.work_unit_id,
            task_id: run.task_id,
            command_usage_id: run.command_usage_id,
            repository_snapshot_id: run.repository_snapshot_id,
            command: run.command,
            artifacts: Vec::new(),
            acceptance_projects: BTreeMap::new(),
            diagnosis: ValidationLinkRunDiagnosis {
                validation_run_id: run.id,
                repairable: false,
                reasons: vec![format!(
                    "validation_gate:{} is missing; no authoritative repair exists",
                    run.validation_gate_id
                )],
                changes: Vec::new(),
            },
        }));
    };

    if !project_exists(conn, gate.project_id)? {
        repairable = false;
        reasons.push(format!(
            "validation_gate:{} references missing project:{}",
            run.validation_gate_id, gate.project_id
        ));
    }
    if let Some(work_unit_id) = gate.work_unit_id
        && work_unit_project(conn, work_unit_id)? != Some(gate.project_id)
    {
        repairable = false;
        reasons.push(format!(
            "validation_gate:{} has invalid work_unit:{}",
            run.validation_gate_id, work_unit_id
        ));
    }
    if let Some(task_id) = gate.task_id {
        let task_scope = task_scope(conn, task_id)?;
        if task_scope != Some((gate.project_id, gate.work_unit_id)) {
            repairable = false;
            reasons.push(format!(
                "validation_gate:{} has invalid task:{} scope",
                run.validation_gate_id, task_id
            ));
        }
    }

    add_i64_change(
        &mut changes,
        run.id,
        "validation_run",
        run.id,
        "project_id",
        run.project_id,
        Some(gate.project_id),
    );
    add_i64_change(
        &mut changes,
        run.id,
        "validation_run",
        run.id,
        "work_unit_id",
        run.work_unit_id,
        gate.work_unit_id,
    );
    add_i64_change(
        &mut changes,
        run.id,
        "validation_run",
        run.id,
        "task_id",
        run.task_id,
        gate.task_id,
    );

    let command_usage = match run.command_usage_id {
        Some(id) => conn
            .query_row(
                r#"
                select project_id, work_unit_id, repository_snapshot_id, command
                from command_usages where id = ?1
                "#,
                [id],
                |row| {
                    Ok(CommandUsageRow {
                        project_id: row.get(0)?,
                        work_unit_id: row.get(1)?,
                        repository_snapshot_id: row.get(2)?,
                        command: row.get(3)?,
                    })
                },
            )
            .optional()?
            .map(|usage| (id, usage)),
        None => None,
    };
    let command_usage_compatible = command_usage.as_ref().is_some_and(|(_, usage)| {
        usage.project_id == Some(gate.project_id)
            && (usage.work_unit_id.is_none() || usage.work_unit_id == gate.work_unit_id)
    });
    let desired_command_usage_id = if command_usage_compatible {
        run.command_usage_id
    } else {
        if let Some(id) = run.command_usage_id {
            reasons.push(format!(
                "command_usage:{id} does not match the gate-derived project/work scope and will be detached"
            ));
        }
        None
    };
    add_i64_change(
        &mut changes,
        run.id,
        "validation_run",
        run.id,
        "command_usage_id",
        run.command_usage_id,
        desired_command_usage_id,
    );

    let desired_command = if desired_command_usage_id.is_none()
        && run
            .command
            .as_deref()
            .is_none_or(|value| value.trim().is_empty())
    {
        command_usage
            .as_ref()
            .map(|(_, usage)| usage.command.clone())
            .filter(|value| !value.trim().is_empty())
            .or_else(|| run.command.clone())
    } else {
        run.command.clone()
    };
    add_text_change(
        &mut changes,
        run.id,
        "validation_run",
        run.id,
        "command",
        run.command.clone(),
        desired_command.clone(),
    );

    let run_snapshot_is_compatible = match run.repository_snapshot_id {
        Some(snapshot_id) => snapshot_project(conn, snapshot_id)? == Some(gate.project_id),
        None => false,
    };
    let desired_snapshot_id = if let Some((_, usage)) = command_usage
        .as_ref()
        .filter(|_| desired_command_usage_id.is_some())
    {
        if let Some(snapshot_id) = usage.repository_snapshot_id {
            if snapshot_project(conn, snapshot_id)? == Some(gate.project_id) {
                if run.repository_snapshot_id == Some(snapshot_id) {
                    Some(snapshot_id)
                } else {
                    None
                }
            } else {
                repairable = false;
                reasons.push(format!(
                    "retained command usage references repository_snapshot:{snapshot_id} outside the gate-derived project"
                ));
                run.repository_snapshot_id
            }
        } else if run_snapshot_is_compatible {
            run.repository_snapshot_id
        } else {
            None
        }
    } else if run_snapshot_is_compatible {
        run.repository_snapshot_id
    } else {
        if let Some(snapshot_id) = run.repository_snapshot_id {
            reasons.push(format!(
                "repository_snapshot:{snapshot_id} does not belong to the gate-derived project and will be detached"
            ));
        }
        None
    };
    if desired_command_usage_id.is_some()
        && command_usage
            .as_ref()
            .and_then(|(_, usage)| usage.repository_snapshot_id)
            .is_some()
        && desired_snapshot_id != run.repository_snapshot_id
    {
        reasons.push(
            "validation-run snapshot conflicts with the retained command usage snapshot and will be detached"
                .to_string(),
        );
    }
    add_i64_change(
        &mut changes,
        run.id,
        "validation_run",
        run.id,
        "repository_snapshot_id",
        run.repository_snapshot_id,
        desired_snapshot_id,
    );

    let mut acceptance_projects = BTreeMap::new();
    if let Some(acceptance_id) = run.acceptance_record_id {
        match acceptance_row(conn, acceptance_id)? {
            None => {
                repairable = false;
                reasons.push(format!(
                    "acceptance_record:{acceptance_id} is missing and cannot be detached automatically"
                ));
            }
            Some(acceptance) if acceptance.project_id == gate.project_id => {}
            Some(acceptance)
                if acceptance.target_type == "validation_run"
                    && acceptance.validation_run_id == Some(run.id)
                    && authority_matches_target(
                        conn,
                        acceptance.authority_event_id,
                        gate.project_id,
                    )? =>
            {
                plan_acceptance_project(
                    &mut acceptance_projects,
                    &mut changes,
                    run.id,
                    acceptance.id,
                    acceptance.project_id,
                    gate.project_id,
                );
            }
            Some(_) => {
                repairable = false;
                reasons.push(format!(
                    "acceptance_record:{acceptance_id} has an unrelated target or cross-project approval authority"
                ));
            }
        }
    }

    let mut artifacts = Vec::new();
    if table_exists(conn, "artifacts")? {
        let mut artifact_statement = conn.prepare(
            r#"
            select id, project_id, command_usage_id, repository_snapshot_id
            from artifacts where validation_run_id = ?1 order by id
            "#,
        )?;
        let artifact_rows = artifact_statement.query_map([run.id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, Option<i64>>(3)?,
            ))
        })?;
        for artifact in artifact_rows.collect::<rusqlite::Result<Vec<_>>>()? {
            add_i64_change(
                &mut changes,
                run.id,
                "artifact",
                artifact.0,
                "project_id",
                Some(artifact.1),
                Some(gate.project_id),
            );
            add_i64_change(
                &mut changes,
                run.id,
                "artifact",
                artifact.0,
                "command_usage_id",
                artifact.2,
                desired_command_usage_id,
            );
            add_i64_change(
                &mut changes,
                run.id,
                "artifact",
                artifact.0,
                "repository_snapshot_id",
                artifact.3,
                desired_snapshot_id,
            );
            artifacts.push(PlannedArtifact {
                id: artifact.0,
                project_id: gate.project_id,
                command_usage_id: desired_command_usage_id,
                repository_snapshot_id: desired_snapshot_id,
            });
        }
    }

    if table_exists(conn, "acceptance_records")? {
        let mut acceptance_statement = conn.prepare(
            r#"
            select id, project_id, approved_by_authority_event_id
            from acceptance_records
            where target_type = 'validation_run' and validation_run_id = ?1
            order by id
            "#,
        )?;
        let acceptance_rows = acceptance_statement.query_map([run.id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        })?;
        for acceptance in acceptance_rows.collect::<rusqlite::Result<Vec<_>>>()? {
            if acceptance.1 == gate.project_id {
                continue;
            }
            if authority_matches_target(conn, acceptance.2, gate.project_id)? {
                plan_acceptance_project(
                    &mut acceptance_projects,
                    &mut changes,
                    run.id,
                    acceptance.0,
                    acceptance.1,
                    gate.project_id,
                );
            } else {
                repairable = false;
                reasons.push(format!(
                    "acceptance_record:{} has cross-project approval authority and cannot follow validation_run:{}",
                    acceptance.0, run.id
                ));
            }
        }
    }

    if !changes.is_empty() {
        for (table, column) in unknown_references {
            let sql = format!(
                "select count(*) from {} where {} = ?1",
                quote_identifier(table),
                quote_identifier(column)
            );
            let count: i64 = conn.query_row(&sql, [run.id], |row| row.get(0))?;
            if count > 0 {
                repairable = false;
                reasons.push(format!(
                    "unknown dependent relation {table}.{column} references this run; no authoritative repair rule exists"
                ));
            }
        }
    }

    if changes.is_empty() && repairable {
        return Ok(None);
    }
    if changes.is_empty() && reasons.is_empty() {
        return Ok(None);
    }
    if !changes.is_empty() {
        reasons.insert(
            0,
            "validation run or its dependent evidence does not match the validation gate authority"
                .to_string(),
        );
    }
    let diagnosis = ValidationLinkRunDiagnosis {
        validation_run_id: run.id,
        repairable,
        reasons,
        changes: changes.into_values().collect(),
    };
    Ok(Some(PlannedRun {
        id: run.id,
        project_id: gate.project_id,
        work_unit_id: gate.work_unit_id,
        task_id: gate.task_id,
        command_usage_id: desired_command_usage_id,
        repository_snapshot_id: desired_snapshot_id,
        command: desired_command,
        artifacts,
        acceptance_projects,
        diagnosis,
    }))
}

#[derive(Debug)]
struct AcceptanceRow {
    id: i64,
    project_id: i64,
    target_type: String,
    validation_run_id: Option<i64>,
    authority_event_id: Option<i64>,
}

fn acceptance_row(conn: &Connection, id: i64) -> Result<Option<AcceptanceRow>> {
    if !table_exists(conn, "acceptance_records")? {
        return Ok(None);
    }
    conn.query_row(
        r#"
        select id, project_id, target_type, validation_run_id,
               approved_by_authority_event_id
        from acceptance_records where id = ?1
        "#,
        [id],
        |row| {
            Ok(AcceptanceRow {
                id: row.get(0)?,
                project_id: row.get(1)?,
                target_type: row.get(2)?,
                validation_run_id: row.get(3)?,
                authority_event_id: row.get(4)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn authority_matches_target(
    conn: &Connection,
    authority_event_id: Option<i64>,
    target_project_id: i64,
) -> Result<bool> {
    let Some(authority_event_id) = authority_event_id else {
        return Ok(true);
    };
    Ok(conn
        .query_row(
            "select project_id from authority_events where id = ?1",
            [authority_event_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        == Some(target_project_id))
}

fn plan_acceptance_project(
    projects: &mut BTreeMap<i64, i64>,
    changes: &mut BTreeMap<(String, i64, String), ValidationLinkChange>,
    validation_run_id: i64,
    acceptance_id: i64,
    before: i64,
    after: i64,
) {
    projects.insert(acceptance_id, after);
    add_i64_change(
        changes,
        validation_run_id,
        "acceptance_record",
        acceptance_id,
        "project_id",
        Some(before),
        Some(after),
    );
}

fn add_i64_change(
    changes: &mut BTreeMap<(String, i64, String), ValidationLinkChange>,
    validation_run_id: i64,
    entity_type: &str,
    entity_id: i64,
    field_name: &str,
    before: Option<i64>,
    after: Option<i64>,
) {
    add_text_change(
        changes,
        validation_run_id,
        entity_type,
        entity_id,
        field_name,
        before.map(|value| value.to_string()),
        after.map(|value| value.to_string()),
    );
}

fn add_text_change(
    changes: &mut BTreeMap<(String, i64, String), ValidationLinkChange>,
    validation_run_id: i64,
    entity_type: &str,
    entity_id: i64,
    field_name: &str,
    before_value: Option<String>,
    after_value: Option<String>,
) {
    if before_value == after_value {
        return;
    }
    let change = ValidationLinkChange {
        validation_run_id,
        entity_type: entity_type.to_string(),
        entity_id,
        field_name: field_name.to_string(),
        before_value,
        after_value,
    };
    changes.insert(
        (
            change.entity_type.clone(),
            change.entity_id,
            change.field_name.clone(),
        ),
        change,
    );
}
