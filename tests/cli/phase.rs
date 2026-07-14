use super::*;

#[test]
fn phase_commands_group_tasks_and_drive_next_phase_order() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(temp.path(), &["work", "start", "aggregate implementation"]);
    ok(
        temp.path(),
        &["task", "add", "beta task", "--work-unit", "1"],
    );
    ok(
        temp.path(),
        &["task", "add", "alpha task", "--work-unit", "1"],
    );

    let beta = ok(
        temp.path(),
        &[
            "phase",
            "create",
            "--work-unit",
            "1",
            "--key",
            "beta",
            "--title",
            "Beta",
            "--kind",
            "feature",
            "--order",
            "2",
        ],
    );
    let alpha = ok(
        temp.path(),
        &[
            "phase",
            "create",
            "--work-unit",
            "1",
            "--key",
            "alpha",
            "--title",
            "Alpha",
            "--kind",
            "feature",
            "--order",
            "1",
        ],
    );
    assert!(beta.contains("phase_id: 1"));
    assert!(alpha.contains("phase_id: 2"));
    ok(temp.path(), &["phase", "assign", "1", "--task", "1"]);
    ok(temp.path(), &["phase", "assign", "2", "--task", "2"]);

    let next = ok(temp.path(), &["next"]);
    assert!(next.contains("next_phase_id: 2"));
    assert!(next.contains("next_phase_id:"));
    assert!(!next.contains("next_phase_key:"));

    let dependency = ok(
        temp.path(),
        &[
            "phase",
            "dependency",
            "add",
            "--from",
            "1",
            "--to",
            "2",
            "--type",
            "blocks",
            "--reason",
            "beta must settle first",
        ],
    );
    assert!(dependency.contains("dependency_id: 1"));
    let next = ok(temp.path(), &["next"]);
    assert!(next.contains("next_phase_id: 1"));
    assert!(next.contains("next_phase_id:"));
    assert!(!next.contains("next_phase_key:"));

    ok(
        temp.path(),
        &[
            "authority",
            "event",
            "add",
            "--type",
            "user_instruction",
            "--summary",
            "approve phase acceptance",
        ],
    );
    let accepted_dependency = ok(
        temp.path(),
        &[
            "phase",
            "dependency",
            "accept",
            "1",
            "--reason",
            "approved",
            "--authority",
            "1",
        ],
    );
    assert_eq!(accepted_dependency, "accepted phase dependency\n");
    let next = ok(temp.path(), &["next"]);
    assert!(next.contains("next_phase_id: 2"));

    let inventory = ok(temp.path(), &["phase", "inventory", "2"]);
    assert!(inventory.contains("task:2 [open decision=-] alpha task"));

    let accepted_phase = ok(
        temp.path(),
        &[
            "phase",
            "accept-out-of-scope",
            "1",
            "--reason",
            "approved",
            "--authority",
            "1",
        ],
    );
    assert_eq!(accepted_phase, "accepted phase out of scope\n");
}
