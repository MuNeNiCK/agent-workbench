use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Result, bail};
use rusqlite::Connection;

struct SourceFamilyContract {
    name: &'static str,
    since_schema: i64,
    required_columns: &'static [&'static str],
    ownership_foreign_keys: &'static [RequiredForeignKey],
}

struct RequiredForeignKey {
    from: &'static str,
    target: &'static str,
    to: &'static str,
}

const fn foreign_key(
    from: &'static str,
    target: &'static str,
    to: &'static str,
) -> RequiredForeignKey {
    RequiredForeignKey { from, target, to }
}

const fn family(
    name: &'static str,
    since_schema: i64,
    required_columns: &'static [&'static str],
    ownership_foreign_keys: &'static [RequiredForeignKey],
) -> SourceFamilyContract {
    SourceFamilyContract {
        name,
        since_schema,
        required_columns,
        ownership_foreign_keys,
    }
}

// This is the task-history input contract, not an inventory of the product's
// storage. Adding an unrelated product relation must not change whether the
// task-history migration can run or alter its source identity.
const SOURCE_FAMILIES: &[SourceFamilyContract] = &[
    family("projects", 6, &["id", "root_path"], &[]),
    family(
        "work_units",
        6,
        &["id", "project_id"],
        &[foreign_key("project_id", "projects", "id")],
    ),
    family(
        "tasks",
        6,
        &["id", "work_unit_id", "title", "details", "status"],
        &[foreign_key("work_unit_id", "work_units", "id")],
    ),
    family(
        "design_versions",
        6,
        &["id", "design_package_id", "version_number"],
        &[],
    ),
    family(
        "design_requirements",
        6,
        &[
            "id",
            "design_version_id",
            "requirement_key",
            "revision",
            "requirement_text",
            "priority",
            "required_surfaces",
            "status",
        ],
        &[foreign_key("design_version_id", "design_versions", "id")],
    ),
    family(
        "task_derivations",
        6,
        &[
            "id",
            "task_id",
            "design_requirement_id",
            "checklist_item_id",
            "status",
        ],
        &[
            foreign_key("task_id", "tasks", "id"),
            foreign_key("design_requirement_id", "design_requirements", "id"),
        ],
    ),
    family(
        "validation_gate_templates",
        6,
        &[
            "id",
            "design_version_id",
            "gate_key",
            "expected_result",
            "stage",
            "gate_text",
        ],
        &[foreign_key("design_version_id", "design_versions", "id")],
    ),
    family(
        "validation_gate_template_requirements",
        6,
        &["id", "validation_gate_template_id", "design_requirement_id"],
        &[
            foreign_key(
                "validation_gate_template_id",
                "validation_gate_templates",
                "id",
            ),
            foreign_key("design_requirement_id", "design_requirements", "id"),
        ],
    ),
    family(
        "work_phases",
        6,
        &["id", "work_unit_id", "status"],
        &[foreign_key("work_unit_id", "work_units", "id")],
    ),
    family(
        "work_phase_task_memberships",
        6,
        &["id", "phase_id", "task_id"],
        &[
            foreign_key("phase_id", "work_phases", "id"),
            foreign_key("task_id", "tasks", "id"),
        ],
    ),
    family(
        "work_phase_dependencies",
        6,
        &["id", "from_phase_id", "to_phase_id", "status"],
        &[
            foreign_key("from_phase_id", "work_phases", "id"),
            foreign_key("to_phase_id", "work_phases", "id"),
        ],
    ),
    family(
        "checklists",
        6,
        &["id", "work_unit_id", "status"],
        &[foreign_key("work_unit_id", "work_units", "id")],
    ),
    family(
        "checklist_items",
        6,
        &[
            "id",
            "checklist_id",
            "design_requirement_id",
            "task_id",
            "item_order",
            "status",
        ],
        &[
            foreign_key("checklist_id", "checklists", "id"),
            foreign_key("design_requirement_id", "design_requirements", "id"),
            foreign_key("task_id", "tasks", "id"),
        ],
    ),
    family(
        "acceptance_records",
        6,
        &["id", "target_type", "checklist_item_id", "status"],
        &[foreign_key("checklist_item_id", "checklist_items", "id")],
    ),
    family(
        "implementation_evidence",
        6,
        &[
            "id",
            "task_id",
            "evidence_type",
            "commit_sha",
            "file_path",
            "line_ref",
            "symbol",
            "artifact_path",
            "note",
        ],
        &[foreign_key("task_id", "tasks", "id")],
    ),
    family(
        "coverage_items",
        6,
        &[
            "id",
            "work_unit_id",
            "task_id",
            "status",
            "runtime_boundary_evidence",
            "ux_boundary_evidence",
            "lifecycle_boundary_evidence",
            "tests_or_gates",
            "missing_or_unverified",
        ],
        &[
            foreign_key("work_unit_id", "work_units", "id"),
            foreign_key("task_id", "tasks", "id"),
        ],
    ),
    family(
        "validation_gates",
        6,
        &["id", "work_unit_id", "task_id", "expected_result"],
        &[
            foreign_key("work_unit_id", "work_units", "id"),
            foreign_key("task_id", "tasks", "id"),
        ],
    ),
    family(
        "validation_runs",
        6,
        &[
            "id",
            "validation_gate_id",
            "work_unit_id",
            "task_id",
            "result",
            "artifact_path",
            "artifact_hash",
            "notes",
        ],
        &[
            foreign_key("validation_gate_id", "validation_gates", "id"),
            foreign_key("work_unit_id", "work_units", "id"),
            foreign_key("task_id", "tasks", "id"),
        ],
    ),
    family(
        "phase_epochs",
        16,
        &["id", "work_unit_id", "state"],
        &[foreign_key("work_unit_id", "work_units", "id")],
    ),
    family(
        "phase_epoch_sources",
        16,
        &["id", "phase_epoch_id", "source_phase_id"],
        &[
            foreign_key("phase_epoch_id", "phase_epochs", "id"),
            foreign_key("source_phase_id", "work_phases", "id"),
        ],
    ),
    family(
        "phase_epoch_dependencies",
        16,
        &["id", "from_phase_epoch_id", "to_phase_epoch_id", "state"],
        &[
            foreign_key("from_phase_epoch_id", "phase_epochs", "id"),
            foreign_key("to_phase_epoch_id", "phase_epochs", "id"),
        ],
    ),
    family(
        "phase_epoch_dependency_sources",
        16,
        &["id", "phase_epoch_dependency_id", "source_dependency_id"],
        &[
            foreign_key(
                "phase_epoch_dependency_id",
                "phase_epoch_dependencies",
                "id",
            ),
            foreign_key("source_dependency_id", "work_phase_dependencies", "id"),
        ],
    ),
    family(
        "validation_link_retirements",
        23,
        &["id", "validation_run_id"],
        &[foreign_key("validation_run_id", "validation_runs", "id")],
    ),
];

pub(super) fn validate(conn: &Connection, schema_version: i64) -> Result<()> {
    if profile_id(schema_version).is_none() {
        bail!("task-history migration source profile is unsupported");
    }
    for family in active_families(schema_version) {
        validate_family(conn, family)?;
    }
    Ok(())
}

fn validate_family(conn: &Connection, family: &SourceFamilyContract) -> Result<()> {
    let escaped = family.name.replace('"', "\"\"");
    let mut statement = conn.prepare(&format!("pragma table_info(\"{escaped}\")"))?;
    let columns = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, i64>(5)?))
        })?
        .collect::<rusqlite::Result<BTreeMap<_, _>>>()?;
    if columns.is_empty() {
        bail!(
            "task-history migration source contract is not satisfied: required relation {} is absent",
            family.name
        );
    }
    if columns.get("id").copied().unwrap_or_default() == 0 {
        bail!(
            "task-history migration source contract is not satisfied: relation {} has no stable identity",
            family.name
        );
    }
    let missing = family
        .required_columns
        .iter()
        .filter(|column| !columns.contains_key(**column))
        .copied()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        bail!(
            "task-history migration source contract is not satisfied: relation {} lacks required fields {}",
            family.name,
            missing.join(",")
        );
    }

    let mut foreign_key_statement =
        conn.prepare(&format!("pragma foreign_key_list(\"{escaped}\")"))?;
    let foreign_keys = foreign_key_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(3)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<BTreeSet<_>>>()?;
    for required in family.ownership_foreign_keys {
        if !foreign_keys.contains(&(
            required.from.to_string(),
            required.target.to_string(),
            Some(required.to.to_string()),
        )) {
            bail!(
                "task-history migration source contract is not satisfied: relation {} lost required ownership link",
                family.name
            );
        }
    }
    Ok(())
}

pub(super) fn profile_id(schema_version: i64) -> Option<String> {
    (6..=crate::db::SCHEMA_VERSION)
        .contains(&schema_version)
        .then(|| format!("task-history-source-v{schema_version}"))
}

fn active_families(schema_version: i64) -> impl Iterator<Item = &'static SourceFamilyContract> {
    SOURCE_FAMILIES
        .iter()
        .filter(move |family| family.since_schema <= schema_version)
}

pub(super) fn source_families(schema_version: i64) -> Vec<&'static str> {
    active_families(schema_version)
        .map(|family| family.name)
        .collect()
}
