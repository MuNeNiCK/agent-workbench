pub const SQL: &str = r#"
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
create table if not exists schema_retirement_records (
    id integer primary key, project_id integer not null references projects(id) on delete cascade,
    source_ledger_digest text not null check(length(source_ledger_digest)=64),
    source_generation integer not null, completed_at text not null,
    unique(project_id,source_generation)
);
create trigger if not exists trg_schema_retirement_update before update on schema_retirement_records begin select raise(abort,'schema retirement record is immutable'); end;
create trigger if not exists trg_schema_retirement_delete before delete on schema_retirement_records begin select raise(abort,'schema retirement record is immutable'); end;
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
create table if not exists legacy_review_acceptance_migrations (
    id integer primary key, project_id integer not null references projects(id) on delete cascade,
    review_run_id integer not null references review_runs(id), owner_decision_id integer not null references owner_decisions(id),
    content_digest text not null check(length(content_digest)=64), created_at text not null,
    unique(project_id,review_run_id), unique(project_id,owner_decision_id)
);
create table if not exists legacy_finding_audits (
    id integer primary key, project_id integer not null references projects(id) on delete cascade,
    finding_id integer not null references findings(id), review_run_id integer not null references review_runs(id),
    content_digest text not null check(length(content_digest)=64), created_at text not null, unique(project_id,finding_id)
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

create trigger if not exists trg_legacy_claim_audit_immutable
before update on legacy_claim_audits begin select raise(abort,'legacy claim audits are append-only'); end;
create trigger if not exists trg_legacy_review_acceptance_migration_update
before update on legacy_review_acceptance_migrations begin select raise(abort,'legacy review acceptance migrations are append-only'); end;
create trigger if not exists trg_legacy_review_acceptance_migration_delete
before delete on legacy_review_acceptance_migrations begin select raise(abort,'legacy review acceptance migrations are append-only'); end;
create trigger if not exists trg_legacy_finding_audit_immutable
before update on legacy_finding_audits begin select raise(abort,'legacy finding audits are append-only'); end;

create table if not exists owner_decisions (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decision_handle text not null,
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

create trigger if not exists trg_owner_decision_immutable_update
before update on owner_decisions begin select raise(abort,'owner decisions are append-only'); end;
create trigger if not exists trg_owner_decision_immutable_delete
before delete on owner_decisions begin select raise(abort,'owner decisions are append-only'); end;
"#;
