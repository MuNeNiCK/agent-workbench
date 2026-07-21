use super::*;

#[test]
fn authorized_supersession_links_a_distinct_candidate_without_rewriting_history() {
    let temp = tempfile::tempdir().unwrap();
    let commit = release_source(temp.path());
    let mut work = init_release_project(temp.path(), &commit);
    let first = ok(
        temp.path(),
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
            "assemble-first",
        ],
    );
    let first_candidate = field(&first, "candidate").to_string();
    let first_revision = field(&first, "current_revision").to_string();

    fs::write(
        temp.path().join("Cargo.toml"),
        "[package]\nname = \"agent-workbench\"\nversion = \"0.2.1\"\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("skills/agent-workbench/CLI_VERSION"),
        "v0.2.1\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("CHANGELOG.md"),
        "# v0.2.1\n\nCorrected candidate.\n",
    )
    .unwrap();
    write_executable(
        &temp.path().join("target/release/agent-workbench"),
        "#!/bin/sh\nprintf 'agent-workbench 0.2.1\\n'\n",
    );
    git(temp.path(), &["add", "."]);
    git(
        temp.path(),
        &["commit", "-qm", "Prepare corrected candidate"],
    );
    let second_commit = git(temp.path(), &["rev-parse", "HEAD"]);
    advance_release_work(temp.path(), &mut work, &second_commit);
    let second = ok(
        temp.path(),
        &[
            "operator",
            "release",
            "candidate",
            "assemble",
            "--work",
            &work.work_unit_id,
            "--version",
            "0.2.1",
            "--commit",
            &second_commit,
            "--expected-current",
            "absent",
            "--idempotency-key",
            "assemble-second",
        ],
    );
    let second_candidate = field(&second, "candidate").to_string();
    let second_revision = field(&second, "current_revision").to_string();
    let authority = ok(
        temp.path(),
        &[
            "authority",
            "event",
            "add",
            "--type",
            "user_instruction",
            "--summary",
            "Supersede the first release candidate with the corrected candidate",
            "--scope",
            "project",
        ],
    );
    let authority = field(&authority, "authority_event_id").to_string();
    let superseded = ok(
        temp.path(),
        &[
            "operator",
            "release",
            "supersede",
            &first_candidate,
            "--expected-current",
            &first_revision,
            "--idempotency-key",
            "supersede-first",
            "--by",
            &second_candidate,
            "--authority",
            &authority,
            "--reason",
            "Use the corrected candidate without deleting the first candidate",
        ],
    );
    assert!(superseded.contains("state: superseded"), "{superseded}");
    let successor = ok(
        temp.path(),
        &[
            "operator",
            "release",
            "candidate",
            "inspect",
            &second_candidate,
            "--expected-current",
            &second_revision,
            "--idempotency-key",
            "inspect-successor",
        ],
    );
    assert!(successor.contains("state: locally_verified"));
    let successor_revision = field(&successor, "current_revision").to_string();

    let replayed = ok(
        temp.path(),
        &[
            "operator",
            "release",
            "supersede",
            &first_candidate,
            "--expected-current",
            &first_revision,
            "--idempotency-key",
            "supersede-first",
            "--by",
            &second_candidate,
            "--authority",
            &authority,
            "--reason",
            "Use the corrected candidate without deleting the first candidate",
        ],
    );
    assert!(replayed.contains("state: superseded"));
    assert!(replayed.contains("already_applied: true"));

    let reverse = aw(
        temp.path(),
        &[
            "operator",
            "release",
            "supersede",
            &second_candidate,
            "--expected-current",
            &successor_revision,
            "--idempotency-key",
            "supersede-reverse-cycle",
            "--by",
            &first_candidate,
            "--authority",
            &authority,
            "--reason",
            "Attempt to reverse the existing supersession",
        ],
    );
    assert!(!reverse.status.success());
    assert!(
        String::from_utf8_lossy(&reverse.stderr)
            .contains("terminal release candidate cannot be selected as a successor")
    );
    let status = ok(temp.path(), &["status"]);
    assert!(status.contains("project_integrity: clear"));
    assert!(status.contains(&format!(
        "owner: release_candidate:{second_candidate}\nowner_state: locally_verified"
    )));

    let (third_candidate, third_revision) =
        assemble_next_release(temp.path(), &mut work, "0.2.2", "assemble-third");
    let conflict = aw(
        temp.path(),
        &[
            "operator",
            "release",
            "supersede",
            &third_candidate,
            "--expected-current",
            &third_revision,
            "--idempotency-key",
            "supersede-third",
            "--by",
            &second_candidate,
            "--authority",
            &authority,
            "--reason",
            "Attempt a conflicting successor link",
        ],
    );
    assert!(!conflict.status.success());
    assert!(
        String::from_utf8_lossy(&conflict.stderr)
            .contains("release successor is already linked to a different predecessor")
    );
    let unchanged = ok(
        temp.path(),
        &[
            "operator",
            "release",
            "candidate",
            "inspect",
            &third_candidate,
            "--expected-current",
            &third_revision,
            "--idempotency-key",
            "inspect-third-after-conflict",
        ],
    );
    assert!(unchanged.contains("state: locally_verified"));
}
