use super::*;

mod assembly;
mod supersession;
pub use assembly::assemble_release_candidate;
pub use supersession::supersede_release_candidate;

#[derive(Clone, Copy)]
pub(super) enum Layer {
    Local,
    RequestedSource,
    ObservedSource,
    RequestedAsset,
    ObservedAsset,
    Downloaded,
}

pub(super) fn transition_with_observations<F>(
    root: &Path,
    candidate_handle: &str,
    expected_current: &str,
    action: &str,
    idempotency_key: &str,
    observations: &[ReleaseObservation],
    mutate: F,
) -> Result<ReleaseTransitionOutcome>
where
    F: FnOnce(
        &mut CurrentRelease,
        &BTreeMap<String, String>,
    ) -> Result<(&'static str, &'static str, Option<String>)>,
{
    let values = observation_map(observations)?;
    require_nonempty(idempotency_key, "release idempotency key")?;
    let mut payload = vec![format!("idempotency={idempotency_key}")];
    payload.extend(
        values
            .iter()
            .map(|(name, identity)| format!("{name}={identity}"))
            .collect::<Vec<_>>(),
    );
    transition(
        root,
        candidate_handle,
        expected_current,
        action,
        &payload,
        |current| mutate(current, &values),
    )
}

pub(super) struct ReleaseTransitionRequest<'a> {
    pub(super) root: &'a Path,
    pub(super) candidate_handle: &'a str,
    pub(super) expected_current: &'a str,
    pub(super) action: &'a str,
    pub(super) idempotency_key: &'a str,
}

pub(super) fn transition_with_two_observation_sets<F>(
    request: ReleaseTransitionRequest<'_>,
    first: &[ReleaseObservation],
    second: &[ReleaseObservation],
    mutate: F,
) -> Result<ReleaseTransitionOutcome>
where
    F: FnOnce(
        &mut CurrentRelease,
        &BTreeMap<String, String>,
        &BTreeMap<String, String>,
    ) -> Result<(&'static str, &'static str, Option<String>)>,
{
    let first = observation_map(first)?;
    let second = observation_map(second)?;
    require_nonempty(request.idempotency_key, "release idempotency key")?;
    let mut payload = vec![format!("idempotency={}", request.idempotency_key)];
    payload.extend(
        first
            .iter()
            .map(|(name, identity)| format!("requested:{name}={identity}"))
            .collect::<Vec<_>>(),
    );
    payload.extend(
        second
            .iter()
            .map(|(name, identity)| format!("observed:{name}={identity}")),
    );
    transition(
        request.root,
        request.candidate_handle,
        request.expected_current,
        request.action,
        &payload,
        |current| mutate(current, &first, &second),
    )
}

pub(super) fn transition_without_observations<F>(
    root: &Path,
    candidate_handle: &str,
    expected_current: &str,
    action: &str,
    idempotency_key: &str,
    payload: &[&str],
    mutate: F,
) -> Result<ReleaseTransitionOutcome>
where
    F: FnOnce(&mut CurrentRelease) -> Result<(&'static str, &'static str, Option<String>)>,
{
    require_nonempty(idempotency_key, "release idempotency key")?;
    let mut values = vec![format!("idempotency={idempotency_key}")];
    values.extend(
        payload
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>(),
    );
    transition(
        root,
        candidate_handle,
        expected_current,
        action,
        &values,
        mutate,
    )
}

pub(super) fn transition<F>(
    root: &Path,
    candidate_handle: &str,
    expected_current: &str,
    action: &str,
    payload: &[String],
    mutate: F,
) -> Result<ReleaseTransitionOutcome>
where
    F: FnOnce(&mut CurrentRelease) -> Result<(&'static str, &'static str, Option<String>)>,
{
    require_nonempty(expected_current, "expected current release revision")?;
    let request = request_identity(
        action,
        expected_current,
        &payload.iter().map(String::as_str).collect::<Vec<_>>(),
    );
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let mut current = load_current(&tx, project_id, candidate_handle)?;
    if current.revision_handle != expected_current {
        if let Some(applied) =
            find_applied_request(&tx, project_id, current.candidate_id, &request)?
        {
            tx.commit()?;
            return Ok(outcome(&applied, true));
        }
        stale_revision(candidate_handle, &current.revision_handle)?;
    }
    if !matches!(action, "reconcile" | "withdraw" | "supersede") {
        revalidate_release_work_boundary(&tx, project_id, root, current.candidate_id)?;
    }
    let (state, stage, reason) = mutate(&mut current)?;
    let applied = insert_revision(
        &tx,
        project_id,
        current,
        action,
        &request,
        state,
        stage,
        reason.as_deref(),
    )?;
    tx.commit()?;
    Ok(outcome(&applied, false))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn insert_revision(
    tx: &Transaction<'_>,
    project_id: i64,
    current: CurrentRelease,
    action: &str,
    request: &str,
    state: &str,
    stage: &str,
    reason: Option<&str>,
) -> Result<CurrentRelease> {
    tx.execute(
        "update release_candidate_revisions set head_state='historical' where id=?1 and head_state='current'",
        [current.revision_id],
    )?;
    let revision = current.revision + 1;
    let handle = revision_handle(&current.candidate_handle, revision, state, request);
    tx.execute(
        r#"
        insert into release_candidate_revisions(
          project_id,release_candidate_id,revision_handle,revision,state,stage,action,
          request_identity,predecessor_id,head_state,reason,created_at
        ) values(?1,?2,?3,?4,?5,?6,?7,?8,?9,'current',?10,current_timestamp)
        "#,
        params![
            project_id,
            current.candidate_id,
            handle,
            revision,
            state,
            stage,
            action,
            request,
            current.revision_id,
            reason
        ],
    )?;
    let revision_id = tx.last_insert_rowid();
    for subject in &current.subjects {
        insert_subject(tx, project_id, revision_id, subject)?;
    }
    tx.execute(
        "update release_candidates set status=?1,updated_at=current_timestamp where id=?2",
        params![state, current.candidate_id],
    )?;
    load_current(tx, project_id, &current.candidate_handle)
}

pub(super) fn load_current(
    conn: &Connection,
    project_id: i64,
    candidate_handle: &str,
) -> Result<CurrentRelease> {
    let mut current = conn
        .query_row(
            r#"
            select candidate.id,candidate.candidate_handle,revision.id,revision.revision_handle,
                   revision.revision,revision.state,revision.stage,revision.action,revision.reason,
                   boundary.work_unit_id
            from release_candidates candidate
            join release_candidate_revisions revision
              on revision.release_candidate_id=candidate.id and revision.head_state='current'
            left join release_candidate_boundaries boundary
              on boundary.release_candidate_id=candidate.id
            where candidate.project_id=?1 and candidate.candidate_handle=?2
            "#,
            params![project_id, candidate_handle],
            |row| {
                Ok(CurrentRelease {
                    candidate_id: row.get(0)?,
                    candidate_handle: row.get(1)?,
                    revision_id: row.get(2)?,
                    revision_handle: row.get(3)?,
                    revision: row.get(4)?,
                    state: row.get(5)?,
                    stage: row.get(6)?,
                    action: row.get(7)?,
                    reason: row.get(8)?,
                    work_unit_id: row.get(9)?,
                    subjects: Vec::new(),
                })
            },
        )
        .with_context(|| {
            format!("release candidate not found or has no current revision: {candidate_handle}")
        })?;
    current.subjects = load_subjects(conn, current.revision_id)?;
    Ok(current)
}

pub(super) fn load_subjects(
    conn: &Connection,
    revision_id: i64,
) -> Result<Vec<ReleaseSubjectRecord>> {
    let mut statement = conn.prepare(
        r#"
        select subject_kind,subject_name,expected_identity,local_identity,requested_identity,
               observed_identity,downloaded_identity
        from release_candidate_subject_revisions
        where release_candidate_revision_id=?1
        order by subject_kind,subject_name
        "#,
    )?;
    statement
        .query_map([revision_id], |row| {
            Ok(ReleaseSubjectRecord {
                kind: row.get(0)?,
                name: row.get(1)?,
                expected_identity: row.get(2)?,
                local_identity: row.get(3)?,
                requested_identity: row.get(4)?,
                observed_identity: row.get(5)?,
                downloaded_identity: row.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

pub(super) fn insert_subject(
    tx: &Transaction<'_>,
    project_id: i64,
    revision_id: i64,
    subject: &ReleaseSubjectRecord,
) -> Result<()> {
    insert_subject_connection(tx, project_id, revision_id, subject)
}

pub(super) fn insert_subject_connection(
    conn: &Connection,
    project_id: i64,
    revision_id: i64,
    subject: &ReleaseSubjectRecord,
) -> Result<()> {
    conn.execute(
        r#"
        insert into release_candidate_subject_revisions(
          project_id,release_candidate_revision_id,subject_kind,subject_name,expected_identity,
          local_identity,requested_identity,observed_identity,downloaded_identity
        ) values(?1,?2,?3,?4,?5,?6,?7,?8,?9)
        "#,
        params![
            project_id,
            revision_id,
            subject.kind,
            subject.name,
            subject.expected_identity,
            subject.local_identity,
            subject.requested_identity,
            subject.observed_identity,
            subject.downloaded_identity
        ],
    )?;
    Ok(())
}

pub(super) fn apply_layer(
    subjects: &mut [ReleaseSubjectRecord],
    values: &BTreeMap<String, String>,
    layer: Layer,
) -> Result<()> {
    let expected = subjects
        .iter()
        .filter(|subject| layer_applies(subject, layer))
        .map(|subject| subject.name.clone())
        .collect::<BTreeSet<_>>();
    let supplied = values.keys().cloned().collect::<BTreeSet<_>>();
    if expected != supplied {
        let missing = expected.difference(&supplied).cloned().collect::<Vec<_>>();
        let unexpected = supplied.difference(&expected).cloned().collect::<Vec<_>>();
        bail!(
            "release observations do not match the required subject set; missing: {}; unexpected: {}",
            display_names(&missing),
            display_names(&unexpected)
        );
    }
    for subject in subjects {
        if !layer_applies(subject, layer) {
            continue;
        }
        let value = values
            .get(&subject.name)
            .context("validated release observation disappeared")?
            .clone();
        match layer {
            Layer::Local => subject.local_identity = Some(value),
            Layer::RequestedSource | Layer::RequestedAsset => {
                subject.requested_identity = Some(value)
            }
            Layer::ObservedSource | Layer::ObservedAsset => subject.observed_identity = Some(value),
            Layer::Downloaded => subject.downloaded_identity = Some(value),
        }
    }
    Ok(())
}

pub(super) fn layer_applies(subject: &ReleaseSubjectRecord, layer: Layer) -> bool {
    match layer {
        Layer::Local => matches!(subject.kind.as_str(), "local" | "asset"),
        Layer::RequestedSource | Layer::ObservedSource => subject.kind == "source",
        Layer::RequestedAsset | Layer::ObservedAsset => {
            matches!(subject.kind.as_str(), "release" | "asset")
        }
        Layer::Downloaded => subject.kind == "asset",
    }
}

pub(super) fn identities_equal(subject: &ReleaseSubjectRecord, layers: &[Layer]) -> bool {
    layers.iter().all(|layer| match layer {
        Layer::Local => subject.local_identity.as_deref() == Some(&subject.expected_identity),
        Layer::RequestedSource | Layer::RequestedAsset => {
            subject.requested_identity.as_deref() == Some(&subject.expected_identity)
        }
        Layer::ObservedSource | Layer::ObservedAsset => {
            subject.observed_identity.as_deref() == Some(&subject.expected_identity)
        }
        Layer::Downloaded => {
            subject.downloaded_identity.as_deref() == Some(&subject.expected_identity)
        }
    })
}

pub(super) fn validate_new_candidate(input: &NewReleaseCandidate) -> Result<()> {
    require_nonempty(&input.version, "release version")?;
    require_nonempty(&input.reviewed_commit, "reviewed commit")?;
    require_nonempty(&input.idempotency_key, "release idempotency key")?;
    let mut actual = BTreeSet::new();
    for subject in &input.subjects {
        if !matches!(
            subject.kind.as_str(),
            "local" | "source" | "release" | "asset"
        ) {
            bail!("unknown release subject kind: {}", subject.kind);
        }
        require_nonempty(&subject.name, "release subject name")?;
        require_nonempty(&subject.expected_identity, "expected release identity")?;
        if !actual.insert((subject.kind.as_str(), subject.name.as_str())) {
            bail!(
                "duplicate release subject: {}:{}",
                subject.kind,
                subject.name
            );
        }
    }
    let missing = REQUIRED_SUBJECTS
        .iter()
        .filter(|required| !actual.contains(required))
        .map(|(kind, name)| format!("{kind}:{name}"))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        bail!(
            "release manifest is incomplete; missing: {}",
            missing.join(", ")
        );
    }
    let has_binary_asset = input
        .subjects
        .iter()
        .any(|subject| subject.kind == "asset" && subject.name.ends_with("-linux-x86_64.tar.gz"));
    let has_checksums_asset = input
        .subjects
        .iter()
        .any(|subject| subject.kind == "asset" && subject.name.ends_with("-checksums.txt"));
    if !has_binary_asset || !has_checksums_asset {
        bail!("release manifest requires a binary archive and checksum asset");
    }
    let names = input
        .subjects
        .iter()
        .map(|subject| subject.name.as_str())
        .collect::<BTreeSet<_>>();
    if names.len() != input.subjects.len() {
        bail!("release subject names must be unique across the candidate");
    }
    Ok(())
}

pub(crate) fn revalidate_release_work_boundary(
    conn: &Connection,
    project_id: i64,
    root: &Path,
    candidate_id: i64,
) -> Result<i64> {
    let stored = conn
        .query_row(
            r#"
            select boundary.work_unit_id,boundary.activation_id,boundary.design_version_id,
                   boundary.repository_snapshot_id,boundary.boundary_identity,
                   candidate.reviewed_commit
            from release_candidates candidate
            join release_candidate_boundaries boundary
              on boundary.release_candidate_id=candidate.id
            where candidate.id=?1 and candidate.project_id=?2
            "#,
            params![candidate_id, project_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )
        .optional()?
        .with_context(|| {
            "release candidate has no close-ready work boundary; assemble a new candidate with --work"
        })?;
    let current = crate::work::resolve_release_work_boundary(
        conn,
        project_id,
        root,
        Some(stored.0),
        &stored.5,
    )?;
    if (
        current.activation_id,
        current.design_version_id,
        current.repository_snapshot_id,
        current.identity.as_str(),
    ) != (stored.1, stored.2, stored.3, stored.4.as_str())
    {
        bail!(
            "release work boundary changed for work unit {}; next: agent-workbench gate close-ready {} --dry-run",
            stored.0,
            stored.0
        );
    }
    Ok(stored.0)
}

pub(super) fn observation_map(values: &[ReleaseObservation]) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for value in values {
        require_nonempty(&value.name, "release observation name")?;
        require_nonempty(&value.identity, "release observation identity")?;
        if result
            .insert(value.name.clone(), value.identity.clone())
            .is_some()
        {
            bail!("duplicate release observation: {}", value.name);
        }
    }
    Ok(result)
}

pub(super) fn require_state(current: &CurrentRelease, expected: &str) -> Result<()> {
    if current.state != expected {
        bail!(
            "release candidate is {}, expected {}; next: {}",
            current.state,
            expected,
            next_action(&current.candidate_handle, current)
        );
    }
    Ok(())
}

pub(super) fn find_applied_request(
    conn: &Connection,
    project_id: i64,
    candidate_id: i64,
    request: &str,
) -> Result<Option<CurrentRelease>> {
    let handle = conn
        .query_row(
            "select revision_handle from release_candidate_revisions where project_id=?1 and release_candidate_id=?2 and request_identity=?3",
            params![project_id, candidate_id, request],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(handle) = handle else {
        return Ok(None);
    };
    let mut applied = load_revision(conn, project_id, candidate_id, &handle)?;
    applied.subjects = load_subjects(conn, applied.revision_id)?;
    Ok(Some(applied))
}

pub(super) fn load_revision(
    conn: &Connection,
    project_id: i64,
    candidate_id: i64,
    revision_handle: &str,
) -> Result<CurrentRelease> {
    conn.query_row(
        r#"
        select candidate.candidate_handle,revision.id,revision.revision,revision.state,revision.stage,
               revision.action,revision.reason,boundary.work_unit_id
        from release_candidate_revisions revision
        join release_candidates candidate on candidate.id=revision.release_candidate_id
        left join release_candidate_boundaries boundary on boundary.release_candidate_id=candidate.id
        where revision.project_id=?1 and revision.release_candidate_id=?2 and revision.revision_handle=?3
        "#,
        params![project_id, candidate_id, revision_handle],
        |row| {
            Ok(CurrentRelease {
                candidate_id,
                candidate_handle: row.get(0)?,
                revision_id: row.get(1)?,
                revision_handle: revision_handle.to_string(),
                revision: row.get(2)?,
                state: row.get(3)?,
                stage: row.get(4)?,
                action: row.get(5)?,
                reason: row.get(6)?,
                work_unit_id: row.get(7)?,
                subjects: Vec::new(),
            })
        },
    )
    .map_err(Into::into)
}

pub(super) fn outcome(current: &CurrentRelease, already_applied: bool) -> ReleaseTransitionOutcome {
    ReleaseTransitionOutcome {
        candidate_handle: current.candidate_handle.clone(),
        work_unit_id: current.work_unit_id,
        current_revision: current.revision_handle.clone(),
        state: current.state.clone(),
        already_applied,
        next_action: next_action(&current.candidate_handle, current),
    }
}

pub(super) fn next_action(candidate_handle: &str, current: &CurrentRelease) -> String {
    let digest = current
        .revision_handle
        .strip_prefix("release_revision_")
        .unwrap_or(&current.revision_handle);
    let key = format!("release-step-{}", digest.get(..16).unwrap_or(digest));
    let prefix = |action: &str| {
        format!(
            "agent-workbench operator release {action} {candidate_handle} --expected-current {} --idempotency-key {key}",
            current.revision_handle,
        )
    };
    if current.action == "reconcile" && current.reason.as_deref() == Some("external step absent") {
        return prefix("retry");
    }
    if current.action == "reconcile"
        && current
            .reason
            .as_deref()
            .is_some_and(|reason| reason.starts_with("withdrawal notice conflicts"))
    {
        return prefix("reconcile");
    }
    match current.state.as_str() {
        "assembled" => format!(
            "agent-workbench operator release candidate inspect {candidate_handle} --expected-current {} --idempotency-key {key}",
            current.revision_handle
        ),
        "locally_verified" => prefix("publish-source"),
        "source_published" => prefix("publish-assets"),
        "assets_published" => prefix("verify-remote"),
        "source_conflict" | "asset_conflict" => prefix("reconcile"),
        "remotely_verified" | "withdrawn" | "superseded" => "agent-workbench status".to_string(),
        _ => "agent-workbench status".to_string(),
    }
}

pub(super) fn resolved_next_action(conn: &Connection, current: &CurrentRelease) -> Result<String> {
    Ok(match pending_attempt(conn, current.candidate_id)? {
        Some((action, key, _)) => interrupted_next_action(current, &action, &key),
        None => next_action(&current.candidate_handle, current),
    })
}

pub(super) fn pending_attempt(
    conn: &Connection,
    candidate_id: i64,
) -> Result<Option<(String, String, String)>> {
    conn.query_row(
        "select action,idempotency_key,requested_identity from release_candidate_attempts where release_candidate_id=?1 and status='requested' order by id desc limit 1",
        [candidate_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )
    .optional()
    .map_err(Into::into)
}

pub(super) fn withdrawal_requested_identity(
    conn: &Connection,
    candidate_id: i64,
    include_completed: bool,
) -> Result<Option<String>> {
    conn.query_row(
        r#"
        select requested_identity
        from release_candidate_attempts
        where release_candidate_id=?1 and action='withdraw'
          and (status='requested' or (status='completed' and ?2=1))
        order by case status when 'requested' then 0 else 1 end,id desc
        limit 1
        "#,
        params![candidate_id, include_completed],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

pub(super) fn interrupted_next_action(current: &CurrentRelease, action: &str, key: &str) -> String {
    let exact = |command: &str| {
        format!(
            "agent-workbench operator release {command} {} --expected-current {} --idempotency-key {key}",
            current.candidate_handle, current.revision_handle
        )
    };
    match action {
        "inspect" => exact("candidate inspect"),
        "publish-source" | "publish-assets" => format!(
            "agent-workbench operator release reconcile {} --expected-current {} --idempotency-key {key}-reconcile",
            current.candidate_handle, current.revision_handle
        ),
        "verify-remote" | "reconcile" | "retry" => exact(action),
        "withdraw" => {
            if current.action == "reconcile"
                && current.reason.as_deref() == Some("external step absent")
            {
                format!(
                    "agent-workbench operator release retry {} --expected-current {} --idempotency-key {key}-retry",
                    current.candidate_handle, current.revision_handle
                )
            } else {
                format!(
                    "agent-workbench operator release reconcile {} --expected-current {} --idempotency-key {key}-reconcile",
                    current.candidate_handle, current.revision_handle
                )
            }
        }
        _ => "agent-workbench status".to_string(),
    }
}

pub(crate) fn withdrawal_state(state: &str) -> bool {
    matches!(
        state,
        "source_published" | "assets_published" | "asset_conflict"
    )
}

pub(super) fn manifest_identity(
    version: &str,
    commit: &str,
    subjects: &[ReleaseSubjectInput],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/release-manifest/v1\0");
    update_part(&mut hasher, version.as_bytes());
    update_part(&mut hasher, commit.as_bytes());
    for subject in subjects {
        update_part(&mut hasher, subject.kind.as_bytes());
        update_part(&mut hasher, subject.name.as_bytes());
        update_part(&mut hasher, subject.expected_identity.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

pub(super) fn request_identity(action: &str, expected_current: &str, payload: &[&str]) -> String {
    digest_handle(
        "request_",
        b"agent-workbench/release-request/v1\0",
        &std::iter::once(action.as_bytes())
            .chain(std::iter::once(expected_current.as_bytes()))
            .chain(payload.iter().map(|value| value.as_bytes()))
            .collect::<Vec<_>>(),
    )
}

pub(super) fn revision_handle(
    candidate: &str,
    revision: i64,
    state: &str,
    request: &str,
) -> String {
    let revision = revision.to_string();
    digest_handle(
        "release_revision_",
        b"agent-workbench/release-revision/v1\0",
        &[
            candidate.as_bytes(),
            revision.as_bytes(),
            state.as_bytes(),
            request.as_bytes(),
        ],
    )
}

pub(super) fn digest_handle(prefix: &str, domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        update_part(&mut hasher, part);
    }
    format!("{prefix}{:x}", hasher.finalize())
}

pub(super) fn update_part(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

pub(super) fn stage_for_state(state: &str) -> &'static str {
    match state {
        "assembled" => "local",
        "locally_verified" | "source_conflict" => "source",
        "source_published" => "assets",
        "assets_published" => "remote",
        "asset_conflict" => "assets",
        _ => "terminal",
    }
}

pub(super) fn is_terminal(state: &str) -> bool {
    matches!(state, "remotely_verified" | "withdrawn" | "superseded")
}

pub(super) fn require_nonempty(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{label} is required");
    }
    Ok(())
}

pub(super) fn display_names(names: &[String]) -> String {
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    }
}

pub(super) fn stale_revision(_candidate_handle: &str, current_revision: &str) -> Result<()> {
    bail!(
        "release candidate changed; current revision is {current_revision}; next: agent-workbench status"
    )
}
