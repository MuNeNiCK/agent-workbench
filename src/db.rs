mod adjudication_migration;
mod closure_migration;
mod completion_migration;
mod integrity_migration;
mod legacy_migration;
mod migration;
mod mutation;
mod owner_routing;
mod project;
mod project_integrity;
mod runtime;
mod schema;
mod status;

#[cfg(test)]
pub(crate) use adjudication_migration::validate_schema11_invalid_combinations;

use std::path::PathBuf;

pub const LEDGER_DIR: &str = ".agent-workbench";
pub const LEDGER_FILE: &str = "ledger.sqlite";
pub const DESIGN_DIR: &str = "designs";
pub const EXPORT_DIR: &str = "exports";
pub const LOG_DIR: &str = "logs";
pub(crate) const SCHEMA_VERSION: i64 = 12;

pub(crate) use migration::*;
pub(crate) use mutation::*;
pub(crate) use runtime::*;
pub use status::*;

#[derive(Debug)]
pub(crate) struct StoredActivation {
    pub(crate) activation_id: i64,
    pub(crate) project_id: i64,
    pub(crate) work_unit_id: i64,
    pub(crate) stack_depth: i64,
    pub(crate) status: String,
}

#[derive(Debug)]
pub(crate) struct StoredSuspendSnapshot {
    pub(crate) id: i64,
    pub(crate) reason: String,
    pub(crate) active_task_ids: Option<String>,
    pub(crate) next_action: String,
    pub(crate) selected_gate_id: Option<i64>,
    pub(crate) authority_refs: Option<String>,
    pub(crate) review_scope_refs: Option<String>,
    pub(crate) repository_heads: Option<String>,
    pub(crate) repository_snapshot_ids: Option<String>,
    pub(crate) repository_status: Option<String>,
    pub(crate) dirty_state_summary: Option<String>,
    pub(crate) open_findings: Option<String>,
    pub(crate) assumptions: Option<String>,
}

pub(crate) struct NewEvent<'a> {
    pub(crate) work_unit_id: i64,
    pub(crate) activation_id: Option<i64>,
    pub(crate) related_activation_id: Option<i64>,
    pub(crate) event_type: &'a str,
    pub(crate) reason: Option<&'a str>,
    pub(crate) status_domain: &'a str,
    pub(crate) previous_status: Option<&'a str>,
    pub(crate) next_status: Option<&'a str>,
}

#[derive(Debug)]
pub struct InitOutcome {
    pub ledger_path: PathBuf,
}

#[derive(Debug)]
pub struct ProjectStatus {
    pub initialized: bool,
    pub ledger_path: PathBuf,
    pub project_name: Option<String>,
    pub open_work_units: i64,
    pub active_activations: i64,
    pub schema_version: Option<i64>,
    pub project_integrity: ProjectIntegrityStatus,
    pub phase_blocker: Option<PhaseBlocker>,
    pub owner_actions: Vec<OwnerAction>,
    pub finding_remediations: Vec<FindingRemediation>,
    pub source_corrections: Vec<SourceCorrection>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum NextAction {
    NotInitialized {
        ledger_path: PathBuf,
    },
    BlockedPhase {
        blocker: PhaseBlocker,
    },
    ProjectIntegrityBlocked {
        integrity: ProjectIntegrityStatus,
    },
    OwnerActions {
        owners: Vec<OwnerAction>,
    },
    FindingRemediation {
        remediations: Vec<FindingRemediation>,
    },
    SourceCorrection {
        corrections: Vec<SourceCorrection>,
    },
    NoOpenWorkUnit,
    ResumeSuspended {
        work_unit: ActiveWorkUnit,
    },
    ActivateOpen {
        work_unit: ActiveWorkUnit,
    },
    ContinueActive {
        work_unit: ActiveWorkUnit,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrityPredicateStatus {
    pub code: String,
    pub name: String,
    pub result: String,
    pub evidence: String,
    pub next_action: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectIntegrityStatus {
    pub result: String,
    pub predicates: Vec<IntegrityPredicateStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerAction {
    pub owner_type: String,
    pub owner_id: i64,
    pub title: String,
    pub state: String,
    pub schedulable: bool,
    pub blocker_kind: Option<String>,
    pub description: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindingRemediation {
    pub review_plan_id: i64,
    pub work_unit_id: i64,
    pub finding_id: i64,
    pub closure_id: i64,
    pub description: String,
    pub affected_surfaces: String,
    pub fix_plan: String,
    pub design_invariant: String,
    pub tests_or_gates: String,
    pub verification_plan: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCorrection {
    pub review_plan_id: i64,
    pub work_unit_id: i64,
    pub finding_id: i64,
    pub closure_id: i64,
    pub correction_session_id: i64,
    pub description: String,
    pub affected_surfaces: String,
    pub fix_plan: String,
    pub design_invariant: String,
    pub tests_or_gates: String,
    pub verification_plan: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseBlocker {
    pub kind: String,
    pub review_plan_id: Option<i64>,
    pub work_unit_id: Option<i64>,
    pub review_type: Option<String>,
    pub stage: Option<String>,
    pub review_run_id: Option<i64>,
    pub finding_id: Option<i64>,
    pub severity: Option<String>,
    pub classification: Option<String>,
    pub description: String,
    pub next_action: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ActiveWorkUnit {
    pub id: i64,
    pub title: String,
    pub design_version_id: Option<i64>,
    pub next_phase_id: Option<i64>,
    pub next_phase_key: Option<String>,
    pub next_phase_title: Option<String>,
}
