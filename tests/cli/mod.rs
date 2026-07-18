use std::path::Path;
use std::process::{Command, Output};

fn aw(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_agent-workbench"))
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .expect("failed to run agent-workbench")
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

mod runtime14;
