use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::path::{Component, Path};

use anyhow::{Context, Result, bail};
use rustix::fs::{AtFlags, Mode, OFlags, Stat, fstat, fsync, open, openat, unlinkat};
use rustix::process::geteuid;
use xattr::FileExt;

#[derive(Clone, Copy)]
pub enum OwnerPolicy {
    Root,
    Invoker,
}

pub fn read_absolute(
    path: &Path,
    max: u64,
    exact: Option<u64>,
    owner: OwnerPolicy,
) -> Result<Vec<u8>> {
    let (parent, name) = walk_parent(path, owner)?;
    let fd = openat(
        &parent,
        name,
        OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .with_context(|| format!("cannot safely open {}", path.display()))?;
    let before = fstat(&fd)?;
    validate_final_stat(&before, false, owner)?;
    reject_posix_acl(&fd)?;
    if before.st_size <= 0
        || before.st_size as u64 > max
        || exact.is_some_and(|size| before.st_size as u64 != size)
    {
        bail!("input file size is outside the supported bound");
    }
    let expected = before.st_size as usize;
    let mut file = File::from(fd);
    let mut bytes = Vec::with_capacity(expected);
    Read::by_ref(&mut file)
        .take(expected as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() != expected {
        bail!("input file changed or had extra/short data");
    }
    let after = fstat(&file)?;
    if !same_identity(&before, &after) {
        bail!("input file changed during acquisition");
    }
    Ok(bytes)
}

pub fn write_new_absolute(path: &Path, bytes: &[u8]) -> Result<()> {
    let (parent, name) = walk_parent(path, OwnerPolicy::Invoker)?;
    let fd = openat(
        &parent,
        name,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::from_raw_mode(0o600),
    )
    .context("output_exists")?;
    let mut file = File::from(fd);
    let result = (|| -> Result<()> {
        file.write_all(bytes)?;
        file.sync_all()?;
        Ok(())
    })();
    if let Err(error) = result {
        drop(file);
        let _ = unlinkat(&parent, name, AtFlags::empty());
        let _ = fsync(&parent);
        return Err(error);
    }
    fsync(&parent)?;
    Ok(())
}

fn walk_parent(path: &Path, owner: OwnerPolicy) -> Result<(rustix::fd::OwnedFd, &std::ffi::OsStr)> {
    if !path.is_absolute() {
        bail!("path must be absolute");
    }
    let mut parts = path
        .components()
        .filter_map(|part| match part {
            Component::RootDir => None,
            Component::Normal(value) => Some(Ok(value)),
            _ => Some(Err(anyhow::anyhow!(
                "path contains dot or unsafe components"
            ))),
        })
        .collect::<Result<Vec<_>>>()?;
    let name = parts.pop().context("path has no final component")?;
    let mut directory = open(
        "/",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    validate_ancestor_stat(&fstat(&directory)?, owner)?;
    reject_posix_acl(&directory)?;
    for part in parts {
        directory = openat(
            &directory,
            part,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::empty(),
        )
        .with_context(|| format!("cannot safely walk {}", path.display()))?;
        validate_ancestor_stat(&fstat(&directory)?, owner)?;
        reject_posix_acl(&directory)?;
    }
    validate_final_stat(&fstat(&directory)?, true, owner)?;
    Ok((directory, name))
}

fn validate_ancestor_stat(stat: &Stat, owner: OwnerPolicy) -> Result<()> {
    validate_kind_and_mode(stat, true)?;
    let invoking_uid = geteuid().as_raw();
    let valid_owner = match owner {
        OwnerPolicy::Root => stat.st_uid == 0,
        OwnerPolicy::Invoker => stat.st_uid == 0 || stat.st_uid == invoking_uid,
    };
    if !valid_owner {
        bail!("path component ownership is unsafe");
    }
    Ok(())
}

fn validate_final_stat(stat: &Stat, directory: bool, owner: OwnerPolicy) -> Result<()> {
    validate_kind_and_mode(stat, directory)?;
    let uid = match owner {
        OwnerPolicy::Root => 0,
        OwnerPolicy::Invoker => geteuid().as_raw(),
    };
    if stat.st_uid != uid {
        bail!("path ownership is unsafe");
    }
    Ok(())
}

fn validate_kind_and_mode(stat: &Stat, directory: bool) -> Result<()> {
    let kind = stat.st_mode & 0o170000;
    let expected = if directory { 0o040000 } else { 0o100000 };
    if kind != expected {
        bail!("path component has the wrong file type");
    }
    if stat.st_mode & 0o022 != 0 {
        bail!("path writability is unsafe");
    }
    Ok(())
}

fn reject_posix_acl(fd: impl AsFd) -> Result<()> {
    let invoking_uid = geteuid().as_raw();
    if invoking_uid == 0 {
        return Ok(());
    }
    let copy = rustix::io::dup(fd)?;
    let file = File::from(copy);
    if let Some(acl) = file.get_xattr("system.posix_acl_access")? {
        if acl.len() < 4 || (acl.len() - 4) % 8 != 0 {
            bail!("path has a malformed POSIX ACL");
        }
        let mut mask = 0x7_u16;
        let mut named = Vec::new();
        for entry in acl[4..].chunks_exact(8) {
            let tag = u16::from_ne_bytes([entry[0], entry[1]]);
            let perm = u16::from_ne_bytes([entry[2], entry[3]]);
            let id = u32::from_ne_bytes([entry[4], entry[5], entry[6], entry[7]]);
            if tag == 0x10 {
                mask = perm;
            } else if tag == 0x2 && id == invoking_uid {
                named.push(perm);
            }
        }
        if named.into_iter().any(|perm| perm & mask & 0x2 != 0) {
            bail!("path ACL grants the invoking uid write access");
        }
    }
    Ok(())
}

fn same_identity(a: &Stat, b: &Stat) -> bool {
    a.st_dev == b.st_dev
        && a.st_ino == b.st_ino
        && a.st_mode == b.st_mode
        && a.st_uid == b.st_uid
        && a.st_gid == b.st_gid
        && a.st_size == b.st_size
        && a.st_mtime == b.st_mtime
        && a.st_mtime_nsec == b.st_mtime_nsec
        && a.st_ctime == b.st_ctime
        && a.st_ctime_nsec == b.st_ctime_nsec
}
