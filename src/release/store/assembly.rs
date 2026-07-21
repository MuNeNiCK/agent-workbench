use super::super::*;

pub fn assemble_release_candidate(
    root: &Path,
    input: NewReleaseCandidate,
) -> Result<ReleaseTransitionOutcome> {
    validate_new_candidate(&input)?;
    let mut subjects = input.subjects;
    subjects.sort_by(|left, right| (&left.kind, &left.name).cmp(&(&right.kind, &right.name)));
    let manifest_identity = manifest_identity(&input.version, &input.reviewed_commit, &subjects);
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction()?;
    let project_id = project_id(&tx)?;
    let boundary = crate::work::resolve_release_work_boundary(
        &tx,
        project_id,
        root,
        input.work_unit_id,
        &input.reviewed_commit,
    )?;
    let candidate_handle = digest_handle(
        "release_",
        b"agent-workbench/release-candidate/v2\0",
        &[
            input.version.as_bytes(),
            input.reviewed_commit.as_bytes(),
            manifest_identity.as_bytes(),
            boundary.identity.as_bytes(),
        ],
    );
    let request_identity = request_identity(
        "assemble",
        "none",
        &[
            &input.idempotency_key,
            &manifest_identity,
            &boundary.identity,
        ],
    );

    if let Some(existing) = tx
        .query_row(
            r#"
            select candidate.candidate_handle,candidate.version,candidate.reviewed_commit,
                   candidate.manifest_identity,boundary.work_unit_id,boundary.boundary_identity
            from release_candidates candidate
            left join release_candidate_boundaries boundary
              on boundary.release_candidate_id=candidate.id
            where candidate.project_id=?1 and candidate.idempotency_key=?2
            "#,
            params![project_id, input.idempotency_key],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .optional()?
    {
        if existing
            != (
                candidate_handle.clone(),
                input.version.clone(),
                input.reviewed_commit.clone(),
                manifest_identity.clone(),
                Some(boundary.work_unit_id),
                Some(boundary.identity.clone()),
            )
        {
            bail!("release idempotency key is already bound to a different candidate");
        }
        let current = load_current(&tx, project_id, &candidate_handle)?;
        tx.commit()?;
        return Ok(outcome(&current, true));
    }

    tx.execute(
        r#"
        insert into release_candidates(
          project_id,candidate_handle,version,reviewed_commit,manifest_identity,status,
          predecessor_id,idempotency_key,created_at,updated_at
        ) values(?1,?2,?3,?4,?5,'assembled',null,?6,current_timestamp,current_timestamp)
        "#,
        params![
            project_id,
            candidate_handle,
            input.version,
            input.reviewed_commit,
            manifest_identity,
            input.idempotency_key
        ],
    )?;
    let candidate_id = tx.last_insert_rowid();
    tx.execute(
        r#"
        insert into release_candidate_boundaries(
          project_id,release_candidate_id,work_unit_id,activation_id,design_version_id,
          repository_snapshot_id,reviewed_commit,boundary_identity,created_at
        ) values(?1,?2,?3,?4,?5,?6,?7,?8,current_timestamp)
        "#,
        params![
            project_id,
            candidate_id,
            boundary.work_unit_id,
            boundary.activation_id,
            boundary.design_version_id,
            boundary.repository_snapshot_id,
            input.reviewed_commit,
            boundary.identity,
        ],
    )?;
    let revision_handle = revision_handle(&candidate_handle, 1, "assembled", &request_identity);
    tx.execute(
        r#"
        insert into release_candidate_revisions(
          project_id,release_candidate_id,revision_handle,revision,state,stage,action,
          request_identity,predecessor_id,head_state,reason,created_at
        ) values(?1,?2,?3,1,'assembled','local','assemble',?4,null,'current',null,current_timestamp)
        "#,
        params![project_id, candidate_id, revision_handle, request_identity],
    )?;
    let revision_id = tx.last_insert_rowid();
    for subject in subjects {
        insert_subject(
            &tx,
            project_id,
            revision_id,
            &ReleaseSubjectRecord {
                kind: subject.kind,
                name: subject.name,
                expected_identity: subject.expected_identity,
                local_identity: None,
                requested_identity: None,
                observed_identity: None,
                downloaded_identity: None,
            },
        )?;
    }
    let current = load_current(&tx, project_id, &candidate_handle)?;
    tx.commit()?;
    Ok(outcome(&current, false))
}
