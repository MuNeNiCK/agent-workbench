use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use fs2::FileExt;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod operations;
mod storage;
pub(crate) mod transition;
pub use operations::*;
#[cfg(test)]
pub(crate) use operations::{apply_update_operation_inner, restore_update_operation_inner};
use storage::*;
#[cfg(test)]
pub(crate) use storage::{
    backup_dir, checkpoint, install_copy, ledger_path, sha256_file, sync_dir,
};

const BACKUP_DIR: &str = "update-backups";
const OPERATION_DIR: &str = "update-operations";

#[cfg(test)]
thread_local! {
    static SHARED_LOCK_BLOCKED_OBSERVER: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        std::cell::RefCell::new(None);
}

#[cfg(test)]
pub(crate) fn set_shared_lock_blocked_observer(observer: impl FnOnce() + 'static) {
    SHARED_LOCK_BLOCKED_OBSERVER.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(observer));
    });
}

#[cfg(test)]
fn notify_shared_lock_blocked() {
    let observer = SHARED_LOCK_BLOCKED_OBSERVER.with(|slot| slot.borrow_mut().take());
    if let Some(observer) = observer {
        observer();
    }
}

pub(crate) fn shared_writer_guard(root: &Path) -> Result<File> {
    let directory = normalized_root(root)?.join(crate::db::LEDGER_DIR);
    let lock = update_lock(&directory)?;
    #[cfg(test)]
    match FileExt::try_lock_shared(&lock) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            notify_shared_lock_blocked();
            FileExt::lock_shared(&lock)?;
        }
        Err(error) => return Err(error.into()),
    }
    #[cfg(not(test))]
    FileExt::lock_shared(&lock)?;
    Ok(lock)
}

pub(crate) fn exclusive_writer_guard(root: &Path) -> Result<File> {
    let directory = normalized_root(root)?.join(crate::db::LEDGER_DIR);
    let lock = update_lock(&directory)?;
    lock.lock_exclusive()?;
    Ok(lock)
}

#[derive(Debug, PartialEq, Eq)]
pub struct UpdateInspection {
    pub inspection_handle: String,
    pub current_identity: String,
    pub restorable_backups: Vec<String>,
    pub status: String,
    pub decision_choices: Vec<String>,
    pub preserved_capabilities: Vec<String>,
    pub next_actions: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct UpdateDecisionOutcome {
    pub inspection_handle: String,
    pub decision_handle: String,
    pub next_action: String,
    pub already_applied: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct UpdateAuthorityOutcome {
    pub authority_handle: String,
    pub next_action: String,
    pub already_recorded: bool,
}

#[derive(Clone, Copy, Debug)]
pub enum UpdateDecisionAuthority<'a> {
    ProjectEvent(i64),
    Recovery(&'a str),
}

fn shell_word(value: &str) -> String {
    if !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':' | b'/' | b'@')
        })
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[derive(Debug, PartialEq, Eq)]
pub struct UpdateRestoreOutcome {
    pub operation_handle: String,
    pub restored_identity: String,
    pub recovery_backup_identity: String,
    pub already_applied: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct UpdateApplyOutcome {
    pub operation_handle: String,
    pub source_identity: String,
    pub result_identity: String,
    pub backup_identity: String,
    pub already_applied: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UpdateBoundary {
    SourceRechecked,
    BackupDurable,
    ReceiptPrepared,
    BeforePublication,
    AfterPublication,
    CompletionDurable,
}

pub fn inspect_update(root: &Path) -> Result<UpdateInspection> {
    let root = normalized_root(root)?;
    let directory = root.join(crate::db::LEDGER_DIR);
    let lock = update_lock(&directory)?;
    lock.lock_exclusive()?;
    inspect_update_locked(&root)
}

fn inspect_update_locked(root: &Path) -> Result<UpdateInspection> {
    let ledger = ledger_path(root);
    let current_identity = ledger_state_identity(&ledger)?;
    let (restorable_backups, recovery_sources) = verified_update_backups(root)?;
    let conn = Connection::open_with_flags(&ledger, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let route = match transition::classify_update_route(&conn, root) {
        Ok(transition::UpdateRoute::UnsupportedSource) if !recovery_sources.is_empty() => {
            transition::UpdateRoute::RecoveryRequired
        }
        Ok(route) => route,
        Err(_) if !recovery_sources.is_empty() => transition::UpdateRoute::RecoveryRequired,
        Err(_) => transition::UpdateRoute::UnsupportedSource,
    };
    let backups = backup_dir(root);
    let selected_recovery = if matches!(route, transition::UpdateRoute::RecoveryRequired) {
        recorded_recovery_choice(root, &current_identity, &recovery_sources)?
    } else {
        None
    };
    let status = match &route {
        transition::UpdateRoute::Current => "current",
        transition::UpdateRoute::CoreNormalization { .. }
        | transition::UpdateRoute::RegisteredPath { .. }
        | transition::UpdateRoute::CurrentRepair { .. } => "ready_to_apply",
        transition::UpdateRoute::RecoveryRequired
            if recovery_sources.len() > 1 && selected_recovery.is_none() =>
        {
            "owner_input_required"
        }
        transition::UpdateRoute::RecoveryRequired => "recovery_required",
        transition::UpdateRoute::UnsupportedSource => "owner_input_required",
    }
    .to_string();
    let inspection_handle = inspection_handle(&current_identity, &status);
    let decision_choices = if status == "owner_input_required" {
        recovery_sources
            .iter()
            .map(|backup| format!("restore:{backup}"))
            .collect()
    } else {
        Vec::new()
    };
    let preserved_capabilities = transition::preserved_capability_classes(&conn, &route)?;
    let next_actions = match &route {
        transition::UpdateRoute::Current => vec!["agent-workbench status".to_string()],
        transition::UpdateRoute::CoreNormalization { .. }
        | transition::UpdateRoute::RegisteredPath { .. }
        | transition::UpdateRoute::CurrentRepair { .. } => vec![format!(
            "agent-workbench update apply {inspection_handle} --expected-current {current_identity} --idempotency-key update-{}",
            short_identity(&current_identity)
        )],
        transition::UpdateRoute::RecoveryRequired if status == "owner_input_required" => {
            let mut actions = Vec::new();
            let mut needs_authority_input = false;
            for backup in &recovery_sources {
                match recovery_authority(&backups.join(format!("{backup}.sqlite")))? {
                    Some((authority, reason)) => actions.push(format!(
                        "agent-workbench update decide {inspection_handle} --choice restore:{backup} --authority {authority} --reason {} --expected-current {current_identity}",
                        shell_word(&reason)
                    )),
                    None => needs_authority_input = true,
                }
            }
            if needs_authority_input {
                actions.push("agent-workbench update authority-record --help".to_string());
            }
            actions
        }
        transition::UpdateRoute::RecoveryRequired => {
            let selected = selected_recovery
                .as_deref()
                .and_then(|choice| choice.strip_prefix("restore:"));
            let mut actions = recovery_sources
                .iter()
                .filter(|backup| selected.is_none_or(|selected| selected == backup.as_str()))
                .map(|backup| {
                    format!(
                        "agent-workbench update restore --backup {backup} --expected-current {current_identity} --idempotency-key restore-{}",
                        short_identity(backup)
                    )
                })
                .collect::<Vec<_>>();
            if actions.is_empty() {
                actions.push(
                    "provide a verified project-owned recovery source, then run agent-workbench update inspect"
                        .to_string(),
                );
            }
            actions
        }
        transition::UpdateRoute::UnsupportedSource => vec![
            "provide a verified project-owned recovery source, then run agent-workbench update inspect"
                .to_string(),
        ],
    };
    Ok(UpdateInspection {
        inspection_handle,
        current_identity,
        restorable_backups,
        status,
        decision_choices,
        preserved_capabilities,
        next_actions,
    })
}

pub(crate) fn connection_requires_update(conn: &Connection) -> Result<bool> {
    Ok(
        match transition::classify_update_route(conn, Path::new(".")) {
            Ok(route) => route.requires_change(),
            Err(_) => true,
        },
    )
}

fn verified_update_backups(root: &Path) -> Result<(Vec<String>, Vec<String>)> {
    let mut restorable = Vec::new();
    let mut recovery_sources = Vec::new();
    let backups = backup_dir(root);
    if !backups.is_dir() {
        return Ok((restorable, recovery_sources));
    }
    for entry in fs::read_dir(&backups)? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(handle) = name.strip_suffix(".sqlite") else {
            continue;
        };
        if valid_handle(handle)
            && sha256_file(&path).is_ok_and(|identity| identity == handle)
            && verify_restorable_ledger(&path, root).is_ok()
        {
            restorable.push(handle.to_string());
            let backup = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            if matches!(
                transition::classify_storage_header(&backup)?,
                transition::StorageHeader::Full { .. }
            ) {
                recovery_sources.push(handle.to_string());
            }
        }
    }
    restorable.sort();
    recovery_sources.sort();
    Ok((restorable, recovery_sources))
}

pub fn decide_update(
    root: &Path,
    inspection_handle: &str,
    choice: &str,
    authority_event_id: i64,
    reason: &str,
    expected_current: &str,
) -> Result<UpdateDecisionOutcome> {
    decide_update_with_authority(
        root,
        inspection_handle,
        choice,
        UpdateDecisionAuthority::ProjectEvent(authority_event_id),
        reason,
        expected_current,
    )
}

pub struct UpdateRecoveryAuthorityInput<'a> {
    pub inspection_handle: &'a str,
    pub choice: &'a str,
    pub statement: &'a str,
    pub provenance: &'a str,
    pub provenance_ref: &'a str,
    pub expected_current: &'a str,
    pub idempotency_key: &'a str,
}

pub fn record_update_recovery_authority(
    root: &Path,
    input: UpdateRecoveryAuthorityInput<'_>,
) -> Result<UpdateAuthorityOutcome> {
    let UpdateRecoveryAuthorityInput {
        inspection_handle,
        choice,
        statement,
        provenance,
        provenance_ref,
        expected_current,
        idempotency_key,
    } = input;
    require_prefixed_handle(inspection_handle, "update_inspection_", "inspection handle")?;
    require_token(choice, "choice")?;
    require_handle(expected_current, "expected current identity")?;
    require_token(idempotency_key, "idempotency key")?;
    if provenance != "user_instruction" {
        bail!("update recovery authority provenance must be user_instruction");
    }
    if statement.trim().is_empty() || provenance_ref.trim().is_empty() {
        bail!("update recovery authority statement and provenance reference must not be empty");
    }
    let root = normalized_root(root)?;
    let directory = root.join(crate::db::LEDGER_DIR);
    let lock = update_lock(&directory)?;
    lock.lock_exclusive()?;
    let observed = ledger_state_identity(&ledger_path(&root))?;
    if observed != expected_current {
        bail!(
            "current identity changed: expected {expected_current}, found {observed}; run agent-workbench update inspect again"
        );
    }
    let authority_handle = digest_handle(
        "update_authority_",
        &[
            inspection_handle,
            choice,
            statement,
            provenance,
            provenance_ref,
            expected_current,
        ],
    );
    if let Some(journal) = journal_for_key(&root, idempotency_key)? {
        if journal.action == "authority_record"
            && journal.inspection_handle == inspection_handle
            && journal.source_identity == expected_current
            && journal.target_identity.as_deref() == Some(choice)
            && journal.result_identity.as_deref() == Some(authority_handle.as_str())
            && journal.recovery_authority_handle.as_deref() == Some(authority_handle.as_str())
            && journal.authority_provenance.as_deref() == Some(provenance)
            && journal.authority_provenance_ref.as_deref() == Some(provenance_ref)
            && journal.reason.as_deref() == Some(statement)
        {
            return Ok(UpdateAuthorityOutcome {
                authority_handle: authority_handle.clone(),
                next_action: format!(
                    "agent-workbench update decide {inspection_handle} --choice {choice} --recovery-authority {authority_handle} --reason {statement} --expected-current {expected_current}"
                ),
                already_recorded: true,
            });
        }
        bail!("idempotency key is already bound to a different update request");
    }
    let inspection = inspect_update_locked(&root)?;
    if inspection.inspection_handle != inspection_handle
        || !inspection
            .decision_choices
            .iter()
            .any(|item| item == choice)
    {
        bail!("inspection or recovery choice changed; run agent-workbench update inspect again");
    }
    let backup_handle = choice
        .strip_prefix("restore:")
        .context("recovery authority choice must select a restore source")?;
    require_handle(backup_handle, "recovery choice")?;
    let operation_handle = digest_handle(
        "update_operation_",
        &["authority_record", inspection_handle, expected_current],
    );
    write_journal(
        &root,
        &UpdateOperationJournal {
            operation_handle,
            action: "authority_record".to_string(),
            inspection_handle: inspection_handle.to_string(),
            source_identity: expected_current.to_string(),
            target_identity: Some(choice.to_string()),
            result_identity: Some(authority_handle.clone()),
            backup_handle: backup_handle.to_string(),
            idempotency_key: idempotency_key.to_string(),
            status: "completed".to_string(),
            authority_event_id: None,
            recovery_authority_handle: Some(authority_handle.clone()),
            authority_provenance: Some(provenance.to_string()),
            authority_provenance_ref: Some(provenance_ref.to_string()),
            reason: Some(statement.to_string()),
        },
    )?;
    Ok(UpdateAuthorityOutcome {
        authority_handle: authority_handle.clone(),
        next_action: format!(
            "agent-workbench update decide {inspection_handle} --choice {choice} --recovery-authority {authority_handle} --reason {statement} --expected-current {expected_current}"
        ),
        already_recorded: false,
    })
}

pub fn decide_update_with_authority(
    root: &Path,
    inspection_handle: &str,
    choice: &str,
    authority: UpdateDecisionAuthority<'_>,
    reason: &str,
    expected_current: &str,
) -> Result<UpdateDecisionOutcome> {
    require_prefixed_handle(inspection_handle, "update_inspection_", "inspection handle")?;
    require_token(choice, "choice")?;
    if matches!(authority, UpdateDecisionAuthority::ProjectEvent(id) if id <= 0) {
        bail!("authority event must be a positive project reference");
    }
    if reason.trim().is_empty() {
        bail!("decision reason must not be empty");
    }
    require_handle(expected_current, "expected current identity")?;
    let root = normalized_root(root)?;
    let directory = root.join(crate::db::LEDGER_DIR);
    let lock = update_lock(&directory)?;
    lock.lock_exclusive()?;
    let observed = ledger_state_identity(&ledger_path(&root))?;
    if observed != expected_current {
        bail!(
            "current identity changed: expected {expected_current}, found {observed}; run agent-workbench update inspect again"
        );
    }
    let authority_identity = match authority {
        UpdateDecisionAuthority::ProjectEvent(id) => format!("event:{id}"),
        UpdateDecisionAuthority::Recovery(handle) => {
            require_prefixed_handle(handle, "update_authority_", "recovery authority")?;
            format!("recovery:{handle}")
        }
    };
    let decision_handle = digest_handle(
        "update_decision_",
        &[
            inspection_handle,
            choice,
            &authority_identity,
            reason,
            expected_current,
        ],
    );
    let operation_handle = digest_handle(
        "update_operation_",
        &["decide", inspection_handle, expected_current],
    );
    if let Some(journal) = decision_for_source(&root, expected_current)? {
        if journal.operation_handle == operation_handle
            && journal.target_identity.as_deref() == Some(choice)
            && journal.result_identity.as_deref() == Some(decision_handle.as_str())
        {
            return Ok(UpdateDecisionOutcome {
                inspection_handle: inspection_handle.to_string(),
                decision_handle,
                next_action: "agent-workbench update inspect".to_string(),
                already_applied: true,
            });
        }
        bail!(
            "an update decision is already recorded for this source; run agent-workbench update inspect again"
        );
    }
    let inspection = inspect_update_locked(&root)?;
    if inspection.inspection_handle != inspection_handle {
        bail!("inspection is stale; run agent-workbench update inspect again");
    }
    if !inspection
        .decision_choices
        .iter()
        .any(|item| item == choice)
    {
        bail!(
            "the inspection does not offer that decision; run agent-workbench update inspect again"
        );
    }

    if let Some(backup_handle) = choice.strip_prefix("restore:") {
        require_handle(backup_handle, "recovery choice")?;
        let (authority_event_id, recovery_authority_handle) = match authority {
            UpdateDecisionAuthority::ProjectEvent(authority_event_id) => {
                let requested = backup_dir(&root).join(format!("{backup_handle}.sqlite"));
                if !recovery_authority_exists(&requested, authority_event_id, reason)? {
                    bail!(
                        "authority event does not belong to the selected project recovery source"
                    );
                }
                (Some(authority_event_id), None)
            }
            UpdateDecisionAuthority::Recovery(handle) => {
                let authority_journal = journal_for_result(&root, handle)?
                    .context("recovery authority handle not found")?;
                if authority_journal.action != "authority_record"
                    || authority_journal.inspection_handle != inspection_handle
                    || authority_journal.source_identity != expected_current
                    || authority_journal.target_identity.as_deref() != Some(choice)
                    || authority_journal.reason.as_deref() != Some(reason)
                    || authority_journal.recovery_authority_handle.as_deref() != Some(handle)
                    || authority_journal.authority_provenance.as_deref() != Some("user_instruction")
                    || authority_journal
                        .authority_provenance_ref
                        .as_deref()
                        .is_none_or(str::is_empty)
                {
                    bail!("recovery authority does not match the current update choice and reason");
                }
                (None, Some(handle.to_string()))
            }
        };
        write_journal(
            &root,
            &UpdateOperationJournal {
                operation_handle,
                action: "decide".to_string(),
                inspection_handle: inspection_handle.to_string(),
                source_identity: expected_current.to_string(),
                target_identity: Some(choice.to_string()),
                result_identity: Some(decision_handle.clone()),
                backup_handle: backup_handle.to_string(),
                idempotency_key: decision_handle.clone(),
                status: "completed".to_string(),
                authority_event_id,
                recovery_authority_handle,
                authority_provenance: None,
                authority_provenance_ref: None,
                reason: Some(reason.to_string()),
            },
        )?;
        return Ok(UpdateDecisionOutcome {
            inspection_handle: inspection_handle.to_string(),
            decision_handle,
            next_action: "agent-workbench update inspect".to_string(),
            already_applied: false,
        });
    }

    let UpdateDecisionAuthority::ProjectEvent(authority_event_id) = authority else {
        bail!("recovery authority may only select a verified restore source");
    };
    record_update_decision(
        &root,
        &operation_handle,
        inspection_handle,
        expected_current,
        &decision_handle,
        choice,
        authority_event_id,
        reason,
    )?;
    write_journal(
        &root,
        &UpdateOperationJournal {
            operation_handle,
            action: "decide".to_string(),
            inspection_handle: inspection_handle.to_string(),
            source_identity: expected_current.to_string(),
            target_identity: Some(choice.to_string()),
            result_identity: Some(decision_handle.clone()),
            backup_handle: String::new(),
            idempotency_key: decision_handle.clone(),
            status: "completed".to_string(),
            authority_event_id: Some(authority_event_id),
            recovery_authority_handle: None,
            authority_provenance: None,
            authority_provenance_ref: None,
            reason: Some(reason.to_string()),
        },
    )?;
    Ok(UpdateDecisionOutcome {
        inspection_handle: inspection_handle.to_string(),
        decision_handle,
        next_action: "agent-workbench update inspect".to_string(),
        already_applied: false,
    })
}
