use super::super::*;

struct StagingCleanup(PathBuf);

impl Drop for StagingCleanup {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub fn operator_assemble_release(
    root: &Path,
    input: OperatorReleaseAssemble,
) -> Result<ReleaseTransitionOutcome> {
    if input.expected_current != "absent" {
        bail!("release assembly requires --expected-current absent");
    }
    require_key(&input.idempotency_key)?;
    let tag = normalized_tag(&input.version)?;
    let commit = git_stdout(root, &["rev-parse", "HEAD"])?;
    if commit != input.reviewed_commit {
        bail!("reviewed commit is not the checked-out release source");
    }
    if !git_stdout(root, &["status", "--porcelain"])?.is_empty() {
        bail!("release assembly requires a clean reviewed source checkout");
    }
    require_package_version(root, &tag)?;
    crate::work::resolve_release_work_boundary_for_root(
        root,
        input.work_unit_id,
        &input.reviewed_commit,
    )?;

    let staging = staging_dir(root, &input.idempotency_key);
    signal_assembly_contender_for_test(&staging)?;
    let staging_identity = staging
        .file_name()
        .and_then(|name| name.to_str())
        .context("release staging identity is unavailable")?;
    let _assembly_guard = acquire_assembly_guard(root, staging_identity)?;
    if staging.exists() {
        fs::remove_dir_all(&staging).with_context(|| {
            format!(
                "failed to replace incomplete release assembly {}",
                staging.display()
            )
        })?;
    }
    fs::create_dir_all(&staging)?;
    let _staging_cleanup = StagingCleanup(staging.clone());
    run(
        Command::new(root.join("scripts/build-release-assets.sh"))
            .current_dir(root)
            .arg(&tag)
            .arg(&staging),
        "release asset assembly",
    )?;
    let subjects = assembled_subjects(root, &staging, &tag, &commit)?;
    let outcome = assemble_release_candidate(
        root,
        NewReleaseCandidate {
            work_unit_id: input.work_unit_id,
            version: tag,
            reviewed_commit: commit,
            idempotency_key: input.idempotency_key,
            subjects,
        },
    )?;
    let candidate_dir = candidate_dir(root, &outcome.candidate_handle);
    if candidate_dir.exists() {
        ensure_equal_directories(&staging, &candidate_dir)?;
        fs::remove_dir_all(&staging)?;
    } else {
        fs::rename(&staging, &candidate_dir)?;
    }
    Ok(outcome)
}

fn signal_assembly_contender_for_test(staging: &Path) -> Result<()> {
    let Some(directory) = std::env::var_os("AGENT_WORKBENCH_TEST_ASSEMBLY_CONTENDER_DIR") else {
        return Ok(());
    };
    let directory = PathBuf::from(directory);
    fs::create_dir_all(&directory)?;
    let identity = staging
        .file_name()
        .and_then(|name| name.to_str())
        .context("release staging identity is unavailable")?;
    fs::write(
        directory.join(format!("{}-{identity}.ready", std::process::id())),
        [],
    )?;
    Ok(())
}
