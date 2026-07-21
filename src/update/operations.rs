use super::*;

pub fn restore_update_operation(
    root: &Path,
    backup_handle: &str,
    expected_current: &str,
    idempotency_key: &str,
) -> Result<UpdateRestoreOutcome> {
    restore_update_operation_inner(
        root,
        backup_handle,
        expected_current,
        idempotency_key,
        &mut |_| Ok(()),
    )
}

pub(crate) fn restore_update_operation_inner<F>(
    root: &Path,
    backup_handle: &str,
    expected_current: &str,
    idempotency_key: &str,
    observer: &mut F,
) -> Result<UpdateRestoreOutcome>
where
    F: FnMut(UpdateBoundary) -> Result<()>,
{
    require_handle(backup_handle, "backup handle")?;
    require_handle(expected_current, "expected current identity")?;
    require_token(idempotency_key, "idempotency key")?;
    let root = normalized_root(root)?;
    let directory = root.join(crate::db::LEDGER_DIR);
    let ledger = ledger_path(&root);
    let backups = backup_dir(&root);
    fs::create_dir_all(&backups)?;
    let lock = update_lock(&directory)?;
    lock.lock_exclusive()?;
    let operation_handle = digest_handle(
        "update_operation_",
        &["restore", backup_handle, expected_current, idempotency_key],
    );
    if let Some(journal) = journal_for_key(&root, idempotency_key)? {
        ensure_journal_payload(
            &journal,
            &operation_handle,
            "restore",
            "",
            expected_current,
            backup_handle,
        )?;
        if journal.status == "completed" {
            return restore_outcome_from_journal(&journal, true);
        }
    }
    let inspection = inspect_update_locked(&root)?;
    if inspection.status == "owner_input_required" {
        bail!(
            "a recovery source choice is required\n{}",
            inspection_actions(&inspection.next_actions)
        );
    }
    if let Some(decision) = decision_for_source(&root, expected_current)?
        && let Some(selected) = decision
            .target_identity
            .as_deref()
            .and_then(|choice| choice.strip_prefix("restore:"))
        && selected != backup_handle
    {
        bail!(
            "backup does not match the recorded recovery decision\n{}",
            inspection_actions(&inspection.next_actions)
        );
    }

    let requested = backups.join(format!("{backup_handle}.sqlite"));
    if sha256_file(&requested)? != backup_handle {
        bail!("backup handle does not match the backup content");
    }
    verify_restorable_ledger(&requested, &root)?;
    let requested_semantics = semantic_ledger_identity(&requested)?;
    let observed = ledger_state_identity(&ledger)?;
    let existing = journal_for_key(&root, idempotency_key)?;
    if let Some(mut journal) = existing.clone()
        && let Some(result_identity) = restore_publication_result(&ledger, &journal)?
    {
        journal.result_identity = Some(result_identity);
        record_restore_in_ledger(&root, &ledger, &journal)?;
        checkpoint(&ledger)?;
        journal.status = "completed".to_string();
        write_journal(&root, &journal)?;
        return restore_outcome_from_journal(&journal, false);
    }
    if observed != expected_current && existing.is_none() {
        bail!(
            "current identity changed: expected {expected_current}, found {observed}\n{}",
            inspection_actions(&inspection.next_actions)
        );
    }
    observer(UpdateBoundary::SourceRechecked)?;
    if sha256_file(&ledger).ok().as_deref() == Some(backup_handle)
        && let Some(mut journal) = existing
    {
        record_restore_in_ledger(&root, &ledger, &journal)?;
        checkpoint(&ledger)?;
        journal.result_identity = Some(sha256_file(&ledger)?);
        journal.status = "completed".to_string();
        write_journal(&root, &journal)?;
        return restore_outcome_from_journal(&journal, false);
    }
    if observed != expected_current {
        let inspection = inspect_update_locked(&root)?;
        bail!(
            "current identity changed after the restore was prepared and no published restore result is present: expected {expected_current}, found {observed}\n{}",
            inspection_actions(&inspection.next_actions)
        );
    }

    checkpoint_restore_source(&ledger)?;
    let source_file_identity = sha256_file(&ledger)?;
    let recovery_path = backups.join(format!("{source_file_identity}.sqlite"));
    if recovery_path.exists() {
        if sha256_file(&recovery_path)? != source_file_identity {
            bail!("existing recovery backup does not match its content-addressed name");
        }
    } else {
        install_copy(&ledger, &recovery_path, &source_file_identity)?;
    }
    observer(UpdateBoundary::BackupDurable)?;
    let mut journal = UpdateOperationJournal {
        operation_handle: operation_handle.clone(),
        action: "restore".to_string(),
        inspection_handle: String::new(),
        source_identity: expected_current.to_string(),
        target_identity: Some(backup_handle.to_string()),
        result_identity: None,
        backup_handle: source_file_identity,
        idempotency_key: idempotency_key.to_string(),
        status: "prepared".to_string(),
        authority_event_id: None,
        recovery_authority_handle: None,
        authority_provenance: None,
        authority_provenance_ref: None,
        reason: None,
    };
    journal.result_identity = Some(restore_result_identity(&journal)?);
    write_journal(&root, &journal)?;
    observer(UpdateBoundary::ReceiptPrepared)?;

    let staged = directory.join(format!("restore-{operation_handle}.tmp"));
    remove_staged(&staged)?;
    install_copy(&requested, &staged, backup_handle)?;
    verify_restorable_ledger(&staged, &root)?;
    if semantic_ledger_identity(&staged)? != requested_semantics {
        bail!("restored project does not match the verified semantic backup");
    }
    record_restore_publication(&staged, &journal)?;
    checkpoint(&staged)?;
    observer(UpdateBoundary::BeforePublication)?;
    fs::rename(&staged, &ledger)?;
    sync_dir(&directory)?;
    observer(UpdateBoundary::AfterPublication)?;
    verify_restorable_ledger(&ledger, &root)?;
    restore_publication_result(&ledger, &journal)?
        .context("published restore result lineage is missing")?;
    record_restore_in_ledger(&root, &ledger, &journal)?;
    checkpoint(&ledger)?;
    journal.status = "completed".to_string();
    write_journal(&root, &journal)?;
    observer(UpdateBoundary::CompletionDurable)?;
    restore_outcome_from_journal(&journal, false)
}

pub fn restore_update(
    root: &Path,
    backup_handle: &str,
    expected_current: &str,
) -> Result<UpdateRestoreOutcome> {
    let key = format!("legacy-restore-{backup_handle}-{expected_current}");
    restore_update_operation(root, backup_handle, expected_current, &key)
}

pub fn apply_update_operation(
    root: &Path,
    inspection_handle: &str,
    expected_current: &str,
    idempotency_key: &str,
) -> Result<UpdateApplyOutcome> {
    apply_update_operation_inner(
        root,
        inspection_handle,
        expected_current,
        idempotency_key,
        &mut |_| Ok(()),
    )
}

pub(crate) fn apply_update_operation_inner<F>(
    root: &Path,
    inspection_handle: &str,
    expected_current: &str,
    idempotency_key: &str,
    observer: &mut F,
) -> Result<UpdateApplyOutcome>
where
    F: FnMut(UpdateBoundary) -> Result<()>,
{
    require_prefixed_handle(inspection_handle, "update_inspection_", "inspection handle")?;
    require_handle(expected_current, "expected current identity")?;
    require_token(idempotency_key, "idempotency key")?;
    let root = normalized_root(root)?;
    let directory = root.join(crate::db::LEDGER_DIR);
    let ledger = ledger_path(&root);
    let backups = backup_dir(&root);
    fs::create_dir_all(&backups)?;
    let lock = update_lock(&directory)?;
    lock.lock_exclusive()?;
    let operation_handle = digest_handle(
        "update_operation_",
        &[
            "apply",
            inspection_handle,
            expected_current,
            idempotency_key,
        ],
    );
    if let Some(mut journal) = journal_for_key(&root, idempotency_key)? {
        ensure_journal_payload(
            &journal,
            &operation_handle,
            "apply",
            inspection_handle,
            expected_current,
            "",
        )?;
        if journal.status == "completed" {
            return apply_outcome_from_journal(&journal, true);
        }
        if update_operation_exists(&ledger, &operation_handle)? {
            complete_apply_receipt(&ledger, &operation_handle)?;
            checkpoint(&ledger)?;
            journal.result_identity = Some(sha256_file(&ledger)?);
            journal.status = "completed".to_string();
            write_journal(&root, &journal)?;
            return apply_outcome_from_journal(&journal, false);
        }
    }

    let observed = ledger_state_identity(&ledger)?;
    if observed != expected_current {
        let inspection = inspect_update_locked(&root)?;
        bail!(
            "current identity changed: expected {expected_current}, found {observed}\n{}",
            inspection_actions(&inspection.next_actions)
        );
    }
    observer(UpdateBoundary::SourceRechecked)?;
    let inspection = inspect_update_locked(&root)?;
    if inspection.inspection_handle != inspection_handle {
        bail!(
            "inspection is stale\n{}",
            inspection_actions(&inspection.next_actions)
        );
    }
    if inspection.status == "recovery_required" {
        bail!(
            "a verified recovery source is required before update can be applied\n{}",
            inspection_actions(&inspection.next_actions)
        );
    }
    if inspection.status != "ready_to_apply" {
        bail!(
            "the inspected project has no applicable update\n{}",
            inspection_actions(&inspection.next_actions)
        );
    }
    checkpoint(&ledger)?;
    let source_file_identity = sha256_file(&ledger)?;
    let backup = backups.join(format!("{source_file_identity}.sqlite"));
    if !backup.exists() {
        install_copy(&ledger, &backup, &source_file_identity)?;
    }
    observer(UpdateBoundary::BackupDurable)?;
    let staged = directory.join(format!("update-{operation_handle}.tmp"));
    remove_staged(&staged)?;
    install_copy(&ledger, &staged, &source_file_identity)?;
    let target_identity = digest_handle("update_target_", &[inspection_handle, expected_current]);
    let update_result = (|| -> Result<()> {
        let conn = Connection::open(&staged)?;
        conn.pragma_update(None, "foreign_keys", true)?;
        let route = transition::classify_update_route(&conn, &root)?;
        transition::apply_update_route(&conn, &route, &root)?;
        crate::db::ensure_project(&conn, &root)?;
        crate::db::sync_agents_md_authority(&conn, &root)?;
        crate::db::sync_commit_message_policy(&conn)?;
        let remaining = crate::db::pending_update_changes(&conn)?;
        if !remaining.is_empty() {
            bail!("staged update remains incomplete: {}", remaining.join(","));
        }
        record_prepared_apply(
            &conn,
            &operation_handle,
            inspection_handle,
            expected_current,
            &source_file_identity,
            &target_identity,
            idempotency_key,
        )?;
        drop(conn);
        verify_restorable_ledger(&staged, &root)
    })();
    if let Err(error) = update_result {
        let _ = fs::remove_file(&staged);
        return Err(error.context("explicit update failed; original project state was preserved"));
    }
    let mut journal = UpdateOperationJournal {
        operation_handle: operation_handle.clone(),
        action: "apply".to_string(),
        inspection_handle: inspection_handle.to_string(),
        source_identity: expected_current.to_string(),
        target_identity: Some(target_identity),
        result_identity: None,
        backup_handle: source_file_identity,
        idempotency_key: idempotency_key.to_string(),
        status: "prepared".to_string(),
        authority_event_id: None,
        recovery_authority_handle: None,
        authority_provenance: None,
        authority_provenance_ref: None,
        reason: None,
    };
    write_journal(&root, &journal)?;
    observer(UpdateBoundary::ReceiptPrepared)?;
    observer(UpdateBoundary::BeforePublication)?;
    fs::rename(&staged, &ledger)?;
    sync_dir(&directory)?;
    observer(UpdateBoundary::AfterPublication)?;
    complete_apply_receipt(&ledger, &operation_handle)?;
    checkpoint(&ledger)?;
    journal.result_identity = Some(sha256_file(&ledger)?);
    journal.status = "completed".to_string();
    write_journal(&root, &journal)?;
    observer(UpdateBoundary::CompletionDurable)?;
    apply_outcome_from_journal(&journal, false)
}

pub fn apply_update(root: &Path, expected_current: &str) -> Result<UpdateApplyOutcome> {
    require_handle(expected_current, "expected current identity")?;
    let root = normalized_root(root)?;
    let key = format!("legacy-update-{expected_current}");
    if let Some(journal) = journal_for_key(&root, &key)? {
        return apply_update_operation(&root, &journal.inspection_handle, expected_current, &key);
    }
    let inspection = inspect_update(&root)?;
    if inspection.status == "current" {
        if inspection.current_identity != expected_current {
            bail!(
                "current identity changed: expected {expected_current}, found {}\n{}",
                inspection.current_identity,
                inspection_actions(&inspection.next_actions)
            );
        }
        return Ok(UpdateApplyOutcome {
            operation_handle: digest_handle(
                "update_operation_",
                &[
                    "apply",
                    &inspection.inspection_handle,
                    expected_current,
                    "legacy-current",
                ],
            ),
            source_identity: expected_current.to_string(),
            result_identity: expected_current.to_string(),
            backup_identity: expected_current.to_string(),
            already_applied: true,
        });
    }
    apply_update_operation(&root, &inspection.inspection_handle, expected_current, &key)
}
