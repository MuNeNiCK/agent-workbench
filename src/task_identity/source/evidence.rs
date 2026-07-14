use anyhow::Result;
use rusqlite::{Connection, params};

use crate::identity::{CanonicalValue, domain_digest};

#[derive(Clone, Debug)]
pub(crate) struct EvidenceSource {
    pub(crate) id: i64,
    pub(crate) kind: String,
    pub(crate) digest: String,
}

pub(super) fn read(conn: &Connection, task_id: i64) -> Result<Vec<EvidenceSource>> {
    let mut result = implementation(conn, task_id)?;
    result.extend(coverage(conn, task_id)?);
    result.extend(validations(conn, task_id)?);
    result.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then(left.id.cmp(&right.id))
            .then(left.digest.cmp(&right.digest))
    });
    Ok(result)
}

fn implementation(conn: &Connection, task_id: i64) -> Result<Vec<EvidenceSource>> {
    let mut statement = conn.prepare(
        r#"
        select id,evidence_type,commit_sha,file_path,line_ref,symbol,artifact_path,note
        from implementation_evidence where task_id=?1 order by id
        "#,
    )?;
    statement
        .query_map(params![task_id], |row| {
            let id = row.get::<_, i64>(0)?;
            let value = CanonicalValue::object([
                ("id", CanonicalValue::string(id.to_string())),
                ("type", CanonicalValue::string(row.get::<_, String>(1)?)),
                ("commit", optional(row.get(2)?)),
                ("file", optional(row.get(3)?)),
                ("line", optional(row.get(4)?)),
                ("symbol", optional(row.get(5)?)),
                ("artifact", optional(row.get(6)?)),
                ("note", optional(row.get(7)?)),
            ]);
            Ok(source("implementation", id, value))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn coverage(conn: &Connection, task_id: i64) -> Result<Vec<EvidenceSource>> {
    let mut statement = conn.prepare(
        r#"
        select id,status,runtime_boundary_evidence,ux_boundary_evidence,
               lifecycle_boundary_evidence,tests_or_gates,missing_or_unverified
        from coverage_items where task_id=?1 and status='covered' order by id
        "#,
    )?;
    statement
        .query_map(params![task_id], |row| {
            let id = row.get::<_, i64>(0)?;
            let value = CanonicalValue::object([
                ("id", CanonicalValue::string(id.to_string())),
                ("status", CanonicalValue::string(row.get::<_, String>(1)?)),
                ("runtime", optional(row.get(2)?)),
                ("ux", optional(row.get(3)?)),
                ("lifecycle", optional(row.get(4)?)),
                ("tests", optional(row.get(5)?)),
                ("missing", optional(row.get(6)?)),
            ]);
            Ok(source("coverage", id, value))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn validations(conn: &Connection, task_id: i64) -> Result<Vec<EvidenceSource>> {
    let mut statement = conn.prepare(
        r#"
        select run.id,run.result,run.artifact_path,run.artifact_hash,run.notes,
               gate.id,gate.expected_result
        from validation_runs run
        join validation_gates gate on gate.id=run.validation_gate_id
        where run.task_id=?1 and run.result='pass'
        order by run.id
        "#,
    )?;
    statement
        .query_map(params![task_id], |row| {
            let id = row.get::<_, i64>(0)?;
            let value = CanonicalValue::object([
                ("id", CanonicalValue::string(id.to_string())),
                ("result", CanonicalValue::string(row.get::<_, String>(1)?)),
                ("artifact", optional(row.get(2)?)),
                ("artifact_hash", optional(row.get(3)?)),
                ("notes", optional(row.get(4)?)),
                (
                    "gate",
                    CanonicalValue::string(row.get::<_, i64>(5)?.to_string()),
                ),
                ("expected", CanonicalValue::string(row.get::<_, String>(6)?)),
            ]);
            Ok(source("validation", id, value))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn source(kind: &str, id: i64, reference: CanonicalValue) -> EvidenceSource {
    let digest = domain_digest(
        b"AWB-EVIDENCE-SOURCE-v1\0",
        &CanonicalValue::object([
            ("kind", CanonicalValue::string(kind)),
            ("value", reference.clone()),
        ]),
    );
    EvidenceSource {
        id,
        kind: kind.to_string(),
        digest,
    }
}

fn optional(value: Option<String>) -> CanonicalValue {
    value
        .map(CanonicalValue::string)
        .unwrap_or(CanonicalValue::Null)
}
