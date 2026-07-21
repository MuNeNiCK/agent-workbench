use anyhow::Result;
use rusqlite::Connection;

use super::project::*;

pub(crate) const EMPTY_CORRECTION_DECOMPOSITION_TASK_MEMBERSHIP_VIEW_SQL: &str = r#"
create view if not exists correction_decomposition_task_memberships as
select cast(null as integer) correction_application_id,
       cast(null as integer) task_id
where 0;
"#;

pub(crate) fn install_correction_decomposition_task_membership_view(
    conn: &Connection,
) -> Result<()> {
    conn.execute_batch(
        r#"
        drop view if exists correction_decomposition_task_memberships;
        create view correction_decomposition_task_memberships as
        select transition.id correction_application_id,application.task_id
        from correction_transition_applications transition
        join decomposition_plans plan
          on transition.result_ref='decomposition-plan:'||plan.id
        join decomposition_applications application
          on application.decomposition_plan_id=plan.id;
        "#,
    )?;
    Ok(())
}

pub(super) fn ensure_closure_lifecycle_schema(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "closures")? {
        return Ok(());
    }
    ensure_column(
        conn,
        "closures",
        "status",
        "text not null default 'registered'",
    )?;
    ensure_column(conn, "closures", "superseded_by_closure_id", "integer")?;
    ensure_column(conn, "closures", "superseded_at", "text")?;
    ensure_column(conn, "closures", "supersession_reason", "text")?;
    ensure_column(
        conn,
        "closures",
        "superseded_by_authority_event_id",
        "integer",
    )?;
    ensure_column(conn, "review_runs", "finding_fix_result", "text")?;
    ensure_column(
        conn,
        "finding_verifications",
        "closure_attempt_id",
        "integer",
    )?;
    ensure_column(
        conn,
        "review_plans",
        "fresh_review_after_run_id",
        "integer not null default 0",
    )?;
    conn.execute_batch(
        r#"
        drop trigger if exists trg_remediation_binding_insert;
        drop trigger if exists trg_remediation_binding_immutable_update;
        drop trigger if exists trg_remediation_binding_immutable_delete;
        drop trigger if exists trg_remediation_recovery_epoch_insert;
        drop trigger if exists trg_remediation_recovery_epoch_immutable_update;
        drop trigger if exists trg_remediation_recovery_epoch_immutable_delete;
        drop trigger if exists trg_correction_session_links_insert;
        drop trigger if exists trg_correction_session_links_update;
        drop trigger if exists trg_correction_session_status_update;
        drop trigger if exists trg_correction_session_immutable_delete;
        drop trigger if exists trg_correction_token_links_insert;
        drop trigger if exists trg_correction_token_links_update;
        drop trigger if exists trg_correction_token_status_update;
        drop trigger if exists trg_correction_token_immutable_delete;
        drop trigger if exists trg_correction_application_links_insert;
        drop trigger if exists trg_correction_application_links_update;
        drop trigger if exists trg_correction_application_immutable_delete;
        drop trigger if exists trg_correction_alias_links_insert;
        drop trigger if exists trg_correction_alias_immutable_update;
        drop trigger if exists trg_correction_alias_immutable_delete;
        "#,
    )?;
    conn.execute_batch(EMPTY_CORRECTION_DECOMPOSITION_TASK_MEMBERSHIP_VIEW_SQL)?;
    let lifecycle_schema = r#"
        create table if not exists closure_attempts (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            closure_id integer not null references closures(id) on delete cascade,
            attempt_number integer not null,
            implementation_evidence text not null,
            tests_or_gates text not null,
            closed_by_commit text,
            review_run_high_watermark integer not null default 0,
            result text check (result in ('verified', 'not_fixed', 'needs_evidence', 'superseded')),
            created_at text not null,
            resolved_at text,
            unique(closure_id, attempt_number),
            unique(project_id, id)
        );
        create unique index if not exists idx_closure_attempt_project_id on closure_attempts(project_id,id);

        create table if not exists finding_remediation_bindings (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            finding_id integer not null references findings(id) on delete cascade,
            closure_id integer not null references closures(id) on delete cascade,
            work_unit_id integer not null references work_units(id) on delete cascade,
            work_unit_activation_id integer not null references work_unit_activations(id) on delete cascade,
            created_at text not null,
            unique(finding_id, closure_id, work_unit_activation_id)
        );

        create table if not exists finding_remediation_recovery_epochs (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            finding_id integer not null references findings(id) on delete cascade,
            closure_id integer not null references closures(id) on delete cascade,
            work_unit_id integer not null references work_units(id) on delete cascade,
            work_unit_activation_id integer not null references work_unit_activations(id) on delete cascade,
            dependency_id integer not null references work_unit_dependencies(id) on delete cascade,
            reopened_event_id integer not null references work_unit_events(id) on delete cascade,
            authority_event_id integer not null references authority_events(id),
            created_at text not null,
            unique(finding_id, closure_id, work_unit_activation_id, dependency_id)
        );

        create table if not exists correction_sessions (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            finding_id integer not null references findings(id) on delete cascade,
            closure_id integer not null references closures(id) on delete cascade,
            status text not null check (status in ('active', 'superseded', 'completed')),
            created_at text not null,
            completed_at text,
            unique(closure_id, status)
        );

        create table if not exists correction_tokens (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            closure_id integer not null references closures(id) on delete cascade,
            token_ordinal integer not null,
            token_kind text not null check (token_kind in ('file', 'transition')),
            operation text not null,
            target text not null,
            pre_state text,
            pre_hash text,
            status text not null default 'pending' check (status in ('pending', 'applied', 'superseded')),
            created_at text not null,
            applied_at text,
            unique(closure_id, token_ordinal)
        );

        create table if not exists correction_transition_applications (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            correction_session_id integer not null references correction_sessions(id) on delete cascade,
            correction_token_id integer not null references correction_tokens(id) on delete cascade,
            authority_event_id integer references authority_events(id),
            evidence_ref text,
            before_state text not null,
            after_state text not null,
            result_ref text not null,
            created_at text not null,
            unique(correction_token_id)
        );

        create table if not exists correction_transition_aliases (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            correction_session_id integer not null references correction_sessions(id) on delete cascade,
            correction_application_id integer not null references correction_transition_applications(id) on delete cascade,
            alias text not null,
            record_type text not null,
            record_id integer not null,
            created_at text not null,
            unique(correction_session_id, alias)
        );

        create table if not exists correction_application_identity_links (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            correction_session_id integer not null references correction_sessions(id) on delete cascade,
            correction_application_id integer not null references correction_transition_applications(id) on delete cascade,
            link_kind text not null check (link_kind in (
                'adopted', 'created', 'superseded', 'updated',
                'membership_removed', 'membership_assigned', 'completion_source'
            )),
            record_type text not null check (record_type in (
                'task', 'checklist', 'task_derivation', 'checklist_item',
                'validation_gate', 'coverage_item', 'phase', 'phase_dependency',
                'acceptance_record', 'phase_membership'
            )),
            record_id integer not null,
            created_at text not null,
            check (
                (record_type='phase_membership' and link_kind in ('membership_removed','membership_assigned'))
                or (record_type!='phase_membership' and link_kind not in ('membership_removed','membership_assigned'))
            ),
            unique(correction_application_id, record_type, record_id)
        );

        create trigger if not exists trg_correction_identity_link_insert
        before insert on correction_application_identity_links
        for each row when
            new.project_id != (select project_id from correction_transition_applications where id=new.correction_application_id)
            or new.correction_session_id != (select correction_session_id from correction_transition_applications where id=new.correction_application_id)
            or new.project_id != (select project_id from correction_sessions where id=new.correction_session_id)
            or (new.record_type='phase_membership' and new.link_kind='membership_removed' and (
                ('|'||(select substr(before_state,
                    instr(before_state,'memberships=[')+13,
                    instr(before_state,'];checklists=')-(instr(before_state,'memberships=[')+13))
                  from correction_transition_applications where id=new.correction_application_id)||'|')
                not like '%|'||new.record_id||':%'
                or ('|'||(select substr(after_state,
                    instr(after_state,'memberships=[')+13,
                    instr(after_state,'];checklists=')-(instr(after_state,'memberships=[')+13))
                  from correction_transition_applications where id=new.correction_application_id)||'|')
                like '%|'||new.record_id||':%'))
            or (new.record_type='phase_membership' and new.link_kind='membership_assigned' and (
                ('|'||(select substr(after_state,
                    instr(after_state,'memberships=[')+13,
                    instr(after_state,'];checklists=')-(instr(after_state,'memberships=[')+13))
                  from correction_transition_applications where id=new.correction_application_id)||'|')
                not like '%|'||new.record_id||':%'
                or ('|'||(select substr(before_state,
                    instr(before_state,'memberships=[')+13,
                    instr(before_state,'];checklists=')-(instr(before_state,'memberships=[')+13))
                  from correction_transition_applications where id=new.correction_application_id)||'|')
                like '%|'||new.record_id||':%'))
            or (new.record_type!='phase_membership' and not (
                (new.record_type='task' and exists(select 1 from tasks t join work_units w on w.id=t.work_unit_id where t.id=new.record_id and w.project_id=new.project_id))
                or (new.record_type='checklist' and exists(select 1 from checklists where id=new.record_id and project_id=new.project_id))
                or (new.record_type='task_derivation' and exists(select 1 from task_derivations where id=new.record_id and project_id=new.project_id))
                or (new.record_type='checklist_item' and exists(select 1 from checklist_items where id=new.record_id and project_id=new.project_id))
                or (new.record_type='validation_gate' and exists(select 1 from validation_gates where id=new.record_id and project_id=new.project_id))
                or (new.record_type='coverage_item' and exists(select 1 from coverage_items where id=new.record_id and project_id=new.project_id))
                or (new.record_type='phase' and exists(select 1 from work_phases where id=new.record_id and project_id=new.project_id))
                or (new.record_type='phase_dependency' and exists(select 1 from work_phase_dependencies d join work_phases p on p.id=d.from_phase_id where d.id=new.record_id and p.project_id=new.project_id))
                or (new.record_type='acceptance_record' and exists(select 1 from acceptance_records where id=new.record_id and project_id=new.project_id))
            ))
            or (new.record_type!='phase_membership' and new.link_kind in ('adopted','created') and not exists(
                select 1 from correction_transition_aliases alias
                where alias.correction_application_id=new.correction_application_id
                  and alias.record_type=new.record_type and alias.record_id=new.record_id
            ))
            or (new.record_type!='phase_membership' and new.link_kind='superseded' and not (
                (new.record_type='task' and exists(
                    select 1 from correction_transition_aliases alias
                    where alias.correction_application_id=new.correction_application_id
                      and alias.record_type='task' and alias.record_id=new.record_id
                      and alias.alias='@superseded-task/'||new.record_id
                ))
                or (new.record_type='checklist'
                    and ('|'||(select substr(before_state, instr(before_state,'checklists=[')+12, instr(before_state,'];derivations=')-(instr(before_state,'checklists=[')+12)) from correction_transition_applications where id=new.correction_application_id)||'|') like '%|'||new.record_id||':active|%'
                    and ('|'||(select substr(after_state, instr(after_state,'checklists=[')+12, instr(after_state,'];derivations=')-(instr(after_state,'checklists=[')+12)) from correction_transition_applications where id=new.correction_application_id)||'|') like '%|'||new.record_id||':closed|%')
                or (new.record_type='task_derivation'
                    and ('|'||(select substr(before_state, instr(before_state,'derivations=[')+13, instr(before_state,'];items=')-(instr(before_state,'derivations=[')+13)) from correction_transition_applications where id=new.correction_application_id)||'|') like '%|'||new.record_id||':active|%'
                    and ('|'||(select substr(after_state, instr(after_state,'derivations=[')+13, instr(after_state,'];items=')-(instr(after_state,'derivations=[')+13)) from correction_transition_applications where id=new.correction_application_id)||'|') like '%|'||new.record_id||':closed|%')
                or (new.record_type='checklist_item'
                    and ('|'||(select substr(before_state, instr(before_state,'items=[')+7, instr(before_state,'];gates=')-(instr(before_state,'items=[')+7)) from correction_transition_applications where id=new.correction_application_id)||'|') like '%|'||new.record_id||':%'
                    and ('|'||(select substr(after_state, instr(after_state,'items=[')+7, instr(after_state,'];gates=')-(instr(after_state,'items=[')+7)) from correction_transition_applications where id=new.correction_application_id)||'|') like '%|'||new.record_id||':closed|%')
                or (new.record_type='validation_gate'
                    and ('|'||(select substr(before_state, instr(before_state,'gates=[')+7, instr(before_state,'];coverage=')-(instr(before_state,'gates=[')+7)) from correction_transition_applications where id=new.correction_application_id)||'|') like '%|'||new.record_id||':active|%'
                    and ('|'||(select substr(after_state, instr(after_state,'gates=[')+7, instr(after_state,'];coverage=')-(instr(after_state,'gates=[')+7)) from correction_transition_applications where id=new.correction_application_id)||'|') like '%|'||new.record_id||':closed|%')
                or (new.record_type='coverage_item'
                    and ('|'||(select substr(before_state, instr(before_state,'coverage=[')+10, instr(before_state,'];phases=')-(instr(before_state,'coverage=[')+10)) from correction_transition_applications where id=new.correction_application_id)||'|') like '%|'||new.record_id||':%'
                    and ('|'||(select substr(after_state, instr(after_state,'coverage=[')+10, instr(after_state,'];phases=')-(instr(after_state,'coverage=[')+10)) from correction_transition_applications where id=new.correction_application_id)||'|') like '%|'||new.record_id||':stale|%')
            ))
            or (new.record_type!='phase_membership' and new.link_kind='updated' and not exists(
                select 1 from correction_transition_aliases alias
                where alias.correction_application_id=new.correction_application_id
                  and alias.record_type=new.record_type and alias.record_id=new.record_id
                  and alias.alias like '@accepted-%'
            ) and not exists(
                select 1 from correction_completion_inheritance_sources source
                where source.correction_application_id=new.correction_application_id
                  and ((new.record_type='task' and source.canonical_task_id=new.record_id)
                    or (new.record_type='checklist_item' and source.canonical_checklist_item_id=new.record_id))
            ) and not exists(
                select 1 from correction_completion_inheritance_evidence evidence
                join correction_completion_inheritance_sources source on source.id=evidence.inheritance_source_id
                where source.correction_application_id=new.correction_application_id
                  and evidence.canonical_record_id=new.record_id
                  and ((new.record_type='validation_gate' and evidence.evidence_kind='validation_gate')
                    or (new.record_type='coverage_item' and evidence.evidence_kind='coverage_item'))
            ))
            or (new.link_kind='completion_source' and not exists(
                select 1 from correction_completion_inheritance_sources source
                where source.correction_application_id=new.correction_application_id
                  and ((new.record_type='task' and source.source_task_id=new.record_id)
                    or (new.record_type='phase' and source.source_phase_id=new.record_id))
            ))
        begin select raise(abort, 'invalid correction application identity link ownership'); end;

        create trigger if not exists trg_correction_identity_link_immutable_update
        before update on correction_application_identity_links
        begin select raise(abort, 'correction application identity links are immutable'); end;

        create trigger if not exists trg_correction_identity_link_immutable_delete
        before delete on correction_application_identity_links
        begin select raise(abort, 'correction application identity links are immutable'); end;

        create trigger if not exists trg_remediation_binding_insert
        before insert on finding_remediation_bindings
        for each row when
            new.project_id != (select project_id from findings where id = new.finding_id)
            or new.project_id != (select project_id from closures where id = new.closure_id)
            or new.project_id != (select project_id from work_units where id = new.work_unit_id)
            or new.project_id != (select project_id from work_unit_activations where id = new.work_unit_activation_id)
            or new.finding_id != (select finding_id from closures where id = new.closure_id)
            or new.work_unit_id != (select work_unit_id from work_unit_activations where id = new.work_unit_activation_id)
            or new.work_unit_id != (
                select p.work_unit_id
                from findings f
                join review_runs r on r.id = f.review_run_id
                join review_plans p on p.id = r.review_plan_id
                where f.id = new.finding_id
            )
            or not exists (
                select 1
                from findings f
                join closures c on c.id = new.closure_id and c.finding_id = f.id
                join review_runs r on r.id = f.review_run_id
                join review_plans p on p.id = r.review_plan_id
                join work_units w on w.id = p.work_unit_id
                join work_unit_activations a on a.id = new.work_unit_activation_id
                where f.id = new.finding_id
                  and f.status = 'open' and f.classification = 'valid'
                  and c.status = 'registered'
                  and p.required = 1 and p.stage = 'close-ready'
                  and p.review_type in ('implementation_review', 'design_implementation_diff')
                  and p.status not in ('exhausted', 'needs_user_decision')
                  and w.id = new.work_unit_id and w.status = 'open'
                  and a.work_unit_id = new.work_unit_id and a.status = 'active'
                  and not exists (
                      select 1 from acceptance_records ar
                      where ar.target_type = 'finding' and ar.finding_id = f.id
                        and ar.status = 'approved'
                  )
            )
        begin
            select raise(abort, 'invalid finding remediation binding links');
        end;

        create trigger if not exists trg_remediation_binding_immutable_update
        before update on finding_remediation_bindings
        begin select raise(abort, 'finding remediation bindings are immutable'); end;

        create trigger if not exists trg_remediation_binding_immutable_delete
        before delete on finding_remediation_bindings
        begin select raise(abort, 'finding remediation bindings are immutable'); end;

        create trigger if not exists trg_remediation_recovery_epoch_insert
        before insert on finding_remediation_recovery_epochs
        for each row when
            new.project_id != (select project_id from findings where id = new.finding_id)
            or new.project_id != (select project_id from closures where id = new.closure_id)
            or new.finding_id != (select finding_id from closures where id = new.closure_id)
            or new.work_unit_id != (select work_unit_id from work_unit_activations where id = new.work_unit_activation_id)
            or new.work_unit_id != (select work_unit_id from work_unit_dependencies where id = new.dependency_id)
            or new.work_unit_id != (select depends_on_work_unit_id from work_unit_dependencies where id = new.dependency_id)
            or new.work_unit_activation_id != (select work_unit_activation_id from work_unit_events where id = new.reopened_event_id)
            or 'reopened' != (select event_type from work_unit_events where id = new.reopened_event_id)
            or new.project_id != (select project_id from authority_events where id = new.authority_event_id)
            or not exists (
                select 1
                from findings f
                join closures c on c.id = new.closure_id and c.finding_id = f.id
                join review_runs r on r.id = f.review_run_id
                join review_plans p on p.id = r.review_plan_id
                join work_units w on w.id = new.work_unit_id
                join work_unit_activations a on a.id = new.work_unit_activation_id
                join work_unit_dependencies d on d.id = new.dependency_id
                join work_unit_events e on e.id = new.reopened_event_id
                join authority_events authority on authority.id = new.authority_event_id
                where f.id = new.finding_id
                  and f.status = 'open' and f.classification = 'valid'
                  and c.status = 'registered'
                  and p.work_unit_id = new.work_unit_id
                  and p.required = 1 and p.stage = 'close-ready'
                  and p.review_type in ('implementation_review', 'design_implementation_diff')
                  and w.status = 'open' and a.status = 'active'
                  and d.work_unit_id = new.work_unit_id
                  and d.depends_on_work_unit_id = new.work_unit_id
                  and d.dependency_type = 'invalidates_closure' and d.status = 'open'
                  and e.work_unit_id = new.work_unit_id and e.event_type = 'reopened'
                  and authority.status = 'active'
                  and authority.event_type in ('user_instruction', 'policy', 'design_doc')
            )
        begin
            select raise(abort, 'invalid finding remediation recovery epoch links');
        end;

        create trigger if not exists trg_remediation_recovery_epoch_immutable_update
        before update on finding_remediation_recovery_epochs
        begin select raise(abort, 'finding remediation recovery epochs are immutable'); end;

        create trigger if not exists trg_remediation_recovery_epoch_immutable_delete
        before delete on finding_remediation_recovery_epochs
        begin select raise(abort, 'finding remediation recovery epochs are immutable'); end;

        create trigger if not exists trg_correction_session_links_insert
        before insert on correction_sessions
        for each row when
            new.project_id != (select project_id from findings where id = new.finding_id)
            or new.project_id != (select project_id from closures where id = new.closure_id)
            or new.finding_id != (select finding_id from closures where id = new.closure_id)
            or exists (
                select 1 from correction_sessions active
                where active.project_id=new.project_id and active.status='active'
            )
            or not exists (
                select 1 from closures c join findings f on f.id=c.finding_id
                join review_runs r on r.id=f.review_run_id
                join review_plans p on p.id=r.review_plan_id
                where c.id=new.closure_id and c.status='registered'
                  and f.id=new.finding_id and f.status='open' and f.classification='valid'
                  and not (p.required=1 and p.stage='close-ready'
                           and p.review_type in ('implementation_review','design_implementation_diff'))
                  and trim(coalesce(c.affected_surfaces,''))!=''
                  and trim(coalesce(c.fix_plan,''))!=''
                  and trim(coalesce(c.tests_or_gates,''))!=''
                  and trim(coalesce(c.verification_plan,''))!=''
            )
        begin select raise(abort, 'invalid correction session links'); end;

        create trigger if not exists trg_correction_session_links_update
        before update of project_id, finding_id, closure_id on correction_sessions
        begin select raise(abort, 'correction session links are immutable'); end;

        create trigger if not exists trg_correction_session_status_update
        before update of status, completed_at on correction_sessions
        for each row when not (
            (old.status='active' and new.status='completed' and new.completed_at is not null
             and exists(select 1 from closures c where c.id=old.closure_id and c.status='ready_for_verification')
             and exists(select 1 from closure_attempts attempt where attempt.closure_id=old.closure_id and attempt.result is null)
             and not exists(select 1 from correction_tokens token where token.closure_id=old.closure_id and token.token_kind='transition' and token.status!='applied'))
            or
            (old.status='active' and new.status='superseded' and new.completed_at is not null
             and exists(select 1 from closures c where c.id=old.closure_id and c.status='superseded'))
            or
            (old.status='completed' and new.status='active' and new.completed_at is null
             and exists (
               select 1 from closures c join findings f on f.id=c.finding_id
               where c.id=old.closure_id and c.status='registered'
                 and f.status='open' and f.classification='valid'
             )
             and exists (
               select 1 from closure_attempts attempt
               where attempt.closure_id=old.closure_id
                 and attempt.result in ('not_fixed','needs_evidence')
                 and attempt.id=(select max(latest.id) from closure_attempts latest where latest.closure_id=old.closure_id)
             )
             and not exists (
               select 1 from correction_sessions other
               where other.project_id=old.project_id and other.status='active' and other.id!=old.id
             ))
        )
        begin select raise(abort, 'invalid correction session status transition'); end;

        create trigger if not exists trg_correction_session_immutable_delete
        before delete on correction_sessions
        begin select raise(abort, 'correction sessions are immutable'); end;

        create trigger if not exists trg_correction_token_links_insert
        before insert on correction_tokens
        for each row when
            new.project_id != (select project_id from closures where id = new.closure_id)
            or new.token_ordinal <= 0
            or not (
                (new.token_kind='file' and new.operation in ('edit','create','delete'))
                or (new.token_kind='transition' and new.operation in (
                    'design-decompose','design-reconcile','decomposition-plan-reconcile',
                    'task-accept-out-of-scope','phase-create',
                    'phase-assign','phase-dependency-add','phase-dependency-satisfy',
                    'phase-dependency-accept','stale-accept','stale-close'
                ))
            )
            or (new.token_kind='transition' and not (
                (new.operation='design-decompose'
                 and length(new.target)-length(replace(new.target,'/',''))=1
                 and new.target not glob '*[^0-9/]*'
                 and cast(substr(new.target,1,instr(new.target,'/')-1) as integer)>0
                 and cast(substr(new.target,instr(new.target,'/')+1) as integer)>0)
                or (new.operation='decomposition-plan-reconcile'
                    and length(new.target)-length(replace(new.target,'/',''))=2
                    and cast(json_extract('["'||replace(new.target,'/','","')||'"]','$[0]') as integer)>0
                    and cast(cast(json_extract('["'||replace(new.target,'/','","')||'"]','$[0]') as integer) as text)=json_extract('["'||replace(new.target,'/','","')||'"]','$[0]')
                    and cast(json_extract('["'||replace(new.target,'/','","')||'"]','$[1]') as integer)>0
                    and cast(cast(json_extract('["'||replace(new.target,'/','","')||'"]','$[1]') as integer) as text)=json_extract('["'||replace(new.target,'/','","')||'"]','$[1]')
                    and json_extract('["'||replace(new.target,'/','","')||'"]','$[2]') like 'b64:%'
                    and length(json_extract('["'||replace(new.target,'/','","')||'"]','$[2]'))>4
                    and substr(json_extract('["'||replace(new.target,'/','","')||'"]','$[2]'),5) not glob '*[^A-Za-z0-9_-]*'
                    and length(substr(json_extract('["'||replace(new.target,'/','","')||'"]','$[2]'),5))%4!=1)
                or (new.operation='design-reconcile'
                    and length(new.target)-length(replace(new.target,'/',''))=2
                    and new.target not glob '*[^0-9/]*'
                    and cast(json_extract('["'||replace(new.target,'/','","')||'"]','$[0]') as integer)>0
                    and cast(json_extract('["'||replace(new.target,'/','","')||'"]','$[1]') as integer)>0
                    and cast(json_extract('["'||replace(new.target,'/','","')||'"]','$[2]') as integer)>0)
                or (new.operation='task-accept-out-of-scope' and (
                    (new.target not glob '*[^0-9]*' and cast(new.target as integer)>0)
                    or (new.target like '@task/b64:%' and length(new.target)>10
                        and length(new.target)-length(replace(new.target,'/',''))=1
                        and substr(new.target,11) not glob '*[^A-Za-z0-9_-]*'
                        and length(substr(new.target,11))%4!=1)
                ))
                or (new.operation='phase-create'
                    and length(new.target)-length(replace(new.target,'/',''))=5
                    and new.target not like '%//%'
                    and new.target not glob '*[^a-z0-9_@/-]*'
                    and cast(json_extract('["'||replace(new.target,'/','","')||'"]','$[0]') as integer)>0
                    and cast(cast(json_extract('["'||replace(new.target,'/','","')||'"]','$[0]') as integer) as text)=json_extract('["'||replace(new.target,'/','","')||'"]','$[0]')
                    and cast(json_extract('["'||replace(new.target,'/','","')||'"]','$[1]') as integer)>0
                    and cast(cast(json_extract('["'||replace(new.target,'/','","')||'"]','$[1]') as integer) as text)=json_extract('["'||replace(new.target,'/','","')||'"]','$[1]')
                    and json_extract('["'||replace(new.target,'/','","')||'"]','$[2]') glob '@[a-z0-9_-]*'
                    and substr(json_extract('["'||replace(new.target,'/','","')||'"]','$[2]'),2) not glob '*[^a-z0-9_-]*'
                    and json_extract('["'||replace(new.target,'/','","')||'"]','$[3]') glob '[a-z0-9_-]*'
                    and json_extract('["'||replace(new.target,'/','","')||'"]','$[3]') not glob '*[^a-z0-9_-]*'
                    and cast(json_extract('["'||replace(new.target,'/','","')||'"]','$[4]') as integer)>0
                    and cast(cast(json_extract('["'||replace(new.target,'/','","')||'"]','$[4]') as integer) as text)=json_extract('["'||replace(new.target,'/','","')||'"]','$[4]')
                    and json_extract('["'||replace(new.target,'/','","')||'"]','$[5]') glob '[a-z0-9_-]*'
                    and json_extract('["'||replace(new.target,'/','","')||'"]','$[5]') not glob '*[^a-z0-9_-]*')
                or (new.operation='phase-assign'
                    and length(new.target)-length(replace(new.target,'/','')) in (1,2)
                    and new.target not like '/%' and new.target not like '%/'
                    and new.target not glob '*[^A-Za-z0-9_@/:-]*'
                    and (json_extract('["'||replace(new.target,'/','","')||'"]','$[0]') glob '@[a-z0-9_-]*'
                         or (cast(json_extract('["'||replace(new.target,'/','","')||'"]','$[0]') as integer)>0
                             and cast(cast(json_extract('["'||replace(new.target,'/','","')||'"]','$[0]') as integer) as text)=json_extract('["'||replace(new.target,'/','","')||'"]','$[0]')))
                    and (json_extract('["'||replace(new.target,'/','","')||'"]','$[0]') not glob '@*'
                         or substr(json_extract('["'||replace(new.target,'/','","')||'"]','$[0]'),2) not glob '*[^a-z0-9_-]*')
                    and ((length(new.target)-length(replace(new.target,'/',''))=1
                          and cast(json_extract('["'||replace(new.target,'/','","')||'"]','$[1]') as integer)>0
                          and cast(cast(json_extract('["'||replace(new.target,'/','","')||'"]','$[1]') as integer) as text)=json_extract('["'||replace(new.target,'/','","')||'"]','$[1]'))
                         or (length(new.target)-length(replace(new.target,'/',''))=2
                          and json_extract('["'||replace(new.target,'/','","')||'"]','$[1]')='@task'
                          and json_extract('["'||replace(new.target,'/','","')||'"]','$[2]') like 'b64:%'
                          and length(json_extract('["'||replace(new.target,'/','","')||'"]','$[2]'))>4
                          and substr(json_extract('["'||replace(new.target,'/','","')||'"]','$[2]'),5) not glob '*[^A-Za-z0-9_-]*'
                          and length(substr(json_extract('["'||replace(new.target,'/','","')||'"]','$[2]'),5))%4!=1)))
                or (new.operation='phase-dependency-add'
                    and length(new.target)-length(replace(new.target,'/',''))=2
                    and (new.target like '%/blocks' or new.target like '%/requires')
                    and new.target not like '/%' and new.target not like '%//%'
                    and new.target not glob '*[^a-z0-9_@/-]*'
                    and (json_extract('["'||replace(new.target,'/','","')||'"]','$[0]') glob '@[a-z0-9_-]*'
                         or (cast(json_extract('["'||replace(new.target,'/','","')||'"]','$[0]') as integer)>0
                             and cast(cast(json_extract('["'||replace(new.target,'/','","')||'"]','$[0]') as integer) as text)=json_extract('["'||replace(new.target,'/','","')||'"]','$[0]')))
                    and (json_extract('["'||replace(new.target,'/','","')||'"]','$[1]') glob '@[a-z0-9_-]*'
                         or (cast(json_extract('["'||replace(new.target,'/','","')||'"]','$[1]') as integer)>0
                             and cast(cast(json_extract('["'||replace(new.target,'/','","')||'"]','$[1]') as integer) as text)=json_extract('["'||replace(new.target,'/','","')||'"]','$[1]'))))
                or (new.operation in ('phase-dependency-satisfy','phase-dependency-accept')
                    and new.target not glob '*[^0-9]*' and cast(new.target as integer)>0)
                or (new.operation in ('stale-accept','stale-close')
                    and length(new.target)-length(replace(new.target,'/',''))=1
                    and new.target glob '*/[0-9]*'
                    and substr(new.target,1,instr(new.target,'/')-1) in (
                      'task_derivation','checklist','validation_gate','coverage_item','review_plan'
                    )
                    and substr(new.target,instr(new.target,'/')+1) not glob '*[^0-9]*'
                    and cast(substr(new.target,instr(new.target,'/')+1) as integer)>0)
            ))
            or (new.operation='design-decompose' and new.pre_state != 'checklist_max:'||(select coalesce(max(id),0) from checklists))
            or (new.operation='phase-create' and new.pre_state != 'phase_max:'||(select coalesce(max(id),0) from work_phases))
            or (new.operation='phase-dependency-add' and new.pre_state != 'phase_dependency_max:'||(select coalesce(max(id),0) from work_phase_dependencies))
            or not exists (
                select 1 from closures c join findings f on f.id=c.finding_id
                join review_runs r on r.id=f.review_run_id
                join review_plans p on p.id=r.review_plan_id
                where c.id=new.closure_id and c.status='registered'
                  and f.status='open' and f.classification='valid'
                  and not (p.required=1 and p.stage='close-ready'
                           and p.review_type in ('implementation_review','design_implementation_diff'))
            )
        begin select raise(abort, 'invalid correction token links'); end;

        create trigger if not exists trg_correction_token_links_update
        before update of project_id, closure_id, token_ordinal, token_kind, operation, target, pre_state, pre_hash on correction_tokens
        begin select raise(abort, 'correction token contract is immutable'); end;

        create trigger if not exists trg_correction_token_status_update
        before update of status, applied_at on correction_tokens
        for each row when
            old.status != 'pending'
            or not (
                (new.status='applied' and new.applied_at is not null
                 and exists (
                    select 1 from correction_transition_applications application
                    where application.correction_token_id=old.id
                ))
                or
                (new.status='superseded' and new.applied_at is null)
            )
        begin select raise(abort, 'invalid correction token status transition'); end;

        create trigger if not exists trg_correction_token_immutable_delete
        before delete on correction_tokens
        begin select raise(abort, 'correction tokens are immutable'); end;

        create trigger if not exists trg_correction_application_links_insert
        before insert on correction_transition_applications
        for each row when
            new.project_id != (select project_id from correction_sessions where id = new.correction_session_id)
            or new.project_id != (select project_id from correction_tokens where id = new.correction_token_id)
            or (select closure_id from correction_sessions where id = new.correction_session_id)
               != (select closure_id from correction_tokens where id = new.correction_token_id)
            or (new.authority_event_id is not null and new.project_id != (
                select project_id from authority_events where id = new.authority_event_id
            ))
            or 'active' != (select status from correction_sessions where id=new.correction_session_id)
            or 'pending' != (select status from correction_tokens where id=new.correction_token_id)
            or (
                (select operation from correction_tokens where id=new.correction_token_id)
                  in ('task-accept-out-of-scope','phase-dependency-accept')
                and (new.authority_event_id is null or new.evidence_ref is not null)
            )
            or (
                (select operation from correction_tokens where id=new.correction_token_id)
                  = 'phase-dependency-satisfy'
                and (new.authority_event_id is not null or trim(coalesce(new.evidence_ref,''))='')
            )
            or (
                (select operation from correction_tokens where id=new.correction_token_id)
                  not in ('task-accept-out-of-scope','phase-dependency-accept','phase-dependency-satisfy')
                and (new.authority_event_id is not null or new.evidence_ref is not null)
            )
            or not (
              ((select operation from correction_tokens where id=new.correction_token_id)='phase-create'
               and exists(select 1 from work_phases p join correction_tokens token on token.id=new.correction_token_id
                 where 'phase:'||p.id=new.result_ref and p.project_id=new.project_id
                   and p.id>cast(substr(token.pre_state,instr(token.pre_state,':')+1) as integer)
                   and cast(json_extract('["'||replace(token.target,'/','","')||'"]','$[0]') as integer)=p.work_unit_id
                   and cast(json_extract('["'||replace(token.target,'/','","')||'"]','$[1]') as integer)=p.design_version_id
                   and json_extract('["'||replace(token.target,'/','","')||'"]','$[3]')=p.kind
                   and cast(json_extract('["'||replace(token.target,'/','","')||'"]','$[4]') as integer)=p.phase_order
                   and json_extract('["'||replace(token.target,'/','","')||'"]','$[5]')=p.phase_key))
              or ((select operation from correction_tokens where id=new.correction_token_id)='phase-assign'
               and exists(select 1 from work_phase_task_memberships m join correction_tokens token on token.id=new.correction_token_id
                 where 'phase:'||m.phase_id||':task:'||m.task_id=new.result_ref and m.project_id=new.project_id
                   and (
                     json_extract('["'||replace(token.target,'/','","')||'"]','$[0]')=cast(m.phase_id as text)
                     or exists(select 1 from correction_transition_aliases alias join correction_transition_applications earlier on earlier.id=alias.correction_application_id join correction_tokens earlier_token on earlier_token.id=earlier.correction_token_id
                       where alias.correction_session_id=new.correction_session_id and alias.alias=json_extract('["'||replace(token.target,'/','","')||'"]','$[0]') and alias.record_type='phase' and alias.record_id=m.phase_id and earlier_token.token_ordinal<token.token_ordinal)
                   )
                   and (
                     substr(token.target,instr(token.target,'/')+1)=cast(m.task_id as text)
                     or exists(
                       select 1
                       from correction_transition_applications earlier
                       join correction_tokens earlier_token on earlier_token.id=earlier.correction_token_id
                       left join checklist_items earlier_item
                         on earlier.result_ref='checklist:'||earlier_item.checklist_id
                       left join correction_decomposition_task_memberships earlier_plan_item
                         on earlier_plan_item.correction_application_id=earlier.id
                       where earlier.correction_session_id=new.correction_session_id
                         and earlier_token.token_ordinal<token.token_ordinal
                         and ((earlier_token.operation in ('design-decompose','design-reconcile') and earlier_item.task_id=m.task_id)
                           or (earlier_token.operation='decomposition-plan-reconcile' and earlier_plan_item.task_id=m.task_id))
                     )
                   )))
              or ((select operation from correction_tokens where id=new.correction_token_id)='phase-dependency-add'
               and exists(select 1 from work_phase_dependencies d join correction_tokens token on token.id=new.correction_token_id
                 where 'phase-dependency:'||d.id=new.result_ref and d.project_id=new.project_id
                   and d.id>cast(substr(token.pre_state,instr(token.pre_state,':')+1) as integer)
                   and json_extract('["'||replace(token.target,'/','","')||'"]','$[2]')=d.dependency_type
                   and (
                     json_extract('["'||replace(token.target,'/','","')||'"]','$[0]')=cast(d.from_phase_id as text)
                     or exists(select 1 from correction_transition_aliases alias join correction_transition_applications earlier on earlier.id=alias.correction_application_id join correction_tokens earlier_token on earlier_token.id=earlier.correction_token_id
                       where alias.correction_session_id=new.correction_session_id and alias.alias=json_extract('["'||replace(token.target,'/','","')||'"]','$[0]') and alias.record_type='phase' and alias.record_id=d.from_phase_id and earlier_token.token_ordinal<token.token_ordinal)
                   )
                   and (
                     json_extract('["'||replace(token.target,'/','","')||'"]','$[1]')=cast(d.to_phase_id as text)
                     or exists(select 1 from correction_transition_aliases alias join correction_transition_applications earlier on earlier.id=alias.correction_application_id join correction_tokens earlier_token on earlier_token.id=earlier.correction_token_id
                       where alias.correction_session_id=new.correction_session_id and alias.alias=json_extract('["'||replace(token.target,'/','","')||'"]','$[1]') and alias.record_type='phase' and alias.record_id=d.to_phase_id and earlier_token.token_ordinal<token.token_ordinal)
                   )))
              or ((select operation from correction_tokens where id=new.correction_token_id)='phase-dependency-satisfy'
               and exists(select 1 from work_phase_dependencies d join correction_tokens token on token.id=new.correction_token_id where d.id=cast(token.target as integer) and 'phase-dependency:'||d.id||':satisfied'=new.result_ref and d.project_id=new.project_id and d.status='satisfied' and d.evidence_ref=new.evidence_ref))
              or ((select operation from correction_tokens where id=new.correction_token_id)='phase-dependency-accept'
               and exists(select 1 from work_phase_dependencies d join correction_tokens token on token.id=new.correction_token_id where d.id=cast(token.target as integer) and 'phase-dependency:'||d.id||':accepted'=new.result_ref and d.project_id=new.project_id and d.status='accepted' and d.authority_event_id=new.authority_event_id))
              or ((select operation from correction_tokens where id=new.correction_token_id)='task-accept-out-of-scope'
               and exists(select 1 from acceptance_records ar join tasks t on t.id=ar.task_id join work_units w on w.id=t.work_unit_id join correction_tokens token on token.id=new.correction_token_id
                 where new.result_ref='task:'||t.id||':acceptance:'||ar.id and w.project_id=new.project_id
                   and t.status='accepted_out_of_scope' and ar.status='approved'
                   and ar.approved_by_authority_event_id=new.authority_event_id
                   and (token.target=cast(t.id as text) or exists(
                     select 1
                     from correction_transition_applications earlier
                     join correction_tokens earlier_token on earlier_token.id=earlier.correction_token_id
                     left join checklist_items earlier_item
                       on earlier.result_ref='checklist:'||earlier_item.checklist_id
                     left join correction_decomposition_task_memberships earlier_plan_item
                       on earlier_plan_item.correction_application_id=earlier.id
                     where earlier.correction_session_id=new.correction_session_id
                       and earlier_token.token_ordinal<token.token_ordinal
                       and ((earlier_token.operation in ('design-decompose','design-reconcile') and earlier_item.task_id=t.id)
                         or (earlier_token.operation='decomposition-plan-reconcile' and earlier_plan_item.task_id=t.id))
                   ))))
              or ((select operation from correction_tokens where id=new.correction_token_id) in ('stale-accept','stale-close')
               and exists(select 1 from acceptance_records ar join correction_tokens token on token.id=new.correction_token_id
                 where ar.project_id=new.project_id and ar.target_type='stale_record' and ar.status='approved'
                   and token.target=ar.stale_record_type||'/'||ar.stale_record_id
                   and new.result_ref like 'stale:'||ar.stale_record_type||':'||ar.stale_record_id||':%'))
              or ((select operation from correction_tokens where id=new.correction_token_id)='design-decompose'
               and exists(select 1 from checklists c join correction_tokens token on token.id=new.correction_token_id
                 where new.result_ref='checklist:'||c.id and c.project_id=new.project_id
                   and c.id>cast(substr(token.pre_state,instr(token.pre_state,':')+1) as integer)
                   and token.target=cast(c.design_version_id as text)||'/'||cast(c.work_unit_id as text)))
              or ((select operation from correction_tokens where id=new.correction_token_id)='design-reconcile'
               and exists(select 1 from checklists c join correction_tokens token on token.id=new.correction_token_id
                 where new.result_ref='checklist:'||c.id and c.project_id=new.project_id
                   and token.target=cast(c.design_version_id as text)||'/'||cast(c.work_unit_id as text)||'/'||cast(c.id as text)))
              or ((select operation from correction_tokens where id=new.correction_token_id)='decomposition-plan-reconcile'
               and new.result_ref glob 'decomposition-plan:[1-9]*'
               and substr(new.result_ref,20) not glob '*[^0-9]*')
            )
        begin select raise(abort, 'invalid correction transition application links'); end;

        create trigger if not exists trg_correction_application_links_update
        before update on correction_transition_applications
        begin select raise(abort, 'correction transition applications are immutable'); end;

        create trigger if not exists trg_correction_application_immutable_delete
        before delete on correction_transition_applications
        begin select raise(abort, 'correction transition applications are immutable'); end;

        create trigger if not exists trg_correction_alias_links_insert
        before insert on correction_transition_aliases
        for each row when
            new.project_id != (select project_id from correction_sessions where id = new.correction_session_id)
            or new.project_id != (select project_id from correction_transition_applications where id = new.correction_application_id)
            or new.correction_session_id != (
                select correction_session_id from correction_transition_applications
                where id = new.correction_application_id
            )
            or not (
                (new.record_type = 'checklist' and exists(select 1 from checklists where id = new.record_id and project_id=new.project_id))
                or (new.record_type = 'task' and exists(select 1 from tasks t join work_units w on w.id=t.work_unit_id where t.id = new.record_id and w.project_id=new.project_id))
                or (new.record_type = 'task_derivation' and exists(select 1 from task_derivations where id = new.record_id and project_id=new.project_id))
                or (new.record_type = 'checklist_item' and exists(select 1 from checklist_items where id = new.record_id and project_id=new.project_id))
                or (new.record_type = 'coverage_item' and exists(select 1 from coverage_items where id = new.record_id and project_id=new.project_id))
                or (new.record_type = 'validation_gate' and exists(select 1 from validation_gates where id = new.record_id and project_id=new.project_id))
                or (new.record_type = 'phase' and exists(select 1 from work_phases where id = new.record_id and project_id=new.project_id))
                or (new.record_type = 'phase_dependency' and exists(select 1 from work_phase_dependencies d join work_phases p on p.id=d.from_phase_id where d.id = new.record_id and p.project_id=new.project_id))
            )
            or not (
              (
                (select token.operation from correction_transition_applications application
                 join correction_tokens token on token.id=application.correction_token_id
                where application.id=new.correction_application_id) in ('design-decompose','design-reconcile')
                and (
                  (new.record_type='checklist' and
                   new.alias='@checklist' and
                   (select result_ref from correction_transition_applications where id=new.correction_application_id)='checklist:'||new.record_id)
                  or (new.record_type='checklist_item' and exists(
                    select 1 from checklist_items ci join design_requirements r on r.id=ci.design_requirement_id
                    where ci.id=new.record_id and
                      new.alias='@checklist-item/'||r.requirement_key and
                      (select result_ref from correction_transition_applications where id=new.correction_application_id)='checklist:'||ci.checklist_id))
                  or (new.record_type='task' and exists(
                    select 1 from checklist_items ci join design_requirements r on r.id=ci.design_requirement_id where ci.task_id=new.record_id and
                      new.alias='@task/'||r.requirement_key and
                      (select result_ref from correction_transition_applications where id=new.correction_application_id)='checklist:'||ci.checklist_id))
                  or (new.record_type='task_derivation' and exists(
                    select 1 from task_derivations td join checklist_items ci on ci.id=td.checklist_item_id join design_requirements r on r.id=ci.design_requirement_id
                    where td.id=new.record_id and
                      new.alias='@derivation/'||r.requirement_key and
                      (select result_ref from correction_transition_applications where id=new.correction_application_id)='checklist:'||ci.checklist_id))
                  or (new.record_type='coverage_item' and exists(
                    select 1 from coverage_items c join checklist_items ci on ci.task_id=c.task_id join design_requirements r on r.id=ci.design_requirement_id
                    where c.id=new.record_id and c.design_requirement_id=ci.design_requirement_id and
                      new.alias='@coverage/'||r.requirement_key and
                      (select result_ref from correction_transition_applications where id=new.correction_application_id)='checklist:'||ci.checklist_id))
                  or (new.record_type='validation_gate' and exists(
                    select 1 from validation_gates vg join checklist_items ci on ci.task_id=vg.task_id join design_requirements r on r.id=ci.design_requirement_id
                    where vg.id=new.record_id and vg.design_requirement_id=ci.design_requirement_id and
                      new.alias='@gate/'||r.requirement_key||'/'||vg.gate_key and
                      (select result_ref from correction_transition_applications where id=new.correction_application_id)='checklist:'||ci.checklist_id))
                  or (new.record_type='task'
                      and (select token.operation from correction_transition_applications application join correction_tokens token on token.id=application.correction_token_id where application.id=new.correction_application_id)='design-reconcile'
                      and new.alias='@superseded-task/'||new.record_id
                      and exists(
                        select 1
                        from task_derivations td
                        join design_requirements r on r.id=td.design_requirement_id
                        join tasks t on t.id=td.task_id
                        join checklist_items ci on ci.id=td.checklist_item_id
                        join correction_transition_applications app on app.id=new.correction_application_id
                        join correction_tokens token on token.id=app.correction_token_id
                        where td.task_id=new.record_id and td.status='closed'
                          and r.design_version_id=cast(json_extract('["'||replace(token.target,'/','","')||'"]','$[0]') as integer)
                          and t.work_unit_id=cast(json_extract('["'||replace(token.target,'/','","')||'"]','$[1]') as integer)
                          and ci.checklist_id!=cast(json_extract('["'||replace(token.target,'/','","')||'"]','$[2]') as integer)
                          and ('|'||substr(app.before_state,
                            instr(app.before_state,'derivations=[')+13,
                            instr(app.before_state,'];items=')-(instr(app.before_state,'derivations=[')+13)
                          )||'|') like '%|'||td.id||':active|%'
                      ))
                )
              )
              or
              (
                (select token.operation from correction_transition_applications application
                 join correction_tokens token on token.id=application.correction_token_id
                 where application.id=new.correction_application_id)='task-accept-out-of-scope'
                and (
                  (new.record_type='task' and
                   new.alias='@accepted-task/'||new.record_id and
                   (select result_ref from correction_transition_applications where id=new.correction_application_id) like 'task:'||new.record_id||':acceptance:%')
                  or (new.record_type='checklist_item' and exists(
                    select 1 from checklist_items ci where ci.id=new.record_id and ci.status='accepted_out_of_scope' and
                      new.alias='@accepted-checklist_item/'||new.record_id and
                      (select result_ref from correction_transition_applications where id=new.correction_application_id) like 'task:'||ci.task_id||':acceptance:%' and
                      ('|'||(select substr(before_state, instr(before_state,'items=[')+7, instr(before_state,'];gates=')-(instr(before_state,'items=[')+7)) from correction_transition_applications where id=new.correction_application_id)||'|') not like '%|'||new.record_id||':accepted_out_of_scope|%' and
                      ('|'||(select substr(after_state, instr(after_state,'items=[')+7, instr(after_state,'];gates=')-(instr(after_state,'items=[')+7)) from correction_transition_applications where id=new.correction_application_id)||'|') like '%|'||new.record_id||':accepted_out_of_scope|%'))
                  or (new.record_type='validation_gate' and exists(
                    select 1 from validation_gates vg where vg.id=new.record_id and vg.status='closed' and
                      new.alias='@accepted-validation_gate/'||new.record_id and
                      (select result_ref from correction_transition_applications where id=new.correction_application_id) like 'task:'||vg.task_id||':acceptance:%' and
                      ('|'||(select substr(before_state, instr(before_state,'gates=[')+7, instr(before_state,'];coverage=')-(instr(before_state,'gates=[')+7)) from correction_transition_applications where id=new.correction_application_id)||'|') not like '%|'||new.record_id||':closed|%' and
                      ('|'||(select substr(after_state, instr(after_state,'gates=[')+7, instr(after_state,'];coverage=')-(instr(after_state,'gates=[')+7)) from correction_transition_applications where id=new.correction_application_id)||'|') like '%|'||new.record_id||':closed|%'))
                  or (new.record_type='coverage_item' and exists(
                    select 1 from coverage_items c where c.id=new.record_id and c.status='accepted_out_of_scope' and
                      new.alias='@accepted-coverage_item/'||new.record_id and
                      (select result_ref from correction_transition_applications where id=new.correction_application_id) like 'task:'||c.task_id||':acceptance:%' and
                      ('|'||(select substr(before_state, instr(before_state,'coverage=[')+10, instr(before_state,'];phases=')-(instr(before_state,'coverage=[')+10)) from correction_transition_applications where id=new.correction_application_id)||'|') not like '%|'||new.record_id||':accepted_out_of_scope|%' and
                      ('|'||(select substr(after_state, instr(after_state,'coverage=[')+10, instr(after_state,'];phases=')-(instr(after_state,'coverage=[')+10)) from correction_transition_applications where id=new.correction_application_id)||'|') like '%|'||new.record_id||':accepted_out_of_scope|%'))
                )
              )
              or
              (
                (select token.operation from correction_transition_applications application
                 join correction_tokens token on token.id=application.correction_token_id
                 where application.id=new.correction_application_id)='phase-create'
                and new.record_type='phase'
                and new.alias=json_extract('["'||replace((select token.target from correction_transition_applications application join correction_tokens token on token.id=application.correction_token_id where application.id=new.correction_application_id),'/','","')||'"]','$[2]')
                and (select result_ref from correction_transition_applications where id=new.correction_application_id)='phase:'||new.record_id
              )
              or
              (
                (select token.operation from correction_transition_applications application
                 join correction_tokens token on token.id=application.correction_token_id
                 where application.id=new.correction_application_id)='phase-dependency-add'
                and new.record_type='phase_dependency'
                and new.alias='@dependency/'||new.record_id
                and (select result_ref from correction_transition_applications where id=new.correction_application_id)='phase-dependency:'||new.record_id
              )
            )
        begin select raise(abort, 'invalid correction transition alias links'); end;

        create trigger if not exists trg_correction_alias_immutable_update
        before update on correction_transition_aliases
        begin select raise(abort, 'correction transition aliases are immutable'); end;

        create trigger if not exists trg_correction_alias_immutable_delete
        before delete on correction_transition_aliases
        begin select raise(abort, 'correction transition aliases are immutable'); end;
        "#;
    conn.execute_batch(lifecycle_schema)?;

    ensure_column(
        conn,
        "finding_remediation_recovery_epochs",
        "authority_event_id",
        "integer references authority_events(id)",
    )?;
    ensure_column(
        conn,
        "correction_transition_applications",
        "before_state",
        "text not null default 'legacy-unrecorded'",
    )?;
    ensure_column(
        conn,
        "correction_transition_applications",
        "after_state",
        "text not null default 'legacy-unrecorded'",
    )?;

    // Preserve verified legacy history first. For other findings, only the
    // greatest-id closure remains current.
    conn.execute_batch(
        r#"
        update closures set status = 'superseded'
        where exists (
            select 1 from finding_verifications fv
            where fv.finding_id = closures.finding_id and fv.result = 'verified'
        );

        update closures set status = 'verified'
        where id = (
            select fv.closure_id from finding_verifications fv
            where fv.finding_id = closures.finding_id and fv.result = 'verified'
            order by fv.id desc limit 1
        );

        update closures set status = 'superseded'
        where not exists (
            select 1 from finding_verifications fv
            where fv.finding_id = closures.finding_id and fv.result = 'verified'
        )
        and id != (select max(c2.id) from closures c2 where c2.finding_id = closures.finding_id);

        update findings set status = 'accepted_out_of_scope'
        where exists (
            select 1 from acceptance_records ar
            where ar.finding_id = findings.id
              and ar.target_type = 'finding'
              and ar.acceptance_type = 'accepted_out_of_scope'
              and ar.status = 'approved'
        );

        update closure_attempts set result = 'superseded', resolved_at = coalesce(resolved_at, current_timestamp)
        where result is null and closure_id in (
            select c.id from closures c
            join findings f on f.id = c.finding_id
            where f.status = 'accepted_out_of_scope'
        );

        update closures set status = 'superseded'
        where finding_id in (
            select id from findings where status = 'accepted_out_of_scope'
        );

        update findings set status = 'open'
        where classification = 'valid'
          and status = 'closed'
          and not exists (
              select 1 from finding_verifications fv
              where fv.finding_id = findings.id and fv.result = 'verified'
          )
          and not exists (
              select 1 from acceptance_records ar
              where ar.finding_id = findings.id
                and ar.target_type = 'finding'
                and ar.acceptance_type = 'accepted_out_of_scope'
                and ar.status = 'approved'
          );

        update closures
        set status = case
            when (
                coalesce(trim(affected_surfaces), '') = ''
                or coalesce(trim(fix_plan), '') = ''
                or coalesce(trim(tests_or_gates), '') = ''
                or coalesce(trim(verification_plan), '') = ''
            ) then 'incomplete'
            else 'registered'
        end
        where id = (select max(c2.id) from closures c2 where c2.finding_id = closures.finding_id)
          and exists (
              select 1 from findings f
              where f.id = closures.finding_id
                and f.status = 'open' and f.classification = 'valid'
          )
          and not exists (
              select 1 from closure_attempts a
              where a.closure_id = closures.id and a.result is null
          )
          and not exists (
              select 1 from finding_verifications fv
              where fv.finding_id = closures.finding_id and fv.result = 'verified'
          );

        update findings set status = 'closed'
        where exists (
            select 1 from finding_verifications fv
            where fv.finding_id = findings.id and fv.result = 'verified'
        );

        update findings set status = 'accepted_out_of_scope'
        where exists (
            select 1 from acceptance_records ar
            where ar.finding_id = findings.id
              and ar.target_type = 'finding'
              and ar.acceptance_type = 'accepted_out_of_scope'
              and ar.status = 'approved'
        );

        update findings set status = 'open'
        where classification = 'valid'
          and status = 'closed'
          and not exists (
              select 1 from finding_verifications fv
              where fv.finding_id = findings.id and fv.result = 'verified'
          )
          and not exists (
              select 1 from acceptance_records ar
              where ar.finding_id = findings.id
                and ar.target_type = 'finding'
                and ar.acceptance_type = 'accepted_out_of_scope'
                and ar.status = 'approved'
          );
        "#,
    )?;
    Ok(())
}
