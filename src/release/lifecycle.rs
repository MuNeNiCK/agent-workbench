use super::*;

pub fn inspect_release_candidate(root: &Path, candidate_handle: &str) -> Result<ReleaseInspection> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let (version, reviewed_commit, work_unit_id): (String, String, Option<i64>) = conn
        .query_row(
            r#"
            select candidate.version,candidate.reviewed_commit,boundary.work_unit_id
            from release_candidates candidate
            left join release_candidate_boundaries boundary on boundary.release_candidate_id=candidate.id
            where candidate.project_id=?1 and candidate.candidate_handle=?2
            "#,
            params![project_id, candidate_handle],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .with_context(|| format!("release candidate not found: {candidate_handle}"))?;
    let current = load_current(&conn, project_id, candidate_handle)?;
    let pending = pending_attempt(&conn, current.candidate_id)?;
    let next_action = match pending.as_ref() {
        Some((_, key, _))
            if revalidate_release_work_boundary(&conn, project_id, root, current.candidate_id)
                .is_err() =>
        {
            format!(
                "agent-workbench operator release reconcile {} --expected-current {} --idempotency-key {key}-reconcile",
                current.candidate_handle, current.revision_handle
            )
        }
        Some((action, key, _)) => interrupted_next_action(&current, action, key),
        None => next_action(candidate_handle, &current),
    };
    let recover_completed_withdrawal = current.action == "reconcile"
        && current.reason.as_deref().is_some_and(|reason| {
            reason == "external step absent" || reason.starts_with("withdrawal notice conflicts")
        });
    let withdrawal_requested_identity =
        withdrawal_requested_identity(&conn, current.candidate_id, recover_completed_withdrawal)?;
    Ok(ReleaseInspection {
        candidate_handle: current.candidate_handle,
        version,
        reviewed_commit,
        work_unit_id,
        state: current.state.clone(),
        stage: current.stage.clone(),
        current_revision: current.revision_handle.clone(),
        subjects: current.subjects,
        next_action,
        withdrawal_requested_identity,
    })
}

pub(crate) fn active_release_candidates(
    conn: &Connection,
    project_id: i64,
) -> Result<Vec<ActiveReleaseCandidate>> {
    let mut statement = conn.prepare(
        r#"
        select id,candidate_handle,version
        from release_candidates
        where project_id=?1
          and (
            status not in ('remotely_verified','withdrawn','superseded')
            or exists(
              select 1 from release_candidate_attempts attempt
              where attempt.release_candidate_id=release_candidates.id
                and attempt.status='requested'
            )
          )
        order by id
        "#,
    )?;
    let candidates = statement
        .query_map([project_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    candidates
        .into_iter()
        .map(|(candidate_id, candidate_handle, version)| {
            let current = load_current(conn, project_id, &candidate_handle)?;
            let interrupted = pending_attempt(conn, current.candidate_id)?.is_some();
            let withdrawal_conflict = current.action == "reconcile"
                && current
                    .reason
                    .as_deref()
                    .is_some_and(|reason| reason.starts_with("withdrawal notice conflicts"));
            Ok(ActiveReleaseCandidate {
                candidate_id,
                candidate_handle,
                version,
                state: if withdrawal_conflict {
                    "withdrawal_conflict".to_string()
                } else if interrupted {
                    format!("{}_interrupted", current.state)
                } else {
                    current.state.clone()
                },
                description: if withdrawal_conflict {
                    "the external withdrawal notice differs from the requested notice".to_string()
                } else if interrupted {
                    "release candidate has an interrupted operation that must be observed or resumed"
                        .to_string()
                } else {
                    "release candidate has an incomplete publication lifecycle".to_string()
                },
                next_action: resolved_next_action(conn, &current)?,
            })
        })
        .collect()
}

pub fn verify_release_locally(
    root: &Path,
    candidate_handle: &str,
    expected_current: &str,
    idempotency_key: &str,
    observations: Vec<ReleaseObservation>,
) -> Result<ReleaseTransitionOutcome> {
    transition_with_observations(
        root,
        candidate_handle,
        expected_current,
        "inspect",
        idempotency_key,
        &observations,
        |current, values| {
            require_state(current, "assembled")?;
            apply_layer(&mut current.subjects, values, Layer::Local)?;
            let exact = current.subjects.iter().all(|subject| {
                !matches!(subject.kind.as_str(), "local" | "asset")
                    || subject.local_identity.as_deref() == Some(&subject.expected_identity)
            });
            Ok(if exact {
                ("locally_verified", "source", None)
            } else {
                (
                    "assembled",
                    "local",
                    Some("local identity mismatch".to_string()),
                )
            })
        },
    )
}

pub(crate) fn publish_release_source_with_action(
    root: &Path,
    candidate_handle: &str,
    expected_current: &str,
    idempotency_key: &str,
    action: &str,
    requested: Vec<ReleaseObservation>,
    observed: Vec<ReleaseObservation>,
) -> Result<ReleaseTransitionOutcome> {
    transition_with_two_observation_sets(
        ReleaseTransitionRequest {
            root,
            candidate_handle,
            expected_current,
            action,
            idempotency_key,
        },
        &requested,
        &observed,
        |current, requested, observed| {
            require_state(current, "locally_verified")?;
            apply_layer(&mut current.subjects, requested, Layer::RequestedSource)?;
            apply_layer(&mut current.subjects, observed, Layer::ObservedSource)?;
            let exact = current.subjects.iter().all(|subject| {
                subject.kind != "source"
                    || identities_equal(subject, &[Layer::RequestedSource, Layer::ObservedSource])
            });
            Ok(if exact {
                ("source_published", "assets", None)
            } else {
                (
                    "source_conflict",
                    "source",
                    Some("source identity mismatch".to_string()),
                )
            })
        },
    )
}

pub(crate) fn publish_release_assets_with_action(
    root: &Path,
    candidate_handle: &str,
    expected_current: &str,
    idempotency_key: &str,
    action: &str,
    requested: Vec<ReleaseObservation>,
    observed: Vec<ReleaseObservation>,
) -> Result<ReleaseTransitionOutcome> {
    transition_with_two_observation_sets(
        ReleaseTransitionRequest {
            root,
            candidate_handle,
            expected_current,
            action,
            idempotency_key,
        },
        &requested,
        &observed,
        |current, requested, observed| {
            require_state(current, "source_published")?;
            apply_layer(&mut current.subjects, requested, Layer::RequestedAsset)?;
            apply_layer(&mut current.subjects, observed, Layer::ObservedAsset)?;
            let exact = current
                .subjects
                .iter()
                .all(|subject| match subject.kind.as_str() {
                    "asset" => identities_equal(
                        subject,
                        &[Layer::Local, Layer::RequestedAsset, Layer::ObservedAsset],
                    ),
                    "release" => {
                        identities_equal(subject, &[Layer::RequestedAsset, Layer::ObservedAsset])
                    }
                    _ => true,
                });
            Ok(if exact {
                ("assets_published", "remote", None)
            } else {
                (
                    "asset_conflict",
                    "assets",
                    Some("asset identity mismatch".to_string()),
                )
            })
        },
    )
}

pub(crate) fn verify_release_remotely_with_action(
    root: &Path,
    candidate_handle: &str,
    expected_current: &str,
    idempotency_key: &str,
    action: &str,
    downloaded: Vec<ReleaseObservation>,
) -> Result<ReleaseTransitionOutcome> {
    transition_with_observations(
        root,
        candidate_handle,
        expected_current,
        action,
        idempotency_key,
        &downloaded,
        |current, values| {
            require_state(current, "assets_published")?;
            apply_layer(&mut current.subjects, values, Layer::Downloaded)?;
            let exact = current.subjects.iter().all(|subject| {
                subject.kind != "asset"
                    || identities_equal(
                        subject,
                        &[
                            Layer::Local,
                            Layer::RequestedAsset,
                            Layer::ObservedAsset,
                            Layer::Downloaded,
                        ],
                    )
            });
            Ok(if exact {
                ("remotely_verified", "terminal", None)
            } else {
                (
                    "asset_conflict",
                    "remote",
                    Some("downloaded identity mismatch".to_string()),
                )
            })
        },
    )
}

pub fn reconcile_release_candidate(
    root: &Path,
    candidate_handle: &str,
    expected_current: &str,
    idempotency_key: &str,
    observations: Vec<ReleaseObservation>,
) -> Result<ReleaseTransitionOutcome> {
    transition_with_observations(
        root,
        candidate_handle,
        expected_current,
        "reconcile",
        idempotency_key,
        &observations,
        |current, values| match (current.state.as_str(), current.stage.as_str()) {
            ("source_conflict", "source") => {
                apply_layer(&mut current.subjects, values, Layer::ObservedSource)?;
                let exact = current.subjects.iter().all(|subject| {
                    subject.kind != "source"
                        || identities_equal(
                            subject,
                            &[Layer::RequestedSource, Layer::ObservedSource],
                        )
                });
                if exact {
                    Ok(("source_published", "assets", None))
                } else {
                    Ok((
                        "source_conflict",
                        "source",
                        Some("source identity mismatch".to_string()),
                    ))
                }
            }
            ("asset_conflict", "assets") => {
                apply_layer(&mut current.subjects, values, Layer::ObservedAsset)?;
                let exact = current
                    .subjects
                    .iter()
                    .all(|subject| match subject.kind.as_str() {
                        "asset" => identities_equal(
                            subject,
                            &[Layer::Local, Layer::RequestedAsset, Layer::ObservedAsset],
                        ),
                        "release" => identities_equal(
                            subject,
                            &[Layer::RequestedAsset, Layer::ObservedAsset],
                        ),
                        _ => true,
                    });
                if exact {
                    Ok(("assets_published", "remote", None))
                } else {
                    Ok((
                        "asset_conflict",
                        "assets",
                        Some("asset identity mismatch".to_string()),
                    ))
                }
            }
            ("asset_conflict", "remote") => {
                apply_layer(&mut current.subjects, values, Layer::Downloaded)?;
                let exact = current.subjects.iter().all(|subject| {
                    subject.kind != "asset"
                        || identities_equal(
                            subject,
                            &[
                                Layer::Local,
                                Layer::RequestedAsset,
                                Layer::ObservedAsset,
                                Layer::Downloaded,
                            ],
                        )
                });
                if exact {
                    Ok(("remotely_verified", "terminal", None))
                } else {
                    Ok((
                        "asset_conflict",
                        "remote",
                        Some("downloaded identity mismatch".to_string()),
                    ))
                }
            }
            _ => bail!("release candidate does not have a reconcilable conflict"),
        },
    )
}

pub fn withdraw_release_candidate(
    root: &Path,
    candidate_handle: &str,
    expected_current: &str,
    idempotency_key: &str,
    reason: &str,
) -> Result<ReleaseTransitionOutcome> {
    withdraw_release_candidate_with_action(
        root,
        candidate_handle,
        expected_current,
        idempotency_key,
        "withdraw",
        reason,
    )
}

pub(crate) fn withdraw_release_candidate_with_action(
    root: &Path,
    candidate_handle: &str,
    expected_current: &str,
    idempotency_key: &str,
    action: &str,
    reason: &str,
) -> Result<ReleaseTransitionOutcome> {
    require_nonempty(reason, "withdrawal reason")?;
    transition_without_observations(
        root,
        candidate_handle,
        expected_current,
        action,
        idempotency_key,
        &[reason],
        |current| {
            if is_terminal(&current.state) {
                bail!("terminal release candidate cannot be withdrawn");
            }
            Ok(("withdrawn", "terminal", Some(reason.to_string())))
        },
    )
}

pub(crate) fn record_release_absent(
    root: &Path,
    candidate_handle: &str,
    expected_current: &str,
    idempotency_key: &str,
    expected_state: &str,
) -> Result<ReleaseTransitionOutcome> {
    transition_without_observations(
        root,
        candidate_handle,
        expected_current,
        "reconcile",
        idempotency_key,
        &["external-step-absent"],
        |current| {
            require_state(current, expected_state)?;
            let stage = match expected_state {
                "locally_verified" | "source_conflict" => "source",
                "source_published" | "asset_conflict" => "assets",
                "assets_published" => "remote",
                _ => bail!("release state does not have an external step to retry"),
            };
            Ok((
                match expected_state {
                    "locally_verified" => "locally_verified",
                    "source_conflict" => "locally_verified",
                    "source_published" => "source_published",
                    "asset_conflict" => "source_published",
                    "assets_published" => "assets_published",
                    _ => unreachable!(),
                },
                stage,
                Some("external step absent".to_string()),
            ))
        },
    )
}

pub(crate) fn record_invalid_release_attempt_rejected(
    root: &Path,
    candidate_handle: &str,
    expected_current: &str,
    idempotency_key: &str,
) -> Result<ReleaseTransitionOutcome> {
    record_release_reconciliation(
        root,
        candidate_handle,
        expected_current,
        idempotency_key,
        "invalid interrupted operation rejected",
    )
}

pub(crate) fn record_release_reconciliation(
    root: &Path,
    candidate_handle: &str,
    expected_current: &str,
    idempotency_key: &str,
    reason: &str,
) -> Result<ReleaseTransitionOutcome> {
    transition_without_observations(
        root,
        candidate_handle,
        expected_current,
        "reconcile",
        idempotency_key,
        &[reason],
        |current| {
            let state = match current.state.as_str() {
                "assembled" => "assembled",
                "locally_verified" => "locally_verified",
                "source_published" => "source_published",
                "assets_published" => "assets_published",
                "remotely_verified" => "remotely_verified",
                "source_conflict" => "source_conflict",
                "asset_conflict" => "asset_conflict",
                "withdrawn" => "withdrawn",
                "superseded" => "superseded",
                _ => bail!("unknown release candidate state"),
            };
            Ok((state, stage_for_state(state), Some(reason.to_string())))
        },
    )
}
