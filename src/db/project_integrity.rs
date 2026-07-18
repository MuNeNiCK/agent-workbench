use std::path::Path;

use rusqlite::{Connection, OptionalExtension, params};

use super::project::table_exists;
use super::{
    IntegrityPredicateStatus, ProjectIntegrityStatus, SCHEMA_VERSION, default_ledger_path,
    open_ledger,
};

const CODES: [(&str, &str); 4] = [
    ("GI-001", "storage_unreadable"),
    ("GI-002", "schema_state_unsupported"),
    ("GI-003", "project_identity_unresolvable"),
    ("GI-004", "validation_project_link_invalid"),
];

pub(super) struct IntegrityEvaluation {
    pub(super) status: ProjectIntegrityStatus,
    pub(super) connection: Option<Connection>,
    pub(super) schema_version: Option<i64>,
    pub(super) diagnostic_error: Option<String>,
}

pub(super) fn evaluate_project_integrity(root: &Path) -> IntegrityEvaluation {
    let ledger_path = default_ledger_path(root);
    let conn = match open_ledger(&ledger_path) {
        Ok(conn) => conn,
        Err(error) => {
            return blocked(0, format!("ledger open failed: {error}"), None, None);
        }
    };

    let integrity = conn
        .query_row("pragma integrity_check(1)", [], |row| {
            row.get::<_, String>(0)
        })
        .map_err(anyhow::Error::from)
        .and_then(|value| {
            if value == "ok" {
                Ok(())
            } else {
                anyhow::bail!("SQLite integrity check returned {value}")
            }
        });
    if let Err(error) = integrity {
        return blocked(0, error.to_string(), Some(conn), None);
    }
    let has_schema_metadata = table_exists(&conn, "schema_migrations").unwrap_or(false);
    let schema_version = has_schema_metadata
        .then(|| {
            conn.query_row(
                "select version from schema_migrations order by version desc limit 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()
        })
        .transpose()
        .ok()
        .flatten()
        .flatten();
    if schema_version.is_none() {
        return blocked(
            0,
            "required schema metadata is absent or undecodable".to_string(),
            Some(conn),
            None,
        );
    }
    let version = schema_version.unwrap();
    if !(1..=SCHEMA_VERSION).contains(&version) {
        return blocked(
            1,
            format!("schema version {version} is outside supported range 1..={SCHEMA_VERSION}"),
            Some(conn),
            Some(version),
        );
    }
    match failed_migration_journal(&conn) {
        Ok(Some(failure)) => {
            let next_action = match failure.backup_handle.as_deref() {
                Some(backup_handle) => format!(
                    "restore the project-owned pre-migration backup {backup_handle}, then run agent-workbench status"
                ),
                None => "external-restore-required: no verified pre-migration backup is recorded; then run agent-workbench status".to_string(),
            };
            return blocked_with_next(
                1,
                format!(
                    "atomic migration journal proves {} application",
                    failure.status
                ),
                Some(conn),
                Some(version),
                Some(next_action),
            );
        }
        Ok(None) => {}
        Err(error) => {
            return blocked(
                0,
                format!("required migration metadata cannot be decoded: {error}"),
                Some(conn),
                Some(version),
            );
        }
    }

    let canonical_root = root
        .canonicalize()
        .unwrap_or_else(|_| root.to_path_buf())
        .display()
        .to_string();
    let project_matches = conn
        .query_row(
            "select count(*) from projects where root_path = ?1",
            params![canonical_root],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    if project_matches != 1 {
        return blocked(
            2,
            format!("canonical root resolves to {project_matches} project identities"),
            Some(conn),
            Some(version),
        );
    }

    let validation_links = match crate::doctor::diagnose_validation_links(root) {
        Ok(diagnosis) => diagnosis,
        Err(error) => {
            return diagnostic_failure(conn, version, error.to_string());
        }
    };
    if validation_links.runs.is_empty() {
        match super::project_requires_update(&conn) {
            Ok(true) => {
                let next = crate::update::current_identity(root).map_or_else(
                    |_| "agent-workbench update inspect".to_string(),
                    |identity| {
                        format!("agent-workbench update apply --expected-current {identity}")
                    },
                );
                return blocked_with_next(
                    1,
                    "project state requires an explicit update".to_string(),
                    Some(conn),
                    Some(version),
                    Some(next),
                );
            }
            Ok(false) => {}
            Err(error) => return blocked(1, error.to_string(), Some(conn), Some(version)),
        }
    }
    match validation_links.runs.len() {
        0 => IntegrityEvaluation {
            status: ProjectIntegrityStatus {
                result: "clear".to_string(),
                predicates: vec![
                    clear(
                        0,
                        "ledger opens, integrity is ok, and schema metadata decodes",
                    ),
                    clear(1, &format!("schema version {version} is supported")),
                    clear(2, "canonical root resolves to exactly one project identity"),
                    clear(3, "validation run project links are valid"),
                ],
            },
            connection: Some(conn),
            schema_version: Some(version),
            diagnostic_error: None,
        },
        count => blocked(
            3,
            format!("{count} validation runs have invalid project links"),
            Some(conn),
            Some(version),
        ),
    }
}

struct MigrationJournalFailure {
    status: String,
    backup_handle: Option<String>,
}

fn failed_migration_journal(
    conn: &Connection,
) -> rusqlite::Result<Option<MigrationJournalFailure>> {
    let exists: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='table' and name='migration_apply_journal')",
        [],
        |row| row.get(0),
    )?;
    if !exists {
        return Ok(None);
    }
    conn.query_row(
        "select status, backup_handle from migration_apply_journal where status in ('incomplete','failed') order by id limit 1",
        [],
        |row| {
            Ok(MigrationJournalFailure {
                status: row.get(0)?,
                backup_handle: row.get(1)?,
            })
        },
    )
    .optional()
}

fn clear(index: usize, evidence: &str) -> IntegrityPredicateStatus {
    let (code, name) = CODES[index];
    IntegrityPredicateStatus {
        code: code.to_string(),
        name: name.to_string(),
        result: "clear".to_string(),
        evidence: evidence.to_string(),
        next_action: None,
    }
}

fn blocked(
    index: usize,
    evidence: String,
    connection: Option<Connection>,
    schema_version: Option<i64>,
) -> IntegrityEvaluation {
    blocked_with_next(index, evidence, connection, schema_version, None)
}

fn blocked_with_next(
    index: usize,
    evidence: String,
    connection: Option<Connection>,
    schema_version: Option<i64>,
    selected_next_action: Option<String>,
) -> IntegrityEvaluation {
    let predicates = CODES
        .iter()
        .enumerate()
        .map(|(position, (code, name))| {
            if position < index {
                clear(position, "prerequisite cleared")
            } else if position == index {
                IntegrityPredicateStatus {
                    code: (*code).to_string(),
                    name: (*name).to_string(),
                    result: "blocked".to_string(),
                    evidence: evidence.clone(),
                    next_action: Some(
                        selected_next_action
                            .clone()
                            .unwrap_or_else(|| next_action(index)),
                    ),
                }
            } else {
                IntegrityPredicateStatus {
                    code: (*code).to_string(),
                    name: (*name).to_string(),
                    result: "not_evaluated".to_string(),
                    evidence: format!("{} did not clear", CODES[index].0),
                    next_action: None,
                }
            }
        })
        .collect();
    IntegrityEvaluation {
        status: ProjectIntegrityStatus {
            result: "blocked".to_string(),
            predicates,
        },
        connection,
        schema_version,
        diagnostic_error: None,
    }
}

fn diagnostic_failure(conn: Connection, version: i64, error: String) -> IntegrityEvaluation {
    IntegrityEvaluation {
        status: ProjectIntegrityStatus {
            result: "indeterminate".to_string(),
            predicates: vec![
                clear(
                    0,
                    "ledger opens, integrity is ok, and schema metadata decodes",
                ),
                clear(1, &format!("schema version {version} is supported")),
                clear(2, "canonical root resolves to exactly one project identity"),
                IntegrityPredicateStatus {
                    code: CODES[3].0.to_string(),
                    name: CODES[3].1.to_string(),
                    result: "not_evaluated".to_string(),
                    evidence: "validation-link diagnosis did not produce closed evidence"
                        .to_string(),
                    next_action: None,
                },
            ],
        },
        connection: Some(conn),
        schema_version: Some(version),
        diagnostic_error: Some(error),
    }
}

fn next_action(index: usize) -> String {
    match index {
        0 => "restore the verified project backup, then run agent-workbench status".to_string(),
        1 => format!(
            "install an agent-workbench version supporting this schema (current maximum {SCHEMA_VERSION}), then run agent-workbench status"
        ),
        2 => "restore or select the exact project identity, then run agent-workbench status"
            .to_string(),
        3 => "agent-workbench doctor validation-links".to_string(),
        _ => unreachable!(),
    }
}
