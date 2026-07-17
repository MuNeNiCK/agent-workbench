use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};
use time::OffsetDateTime;

use crate::db::{open_authority_migration_project, open_existing_project, project_id};
use crate::identity::{AssertionHandle, CanonicalValue, PrincipalHandle};
use crate::identity::{domain_digest, signed_source_id};

use super::safe_file::{OwnerPolicy, read_absolute, write_new_absolute};
use super::signed_envelope::{
    UnsignedEnvelopeRequest, hex_digest, parse_hex, purpose_name, subject_kind_name,
    unsigned_request, verify_envelope,
};
use super::trust_store::load_fixed_trust_store;

#[derive(Debug, Clone)]
pub struct AssertionImportOutcome {
    pub assertion_handle: String,
    pub assertion_digest: String,
    pub purpose: String,
}

#[derive(Debug, Clone)]
pub struct PrincipalResolutionOutcome {
    pub principal_handle: String,
    pub subject_kind: String,
}

#[derive(Debug, Clone)]
pub struct AssertionRequestOutcome {
    pub preimage_digest: String,
    pub request_digest: String,
}

struct StoredAssertion {
    id: i64,
    purpose: String,
    subject: String,
    project_digest: String,
    trust_digest: String,
    key_id: String,
    assertion_digest: String,
    payload_digest: String,
    payload: Vec<u8>,
    envelope: Vec<u8>,
    expires: String,
    consumed: Option<String>,
}

pub fn create_assertion_request(
    root: &Path,
    mut request: UnsignedEnvelopeRequest,
    output: &Path,
) -> Result<AssertionRequestOutcome> {
    let now = OffsetDateTime::now_utc();
    let store = load_fixed_trust_store(now)?;
    let conn = open_authority_migration_project(root)?;
    let project = project_id(&conn)?;
    let project_hex = domain_digest(
        b"AWB-RECOVERY-PROJECT-v1\0",
        &CanonicalValue::object([(
            "project",
            CanonicalValue::string(signed_source_id(project)?),
        )]),
    );
    request.project_digest = parse_hex(&project_hex, "project digest")?;
    request.trust_identity = parse_hex(&store.digest, "trust identity")?;
    if !store
        .keys
        .iter()
        .any(|key| key.usable_at(now) && key.key_id == request.key_id)
    {
        bail!("request key is not active in the pinned trust store");
    }
    let (bytes, preimage_digest) = unsigned_request(&request)?;
    write_create_new(output, &bytes)?;
    Ok(AssertionRequestOutcome {
        preimage_digest,
        request_digest: hex_digest(&bytes),
    })
}

pub fn assemble_assertion(
    root: &Path,
    request: &Path,
    signature: &Path,
    output: &Path,
) -> Result<String> {
    let request_bytes = read_absolute(request, 16_384, None, OwnerPolicy::Invoker)
        .context("cannot read assertion request")?;
    let signature_bytes = read_absolute(signature, 64, Some(64), OwnerPolicy::Invoker)
        .context("cannot read assertion signature")?;
    let bytes = super::signed_envelope::assemble(&request_bytes, &signature_bytes)?;
    let now = OffsetDateTime::now_utc();
    let store = load_fixed_trust_store(now)?;
    let map = match super::signed_envelope::decode_canonical(&bytes)? {
        super::signed_envelope::CborValue::Map(map) => map,
        _ => bail!("assembled envelope must be a map"),
    };
    let key_id = match map.get(&2) {
        Some(super::signed_envelope::CborValue::Bytes(value)) => value,
        _ => bail!("assembled envelope key id is missing"),
    };
    let key = store
        .keys
        .iter()
        .find(|key| key.usable_at(now) && key.key_id == *key_id)
        .context("assembled envelope key is not active")?;
    let trust = parse_hex(&store.digest, "trust identity")?;
    let envelope = verify_envelope(&bytes, &key.public_key, &trust, now)?;
    let conn = open_authority_migration_project(root)?;
    let project = project_id(&conn)?;
    let expected_project = parse_hex::<32>(
        &domain_digest(
            b"AWB-RECOVERY-PROJECT-v1\0",
            &CanonicalValue::object([(
                "project",
                CanonicalValue::string(signed_source_id(project)?),
            )]),
        ),
        "project digest",
    )?;
    if envelope.project_digest != expected_project {
        bail!("assembled envelope project does not match this project");
    }
    write_create_new(output, &bytes)?;
    Ok(super::signed_envelope::hex_digest(&bytes))
}

fn write_create_new(output: &Path, bytes: &[u8]) -> Result<()> {
    write_new_absolute(output, bytes)
}

pub fn verify_provider(root: &Path) -> Result<String> {
    let store = load_fixed_trust_store(OffsetDateTime::now_utc())?;
    let mut conn = open_authority_migration_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    tx.execute(
        "insert or ignore into authority_provider_snapshots(project_id,provider,trust_digest,verified_at) values(?1,'signed-envelope-v1',?2,current_timestamp)",
        params![project, store.digest],
    )?;
    tx.commit()?;
    Ok(store.digest)
}

pub fn import_assertion(
    root: &Path,
    expected_purpose: &str,
    path: &Path,
) -> Result<AssertionImportOutcome> {
    let bytes = read_absolute(path, 16_384, None, OwnerPolicy::Invoker)?;
    let now = OffsetDateTime::now_utc();
    let store = load_fixed_trust_store(now)?;
    let trust: [u8; 32] = hex_to_array(&store.digest)?;

    // Key id is field 2. Decode canonically before selecting the pinned key.
    let root_value = super::signed_envelope::decode_canonical(&bytes)?;
    let super::signed_envelope::CborValue::Map(map) = root_value else {
        bail!("assertion envelope must be a map");
    };
    let key_id = match map.get(&2) {
        Some(super::signed_envelope::CborValue::Bytes(value)) => value,
        _ => bail!("assertion key id is missing"),
    };
    let key = store
        .keys
        .iter()
        .find(|key| key.usable_at(now) && key.key_id == *key_id)
        .context("assertion key is not active in the pinned trust store")?;
    let envelope = verify_envelope(&bytes, &key.public_key, &trust, now)?;
    let purpose = purpose_name(envelope.purpose)?;
    if purpose != expected_purpose {
        bail!("assertion purpose does not match --purpose");
    }
    let subject_kind = subject_kind_name(envelope.subject_kind)?;
    let assertion_handle =
        AssertionHandle::derive_raw(b"agent-workbench:assertion-v1\0", &[&bytes]);
    let assertion_id_hex = hex(&envelope.assertion_id);
    let nonce_hex = hex(&envelope.nonce);
    let subject_digest = hex(&envelope.subject_digest);
    let key_id_hex = hex(&envelope.key_id);
    let project_digest = hex(&envelope.project_digest);
    let trust_digest = hex(&envelope.trust_identity);
    let payload_cbor = super::signed_envelope::encode_value(&envelope.payload)?;

    let mut conn = open_authority_migration_project(root)?;
    let tx = conn.transaction()?;
    let project = project_id(&tx)?;
    let expected_project = domain_digest(
        b"AWB-RECOVERY-PROJECT-v1\0",
        &CanonicalValue::object([(
            "project",
            CanonicalValue::string(signed_source_id(project)?),
        )]),
    );
    if project_digest != expected_project {
        bail!("assertion project does not match this project");
    }
    let existing: Option<(String, String)> = tx.query_row(
        "select assertion_digest,purpose from authority_assertions where project_id=?1 and assertion_digest=?2",
        params![project, envelope.digest], |row| Ok((row.get(0)?,row.get(1)?)),
    ).optional()?;
    if let Some((digest, stored_purpose)) = existing {
        if stored_purpose != purpose {
            bail!("stored assertion purpose mismatch");
        }
        return Ok(AssertionImportOutcome {
            assertion_handle: assertion_handle.as_str().to_string(),
            assertion_digest: digest,
            purpose: stored_purpose,
        });
    }
    tx.execute(
        r#"insert into authority_assertions(project_id,provider,purpose,assertion_digest,assertion_id,nonce,key_id,subject_kind,subject_digest,project_digest,trust_digest,payload_digest,payload_cbor,envelope_cbor,issued_at,expires_at,created_at)
           values(?1,'signed-envelope-v1',?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,current_timestamp)"#,
        params![project,purpose,envelope.digest,assertion_id_hex,nonce_hex,key_id_hex,subject_kind,subject_digest,project_digest,trust_digest,envelope.payload_digest,payload_cbor,bytes,envelope.issued_at.to_string(),envelope.expires_at.to_string()],
    )?;
    tx.commit()?;
    Ok(AssertionImportOutcome {
        assertion_handle: assertion_handle.as_str().to_string(),
        assertion_digest: envelope.digest,
        purpose: purpose.to_string(),
    })
}

pub fn resolve_principal(
    root: &Path,
    assertion_handle: &str,
) -> Result<PrincipalResolutionOutcome> {
    let parsed = AssertionHandle::parse(assertion_handle)?;
    let conn = open_existing_project(root)?;
    let project = project_id(&conn)?;
    let digest = parsed
        .as_str()
        .strip_prefix("assertion_")
        .context("invalid assertion handle")?;
    let (kind, subject): (String,String) = conn.query_row(
        "select subject_kind,subject_digest from authority_assertions where project_id=?1 and assertion_digest=?2",
        params![project,digest], |row| Ok((row.get(0)?,row.get(1)?)),
    ).context("assertion handle is not imported in this project")?;
    let principal = PrincipalHandle::derive(
        b"agent-workbench:principal-v1\0",
        &CanonicalValue::object([
            ("provider", CanonicalValue::string("signed-envelope-v1")),
            ("subject_kind", CanonicalValue::string(kind.clone())),
            ("subject_digest", CanonicalValue::string(subject.clone())),
        ]),
    );
    conn.execute(
        "insert or ignore into authority_principals(project_id,principal_handle,provider,subject_kind,subject_digest,created_at) values(?1,?2,'signed-envelope-v1',?3,?4,current_timestamp)",
        params![project,principal.as_str(),kind,subject],
    )?;
    Ok(PrincipalResolutionOutcome {
        principal_handle: principal.as_str().to_string(),
        subject_kind: kind,
    })
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn hex_to_array(value: &str) -> Result<[u8; 32]> {
    if value.len() != 64 {
        bail!("invalid trust digest");
    }
    let mut out = [0u8; 32];
    for (index, slot) in out.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)?;
    }
    Ok(out)
}

pub(crate) fn stored_assertion_payload(
    conn: &rusqlite::Connection,
    project: i64,
    assertion_handle: &str,
    expected_purpose: &str,
    expected_subject_digest: &str,
) -> Result<(i64, super::signed_envelope::CborValue)> {
    let assertion = AssertionHandle::parse(assertion_handle)?;
    let digest = assertion
        .as_str()
        .strip_prefix("assertion_")
        .context("invalid assertion handle")?;
    let stored: StoredAssertion = conn.query_row(
        "select id,purpose,subject_digest,project_digest,trust_digest,key_id,assertion_digest,payload_digest,payload_cbor,envelope_cbor,expires_at,consumed_at from authority_assertions where project_id=?1 and assertion_digest=?2",
        params![project,digest], |row| Ok(StoredAssertion{id:row.get(0)?,purpose:row.get(1)?,subject:row.get(2)?,project_digest:row.get(3)?,trust_digest:row.get(4)?,key_id:row.get(5)?,assertion_digest:row.get(6)?,payload_digest:row.get(7)?,payload:row.get(8)?,envelope:row.get(9)?,expires:row.get(10)?,consumed:row.get(11)?}),
    ).context("assertion handle is not imported in this project")?;
    let StoredAssertion {
        id,
        purpose,
        subject,
        project_digest,
        trust_digest,
        key_id,
        assertion_digest,
        payload_digest,
        payload,
        envelope,
        expires,
        consumed,
    } = stored;
    let now = OffsetDateTime::now_utc();
    let store = load_fixed_trust_store(now).context("current pinned trust store is unavailable")?;
    if store.digest != trust_digest {
        bail!("trust_rotated");
    }
    let stored_key_id = parse_hex::<16>(&key_id, "stored assertion key id")?;
    let key = store
        .keys
        .iter()
        .find(|key| key.usable_at(now) && key.key_id == stored_key_id)
        .context("assertion key is not currently active")?;
    let verified = verify_envelope(
        &envelope,
        &key.public_key,
        &parse_hex::<32>(&store.digest, "trust identity")?,
        now,
    )?;
    if verified.digest != assertion_digest
        || purpose_name(verified.purpose)? != purpose
        || hex(&verified.subject_digest) != subject
        || hex(&verified.project_digest) != project_digest
        || verified.payload_digest != payload_digest
        || super::signed_envelope::encode_value(&verified.payload)? != payload
    {
        bail!("stored assertion failed current verification");
    }
    if purpose != expected_purpose {
        bail!("assertion purpose mismatch");
    }
    if subject != expected_subject_digest {
        bail!("assertion subject mismatch");
    }
    if consumed.is_some() {
        bail!("assertion_replayed");
    }
    let expires: i64 = expires
        .parse()
        .context("stored assertion expiry is invalid")?;
    if now.unix_timestamp() >= expires {
        bail!("assertion_expired");
    }
    if hex_digest(&payload) != payload_digest {
        bail!("stored assertion payload digest mismatch");
    }
    Ok((id, super::signed_envelope::decode_canonical(&payload)?))
}
