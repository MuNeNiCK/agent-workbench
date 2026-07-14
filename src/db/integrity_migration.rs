use anyhow::{Result, bail};
use rusqlite::{Connection, params};

use super::project::*;

pub(super) fn validate_review_required_links(conn: &Connection) -> Result<()> {
    if table_exists(conn, "review_plans")? {
        let missing_policy_count: i64 = conn.query_row(
            "select count(*) from review_plans where review_policy_id is null",
            [],
            |row| row.get(0),
        )?;
        if missing_policy_count > 0 {
            bail!("review_plans contains rows without review_policy_id");
        }
    }
    if table_exists(conn, "review_runs")? {
        let missing_plan_count: i64 = conn.query_row(
            "select count(*) from review_runs where review_plan_id is null",
            [],
            |row| row.get(0),
        )?;
        if missing_plan_count > 0 {
            bail!("review_runs contains rows without review_plan_id");
        }
    }
    Ok(())
}

pub(super) fn refresh_review_integrity_triggers(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        drop trigger if exists trg_review_policy_referenced_update;
        drop trigger if exists trg_review_policy_resume_findings_update;
        drop trigger if exists trg_review_scope_referenced_update;
        drop trigger if exists trg_review_plan_policy_required_insert;
        drop trigger if exists trg_review_plan_policy_required_update;
        drop trigger if exists trg_review_plan_project_insert;
        drop trigger if exists trg_review_plan_project_update;
        drop trigger if exists trg_review_plan_type_insert;
        drop trigger if exists trg_review_plan_type_update;
        drop trigger if exists trg_review_plan_resume_policy_update;
        drop trigger if exists trg_review_run_plan_required_insert;
        drop trigger if exists trg_review_run_plan_required_update;
        drop trigger if exists trg_review_run_project_insert;
        drop trigger if exists trg_review_run_target_insert;
        drop trigger if exists trg_review_run_project_update;
        drop trigger if exists trg_review_run_target_update;
        drop trigger if exists trg_review_run_plan_target_insert;
        drop trigger if exists trg_review_run_plan_target_update;
        drop trigger if exists trg_review_run_type_purpose_insert;
        drop trigger if exists trg_review_run_type_purpose_update;
        drop trigger if exists trg_review_run_resume_policy_insert;
        drop trigger if exists trg_review_run_resume_policy_update;
        drop trigger if exists trg_review_run_result_insert;
        drop trigger if exists trg_review_run_result_update;
        drop trigger if exists trg_review_plan_target_project_insert;
        drop trigger if exists trg_review_plan_target_project_update;
        drop trigger if exists trg_review_plan_target_referenced_update;
        drop trigger if exists trg_review_plan_target_referenced_delete;
        drop trigger if exists trg_work_phase_review_target_referenced_update;
        drop trigger if exists trg_work_phase_review_target_referenced_delete;
        drop trigger if exists trg_finding_project_insert;
        drop trigger if exists trg_finding_project_update;
        drop trigger if exists trg_finding_clean_run_insert;
        drop trigger if exists trg_finding_clean_run_update;
        drop trigger if exists trg_finding_resume_policy_insert;
        drop trigger if exists trg_finding_resume_policy_update;
        drop trigger if exists trg_finding_review_type_insert;
        drop trigger if exists trg_finding_review_type_update;
        drop trigger if exists trg_closure_project_insert;
        drop trigger if exists trg_closure_project_update;
        drop trigger if exists trg_finding_verification_project_insert;
        drop trigger if exists trg_finding_verification_project_update;
        "#,
    )?;
    Ok(())
}

pub(super) fn drop_phase_review_target_reference_triggers(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        drop trigger if exists trg_work_phase_review_target_referenced_update;
        drop trigger if exists trg_work_phase_review_target_referenced_delete;
        "#,
    )?;
    Ok(())
}

pub(super) fn prepare_acceptance_records_for_schema(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "acceptance_records")? {
        return Ok(());
    }
    ensure_column(conn, "acceptance_records", "design_package_key", "text")?;
    ensure_column(conn, "acceptance_records", "design_file_path", "text")?;
    ensure_column(conn, "acceptance_records", "design_requirement_key", "text")?;
    ensure_column(conn, "acceptance_records", "coverage_item_id", "integer")?;
    ensure_column(conn, "acceptance_records", "finding_id", "integer")?;
    ensure_column(conn, "acceptance_records", "validation_gate_id", "integer")?;
    ensure_column(conn, "acceptance_records", "validation_run_id", "integer")?;
    ensure_column(
        conn,
        "acceptance_records",
        "repository_state_classification_id",
        "integer",
    )?;
    ensure_column(
        conn,
        "acceptance_records",
        "repository_snapshot_comparison_id",
        "integer",
    )?;
    ensure_column(conn, "acceptance_records", "review_plan_id", "integer")?;
    ensure_column(conn, "acceptance_records", "checklist_item_id", "integer")?;
    ensure_column(conn, "acceptance_records", "command_profile_id", "integer")?;
    ensure_column(conn, "acceptance_records", "command_usage_id", "integer")?;
    ensure_column(
        conn,
        "acceptance_records",
        "command_deviation_id",
        "integer",
    )?;
    ensure_column(conn, "acceptance_records", "rule_binding_id", "integer")?;
    ensure_column(conn, "acceptance_records", "stale_record_type", "text")?;
    ensure_column(conn, "acceptance_records", "stale_record_id", "integer")?;
    Ok(())
}

pub(super) fn prepare_review_runs_for_schema(conn: &Connection) -> Result<()> {
    if !table_exists(conn, "review_runs")? {
        return Ok(());
    }
    ensure_column(
        conn,
        "review_runs",
        "review_provenance",
        "text not null default 'self_recorded'",
    )?;
    ensure_column(conn, "review_runs", "review_provenance_ref", "text")?;
    Ok(())
}

pub(super) fn prepare_project_scoped_ledger_rows_for_schema(conn: &Connection) -> Result<()> {
    if table_exists(conn, "work_records")? {
        ensure_column(conn, "work_records", "project_id", "integer")?;
        if table_exists(conn, "work_units")? {
            conn.execute(
                r#"
                update work_records
                set project_id = (select project_id from work_units where id = work_records.work_unit_id)
                where project_id is null and work_unit_id is not null
                "#,
                [],
            )?;
        }
        if table_exists(conn, "projects")? {
            conn.execute(
                r#"
                update work_records
                set project_id = (select id from projects order by id limit 1)
                where project_id is null
                "#,
                [],
            )?;
        }
    }
    if table_exists(conn, "command_usages")? {
        ensure_column(conn, "command_usages", "project_id", "integer")?;
        if table_exists(conn, "command_profiles")? {
            conn.execute(
                r#"
                update command_usages
                set project_id = (select project_id from command_profiles where id = command_usages.command_profile_id)
                where project_id is null and command_profile_id is not null
                "#,
                [],
            )?;
        }
        if table_exists(conn, "work_units")? {
            conn.execute(
                r#"
                update command_usages
                set project_id = (select project_id from work_units where id = command_usages.work_unit_id)
                where project_id is null and work_unit_id is not null
                "#,
                [],
            )?;
        }
        if table_exists(conn, "work_unit_activations")? {
            conn.execute(
                r#"
                update command_usages
                set project_id = (select project_id from work_unit_activations where id = command_usages.work_unit_activation_id)
                where project_id is null and work_unit_activation_id is not null
                "#,
                [],
            )?;
        }
        if table_exists(conn, "repository_snapshots")? && table_exists(conn, "repositories")? {
            conn.execute(
                r#"
                update command_usages
                set project_id = (
                    select r.project_id
                    from repository_snapshots s
                    join repositories r on r.id = s.repository_id
                    where s.id = command_usages.repository_snapshot_id
                )
                where project_id is null and repository_snapshot_id is not null
                "#,
                [],
            )?;
        }
        if table_exists(conn, "projects")? {
            conn.execute(
                r#"
                update command_usages
                set project_id = (select id from projects order by id limit 1)
                where project_id is null
                "#,
                [],
            )?;
        }
    }
    Ok(())
}

pub(super) fn validate_project_scoped_ledger_links(conn: &Connection) -> Result<()> {
    reject_invalid_rows(
        conn,
        "work_records",
        r#"
        select count(*)
        from work_records wr
        left join work_units w on w.id = wr.work_unit_id
        where wr.project_id is null
           or not exists (select 1 from projects where id = wr.project_id)
           or (wr.work_unit_id is not null and (w.id is null or wr.project_id != w.project_id))
        "#,
        "work_records contains rows without a valid project_id",
    )?;
    reject_invalid_rows(
        conn,
        "command_usages",
        r#"
        select count(*)
        from command_usages cu
        left join command_profiles cp on cp.id = cu.command_profile_id
        left join work_units w on w.id = cu.work_unit_id
        left join work_unit_activations a on a.id = cu.work_unit_activation_id
        left join repository_snapshots s on s.id = cu.repository_snapshot_id
        left join repositories sr on sr.id = s.repository_id
        where cu.project_id is null
           or not exists (select 1 from projects where id = cu.project_id)
           or (cu.command_profile_id is not null and (cp.id is null or cu.project_id != cp.project_id))
           or (cu.work_unit_id is not null and (w.id is null or cu.project_id != w.project_id))
           or (cu.work_unit_activation_id is not null and (a.id is null or cu.project_id != a.project_id))
           or (
               cu.repository_snapshot_id is not null
               and (s.id is null or sr.id is null or cu.project_id != sr.project_id)
           )
        "#,
        "command_usages contains rows without a valid project_id",
    )?;
    reject_invalid_rows(
        conn,
        "validation_runs",
        r#"
        select count(*)
        from validation_runs vr
        left join validation_gates vg on vg.id = vr.validation_gate_id
        left join work_units w on w.id = vr.work_unit_id
        left join tasks t on t.id = vr.task_id
        left join command_usages cu on cu.id = vr.command_usage_id
        left join repository_snapshots s on s.id = vr.repository_snapshot_id
        left join repositories sr on sr.id = s.repository_id
        where vr.project_id is null
           or not exists (select 1 from projects where id = vr.project_id)
           or vg.id is null
           or vr.project_id != vg.project_id
           or vr.work_unit_id is not vg.work_unit_id
           or vr.task_id is not vg.task_id
           or (vr.work_unit_id is not null and (w.id is null or vr.project_id != w.project_id))
           or (
               vr.task_id is not null
               and (
                   t.id is null
                   or t.work_unit_id is null
                   or vr.project_id != (
                       select project_id from work_units where id = t.work_unit_id
                   )
               )
           )
           or (vr.command_usage_id is not null and (cu.id is null or vr.project_id != cu.project_id))
           or (
               vr.command_usage_id is not null
               and cu.work_unit_id is not null
               and cu.work_unit_id is not vr.work_unit_id
           )
           or (
               vr.repository_snapshot_id is not null
               and (s.id is null or sr.id is null or vr.project_id != sr.project_id)
           )
           or (
               vr.command_usage_id is not null
               and vr.repository_snapshot_id is not null
               and cu.repository_snapshot_id is not null
               and vr.repository_snapshot_id != cu.repository_snapshot_id
           )
        "#,
        "validation_runs contains invalid project links; run `agent-workbench doctor validation-links`, then `agent-workbench doctor validation-links --repair`",
    )?;
    reject_invalid_rows(
        conn,
        "artifacts",
        r#"
        select count(*)
        from artifacts a
        left join validation_runs vr on vr.id = a.validation_run_id
        left join command_usages cu on cu.id = a.command_usage_id
        left join repository_snapshots s on s.id = a.repository_snapshot_id
        left join repositories sr on sr.id = s.repository_id
        where a.project_id is null
           or not exists (select 1 from projects where id = a.project_id)
           or (a.validation_run_id is not null and (vr.id is null or a.project_id != vr.project_id))
           or (a.command_usage_id is not null and (cu.id is null or a.project_id != cu.project_id))
           or (
               a.repository_snapshot_id is not null
               and (s.id is null or sr.id is null or a.project_id != sr.project_id)
           )
           or (
               a.validation_run_id is not null
               and a.command_usage_id is not vr.command_usage_id
           )
           or (
               a.validation_run_id is not null
               and a.repository_snapshot_id is not vr.repository_snapshot_id
           )
        "#,
        "artifacts contains invalid validation links",
    )?;
    reject_invalid_rows(
        conn,
        "work_record_commands",
        r#"
        select count(*)
        from work_record_commands wrc
        left join work_records wr on wr.id = wrc.work_record_id
        left join command_usages cu on cu.id = wrc.command_usage_id
        left join command_profiles cp on cp.id = wrc.command_profile_id
        where wr.id is null
           or (wrc.command_usage_id is not null and (cu.id is null or wr.project_id != cu.project_id))
           or (wrc.command_profile_id is not null and (cp.id is null or wr.project_id != cp.project_id))
        "#,
        "work_record_commands contains cross-project links",
    )?;
    reject_invalid_rows(
        conn,
        "work_record_commits",
        r#"
        select count(*)
        from work_record_commits wrc
        left join work_records wr on wr.id = wrc.work_record_id
        left join git_commits gc on gc.id = wrc.git_commit_id
        left join repositories r on r.id = gc.repository_id
        where wr.id is null
           or (
               wrc.git_commit_id is not null
               and (
                   gc.id is null
                   or r.id is null
                   or wrc.commit_sha is null
                   or wrc.commit_sha != gc.commit_sha
                   or wr.project_id != r.project_id
               )
           )
        "#,
        "work_record_commits contains invalid git links",
    )?;
    reject_invalid_rows(
        conn,
        "work_record_files",
        r#"
        select count(*)
        from work_record_files wrf
        left join work_records wr on wr.id = wrf.work_record_id
        left join repositories r on r.id = wrf.repository_id
        left join git_file_changes gf on gf.id = wrf.git_file_change_id
        where wr.id is null
           or (wrf.repository_id is not null and (r.id is null or wr.project_id != r.project_id))
           or (
               wrf.git_file_change_id is not null
               and (
                   gf.id is null
                   or wrf.repository_id is null
                   or wrf.repository_id != gf.repository_id
                   or wrf.path != gf.path
               )
           )
        "#,
        "work_record_files contains invalid repository links",
    )?;
    reject_invalid_rows(
        conn,
        "work_record_forks",
        r#"
        select count(*)
        from work_record_forks f
        left join work_units forked on forked.id = f.forked_work_unit_id
        left join work_units source_w on source_w.id = f.source_work_unit_id
        left join work_unit_activations source_a on source_a.id = f.source_work_unit_activation_id
        left join work_records source_r on source_r.id = f.source_work_record_id
        left join repository_snapshots source_s on source_s.id = f.source_repository_snapshot_id
        left join repositories source_sr on source_sr.id = source_s.repository_id
        left join git_commits source_gc on source_gc.id = f.source_git_commit_id
        left join repositories source_gr on source_gr.id = source_gc.repository_id
        where f.project_id is null
           or forked.id is null
           or f.project_id != forked.project_id
           or (f.source_work_unit_id is not null and (source_w.id is null or f.project_id != source_w.project_id))
           or (f.source_work_unit_activation_id is not null and (source_a.id is null or f.project_id != source_a.project_id))
           or (f.source_work_record_id is not null and (source_r.id is null or f.project_id != source_r.project_id))
           or (
               f.source_repository_snapshot_id is not null
               and (source_s.id is null or source_sr.id is null or f.project_id != source_sr.project_id)
           )
           or (
               f.source_git_commit_id is not null
               and (
                   source_gc.id is null
                   or source_gr.id is null
                   or f.project_id != source_gr.project_id
                   or (f.source_git_commit_sha is not null and f.source_git_commit_sha != source_gc.commit_sha)
               )
           )
        "#,
        "work_record_forks contains invalid project links",
    )?;
    Ok(())
}

pub(super) fn reject_invalid_rows(
    conn: &Connection,
    table: &str,
    sql: &str,
    message: &'static str,
) -> Result<()> {
    if !table_exists(conn, table)? {
        return Ok(());
    }
    let count: i64 = conn.query_row(sql, [], |row| row.get(0))?;
    if count > 0 {
        bail!("{message}");
    }
    Ok(())
}

pub(super) fn migrate_work_record_auto_link_markers(
    conn: &Connection,
    had_work_record_commit_auto_linked: bool,
    had_work_record_file_auto_linked: bool,
    had_work_record_file_repository_auto_linked: bool,
) -> Result<()> {
    if !had_work_record_commit_auto_linked {
        conn.execute(
            r#"
            update work_record_commits
            set auto_linked = 1
            where git_commit_id is not null
            "#,
            [],
        )?;
    }
    if !had_work_record_file_auto_linked {
        conn.execute(
            r#"
            update work_record_files
            set auto_linked = 1
            where git_file_change_id is not null
            "#,
            [],
        )?;
    }
    if !had_work_record_file_repository_auto_linked {
        conn.execute(
            r#"
            update work_record_files
            set repository_auto_linked = 1
            where git_file_change_id is not null
              and ?1 = 0
            "#,
            params![i64::from(had_work_record_file_auto_linked)],
        )?;
    }

    Ok(())
}

pub(super) fn refresh_ledger_integrity_triggers(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        drop trigger if exists trg_command_usage_project_insert;
        drop trigger if exists trg_command_usage_project_update;
        drop trigger if exists trg_command_usage_repository_snapshot_insert;
        drop trigger if exists trg_command_usage_repository_snapshot_update;
        drop trigger if exists trg_work_record_project_insert;
        drop trigger if exists trg_work_record_project_update;
        drop trigger if exists trg_work_record_command_project_insert;
        drop trigger if exists trg_work_record_command_project_update;
        drop trigger if exists trg_work_record_commit_git_insert;
        drop trigger if exists trg_work_record_commit_git_update;
        drop trigger if exists trg_work_record_file_git_insert;
        drop trigger if exists trg_work_record_file_git_update;
        drop trigger if exists trg_work_record_fork_repository_git_insert;
        drop trigger if exists trg_work_record_fork_repository_git_update;
        drop trigger if exists trg_implementation_evidence_project_insert;
        drop trigger if exists trg_implementation_evidence_project_update;
        drop trigger if exists trg_validation_run_project_insert;
        drop trigger if exists trg_validation_run_project_update;
        drop trigger if exists trg_artifact_project_insert;
        drop trigger if exists trg_artifact_project_update;
        drop trigger if exists trg_repository_snapshot_referenced_delete;
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
        "#,
    )?;
    Ok(())
}
