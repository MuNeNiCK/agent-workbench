use super::super::*;

pub fn supersede_release_candidate(
    root: &Path,
    candidate_handle: &str,
    expected_current: &str,
    idempotency_key: &str,
    successor_handle: &str,
    reason: &str,
) -> Result<ReleaseTransitionOutcome> {
    require_nonempty(reason, "supersession reason")?;
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let successor_id: i64 = tx
        .query_row(
            "select id from release_candidates where project_id=?1 and candidate_handle=?2",
            params![project_id, successor_handle],
            |row| row.get(0),
        )
        .with_context(|| format!("successor release candidate not found: {successor_handle}"))?;
    let current = load_current(&tx, project_id, candidate_handle)?;
    if current.candidate_id == successor_id {
        bail!("release candidate cannot supersede itself");
    }
    let request = request_identity(
        "supersede",
        expected_current,
        &[idempotency_key, successor_handle, reason],
    );
    if current.revision_handle != expected_current {
        if let Some(applied) =
            find_applied_request(&tx, project_id, current.candidate_id, &request)?
        {
            tx.commit()?;
            return Ok(outcome(&applied, true));
        }
        stale_revision(candidate_handle, &current.revision_handle)?;
    }
    if is_terminal(&current.state) {
        bail!("terminal release candidate cannot be superseded");
    }
    let successor = load_current(&tx, project_id, successor_handle)?;
    if is_terminal(&successor.state) {
        bail!("terminal release candidate cannot be selected as a successor");
    }
    let successor_is_ancestor: bool = tx.query_row(
        r#"
        with recursive ancestors(id) as (
          select predecessor_id from release_candidates
          where id=?1 and predecessor_id is not null
          union
          select candidate.predecessor_id
          from release_candidates candidate
          join ancestors on ancestors.id=candidate.id
          where candidate.predecessor_id is not null
        )
        select exists(select 1 from ancestors where id=?2)
        "#,
        params![current.candidate_id, successor_id],
        |row| row.get(0),
    )?;
    if successor_is_ancestor {
        bail!("release successor would create a cyclic supersession; no release state changed");
    }
    let predecessor_id = current.candidate_id;
    let linked = tx.execute(
        "update release_candidates set predecessor_id=?1 where id=?2 and predecessor_id is null",
        params![predecessor_id, successor_id],
    )?;
    if linked == 0 {
        let existing: Option<i64> = tx.query_row(
            "select predecessor_id from release_candidates where id=?1",
            [successor_id],
            |row| row.get(0),
        )?;
        if existing != Some(predecessor_id) {
            bail!(
                "release successor is already linked to a different predecessor; no release state changed"
            );
        }
    }
    let applied = insert_revision(
        &tx,
        project_id,
        current,
        "supersede",
        &request,
        "superseded",
        "terminal",
        Some(reason),
    )?;
    tx.commit()?;
    Ok(outcome(&applied, false))
}
