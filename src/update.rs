use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use fs2::FileExt;
use rusqlite::{Connection, DatabaseName, OptionalExtension, backup::Backup, params};
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

const LEDGER_DIR: &str = ".agent-workbench";
const LEDGER_FILE: &str = "ledger.sqlite";
const TARGET_SCHEMA: i64 = 14;
const TARGET_PROFILE: &str = "5ea4819df0978e86402a81a94f6f61e61b2e7f9b501e052d1e4455fb934243ee";
const SOURCE_PROFILES: [&str; 4] = [
    "877adac85029da006fd293f5f943b0191dac22f45fab2b6f596f156626bfcf76",
    "b2b5db94248639cc319345bbacf42972884903220c3eb30b42996bf1b6bdbc35",
    "08ce59ebc53cc6b422e01b25c173e84cccee539d23175fd72317f12f0a436166",
    "c1ec0110b62a963f692abc1cdfe2ce2774b95fba5c795aaff79436db62d2bd9e",
];
const SCHEMA14: &str = include_str!("schema14.sql");

#[derive(Debug, Clone)]
pub struct UpdatePlan {
    pub source_schema: i64,
    pub source_profile: String,
    pub source_identity: String,
    pub target_schema: i64,
    pub target_profile: String,
    pub backup_path: PathBuf,
    pub nonempty_tables: Vec<(String, i64)>,
    pub already_applied: bool,
}

#[derive(Debug, Clone)]
pub struct UpdateResetOutcome {
    pub plan: UpdatePlan,
    pub backup_handle: String,
}

#[derive(Debug, Clone)]
pub struct RestoreOutcome {
    pub operation_id: String,
    pub result_identity: String,
    pub recovery_backup_identity: String,
    pub receipt_path: PathBuf,
    pub already_applied: bool,
}

pub fn init_fresh(root: &Path) -> Result<PathBuf> {
    let root = normalized_root(root)?;
    let directory = ledger_dir(&root);
    fs::create_dir_all(&directory)?;
    let _lock = acquire_project_lock(&root)?;
    let ledger = ledger_path(&root);
    if ledger.exists() {
        let conn =
            Connection::open_with_flags(&ledger, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        if is_schema14(&conn)? {
            verify_schema14(&conn)?;
            let stored_root: String = conn.query_row(
                "select root_path from projects where handle='project:current'",
                [],
                |row| row.get(0),
            )?;
            if normalized_root(Path::new(&stored_root))? != root {
                bail!("schema14 project root differs from the invoked normalized root");
            }
            return Ok(ledger);
        }
        bail!("existing ledger requires explicit agent-workbench update --dry-run");
    }
    let temp = temp_path(&directory, "fresh-schema14");
    let mut conn = Connection::open(&temp)?;
    conn.execute_batch(SCHEMA14)?;
    if schema_profile(&conn)? != TARGET_PROFILE {
        bail!("embedded schema14 manifest digest does not match the normative digest");
    }
    let name = root
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("project");
    let root_text = root.to_string_lossy();
    let now = OffsetDateTime::now_utc().format(&Rfc3339)?;
    let tx = conn.transaction()?;
    tx.execute(
        "insert into schema_metadata(singleton,schema_version,manifest_digest) values(1,14,?1)",
        params![TARGET_PROFILE],
    )?;
    tx.execute(
        "insert into projects(handle,name,root_path,created_at) values('project:current',?1,?2,?3)",
        params![name, root_text.as_ref(), now],
    )?;
    tx.execute(
        "insert into update_audits(handle,project_handle,source_schema,target_schema,source_profile,source_identity,backup_handle,mode,created_at) values(?1,'project:current',14,14,?2,?2,'none','fresh',?3)",
        params![format!("update:{TARGET_PROFILE}:14"), TARGET_PROFILE, now],
    )?;
    tx.commit()?;
    drop(conn);
    sync_file(&temp)?;
    fs::rename(&temp, &ledger)?;
    sync_dir(&directory)?;
    let installed =
        Connection::open_with_flags(&ledger, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    verify_schema14(&installed)?;
    Ok(ledger)
}

pub fn is_schema14_root(root: &Path) -> Result<bool> {
    let ledger = ledger_path(root);
    if !ledger.exists() {
        return Ok(false);
    }
    let _lock = if ledger_dir(root).join("update.lock").exists() {
        Some(acquire_project_read_lock(root)?)
    } else {
        None
    };
    let conn = Connection::open_with_flags(ledger, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    if !is_schema14(&conn)? {
        return Ok(false);
    }
    verify_schema14(&conn)?;
    Ok(true)
}

#[derive(Debug)]
struct SourceInfo {
    schema: i64,
    profile: String,
    name: String,
    root_path: String,
    created_at: String,
    nonempty_tables: Vec<(String, i64)>,
}

pub fn dry_run(root: &Path) -> Result<UpdatePlan> {
    let root = normalized_root(root)?;
    let ledger = ledger_path(&root);
    let conn = Connection::open_with_flags(&ledger, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("failed to open {}", ledger.display()))?;
    conn.execute_batch("begin;")?;
    if is_schema14(&conn)? {
        verify_schema14(&conn)?;
        let identity = sha256_file(&ledger)?;
        return Ok(UpdatePlan {
            source_schema: TARGET_SCHEMA,
            source_profile: TARGET_PROFILE.to_string(),
            source_identity: identity.clone(),
            target_schema: TARGET_SCHEMA,
            target_profile: TARGET_PROFILE.to_string(),
            backup_path: backup_dir(&root).join(format!("{identity}.sqlite")),
            nonempty_tables: Vec::new(),
            already_applied: true,
        });
    }
    let source = inspect_schema13(&conn, &root)?;
    let identity = sqlite_backup_identity(&conn)?;
    Ok(UpdatePlan {
        source_schema: source.schema,
        source_profile: source.profile,
        source_identity: identity.clone(),
        target_schema: TARGET_SCHEMA,
        target_profile: TARGET_PROFILE.to_string(),
        backup_path: backup_dir(&root).join(format!("{identity}.sqlite")),
        nonempty_tables: source.nonempty_tables,
        already_applied: false,
    })
}

pub fn reset(root: &Path, reason: &str) -> Result<UpdateResetOutcome> {
    if reason.trim().is_empty() {
        bail!("reset reason is required and cannot be blank");
    }
    let root = normalized_root(root)?;
    let _lock = acquire_project_lock(&root)?;
    let ledger = ledger_path(&root);
    let source_conn = Connection::open(&ledger)
        .with_context(|| format!("failed to open {}", ledger.display()))?;
    if is_schema14(&source_conn)? {
        verify_schema14(&source_conn)?;
        let identity = sha256_file(&ledger)?;
        return Ok(UpdateResetOutcome {
            plan: UpdatePlan {
                source_schema: TARGET_SCHEMA,
                source_profile: TARGET_PROFILE.to_string(),
                source_identity: identity.clone(),
                target_schema: TARGET_SCHEMA,
                target_profile: TARGET_PROFILE.to_string(),
                backup_path: backup_dir(&root).join(format!("{identity}.sqlite")),
                nonempty_tables: Vec::new(),
                already_applied: true,
            },
            backup_handle: "none".to_string(),
        });
    }
    source_conn.execute_batch("pragma wal_checkpoint(truncate);")?;
    let source = inspect_schema13(&source_conn, &root)?;
    fs::create_dir_all(backup_dir(&root))?;
    sync_dir(&ledger_dir(&root))?;

    let source_data_version = sqlite_data_version(&source_conn)?;
    let planned_source_identity = sqlite_backup_identity(&source_conn)?;
    let backup_temp = temp_path(&backup_dir(&root), "backup");
    sqlite_backup(&source_conn, &backup_temp)?;
    let backup_identity = sha256_file(&backup_temp)?;
    if backup_identity != planned_source_identity {
        bail!("verified backup does not reproduce the planned source identity");
    }
    if sqlite_data_version(&source_conn)? != source_data_version {
        bail!("source changed while the verified backup was being created");
    }
    let backup_path = backup_dir(&root).join(format!("{backup_identity}.sqlite"));
    install_content_addressed(&backup_temp, &backup_path, &backup_identity)?;
    verify_backup_schema13(&backup_path, &root, &source.profile)?;

    let source_identity = sha256_file(&backup_path)?;
    let receipt_dir = ledger_dir(&root).join("update-receipts");
    let restore_receipt = if receipt_dir.exists() {
        find_completed_receipt(&receipt_dir, &root, &source_identity, None)?
            .map(|receipt| receipt.0)
    } else {
        None
    };

    let target_temp = temp_path(&ledger_dir(&root), "schema14");
    create_reset_target(
        &target_temp,
        &source,
        &source_identity,
        &backup_identity,
        reason,
        restore_receipt.as_deref(),
    )?;
    let target_conn =
        Connection::open_with_flags(&target_temp, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    verify_schema14(&target_conn)?;
    drop(target_conn);
    sync_file(&target_temp)?;
    source_conn.execute_batch("begin exclusive;")?;
    if sqlite_data_version(&source_conn)? != source_data_version {
        bail!("source identity changed before atomic replacement");
    }
    fs::rename(&target_temp, &ledger)?;
    sync_dir(&ledger_dir(&root))?;
    drop(source_conn);
    let installed =
        Connection::open_with_flags(&ledger, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    verify_schema14(&installed)?;
    let audit_count: i64 = installed.query_row(
        "select count(*) from update_audits where source_identity=?1 and backup_handle=?2 and mode='reset'",
        params![source_identity, backup_identity],
        |row| row.get(0),
    )?;
    if audit_count != 1 {
        bail!("installed schema14 update audit does not match the reset");
    }
    Ok(UpdateResetOutcome {
        plan: UpdatePlan {
            source_schema: source.schema,
            source_profile: source.profile,
            source_identity,
            target_schema: TARGET_SCHEMA,
            target_profile: TARGET_PROFILE.to_string(),
            backup_path,
            nonempty_tables: source.nonempty_tables,
            already_applied: false,
        },
        backup_handle: backup_identity,
    })
}

pub fn restore(root: &Path, backup_handle: &str, expected_current: &str) -> Result<RestoreOutcome> {
    validate_hex(backup_handle, "backup handle")?;
    validate_hex(expected_current, "expected current identity")?;
    let root = normalized_root(root)?;
    let _lock = acquire_project_lock(&root)?;
    let ledger = ledger_path(&root);
    let requested = backup_dir(&root).join(format!("{backup_handle}.sqlite"));
    if sha256_file(&requested)? != backup_handle {
        bail!("requested backup identity does not match its content");
    }
    verify_restorable(&requested, &root)?;
    verify_restorable(&ledger, &root)?;
    let current_conn = Connection::open(&ledger)?;
    current_conn.execute_batch("pragma wal_checkpoint(truncate);")?;
    let current_data_version = sqlite_data_version(&current_conn)?;
    let current_identity = sha256_file(&ledger)?;
    let result_identity = backup_handle.to_string();

    let receipt_dir = ledger_dir(&root).join("update-receipts");
    fs::create_dir_all(&receipt_dir)?;
    if current_identity == result_identity {
        let recovery = find_completed_receipt(
            &receipt_dir,
            &root,
            &current_identity,
            Some(expected_current),
        )?
        .context(
            "ledger already matches the requested backup but no matching restore receipt exists",
        )?;
        return Ok(RestoreOutcome {
            operation_id: recovery.0,
            result_identity,
            recovery_backup_identity: recovery.1,
            receipt_path: recovery.2,
            already_applied: true,
        });
    }
    if current_identity != expected_current {
        bail!("current ledger identity does not match --expected-current");
    }

    let recovery_temp = temp_path(&backup_dir(&root), "recovery");
    sqlite_backup(&current_conn, &recovery_temp)?;
    let recovery_identity = sha256_file(&recovery_temp)?;
    let recovery_path = backup_dir(&root).join(format!("{recovery_identity}.sqlite"));
    install_content_addressed(&recovery_temp, &recovery_path, &recovery_identity)?;
    verify_restorable(&recovery_path, &root)?;
    if sqlite_data_version(&current_conn)? != current_data_version {
        bail!("current ledger changed while the recovery backup was being created");
    }

    let operation_id = restore_operation_id(
        &current_identity,
        backup_handle,
        &result_identity,
        &recovery_identity,
    );
    let receipt_path = receipt_dir.join(format!("{operation_id}.json"));
    let bytes = canonical_restore_receipt(
        &operation_id,
        &current_identity,
        backup_handle,
        &result_identity,
        &recovery_identity,
    )?;
    install_immutable_bytes(&receipt_path, &bytes)?;
    sync_dir(&receipt_dir)?;

    let restored_temp = temp_path(&ledger_dir(&root), "restore");
    fs::copy(&requested, &restored_temp)?;
    sync_file(&restored_temp)?;
    if sha256_file(&restored_temp)? != result_identity {
        bail!("restored temporary image is not byte-identical to the requested backup");
    }
    current_conn.execute_batch("begin exclusive;")?;
    if sqlite_data_version(&current_conn)? != current_data_version
        || sha256_file(&ledger)? != current_identity
    {
        bail!("current ledger identity changed before atomic restore");
    }
    fs::rename(&restored_temp, &ledger)?;
    sync_dir(&ledger_dir(&root))?;
    drop(current_conn);
    if sha256_file(&ledger)? != result_identity {
        bail!("installed restored ledger identity is incorrect");
    }
    Ok(RestoreOutcome {
        operation_id,
        result_identity,
        recovery_backup_identity: recovery_identity,
        receipt_path,
        already_applied: false,
    })
}

fn inspect_schema13(conn: &Connection, root: &Path) -> Result<SourceInfo> {
    verify_sqlite_connection(conn)?;
    let schema: i64 = conn
        .query_row("select max(version) from schema_migrations", [], |row| {
            row.get(0)
        })
        .context("source ledger does not expose schema_migrations")?;
    if schema != 13 {
        bail!("unsupported source schema {schema}; supported update is 13 -> 14");
    }
    let profile = schema_profile(conn)?;
    if !SOURCE_PROFILES.contains(&profile.as_str()) {
        bail!("unsupported schema13 source profile {profile}");
    }
    let count: i64 = conn.query_row("select count(*) from projects", [], |row| row.get(0))?;
    if count != 1 {
        bail!("schema13 source must contain exactly one project row");
    }
    let (name, root_path, created_at): (String, String, String) = conn.query_row(
        "select name,root_path,created_at from projects",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    if normalized_root(Path::new(&root_path))? != root {
        bail!("schema13 project root differs from the invoked normalized root");
    }
    Ok(SourceInfo {
        schema,
        profile,
        name,
        root_path,
        created_at,
        nonempty_tables: nonempty_table_counts(conn)?,
    })
}

fn create_reset_target(
    path: &Path,
    source: &SourceInfo,
    source_identity: &str,
    backup_handle: &str,
    reason: &str,
    restore_receipt_handle: Option<&str>,
) -> Result<()> {
    let mut conn = Connection::open(path)?;
    conn.execute_batch(SCHEMA14)?;
    if schema_profile(&conn)? != TARGET_PROFILE {
        bail!("embedded schema14 manifest digest does not match the normative digest");
    }
    let now = OffsetDateTime::now_utc().format(&Rfc3339)?;
    let tx = conn.transaction()?;
    tx.execute(
        "insert into schema_metadata(singleton,schema_version,manifest_digest) values(1,14,?1)",
        params![TARGET_PROFILE],
    )?;
    tx.execute(
        "insert into projects(handle,name,root_path,created_at) values('project:current',?1,?2,?3)",
        params![source.name, source.root_path, source.created_at],
    )?;
    tx.execute(
        "insert into legacy_ledgers(handle,project_handle,source_schema,source_profile,source_identity,backup_handle,reset_reason,created_at) values(?1,'project:current',13,?2,?3,?4,?5,?6)",
        params![format!("legacy:{source_identity}"), source.profile, source_identity, backup_handle, reason, now],
    )?;
    tx.execute(
        "insert into update_audits(handle,project_handle,source_schema,target_schema,source_profile,source_identity,backup_handle,restore_receipt_handle,mode,created_at) values(?1,'project:current',13,14,?2,?3,?4,?5,'reset',?6)",
        params![format!("update:{source_identity}:14"), source.profile, source_identity, backup_handle, restore_receipt_handle, now],
    )?;
    tx.commit()?;
    Ok(())
}

pub(crate) fn verify_schema14(conn: &Connection) -> Result<()> {
    verify_sqlite_connection(conn)?;
    if schema_profile(conn)? != TARGET_PROFILE {
        bail!("schema14 manifest digest mismatch");
    }
    let metadata: (i64, String) = conn.query_row(
        "select schema_version,manifest_digest from schema_metadata where singleton=1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if metadata.0 != TARGET_SCHEMA || metadata.1 != TARGET_PROFILE {
        bail!("schema14 metadata does not match the normative manifest");
    }
    Ok(())
}

fn is_schema14(conn: &Connection) -> Result<bool> {
    let table_exists: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='table' and name='schema_metadata')",
        [],
        |row| row.get(0),
    )?;
    if !table_exists {
        return Ok(false);
    }
    Ok(conn
        .query_row(
            "select schema_version from schema_metadata where singleton=1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        == Some(TARGET_SCHEMA))
}

fn verify_backup_schema13(path: &Path, root: &Path, expected_profile: &str) -> Result<()> {
    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let source = inspect_schema13(&conn, root)?;
    if source.profile != expected_profile {
        bail!("verified backup profile changed");
    }
    Ok(())
}

fn verify_restorable(path: &Path, root: &Path) -> Result<()> {
    let conn = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    if is_schema14(&conn)? {
        verify_schema14(&conn)?;
        let stored_root: String = conn.query_row(
            "select root_path from projects where handle='project:current'",
            [],
            |row| row.get(0),
        )?;
        if normalized_root(Path::new(&stored_root))? != root {
            bail!("schema14 restore project root differs from the invoked normalized root");
        }
    } else {
        inspect_schema13(&conn, root)?;
    }
    Ok(())
}

fn verify_sqlite_connection(conn: &Connection) -> Result<()> {
    let mut stmt = conn.prepare("pragma integrity_check")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    for row in rows {
        if row? != "ok" {
            bail!("SQLite integrity_check failed");
        }
    }
    let fk: i64 = conn.query_row("select count(*) from pragma_foreign_key_check", [], |row| {
        row.get(0)
    })?;
    if fk != 0 {
        bail!("SQLite foreign_key_check failed");
    }
    Ok(())
}

pub fn schema_profile(conn: &Connection) -> Result<String> {
    let mut stmt = conn.prepare(
        "select type,name,tbl_name,sql from sqlite_schema where name not like 'sqlite_%' order by type,name",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?.unwrap_or_default(),
        ))
    })?;
    let mut hasher = Sha256::new();
    for row in rows {
        let (kind, name, table, sql) = row?;
        hasher.update(kind.as_bytes());
        hasher.update(b"|");
        hasher.update(name.as_bytes());
        hasher.update(b"|");
        hasher.update(table.as_bytes());
        hasher.update(b"|");
        hasher.update(sql.replace('\r', "").as_bytes());
        hasher.update(b"\n");
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn nonempty_table_counts(conn: &Connection) -> Result<Vec<(String, i64)>> {
    let mut stmt = conn.prepare(
        "select name from sqlite_schema where type='table' and name not like 'sqlite_%' order by name",
    )?;
    let names = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut counts = Vec::new();
    for name in names {
        let quoted = name.replace('"', "\"\"");
        let count: i64 =
            conn.query_row(&format!("select count(*) from \"{quoted}\""), [], |row| {
                row.get(0)
            })?;
        if count > 0 {
            counts.push((name, count));
        }
    }
    Ok(counts)
}

fn sqlite_backup(source: &Connection, destination: &Path) -> Result<()> {
    let mut dest = Connection::open_in_memory()?;
    let backup = Backup::new(source, &mut dest)?;
    backup.run_to_completion(64, Duration::from_millis(10), None)?;
    drop(backup);
    let image = dest.serialize(DatabaseName::Main)?;
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)?;
    file.write_all(&image)?;
    file.sync_all()?;
    drop(image);
    drop(dest);
    Ok(())
}

fn sqlite_backup_identity(source: &Connection) -> Result<String> {
    let mut destination = Connection::open_in_memory()?;
    let backup = Backup::new(source, &mut destination)?;
    backup.run_to_completion(64, Duration::from_millis(1), None)?;
    drop(backup);
    let image = destination.serialize(DatabaseName::Main)?;
    Ok(format!("{:x}", Sha256::digest(&*image)))
}

fn sqlite_data_version(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("pragma data_version", [], |row| row.get(0))?)
}

fn install_content_addressed(temp: &Path, final_path: &Path, identity: &str) -> Result<()> {
    if final_path.exists() {
        if sha256_file(final_path)? != identity {
            bail!("content-addressed backup path contains different bytes");
        }
        fs::remove_file(temp)?;
        return Ok(());
    }
    fs::rename(temp, final_path)?;
    sync_dir(final_path.parent().context("backup path has no parent")?)
}

fn install_immutable_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        if fs::read(path)? != bytes {
            bail!("existing restore receipt differs from the canonical receipt");
        }
        return Ok(());
    }
    let temp = temp_path(
        path.parent().context("receipt path has no parent")?,
        "receipt",
    );
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(temp, path)?;
    Ok(())
}

fn find_completed_receipt(
    dir: &Path,
    root: &Path,
    result_identity: &str,
    expected_source_identity: Option<&str>,
) -> Result<Option<(String, String, PathBuf)>> {
    let mut match_found: Option<(String, String, PathBuf)> = None;
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        let receipt_bytes = fs::read(&path)?;
        let value: serde_json::Value = serde_json::from_slice(&receipt_bytes)?;
        if value.get("result_identity").and_then(|v| v.as_str()) == Some(result_identity) {
            let object = value
                .as_object()
                .context("restore receipt is not an object")?;
            let expected_fields = [
                "format",
                "operation",
                "operation_id",
                "recovery_backup_identity",
                "requested_backup_identity",
                "result_identity",
                "source_identity",
            ];
            if object.len() != expected_fields.len()
                || expected_fields
                    .iter()
                    .any(|field| !object.contains_key(*field))
                || object.get("format").and_then(|v| v.as_u64()) != Some(1)
                || object.get("operation").and_then(|v| v.as_str()) != Some("restore")
            {
                bail!("restore receipt has a noncanonical shape");
            }
            let operation = value
                .get("operation_id")
                .and_then(|v| v.as_str())
                .context("restore receipt has no operation_id")?;
            let recovery = value
                .get("recovery_backup_identity")
                .and_then(|v| v.as_str())
                .context("restore receipt has no recovery identity")?;
            let source = value
                .get("source_identity")
                .and_then(|v| v.as_str())
                .context("restore receipt has no source identity")?;
            let requested = value
                .get("requested_backup_identity")
                .and_then(|v| v.as_str())
                .context("restore receipt has no requested identity")?;
            for (identity, label) in [
                (operation, "operation id"),
                (recovery, "recovery identity"),
                (source, "source identity"),
                (requested, "requested identity"),
                (result_identity, "result identity"),
            ] {
                validate_hex(identity, label)?;
            }
            let expected_operation =
                restore_operation_id(source, requested, result_identity, recovery);
            if operation != expected_operation
                || requested != result_identity
                || expected_source_identity.is_some_and(|expected| source != expected)
                || path.file_stem().and_then(|value| value.to_str()) != Some(operation)
                || receipt_bytes
                    != canonical_restore_receipt(
                        operation,
                        source,
                        requested,
                        result_identity,
                        recovery,
                    )?
                || sha256_file(&backup_dir(root).join(format!("{recovery}.sqlite")))? != recovery
            {
                bail!("restore receipt failed canonical identity validation");
            }
            verify_restorable(&backup_dir(root).join(format!("{recovery}.sqlite")), root)?;
            if match_found.is_some() {
                bail!("multiple canonical restore receipts match the completed operation");
            }
            match_found = Some((operation.to_string(), recovery.to_string(), path));
        }
    }
    Ok(match_found)
}

fn restore_operation_id(source: &str, requested: &str, result: &str, recovery: &str) -> String {
    let mut bytes = b"agent-workbench:restore:v1\0".to_vec();
    for value in [source, requested, result, recovery] {
        bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
        bytes.extend_from_slice(value.as_bytes());
    }
    format!("{:x}", Sha256::digest(bytes))
}

fn canonical_restore_receipt(
    operation_id: &str,
    source_identity: &str,
    requested_backup_identity: &str,
    result_identity: &str,
    recovery_backup_identity: &str,
) -> Result<Vec<u8>> {
    let mut object = serde_json::Map::new();
    object.insert("format".to_string(), serde_json::Value::from(1));
    object.insert("operation".to_string(), serde_json::Value::from("restore"));
    object.insert(
        "operation_id".to_string(),
        serde_json::Value::from(operation_id),
    );
    object.insert(
        "recovery_backup_identity".to_string(),
        serde_json::Value::from(recovery_backup_identity),
    );
    object.insert(
        "requested_backup_identity".to_string(),
        serde_json::Value::from(requested_backup_identity),
    );
    object.insert(
        "result_identity".to_string(),
        serde_json::Value::from(result_identity),
    );
    object.insert(
        "source_identity".to_string(),
        serde_json::Value::from(source_identity),
    );
    Ok(serde_json::to_vec(&serde_json::Value::Object(object))?)
}

pub(crate) fn acquire_project_lock(root: &Path) -> Result<File> {
    fs::create_dir_all(ledger_dir(root))?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(ledger_dir(root).join("update.lock"))?;
    file.try_lock_exclusive()
        .context("another Agent Workbench update holds the project lock")?;
    Ok(file)
}

pub(crate) fn acquire_project_read_lock(root: &Path) -> Result<File> {
    fs::create_dir_all(ledger_dir(root))?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(ledger_dir(root).join("update.lock"))?;
    FileExt::try_lock_shared(&file)
        .context("an Agent Workbench update is replacing the project ledger")?;
    Ok(file)
}

fn normalized_root(root: &Path) -> Result<PathBuf> {
    fs::canonicalize(root)
        .with_context(|| format!("cannot normalize project root {}", root.display()))
}

fn ledger_dir(root: &Path) -> PathBuf {
    root.join(LEDGER_DIR)
}

fn ledger_path(root: &Path) -> PathBuf {
    ledger_dir(root).join(LEDGER_FILE)
}

fn backup_dir(root: &Path) -> PathBuf {
    ledger_dir(root).join("update-backups")
}

fn temp_path(directory: &Path, label: &str) -> PathBuf {
    directory.join(format!(
        ".{label}-{}-{}.tmp",
        std::process::id(),
        OffsetDateTime::now_utc().unix_timestamp_nanos()
    ))
}

fn sha256_file(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn validate_hex(value: &str, label: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("{label} must be lowercase 64-digit SHA-256 hex");
    }
    Ok(())
}

fn sync_file(path: &Path) -> Result<()> {
    OpenOptions::new().read(true).open(path)?.sync_all()?;
    Ok(())
}

fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema13_reset_and_restore_are_explicit_exact_and_byte_identical() {
        let temp = tempfile::tempdir().unwrap();
        crate::db::init_project(temp.path()).unwrap();

        let plan = dry_run(temp.path()).unwrap();
        assert_eq!(plan.source_schema, 13);
        assert!(SOURCE_PROFILES.contains(&plan.source_profile.as_str()));
        assert_eq!(plan.target_profile, TARGET_PROFILE);
        assert!(!plan.already_applied);

        let outcome = reset(temp.path(), "test the explicit breaking reset").unwrap();
        assert_eq!(outcome.plan.source_identity, plan.source_identity);
        assert_eq!(outcome.backup_handle, plan.source_identity);
        let ledger = ledger_path(temp.path());
        let target = Connection::open(&ledger).unwrap();
        verify_schema14(&target).unwrap();
        for (table, expected) in [
            ("schema_metadata", 1),
            ("projects", 1),
            ("legacy_ledgers", 1),
            ("update_audits", 1),
            ("records", 0),
            ("relations", 0),
            ("claims", 0),
            ("decisions", 0),
            ("evidence", 0),
            ("snapshots", 0),
            ("snapshot_components", 0),
            ("lifecycle_events", 0),
        ] {
            let count: i64 = target
                .query_row(&format!("select count(*) from {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(count, expected, "{table}");
        }
        drop(target);

        let reset_retry = reset(temp.path(), "test the explicit breaking reset").unwrap();
        assert!(reset_retry.plan.already_applied);
        assert_eq!(reset_retry.backup_handle, "none");

        let current_identity = sha256_file(&ledger).unwrap();
        let wrong_expected = "0".repeat(64);
        assert!(
            restore(temp.path(), &outcome.backup_handle, &wrong_expected)
                .unwrap_err()
                .to_string()
                .contains("does not match --expected-current")
        );
        let restored = restore(temp.path(), &outcome.backup_handle, &current_identity).unwrap();
        assert!(!restored.already_applied);
        assert_eq!(sha256_file(&ledger).unwrap(), outcome.backup_handle);
        assert_eq!(
            fs::read(&ledger).unwrap(),
            fs::read(&outcome.plan.backup_path).unwrap()
        );
        let receipt = fs::read_to_string(&restored.receipt_path).unwrap();
        assert!(receipt.starts_with("{\"format\":1,\"operation\":\"restore\",\"operation_id\":"));
        assert!(
            receipt.find("\"recovery_backup_identity\"").unwrap()
                < receipt.find("\"requested_backup_identity\"").unwrap()
        );
        assert!(
            receipt.find("\"result_identity\"").unwrap()
                < receipt.find("\"source_identity\"").unwrap()
        );
        let retry = restore(temp.path(), &outcome.backup_handle, &current_identity).unwrap();
        assert!(retry.already_applied);
        assert_eq!(retry.operation_id, restored.operation_id);
    }

    #[test]
    fn embedded_schema14_manifest_has_the_normative_digest_and_twelve_objects() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(SCHEMA14).unwrap();
        assert_eq!(schema_profile(&conn).unwrap(), TARGET_PROFILE);
        let count: i64 = conn
            .query_row(
                "select count(*) from sqlite_schema where name not like 'sqlite_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 12);
        verify_sqlite_connection(&conn).unwrap();

        conn.execute_batch("create table forbidden_extra_object(id integer)")
            .unwrap();
        assert_ne!(schema_profile(&conn).unwrap(), TARGET_PROFILE);
        assert!(verify_schema14(&conn).is_err());
    }

    #[test]
    fn ordinary_open_and_restore_reject_non_exact_sqlite_shapes() {
        let temp = tempfile::tempdir().unwrap();
        let ledger = init_fresh(temp.path()).unwrap();
        let conn = Connection::open(&ledger).unwrap();
        conn.execute_batch("create table unauthorized(id integer)")
            .unwrap();
        drop(conn);
        assert!(is_schema14_root(temp.path()).is_err());

        let arbitrary = backup_dir(temp.path()).join(format!("{}.sqlite", "a".repeat(64)));
        fs::create_dir_all(backup_dir(temp.path())).unwrap();
        Connection::open(&arbitrary).unwrap();
        let actual = sha256_file(&arbitrary).unwrap();
        let named = backup_dir(temp.path()).join(format!("{actual}.sqlite"));
        fs::rename(arbitrary, &named).unwrap();
        assert!(restore(temp.path(), &actual, &sha256_file(&ledger).unwrap()).is_err());
    }

    #[test]
    fn completed_restore_rejects_noncanonical_receipt_bytes() {
        let temp = tempfile::tempdir().unwrap();
        crate::db::init_project(temp.path()).unwrap();
        let outcome = reset(temp.path(), "receipt validation").unwrap();
        let current = sha256_file(&ledger_path(temp.path())).unwrap();
        let restored = restore(temp.path(), &outcome.backup_handle, &current).unwrap();
        let mut bytes = fs::read(&restored.receipt_path).unwrap();
        bytes.push(b'\n');
        fs::write(&restored.receipt_path, bytes).unwrap();
        assert!(restore(temp.path(), &outcome.backup_handle, &current).is_err());
    }

    #[test]
    fn data_version_detects_a_wal_only_concurrent_commit() {
        let temp = tempfile::tempdir().unwrap();
        let ledger = init_fresh(temp.path()).unwrap();
        let watcher = Connection::open(&ledger).unwrap();
        watcher
            .execute_batch(
                "pragma journal_mode=wal; pragma wal_autocheckpoint=0; pragma wal_checkpoint(truncate);",
            )
            .unwrap();
        let before_version = sqlite_data_version(&watcher).unwrap();
        let before_main = sha256_file(&ledger).unwrap();
        let writer = Connection::open(&ledger).unwrap();
        writer
            .execute_batch("pragma wal_autocheckpoint=0;")
            .unwrap();
        writer
            .execute(
                "insert into projects(handle,name,root_path,created_at) values('project:concurrent','concurrent','/concurrent','9999-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
        assert_ne!(sqlite_data_version(&watcher).unwrap(), before_version);
        assert_eq!(sha256_file(&ledger).unwrap(), before_main);
    }

    #[test]
    fn fresh_init_creates_only_the_declared_schema14_management_rows() {
        let temp = tempfile::tempdir().unwrap();
        let ledger = init_fresh(temp.path()).unwrap();
        assert_eq!(init_fresh(temp.path()).unwrap(), ledger);
        let conn = Connection::open(ledger).unwrap();
        verify_schema14(&conn).unwrap();
        assert_eq!(
            conn.query_row("select count(*) from projects", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row(
                "select count(*) from update_audits where mode='fresh'",
                [],
                |row| { row.get::<_, i64>(0) }
            )
            .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row("select count(*) from legacy_ledgers", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            conn.query_row("select count(*) from records", [], |row| row
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
