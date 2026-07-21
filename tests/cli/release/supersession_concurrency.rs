use super::*;

#[test]
fn concurrent_supersession_commits_one_lineage_and_rolls_back_the_loser() {
    let temp = tempfile::tempdir().unwrap();
    let first_commit = release_source(temp.path());
    let mut work = init_release_project(temp.path(), &first_commit);
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
            &first_commit,
            "--expected-current",
            "absent",
            "--idempotency-key",
            "assemble-race-first",
        ],
    );
    let first_candidate = field(&first, "candidate").to_string();
    let first_revision = field(&first, "current_revision").to_string();
    let (second_candidate, second_revision) =
        assemble_next_release(temp.path(), &mut work, "0.2.1", "assemble-race-second");
    let (successor, _) =
        assemble_next_release(temp.path(), &mut work, "0.2.2", "assemble-race-successor");
    let authority = ok(
        temp.path(),
        &[
            "authority",
            "event",
            "add",
            "--type",
            "user_instruction",
            "--summary",
            "Choose exactly one predecessor for a release successor",
            "--scope",
            "project",
        ],
    );
    let authority = field(&authority, "authority_event_id").to_string();

    let left_root = temp.path().to_path_buf();
    let left_candidate = first_candidate.clone();
    let left_revision = first_revision.clone();
    let left_successor = successor.clone();
    let left_authority = authority.clone();
    let left = std::thread::spawn(move || {
        aw(
            &left_root,
            &[
                "operator",
                "release",
                "supersede",
                &left_candidate,
                "--expected-current",
                &left_revision,
                "--idempotency-key",
                "supersede-race-left",
                "--by",
                &left_successor,
                "--authority",
                &left_authority,
                "--reason",
                "Concurrent lineage selection",
            ],
        )
    });
    let right_root = temp.path().to_path_buf();
    let right_candidate = second_candidate.clone();
    let right_revision = second_revision.clone();
    let right_successor = successor.clone();
    let right_authority = authority.clone();
    let right = std::thread::spawn(move || {
        aw(
            &right_root,
            &[
                "operator",
                "release",
                "supersede",
                &right_candidate,
                "--expected-current",
                &right_revision,
                "--idempotency-key",
                "supersede-race-right",
                "--by",
                &right_successor,
                "--authority",
                &right_authority,
                "--reason",
                "Concurrent lineage selection",
            ],
        )
    });
    let left = left.join().unwrap();
    let right = right.join().unwrap();
    assert_ne!(left.status.success(), right.status.success());

    let (winner, winner_revision, winner_key, loser, loser_revision, loser_key) =
        if left.status.success() {
            (
                &first_candidate,
                &first_revision,
                "supersede-race-left",
                &second_candidate,
                &second_revision,
                "supersede-race-right",
            )
        } else {
            (
                &second_candidate,
                &second_revision,
                "supersede-race-right",
                &first_candidate,
                &first_revision,
                "supersede-race-left",
            )
        };
    let replayed = ok(
        temp.path(),
        &[
            "operator",
            "release",
            "supersede",
            winner,
            "--expected-current",
            winner_revision,
            "--idempotency-key",
            winner_key,
            "--by",
            &successor,
            "--authority",
            &authority,
            "--reason",
            "Concurrent lineage selection",
        ],
    );
    assert!(replayed.contains("already_applied: true"));
    let rejected = aw(
        temp.path(),
        &[
            "operator",
            "release",
            "supersede",
            loser,
            "--expected-current",
            loser_revision,
            "--idempotency-key",
            loser_key,
            "--by",
            &successor,
            "--authority",
            &authority,
            "--reason",
            "Concurrent lineage selection",
        ],
    );
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr)
            .contains("release successor is already linked to a different predecessor")
    );
    let status = ok(temp.path(), &["status"]);
    assert!(status.contains("project_integrity: clear"));
    assert!(status.contains(&format!(
        "owner: release_candidate:{loser}\nowner_state: assembled"
    )));
}
