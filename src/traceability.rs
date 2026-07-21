mod checklists;
mod decomposition;
mod evidence;
mod phase_membership;
mod readiness;
mod reconciliation;

pub use checklists::*;
pub use decomposition::*;
pub use evidence::*;
pub use readiness::*;
pub(crate) use reconciliation::*;

fn resolved_design_version(row: &rusqlite::Row<'_>) -> rusqlite::Result<ResolvedDesignVersion> {
    Ok(ResolvedDesignVersion {
        design_version_id: row.get(0)?,
        design_package_id: row.get(1)?,
        status: row.get(2)?,
        approved_by_authority_event_id: row.get(3)?,
        current_design_version_id: row.get(4)?,
    })
}

struct ResolvedRequirement {
    id: i64,
    key: String,
}

struct RequirementForDecomposition {
    id: i64,
    key: String,
    text: String,
    priority: String,
}

struct ResolvedTask {
    id: i64,
    work_unit_id: Option<i64>,
    title: String,
    completion_condition: Option<String>,
}

struct ResolvedGateTemplate {
    id: i64,
    gate_key: String,
    command: Option<String>,
    expected_result: String,
}

struct ResolvedValidationGate {
    work_unit_id: Option<i64>,
    task_id: Option<i64>,
}

struct ResolvedDesignVersion {
    design_version_id: i64,
    design_package_id: i64,
    status: String,
    approved_by_authority_event_id: Option<i64>,
    current_design_version_id: Option<i64>,
}

pub struct NewTaskDerivation<'a> {
    pub design_version_id: i64,
    pub requirement_key: &'a str,
    pub task_id: i64,
    pub derivation_reason: Option<&'a str>,
    pub checklist_title: Option<&'a str>,
    pub item_title: Option<&'a str>,
    pub completion_condition: Option<&'a str>,
}

pub struct DesignDecomposition<'a> {
    pub design_version_id: i64,
    pub work_unit_id: i64,
    pub checklist_title: Option<&'a str>,
    pub reason: Option<&'a str>,
}

pub struct TaskDerivationListQuery {
    pub design_version_id: i64,
    pub work_unit_id: Option<i64>,
}

pub struct TaskDerivationListFilter {
    pub design_version_id: Option<i64>,
    pub task_id: Option<i64>,
    pub work_unit_id: Option<i64>,
}

pub struct ImplementationReadyCheck {
    pub design_version_id: Option<i64>,
}

pub struct ValidationGateSelection<'a> {
    pub design_version_id: i64,
    pub gate_key: &'a str,
    pub requirement_key: &'a str,
    pub task_id: i64,
    pub command: Option<&'a str>,
    pub command_profile: Option<&'a str>,
    pub timeout: Option<&'a str>,
}

pub struct NewValidationRun<'a> {
    pub validation_gate_id: i64,
    pub command_usage_id: Option<i64>,
    pub repository_snapshot_id: Option<i64>,
    pub result: &'a str,
    pub command: Option<&'a str>,
    pub classification: Option<&'a str>,
    pub acceptance_record_id: Option<i64>,
    pub artifact_path: Option<&'a str>,
    pub artifact_hash: Option<&'a str>,
    pub notes: Option<&'a str>,
}

pub struct ValidationRunListQuery {
    pub validation_gate_id: Option<i64>,
}

pub struct NewImplementationEvidence<'a> {
    pub task_id: Option<i64>,
    pub design_version_id: Option<i64>,
    pub requirement_key: Option<&'a str>,
    pub evidence_type: &'a str,
    pub commit_sha: Option<&'a str>,
    pub file_path: Option<&'a str>,
    pub line_ref: Option<&'a str>,
    pub symbol: Option<&'a str>,
    pub artifact_path: Option<&'a str>,
    pub note: Option<&'a str>,
}

pub struct NewImplementationEvidenceWithGit<'a> {
    pub task_id: Option<i64>,
    pub design_version_id: Option<i64>,
    pub requirement_key: Option<&'a str>,
    pub evidence_type: &'a str,
    pub repository_id: Option<i64>,
    pub git_commit_id: Option<i64>,
    pub git_file_change_id: Option<i64>,
    pub commit_sha: Option<&'a str>,
    pub file_path: Option<&'a str>,
    pub line_ref: Option<&'a str>,
    pub symbol: Option<&'a str>,
    pub artifact_path: Option<&'a str>,
    pub note: Option<&'a str>,
}

struct ImplementationEvidenceInput<'a> {
    task_id: Option<i64>,
    design_version_id: Option<i64>,
    requirement_key: Option<&'a str>,
    evidence_type: &'a str,
    repository_id: Option<i64>,
    git_commit_id: Option<i64>,
    git_file_change_id: Option<i64>,
    commit_sha: Option<&'a str>,
    file_path: Option<&'a str>,
    line_ref: Option<&'a str>,
    symbol: Option<&'a str>,
    artifact_path: Option<&'a str>,
    note: Option<&'a str>,
}

struct ResolvedGitEvidence {
    repository_id: Option<i64>,
    git_commit_id: Option<i64>,
    git_file_change_id: Option<i64>,
    commit_sha: Option<String>,
    file_path: Option<String>,
}

struct ResolvedCommit {
    repository_id: i64,
    commit_sha: String,
}

struct ResolvedFileChange {
    repository_id: i64,
    git_commit_id: i64,
    path: String,
    commit_sha: String,
}

pub struct ImplementationEvidenceListQuery {
    pub task_id: Option<i64>,
    pub design_version_id: Option<i64>,
    pub work_unit_id: Option<i64>,
    pub evidence_type: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskDerivationOutcome {
    pub task_derivation_id: i64,
    pub checklist_id: i64,
    pub checklist_item_id: i64,
    pub design_requirement_id: i64,
    pub task_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct TaskDerivationRecord {
    pub id: i64,
    pub requirement_key: String,
    pub task_id: i64,
    pub task_title: String,
    pub checklist_item_id: Option<i64>,
    pub checklist_item_title: Option<String>,
    pub status: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DesignDecompositionOutcome {
    pub design_version_id: i64,
    pub work_unit_id: i64,
    pub checklist_id: i64,
    pub created_tasks: i64,
    pub created_derivations: i64,
    pub created_validation_gates: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ChecklistRecord {
    pub id: i64,
    pub work_unit_id: i64,
    pub design_version_id: i64,
    pub title: String,
    pub status: String,
    pub item_count: i64,
    pub closed_count: i64,
}

pub struct ChecklistListFilter<'a> {
    pub status: Option<&'a str>,
    pub work_unit_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ChecklistItemListQuery<'a> {
    pub checklist_id: Option<i64>,
    pub status: Option<&'a str>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ChecklistItemRecord {
    pub id: i64,
    pub checklist_id: i64,
    pub work_unit_id: i64,
    pub design_version_id: i64,
    pub design_requirement_id: i64,
    pub requirement_key: String,
    pub task_id: i64,
    pub item_order: i64,
    pub title: String,
    pub completion_condition: Option<String>,
    pub status: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ChecklistItemOutcome {
    pub checklist_item_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ChecklistOutcome {
    pub checklist_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct StaleRecord {
    pub record_type: String,
    pub id: i64,
    pub label: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct StaleRecordDisposition<'a> {
    pub record_type: &'a str,
    pub record_id: i64,
    pub reason: &'a str,
}

#[derive(Debug, PartialEq, Eq)]
pub struct StaleRecordDispositionOutcome {
    pub record_type: String,
    pub record_id: i64,
    pub label: String,
    pub status: String,
    pub acceptance_record_id: i64,
    pub authority_event_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ValidationGateContextQuery {
    pub design_version_id: i64,
    pub work_unit_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ValidationGateContextRecord {
    pub id: i64,
    pub gate_key: String,
    pub requirement_key: String,
    pub task_id: Option<i64>,
    pub status: String,
    pub latest_run_id: Option<i64>,
    pub latest_command_usage_id: Option<i64>,
    pub latest_repository_snapshot_id: Option<i64>,
    pub latest_result: Option<String>,
    pub latest_artifact_path: Option<String>,
    pub latest_notes: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ImplementationEvidenceOutcome {
    pub implementation_evidence_id: i64,
    pub task_id: Option<i64>,
    pub design_requirement_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ImplementationEvidenceRecord {
    pub id: i64,
    pub task_id: Option<i64>,
    pub requirement_key: Option<String>,
    pub evidence_type: String,
    pub commit_sha: Option<String>,
    pub file_path: Option<String>,
    pub line_ref: Option<String>,
    pub symbol: Option<String>,
    pub artifact_path: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ValidationGateSelectionOutcome {
    pub validation_gate_id: i64,
    pub validation_gate_template_id: i64,
    pub design_requirement_id: i64,
    pub task_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ValidationRunOutcome {
    pub validation_run_id: i64,
    pub validation_gate_id: i64,
    pub work_unit_id: Option<i64>,
    pub task_id: Option<i64>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ValidationRunRecord {
    pub id: i64,
    pub validation_gate_id: i64,
    pub gate_key: String,
    pub work_unit_id: Option<i64>,
    pub task_id: Option<i64>,
    pub command_usage_id: Option<i64>,
    pub repository_snapshot_id: Option<i64>,
    pub result: String,
    pub command: Option<String>,
    pub classification: Option<String>,
    pub acceptance_record_id: Option<i64>,
    pub artifact_path: Option<String>,
    pub artifact_hash: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub retired: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ImplementationReadyOutcome {
    pub result: String,
    pub blocking_reason: Option<String>,
    pub design_package_id: Option<i64>,
    pub design_version_id: Option<i64>,
    pub items: Vec<ImplementationReadyItem>,
}

impl ImplementationReadyOutcome {
    fn blocked(
        requested_design_version_id: Option<i64>,
        design_package_id: Option<i64>,
        reason: &str,
        items: Vec<ImplementationReadyItem>,
    ) -> Self {
        Self {
            result: "blocked".to_string(),
            blocking_reason: Some(reason.to_string()),
            design_package_id,
            design_version_id: requested_design_version_id,
            items,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ImplementationReadyItem {
    pub name: String,
    pub result: String,
    pub detail: Option<String>,
}

impl ImplementationReadyItem {
    fn pass(name: &str, detail: Option<String>) -> Self {
        Self {
            name: name.to_string(),
            result: "pass".to_string(),
            detail,
        }
    }

    fn fail<S: Into<String>>(name: &str, detail: Option<S>) -> Self {
        Self {
            name: name.to_string(),
            result: "fail".to_string(),
            detail: detail.map(Into::into),
        }
    }
}
