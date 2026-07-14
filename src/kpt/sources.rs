use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use super::*;

pub(super) fn convertible_kpt_item(
    conn: &rusqlite::Connection,
    kpt_item_id: i64,
) -> Result<StoredKptItem> {
    conn.query_row(
        r#"
        select id, title, details, proposed_action
        from kpt_items
        where id = ?1 and status in ('open', 'accepted')
        "#,
        params![kpt_item_id],
        |row| {
            Ok(StoredKptItem {
                id: row.get(0)?,
                title: row.get(1)?,
                details: row.get(2)?,
                proposed_action: row.get(3)?,
            })
        },
    )
    .optional()?
    .context("kpt item not found or not convertible")
}

pub(super) fn ensure_fixed_command_authority(
    conn: &rusqlite::Connection,
    project_id: i64,
    authority_event_id: i64,
) -> Result<()> {
    let allowed: bool = conn.query_row(
        r#"
        select exists (
            select 1
            from authority_events
            where id = ?1
              and project_id = ?2
              and status = 'active'
              and event_type in ('user_instruction', 'policy')
        )
        "#,
        params![authority_event_id, project_id],
        |row| row.get(0),
    )?;
    if !allowed {
        bail!("fixed command conversion requires active user or policy authority");
    }
    Ok(())
}

pub(super) fn latest_open_kpt_review(conn: &rusqlite::Connection) -> Result<i64> {
    conn.query_row(
        "select id from kpt_reviews where status = 'open' order by id desc limit 1",
        [],
        |row| row.get::<_, i64>(0),
    )
    .optional()?
    .context("no open kpt review; run kpt start first")
}

pub(super) fn parse_kpt_sources(input: Option<&str>) -> Result<Vec<KptSource>> {
    let Some(input) = input else {
        return Ok(Vec::new());
    };
    let mut sources = Vec::new();
    for raw in input.split(',') {
        let source = raw.trim();
        if source.is_empty() {
            continue;
        }
        let parsed = match source {
            "corrections" => KptSource::Corrections,
            "findings" => KptSource::Findings,
            "commands" | "command-drift" => KptSource::Commands,
            "reviews" | "review-runs" | "review-outcomes" => KptSource::ReviewRuns,
            "work-records" | "work-units" | "work-unit-outcomes" | "outcomes" => {
                KptSource::WorkRecords
            }
            _ => bail!("unsupported kpt source: {source}"),
        };
        if !sources.contains(&parsed) {
            sources.push(parsed);
        }
    }
    Ok(sources)
}

pub(super) fn import_findings_as_kpt_items(
    conn: &Connection,
    kpt_review_id: i64,
    project_id: i64,
    period_modifier: Option<&str>,
) -> Result<i64> {
    let sql = match period_modifier {
        Some(_) => {
            r#"
            select id, finding_type, severity, description, classification, status
            from findings
            where project_id = ?1
              and status = 'open'
              and created_at >= datetime('now', ?2)
            order by severity, id
            "#
        }
        None => {
            r#"
            select id, finding_type, severity, description, classification, status
            from findings
            where project_id = ?1
              and status = 'open'
            order by severity, id
            "#
        }
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = match period_modifier {
        Some(period_modifier) => {
            stmt.query_map(params![project_id, period_modifier], stored_finding_record)?
        }
        None => stmt.query_map(params![project_id], stored_finding_record)?,
    };
    let mut count = 0;
    for row in rows {
        let finding = row?;
        let title = format!("Review finding: {}", finding.description);
        let details = format!(
            "type: {}\nclassification: {}\nstatus: {}",
            finding.finding_type, finding.classification, finding.status
        );
        conn.execute(
            r#"
            insert into kpt_items(
                kpt_review_id, item_type, title, details, severity,
                linked_review_finding_id, proposed_action, status, created_at
            )
            values (?1, 'problem', ?2, ?3, ?4, ?5, ?6, 'open', current_timestamp)
            "#,
            params![
                kpt_review_id,
                title,
                details,
                finding.severity,
                finding.id,
                "classify, close, or convert this review finding",
            ],
        )?;
        count += 1;
    }
    Ok(count)
}

pub(super) fn stored_finding_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredFinding> {
    Ok(StoredFinding {
        id: row.get(0)?,
        finding_type: row.get(1)?,
        severity: row.get(2)?,
        description: row.get(3)?,
        classification: row.get(4)?,
        status: row.get(5)?,
    })
}

pub(super) fn import_corrections_as_kpt_items(
    conn: &Connection,
    kpt_review_id: i64,
    project_id: i64,
    scope: Option<&str>,
    period_modifier: Option<&str>,
) -> Result<i64> {
    let corrections = corrections_for_kpt(conn, project_id, scope, period_modifier)?;
    for correction in &corrections {
        let title = format!("Repeated correction: {}", correction.mistake_pattern);
        let details = format!(
            "scope: {}\ntype: {}\ncorrection: {}",
            correction.scope, correction.correction_type, correction.correction
        );
        conn.execute(
            r#"
            insert into kpt_items(
                kpt_review_id, item_type, title, details, severity,
                linked_user_correction_id, proposed_action, status, created_at
            )
            values (?1, 'problem', ?2, ?3, ?4, ?5, ?6, 'open', current_timestamp)
            "#,
            params![
                kpt_review_id,
                title,
                details,
                correction.severity,
                correction.id,
                correction.correction,
            ],
        )?;
    }

    Ok(corrections.len() as i64)
}

pub(super) fn import_commands_as_kpt_items(
    conn: &Connection,
    kpt_review_id: i64,
    project_id: i64,
    period_modifier: Option<&str>,
) -> Result<i64> {
    let mut records = Vec::new();
    match period_modifier {
        Some(period_modifier) => {
            let mut stmt = conn.prepare(
                r#"
                select d.id, d.command_profile_id, p.name, d.reason, d.status
                from command_deviations d
                join command_profiles p on p.id = d.command_profile_id
                where p.project_id = ?1 and d.created_at >= datetime('now', ?2)
                order by d.id
                "#,
            )?;
            let rows =
                stmt.query_map(params![project_id, period_modifier], command_drift_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        None => {
            let mut stmt = conn.prepare(
                r#"
                select d.id, d.command_profile_id, p.name, d.reason, d.status
                from command_deviations d
                join command_profiles p on p.id = d.command_profile_id
                where p.project_id = ?1
                order by d.id
                "#,
            )?;
            let rows = stmt.query_map(params![project_id], command_drift_record)?;
            for row in rows {
                records.push(row?);
            }
        }
    }

    for record in &records {
        let title = format!("Command drift: {}", record.profile_name);
        let details = format!(
            "deviation_id: {}\nstatus: {}\nreason: {}",
            record.id, record.status, record.reason
        );
        conn.execute(
            r#"
            insert into kpt_items(
                kpt_review_id, item_type, title, details, severity,
                linked_command_profile_id, proposed_action, status, created_at
            )
            values (?1, 'problem', ?2, ?3, 'medium', ?4, ?5, 'open', current_timestamp)
            "#,
            params![
                kpt_review_id,
                title,
                details,
                record.command_profile_id,
                "decide whether to update, keep, or deprecate this command profile",
            ],
        )?;
    }

    Ok(records.len() as i64)
}

pub(super) fn import_review_runs_as_kpt_items(
    conn: &Connection,
    kpt_review_id: i64,
    project_id: i64,
    period_modifier: Option<&str>,
) -> Result<i64> {
    let sql = match period_modifier {
        Some(_) => {
            r#"
            select id, review_plan_id, run_type, run_purpose, status,
                   new_findings_count, carried_findings_checked, clean_run,
                   coalesce(result_summary, '')
            from review_runs
            where project_id = ?1
              and created_at >= datetime('now', ?2)
              and (status != 'completed' or clean_run = 0 or new_findings_count > 0)
            order by id
            "#
        }
        None => {
            r#"
            select id, review_plan_id, run_type, run_purpose, status,
                   new_findings_count, carried_findings_checked, clean_run,
                   coalesce(result_summary, '')
            from review_runs
            where project_id = ?1
              and (status != 'completed' or clean_run = 0 or new_findings_count > 0)
            order by id
            "#
        }
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = match period_modifier {
        Some(period_modifier) => {
            stmt.query_map(params![project_id, period_modifier], review_run_kpt_record)?
        }
        None => stmt.query_map(params![project_id], review_run_kpt_record)?,
    };
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    for record in &records {
        let title = format!("Review outcome: {} {}", record.run_type, record.id);
        let details = format!(
            "plan: {}\npurpose: {}\nstatus: {}\nnew_findings: {}\ncarried_checked: {}\nclean: {}\nsummary: {}",
            record
                .review_plan_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "-".to_string()),
            record.run_purpose,
            record.status,
            record.new_findings_count,
            record.carried_findings_checked,
            record.clean_run,
            record.result_summary,
        );
        conn.execute(
            r#"
            insert into kpt_items(
                kpt_review_id, item_type, title, details, severity,
                proposed_action, status, created_at
            )
            values (?1, 'problem', ?2, ?3, ?4, ?5, 'open', current_timestamp)
            "#,
            params![
                kpt_review_id,
                title,
                details,
                review_outcome_severity(record),
                "classify the review outcome and adjust tasks, policy, or design if needed",
            ],
        )?;
    }
    Ok(records.len() as i64)
}

pub(super) fn import_work_records_as_kpt_items(
    conn: &Connection,
    kpt_review_id: i64,
    project_id: i64,
    period_modifier: Option<&str>,
) -> Result<i64> {
    let sql = match period_modifier {
        Some(_) => {
            r#"
            select r.id, r.topic, coalesce(r.next_actions, ''), coalesce(r.notable_operations, '')
            from work_records r
            where r.project_id = ?1
              and r.created_at >= datetime('now', ?2)
              and (coalesce(r.next_actions, '') != '' or coalesce(r.notable_operations, '') != '')
            order by r.id
            "#
        }
        None => {
            r#"
            select r.id, r.topic, coalesce(r.next_actions, ''), coalesce(r.notable_operations, '')
            from work_records r
            where r.project_id = ?1
              and (coalesce(r.next_actions, '') != '' or coalesce(r.notable_operations, '') != '')
            order by r.id
            "#
        }
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = match period_modifier {
        Some(period_modifier) => {
            stmt.query_map(params![project_id, period_modifier], work_record_kpt_record)?
        }
        None => stmt.query_map(params![project_id], work_record_kpt_record)?,
    };
    let mut records = Vec::new();
    for row in rows {
        records.push(row?);
    }
    for record in &records {
        let details = format!(
            "work_record_id: {}\nnext_actions: {}\nnotable_operations: {}",
            record.id, record.next_actions, record.notable_operations
        );
        conn.execute(
            r#"
            insert into kpt_items(
                kpt_review_id, item_type, title, details, severity,
                proposed_action, status, created_at
            )
            values (?1, 'try', ?2, ?3, 'medium', ?4, 'open', current_timestamp)
            "#,
            params![
                kpt_review_id,
                format!("Work outcome: {}", record.topic),
                details,
                "convert unresolved next actions into tasks or decisions",
            ],
        )?;
    }
    Ok(records.len() as i64)
}

pub(super) fn corrections_for_kpt(
    conn: &Connection,
    project_id: i64,
    scope: Option<&str>,
    period_modifier: Option<&str>,
) -> Result<Vec<StoredCorrection>> {
    let mut records = Vec::new();
    match (scope, period_modifier) {
        (Some(scope), Some(period_modifier)) => {
            let mut stmt = conn.prepare(
                r#"
                select id, scope, correction_type, mistake_pattern, correction, severity
                from user_corrections
                where project_id = ?1
                  and status = 'active'
                  and scope = ?2
                  and created_at >= datetime('now', ?3)
                order by id
                "#,
            )?;
            let rows = stmt.query_map(
                params![project_id, scope, period_modifier],
                stored_correction_record,
            )?;
            for row in rows {
                records.push(row?);
            }
        }
        (Some(scope), None) => {
            let mut stmt = conn.prepare(
                r#"
                select id, scope, correction_type, mistake_pattern, correction, severity
                from user_corrections
                where project_id = ?1 and status = 'active' and scope = ?2
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![project_id, scope], stored_correction_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        (None, Some(period_modifier)) => {
            let mut stmt = conn.prepare(
                r#"
                select id, scope, correction_type, mistake_pattern, correction, severity
                from user_corrections
                where project_id = ?1
                  and status = 'active'
                  and created_at >= datetime('now', ?2)
                order by id
                "#,
            )?;
            let rows = stmt.query_map(
                params![project_id, period_modifier],
                stored_correction_record,
            )?;
            for row in rows {
                records.push(row?);
            }
        }
        (None, None) => {
            let mut stmt = conn.prepare(
                r#"
                select id, scope, correction_type, mistake_pattern, correction, severity
                from user_corrections
                where project_id = ?1 and status = 'active'
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![project_id], stored_correction_record)?;
            for row in rows {
                records.push(row?);
            }
        }
    }

    Ok(records)
}

pub(super) fn period_to_sqlite_modifier(period: &str) -> Result<String> {
    let period = period.trim();
    if let Some(days) = period.strip_suffix('d') {
        let days = days.parse::<u32>()?;
        if days == 0 {
            bail!("period must be greater than zero");
        }
        return Ok(format!("-{days} days"));
    }
    if let Some(hours) = period.strip_suffix('h') {
        let hours = hours.parse::<u32>()?;
        if hours == 0 {
            bail!("period must be greater than zero");
        }
        return Ok(format!("-{hours} hours"));
    }
    bail!("unsupported period; use values like 30d or 12h")
}

pub(super) fn bool_to_i64(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

pub(super) fn kpt_review_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<KptReviewRecord> {
    Ok(KptReviewRecord {
        id: row.get(0)?,
        scope: row.get(1)?,
        summary: row.get(2)?,
        status: row.get(3)?,
        created_at: row.get(4)?,
        closed_at: row.get(5)?,
    })
}

pub(super) fn kpt_item_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<KptItemRecord> {
    Ok(KptItemRecord {
        id: row.get(0)?,
        kpt_review_id: row.get(1)?,
        item_type: row.get(2)?,
        title: row.get(3)?,
        severity: row.get(4)?,
        status: row.get(5)?,
        linked_task_id: row.get(6)?,
    })
}

pub(super) fn stored_correction_record(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredCorrection> {
    Ok(StoredCorrection {
        id: row.get(0)?,
        scope: row.get(1)?,
        correction_type: row.get(2)?,
        mistake_pattern: row.get(3)?,
        correction: row.get(4)?,
        severity: row.get(5)?,
    })
}

pub(super) fn command_drift_record(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredCommandDrift> {
    Ok(StoredCommandDrift {
        id: row.get(0)?,
        command_profile_id: row.get(1)?,
        profile_name: row.get(2)?,
        reason: row.get(3)?,
        status: row.get(4)?,
    })
}

pub(super) fn review_run_kpt_record(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredReviewRunOutcome> {
    Ok(StoredReviewRunOutcome {
        id: row.get(0)?,
        review_plan_id: row.get(1)?,
        run_type: row.get(2)?,
        run_purpose: row.get(3)?,
        status: row.get(4)?,
        new_findings_count: row.get(5)?,
        carried_findings_checked: row.get(6)?,
        clean_run: row.get::<_, i64>(7)? == 1,
        result_summary: row.get(8)?,
    })
}

pub(super) fn work_record_kpt_record(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<StoredWorkRecordOutcome> {
    Ok(StoredWorkRecordOutcome {
        id: row.get(0)?,
        topic: row.get(1)?,
        next_actions: row.get(2)?,
        notable_operations: row.get(3)?,
    })
}

pub(super) fn review_outcome_severity(record: &StoredReviewRunOutcome) -> &'static str {
    if record.status == "failed" || record.new_findings_count > 0 {
        "high"
    } else if !record.clean_run {
        "medium"
    } else {
        "low"
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum KptSource {
    Corrections,
    Findings,
    Commands,
    ReviewRuns,
    WorkRecords,
}

pub(super) struct StoredKptItem {
    pub(super) id: i64,
    pub(super) title: String,
    pub(super) details: Option<String>,
    pub(super) proposed_action: Option<String>,
}

pub(super) struct StoredCorrection {
    id: i64,
    scope: String,
    correction_type: String,
    mistake_pattern: String,
    correction: String,
    severity: String,
}

pub(super) struct StoredFinding {
    id: i64,
    finding_type: String,
    severity: String,
    description: String,
    classification: String,
    status: String,
}

pub(super) struct StoredCommandDrift {
    id: i64,
    command_profile_id: i64,
    profile_name: String,
    reason: String,
    status: String,
}

pub(super) struct StoredReviewRunOutcome {
    id: i64,
    review_plan_id: Option<i64>,
    run_type: String,
    run_purpose: String,
    status: String,
    new_findings_count: i64,
    carried_findings_checked: i64,
    clean_run: bool,
    result_summary: String,
}

pub(super) struct StoredWorkRecordOutcome {
    id: i64,
    topic: String,
    next_actions: String,
    notable_operations: String,
}
