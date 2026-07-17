pub(super) const SQL: &str = r#"
create table if not exists review_policies (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    name text not null,
    review_type text not null check (review_type in ('design_review', 'design_implementation_diff', 'design_task_decomposition', 'implementation_review', 'general')),
    max_fresh_agents integer not null default 1,
    max_resume_agents integer not null default 1,
    max_parallel_agents integer not null default 1,
    required_consecutive_clean_fresh_runs integer not null default 1,
    required_consecutive_clean_resume_runs integer not null default 1,
    stop_on_severity text not null default 'none' check (stop_on_severity in ('critical', 'high', 'medium', 'low', 'none')),
    allow_resume_review integer not null default 1 check (allow_resume_review in (0, 1)),
    allow_fresh_review integer not null default 1 check (allow_fresh_review in (0, 1)),
    allow_new_findings_in_resume integer not null default 0 check (allow_new_findings_in_resume in (0, 1)),
    on_max_agents_exceeded text not null default 'block' check (on_max_agents_exceeded in ('block', 'accept_with_user_approval', 'mark_exhausted')),
    run_count_scope text not null default 'review_plan' check (run_count_scope in ('review_plan', 'review_scope', 'work_unit')),
    default_run_mode text not null default 'fresh' check (default_run_mode in ('fresh', 'resume')),
    created_at text not null,
    unique(project_id, name)
);

create table if not exists review_scopes (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    name text not null,
    review_type text not null check (review_type in ('design_review', 'design_implementation_diff', 'design_task_decomposition', 'implementation_review', 'general')),
    agent_role text not null check (agent_role in ('general', 'design_document_review', 'design_task_decomposition', 'design_implementation_diff_review', 'implementation_review')),
    user_declared_scope text not null,
    allowed_inputs text,
    forbidden_judgments text,
    expected_output_type text,
    exclusions text,
    prompt_template_ref text,
    status text not null default 'open' check (status in ('open', 'blocked', 'clean', 'closed')),
    no_findings_streak integer not null default 0,
    created_at text not null,
    unique(project_id, name)
);

create table if not exists review_plans (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    work_unit_id integer not null references work_units(id) on delete cascade,
    design_version_id integer references design_versions(id) on delete cascade,
    review_type text not null check (review_type in ('design_review', 'design_implementation_diff', 'design_task_decomposition', 'implementation_review', 'general')),
    required integer not null default 1 check (required in (0, 1)),
    stage text not null check (stage in ('design-ready', 'implementation-ready', 'close-ready', 'resume-ready')),
    scope text,
    clean_condition text,
    stop_condition text,
    review_policy_id integer not null references review_policies(id),
    review_scope_id integer references review_scopes(id),
    status text not null default 'open' check (status in ('open', 'blocked', 'clean', 'accepted_exception', 'not_required', 'exhausted', 'needs_user_decision')),
    created_at text not null
);

create trigger if not exists trg_review_policy_referenced_update
before update of project_id, review_type on review_policies
for each row
when exists (
    select 1
    from review_plans p
    where p.review_policy_id = old.id
      and (p.project_id != new.project_id or p.review_type != new.review_type)
)
begin
    select raise(abort, 'review policy update would break referenced review plans');
end;

create trigger if not exists trg_review_policy_resume_findings_update
before update of allow_new_findings_in_resume on review_policies
for each row
when new.allow_new_findings_in_resume = 0
  and exists (
      select 1
      from review_plans p
      join review_runs r on r.review_plan_id = p.id
      left join findings f on f.review_run_id = r.id
      where p.review_policy_id = old.id
        and r.run_type = 'resume'
        and (r.new_findings_count > 0 or f.id is not null)
  )
begin
    select raise(abort, 'review policy update would conflict with resume findings');
end;

create trigger if not exists trg_review_scope_referenced_update
before update of project_id, review_type on review_scopes
for each row
when exists (
    select 1
    from review_plans p
    where p.review_scope_id = old.id
      and (p.project_id != new.project_id or p.review_type != new.review_type)
)
or exists (
    select 1
    from review_runs r
    where r.review_scope_id = old.id
      and r.project_id != new.project_id
)
begin
    select raise(abort, 'review scope update would break referenced review plans or runs');
end;

create table if not exists review_plan_targets (
    id integer primary key,
    review_plan_id integer not null references review_plans(id) on delete cascade,
    target_type text not null check (target_type in ('design_version', 'design_requirement', 'task', 'work_unit', 'phase', 'repository_snapshot', 'file', 'symbol')),
    design_version_id integer references design_versions(id),
    design_requirement_id integer references design_requirements(id),
    task_id integer references tasks(id),
    work_unit_id integer references work_units(id),
    phase_id integer references work_phases(id),
    repository_snapshot_id integer,
    file_path text,
    symbol text,
    check (
        (target_type = 'design_version' and design_version_id is not null and design_requirement_id is null and task_id is null and work_unit_id is null and repository_snapshot_id is null and file_path is null and symbol is null)
        or (target_type = 'design_requirement' and design_version_id is null and design_requirement_id is not null and task_id is null and work_unit_id is null and repository_snapshot_id is null and file_path is null and symbol is null)
        or (target_type = 'task' and design_version_id is null and design_requirement_id is null and task_id is not null and work_unit_id is null and repository_snapshot_id is null and file_path is null and symbol is null)
        or (target_type = 'work_unit' and design_version_id is null and design_requirement_id is null and task_id is null and work_unit_id is not null and repository_snapshot_id is null and file_path is null and symbol is null)
        or (target_type = 'repository_snapshot' and design_version_id is null and design_requirement_id is null and task_id is null and work_unit_id is null and repository_snapshot_id is not null and file_path is null and symbol is null)
        or (target_type = 'file' and design_version_id is null and design_requirement_id is null and task_id is null and work_unit_id is null and repository_snapshot_id is null and file_path is not null and symbol is null)
        or (target_type = 'symbol' and design_version_id is null and design_requirement_id is null and task_id is null and work_unit_id is null and repository_snapshot_id is null and file_path is null and symbol is not null)
    )
);

create table if not exists review_runs (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    review_scope_id integer references review_scopes(id),
    review_plan_id integer not null references review_plans(id),
    run_type text not null check (run_type in ('fresh', 'resume', 'coverage')),
    run_purpose text not null check (run_purpose in ('new_unbiased_review', 'finding_fix_verification', 'coverage_audit')),
    target_type text not null check (target_type in ('design_version', 'design_requirement', 'task', 'work_unit', 'repository_snapshot', 'file', 'symbol')),
    design_version_id integer references design_versions(id),
    design_requirement_id integer references design_requirements(id),
    task_id integer references tasks(id),
    work_unit_id integer references work_units(id),
    repository_snapshot_id integer,
    file_path text,
    symbol text,
    target_ref text,
    prompt_deviations text,
    result_summary text,
    new_findings_count integer not null default 0 check (new_findings_count >= 0),
    carried_findings_checked integer not null default 0 check (carried_findings_checked >= 0),
    clean_run integer not null default 0 check (clean_run in (0, 1)),
    review_provenance text not null default 'self_recorded' check (review_provenance in ('self_recorded', 'external_agent', 'human_review')),
    review_provenance_ref text,
    status text not null default 'requested' check (status in ('requested', 'running', 'completed', 'failed', 'cancelled')),
    created_at text not null,
    check (
        (run_type = 'fresh' and run_purpose = 'new_unbiased_review')
        or (run_type = 'resume' and run_purpose = 'finding_fix_verification')
        or (run_type = 'coverage' and run_purpose = 'coverage_audit')
    ),
    check (
        clean_run = 0
        or (status = 'completed' and new_findings_count = 0)
    )
);

create table if not exists review_agent_invocations (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    review_plan_id integer references review_plans(id),
    review_run_id integer references review_runs(id),
    run_type text not null check (run_type in ('fresh', 'resume', 'coverage')),
    agent_label text,
    external_agent_id text,
    status text not null default 'requested' check (status in ('requested', 'running', 'completed', 'failed', 'cancelled')),
    started_at text,
    finished_at text
);

create table if not exists findings (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    review_run_id integer not null references review_runs(id) on delete cascade,
    finding_type text not null check (finding_type in ('design_finding', 'design_implementation_drift', 'design_task_gap', 'implementation_finding', 'coverage_finding')),
    severity text not null check (severity in ('critical', 'high', 'medium', 'low')),
    description text not null,
    classification text not null default 'unclassified' check (classification in ('unclassified', 'valid', 'invalid', 'design_conflict', 'needs_evidence')),
    status text not null default 'open' check (status in ('open', 'closed', 'accepted_out_of_scope')),
    lifecycle_state text not null default 'open' check(lifecycle_state in ('open','remediating','awaiting_verification','closed')),
    close_reason text check(close_reason is null or close_reason in ('verified','rejected','authority_disposed','legacy_rejected')),
    design_requirement_id integer references design_requirements(id),
    task_id integer references tasks(id),
    created_at text not null
);

create table if not exists review_adjudication_decisions (
    id integer primary key, project_id integer not null references projects(id) on delete cascade,
    owner_decision_id integer not null unique references owner_decisions(id),
    review_run_id integer not null references review_runs(id),
    value text not null check(value in ('accepted','rejected','needs_evidence')),
    predecessor_id integer references review_adjudication_decisions(id), created_at text not null
);
create table if not exists finding_disposition_decisions (
    id integer primary key, project_id integer not null references projects(id) on delete cascade,
    owner_decision_id integer not null unique references owner_decisions(id),
    finding_id integer not null references findings(id),
    value text not null check(value in ('accepted','rejected','needs_evidence','design_conflict','deferred','authority_disposed')),
    predecessor_id integer references finding_disposition_decisions(id), created_at text not null
);
create table if not exists verification_adjudication_decisions (
    id integer primary key, project_id integer not null references projects(id) on delete cascade,
    owner_decision_id integer not null unique references owner_decisions(id),
    closure_attempt_id integer not null references closure_attempts(id),
    value text not null check(value in ('accepted','rejected','needs_evidence')),
    predecessor_id integer references verification_adjudication_decisions(id), created_at text not null,
    foreign key(project_id,closure_attempt_id) references closure_attempts(project_id,id)
);
create table if not exists finding_lifecycle_events (
    id integer primary key, project_id integer not null references projects(id) on delete cascade,
    finding_id integer not null references findings(id), owner_decision_id integer references owner_decisions(id),
    from_state text not null check(from_state in ('open','remediating','awaiting_verification','closed')),
    to_state text not null check(to_state in ('open','remediating','awaiting_verification','closed')),
    effect text not null, created_at text not null
);
create table if not exists review_correction_events (
    id integer primary key, project_id integer not null references projects(id) on delete cascade,
    owner_decision_id integer not null unique references owner_decisions(id),
    historical_owner_decision_id integer not null references owner_decisions(id),
    boundary_handle text not null, outcome text not null check(outcome in ('accepted','rejected','needs_evidence')),
    created_at text not null, unique(project_id,historical_owner_decision_id,boundary_handle)
);
create table if not exists review_boundary_snapshots (
    id integer primary key, project_id integer not null references projects(id) on delete cascade,
    owner_ref text not null, boundary_handle text not null, snapshot_handle text not null,
    historical_owner_decision_id integer references owner_decisions(id), dependency_digest text not null check(length(dependency_digest)=64),
    status text not null check(status in ('current','invalidated')), created_at text not null, invalidated_at text,
    unique(project_id,boundary_handle,historical_owner_decision_id), unique(project_id,snapshot_handle)
);
create table if not exists review_correction_recovery_obligations (
    id integer primary key, project_id integer not null references projects(id) on delete cascade,
    correction_event_id integer not null references review_correction_events(id), owner_ref text not null,
    obligation text not null, status text not null check(status='open'), created_at text not null,
    unique(project_id,correction_event_id,obligation)
);
create table if not exists finding_decision_epochs (
    id integer primary key, project_id integer not null references projects(id) on delete cascade,
    finding_id integer not null references findings(id), epoch_number integer not null,
    terminal_decision_id integer references owner_decisions(id), reopen_decision_id integer references owner_decisions(id),
    status text not null check(status in ('open','terminal')), created_at text not null,
    unique(project_id,finding_id,epoch_number)
);

create trigger if not exists trg_review_adjudication_immutable_update before update on review_adjudication_decisions begin select raise(abort,'review adjudication is append-only'); end;
create trigger if not exists trg_review_adjudication_immutable_delete before delete on review_adjudication_decisions begin select raise(abort,'review adjudication is append-only'); end;
create trigger if not exists trg_finding_disposition_immutable_update before update on finding_disposition_decisions begin select raise(abort,'finding disposition is append-only'); end;
create trigger if not exists trg_finding_disposition_immutable_delete before delete on finding_disposition_decisions begin select raise(abort,'finding disposition is append-only'); end;
create trigger if not exists trg_verification_adjudication_immutable_update before update on verification_adjudication_decisions begin select raise(abort,'verification adjudication is append-only'); end;
create trigger if not exists trg_verification_adjudication_immutable_delete before delete on verification_adjudication_decisions begin select raise(abort,'verification adjudication is append-only'); end;
create trigger if not exists trg_finding_lifecycle_event_immutable_update before update on finding_lifecycle_events begin select raise(abort,'finding lifecycle events are append-only'); end;
create trigger if not exists trg_finding_lifecycle_event_immutable_delete before delete on finding_lifecycle_events begin select raise(abort,'finding lifecycle events are append-only'); end;
create trigger if not exists trg_review_correction_immutable_update before update on review_correction_events begin select raise(abort,'review corrections are append-only'); end;
create trigger if not exists trg_review_correction_immutable_delete before delete on review_correction_events begin select raise(abort,'review corrections are append-only'); end;
create trigger if not exists trg_review_boundary_snapshot_immutable before update on review_boundary_snapshots for each row
when old.status!='current' or new.project_id!=old.project_id or new.owner_ref!=old.owner_ref or new.boundary_handle!=old.boundary_handle or new.snapshot_handle!=old.snapshot_handle or new.historical_owner_decision_id is not old.historical_owner_decision_id or new.dependency_digest!=old.dependency_digest or new.created_at!=old.created_at or new.status!='invalidated' or new.invalidated_at is null
begin select raise(abort,'review boundary snapshot identity is immutable'); end;
create trigger if not exists trg_review_boundary_snapshot_delete before delete on review_boundary_snapshots begin select raise(abort,'review boundary snapshots are append-only'); end;
create trigger if not exists trg_review_correction_obligation_update before update on review_correction_recovery_obligations begin select raise(abort,'review correction recovery obligations are append-only'); end;
create trigger if not exists trg_review_correction_obligation_delete before delete on review_correction_recovery_obligations begin select raise(abort,'review correction recovery obligations are append-only'); end;
create trigger if not exists trg_finding_epoch_immutable_update before update on finding_decision_epochs begin select raise(abort,'finding decision epochs are append-only'); end;
create trigger if not exists trg_finding_epoch_immutable_delete before delete on finding_decision_epochs begin select raise(abort,'finding decision epochs are append-only'); end;

create trigger if not exists trg_review_adjudication_project_insert before insert on review_adjudication_decisions
when not exists(select 1 from owner_decisions od join review_runs r on r.id=new.review_run_id where od.id=new.owner_decision_id and od.project_id=new.project_id and r.project_id=new.project_id)
begin select raise(abort,'review adjudication project mismatch'); end;
create trigger if not exists trg_finding_disposition_project_insert before insert on finding_disposition_decisions
when not exists(select 1 from owner_decisions od join findings f on f.id=new.finding_id where od.id=new.owner_decision_id and od.project_id=new.project_id and f.project_id=new.project_id)
begin select raise(abort,'finding disposition project mismatch'); end;
create trigger if not exists trg_review_correction_project_insert before insert on review_correction_events
when not exists(select 1 from owner_decisions current join owner_decisions historical on historical.id=new.historical_owner_decision_id where current.id=new.owner_decision_id and current.project_id=new.project_id and historical.project_id=new.project_id)
begin select raise(abort,'review correction project mismatch'); end;
create trigger if not exists trg_review_boundary_project_insert before insert on review_boundary_snapshots
when new.historical_owner_decision_id is not null and not exists(select 1 from owner_decisions od where od.id=new.historical_owner_decision_id and od.project_id=new.project_id)
begin select raise(abort,'review boundary snapshot project mismatch'); end;
create trigger if not exists trg_finding_epoch_project_insert before insert on finding_decision_epochs
when not exists(select 1 from findings f where f.id=new.finding_id and f.project_id=new.project_id)
 or (new.terminal_decision_id is not null and not exists(select 1 from owner_decisions od where od.id=new.terminal_decision_id and od.project_id=new.project_id))
 or (new.reopen_decision_id is not null and not exists(select 1 from owner_decisions od where od.id=new.reopen_decision_id and od.project_id=new.project_id))
begin select raise(abort,'finding epoch project mismatch'); end;

create trigger if not exists trg_finding_review_type_insert
before insert on findings
for each row
when not exists (
    select 1
    from review_runs r
    join review_plans p on p.id = r.review_plan_id
    where r.id = new.review_run_id
      and (
          (p.review_type = 'design_review' and new.finding_type = 'design_finding')
          or (p.review_type = 'design_implementation_diff' and new.finding_type = 'design_implementation_drift')
          or (p.review_type = 'design_task_decomposition' and new.finding_type = 'design_task_gap')
          or (p.review_type = 'implementation_review' and new.finding_type in ('implementation_finding', 'coverage_finding'))
          or p.review_type = 'general'
      )
)
begin
    select raise(abort, 'finding type must match review type');
end;

create trigger if not exists trg_finding_review_type_update
before update of review_run_id, finding_type on findings
for each row
when not exists (
    select 1
    from review_runs r
    join review_plans p on p.id = r.review_plan_id
    where r.id = new.review_run_id
      and (
          (p.review_type = 'design_review' and new.finding_type = 'design_finding')
          or (p.review_type = 'design_implementation_diff' and new.finding_type = 'design_implementation_drift')
          or (p.review_type = 'design_task_decomposition' and new.finding_type = 'design_task_gap')
          or (p.review_type = 'implementation_review' and new.finding_type in ('implementation_finding', 'coverage_finding'))
          or p.review_type = 'general'
      )
)
begin
    select raise(abort, 'finding type must match review type');
end;

create table if not exists closures (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    finding_id integer not null references findings(id) on delete cascade,
    design_invariant text not null,
    design_citations text,
    implementation_evidence text,
    affected_surfaces text,
    same_invariant_search text,
    other_violations_found text,
    fix_plan text,
    tests_or_gates text,
    verification_plan text,
    closed_by_commit text,
    created_at text not null
);

create table if not exists finding_verifications (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    review_run_id integer not null references review_runs(id) on delete cascade,
    finding_id integer not null references findings(id) on delete cascade,
    closure_id integer not null references closures(id) on delete cascade,
    result text not null check (result in ('verified', 'not_fixed', 'needs_evidence', 'out_of_scope')),
    notes text,
    created_at text not null,
    unique(review_run_id, finding_id, closure_id)
);

create trigger if not exists trg_review_plan_policy_required_insert
before insert on review_plans
for each row
when new.review_policy_id is null
begin
    select raise(abort, 'review plan requires review policy');
end;

create trigger if not exists trg_review_plan_policy_required_update
before update of review_policy_id on review_plans
for each row
when new.review_policy_id is null
begin
    select raise(abort, 'review plan requires review policy');
end;

create trigger if not exists trg_review_plan_project_insert
before insert on review_plans
for each row
when new.project_id != (select project_id from work_units where id = new.work_unit_id)
  or (new.design_version_id is not null and new.project_id != (select project_id from design_versions where id = new.design_version_id))
  or (new.review_policy_id is not null and new.project_id != (select project_id from review_policies where id = new.review_policy_id))
  or (new.review_scope_id is not null and new.project_id != (select project_id from review_scopes where id = new.review_scope_id))
begin
    select raise(abort, 'review plan project_id must match referenced rows');
end;

create trigger if not exists trg_review_plan_project_update
before update of project_id, work_unit_id, design_version_id, review_policy_id, review_scope_id on review_plans
for each row
when new.project_id != (select project_id from work_units where id = new.work_unit_id)
  or (new.design_version_id is not null and new.project_id != (select project_id from design_versions where id = new.design_version_id))
  or (new.review_policy_id is not null and new.project_id != (select project_id from review_policies where id = new.review_policy_id))
  or (new.review_scope_id is not null and new.project_id != (select project_id from review_scopes where id = new.review_scope_id))
begin
    select raise(abort, 'review plan project_id must match referenced rows');
end;

create trigger if not exists trg_review_plan_type_insert
before insert on review_plans
for each row
when (new.review_policy_id is not null and new.review_type != (select review_type from review_policies where id = new.review_policy_id))
  or (new.review_scope_id is not null and new.review_type != (select review_type from review_scopes where id = new.review_scope_id))
begin
    select raise(abort, 'review plan type must match policy and scope type');
end;

create trigger if not exists trg_review_plan_type_update
before update of review_type, review_policy_id, review_scope_id on review_plans
for each row
when (new.review_policy_id is not null and new.review_type != (select review_type from review_policies where id = new.review_policy_id))
  or (new.review_scope_id is not null and new.review_type != (select review_type from review_scopes where id = new.review_scope_id))
begin
    select raise(abort, 'review plan type must match policy and scope type');
end;

create trigger if not exists trg_review_plan_resume_policy_update
before update of review_policy_id on review_plans
for each row
when (select allow_new_findings_in_resume from review_policies where id = new.review_policy_id) = 0
  and exists (
      select 1
      from review_runs r
      left join findings f on f.review_run_id = r.id
      where r.review_plan_id = new.id
        and r.run_type = 'resume'
        and (r.new_findings_count > 0 or f.id is not null)
  )
begin
    select raise(abort, 'review plan policy update would conflict with resume findings');
end;

create trigger if not exists trg_review_run_plan_required_insert
before insert on review_runs
for each row
when new.review_plan_id is null
begin
    select raise(abort, 'review run requires review plan');
end;

create trigger if not exists trg_review_run_plan_required_update
before update of review_plan_id on review_runs
for each row
when new.review_plan_id is null
begin
    select raise(abort, 'review run requires review plan');
end;

create trigger if not exists trg_review_run_project_insert
before insert on review_runs
for each row
when (new.review_scope_id is not null and new.project_id != (select project_id from review_scopes where id = new.review_scope_id))
  or (new.review_plan_id is not null and new.project_id != (select project_id from review_plans where id = new.review_plan_id))
begin
    select raise(abort, 'review run project_id must match referenced rows');
end;

create trigger if not exists trg_review_run_target_insert
before insert on review_runs
for each row
when (new.target_type = 'design_version' and not (
        new.design_version_id is not null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is null
        and new.phase_id is null
        and new.repository_snapshot_id is null
        and new.project_id = (select project_id from design_versions where id = new.design_version_id)
    ))
  or (new.target_type = 'design_requirement' and not (
        new.design_version_id is null
        and new.design_requirement_id is not null
        and new.task_id is null
        and new.work_unit_id is null
        and new.phase_id is null
        and new.repository_snapshot_id is null
        and new.project_id = (select project_id from design_requirements where id = new.design_requirement_id)
    ))
  or (new.target_type = 'task' and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is not null
        and new.work_unit_id is null
        and new.phase_id is null
        and new.repository_snapshot_id is null
        and new.project_id = coalesce(
            (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
            (select id from projects order by id limit 1)
        )
    ))
  or (new.target_type = 'work_unit' and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is not null
        and new.phase_id is null
        and new.repository_snapshot_id is null
        and new.project_id = (select project_id from work_units where id = new.work_unit_id)
    ))
  or (new.target_type = 'phase' and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is null
        and new.phase_id is not null
        and new.repository_snapshot_id is null
        and new.file_path is null
        and new.symbol is null
        and new.project_id = (select project_id from work_phases where id = new.phase_id)
    ))
  or (new.target_type = 'repository_snapshot' and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is null
        and new.phase_id is null
        and new.repository_snapshot_id is not null
        and exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
        and new.project_id = (
            select r.project_id
            from repository_snapshots s
            join repositories r on r.id = s.repository_id
            where s.id = new.repository_snapshot_id
        )
    ))
  or (new.target_type in ('file', 'symbol') and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is null
        and new.phase_id is null
        and new.repository_snapshot_id is null
        and (
            (new.target_type = 'file' and new.file_path is not null and new.symbol is null)
            or (new.target_type = 'symbol' and new.file_path is null and new.symbol is not null)
        )
    ))
begin
    select raise(abort, 'review run target must match target_type and project_id');
end;

create trigger if not exists trg_review_run_project_update
before update of project_id, review_scope_id, review_plan_id on review_runs
for each row
when (new.review_scope_id is not null and new.project_id != (select project_id from review_scopes where id = new.review_scope_id))
  or (new.review_plan_id is not null and new.project_id != (select project_id from review_plans where id = new.review_plan_id))
begin
    select raise(abort, 'review run project_id must match referenced rows');
end;

create trigger if not exists trg_review_run_target_update
before update of project_id, target_type, design_version_id, design_requirement_id, task_id, work_unit_id, phase_id, repository_snapshot_id, file_path, symbol, target_ref on review_runs
for each row
when (new.target_type = 'design_version' and not (
        new.design_version_id is not null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is null
        and new.phase_id is null
        and new.repository_snapshot_id is null
        and new.project_id = (select project_id from design_versions where id = new.design_version_id)
    ))
  or (new.target_type = 'design_requirement' and not (
        new.design_version_id is null
        and new.design_requirement_id is not null
        and new.task_id is null
        and new.work_unit_id is null
        and new.phase_id is null
        and new.repository_snapshot_id is null
        and new.project_id = (select project_id from design_requirements where id = new.design_requirement_id)
    ))
  or (new.target_type = 'task' and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is not null
        and new.work_unit_id is null
        and new.phase_id is null
        and new.repository_snapshot_id is null
        and new.project_id = coalesce(
            (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
            (select id from projects order by id limit 1)
        )
    ))
  or (new.target_type = 'work_unit' and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is not null
        and new.phase_id is null
        and new.repository_snapshot_id is null
        and new.project_id = (select project_id from work_units where id = new.work_unit_id)
    ))
  or (new.target_type = 'phase' and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is null
        and new.phase_id is not null
        and new.repository_snapshot_id is null
        and new.file_path is null
        and new.symbol is null
        and new.project_id = (select project_id from work_phases where id = new.phase_id)
    ))
  or (new.target_type = 'repository_snapshot' and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is null
        and new.phase_id is null
        and new.repository_snapshot_id is not null
        and exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
        and new.project_id = (
            select r.project_id
            from repository_snapshots s
            join repositories r on r.id = s.repository_id
            where s.id = new.repository_snapshot_id
        )
    ))
  or (new.target_type in ('file', 'symbol') and not (
        new.design_version_id is null
        and new.design_requirement_id is null
        and new.task_id is null
        and new.work_unit_id is null
        and new.phase_id is null
        and new.repository_snapshot_id is null
        and (
            (new.target_type = 'file' and new.file_path is not null and new.symbol is null)
            or (new.target_type = 'symbol' and new.file_path is null and new.symbol is not null)
        )
    ))
begin
    select raise(abort, 'review run target must match target_type and project_id');
end;

create trigger if not exists trg_review_run_plan_target_insert
before insert on review_runs
for each row
when new.review_plan_id is not null
  and (
      (
          new.target_type != 'phase'
          and not exists (
              select 1
              from review_plan_targets t
              where t.review_plan_id = new.review_plan_id
                and (
                    (new.target_type = 'design_version' and t.target_type = 'design_version' and t.design_version_id = new.design_version_id)
                    or (new.target_type = 'design_requirement' and t.target_type = 'design_requirement' and t.design_requirement_id = new.design_requirement_id)
                    or (new.target_type = 'task' and t.target_type = 'task' and t.task_id = new.task_id)
                    or (new.target_type = 'work_unit' and t.target_type = 'work_unit' and t.work_unit_id = new.work_unit_id)
                    or (new.target_type = 'repository_snapshot' and t.target_type = 'repository_snapshot' and t.repository_snapshot_id = new.repository_snapshot_id)
                    or (new.target_type = 'file' and t.target_type = 'file' and t.file_path = new.file_path)
                    or (new.target_type = 'symbol' and t.target_type = 'symbol' and t.symbol = new.symbol)
                )
          )
      )
      or (
          new.target_type = 'phase'
          and not exists (
              select 1
              from work_phase_review_targets t
              where t.review_plan_id = new.review_plan_id
                and t.phase_id = new.phase_id
          )
      )
  )
begin
    select raise(abort, 'review run target must be included in review plan targets');
end;

create trigger if not exists trg_review_run_plan_target_update
before update of review_plan_id, target_type, design_version_id, design_requirement_id, task_id, work_unit_id, phase_id, repository_snapshot_id, file_path, symbol, target_ref on review_runs
for each row
when new.review_plan_id is not null
  and (
      (
          new.target_type != 'phase'
          and not exists (
              select 1
              from review_plan_targets t
              where t.review_plan_id = new.review_plan_id
                and (
                    (new.target_type = 'design_version' and t.target_type = 'design_version' and t.design_version_id = new.design_version_id)
                    or (new.target_type = 'design_requirement' and t.target_type = 'design_requirement' and t.design_requirement_id = new.design_requirement_id)
                    or (new.target_type = 'task' and t.target_type = 'task' and t.task_id = new.task_id)
                    or (new.target_type = 'work_unit' and t.target_type = 'work_unit' and t.work_unit_id = new.work_unit_id)
                    or (new.target_type = 'repository_snapshot' and t.target_type = 'repository_snapshot' and t.repository_snapshot_id = new.repository_snapshot_id)
                    or (new.target_type = 'file' and t.target_type = 'file' and t.file_path = new.file_path)
                    or (new.target_type = 'symbol' and t.target_type = 'symbol' and t.symbol = new.symbol)
                )
          )
      )
      or (
          new.target_type = 'phase'
          and not exists (
              select 1
              from work_phase_review_targets t
              where t.review_plan_id = new.review_plan_id
                and t.phase_id = new.phase_id
          )
      )
  )
begin
    select raise(abort, 'review run target must be included in review plan targets');
end;

create trigger if not exists trg_review_run_type_purpose_insert
before insert on review_runs
for each row
when not (
    (new.run_type = 'fresh' and new.run_purpose = 'new_unbiased_review')
    or (new.run_type = 'resume' and new.run_purpose = 'finding_fix_verification')
    or (new.run_type = 'coverage' and new.run_purpose = 'coverage_audit')
)
begin
    select raise(abort, 'review run type must match purpose');
end;

create trigger if not exists trg_review_run_type_purpose_update
before update of run_type, run_purpose on review_runs
for each row
when not (
    (new.run_type = 'fresh' and new.run_purpose = 'new_unbiased_review')
    or (new.run_type = 'resume' and new.run_purpose = 'finding_fix_verification')
    or (new.run_type = 'coverage' and new.run_purpose = 'coverage_audit')
)
begin
    select raise(abort, 'review run type must match purpose');
end;

create trigger if not exists trg_review_run_resume_policy_insert
before insert on review_runs
for each row
when new.run_type = 'resume'
  and new.new_findings_count > 0
  and (select allow_new_findings_in_resume
       from review_policies
       where id = (select review_policy_id from review_plans where id = new.review_plan_id)) = 0
begin
    select raise(abort, 'new findings are disabled for resume review by policy');
end;

create trigger if not exists trg_review_run_resume_policy_update
before update of review_plan_id, run_type, new_findings_count on review_runs
for each row
when new.run_type = 'resume'
  and (select allow_new_findings_in_resume
       from review_policies
       where id = (select review_policy_id from review_plans where id = new.review_plan_id)) = 0
  and (
      new.new_findings_count > 0
      or exists (select 1 from findings where review_run_id = new.id)
  )
begin
    select raise(abort, 'new findings are disabled for resume review by policy');
end;

create trigger if not exists trg_review_run_result_insert
before insert on review_runs
for each row
when new.new_findings_count < 0
  or new.carried_findings_checked < 0
  or (new.clean_run = 1 and (new.status != 'completed' or new.new_findings_count != 0))
  or new.review_provenance not in ('self_recorded', 'external_agent', 'human_review')
  or (new.review_provenance in ('external_agent', 'human_review') and coalesce(new.review_provenance_ref, '') = '')
begin
    select raise(abort, 'review run result is inconsistent');
end;

create trigger if not exists trg_review_run_result_update
before update of new_findings_count, carried_findings_checked, clean_run, status, review_provenance, review_provenance_ref on review_runs
for each row
when new.new_findings_count < 0
  or new.carried_findings_checked < 0
  or (new.clean_run = 1 and (new.status != 'completed' or new.new_findings_count != 0))
  or new.review_provenance not in ('self_recorded', 'external_agent', 'human_review')
  or (new.review_provenance in ('external_agent', 'human_review') and coalesce(new.review_provenance_ref, '') = '')
  or (new.clean_run = 1 and exists (
      select 1 from findings where review_run_id = new.id
  ))
begin
    select raise(abort, 'review run result is inconsistent');
end;
"#;
