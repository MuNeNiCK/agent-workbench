use super::*;

pub(crate) fn replay_completed_release_attempt(
    root: &Path,
    candidate_handle: &str,
    expected_current: &str,
    action: &str,
    idempotency_key: &str,
) -> Result<Option<ReleaseTransitionOutcome>> {
    let conn = open_existing_project(root)?;
    let project_id = project_id(&conn)?;
    let current = load_current(&conn, project_id, candidate_handle)?;
    let existing = conn
        .query_row(
            r#"
            select action,expected_current,status,result_revision_handle
            from release_candidate_attempts
            where release_candidate_id=?1 and idempotency_key=?2
            "#,
            params![current.candidate_id, idempotency_key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((bound_action, bound_current, status, result)) = existing else {
        return Ok(None);
    };
    if bound_action != action || bound_current != expected_current {
        bail!("release idempotency key is already bound to a different operation");
    }
    if status != "completed" {
        return Ok(None);
    }
    let result = result.context("completed release attempt has no result revision")?;
    let mut applied = load_revision(&conn, project_id, current.candidate_id, &result)?;
    applied.subjects = load_subjects(&conn, applied.revision_id)?;
    Ok(Some(outcome(&applied, true)))
}

pub(crate) fn start_release_attempt(
    root: &Path,
    candidate_handle: &str,
    expected_current: &str,
    action: &str,
    idempotency_key: &str,
    requested_identity: &str,
) -> Result<ReleaseAttemptStart> {
    require_nonempty(action, "release action")?;
    require_nonempty(idempotency_key, "release idempotency key")?;
    require_nonempty(requested_identity, "requested external identity")?;
    let payload_identity = request_identity(
        action,
        expected_current,
        &[idempotency_key, requested_identity],
    );
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let current = load_current(&tx, project_id, candidate_handle)?;
    let existing = tx
        .query_row(
            r#"
            select id,action,expected_current,payload_identity,requested_identity,status,
                   result_revision_handle
            from release_candidate_attempts
            where release_candidate_id=?1 and idempotency_key=?2
            "#,
            params![current.candidate_id, idempotency_key],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            },
        )
        .optional()?;
    if let Some((id, bound_action, bound_current, bound_payload, bound_requested, status, result)) =
        existing
    {
        if bound_action != action
            || bound_current != expected_current
            || bound_payload != payload_identity
            || bound_requested != requested_identity
        {
            bail!("release idempotency key is already bound to a different operation");
        }
        if status == "completed" {
            let result = result.context("completed release attempt has no result revision")?;
            let mut applied = load_revision(&tx, project_id, current.candidate_id, &result)?;
            applied.subjects = load_subjects(&tx, applied.revision_id)?;
            tx.commit()?;
            return Ok(ReleaseAttemptStart::Completed(outcome(&applied, true)));
        }
        revalidate_release_work_boundary(&tx, project_id, root, current.candidate_id)?;
        tx.commit()?;
        return Ok(ReleaseAttemptStart::Ready {
            attempt_id: id,
            resumed: true,
        });
    }
    revalidate_release_work_boundary(&tx, project_id, root, current.candidate_id)?;
    if current.revision_handle != expected_current {
        stale_revision(candidate_handle, &current.revision_handle)?;
    }
    let pending = pending_attempt(&tx, current.candidate_id)?;
    if let Some((pending_action, pending_key, _)) = pending {
        let resolves_interrupted = (action == "reconcile" && pending_action != "reconcile")
            || (action == "retry"
                && pending_action == "withdraw"
                && current.action == "reconcile"
                && current.reason.as_deref() == Some("external step absent"));
        if resolves_interrupted {
            // The resolver selected an observation or retry for the older request.
        } else {
            let recovery = interrupted_next_action(&current, &pending_action, &pending_key);
            bail!("release candidate has an interrupted operation; next: {recovery}");
        }
    }
    tx.execute(
        r#"
        insert into release_candidate_attempts(
          project_id,release_candidate_id,action,idempotency_key,expected_current,payload_identity,
          requested_identity,observed_identity,result_revision_handle,status,created_at,completed_at
        ) values(?1,?2,?3,?4,?5,?6,?7,null,null,'requested',current_timestamp,null)
        "#,
        params![
            project_id,
            current.candidate_id,
            action,
            idempotency_key,
            expected_current,
            payload_identity,
            requested_identity
        ],
    )?;
    let attempt_id = tx.last_insert_rowid();
    tx.commit()?;
    Ok(ReleaseAttemptStart::Ready {
        attempt_id,
        resumed: false,
    })
}

pub(crate) fn finish_release_attempt(
    root: &Path,
    candidate_handle: &str,
    attempt_id: i64,
    observed_identity: &str,
    result: &ReleaseTransitionOutcome,
    finish_interrupted: bool,
) -> Result<()> {
    require_nonempty(observed_identity, "observed external identity")?;
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let current = load_current(&tx, project_id, candidate_handle)?;
    if current.revision_handle != result.current_revision {
        bail!("release candidate changed before its external attempt was recorded");
    }
    let changed = tx.execute(
        r#"
        update release_candidate_attempts
        set observed_identity=?1,result_revision_handle=?2,status='completed',completed_at=current_timestamp
        where id=?3 and project_id=?4 and release_candidate_id=?5 and status='requested'
        "#,
        params![
            observed_identity,
            result.current_revision,
            attempt_id,
            project_id,
            current.candidate_id
        ],
    )?;
    if changed != 1 {
        let completed = tx
            .query_row(
                r#"
                select observed_identity,result_revision_handle
                from release_candidate_attempts
                where id=?1 and project_id=?2 and release_candidate_id=?3 and status='completed'
                "#,
                params![attempt_id, project_id, current.candidate_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                    ))
                },
            )
            .optional()?;
        if completed
            != Some((
                Some(observed_identity.to_string()),
                Some(result.current_revision.clone()),
            ))
        {
            bail!("release attempt is not the current requested operation");
        }
        tx.commit()?;
        return Ok(());
    }
    if finish_interrupted {
        tx.execute(
            r#"
            update release_candidate_attempts
            set observed_identity=?1,result_revision_handle=?2,status='completed',completed_at=current_timestamp
            where project_id=?3 and release_candidate_id=?4 and status='requested'
            "#,
            params![
                observed_identity,
                result.current_revision,
                project_id,
                current.candidate_id
            ],
        )?;
    }
    tx.commit()?;
    Ok(())
}
