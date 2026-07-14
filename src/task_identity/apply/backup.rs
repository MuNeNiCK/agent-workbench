use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::db::{default_ledger_path, open_ledger};
use crate::identity::{CanonicalValue, canonical_bytes};

use super::super::source::{OwnerSource, read_from_connection};

pub(crate) struct BackupArtifact {
    pub(crate) digest: String,
    pub(crate) _path: PathBuf,
}

pub(crate) fn create(
    root: &Path,
    owner: &OwnerSource,
    plan_digest: &str,
    database_digest: &str,
) -> Result<BackupArtifact> {
    let directory = root
        .join(".agent-workbench")
        .join("recovery")
        .join("task-history");
    fs::create_dir_all(&directory)?;
    let temporary = directory.join(format!(
        ".backup-{}-{}.tmp",
        std::process::id(),
        owner.owner_id
    ));
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let source = open_ledger(&default_ledger_path(root))?;
    let mut destination = Connection::open(&temporary)?;
    {
        let backup = rusqlite::backup::Backup::new(&source, &mut destination)?;
        backup.run_to_completion(64, Duration::from_millis(5), None)?;
    }
    let integrity: String =
        destination.query_row("pragma integrity_check(1)", [], |row| row.get(0))?;
    if integrity != "ok" {
        bail!("backup integrity validation failed");
    }
    let snapshot = read_from_connection(&destination, root)?;
    let backup_owner = snapshot
        .owners
        .iter()
        .find(|candidate| candidate.owner_digest == owner.owner_digest)
        .context("backup source component is missing")?;
    if backup_owner.source_digest != owner.source_digest {
        bail!("backup source snapshot does not match selected plan");
    }
    if snapshot.database_digest != database_digest {
        bail!("backup database snapshot does not match prepared source");
    }
    destination.close().map_err(|(_, error)| error)?;
    File::open(&temporary)?.sync_all()?;
    let digest = file_digest(&temporary)?;
    let final_path = directory.join(format!("backup-{plan_digest}-{digest}.sqlite"));
    if final_path.exists() {
        if file_digest(&final_path)? != digest {
            bail!("backup artifact collision");
        }
        fs::remove_file(&temporary)?;
    } else {
        fs::rename(&temporary, &final_path)?;
        sync_directory(&directory)?;
    }
    let metadata = canonical_bytes(&CanonicalValue::object([
        ("algorithm", CanonicalValue::string("AWB-BACKUP-v1")),
        (
            "owner_digest",
            CanonicalValue::string(owner.owner_digest.clone()),
        ),
        (
            "component_sha256",
            CanonicalValue::string(owner.component_digest.clone()),
        ),
        (
            "source_sha256",
            CanonicalValue::string(owner.source_digest.clone()),
        ),
        ("database_sha256", CanonicalValue::string(database_digest)),
        ("bound_plan_sha256", CanonicalValue::string(plan_digest)),
        ("sqlite_sha256", CanonicalValue::string(digest.clone())),
    ]));
    write_content_once(
        &directory,
        &directory.join(format!("backup-{plan_digest}-{digest}.json")),
        &metadata,
    )?;
    Ok(BackupArtifact {
        digest,
        _path: final_path,
    })
}

fn write_content_once(directory: &Path, path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        if fs::read(path)? != bytes {
            bail!("backup metadata collision");
        }
        return Ok(());
    }
    let temporary = directory.join(format!(".backup-metadata-{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary, path)?;
    sync_directory(directory)
}

fn file_digest(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn sync_directory(path: &Path) -> Result<()> {
    File::open(path)?.sync_all()?;
    Ok(())
}
