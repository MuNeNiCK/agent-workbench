use super::*;

#[test]
fn review_run_rejects_invalid_type_purpose_pairs() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "type purpose pairs", None).unwrap();
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

    let fresh_fix = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "finding_fix_verification",
            target_ref: None,
            prompt_deviations: None,
            result_summary: None,
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
            review_provenance: "self_recorded",
            review_provenance_ref: None,
        },
    );
    let resume_unbiased = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "resume",
            run_purpose: "new_unbiased_review",
            target_ref: None,
            prompt_deviations: None,
            result_summary: None,
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
            review_provenance: "self_recorded",
            review_provenance_ref: None,
        },
    );

    assert!(fresh_fix.is_err());
    assert!(resume_unbiased.is_err());
}
