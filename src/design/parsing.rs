use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result, bail};
use sha2::{Digest, Sha256};

use super::{validation::*, *};

pub(super) fn extract_design_requirements(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_key: &str,
    content: &str,
    file: &ImportedDesignFile,
) -> Result<Vec<ExtractedDesignRequirement>> {
    let blocks = extract_agent_workbench_blocks(content, file, "requirement")?;
    let mut requirements = Vec::with_capacity(blocks.len());
    let mut seen_keys = BTreeSet::new();
    for block in blocks {
        let metadata: RequirementMetadata = yaml_serde::from_str(&block.metadata_text)
            .with_context(|| {
                format!(
                    "failed to parse requirement metadata for {} in {}",
                    block.source_section, file.relative_path
                )
            })?;
        validate_requirement_metadata(&metadata, &mut seen_keys)?;

        let body = block.body.trim().to_string();
        if body.is_empty() {
            bail!(
                "requirement {} in {} must have body text",
                metadata.key,
                file.relative_path
            );
        }
        let body_line_count = body.lines().count();
        let warning_count = usize::from(body_line_count > 80);
        if body_line_count > 150
            && !design_requirement_key_exception_exists(
                conn,
                project_id,
                design_key,
                &metadata.key,
            )?
        {
            bail!(
                "requirement {} exceeds 150 lines in {}",
                metadata.key,
                file.relative_path
            );
        }

        let mut hasher = Sha256::new();
        hasher.update(block.metadata_text.as_bytes());
        hasher.update(b"\0");
        hasher.update(body.as_bytes());
        let digest = hasher.finalize();
        let revision = metadata.revision.unwrap_or(1);
        if revision <= 0 {
            bail!("requirement revision must be positive");
        }
        requirements.push(ExtractedDesignRequirement {
            source_section: block.source_section,
            requirement_key: metadata.key,
            revision,
            requirement_hash: hex_digest(&digest),
            requirement_text: body,
            priority: metadata.priority,
            required_surfaces: join_metadata_list(&metadata.surfaces),
            validation_expectation: join_metadata_list(&metadata.validation),
            supersedes_requirement_keys: metadata.supersedes,
            status: metadata.status,
            warning_count,
        });
    }
    Ok(requirements)
}

pub(super) fn validate_requirement_metadata(
    metadata: &RequirementMetadata,
    seen_keys: &mut BTreeSet<String>,
) -> Result<()> {
    if metadata.record_type != "requirement" {
        bail!("requirement metadata type must be requirement");
    }
    validate_opaque_key(&metadata.key, "requirement")?;
    if !seen_keys.insert(metadata.key.clone()) {
        bail!("duplicate requirement key: {}", metadata.key);
    }
    match metadata.priority.as_str() {
        "critical" | "high" | "medium" | "low" => {}
        _ => bail!("invalid requirement priority: {}", metadata.priority),
    }
    match metadata.status.as_str() {
        "active" | "superseded" | "accepted_out_of_scope" => {}
        _ => bail!("invalid requirement status: {}", metadata.status),
    }
    for validation_key in &metadata.validation {
        validate_opaque_key(validation_key, "requirement validation")?;
    }
    for superseded_key in &metadata.supersedes {
        validate_opaque_key(superseded_key, "superseded requirement")?;
    }
    Ok(())
}

pub(super) fn extract_design_decisions(
    content: &str,
    file: &ImportedDesignFile,
) -> Result<Vec<ExtractedDesignDecision>> {
    let blocks = extract_agent_workbench_blocks(content, file, "decision")?;
    let mut decisions = Vec::with_capacity(blocks.len());
    let mut seen_keys = BTreeSet::new();
    for block in blocks {
        let metadata: DecisionMetadata =
            yaml_serde::from_str(&block.metadata_text).with_context(|| {
                format!(
                    "failed to parse decision metadata for {} in {}",
                    block.source_section, file.relative_path
                )
            })?;
        validate_decision_metadata(&metadata, &mut seen_keys)?;
        let body = block.body.trim().to_string();
        if body.is_empty() {
            bail!(
                "decision {} in {} must have body text",
                metadata.key,
                file.relative_path
            );
        }
        if body.lines().count() > 150 {
            bail!(
                "decision {} exceeds 150 lines in {}",
                metadata.key,
                file.relative_path
            );
        }
        let mut hasher = Sha256::new();
        hasher.update(block.metadata_text.as_bytes());
        hasher.update(b"\0");
        hasher.update(body.as_bytes());
        let digest = hasher.finalize();
        decisions.push(ExtractedDesignDecision {
            topic: title_from_section(&block.source_section),
            source_section: block.source_section,
            decision_key: metadata.key,
            decision_hash: hex_digest(&digest),
            decision_text: body,
            rationale: None,
            supersedes_decision_keys: join_metadata_list(&metadata.supersedes),
            status: metadata.status,
        });
    }
    Ok(decisions)
}

pub(super) fn validate_decision_metadata(
    metadata: &DecisionMetadata,
    seen_keys: &mut BTreeSet<String>,
) -> Result<()> {
    if metadata.record_type != "decision" {
        bail!("decision metadata type must be decision");
    }
    validate_opaque_key(&metadata.key, "decision")?;
    if !seen_keys.insert(metadata.key.clone()) {
        bail!("duplicate decision key: {}", metadata.key);
    }
    for superseded_key in &metadata.supersedes {
        validate_opaque_key(superseded_key, "superseded decision")?;
    }
    match metadata.status.as_str() {
        "accepted" | "rejected" | "superseded" => {}
        _ => bail!("invalid decision status: {}", metadata.status),
    }
    Ok(())
}

pub(super) fn extract_validation_gate_templates(
    content: &str,
    file: &ImportedDesignFile,
) -> Result<Vec<ExtractedValidationGateTemplate>> {
    let blocks = extract_agent_workbench_blocks(content, file, "validation_gate_template")?;
    let mut templates = Vec::with_capacity(blocks.len());
    let mut seen_keys = BTreeSet::new();
    for block in blocks {
        let metadata: ValidationGateTemplateMetadata = yaml_serde::from_str(&block.metadata_text)
            .with_context(|| {
            format!(
                "failed to parse validation gate metadata for {} in {}",
                block.source_section, file.relative_path
            )
        })?;
        validate_validation_gate_template_metadata(&metadata, &mut seen_keys)?;
        let body = block.body.trim().to_string();
        if body.is_empty() {
            bail!(
                "validation gate {} in {} must have body text",
                metadata.key,
                file.relative_path
            );
        }
        if body.lines().count() > 150 {
            bail!(
                "validation gate {} exceeds 150 lines in {}",
                metadata.key,
                file.relative_path
            );
        }
        let mut hasher = Sha256::new();
        hasher.update(block.metadata_text.as_bytes());
        hasher.update(b"\0");
        hasher.update(body.as_bytes());
        let digest = hasher.finalize();
        templates.push(ExtractedValidationGateTemplate {
            source_section: block.source_section,
            gate_key: metadata.key,
            gate_hash: hex_digest(&digest),
            stage: normalize_validation_phase(&metadata.phase)?,
            command: metadata.command_template,
            expected_result: metadata.expected_result,
            requirement_keys: join_metadata_list(&metadata.applies_to),
            gate_text: body,
            status: metadata.status,
        });
    }
    Ok(templates)
}

pub(super) fn validate_validation_gate_template_metadata(
    metadata: &ValidationGateTemplateMetadata,
    seen_keys: &mut BTreeSet<String>,
) -> Result<()> {
    if metadata.record_type != "validation_gate_template" {
        bail!("validation gate metadata type must be validation_gate_template");
    }
    validate_opaque_key(&metadata.key, "validation gate")?;
    if !seen_keys.insert(metadata.key.clone()) {
        bail!("duplicate validation gate key: {}", metadata.key);
    }
    normalize_validation_phase(&metadata.phase)?;
    validate_expected_result(&metadata.expected_result)?;
    match metadata.status.as_str() {
        "active" | "superseded" | "accepted_out_of_scope" => {}
        _ => bail!("invalid validation gate status: {}", metadata.status),
    }
    for requirement_key in &metadata.applies_to {
        validate_opaque_key(requirement_key, "validation gate applies_to")?;
    }
    if metadata.applies_to.is_empty() && metadata.status == "active" {
        bail!("active validation gate must declare applies_to metadata");
    }
    Ok(())
}

pub(super) fn validate_agent_blocks_for_file(file: &ImportedDesignFile) -> Result<()> {
    let lines: Vec<&str> = file.content.lines().collect();
    reject_agent_workbench_blocks_without_level_two_heading(&lines, file)?;
    for block in agent_workbench_header_blocks(&lines, file)? {
        let metadata: AgentBlockHeaderMetadata = yaml_serde::from_str(&block.metadata_text)
            .with_context(|| {
                format!(
                    "failed to parse agent-workbench metadata for {} in {}",
                    block.source_section, file.relative_path
                )
            })?;
        let allowed_section = match metadata.record_type.as_str() {
            "requirement" => "requirements",
            "decision" => "arc42.decisions",
            "validation_gate_template" => "validation",
            _ => bail!(
                "unknown agent-workbench metadata type {} in {}",
                metadata.record_type,
                file.relative_path
            ),
        };
        validate_opaque_key(&metadata.key, "agent-workbench metadata")?;
        if file.section_key != allowed_section {
            bail!(
                "agent-workbench metadata type {} is not allowed in {}",
                metadata.record_type,
                file.relative_path
            );
        }
    }
    Ok(())
}

pub(super) fn agent_workbench_header_blocks(
    lines: &[&str],
    file: &ImportedDesignFile,
) -> Result<Vec<ExtractedBlock>> {
    let mut blocks = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index];
        if !line.starts_with("## ") {
            index += 1;
            continue;
        }
        let source_section = line.trim_start_matches("## ").trim().to_string();
        let fence_start = index + 1;
        if lines.get(fence_start).map(|line| line.trim()) != Some("```yaml agent-workbench") {
            index += 1;
            continue;
        }
        let mut fence_end = fence_start + 1;
        while fence_end < lines.len() && lines[fence_end].trim() != "```" {
            fence_end += 1;
        }
        if fence_end == lines.len() {
            bail!(
                "agent-workbench block {} in {} has an unterminated yaml block",
                source_section,
                file.relative_path
            );
        }
        blocks.push(ExtractedBlock {
            source_section,
            metadata_text: lines[fence_start + 1..fence_end].join("\n"),
            body: String::new(),
        });
        index = fence_end + 1;
    }
    Ok(blocks)
}

pub(super) fn reject_agent_workbench_blocks_without_level_two_heading(
    lines: &[&str],
    file: &ImportedDesignFile,
) -> Result<()> {
    for (index, line) in lines.iter().enumerate() {
        if line.trim() != "```yaml agent-workbench" {
            continue;
        }
        let Some(previous_line) = index
            .checked_sub(1)
            .and_then(|previous| lines.get(previous))
        else {
            bail!(
                "yaml agent-workbench block in {} must follow a level-two heading",
                file.relative_path
            );
        };
        if !previous_line.starts_with("## ") {
            bail!(
                "yaml agent-workbench block in {} must follow a level-two heading",
                file.relative_path
            );
        }
    }
    Ok(())
}

pub(super) fn extract_agent_workbench_blocks(
    content: &str,
    file: &ImportedDesignFile,
    expected_type: &str,
) -> Result<Vec<ExtractedBlock>> {
    let lines: Vec<&str> = content.lines().collect();
    let mut blocks = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index];
        if !line.starts_with("## ") {
            index += 1;
            continue;
        }
        let source_section = line.trim_start_matches("## ").trim().to_string();
        let fence_start = index + 1;
        if lines.get(fence_start).map(|line| line.trim()) != Some("```yaml agent-workbench") {
            index += 1;
            continue;
        }
        let mut fence_end = fence_start + 1;
        while fence_end < lines.len() && lines[fence_end].trim() != "```" {
            fence_end += 1;
        }
        if fence_end == lines.len() {
            bail!(
                "agent-workbench block {} in {} has an unterminated yaml block",
                source_section,
                file.relative_path
            );
        }
        let metadata_text = lines[fence_start + 1..fence_end].join("\n");
        let metadata: AgentBlockHeaderMetadata = yaml_serde::from_str(&metadata_text)
            .with_context(|| {
                format!(
                    "failed to parse agent-workbench metadata for {} in {}",
                    source_section, file.relative_path
                )
            })?;
        if metadata.record_type != expected_type {
            bail!(
                "unexpected agent-workbench metadata type {} in {}; expected {}",
                metadata.record_type,
                file.relative_path,
                expected_type
            );
        }
        let mut body_end = fence_end + 1;
        while body_end < lines.len() && !lines[body_end].starts_with("## ") {
            body_end += 1;
        }
        blocks.push(ExtractedBlock {
            source_section,
            metadata_text,
            body: lines[fence_end + 1..body_end].join("\n"),
        });
        index = body_end;
    }
    Ok(blocks)
}

pub(super) fn join_metadata_list(values: &[String]) -> Option<String> {
    if values.is_empty() {
        None
    } else {
        Some(values.join(","))
    }
}

pub(super) fn design_manifest(design_id: &str, title: &str) -> String {
    let design_id = yaml_string(design_id);
    let title = yaml_string(title);
    format!(
        r#"id: {design_id}
title: {title}
format: arc42-agent-workbench
version: 1
status: draft

arc42:
  introduction_goals: 01-introduction-goals.md
  constraints: 02-constraints.md
  context_scope: 03-context-scope.md
  solution_strategy: 04-solution-strategy.md
  building_blocks: 05-building-blocks.md
  runtime_view: 06-runtime-view.md
  deployment_view: 07-deployment-view.md
  crosscutting_concepts: 08-crosscutting-concepts.md
  decisions: 09-decisions.md
  quality_requirements: 10-quality-requirements.md
  risks_technical_debt: 11-risks-technical-debt.md
  glossary: 12-glossary.md

requirements:
  - requirements/README.md

validation:
  - validation/gates.md

depends_on: []
"#
    )
}

pub(super) fn yaml_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    format!("\"{escaped}\"")
}

pub(super) fn line_count(content: &str) -> i64 {
    content.lines().count().try_into().unwrap_or(i64::MAX)
}

pub(super) fn display_path(path: &Path) -> String {
    path.display().to_string()
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = hasher.finalize();
    hex_digest(&digest)
}

pub(super) fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

pub(super) fn markdown_stub(title: &str) -> String {
    format!("# {title}\n\n")
}
