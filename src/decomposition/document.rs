use super::*;

mod validation;
use validation::sorted_directories;
pub(in crate::decomposition) use validation::{
    fenced_metadata, require_digest, require_key, sorted_entries, validate_plan,
    validate_plan_header,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParsedPlan {
    pub(crate) source_path: PathBuf,
    pub(crate) design_root: PathBuf,
    pub(crate) source_identity: String,
    pub(crate) content_identity: String,
    pub(crate) content: String,
    pub(crate) document: Option<PlanDocument>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanDocument {
    #[serde(rename = "type")]
    pub(crate) record_type: String,
    pub(crate) format: i64,
    pub(crate) key: String,
    pub(crate) design_fingerprint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) work: Option<i64>,
    pub(crate) items: Vec<PlanItem>,
    pub(crate) slices: Vec<PlanSlice>,
    pub(crate) reconciliation: Option<PlanReconciliation>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanReconciliation {
    pub(crate) predecessor: i64,
    pub(crate) expected_current: String,
    pub(crate) tasks: Vec<TaskReconciliation>,
    pub(crate) checklist: Vec<ChecklistReconciliation>,
    pub(crate) gates: Vec<GateReconciliation>,
    pub(crate) phases: Vec<PhaseReconciliation>,
    pub(crate) dependencies: Vec<DependencyReconciliation>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskReconciliation {
    pub(crate) source: i64,
    pub(crate) disposition: String,
    pub(crate) item: Option<String>,
    pub(crate) reason: Option<String>,
    pub(crate) effect: Option<ReconciliationEffect>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ChecklistReconciliation {
    pub(crate) source: i64,
    pub(crate) disposition: String,
    pub(crate) item: Option<String>,
    pub(crate) boundary: Option<String>,
    pub(crate) reason: Option<String>,
    pub(crate) effect: Option<ReconciliationEffect>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct GateReconciliation {
    pub(crate) source: i64,
    pub(crate) disposition: String,
    pub(crate) item: Option<String>,
    pub(crate) gate: Option<String>,
    pub(crate) boundary: Option<String>,
    pub(crate) reason: Option<String>,
    pub(crate) effect: Option<ReconciliationEffect>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct PhaseReconciliation {
    pub(crate) source: i64,
    pub(crate) disposition: String,
    pub(crate) slice: Option<String>,
    pub(crate) reason: Option<String>,
    pub(crate) effect: Option<ReconciliationEffect>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct DependencyReconciliation {
    pub(crate) source: i64,
    pub(crate) disposition: String,
    pub(crate) from: Option<String>,
    pub(crate) to: Option<String>,
    pub(crate) reason: Option<String>,
    pub(crate) effect: Option<ReconciliationEffect>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ReconciliationEffect {
    Preserve,
    Open,
}

impl ReconciliationEffect {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Preserve => "preserve",
            Self::Open => "open",
        }
    }
}

pub(super) fn normalized_effect(effect: Option<ReconciliationEffect>) -> ReconciliationEffect {
    effect.unwrap_or(ReconciliationEffect::Preserve)
}

pub(super) fn stored_effect(
    disposition: &str,
    effect: Option<ReconciliationEffect>,
) -> Option<&'static str> {
    (disposition == "retained").then(|| normalized_effect(effect).as_str())
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanItem {
    pub(crate) key: String,
    pub(crate) requirements: Vec<String>,
    pub(crate) title: String,
    pub(crate) details: String,
    pub(crate) completion: PlanCompletion,
    pub(crate) checklist: Vec<PlanChecklistBoundary>,
    pub(crate) slice: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanCompletion {
    pub(crate) outcome: String,
    pub(crate) observation: String,
    pub(crate) evidence_owner: String,
    pub(crate) evidence_kind: String,
    #[serde(default)]
    pub(crate) gates: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanChecklistBoundary {
    pub(crate) key: String,
    pub(crate) condition: String,
    pub(crate) evidence_kind: String,
    #[serde(default)]
    pub(crate) gates: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanSlice {
    pub(crate) key: String,
    pub(crate) title: String,
    pub(crate) order: i64,
    #[serde(default)]
    pub(crate) depends_on: Vec<String>,
}

pub(crate) fn discover_plans(root: &Path) -> Result<Vec<ParsedPlan>> {
    let designs = root.join(crate::db::LEDGER_DIR).join(crate::db::DESIGN_DIR);
    if !designs.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for design in sorted_directories(&designs)? {
        let plan_dir = design.join("plans");
        if !plan_dir.is_dir() {
            continue;
        }
        for entry in sorted_entries(&plan_dir)? {
            if entry.extension().and_then(|value| value.to_str()) == Some("md") {
                paths.push(entry);
            }
        }
    }
    paths
        .into_iter()
        .map(|path| parse_plan(root, &path))
        .collect()
}

pub(crate) fn parse_plan(root: &Path, path: &Path) -> Result<ParsedPlan> {
    let parsed = parse_plan_unvalidated(root, path)?;
    if let Some(document) = parsed.document.as_ref() {
        validate_plan(document)?;
    }
    Ok(parsed)
}

pub(super) fn parse_plan_unvalidated(root: &Path, path: &Path) -> Result<ParsedPlan> {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let selected_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let canonical_path = selected_path
        .canonicalize()
        .with_context(|| format!("decomposition plan is unavailable: {}", path.display()))?;
    if !canonical_path.starts_with(&canonical_root) {
        bail!("decomposition plan must be owned by the selected project");
    }
    let content = fs::read_to_string(&canonical_path)
        .with_context(|| format!("decomposition plan is unreadable: {}", path.display()))?;
    let source_path = canonical_path
        .strip_prefix(&canonical_root)
        .context("decomposition plan source escaped the project root")?
        .to_path_buf();
    let design_root = canonical_path
        .parent()
        .and_then(Path::parent)
        .context("decomposition plan is not below a design plans directory")?
        .to_path_buf();
    parse_owned_plan_content(content, source_path, design_root)
}

pub(super) fn parse_owned_plan_content(
    source_content: String,
    source_path: PathBuf,
    design_root: PathBuf,
) -> Result<ParsedPlan> {
    let document = fenced_metadata(&source_content)?
        .map(|metadata| {
            let document: PlanDocument =
                yaml_serde::from_str(metadata).context("decomposition plan metadata is invalid")?;
            Ok::<_, anyhow::Error>(document)
        })
        .transpose()?;
    let content = document
        .as_ref()
        .map(canonical_plan_content)
        .transpose()?
        .unwrap_or_else(|| source_content.clone());
    let content_identity = plan_content_identity(&content);
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/decomposition-plan-source/v1\0");
    hasher.update(source_content.as_bytes());
    Ok(ParsedPlan {
        source_path,
        design_root,
        source_identity: format!("{:x}", hasher.finalize()),
        content_identity,
        content,
        document,
    })
}

pub(crate) fn canonical_plan_content(document: &PlanDocument) -> Result<String> {
    let metadata = serde_json::to_string_pretty(document)
        .context("failed to serialize canonical Decomposition Plan content")?;
    Ok(format!(
        "# Decomposition Plan\n\n```yaml agent-workbench\n{metadata}\n```\n"
    ))
}

pub(crate) fn plan_content_identity(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/decomposition-plan-content/v1\0");
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(super) fn stored_source_path(parsed: &ParsedPlan) -> Option<String> {
    (!parsed.source_path.as_os_str().is_empty())
        .then(|| parsed.source_path.to_string_lossy().into_owned())
}
