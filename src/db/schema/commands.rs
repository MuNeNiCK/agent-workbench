pub(super) const SQL: &str = r#"
create table if not exists rule_bindings (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    rule_source_type text not null check (rule_source_type in ('authority_event', 'user_correction', 'command_profile', 'review_policy', 'work_unit', 'validation_gate', 'acceptance_record', 'skill_default')),
    authority_event_id integer references authority_events(id),
    user_correction_id integer,
    command_profile_id integer,
    review_policy_id integer,
    review_plan_id integer references review_plans(id),
    work_unit_id integer references work_units(id),
    validation_gate_id integer,
    acceptance_record_id integer,
    scope_type text not null check (scope_type in ('project', 'repository', 'design_package', 'work_unit', 'agent_role', 'command', 'review')),
    scope_key text,
    precedence integer not null default 0,
    status text not null default 'active' check (status in ('active', 'shadowed', 'inactive', 'superseded')),
    created_at text not null
);

create table if not exists user_corrections (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    authority_event_id integer references authority_events(id),
    scope text,
    correction_type text not null check (correction_type in ('process', 'design', 'implementation', 'review', 'command', 'communication', 'other')),
    mistake_pattern text not null,
    correction text not null,
    applies_to text not null check (applies_to in ('current_work_unit', 'project', 'repository', 'design_package', 'command_profile', 'agent_role')),
    severity text not null default 'medium' check (severity in ('critical', 'high', 'medium', 'low')),
    status text not null default 'active' check (status in ('active', 'superseded', 'retired')),
    supersedes_user_correction_id integer references user_corrections(id),
    created_at text not null
);

create table if not exists command_profiles (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    repository_id integer,
    name text not null,
    command text not null,
    command_type text not null check (command_type in ('validation', 'test', 'lint', 'format', 'build', 'dev_server', 'review_context', 'export', 'other')),
    scope text,
    status text not null default 'candidate' check (status in ('candidate', 'preferred', 'fixed', 'deprecated', 'blocked')),
    stability text not null default 'context_dependent' check (stability in ('stable', 'context_dependent', 'experimental')),
    working_directory text,
    environment text,
    timeout text,
    expected_result text,
    replaces_command_profile_id integer references command_profiles(id),
    source text not null default 'agent_observed' check (source in ('user', 'agent_observed', 'design', 'validation_gate')),
    created_at text not null,
    updated_at text not null,
    unique(project_id, name)
);

create trigger if not exists trg_command_profile_repository_insert
before insert on command_profiles
for each row
when new.repository_id is not null
  and (
      not exists (select 1 from repositories where id = new.repository_id)
      or new.project_id != (select project_id from repositories where id = new.repository_id)
  )
begin
    select raise(abort, 'command profile repository must match project_id');
end;

create trigger if not exists trg_command_profile_repository_update
before update of project_id, repository_id on command_profiles
for each row
when new.repository_id is not null
  and (
      not exists (select 1 from repositories where id = new.repository_id)
      or new.project_id != (select project_id from repositories where id = new.repository_id)
  )
begin
    select raise(abort, 'command profile repository must match project_id');
end;

create table if not exists command_usages (
    id integer primary key,
    project_id integer references projects(id) on delete cascade,
    command_profile_id integer references command_profiles(id),
    work_unit_id integer references work_units(id),
    work_unit_activation_id integer references work_unit_activations(id),
    command text not null,
    result text not null check (result in ('pass', 'fail', 'timeout', 'cancelled', 'unknown')),
    log_path text,
    repository_snapshot_id integer,
    created_at text not null
);

create trigger if not exists trg_command_usage_project_insert
before insert on command_usages
for each row
when new.project_id is null
  or not exists (select 1 from projects where id = new.project_id)
  or (new.command_profile_id is not null and not exists (
      select 1 from command_profiles where id = new.command_profile_id
  ))
  or (new.work_unit_id is not null and not exists (
      select 1 from work_units where id = new.work_unit_id
  ))
  or (new.work_unit_activation_id is not null and not exists (
      select 1 from work_unit_activations where id = new.work_unit_activation_id
  ))
  or (
      new.command_profile_id is not null
      and new.project_id != (select project_id from command_profiles where id = new.command_profile_id)
  )
  or (
      new.work_unit_id is not null
      and new.project_id != (select project_id from work_units where id = new.work_unit_id)
  )
  or (
      new.work_unit_activation_id is not null
      and new.project_id != (select project_id from work_unit_activations where id = new.work_unit_activation_id)
  )
  or (
      new.command_profile_id is not null
      and new.work_unit_id is not null
      and (select project_id from command_profiles where id = new.command_profile_id) != (
          select project_id from work_units where id = new.work_unit_id
      )
  )
  or (
      new.command_profile_id is not null
      and new.work_unit_activation_id is not null
      and (select project_id from command_profiles where id = new.command_profile_id) != (
          select project_id from work_unit_activations where id = new.work_unit_activation_id
      )
  )
  or (
      new.work_unit_id is not null
      and new.work_unit_activation_id is not null
      and (select project_id from work_units where id = new.work_unit_id) != (
          select project_id from work_unit_activations where id = new.work_unit_activation_id
      )
  )
begin
    select raise(abort, 'command usage references must match project');
end;

create trigger if not exists trg_command_usage_project_update
before update of project_id, command_profile_id, work_unit_id, work_unit_activation_id on command_usages
for each row
when new.project_id is null
  or not exists (select 1 from projects where id = new.project_id)
  or (new.command_profile_id is not null and not exists (
      select 1 from command_profiles where id = new.command_profile_id
  ))
  or (new.work_unit_id is not null and not exists (
      select 1 from work_units where id = new.work_unit_id
  ))
  or (new.work_unit_activation_id is not null and not exists (
      select 1 from work_unit_activations where id = new.work_unit_activation_id
  ))
  or (
      new.command_profile_id is not null
      and new.project_id != (select project_id from command_profiles where id = new.command_profile_id)
  )
  or (
      new.work_unit_id is not null
      and new.project_id != (select project_id from work_units where id = new.work_unit_id)
  )
  or (
      new.work_unit_activation_id is not null
      and new.project_id != (select project_id from work_unit_activations where id = new.work_unit_activation_id)
  )
  or (
      new.command_profile_id is not null
      and new.work_unit_id is not null
      and (select project_id from command_profiles where id = new.command_profile_id) != (
          select project_id from work_units where id = new.work_unit_id
      )
  )
  or (
      new.command_profile_id is not null
      and new.work_unit_activation_id is not null
      and (select project_id from command_profiles where id = new.command_profile_id) != (
          select project_id from work_unit_activations where id = new.work_unit_activation_id
      )
  )
  or (
      new.work_unit_id is not null
      and new.work_unit_activation_id is not null
      and (select project_id from work_units where id = new.work_unit_id) != (
          select project_id from work_unit_activations where id = new.work_unit_activation_id
      )
  )
begin
    select raise(abort, 'command usage references must match project');
end;

create table if not exists command_deviations (
    id integer primary key,
    command_profile_id integer not null references command_profiles(id) on delete cascade,
    command_usage_id integer references command_usages(id),
    work_unit_id integer references work_units(id),
    reason text not null,
    status text not null default 'proposed' check (status in ('proposed', 'approved', 'rejected')),
    acceptance_record_id integer,
    created_at text not null
);

create trigger if not exists trg_command_usage_repository_snapshot_insert
before insert on command_usages
for each row
when new.repository_snapshot_id is not null
  and (
      not exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
      or new.project_id != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
      or (
          new.command_profile_id is not null
          and (select project_id from command_profiles where id = new.command_profile_id) != (
              select r.project_id
              from repository_snapshots s
              join repositories r on r.id = s.repository_id
              where s.id = new.repository_snapshot_id
          )
      )
      or (
          new.work_unit_id is not null
          and (select project_id from work_units where id = new.work_unit_id) != (
              select r.project_id
              from repository_snapshots s
              join repositories r on r.id = s.repository_id
              where s.id = new.repository_snapshot_id
          )
      )
      or (
          new.work_unit_activation_id is not null
          and (select project_id from work_unit_activations where id = new.work_unit_activation_id) != (
              select r.project_id
              from repository_snapshots s
              join repositories r on r.id = s.repository_id
              where s.id = new.repository_snapshot_id
          )
      )
  )
begin
    select raise(abort, 'command usage repository snapshot must match referenced project');
end;

create trigger if not exists trg_command_usage_repository_snapshot_update
before update of project_id, command_profile_id, work_unit_id, work_unit_activation_id, repository_snapshot_id on command_usages
for each row
when new.repository_snapshot_id is not null
  and (
      not exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
      or new.project_id != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
      or (
          new.command_profile_id is not null
          and (select project_id from command_profiles where id = new.command_profile_id) != (
              select r.project_id
              from repository_snapshots s
              join repositories r on r.id = s.repository_id
              where s.id = new.repository_snapshot_id
          )
      )
      or (
          new.work_unit_id is not null
          and (select project_id from work_units where id = new.work_unit_id) != (
              select r.project_id
              from repository_snapshots s
              join repositories r on r.id = s.repository_id
              where s.id = new.repository_snapshot_id
          )
      )
      or (
          new.work_unit_activation_id is not null
          and (select project_id from work_unit_activations where id = new.work_unit_activation_id) != (
              select r.project_id
              from repository_snapshots s
              join repositories r on r.id = s.repository_id
              where s.id = new.repository_snapshot_id
          )
      )
  )
begin
    select raise(abort, 'command usage repository snapshot must match referenced project');
end;

create table if not exists work_records (
    id integer primary key,
    project_id integer references projects(id) on delete cascade,
    work_unit_id integer references work_units(id) on delete cascade,
    topic text not null,
    work_performed text,
    next_actions text,
    notable_operations text,
    export_path text,
    created_at text not null
);

create trigger if not exists trg_work_record_project_insert
before insert on work_records
for each row
when new.project_id is null
  or not exists (select 1 from projects where id = new.project_id)
  or (
      new.work_unit_id is not null
      and (
          not exists (select 1 from work_units where id = new.work_unit_id)
          or new.project_id != (select project_id from work_units where id = new.work_unit_id)
      )
  )
begin
    select raise(abort, 'work record project_id must match referenced work unit');
end;

create trigger if not exists trg_work_record_project_update
before update of project_id, work_unit_id on work_records
for each row
when new.project_id is null
  or not exists (select 1 from projects where id = new.project_id)
  or (
      new.work_unit_id is not null
      and (
          not exists (select 1 from work_units where id = new.work_unit_id)
          or new.project_id != (select project_id from work_units where id = new.work_unit_id)
      )
  )
begin
    select raise(abort, 'work record project_id must match referenced work unit');
end;

create table if not exists work_record_commands (
    id integer primary key,
    work_record_id integer not null references work_records(id) on delete cascade,
    command_usage_id integer references command_usages(id),
    command_profile_id integer references command_profiles(id),
    command text,
    result text,
    log_path text,
    note text
);

create trigger if not exists trg_work_record_command_required_insert
before insert on work_record_commands
for each row
when new.command_usage_id is null and new.command is null
begin
    select raise(abort, 'work record command requires command_usage_id or command');
end;

create trigger if not exists trg_work_record_command_required_update
before update of command_usage_id, command on work_record_commands
for each row
when new.command_usage_id is null and new.command is null
begin
    select raise(abort, 'work record command requires command_usage_id or command');
end;

create table if not exists work_record_commits (
    id integer primary key,
    work_record_id integer not null references work_records(id) on delete cascade,
    git_commit_id integer,
    commit_sha text,
    role text not null default 'referenced' check (role in ('created', 'referenced', 'validation_base', 'rollback_point')),
    note text,
    auto_linked integer not null default 0 check (auto_linked in (0, 1))
);

create trigger if not exists trg_work_record_commit_required_insert
before insert on work_record_commits
for each row
when new.commit_sha is null
begin
    select raise(abort, 'work record commit requires commit_sha');
end;

create trigger if not exists trg_work_record_commit_required_update
before update of commit_sha on work_record_commits
for each row
when new.commit_sha is null
begin
    select raise(abort, 'work record commit requires commit_sha');
end;

create table if not exists work_record_files (
    id integer primary key,
    work_record_id integer not null references work_records(id) on delete cascade,
    git_file_change_id integer,
    repository_id integer,
    path text not null,
    role text not null default 'changed' check (role in ('changed', 'reviewed', 'generated', 'evidence', 'ignored')),
    note text,
    auto_linked integer not null default 0 check (auto_linked in (0, 1)),
    repository_auto_linked integer not null default 0 check (repository_auto_linked in (0, 1))
);

create trigger if not exists trg_work_record_command_project_insert
before insert on work_record_commands
for each row
when (new.command_usage_id is not null and (
      not exists (select 1 from command_usages where id = new.command_usage_id)
      or (select project_id from command_usages where id = new.command_usage_id) != (
          select project_id from work_records where id = new.work_record_id
      )
  ))
  or (new.command_profile_id is not null and (
      not exists (select 1 from command_profiles where id = new.command_profile_id)
      or (select project_id from command_profiles where id = new.command_profile_id) != (
          select project_id from work_records where id = new.work_record_id
      )
  ))
begin
    select raise(abort, 'work record command must match referenced project');
end;

create trigger if not exists trg_work_record_command_project_update
before update of work_record_id, command_usage_id, command_profile_id on work_record_commands
for each row
when (new.command_usage_id is not null and (
      not exists (select 1 from command_usages where id = new.command_usage_id)
      or (select project_id from command_usages where id = new.command_usage_id) != (
          select project_id from work_records where id = new.work_record_id
      )
  ))
  or (new.command_profile_id is not null and (
      not exists (select 1 from command_profiles where id = new.command_profile_id)
      or (select project_id from command_profiles where id = new.command_profile_id) != (
          select project_id from work_records where id = new.work_record_id
      )
  ))
begin
    select raise(abort, 'work record command must match referenced project');
end;

create trigger if not exists trg_work_record_commit_git_insert
before insert on work_record_commits
for each row
when new.git_commit_id is not null
  and (
      not exists (select 1 from git_commits where id = new.git_commit_id)
      or new.commit_sha is null
      or new.commit_sha != (select commit_sha from git_commits where id = new.git_commit_id)
      or (select project_id from work_records where id = new.work_record_id) != (
          select r.project_id
          from git_commits c
          join repositories r on r.id = c.repository_id
          where c.id = new.git_commit_id
      )
  )
begin
    select raise(abort, 'work record commit must match git commit');
end;

create trigger if not exists trg_work_record_commit_git_update
before update of work_record_id, git_commit_id, commit_sha on work_record_commits
for each row
when new.git_commit_id is not null
  and (
      not exists (select 1 from git_commits where id = new.git_commit_id)
      or new.commit_sha is null
      or new.commit_sha != (select commit_sha from git_commits where id = new.git_commit_id)
      or (select project_id from work_records where id = new.work_record_id) != (
          select r.project_id
          from git_commits c
          join repositories r on r.id = c.repository_id
          where c.id = new.git_commit_id
      )
  )
begin
    select raise(abort, 'work record commit must match git commit');
end;

create trigger if not exists trg_work_record_file_git_insert
before insert on work_record_files
for each row
when (new.repository_id is not null and not exists (select 1 from repositories where id = new.repository_id))
  or (
      new.git_file_change_id is not null
      and (
          new.repository_id is null
          or not exists (select 1 from git_file_changes where id = new.git_file_change_id)
          or new.repository_id != (select repository_id from git_file_changes where id = new.git_file_change_id)
          or new.path != (select path from git_file_changes where id = new.git_file_change_id)
      )
  )
  or (
      new.repository_id is not null
      and (select project_id from work_records where id = new.work_record_id) != (
          select project_id from repositories where id = new.repository_id
      )
  )
begin
    select raise(abort, 'work record file must match repository or git file change');
end;

create trigger if not exists trg_work_record_file_git_update
before update of work_record_id, git_file_change_id, repository_id, path on work_record_files
for each row
when (new.repository_id is not null and not exists (select 1 from repositories where id = new.repository_id))
  or (
      new.git_file_change_id is not null
      and (
          new.repository_id is null
          or not exists (select 1 from git_file_changes where id = new.git_file_change_id)
          or new.repository_id != (select repository_id from git_file_changes where id = new.git_file_change_id)
          or new.path != (select path from git_file_changes where id = new.git_file_change_id)
      )
  )
  or (
      new.repository_id is not null
      and (select project_id from work_records where id = new.work_record_id) != (
          select project_id from repositories where id = new.repository_id
      )
  )
begin
    select raise(abort, 'work record file must match repository or git file change');
end;

create table if not exists work_record_forks (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    source_work_unit_id integer references work_units(id),
    source_work_unit_activation_id integer references work_unit_activations(id),
    source_work_record_id integer references work_records(id),
    source_repository_snapshot_id integer,
    source_git_commit_id integer,
    source_git_commit_sha text,
    forked_work_unit_id integer references work_units(id),
    fork_reason text not null check (fork_reason in ('design_changed', 'agent_drift', 'invalid_assumption', 'failed_validation', 'user_requested_redo', 'other')),
    discard_policy text not null default 'keep_history' check (discard_policy in ('keep_history', 'supersede_source', 'mark_abandoned')),
    status text not null default 'open' check (status in ('open', 'closed', 'abandoned')),
    created_by_authority_event_id integer references authority_events(id),
    created_at text not null,
    closed_at text
);

create trigger if not exists trg_work_record_fork_repository_git_insert
before insert on work_record_forks
for each row
when new.forked_work_unit_id is null
  or not exists (select 1 from work_units where id = new.forked_work_unit_id)
  or (
      (case when new.source_work_unit_activation_id is not null then 1 else 0 end)
      + (case when new.source_work_record_id is not null then 1 else 0 end)
      + (case when new.source_repository_snapshot_id is not null then 1 else 0 end)
      + (case when new.source_git_commit_id is not null or new.source_git_commit_sha is not null then 1 else 0 end)
  ) != 1
  or new.project_id != (select project_id from work_units where id = new.forked_work_unit_id)
  or (new.source_work_unit_id is not null and new.project_id != (
      select project_id from work_units where id = new.source_work_unit_id
  ))
  or (new.source_work_unit_activation_id is not null and new.project_id != (
      select project_id from work_unit_activations where id = new.source_work_unit_activation_id
  ))
  or (new.source_work_record_id is not null and (
      not exists (select 1 from work_records where id = new.source_work_record_id)
      or new.project_id != (
          select project_id from work_records where id = new.source_work_record_id
      )
  ))
  or (new.source_repository_snapshot_id is not null and (
      not exists (select 1 from repository_snapshots where id = new.source_repository_snapshot_id)
      or new.project_id != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.source_repository_snapshot_id
      )
  ))
  or (new.source_git_commit_id is not null and (
      not exists (select 1 from git_commits where id = new.source_git_commit_id)
      or new.project_id != (
          select r.project_id
          from git_commits c
          join repositories r on r.id = c.repository_id
          where c.id = new.source_git_commit_id
      )
      or (new.source_git_commit_sha is not null and new.source_git_commit_sha != (
          select commit_sha from git_commits where id = new.source_git_commit_id
      ))
  ))
begin
    select raise(abort, 'work record fork repository and git sources must match project');
end;

create trigger if not exists trg_work_record_fork_repository_git_update
before update of project_id, source_work_unit_id, source_work_unit_activation_id, source_work_record_id, source_repository_snapshot_id, source_git_commit_id, source_git_commit_sha, forked_work_unit_id on work_record_forks
for each row
when new.forked_work_unit_id is null
  or not exists (select 1 from work_units where id = new.forked_work_unit_id)
  or (
      (case when new.source_work_unit_activation_id is not null then 1 else 0 end)
      + (case when new.source_work_record_id is not null then 1 else 0 end)
      + (case when new.source_repository_snapshot_id is not null then 1 else 0 end)
      + (case when new.source_git_commit_id is not null or new.source_git_commit_sha is not null then 1 else 0 end)
  ) != 1
  or new.project_id != (select project_id from work_units where id = new.forked_work_unit_id)
  or (new.source_work_unit_id is not null and new.project_id != (
      select project_id from work_units where id = new.source_work_unit_id
  ))
  or (new.source_work_unit_activation_id is not null and new.project_id != (
      select project_id from work_unit_activations where id = new.source_work_unit_activation_id
  ))
  or (new.source_work_record_id is not null and (
      not exists (select 1 from work_records where id = new.source_work_record_id)
      or new.project_id != (
          select project_id from work_records where id = new.source_work_record_id
      )
  ))
  or (new.source_repository_snapshot_id is not null and (
      not exists (select 1 from repository_snapshots where id = new.source_repository_snapshot_id)
      or new.project_id != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.source_repository_snapshot_id
      )
  ))
  or (new.source_git_commit_id is not null and (
      not exists (select 1 from git_commits where id = new.source_git_commit_id)
      or new.project_id != (
          select r.project_id
          from git_commits c
          join repositories r on r.id = c.repository_id
          where c.id = new.source_git_commit_id
      )
      or (new.source_git_commit_sha is not null and new.source_git_commit_sha != (
          select commit_sha from git_commits where id = new.source_git_commit_id
      ))
  ))
begin
    select raise(abort, 'work record fork repository and git sources must match project');
end;
"#;
