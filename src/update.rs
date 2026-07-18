use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use fs2::FileExt;
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use sha2::{Digest, Sha256};

const BACKUP_DIR: &str = "update-backups";

#[derive(Debug, PartialEq, Eq)]
pub struct UpdateInspection {
    pub current_identity: String,
    pub restorable_backups: Vec<String>,
    pub pending_changes: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct UpdateRestoreOutcome {
    pub restored_identity: String,
    pub recovery_backup_identity: String,
    pub already_applied: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct UpdateApplyOutcome {
    pub source_identity: String,
    pub result_identity: String,
    pub backup_identity: String,
    pub already_applied: bool,
}

pub fn inspect_update(root: &Path) -> Result<UpdateInspection> {
    let root = normalized_root(root)?;
    let ledger = ledger_path(&root);
    let current_identity = sha256_file(&ledger)?;
    let conn = Connection::open_with_flags(&ledger, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let has_full_schema: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='table' and name='schema_migrations')",
        [],
        |row| row.get(0),
    )?;
    let pending_changes = if has_full_schema {
        crate::db::pending_update_changes(&conn)?
    } else {
        vec!["restore_full_project_state".to_string()]
    };
    let mut restorable_backups = Vec::new();
    let backups = backup_dir(&root);
    if backups.is_dir() {
        for entry in fs::read_dir(backups)? {
            let path = entry?.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(handle) = name.strip_suffix(".sqlite") else {
                continue;
            };
            if valid_handle(handle)
                && sha256_file(&path).is_ok_and(|identity| identity == handle)
                && verify_restorable_ledger(&path, &root).is_ok()
            {
                restorable_backups.push(handle.to_string());
            }
        }
    }
    restorable_backups.sort();
    Ok(UpdateInspection {
        current_identity,
        restorable_backups,
        pending_changes,
    })
}

pub fn restore_update(
    root: &Path,
    backup_handle: &str,
    expected_current: &str,
) -> Result<UpdateRestoreOutcome> {
    require_handle(backup_handle, "backup handle")?;
    require_handle(expected_current, "expected current identity")?;
    let root = normalized_root(root)?;
    let directory = root.join(crate::db::LEDGER_DIR);
    let ledger = ledger_path(&root);
    let backups = backup_dir(&root);
    fs::create_dir_all(&backups)?;
    let lock_path = directory.join("update.lock");
    let lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(lock_path)?;
    lock.lock_exclusive()?;

    checkpoint(&ledger)?;
    let current_identity = sha256_file(&ledger)?;
    if current_identity != expected_current {
        bail!(
            "current identity changed: expected {expected_current}, found {current_identity}; run agent-workbench update inspect again"
        );
    }
    let requested = backups.join(format!("{backup_handle}.sqlite"));
    if sha256_file(&requested)? != backup_handle {
        bail!("backup handle does not match the backup content");
    }
    verify_restorable_ledger(&requested, &root)?;
    if current_identity == backup_handle {
        return Ok(UpdateRestoreOutcome {
            restored_identity: backup_handle.to_string(),
            recovery_backup_identity: current_identity,
            already_applied: true,
        });
    }

    let recovery_path = backups.join(format!("{current_identity}.sqlite"));
    if recovery_path.exists() {
        if sha256_file(&recovery_path)? != current_identity {
            bail!("existing recovery backup does not match its content-addressed name");
        }
    } else {
        install_copy(&ledger, &recovery_path, &current_identity)?;
    }

    let staged = directory.join(format!("restore-{backup_handle}.tmp"));
    if staged.exists() {
        fs::remove_file(&staged)?;
    }
    install_copy(&requested, &staged, backup_handle)?;
    fs::rename(&staged, &ledger)?;
    sync_dir(&directory)?;
    verify_restorable_ledger(&ledger, &root)?;
    if sha256_file(&ledger)? != backup_handle {
        bail!("installed ledger identity differs from the requested backup");
    }
    Ok(UpdateRestoreOutcome {
        restored_identity: backup_handle.to_string(),
        recovery_backup_identity: current_identity,
        already_applied: false,
    })
}

pub fn apply_update(root: &Path, expected_current: &str) -> Result<UpdateApplyOutcome> {
    require_handle(expected_current, "expected current identity")?;
    let root = normalized_root(root)?;
    let directory = root.join(crate::db::LEDGER_DIR);
    let ledger = ledger_path(&root);
    let backups = backup_dir(&root);
    fs::create_dir_all(&backups)?;
    let lock = update_lock(&directory)?;
    lock.lock_exclusive()?;
    checkpoint(&ledger)?;
    let source_identity = sha256_file(&ledger)?;
    if source_identity != expected_current {
        bail!(
            "current identity changed: expected {expected_current}, found {source_identity}; run agent-workbench update inspect again"
        );
    }
    let backup = backups.join(format!("{source_identity}.sqlite"));
    if !backup.exists() {
        install_copy(&ledger, &backup, &source_identity)?;
    }
    let staged = directory.join(format!("update-{source_identity}.tmp"));
    if staged.exists() {
        fs::remove_file(&staged)?;
    }
    install_copy(&ledger, &staged, &source_identity)?;
    let update_result = (|| -> Result<()> {
        let conn = Connection::open(&staged)?;
        conn.pragma_update(None, "foreign_keys", true)?;
        crate::db::apply_pending_update(&conn)?;
        crate::db::migrate(&conn)?;
        crate::db::ensure_project(&conn, &root)?;
        crate::db::sync_agents_md_authority(&conn, &root)?;
        crate::db::sync_commit_message_policy(&conn)?;
        let remaining = crate::db::pending_update_changes(&conn)?;
        if !remaining.is_empty() {
            bail!("staged update remains incomplete: {}", remaining.join(","));
        }
        drop(conn);
        verify_restorable_ledger(&staged, &root)
    })();
    if let Err(error) = update_result {
        let _ = fs::remove_file(&staged);
        return Err(error.context("explicit update failed; original project state was preserved"));
    }
    let result_identity = sha256_file(&staged)?;
    fs::rename(&staged, &ledger)?;
    sync_dir(&directory)?;
    if sha256_file(&ledger)? != result_identity {
        bail!("installed update identity differs from the verified staged state");
    }
    let already_applied = result_identity == source_identity;
    Ok(UpdateApplyOutcome {
        source_identity: source_identity.clone(),
        result_identity,
        backup_identity: source_identity,
        already_applied,
    })
}

fn verify_restorable_ledger(path: &Path, root: &Path) -> Result<()> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let integrity: String = conn.query_row("pragma integrity_check", [], |row| row.get(0))?;
    if integrity != "ok" {
        bail!("backup integrity check failed: {integrity}");
    }
    let has_full_schema: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='table' and name='schema_migrations')",
        [],
        |row| row.get(0),
    )?;
    let has_reset_schema: bool = conn.query_row(
        "select exists(select 1 from sqlite_schema where type='table' and name='schema_metadata')",
        [],
        |row| row.get(0),
    )?;
    if has_full_schema {
        let version = conn
            .query_row(
                "select version from schema_migrations order by version desc limit 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .context("backup has no full-schema version")?;
        if !(1..=crate::db::SCHEMA_VERSION).contains(&version) {
            bail!("backup full schema {version} is not restorable by this build");
        }
    } else if has_reset_schema {
        let version: i64 = conn.query_row(
            "select schema_version from schema_metadata where singleton=1",
            [],
            |row| row.get(0),
        )?;
        if version != 14 {
            bail!("backup reset schema {version} is not recognized");
        }
    } else {
        bail!("backup is not a recognized Agent Workbench ledger");
    }
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

fn checkpoint(path: &Path) -> Result<()> {
    let conn = Connection::open(path)?;
    conn.execute_batch("pragma wal_checkpoint(truncate);")?;
    Ok(())
}

fn update_lock(directory: &Path) -> Result<File> {
    OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(directory.join("update.lock"))
        .map_err(Into::into)
}

fn install_copy(source: &Path, target: &Path, expected_identity: &str) -> Result<()> {
    let mut input = File::open(source)?;
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
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
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

pub(crate) fn current_identity(root: &Path) -> Result<String> {
    sha256_file(&ledger_path(&normalized_root(root)?))
}

fn normalized_root(root: &Path) -> Result<PathBuf> {
    root.canonicalize()
        .with_context(|| format!("cannot resolve project root {}", root.display()))
}

fn ledger_path(root: &Path) -> PathBuf {
    root.join(crate::db::LEDGER_DIR)
        .join(crate::db::LEDGER_FILE)
}

fn backup_dir(root: &Path) -> PathBuf {
    root.join(crate::db::LEDGER_DIR).join(BACKUP_DIR)
}

fn require_handle(value: &str, label: &str) -> Result<()> {
    if !valid_handle(value) {
        bail!("{label} must be a 64-character lowercase SHA-256 value");
    }
    Ok(())
}

fn valid_handle(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn sync_dir(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restore_is_verified_reversible_and_idempotent() {
        let temp = tempfile::tempdir().unwrap();
        crate::init_project(temp.path()).unwrap();
        let root = temp.path().canonicalize().unwrap();
        let ledger = ledger_path(&root);
        let original_identity = sha256_file(&ledger).unwrap();
        let backups = backup_dir(&root);
        fs::create_dir_all(&backups).unwrap();
        install_copy(
            &ledger,
            &backups.join(format!("{original_identity}.sqlite")),
            &original_identity,
        )
        .unwrap();

        crate::start_work(&root, "change current state", None).unwrap();
        checkpoint(&ledger).unwrap();
        let changed_identity = sha256_file(&ledger).unwrap();
        assert_ne!(changed_identity, original_identity);

        let restored = restore_update(&root, &original_identity, &changed_identity).unwrap();
        assert!(!restored.already_applied);
        assert_eq!(restored.restored_identity, original_identity);
        assert_eq!(restored.recovery_backup_identity, changed_identity);
        assert_eq!(sha256_file(&ledger).unwrap(), original_identity);
        assert_eq!(
            sha256_file(&backups.join(format!("{changed_identity}.sqlite"))).unwrap(),
            changed_identity
        );

        let repeated = restore_update(&root, &original_identity, &original_identity).unwrap();
        assert!(repeated.already_applied);
        assert!(crate::project_status(&root).unwrap().initialized);
    }
}
