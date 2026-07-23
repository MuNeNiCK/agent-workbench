use super::*;
use agent_workbench::{
    AdjudicationInput, ClosureReady, ClosureSupersession, DesignPackageImport, NewAuthorityEvent,
    NewClosure, NewDecisionContinuation, NewDesignPackage, NewFinding, NewReviewPlan,
    NewReviewPolicy, NewReviewRun, NewTask, add_authority_event, add_closure,
    add_decision_continuation, add_finding, add_review_plan, add_review_policy, add_review_run,
    add_review_run_with_finding_result, add_task, adjudicate_verification, begin_correction,
    classify_finding, import_design_package, init_design_package, init_project, list_findings,
    ready_closure, start_work, supersede_closure,
};

#[test]
fn kpt_public_lifecycle_exposes_all_targets_and_replayable_dismissal() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(temp.path(), &["work", "start", "KPT lifecycle gate"]);
    let convert_help = ok(temp.path(), &["kpt", "item", "convert", "--help"]);
    for target in [
        "rule",
        "correction",
        "task",
        "command-profile",
        "review-policy",
        "decision",
        "design-version",
    ] {
        assert!(convert_help.contains(target));
    }
    ok(
        temp.path(),
        &[
            "kpt",
            "start",
            "--scope",
            "project",
            "--summary",
            "lifecycle",
        ],
    );
    for item in 1..=8 {
        ok(
            temp.path(),
            &[
                "kpt",
                "item",
                "add",
                "--type",
                "try",
                "--title",
                &format!("item-{item}"),
                "--details",
                &format!("action-{item}"),
            ],
        );
    }
    let authority = ok(
        temp.path(),
        &[
            "authority",
            "event",
            "add",
            "--type",
            "user_instruction",
            "--summary",
            "reviewed KPT no-action outcome",
            "--scope",
            "project",
        ],
    );
    let authority_id = cli_value(&authority, "authority_event_id");
    let initial = ok(temp.path(), &["kpt", "item", "list", "--review", "1"]);
    let manual_handle = kpt_item_handle(&initial, 8);
    assert!(initial.contains("legal_actions: convert,dismiss"));
    assert!(!initial.contains("dismissal.item_revision:"));
    let unsettled_open = ok(temp.path(), &["gate", "close-ready", "--dry-run"]);
    assert!(unsettled_open.contains("corrections_kpt_checked: fail"));
    assert!(unsettled_open.contains("8 unsettled KPT items"));
    let invalid = aw(
        temp.path(),
        &[
            "kpt",
            "item",
            "convert",
            "--item",
            "1",
            "--to",
            "rule",
            "--design-version",
            "999",
        ],
    );
    assert!(!invalid.status.success());
    assert!(String::from_utf8_lossy(&invalid.stderr).contains("unexpected operand"));
    assert!(
        ok(temp.path(), &["kpt", "item", "list", "--review", "1"]).contains("1 [review=1 try:open")
    );
    let ignored_defaults = aw(
        temp.path(),
        &[
            "kpt",
            "item",
            "convert",
            "--item",
            "8",
            "--to",
            "rule",
            "--priority",
            "critical",
            "--command-status",
            "fixed",
            "--fresh-clean",
            "99",
            "--default-run-mode",
            "resume",
        ],
    );
    assert!(!ignored_defaults.status.success());
    assert!(String::from_utf8_lossy(&ignored_defaults.stderr).contains("unexpected operand"));
    assert!(
        ok(temp.path(), &["kpt", "item", "list", "--review", "1"]).contains("8 [review=1 try:open")
    );

    let variants = [
        (1, vec!["--to", "rule"]),
        (2, vec!["--to", "correction"]),
        (3, vec!["--to", "task"]),
        (4, vec!["--to", "command-profile"]),
        (5, vec!["--to", "review-policy", "--review-type", "general"]),
        (6, vec!["--to", "decision"]),
    ];
    for (item, tail) in variants {
        let mut args = vec!["kpt", "item", "convert", "--item"];
        let item = item.to_string();
        args.push(&item);
        args.extend(tail);
        let converted = ok(temp.path(), &args);
        assert!(converted.contains("converted kpt item"));
        let replayed = ok(temp.path(), &args);
        assert!(replayed.contains("kpt item conversion already exists"));
        assert_eq!(
            conversion_projection(&converted),
            conversion_projection(&replayed)
        );
    }
    let changed_rule = aw(
        temp.path(),
        &[
            "kpt",
            "item",
            "convert",
            "--item",
            "1",
            "--to",
            "rule",
            "--details",
            "changed body",
        ],
    );
    assert!(!changed_rule.status.success());
    assert!(String::from_utf8_lossy(&changed_rule.stderr).contains("conversion_already_committed"));
    ok(
        temp.path(),
        &["design", "init", "sample", "--title", "Sample"],
    );
    let package = temp.path().join(".agent-workbench/designs/sample");
    let imported = ok(
        temp.path(),
        &["design", "import", package.to_str().unwrap()],
    );
    let design_version = cli_value(&imported, "design_version_id");
    let design_conversion = [
        "kpt",
        "item",
        "convert",
        "--item",
        "7",
        "--to",
        "design-version",
        "--design-version",
        design_version,
    ];
    let converted_design = ok(temp.path(), &design_conversion);
    assert!(converted_design.contains("converted kpt item"));
    let replayed_design = ok(temp.path(), &design_conversion);
    assert!(replayed_design.contains("kpt item conversion already exists"));
    assert_eq!(
        conversion_projection(&converted_design),
        conversion_projection(&replayed_design)
    );
    let rules = ok(temp.path(), &["rules", "applicable", "--scope", "project"]);
    assert!(rules.contains("[kpt_rule:project"));
    assert!(rules.contains("user_correction_id="));

    ok(temp.path(), &["kpt", "close", "1"]);
    let dismiss_args = [
        "kpt",
        "item",
        "dismiss",
        "8",
        "--authority",
        authority_id,
        "--reason",
        "no action required",
        "--expected-current",
        manual_handle,
    ];
    let dismissed = ok(temp.path(), &dismiss_args);
    assert!(dismissed.contains("dismissed kpt item"));
    assert!(dismissed.contains("dismissal.source: none"));
    assert!(dismissed.contains("dismissal.review_status: closed"));
    let existing = ok(temp.path(), &dismiss_args);
    assert!(existing.contains("dismissal already exists"));
    assert_eq!(
        dismissal_projection(&dismissed),
        dismissal_projection(&existing)
    );
    let settled_closed = ok(temp.path(), &["gate", "close-ready", "--dry-run"]);
    assert!(settled_closed.contains("corrections_kpt_checked: pass"));
    assert!(settled_closed.contains("0 unsettled KPT items"));

    let changed = aw(
        temp.path(),
        &[
            "kpt",
            "item",
            "dismiss",
            "8",
            "--authority",
            authority_id,
            "--reason",
            "different reason",
            "--expected-current",
            manual_handle,
        ],
    );
    assert!(!changed.status.success());
    let after_changed = ok(temp.path(), &["kpt", "item", "list", "--review", "1"]);
    assert_eq!(
        dismissal_projection(&dismissed),
        dismissal_projection(&after_changed)
    );

    ok(
        temp.path(),
        &[
            "correction",
            "add",
            "--scope",
            "project",
            "--type",
            "process",
            "--pattern",
            "imported-pattern",
            "--correction",
            "imported-change",
        ],
    );
    ok(
        temp.path(),
        &[
            "kpt",
            "start",
            "--scope",
            "project",
            "--from",
            "corrections",
            "--summary",
            "imported",
        ],
    );
    let imported_items = ok(temp.path(), &["kpt", "item", "list", "--review", "2"]);
    let (imported_item, imported_handle) =
        kpt_item_by_title(&imported_items, "Repeated correction: imported-pattern");
    let imported_unsettled = ok(temp.path(), &["gate", "close-ready", "--dry-run"]);
    assert!(imported_unsettled.contains("corrections_kpt_checked: fail"));
    let imported_dismissal = ok(
        temp.path(),
        &[
            "kpt",
            "item",
            "dismiss",
            imported_item,
            "--authority",
            authority_id,
            "--reason",
            "source reviewed",
            "--expected-current",
            imported_handle,
        ],
    );
    assert!(imported_dismissal.contains("dismissal.source: exact(correction,"));
    let (converted_correction_item, converted_correction_handle) =
        kpt_item_by_title(&imported_items, "Repeated correction: item-2");
    let second_imported_dismissal = ok(
        temp.path(),
        &[
            "kpt",
            "item",
            "dismiss",
            converted_correction_item,
            "--authority",
            authority_id,
            "--reason",
            "converted correction source reviewed",
            "--expected-current",
            converted_correction_handle,
        ],
    );
    assert!(second_imported_dismissal.contains("dismissal.source: exact(correction,"));
    ok(temp.path(), &["kpt", "close", "2"]);
    let imported_after_close = ok(temp.path(), &["kpt", "item", "list", "--review", "2"]);
    for receipt in [&imported_dismissal, &second_imported_dismissal] {
        for line in dismissal_projection(receipt) {
            assert!(
                imported_after_close
                    .lines()
                    .any(|candidate| candidate == line)
            );
        }
    }
    let imported_settled = ok(temp.path(), &["gate", "close-ready", "--dry-run"]);
    assert!(imported_settled.contains("corrections_kpt_checked: pass"));
}

#[test]
fn fixed_kpt_command_conversion_resolves_only_one_open_review_and_item() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let authority = ok(
        temp.path(),
        &[
            "authority",
            "event",
            "add",
            "--type",
            "user_instruction",
            "--summary",
            "fix validation command",
            "--scope",
            "project",
        ],
    );
    let authority_id = cli_value(&authority, "authority_event_id");
    let missing = aw(
        temp.path(),
        &[
            "kpt",
            "item",
            "convert",
            "--to",
            "command-profile",
            "--command-status",
            "fixed",
            "--command",
            "cargo test",
            "--authority",
            authority_id,
        ],
    );
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("exactly one open KPT review"));

    ok(temp.path(), &["kpt", "start", "--scope", "project"]);
    ok(
        temp.path(),
        &[
            "kpt",
            "item",
            "add",
            "--type",
            "try",
            "--title",
            "stable tests",
            "--details",
            "cargo test",
        ],
    );
    let converted = ok(
        temp.path(),
        &[
            "kpt",
            "item",
            "convert",
            "--to",
            "command-profile",
            "--command-status",
            "fixed",
            "--name",
            "stable-tests",
            "--authority",
            authority_id,
        ],
    );
    assert!(converted.contains("command_profile_id:"));
}

#[test]
fn concurrent_kpt_settlement_commits_one_public_outcome() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let authority = ok(
        temp.path(),
        &[
            "authority",
            "event",
            "add",
            "--type",
            "user_instruction",
            "--summary",
            "settle KPT observations",
            "--scope",
            "project",
        ],
    );
    let authority_id = cli_value(&authority, "authority_event_id").to_string();

    ok(temp.path(), &["kpt", "start", "--scope", "project"]);
    ok(
        temp.path(),
        &[
            "kpt",
            "item",
            "add",
            "--type",
            "try",
            "--title",
            "same request race",
        ],
    );
    let listed = ok(temp.path(), &["kpt", "item", "list", "--review", "1"]);
    let handle = kpt_item_handle(&listed, 1).to_string();
    let run_dismiss = |root: std::path::PathBuf| {
        let authority_id = authority_id.clone();
        let handle = handle.clone();
        std::thread::spawn(move || {
            aw(
                &root,
                &[
                    "kpt",
                    "item",
                    "dismiss",
                    "1",
                    "--authority",
                    &authority_id,
                    "--reason",
                    "same public decision",
                    "--expected-current",
                    &handle,
                ],
            )
        })
    };
    let first = run_dismiss(temp.path().to_path_buf());
    let second = run_dismiss(temp.path().to_path_buf());
    let first = first.join().unwrap();
    let second = second.join().unwrap();
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
    let first = String::from_utf8(first.stdout).unwrap();
    let second = String::from_utf8(second.stdout).unwrap();
    assert_ne!(
        first.contains("dismissed kpt item"),
        second.contains("dismissed kpt item")
    );
    assert_eq!(dismissal_projection(&first), dismissal_projection(&second));
    ok(temp.path(), &["kpt", "close", "1"]);
    let after_close = ok(temp.path(), &["kpt", "item", "list", "--review", "1"]);
    assert_eq!(
        dismissal_projection(&first),
        dismissal_projection(&after_close)
    );

    ok(temp.path(), &["kpt", "start", "--scope", "project"]);
    ok(
        temp.path(),
        &[
            "kpt",
            "item",
            "add",
            "--type",
            "try",
            "--title",
            "competing outcomes",
        ],
    );
    let listed = ok(temp.path(), &["kpt", "item", "list", "--review", "2"]);
    let handle = kpt_item_handle(&listed, 2).to_string();
    let convert_root = temp.path().to_path_buf();
    let convert = std::thread::spawn(move || {
        aw(
            &convert_root,
            &["kpt", "item", "convert", "--item", "2", "--to", "rule"],
        )
    });
    let dismiss_root = temp.path().to_path_buf();
    let dismiss_authority = authority_id.clone();
    let dismiss = std::thread::spawn(move || {
        aw(
            &dismiss_root,
            &[
                "kpt",
                "item",
                "dismiss",
                "2",
                "--authority",
                &dismiss_authority,
                "--reason",
                "choose no action",
                "--expected-current",
                &handle,
            ],
        )
    });
    let convert = convert.join().unwrap();
    let dismiss = dismiss.join().unwrap();
    assert_ne!(convert.status.success(), dismiss.status.success());
    let final_state = ok(temp.path(), &["kpt", "item", "list", "--review", "2"]);
    assert_eq!(
        final_state.contains("try:converted"),
        convert.status.success()
    );
    assert_eq!(
        final_state.contains("try:dismissed"),
        dismiss.status.success()
    );
}

fn kpt_item_handle(output: &str, item_id: i64) -> &str {
    let marker = format!("{item_id} [review=");
    let mut lines = output.lines();
    while let Some(line) = lines.next() {
        if line.starts_with(&marker) {
            return lines
                .next()
                .and_then(|line| line.strip_prefix("current: "))
                .expect("KPT item current handle must follow its row");
        }
    }
    panic!("KPT item {item_id} missing from output")
}

fn kpt_item_by_title<'a>(output: &'a str, title: &str) -> (&'a str, &'a str) {
    let mut lines = output.lines();
    while let Some(line) = lines.next() {
        if line.ends_with(title) {
            let id = line.split_whitespace().next().unwrap();
            let handle = lines
                .next()
                .and_then(|line| line.strip_prefix("current: "))
                .expect("KPT item current handle must follow its row");
            return (id, handle);
        }
    }
    panic!("KPT item titled {title:?} missing from output:\n{output}")
}

fn dismissal_projection(output: &str) -> Vec<&str> {
    output
        .lines()
        .filter(|line| line.starts_with("dismissal."))
        .collect()
}

fn conversion_projection(output: &str) -> Vec<&str> {
    output.lines().skip(1).collect()
}

#[test]
fn remediation_cli_exposes_ready_supersede_disposition_and_typed_result() {
    let temp = tempfile::tempdir().unwrap();
    let closure_help = ok(temp.path(), &["closure", "--help"]);
    assert!(closure_help.contains("ready"));
    assert!(closure_help.contains("supersede"));

    let finding_help = ok(temp.path(), &["finding", "--help"]);
    assert!(finding_help.contains("accept-out-of-scope"));

    let run_help = ok(temp.path(), &["review", "run", "add", "--help"]);
    assert!(run_help.contains("--finding-result"));

    let context_help = ok(temp.path(), &["review-context", "--help"]);
    assert!(context_help.contains("--finding"));
    assert!(context_help.contains("--closure"));
    assert!(context_help.contains("--attempt"));
}

#[test]
fn remediation_cli_keeps_the_same_canonical_finding_after_unblock() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "public ordered remediation", None).unwrap();
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            review_type: "implementation_review",
            required: true,
            stage: "close-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    let run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("work_unit:1"),
            prompt_deviations: None,
            result_summary: Some("two ordered findings"),
            new_findings_count: 2,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
            review_provenance: "self_recorded",
            review_provenance_ref: None,
        },
    )
    .unwrap();
    let first = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: run.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "first public remediation",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    let second = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: run.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "second public remediation",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), first.finding_id, "valid").unwrap();
    classify_finding(temp.path(), second.finding_id, "valid").unwrap();
    let closure = |finding_id, invariant: &'static str| NewClosure {
        finding_id,
        design_invariant: invariant,
        design_citations: None,
        implementation_evidence: None,
        affected_surfaces: Some("src/review.rs"),
        same_invariant_search: None,
        other_violations_found: None,
        fix_plan: Some("repair the selected implementation surface"),
        tests_or_gates: Some("cargo test"),
        verification_plan: Some("independent verification"),
        closed_by_commit: None,
    };
    let first_closure = add_closure(
        temp.path(),
        closure(first.finding_id, "first public invariant"),
    )
    .unwrap();
    add_closure(
        temp.path(),
        closure(second.finding_id, "second public invariant"),
    )
    .unwrap();

    ok(
        temp.path(),
        &[
            "work",
            "remediate",
            "--finding",
            &first.finding_id.to_string(),
        ],
    );
    let authority = add_authority_event(
        temp.path(),
        NewAuthorityEvent {
            event_type: "user_instruction",
            source: Some("test-owner"),
            summary: "replace the first public remediation contract",
            scope: Some("work-unit:1"),
            precedence: 100,
        },
    )
    .unwrap();
    let replacement = supersede_closure(
        temp.path(),
        ClosureSupersession {
            closure_id: first_closure.closure_id,
            new_closure: closure(first.finding_id, "replacement public invariant"),
            reason: "replace the first public contract",
            authority_event_id: authority.authority_event_id,
        },
    )
    .unwrap();
    ok(
        temp.path(),
        &[
            "work",
            "remediate",
            "--finding",
            &first.finding_id.to_string(),
        ],
    );
    ok(
        temp.path(),
        &[
            "work",
            "block",
            &work.work_unit_id.to_string(),
            "--reason",
            "pause public remediation",
        ],
    );
    let blocked = ok(temp.path(), &["status", "--work", "1"]);
    assert!(blocked.contains(&format!(
        "work unblock 1 --reason \"<reason>\"; then agent-workbench work remediate --finding {}",
        first.finding_id
    )));

    ok(
        temp.path(),
        &[
            "work",
            "unblock",
            &work.work_unit_id.to_string(),
            "--reason",
            "continue public remediation",
        ],
    );
    let status = ok(temp.path(), &["status", "--work", "1"]);
    assert!(status.contains(&format!("finding_id: {}", first.finding_id)));
    assert!(status.contains(&format!("closure ready {}", replacement.closure_id)));
    assert!(!status.contains(&format!("closure ready {}", first_closure.closure_id)));
}

#[test]
fn review_and_owner_decisions_have_simple_help() {
    let temp = tempfile::tempdir().unwrap();
    for (path, needles) in [
        (
            &["review", "run", "add", "--help"][..],
            &["--plan", "--type", "--purpose", "--provenance-ref"][..],
        ),
        (
            &["review", "adjudicate", "--help"][..],
            &["--decision", "--reason", "--expected-current"][..],
        ),
        (
            &["finding", "decide", "--help"][..],
            &["--decision", "--reason", "--expected-current"][..],
        ),
        (
            &["verification", "adjudicate", "--help"][..],
            &["--run", "--finding", "--closure", "--attempt"][..],
        ),
    ] {
        let output = ok(temp.path(), path);
        for needle in needles {
            assert!(output.contains(needle), "{path:?} missing {needle}");
        }
        assert!(!output.contains("--principal"));
        assert!(!output.contains("--capability"));
    }
    for retired in [
        &["authority", "assertion", "--help"][..],
        &["owner", "grant", "--help"][..],
        &["principal", "resolve", "--help"][..],
    ] {
        assert!(!aw(temp.path(), retired).status.success());
    }
    for route in [
        &["review", "provenance", "issue", "--help"][..],
        &["review", "invocation", "request", "--help"][..],
        &["review", "invocation", "start", "--help"][..],
        &["review", "invocation", "complete", "--help"][..],
        &["review", "result", "stage", "--help"][..],
        &["review", "result", "finding-add", "--help"][..],
        &["review", "result", "complete", "--help"][..],
    ] {
        let output = ok(temp.path(), route);
        assert!(!output.contains("--principal"));
        assert!(!output.contains("--capability"));
    }
}

#[test]
fn finding_classification_is_durable_and_review_requiredness_preserves_presence() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "classification owner", None).unwrap();
    let work_id = work.work_unit_id.to_string();

    ok(
        temp.path(),
        &[
            "review",
            "plan",
            "add",
            "--work-unit",
            &work_id,
            "--type",
            "general",
            "--stage",
            "implementation-ready",
        ],
    );
    ok(
        temp.path(),
        &[
            "review",
            "plan",
            "add",
            "--work-unit",
            &work_id,
            "--type",
            "general",
            "--stage",
            "close-ready",
            "--required",
        ],
    );
    let plans = ok(temp.path(), &["review", "plan", "list"]);
    assert!(
        plans
            .lines()
            .any(|line| line.contains("required=false")
                && line.ends_with("stage=implementation-ready")),
        "{plans}"
    );
    assert!(
        plans
            .lines()
            .any(|line| line.contains("required=true") && line.ends_with("stage=close-ready")),
        "{plans}"
    );

    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            review_type: "general",
            required: false,
            stage: "resume-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    let run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: None,
            prompt_deviations: None,
            result_summary: Some("one finding"),
            new_findings_count: 1,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: Some("classification-reviewer"),
            external_agent_id: Some("classification-reviewer-1"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:classification"),
        },
    )
    .unwrap();
    let finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: run.review_run_id,
            finding_type: "process_finding",
            severity: "high",
            description: "classification must be durable",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    let finding_id = finding.finding_id.to_string();
    let classify = [
        "finding",
        "classify",
        &finding_id,
        "--classification",
        "valid",
    ];
    let classified = ok(temp.path(), &classify);
    assert!(classified.contains("classification_result: classified"));
    assert!(classified.contains("classification: valid"));
    assert!(classified.contains("status: open"));
    let replayed = ok(temp.path(), &classify);
    assert!(replayed.contains("classification_result: existing"));

    let forbidden = aw(
        temp.path(),
        &[
            "finding",
            "classify",
            &finding_id,
            "--classification",
            "invalid",
        ],
    );
    assert!(!forbidden.status.success());
    assert!(
        String::from_utf8_lossy(&forbidden.stderr)
            .contains("code: classification_change_forbidden")
    );
    let unknown = aw(
        temp.path(),
        &[
            "finding",
            "classify",
            &finding_id,
            "--classification",
            "invented",
        ],
    );
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("code: classification_unknown"));

    let other_run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: 1,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: None,
            prompt_deviations: None,
            result_summary: Some("other finding"),
            new_findings_count: 1,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: Some("other-reviewer"),
            external_agent_id: Some("other-reviewer-1"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:other"),
        },
    )
    .unwrap();
    let other_finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: other_run.review_run_id,
            finding_type: "process_finding",
            severity: "medium",
            description: "finding from another run",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    let decided = ok(
        temp.path(),
        &[
            "finding",
            "classify",
            &other_finding.finding_id.to_string(),
            "--decision",
            "rejected",
            "--reason",
            "owner rejected the advisory finding",
            "--expected-current",
            "pending",
        ],
    );
    assert!(decided.contains("decision_handle: decision_"));
    let filtered = ok(
        temp.path(),
        &["finding", "list", "--run", &run.review_run_id.to_string()],
    );
    assert!(filtered.contains("classification must be durable"));
    assert!(!filtered.contains("finding from another run"));
}

#[test]
fn generic_decision_adjudication_is_the_same_review_owner_operation() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "generic decision adapter", None).unwrap();
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            review_type: "general",
            required: true,
            stage: "close-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    let run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: None,
            prompt_deviations: None,
            result_summary: Some("clean generic decision target"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: Some("generic-decision-reviewer"),
            external_agent_id: Some("generic-decision-reviewer-1"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:generic-decision"),
        },
    )
    .unwrap();
    let run_id = run.review_run_id.to_string();
    let generic = ok(
        temp.path(),
        &[
            "decision",
            "adjudicate",
            "--owner",
            "review",
            "--target",
            &run_id,
            "--decision",
            "accepted",
            "--reason",
            "accept the clean review",
            "--expected-current",
            "pending",
        ],
    );
    let specialized = ok(
        temp.path(),
        &[
            "review",
            "adjudicate",
            &run_id,
            "--decision",
            "accepted",
            "--reason",
            "accept the clean review",
            "--expected-current",
            "pending",
        ],
    );
    assert_eq!(
        cli_value(&generic, "decision_handle"),
        cli_value(&specialized, "decision_handle")
    );
    assert!(
        ok(temp.path(), &["review", "plan", "list"]).contains(&format!(
            "{} [general:clean required=true]",
            plan.review_plan_id
        ))
    );

    let invalid_owner = aw(
        temp.path(),
        &[
            "decision",
            "adjudicate",
            "--owner",
            "project",
            "--target",
            &run_id,
            "--decision",
            "accepted",
            "--reason",
            "must reject unknown owner",
            "--expected-current",
            "pending",
        ],
    );
    assert!(!invalid_owner.status.success());
}

#[test]
fn decision_continuation_uses_the_owner_revision_and_supersedes_on_drift() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "decision continuation", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "decision-continuation-policy",
            review_type: "general",
            max_fresh_agents: 4,
            max_resume_agents: 1,
            max_parallel_agents: 4,
            required_consecutive_clean_fresh_runs: 3,
            required_consecutive_clean_resume_runs: 0,
            stop_on_severity: "none",
            allow_resume_review: true,
            allow_fresh_review: true,
            allow_new_findings_in_resume: true,
            on_max_agents_exceeded: "block",
            run_count_scope: "review_plan",
            default_run_mode: "fresh",
        },
    )
    .unwrap();
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            review_type: "general",
            required: true,
            stage: "close-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: Some(policy.review_policy_id),
            review_scope_id: None,
        },
    )
    .unwrap();
    let add_run = |label: &str| {
        add_review_run(
            temp.path(),
            NewReviewRun {
                review_plan_id: plan.review_plan_id,
                run_type: "fresh",
                run_purpose: "new_unbiased_review",
                target_ref: None,
                prompt_deviations: None,
                result_summary: Some("continuation review claim"),
                new_findings_count: 0,
                carried_findings_checked: 0,
                clean_run: true,
                status: "completed",
                agent_label: Some(label),
                external_agent_id: Some(label),
                review_provenance: "external_agent",
                review_provenance_ref: Some("review-output:continuation"),
            },
        )
        .unwrap()
    };

    let run = add_run("continuation-reviewer-1");
    let continuation = add_decision_continuation(
        temp.path(),
        NewDecisionContinuation {
            command_kind: "decision adjudicate",
            owner_ref: &format!("work_unit:{}", work.work_unit_id),
            target_ref: &format!("review_run:{}", run.review_run_id),
            decision_family: "review",
            action: "adjudicate",
            expected_current: "pending",
            rejection_code: "accountable_input_required",
        },
    )
    .unwrap();
    let shown = ok(
        temp.path(),
        &[
            "decision",
            "continuation",
            "show",
            &continuation.continuation_handle,
        ],
    );
    assert!(shown.contains("status: pending"));
    assert!(shown.contains("required_input: decision,reason"));
    assert!(!shown.contains("design_version"));

    let applied = ok(
        temp.path(),
        &[
            "decision",
            "continuation",
            "apply",
            &continuation.continuation_handle,
            "--decision",
            "accepted",
            "--reason",
            "accept the independent review",
            "--expected-current",
            "pending",
        ],
    );
    assert!(applied.contains("status: applied"));
    assert!(applied.contains("idempotent: false"));
    let replay = ok(
        temp.path(),
        &[
            "decision",
            "continuation",
            "apply",
            &continuation.continuation_handle,
            "--decision",
            "accepted",
            "--reason",
            "accept the independent review",
            "--expected-current",
            "pending",
        ],
    );
    assert_eq!(
        cli_value(&applied, "decision_handle"),
        cli_value(&replay, "decision_handle")
    );
    assert!(replay.contains("idempotent: true"));

    let drift_run = add_run("continuation-reviewer-2");
    let drifted = add_decision_continuation(
        temp.path(),
        NewDecisionContinuation {
            command_kind: "decision adjudicate",
            owner_ref: &format!("work_unit:{}", work.work_unit_id),
            target_ref: &format!("review_run:{}", drift_run.review_run_id),
            decision_family: "review",
            action: "adjudicate",
            expected_current: "pending",
            rejection_code: "accountable_input_required",
        },
    )
    .unwrap();
    let drift_run_id = drift_run.review_run_id.to_string();
    ok(
        temp.path(),
        &[
            "decision",
            "adjudicate",
            "--owner",
            "review",
            "--target",
            &drift_run_id,
            "--decision",
            "needs_evidence",
            "--reason",
            "record newer owner state",
            "--expected-current",
            "pending",
        ],
    );
    let superseded = ok(
        temp.path(),
        &[
            "decision",
            "continuation",
            "apply",
            &drifted.continuation_handle,
            "--decision",
            "accepted",
            "--reason",
            "must not overwrite the newer owner state",
            "--expected-current",
            "pending",
        ],
    );
    assert!(superseded.contains("status: superseded"));
    let successor = cli_value(&superseded, "successor_continuation");
    let successor_show = ok(
        temp.path(),
        &["decision", "continuation", "show", successor],
    );
    assert!(successor_show.contains("status: pending"));
    assert!(successor_show.contains("code: owner_revision_changed"));
    assert!(!successor_show.contains("expected_current: pending"));
}

#[test]
fn external_review_orchestration_is_project_local_atomic_and_replayable() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "external review lifecycle", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "external-review",
            review_type: "implementation_review",
            max_fresh_agents: 6,
            max_resume_agents: 2,
            max_parallel_agents: 6,
            required_consecutive_clean_fresh_runs: 1,
            required_consecutive_clean_resume_runs: 0,
            stop_on_severity: "none",
            allow_resume_review: true,
            allow_fresh_review: true,
            allow_new_findings_in_resume: false,
            on_max_agents_exceeded: "block",
            run_count_scope: "review_plan",
            default_run_mode: "fresh",
        },
    )
    .unwrap();
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            review_type: "implementation_review",
            required: true,
            stage: "close-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: Some(policy.review_policy_id),
            review_scope_id: None,
        },
    )
    .unwrap();
    let target = "review-context:external-lifecycle";

    let provenance = ok(
        temp.path(),
        &[
            "review",
            "provenance",
            "issue",
            "--reviewer",
            "external-reviewer-1",
            "--plan",
            &plan.review_plan_id.to_string(),
            "--target",
            target,
            "--purpose",
            "new_unbiased_review",
            "--reference",
            "agent-output:one",
            "--idempotency-key",
            "provenance-one",
        ],
    );
    let provenance_replay = ok(
        temp.path(),
        &[
            "review",
            "provenance",
            "issue",
            "--reviewer",
            "external-reviewer-1",
            "--plan",
            &plan.review_plan_id.to_string(),
            "--target",
            target,
            "--purpose",
            "new_unbiased_review",
            "--reference",
            "agent-output:one",
            "--idempotency-key",
            "provenance-one",
        ],
    );
    assert_eq!(
        cli_value(&provenance, "provenance_handle"),
        cli_value(&provenance_replay, "provenance_handle")
    );
    assert!(provenance_replay.contains("already_recorded: true"));

    let invocation_args = [
        "review",
        "invocation",
        "request",
        "--plan",
        &plan.review_plan_id.to_string(),
        "--target",
        target,
        "--reviewer",
        "external-reviewer-1",
        "--provenance",
        cli_value(&provenance, "provenance_handle"),
        "--purpose",
        "new_unbiased_review",
        "--idempotency-key",
        "invocation-one",
    ];
    let invocation = ok(temp.path(), &invocation_args);
    let invocation_replay = ok(temp.path(), &invocation_args);
    assert_eq!(
        cli_value(&invocation, "invocation_handle"),
        cli_value(&invocation_replay, "invocation_handle")
    );
    assert!(invocation_replay.contains("already_applied: true"));
    let invocation_id = cli_value(&invocation, "invocation_id");

    let start_args = [
        "review",
        "invocation",
        "start",
        invocation_id,
        "--expected-current",
        "requested",
        "--idempotency-key",
        "start-one",
    ];
    let started = ok(temp.path(), &start_args);
    let started_replay = ok(temp.path(), &start_args);
    assert!(started.contains("invocation_state: running"));
    assert!(started_replay.contains("already_applied: true"));

    let stage = ok(
        temp.path(),
        &[
            "review",
            "result",
            "stage",
            invocation_id,
            "--expected-current",
            "running",
            "--idempotency-key",
            "stage-one",
        ],
    );
    let finding = ok(
        temp.path(),
        &[
            "review",
            "result",
            "finding-add",
            cli_value(&stage, "stage_handle"),
            "--type",
            "implementation_finding",
            "--severity",
            "high",
            "--description",
            "atomic external finding",
            "--expected-current",
            cli_value(&stage, "version_handle"),
            "--idempotency-key",
            "finding-one",
        ],
    );
    let completed_args = [
        "review",
        "result",
        "complete",
        cli_value(&stage, "stage_handle"),
        "--expected-findings",
        "1",
        "--summary",
        "one external finding",
        "--expected-current",
        cli_value(&finding, "version_handle"),
        "--invocation-current",
        "running",
        "--idempotency-key",
        "complete-stage-one",
    ];
    let completed = ok(temp.path(), &completed_args);
    let completed_replay = ok(temp.path(), &completed_args);
    assert!(completed.contains("stage_state: completed"));
    assert_eq!(
        cli_value(&completed, "result_handle"),
        cli_value(&completed_replay, "result_handle")
    );
    assert!(completed_replay.contains("already_applied: true"));
    let findings = list_findings(temp.path(), Some("open")).unwrap();
    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].description, "atomic external finding");

    let clean_invocation = request_external_invocation(
        temp.path(),
        plan.review_plan_id,
        target,
        "external-reviewer-clean",
        "clean",
    );
    let clean_args = [
        "review",
        "invocation",
        "complete",
        cli_value(&clean_invocation, "invocation_id"),
        "--claim",
        "clean",
        "--summary",
        "independent clean review",
        "--expected-current",
        "requested",
        "--idempotency-key",
        "complete-clean",
    ];
    let clean = ok(temp.path(), &clean_args);
    let clean_replay = ok(temp.path(), &clean_args);
    assert!(clean.contains("invocation_state: completed"));
    assert!(clean.contains("review_run_id:"));
    assert!(clean_replay.contains("already_applied: true"));

    let cancelled_stage_invocation = request_external_invocation(
        temp.path(),
        plan.review_plan_id,
        target,
        "external-reviewer-stage-cancel",
        "stage-cancel",
    );
    let cancelled_stage = ok(
        temp.path(),
        &[
            "review",
            "result",
            "stage",
            cli_value(&cancelled_stage_invocation, "invocation_id"),
            "--expected-current",
            "requested",
            "--idempotency-key",
            "stage-cancel",
        ],
    );
    let cancel_stage_args = [
        "review",
        "result",
        "cancel",
        cli_value(&cancelled_stage, "stage_handle"),
        "--reason",
        "discard incomplete findings",
        "--expected-current",
        cli_value(&cancelled_stage, "version_handle"),
        "--idempotency-key",
        "cancel-stage",
    ];
    let stage_cancelled = ok(temp.path(), &cancel_stage_args);
    let stage_cancelled_replay = ok(temp.path(), &cancel_stage_args);
    assert!(stage_cancelled.contains("stage_state: cancelled"));
    assert!(stage_cancelled_replay.contains("already_applied: true"));

    for (reviewer, terminal, expected, reason) in [
        (
            "external-reviewer-fail",
            "fail",
            "failed",
            "review process failed",
        ),
        (
            "external-reviewer-cancel",
            "cancel",
            "cancelled",
            "review request cancelled",
        ),
    ] {
        let key = format!("provenance-{terminal}");
        let provenance = ok(
            temp.path(),
            &[
                "review",
                "provenance",
                "issue",
                "--reviewer",
                reviewer,
                "--plan",
                &plan.review_plan_id.to_string(),
                "--target",
                target,
                "--purpose",
                "new_unbiased_review",
                "--reference",
                reviewer,
                "--idempotency-key",
                &key,
            ],
        );
        let invocation = ok(
            temp.path(),
            &[
                "review",
                "invocation",
                "request",
                "--plan",
                &plan.review_plan_id.to_string(),
                "--target",
                target,
                "--reviewer",
                reviewer,
                "--provenance",
                cli_value(&provenance, "provenance_handle"),
                "--purpose",
                "new_unbiased_review",
                "--idempotency-key",
                &format!("invocation-{terminal}"),
            ],
        );
        let args = [
            "review",
            "invocation",
            terminal,
            cli_value(&invocation, "invocation_id"),
            "--reason",
            reason,
            "--expected-current",
            "requested",
            "--idempotency-key",
            &format!("terminal-{terminal}"),
        ];
        let ended = ok(temp.path(), &args);
        let replayed = ok(temp.path(), &args);
        assert!(ended.contains(&format!("invocation_state: {expected}")));
        assert!(replayed.contains("already_applied: true"));
    }
}

fn cli_value<'a>(output: &'a str, key: &str) -> &'a str {
    output
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}: ")))
        .unwrap_or_else(|| panic!("missing {key} in:\n{output}"))
}

fn request_external_invocation(
    root: &std::path::Path,
    plan: i64,
    target: &str,
    reviewer: &str,
    suffix: &str,
) -> String {
    let plan = plan.to_string();
    let provenance_key = format!("provenance-{suffix}");
    let provenance = ok(
        root,
        &[
            "review",
            "provenance",
            "issue",
            "--reviewer",
            reviewer,
            "--plan",
            &plan,
            "--target",
            target,
            "--purpose",
            "new_unbiased_review",
            "--reference",
            reviewer,
            "--idempotency-key",
            &provenance_key,
        ],
    );
    let invocation_key = format!("invocation-{suffix}");
    ok(
        root,
        &[
            "review",
            "invocation",
            "request",
            "--plan",
            &plan,
            "--target",
            target,
            "--reviewer",
            reviewer,
            "--provenance",
            cli_value(&provenance, "provenance_handle"),
            "--purpose",
            "new_unbiased_review",
            "--idempotency-key",
            &invocation_key,
        ],
    )
}

#[test]
fn acceptance_success_confirmations_do_not_expose_target_ids() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "publication boundary", None).unwrap();
    let task = add_task(
        temp.path(),
        NewTask {
            work_unit_id: Some(work.work_unit_id),
            title: "accepted task",
            priority: "medium",
            source: "user",
            details: None,
            completion_condition: Some("accepted"),
        },
    )
    .unwrap();
    let task_output = ok(
        temp.path(),
        &[
            "task",
            "accept-out-of-scope",
            &task.task_id.to_string(),
            "--reason",
            "approved",
        ],
    );
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "publication-review",
            review_type: "implementation_review",
            max_fresh_agents: 2,
            max_resume_agents: 1,
            max_parallel_agents: 1,
            required_consecutive_clean_fresh_runs: 1,
            required_consecutive_clean_resume_runs: 0,
            stop_on_severity: "none",
            allow_resume_review: true,
            allow_fresh_review: true,
            allow_new_findings_in_resume: false,
            on_max_agents_exceeded: "block",
            run_count_scope: "review_plan",
            default_run_mode: "fresh",
        },
    )
    .unwrap();
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            review_type: "implementation_review",
            required: true,
            stage: "close-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: Some(policy.review_policy_id),
            review_scope_id: None,
        },
    )
    .unwrap();
    let run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("publication-boundary"),
            prompt_deviations: None,
            result_summary: Some("one finding"),
            new_findings_count: 1,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
            review_provenance: "self_recorded",
            review_provenance_ref: None,
        },
    )
    .unwrap();
    let finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: run.review_run_id,
            finding_type: "implementation_finding",
            severity: "high",
            description: "accepted finding",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), finding.finding_id, "valid").unwrap();
    let authority = add_authority_event(
        temp.path(),
        NewAuthorityEvent {
            event_type: "user_instruction",
            source: Some("test-user"),
            summary: "approve acceptance",
            scope: Some("publication boundary"),
            precedence: 100,
        },
    )
    .unwrap();

    let finding_output = ok(
        temp.path(),
        &[
            "finding",
            "accept-out-of-scope",
            &finding.finding_id.to_string(),
            "--reason",
            "approved",
            "--authority",
            &authority.authority_event_id.to_string(),
        ],
    );
    let plan_output = ok(
        temp.path(),
        &[
            "review",
            "plan",
            "waive",
            &plan.review_plan_id.to_string(),
            "--reason",
            "approved",
        ],
    );

    assert_eq!(task_output, "accepted task out of scope\n");
    assert_eq!(finding_output, "accepted finding out of scope\n");
    assert_eq!(plan_output, "waived review plan\n");
}

#[test]
fn finding_recover_cli_publishes_and_replays_the_exact_successor() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "CLI terminal recovery", None).unwrap();
    let package = init_design_package(
        temp.path(),
        NewDesignPackage {
            design_id: "cli-terminal-recovery",
            title: "CLI Terminal Recovery",
        },
    )
    .unwrap();
    let source_design = import_design_package(
        temp.path(),
        DesignPackageImport {
            package_path: &package.package_path,
            status: "draft",
        },
    )
    .unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "cli-terminal-recovery-review",
            review_type: "design_review",
            max_fresh_agents: 2,
            max_resume_agents: 2,
            max_parallel_agents: 1,
            required_consecutive_clean_fresh_runs: 1,
            required_consecutive_clean_resume_runs: 0,
            stop_on_severity: "none",
            allow_resume_review: true,
            allow_fresh_review: true,
            allow_new_findings_in_resume: false,
            on_max_agents_exceeded: "block",
            run_count_scope: "review_plan",
            default_run_mode: "fresh",
        },
    )
    .unwrap();
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(source_design.design_version_id),
            review_type: "design_review",
            required: true,
            stage: "design-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: Some(policy.review_policy_id),
            review_scope_id: None,
        },
    )
    .unwrap();
    let context = format!(
        "review-context:design-review:design={}:work={}",
        source_design.design_version_id, work.work_unit_id
    );
    let run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some(&context),
            prompt_deviations: None,
            result_summary: Some("successor publication missing"),
            new_findings_count: 1,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: Some("source-reviewer"),
            external_agent_id: Some("source-reviewer"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("source-review-output"),
        },
    )
    .unwrap();
    let finding = add_finding(
        temp.path(),
        NewFinding {
            review_run_id: run.review_run_id,
            finding_type: "design_finding",
            severity: "critical",
            description: "publish corrected design successor",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), finding.finding_id, "valid").unwrap();
    let closure = add_closure(
        temp.path(),
        NewClosure {
            finding_id: finding.finding_id,
            design_invariant: "corrected design has one successor",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some("design:edit:01-introduction-goals.md"),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("correct the design package"),
            tests_or_gates: Some("design validation"),
            verification_plan: Some("fresh verification"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    begin_correction(temp.path(), closure.closure_id).unwrap();
    let edited = package.package_path.join("01-introduction-goals.md");
    let mut content = std::fs::read_to_string(&edited).unwrap();
    content.push_str("\nCorrected through the public recovery boundary.\n");
    std::fs::write(&edited, content).unwrap();
    let attempt = ready_closure(
        temp.path(),
        ClosureReady {
            closure_id: closure.closure_id,
            implementation_evidence: "corrected package",
            tests_or_gates: "design validation passed",
            closed_by_commit: None,
        },
    )
    .unwrap();
    let verification = add_review_run_with_finding_result(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "finding_fix_verification",
            target_ref: Some(&attempt.context_ref),
            prompt_deviations: None,
            result_summary: Some("correction verified"),
            new_findings_count: 0,
            carried_findings_checked: 1,
            clean_run: true,
            status: "completed",
            agent_label: Some("verification-reviewer"),
            external_agent_id: Some("verification-reviewer"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("verification-output"),
        },
        Some("verified"),
    )
    .unwrap();
    let verification_output = ok(
        temp.path(),
        &[
            "finding",
            "verify",
            "--run",
            &verification.review_run_id.to_string(),
            "--finding",
            &finding.finding_id.to_string(),
            "--closure",
            &closure.closure_id.to_string(),
            "--attempt",
            &attempt.attempt_id.to_string(),
            "--result",
            "verified",
        ],
    );
    assert!(verification_output.contains("added finding verification"));
    let terminal = adjudicate_verification(
        temp.path(),
        verification.review_run_id,
        finding.finding_id,
        closure.closure_id,
        attempt.attempt_id,
        AdjudicationInput {
            decision: "accepted",
            reason: "accept verified correction",
            expected_current: "pending",
        },
    )
    .unwrap();
    let authority = add_authority_event(
        temp.path(),
        NewAuthorityEvent {
            event_type: "user_instruction",
            source: Some("test-owner"),
            summary: "recover terminal publication",
            scope: Some("project"),
            precedence: 100,
        },
    )
    .unwrap();
    let finding_id = finding.finding_id.to_string();
    let authority_id = authority.authority_event_id.to_string();
    let args = [
        "finding",
        "recover",
        &finding_id,
        "--epoch",
        "1",
        "--evidence",
        "corrected package requires successor publication",
        "--authority",
        &authority_id,
        "--reason",
        "publish corrected successor",
        "--package-current",
        &source_design.content_hash,
        "--expected-current",
        &terminal.decision_handle,
        "--idempotency-key",
        "cli-terminal-recovery-1",
    ];
    let mut invalid_args = args;
    invalid_args[2] = "0";
    let invalid = aw(temp.path(), &invalid_args);
    assert!(!invalid.status.success());
    assert!(
        String::from_utf8_lossy(&invalid.stderr)
            .contains("next: agent-workbench finding recover --help")
    );

    let recovered = ok(temp.path(), &args);
    assert!(recovered.contains("recovered terminal design finding"));
    assert!(recovered.contains("idempotent: false"));
    assert!(recovered.contains("converged: false"));
    assert!(recovered.contains("corrected_design_ref: revision_"));
    assert!(recovered.contains("next: agent-workbench review plan add"));
    let replayed = ok(temp.path(), &args);
    assert!(replayed.contains("idempotent: true"));
    assert!(replayed.contains("converged: false"));
    assert_eq!(
        cli_value(&recovered, "recovery_handle"),
        cli_value(&replayed, "recovery_handle")
    );
    assert_eq!(
        cli_value(&recovered, "corrected_design_version_id"),
        cli_value(&replayed, "corrected_design_version_id")
    );
    let mut competing_args = args;
    competing_args[16] = "cli-terminal-recovery-competing-key";
    let competing = ok(temp.path(), &competing_args);
    assert!(competing.contains("idempotent: false"));
    assert!(competing.contains("converged: true"));
    assert_eq!(
        cli_value(&recovered, "recovery_handle"),
        cli_value(&competing, "recovery_handle")
    );
    assert_eq!(
        cli_value(&recovered, "corrected_design_version_id"),
        cli_value(&competing, "corrected_design_version_id")
    );
    let corrected_design_ref = cli_value(&recovered, "corrected_design_ref");
    let corrected_design_version_id = cli_value(&recovered, "corrected_design_version_id");
    let inspected = ok(temp.path(), &["design", "inspect", corrected_design_ref]);
    assert_eq!(
        cli_value(&inspected, "design_version_id"),
        corrected_design_version_id
    );
    let work_unit_id = work.work_unit_id.to_string();
    let context = ok(
        temp.path(),
        &[
            "review-context",
            "design-review",
            "--design-version",
            corrected_design_ref,
            "--work-unit",
            &work_unit_id,
        ],
    );
    assert_eq!(
        cli_value(&context, "design_version_id"),
        corrected_design_version_id
    );
    let corrected_bytes = std::fs::read(&edited).unwrap();
    let mut drifted = corrected_bytes.clone();
    drifted.extend_from_slice(b"\nchanged after recovery\n");
    std::fs::write(&edited, drifted).unwrap();
    let rejected_replay = aw(temp.path(), &args);
    assert!(!rejected_replay.status.success());
    let rejected_replay = String::from_utf8_lossy(&rejected_replay.stderr);
    assert!(rejected_replay.contains("recovery_postconditions_changed"));
    assert!(rejected_replay.contains("next: restore the exact corrected file state"));
    std::fs::write(&edited, corrected_bytes).unwrap();
}
