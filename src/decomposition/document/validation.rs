use super::*;

pub(in crate::decomposition) fn fenced_metadata(content: &str) -> Result<Option<&str>> {
    let opening = "```yaml agent-workbench";
    let Some(start) = content.find(opening) else {
        return Ok(None);
    };
    if content[start + opening.len()..].contains(opening) {
        bail!("decomposition plan must contain exactly one metadata block");
    }
    let body_start = start + opening.len();
    let body = content[body_start..]
        .strip_prefix('\n')
        .context("decomposition plan metadata fence must end its line")?;
    let end = body
        .find("\n```")
        .context("decomposition plan metadata fence is unterminated")?;
    Ok(Some(&body[..end]))
}

pub(in crate::decomposition) fn validate_plan(plan: &PlanDocument) -> Result<()> {
    validate_plan_header(plan)?;
    if plan.items.is_empty() || plan.slices.is_empty() {
        bail!("decomposition plan requires items and slices");
    }
    let mut slices = BTreeMap::new();
    let mut orders = BTreeSet::new();
    for slice in &plan.slices {
        require_text(&slice.key, "slice key")?;
        require_text(&slice.title, "slice title")?;
        if slice.order <= 0 || !orders.insert(slice.order) {
            bail!("decomposition slice order must be unique and positive");
        }
        if slices.insert(slice.key.as_str(), slice).is_some() {
            bail!("decomposition slice key must be unique");
        }
    }
    validate_plan_graph(plan, &slices)
}

pub(in crate::decomposition) fn validate_plan_header(plan: &PlanDocument) -> Result<()> {
    if plan.record_type != "decomposition_plan" || plan.format != 1 {
        bail!("decomposition plan type or format is unsupported");
    }
    require_text(&plan.key, "plan key")?;
    require_digest(&plan.design_fingerprint, "design fingerprint")?;
    Ok(())
}

fn validate_plan_graph<'a>(
    plan: &'a PlanDocument,
    slices: &BTreeMap<&'a str, &'a PlanSlice>,
) -> Result<()> {
    for slice in &plan.slices {
        let mut dependencies = BTreeSet::new();
        for dependency in &slice.depends_on {
            if dependency == &slice.key || !slices.contains_key(dependency.as_str()) {
                bail!("decomposition slice dependency is invalid");
            }
            if !dependencies.insert(dependency) {
                bail!("decomposition slice dependency must be unique");
            }
        }
    }
    ensure_acyclic(slices)?;

    let mut items = BTreeSet::new();
    let mut populated_slices = BTreeSet::new();
    for item in &plan.items {
        require_text(&item.key, "item key")?;
        if !items.insert(item.key.as_str()) {
            bail!("decomposition item key must be unique");
        }
        if !slices.contains_key(item.slice.as_str()) {
            bail!("decomposition item references an unknown slice");
        }
        populated_slices.insert(item.slice.as_str());
        require_text(&item.title, "item title")?;
        require_text(&item.details, "item details")?;
        require_text(&item.completion.outcome, "completion outcome")?;
        require_text(&item.completion.observation, "completion observation")?;
        require_text(&item.completion.evidence_owner, "evidence owner")?;
        require_text(&item.completion.evidence_kind, "evidence kind")?;
        if item.requirements.is_empty() || item.checklist.is_empty() {
            bail!("decomposition item requires requirement and checklist coverage");
        }
        unique_nonempty(&item.requirements, "item requirement")?;
        unique_nonempty(&item.completion.gates, "completion gate")?;
        let mut boundaries = BTreeSet::new();
        for boundary in &item.checklist {
            require_text(&boundary.key, "checklist boundary key")?;
            require_text(&boundary.condition, "checklist boundary condition")?;
            require_text(&boundary.evidence_kind, "checklist evidence kind")?;
            if !boundaries.insert(boundary.key.as_str()) {
                bail!("checklist boundary key must be unique within an item");
            }
            unique_nonempty(&boundary.gates, "checklist gate")?;
            if boundary
                .gates
                .iter()
                .any(|gate| !item.completion.gates.contains(gate))
            {
                bail!("checklist gates must be contained by the item completion gates");
            }
        }
    }
    if populated_slices.len() != slices.len() {
        bail!("every decomposition slice must own at least one item");
    }
    if let Some(reconciliation) = &plan.reconciliation {
        validate_reconciliation(plan, reconciliation)?;
    }
    Ok(())
}

fn validate_reconciliation(plan: &PlanDocument, reconciliation: &PlanReconciliation) -> Result<()> {
    if reconciliation.predecessor <= 0 {
        bail!("reconciliation predecessor must be a positive reference");
    }
    require_digest(
        &reconciliation.expected_current,
        "reconciliation expected current",
    )?;
    validate_mapping_sources(
        reconciliation.tasks.iter().map(|mapping| mapping.source),
        "task",
    )?;
    validate_mapping_sources(
        reconciliation
            .checklist
            .iter()
            .map(|mapping| mapping.source),
        "checklist",
    )?;
    validate_mapping_sources(
        reconciliation.gates.iter().map(|mapping| mapping.source),
        "gate",
    )?;
    validate_mapping_sources(
        reconciliation.phases.iter().map(|mapping| mapping.source),
        "phase",
    )?;
    validate_mapping_sources(
        reconciliation
            .dependencies
            .iter()
            .map(|mapping| mapping.source),
        "dependency",
    )?;
    let item_keys = plan
        .items
        .iter()
        .map(|item| item.key.as_str())
        .collect::<BTreeSet<_>>();
    let slice_keys = plan
        .slices
        .iter()
        .map(|slice| slice.key.as_str())
        .collect::<BTreeSet<_>>();
    for mapping in &reconciliation.tasks {
        validate_disposition(
            &mapping.disposition,
            mapping.item.as_deref(),
            mapping.reason.as_deref(),
            mapping.effect,
            "task",
        )?;
        if mapping
            .item
            .as_deref()
            .is_some_and(|item| !item_keys.contains(item))
        {
            bail!("task reconciliation references an unknown item");
        }
    }
    for mapping in &reconciliation.checklist {
        validate_disposition(
            &mapping.disposition,
            mapping.item.as_deref(),
            mapping.reason.as_deref(),
            mapping.effect,
            "checklist",
        )?;
        match mapping.disposition.as_str() {
            "retained" => {
                let item = mapping
                    .item
                    .as_deref()
                    .context("retained checklist mapping requires item")?;
                let boundary = mapping
                    .boundary
                    .as_deref()
                    .context("retained checklist mapping requires boundary")?;
                let exists = plan.items.iter().any(|candidate| {
                    candidate.key == item
                        && candidate
                            .checklist
                            .iter()
                            .any(|candidate| candidate.key == boundary)
                });
                if !exists {
                    bail!("checklist reconciliation references an unknown item boundary");
                }
                if mapping.reason.is_some() {
                    bail!("retained checklist mapping forbids reason");
                }
            }
            "retired" if mapping.boundary.is_some() => {
                bail!("retired checklist mapping forbids boundary")
            }
            _ => {}
        }
    }
    for mapping in &reconciliation.gates {
        validate_disposition(
            &mapping.disposition,
            mapping.item.as_deref(),
            mapping.reason.as_deref(),
            mapping.effect,
            "gate",
        )?;
        match mapping.disposition.as_str() {
            "retained" => {
                let item = mapping
                    .item
                    .as_deref()
                    .context("retained gate mapping requires item")?;
                let gate = mapping
                    .gate
                    .as_deref()
                    .context("retained gate mapping requires gate")?;
                let boundary = mapping
                    .boundary
                    .as_deref()
                    .context("retained gate mapping requires boundary")?;
                require_text(boundary, "retained gate boundary")?;
                let exists = plan.items.iter().any(|candidate| {
                    candidate.key == item
                        && candidate
                            .completion
                            .gates
                            .iter()
                            .any(|candidate| candidate == gate)
                });
                if !exists {
                    bail!("gate reconciliation references an unknown item gate");
                }
                if mapping.reason.is_some() {
                    bail!("retained gate mapping forbids reason");
                }
            }
            "retired" if mapping.gate.is_some() || mapping.boundary.is_some() => {
                bail!("retired gate mapping forbids gate and boundary")
            }
            _ => {}
        }
    }
    for mapping in &reconciliation.phases {
        validate_disposition(
            &mapping.disposition,
            mapping.slice.as_deref(),
            mapping.reason.as_deref(),
            mapping.effect,
            "phase",
        )?;
        if mapping
            .slice
            .as_deref()
            .is_some_and(|slice| !slice_keys.contains(slice))
        {
            bail!("phase reconciliation references an unknown slice");
        }
    }
    for mapping in &reconciliation.dependencies {
        let target = mapping.from.as_deref().zip(mapping.to.as_deref());
        validate_disposition(
            &mapping.disposition,
            target.map(|_| "edge"),
            mapping.reason.as_deref(),
            mapping.effect,
            "dependency",
        )?;
        match mapping.disposition.as_str() {
            "retained" => {
                let (from, to) =
                    target.context("retained dependency mapping requires endpoints")?;
                if !slice_keys.contains(from)
                    || !slice_keys.contains(to)
                    || !plan.slices.iter().any(|slice| {
                        slice.key == to
                            && slice.depends_on.iter().any(|dependency| dependency == from)
                    })
                {
                    bail!("dependency reconciliation references an unknown slice edge");
                }
                if mapping.reason.is_some() {
                    bail!("retained dependency mapping forbids reason");
                }
            }
            "retired" if mapping.from.is_some() || mapping.to.is_some() => {
                bail!("retired dependency mapping forbids endpoints")
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_mapping_sources(sources: impl Iterator<Item = i64>, label: &str) -> Result<()> {
    let mut seen = BTreeSet::new();
    for source in sources {
        if source <= 0 || !seen.insert(source) {
            bail!("reconciliation {label} sources must be unique positive references");
        }
    }
    Ok(())
}

fn validate_disposition(
    disposition: &str,
    retained_target: Option<&str>,
    reason: Option<&str>,
    effect: Option<ReconciliationEffect>,
    label: &str,
) -> Result<()> {
    match disposition {
        "retained" => {
            let target =
                retained_target.context(format!("retained {label} mapping requires target"))?;
            require_text(target, "reconciliation target")?;
            if reason.is_some() {
                bail!("retained {label} mapping forbids reason");
            }
            let _ = normalized_effect(effect);
        }
        "retired" => {
            if retained_target.is_some() {
                bail!("retired {label} mapping forbids target");
            }
            require_text(
                reason.context(format!("retired {label} mapping requires reason"))?,
                "reconciliation reason",
            )?;
            if effect.is_some() {
                bail!("retired {label} mapping forbids lifecycle effect");
            }
        }
        _ => bail!("reconciliation disposition must be retained or retired"),
    }
    Ok(())
}

fn ensure_acyclic(slices: &BTreeMap<&str, &PlanSlice>) -> Result<()> {
    fn visit<'a>(
        key: &'a str,
        slices: &BTreeMap<&'a str, &'a PlanSlice>,
        visiting: &mut BTreeSet<&'a str>,
        visited: &mut BTreeSet<&'a str>,
    ) -> Result<()> {
        if visited.contains(key) {
            return Ok(());
        }
        if !visiting.insert(key) {
            bail!("decomposition slice dependencies contain a cycle");
        }
        for dependency in &slices[key].depends_on {
            visit(dependency, slices, visiting, visited)?;
        }
        visiting.remove(key);
        visited.insert(key);
        Ok(())
    }
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for key in slices.keys() {
        visit(key, slices, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn unique_nonempty(values: &[String], label: &str) -> Result<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        require_text(value, label)?;
        if !seen.insert(value) {
            bail!("{label} must be unique");
        }
    }
    Ok(())
}

fn require_text(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() || value != value.trim() {
        bail!("{label} must be non-empty canonical text");
    }
    Ok(())
}

pub(in crate::decomposition) fn require_key(value: &str, label: &str) -> Result<()> {
    require_text(value, label)?;
    if !value
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-'))
    {
        bail!("{label} must be a portable token");
    }
    Ok(())
}

pub(in crate::decomposition) fn require_digest(value: &str, label: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("{label} must be a SHA-256 identity");
    }
    Ok(())
}

pub(super) fn sorted_directories(root: &Path) -> Result<Vec<PathBuf>> {
    Ok(sorted_entries(root)?
        .into_iter()
        .filter(|path| path.is_dir())
        .collect())
}

pub(in crate::decomposition) fn sorted_entries(root: &Path) -> Result<Vec<PathBuf>> {
    let mut entries = fs::read_dir(root)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    entries.sort();
    Ok(entries)
}
