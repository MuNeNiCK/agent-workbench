use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::*;

struct CommandShape {
    children: Vec<String>,
    invocable_without_child: bool,
}

fn command_shape(root: &Path, path: &[String]) -> CommandShape {
    let args = path
        .iter()
        .map(String::as_str)
        .chain(std::iter::once("--help"))
        .collect::<Vec<_>>();
    let output = aw(root, &args);
    assert!(
        output.status.success(),
        "public command help failed for {path:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let help = String::from_utf8(output.stdout).expect("help must be utf-8");
    let mut in_commands = false;
    let mut children = Vec::new();
    for line in help.lines() {
        if line == "Commands:" {
            in_commands = true;
            continue;
        }
        if !in_commands {
            continue;
        }
        if line.is_empty() {
            break;
        }
        let Some(entry) = line.strip_prefix("  ") else {
            break;
        };
        if entry.starts_with(char::is_whitespace) {
            continue;
        }
        let Some(name) = entry.split_ascii_whitespace().next() else {
            continue;
        };
        if name != "help" {
            children.push(name.to_string());
        }
    }
    CommandShape {
        children,
        invocable_without_child: help
            .lines()
            .any(|line| line.starts_with("Usage:") && line.contains("[COMMAND]")),
    }
}

fn discover_leaves(root: &Path) -> BTreeSet<Vec<String>> {
    let mut leaves = BTreeSet::new();
    let mut pending = vec![Vec::<String>::new()];
    while let Some(path) = pending.pop() {
        let shape = command_shape(root, &path);
        if shape.children.is_empty() {
            assert!(!path.is_empty(), "the public command tree has no leaves");
            leaves.insert(path);
            continue;
        }
        if shape.invocable_without_child {
            leaves.insert(path.clone());
        }
        for child in shape.children.into_iter().rev() {
            let mut next = path.clone();
            next.push(child);
            pending.push(next);
        }
    }
    leaves
}

fn skill_root() -> PathBuf {
    std::env::var_os("AGENT_WORKBENCH_SKILL_UNDER_TEST")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("skills/agent-workbench"))
}

fn docs_root() -> PathBuf {
    std::env::var_os("AGENT_WORKBENCH_DOCS_UNDER_TEST")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("docs"))
}

fn skill_command_entries() -> Vec<Vec<String>> {
    let skill = std::fs::read_to_string(skill_root().join("SKILL.md"))
        .expect("installed skill must contain SKILL.md");
    let scope = skill
        .split_once("## Command Scope")
        .expect("installed skill must declare Command Scope")
        .1
        .split_once("Load only the reference needed")
        .expect("installed skill must terminate its command inventory")
        .0;
    scope
        .lines()
        .filter_map(|line| {
            let command = line.strip_prefix("- `agent-workbench ")?;
            let command = command.strip_suffix('`')?;
            Some(
                command
                    .split_ascii_whitespace()
                    .map(str::to_string)
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}

fn markdown_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "md") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn clean_token(token: &str) -> String {
    token
        .trim_matches(|character: char| {
            matches!(
                character,
                '`' | '\'' | '"' | ',' | '.' | ';' | '(' | ')' | '[' | ']'
            )
        })
        .to_string()
}

fn documented_invocations() -> Vec<(PathBuf, usize, Vec<String>)> {
    let mut invocations = Vec::new();
    let paths = [skill_root(), docs_root()]
        .into_iter()
        .flat_map(|root| markdown_files(&root))
        .collect::<Vec<_>>();
    for path in paths {
        let source = std::fs::read_to_string(&path).unwrap();
        for (line_index, line) in source.lines().enumerate() {
            for segment in line.split("agent-workbench ").skip(1) {
                let segment = segment.split('`').next().unwrap_or(segment);
                let tokens = segment
                    .split_ascii_whitespace()
                    .map(clean_token)
                    .take_while(|token| !token.is_empty())
                    .collect::<Vec<_>>();
                if tokens
                    .first()
                    .is_some_and(|token| !token.starts_with('<') && token != "...")
                {
                    invocations.push((path.clone(), line_index + 1, tokens));
                }
            }
        }
    }
    invocations
}

fn matching_leaf<'a>(entry: &[String], leaves: &'a BTreeSet<Vec<String>>) -> Vec<&'a Vec<String>> {
    let candidates = leaves
        .iter()
        .filter(|leaf| entry.starts_with(leaf.as_slice()))
        .collect::<Vec<_>>();
    let longest = candidates.iter().map(|leaf| leaf.len()).max();
    candidates
        .into_iter()
        .filter(|leaf| Some(leaf.len()) == longest)
        .collect()
}

#[test]
fn installed_skill_and_public_command_tree_are_bidirectionally_complete() {
    let root = tempfile::tempdir().unwrap();
    let leaves = discover_leaves(root.path());
    let entries = skill_command_entries();
    let mut matched = BTreeMap::<Vec<String>, usize>::new();
    for entry in &entries {
        let candidates = matching_leaf(entry, &leaves);
        assert_eq!(
            candidates.len(),
            1,
            "installed skill command must resolve to exactly one public leaf: {entry:?}; matches={candidates:?}"
        );
        *matched.entry(candidates[0].clone()).or_default() += 1;
    }
    let missing = leaves
        .difference(&matched.keys().cloned().collect())
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "public command leaves missing from the installed skill: {missing:?}"
    );
}

#[test]
fn installed_skill_recipe_operands_belong_to_the_resolved_public_leaf() {
    let root = tempfile::tempdir().unwrap();
    let leaves = discover_leaves(root.path());
    let top_level = leaves
        .iter()
        .filter_map(|leaf| leaf.first().cloned())
        .collect::<BTreeSet<_>>();
    let mut help_by_leaf = BTreeMap::<Vec<String>, String>::new();
    for (path, line, invocation) in documented_invocations() {
        if !invocation
            .first()
            .is_some_and(|command| top_level.contains(command))
        {
            continue;
        }
        let candidates = matching_leaf(&invocation, &leaves);
        assert_eq!(
            candidates.len(),
            1,
            "installed recipe does not resolve to one public leaf at {}:{line}: {invocation:?}",
            path.display()
        );
        let leaf = candidates[0].clone();
        let help = help_by_leaf.entry(leaf.clone()).or_insert_with(|| {
            let args = leaf
                .iter()
                .map(String::as_str)
                .chain(std::iter::once("--help"))
                .collect::<Vec<_>>();
            let output = aw(root.path(), &args);
            assert!(output.status.success());
            String::from_utf8(output.stdout).unwrap()
        });
        for option in invocation.iter().filter(|token| token.starts_with("--")) {
            let option = option.split('=').next().unwrap();
            assert!(
                help.contains(option),
                "installed recipe supplies an option absent from its public leaf at {}:{line}: leaf={leaf:?} option={option}",
                path.display()
            );
        }
    }
}

#[test]
fn every_public_leaf_rejects_an_unknown_option_before_product_mutation() {
    let root = tempfile::tempdir().unwrap();
    for leaf in discover_leaves(root.path()) {
        let args = leaf
            .iter()
            .map(String::as_str)
            .chain(std::iter::once("--agent-workbench-unknown-contract-option"))
            .collect::<Vec<_>>();
        let output = aw(root.path(), &args);
        assert!(
            !output.status.success(),
            "public leaf accepted an undeclared option: {leaf:?}"
        );
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("unexpected argument") || stderr.contains("unexpected option"),
            "public leaf did not reject the option at its parser boundary: {leaf:?}\nstderr:\n{stderr}"
        );
        assert!(
            !root.path().join(".agent-workbench").exists(),
            "invalid public input changed project state: {leaf:?}"
        );
    }
}

#[test]
fn every_public_leaf_reaches_its_owner_with_the_declared_required_shape() {
    let discovery_root = tempfile::tempdir().unwrap();
    for leaf in discover_leaves(discovery_root.path()) {
        let help_args = leaf
            .iter()
            .map(String::as_str)
            .chain(std::iter::once("--help"))
            .collect::<Vec<_>>();
        let help = ok(discovery_root.path(), &help_args);
        let usage = help
            .lines()
            .find_map(|line| line.strip_prefix("Usage: agent-workbench "))
            .expect("public leaf help must expose one usage shape");
        let mut suffix = usage.split_ascii_whitespace().skip(leaf.len()).peekable();
        let mut required = Vec::<String>::new();
        while let Some(token) = suffix.next() {
            if token == "[OPTIONS]" || token.starts_with('[') {
                continue;
            }
            if token.starts_with("<--") {
                required.push(
                    token
                        .trim_start_matches('<')
                        .trim_end_matches(['>', '|'])
                        .to_string(),
                );
                if suffix.peek().is_some_and(|next| next.starts_with('<')) {
                    let alternative = suffix.next().unwrap();
                    required.push("1".to_string());
                    if alternative.contains("|--")
                        && !alternative.ends_with('>')
                        && suffix.peek().is_some_and(|next| next.starts_with('<'))
                    {
                        suffix.next();
                    }
                }
            } else if token.starts_with("--") {
                required.push(token.to_string());
                if suffix.peek().is_some_and(|next| next.starts_with('<')) {
                    suffix.next();
                    required.push("1".to_string());
                }
            } else if token.starts_with('<') {
                required.push("1".to_string());
            }
        }
        let args = leaf
            .iter()
            .chain(required.iter())
            .map(String::as_str)
            .collect::<Vec<_>>();
        let root = tempfile::tempdir().unwrap();
        let output = aw(root.path(), &args);
        if output.status.success() {
            continue;
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !stderr.contains("Usage:")
                && !stderr.contains("unexpected argument")
                && !stderr.contains("required arguments were not provided"),
            "public leaf did not reach its command owner with its documented required shape: {leaf:?}\nargs={args:?}\nstderr:\n{stderr}"
        );
    }
}

#[test]
fn retired_cryptographic_authority_routes_are_not_public() {
    let root = tempfile::tempdir().unwrap();
    for route in [
        ["authority", "assertion"].as_slice(),
        ["authority", "provider"].as_slice(),
        ["authority", "grant"].as_slice(),
        ["owner", "grant"].as_slice(),
        ["principal", "resolve"].as_slice(),
        ["decision", "capability"].as_slice(),
    ] {
        let before = std::fs::read_dir(root.path()).unwrap().count();
        let output = aw(root.path(), route);
        assert!(
            !output.status.success(),
            "retired route remained public: {route:?}"
        );
        assert_eq!(
            std::fs::read_dir(root.path()).unwrap().count(),
            before,
            "retired route changed project state: {route:?}"
        );
    }
}

#[test]
fn help_route_alternate_projects_the_same_public_leaf() {
    let temp = tempfile::tempdir().unwrap();
    let positional = ok(temp.path(), &["help", "review", "plan", "add"]);
    let alternate = ok(temp.path(), &["help", "--route", "review/plan/add"]);
    for output in [&positional, &alternate] {
        assert!(output.contains("Usage: agent-workbench review plan add"));
        assert!(output.contains("--work-unit"));
        assert!(output.contains("--type"));
    }
    let invalid = aw(temp.path(), &["help", "--route", "review/missing"]);
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("unknown help route"));
    assert!(!temp.path().join(".agent-workbench").exists());
}
