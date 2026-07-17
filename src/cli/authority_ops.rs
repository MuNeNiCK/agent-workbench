use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;

use super::args::*;
use agent_workbench::*;

pub(crate) fn handle_authority(root: &Path, command: AuthorityCommand) -> Result<()> {
    match command {
        AuthorityCommand::Add(args) => {
            let event_type = match args.authority_type.as_str() {
                "design" => "design_doc",
                other => other,
            };
            let summary = args.summary.unwrap_or_else(|| {
                format!(
                    "registered {} authority at {}",
                    args.authority_type, args.path
                )
            });
            let outcome = add_authority_event(
                root,
                NewAuthorityEvent {
                    event_type,
                    source: Some(&args.path),
                    summary: &summary,
                    scope: args.scope.as_deref(),
                    precedence: args.precedence,
                },
            )?;
            println!("added authority");
            println!("authority_id: {}", outcome.authority_id);
            println!("authority_event_id: {}", outcome.authority_event_id);
        }
        AuthorityCommand::Event { command } => match command {
            AuthorityEventCommand::Add(args) => {
                let outcome = add_authority_event(
                    root,
                    NewAuthorityEvent {
                        event_type: &args.event_type,
                        source: args.source.as_deref(),
                        summary: &args.summary,
                        scope: args.scope.as_deref(),
                        precedence: args.precedence,
                    },
                )?;
                println!("added authority event");
                println!("authority_id: {}", outcome.authority_id);
                println!("authority_event_id: {}", outcome.authority_event_id);
            }
        },
        AuthorityCommand::List(args) => {
            let records = list_authorities(root, args.scope.as_deref())?;
            if records.is_empty() {
                println!("no authorities");
            }
            for record in records {
                let scope = record.scope.as_deref().unwrap_or("-");
                println!(
                    "{} [{} scope={} precedence={}] {}",
                    record.id,
                    record.authority_type,
                    scope,
                    record.precedence,
                    record.path_or_label
                );
            }
        }
        AuthorityCommand::Provider { command } => match command {
            AuthorityProviderCommand::Verify(args) => {
                if args.provider != "signed-envelope-v1" {
                    anyhow::bail!("unsupported authority provider");
                }
                let digest = verify_provider(root)?;
                println!("provider: signed-envelope-v1");
                println!("trust_digest: {digest}");
                println!("status: verified");
            }
        },
        AuthorityCommand::Assertion { command } => match *command {
            AuthorityAssertionCommand::Request { command } => {
                handle_assertion_request(root, *command)?;
            }
            AuthorityAssertionCommand::Import(args) => {
                if args.provider != "signed-envelope-v1" {
                    anyhow::bail!("unsupported authority provider");
                }
                let outcome = import_assertion(root, &args.purpose, &args.file)?;
                println!("assertion_handle: {}", outcome.assertion_handle);
                println!("assertion_digest: {}", outcome.assertion_digest);
                println!("purpose: {}", outcome.purpose);
            }
            AuthorityAssertionCommand::Assemble(args) => {
                if args.provider != "signed-envelope-v1" {
                    anyhow::bail!("unsupported authority provider");
                }
                let digest = assemble_assertion(root, &args.request, &args.signature, &args.out)?;
                println!(
                    "{{\"classification\":\"project-internal\",\"envelope_sha256\":\"{digest}\",\"status\":\"assembled\"}}"
                );
            }
        },
        AuthorityCommand::Grant { command } => match command {
            AuthorityGrantCommand::RootIssue(args) => {
                let outcome = issue_root_grant(
                    root,
                    RootGrantRequest {
                        principal_handle: &args.principal,
                        assertion_handle: &args.assertion,
                        owner_ref: &args.owner,
                        maximum_target: &args.maximum_target,
                        roles: &args.allowed_roles,
                        decision_families: &args.allowed_families,
                        actions: &args.allowed_actions,
                        maximum_depth: args.maximum_depth,
                        expires_at: &args.expires,
                    },
                )?;
                println!("grant_handle: {}", outcome.grant_handle);
            }
            AuthorityGrantCommand::Delegate(args) => {
                let outcome = delegate_grant(
                    root,
                    DelegateGrantRequest {
                        parent_grant: &args.parent_grant,
                        grantor_principal: &args.grantor,
                        grantee_principal: &args.grantee,
                        assertion_handle: &args.grantor_assertion,
                        target_scope: &args.target_scope,
                        roles: &args.roles,
                        decision_families: &args.decision_families,
                        actions: &args.actions,
                        delegation_depth: args.delegation_depth,
                        expires_at: &args.expires,
                    },
                )?;
                println!("grant_handle: {}", outcome.grant_handle);
            }
            AuthorityGrantCommand::Revoke(args) => {
                revoke_grant(root, &args.grant, &args.assertion)?;
                println!("revoked grant");
            }
        },
    }
    Ok(())
}

fn handle_assertion_request(root: &Path, command: AuthorityAssertionRequestCommand) -> Result<()> {
    let (common, purpose, payload) = match &command {
        AuthorityAssertionRequestCommand::RootGrant(args) => (
            &args.common,
            0,
            CborValue::Map(BTreeMap::from([
                (
                    0,
                    CborValue::Bytes(
                        digest_reference(b"agent-workbench:owner-ref-v1\0", &args.owner).to_vec(),
                    ),
                ),
                (1, target_value(&args.maximum_target)?),
                (2, role_set(&args.allowed_roles)?),
                (3, family_set(&args.allowed_families)?),
                (4, action_set(&args.allowed_actions)?),
                (5, CborValue::U64(args.maximum_depth)),
                (6, time_value(&args.expiry_ceiling, "expiry ceiling")?),
            ])),
        ),
        AuthorityAssertionRequestCommand::CapabilityIssue(args) => (
            &args.common,
            3,
            CborValue::Map(BTreeMap::from([
                (
                    0,
                    CborValue::Bytes(
                        digest_reference(b"agent-workbench:owner-ref-v1\0", &args.owner).to_vec(),
                    ),
                ),
                (1, target_value(&args.maximum_target)?),
                (2, role_set(&args.allowed_roles)?),
                (3, family_set(&args.allowed_families)?),
                (4, action_set(&args.allowed_actions)?),
                (5, time_value(&args.expiry_ceiling, "expiry ceiling")?),
                (
                    6,
                    CborValue::Bytes(
                        parse_hex::<32>(&args.design_context, "design context")?.to_vec(),
                    ),
                ),
            ])),
        ),
        AuthorityAssertionRequestCommand::GrantDelegate(args) => {
            let child_kind = subject_kind_code(&args.child_kind)?;
            (
                &args.common,
                1,
                CborValue::Map(BTreeMap::from([
                    (
                        0,
                        CborValue::Bytes(
                            digest_reference(b"agent-workbench:owner-ref-v1\0", &args.owner)
                                .to_vec(),
                        ),
                    ),
                    (
                        1,
                        CborValue::Bytes(
                            digest_reference(b"agent-workbench:grant-ref-v1\0", &args.parent_grant)
                                .to_vec(),
                        ),
                    ),
                    (
                        2,
                        subject_value(
                            child_kind,
                            parse_hex::<32>(&args.child_digest, "child digest")?,
                        ),
                    ),
                    (3, target_value(&args.target)?),
                    (4, role_set(&args.roles)?),
                    (5, family_set(&args.families)?),
                    (6, action_set(&args.actions)?),
                    (7, CborValue::U64(args.depth)),
                    (8, time_value(&args.grant_expires, "grant expiry")?),
                ])),
            )
        }
        AuthorityAssertionRequestCommand::GrantRevoke(args) => {
            if args.expected_current != "active" {
                anyhow::bail!("expected-current must be active");
            }
            (
                &args.common,
                2,
                CborValue::Map(BTreeMap::from([
                    (
                        0,
                        CborValue::Bytes(
                            digest_reference(b"agent-workbench:owner-ref-v1\0", &args.owner)
                                .to_vec(),
                        ),
                    ),
                    (
                        1,
                        CborValue::Bytes(
                            digest_reference(b"agent-workbench:grant-ref-v1\0", &args.target_grant)
                                .to_vec(),
                        ),
                    ),
                    (
                        2,
                        CborValue::Bytes(
                            parse_hex::<32>(&args.reason_digest, "reason digest")?.to_vec(),
                        ),
                    ),
                    (3, CborValue::U64(0)),
                ])),
            )
        }
        AuthorityAssertionRequestCommand::ReviewProvenance(args) => (
            &args.common,
            4,
            CborValue::Map(BTreeMap::from([
                (
                    0,
                    CborValue::Bytes(
                        digest_reference(b"agent-workbench:review-plan-ref-v1\0", &args.plan)
                            .to_vec(),
                    ),
                ),
                (
                    1,
                    CborValue::Bytes(
                        digest_reference(b"agent-workbench:review-context-ref-v1\0", &args.target)
                            .to_vec(),
                    ),
                ),
                (
                    2,
                    CborValue::U64(match args.provenance_kind.as_str() {
                        "human_review" => 0,
                        "external_agent" => 1,
                        "service_review" => 2,
                        _ => anyhow::bail!("unsupported provenance kind"),
                    }),
                ),
                (
                    3,
                    CborValue::U64(match args.review_purpose.as_str() {
                        "new_unbiased_review" => 0,
                        "finding_fix_verification" => 1,
                        _ => anyhow::bail!("unsupported review purpose"),
                    }),
                ),
                (
                    4,
                    CborValue::Bytes(
                        parse_hex::<32>(&args.reference_digest, "reference digest")?.to_vec(),
                    ),
                ),
            ])),
        ),
        AuthorityAssertionRequestCommand::LegacyReviewerBinding(args) => (
            &args.common,
            5,
            CborValue::Map(BTreeMap::from([
                (
                    0,
                    CborValue::Bytes(
                        parse_hex::<32>(&args.source_ledger_digest, "source ledger digest")?
                            .to_vec(),
                    ),
                ),
                (1, CborValue::U64(args.source_generation)),
                (
                    2,
                    CborValue::Bytes(
                        parse_hex::<32>(&args.source_reviewer_digest, "source reviewer digest")?
                            .to_vec(),
                    ),
                ),
            ])),
        ),
    };
    if common.provider != "signed-envelope-v1" {
        anyhow::bail!("unsupported authority provider");
    }
    let request = UnsignedEnvelopeRequest {
        key_id: parse_hex(&common.key_id, "key id")?,
        assertion_id: parse_hex(&common.assertion_id, "assertion id")?,
        nonce: parse_hex(&common.nonce, "nonce")?,
        purpose,
        issued: parse_rfc3339_seconds(&common.issued, "issued")?,
        expires: parse_rfc3339_seconds(&common.expires, "expires")?,
        subject_kind: subject_kind_code(&common.subject_kind)?,
        subject_digest: parse_hex(&common.subject_digest, "subject digest")?,
        project_digest: [0; 32],
        payload,
        trust_identity: [0; 32],
    };
    let outcome = create_assertion_request(root, request, &common.out)?;
    println!(
        "{{\"classification\":\"project-internal\",\"preimage_sha256\":\"{}\",\"request_sha256\":\"{}\",\"status\":\"created\"}}",
        outcome.preimage_digest, outcome.request_digest
    );
    Ok(())
}

fn subject_kind_code(value: &str) -> Result<u64> {
    Ok(match value {
        "human" => 0,
        "agent" => 1,
        "service" => 2,
        _ => anyhow::bail!("unsupported subject kind"),
    })
}
fn time_value(value: &str, label: &str) -> Result<CborValue> {
    let value = parse_rfc3339_seconds(value, label)?;
    Ok(if value >= 0 {
        CborValue::U64(value as u64)
    } else {
        CborValue::I64(value)
    })
}
fn role_set(value: &str) -> Result<CborValue> {
    closed_set(
        value,
        &[
            ("grant_admin", 0),
            ("review_adjudicator", 1),
            ("finding_adjudicator", 2),
            ("verification_adjudicator", 3),
            ("human_authority", 4),
        ],
        "role",
    )
}
fn family_set(value: &str) -> Result<CborValue> {
    closed_set(
        value,
        &[("review", 0), ("finding", 1), ("verification", 2)],
        "family",
    )
}
fn action_set(value: &str) -> Result<CborValue> {
    closed_set(
        value,
        &[
            ("adjudicate", 0),
            ("dispose", 1),
            ("bootstrap_adjudicate", 2),
            ("correct_terminal", 3),
            ("reopen", 4),
        ],
        "action",
    )
}

pub(crate) fn handle_principal(root: &Path, command: PrincipalCommand) -> Result<()> {
    match command {
        PrincipalCommand::Resolve(args) => {
            if args.provider != "signed-envelope-v1" {
                anyhow::bail!("unsupported authority provider");
            }
            let outcome = resolve_principal(root, &args.assertion)?;
            println!("principal_handle: {}", outcome.principal_handle);
            println!("subject_kind: {}", outcome.subject_kind);
        }
    }
    Ok(())
}

pub(crate) fn handle_owner(root: &Path, command: OwnerCommand) -> Result<()> {
    match command {
        OwnerCommand::Grant { command } => match command {
            OwnerGrantCommand::RootIssue(args) => {
                if args.provider != "signed-envelope-v1" {
                    anyhow::bail!("unsupported authority provider");
                }
                let outcome = issue_root_grant(
                    root,
                    RootGrantRequest {
                        principal_handle: &args.grantee,
                        assertion_handle: &args.human_assertion,
                        owner_ref: &args.owner,
                        maximum_target: &args.target_scope,
                        roles: &args.roles,
                        decision_families: &args.decision_families,
                        actions: &args.actions,
                        maximum_depth: args.delegation_depth,
                        expires_at: &args.expires,
                    },
                )?;
                println!("grant_handle: {}", outcome.grant_handle);
            }
            OwnerGrantCommand::Delegate(args) => {
                let outcome = delegate_grant(
                    root,
                    DelegateGrantRequest {
                        parent_grant: &args.parent_grant,
                        grantor_principal: &args.grantor,
                        grantee_principal: &args.grantee,
                        assertion_handle: &args.grantor_assertion,
                        target_scope: &args.target_scope,
                        roles: &args.roles,
                        decision_families: &args.decision_families,
                        actions: &args.actions,
                        delegation_depth: args.delegation_depth,
                        expires_at: &args.expires,
                    },
                )?;
                println!("grant_handle: {}", outcome.grant_handle);
            }
            OwnerGrantCommand::Revoke(args) => {
                revoke_grant_as(
                    root,
                    &args.grant,
                    &args.grantor,
                    &args.grantor_assertion,
                    &args.reason,
                    &args.expected_current,
                )?;
                println!("revoked grant");
            }
        },
    }
    Ok(())
}
