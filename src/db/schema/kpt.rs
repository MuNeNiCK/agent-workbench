pub(super) const SQL: &str = r#"
create table if not exists kpt_reviews (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    scope text,
    period_start text,
    period_end text,
    trigger text not null default 'manual' check (trigger in ('manual', 'work_unit_close', 'review_close', 'scheduled')),
    summary text,
    status text not null default 'open' check (status in ('open', 'closed')),
    created_at text not null,
    closed_at text
);

create table if not exists kpt_items (
    id integer primary key,
    kpt_review_id integer not null references kpt_reviews(id) on delete cascade,
    item_type text not null check (item_type in ('keep', 'problem', 'try')),
    title text not null,
    details text,
    severity text not null default 'medium' check (severity in ('critical', 'high', 'medium', 'low')),
    linked_user_correction_id integer references user_corrections(id),
    linked_command_profile_id integer references command_profiles(id),
    linked_review_finding_id integer references findings(id),
    linked_task_id integer references tasks(id),
    proposed_action text,
    status text not null default 'open' check (status in ('open', 'accepted', 'converted', 'converted_to_task', 'dismissed')),
    created_at text not null
);

create table if not exists kpt_item_conversions (
    id integer primary key,
    kpt_item_id integer not null references kpt_items(id) on delete cascade,
    target_type text not null check (target_type in ('task', 'command_profile', 'review_policy', 'design_version', 'decision', 'user_correction')),
    task_id integer references tasks(id),
    command_profile_id integer references command_profiles(id),
    review_policy_id integer references review_policies(id),
    design_version_id integer references design_versions(id),
    decision_id integer references decisions(id),
    user_correction_id integer references user_corrections(id),
    item_revision text,
    predecessor_handle text,
    request_identity text,
    receipt_identity text,
    current_handle text,
    created_at text not null,
    check (
        (target_type = 'task' and task_id is not null and command_profile_id is null and review_policy_id is null and design_version_id is null and decision_id is null and user_correction_id is null)
        or (target_type = 'command_profile' and task_id is null and command_profile_id is not null and review_policy_id is null and design_version_id is null and decision_id is null and user_correction_id is null)
        or (target_type = 'review_policy' and task_id is null and command_profile_id is null and review_policy_id is not null and design_version_id is null and decision_id is null and user_correction_id is null)
        or (target_type = 'design_version' and task_id is null and command_profile_id is null and review_policy_id is null and design_version_id is not null and decision_id is null and user_correction_id is null)
        or (target_type = 'decision' and task_id is null and command_profile_id is null and review_policy_id is null and design_version_id is null and decision_id is not null and user_correction_id is null)
        or (target_type = 'user_correction' and task_id is null and command_profile_id is null and review_policy_id is null and design_version_id is null and decision_id is null and user_correction_id is not null)
    ),
    check (
        (item_revision is null and predecessor_handle is null and request_identity is null and receipt_identity is null and current_handle is null)
        or (item_revision is not null and predecessor_handle is not null and request_identity is not null and receipt_identity is not null and current_handle is not null)
    )
);

create trigger if not exists trg_kpt_item_conversion_project_insert
before insert on kpt_item_conversions
for each row
when (new.target_type = 'task' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
  or (new.target_type = 'command_profile' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from command_profiles where id = new.command_profile_id))
  or (new.target_type = 'review_policy' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from review_policies where id = new.review_policy_id))
  or (new.target_type = 'design_version' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from design_versions where id = new.design_version_id))
  or (new.target_type = 'decision' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from decisions where id = new.decision_id))
  or (new.target_type = 'user_correction' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from user_corrections where id = new.user_correction_id))
begin
    select raise(abort, 'kpt item conversion project_id must match target project_id');
end;

create trigger if not exists trg_kpt_item_conversion_project_update
before update of kpt_item_id, target_type, task_id, command_profile_id, review_policy_id, design_version_id, decision_id, user_correction_id on kpt_item_conversions
for each row
when (new.target_type = 'task' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
  or (new.target_type = 'command_profile' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from command_profiles where id = new.command_profile_id))
  or (new.target_type = 'review_policy' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from review_policies where id = new.review_policy_id))
  or (new.target_type = 'design_version' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from design_versions where id = new.design_version_id))
  or (new.target_type = 'decision' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from decisions where id = new.decision_id))
  or (new.target_type = 'user_correction' and (select project_id from kpt_reviews where id = (select kpt_review_id from kpt_items where id = new.kpt_item_id)) != (select project_id from user_corrections where id = new.user_correction_id))
begin
    select raise(abort, 'kpt item conversion project_id must match target project_id');
end;

create trigger if not exists trg_repository_snapshot_referenced_delete
before delete on repository_snapshots
for each row
when exists (select 1 from resume_checks where repository_snapshot_id = old.id)
  or exists (select 1 from command_usages where repository_snapshot_id = old.id)
  or exists (select 1 from validation_runs where repository_snapshot_id = old.id)
  or exists (select 1 from artifacts where repository_snapshot_id = old.id)
  or exists (select 1 from review_plan_targets where repository_snapshot_id = old.id)
  or exists (select 1 from review_runs where repository_snapshot_id = old.id)
  or exists (select 1 from work_record_forks where source_repository_snapshot_id = old.id)
begin
    select raise(abort, 'cannot delete repository snapshot referenced by ledger rows');
end;

create trigger if not exists trg_repository_referenced_delete
before delete on repositories
for each row
when exists (select 1 from repository_snapshots where repository_id = old.id)
  or exists (select 1 from git_commits where repository_id = old.id)
  or exists (select 1 from git_file_changes where repository_id = old.id)
  or exists (select 1 from command_profiles where repository_id = old.id)
  or exists (select 1 from work_record_files where repository_id = old.id)
  or exists (select 1 from implementation_evidence where repository_id = old.id)
begin
    select raise(abort, 'cannot delete repository referenced by ledger rows');
end;

create trigger if not exists trg_git_commit_referenced_delete
before delete on git_commits
for each row
when exists (select 1 from work_record_commits where git_commit_id = old.id)
  or exists (select 1 from work_record_forks where source_git_commit_id = old.id)
  or exists (select 1 from implementation_evidence where git_commit_id = old.id)
begin
    select raise(abort, 'cannot delete git commit referenced by ledger rows');
end;

create trigger if not exists trg_git_file_change_referenced_delete
before delete on git_file_changes
for each row
when exists (select 1 from work_record_files where git_file_change_id = old.id)
  or exists (select 1 from implementation_evidence where git_file_change_id = old.id)
begin
    select raise(abort, 'cannot delete git file change referenced by ledger rows');
end;

create table if not exists correction_completion_inheritance_sources (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    correction_session_id integer not null references correction_sessions(id) on delete cascade,
    correction_application_id integer not null references correction_transition_applications(id) on delete cascade,
    current_requirement_id integer not null references design_requirements(id),
    source_requirement_id integer not null references design_requirements(id),
    source_design_approval_event_id integer not null references authority_events(id),
    source_task_id integer not null references tasks(id),
    source_checklist_item_id integer not null references checklist_items(id),
    source_membership_id integer not null,
    source_membership_assigned_at text not null,
    source_phase_id integer not null references work_phases(id),
    source_phase_closed_event_id integer not null references work_phase_events(id),
    canonical_task_id integer not null references tasks(id),
    canonical_checklist_item_id integer not null references checklist_items(id),
    created_at text not null,
    unique(correction_application_id, current_requirement_id),
    unique(correction_application_id, source_membership_id)
);

create table if not exists correction_completion_inheritance_evidence (
    id integer primary key,
    project_id integer not null references projects(id) on delete cascade,
    inheritance_source_id integer not null references correction_completion_inheritance_sources(id) on delete cascade,
    evidence_kind text not null check (evidence_kind in ('implementation_evidence','coverage_item','validation_gate')),
    source_record_id integer not null,
    canonical_record_id integer,
    validation_run_id integer references validation_runs(id),
    created_at text not null,
    check ((evidence_kind='implementation_evidence' and canonical_record_id is null and validation_run_id is null)
        or (evidence_kind='coverage_item' and canonical_record_id is not null and validation_run_id is null)
        or (evidence_kind='validation_gate' and canonical_record_id is not null and validation_run_id is not null))
);

create unique index if not exists idx_completion_evidence_null_canonical
on correction_completion_inheritance_evidence(inheritance_source_id,evidence_kind,source_record_id)
where canonical_record_id is null;

create unique index if not exists idx_completion_evidence_mapped
on correction_completion_inheritance_evidence(inheritance_source_id,evidence_kind,source_record_id,canonical_record_id)
where canonical_record_id is not null;

create trigger if not exists trg_completion_source_immutable_update
before update on correction_completion_inheritance_sources
begin select raise(abort, 'completion inheritance source is immutable'); end;
create trigger if not exists trg_completion_source_immutable_delete
before delete on correction_completion_inheritance_sources
begin select raise(abort, 'completion inheritance source is immutable'); end;
create trigger if not exists trg_completion_evidence_immutable_update
before update on correction_completion_inheritance_evidence
begin select raise(abort, 'completion inheritance evidence is immutable'); end;
create trigger if not exists trg_completion_evidence_immutable_delete
before delete on correction_completion_inheritance_evidence
begin select raise(abort, 'completion inheritance evidence is immutable'); end;

create view if not exists valid_completion_inheritance_sources as
select source.id
from correction_completion_inheritance_sources source
join design_requirements old_requirement on old_requirement.id=source.source_requirement_id
join design_requirements current_requirement on current_requirement.id=source.current_requirement_id
join design_versions old_version on old_version.id=old_requirement.design_version_id
join design_versions current_version on current_version.id=current_requirement.design_version_id
join tasks old_task on old_task.id=source.source_task_id and old_task.status='closed'
join tasks current_task on current_task.id=source.canonical_task_id and current_task.status='closed'
join checklist_items old_item on old_item.id=source.source_checklist_item_id and old_item.status='closed'
join checklist_items current_item on current_item.id=source.canonical_checklist_item_id and current_item.status='closed'
join work_phases phase on phase.id=source.source_phase_id and phase.status='closed'
join work_phase_events close_event on close_event.id=source.source_phase_closed_event_id
  and close_event.phase_id=phase.id and close_event.event_type='closed' and close_event.created_at=phase.closed_at
where old_version.design_package_id=current_version.design_package_id
  and old_version.status in ('approved','superseded')
  and old_version.approved_at is not null
  and old_version.approved_by_authority_event_id=source.source_design_approval_event_id
  and exists(select 1 from authority_events approval
      where approval.id=old_version.approved_by_authority_event_id
        and approval.project_id=source.project_id and approval.status='active')
  and old_version.version_number<current_version.version_number
  and old_version.version_number=(
      select max(candidate_version.version_number)
      from design_versions candidate_version
      where candidate_version.design_package_id=current_version.design_package_id
        and candidate_version.version_number<current_version.version_number
        and candidate_version.status in ('approved','superseded')
        and candidate_version.approved_at is not null
        and candidate_version.approved_by_authority_event_id is not null
        and exists(select 1 from authority_events candidate_approval
            where candidate_approval.id=candidate_version.approved_by_authority_event_id
              and candidate_approval.project_id=source.project_id and candidate_approval.status='active')
        and (select count(*) from design_requirements candidate_requirement
            where candidate_requirement.design_version_id=candidate_version.id
              and candidate_requirement.requirement_key=current_requirement.requirement_key)=1)
  and old_requirement.requirement_key=current_requirement.requirement_key
  and old_requirement.revision=current_requirement.revision
  and old_requirement.requirement_hash=current_requirement.requirement_hash
  and old_requirement.required_surfaces is current_requirement.required_surfaces
  and not exists(
      select 1 from validation_gate_template_requirements current_map
      join validation_gate_templates current_template on current_template.id=current_map.validation_gate_template_id
        and current_template.status='active'
      where current_map.design_requirement_id=current_requirement.id
        and not exists(
          select 1 from validation_gate_template_requirements old_map
          join validation_gate_templates old_template on old_template.id=old_map.validation_gate_template_id
            and old_template.status='active'
          where old_map.design_requirement_id=old_requirement.id
            and old_template.gate_key=current_template.gate_key
            and old_template.gate_hash=current_template.gate_hash))
  and not exists(
      select 1 from validation_gate_template_requirements old_map
      join validation_gate_templates old_template on old_template.id=old_map.validation_gate_template_id
        and old_template.status='active'
      where old_map.design_requirement_id=old_requirement.id
        and not exists(
          select 1 from validation_gate_template_requirements current_map
          join validation_gate_templates current_template on current_template.id=current_map.validation_gate_template_id
            and current_template.status='active'
          where current_map.design_requirement_id=current_requirement.id
            and current_template.gate_key=old_template.gate_key
            and current_template.gate_hash=old_template.gate_hash))
  and exists(select 1 from implementation_evidence evidence
      where evidence.task_id=source.source_task_id and evidence.design_requirement_id=source.source_requirement_id
        and evidence.created_at<=phase.closed_at)
  and (select count(*) from correction_completion_inheritance_evidence mapped
       where mapped.inheritance_source_id=source.id and mapped.evidence_kind='implementation_evidence')
      =(select count(*) from implementation_evidence evidence
        where evidence.task_id=source.source_task_id and evidence.design_requirement_id=source.source_requirement_id
          and evidence.created_at<=phase.closed_at)
  and (select count(*) from correction_completion_inheritance_evidence mapped
       join coverage_items old_coverage on old_coverage.id=mapped.source_record_id and old_coverage.status='covered'
       join coverage_items current_coverage on current_coverage.id=mapped.canonical_record_id and current_coverage.status='covered'
       where mapped.inheritance_source_id=source.id and mapped.evidence_kind='coverage_item'
         and old_coverage.task_id=source.source_task_id and old_coverage.design_requirement_id=source.source_requirement_id
         and current_coverage.task_id=source.canonical_task_id and current_coverage.design_requirement_id=source.current_requirement_id)=1
  and (select count(*) from correction_completion_inheritance_evidence mapped
       join validation_gates old_gate on old_gate.id=mapped.source_record_id
       join validation_gates current_gate on current_gate.id=mapped.canonical_record_id and current_gate.status='closed'
       join validation_gate_templates old_template on old_template.id=old_gate.template_id
       join validation_gate_templates current_template on current_template.id=current_gate.template_id
       join validation_runs run on run.id=mapped.validation_run_id and run.validation_gate_id=old_gate.id and run.result='pass'
         and not exists(select 1 from validation_link_retirements retirement where retirement.validation_run_id=run.id)
       left join command_usages usage on usage.id=run.command_usage_id and usage.result='pass'
       where mapped.inheritance_source_id=source.id and mapped.evidence_kind='validation_gate'
         and old_template.gate_key=current_template.gate_key and old_template.gate_hash=current_template.gate_hash
         and (old_gate.command is null or (run.command is old_gate.command
           and usage.project_id=source.project_id and usage.work_unit_id=old_task.work_unit_id
           and usage.command=old_gate.command))
         and run.created_at<=phase.closed_at
         and run.id=(select max(candidate.id) from validation_runs candidate where candidate.validation_gate_id=old_gate.id and candidate.created_at<=phase.closed_at and not exists(select 1 from validation_link_retirements retirement where retirement.validation_run_id=candidate.id)))
      =(select count(*) from validation_gate_template_requirements mapping
        join validation_gate_templates template on template.id=mapping.validation_gate_template_id
        where mapping.design_requirement_id=source.current_requirement_id and template.status='active');
"#;
