use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use crate::identity::{CanonicalValue, domain_digest};

struct ItemMaterial {
    work_unit_id: i64,
    plan_key: String,
    item_key: String,
    title: String,
    details: String,
    outcome: String,
    requirements: Vec<(i64, String, String)>,
}

pub(crate) fn materialize_decomposition_item(
    conn: &Connection,
    project_id: i64,
    plan_id: i64,
    item_id: i64,
    task_id: i64,
    phase_id: i64,
    retained_source_task: Option<(i64, crate::decomposition::ReconciliationEffect)>,
) -> Result<()> {
    let material = item_material(conn, project_id, plan_id, item_id)?;
    let (task_identity_id, identity_digest) = match retained_source_task {
        Some((source_task_id, _)) => ensure_source_identity(conn, project_id, source_task_id)?,
        None => ensure_item_identity(conn, project_id, &material)?,
    };
    if let Some((source_task_id, crate::decomposition::ReconciliationEffect::Preserve)) =
        retained_source_task
    {
        require_preservable_revision(
            conn,
            project_id,
            source_task_id,
            &identity_digest,
            &material,
        )?;
    }
    let revision_id = ensure_item_revision(
        conn,
        project_id,
        task_identity_id,
        &identity_digest,
        &material,
        retained_source_task.is_some(),
    )?;
    conn.execute(
        "insert into task_revision_aliases(project_id,task_revision_id,historical_task_id,source_schema,created_at) values(?1,?2,?3,15,current_timestamp)",
        params![project_id, revision_id, task_id],
    )?;
    let task_status = retained_source_task
        .map(|(source, effect)| reconciliation_task_status(conn, source, effect))
        .transpose()?
        .unwrap_or_else(|| "open".to_string());
    add_membership(
        conn,
        project_id,
        task_identity_id,
        revision_id,
        phase_id,
        task_id,
        task_state(&task_status)?,
    )
}

fn reconciliation_task_status(
    conn: &Connection,
    source_task_id: i64,
    effect: crate::decomposition::ReconciliationEffect,
) -> Result<String> {
    if effect == crate::decomposition::ReconciliationEffect::Open {
        return Ok("open".to_string());
    }
    conn.query_row(
        "select status from tasks where id=?1",
        [source_task_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn require_preservable_revision(
    conn: &Connection,
    project_id: i64,
    source_task_id: i64,
    identity_digest: &str,
    material: &ItemMaterial,
) -> Result<()> {
    let source_digest: String = conn.query_row(
        r#"
        select revision.revision_digest
        from task_revision_aliases alias
        join task_revisions revision on revision.id=alias.task_revision_id
        where alias.project_id=?1 and alias.historical_task_id=?2
        "#,
        params![project_id, source_task_id],
        |row| row.get(0),
    )?;
    if source_digest != item_revision_digest(identity_digest, material) {
        bail!("preserve task effect requires unchanged task meaning and current revision");
    }
    Ok(())
}

pub(crate) fn revise_decomposition_task(
    conn: &Connection,
    project_id: i64,
    task_id: i64,
    details: &str,
    outcome: &str,
) -> Result<Option<i64>> {
    let candidates = conn
        .prepare(
            r#"
            select application.decomposition_plan_id,application.decomposition_item_id
            from decomposition_applications application
            join decomposition_plans plan on plan.id=application.decomposition_plan_id
            where application.project_id=?1 and application.task_id=?2
              and plan.status in ('applied','incomplete')
            order by application.decomposition_plan_id,application.decomposition_item_id
            "#,
        )?
        .query_map(params![project_id, task_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let (plan_id, item_id) = match candidates.as_slice() {
        [candidate] => *candidate,
        [] => return Ok(None),
        _ => bail!("task correction has multiple current Decomposition Plan items"),
    };
    let mut material = item_material(conn, project_id, plan_id, item_id)?;
    material.details = details.to_string();
    material.outcome = outcome.to_string();
    let (identity_id, identity_digest): (i64, String) = conn
        .query_row(
            r#"
            select identity.id,identity.identity_digest
            from task_revision_aliases alias
            join task_revisions revision on revision.id=alias.task_revision_id
            join task_identities identity on identity.id=revision.task_identity_id
            where alias.project_id=?1 and alias.historical_task_id=?2
            "#,
            params![project_id, task_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .context("task correction requires one canonical task identity alias")?;
    let revision_id = ensure_item_revision(
        conn,
        project_id,
        identity_id,
        &identity_digest,
        &material,
        true,
    )?;
    conn.execute(
        "update task_revision_aliases set task_revision_id=?1 where project_id=?2 and historical_task_id=?3",
        params![revision_id, project_id, task_id],
    )?;
    Ok(Some(revision_id))
}

fn item_material(
    conn: &Connection,
    project_id: i64,
    plan_id: i64,
    item_id: i64,
) -> Result<ItemMaterial> {
    let mut material: ItemMaterial = conn.query_row(
        r#"
        select plan.work_unit_id,plan.plan_key,
               item.item_key,item.title,item.details,item.outcome
        from decomposition_items item
        join decomposition_plans plan on plan.id=item.decomposition_plan_id
        where plan.id=?1 and item.id=?2 and plan.project_id=?3
        "#,
        params![plan_id, item_id, project_id],
        |row| {
            Ok(ItemMaterial {
                work_unit_id: row.get(0)?,
                plan_key: row.get(1)?,
                item_key: row.get(2)?,
                title: row.get(3)?,
                details: row.get(4)?,
                outcome: row.get(5)?,
                requirements: Vec::new(),
            })
        },
    )?;
    material.requirements = conn
        .prepare(
            r#"
            select requirement.id,requirement.requirement_key,requirement.requirement_hash
            from decomposition_item_requirements link
            join design_requirements requirement on requirement.id=link.design_requirement_id
            where link.decomposition_item_id=?1
            order by requirement.requirement_key,requirement.id
            "#,
        )?
        .query_map([item_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if material.requirements.is_empty() {
        bail!("decomposition task identity requires requirement coverage");
    }
    Ok(material)
}

fn identity_digest(material: &ItemMaterial) -> String {
    domain_digest(
        b"AWB-DECOMPOSITION-TASK-IDENTITY-v1\0",
        &CanonicalValue::object([
            ("work", CanonicalValue::Integer(material.work_unit_id)),
            ("plan", CanonicalValue::string(material.plan_key.clone())),
            ("item", CanonicalValue::string(material.item_key.clone())),
        ]),
    )
}

fn ensure_item_identity(
    conn: &Connection,
    project_id: i64,
    material: &ItemMaterial,
) -> Result<(i64, String)> {
    let digest = identity_digest(material);
    let id = match conn
        .query_row(
            "select id,owner_work_unit_id from task_identities where project_id=?1 and identity_digest=?2",
            params![project_id, digest],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?
    {
        Some((id, owner)) if owner == material.work_unit_id => id,
        Some(_) => bail!("decomposition task identity is owned by another work unit"),
        None => {
            conn.execute(
                "insert into task_identities(project_id,owner_work_unit_id,identity_digest,kind,status,created_at) values(?1,?2,?3,'design','current',current_timestamp)",
                params![project_id, material.work_unit_id, digest],
            )?;
            conn.last_insert_rowid()
        }
    };
    Ok((id, digest))
}

fn ensure_source_identity(
    conn: &Connection,
    project_id: i64,
    source_task_id: i64,
) -> Result<(i64, String)> {
    if let Some(existing) = conn
        .query_row(
            r#"
            select identity.id,identity.identity_digest
            from task_revision_aliases alias
            join task_revisions revision on revision.id=alias.task_revision_id
            join task_identities identity on identity.id=revision.task_identity_id
            where alias.project_id=?1 and alias.historical_task_id=?2
            "#,
            params![project_id, source_task_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
    {
        return Ok(existing);
    }
    let candidates = conn
        .prepare(
            r#"
            select distinct item.decomposition_plan_id,item.id
            from decomposition_items item
            left join decomposition_applications application
              on application.decomposition_item_id=item.id
            left join decomposition_migration_sources migration
              on migration.decomposition_item_id=item.id
            where application.task_id=?1 or migration.source_task_id=?1
            order by item.decomposition_plan_id,item.id
            "#,
        )?
        .query_map([source_task_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let (plan_id, item_id) = match candidates.as_slice() {
        [candidate] => *candidate,
        [] => bail!("retained source task has no predecessor Plan identity"),
        _ => bail!("retained source task has ambiguous predecessor Plan identity"),
    };
    let material = item_material(conn, project_id, plan_id, item_id)?;
    let (identity_id, digest) = ensure_item_identity(conn, project_id, &material)?;
    let revision_id =
        ensure_item_revision(conn, project_id, identity_id, &digest, &material, false)?;
    conn.execute(
        "insert into task_revision_aliases(project_id,task_revision_id,historical_task_id,source_schema,created_at) values(?1,?2,?3,15,current_timestamp)",
        params![project_id, revision_id, source_task_id],
    )?;
    let memberships = conn
        .prepare(
            r#"
            select membership.id,membership.phase_id,phase.status
            from work_phase_task_memberships membership
            join work_phases phase on phase.id=membership.phase_id
            where membership.project_id=?1 and membership.task_id=?2
            order by membership.id
            "#,
        )?
        .query_map(params![project_id, source_task_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (source_membership_id, phase_id, phase_status) in memberships {
        let state = phase_state(&phase_status)?;
        let boundary = (state == "closed").then_some(revision_id);
        conn.execute(
            "insert into task_phase_memberships(project_id,phase_id,task_identity_id,boundary_revision_id,state,created_at) values(?1,?2,?3,?4,?5,current_timestamp)",
            params![project_id, phase_id, identity_id, boundary, state],
        )?;
        let membership_id = conn.last_insert_rowid();
        conn.execute(
            "insert into task_phase_membership_sources(project_id,task_phase_membership_id,source_membership_id,created_at) values(?1,?2,?3,current_timestamp)",
            params![project_id, membership_id, source_membership_id],
        )?;
        add_epoch_membership(
            conn,
            project_id,
            membership_id,
            phase_id,
            identity_id,
            boundary,
            state,
            source_membership_id,
        )?;
    }
    Ok((identity_id, digest))
}

fn ensure_item_revision(
    conn: &Connection,
    project_id: i64,
    task_identity_id: i64,
    identity_digest: &str,
    material: &ItemMaterial,
    explicit_reconciliation: bool,
) -> Result<i64> {
    let revision_digest = item_revision_digest(identity_digest, material);
    let revision_id = match conn
        .query_row(
            "select id,task_identity_id,status from task_revisions where project_id=?1 and revision_digest=?2",
            params![project_id, revision_digest],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, String>(2)?)),
        )
        .optional()?
    {
        Some((id, identity, status)) if identity == task_identity_id => {
            if explicit_reconciliation && status != "current" {
                conn.execute(
                    "update task_revisions set status='historical' where project_id=?1 and task_identity_id=?2 and status='current'",
                    params![project_id, task_identity_id],
                )?;
                conn.execute(
                    "update task_revisions set status='current' where id=?1",
                    [id],
                )?;
            }
            id
        }
        Some(_) => bail!("decomposition task revision belongs to another identity"),
        None => {
            let current = conn
                .query_row(
                    "select id from task_revisions where project_id=?1 and task_identity_id=?2 and status='current'",
                    params![project_id, task_identity_id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?;
            if current.is_some() && !explicit_reconciliation {
                bail!("a new task revision requires explicit decomposition reconciliation");
            }
            if let Some(current) = current {
                conn.execute(
                    "update task_revisions set status='historical' where id=?1 and status='current'",
                    [current],
                )?;
            }
            conn.execute(
                "insert into task_revisions(project_id,task_identity_id,source_design_requirement_id,revision_digest,design_sequence,status,created_at) values(?1,?2,?3,?4,null,'current',current_timestamp)",
                params![project_id, task_identity_id, material.requirements[0].0, revision_digest],
            )?;
            conn.last_insert_rowid()
        }
    };
    for (requirement_id, _, _) in &material.requirements {
        conn.execute(
            "insert or ignore into task_revision_requirements(project_id,task_revision_id,design_requirement_id,created_at) values(?1,?2,?3,current_timestamp)",
            params![project_id, revision_id, requirement_id],
        )?;
    }
    Ok(revision_id)
}

fn item_revision_digest(identity_digest: &str, material: &ItemMaterial) -> String {
    domain_digest(
        b"AWB-DECOMPOSITION-TASK-REVISION-v1\0",
        &CanonicalValue::object([
            (
                "identity",
                CanonicalValue::string(identity_digest.to_string()),
            ),
            ("title", CanonicalValue::string(material.title.clone())),
            ("details", CanonicalValue::string(material.details.clone())),
            ("outcome", CanonicalValue::string(material.outcome.clone())),
            (
                "requirements",
                CanonicalValue::Array(
                    material
                        .requirements
                        .iter()
                        .map(|(_, key, hash)| {
                            CanonicalValue::object([
                                ("key", CanonicalValue::string(key.clone())),
                                ("revision", CanonicalValue::string(hash.clone())),
                            ])
                        })
                        .collect(),
                ),
            ),
        ]),
    )
}

fn add_membership(
    conn: &Connection,
    project_id: i64,
    task_identity_id: i64,
    revision_id: i64,
    phase_id: i64,
    task_id: i64,
    state: &str,
) -> Result<()> {
    let source_membership_id = conn
        .query_row(
            "select id from work_phase_task_memberships where project_id=?1 and phase_id=?2 and task_id=?3",
            params![project_id, phase_id, task_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .context("decomposition task has no published phase membership")?;
    let boundary = (state == "closed").then_some(revision_id);
    conn.execute(
        "insert into task_phase_memberships(project_id,phase_id,task_identity_id,boundary_revision_id,state,created_at) values(?1,?2,?3,?4,?5,current_timestamp)",
        params![project_id, phase_id, task_identity_id, boundary, state],
    )?;
    let membership_id = conn.last_insert_rowid();
    conn.execute(
        "insert into task_phase_membership_sources(project_id,task_phase_membership_id,source_membership_id,created_at) values(?1,?2,?3,current_timestamp)",
        params![project_id, membership_id, source_membership_id],
    )?;
    add_epoch_membership(
        conn,
        project_id,
        membership_id,
        phase_id,
        task_identity_id,
        boundary,
        state,
        source_membership_id,
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn add_epoch_membership(
    conn: &Connection,
    project_id: i64,
    membership_id: i64,
    phase_id: i64,
    task_identity_id: i64,
    boundary_revision_id: Option<i64>,
    state: &str,
    source_membership_id: i64,
) -> Result<()> {
    let epoch_state = match state {
        "open" | "blocked" => "current",
        "closed" => "closed",
        "out_of_scope" => "out_of_scope",
        "split" => "split",
        _ => bail!("unsupported phase membership epoch state"),
    };
    conn.execute(
        "insert into phase_epoch_memberships(id,project_id,phase_epoch_id,task_identity_id,boundary_revision_id,state,predecessor_membership_id,created_at,terminal_at) values(?1,?2,?3,?4,?5,?6,null,current_timestamp,case when ?6='current' then null else current_timestamp end)",
        params![
            membership_id,
            project_id,
            phase_id,
            task_identity_id,
            boundary_revision_id,
            epoch_state
        ],
    )?;
    conn.execute(
        "insert into phase_epoch_membership_sources(project_id,phase_epoch_membership_id,source_membership_id,source_generation,created_at) values(?1,?2,?3,15,current_timestamp)",
        params![project_id, membership_id, source_membership_id],
    )?;
    Ok(())
}

fn task_state(status: &str) -> Result<&'static str> {
    match status {
        "open" => Ok("open"),
        "blocked" => Ok("blocked"),
        "closed" => Ok("closed"),
        "accepted_out_of_scope" => Ok("out_of_scope"),
        _ => bail!("unsupported task state for decomposition identity"),
    }
}

fn phase_state(status: &str) -> Result<&'static str> {
    match status {
        "open" => Ok("open"),
        "blocked" => Ok("blocked"),
        "closed" => Ok("closed"),
        "accepted_out_of_scope" => Ok("out_of_scope"),
        "split" => Ok("split"),
        _ => bail!("unsupported phase state for decomposition identity"),
    }
}
