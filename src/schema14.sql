pragma foreign_keys = on;

create table schema_metadata (
  singleton integer primary key check (singleton = 1),
  schema_version integer not null check (schema_version = 14),
  manifest_digest text not null check (length(manifest_digest) = 64)
);

create table projects (
  handle text primary key,
  name text not null,
  root_path text not null,
  created_at text not null
);

create table records (
  handle text primary key,
  project_handle text not null references projects(handle) on delete cascade,
  kind text not null check (kind in (
    'work','activation','task','phase','checklist','checklist_item',
    'stale_disposition','review_policy','review_plan','finding','closure',
    'closure_attempt','correction','kpt_review','kpt_item','design_package',
    'design_version','requirement','design_decision','validation_gate',
    'coverage','rule','acceptance','command_profile','command_usage','repository',
    'repository_snapshot','repository_commit','repository_change',
    'repository_comparison','work_record'
  )),
  state text not null,
  revision integer not null check (revision >= 1),
  owner_handle text references records(handle),
  parent_handle text references records(handle),
  record_key text,
  occurrence integer not null default 1 check (occurrence >= 1),
  title text,
  priority text,
  stage text,
  required integer check (required in (0,1)),
  ordinal integer,
  policy_limit integer check (policy_limit is null or policy_limit >= 0),
  policy_action text,
  content_digest text,
  details text,
  created_at text not null,
  updated_at text not null,
  unique(project_handle, kind, owner_handle, record_key, occurrence)
);

create table relations (
  handle text primary key,
  project_handle text not null references projects(handle) on delete cascade,
  kind text not null check (kind in (
    'work_dependency','phase_dependency','membership','review_target',
    'remediation','trace','checklist_target','evidence_target','record_link',
    'rule_scope'
  )),
  source_handle text not null references records(handle),
  target_handle text not null references records(handle),
  state text not null,
  revision integer not null check (revision >= 1),
  required integer check (required in (0,1)),
  ordinal integer,
  expected_target_revision integer,
  details text,
  created_at text not null,
  updated_at text not null,
  unique(project_handle, kind, source_handle, target_handle)
);

create table claims (
  handle text primary key,
  project_handle text not null references projects(handle) on delete cascade,
  kind text not null check (kind in ('review','verification')),
  target_handle text not null references records(handle),
  plan_handle text references records(handle),
  attempt_handle text references records(handle),
  target_revision integer not null check (target_revision >= 1),
  outcome text not null,
  producer text not null,
  scope_digest text not null,
  evidence_text text,
  created_at text not null
);

create table decisions (
  handle text primary key,
  project_handle text not null references projects(handle) on delete cascade,
  kind text not null check (kind in (
    'review','verification','waiver','acceptance','exception','dependency',
    'stale','correction'
  )),
  target_handle text references records(handle),
  target_relation_handle text references relations(handle),
  claim_handle text references claims(handle),
  predecessor_handle text references decisions(handle),
  value text not null,
  expected_target_revision integer not null check (expected_target_revision >= 1),
  resulting_state text not null,
  reason text not null,
  risk text,
  created_at text not null,
  check ((target_handle is not null) != (target_relation_handle is not null)),
  unique(project_handle, target_handle, predecessor_handle),
  unique(project_handle, target_relation_handle, predecessor_handle)
);

create table evidence (
  handle text primary key,
  project_handle text not null references projects(handle) on delete cascade,
  kind text not null check (kind in (
    'validation','implementation','repository','command_usage','work_record',
    'coverage','update'
  )),
  owner_handle text not null references records(handle),
  subject_handle text not null references records(handle),
  subject_revision integer not null check (subject_revision >= 1),
  producer text not null,
  result text not null,
  content_digest text,
  details text,
  created_at text not null
);

create table snapshots (
  handle text primary key,
  project_handle text not null references projects(handle) on delete cascade,
  owner_handle text not null references records(handle),
  owner_revision integer not null check (owner_revision >= 1),
  maturity text not null check (maturity in ('basic','trace-aware','repo-aware')),
  semantic_digest text not null,
  created_at text not null
);

create table snapshot_components (
  snapshot_handle text not null references snapshots(handle) on delete cascade,
  component_kind text not null check (component_kind in (
    'owner','obligation','assumption','evidence','repository'
  )),
  component_handle text not null,
  component_state text,
  component_revision text not null,
  component_digest text not null,
  primary key(snapshot_handle, component_kind, component_handle)
);

create table lifecycle_events (
  handle text primary key,
  project_handle text not null references projects(handle) on delete cascade,
  target_handle text references records(handle),
  target_relation_handle text references relations(handle),
  decision_handle text references decisions(handle),
  event_kind text not null,
  from_state text,
  to_state text not null,
  from_revision integer,
  to_revision integer not null check (to_revision >= 1),
  details text,
  created_at text not null,
  check ((target_handle is not null) != (target_relation_handle is not null))
);

create table update_audits (
  handle text primary key,
  project_handle text not null references projects(handle) on delete cascade,
  source_schema integer not null,
  target_schema integer not null check (target_schema = 14),
  source_profile text not null,
  source_identity text not null,
  backup_handle text not null,
  restore_receipt_handle text,
  mode text not null check (mode in ('fresh','reset')),
  created_at text not null
);

create table legacy_ledgers (
  handle text primary key,
  project_handle text not null references projects(handle) on delete cascade,
  source_schema integer not null,
  source_profile text not null,
  source_identity text not null,
  backup_handle text not null,
  reset_reason text not null,
  created_at text not null,
  unique(project_handle, source_identity)
);
