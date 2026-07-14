use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use super::status::*;

pub(crate) fn ensure_unscoped_mutation_allowed(conn: &Connection, operation: &str) -> Result<()> {
    if let Some(blocker) = current_phase_blocker(conn)? {
        bail!(
            "{operation} is blocked by the selected lifecycle action; next: {}",
            blocker.next_action
        );
    }
    let active_source_correction: bool = conn.query_row(
        "select exists(select 1 from correction_sessions where status='active')",
        [],
        |row| row.get(0),
    )?;
    if active_source_correction {
        bail!("{operation} must be applied through closure transition apply");
    }
    Ok(())
}

pub(super) struct FindingActionState<'a> {
    pub(super) finding_id: i64,
    pub(super) review_plan_id: i64,
    pub(super) closure_id: Option<i64>,
    pub(super) closure_status: Option<&'a str>,
    pub(super) attempt_id: Option<i64>,
    pub(super) verification: Option<(i64, Option<&'a str>)>,
    pub(super) classification: &'a str,
    pub(super) implementation_eligible: bool,
    pub(super) work_unit_id: i64,
    pub(super) work_status: &'a str,
    pub(super) plan_status: &'a str,
    pub(super) plan_required: bool,
    pub(super) plan_accepted: bool,
}

pub(super) fn finding_next_action(state: FindingActionState<'_>) -> String {
    let FindingActionState {
        finding_id,
        review_plan_id,
        closure_id,
        closure_status,
        attempt_id,
        verification,
        classification,
        implementation_eligible,
        work_unit_id,
        work_status,
        plan_status,
        plan_required,
        plan_accepted,
    } = state;
    if classification != "valid" {
        return format!(
            "agent-workbench finding classify {finding_id} --classification valid|invalid|design_conflict|needs_evidence"
        );
    }
    if matches!(plan_status, "exhausted" | "needs_user_decision") {
        return format!(
            "agent-workbench authority event add --type user_instruction --summary \"review plan decision\" --scope \"review-plan:{review_plan_id}\"; then agent-workbench review plan waive {review_plan_id} --reason \"<reason>\" --authority <authority-event-id>"
        );
    }
    if !plan_required || plan_accepted {
        return format!(
            "agent-workbench authority event add --type user_instruction --summary \"dispose finding on non-required review plan\" --scope \"finding:{finding_id}\"; then agent-workbench finding accept-out-of-scope {finding_id} --reason \"<reason>\" --authority <authority-event-id>"
        );
    }
    match classification {
        "valid" => match (closure_id, closure_status, attempt_id, verification) {
            (
                Some(closure_id),
                Some("ready_for_verification"),
                Some(_),
                Some((run_id, finding_result)),
            ) => {
                let result = finding_result.unwrap_or("<missing-finding-result>");
                format!(
                    "agent-workbench finding verify --run {run_id} --finding {finding_id} --closure {closure_id} --result {result}"
                )
            }
            (Some(closure_id), Some("ready_for_verification"), Some(attempt_id), None) => {
                let context = format!(
                    "review-context:finding-fix:finding={finding_id}:closure={closure_id}:attempt={attempt_id}"
                );
                format!(
                    "agent-workbench review-context finding-fix --finding {finding_id} --closure {closure_id} --attempt {attempt_id}; then agent-workbench review run add --plan {review_plan_id} --type resume --purpose finding_fix_verification --target {context} --finding-result verified|not_fixed|needs_evidence --carried-findings 1 --provenance external_agent --external-agent-id <id> --provenance-ref <ref>"
                )
            }
            (Some(_), Some("registered"), _, _)
                if implementation_eligible && work_status == "blocked" =>
            {
                format!(
                    "agent-workbench work unblock {work_unit_id} --reason \"<reason>\"; then agent-workbench work remediate --finding {finding_id}"
                )
            }
            (Some(_), Some("registered"), _, _)
                if implementation_eligible && matches!(work_status, "closed" | "abandoned") =>
            {
                format!(
                    "agent-workbench authority event add --type user_instruction --summary \"reopen remediation owner {work_unit_id} for finding {finding_id}\" --scope \"work-unit:{work_unit_id}\"; then agent-workbench work reopen {work_unit_id} --reason \"remediate finding {finding_id}\" --reason-type closure_invalid --authority <authority-event-id>; then agent-workbench work remediate --finding {finding_id}"
                )
            }
            (Some(_), Some("registered"), _, _) if implementation_eligible => {
                format!("agent-workbench work remediate --finding {finding_id}")
            }
            (Some(closure_id), Some("registered"), _, _) => {
                format!("agent-workbench closure correction-begin {closure_id}")
            }
            (Some(closure_id), Some("incomplete"), _, _) => format!(
                "agent-workbench closure supersede {closure_id} --invariant \"<invariant>\" --surfaces \"<surfaces>\" --fix-plan \"<plan>\" --tests \"<tests>\" --verification \"<plan>\" --reason \"<reason>\" --authority <authority-event-id>"
            ),
            (None, _, _, _) => format!(
                "agent-workbench closure add --finding {finding_id} --invariant \"<invariant>\" --surfaces \"<typed-surfaces>\" --fix-plan \"<fix-plan>\" --tests \"<tests-or-gates>\" --verification \"<verification-plan>\""
            ),
            _ => format!("resolve closure state for finding {finding_id}"),
        },
        _ => format!("resolve finding {finding_id}"),
    }
}

pub(super) fn remediation_dependency_action(
    conn: &Connection,
    work_unit_id: i64,
    finding_id: i64,
) -> Result<Option<String>> {
    let dependency: Option<(
        i64,
        i64,
        String,
        Option<String>,
    )> = conn.query_row(
        r#"
        select d.id, d.depends_on_work_unit_id, w.status,
               (select a.status from work_unit_activations a
                where a.work_unit_id=d.depends_on_work_unit_id
                  and a.status in ('active','suspended')
                order by case a.status when 'active' then 0 else 1 end, a.id desc limit 1)
        from work_unit_dependencies d
        join work_units w on w.id=d.depends_on_work_unit_id
        where d.work_unit_id=?1 and d.status='open'
          and d.dependency_type in ('blocks','invalidates_assumption','invalidates_closure')
          and w.status in ('open','blocked')
          and not exists (
            select 1 from finding_remediation_recovery_epochs epoch
            join work_unit_activations epoch_activation
              on epoch_activation.id=epoch.work_unit_activation_id and epoch_activation.status='active'
            where epoch.dependency_id=d.id and epoch.work_unit_id=d.work_unit_id
          )
        order by d.id limit 1
        "#,
        params![work_unit_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    ).optional()?;
    let Some((dependency_id, depends_on, dependent_status, activation_status)) = dependency else {
        return Ok(None);
    };
    if dependent_status == "blocked" {
        return Ok(Some(format!(
            "agent-workbench work unblock {depends_on} --reason \"resolve dependency {dependency_id} for remediation owner {work_unit_id}\""
        )));
    }
    if activation_status.as_deref() == Some("active") {
        return Ok(Some(format!(
            "agent-workbench gate close-ready; then agent-workbench work close --summary \"resolve dependency {dependency_id} for remediation owner {work_unit_id}\""
        )));
    }
    if dependent_status == "open" {
        return Ok(Some(format!(
            "agent-workbench work activate {depends_on} --reason \"resolve dependency {dependency_id} before remediation finding {finding_id}\""
        )));
    }
    Ok(None)
}
