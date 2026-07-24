use anyhow::{Result, bail};
use rusqlite::{Connection, types::ValueRef};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConservationSnapshot {
    tables: Vec<TableProjection>,
    text_mutations: Vec<DeclaredTextMutation>,
    integer_mutations: Vec<DeclaredIntegerMutation>,
    pub(super) digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeclaredTextMutation {
    pub(crate) table: String,
    pub(crate) key_column: String,
    pub(crate) key_value: i64,
    pub(crate) column: String,
    pub(crate) before: String,
    pub(crate) after: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DeclaredIntegerMutation {
    pub(crate) table: String,
    pub(crate) key_column: String,
    pub(crate) key_value: i64,
    pub(crate) column: String,
    pub(crate) before: i64,
    pub(crate) after: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TableProjection {
    name: String,
    columns: Vec<String>,
}

pub(super) fn capture_product_facts(
    conn: &Connection,
    excluded_tables: &[&str],
) -> Result<ConservationSnapshot> {
    let mut statement = conn.prepare(
        "select name from sqlite_schema where type='table' and name not like 'sqlite_%' order by name",
    )?;
    let table_names = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .filter(|table| !excluded_tables.contains(&table.as_str()))
        .collect::<Vec<_>>();
    let tables = table_names
        .into_iter()
        .map(|name| {
            Ok(TableProjection {
                columns: columns(conn, &name)?,
                name,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let digest = digest_projections(conn, &tables, &[], &[])?;
    Ok(ConservationSnapshot {
        tables,
        text_mutations: Vec::new(),
        integer_mutations: Vec::new(),
        digest,
    })
}

pub(super) fn capture_product_facts_with_text_mutations(
    conn: &Connection,
    excluded_tables: &[&str],
    text_mutations: Vec<DeclaredTextMutation>,
) -> Result<ConservationSnapshot> {
    capture_product_facts_with_mutations(conn, excluded_tables, text_mutations, Vec::new())
}

pub(super) fn capture_product_facts_with_mutations(
    conn: &Connection,
    excluded_tables: &[&str],
    text_mutations: Vec<DeclaredTextMutation>,
    integer_mutations: Vec<DeclaredIntegerMutation>,
) -> Result<ConservationSnapshot> {
    let mut snapshot = capture_product_facts(conn, excluded_tables)?;
    validate_declared_text_mutations(conn, &snapshot.tables, &text_mutations, false)?;
    validate_declared_integer_mutations(conn, &snapshot.tables, &integer_mutations, false)?;
    snapshot.text_mutations = text_mutations;
    snapshot.integer_mutations = integer_mutations;
    Ok(snapshot)
}

pub(super) fn capture_named_product_facts(
    conn: &Connection,
    table_names: &[&str],
) -> Result<ConservationSnapshot> {
    let tables = table_names
        .iter()
        .filter_map(|name| match columns(conn, name) {
            Ok(columns) if !columns.is_empty() => Some(Ok(TableProjection {
                name: (*name).to_string(),
                columns,
            })),
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>>>()?;
    let digest = digest_projections(conn, &tables, &[], &[])?;
    Ok(ConservationSnapshot {
        tables,
        text_mutations: Vec::new(),
        integer_mutations: Vec::new(),
        digest,
    })
}

pub(super) fn verify_product_facts(conn: &Connection, source: &ConservationSnapshot) -> Result<()> {
    validate_declared_text_mutations(conn, &source.tables, &source.text_mutations, true)?;
    validate_declared_integer_mutations(conn, &source.tables, &source.integer_mutations, true)?;
    let target = digest_projections(
        conn,
        &source.tables,
        &source.text_mutations,
        &source.integer_mutations,
    )?;
    if target != source.digest {
        bail!("transition did not conserve an unaffected product fact");
    }
    Ok(())
}

pub(crate) fn semantic_ledger_identity(conn: &Connection) -> Result<String> {
    let mut statement = conn.prepare(
        "select name from sqlite_schema where type='table' and name not like 'sqlite_%' order by name",
    )?;
    let tables = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    digest_tables(conn, &tables)
}

fn digest_tables(conn: &Connection, tables: &[String]) -> Result<String> {
    let projections = tables
        .iter()
        .map(|name| {
            Ok(TableProjection {
                columns: columns(conn, name)?,
                name: name.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    digest_projections(conn, &projections, &[], &[])
}

fn digest_projections(
    conn: &Connection,
    tables: &[TableProjection],
    text_mutations: &[DeclaredTextMutation],
    integer_mutations: &[DeclaredIntegerMutation],
) -> Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/conserved-product-facts/v1\0");
    for table in tables {
        if table.columns.is_empty() {
            bail!("conserved aggregate family has no readable fields");
        }
        let target_columns = columns(conn, &table.name)?;
        if table
            .columns
            .iter()
            .any(|column| !target_columns.contains(column))
        {
            bail!("transition removed a field from a conserved aggregate family");
        }
        hasher.update(table.name.as_bytes());
        hasher.update(b"\0");
        for column in &table.columns {
            hasher.update(column.as_bytes());
            hasher.update(b"\0");
        }
        let projection = table
            .columns
            .iter()
            .map(|column| quote_identifier(column))
            .collect::<Vec<_>>()
            .join(",");
        let query = format!(
            "select {projection} from {} order by {projection}",
            quote_identifier(&table.name)
        );
        let mut statement = conn.prepare(&query)?;
        let column_count = table.columns.len();
        let mut rows = statement.query([])?;
        while let Some(row) = rows.next()? {
            let row_mutations = text_mutations
                .iter()
                .filter(|mutation| mutation.table == table.name)
                .filter(|mutation| {
                    table
                        .columns
                        .iter()
                        .position(|column| column == &mutation.key_column)
                        .and_then(|index| row.get_ref(index).ok())
                        .is_some_and(|value| {
                            matches!(value, ValueRef::Integer(actual) if actual == mutation.key_value)
                        })
                })
                .collect::<Vec<_>>();
            let row_integer_mutations = integer_mutations
                .iter()
                .filter(|mutation| mutation.table == table.name)
                .filter(|mutation| {
                    table
                        .columns
                        .iter()
                        .position(|column| column == &mutation.key_column)
                        .and_then(|index| row.get_ref(index).ok())
                        .is_some_and(|value| {
                            matches!(value, ValueRef::Integer(actual) if actual == mutation.key_value)
                        })
                })
                .collect::<Vec<_>>();
            hasher.update(b"row\0");
            for index in 0..column_count {
                let normalized = row_mutations
                    .iter()
                    .find(|mutation| mutation.column == table.columns[index]);
                let normalized_integer = row_integer_mutations
                    .iter()
                    .find(|mutation| mutation.column == table.columns[index]);
                let value = row.get_ref(index)?;
                match (value, normalized, normalized_integer) {
                    (ValueRef::Text(_), Some(mutation), None) => {
                        hasher.update(b"text\0");
                        hasher.update((mutation.before.len() as u64).to_be_bytes());
                        hasher.update(mutation.before.as_bytes());
                    }
                    (ValueRef::Integer(_), None, Some(mutation)) => {
                        hasher.update(b"integer\0");
                        hasher.update(mutation.before.to_be_bytes());
                    }
                    (ValueRef::Null, None, None) => hasher.update(b"null\0"),
                    (ValueRef::Integer(value), None, None) => {
                        hasher.update(b"integer\0");
                        hasher.update(value.to_be_bytes());
                    }
                    (ValueRef::Real(value), None, None) => {
                        hasher.update(b"real\0");
                        hasher.update(value.to_bits().to_be_bytes());
                    }
                    (ValueRef::Text(value), None, None) => {
                        hasher.update(b"text\0");
                        hasher.update((value.len() as u64).to_be_bytes());
                        hasher.update(value);
                    }
                    (ValueRef::Blob(value), None, None) => {
                        hasher.update(b"blob\0");
                        hasher.update((value.len() as u64).to_be_bytes());
                        hasher.update(value);
                    }
                    (_, Some(_), _) => {
                        bail!("declared text mutation does not target a text value")
                    }
                    (_, _, Some(_)) => {
                        bail!("declared integer mutation does not target an integer value")
                    }
                    #[allow(unreachable_patterns)]
                    _ => unreachable!(),
                }
                hasher.update(b"\0");
            }
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn validate_declared_integer_mutations(
    conn: &Connection,
    tables: &[TableProjection],
    mutations: &[DeclaredIntegerMutation],
    target: bool,
) -> Result<()> {
    for (index, mutation) in mutations.iter().enumerate() {
        if mutations[..index].iter().any(|candidate| {
            candidate.table == mutation.table
                && candidate.key_column == mutation.key_column
                && candidate.key_value == mutation.key_value
                && candidate.column == mutation.column
        }) {
            bail!("declared semantic mutation is duplicated");
        }
        let table = tables
            .iter()
            .find(|table| table.name == mutation.table)
            .ok_or_else(|| {
                anyhow::anyhow!("declared semantic mutation targets an excluded table")
            })?;
        if !table.columns.contains(&mutation.key_column)
            || !table.columns.contains(&mutation.column)
        {
            bail!("declared semantic mutation targets an unavailable field");
        }
        let query = format!(
            "select {} from {} where {}=?1",
            quote_identifier(&mutation.column),
            quote_identifier(&mutation.table),
            quote_identifier(&mutation.key_column)
        );
        let values = conn
            .prepare(&query)?
            .query_map([mutation.key_value], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let expected = if target {
            mutation.after
        } else {
            mutation.before
        };
        if values.len() != 1 || values[0] != expected {
            bail!("declared semantic mutation does not match its exact stored value");
        }
    }
    Ok(())
}

fn validate_declared_text_mutations(
    conn: &Connection,
    tables: &[TableProjection],
    mutations: &[DeclaredTextMutation],
    target: bool,
) -> Result<()> {
    for (index, mutation) in mutations.iter().enumerate() {
        if mutations[..index].iter().any(|candidate| {
            candidate.table == mutation.table
                && candidate.key_column == mutation.key_column
                && candidate.key_value == mutation.key_value
                && candidate.column == mutation.column
        }) {
            bail!("declared semantic mutation is duplicated");
        }
        let table = tables
            .iter()
            .find(|table| table.name == mutation.table)
            .ok_or_else(|| {
                anyhow::anyhow!("declared semantic mutation targets an excluded table")
            })?;
        if !table.columns.contains(&mutation.key_column)
            || !table.columns.contains(&mutation.column)
        {
            bail!("declared semantic mutation targets an unavailable field");
        }
        let query = format!(
            "select {} from {} where {}=?1",
            quote_identifier(&mutation.column),
            quote_identifier(&mutation.table),
            quote_identifier(&mutation.key_column)
        );
        let values = conn
            .prepare(&query)?
            .query_map([mutation.key_value], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let expected = if target {
            &mutation.after
        } else {
            &mutation.before
        };
        if values.len() != 1 || values[0] != *expected {
            bail!("declared semantic mutation does not match its exact stored value");
        }
    }
    Ok(())
}

fn columns(conn: &Connection, table: &str) -> Result<Vec<String>> {
    let mut statement = conn.prepare(&format!(
        "select name from pragma_table_info({}) order by cid",
        quote_literal(table)
    ))?;
    statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn quote_identifier(identifier: &str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn quote_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
