use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use crate::identity::{CanonicalValue, canonical_bytes, domain_digest};

use super::OwnerSource;

pub(super) struct PreparedIntent {
    root: PathBuf,
    pub(super) digest: String,
}

pub(super) fn prepare(
    root: &Path,
    owner: &OwnerSource,
    plan_digest: &str,
    mode: &str,
    backup_digest: &str,
    database_digest: &str,
) -> Result<PreparedIntent> {
    let value = CanonicalValue::object([
        ("algorithm", CanonicalValue::string("AWB-APPLY-INTENT-v1")),
        ("state", CanonicalValue::string("prepared")),
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
        ("bound_plan_sha256", CanonicalValue::string(plan_digest)),
        ("mode", CanonicalValue::string(mode)),
        ("backup_sha256", CanonicalValue::string(backup_digest)),
        ("database_sha256", CanonicalValue::string(database_digest)),
    ]);
    let bytes = canonical_bytes(&value);
    let digest = domain_digest(b"AWB-APPLY-INTENT-v1\0", &value);
    let directory = directory(root);
    fs::create_dir_all(&directory)?;
    write_content_once(
        &directory,
        &directory.join(format!("intent-{digest}-prepared.json")),
        &bytes,
    )?;
    Ok(PreparedIntent {
        root: root.to_path_buf(),
        digest,
    })
}

impl PreparedIntent {
    pub(super) fn publish_committed(&self) -> Result<()> {
        publish_committed(&self.root, &self.digest)
    }
}

pub(super) fn publish_committed(root: &Path, digest: &str) -> Result<()> {
    validate_digest(digest)?;
    let directory = directory(root);
    let prepared_path = directory.join(format!("intent-{digest}-prepared.json"));
    let prepared = fs::read(&prepared_path).context("prepared apply intent is missing")?;
    if digest_bytes(b"AWB-APPLY-INTENT-v1\0", &prepared) != digest {
        bail!("prepared apply intent validation failed");
    }
    let marker = canonical_bytes(&CanonicalValue::object([
        ("algorithm", CanonicalValue::string("AWB-APPLY-INTENT-v1")),
        ("state", CanonicalValue::string("committed")),
        ("intent_sha256", CanonicalValue::string(digest)),
    ]));
    write_content_once(
        &directory,
        &directory.join(format!("intent-{digest}-committed.json")),
        &marker,
    )
}

fn directory(root: &Path) -> PathBuf {
    root.join(".agent-workbench")
        .join("recovery")
        .join("task-history")
}

fn write_content_once(directory: &Path, path: &Path, bytes: &[u8]) -> Result<()> {
    if path.exists() {
        if fs::read(path)? != bytes {
            bail!("apply intent content-address collision");
        }
        return Ok(());
    }
    let temporary = directory.join(format!(".intent-{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary, path)?;
    File::open(directory)?.sync_all()?;
    Ok(())
}

fn digest_bytes(domain: &[u8], bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn validate_digest(value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("invalid apply intent digest");
    }
    Ok(())
}
