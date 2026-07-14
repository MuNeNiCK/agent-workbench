pub(super) const SQL: &str = r#"
create table if not exists work_units (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    parent_work_unit_id integer references work_units(id),
    title text not null,
    status text not null default 'open' check (status in ('open', 'blocked', 'closed', 'abandoned')),
    responsibility text,
    in_scope text,
    out_of_scope text,
    interrupt_reason text,
    selected_gate_id integer,
    review_plan_status text,
    started_at text not null,
    closed_at text,
    close_summary text
);

create table if not exists work_unit_activations (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    work_unit_id integer not null references work_units(id) on delete cascade,
    parent_activation_id integer references work_unit_activations(id),
    suspended_by_activation_id integer references work_unit_activations(id),
    stack_depth integer not null default 0,
    status text not null check (status in ('active', 'suspended', 'completed', 'abandoned')),
    activation_reason text not null check (activation_reason in ('start', 'interrupt', 'resume', 'reopen', 'follow_up')),
    suspend_snapshot_id integer,
    opened_at text not null,
    suspended_at text,
    completed_at text
);

create unique index if not exists ux_one_active_activation
on work_unit_activations(project_id)
where status = 'active';

create trigger if not exists trg_activation_project_matches_work_unit_insert
before insert on work_unit_activations
for each row
when new.project_id != (select project_id from work_units where id = new.work_unit_id)
begin
    select raise(abort, 'activation project_id must match work unit project_id');
end;

create trigger if not exists trg_activation_project_matches_work_unit_update
before update of project_id, work_unit_id on work_unit_activations
for each row
when new.project_id != (select project_id from work_units where id = new.work_unit_id)
begin
    select raise(abort, 'activation project_id must match work unit project_id');
end;

create table if not exists work_unit_events (
    id integer primary key,
    work_unit_id integer not null references work_units(id) on delete cascade,
    work_unit_activation_id integer references work_unit_activations(id),
    related_activation_id integer references work_unit_activations(id),
    event_type text not null check (event_type in ('opened', 'suspended', 'resumed', 'closed', 'abandoned', 'blocked', 'unblocked', 'reopened', 'invalidated', 'follow_up_created')),
    reason text,
    caused_by_work_unit_id integer references work_units(id),
    authority_event_id integer references authority_events(id),
    status_domain text not null check (status_domain in ('work_unit', 'activation')),
    previous_status text,
    next_status text,
    created_at text not null
);

create table if not exists suspend_snapshots (
    id integer primary key,
    work_unit_activation_id integer not null references work_unit_activations(id) on delete cascade,
    work_unit_id integer not null references work_units(id) on delete cascade,
    reason text not null,
    active_task_ids text,
    next_action text not null,
    selected_gate_id integer,
    authority_refs text,
    review_scope_refs text,
    repository_heads text,
    repository_snapshot_ids text,
    repository_status text,
    dirty_state_summary text,
    open_findings text,
    assumptions text,
    created_at text not null
);

create table if not exists resume_checks (
    id integer primary key,
    work_unit_id integer not null references work_units(id) on delete cascade,
    work_unit_activation_id integer not null references work_unit_activations(id) on delete cascade,
    suspend_snapshot_id integer not null references suspend_snapshots(id) on delete cascade,
    maturity text not null check (maturity in ('basic', 'trace-aware', 'repo-aware')),
    status text not null default 'pending' check (status in ('pending', 'consumed', 'stale')),
    result text not null check (result in ('allowed', 'blocked', 'needs_design_review', 'needs_gate_reselection', 'needs_user_decision')),
    authority_event_high_watermark integer,
    activation_stack_revision integer,
    repository_snapshot_id integer,
    repository_state_revision integer,
    allowed_next_action text,
    blocking_reason text,
    consumed_at text,
    consumed_by_work_unit_event_id integer references work_unit_events(id),
    created_at text not null
);

create table if not exists resume_check_items (
    id integer primary key,
    resume_check_id integer not null references resume_checks(id) on delete cascade,
    check_name text not null check (check_name in ('resume_target_suspended', 'snapshot_exists', 'suspend_reason_exists', 'next_action_exists', 'deeper_frames_closed', 'blocking_dependencies_clear', 'active_tasks_current', 'authority_refs_current', 'review_scope_refs_current', 'design_version_current', 'task_derivation_current', 'checklist_current', 'selected_gate_current', 'review_plan_current', 'open_findings_current', 'repository_heads_current', 'repository_state_current', 'assumptions_current')),
    result text not null check (result in ('pass', 'fail', 'not_checked', 'needs_evidence')),
    evidence_ref text,
    blocking_action text,
    details text
);

create trigger if not exists trg_resume_check_repository_snapshot_insert
before insert on resume_checks
for each row
when new.repository_snapshot_id is not null
  and (
      not exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
      or (select project_id from work_units where id = new.work_unit_id) != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
  )
begin
    select raise(abort, 'resume check repository snapshot must match work unit project_id');
end;

create trigger if not exists trg_resume_check_repository_snapshot_update
before update of work_unit_id, repository_snapshot_id on resume_checks
for each row
when new.repository_snapshot_id is not null
  and (
      not exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
      or (select project_id from work_units where id = new.work_unit_id) != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
  )
begin
    select raise(abort, 'resume check repository snapshot must match work unit project_id');
end;

create table if not exists work_unit_dependencies (
    id integer primary key,
    work_unit_id integer not null references work_units(id) on delete cascade,
    depends_on_work_unit_id integer not null references work_units(id) on delete cascade,
    dependency_type text not null check (dependency_type in ('blocks', 'discovered_by', 'supersedes', 'invalidates_assumption', 'regression_of', 'invalidates_closure', 'follow_up_of')),
    reason text,
    status text not null default 'open' check (status in ('open', 'resolved')),
    created_at text not null,
    resolved_at text,
    resolved_by_work_unit_event_id integer references work_unit_events(id)
);

create table if not exists tasks (
    id integer primary key,
    work_unit_id integer references work_units(id) on delete cascade,
    title text not null,
    priority text not null default 'medium' check (priority in ('critical', 'high', 'medium', 'low')),
    status text not null default 'open' check (status in ('open', 'blocked', 'closed', 'accepted_out_of_scope')),
    source text not null default 'user' check (source in ('user', 'plan', 'review', 'coverage', 'design', 'work_record')),
    parent_task_id integer references tasks(id),
    details text,
    completion_condition text,
    closed_by_commit text
);

create table if not exists decisions (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    decision_key text,
    topic text not null,
    decision text not null,
    rationale text,
    compatibility_impact text,
    status text not null default 'accepted' check (status in ('accepted', 'rejected', 'superseded')),
    authority_refs text,
    created_at text not null
);

create table if not exists acceptance_records (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    target_type text not null check (target_type in (
        'task', 'design_requirement', 'validation_gate_template', 'design_file',
        'design_requirement_key', 'coverage_item', 'finding', 'validation_gate',
        'validation_run', 'repository_state_classification',
        'repository_snapshot_comparison', 'review_plan', 'checklist_item',
        'command_profile', 'command_usage', 'command_deviation',
        'rule_binding', 'stale_record'
    )),
    task_id integer references tasks(id),
    design_requirement_id integer references design_requirements(id),
    validation_gate_template_id integer references validation_gate_templates(id),
    coverage_item_id integer references coverage_items(id),
    finding_id integer references findings(id),
    validation_gate_id integer references validation_gates(id),
    validation_run_id integer references validation_runs(id),
    repository_state_classification_id integer references repository_state_classifications(id),
    repository_snapshot_comparison_id integer references repository_snapshot_comparisons(id),
    review_plan_id integer references review_plans(id),
    checklist_item_id integer references checklist_items(id),
    command_profile_id integer references command_profiles(id),
    command_usage_id integer references command_usages(id),
    command_deviation_id integer references command_deviations(id),
    rule_binding_id integer references rule_bindings(id),
    stale_record_type text,
    stale_record_id integer,
    design_package_key text,
    design_file_path text,
    design_requirement_key text,
    acceptance_type text not null check (acceptance_type in (
        'accepted_out_of_scope', 'explicit_exception', 'evidence_gap',
        'classified_failure', 'stale_accepted'
    )),
    reason text not null,
    scope text,
    created_by text not null check (created_by in ('user', 'agent', 'system')),
    status text not null check (status in ('proposed', 'approved', 'rejected', 'expired')),
    approved_by_authority_event_id integer references authority_events(id),
    approved_at text,
    created_at text not null,
    review_impact text,
    check (
        (
            (case when task_id is not null then 1 else 0 end) +
            (case when design_requirement_id is not null then 1 else 0 end) +
            (case when validation_gate_template_id is not null then 1 else 0 end) +
            (case when coverage_item_id is not null then 1 else 0 end) +
            (case when finding_id is not null then 1 else 0 end) +
            (case when validation_gate_id is not null then 1 else 0 end) +
            (case when validation_run_id is not null then 1 else 0 end) +
            (case when repository_state_classification_id is not null then 1 else 0 end) +
            (case when repository_snapshot_comparison_id is not null then 1 else 0 end) +
            (case when review_plan_id is not null then 1 else 0 end) +
            (case when checklist_item_id is not null then 1 else 0 end) +
            (case when command_profile_id is not null then 1 else 0 end) +
            (case when command_usage_id is not null then 1 else 0 end) +
            (case when command_deviation_id is not null then 1 else 0 end) +
            (case when rule_binding_id is not null then 1 else 0 end) +
            (case when design_package_key is not null and design_file_path is not null and design_requirement_key is null then 1 else 0 end) +
            (case when design_package_key is not null and design_requirement_key is not null and design_file_path is null then 1 else 0 end) +
            (case when stale_record_type is not null and stale_record_id is not null then 1 else 0 end)
        ) = 1
        and (
            (target_type = 'task' and task_id is not null)
            or (target_type = 'design_requirement' and design_requirement_id is not null)
            or (target_type = 'validation_gate_template' and validation_gate_template_id is not null)
            or (target_type = 'coverage_item' and coverage_item_id is not null)
            or (target_type = 'finding' and finding_id is not null)
            or (target_type = 'validation_gate' and validation_gate_id is not null)
            or (target_type = 'validation_run' and validation_run_id is not null)
            or (target_type = 'repository_state_classification' and repository_state_classification_id is not null)
            or (target_type = 'repository_snapshot_comparison' and repository_snapshot_comparison_id is not null)
            or (target_type = 'review_plan' and review_plan_id is not null)
            or (target_type = 'checklist_item' and checklist_item_id is not null)
            or (target_type = 'command_profile' and command_profile_id is not null)
            or (target_type = 'command_usage' and command_usage_id is not null)
            or (target_type = 'command_deviation' and command_deviation_id is not null)
            or (target_type = 'rule_binding' and rule_binding_id is not null)
            or (target_type = 'design_file' and design_package_key is not null and design_file_path is not null)
            or (target_type = 'design_requirement_key' and design_package_key is not null and design_requirement_key is not null)
            or (target_type = 'stale_record' and stale_record_type is not null and stale_record_id is not null)
        )
    )
);

create trigger if not exists trg_repository_state_classification_acceptance_insert
before insert on repository_state_classifications
for each row
when (new.classification = 'accepted_exception' and new.acceptance_record_id is null)
  or (new.classification != 'accepted_exception' and new.acceptance_record_id is not null)
  or (new.acceptance_record_id is not null and (
      not exists (select 1 from acceptance_records where id = new.acceptance_record_id)
      or (select project_id from acceptance_records where id = new.acceptance_record_id) != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
  ))
begin
    select raise(abort, 'repository state classification acceptance must match snapshot project_id');
end;

create trigger if not exists trg_repository_state_classification_acceptance_update
before update of repository_snapshot_id, classification, acceptance_record_id on repository_state_classifications
for each row
when (new.classification = 'accepted_exception' and new.acceptance_record_id is null)
  or (new.classification != 'accepted_exception' and new.acceptance_record_id is not null)
  or (new.acceptance_record_id is not null and (
      not exists (select 1 from acceptance_records where id = new.acceptance_record_id)
      or (select project_id from acceptance_records where id = new.acceptance_record_id) != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
  ))
begin
    select raise(abort, 'repository state classification acceptance must match snapshot project_id');
end;
"#;
