use super::*;

pub fn reconcile_decomposition_plan(
    root: &Path,
    input: DecompositionReconciliationApplication<'_>,
) -> Result<DecompositionReconciliationOutcome> {
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let outcome = match resolve_decomposition_reconciliation(&tx, root, &input, false)? {
        DecompositionReconciliationResolution::Retry(outcome) => *outcome,
        DecompositionReconciliationResolution::Pending(resolution) => {
            apply_decomposition_reconciliation(&tx, *resolution)?
        }
    };
    tx.commit()?;
    Ok(outcome)
}

pub fn preview_decomposition_reconciliation(
    root: &Path,
    input: DecompositionReconciliationApplication<'_>,
) -> Result<DecompositionReconciliationOutcome> {
    let conn = open_existing_project(root)?;
    match resolve_decomposition_reconciliation(&conn, root, &input, true)? {
        DecompositionReconciliationResolution::Retry(outcome) => Ok(*outcome),
        DecompositionReconciliationResolution::Pending(resolution) => Ok(
            preview_reconciliation_outcome(&resolution, input.closure_id),
        ),
    }
}

pub(super) fn resolve_decomposition_reconciliation(
    conn: &Connection,
    root: &Path,
    input: &DecompositionReconciliationApplication<'_>,
    preview: bool,
) -> Result<DecompositionReconciliationResolution> {
    require_digest(input.expected_current, "reconciliation expected current")?;
    let (parsed, staged_plan_id) = resolve_reconciliation_input_plan(conn, root, input)?;
    let document = parsed
        .document
        .as_ref()
        .context("Decomposition Plan metadata is required for reconciliation")?;
    let reconciliation = document
        .reconciliation
        .as_ref()
        .context("Decomposition Plan reconciliation metadata is required")?;
    let payload_identity = reconciliation_payload_identity(&parsed, input);
    let project_id = project_id(conn)?;

    if let Some((
        plan_id,
        reconciliation_application_id,
        correction_application_id,
        session_id,
        token_id,
        token_ordinal,
        operation,
        target,
        review_work,
        review_design,
        stored_payload,
    )) = conn
        .query_row(
            r#"
            select application.successor_plan_id,application.id,
                   application.correction_application_id,
                   session.id,token.id,token.token_ordinal,token.operation,token.target,
                   plan.work_unit_id,plan.design_version_id,application.payload_identity
            from decomposition_reconciliation_applications application
            join correction_tokens token on token.id=application.correction_token_id
            join correction_transition_applications transition
              on transition.id=application.correction_application_id
            join correction_sessions session on session.id=transition.correction_session_id
            join closures closure on closure.id=session.closure_id
            join findings finding on finding.id=closure.finding_id
            join review_runs run on run.id=finding.review_run_id
            join review_plans plan on plan.id=run.review_plan_id
            where application.project_id=?1 and token.closure_id=?2
              and application.source_identity=?3
            "#,
            params![project_id, input.closure_id, parsed.source_identity],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, Option<i64>>(9)?,
                    row.get::<_, String>(10)?,
                ))
            },
        )
        .optional()?
    {
        if let Err(error) =
            validate_reconciliation_token_target(&operation, &target, input, &parsed)
        {
            let legacy_target = format!("{}/{}", input.design_version_id, input.work_unit_id);
            if operation != "decomposition-plan-reconcile" || target != legacy_target {
                return Err(error);
            }
        }
        if stored_payload != payload_identity {
            bail!("reconciliation retry payload differs from the recorded application");
        }
        let mut outcome = load_reconciliation_result(conn, reconciliation_application_id)?;
        if outcome.plan.plan_id != plan_id
            || outcome.predecessor_plan_id != reconciliation.predecessor
            || outcome.closure_id != input.closure_id
            || outcome.token_ordinal != token_ordinal
            || outcome.correction_application_id != correction_application_id
        {
            bail!("recorded reconciliation result does not match its application");
        }
        let expected_correction = reconciliation_correction_handle(
            input.closure_id,
            session_id,
            token_id,
            token_ordinal,
            &operation,
            &target,
            review_work,
            review_design,
        );
        if outcome.projection.observed_correction != expected_correction {
            bail!("recorded reconciliation result does not match its correction boundary");
        }
        if input.expected_current != outcome.projection.commit_current {
            bail!("reconciliation retry expected current differs from the recorded result");
        }
        outcome.idempotent = true;
        outcome.plan.already_applied = true;
        return Ok(DecompositionReconciliationResolution::Retry(Box::new(
            outcome,
        )));
    }

    validate_application_owner(
        conn,
        project_id,
        input.design_version_id,
        input.work_unit_id,
    )?;
    if resolve_design_version(conn, project_id, &document.design_fingerprint)?
        != input.design_version_id
        || resolve_work_binding(conn, project_id, input.design_version_id, document)?
            != Some(input.work_unit_id)
    {
        bail!("Decomposition Plan does not identify the selected design and work owner");
    }
    validate_plan_package_root(
        conn,
        project_id,
        input.design_version_id,
        &parsed.design_root,
    )?;
    let (session_id, token_id, token_ordinal, operation, target, review_work, review_design): (
        i64,
        i64,
        i64,
        String,
        String,
        i64,
        Option<i64>,
    ) = conn
        .query_row(
            r#"
            select session.id,token.id,token.token_ordinal,token.operation,token.target,
                   plan.work_unit_id,plan.design_version_id
            from correction_sessions session
            join closures closure on closure.id=session.closure_id
            join findings finding on finding.id=closure.finding_id
            join review_runs run on run.id=finding.review_run_id
            join review_plans plan on plan.id=run.review_plan_id
            join correction_tokens token on token.closure_id=closure.id
            where closure.id=?1 and closure.project_id=?2 and closure.status='registered'
              and finding.status='open' and finding.classification='valid'
              and session.status='active' and token.token_kind='transition'
              and token.status='pending'
              and token.token_ordinal=(
                select min(next.token_ordinal) from correction_tokens next
                where next.closure_id=closure.id and next.token_kind='transition'
                  and next.status='pending'
              )
            "#,
            params![input.closure_id, project_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .optional()?
        .context("active correction has no pending decomposition reconciliation token")?;
    let authorized_path =
        validate_reconciliation_token_target(&operation, &target, input, &parsed)?;
    validate_reconciliation_file_surface(
        conn,
        root,
        token_id,
        input.design_version_id,
        &authorized_path,
    )?;
    if review_work != input.work_unit_id
        || !review_design_accepts(conn, review_design, input.design_version_id)?
    {
        bail!("Decomposition Plan reconciliation is outside the correction review owner");
    }

    validate_reconciliation_scope(
        conn,
        project_id,
        input.design_version_id,
        input.work_unit_id,
        document,
    )?;
    validate_reconciliation_successor(conn, project_id, input.design_version_id, document)?;
    if !preview && staged_plan_id.is_none() {
        bail!(
            "an applied Decomposition Plan can only be reconciled from its reviewed ready successor; next: agent-workbench decomposition show --design-version {} --work {}",
            input.design_version_id,
            input.work_unit_id
        );
    }
    if let Some(staged_plan_id) = staged_plan_id {
        let staged = load_decomposition_plan(conn, staged_plan_id)?;
        let owner = resolve_plan_review_owner(conn, project_id, &staged, input.design_version_id)?;
        if owner.state != PlanReviewOwnerState::AcceptedClean {
            bail!(
                "successor Decomposition Plan requires accepted clean review; next: {}",
                owner.actions.join(" | ")
            );
        }
    }
    let correction_handle = reconciliation_correction_handle(
        input.closure_id,
        session_id,
        token_id,
        token_ordinal,
        &operation,
        &target,
        review_work,
        review_design,
    );
    let projection = resolve_reconciliation_projection(conn, &parsed, input, correction_handle)?;
    require_reconciliation_observation(input, reconciliation, &projection, preview)?;

    Ok(DecompositionReconciliationResolution::Pending(Box::new(
        PendingDecompositionReconciliation {
            parsed,
            project_id,
            session_id,
            token_id,
            token_ordinal,
            payload_identity,
            projection,
        },
    )))
}

fn validate_reconciliation_token_target(
    operation: &str,
    target: &str,
    input: &DecompositionReconciliationApplication<'_>,
    parsed: &ParsedPlan,
) -> Result<String> {
    if operation != "decomposition-plan-reconcile" {
        bail!("the next correction transition is not a Decomposition Plan reconciliation");
    }
    let (design, work, authorized_path) =
        crate::review::parse_decomposition_reconciliation_target(target)?;
    if design != input.design_version_id
        || work != input.work_unit_id
        || parsed.source_path != Path::new(&authorized_path)
    {
        bail!(
            "the next correction transition does not authorize this Decomposition Plan path and owner"
        );
    }
    Ok(authorized_path)
}

fn validate_reconciliation_file_surface(
    conn: &Connection,
    root: &Path,
    token_id: i64,
    design_version_id: i64,
    authorized_path: &str,
) -> Result<()> {
    let package_root: String = conn.query_row(
        r#"
        select package.root_path
        from design_versions version
        join design_packages package on package.id=version.design_package_id
        where version.id=?1
        "#,
        [design_version_id],
        |row| row.get(0),
    )?;
    let canonical_root = root
        .canonicalize()
        .with_context(|| format!("project root does not exist: {}", root.display()))?;
    let package_path = if Path::new(&package_root).is_absolute() {
        PathBuf::from(&package_root)
    } else {
        root.join(&package_root)
    };
    let project_package_root = package_path
        .canonicalize()
        .with_context(|| format!("Design Package does not exist: {}", package_path.display()))?
        .strip_prefix(&canonical_root)
        .context("Design Package is outside the selected project")?
        .to_path_buf();
    let package_relative = Path::new(authorized_path)
        .strip_prefix(&project_package_root)
        .context("authorized Decomposition Plan path is outside its Design Package")?
        .to_string_lossy();
    let has_surface: bool = conn.query_row(
        r#"
        select exists(
          select 1
          from correction_tokens transition_token
          join correction_tokens file_token
            on file_token.closure_id=transition_token.closure_id
          where transition_token.id=?1 and file_token.token_kind='file'
            and file_token.operation in ('edit','create')
            and file_token.target=?2
        )
        "#,
        params![token_id, format!("design:{package_relative}")],
        |row| row.get(0),
    )?;
    if !has_surface {
        bail!(
            "Decomposition Plan reconciliation is not bound to a same-closure design file surface"
        );
    }
    Ok(())
}

pub(super) fn resolve_reconciliation_input_plan(
    conn: &Connection,
    root: &Path,
    input: &DecompositionReconciliationApplication<'_>,
) -> Result<(ParsedPlan, Option<i64>)> {
    let ingress = parse_plan(root, input.plan_path)?;
    let source_path = ingress.source_path.to_string_lossy().into_owned();
    let stored = conn
        .query_row(
            r#"
            select candidate.id,candidate.source_identity,candidate.content_identity,
                   candidate.document_content,package.root_path,ingress.source_identity,
                   ingress.content_identity
            from decomposition_plans candidate
            join design_versions version on version.id=candidate.design_version_id
            join design_packages package on package.id=version.design_package_id
            left join decomposition_plan_ingress_identities ingress on ingress.plan_id=candidate.id
            where candidate.design_version_id=?1 and candidate.work_unit_id=?2
              and candidate.source_path=?3 and candidate.status in ('ready','applied')
            order by candidate.revision desc,candidate.id desc limit 1
            "#,
            params![input.design_version_id, input.work_unit_id, source_path],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            },
        )
        .optional()?;
    let Some((
        plan_id,
        source_identity,
        content_identity,
        content,
        design_root,
        ingress_identity,
        ingress_content_identity,
    )) = stored
    else {
        return Ok((ingress, None));
    };
    let ingress_identity = ingress_identity.context(
        "staged Decomposition Plan has no immutable ingress identity; revise and review it again",
    )?;
    let mut parsed =
        parse_owned_plan_content(content, ingress.source_path, PathBuf::from(design_root))?;
    if ingress.source_identity != ingress_identity {
        bail!(
            "Decomposition Plan bytes changed after the successor was staged for review; revise and review the authorized Plan path again"
        );
    }
    let ingress_content_identity = ingress_content_identity.context(
        "staged Decomposition Plan has no immutable reviewed content identity; revise and review it again",
    )?;
    if ingress_content_identity != content_identity {
        bail!("staged Decomposition Plan ingress does not bind the reviewed Plan content");
    }
    parsed.source_identity = source_identity;
    Ok((parsed, Some(plan_id)))
}

pub(super) fn preview_reconciliation_outcome(
    resolution: &PendingDecompositionReconciliation,
    closure_id: i64,
) -> DecompositionReconciliationOutcome {
    let document = resolution
        .parsed
        .document
        .as_ref()
        .expect("resolved reconciliation has document metadata");
    let reconciliation = document
        .reconciliation
        .as_ref()
        .expect("resolved reconciliation has reconciliation metadata");
    DecompositionReconciliationOutcome {
        plan: DecompositionApplicationOutcome {
            plan_id: 0,
            task_count: document.items.len() as i64,
            checklist_item_count: document
                .items
                .iter()
                .map(|item| item.checklist.len() as i64)
                .sum(),
            phase_count: document.slices.len() as i64,
            dependency_count: document
                .slices
                .iter()
                .map(|slice| slice.depends_on.len() as i64)
                .sum(),
            already_applied: false,
            applied: false,
        },
        predecessor_plan_id: reconciliation.predecessor,
        closure_id,
        token_ordinal: resolution.token_ordinal,
        correction_application_id: 0,
        idempotent: false,
        projection: resolution.projection.clone(),
    }
}

pub(super) fn apply_decomposition_reconciliation(
    conn: &Connection,
    resolution: PendingDecompositionReconciliation,
) -> Result<DecompositionReconciliationOutcome> {
    let document = resolution
        .parsed
        .document
        .as_ref()
        .context("Decomposition Plan metadata is required for reconciliation")?;
    let reconciliation = document
        .reconciliation
        .as_ref()
        .context("Decomposition Plan reconciliation metadata is required")?;
    let work_unit_id: i64 = conn.query_row(
        "select work_unit_id from decomposition_plans where id=?1 and project_id=?2",
        params![reconciliation.predecessor, resolution.project_id],
        |row| row.get(0),
    )?;
    let design_version_id =
        resolve_design_version(conn, resolution.project_id, &document.design_fingerprint)?;
    let before_state = crate::review::transition_state_snapshot(conn, work_unit_id)?;
    let plan = publish_reconciliation_successor(
        conn,
        resolution.project_id,
        design_version_id,
        work_unit_id,
        &resolution.parsed,
    )?;
    let after_state = crate::review::transition_state_snapshot(conn, work_unit_id)?;
    let result_ref = format!("decomposition-plan:{}", plan.plan_id);
    conn.execute(
        r#"
        insert into correction_transition_applications(
          project_id,correction_session_id,correction_token_id,authority_event_id,
          evidence_ref,before_state,after_state,result_ref,created_at
        ) values(?1,?2,?3,null,null,?4,?5,?6,current_timestamp)
        "#,
        params![
            resolution.project_id,
            resolution.session_id,
            resolution.token_id,
            before_state,
            after_state,
            result_ref
        ],
    )?;
    let correction_application_id = conn.last_insert_rowid();
    conn.execute(
        r#"
        insert into decomposition_reconciliation_applications(
          project_id,correction_application_id,correction_token_id,predecessor_plan_id,
          successor_plan_id,source_identity,expected_current,payload_identity,created_at
        ) values(?1,?2,?3,?4,?5,?6,?7,?8,current_timestamp)
        "#,
        params![
            resolution.project_id,
            correction_application_id,
            resolution.token_id,
            reconciliation.predecessor,
            plan.plan_id,
            resolution.parsed.source_identity,
            reconciliation.expected_current,
            resolution.payload_identity
        ],
    )?;
    let reconciliation_application_id = conn.last_insert_rowid();
    conn.execute(
        "update correction_tokens set status='applied',applied_at=current_timestamp where id=?1 and status='pending'",
        [resolution.token_id],
    )?;
    let outcome = DecompositionReconciliationOutcome {
        plan,
        predecessor_plan_id: reconciliation.predecessor,
        closure_id: conn.query_row(
            "select closure_id from correction_tokens where id=?1",
            [resolution.token_id],
            |row| row.get(0),
        )?,
        token_ordinal: resolution.token_ordinal,
        correction_application_id,
        idempotent: false,
        projection: resolution.projection,
    };
    persist_reconciliation_result(
        conn,
        resolution.project_id,
        reconciliation_application_id,
        &outcome,
    )?;
    Ok(outcome)
}

pub(super) fn reconciliation_result_identity(result_json: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/decomposition-reconciliation-result/v1\0");
    hasher.update(result_json.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub(super) fn persist_reconciliation_result(
    conn: &Connection,
    project_id: i64,
    reconciliation_application_id: i64,
    outcome: &DecompositionReconciliationOutcome,
) -> Result<()> {
    let result_json =
        serde_json::to_string(outcome).context("failed to serialize the reconciliation result")?;
    let result_identity = reconciliation_result_identity(&result_json);
    conn.execute(
        r#"
        insert into decomposition_reconciliation_results(
          project_id,reconciliation_application_id,result_json,result_identity,created_at
        ) values(?1,?2,?3,?4,current_timestamp)
        "#,
        params![
            project_id,
            reconciliation_application_id,
            result_json,
            result_identity
        ],
    )?;
    Ok(())
}

pub(super) fn load_reconciliation_result(
    conn: &Connection,
    reconciliation_application_id: i64,
) -> Result<DecompositionReconciliationOutcome> {
    let (result_json, stored_identity): (String, String) = conn
        .query_row(
            r#"
            select result_json,result_identity
            from decomposition_reconciliation_results
            where reconciliation_application_id=?1
            "#,
            [reconciliation_application_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .context("recorded reconciliation result is missing")?;
    if reconciliation_result_identity(&result_json) != stored_identity {
        bail!("recorded reconciliation result identity is invalid");
    }
    serde_json::from_str(&result_json).context("recorded reconciliation result is invalid")
}

pub(crate) fn validate_reconciliation_results(conn: &Connection) -> Result<()> {
    let applications = conn
        .prepare(
            r#"
            select application.id,application.project_id,application.successor_plan_id,
                   application.predecessor_plan_id,application.correction_application_id,
                   token.closure_id,token.token_ordinal
            from decomposition_reconciliation_applications application
            join correction_tokens token on token.id=application.correction_token_id
            order by application.id
            "#,
        )?
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (application, project, successor, predecessor, correction, closure, token_ordinal) in
        applications
    {
        let outcome = load_reconciliation_result(conn, application)?;
        let stored_project: i64 = conn.query_row(
            "select project_id from decomposition_reconciliation_results where reconciliation_application_id=?1",
            [application],
            |row| row.get(0),
        )?;
        if stored_project != project
            || outcome.plan.plan_id != successor
            || outcome.predecessor_plan_id != predecessor
            || outcome.correction_application_id != correction
            || outcome.closure_id != closure
            || outcome.token_ordinal != token_ordinal
            || outcome.idempotent
            || outcome.projection.commit_current.len() != 64
        {
            bail!("recorded reconciliation result does not match its immutable application");
        }
    }
    Ok(())
}

pub(crate) fn backfill_reconciliation_results(conn: &Connection) -> Result<()> {
    let application_ids = conn
        .prepare(
            r#"
            select application.id
            from decomposition_reconciliation_applications application
            where not exists(
              select 1 from decomposition_reconciliation_results result
              where result.reconciliation_application_id=application.id
            )
            order by application.id
            "#,
        )?
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for application_id in application_ids {
        let stored = conn.query_row(
            r#"
            select application.project_id,application.successor_plan_id,
                   application.predecessor_plan_id,application.correction_application_id,
                   token.closure_id,session.id,token.id,token.token_ordinal,
                   token.operation,token.target,review_plan.work_unit_id,
                   review_plan.design_version_id,application.source_identity,
                   successor.source_path,successor.content_identity,successor.document_content
            from decomposition_reconciliation_applications application
            join correction_transition_applications correction
              on correction.id=application.correction_application_id
            join correction_sessions session on session.id=correction.correction_session_id
            join correction_tokens token on token.id=application.correction_token_id
            join closures closure on closure.id=token.closure_id
            join findings finding on finding.id=closure.finding_id
            join review_runs run on run.id=finding.review_run_id
            join review_plans review_plan on review_plan.id=run.review_plan_id
            join decomposition_plans successor on successor.id=application.successor_plan_id
            where application.id=?1
            "#,
            [application_id],
            |row| {
                Ok(StoredReconciliationApplication {
                    project: row.get(0)?,
                    successor: row.get(1)?,
                    predecessor: row.get(2)?,
                    correction_application: row.get(3)?,
                    closure: row.get(4)?,
                    session: row.get(5)?,
                    token: row.get(6)?,
                    token_ordinal: row.get(7)?,
                    operation: row.get(8)?,
                    target: row.get(9)?,
                    work: row.get(10)?,
                    design: row.get(11)?,
                    source_identity: row.get(12)?,
                    source_path: row.get(13)?,
                    content_identity: row.get(14)?,
                    content: row.get(15)?,
                })
            },
        )?;
        let metadata = fenced_metadata(&stored.content)?
            .context("stored reconciliation result has no Decomposition Plan metadata")?;
        let document: PlanDocument = yaml_serde::from_str(metadata)
            .context("stored reconciliation Plan metadata is invalid")?;
        let parsed = ParsedPlan {
            source_path: PathBuf::from(stored.source_path.unwrap_or_default()),
            design_root: PathBuf::new(),
            source_identity: stored.source_identity,
            content_identity: stored.content_identity,
            content: stored.content,
            document: Some(document),
        };
        let input = DecompositionReconciliationApplication {
            design_version_id: stored
                .design
                .context("reconciliation review has no design owner")?,
            work_unit_id: stored.work,
            plan_path: &parsed.source_path,
            closure_id: stored.closure,
            expected_current: "",
        };
        let projection = resolve_reconciliation_projection(
            conn,
            &parsed,
            &input,
            reconciliation_correction_handle(
                stored.closure,
                stored.session,
                stored.token,
                stored.token_ordinal,
                &stored.operation,
                &stored.target,
                stored.work,
                stored.design,
            ),
        )?;
        let outcome = DecompositionReconciliationOutcome {
            plan: application_outcome(conn, stored.successor, false)?,
            predecessor_plan_id: stored.predecessor,
            closure_id: stored.closure,
            token_ordinal: stored.token_ordinal,
            correction_application_id: stored.correction_application,
            idempotent: false,
            projection,
        };
        persist_reconciliation_result(conn, stored.project, application_id, &outcome)?;
    }
    validate_reconciliation_results(conn)
}

pub(super) fn validate_reconciliation_successor(
    conn: &Connection,
    project_id: i64,
    design_version_id: i64,
    plan: &PlanDocument,
) -> Result<()> {
    let reconciliation = plan
        .reconciliation
        .as_ref()
        .context("Decomposition Plan reconciliation metadata is required")?;
    let active_requirements = string_column(
        conn,
        "select requirement_key from design_requirements where project_id=?1 and design_version_id=?2 and status='active' order by requirement_key",
        params![project_id, design_version_id],
    )?
    .into_iter()
    .collect::<BTreeSet<_>>();
    let declared_requirements = plan
        .items
        .iter()
        .flat_map(|item| item.requirements.iter().cloned())
        .collect::<BTreeSet<_>>();
    if active_requirements.is_empty() || active_requirements != declared_requirements {
        bail!("Decomposition Plan must cover every current requirement");
    }
    for item in &plan.items {
        let mut expected_gates = BTreeSet::new();
        for requirement in &item.requirements {
            for gate in string_column(
                conn,
                r#"
                select template.gate_key
                from validation_gate_templates template
                join validation_gate_template_requirements link
                  on link.validation_gate_template_id=template.id
                join design_requirements requirement on requirement.id=link.design_requirement_id
                where template.project_id=?1 and template.design_version_id=?2
                  and template.status='active' and requirement.requirement_key=?3
                order by template.gate_key
                "#,
                params![project_id, design_version_id, requirement],
            )? {
                expected_gates.insert(gate);
            }
        }
        let declared_gates = item
            .completion
            .gates
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        if expected_gates != declared_gates {
            bail!(
                "Decomposition Plan item '{}' gate coverage differs: expected [{}], declared [{}]",
                item.key,
                expected_gates.into_iter().collect::<Vec<_>>().join(","),
                declared_gates.into_iter().collect::<Vec<_>>().join(",")
            );
        }
    }

    require_unique_retained_targets(
        "task",
        reconciliation
            .tasks
            .iter()
            .filter(|mapping| mapping.disposition == "retained")
            .map(|mapping| {
                mapping
                    .item
                    .clone()
                    .expect("validated retained task target")
            }),
    )?;
    require_unique_retained_targets(
        "checklist item",
        reconciliation
            .checklist
            .iter()
            .filter(|mapping| mapping.disposition == "retained")
            .map(|mapping| {
                format!(
                    "{}/{}",
                    mapping.item.as_deref().expect("validated checklist item"),
                    mapping
                        .boundary
                        .as_deref()
                        .expect("validated checklist boundary")
                )
            }),
    )?;
    require_unique_retained_targets(
        "phase",
        reconciliation
            .phases
            .iter()
            .filter(|mapping| mapping.disposition == "retained")
            .map(|mapping| {
                mapping
                    .slice
                    .clone()
                    .expect("validated retained phase target")
            }),
    )?;
    require_unique_retained_targets(
        "phase dependency",
        reconciliation
            .dependencies
            .iter()
            .filter(|mapping| mapping.disposition == "retained")
            .map(|mapping| {
                format!(
                    "{}->{}",
                    mapping
                        .from
                        .as_deref()
                        .expect("validated dependency source"),
                    mapping.to.as_deref().expect("validated dependency target")
                )
            }),
    )?;

    let item_by_key = plan
        .items
        .iter()
        .map(|item| (item.key.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    for mapping in &reconciliation.tasks {
        if mapping.disposition != "retained"
            || normalized_effect(mapping.effect) != ReconciliationEffect::Preserve
        {
            continue;
        }
        let item = item_by_key[mapping.item.as_deref().expect("validated task target")];
        let (title, details, outcome): (String, String, String) = conn.query_row(
            "select title,coalesce(details,''),coalesce(completion_condition,'') from tasks where id=?1",
            [mapping.source],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        let source_requirements = conn
            .prepare(
                r#"
                select distinct requirement.requirement_key,requirement.requirement_hash
                from task_derivations derivation
                join design_requirements requirement on requirement.id=derivation.design_requirement_id
                where derivation.task_id=?1 and derivation.status='active'
                order by requirement.requirement_key,requirement.requirement_hash
                "#,
            )?
            .query_map([mapping.source], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut target_requirements = Vec::new();
        for requirement in &item.requirements {
            target_requirements.push(conn.query_row(
                "select requirement_key,requirement_hash from design_requirements where project_id=?1 and design_version_id=?2 and requirement_key=?3 and status='active'",
                params![project_id, design_version_id, requirement],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )?);
        }
        target_requirements.sort();
        if title != item.title
            || details != item.details
            || outcome != item.completion.outcome
            || source_requirements != target_requirements
        {
            bail!(
                "preserve task effect differs for source {} and item '{}'",
                mapping.source,
                item.key
            );
        }
    }

    for mapping in &reconciliation.checklist {
        if mapping.disposition != "retained"
            || normalized_effect(mapping.effect) != ReconciliationEffect::Preserve
        {
            continue;
        }
        let item = item_by_key[mapping.item.as_deref().expect("validated checklist item")];
        let boundary = item
            .checklist
            .iter()
            .find(|boundary| Some(boundary.key.as_str()) == mapping.boundary.as_deref())
            .expect("validated checklist boundary");
        let source_meaning: String = conn.query_row(
            "select coalesce(completion_condition,title) from checklist_items where id=?1",
            [mapping.source],
            |row| row.get(0),
        )?;
        if source_meaning != boundary.condition {
            bail!("preserve checklist effect requires unchanged boundary meaning");
        }
    }

    let mut gate_targets = BTreeSet::new();
    for mapping in &reconciliation.gates {
        if mapping.disposition != "retained" {
            continue;
        }
        let boundary = resolve_gate_boundary_identity(
            conn,
            project_id,
            mapping.source,
            mapping
                .boundary
                .as_deref()
                .expect("validated retained gate boundary"),
        )?;
        let target = format!(
            "{}/{}@{boundary}",
            mapping.item.as_deref().expect("validated gate item"),
            mapping.gate.as_deref().expect("validated gate key")
        );
        if !gate_targets.insert(target) {
            bail!("retained validation gate targets must be one-to-one");
        }
        if normalized_effect(mapping.effect) != ReconciliationEffect::Preserve {
            continue;
        }
        let source_meaning = conn.query_row(
            r#"
            select gate.gate_key,gate.command,gate.expected_result,gate.environment,gate.timeout,
                   gate.artifact_requirements,requirement.requirement_key
            from validation_gates gate
            join design_requirements requirement on requirement.id=gate.design_requirement_id
            where gate.id=?1
            "#,
            [mapping.source],
            |row| {
                Ok(GateMeaning {
                    key: row.get(0)?,
                    command: row.get(1)?,
                    expected: row.get(2)?,
                    environment: row.get(3)?,
                    timeout: row.get(4)?,
                    artifacts: row.get(5)?,
                    requirement: row.get(6)?,
                })
            },
        )?;
        let target_rows = conn
            .prepare(
                r#"
                select template.gate_key,template.command,template.expected_result,
                       null,null,null,
                       requirement.requirement_key
                from validation_gate_templates template
                join validation_gate_template_requirements link
                  on link.validation_gate_template_id=template.id
                join design_requirements requirement on requirement.id=link.design_requirement_id
                where template.project_id=?1 and template.design_version_id=?2
                  and template.status='active' and template.gate_key=?3
                  and requirement.requirement_key in (
                    select value from json_each(?4)
                  )
                order by requirement.requirement_key
                "#,
            )?
            .query_map(
                params![
                    project_id,
                    design_version_id,
                    mapping.gate,
                    serde_json::to_string(
                        &item_by_key[mapping.item.as_deref().expect("validated gate item")]
                            .requirements
                    )?
                ],
                |row| {
                    Ok(GateMeaning {
                        key: row.get(0)?,
                        command: row.get(1)?,
                        expected: row.get(2)?,
                        environment: row.get(3)?,
                        timeout: row.get(4)?,
                        artifacts: row.get(5)?,
                        requirement: row.get(6)?,
                    })
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if target_rows.as_slice() != [source_meaning] {
            bail!("preserve gate effect requires unchanged validation meaning");
        }
    }

    let phase_effects = reconciliation
        .phases
        .iter()
        .map(|mapping| {
            (
                mapping.source,
                (
                    mapping.slice.as_deref(),
                    mapping.disposition.as_str(),
                    normalized_effect(mapping.effect),
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for mapping in &reconciliation.phases {
        if mapping.disposition != "retained"
            || normalized_effect(mapping.effect) != ReconciliationEffect::Preserve
        {
            continue;
        }
        let (title, kind): (String, String) = conn.query_row(
            "select title,kind from work_phases where id=?1",
            [mapping.source],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let slice = plan
            .slices
            .iter()
            .find(|slice| Some(slice.key.as_str()) == mapping.slice.as_deref())
            .expect("validated phase target");
        if title != slice.title || kind != "implementation" {
            bail!("preserve phase effect requires unchanged phase meaning");
        }
    }
    for mapping in &reconciliation.dependencies {
        if mapping.disposition != "retained"
            || normalized_effect(mapping.effect) != ReconciliationEffect::Preserve
        {
            continue;
        }
        let (from, to, dependency_type, reason, status, evidence, authority): (
            i64,
            i64,
            String,
            String,
            String,
            Option<String>,
            Option<i64>,
        ) = conn.query_row(
            "select from_phase_id,to_phase_id,dependency_type,reason,status,evidence_ref,authority_event_id from work_phase_dependencies where id=?1",
            [mapping.source],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?)),
        )?;
        let source_from = phase_effects
            .get(&from)
            .context("reconciliation dependency references a foreign predecessor phase")?;
        let source_to = phase_effects
            .get(&to)
            .context("reconciliation dependency references a foreign successor phase")?;
        let endpoints_preserved = source_from.1 == "retained"
            && source_to.1 == "retained"
            && source_from.2 == ReconciliationEffect::Preserve
            && source_to.2 == ReconciliationEffect::Preserve;
        let qualified = match status.as_str() {
            "open" => true,
            "satisfied" => evidence.is_some_and(|value| !value.trim().is_empty()),
            "accepted" => authority.is_some(),
            _ => false,
        };
        if dependency_type != "requires"
            || reason != "declared by the current Decomposition Plan"
            || (status != "open" && !endpoints_preserved)
            || !qualified
            || source_from.0 != mapping.from.as_deref()
            || source_to.0 != mapping.to.as_deref()
        {
            bail!(
                "preserve dependency effect requires current successor endpoints and qualifying evidence"
            );
        }
    }
    Ok(())
}

pub(super) fn require_unique_retained_targets(
    label: &str,
    targets: impl Iterator<Item = String>,
) -> Result<()> {
    let mut seen = BTreeSet::new();
    for target in targets {
        if !seen.insert(target) {
            bail!("retained {label} targets must be one-to-one");
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn reconciliation_correction_handle(
    closure_id: i64,
    session_id: i64,
    token_id: i64,
    token_ordinal: i64,
    operation: &str,
    target: &str,
    review_work: i64,
    review_design: Option<i64>,
) -> String {
    projection_handle(
        "correction",
        &[
            closure_id.to_string(),
            session_id.to_string(),
            token_id.to_string(),
            token_ordinal.to_string(),
            operation.to_string(),
            target.to_string(),
            review_work.to_string(),
            review_design.map_or_else(|| "-".to_string(), |value| value.to_string()),
        ],
    )
}

pub(super) fn require_reconciliation_observation(
    input: &DecompositionReconciliationApplication<'_>,
    reconciliation: &PlanReconciliation,
    projection: &DecompositionReconciliationProjection,
    preview: bool,
) -> Result<()> {
    let expected = if preview {
        &reconciliation.expected_current
    } else {
        &projection.commit_current
    };
    if input.expected_current != expected {
        if preview {
            bail!("command and Plan predecessor identities disagree");
        }
        bail!(
            "reconciliation observations changed; preview again before committing: agent-workbench decomposition reconcile {} --work {} --plan {} --closure {} --expected-current {} --dry-run",
            input.design_version_id,
            input.work_unit_id,
            shell_operand(&input.plan_path.to_string_lossy()),
            input.closure_id,
            reconciliation.expected_current,
        );
    }
    Ok(())
}

pub(super) fn reconciliation_payload_identity(
    parsed: &ParsedPlan,
    input: &DecompositionReconciliationApplication<'_>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"agent-workbench/decomposition-reconciliation-payload/v1\0");
    hasher.update(input.closure_id.to_be_bytes());
    hasher.update(input.design_version_id.to_be_bytes());
    hasher.update(input.work_unit_id.to_be_bytes());
    hasher.update(parsed.source_path.as_os_str().as_encoded_bytes());
    hasher.update(b"\0");
    hasher.update(parsed.source_identity.as_bytes());
    hasher.update(b"\0");
    if let Some(reconciliation) = parsed
        .document
        .as_ref()
        .and_then(|document| document.reconciliation.as_ref())
    {
        hasher.update(reconciliation.expected_current.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

pub(super) fn resolve_reconciliation_projection(
    conn: &Connection,
    parsed: &ParsedPlan,
    input: &DecompositionReconciliationApplication<'_>,
    observed_correction: String,
) -> Result<DecompositionReconciliationProjection> {
    let document = parsed
        .document
        .as_ref()
        .context("Decomposition Plan metadata is required for reconciliation")?;
    let reconciliation = document
        .reconciliation
        .as_ref()
        .context("Decomposition Plan reconciliation metadata is required")?;
    let mut endpoint_effects = Vec::new();
    let qualification = |disposition: &str, effect: Option<ReconciliationEffect>| match (
        disposition,
        normalized_effect(effect),
    ) {
        ("retained", ReconciliationEffect::Preserve) => "preserved_qualified",
        ("retained", ReconciliationEffect::Open) => "open",
        ("new", _) => "open",
        ("retired", _) => "historical_only",
        _ => "unqualified",
    };
    let projected_effect = |category: &str,
                            source_id: i64,
                            target: Option<String>,
                            disposition: &str,
                            effect: Option<ReconciliationEffect>,
                            reason: Option<&str>| {
        let normalized = matches!(disposition, "retained" | "new")
            .then(|| normalized_effect(effect).as_str().to_string());
        let qualification = qualification(disposition, effect).to_string();
        let observed_handle = projection_handle(
            "projected-owned-effect",
            &[
                category.to_string(),
                source_id.to_string(),
                target.clone().unwrap_or_else(|| "-".to_string()),
                disposition.to_string(),
                normalized.clone().unwrap_or_else(|| "-".to_string()),
                reason.unwrap_or("-").to_string(),
            ],
        );
        DecompositionMappingRecord {
            category: category.to_string(),
            source_id,
            target,
            disposition: disposition.to_string(),
            effect: normalized,
            qualification,
            observed_handle,
        }
    };
    for mapping in &reconciliation.tasks {
        endpoint_effects.push(projected_effect(
            "task",
            mapping.source,
            mapping.item.as_deref().map(|item| format!("item:{item}")),
            &mapping.disposition,
            mapping.effect,
            mapping.reason.as_deref(),
        ));
    }
    for mapping in &reconciliation.checklist {
        endpoint_effects.push(projected_effect(
            "checklist",
            mapping.source,
            mapping
                .item
                .as_deref()
                .zip(mapping.boundary.as_deref())
                .map(|(item, boundary)| format!("boundary:{item}/{boundary}")),
            &mapping.disposition,
            mapping.effect,
            mapping.reason.as_deref(),
        ));
    }
    for mapping in &reconciliation.gates {
        let target = if mapping.disposition == "retained" {
            Some(format!(
                "gate:{}/{}@{}",
                mapping.item.as_deref().expect("validated gate item"),
                mapping.gate.as_deref().expect("validated gate key"),
                resolve_gate_boundary_identity(
                    conn,
                    project_id(conn)?,
                    mapping.source,
                    mapping
                        .boundary
                        .as_deref()
                        .expect("validated retained gate boundary"),
                )?
            ))
        } else {
            None
        };
        endpoint_effects.push(projected_effect(
            "gate",
            mapping.source,
            target,
            &mapping.disposition,
            mapping.effect,
            mapping.reason.as_deref(),
        ));
    }
    for mapping in &reconciliation.phases {
        endpoint_effects.push(projected_effect(
            "phase",
            mapping.source,
            mapping
                .slice
                .as_deref()
                .map(|slice| format!("slice:{slice}")),
            &mapping.disposition,
            mapping.effect,
            mapping.reason.as_deref(),
        ));
    }
    for mapping in &reconciliation.dependencies {
        endpoint_effects.push(projected_effect(
            "dependency",
            mapping.source,
            mapping
                .from
                .as_deref()
                .zip(mapping.to.as_deref())
                .map(|(from, to)| format!("dependency:{from}->{to}")),
            &mapping.disposition,
            mapping.effect,
            mapping.reason.as_deref(),
        ));
    }

    let retained_targets = endpoint_effects
        .iter()
        .filter(|effect| effect.disposition == "retained")
        .filter_map(|effect| effect.target.clone())
        .collect::<BTreeSet<_>>();
    let mut add_new = |category: &str, target: String| {
        if !retained_targets.contains(&target) {
            endpoint_effects.push(projected_effect(
                category,
                0,
                Some(target),
                "new",
                Some(ReconciliationEffect::Open),
                None,
            ));
        }
    };
    for item in &document.items {
        add_new("task", format!("item:{}", item.key));
        for boundary in &item.checklist {
            add_new(
                "checklist",
                format!("boundary:{}/{}", item.key, boundary.key),
            );
        }
        for gate in &item.completion.gates {
            let prefix = format!("gate:{}/{}@", item.key, gate);
            if !retained_targets
                .iter()
                .any(|target| target.starts_with(&prefix))
            {
                add_new("gate", format!("{prefix}new"));
            }
        }
    }
    for slice in &document.slices {
        add_new("phase", format!("slice:{}", slice.key));
        for predecessor in &slice.depends_on {
            add_new(
                "dependency",
                format!("dependency:{predecessor}->{}", slice.key),
            );
        }
    }
    endpoint_effects.sort_by(|left, right| {
        (&left.category, left.source_id, &left.target).cmp(&(
            &right.category,
            right.source_id,
            &right.target,
        ))
    });

    let source_shared = decomposition_shared_bindings(conn, reconciliation.predecessor)?;
    let mut shared_bindings = Vec::with_capacity(source_shared.len() + 1);
    for mut binding in source_shared {
        let preserved = binding.qualification == "current"
            && reconciliation_preserves_shared_binding(
                conn,
                &binding,
                reconciliation,
                document,
                input.design_version_id,
            )?;
        match binding.kind.as_str() {
            "review" => {
                binding.disposition = "historical".to_string();
                binding.qualification = "historical_only".to_string();
            }
            "evidence" if preserved => {
                binding.disposition = "preserved".to_string();
                binding.qualification = "preserved_qualified".to_string();
            }
            "evidence" => {
                binding.disposition = "historical".to_string();
                if binding.qualification != "stale" {
                    binding.qualification = "historical_only".to_string();
                }
            }
            "coverage" if preserved => {
                binding.disposition = "preserved".to_string();
                binding.qualification = "preserved_current".to_string();
            }
            "coverage" if binding.qualification == "stale" => {
                binding.disposition = "historical".to_string();
            }
            "coverage" => {
                binding.disposition = "recompute".to_string();
                binding.qualification = "recompute_required".to_string();
            }
            _ => {}
        }
        shared_bindings.push(binding);
    }
    shared_bindings.push(DecompositionSharedBindingRecord {
        kind: "review".to_string(),
        id: 0,
        owner: "successor-plan".to_string(),
        disposition: "new".to_string(),
        qualification: "fresh_review_required".to_string(),
        observed_handle: projection_handle(
            "successor-review",
            &[
                parsed.source_identity.clone(),
                input.design_version_id.to_string(),
            ],
        ),
    });
    shared_bindings.sort_by(|left, right| (&left.kind, left.id).cmp(&(&right.kind, right.id)));
    let observed_shared = projection_handle(
        "shared-set",
        &shared_bindings
            .iter()
            .flat_map(|binding| {
                [
                    binding.kind.clone(),
                    binding.id.to_string(),
                    binding.owner.clone(),
                    binding.disposition.clone(),
                    binding.qualification.clone(),
                    binding.observed_handle.clone(),
                ]
            })
            .collect::<Vec<_>>(),
    );
    let mut projection_values = vec![
        reconciliation.expected_current.clone(),
        parsed.source_identity.clone(),
        observed_shared.clone(),
    ];
    projection_values.extend(
        endpoint_effects
            .iter()
            .map(|effect| effect.observed_handle.clone()),
    );
    projection_values.extend(
        shared_bindings
            .iter()
            .map(|binding| binding.observed_handle.clone()),
    );
    let projection_identity =
        projection_handle("decomposition-review-projection", &projection_values);
    let commit_current = projection_handle(
        "reconciliation-commit",
        &[
            reconciliation.expected_current.clone(),
            parsed.source_identity.clone(),
            observed_correction.clone(),
            observed_shared.clone(),
        ],
    );
    let command = format!(
        "agent-workbench decomposition reconcile {} --work {} --plan {} --closure {} --expected-current {}",
        input.design_version_id,
        input.work_unit_id,
        shell_operand(&parsed.source_path.to_string_lossy()),
        input.closure_id,
        commit_current
    );
    Ok(DecompositionReconciliationProjection {
        endpoint_effects,
        shared_bindings,
        projection_identity,
        observed_predecessor: reconciliation.expected_current.clone(),
        observed_document: parsed.source_identity.clone(),
        observed_correction,
        observed_shared,
        commit_current,
        command,
    })
}

pub(super) fn decomposition_review_projection(
    conn: &Connection,
    plan_id: i64,
    selected_design_version_id: i64,
) -> Result<Option<DecompositionReconciliationProjection>> {
    let plan = load_decomposition_plan(conn, plan_id)?;
    let (source_identity, package_root): (String, String) = conn.query_row(
        r#"
        select plan.source_identity,package.root_path
        from decomposition_plans plan
        join design_versions version on version.id=plan.design_version_id
        join design_packages package on package.id=version.design_package_id
        join design_versions selected on selected.id=?2
        where plan.id=?1 and selected.design_package_id=package.id
        "#,
        params![plan_id, selected_design_version_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let source_path = plan
        .source_path
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("owned-decomposition-plan-{plan_id}.md")));
    let mut parsed = parse_owned_plan_content(
        plan.document_content.clone(),
        source_path,
        PathBuf::from(package_root),
    )?;
    if parsed
        .document
        .as_ref()
        .and_then(|document| document.reconciliation.as_ref())
        .is_none()
    {
        return Ok(None);
    }
    parsed.source_identity = source_identity;
    let input = DecompositionReconciliationApplication {
        design_version_id: selected_design_version_id,
        work_unit_id: plan.work_unit_id,
        plan_path: &parsed.source_path,
        closure_id: 0,
        expected_current: &plan.current_identity,
    };
    resolve_reconciliation_projection(conn, &parsed, &input, "review-independent".to_string())
        .map(Some)
}

pub(crate) fn decomposition_review_projection_identity(
    conn: &Connection,
    plan_id: i64,
    selected_design_version_id: i64,
) -> Result<Option<String>> {
    let Some(projection) =
        decomposition_review_projection(conn, plan_id, selected_design_version_id)?
    else {
        return Ok(None);
    };
    Ok(Some(projection.projection_identity))
}

pub(super) fn shell_operand(value: &str) -> String {
    if !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_@%+=:,./-".contains(&byte))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(super) fn review_design_accepts(
    conn: &Connection,
    review_design: Option<i64>,
    next_design_version: i64,
) -> Result<bool> {
    let Some(review_design) = review_design else {
        return Ok(false);
    };
    conn.query_row(
        r#"
        select review.design_package_id=successor.design_package_id
        from design_versions review,design_versions successor
        where review.id=?1 and successor.id=?2
        "#,
        params![review_design, next_design_version],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub(super) fn validate_plan_package_root(
    conn: &Connection,
    project_id: i64,
    design_version_id: i64,
    design_root: &Path,
) -> Result<()> {
    let package_root: String = conn.query_row(
        r#"
        select package.root_path from design_versions version
        join design_packages package on package.id=version.design_package_id
        where version.id=?1 and version.project_id=?2
        "#,
        params![design_version_id, project_id],
        |row| row.get(0),
    )?;
    if Path::new(&package_root)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(&package_root))
        != design_root
    {
        bail!("Decomposition Plan must belong to the selected Design Package");
    }
    Ok(())
}

pub(super) fn publish_reconciliation_successor(
    conn: &Connection,
    project_id: i64,
    design_version_id: i64,
    work_unit_id: i64,
    parsed: &ParsedPlan,
) -> Result<DecompositionApplicationOutcome> {
    let document = parsed
        .document
        .as_ref()
        .context("Decomposition Plan metadata is required for reconciliation")?;
    let reconciliation = document
        .reconciliation
        .as_ref()
        .context("Decomposition Plan reconciliation metadata is required")?;
    validate_reconciliation_scope(conn, project_id, design_version_id, work_unit_id, document)?;
    let existing = conn
        .query_row(
            "select id from decomposition_plans where project_id=?1 and source_identity=?2",
            params![project_id, parsed.source_identity],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    if let Some(existing) = existing {
        let staged: bool = conn.query_row(
            "select status='ready' and predecessor_id=?2 from decomposition_plans where id=?1",
            params![existing, reconciliation.predecessor],
            |row| row.get(0),
        )?;
        if !staged {
            bail!(
                "the reconciliation Plan document was already registered without this application"
            );
        }
    }
    let predecessor_revision: i64 = conn.query_row(
        "select revision from decomposition_plans where id=?1 and project_id=?2",
        params![reconciliation.predecessor, project_id],
        |row| row.get(0),
    )?;
    let changed = conn.execute(
        "update decomposition_plans set status='superseded' where id=?1 and status in ('applied','incomplete')",
        [reconciliation.predecessor],
    )?;
    if changed != 1 {
        bail!("reconciliation predecessor changed before successor publication");
    }
    let plan_id = if let Some(existing) = existing {
        existing
    } else {
        install_discovered_plans(conn, std::slice::from_ref(parsed))?;
        let plan_id: i64 = conn.query_row(
            "select id from decomposition_plans where project_id=?1 and source_identity=?2",
            params![project_id, parsed.source_identity],
            |row| row.get(0),
        )?;
        conn.execute(
            "update decomposition_plans set predecessor_id=?1,revision=?2,status='ready',binding_issue=null where id=?3",
            params![reconciliation.predecessor, predecessor_revision + 1, plan_id],
        )?;
        plan_id
    };
    install_reconciliation_mappings(conn, project_id, plan_id, reconciliation)?;
    supersede_predecessor_endpoints(conn, reconciliation.predecessor, plan_id)?;
    validate_ready_graph(conn, project_id, plan_id, design_version_id)?;
    let retained_tasks = retained_task_targets(conn, plan_id)?;
    publish_application(
        conn,
        project_id,
        plan_id,
        design_version_id,
        work_unit_id,
        &retained_tasks,
    )?;
    validate_preserve_effects(conn, plan_id)?;
    carry_reconciliation_states(conn, project_id, plan_id)?;
    retire_reconciliation_task_identities(conn, project_id, plan_id)?;
    retire_predecessor_trace_endpoints(conn, reconciliation.predecessor)?;
    record_decomposition_lineage(conn, project_id, reconciliation.predecessor, plan_id)?;
    application_outcome(conn, plan_id, false)
}
