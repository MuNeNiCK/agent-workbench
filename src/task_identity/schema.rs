pub(super) const SQL: &str = r#"
create table if not exists task_identities (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    owner_work_unit_id integer not null references work_units(id) on delete cascade,
    identity_digest text not null,
    kind text not null check(kind in ('design','manual')),
    status text not null check(status in ('current','retired')),
    created_at text not null,
    unique(project_id,identity_digest)
);

create table if not exists task_revisions (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    task_identity_id integer not null references task_identities(id) on delete cascade,
    source_design_requirement_id integer references design_requirements(id),
    revision_digest text not null,
    design_sequence integer,
    status text not null check(status in ('current','historical','retired')),
    created_at text not null,
    unique(project_id,revision_digest)
);

create table if not exists task_revision_aliases (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    task_revision_id integer not null references task_revisions(id) on delete cascade,
    historical_task_id integer not null references tasks(id),
    source_schema integer not null,
    created_at text not null,
    unique(project_id,historical_task_id)
);

create table if not exists task_phase_memberships (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    phase_id integer not null references work_phases(id) on delete cascade,
    task_identity_id integer not null references task_identities(id) on delete cascade,
    boundary_revision_id integer references task_revisions(id),
    state text not null check(state in ('open','blocked','closed','out_of_scope','split')),
    created_at text not null,
    unique(project_id,phase_id,task_identity_id)
);

create table if not exists task_phase_membership_sources (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    task_phase_membership_id integer not null references task_phase_memberships(id) on delete cascade,
    source_membership_id integer not null references work_phase_task_memberships(id),
    created_at text not null,
    unique(project_id,source_membership_id)
);

create table if not exists task_identity_dependencies (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    from_task_identity_id integer not null references task_identities(id) on delete cascade,
    to_task_identity_id integer not null references task_identities(id) on delete cascade,
    state text not null check(state in ('open','completed','out_of_scope')),
    created_at text not null,
    unique(project_id,from_task_identity_id,to_task_identity_id)
);

create table if not exists task_identity_dependency_sources (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    task_identity_dependency_id integer not null references task_identity_dependencies(id) on delete cascade,
    source_dependency_id integer not null references work_phase_dependencies(id),
    created_at text not null,
    unique(project_id,task_identity_dependency_id,source_dependency_id)
);

create table if not exists task_completion_claims (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    task_identity_id integer not null references task_identities(id) on delete cascade,
    task_revision_id integer not null references task_revisions(id) on delete cascade,
    completion_digest text not null,
    state text not null check(state in ('completed')),
    created_at text not null,
    unique(project_id,completion_digest)
);

create table if not exists task_completion_sources (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    task_completion_claim_id integer not null references task_completion_claims(id) on delete cascade,
    source_kind text not null check(source_kind in ('implementation','coverage','validation')),
    source_record_id integer not null,
    source_digest text not null,
    created_at text not null,
    unique(project_id,task_completion_claim_id,source_kind,source_record_id)
);

create table if not exists task_identity_migration_audits (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    owner_work_unit_id integer not null references work_units(id),
    owner_digest text not null,
    component_digest text not null,
    source_digest text not null,
    database_digest text not null,
    plan_digest text not null,
    plan_mode text not null check(plan_mode in ('base','resolved')),
    backup_digest text not null,
    intent_digest text not null,
    audit_digest text not null,
    status text not null check(status in ('applied')),
    created_at text not null,
    unique(project_id,owner_digest)
);
"#;
