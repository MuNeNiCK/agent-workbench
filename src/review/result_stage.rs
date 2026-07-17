use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{open_existing_project, project_id};
use crate::identity::{
    CanonicalValue, PrincipalHandle, ReviewResultItemHandle, ReviewResultStageHandle,
    ReviewResultVersionHandle, domain_digest,
};

#[derive(Clone, Debug)]
pub struct ResultStageOutcome {
    pub stage_handle: String,
    pub version_handle: String,
    pub state: String,
    pub result_handle: Option<String>,
}

pub struct CreateResultStageRequest<'a> {
    pub invocation_id: i64,
    pub principal: &'a str,
    pub expected_current: &'a str,
    pub idempotency_key: &'a str,
}

pub fn create_result_stage(
    root: &Path,
    request: CreateResultStageRequest<'_>,
) -> Result<ResultStageOutcome> {
    if !matches!(request.expected_current, "requested" | "running")
        || request.idempotency_key.is_empty()
    {
        bail!("result stage requires a current invocation and idempotency key");
    }
    let principal = PrincipalHandle::parse(request.principal)?;
    let payload = CanonicalValue::object([
        ("invocation", CanonicalValue::Integer(request.invocation_id)),
        ("principal", CanonicalValue::string(principal.as_str())),
        ("expected", CanonicalValue::string(request.expected_current)),
    ]);
    let digest = domain_digest(b"agent-workbench:review-result-stage-create-v1\0", &payload);
    let handle = ReviewResultStageHandle::derive(
        b"agent-workbench:review-result-stage-v1\0",
        &CanonicalValue::object([
            ("payload", payload),
            ("key", CanonicalValue::string(request.idempotency_key)),
        ]),
    );
    let version = version_handle(handle.as_str(), 0);
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    let principal_id = resolve_principal(&tx, project, principal.as_str())?;
    let existing:Option<(String,String,String)>=tx.query_row("select stage_handle,version_handle,status from review_result_stages where project_id=?1 and invocation_id=?2 and reviewer_principal_id=?3 and create_idempotency_key=?4 and create_payload_digest=?5",params![project,request.invocation_id,principal_id,request.idempotency_key,digest],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
    if let Some((stage, version, state)) = existing {
        return Ok(ResultStageOutcome {
            stage_handle: stage,
            version_handle: version,
            state,
            result_handle: None,
        });
    }
    let drift:i64=tx.query_row("select exists(select 1 from review_result_stages where project_id=?1 and invocation_id=?2 and reviewer_principal_id=?3 and create_idempotency_key=?4)",params![project,request.invocation_id,principal_id,request.idempotency_key],|row|row.get(0))?;
    if drift == 1 {
        bail!("result_stage_idempotency_payload_mismatch");
    }
    let (holder,current,purpose):(i64,String,String)=tx.query_row("select reviewer_principal_id,status,purpose from review_agent_invocations where project_id=?1 and id=?2",params![project,request.invocation_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).context("invocation not found")?;
    if holder != principal_id
        || current != request.expected_current
        || purpose != "new_unbiased_review"
    {
        bail!("result stage does not bind a current findings invocation");
    }
    let active:Option<String>=tx.query_row("select stage_handle from review_result_stages where project_id=?1 and invocation_id=?2 and status='staging'",params![project,request.invocation_id],|row|row.get(0)).optional()?;
    if let Some(active) = active {
        bail!("active_result_stage:{active}");
    }
    tx.execute("insert into review_result_stages(project_id,stage_handle,invocation_id,reviewer_principal_id,status,version,version_handle,create_idempotency_key,create_payload_digest,created_at) values(?1,?2,?3,?4,'staging',0,?5,?6,?7,current_timestamp)",params![project,handle.as_str(),request.invocation_id,principal_id,version.as_str(),request.idempotency_key,digest])?;
    tx.commit()?;
    Ok(ResultStageOutcome {
        stage_handle: handle.as_str().into(),
        version_handle: version.as_str().into(),
        state: "staging".into(),
        result_handle: None,
    })
}

pub struct AddResultFindingRequest<'a> {
    pub stage_handle: &'a str,
    pub finding_type: &'a str,
    pub severity: &'a str,
    pub description: &'a str,
    pub requirement: Option<i64>,
    pub task: Option<i64>,
    pub principal: &'a str,
    pub expected_current: &'a str,
    pub idempotency_key: &'a str,
}

pub fn add_result_finding(
    root: &Path,
    request: AddResultFindingRequest<'_>,
) -> Result<ResultStageOutcome> {
    if !matches!(request.severity, "critical" | "high" | "medium" | "low")
        || request.description.is_empty()
    {
        bail!("invalid staged finding");
    }
    let stage = ReviewResultStageHandle::parse(request.stage_handle)?;
    let principal = PrincipalHandle::parse(request.principal)?;
    let payload = CanonicalValue::object([
        ("stage", CanonicalValue::string(stage.as_str())),
        ("type", CanonicalValue::string(request.finding_type)),
        ("severity", CanonicalValue::string(request.severity)),
        ("description", CanonicalValue::string(request.description)),
        (
            "requirement",
            request
                .requirement
                .map_or(CanonicalValue::Null, CanonicalValue::Integer),
        ),
        (
            "task",
            request
                .task
                .map_or(CanonicalValue::Null, CanonicalValue::Integer),
        ),
        ("principal", CanonicalValue::string(principal.as_str())),
        ("expected", CanonicalValue::string(request.expected_current)),
    ]);
    let digest = domain_digest(b"agent-workbench:review-result-finding-add-v1\0", &payload);
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    let principal_id = resolve_principal(&tx, project, principal.as_str())?;
    let (stage_id,holder,status,version,stored_version):(i64,i64,String,i64,String)=tx.query_row("select id,reviewer_principal_id,status,version,version_handle from review_result_stages where project_id=?1 and stage_handle=?2",params![project,stage.as_str()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).context("result stage not found")?;
    if let Some((stored_digest, result)) = lookup_stage_audit(
        &tx,
        project,
        stage_id,
        principal_id,
        "finding_add",
        request.idempotency_key,
    )? {
        if stored_digest != digest {
            bail!("result_stage_idempotency_payload_mismatch");
        }
        let item_version:i64=tx.query_row("select item_version from review_result_stage_items where project_id=?1 and item_handle=?2",params![project,result],|row|row.get(0))?;
        let current = version_handle(stage.as_str(), item_version)
            .as_str()
            .to_string();
        return Ok(ResultStageOutcome {
            stage_handle: stage.as_str().into(),
            version_handle: current,
            state: status,
            result_handle: Some(result),
        });
    }
    if holder != principal_id || status != "staging" || stored_version != request.expected_current {
        bail!("stale_or_terminal_result_stage");
    }
    let next = version + 1;
    let item = ReviewResultItemHandle::derive(b"agent-workbench:review-result-item-v1\0", &payload);
    let next_version = version_handle(stage.as_str(), next);
    tx.execute("insert into review_result_stage_items(project_id,stage_id,item_handle,item_version,finding_type,severity,description,design_requirement_id,task_id,created_at) values(?1,?2,?3,?4,?5,?6,?7,?8,?9,current_timestamp)",params![project,stage_id,item.as_str(),next,request.finding_type,request.severity,request.description,request.requirement,request.task])?;
    tx.execute("update review_result_stages set version=?1,version_handle=?2 where id=?3 and status='staging' and version=?4",params![next,next_version.as_str(),stage_id,version])?;
    insert_stage_audit(
        &tx,
        StageAudit {
            project,
            stage: stage_id,
            principal: principal_id,
            command: "finding_add",
            key: request.idempotency_key,
            digest: &digest,
            result: item.as_str(),
        },
    )?;
    tx.commit()?;
    Ok(ResultStageOutcome {
        stage_handle: stage.as_str().into(),
        version_handle: next_version.as_str().into(),
        state: "staging".into(),
        result_handle: Some(item.as_str().into()),
    })
}

pub struct CompleteResultStageRequest<'a> {
    pub stage_handle: &'a str,
    pub expected_findings: i64,
    pub summary: &'a str,
    pub principal: &'a str,
    pub expected_current: &'a str,
    pub invocation_current: &'a str,
    pub idempotency_key: &'a str,
}
pub fn complete_result_stage(
    root: &Path,
    request: CompleteResultStageRequest<'_>,
) -> Result<ResultStageOutcome> {
    if request.expected_findings <= 0
        || !matches!(request.invocation_current, "requested" | "running")
    {
        bail!("invalid result stage completion");
    }
    let stage = ReviewResultStageHandle::parse(request.stage_handle)?;
    let principal = PrincipalHandle::parse(request.principal)?;
    let payload = CanonicalValue::object([
        ("stage", CanonicalValue::string(stage.as_str())),
        ("count", CanonicalValue::Integer(request.expected_findings)),
        ("summary", CanonicalValue::string(request.summary)),
        ("principal", CanonicalValue::string(principal.as_str())),
        ("expected", CanonicalValue::string(request.expected_current)),
        (
            "invocation",
            CanonicalValue::string(request.invocation_current),
        ),
    ]);
    let digest = domain_digest(b"agent-workbench:review-result-complete-v1\0", &payload);
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    let principal_id = resolve_principal(&tx, project, principal.as_str())?;
    let (stage_id,invocation_id,holder,status,stored_version):(i64,i64,i64,String,String)=tx.query_row("select id,invocation_id,reviewer_principal_id,status,version_handle from review_result_stages where project_id=?1 and stage_handle=?2",params![project,stage.as_str()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).context("result stage not found")?;
    if let Some((stored_digest, result)) = lookup_stage_audit(
        &tx,
        project,
        stage_id,
        principal_id,
        "complete",
        request.idempotency_key,
    )? {
        if stored_digest != digest {
            bail!("result_stage_idempotency_payload_mismatch");
        }
        return Ok(ResultStageOutcome {
            stage_handle: stage.as_str().into(),
            version_handle: stored_version,
            state: status,
            result_handle: Some(result),
        });
    }
    if holder != principal_id || status != "staging" || stored_version != request.expected_current {
        bail!("stale_or_terminal_result_stage");
    }
    let (inv_holder,inv_status,plan_id,run_type,purpose):(i64,String,i64,String,String)=tx.query_row("select reviewer_principal_id,status,review_plan_id,run_type,purpose from review_agent_invocations where project_id=?1 and id=?2",params![project,invocation_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)))?;
    if inv_holder != principal_id
        || inv_status != request.invocation_current
        || purpose != "new_unbiased_review"
    {
        bail!("result stage invocation is stale");
    }
    let count: i64 = tx.query_row(
        "select count(*) from review_result_stage_items where stage_id=?1",
        params![stage_id],
        |row| row.get(0),
    )?;
    if count != request.expected_findings {
        bail!("staged finding inventory count mismatch");
    }
    let (scope,work,provenance_kind,provenance_handle):(Option<i64>,i64,String,String)=tx.query_row("select p.review_scope_id,p.work_unit_id,pr.provenance_kind,pr.provenance_handle from review_plans p join review_agent_invocations i on i.review_plan_id=p.id join review_provenance_records pr on pr.id=i.review_provenance_id where i.id=?1",params![invocation_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)))?;
    tx.execute("insert into review_runs(project_id,review_scope_id,review_plan_id,run_type,run_purpose,target_type,work_unit_id,target_ref,result_summary,new_findings_count,carried_findings_checked,clean_run,review_provenance,review_provenance_ref,status,created_at) select project_id,?1,?2,?3,purpose,'work_unit',?4,target_context,?5,?6,0,0,?7,?8,'completed',current_timestamp from review_agent_invocations where id=?9",params![scope,plan_id,run_type,work,request.summary,count,if provenance_kind=="human_review"{"human_review"}else{"external_agent"},provenance_handle,invocation_id])?;
    let run_id = tx.last_insert_rowid();
    tx.execute("insert into findings(project_id,review_run_id,finding_type,severity,description,design_requirement_id,task_id,created_at) select project_id,?1,finding_type,severity,description,design_requirement_id,task_id,current_timestamp from review_result_stage_items where stage_id=?2 order by item_version",params![run_id,stage_id])?;
    tx.execute("update review_agent_invocations set status='completed',claim='findings',result_summary=?1,review_run_id=?2,finished_at=current_timestamp where id=?3 and status=?4",params![request.summary,run_id,invocation_id,request.invocation_current])?;
    let result = format!("review_run:{run_id}");
    tx.execute("update review_result_stages set status='completed',review_run_id=?1,completed_at=current_timestamp where id=?2 and status='staging'",params![run_id,stage_id])?;
    insert_stage_audit(
        &tx,
        StageAudit {
            project,
            stage: stage_id,
            principal: principal_id,
            command: "complete",
            key: request.idempotency_key,
            digest: &digest,
            result: &result,
        },
    )?;
    tx.commit()?;
    Ok(ResultStageOutcome {
        stage_handle: stage.as_str().into(),
        version_handle: stored_version,
        state: "completed".into(),
        result_handle: Some(result),
    })
}

pub struct CancelResultStageRequest<'a> {
    pub stage_handle: &'a str,
    pub reason: &'a str,
    pub principal: &'a str,
    pub expected_current: &'a str,
    pub idempotency_key: &'a str,
}
pub fn cancel_result_stage(
    root: &Path,
    request: CancelResultStageRequest<'_>,
) -> Result<ResultStageOutcome> {
    if request.reason.is_empty() {
        bail!("stage cancellation reason is required");
    }
    let stage = ReviewResultStageHandle::parse(request.stage_handle)?;
    let principal = PrincipalHandle::parse(request.principal)?;
    let payload = CanonicalValue::object([
        ("stage", CanonicalValue::string(stage.as_str())),
        ("reason", CanonicalValue::string(request.reason)),
        ("principal", CanonicalValue::string(principal.as_str())),
        ("expected", CanonicalValue::string(request.expected_current)),
    ]);
    let digest = domain_digest(b"agent-workbench:review-result-cancel-v1\0", &payload);
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    let principal_id = resolve_principal(&tx, project, principal.as_str())?;
    let(stage_id,holder,status,stored_version):(i64,i64,String,String)=tx.query_row("select id,reviewer_principal_id,status,version_handle from review_result_stages where project_id=?1 and stage_handle=?2",params![project,stage.as_str()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)))?;
    if let Some((stored_digest, result)) = lookup_stage_audit(
        &tx,
        project,
        stage_id,
        principal_id,
        "cancel",
        request.idempotency_key,
    )? {
        if stored_digest != digest {
            bail!("result_stage_idempotency_payload_mismatch");
        }
        return Ok(ResultStageOutcome {
            stage_handle: stage.as_str().into(),
            version_handle: stored_version,
            state: status,
            result_handle: Some(result),
        });
    }
    if holder != principal_id || status != "staging" || stored_version != request.expected_current {
        bail!("stale_or_terminal_result_stage");
    }
    tx.execute("update review_result_stages set status='cancelled',terminal_reason=?1,completed_at=current_timestamp where id=?2 and status='staging'",params![request.reason,stage_id])?;
    insert_stage_audit(
        &tx,
        StageAudit {
            project,
            stage: stage_id,
            principal: principal_id,
            command: "cancel",
            key: request.idempotency_key,
            digest: &digest,
            result: stage.as_str(),
        },
    )?;
    tx.commit()?;
    Ok(ResultStageOutcome {
        stage_handle: stage.as_str().into(),
        version_handle: stored_version,
        state: "cancelled".into(),
        result_handle: Some(stage.as_str().into()),
    })
}

fn version_handle(stage: &str, version: i64) -> ReviewResultVersionHandle {
    ReviewResultVersionHandle::derive(
        b"agent-workbench:review-result-version-v1\0",
        &CanonicalValue::object([
            ("stage", CanonicalValue::string(stage)),
            ("version", CanonicalValue::Integer(version)),
        ]),
    )
}
fn resolve_principal(conn: &rusqlite::Connection, project: i64, principal: &str) -> Result<i64> {
    conn.query_row(
        "select id from authority_principals where project_id=?1 and principal_handle=?2",
        params![project, principal],
        |row| row.get(0),
    )
    .context("reviewer principal is not resolved")
}
fn lookup_stage_audit(
    conn: &rusqlite::Connection,
    project: i64,
    stage: i64,
    principal: i64,
    command: &str,
    key: &str,
) -> Result<Option<(String, String)>> {
    Ok(conn.query_row("select payload_digest,result_handle from review_result_stage_audits where project_id=?1 and stage_id=?2 and principal_id=?3 and command=?4 and idempotency_key=?5",params![project,stage,principal,command,key],|row|Ok((row.get(0)?,row.get(1)?))).optional()?)
}
struct StageAudit<'a> {
    project: i64,
    stage: i64,
    principal: i64,
    command: &'a str,
    key: &'a str,
    digest: &'a str,
    result: &'a str,
}
fn insert_stage_audit(conn: &rusqlite::Connection, audit: StageAudit<'_>) -> Result<()> {
    conn.execute("insert into review_result_stage_audits(project_id,stage_id,principal_id,command,idempotency_key,payload_digest,result_handle,created_at) values(?1,?2,?3,?4,?5,?6,?7,current_timestamp)",params![audit.project,audit.stage,audit.principal,audit.command,audit.key,audit.digest,audit.result])?;
    Ok(())
}
