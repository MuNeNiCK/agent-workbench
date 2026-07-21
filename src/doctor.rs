mod integrity;

use integrity::*;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, DatabaseName, OptionalExtension, TransactionBehavior, params};

use crate::db::{default_ledger_path, migrate, open_ledger};
use crate::identity::{CanonicalValue, domain_digest};

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
    pub artifact_ref: String,
    pub expected_project_id: Option<i64>,
    pub current_revision: String,
    pub repairable: bool,
    pub reasons: Vec<String>,
    pub changes: Vec<ValidationLinkChange>,
    pub legal_actions: Vec<String>,
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
pub struct ValidationLinkArtifactOutcome {
    pub artifact_ref: String,
    pub validation_run_id: i64,
    pub operation: String,
    pub result_current: String,
    pub repair_run_id: Option<i64>,
    pub retirement_id: Option<i64>,
    pub backup_path: Option<PathBuf>,
    pub idempotent: bool,
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

pub fn diagnose_validation_link(
    root: &Path,
    artifact_ref: &str,
) -> Result<ValidationLinkDiagnosis> {
    let run_id = parse_validation_artifact_ref(artifact_ref)?;
    let ledger_path = require_ledger(root)?;
    let conn = open_ledger(&ledger_path)?;
    let scan = scan_validation_links(&conn)?;
    let runs = scan
        .diagnosis
        .runs
        .into_iter()
        .filter(|run| run.validation_run_id == run_id)
        .collect::<Vec<_>>();
    if runs.is_empty() {
        let exists: bool = conn.query_row(
            "select exists(select 1 from validation_runs where id=?1)",
            [run_id],
            |row| row.get(0),
        )?;
        if !exists {
            bail!("validation_artifact_not_found: {artifact_ref}");
        }
    }
    Ok(ValidationLinkDiagnosis {
        repairable: runs.iter().all(|run| run.repairable),
        runs,
    })
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
    let _update_guard = crate::update::shared_writer_guard(root)?;
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
        apply_planned_runs(&tx, &scan.plans)?;

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

pub fn repair_validation_link(
    root: &Path,
    artifact_ref: &str,
    expected_project_id: i64,
    expected_current: &str,
) -> Result<ValidationLinkArtifactOutcome> {
    if expected_project_id <= 0 || expected_current.trim().is_empty() {
        bail!("validation_link_repair_input_invalid: project and expected current are required");
    }
    let validation_run_id = parse_validation_artifact_ref(artifact_ref)?;
    let ledger_path = require_ledger(root)?;
    let request_digest = validation_link_request_digest(
        "relink",
        artifact_ref,
        Some(expected_project_id),
        None,
        expected_current,
    );
    let _update_guard = crate::update::shared_writer_guard(root)?;
    let mut conn = open_ledger(&ledger_path)?;
    if let Some(outcome) =
        lookup_validation_link_receipt(&conn, expected_project_id, &request_digest, true)?
    {
        return Ok(outcome);
    }
    let initial = scan_validation_links(&conn)?;
    let initial_plan = exact_planned_run(&initial, validation_run_id)?;
    validate_explicit_repair(
        initial_plan,
        expected_project_id,
        expected_current,
        artifact_ref,
    )?;

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let scan = scan_validation_links(&tx)?;
    let plan = exact_planned_run(&scan, validation_run_id)?;
    validate_explicit_repair(plan, expected_project_id, expected_current, artifact_ref)?;

    let backup_path = create_validation_link_backup(root, &ledger_path)?;
    let changes = plan.diagnosis.changes.clone();
    apply_planned_runs(&tx, std::slice::from_ref(plan))?;
    tx.execute(
        "insert into validation_link_repair_runs(backup_path,repaired_validation_run_count,change_count,created_at) values(?1,1,?2,current_timestamp)",
        params![backup_path.display().to_string(), changes.len() as i64],
    )?;
    let repair_run_id = tx.last_insert_rowid();
    insert_validation_link_changes(&tx, repair_run_id, &changes)?;
    let result_current = validation_link_revision(&tx, validation_run_id)?;
    tx.execute(
        r#"
        insert into validation_link_repair_receipts(
          project_id,artifact_ref,validation_run_id,operation,expected_current,
          result_current,repair_run_id,request_digest,created_at
        ) values(?1,?2,?3,'relink',?4,?5,?6,?7,current_timestamp)
        "#,
        params![
            expected_project_id,
            artifact_ref,
            validation_run_id,
            expected_current,
            result_current,
            repair_run_id,
            request_digest,
        ],
    )?;
    if scan_validation_links(&tx)?
        .plans
        .iter()
        .any(|candidate| candidate.id == validation_run_id)
    {
        bail!("validation-link repair did not clear the exact artifact defect");
    }
    let integrity: String = tx.query_row("pragma integrity_check(1)", [], |row| row.get(0))?;
    if integrity != "ok" {
        bail!("SQLite integrity check failed after validation-link repair: {integrity}");
    }
    tx.commit()?;
    Ok(ValidationLinkArtifactOutcome {
        artifact_ref: artifact_ref.to_string(),
        validation_run_id,
        operation: "relink".to_string(),
        result_current,
        repair_run_id: Some(repair_run_id),
        retirement_id: None,
        backup_path: Some(backup_path),
        idempotent: false,
    })
}

pub fn retire_validation_link(
    root: &Path,
    artifact_ref: &str,
    reason: &str,
    expected_current: &str,
) -> Result<ValidationLinkArtifactOutcome> {
    if reason.trim().is_empty() || expected_current.trim().is_empty() {
        bail!("validation_link_retirement_input_invalid: reason and expected current are required");
    }
    let validation_run_id = parse_validation_artifact_ref(artifact_ref)?;
    let ledger_path = require_ledger(root)?;
    let _update_guard = crate::update::shared_writer_guard(root)?;
    let mut conn = open_ledger(&ledger_path)?;
    let project = crate::db::project_id(&conn)?;
    let request_digest = validation_link_request_digest(
        "retire",
        artifact_ref,
        None,
        Some(reason),
        expected_current,
    );
    if let Some(outcome) = lookup_validation_link_receipt(&conn, project, &request_digest, true)? {
        return Ok(outcome);
    }
    let initial = scan_validation_links(&conn)?;
    let initial_plan = exact_planned_run(&initial, validation_run_id)?;
    validate_explicit_retirement(initial_plan, expected_current, artifact_ref)?;

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let scan = scan_validation_links(&tx)?;
    let plan = exact_planned_run(&scan, validation_run_id)?;
    validate_explicit_retirement(plan, expected_current, artifact_ref)?;
    tx.execute(
        "insert into validation_link_retirements(project_id,validation_run_id,artifact_ref,reason,expected_current,request_digest,created_at) values(?1,?2,?3,?4,?5,?6,current_timestamp)",
        params![project,validation_run_id,artifact_ref,reason,expected_current,request_digest],
    )?;
    let retirement_id = tx.last_insert_rowid();
    let result_current = validation_link_revision(&tx, validation_run_id)?;
    tx.execute(
        r#"
        insert into validation_link_repair_receipts(
          project_id,artifact_ref,validation_run_id,operation,expected_current,
          result_current,retirement_id,request_digest,created_at
        ) values(?1,?2,?3,'retire',?4,?5,?6,?7,current_timestamp)
        "#,
        params![
            project,
            artifact_ref,
            validation_run_id,
            expected_current,
            result_current,
            retirement_id,
            request_digest,
        ],
    )?;
    tx.commit()?;
    Ok(ValidationLinkArtifactOutcome {
        artifact_ref: artifact_ref.to_string(),
        validation_run_id,
        operation: "retire".to_string(),
        result_current,
        repair_run_id: None,
        retirement_id: Some(retirement_id),
        backup_path: None,
        idempotent: false,
    })
}

fn apply_planned_runs(conn: &Connection, plans: &[PlannedRun]) -> Result<()> {
    for plan in plans {
        conn.execute(
            r#"
            update validation_runs
            set project_id=?1,work_unit_id=?2,task_id=?3,command_usage_id=?4,
                repository_snapshot_id=?5,command=?6
            where id=?7
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
            conn.execute(
                "update acceptance_records set project_id=?1 where id=?2",
                params![project_id, acceptance_id],
            )?;
        }
        for artifact in &plan.artifacts {
            conn.execute(
                "update artifacts set project_id=?1,command_usage_id=?2,repository_snapshot_id=?3 where id=?4",
                params![
                    artifact.project_id,
                    artifact.command_usage_id,
                    artifact.repository_snapshot_id,
                    artifact.id,
                ],
            )?;
        }
    }
    Ok(())
}

fn insert_validation_link_changes(
    conn: &Connection,
    repair_run_id: i64,
    changes: &[ValidationLinkChange],
) -> Result<()> {
    for change in changes {
        conn.execute(
            r#"
            insert into validation_link_repair_changes(
              repair_run_id,validation_run_id,entity_type,entity_id,field_name,
              before_value,after_value,created_at
            ) values(?1,?2,?3,?4,?5,?6,?7,current_timestamp)
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
    Ok(())
}

fn exact_planned_run(scan: &ScanOutcome, validation_run_id: i64) -> Result<&PlannedRun> {
    scan.plans
        .iter()
        .find(|plan| plan.id == validation_run_id)
        .context("validation_artifact_not_actionable: artifact is clean, retired, or missing")
}

fn validate_explicit_repair(
    plan: &PlannedRun,
    expected_project_id: i64,
    expected_current: &str,
    artifact_ref: &str,
) -> Result<()> {
    if !plan.diagnosis.repairable {
        bail!(
            "validation_link_not_repairable: {artifact_ref}\nnext: agent-workbench doctor validation-links retire --help"
        );
    }
    if plan.project_id != expected_project_id {
        bail!("validation_link_project_mismatch: expected project does not match diagnosis");
    }
    if plan.diagnosis.current_revision != expected_current {
        bail!(
            "validation_link_state_changed: expected current no longer matches\nnext: agent-workbench doctor validation-links --artifact {artifact_ref}"
        );
    }
    Ok(())
}

fn validate_explicit_retirement(
    plan: &PlannedRun,
    expected_current: &str,
    artifact_ref: &str,
) -> Result<()> {
    if plan.diagnosis.repairable {
        bail!(
            "validation_link_repair_available: safe relink must be used instead\nnext: {}",
            plan.diagnosis.legal_actions[0]
        );
    }
    if plan.diagnosis.current_revision != expected_current {
        bail!(
            "validation_link_state_changed: expected current no longer matches\nnext: agent-workbench doctor validation-links --artifact {artifact_ref}"
        );
    }
    Ok(())
}

fn validation_link_request_digest(
    operation: &str,
    artifact_ref: &str,
    project: Option<i64>,
    reason: Option<&str>,
    expected_current: &str,
) -> String {
    domain_digest(
        b"agent-workbench:validation-link-request-v1\0",
        &CanonicalValue::object([
            ("operation", CanonicalValue::string(operation)),
            ("artifact", CanonicalValue::string(artifact_ref)),
            ("project", optional_integer(project)),
            (
                "reason",
                reason.map_or(CanonicalValue::Null, CanonicalValue::string),
            ),
            ("expected_current", CanonicalValue::string(expected_current)),
        ]),
    )
}

fn lookup_validation_link_receipt(
    conn: &Connection,
    project: i64,
    request_digest: &str,
    idempotent: bool,
) -> Result<Option<ValidationLinkArtifactOutcome>> {
    conn.query_row(
        r#"
        select receipt.artifact_ref,receipt.validation_run_id,receipt.operation,
               receipt.result_current,receipt.repair_run_id,receipt.retirement_id,
               repair.backup_path
        from validation_link_repair_receipts receipt
        left join validation_link_repair_runs repair on repair.id=receipt.repair_run_id
        where receipt.project_id=?1 and receipt.request_digest=?2
        "#,
        params![project, request_digest],
        |row| {
            Ok(ValidationLinkArtifactOutcome {
                artifact_ref: row.get(0)?,
                validation_run_id: row.get(1)?,
                operation: row.get(2)?,
                result_current: row.get(3)?,
                repair_run_id: row.get(4)?,
                retirement_id: row.get(5)?,
                backup_path: row.get::<_, Option<String>>(6)?.map(PathBuf::from),
                idempotent,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn create_validation_link_backup(root: &Path, ledger_path: &Path) -> Result<PathBuf> {
    let backup_path = next_backup_path(root)?;
    if let Some(parent) = backup_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create backup directory {}", parent.display()))?;
    }
    let backup_source = open_ledger(ledger_path)?;
    backup_source
        .backup(DatabaseName::Main, &backup_path, None)
        .with_context(|| format!("failed to create backup {}", backup_path.display()))?;
    Ok(backup_path)
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

fn validation_artifact_ref(validation_run_id: i64) -> String {
    format!("validation-run:{validation_run_id}")
}

fn parse_validation_artifact_ref(value: &str) -> Result<i64> {
    let id = value
        .strip_prefix("validation-run:")
        .context("validation_artifact_invalid: expected validation-run:<id>")?
        .parse::<i64>()
        .context("validation_artifact_invalid: validation run id is invalid")?;
    if id <= 0 {
        bail!("validation_artifact_invalid: validation run id must be positive");
    }
    Ok(id)
}

fn validation_link_revision(conn: &Connection, validation_run_id: i64) -> Result<String> {
    let run = conn
        .query_row(
            r#"
            select project_id,validation_gate_id,work_unit_id,task_id,command_usage_id,
                   repository_snapshot_id,command,acceptance_record_id,result,created_at
            from validation_runs where id=?1
            "#,
            [validation_run_id],
            |row| {
                Ok(CanonicalValue::object([
                    ("project", optional_integer(row.get(0)?)),
                    ("gate", CanonicalValue::Integer(row.get(1)?)),
                    ("work", optional_integer(row.get(2)?)),
                    ("task", optional_integer(row.get(3)?)),
                    ("command_usage", optional_integer(row.get(4)?)),
                    ("repository_snapshot", optional_integer(row.get(5)?)),
                    ("command", optional_string(row.get(6)?)),
                    ("acceptance", optional_integer(row.get(7)?)),
                    ("result", CanonicalValue::string(row.get::<_, String>(8)?)),
                    (
                        "created_at",
                        CanonicalValue::string(row.get::<_, String>(9)?),
                    ),
                ]))
            },
        )
        .optional()?
        .context("validation artifact not found")?;
    let gate = conn
        .query_row(
            r#"
            select project_id,work_unit_id,task_id,status,template_id
            from validation_gates
            where id=(select validation_gate_id from validation_runs where id=?1)
            "#,
            [validation_run_id],
            |row| {
                Ok(CanonicalValue::object([
                    ("project", CanonicalValue::Integer(row.get(0)?)),
                    ("work", optional_integer(row.get(1)?)),
                    ("task", optional_integer(row.get(2)?)),
                    ("status", CanonicalValue::string(row.get::<_, String>(3)?)),
                    ("template", optional_integer(row.get(4)?)),
                ]))
            },
        )
        .optional()?
        .unwrap_or(CanonicalValue::Null);
    let artifacts = if table_exists(conn, "artifacts")? {
        conn.prepare(
            "select id,project_id,command_usage_id,repository_snapshot_id from artifacts where validation_run_id=?1 order by id",
        )?
        .query_map([validation_run_id], |row| {
            Ok(CanonicalValue::object([
                ("id", CanonicalValue::Integer(row.get(0)?)),
                ("project", CanonicalValue::Integer(row.get(1)?)),
                ("command_usage", optional_integer(row.get(2)?)),
                ("repository_snapshot", optional_integer(row.get(3)?)),
            ]))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        Vec::new()
    };
    let acceptances = if table_exists(conn, "acceptance_records")? {
        conn.prepare(
            "select id,project_id,approved_by_authority_event_id,status,acceptance_type from acceptance_records where validation_run_id=?1 order by id",
        )?
        .query_map([validation_run_id], |row| {
            Ok(CanonicalValue::object([
                ("id", CanonicalValue::Integer(row.get(0)?)),
                ("project", CanonicalValue::Integer(row.get(1)?)),
                ("authority", optional_integer(row.get(2)?)),
                ("status", CanonicalValue::string(row.get::<_, String>(3)?)),
                ("type", CanonicalValue::string(row.get::<_, String>(4)?)),
            ]))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        Vec::new()
    };
    let retirement = if table_exists(conn, "validation_link_retirements")? {
        conn.query_row(
            "select id,project_id,reason,expected_current,created_at from validation_link_retirements where validation_run_id=?1",
            [validation_run_id],
            |row| {
                Ok(CanonicalValue::object([
                    ("id", CanonicalValue::Integer(row.get(0)?)),
                    ("project", CanonicalValue::Integer(row.get(1)?)),
                    ("reason", CanonicalValue::string(row.get::<_, String>(2)?)),
                    ("expected_current", CanonicalValue::string(row.get::<_, String>(3)?)),
                    ("created_at", CanonicalValue::string(row.get::<_, String>(4)?)),
                ]))
            },
        )
        .optional()?
        .unwrap_or(CanonicalValue::Null)
    } else {
        CanonicalValue::Null
    };
    Ok(domain_digest(
        b"agent-workbench:validation-link-revision-v1\0",
        &CanonicalValue::object([
            ("validation_run", run),
            ("gate", gate),
            ("artifacts", CanonicalValue::Array(artifacts)),
            ("acceptances", CanonicalValue::Array(acceptances)),
            ("retirement", retirement),
        ]),
    ))
}

fn optional_integer(value: Option<i64>) -> CanonicalValue {
    value.map_or(CanonicalValue::Null, CanonicalValue::Integer)
}

fn optional_string(value: Option<String>) -> CanonicalValue {
    value.map_or(CanonicalValue::Null, CanonicalValue::String)
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
        where not exists(
          select 1 from validation_link_retirements retirement
          where retirement.validation_run_id=validation_runs.id
        )
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
        let artifact_ref = validation_artifact_ref(run.id);
        let current_revision = validation_link_revision(conn, run.id)?;
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
                artifact_ref,
                expected_project_id: None,
                current_revision,
                repairable: false,
                reasons: vec![format!(
                    "validation_gate:{} is missing; no authoritative repair exists",
                    run.validation_gate_id
                )],
                changes: Vec::new(),
                legal_actions: vec![
                    "agent-workbench doctor validation-links retire --help".to_string(),
                ],
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
    let artifact_ref = validation_artifact_ref(run.id);
    let current_revision = validation_link_revision(conn, run.id)?;
    let legal_actions = if repairable {
        vec![format!(
            "agent-workbench doctor validation-links repair {artifact_ref} --project {} --expected-current {current_revision}",
            gate.project_id
        )]
    } else {
        vec!["agent-workbench doctor validation-links retire --help".to_string()]
    };
    let diagnosis = ValidationLinkRunDiagnosis {
        validation_run_id: run.id,
        artifact_ref,
        expected_project_id: Some(gate.project_id),
        current_revision,
        repairable,
        reasons,
        changes: changes.into_values().collect(),
        legal_actions,
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
