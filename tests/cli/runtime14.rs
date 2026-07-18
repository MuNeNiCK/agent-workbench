use super::*;

#[test]
fn schema14_ordinary_workflow_has_no_external_setup() {
    let temp = tempfile::tempdir().unwrap();
    let init = ok(temp.path(), &["init"]);
    assert!(init.contains("schema_version: 14"));
    assert!(ok(temp.path(), &["status"]).contains("next: work start"));

    ok(temp.path(), &["work", "start", "ordinary"]);
    ok(temp.path(), &["task", "add", "implement"]);
    ok(
        temp.path(),
        &[
            "phase",
            "create",
            "--work-unit",
            "1",
            "--key",
            "implementation",
            "--title",
            "Implementation",
            "--order",
            "1",
        ],
    );
    ok(temp.path(), &["phase", "assign", "1", "--task", "1"]);
    ok(
        temp.path(),
        &["work", "suspend", "--reason", "pause", "--next", "resume"],
    );
    assert!(ok(temp.path(), &["resume-check"]).contains("result: pass"));
    ok(temp.path(), &["work", "resume", "--check", "1"]);
    ok(temp.path(), &["task", "close", "1"]);
    ok(temp.path(), &["phase", "close", "1", "--summary", "done"]);
    assert!(ok(temp.path(), &["gate", "close-ready"]).contains("result: pass"));
    ok(temp.path(), &["work", "close", "--summary", "complete"]);
}

#[test]
fn blocked_review_plan_exit_is_identical_across_next_gate_and_mutation() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(temp.path(), &["work", "start", "review"]);
    ok(
        temp.path(),
        &[
            "phase",
            "create",
            "--work-unit",
            "1",
            "--key",
            "review",
            "--title",
            "Review",
            "--order",
            "1",
        ],
    );
    ok(
        temp.path(),
        &[
            "review",
            "policy",
            "add",
            "--name",
            "one",
            "--type",
            "phase",
            "--max-fresh-agents",
            "1",
        ],
    );
    for plan in ["1", "2"] {
        let _ = plan;
        ok(
            temp.path(),
            &[
                "review",
                "plan",
                "add",
                "--work-unit",
                "1",
                "--type",
                "phase",
                "--stage",
                "phase",
                "--policy",
                "1",
                "--phase",
                "1",
            ],
        );
    }
    ok(
        temp.path(),
        &[
            "review",
            "run",
            "add",
            "--plan",
            "2",
            "--type",
            "fresh",
            "--purpose",
            "scope-b",
            "--clean",
        ],
    );
    ok(
        temp.path(),
        &[
            "review",
            "decide",
            "1",
            "--plan",
            "2",
            "--decision",
            "accept",
            "--reason",
            "accept clean",
            "--expected-current",
            "none",
        ],
    );
    ok(
        temp.path(),
        &[
            "review",
            "run",
            "add",
            "--plan",
            "1",
            "--type",
            "fresh",
            "--purpose",
            "scope-a",
            "--finding-result",
            "changes-required",
        ],
    );
    ok(
        temp.path(),
        &[
            "review",
            "decide",
            "2",
            "--plan",
            "1",
            "--decision",
            "accept",
            "--reason",
            "accept result",
            "--expected-current",
            "none",
        ],
    );

    let next = ok(temp.path(), &["next"]);
    let gate = ok(temp.path(), &["phase", "close-ready", "1"]);
    let expected = "review plan waive 1 --expected-current decision:2 --reason <reason>";
    assert!(next.contains(expected));
    assert!(gate.contains(expected));
    ok(
        temp.path(),
        &[
            "review",
            "plan",
            "waive",
            "1",
            "--expected-current",
            "decision:2",
            "--reason",
            "clean sibling covers scope",
        ],
    );
    assert!(ok(temp.path(), &["phase", "close-ready", "1"]).contains("result: pass"));
}

#[test]
fn schema14_design_work_requires_trace_coverage_and_current_evidence() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let package = temp.path().join("design");
    std::fs::create_dir_all(package.join("requirements")).unwrap();
    std::fs::write(
        package.join("design.yaml"),
        "id: simple\ntitle: Simple\nformat: arc42\nversion: 1\nstatus: draft\narc42: {}\nrequirements: [requirements/main.md]\nvalidation: []\n",
    )
    .unwrap();
    std::fs::write(
        package.join("requirements/main.md"),
        "# Requirements\n\n## Main\n```yaml agent-workbench\ntype: requirement\nkey: REQ-001\npriority: critical\nstatus: active\n```\n\nImplement the main behavior.\n",
    )
    .unwrap();
    ok(
        temp.path(),
        &["design", "import", package.to_str().unwrap()],
    );
    ok(
        temp.path(),
        &["design", "approve", "1", "--summary", "approved"],
    );
    ok(
        temp.path(),
        &[
            "work",
            "start",
            "implementation",
            "--design-version",
            "1",
            "--implementation",
        ],
    );
    ok(temp.path(), &["task", "add", "implement"]);
    ok(
        temp.path(),
        &[
            "trace",
            "derive-task",
            "--design",
            "1",
            "--requirement",
            "REQ-001",
            "--task",
            "1",
        ],
    );
    ok(
        temp.path(),
        &["decompose", "design", "1", "--work-unit", "1"],
    );
    ok(
        temp.path(),
        &[
            "coverage",
            "add",
            "--design",
            "1",
            "--requirement",
            "REQ-001",
            "--task",
            "1",
            "--work-unit",
            "1",
            "--status",
            "covered",
            "--requirement-text",
            "implemented",
        ],
    );
    ok(temp.path(), &["task", "close", "1"]);
    ok(
        temp.path(),
        &[
            "evidence",
            "add",
            "--task",
            "1",
            "--type",
            "implementation",
            "--note",
            "current implementation",
        ],
    );
    ok(temp.path(), &["checklist", "item", "close", "1"]);
    ok(temp.path(), &["checklist", "close", "1"]);
    let gate = ok(temp.path(), &["gate", "close-ready"]);
    assert!(gate.contains("result: pass"), "{gate}");
}
