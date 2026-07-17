use std::collections::BTreeSet;

use anyhow::{Result, bail};
use rusqlite::Connection;

struct Family {
    name: &'static str,
    since_schema: i64,
    until_schema: i64,
}

const FAMILIES: &[Family] = &[
    family("acceptance_records", 6, 13),
    family("artifacts", 6, 13),
    family("authority_assertions", 11, 12),
    family("authority_bootstrap_journals", 11, 12),
    family("authority_bootstrap_targets", 12, 12),
    family("authority_grant_epochs", 11, 12),
    family("authority_migration_sources", 12, 13),
    family("authorities", 6, 13),
    family("authority_events", 6, 13),
    family("authority_principals", 11, 12),
    family("authority_provider_snapshots", 11, 12),
    family("authority_security_audits", 12, 12),
    family("checklist_items", 6, 13),
    family("checklists", 6, 13),
    family("closure_attempts", 7, 13),
    family("closures", 6, 13),
    family("command_deviations", 6, 13),
    family("command_profiles", 6, 13),
    family("command_usages", 6, 13),
    family("correction_application_identity_links", 10, 13),
    family("correction_completion_inheritance_evidence", 11, 13),
    family("correction_completion_inheritance_sources", 11, 13),
    family("correction_sessions", 9, 13),
    family("correction_tokens", 9, 13),
    family("correction_transition_aliases", 9, 13),
    family("correction_transition_applications", 9, 13),
    family("coverage_items", 6, 13),
    family("decisions", 6, 13),
    family("decision_capabilities", 11, 12),
    family("capability_consumption_audits", 12, 12),
    family("capability_issue_audits", 12, 12),
    family("design_decisions", 6, 13),
    family("design_files", 6, 13),
    family("design_packages", 6, 13),
    family("design_requirements", 6, 13),
    family("design_versions", 6, 13),
    family("finding_remediation_bindings", 9, 13),
    family("finding_remediation_recovery_epochs", 9, 13),
    family("finding_verifications", 6, 13),
    family("finding_disposition_decisions", 11, 13),
    family("finding_lifecycle_events", 11, 13),
    family("findings", 6, 13),
    family("git_commits", 6, 13),
    family("git_file_changes", 6, 13),
    family("implementation_evidence", 6, 13),
    family("legacy_claim_audits", 12, 13),
    family("legacy_adjudication_migrations", 12, 13),
    family("legacy_finding_audits", 12, 13),
    family("legacy_reviewer_bindings", 12, 12),
    family("legacy_signed_review_effects", 13, 13),
    family("legacy_migration_candidates", 12, 13),
    family("legacy_migration_candidate_members", 12, 13),
    family("legacy_migration_edges", 12, 13),
    family("legacy_migration_projections", 12, 13),
    family("decision_continuations", 12, 12),
    family("review_correction_events", 12, 13),
    family("review_boundary_snapshots", 12, 13),
    family("review_correction_recovery_obligations", 12, 13),
    family("finding_decision_epochs", 12, 13),
    family("kpt_item_conversions", 6, 13),
    family("kpt_items", 6, 13),
    family("kpt_reviews", 6, 13),
    family("owner_decision_grants", 11, 12),
    family("owner_decisions", 11, 13),
    family("projects", 6, 13),
    family("repositories", 6, 13),
    family("repository_dirty_entries", 6, 13),
    family("repository_snapshot_comparisons", 6, 13),
    family("repository_snapshots", 6, 13),
    family("repository_state_classifications", 6, 13),
    family("resume_check_items", 6, 13),
    family("resume_checks", 6, 13),
    family("review_agent_invocations", 6, 13),
    family("review_invocation_transition_audits", 12, 12),
    family("review_adjudication_decisions", 11, 13),
    family("review_plan_targets", 6, 13),
    family("review_plans", 6, 13),
    family("review_policies", 6, 13),
    family("review_provenance_records", 11, 12),
    family("review_runs", 6, 13),
    family("review_result_stage_audits", 12, 12),
    family("review_result_stage_items", 12, 12),
    family("review_result_stages", 12, 12),
    family("review_scopes", 6, 13),
    family("rule_bindings", 6, 13),
    family("schema_migrations", 6, 13),
    family("schema_retirement_records", 13, 13),
    family("suspend_snapshots", 6, 13),
    family("task_derivations", 6, 13),
    family("tasks", 6, 13),
    family("user_corrections", 6, 13),
    family("validation_gate_template_requirements", 6, 13),
    family("validation_gate_templates", 6, 13),
    family("validation_gates", 6, 13),
    family("validation_link_repair_changes", 8, 13),
    family("validation_link_repair_runs", 8, 13),
    family("validation_runs", 6, 13),
    family("verification_adjudication_decisions", 11, 13),
    family("work_phase_dependencies", 6, 13),
    family("work_phase_events", 6, 13),
    family("work_phase_review_targets", 6, 13),
    family("work_phase_task_memberships", 6, 13),
    family("work_phase_trace_decisions", 6, 13),
    family("work_phases", 6, 13),
    family("work_record_commands", 6, 13),
    family("work_record_commits", 6, 13),
    family("work_record_files", 6, 13),
    family("work_record_forks", 6, 13),
    family("work_records", 6, 13),
    family("work_unit_activations", 6, 13),
    family("work_unit_dependencies", 6, 13),
    family("work_unit_events", 6, 13),
    family("work_units", 6, 13),
];

const TARGET_FAMILIES: &[&str] = &[
    "task_completion_claims",
    "task_completion_sources",
    "task_identity_migration_audits",
    "task_phase_memberships",
    "task_phase_membership_sources",
    "task_identity_dependencies",
    "task_identity_dependency_sources",
    "task_identities",
    "task_revision_aliases",
    "task_revisions",
];

// Schema 13 no longer uses these schema-12 security tables. Migrated ledgers
// may retain them as immutable audit history; they are not task-history input.
const INERT_SCHEMA12_AUDIT_FAMILIES: &[&str] = &[
    "authority_assertions",
    "authority_bootstrap_journals",
    "authority_bootstrap_targets",
    "authority_grant_epochs",
    "authority_principals",
    "authority_provider_snapshots",
    "authority_security_audits",
    "capability_consumption_audits",
    "capability_issue_audits",
    "decision_capabilities",
    "decision_continuations",
    "legacy_reviewer_bindings",
    "owner_decision_grants",
    "review_invocation_transition_audits",
    "review_provenance_records",
    "review_result_stage_audits",
    "review_result_stage_items",
    "review_result_stages",
];

const fn family(name: &'static str, since_schema: i64, until_schema: i64) -> Family {
    Family {
        name,
        since_schema,
        until_schema,
    }
}

pub(super) fn validate(conn: &Connection, schema_version: i64) -> Result<()> {
    if profile_id(schema_version).is_none() {
        bail!("task-history migration source profile is unsupported");
    }
    let expected = expected_families(schema_version);
    let mut statement = conn.prepare(
        "select name from sqlite_schema where type='table' and name not like 'sqlite_%' order by name",
    )?;
    let mut actual = statement
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<BTreeSet<_>>>()?;
    for target in TARGET_FAMILIES {
        actual.remove(*target);
    }
    if schema_version == 13 {
        for inert in INERT_SCHEMA12_AUDIT_FAMILIES {
            actual.remove(*inert);
        }
    }
    if actual != expected {
        bail!(
            "task-history migration source profile is unsupported (expected {} persisted families, found {})",
            expected.len(),
            actual.len()
        );
    }
    Ok(())
}

pub(super) fn profile_id(schema_version: i64) -> Option<&'static str> {
    match schema_version {
        6 => Some("task-history-source-v6"),
        7 => Some("task-history-source-v7"),
        8 => Some("task-history-source-v8"),
        9 => Some("task-history-source-v9"),
        10 => Some("task-history-source-v10"),
        11 => Some("task-history-source-v11"),
        12 => Some("task-history-source-v12"),
        13 => Some("task-history-source-v13"),
        _ => None,
    }
}

fn expected_families(schema_version: i64) -> BTreeSet<String> {
    FAMILIES
        .iter()
        .filter(|family| {
            family.since_schema <= schema_version && schema_version <= family.until_schema
        })
        .map(|family| family.name.to_string())
        .collect()
}

pub(super) fn source_families(schema_version: i64) -> Vec<&'static str> {
    FAMILIES
        .iter()
        .filter(|family| {
            family.since_schema <= schema_version && schema_version <= family.until_schema
        })
        .map(|family| family.name)
        .collect()
}
