use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Output};

// Public black-box behavior is owned here by command responsibility. Shared code prepares inputs
// and executes commands; each responsibility module owns its observable-result assertions.

fn binary_under_test() -> PathBuf {
    std::env::var_os("AGENT_WORKBENCH_UNDER_TEST")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_agent-workbench")))
}

fn aw(root: &Path, args: &[&str]) -> Output {
    Command::new(binary_under_test())
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .expect("failed to run agent-workbench")
}
fn aw_env(root: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut command = Command::new(binary_under_test());
    command.arg("--root").arg(root).args(args);
    for (key, value) in envs {
        command.env(key, value);
    }
    command.output().expect("failed to run agent-workbench")
}
fn ok(root: &Path, args: &[&str]) -> String {
    let output = aw(root, args);
    assert!(
        output.status.success(),
        "command failed: {:?}\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout must be utf-8")
}
fn ok_env(root: &Path, args: &[&str], envs: &[(&str, &str)]) -> String {
    let output = aw_env(root, args, envs);
    assert!(
        output.status.success(),
        "command failed: {:?}\nstdout:\n{}\nstderr:\n{}",
        args,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout must be utf-8")
}

mod decomposition;
mod doctor;
mod migration;
mod phase;
mod registry;
mod release;
mod review;
mod smoke;
mod update;
mod work;
