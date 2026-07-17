use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use minicbor::{Decoder, Encoder, data::Type};
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

pub const PROVIDER: &str = "signed-envelope-v1";
pub const SIGNING_DOMAIN: &[u8] = b"agent-workbench:signed-envelope-v1\0";
pub const MAX_ENVELOPE_BYTES: usize = 16_384;

#[derive(Clone, Debug)]
pub struct UnsignedEnvelopeRequest {
    pub key_id: [u8; 16],
    pub purpose: u64,
    pub assertion_id: [u8; 16],
    pub nonce: [u8; 16],
    pub issued: i64,
    pub expires: i64,
    pub subject_kind: u64,
    pub subject_digest: [u8; 32],
    pub project_digest: [u8; 32],
    pub payload: CborValue,
    pub trust_identity: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CborValue {
    U64(u64),
    I64(i64),
    Bytes(Vec<u8>),
    Text(String),
    Array(Vec<CborValue>),
    Map(BTreeMap<u64, CborValue>),
}

#[derive(Clone, Debug)]
pub struct VerifiedEnvelope {
    pub digest: String,
    pub purpose: u64,
    pub assertion_id: Vec<u8>,
    pub nonce: Vec<u8>,
    pub key_id: Vec<u8>,
    pub issued_at: i64,
    pub expires_at: i64,
    pub subject_kind: u64,
    pub subject_digest: Vec<u8>,
    pub project_digest: Vec<u8>,
    pub payload: CborValue,
    pub payload_digest: String,
    pub trust_identity: Vec<u8>,
}

pub fn purpose_name(value: u64) -> Result<&'static str> {
    match value {
        0 => Ok("root_grant"),
        1 => Ok("grant_delegate"),
        2 => Ok("grant_revoke"),
        3 => Ok("capability_issue"),
        4 => Ok("review_provenance"),
        5 => Ok("legacy_reviewer_binding"),
        _ => bail!("unsupported assertion purpose"),
    }
}

pub fn subject_kind_name(value: u64) -> Result<&'static str> {
    match value {
        0 => Ok("human"),
        1 => Ok("agent"),
        2 => Ok("service"),
        _ => bail!("unsupported assertion subject kind"),
    }
}

pub fn verify_envelope(
    bytes: &[u8],
    public_key: &[u8; 32],
    expected_trust_identity: &[u8; 32],
    now: OffsetDateTime,
) -> Result<VerifiedEnvelope> {
    if bytes.is_empty() || bytes.len() > MAX_ENVELOPE_BYTES {
        bail!("signed envelope size is outside the supported bound");
    }
    let value = decode_canonical(bytes)?;
    let CborValue::Map(map) = value else {
        bail!("signed envelope must be a CBOR map");
    };
    if map.len() != 13 || map.keys().copied().ne(0..=12) {
        bail!("signed envelope must contain every key 0..12 exactly once");
    }
    expect_u64(&map, 0, "schema")?
        .eq(&1)
        .then_some(())
        .context("unsupported envelope schema")?;
    if expect_text(&map, 1, "provider")? != PROVIDER {
        bail!("unsupported authority provider");
    }
    let key_id = expect_bytes(&map, 2, "key id")?.to_vec();
    if key_id.len() != 16 {
        bail!("key id must be exactly 16 bytes");
    }
    let purpose = expect_u64(&map, 3, "purpose")?;
    purpose_name(purpose)?;
    let assertion_id = exact_bytes(&map, 4, 16, "assertion id")?;
    let nonce = exact_bytes(&map, 5, 16, "nonce")?;
    let issued_at = expect_i64(&map, 6, "issued")?;
    let expires_at = expect_i64(&map, 7, "expires")?;
    validate_time_window(issued_at, expires_at, now)?;
    let subject = expect_map(&map, 8, "subject")?;
    if subject.len() != 2 || subject.keys().copied().ne(0..=1) {
        bail!("subject must contain keys 0 and 1");
    }
    let subject_kind = expect_u64(subject, 0, "subject kind")?;
    subject_kind_name(subject_kind)?;
    let subject_digest = exact_bytes(subject, 1, 32, "subject digest")?;
    let project_digest = exact_bytes(&map, 9, 32, "project")?;
    let payload = map.get(&10).cloned().context("missing purpose payload")?;
    let signature_bytes = exact_bytes(&map, 11, 64, "signature")?;
    let trust_identity = exact_bytes(&map, 12, 32, "trust identity")?;
    if trust_identity.as_slice() != expected_trust_identity {
        bail!("trust identity does not match the pinned trust store");
    }

    let mut unsigned = map.clone();
    unsigned.remove(&11);
    let unsigned_bytes = encode_value(&CborValue::Map(unsigned))?;
    let mut preimage = SIGNING_DOMAIN.to_vec();
    preimage.extend_from_slice(&unsigned_bytes);
    let key = VerifyingKey::from_bytes(public_key)?;
    let signature = Signature::from_slice(&signature_bytes)?;
    key.verify(&preimage, &signature)
        .context("signed envelope signature verification failed")?;

    let canonical_bytes = encode_value(&CborValue::Map(map))?;
    if canonical_bytes != bytes {
        bail!("signed envelope is not canonical CBOR");
    }
    let digest = hex_digest(bytes);
    let payload_digest = hex_digest(&encode_value(&payload)?);
    Ok(VerifiedEnvelope {
        digest,
        purpose,
        assertion_id,
        nonce,
        key_id,
        issued_at,
        expires_at,
        subject_kind,
        subject_digest,
        project_digest,
        payload,
        payload_digest,
        trust_identity,
    })
}

pub fn assemble(request: &[u8], signature: &[u8]) -> Result<Vec<u8>> {
    if signature.len() != 64 {
        bail!("signature must be exactly 64 bytes");
    }
    let CborValue::Map(mut map) = decode_canonical(request)? else {
        bail!("unsigned request must be a CBOR map");
    };
    if map.contains_key(&11) || map.len() != 12 {
        bail!("unsigned request has an invalid key set");
    }
    map.insert(11, CborValue::Bytes(signature.to_vec()));
    encode_value(&CborValue::Map(map))
}

pub fn unsigned_request(request: &UnsignedEnvelopeRequest) -> Result<(Vec<u8>, String)> {
    purpose_name(request.purpose)?;
    subject_kind_name(request.subject_kind)?;
    if request.expires <= request.issued || request.expires > request.issued.saturating_add(300) {
        bail!("assertion request must have a positive validity window of at most five minutes");
    }
    if request.purpose == 0 && request.subject_kind != 0 {
        bail!("root-grant assertion subject must be human");
    }
    validate_payload(request.purpose, &request.payload)?;
    let subject = CborValue::Map(BTreeMap::from([
        (0, CborValue::U64(request.subject_kind)),
        (1, CborValue::Bytes(request.subject_digest.to_vec())),
    ]));
    let value = CborValue::Map(BTreeMap::from([
        (0, CborValue::U64(1)),
        (1, CborValue::Text(PROVIDER.to_string())),
        (2, CborValue::Bytes(request.key_id.to_vec())),
        (3, CborValue::U64(request.purpose)),
        (4, CborValue::Bytes(request.assertion_id.to_vec())),
        (5, CborValue::Bytes(request.nonce.to_vec())),
        (6, signed_integer(request.issued)),
        (7, signed_integer(request.expires)),
        (8, subject),
        (9, CborValue::Bytes(request.project_digest.to_vec())),
        (10, request.payload.clone()),
        (12, CborValue::Bytes(request.trust_identity.to_vec())),
    ]));
    let bytes = encode_value(&value)?;
    let mut preimage = SIGNING_DOMAIN.to_vec();
    preimage.extend_from_slice(&bytes);
    Ok((bytes, hex_digest(&preimage)))
}

pub fn parse_hex<const N: usize>(value: &str, label: &str) -> Result<[u8; N]> {
    if value.len() != N * 2
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        bail!("{label} must be exactly {} lowercase hex characters", N * 2);
    }
    let mut output = [0_u8; N];
    for (index, slot) in output.iter_mut().enumerate() {
        *slot = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)?;
    }
    Ok(output)
}

pub fn parse_rfc3339_seconds(value: &str, label: &str) -> Result<i64> {
    OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .with_context(|| format!("invalid {label} timestamp"))
        .map(|time| time.unix_timestamp())
}

pub fn digest_reference(domain: &[u8], value: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(value.as_bytes());
    hasher.finalize().into()
}

pub fn subject_value(kind: u64, digest: [u8; 32]) -> CborValue {
    CborValue::Map(BTreeMap::from([
        (0, CborValue::U64(kind)),
        (1, CborValue::Bytes(digest.to_vec())),
    ]))
}

pub fn target_value(value: &str) -> Result<CborValue> {
    if value == "owner_all" {
        return Ok(CborValue::Array(vec![CborValue::U64(0)]));
    }
    let (tag, expected, domain) = if value.starts_with("review_plan:") {
        (1, 1, b"agent-workbench:review-plan-ref-v1\0".as_slice())
    } else if value.starts_with("review_run:") {
        (2, 1, b"agent-workbench:review-run-ref-v1\0".as_slice())
    } else if value.starts_with("finding:") {
        (3, 1, b"agent-workbench:finding-ref-v1\0".as_slice())
    } else if value.starts_with("closure_attempt:") {
        (4, 1, b"agent-workbench:closure-attempt-ref-v1\0".as_slice())
    } else if value.starts_with("review_correction:") {
        (
            5,
            2,
            b"agent-workbench:review-correction-ref-v1\0".as_slice(),
        )
    } else if value.starts_with("finding_epoch:") {
        (6, 2, b"agent-workbench:finding-epoch-ref-v1\0".as_slice())
    } else if value.starts_with("bootstrap_target:") {
        (
            7,
            6,
            b"agent-workbench:bootstrap-target-ref-v1\0".as_slice(),
        )
    } else {
        bail!("target reference uses an unsupported tag");
    };
    let components = value.split(':').skip(1).collect::<Vec<_>>();
    if components.len() != expected || components.iter().any(|item| item.is_empty()) {
        bail!("target reference has the wrong component count");
    }
    let mut result = vec![CborValue::U64(tag)];
    result.extend(
        components
            .into_iter()
            .map(|part| CborValue::Bytes(digest_reference(domain, part).to_vec())),
    );
    Ok(CborValue::Array(result))
}

pub fn closed_set(value: &str, allowed: &[(&str, u64)], label: &str) -> Result<CborValue> {
    let mut values = value
        .split(',')
        .map(str::trim)
        .map(|item| {
            allowed
                .iter()
                .find_map(|(name, code)| (*name == item).then_some(*code))
                .with_context(|| format!("unsupported {label} member"))
        })
        .collect::<Result<Vec<_>>>()?;
    values.sort_unstable();
    if values.is_empty() || values.len() > 32 || values.windows(2).any(|pair| pair[0] == pair[1]) {
        bail!("{label} must contain 1..32 unique members");
    }
    Ok(CborValue::Array(
        values.into_iter().map(CborValue::U64).collect(),
    ))
}

fn signed_integer(value: i64) -> CborValue {
    if value >= 0 {
        CborValue::U64(value as u64)
    } else {
        CborValue::I64(value)
    }
}

fn validate_payload(purpose: u64, payload: &CborValue) -> Result<()> {
    let CborValue::Map(map) = payload else {
        bail!("purpose payload must be a map");
    };
    let expected = match purpose {
        0 => 7,
        1 => 9,
        2 => 4,
        3 => 7,
        4 => 5,
        5 => 3,
        _ => bail!("unsupported assertion purpose"),
    };
    if map.len() != expected || map.keys().copied().ne(0..expected as u64) {
        bail!("purpose payload has an invalid key set");
    }
    Ok(())
}

pub fn decode_canonical(bytes: &[u8]) -> Result<CborValue> {
    let mut decoder = Decoder::new(bytes);
    let value = decode_value(&mut decoder, 0)?;
    if decoder.position() != bytes.len() {
        bail!("trailing CBOR bytes are not allowed");
    }
    if encode_value(&value)? != bytes {
        bail!("CBOR input is not canonical");
    }
    Ok(value)
}

pub fn encode_value(value: &CborValue) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut encoder = Encoder::new(&mut output);
    encode_into(&mut encoder, value)?;
    Ok(output)
}

fn decode_value(decoder: &mut Decoder<'_>, depth: usize) -> Result<CborValue> {
    if depth > 16 {
        bail!("CBOR nesting exceeds the supported bound");
    }
    match decoder.datatype()? {
        Type::U8 | Type::U16 | Type::U32 | Type::U64 => Ok(CborValue::U64(decoder.u64()?)),
        Type::I8 | Type::I16 | Type::I32 | Type::I64 => Ok(CborValue::I64(decoder.i64()?)),
        Type::Bytes => Ok(CborValue::Bytes(decoder.bytes()?.to_vec())),
        Type::String => Ok(CborValue::Text(decoder.str()?.to_string())),
        Type::Array => {
            let length = decoder
                .array()?
                .context("indefinite arrays are not allowed")?;
            if length > 1024 {
                bail!("CBOR array is too large");
            }
            let mut values = Vec::with_capacity(length as usize);
            for _ in 0..length {
                values.push(decode_value(decoder, depth + 1)?);
            }
            Ok(CborValue::Array(values))
        }
        Type::Map => {
            let length = decoder.map()?.context("indefinite maps are not allowed")?;
            if length > 1024 {
                bail!("CBOR map is too large");
            }
            let mut values = BTreeMap::new();
            for _ in 0..length {
                let key = decoder.u64()?;
                if values
                    .insert(key, decode_value(decoder, depth + 1)?)
                    .is_some()
                {
                    bail!("duplicate CBOR map key");
                }
            }
            Ok(CborValue::Map(values))
        }
        other => bail!("unsupported CBOR type {other:?}"),
    }
}

fn encode_into<W: minicbor::encode::Write>(
    encoder: &mut Encoder<W>,
    value: &CborValue,
) -> Result<()>
where
    W::Error: std::error::Error + Send + Sync + 'static,
{
    match value {
        CborValue::U64(value) => {
            encoder.u64(*value)?;
        }
        CborValue::I64(value) => {
            encoder.i64(*value)?;
        }
        CborValue::Bytes(value) => {
            encoder.bytes(value)?;
        }
        CborValue::Text(value) => {
            encoder.str(value)?;
        }
        CborValue::Array(values) => {
            encoder.array(values.len() as u64)?;
            for value in values {
                encode_into(encoder, value)?;
            }
        }
        CborValue::Map(values) => {
            encoder.map(values.len() as u64)?;
            for (key, value) in values {
                encoder.u64(*key)?;
                encode_into(encoder, value)?;
            }
        }
    }
    Ok(())
}

fn validate_time_window(issued: i64, expires: i64, now: OffsetDateTime) -> Result<()> {
    let now = now.unix_timestamp();
    if issued > now || now >= expires || expires > issued.saturating_add(300) {
        bail!("assertion is outside its five-minute validity window");
    }
    Ok(())
}
fn expect_i64(map: &BTreeMap<u64, CborValue>, key: u64, label: &str) -> Result<i64> {
    match map.get(&key) {
        Some(CborValue::I64(value)) => Ok(*value),
        Some(CborValue::U64(value)) => {
            i64::try_from(*value).context(format!("{label} is too large"))
        }
        _ => bail!("{label} has the wrong CBOR type"),
    }
}

fn expect_u64(map: &BTreeMap<u64, CborValue>, key: u64, label: &str) -> Result<u64> {
    match map.get(&key) {
        Some(CborValue::U64(value)) => Ok(*value),
        _ => bail!("{label} has the wrong CBOR type"),
    }
}
fn expect_text<'a>(map: &'a BTreeMap<u64, CborValue>, key: u64, label: &str) -> Result<&'a str> {
    match map.get(&key) {
        Some(CborValue::Text(value)) => Ok(value),
        _ => bail!("{label} has the wrong CBOR type"),
    }
}
fn expect_bytes<'a>(map: &'a BTreeMap<u64, CborValue>, key: u64, label: &str) -> Result<&'a [u8]> {
    match map.get(&key) {
        Some(CborValue::Bytes(value)) => Ok(value),
        _ => bail!("{label} has the wrong CBOR type"),
    }
}
fn expect_map<'a>(
    map: &'a BTreeMap<u64, CborValue>,
    key: u64,
    label: &str,
) -> Result<&'a BTreeMap<u64, CborValue>> {
    match map.get(&key) {
        Some(CborValue::Map(value)) => Ok(value),
        _ => bail!("{label} has the wrong CBOR type"),
    }
}
fn exact_bytes(
    map: &BTreeMap<u64, CborValue>,
    key: u64,
    len: usize,
    label: &str,
) -> Result<Vec<u8>> {
    let value = expect_bytes(map, key, label)?;
    if value.len() != len {
        bail!("{label} must be exactly {len} bytes");
    }
    Ok(value.to_vec())
}
pub fn hex_digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
