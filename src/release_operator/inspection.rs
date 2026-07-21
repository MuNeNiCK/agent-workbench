use super::*;

pub fn operator_inspect_release(
    root: &Path,
    input: OperatorReleaseMutation,
) -> Result<ReleaseTransitionOutcome> {
    require_key(&input.idempotency_key)?;
    let inspection = inspect_release_candidate(root, &input.candidate)?;
    if inspection.current_revision == input.expected_current {
        ensure_current(&inspection, &input.expected_current, "assembled")?;
    }
    let requested = expected_identity(&inspection, &["local", "asset"]);
    let attempt = start_attempt(root, &input, "inspect", &requested)?;
    let Attempt::Ready { id, guard, .. } = attempt else {
        return attempt.completed();
    };
    let _attempt_guard = guard;
    if inspection.current_revision != input.expected_current {
        let outcome = inspection_outcome(&inspection, true);
        finish_release_attempt(root, &input.candidate, id, &requested, &outcome, false)?;
        return Ok(outcome);
    }
    ensure_current(&inspection, &input.expected_current, "assembled")?;
    let directory = candidate_dir(root, &input.candidate);
    let observations = verify_local_candidate(root, &directory, &inspection)?;
    let outcome = verify_release_locally(
        root,
        &input.candidate,
        &input.expected_current,
        &input.idempotency_key,
        observations,
    )?;
    finish_release_attempt(root, &input.candidate, id, &requested, &outcome, false)?;
    Ok(outcome)
}
