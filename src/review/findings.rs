use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, params};

use crate::db::{current_phase_blocker, open_existing_project, project_id};

use super::{closure::*, correction_contract::*, evaluation::*, *};

pub fn add_finding(root: &Path, input: NewFinding<'_>) -> Result<FindingOutcome> {
    let targets = if input.design_requirement_id.is_some() || input.task_id.is_some() {
        vec![FindingTargetInput {
            design_requirement_id: input.design_requirement_id,
            task_id: input.task_id,
        }]
    } else {
        Vec::new()
    };
    add_finding_with_targets(root, input, &targets)
}

pub fn add_finding_with_targets(
    root: &Path,
    input: NewFinding<'_>,
    targets: &[FindingTargetInput],
) -> Result<FindingOutcome> {
    for (index, target) in targets.iter().enumerate() {
        if target.design_requirement_id.is_none() && target.task_id.is_none() {
            bail!("finding target {} is empty", index + 1);
        }
        if targets[..index].contains(target) {
            bail!("finding targets must be unique");
        }
    }
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let (run, declared_findings, actual_findings, run_status) = tx
        .query_row(
            r#"
            select r.run_type, p.review_policy_id, p.review_type, r.clean_run,
                   r.new_findings_count,
                   (select count(*) from findings existing where existing.review_run_id=r.id),
                   r.status
            from review_runs r
            join review_plans p on p.id = r.review_plan_id
            where r.id = ?1 and r.project_id = ?2
            "#,
            params![input.review_run_id, project_id],
            |row| {
                Ok((
                    StoredReviewRunPolicy {
                        run_type: row.get(0)?,
                        review_policy_id: row.get(1)?,
                        review_type: row.get(2)?,
                        clean_run: row.get::<_, i64>(3)? == 1,
                    },
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .optional()?
        .context("review run not found")?;
    ensure_finding_type_matches_review_type(input.finding_type, &run.review_type)?;
    if run.clean_run {
        bail!("cannot add finding to a clean review run");
    }
    let policy = load_review_policy(&tx, project_id, run.review_policy_id)?;
    if run.run_type == "resume" && !policy.allow_new_findings_in_resume {
        bail!("new findings are disabled for resume review by policy");
    }
    if declared_findings <= 0 {
        bail!(
            "review run declares no findings; publish findings through the staged review result lifecycle"
        );
    }
    if actual_findings >= declared_findings {
        bail!(
            "review finding inventory would exceed declared new_findings_count {declared_findings}"
        );
    }
    if !matches!(run_status.as_str(), "requested" | "running") {
        bail!(
            "review finding inventory is not staging; publish findings through review result stage, finding-add, and complete"
        );
    }
    if run_status == "requested" {
        let started = tx.execute(
            "update review_runs set status='running' where id=?1 and project_id=?2 and status='requested'",
            params![input.review_run_id, project_id],
        )?;
        if started != 1 {
            bail!("review finding inventory start lost");
        }
        let invocations = tx.execute(
            "update review_agent_invocations set status='running',started_at=current_timestamp where project_id=?1 and review_run_id=?2 and status='requested'",
            params![project_id, input.review_run_id],
        )?;
        if invocations != 1 {
            bail!("review finding inventory requires one requested compatibility invocation");
        }
    }
    let first_target = targets.first().copied();
    tx.execute(
        r#"
        insert into findings(
            project_id, review_run_id, finding_type, severity, description,
            classification, status, design_requirement_id, task_id, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, 'unclassified', 'open', ?6, ?7, current_timestamp)
        "#,
        params![
            project_id,
            input.review_run_id,
            input.finding_type,
            input.severity,
            input.description,
            first_target.and_then(|target| target.design_requirement_id),
            first_target.and_then(|target| target.task_id),
        ],
    )?;
    let finding_id = tx.last_insert_rowid();
    for (index, target) in targets.iter().enumerate() {
        tx.execute(
            "insert into finding_targets(project_id,finding_id,ordinal,design_requirement_id,task_id,created_at) values(?1,?2,?3,?4,?5,current_timestamp)",
            params![
                project_id,
                finding_id,
                i64::try_from(index + 1)?,
                target.design_requirement_id,
                target.task_id
            ],
        )?;
    }
    if actual_findings + 1 == declared_findings {
        let completed = tx.execute(
            "update review_runs set status='completed' where id=?1 and project_id=?2 and status='running'",
            params![input.review_run_id, project_id],
        )?;
        if completed != 1 {
            bail!("review finding inventory completion lost");
        }
        tx.execute(
            "update review_agent_invocations set status='completed',finished_at=current_timestamp where project_id=?1 and review_run_id=?2 and status='running'",
            params![project_id, input.review_run_id],
        )?;
    }
    tx.execute(
        "insert into finding_target_seals(finding_id,project_id,target_count,created_at) values(?1,?2,?3,current_timestamp)",
        params![finding_id, project_id, i64::try_from(targets.len())?],
    )?;
    refresh_plan_for_run(&tx, project_id, input.review_run_id)?;
    tx.commit()?;
    Ok(FindingOutcome { finding_id })
}

pub fn classify_finding(
    root: &Path,
    finding_id: i64,
    classification: &str,
) -> Result<FindingClassificationOutcome> {
    if !matches!(
        classification,
        "unclassified" | "invalid" | "valid" | "design_conflict" | "needs_evidence"
    ) {
        bail!(
            "code: classification_unknown\nsupplied: {classification}\nallowed: unclassified,invalid,valid,design_conflict,needs_evidence\nnext: agent-workbench finding classify --help"
        );
    }
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let (current_status, current_classification): (String, String) = tx
        .query_row(
            "select status,classification from findings where id = ?1 and project_id = ?2",
            params![finding_id, project_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .context("finding not found")?;
    ensure_review_finding_target(&tx, finding_id, "finding classify")?;
    if current_classification == classification {
        return Ok(FindingClassificationOutcome {
            finding_id,
            classification: current_classification,
            status: current_status,
            existing: true,
        });
    }
    if matches!(current_status.as_str(), "closed" | "accepted_out_of_scope") {
        bail!(
            "code: finding_terminal\nstatus: {current_status}\nnext: agent-workbench finding reopen --help"
        );
    }
    if current_classification == "valid" && classification != "valid" {
        bail!(
            "code: classification_change_forbidden\ncurrent: valid\nsupplied: {classification}\nnext: agent-workbench closure add --help"
        );
    }
    if let Some(blocker) = current_phase_blocker(&tx)? {
        let expected = format!("agent-workbench finding classify {finding_id}");
        if !blocker.next_action.starts_with(&expected) {
            bail!("code: finding_not_selected\nnext: {}", blocker.next_action);
        }
    }
    let status = match classification {
        "invalid" => "closed",
        "valid" => "open",
        "design_conflict" | "needs_evidence" | "unclassified" => "open",
        _ => unreachable!("the classification enum was checked before opening the project"),
    };
    let changed = tx.execute(
        r#"
        update findings
        set classification = ?1, status = ?2
        where id = ?3 and project_id = ?4
        "#,
        params![classification, status, finding_id, project_id],
    )?;
    if changed == 0 {
        bail!("finding not found");
    }
    let review_run_id: i64 = tx.query_row(
        "select review_run_id from findings where id = ?1",
        params![finding_id],
        |row| row.get(0),
    )?;
    refresh_plan_for_run(&tx, project_id, review_run_id)?;
    tx.commit()?;
    Ok(FindingClassificationOutcome {
        finding_id,
        classification: classification.to_string(),
        status: status.to_string(),
        existing: false,
    })
}

pub fn add_closure(root: &Path, input: NewClosure<'_>) -> Result<ClosureOutcome> {
    require_text(Some(input.design_invariant), "closure requires --invariant")?;
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let finding = tx
        .query_row(
            "select id, classification, status from findings where id = ?1 and project_id = ?2",
            params![input.finding_id, project_id],
            |row| {
                Ok(StoredFinding {
                    id: row.get(0)?,
                    classification: row.get(1)?,
                    status: row.get(2)?,
                })
            },
        )
        .optional()?
        .context("finding not found")?;
    ensure_review_finding_target(&tx, finding.id, "closure add")?;
    let blocker = current_phase_blocker(&tx)?;
    if finding.classification != "valid" {
        bail!("closure requires a valid finding");
    }
    if finding.status != "open" {
        bail!("finding is not open");
    }
    let current_exists: bool = tx.query_row(
        "select exists(select 1 from closures where finding_id = ?1 and status != 'superseded')",
        params![finding.id],
        |row| row.get(0),
    )?;
    if current_exists {
        bail!(
            "finding already has a current closure; use closure supersede when the contract must change"
        );
    }
    let surfaces = input.affected_surfaces.unwrap_or_default();
    let source_correction = declares_typed_correction(surfaces);
    let eligible =
        finding_is_remediation_eligible(&tx, project_id, finding.id)? && !source_correction;
    if let Some(blocker) = blocker {
        let expected = format!("agent-workbench closure add --finding {}", finding.id);
        let stale_source_recovery = !eligible && blocker.kind == "stale_design";
        if !blocker.next_action.starts_with(&expected) && !stale_source_recovery {
            bail!("closure add is not selected; next: {}", blocker.next_action);
        }
    }
    if eligible {
        require_text(
            input.affected_surfaces,
            "eligible closure requires --surfaces",
        )?;
        require_text(input.fix_plan, "eligible closure requires --fix-plan")?;
        require_text(input.tests_or_gates, "eligible closure requires --tests")?;
        require_text(
            input.verification_plan,
            "eligible closure requires --verification",
        )?;
    } else {
        require_text(
            input.affected_surfaces,
            "source correction closure requires --surfaces",
        )?;
        require_text(
            input.fix_plan,
            "source correction closure requires --fix-plan",
        )?;
        require_text(
            input.tests_or_gates,
            "source correction closure requires --tests",
        )?;
        require_text(
            input.verification_plan,
            "source correction closure requires --verification",
        )?;
        parse_correction_tokens(surfaces)?;
    }
    tx.execute(
        r#"
        insert into closures(
            project_id, finding_id, design_invariant, design_citations,
            implementation_evidence, affected_surfaces, same_invariant_search,
            other_violations_found, fix_plan, tests_or_gates,
            verification_plan, closed_by_commit, status, created_at
        )
        values (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 'registered', current_timestamp)
        "#,
        params![
            project_id,
            finding.id,
            input.design_invariant,
            input.design_citations,
            input.implementation_evidence,
            input.affected_surfaces,
            input.same_invariant_search,
            input.other_violations_found,
            input.fix_plan,
            input.tests_or_gates,
            input.verification_plan,
            input.closed_by_commit,
        ],
    )?;
    let closure_id = tx.last_insert_rowid();
    if source_correction || !eligible {
        let design_root = correction_design_root(&tx, finding.id)?;
        record_correction_tokens(
            &tx,
            root,
            project_id,
            closure_id,
            surfaces,
            design_root.as_deref(),
        )?;
    }
    tx.commit()?;
    Ok(ClosureOutcome { closure_id })
}

pub fn begin_correction(root: &Path, closure_id: i64) -> Result<CorrectionBeginOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let selected_active_closure: Option<i64> = tx
        .query_row(
            "select closure_id from correction_sessions where project_id = ?1 and status = 'active' order by id limit 1",
            params![project_id],
            |row| row.get(0),
        )
        .optional()?;
    if selected_active_closure.is_some_and(|selected| selected != closure_id) {
        bail!(
            "another source correction session is selected; finish closure {} first",
            selected_active_closure.unwrap()
        );
    }
    let blocker = current_phase_blocker(&tx)?;
    let (finding_id, surfaces, eligible, design_root): (i64, String, bool, Option<String>) = tx
        .query_row(
            r#"
            select c.finding_id, c.affected_surfaces,
                   p.required = 1 and p.stage = 'close-ready'
                     and p.review_type in ('implementation_review', 'design_implementation_diff')
                     and not exists(
                       select 1 from correction_tokens token where token.closure_id=c.id
                     ),
                   dp.root_path
            from closures c
            join findings f on f.id = c.finding_id
            join review_runs r on r.id = f.review_run_id
            join review_plans p on p.id = r.review_plan_id
            left join design_versions dv on dv.id = p.design_version_id
            left join design_packages dp on dp.id = dv.design_package_id
            where c.id = ?1 and c.project_id = ?2 and c.status = 'registered'
              and f.status = 'open' and f.classification = 'valid'
              and not exists(
                select 1 from acceptance_records accepted
                where accepted.finding_id=f.id and accepted.target_type='finding'
                  and accepted.status='approved'
                  and accepted.acceptance_type in (
                    'accepted_out_of_scope','explicit_exception','classified_failure'
                  )
              )
            "#,
            params![closure_id, project_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?
        .context("registered correction closure not found")?;
    if eligible {
        bail!("implementation findings use agent-workbench work remediate");
    }
    if let Some(blocker) = blocker {
        let expected = format!("agent-workbench closure correction-begin {closure_id}");
        if blocker.next_action != expected && blocker.kind != "stale_design" {
            bail!(
                "closure correction-begin is not the selected action; next: {}",
                blocker.next_action
            );
        }
    }
    if let Some(session_id) = tx
        .query_row(
            "select id from correction_sessions where closure_id = ?1 and status = 'active'",
            params![closure_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
    {
        let token_count = tx.query_row(
            "select count(*) from correction_tokens where closure_id = ?1",
            params![closure_id],
            |row| row.get(0),
        )?;
        return Ok(CorrectionBeginOutcome {
            closure_id,
            session_id,
            token_count,
            idempotent: true,
        });
    }
    let mut token_count: i64 = tx.query_row(
        "select count(*) from correction_tokens where closure_id = ?1",
        params![closure_id],
        |row| row.get(0),
    )?;
    if token_count == 0 {
        token_count = record_correction_tokens(
            &tx,
            root,
            project_id,
            closure_id,
            &surfaces,
            design_root.as_deref(),
        )?;
    } else {
        ensure_correction_prestate_unchanged(&tx, root, closure_id, design_root.as_deref())?;
    }
    validate_correction_transition_preflight(&tx, project_id, closure_id, finding_id)?;
    tx.execute(
        r#"
        insert into correction_sessions(project_id, finding_id, closure_id, status, created_at)
        values (?1, ?2, ?3, 'active', current_timestamp)
        "#,
        params![project_id, finding_id, closure_id],
    )?;
    let session_id = tx.last_insert_rowid();
    tx.commit()?;
    Ok(CorrectionBeginOutcome {
        closure_id,
        session_id,
        token_count,
        idempotent: false,
    })
}
