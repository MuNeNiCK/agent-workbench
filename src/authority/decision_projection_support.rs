use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use super::owner_decisions::OwnerDecisionRequest;

pub(crate) fn expected_current_for_target(
    conn: &rusqlite::Connection,
    project: i64,
    request: &OwnerDecisionRequest<'_>,
) -> Result<Option<String>> {
    if request.action == "correct_terminal" {
        let (historical, boundary) = parse_review_correction_target(request.target_ref)?;
        return conn.query_row("select s.snapshot_handle from review_boundary_snapshots s join owner_decisions od on od.id=s.historical_owner_decision_id where s.project_id=?1 and s.boundary_handle=?2 and od.decision_handle=?3 and s.status='current'",params![project,boundary,historical],|row|row.get(0)).optional().map_err(Into::into);
    }
    if request.action == "reopen" {
        let (finding, epoch) = parse_finding_epoch_target(request.target_ref)?;
        return conn.query_row("select od.decision_handle from finding_decision_epochs e join owner_decisions od on od.id=e.terminal_decision_id where e.project_id=?1 and e.finding_id=?2 and e.epoch_number=?3 and e.status='terminal'",params![project,finding,epoch],|row|row.get(0)).optional().map_err(Into::into);
    }
    conn.query_row("select decision_handle from owner_decisions where project_id=?1 and owner_ref=?2 and target_ref=?3 and decision_family=?4 order by id desc limit 1",params![project,request.owner_ref,request.target_ref,request.decision_family],|row|row.get(0)).optional().map_err(Into::into)
}

pub(super) fn parse_review_correction_target(target: &str) -> Result<(&str, &str)> {
    target
        .strip_prefix("review_correction:")
        .context("review correction target has the wrong tag")?
        .split_once(':')
        .context("review correction target is incomplete")
}

pub(super) fn parse_finding_epoch_target(target: &str) -> Result<(i64, i64)> {
    let (finding, epoch) = target
        .strip_prefix("finding_epoch:")
        .context("finding epoch target has the wrong tag")?
        .split_once(':')
        .context("finding epoch target is incomplete")?;
    let result = (finding.parse::<i64>()?, epoch.parse::<i64>()?);
    if result.0 <= 0 || result.1 <= 0 {
        bail!("finding epoch target ids must be positive");
    }
    Ok(result)
}

pub(super) fn apply_review_correction(
    conn: &rusqlite::Connection,
    project: i64,
    owner_decision_id: i64,
    request: &OwnerDecisionRequest<'_>,
) -> Result<()> {
    if !matches!(
        request.decision_value,
        "accepted" | "rejected" | "needs_evidence"
    ) {
        bail!("review correction outcome is not allowed");
    }
    let (historical, boundary) = parse_review_correction_target(request.target_ref)?;
    let (historical_id,plan_id,owner,dependency):(i64,i64,String,String)=conn.query_row("select od.id,r.review_plan_id,s.owner_ref,s.dependency_digest from owner_decisions od join review_adjudication_decisions d on d.owner_decision_id=od.id join review_runs r on r.id=d.review_run_id join review_boundary_snapshots s on s.historical_owner_decision_id=od.id and s.boundary_handle=?3 and s.status='current' where od.project_id=?1 and od.decision_handle=?2",params![project,historical,boundary],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).context("historical review decision not found")?;
    conn.execute("insert into review_correction_events(project_id,owner_decision_id,historical_owner_decision_id,boundary_handle,outcome,created_at) values(?1,?2,?3,?4,?5,current_timestamp)",params![project,owner_decision_id,historical_id,boundary,request.decision_value])?;
    let correction = conn.last_insert_rowid();
    conn.execute("insert into review_correction_recovery_obligations(project_id,correction_event_id,owner_ref,obligation,status,created_at) values(?1,?2,?3,?4,'open',current_timestamp)",params![project,correction,owner,format!("revalidate_dependencies:{dependency}")])?;
    conn.execute("update review_boundary_snapshots set status='invalidated',invalidated_at=current_timestamp where project_id=?1 and boundary_handle=?2 and status='current'",params![project,boundary])?;
    conn.execute(
        "update review_plans set status='blocked' where project_id=?1 and id=?2 and status='clean'",
        params![project, plan_id],
    )?;
    let affected_work = if let Some(raw) = owner.strip_prefix("design_version:") {
        let design_version = raw.parse::<i64>()?;
        conn.execute("update task_derivations set status='stale' where project_id=?1 and status='active' and design_requirement_id in(select id from design_requirements where project_id=?1 and design_version_id=?2)",params![project,design_version])?;
        conn.execute("update checklists set status='stale' where project_id=?1 and design_version_id=?2 and status='active'",params![project,design_version])?;
        conn.execute("update validation_gates set status='stale' where project_id=?1 and status='active' and design_requirement_id in(select id from design_requirements where project_id=?1 and design_version_id=?2)",params![project,design_version])?;
        conn.prepare("select distinct t.work_unit_id from tasks t join task_derivations d on d.task_id=t.id join design_requirements r on r.id=d.design_requirement_id where r.project_id=?1 and r.design_version_id=?2 order by t.work_unit_id")?.query_map(params![project,design_version],|row|row.get::<_,i64>(0))?.collect::<rusqlite::Result<Vec<_>>>()?
    } else if let Some(raw) = owner.strip_prefix("work_unit:") {
        vec![raw.parse::<i64>()?]
    } else {
        Vec::new()
    };
    for work in affected_work {
        let activations=conn.prepare("select id,status from work_unit_activations where project_id=?1 and work_unit_id=?2 and status in ('active','suspended') order by id")?.query_map(params![project,work],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
        conn.execute("update work_units set status='blocked' where project_id=?1 and id=?2 and status='open'",params![project,work])?;
        for (activation, status) in activations {
            if status == "active" {
                conn.execute("update work_unit_activations set status='suspended',suspended_at=current_timestamp where project_id=?1 and id=?2",params![project,activation])?;
            }
            conn.execute("insert into work_unit_events(work_unit_id,work_unit_activation_id,event_type,reason,status_domain,previous_status,next_status,created_at) values(?1,?2,'invalidated',?3,'activation',?4,'suspended',current_timestamp)",params![work,activation,format!("terminal review correction at {boundary}"),status])?;
        }
    }
    Ok(())
}

pub(super) fn apply_finding_reopen(
    conn: &rusqlite::Connection,
    project: i64,
    owner_decision_id: i64,
    request: &OwnerDecisionRequest<'_>,
) -> Result<()> {
    if request.decision_value != "reopened" {
        bail!("finding reopen outcome is fixed");
    }
    let (finding, epoch) = parse_finding_epoch_target(request.target_ref)?;
    let state:String=conn.query_row("select f.lifecycle_state from findings f join finding_decision_epochs e on e.finding_id=f.id where f.project_id=?1 and f.id=?2 and e.epoch_number=?3 and e.status='terminal'",params![project,finding,epoch],|row|row.get(0)).context("terminal finding epoch not found")?;
    if state != "closed" {
        bail!("finding epoch is not terminal");
    }
    conn.execute("insert into finding_decision_epochs(project_id,finding_id,epoch_number,reopen_decision_id,status,created_at) values(?1,?2,?3,?4,'open',current_timestamp)",params![project,finding,epoch+1,owner_decision_id])?;
    conn.execute(
        "update closures set status='superseded',superseded_at=current_timestamp,supersession_reason='finding reopened by owner decision' where project_id=?1 and finding_id=?2 and status!='superseded'",
        params![project, finding],
    )?;
    conn.execute(
        "update correction_sessions set status='superseded',completed_at=current_timestamp where project_id=?1 and finding_id=?2 and status='active'",
        params![project, finding],
    )?;
    conn.execute(
        "update correction_tokens set status='superseded' where project_id=?1 and status='pending' and closure_id in(select id from closures where project_id=?1 and finding_id=?2)",
        params![project, finding],
    )?;
    conn.execute("update findings set lifecycle_state='open',status='open',close_reason=null where project_id=?1 and id=?2",params![project,finding])?;
    conn.execute("insert into finding_lifecycle_events(project_id,finding_id,owner_decision_id,from_state,to_state,effect,created_at) values(?1,?2,?3,'closed','open','authority_reopen',current_timestamp)",params![project,finding,owner_decision_id])?;
    Ok(())
}

pub(super) fn terminalize_finding_epoch(
    conn: &rusqlite::Connection,
    project: i64,
    finding: i64,
    owner_decision_id: i64,
) -> Result<()> {
    let open_epoch: Option<i64> = conn
        .query_row(
            "select max(epoch_number) from finding_decision_epochs where project_id=?1 and finding_id=?2 and status='open'",
            params![project, finding],
            |row| row.get(0),
        )?;
    if let Some(epoch) = open_epoch {
        conn.execute(
            "update finding_decision_epochs set terminal_decision_id=?1,status='terminal' where project_id=?2 and finding_id=?3 and epoch_number=?4 and status='open'",
            params![owner_decision_id, project, finding, epoch],
        )?;
    } else {
        let epoch: i64 = conn.query_row(
            "select coalesce(max(epoch_number),0)+1 from finding_decision_epochs where project_id=?1 and finding_id=?2",
            params![project, finding],
            |row| row.get(0),
        )?;
        conn.execute(
            "insert into finding_decision_epochs(project_id,finding_id,epoch_number,terminal_decision_id,status,created_at) values(?1,?2,?3,?4,'terminal',current_timestamp)",
            params![project, finding, epoch, owner_decision_id],
        )?;
    }
    Ok(())
}
