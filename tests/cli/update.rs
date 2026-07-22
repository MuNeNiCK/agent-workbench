use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use sha2::{Digest, Sha256};

use super::*;

#[test]
fn update_inspection_reports_public_state_and_executable_next_action() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);

    let before = file_bytes(temp.path());
    let output = ok(temp.path(), &["update", "inspect"]);
    let after = file_bytes(temp.path());
    assert_eq!(
        after, before,
        "update inspect must not change project files"
    );
    assert!(output.contains("inspection_handle: update_inspection_"));
    assert!(output.contains("update_status: current"));
    assert!(output.contains("next: agent-workbench status"));
    assert!(!output.contains("pending_change:"));
    for private_term in ["schema_", "missing_table", "normalization", "sqlite"] {
        assert!(!output.contains(private_term), "{output}");
    }
}

#[test]
fn update_inspection_keeps_verified_backup_inventory_visible_in_normal_routes() {
    let current = tempfile::tempdir().unwrap();
    ok(current.path(), &["init"]);
    let current_backup = copy_current_backup(current.path());
    let current_ineligible = write_update_ineligible_backup(current.path());
    let current_inspection = ok(current.path(), &["update", "inspect"]);
    assert!(current_inspection.contains("update_status: current"));
    assert!(
        current_inspection.contains(&format!("backup: {current_backup}")),
        "{current_inspection}"
    );
    assert!(current_inspection.contains(&format!("backup: {current_ineligible}")));
    assert!(!current_inspection.contains(&format!("restore:{current_ineligible}")));

    let ready = tempfile::tempdir().unwrap();
    ok(ready.path(), &["init"]);
    let ready_backup = copy_current_backup(ready.path());
    let ready_ineligible = write_update_ineligible_backup(ready.path());
    let ledger = ready.path().join(".agent-workbench/ledger.sqlite");
    let conn = Connection::open(&ledger).unwrap();
    conn.execute_batch("drop view current_tasks").unwrap();
    drop(conn);
    let ready_inspection = ok(ready.path(), &["update", "inspect"]);
    assert!(ready_inspection.contains("update_status: ready_to_apply"));
    assert!(
        ready_inspection.contains(&format!("backup: {ready_backup}")),
        "{ready_inspection}"
    );
    assert!(ready_inspection.contains(&format!("backup: {ready_ineligible}")));
    assert!(!ready_inspection.contains(&format!("restore:{ready_ineligible}")));
}

#[test]
fn recovery_only_state_never_runs_the_normal_update_path() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let backup = copy_current_backup(temp.path());
    replace_current_with_unreadable_state(temp.path());
    let ledger = temp.path().join(".agent-workbench/ledger.sqlite");

    let before = fs::read(&ledger).unwrap();
    let inspection = ok(temp.path(), &["update", "inspect"]);
    assert!(inspection.contains("update_status: recovery_required"));
    assert!(inspection.contains(&format!("update restore --backup {backup}")));
    assert!(!inspection.contains("update apply"));
    let identity = inspection
        .lines()
        .find_map(|line| line.strip_prefix("current_identity: "))
        .unwrap();
    let inspection_handle = inspection
        .lines()
        .find_map(|line| line.strip_prefix("inspection_handle: "))
        .unwrap();

    let apply = aw(
        temp.path(),
        &[
            "update",
            "apply",
            inspection_handle,
            "--expected-current",
            identity,
            "--idempotency-key",
            "recovery-must-not-apply",
        ],
    );
    assert!(!apply.status.success());
    assert!(String::from_utf8_lossy(&apply.stderr).contains("recovery source is required"));
    assert_eq!(fs::read(&ledger).unwrap(), before);
}

#[test]
fn sqlite_page_corruption_has_an_executable_restore_action() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let backup = copy_current_backup(temp.path());
    let ledger = temp.path().join(".agent-workbench/ledger.sqlite");
    let mut bytes = fs::read(&ledger).unwrap();
    assert_eq!(&bytes[..16], b"SQLite format 3\0");
    bytes[100] = 0xff;
    fs::write(&ledger, bytes).unwrap();

    let inspection = ok(temp.path(), &["update", "inspect"]);
    assert!(inspection.contains("update_status: recovery_required"));
    assert!(inspection.contains(&format!("update restore --backup {backup}")));
    ok(
        temp.path(),
        &[
            "update",
            "restore",
            "--backup",
            &backup,
            "--expected-current",
            value(&inspection, "current_identity"),
            "--idempotency-key",
            "restore-corrupt-sqlite-page",
        ],
    );
}

#[test]
fn recovery_offers_only_backups_with_a_complete_registered_update_path() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let usable = copy_current_backup(temp.path());
    let unsupported_handle = write_update_ineligible_backup(temp.path());
    replace_current_with_unreadable_state(temp.path());

    let inspection = ok(temp.path(), &["update", "inspect"]);
    assert!(inspection.contains("update_status: recovery_required"));
    assert!(inspection.contains(&format!("update restore --backup {usable}")));
    assert!(inspection.contains(&format!("backup: {unsupported_handle}")));
    assert!(!inspection.contains(&format!("decision_choice: restore:{unsupported_handle}")));
    assert!(!inspection.contains(&format!("update restore --backup {unsupported_handle}")));
}

#[test]
fn recovery_preflight_does_not_require_external_temporary_storage() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let usable = copy_current_backup(temp.path());
    replace_current_with_unreadable_state(temp.path());
    let unavailable_temp = temp.path().join("not-a-directory");
    fs::write(&unavailable_temp, b"occupied").unwrap();

    let inspection = ok_env(
        temp.path(),
        &["update", "inspect"],
        &[("TMPDIR", unavailable_temp.to_str().unwrap())],
    );
    assert!(inspection.contains("update_status: recovery_required"));
    assert!(
        inspection.contains(&format!("update restore --backup {usable}")),
        "{inspection}"
    );
}

#[test]
fn recovery_never_offers_a_snapshot_with_database_sidecars() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let original = copy_current_backup(temp.path());
    let backups = temp.path().join(".agent-workbench/update-backups");
    let original_path = backups.join(format!("{original}.sqlite"));
    let conn = Connection::open(&original_path).unwrap();
    conn.pragma_update(None, "journal_mode", "wal").unwrap();
    drop(conn);
    let standalone = format!("{:x}", Sha256::digest(fs::read(&original_path).unwrap()));
    let wal_path = backups.join(format!("{standalone}.sqlite"));
    fs::rename(&original_path, &wal_path).unwrap();
    let writer = Connection::open(&wal_path).unwrap();
    writer.pragma_update(None, "wal_autocheckpoint", 0).unwrap();
    writer
        .execute_batch(
            "create table sidecar_probe(value text); insert into sidecar_probe values('pending');",
        )
        .unwrap();
    assert!(PathBuf::from(format!("{}-wal", wal_path.display())).exists());
    replace_current_with_unreadable_state(temp.path());

    let inspection = ok(temp.path(), &["update", "inspect"]);
    assert!(inspection.contains("update_status: owner_input_required"));
    assert!(!inspection.contains(&format!("backup: {standalone}")));
    assert!(!inspection.contains(&format!("update restore --backup {standalone}")));
    let restore = aw(
        temp.path(),
        &[
            "update",
            "restore",
            "--backup",
            &standalone,
            "--expected-current",
            value(&inspection, "current_identity"),
            "--idempotency-key",
            "reject-sidecar-snapshot",
        ],
    );
    assert!(!restore.status.success());
    assert!(
        !String::from_utf8_lossy(&restore.stderr)
            .contains(&format!("update restore --backup {standalone}"))
    );
    drop(writer);
}

#[test]
fn recovery_never_offers_a_snapshot_with_a_hot_rollback_journal() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let original = copy_current_backup(temp.path());
    let backups = temp.path().join(".agent-workbench/update-backups");
    let original_path = backups.join(format!("{original}.sqlite"));
    let writer = Connection::open(&original_path).unwrap();
    writer
        .pragma_update(None, "journal_mode", "delete")
        .unwrap();
    writer.pragma_update(None, "cache_size", 1).unwrap();
    writer.execute_batch("begin immediate").unwrap();
    writer
        .execute_batch(
            r#"
            create table rollback_probe(value text);
            with recursive rows(value) as (
                values(1)
                union all
                select value + 1 from rows where value < 4096
            )
            insert into rollback_probe(value)
            select printf('%01024d', value) from rows;
            "#,
        )
        .unwrap();
    let original_journal = PathBuf::from(format!("{}-journal", original_path.display()));
    assert!(original_journal.exists());
    let main_bytes = fs::read(&original_path).unwrap();
    let journal_bytes = fs::read(&original_journal).unwrap();
    let handle = format!("{:x}", Sha256::digest(&main_bytes));
    assert_ne!(handle, original);
    let snapshot = backups.join(format!("{handle}.sqlite"));
    let snapshot_journal = PathBuf::from(format!("{}-journal", snapshot.display()));
    fs::write(&snapshot, main_bytes).unwrap();
    fs::write(&snapshot_journal, journal_bytes).unwrap();
    drop(writer);
    if original_path != snapshot {
        fs::remove_file(&original_path).unwrap();
    }
    replace_current_with_unreadable_state(temp.path());

    let inspection = ok(temp.path(), &["update", "inspect"]);
    assert!(inspection.contains("update_status: owner_input_required"));
    assert!(!inspection.contains(&format!("backup: {handle}")));
    assert!(!inspection.contains(&format!("update restore --backup {handle}")));
}

#[test]
fn recovery_inspection_keeps_a_wal_mode_standalone_snapshot_immutable() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let original = copy_current_backup(temp.path());
    let backups = temp.path().join(".agent-workbench/update-backups");
    let original_path = backups.join(format!("{original}.sqlite"));
    let conn = Connection::open(&original_path).unwrap();
    conn.pragma_update(None, "journal_mode", "wal").unwrap();
    drop(conn);
    let standalone = format!("{:x}", Sha256::digest(fs::read(&original_path).unwrap()));
    let wal_mode_path = backups.join(format!("{standalone}.sqlite"));
    fs::rename(&original_path, &wal_mode_path).unwrap();
    let wal_sidecar = PathBuf::from(format!("{}-wal", wal_mode_path.display()));
    let shm_sidecar = PathBuf::from(format!("{}-shm", wal_mode_path.display()));
    assert!(!wal_sidecar.exists());
    assert!(!shm_sidecar.exists());
    replace_current_with_unreadable_state(temp.path());

    let inspection = ok(temp.path(), &["update", "inspect"]);
    assert!(inspection.contains(&format!("update restore --backup {standalone}")));
    assert!(!wal_sidecar.exists());
    assert!(!shm_sidecar.exists());
    ok(
        temp.path(),
        &[
            "update",
            "restore",
            "--backup",
            &standalone,
            "--expected-current",
            value(&inspection, "current_identity"),
            "--idempotency-key",
            "restore-wal-mode-standalone",
        ],
    );
}

#[test]
fn managed_recovery_selection_keeps_external_restorable_inventory_visible() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let ledger = temp.path().join(".agent-workbench/ledger.sqlite");
    let conn = Connection::open(&ledger).unwrap();
    conn.execute_batch("drop view current_tasks").unwrap();
    drop(conn);
    let inspection = ok(temp.path(), &["update", "inspect"]);
    let applied = ok(
        temp.path(),
        &[
            "update",
            "apply",
            value(&inspection, "inspection_handle"),
            "--expected-current",
            value(&inspection, "current_identity"),
            "--idempotency-key",
            "managed-inventory-selection",
        ],
    );
    let managed = value(&applied, "backup_identity");
    let external = write_update_ineligible_backup(temp.path());
    replace_current_with_unreadable_state(temp.path());

    let recovery = ok(temp.path(), &["update", "inspect"]);
    assert!(
        recovery.contains(&format!("backup: {managed}")),
        "{recovery}"
    );
    assert!(
        recovery.contains(&format!("backup: {external}")),
        "{recovery}"
    );
    assert!(
        recovery.contains(&format!("update restore --backup {managed}")),
        "{recovery}"
    );
    assert!(
        !recovery.contains(&format!("restore:{external}")),
        "{recovery}"
    );
}

#[test]
fn completed_restore_lineage_recovers_its_published_target_not_its_pre_restore_image() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let target = copy_current_backup(temp.path());
    ok(
        temp.path(),
        &[
            "kpt",
            "start",
            "--scope",
            "project",
            "--summary",
            "pre-restore state",
        ],
    );
    let before_restore = ok(temp.path(), &["update", "inspect"]);
    let restored = ok(
        temp.path(),
        &[
            "update",
            "restore",
            "--backup",
            &target,
            "--expected-current",
            value(&before_restore, "current_identity"),
            "--idempotency-key",
            "restore-lineage-direction",
        ],
    );
    let pre_restore = value(&restored, "recovery_backup_identity");
    assert_ne!(target, pre_restore);
    replace_current_with_unreadable_state(temp.path());

    let recovery = ok(temp.path(), &["update", "inspect"]);
    assert!(
        recovery.contains(&format!("update restore --backup {target}")),
        "{recovery}"
    );
    assert!(recovery.contains(&format!("backup: {pre_restore}")));
    assert!(!recovery.contains(&format!("update restore --backup {pre_restore}")));
}

#[cfg(unix)]
#[test]
fn restore_rejects_a_symlink_substituted_backup() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let backup = copy_current_backup(temp.path());
    let backup_path = temp
        .path()
        .join(format!(".agent-workbench/update-backups/{backup}.sqlite"));
    let external = temp.path().join("external-backup.sqlite");
    fs::rename(&backup_path, &external).unwrap();
    symlink(&external, &backup_path).unwrap();
    ok(
        temp.path(),
        &[
            "kpt",
            "start",
            "--scope",
            "project",
            "--summary",
            "changed current state",
        ],
    );
    let inspection = ok(temp.path(), &["update", "inspect"]);
    let restore = aw(
        temp.path(),
        &[
            "update",
            "restore",
            "--backup",
            &backup,
            "--expected-current",
            value(&inspection, "current_identity"),
            "--idempotency-key",
            "reject-symlink-backup",
        ],
    );
    assert!(!restore.status.success());
    assert!(
        String::from_utf8_lossy(&restore.stderr).contains("update restore could not be completed")
    );
}

#[cfg(unix)]
#[test]
fn recovery_inspection_never_offers_a_symlinked_backup_directory() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let backup = copy_current_backup(temp.path());
    let backups = temp.path().join(".agent-workbench/update-backups");
    let external = temp.path().join("external-backup-directory");
    fs::rename(&backups, &external).unwrap();
    symlink(&external, &backups).unwrap();
    replace_current_with_unreadable_state(temp.path());

    let inspection = ok(temp.path(), &["update", "inspect"]);
    assert!(inspection.contains("update_status: owner_input_required"));
    assert!(!inspection.contains(&format!("backup: {backup}")));
    assert!(!inspection.contains("update restore --backup"));
    assert!(inspection.contains("provide a verified project-owned recovery source"));

    let restore = aw(
        temp.path(),
        &[
            "update",
            "restore",
            "--backup",
            &backup,
            "--expected-current",
            value(&inspection, "current_identity"),
            "--idempotency-key",
            "reject-symlink-backup-directory",
        ],
    );
    assert!(!restore.status.success());
    let error = String::from_utf8_lossy(&restore.stderr);
    assert!(!error.contains("update restore --backup"), "{error}");
    assert!(error.contains("provide a verified project-owned recovery source"));
}

#[test]
fn unusable_managed_checkpoint_falls_back_to_ambiguous_external_sources() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let ledger = temp.path().join(".agent-workbench/ledger.sqlite");
    let conn = Connection::open(&ledger).unwrap();
    conn.execute_batch("drop view current_tasks").unwrap();
    drop(conn);
    let inspection = ok(temp.path(), &["update", "inspect"]);
    let applied = ok(
        temp.path(),
        &[
            "update",
            "apply",
            value(&inspection, "inspection_handle"),
            "--expected-current",
            value(&inspection, "current_identity"),
            "--idempotency-key",
            "create-managed-recovery-lineage",
        ],
    );
    let managed = value(&applied, "backup_identity");
    fs::write(
        temp.path()
            .join(format!(".agent-workbench/update-backups/{managed}.sqlite")),
        b"corrupted managed checkpoint",
    )
    .unwrap();

    let first_external = copy_current_backup(temp.path());
    ok(
        temp.path(),
        &[
            "kpt",
            "start",
            "--scope",
            "project",
            "--summary",
            "second external recovery state",
        ],
    );
    let second_external = copy_current_backup(temp.path());
    replace_current_with_unreadable_state(temp.path());

    let recovery = ok(temp.path(), &["update", "inspect"]);
    assert!(recovery.contains("update_status: owner_input_required"));
    assert_eq!(recovery.matches("decision_choice: restore:").count(), 2);
    assert!(recovery.contains(&format!("decision_choice: restore:{first_external}")));
    assert!(recovery.contains(&format!("decision_choice: restore:{second_external}")));
    assert!(!recovery.contains(&format!("decision_choice: restore:{managed}")));
}

#[test]
fn partial_state_projects_a_typed_no_mutation_owner_action() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let ledger = temp.path().join(".agent-workbench/ledger.sqlite");
    let conn = Connection::open(&ledger).unwrap();
    conn.execute_batch("drop table design_versions").unwrap();
    drop(conn);

    let before = file_bytes(temp.path());
    let status = ok(temp.path(), &["status"]);
    assert!(status.contains("project_integrity: blocked"));
    assert!(status.contains("next: agent-workbench update inspect"));

    let inspection = ok(temp.path(), &["update", "inspect"]);
    assert!(
        inspection.contains("update_status: owner_input_required"),
        "{inspection}"
    );
    assert!(inspection.contains(
        "next: provide a verified project-owned recovery source, then run agent-workbench update inspect"
    ));
    assert!(!inspection.contains("decision_choice:"));
    for private_term in ["sqlite", "missing table", "no such table"] {
        assert!(
            !inspection.to_lowercase().contains(private_term),
            "{inspection}"
        );
    }
    assert_eq!(file_bytes(temp.path()), before);

    let apply = aw(
        temp.path(),
        &[
            "update",
            "apply",
            value(&inspection, "inspection_handle"),
            "--expected-current",
            value(&inspection, "current_identity"),
            "--idempotency-key",
            "partial-state-must-not-mutate",
        ],
    );
    assert!(!apply.status.success());
    assert!(
        String::from_utf8_lossy(&apply.stderr)
            .contains("the inspected project has no applicable update")
    );
    assert_eq!(file_bytes(temp.path()), before);
}

#[test]
fn unusable_backup_reports_only_public_state_and_action() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let bytes = b"not an Agent Workbench project backup";
    let backup = format!("{:x}", Sha256::digest(bytes));
    let backups = temp.path().join(".agent-workbench/update-backups");
    fs::create_dir_all(&backups).unwrap();
    fs::write(backups.join(format!("{backup}.sqlite")), bytes).unwrap();
    let inspection = ok(temp.path(), &["update", "inspect"]);
    let restore = aw(
        temp.path(),
        &[
            "update",
            "restore",
            "--backup",
            &backup,
            "--expected-current",
            value(&inspection, "current_identity"),
        ],
    );
    assert!(!restore.status.success());
    let error = String::from_utf8_lossy(&restore.stderr);
    assert!(error.contains("update restore could not be completed"));
    assert!(error.contains("update_status: current"));
    assert!(error.contains("next: agent-workbench status"));
    for private_term in [
        "conservation",
        "descriptor",
        "family",
        "schema",
        "sqlite",
        "foreign key",
        "foreign-key",
        "generation",
        "normalization",
        "storage",
        "transition",
        "ledger",
        "table",
    ] {
        assert!(!error.to_lowercase().contains(private_term), "{error}");
    }
}

#[test]
fn failed_apply_reports_only_public_state_and_exact_action() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let ledger = temp.path().join(".agent-workbench/ledger.sqlite");
    let conn = Connection::open(&ledger).unwrap();
    conn.execute_batch("drop view current_tasks").unwrap();
    drop(conn);

    let inspection = ok(temp.path(), &["update", "inspect"]);
    assert!(
        inspection.contains("update_status: ready_to_apply"),
        "{inspection}"
    );
    fs::write(
        temp.path().join(".agent-workbench/update-operations"),
        b"simulate an unavailable operation receipt location",
    )
    .unwrap();
    let exact_action = inspection
        .lines()
        .find(|line| line.starts_with("next: "))
        .unwrap();
    let apply = aw(
        temp.path(),
        &[
            "update",
            "apply",
            value(&inspection, "inspection_handle"),
            "--expected-current",
            value(&inspection, "current_identity"),
            "--idempotency-key",
            "public-failed-update",
        ],
    );
    assert!(!apply.status.success());
    let error = String::from_utf8_lossy(&apply.stderr);
    assert!(error.contains("update apply could not be completed"));
    assert!(error.contains("update_status: ready_to_apply"));
    assert!(error.contains(exact_action));
    for private_term in [
        "descriptor",
        "family",
        "schema",
        "sqlite",
        "foreign key",
        "foreign-key",
        "generation",
        "normalization",
        "storage",
        "transition",
        "conservation",
        "ledger",
        "table",
    ] {
        assert!(!error.to_lowercase().contains(private_term), "{error}");
    }
}

#[test]
fn update_apply_and_restore_are_exactly_replayable_public_operations() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let authority = ok(
        temp.path(),
        &[
            "authority",
            "event",
            "add",
            "--type",
            "user_instruction",
            "--summary",
            "settle retained KPT outcome",
            "--scope",
            "project",
        ],
    );
    ok(
        temp.path(),
        &[
            "kpt",
            "start",
            "--scope",
            "project",
            "--summary",
            "retained public state",
        ],
    );
    ok(
        temp.path(),
        &[
            "kpt",
            "item",
            "add",
            "--type",
            "keep",
            "--title",
            "retain converted rule",
            "--details",
            "preserve this rule through update and restore",
        ],
    );
    ok(
        temp.path(),
        &["kpt", "item", "convert", "--item", "1", "--to", "rule"],
    );
    ok(
        temp.path(),
        &[
            "kpt",
            "item",
            "add",
            "--type",
            "try",
            "--title",
            "retain dismissal receipt",
        ],
    );
    let items = ok(temp.path(), &["kpt", "item", "list", "--review", "1"]);
    let dismissed_current = kpt_item_current(&items, 2);
    ok(
        temp.path(),
        &[
            "kpt",
            "item",
            "dismiss",
            "2",
            "--authority",
            value(&authority, "authority_event_id"),
            "--reason",
            "no follow-up required",
            "--expected-current",
            dismissed_current,
        ],
    );
    ok(temp.path(), &["kpt", "close", "1"]);
    let expected_kpt = kpt_public_projection(temp.path());
    let ledger = temp.path().join(".agent-workbench/ledger.sqlite");
    let conn = Connection::open(&ledger).unwrap();
    conn.execute_batch("drop view current_tasks").unwrap();
    drop(conn);

    let inspection = ok(temp.path(), &["update", "inspect"]);
    assert!(inspection.contains("update_status: ready_to_apply"));
    let inspection_handle = value(&inspection, "inspection_handle");
    let expected = value(&inspection, "current_identity");
    let apply_args = [
        "update",
        "apply",
        inspection_handle,
        "--expected-current",
        expected,
        "--idempotency-key",
        "public-update-replay",
    ];
    let applied = ok(temp.path(), &apply_args);
    let replayed = ok(temp.path(), &apply_args);
    assert_eq!(
        value(&applied, "operation_handle"),
        value(&replayed, "operation_handle")
    );
    assert_eq!(
        value(&applied, "result_identity"),
        value(&replayed, "result_identity")
    );
    assert!(replayed.contains("already_applied: true"));
    assert_eq!(kpt_public_projection(temp.path()), expected_kpt);

    let changed_payload = aw(
        temp.path(),
        &[
            "update",
            "apply",
            inspection_handle,
            "--expected-current",
            &"f".repeat(64),
            "--idempotency-key",
            "public-update-replay",
        ],
    );
    assert!(!changed_payload.status.success());
    assert!(
        String::from_utf8_lossy(&changed_payload.stderr)
            .contains("idempotency key was already used with a different update request")
    );

    let backup = value(&applied, "backup_identity");
    let current = ok(temp.path(), &["update", "inspect"]);
    let restore_args = [
        "update",
        "restore",
        "--backup",
        backup,
        "--expected-current",
        value(&current, "current_identity"),
        "--idempotency-key",
        "public-restore-replay",
    ];
    let restored = ok(temp.path(), &restore_args);
    let restored_again = ok(temp.path(), &restore_args);
    assert_eq!(
        value(&restored, "operation_handle"),
        value(&restored_again, "operation_handle")
    );
    assert_eq!(
        value(&restored, "restored_identity"),
        value(&restored_again, "restored_identity")
    );
    assert!(restored_again.contains("already_applied: true"));
    let restored_inspection = ok(temp.path(), &["update", "inspect"]);
    assert!(restored_inspection.contains("update_status: ready_to_apply"));
    assert!(restored_inspection.contains("update_required: true"));
    ok(
        temp.path(),
        &[
            "update",
            "apply",
            value(&restored_inspection, "inspection_handle"),
            "--expected-current",
            value(&restored_inspection, "current_identity"),
            "--idempotency-key",
            "public-update-after-restore",
        ],
    );
    assert_eq!(kpt_public_projection(temp.path()), expected_kpt);
}

#[test]
fn established_apply_and_restore_forms_remain_replayable() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let ledger = temp.path().join(".agent-workbench/ledger.sqlite");
    let conn = Connection::open(&ledger).unwrap();
    conn.execute_batch("drop view current_tasks").unwrap();
    drop(conn);

    let inspection = ok(temp.path(), &["update", "inspect"]);
    let expected = value(&inspection, "current_identity");
    let apply_args = ["update", "apply", "--expected-current", expected];
    let applied = ok(temp.path(), &apply_args);
    let replayed = ok(temp.path(), &apply_args);
    assert_eq!(
        value(&applied, "operation_handle"),
        value(&replayed, "operation_handle")
    );
    assert!(replayed.contains("already_applied: true"));

    let backup = value(&applied, "backup_identity");
    let current = ok(temp.path(), &["update", "inspect"]);
    let restore_args = [
        "update",
        "restore",
        "--backup",
        backup,
        "--expected-current",
        value(&current, "current_identity"),
    ];
    let restored = ok(temp.path(), &restore_args);
    let replayed = ok(temp.path(), &restore_args);
    assert_eq!(
        value(&restored, "operation_handle"),
        value(&replayed, "operation_handle")
    );
    assert!(replayed.contains("already_applied: true"));
}

#[test]
fn stale_or_incomplete_apply_prints_the_current_executable_command() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let backup = copy_current_backup(temp.path());
    let ledger = temp.path().join(".agent-workbench/ledger.sqlite");
    let conn = Connection::open(&ledger).unwrap();
    conn.execute_batch("drop view current_tasks").unwrap();
    drop(conn);

    let inspection = ok(temp.path(), &["update", "inspect"]);
    let exact_action = inspection
        .lines()
        .find_map(|line| line.strip_prefix("next: "))
        .unwrap();
    let stale = aw(
        temp.path(),
        &["update", "apply", "--expected-current", &"f".repeat(64)],
    );
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains(&format!("next: {exact_action}")));

    let incomplete = aw(
        temp.path(),
        &[
            "update",
            "apply",
            value(&inspection, "inspection_handle"),
            "--expected-current",
            value(&inspection, "current_identity"),
        ],
    );
    assert!(!incomplete.status.success());
    assert!(String::from_utf8_lossy(&incomplete.stderr).contains(&format!("next: {exact_action}")));

    for restore_args in [
        vec![
            "update",
            "restore",
            "--backup",
            &backup,
            "--expected-current",
            &"e".repeat(64),
        ],
        vec![
            "update",
            "restore",
            "--backup",
            &backup,
            "--expected-current",
            &"e".repeat(64),
            "--idempotency-key",
            "stale-current-restore",
        ],
    ] {
        let stale_restore = aw(temp.path(), &restore_args);
        assert!(!stale_restore.status.success());
        assert!(
            String::from_utf8_lossy(&stale_restore.stderr)
                .contains(&format!("next: {exact_action}"))
        );
    }
}

#[test]
fn established_apply_keeps_distinct_valid_identities_distinct() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let ledger = temp.path().join(".agent-workbench/ledger.sqlite");
    let conn = Connection::open(&ledger).unwrap();
    conn.execute_batch("drop view current_tasks").unwrap();
    drop(conn);

    let inspection = ok(temp.path(), &["update", "inspect"]);
    let expected = value(&inspection, "current_identity");
    ok(
        temp.path(),
        &["update", "apply", "--expected-current", expected],
    );
    let current = ok(temp.path(), &["update", "inspect"]);
    let exact_action = current
        .lines()
        .find_map(|line| line.strip_prefix("next: "))
        .unwrap();
    let replacement = if expected[12..].bytes().all(|byte| byte == b'f') {
        "e".repeat(52)
    } else {
        "f".repeat(52)
    };
    let colliding_prefix_identity = format!("{}{replacement}", &expected[..12]);
    let stale = aw(
        temp.path(),
        &[
            "update",
            "apply",
            "--expected-current",
            &colliding_prefix_identity,
        ],
    );
    assert!(!stale.status.success());
    let error = String::from_utf8_lossy(&stale.stderr);
    assert!(error.contains(&format!("next: {exact_action}")));
    assert!(!error.contains("idempotency key was already used"));
}

#[test]
fn ambiguous_recovery_never_chooses_a_source_for_either_cli_form() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    ok(
        temp.path(),
        &[
            "authority",
            "event",
            "add",
            "--type",
            "user_instruction",
            "--summary",
            "first recovery state",
        ],
    );
    let first_backup = copy_current_backup(temp.path());
    ok(
        temp.path(),
        &[
            "authority",
            "event",
            "add",
            "--type",
            "user_instruction",
            "--summary",
            "second recovery state",
        ],
    );
    let second_backup = copy_current_backup(temp.path());
    replace_current_with_unreadable_state(temp.path());
    let before = file_bytes(temp.path());
    let inspection = ok(temp.path(), &["update", "inspect"]);
    let expected_actions = inspection
        .lines()
        .filter(|line| line.starts_with("next: "))
        .collect::<Vec<_>>();
    assert_eq!(expected_actions.len(), 2);

    let established_apply = aw(
        temp.path(),
        &[
            "update",
            "apply",
            "--expected-current",
            value(&inspection, "current_identity"),
        ],
    );
    assert!(!established_apply.status.success());
    let apply_error = String::from_utf8_lossy(&established_apply.stderr);
    for action in &expected_actions {
        assert!(apply_error.contains(action));
    }
    let current_apply = aw(
        temp.path(),
        &[
            "update",
            "apply",
            value(&inspection, "inspection_handle"),
            "--expected-current",
            value(&inspection, "current_identity"),
            "--idempotency-key",
            "ambiguous-current-apply",
        ],
    );
    assert!(!current_apply.status.success());
    let current_apply_error = String::from_utf8_lossy(&current_apply.stderr);
    for action in &expected_actions {
        assert!(current_apply_error.contains(action));
    }

    for restore_args in [
        vec![
            "update",
            "restore",
            "--backup",
            &first_backup,
            "--expected-current",
            value(&inspection, "current_identity"),
        ],
        vec![
            "update",
            "restore",
            "--backup",
            &second_backup,
            "--expected-current",
            value(&inspection, "current_identity"),
            "--idempotency-key",
            "ambiguous-current-form",
        ],
    ] {
        let restore = aw(temp.path(), &restore_args);
        assert!(!restore.status.success());
        let restore_error = String::from_utf8_lossy(&restore.stderr);
        for action in &expected_actions {
            assert!(restore_error.contains(action));
        }
    }
    assert_eq!(file_bytes(temp.path()), before);
}

#[test]
fn update_decide_rejects_a_choice_that_inspect_did_not_offer() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let inspection = ok(temp.path(), &["update", "inspect"]);
    let decision = aw(
        temp.path(),
        &[
            "update",
            "decide",
            value(&inspection, "inspection_handle"),
            "--choice",
            "invented",
            "--authority",
            "1",
            "--reason",
            "must be offered",
            "--expected-current",
            value(&inspection, "current_identity"),
        ],
    );
    assert!(!decision.status.success());
    assert!(
        String::from_utf8_lossy(&decision.stderr)
            .contains("inspection does not offer that decision")
    );
}

#[test]
fn update_decide_selects_one_recovery_source_and_persists_after_restore() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let first_authority = ok(
        temp.path(),
        &[
            "authority",
            "event",
            "add",
            "--type",
            "user_instruction",
            "--summary",
            "select recovery source",
        ],
    );
    let authority = value(&first_authority, "authority_event_id");
    let first_backup = copy_current_backup(temp.path());
    ok(
        temp.path(),
        &[
            "authority",
            "event",
            "add",
            "--type",
            "user_instruction",
            "--summary",
            "second recovery state",
        ],
    );
    let second_backup = copy_current_backup(temp.path());
    assert_ne!(first_backup, second_backup);

    let ledger = temp.path().join(".agent-workbench/ledger.sqlite");
    replace_current_with_unreadable_state(temp.path());

    let inspection = ok(temp.path(), &["update", "inspect"]);
    assert!(inspection.contains("update_status: owner_input_required"));
    assert_eq!(inspection.matches("decision_choice: restore:").count(), 2);
    let choice = format!("restore:{first_backup}");
    let decide_args = [
        "update",
        "decide",
        value(&inspection, "inspection_handle"),
        "--choice",
        &choice,
        "--authority",
        authority,
        "--reason",
        "select recovery source",
        "--expected-current",
        value(&inspection, "current_identity"),
    ];
    let decided = ok(temp.path(), &decide_args);
    let repeated = ok(temp.path(), &decide_args);
    assert_eq!(
        value(&decided, "decision_handle"),
        value(&repeated, "decision_handle")
    );
    assert!(repeated.contains("already_applied: true"));

    let selected = ok(temp.path(), &["update", "inspect"]);
    assert!(selected.contains("update_status: recovery_required"));
    assert!(!selected.contains("decision_choice:"));
    assert!(selected.contains(&format!("update restore --backup {first_backup}")));
    assert!(!selected.contains(&format!("update restore --backup {second_backup}")));
    let bypass = aw(
        temp.path(),
        &[
            "update",
            "restore",
            "--backup",
            &second_backup,
            "--expected-current",
            value(&selected, "current_identity"),
            "--idempotency-key",
            "bypass-selected-source",
        ],
    );
    assert!(!bypass.status.success());
    assert!(
        String::from_utf8_lossy(&bypass.stderr)
            .contains("backup does not match the recorded recovery decision")
    );
    ok(
        temp.path(),
        &[
            "update",
            "restore",
            "--backup",
            &first_backup,
            "--expected-current",
            value(&selected, "current_identity"),
            "--idempotency-key",
            "restore-selected-source",
        ],
    );
    let conn = Connection::open(&ledger).unwrap();
    let persisted: (i64, i64) = conn
        .query_row(
            "select (select count(*) from update_operations where status='decision_recorded'),(select count(*) from update_decisions where status='recorded')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(persisted, (1, 1));
}

#[test]
fn recovery_authority_selects_an_exact_source_without_fabricating_project_authority() {
    let temp = tempfile::tempdir().unwrap();
    ok(temp.path(), &["init"]);
    let initial_authority_events: i64 =
        Connection::open(temp.path().join(".agent-workbench/ledger.sqlite"))
            .unwrap()
            .query_row("select count(*) from authority_events", [], |row| {
                row.get(0)
            })
            .unwrap();
    let first_backup = copy_current_backup(temp.path());
    ok(
        temp.path(),
        &[
            "kpt",
            "start",
            "--scope",
            "project",
            "--summary",
            "second recovery candidate",
        ],
    );
    let second_backup = copy_current_backup(temp.path());
    assert_ne!(first_backup, second_backup);
    replace_current_with_unreadable_state(temp.path());

    let inspection = ok(temp.path(), &["update", "inspect"]);
    assert!(inspection.contains("update_status: owner_input_required"));
    assert_eq!(inspection.matches("decision_choice: restore:").count(), 2);
    assert!(
        inspection.contains("next: agent-workbench update authority-record --help"),
        "{inspection}"
    );
    assert!(!inspection.contains("--authority "));
    let inspection_handle = value(&inspection, "inspection_handle");
    let expected_current = value(&inspection, "current_identity");
    let choice = format!("restore:{first_backup}");
    let authority_args = [
        "update",
        "authority-record",
        inspection_handle,
        "--choice",
        &choice,
        "--statement",
        "select-first-source",
        "--provenance",
        "user_instruction",
        "--provenance-ref",
        "user-request:recovery-source",
        "--expected-current",
        expected_current,
        "--idempotency-key",
        "record-recovery-choice",
    ];
    let authority = ok(temp.path(), &authority_args);
    let replayed = ok(temp.path(), &authority_args);
    assert_eq!(
        value(&authority, "authority_handle"),
        value(&replayed, "authority_handle")
    );
    assert!(replayed.contains("already_recorded: true"));
    let authority_handle = value(&authority, "authority_handle");
    assert!(authority.contains(&format!(
        "next: agent-workbench update decide {inspection_handle} --choice {choice} --recovery-authority {authority_handle} --reason select-first-source --expected-current {expected_current}"
    )));

    let changed_replay = aw(
        temp.path(),
        &[
            "update",
            "authority-record",
            inspection_handle,
            "--choice",
            &choice,
            "--statement",
            "different-statement",
            "--provenance",
            "user_instruction",
            "--provenance-ref",
            "user-request:recovery-source",
            "--expected-current",
            expected_current,
            "--idempotency-key",
            "record-recovery-choice",
        ],
    );
    assert!(!changed_replay.status.success());
    assert!(
        String::from_utf8_lossy(&changed_replay.stderr)
            .contains("idempotency key is already bound to a different update request")
    );

    let wrong_reason = aw(
        temp.path(),
        &[
            "update",
            "decide",
            inspection_handle,
            "--choice",
            &choice,
            "--recovery-authority",
            authority_handle,
            "--reason",
            "different-statement",
            "--expected-current",
            expected_current,
        ],
    );
    assert!(!wrong_reason.status.success());
    assert!(
        String::from_utf8_lossy(&wrong_reason.stderr)
            .contains("recovery authority does not match the current update choice and reason")
    );

    let decision_args = [
        "update",
        "decide",
        inspection_handle,
        "--choice",
        &choice,
        "--recovery-authority",
        authority_handle,
        "--reason",
        "select-first-source",
        "--expected-current",
        expected_current,
    ];
    let decided = ok(temp.path(), &decision_args);
    let decision_replay = ok(temp.path(), &decision_args);
    assert_eq!(
        value(&decided, "decision_handle"),
        value(&decision_replay, "decision_handle")
    );
    assert!(decision_replay.contains("already_applied: true"));

    let selected = ok(temp.path(), &["update", "inspect"]);
    assert!(selected.contains("update_status: recovery_required"));
    assert!(selected.contains(&format!("update restore --backup {first_backup}")));
    assert!(!selected.contains(&format!("update restore --backup {second_backup}")));
    ok(
        temp.path(),
        &[
            "update",
            "restore",
            "--backup",
            &first_backup,
            "--expected-current",
            value(&selected, "current_identity"),
            "--idempotency-key",
            "restore-recovery-choice",
        ],
    );
    let conn = Connection::open(temp.path().join(".agent-workbench/ledger.sqlite")).unwrap();
    let authority_events: i64 = conn
        .query_row("select count(*) from authority_events", [], |row| {
            row.get(0)
        })
        .unwrap();
    let ordinary_update_decisions: i64 = conn
        .query_row("select count(*) from update_decisions", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(authority_events, initial_authority_events);
    assert_eq!(ordinary_update_decisions, 0);
}

fn value<'a>(output: &'a str, key: &str) -> &'a str {
    output
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}: ")))
        .unwrap_or_else(|| panic!("missing {key} in:\n{output}"))
}

fn kpt_item_current(output: &str, item_id: i64) -> &str {
    let marker = format!("{item_id} [review=");
    let mut lines = output.lines();
    while let Some(line) = lines.next() {
        if line.starts_with(&marker) {
            return lines
                .next()
                .and_then(|line| line.strip_prefix("current: "))
                .expect("KPT item current handle must follow its row");
        }
    }
    panic!("KPT item {item_id} missing from output:\n{output}")
}

fn kpt_public_projection(root: &Path) -> Vec<String> {
    [
        ok(root, &["kpt", "list"]),
        ok(root, &["kpt", "item", "list", "--review", "1"]),
        ok(root, &["rules", "applicable", "--scope", "project"]),
    ]
    .into_iter()
    .collect()
}

fn file_bytes(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn collect(root: &Path, at: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(at).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                collect(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root).unwrap().to_path_buf(),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    collect(root, root, &mut files);
    files
}

fn copy_current_backup(root: &Path) -> String {
    let ledger = root.join(".agent-workbench/ledger.sqlite");
    let conn = Connection::open(&ledger).unwrap();
    conn.execute_batch("pragma wal_checkpoint(truncate);")
        .unwrap();
    drop(conn);
    let bytes = fs::read(&ledger).unwrap();
    let identity = format!("{:x}", Sha256::digest(&bytes));
    let backups = root.join(".agent-workbench/update-backups");
    fs::create_dir_all(&backups).unwrap();
    fs::write(backups.join(format!("{identity}.sqlite")), bytes).unwrap();
    identity
}

fn write_update_ineligible_backup(root: &Path) -> String {
    let backups = root.join(".agent-workbench/update-backups");
    fs::create_dir_all(&backups).unwrap();
    let unsupported_path = backups.join("unsupported.sqlite");
    let unsupported = Connection::open(&unsupported_path).unwrap();
    unsupported
        .execute_batch(
            "create table schema_migrations(version integer primary key,applied_at text not null);\
             insert into schema_migrations values(25,current_timestamp);\
             create table projects(id integer primary key,root_path text not null);",
        )
        .unwrap();
    unsupported
        .execute(
            "insert into projects(id,root_path) values(1,?1)",
            [root.canonicalize().unwrap().to_string_lossy().as_ref()],
        )
        .unwrap();
    drop(unsupported);
    let bytes = fs::read(&unsupported_path).unwrap();
    let handle = format!("{:x}", Sha256::digest(&bytes));
    fs::rename(&unsupported_path, backups.join(format!("{handle}.sqlite"))).unwrap();
    handle
}

fn replace_current_with_unreadable_state(root: &Path) {
    let ledger = root.join(".agent-workbench/ledger.sqlite");
    fs::write(ledger, b"unreadable current ledger state").unwrap();
}
