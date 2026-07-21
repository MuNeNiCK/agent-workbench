use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

use crate::db::{open_existing_project, project_id};
use crate::identity::{CanonicalValue, domain_digest};
use crate::rules::{RuleBindingInput, insert_rule_binding, scope_type_for};

use super::*;

pub fn start_kpt_review(root: &Path, input: NewKptReview<'_>) -> Result<KptReviewOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
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
    let sql = if kpt_review_id.is_some() {
        "select id from kpt_items where kpt_review_id=?1 order by id"
    } else {
        "select id from kpt_items where ?1 is null order by id"
    };
    let ids = conn
        .prepare(sql)?
        .query_map(params![kpt_review_id], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut records = Vec::with_capacity(ids.len());
    for id in ids {
        let item = load_item(&conn, id)?;
        let conversion = existing_kpt_conversion(&conn, id)?
            .map(|value| value.record(id))
            .transpose()?;
        let dismissal = load_dismissal(&conn, id)?;
        let current_handle = dismissal
            .as_ref()
            .map(|receipt| receipt.current_handle.clone())
            .or_else(|| {
                conversion
                    .as_ref()
                    .and_then(|record| record.receipt.as_ref())
                    .map(|receipt| receipt.current_handle.clone())
            })
            .unwrap_or_else(|| item_handle(&item));
        let legal_actions = if matches!(item.status.as_str(), "open" | "accepted") {
            vec!["convert".to_string(), "dismiss".to_string()]
        } else {
            Vec::new()
        };
        let linked_task_id = conn.query_row(
            "select linked_task_id from kpt_items where id=?1",
            params![id],
            |row| row.get(0),
        )?;
        records.push(KptItemRecord {
            id,
            kpt_review_id: item.review_id,
            item_type: item.item_type,
            title: item.title,
            severity: item.severity,
            status: item.status,
            linked_task_id,
            details: item.details,
            proposed_action: item.proposed_action,
            current_handle,
            legal_actions,
            conversion,
            dismissal,
        });
    }
    Ok(records)
}

pub fn close_kpt_review(root: &Path, kpt_review_id: i64) -> Result<KptReviewCloseOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let changed = tx.execute(
        "update kpt_reviews set status = 'closed', closed_at = current_timestamp where id = ?1 and status = 'open'",
        params![kpt_review_id],
    )?;
    if changed == 0 {
        bail!("kpt review not found or already closed");
    }
    tx.commit()?;
    Ok(KptReviewCloseOutcome { kpt_review_id })
}

pub fn dismiss_kpt_item(
    root: &Path,
    input: KptItemDismissalRequest<'_>,
) -> Result<KptItemDismissalOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let project_id = project_id(&tx)?;
    let item = load_item(&tx, input.kpt_item_id)?;
    let review = load_review(&tx, item.review_id)?;
    let next = format!("agent-workbench kpt item list --review {}", item.review_id);

    if let Some(receipt) = load_dismissal(&tx, item.id)? {
        if receipt.predecessor_handle == input.expected_current
            && receipt.authority_event_id == input.authority_event_id
            && receipt.reason == input.reason
        {
            return Ok(KptItemDismissalOutcome::Existing(receipt));
        }
        return Ok(KptItemDismissalOutcome::ItemTerminal {
            state: item.status,
            current: receipt.current_handle,
            next,
        });
    }
    let observed = item_handle(&item);
    if !matches!(item.status.as_str(), "open" | "accepted") {
        return Ok(KptItemDismissalOutcome::ItemTerminal {
            state: item.status,
            current: observed,
            next,
        });
    }
    if input.expected_current != observed {
        return Ok(KptItemDismissalOutcome::StateChanged {
            expected: input.expected_current.to_string(),
            observed,
            next,
        });
    }
    if !input
        .reason
        .chars()
        .any(|character| !character.is_whitespace())
    {
        return Ok(KptItemDismissalOutcome::InputInvalid {
            field: "reason".to_string(),
            next,
        });
    }
    let required_scope = review
        .scope
        .as_deref()
        .filter(|scope| !scope.is_empty())
        .unwrap_or("project");
    let authority_valid: bool = tx.query_row(
        r#"
        select exists(
          select 1 from authority_events
          where id=?1 and project_id=?2 and status='active'
            and event_type in ('user_instruction','policy')
            and (coalesce(scope,'project')='project' or scope=?3)
        )
        "#,
        params![input.authority_event_id, project_id, required_scope],
        |row| row.get(0),
    )?;
    if !authority_valid {
        return Ok(KptItemDismissalOutcome::AuthorityInvalid {
            authority_event_id: input.authority_event_id,
            required_scope: required_scope.to_string(),
            next,
        });
    }
    let sources = load_source(&tx, item.id)?;
    if sources.len() > 1 {
        return Ok(KptItemDismissalOutcome::StateChanged {
            expected: input.expected_current.to_string(),
            observed: item_handle(&item),
            next,
        });
    }
    let source = sources.into_iter().next();
    let item_revision = item_revision(&item);
    let review_revision = review_handle(&review);
    let source_value = source
        .as_ref()
        .map(|source| {
            CanonicalValue::object([
                ("kind", CanonicalValue::string(source.source_kind.as_str())),
                (
                    "identity",
                    CanonicalValue::string(source.source_identity.as_str()),
                ),
                (
                    "revision",
                    CanonicalValue::string(source.source_revision.as_str()),
                ),
            ])
        })
        .unwrap_or(CanonicalValue::Null);
    let request_value = CanonicalValue::object([
        (
            "item_revision",
            CanonicalValue::string(item_revision.as_str()),
        ),
        ("source", source_value),
        (
            "review_revision",
            CanonicalValue::string(review_revision.as_str()),
        ),
        (
            "review_status",
            CanonicalValue::string(review.status.as_str()),
        ),
        (
            "authority",
            CanonicalValue::Integer(input.authority_event_id),
        ),
        ("reason", CanonicalValue::string(input.reason)),
        (
            "predecessor",
            CanonicalValue::string(input.expected_current),
        ),
    ]);
    let replay_identity =
        domain_digest(b"agent-workbench:kpt-dismissal-replay-v1\0", &request_value);
    let decision_handle = format!(
        "kpt_decision_{}",
        domain_digest(
            b"agent-workbench:kpt-dismissal-decision-v1\0",
            &request_value
        )
    );
    let current_handle = format!(
        "kpt_item_{}",
        domain_digest(
            b"agent-workbench:kpt-dismissed-current-v1\0",
            &CanonicalValue::object([
                (
                    "predecessor",
                    CanonicalValue::string(input.expected_current)
                ),
                ("decision", CanonicalValue::string(decision_handle.as_str())),
            ]),
        )
    );
    let changed = tx.execute(
        "update kpt_items set status='dismissed' where id=?1 and status in ('open','accepted')",
        params![item.id],
    )?;
    if changed != 1 {
        return Ok(KptItemDismissalOutcome::StateChanged {
            expected: input.expected_current.to_string(),
            observed: item_handle(&item),
            next,
        });
    }
    tx.execute(
        r#"
        insert into kpt_item_dismissals(
          kpt_item_id,item_revision,source_kind,source_identity,source_revision,
          review_revision,review_status,authority_event_id,reason,predecessor_handle,
          decision_handle,current_handle,replay_identity,created_at
        ) values(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,current_timestamp)
        "#,
        params![
            item.id,
            item_revision,
            source.as_ref().map(|value| value.source_kind.as_str()),
            source.as_ref().map(|value| value.source_identity.as_str()),
            source.as_ref().map(|value| value.source_revision.as_str()),
            review_revision,
            review.status,
            input.authority_event_id,
            input.reason,
            input.expected_current,
            decision_handle,
            current_handle,
            replay_identity,
        ],
    )?;
    let receipt = load_dismissal(&tx, item.id)?.context("dismissal receipt was not committed")?;
    tx.commit()?;
    Ok(KptItemDismissalOutcome::Dismissed(receipt))
}

struct PreparedKptConversion {
    item: ItemSnapshot,
    item_revision: String,
    predecessor_handle: String,
    request_identity: String,
}

struct PreparedKptConversionReceipt {
    receipt_identity: String,
    current_handle: String,
}

fn prepare_kpt_conversion(
    conn: &rusqlite::Connection,
    kpt_item_id: i64,
    target_type: &str,
    payload: CanonicalValue,
) -> Result<(PreparedKptConversion, Option<KptItemConversionReceipt>)> {
    let item = load_item(conn, kpt_item_id)?;
    let item_revision = item_revision(&item);
    let predecessor_handle = item_handle(&item);
    let request_identity = domain_digest(
        b"agent-workbench:kpt-conversion-request-v1\0",
        &CanonicalValue::object([
            ("item_revision", CanonicalValue::string(&item_revision)),
            ("target_type", CanonicalValue::string(target_type)),
            ("payload", payload),
        ]),
    );
    let prepared = PreparedKptConversion {
        item,
        item_revision,
        predecessor_handle,
        request_identity,
    };
    let Some(existing) = existing_kpt_conversion(conn, kpt_item_id)? else {
        if !matches!(prepared.item.status.as_str(), "open" | "accepted") {
            bail!("kpt item not found or not convertible");
        }
        return Ok((prepared, None));
    };
    let record = existing.record(kpt_item_id)?;
    if existing.target_type == target_type
        && record
            .receipt
            .as_ref()
            .is_some_and(|receipt| receipt.request_identity == prepared.request_identity)
    {
        return Ok((prepared, record.receipt));
    }
    let current_handle = record
        .receipt
        .as_ref()
        .map(|receipt| receipt.current_handle.clone())
        .unwrap_or_else(|| item_handle(&prepared.item));
    Err(KptConversionAlreadyCommitted {
        record,
        current_handle,
        next: format!(
            "agent-workbench kpt item list --review {}",
            prepared.item.review_id
        ),
    }
    .into())
}

fn prepare_kpt_conversion_receipt(
    prepared: &PreparedKptConversion,
    target: &KptItemConversionTarget,
) -> PreparedKptConversionReceipt {
    let receipt_identity = domain_digest(
        b"agent-workbench:kpt-conversion-receipt-v1\0",
        &CanonicalValue::object([
            (
                "request_identity",
                CanonicalValue::string(&prepared.request_identity),
            ),
            ("target_type", CanonicalValue::string(target.target_type())),
            ("target_id", CanonicalValue::Integer(target.target_id())),
        ]),
    );
    let current_handle = format!(
        "kpt_item_{}",
        domain_digest(
            b"agent-workbench:kpt-converted-current-v1\0",
            &CanonicalValue::object([
                (
                    "predecessor",
                    CanonicalValue::string(&prepared.predecessor_handle),
                ),
                ("receipt", CanonicalValue::string(&receipt_identity)),
            ]),
        )
    );
    PreparedKptConversionReceipt {
        receipt_identity,
        current_handle,
    }
}

fn complete_kpt_conversion_receipt(
    conversion_id: i64,
    prepared: PreparedKptConversion,
    target: KptItemConversionTarget,
    receipt: PreparedKptConversionReceipt,
) -> KptItemConversionReceipt {
    KptItemConversionReceipt {
        kpt_item_conversion_id: conversion_id,
        kpt_item_id: prepared.item.id,
        item_revision: prepared.item_revision,
        target,
        predecessor_handle: prepared.predecessor_handle,
        request_identity: prepared.request_identity,
        receipt_identity: receipt.receipt_identity,
        current_handle: receipt.current_handle,
    }
}

fn optional_string(value: Option<&str>) -> CanonicalValue {
    value
        .map(CanonicalValue::string)
        .unwrap_or(CanonicalValue::Null)
}

fn optional_integer(value: Option<i64>) -> CanonicalValue {
    value
        .map(CanonicalValue::Integer)
        .unwrap_or(CanonicalValue::Null)
}

pub fn convert_kpt_item_to_task(
    root: &Path,
    input: KptItemTaskConversion<'_>,
) -> Result<KptItemConversionOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let item = load_item(&tx, input.kpt_item_id)?;
    let task_title = input.task_title.unwrap_or(&item.title);
    let details = input
        .details
        .or(item.proposed_action.as_deref())
        .or(item.details.as_deref());
    let payload = CanonicalValue::object([
        ("work_unit", optional_integer(input.work_unit_id)),
        ("title", CanonicalValue::string(task_title)),
        ("details", optional_string(details)),
        ("priority", CanonicalValue::string(input.priority)),
    ]);
    let (prepared, replay) = prepare_kpt_conversion(&tx, input.kpt_item_id, "task", payload)?;
    if let Some(receipt) = replay {
        let KptItemConversionTarget::Task(task_id) = receipt.target else {
            unreachable!("exact task replay must retain a task target")
        };
        return Ok(KptItemConversionOutcome {
            kpt_item_conversion_id: receipt.kpt_item_conversion_id,
            task_id,
            receipt,
            already_applied: true,
        });
    }

    tx.execute(
        r#"
        insert into tasks(work_unit_id, title, priority, status, source, details)
        values (?1, ?2, ?3, 'open', 'review', ?4)
        "#,
        params![input.work_unit_id, task_title, input.priority, details],
    )?;
    let task_id = tx.last_insert_rowid();
    let target = KptItemConversionTarget::Task(task_id);
    let receipt = prepare_kpt_conversion_receipt(&prepared, &target);
    tx.execute(
        r#"
        insert into kpt_item_conversions(
          kpt_item_id,target_type,task_id,item_revision,predecessor_handle,
          request_identity,receipt_identity,current_handle,created_at
        ) values (?1,'task',?2,?3,?4,?5,?6,?7,current_timestamp)
        "#,
        params![
            prepared.item.id,
            task_id,
            prepared.item_revision,
            prepared.predecessor_handle,
            prepared.request_identity,
            receipt.receipt_identity,
            receipt.current_handle,
        ],
    )?;
    let conversion_id = tx.last_insert_rowid();
    tx.execute(
        "update kpt_items set status = 'converted', linked_task_id = ?1 where id = ?2",
        params![task_id, prepared.item.id],
    )?;
    let receipt = complete_kpt_conversion_receipt(conversion_id, prepared, target, receipt);
    tx.commit()?;

    Ok(KptItemConversionOutcome {
        kpt_item_conversion_id: conversion_id,
        task_id,
        receipt,
        already_applied: false,
    })
}

pub fn convert_kpt_item_to_rule(
    root: &Path,
    input: KptItemRuleConversion<'_>,
) -> Result<KptItemRuleConversionOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let project_id = project_id(&tx)?;
    let item = load_item(&tx, input.kpt_item_id)?;
    let review_scope = load_review(&tx, item.review_id)?.scope;
    let scope = input
        .scope
        .or(review_scope.as_deref().filter(|scope| !scope.is_empty()))
        .unwrap_or("project");
    let title = input.title.unwrap_or(&item.title);
    let body = input
        .body
        .filter(|value| !value.is_empty())
        .or(item
            .proposed_action
            .as_deref()
            .filter(|value| !value.is_empty()))
        .or(item.details.as_deref().filter(|value| !value.is_empty()))
        .context("rule conversion requires a nonempty body")?;
    let payload = CanonicalValue::object([
        ("scope", CanonicalValue::string(scope)),
        ("title", CanonicalValue::string(title)),
        ("body", CanonicalValue::string(body)),
    ]);
    let (prepared, replay) = prepare_kpt_conversion(&tx, input.kpt_item_id, "rule", payload)?;
    if let Some(receipt) = replay {
        let KptItemConversionTarget::Rule(kpt_rule_id) = receipt.target else {
            unreachable!("exact rule replay must retain a rule target")
        };
        return Ok(KptItemRuleConversionOutcome {
            kpt_item_conversion_id: receipt.kpt_item_conversion_id,
            kpt_rule_id,
            receipt,
            already_applied: true,
        });
    }
    tx.execute(
        "insert into kpt_rules(project_id,kpt_item_id,scope,title,body,status,created_at) values(?1,?2,?3,?4,?5,'recorded',current_timestamp)",
        params![project_id, prepared.item.id, scope, title, body],
    )?;
    let kpt_rule_id = tx.last_insert_rowid();
    let target = KptItemConversionTarget::Rule(kpt_rule_id);
    let receipt = prepare_kpt_conversion_receipt(&prepared, &target);
    tx.execute(
        r#"insert into kpt_item_conversions(
             kpt_item_id,target_type,kpt_rule_id,item_revision,predecessor_handle,
             request_identity,receipt_identity,current_handle,created_at
           ) values(?1,'rule',?2,?3,?4,?5,?6,?7,current_timestamp)"#,
        params![
            prepared.item.id,
            kpt_rule_id,
            prepared.item_revision,
            prepared.predecessor_handle,
            prepared.request_identity,
            receipt.receipt_identity,
            receipt.current_handle
        ],
    )?;
    let kpt_item_conversion_id = tx.last_insert_rowid();
    tx.execute(
        "update kpt_items set status='converted' where id=?1",
        params![prepared.item.id],
    )?;
    let receipt =
        complete_kpt_conversion_receipt(kpt_item_conversion_id, prepared, target, receipt);
    tx.commit()?;
    Ok(KptItemRuleConversionOutcome {
        kpt_item_conversion_id,
        kpt_rule_id,
        receipt,
        already_applied: false,
    })
}

pub fn convert_kpt_item_to_correction(
    root: &Path,
    input: KptItemCorrectionConversion<'_>,
) -> Result<KptItemCorrectionConversionOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let project_id = project_id(&tx)?;
    let item = load_item(&tx, input.kpt_item_id)?;
    let review_scope = load_review(&tx, item.review_id)?.scope;
    let scope = input
        .scope
        .or(review_scope.as_deref().filter(|scope| !scope.is_empty()))
        .unwrap_or("project");
    let source_label = input.source_label.unwrap_or(&item.title);
    let expected_change = input
        .expected_change
        .filter(|value| !value.is_empty())
        .or(item
            .proposed_action
            .as_deref()
            .filter(|value| !value.is_empty()))
        .or(item.details.as_deref().filter(|value| !value.is_empty()))
        .context("correction conversion requires a nonempty expected change")?;
    let applies_to = match scope {
        "repository" | "design_package" | "command_profile" | "agent_role" => scope,
        "current_work_unit" => "current_work_unit",
        _ => "project",
    };
    let payload = CanonicalValue::object([
        ("scope", CanonicalValue::string(scope)),
        ("source_label", CanonicalValue::string(source_label)),
        ("expected_change", CanonicalValue::string(expected_change)),
        ("severity", CanonicalValue::string(input.severity)),
    ]);
    let (prepared, replay) = prepare_kpt_conversion(&tx, input.kpt_item_id, "correction", payload)?;
    if let Some(receipt) = replay {
        let KptItemConversionTarget::Correction(user_correction_id) = receipt.target else {
            unreachable!("exact correction replay must retain a correction target")
        };
        return Ok(KptItemCorrectionConversionOutcome {
            kpt_item_conversion_id: receipt.kpt_item_conversion_id,
            user_correction_id,
            receipt,
            already_applied: true,
        });
    }
    tx.execute(
        r#"
        insert into user_corrections(
          project_id,scope,correction_type,mistake_pattern,correction,applies_to,severity,status,created_at
        ) values(?1,?2,'process',?3,?4,?5,?6,'active',current_timestamp)
        "#,
        params![project_id, scope, source_label, expected_change, applies_to, input.severity],
    )?;
    let user_correction_id = tx.last_insert_rowid();
    insert_rule_binding(
        &tx,
        RuleBindingInput {
            project_id,
            rule_source_type: "user_correction",
            authority_event_id: None,
            user_correction_id: Some(user_correction_id),
            command_profile_id: None,
            review_policy_id: None,
            review_plan_id: None,
            work_unit_id: None,
            validation_gate_id: None,
            acceptance_record_id: None,
            scope_type: scope_type_for(scope),
            scope_key: Some(scope),
            precedence: 80,
        },
    )?;
    let target = KptItemConversionTarget::Correction(user_correction_id);
    let prepared_receipt = prepare_kpt_conversion_receipt(&prepared, &target);
    tx.execute(
        r#"insert into kpt_item_conversions(
             kpt_item_id,target_type,user_correction_id,item_revision,predecessor_handle,
             request_identity,receipt_identity,current_handle,created_at
           ) values(?1,'correction',?2,?3,?4,?5,?6,?7,current_timestamp)"#,
        params![
            prepared.item.id,
            user_correction_id,
            prepared.item_revision,
            prepared.predecessor_handle,
            prepared.request_identity,
            prepared_receipt.receipt_identity,
            prepared_receipt.current_handle
        ],
    )?;
    let kpt_item_conversion_id = tx.last_insert_rowid();
    tx.execute(
        "update kpt_items set status='converted' where id=?1",
        params![prepared.item.id],
    )?;
    let receipt =
        complete_kpt_conversion_receipt(kpt_item_conversion_id, prepared, target, prepared_receipt);
    tx.commit()?;
    Ok(KptItemCorrectionConversionOutcome {
        kpt_item_conversion_id,
        user_correction_id,
        receipt,
        already_applied: false,
    })
}

pub fn convert_kpt_item_to_review_policy(
    root: &Path,
    input: KptItemReviewPolicyConversion<'_>,
) -> Result<KptItemReviewPolicyConversionOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let project_id = project_id(&tx)?;
    let item = load_item(&tx, input.kpt_item_id)?;
    let policy_name = input.name.unwrap_or(&item.title);
    let payload = CanonicalValue::object([
        ("name", CanonicalValue::string(policy_name)),
        ("review_type", CanonicalValue::string(input.review_type)),
        (
            "max_fresh_agents",
            CanonicalValue::Integer(input.max_fresh_agents),
        ),
        (
            "max_resume_agents",
            CanonicalValue::Integer(input.max_resume_agents),
        ),
        (
            "max_parallel_agents",
            CanonicalValue::Integer(input.max_parallel_agents),
        ),
        (
            "fresh_clean",
            CanonicalValue::Integer(input.required_consecutive_clean_fresh_runs),
        ),
        (
            "resume_clean",
            CanonicalValue::Integer(input.required_consecutive_clean_resume_runs),
        ),
        (
            "stop_on_severity",
            CanonicalValue::string(input.stop_on_severity),
        ),
        (
            "allow_new_findings",
            CanonicalValue::Integer(bool_to_i64(input.allow_new_findings_in_resume)),
        ),
        (
            "run_count_scope",
            CanonicalValue::string(input.run_count_scope),
        ),
        (
            "default_run_mode",
            CanonicalValue::string(input.default_run_mode),
        ),
        (
            "on_max_agents_exceeded",
            CanonicalValue::string(input.on_max_agents_exceeded),
        ),
    ]);
    let (prepared, replay) =
        prepare_kpt_conversion(&tx, input.kpt_item_id, "review_policy", payload)?;
    if let Some(receipt) = replay {
        let KptItemConversionTarget::ReviewPolicy(review_policy_id) = receipt.target else {
            unreachable!("exact review-policy replay must retain a review-policy target")
        };
        return Ok(KptItemReviewPolicyConversionOutcome {
            kpt_item_conversion_id: receipt.kpt_item_conversion_id,
            review_policy_id,
            receipt,
            already_applied: true,
        });
    }
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
    let target = KptItemConversionTarget::ReviewPolicy(review_policy_id);
    let prepared_receipt = prepare_kpt_conversion_receipt(&prepared, &target);
    tx.execute(
        r#"
        insert into kpt_item_conversions(
          kpt_item_id,target_type,review_policy_id,item_revision,predecessor_handle,
          request_identity,receipt_identity,current_handle,created_at
        ) values (?1,'review_policy',?2,?3,?4,?5,?6,?7,current_timestamp)
        "#,
        params![
            prepared.item.id,
            review_policy_id,
            prepared.item_revision,
            prepared.predecessor_handle,
            prepared.request_identity,
            prepared_receipt.receipt_identity,
            prepared_receipt.current_handle
        ],
    )?;
    let conversion_id = tx.last_insert_rowid();
    tx.execute(
        "update kpt_items set status = 'converted' where id = ?1",
        params![prepared.item.id],
    )?;
    let receipt =
        complete_kpt_conversion_receipt(conversion_id, prepared, target, prepared_receipt);
    tx.commit()?;
    Ok(KptItemReviewPolicyConversionOutcome {
        kpt_item_conversion_id: conversion_id,
        review_policy_id,
        receipt,
        already_applied: false,
    })
}

pub fn convert_kpt_item_to_decision(
    root: &Path,
    input: KptItemDecisionConversion<'_>,
) -> Result<KptItemDecisionConversionOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let project_id = project_id(&tx)?;
    let item = load_item(&tx, input.kpt_item_id)?;
    let topic = input.topic.unwrap_or(&item.title);
    let decision = input
        .decision
        .or(item.proposed_action.as_deref())
        .or(item.details.as_deref())
        .unwrap_or(&item.title);
    let payload = CanonicalValue::object([
        ("decision_key", optional_string(input.decision_key)),
        ("topic", CanonicalValue::string(topic)),
        ("decision", CanonicalValue::string(decision)),
        ("rationale", optional_string(input.rationale)),
        (
            "compatibility_impact",
            optional_string(input.compatibility_impact),
        ),
        ("authority_refs", optional_string(input.authority_refs)),
    ]);
    let (prepared, replay) = prepare_kpt_conversion(&tx, input.kpt_item_id, "decision", payload)?;
    if let Some(receipt) = replay {
        let KptItemConversionTarget::Decision(decision_id) = receipt.target else {
            unreachable!("exact decision replay must retain a decision target")
        };
        return Ok(KptItemDecisionConversionOutcome {
            kpt_item_conversion_id: receipt.kpt_item_conversion_id,
            decision_id,
            receipt,
            already_applied: true,
        });
    }
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
    let target = KptItemConversionTarget::Decision(decision_id);
    let prepared_receipt = prepare_kpt_conversion_receipt(&prepared, &target);
    tx.execute(
        r#"
        insert into kpt_item_conversions(
          kpt_item_id,target_type,decision_id,item_revision,predecessor_handle,
          request_identity,receipt_identity,current_handle,created_at
        ) values (?1,'decision',?2,?3,?4,?5,?6,?7,current_timestamp)
        "#,
        params![
            prepared.item.id,
            decision_id,
            prepared.item_revision,
            prepared.predecessor_handle,
            prepared.request_identity,
            prepared_receipt.receipt_identity,
            prepared_receipt.current_handle
        ],
    )?;
    let conversion_id = tx.last_insert_rowid();
    tx.execute(
        "update kpt_items set status = 'converted' where id = ?1",
        params![prepared.item.id],
    )?;
    let receipt =
        complete_kpt_conversion_receipt(conversion_id, prepared, target, prepared_receipt);
    tx.commit()?;
    Ok(KptItemDecisionConversionOutcome {
        kpt_item_conversion_id: conversion_id,
        decision_id,
        receipt,
        already_applied: false,
    })
}

pub fn convert_kpt_item_to_design_version(
    root: &Path,
    input: KptItemDesignVersionConversion,
) -> Result<KptItemDesignVersionConversionOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let project_id = project_id(&tx)?;
    let payload = CanonicalValue::object([(
        "design_version_id",
        CanonicalValue::Integer(input.design_version_id),
    )]);
    let (prepared, replay) =
        prepare_kpt_conversion(&tx, input.kpt_item_id, "design_version", payload)?;
    if let Some(receipt) = replay {
        let KptItemConversionTarget::DesignVersion(design_version_id) = receipt.target else {
            unreachable!("exact design-version replay must retain a design-version target")
        };
        return Ok(KptItemDesignVersionConversionOutcome {
            kpt_item_conversion_id: receipt.kpt_item_conversion_id,
            design_version_id,
            receipt,
            already_applied: true,
        });
    }
    tx.query_row(
        "select id from design_versions where id = ?1 and project_id = ?2",
        params![input.design_version_id, project_id],
        |row| row.get::<_, i64>(0),
    )
    .optional()?
    .context("design version not found")?;
    let target = KptItemConversionTarget::DesignVersion(input.design_version_id);
    let prepared_receipt = prepare_kpt_conversion_receipt(&prepared, &target);
    tx.execute(
        r#"
        insert into kpt_item_conversions(
          kpt_item_id,target_type,design_version_id,item_revision,predecessor_handle,
          request_identity,receipt_identity,current_handle,created_at
        ) values (?1,'design_version',?2,?3,?4,?5,?6,?7,current_timestamp)
        "#,
        params![
            prepared.item.id,
            input.design_version_id,
            prepared.item_revision,
            prepared.predecessor_handle,
            prepared.request_identity,
            prepared_receipt.receipt_identity,
            prepared_receipt.current_handle
        ],
    )?;
    let conversion_id = tx.last_insert_rowid();
    tx.execute(
        "update kpt_items set status = 'converted' where id = ?1",
        params![prepared.item.id],
    )?;
    let receipt =
        complete_kpt_conversion_receipt(conversion_id, prepared, target, prepared_receipt);
    tx.commit()?;
    Ok(KptItemDesignVersionConversionOutcome {
        kpt_item_conversion_id: conversion_id,
        design_version_id: input.design_version_id,
        receipt,
        already_applied: false,
    })
}

pub fn convert_kpt_item_to_command_profile(
    root: &Path,
    input: KptItemCommandProfileConversion<'_>,
) -> Result<KptItemCommandProfileConversionOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let project_id = project_id(&tx)?;
    let item = load_item(&tx, input.kpt_item_id)?;
    let name = input.name.unwrap_or(&item.title);
    let command = input
        .command
        .or(item.proposed_action.as_deref())
        .or(item.details.as_deref())
        .context("command profile conversion requires --command or item action/details")?;
    let payload = CanonicalValue::object([
        ("name", CanonicalValue::string(name)),
        ("command", CanonicalValue::string(command)),
        ("command_type", CanonicalValue::string(input.command_type)),
        ("scope", optional_string(input.scope)),
        ("status", CanonicalValue::string(input.status)),
        ("stability", CanonicalValue::string(input.stability)),
        ("timeout", optional_string(input.timeout)),
        ("expected_result", optional_string(input.expected_result)),
        ("authority", optional_integer(input.authority_event_id)),
    ]);
    let (prepared, replay) =
        prepare_kpt_conversion(&tx, input.kpt_item_id, "command_profile", payload)?;
    if let Some(receipt) = replay {
        let KptItemConversionTarget::CommandProfile(command_profile_id) = receipt.target else {
            unreachable!("exact command-profile replay must retain a command-profile target")
        };
        return Ok(KptItemCommandProfileConversionOutcome {
            kpt_item_conversion_id: receipt.kpt_item_conversion_id,
            command_profile_id,
            receipt,
            already_applied: true,
        });
    }
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
    let target = KptItemConversionTarget::CommandProfile(command_profile_id);
    let prepared_receipt = prepare_kpt_conversion_receipt(&prepared, &target);
    tx.execute(
        r#"
        insert into kpt_item_conversions(
          kpt_item_id,target_type,command_profile_id,item_revision,predecessor_handle,
          request_identity,receipt_identity,current_handle,created_at
        ) values (?1,'command_profile',?2,?3,?4,?5,?6,?7,current_timestamp)
        "#,
        params![
            prepared.item.id,
            command_profile_id,
            prepared.item_revision,
            prepared.predecessor_handle,
            prepared.request_identity,
            prepared_receipt.receipt_identity,
            prepared_receipt.current_handle
        ],
    )?;
    let conversion_id = tx.last_insert_rowid();
    tx.execute(
        "update kpt_items set status = 'converted' where id = ?1",
        params![prepared.item.id],
    )?;
    let receipt =
        complete_kpt_conversion_receipt(conversion_id, prepared, target, prepared_receipt);
    tx.commit()?;
    Ok(KptItemCommandProfileConversionOutcome {
        kpt_item_conversion_id: conversion_id,
        command_profile_id,
        receipt,
        already_applied: false,
    })
}
