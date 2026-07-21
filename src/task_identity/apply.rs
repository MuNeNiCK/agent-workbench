use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::db::{default_ledger_path, open_ledger};
use crate::identity::{
    AuditHandle, BackupHandle, CanonicalValue, OwnerHandle, PlanHandle, domain_digest,
};

use super::TaskIdentityApplyOutput;
use super::plan::{
    PlannedTask, owner_handles, owner_plan_handle, plan_task, plan_task_requirement,
};
use super::source::{OwnerSource, SourceSnapshot, read_from_connection};

mod backup;
mod completion;
mod intent;
mod membership;

pub(crate) use backup::create as create_verified_backup;

pub(super) fn apply(
    root: &Path,
    owner_selector: &str,
    plan_selector: &str,
) -> Result<TaskIdentityApplyOutput> {
    let _update_guard = crate::update::shared_writer_guard(root)?;
    let _lock = super::lock::exclusive(root)?;
    let snapshot = SourceSnapshot::open(root)?;
    let requested_owner = OwnerHandle::parse(owner_selector)?;
    let owner = snapshot
        .owners
        .iter()
        .find(|candidate| owner_handles(&snapshot, candidate).0 == requested_owner)
        .context("owner handle is unknown or stale; rerun migration task-history plan")?;
    if owner.migrated {
        return replay_applied(root, owner, plan_selector);
    }
    let planned = owner
        .tasks
        .iter()
        .map(|task| plan_task(snapshot.project_id, owner, task))
        .collect::<Result<Vec<_>>>()?;
    let resolution = super::recovery::resolve_for_apply(root, &snapshot, owner, plan_selector)?;
    let planned = owner
        .tasks
        .iter()
        .zip(&planned)
        .map(|(task, base)| {
            let Some(requirement_id) = resolution.selected_requirement_ids.get(&task.task_id)
            else {
                return Ok(base.clone());
            };
            let requirement = task
                .requirements
                .iter()
                .find(|requirement| requirement.requirement_id == *requirement_id)
                .context("resolved plan selects a missing source requirement")?;
            plan_task_requirement(snapshot.project_id, owner, requirement)
        })
        .collect::<Result<Vec<_>>>()?;

    let backup = backup::create(
        root,
        owner,
        &resolution.plan_digest,
        &snapshot.database_digest,
    )?;
    let intent = intent::prepare(
        root,
        owner,
        &resolution.plan_digest,
        resolution.mode,
        &backup.digest,
        &snapshot.database_digest,
    )?;
    let conn = open_ledger(&default_ledger_path(root))?;
    conn.execute_batch("begin immediate")?;
    let current = read_from_connection(&conn, root)?;
    let current_owner = current
        .owners
        .iter()
        .find(|candidate| candidate.owner_digest == owner.owner_digest)
        .context("source_drift")?;
    if current_owner.source_digest != owner.source_digest {
        bail!("source_drift: rerun migration task-history plan");
    }
    if current.database_digest != snapshot.database_digest {
        bail!("source_drift: database changed after backup; retry apply");
    }

    conn.execute_batch(super::schema::SQL)?;
    materialize_owner(
        &conn,
        &snapshot,
        owner,
        &planned,
        &resolution.retired_task_ids,
        &resolution.retired_dependency_ids,
    )?;
    let conserved = read_from_connection(&conn, root)?;
    if conserved.database_digest != snapshot.database_digest {
        bail!("source_conservation: migration changed a historical source family");
    }
    let conserved_owner = conserved
        .owners
        .iter()
        .find(|candidate| candidate.owner_digest == owner.owner_digest)
        .context("source_conservation: owner disappeared during migration")?;
    if conserved_owner.source_digest != owner.source_digest {
        bail!("source_conservation: owner source changed during migration");
    }
    let audit_digest = domain_digest(
        b"AWB-MIGRATION-AUDIT-v1\0",
        &CanonicalValue::object([
            ("owner", CanonicalValue::string(owner.owner_digest.clone())),
            (
                "component",
                CanonicalValue::string(owner.component_digest.clone()),
            ),
            (
                "source",
                CanonicalValue::string(owner.source_digest.clone()),
            ),
            (
                "plan",
                CanonicalValue::string(resolution.plan_digest.clone()),
            ),
            ("backup", CanonicalValue::string(backup.digest.clone())),
            (
                "database",
                CanonicalValue::string(snapshot.database_digest.clone()),
            ),
            ("intent", CanonicalValue::string(intent.digest.clone())),
        ]),
    );
    conn.execute(
        r#"
        insert into task_identity_migration_audits(
          project_id,owner_work_unit_id,owner_digest,component_digest,source_digest,database_digest,
          plan_digest,plan_mode,backup_digest,intent_digest,audit_digest,status,created_at
        ) values(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'applied',current_timestamp)
        "#,
        params![
            snapshot.project_id,
            owner.owner_id,
            owner.owner_digest,
            owner.component_digest,
            owner.source_digest,
            snapshot.database_digest,
            resolution.plan_digest,
            resolution.mode,
            backup.digest,
            intent.digest,
            audit_digest,
        ],
    )?;
    conn.execute_batch("commit")?;
    intent.publish_committed()?;

    let backup_handle = BackupHandle::derive(
        b"AWB-BACKUP-HANDLE-v1\0",
        &CanonicalValue::object([
            (
                "plan",
                CanonicalValue::string(resolution.plan_handle.as_str()),
            ),
            ("backup", CanonicalValue::string(backup.digest)),
        ]),
    );
    let audit_handle = AuditHandle::derive(
        b"AWB-AUDIT-HANDLE-v1\0",
        &CanonicalValue::object([
            (
                "plan",
                CanonicalValue::string(resolution.plan_handle.as_str()),
            ),
            ("audit", CanonicalValue::string(audit_digest)),
        ]),
    );
    Ok(TaskIdentityApplyOutput {
        classification: "project-internal",
        result: "applied",
        backup_handle: backup_handle.as_str().to_string(),
        audit_handle: audit_handle.as_str().to_string(),
    })
}

fn replay_applied(
    root: &Path,
    owner: &OwnerSource,
    plan_selector: &str,
) -> Result<TaskIdentityApplyOutput> {
    let requested = PlanHandle::parse(plan_selector)?;
    let conn = open_ledger(&default_ledger_path(root))?;
    let persisted = conn
        .query_row(
            "select plan_digest,plan_mode,backup_digest,audit_digest,intent_digest from task_identity_migration_audits where owner_digest=?1 and status='applied'",
            params![owner.owner_digest],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()?
        .context("migration audit is missing for migrated owner")?;
    let plan_handle = owner_plan_handle(owner, &persisted.0, &persisted.1);
    if requested != plan_handle {
        bail!("plan handle is unknown or stale; use migration task-history audit");
    }
    intent::publish_committed(root, &persisted.4)?;
    let backup_handle = BackupHandle::derive(
        b"AWB-BACKUP-HANDLE-v1\0",
        &CanonicalValue::object([
            ("plan", CanonicalValue::string(plan_handle.as_str())),
            ("backup", CanonicalValue::string(persisted.2)),
        ]),
    );
    let audit_handle = AuditHandle::derive(
        b"AWB-AUDIT-HANDLE-v1\0",
        &CanonicalValue::object([
            ("plan", CanonicalValue::string(plan_handle.as_str())),
            ("audit", CanonicalValue::string(persisted.3)),
        ]),
    );
    Ok(TaskIdentityApplyOutput {
        classification: "project-internal",
        result: "applied",
        backup_handle: backup_handle.as_str().to_string(),
        audit_handle: audit_handle.as_str().to_string(),
    })
}

#[derive(Serialize)]
struct AuditView {
    algorithm: &'static str,
    records: Vec<AuditRecordView>,
    pending_recoveries: Vec<super::recovery::audit::PendingRecoveryView>,
}

#[derive(Serialize)]
struct AuditRecordView {
    owner_handle: String,
    component_handle: String,
    plan_handle: String,
    recovery_handle: Option<String>,
    backup_handle: String,
    audit_handle: String,
    result: &'static str,
    next_action: &'static str,
}

pub(super) fn audit(root: &Path, owner_selector: Option<&str>) -> Result<String> {
    let snapshot = SourceSnapshot::open(root)?;
    let selected_owner = owner_selector.map(OwnerHandle::parse).transpose()?;
    let conn = open_ledger(&default_ledger_path(root))?;
    let has_audit_table: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='table' and name='task_identity_migration_audits')",
        [],
        |row| row.get(0),
    )?;
    let mut records = Vec::new();
    let pending_recoveries =
        super::recovery::audit::pending(root, &snapshot, selected_owner.as_ref())?;
    if has_audit_table {
        for owner in &snapshot.owners {
            let (owner_handle, component_handle) = owner_handles(&snapshot, owner);
            if selected_owner
                .as_ref()
                .is_some_and(|selected| selected != &owner_handle)
            {
                continue;
            }
            let persisted = conn.query_row(
                "select plan_digest,plan_mode,backup_digest,audit_digest from task_identity_migration_audits where owner_digest=?1 and status='applied'",
                params![owner.owner_digest],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?)),
            ).optional()?;
            let Some((plan_digest, plan_mode, backup_digest, audit_digest)) = persisted else {
                continue;
            };
            let plan_handle = owner_plan_handle(owner, &plan_digest, &plan_mode);
            let backup_handle = BackupHandle::derive(
                b"AWB-BACKUP-HANDLE-v1\0",
                &CanonicalValue::object([
                    ("plan", CanonicalValue::string(plan_handle.as_str())),
                    ("backup", CanonicalValue::string(backup_digest)),
                ]),
            );
            let audit_handle = AuditHandle::derive(
                b"AWB-AUDIT-HANDLE-v1\0",
                &CanonicalValue::object([
                    ("plan", CanonicalValue::string(plan_handle.as_str())),
                    ("audit", CanonicalValue::string(audit_digest)),
                ]),
            );
            records.push(AuditRecordView {
                owner_handle: owner_handle.as_str().to_string(),
                component_handle: component_handle.as_str().to_string(),
                plan_handle: plan_handle.as_str().to_string(),
                recovery_handle: None,
                backup_handle: backup_handle.as_str().to_string(),
                audit_handle: audit_handle.as_str().to_string(),
                result: "applied",
                next_action: "migration_not_required",
            });
        }
    }
    if selected_owner.is_some() && records.is_empty() && pending_recoveries.is_empty() {
        bail!(
            "owner handle is unknown, stale, or has no migration audit; rerun migration task-history plan"
        );
    }
    records.sort_by(|left, right| left.owner_handle.cmp(&right.owner_handle));
    serde_json::to_string(&AuditView {
        algorithm: "ID-PLAN-AUDIT-VIEW-v1",
        records,
        pending_recoveries,
    })
    .map_err(Into::into)
}

fn materialize_owner(
    conn: &Connection,
    snapshot: &SourceSnapshot,
    owner: &OwnerSource,
    planned: &[PlannedTask],
    retired_task_ids: &std::collections::BTreeSet<i64>,
    retired_dependency_ids: &std::collections::BTreeSet<i64>,
) -> Result<()> {
    let mut task_identity_ids = BTreeMap::<String, i64>::new();
    let mut revision_ids = BTreeMap::<String, i64>::new();
    let mut membership_ids = BTreeMap::<(i64, i64), (i64, Option<i64>, String)>::new();
    let mut current_revisions = BTreeMap::<String, (Option<i64>, String)>::new();
    for (task, planned) in owner.tasks.iter().zip(planned) {
        if retired_task_ids.contains(&task.task_id) || planned.ambiguity {
            continue;
        }
        let candidate = (planned.design_sequence, planned.revision_digest.clone());
        match current_revisions.get(&planned.identity_digest) {
            Some(current) if current >= &candidate => {}
            _ => {
                current_revisions.insert(planned.identity_digest.clone(), candidate);
            }
        }
    }
    for (task, planned) in owner.tasks.iter().zip(planned) {
        if retired_task_ids.contains(&task.task_id) {
            continue;
        }
        let kind = if task.requirements.is_empty() {
            "manual"
        } else {
            "design"
        };
        let task_identity_id = match task_identity_ids.get(&planned.identity_digest) {
            Some(id) => *id,
            None => {
                let id = match conn
                    .query_row(
                        "select id,owner_work_unit_id,kind from task_identities where project_id=?1 and identity_digest=?2",
                        params![snapshot.project_id, planned.identity_digest],
                        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?)),
                    )
                    .optional()?
                {
                    Some((id, existing_owner, existing_kind))
                        if existing_owner == owner.owner_id && existing_kind == kind => id,
                    Some(_) => bail!("identity_conflict: existing identity owner or kind differs"),
                    None => {
                        conn.execute(
                            "insert into task_identities(project_id,owner_work_unit_id,identity_digest,kind,status,created_at) values(?1,?2,?3,?4,'current',current_timestamp)",
                            params![snapshot.project_id, owner.owner_id, planned.identity_digest, kind],
                        )?;
                        conn.last_insert_rowid()
                    }
                };
                task_identity_ids.insert(planned.identity_digest.clone(), id);
                id
            }
        };
        let requirement = planned.source_requirement_id.and_then(|requirement_id| {
            task.requirements
                .iter()
                .find(|requirement| requirement.requirement_id == requirement_id)
        });
        let revision_id = match revision_ids.get(&planned.revision_digest) {
            Some(id) => *id,
            None => {
                let revision_status =
                    if current_revisions[&planned.identity_digest].1 == planned.revision_digest {
                        "current"
                    } else {
                        "historical"
                    };
                if revision_status == "current" {
                    conn.execute(
                        "update task_revisions set status='historical' where project_id=?1 and task_identity_id=?2 and status='current' and revision_digest!=?3",
                        params![snapshot.project_id, task_identity_id, planned.revision_digest],
                    )?;
                }
                let id = match conn
                    .query_row(
                        "select id,task_identity_id,status from task_revisions where project_id=?1 and revision_digest=?2",
                        params![snapshot.project_id, planned.revision_digest],
                        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?)),
                    )
                    .optional()?
                {
                    Some((id, existing_identity, existing_status))
                        if existing_identity == task_identity_id => {
                            if existing_status != revision_status {
                                conn.execute(
                                    "update task_revisions set status=?1 where id=?2",
                                    params![revision_status, id],
                                )?;
                            }
                            id
                        }
                    Some(_) => bail!("revision_conflict: existing revision identity differs"),
                    None => {
                        conn.execute(
                            "insert into task_revisions(project_id,task_identity_id,source_design_requirement_id,revision_digest,design_sequence,status,created_at) values(?1,?2,?3,?4,?5,?6,current_timestamp)",
                            params![
                                snapshot.project_id,
                                task_identity_id,
                                requirement.map(|value| value.requirement_id),
                                planned.revision_digest,
                                requirement.map(|value| value.design_sequence),
                                revision_status,
                            ],
                        )?;
                        conn.last_insert_rowid()
                    }
                };
                revision_ids.insert(planned.revision_digest.clone(), id);
                id
            }
        };
        retire_conflicting_manual_alias(
            conn,
            snapshot.project_id,
            owner.owner_id,
            task.task_id,
            revision_id,
        )?;
        conn.execute(
            "insert or ignore into task_revision_aliases(project_id,task_revision_id,historical_task_id,source_schema,created_at) values(?1,?2,?3,?4,current_timestamp)",
            params![snapshot.project_id, revision_id, task.task_id, snapshot.schema_version],
        )?;
        let linked_revision: i64 = conn.query_row(
            "select task_revision_id from task_revision_aliases where project_id=?1 and historical_task_id=?2",
            params![snapshot.project_id, task.task_id],
            |row| row.get(0),
        )?;
        if linked_revision != revision_id {
            bail!("revision_conflict: task alias points to another revision");
        }
        for requirement in &task.requirements {
            conn.execute(
                "insert or ignore into task_revision_requirements(project_id,task_revision_id,design_requirement_id,created_at) values(?1,?2,?3,current_timestamp)",
                params![snapshot.project_id, revision_id, requirement.requirement_id],
            )?;
        }
        membership::materialize(
            conn,
            snapshot,
            task,
            task_identity_id,
            revision_id,
            &mut membership_ids,
        )?;
    }
    completion::materialize(
        conn,
        snapshot,
        owner,
        planned,
        &task_identity_ids,
        &revision_ids,
    )?;
    materialize_dependencies(
        conn,
        snapshot,
        owner,
        planned,
        &task_identity_ids,
        retired_dependency_ids,
    )?;
    Ok(())
}

fn retire_conflicting_manual_alias(
    conn: &Connection,
    project_id: i64,
    owner_work_unit_id: i64,
    historical_task_id: i64,
    target_revision_id: i64,
) -> Result<()> {
    let existing = conn
        .query_row(
            r#"
            select revision.id,revision.task_identity_id,identity.owner_work_unit_id,
                   identity.kind,identity.status
            from task_revision_aliases alias
            join task_revisions revision on revision.id=alias.task_revision_id
            join task_identities identity on identity.id=revision.task_identity_id
            where alias.project_id=?1 and alias.historical_task_id=?2
            "#,
            params![project_id, historical_task_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()?;
    let Some((revision_id, identity_id, existing_owner, kind, status)) = existing else {
        return Ok(());
    };
    if revision_id == target_revision_id {
        return Ok(());
    }
    if existing_owner != owner_work_unit_id || kind != "manual" || status != "current" {
        bail!("revision_conflict: task alias points to another revision");
    }
    conn.execute(
        "update task_revisions set status='retired' where id=?1 and status='current'",
        [revision_id],
    )?;
    conn.execute(
        "update task_identities set status='retired' where id=?1 and status='current'",
        [identity_id],
    )?;
    conn.execute(
        "delete from task_revision_aliases where project_id=?1 and historical_task_id=?2",
        params![project_id, historical_task_id],
    )?;
    Ok(())
}

fn materialize_dependencies(
    conn: &Connection,
    snapshot: &SourceSnapshot,
    owner: &OwnerSource,
    planned: &[PlannedTask],
    task_identity_ids: &BTreeMap<String, i64>,
    retired_dependency_ids: &std::collections::BTreeSet<i64>,
) -> Result<()> {
    let mut phase_identities = BTreeMap::<i64, Vec<(String, i64)>>::new();
    for (task, planned) in owner.tasks.iter().zip(planned) {
        if planned.ambiguity {
            continue;
        }
        let task_identity_id = task_identity_ids[&planned.identity_digest];
        for membership in &task.memberships {
            phase_identities
                .entry(membership.phase_id)
                .or_default()
                .push((planned.identity_digest.clone(), task_identity_id));
        }
    }
    for targets in phase_identities.values_mut() {
        targets.sort();
        targets.dedup();
    }

    let mut materialized = BTreeMap::<(i64, i64), (i64, String)>::new();
    for source in &owner.dependencies {
        if retired_dependency_ids.contains(&source.dependency_id) {
            continue;
        }
        let from_targets = phase_identities
            .get(&source.from_phase_id)
            .cloned()
            .unwrap_or_default();
        let to_targets = phase_identities
            .get(&source.to_phase_id)
            .cloned()
            .unwrap_or_default();
        let state = match source.status.as_str() {
            "open" => "open",
            "completed" => "completed",
            "out_of_scope" => "out_of_scope",
            _ => bail!("unreadable_source: unsupported dependency status"),
        };
        for (_, from_id) in &from_targets {
            for (_, to_id) in &to_targets {
                if from_id == to_id {
                    bail!(
                        "dependency_self: phase dependency collapses to a task identity self-edge"
                    );
                }
                if materialized
                    .get(&(*to_id, *from_id))
                    .is_some_and(|(_, reverse_state)| {
                        reverse_state == "open"
                            && source.status
                                != crate::task_identity::status::DependencyState::Superseded
                    })
                {
                    bail!("dependency_reverse: contradictory task identity dependency");
                }
                let key = (*from_id, *to_id);
                let task_dependency_id = match materialized.get(&key).cloned() {
                    Some((id, existing_state)) => {
                        if existing_state != state
                            && existing_state != "out_of_scope"
                            && source.status
                                != crate::task_identity::status::DependencyState::Superseded
                        {
                            bail!(
                                "dependency_state: duplicate task identity dependency states disagree"
                            );
                        }
                        if existing_state == "out_of_scope" && state != "out_of_scope" {
                            conn.execute(
                                "update task_identity_dependencies set state=?1 where id=?2",
                                params![state, id],
                            )?;
                            materialized.insert(key, (id, state.to_string()));
                        }
                        id
                    }
                    None => {
                        conn.execute(
                            "insert into task_identity_dependencies(project_id,from_task_identity_id,to_task_identity_id,state,created_at) values(?1,?2,?3,?4,current_timestamp)",
                            params![snapshot.project_id, from_id, to_id, state],
                        )?;
                        let id = conn.last_insert_rowid();
                        materialized.insert(key, (id, state.to_string()));
                        id
                    }
                };
                conn.execute(
                    "insert into task_identity_dependency_sources(project_id,task_identity_dependency_id,source_dependency_id,created_at) values(?1,?2,?3,current_timestamp)",
                    params![snapshot.project_id, task_dependency_id, source.dependency_id],
                )?;
            }
        }
    }
    Ok(())
}
