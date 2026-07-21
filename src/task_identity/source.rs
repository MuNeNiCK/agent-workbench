use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use crate::db::{SCHEMA_VERSION, default_ledger_path, open_ledger};
use crate::identity::{CanonicalValue, domain_digest, signed_source_id};

use super::status::{DependencyState, DerivationState, PhaseState, RequirementState, TaskState};

mod completion;
mod database;
mod evidence;
mod ownership;
mod snapshot;

pub(super) use completion::ChecklistSource;
pub(super) use evidence::EvidenceSource;

#[derive(Debug)]
pub(super) struct SourceSnapshot {
    pub(super) schema_version: i64,
    pub(super) project_id: i64,
    pub(super) project_digest: String,
    pub(super) database_digest: String,
    pub(super) owners: Vec<OwnerSource>,
}

#[derive(Clone, Debug)]
pub(super) struct OwnerSource {
    pub(super) owner_id: i64,
    pub(super) owner_digest: String,
    pub(super) component_digest: String,
    pub(super) source_digest: String,
    pub(super) owner_conflict: bool,
    pub(super) migrated: bool,
    pub(super) tasks: Vec<TaskSource>,
    pub(super) dependencies: Vec<DependencySource>,
}

#[derive(Clone, Debug)]
pub(super) struct TaskSource {
    pub(super) task_id: i64,
    pub(super) title: String,
    pub(super) details: Option<String>,
    pub(super) status: TaskState,
    pub(super) requirements: Vec<RequirementSource>,
    pub(super) memberships: Vec<MembershipSource>,
    pub(super) checklists: Vec<ChecklistSource>,
    pub(super) evidence: Vec<EvidenceSource>,
}

#[derive(Clone, Debug)]
pub(super) struct RequirementSource {
    pub(super) derivation_status: DerivationState,
    pub(super) requirement_id: i64,
    pub(super) design_version_id: i64,
    pub(super) design_sequence: i64,
    pub(super) design_package_id: i64,
    pub(super) requirement_key: String,
    pub(super) revision: i64,
    pub(super) requirement_text: String,
    pub(super) priority: String,
    pub(super) surfaces: Option<String>,
    pub(super) status: RequirementState,
    pub(super) gates: Vec<GateSource>,
}

#[derive(Clone, Debug)]
pub(super) struct GateSource {
    pub(super) key: String,
    pub(super) expected: String,
    pub(super) stage: String,
    pub(super) body: String,
}

#[derive(Clone, Debug)]
pub(super) struct MembershipSource {
    pub(super) membership_id: i64,
    pub(super) phase_id: i64,
    pub(super) phase_status: PhaseState,
}

#[derive(Clone, Debug)]
pub(super) struct DependencySource {
    pub(super) dependency_id: i64,
    pub(super) from_phase_id: i64,
    pub(super) to_phase_id: i64,
    pub(super) status: DependencyState,
}

impl SourceSnapshot {
    pub(super) fn open(root: &Path) -> Result<Self> {
        let ledger_path = default_ledger_path(root);
        if !ledger_path.exists() {
            bail!("project is not initialized; run agent-workbench init");
        }
        let conn = open_ledger(&ledger_path)?;
        conn.pragma_update(None, "query_only", true)?;
        let data_version_before: i64 =
            conn.query_row("pragma data_version", [], |row| row.get(0))?;
        conn.execute_batch("begin")?;
        let snapshot = read_from_connection(&conn, root);
        conn.execute_batch("rollback")?;
        let data_version_after: i64 =
            conn.query_row("pragma data_version", [], |row| row.get(0))?;
        if data_version_before != data_version_after {
            bail!("source_drift: ledger changed while the migration snapshot was read");
        }
        snapshot
    }
}

pub(super) fn read_from_connection(conn: &Connection, root: &Path) -> Result<SourceSnapshot> {
    let schema_version = conn
        .query_row(
            "select version from schema_migrations order by version desc limit 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .context("task-history migration requires readable schema metadata")?;
    if !(6..=SCHEMA_VERSION).contains(&schema_version) {
        bail!("task-history migration source is not a supported phase-bearing schema");
    }
    super::profile::validate(conn, schema_version)?;
    let database_snapshot = snapshot::read_all(conn, schema_version)?;
    let database_digest = database_snapshot.digest.clone();
    let ownership = ownership::classify(conn, &database_snapshot)?;

    let canonical_root = root
        .canonicalize()
        .unwrap_or_else(|_| root.to_path_buf())
        .display()
        .to_string();
    let project_id = conn
        .query_row(
            "select id from projects where root_path=?1",
            params![canonical_root],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .context("task-history migration project identity is unresolved")?;
    let project_digest = domain_digest(
        b"AWB-RECOVERY-PROJECT-v1\0",
        &CanonicalValue::object([(
            "project",
            CanonicalValue::string(signed_source_id(project_id)?),
        )]),
    );

    let mut owner_stmt = conn.prepare("select id from work_units order by id")?;
    let owner_ids = owner_stmt
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut owners = Vec::with_capacity(owner_ids.len());
    for owner_id in owner_ids {
        let component = ownership
            .components
            .get(&owner_id)
            .context("task-history migration owner component is missing")?;
        owners.push(read_owner(
            conn,
            owner_id,
            component,
            ownership.conflicted_owners.contains(&owner_id),
        )?);
    }
    Ok(SourceSnapshot {
        schema_version,
        project_id,
        project_digest,
        database_digest,
        owners,
    })
}

fn read_owner(
    conn: &Connection,
    owner_id: i64,
    component: &ownership::ComponentSnapshot,
    owner_conflict: bool,
) -> Result<OwnerSource> {
    let mut task_stmt = conn
        .prepare("select id,title,details,status from tasks where work_unit_id=?1 order by id")?;
    let task_rows = task_stmt
        .query_map(params![owner_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut tasks = Vec::with_capacity(task_rows.len());
    for (task_id, title, details, status) in task_rows {
        tasks.push(TaskSource {
            task_id,
            title,
            details,
            status: TaskState::parse(&status)?,
            requirements: read_requirements(conn, task_id)?,
            memberships: read_memberships(conn, task_id)?,
            checklists: completion::read(conn, task_id)?,
            evidence: evidence::read(conn, task_id)?,
        });
    }

    let dependencies = read_dependencies(conn, owner_id)?;
    debug_assert_eq!(component.owner_id, owner_id);
    let migrated = database::migration_applied(conn, &component.owner_digest)?;

    Ok(OwnerSource {
        owner_id,
        owner_digest: component.owner_digest.clone(),
        component_digest: component.component_digest.clone(),
        source_digest: component.source_digest.clone(),
        owner_conflict,
        migrated,
        tasks,
        dependencies,
    })
}

fn read_requirements(conn: &Connection, task_id: i64) -> Result<Vec<RequirementSource>> {
    let mut stmt = conn.prepare(
        r#"
        select d.status,r.id,r.design_version_id,v.version_number,v.design_package_id,
               r.requirement_key,r.revision,r.requirement_text,
               r.priority,r.required_surfaces,r.status
        from task_derivations d
        join design_requirements r on r.id=d.design_requirement_id
        join design_versions v on v.id=r.design_version_id
        where d.task_id=?1
        order by v.design_package_id,v.version_number,r.requirement_key,r.id
        "#,
    )?;
    let rows = stmt
        .query_map(params![task_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, String>(10)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(
            |(
                derivation_status,
                requirement_id,
                design_version_id,
                design_sequence,
                design_package_id,
                requirement_key,
                revision,
                requirement_text,
                priority,
                surfaces,
                status,
            )| {
                Ok(RequirementSource {
                    derivation_status: DerivationState::parse(&derivation_status)?,
                    requirement_id,
                    design_version_id,
                    design_sequence,
                    design_package_id,
                    requirement_key,
                    revision,
                    requirement_text,
                    priority,
                    surfaces,
                    status: RequirementState::parse(&status)?,
                    gates: read_gates(conn, requirement_id)?,
                })
            },
        )
        .collect()
}

fn read_gates(conn: &Connection, requirement_id: i64) -> Result<Vec<GateSource>> {
    let mut stmt = conn.prepare(
        r#"
        select g.gate_key,g.expected_result,g.stage,g.gate_text
        from validation_gate_template_requirements link
        join validation_gate_templates g on g.id=link.validation_gate_template_id
        where link.design_requirement_id=?1
        order by g.gate_key,g.id
        "#,
    )?;
    stmt.query_map(params![requirement_id], |row| {
        Ok(GateSource {
            key: row.get(0)?,
            expected: row.get(1)?,
            stage: row.get(2)?,
            body: row.get(3)?,
        })
    })?
    .collect::<rusqlite::Result<Vec<_>>>()
    .map_err(Into::into)
}

fn read_memberships(conn: &Connection, task_id: i64) -> Result<Vec<MembershipSource>> {
    let sql = if source_table_exists(conn, "phase_epoch_sources")? {
        r#"
        select m.id,m.phase_id,
               case when epoch.state='superseded' then 'superseded' else p.status end
        from work_phase_task_memberships m
        join work_phases p on p.id=m.phase_id
        left join phase_epoch_sources source on source.source_phase_id=p.id
        left join phase_epochs epoch on epoch.id=source.phase_epoch_id
        where m.task_id=?1
        order by m.phase_id,m.id
        "#
    } else {
        r#"
        select m.id,m.phase_id,p.status
        from work_phase_task_memberships m
        join work_phases p on p.id=m.phase_id
        where m.task_id=?1
        order by m.phase_id,m.id
        "#
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map(params![task_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(membership_id, phase_id, status)| {
            Ok(MembershipSource {
                membership_id,
                phase_id,
                phase_status: PhaseState::parse(&status)?,
            })
        })
        .collect()
}

fn read_dependencies(conn: &Connection, owner_id: i64) -> Result<Vec<DependencySource>> {
    let sql = if source_table_exists(conn, "phase_epoch_dependency_sources")? {
        r#"
        select d.id,d.from_phase_id,d.to_phase_id,
               case when epoch.state='invalidated' then 'invalidated' else d.status end
        from work_phase_dependencies d
        join work_phases source_phase on source_phase.id=d.from_phase_id
        join work_phases target_phase on target_phase.id=d.to_phase_id
        left join phase_epoch_dependency_sources epoch_source
          on epoch_source.source_dependency_id=d.id
        left join phase_epoch_dependencies epoch
          on epoch.id=epoch_source.phase_epoch_dependency_id
        where source_phase.work_unit_id=?1 and target_phase.work_unit_id=?1
        order by d.from_phase_id,d.to_phase_id,d.id
        "#
    } else {
        r#"
        select d.id,d.from_phase_id,d.to_phase_id,d.status
        from work_phase_dependencies d
        join work_phases source on source.id=d.from_phase_id
        join work_phases target on target.id=d.to_phase_id
        where source.work_unit_id=?1 and target.work_unit_id=?1
        order by d.from_phase_id,d.to_phase_id,d.id
        "#
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map(params![owner_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(|(dependency_id, from_phase_id, to_phase_id, status)| {
            Ok(DependencySource {
                dependency_id,
                from_phase_id,
                to_phase_id,
                status: DependencyState::parse(&status)?,
            })
        })
        .collect()
}

fn source_table_exists(conn: &Connection, table: &str) -> Result<bool> {
    conn.query_row(
        "select exists(select 1 from sqlite_schema where type='table' and name=?1)",
        [table],
        |row| row.get(0),
    )
    .map_err(Into::into)
}
