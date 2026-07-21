use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

use crate::db::{open_existing_project, project_id};
use crate::identity::{CanonicalValue, ReviewerMigrationBindingHandle, domain_digest};

#[derive(Clone, Debug)]
pub struct ReviewerMigrationBinding<'a> {
    pub source_reviewer_ref: &'a str,
    pub agent_label: &'a str,
    pub external_agent_id: &'a str,
    pub provenance_ref: &'a str,
    pub authority_event_id: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewerMigrationBindingOutcome {
    pub binding_handle: String,
    pub source_reviewer_ref: String,
    pub status: String,
    pub idempotent: bool,
}

pub fn bind_migration_reviewer(
    root: &Path,
    request: ReviewerMigrationBinding<'_>,
) -> Result<ReviewerMigrationBindingOutcome> {
    validate_binding(&request)?;
    let payload = CanonicalValue::object([
        (
            "source_reviewer",
            CanonicalValue::string(request.source_reviewer_ref),
        ),
        ("agent_label", CanonicalValue::string(request.agent_label)),
        (
            "external_agent_id",
            CanonicalValue::string(request.external_agent_id),
        ),
        (
            "provenance_ref",
            CanonicalValue::string(request.provenance_ref),
        ),
        (
            "authority_event",
            CanonicalValue::Integer(request.authority_event_id),
        ),
    ]);
    let payload_digest = domain_digest(
        b"agent-workbench:reviewer-migration-binding-payload-v1\0",
        &payload,
    );
    let handle = ReviewerMigrationBindingHandle::derive(
        b"agent-workbench:reviewer-migration-binding-v1\0",
        &payload,
    );

    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let project = project_id(&tx)?;
    let (source_id, source_status): (i64, String) = tx
        .query_row(
            "select id,status from reviewer_migration_sources where project_id=?1 and source_reviewer_ref=?2",
            params![project, request.source_reviewer_ref],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .context("reviewer_migration_source_not_found: source reviewer is not pending migration")?;

    if source_status == "bound" {
        let stored: (String, String) = tx.query_row(
            "select binding_handle,payload_digest from reviewer_migration_bindings where project_id=?1 and source_id=?2",
            params![project, source_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if stored.1 != payload_digest {
            bail!(
                "reviewer_migration_source_already_bound: changed binding cannot replace retained provenance"
            );
        }
        tx.commit()?;
        return Ok(ReviewerMigrationBindingOutcome {
            binding_handle: stored.0,
            source_reviewer_ref: request.source_reviewer_ref.to_string(),
            status: "bound".to_string(),
            idempotent: true,
        });
    }
    if source_status == "retired" {
        bail!("reviewer_migration_source_retired: superseded source review cannot be rebound");
    }

    let authority_type: String = tx
        .query_row(
            "select event_type from authority_events where project_id=?1 and id=?2 and status='active'",
            params![project, request.authority_event_id],
            |row| row.get(0),
        )
        .optional()?
        .context("reviewer_migration_authority_invalid: active project-local authority not found")?;
    if !matches!(
        authority_type.as_str(),
        "user_instruction" | "policy" | "agents"
    ) {
        bail!(
            "reviewer_migration_authority_invalid: authority must be a user instruction, policy, or agent-governance event"
        );
    }

    tx.execute(
        r#"
        insert into reviewer_migration_bindings(
          project_id,source_id,binding_handle,agent_label,external_agent_id,
          provenance_ref,authority_event_id,idempotency_key,payload_digest,created_at
        ) values(?1,?2,?3,?4,?5,?6,?7,?8,?9,current_timestamp)
        "#,
        params![
            project,
            source_id,
            handle.as_str(),
            request.agent_label,
            request.external_agent_id,
            request.provenance_ref,
            request.authority_event_id,
            payload_digest,
            payload_digest,
        ],
    )?;
    let binding_id = tx.last_insert_rowid();
    let changed = tx.execute(
        "update reviewer_migration_sources set status='bound',binding_id=?1 where project_id=?2 and id=?3 and status='pending'",
        params![binding_id, project, source_id],
    )?;
    if changed != 1 {
        bail!("reviewer_migration_state_changed: source reviewer changed before binding");
    }
    tx.commit()?;
    Ok(ReviewerMigrationBindingOutcome {
        binding_handle: handle.as_str().to_string(),
        source_reviewer_ref: request.source_reviewer_ref.to_string(),
        status: "bound".to_string(),
        idempotent: false,
    })
}

fn validate_binding(request: &ReviewerMigrationBinding<'_>) -> Result<()> {
    if !request.source_reviewer_ref.starts_with("legacy-reviewer:")
        || request.source_reviewer_ref.len() != "legacy-reviewer:".len() + 64
        || !request
            .source_reviewer_ref
            .bytes()
            .skip("legacy-reviewer:".len())
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("reviewer_migration_source_invalid: expected legacy-reviewer:<digest>");
    }
    if request.authority_event_id <= 0 {
        bail!("reviewer_migration_authority_invalid: authority must be positive");
    }
    if [
        request.agent_label,
        request.external_agent_id,
        request.provenance_ref,
    ]
    .into_iter()
    .any(|value| value.trim().is_empty())
    {
        bail!(
            "reviewer_migration_binding_incomplete: agent label, external agent id, and provenance ref are required"
        );
    }
    Ok(())
}
