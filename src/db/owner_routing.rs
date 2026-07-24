use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use super::status::{current_finding_remediations, current_source_corrections};
use super::{
    FindingActionState, OwnerAction, PhaseBlocker, active_activation, finding_next_action,
    project_id, review_action_must_precede_correction,
};

pub(super) fn current_owner_actions(conn: &Connection) -> Result<Vec<OwnerAction>> {
    let project_id = project_id(conn)?;
    let remediations = current_finding_remediations(conn)?;
    let corrections = current_source_corrections(conn)?;
    let globally_selected = super::status::current_phase_blocker(conn)?;
    let active_owner = active_activation(conn)?.map(|activation| activation.work_unit_id);
    let mut stmt = conn.prepare(
        "select id, title, status from work_units where project_id=?1 and status in ('open','blocked') order by id",
    )?;
    let owners = stmt
        .query_map(params![project_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut actions = owners
        .into_iter()
        .map(|(owner_id, title, work_status)| {
            let review_blocker = owner_review_blocker(conn, owner_id)?;
            if !corrections.is_empty()
                && let Some(selected) = globally_selected.as_ref()
                && selected.kind != "source_correction"
            {
                return Ok(owner_action_from_blocker(
                    owner_id,
                    title,
                    selected.clone(),
                ));
            }
            if let Some(correction) = corrections.first() {
                return Ok(OwnerAction {
                    owner_type: "work_unit".to_string(),
                    owner_id,
                    owner_handle: None,
                    title,
                    state: "source_correction".to_string(),
                    schedulable: true,
                    blocker_kind: Some("required_review_finding".to_string()),
                    description: correction.description.clone(),
                    next_action: correction.next_action.clone(),
                    next_actions: vec![correction.next_action.clone()],
                });
            }
            let stale_blocker = owner_stale_blocker(conn, owner_id)?;
            if stale_blocker.as_ref().is_some_and(|blocker| {
                blocker
                    .next_action
                    .starts_with("agent-workbench closure transition apply")
            }) {
                return Ok(owner_action_from_blocker(
                    owner_id,
                    title,
                    stale_blocker.expect("owned stale transition blocker"),
                ));
            }
            if review_blocker.as_ref().is_some_and(|blocker| {
                blocker
                    .next_action
                    .starts_with("agent-workbench closure correction-begin")
            }) {
                return Ok(owner_action_from_blocker(
                    owner_id,
                    title,
                    review_blocker.expect("source correction review blocker"),
                ));
            }
            if let Some(blocker) = stale_blocker {
                return Ok(owner_action_from_blocker(owner_id, title, blocker));
            }
            if let Some(blocker) = owner_dependency_blocker(conn, owner_id)? {
                return Ok(owner_action_from_blocker(owner_id, title, blocker));
            }
            if review_blocker
                .as_ref()
                .is_some_and(|blocker| {
                    review_action_must_precede_correction(&blocker.next_action)
                })
            {
                return Ok(owner_action_from_blocker(
                    owner_id,
                    title,
                    review_blocker.expect("urgent review blocker"),
                ));
            }
            if let Some(action) =
                crate::phases::phase_review_lifecycle_action(conn, owner_id, None)?
            {
                return Ok(owner_action_from_blocker(
                    owner_id,
                    title,
                    PhaseBlocker {
                        kind: "required_phase_review".to_string(),
                        review_plan_id: Some(action.review_plan_id),
                        work_unit_id: Some(owner_id),
                        review_type: Some(action.review_type),
                        stage: Some(action.stage),
                        review_run_id: None,
                        finding_id: None,
                        severity: None,
                        classification: None,
                        description: format!(
                            "phase {} has an incomplete required review plan",
                            action.phase_id
                        ),
                        next_action: action.action,
                    },
                ));
            }
            if let Some(remediation) = remediations
                .iter()
                .find(|remediation| remediation.work_unit_id == owner_id)
            {
                return Ok(OwnerAction {
                    owner_type: "work_unit".to_string(),
                    owner_id,
                    owner_handle: None,
                    title,
                    state: "finding_remediation".to_string(),
                    schedulable: true,
                    blocker_kind: Some("required_review_finding".to_string()),
                    description: remediation.description.clone(),
                    next_action: remediation.next_action.clone(),
                    next_actions: vec![remediation.next_action.clone()],
                });
            }
            if let Some(blocker) = review_blocker {
                return Ok(owner_action_from_blocker(owner_id, title, blocker));
            }
            if let Some(action) = owner_decomposition_action(conn, owner_id, &title)? {
                return Ok(action);
            }
            if work_status == "blocked" {
                return Ok(OwnerAction {
                    owner_type: "work_unit".to_string(),
                    owner_id,
                    owner_handle: None,
                    title,
                    state: "blocked".to_string(),
                    schedulable: true,
                    blocker_kind: Some("blocked_work_unit".to_string()),
                    description: "this owner is explicitly blocked".to_string(),
                    next_action: format!(
                        "agent-workbench work unblock {owner_id} --reason \"<reason>\""
                    ),
                    next_actions: vec![format!(
                        "agent-workbench work unblock {owner_id} --reason \"<reason>\""
                    )],
                });
            }
            let activation_status = conn
                .query_row(
                    "select status from work_unit_activations where work_unit_id=?1 and status in ('active','suspended') order by id desc limit 1",
                    params![owner_id],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            let (state, description, next_action) = match activation_status.as_deref() {
                Some("active") => (
                    "active",
                    "owner holds the execution slot",
                    format!("continue work unit {owner_id}"),
                ),
                Some("suspended") if active_owner.is_some() => (
                    "schedulable",
                    "suspended owner is eligible after the current execution slot is released",
                    format!(
                        "suspend the active owner, then agent-workbench resume-check {owner_id} --maturity trace-aware"
                    ),
                ),
                Some("suspended") => (
                    "suspended",
                    "owner can be revalidated for resume",
                    format!(
                        "agent-workbench resume-check {owner_id} --maturity trace-aware"
                    ),
                ),
                _ if active_owner.is_some() => (
                    "schedulable",
                    "owner is eligible after the current execution slot is released",
                    format!(
                        "suspend the active owner, then agent-workbench work activate {owner_id}"
                    ),
                ),
                _ => (
                    "schedulable",
                    "owner is eligible for activation",
                    format!("agent-workbench work activate {owner_id}"),
                ),
            };
            Ok(OwnerAction {
                owner_type: "work_unit".to_string(),
                owner_id,
                owner_handle: None,
                title,
                state: state.to_string(),
                schedulable: true,
                blocker_kind: None,
                description: description.to_string(),
                next_actions: vec![next_action.clone()],
                next_action,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    for candidate in crate::release::active_release_candidates(conn, project_id)? {
        actions.push(OwnerAction {
            owner_type: "release_candidate".to_string(),
            owner_id: candidate.candidate_id,
            owner_handle: Some(candidate.candidate_handle),
            title: candidate.version,
            state: candidate.state,
            schedulable: true,
            blocker_kind: Some("release_candidate_action".to_string()),
            description: candidate.description,
            next_action: candidate.next_action.clone(),
            next_actions: vec![candidate.next_action],
        });
    }
    Ok(actions)
}

fn owner_decomposition_action(
    conn: &Connection,
    work_unit_id: i64,
    title: &str,
) -> Result<Option<OwnerAction>> {
    let Some(resolution) = crate::decomposition::resolve_decomposition_owner(conn, work_unit_id)?
    else {
        return Ok(None);
    };
    if !resolution.blocks_work {
        return Ok(None);
    }
    let next_action = resolution.actions[0].clone();
    Ok(Some(OwnerAction {
        owner_type: "work_unit".to_string(),
        owner_id: work_unit_id,
        owner_handle: None,
        title: title.to_string(),
        state: "obligation".to_string(),
        schedulable: resolution
            .actions
            .iter()
            .any(|action| action_is_schedulable(action)),
        blocker_kind: Some("decomposition_plan_incomplete".to_string()),
        description: resolution.issue.unwrap_or_else(|| {
            format!(
                "Decomposition Plan {} requires its {} lifecycle transition before it can authorize work",
                resolution.plan_id, resolution.status
            )
        }),
        next_action,
        next_actions: resolution.actions,
    }))
}

fn owner_action_from_blocker(owner_id: i64, title: String, blocker: PhaseBlocker) -> OwnerAction {
    let next_action = blocker.next_action;
    OwnerAction {
        owner_type: "work_unit".to_string(),
        owner_id,
        owner_handle: None,
        title,
        state: "obligation".to_string(),
        schedulable: action_is_schedulable(&next_action),
        blocker_kind: Some(blocker.kind),
        description: blocker.description,
        next_actions: vec![next_action.clone()],
        next_action,
    }
}

fn owner_stale_blocker(conn: &Connection, owner_id: i64) -> Result<Option<PhaseBlocker>> {
    let stale = conn
        .query_row(
            r#"
            select kind, record_id from (
              select 0 kind_rank, 'task_derivation' kind, td.id record_id, t.work_unit_id work_id
              from task_derivations td join tasks t on t.id=td.task_id
              where td.status='stale'
                and not exists(
                  select 1 from decomposition_reconciliation_tasks mapping
                  join decomposition_plans plan on plan.id=mapping.decomposition_plan_id
                  where mapping.source_task_id=td.task_id and plan.status='applied'
                )
                and not exists(
                  select 1
                  from task_derivations replacement
                  join design_requirements source_requirement
                    on source_requirement.id=td.design_requirement_id
                  join design_requirements replacement_requirement
                    on replacement_requirement.id=replacement.design_requirement_id
                  join design_versions source_version
                    on source_version.id=source_requirement.design_version_id
                  join design_versions replacement_version
                    on replacement_version.id=replacement_requirement.design_version_id
                  join design_packages package
                    on package.id=source_version.design_package_id
                   and package.id=replacement_version.design_package_id
                   and package.current_design_version_id=replacement_version.id
                  join tasks replacement_task on replacement_task.id=replacement.task_id
                  where replacement.project_id=td.project_id
                    and replacement.status!='stale'
                    and replacement_version.status='approved'
                    and replacement_requirement.requirement_key=source_requirement.requirement_key
                    and replacement_task.work_unit_id is t.work_unit_id
                )
              union all
              select 1, 'checklist', c.id, c.work_unit_id
              from checklists c where c.status='stale'
                and not exists(
                  select 1 from decomposition_plans plan
                  where plan.status='applied'
                    and exists(
                      select 1 from checklist_items item
                      join decomposition_reconciliation_checklist_items mapping
                        on mapping.source_checklist_item_id=item.id
                       and mapping.decomposition_plan_id=plan.id
                      where item.checklist_id=c.id
                    )
                    and not exists(
                      select 1 from checklist_items item
                      where item.checklist_id=c.id and not exists(
                        select 1 from decomposition_reconciliation_checklist_items mapping
                        where mapping.decomposition_plan_id=plan.id
                          and mapping.source_checklist_item_id=item.id
                      )
                    )
                )
              union all
              select 2, 'validation_gate', vg.id, coalesce(vg.work_unit_id,t.work_unit_id,0)
              from validation_gates vg left join tasks t on t.id=vg.task_id
              where vg.status='stale'
                and not exists(
                  select 1 from decomposition_reconciliation_gates mapping
                  join decomposition_plans plan on plan.id=mapping.decomposition_plan_id
                  where mapping.source_validation_gate_id=vg.id and plan.status='applied'
                )
                and not exists(
                  select 1
                  from validation_gates replacement
                  join design_requirements source_requirement
                    on source_requirement.id=vg.design_requirement_id
                  join design_requirements replacement_requirement
                    on replacement_requirement.id=replacement.design_requirement_id
                  join design_versions source_version
                    on source_version.id=source_requirement.design_version_id
                  join design_versions replacement_version
                    on replacement_version.id=replacement_requirement.design_version_id
                  join design_packages package
                    on package.id=source_version.design_package_id
                   and package.id=replacement_version.design_package_id
                   and package.current_design_version_id=replacement_version.id
                  left join tasks replacement_task on replacement_task.id=replacement.task_id
                  where replacement.project_id=vg.project_id
                    and replacement.status!='stale' and replacement.gate_key=vg.gate_key
                    and replacement_version.status='approved'
                    and replacement_requirement.requirement_key=source_requirement.requirement_key
                    and coalesce(replacement.work_unit_id,replacement_task.work_unit_id,0)
                        =coalesce(vg.work_unit_id,t.work_unit_id,0)
                )
              union all
              select 3, 'coverage_item', c.id, coalesce(c.work_unit_id,t.work_unit_id,0)
              from coverage_items c left join tasks t on t.id=c.task_id
              where c.status='stale'
                and not exists (
                  select 1 from coverage_items replacement
                  where replacement.project_id=c.project_id
                    and replacement.design_requirement_id=c.design_requirement_id
                    and replacement.task_id is c.task_id
                    and replacement.work_unit_id is c.work_unit_id
                    and replacement.status!='stale'
                    and replacement.id>c.id
                )
              union all
              select 4, 'review_plan', rp.id, rp.work_unit_id
              from review_plans rp
              join design_versions v on v.id=rp.design_version_id
              join design_packages p on p.id=v.design_package_id
              where rp.status='blocked' and p.current_design_version_id!=rp.design_version_id
            ) stale
            where work_id=?1
              and not exists(
                select 1 from acceptance_records ar
                where ar.target_type='stale_record' and ar.stale_record_type=stale.kind
                  and ar.stale_record_id=stale.record_id and ar.status='approved'
              )
            order by kind_rank, record_id limit 1
            "#,
            params![owner_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;
    let Some((kind, record_id)) = stale else {
        return Ok(None);
    };
    let target = format!("{kind}/{record_id}");
    let transition = conn
        .query_row(
            r#"
            select token.closure_id, token.token_ordinal
            from correction_tokens token
            join closures c on c.id=token.closure_id and c.status='registered'
            join findings f on f.id=c.finding_id and f.status='open' and f.classification='valid'
            where token.status='pending' and token.token_kind='transition'
              and token.operation in ('stale-accept','stale-close') and token.target=?1
            order by token.closure_id,token.token_ordinal limit 1
            "#,
            params![target],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;
    let next_action = transition.map_or_else(
        || format!("agent-workbench stale accept {kind} {record_id} --reason \"<reason>\""),
        |(closure_id, token)| {
            format!("agent-workbench closure transition apply {closure_id} --token {token}")
        },
    );
    Ok(Some(PhaseBlocker {
        kind: "stale_design".to_string(),
        review_plan_id: None,
        work_unit_id: Some(owner_id),
        review_type: None,
        stage: None,
        review_run_id: None,
        finding_id: None,
        severity: None,
        classification: None,
        description: format!("owner has unresolved stale {kind} {record_id}"),
        next_action,
    }))
}

fn owner_dependency_blocker(conn: &Connection, owner_id: i64) -> Result<Option<PhaseBlocker>> {
    conn.query_row(
        r#"
        select d.id,d.depends_on_work_unit_id,target.status,
               (select a.status from work_unit_activations a
                where a.work_unit_id=target.id and a.status in ('active','suspended')
                order by a.id desc limit 1)
        from work_unit_dependencies d
        join work_units target on target.id=d.depends_on_work_unit_id
        where d.work_unit_id=?1 and d.status='open'
          and d.dependency_type in ('blocks','invalidates_assumption','invalidates_closure')
          and target.status in ('open','blocked')
        order by d.id limit 1
        "#,
        params![owner_id],
        |row| {
            let dependency_id: i64 = row.get(0)?;
            let target_id: i64 = row.get(1)?;
            let target_status: String = row.get(2)?;
            let activation_status: Option<String> = row.get(3)?;
            let next_action = match (target_status.as_str(), activation_status.as_deref()) {
                ("blocked", _) => format!(
                    "agent-workbench work unblock {target_id} --reason \"resolve dependency {dependency_id} for owner {owner_id}\""
                ),
                (_, Some("active")) => format!("continue work unit {target_id}"),
                (_, Some("suspended")) => {
                    "agent-workbench resume-check --maturity trace-aware".to_string()
                }
                _ => format!("agent-workbench work activate {target_id}"),
            };
            Ok(PhaseBlocker {
                kind: "work_dependency".to_string(),
                review_plan_id: None,
                work_unit_id: Some(owner_id),
                review_type: None,
                stage: None,
                review_run_id: None,
                finding_id: None,
                severity: None,
                classification: None,
                description: format!(
                    "owner is waiting for dependency {dependency_id} on work unit {target_id}"
                ),
                next_action,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn owner_review_blocker(conn: &Connection, owner_id: i64) -> Result<Option<PhaseBlocker>> {
    conn.query_row(
        r#"
        select p.id, p.review_type, p.stage, r.id, f.id, f.severity,
               f.classification, f.description, p.status, p.required, w.status,
               exists(select 1 from acceptance_records pa
                      where pa.target_type='review_plan' and pa.review_plan_id=p.id
                        and pa.status='approved'),
               (select c.id from closures c where c.finding_id=f.id
                  and c.status!='superseded' order by c.id desc limit 1),
               (select c.status from closures c where c.finding_id=f.id
                  and c.status!='superseded' order by c.id desc limit 1),
               (select a.id from closure_attempts a join closures c on c.id=a.closure_id
                  where c.finding_id=f.id and a.result is null order by a.id desc limit 1),
               (select rr.id from review_runs rr join closure_attempts a
                  on rr.target_ref='review-context:finding-fix:finding=' || f.id
                    || ':closure=' || a.closure_id || ':attempt=' || a.id
                  where rr.review_plan_id=p.id and rr.run_type='resume'
                    and rr.run_purpose='finding_fix_verification'
                    and a.result is null
                    and rr.id>a.review_run_high_watermark order by rr.id desc limit 1),
               (select rr.finding_fix_result from review_runs rr join closure_attempts a
                  on rr.target_ref='review-context:finding-fix:finding=' || f.id
                    || ':closure=' || a.closure_id || ':attempt=' || a.id
                  where rr.review_plan_id=p.id and rr.run_type='resume'
                    and rr.run_purpose='finding_fix_verification'
                    and a.result is null
                    and rr.id>a.review_run_high_watermark order by rr.id desc limit 1),
               not exists(
                 select 1 from closures correction_closure
                 join correction_tokens correction_token
                   on correction_token.closure_id=correction_closure.id
                 where correction_closure.finding_id=f.id
                   and correction_closure.status='registered'
               )
        from review_plans p
        join review_runs r on r.review_plan_id=p.id
        join findings f on f.review_run_id=r.id and f.status='open'
        join work_units w on w.id=p.work_unit_id
        where p.work_unit_id=?1
          and f.classification in ('unclassified','valid','design_conflict','needs_evidence')
          and not exists(select 1 from legacy_claim_audits l where l.project_id=f.project_id and l.review_run_id=f.review_run_id and l.reviewer_resolution in ('unbound','ambiguous'))
          and not exists(select 1 from acceptance_records ar
                         where ar.target_type='finding' and ar.finding_id=f.id
                           and ar.status='approved'
                           and ar.acceptance_type in (
                             'accepted_out_of_scope','explicit_exception','classified_failure'
                           ))
        order by
          case
            when f.classification!='valid' then 1
            when p.status in ('exhausted','needs_user_decision') then 2
            when p.required=0 or exists(
              select 1 from acceptance_records plan_acceptance
              where plan_acceptance.target_type='review_plan'
                and plan_acceptance.review_plan_id=p.id
                and plan_acceptance.status='approved'
            ) then 3
            when not exists(
              select 1 from closures c
              where c.finding_id=f.id and c.status!='superseded'
            ) then 4
            when exists(
              select 1 from closures c
              where c.finding_id=f.id and c.status='incomplete'
            ) then 5
            when exists(
              select 1 from closures c
              where c.finding_id=f.id and c.status='ready_for_verification'
            ) then 6
            when not (
              p.required=1 and p.stage='close-ready'
              and p.review_type in ('implementation_review','design_implementation_diff')
            ) then 7
            else 8
          end,
          case p.stage
            when 'design-ready' then 1
            when 'implementation-ready' then 2
            when 'close-ready' then 3
            when 'resume-ready' then 4
            else 5
          end,
          case when
            f.classification='valid'
            and p.stage='close-ready'
            and p.review_type in ('implementation_review','design_implementation_diff')
            and exists(
              select 1 from finding_remediation_bindings prior
              join work_unit_activations prior_a
                on prior_a.id=prior.work_unit_activation_id
              where prior.work_unit_id=p.work_unit_id and prior_a.status='suspended'
                and prior.id=(
                  select max(last.id) from finding_remediation_bindings last
                  where last.work_unit_id=p.work_unit_id
                )
            ) then 1 else 0 end,
          case when
            f.classification='valid'
            and p.stage='close-ready'
            and p.review_type in ('implementation_review','design_implementation_diff')
            and exists(
              select 1 from finding_remediation_bindings prior
              join work_unit_activations prior_a
                on prior_a.id=prior.work_unit_activation_id
              where prior.work_unit_id=p.work_unit_id and prior_a.status='suspended'
                and prior.id=(
                  select max(last.id) from finding_remediation_bindings last
                  where last.work_unit_id=p.work_unit_id
                )
            ) then coalesce((
              select max(last.id) from finding_remediation_bindings last
              where last.work_unit_id=p.work_unit_id
            ),0) else 0 end,
          f.id,
          p.work_unit_id,
          coalesce((select max(c.id) from closures c where c.finding_id=f.id),0),
          p.id,
          r.id
        limit 1
        "#,
        params![owner_id],
        |row| {
            let review_plan_id = row.get(0)?;
            let review_type: String = row.get(1)?;
            let stage: String = row.get(2)?;
            let finding_id = row.get(4)?;
            let classification: String = row.get(6)?;
            let plan_status: String = row.get(8)?;
            let plan_required: bool = row.get(9)?;
            let work_status: String = row.get(10)?;
            let plan_accepted: bool = row.get(11)?;
            let closure_id = row.get(12)?;
            let closure_status = row.get::<_, Option<String>>(13)?;
            let attempt_id = row.get(14)?;
            let verification_run_id: Option<i64> = row.get(15)?;
            let verification_result = row.get::<_, Option<String>>(16)?;
            let implementation_surface = row.get::<_, bool>(17)?;
            let implementation_eligible = stage == "close-ready"
                && matches!(
                    review_type.as_str(),
                    "implementation_review" | "design_implementation_diff"
                )
                && implementation_surface;
            Ok(PhaseBlocker {
                kind: "required_review_finding".to_string(),
                review_plan_id: Some(review_plan_id),
                work_unit_id: Some(owner_id),
                review_type: Some(review_type),
                stage: Some(stage),
                review_run_id: Some(row.get(3)?),
                finding_id: Some(finding_id),
                severity: Some(row.get(5)?),
                classification: Some(classification.clone()),
                description: row.get(7)?,
                next_action: finding_next_action(FindingActionState {
                    finding_id,
                    review_plan_id,
                    closure_id,
                    closure_status: closure_status.as_deref(),
                    attempt_id,
                    verification: verification_run_id
                        .map(|run_id| (run_id, verification_result.as_deref())),
                    classification: &classification,
                    implementation_eligible,
                    work_unit_id: owner_id,
                    work_status: &work_status,
                    plan_status: &plan_status,
                    plan_required,
                    plan_accepted,
                }),
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn action_is_schedulable(action: &str) -> bool {
    action.contains("agent-workbench ")
}
