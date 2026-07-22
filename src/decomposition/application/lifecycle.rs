use super::super::*;

pub fn apply_decomposition_plan(
    root: &Path,
    input: DecompositionApplication<'_>,
) -> Result<DecompositionApplicationOutcome> {
    let resolution = resolve_decomposition_plan(
        root,
        DecompositionPlanQuery {
            design_version_id: input.design_version_id,
            work_unit_id: input.work_unit_id,
        },
    )?;
    let Some(current) = resolution.current.as_ref() else {
        let idempotency_key = format!(
            "apply-import-{}-{}",
            input.design_version_id, input.work_unit_id
        );
        let imported = import_decomposition_plan(
            root,
            DecompositionImport {
                design_version_id: input.design_version_id,
                work_unit_id: input.work_unit_id,
                plan_path: input.plan_path,
                expected_content: None,
                draft: false,
                expected_current: "absent",
                idempotency_key: &idempotency_key,
            },
        )?;
        let conn = open_existing_project(root)?;
        return application_outcome(&conn, imported.plan.id, false);
    };
    if matches!(current.status.as_str(), "draft" | "incomplete") {
        let Some(plan_path) = input.plan_path else {
            let conn = open_existing_project(root)?;
            return application_outcome(&conn, current.id, false);
        };
        let idempotency_key = format!("apply-revise-{}-{}", current.id, current.revision);
        let revised = revise_decomposition_plan_request(
            root,
            DecompositionReviseRequest {
                plan_id: current.id,
                plan_path: Some(plan_path),
                expected_content: None,
                draft: false,
                expected_current: &current.current_identity,
                idempotency_key: &idempotency_key,
            },
        )?;
        let conn = open_existing_project(root)?;
        return application_outcome(&conn, revised.plan.id, false);
    }
    let supplied = input
        .plan_path
        .map(|path| parse_plan_unvalidated(root, path))
        .transpose()?;
    if supplied
        .as_ref()
        .is_some_and(|parsed| parsed.content_identity != current.content_identity)
    {
        let conn = open_existing_project(root)?;
        return application_outcome(&conn, current.id, current.status == "applied");
    }
    if current.status == "applied" {
        let conn = open_existing_project(root)?;
        return application_outcome(&conn, current.id, true);
    }
    if current.status != "ready" {
        let conn = open_existing_project(root)?;
        return application_outcome(&conn, current.id, false);
    }
    let review_owner = resolution
        .review_owner
        .as_ref()
        .context("ready Decomposition Plan has no exact review-owner resolution")?;
    if review_owner.state != PlanReviewOwnerState::AcceptedClean {
        let conn = open_existing_project(root)?;
        return application_outcome(&conn, current.id, false);
    }
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    validate_application_owner(&tx, project_id, input.design_version_id, input.work_unit_id)?;
    let plan_id =
        resolve_current_plan_id(&tx, project_id, input.design_version_id, input.work_unit_id)?
            .context("current Decomposition Plan not found")?;
    if plan_id != current.id {
        bail!("Decomposition Plan current owner changed before application");
    }
    let status: String = tx.query_row(
        r#"
        select plan.status from decomposition_plans plan
        join design_versions plan_version on plan_version.id=plan.design_version_id
        join design_versions selected_version on selected_version.id=?3
        where plan.id=?1 and plan.project_id=?2 and plan.work_unit_id=?4
          and plan_version.design_package_id=selected_version.design_package_id
        "#,
        params![
            plan_id,
            project_id,
            input.design_version_id,
            input.work_unit_id
        ],
        |row| row.get(0),
    )?;
    if status == "applied" {
        let outcome = application_outcome(&tx, plan_id, true)?;
        tx.commit()?;
        return Ok(outcome);
    }
    if status != "ready" {
        bail!("current Decomposition Plan is not structurally ready");
    }
    let review_target = plan_review_target(&tx, current, input.design_version_id)?;
    require_accepted_decomposition_plan_review(
        &tx,
        project_id,
        &review_target,
        &review_owner.observed_handle,
    )?;
    validate_ready_graph(&tx, project_id, plan_id, input.design_version_id)?;
    validate_empty_application_target(
        &tx,
        project_id,
        input.design_version_id,
        input.work_unit_id,
    )?;
    publish_application(
        &tx,
        project_id,
        plan_id,
        input.design_version_id,
        input.work_unit_id,
        &BTreeMap::new(),
    )?;
    let outcome = application_outcome(&tx, plan_id, false)?;
    tx.commit()?;
    Ok(outcome)
}

pub(in crate::decomposition) fn transition_decomposition_plan(
    root: &Path,
    plan_id: i64,
    replacement: Option<&Path>,
    expected_content: Option<&str>,
    draft: bool,
    expected_current: &str,
    idempotency_key: &str,
) -> Result<DecompositionPlanTransitionOutcome> {
    let preflighted = replacement
        .map(|path| parse_plan_unvalidated(root, path))
        .transpose()?;
    if let Some(expected) = expected_content {
        require_digest(expected, "decomposition expected content")?;
        let parsed = preflighted
            .as_ref()
            .context("--expected-content requires --plan")?;
        if parsed.source_identity != expected {
            bail!("Decomposition Plan ingress content changed before revision");
        }
    }
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let predecessor = load_decomposition_plan(&tx, plan_id)?;
    let mut parsed = match preflighted {
        Some(parsed) => parsed,
        None => parsed_owned_predecessor(&tx, project_id, &predecessor)?,
    };
    let document = parsed
        .document
        .clone()
        .context("Decomposition Plan metadata is required")?;
    validate_plan_header(&document)?;
    let revision = predecessor.revision + 1;
    let operation = if replacement.is_some() {
        "revise"
    } else {
        "validate"
    };
    let ingress_identity = parsed.source_identity.clone();
    parsed.source_identity = lifecycle_source_identity(LifecycleSourceIdentity {
        operation,
        idempotency_key,
        expected_current,
        predecessor_id: plan_id,
        revision,
        draft,
        source_path: &parsed.source_path,
        source_identity: &ingress_identity,
    });
    if predecessor.status == "superseded" {
        if let Some(successor_id) =
            plan_by_source_identity(&tx, project_id, &parsed.source_identity)?
        {
            let plan = load_decomposition_plan(&tx, successor_id)?;
            tx.commit()?;
            return Ok(DecompositionPlanTransitionOutcome {
                plan,
                idempotent: true,
            });
        }
        let successor_id = tx
            .query_row(
                "select id from decomposition_plans where project_id=?1 and predecessor_id=?2 and source_identity=?3",
                params![project_id, plan_id, parsed.source_identity],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        let Some(successor_id) = successor_id else {
            let current_design: i64 = tx.query_row(
                r#"
                select package.current_design_version_id
                from design_versions version
                join design_packages package on package.id=version.design_package_id
                where version.id=?1
                "#,
                [predecessor.design_version_id],
                |row| row.get(0),
            )?;
            let current_id =
                resolve_current_plan_id(&tx, project_id, current_design, predecessor.work_unit_id)?
                    .context("superseded Decomposition Plan has no package-lineage successor")?;
            let current = load_decomposition_plan(&tx, current_id)?;
            let current_successor = (current.status == "applied")
                .then(|| load_ready_successor(&tx, project_id, current.id, current_design))
                .transpose()?
                .flatten();
            let candidate_issue = validate_plan(&document)
                .err()
                .map(|error| error.to_string());
            let candidate = DecompositionPlanCandidate {
                source_path: parsed.source_path.to_string_lossy().into_owned(),
                ingress_identity,
                content_identity: parsed.content_identity,
                managed_content_identity: plan_content_identity(&canonical_plan_content(
                    &document,
                )?),
                structurally_ready: candidate_issue.is_none(),
                issue: candidate_issue,
            };
            let action_resolution = decomposition_actions(
                &tx,
                &DecompositionPlanQuery {
                    design_version_id: current_design,
                    work_unit_id: predecessor.work_unit_id,
                },
                Some(&current),
                current_successor.as_ref(),
                std::slice::from_ref(&candidate),
                current_successor
                    .as_ref()
                    .or(Some(&current))
                    .filter(|plan| matches!(plan.status.as_str(), "ready" | "applied"))
                    .map(|plan| resolve_plan_review_owner(&tx, project_id, plan, current_design))
                    .transpose()?
                    .as_ref(),
            )?;
            let action_owner = current_successor.as_ref().unwrap_or(&current);
            bail!(
                "Decomposition Plan retry payload differs from the recorded successor; successor: {}; current: {}; next: {}",
                action_owner.id,
                action_owner.current_identity,
                action_resolution.actions.join(" | ")
            );
        };
        let plan = load_decomposition_plan(&tx, successor_id)?;
        tx.commit()?;
        return Ok(DecompositionPlanTransitionOutcome {
            plan,
            idempotent: true,
        });
    }
    if predecessor.current_identity != expected_current {
        bail!(
            "Decomposition Plan changed; current handle: {}",
            predecessor.current_identity
        );
    }
    if predecessor.status == "applied" {
        if replacement.is_none() || draft {
            bail!("an applied Decomposition Plan requires a non-draft successor document");
        }
        let outcome = stage_applied_reconciliation_successor(
            &tx,
            project_id,
            &predecessor,
            parsed,
            &ingress_identity,
            None,
        )?;
        tx.commit()?;
        return Ok(outcome);
    }
    let reconciliation_predecessor = if predecessor.status == "ready" {
        let metadata = fenced_metadata(&predecessor.document_content)?
            .context("ready Decomposition Plan has no stored metadata")?;
        let document: PlanDocument = yaml_serde::from_str(metadata)
            .context("ready Decomposition Plan metadata is invalid")?;
        document
            .reconciliation
            .as_ref()
            .map(|reconciliation| reconciliation.predecessor)
    } else {
        None
    };
    if let Some(applied_predecessor_id) = reconciliation_predecessor {
        if replacement.is_none() || draft {
            bail!("a ready reconciliation successor requires a non-draft replacement document");
        }
        if predecessor.predecessor_id != Some(applied_predecessor_id) {
            bail!("ready reconciliation successor does not select its recorded predecessor");
        }
        let applied = load_decomposition_plan(&tx, applied_predecessor_id)?;
        if applied.status != "applied" {
            bail!("ready reconciliation successor no longer has an applied predecessor");
        }
        let outcome = stage_applied_reconciliation_successor(
            &tx,
            project_id,
            &applied,
            parsed,
            &ingress_identity,
            Some(predecessor.id),
        )?;
        tx.commit()?;
        return Ok(outcome);
    }
    let revises_ordinary_ready = predecessor.status == "ready"
        && reconciliation_predecessor.is_none()
        && replacement.is_some();
    if !matches!(predecessor.status.as_str(), "draft" | "incomplete") && !revises_ordinary_ready {
        bail!(
            "only a draft or incomplete Decomposition Plan can be validated or revised; an ordinary ready Plan can only be revised"
        );
    }
    let design_version_id = resolve_design_version(&tx, project_id, &document.design_fingerprint)?;
    validate_document_binding(
        &tx,
        project_id,
        design_version_id,
        predecessor.work_unit_id,
        &parsed,
    )?;
    let same_package: bool = tx.query_row(
        r#"
        select predecessor.design_package_id=successor.design_package_id
        from design_versions predecessor,design_versions successor
        where predecessor.id=?1 and successor.id=?2
        "#,
        params![predecessor.design_version_id, design_version_id],
        |row| row.get(0),
    )?;
    if !same_package {
        bail!("a revised Decomposition Plan must remain in the predecessor package lineage");
    }
    if let Some(existing) = plan_by_source_identity(&tx, project_id, &parsed.source_identity)? {
        let plan = load_decomposition_plan(&tx, existing)?;
        tx.commit()?;
        return Ok(DecompositionPlanTransitionOutcome {
            plan,
            idempotent: true,
        });
    }
    let issue = validate_plan(&document)
        .err()
        .map(|error| error.to_string());
    let status = if draft {
        "draft"
    } else if issue.is_some() {
        "incomplete"
    } else {
        "ready"
    };
    let changed = if revises_ordinary_ready {
        tx.execute(
            "update decomposition_plans set status='superseded' where id=?1 and status='ready'",
            [plan_id],
        )?
    } else {
        tx.execute(
            "update decomposition_plans set status='superseded' where id=?1 and status in ('draft','incomplete')",
            [plan_id],
        )?
    };
    if changed != 1 {
        bail!("Decomposition Plan changed before successor publication");
    }
    let successor_id = insert_lifecycle_plan(
        &tx,
        project_id,
        design_version_id,
        predecessor.work_unit_id,
        &parsed,
        &ingress_identity,
        revision,
        Some(plan_id),
        status,
        issue.as_deref(),
    )?;
    let plan = load_decomposition_plan(&tx, successor_id)?;
    tx.commit()?;
    Ok(DecompositionPlanTransitionOutcome {
        plan,
        idempotent: false,
    })
}

pub(in crate::decomposition) fn stage_applied_reconciliation_successor(
    conn: &Connection,
    project_id: i64,
    predecessor: &DecompositionPlanRecord,
    mut parsed: ParsedPlan,
    ingress_source_identity: &str,
    replaced_ready_id: Option<i64>,
) -> Result<DecompositionPlanTransitionOutcome> {
    let mut document = parsed
        .document
        .clone()
        .context("Decomposition Plan metadata is required")?;
    let reconciliation = document
        .reconciliation
        .as_mut()
        .context("an applied Plan successor requires reconciliation metadata")?;
    if reconciliation.predecessor != predecessor.id {
        bail!("successor reconciliation does not select the applied predecessor");
    }
    reconciliation.expected_current = predecessor.current_identity.clone();
    parsed.content = canonical_plan_content(&document)?;
    parsed.content_identity = plan_content_identity(&parsed.content);
    parsed.document = Some(document.clone());

    let design_version_id = resolve_design_version(conn, project_id, &document.design_fingerprint)?;
    validate_document_binding(
        conn,
        project_id,
        design_version_id,
        predecessor.work_unit_id,
        &parsed,
    )?;
    validate_reconciliation_scope(
        conn,
        project_id,
        design_version_id,
        predecessor.work_unit_id,
        &document,
    )?;
    validate_reconciliation_successor(conn, project_id, design_version_id, &document)?;

    if let Some(existing) = plan_by_source_identity(conn, project_id, &parsed.source_identity)? {
        let plan = load_decomposition_plan(conn, existing)?;
        if plan.status != "ready" || plan.predecessor_id != Some(predecessor.id) {
            bail!("Decomposition Plan revision identity belongs to another lifecycle transition");
        }
        return Ok(DecompositionPlanTransitionOutcome {
            plan,
            idempotent: true,
        });
    }

    let revision: i64 = conn.query_row(
        r#"
        select coalesce(max(candidate.revision),?2)+1
        from decomposition_plans candidate
        join design_versions candidate_version on candidate_version.id=candidate.design_version_id
        join design_versions predecessor_version on predecessor_version.id=?3
        where candidate.project_id=?1 and candidate.work_unit_id=?4
          and candidate_version.design_package_id=predecessor_version.design_package_id
        "#,
        params![
            project_id,
            predecessor.revision,
            predecessor.design_version_id,
            predecessor.work_unit_id
        ],
        |row| row.get(0),
    )?;
    if let Some(replaced_ready_id) = replaced_ready_id {
        let changed = conn.execute(
            "update decomposition_plans set status='superseded' where id=?1 and status='ready' and predecessor_id=?2",
            params![replaced_ready_id, predecessor.id],
        )?;
        if changed != 1 {
            bail!("ready Decomposition Plan changed before successor revision");
        }
    }
    let successor_id = insert_lifecycle_plan(
        conn,
        project_id,
        design_version_id,
        predecessor.work_unit_id,
        &parsed,
        ingress_source_identity,
        revision,
        Some(predecessor.id),
        "ready",
        None,
    )?;
    let plan = load_decomposition_plan(conn, successor_id)?;
    Ok(DecompositionPlanTransitionOutcome {
        plan,
        idempotent: false,
    })
}
