use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::params;

use crate::db::{open_existing_project, project_id};
use crate::review_context::required_plans_missing_context_count;
use crate::rules::{RuleBindingInput, insert_rule_binding};

use super::{validation::*, *};

pub fn approve_design_version(
    root: &Path,
    input: DesignVersionApproval<'_>,
) -> Result<DesignVersionApprovalOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let version = stored_design_version(&tx, project_id, input.design_version_id)?
        .context("design version not found")?;
    if version.current_design_version_id != Some(version.design_version_id) {
        bail!("only the current design version can be approved");
    }
    if version.status == "approved" {
        bail!("design version is already approved");
    }

    let summary = input.summary.map(str::to_string).unwrap_or_else(|| {
        format!(
            "approved design version {} for {}",
            version.design_version_id, version.design_key
        )
    });
    let source = format!("design_version:{}", version.design_version_id);
    tx.execute(
        r#"
        insert into authority_events(
            project_id, event_type, source, text_or_summary, scope, precedence,
            status, created_at
        )
        values (?1, 'design_doc', ?2, ?3, ?4, ?5, 'active', current_timestamp)
        "#,
        params![project_id, source, summary, version.design_key, 90],
    )?;
    let authority_event_id = tx.last_insert_rowid();
    insert_rule_binding(
        &tx,
        RuleBindingInput {
            project_id,
            rule_source_type: "authority_event",
            authority_event_id: Some(authority_event_id),
            user_correction_id: None,
            command_profile_id: None,
            review_policy_id: None,
            review_plan_id: None,
            work_unit_id: None,
            validation_gate_id: None,
            acceptance_record_id: None,
            scope_type: "design_package",
            scope_key: Some(&version.design_key),
            precedence: 90,
        },
    )?;
    tx.execute(
        r#"
        update design_versions
        set status = 'approved',
            approved_by_authority_event_id = ?1,
            approved_at = current_timestamp
        where id = ?2
        "#,
        params![authority_event_id, version.design_version_id],
    )?;
    tx.execute(
        r#"
        update design_packages
        set status = 'approved', updated_at = current_timestamp
        where id = ?1
        "#,
        params![version.design_package_id],
    )?;
    tx.commit()?;

    Ok(DesignVersionApprovalOutcome {
        design_package_id: version.design_package_id,
        design_version_id: version.design_version_id,
        authority_event_id,
    })
}

pub fn design_ready(root: &Path, input: DesignReadyCheck) -> Result<DesignReadyOutcome> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let mut items = Vec::new();

    let version = match resolve_design_version_for_gate(&conn, project_id, input.design_version_id)?
    {
        Some(version) => {
            items.push(DesignReadyItem::pass("design_version_exists", None));
            version
        }
        None => {
            items.push(DesignReadyItem::fail(
                "design_version_exists",
                Some("import a design package first"),
            ));
            return Ok(DesignReadyOutcome::blocked(
                input.design_version_id,
                None,
                "no design version is available",
                items,
            ));
        }
    };

    if version.current_design_version_id == Some(version.design_version_id) {
        items.push(DesignReadyItem::pass("design_version_current", None));
    } else {
        items.push(DesignReadyItem::fail(
            "design_version_current",
            Some("import or select the current design version"),
        ));
    }

    let file_count: i64 = conn.query_row(
        "select count(*) from design_files where design_version_id = ?1",
        params![version.design_version_id],
        |row| row.get(0),
    )?;
    if file_count > 0 {
        items.push(DesignReadyItem::pass(
            "design_files_imported",
            Some(format!("{file_count} files")),
        ));
    } else {
        items.push(DesignReadyItem::fail(
            "design_files_imported",
            Some("imported design version has no files"),
        ));
    }

    let active_requirement_count: i64 = conn.query_row(
        "select count(*) from design_requirements where design_version_id = ?1 and status = 'active'",
        params![version.design_version_id],
        |row| row.get(0),
    )?;
    if active_requirement_count > 0 {
        items.push(DesignReadyItem::pass(
            "active_requirements_extracted",
            Some(format!("{active_requirement_count} requirements")),
        ));
    } else {
        items.push(DesignReadyItem::fail(
            "active_requirements_extracted",
            Some("add requirement records to requirements/*.md"),
        ));
    }

    let missing_validation_count: i64 = conn.query_row(
        r#"
        select count(*)
        from design_requirements r
        join design_versions v on v.id = r.design_version_id
        join design_packages p on p.id = v.design_package_id
        where r.design_version_id = ?1
          and r.status = 'active'
          and (r.validation_expectation is null or r.validation_expectation = '')
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'design_requirement'
              and ar.design_requirement_id = r.id
              and ar.status = 'approved'
              and ar.acceptance_type in ('explicit_exception', 'evidence_gap')
          )
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'design_requirement_key'
              and ar.design_package_key = p.design_key
              and ar.design_requirement_key = r.requirement_key
              and ar.status = 'approved'
              and ar.acceptance_type in ('explicit_exception', 'evidence_gap')
          )
        "#,
        params![version.design_version_id],
        |row| row.get(0),
    )?;
    if missing_validation_count == 0 {
        items.push(DesignReadyItem::pass(
            "requirement_validation_defined",
            None,
        ));
    } else {
        items.push(DesignReadyItem::fail(
            "requirement_validation_defined",
            Some("every active requirement needs validation metadata"),
        ));
    }

    let review_state = design_review_gate_state(&conn, project_id, version.design_version_id)?;
    if review_state.required_plan_count == 0 {
        items.push(DesignReadyItem::fail(
            "design_review_clean",
            Some("add a required design-ready design_review plan for this design version"),
        ));
    } else if review_state.incomplete_required_plan_count == 0
        && review_state.missing_context_run_count == 0
        && review_state.unresolved_finding_count == 0
    {
        items.push(DesignReadyItem::pass(
            "design_review_clean",
            Some(format!(
                "{} required plans, {} missing review-context runs, {} unresolved findings",
                review_state.required_plan_count,
                review_state.missing_context_run_count,
                review_state.unresolved_finding_count
            )),
        ));
    } else {
        items.push(DesignReadyItem::fail(
            "design_review_clean",
            Some(format!(
                "{} required plans, {} incomplete, {} missing review-context runs, {} unresolved findings",
                review_state.required_plan_count,
                review_state.incomplete_required_plan_count,
                review_state.missing_context_run_count,
                review_state.unresolved_finding_count
            )),
        ));
    }

    let result = if items.iter().all(|item| item.result == "pass") {
        "pass"
    } else {
        "blocked"
    };
    let blocking_reason = if result == "pass" {
        None
    } else {
        Some("design version is not ready".to_string())
    };
    Ok(DesignReadyOutcome {
        result: result.to_string(),
        blocking_reason,
        design_package_id: Some(version.design_package_id),
        design_version_id: Some(version.design_version_id),
        items,
    })
}

pub(super) fn design_review_gate_state(
    conn: &rusqlite::Connection,
    project_id: i64,
    design_version_id: i64,
) -> Result<ReviewGateState> {
    let required_plan_count = conn.query_row(
        r#"
        select count(*)
        from review_plans
        where project_id = ?1
          and design_version_id = ?2
          and stage = 'design-ready'
          and review_type = 'design_review'
          and required = 1
        "#,
        params![project_id, design_version_id],
        |row| row.get::<_, i64>(0),
    )?;
    let incomplete_required_plan_count = conn.query_row(
        r#"
        select count(*)
        from review_plans
        where project_id = ?1
          and design_version_id = ?2
          and stage = 'design-ready'
          and review_type = 'design_review'
          and required = 1
          and status != 'clean'
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'review_plan'
              and ar.review_plan_id = review_plans.id
              and ar.status = 'approved'
              and ar.acceptance_type in ('explicit_exception', 'stale_accepted')
          )
        "#,
        params![project_id, design_version_id],
        |row| row.get::<_, i64>(0),
    )?;
    let unresolved_finding_count = conn.query_row(
        r#"
        select count(*)
        from findings f
        join review_runs rr on rr.id = f.review_run_id
        join review_plans rp on rp.id = rr.review_plan_id
        where rp.project_id = ?1
          and rp.design_version_id = ?2
          and rp.stage = 'design-ready'
          and rp.review_type = 'design_review'
          and f.status not in ('closed', 'accepted_out_of_scope')
          and f.classification not in ('invalid')
          and not exists (
            select 1
            from acceptance_records ar
            where ar.target_type = 'finding'
              and ar.finding_id = f.id
              and ar.status = 'approved'
              and ar.acceptance_type in (
                'accepted_out_of_scope', 'explicit_exception', 'classified_failure'
              )
          )
        "#,
        params![project_id, design_version_id],
        |row| row.get::<_, i64>(0),
    )?;
    let missing_context_run_count = required_plans_missing_context_count(
        conn,
        project_id,
        "design-ready",
        "design_review",
        Some(design_version_id),
        None,
        "design-review",
    )?;
    Ok(ReviewGateState {
        required_plan_count,
        incomplete_required_plan_count,
        missing_context_run_count,
        unresolved_finding_count,
    })
}

#[derive(Default)]
pub(super) struct ReviewGateState {
    required_plan_count: i64,
    incomplete_required_plan_count: i64,
    missing_context_run_count: i64,
    unresolved_finding_count: i64,
}
