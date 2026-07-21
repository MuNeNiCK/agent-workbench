use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, types::ValueRef};

use crate::identity::{CanonicalValue, canonical_bytes, domain_digest, normalize_identifier};

use super::super::profile;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DatabaseSnapshot {
    pub(super) value: CanonicalValue,
    pub(super) digest: String,
    pub(super) profile_id: String,
    pub(super) schema_version: i64,
    pub(super) application_id: i64,
    pub(super) user_version: i64,
    pub(super) families: Vec<FamilySnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct FamilySnapshot {
    pub(super) name: String,
    pub(super) columns: Vec<ColumnSnapshot>,
    pub(super) rows: Vec<RowSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ColumnSnapshot {
    pub(super) name: String,
    pub(super) declared_type: String,
    pub(super) primary_key_order: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RowSnapshot {
    pub(super) identity: CanonicalValue,
    pub(super) values: CanonicalValue,
    pub(super) cells: BTreeMap<String, SqliteScalar>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SqliteScalar {
    Null,
    Integer(i64),
    Real(u64),
    Text(String),
    Blob(Vec<u8>),
}

impl SqliteScalar {
    pub(super) fn canonical(&self) -> CanonicalValue {
        match self {
            Self::Null => CanonicalValue::object([
                ("type", CanonicalValue::string("null")),
                ("value", CanonicalValue::Null),
            ]),
            Self::Integer(value) => CanonicalValue::object([
                ("type", CanonicalValue::string("integer-decimal")),
                ("value", CanonicalValue::string(value.to_string())),
            ]),
            Self::Real(value) => CanonicalValue::object([
                ("type", CanonicalValue::string("real-ieee754-hex")),
                ("value", CanonicalValue::string(format!("{value:016x}"))),
            ]),
            Self::Text(value) => CanonicalValue::object([
                ("type", CanonicalValue::string("text-nfc")),
                ("value", CanonicalValue::string(value.clone())),
            ]),
            Self::Blob(value) => CanonicalValue::object([
                ("type", CanonicalValue::string("blob-hex")),
                ("value", CanonicalValue::string(hex(value))),
            ]),
        }
    }

    pub(super) fn as_positive_id(&self) -> Result<i64> {
        match self {
            Self::Integer(value) if *value > 0 => Ok(*value),
            _ => bail!("task-history migration source identity is invalid"),
        }
    }
}

pub(super) fn read_all(conn: &Connection, schema_version: i64) -> Result<DatabaseSnapshot> {
    let application_id = pragma_i64(conn, "application_id")?;
    let user_version = pragma_i64(conn, "user_version")?;
    let profile_id = profile::profile_id(schema_version)
        .context("task-history migration source profile is unsupported")?;
    let mut families = Vec::new();
    for family in profile::source_families(schema_version) {
        families.push(read_family(conn, family)?);
    }
    let family_values = families.iter().map(family_value).collect::<Vec<_>>();
    let value = CanonicalValue::object([
        ("profile", CanonicalValue::string(profile_id.clone())),
        (
            "sqlite_schema_version",
            CanonicalValue::string(schema_version.to_string()),
        ),
        (
            "application_id",
            CanonicalValue::string(application_id.to_string()),
        ),
        (
            "user_version",
            CanonicalValue::string(user_version.to_string()),
        ),
        ("families", CanonicalValue::Array(family_values)),
    ]);
    let digest = domain_digest(b"AWB-DATABASE-SNAPSHOT-v1\0", &value);
    Ok(DatabaseSnapshot {
        value,
        digest,
        profile_id: profile_id.to_string(),
        schema_version,
        application_id,
        user_version,
        families,
    })
}

fn read_family(conn: &Connection, family: &str) -> Result<FamilySnapshot> {
    let escaped = family.replace('"', "\"\"");
    let mut column_statement = conn.prepare(&format!("pragma table_info(\"{escaped}\")"))?;
    let columns = column_statement
        .query_map([], |row| {
            Ok(ColumnSnapshot {
                name: row.get(1)?,
                declared_type: row.get(2)?,
                primary_key_order: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if columns.is_empty() {
        bail!("task-history migration source family has no declared columns");
    }
    let mut primary = columns
        .iter()
        .enumerate()
        .filter(|(_, column)| column.primary_key_order > 0)
        .map(|(index, column)| (column.primary_key_order, index))
        .collect::<Vec<_>>();
    primary.sort_unstable();
    if primary.is_empty() {
        bail!("task-history migration source family has no primary identity");
    }
    let projection = columns
        .iter()
        .map(|column| format!("\"{}\"", column.name.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(",");
    let mut statement = conn.prepare(&format!("select {projection} from \"{escaped}\""))?;
    let mut rows = statement
        .query_map([], |row| {
            let scalars = (0..columns.len())
                .map(|index| sqlite_value(row.get_ref(index)?))
                .collect::<rusqlite::Result<Vec<_>>>()?;
            let canonical_values = scalars
                .iter()
                .map(SqliteScalar::canonical)
                .collect::<Vec<_>>();
            let identity = primary
                .iter()
                .map(|(_, index)| {
                    (
                        columns[*index].name.clone(),
                        canonical_values[*index].clone(),
                    )
                })
                .collect::<std::collections::BTreeMap<_, _>>();
            let values = columns
                .iter()
                .zip(&canonical_values)
                .map(|(column, value)| (column.name.clone(), value.clone()))
                .collect::<std::collections::BTreeMap<_, _>>();
            let cells = columns
                .iter()
                .zip(scalars)
                .map(|(column, value)| (column.name.clone(), value))
                .collect::<std::collections::BTreeMap<_, _>>();
            Ok(RowSnapshot {
                identity: CanonicalValue::Object(identity),
                values: CanonicalValue::Object(values),
                cells,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.sort_by(|left, right| {
        canonical_bytes(&left.identity)
            .cmp(&canonical_bytes(&right.identity))
            .then_with(|| canonical_bytes(&left.values).cmp(&canonical_bytes(&right.values)))
    });
    for pair in rows.windows(2) {
        if canonical_bytes(&pair[0].identity) == canonical_bytes(&pair[1].identity) {
            bail!("task-history migration source family has duplicate primary identity");
        }
    }
    Ok(FamilySnapshot {
        name: family.to_string(),
        columns,
        rows,
    })
}

fn family_value(family: &FamilySnapshot) -> CanonicalValue {
    CanonicalValue::object([
        ("family", CanonicalValue::string(family.name.clone())),
        (
            "columns",
            CanonicalValue::Array(
                family
                    .columns
                    .iter()
                    .map(|column| {
                        CanonicalValue::object([
                            ("name", CanonicalValue::string(column.name.clone())),
                            ("type", CanonicalValue::string(column.declared_type.clone())),
                        ])
                    })
                    .collect(),
            ),
        ),
        (
            "rows",
            CanonicalValue::Array(family.rows.iter().map(|row| row.values.clone()).collect()),
        ),
    ])
}

fn sqlite_value(value: ValueRef<'_>) -> rusqlite::Result<SqliteScalar> {
    let scalar = match value {
        ValueRef::Null => SqliteScalar::Null,
        ValueRef::Integer(value) => SqliteScalar::Integer(value),
        ValueRef::Real(value) => SqliteScalar::Real(value.to_bits()),
        ValueRef::Text(value) => {
            let value = std::str::from_utf8(value).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    value.len(),
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
            SqliteScalar::Text(normalize_identifier(value))
        }
        ValueRef::Blob(value) => SqliteScalar::Blob(value.to_vec()),
    };
    Ok(scalar)
}

fn pragma_i64(conn: &Connection, name: &str) -> Result<i64> {
    conn.query_row(&format!("pragma {name}"), [], |row| row.get(0))
        .map_err(Into::into)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}
