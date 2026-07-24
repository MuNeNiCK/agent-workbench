pub(super) const SQL: &str = r#"
create trigger if not exists trg_review_plan_target_project_insert
before insert on review_plan_targets
for each row
when (new.target_type = 'design_version' and (select project_id from review_plans where id = new.review_plan_id) != (select project_id from design_versions where id = new.design_version_id))
  or (new.target_type = 'design_requirement' and (select project_id from review_plans where id = new.review_plan_id) != (select project_id from design_requirements where id = new.design_requirement_id))
  or (new.target_type = 'task' and (select project_id from review_plans where id = new.review_plan_id) != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
  or (new.target_type = 'work_unit' and (select project_id from review_plans where id = new.review_plan_id) != (select project_id from work_units where id = new.work_unit_id))
  or (new.target_type = 'repository_snapshot' and (
      not exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
      or (select project_id from review_plans where id = new.review_plan_id) != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
  ))
begin
    select raise(abort, 'review plan target project_id must match review plan project_id');
end;

create trigger if not exists trg_review_plan_target_project_update
before update of review_plan_id, target_type, design_version_id, design_requirement_id, task_id, work_unit_id, repository_snapshot_id on review_plan_targets
for each row
when (new.target_type = 'design_version' and (select project_id from review_plans where id = new.review_plan_id) != (select project_id from design_versions where id = new.design_version_id))
  or (new.target_type = 'design_requirement' and (select project_id from review_plans where id = new.review_plan_id) != (select project_id from design_requirements where id = new.design_requirement_id))
  or (new.target_type = 'task' and (select project_id from review_plans where id = new.review_plan_id) != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
  or (new.target_type = 'work_unit' and (select project_id from review_plans where id = new.review_plan_id) != (select project_id from work_units where id = new.work_unit_id))
  or (new.target_type = 'repository_snapshot' and (
      not exists (select 1 from repository_snapshots where id = new.repository_snapshot_id)
      or (select project_id from review_plans where id = new.review_plan_id) != (
          select r.project_id
          from repository_snapshots s
          join repositories r on r.id = s.repository_id
          where s.id = new.repository_snapshot_id
      )
  ))
begin
    select raise(abort, 'review plan target project_id must match review plan project_id');
end;

create trigger if not exists trg_review_plan_target_referenced_update
before update of review_plan_id, target_type, design_version_id, design_requirement_id, task_id, work_unit_id, repository_snapshot_id, file_path, symbol on review_plan_targets
for each row
when exists (
    select 1
    from review_runs r
    where r.review_plan_id = old.review_plan_id
      and (
          (r.target_type = 'design_version' and old.target_type = 'design_version' and r.design_version_id = old.design_version_id)
          or (r.target_type = 'design_requirement' and old.target_type = 'design_requirement' and r.design_requirement_id = old.design_requirement_id)
          or (r.target_type = 'task' and old.target_type = 'task' and r.task_id = old.task_id)
          or (r.target_type = 'work_unit' and old.target_type = 'work_unit' and r.work_unit_id = old.work_unit_id)
          or (r.target_type = 'repository_snapshot' and old.target_type = 'repository_snapshot' and r.repository_snapshot_id = old.repository_snapshot_id)
          or (r.target_type = 'file' and old.target_type = 'file' and r.target_ref = old.file_path)
          or (r.target_type = 'symbol' and old.target_type = 'symbol' and r.target_ref = old.symbol)
      )
)
begin
    select raise(abort, 'cannot update review plan target referenced by review runs');
end;

create trigger if not exists trg_review_plan_target_referenced_delete
before delete on review_plan_targets
for each row
when exists (
    select 1
    from review_runs r
    where r.review_plan_id = old.review_plan_id
      and (
          (r.target_type = 'design_version' and old.target_type = 'design_version' and r.design_version_id = old.design_version_id)
          or (r.target_type = 'design_requirement' and old.target_type = 'design_requirement' and r.design_requirement_id = old.design_requirement_id)
          or (r.target_type = 'task' and old.target_type = 'task' and r.task_id = old.task_id)
          or (r.target_type = 'work_unit' and old.target_type = 'work_unit' and r.work_unit_id = old.work_unit_id)
          or (r.target_type = 'repository_snapshot' and old.target_type = 'repository_snapshot' and r.repository_snapshot_id = old.repository_snapshot_id)
          or (r.target_type = 'file' and old.target_type = 'file' and r.target_ref = old.file_path)
          or (r.target_type = 'symbol' and old.target_type = 'symbol' and r.target_ref = old.symbol)
      )
)
begin
    select raise(abort, 'cannot delete review plan target referenced by review runs');
end;

create trigger if not exists trg_finding_project_insert
before insert on findings
for each row
when new.project_id != (select project_id from review_runs where id = new.review_run_id)
  or (new.design_requirement_id is not null and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id))
  or (new.task_id is not null and new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
begin
    select raise(abort, 'finding project_id must match referenced rows');
end;

create trigger if not exists trg_finding_project_update
before update of project_id, review_run_id, design_requirement_id, task_id on findings
for each row
when new.project_id != (select project_id from review_runs where id = new.review_run_id)
  or (new.design_requirement_id is not null and new.project_id != (select project_id from design_requirements where id = new.design_requirement_id))
  or (new.task_id is not null and new.project_id != coalesce(
      (select project_id from work_units where id = (select work_unit_id from tasks where id = new.task_id)),
      (select id from projects order by id limit 1)
  ))
begin
    select raise(abort, 'finding project_id must match referenced rows');
end;

create trigger if not exists trg_finding_target_project_insert
before insert on finding_targets
for each row
when new.project_id != (select project_id from findings where id=new.finding_id)
  or (new.design_requirement_id is not null
      and new.project_id != (select project_id from design_requirements where id=new.design_requirement_id))
  or (new.task_id is not null and new.project_id != coalesce((
      select work.project_id from tasks task
      join work_units work on work.id=task.work_unit_id
      where task.id=new.task_id
  ), (select id from projects order by id limit 1)))
begin
    select raise(abort, 'finding target project_id must match referenced rows');
end;

create trigger if not exists trg_review_result_draft_item_target_project_insert
before insert on review_result_draft_item_targets
for each row
when new.project_id != (select project_id from review_result_draft_items where id=new.draft_item_id)
  or (new.design_requirement_id is not null
      and new.project_id != (select project_id from design_requirements where id=new.design_requirement_id))
  or (new.task_id is not null and new.project_id != coalesce((
      select work.project_id from tasks task
      join work_units work on work.id=task.work_unit_id
      where task.id=new.task_id
  ), (select id from projects order by id limit 1)))
begin
    select raise(abort, 'draft finding target project_id must match referenced rows');
end;

create trigger if not exists trg_finding_target_seal_insert
before insert on finding_target_seals
for each row
when new.project_id != (select project_id from findings where id=new.finding_id)
  or new.target_count != (
      select count(*) from finding_targets target where target.finding_id=new.finding_id
  )
begin
    select raise(abort, 'finding target seal must match the complete target set');
end;

create trigger if not exists trg_review_result_draft_item_target_seal_insert
before insert on review_result_draft_item_target_seals
for each row
when new.project_id != (
      select project_id from review_result_draft_items where id=new.draft_item_id
    )
  or new.target_count != (
      select count(*) from review_result_draft_item_targets target
      where target.draft_item_id=new.draft_item_id
  )
begin
    select raise(abort, 'draft finding target seal must match the complete target set');
end;

create trigger if not exists trg_finding_clean_run_insert
before insert on findings
for each row
when (select clean_run from review_runs where id = new.review_run_id) = 1
begin
    select raise(abort, 'cannot attach finding to clean review run');
end;

create trigger if not exists trg_finding_clean_run_update
before update of review_run_id on findings
for each row
when (select clean_run from review_runs where id = new.review_run_id) = 1
begin
    select raise(abort, 'cannot attach finding to clean review run');
end;

create trigger if not exists trg_finding_resume_policy_insert
before insert on findings
for each row
when exists (
    select 1
    from review_runs r
    join review_plans p on p.id = r.review_plan_id
    join review_policies rp on rp.id = p.review_policy_id
    where r.id = new.review_run_id
      and r.run_type = 'resume'
      and rp.allow_new_findings_in_resume = 0
)
begin
    select raise(abort, 'new findings are disabled for resume review by policy');
end;

create trigger if not exists trg_finding_resume_policy_update
before update of review_run_id on findings
for each row
when exists (
    select 1
    from review_runs r
    join review_plans p on p.id = r.review_plan_id
    join review_policies rp on rp.id = p.review_policy_id
    where r.id = new.review_run_id
      and r.run_type = 'resume'
      and rp.allow_new_findings_in_resume = 0
)
begin
    select raise(abort, 'new findings are disabled for resume review by policy');
end;

create trigger if not exists trg_closure_project_insert
before insert on closures
for each row
when new.project_id != (select project_id from findings where id = new.finding_id)
begin
    select raise(abort, 'closure project_id must match finding project_id');
end;

create trigger if not exists trg_closure_project_update
before update of project_id, finding_id on closures
for each row
when new.project_id != (select project_id from findings where id = new.finding_id)
begin
    select raise(abort, 'closure project_id must match finding project_id');
end;

create trigger if not exists trg_finding_verification_project_insert
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
          verifier_plan.design_version_id is source_plan.design_version_id
          or exists(
            select 1 from design_versions source_design
            join design_versions verifier_design
              on verifier_design.design_package_id=source_design.design_package_id
            where source_design.id=source_plan.design_version_id
              and verifier_design.id=verifier_plan.design_version_id
              and verifier_design.version_number>=source_design.version_number
              and verifier_design.status='approved'
          )
        )
        and coalesce(verifier_plan.scope, '') = coalesce(source_plan.scope, '')
  )
begin
    select raise(abort, 'finding verification project_id must match referenced rows');
end;

create trigger if not exists trg_finding_verification_project_update
before update of project_id, review_run_id, finding_id, closure_id on finding_verifications
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
          verifier_plan.design_version_id is source_plan.design_version_id
          or exists(
            select 1 from design_versions source_design
            join design_versions verifier_design
              on verifier_design.design_package_id=source_design.design_package_id
            where source_design.id=source_plan.design_version_id
              and verifier_design.id=verifier_plan.design_version_id
              and verifier_design.version_number>=source_design.version_number
              and verifier_design.status='approved'
          )
        )
        and coalesce(verifier_plan.scope, '') = coalesce(source_plan.scope, '')
  )
begin
    select raise(abort, 'finding verification project_id must match referenced rows');
end;

create trigger if not exists trg_acceptance_general_project_insert
before insert on acceptance_records
for each row
when (new.target_type = 'finding' and new.project_id != (select project_id from findings where id = new.finding_id))
  or (new.target_type = 'validation_gate' and new.project_id != (select project_id from validation_gates where id = new.validation_gate_id))
  or (new.target_type = 'validation_run' and new.project_id != (select project_id from validation_runs where id = new.validation_run_id))
  or (new.target_type = 'repository_state_classification' and new.project_id != (
      select r.project_id
      from repository_state_classifications c
      join repository_snapshots s on s.id = c.repository_snapshot_id
      join repositories r on r.id = s.repository_id
      where c.id = new.repository_state_classification_id
  ))
  or (new.target_type = 'repository_snapshot_comparison' and (
      new.project_id != (
          select base_repo.project_id
          from repository_snapshot_comparisons c
          join repository_snapshots base on base.id = c.base_repository_snapshot_id
          join repositories base_repo on base_repo.id = base.repository_id
          where c.id = new.repository_snapshot_comparison_id
      )
      or new.project_id != (
          select current_repo.project_id
          from repository_snapshot_comparisons c
          join repository_snapshots current on current.id = c.current_repository_snapshot_id
          join repositories current_repo on current_repo.id = current.repository_id
          where c.id = new.repository_snapshot_comparison_id
      )
  ))
  or (new.target_type = 'review_plan' and new.project_id != (select project_id from review_plans where id = new.review_plan_id))
  or (new.target_type = 'checklist_item' and new.project_id != (select project_id from checklist_items where id = new.checklist_item_id))
  or (new.target_type = 'command_profile' and new.project_id != (select project_id from command_profiles where id = new.command_profile_id))
  or (new.target_type = 'command_usage' and new.project_id != (select project_id from command_usages where id = new.command_usage_id))
  or (new.target_type = 'command_deviation' and new.project_id != (
      select p.project_id
      from command_deviations d
      join command_profiles p on p.id = d.command_profile_id
      where d.id = new.command_deviation_id
  ))
  or (new.target_type = 'rule_binding' and new.project_id != (
      select project_id from rule_bindings where id = new.rule_binding_id
  ))
begin
    select raise(abort, 'acceptance project_id must match general target project_id');
end;

create trigger if not exists trg_acceptance_general_project_update
before update of project_id, target_type, finding_id, validation_gate_id, validation_run_id, repository_state_classification_id, repository_snapshot_comparison_id, review_plan_id, checklist_item_id, command_profile_id, command_usage_id, command_deviation_id, rule_binding_id on acceptance_records
for each row
when (new.target_type = 'finding' and new.project_id != (select project_id from findings where id = new.finding_id))
  or (new.target_type = 'validation_gate' and new.project_id != (select project_id from validation_gates where id = new.validation_gate_id))
  or (new.target_type = 'validation_run' and new.project_id != (select project_id from validation_runs where id = new.validation_run_id))
  or (new.target_type = 'repository_state_classification' and new.project_id != (
      select r.project_id
      from repository_state_classifications c
      join repository_snapshots s on s.id = c.repository_snapshot_id
      join repositories r on r.id = s.repository_id
      where c.id = new.repository_state_classification_id
  ))
  or (new.target_type = 'repository_snapshot_comparison' and (
      new.project_id != (
          select base_repo.project_id
          from repository_snapshot_comparisons c
          join repository_snapshots base on base.id = c.base_repository_snapshot_id
          join repositories base_repo on base_repo.id = base.repository_id
          where c.id = new.repository_snapshot_comparison_id
      )
      or new.project_id != (
          select current_repo.project_id
          from repository_snapshot_comparisons c
          join repository_snapshots current on current.id = c.current_repository_snapshot_id
          join repositories current_repo on current_repo.id = current.repository_id
          where c.id = new.repository_snapshot_comparison_id
      )
  ))
  or (new.target_type = 'review_plan' and new.project_id != (select project_id from review_plans where id = new.review_plan_id))
  or (new.target_type = 'checklist_item' and new.project_id != (select project_id from checklist_items where id = new.checklist_item_id))
  or (new.target_type = 'command_profile' and new.project_id != (select project_id from command_profiles where id = new.command_profile_id))
  or (new.target_type = 'command_usage' and new.project_id != (select project_id from command_usages where id = new.command_usage_id))
  or (new.target_type = 'command_deviation' and new.project_id != (
      select p.project_id
      from command_deviations d
      join command_profiles p on p.id = d.command_profile_id
      where d.id = new.command_deviation_id
  ))
  or (new.target_type = 'rule_binding' and new.project_id != (
      select project_id from rule_bindings where id = new.rule_binding_id
  ))
begin
    select raise(abort, 'acceptance project_id must match general target project_id');
end;
"#;
