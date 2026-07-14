use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{open_existing_project, project_id};
use crate::rules::{RuleBindingInput, insert_rule_binding, scope_type_for};

use super::*;

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
    if input.status == "fixed" {
        let Some(authority_event_id) = input.authority_event_id else {
            bail!("fixed command conversion requires --authority");
        };
        ensure_fixed_command_authority(&tx, project_id, authority_event_id)?;
    }
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
                review_policy_id: None,
                review_plan_id: None,
                work_unit_id: None,
                validation_gate_id: None,
                acceptance_record_id: None,
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
