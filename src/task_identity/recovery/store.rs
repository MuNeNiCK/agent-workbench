use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use crate::identity::canonical_bytes;

use super::model::{RecoveryEnvelope, envelope_value, recovery_digest};

#[derive(Deserialize, Serialize)]
struct RecoveryHead {
    generation: u64,
    recovery_sha256: String,
}

pub(super) fn load_envelope(root: &Path, owner_digest: &str) -> Result<Option<RecoveryEnvelope>> {
    let directory = recovery_directory(root);
    let head_path = directory.join(format!("head-{owner_digest}.json"));
    if !head_path.exists() {
        return Ok(None);
    }
    let head: RecoveryHead = serde_json::from_slice(&fs::read(&head_path)?)?;
    let content = fs::read(directory.join(format!("recovery-{}.json", head.recovery_sha256)))?;
    let envelope: RecoveryEnvelope = serde_json::from_slice(&content)?;
    if recovery_digest(&envelope) != head.recovery_sha256
        || canonical_bytes(&envelope_value(&envelope)) != content
    {
        bail!("recovery artifact validation failed");
    }
    Ok(Some(envelope))
}

pub(super) fn persist_envelope(root: &Path, envelope: &RecoveryEnvelope) -> Result<String> {
    let directory = recovery_directory(root);
    fs::create_dir_all(&directory)?;
    let bytes = canonical_bytes(&envelope_value(envelope));
    let digest = recovery_digest(envelope);
    let content_path = directory.join(format!("recovery-{digest}.json"));
    write_content_once(&directory, &content_path, &bytes)?;

    let head_path = directory.join(format!("head-{}.json", envelope.owner_digest));
    let generation = if head_path.exists() {
        let prior: RecoveryHead = serde_json::from_slice(&fs::read(&head_path)?)?;
        if prior.recovery_sha256 == digest {
            return Ok(digest);
        }
        prior.generation + 1
    } else {
        1
    };
    let head = serde_json::to_vec(&RecoveryHead {
        generation,
        recovery_sha256: digest.clone(),
    })?;
    let temporary = directory.join(format!(".head-{}-{generation}.tmp", std::process::id()));
    write_new_file(&temporary, &head)?;
    fs::rename(&temporary, &head_path)?;
    File::open(&directory)?.sync_all()?;
    Ok(digest)
}

fn recovery_directory(root: &Path) -> PathBuf {
    root.join(".agent-workbench")
        .join("recovery")
        .join("task-history")
}

fn write_content_once(directory: &Path, path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        if fs::read(path)? != bytes {
            bail!("recovery content-address collision");
        }
        return Ok(());
    }
    let temporary = directory.join(format!(".content-{}.tmp", std::process::id()));
    write_new_file(&temporary, bytes)?;
    fs::rename(&temporary, path)?;
    File::open(directory)?.sync_all()?;
    Ok(())
}

fn write_new_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}
