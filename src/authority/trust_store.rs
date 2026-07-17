use std::path::Path;

use anyhow::{Context, Result, bail};
use std::collections::{BTreeMap, BTreeSet};
use time::OffsetDateTime;

use super::safe_file::{OwnerPolicy, read_absolute};
use super::signed_envelope::{CborValue, decode_canonical, hex_digest};

pub const TRUST_STORE_PATH: &str = "/etc/agent-workbench/authority/signed-envelope-v1.cbor";
pub const MAX_TRUST_STORE_BYTES: u64 = 65_536;

#[derive(Clone, Debug)]
pub struct TrustKey {
    pub key_id: Vec<u8>,
    pub public_key: [u8; 32],
    pub status: TrustKeyStatus,
    pub not_before: i64,
    pub not_after: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrustKeyStatus {
    Active,
    Retired,
}

impl TrustKey {
    pub fn usable_at(&self, now: OffsetDateTime) -> bool {
        self.status == TrustKeyStatus::Active
            && self.not_before <= now.unix_timestamp()
            && now.unix_timestamp() < self.not_after
    }
}

#[derive(Clone, Debug)]
pub struct TrustStore {
    pub digest: String,
    pub keys: Vec<TrustKey>,
}

pub fn load_fixed_trust_store(now: OffsetDateTime) -> Result<TrustStore> {
    load_trust_store(Path::new(TRUST_STORE_PATH), now, true)
}

pub(crate) fn load_trust_store(
    path: &Path,
    now: OffsetDateTime,
    require_fixed: bool,
) -> Result<TrustStore> {
    if require_fixed && path != Path::new(TRUST_STORE_PATH) {
        bail!("signed-envelope-v1 trust store path is fixed");
    }
    let bytes = read_absolute(path, MAX_TRUST_STORE_BYTES, None, OwnerPolicy::Root)?;
    let root = decode_canonical(&bytes)?;
    let CborValue::Map(map) = root else {
        bail!("trust store must be a map");
    };
    if map.len() != 2 || uint_field(&map, 0, "schema")? != 1 {
        bail!("trust store must contain exactly schema 1 and key records");
    }
    let records = match map.get(&1) {
        Some(CborValue::Array(values)) if (1..=32).contains(&values.len()) => values.clone(),
        _ => bail!("trust store must contain 1..32 key records"),
    };
    let mut keys = Vec::with_capacity(records.len());
    let mut replacements = BTreeMap::<Vec<u8>, Vec<u8>>::new();
    let mut prior_id: Option<Vec<u8>> = None;
    for record in records {
        let CborValue::Map(map) = record else {
            bail!("trust key record must be a map");
        };
        let key_id = bytes_field(&map, 0, "key id")?.to_vec();
        if key_id.len() != 16 {
            bail!("trust key id must be exactly 16 bytes");
        }
        if prior_id.as_ref().is_some_and(|prior| prior >= &key_id) {
            bail!("trust key records must be strictly key-id ordered");
        }
        prior_id = Some(key_id.clone());
        let key: [u8; 32] = bytes_field(&map, 1, "public key")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("public key must be 32 bytes"))?;
        let start = int_field(&map, 2, "not-before")?;
        let end = int_field(&map, 3, "not-after")?;
        if end <= start {
            bail!("trust key validity window is empty");
        }
        let status = uint_field(&map, 4, "status")?;
        if status > 1 {
            bail!("trust key status is invalid");
        }
        if status == 0 && (map.len() != 5 || map.contains_key(&5)) {
            bail!("active trust key cannot name a replacement");
        }
        if status == 1 && (map.len() != 6 || !map.contains_key(&5)) {
            bail!("retired trust key requires a replacement");
        }
        if status == 1 {
            let replacement = bytes_field(&map, 5, "replacement key id")?.to_vec();
            if replacement.len() != 16 || replacement == key_id {
                bail!("retired trust key has an invalid replacement");
            }
            replacements.insert(key_id.clone(), replacement);
        }
        if keys.iter().any(|stored: &TrustKey| stored.key_id == key_id) {
            bail!("duplicate trust key id");
        }
        keys.push(TrustKey {
            key_id,
            public_key: key,
            status: if status == 0 {
                TrustKeyStatus::Active
            } else {
                TrustKeyStatus::Retired
            },
            not_before: start,
            not_after: end,
        });
    }
    let ids = keys
        .iter()
        .map(|key| key.key_id.clone())
        .collect::<BTreeSet<_>>();
    let mut predecessors = BTreeSet::new();
    for target in replacements.values() {
        if !ids.contains(target) {
            bail!("replacement trust key is missing");
        }
        if !predecessors.insert(target.clone()) {
            bail!("trust replacement graph merges");
        }
    }
    for start in replacements.keys() {
        let mut seen = BTreeSet::new();
        let mut current = start;
        while let Some(next) = replacements.get(current) {
            if !seen.insert(current.clone()) {
                bail!("trust replacement graph contains a cycle");
            }
            current = next;
        }
        let terminal = keys
            .iter()
            .find(|key| key.key_id == *current)
            .context("replacement terminal is missing")?;
        if terminal.status != TrustKeyStatus::Active || !terminal.usable_at(now) {
            bail!("trust replacement path must end in an active key");
        }
    }
    if !keys.iter().any(|key| key.usable_at(now)) {
        bail!("trust store has no currently active key");
    }
    Ok(TrustStore {
        digest: hex_digest(&bytes),
        keys,
    })
}

fn bytes_field<'a>(
    map: &'a std::collections::BTreeMap<u64, CborValue>,
    key: u64,
    label: &str,
) -> Result<&'a [u8]> {
    match map.get(&key) {
        Some(CborValue::Bytes(value)) => Ok(value),
        _ => bail!("{label} has the wrong type"),
    }
}
fn int_field(
    map: &std::collections::BTreeMap<u64, CborValue>,
    key: u64,
    label: &str,
) -> Result<i64> {
    match map.get(&key) {
        Some(CborValue::I64(value)) => Ok(*value),
        Some(CborValue::U64(value)) => {
            i64::try_from(*value).context(format!("{label} is too large"))
        }
        _ => bail!("{label} has the wrong type"),
    }
}
fn uint_field(
    map: &std::collections::BTreeMap<u64, CborValue>,
    key: u64,
    label: &str,
) -> Result<u64> {
    match map.get(&key) {
        Some(CborValue::U64(value)) => Ok(*value),
        _ => bail!("{label} has the wrong type"),
    }
}
