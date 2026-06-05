use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::db::{default_design_root, open_existing_project};

const ARC42_FILES: &[(&str, &str)] = &[
    ("01-introduction-goals.md", "Introduction And Goals"),
    ("02-constraints.md", "Constraints"),
    ("03-context-scope.md", "Context And Scope"),
    ("04-solution-strategy.md", "Solution Strategy"),
    ("05-building-blocks.md", "Building Blocks"),
    ("06-runtime-view.md", "Runtime View"),
    ("07-deployment-view.md", "Deployment View"),
    ("08-crosscutting-concepts.md", "Crosscutting Concepts"),
    ("09-decisions.md", "Decisions"),
    ("10-quality-requirements.md", "Quality Requirements"),
    ("11-risks-technical-debt.md", "Risks And Technical Debt"),
    ("12-glossary.md", "Glossary"),
];

pub fn init_design_package(
    root: &Path,
    input: NewDesignPackage<'_>,
) -> Result<DesignPackageInitOutcome> {
    validate_design_id(input.design_id)?;
    open_existing_project(root)?;

    let design_root = default_design_root(root);
    let package_path = design_root.join(input.design_id);
    if package_path.exists() {
        bail!("design package already exists");
    }

    fs::create_dir_all(package_path.join("requirements"))
        .with_context(|| format!("failed to create {}", package_path.display()))?;
    fs::create_dir_all(package_path.join("validation"))
        .with_context(|| format!("failed to create {}", package_path.display()))?;
    fs::create_dir_all(package_path.join("notes"))
        .with_context(|| format!("failed to create {}", package_path.display()))?;

    fs::write(
        package_path.join("design.yaml"),
        design_manifest(input.design_id, input.title),
    )
    .with_context(|| {
        format!(
            "failed to write {}",
            package_path.join("design.yaml").display()
        )
    })?;
    for (file, title) in ARC42_FILES {
        fs::write(package_path.join(file), markdown_stub(title))
            .with_context(|| format!("failed to write {}", package_path.join(file).display()))?;
    }
    fs::write(
        package_path.join("requirements").join("README.md"),
        markdown_stub("Requirements"),
    )
    .with_context(|| {
        format!(
            "failed to write {}",
            package_path
                .join("requirements")
                .join("README.md")
                .display()
        )
    })?;
    fs::write(
        package_path.join("validation").join("gates.md"),
        markdown_stub("Validation Gates"),
    )
    .with_context(|| {
        format!(
            "failed to write {}",
            package_path.join("validation").join("gates.md").display()
        )
    })?;

    Ok(DesignPackageInitOutcome { package_path })
}

fn validate_design_id(design_id: &str) -> Result<()> {
    if design_id.is_empty() {
        bail!("design id is required");
    }
    let valid = design_id
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if !valid {
        bail!("design id must contain only lowercase letters, digits, and hyphens");
    }
    Ok(())
}

fn design_manifest(design_id: &str, title: &str) -> String {
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

fn yaml_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    format!("\"{escaped}\"")
}

fn markdown_stub(title: &str) -> String {
    format!("# {title}\n\n")
}

pub struct NewDesignPackage<'a> {
    pub design_id: &'a str,
    pub title: &'a str,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DesignPackageInitOutcome {
    pub package_path: PathBuf,
}
