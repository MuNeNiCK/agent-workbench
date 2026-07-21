use super::super::*;

pub fn operator_publish_release_source(
    root: &Path,
    input: OperatorReleaseMutation,
) -> Result<ReleaseTransitionOutcome> {
    operator_publish_release_source_with_action(root, input, "publish-source")
}

pub(in crate::release_operator) fn operator_publish_release_source_with_action(
    root: &Path,
    input: OperatorReleaseMutation,
    action: &str,
) -> Result<ReleaseTransitionOutcome> {
    require_key(&input.idempotency_key)?;
    let inspection = inspect_release_candidate(root, &input.candidate)?;
    if inspection.current_revision == input.expected_current {
        ensure_current(&inspection, &input.expected_current, "locally_verified")?;
    }
    let tag = inspection.version.clone();
    let commit = inspection.reviewed_commit.clone();
    let expected = subject(&inspection, "source", "tag")?
        .expected_identity
        .clone();
    let attempt = start_attempt(root, &input, action, &expected)?;
    let Attempt::Ready { id, resumed, guard } = attempt else {
        return attempt.completed();
    };
    let _attempt_guard = guard;
    if inspection.current_revision != input.expected_current {
        let observed = remote_tag_commit(root, &tag)?.unwrap_or_else(|| "absent".to_string());
        let outcome = inspection_outcome(&inspection, true);
        finish_release_attempt(root, &input.candidate, id, &observed, &outcome, false)?;
        return Ok(outcome);
    }
    ensure_current(&inspection, &input.expected_current, "locally_verified")?;
    let observed_before = remote_tag_commit(root, &tag)?;
    if let Some(observed) = observed_before {
        let outcome = publish_release_source_with_action(
            root,
            &input.candidate,
            &input.expected_current,
            &input.idempotency_key,
            action,
            vec![observation("tag", &expected)],
            vec![observation("tag", &observed)],
        )?;
        finish_release_attempt(root, &input.candidate, id, &observed, &outcome, false)?;
        return Ok(outcome);
    }
    if resumed {
        let outcome = record_release_absent(
            root,
            &input.candidate,
            &input.expected_current,
            &format!("{}-reconcile", input.idempotency_key),
            "locally_verified",
        )?;
        finish_release_attempt(root, &input.candidate, id, "absent", &outcome, false)?;
        return Ok(outcome);
    }
    ensure_local_annotated_tag(root, &tag, &commit)?;
    run(
        Command::new("git")
            .current_dir(root)
            .args(["push", "origin"])
            .arg(format!("refs/tags/{tag}:refs/tags/{tag}")),
        "create-only source tag publication",
    )?;
    let observed = remote_tag_commit(root, &tag)?
        .context("source tag publication returned without an observable remote tag")?;
    let outcome = publish_release_source_with_action(
        root,
        &input.candidate,
        &input.expected_current,
        &input.idempotency_key,
        action,
        vec![observation("tag", &expected)],
        vec![observation("tag", &observed)],
    )?;
    finish_release_attempt(root, &input.candidate, id, &observed, &outcome, false)?;
    Ok(outcome)
}

pub fn operator_publish_release_assets(
    root: &Path,
    input: OperatorReleaseMutation,
) -> Result<ReleaseTransitionOutcome> {
    operator_publish_release_assets_with_action(root, input, "publish-assets")
}

pub(in crate::release_operator) fn operator_publish_release_assets_with_action(
    root: &Path,
    input: OperatorReleaseMutation,
    action: &str,
) -> Result<ReleaseTransitionOutcome> {
    require_key(&input.idempotency_key)?;
    let inspection = inspect_release_candidate(root, &input.candidate)?;
    if inspection.current_revision == input.expected_current {
        ensure_current(&inspection, &input.expected_current, "source_published")?;
    }
    let directory = candidate_dir(root, &input.candidate);
    let expected = expected_remote_observations(&inspection);
    let requested_identity = observations_identity(&expected);
    let attempt = start_attempt(root, &input, action, &requested_identity)?;
    let Attempt::Ready { id, resumed, guard } = attempt else {
        return attempt.completed();
    };
    let _attempt_guard = guard;
    if inspection.current_revision != input.expected_current {
        let outcome = inspection_outcome(&inspection, true);
        finish_release_attempt(
            root,
            &input.candidate,
            id,
            &requested_identity,
            &outcome,
            false,
        )?;
        return Ok(outcome);
    }
    ensure_current(&inspection, &input.expected_current, "source_published")?;
    let remote = probe_remote_release(root, &inspection.version)?;
    if let Some(remote) = remote.as_ref() {
        let observed = remote_observations(remote, &inspection);
        if observations_match(&expected, &observed) {
            let observed_identity = observations_identity(&observed);
            let outcome = publish_release_assets_with_action(
                root,
                &input.candidate,
                &input.expected_current,
                &input.idempotency_key,
                action,
                expected,
                observed,
            )?;
            finish_release_attempt(
                root,
                &input.candidate,
                id,
                &observed_identity,
                &outcome,
                false,
            )?;
            return Ok(outcome);
        }
        let expected_names = asset_names(&inspection);
        let remote_names = remote
            .assets
            .iter()
            .map(|asset| asset.name.as_str())
            .collect::<BTreeSet<_>>();
        if !remote_names.is_subset(&expected_names) {
            let observed = conflict_observations(&inspection, remote);
            let observed_identity = observations_identity(&observed);
            let outcome = publish_release_assets_with_action(
                root,
                &input.candidate,
                &input.expected_current,
                &input.idempotency_key,
                action,
                expected,
                observed,
            )?;
            finish_release_attempt(
                root,
                &input.candidate,
                id,
                &observed_identity,
                &outcome,
                false,
            )?;
            return Ok(outcome);
        }
        for asset in inspection.subjects.iter().filter(|subject| {
            subject.kind == "asset" && !remote_names.contains(subject.name.as_str())
        }) {
            run(
                Command::new("gh")
                    .current_dir(root)
                    .args(["release", "upload", &inspection.version])
                    .arg(directory.join(&asset.name)),
                "create-only release asset publication",
            )?;
        }
    } else if resumed {
        let outcome = record_release_absent(
            root,
            &input.candidate,
            &input.expected_current,
            &format!("{}-reconcile", input.idempotency_key),
            "source_published",
        )?;
        finish_release_attempt(root, &input.candidate, id, "absent", &outcome, false)?;
        return Ok(outcome);
    } else {
        let mut command = Command::new("gh");
        command
            .current_dir(root)
            .args(["release", "create", &inspection.version, "--verify-tag"])
            .args(["--title", &inspection.version])
            .arg("--notes-file")
            .arg(root.join("CHANGELOG.md"));
        for subject in inspection
            .subjects
            .iter()
            .filter(|subject| subject.kind == "asset")
        {
            command.arg(directory.join(&subject.name));
        }
        run(&mut command, "release and asset publication")?;
    }
    let remote = probe_remote_release(root, &inspection.version)?
        .context("asset publication returned without an observable remote release")?;
    let observed = remote_observations(&remote, &inspection);
    let observed_identity = observations_identity(&observed);
    let outcome = publish_release_assets_with_action(
        root,
        &input.candidate,
        &input.expected_current,
        &input.idempotency_key,
        action,
        expected,
        observed,
    )?;
    finish_release_attempt(
        root,
        &input.candidate,
        id,
        &observed_identity,
        &outcome,
        false,
    )?;
    Ok(outcome)
}

pub fn operator_verify_release_remote(
    root: &Path,
    input: OperatorReleaseMutation,
) -> Result<ReleaseTransitionOutcome> {
    operator_verify_release_remote_with_action(root, input, "verify-remote")
}

pub(in crate::release_operator) fn operator_verify_release_remote_with_action(
    root: &Path,
    input: OperatorReleaseMutation,
    action: &str,
) -> Result<ReleaseTransitionOutcome> {
    require_key(&input.idempotency_key)?;
    let inspection = inspect_release_candidate(root, &input.candidate)?;
    if inspection.current_revision == input.expected_current {
        ensure_current(&inspection, &input.expected_current, "assets_published")?;
    }
    let requested = expected_identity(&inspection, &["asset"]);
    let attempt = start_attempt(root, &input, action, &requested)?;
    let Attempt::Ready { id, guard, .. } = attempt else {
        return attempt.completed();
    };
    let _attempt_guard = guard;
    if inspection.current_revision != input.expected_current {
        let outcome = inspection_outcome(&inspection, true);
        finish_release_attempt(root, &input.candidate, id, &requested, &outcome, false)?;
        return Ok(outcome);
    }
    ensure_current(&inspection, &input.expected_current, "assets_published")?;
    let observations = download_remote_observations(root, &inspection, &input.expected_current)?;
    let observed_identity = observations_identity(&observations);
    let outcome = verify_release_remotely_with_action(
        root,
        &input.candidate,
        &input.expected_current,
        &input.idempotency_key,
        action,
        observations,
    )?;
    finish_release_attempt(
        root,
        &input.candidate,
        id,
        &observed_identity,
        &outcome,
        false,
    )?;
    Ok(outcome)
}

pub(in crate::release_operator) fn download_remote_observations(
    root: &Path,
    inspection: &ReleaseInspection,
    operation_identity: &str,
) -> Result<Vec<ReleaseObservation>> {
    let downloads = candidate_dir(root, &inspection.candidate_handle)
        .join(format!("download-{}", short_identity(operation_identity)));
    if downloads.exists() {
        fs::remove_dir_all(&downloads)?;
    }
    fs::create_dir_all(&downloads)?;
    let result = (|| -> Result<Vec<ReleaseObservation>> {
        run(
            Command::new("gh")
                .current_dir(root)
                .args(["release", "download", &inspection.version, "--dir"])
                .arg(&downloads),
            "remote release download",
        )?;
        let actual = directory_identities(&downloads)?;
        let expected_names = asset_names(inspection);
        if actual.keys().map(String::as_str).collect::<BTreeSet<_>>() != expected_names {
            let marker = digest_parts(
                b"agent-workbench/remote-asset-set-conflict/v1\0",
                &actual
                    .iter()
                    .flat_map(|(name, identity)| [name.as_bytes(), identity.as_bytes()])
                    .collect::<Vec<_>>(),
            );
            return Ok(inspection
                .subjects
                .iter()
                .filter(|subject| subject.kind == "asset")
                .map(|subject| observation(&subject.name, &marker))
                .collect());
        }
        Ok(actual
            .into_iter()
            .map(|(name, identity)| observation(&name, &identity))
            .collect())
    })();
    let cleanup = fs::remove_dir_all(&downloads);
    let observations = result?;
    cleanup?;
    Ok(observations)
}
