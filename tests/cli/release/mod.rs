use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use super::*;

mod candidate;
mod errors;
mod idempotency_concurrency;
mod mutation_contract;
mod publication_absence;
mod publication_reconcile;
mod supersession;
mod supersession_concurrency;
mod withdrawal;
mod withdrawal_recovery;

fn field<'a>(output: &'a str, name: &str) -> &'a str {
    output
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{name}: ")))
        .unwrap()
}

fn owner_field<'a>(output: &'a str, owner: &str, name: &str) -> &'a str {
    let marker = format!("owner: {owner}");
    let prefix = format!("{name}: ");
    let mut selected = false;
    for line in output.lines() {
        if line.starts_with("owner: ") {
            selected = line == marker;
            continue;
        }
        if selected && let Some(value) = line.strip_prefix(&prefix) {
            return value;
        }
    }
    panic!("owner field {name} not found for {owner}");
}

fn execute_rendered(root: &Path, command: &str, envs: &[(&str, &str)]) -> Output {
    let arguments = command
        .strip_prefix("agent-workbench ")
        .expect("rendered action must use the public executable")
        .split_whitespace()
        .collect::<Vec<_>>();
    aw_env(root, &arguments, envs)
}

fn release_owner_outputs(root: &Path) -> (String, String) {
    (ok(root, &["status"]), ok(root, &["next"]))
}

fn write_executable(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}

fn release_source(root: &Path) -> String {
    fs::create_dir_all(root.join("skills/agent-workbench/scripts")).unwrap();
    fs::create_dir_all(root.join("target/release")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"agent-workbench\"\nversion = \"0.2.0\"\n",
    )
    .unwrap();
    fs::write(root.join("Cargo.lock"), "lock identity\n").unwrap();
    fs::write(root.join("LICENSE"), "license identity\n").unwrap();
    fs::write(root.join("CHANGELOG.md"), "# v0.2.0\n\nCandidate notes.\n").unwrap();
    fs::write(
        root.join(".gitignore"),
        ".agent-workbench/\ntarget/\n.remote.git/\n.fake-gh/\n.fake-bin/\n",
    )
    .unwrap();
    fs::write(root.join("skills/agent-workbench/CLI_VERSION"), "v0.2.0\n").unwrap();
    write_executable(
        &root.join("skills/agent-workbench/scripts/agent-workbench.sh"),
        "#!/bin/sh\nset -eu\nexec \"${AGENT_WORKBENCH_BIN:-agent-workbench}\" \"$@\"\n",
    );
    write_executable(
        &root.join("target/release/agent-workbench"),
        r#"#!/bin/sh
set -eu
if [ -n "${FAKE_INSPECT_BARRIER_DIR:-}" ]; then
  mkdir -p "$FAKE_INSPECT_BARRIER_DIR"
  printf '%s\n' "$$" >> "$FAKE_INSPECT_BARRIER_DIR/calls"
  : > "$FAKE_INSPECT_BARRIER_DIR/started"
  while [ ! -e "$FAKE_INSPECT_BARRIER_DIR/release" ]; do sleep 0.01; done
fi
printf 'agent-workbench 0.2.0\n'
"#,
    );
    write_executable(
        &root.join("scripts/build-release-assets.sh"),
        r#"#!/bin/sh
set -eu
tag="$1"
out="$2"
if [ -n "${FAKE_ASSEMBLY_BARRIER_DIR:-}" ]; then
  mkdir -p "$FAKE_ASSEMBLY_BARRIER_DIR"
  printf '%s\n' "$$" >> "$FAKE_ASSEMBLY_BARRIER_DIR/calls"
  : > "$FAKE_ASSEMBLY_BARRIER_DIR/started"
  while [ ! -e "$FAKE_ASSEMBLY_BARRIER_DIR/release" ]; do sleep 0.01; done
fi
mkdir -p "$out"
printf binary > "$out/agent-workbench-${tag}-linux-x86_64.tar.gz"
printf skill > "$out/agent-workbench-${tag}-skill.tar.gz"
printf docs > "$out/agent-workbench-${tag}-docs.tar.gz"
printf source > "$out/agent-workbench-${tag}-source.tar.gz"
printf metadata > "$out/agent-workbench-${tag}-release-metadata.txt"
(cd "$out" && sha256sum \
  "agent-workbench-${tag}-linux-x86_64.tar.gz" \
  "agent-workbench-${tag}-skill.tar.gz" \
  "agent-workbench-${tag}-docs.tar.gz" \
  "agent-workbench-${tag}-source.tar.gz" \
  "agent-workbench-${tag}-release-metadata.txt" \
  > "agent-workbench-${tag}-checksums.txt")
if [ -n "${FAKE_ASSEMBLY_REMOVE_BINARY_AFTER_BUILD:-}" ]; then
  rm -f "$PWD/target/release/agent-workbench"
fi
"#,
    );
    git(root, &["init", "-q"]);
    git(
        root,
        &["config", "user.email", "release-test@example.invalid"],
    );
    git(root, &["config", "user.name", "Release Test"]);
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "Prepare release source"]);
    let commit = git(root, &["rev-parse", "HEAD"]);
    git(root, &["init", "--bare", "-q", ".remote.git"]);
    git(root, &["remote", "add", "origin", ".remote.git"]);
    commit
}

#[derive(Clone, Debug)]
struct ReleaseWork {
    work_unit_id: String,
    activation_id: String,
    snapshot_id: String,
}

fn init_release_project(root: &Path, commit: &str) -> ReleaseWork {
    let work = init_open_release_project(root, commit);
    ok(
        root,
        &[
            "work",
            "close",
            &work.work_unit_id,
            "--summary",
            "release boundary is complete",
        ],
    );
    work
}

fn init_open_release_project(root: &Path, commit: &str) -> ReleaseWork {
    ok(root, &["init"]);
    let started = ok(root, &["work", "start", "release qualification"]);
    let work_unit_id = field(&started, "work_unit_id").to_string();
    let activation_id = field(&started, "activation_id").to_string();
    ok(
        root,
        &[
            "record",
            "create",
            "--topic",
            "release qualification",
            "--work-unit",
            &work_unit_id,
            "--work-performed",
            "recorded release boundary",
        ],
    );
    ok(
        root,
        &[
            "repository",
            "add",
            "release-source",
            "--path",
            root.to_str().unwrap(),
            "--head",
            commit,
            "--status",
            "clean",
        ],
    );
    let snapshot = ok(
        root,
        &[
            "repository",
            "snapshot",
            "add",
            "--repository",
            "release-source",
            "--activation",
            &activation_id,
            "--head",
            commit,
            "--branch",
            "main",
            "--status",
            "clean",
            "--clean",
        ],
    );
    let snapshot_id = field(&snapshot, "repository_snapshot_id").to_string();
    let ready = ok(root, &["gate", "close-ready", &work_unit_id, "--dry-run"]);
    assert!(ready.contains("result: pass"), "{ready}");
    ReleaseWork {
        work_unit_id,
        activation_id,
        snapshot_id,
    }
}

fn advance_release_work(root: &Path, work: &mut ReleaseWork, commit: &str) {
    let snapshot = ok(
        root,
        &[
            "repository",
            "snapshot",
            "add",
            "--repository",
            "release-source",
            "--activation",
            &work.activation_id,
            "--head",
            commit,
            "--branch",
            "main",
            "--status",
            "clean",
            "--clean",
        ],
    );
    let current = field(&snapshot, "repository_snapshot_id").to_string();
    ok(
        root,
        &[
            "repository",
            "compare",
            "add",
            "--base",
            &work.snapshot_id,
            "--current",
            &current,
            "--type",
            "close",
            "--head-changed",
            "--result",
            "changed_classified",
        ],
    );
    work.snapshot_id = current;
}

#[test]
fn source_release_inventory_excludes_managed_project_state() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    release_source(root);

    for member in [
        "docs/README.md",
        "docs/mkdocs.yml",
        "docs/requirements.txt",
        "skills/agent-workbench/SKILL.md",
        "skills/agent-workbench/agents/openai.yaml",
        "skills/agent-workbench/references/cli-workflow.md",
        "skills/agent-workbench/references/close-ready-troubleshooting.md",
        "skills/agent-workbench/references/interruption-recovery.md",
        "skills/agent-workbench/references/quickstart.md",
        "skills/agent-workbench/references/repository-validation.md",
        "skills/agent-workbench/references/review-recipes.md",
        "skills/agent-workbench/references/state-recovery.md",
        "docs/content/agent-skills.md",
        "docs/content/concepts.md",
        "docs/content/design-packages.md",
        "docs/content/index.md",
        "docs/content/operations.md",
        "docs/content/quick-start.md",
        "docs/content/reference.md",
        "docs/content/workflows.md",
        "src/lib.rs",
    ] {
        let path = root.join(member);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!("public source member: {member}\n")).unwrap();
    }
    fs::create_dir_all(root.join(".agent-workbench/designs/private")).unwrap();
    fs::write(
        root.join(".agent-workbench/designs/private/architecture.md"),
        "managed project design state\n",
    )
    .unwrap();
    write_executable(
        &root.join("scripts/build-release-assets.sh"),
        include_str!("../../../scripts/build-release-assets.sh"),
    );
    git(root, &["add", "."]);
    git(
        root,
        &[
            "add",
            "-f",
            ".agent-workbench/designs/private/architecture.md",
        ],
    );
    git(root, &["commit", "-qm", "Assemble bounded source release"]);

    let fake_bin = root.join(".fake-build-bin");
    write_executable(&fake_bin.join("cargo"), "#!/bin/sh\nexit 0\n");
    let artifacts = root.join("artifacts");
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let output = Command::new(root.join("scripts/build-release-assets.sh"))
        .current_dir(root)
        .arg("v0.2.0")
        .arg(&artifacts)
        .env("PATH", path)
        .env("CARGO_TARGET_DIR", root.join("target"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "asset build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let archive = artifacts.join("agent-workbench-v0.2.0-source.tar.gz");
    let inventory = Command::new("tar")
        .args(["-tzf", archive.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(inventory.status.success());
    let inventory = String::from_utf8(inventory.stdout).unwrap();
    assert!(inventory.lines().any(|member| member == "Cargo.toml"));
    assert!(inventory.lines().any(|member| member == "src/lib.rs"));
    assert!(
        !inventory
            .lines()
            .any(|member| member == ".agent-workbench" || member.starts_with(".agent-workbench/"))
    );
}

fn fake_gh(root: &Path) -> (String, String) {
    let state = root.join(".fake-gh");
    fs::create_dir_all(&state).unwrap();
    let executable = root.join(".fake-bin/gh");
    write_executable(
        &executable,
        r#"#!/usr/bin/env python3
import hashlib, json, os, pathlib, shutil, sys, time

state = pathlib.Path(os.environ["FAKE_GH_STATE"])
state.mkdir(parents=True, exist_ok=True)
args = sys.argv[1:]
if len(args) < 3 or args[0] != "release":
    print("unsupported fake gh command", file=sys.stderr)
    sys.exit(2)
action, tag = args[1], args[2]
meta_path = state / "release.json"
assets_dir = state / "assets"

def metadata():
    if not meta_path.exists():
        print("release not found", file=sys.stderr)
        sys.exit(1)
    meta = json.loads(meta_path.read_text())
    assets = []
    if assets_dir.exists():
        for path in sorted(assets_dir.iterdir()):
            data = path.read_bytes()
            assets.append({"name": path.name, "size": len(data), "digest": "sha256:" + hashlib.sha256(data).hexdigest()})
    meta["assets"] = assets
    return meta

if action == "view":
    print(json.dumps(metadata()))
elif action == "create":
    if meta_path.exists():
        print("release already exists", file=sys.stderr)
        sys.exit(1)
    barrier = os.environ.get("FAKE_GH_CREATE_BARRIER")
    if barrier:
        barrier = pathlib.Path(barrier)
        barrier.mkdir(parents=True, exist_ok=True)
        with (barrier / "calls").open("a") as calls: calls.write(str(os.getpid()) + "\n")
        (barrier / "started").touch()
        while not (barrier / "release").exists(): time.sleep(0.01)
    if os.environ.get("FAKE_GH_FAIL_BEFORE_CREATE") == "1":
        print("injected failure before create", file=sys.stderr)
        sys.exit(1)
    title, body, files = tag, "", []
    i = 3
    while i < len(args):
        if args[i] == "--verify-tag": i += 1
        elif args[i] == "--title": title, i = args[i+1], i + 2
        elif args[i] == "--notes-file": body, i = pathlib.Path(args[i+1]).read_text(), i + 2
        elif args[i] == "--notes": body, i = args[i+1], i + 2
        else: files.append(pathlib.Path(args[i])); i += 1
    assets_dir.mkdir(parents=True, exist_ok=True)
    selected = files[:1] if os.environ.get("FAKE_GH_PARTIAL_CREATE") == "1" else files
    for path in selected: shutil.copy2(path, assets_dir / path.name)
    meta_path.write_text(json.dumps({"tagName": tag, "name": title, "body": body, "targetCommitish": ""}))
    if os.environ.get("FAKE_GH_FAIL_AFTER_CREATE") == "1" or os.environ.get("FAKE_GH_PARTIAL_CREATE") == "1":
        print("injected failure after create", file=sys.stderr)
        sys.exit(1)
elif action == "upload":
    metadata()
    for value in args[3:]:
        path = pathlib.Path(value)
        target = assets_dir / path.name
        if target.exists():
            print("asset already exists", file=sys.stderr)
            sys.exit(1)
        shutil.copy2(path, target)
elif action == "download":
    metadata()
    destination = pathlib.Path(args[args.index("--dir") + 1])
    destination.mkdir(parents=True, exist_ok=True)
    for path in assets_dir.iterdir(): shutil.copy2(path, destination / path.name)
else:
    print("unsupported fake gh release action", file=sys.stderr)
    sys.exit(2)
"#,
    );
    let inherited = std::env::var("PATH").unwrap();
    (
        format!("{}:{inherited}", executable.parent().unwrap().display()),
        state.to_string_lossy().into_owned(),
    )
}

fn source_published(root: &Path) -> (String, String, String) {
    let commit = release_source(root);
    let work = init_release_project(root, &commit);
    let assembled = ok(
        root,
        &[
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
            "assemble-source",
        ],
    );
    let candidate = field(&assembled, "candidate").to_string();
    let assembled_revision = field(&assembled, "current_revision").to_string();
    let inspected = ok(
        root,
        &[
            "operator",
            "release",
            "candidate",
            "inspect",
            &candidate,
            "--expected-current",
            &assembled_revision,
            "--idempotency-key",
            "inspect-source",
        ],
    );
    let inspected_revision = field(&inspected, "current_revision").to_string();
    let published = ok(
        root,
        &[
            "operator",
            "release",
            "publish-source",
            &candidate,
            "--expected-current",
            &inspected_revision,
            "--idempotency-key",
            "publish-source",
        ],
    );
    let revision = field(&published, "current_revision").to_string();
    (candidate, revision, published)
}

fn assemble_next_release(
    root: &Path,
    work: &mut ReleaseWork,
    version: &str,
    key: &str,
) -> (String, String) {
    fs::write(
        root.join("Cargo.toml"),
        format!("[package]\nname = \"agent-workbench\"\nversion = \"{version}\"\n"),
    )
    .unwrap();
    fs::write(
        root.join("skills/agent-workbench/CLI_VERSION"),
        format!("v{version}\n"),
    )
    .unwrap();
    fs::write(
        root.join("CHANGELOG.md"),
        format!("# v{version}\n\nCandidate notes.\n"),
    )
    .unwrap();
    write_executable(
        &root.join("target/release/agent-workbench"),
        &format!("#!/bin/sh\nprintf 'agent-workbench {version}\\n'\n"),
    );
    git(root, &["add", "."]);
    git(
        root,
        &["commit", "-qm", &format!("Prepare release {version}")],
    );
    let commit = git(root, &["rev-parse", "HEAD"]);
    advance_release_work(root, work, &commit);
    let assembled = ok(
        root,
        &[
            "operator",
            "release",
            "candidate",
            "assemble",
            "--work",
            &work.work_unit_id,
            "--version",
            version,
            "--commit",
            &commit,
            "--expected-current",
            "absent",
            "--idempotency-key",
            key,
        ],
    );
    (
        field(&assembled, "candidate").to_string(),
        field(&assembled, "current_revision").to_string(),
    )
}
