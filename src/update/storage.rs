use super::*;
use std::collections::BTreeMap;

const RESTORE_PUBLICATION_SQL: &str = r#"
create table if not exists restore_publications (
    operation_handle text primary key,
    source_identity text not null,
    target_backup_identity text not null,
    recovery_backup_identity text not null,
    result_identity text not null,
    idempotency_key text not null unique,
    published_at text not null
);
create trigger if not exists trg_restore_publications_immutable_update
before update on restore_publications
begin
    select raise(abort, 'restore publication is immutable');
end;
create trigger if not exists trg_restore_publications_immutable_delete
before delete on restore_publications
begin
    select raise(abort, 'restore publication is immutable');
end;
"#;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(in crate::update) struct UpdateOperationJournal {
    pub(in crate::update) operation_handle: String,
    pub(in crate::update) action: String,
    pub(in crate::update) inspection_handle: String,
    pub(in crate::update) source_identity: String,
    pub(in crate::update) target_identity: Option<String>,
    pub(in crate::update) result_identity: Option<String>,
    pub(in crate::update) backup_handle: String,
    pub(in crate::update) idempotency_key: String,
    pub(in crate::update) status: String,
    #[serde(default)]
    pub(in crate::update) completion_sequence: Option<u64>,
    #[serde(default)]
    pub(in crate::update) authority_event_id: Option<i64>,
    #[serde(default)]
    pub(in crate::update) recovery_authority_handle: Option<String>,
    #[serde(default)]
    pub(in crate::update) authority_provenance: Option<String>,
    #[serde(default)]
    pub(in crate::update) authority_provenance_ref: Option<String>,
    #[serde(default)]
    pub(in crate::update) reason: Option<String>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn record_update_decision(
    root: &Path,
    operation_handle: &str,
    inspection_handle: &str,
    expected_current: &str,
    decision_handle: &str,
    choice: &str,
    authority_event_id: i64,
    reason: &str,
) -> Result<()> {
    let conn = Connection::open(ledger_path(root))?;
    let project: i64 = conn.query_row("select id from projects", [], |row| row.get(0))?;
    let authority_exists: bool = conn.query_row(
        "select exists(select 1 from authority_events where id=?1 and project_id=?2)",
        params![authority_event_id, project],
        |row| row.get(0),
    )?;
    if !authority_exists {
        bail!("authority event does not belong to this project");
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "insert or ignore into update_operations(project_id,operation_handle,source_descriptor,expected_current,status,backup_handle,target_identity,edge_path,idempotency_key,created_at,updated_at) values(?1,?2,?3,?4,'decision_recorded',null,null,'owner-decision',?5,current_timestamp,current_timestamp)",
        params![project, operation_handle, inspection_handle, expected_current, decision_handle],
    )?;
    let operation_id: i64 = tx.query_row(
        "select id from update_operations where project_id=?1 and operation_handle=?2",
        params![project, operation_handle],
        |row| row.get(0),
    )?;
    tx.execute(
        "insert or ignore into update_decisions(project_id,update_operation_id,choice_key,authority_event_id,reason,source_revision,predecessor_id,status,created_at) values(?1,?2,?3,?4,?5,?6,null,'recorded',current_timestamp)",
        params![project, operation_id, choice, authority_event_id, reason, expected_current],
    )?;
    tx.commit()?;
    Ok(())
}

pub(super) fn inspection_actions(next_actions: &[String]) -> String {
    next_actions
        .iter()
        .map(|action| format!("next: {action}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn record_prepared_apply(
    conn: &Connection,
    operation_handle: &str,
    inspection_handle: &str,
    expected_current: &str,
    backup_handle: &str,
    target_identity: &str,
    idempotency_key: &str,
) -> Result<()> {
    let project: i64 = conn.query_row("select id from projects", [], |row| row.get(0))?;
    conn.execute(
        "insert into update_operations(project_id,operation_handle,source_descriptor,expected_current,status,backup_handle,target_identity,edge_path,idempotency_key,created_at,updated_at) values(?1,?2,?3,?4,'prepared',?5,?6,'registered-transition',?7,current_timestamp,current_timestamp)",
        params![project, operation_handle, inspection_handle, expected_current, backup_handle, target_identity, idempotency_key],
    )?;
    let operation_id = conn.last_insert_rowid();
    conn.execute(
        "insert into update_receipts(project_id,update_operation_id,source_identity,target_identity,backup_handle,edge_path,status,prepared_at,completed_at) values(?1,?2,?3,?4,?5,'registered-transition','prepared',current_timestamp,null)",
        params![project, operation_id, expected_current, target_identity, backup_handle],
    )?;
    Ok(())
}

pub(super) fn complete_apply_receipt(ledger: &Path, operation_handle: &str) -> Result<()> {
    let conn = Connection::open(ledger)?;
    conn.pragma_update(None, "foreign_keys", true)?;
    let tx = conn.unchecked_transaction()?;
    let project: i64 = tx.query_row("select id from projects", [], |row| row.get(0))?;
    let operation_id: i64 = tx
        .query_row(
            "select id from update_operations where project_id=?1 and operation_handle=?2 and status in ('prepared','published')",
            params![project, operation_handle],
            |row| row.get(0),
        )
        .context("prepared update operation is not present in the published ledger")?;
    tx.execute(
        "update update_operations set status='published',updated_at=current_timestamp where id=?1 and status='prepared'",
        params![operation_id],
    )?;
    tx.execute(
        "update update_receipts set status='published',completed_at=coalesce(completed_at,current_timestamp) where update_operation_id=?1 and status in ('prepared','published')",
        params![operation_id],
    )?;
    tx.commit()?;
    Ok(())
}

pub(super) fn update_operation_exists(ledger: &Path, operation_handle: &str) -> Result<bool> {
    let conn = Connection::open_with_flags(ledger, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let has_table: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='table' and name='update_operations')",
        [],
        |row| row.get(0),
    )?;
    if !has_table {
        return Ok(false);
    }
    conn.query_row(
        "select exists(select 1 from update_operations where operation_handle=?1 and status in ('prepared','published'))",
        params![operation_handle],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn restore_result_identity(journal: &UpdateOperationJournal) -> Result<String> {
    let target = journal
        .target_identity
        .as_deref()
        .context("prepared restore has no target backup identity")?;
    let mut digest = Sha256::new();
    digest.update(b"agent-workbench:restore-result-v1\0");
    for part in [
        journal.operation_handle.as_str(),
        journal.source_identity.as_str(),
        target,
        journal.backup_handle.as_str(),
        journal.idempotency_key.as_str(),
    ] {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(super) fn record_restore_publication(
    ledger: &Path,
    journal: &UpdateOperationJournal,
) -> Result<()> {
    let target = journal
        .target_identity
        .as_deref()
        .context("prepared restore has no target backup identity")?;
    let result = journal
        .result_identity
        .as_deref()
        .context("prepared restore has no result identity")?;
    if result != restore_result_identity(journal)? {
        bail!("prepared restore result identity is inconsistent");
    }
    let mut conn = Connection::open(ledger)?;
    conn.pragma_update(None, "foreign_keys", true)?;
    let tx = conn.transaction()?;
    tx.execute_batch(RESTORE_PUBLICATION_SQL)?;
    tx.execute(
        r#"
        insert or ignore into restore_publications(
            operation_handle, source_identity, target_backup_identity,
            recovery_backup_identity, result_identity, idempotency_key, published_at
        ) values(?1, ?2, ?3, ?4, ?5, ?6, current_timestamp)
        "#,
        params![
            journal.operation_handle,
            journal.source_identity,
            target,
            journal.backup_handle,
            result,
            journal.idempotency_key
        ],
    )?;
    let exact: bool = tx.query_row(
        r#"
        select exists(
            select 1 from restore_publications
            where operation_handle=?1 and source_identity=?2
              and target_backup_identity=?3 and recovery_backup_identity=?4
              and result_identity=?5 and idempotency_key=?6
        )
        "#,
        params![
            journal.operation_handle,
            journal.source_identity,
            target,
            journal.backup_handle,
            result,
            journal.idempotency_key
        ],
        |row| row.get(0),
    )?;
    if !exact {
        bail!("restore publication conflicts with an existing result");
    }
    tx.commit()?;
    Ok(())
}

pub(super) fn restore_publication_result(
    ledger: &Path,
    journal: &UpdateOperationJournal,
) -> Result<Option<String>> {
    let conn = Connection::open_with_flags(ledger, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let has_table: bool = match conn.query_row(
        "select exists(select 1 from sqlite_schema where type='table' and name='restore_publications')",
        [],
        |row| row.get(0),
    ) {
        Ok(has_table) => has_table,
        Err(error) if is_unreadable_ledger_error(&error) =>
        {
            return Ok(None);
        }
        Err(error) => return Err(error.into()),
    };
    if !has_table {
        return Ok(None);
    }
    let marker = match conn
        .query_row(
            r#"
            select source_identity, target_backup_identity, recovery_backup_identity,
                   result_identity, idempotency_key
            from restore_publications
            where operation_handle=?1
            "#,
            params![journal.operation_handle],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()
    {
        Ok(marker) => marker,
        Err(error) if is_unreadable_ledger_error(&error) => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let Some((source, target, recovery, result, idempotency_key)) = marker else {
        return Ok(None);
    };
    let expected_target = journal
        .target_identity
        .as_deref()
        .context("prepared restore has no target backup identity")?;
    let expected_result = restore_result_identity(journal)?;
    if source != journal.source_identity
        || target != expected_target
        || recovery != journal.backup_handle
        || result != expected_result
        || idempotency_key != journal.idempotency_key
    {
        bail!("published restore result conflicts with the prepared request");
    }
    Ok(Some(result))
}

pub(super) fn record_restore_in_ledger(
    root: &Path,
    ledger: &Path,
    journal: &UpdateOperationJournal,
) -> Result<()> {
    let conn = Connection::open(ledger)?;
    conn.pragma_update(None, "foreign_keys", true)?;
    let has_tables: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='table' and name='update_operations') and exists(select 1 from sqlite_schema where type='table' and name='update_receipts')",
        [],
        |row| row.get(0),
    )?;
    if !has_tables {
        return Ok(());
    }
    let tx = conn.unchecked_transaction()?;
    let project: i64 = tx.query_row("select id from projects", [], |row| row.get(0))?;
    if let Some(decision) = decision_for_source(root, &journal.source_identity)?
        && decision.target_identity.as_deref()
            == journal
                .target_identity
                .as_deref()
                .map(|target| format!("restore:{target}"))
                .as_deref()
    {
        let reason = decision
            .reason
            .as_deref()
            .context("recorded update decision has no reason")?;
        let choice = decision
            .target_identity
            .as_deref()
            .context("recorded update decision has no choice")?;
        match (
            decision.authority_event_id,
            decision.recovery_authority_handle.as_deref(),
        ) {
            (Some(authority_event_id), None) => {
                tx.execute(
                    "insert or ignore into update_operations(project_id,operation_handle,source_descriptor,expected_current,status,backup_handle,target_identity,edge_path,idempotency_key,created_at,updated_at) values(?1,?2,?3,?4,'decision_recorded',null,?5,'owner-decision',?6,current_timestamp,current_timestamp)",
                    params![project, decision.operation_handle, decision.inspection_handle, decision.source_identity, choice, decision.idempotency_key],
                )?;
                let decision_operation_id: i64 = tx.query_row(
                    "select id from update_operations where project_id=?1 and operation_handle=?2",
                    params![project, decision.operation_handle],
                    |row| row.get(0),
                )?;
                tx.execute(
                    "insert or ignore into update_decisions(project_id,update_operation_id,choice_key,authority_event_id,reason,source_revision,predecessor_id,status,created_at) values(?1,?2,?3,?4,?5,?6,null,'recorded',current_timestamp)",
                    params![project, decision_operation_id, choice, authority_event_id, reason, decision.source_identity],
                )?;
            }
            (None, Some(authority_handle)) => {
                let authority = journal_for_result(root, authority_handle)?
                    .context("recorded recovery authority is unavailable")?;
                if authority.action != "authority_record"
                    || authority.inspection_handle != decision.inspection_handle
                    || authority.source_identity != decision.source_identity
                    || authority.target_identity.as_deref() != Some(choice)
                    || authority.reason.as_deref() != Some(reason)
                    || authority.recovery_authority_handle.as_deref() != Some(authority_handle)
                    || authority.authority_provenance.as_deref() != Some("user_instruction")
                    || authority
                        .authority_provenance_ref
                        .as_deref()
                        .is_none_or(str::is_empty)
                {
                    bail!("recorded recovery authority no longer matches the update decision");
                }
            }
            _ => bail!("recorded update decision has an invalid authority identity"),
        }
    }
    tx.execute(
        "insert or ignore into update_operations(project_id,operation_handle,source_descriptor,expected_current,status,backup_handle,target_identity,edge_path,idempotency_key,created_at,updated_at) values(?1,?2,'restore',?3,'restored',?4,?5,'restore',?6,current_timestamp,current_timestamp)",
        params![project, journal.operation_handle, journal.source_identity, journal.backup_handle, journal.target_identity, journal.idempotency_key],
    )?;
    let operation_id: i64 = tx.query_row(
        "select id from update_operations where project_id=?1 and operation_handle=?2 and idempotency_key=?3",
        params![project, journal.operation_handle, journal.idempotency_key],
        |row| row.get(0),
    )?;
    tx.execute(
        "insert or ignore into update_receipts(project_id,update_operation_id,source_identity,target_identity,backup_handle,edge_path,status,prepared_at,completed_at) values(?1,?2,?3,?4,?5,'restore','restored',current_timestamp,current_timestamp)",
        params![project, operation_id, journal.source_identity, journal.target_identity, journal.backup_handle],
    )?;
    tx.commit()?;
    Ok(())
}

pub(super) fn apply_outcome_from_journal(
    journal: &UpdateOperationJournal,
    already_applied: bool,
) -> Result<UpdateApplyOutcome> {
    Ok(UpdateApplyOutcome {
        operation_handle: journal.operation_handle.clone(),
        source_identity: journal.source_identity.clone(),
        result_identity: journal
            .result_identity
            .clone()
            .context("completed update journal has no result identity")?,
        backup_identity: journal.backup_handle.clone(),
        already_applied,
    })
}

pub(super) fn restore_outcome_from_journal(
    journal: &UpdateOperationJournal,
    already_applied: bool,
) -> Result<UpdateRestoreOutcome> {
    Ok(UpdateRestoreOutcome {
        operation_handle: journal.operation_handle.clone(),
        restored_identity: journal
            .result_identity
            .clone()
            .context("completed restore journal has no result identity")?,
        recovery_backup_identity: journal.backup_handle.clone(),
        already_applied,
    })
}

pub(super) fn ensure_journal_payload(
    journal: &UpdateOperationJournal,
    operation_handle: &str,
    action: &str,
    inspection_handle: &str,
    expected_current: &str,
    target_identity: &str,
) -> Result<()> {
    let target_matches =
        target_identity.is_empty() || journal.target_identity.as_deref() == Some(target_identity);
    if journal.operation_handle != operation_handle
        || journal.action != action
        || journal.inspection_handle != inspection_handle
        || journal.source_identity != expected_current
        || !target_matches
    {
        bail!("idempotency key was already used with a different update request");
    }
    Ok(())
}

pub(super) fn recorded_recovery_choice(
    root: &Path,
    source_identity: &str,
    recovery_sources: &[String],
) -> Result<Option<String>> {
    let Some(journal) = decision_for_source(root, source_identity)? else {
        return Ok(None);
    };
    let choice = journal
        .target_identity
        .context("recorded update decision has no selected recovery source")?;
    let Some(backup) = choice.strip_prefix("restore:") else {
        return Ok(None);
    };
    if !recovery_sources.iter().any(|candidate| candidate == backup) {
        bail!("recorded update decision refers to an unavailable recovery source");
    }
    Ok(Some(choice))
}

pub(super) fn decision_for_source(
    root: &Path,
    source_identity: &str,
) -> Result<Option<UpdateOperationJournal>> {
    let directory = operation_dir(root);
    if !directory.is_dir() {
        return Ok(None);
    }
    let mut selected = None;
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let journal: UpdateOperationJournal = serde_json::from_slice(&fs::read(&path)?)
            .with_context(|| format!("cannot read update operation {}", path.display()))?;
        if journal.action == "decide"
            && journal.source_identity == source_identity
            && journal.status == "completed"
        {
            if selected.is_some() {
                bail!("multiple update decisions are recorded for the same source");
            }
            selected = Some(journal);
        }
    }
    Ok(selected)
}

pub(super) fn recovery_authority(path: &Path) -> Result<Option<(i64, String)>> {
    let conn = open_immutable_snapshot(path)?;
    let has_authority: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='table' and name='authority_events')",
        [],
        |row| row.get(0),
    )?;
    if !has_authority {
        return Ok(None);
    }
    let mut statement = conn.prepare(
        "select id,text_or_summary from authority_events where status='active' and event_type='user_instruction' and coalesce(scope,'project')='project' order by id",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(match rows.as_slice() {
        [(id, reason)] if !reason.trim().is_empty() => Some((*id, reason.clone())),
        _ => None,
    })
}

pub(super) fn recovery_authority_exists(
    path: &Path,
    authority_event_id: i64,
    reason: &str,
) -> Result<bool> {
    let conn = open_immutable_snapshot(path)?;
    conn.query_row(
        "select exists(select 1 from authority_events where id=?1 and status='active' and event_type='user_instruction' and coalesce(scope,'project')='project' and text_or_summary=?2)",
        params![authority_event_id, reason],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn journal_for_key(
    root: &Path,
    idempotency_key: &str,
) -> Result<Option<UpdateOperationJournal>> {
    let directory = operation_dir(root);
    if !directory.is_dir() {
        return Ok(None);
    }
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let bytes = fs::read(&path)?;
        let journal: UpdateOperationJournal = serde_json::from_slice(&bytes)
            .with_context(|| format!("cannot read update operation {}", path.display()))?;
        if journal.idempotency_key == idempotency_key {
            return Ok(Some(journal));
        }
    }
    Ok(None)
}

pub(super) fn journal_for_result(
    root: &Path,
    result_identity: &str,
) -> Result<Option<UpdateOperationJournal>> {
    let directory = operation_dir(root);
    if !directory.is_dir() {
        return Ok(None);
    }
    let mut found = None;
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let journal: UpdateOperationJournal = serde_json::from_slice(&fs::read(&path)?)
            .with_context(|| format!("cannot read update operation {}", path.display()))?;
        if journal.result_identity.as_deref() == Some(result_identity) {
            if found.is_some() {
                bail!("multiple update operations publish the same result identity");
            }
            found = Some(journal);
        }
    }
    Ok(found)
}

pub(super) fn managed_backup_priorities(root: &Path) -> Result<BTreeMap<String, (u64, u8)>> {
    let directory = operation_dir(root);
    let mut handles = BTreeMap::new();
    if !directory.is_dir() {
        return Ok(handles);
    }
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let journal: UpdateOperationJournal = serde_json::from_slice(&fs::read(&path)?)
            .with_context(|| format!("cannot read update operation {}", path.display()))?;
        let Some(sequence) = journal.completion_sequence else {
            continue;
        };
        if journal.status != "completed" || !matches!(journal.action.as_str(), "apply" | "restore")
        {
            continue;
        }
        let mut record = |handle: String, rank: u8| {
            if !valid_handle(&handle) {
                return;
            }
            let priority = (sequence, rank);
            if handles.get(&handle).is_none_or(|current| {
                priority.0 > current.0 || (priority.0 == current.0 && priority.1 < current.1)
            }) {
                handles.insert(handle, priority);
            }
        };
        if journal.action == "restore" {
            if let Some(target) = journal.target_identity {
                record(target, 0);
            }
            record(journal.backup_handle, 1);
        } else {
            record(journal.backup_handle, 0);
        }
    }
    Ok(handles)
}

pub(super) fn complete_journal(root: &Path, journal: &mut UpdateOperationJournal) -> Result<()> {
    if journal.completion_sequence.is_none() {
        let directory = operation_dir(root);
        let mut latest = 0_u64;
        if directory.is_dir() {
            for entry in fs::read_dir(&directory)? {
                let path = entry?.path();
                if path.extension().and_then(|value| value.to_str()) != Some("json") {
                    continue;
                }
                let existing: UpdateOperationJournal = serde_json::from_slice(&fs::read(&path)?)
                    .with_context(|| format!("cannot read update operation {}", path.display()))?;
                latest = latest.max(existing.completion_sequence.unwrap_or(0));
            }
        }
        journal.completion_sequence = Some(
            latest
                .checked_add(1)
                .context("update operation completion sequence is exhausted")?,
        );
    }
    journal.status = "completed".to_string();
    write_journal(root, journal)
}

pub(super) fn write_journal(root: &Path, journal: &UpdateOperationJournal) -> Result<()> {
    let directory = operation_dir(root);
    fs::create_dir_all(&directory)?;
    let target = directory.join(format!("{}.json", journal.operation_handle));
    let staged = directory.join(format!("{}.tmp", journal.operation_handle));
    let bytes = serde_json::to_vec_pretty(journal)?;
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&staged)?;
    file.write_all(&bytes)?;
    file.flush()?;
    file.sync_all()?;
    drop(file);
    fs::rename(&staged, &target)?;
    sync_dir(&directory)?;
    Ok(())
}

pub(super) fn operation_dir(root: &Path) -> PathBuf {
    root.join(crate::db::LEDGER_DIR).join(OPERATION_DIR)
}

pub(super) fn remove_staged(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub(super) fn verify_restorable_ledger(path: &Path, root: &Path) -> Result<()> {
    require_standalone_snapshot(path)?;
    let conn = open_immutable_snapshot(path)?;
    verify_restorable_connection(&conn, root)
}

pub(super) fn verify_content_addressed_snapshot(
    path: &Path,
    expected_identity: &str,
) -> Result<()> {
    require_standalone_snapshot(path)?;
    if sha256_file(path)? != expected_identity {
        bail!("managed backup does not match its content-addressed name");
    }
    Ok(())
}

pub(super) fn require_standalone_snapshot(path: &Path) -> Result<()> {
    for suffix in ["-wal", "-shm", "-journal"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        match fs::symlink_metadata(PathBuf::from(sidecar)) {
            Ok(_) => bail!("managed update source is not a standalone snapshot"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

pub(super) fn open_immutable_snapshot(path: &Path) -> Result<Connection> {
    require_standalone_snapshot(path)?;
    let absolute = fs::canonicalize(path)?;
    let mut uri = String::from("file:");
    for byte in absolute.as_os_str().as_encoded_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'.' | b'_' | b'~' => {
                uri.push(char::from(*byte));
            }
            _ => {
                use std::fmt::Write as _;
                write!(&mut uri, "%{byte:02X}")?;
            }
        }
    }
    uri.push_str("?immutable=1");
    Connection::open_with_flags(
        uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )
    .map_err(Into::into)
}

pub(super) fn verify_restorable_connection(conn: &Connection, root: &Path) -> Result<()> {
    let integrity: String = conn.query_row("pragma integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        bail!("backup integrity check failed: {integrity}");
    }
    transition::classify_storage_header(conn)
        .context("backup is not a recognized restorable Agent Workbench ledger")?;
    let root_text = root.to_string_lossy();
    let project_count: i64 = conn.query_row(
        "select count(*) from projects where root_path=?1",
        params![root_text.as_ref()],
        |row| row.get(0),
    )?;
    if project_count != 1 {
        bail!("backup does not contain exactly one project identity for this root");
    }
    let foreign_key_failures: i64 =
        conn.query_row("select count(*) from pragma_foreign_key_check", [], |row| {
            row.get(0)
        })?;
    if foreign_key_failures != 0 {
        bail!("backup foreign-key check failed");
    }
    Ok(())
}

pub(super) fn semantic_ledger_identity(path: &Path) -> Result<String> {
    let conn = open_immutable_snapshot(path)?;
    transition::semantic_storage_identity(&conn)
}

pub(crate) fn checkpoint(path: &Path) -> Result<()> {
    let conn = Connection::open(path)?;
    conn.execute_batch("pragma wal_checkpoint(truncate);")?;
    Ok(())
}

pub(super) fn checkpoint_restore_source(path: &Path) -> Result<()> {
    match checkpoint(path) {
        Ok(()) => Ok(()),
        Err(error)
            if error
                .downcast_ref::<rusqlite::Error>()
                .is_some_and(is_unreadable_ledger_error) =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

fn is_unreadable_ledger_error(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(code, _)
            if matches!(
                code.code,
                rusqlite::ffi::ErrorCode::NotADatabase
                    | rusqlite::ffi::ErrorCode::DatabaseCorrupt
            )
    )
}

pub(super) fn update_lock(directory: &Path) -> Result<File> {
    File::open(directory).map_err(Into::into)
}

pub(crate) fn install_copy(source: &Path, target: &Path, expected_identity: &str) -> Result<()> {
    let mut input = open_regular_file(source)?;
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(target)?;
    std::io::copy(&mut input, &mut output)?;
    output.flush()?;
    output.sync_all()?;
    drop(output);
    if sha256_file(target)? != expected_identity {
        let _ = fs::remove_file(target);
        bail!("staged copy identity mismatch");
    }
    let parent = target
        .parent()
        .context("staged copy has no parent directory")?;
    sync_dir(parent)?;
    Ok(())
}

pub(crate) fn sha256_file(path: &Path) -> Result<String> {
    let mut file = open_regular_file(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn open_regular_file(path: &Path) -> Result<File> {
    let metadata =
        fs::symlink_metadata(path).with_context(|| format!("cannot inspect {}", path.display()))?;
    if !metadata.file_type().is_file() {
        bail!("managed update source is not a regular file");
    }
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    if !file.metadata()?.is_file() {
        bail!("managed update source is not a regular file");
    }
    Ok(file)
}

pub(super) fn ledger_state_identity(ledger: &Path) -> Result<String> {
    let mut digest = Sha256::new();
    digest.update(b"agent-workbench:ledger-state-v1\0");
    hash_identity_part(&mut digest, ledger)?;
    let wal = PathBuf::from(format!("{}-wal", ledger.display()));
    if wal.exists() {
        hash_identity_part(&mut digest, &wal)?;
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub(super) fn hash_identity_part(digest: &mut Sha256, path: &Path) -> Result<()> {
    let mut file = File::open(path)?;
    let length = file.metadata()?.len();
    digest.update(length.to_be_bytes());
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(())
}

pub(super) fn normalized_root(root: &Path) -> Result<PathBuf> {
    root.canonicalize()
        .with_context(|| format!("cannot resolve project root {}", root.display()))
}

pub(crate) fn ledger_path(root: &Path) -> PathBuf {
    root.join(crate::db::LEDGER_DIR)
        .join(crate::db::LEDGER_FILE)
}

pub(crate) fn backup_dir(root: &Path) -> PathBuf {
    root.join(crate::db::LEDGER_DIR).join(BACKUP_DIR)
}

pub(super) fn inspection_handle(current_identity: &str, status: &str) -> String {
    digest_handle("update_inspection_", &[current_identity, status])
}

pub(super) fn digest_handle(prefix: &str, parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"agent-workbench:update-handle-v1\0");
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("{prefix}{:x}", digest.finalize())
}

pub(super) fn short_identity(value: &str) -> &str {
    &value[..value.len().min(12)]
}

pub(super) fn require_handle(value: &str, label: &str) -> Result<()> {
    if !valid_handle(value) {
        bail!("{label} must be a 64-character lowercase SHA-256 value");
    }
    Ok(())
}

pub(super) fn require_prefixed_handle(value: &str, prefix: &str, label: &str) -> Result<()> {
    let Some(digest) = value.strip_prefix(prefix) else {
        bail!("{label} is not a recognized opaque handle");
    };
    require_handle(digest, label)
}

pub(super) fn require_token(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 200
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
    {
        bail!("{label} must be a non-empty portable token");
    }
    Ok(())
}

pub(super) fn valid_handle(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub(crate) fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}
