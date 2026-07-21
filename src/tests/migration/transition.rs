use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::process::Command;

use crate::db::{default_ledger_path, open_ledger};
use crate::init_project;
use crate::update::transition::{
    RESET_SCHEMA_GENERATION, SourceObservation, StateDescriptor, StorageHeader, TransitionContext,
    TransitionEdge, UpdateRoute, classify_storage_header, classify_update_route, execute_adjacent,
    generation_13_to_14_edge, generation_15_to_16_edge, generation_16_to_17_edge,
    generation_17_to_18_edge, generation_21_to_22_edge, registered_historical_generations,
    registered_storage_path, resolve_path,
};

const SOURCE: StateDescriptor = StateDescriptor {
    key: "source-contract",
};

fn release_work(root: &Path, reviewed_commit: &str) -> i64 {
    let work = crate::start_work(root, "release boundary", None).unwrap();
    crate::create_work_record(
        root,
        crate::NewWorkRecord {
            work_unit_id: Some(work.work_unit_id),
            topic: "release boundary",
            work_performed: Some("recorded release qualification"),
            next_actions: None,
            notable_operations: None,
            export_path: None,
        },
    )
    .unwrap();
    crate::add_repository(
        root,
        crate::NewRepository {
            name: "release-source",
            path: ".",
            current_head: Some(reviewed_commit),
            status_summary: Some("clean"),
        },
    )
    .unwrap();
    crate::add_repository_snapshot(
        root,
        crate::NewRepositorySnapshot {
            repository: "release-source",
            work_unit_activation_id: Some(work.activation_id),
            head_sha: Some(reviewed_commit),
            branch: Some("main"),
            status_summary: Some("clean"),
            is_clean: true,
        },
    )
    .unwrap();
    assert_eq!(
        crate::close_ready_for(root, work.work_unit_id)
            .unwrap()
            .result,
        "pass"
    );
    work.work_unit_id
}

#[test]
fn newly_initialized_storage_is_the_registered_current_generation() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    assert_eq!(
        classify_update_route(&conn, temp.path()).unwrap(),
        UpdateRoute::Current
    );
}

#[test]
fn generation_22_update_preserves_continuations_and_materializes_pending_reviewer_sources() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let ledger = default_ledger_path(temp.path());
    let conn = open_ledger(&ledger).unwrap();
    let continuation = format!("continuation_{}", "a".repeat(64));
    let context = "b".repeat(64);
    let reviewer_digest = "c".repeat(64);
    conn.execute_batch(
        r#"
        drop trigger if exists trg_decision_continuation_insert;
        drop trigger if exists trg_decision_continuation_update;
        drop trigger if exists trg_decision_continuation_delete;
        drop table decision_continuations;
        create table decision_continuations (
          id integer primary key,
          project_id integer not null references projects(id) on delete cascade,
          continuation_handle text not null,
          command_kind text not null,
          owner_ref text not null,
          target_ref text not null,
          decision_family text not null,
          action text not null,
          expected_current text not null,
          design_context text not null check(length(design_context)=64),
          rejection_code text not null,
          status text not null check(status in ('pending','applied')),
          created_at text not null,
          applied_at text,
          unique(project_id,continuation_handle)
        );
        drop table reviewer_migration_bindings;
        drop table reviewer_migration_sources;
        drop table validation_link_repair_receipts;
        alter table review_agent_invocations add column legacy_source_reviewer_digest text;
        delete from schema_migrations where version>=23;
        "#,
    )
    .unwrap();
    conn.execute(
        "insert into decision_continuations(project_id,continuation_handle,command_kind,owner_ref,target_ref,decision_family,action,expected_current,design_context,rejection_code,status,created_at) values(1,?1,'decision adjudicate','work_unit:1','review_run:1','review','adjudicate','pending',?2,'accountable_input_required','pending',current_timestamp)",
        params![continuation, context],
    )
    .unwrap();
    conn.execute(
        "insert into review_agent_invocations(project_id,run_type,status,legacy_source_reviewer_digest) values(1,'fresh','completed',?1)",
        [reviewer_digest.as_str()],
    )
    .unwrap();
    drop(conn);

    let inspection = crate::inspect_update(temp.path()).unwrap();
    assert_eq!(inspection.status, "ready_to_apply");
    crate::apply_update_operation(
        temp.path(),
        &inspection.inspection_handle,
        &inspection.current_identity,
        "generation-22-public-owner-recovery",
    )
    .unwrap();
    let shown = crate::show_decision_continuation(temp.path(), &continuation).unwrap();
    assert_eq!(shown.status, "pending");
    assert_eq!(shown.context_identity, context);
    assert_eq!(shown.required_inputs, "decision,reason");
    let conn = open_ledger(&ledger).unwrap();
    let source: (String, String) = conn
        .query_row(
            "select source_reviewer_ref,status from reviewer_migration_sources where source_reviewer_digest=?1",
            [reviewer_digest.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        source,
        (
            format!("legacy-reviewer:{reviewer_digest}"),
            "pending".to_string()
        )
    );
    assert_eq!(
        crate::inspect_update(temp.path()).unwrap().status,
        "current"
    );
}

#[test]
fn generation_23_update_preserves_unbound_release_history_without_fabricating_a_boundary() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work_unit_id = release_work(temp.path(), "reviewed-commit");
    let candidate = crate::release::assemble_release_candidate(
        temp.path(),
        crate::release::NewReleaseCandidate {
            work_unit_id: Some(work_unit_id),
            version: "0.2.0".to_string(),
            reviewed_commit: "reviewed-commit".to_string(),
            idempotency_key: "generation-23-release-history".to_string(),
            subjects: release_subjects(),
        },
    )
    .unwrap();
    let ledger = default_ledger_path(temp.path());
    let conn = open_ledger(&ledger).unwrap();
    conn.execute_batch(
        r#"
        drop table release_candidate_boundaries;
        delete from schema_migrations where version>=24;
        "#,
    )
    .unwrap();
    drop(conn);

    let inspection = crate::inspect_update(temp.path()).unwrap();
    assert_eq!(inspection.status, "ready_to_apply");
    crate::apply_update_operation(
        temp.path(),
        &inspection.inspection_handle,
        &inspection.current_identity,
        "generation-23-release-work-boundaries",
    )
    .unwrap();
    let conn = open_ledger(&ledger).unwrap();
    assert_eq!(
        conn.query_row("select count(*) from release_candidates", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row(
            "select count(*) from release_candidate_boundaries",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    drop(conn);
    let preserved =
        crate::release::inspect_release_candidate(temp.path(), &candidate.candidate_handle)
            .unwrap();
    assert_eq!(preserved.work_unit_id, None);
    let error = match crate::release::start_release_attempt(
        temp.path(),
        &candidate.candidate_handle,
        &candidate.current_revision,
        "inspect",
        "legacy-unbound-inspection",
        "requested-observation",
    ) {
        Err(error) => error,
        Ok(_) => panic!("an unbound historical candidate must not start a new release effect"),
    };
    assert!(
        error
            .to_string()
            .contains("assemble a new candidate with --work")
    );
}

#[test]
fn current_storage_repairs_the_missing_plan_ingress_contract_through_update() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let conn = open_ledger(&default_ledger_path(temp.path())).unwrap();
    conn.execute_batch(
        r#"
        drop trigger trg_decomposition_plan_ingress_links_insert;
        drop trigger trg_decomposition_plan_ingress_immutable_update;
        drop trigger trg_decomposition_plan_ingress_immutable_delete;
        drop table decomposition_plan_ingress_identities;
        "#,
    )
    .unwrap();
    drop(conn);
    let inspection = crate::inspect_update(temp.path()).unwrap();
    assert_eq!(inspection.status, "ready_to_apply");
    crate::apply_update_operation(
        temp.path(),
        &inspection.inspection_handle,
        &inspection.current_identity,
        "repair-missing-plan-ingress-contract",
    )
    .unwrap();
    assert_eq!(
        crate::inspect_update(temp.path()).unwrap().status,
        "current"
    );
}

#[test]
fn current_storage_extends_the_finding_type_domain_only_through_update() {
    let temp = tempfile::tempdir().unwrap();
    init_project(temp.path()).unwrap();
    let work = crate::start_work(temp.path(), "finding type migration", None).unwrap();
    let plan = crate::add_review_plan(
        temp.path(),
        crate::NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: None,
            review_type: "general",
            required: false,
            stage: "implementation-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    let run = crate::add_review_run(
        temp.path(),
        crate::NewReviewRun {
            review_plan_id: plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: None,
            prompt_deviations: None,
            result_summary: Some("legacy finding"),
            new_findings_count: 1,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: Some("legacy-reviewer"),
            external_agent_id: Some("legacy-reviewer-1"),
            review_provenance: "external_agent",
            review_provenance_ref: Some("review-output:legacy-finding"),
        },
    )
    .unwrap();
    let legacy = crate::add_finding(
        temp.path(),
        crate::NewFinding {
            review_run_id: run.review_run_id,
            finding_type: "design_finding",
            severity: "high",
            description: "preserve this finding",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();

    let ledger = default_ledger_path(temp.path());
    let conn = open_ledger(&ledger).unwrap();
    crate::db::run_atomic_schema_migration(&conn, |tx| {
        tx.execute_batch(
            r#"
            pragma legacy_alter_table=on;
            alter table findings rename to findings_new_domain;
            pragma legacy_alter_table=off;
            create table findings (
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
            insert into findings select * from findings_new_domain;
            drop table findings_new_domain;
            "#,
        )?;
        Ok(())
    })
    .unwrap();
    drop(conn);

    let inspection = crate::inspect_update(temp.path()).unwrap();
    assert_eq!(inspection.status, "ready_to_apply");
    crate::apply_update_operation(
        temp.path(),
        &inspection.inspection_handle,
        &inspection.current_identity,
        "extend-finding-type-domain",
    )
    .unwrap();
    assert_eq!(
        crate::inspect_update(temp.path()).unwrap().status,
        "current"
    );
    assert_eq!(
        crate::list_findings(temp.path(), Some("open")).unwrap()[0].id,
        legacy.finding_id
    );
    let common = crate::add_finding(
        temp.path(),
        crate::NewFinding {
            review_run_id: run.review_run_id,
            finding_type: "process_finding",
            severity: "medium",
            description: "common finding type remains public",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    assert!(common.finding_id > legacy.finding_id);
}
const TARGET: StateDescriptor = StateDescriptor {
    key: "target-contract",
};
const LATER: StateDescriptor = StateDescriptor {
    key: "later-contract",
};

fn observe(
    conn: &Connection,
    _context: &TransitionContext<'_>,
) -> anyhow::Result<SourceObservation> {
    let revision = conn.query_row(
        "select value from update_test_state where key='revision'",
        [],
        |row| row.get::<_, String>(0),
    )?;
    let descriptor_key = conn.query_row(
        "select value from update_test_state where key='descriptor'",
        [],
        |row| row.get::<_, String>(0),
    )?;
    Ok(SourceObservation {
        descriptor_key,
        revision,
        historical_source: None,
        conservation: None,
        plans: Vec::new(),
        derived_bundle_count: 0,
        decomposition_projection: None,
        reconciliation_balance: None,
    })
}

fn apply(
    conn: &Connection,
    _source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> anyhow::Result<()> {
    conn.execute(
        "insert into update_test_target(source_id,payload) select id,payload from update_test_source",
        [],
    )?;
    conn.execute(
        "update update_test_state set value=?1 where key='descriptor'",
        params![TARGET.key],
    )?;
    Ok(())
}

fn validate(
    conn: &Connection,
    _source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> anyhow::Result<()> {
    let source_count: i64 =
        conn.query_row("select count(*) from update_test_source", [], |row| {
            row.get(0)
        })?;
    let target_count: i64 =
        conn.query_row("select count(*) from update_test_target", [], |row| {
            row.get(0)
        })?;
    anyhow::ensure!(
        source_count == target_count,
        "source facts were not conserved"
    );
    let descriptor: String = conn.query_row(
        "select value from update_test_state where key='descriptor'",
        [],
        |row| row.get(0),
    )?;
    anyhow::ensure!(descriptor == TARGET.key, "target descriptor is not current");
    Ok(())
}

fn fail_validation(
    _conn: &Connection,
    _source: &SourceObservation,
    _context: &TransitionContext<'_>,
) -> anyhow::Result<()> {
    anyhow::bail!("injected target validation failure")
}

fn edge(validate_target: crate::update::transition::ValidateTarget) -> TransitionEdge {
    TransitionEdge {
        key: "source-to-target",
        source: SOURCE,
        target: TARGET,
        observe_source: observe,
        apply,
        validate_target,
    }
}

fn ledger() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        r#"
        create table update_test_state(key text primary key,value text not null);
        insert into update_test_state values('descriptor','source-contract'),('revision','revision-one');
        create table update_test_source(id integer primary key,payload text not null);
        insert into update_test_source values(1,'first'),(2,'second');
        create table update_test_target(source_id integer primary key,payload text not null);
        "#,
    )
    .unwrap();
    conn
}

fn generation_13_ledger(root: &Path) -> Connection {
    crate::init_project(root).unwrap();
    let conn = crate::db::open_ledger(&crate::db::default_ledger_path(root)).unwrap();
    retain_deployed_core_storage(&conn);
    conn
}

fn generated_predecessor_ledger(root: &Path, subject: &TransitionEdge) -> Connection {
    crate::init_project(root).unwrap();
    let conn = crate::db::open_ledger(&crate::db::default_ledger_path(root)).unwrap();
    retain_deployed_core_storage(&conn);
    let context = TransitionContext { root };
    for edge in registered_storage_path(crate::db::CORE_SCHEMA_VERSION).unwrap() {
        if edge.key == subject.key {
            break;
        }
        let source = (edge.observe_source)(&conn, &context).unwrap();
        execute_adjacent(&conn, &edge, &source.revision, &context).unwrap();
    }
    (subject.observe_source)(&conn, &context).unwrap();
    conn
}

fn retain_deployed_core_storage(conn: &Connection) {
    crate::tests::retain_core_storage_only(conn);
    conn.execute_batch(
        r#"
        drop trigger if exists trg_decomposition_plan_ingress_links_insert;
        drop trigger if exists trg_decomposition_plan_ingress_immutable_update;
        drop trigger if exists trg_decomposition_plan_ingress_immutable_delete;
        drop table if exists decomposition_plan_ingress_identities;
        drop trigger if exists trg_decomposition_reconciliation_result_links_insert;
        drop trigger if exists trg_decomposition_reconciliation_result_immutable_update;
        drop trigger if exists trg_decomposition_reconciliation_result_immutable_delete;
        drop table if exists decomposition_reconciliation_results;
        "#,
    )
    .unwrap();
}

struct PendingLegacyReconciliationSource {
    conn: Connection,
    token_id: i64,
    closure_id: i64,
    design_version_id: i64,
    work_unit_id: i64,
    project_paths: Vec<String>,
}

fn pending_legacy_reconciliation_source(
    root: &Path,
    plan_names: &[&str],
) -> PendingLegacyReconciliationSource {
    crate::init_project(root).unwrap();
    let work = crate::start_work(root, "legacy reconciliation migration owner", None).unwrap();
    let package = crate::init_design_package(
        root,
        crate::NewDesignPackage {
            design_id: "legacy-reconciliation-migration",
            title: "Legacy Reconciliation Migration",
        },
    )
    .unwrap();
    std::fs::write(
        package.package_path.join("requirements/README.md"),
        crate::tests::requirement_doc_without_validation(
            "REQ-LEGACY",
            "Migrate a pending reconciliation path",
            "high",
        ),
    )
    .unwrap();
    std::fs::write(
        package.package_path.join("validation/gates.md"),
        crate::tests::validation_gate_doc("GATE-LEGACY")
            .replace("REQ-001", "REQ-LEGACY")
            .replace("GATE-001", "GATE-LEGACY"),
    )
    .unwrap();
    let imported = crate::import_design_package(
        root,
        crate::DesignPackageImport {
            package_path: &package.package_path,
            status: "draft",
        },
    )
    .unwrap();
    let review_plan = crate::add_review_plan(
        root,
        crate::NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(imported.design_version_id),
            review_type: "design_task_decomposition",
            required: true,
            stage: "implementation-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    let review_target = format!(
        "review-context:design-task-decomposition:design={}:work={}",
        imported.design_version_id, work.work_unit_id
    );
    let run = crate::review::add_review_run(
        root,
        crate::NewReviewRun {
            review_plan_id: review_plan.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some(&review_target),
            prompt_deviations: None,
            result_summary: Some("legacy pending target requires a formal update"),
            new_findings_count: 1,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
            review_provenance: "self_recorded",
            review_provenance_ref: None,
        },
    )
    .unwrap();
    let finding = crate::add_finding(
        root,
        crate::NewFinding {
            review_run_id: run.review_run_id,
            finding_type: "design_task_gap",
            severity: "high",
            description: "migrate the pending reconciliation path without losing its owner",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    crate::classify_finding(root, finding.finding_id, "valid").unwrap();
    let mut project_paths = Vec::new();
    let mut surfaces = Vec::new();
    for plan_name in plan_names {
        let relative = format!("plans/{plan_name}.md");
        let path = package.package_path.join(&relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("# {plan_name}\n")).unwrap();
        project_paths.push(
            path.strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .into_owned(),
        );
        surfaces.push(format!("design:edit:{relative}"));
    }
    let authorized_path = project_paths.first().unwrap();
    surfaces.push(format!(
        "transition:decomposition-plan-reconcile:{}/{}/{}",
        imported.design_version_id,
        work.work_unit_id,
        crate::review::encode_opaque_component(authorized_path)
    ));
    let closure = crate::add_closure(
        root,
        crate::NewClosure {
            finding_id: finding.finding_id,
            design_invariant: "the formal update retains one exact authorized Plan path",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some(&surfaces.join(",")),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("migrate the pending target through the registered transition"),
            tests_or_gates: Some("formal adjacent update"),
            verification_plan: Some("inspect the canonical target and unchanged source facts"),
            closed_by_commit: None,
        },
    )
    .unwrap();
    let edge = generation_21_to_22_edge();
    let conn = generated_predecessor_ledger(root, &edge);
    let token_id: i64 = conn
        .query_row(
            "select id from correction_tokens where closure_id=?1 and operation='decomposition-plan-reconcile'",
            [closure.closure_id],
            |row| row.get(0),
        )
        .unwrap();
    rewrite_immutable_token_target(
        &conn,
        token_id,
        &format!("{}/{}", imported.design_version_id, work.work_unit_id),
    );
    PendingLegacyReconciliationSource {
        conn,
        token_id,
        closure_id: closure.closure_id,
        design_version_id: imported.design_version_id,
        work_unit_id: work.work_unit_id,
        project_paths,
    }
}

fn rewrite_immutable_token_target(conn: &Connection, token_id: i64, target: &str) {
    let trigger: String = conn
        .query_row(
            "select sql from sqlite_schema where type='trigger' and name='trg_correction_token_links_update'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    conn.execute_batch("drop trigger trg_correction_token_links_update;")
        .unwrap();
    conn.execute(
        "update correction_tokens set target=?1 where id=?2",
        params![target, token_id],
    )
    .unwrap();
    conn.execute_batch(&trigger).unwrap();
}

fn remove_immutable_design_surfaces(conn: &Connection, closure_id: i64) {
    let trigger: String = conn
        .query_row(
            "select sql from sqlite_schema where type='trigger' and name='trg_correction_token_immutable_delete'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    conn.execute_batch("drop trigger trg_correction_token_immutable_delete;")
        .unwrap();
    conn.execute(
        "delete from correction_tokens where closure_id=?1 and token_kind='file'",
        [closure_id],
    )
    .unwrap();
    conn.execute_batch(&trigger).unwrap();
}

#[test]
fn pending_legacy_reconciliation_targets_migrate_only_from_one_exact_surface() {
    let exact = tempfile::tempdir().unwrap();
    let exact_source = pending_legacy_reconciliation_source(exact.path(), &["authorized"]);
    let expected_target = format!(
        "{}/{}/{}",
        exact_source.design_version_id,
        exact_source.work_unit_id,
        crate::review::encode_opaque_component(&exact_source.project_paths[0])
    );
    drop(exact_source.conn);
    let inspection = crate::inspect_update(exact.path()).unwrap();
    assert_eq!(inspection.status, "ready_to_apply");
    let applied = crate::apply_update_operation(
        exact.path(),
        &inspection.inspection_handle,
        &inspection.current_identity,
        "migrate-one-exact-pending-reconciliation-target",
    )
    .unwrap();
    let replay = crate::apply_update_operation(
        exact.path(),
        &inspection.inspection_handle,
        &inspection.current_identity,
        "migrate-one-exact-pending-reconciliation-target",
    )
    .unwrap();
    assert!(replay.already_applied);
    assert_eq!(replay.operation_handle, applied.operation_handle);
    let conn = crate::db::open_ledger(&crate::db::default_ledger_path(exact.path())).unwrap();
    assert_eq!(
        conn.query_row(
            "select target from correction_tokens where id=?1",
            [exact_source.token_id],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        expected_target
    );
    let membership_view: String = conn
        .query_row(
            "select sql from sqlite_schema where type='view' and name='correction_decomposition_task_memberships'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(membership_view.contains("decomposition_applications"));
    assert!(membership_view.contains("correction_transition_applications"));
    drop(conn);
    assert_eq!(
        crate::inspect_update(exact.path()).unwrap().status,
        "current"
    );

    let absent = tempfile::tempdir().unwrap();
    let absent_source = pending_legacy_reconciliation_source(absent.path(), &["removed"]);
    remove_immutable_design_surfaces(&absent_source.conn, absent_source.closure_id);
    let absent_target: String = absent_source
        .conn
        .query_row(
            "select target from correction_tokens where id=?1",
            [absent_source.token_id],
            |row| row.get(0),
        )
        .unwrap();
    drop(absent_source.conn);
    let absent_inspection = crate::inspect_update(absent.path()).unwrap();
    let absent_error = crate::apply_update_operation(
        absent.path(),
        &absent_inspection.inspection_handle,
        &absent_inspection.current_identity,
        "reject-absent-pending-reconciliation-surface",
    )
    .unwrap_err();
    assert!(
        format!("{absent_error:#}")
            .contains("has no contained same-closure design edit/create Plan path"),
        "{absent_error:#}"
    );
    let conn = crate::db::open_ledger(&crate::db::default_ledger_path(absent.path())).unwrap();
    assert_eq!(
        conn.query_row(
            "select target from correction_tokens where id=?1",
            [absent_source.token_id],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        absent_target
    );
    assert_eq!(
        conn.query_row("select max(version) from schema_migrations", [], |row| row
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        21
    );
    drop(conn);
    assert_eq!(
        crate::inspect_update(absent.path())
            .unwrap()
            .current_identity,
        absent_inspection.current_identity
    );

    let ambiguous = tempfile::tempdir().unwrap();
    let ambiguous_source =
        pending_legacy_reconciliation_source(ambiguous.path(), &["first", "second"]);
    let ambiguous_target: String = ambiguous_source
        .conn
        .query_row(
            "select target from correction_tokens where id=?1",
            [ambiguous_source.token_id],
            |row| row.get(0),
        )
        .unwrap();
    drop(ambiguous_source.conn);
    let ambiguous_inspection = crate::inspect_update(ambiguous.path()).unwrap();
    let ambiguous_error = crate::apply_update_operation(
        ambiguous.path(),
        &ambiguous_inspection.inspection_handle,
        &ambiguous_inspection.current_identity,
        "reject-ambiguous-pending-reconciliation-surface",
    )
    .unwrap_err();
    assert!(
        format!("{ambiguous_error:#}")
            .contains("has ambiguous contained same-closure design edit/create Plan paths"),
        "{ambiguous_error:#}"
    );
    let conn = crate::db::open_ledger(&crate::db::default_ledger_path(ambiguous.path())).unwrap();
    assert_eq!(
        conn.query_row(
            "select target from correction_tokens where id=?1",
            [ambiguous_source.token_id],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        ambiguous_target
    );
    assert_eq!(
        conn.query_row("select max(version) from schema_migrations", [], |row| row
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        21
    );
    drop(conn);
    assert_eq!(
        crate::inspect_update(ambiguous.path())
            .unwrap()
            .current_identity,
        ambiguous_inspection.current_identity
    );

    let inactive = tempfile::tempdir().unwrap();
    let inactive_source = pending_legacy_reconciliation_source(inactive.path(), &["historical"]);
    inactive_source
        .conn
        .execute(
            r#"
            insert into closures(
              project_id,finding_id,design_invariant,design_citations,
              implementation_evidence,affected_surfaces,same_invariant_search,
              other_violations_found,fix_plan,tests_or_gates,verification_plan,
              closed_by_commit,status,created_at
            )
            select project_id,finding_id,
                   'the successor contract owns any current reconciliation',
                   design_citations,implementation_evidence,affected_surfaces,
                   same_invariant_search,other_violations_found,fix_plan,
                   tests_or_gates,verification_plan,closed_by_commit,
                   'registered',current_timestamp
            from closures where id=?1
            "#,
            [inactive_source.closure_id],
        )
        .unwrap();
    let successor_closure_id = inactive_source.conn.last_insert_rowid();
    inactive_source
        .conn
        .execute(
            r#"
            update closures
            set status='superseded',superseded_by_closure_id=?1,
                superseded_at=current_timestamp,
                supersession_reason='preserve inactive legacy history'
            where id=?2
            "#,
            params![successor_closure_id, inactive_source.closure_id],
        )
        .unwrap();
    let inactive_target: String = inactive_source
        .conn
        .query_row(
            "select target from correction_tokens where id=?1",
            [inactive_source.token_id],
            |row| row.get(0),
        )
        .unwrap();
    drop(inactive_source.conn);
    let inactive_inspection = crate::inspect_update(inactive.path()).unwrap();
    crate::apply_update_operation(
        inactive.path(),
        &inactive_inspection.inspection_handle,
        &inactive_inspection.current_identity,
        "preserve-inactive-legacy-reconciliation-history",
    )
    .unwrap();
    let conn = crate::db::open_ledger(&crate::db::default_ledger_path(inactive.path())).unwrap();
    assert_eq!(
        conn.query_row(
            "select target from correction_tokens where id=?1",
            [inactive_source.token_id],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        inactive_target
    );
    assert_eq!(
        conn.query_row(
            "select status from closures where id=?1",
            [inactive_source.closure_id],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "superseded"
    );
    assert_eq!(
        conn.query_row(
            "select status from closures where id=?1",
            [successor_closure_id],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "registered"
    );
    drop(conn);
    assert_eq!(
        crate::inspect_update(inactive.path()).unwrap().status,
        "current"
    );
}

fn publish_later_historical_label(conn: &Connection) {
    conn.execute(
        "insert or ignore into schema_migrations(version,applied_at) values(17,current_timestamp)",
        [],
    )
    .unwrap();
}

fn release_subjects() -> Vec<crate::release::ReleaseSubjectInput> {
    [
        ("local", "package-version"),
        ("local", "lockfile"),
        ("local", "binary-version"),
        ("local", "wrapper"),
        ("local", "skill"),
        ("local", "license"),
        ("local", "source-archive"),
        ("local", "release-notes"),
        ("source", "tag"),
        ("release", "release"),
        ("asset", "agent-workbench-v0.2.0-linux-x86_64.tar.gz"),
        ("asset", "agent-workbench-v0.2.0-skill.tar.gz"),
        ("asset", "agent-workbench-v0.2.0-docs.tar.gz"),
        ("asset", "agent-workbench-v0.2.0-source.tar.gz"),
        ("asset", "agent-workbench-v0.2.0-release-metadata.txt"),
        ("asset", "agent-workbench-v0.2.0-checksums.txt"),
    ]
    .into_iter()
    .map(|(kind, name)| crate::release::ReleaseSubjectInput {
        kind: kind.to_string(),
        name: name.to_string(),
        expected_identity: format!("expected-{name}"),
    })
    .collect()
}

#[allow(clippy::too_many_arguments)]
fn insert_release_lineage_candidate(
    conn: &Connection,
    project: i64,
    suffix: &str,
    status: &str,
    state: &str,
    stage: &str,
    action: &str,
    predecessor_id: Option<i64>,
) -> i64 {
    conn.execute(
        r#"
        insert into release_candidates(
          project_id,candidate_handle,version,reviewed_commit,manifest_identity,status,
          predecessor_id,idempotency_key,created_at,updated_at
        ) values(?1,?2,?3,?4,?5,?6,?7,?8,current_timestamp,current_timestamp)
        "#,
        params![
            project,
            format!("candidate-{suffix}"),
            format!("version-{suffix}"),
            format!("commit-{suffix}"),
            format!("manifest-{suffix}"),
            status,
            predecessor_id,
            format!("assemble-{suffix}")
        ],
    )
    .unwrap();
    let candidate = conn.last_insert_rowid();
    conn.execute(
        r#"
        insert into release_candidate_revisions(
          project_id,release_candidate_id,revision_handle,revision,state,stage,action,
          request_identity,predecessor_id,head_state,reason,created_at
        ) values(?1,?2,?3,1,?4,?5,?6,?7,null,'current',null,current_timestamp)
        "#,
        params![
            project,
            candidate,
            format!("revision-{suffix}"),
            state,
            stage,
            action,
            format!("request-{suffix}")
        ],
    )
    .unwrap();
    let revision = conn.last_insert_rowid();
    conn.execute(
        r#"
        insert into release_candidate_subject_revisions(
          project_id,release_candidate_revision_id,subject_kind,subject_name,
          expected_identity,local_identity,requested_identity,observed_identity,
          downloaded_identity
        ) values(?1,?2,'local','manifest',?3,?3,null,null,null)
        "#,
        params![project, revision, format!("manifest-{suffix}")],
    )
    .unwrap();
    candidate
}

fn insert_legacy_release_candidate(
    conn: &Connection,
    project: i64,
    suffix: &str,
    status: &str,
    predecessor_id: Option<i64>,
) -> i64 {
    conn.execute(
        r#"
        insert into release_candidates(
          project_id,candidate_handle,version,reviewed_commit,manifest_identity,status,
          predecessor_id,idempotency_key,created_at,updated_at
        ) values(?1,?2,?3,?4,?5,?6,?7,?8,current_timestamp,current_timestamp)
        "#,
        params![
            project,
            format!("legacy-candidate-{suffix}"),
            format!("legacy-version-{suffix}"),
            format!("legacy-commit-{suffix}"),
            format!("legacy-manifest-{suffix}"),
            status,
            predecessor_id,
            format!("legacy-assemble-{suffix}")
        ],
    )
    .unwrap();
    let candidate = conn.last_insert_rowid();
    conn.execute(
        r#"
        insert into release_candidate_assets(
          project_id,release_candidate_id,asset_name,expected_identity,local_identity,
          remote_identity,status,created_at,updated_at
        ) values(?1,?2,?3,?4,?4,null,'locally_verified',current_timestamp,current_timestamp)
        "#,
        params![
            project,
            candidate,
            format!("legacy-asset-{suffix}"),
            format!("legacy-identity-{suffix}")
        ],
    )
    .unwrap();
    candidate
}

fn assert_generation_16_lineage_rejected(conn: &Connection, edge: &TransitionEdge, root: &Path) {
    let context = TransitionContext { root };
    let source = (edge.observe_source)(conn, &context).unwrap();
    let error = execute_adjacent(conn, edge, &source.revision, &context).unwrap_err();
    assert!(
        error.to_string().contains(
            "transition install-explicit-reconciliation-effects target validation failed"
        )
    );
    assert_eq!(
        conn.query_row("select max(version) from schema_migrations", [], |row| row
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        16
    );
}

#[test]
fn release_supersession_lineage_requires_one_terminal_current_history() {
    let valid = tempfile::tempdir().unwrap();
    let edge = generation_16_to_17_edge();
    let conn = generated_predecessor_ledger(valid.path(), &edge);
    let project: i64 = conn
        .query_row("select id from projects", [], |row| row.get(0))
        .unwrap();
    let predecessor = insert_release_lineage_candidate(
        &conn,
        project,
        "valid-predecessor",
        "superseded",
        "superseded",
        "terminal",
        "supersede",
        None,
    );
    insert_release_lineage_candidate(
        &conn,
        project,
        "valid-successor",
        "assembled",
        "assembled",
        "local",
        "assemble",
        Some(predecessor),
    );
    let context = TransitionContext { root: valid.path() };
    let source = (edge.observe_source)(&conn, &context).unwrap();
    execute_adjacent(&conn, &edge, &source.revision, &context).unwrap();
    assert_eq!(
        conn.query_row("select max(version) from schema_migrations", [], |row| row
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        17
    );

    let partial = tempfile::tempdir().unwrap();
    let edge = generation_16_to_17_edge();
    let conn = generated_predecessor_ledger(partial.path(), &edge);
    let project: i64 = conn
        .query_row("select id from projects", [], |row| row.get(0))
        .unwrap();
    let predecessor = insert_release_lineage_candidate(
        &conn,
        project,
        "partial-predecessor",
        "superseded",
        "locally_verified",
        "source",
        "inspect",
        None,
    );
    insert_release_lineage_candidate(
        &conn,
        project,
        "partial-successor",
        "assembled",
        "assembled",
        "local",
        "assemble",
        Some(predecessor),
    );
    assert_generation_16_lineage_rejected(&conn, &edge, partial.path());

    let missing_edge = tempfile::tempdir().unwrap();
    let edge = generation_16_to_17_edge();
    let conn = generated_predecessor_ledger(missing_edge.path(), &edge);
    let project: i64 = conn
        .query_row("select id from projects", [], |row| row.get(0))
        .unwrap();
    insert_release_lineage_candidate(
        &conn,
        project,
        "missing-edge",
        "superseded",
        "superseded",
        "terminal",
        "supersede",
        None,
    );
    assert_generation_16_lineage_rejected(&conn, &edge, missing_edge.path());

    let foreign = tempfile::tempdir().unwrap();
    let edge = generation_16_to_17_edge();
    let conn = generated_predecessor_ledger(foreign.path(), &edge);
    let project: i64 = conn
        .query_row("select id from projects", [], |row| row.get(0))
        .unwrap();
    let predecessor = insert_release_lineage_candidate(
        &conn,
        project,
        "foreign-predecessor",
        "superseded",
        "superseded",
        "terminal",
        "supersede",
        None,
    );
    conn.execute(
        "insert into projects(name,root_path,created_at,updated_at) values('foreign-lineage','/foreign-lineage',current_timestamp,current_timestamp)",
        [],
    )
    .unwrap();
    let foreign_project = conn.last_insert_rowid();
    insert_release_lineage_candidate(
        &conn,
        foreign_project,
        "foreign-successor",
        "assembled",
        "assembled",
        "local",
        "assemble",
        Some(predecessor),
    );
    assert_generation_16_lineage_rejected(&conn, &edge, foreign.path());

    let cyclic = tempfile::tempdir().unwrap();
    let edge = generation_16_to_17_edge();
    let conn = generated_predecessor_ledger(cyclic.path(), &edge);
    let project: i64 = conn
        .query_row("select id from projects", [], |row| row.get(0))
        .unwrap();
    let first = insert_release_lineage_candidate(
        &conn,
        project,
        "cycle-first",
        "superseded",
        "superseded",
        "terminal",
        "supersede",
        None,
    );
    let second = insert_release_lineage_candidate(
        &conn,
        project,
        "cycle-second",
        "superseded",
        "superseded",
        "terminal",
        "supersede",
        Some(first),
    );
    conn.execute(
        "update release_candidates set predecessor_id=?1 where id=?2",
        params![second, first],
    )
    .unwrap();
    assert_generation_16_lineage_rejected(&conn, &edge, cyclic.path());
}

#[test]
fn legacy_supersession_lineage_updates_through_the_registered_current_path() {
    let temp = tempfile::tempdir().unwrap();
    let first_edge = generation_15_to_16_edge();
    let conn = generated_predecessor_ledger(temp.path(), &first_edge);
    let project: i64 = conn
        .query_row("select id from projects", [], |row| row.get(0))
        .unwrap();
    let predecessor =
        insert_legacy_release_candidate(&conn, project, "predecessor", "superseded", None);
    let successor = insert_legacy_release_candidate(
        &conn,
        project,
        "successor",
        "assembled",
        Some(predecessor),
    );

    let context = TransitionContext { root: temp.path() };
    for edge in registered_storage_path(15).unwrap() {
        let source = (edge.observe_source)(&conn, &context).unwrap();
        execute_adjacent(&conn, &edge, &source.revision, &context).unwrap();
    }
    assert_eq!(
        classify_update_route(&conn, temp.path()).unwrap(),
        UpdateRoute::Current
    );
    assert_eq!(
        conn.query_row("select max(version) from schema_migrations", [], |row| row
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        crate::db::SCHEMA_VERSION
    );
    let (state, stage, action): (String, String, String) = conn
        .query_row(
            r#"
            select revision.state,revision.stage,revision.action
            from release_candidate_revisions revision
            where revision.release_candidate_id=?1 and revision.head_state='current'
            "#,
            [predecessor],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (state.as_str(), stage.as_str(), action.as_str()),
        ("superseded", "terminal", "supersede")
    );
    assert_eq!(
        conn.query_row(
            "select predecessor_id from release_candidates where id=?1",
            [successor],
            |row| row.get::<_, Option<i64>>(0)
        )
        .unwrap(),
        Some(predecessor)
    );

    let cyclic = tempfile::tempdir().unwrap();
    let edge = generation_15_to_16_edge();
    let conn = generated_predecessor_ledger(cyclic.path(), &edge);
    let project: i64 = conn
        .query_row("select id from projects", [], |row| row.get(0))
        .unwrap();
    let first = insert_legacy_release_candidate(&conn, project, "cycle-first", "superseded", None);
    let second =
        insert_legacy_release_candidate(&conn, project, "cycle-second", "superseded", Some(first));
    conn.execute(
        "update release_candidates set predecessor_id=?1 where id=?2",
        params![second, first],
    )
    .unwrap();
    let context = TransitionContext {
        root: cyclic.path(),
    };
    let source = (edge.observe_source)(&conn, &context).unwrap();
    let error = execute_adjacent(&conn, &edge, &source.revision, &context).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("transition install-release-candidate-lifecycle did not apply")
    );
    assert_eq!(
        conn.query_row("select max(version) from schema_migrations", [], |row| row
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        15
    );
}

#[test]
fn valid_release_candidate_mutation_keeps_the_storage_current() {
    let temp = tempfile::tempdir().unwrap();
    crate::init_project(temp.path()).unwrap();
    let work_unit_id = release_work(temp.path(), "reviewed-commit");
    let candidate = crate::release::assemble_release_candidate(
        temp.path(),
        crate::release::NewReleaseCandidate {
            work_unit_id: Some(work_unit_id),
            version: "0.2.0".to_string(),
            reviewed_commit: "reviewed-commit".to_string(),
            idempotency_key: "assemble-current-storage".to_string(),
            subjects: release_subjects(),
        },
    )
    .unwrap();

    let conn = crate::db::open_ledger(&crate::db::default_ledger_path(temp.path())).unwrap();
    assert_eq!(
        classify_update_route(&conn, temp.path()).unwrap(),
        UpdateRoute::Current
    );
    drop(conn);
    let attempt = crate::release::start_release_attempt(
        temp.path(),
        &candidate.candidate_handle,
        &candidate.current_revision,
        "inspect",
        "inspect-current-storage",
        "requested-observation",
    )
    .unwrap();
    let crate::release::ReleaseAttemptStart::Ready { attempt_id, .. } = attempt else {
        panic!("fresh release attempt must be ready");
    };
    let conn = crate::db::open_ledger(&crate::db::default_ledger_path(temp.path())).unwrap();
    assert_eq!(
        classify_update_route(&conn, temp.path()).unwrap(),
        UpdateRoute::Current
    );
    drop(conn);
    let inspection =
        crate::release::inspect_release_candidate(temp.path(), &candidate.candidate_handle)
            .unwrap();
    let observations = inspection
        .subjects
        .iter()
        .filter(|subject| matches!(subject.kind.as_str(), "local" | "asset"))
        .map(|subject| crate::release::ReleaseObservation {
            name: subject.name.clone(),
            identity: subject.expected_identity.clone(),
        })
        .collect();
    let verified = crate::release::verify_release_locally(
        temp.path(),
        &candidate.candidate_handle,
        &candidate.current_revision,
        "inspect-current-storage",
        observations,
    )
    .unwrap();
    let conn = crate::db::open_ledger(&crate::db::default_ledger_path(temp.path())).unwrap();
    assert_eq!(
        classify_update_route(&conn, temp.path()).unwrap(),
        UpdateRoute::Current
    );
    drop(conn);
    crate::release::finish_release_attempt(
        temp.path(),
        &candidate.candidate_handle,
        attempt_id,
        "requested-observation",
        &verified,
        false,
    )
    .unwrap();
    let conn = crate::db::open_ledger(&crate::db::default_ledger_path(temp.path())).unwrap();
    assert_eq!(
        classify_update_route(&conn, temp.path()).unwrap(),
        UpdateRoute::Current
    );
    drop(conn);

    let published = crate::release::publish_release_source_with_action(
        temp.path(),
        &candidate.candidate_handle,
        &verified.current_revision,
        "publish-source-current-storage",
        "publish-source",
        vec![crate::release::ReleaseObservation {
            name: "tag".to_string(),
            identity: "expected-tag".to_string(),
        }],
        vec![crate::release::ReleaseObservation {
            name: "tag".to_string(),
            identity: "expected-tag".to_string(),
        }],
    )
    .unwrap();
    let interrupted = crate::release::start_release_attempt(
        temp.path(),
        &candidate.candidate_handle,
        &published.current_revision,
        "publish-assets",
        "publish-assets-interrupted",
        "requested-assets",
    )
    .unwrap();
    assert!(matches!(
        interrupted,
        crate::release::ReleaseAttemptStart::Ready { .. }
    ));
    let reconciliation = crate::release::start_release_attempt(
        temp.path(),
        &candidate.candidate_handle,
        &published.current_revision,
        "reconcile",
        "reconcile-absent-assets",
        "requested-reconciliation",
    )
    .unwrap();
    let crate::release::ReleaseAttemptStart::Ready {
        attempt_id: reconciliation_id,
        ..
    } = reconciliation
    else {
        panic!("fresh reconciliation attempt must be ready");
    };
    let reconciled = crate::release::record_release_absent(
        temp.path(),
        &candidate.candidate_handle,
        &published.current_revision,
        "reconcile-absent-assets",
        "source_published",
    )
    .unwrap();
    crate::release::finish_release_attempt(
        temp.path(),
        &candidate.candidate_handle,
        reconciliation_id,
        "absent",
        &reconciled,
        true,
    )
    .unwrap();
    let conn = crate::db::open_ledger(&crate::db::default_ledger_path(temp.path())).unwrap();
    assert_eq!(
        classify_update_route(&conn, temp.path()).unwrap(),
        UpdateRoute::Current
    );
}

#[test]
fn invalid_historical_release_attempts_have_one_public_recovery_action() {
    let temp = tempfile::tempdir().unwrap();
    crate::init_project(temp.path()).unwrap();
    let work_unit_id = release_work(temp.path(), "reviewed-commit");
    let candidate = crate::release::assemble_release_candidate(
        temp.path(),
        crate::release::NewReleaseCandidate {
            work_unit_id: Some(work_unit_id),
            version: "0.2.0".to_string(),
            reviewed_commit: "reviewed-commit".to_string(),
            idempotency_key: "assemble-invalid-attempt-recovery".to_string(),
            subjects: release_subjects(),
        },
    )
    .unwrap();

    crate::release::start_release_attempt(
        temp.path(),
        &candidate.candidate_handle,
        &candidate.current_revision,
        "withdraw",
        "historical-invalid-withdraw",
        "invalid-withdrawal",
    )
    .unwrap();
    let inspection =
        crate::release::inspect_release_candidate(temp.path(), &candidate.candidate_handle)
            .unwrap();
    assert!(
        inspection
            .next_action
            .contains(" operator release reconcile ")
    );
    let recovered = crate::release_operator::operator_reconcile_release(
        temp.path(),
        crate::release_operator::OperatorReleaseMutation {
            candidate: candidate.candidate_handle,
            expected_current: candidate.current_revision,
            idempotency_key: "historical-invalid-withdraw-reconcile".to_string(),
        },
    )
    .unwrap();
    assert_eq!(recovered.state, "assembled");
    assert!(recovered.next_action.contains(" candidate inspect "));
}

fn add_invalid_validation_history(conn: &Connection) {
    conn.execute_batch(
        r#"
        insert into projects(id,name,root_path,created_at,updated_at)
        values(900001,'other-generated-project','/generated-other',current_timestamp,current_timestamp);
        insert into validation_gates(id,project_id,gate_key,expected_result,status,created_at)
        values(900001,1,'generated-structural-gate','pass','active',current_timestamp);
        drop trigger trg_validation_run_project_insert;
        drop trigger trg_validation_run_project_update;
        insert into validation_runs(id,project_id,validation_gate_id,result,created_at)
        values(900001,900001,900001,'pass',current_timestamp);
        "#,
    )
    .unwrap();
}

fn public_binary(root: &Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args([
            "run",
            "--quiet",
            "--locked",
            "--bin",
            "agent-workbench",
            "--",
        ])
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .expect("failed to run the public agent-workbench binary");
    assert!(
        output.status.success(),
        "public command failed: {args:?}\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn public_value<'a>(output: &'a str, key: &str) -> &'a str {
    output
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}: ")))
        .unwrap_or_else(|| panic!("missing {key} in public output:\n{output}"))
}

#[test]
fn adjacent_transition_publishes_only_after_target_conservation_passes() {
    let conn = ledger();
    let receipt = execute_adjacent(
        &conn,
        &edge(validate),
        "revision-one",
        &TransitionContext {
            root: Path::new("."),
        },
    )
    .unwrap();
    assert_eq!(receipt.edge_key, "source-to-target");
    assert_eq!(receipt.source_revision, "revision-one");
    assert_eq!(receipt.target_descriptor, TARGET.key);
    assert_eq!(
        conn.query_row("select count(*) from update_test_target", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[test]
fn adjacent_transition_failure_rolls_back_every_target_effect() {
    let conn = ledger();
    let error = execute_adjacent(
        &conn,
        &edge(fail_validation),
        "revision-one",
        &TransitionContext {
            root: Path::new("."),
        },
    )
    .unwrap_err();
    assert!(error.to_string().contains("target validation failed"));
    assert_eq!(
        conn.query_row("select count(*) from update_test_target", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        observe(
            &conn,
            &TransitionContext {
                root: Path::new("."),
            }
        )
        .unwrap()
        .descriptor_key,
        SOURCE.key
    );
}

#[test]
fn transition_path_requires_one_declared_edge_per_adjacent_descriptor() {
    let first = edge(validate);
    let second = TransitionEdge {
        key: "target-to-later",
        source: TARGET,
        target: LATER,
        observe_source: observe,
        apply,
        validate_target: validate,
    };
    let states = [SOURCE, TARGET, LATER];
    let edges = [first, second];
    let path = resolve_path(&states, &edges, SOURCE.key, LATER.key).unwrap();
    assert_eq!(
        path.iter().map(|edge| edge.key).collect::<Vec<_>>(),
        ["source-to-target", "target-to-later",]
    );

    let duplicate = edge(validate);
    let error = resolve_path(
        &[SOURCE, TARGET, LATER],
        &[first, duplicate, second],
        SOURCE.key,
        LATER.key,
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("more than one declared transition")
    );
}

#[test]
fn storage_header_classification_is_structural_and_closed() {
    let full = Connection::open_in_memory().unwrap();
    full.execute_batch(&format!(
        "create table schema_migrations(version integer primary key,applied_at text not null);insert into schema_migrations values({},current_timestamp);",
        crate::db::SCHEMA_VERSION
    ))
    .unwrap();
    assert_eq!(
        classify_storage_header(&full).unwrap(),
        StorageHeader::Full {
            generation: crate::db::SCHEMA_VERSION
        }
    );

    let old = Connection::open_in_memory().unwrap();
    old.execute_batch(
        "create table schema_migrations(version integer primary key,applied_at text not null);insert into schema_migrations values(1,current_timestamp);",
    )
    .unwrap();
    assert_eq!(
        classify_storage_header(&old).unwrap(),
        StorageHeader::Full { generation: 1 }
    );

    let recovery = Connection::open_in_memory().unwrap();
    recovery
        .execute_batch(&format!(
            "create table schema_metadata(singleton integer primary key,schema_version integer not null);insert into schema_metadata values(1,{});create table projects(id integer primary key,root_path text not null);insert into projects values(1,'/tmp/project');",
            RESET_SCHEMA_GENERATION
        ))
        .unwrap();
    assert_eq!(
        classify_storage_header(&recovery).unwrap(),
        StorageHeader::Reset {
            generation: RESET_SCHEMA_GENERATION
        }
    );

    let newer = Connection::open_in_memory().unwrap();
    newer
        .execute_batch(&format!(
            "create table schema_migrations(version integer primary key);insert into schema_migrations values({});",
            crate::db::SCHEMA_VERSION + 1
        ))
        .unwrap();
    assert!(
        classify_storage_header(&newer)
            .unwrap_err()
            .to_string()
            .contains("newer than supported")
    );

    let contradictory = Connection::open_in_memory().unwrap();
    contradictory
        .execute_batch(
            "create table schema_migrations(version integer primary key);create table schema_metadata(singleton integer primary key,schema_version integer not null);",
        )
        .unwrap();
    assert!(
        classify_storage_header(&contradictory)
            .unwrap_err()
            .to_string()
            .contains("contradictory")
    );
}

#[test]
fn deployed_descriptor_ignores_later_label_and_validation_history() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let edge = generation_16_to_17_edge();
    let conn = generated_predecessor_ledger(&root, &edge);
    conn.execute_batch(
        "alter table decomposition_plans add column document_content text not null default ''",
    )
    .unwrap();
    publish_later_historical_label(&conn);
    add_invalid_validation_history(&conn);

    let route = classify_update_route(&conn, &root).unwrap();
    assert!(matches!(
        route,
        UpdateRoute::RegisteredPath {
            source_generation: 16,
            ..
        }
    ));
    drop(conn);

    let status = crate::project_status(&root).unwrap();
    let blocked = status
        .project_integrity
        .predicates
        .iter()
        .find(|predicate| predicate.result == "blocked")
        .unwrap();
    assert_eq!(blocked.code, "GI-002");
    assert_eq!(
        blocked.next_action.as_deref(),
        Some("agent-workbench update inspect")
    );

    let ordinary = crate::list_tasks(
        &root,
        crate::TaskListQuery {
            work_unit_id: None,
            status: None,
        },
    )
    .unwrap_err();
    assert!(ordinary.to_string().contains("project integrity GI-002"));
    assert!(
        ordinary
            .to_string()
            .contains("agent-workbench update inspect")
    );

    let inspection = public_binary(&root, &["update", "inspect"]);
    assert!(inspection.contains("update_status: ready_to_apply"));
    assert!(inspection.contains("next: agent-workbench update apply "));
}

#[test]
fn deployed_descriptor_with_later_label_applies_and_restores_semantically() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let edge = generation_16_to_17_edge();
    let conn = generated_predecessor_ledger(&root, &edge);
    conn.execute_batch(
        "alter table decomposition_plans add column document_content text not null default ''",
    )
    .unwrap();
    publish_later_historical_label(&conn);
    drop(conn);

    let inspection = public_binary(&root, &["update", "inspect"]);
    let inspection_handle = public_value(&inspection, "inspection_handle");
    let expected = public_value(&inspection, "current_identity");
    let applied = public_binary(
        &root,
        &[
            "update",
            "apply",
            inspection_handle,
            "--expected-current",
            expected,
            "--idempotency-key",
            "generated-later-label-apply",
        ],
    );
    let backup = public_value(&applied, "backup_identity");
    let current = public_binary(&root, &["update", "inspect"]);
    assert!(current.contains("update_status: current"));
    let conn = Connection::open(crate::db::default_ledger_path(&root)).unwrap();
    let marker_count: i64 = conn
        .query_row(
            "select count(*) from schema_migrations where version=17",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(marker_count, 1, "the existing history marker is preserved");
    drop(conn);

    let restored = public_binary(
        &root,
        &[
            "update",
            "restore",
            "--backup",
            backup,
            "--expected-current",
            public_value(&current, "current_identity"),
            "--idempotency-key",
            "generated-later-label-restore",
        ],
    );
    assert!(restored.contains("already_applied: false"));
    let restored_inspection = public_binary(&root, &["update", "inspect"]);
    assert!(restored_inspection.contains("update_status: ready_to_apply"));
}

#[test]
fn adjacent_storage_transition_conserves_predecessor_product_facts() {
    let temp = tempfile::tempdir().unwrap();
    let conn = generation_13_ledger(temp.path());
    let edge = generation_13_to_14_edge();
    let context = TransitionContext { root: temp.path() };
    let source = (edge.observe_source)(&conn, &context).unwrap();
    let receipt = execute_adjacent(&conn, &edge, &source.revision, &context).unwrap();

    assert_eq!(receipt.target_descriptor, edge.target.key);
    assert_eq!(
        conn.query_row("select max(version) from schema_migrations", [], |row| row
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        14
    );
    assert_eq!(
        conn.query_row("select count(*) from projects", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn adjacent_storage_edges_compose_to_the_current_target() {
    let temp = tempfile::tempdir().unwrap();
    let conn = generation_13_ledger(temp.path());
    let context = TransitionContext { root: temp.path() };

    for edge in registered_storage_path(crate::db::CORE_SCHEMA_VERSION).unwrap() {
        let source = (edge.observe_source)(&conn, &context).unwrap();
        execute_adjacent(&conn, &edge, &source.revision, &context).unwrap();
    }

    assert_eq!(
        conn.query_row("select max(version) from schema_migrations", [], |row| row
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        crate::db::SCHEMA_VERSION
    );
    assert_eq!(
        conn.query_row("select count(*) from decomposition_plans", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap(),
        0
    );
}

#[test]
fn every_supported_historical_state_updates_through_the_public_contract() {
    for generation in registered_historical_generations() {
        let temp = tempfile::tempdir().unwrap();
        crate::init_project(temp.path()).unwrap();
        let work = crate::start_work(
            temp.path(),
            &format!("generated migration owner {generation}"),
            None,
        )
        .unwrap();
        let title = format!("opaque task identity {generation} / 移行対象");
        let details = format!("arbitrary prose retained from source class {generation}");
        let completion = format!("observable outcome remains {generation}");
        let task = crate::add_task(
            temp.path(),
            crate::NewTask {
                title: &title,
                priority: "high",
                source: "user",
                work_unit_id: Some(work.work_unit_id),
                details: Some(&details),
                completion_condition: Some(&completion),
            },
        )
        .unwrap();
        let conn = crate::db::open_ledger(&crate::db::default_ledger_path(temp.path())).unwrap();
        retain_deployed_core_storage(&conn);
        conn.execute("delete from schema_migrations", []).unwrap();
        conn.execute(
            "insert into schema_migrations(version,applied_at) values(?1,current_timestamp)",
            [generation],
        )
        .unwrap();
        drop(conn);

        let inspection = crate::inspect_update(temp.path()).unwrap();
        assert_eq!(
            inspection.status, "ready_to_apply",
            "generation {generation}"
        );
        let applied = crate::apply_update_operation(
            temp.path(),
            &inspection.inspection_handle,
            &inspection.current_identity,
            "generated-structural-migration",
        )
        .unwrap();
        let replayed = crate::apply_update_operation(
            temp.path(),
            &inspection.inspection_handle,
            &inspection.current_identity,
            "generated-structural-migration",
        )
        .unwrap();
        assert!(replayed.already_applied, "generation {generation}");
        assert_eq!(replayed.operation_handle, applied.operation_handle);
        assert_eq!(replayed.result_identity, applied.result_identity);
        assert_eq!(
            crate::inspect_update(temp.path()).unwrap().status,
            "current"
        );

        let conn = crate::db::open_ledger(&crate::db::default_ledger_path(temp.path())).unwrap();
        let preserved: (String, String, String) = conn
            .query_row(
                "select title,details,completion_condition from tasks where id=?1",
                [task.task_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(preserved, (title, details, completion));
        let retirement_count: i64 = conn
            .query_row(
                "select count(*) from schema_retirement_records where source_generation=?1 and length(source_ledger_digest)=64",
                [generation],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(retirement_count, 1, "generation {generation}");
        drop(conn);

        let current_identity = crate::inspect_update(temp.path()).unwrap().current_identity;
        let restored = crate::restore_update_operation(
            temp.path(),
            &applied.backup_identity,
            &current_identity,
            "generated-structural-restore",
        )
        .unwrap();
        let restore_replay = crate::restore_update_operation(
            temp.path(),
            &applied.backup_identity,
            &current_identity,
            "generated-structural-restore",
        )
        .unwrap();
        assert!(restore_replay.already_applied, "generation {generation}");
        assert_eq!(restore_replay.operation_handle, restored.operation_handle);
        assert_eq!(
            crate::inspect_update(temp.path()).unwrap().status,
            "ready_to_apply"
        );
    }
}

#[test]
fn rejected_historical_update_leaves_the_source_ledger_unchanged() {
    let generation = registered_historical_generations()[0];
    let temp = tempfile::tempdir().unwrap();
    crate::init_project(temp.path()).unwrap();
    let work = crate::start_work(temp.path(), "rollback owner", None).unwrap();
    let _task = crate::add_task(
        temp.path(),
        crate::NewTask {
            title: "opaque rollback subject",
            priority: "high",
            source: "user",
            work_unit_id: Some(work.work_unit_id),
            details: Some("must survive a rejected migration"),
            completion_condition: Some("source bytes remain current"),
        },
    )
    .unwrap();
    let conn = crate::db::open_ledger(&crate::db::default_ledger_path(temp.path())).unwrap();
    retain_deployed_core_storage(&conn);
    conn.execute("delete from schema_migrations", []).unwrap();
    conn.execute(
        "insert into schema_migrations(version,applied_at) values(?1,current_timestamp)",
        [generation],
    )
    .unwrap();
    conn.pragma_update(None, "foreign_keys", false).unwrap();
    conn.execute(
        "update work_units set project_id=999 where id=?1",
        [work.work_unit_id],
    )
    .unwrap();
    conn.pragma_update(None, "foreign_keys", true).unwrap();
    drop(conn);

    let before = crate::inspect_update(temp.path()).unwrap();
    let error = crate::apply_update_operation(
        temp.path(),
        &before.inspection_handle,
        &before.current_identity,
        "generated-rejected-migration",
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("foreign key"), "{error:#}");
    let after = crate::inspect_update(temp.path()).unwrap();
    assert_eq!(after.current_identity, before.current_identity);
    assert_eq!(after.status, "ready_to_apply");
    let conn = crate::db::open_ledger(&crate::db::default_ledger_path(temp.path())).unwrap();
    let retired: i64 = conn
        .query_row(
            "select count(*) from schema_retirement_records where source_generation=?1",
            [generation],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(retired, 0);
}

#[test]
fn explicit_effect_transition_conserves_populated_reconciliation_mappings() {
    let temp = tempfile::tempdir().unwrap();
    crate::init_project(temp.path()).unwrap();
    let work = crate::start_work(temp.path(), "generated mapping owner", None).unwrap();
    let package = crate::init_design_package(
        temp.path(),
        crate::NewDesignPackage {
            design_id: "generated-effect-transition",
            title: "Generated Effect Transition",
        },
    )
    .unwrap();
    std::fs::write(
        package.package_path.join("requirements/README.md"),
        crate::tests::requirement_doc_without_validation(
            "REQ-MAP",
            "Preserve generated reconciliation endpoints",
            "high",
        ),
    )
    .unwrap();
    std::fs::write(
        package.package_path.join("validation/gates.md"),
        crate::tests::validation_gate_doc("GATE-MAP")
            .replace("REQ-001", "REQ-MAP")
            .replace("GATE-001", "GATE-MAP"),
    )
    .unwrap();
    let imported = crate::import_design_package(
        temp.path(),
        crate::DesignPackageImport {
            package_path: &package.package_path,
            status: "draft",
        },
    )
    .unwrap();
    crate::approve_design_version(
        temp.path(),
        crate::DesignVersionApproval {
            design_version_id: imported.design_version_id,
            summary: None,
        },
    )
    .unwrap();
    let retained = crate::add_task(
        temp.path(),
        crate::NewTask {
            title: "retained endpoint",
            priority: "high",
            source: "user",
            work_unit_id: Some(work.work_unit_id),
            details: Some("generated retained endpoint"),
            completion_condition: Some("the endpoint remains observable"),
        },
    )
    .unwrap();
    let retained_peer = crate::add_task(
        temp.path(),
        crate::NewTask {
            title: "retained peer endpoint",
            priority: "high",
            source: "user",
            work_unit_id: Some(work.work_unit_id),
            details: Some("generated retained peer endpoint"),
            completion_condition: Some("the peer endpoint remains observable"),
        },
    )
    .unwrap();
    let retired = crate::add_task(
        temp.path(),
        crate::NewTask {
            title: "retired endpoint",
            priority: "medium",
            source: "user",
            work_unit_id: Some(work.work_unit_id),
            details: Some("generated retired endpoint"),
            completion_condition: Some("the endpoint remains historical"),
        },
    )
    .unwrap();

    let mut source_checklist_items = Vec::new();
    let mut source_gates = Vec::new();
    for task in [retained.task_id, retained_peer.task_id, retired.task_id] {
        let derivation = crate::derive_task_from_requirement(
            temp.path(),
            crate::NewTaskDerivation {
                design_version_id: imported.design_version_id,
                requirement_key: "REQ-MAP",
                task_id: task,
                derivation_reason: Some("generate a populated mapping source"),
                checklist_title: Some("Generated mapping checklist"),
                item_title: Some("Generated mapping boundary"),
                completion_condition: Some("the generated endpoint remains observable"),
            },
        )
        .unwrap();
        source_checklist_items.push(derivation.checklist_item_id);
        source_gates.push(
            crate::select_validation_gate(
                temp.path(),
                crate::ValidationGateSelection {
                    design_version_id: imported.design_version_id,
                    gate_key: "GATE-MAP",
                    requirement_key: "REQ-MAP",
                    task_id: task,
                    command: None,
                    command_profile: None,
                    timeout: None,
                },
            )
            .unwrap()
            .validation_gate_id,
        );
    }
    let mut source_phases = Vec::new();
    for (key, order) in [("map-a", 1), ("map-b", 2), ("map-c", 3)] {
        source_phases.push(
            crate::create_phase(
                temp.path(),
                crate::NewWorkPhase {
                    work_unit_id: work.work_unit_id,
                    design_version_id: Some(imported.design_version_id),
                    key,
                    title: key,
                    kind: "implementation",
                    order,
                    reason: Some("generate a populated mapping source"),
                },
            )
            .unwrap()
            .phase_id,
        );
    }
    for (task, phase) in [
        (retained.task_id, source_phases[0]),
        (retained_peer.task_id, source_phases[1]),
        (retired.task_id, source_phases[2]),
    ] {
        crate::assign_task_to_phase(temp.path(), phase, task).unwrap();
    }
    let retained_dependency = crate::add_phase_dependency(
        temp.path(),
        crate::NewPhaseDependency {
            from_phase_id: source_phases[0],
            to_phase_id: source_phases[1],
            dependency_type: "requires",
            reason: "generated retained dependency",
        },
    )
    .unwrap()
    .dependency_id;
    let retired_dependency = crate::add_phase_dependency(
        temp.path(),
        crate::NewPhaseDependency {
            from_phase_id: source_phases[0],
            to_phase_id: source_phases[2],
            dependency_type: "requires",
            reason: "generated retired dependency",
        },
    )
    .unwrap()
    .dependency_id;

    let expected_tasks = crate::list_tasks(
        temp.path(),
        crate::TaskListQuery {
            status: None,
            work_unit_id: Some(work.work_unit_id),
        },
    )
    .unwrap();
    let expected_checklists = crate::list_checklists(temp.path(), None).unwrap();
    let expected_gates = crate::list_validation_gate_context(
        temp.path(),
        crate::ValidationGateContextQuery {
            design_version_id: imported.design_version_id,
            work_unit_id: Some(work.work_unit_id),
        },
    )
    .unwrap();
    let expected_phases = crate::list_phases(temp.path(), work.work_unit_id).unwrap();
    let expected_dependencies =
        crate::list_phase_dependencies(temp.path(), work.work_unit_id).unwrap();

    let migration_plan_path = package
        .package_path
        .join("plans")
        .join("generated-migration-successor.md");
    std::fs::create_dir_all(migration_plan_path.parent().unwrap()).unwrap();
    std::fs::write(&migration_plan_path, "# generated migration successor\n").unwrap();
    let migration_project_path = migration_plan_path
        .strip_prefix(temp.path())
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let migration_review = crate::add_review_plan(
        temp.path(),
        crate::NewReviewPlan {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(imported.design_version_id),
            review_type: "design_task_decomposition",
            required: true,
            stage: "implementation-ready",
            scope: None,
            clean_condition: None,
            stop_condition: None,
            review_policy_id: None,
            review_scope_id: None,
        },
    )
    .unwrap();
    let migration_target = format!(
        "review-context:design-task-decomposition:design={}:work={}",
        imported.design_version_id, work.work_unit_id
    );
    let migration_run = crate::review::add_review_run(
        temp.path(),
        crate::NewReviewRun {
            review_plan_id: migration_review.review_plan_id,
            run_type: "fresh",
            run_purpose: "new_unbiased_review",
            target_ref: Some(&migration_target),
            prompt_deviations: None,
            result_summary: Some("generated source requires a reconciled successor"),
            new_findings_count: 1,
            carried_findings_checked: 0,
            clean_run: false,
            status: "completed",
            agent_label: None,
            external_agent_id: None,
            review_provenance: "self_recorded",
            review_provenance_ref: None,
        },
    )
    .unwrap();
    let migration_finding = crate::add_finding(
        temp.path(),
        crate::NewFinding {
            review_run_id: migration_run.review_run_id,
            finding_type: "design_task_gap",
            severity: "high",
            description: "generated source needs one immutable reconciliation result",
            design_requirement_id: None,
            task_id: None,
        },
    )
    .unwrap();
    crate::classify_finding(temp.path(), migration_finding.finding_id, "valid").unwrap();
    let migration_surface = format!(
        "design:edit:plans/generated-migration-successor.md,transition:decomposition-plan-reconcile:{}/{}/{}",
        imported.design_version_id,
        work.work_unit_id,
        crate::review::encode_opaque_component(&migration_project_path)
    );
    let migration_closure = crate::add_closure(
        temp.path(),
        crate::NewClosure {
            finding_id: migration_finding.finding_id,
            design_invariant: "a formal update preserves the exact reconciliation outcome",
            design_citations: None,
            implementation_evidence: None,
            affected_surfaces: Some(&migration_surface),
            same_invariant_search: None,
            other_violations_found: None,
            fix_plan: Some("install the immutable result through the registered transition"),
            tests_or_gates: Some("public update apply and replay"),
            verification_plan: Some("compare the result to its immutable application"),
            closed_by_commit: None,
        },
    )
    .unwrap();

    let edge = generation_16_to_17_edge();
    let conn = generated_predecessor_ledger(temp.path(), &edge);
    let context = TransitionContext { root: temp.path() };
    let project: i64 = conn
        .query_row("select id from projects", [], |row| row.get(0))
        .unwrap();
    let plan: i64 = conn
        .query_row(
            "select id from decomposition_plans where project_id=?1 and work_unit_id=?2 and design_version_id=?3",
            params![project, work.work_unit_id, imported.design_version_id],
            |row| row.get(0),
        )
        .unwrap();
    conn.execute(
        r#"
        insert into decomposition_plans(
          project_id,work_unit_id,design_version_id,plan_key,revision,source_path,
          source_identity,source_kind,design_fingerprint,status,binding_issue,created_at
        )
        select project_id,work_unit_id,design_version_id,'mapping-predecessor',1,null,
               'mapping-predecessor-identity','derived_bundle',design_fingerprint,
               'superseded',null,current_timestamp
        from decomposition_plans where id=?1
        "#,
        [plan],
    )
    .unwrap();
    let predecessor_plan = conn.last_insert_rowid();
    conn.execute(
        "update decomposition_plans set predecessor_id=?1,revision=2 where id=?2",
        params![predecessor_plan, plan],
    )
    .unwrap();
    let tasks = [retained.task_id, retained_peer.task_id, retired.task_id];
    let items = tasks
        .iter()
        .map(|task| {
            conn.query_row(
                "select decomposition_item_id from decomposition_migration_sources where decomposition_plan_id=?1 and source_task_id=?2",
                params![plan, task],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let slices = items
        .iter()
        .map(|item| {
            conn.query_row(
                "select slice_id from decomposition_items where id=?1",
                [item],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let boundaries = items
        .iter()
        .map(|item| {
            conn.query_row(
                "select id from decomposition_item_checklist_boundaries where decomposition_item_id=?1",
                [item],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let item_gates = items
        .iter()
        .map(|item| {
            conn.query_row(
                "select id from decomposition_item_gates where decomposition_item_id=?1",
                [item],
                |row| row.get::<_, i64>(0),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    conn.execute(
        "insert into decomposition_slice_dependencies(project_id,decomposition_plan_id,predecessor_slice_id,successor_slice_id) values(?1,?2,?3,?4)",
        params![project, plan, slices[0], slices[1]],
    )
    .unwrap();
    let retained_declared_dependency = conn.last_insert_rowid();
    conn.execute(
        "insert into decomposition_slice_dependencies(project_id,decomposition_plan_id,predecessor_slice_id,successor_slice_id) values(?1,?2,?3,?4)",
        params![project, plan, slices[0], slices[2]],
    )
    .unwrap();
    conn.execute(
        "insert into decomposition_reconciliation_tasks(project_id,decomposition_plan_id,source_task_id,successor_item_id,disposition,reason) values(?1,?2,?3,?4,'retained',null)",
        params![project, plan, retained.task_id, items[0]],
    )
    .unwrap();
    conn.execute(
        "insert into decomposition_reconciliation_tasks(project_id,decomposition_plan_id,source_task_id,successor_item_id,disposition,reason) values(?1,?2,?3,?4,'retained',null)",
        params![project, plan, retained_peer.task_id, items[1]],
    )
    .unwrap();
    conn.execute(
        "insert into decomposition_reconciliation_tasks(project_id,decomposition_plan_id,source_task_id,successor_item_id,disposition,reason) values(?1,?2,?3,null,'retired','generated retirement reason')",
        params![project, plan, retired.task_id],
    )
    .unwrap();
    for index in 0..2 {
        conn.execute(
            "insert into decomposition_reconciliation_checklist_items(project_id,decomposition_plan_id,source_checklist_item_id,successor_boundary_id,disposition,reason) values(?1,?2,?3,?4,'retained',null)",
            params![project, plan, source_checklist_items[index], boundaries[index]],
        )
        .unwrap();
        conn.execute(
            "insert into decomposition_reconciliation_gates(project_id,decomposition_plan_id,source_validation_gate_id,successor_item_gate_id,disposition,reason) values(?1,?2,?3,?4,'retained',null)",
            params![project, plan, source_gates[index], item_gates[index]],
        )
        .unwrap();
        conn.execute(
            "insert into decomposition_reconciliation_phases(project_id,decomposition_plan_id,source_phase_id,successor_slice_id,disposition,reason) values(?1,?2,?3,?4,'retained',null)",
            params![project, plan, source_phases[index], slices[index]],
        )
        .unwrap();
    }
    conn.execute(
        "insert into decomposition_reconciliation_checklist_items(project_id,decomposition_plan_id,source_checklist_item_id,successor_boundary_id,disposition,reason) values(?1,?2,?3,null,'retired','generated checklist retirement')",
        params![project, plan, source_checklist_items[2]],
    )
    .unwrap();
    conn.execute(
        "insert into decomposition_reconciliation_gates(project_id,decomposition_plan_id,source_validation_gate_id,successor_item_gate_id,disposition,reason) values(?1,?2,?3,null,'retired','generated gate retirement')",
        params![project, plan, source_gates[2]],
    )
    .unwrap();
    conn.execute(
        "insert into decomposition_reconciliation_phases(project_id,decomposition_plan_id,source_phase_id,successor_slice_id,disposition,reason) values(?1,?2,?3,null,'retired','generated phase retirement')",
        params![project, plan, source_phases[2]],
    )
    .unwrap();
    conn.execute(
        "insert into decomposition_reconciliation_dependencies(project_id,decomposition_plan_id,source_dependency_id,successor_dependency_id,disposition,reason) values(?1,?2,?3,?4,'retained',null)",
        params![project, plan, retained_dependency, retained_declared_dependency],
    )
    .unwrap();
    conn.execute(
        "insert into decomposition_reconciliation_dependencies(project_id,decomposition_plan_id,source_dependency_id,successor_dependency_id,disposition,reason) values(?1,?2,?3,null,'retired','generated dependency retirement')",
        params![project, plan, retired_dependency],
    )
    .unwrap();
    conn.execute(
        "update decomposition_reconciliation_tasks set disposition='retained',reason=null where source_task_id=?1",
        [retired.task_id],
    )
    .unwrap();
    (edge.observe_source)(&conn, &context).unwrap();
    conn.execute(
        "update decomposition_reconciliation_tasks set disposition='retired',reason='generated retirement reason' where source_task_id=?1",
        [retired.task_id],
    )
    .unwrap();
    (edge.observe_source)(&conn, &context).unwrap();
    let source = (edge.observe_source)(&conn, &context).unwrap();
    execute_adjacent(&conn, &edge, &source.revision, &context).unwrap();

    let item_keys = items
        .iter()
        .map(|item| {
            conn.query_row(
                "select item_key from decomposition_items where id=?1",
                [item],
                |row| row.get::<_, String>(0),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let boundary_keys = boundaries
        .iter()
        .map(|boundary| {
            conn.query_row(
                "select boundary_key from decomposition_item_checklist_boundaries where id=?1",
                [boundary],
                |row| row.get::<_, String>(0),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let slice_keys = slices
        .iter()
        .map(|slice| {
            conn.query_row(
                "select slice_key from decomposition_slices where id=?1",
                [slice],
                |row| row.get::<_, String>(0),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let mut document = crate::decomposition::PlanDocument {
        record_type: "decomposition_plan".to_string(),
        format: 1,
        key: "generated-migration-successor".to_string(),
        design_fingerprint: imported.content_hash.clone(),
        work: Some(work.work_unit_id),
        items: item_keys
            .iter()
            .zip(boundary_keys.iter())
            .zip(slice_keys.iter())
            .enumerate()
            .map(
                |(index, ((item, boundary), slice))| crate::decomposition::PlanItem {
                    key: item.clone(),
                    requirements: vec!["REQ-MAP".to_string()],
                    title: format!("Generated migration item {}", index + 1),
                    details:
                        "Preserve the generated endpoint through the formal update transition."
                            .to_string(),
                    completion: crate::decomposition::PlanCompletion {
                        outcome: "The generated endpoint remains observable after update."
                            .to_string(),
                        observation: "Query the public endpoint after applying the update."
                            .to_string(),
                        evidence_owner: format!("work:{}", work.work_unit_id),
                        evidence_kind: "validation".to_string(),
                        gates: vec!["GATE-MAP".to_string()],
                    },
                    checklist: vec![crate::decomposition::PlanChecklistBoundary {
                        key: boundary.clone(),
                        condition: "The generated endpoint is observed after update.".to_string(),
                        evidence_kind: "validation".to_string(),
                        gates: vec!["GATE-MAP".to_string()],
                    }],
                    slice: slice.clone(),
                },
            )
            .collect(),
        slices: slice_keys
            .iter()
            .enumerate()
            .map(|(index, slice)| crate::decomposition::PlanSlice {
                key: slice.clone(),
                title: format!("Generated migration slice {}", index + 1),
                order: (index + 1) as i64,
                depends_on: if index == 0 {
                    Vec::new()
                } else {
                    vec![slice_keys[0].clone()]
                },
            })
            .collect(),
        reconciliation: None,
    };
    document.reconciliation = Some(crate::decomposition::PlanReconciliation {
        predecessor: predecessor_plan,
        expected_current: "e".repeat(64),
        tasks: vec![
            crate::decomposition::TaskReconciliation {
                source: retained.task_id,
                disposition: "retained".to_string(),
                item: Some(item_keys[0].clone()),
                reason: None,
                effect: Some(crate::decomposition::ReconciliationEffect::Preserve),
            },
            crate::decomposition::TaskReconciliation {
                source: retained_peer.task_id,
                disposition: "retained".to_string(),
                item: Some(item_keys[1].clone()),
                reason: None,
                effect: Some(crate::decomposition::ReconciliationEffect::Preserve),
            },
            crate::decomposition::TaskReconciliation {
                source: retired.task_id,
                disposition: "retired".to_string(),
                item: None,
                reason: Some("generated retirement reason".to_string()),
                effect: None,
            },
        ],
        checklist: vec![
            crate::decomposition::ChecklistReconciliation {
                source: source_checklist_items[0],
                disposition: "retained".to_string(),
                item: Some(item_keys[0].clone()),
                boundary: Some(boundary_keys[0].clone()),
                reason: None,
                effect: Some(crate::decomposition::ReconciliationEffect::Preserve),
            },
            crate::decomposition::ChecklistReconciliation {
                source: source_checklist_items[1],
                disposition: "retained".to_string(),
                item: Some(item_keys[1].clone()),
                boundary: Some(boundary_keys[1].clone()),
                reason: None,
                effect: Some(crate::decomposition::ReconciliationEffect::Preserve),
            },
            crate::decomposition::ChecklistReconciliation {
                source: source_checklist_items[2],
                disposition: "retired".to_string(),
                item: None,
                boundary: None,
                reason: Some("generated checklist retirement".to_string()),
                effect: None,
            },
        ],
        gates: vec![
            crate::decomposition::GateReconciliation {
                source: source_gates[0],
                disposition: "retained".to_string(),
                item: Some(item_keys[0].clone()),
                gate: Some("GATE-MAP".to_string()),
                boundary: Some("retained-source".to_string()),
                reason: None,
                effect: Some(crate::decomposition::ReconciliationEffect::Preserve),
            },
            crate::decomposition::GateReconciliation {
                source: source_gates[1],
                disposition: "retained".to_string(),
                item: Some(item_keys[1].clone()),
                gate: Some("GATE-MAP".to_string()),
                boundary: Some("retained-source".to_string()),
                reason: None,
                effect: Some(crate::decomposition::ReconciliationEffect::Preserve),
            },
            crate::decomposition::GateReconciliation {
                source: source_gates[2],
                disposition: "retired".to_string(),
                item: None,
                gate: None,
                boundary: None,
                reason: Some("generated gate retirement".to_string()),
                effect: None,
            },
        ],
        phases: vec![
            crate::decomposition::PhaseReconciliation {
                source: source_phases[0],
                disposition: "retained".to_string(),
                slice: Some(slice_keys[0].clone()),
                reason: None,
                effect: Some(crate::decomposition::ReconciliationEffect::Preserve),
            },
            crate::decomposition::PhaseReconciliation {
                source: source_phases[1],
                disposition: "retained".to_string(),
                slice: Some(slice_keys[1].clone()),
                reason: None,
                effect: Some(crate::decomposition::ReconciliationEffect::Preserve),
            },
            crate::decomposition::PhaseReconciliation {
                source: source_phases[2],
                disposition: "retired".to_string(),
                slice: None,
                reason: Some("generated phase retirement".to_string()),
                effect: None,
            },
        ],
        dependencies: vec![
            crate::decomposition::DependencyReconciliation {
                source: retained_dependency,
                disposition: "retained".to_string(),
                from: Some(slice_keys[0].clone()),
                to: Some(slice_keys[1].clone()),
                reason: None,
                effect: Some(crate::decomposition::ReconciliationEffect::Preserve),
            },
            crate::decomposition::DependencyReconciliation {
                source: retired_dependency,
                disposition: "retired".to_string(),
                from: None,
                to: None,
                reason: Some("generated dependency retirement".to_string()),
                effect: None,
            },
        ],
    });
    let migration_content = crate::decomposition::canonical_plan_content(&document).unwrap();
    std::fs::write(&migration_plan_path, &migration_content).unwrap();
    let content_identity = crate::decomposition::plan_content_identity(&migration_content);
    let mut source_hasher = Sha256::new();
    source_hasher.update(b"agent-workbench/decomposition-plan-source/v1\0");
    source_hasher.update(migration_content.as_bytes());
    let source_identity = format!("{:x}", source_hasher.finalize());
    conn.execute(
        "update decomposition_plans set source_path=?1,source_identity=?2,content_identity=?3,document_content=?4 where id=?5",
        params![migration_project_path, source_identity, content_identity, migration_content, plan],
    )
    .unwrap();
    conn.execute(
        "update decomposition_plans set status='applied' where id=?1",
        [plan],
    )
    .unwrap();
    conn.execute(
        "insert into correction_sessions(project_id,finding_id,closure_id,status,created_at,completed_at) values(?1,?2,?3,'active',current_timestamp,null)",
        params![project, migration_finding.finding_id, migration_closure.closure_id],
    )
    .unwrap();
    let correction_session = conn.last_insert_rowid();
    let (correction_token, token_ordinal, token_target): (i64, i64, String) = conn
        .query_row(
            "select id,token_ordinal,target from correction_tokens where closure_id=?1 and operation='decomposition-plan-reconcile'",
            [migration_closure.closure_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        token_target,
        format!(
            "{}/{}/{}",
            imported.design_version_id,
            work.work_unit_id,
            crate::review::encode_opaque_component(&migration_project_path)
        )
    );
    conn.execute(
        "insert into correction_transition_applications(project_id,correction_session_id,correction_token_id,authority_event_id,evidence_ref,before_state,after_state,result_ref,created_at) values(?1,?2,?3,null,null,'generated-before','generated-after',?4,current_timestamp)",
        params![project, correction_session, correction_token, format!("decomposition-plan:{plan}")],
    )
    .unwrap();
    let correction_application = conn.last_insert_rowid();
    conn.execute(
        "update correction_tokens set status='applied',applied_at=current_timestamp where id=?1",
        [correction_token],
    )
    .unwrap();
    let mut payload_hasher = Sha256::new();
    payload_hasher.update(b"agent-workbench/decomposition-reconciliation-payload/v1\0");
    payload_hasher.update(migration_closure.closure_id.to_be_bytes());
    payload_hasher.update(imported.design_version_id.to_be_bytes());
    payload_hasher.update(work.work_unit_id.to_be_bytes());
    payload_hasher.update(
        Path::new(&migration_project_path)
            .as_os_str()
            .as_encoded_bytes(),
    );
    payload_hasher.update(b"\0");
    payload_hasher.update(source_identity.as_bytes());
    payload_hasher.update(b"\0");
    payload_hasher.update("e".repeat(64).as_bytes());
    let payload_identity = format!("{:x}", payload_hasher.finalize());
    conn.execute(
        "insert into decomposition_reconciliation_applications(project_id,correction_application_id,correction_token_id,predecessor_plan_id,successor_plan_id,source_identity,expected_current,payload_identity,created_at) values(?1,?2,?3,?4,?5,?6,?7,?8,current_timestamp)",
        params![project, correction_application, correction_token, predecessor_plan, plan, source_identity, "e".repeat(64), payload_identity],
    )
    .unwrap();
    let reconciliation_application = conn.last_insert_rowid();
    (generation_17_to_18_edge().observe_source)(&conn, &context).unwrap();
    let applied_legacy_target = format!("{}/{}", imported.design_version_id, work.work_unit_id);
    rewrite_immutable_token_target(&conn, correction_token, &applied_legacy_target);
    let plan_source_path: Option<String> = conn
        .query_row(
            "select source_path from decomposition_plans where id=?1",
            [plan],
            |row| row.get(0),
        )
        .unwrap();
    drop(conn);

    let changed_plan_bytes = b"changed optional ingress provenance\n";
    if let Some(source_path) = plan_source_path.as_deref() {
        std::fs::write(temp.path().join(source_path), changed_plan_bytes).unwrap();
    }

    let inspection = public_binary(temp.path(), &["update", "inspect"]);
    assert_eq!(public_value(&inspection, "update_status"), "ready_to_apply");
    let inspection_handle = public_value(&inspection, "inspection_handle").to_string();
    let source_identity = public_value(&inspection, "current_identity").to_string();
    let apply_args = [
        "update",
        "apply",
        inspection_handle.as_str(),
        "--expected-current",
        source_identity.as_str(),
        "--idempotency-key",
        "generated-populated-reconciliation-update",
    ];
    let applied = public_binary(temp.path(), &apply_args);
    let replayed = public_binary(temp.path(), &apply_args);
    assert_eq!(
        public_value(&replayed, "operation_handle"),
        public_value(&applied, "operation_handle")
    );
    assert_eq!(
        public_value(&replayed, "result_identity"),
        public_value(&applied, "result_identity")
    );
    assert_eq!(public_value(&replayed, "already_applied"), "true");
    if let Some(source_path) = plan_source_path.as_deref() {
        assert_eq!(
            std::fs::read(temp.path().join(source_path)).unwrap(),
            changed_plan_bytes
        );
    }
    let owned_plan = crate::show_decomposition_plan(
        temp.path(),
        crate::DecompositionPlanQuery {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
        },
    )
    .unwrap();
    assert_eq!(owned_plan.content_identity.len(), 64);
    assert!(owned_plan.document_content.contains("decomposition_plan"));
    assert!(
        !owned_plan
            .document_content
            .contains("changed optional ingress")
    );
    let current_conn =
        crate::db::open_ledger(&crate::db::default_ledger_path(temp.path())).unwrap();
    assert_eq!(
        current_conn
            .query_row(
                "select target from correction_tokens where id=?1",
                [correction_token],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        applied_legacy_target
    );
    let gate_boundary_balance: (i64, i64) = current_conn
        .query_row(
            r#"
            select
              sum(disposition='retained' and effect='preserve'
                  and boundary_selector='retained-source'
                  and length(resolved_boundary_identity)=64),
              sum(effect='open')
            from decomposition_reconciliation_gates
            "#,
            [],
            |row| {
                Ok((
                    row.get::<_, Option<i64>>(0)?.unwrap_or(0),
                    row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                ))
            },
        )
        .unwrap();
    assert_eq!(gate_boundary_balance, (2, 0));
    let (result_count, result_json): (i64, String) = current_conn
        .query_row(
            r#"
            select count(*),max(result_json)
            from decomposition_reconciliation_results
            where reconciliation_application_id=?1 and project_id=?2
            "#,
            params![reconciliation_application, project],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(result_count, 1);
    let migrated: crate::DecompositionReconciliationOutcome =
        serde_json::from_str(&result_json).unwrap();
    assert_eq!(migrated.plan.plan_id, plan);
    assert_eq!(migrated.predecessor_plan_id, predecessor_plan);
    assert_eq!(migrated.closure_id, migration_closure.closure_id);
    assert_eq!(migrated.token_ordinal, token_ordinal);
    assert_eq!(migrated.correction_application_id, correction_application);
    assert!(!migrated.idempotent);
    assert_eq!(migrated.projection.projection_identity.len(), 64);
    assert_eq!(migrated.projection.commit_current.len(), 64);
    assert!(
        migrated
            .projection
            .endpoint_effects
            .iter()
            .any(|effect| effect.category == "task" && effect.source_id == retained.task_id)
    );
    drop(current_conn);
    std::fs::write(&migration_plan_path, &migration_content).unwrap();
    let legacy_retry = crate::reconcile_decomposition_plan(
        temp.path(),
        crate::DecompositionReconciliationApplication {
            design_version_id: imported.design_version_id,
            work_unit_id: work.work_unit_id,
            plan_path: &migration_plan_path,
            closure_id: migration_closure.closure_id,
            expected_current: &migrated.projection.commit_current,
        },
    )
    .unwrap();
    assert!(legacy_retry.idempotent);
    assert_eq!(
        legacy_retry.correction_application_id,
        correction_application
    );
    assert_eq!(legacy_retry.plan.plan_id, plan);
    std::fs::write(&migration_plan_path, changed_plan_bytes).unwrap();
    assert_eq!(
        crate::list_tasks(
            temp.path(),
            crate::TaskListQuery {
                status: None,
                work_unit_id: Some(work.work_unit_id),
            },
        )
        .unwrap(),
        expected_tasks
    );
    assert_eq!(
        crate::list_checklists(temp.path(), None).unwrap(),
        expected_checklists
    );
    assert_eq!(
        crate::list_validation_gate_context(
            temp.path(),
            crate::ValidationGateContextQuery {
                design_version_id: imported.design_version_id,
                work_unit_id: Some(work.work_unit_id),
            },
        )
        .unwrap(),
        expected_gates
    );
    assert_eq!(
        crate::list_phases(temp.path(), work.work_unit_id).unwrap(),
        expected_phases
    );
    assert_eq!(
        crate::list_phase_dependencies(temp.path(), work.work_unit_id).unwrap(),
        expected_dependencies
    );

    let backup_identity = public_value(&applied, "backup_identity").to_string();
    let current = public_binary(temp.path(), &["update", "inspect"]);
    assert_eq!(public_value(&current, "update_status"), "current");
    let current_identity = public_value(&current, "current_identity").to_string();
    let restored = public_binary(
        temp.path(),
        &[
            "update",
            "restore",
            "--backup",
            backup_identity.as_str(),
            "--expected-current",
            current_identity.as_str(),
            "--idempotency-key",
            "restore-generated-populated-reconciliation-source",
        ],
    );
    assert_eq!(public_value(&restored, "already_applied"), "false");
    if let Some(source_path) = plan_source_path.as_deref() {
        assert_eq!(
            std::fs::read(temp.path().join(source_path)).unwrap(),
            changed_plan_bytes
        );
    }
    let restored_inspection = public_binary(temp.path(), &["update", "inspect"]);
    assert_eq!(
        public_value(&restored_inspection, "update_status"),
        "ready_to_apply"
    );
    let restored_handle = public_value(&restored_inspection, "inspection_handle").to_string();
    let restored_identity = public_value(&restored_inspection, "current_identity").to_string();
    let reapplied = public_binary(
        temp.path(),
        &[
            "update",
            "apply",
            restored_handle.as_str(),
            "--expected-current",
            restored_identity.as_str(),
            "--idempotency-key",
            "reapply-generated-populated-reconciliation-update",
        ],
    );
    assert_eq!(public_value(&reapplied, "already_applied"), "false");
    assert_eq!(
        public_value(
            &public_binary(temp.path(), &["update", "inspect"]),
            "update_status"
        ),
        "current"
    );
    assert_eq!(
        crate::list_tasks(
            temp.path(),
            crate::TaskListQuery {
                status: None,
                work_unit_id: Some(work.work_unit_id),
            },
        )
        .unwrap(),
        expected_tasks
    );
    assert_eq!(
        crate::list_checklists(temp.path(), None).unwrap(),
        expected_checklists
    );
    assert_eq!(
        crate::list_validation_gate_context(
            temp.path(),
            crate::ValidationGateContextQuery {
                design_version_id: imported.design_version_id,
                work_unit_id: Some(work.work_unit_id),
            },
        )
        .unwrap(),
        expected_gates
    );
    assert_eq!(
        crate::list_phases(temp.path(), work.work_unit_id).unwrap(),
        expected_phases
    );
    assert_eq!(
        crate::list_phase_dependencies(temp.path(), work.work_unit_id).unwrap(),
        expected_dependencies
    );
    let reapplied_conn =
        crate::db::open_ledger(&crate::db::default_ledger_path(temp.path())).unwrap();
    let reapplied_result_count: i64 = reapplied_conn
        .query_row(
            "select count(*) from decomposition_reconciliation_results where reconciliation_application_id=?1",
            [reconciliation_application],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(reapplied_result_count, 1);
}

#[test]
fn deployed_descriptor_is_structural_but_target_rejects_an_unproved_completed_attempt() {
    let temp = tempfile::tempdir().unwrap();
    let edge = generation_16_to_17_edge();
    let conn = generated_predecessor_ledger(temp.path(), &edge);
    let project: i64 = conn
        .query_row("select id from projects", [], |row| row.get(0))
        .unwrap();
    conn.execute(
        r#"
        insert into release_candidates(
          project_id,candidate_handle,version,reviewed_commit,manifest_identity,status,
          predecessor_id,idempotency_key,created_at,updated_at
        ) values(?1,'candidate_descriptor_contract','candidate-version','reviewed-commit',
                 'manifest-identity','assembled',null,'candidate-descriptor-contract',
                 current_timestamp,current_timestamp)
        "#,
        [project],
    )
    .unwrap();
    let candidate = conn.last_insert_rowid();
    conn.execute(
        r#"
        insert into release_candidate_revisions(
          project_id,release_candidate_id,revision_handle,revision,state,stage,action,
          request_identity,predecessor_id,head_state,reason,created_at
        ) values(?1,?2,'candidate_revision_current',1,'assembled','local','assemble',
                 'candidate-request',null,'current',null,current_timestamp)
        "#,
        params![project, candidate],
    )
    .unwrap();
    let revision = conn.last_insert_rowid();
    conn.execute(
        r#"
        insert into release_candidate_subject_revisions(
          project_id,release_candidate_revision_id,subject_kind,subject_name,
          expected_identity,local_identity,requested_identity,observed_identity,
          downloaded_identity
        ) values(?1,?2,'local','manifest','manifest-identity','manifest-identity',null,null,null)
        "#,
        params![project, revision],
    )
    .unwrap();
    conn.execute(
        r#"
        insert into release_candidate_attempts(
          project_id,release_candidate_id,action,idempotency_key,expected_current,
          payload_identity,requested_identity,observed_identity,result_revision_handle,
          status,created_at,completed_at
        ) values(?1,?2,'publish','invalid-completed-attempt','candidate_revision_current',
                 'payload','requested',null,null,'completed',current_timestamp,current_timestamp)
        "#,
        params![project, candidate],
    )
    .unwrap();

    let context = TransitionContext { root: temp.path() };
    let source = (edge.observe_source)(&conn, &context).unwrap();
    let error = execute_adjacent(&conn, &edge, &source.revision, &context).unwrap_err();
    assert!(
        error.to_string().contains(
            "transition install-explicit-reconciliation-effects target validation failed"
        )
    );
    assert_eq!(
        conn.query_row("select max(version) from schema_migrations", [], |row| row
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        16
    );

    conn.execute(
        "delete from release_candidate_attempts where release_candidate_id=?1",
        [candidate],
    )
    .unwrap();
    conn.execute(
        "update release_candidate_revisions set head_state='historical' where id=?1",
        [revision],
    )
    .unwrap();
    conn.execute(
        r#"
        insert into release_candidate_revisions(
          project_id,release_candidate_id,revision_handle,revision,state,stage,action,
          request_identity,predecessor_id,head_state,reason,created_at
        ) values(?1,?2,'candidate_revision_withdrawn',2,'withdrawn','terminal','withdraw',
                 'withdraw-request',?3,'current','requested withdrawal',current_timestamp)
        "#,
        params![project, candidate, revision],
    )
    .unwrap();
    let withdrawn_revision = conn.last_insert_rowid();
    conn.execute(
        r#"
        insert into release_candidate_subject_revisions(
          project_id,release_candidate_revision_id,subject_kind,subject_name,
          expected_identity,local_identity,requested_identity,observed_identity,
          downloaded_identity
        ) values(?1,?2,'local','manifest','manifest-identity','manifest-identity',null,null,null)
        "#,
        params![project, withdrawn_revision],
    )
    .unwrap();
    conn.execute(
        "update release_candidates set status='withdrawn' where id=?1",
        [candidate],
    )
    .unwrap();
    conn.execute(
        r#"
        insert into release_candidate_attempts(
          project_id,release_candidate_id,action,idempotency_key,expected_current,
          payload_identity,requested_identity,observed_identity,result_revision_handle,
          status,created_at,completed_at
        ) values(?1,?2,'withdraw','mismatched-withdrawal-effect','candidate_revision_current',
                 'withdraw-payload',?3,?4,'candidate_revision_withdrawn',
                 'completed',current_timestamp,current_timestamp)
        "#,
        params![project, candidate, "a".repeat(64), "b".repeat(64)],
    )
    .unwrap();

    let source = (edge.observe_source)(&conn, &context).unwrap();
    let error = execute_adjacent(&conn, &edge, &source.revision, &context).unwrap_err();
    assert!(
        error.to_string().contains(
            "transition install-explicit-reconciliation-effects target validation failed"
        )
    );
    assert_eq!(
        conn.query_row("select max(version) from schema_migrations", [], |row| row
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        16
    );

    let uppercase_identity = "A".repeat(64);
    conn.execute(
        r#"
        update release_candidate_attempts
        set requested_identity=?1,observed_identity=?1
        where release_candidate_id=?2 and idempotency_key='mismatched-withdrawal-effect'
        "#,
        params![uppercase_identity, candidate],
    )
    .unwrap();
    let source = (edge.observe_source)(&conn, &context).unwrap();
    let error = execute_adjacent(&conn, &edge, &source.revision, &context).unwrap_err();
    assert!(
        error.to_string().contains(
            "transition install-explicit-reconciliation-effects target validation failed"
        )
    );
    assert_eq!(
        conn.query_row("select max(version) from schema_migrations", [], |row| row
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        16
    );
}

#[test]
fn empty_decomposition_domain_reaches_current_without_fabricating_a_plan() {
    let temp = tempfile::tempdir().unwrap();
    let edge = generation_16_to_17_edge();
    let conn = generated_predecessor_ledger(temp.path(), &edge);
    let context = TransitionContext { root: temp.path() };
    let source = (edge.observe_source)(&conn, &context).unwrap();
    execute_adjacent(&conn, &edge, &source.revision, &context).unwrap();
    assert_eq!(
        conn.query_row("select max(version) from schema_migrations", [], |row| row
            .get::<_, i64>(
            0
        ))
        .unwrap(),
        17
    );
    assert_eq!(
        conn.query_row("select count(*) from decomposition_plans", [], |row| row
            .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn structural_plan_input_becomes_a_ready_first_class_graph() {
    let temp = tempfile::tempdir().unwrap();
    crate::init_project(temp.path()).unwrap();
    let work = crate::start_work(temp.path(), "plan owner", None).unwrap();
    let package = crate::init_design_package(
        temp.path(),
        crate::NewDesignPackage {
            design_id: "arbitrary-plan-design",
            title: "Arbitrary Plan Design",
        },
    )
    .unwrap();
    std::fs::write(
        package.package_path.join("requirements").join("README.md"),
        crate::tests::requirement_doc_without_validation("REQ-X", "Arbitrary behavior", "high"),
    )
    .unwrap();
    let imported = crate::import_design_package(
        temp.path(),
        crate::DesignPackageImport {
            package_path: &package.package_path,
            status: "draft",
        },
    )
    .unwrap();
    crate::create_phase(
        temp.path(),
        crate::NewWorkPhase {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(imported.design_version_id),
            key: "arbitrary-slice",
            title: "Arbitrary Slice",
            kind: "implementation",
            order: 1,
            reason: Some("declare the plan owner without deriving a task"),
        },
    )
    .unwrap();
    let plans = package.package_path.join("plans");
    std::fs::create_dir_all(&plans).unwrap();
    std::fs::write(
        plans.join("arbitrary.md"),
        format!(
            r#"# Arbitrary plan

```yaml agent-workbench
type: decomposition_plan
format: 1
key: arbitrary-plan
design_fingerprint: {}
items:
  - key: arbitrary-item
    requirements: [REQ-X]
    title: Arbitrary item
    details: Preserve an opaque user-defined behavior.
    completion:
      outcome: The behavior is observable.
      observation: Exercise the public operation with arbitrary input.
      evidence_owner: work:{}
      evidence_kind: validation
      gates: []
    checklist:
      - key: arbitrary-boundary
        condition: The declared outcome is observed.
        evidence_kind: validation
        gates: []
    slice: arbitrary-slice
slices:
  - key: arbitrary-slice
    title: Arbitrary Slice
    order: 1
    depends_on: []
```
"#,
            imported.content_hash, work.work_unit_id
        ),
    )
    .unwrap();

    let conn = generation_13_ledger(temp.path());
    let context = TransitionContext { root: temp.path() };
    for edge in registered_storage_path(crate::db::CORE_SCHEMA_VERSION).unwrap() {
        let source = (edge.observe_source)(&conn, &context).unwrap();
        execute_adjacent(&conn, &edge, &source.revision, &context).unwrap();
    }

    let graph: (String, i64, i64, i64) = conn
        .query_row(
            "select status,(select count(*) from decomposition_items),(select count(*) from decomposition_slices),(select count(*) from decomposition_item_checklist_boundaries) from decomposition_plans",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(graph, ("ready".to_string(), 1, 1, 1));
}

#[test]
fn legacy_plan_document_is_preserved_without_inventing_structure() {
    let temp = tempfile::tempdir().unwrap();
    crate::init_project(temp.path()).unwrap();
    let package = crate::init_design_package(
        temp.path(),
        crate::NewDesignPackage {
            design_id: "legacy-plan-design",
            title: "Legacy Plan Design",
        },
    )
    .unwrap();
    let imported = crate::import_design_package(
        temp.path(),
        crate::DesignPackageImport {
            package_path: &package.package_path,
            status: "draft",
        },
    )
    .unwrap();
    let plans = package.package_path.join("plans");
    std::fs::create_dir_all(&plans).unwrap();
    std::fs::write(
        plans.join("legacy.md"),
        "# Historical plan\n\nThis document predates structured plan metadata.\n",
    )
    .unwrap();

    let conn = generation_13_ledger(temp.path());
    let context = TransitionContext { root: temp.path() };
    for edge in registered_storage_path(crate::db::CORE_SCHEMA_VERSION).unwrap() {
        let source = (edge.observe_source)(&conn, &context).unwrap();
        execute_adjacent(&conn, &edge, &source.revision, &context).unwrap();
    }

    let preserved: (i64, String, String, i64, i64) = conn
        .query_row(
            r#"
            select design_version_id,status,binding_issue,
                   (select count(*) from decomposition_items),
                   (select count(*) from decomposition_slices)
            from decomposition_plans
            "#,
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(preserved.0, imported.design_version_id);
    assert_eq!(preserved.1, "incomplete");
    assert_eq!(
        preserved.2,
        "formal decomposition plan metadata is required"
    );
    assert_eq!((preserved.3, preserved.4), (0, 0));
}

#[test]
fn derived_state_without_a_plan_is_preserved_as_explicitly_incomplete() {
    let temp = tempfile::tempdir().unwrap();
    crate::init_project(temp.path()).unwrap();
    let work = crate::start_work(temp.path(), "derived owner", None).unwrap();
    let package = crate::init_design_package(
        temp.path(),
        crate::NewDesignPackage {
            design_id: "derived-source-design",
            title: "Derived Source Design",
        },
    )
    .unwrap();
    std::fs::write(
        package.package_path.join("requirements").join("README.md"),
        crate::tests::requirement_doc_without_validation("REQ-Y", "Derived behavior", "high"),
    )
    .unwrap();
    let imported = crate::import_design_package(
        temp.path(),
        crate::DesignPackageImport {
            package_path: &package.package_path,
            status: "draft",
        },
    )
    .unwrap();
    let task = crate::add_task(
        temp.path(),
        crate::NewTask {
            title: "existing task",
            priority: "high",
            source: "design",
            work_unit_id: Some(work.work_unit_id),
            details: Some("existing details"),
            completion_condition: Some("existing condition"),
        },
    )
    .unwrap();
    crate::derive_task_from_requirement(
        temp.path(),
        crate::NewTaskDerivation {
            design_version_id: imported.design_version_id,
            requirement_key: "REQ-Y",
            task_id: task.task_id,
            derivation_reason: Some("existing derivation"),
            checklist_title: Some("existing checklist"),
            item_title: Some("existing boundary"),
            completion_condition: Some("existing condition"),
        },
    )
    .unwrap();
    let phase = crate::create_phase(
        temp.path(),
        crate::NewWorkPhase {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(imported.design_version_id),
            key: "existing-phase",
            title: "Existing Phase",
            kind: "implementation",
            order: 1,
            reason: Some("existing phase membership"),
        },
    )
    .unwrap();
    crate::assign_task_to_phase(temp.path(), phase.phase_id, task.task_id).unwrap();

    let conn = generation_13_ledger(temp.path());
    let context = TransitionContext { root: temp.path() };
    for edge in registered_storage_path(crate::db::CORE_SCHEMA_VERSION).unwrap() {
        let source = (edge.observe_source)(&conn, &context).unwrap();
        execute_adjacent(&conn, &edge, &source.revision, &context).unwrap();
    }

    let result: (String, String, i64, i64, String) = conn
        .query_row(
            "select source_kind,status,(select count(*) from decomposition_items),(select count(*) from decomposition_item_checklist_boundaries),(select mapping_state from decomposition_migration_sources) from decomposition_plans",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
        )
        .unwrap();
    assert_eq!(
        result,
        (
            "derived_bundle".to_string(),
            "incomplete".to_string(),
            1,
            1,
            "exact".to_string()
        )
    );
}

#[test]
fn current_descriptor_rejects_duplicate_package_lineage_plan_heads_without_repair() {
    let temp = tempfile::tempdir().unwrap();
    crate::init_project(temp.path()).unwrap();
    let work = crate::start_work(temp.path(), "head normalization owner", None).unwrap();
    let package = crate::init_design_package(
        temp.path(),
        crate::NewDesignPackage {
            design_id: "head-normalization-design",
            title: "Head Normalization Design",
        },
    )
    .unwrap();
    let requirement_path = package.package_path.join("requirements").join("README.md");
    std::fs::write(
        &requirement_path,
        crate::tests::requirement_doc_without_validation(
            "REQ-HEAD",
            "The current plan is unique per work and package.",
            "high",
        ),
    )
    .unwrap();
    let older_design = crate::import_design_package(
        temp.path(),
        crate::DesignPackageImport {
            package_path: &package.package_path,
            status: "draft",
        },
    )
    .unwrap();
    std::fs::write(
        &requirement_path,
        crate::tests::requirement_doc_without_validation(
            "REQ-HEAD",
            "The current plan and its public phase keys are unique per work and package.",
            "high",
        ),
    )
    .unwrap();
    let newer_design = crate::import_design_package(
        temp.path(),
        crate::DesignPackageImport {
            package_path: &package.package_path,
            status: "draft",
        },
    )
    .unwrap();
    let obsolete_phase = crate::create_phase(
        temp.path(),
        crate::NewWorkPhase {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(older_design.design_version_id),
            key: "public-release",
            title: "Obsolete public release",
            kind: "release",
            order: 1,
            reason: Some("exercise generic plan-head normalization"),
        },
    )
    .unwrap();
    let obsolete_successor = crate::create_phase(
        temp.path(),
        crate::NewWorkPhase {
            work_unit_id: work.work_unit_id,
            design_version_id: Some(older_design.design_version_id),
            key: "publish-history",
            title: "Obsolete publish history",
            kind: "release",
            order: 2,
            reason: Some("exercise superseded dependency invalidation"),
        },
    )
    .unwrap();
    let obsolete_dependency = crate::add_phase_dependency(
        temp.path(),
        crate::NewPhaseDependency {
            from_phase_id: obsolete_phase.phase_id,
            to_phase_id: obsolete_successor.phase_id,
            dependency_type: "blocks",
            reason: "obsolete head ordering",
        },
    )
    .unwrap();

    let ledger_path = crate::db::default_ledger_path(temp.path());
    let conn = crate::db::open_ledger(&ledger_path).unwrap();
    let project = crate::db::project_id(&conn).unwrap();
    conn.execute_batch("drop index decomposition_plan_editable_package_work_unique")
        .unwrap();
    for (design, key, identity) in [
        (
            &older_design,
            "older-head",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
        (
            &newer_design,
            "newer-head",
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        ),
    ] {
        let document = crate::decomposition::PlanDocument {
            record_type: "decomposition_plan".to_string(),
            format: 1,
            key: key.to_string(),
            design_fingerprint: design.content_hash.clone(),
            work: Some(work.work_unit_id),
            items: Vec::new(),
            slices: Vec::new(),
            reconciliation: None,
        };
        let content = crate::decomposition::canonical_plan_content(&document).unwrap();
        let content_identity = crate::decomposition::plan_content_identity(&content);
        conn.execute(
            r#"
            insert into decomposition_plans(
              project_id,work_unit_id,design_version_id,design_package_id,plan_key,revision,source_path,
              source_identity,document_content,content_identity,source_kind,design_fingerprint,status,binding_issue,created_at
            ) values(?1,?2,?3,(select design_package_id from design_versions where id=?3),?4,1,null,?5,?6,?7,'derived_bundle',?8,'incomplete',
                     'requires explicit reconciliation',current_timestamp)
            "#,
            params![
                project,
                work.work_unit_id,
                design.design_version_id,
                key,
                identity,
                content,
                content_identity,
                design.content_hash
            ],
        )
        .unwrap();
    }
    drop(conn);

    let inspection = crate::inspect_update(temp.path()).unwrap();
    assert_eq!(inspection.status, "owner_input_required");
    assert!(inspection.decision_choices.is_empty());
    assert_eq!(
        inspection.next_actions,
        [
            "provide a verified project-owned recovery source, then run agent-workbench update inspect"
                .to_string()
        ]
    );
    let conn = crate::db::open_ledger(&ledger_path).unwrap();
    let heads_before = conn
        .prepare("select status from decomposition_plans order by design_version_id")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    let phase_before: (String, String) = conn
        .query_row(
            "select phase.phase_key,epoch.state from work_phases phase join phase_epochs epoch on epoch.id=phase.id where phase.id=?1",
            [obsolete_phase.phase_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let dependency_before: String = conn
        .query_row(
            "select state from phase_epoch_dependencies where id=?1",
            [obsolete_dependency.dependency_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(heads_before, vec!["incomplete", "incomplete"]);
    assert_eq!(phase_before, ("public-release".into(), "open".into()));
    assert_eq!(dependency_before, "open");
}
