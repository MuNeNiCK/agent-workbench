use super::*;

fn staging_directory_count(root: &Path) -> usize {
    fs::read_dir(root.join(".agent-workbench/release-candidates"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("staging-"))
        .count()
}

#[test]
fn operator_candidate_assembles_and_reinspects_only_bound_local_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let commit = release_source(temp.path());
    let work = init_release_project(temp.path(), &commit);
    let assembled = ok(
        temp.path(),
        &[
            "operator",
            "release",
            "candidate",
            "assemble",
            "--version",
            "0.2.0",
            "--commit",
            &commit,
            "--expected-current",
            "absent",
            "--idempotency-key",
            "assemble-one",
        ],
    );
    assert!(assembled.contains("state: assembled"));
    assert_eq!(field(&assembled, "work_unit_id"), work.work_unit_id);
    assert!(assembled.contains("next: agent-workbench operator release candidate inspect"));
    let candidate = field(&assembled, "candidate");
    let candidate_directory = temp
        .path()
        .join(".agent-workbench/release-candidates")
        .join(candidate);
    fs::remove_dir_all(&candidate_directory).unwrap();
    let recovered = ok(
        temp.path(),
        &[
            "operator",
            "release",
            "candidate",
            "assemble",
            "--version",
            "0.2.0",
            "--commit",
            &commit,
            "--expected-current",
            "absent",
            "--idempotency-key",
            "assemble-one",
        ],
    );
    assert!(recovered.contains("already_applied: true"));
    assert!(candidate_directory.is_dir());
    let status = ok(temp.path(), &["status"]);
    assert!(status.contains(&format!("owner: release_candidate:{candidate}")));
    assert!(status.contains("owner_state: assembled"));
    let owner = format!("release_candidate:{candidate}");
    let status_next = owner_field(&status, &owner, "owner_next");
    assert_eq!(status_next, field(&assembled, "next"));
    let next = ok(temp.path(), &["next"]);
    assert!(next.contains(&format!("owner: release_candidate:{candidate}")));
    assert_eq!(owner_field(&next, &owner, "owner_next"), status_next);
    let rendered = execute_rendered(temp.path(), status_next, &[]);
    assert!(
        rendered.status.success(),
        "rendered release action failed: {}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    let inspected = String::from_utf8(rendered.stdout).unwrap();
    assert!(inspected.contains("state: locally_verified"));
    assert!(inspected.contains("next: agent-workbench operator release publish-source"));
    let (status, next) = release_owner_outputs(temp.path());
    assert!(status.contains(&format!("owner: release_candidate:{candidate}")));
    assert!(status.contains("owner_state: locally_verified"));
    assert!(owner_field(&status, &owner, "owner_next").contains("publish-source"));
    assert!(next.contains(&format!("owner: release_candidate:{candidate}")));
    assert_eq!(
        owner_field(&next, &owner, "owner_next"),
        owner_field(&status, &owner, "owner_next")
    );
    let inspected_revision = field(&inspected, "current_revision").to_string();

    let retry = execute_rendered(temp.path(), status_next, &[]);
    assert!(retry.status.success());
    let retry = String::from_utf8(retry.stdout).unwrap();
    assert!(retry.contains("already_applied: true"));
    assert_eq!(
        field(&retry, "current_revision"),
        field(&inspected, "current_revision")
    );

    let published = ok(
        temp.path(),
        &[
            "operator",
            "release",
            "publish-source",
            candidate,
            "--expected-current",
            &inspected_revision,
            "--idempotency-key",
            "publish-source-one",
        ],
    );
    assert!(published.contains("state: source_published"), "{published}");
    assert!(published.contains("next: agent-workbench operator release publish-assets"));
    let (status, next) = release_owner_outputs(temp.path());
    assert!(status.contains(&format!("owner: release_candidate:{candidate}")));
    assert!(status.contains("owner_state: source_published"));
    assert!(owner_field(&status, &owner, "owner_next").contains("publish-assets"));
    assert!(next.contains(&format!("owner: release_candidate:{candidate}")));
    assert_eq!(
        owner_field(&next, &owner, "owner_next"),
        owner_field(&status, &owner, "owner_next")
    );
    let publication_retry = ok(
        temp.path(),
        &[
            "operator",
            "release",
            "publish-source",
            candidate,
            "--expected-current",
            &inspected_revision,
            "--idempotency-key",
            "publish-source-one",
        ],
    );
    assert!(publication_retry.contains("already_applied: true"));
    assert_eq!(
        field(&publication_retry, "current_revision"),
        field(&published, "current_revision")
    );

    let (path, gh_state) = fake_gh(temp.path());
    let published_revision = field(&published, "current_revision").to_string();
    let assets = ok_env(
        temp.path(),
        &[
            "operator",
            "release",
            "publish-assets",
            candidate,
            "--expected-current",
            &published_revision,
            "--idempotency-key",
            "publish-assets-one",
        ],
        &[("PATH", &path), ("FAKE_GH_STATE", &gh_state)],
    );
    assert!(assets.contains("state: assets_published"), "{assets}");
    let (status, next) = release_owner_outputs(temp.path());
    assert!(status.contains(&format!("owner: release_candidate:{candidate}")));
    assert!(status.contains("owner_state: assets_published"));
    assert!(owner_field(&status, &owner, "owner_next").contains("verify-remote"));
    assert!(next.contains(&format!("owner: release_candidate:{candidate}")));
    assert_eq!(
        owner_field(&next, &owner, "owner_next"),
        owner_field(&status, &owner, "owner_next")
    );
    let assets_revision = field(&assets, "current_revision").to_string();
    let verified = ok_env(
        temp.path(),
        &[
            "operator",
            "release",
            "verify-remote",
            candidate,
            "--expected-current",
            &assets_revision,
            "--idempotency-key",
            "verify-remote-one",
        ],
        &[("PATH", &path), ("FAKE_GH_STATE", &gh_state)],
    );
    assert!(verified.contains("state: remotely_verified"), "{verified}");
    assert!(verified.contains("next: agent-workbench status"));
    let terminal_status = ok(temp.path(), &["status"]);
    assert!(!terminal_status.contains(&format!("owner: release_candidate:{candidate}")));
}

#[test]
fn failed_release_assembly_removes_every_owned_staging_directory() {
    let subject_root = tempfile::tempdir().unwrap();
    let subject_commit = release_source(subject_root.path());
    let subject_work = init_release_project(subject_root.path(), &subject_commit);
    let subject_failure = aw_env(
        subject_root.path(),
        &[
            "operator",
            "release",
            "candidate",
            "assemble",
            "--work",
            &subject_work.work_unit_id,
            "--version",
            "0.2.0",
            "--commit",
            &subject_commit,
            "--expected-current",
            "absent",
            "--idempotency-key",
            "subject-failure-cleanup",
        ],
        &[("FAKE_ASSEMBLY_REMOVE_BINARY_AFTER_BUILD", "1")],
    );
    assert!(!subject_failure.status.success());
    assert_eq!(staging_directory_count(subject_root.path()), 0);

    let mismatch_root = tempfile::tempdir().unwrap();
    let mismatch_commit = release_source(mismatch_root.path());
    let mismatch_work = init_release_project(mismatch_root.path(), &mismatch_commit);
    ok(
        mismatch_root.path(),
        &[
            "operator",
            "release",
            "candidate",
            "assemble",
            "--work",
            &mismatch_work.work_unit_id,
            "--version",
            "0.2.0",
            "--commit",
            &mismatch_commit,
            "--expected-current",
            "absent",
            "--idempotency-key",
            "mismatch-cleanup",
        ],
    );
    let mismatch_failure = aw_env(
        mismatch_root.path(),
        &[
            "operator",
            "release",
            "candidate",
            "assemble",
            "--work",
            &mismatch_work.work_unit_id,
            "--version",
            "0.2.0",
            "--commit",
            &mismatch_commit,
            "--expected-current",
            "absent",
            "--idempotency-key",
            "mismatch-cleanup",
        ],
        &[("FAKE_ASSEMBLY_ASSET_DRIFT", "1")],
    );
    assert!(!mismatch_failure.status.success());
    assert_eq!(staging_directory_count(mismatch_root.path()), 0);
}

#[test]
fn assembly_requires_one_close_ready_work_and_publishes_nothing_on_zero_matches() {
    let temp = tempfile::tempdir().unwrap();
    let commit = release_source(temp.path());
    ok(temp.path(), &["init"]);
    let rejected = aw(
        temp.path(),
        &[
            "operator",
            "release",
            "candidate",
            "assemble",
            "--version",
            "0.2.0",
            "--commit",
            &commit,
            "--expected-current",
            "absent",
            "--idempotency-key",
            "assemble-without-work",
        ],
    );
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr)
            .contains("no close-ready work for reviewed commit")
    );
    let candidate_root = temp.path().join(".agent-workbench/release-candidates");
    assert!(
        !candidate_root.exists() || std::fs::read_dir(candidate_root).unwrap().next().is_none()
    );
    let status = ok(temp.path(), &["status"]);
    assert!(!status.contains("owner: release_candidate:"));
}

#[test]
fn assembly_rejects_an_open_work_even_when_its_close_ready_gate_passes() {
    let temp = tempfile::tempdir().unwrap();
    let commit = release_source(temp.path());
    let work = init_open_release_project(temp.path(), &commit);

    let rejected = aw(
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
            "assemble-open-work",
        ],
    );
    assert!(!rejected.status.success());
    let error = String::from_utf8_lossy(&rejected.stderr);
    assert!(error.contains("is not close-ready"), "{error}");
    assert!(!ok(temp.path(), &["status"]).contains("owner: release_candidate:"));
}

#[test]
fn assembly_accepts_the_same_waived_review_plans_as_close_ready() {
    let temp = tempfile::tempdir().unwrap();
    let commit = release_source(temp.path());
    let work = init_open_release_project(temp.path(), &commit);
    let plan = ok(
        temp.path(),
        &[
            "review",
            "plan",
            "add",
            "--work-unit",
            &work.work_unit_id,
            "--type",
            "general",
            "--stage",
            "close-ready",
            "--required",
        ],
    );
    let plan = field(&plan, "review_plan_id");
    ok(
        temp.path(),
        &[
            "review",
            "plan",
            "waive",
            plan,
            "--reason",
            "a separate accepted review supplies the release boundary",
        ],
    );
    let ready = ok(
        temp.path(),
        &["gate", "close-ready", &work.work_unit_id, "--dry-run"],
    );
    assert!(ready.contains("result: pass"), "{ready}");
    ok(
        temp.path(),
        &[
            "work",
            "close",
            &work.work_unit_id,
            "--summary",
            "release boundary is complete",
        ],
    );

    let assembled = ok(
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
            "assemble-with-waived-review",
        ],
    );
    assert!(assembled.contains("state: assembled"), "{assembled}");
}

#[test]
fn unrelated_work_review_state_does_not_block_assembly_or_revalidation() {
    let temp = tempfile::tempdir().unwrap();
    let commit = release_source(temp.path());
    let release_work = init_release_project(temp.path(), &commit);
    let started = ok(temp.path(), &["work", "start", "separate review owner"]);
    let finding_work = field(&started, "work_unit_id").to_string();
    let plan = ok(
        temp.path(),
        &[
            "review",
            "plan",
            "add",
            "--work-unit",
            &finding_work,
            "--type",
            "general",
            "--stage",
            "resume-ready",
        ],
    );
    let plan = field(&plan, "review_plan_id").to_string();
    let run = ok(
        temp.path(),
        &[
            "review",
            "run",
            "add",
            "--plan",
            &plan,
            "--type",
            "fresh",
            "--purpose",
            "new_unbiased_review",
            "--new-findings",
            "1",
            "--summary",
            "separate work has unresolved review state",
        ],
    );
    let run = field(&run, "review_run_id").to_string();
    let assembled = ok(
        temp.path(),
        &[
            "operator",
            "release",
            "candidate",
            "assemble",
            "--work",
            &release_work.work_unit_id,
            "--version",
            "0.2.0",
            "--commit",
            &commit,
            "--expected-current",
            "absent",
            "--idempotency-key",
            "assemble-with-unrelated-review",
        ],
    );
    let candidate = field(&assembled, "candidate").to_string();
    let revision = field(&assembled, "current_revision").to_string();

    let finding = ok(
        temp.path(),
        &[
            "finding",
            "add",
            "--run",
            &run,
            "--type",
            "process_finding",
            "--severity",
            "high",
            "--description",
            "unrelated work remains unresolved",
        ],
    );
    let finding = field(&finding, "finding_id").to_string();
    ok(
        temp.path(),
        &["finding", "classify", &finding, "--classification", "valid"],
    );

    let inspected = ok(
        temp.path(),
        &[
            "operator",
            "release",
            "candidate",
            "inspect",
            &candidate,
            "--expected-current",
            &revision,
            "--idempotency-key",
            "inspect-with-unrelated-finding",
        ],
    );
    assert!(inspected.contains("state: locally_verified"), "{inspected}");
}

#[test]
fn requested_release_attempt_freezes_project_review_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let commit = release_source(temp.path());
    let release_work = init_release_project(temp.path(), &commit);
    let assembled = ok(
        temp.path(),
        &[
            "operator",
            "release",
            "candidate",
            "assemble",
            "--work",
            &release_work.work_unit_id,
            "--version",
            "0.2.0",
            "--commit",
            &commit,
            "--expected-current",
            "absent",
            "--idempotency-key",
            "assemble-before-review-freeze",
        ],
    );
    let candidate = field(&assembled, "candidate");
    let revision = field(&assembled, "current_revision");
    let started = ok(temp.path(), &["work", "start", "review mutation contender"]);
    let work = field(&started, "work_unit_id");
    let plan = ok(
        temp.path(),
        &[
            "review",
            "plan",
            "add",
            "--work-unit",
            work,
            "--type",
            "general",
            "--stage",
            "resume-ready",
        ],
    );
    let plan = field(&plan, "review_plan_id");

    let conn =
        rusqlite::Connection::open(agent_workbench::default_ledger_path(temp.path())).unwrap();
    conn.execute(
        r#"
        insert into release_candidate_attempts(
          project_id,release_candidate_id,action,idempotency_key,expected_current,
          payload_identity,requested_identity,status,created_at
        )
        select project_id,id,'publish-source','freeze-review',?2,
               'payload','requested','requested',current_timestamp
        from release_candidates where candidate_handle=?1
        "#,
        rusqlite::params![candidate, revision],
    )
    .unwrap();
    drop(conn);

    let rejected = aw(
        temp.path(),
        &[
            "review",
            "run",
            "add",
            "--plan",
            plan,
            "--type",
            "fresh",
            "--purpose",
            "new_unbiased_review",
            "--new-findings",
            "0",
            "--clean",
            "--summary",
            "must not race external publication",
        ],
    );
    assert!(!rejected.status.success());
    assert!(
        String::from_utf8_lossy(&rejected.stderr)
            .contains("review mutation blocked by requested release attempt")
    );
}

#[test]
fn candidate_transition_rejects_a_changed_close_ready_source_boundary() {
    let temp = tempfile::tempdir().unwrap();
    let commit = release_source(temp.path());
    let work = init_release_project(temp.path(), &commit);
    let assembled = ok(
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
            "assemble-before-source-drift",
        ],
    );
    let candidate = field(&assembled, "candidate");
    let revision = field(&assembled, "current_revision");
    let snapshot = ok(
        temp.path(),
        &[
            "repository",
            "snapshot",
            "add",
            "--repository",
            "release-source",
            "--activation",
            &work.activation_id,
            "--head",
            &commit,
            "--branch",
            "main",
            "--status",
            "clean",
            "--clean",
        ],
    );
    let current_snapshot = field(&snapshot, "repository_snapshot_id");
    ok(
        temp.path(),
        &[
            "repository",
            "compare",
            "add",
            "--base",
            &work.snapshot_id,
            "--current",
            current_snapshot,
            "--type",
            "close",
            "--result",
            "same",
        ],
    );
    let rejected = aw(
        temp.path(),
        &[
            "operator",
            "release",
            "candidate",
            "inspect",
            candidate,
            "--expected-current",
            revision,
            "--idempotency-key",
            "inspect-after-source-drift",
        ],
    );
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("release work boundary changed"));
    let status = ok(temp.path(), &["status"]);
    assert!(status.contains(&format!(
        "owner: release_candidate:{candidate}\nowner_state: assembled"
    )));
}

#[test]
fn omitted_work_counts_only_review_ready_closed_owners() {
    let temp = tempfile::tempdir().unwrap();
    let commit = release_source(temp.path());
    let first = init_release_project(temp.path(), &commit);

    let started = ok(
        temp.path(),
        &["work", "start", "second release qualification"],
    );
    let second_work = field(&started, "work_unit_id").to_string();
    let second_activation = field(&started, "activation_id").to_string();
    ok(
        temp.path(),
        &[
            "record",
            "create",
            "--topic",
            "second release qualification",
            "--work-unit",
            &second_work,
            "--work-performed",
            "recorded second release boundary",
        ],
    );
    let snapshot = ok(
        temp.path(),
        &[
            "repository",
            "snapshot",
            "add",
            "--repository",
            "release-source",
            "--activation",
            &second_activation,
            "--head",
            &commit,
            "--branch",
            "main",
            "--status",
            "clean",
            "--clean",
        ],
    );
    let second_snapshot = field(&snapshot, "repository_snapshot_id");
    ok(
        temp.path(),
        &[
            "repository",
            "compare",
            "add",
            "--base",
            &first.snapshot_id,
            "--current",
            second_snapshot,
            "--type",
            "close",
            "--result",
            "same",
        ],
    );
    ok(
        temp.path(),
        &[
            "work",
            "close",
            &second_work,
            "--summary",
            "second release boundary is complete",
        ],
    );

    let ambiguous = aw(
        temp.path(),
        &[
            "operator",
            "release",
            "candidate",
            "assemble",
            "--version",
            "0.2.0",
            "--commit",
            &commit,
            "--expected-current",
            "absent",
            "--idempotency-key",
            "ambiguous-closed-work",
        ],
    );
    assert!(!ambiguous.status.success());
    let error = String::from_utf8_lossy(&ambiguous.stderr);
    assert!(
        error.contains("release assembly work is ambiguous"),
        "{error}"
    );
    assert!(error.contains(&format!("--work {}", first.work_unit_id)));
    assert!(error.contains(&format!("--work {second_work}")));

    let plan = ok(
        temp.path(),
        &[
            "review",
            "plan",
            "add",
            "--work-unit",
            &second_work,
            "--type",
            "general",
            "--stage",
            "resume-ready",
        ],
    );
    let plan = field(&plan, "review_plan_id").to_string();
    ok(
        temp.path(),
        &[
            "review",
            "run",
            "add",
            "--plan",
            &plan,
            "--type",
            "fresh",
            "--purpose",
            "new_unbiased_review",
            "--new-findings",
            "1",
            "--summary",
            "second closed work is not release-ready",
        ],
    );

    let assembled = ok(
        temp.path(),
        &[
            "operator",
            "release",
            "candidate",
            "assemble",
            "--version",
            "0.2.0",
            "--commit",
            &commit,
            "--expected-current",
            "absent",
            "--idempotency-key",
            "only-review-ready-closed-work",
        ],
    );
    assert_eq!(field(&assembled, "work_unit_id"), first.work_unit_id);
}
