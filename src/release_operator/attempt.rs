use super::*;

pub(super) enum Attempt {
    Ready {
        id: i64,
        resumed: bool,
        guard: AttemptGuard,
    },
    Completed(ReleaseTransitionOutcome),
}

pub(super) struct AttemptGuard {
    _file: File,
}

pub(super) fn acquire_assembly_guard(root: &Path, staging_identity: &str) -> Result<AttemptGuard> {
    acquire_guard(root, format!("assembly-{staging_identity}"))
}

impl Attempt {
    pub(super) fn completed(self) -> Result<ReleaseTransitionOutcome> {
        match self {
            Self::Completed(outcome) => Ok(outcome),
            Self::Ready { .. } => bail!("release attempt unexpectedly remained ready"),
        }
    }
}

pub(super) fn start_attempt(
    root: &Path,
    input: &OperatorReleaseMutation,
    action: &str,
    requested_identity: &str,
) -> Result<Attempt> {
    let guard = acquire_attempt_guard(root, &input.candidate, &input.idempotency_key)?;
    Ok(
        match start_release_attempt(
            root,
            &input.candidate,
            &input.expected_current,
            action,
            &input.idempotency_key,
            requested_identity,
        )? {
            ReleaseAttemptStart::Ready {
                attempt_id,
                resumed,
            } => Attempt::Ready {
                id: attempt_id,
                resumed,
                guard,
            },
            ReleaseAttemptStart::Completed(outcome) => Attempt::Completed(outcome),
        },
    )
}

fn acquire_attempt_guard(
    root: &Path,
    candidate: &str,
    idempotency_key: &str,
) -> Result<AttemptGuard> {
    acquire_guard(
        root,
        digest_parts(
            b"agent-workbench/release-attempt-lock/v1\0",
            &[candidate.as_bytes(), idempotency_key.as_bytes()],
        ),
    )
}

fn acquire_guard(root: &Path, identity: String) -> Result<AttemptGuard> {
    let directory = root
        .join(crate::db::LEDGER_DIR)
        .join("release-attempt-locks");
    fs::create_dir_all(&directory)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join(format!("{identity}.lock")))?;
    file.lock_exclusive()?;
    Ok(AttemptGuard { _file: file })
}

pub(super) fn inspection_outcome(
    inspection: &ReleaseInspection,
    already_applied: bool,
) -> ReleaseTransitionOutcome {
    ReleaseTransitionOutcome {
        candidate_handle: inspection.candidate_handle.clone(),
        work_unit_id: inspection.work_unit_id,
        current_revision: inspection.current_revision.clone(),
        state: inspection.state.clone(),
        already_applied,
        next_action: inspection.next_action.clone(),
    }
}

pub(super) fn expected_identity(inspection: &ReleaseInspection, kinds: &[&str]) -> String {
    let observations = inspection
        .subjects
        .iter()
        .filter(|subject| kinds.contains(&subject.kind.as_str()))
        .map(|subject| observation(&subject.name, &subject.expected_identity))
        .collect::<Vec<_>>();
    observations_identity(&observations)
}

pub(super) fn observations_identity(observations: &[ReleaseObservation]) -> String {
    let mut observations = observations.to_vec();
    observations.sort_by(|left, right| left.name.cmp(&right.name));
    digest_parts(
        b"agent-workbench/release-observations/v1\0",
        &observations
            .iter()
            .flat_map(|observation| [observation.name.as_bytes(), observation.identity.as_bytes()])
            .collect::<Vec<_>>(),
    )
}

pub(super) fn assembled_subjects(
    root: &Path,
    directory: &Path,
    tag: &str,
    commit: &str,
) -> Result<Vec<ReleaseSubjectInput>> {
    let files = directory_identities(directory)?;
    let binary_output = command_stdout(
        Command::new(root.join("target/release/agent-workbench")).arg("--version"),
        "release binary version inspection",
    )?;
    let mut subjects = vec![
        release_subject("local", "package-version", tag),
        release_subject("local", "lockfile", &sha256_file(&root.join("Cargo.lock"))?),
        release_subject("local", "binary-version", &binary_output),
        release_subject(
            "local",
            "wrapper",
            &sha256_file(&root.join("skills/agent-workbench/scripts/agent-workbench.sh"))?,
        ),
        release_subject(
            "local",
            "skill",
            files
                .iter()
                .find(|(name, _)| name.ends_with("-skill.tar.gz"))
                .map(|(_, identity)| identity.as_str())
                .context("assembled release has no skill archive")?,
        ),
        release_subject("local", "license", &sha256_file(&root.join("LICENSE"))?),
        release_subject(
            "local",
            "source-archive",
            files
                .iter()
                .find(|(name, _)| name.ends_with("-source.tar.gz"))
                .map(|(_, identity)| identity.as_str())
                .context("assembled release has no source archive")?,
        ),
        release_subject(
            "local",
            "release-notes",
            &sha256_file(&root.join("CHANGELOG.md"))?,
        ),
        release_subject("source", "tag", &format!("annotated:{commit}")),
        release_subject(
            "release",
            "release",
            &release_identity(tag, &fs::read_to_string(root.join("CHANGELOG.md"))?),
        ),
    ];
    subjects.extend(
        files
            .into_iter()
            .map(|(name, identity)| release_subject("asset", &name, &identity)),
    );
    Ok(subjects)
}

pub(super) fn verify_local_candidate(
    root: &Path,
    directory: &Path,
    inspection: &ReleaseInspection,
) -> Result<Vec<ReleaseObservation>> {
    if !directory.is_dir() {
        bail!("release candidate assets are unavailable; assemble the same candidate again");
    }
    let files = directory_identities(directory)?;
    let expected_assets = inspection
        .subjects
        .iter()
        .filter(|subject| subject.kind == "asset")
        .map(|subject| (subject.name.clone(), subject.expected_identity.clone()))
        .collect::<BTreeMap<_, _>>();
    if files != expected_assets {
        bail!("local release assets differ from the assembled candidate");
    }
    let checksums = expected_assets
        .keys()
        .find(|name| name.ends_with("-checksums.txt"))
        .context("release candidate has no checksum asset")?;
    run(
        Command::new("sha256sum")
            .current_dir(directory)
            .args(["-c", checksums]),
        "local checksum verification",
    )?;
    require_package_version(root, &inspection.version)?;
    let actual = local_identity_map(root, directory, inspection)?;
    Ok(inspection
        .subjects
        .iter()
        .filter(|subject| matches!(subject.kind.as_str(), "local" | "asset"))
        .map(|subject| {
            observation(
                &subject.name,
                actual
                    .get(&subject.name)
                    .map(String::as_str)
                    .unwrap_or(&subject.expected_identity),
            )
        })
        .collect())
}

pub(super) fn local_identity_map(
    root: &Path,
    directory: &Path,
    inspection: &ReleaseInspection,
) -> Result<BTreeMap<String, String>> {
    let mut values = BTreeMap::new();
    values.insert("package-version".to_string(), inspection.version.clone());
    values.insert(
        "lockfile".to_string(),
        sha256_file(&root.join("Cargo.lock"))?,
    );
    values.insert(
        "binary-version".to_string(),
        command_stdout(
            Command::new(root.join("target/release/agent-workbench")).arg("--version"),
            "release binary version inspection",
        )?,
    );
    values.insert(
        "wrapper".to_string(),
        sha256_file(&root.join("skills/agent-workbench/scripts/agent-workbench.sh"))?,
    );
    values.insert("license".to_string(), sha256_file(&root.join("LICENSE"))?);
    values.insert(
        "release-notes".to_string(),
        sha256_file(&root.join("CHANGELOG.md"))?,
    );
    for (name, identity) in directory_identities(directory)? {
        if name.ends_with("-skill.tar.gz") {
            values.insert("skill".to_string(), identity.clone());
        }
        if name.ends_with("-source.tar.gz") {
            values.insert("source-archive".to_string(), identity.clone());
        }
        values.insert(name, identity);
    }
    Ok(values)
}

pub(super) fn expected_remote_observations(
    inspection: &ReleaseInspection,
) -> Vec<ReleaseObservation> {
    inspection
        .subjects
        .iter()
        .filter(|subject| matches!(subject.kind.as_str(), "release" | "asset"))
        .map(|subject| observation(&subject.name, &subject.expected_identity))
        .collect()
}

pub(super) fn remote_observations(
    remote: &RemoteRelease,
    inspection: &ReleaseInspection,
) -> Vec<ReleaseObservation> {
    let assets = remote
        .assets
        .iter()
        .map(|asset| {
            (
                asset.name.as_str(),
                asset
                    .digest
                    .as_deref()
                    .and_then(|digest| digest.strip_prefix("sha256:"))
                    .unwrap_or("unverified"),
            )
        })
        .collect::<BTreeMap<_, _>>();
    inspection
        .subjects
        .iter()
        .filter(|subject| matches!(subject.kind.as_str(), "release" | "asset"))
        .map(|subject| {
            if subject.kind == "release" {
                observation(
                    &subject.name,
                    &release_identity(&remote.tag_name, &remote.body),
                )
            } else {
                observation(
                    &subject.name,
                    assets
                        .get(subject.name.as_str())
                        .copied()
                        .unwrap_or("absent"),
                )
            }
        })
        .collect()
}

pub(super) fn conflict_observations(
    inspection: &ReleaseInspection,
    remote: &RemoteRelease,
) -> Vec<ReleaseObservation> {
    let identity = digest_parts(
        b"agent-workbench/remote-release-conflict/v1\0",
        &remote
            .assets
            .iter()
            .flat_map(|asset| {
                [
                    asset.name.as_bytes(),
                    asset.digest.as_deref().unwrap_or("").as_bytes(),
                ]
            })
            .collect::<Vec<_>>(),
    );
    inspection
        .subjects
        .iter()
        .filter(|subject| matches!(subject.kind.as_str(), "release" | "asset"))
        .map(|subject| observation(&subject.name, &identity))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn assembly_guard_serializes_one_staging_resource() {
        let root = tempfile::tempdir().unwrap();
        let first = acquire_assembly_guard(root.path(), "staging-resource").unwrap();
        let second_root = root.path().to_path_buf();
        let (acquired_tx, acquired_rx) = mpsc::channel();
        let second = thread::spawn(move || {
            let guard = acquire_assembly_guard(&second_root, "staging-resource").unwrap();
            acquired_tx.send(()).unwrap();
            guard
        });

        assert!(
            acquired_rx
                .recv_timeout(Duration::from_millis(200))
                .is_err()
        );
        drop(first);
        acquired_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        drop(second.join().unwrap());
    }
}
