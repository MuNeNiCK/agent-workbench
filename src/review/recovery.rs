use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::{OptionalExtension, TransactionBehavior, params};

use crate::db::{open_existing_project, project_id};
use crate::design::{DesignPackageImport, import_design_package_in};
use crate::identity::{
    CanonicalValue, DecisionHandle, RecoveryHandle, RevisionHandle, domain_digest,
};

use super::{CorrectionToken, correction_contract::*, finding_fix_context_ref};

#[derive(Clone, Debug)]
pub struct FindingDesignRecovery<'a> {
    pub finding_id: i64,
    pub terminal_epoch: i64,
    pub evidence: &'a str,
    pub authority_event_id: i64,
    pub reason: &'a str,
    pub package_current: &'a str,
    pub expected_current: &'a str,
    pub idempotency_key: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindingDesignRecoveryOutcome {
    pub recovery_handle: String,
    pub finding_id: i64,
    pub terminal_epoch: i64,
    pub source_closure_id: i64,
    pub source_session_id: i64,
    pub source_attempt_id: i64,
    pub corrected_design_version_id: i64,
    pub corrected_design_ref: String,
    pub successor_closure_id: i64,
    pub successor_session_id: i64,
    pub successor_attempt_id: i64,
    pub context_ref: String,
    pub next_action: String,
    pub idempotent: bool,
    pub converged: bool,
}

struct TerminalCorrection {
    source_closure_id: i64,
    source_session_id: i64,
    source_attempt_id: i64,
    source_design_version_id: i64,
    work_unit_id: i64,
    package_root: String,
    package_identity: String,
    review_type: String,
    review_stage: String,
    review_scope: Option<String>,
    review_policy_id: i64,
}

struct StoredRecovery {
    payload_digest: String,
    postcondition_digest: String,
    recovery_handle: String,
    finding_id: i64,
    terminal_epoch: i64,
    source_closure_id: i64,
    source_session_id: i64,
    source_attempt_id: i64,
    authority_event_id: i64,
    evidence: String,
    reason: String,
    package_current: String,
    expected_current: String,
    successor_design_version_id: i64,
    successor_alias: String,
    successor_closure_id: i64,
    successor_session_id: i64,
    successor_attempt_id: i64,
}

pub fn recover_finding_design(
    root: &Path,
    request: FindingDesignRecovery<'_>,
) -> Result<FindingDesignRecoveryOutcome> {
    validate_request(&request)?;
    let mut conn = open_existing_project(root)?;
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let project = project_id(&tx)?;

    if let Some(outcome) = lookup_recovery(&tx, root, project, &request)? {
        tx.commit()?;
        return Ok(outcome);
    }
    if let Some(outcome) = lookup_competing_recovery(&tx, root, project, &request)? {
        tx.commit()?;
        return Ok(outcome);
    }

    let source = terminal_correction(
        &tx,
        project,
        request.finding_id,
        request.terminal_epoch,
        request.expected_current,
    )?;
    if source.package_identity != request.package_current {
        bail!(
            "package_current_stale: expected {}; current {}; next: agent-workbench design inspect {}",
            request.package_current,
            source.package_identity,
            source.source_design_version_id
        );
    }
    validate_authority(
        &tx,
        project,
        request.authority_event_id,
        request.finding_id,
        source.work_unit_id,
    )?;
    let postcondition_digest =
        validate_file_postconditions(&tx, root, request.finding_id, source.source_closure_id)?;
    let payload = recovery_payload(&request, &source, &postcondition_digest);
    let payload_digest = domain_digest(
        b"agent-workbench:finding-design-recovery-payload-v1\0",
        &payload,
    );
    let recovery_handle =
        RecoveryHandle::derive(b"agent-workbench:finding-design-recovery-v1\0", &payload);

    let package_path = root.join(&source.package_root);
    let imported = import_design_package_in(
        &tx,
        root,
        DesignPackageImport {
            package_path: &package_path,
            status: "draft",
        },
    )
    .with_context(|| {
        format!(
            "corrected Design Package is invalid; next: agent-workbench design inspect {}",
            source.source_design_version_id
        )
    })?;
    let successor_alias = RevisionHandle::derive(
        b"agent-workbench:recovered-design-version-alias-v1\0",
        &CanonicalValue::object([
            ("recovery", CanonicalValue::string(recovery_handle.as_str())),
            (
                "design_version",
                CanonicalValue::Integer(imported.design_version_id),
            ),
            ("identity", CanonicalValue::string(&imported.content_hash)),
        ]),
    );

    tx.execute(
        "update findings set lifecycle_state='open',status='open',classification='valid',close_reason=null where project_id=?1 and id=?2 and lifecycle_state='closed' and status='closed'",
        params![project, request.finding_id],
    )?;
    if tx.changes() != 1 {
        bail!("terminal_finding_changed; next: agent-workbench finding list --status closed");
    }
    let successor_epoch_decision_id =
        insert_recovery_owner_decision(&tx, project, &request, source.work_unit_id)?;
    tx.execute(
        "insert into finding_decision_epochs(project_id,finding_id,epoch_number,reopen_decision_id,status,created_at) values(?1,?2,?3,?4,'open',current_timestamp)",
        params![project, request.finding_id, request.terminal_epoch + 1, successor_epoch_decision_id],
    )?;
    tx.execute(
        "insert into finding_lifecycle_events(project_id,finding_id,owner_decision_id,from_state,to_state,effect,created_at) values(?1,?2,?3,'closed','open','terminal_design_recovery',current_timestamp)",
        params![project, request.finding_id, successor_epoch_decision_id],
    )?;

    tx.execute(
        r#"
        insert into closures(
            project_id,finding_id,design_invariant,design_citations,
            implementation_evidence,affected_surfaces,same_invariant_search,
            other_violations_found,fix_plan,tests_or_gates,verification_plan,
            closed_by_commit,status,created_at
        )
        select project_id,finding_id,design_invariant,design_citations,
               ?1,affected_surfaces,same_invariant_search,other_violations_found,
               fix_plan,tests_or_gates,verification_plan,null,'registered',current_timestamp
        from closures where project_id=?2 and id=?3
        "#,
        params![request.evidence, project, source.source_closure_id],
    )?;
    let successor_closure_id = tx.last_insert_rowid();

    copy_applied_tokens(&tx, project, source.source_closure_id, successor_closure_id)?;
    tx.execute(
        "insert into correction_sessions(project_id,finding_id,closure_id,status,created_at) values(?1,?2,?3,'active',current_timestamp)",
        params![project, request.finding_id, successor_closure_id],
    )?;
    let successor_session_id = tx.last_insert_rowid();

    let high_watermark: i64 =
        tx.query_row("select coalesce(max(id),0) from review_runs", [], |row| {
            row.get(0)
        })?;
    let tests: String = tx.query_row(
        "select tests_or_gates from closures where id=?1",
        params![source.source_closure_id],
        |row| row.get(0),
    )?;
    tx.execute(
        r#"
        insert into closure_attempts(
            project_id,closure_id,attempt_number,implementation_evidence,
            tests_or_gates,review_run_high_watermark,created_at
        ) values(?1,?2,1,?3,?4,?5,current_timestamp)
        "#,
        params![
            project,
            successor_closure_id,
            request.evidence,
            tests,
            high_watermark
        ],
    )?;
    let successor_attempt_id = tx.last_insert_rowid();
    tx.execute(
        "update closures set status='ready_for_verification' where project_id=?1 and id=?2 and status='registered'",
        params![project, successor_closure_id],
    )?;
    tx.execute(
        "update correction_sessions set status='completed',completed_at=current_timestamp where project_id=?1 and id=?2 and status='active'",
        params![project, successor_session_id],
    )?;
    tx.execute(
        "update findings set lifecycle_state='awaiting_verification' where project_id=?1 and id=?2 and lifecycle_state='open'",
        params![project, request.finding_id],
    )?;
    tx.execute(
        "insert into finding_lifecycle_events(project_id,finding_id,owner_decision_id,from_state,to_state,effect,created_at) values(?1,?2,?3,'open','awaiting_verification','design_recovery_ready',current_timestamp)",
        params![project, request.finding_id, successor_epoch_decision_id],
    )?;

    tx.execute(
        r#"
        insert into finding_design_recoveries(
            project_id,recovery_handle,finding_id,terminal_epoch,
            source_closure_id,source_session_id,source_attempt_id,
            authority_event_id,evidence,reason,package_current,expected_current,
            idempotency_key,payload_digest,postcondition_digest,successor_design_version_id,
            successor_alias,successor_closure_id,successor_session_id,
            successor_attempt_id,successor_epoch_decision_id,created_at
        ) values(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,current_timestamp)
        "#,
        params![
            project,
            recovery_handle.as_str(),
            request.finding_id,
            request.terminal_epoch,
            source.source_closure_id,
            source.source_session_id,
            source.source_attempt_id,
            request.authority_event_id,
            request.evidence,
            request.reason,
            request.package_current,
            request.expected_current,
            request.idempotency_key,
            payload_digest,
            postcondition_digest,
            imported.design_version_id,
            successor_alias.as_str(),
            successor_closure_id,
            successor_session_id,
            successor_attempt_id,
            successor_epoch_decision_id,
        ],
    )?;
    let next_action = successor_review_plan_action(&source, imported.design_version_id);
    tx.commit()?;
    Ok(FindingDesignRecoveryOutcome {
        recovery_handle: recovery_handle.as_str().to_string(),
        finding_id: request.finding_id,
        terminal_epoch: request.terminal_epoch,
        source_closure_id: source.source_closure_id,
        source_session_id: source.source_session_id,
        source_attempt_id: source.source_attempt_id,
        corrected_design_version_id: imported.design_version_id,
        corrected_design_ref: successor_alias.as_str().to_string(),
        successor_closure_id,
        successor_session_id,
        successor_attempt_id,
        context_ref: finding_fix_context_ref(
            request.finding_id,
            successor_closure_id,
            successor_attempt_id,
        ),
        next_action,
        idempotent: false,
        converged: false,
    })
}

fn recovery_payload(
    request: &FindingDesignRecovery<'_>,
    source: &TerminalCorrection,
    postcondition_digest: &str,
) -> CanonicalValue {
    CanonicalValue::object([
        ("finding", CanonicalValue::Integer(request.finding_id)),
        ("epoch", CanonicalValue::Integer(request.terminal_epoch)),
        (
            "source_closure",
            CanonicalValue::Integer(source.source_closure_id),
        ),
        (
            "source_session",
            CanonicalValue::Integer(source.source_session_id),
        ),
        (
            "source_attempt",
            CanonicalValue::Integer(source.source_attempt_id),
        ),
        (
            "postconditions",
            CanonicalValue::string(postcondition_digest),
        ),
        ("evidence", CanonicalValue::string(request.evidence)),
        (
            "authority",
            CanonicalValue::Integer(request.authority_event_id),
        ),
        ("reason", CanonicalValue::string(request.reason)),
        (
            "package_current",
            CanonicalValue::string(request.package_current),
        ),
        (
            "expected_current",
            CanonicalValue::string(request.expected_current),
        ),
        ("key", CanonicalValue::string(request.idempotency_key)),
    ])
}

fn insert_recovery_owner_decision(
    conn: &rusqlite::Connection,
    project: i64,
    request: &FindingDesignRecovery<'_>,
    work_unit_id: i64,
) -> Result<i64> {
    let owner_ref = format!("work_unit:{work_unit_id}");
    let target_ref = format!(
        "finding_epoch:{}:{}",
        request.finding_id, request.terminal_epoch
    );
    let payload = CanonicalValue::object([
        ("command", CanonicalValue::string("finding recover")),
        ("owner", CanonicalValue::string(&owner_ref)),
        ("target", CanonicalValue::string(&target_ref)),
        ("family", CanonicalValue::string("finding")),
        ("action", CanonicalValue::string("reopen")),
        ("decision", CanonicalValue::string("reopened")),
        ("reason", CanonicalValue::string(request.reason)),
        (
            "expected_current",
            CanonicalValue::string(request.expected_current),
        ),
    ]);
    let decision = DecisionHandle::derive(b"agent-workbench:owner-decision-v1\0", &payload);
    let payload_digest = domain_digest(b"agent-workbench:owner-decision-payload-v1\0", &payload);
    conn.execute(
        r#"
        insert into owner_decisions(
          project_id,decision_handle,owner_ref,target_ref,
          decision_family,action,decision_value,reason,expected_current,payload_digest,created_at
        ) values(?1,?2,?3,?4,'finding','reopen','reopened',?5,?6,?7,current_timestamp)
        "#,
        params![
            project,
            decision.as_str(),
            owner_ref,
            target_ref,
            request.reason,
            request.expected_current,
            payload_digest
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn successor_review_plan_action(source: &TerminalCorrection, design_version_id: i64) -> String {
    let mut action = format!(
        "agent-workbench review plan add --work-unit {} --type {} --stage {} --design-version {} --policy {}",
        source.work_unit_id,
        source.review_type,
        source.review_stage,
        design_version_id,
        source.review_policy_id
    );
    if let Some(scope) = source.review_scope.as_deref() {
        action.push_str(" --scope \"");
        action.push_str(&scope.replace('\\', "\\\\").replace('"', "\\\""));
        action.push('"');
    }
    action
}

fn current_recovery_next_action(
    conn: &rusqlite::Connection,
    project: i64,
    source: &TerminalCorrection,
    stored: &StoredRecovery,
) -> Result<String> {
    let (finding_status, lifecycle_state, closure_status, attempt_result) = conn.query_row(
        r#"
        select finding.status,finding.lifecycle_state,closure.status,attempt.result
        from findings finding
        join closures closure on closure.project_id=finding.project_id
          and closure.finding_id=finding.id and closure.id=?3
        join closure_attempts attempt on attempt.project_id=finding.project_id
          and attempt.closure_id=closure.id and attempt.id=?4
        where finding.project_id=?1 and finding.id=?2
        "#,
        params![
            project,
            stored.finding_id,
            stored.successor_closure_id,
            stored.successor_attempt_id
        ],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        },
    )?;
    if finding_status == "closed"
        && lifecycle_state == "closed"
        && closure_status == "verified"
        && attempt_result.as_deref() == Some("verified")
    {
        return Ok(format!(
            "agent-workbench design inspect {}",
            stored.successor_alias
        ));
    }
    let adjudicated: bool = conn.query_row(
        "select exists(select 1 from verification_adjudication_decisions where project_id=?1 and closure_attempt_id=?2)",
        params![project, stored.successor_attempt_id],
        |row| row.get(0),
    )?;
    if adjudicated {
        return Ok("agent-workbench next".to_string());
    }
    if conn
        .query_row(
            r#"
            select 1
            from finding_verifications
            where project_id=?1 and finding_id=?2 and closure_id=?3
              and closure_attempt_id=?4
            order by id desc limit 1
            "#,
            params![
                project,
                stored.finding_id,
                stored.successor_closure_id,
                stored.successor_attempt_id
            ],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some()
    {
        return Ok("agent-workbench verification adjudicate --help".to_string());
    }
    if closure_status != "ready_for_verification"
        || attempt_result.is_some()
        || lifecycle_state != "awaiting_verification"
    {
        return Ok(format!(
            "agent-workbench finding list --status {}",
            finding_status
        ));
    }
    let exact_plan_exists: bool = conn.query_row(
        "select exists(select 1 from review_plans where project_id=?1 and work_unit_id=?2 and design_version_id=?3 and review_type=?4 and stage=?5 and coalesce(scope,'')=coalesce(?6,'') and required=1 and status in ('open','clean'))",
        params![
            project,
            source.work_unit_id,
            stored.successor_design_version_id,
            source.review_type,
            source.review_stage,
            source.review_scope.as_deref()
        ],
        |row| row.get(0),
    )?;
    if exact_plan_exists {
        return Ok(format!(
            "agent-workbench review-context finding-fix --finding {} --closure {} --attempt {}",
            stored.finding_id, stored.successor_closure_id, stored.successor_attempt_id
        ));
    }
    Ok(successor_review_plan_action(
        source,
        stored.successor_design_version_id,
    ))
}

fn validate_request(request: &FindingDesignRecovery<'_>) -> Result<()> {
    if request.finding_id <= 0 || request.terminal_epoch <= 0 || request.authority_event_id <= 0 {
        bail!(
            "finding recovery ids must be positive; next: agent-workbench finding recover --help"
        );
    }
    if request.evidence.trim().is_empty()
        || request.reason.trim().is_empty()
        || request.package_current.trim().is_empty()
        || request.expected_current.trim().is_empty()
    {
        bail!(
            "finding recovery evidence, reason, and current handles must not be empty; next: agent-workbench finding recover --help"
        );
    }
    if request.idempotency_key.is_empty()
        || request.idempotency_key.len() > 200
        || !request.idempotency_key.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':' | b'/')
        })
    {
        bail!(
            "idempotency key must be a non-empty portable token; next: agent-workbench finding recover --help"
        );
    }
    Ok(())
}

fn terminal_correction(
    conn: &rusqlite::Connection,
    project: i64,
    finding_id: i64,
    terminal_epoch: i64,
    expected_current: &str,
) -> Result<TerminalCorrection> {
    let mut stmt = conn.prepare(
        r#"
        select closure.id,session.id,attempt.id,plan.design_version_id,
               plan.work_unit_id,package.root_path,version.content_hash,
               plan.review_type,plan.stage,plan.scope,plan.review_policy_id
        from findings finding
        join review_runs run on run.id=finding.review_run_id
        join review_plans plan on plan.id=run.review_plan_id
        join design_versions version on version.id=plan.design_version_id
        join design_packages package on package.id=version.design_package_id
          and package.current_design_version_id=version.id
        join finding_decision_epochs epoch on epoch.finding_id=finding.id
          and epoch.project_id=finding.project_id and epoch.epoch_number=?3
          and epoch.status='terminal'
        join owner_decisions terminal on terminal.id=epoch.terminal_decision_id
          and terminal.decision_handle=?4
        join closures closure on closure.finding_id=finding.id
          and closure.project_id=finding.project_id and closure.status='verified'
        join correction_sessions session on session.closure_id=closure.id
          and session.project_id=finding.project_id and session.status='completed'
        join closure_attempts attempt on attempt.closure_id=closure.id
          and attempt.project_id=finding.project_id and attempt.result='verified'
        join verification_adjudication_decisions decision
          on decision.closure_attempt_id=attempt.id
          and decision.project_id=finding.project_id and decision.value='accepted'
          and decision.owner_decision_id=epoch.terminal_decision_id
        where finding.project_id=?1 and finding.id=?2
          and finding.status='closed' and finding.lifecycle_state='closed'
          and finding.close_reason='verified'
          and finding.finding_type in ('design_finding','design_task_gap','design_implementation_drift')
        "#,
    )?;
    let rows = stmt
        .query_map(
            params![project, finding_id, terminal_epoch, expected_current],
            |row| {
                Ok(TerminalCorrection {
                    source_closure_id: row.get(0)?,
                    source_session_id: row.get(1)?,
                    source_attempt_id: row.get(2)?,
                    source_design_version_id: row.get(3)?,
                    work_unit_id: row.get(4)?,
                    package_root: row.get(5)?,
                    package_identity: row.get(6)?,
                    review_type: row.get(7)?,
                    review_stage: row.get(8)?,
                    review_scope: row.get(9)?,
                    review_policy_id: row.get(10)?,
                })
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    match rows.len() {
        1 => Ok(rows.into_iter().next().unwrap()),
        0 => bail!(
            "terminal_design_recovery_not_current; next: agent-workbench finding list --status closed"
        ),
        count => bail!(
            "terminal_design_recovery_ambiguous: {count} source tuples; no recovery was published; next: agent-workbench finding list --status closed"
        ),
    }
}

fn validate_authority(
    conn: &rusqlite::Connection,
    project: i64,
    authority: i64,
    finding: i64,
    work: i64,
) -> Result<()> {
    let valid: bool = conn.query_row(
        r#"
        select exists(
          select 1 from authority_events
          where project_id=?1 and id=?2 and status='active'
            and event_type in ('user_instruction','policy','design_doc')
            and coalesce(scope,'project') in (
              'project','finding:'||?3,'work-unit:'||?4,'work_unit:'||?4
            )
        )
        "#,
        params![project, authority, finding, work],
        |row| row.get(0),
    )?;
    if !valid {
        bail!(
            "authority_invalid; next: agent-workbench authority event add --type user_instruction --summary \"recover terminal design finding {finding}\" --scope \"finding:{finding}\""
        );
    }
    Ok(())
}

fn validate_file_postconditions(
    conn: &rusqlite::Connection,
    root: &Path,
    finding_id: i64,
    closure_id: i64,
) -> Result<String> {
    let design_root = correction_design_root(conn, finding_id)?;
    let mut stmt = conn.prepare(
        "select operation,target from correction_tokens where closure_id=?1 and token_kind='file' order by token_ordinal",
    )?;
    let tokens = stmt
        .query_map(params![closure_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if tokens.is_empty() {
        bail!(
            "terminal design correction has no declared file postconditions; next: agent-workbench finding list --status closed"
        );
    }
    let mut postconditions = Vec::new();
    for (operation, target) in tokens {
        let token = CorrectionToken {
            kind: "file".to_string(),
            operation: operation.clone(),
            target: target.clone(),
        };
        let path = correction_file_path(root, design_root.as_deref(), &token)?;
        let pre_hash = effective_file_pre_hash(conn, closure_id, &operation, &target)?;
        let current_identity = match operation.as_str() {
            "create" if path.is_file() => file_sha256(&path)?,
            "delete" if !path.exists() => "absent".to_string(),
            "edit" if path.is_file() => file_sha256(&path)?,
            _ => String::new(),
        };
        let valid = match operation.as_str() {
            "create" => !current_identity.is_empty(),
            "delete" => current_identity == "absent",
            "edit" => !current_identity.is_empty() && Some(current_identity.clone()) != pre_hash,
            _ => false,
        };
        if !valid {
            bail!(
                "correction_postcondition_failed: {}; next: restore the declared corrected file state, then retry the exact finding recover command",
                path.display()
            );
        }
        postconditions.push(CanonicalValue::object([
            ("operation", CanonicalValue::string(operation)),
            ("target", CanonicalValue::string(target)),
            (
                "pre_hash",
                pre_hash.map_or(CanonicalValue::Null, CanonicalValue::string),
            ),
            ("current", CanonicalValue::string(current_identity)),
        ]));
    }
    Ok(domain_digest(
        b"agent-workbench:finding-design-recovery-postconditions-v1\0",
        &CanonicalValue::Array(postconditions),
    ))
}

fn copy_applied_tokens(
    conn: &rusqlite::Connection,
    project: i64,
    source_closure: i64,
    successor_closure: i64,
) -> Result<()> {
    let mut stmt = conn.prepare(
        "select token_ordinal,token_kind,operation,target,pre_state,pre_hash from correction_tokens where closure_id=?1 order by token_ordinal",
    )?;
    let tokens = stmt
        .query_map(params![source_closure], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (ordinal, kind, operation, target, pre_state, pre_hash) in tokens {
        let recovery_pre_state = if kind == "transition" {
            transition_pre_state(conn, &operation)?
        } else {
            pre_state
        };
        conn.execute(
            r#"
            insert into correction_tokens(
              project_id,closure_id,token_ordinal,token_kind,operation,target,
              pre_state,pre_hash,status,created_at,applied_at
            ) values(?1,?2,?3,?4,?5,?6,?7,?8,'applied',current_timestamp,current_timestamp)
            "#,
            params![
                project,
                successor_closure,
                ordinal,
                kind,
                operation,
                target,
                recovery_pre_state,
                pre_hash
            ],
        )?;
    }
    Ok(())
}

fn terminal_correction_for_receipt(
    conn: &rusqlite::Connection,
    project: i64,
    stored: &StoredRecovery,
) -> Result<TerminalCorrection> {
    conn.query_row(
        r#"
        select closure.id,session.id,attempt.id,plan.design_version_id,
               plan.work_unit_id,package.root_path,version.content_hash,
               plan.review_type,plan.stage,plan.scope,plan.review_policy_id
        from findings finding
        join review_runs run on run.id=finding.review_run_id
        join review_plans plan on plan.id=run.review_plan_id
        join design_versions version on version.id=plan.design_version_id
        join design_packages package on package.id=version.design_package_id
        join closures closure on closure.id=?3 and closure.finding_id=finding.id
          and closure.project_id=finding.project_id and closure.status='verified'
        join correction_sessions session on session.id=?4 and session.closure_id=closure.id
          and session.project_id=finding.project_id and session.status='completed'
        join closure_attempts attempt on attempt.id=?5 and attempt.closure_id=closure.id
          and attempt.project_id=finding.project_id and attempt.result='verified'
        where finding.project_id=?1 and finding.id=?2
        "#,
        params![
            project,
            stored.finding_id,
            stored.source_closure_id,
            stored.source_session_id,
            stored.source_attempt_id
        ],
        |row| {
            Ok(TerminalCorrection {
                source_closure_id: row.get(0)?,
                source_session_id: row.get(1)?,
                source_attempt_id: row.get(2)?,
                source_design_version_id: row.get(3)?,
                work_unit_id: row.get(4)?,
                package_root: row.get(5)?,
                package_identity: row.get(6)?,
                review_type: row.get(7)?,
                review_stage: row.get(8)?,
                review_scope: row.get(9)?,
                review_policy_id: row.get(10)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| {
        anyhow::anyhow!(
            "recorded recovery source is no longer intact; next: agent-workbench finding list --status closed"
        )
    })
}

fn lookup_recovery(
    conn: &rusqlite::Connection,
    root: &Path,
    project: i64,
    request: &FindingDesignRecovery<'_>,
) -> Result<Option<FindingDesignRecoveryOutcome>> {
    let stored = conn
        .query_row(
            &format!(
                "{} where project_id=?1 and idempotency_key=?2",
                stored_recovery_select()
            ),
            params![project, request.idempotency_key],
            read_stored_recovery,
        )
        .optional()?;
    let Some(stored) = stored else {
        return Ok(None);
    };
    if stored.finding_id != request.finding_id
        || stored.terminal_epoch != request.terminal_epoch
        || stored.authority_event_id != request.authority_event_id
        || stored.evidence != request.evidence
        || stored.reason != request.reason
        || stored.package_current != request.package_current
        || stored.expected_current != request.expected_current
    {
        bail!(
            "idempotency_key_payload_mismatch; next: use a new --idempotency-key for a changed request, or retry the original finding recover request unchanged"
        );
    }
    let (source, postcondition_digest) = validate_stored_recovery(conn, root, project, &stored)?;
    let payload = recovery_payload(request, &source, &postcondition_digest);
    let payload_digest = domain_digest(
        b"agent-workbench:finding-design-recovery-payload-v1\0",
        &payload,
    );
    let recovery_handle =
        RecoveryHandle::derive(b"agent-workbench:finding-design-recovery-v1\0", &payload);
    if stored.payload_digest != payload_digest || stored.recovery_handle != recovery_handle.as_str()
    {
        bail!(
            "idempotency_key_payload_mismatch; next: restore the exact declared corrected file state and retry the original finding recover request, or use a new --idempotency-key for a changed request"
        );
    }
    Ok(Some(stored_recovery_outcome(
        conn, project, &source, stored, true, false,
    )?))
}

fn lookup_competing_recovery(
    conn: &rusqlite::Connection,
    root: &Path,
    project: i64,
    request: &FindingDesignRecovery<'_>,
) -> Result<Option<FindingDesignRecoveryOutcome>> {
    let stored = conn
        .query_row(
            &format!(
                "{} where project_id=?1 and finding_id=?2 and terminal_epoch=?3",
                stored_recovery_select()
            ),
            params![project, request.finding_id, request.terminal_epoch],
            read_stored_recovery,
        )
        .optional()?;
    let Some(stored) = stored else {
        return Ok(None);
    };
    let (source, _) = validate_stored_recovery(conn, root, project, &stored)?;
    Ok(Some(stored_recovery_outcome(
        conn, project, &source, stored, false, true,
    )?))
}

fn stored_recovery_select() -> &'static str {
    r#"select payload_digest,postcondition_digest,recovery_handle,finding_id,terminal_epoch,
              source_closure_id,source_session_id,source_attempt_id,
              authority_event_id,evidence,reason,package_current,expected_current,
              successor_design_version_id,successor_alias,
              successor_closure_id,successor_session_id,successor_attempt_id
       from finding_design_recoveries"#
}

fn read_stored_recovery(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredRecovery> {
    Ok(StoredRecovery {
        payload_digest: row.get(0)?,
        postcondition_digest: row.get(1)?,
        recovery_handle: row.get(2)?,
        finding_id: row.get(3)?,
        terminal_epoch: row.get(4)?,
        source_closure_id: row.get(5)?,
        source_session_id: row.get(6)?,
        source_attempt_id: row.get(7)?,
        authority_event_id: row.get(8)?,
        evidence: row.get(9)?,
        reason: row.get(10)?,
        package_current: row.get(11)?,
        expected_current: row.get(12)?,
        successor_design_version_id: row.get(13)?,
        successor_alias: row.get(14)?,
        successor_closure_id: row.get(15)?,
        successor_session_id: row.get(16)?,
        successor_attempt_id: row.get(17)?,
    })
}

fn validate_stored_recovery(
    conn: &rusqlite::Connection,
    root: &Path,
    project: i64,
    stored: &StoredRecovery,
) -> Result<(TerminalCorrection, String)> {
    let source = terminal_correction_for_receipt(conn, project, stored)?;
    let successor_is_current: bool = conn.query_row(
        r#"
        select exists(
          select 1 from design_versions version
          join design_packages package on package.id=version.design_package_id
          where version.project_id=?1 and version.id=?2
            and package.current_design_version_id=version.id
        )
        "#,
        params![project, stored.successor_design_version_id],
        |row| row.get(0),
    )?;
    if !successor_is_current {
        bail!(
            "recovery_successor_not_current; next: agent-workbench design inspect {}",
            stored.successor_alias
        );
    }
    let postcondition_digest =
        validate_file_postconditions(conn, root, stored.finding_id, stored.source_closure_id)?;
    if postcondition_digest != stored.postcondition_digest {
        bail!(
            "recovery_postconditions_changed; next: restore the exact corrected file state recorded by recovery {} and retry",
            stored.recovery_handle
        );
    }
    Ok((source, postcondition_digest))
}

fn stored_recovery_outcome(
    conn: &rusqlite::Connection,
    project: i64,
    source: &TerminalCorrection,
    stored: StoredRecovery,
    idempotent: bool,
    converged: bool,
) -> Result<FindingDesignRecoveryOutcome> {
    let next_action = current_recovery_next_action(conn, project, source, &stored)?;
    Ok(FindingDesignRecoveryOutcome {
        recovery_handle: stored.recovery_handle,
        finding_id: stored.finding_id,
        terminal_epoch: stored.terminal_epoch,
        source_closure_id: stored.source_closure_id,
        source_session_id: stored.source_session_id,
        source_attempt_id: stored.source_attempt_id,
        corrected_design_version_id: stored.successor_design_version_id,
        corrected_design_ref: stored.successor_alias,
        successor_closure_id: stored.successor_closure_id,
        successor_session_id: stored.successor_session_id,
        successor_attempt_id: stored.successor_attempt_id,
        context_ref: finding_fix_context_ref(
            stored.finding_id,
            stored.successor_closure_id,
            stored.successor_attempt_id,
        ),
        next_action,
        idempotent,
        converged,
    })
}
