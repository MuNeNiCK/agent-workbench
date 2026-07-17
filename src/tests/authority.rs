use super::*;
use crate::authority::signed_envelope::{CborValue, assemble, decode_canonical, encode_value};
use crate::identity::{CanonicalValue, PrincipalHandle};
use std::collections::BTreeMap;
use time::OffsetDateTime;

#[test]
fn init_installs_closed_authority_schema() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    for table in [
        "authority_provider_snapshots",
        "authority_assertions",
        "authority_principals",
        "owner_decision_grants",
        "decision_capabilities",
        "owner_decisions",
    ] {
        let exists: bool = conn
            .query_row(
                "select exists(select 1 from sqlite_schema where type='table' and name=?1)",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert!(exists, "missing {table}");
    }
}

#[test]
fn assembly_is_canonical_and_create_signature_field_only() {
    let mut unsigned = BTreeMap::new();
    unsigned.insert(0, CborValue::U64(1));
    unsigned.insert(1, CborValue::Text("signed-envelope-v1".into()));
    unsigned.insert(2, CborValue::Bytes(vec![1; 16]));
    unsigned.insert(3, CborValue::U64(0));
    unsigned.insert(4, CborValue::Bytes(vec![2; 16]));
    unsigned.insert(5, CborValue::Bytes(vec![3; 16]));
    unsigned.insert(6, CborValue::Text("2026-07-17T00:00:00Z".into()));
    unsigned.insert(7, CborValue::Text("2026-07-17T00:05:00Z".into()));
    unsigned.insert(
        8,
        CborValue::Map(BTreeMap::from([
            (0, CborValue::U64(0)),
            (1, CborValue::Bytes(vec![4; 32])),
        ])),
    );
    unsigned.insert(9, CborValue::Bytes(vec![5; 32]));
    unsigned.insert(10, CborValue::Map(BTreeMap::new()));
    unsigned.insert(12, CborValue::Bytes(vec![6; 32]));
    let request = encode_value(&CborValue::Map(unsigned)).unwrap();
    let envelope = assemble(&request, &[7; 64]).unwrap();
    let CborValue::Map(decoded) = decode_canonical(&envelope).unwrap() else {
        panic!()
    };
    assert_eq!(decoded.len(), 13);
    assert_eq!(decoded.get(&11), Some(&CborValue::Bytes(vec![7; 64])));
    assert!(assemble(&request, &[7; 63]).is_err());
}

#[test]
fn signed_envelope_v1_normative_root_vector_matches() {
    use crate::authority::signed_envelope::{
        CborValue, SIGNING_DOMAIN, assemble, encode_value, hex_digest, verify_envelope,
    };
    let bytes = |value: &str| {
        (0..value.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&value[index..index + 2], 16).unwrap())
            .collect::<Vec<_>>()
    };
    let map = |values: Vec<(u64, CborValue)>| values.into_iter().collect::<BTreeMap<_, _>>();
    let trust = bytes("f10521ad8a4e6db53d722c73cd8d66a35fa843ff285ae54f01ade225b398d4a0");
    let unsigned = CborValue::Map(map(vec![
        (0, CborValue::U64(1)),
        (1, CborValue::Text("signed-envelope-v1".into())),
        (
            2,
            CborValue::Bytes(bytes("000102030405060708090a0b0c0d0e0f")),
        ),
        (3, CborValue::U64(0)),
        (
            4,
            CborValue::Bytes(bytes("101112131415161718191a1b1c1d1e1f")),
        ),
        (
            5,
            CborValue::Bytes(bytes("202122232425262728292a2b2c2d2e2f")),
        ),
        (6, CborValue::U64(0)),
        (7, CborValue::U64(300)),
        (
            8,
            CborValue::Map(map(vec![
                (0, CborValue::U64(0)),
                (1, CborValue::Bytes(vec![0x11; 32])),
            ])),
        ),
        (9, CborValue::Bytes(vec![0x22; 32])),
        (
            10,
            CborValue::Map(map(vec![
                (0, CborValue::Bytes(vec![0x33; 32])),
                (1, CborValue::Array(vec![CborValue::U64(0)])),
                (2, CborValue::Array(vec![CborValue::U64(0)])),
                (3, CborValue::Array(vec![CborValue::U64(0)])),
                (4, CborValue::Array(vec![CborValue::U64(0)])),
                (5, CborValue::U64(0)),
                (6, CborValue::U64(300)),
            ])),
        ),
        (12, CborValue::Bytes(trust.clone())),
    ]));
    let request = encode_value(&unsigned).unwrap();
    assert_eq!(
        hex_digest(&request),
        "1d89566f21d8616c09afca9bab8b78453215eaa2ddfa98ba578c258da775d462"
    );
    let mut preimage = SIGNING_DOMAIN.to_vec();
    preimage.extend_from_slice(&request);
    assert_eq!(
        hex_digest(&preimage),
        "fccbd212d0059b9f0e6c056aef08b616a984baa1dc90c28a7692b579f90d8630"
    );
    let signature = bytes(
        "e13529cba2eed14799b03bfebcde868e89d5a857931703bd0ed95b9c1dd3058002f9142d978be17ee46dc53908c18dc9398088c466a4f877cefb35719315d106",
    );
    let envelope = assemble(&request, &signature).unwrap();
    assert_eq!(envelope.len(), 316);
    assert_eq!(
        hex_digest(&envelope),
        "6b1d483a47c2a5d63588a9865893c5bdab35ba81fe8b1659e25ce2fbf80f80e1"
    );
    let public_key: [u8; 32] =
        bytes("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a")
            .try_into()
            .unwrap();
    let trust: [u8; 32] = trust.try_into().unwrap();
    verify_envelope(&envelope, &public_key, &trust, OffsetDateTime::UNIX_EPOCH).unwrap();
}

#[test]
fn grant_capability_and_decision_are_one_shot() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = start_work(temp.path(), "authority projection", None).unwrap();
    let policy = add_review_policy(
        temp.path(),
        NewReviewPolicy {
            name: "authority-policy",
            review_type: "implementation_review",
            max_fresh_agents: 2,
            max_resume_agents: 1,
            max_parallel_agents: 1,
            required_consecutive_clean_fresh_runs: 1,
            required_consecutive_clean_resume_runs: 0,
            stop_on_severity: "none",
            allow_resume_review: true,
            allow_fresh_review: true,
            allow_new_findings_in_resume: false,
            on_max_agents_exceeded: "block",
            run_count_scope: "review_plan",
            default_run_mode: "fresh",
        },
    )
    .unwrap();
    let scope = start_review_scope(
        temp.path(),
        NewReviewScope {
            name: "authority-scope",
            review_type: "implementation_review",
            scope: "authority",
            allowed_inputs: None,
            forbidden_judgments: None,
            expected_output_type: None,
            exclusions: None,
            prompt_template_ref: None,
        },
    )
    .unwrap();
    let plan = add_review_plan(
        temp.path(),
        NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            review_type: "implementation_review",
            required: true,
            stage: "close-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: Some(policy.review_policy_id),
            review_scope_id: Some(scope.review_scope_id),
        },
    )
    .unwrap();
    let run = add_review_run(
        temp.path(),
        NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some("HEAD"),
            prompt_deviations: None,
            result_summary: Some("clean advisory claim"),
            new_findings_count: 0,
            carried_findings_checked: 0,
            clean_run: true,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
            review_provenance: "self_recorded",
            review_provenance_ref: None,
        },
    )
    .unwrap();
    let decision_target = format!("review_run:{}", run.review_run_id);
    let principal = PrincipalHandle::derive(
        b"agent-workbench:principal-v1\0",
        &CanonicalValue::object([
            ("provider", CanonicalValue::string("signed-envelope-v1")),
            ("subject_kind", CanonicalValue::string("human")),
            ("subject_digest", CanonicalValue::string("11".repeat(32))),
        ]),
    );
    let root_digest = "22".repeat(32);
    let capability_digest = "33".repeat(32);
    let expires = (OffsetDateTime::now_utc() + time::Duration::hours(1))
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let assertion_expires = (OffsetDateTime::now_utc() + time::Duration::minutes(5))
        .unix_timestamp()
        .to_string();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute(
        "insert into authority_principals(project_id,principal_handle,provider,subject_kind,subject_digest,created_at) values(1,?1,'signed-envelope-v1','human',?2,current_timestamp)",
        params![principal.as_str(), "11".repeat(32)],
    ).unwrap();
    let reviewer = PrincipalHandle::derive(
        b"agent-workbench:principal-v1\0",
        &CanonicalValue::object([
            ("provider", CanonicalValue::string("signed-envelope-v1")),
            ("subject_kind", CanonicalValue::string("agent")),
            ("subject_digest", CanonicalValue::string("12".repeat(32))),
        ]),
    );
    conn.execute("insert into authority_principals(project_id,principal_handle,provider,subject_kind,subject_digest,created_at) values(1,?1,'signed-envelope-v1','agent',?2,current_timestamp)",params![reviewer.as_str(),"12".repeat(32)]).unwrap();
    conn.execute("update review_agent_invocations set reviewer_principal_id=(select id from authority_principals where principal_handle=?1) where review_run_id=?2",params![reviewer.as_str(),run.review_run_id]).unwrap();
    for (digest, purpose, assertion_id, nonce) in [
        (root_digest.as_str(), "root_grant", "aa", "bb"),
        (capability_digest.as_str(), "capability_issue", "cc", "dd"),
    ] {
        let payload = if purpose == "root_grant" {
            CborValue::Map(BTreeMap::from([
                (
                    0,
                    CborValue::Bytes(
                        crate::authority::signed_envelope::digest_reference(
                            b"agent-workbench:owner-ref-v1\0",
                            "work-unit:1",
                        )
                        .to_vec(),
                    ),
                ),
                (1, CborValue::Array(vec![CborValue::U64(0)])),
                (2, CborValue::Array(vec![CborValue::U64(1)])),
                (3, CborValue::Array(vec![CborValue::U64(0)])),
                (4, CborValue::Array(vec![CborValue::U64(0)])),
                (5, CborValue::U64(0)),
                (
                    6,
                    CborValue::U64(
                        crate::authority::signed_envelope::parse_rfc3339_seconds(&expires, "expiry")
                            .unwrap() as u64,
                    ),
                ),
            ]))
        } else {
            CborValue::Map(BTreeMap::from([
                (
                    0,
                    CborValue::Bytes(
                        crate::authority::signed_envelope::digest_reference(
                            b"agent-workbench:owner-ref-v1\0",
                            "work-unit:1",
                        )
                        .to_vec(),
                    ),
                ),
                (
                    1,
                    crate::authority::signed_envelope::target_value(&decision_target).unwrap(),
                ),
                (2, CborValue::Array(vec![CborValue::U64(1)])),
                (3, CborValue::Array(vec![CborValue::U64(0)])),
                (4, CborValue::Array(vec![CborValue::U64(0)])),
                (
                    5,
                    CborValue::U64(
                        crate::authority::signed_envelope::parse_rfc3339_seconds(&expires, "expiry")
                            .unwrap() as u64,
                    ),
                ),
                (6, CborValue::Bytes(vec![0x66; 32])),
            ]))
        };
        let payload = encode_value(&payload).unwrap();
        let payload_digest = crate::authority::signed_envelope::hex_digest(&payload);
        conn.execute(
            "insert into authority_assertions(project_id,provider,purpose,assertion_digest,assertion_id,nonce,key_id,subject_kind,subject_digest,project_digest,trust_digest,payload_digest,payload_cbor,envelope_cbor,issued_at,expires_at,created_at) values(1,'signed-envelope-v1',?1,?2,?3,?4,?5,'human',?6,?7,?8,?9,?10,x'a0','0',?11,current_timestamp)",
            params![purpose,digest,assertion_id,nonce,"00".repeat(16),"11".repeat(32),"22".repeat(32),"44".repeat(32),payload_digest,payload,assertion_expires],
        ).unwrap();
    }
    let grant_handle = format!("grant_{}", "55".repeat(32));
    conn.execute("insert into owner_decision_grants(project_id,grant_handle,owner_ref,grantor_principal_id,grantee_principal_id,maximum_target,roles,decision_families,actions,maximum_depth,expires_at,assertion_id,status,created_at) values(1,?1,'work-unit:1',(select id from authority_principals where principal_handle=?2),(select id from authority_principals where principal_handle=?2),'owner_all','review_adjudicator','review','adjudicate',0,?3,(select id from authority_assertions where purpose='root_grant'),'active',current_timestamp)",params![grant_handle,principal.as_str(),expires]).unwrap();
    let capability_handle = format!("capability_{}", "77".repeat(32));
    let design_context = domain_digest(
        b"agent-workbench:target-design-context-v1\0",
        &CanonicalValue::string(&decision_target),
    );
    conn.execute("insert into decision_capabilities(project_id,capability_handle,owner_grant_id,issuer_principal_id,holder_principal_id,owner_ref,target_ref,role,decision_family,action,design_context,assertion_id,expires_at,status,created_at) values(1,?1,(select id from owner_decision_grants where grant_handle=?2),(select id from authority_principals where principal_handle=?3),(select id from authority_principals where principal_handle=?3),'work-unit:1',?4,'review_adjudicator','review','adjudicate',?5,(select id from authority_assertions where purpose='capability_issue'),?6,'active',current_timestamp)",params![capability_handle,grant_handle,principal.as_str(),decision_target,design_context,expires]).unwrap();
    drop(conn);
    let request = OwnerDecisionRequest {
        command_kind: "review adjudicate",
        principal_handle: principal.as_str(),
        capability_handle: &capability_handle,
        owner_ref: "work-unit:1",
        target_ref: &decision_target,
        decision_family: "review",
        action: "adjudicate",
        decision_value: "accepted",
        reason: "independent owner decision",
        expected_current: "pending",
    };
    assert!(present_decision(temp.path(), request.clone()).is_ok());
    assert!(present_decision(temp.path(), request).is_ok());
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    let rejected_capability = format!("capability_{}", "88".repeat(32));
    conn.execute("insert into decision_capabilities(project_id,capability_handle,owner_grant_id,issuer_principal_id,holder_principal_id,owner_ref,target_ref,role,decision_family,action,design_context,assertion_id,expires_at,status,created_at) select project_id,?1,owner_grant_id,issuer_principal_id,holder_principal_id,owner_ref,target_ref,role,decision_family,action,design_context,assertion_id,expires_at,'active',current_timestamp from decision_capabilities where capability_handle=?2",params![rejected_capability,capability_handle]).unwrap();
    drop(conn);
    let rejected = OwnerDecisionRequest {
        command_kind: "review adjudicate",
        principal_handle: principal.as_str(),
        capability_handle: &rejected_capability,
        owner_ref: "work-unit:wrong",
        target_ref: &decision_target,
        decision_family: "review",
        action: "adjudicate",
        decision_value: "accepted",
        reason: "consume before tuple rejection",
        expected_current: "pending",
    };
    assert!(
        present_decision(temp.path(), rejected.clone())
            .unwrap_err()
            .to_string()
            .contains("capability_tuple_mismatch")
    );
    assert!(present_decision(temp.path(), rejected).is_err());
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    assert_eq!(
        conn.query_row("select count(*) from owner_decisions", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row(
            "select status from decision_capabilities where capability_handle=?1",
            params![capability_handle],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "consumed"
    );
    assert_eq!(
        conn.query_row(
            "select count(*) from capability_consumption_audits",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(
        conn.query_row(
            "select count(*) from capability_consumption_audits where outcome='rejected'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row(
            "select status from decision_capabilities where capability_handle=?1",
            params![rejected_capability],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "consumed"
    );
}
