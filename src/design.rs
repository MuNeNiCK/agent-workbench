mod acceptance;
mod package;
mod parsing;
mod readiness;
mod validation;

use std::path::{Path, PathBuf};

use serde::Deserialize;

pub use acceptance::*;
pub use package::*;
pub use readiness::*;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DesignManifest {
    id: String,
    title: String,
    format: String,
    version: i64,
    status: String,
    arc42: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    requirements: Vec<String>,
    #[serde(default)]
    validation: Vec<String>,
    #[serde(default)]
    depends_on: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RequirementMetadata {
    #[serde(rename = "type")]
    record_type: String,
    key: String,
    #[serde(default)]
    revision: Option<i64>,
    priority: String,
    #[serde(default)]
    surfaces: Vec<String>,
    #[serde(default)]
    validation: Vec<String>,
    #[serde(default)]
    supersedes: Vec<String>,
    status: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DecisionMetadata {
    #[serde(rename = "type")]
    record_type: String,
    key: String,
    status: String,
    #[serde(default)]
    supersedes: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ValidationGateTemplateMetadata {
    #[serde(rename = "type")]
    record_type: String,
    key: String,
    phase: String,
    expected_result: String,
    #[serde(default)]
    applies_to: Vec<String>,
    #[serde(default)]
    command_template: Option<String>,
    status: String,
}

#[derive(Debug, Deserialize)]
struct AgentBlockHeaderMetadata {
    #[serde(rename = "type")]
    record_type: String,
    key: String,
}

impl DesignManifest {
    fn design_files(&self) -> Vec<ManifestDesignFile> {
        let mut files = Vec::new();
        files.extend(
            self.arc42
                .iter()
                .map(|(section_key, relative_path)| ManifestDesignFile {
                    section_key: format!("arc42.{section_key}"),
                    relative_path: relative_path.clone(),
                }),
        );
        files.extend(
            self.requirements
                .iter()
                .map(|relative_path| ManifestDesignFile {
                    section_key: "requirements".to_string(),
                    relative_path: relative_path.clone(),
                }),
        );
        files.extend(
            self.validation
                .iter()
                .map(|relative_path| ManifestDesignFile {
                    section_key: "validation".to_string(),
                    relative_path: relative_path.clone(),
                }),
        );
        files
    }
}

struct ManifestDesignFile {
    section_key: String,
    relative_path: String,
}

struct ImportedDesignFile {
    section_key: String,
    relative_path: String,
    content_hash: String,
    line_count: i64,
    content: String,
}

struct ExtractedDesignRequirement {
    source_section: String,
    requirement_key: String,
    revision: i64,
    requirement_hash: String,
    requirement_text: String,
    priority: String,
    required_surfaces: Option<String>,
    validation_expectation: Option<String>,
    supersedes_requirement_keys: Vec<String>,
    status: String,
    warning_count: usize,
}

struct ExtractedDesignDecision {
    source_section: String,
    decision_key: String,
    decision_hash: String,
    topic: String,
    decision_text: String,
    rationale: Option<String>,
    supersedes_decision_keys: Option<String>,
    status: String,
}

struct ExtractedValidationGateTemplate {
    source_section: String,
    gate_key: String,
    gate_hash: String,
    stage: String,
    command: Option<String>,
    expected_result: String,
    requirement_keys: Option<String>,
    gate_text: String,
    status: String,
}

struct ExtractedBlock {
    source_section: String,
    metadata_text: String,
    body: String,
}

struct StoredDesignVersion {
    design_version_id: i64,
    design_package_id: i64,
    status: String,
    design_key: String,
    current_design_version_id: Option<i64>,
}

struct ResolvedDesignAcceptanceTarget {
    target_type: &'static str,
    task_id: Option<i64>,
    design_requirement_id: Option<i64>,
    validation_gate_template_id: Option<i64>,
    coverage_item_id: Option<i64>,
    design_package_key: Option<String>,
    design_file_path: Option<String>,
    design_requirement_key: Option<String>,
}

struct ResolvedGeneralAcceptanceTarget {
    target_type: &'static str,
    task_id: Option<i64>,
    finding_id: Option<i64>,
    validation_gate_id: Option<i64>,
    validation_run_id: Option<i64>,
    repository_state_classification_id: Option<i64>,
    repository_snapshot_comparison_id: Option<i64>,
    review_plan_id: Option<i64>,
    checklist_item_id: Option<i64>,
    command_profile_id: Option<i64>,
    command_usage_id: Option<i64>,
    command_deviation_id: Option<i64>,
    rule_binding_id: Option<i64>,
    stale_record_type: Option<String>,
    stale_record_id: Option<i64>,
}

impl ResolvedGeneralAcceptanceTarget {
    fn new(target_type: &'static str) -> Self {
        Self {
            target_type,
            task_id: None,
            finding_id: None,
            validation_gate_id: None,
            validation_run_id: None,
            repository_state_classification_id: None,
            repository_snapshot_comparison_id: None,
            review_plan_id: None,
            checklist_item_id: None,
            command_profile_id: None,
            command_usage_id: None,
            command_deviation_id: None,
            rule_binding_id: None,
            stale_record_type: None,
            stale_record_id: None,
        }
    }
}

pub struct NewDesignPackage<'a> {
    pub design_id: &'a str,
    pub title: &'a str,
}

pub struct DesignPackageImport<'a> {
    pub package_path: &'a Path,
    pub status: &'a str,
}

pub struct DesignVersionApproval<'a> {
    pub design_version_id: i64,
    pub summary: Option<&'a str>,
}

pub struct DesignReadyCheck {
    pub design_version_id: Option<i64>,
}

pub struct DesignRequirementListQuery {
    pub design_version_id: i64,
}

pub struct DesignDecisionListQuery {
    pub design_version_id: i64,
}

pub struct ValidationGateTemplateListQuery {
    pub design_version_id: i64,
}

pub struct NewDesignExceptionAcceptance<'a> {
    pub design_version_id: Option<i64>,
    pub design_package: Option<&'a str>,
    pub target: &'a str,
    pub acceptance_type: &'a str,
    pub reason: &'a str,
    pub approval_authority_event_id: i64,
}

pub struct NewGeneralAcceptance<'a> {
    pub target: &'a str,
    pub acceptance_type: &'a str,
    pub reason: &'a str,
    pub approval_authority_event_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DesignPackageInitOutcome {
    pub package_path: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DesignPackageImportOutcome {
    pub design_package_id: i64,
    pub design_version_id: i64,
    pub version_number: i64,
    pub content_hash: String,
    pub file_count: usize,
    pub requirement_count: usize,
    pub decision_count: usize,
    pub validation_gate_template_count: usize,
    pub warning_count: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DesignVersionApprovalOutcome {
    pub design_package_id: i64,
    pub design_version_id: i64,
    pub authority_event_id: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DesignReadyOutcome {
    pub result: String,
    pub blocking_reason: Option<String>,
    pub design_package_id: Option<i64>,
    pub design_version_id: Option<i64>,
    pub items: Vec<DesignReadyItem>,
}

impl DesignReadyOutcome {
    fn blocked(
        requested_design_version_id: Option<i64>,
        design_package_id: Option<i64>,
        reason: &str,
        items: Vec<DesignReadyItem>,
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
pub struct DesignReadyItem {
    pub name: String,
    pub result: String,
    pub detail: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DesignRequirementRecord {
    pub id: i64,
    pub design_version_id: i64,
    pub source_design_file_id: i64,
    pub source_path: String,
    pub source_section: String,
    pub requirement_key: String,
    pub revision: i64,
    pub requirement_text: String,
    pub priority: String,
    pub required_surfaces: Option<String>,
    pub validation_expectation: Option<String>,
    pub status: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DesignDecisionRecord {
    pub id: i64,
    pub design_version_id: i64,
    pub source_design_file_id: i64,
    pub source_path: String,
    pub source_section: String,
    pub decision_key: String,
    pub topic: String,
    pub decision_text: String,
    pub rationale: Option<String>,
    pub supersedes_decision_keys: Option<String>,
    pub status: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ValidationGateTemplateRecord {
    pub id: i64,
    pub design_version_id: i64,
    pub source_design_file_id: i64,
    pub source_path: String,
    pub source_section: String,
    pub gate_key: String,
    pub stage: String,
    pub command: Option<String>,
    pub expected_result: String,
    pub requirement_keys: Option<String>,
    pub gate_text: String,
    pub status: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DesignExceptionAcceptanceOutcome {
    pub acceptance_record_id: i64,
    pub authority_event_id: i64,
    pub target_type: String,
    pub design_requirement_id: Option<i64>,
    pub validation_gate_template_id: Option<i64>,
    pub coverage_item_id: Option<i64>,
    pub design_package_key: Option<String>,
    pub design_file_path: Option<String>,
    pub design_requirement_key: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct GeneralAcceptanceOutcome {
    pub acceptance_record_id: i64,
    pub authority_event_id: i64,
    pub target_type: String,
}

impl DesignReadyItem {
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
