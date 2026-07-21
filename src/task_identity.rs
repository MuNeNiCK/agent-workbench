mod apply;
mod decomposition;
mod lock;
mod manual;
mod plan;
mod profile;
mod recovery;
pub(crate) mod schema;
mod source;
pub(crate) mod status;
pub(crate) use decomposition::materialize_decomposition_item;

pub(crate) fn revise_canonical_task(
    conn: &rusqlite::Connection,
    project_id: i64,
    task_id: i64,
    details: &str,
    outcome: &str,
) -> anyhow::Result<i64> {
    if let Some(revision) =
        decomposition::revise_decomposition_task(conn, project_id, task_id, details, outcome)?
    {
        return Ok(revision);
    }
    manual::revise_manual_task(conn, project_id, task_id, outcome)
}
pub(crate) use manual::materialize_manual_task;

use std::path::Path;

use anyhow::Result;

#[derive(Debug)]
pub struct TaskIdentityPlanOutput {
    pub classification: &'static str,
    pub json: String,
}

#[derive(Debug)]
pub struct TaskIdentityApplyOutput {
    pub classification: &'static str,
    pub result: &'static str,
    pub backup_handle: String,
    pub audit_handle: String,
}

#[derive(Debug)]
pub struct TaskIdentityAuditOutput {
    pub classification: &'static str,
    pub json: String,
}

#[derive(Debug)]
pub struct TaskIdentityAmbiguityOutput {
    pub classification: &'static str,
    pub json: String,
}

pub struct TaskIdentityAuthorityRequest<'a> {
    pub owner_handle: &'a str,
    pub plan_handle: &'a str,
    pub ambiguity_handle: &'a str,
    pub resolution_handle: Option<&'a str>,
    pub retire: bool,
    pub statement: &'a str,
    pub provenance: &'a str,
    pub provenance_ref: &'a str,
}

#[derive(Debug)]
pub struct TaskIdentityAuthorityOutput {
    pub classification: &'static str,
    pub authority_handle: String,
    pub recovery_handle: String,
    pub backup_handle: String,
}

pub struct TaskIdentityDecisionRequest<'a> {
    pub owner_handle: &'a str,
    pub plan_handle: &'a str,
    pub ambiguity_handle: &'a str,
    pub resolution_handle: Option<&'a str>,
    pub retire: bool,
    pub authority_handle: &'a str,
}

#[derive(Debug)]
pub struct TaskIdentityDecisionOutput {
    pub classification: &'static str,
    pub decision_handle: String,
    pub recovery_handle: String,
    pub json: String,
}

pub fn plan_task_identity(
    root: &Path,
    owner_handle: Option<&str>,
) -> Result<TaskIdentityPlanOutput> {
    let _lock = lock::shared(root)?;
    let snapshot = source::SourceSnapshot::open(root)?;
    let json = match owner_handle {
        None => plan::render_index(&snapshot)?,
        Some(owner_handle) => plan::render_owner_plan(&snapshot, owner_handle)?,
    };
    Ok(TaskIdentityPlanOutput {
        classification: "project-internal",
        json,
    })
}

pub fn apply_task_identity(
    root: &Path,
    owner_handle: &str,
    plan_handle: &str,
) -> Result<TaskIdentityApplyOutput> {
    apply::apply(root, owner_handle, plan_handle)
}

pub fn audit_task_identity(
    root: &Path,
    owner_handle: Option<&str>,
) -> Result<TaskIdentityAuditOutput> {
    let _lock = lock::shared(root)?;
    Ok(TaskIdentityAuditOutput {
        classification: "project-internal",
        json: apply::audit(root, owner_handle)?,
    })
}

pub fn list_task_identity_ambiguities(
    root: &Path,
    owner_handle: &str,
    plan_handle: &str,
) -> Result<TaskIdentityAmbiguityOutput> {
    let plan = plan_task_identity(root, Some(owner_handle))?;
    let value: serde_json::Value = serde_json::from_str(&plan.json)?;
    if value["plan"]["plan_handle"].as_str() != Some(plan_handle) {
        anyhow::bail!("plan handle is unknown or stale; rerun migration task-history plan");
    }
    let ambiguities = value["plan"]["ambiguities"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    Ok(TaskIdentityAmbiguityOutput {
        classification: "project-internal",
        json: serde_json::to_string(&serde_json::json!({
            "algorithm": "ID-PLAN-AMBIGUITY-VIEW-v1",
            "ambiguities": ambiguities,
        }))?,
    })
}

pub fn record_task_identity_authority(
    root: &Path,
    request: TaskIdentityAuthorityRequest<'_>,
) -> Result<TaskIdentityAuthorityOutput> {
    let _lock = lock::exclusive(root)?;
    recovery::record_authority(root, request)
}

pub fn decide_task_identity_ambiguity(
    root: &Path,
    request: TaskIdentityDecisionRequest<'_>,
) -> Result<TaskIdentityDecisionOutput> {
    let _lock = lock::exclusive(root)?;
    recovery::record_decision(root, request)
}
