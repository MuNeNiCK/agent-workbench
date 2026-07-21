use super::*;

mod assembly;
mod publication;
pub use assembly::operator_assemble_release;
pub(in crate::release_operator) use publication::{
    download_remote_observations, operator_publish_release_assets_with_action,
    operator_publish_release_source_with_action, operator_verify_release_remote_with_action,
};
pub use publication::{
    operator_publish_release_assets, operator_publish_release_source,
    operator_verify_release_remote,
};

pub(super) fn probe_remote_release(root: &Path, tag: &str) -> Result<Option<RemoteRelease>> {
    let output = Command::new("gh")
        .current_dir(root)
        .args([
            "release",
            "view",
            tag,
            "--json",
            "tagName,name,body,targetCommitish,assets",
        ])
        .output()
        .context("failed to start remote release probe")?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        if error.to_ascii_lowercase().contains("not found")
            || error
                .to_ascii_lowercase()
                .contains("release does not exist")
        {
            return Ok(None);
        }
        bail!("remote release probe failed: {}", error.trim());
    }
    serde_json::from_slice(&output.stdout)
        .context("remote release probe returned an unsupported response")
        .map(Some)
}

pub(super) fn remote_tag_commit(root: &Path, tag: &str) -> Result<Option<String>> {
    let direct = format!("refs/tags/{tag}");
    let peeled = format!("refs/tags/{tag}^{{}}");
    let output = Command::new("git")
        .current_dir(root)
        .args(["ls-remote", "origin", &direct, &peeled])
        .output()
        .context("failed to start remote tag probe")?;
    if !output.status.success() {
        bail!(
            "remote tag probe failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let text = String::from_utf8(output.stdout)?;
    let rows = text
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((fields.next()?.to_string(), fields.next()?.to_string()))
        })
        .collect::<Vec<_>>();
    if let Some((identity, _)) = rows.iter().find(|(_, reference)| reference == &peeled) {
        return Ok(Some(format!("annotated:{identity}")));
    }
    if let Some((identity, _)) = rows.iter().find(|(_, reference)| reference == &direct) {
        return Ok(Some(format!("lightweight:{identity}")));
    }
    Ok(None)
}

pub(super) fn ensure_local_annotated_tag(root: &Path, tag: &str, commit: &str) -> Result<()> {
    let reference = format!("refs/tags/{tag}^{{}}");
    let output = Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "--verify", &reference])
        .output()?;
    if output.status.success() {
        let observed = String::from_utf8(output.stdout)?.trim().to_string();
        if observed != commit {
            bail!("local release tag already names a different commit");
        }
        return Ok(());
    }
    run(
        Command::new("git")
            .current_dir(root)
            .args(["tag", "-a", tag, commit, "-m"])
            .arg(format!("Agent Workbench {tag}")),
        "annotated source tag creation",
    )?;
    Ok(())
}

pub(super) fn withdrawal_notice_path(root: &Path, inspection: &ReleaseInspection) -> PathBuf {
    candidate_dir(root, &inspection.candidate_handle).join("WITHDRAWN.txt")
}

pub(super) fn prepare_withdrawal_notice(
    root: &Path,
    inspection: &ReleaseInspection,
    reason: &str,
) -> Result<String> {
    let notice = withdrawal_notice_path(root, inspection);
    let bytes = format!("WITHDRAWN\n\n{reason}\n").into_bytes();
    if notice.exists() && fs::read(&notice)? != bytes {
        bail!("prepared withdrawal notice differs from the exact retry");
    }
    fs::write(&notice, &bytes)?;
    Ok(sha256_bytes(&bytes))
}

pub(super) fn publish_prepared_withdrawal_notice(
    root: &Path,
    inspection: &ReleaseInspection,
) -> Result<()> {
    let notice = withdrawal_notice_path(root, inspection);
    if !notice.is_file() {
        bail!("prepared withdrawal notice is unavailable; next: agent-workbench status");
    }
    let remote = probe_remote_release(root, &inspection.version)?;
    if remote.is_some() {
        run(
            Command::new("gh")
                .current_dir(root)
                .args(["release", "upload", &inspection.version])
                .arg(&notice),
            "non-destructive withdrawal notice publication",
        )?;
    } else {
        run(
            Command::new("gh")
                .current_dir(root)
                .args(["release", "create", &inspection.version, "--verify-tag"])
                .args(["--title", &format!("{} (WITHDRAWN)", inspection.version)])
                .args(["--notes", &withdrawal_reason(root, inspection)])
                .arg(&notice),
            "non-destructive withdrawn release publication",
        )?;
    }
    Ok(())
}

pub(super) fn probe_withdrawal_notice(
    root: &Path,
    inspection: &ReleaseInspection,
    expected_identity: &str,
) -> Result<WithdrawalNoticeObservation> {
    let Some(remote) = probe_remote_release(root, &inspection.version)? else {
        return Ok(WithdrawalNoticeObservation::Absent);
    };
    let notices = remote
        .assets
        .iter()
        .filter(|asset| asset.name == "WITHDRAWN.txt")
        .collect::<Vec<_>>();
    let [notice] = notices.as_slice() else {
        return Ok(if notices.is_empty() {
            WithdrawalNoticeObservation::Absent
        } else {
            WithdrawalNoticeObservation::Conflict("duplicate-withdrawal-notices".to_string())
        });
    };
    let observed = notice
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
        .unwrap_or("unverified")
        .to_string();
    Ok(if observed == expected_identity {
        WithdrawalNoticeObservation::Exact(observed)
    } else {
        WithdrawalNoticeObservation::Conflict(observed)
    })
}

pub(super) fn withdrawal_reason(root: &Path, inspection: &ReleaseInspection) -> String {
    fs::read_to_string(withdrawal_notice_path(root, inspection))
        .ok()
        .and_then(|notice| {
            notice
                .strip_prefix("WITHDRAWN\n\n")
                .map(|reason| reason.trim_end().to_string())
        })
        .filter(|reason| !reason.is_empty())
        .unwrap_or_else(|| "withdrawal notice observed".to_string())
}

pub(super) fn ensure_authority(
    root: &Path,
    authority_event_id: i64,
    candidate: &str,
) -> Result<()> {
    if authority_event_id <= 0 {
        bail!("an active project authority event is required");
    }
    let events = crate::list_authority_events(root, None)?;
    let expected_scope = format!("release:{candidate}");
    if !events.iter().any(|event| {
        event.id == authority_event_id
            && event.status == "active"
            && event.event_type == "user_instruction"
            && (matches!(event.scope.as_deref(), Some("project") | None)
                || event.scope.as_deref() == Some(&expected_scope))
    }) {
        bail!("active project authority event not found: {authority_event_id}");
    }
    Ok(())
}

pub(super) fn require_package_version(root: &Path, tag: &str) -> Result<()> {
    let manifest = fs::read_to_string(root.join("Cargo.toml"))?;
    let expected = tag.trim_start_matches('v');
    let actual = manifest
        .lines()
        .find_map(|line| {
            line.strip_prefix("version = \"")
                .and_then(|v| v.strip_suffix('"'))
        })
        .context("package version is unavailable")?;
    if actual != expected {
        bail!("package version does not match the requested release version");
    }
    let installed = fs::read_to_string(root.join("skills/agent-workbench/CLI_VERSION"))?;
    if installed.trim() != tag {
        bail!("installed skill version does not match the requested release version");
    }
    Ok(())
}

pub(super) fn normalized_tag(version: &str) -> Result<String> {
    let version = version.trim();
    let core = version.strip_prefix('v').unwrap_or(version);
    let parts = core.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        bail!("release version must be a three-part semantic version");
    }
    Ok(format!("v{core}"))
}

pub(super) fn directory_identities(directory: &Path) -> Result<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if !path.is_file() {
            bail!("release asset directory contains a non-file entry");
        }
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .context("release asset name is not valid UTF-8")?
            .to_string();
        result.insert(name, sha256_file(&path)?);
    }
    Ok(result)
}

pub(super) fn ensure_equal_directories(left: &Path, right: &Path) -> Result<()> {
    if directory_identities(left)? != directory_identities(right)? {
        bail!("existing release candidate assets differ from the exact retry");
    }
    Ok(())
}

pub(super) fn sha256_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

pub(super) fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub(super) fn release_identity(tag: &str, notes: &str) -> String {
    digest_parts(
        b"agent-workbench/remote-release/v1\0",
        &[tag.as_bytes(), notes.as_bytes()],
    )
}

pub(super) fn digest_parts(domain: &[u8], parts: &[&[u8]]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part);
    }
    format!("{:x}", hasher.finalize())
}

pub(super) fn release_subject(kind: &str, name: &str, identity: &str) -> ReleaseSubjectInput {
    ReleaseSubjectInput {
        kind: kind.to_string(),
        name: name.to_string(),
        expected_identity: identity.to_string(),
    }
}

pub(super) fn observation(name: &str, identity: &str) -> ReleaseObservation {
    ReleaseObservation {
        name: name.to_string(),
        identity: identity.to_string(),
    }
}

pub(super) fn subject<'a>(
    inspection: &'a ReleaseInspection,
    kind: &str,
    name: &str,
) -> Result<&'a crate::release::ReleaseSubjectRecord> {
    inspection
        .subjects
        .iter()
        .find(|subject| subject.kind == kind && subject.name == name)
        .with_context(|| format!("release candidate is missing {kind}:{name}"))
}

pub(super) fn asset_names(inspection: &ReleaseInspection) -> BTreeSet<&str> {
    inspection
        .subjects
        .iter()
        .filter(|subject| subject.kind == "asset")
        .map(|subject| subject.name.as_str())
        .collect()
}

pub(super) fn observations_match(
    left: &[ReleaseObservation],
    right: &[ReleaseObservation],
) -> bool {
    left == right
}

pub(super) fn ensure_current(
    inspection: &ReleaseInspection,
    expected_current: &str,
    expected_state: &str,
) -> Result<()> {
    ensure_current_revision(inspection, expected_current)?;
    if inspection.state != expected_state {
        bail!(
            "release candidate is {}, expected {expected_state}; next: {}",
            inspection.state,
            inspection.next_action
        );
    }
    Ok(())
}

pub(super) fn ensure_current_revision(
    inspection: &ReleaseInspection,
    expected_current: &str,
) -> Result<()> {
    if inspection.current_revision != expected_current {
        stale(inspection)?;
    }
    Ok(())
}

pub(super) fn stale(inspection: &ReleaseInspection) -> Result<()> {
    bail!(
        "release candidate changed; current revision is {}; next: {}",
        inspection.current_revision,
        inspection.next_action
    )
}

pub(super) fn require_key(key: &str) -> Result<()> {
    if key.trim().is_empty() {
        bail!("release idempotency key is required");
    }
    Ok(())
}

pub(super) fn candidate_dir(root: &Path, candidate: &str) -> PathBuf {
    root.join(crate::db::LEDGER_DIR)
        .join("release-candidates")
        .join(candidate)
}

pub(super) fn staging_dir(root: &Path, idempotency_key: &str) -> PathBuf {
    root.join(crate::db::LEDGER_DIR)
        .join("release-candidates")
        .join(format!(
            "staging-{}",
            short_identity(&digest_parts(
                b"agent-workbench/release-staging/v1\0",
                &[idempotency_key.as_bytes()]
            ))
        ))
}

pub(super) fn short_identity(identity: &str) -> &str {
    identity.get(..16).unwrap_or(identity)
}

pub(super) fn git_stdout(root: &Path, args: &[&str]) -> Result<String> {
    command_stdout(
        Command::new("git").current_dir(root).args(args),
        "Git inspection",
    )
}

pub(super) fn command_stdout(command: &mut Command, label: &str) -> Result<String> {
    let output = command
        .output()
        .with_context(|| format!("failed to start {label}"))?;
    require_success(output, label)
}

pub(super) fn run(command: &mut Command, label: &str) -> Result<()> {
    let output = command
        .output()
        .with_context(|| format!("failed to start {label}"))?;
    require_success(output, label).map(drop)
}

pub(super) fn require_success(output: Output, label: &str) -> Result<String> {
    if !output.status.success() {
        bail!(
            "{label} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RemoteRelease {
    pub(super) tag_name: String,
    #[allow(dead_code)]
    pub(super) name: String,
    pub(super) body: String,
    #[allow(dead_code)]
    pub(super) target_commitish: String,
    pub(super) assets: Vec<RemoteAsset>,
}

#[derive(Debug, Deserialize)]
pub(super) struct RemoteAsset {
    pub(super) name: String,
    #[allow(dead_code)]
    pub(super) size: u64,
    pub(super) digest: Option<String>,
}

pub(super) enum WithdrawalNoticeObservation {
    Absent,
    Exact(String),
    Conflict(String),
}
