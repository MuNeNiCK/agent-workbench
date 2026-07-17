pub(super) const SQL: &str = r#"
create table if not exists authority_provider_snapshots (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    provider text not null check(provider='signed-envelope-v1'),
    trust_digest text not null check(length(trust_digest)=64),
    verified_at text not null,
    unique(project_id, provider, trust_digest)
);

create table if not exists authority_grant_epochs (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    epoch_digest text not null check(length(epoch_digest)=64),
    trust_digest text not null check(length(trust_digest)=64),
    status text not null check(status in ('open','closed','rolled_back')),
    created_at text not null,
    closed_at text,
    unique(project_id, epoch_digest)
);

create table if not exists authority_bootstrap_journals (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    epoch_id integer not null references authority_grant_epochs(id),
    operation text not null check(operation in ('provider_verify','root_import','capability_import','principal_resolve','root_issue','capability_issue','review_adjudicate')),
    ordinal integer not null check(ordinal between 1 and 7),
    input_digest text not null check(length(input_digest)=64),
    outcome_digest text,
    status text not null check(status in ('pending','complete','failed','rolled_back')),
    created_at text not null,
    completed_at text,
    unique(project_id, epoch_id, ordinal)
);

create table if not exists legacy_reviewer_bindings (
    id integer primary key, project_id integer not null references projects(id) on delete cascade,
    source_ledger_digest text not null check(length(source_ledger_digest)=64), source_generation integer not null,
    source_reviewer_digest text not null check(length(source_reviewer_digest)=64), principal_id integer not null references authority_principals(id),
    assertion_id integer not null references authority_assertions(id), binding_handle text not null, created_at text not null,
    idempotency_key text not null, payload_digest text not null check(length(payload_digest)=64),
    unique(project_id,source_ledger_digest,source_generation,source_reviewer_digest), unique(project_id,binding_handle), unique(project_id,idempotency_key)
);
create table if not exists authority_migration_sources (
    id integer primary key, project_id integer not null references projects(id) on delete cascade,
    source_ledger_digest text not null check(length(source_ledger_digest)=64), source_generation integer not null,
    created_at text not null, unique(project_id)
);
create table if not exists legacy_adjudication_migrations (
    id integer primary key, project_id integer not null references projects(id) on delete cascade,
    source_ledger_digest text not null check(length(source_ledger_digest)=64), source_generation integer not null,
    completed_at text not null, unique(project_id)
);
create trigger if not exists trg_legacy_adjudication_migration_update before update on legacy_adjudication_migrations begin select raise(abort,'legacy adjudication migration is immutable'); end;
create trigger if not exists trg_legacy_adjudication_migration_delete before delete on legacy_adjudication_migrations begin select raise(abort,'legacy adjudication migration is immutable'); end;
create trigger if not exists trg_authority_migration_source_update before update on authority_migration_sources begin select raise(abort,'authority migration source is immutable'); end;
create trigger if not exists trg_authority_migration_source_delete before delete on authority_migration_sources begin select raise(abort,'authority migration source is immutable'); end;
create table if not exists legacy_claim_audits (
    id integer primary key, project_id integer not null references projects(id) on delete cascade,
    review_run_id integer not null references review_runs(id), candidate_kind text not null,
    content_digest text not null check(length(content_digest)=64), reviewer_resolution text not null check(reviewer_resolution in ('trusted','unbound','ambiguous')),
    mapping_row text not null, before_lifecycle text not null, after_lifecycle text not null, created_at text not null,
    unique(project_id,review_run_id)
);
create table if not exists legacy_finding_audits (
    id integer primary key, project_id integer not null references projects(id) on delete cascade,
    finding_id integer not null references findings(id), review_run_id integer not null references review_runs(id),
    content_digest text not null check(length(content_digest)=64), created_at text not null, unique(project_id,finding_id)
);
create table if not exists authority_bootstrap_targets (
    id integer primary key, project_id integer not null references projects(id) on delete cascade,
    epoch_id integer not null references authority_grant_epochs(id), target_handle text not null, owner_ref text not null,
    boundary_handle text not null, claim_handle text not null, context_digest text not null check(length(context_digest)=64),
    status text not null check(status in ('pending','satisfied','retired')), created_at text not null, resolved_at text,
    unique(project_id,target_handle), unique(epoch_id,owner_ref,boundary_handle,claim_handle,context_digest)
);
create table if not exists legacy_migration_candidates (
    id integer primary key, project_id integer not null references projects(id) on delete cascade,
    candidate_kind text not null check(candidate_kind in ('invocation','completed_run','finding_epoch','plan_gate','work_owner','completed_boundary')),
    candidate_handle text not null, base_digest text not null check(length(base_digest)=64), content_digest text not null check(length(content_digest)=64),
    boundary_generation integer, commit_sequence integer,
    created_at text not null,
    check(candidate_kind='completed_boundary' or (boundary_generation is null and commit_sequence is null)),
    unique(project_id,candidate_handle)
);
create table if not exists legacy_migration_candidate_members (
    id integer primary key, project_id integer not null references projects(id) on delete cascade,
    candidate_id integer not null references legacy_migration_candidates(id), source_table text not null, source_row_id integer not null,
    member_digest text not null check(length(member_digest)=64), created_at text not null,
    unique(project_id,source_table,source_row_id)
);
create table if not exists legacy_migration_edges (
    id integer primary key, project_id integer not null references projects(id) on delete cascade,
    edge_kind text not null check(edge_kind in ('plan_has_run','run_reports_finding','finding_has_verification','boundary_consumes','work_depends_on')),
    source_candidate_id integer not null references legacy_migration_candidates(id), target_candidate_id integer not null references legacy_migration_candidates(id),
    edge_digest text not null check(length(edge_digest)=64), created_at text not null,
    unique(project_id,edge_kind,source_candidate_id,target_candidate_id)
);
create table if not exists legacy_migration_projections (
    id integer primary key, project_id integer not null references projects(id) on delete cascade,
    candidate_id integer not null references legacy_migration_candidates(id), stratum integer not null check(stratum between 1 and 6),
    mapping_row text not null, before_lifecycle text not null, after_lifecycle text not null, created_at text not null,
    unique(project_id,candidate_id,stratum)
);
create trigger if not exists trg_legacy_candidate_update before update on legacy_migration_candidates
when exists(select 1 from legacy_adjudication_migrations m where m.project_id=old.project_id)
begin select raise(abort,'legacy migration candidates are append-only'); end;
create trigger if not exists trg_legacy_candidate_delete before delete on legacy_migration_candidates begin select raise(abort,'legacy migration candidates are append-only'); end;
create trigger if not exists trg_legacy_member_update before update on legacy_migration_candidate_members begin select raise(abort,'legacy migration members are append-only'); end;
create trigger if not exists trg_legacy_member_delete before delete on legacy_migration_candidate_members begin select raise(abort,'legacy migration members are append-only'); end;
create trigger if not exists trg_legacy_edge_update before update on legacy_migration_edges begin select raise(abort,'legacy migration edges are append-only'); end;
create trigger if not exists trg_legacy_edge_delete before delete on legacy_migration_edges begin select raise(abort,'legacy migration edges are append-only'); end;
create trigger if not exists trg_legacy_projection_update before update on legacy_migration_projections begin select raise(abort,'legacy migration projections are append-only'); end;
create trigger if not exists trg_legacy_projection_delete before delete on legacy_migration_projections begin select raise(abort,'legacy migration projections are append-only'); end;

create trigger if not exists trg_authority_epoch_immutable
before update on authority_grant_epochs for each row
when new.project_id!=old.project_id or new.epoch_digest!=old.epoch_digest or new.trust_digest!=old.trust_digest or new.created_at!=old.created_at
begin select raise(abort,'authority grant epoch identity is immutable'); end;
create trigger if not exists trg_authority_bootstrap_journal_immutable
before update on authority_bootstrap_journals for each row
when new.project_id!=old.project_id or new.epoch_id!=old.epoch_id or new.operation!=old.operation or new.ordinal!=old.ordinal or new.input_digest!=old.input_digest or new.created_at!=old.created_at
begin select raise(abort,'authority bootstrap journal identity is immutable'); end;
create trigger if not exists trg_legacy_reviewer_binding_immutable before update on legacy_reviewer_bindings begin select raise(abort,'legacy reviewer bindings are append-only'); end;
create trigger if not exists trg_legacy_claim_audit_immutable before update on legacy_claim_audits begin select raise(abort,'legacy claim audits are append-only'); end;
create trigger if not exists trg_legacy_finding_audit_immutable before update on legacy_finding_audits begin select raise(abort,'legacy finding audits are append-only'); end;

create table if not exists authority_assertions (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    provider text not null check(provider='signed-envelope-v1'),
    purpose text not null check(purpose in ('root_grant','grant_delegate','grant_revoke','capability_issue','review_provenance','legacy_reviewer_binding')),
    assertion_digest text not null check(length(assertion_digest)=64),
    assertion_id text not null,
    nonce text not null,
    key_id text not null check(length(key_id)=32),
    subject_kind text not null check(subject_kind in ('human','agent','service')),
    subject_digest text not null check(length(subject_digest)=64),
    project_digest text not null check(length(project_digest)=64),
    trust_digest text not null check(length(trust_digest)=64),
    payload_digest text not null check(length(payload_digest)=64),
    payload_cbor blob not null,
    envelope_cbor blob not null,
    issued_at text not null,
    expires_at text not null,
    consumed_at text,
    created_at text not null,
    unique(project_id, assertion_digest),
    unique(project_id, assertion_id),
    unique(project_id, nonce)
);

create table if not exists authority_principals (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    principal_handle text not null,
    provider text not null check(provider='signed-envelope-v1'),
    subject_kind text not null check(subject_kind in ('human','agent','service')),
    subject_digest text not null check(length(subject_digest)=64),
    created_at text not null,
    unique(project_id, principal_handle),
    unique(project_id, provider, subject_kind, subject_digest)
);

create table if not exists review_provenance_records (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    provenance_handle text not null,
    principal_id integer not null references authority_principals(id),
    assertion_id integer not null references authority_assertions(id),
    review_plan_id integer not null references review_plans(id),
    target_context text not null,
    provenance_kind text not null check(provenance_kind in ('human_review','external_agent','service_review')),
    review_purpose text not null check(review_purpose in ('new_unbiased_review','finding_fix_verification')),
    reference_digest text not null check(length(reference_digest)=64),
    idempotency_key text not null,
    payload_digest text not null check(length(payload_digest)=64),
    created_at text not null,
    unique(project_id, provenance_handle),
    unique(project_id, principal_id, idempotency_key)
);

create trigger if not exists trg_review_provenance_immutable_update
before update on review_provenance_records begin select raise(abort,'review provenance is append-only'); end;
create trigger if not exists trg_review_provenance_immutable_delete
before delete on review_provenance_records begin select raise(abort,'review provenance is append-only'); end;

create table if not exists owner_decision_grants (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    grant_handle text not null,
    parent_grant_id integer references owner_decision_grants(id),
    owner_ref text not null,
    grantor_principal_id integer not null references authority_principals(id),
    grantee_principal_id integer not null references authority_principals(id),
    maximum_target text not null,
    roles text not null,
    decision_families text not null,
    actions text not null,
    maximum_depth integer not null check(maximum_depth>=0),
    expires_at text not null,
    assertion_id integer not null references authority_assertions(id),
    status text not null check(status in ('active','revoked','expired')),
    created_at text not null,
    revoked_at text,
    unique(project_id, grant_handle)
);

create table if not exists decision_capabilities (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    capability_handle text not null,
    owner_grant_id integer not null references owner_decision_grants(id),
    issuer_principal_id integer not null references authority_principals(id),
    holder_principal_id integer not null references authority_principals(id),
    owner_ref text not null,
    target_ref text not null,
    role text not null check(role in ('review_adjudicator','finding_adjudicator','verification_adjudicator','human_authority')),
    decision_family text not null check(decision_family in ('review','finding','verification')),
    action text not null check(action in ('adjudicate','dispose','bootstrap_adjudicate','correct_terminal','reopen')),
    design_context text not null check(length(design_context)=64),
    assertion_id integer not null references authority_assertions(id),
    expires_at text not null,
    status text not null check(status in ('active','consumed','revoked','expired')),
    created_at text not null,
    consumed_at text,
    unique(project_id, capability_handle)
);

create table if not exists capability_issue_audits (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    capability_id integer not null unique references decision_capabilities(id),
    assertion_id integer not null references authority_assertions(id),
    owner_grant_id integer not null references owner_decision_grants(id),
    principal_id integer not null references authority_principals(id),
    lineage_digest text not null check(length(lineage_digest)=64),
    binding_digest text not null check(length(binding_digest)=64),
    created_at text not null
);

create table if not exists capability_consumption_audits (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    capability_id integer not null unique references decision_capabilities(id),
    attempted_principal text not null,
    attempted_owner text not null,
    attempted_target text not null,
    attempted_design_context text not null check(length(attempted_design_context)=64),
    attempted_family text not null,
    attempted_action text not null,
    presentation_digest text not null check(length(presentation_digest)=64),
    outcome text not null check(outcome in ('pending','accepted','rejected')),
    rejection_reason text,
    decision_handle text,
    attempted_at text not null,
    completed_at text,
    check((outcome='pending' and rejection_reason is null and decision_handle is null and completed_at is null)
       or (outcome='accepted' and rejection_reason is null and decision_handle is not null and completed_at is not null)
       or (outcome='rejected' and rejection_reason is not null and decision_handle is null and completed_at is not null))
);

create table if not exists authority_security_audits (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    boundary text not null,
    presented_handle text not null,
    presentation_digest text not null check(length(presentation_digest)=64),
    reason text not null,
    created_at text not null
);

create table if not exists owner_decisions (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decision_handle text not null,
    capability_id integer not null unique references decision_capabilities(id),
    principal_id integer not null references authority_principals(id),
    owner_ref text not null,
    target_ref text not null,
    decision_family text not null,
    action text not null,
    decision_value text not null,
    reason text not null,
    expected_current text not null,
    payload_digest text not null check(length(payload_digest)=64),
    created_at text not null,
    unique(project_id, decision_handle)
);

create table if not exists decision_continuations (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    continuation_handle text not null,
    command_kind text not null,
    owner_ref text not null,
    target_ref text not null,
    decision_family text not null,
    action text not null,
    expected_current text not null,
    design_context text not null check(length(design_context)=64),
    rejection_code text not null,
    status text not null check(status in ('pending','applied')),
    created_at text not null,
    applied_at text,
    unique(project_id, continuation_handle)
);

create trigger if not exists trg_decision_continuation_immutable
before update on decision_continuations for each row
when old.status!='pending' or new.project_id!=old.project_id
  or new.continuation_handle!=old.continuation_handle or new.command_kind!=old.command_kind
  or new.owner_ref!=old.owner_ref or new.target_ref!=old.target_ref
  or new.decision_family!=old.decision_family or new.action!=old.action
  or new.expected_current!=old.expected_current or new.design_context!=old.design_context
  or new.rejection_code!=old.rejection_code or new.created_at!=old.created_at
  or new.status!='applied' or new.applied_at is null
begin select raise(abort,'decision continuation identity is immutable'); end;

create trigger if not exists trg_authority_assertion_immutable
before update on authority_assertions for each row
when new.project_id!=old.project_id or new.provider!=old.provider or new.purpose!=old.purpose
  or new.assertion_digest!=old.assertion_digest or new.assertion_id!=old.assertion_id
  or new.nonce!=old.nonce or new.key_id!=old.key_id or new.subject_kind!=old.subject_kind
  or new.subject_digest!=old.subject_digest or new.project_digest!=old.project_digest
  or new.trust_digest!=old.trust_digest or new.payload_digest!=old.payload_digest
  or new.payload_cbor!=old.payload_cbor or new.envelope_cbor!=old.envelope_cbor or new.issued_at!=old.issued_at
  or new.expires_at!=old.expires_at or new.created_at!=old.created_at
begin select raise(abort,'authority assertion identity is immutable'); end;

create trigger if not exists trg_owner_grant_immutable
before update on owner_decision_grants for each row
when new.project_id!=old.project_id or new.grant_handle!=old.grant_handle
  or new.parent_grant_id is not old.parent_grant_id or new.owner_ref!=old.owner_ref
  or new.grantor_principal_id!=old.grantor_principal_id
  or new.grantee_principal_id!=old.grantee_principal_id
  or new.maximum_target!=old.maximum_target or new.roles!=old.roles
  or new.decision_families!=old.decision_families or new.actions!=old.actions
  or new.maximum_depth!=old.maximum_depth or new.expires_at!=old.expires_at
  or new.assertion_id!=old.assertion_id or new.created_at!=old.created_at
begin select raise(abort,'owner decision grant identity is immutable'); end;

create trigger if not exists trg_capability_immutable
before update on decision_capabilities for each row
when new.project_id!=old.project_id or new.capability_handle!=old.capability_handle
  or new.owner_grant_id!=old.owner_grant_id or new.issuer_principal_id!=old.issuer_principal_id
  or new.holder_principal_id!=old.holder_principal_id or new.owner_ref!=old.owner_ref
  or new.target_ref!=old.target_ref or new.role!=old.role
  or new.decision_family!=old.decision_family or new.action!=old.action
  or new.design_context!=old.design_context or new.assertion_id!=old.assertion_id
  or new.expires_at!=old.expires_at or new.created_at!=old.created_at
begin select raise(abort,'decision capability identity is immutable'); end;

create trigger if not exists trg_owner_decision_immutable_update
before update on owner_decisions begin select raise(abort,'owner decisions are append-only'); end;
create trigger if not exists trg_owner_decision_immutable_delete
before delete on owner_decisions begin select raise(abort,'owner decisions are append-only'); end;
create trigger if not exists trg_capability_issue_audit_immutable_update
before update on capability_issue_audits begin select raise(abort,'capability issue audits are append-only'); end;
create trigger if not exists trg_capability_issue_audit_immutable_delete
before delete on capability_issue_audits begin select raise(abort,'capability issue audits are append-only'); end;
create trigger if not exists trg_capability_consumption_audit_transition
before update on capability_consumption_audits for each row
when old.outcome!='pending' or new.project_id!=old.project_id or new.capability_id!=old.capability_id
  or new.attempted_principal!=old.attempted_principal or new.attempted_owner!=old.attempted_owner
  or new.attempted_target!=old.attempted_target or new.attempted_design_context!=old.attempted_design_context
  or new.attempted_family!=old.attempted_family or new.attempted_action!=old.attempted_action
  or new.presentation_digest!=old.presentation_digest or new.attempted_at!=old.attempted_at
  or new.outcome='pending'
begin select raise(abort,'capability consumption audit is immutable after completion'); end;
create trigger if not exists trg_capability_consumption_audit_delete
before delete on capability_consumption_audits begin select raise(abort,'capability consumption audits are append-only'); end;
create trigger if not exists trg_authority_security_audit_immutable_update
before update on authority_security_audits begin select raise(abort,'authority security audits are append-only'); end;
create trigger if not exists trg_authority_security_audit_immutable_delete
before delete on authority_security_audits begin select raise(abort,'authority security audits are append-only'); end;
"#;
