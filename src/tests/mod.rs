use super::*;
use crate::db::{SCHEMA_VERSION, open_ledger};
use crate::identity::{CanonicalValue, PrincipalHandle, domain_digest};
use rusqlite::params;
use std::fs;

mod authority;
mod database;
mod design;
mod governance;
mod identity;
mod migration;
mod repository;
mod review;
mod work;

fn add_review_run(
    root: &std::path::Path,
    input: NewReviewRun<'_>,
) -> anyhow::Result<ReviewRunOutcome> {
    let accepted = input.clean_run
        && input.status == "completed"
        && matches!(input.review_provenance, "external_agent" | "human_review")
        && input.review_provenance_ref.is_some();
    let outcome = crate::review::add_review_run(root, input)?;
    if accepted {
        record_accepted_review_claim(root, outcome.review_run_id);
    }
    Ok(outcome)
}

fn record_accepted_review_claim(root: &std::path::Path, run_id: i64) {
    let conn = open_ledger(&crate::default_ledger_path(root)).unwrap();
    let (project,plan,work,target):(i64,i64,i64,String)=conn.query_row("select r.project_id,r.review_plan_id,p.work_unit_id,coalesce(r.target_ref,'test-context') from review_runs r join review_plans p on p.id=r.review_plan_id where r.id=?1",params![run_id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).unwrap();
    let reviewer = PrincipalHandle::derive(
        b"agent-workbench:test-reviewer-v1\0",
        &CanonicalValue::object([("project", CanonicalValue::Integer(project))]),
    );
    let adjudicator = PrincipalHandle::derive(
        b"agent-workbench:test-adjudicator-v1\0",
        &CanonicalValue::object([("project", CanonicalValue::Integer(project))]),
    );
    for (handle, kind, digest) in [
        (
            reviewer.as_str(),
            "agent",
            domain_digest(b"test-reviewer\0", &CanonicalValue::Integer(project)),
        ),
        (
            adjudicator.as_str(),
            "human",
            domain_digest(b"test-adjudicator\0", &CanonicalValue::Integer(project)),
        ),
    ] {
        conn.execute("insert or ignore into authority_principals(project_id,principal_handle,provider,subject_kind,subject_digest,created_at) values(?1,?2,'signed-envelope-v1',?3,?4,current_timestamp)",params![project,handle,kind,digest]).unwrap();
    }
    let assertion_digest = domain_digest(
        b"agent-workbench:test-assertion-v1\0",
        &CanonicalValue::Integer(run_id),
    );
    conn.execute("insert into authority_assertions(project_id,provider,purpose,assertion_digest,assertion_id,nonce,key_id,subject_kind,subject_digest,project_digest,trust_digest,payload_digest,payload_cbor,envelope_cbor,issued_at,expires_at,consumed_at,created_at) values(?1,'signed-envelope-v1','review_provenance',?2,?3,?4,?5,'agent',?6,?7,?8,?9,x'a0',x'a0','0','4102444800',current_timestamp,current_timestamp)",params![project,assertion_digest,format!("test-assertion-{run_id}"),format!("test-nonce-{run_id}"),"00".repeat(16),domain_digest(b"test-reviewer\0",&CanonicalValue::Integer(project)),"11".repeat(32),"22".repeat(32),domain_digest(b"test-payload\0",&CanonicalValue::Integer(run_id))]).unwrap();
    let provenance = format!(
        "review_provenance_{}",
        domain_digest(b"test-provenance\0", &CanonicalValue::Integer(run_id))
    );
    conn.execute("insert into review_provenance_records(project_id,provenance_handle,principal_id,assertion_id,review_plan_id,target_context,provenance_kind,review_purpose,reference_digest,idempotency_key,payload_digest,created_at) values(?1,?2,(select id from authority_principals where project_id=?1 and principal_handle=?3),(select id from authority_assertions where project_id=?1 and assertion_digest=?4),?5,?6,'external_agent','new_unbiased_review',?7,?8,?9,current_timestamp)",params![project,provenance,reviewer.as_str(),assertion_digest,plan,target,domain_digest(b"test-reference\0",&CanonicalValue::Integer(run_id)),format!("test-provenance-{run_id}"),domain_digest(b"test-provenance-payload\0",&CanonicalValue::Integer(run_id))]).unwrap();
    conn.execute("update review_agent_invocations set reviewer_principal_id=(select id from authority_principals where project_id=?1 and principal_handle=?2),review_provenance_id=(select id from review_provenance_records where project_id=?1 and provenance_handle=?3) where project_id=?1 and review_run_id=?4",params![project,reviewer.as_str(),provenance,run_id]).unwrap();
    let owner = format!("work_unit:{work}");
    let grant = format!(
        "grant_{}",
        domain_digest(b"test-grant\0", &CanonicalValue::string(&owner))
    );
    let capability = format!(
        "capability_{}",
        domain_digest(b"test-capability\0", &CanonicalValue::Integer(run_id))
    );
    let decision = format!(
        "decision_{}",
        domain_digest(b"test-decision\0", &CanonicalValue::Integer(run_id))
    );
    conn.execute("insert or ignore into owner_decision_grants(project_id,grant_handle,owner_ref,grantor_principal_id,grantee_principal_id,maximum_target,roles,decision_families,actions,maximum_depth,expires_at,assertion_id,status,created_at) values(?1,?2,?3,(select id from authority_principals where project_id=?1 and principal_handle=?4),(select id from authority_principals where project_id=?1 and principal_handle=?4),'owner_all','review_adjudicator','review','adjudicate',0,'2099-01-01T00:00:00Z',(select id from authority_assertions where project_id=?1 and assertion_digest=?5),'active',current_timestamp)",params![project,grant,owner,adjudicator.as_str(),assertion_digest]).unwrap();
    conn.execute("insert into decision_capabilities(project_id,capability_handle,owner_grant_id,issuer_principal_id,holder_principal_id,owner_ref,target_ref,role,decision_family,action,design_context,assertion_id,expires_at,status,consumed_at,created_at) values(?1,?2,(select id from owner_decision_grants where project_id=?1 and grant_handle=?3),(select id from authority_principals where project_id=?1 and principal_handle=?4),(select id from authority_principals where project_id=?1 and principal_handle=?4),?5,?6,'review_adjudicator','review','adjudicate',?7,(select id from authority_assertions where project_id=?1 and assertion_digest=?8),'2099-01-01T00:00:00Z','consumed',current_timestamp,current_timestamp)",params![project,capability,grant,adjudicator.as_str(),owner,format!("review_run:{run_id}"),"66".repeat(32),assertion_digest]).unwrap();
    conn.execute("insert into owner_decisions(project_id,decision_handle,capability_id,principal_id,owner_ref,target_ref,decision_family,action,decision_value,reason,expected_current,payload_digest,created_at) values(?1,?2,(select id from decision_capabilities where project_id=?1 and capability_handle=?3),(select id from authority_principals where project_id=?1 and principal_handle=?4),?5,?6,'review','adjudicate','accepted','test accepted claim','pending',?7,current_timestamp)",params![project,decision,capability,adjudicator.as_str(),owner,format!("review_run:{run_id}"),domain_digest(b"test-decision-payload\0",&CanonicalValue::Integer(run_id))]).unwrap();
    conn.execute("insert into review_adjudication_decisions(project_id,owner_decision_id,review_run_id,value,created_at) values(?1,(select id from owner_decisions where project_id=?1 and decision_handle=?2),?3,'accepted',current_timestamp)",params![project,decision,run_id]).unwrap();
    conn.execute(
        "update review_plans set status='clean' where project_id=?1 and id=?2 and (select count(*) from review_runs r where r.review_plan_id=?2 and r.run_type='fresh' and r.clean_run=1 and exists(select 1 from review_adjudication_decisions d where d.review_run_id=r.id and d.value='accepted')) >= (select required_consecutive_clean_fresh_runs from review_policies where id=review_plans.review_policy_id) and (select count(*) from review_runs r where r.review_plan_id=?2 and r.run_type='resume' and r.clean_run=1 and exists(select 1 from review_adjudication_decisions d where d.review_run_id=r.id and d.value='accepted')) >= (select required_consecutive_clean_resume_runs from review_policies where id=review_plans.review_policy_id)",
        params![project, plan],
    )
    .unwrap();
}

fn requirement_doc(key: &str, title: &str, priority: &str) -> String {
    format!(
        r#"## {key}: {title}
```yaml agent-workbench
type: requirement
key: {key}
priority: {priority}
surfaces: [cli, database]
validation: [GATE-001]
status: active
```

This requirement describes one verifiable behavior that must be implemented.
"#
    )
}

fn requirement_doc_without_validation(key: &str, title: &str, priority: &str) -> String {
    format!(
        r#"## {key}: {title}
```yaml agent-workbench
type: requirement
key: {key}
priority: {priority}
surfaces: [cli, database]
status: active
```

This requirement describes behavior whose validation is intentionally unresolved.
"#
    )
}

fn decision_doc() -> String {
    r#"## DEC-001: Keep project-local ledger
```yaml agent-workbench
type: decision
key: DEC-001
status: accepted
supersedes: []
```

Use one SQLite ledger per project.
"#
    .to_string()
}

fn validation_gate_doc(key: &str) -> String {
    format!(
        r#"## {key}: Unit test command
```yaml agent-workbench
type: validation_gate_template
key: {key}
applies_to: [REQ-001]
expected_result: pass
phase: implementation
status: active
```

Run the project test suite before implementation handoff.
"#
    )
}

fn approval_authority_event(root: &std::path::Path) -> i64 {
    add_authority_event(
        root,
        NewAuthorityEvent {
            event_type: "user_instruction",
            source: Some("test-user"),
            summary: "approve exception for test",
            scope: Some("test"),
            precedence: 100,
        },
    )
    .unwrap()
    .authority_event_id
}

fn record_close_evidence(
    root: &std::path::Path,
    work_unit_id: i64,
    activation_id: i64,
) -> RepositorySnapshotOutcome {
    create_work_record(
        root,
        NewWorkRecord {
            work_unit_id: Some(work_unit_id),
            topic: "close evidence",
            work_performed: Some("recorded close readiness evidence"),
            next_actions: None,
            notable_operations: None,
            export_path: None,
        },
    )
    .unwrap();
    if list_repositories(root)
        .unwrap()
        .iter()
        .all(|repo| repo.name != "main")
    {
        add_repository(
            root,
            NewRepository {
                name: "main",
                path: ".",
                current_head: Some("abc123"),
                status_summary: Some("clean"),
            },
        )
        .unwrap();
    }
    add_repository_snapshot(
        root,
        NewRepositorySnapshot {
            repository: "main",
            work_unit_activation_id: Some(activation_id),
            head_sha: Some("abc123"),
            branch: Some("master"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap()
}
