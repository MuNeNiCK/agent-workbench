use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};

use crate::identity::{CanonicalValue, domain_digest};

use super::{KptItemDismissalReceipt, KptItemSourceBinding};

pub(super) struct ItemSnapshot {
    pub id: i64,
    pub review_id: i64,
    pub item_type: String,
    pub title: String,
    pub details: Option<String>,
    pub severity: String,
    pub proposed_action: Option<String>,
    pub status: String,
    pub created_at: String,
}

pub(super) struct ReviewSnapshot {
    pub id: i64,
    pub scope: Option<String>,
    pub summary: Option<String>,
    pub status: String,
    pub created_at: String,
    pub closed_at: Option<String>,
}

pub(super) fn load_item(conn: &Connection, item_id: i64) -> Result<ItemSnapshot> {
    conn.query_row(
        "select id,kpt_review_id,item_type,title,details,severity,proposed_action,status,created_at from kpt_items where id=?1",
        params![item_id],
        |row| Ok(ItemSnapshot {
            id: row.get(0)?, review_id: row.get(1)?, item_type: row.get(2)?,
            title: row.get(3)?, details: row.get(4)?, severity: row.get(5)?,
            proposed_action: row.get(6)?, status: row.get(7)?, created_at: row.get(8)?,
        }),
    ).optional()?.context("kpt item not found")
}

pub(super) fn load_review(conn: &Connection, review_id: i64) -> Result<ReviewSnapshot> {
    conn.query_row(
        "select id,scope,summary,status,created_at,closed_at from kpt_reviews where id=?1",
        params![review_id],
        |row| {
            Ok(ReviewSnapshot {
                id: row.get(0)?,
                scope: row.get(1)?,
                summary: row.get(2)?,
                status: row.get(3)?,
                created_at: row.get(4)?,
                closed_at: row.get(5)?,
            })
        },
    )
    .optional()?
    .context("kpt review not found")
}

pub(super) fn item_revision(item: &ItemSnapshot) -> String {
    domain_digest(
        b"agent-workbench:kpt-item-revision-v1\0",
        &CanonicalValue::object([
            ("id", CanonicalValue::Integer(item.id)),
            ("review", CanonicalValue::Integer(item.review_id)),
            ("type", CanonicalValue::string(&item.item_type)),
            ("title", CanonicalValue::string(&item.title)),
            ("details", optional(&item.details)),
            ("severity", CanonicalValue::string(&item.severity)),
            ("proposed_action", optional(&item.proposed_action)),
            ("created_at", CanonicalValue::string(&item.created_at)),
        ]),
    )
}

pub(super) fn item_handle(item: &ItemSnapshot) -> String {
    format!(
        "kpt_item_{}",
        domain_digest(
            b"agent-workbench:kpt-item-current-v1\0",
            &CanonicalValue::object([
                ("revision", CanonicalValue::string(item_revision(item))),
                ("status", CanonicalValue::string(&item.status)),
            ]),
        )
    )
}

pub(super) fn review_handle(review: &ReviewSnapshot) -> String {
    format!(
        "kpt_review_{}",
        domain_digest(
            b"agent-workbench:kpt-review-current-v1\0",
            &CanonicalValue::object([
                ("id", CanonicalValue::Integer(review.id)),
                ("scope", optional(&review.scope)),
                ("summary", optional(&review.summary)),
                ("status", CanonicalValue::string(&review.status)),
                ("created_at", CanonicalValue::string(&review.created_at)),
                ("closed_at", optional(&review.closed_at)),
            ]),
        )
    )
}

pub(super) fn load_source(conn: &Connection, item_id: i64) -> Result<Vec<KptItemSourceBinding>> {
    let mut statement = conn.prepare(
        "select source_kind,source_identity,source_revision from kpt_item_sources where kpt_item_id=?1 order by id",
    )?;
    statement
        .query_map(params![item_id], |row| {
            Ok(KptItemSourceBinding {
                source_kind: row.get(0)?,
                source_identity: row.get(1)?,
                source_revision: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub(super) fn load_dismissal(
    conn: &Connection,
    item_id: i64,
) -> Result<Option<KptItemDismissalReceipt>> {
    conn.query_row(
        r#"
        select kpt_item_id,item_revision,source_kind,source_identity,source_revision,
               review_revision,review_status,authority_event_id,reason,predecessor_handle,
               decision_handle,current_handle,replay_identity
        from kpt_item_dismissals where kpt_item_id=?1
        "#,
        params![item_id],
        |row| {
            let kind: Option<String> = row.get(2)?;
            let identity: Option<String> = row.get(3)?;
            let revision: Option<String> = row.get(4)?;
            Ok(KptItemDismissalReceipt {
                kpt_item_id: row.get(0)?,
                item_revision: row.get(1)?,
                source: kind.map(|source_kind| KptItemSourceBinding {
                    source_kind,
                    source_identity: identity.expect("dismissal source identity constrained"),
                    source_revision: revision.expect("dismissal source revision constrained"),
                }),
                review_revision: row.get(5)?,
                review_status: row.get(6)?,
                authority_event_id: row.get(7)?,
                reason: row.get(8)?,
                predecessor_handle: row.get(9)?,
                decision_handle: row.get(10)?,
                current_handle: row.get(11)?,
                replay_identity: row.get(12)?,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn optional(value: &Option<String>) -> CanonicalValue {
    value
        .as_ref()
        .map(CanonicalValue::string)
        .unwrap_or(CanonicalValue::Null)
}
