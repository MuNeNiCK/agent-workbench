use std::collections::BTreeMap;

use anyhow::{Context, Result};
use rusqlite::{Connection, params};

use crate::identity::{CanonicalValue, domain_digest};

use super::super::plan::PlannedTask;
use super::super::source::{EvidenceSource, OwnerSource, SourceSnapshot};
use super::super::status::{ChecklistItemState, ChecklistState, TaskState};

struct CompletionClaim<'a> {
    identity_digest: &'a str,
    revision_digest: &'a str,
    digest: String,
    evidence: Vec<&'a EvidenceSource>,
}

pub(super) fn materialize(
    conn: &Connection,
    snapshot: &SourceSnapshot,
    owner: &OwnerSource,
    planned: &[PlannedTask],
    task_identity_ids: &BTreeMap<String, i64>,
    revision_ids: &BTreeMap<String, i64>,
) -> Result<()> {
    for claim in carried_claims(owner, planned) {
        let task_identity_id = task_identity_ids
            .get(claim.identity_digest)
            .context("completion claim task identity was not materialized")?;
        let task_revision_id = revision_ids
            .get(claim.revision_digest)
            .context("completion claim revision was not materialized")?;
        conn.execute(
            "insert into task_completion_claims(project_id,task_identity_id,task_revision_id,completion_digest,state,created_at) values(?1,?2,?3,?4,'completed',current_timestamp)",
            params![
                snapshot.project_id,
                task_identity_id,
                task_revision_id,
                claim.digest,
            ],
        )?;
        let claim_id = conn.last_insert_rowid();
        for source in claim.evidence {
            conn.execute(
                "insert into task_completion_sources(project_id,task_completion_claim_id,source_kind,source_record_id,source_digest,created_at) values(?1,?2,?3,?4,?5,current_timestamp)",
                params![
                    snapshot.project_id,
                    claim_id,
                    source.kind,
                    source.id,
                    source.digest,
                ],
            )?;
        }
    }
    Ok(())
}

fn carried_claims<'a>(
    owner: &'a OwnerSource,
    planned: &'a [PlannedTask],
) -> Vec<CompletionClaim<'a>> {
    let mut claims = Vec::new();
    for (task, planned) in owner.tasks.iter().zip(planned) {
        if planned.ambiguity || task.status != TaskState::Completed {
            continue;
        }
        let structurally_complete = !task.checklists.is_empty()
            && task.checklists.iter().all(|checklist| {
                checklist.status == ChecklistState::Closed
                    && !checklist.items.is_empty()
                    && checklist.items.iter().all(|item| {
                        item.status == ChecklistItemState::Closed
                            || (item.status == ChecklistItemState::OutOfScope
                                && item.acceptance_ids.len() == 1)
                    })
            });
        if structurally_complete {
            push_claim(
                &mut claims,
                planned,
                "implementation",
                task.evidence
                    .iter()
                    .filter(|source| matches!(source.kind.as_str(), "implementation" | "coverage"))
                    .collect(),
            );
        }
        if !task.requirements.is_empty() {
            push_claim(
                &mut claims,
                planned,
                "validation",
                task.evidence
                    .iter()
                    .filter(|source| source.kind == "validation")
                    .collect(),
            );
        }
    }
    claims
}

fn push_claim<'a>(
    claims: &mut Vec<CompletionClaim<'a>>,
    planned: &'a PlannedTask,
    kind: &str,
    mut evidence: Vec<&'a EvidenceSource>,
) {
    if evidence.is_empty() {
        return;
    }
    evidence.sort_by(|left, right| left.digest.cmp(&right.digest));
    evidence.dedup_by(|left, right| left.digest == right.digest);
    let payload = CanonicalValue::object([
        (
            "revision_digest",
            CanonicalValue::string(planned.revision_digest.clone()),
        ),
        ("phase", CanonicalValue::Null),
        ("claim", CanonicalValue::string(kind)),
        ("result", CanonicalValue::string("carry")),
        (
            "evidence",
            CanonicalValue::Array(
                evidence
                    .iter()
                    .map(|source| CanonicalValue::string(source.digest.clone()))
                    .collect(),
            ),
        ),
    ]);
    claims.push(CompletionClaim {
        identity_digest: &planned.identity_digest,
        revision_digest: &planned.revision_digest,
        digest: domain_digest(b"AWB-COMPLETION-v1\0", &payload),
        evidence,
    });
}
