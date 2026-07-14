use std::fs::{self, File, OpenOptions};
use std::path::Path;

use anyhow::Result;
use fs2::FileExt;

pub(super) struct MigrationLock {
    _file: File,
}

pub(super) fn shared(root: &Path) -> Result<MigrationLock> {
    acquire(root, false)
}

pub(super) fn exclusive(root: &Path) -> Result<MigrationLock> {
    acquire(root, true)
}

fn acquire(root: &Path, exclusive: bool) -> Result<MigrationLock> {
    let directory = root.join(".agent-workbench").join("recovery");
    fs::create_dir_all(&directory)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("task-history.lock"))?;
    if exclusive {
        FileExt::lock_exclusive(&file)?;
    } else {
        FileExt::lock_shared(&file)?;
    }
    Ok(MigrationLock { _file: file })
}
