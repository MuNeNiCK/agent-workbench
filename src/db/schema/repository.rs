pub(super) const SQL: &str = r#"
create table if not exists schema_migrations (
    version integer primary key,
    applied_at text not null
);

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
before update on validation_link_repair_runs
begin
    select raise(abort, 'validation link repair audit is immutable');
end;

create trigger if not exists trg_validation_link_repair_runs_immutable_delete
before delete on validation_link_repair_runs
begin
    select raise(abort, 'validation link repair audit is immutable');
end;

create trigger if not exists trg_validation_link_repair_changes_immutable_update
before update on validation_link_repair_changes
begin
    select raise(abort, 'validation link repair audit is immutable');
end;

create trigger if not exists trg_validation_link_repair_changes_immutable_delete
before delete on validation_link_repair_changes
begin
    select raise(abort, 'validation link repair audit is immutable');
end;

create table if not exists projects (
    id integer primary key,
    name text not null,
    root_path text not null unique,
    created_at text not null,
    updated_at text not null
);

create table if not exists repositories (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    name text not null,
    path text not null,
    current_head text,
    status_summary text,
    last_checked_at text,
    unique(project_id, name),
    unique(project_id, path)
);

create table if not exists repository_snapshots (
    id integer primary key,
    repository_id integer not null references repositories(id) on delete cascade,
    work_unit_activation_id integer references work_unit_activations(id),
    head_sha text,
    branch text,
    status_summary text,
    is_clean integer not null check (is_clean in (0, 1)),
    created_at text not null
);

create table if not exists repository_dirty_entries (
    id integer primary key,
    repository_snapshot_id integer not null references repository_snapshots(id) on delete cascade,
    path text not null,
    change_type text not null check (change_type in ('modified', 'added', 'deleted', 'renamed', 'untracked', 'ignored')),
    staged integer not null default 0 check (staged in (0, 1)),
    content_hash text
);

create table if not exists repository_state_classifications (
    id integer primary key,
    repository_snapshot_id integer not null references repository_snapshots(id) on delete cascade,
    dirty_entry_id integer references repository_dirty_entries(id) on delete cascade,
    classification text not null check (classification in ('expected', 'unrelated', 'generated', 'requires_action', 'accepted_exception')),
    reason text not null,
    acceptance_record_id integer references acceptance_records(id),
    created_at text not null
);

create table if not exists repository_snapshot_comparisons (
    id integer primary key,
    base_repository_snapshot_id integer not null references repository_snapshots(id) on delete cascade,
    current_repository_snapshot_id integer not null references repository_snapshots(id) on delete cascade,
    comparison_type text not null check (comparison_type in ('resume', 'close', 'validation', 'review')),
    head_changed integer not null check (head_changed in (0, 1)),
    dirty_state_changed integer not null check (dirty_state_changed in (0, 1)),
    nested_repository_changed integer not null default 0 check (nested_repository_changed in (0, 1)),
    result text not null check (result in ('same', 'changed_classified', 'changed_unclassified')),
    created_at text not null
);

create table if not exists git_commits (
    id integer primary key,
    repository_id integer not null references repositories(id) on delete cascade,
    commit_sha text not null,
    short_sha text,
    subject text,
    author_name text,
    author_email text,
    committed_at text,
    parent_shas text,
    created_at text not null,
    unique(repository_id, commit_sha)
);

create table if not exists git_file_changes (
    id integer primary key,
    git_commit_id integer not null references git_commits(id) on delete cascade,
    repository_id integer not null references repositories(id) on delete cascade,
    path text not null,
    old_path text,
    change_type text not null check (change_type in ('added', 'modified', 'deleted', 'renamed', 'copied')),
    additions integer,
    deletions integer,
    content_hash text
);

create trigger if not exists trg_repository_snapshot_activation_project_insert
before insert on repository_snapshots
for each row
when new.work_unit_activation_id is not null
  and (select project_id from repositories where id = new.repository_id)
      != (select project_id from work_unit_activations where id = new.work_unit_activation_id)
begin
    select raise(abort, 'repository snapshot activation must match repository project_id');
end;

create trigger if not exists trg_repository_snapshot_activation_project_update
before update of repository_id, work_unit_activation_id on repository_snapshots
for each row
when new.work_unit_activation_id is not null
  and (select project_id from repositories where id = new.repository_id)
      != (select project_id from work_unit_activations where id = new.work_unit_activation_id)
begin
    select raise(abort, 'repository snapshot activation must match repository project_id');
end;

create trigger if not exists trg_repository_state_classification_dirty_insert
before insert on repository_state_classifications
for each row
when new.dirty_entry_id is not null
  and new.repository_snapshot_id != (
      select repository_snapshot_id
      from repository_dirty_entries
      where id = new.dirty_entry_id
  )
begin
    select raise(abort, 'repository state classification dirty entry must match snapshot');
end;

create trigger if not exists trg_repository_state_classification_dirty_update
before update of repository_snapshot_id, dirty_entry_id on repository_state_classifications
for each row
when new.dirty_entry_id is not null
  and new.repository_snapshot_id != (
      select repository_snapshot_id
      from repository_dirty_entries
      where id = new.dirty_entry_id
  )
begin
    select raise(abort, 'repository state classification dirty entry must match snapshot');
end;

create trigger if not exists trg_repository_snapshot_comparison_repository_insert
before insert on repository_snapshot_comparisons
for each row
when (select repository_id from repository_snapshots where id = new.base_repository_snapshot_id)
  != (select repository_id from repository_snapshots where id = new.current_repository_snapshot_id)
begin
    select raise(abort, 'repository snapshot comparison requires one repository');
end;

create trigger if not exists trg_repository_snapshot_comparison_repository_update
before update of base_repository_snapshot_id, current_repository_snapshot_id on repository_snapshot_comparisons
for each row
when (select repository_id from repository_snapshots where id = new.base_repository_snapshot_id)
  != (select repository_id from repository_snapshots where id = new.current_repository_snapshot_id)
begin
    select raise(abort, 'repository snapshot comparison requires one repository');
end;

create trigger if not exists trg_git_file_change_repository_insert
before insert on git_file_changes
for each row
when new.repository_id != (select repository_id from git_commits where id = new.git_commit_id)
begin
    select raise(abort, 'git file change repository must match git commit repository');
end;

create trigger if not exists trg_git_file_change_repository_update
before update of git_commit_id, repository_id on git_file_changes
for each row
when new.repository_id != (select repository_id from git_commits where id = new.git_commit_id)
begin
    select raise(abort, 'git file change repository must match git commit repository');
end;

create table if not exists authorities (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    path_or_label text not null,
    authority_type text not null check (authority_type in ('user', 'design', 'spec', 'plan', 'policy', 'validation')),
    scope text,
    precedence integer not null default 0,
    summary text not null,
    status text not null default 'active' check (status in ('active', 'inactive', 'superseded')),
    created_at text not null,
    updated_at text not null
);

create unique index if not exists ux_authorities_identity
on authorities(project_id, path_or_label, authority_type, coalesce(scope, 'project'));

create table if not exists authority_events (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    authority_id integer references authorities(id),
    event_type text not null check (event_type in ('user_instruction', 'design_doc', 'agents', 'policy', 'review_result', 'validation_result')),
    source text,
    text_or_summary text not null,
    scope text,
    precedence integer not null default 0,
    supersedes_event_id integer references authority_events(id),
    status text not null default 'active' check (status in ('active', 'inactive', 'superseded')),
    created_at text not null
);

create trigger if not exists trg_authority_event_authority_project_insert
before insert on authority_events
for each row
when new.authority_id is not null
 and new.project_id != (select project_id from authorities where id = new.authority_id)
begin
    select raise(abort, 'authority event authority must match project_id');
end;

create trigger if not exists trg_authority_event_authority_project_update
before update of project_id, authority_id on authority_events
for each row
when new.authority_id is not null
 and new.project_id != (select project_id from authorities where id = new.authority_id)
begin
    select raise(abort, 'authority event authority must match project_id');
end;
"#;
