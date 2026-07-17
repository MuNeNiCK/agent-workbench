use anyhow::{Context, Result, bail};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OptionalExtension, params};

use crate::identity::{CanonicalValue, domain_digest};

pub(super) fn legacy_source_digest(
    conn: &Connection,
    project: i64,
    generation: i64,
) -> Result<String> {
    let mut rows = Vec::new();
    for table in [
        "work_units",
        "work_unit_activations",
        "review_plans",
        "review_policies",
        "review_runs",
        "review_agent_invocations",
        "findings",
        "closures",
        "closure_attempts",
        "finding_verifications",
        "validation_gates",
        "validation_runs",
        "acceptance_records",
        "authority_events",
        "design_versions",
    ] {
        if conn.query_row(
            "select exists(select 1 from sqlite_schema where type='table' and name=?1)",
            params![table],
            |row| row.get::<_, i64>(0),
        )? == 0
        {
            continue;
        }
        rows.extend(snapshot_query_rows(
            conn,
            table,
            &format!("select * from {table} where project_id=?1 order by id"),
            project,
        )?);
    }
    if conn.query_row(
        "select exists(select 1 from sqlite_schema where type='table' and name='work_unit_events')",
        [],
        |row| row.get::<_, i64>(0),
    )? == 1
    {
        rows.extend(snapshot_query_rows(conn, "work_unit_events", "select e.* from work_unit_events e join work_units w on w.id=e.work_unit_id where w.project_id=?1 order by e.id", project)?);
    }
    if conn.query_row("select exists(select 1 from sqlite_schema where type='table' and name='review_plan_targets')", [], |row| row.get::<_, i64>(0))? == 1 {
        rows.extend(snapshot_query_rows(conn, "review_plan_targets", "select t.* from review_plan_targets t join review_plans p on p.id=t.review_plan_id where p.project_id=?1 order by t.id", project)?);
    }
    rows.sort();
    Ok(domain_digest(
        b"agent-workbench:legacy-source-ledger-v1\0",
        &CanonicalValue::object([
            ("generation", CanonicalValue::Integer(generation)),
            (
                "rows",
                CanonicalValue::Array(rows.into_iter().map(CanonicalValue::String).collect()),
            ),
        ]),
    ))
}

fn snapshot_query_rows(
    conn: &Connection,
    table: &str,
    query: &str,
    project: i64,
) -> Result<Vec<String>> {
    let mut statement = conn.prepare(query)?;
    let column_count = statement.column_count();
    let mapped = statement.query_map(params![project], |row| {
        let mut encoded = Vec::with_capacity(column_count);
        for index in 0..column_count {
            let value = match row.get_ref(index)? {
                ValueRef::Null => "n:".to_owned(),
                ValueRef::Integer(value) => format!("i:{value}"),
                ValueRef::Real(value) => format!("r:{:016x}", value.to_bits()),
                ValueRef::Text(value) => format!("t:hex:{}", encode_hex(value)),
                ValueRef::Blob(value) => {
                    let mut hex = String::with_capacity(value.len() * 2 + 2);
                    hex.push_str("b:");
                    for byte in value {
                        use std::fmt::Write as _;
                        let _ = write!(hex, "{byte:02x}");
                    }
                    hex
                }
            };
            encoded.push(value);
        }
        Ok(format!("{table}:{}", encoded.join("\u{1f}")))
    })?;
    Ok(mapped.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn encode_hex(value: &[u8]) -> String {
    let mut hex = String::with_capacity(value.len() * 2);
    for byte in value {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

pub(super) fn validate_schema11_invalid_combinations(
    conn: &Connection,
    project: i64,
) -> Result<()> {
    let clean_with_findings:i64=conn.query_row("select exists(select 1 from review_runs r where r.project_id=?1 and r.status='completed' and r.clean_run=1 and (r.new_findings_count>0 or exists(select 1 from findings f where f.review_run_id=r.id)))",params![project],|row|row.get(0))?;
    if clean_with_findings == 1 {
        bail!("migration ambiguity: clean_with_findings");
    }
    let orphan_verification:i64=conn.query_row("select exists(select 1 from finding_verifications v left join closure_attempts a on a.id=v.closure_attempt_id left join closures c on c.id=v.closure_id where v.project_id=?1 and v.result in ('verified','not_fixed','needs_evidence') and (v.closure_attempt_id is null or a.id is null or a.closure_id!=v.closure_id or c.finding_id!=v.finding_id))",params![project],|row|row.get(0))?;
    if orphan_verification == 1 {
        bail!("migration ambiguity: applied_verification_without_exact_attempt");
    }
    let terminal_without_authority:i64=conn.query_row("select exists(select 1 from findings f where f.project_id=?1 and f.status='accepted_out_of_scope' and not exists(select 1 from acceptance_records a where a.project_id=f.project_id and a.finding_id=f.id and a.status='approved'))",params![project],|row|row.get(0))?;
    if terminal_without_authority == 1 {
        bail!("migration ambiguity: terminal_disposition_without_matching_authority");
    }
    Ok(())
}

pub(super) fn normalize_schema11_adjudication(conn: &Connection, project: i64) -> Result<()> {
    if conn.query_row(
        "select exists(select 1 from legacy_migration_candidates where project_id=?1)",
        params![project],
        |row| row.get::<_, i64>(0),
    )? == 1
    {
        return Ok(());
    }
    for (table, kind, query) in [
        (
            "work_units",
            "work_owner",
            "select id,coalesce(status,'') from work_units where project_id=?1 order by id",
        ),
        (
            "review_plans",
            "plan_gate",
            "select id,coalesce(status,'') from review_plans where project_id=?1 order by id",
        ),
        (
            "findings",
            "finding_epoch",
            "select id,coalesce(lifecycle_state,'') from findings where project_id=?1 order by id",
        ),
    ] {
        insert_query_candidates(conn, project, table, kind, query)?;
    }
    insert_query_candidates(
        conn,
        project,
        "validation_gates",
        "plan_gate",
        "select id,coalesce(status,'') from validation_gates where project_id=?1 order by id",
    )?;
    insert_query_candidates(
        conn,
        project,
        "review_policies",
        "plan_gate",
        "select id,'policy_epoch' from review_policies where project_id=?1 order by id",
    )?;
    let runs = conn
        .prepare("select id,status from review_runs where project_id=?1 order by id")?
        .query_map(params![project], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, status) in runs {
        insert_candidate(
            conn,
            project,
            "review_runs",
            id,
            if status == "completed" {
                "completed_run"
            } else {
                "invocation"
            },
            &status,
        )?;
    }
    let standalone_invocations=conn.prepare("select id,status from review_agent_invocations where project_id=?1 and review_run_id is null order by id")?.query_map(params![project],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, status) in standalone_invocations {
        insert_candidate(
            conn,
            project,
            "review_agent_invocations",
            id,
            "invocation",
            &status,
        )?;
    }
    let boundaries=conn.prepare("select e.id,coalesce(e.next_status,'') from work_unit_events e join work_units w on w.id=e.work_unit_id where w.project_id=?1 and e.event_type='closed' order by e.id")?.query_map(params![project],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, status) in boundaries {
        insert_candidate(
            conn,
            project,
            "work_unit_events",
            id,
            "completed_boundary",
            &status,
        )?;
    }
    let approvals=conn.prepare("select id,status from design_versions where project_id=?1 and status='approved' order by id")?.query_map(params![project],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, status) in approvals {
        insert_candidate(
            conn,
            project,
            "design_versions",
            id,
            "completed_boundary",
            &status,
        )?;
    }
    assign_boundary_coordinates(conn, project)?;

    let invocation_members=conn.prepare("select id,review_run_id from review_agent_invocations where project_id=?1 and review_run_id is not null order by id")?.query_map(params![project],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (member, run) in invocation_members {
        insert_member_for_root(
            conn,
            project,
            "review_runs",
            run,
            "review_agent_invocations",
            member,
        )?;
    }
    let plan_members=conn.prepare("select id,review_plan_id from review_plan_targets where review_plan_id in(select id from review_plans where project_id=?1) order by id")?.query_map(params![project],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (member, plan) in plan_members {
        insert_member_for_root(
            conn,
            project,
            "review_plans",
            plan,
            "review_plan_targets",
            member,
        )?;
    }
    let closures = conn
        .prepare("select id,finding_id from closures where project_id=?1 order by id")?
        .query_map(params![project], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (member, finding) in closures {
        insert_member_for_root(conn, project, "findings", finding, "closures", member)?;
    }
    let attempts=conn.prepare("select a.id,c.finding_id from closure_attempts a join closures c on c.id=a.closure_id where a.project_id=?1 order by a.id")?.query_map(params![project],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (member, finding) in attempts {
        insert_member_for_root(
            conn,
            project,
            "findings",
            finding,
            "closure_attempts",
            member,
        )?;
    }
    let verifications = conn
        .prepare("select id,finding_id from finding_verifications where project_id=?1 order by id")?
        .query_map(params![project], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (member, finding) in verifications {
        insert_member_for_root(
            conn,
            project,
            "findings",
            finding,
            "finding_verifications",
            member,
        )?;
    }
    let activations=conn.prepare("select a.id,a.work_unit_id from work_unit_activations a join work_units w on w.id=a.work_unit_id where w.project_id=?1 order by a.id")?.query_map(params![project],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (member, work) in activations {
        insert_member_for_root(
            conn,
            project,
            "work_units",
            work,
            "work_unit_activations",
            member,
        )?;
    }
    let work_events=conn.prepare("select e.id,e.work_unit_id from work_unit_events e join work_units w on w.id=e.work_unit_id where w.project_id=?1 and e.event_type!='closed' order by e.id")?.query_map(params![project],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (member, work) in work_events {
        insert_member_for_root(
            conn,
            project,
            "work_units",
            work,
            "work_unit_events",
            member,
        )?;
    }
    let validation_runs = conn
        .prepare(
            "select id,validation_gate_id from validation_runs where project_id=?1 order by id",
        )?
        .query_map(params![project], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (member, gate) in validation_runs {
        insert_member_for_root(
            conn,
            project,
            "validation_gates",
            gate,
            "validation_runs",
            member,
        )?;
    }
    let finding_acceptance=conn.prepare("select id,finding_id,approved_by_authority_event_id from acceptance_records where project_id=?1 and target_type='finding' order by id")?.query_map(params![project],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,Option<i64>>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (member, finding, authority) in finding_acceptance {
        insert_member_for_root(
            conn,
            project,
            "findings",
            finding,
            "acceptance_records",
            member,
        )?;
        if let Some(authority) = authority {
            insert_member_for_root(
                conn,
                project,
                "findings",
                finding,
                "authority_events",
                authority,
            )?;
        }
    }
    let plan_acceptance=conn.prepare("select id,review_plan_id,approved_by_authority_event_id from acceptance_records where project_id=?1 and target_type='review_plan' order by id")?.query_map(params![project],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?,row.get::<_,Option<i64>>(2)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (member, plan, authority) in plan_acceptance {
        insert_member_for_root(
            conn,
            project,
            "review_plans",
            plan,
            "acceptance_records",
            member,
        )?;
        if let Some(authority) = authority {
            insert_member_for_root(
                conn,
                project,
                "review_plans",
                plan,
                "authority_events",
                authority,
            )?;
        }
    }

    validate_candidate_membership(conn, project)?;
    refresh_candidate_base_digests(conn, project)?;

    let plan_runs=conn.prepare("select p.id,r.id from review_plans p join review_runs r on r.review_plan_id=p.id where p.project_id=?1 order by p.id,r.id")?.query_map(params![project],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (plan, run) in plan_runs {
        insert_edge_for_members(
            conn,
            project,
            "plan_has_run",
            "review_plans",
            plan,
            "review_runs",
            run,
        )?;
    }
    let run_findings = conn
        .prepare(
            "select review_run_id,id from findings where project_id=?1 order by review_run_id,id",
        )?
        .query_map(params![project], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (run, finding) in run_findings {
        insert_edge_for_members(
            conn,
            project,
            "run_reports_finding",
            "review_runs",
            run,
            "findings",
            finding,
        )?;
    }
    let verification=conn.prepare("select finding_id,review_run_id from finding_verifications where project_id=?1 order by finding_id,review_run_id")?.query_map(params![project],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (finding, run) in verification {
        insert_edge_for_members(
            conn,
            project,
            "finding_has_verification",
            "findings",
            finding,
            "review_runs",
            run,
        )?;
    }
    let work_plans = conn
        .prepare(
            "select work_unit_id,id from review_plans where project_id=?1 order by work_unit_id,id",
        )?
        .query_map(params![project], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (work, plan) in work_plans {
        insert_edge_for_members(
            conn,
            project,
            "work_depends_on",
            "work_units",
            work,
            "review_plans",
            plan,
        )?;
    }
    let work_validation_gates=conn.prepare("select coalesce(g.work_unit_id,t.work_unit_id),g.id from validation_gates g left join tasks t on t.id=g.task_id where g.project_id=?1 and coalesce(g.work_unit_id,t.work_unit_id) is not null order by 1,g.id")?.query_map(params![project],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,i64>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (work, gate) in work_validation_gates {
        insert_edge_for_members(
            conn,
            project,
            "work_depends_on",
            "work_units",
            work,
            "validation_gates",
            gate,
        )?;
    }
    // Schema 11 did not persist an exact boundary-to-review evidence link.
    // Current mutable plan status and timestamps are therefore insufficient to
    // synthesize boundary_consumes. Those boundaries remain grandfathered
    // audit history but are deliberately not bootstrap eligible.

    finalize_candidate_content_digests(conn, project)?;

    Ok(())
}

fn assign_boundary_coordinates(conn: &Connection, project: i64) -> Result<()> {
    conn.execute("update legacy_migration_candidates set boundary_generation=11,commit_sequence=(select unixepoch(e.created_at) from legacy_migration_candidate_members m join work_unit_events e on m.source_table='work_unit_events' and m.source_row_id=e.id where m.project_id=?1 and m.candidate_id=legacy_migration_candidates.id) where project_id=?1 and candidate_kind='completed_boundary' and exists(select 1 from legacy_migration_candidate_members m where m.project_id=?1 and m.candidate_id=legacy_migration_candidates.id and m.source_table='work_unit_events')",params![project])?;
    conn.execute("update legacy_migration_candidates set boundary_generation=11,commit_sequence=(select unixepoch(v.approved_at) from legacy_migration_candidate_members m join design_versions v on m.source_table='design_versions' and m.source_row_id=v.id where m.project_id=?1 and m.candidate_id=legacy_migration_candidates.id) where project_id=?1 and candidate_kind='completed_boundary' and exists(select 1 from legacy_migration_candidate_members m where m.project_id=?1 and m.candidate_id=legacy_migration_candidates.id and m.source_table='design_versions')",params![project])?;
    let missing: i64 = conn.query_row(
        "select count(*) from legacy_migration_candidates where project_id=?1 and candidate_kind='completed_boundary' and (boundary_generation is null or commit_sequence is null)",
        params![project],
        |row| row.get(0),
    )?;
    if missing != 0 {
        bail!("migration ambiguity: completed_boundary_without_sequence");
    }
    Ok(())
}

pub(super) fn record_candidate_projections(conn: &Connection, project: i64) -> Result<()> {
    let candidates=conn.prepare("select id,candidate_kind from legacy_migration_candidates where project_id=?1 order by candidate_kind,candidate_handle")?.query_map(params![project],|row|Ok((row.get::<_,i64>(0)?,row.get::<_,String>(1)?)))?.collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, kind) in candidates {
        conn.execute("insert into legacy_migration_projections(project_id,candidate_id,stratum,mapping_row,before_lifecycle,after_lifecycle,created_at) values(?1,?2,1,'candidate_base_fact','source','immutable',current_timestamp)",params![project,id])?;
        let (stratum, mapping, before, after) = match kind.as_str() {
            "invocation" => (
                1,
                "invocation_base".to_owned(),
                "legacy".to_owned(),
                "preserved".to_owned(),
            ),
            "completed_run" => {
                let (clean, findings):(i64,i64)=conn.query_row("select r.clean_run,(select count(*) from legacy_migration_edges e where e.project_id=?1 and e.source_candidate_id=?2 and e.edge_kind='run_reports_finding') from review_runs r join legacy_migration_candidate_members m on m.source_table='review_runs' and m.source_row_id=r.id where m.project_id=?1 and m.candidate_id=?2",params![project,id],|row|Ok((row.get(0)?,row.get(1)?)))?;
                let claim = if clean == 1 {
                    "clean"
                } else if findings > 0 {
                    "findings"
                } else {
                    "inconclusive"
                };
                let (resolution,consumed):(String,i64)=conn.query_row("select a.reviewer_resolution,exists(select 1 from legacy_migration_edges e where e.project_id=?1 and e.target_candidate_id=?2 and e.edge_kind='boundary_consumes') from legacy_claim_audits a join legacy_migration_candidate_members m on m.source_table='review_runs' and m.source_row_id=a.review_run_id where m.project_id=?1 and m.candidate_id=?2",params![project,id],|row|Ok((row.get(0)?,row.get(1)?)))?;
                let after = if resolution != "trusted" {
                    "audit_only"
                } else if consumed == 1 {
                    "grandfathered_consumed"
                } else {
                    "pending_adjudication"
                };
                (
                    2,
                    format!(
                        "completed_claim:{claim}:inventory={findings}:reviewer={resolution}:consumed={consumed}"
                    ),
                    "completed".to_owned(),
                    after.to_owned(),
                )
            }
            "finding_epoch" => {
                let lifecycle:String=conn.query_row("select f.lifecycle_state from findings f join legacy_migration_candidate_members m on m.source_table='findings' and m.source_row_id=f.id where m.project_id=?1 and m.candidate_id=?2",params![project,id],|row|row.get(0))?;
                let pending_verifications:i64=conn.query_row("select count(*) from finding_verifications v join legacy_migration_candidate_members m on m.source_table='findings' and m.source_row_id=v.finding_id left join closure_attempts a on a.id=v.closure_attempt_id where m.project_id=?1 and m.candidate_id=?2 and (v.closure_attempt_id is null or a.result is null)",params![project,id],|row|row.get(0))?;
                (
                    3,
                    format!(
                        "finding_lifecycle:{lifecycle}:pending_verifications={pending_verifications}"
                    ),
                    lifecycle.clone(),
                    lifecycle,
                )
            }
            "plan_gate" => {
                let member:(String,i64)=conn.query_row("select source_table,source_row_id from legacy_migration_candidate_members where project_id=?1 and candidate_id=?2 and source_table in ('review_plans','validation_gates','review_policies') order by source_table limit 1",params![project,id],|row|Ok((row.get(0)?,row.get(1)?)))?;
                let status: String = if member.0 == "review_plans" {
                    conn.query_row(
                        "select status from review_plans where id=?1",
                        params![member.1],
                        |row| row.get(0),
                    )?
                } else if member.0 == "validation_gates" {
                    conn.query_row(
                        "select status from validation_gates where id=?1",
                        params![member.1],
                        |row| row.get(0),
                    )?
                } else {
                    "policy_epoch".to_owned()
                };
                let claims:i64=conn.query_row("select count(*) from legacy_migration_edges e join legacy_migration_candidates target on target.id=e.target_candidate_id join legacy_migration_candidate_members m on m.candidate_id=target.id and m.source_table='review_runs' join review_runs r on r.id=m.source_row_id join review_plans p on p.id=r.review_plan_id join legacy_claim_audits audit on audit.review_run_id=r.id where e.project_id=?1 and e.source_candidate_id=?2 and e.edge_kind='plan_has_run' and target.candidate_kind='completed_run' and r.id>coalesce(p.fresh_review_after_run_id,0) and r.run_type='fresh' and r.run_purpose='new_unbiased_review' and audit.reviewer_resolution='trusted' and exists(select 1 from review_agent_invocations i where i.review_run_id=r.id and i.target_context=r.target_ref and i.purpose='new_unbiased_review') and not exists(select 1 from legacy_migration_edges consumed where consumed.project_id=e.project_id and consumed.target_candidate_id=target.id and consumed.edge_kind='boundary_consumes')",params![project,id],|row|row.get(0))?;
                let in_flight:i64=conn.query_row("select count(*) from legacy_migration_edges e join legacy_migration_candidates target on target.id=e.target_candidate_id join legacy_migration_candidate_members m on m.candidate_id=target.id and m.source_table='review_runs' join review_runs r on r.id=m.source_row_id where e.project_id=?1 and e.source_candidate_id=?2 and e.edge_kind='plan_has_run' and target.candidate_kind='invocation' and r.status in ('requested','running')",params![project,id],|row|row.get(0))?;
                let terminal_no_claim: i64 = if member.0 == "review_plans" {
                    conn.query_row("select count(*) from review_runs where project_id=?1 and review_plan_id=?2 and status in ('failed','cancelled')",params![project,member.1],|row|row.get(0))?
                } else {
                    0
                };
                if member.0 == "review_plans" {
                    let limit:i64=conn.query_row("select pol.max_parallel_agents from review_plans p join review_policies pol on pol.id=p.review_policy_id where p.project_id=?1 and p.id=?2",params![project,member.1],|row|row.get(0))?;
                    if in_flight > limit {
                        bail!("migration ambiguity: invocation_concurrency_exceeded");
                    }
                    let duplicate_slot:i64=conn.query_row("select exists(select 1 from review_agent_invocations i join review_runs r on r.id=i.review_run_id where i.project_id=?1 and r.review_plan_id=?2 and r.status in ('requested','running') group by coalesce(cast(i.reviewer_principal_id as text),i.external_agent_id,'invocation:'||i.id) having count(*)>1)",params![project,member.1],|row|row.get(0))?;
                    if duplicate_slot == 1 {
                        bail!("migration ambiguity: duplicate_invocation_slot");
                    }
                }
                let selected_outcome = if member.0 == "validation_gates" {
                    conn.query_row("select coalesce((select result from validation_runs where project_id=?1 and validation_gate_id=?2 order by id desc limit 1),'unmet')",params![project,member.1],|row|row.get::<_,String>(0))?
                } else {
                    "none".to_owned()
                };
                let accepted_failure: i64 = if member.0 == "validation_gates" {
                    conn.query_row(
                        "select exists(
                            select 1
                            from validation_runs vr
                            join acceptance_records ar on ar.id=vr.acceptance_record_id
                            where vr.project_id=?1
                              and vr.validation_gate_id=?2
                              and vr.id=(select id from validation_runs where project_id=?1 and validation_gate_id=?2 order by id desc limit 1)
                              and ar.status='approved'
                              and ar.acceptance_type in ('classified_failure','evidence_gap','explicit_exception')
                            union all
                            select 1
                            from validation_gates vg
                            join validation_runs vr on vr.validation_gate_id=vg.id
                            join acceptance_records ar on ar.validation_gate_template_id=vg.template_id
                            where vg.project_id=?1
                              and vg.id=?2
                              and vr.id=(select id from validation_runs where project_id=?1 and validation_gate_id=?2 order by id desc limit 1)
                              and ar.project_id=vg.project_id
                              and ar.target_type='validation_gate_template'
                              and ar.status='approved'
                              and ar.acceptance_type in ('classified_failure','evidence_gap','explicit_exception')
                        )",
                        params![project, member.1],
                        |row| row.get(0),
                    )?
                } else {
                    0
                };
                let consumed:i64=conn.query_row("select exists(select 1 from legacy_migration_edges where project_id=?1 and target_candidate_id=?2 and edge_kind='boundary_consumes')",params![project,id],|row|row.get(0))?;
                let waived = if member.0 == "review_plans" {
                    conn.query_row("select exists(select 1 from acceptance_records where project_id=?1 and review_plan_id=?2 and status='approved')",params![project,member.1],|row|row.get::<_,i64>(0))?
                } else {
                    0
                };
                let reduction = if waived == 1 {
                    "waived"
                } else if consumed == 1 {
                    "consumed_boundary"
                } else if selected_outcome != "none" && selected_outcome != "unmet" {
                    "selected_recorded_gate"
                } else if claims > 0 {
                    "current_claim_set"
                } else if in_flight > 0 {
                    "in_flight_set"
                } else if terminal_no_claim > 0 {
                    "terminal_no_claim"
                } else if status == "policy_epoch" {
                    "policy_epoch"
                } else {
                    "unmet"
                };
                let after = if member.0 == "validation_gates" {
                    if selected_outcome == "pass" {
                        "gate_satisfied".to_owned()
                    } else if selected_outcome == "unmet" {
                        "unmet".to_owned()
                    } else if accepted_failure == 1 {
                        "accepted_exception".to_owned()
                    } else {
                        "rerun_required".to_owned()
                    }
                } else {
                    status.clone()
                };
                (
                    4,
                    format!(
                        "plan_gate_reduction:{reduction}:status={status}:selected_outcome={selected_outcome}:accepted_failure={accepted_failure}:claims={claims}:in_flight={in_flight}:terminal_no_claim={terminal_no_claim}"
                    ),
                    status.clone(),
                    after,
                )
            }
            "completed_boundary" => {
                let consumed:i64=conn.query_row("select count(*) from legacy_migration_edges where project_id=?1 and source_candidate_id=?2 and edge_kind='boundary_consumes'",params![project,id],|row|row.get(0))?;
                (
                    5,
                    format!("completed_boundary_snapshot:consumed={consumed}"),
                    "completed".to_owned(),
                    "grandfathered".to_owned(),
                )
            }
            "work_owner" => {
                let status:String=conn.query_row("select w.status from work_units w join legacy_migration_candidate_members m on m.source_table='work_units' and m.source_row_id=w.id where m.project_id=?1 and m.candidate_id=?2",params![project,id],|row|row.get(0))?;
                let work:i64=conn.query_row("select source_row_id from legacy_migration_candidate_members where project_id=?1 and candidate_id=?2 and source_table='work_units'",params![project,id],|row|row.get(0))?;
                let dependencies:i64=conn.query_row("select count(*) from legacy_migration_edges where project_id=?1 and source_candidate_id=?2 and edge_kind='work_depends_on'",params![project,id],|row|row.get(0))?;
                let blockers:i64=conn.query_row("select count(*) from legacy_migration_edges e join legacy_migration_projections p on p.candidate_id=e.target_candidate_id where e.project_id=?1 and e.source_candidate_id=?2 and e.edge_kind='work_depends_on' and p.after_lifecycle in ('pending_adjudication','open','remediating','awaiting_verification','rerun_required','unmet')",params![project,id],|row|row.get(0))?;
                let pending_review:Option<i64>=conn.query_row("select r.id from review_plans p join review_runs r on r.review_plan_id=p.id join legacy_claim_audits a on a.review_run_id=r.id where p.project_id=?1 and p.work_unit_id=?2 and r.run_type='fresh' and r.run_purpose='new_unbiased_review' and a.reviewer_resolution='trusted' and not exists(select 1 from review_adjudication_decisions d where d.review_run_id=r.id) order by r.id limit 1",params![project,work],|row|row.get(0)).optional()?;
                let pending_verification:Option<(i64,i64,i64,i64)>=conn.query_row("select v.review_run_id,c.finding_id,c.id,a.id from closure_attempts a join closures c on c.id=a.closure_id join findings f on f.id=c.finding_id join review_runs source on source.id=f.review_run_id join review_plans p on p.id=source.review_plan_id join finding_verifications v on v.closure_attempt_id=a.id where a.project_id=?1 and p.work_unit_id=?2 and a.result is null and not exists(select 1 from verification_adjudication_decisions d where d.closure_attempt_id=a.id) order by a.id limit 1",params![project,work],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?;
                let pending_finding:Option<i64>=conn.query_row("select f.id from findings f join review_runs r on r.id=f.review_run_id join review_plans p on p.id=r.review_plan_id where f.project_id=?1 and p.work_unit_id=?2 and f.lifecycle_state='open' and not exists(select 1 from legacy_claim_audits a where a.review_run_id=r.id and a.reviewer_resolution in ('unbound','ambiguous')) order by f.id limit 1",params![project,work],|row|row.get(0)).optional()?;
                let failed_gate:Option<i64>=conn.query_row("select g.id from validation_gates g left join tasks t on t.id=g.task_id where g.project_id=?1 and coalesce(g.work_unit_id,t.work_unit_id)=?2 and coalesce((select result from validation_runs r where r.validation_gate_id=g.id order by r.id desc limit 1),'unmet')!='pass' and not exists(select 1 from validation_runs vr join acceptance_records ar on ar.id=vr.acceptance_record_id where vr.project_id=g.project_id and vr.validation_gate_id=g.id and vr.id=(select id from validation_runs where project_id=g.project_id and validation_gate_id=g.id order by id desc limit 1) and ar.status='approved' and ar.acceptance_type in ('classified_failure','evidence_gap','explicit_exception')) and not exists(select 1 from acceptance_records ar where ar.project_id=g.project_id and ar.target_type='validation_gate_template' and ar.validation_gate_template_id=g.template_id and ar.status='approved' and ar.acceptance_type in ('classified_failure','evidence_gap','explicit_exception') and exists(select 1 from validation_runs vr where vr.project_id=g.project_id and vr.validation_gate_id=g.id)) order by g.id limit 1",params![project,work],|row|row.get(0)).optional()?;
                let unmet_plan:Option<i64>=conn.query_row("select id from review_plans where project_id=?1 and work_unit_id=?2 and required=1 and status in ('open','blocked') order by id limit 1",params![project,work],|row|row.get(0)).optional()?;
                let next = if matches!(status.as_str(), "closed" | "abandoned") {
                    "terminal".to_owned()
                } else if let Some((run, finding, closure, attempt)) = pending_verification {
                    format!(
                        "agent-workbench verification adjudicate --run {run} --finding {finding} --closure {closure} --attempt {attempt} --help"
                    )
                } else if let Some(run) = pending_review {
                    format!("agent-workbench review adjudicate {run} --help")
                } else if let Some(finding) = pending_finding {
                    format!("agent-workbench finding decide {finding} --help")
                } else if let Some(gate) = failed_gate {
                    format!("agent-workbench gate record --gate {gate} --help")
                } else if let Some(plan) = unmet_plan {
                    format!("agent-workbench review invocation request --plan {plan} --help")
                } else {
                    "agent-workbench next".to_owned()
                };
                (
                    6,
                    format!(
                        "owner_reduction:{status}:dependencies={dependencies}:blockers={blockers}:next={next}"
                    ),
                    status.clone(),
                    status,
                )
            }
            _ => bail!("unmatched_mapping"),
        };
        if stratum != 1 {
            conn.execute("insert into legacy_migration_projections(project_id,candidate_id,stratum,mapping_row,before_lifecycle,after_lifecycle,created_at) values(?1,?2,?3,?4,?5,?6,current_timestamp)",params![project,id,stratum,mapping,before,after])?;
        }
    }
    Ok(())
}

fn insert_query_candidates(
    conn: &Connection,
    project: i64,
    table: &str,
    kind: &str,
    query: &str,
) -> Result<()> {
    let rows = conn
        .prepare(query)?
        .query_map(params![project], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, state) in rows {
        insert_candidate(conn, project, table, id, kind, &state)?;
    }
    Ok(())
}

fn insert_candidate(
    conn: &Connection,
    project: i64,
    table: &str,
    row_id: i64,
    kind: &str,
    state: &str,
) -> Result<()> {
    let base = domain_digest(
        b"agent-workbench:legacy-candidate-base-v1\0",
        &CanonicalValue::object([
            ("kind", CanonicalValue::string(kind)),
            ("table", CanonicalValue::string(table)),
            ("row", CanonicalValue::Integer(row_id)),
        ]),
    );
    let content = domain_digest(
        b"agent-workbench:legacy-candidate-content-v1\0",
        &CanonicalValue::object([
            ("base", CanonicalValue::string(&base)),
            ("state", CanonicalValue::string(state)),
        ]),
    );
    let handle = format!("legacy_candidate_{content}");
    conn.execute("insert into legacy_migration_candidates(project_id,candidate_kind,candidate_handle,base_digest,content_digest,created_at) values(?1,?2,?3,?4,?5,current_timestamp)",params![project,kind,handle,base,content])?;
    let candidate = conn.last_insert_rowid();
    let member = source_member_digest(conn, table, row_id)?;
    conn.execute("insert into legacy_migration_candidate_members(project_id,candidate_id,source_table,source_row_id,member_digest,created_at) values(?1,?2,?3,?4,?5,current_timestamp)",params![project,candidate,table,row_id,member])?;
    Ok(())
}

fn insert_member_for_root(
    conn: &Connection,
    project: i64,
    root_table: &str,
    root_row: i64,
    member_table: &str,
    member_row: i64,
) -> Result<()> {
    let candidate = find_candidate(conn, project, root_table, root_row)?;
    let existing: Option<i64> = conn
        .query_row(
            "select candidate_id from legacy_migration_candidate_members where project_id=?1 and source_table=?2 and source_row_id=?3",
            params![project, member_table, member_row],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(existing) = existing {
        if existing != candidate.0 {
            bail!("migration ambiguity: ambiguous_candidate_membership");
        }
        return Ok(());
    }
    let digest = source_member_digest(conn, member_table, member_row)?;
    conn.execute("insert into legacy_migration_candidate_members(project_id,candidate_id,source_table,source_row_id,member_digest,created_at) values(?1,?2,?3,?4,?5,current_timestamp)",params![project,candidate.0,member_table,member_row,digest])?;
    Ok(())
}

fn source_member_digest(conn: &Connection, table: &str, row_id: i64) -> Result<String> {
    let mut statement = conn.prepare(&format!("select * from {table} where id=?1"))?;
    let column_count = statement.column_count();
    let values = statement.query_row(params![row_id], |row| {
        let mut encoded = Vec::with_capacity(column_count);
        for index in 0..column_count {
            let value = match row.get_ref(index)? {
                ValueRef::Null => "n:".to_owned(),
                ValueRef::Integer(value) => format!("i:{value}"),
                ValueRef::Real(value) => format!("r:{:016x}", value.to_bits()),
                ValueRef::Text(value) => format!("t:{}", encode_hex(value)),
                ValueRef::Blob(value) => format!("b:{}", encode_hex(value)),
            };
            encoded.push(CanonicalValue::String(value));
        }
        Ok(encoded)
    })?;
    Ok(domain_digest(
        b"agent-workbench:legacy-candidate-member-v2\0",
        &CanonicalValue::object([
            ("table", CanonicalValue::string(table)),
            ("row", CanonicalValue::Integer(row_id)),
            ("values", CanonicalValue::Array(values)),
        ]),
    ))
}

fn validate_candidate_membership(conn: &Connection, project: i64) -> Result<()> {
    for (table, source) in [
        (
            "work_units",
            "select id from work_units where project_id=?1",
        ),
        (
            "work_unit_activations",
            "select id from work_unit_activations where project_id=?1",
        ),
        (
            "work_unit_events",
            "select e.id from work_unit_events e join work_units w on w.id=e.work_unit_id where w.project_id=?1",
        ),
        (
            "review_plans",
            "select id from review_plans where project_id=?1",
        ),
        (
            "review_runs",
            "select id from review_runs where project_id=?1",
        ),
        (
            "review_agent_invocations",
            "select id from review_agent_invocations where project_id=?1",
        ),
        (
            "review_plan_targets",
            "select t.id from review_plan_targets t join review_plans p on p.id=t.review_plan_id where p.project_id=?1",
        ),
        ("findings", "select id from findings where project_id=?1"),
        ("closures", "select id from closures where project_id=?1"),
        (
            "closure_attempts",
            "select id from closure_attempts where project_id=?1",
        ),
        (
            "finding_verifications",
            "select id from finding_verifications where project_id=?1",
        ),
        (
            "validation_gates",
            "select id from validation_gates where project_id=?1",
        ),
        (
            "validation_runs",
            "select id from validation_runs where project_id=?1",
        ),
        (
            "review_policies",
            "select id from review_policies where project_id=?1",
        ),
        (
            "design_versions",
            "select id from design_versions where project_id=?1 and status='approved'",
        ),
        (
            "acceptance_records",
            "select id from acceptance_records where project_id=?1 and target_type in ('finding','review_plan')",
        ),
        (
            "authority_events",
            "select distinct approved_by_authority_event_id id from acceptance_records where project_id=?1 and target_type in ('finding','review_plan') and approved_by_authority_event_id is not null",
        ),
    ] {
        let query = format!(
            "select count(*) from ({source}) source where not exists(select 1 from legacy_migration_candidate_members m where m.project_id=?1 and m.source_table='{table}' and m.source_row_id=source.id)"
        );
        let missing: i64 = conn.query_row(&query, params![project], |row| row.get(0))?;
        if missing != 0 {
            bail!("migration ambiguity: ambiguous_candidate_membership");
        }
    }
    Ok(())
}

fn insert_edge_for_members(
    conn: &Connection,
    project: i64,
    kind: &str,
    source_table: &str,
    source_row: i64,
    target_table: &str,
    target_row: i64,
) -> Result<()> {
    let source: candidate::Id = find_candidate(conn, project, source_table, source_row)?;
    let target: candidate::Id = find_candidate(conn, project, target_table, target_row)?;
    let source_base: String = conn.query_row(
        "select base_digest from legacy_migration_candidates where id=?1",
        params![source.0],
        |row| row.get(0),
    )?;
    let target_base: String = conn.query_row(
        "select base_digest from legacy_migration_candidates where id=?1",
        params![target.0],
        |row| row.get(0),
    )?;
    let digest = domain_digest(
        b"agent-workbench:legacy-candidate-edge-v1\0",
        &CanonicalValue::object([
            ("kind", CanonicalValue::string(kind)),
            ("source", CanonicalValue::string(&source_base)),
            ("target", CanonicalValue::string(&target_base)),
        ]),
    );
    conn.execute("insert into legacy_migration_edges(project_id,edge_kind,source_candidate_id,target_candidate_id,edge_digest,created_at) values(?1,?2,?3,?4,?5,current_timestamp)",params![project,kind,source.0,target.0,digest])?;
    Ok(())
}

fn refresh_candidate_base_digests(conn: &Connection, project: i64) -> Result<()> {
    let candidates = conn
        .prepare("select id,candidate_kind from legacy_migration_candidates where project_id=?1 order by id")?
        .query_map(params![project], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, kind) in candidates {
        let members = conn
            .prepare("select member_digest from legacy_migration_candidate_members where project_id=?1 and candidate_id=?2 order by member_digest")?
            .query_map(params![project, id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let base = domain_digest(
            b"agent-workbench:legacy-candidate-base-v2\0",
            &CanonicalValue::object([
                ("kind", CanonicalValue::string(&kind)),
                (
                    "members",
                    CanonicalValue::Array(
                        members.into_iter().map(CanonicalValue::String).collect(),
                    ),
                ),
            ]),
        );
        conn.execute(
            "update legacy_migration_candidates set base_digest=?1 where project_id=?2 and id=?3",
            params![base, project, id],
        )?;
    }
    let duplicate: Option<String> = conn.query_row(
        "select base_digest from legacy_migration_candidates where project_id=?1 group by base_digest having count(*)>1 limit 1",
        params![project],
        |row| row.get(0),
    ).optional()?;
    if duplicate.is_some() {
        bail!("migration ambiguity: duplicate_candidate_identity");
    }
    Ok(())
}

fn finalize_candidate_content_digests(conn: &Connection, project: i64) -> Result<()> {
    let candidates = conn
        .prepare("select id,base_digest from legacy_migration_candidates where project_id=?1 order by base_digest")?
        .query_map(params![project], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (id, base) in candidates {
        let edges = conn
            .prepare("select edge_digest from legacy_migration_edges where project_id=?1 and (source_candidate_id=?2 or target_candidate_id=?2) order by edge_digest")?
            .query_map(params![project, id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let content = domain_digest(
            b"agent-workbench:legacy-candidate-content-v2\0",
            &CanonicalValue::object([
                ("base", CanonicalValue::string(&base)),
                (
                    "edges",
                    CanonicalValue::Array(edges.into_iter().map(CanonicalValue::String).collect()),
                ),
            ]),
        );
        let handle = format!("legacy_candidate_{content}");
        conn.execute("update legacy_migration_candidates set candidate_handle=?1,content_digest=?2 where project_id=?3 and id=?4", params![handle, content, project, id])?;
    }
    Ok(())
}

mod candidate {
    pub(super) struct Id(pub(super) i64);
}
fn find_candidate(conn: &Connection, project: i64, table: &str, row: i64) -> Result<candidate::Id> {
    conn.query_row("select candidate_id from legacy_migration_candidate_members where project_id=?1 and source_table=?2 and source_row_id=?3",params![project,table,row],|r|r.get(0)).optional()?.map(candidate::Id).context("typed migration edge target is missing")
}
