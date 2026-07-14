pub(super) const SQL: &str = r#"
create table if not exists design_packages (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    design_key text not null,
    package_id text not null,
    title text not null,
    root_path text not null,
    format text not null,
    version integer not null,
    package_hash text,
    status text not null default 'draft' check (status in ('draft', 'reviewed', 'approved', 'superseded')),
    current_design_version_id integer,
    created_at text not null,
    updated_at text not null,
    unique(project_id, design_key),
    unique(project_id, package_id)
);

create table if not exists design_versions (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    design_package_id integer not null references design_packages(id) on delete cascade,
    version_number integer not null,
    source_ref text not null,
    package_hash text not null,
    content_hash text not null,
    package_path text not null,
    manifest_path text not null,
    format text not null,
    manifest_version integer not null,
    status text not null default 'draft' check (status in ('draft', 'reviewed', 'approved', 'superseded')),
    imported_at text not null,
    approved_by_authority_event_id integer references authority_events(id),
    approved_at text,
    unique(design_package_id, version_number),
    unique(design_package_id, content_hash)
);

create table if not exists design_files (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    design_package_id integer not null references design_packages(id) on delete cascade,
    design_version_id integer not null references design_versions(id) on delete cascade,
    section_key text not null,
    relative_path text not null,
    content_hash text not null,
    line_count integer not null,
    unique(design_version_id, relative_path)
);

create table if not exists design_requirements (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    design_version_id integer not null references design_versions(id) on delete cascade,
    source_design_file_id integer not null references design_files(id) on delete cascade,
    source_section text not null,
    requirement_key text not null,
    revision integer not null default 1,
    requirement_hash text not null,
    supersedes_requirement_id integer references design_requirements(id),
    requirement_text text not null,
    priority text not null check (priority in ('critical', 'high', 'medium', 'low')),
    required_surfaces text,
    validation_expectation text,
    status text not null check (status in ('active', 'superseded', 'accepted_out_of_scope')),
    created_at text not null,
    unique(design_version_id, requirement_key)
);

create table if not exists design_decisions (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    design_version_id integer not null references design_versions(id) on delete cascade,
    source_design_file_id integer not null references design_files(id) on delete cascade,
    source_section text not null,
    decision_key text not null,
    decision_hash text not null,
    topic text not null,
    decision_text text not null,
    rationale text,
    supersedes_decision_keys text,
    status text not null check (status in ('accepted', 'rejected', 'superseded')),
    created_at text not null,
    unique(design_version_id, decision_key)
);

create table if not exists validation_gate_templates (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    design_version_id integer not null references design_versions(id) on delete cascade,
    source_design_file_id integer not null references design_files(id) on delete cascade,
    source_section text not null,
    gate_key text not null,
    gate_hash text not null,
    stage text not null check (stage in ('design-ready', 'implementation-ready', 'close-ready', 'resume-ready')),
    command text,
    expected_result text not null,
    requirement_keys text,
    gate_text text not null,
    status text not null check (status in ('active', 'superseded', 'accepted_out_of_scope')),
    created_at text not null,
    unique(design_version_id, gate_key)
);

create table if not exists validation_gate_template_requirements (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    validation_gate_template_id integer not null references validation_gate_templates(id) on delete cascade,
    design_requirement_id integer not null references design_requirements(id) on delete cascade,
    unique(validation_gate_template_id, design_requirement_id)
);

create table if not exists checklists (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    work_unit_id integer not null references work_units(id) on delete cascade,
    design_version_id integer not null references design_versions(id) on delete cascade,
    title text not null,
    status text not null default 'active' check (status in ('active', 'stale', 'closed')),
    created_by_review_run_id integer,
    created_at text not null
);

create table if not exists checklist_items (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    checklist_id integer not null references checklists(id) on delete cascade,
    design_requirement_id integer not null references design_requirements(id) on delete cascade,
    task_id integer not null references tasks(id) on delete cascade,
    item_order integer not null,
    title text not null,
    completion_condition text,
    status text not null default 'open' check (status in ('open', 'blocked', 'closed', 'accepted_out_of_scope')),
    unique(checklist_id, item_order)
);

create table if not exists task_derivations (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    design_requirement_id integer not null references design_requirements(id) on delete cascade,
    task_id integer not null references tasks(id) on delete cascade,
    checklist_item_id integer references checklist_items(id) on delete set null,
    derivation_reason text,
    generated_by_review_run_id integer,
    status text not null default 'active' check (status in ('active', 'stale', 'closed')),
    created_at text not null,
    unique(design_requirement_id, task_id)
);

create table if not exists validation_gates (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    gate_key text not null,
    template_id integer references validation_gate_templates(id),
    work_unit_id integer references work_units(id) on delete cascade,
    task_id integer references tasks(id) on delete cascade,
    design_requirement_id integer references design_requirements(id) on delete cascade,
    command_profile_id integer references command_profiles(id),
    command text,
    expected_result text not null,
    environment text,
    timeout text,
    artifact_requirements text,
    selected_before_edit integer not null default 1 check (selected_before_edit in (0, 1)),
    status text not null default 'active' check (status in ('active', 'stale', 'closed')),
    created_at text not null
);

create view if not exists current_task_validation_gates as
select vg.*
from validation_gates vg
left join tasks t on t.id=vg.task_id
where (vg.task_id is null and vg.status='active')
   or (t.status in ('open','blocked') and vg.status='active')
   or (t.status in ('closed','accepted_out_of_scope') and vg.status='closed');

create table if not exists validation_runs (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    validation_gate_id integer not null references validation_gates(id) on delete cascade,
    work_unit_id integer references work_units(id) on delete cascade,
    task_id integer references tasks(id) on delete cascade,
    command_usage_id integer references command_usages(id),
    repository_snapshot_id integer,
    result text not null check (result in (
        'pass', 'fail', 'timeout', 'cancelled', 'unknown',
        'expected_red', 'oom', 'non_strict_observation', 'evidence_gap'
    )),
    command text,
    classification text check (classification in (
        'none', 'classified_failure', 'evidence_gap', 'accepted_exception'
    )),
    acceptance_record_id integer references acceptance_records(id),
    artifact_path text,
    artifact_hash text,
    notes text,
    created_at text not null
);

create table if not exists artifacts (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    artifact_type text not null check (artifact_type in ('validation_output', 'test_report', 'build_output', 'generated_file', 'other')),
    identity_key text not null,
    artifact_path text,
    artifact_hash text,
    validation_run_id integer references validation_runs(id) on delete cascade,
    command_usage_id integer references command_usages(id),
    repository_snapshot_id integer,
    created_at text not null,
    check (artifact_path is not null or artifact_hash is not null)
);

create trigger if not exists trg_validation_run_project_insert
before insert on validation_runs
for each row
when new.project_id != (select project_id from validation_gates where id = new.validation_gate_id)
  or new.work_unit_id is not (select work_unit_id from validation_gates where id = new.validation_gate_id)
  or new.task_id is not (select task_id from validation_gates where id = new.validation_gate_id)
  or (new.work_unit_id is not null and new.project_id != (select project_id from work_units where id = new.work_unit_id))
  or (new.task_id is not null and (
      not exists (select 1 from tasks where id = new.task_id)
      or (select work_unit_id from tasks where id = new.task_id) is null
      or new.project_id != (
          select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)
      )
  ))
  or (new.command_usage_id is not null and (
      not exists (select 1 from command_usages where id = new.command_usage_id)
      or new.project_id != (select project_id from command_usages where id = new.command_usage_id)
      or (
          (select work_unit_id from command_usages where id = new.command_usage_id) is not null
          and (select work_unit_id from command_usages where id = new.command_usage_id) is not new.work_unit_id
      )
  ))
  or (new.repository_snapshot_id is not null and (
      not exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
      or new.project_id != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
  ))
  or (
      new.command_usage_id is not null
      and new.repository_snapshot_id is not null
      and (select repository_snapshot_id from command_usages where id = new.command_usage_id) is not null
      and new.repository_snapshot_id != (select repository_snapshot_id from command_usages where id = new.command_usage_id)
  )
begin
    select raise(abort, 'validation run project_id must match referenced rows');
end;

create trigger if not exists trg_validation_run_project_update
before update of project_id, validation_gate_id, work_unit_id, task_id, command_usage_id, repository_snapshot_id on validation_runs
for each row
when new.project_id != (select project_id from validation_gates where id = new.validation_gate_id)
  or new.work_unit_id is not (select work_unit_id from validation_gates where id = new.validation_gate_id)
  or new.task_id is not (select task_id from validation_gates where id = new.validation_gate_id)
  or (new.work_unit_id is not null and new.project_id != (select project_id from work_units where id = new.work_unit_id))
  or (new.task_id is not null and (
      not exists (select 1 from tasks where id = new.task_id)
      or (select work_unit_id from tasks where id = new.task_id) is null
      or new.project_id != (
          select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)
      )
  ))
  or (new.command_usage_id is not null and (
      not exists (select 1 from command_usages where id = new.command_usage_id)
      or new.project_id != (select project_id from command_usages where id = new.command_usage_id)
      or (
          (select work_unit_id from command_usages where id = new.command_usage_id) is not null
          and (select work_unit_id from command_usages where id = new.command_usage_id) is not new.work_unit_id
      )
  ))
  or (new.repository_snapshot_id is not null and (
      not exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
      or new.project_id != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
  ))
  or (
      new.command_usage_id is not null
      and new.repository_snapshot_id is not null
      and (select repository_snapshot_id from command_usages where id = new.command_usage_id) is not null
      and new.repository_snapshot_id != (select repository_snapshot_id from command_usages where id = new.command_usage_id)
  )
begin
    select raise(abort, 'validation run project_id must match referenced rows');
end;

create trigger if not exists trg_artifact_project_insert
before insert on artifacts
for each row
when (new.validation_run_id is not null and (
      not exists (select 1 from validation_runs where id = new.validation_run_id)
      or new.project_id != (select project_id from validation_runs where id = new.validation_run_id)
  ))
  or (new.command_usage_id is not null and (
      not exists (select 1 from command_usages where id = new.command_usage_id)
      or new.project_id != (select project_id from command_usages where id = new.command_usage_id)
  ))
  or (new.repository_snapshot_id is not null and (
      not exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
      or new.project_id != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
  ))
  or (
      new.validation_run_id is not null
      and new.command_usage_id is not (
          select command_usage_id from validation_runs where id = new.validation_run_id
      )
  )
  or (
      new.validation_run_id is not null
      and new.repository_snapshot_id is not (
          select repository_snapshot_id from validation_runs where id = new.validation_run_id
      )
  )
begin
    select raise(abort, 'artifact project_id must match referenced rows');
end;

create trigger if not exists trg_artifact_project_update
before update of project_id, validation_run_id, command_usage_id, repository_snapshot_id on artifacts
for each row
when (new.validation_run_id is not null and (
      not exists (select 1 from validation_runs where id = new.validation_run_id)
      or new.project_id != (select project_id from validation_runs where id = new.validation_run_id)
  ))
  or (new.command_usage_id is not null and (
      not exists (select 1 from command_usages where id = new.command_usage_id)
      or new.project_id != (select project_id from command_usages where id = new.command_usage_id)
  ))
  or (new.repository_snapshot_id is not null and (
      not exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
      or new.project_id != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
  ))
  or (
      new.validation_run_id is not null
      and new.command_usage_id is not (
          select command_usage_id from validation_runs where id = new.validation_run_id
      )
  )
  or (
      new.validation_run_id is not null
      and new.repository_snapshot_id is not (
          select repository_snapshot_id from validation_runs where id = new.validation_run_id
      )
  )
begin
    select raise(abort, 'artifact project_id must match referenced rows');
end;

create table if not exists implementation_evidence (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    task_id integer references tasks(id) on delete cascade,
    design_requirement_id integer references design_requirements(id) on delete cascade,
    evidence_type text not null check (evidence_type in ('commit', 'file', 'symbol', 'test', 'artifact', 'command_output', 'manual_note')),
    repository_id integer,
    git_commit_id integer,
    git_file_change_id integer,
    commit_sha text,
    file_path text,
    line_ref text,
    symbol text,
    artifact_path text,
    note text,
    created_at text not null,
    check (task_id is not null or design_requirement_id is not null)
);

create trigger if not exists trg_implementation_evidence_git_insert
before insert on implementation_evidence
for each row
when (new.repository_id is not null and (
      not exists (select 1 from repositories where id = new.repository_id)
      or new.project_id != (select project_id from repositories where id = new.repository_id)
  ))
  or (new.git_commit_id is not null and (
      not exists (select 1 from git_commits where id = new.git_commit_id)
      or new.project_id != (
          select r.project_id
          from git_commits c
          join repositories r on r.id = c.repository_id
          where c.id = new.git_commit_id
      )
      or (new.repository_id is not null and new.repository_id != (
          select repository_id from git_commits where id = new.git_commit_id
      ))
      or (new.commit_sha is not null and new.commit_sha != (
          select commit_sha from git_commits where id = new.git_commit_id
      ))
  ))
  or (new.git_file_change_id is not null and (
      not exists (select 1 from git_file_changes where id = new.git_file_change_id)
      or new.project_id != (
          select r.project_id
          from git_file_changes f
          join repositories r on r.id = f.repository_id
          where f.id = new.git_file_change_id
      )
      or (new.repository_id is not null and new.repository_id != (
          select repository_id from git_file_changes where id = new.git_file_change_id
      ))
      or (new.git_commit_id is not null and new.git_commit_id != (
          select git_commit_id from git_file_changes where id = new.git_file_change_id
      ))
      or (new.file_path is not null and new.file_path != (
          select path from git_file_changes where id = new.git_file_change_id
      ))
  ))
begin
    select raise(abort, 'implementation evidence git links must match project and paths');
end;

create trigger if not exists trg_implementation_evidence_git_update
before update of project_id, repository_id, git_commit_id, git_file_change_id, commit_sha, file_path on implementation_evidence
for each row
when (new.repository_id is not null and (
      not exists (select 1 from repositories where id = new.repository_id)
      or new.project_id != (select project_id from repositories where id = new.repository_id)
  ))
  or (new.git_commit_id is not null and (
      not exists (select 1 from git_commits where id = new.git_commit_id)
      or new.project_id != (
          select r.project_id
          from git_commits c
          join repositories r on r.id = c.repository_id
          where c.id = new.git_commit_id
      )
      or (new.repository_id is not null and new.repository_id != (
          select repository_id from git_commits where id = new.git_commit_id
      ))
      or (new.commit_sha is not null and new.commit_sha != (
          select commit_sha from git_commits where id = new.git_commit_id
      ))
  ))
  or (new.git_file_change_id is not null and (
      not exists (select 1 from git_file_changes where id = new.git_file_change_id)
      or new.project_id != (
          select r.project_id
          from git_file_changes f
          join repositories r on r.id = f.repository_id
          where f.id = new.git_file_change_id
      )
      or (new.repository_id is not null and new.repository_id != (
          select repository_id from git_file_changes where id = new.git_file_change_id
      ))
      or (new.git_commit_id is not null and new.git_commit_id != (
          select git_commit_id from git_file_changes where id = new.git_file_change_id
      ))
      or (new.file_path is not null and new.file_path != (
          select path from git_file_changes where id = new.git_file_change_id
      ))
  ))
begin
    select raise(abort, 'implementation evidence git links must match project and paths');
end;

create table if not exists coverage_items (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    review_scope_id integer,
    work_unit_id integer references work_units(id) on delete cascade,
    design_requirement_id integer not null references design_requirements(id) on delete cascade,
    task_id integer references tasks(id) on delete cascade,
    requirement text not null,
    runtime_boundary_evidence text,
    ux_boundary_evidence text,
    lifecycle_boundary_evidence text,
    tests_or_gates text,
    missing_or_unverified text,
    status text not null check (status in ('covered', 'partial', 'missing_required_surface', 'design_conflict', 'accepted_out_of_scope', 'needs_evidence', 'stale')),
    created_at text not null
);

create trigger if not exists trg_gate_template_requirement_project_insert
before insert on validation_gate_template_requirements
for each row
when new.project_id != (select project_id from validation_gate_templates where id = new.validation_gate_template_id)
  or new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
begin
    select raise(abort, 'validation gate template requirement project_id must match referenced rows');
end;

create trigger if not exists trg_gate_template_requirement_project_update
before update of project_id, validation_gate_template_id, design_requirement_id on validation_gate_template_requirements
for each row
when new.project_id != (select project_id from validation_gate_templates where id = new.validation_gate_template_id)
  or new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
begin
    select raise(abort, 'validation gate template requirement project_id must match referenced rows');
end;

create trigger if not exists trg_checklist_project_insert
before insert on checklists
for each row
when new.project_id != (select project_id from work_units where id = new.work_unit_id)
  or new.project_id != (select project_id from design_versions where id = new.design_version_id)
begin
    select raise(abort, 'checklist project_id must match referenced rows');
end;

create trigger if not exists trg_checklist_project_update
before update of project_id, work_unit_id, design_version_id on checklists
for each row
when new.project_id != (select project_id from work_units where id = new.work_unit_id)
  or new.project_id != (select project_id from design_versions where id = new.design_version_id)
begin
    select raise(abort, 'checklist project_id must match referenced rows');
end;

create trigger if not exists trg_checklist_item_project_insert
before insert on checklist_items
for each row
when new.project_id != (select project_id from checklists where id = new.checklist_id)
  or new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
  or new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  )
begin
    select raise(abort, 'checklist item project_id must match referenced rows');
end;

create trigger if not exists trg_checklist_item_project_update
before update of project_id, checklist_id, design_requirement_id, task_id on checklist_items
for each row
when new.project_id != (select project_id from checklists where id = new.checklist_id)
  or new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
  or new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  )
begin
    select raise(abort, 'checklist item project_id must match referenced rows');
end;

create trigger if not exists trg_task_derivation_project_insert
before insert on task_derivations
for each row
when new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
  or new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  )
  or (new.checklist_item_id is not null and new.project_id != (select project_id from checklist_items where id = new.checklist_item_id))
begin
    select raise(abort, 'task derivation project_id must match referenced rows');
end;

create trigger if not exists trg_task_derivation_project_update
before update of project_id, design_requirement_id, task_id, checklist_item_id on task_derivations
for each row
when new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
  or new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  )
  or (new.checklist_item_id is not null and new.project_id != (select project_id from checklist_items where id = new.checklist_item_id))
begin
    select raise(abort, 'task derivation project_id must match referenced rows');
end;

create trigger if not exists trg_validation_gate_project_insert
before insert on validation_gates
for each row
when (new.template_id is not null and new.project_id != (select project_id from validation_gate_templates where id = new.template_id))
  or (new.work_unit_id is not null and new.project_id != (select project_id from work_units where id = new.work_unit_id))
  or (new.task_id is not null and new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
  or (new.design_requirement_id is not null and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id))
begin
    select raise(abort, 'validation gate project_id must match referenced rows');
end;

create trigger if not exists trg_validation_gate_project_update
before update of project_id, template_id, work_unit_id, task_id, design_requirement_id on validation_gates
for each row
when (new.template_id is not null and new.project_id != (select project_id from validation_gate_templates where id = new.template_id))
  or (new.work_unit_id is not null and new.project_id != (select project_id from work_units where id = new.work_unit_id))
  or (new.task_id is not null and new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
  or (new.design_requirement_id is not null and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id))
begin
    select raise(abort, 'validation gate project_id must match referenced rows');
end;

create trigger if not exists trg_implementation_evidence_project_insert
before insert on implementation_evidence
for each row
when (new.task_id is not null and (
      not exists (select 1 from tasks where id = new.task_id)
      or (select work_unit_id from tasks where id = new.task_id) is null
      or new.project_id != (
          select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)
      )
  ))
  or (new.design_requirement_id is not null and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id))
begin
    select raise(abort, 'implementation evidence project_id must match referenced rows');
end;

create trigger if not exists trg_implementation_evidence_project_update
before update of project_id, task_id, design_requirement_id on implementation_evidence
for each row
when (new.task_id is not null and (
      not exists (select 1 from tasks where id = new.task_id)
      or (select work_unit_id from tasks where id = new.task_id) is null
      or new.project_id != (
          select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)
      )
  ))
  or (new.design_requirement_id is not null and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id))
begin
    select raise(abort, 'implementation evidence project_id must match referenced rows');
end;

create trigger if not exists trg_coverage_item_project_insert
before insert on coverage_items
for each row
when (new.work_unit_id is not null and new.project_id != (select project_id from work_units where id = new.work_unit_id))
  or new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
  or (new.task_id is not null and new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
begin
    select raise(abort, 'coverage item project_id must match referenced rows');
end;

create trigger if not exists trg_coverage_item_project_update
before update of project_id, work_unit_id, design_requirement_id, task_id on coverage_items
for each row
when (new.work_unit_id is not null and new.project_id != (select project_id from work_units where id = new.work_unit_id))
  or new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
  or (new.task_id is not null and new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
begin
    select raise(abort, 'coverage item project_id must match referenced rows');
end;

create trigger if not exists trg_acceptance_design_requirement_project_insert
before insert on acceptance_records
for each row
when new.target_type = 'design_requirement'
 and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
begin
    select raise(abort, 'acceptance project_id must match design requirement project_id');
end;

create trigger if not exists trg_acceptance_design_requirement_project_update
before update of project_id, target_type, design_requirement_id on acceptance_records
for each row
when new.target_type = 'design_requirement'
 and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id)
begin
    select raise(abort, 'acceptance project_id must match design requirement project_id');
end;

create trigger if not exists trg_acceptance_task_project_insert
before insert on acceptance_records
for each row
when new.target_type = 'task'
 and new.project_id != coalesce(
     (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
     (select id from projects order by id limit 1)
 )
begin
    select raise(abort, 'acceptance project_id must match task project_id');
end;

create trigger if not exists trg_acceptance_task_project_update
before update of project_id, target_type, task_id on acceptance_records
for each row
when new.target_type = 'task'
 and new.project_id != coalesce(
     (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
     (select id from projects order by id limit 1)
 )
begin
    select raise(abort, 'acceptance project_id must match task project_id');
end;

create trigger if not exists trg_acceptance_validation_gate_template_project_insert
before insert on acceptance_records
for each row
when new.target_type = 'validation_gate_template'
 and new.project_id != (select project_id from validation_gate_templates where id = new.validation_gate_template_id)
begin
    select raise(abort, 'acceptance project_id must match validation gate template project_id');
end;

create trigger if not exists trg_acceptance_validation_gate_template_project_update
before update of project_id, target_type, validation_gate_template_id on acceptance_records
for each row
when new.target_type = 'validation_gate_template'
 and new.project_id != (select project_id from validation_gate_templates where id = new.validation_gate_template_id)
begin
    select raise(abort, 'acceptance project_id must match validation gate template project_id');
end;

create trigger if not exists trg_acceptance_coverage_item_project_insert
before insert on acceptance_records
for each row
when new.target_type = 'coverage_item'
 and new.project_id != (select project_id from coverage_items where id = new.coverage_item_id)
begin
    select raise(abort, 'acceptance project_id must match coverage item project_id');
end;

create trigger if not exists trg_acceptance_coverage_item_project_update
before update of project_id, target_type, coverage_item_id on acceptance_records
for each row
when new.target_type = 'coverage_item'
 and new.project_id != (select project_id from coverage_items where id = new.coverage_item_id)
begin
    select raise(abort, 'acceptance project_id must match coverage item project_id');
end;
"#;
