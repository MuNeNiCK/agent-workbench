use super::*;
use std::process::{Child, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn spawn_aw(root: &Path, args: &[&str], envs: &[(&str, &str)]) -> Child {
    let mut command = Command::new(binary_under_test());
    command
        .arg("--root")
        .arg(root)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        command.env(key, value);
    }
    command.spawn().unwrap()
}

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn call_count(path: &Path) -> usize {
    fs::read_to_string(path).unwrap_or_default().lines().count()
}

fn assert_same_exact_result(first: &Output, second: &Output) {
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(
        second.status.success(),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    let first = String::from_utf8(first.stdout.clone()).unwrap();
    let second = String::from_utf8(second.stdout.clone()).unwrap();
    assert_eq!(field(&first, "candidate"), field(&second, "candidate"));
    assert_eq!(
        field(&first, "current_revision"),
        field(&second, "current_revision")
    );
    assert_ne!(
        field(&first, "already_applied"),
        field(&second, "already_applied")
    );
}

#[test]
fn identical_concurrent_release_requests_converge_without_repeating_effects() {
    let inspect_root = tempfile::tempdir().unwrap();
    let commit = release_source(inspect_root.path());
    let inspect_work = init_release_project(inspect_root.path(), &commit);
    let assembled = ok(
        inspect_root.path(),
        &[
            "operator",
            "release",
            "candidate",
            "assemble",
            "--work",
            &inspect_work.work_unit_id,
            "--version",
            "0.2.0",
            "--commit",
            &commit,
            "--expected-current",
            "absent",
            "--idempotency-key",
            "assemble-concurrent-inspection",
        ],
    );
    let candidate = field(&assembled, "candidate").to_string();
    let revision = field(&assembled, "current_revision").to_string();
    let inspect_barrier = inspect_root.path().join(".inspect-barrier");
    let inspect_barrier_value = inspect_barrier.to_string_lossy().into_owned();
    let inspect_args = [
        "operator",
        "release",
        "candidate",
        "inspect",
        &candidate,
        "--expected-current",
        &revision,
        "--idempotency-key",
        "same-inspection",
    ];
    let first = spawn_aw(
        inspect_root.path(),
        &inspect_args,
        &[("FAKE_INSPECT_BARRIER_DIR", &inspect_barrier_value)],
    );
    wait_for(&inspect_barrier.join("started"));
    let second = spawn_aw(
        inspect_root.path(),
        &inspect_args,
        &[("FAKE_INSPECT_BARRIER_DIR", &inspect_barrier_value)],
    );
    thread::sleep(Duration::from_millis(150));
    assert_eq!(call_count(&inspect_barrier.join("calls")), 1);
    fs::write(inspect_barrier.join("release"), "continue").unwrap();
    let first = first.wait_with_output().unwrap();
    let second = second.wait_with_output().unwrap();
    assert_same_exact_result(&first, &second);
    assert_eq!(call_count(&inspect_barrier.join("calls")), 1);
    let completed = String::from_utf8(first.stdout.clone()).unwrap();
    let changed_binding = aw(
        inspect_root.path(),
        &[
            "operator",
            "release",
            "candidate",
            "inspect",
            &candidate,
            "--expected-current",
            field(&completed, "current_revision"),
            "--idempotency-key",
            "same-inspection",
        ],
    );
    assert!(!changed_binding.status.success());
    assert!(!changed_binding.stderr.is_empty());

    let publish_root = tempfile::tempdir().unwrap();
    let commit = release_source(publish_root.path());
    let (path, gh_state) = fake_gh(publish_root.path());
    let publish_work = init_release_project(publish_root.path(), &commit);
    let assembled = ok(
        publish_root.path(),
        &[
            "operator",
            "release",
            "candidate",
            "assemble",
            "--work",
            &publish_work.work_unit_id,
            "--version",
            "0.2.0",
            "--commit",
            &commit,
            "--expected-current",
            "absent",
            "--idempotency-key",
            "assemble-concurrent-publish",
        ],
    );
    let candidate = field(&assembled, "candidate").to_string();
    let assembled_revision = field(&assembled, "current_revision").to_string();
    let inspected = ok(
        publish_root.path(),
        &[
            "operator",
            "release",
            "candidate",
            "inspect",
            &candidate,
            "--expected-current",
            &assembled_revision,
            "--idempotency-key",
            "inspect-before-concurrent-publish",
        ],
    );
    let inspected_revision = field(&inspected, "current_revision").to_string();
    let source = ok_env(
        publish_root.path(),
        &[
            "operator",
            "release",
            "publish-source",
            &candidate,
            "--expected-current",
            &inspected_revision,
            "--idempotency-key",
            "publish-source-before-concurrent-assets",
        ],
        &[
            ("PATH", path.as_str()),
            ("FAKE_GH_STATE", gh_state.as_str()),
        ],
    );
    assert!(source.contains("state: source_published"));
    let revision = field(&source, "current_revision").to_string();
    let publish_barrier = publish_root.path().join(".publish-barrier");
    let publish_barrier_value = publish_barrier.to_string_lossy().into_owned();
    let publish_args = [
        "operator",
        "release",
        "publish-assets",
        &candidate,
        "--expected-current",
        &revision,
        "--idempotency-key",
        "same-asset-publication",
    ];
    let envs = [
        ("PATH", path.as_str()),
        ("FAKE_GH_STATE", gh_state.as_str()),
        ("FAKE_GH_CREATE_BARRIER", publish_barrier_value.as_str()),
    ];
    let first = spawn_aw(publish_root.path(), &publish_args, &envs);
    wait_for(&publish_barrier.join("started"));
    let second = spawn_aw(publish_root.path(), &publish_args, &envs);
    thread::sleep(Duration::from_millis(150));
    assert_eq!(call_count(&publish_barrier.join("calls")), 1);
    fs::write(publish_barrier.join("release"), "continue").unwrap();
    let first = first.wait_with_output().unwrap();
    let second = second.wait_with_output().unwrap();
    assert_same_exact_result(&first, &second);
    assert_eq!(call_count(&publish_barrier.join("calls")), 1);
    assert!(Path::new(&gh_state).join("release.json").exists());
}

#[test]
fn identical_concurrent_assembly_preserves_the_shared_staging_directory() {
    let root = tempfile::tempdir().unwrap();
    let commit = release_source(root.path());
    let work = init_release_project(root.path(), &commit);
    let barrier_root = tempfile::tempdir().unwrap();
    let barrier = barrier_root.path().join("assembly");
    let barrier_value = barrier.to_string_lossy().into_owned();
    let args = [
        "operator",
        "release",
        "candidate",
        "assemble",
        "--work",
        &work.work_unit_id,
        "--version",
        "0.2.0",
        "--commit",
        &commit,
        "--expected-current",
        "absent",
        "--idempotency-key",
        "same-concurrent-assembly",
    ];
    let envs = [("FAKE_ASSEMBLY_BARRIER_DIR", barrier_value.as_str())];
    let first = spawn_aw(root.path(), &args, &envs);
    wait_for(&barrier.join("started"));
    let second = spawn_aw(root.path(), &args, &envs);
    thread::sleep(Duration::from_millis(150));
    assert_eq!(call_count(&barrier.join("calls")), 1);
    fs::write(barrier.join("release"), "continue").unwrap();

    let first = first.wait_with_output().unwrap();
    let second = second.wait_with_output().unwrap();
    assert_same_exact_result(&first, &second);
    let output = String::from_utf8(first.stdout).unwrap();
    let candidate = field(&output, "candidate");
    assert!(
        root.path()
            .join(".agent-workbench/release-candidates")
            .join(candidate)
            .is_dir()
    );
}
