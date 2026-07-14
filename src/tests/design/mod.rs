use super::*;
use crate::db::open_existing_project;

mod acceptance;
mod carry_forward;
mod decomposition;
mod package;
mod phase;
mod readiness;
mod reconciliation;
mod stale;
mod trace;

fn add_clean_implementation_ready_review(
    root: &std::path::Path,
    work_unit_id: i64,
    design_version_id: i64,
) {
    let plan_id = add_implementation_ready_review_plan(root, work_unit_id, design_version_id);
    add_clean_review_run(
        root,
        plan_id,
        Some(&format!(
            "review-context:design-task-decomposition:design={design_version_id}:work={work_unit_id}"
        )),
        "clean decomposition review",
    );
}

fn add_implementation_ready_review_plan(
    root: &std::path::Path,
    work_unit_id: i64,
    design_version_id: i64,
) -> i64 {
    let plan = add_review_plan(
        root,
        NewReviewPlan {
            work_unit_id,
            design_version_id: Some(design_version_id),
            review_type: "design_task_decomposition",
            required: true,
            stage: "implementation-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    plan.review_plan_id
}

fn add_close_ready_review_plans(
    root: &std::path::Path,
    work_unit_id: i64,
    design_version_id: i64,
) -> Vec<(i64, &'static str)> {
    let mut plans = Vec::new();
    for review_type in ["design_implementation_diff", "implementation_review"] {
        let plan = add_review_plan(
            root,
            NewReviewPlan {
                work_unit_id,
                design_version_id: Some(design_version_id),
                review_type,
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
        plans.push((plan.review_plan_id, review_type));
    }
    plans
}

fn add_clean_close_ready_review_runs(
    root: &std::path::Path,
    work_unit_id: i64,
    design_version_id: i64,
    plans: &[(i64, &'static str)],
    use_context: bool,
) -> Vec<anyhow::Result<ReviewRunOutcome>> {
    let mut results = Vec::new();
    for (review_plan_id, review_type) in plans {
        let context_kind = match *review_type {
            "design_implementation_diff" => "design-implementation-diff",
            "implementation_review" => "implementation-review",
            _ => unreachable!(),
        };
        let target_ref = use_context.then(|| {
            format!("review-context:{context_kind}:design={design_version_id}:work={work_unit_id}")
        });
        let result = add_clean_review_run_result(
            root,
            *review_plan_id,
            target_ref.as_deref(),
            "clean close review",
        );
        results.push(result);
    }
    results
}

fn add_clean_review_run(
    root: &std::path::Path,
    review_plan_id: i64,
    target_ref: Option<&str>,
    summary: &str,
) -> i64 {
    add_clean_review_run_result(root, review_plan_id, target_ref, summary)
        .unwrap()
        .review_run_id
}

fn add_clean_review_run_result(
    root: &std::path::Path,
    review_plan_id: i64,
    target_ref: Option<&str>,
    summary: &str,
) -> anyhow::Result<ReviewRunOutcome> {
    add_review_run(
        root,
        NewReviewRun {
            review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref,
            prompt_deviations: None,
            result_summary: Some(summary),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: Some("test-reviewer"),
            external_agent_id: Some("test-reviewer-1"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("test-reviewer-output"),
        },
    )
}

fn reconciliation_state_snapshot(conn: &rusqlite::Connection, work_unit_id: i64) -> String {
    conn.query_row(
        r#"select
          coalesce((select group_concat(id||':'||status,'|') from tasks where work_unit_id=?1),'')||';'||
          coalesce((select group_concat(id||':'||phase_id||':'||task_id||':'||assigned_at,'|') from work_phase_task_memberships),'')||';'||
          coalesce((select group_concat(id||':'||status,'|') from checklists),'')||';'||
          coalesce((select group_concat(id||':'||status,'|') from task_derivations),'')||';'||
          coalesce((select group_concat(id||':'||status,'|') from checklist_items),'')||';'||
          coalesce((select group_concat(id||':'||status,'|') from validation_gates),'')||';'||
          coalesce((select group_concat(id||':'||status,'|') from coverage_items),'')||';'||
          coalesce((select group_concat(id||':'||status||':'||coalesce(closed_at,''),'|') from work_phases),'')||';'||
          coalesce((select group_concat(id||':'||phase_id||':'||event_type||':'||created_at,'|') from work_phase_events),'')||';'||
          (select count(*) from correction_transition_applications)||':'||
          (select count(*) from correction_transition_aliases)||':'||
          (select count(*) from correction_application_identity_links)||':'||
          (select count(*) from correction_completion_inheritance_sources)||':'||
          (select count(*) from correction_completion_inheritance_evidence)"#,
        params![work_unit_id],
        |row| row.get(0),
    )
    .unwrap()
}
