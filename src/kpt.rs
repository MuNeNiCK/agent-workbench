use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use crate::db::{open_existing_project, project_id};
use crate::rules::{RuleBindingInput, insert_rule_binding, scope_type_for};

pub fn start_kpt_review(root: &Path, input: NewKptReview<'_>) -> Result<KptReviewOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let period_modifier = input.period.map(period_to_sqlite_modifier).transpose()?;
    tx.execute(
        r#"
        insert into kpt_reviews(
            project_id, scope, period_start, period_end, trigger, summary, status, created_at
        )
        values (
            ?1, ?2,
            case when ?3 is null then null else datetime('now', ?3) end,
            case when ?3 is null then null else current_timestamp end,
            'manual', ?4, 'open', current_timestamp
        )
        "#,
        params![project_id, input.scope, period_modifier, input.summary],
    )?;
    let kpt_review_id = tx.last_insert_rowid();

    let sources = parse_kpt_sources(input.from)?;
    let mut generated_item_count = 0;
    if sources.contains(&KptSource::Corrections) {
        generated_item_count += import_corrections_as_kpt_items(
            &tx,
            kpt_review_id,
            project_id,
            input.scope,
            period_modifier.as_deref(),
        )?;
    }
    if sources.contains(&KptSource::Findings) {
        generated_item_count += import_findings_as_kpt_items(
            &tx,
            kpt_review_id,
            project_id,
            period_modifier.as_deref(),
        )?;
    }
    if sources.contains(&KptSource::Commands) {
        generated_item_count += import_commands_as_kpt_items(
            &tx,
            kpt_review_id,
            project_id,
            period_modifier.as_deref(),
        )?;
    }
    if sources.contains(&KptSource::ReviewRuns) {
        generated_item_count += import_review_runs_as_kpt_items(
            &tx,
            kpt_review_id,
            project_id,
            period_modifier.as_deref(),
        )?;
    }
    if sources.contains(&KptSource::WorkRecords) {
        generated_item_count += import_work_records_as_kpt_items(
            &tx,
            kpt_review_id,
            project_id,
            period_modifier.as_deref(),
        )?;
    }

    tx.commit()?;

    Ok(KptReviewOutcome {
        kpt_review_id,
        generated_item_count,
    })
}

pub fn list_kpt_reviews(root: &Path, status: Option<&str>) -> Result<Vec<KptReviewRecord>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut records = Vec::new();

    match status {
        Some(status) => {
            let mut stmt = conn.prepare(
                r#"
                select id, scope, summary, status, created_at, closed_at
                from kpt_reviews
                where project_id = ?1 and status = ?2
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![project_id, status], kpt_review_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        None => {
            let mut stmt = conn.prepare(
                r#"
                select id, scope, summary, status, created_at, closed_at
                from kpt_reviews
                where project_id = ?1
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![project_id], kpt_review_record)?;
            for row in rows {
                records.push(row?);
            }
        }
    }

    Ok(records)
}

pub fn add_kpt_item(root: &Path, input: NewKptItem<'_>) -> Result<KptItemOutcome> {
    let conn = open_existing_project(root)?;
    let review_id = match input.kpt_review_id {
        Some(id) => id,
        None => latest_open_kpt_review(&conn)?,
    };

    conn.execute(
        r#"
        insert into kpt_items(
            kpt_review_id, item_type, title, details, severity, proposed_action,
            status, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, 'open', current_timestamp)
        "#,
        params![
            review_id,
            input.item_type,
            input.title,
            input.details,
            input.severity,
            input.proposed_action,
        ],
    )?;

    Ok(KptItemOutcome {
        kpt_item_id: conn.last_insert_rowid(),
        kpt_review_id: review_id,
    })
}

pub fn list_kpt_items(root: &Path, kpt_review_id: Option<i64>) -> Result<Vec<KptItemRecord>> {
    let conn = open_existing_project(root)?;
    let mut records = Vec::new();

    match kpt_review_id {
        Some(kpt_review_id) => {
            let mut stmt = conn.prepare(
                r#"
                select id, kpt_review_id, item_type, title, severity, status, linked_task_id
                from kpt_items
                where kpt_review_id = ?1
                order by id
                "#,
            )?;
            let rows = stmt.query_map(params![kpt_review_id], kpt_item_record)?;
            for row in rows {
                records.push(row?);
            }
        }
        None => {
            let mut stmt = conn.prepare(
                r#"
                select id, kpt_review_id, item_type, title, severity, status, linked_task_id
                from kpt_items
                order by id
                "#,
            )?;
            let rows = stmt.query_map([], kpt_item_record)?;
            for row in rows {
                records.push(row?);
            }
        }
    }

    Ok(records)
}

pub fn close_kpt_review(root: &Path, kpt_review_id: i64) -> Result<KptReviewCloseOutcome> {
    let conn = open_existing_project(root)?;
    let changed = conn.execute(
        "update kpt_reviews set status = 'closed', closed_at = current_timestamp where id = ?1 and status = 'open'",
        params![kpt_review_id],
    )?;
    if changed == 0 {
        bail!("kpt review not found or already closed");
    }

    Ok(KptReviewCloseOutcome { kpt_review_id })
}

pub fn convert_kpt_item_to_task(
    root: &Path,
    input: KptItemTaskConversion<'_>,
) -> Result<KptItemConversionOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let item = tx
        .query_row(
            r#"
            select id, title, details, proposed_action
            from kpt_items
            where id = ?1 and status in ('open', 'accepted')
            "#,
            params![input.kpt_item_id],
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
        .context("kpt item not found or not convertible")?;
    let task_title = input.task_title.unwrap_or(&item.title);
    let details = input
        .details
        .or(item.proposed_action.as_deref())
        .or(item.details.as_deref());

    tx.execute(
        r#"
        insert into tasks(work_unit_id, title, priority, status, source, details)
        values (?1, ?2, ?3, 'open', 'review', ?4)
        "#,
        params![input.work_unit_id, task_title, input.priority, details],
    )?;
    let task_id = tx.last_insert_rowid();
    tx.execute(
        r#"
        insert into kpt_item_conversions(kpt_item_id, target_type, task_id, created_at)
        values (?1, 'task', ?2, current_timestamp)
        "#,
        params![item.id, task_id],
    )?;
    let conversion_id = tx.last_insert_rowid();
    tx.execute(
        "update kpt_items set status = 'converted', linked_task_id = ?1 where id = ?2",
        params![task_id, item.id],
    )?;
    tx.commit()?;

    Ok(KptItemConversionOutcome {
        kpt_item_conversion_id: conversion_id,
        task_id,
    })
}

pub fn convert_kpt_item_to_review_policy(
    root: &Path,
    input: KptItemReviewPolicyConversion<'_>,
) -> Result<KptItemReviewPolicyConversionOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let item = convertible_kpt_item(&tx, input.kpt_item_id)?;
    let policy_name = input.name.unwrap_or(&item.title);
    tx.execute(
        r#"
        insert into review_policies(
            project_id, name, review_type, max_fresh_agents, max_resume_agents,
            max_parallel_agents, required_consecutive_clean_fresh_runs,
            required_consecutive_clean_resume_runs, stop_on_severity,
            allow_resume_review, allow_fresh_review, allow_new_findings_in_resume,
            on_max_agents_exceeded, run_count_scope, default_run_mode, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, 1, ?10, ?11, ?12, ?13, current_timestamp)
        "#,
        params![
            project_id,
            policy_name,
            input.review_type,
            input.max_fresh_agents,
            input.max_resume_agents,
            input.max_parallel_agents,
            input.required_consecutive_clean_fresh_runs,
            input.required_consecutive_clean_resume_runs,
            input.stop_on_severity,
            bool_to_i64(input.allow_new_findings_in_resume),
            input.on_max_agents_exceeded,
            input.run_count_scope,
            input.default_run_mode,
        ],
    )?;
    let review_policy_id = tx.last_insert_rowid();
    tx.execute(
        r#"
        insert into kpt_item_conversions(kpt_item_id, target_type, review_policy_id, created_at)
        values (?1, 'review_policy', ?2, current_timestamp)
        "#,
        params![item.id, review_policy_id],
    )?;
    let conversion_id = tx.last_insert_rowid();
    tx.execute(
        "update kpt_items set status = 'converted' where id = ?1",
        params![item.id],
    )?;
    tx.commit()?;
    Ok(KptItemReviewPolicyConversionOutcome {
        kpt_item_conversion_id: conversion_id,
        review_policy_id,
    })
}

pub fn convert_kpt_item_to_decision(
    root: &Path,
    input: KptItemDecisionConversion<'_>,
) -> Result<KptItemDecisionConversionOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let item = convertible_kpt_item(&tx, input.kpt_item_id)?;
    let topic = input.topic.unwrap_or(&item.title);
    let decision = input
        .decision
        .or(item.proposed_action.as_deref())
        .or(item.details.as_deref())
        .unwrap_or(&item.title);
    tx.execute(
        r#"
        insert into decisions(
            project_id, decision_key, topic, decision, rationale,
            compatibility_impact, status, authority_refs, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, 'accepted', ?7, current_timestamp)
        "#,
        params![
            project_id,
            input.decision_key,
            topic,
            decision,
            input.rationale,
            input.compatibility_impact,
            input.authority_refs,
        ],
    )?;
    let decision_id = tx.last_insert_rowid();
    tx.execute(
        r#"
        insert into kpt_item_conversions(kpt_item_id, target_type, decision_id, created_at)
        values (?1, 'decision', ?2, current_timestamp)
        "#,
        params![item.id, decision_id],
    )?;
    let conversion_id = tx.last_insert_rowid();
    tx.execute(
        "update kpt_items set status = 'converted' where id = ?1",
        params![item.id],
    )?;
    tx.commit()?;
    Ok(KptItemDecisionConversionOutcome {
        kpt_item_conversion_id: conversion_id,
        decision_id,
    })
}

pub fn convert_kpt_item_to_design_version(
    root: &Path,
    input: KptItemDesignVersionConversion,
) -> Result<KptItemDesignVersionConversionOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let item = convertible_kpt_item(&tx, input.kpt_item_id)?;
    tx.query_row(
        "select id from design_versions where id = ?1 and project_id = ?2",
        params![input.design_version_id, project_id],
        |row| row.get::<_, i64>(0),
    )
    .optional()?
    .context("design version not found")?;
    tx.execute(
        r#"
        insert into kpt_item_conversions(kpt_item_id, target_type, design_version_id, created_at)
        values (?1, 'design_version', ?2, current_timestamp)
        "#,
        params![item.id, input.design_version_id],
    )?;
    let conversion_id = tx.last_insert_rowid();
    tx.execute(
        "update kpt_items set status = 'converted' where id = ?1",
        params![item.id],
    )?;
    tx.commit()?;
    Ok(KptItemDesignVersionConversionOutcome {
        kpt_item_conversion_id: conversion_id,
        design_version_id: input.design_version_id,
    })
}

pub fn convert_kpt_item_to_command_profile(
    root: &Path,
    input: KptItemCommandProfileConversion<'_>,
) -> Result<KptItemCommandProfileConversionOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let item = convertible_kpt_item(&tx, input.kpt_item_id)?;
    let name = input.name.unwrap_or(&item.title);
    let command = input
        .command
        .or(item.proposed_action.as_deref())
        .or(item.details.as_deref())
        .context("command profile conversion requires --command or item action/details")?;
    tx.execute(
        r#"
        insert into command_profiles(
            project_id, name, command, command_type, scope, status, stability,
            timeout, expected_result, source, created_at, updated_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'agent_observed', current_timestamp, current_timestamp)
        "#,
        params![
            project_id,
            name,
            command,
            input.command_type,
            input.scope,
            input.status,
            input.stability,
            input.timeout,
            input.expected_result,
        ],
    )?;
    let command_profile_id = tx.last_insert_rowid();
    if matches!(input.status, "fixed" | "preferred") {
        let rule_scope = input.scope.unwrap_or("project");
        insert_rule_binding(
            &tx,
            RuleBindingInput {
                project_id,
                rule_source_type: "command_profile",
                authority_event_id: None,
                user_correction_id: None,
                command_profile_id: Some(command_profile_id),
                work_unit_id: None,
                scope_type: scope_type_for(rule_scope),
                scope_key: Some(rule_scope),
                precedence: if input.status == "fixed" { 70 } else { 55 },
            },
        )?;
    }
    tx.execute(
        r#"
        insert into kpt_item_conversions(kpt_item_id, target_type, command_profile_id, created_at)
        values (?1, 'command_profile', ?2, current_timestamp)
        "#,
        params![item.id, command_profile_id],
    )?;
    let conversion_id = tx.last_insert_rowid();
    tx.execute(
        "update kpt_items set status = 'converted' where id = ?1",
        params![item.id],
    )?;
    tx.commit()?;
    Ok(KptItemCommandProfileConversionOutcome {
        kpt_item_conversion_id: conversion_id,
        command_profile_id,
    })
}

fn convertible_kpt_item(conn: &rusqlite::Connection, kpt_item_id: i64) -> Result<StoredKptItem> {
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

fn latest_open_kpt_review(conn: &rusqlite::Connection) -> Result<i64> {
    conn.query_row(
        "select id from kpt_reviews where status = 'open' order by id desc limit 1",
        [],
        |row| row.get::<_, i64>(0),
    )
    .optional()?
    .context("no open kpt review; run kpt start first")
}

fn parse_kpt_sources(input: Option<&str>) -> Result<Vec<KptSource>> {
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

fn import_findings_as_kpt_items(
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

fn stored_finding_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredFinding> {
    Ok(StoredFinding {
        id: row.get(0)?,
        finding_type: row.get(1)?,
        severity: row.get(2)?,
        description: row.get(3)?,
        classification: row.get(4)?,
        status: row.get(5)?,
    })
}

fn import_corrections_as_kpt_items(
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

fn import_commands_as_kpt_items(
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

fn import_review_runs_as_kpt_items(
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

fn import_work_records_as_kpt_items(
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
            left join work_units w on w.id = r.work_unit_id
            where coalesce(w.project_id, ?1) = ?1
              and r.created_at >= datetime('now', ?2)
              and (coalesce(r.next_actions, '') != '' or coalesce(r.notable_operations, '') != '')
            order by r.id
            "#
        }
        None => {
            r#"
            select r.id, r.topic, coalesce(r.next_actions, ''), coalesce(r.notable_operations, '')
            from work_records r
            left join work_units w on w.id = r.work_unit_id
            where coalesce(w.project_id, ?1) = ?1
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

fn corrections_for_kpt(
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

fn period_to_sqlite_modifier(period: &str) -> Result<String> {
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

fn bool_to_i64(value: bool) -> i64 {
    if value { 1 } else { 0 }
}

fn kpt_review_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<KptReviewRecord> {
    Ok(KptReviewRecord {
        id: row.get(0)?,
        scope: row.get(1)?,
        summary: row.get(2)?,
        status: row.get(3)?,
        created_at: row.get(4)?,
        closed_at: row.get(5)?,
    })
}

fn kpt_item_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<KptItemRecord> {
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

fn stored_correction_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredCorrection> {
    Ok(StoredCorrection {
        id: row.get(0)?,
        scope: row.get(1)?,
        correction_type: row.get(2)?,
        mistake_pattern: row.get(3)?,
        correction: row.get(4)?,
        severity: row.get(5)?,
    })
}

fn command_drift_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredCommandDrift> {
    Ok(StoredCommandDrift {
        id: row.get(0)?,
        command_profile_id: row.get(1)?,
        profile_name: row.get(2)?,
        reason: row.get(3)?,
        status: row.get(4)?,
    })
}

fn review_run_kpt_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredReviewRunOutcome> {
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

fn work_record_kpt_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredWorkRecordOutcome> {
    Ok(StoredWorkRecordOutcome {
        id: row.get(0)?,
        topic: row.get(1)?,
        next_actions: row.get(2)?,
        notable_operations: row.get(3)?,
    })
}

fn review_outcome_severity(record: &StoredReviewRunOutcome) -> &'static str {
    if record.status == "failed" || record.new_findings_count > 0 {
        "high"
    } else if !record.clean_run {
        "medium"
    } else {
        "low"
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KptSource {
    Corrections,
    Findings,
    Commands,
    ReviewRuns,
    WorkRecords,
}

struct StoredKptItem {
    id: i64,
    title: String,
    details: Option<String>,
    proposed_action: Option<String>,
}

struct StoredCorrection {
    id: i64,
    scope: String,
    correction_type: String,
    mistake_pattern: String,
    correction: String,
    severity: String,
}

struct StoredFinding {
    id: i64,
    finding_type: String,
    severity: String,
    description: String,
    classification: String,
    status: String,
}

struct StoredCommandDrift {
    id: i64,
    command_profile_id: i64,
    profile_name: String,
    reason: String,
    status: String,
}

struct StoredReviewRunOutcome {
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

struct StoredWorkRecordOutcome {
    id: i64,
    topic: String,
    next_actions: String,
    notable_operations: String,
}

pub struct NewKptReview<'a> {
    pub scope: Option<&'a str>,
    pub summary: Option<&'a str>,
    pub from: Option<&'a str>,
    pub period: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptReviewOutcome {
    pub kpt_review_id: i64,
    pub generated_item_count: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptReviewRecord {
    pub id: i64,
    pub scope: Option<String>,
    pub summary: Option<String>,
    pub status: String,
    pub created_at: String,
    pub closed_at: Option<String>,
}

pub struct NewKptItem<'a> {
    pub kpt_review_id: Option<i64>,
    pub item_type: &'a str,
    pub title: &'a str,
    pub details: Option<&'a str>,
    pub severity: &'a str,
    pub proposed_action: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemOutcome {
    pub kpt_item_id: i64,
    pub kpt_review_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemRecord {
    pub id: i64,
    pub kpt_review_id: i64,
    pub item_type: String,
    pub title: String,
    pub severity: String,
    pub status: String,
    pub linked_task_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptReviewCloseOutcome {
    pub kpt_review_id: i64,
}

pub struct KptItemTaskConversion<'a> {
    pub kpt_item_id: i64,
    pub task_title: Option<&'a str>,
    pub details: Option<&'a str>,
    pub priority: &'a str,
    pub work_unit_id: Option<i64>,
}

pub struct KptItemReviewPolicyConversion<'a> {
    pub kpt_item_id: i64,
    pub name: Option<&'a str>,
    pub review_type: &'a str,
    pub max_fresh_agents: i64,
    pub max_resume_agents: i64,
    pub max_parallel_agents: i64,
    pub required_consecutive_clean_fresh_runs: i64,
    pub required_consecutive_clean_resume_runs: i64,
    pub stop_on_severity: &'a str,
    pub allow_new_findings_in_resume: bool,
    pub run_count_scope: &'a str,
    pub default_run_mode: &'a str,
    pub on_max_agents_exceeded: &'a str,
}

pub struct KptItemCommandProfileConversion<'a> {
    pub kpt_item_id: i64,
    pub name: Option<&'a str>,
    pub command: Option<&'a str>,
    pub command_type: &'a str,
    pub scope: Option<&'a str>,
    pub status: &'a str,
    pub stability: &'a str,
    pub timeout: Option<&'a str>,
    pub expected_result: Option<&'a str>,
}

pub struct KptItemDecisionConversion<'a> {
    pub kpt_item_id: i64,
    pub decision_key: Option<&'a str>,
    pub topic: Option<&'a str>,
    pub decision: Option<&'a str>,
    pub rationale: Option<&'a str>,
    pub compatibility_impact: Option<&'a str>,
    pub authority_refs: Option<&'a str>,
}

pub struct KptItemDesignVersionConversion {
    pub kpt_item_id: i64,
    pub design_version_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemConversionOutcome {
    pub kpt_item_conversion_id: i64,
    pub task_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemReviewPolicyConversionOutcome {
    pub kpt_item_conversion_id: i64,
    pub review_policy_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemCommandProfileConversionOutcome {
    pub kpt_item_conversion_id: i64,
    pub command_profile_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemDecisionConversionOutcome {
    pub kpt_item_conversion_id: i64,
    pub decision_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct KptItemDesignVersionConversionOutcome {
    pub kpt_item_conversion_id: i64,
    pub design_version_id: i64,
}
