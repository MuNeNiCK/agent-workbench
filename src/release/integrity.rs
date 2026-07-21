use super::*;

pub(crate) fn validate_release_candidate_lineage(conn: &Connection) -> Result<()> {
    let invalid_lineage: i64 = conn.query_row(
        r#"
        select count(*)
        from release_candidates candidate
        left join release_candidates predecessor on predecessor.id=candidate.predecessor_id
        where (candidate.predecessor_id is not null and (
                 predecessor.id is null
                 or predecessor.project_id!=candidate.project_id
                 or predecessor.status!='superseded'
              ))
           or (candidate.status='superseded' and (
                 select count(*) from release_candidates successor
                 where successor.predecessor_id=candidate.id
              )!=1)
           or (candidate.status!='superseded' and exists(
                 select 1 from release_candidates successor
                 where successor.predecessor_id=candidate.id
              ))
        "#,
        [],
        |row| row.get(0),
    )?;
    let cyclic_lineage: bool = conn.query_row(
        r#"
        with recursive lineage(current_id,path,cycle) as (
          select id,printf('/%d/',id),0 from release_candidates
          union all
          select candidate.predecessor_id,
                 lineage.path||candidate.predecessor_id||'/',
                 instr(lineage.path,printf('/%d/',candidate.predecessor_id))>0
          from lineage
          join release_candidates candidate on candidate.id=lineage.current_id
          where candidate.predecessor_id is not null and lineage.cycle=0
        )
        select exists(select 1 from lineage where cycle=1)
        "#,
        [],
        |row| row.get(0),
    )?;
    if invalid_lineage != 0 || cyclic_lineage {
        bail!("release candidate supersession history is partial or contradictory");
    }
    Ok(())
}

pub(crate) fn migrate_release_candidate_revisions(conn: &Connection) -> Result<()> {
    validate_release_candidate_lineage(conn)?;
    let mut statement = conn.prepare(
        "select id,project_id,candidate_handle,status,manifest_identity from release_candidates order by id",
    )?;
    let candidates = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    for (candidate_id, project_id, candidate_handle, state, manifest) in candidates {
        let present: bool = conn.query_row(
            "select exists(select 1 from release_candidate_revisions where release_candidate_id=?1)",
            [candidate_id],
            |row| row.get(0),
        )?;
        if present {
            continue;
        }
        let stage = stage_for_state(&state);
        let action = if state == "superseded" {
            "supersede"
        } else {
            "migrate"
        };
        let request = request_identity(action, "generation-15", &[&candidate_handle, &state]);
        let handle = revision_handle(&candidate_handle, 1, &state, &request);
        conn.execute(
            r#"
            insert into release_candidate_revisions(
              project_id,release_candidate_id,revision_handle,revision,state,stage,action,
              request_identity,predecessor_id,head_state,reason,created_at
            ) values(?1,?2,?3,1,?4,?5,?6,?7,null,'current',
                     'preserved from the predecessor storage generation',current_timestamp)
            "#,
            params![
                project_id,
                candidate_id,
                handle,
                state,
                stage,
                action,
                request
            ],
        )?;
        let revision_id = conn.last_insert_rowid();
        let mut assets = conn.prepare(
            "select asset_name,expected_identity,local_identity,remote_identity,status from release_candidate_assets where release_candidate_id=?1 order by asset_name",
        )?;
        let rows = assets
            .query_map([candidate_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(assets);
        if rows.is_empty() {
            insert_subject_connection(
                conn,
                project_id,
                revision_id,
                &ReleaseSubjectRecord {
                    kind: "local".to_string(),
                    name: "legacy-manifest".to_string(),
                    expected_identity: manifest.clone(),
                    local_identity: (state != "assembled").then_some(manifest.clone()),
                    requested_identity: None,
                    observed_identity: None,
                    downloaded_identity: None,
                },
            )?;
        } else {
            for (name, expected, local, remote, asset_state) in rows {
                let published = matches!(
                    asset_state.as_str(),
                    "published" | "remotely_verified" | "conflict"
                );
                let downloaded = (asset_state == "remotely_verified")
                    .then(|| remote.clone())
                    .flatten();
                insert_subject_connection(
                    conn,
                    project_id,
                    revision_id,
                    &ReleaseSubjectRecord {
                        kind: "asset".to_string(),
                        name,
                        expected_identity: expected,
                        local_identity: local,
                        requested_identity: published.then(|| remote.clone()).flatten(),
                        observed_identity: remote,
                        downloaded_identity: downloaded,
                    },
                )?;
            }
        }
    }
    Ok(())
}
