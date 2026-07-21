pub(crate) const GENERATION_14_SQL: &str = r#"
create table if not exists update_operations (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    operation_handle text not null,
    source_descriptor text not null,
    expected_current text not null,
    status text not null check (status in (
        'decision_recorded','prepared','published','restored','failed_recoverable'
    )),
    backup_handle text,
    target_identity text,
    edge_path text not null,
    idempotency_key text not null,
    created_at text not null,
    updated_at text not null,
    unique(project_id, operation_handle),
    unique(project_id, idempotency_key)
);

create table if not exists update_decisions (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    update_operation_id integer not null references update_operations(id) on delete cascade,
    choice_key text not null,
    authority_event_id integer not null references authority_events(id),
    reason text not null,
    source_revision text not null,
    predecessor_id integer references update_decisions(id),
    status text not null check (status in ('recorded','superseded')),
    created_at text not null,
    unique(update_operation_id, choice_key, source_revision)
);

create table if not exists update_receipts (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    update_operation_id integer not null unique references update_operations(id) on delete cascade,
    source_identity text not null,
    target_identity text,
    backup_handle text not null,
    edge_path text not null,
    status text not null check (status in (
        'prepared','published','restored','failed_recoverable'
    )),
    prepared_at text not null,
    completed_at text
);

create table if not exists release_candidates (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    candidate_handle text not null,
    version text not null,
    reviewed_commit text not null,
    manifest_identity text not null,
    status text not null check (status in (
        'assembled','locally_verified','source_published','assets_published',
        'remotely_verified','source_conflict','asset_conflict','withdrawn','superseded'
    )),
    predecessor_id integer references release_candidates(id),
    idempotency_key text not null,
    created_at text not null,
    updated_at text not null,
    unique(project_id, candidate_handle),
    unique(project_id, version, reviewed_commit, manifest_identity),
    unique(project_id, idempotency_key)
);

create table if not exists release_candidate_assets (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    release_candidate_id integer not null references release_candidates(id) on delete cascade,
    asset_name text not null,
    expected_identity text not null,
    local_identity text,
    remote_identity text,
    status text not null check (status in (
        'expected','locally_verified','published','remotely_verified','conflict'
    )),
    created_at text not null,
    updated_at text not null,
    unique(release_candidate_id, asset_name)
);

create table if not exists release_candidate_events (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    release_candidate_id integer not null references release_candidates(id) on delete cascade,
    event_type text not null,
    previous_status text,
    next_status text not null,
    observed_identity text,
    reason text,
    created_at text not null
);
"#;

pub(crate) const GENERATION_15_SQL: &str = r#"
create table if not exists decomposition_plans (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    work_unit_id integer references work_units(id) on delete cascade,
    design_version_id integer not null references design_versions(id) on delete cascade,
    plan_key text not null,
    revision integer not null check (revision > 0),
    source_path text,
    source_identity text not null,
    source_kind text not null check (source_kind in ('document','derived_bundle')),
    design_fingerprint text not null,
    status text not null check (status in ('draft','ready','applied','incomplete','superseded')),
    binding_issue text,
    predecessor_id integer references decomposition_plans(id),
    created_at text not null,
    applied_at text,
    unique(project_id, work_unit_id, design_version_id, plan_key, revision),
    unique(project_id, source_identity)
);

create unique index if not exists decomposition_plan_current_unique
on decomposition_plans(project_id,work_unit_id,design_version_id)
where status != 'superseded' and work_unit_id is not null;

create table if not exists decomposition_slices (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decomposition_plan_id integer not null references decomposition_plans(id) on delete cascade,
    slice_key text not null,
    title text not null,
    slice_order integer not null check (slice_order > 0),
    unique(decomposition_plan_id, slice_key),
    unique(decomposition_plan_id, slice_order)
);

create table if not exists decomposition_slice_dependencies (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decomposition_plan_id integer not null references decomposition_plans(id) on delete cascade,
    predecessor_slice_id integer not null references decomposition_slices(id),
    successor_slice_id integer not null references decomposition_slices(id),
    check(predecessor_slice_id != successor_slice_id),
    unique(decomposition_plan_id, predecessor_slice_id, successor_slice_id)
);

create table if not exists decomposition_items (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decomposition_plan_id integer not null references decomposition_plans(id) on delete cascade,
    item_key text not null,
    title text not null,
    details text not null,
    outcome text not null,
    observation text not null,
    evidence_owner text not null,
    evidence_kind text not null,
    slice_id integer references decomposition_slices(id),
    status text not null check (status in ('open','closed','accepted_out_of_scope','superseded')),
    unique(decomposition_plan_id, item_key)
);

create table if not exists decomposition_item_requirements (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decomposition_item_id integer not null references decomposition_items(id) on delete cascade,
    design_requirement_id integer not null references design_requirements(id),
    unique(decomposition_item_id, design_requirement_id)
);

create table if not exists decomposition_item_checklist_boundaries (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decomposition_item_id integer not null references decomposition_items(id) on delete cascade,
    boundary_key text not null,
    title text,
    condition_text text not null,
    evidence_kind text not null,
    boundary_order integer not null check (boundary_order > 0),
    unique(decomposition_item_id, boundary_key),
    unique(decomposition_item_id, boundary_order)
);

create table if not exists decomposition_item_gates (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decomposition_item_id integer not null references decomposition_items(id) on delete cascade,
    gate_key text not null,
    unique(decomposition_item_id, gate_key)
);

create table if not exists decomposition_item_checklist_boundary_gates (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decomposition_item_checklist_boundary_id integer not null references decomposition_item_checklist_boundaries(id) on delete cascade,
    gate_key text not null,
    unique(decomposition_item_checklist_boundary_id,gate_key)
);

create table if not exists decomposition_applications (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decomposition_plan_id integer not null references decomposition_plans(id) on delete cascade,
    decomposition_item_id integer not null unique references decomposition_items(id) on delete cascade,
    task_id integer not null references tasks(id),
    checklist_id integer not null references checklists(id),
    phase_id integer not null references work_phases(id),
    applied_at text not null,
    unique(decomposition_plan_id, task_id)
);

create table if not exists decomposition_lineage (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    predecessor_plan_id integer not null references decomposition_plans(id),
    predecessor_item_id integer not null references decomposition_items(id),
    successor_plan_id integer not null references decomposition_plans(id),
    successor_item_id integer references decomposition_items(id),
    disposition text not null check (disposition in ('retained','revised','retired')),
    reason text,
    unique(predecessor_plan_id, predecessor_item_id, successor_plan_id)
);

create table if not exists decomposition_migration_sources (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decomposition_plan_id integer not null references decomposition_plans(id) on delete cascade,
    decomposition_item_id integer not null references decomposition_items(id) on delete cascade,
    source_task_id integer not null references tasks(id),
    source_checklist_item_id integer references checklist_items(id),
    source_phase_id integer references work_phases(id),
    mapping_state text not null check (mapping_state in ('exact','missing','ambiguous')),
    issue text,
    unique(decomposition_item_id,source_task_id,source_checklist_item_id,source_phase_id)
);
"#;

pub(crate) const GENERATION_15_APPLICATION_LINK_SQL: &str = r#"
create table if not exists decomposition_item_checklist_boundary_gates (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decomposition_item_checklist_boundary_id integer not null references decomposition_item_checklist_boundaries(id) on delete cascade,
    gate_key text not null,
    unique(decomposition_item_checklist_boundary_id,gate_key)
);

create table if not exists decomposition_application_requirements (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decomposition_item_requirement_id integer not null unique references decomposition_item_requirements(id),
    task_derivation_id integer not null unique references task_derivations(id)
);

create table if not exists decomposition_application_boundaries (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decomposition_item_checklist_boundary_id integer not null unique references decomposition_item_checklist_boundaries(id),
    checklist_item_id integer not null unique references checklist_items(id)
);

create table if not exists decomposition_application_gates (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decomposition_item_gate_id integer not null references decomposition_item_gates(id),
    validation_gate_id integer not null unique references validation_gates(id),
    unique(decomposition_item_gate_id,validation_gate_id)
);

create table if not exists decomposition_application_dependencies (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decomposition_slice_dependency_id integer not null unique references decomposition_slice_dependencies(id),
    work_phase_dependency_id integer not null unique references work_phase_dependencies(id)
);

create table if not exists decomposition_reconciliation_tasks (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decomposition_plan_id integer not null references decomposition_plans(id) on delete cascade,
    source_task_id integer not null references tasks(id),
    successor_item_id integer references decomposition_items(id),
    disposition text not null check(disposition in ('retained','retired')),
    reason text,
    unique(decomposition_plan_id,source_task_id)
);

create table if not exists decomposition_reconciliation_checklist_items (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decomposition_plan_id integer not null references decomposition_plans(id) on delete cascade,
    source_checklist_item_id integer not null references checklist_items(id),
    successor_boundary_id integer references decomposition_item_checklist_boundaries(id),
    disposition text not null check(disposition in ('retained','retired')),
    reason text,
    unique(decomposition_plan_id,source_checklist_item_id)
);

create table if not exists decomposition_reconciliation_gates (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decomposition_plan_id integer not null references decomposition_plans(id) on delete cascade,
    source_validation_gate_id integer not null references validation_gates(id),
    successor_item_gate_id integer references decomposition_item_gates(id),
    disposition text not null check(disposition in ('retained','retired')),
    reason text,
    unique(decomposition_plan_id,source_validation_gate_id)
);

create table if not exists decomposition_reconciliation_phases (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decomposition_plan_id integer not null references decomposition_plans(id) on delete cascade,
    source_phase_id integer not null references work_phases(id),
    successor_slice_id integer references decomposition_slices(id),
    disposition text not null check(disposition in ('retained','retired')),
    reason text,
    unique(decomposition_plan_id,source_phase_id)
);

create table if not exists decomposition_reconciliation_dependencies (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decomposition_plan_id integer not null references decomposition_plans(id) on delete cascade,
    source_dependency_id integer not null references work_phase_dependencies(id),
    successor_dependency_id integer references decomposition_slice_dependencies(id),
    disposition text not null check(disposition in ('retained','retired')),
    reason text,
    unique(decomposition_plan_id,source_dependency_id)
);

create table if not exists decomposition_reconciliation_applications (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    correction_application_id integer not null unique references correction_transition_applications(id),
    correction_token_id integer not null unique references correction_tokens(id),
    predecessor_plan_id integer not null references decomposition_plans(id),
    successor_plan_id integer not null unique references decomposition_plans(id),
    source_identity text not null,
    expected_current text not null,
    payload_identity text not null unique,
    created_at text not null
);

drop trigger if exists trg_decomposition_reconciliation_application_links_insert;
create trigger if not exists trg_decomposition_reconciliation_application_links_insert
before insert on decomposition_reconciliation_applications
for each row when
    new.project_id != (select project_id from correction_transition_applications where id=new.correction_application_id)
    or new.project_id != (select project_id from correction_tokens where id=new.correction_token_id)
    or new.correction_token_id != (
        select correction_token_id from correction_transition_applications
        where id=new.correction_application_id
    )
    or 'decomposition-plan-reconcile' != (
        select operation from correction_tokens where id=new.correction_token_id
    )
    or new.project_id != (select project_id from decomposition_plans where id=new.predecessor_plan_id)
    or new.project_id != (select project_id from decomposition_plans where id=new.successor_plan_id)
    or 'superseded' != (select status from decomposition_plans where id=new.predecessor_plan_id)
    or 'applied' != (select status from decomposition_plans where id=new.successor_plan_id)
    or new.predecessor_plan_id != (
        select predecessor_id from decomposition_plans where id=new.successor_plan_id
    )
    or 'decomposition-plan:'||new.successor_plan_id != (
        select result_ref from correction_transition_applications
        where id=new.correction_application_id
    )
    or (select target from correction_tokens where id=new.correction_token_id) not like (
        select cast(design_version_id as text)||'/'||cast(work_unit_id as text)||'/b64:%'
        from decomposition_plans where id=new.successor_plan_id
    )
begin select raise(abort, 'invalid decomposition reconciliation application links'); end;

create trigger if not exists trg_decomposition_reconciliation_application_immutable_update
before update on decomposition_reconciliation_applications
begin select raise(abort, 'decomposition reconciliation applications are immutable'); end;

create trigger if not exists trg_decomposition_reconciliation_application_immutable_delete
before delete on decomposition_reconciliation_applications
begin select raise(abort, 'decomposition reconciliation applications are immutable'); end;
"#;

pub(crate) const GENERATION_16_SQL: &str = r#"
create table if not exists release_candidate_revisions (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    release_candidate_id integer not null references release_candidates(id) on delete cascade,
    revision_handle text not null,
    revision integer not null check (revision > 0),
    state text not null check (state in (
        'assembled','locally_verified','source_published','assets_published',
        'remotely_verified','source_conflict','asset_conflict',
        'withdrawn','superseded'
    )),
    stage text not null check (stage in (
        'local','source','assets','remote','terminal'
    )),
    action text not null,
    request_identity text not null,
    predecessor_id integer references release_candidate_revisions(id),
    head_state text not null check (head_state in ('current','historical')),
    reason text,
    created_at text not null,
    unique(project_id, revision_handle),
    unique(release_candidate_id, revision),
    unique(release_candidate_id, request_identity)
);

create unique index if not exists release_candidate_current_revision_unique
on release_candidate_revisions(release_candidate_id)
where head_state='current';

create table if not exists release_candidate_subject_revisions (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    release_candidate_revision_id integer not null references release_candidate_revisions(id) on delete cascade,
    subject_kind text not null check (subject_kind in ('local','source','release','asset')),
    subject_name text not null,
    expected_identity text not null,
    local_identity text,
    requested_identity text,
    observed_identity text,
    downloaded_identity text,
    unique(release_candidate_revision_id, subject_kind, subject_name),
    unique(release_candidate_revision_id, subject_name)
);

create table if not exists release_candidate_attempts (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    release_candidate_id integer not null references release_candidates(id) on delete cascade,
    action text not null,
    idempotency_key text not null,
    expected_current text not null,
    payload_identity text not null,
    requested_identity text not null,
    observed_identity text,
    result_revision_handle text,
    status text not null check(status in ('requested','completed')),
    created_at text not null,
    completed_at text,
    unique(release_candidate_id,idempotency_key)
);
"#;

pub(crate) const GENERATION_17_SQL: &str = r#"
-- The adjacent update adds lifecycle intent without replaying domain rows.
update decomposition_plans
set design_package_id=(
  select version.design_package_id from design_versions version
  where version.id=decomposition_plans.design_version_id
);

drop index if exists decomposition_plan_current_unique;
create unique index if not exists decomposition_plan_current_package_work_unique
on decomposition_plans(project_id,design_package_id,work_unit_id)
where status!='superseded' and work_unit_id is not null;

update decomposition_reconciliation_tasks set effect='preserve' where disposition='retained';
update decomposition_reconciliation_checklist_items set effect='preserve' where disposition='retained';
update decomposition_reconciliation_gates set effect='preserve' where disposition='retained';
update decomposition_reconciliation_gates
set boundary_selector='retained-source'
where disposition='retained';
update decomposition_reconciliation_phases set effect='preserve' where disposition='retained';
update decomposition_reconciliation_dependencies set effect='preserve' where disposition='retained';

create trigger if not exists decomposition_reconciliation_tasks_effect_insert
before insert on decomposition_reconciliation_tasks
for each row when (new.disposition='retained') != (new.effect is not null)
begin select raise(abort, 'retained reconciliation task requires one effect and retired forbids it'); end;
create trigger if not exists decomposition_reconciliation_tasks_effect_update
before update of disposition,effect on decomposition_reconciliation_tasks
for each row when (new.disposition='retained') != (new.effect is not null)
begin select raise(abort, 'retained reconciliation task requires one effect and retired forbids it'); end;

create trigger if not exists decomposition_reconciliation_checklist_effect_insert
before insert on decomposition_reconciliation_checklist_items
for each row when (new.disposition='retained') != (new.effect is not null)
begin select raise(abort, 'retained reconciliation checklist item requires one effect and retired forbids it'); end;
create trigger if not exists decomposition_reconciliation_checklist_effect_update
before update of disposition,effect on decomposition_reconciliation_checklist_items
for each row when (new.disposition='retained') != (new.effect is not null)
begin select raise(abort, 'retained reconciliation checklist item requires one effect and retired forbids it'); end;

create trigger if not exists decomposition_reconciliation_gates_effect_insert
before insert on decomposition_reconciliation_gates
for each row when
  (new.disposition='retained') != (new.effect is not null)
  or (new.disposition='retained') != (new.boundary_selector is not null)
  or (new.disposition='retained') != (new.resolved_boundary_identity is not null)
begin select raise(abort, 'retained reconciliation gate requires one effect and exact boundary; retired forbids them'); end;
create trigger if not exists decomposition_reconciliation_gates_effect_update
before update of disposition,effect,boundary_selector,resolved_boundary_identity on decomposition_reconciliation_gates
for each row when
  (new.disposition='retained') != (new.effect is not null)
  or (new.disposition='retained') != (new.boundary_selector is not null)
  or (new.disposition='retained') != (new.resolved_boundary_identity is not null)
begin select raise(abort, 'retained reconciliation gate requires one effect and exact boundary; retired forbids them'); end;

create trigger if not exists decomposition_plan_v2_insert
before insert on decomposition_plans
for each row when
  new.document_content is null or length(new.document_content)=0
  or new.content_identity is null or length(new.content_identity)!=64
  or new.design_package_id is null
  or new.design_package_id is not (
    select version.design_package_id from design_versions version
    where version.id=new.design_version_id and version.project_id=new.project_id
  )
begin select raise(abort, 'current Decomposition Plan revision requires owned content and package lineage'); end;

create trigger if not exists decomposition_plan_v2_update
before update of document_content,content_identity,design_package_id,design_version_id,project_id
on decomposition_plans
for each row when
  new.document_content is null or length(new.document_content)=0
  or new.content_identity is null or length(new.content_identity)!=64
  or new.design_package_id is null
  or new.design_package_id is not (
    select version.design_package_id from design_versions version
    where version.id=new.design_version_id and version.project_id=new.project_id
  )
begin select raise(abort, 'current Decomposition Plan revision requires owned content and package lineage'); end;

create trigger if not exists decomposition_reconciliation_phases_effect_insert
before insert on decomposition_reconciliation_phases
for each row when (new.disposition='retained') != (new.effect is not null)
begin select raise(abort, 'retained reconciliation phase requires one effect and retired forbids it'); end;
create trigger if not exists decomposition_reconciliation_phases_effect_update
before update of disposition,effect on decomposition_reconciliation_phases
for each row when (new.disposition='retained') != (new.effect is not null)
begin select raise(abort, 'retained reconciliation phase requires one effect and retired forbids it'); end;

create trigger if not exists decomposition_reconciliation_dependencies_effect_insert
before insert on decomposition_reconciliation_dependencies
for each row when (new.disposition='retained') != (new.effect is not null)
begin select raise(abort, 'retained reconciliation dependency requires one effect and retired forbids it'); end;
create trigger if not exists decomposition_reconciliation_dependencies_effect_update
before update of disposition,effect on decomposition_reconciliation_dependencies
for each row when (new.disposition='retained') != (new.effect is not null)
begin select raise(abort, 'retained reconciliation dependency requires one effect and retired forbids it'); end;

drop index if exists phase_epoch_current_design_key;
create unique index if not exists phase_epoch_current_design_key
on phase_epochs(project_id,work_unit_id,design_version_id,phase_key)
where design_version_id is not null and state in ('open','blocked');

drop index if exists phase_epoch_current_manual_key;
create unique index if not exists phase_epoch_current_manual_key
on phase_epochs(project_id,work_unit_id,phase_key)
where design_version_id is null and state in ('open','blocked');
"#;

pub(crate) const GENERATION_18_SQL: &str = r#"
create table if not exists decomposition_reconciliation_results (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    reconciliation_application_id integer not null unique
        references decomposition_reconciliation_applications(id),
    result_json text not null check(json_valid(result_json)),
    result_identity text not null check(length(result_identity)=64),
    created_at text not null
);

create trigger if not exists trg_decomposition_reconciliation_result_links_insert
before insert on decomposition_reconciliation_results
for each row when
    new.project_id != (
        select project_id from decomposition_reconciliation_applications
        where id=new.reconciliation_application_id
    )
begin select raise(abort, 'invalid decomposition reconciliation result links'); end;

create trigger if not exists trg_decomposition_reconciliation_result_immutable_update
before update on decomposition_reconciliation_results
begin select raise(abort, 'decomposition reconciliation results are immutable'); end;

create trigger if not exists trg_decomposition_reconciliation_result_immutable_delete
before delete on decomposition_reconciliation_results
begin select raise(abort, 'decomposition reconciliation results are immutable'); end;
"#;

pub(crate) const GENERATION_19_SQL: &str = r#"
drop index if exists decomposition_plan_current_package_work_unique;

create unique index if not exists decomposition_plan_applied_package_work_unique
on decomposition_plans(project_id,design_package_id,work_unit_id)
where status='applied' and work_unit_id is not null;

create unique index if not exists decomposition_plan_editable_package_work_unique
on decomposition_plans(project_id,design_package_id,work_unit_id)
where status in ('draft','incomplete','ready') and work_unit_id is not null;
"#;

pub(crate) const GENERATION_20_SQL: &str = r#"
create table if not exists kpt_item_sources (
    id integer primary key,
    kpt_item_id integer not null unique references kpt_items(id) on delete cascade,
    source_kind text not null check(source_kind in ('correction','finding','command-drift','review-outcome','work-outcome','legacy-command-profile')),
    source_identity text not null,
    source_revision text not null,
    created_at text not null
);

create table if not exists kpt_rules (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    kpt_item_id integer not null unique references kpt_items(id) on delete cascade,
    scope text not null,
    title text not null,
    body text not null,
    status text not null check(status in ('recorded','superseded')),
    created_at text not null
);

create table if not exists kpt_item_dismissals (
    id integer primary key,
    kpt_item_id integer not null unique references kpt_items(id) on delete cascade,
    item_revision text not null,
    source_kind text,
    source_identity text,
    source_revision text,
    review_revision text not null,
    review_status text not null check(review_status in ('open','closed')),
    authority_event_id integer not null references authority_events(id),
    reason text not null,
    predecessor_handle text not null,
    decision_handle text not null unique,
    current_handle text not null unique,
    replay_identity text not null unique,
    created_at text not null,
    check((source_kind is null and source_identity is null and source_revision is null)
       or (source_kind is not null and source_identity is not null and source_revision is not null))
);

create trigger if not exists trg_kpt_item_source_immutable_update
before update on kpt_item_sources begin select raise(abort,'KPT item source is immutable'); end;
create trigger if not exists trg_kpt_item_source_immutable_delete
before delete on kpt_item_sources begin select raise(abort,'KPT item source is immutable'); end;
create trigger if not exists trg_kpt_rule_restricted_update
before update on kpt_rules
when old.project_id!=new.project_id or old.kpt_item_id!=new.kpt_item_id
  or old.scope!=new.scope or old.title!=new.title or old.body!=new.body
  or old.status!='recorded' or new.status!='superseded'
begin select raise(abort,'KPT rule content is immutable'); end;
create trigger if not exists trg_kpt_rule_project_insert
before insert on kpt_rules
when new.project_id!=(
  select review.project_id from kpt_items item
  join kpt_reviews review on review.id=item.kpt_review_id
  where item.id=new.kpt_item_id
)
begin select raise(abort,'KPT rule must belong to its item project'); end;
create trigger if not exists trg_kpt_rule_immutable_delete
before delete on kpt_rules begin select raise(abort,'KPT rule history is immutable'); end;
create trigger if not exists trg_kpt_dismissal_immutable_update
before update on kpt_item_dismissals begin select raise(abort,'KPT dismissal is immutable'); end;
create trigger if not exists trg_kpt_dismissal_immutable_delete
before delete on kpt_item_dismissals begin select raise(abort,'KPT dismissal is immutable'); end;
create trigger if not exists trg_kpt_dismissal_links_insert
before insert on kpt_item_dismissals
when
  (select review.project_id from kpt_items item join kpt_reviews review on review.id=item.kpt_review_id where item.id=new.kpt_item_id)
    !=(select project_id from authority_events where id=new.authority_event_id)
  or (new.source_kind is null and exists(select 1 from kpt_item_sources where kpt_item_id=new.kpt_item_id))
  or (new.source_kind is not null and not exists(
    select 1 from kpt_item_sources source
    where source.kpt_item_id=new.kpt_item_id
      and source.source_kind=new.source_kind
      and source.source_identity=new.source_identity
      and source.source_revision=new.source_revision
  ))
begin select raise(abort,'KPT dismissal must bind its exact item, source, and authority project'); end;

create unique index if not exists kpt_item_single_conversion
on kpt_item_conversions(kpt_item_id);

create unique index if not exists kpt_item_conversion_request_identity
on kpt_item_conversions(request_identity) where request_identity is not null;

create unique index if not exists kpt_item_conversion_receipt_identity
on kpt_item_conversions(receipt_identity) where receipt_identity is not null;

create unique index if not exists kpt_item_conversion_current_handle
on kpt_item_conversions(current_handle) where current_handle is not null;

create trigger if not exists trg_kpt_item_conversion_receipt_insert
before insert on kpt_item_conversions
for each row when not (
  (new.item_revision is null and new.predecessor_handle is null and new.request_identity is null
   and new.receipt_identity is null and new.current_handle is null)
  or
  (new.item_revision is not null and new.predecessor_handle is not null and new.request_identity is not null
   and new.receipt_identity is not null and new.current_handle is not null)
)
begin select raise(abort,'KPT item conversion receipt must be complete or legacy-absent'); end;

create trigger if not exists trg_kpt_item_conversion_project_insert
before insert on kpt_item_conversions
for each row when
  (new.target_type='rule' and (select project_id from kpt_reviews where id=(select kpt_review_id from kpt_items where id=new.kpt_item_id))!=(select project_id from kpt_rules where id=new.kpt_rule_id))
  or (new.target_type in ('correction','user_correction') and (select project_id from kpt_reviews where id=(select kpt_review_id from kpt_items where id=new.kpt_item_id))!=(select project_id from user_corrections where id=new.user_correction_id))
  or (new.target_type='command_profile' and (select project_id from kpt_reviews where id=(select kpt_review_id from kpt_items where id=new.kpt_item_id))!=(select project_id from command_profiles where id=new.command_profile_id))
  or (new.target_type='review_policy' and (select project_id from kpt_reviews where id=(select kpt_review_id from kpt_items where id=new.kpt_item_id))!=(select project_id from review_policies where id=new.review_policy_id))
  or (new.target_type='design_version' and (select project_id from kpt_reviews where id=(select kpt_review_id from kpt_items where id=new.kpt_item_id))!=(select project_id from design_versions where id=new.design_version_id))
  or (new.target_type='decision' and (select project_id from kpt_reviews where id=(select kpt_review_id from kpt_items where id=new.kpt_item_id))!=(select project_id from decisions where id=new.decision_id))
  or (new.target_type='task' and (select project_id from kpt_reviews where id=(select kpt_review_id from kpt_items where id=new.kpt_item_id))!=coalesce((select project_id from work_units where id=(select work_unit_id from tasks where id=new.task_id)),(select id from projects order by id limit 1)))
begin select raise(abort,'KPT item conversion target must belong to its project'); end;

create trigger if not exists trg_kpt_item_conversion_project_update
before update on kpt_item_conversions
begin select raise(abort,'KPT item conversion is immutable'); end;

create trigger if not exists trg_kpt_item_conversion_immutable_delete
before delete on kpt_item_conversions
begin select raise(abort,'KPT item conversion is immutable'); end;
"#;

pub(crate) const GENERATION_21_SQL: &str = r#"
create table if not exists finding_design_recoveries (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    recovery_handle text not null,
    finding_id integer not null references findings(id),
    terminal_epoch integer not null,
    source_closure_id integer not null references closures(id),
    source_session_id integer not null references correction_sessions(id),
    source_attempt_id integer not null references closure_attempts(id),
    authority_event_id integer not null references authority_events(id),
    evidence text not null,
    reason text not null,
    package_current text not null,
    expected_current text not null,
    idempotency_key text not null,
    payload_digest text not null check(length(payload_digest)=64),
    postcondition_digest text not null check(length(postcondition_digest)=64),
    successor_design_version_id integer not null references design_versions(id),
    successor_alias text not null,
    successor_closure_id integer not null references closures(id),
    successor_session_id integer not null references correction_sessions(id),
    successor_attempt_id integer not null references closure_attempts(id),
    successor_epoch_decision_id integer not null unique references owner_decisions(id),
    created_at text not null,
    unique(project_id,recovery_handle),
    unique(project_id,idempotency_key),
    unique(project_id,finding_id,terminal_epoch),
    unique(project_id,successor_alias),
    unique(project_id,successor_design_version_id),
    unique(project_id,successor_closure_id),
    unique(project_id,successor_session_id),
    unique(project_id,successor_attempt_id)
);
create trigger if not exists trg_finding_design_recovery_immutable_update
before update on finding_design_recoveries
begin select raise(abort,'finding design recovery is append-only'); end;
create trigger if not exists trg_finding_design_recovery_immutable_delete
before delete on finding_design_recoveries
begin select raise(abort,'finding design recovery is append-only'); end;
create trigger if not exists trg_finding_design_recovery_project_insert
before insert on finding_design_recoveries
for each row when
    not exists(select 1 from findings f where f.id=new.finding_id and f.project_id=new.project_id)
    or not exists(select 1 from closures c where c.id=new.source_closure_id and c.project_id=new.project_id and c.finding_id=new.finding_id)
    or not exists(select 1 from correction_sessions s where s.id=new.source_session_id and s.project_id=new.project_id and s.finding_id=new.finding_id and s.closure_id=new.source_closure_id)
    or not exists(select 1 from closure_attempts a where a.id=new.source_attempt_id and a.project_id=new.project_id and a.closure_id=new.source_closure_id)
    or not exists(select 1 from authority_events a where a.id=new.authority_event_id and a.project_id=new.project_id)
    or not exists(select 1 from design_versions v where v.id=new.successor_design_version_id and v.project_id=new.project_id)
    or not exists(select 1 from closures c where c.id=new.successor_closure_id and c.project_id=new.project_id and c.finding_id=new.finding_id)
    or not exists(select 1 from correction_sessions s where s.id=new.successor_session_id and s.project_id=new.project_id and s.finding_id=new.finding_id and s.closure_id=new.successor_closure_id)
    or not exists(select 1 from closure_attempts a where a.id=new.successor_attempt_id and a.project_id=new.project_id and a.closure_id=new.successor_closure_id)
    or not exists(select 1 from owner_decisions d where d.id=new.successor_epoch_decision_id and d.project_id=new.project_id and d.decision_family='finding' and d.action='reopen' and d.decision_value='reopened')
    or not exists(select 1 from finding_decision_epochs e where e.project_id=new.project_id and e.finding_id=new.finding_id and e.epoch_number=new.terminal_epoch+1 and e.reopen_decision_id=new.successor_epoch_decision_id)
begin select raise(abort,'finding design recovery project mismatch'); end;
"#;

pub(crate) const GENERATION_21_FINDING_VERIFICATION_SQL: &str = r#"
create trigger trg_finding_verification_project_insert
before insert on finding_verifications
for each row
when new.project_id != (select project_id from review_runs where id = new.review_run_id)
  or new.project_id != (select project_id from findings where id = new.finding_id)
  or new.project_id != (select project_id from closures where id = new.closure_id)
  or new.finding_id != (select finding_id from closures where id = new.closure_id)
  or (select run_type from review_runs where id = new.review_run_id) != 'resume'
  or (select run_purpose from review_runs where id = new.review_run_id) != 'finding_fix_verification'
  or not exists (
      select 1 from findings f
      join review_runs source_run on source_run.id = f.review_run_id
      join review_plans source_plan on source_plan.id = source_run.review_plan_id
      join review_runs verifier_run on verifier_run.id = new.review_run_id
      join review_plans verifier_plan on verifier_plan.id = verifier_run.review_plan_id
      where f.id = new.finding_id
        and verifier_plan.work_unit_id = source_plan.work_unit_id
        and verifier_plan.review_type = source_plan.review_type
        and verifier_plan.stage = source_plan.stage
        and (
          exists(
            select 1 from finding_design_recoveries recovery
            where recovery.project_id=new.project_id
              and recovery.successor_closure_id=new.closure_id
              and recovery.successor_attempt_id=new.closure_attempt_id
              and recovery.successor_design_version_id=verifier_plan.design_version_id
          )
          or (
            not exists(
              select 1 from finding_design_recoveries recovery
              where recovery.project_id=new.project_id and recovery.successor_closure_id=new.closure_id
            )
            and (
              verifier_plan.design_version_id is source_plan.design_version_id
              or exists(
                select 1 from design_versions source_design
                join design_versions verifier_design on verifier_design.design_package_id=source_design.design_package_id
                where source_design.id=source_plan.design_version_id
                  and verifier_design.id=verifier_plan.design_version_id
                  and verifier_design.version_number>=source_design.version_number
                  and verifier_design.status='approved'
              )
            )
          )
        )
        and coalesce(verifier_plan.scope, '') = coalesce(source_plan.scope, '')
  )
begin select raise(abort, 'finding verification project_id must match referenced rows'); end;

create trigger trg_finding_verification_project_update
before update of project_id, review_run_id, finding_id, closure_id, closure_attempt_id on finding_verifications
for each row
when new.project_id != (select project_id from review_runs where id = new.review_run_id)
  or new.project_id != (select project_id from findings where id = new.finding_id)
  or new.project_id != (select project_id from closures where id = new.closure_id)
  or new.finding_id != (select finding_id from closures where id = new.closure_id)
  or (select run_type from review_runs where id = new.review_run_id) != 'resume'
  or (select run_purpose from review_runs where id = new.review_run_id) != 'finding_fix_verification'
  or not exists (
      select 1 from findings f
      join review_runs source_run on source_run.id = f.review_run_id
      join review_plans source_plan on source_plan.id = source_run.review_plan_id
      join review_runs verifier_run on verifier_run.id = new.review_run_id
      join review_plans verifier_plan on verifier_plan.id = verifier_run.review_plan_id
      where f.id = new.finding_id
        and verifier_plan.work_unit_id = source_plan.work_unit_id
        and verifier_plan.review_type = source_plan.review_type
        and verifier_plan.stage = source_plan.stage
        and (
          exists(
            select 1 from finding_design_recoveries recovery
            where recovery.project_id=new.project_id
              and recovery.successor_closure_id=new.closure_id
              and recovery.successor_attempt_id=new.closure_attempt_id
              and recovery.successor_design_version_id=verifier_plan.design_version_id
          )
          or (
            not exists(
              select 1 from finding_design_recoveries recovery
              where recovery.project_id=new.project_id and recovery.successor_closure_id=new.closure_id
            )
            and (
              verifier_plan.design_version_id is source_plan.design_version_id
              or exists(
                select 1 from design_versions source_design
                join design_versions verifier_design on verifier_design.design_package_id=source_design.design_package_id
                where source_design.id=source_plan.design_version_id
                  and verifier_design.id=verifier_plan.design_version_id
                  and verifier_design.version_number>=source_design.version_number
                  and verifier_design.status='approved'
              )
            )
          )
        )
        and coalesce(verifier_plan.scope, '') = coalesce(source_plan.scope, '')
  )
begin select raise(abort, 'finding verification project_id must match referenced rows'); end;
"#;

pub(crate) const GENERATION_22_SQL: &str = r#"
create table if not exists decomposition_plan_ingress_identities(
    plan_id integer primary key references decomposition_plans(id) on delete cascade,
    project_id integer not null references projects(id) on delete cascade,
    source_identity text not null check(length(source_identity)=64),
    content_identity text not null check(length(content_identity)=64),
    created_at text not null
);
create unique index if not exists idx_decomposition_plan_ingress_project_id
on decomposition_plan_ingress_identities(project_id,plan_id);

create trigger if not exists trg_decomposition_plan_ingress_links_insert
before insert on decomposition_plan_ingress_identities
for each row when
    new.project_id != (select project_id from decomposition_plans where id=new.plan_id)
    or new.content_identity != (
        select content_identity from decomposition_plans where id=new.plan_id
    )
begin select raise(abort, 'invalid Decomposition Plan ingress identity links'); end;

create trigger if not exists trg_decomposition_plan_ingress_immutable_update
before update on decomposition_plan_ingress_identities
begin select raise(abort, 'Decomposition Plan ingress identities are immutable'); end;

create trigger if not exists trg_decomposition_plan_ingress_immutable_delete
before delete on decomposition_plan_ingress_identities
begin select raise(abort, 'Decomposition Plan ingress identities are immutable'); end;

drop trigger if exists trg_decomposition_reconciliation_application_links_insert;
create trigger trg_decomposition_reconciliation_application_links_insert
before insert on decomposition_reconciliation_applications
for each row when
    new.project_id != (select project_id from correction_transition_applications where id=new.correction_application_id)
    or new.project_id != (select project_id from correction_tokens where id=new.correction_token_id)
    or new.correction_token_id != (
        select correction_token_id from correction_transition_applications
        where id=new.correction_application_id
    )
    or 'decomposition-plan-reconcile' != (
        select operation from correction_tokens where id=new.correction_token_id
    )
    or new.project_id != (select project_id from decomposition_plans where id=new.predecessor_plan_id)
    or new.project_id != (select project_id from decomposition_plans where id=new.successor_plan_id)
    or 'superseded' != (select status from decomposition_plans where id=new.predecessor_plan_id)
    or 'applied' != (select status from decomposition_plans where id=new.successor_plan_id)
    or new.predecessor_plan_id != (
        select predecessor_id from decomposition_plans where id=new.successor_plan_id
    )
    or new.source_identity != (
        select source_identity from decomposition_plans where id=new.successor_plan_id
    )
    or 'decomposition-plan:'||new.successor_plan_id != (
        select result_ref from correction_transition_applications
        where id=new.correction_application_id
    )
    or substr(
        (select target from correction_tokens where id=new.correction_token_id),
        1,
        length((select cast(design_version_id as text)||'/'||cast(work_unit_id as text)||'/' from decomposition_plans where id=new.successor_plan_id))
    ) != (
        select cast(design_version_id as text)||'/'||cast(work_unit_id as text)||'/'
        from decomposition_plans where id=new.successor_plan_id
    )
begin select raise(abort, 'invalid decomposition reconciliation application links'); end;
"#;

pub(crate) const GENERATION_23_SQL: &str = r#"
create table if not exists decision_continuations (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    continuation_handle text not null,
    command_kind text not null,
    owner_ref text not null,
    target_ref text not null,
    decision_family text not null check(decision_family in ('review','finding','verification')),
    action text not null,
    expected_current text not null,
    context_identity text not null check(length(context_identity)=64),
    design_context text,
    rejection_code text not null,
    required_inputs text not null check(required_inputs='decision,reason'),
    status text not null check(status in ('pending','applied','superseded')),
    owner_decision_id integer references owner_decisions(id),
    applied_payload_digest text check(applied_payload_digest is null or length(applied_payload_digest)=64),
    successor_id integer references decision_continuations(id),
    created_at text not null,
    applied_at text,
    superseded_at text,
    unique(project_id,continuation_handle),
    check(successor_id is null or successor_id!=id)
);

create trigger if not exists trg_decision_continuation_insert
before insert on decision_continuations
for each row when
    (new.status='pending' and (new.owner_decision_id is not null or new.applied_payload_digest is not null or new.successor_id is not null or new.applied_at is not null or new.superseded_at is not null))
    or (new.status='applied' and (new.owner_decision_id is null or new.successor_id is not null or new.applied_at is null or new.superseded_at is not null))
    or (new.status='superseded' and (new.owner_decision_id is not null or new.applied_payload_digest is not null or new.successor_id is null or new.applied_at is not null or new.superseded_at is null))
    or (new.owner_decision_id is not null and not exists(select 1 from owner_decisions d where d.id=new.owner_decision_id and d.project_id=new.project_id))
    or (new.successor_id is not null and not exists(select 1 from decision_continuations c where c.id=new.successor_id and c.project_id=new.project_id))
begin select raise(abort,'invalid decision continuation state'); end;

create trigger if not exists trg_decision_continuation_update
before update on decision_continuations
for each row when
    old.status!='pending'
    or new.project_id!=old.project_id or new.continuation_handle!=old.continuation_handle
    or new.command_kind!=old.command_kind or new.owner_ref!=old.owner_ref
    or new.target_ref!=old.target_ref or new.decision_family!=old.decision_family
    or new.action!=old.action or new.expected_current!=old.expected_current
    or new.context_identity!=old.context_identity or new.design_context is not old.design_context
    or new.rejection_code!=old.rejection_code
    or new.required_inputs!=old.required_inputs or new.created_at!=old.created_at
    or not (
      (new.status='applied' and new.owner_decision_id is not null
       and new.applied_payload_digest is not null and length(new.applied_payload_digest)=64
       and new.successor_id is null and new.applied_at is not null and new.superseded_at is null)
      or
      (new.status='superseded' and new.owner_decision_id is null
       and new.applied_payload_digest is null and new.successor_id is not null
       and new.applied_at is null and new.superseded_at is not null)
    )
    or (new.owner_decision_id is not null and not exists(select 1 from owner_decisions d where d.id=new.owner_decision_id and d.project_id=new.project_id))
    or (new.successor_id is not null and not exists(select 1 from decision_continuations c where c.id=new.successor_id and c.project_id=new.project_id))
begin select raise(abort,'decision continuation transition is invalid'); end;

create trigger if not exists trg_decision_continuation_delete
before delete on decision_continuations
begin select raise(abort,'decision continuations are append-only'); end;

create table if not exists reviewer_migration_sources (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    source_reviewer_ref text not null,
    source_reviewer_digest text not null check(length(source_reviewer_digest)=64),
    source_ledger_digest text check(source_ledger_digest is null or length(source_ledger_digest)=64),
    source_generation integer,
    status text not null check(status in ('pending','bound','retired')),
    binding_id integer,
    created_at text not null,
    retired_at text,
    unique(project_id,source_reviewer_ref),
    unique(project_id,source_reviewer_digest)
);

create table if not exists reviewer_migration_bindings (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    source_id integer not null unique references reviewer_migration_sources(id),
    binding_handle text not null,
    agent_label text not null,
    external_agent_id text not null,
    provenance_ref text not null,
    authority_event_id integer not null references authority_events(id),
    idempotency_key text not null,
    payload_digest text not null check(length(payload_digest)=64),
    legacy_binding_handle text,
    created_at text not null,
    unique(project_id,binding_handle),
    unique(project_id,idempotency_key)
);

create trigger if not exists trg_reviewer_migration_source_insert
before insert on reviewer_migration_sources
for each row when
    (new.status='pending' and (new.binding_id is not null or new.retired_at is not null))
    or (new.status='bound' and (new.binding_id is null or new.retired_at is not null))
    or (new.status='retired' and new.retired_at is null)
begin select raise(abort,'invalid reviewer migration source state'); end;

create trigger if not exists trg_reviewer_migration_source_update
before update on reviewer_migration_sources
for each row when
    old.status!='pending'
    or new.project_id!=old.project_id or new.source_reviewer_ref!=old.source_reviewer_ref
    or new.source_reviewer_digest!=old.source_reviewer_digest
    or new.source_ledger_digest is not old.source_ledger_digest
    or new.source_generation is not old.source_generation or new.created_at!=old.created_at
    or not (
      (new.status='bound' and new.binding_id is not null and new.retired_at is null)
      or (new.status='retired' and new.binding_id is null and new.retired_at is not null)
    )
begin select raise(abort,'invalid reviewer migration source transition'); end;

create trigger if not exists trg_reviewer_migration_source_delete
before delete on reviewer_migration_sources
begin select raise(abort,'reviewer migration sources are retained'); end;

create trigger if not exists trg_reviewer_migration_binding_insert
before insert on reviewer_migration_bindings
for each row when
    not exists(select 1 from reviewer_migration_sources s where s.id=new.source_id and s.project_id=new.project_id and s.status='pending')
    or not exists(select 1 from authority_events a where a.id=new.authority_event_id and a.project_id=new.project_id and a.status='active')
begin select raise(abort,'invalid reviewer migration binding'); end;

create trigger if not exists trg_reviewer_migration_binding_update
before update on reviewer_migration_bindings
begin select raise(abort,'reviewer migration bindings are append-only'); end;
create trigger if not exists trg_reviewer_migration_binding_delete
before delete on reviewer_migration_bindings
begin select raise(abort,'reviewer migration bindings are append-only'); end;

create table if not exists validation_link_repair_runs (
    id integer primary key,
    backup_path text not null unique,
    repaired_validation_run_count integer not null,
    change_count integer not null,
    created_at text not null
);
create table if not exists validation_link_repair_changes (
    id integer primary key,
    repair_run_id integer not null references validation_link_repair_runs(id),
    validation_run_id integer not null,
    entity_type text not null,
    entity_id integer not null,
    field_name text not null,
    before_value text,
    after_value text,
    created_at text not null
);
create trigger if not exists trg_validation_link_repair_runs_immutable_update
before update on validation_link_repair_runs begin select raise(abort,'validation link repair audit is immutable'); end;
create trigger if not exists trg_validation_link_repair_runs_immutable_delete
before delete on validation_link_repair_runs begin select raise(abort,'validation link repair audit is immutable'); end;
create trigger if not exists trg_validation_link_repair_changes_immutable_update
before update on validation_link_repair_changes begin select raise(abort,'validation link repair audit is immutable'); end;
create trigger if not exists trg_validation_link_repair_changes_immutable_delete
before delete on validation_link_repair_changes begin select raise(abort,'validation link repair audit is immutable'); end;

create table if not exists validation_link_retirements (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    validation_run_id integer not null unique references validation_runs(id),
    artifact_ref text not null,
    reason text not null,
    expected_current text not null,
    request_digest text not null check(length(request_digest)=64),
    created_at text not null,
    unique(project_id,artifact_ref),
    unique(project_id,request_digest)
);
create trigger if not exists trg_validation_link_retirement_update
before update on validation_link_retirements
begin select raise(abort,'validation link retirements are append-only'); end;
create trigger if not exists trg_validation_link_retirement_delete
before delete on validation_link_retirements
begin select raise(abort,'validation link retirements are append-only'); end;

create table if not exists validation_link_repair_receipts (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    artifact_ref text not null,
    validation_run_id integer not null references validation_runs(id),
    operation text not null check(operation in ('relink','retire')),
    expected_current text not null,
    result_current text not null,
    repair_run_id integer references validation_link_repair_runs(id),
    retirement_id integer references validation_link_retirements(id),
    request_digest text not null check(length(request_digest)=64),
    created_at text not null,
    unique(project_id,request_digest),
    check((operation='relink' and repair_run_id is not null and retirement_id is null)
       or (operation='retire' and repair_run_id is null and retirement_id is not null))
);
create trigger if not exists trg_validation_link_repair_receipt_update
before update on validation_link_repair_receipts
begin select raise(abort,'validation link repair receipts are append-only'); end;
create trigger if not exists trg_validation_link_repair_receipt_delete
before delete on validation_link_repair_receipts
begin select raise(abort,'validation link repair receipts are append-only'); end;
"#;

pub(crate) const GENERATION_24_SQL: &str = r#"
create table if not exists release_candidate_boundaries (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    release_candidate_id integer not null unique references release_candidates(id) on delete cascade,
    work_unit_id integer not null references work_units(id),
    activation_id integer references work_unit_activations(id),
    design_version_id integer references design_versions(id),
    repository_snapshot_id integer not null references repository_snapshots(id),
    reviewed_commit text not null,
    boundary_identity text not null check(length(boundary_identity)=64),
    created_at text not null
);

create trigger if not exists trg_release_candidate_boundary_insert
before insert on release_candidate_boundaries
for each row when
    not exists(
      select 1 from release_candidates candidate
      where candidate.id=new.release_candidate_id
        and candidate.project_id=new.project_id
        and candidate.reviewed_commit=new.reviewed_commit
    )
    or not exists(
      select 1 from work_units work
      where work.id=new.work_unit_id and work.project_id=new.project_id
    )
    or (new.activation_id is not null and not exists(
      select 1 from work_unit_activations activation
      where activation.id=new.activation_id
        and activation.project_id=new.project_id
        and activation.work_unit_id=new.work_unit_id
    ))
    or (new.design_version_id is not null and not exists(
      select 1 from design_versions version
      join design_packages package on package.id=version.design_package_id
      where version.id=new.design_version_id and package.project_id=new.project_id
    ))
    or not exists(
      select 1 from repository_snapshots snapshot
      join repositories repository on repository.id=snapshot.repository_id
      where snapshot.id=new.repository_snapshot_id
        and repository.project_id=new.project_id
        and snapshot.head_sha=new.reviewed_commit
        and (new.activation_id is null or snapshot.work_unit_activation_id=new.activation_id)
    )
begin select raise(abort,'invalid release candidate work boundary'); end;

create trigger if not exists trg_release_candidate_boundary_update
before update on release_candidate_boundaries
begin select raise(abort,'release candidate work boundaries are immutable'); end;

create trigger if not exists trg_release_candidate_boundary_delete
before delete on release_candidate_boundaries
begin select raise(abort,'release candidate work boundaries are immutable'); end;
"#;
