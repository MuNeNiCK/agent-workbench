use std::collections::{BTreeMap, BTreeSet, VecDeque};

use anyhow::{Context, Result, bail};
use rusqlite::Connection;

use crate::identity::{CanonicalValue, canonical_bytes, domain_digest, signed_source_id};

use super::snapshot::{DatabaseSnapshot, SqliteScalar};

const CANDIDATE_SHARED: &[&str] = &[
    "projects",
    "design_packages",
    "design_versions",
    "design_files",
    "design_requirements",
    "design_decisions",
    "validation_gate_templates",
    "validation_gate_template_requirements",
    "authorities",
    "authority_events",
    "rule_bindings",
    "command_profiles",
    "repositories",
    "repository_snapshots",
    "repository_dirty_entries",
    "repository_state_classifications",
    "repository_snapshot_comparisons",
    "git_commits",
    "git_file_changes",
    "schema_migrations",
    "legacy_migration_candidates",
    "legacy_migration_candidate_members",
    "legacy_migration_edges",
    "legacy_migration_projections",
    "legacy_reviewer_bindings",
    "legacy_claim_audits",
    "legacy_finding_audits",
    "authority_bootstrap_targets",
    "review_boundary_snapshots",
    "review_correction_events",
    "review_correction_recovery_obligations",
    "finding_decision_epochs",
    "decision_continuations",
    "authority_migration_sources",
    "legacy_adjudication_migrations",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Node {
    family: usize,
    row: usize,
}

#[derive(Clone, Debug)]
struct Edge {
    source: Node,
    target: Node,
}

#[derive(Clone, Debug)]
pub(super) struct ComponentSnapshot {
    pub(super) owner_id: i64,
    pub(super) owner_digest: String,
    pub(super) component_digest: String,
    pub(super) source_digest: String,
}

pub(super) struct OwnershipSnapshot {
    pub(super) components: BTreeMap<i64, ComponentSnapshot>,
    pub(super) conflicted_owners: BTreeSet<i64>,
}

pub(super) fn classify(
    conn: &Connection,
    database: &DatabaseSnapshot,
) -> Result<OwnershipSnapshot> {
    let nodes = nodes(database);
    let positions = nodes
        .iter()
        .enumerate()
        .map(|(position, node)| (*node, position))
        .collect::<BTreeMap<_, _>>();
    let edges = read_edges(conn, database)?;
    let mut adjacency = vec![Vec::new(); nodes.len()];
    for edge in &edges {
        adjacency[positions[&edge.source]].push(positions[&edge.target]);
    }
    for targets in &mut adjacency {
        targets.sort_unstable();
        targets.dedup();
    }
    let mut reverse_adjacency = vec![Vec::new(); nodes.len()];
    for (source, targets) in adjacency.iter().enumerate() {
        for target in targets {
            reverse_adjacency[*target].push(source);
        }
    }
    for sources in &mut reverse_adjacency {
        sources.sort_unstable();
        sources.dedup();
    }
    let (component_of, members) = strongly_connected(&adjacency);
    let mut component_edges = vec![BTreeSet::new(); members.len()];
    for (source, targets) in adjacency.iter().enumerate() {
        for target in targets {
            let from = component_of[source];
            let to = component_of[*target];
            if from != to {
                component_edges[from].insert(to);
            }
        }
    }
    let mut owners = vec![BTreeSet::new(); members.len()];
    for (position, node) in nodes.iter().enumerate() {
        if database.families[node.family].name == "work_units" {
            owners[component_of[position]].insert(row_id(database, *node)?);
        }
    }
    let topological = topological_order(&component_edges)?;
    for component in topological.into_iter().rev() {
        let reachable = component_edges[component]
            .iter()
            .flat_map(|target| owners[*target].iter().copied())
            .collect::<Vec<_>>();
        owners[component].extend(reachable);
    }
    let node_owners = nodes
        .iter()
        .enumerate()
        .map(|(position, _)| owners[component_of[position]].clone())
        .collect::<Vec<_>>();
    validate_classification(database, &nodes, &node_owners)?;

    let mut conflicted_owners = BTreeSet::new();
    for owner_set in &node_owners {
        if owner_set.len() > 1 {
            conflicted_owners.extend(owner_set);
        }
    }
    for edge in &edges {
        let source = &node_owners[positions[&edge.source]];
        let target = &node_owners[positions[&edge.target]];
        match (single(source), single(target)) {
            (Some(left), Some(right)) if left != right => {
                conflicted_owners.insert(left);
                conflicted_owners.insert(right);
            }
            (None, Some(_)) if source.is_empty() => {
                bail!("task-history migration source classification is inconsistent");
            }
            _ => {}
        }
    }

    let mut components = BTreeMap::new();
    let owner_ids = node_owners
        .iter()
        .filter_map(single)
        .collect::<BTreeSet<_>>();
    for owner_id in owner_ids {
        let owned = nodes
            .iter()
            .enumerate()
            .filter(|(position, _)| node_owners[*position] == BTreeSet::from([owner_id]))
            .map(|(_, node)| *node)
            .collect::<BTreeSet<_>>();
        let shared = shared_incidence(
            &owned,
            &nodes,
            &positions,
            &adjacency,
            &reverse_adjacency,
            &node_owners,
        );
        let component = component_snapshot(database, owner_id, &owned, &shared)?;
        components.insert(owner_id, component);
    }
    Ok(OwnershipSnapshot {
        components,
        conflicted_owners,
    })
}

fn nodes(database: &DatabaseSnapshot) -> Vec<Node> {
    database
        .families
        .iter()
        .enumerate()
        .flat_map(|(family, snapshot)| {
            (0..snapshot.rows.len()).map(move |row| Node { family, row })
        })
        .collect()
}

fn read_edges(conn: &Connection, database: &DatabaseSnapshot) -> Result<Vec<Edge>> {
    let family_positions = database
        .families
        .iter()
        .enumerate()
        .map(|(index, family)| (family.name.as_str(), index))
        .collect::<BTreeMap<_, _>>();
    let mut edges = Vec::new();
    for (source_family_index, source_family) in database.families.iter().enumerate() {
        let escaped = source_family.name.replace('"', "\"\"");
        let mut statement = conn.prepare(&format!("pragma foreign_key_list(\"{escaped}\")"))?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut groups = BTreeMap::<i64, Vec<(i64, String, String, String)>>::new();
        for (id, sequence, target_family, source_column, target_column) in rows {
            let target_column = target_column
                .context("task-history migration source foreign key lacks an explicit target")?;
            groups.entry(id).or_default().push((
                sequence,
                target_family,
                source_column,
                target_column,
            ));
        }
        for group in groups.values_mut() {
            group.sort_by_key(|item| item.0);
            let target_name = &group[0].1;
            if group.iter().any(|item| &item.1 != target_name) {
                bail!("task-history migration source foreign key is malformed");
            }
            let target_family_index = *family_positions
                .get(target_name.as_str())
                .context("task-history migration source foreign key leaves the profile")?;
            let target_family = &database.families[target_family_index];
            let mut target_index = BTreeMap::<Vec<u8>, Vec<usize>>::new();
            for (row_index, row) in target_family.rows.iter().enumerate() {
                let values = group
                    .iter()
                    .map(|item| row.cells.get(&item.3))
                    .collect::<Option<Vec<_>>>()
                    .context("task-history migration target column is absent")?;
                let key = scalar_key(&values);
                target_index.entry(key).or_default().push(row_index);
            }
            for (source_row_index, source_row) in source_family.rows.iter().enumerate() {
                let source_values = group
                    .iter()
                    .map(|item| source_row.cells.get(&item.2))
                    .collect::<Option<Vec<_>>>()
                    .context("task-history migration source foreign key column is absent")?;
                if source_values
                    .iter()
                    .any(|value| matches!(value, SqliteScalar::Null))
                {
                    continue;
                }
                let matches = target_index.get(&scalar_key(&source_values));
                let [target_row_index] = matches.map(Vec::as_slice).unwrap_or_default() else {
                    bail!("task-history migration source foreign key is unresolved");
                };
                edges.push(Edge {
                    source: Node {
                        family: source_family_index,
                        row: source_row_index,
                    },
                    target: Node {
                        family: target_family_index,
                        row: *target_row_index,
                    },
                });
            }
        }
    }
    edges.sort_by_key(|edge| (edge.source, edge.target));
    edges.dedup_by_key(|edge| (edge.source, edge.target));
    Ok(edges)
}

fn scalar_key(values: &[&SqliteScalar]) -> Vec<u8> {
    canonical_bytes(&CanonicalValue::Array(
        values.iter().map(|value| value.canonical()).collect(),
    ))
}

fn strongly_connected(adjacency: &[Vec<usize>]) -> (Vec<usize>, Vec<Vec<usize>>) {
    let mut visited = vec![false; adjacency.len()];
    let mut order = Vec::with_capacity(adjacency.len());
    for start in 0..adjacency.len() {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut stack = vec![(start, 0_usize)];
        while let Some((node, next)) = stack.last_mut() {
            if *next < adjacency[*node].len() {
                let target = adjacency[*node][*next];
                *next += 1;
                if !visited[target] {
                    visited[target] = true;
                    stack.push((target, 0));
                }
            } else {
                order.push(*node);
                stack.pop();
            }
        }
    }
    let mut reverse = vec![Vec::new(); adjacency.len()];
    for (source, targets) in adjacency.iter().enumerate() {
        for target in targets {
            reverse[*target].push(source);
        }
    }
    let mut component_of = vec![usize::MAX; adjacency.len()];
    let mut members = Vec::new();
    for start in order.into_iter().rev() {
        if component_of[start] != usize::MAX {
            continue;
        }
        let component = members.len();
        let mut group = Vec::new();
        let mut stack = vec![start];
        component_of[start] = component;
        while let Some(node) = stack.pop() {
            group.push(node);
            for source in &reverse[node] {
                if component_of[*source] == usize::MAX {
                    component_of[*source] = component;
                    stack.push(*source);
                }
            }
        }
        group.sort_unstable();
        members.push(group);
    }
    (component_of, members)
}

fn topological_order(edges: &[BTreeSet<usize>]) -> Result<Vec<usize>> {
    let mut indegree = vec![0_usize; edges.len()];
    for targets in edges {
        for target in targets {
            indegree[*target] += 1;
        }
    }
    let mut ready = indegree
        .iter()
        .enumerate()
        .filter(|(_, degree)| **degree == 0)
        .map(|(index, _)| index)
        .collect::<BTreeSet<_>>();
    let mut order = Vec::with_capacity(edges.len());
    while let Some(component) = ready.pop_first() {
        order.push(component);
        for target in &edges[component] {
            indegree[*target] -= 1;
            if indegree[*target] == 0 {
                ready.insert(*target);
            }
        }
    }
    if order.len() != edges.len() {
        bail!("task-history migration source component graph is cyclic");
    }
    Ok(order)
}

fn validate_classification(
    database: &DatabaseSnapshot,
    nodes: &[Node],
    owners: &[BTreeSet<i64>],
) -> Result<()> {
    for (node, owner_set) in nodes.iter().zip(owners) {
        let family = database.families[node.family].name.as_str();
        if !CANDIDATE_SHARED.contains(&family) && owner_set.len() != 1 {
            bail!("task-history migration source owner is unreadable");
        }
    }
    Ok(())
}

fn shared_incidence(
    owned: &BTreeSet<Node>,
    nodes: &[Node],
    positions: &BTreeMap<Node, usize>,
    adjacency: &[Vec<usize>],
    reverse_adjacency: &[Vec<usize>],
    owners: &[BTreeSet<i64>],
) -> BTreeSet<Node> {
    let mut shared = BTreeSet::new();
    let mut queue = VecDeque::new();
    for node in owned {
        for target in &adjacency[positions[node]] {
            if owners[*target].is_empty() {
                queue.push_back(nodes[*target]);
            }
        }
        for source in &reverse_adjacency[positions[node]] {
            if owners[*source].is_empty() {
                queue.push_back(nodes[*source]);
            }
        }
    }
    while let Some(node) = queue.pop_front() {
        if !shared.insert(node) {
            continue;
        }
        for target in &adjacency[positions[&node]] {
            if owners[*target].is_empty() {
                queue.push_back(nodes[*target]);
            }
        }
        for source in &reverse_adjacency[positions[&node]] {
            if owners[*source].is_empty() {
                queue.push_back(nodes[*source]);
            }
        }
    }
    shared
}

fn component_snapshot(
    database: &DatabaseSnapshot,
    owner_id: i64,
    owned: &BTreeSet<Node>,
    shared: &BTreeSet<Node>,
) -> Result<ComponentSnapshot> {
    let owner_digest = domain_digest(
        b"AWB-OWNER-v1\0",
        &CanonicalValue::object([("work", CanonicalValue::string(signed_source_id(owner_id)?))]),
    );
    let component_input = CanonicalValue::object([
        ("owner", CanonicalValue::string(owner_digest.clone())),
        ("owned", CanonicalValue::Array(ref_values(database, owned))),
        (
            "shared",
            CanonicalValue::Array(ref_values(database, shared)),
        ),
    ]);
    let component_digest = domain_digest(b"AWB-COMPONENT-v1\0", &component_input);
    let selected = owned.union(shared).copied().collect::<BTreeSet<_>>();
    let source_value = CanonicalValue::object([
        (
            "profile",
            CanonicalValue::string(database.profile_id.clone()),
        ),
        (
            "scope",
            CanonicalValue::object([
                ("owner_digest", CanonicalValue::string(owner_digest.clone())),
                (
                    "component_sha256",
                    CanonicalValue::string(component_digest.clone()),
                ),
            ]),
        ),
        (
            "sqlite_schema_version",
            CanonicalValue::Integer(database.schema_version),
        ),
        (
            "application_id",
            CanonicalValue::Integer(database.application_id),
        ),
        (
            "user_version",
            CanonicalValue::Integer(database.user_version),
        ),
        (
            "families",
            CanonicalValue::Array(component_families(database, &selected)),
        ),
    ]);
    let source_digest = domain_digest(b"AWB-SOURCE-SNAPSHOT-v1\0", &source_value);
    Ok(ComponentSnapshot {
        owner_id,
        owner_digest,
        component_digest,
        source_digest,
    })
}

fn ref_values(database: &DatabaseSnapshot, nodes: &BTreeSet<Node>) -> Vec<CanonicalValue> {
    nodes
        .iter()
        .map(|node| {
            let family = &database.families[node.family];
            let row = &family.rows[node.row];
            CanonicalValue::object([
                ("family", CanonicalValue::string(family.name.clone())),
                ("identity", row.identity.clone()),
                (
                    "payload_sha256",
                    CanonicalValue::string(domain_digest(b"AWB-SOURCE-ROW-v1\0", &row.values)),
                ),
            ])
        })
        .collect()
}

fn component_families(
    database: &DatabaseSnapshot,
    selected: &BTreeSet<Node>,
) -> Vec<CanonicalValue> {
    database
        .families
        .iter()
        .enumerate()
        .map(|(family_index, family)| {
            CanonicalValue::object([
                ("family", CanonicalValue::string(family.name.clone())),
                (
                    "columns",
                    CanonicalValue::Array(
                        family
                            .columns
                            .iter()
                            .map(|column| {
                                CanonicalValue::object([
                                    ("name", CanonicalValue::string(column.name.clone())),
                                    ("type", CanonicalValue::string(column.declared_type.clone())),
                                ])
                            })
                            .collect(),
                    ),
                ),
                (
                    "rows",
                    CanonicalValue::Array(
                        family
                            .rows
                            .iter()
                            .enumerate()
                            .filter(|(row, _)| {
                                selected.contains(&Node {
                                    family: family_index,
                                    row: *row,
                                })
                            })
                            .map(|(_, row)| row.values.clone())
                            .collect(),
                    ),
                ),
            ])
        })
        .collect()
}

fn row_id(database: &DatabaseSnapshot, node: Node) -> Result<i64> {
    database.families[node.family].rows[node.row]
        .cells
        .get("id")
        .context("task-history migration source owner lacks identity")?
        .as_positive_id()
}

fn single(values: &BTreeSet<i64>) -> Option<i64> {
    let mut values = values.iter();
    let value = *values.next()?;
    values.next().is_none().then_some(value)
}
