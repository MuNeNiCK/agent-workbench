use anyhow::{Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

use super::project::*;

pub(super) fn migrate_acceptance_records(conn: &Connection) -> Result<()> {
    let table_sql = conn
        .query_row(
            "select sql from sqlite_schema where type = 'table' and name = 'acceptance_records'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(table_sql) = table_sql else {
        return Ok(());
    };
    if acceptance_records_schema_current(conn, &table_sql)? {
        return Ok(());
    }

    conn.execute_batch(
        r#"
        drop trigger if exists trg_acceptance_design_requirement_project_insert;
        drop trigger if exists trg_acceptance_design_requirement_project_update;
        drop trigger if exists trg_acceptance_task_project_insert;
        drop trigger if exists trg_acceptance_task_project_update;
        drop trigger if exists trg_acceptance_validation_gate_template_project_insert;
        drop trigger if exists trg_acceptance_validation_gate_template_project_update;
        drop trigger if exists trg_acceptance_coverage_item_project_insert;
        drop trigger if exists trg_acceptance_coverage_item_project_update;
        drop trigger if exists trg_acceptance_general_project_insert;
        drop trigger if exists trg_acceptance_general_project_update;
        drop trigger if exists trg_repository_state_classification_acceptance_insert;
        drop trigger if exists trg_repository_state_classification_acceptance_update;
        pragma legacy_alter_table = on;
        alter table acceptance_records rename to acceptance_records_old;
        pragma legacy_alter_table = off;

        create table acceptance_records (
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

        insert into acceptance_records(
            id, project_id, target_type, task_id, design_requirement_id,
            validation_gate_template_id, coverage_item_id, finding_id, validation_gate_id,
            validation_run_id, repository_state_classification_id,
            repository_snapshot_comparison_id, review_plan_id, checklist_item_id,
            command_profile_id, command_usage_id, command_deviation_id,
            rule_binding_id, stale_record_type, stale_record_id, design_package_key,
            design_file_path, design_requirement_key,
            acceptance_type, reason, scope, created_by, status,
            approved_by_authority_event_id, approved_at, created_at, review_impact
        )
        select
            id, project_id, target_type, task_id, design_requirement_id,
            validation_gate_template_id, coverage_item_id, finding_id, validation_gate_id,
            validation_run_id, repository_state_classification_id,
            repository_snapshot_comparison_id, review_plan_id, checklist_item_id,
            command_profile_id, command_usage_id, command_deviation_id, null,
            stale_record_type, stale_record_id, design_package_key, design_file_path,
            design_requirement_key,
            acceptance_type, reason, scope,
            case
                when created_by in ('user', 'agent', 'system') then created_by
                else 'system'
            end,
            case
                when status in ('proposed', 'approved', 'rejected', 'expired') then status
                when status = 'revoked' then 'rejected'
                else 'approved'
            end,
            approved_by_authority_event_id, approved_at,
            created_at, review_impact
        from acceptance_records_old;

        drop table acceptance_records_old;
        "#,
    )?;
    Ok(())
}

pub(super) fn acceptance_records_needs_migration(conn: &Connection) -> Result<bool> {
    let table_sql = conn
        .query_row(
            "select sql from sqlite_schema where type = 'table' and name = 'acceptance_records'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(table_sql) = table_sql else {
        return Ok(false);
    };
    Ok(!acceptance_records_schema_current(conn, &table_sql)?)
}

pub(super) fn acceptance_records_schema_current(
    conn: &Connection,
    table_sql: &str,
) -> Result<bool> {
    Ok(table_sql.contains("'design_file'")
        && table_sql.contains("'coverage_item'")
        && table_sql.contains("'design_requirement_key'")
        && table_sql.contains("'validation_run'")
        && table_sql.contains("'rule_binding'")
        && table_sql.contains("'evidence_gap'")
        && table_has_column(conn, "acceptance_records", "design_package_key")?
        && table_has_column(conn, "acceptance_records", "design_file_path")?
        && table_has_column(conn, "acceptance_records", "design_requirement_key")?
        && table_has_column(conn, "acceptance_records", "coverage_item_id")?
        && table_has_column(conn, "acceptance_records", "validation_run_id")?
        && table_has_column(conn, "acceptance_records", "rule_binding_id")?)
}

pub(super) fn repair_acceptance_record_references(conn: &Connection) -> Result<()> {
    let broken_reference_count: i64 = conn.query_row(
        r#"
        select count(*)
        from sqlite_schema
        where sql like '%acceptance_records_old%'
        "#,
        [],
        |row| row.get(0),
    )?;
    if broken_reference_count == 0 {
        return Ok(());
    }

    let schema_version: i64 = conn.pragma_query_value(None, "schema_version", |row| row.get(0))?;
    conn.execute_batch(
        r#"
        pragma writable_schema = on;
        update sqlite_schema
        set sql = replace(replace(sql, '"acceptance_records_old"', 'acceptance_records'), 'acceptance_records_old', 'acceptance_records')
        where sql like '%acceptance_records_old%';
        pragma writable_schema = off;
        "#,
    )?;
    conn.pragma_update(None, "schema_version", schema_version + 1)?;
    Ok(())
}

pub(super) fn migrate_repository_snapshot_comparisons(conn: &Connection) -> Result<()> {
    let table_sql = conn
        .query_row(
            "select sql from sqlite_schema where type = 'table' and name = 'repository_snapshot_comparisons'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(table_sql) = table_sql else {
        return Ok(());
    };
    if table_sql.contains("'review'") && !table_sql.contains("'inspection'") {
        return Ok(());
    }

    conn.pragma_update(None, "foreign_keys", false)?;
    conn.execute_batch(
        r#"
        create table repository_snapshot_comparisons_new (
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

        insert into repository_snapshot_comparisons_new(
            id, base_repository_snapshot_id, current_repository_snapshot_id,
            comparison_type, head_changed, dirty_state_changed,
            nested_repository_changed, result, created_at
        )
        select
            id, base_repository_snapshot_id, current_repository_snapshot_id,
            case when comparison_type = 'inspection' then 'review' else comparison_type end,
            head_changed, dirty_state_changed, nested_repository_changed, result, created_at
        from repository_snapshot_comparisons;

        drop table repository_snapshot_comparisons;
        alter table repository_snapshot_comparisons_new rename to repository_snapshot_comparisons;
        "#,
    )?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(())
}

pub(super) fn migrate_kpt_items(conn: &Connection) -> Result<()> {
    let table_sql = conn
        .query_row(
            "select sql from sqlite_schema where type = 'table' and name = 'kpt_items'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(table_sql) = table_sql else {
        return Ok(());
    };
    let conversion_sql = conn
        .query_row(
            "select sql from sqlite_schema where type = 'table' and name = 'kpt_item_conversions'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let conversion_sql = conversion_sql.unwrap_or_default();
    if table_sql.contains("'converted'")
        && table_sql.contains("linked_review_finding_id integer references findings")
        && conversion_sql.contains("review_policy_id integer references review_policies")
        && conversion_sql.contains("design_version_id integer references design_versions")
        && conversion_sql.contains("target_type = 'task'")
    {
        return Ok(());
    }

    conn.execute_batch(
        r#"
        pragma foreign_keys = off;

        alter table kpt_item_conversions rename to kpt_item_conversions_old;
        alter table kpt_items rename to kpt_items_old;

        create table kpt_items (
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

        insert into kpt_items(
            id, kpt_review_id, item_type, title, details, severity,
            linked_user_correction_id, linked_command_profile_id,
            linked_review_finding_id, linked_task_id, proposed_action, status, created_at
        )
        select
            id, kpt_review_id, item_type, title, details, severity,
            linked_user_correction_id, linked_command_profile_id,
            linked_review_finding_id, linked_task_id, proposed_action, status, created_at
        from kpt_items_old;

        create table kpt_item_conversions (
            id integer primary key,
            kpt_item_id integer not null references kpt_items(id) on delete cascade,
            target_type text not null check (target_type in ('task', 'command_profile', 'review_policy', 'design_version', 'decision', 'user_correction')),
            task_id integer references tasks(id),
            command_profile_id integer references command_profiles(id),
            review_policy_id integer references review_policies(id),
            design_version_id integer references design_versions(id),
            decision_id integer references decisions(id),
            user_correction_id integer references user_corrections(id),
            created_at text not null,
            check (
                (target_type = 'task' and task_id is not null and command_profile_id is null and review_policy_id is null and design_version_id is null and decision_id is null and user_correction_id is null)
                or (target_type = 'command_profile' and task_id is null and command_profile_id is not null and review_policy_id is null and design_version_id is null and decision_id is null and user_correction_id is null)
                or (target_type = 'review_policy' and task_id is null and command_profile_id is null and review_policy_id is not null and design_version_id is null and decision_id is null and user_correction_id is null)
                or (target_type = 'design_version' and task_id is null and command_profile_id is null and review_policy_id is null and design_version_id is not null and decision_id is null and user_correction_id is null)
                or (target_type = 'decision' and task_id is null and command_profile_id is null and review_policy_id is null and design_version_id is null and decision_id is not null and user_correction_id is null)
                or (target_type = 'user_correction' and task_id is null and command_profile_id is null and review_policy_id is null and design_version_id is null and decision_id is null and user_correction_id is not null)
            )
        );

        insert into kpt_item_conversions(
            id, kpt_item_id, target_type, task_id, command_profile_id,
            review_policy_id, design_version_id, decision_id, user_correction_id, created_at
        )
        select
            id, kpt_item_id, target_type, task_id, command_profile_id,
            review_policy_id, design_version_id, decision_id, user_correction_id, created_at
        from kpt_item_conversions_old;

        drop table kpt_item_conversions_old;
        drop table kpt_items_old;

        pragma foreign_keys = on;
        "#,
    )?;

    let invalid_source_correction_ids = {
        let mut stmt = conn.prepare(
            r#"
            select c.id, c.affected_surfaces
            from closures c
            join findings f on f.id = c.finding_id
            join review_runs r on r.id = f.review_run_id
            join review_plans p on p.id = r.review_plan_id
            where c.status = 'registered'
              and f.status = 'open' and f.classification = 'valid'
              and not (
                p.required = 1 and p.stage = 'close-ready'
                and p.review_type in ('implementation_review', 'design_implementation_diff')
              )
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        rows.filter_map(|row| match row {
            Ok((_id, Some(surfaces)))
                if crate::review::validate_correction_surfaces(&surfaces).is_ok() =>
            {
                None
            }
            Ok((id, _)) => Some(Ok(id)),
            Err(error) => Some(Err(error)),
        })
        .collect::<rusqlite::Result<Vec<_>>>()?
    };
    for closure_id in invalid_source_correction_ids {
        conn.execute(
            "update closures set status = 'incomplete' where id = ?1 and status = 'registered'",
            params![closure_id],
        )?;
    }

    Ok(())
}

pub(super) fn migrate_review_runs(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "review_runs")? {
        return Ok(());
    }

    let invalid_count: i64 = conn.query_row(
        r#"
        select count(*)
        from review_runs
        where not (
            (run_type = 'fresh' and run_purpose = 'new_unbiased_review')
            or (run_type = 'resume' and run_purpose = 'finding_fix_verification')
            or (run_type = 'coverage' and run_purpose = 'coverage_audit')
        )
        "#,
        [],
        |row| row.get(0),
    )?;
    if invalid_count > 0 {
        bail!("review_runs contains invalid run_type/run_purpose combinations");
    }
    conn.execute_batch(
        r#"
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
        "#,
    )?;

    Ok(())
}

pub(super) fn migrate_review_runs_phase_targets(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "review_runs")? {
        return Ok(());
    }

    ensure_column(conn, "review_runs", "file_path", "text")?;
    ensure_column(conn, "review_runs", "symbol", "text")?;
    ensure_column(
        conn,
        "review_runs",
        "review_provenance",
        "text not null default 'self_recorded'",
    )?;
    ensure_column(conn, "review_runs", "review_provenance_ref", "text")?;
    ensure_column(conn, "review_runs", "phase_id", "integer")?;

    let table_sql: String = conn.query_row(
        "select sql from sqlite_schema where type = 'table' and name = 'review_runs'",
        [],
        |row| row.get(0),
    )?;
    if table_sql.contains("'phase'") {
        return Ok(());
    }

    conn.execute_batch(
        r#"
        pragma legacy_alter_table = on;
        alter table review_runs rename to review_runs_old;
        pragma legacy_alter_table = off;

        create table review_runs (
            id integer primary key,
            project_id integer not null references projects(id) on delete cascade,
            review_scope_id integer references review_scopes(id),
            review_plan_id integer not null references review_plans(id),
            run_type text not null check (run_type in ('fresh', 'resume', 'coverage')),
            run_purpose text not null check (run_purpose in ('new_unbiased_review', 'finding_fix_verification', 'coverage_audit')),
            target_type text not null check (target_type in ('design_version', 'design_requirement', 'task', 'work_unit', 'phase', 'repository_snapshot', 'file', 'symbol')),
            design_version_id integer references design_versions(id),
            design_requirement_id integer references design_requirements(id),
            task_id integer references tasks(id),
            work_unit_id integer references work_units(id),
            phase_id integer references work_phases(id),
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

        insert into review_runs(
            id, project_id, review_scope_id, review_plan_id, run_type, run_purpose,
            target_type, design_version_id, design_requirement_id, task_id,
            work_unit_id, phase_id, repository_snapshot_id, file_path, symbol,
            target_ref, prompt_deviations, result_summary, new_findings_count,
            carried_findings_checked, clean_run, review_provenance,
            review_provenance_ref, status, created_at
        )
        select
            id, project_id, review_scope_id, review_plan_id, run_type, run_purpose,
            target_type, design_version_id, design_requirement_id, task_id,
            work_unit_id, phase_id, repository_snapshot_id, file_path, symbol,
            target_ref, prompt_deviations, result_summary, new_findings_count,
            carried_findings_checked, clean_run, review_provenance,
            review_provenance_ref, status, created_at
        from review_runs_old;

        drop table review_runs_old;
        "#,
    )?;

    Ok(())
}

pub(super) fn migrate_resume_check_items(conn: &Connection) -> Result<()> {
    let table_sql = conn
        .query_row(
            "select sql from sqlite_schema where type = 'table' and name = 'resume_check_items'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    let Some(table_sql) = table_sql else {
        return Ok(());
    };
    if table_sql.contains("'active_tasks_current'")
        && table_sql.contains("'repository_heads_current'")
    {
        return Ok(());
    }

    conn.execute_batch(
        r#"
        pragma foreign_keys = off;

        alter table resume_check_items rename to resume_check_items_old;

        create table resume_check_items (
            id integer primary key,
            resume_check_id integer not null references resume_checks(id) on delete cascade,
            check_name text not null check (check_name in ('resume_target_suspended', 'snapshot_exists', 'suspend_reason_exists', 'next_action_exists', 'deeper_frames_closed', 'blocking_dependencies_clear', 'active_tasks_current', 'authority_refs_current', 'review_scope_refs_current', 'design_version_current', 'task_derivation_current', 'checklist_current', 'selected_gate_current', 'review_plan_current', 'open_findings_current', 'repository_heads_current', 'repository_state_current', 'assumptions_current')),
            result text not null check (result in ('pass', 'fail', 'not_checked', 'needs_evidence')),
            evidence_ref text,
            blocking_action text,
            details text
        );

        insert into resume_check_items(
            id, resume_check_id, check_name, result, evidence_ref, blocking_action, details
        )
        select id, resume_check_id, check_name, result, evidence_ref, blocking_action, details
        from resume_check_items_old;

        drop table resume_check_items_old;

        pragma foreign_keys = on;
        "#,
    )?;

    Ok(())
}

pub(super) fn ensure_phase_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(PHASE_SCHEMA)?;
    Ok(())
}

pub(super) fn ensure_phase_review_target_reference_triggers(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "work_phase_review_targets")? || !table_exists(conn, "review_runs")? {
        return Ok(());
    }
    conn.execute_batch(
        r#"
        drop trigger if exists trg_work_phase_review_target_referenced_update;
        drop trigger if exists trg_work_phase_review_target_referenced_delete;

        create trigger trg_work_phase_review_target_referenced_update
        before update of review_plan_id, phase_id on work_phase_review_targets
        for each row
        when exists (
            select 1
            from review_runs r
            where r.review_plan_id = old.review_plan_id
              and r.target_type = 'phase'
              and r.phase_id = old.phase_id
        )
        begin
            select raise(abort, 'work phase review target is referenced by review runs');
        end;

        create trigger trg_work_phase_review_target_referenced_delete
        before delete on work_phase_review_targets
        for each row
        when exists (
            select 1
            from review_runs r
            where r.review_plan_id = old.review_plan_id
              and r.target_type = 'phase'
              and r.phase_id = old.phase_id
        )
        begin
            select raise(abort, 'work phase review target is referenced by review runs');
        end;
        "#,
    )?;
    Ok(())
}
