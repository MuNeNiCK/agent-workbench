use super::*;

#[test]
fn superseded_source_correction_inherits_only_contiguous_file_preconditions() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "superseded source correction", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "superseded-source-correction",
            review_type: "design_review",
            max_fresh_agents: 1,
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
    let run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("HEAD"),
            prompt_deviations: None,
            result_summary: Some("source correction required"),
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
            finding_type: "design_finding",
            severity: "high",
            description: "typed source correction must expand without no-op edits",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    classify_finding(temp.path(), finding.finding_id, "valid").unwrap();
    let authority = approval_authority_event(temp.path());
    std::fs::create_dir_all(temp.path().join("docs")).unwrap();
    std::fs::write(temp.path().join("docs/a.md"), "original a").unwrap();
    std::fs::write(temp.path().join("docs/b.md"), "original b").unwrap();
    std::fs::write(temp.path().join("docs/c.md"), "original c").unwrap();

    let first = add_closure(
        temp.path(),
        NewClosure {
            finding_id: finding.finding_id,
            design_invariant: "typed source correction preserves its original precondition",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some("docs:edit:docs/a.md,docs:edit:docs/b.md"),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("correct both declared documents"),
            tests_or_gates: Some("source correction lifecycle"),
            verification_plan: Some("verify the exact corrected files"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    begin_correction(temp.path(), first.closure_id).unwrap();
    std::fs::write(temp.path().join("docs/a.md"), "corrected a once").unwrap();
    std::fs::write(temp.path().join("docs/b.md"), "corrected b").unwrap();

    let second = supersede_closure(
        temp.path(),
        ClosureSupersession {
            closure_id: first.closure_id,
            new_closure: NewClosure {
                finding_id: finding.finding_id,
                design_invariant: "the narrowed contract keeps the continuous b correction",
                design_citations: None,
                implementation_evidence: None,
                affected_surfaces: Some("docs:edit:docs/b.md"),
                same_invariant_search: None,
                other_violations_found: None,
                fix_plan: Some("retain the corrected b document"),
                tests_or_gates: Some("source correction lifecycle"),
                verification_plan: Some("verify the continuous b correction"),
                closed_by_commit: None,
            },
            reason: "temporarily narrow the typed correction contract",
            authority_event_id: authority,
        },
    )
    .unwrap();
    begin_correction(temp.path(), second.closure_id).unwrap();
    let third = supersede_closure(
        temp.path(),
        ClosureSupersession {
            closure_id: second.closure_id,
            new_closure: NewClosure {
                finding_id: finding.finding_id,
                design_invariant:
                    "continuous tokens inherit their baseline while reintroduced tokens restart",
                design_citations: None,
                implementation_evidence: None,
                affected_surfaces: Some(
                    "docs:edit:docs/a.md,docs:edit:docs/b.md,docs:edit:docs/c.md",
                ),
                same_invariant_search: None,
                other_violations_found: None,
                fix_plan: Some("correct the reintroduced a document without re-editing b"),
                tests_or_gates: Some("source correction lifecycle"),
                verification_plan: Some("verify contiguous and restarted preconditions"),
                closed_by_commit: None,
            },
            reason: "reintroduce a with a new correction boundary",
            authority_event_id: authority,
        },
    )
    .unwrap();
    begin_correction(temp.path(), third.closure_id).unwrap();
    let unchanged = ready_closure(
        temp.path(),
        ClosureReady {
            closure_id: third.closure_id,
            implementation_evidence: "continuous correction applied",
            tests_or_gates: "source correction lifecycle passed",
            closed_by_commit: None,
        },
    )
    .unwrap_err()
    .to_string();
    assert!(unchanged.contains("docs/a.md"), "{unchanged}");
    assert!(unchanged.contains("docs/c.md"), "{unchanged}");

    std::fs::write(temp.path().join("docs/a.md"), "corrected a twice").unwrap();
    std::fs::write(temp.path().join("docs/c.md"), "corrected c").unwrap();
    ready_closure(
        temp.path(),
        ClosureReady {
            closure_id: third.closure_id,
            implementation_evidence: "continuous and reintroduced corrections applied",
            tests_or_gates: "source correction lifecycle passed",
            closed_by_commit: None,
        },
    )
    .unwrap();
}
