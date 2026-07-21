use super::*;

pub fn operator_reconcile_release(
    root: &Path,
    input: OperatorReleaseMutation,
) -> Result<ReleaseTransitionOutcome> {
    require_key(&input.idempotency_key)?;
    let inspection = inspect_release_candidate(root, &input.candidate)?;
    let withdrawal_recovery = inspection.withdrawal_requested_identity.is_some()
        && crate::release::withdrawal_state(&inspection.state);
    let reconcilable = matches!(
        inspection.state.as_str(),
        "locally_verified" | "source_conflict" | "source_published" | "asset_conflict"
    ) || withdrawal_recovery;
    let clears_invalid_attempt = !reconcilable
        && inspection
            .next_action
            .contains("agent-workbench operator release reconcile ");
    if inspection.current_revision == input.expected_current
        && !reconcilable
        && !clears_invalid_attempt
    {
        bail!(
            "release candidate has no external publication step to reconcile; next: {}",
            inspection.next_action
        );
    }
    let requested = if withdrawal_recovery {
        inspection
            .withdrawal_requested_identity
            .clone()
            .expect("withdrawal recovery requires a requested notice identity")
    } else {
        format!(
            "{}:{}",
            inspection.state,
            expected_identity(&inspection, &["source", "release", "asset"])
        )
    };
    let attempt = start_attempt(root, &input, "reconcile", &requested)?;
    let Attempt::Ready { id, guard, .. } = attempt else {
        return attempt.completed();
    };
    let _attempt_guard = guard;
    if inspection.current_revision != input.expected_current {
        let outcome = inspection_outcome(&inspection, true);
        finish_release_attempt(root, &input.candidate, id, &requested, &outcome, true)?;
        return Ok(outcome);
    }
    if clears_invalid_attempt {
        let outcome = record_invalid_release_attempt_rejected(
            root,
            &input.candidate,
            &input.expected_current,
            &input.idempotency_key,
        )?;
        finish_release_attempt(root, &input.candidate, id, "not-applicable", &outcome, true)?;
        return Ok(outcome);
    }
    if withdrawal_recovery {
        let expected_notice = inspection
            .withdrawal_requested_identity
            .as_deref()
            .expect("withdrawal recovery requires a requested notice identity");
        return match probe_withdrawal_notice(root, &inspection, expected_notice)? {
            WithdrawalNoticeObservation::Absent => {
                let outcome = record_release_absent(
                    root,
                    &input.candidate,
                    &input.expected_current,
                    &input.idempotency_key,
                    &inspection.state,
                )?;
                finish_release_attempt(root, &input.candidate, id, "absent", &outcome, true)?;
                let current = inspect_release_candidate(root, &input.candidate)?;
                Ok(inspection_outcome(&current, false))
            }
            WithdrawalNoticeObservation::Exact(observed) => {
                let outcome = withdraw_release_candidate_with_action(
                    root,
                    &input.candidate,
                    &input.expected_current,
                    &input.idempotency_key,
                    "reconcile",
                    &withdrawal_reason(root, &inspection),
                )?;
                finish_release_attempt(root, &input.candidate, id, &observed, &outcome, true)?;
                Ok(outcome)
            }
            WithdrawalNoticeObservation::Conflict(observed) => {
                let outcome = record_release_reconciliation(
                    root,
                    &input.candidate,
                    &input.expected_current,
                    &input.idempotency_key,
                    "withdrawal notice conflicts with the requested notice",
                )?;
                finish_release_attempt(root, &input.candidate, id, &observed, &outcome, true)?;
                let current = inspect_release_candidate(root, &input.candidate)?;
                let mut projected = inspection_outcome(&current, false);
                projected.state = "withdrawal_conflict".to_string();
                Ok(projected)
            }
        };
    }
    let (outcome, observed_identity) = match inspection.state.as_str() {
        "locally_verified" | "source_conflict" => {
            let expected = subject(&inspection, "source", "tag")?
                .expected_identity
                .clone();
            let Some(observed) = remote_tag_commit(root, &inspection.version)? else {
                let outcome = record_release_absent(
                    root,
                    &input.candidate,
                    &input.expected_current,
                    &input.idempotency_key,
                    &inspection.state,
                )?;
                finish_release_attempt(root, &input.candidate, id, "absent", &outcome, true)?;
                return Ok(outcome);
            };
            let outcome = if inspection.state == "source_conflict" {
                reconcile_release_candidate(
                    root,
                    &input.candidate,
                    &input.expected_current,
                    &input.idempotency_key,
                    vec![observation("tag", &observed)],
                )?
            } else {
                publish_release_source_with_action(
                    root,
                    &input.candidate,
                    &input.expected_current,
                    &input.idempotency_key,
                    "reconcile",
                    vec![observation("tag", &expected)],
                    vec![observation("tag", &observed)],
                )?
            };
            (outcome, observed)
        }
        "asset_conflict" if inspection.stage == "remote" => {
            let observed = download_remote_observations(root, &inspection, &input.idempotency_key)?;
            let observed_identity = observations_identity(&observed);
            let outcome = reconcile_release_candidate(
                root,
                &input.candidate,
                &input.expected_current,
                &input.idempotency_key,
                observed,
            )?;
            (outcome, observed_identity)
        }
        "source_published" | "asset_conflict" => {
            let Some(remote) = probe_remote_release(root, &inspection.version)? else {
                let outcome = record_release_absent(
                    root,
                    &input.candidate,
                    &input.expected_current,
                    &input.idempotency_key,
                    &inspection.state,
                )?;
                finish_release_attempt(root, &input.candidate, id, "absent", &outcome, true)?;
                return Ok(outcome);
            };
            let observed = remote_observations(&remote, &inspection);
            let observed_identity = observations_identity(&observed);
            let outcome = if inspection.state == "asset_conflict" {
                reconcile_release_candidate(
                    root,
                    &input.candidate,
                    &input.expected_current,
                    &input.idempotency_key,
                    observed,
                )?
            } else {
                publish_release_assets_with_action(
                    root,
                    &input.candidate,
                    &input.expected_current,
                    &input.idempotency_key,
                    "reconcile",
                    expected_remote_observations(&inspection),
                    observed,
                )?
            };
            (outcome, observed_identity)
        }
        _ => bail!("release candidate has no external publication step to reconcile"),
    };
    finish_release_attempt(
        root,
        &input.candidate,
        id,
        &observed_identity,
        &outcome,
        true,
    )?;
    Ok(outcome)
}

pub fn operator_retry_release(
    root: &Path,
    input: OperatorReleaseMutation,
) -> Result<ReleaseTransitionOutcome> {
    require_key(&input.idempotency_key)?;
    if let Some(outcome) = replay_completed_release_attempt(
        root,
        &input.candidate,
        &input.expected_current,
        "retry",
        &input.idempotency_key,
    )? {
        return Ok(outcome);
    }
    let inspection = inspect_release_candidate(root, &input.candidate)?;
    ensure_current_revision(&inspection, &input.expected_current)?;
    if !inspection.next_action.contains(" operator release retry ") {
        bail!("release resolver did not select a retry for this candidate");
    }
    if let Some(expected_notice) = inspection.withdrawal_requested_identity.clone() {
        return retry_withdrawal_notice(root, input, inspection, &expected_notice);
    }
    match inspection.state.as_str() {
        "locally_verified" => operator_publish_release_source_with_action(root, input, "retry"),
        "source_published" => operator_publish_release_assets_with_action(root, input, "retry"),
        "assets_published" => operator_verify_release_remote_with_action(root, input, "retry"),
        _ => bail!("release resolver selected an unsupported retry state"),
    }
}

fn retry_withdrawal_notice(
    root: &Path,
    input: OperatorReleaseMutation,
    inspection: ReleaseInspection,
    expected_notice: &str,
) -> Result<ReleaseTransitionOutcome> {
    let attempt = start_attempt(root, &input, "retry", expected_notice)?;
    let Attempt::Ready { guard, .. } = attempt else {
        return attempt.completed();
    };
    let _attempt_guard = guard;
    let observed = match probe_withdrawal_notice(root, &inspection, expected_notice)? {
        WithdrawalNoticeObservation::Exact(observed) => observed,
        WithdrawalNoticeObservation::Absent => {
            publish_prepared_withdrawal_notice(root, &inspection)?;
            match probe_withdrawal_notice(root, &inspection, expected_notice)? {
                WithdrawalNoticeObservation::Exact(observed) => observed,
                WithdrawalNoticeObservation::Absent => {
                    bail!("withdrawal notice publication is not externally observable")
                }
                WithdrawalNoticeObservation::Conflict(observed) => bail!(
                    "withdrawal notice conflicts with the requested notice ({observed}); next: agent-workbench status"
                ),
            }
        }
        WithdrawalNoticeObservation::Conflict(observed) => bail!(
            "withdrawal notice conflicts with the requested notice ({observed}); next: agent-workbench status"
        ),
    };
    let reconciliation_key = format!("{}-reconcile", input.idempotency_key);
    let reconciliation = start_release_attempt(
        root,
        &input.candidate,
        &input.expected_current,
        "reconcile",
        &reconciliation_key,
        expected_notice,
    )?;
    let ReleaseAttemptStart::Ready {
        attempt_id: reconciliation_id,
        ..
    } = reconciliation
    else {
        bail!("withdrawal retry reconciliation unexpectedly completed before observation");
    };
    let outcome = withdraw_release_candidate_with_action(
        root,
        &input.candidate,
        &input.expected_current,
        &reconciliation_key,
        "reconcile",
        &withdrawal_reason(root, &inspection),
    )?;
    finish_release_attempt(
        root,
        &input.candidate,
        reconciliation_id,
        &observed,
        &outcome,
        true,
    )?;
    Ok(outcome)
}

pub fn operator_withdraw_release(
    root: &Path,
    input: OperatorReleaseAuthorityMutation,
) -> Result<ReleaseTransitionOutcome> {
    require_key(&input.idempotency_key)?;
    if input.reason.trim().is_empty() {
        bail!("withdrawal reason is required");
    }
    ensure_authority(root, input.authority_event_id, &input.candidate)?;
    let inspection = inspect_release_candidate(root, &input.candidate)?;
    if inspection.current_revision == input.expected_current
        && !crate::release::withdrawal_state(&inspection.state)
    {
        bail!(
            "release resolver does not allow withdrawal in the current state; next: {}",
            inspection.next_action
        );
    }
    let mutation = OperatorReleaseMutation {
        candidate: input.candidate.clone(),
        expected_current: input.expected_current.clone(),
        idempotency_key: input.idempotency_key.clone(),
    };
    let requested = prepare_withdrawal_notice(root, &inspection, &input.reason)?;
    let attempt = start_attempt(root, &mutation, "withdraw", &requested)?;
    let Attempt::Ready { id, guard, .. } = attempt else {
        return attempt.completed();
    };
    let _attempt_guard = guard;
    if inspection.current_revision != input.expected_current {
        let outcome = inspection_outcome(&inspection, true);
        finish_release_attempt(root, &input.candidate, id, &requested, &outcome, false)?;
        return Ok(outcome);
    }
    ensure_current_revision(&inspection, &input.expected_current)?;
    debug_assert!(crate::release::withdrawal_state(&inspection.state));
    let observed = match probe_withdrawal_notice(root, &inspection, &requested)? {
        WithdrawalNoticeObservation::Exact(observed) => observed,
        WithdrawalNoticeObservation::Absent => {
            publish_prepared_withdrawal_notice(root, &inspection)?;
            match probe_withdrawal_notice(root, &inspection, &requested)? {
                WithdrawalNoticeObservation::Exact(observed) => observed,
                WithdrawalNoticeObservation::Absent => {
                    bail!("withdrawal notice publication is not externally observable")
                }
                WithdrawalNoticeObservation::Conflict(observed) => bail!(
                    "withdrawal notice conflicts with the requested notice ({observed}); next: agent-workbench status"
                ),
            }
        }
        WithdrawalNoticeObservation::Conflict(observed) => bail!(
            "withdrawal notice conflicts with the requested notice ({observed}); next: agent-workbench status"
        ),
    };
    let outcome = withdraw_release_candidate(
        root,
        &input.candidate,
        &input.expected_current,
        &input.idempotency_key,
        &input.reason,
    )?;
    finish_release_attempt(root, &input.candidate, id, &observed, &outcome, false)?;
    Ok(outcome)
}

pub fn operator_supersede_release(
    root: &Path,
    input: OperatorReleaseSupersession,
) -> Result<ReleaseTransitionOutcome> {
    require_key(&input.idempotency_key)?;
    ensure_authority(root, input.authority_event_id, &input.candidate)?;
    supersede_release_candidate(
        root,
        &input.candidate,
        &input.expected_current,
        &input.idempotency_key,
        &input.successor,
        &input.reason,
    )
}
