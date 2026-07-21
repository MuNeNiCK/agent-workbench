use super::super::*;

pub(in crate::decomposition) fn application_outcome(
    conn: &Connection,
    plan_id: i64,
    already_applied: bool,
) -> Result<DecompositionApplicationOutcome> {
    conn.query_row(
        r#"
        select
          (select count(*) from decomposition_applications where decomposition_plan_id=?1),
          (select count(*) from decomposition_application_boundaries boundary
           join decomposition_item_checklist_boundaries source on source.id=boundary.decomposition_item_checklist_boundary_id
           join decomposition_items item on item.id=source.decomposition_item_id
           where item.decomposition_plan_id=?1),
          (select count(distinct phase_id) from decomposition_applications where decomposition_plan_id=?1),
          (select count(*) from decomposition_application_dependencies application
           join decomposition_slice_dependencies source on source.id=application.decomposition_slice_dependency_id
           where source.decomposition_plan_id=?1),
          (select status from decomposition_plans where id=?1)
        "#,
        [plan_id],
        |row| {
            Ok(DecompositionApplicationOutcome {
                plan_id,
                task_count: row.get(0)?,
                checklist_item_count: row.get(1)?,
                phase_count: row.get(2)?,
                dependency_count: row.get(3)?,
                already_applied,
                applied: row.get::<_, String>(4)? == "applied",
            })
        },
    )
    .map_err(Into::into)
}

pub(in crate::decomposition) fn resolve_current_plan_id(
    conn: &Connection,
    project_id: i64,
    design_version_id: i64,
    work_unit_id: i64,
) -> Result<Option<i64>> {
    let mut statement = conn.prepare(
        r#"
        select plan.id,plan.status
        from decomposition_plans plan
        join design_versions plan_version on plan_version.id=plan.design_version_id
        join design_versions selected_version on selected_version.id=?2
        where plan.project_id=?1 and plan.work_unit_id=?3 and plan.status!='superseded'
          and plan_version.design_package_id=selected_version.design_package_id
        order by plan.id
        "#,
    )?;
    let matches = statement
        .query_map(
            params![project_id, design_version_id, work_unit_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let applied = matches
        .iter()
        .filter(|(_, status)| status == "applied")
        .map(|(id, _)| *id)
        .collect::<Vec<_>>();
    match applied.as_slice() {
        [plan_id] => return Ok(Some(*plan_id)),
        [] => {}
        _ => bail!("the selected package and work have multiple applied Decomposition Plans"),
    }
    match matches.as_slice() {
        [] => Ok(None),
        [(plan_id, _)] => Ok(Some(*plan_id)),
        _ => bail!("the selected package and work have multiple editable Decomposition Plans"),
    }
}

pub(in crate::decomposition) fn plan_by_source_identity(
    conn: &Connection,
    project_id: i64,
    source_identity: &str,
) -> Result<Option<i64>> {
    conn.query_row(
        "select id from decomposition_plans where project_id=?1 and source_identity=?2",
        params![project_id, source_identity],
        |row| row.get(0),
    )
    .optional()
    .map_err(Into::into)
}

pub(in crate::decomposition) fn resolve_decomposition_slot(
    conn: &Connection,
    root: &Path,
    project_id: i64,
    query: DecompositionPlanQuery,
) -> Result<DecompositionPlanResolution> {
    let current = resolve_current_plan_id(
        conn,
        project_id,
        query.design_version_id,
        query.work_unit_id,
    )?
    .map(|plan_id| load_decomposition_plan(conn, plan_id))
    .transpose()?;
    let candidates = resolve_plan_candidates(conn, root, project_id, &query)?;
    let successor = current
        .as_ref()
        .filter(|plan| plan.status == "applied")
        .map(|plan| load_ready_successor(conn, project_id, plan.id, query.design_version_id))
        .transpose()?
        .flatten();
    let review_plan = successor.as_ref().or(current.as_ref());
    let review_owner = review_plan
        .filter(|plan| matches!(plan.status.as_str(), "ready" | "applied"))
        .map(|plan| resolve_plan_review_owner(conn, project_id, plan, query.design_version_id))
        .transpose()?;
    let action_resolution = decomposition_actions(
        conn,
        &query,
        current.as_ref(),
        successor.as_ref(),
        &candidates,
        review_owner.as_ref(),
    )?;
    let successor_projection = successor
        .as_ref()
        .map(|plan| decomposition_review_projection(conn, plan.id, query.design_version_id))
        .transpose()?
        .flatten();
    Ok(DecompositionPlanResolution {
        design_version_id: query.design_version_id,
        work_unit_id: query.work_unit_id,
        current,
        successor,
        successor_projection,
        candidates,
        review_owner,
        actions: action_resolution.actions,
    })
}

pub(in crate::decomposition) fn load_ready_successor(
    conn: &Connection,
    project_id: i64,
    predecessor_id: i64,
    design_version_id: i64,
) -> Result<Option<DecompositionPlanRecord>> {
    let id = conn
        .query_row(
            r#"
            select candidate.id
            from decomposition_plans candidate
            join design_versions candidate_version on candidate_version.id=candidate.design_version_id
            join design_versions selected_version on selected_version.id=?3
            where candidate.project_id=?1 and candidate.predecessor_id=?2
              and candidate.status='ready'
              and candidate_version.design_package_id=selected_version.design_package_id
            order by candidate.revision desc,candidate.id desc limit 1
            "#,
            params![project_id, predecessor_id, design_version_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    id.map(|id| load_decomposition_plan(conn, id)).transpose()
}

pub(in crate::decomposition) fn plan_review_target(
    conn: &Connection,
    plan: &DecompositionPlanRecord,
    selected_design_version_id: i64,
) -> Result<DecompositionPlanReviewTarget> {
    Ok(DecompositionPlanReviewTarget {
        plan_id: plan.id,
        revision: plan.revision,
        content_identity: plan.content_identity.clone(),
        current_identity: plan.current_identity.clone(),
        projection_identity: decomposition_review_projection_identity(
            conn,
            plan.id,
            selected_design_version_id,
        )?,
        design_version_id: selected_design_version_id,
        work_unit_id: plan.work_unit_id,
    })
}

pub(in crate::decomposition) fn resolve_plan_review_owner(
    conn: &Connection,
    project_id: i64,
    plan: &DecompositionPlanRecord,
    selected_design_version_id: i64,
) -> Result<PlanReviewOwnerResolution> {
    resolve_decomposition_plan_review_owner(
        conn,
        project_id,
        &plan_review_target(conn, plan, selected_design_version_id)?,
    )
}

fn resolve_managed_plan_source(
    conn: &Connection,
    project_id: i64,
    query: &DecompositionPlanQuery,
    parsed: &ParsedPlan,
) -> Result<Option<(String, bool)>> {
    let mut managed_document = parsed
        .document
        .clone()
        .context("Decomposition Plan metadata is required")?;
    if let Some(reconciliation) = managed_document.reconciliation.as_mut() {
        let predecessor_is_in_scope: bool = conn.query_row(
            r#"
            select exists(
              select 1
              from decomposition_plans predecessor
              join design_versions selected on selected.id=?2
              where predecessor.id=?4 and predecessor.project_id=?1
                and predecessor.work_unit_id=?3
                and predecessor.design_package_id=selected.design_package_id
            )
            "#,
            params![
                project_id,
                query.design_version_id,
                query.work_unit_id,
                reconciliation.predecessor
            ],
            |row| row.get(0),
        )?;
        if predecessor_is_in_scope {
            reconciliation.expected_current =
                load_decomposition_plan(conn, reconciliation.predecessor)?.current_identity;
        }
    }
    let managed_content_identity =
        plan_content_identity(&canonical_plan_content(&managed_document)?);
    let (is_managed, continues_successor_lifecycle): (bool, bool) = conn.query_row(
        r#"
        select
          exists(
            select 1
            from decomposition_plans prior
            join design_versions selected on selected.id=?2
            where prior.project_id=?1 and prior.work_unit_id=?3
              and prior.design_package_id=selected.design_package_id
              and prior.content_identity in (?4,?5)
          ),
          exists(
            select 1
            from decomposition_plans successor
            join decomposition_plans predecessor on predecessor.id=successor.predecessor_id
            join design_versions selected on selected.id=?2
            where successor.project_id=?1 and successor.work_unit_id=?3
              and successor.design_package_id=selected.design_package_id
              and successor.status='ready' and predecessor.status='applied'
              and predecessor.project_id=successor.project_id
              and predecessor.work_unit_id=successor.work_unit_id
              and predecessor.design_package_id=successor.design_package_id
              and successor.content_identity in (?4,?5)
          )
        "#,
        params![
            project_id,
            query.design_version_id,
            query.work_unit_id,
            parsed.content_identity,
            managed_content_identity
        ],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    Ok(is_managed.then_some((managed_content_identity, continues_successor_lifecycle)))
}

pub(in crate::decomposition) fn resolve_plan_candidates(
    conn: &Connection,
    root: &Path,
    project_id: i64,
    query: &DecompositionPlanQuery,
) -> Result<Vec<DecompositionPlanCandidate>> {
    let (package_root, design_identity): (String, String) = conn.query_row(
        r#"
        select package.root_path,version.content_hash
        from design_versions version
        join design_packages package on package.id=version.design_package_id
        where version.id=?1 and version.project_id=?2
        "#,
        params![query.design_version_id, project_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let plans = Path::new(&package_root).join("plans");
    if !plans.is_dir() {
        return Ok(Vec::new());
    }
    let mut candidates = Vec::new();
    for path in sorted_entries(&plans)? {
        if path.extension().and_then(|value| value.to_str()) != Some("md") {
            continue;
        }
        if !fs::symlink_metadata(&path)?.file_type().is_file() {
            continue;
        }
        let parsed = match parse_plan_unvalidated(root, &path) {
            Ok(parsed) => parsed,
            Err(_) => continue,
        };
        let Some(document) = parsed.document.as_ref() else {
            continue;
        };
        if document.design_fingerprint != design_identity
            || resolve_work_binding(conn, project_id, query.design_version_id, document)?
                != Some(query.work_unit_id)
        {
            continue;
        }
        if let Some((managed_content_identity, continues_successor_lifecycle)) =
            resolve_managed_plan_source(conn, project_id, query, &parsed)?
        {
            if continues_successor_lifecycle {
                candidates.push(DecompositionPlanCandidate {
                    source_path: parsed.source_path.to_string_lossy().into_owned(),
                    ingress_identity: parsed.source_identity,
                    content_identity: parsed.content_identity,
                    managed_content_identity,
                    structurally_ready: true,
                    issue: None,
                });
            }
            continue;
        }
        let mut candidate_document = document.clone();
        let mut issue = validate_plan(&candidate_document)
            .err()
            .map(|error| error.to_string());
        if issue.is_none()
            && let Some(reconciliation) = candidate_document.reconciliation.as_mut()
            && let Some(current_id) = resolve_current_plan_id(
                conn,
                project_id,
                query.design_version_id,
                query.work_unit_id,
            )?
        {
            let current = load_decomposition_plan(conn, current_id)?;
            reconciliation.expected_current = current.current_identity;
            issue = validate_reconciliation_scope(
                conn,
                project_id,
                query.design_version_id,
                query.work_unit_id,
                &candidate_document,
            )
            .and_then(|_| {
                validate_reconciliation_successor(
                    conn,
                    project_id,
                    query.design_version_id,
                    &candidate_document,
                )
            })
            .err()
            .map(|error| error.to_string());
        }
        candidates.push(DecompositionPlanCandidate {
            source_path: parsed.source_path.to_string_lossy().into_owned(),
            ingress_identity: parsed.source_identity,
            content_identity: parsed.content_identity,
            managed_content_identity: plan_content_identity(&canonical_plan_content(
                &candidate_document,
            )?),
            structurally_ready: issue.is_none(),
            issue,
        });
    }
    Ok(candidates)
}

pub(in crate::decomposition) fn decomposition_actions(
    conn: &Connection,
    query: &DecompositionPlanQuery,
    current: Option<&DecompositionPlanRecord>,
    successor: Option<&DecompositionPlanRecord>,
    candidates: &[DecompositionPlanCandidate],
    review_owner: Option<&PlanReviewOwnerResolution>,
) -> Result<DecompositionActionResolution> {
    let show = format!(
        "agent-workbench decomposition show --design-version {} --work {}",
        query.design_version_id, query.work_unit_id
    );
    let Some(plan) = current else {
        if candidates.is_empty() {
            return Ok(DecompositionActionResolution {
                actions: vec![format!(
                    "agent-workbench decomposition import --design-version {} --work {} --expected-current absent --idempotency-key import-empty-{}-{}",
                    query.design_version_id,
                    query.work_unit_id,
                    query.design_version_id,
                    query.work_unit_id
                )],
                blocks_work: true,
            });
        }
        return Ok(DecompositionActionResolution {
            actions: candidates
                .iter()
                .map(|candidate| {
                    format!(
                        "agent-workbench decomposition import --design-version {} --work {} --plan {} --expected-content {} --expected-current absent --idempotency-key import-{}",
                        query.design_version_id,
                        query.work_unit_id,
                        candidate.source_path,
                        candidate.ingress_identity,
                        &candidate.ingress_identity[..12]
                    )
                })
                .collect(),
            blocks_work: true,
        });
    };
    let resolution = match plan.status.as_str() {
        "draft" | "incomplete" => {
            let mut actions = vec![format!(
                "agent-workbench decomposition validate {} --expected-current {} --idempotency-key validate-{}-{}",
                plan.id, plan.current_identity, plan.id, plan.revision
            )];
            actions.push(format!(
                "agent-workbench decomposition revise {} --expected-current {} --idempotency-key revise-owned-{}-{}",
                plan.id, plan.current_identity, plan.id, plan.revision
            ));
            actions.extend(candidates.iter().map(|candidate| {
                    format!(
                        "agent-workbench decomposition revise {} --plan {} --expected-content {} --expected-current {} --idempotency-key revise-{}-{}-{}",
                        plan.id,
                        candidate.source_path,
                        candidate.ingress_identity,
                        plan.current_identity,
                        plan.id,
                        plan.revision,
                        &candidate.ingress_identity[..12]
                    )
                }));
            DecompositionActionResolution {
                actions,
                blocks_work: true,
            }
        }
        "ready" => DecompositionActionResolution {
            actions: match review_owner {
                Some(owner) if owner.state == PlanReviewOwnerState::AcceptedClean => vec![format!(
                    "agent-workbench decomposition apply {} --work {}",
                    query.design_version_id, query.work_unit_id
                )],
                Some(owner) => owner.actions.clone(),
                None => vec![show],
            },
            blocks_work: true,
        },
        "applied" => {
            let mut successor_candidates = Vec::new();
            for candidate in candidates {
                if !candidate.structurally_ready
                    || (plan.design_version_id == query.design_version_id
                        && plan.content_identity == candidate.managed_content_identity)
                {
                    continue;
                }
                let is_current_successor = successor.is_some_and(|successor| {
                    successor.predecessor_id == Some(plan.id)
                        && successor.content_identity == candidate.managed_content_identity
                });
                if is_current_successor {
                    successor_candidates.push(candidate);
                    continue;
                }
                let historical: bool = conn.query_row(
                    r#"
                    select exists(
                      select 1 from decomposition_plans prior
                      join decomposition_plans current on current.id=?1
                      where prior.project_id=current.project_id
                        and prior.design_package_id=current.design_package_id
                        and prior.work_unit_id=current.work_unit_id
                        and prior.status='superseded'
                        and prior.id!=current.id
                        and prior.content_identity in (?2,?3)
                    )
                    "#,
                    params![
                        plan.id,
                        candidate.content_identity,
                        candidate.managed_content_identity
                    ],
                    |row| row.get(0),
                )?;
                if !historical {
                    successor_candidates.push(candidate);
                }
            }
            if !successor_candidates.is_empty() {
                let mut stmt = conn.prepare(
                    r#"
                        select closure.id,token.target
                        from closures closure
                        join correction_sessions session on session.closure_id=closure.id
                        join correction_tokens token on token.closure_id=closure.id
                        where closure.status='registered' and session.status='active'
                          and token.status='pending' and token.token_kind='transition'
                          and token.operation='decomposition-plan-reconcile'
                        order by closure.id
                        "#,
                )?;
                let closure_tokens = stmt
                    .query_map([], |row| {
                        Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let mut actions = Vec::new();
                for candidate in successor_candidates {
                    let is_current_successor = successor.is_some_and(|successor| {
                        successor.predecessor_id == Some(plan.id)
                            && successor.content_identity == candidate.managed_content_identity
                    });
                    if !is_current_successor {
                        let revision_target = successor.unwrap_or(plan);
                        actions.push(format!(
                            "agent-workbench decomposition revise {} --plan {} --expected-content {} --expected-current {} --idempotency-key successor-{}-{}-{}",
                            revision_target.id,
                            candidate.source_path,
                            candidate.ingress_identity,
                            revision_target.current_identity,
                            revision_target.id,
                            revision_target.revision,
                            &candidate.managed_content_identity[..12]
                        ));
                        continue;
                    }
                    if let Some(owner) = review_owner
                        && owner.state != PlanReviewOwnerState::AcceptedClean
                    {
                        actions.extend(owner.actions.clone());
                        continue;
                    }
                    for (closure_id, target) in &closure_tokens {
                        let Ok((design, work, authorized_path)) =
                            crate::review::parse_decomposition_reconciliation_target(target)
                        else {
                            continue;
                        };
                        if design != query.design_version_id
                            || work != query.work_unit_id
                            || authorized_path != candidate.source_path
                        {
                            continue;
                        }
                        actions.push(format!(
                            "agent-workbench decomposition reconcile {} --work {} --plan {} --closure {} --expected-current {} --dry-run",
                            query.design_version_id,
                            query.work_unit_id,
                            candidate.source_path,
                            closure_id,
                            plan.current_identity
                        ));
                    }
                }
                if actions.is_empty() {
                    actions.push(show);
                }
                DecompositionActionResolution {
                    actions,
                    blocks_work: true,
                }
            } else {
                let mut actions = vec![show];
                if let Some(owner) = review_owner {
                    if owner.state == PlanReviewOwnerState::AcceptedClean {
                        actions.push(format!(
                            "agent-workbench gate implementation-ready --design-version {}",
                            query.design_version_id
                        ));
                    } else {
                        actions.extend(owner.actions.clone());
                    }
                }
                let exact_review_accepted = review_owner
                    .is_some_and(|owner| owner.state == PlanReviewOwnerState::AcceptedClean);
                DecompositionActionResolution {
                    actions,
                    blocks_work: plan.issue.is_some()
                        || successor.is_some()
                        || !exact_review_accepted,
                }
            }
        }
        _ => DecompositionActionResolution {
            actions: vec![show],
            blocks_work: true,
        },
    };
    Ok(resolution)
}
