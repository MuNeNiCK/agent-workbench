use crate::update::{
    UpdateBoundary, apply_update_operation_inner, backup_dir, checkpoint, install_copy,
    ledger_path, restore_update_operation_inner, set_shared_lock_blocked_observer, sha256_file,
    sync_dir,
};
use crate::{
    UpdateInspection, apply_update_operation, inspect_update, restore_update,
    restore_update_operation,
};
use anyhow::bail;
use rusqlite::{Connection, params};
use std::fs::{self, File};
use std::path::Path;
use std::sync::mpsc;

fn require_registered_repair(root: &Path) -> UpdateInspection {
    let conn = Connection::open(ledger_path(root)).unwrap();
    conn.execute_batch("drop view current_tasks").unwrap();
    drop(conn);
    let inspection = inspect_update(root).unwrap();
    assert_eq!(inspection.status, "ready_to_apply");
    inspection
}

#[test]
fn restore_is_verified_reversible_and_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    crate::init_project(temp.path()).unwrap();
    let root = temp.path().canonicalize().unwrap();
    let ledger = ledger_path(&root);
    let original_identity = sha256_file(&ledger).unwrap();
    let backups = backup_dir(&root);
    fs::create_dir_all(&backups).unwrap();
    install_copy(
        &ledger,
        &backups.join(format!("{original_identity}.sqlite")),
        &original_identity,
    )
    .unwrap();

    crate::start_work(&root, "change current state", None).unwrap();
    checkpoint(&ledger).unwrap();
    let changed_identity = sha256_file(&ledger).unwrap();
    assert_ne!(changed_identity, original_identity);
    let expected_current = inspect_update(&root).unwrap().current_identity;

    let restored = restore_update(&root, &original_identity, &expected_current).unwrap();
    assert!(!restored.already_applied);
    assert_eq!(restored.recovery_backup_identity, changed_identity);
    assert_ne!(restored.restored_identity, changed_identity);
    assert_eq!(
        sha256_file(&backups.join(format!("{changed_identity}.sqlite"))).unwrap(),
        changed_identity
    );

    let repeated = restore_update(&root, &original_identity, &expected_current).unwrap();
    assert!(repeated.already_applied);
    assert_eq!(repeated.operation_handle, restored.operation_handle);
    assert_eq!(repeated.restored_identity, restored.restored_identity);
    assert!(crate::project_status(&root).unwrap().initialized);
}

#[test]
fn ordinary_writer_waits_for_publication_and_its_commit_is_not_lost() {
    let temp = tempfile::tempdir().unwrap();
    crate::init_project(temp.path()).unwrap();
    let root = temp.path().canonicalize().unwrap();
    let inspection = require_registered_repair(&root);
    let (at_publication_tx, at_publication_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let update_root = root.clone();
    let inspection_handle = inspection.inspection_handle.clone();
    let expected_current = inspection.current_identity.clone();
    let update = std::thread::spawn(move || {
        let mut paused = false;
        apply_update_operation_inner(
            &update_root,
            &inspection_handle,
            &expected_current,
            "writer-race-update",
            &mut |boundary| {
                if boundary == UpdateBoundary::BeforePublication && !paused {
                    paused = true;
                    at_publication_tx.send(()).unwrap();
                    resume_rx.recv().unwrap();
                }
                Ok(())
            },
        )
    });
    at_publication_rx.recv().unwrap();

    let (writer_blocked_tx, writer_blocked_rx) = mpsc::channel();
    let (writer_done_tx, writer_done_rx) = mpsc::channel();
    let writer_root = root.clone();
    let writer = std::thread::spawn(move || {
        set_shared_lock_blocked_observer(move || writer_blocked_tx.send(()).unwrap());
        let outcome = crate::start_work(&writer_root, "writer after update", None);
        writer_done_tx.send(outcome).unwrap();
    });
    writer_blocked_rx.recv().unwrap();
    assert!(
        matches!(writer_done_rx.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "ordinary writer completed while update publication was paused"
    );

    resume_tx.send(()).unwrap();
    update.join().unwrap().unwrap();
    let work = writer_done_rx.recv().unwrap().unwrap();
    writer.join().unwrap();
    let records = crate::list_tasks(
        &root,
        crate::TaskListQuery {
            work_unit_id: Some(work.work_unit_id),
            status: None,
        },
    )
    .unwrap();
    assert!(records.is_empty());
    assert_eq!(crate::project_status(&root).unwrap().open_work_units, 1);
}

#[test]
fn restore_retry_replays_the_published_result_without_overwriting_a_later_writer() {
    let temp = tempfile::tempdir().unwrap();
    crate::init_project(temp.path()).unwrap();
    let root = temp.path().canonicalize().unwrap();
    let ledger = ledger_path(&root);
    checkpoint(&ledger).unwrap();
    let original_identity = sha256_file(&ledger).unwrap();
    let backups = backup_dir(&root);
    fs::create_dir_all(&backups).unwrap();
    install_copy(
        &ledger,
        &backups.join(format!("{original_identity}.sqlite")),
        &original_identity,
    )
    .unwrap();
    crate::start_work(&root, "state before interrupted restore", None).unwrap();
    checkpoint(&ledger).unwrap();
    let current = inspect_update(&root).unwrap();
    let key = "restore-writer-after-publication";
    let failure = restore_update_operation_inner(
        &root,
        &original_identity,
        &current.current_identity,
        key,
        &mut |boundary| {
            if boundary == UpdateBoundary::AfterPublication {
                bail!("injected interruption after restore publication")
            }
            Ok(())
        },
    )
    .unwrap_err();
    assert!(
        failure
            .to_string()
            .contains("injected interruption after restore publication")
    );

    let published_result: String = Connection::open(&ledger)
        .unwrap()
        .query_row(
            "select result_identity from restore_publications where idempotency_key=?1",
            params![key],
            |row| row.get(0),
        )
        .unwrap();
    let writer = crate::start_work(&root, "writer after interrupted restore", None).unwrap();
    assert_eq!(crate::project_status(&root).unwrap().open_work_units, 1);

    let recovered =
        restore_update_operation(&root, &original_identity, &current.current_identity, key)
            .unwrap();
    assert_eq!(recovered.restored_identity, published_result);
    assert_eq!(crate::project_status(&root).unwrap().open_work_units, 1);
    let writer_status: String = Connection::open(&ledger)
        .unwrap()
        .query_row(
            "select status from work_units where id=?1",
            params![writer.work_unit_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(writer_status, "open");

    let replayed =
        restore_update_operation(&root, &original_identity, &current.current_identity, key)
            .unwrap();
    assert!(replayed.already_applied);
    assert_eq!(replayed.restored_identity, published_result);
    assert_eq!(replayed.operation_handle, recovered.operation_handle);
}

#[test]
fn apply_retry_replays_the_published_result_without_overwriting_a_later_writer() {
    let temp = tempfile::tempdir().unwrap();
    crate::init_project(temp.path()).unwrap();
    let root = temp.path().canonicalize().unwrap();
    let inspection = require_registered_repair(&root);
    let key = "apply-writer-after-publication";
    let failure = apply_update_operation_inner(
        &root,
        &inspection.inspection_handle,
        &inspection.current_identity,
        key,
        &mut |boundary| {
            if boundary == UpdateBoundary::AfterPublication {
                bail!("injected interruption after update publication")
            }
            Ok(())
        },
    )
    .unwrap_err();
    assert!(
        failure
            .to_string()
            .contains("injected interruption after update publication")
    );

    let writer = crate::start_work(&root, "writer after interrupted update", None).unwrap();
    let recovered = apply_update_operation(
        &root,
        &inspection.inspection_handle,
        &inspection.current_identity,
        key,
    )
    .unwrap();
    assert_eq!(crate::project_status(&root).unwrap().open_work_units, 1);
    let writer_status: String = Connection::open(ledger_path(&root))
        .unwrap()
        .query_row(
            "select status from work_units where id=?1",
            params![writer.work_unit_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(writer_status, "open");

    let replayed = apply_update_operation(
        &root,
        &inspection.inspection_handle,
        &inspection.current_identity,
        key,
    )
    .unwrap();
    assert!(replayed.already_applied);
    assert_eq!(replayed.result_identity, recovered.result_identity);
    assert_eq!(replayed.operation_handle, recovered.operation_handle);
}

#[test]
fn retry_recovers_apply_interruptions_at_durability_and_publication_boundaries() {
    for boundary in [
        UpdateBoundary::BackupDurable,
        UpdateBoundary::ReceiptPrepared,
        UpdateBoundary::BeforePublication,
        UpdateBoundary::AfterPublication,
        UpdateBoundary::CompletionDurable,
    ] {
        let temp = tempfile::tempdir().unwrap();
        crate::init_project(temp.path()).unwrap();
        let root = temp.path().canonicalize().unwrap();
        let inspection = require_registered_repair(&root);
        let key = match boundary {
            UpdateBoundary::BackupDurable => "apply-after-backup-durable",
            UpdateBoundary::ReceiptPrepared => "apply-after-receipt-prepared",
            UpdateBoundary::BeforePublication => "interrupt-before-publication",
            UpdateBoundary::AfterPublication => "interrupt-after-publication",
            UpdateBoundary::CompletionDurable => "apply-after-completion-durable",
            _ => unreachable!(),
        };
        let failure = apply_update_operation_inner(
            &root,
            &inspection.inspection_handle,
            &inspection.current_identity,
            key,
            &mut |observed| {
                if observed == boundary {
                    bail!("injected interruption")
                }
                Ok(())
            },
        )
        .unwrap_err();
        assert!(failure.to_string().contains("injected interruption"));

        let recovered = apply_update_operation(
            &root,
            &inspection.inspection_handle,
            &inspection.current_identity,
            key,
        )
        .unwrap();
        let replayed = apply_update_operation(
            &root,
            &inspection.inspection_handle,
            &inspection.current_identity,
            key,
        )
        .unwrap();
        assert_eq!(replayed.operation_handle, recovered.operation_handle);
        assert_eq!(replayed.result_identity, recovered.result_identity);
        assert!(replayed.already_applied);
        let conn = Connection::open(ledger_path(&root)).unwrap();
        let recorded: (i64, i64) = conn
                .query_row(
                    "select (select count(*) from update_operations where operation_handle=?1 and status='published'),(select count(*) from update_receipts receipt join update_operations operation on operation.id=receipt.update_operation_id where operation.operation_handle=?1 and receipt.status='published')",
                    params![recovered.operation_handle],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
        assert_eq!(recorded, (1, 1));
    }
}

#[test]
fn retry_recovers_restore_interruptions_at_durability_and_publication_boundaries() {
    for boundary in [
        UpdateBoundary::BackupDurable,
        UpdateBoundary::ReceiptPrepared,
        UpdateBoundary::BeforePublication,
        UpdateBoundary::AfterPublication,
        UpdateBoundary::CompletionDurable,
    ] {
        let temp = tempfile::tempdir().unwrap();
        crate::init_project(temp.path()).unwrap();
        let root = temp.path().canonicalize().unwrap();
        let inspection = require_registered_repair(&root);
        let applied = apply_update_operation(
            &root,
            &inspection.inspection_handle,
            &inspection.current_identity,
            "prepare-restore-interruption",
        )
        .unwrap();
        let current = inspect_update(&root).unwrap();
        let key = match boundary {
            UpdateBoundary::BackupDurable => "restore-after-backup-durable",
            UpdateBoundary::ReceiptPrepared => "restore-after-receipt-prepared",
            UpdateBoundary::BeforePublication => "restore-before-publication",
            UpdateBoundary::AfterPublication => "restore-after-publication",
            UpdateBoundary::CompletionDurable => "restore-after-completion-durable",
            _ => unreachable!(),
        };
        let failure = restore_update_operation_inner(
            &root,
            &applied.backup_identity,
            &current.current_identity,
            key,
            &mut |observed| {
                if observed == boundary {
                    bail!("injected interruption")
                }
                Ok(())
            },
        )
        .unwrap_err();
        assert!(failure.to_string().contains("injected interruption"));

        let restored = restore_update_operation(
            &root,
            &applied.backup_identity,
            &current.current_identity,
            key,
        )
        .unwrap();
        let replayed = restore_update_operation(
            &root,
            &applied.backup_identity,
            &current.current_identity,
            key,
        )
        .unwrap();
        assert_eq!(replayed.operation_handle, restored.operation_handle);
        assert_eq!(replayed.restored_identity, restored.restored_identity);
        assert!(replayed.already_applied);
        let conn = Connection::open(ledger_path(&root)).unwrap();
        let recorded: (i64, i64) = conn
                .query_row(
                    "select (select count(*) from update_operations where operation_handle=?1 and status='restored'),(select count(*) from update_receipts receipt join update_operations operation on operation.id=receipt.update_operation_id where operation.operation_handle=?1 and receipt.status='restored')",
                    params![restored.operation_handle],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
        assert_eq!(recorded, (1, 1));
    }
}

#[test]
fn unreadable_current_is_preserved_at_the_durable_backup_boundary() {
    let temp = tempfile::tempdir().unwrap();
    crate::init_project(temp.path()).unwrap();
    let root = temp.path().canonicalize().unwrap();
    let ledger = ledger_path(&root);
    checkpoint(&ledger).unwrap();
    let original_identity = sha256_file(&ledger).unwrap();
    let backups = backup_dir(&root);
    fs::create_dir_all(&backups).unwrap();
    install_copy(
        &ledger,
        &backups.join(format!("{original_identity}.sqlite")),
        &original_identity,
    )
    .unwrap();

    fs::write(&ledger, b"unreadable current ledger state").unwrap();
    File::open(&ledger).unwrap().sync_all().unwrap();
    sync_dir(ledger.parent().unwrap()).unwrap();
    let unreadable_identity = sha256_file(&ledger).unwrap();
    let inspection = inspect_update(&root).unwrap();
    assert_eq!(inspection.status, "recovery_required");

    let failure = restore_update_operation_inner(
        &root,
        &original_identity,
        &inspection.current_identity,
        "unreadable-current-recovery",
        &mut |boundary| {
            if boundary == UpdateBoundary::BackupDurable {
                bail!("injected interruption")
            }
            Ok(())
        },
    )
    .unwrap_err();
    assert!(failure.to_string().contains("injected interruption"));
    assert_eq!(
        sha256_file(&backups.join(format!("{unreadable_identity}.sqlite"))).unwrap(),
        unreadable_identity
    );

    let restored = restore_update_operation(
        &root,
        &original_identity,
        &inspection.current_identity,
        "unreadable-current-recovery",
    )
    .unwrap();
    assert_eq!(restored.recovery_backup_identity, unreadable_identity);
    assert!(crate::project_status(&root).unwrap().initialized);
}
