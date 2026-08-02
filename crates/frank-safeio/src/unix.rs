//! Unix flag/log IO anchored to a verified directory fd.
//!
//! Ported from `archive/src/hooks/caveman-config.js` (`safeWriteFlag` /
//! `readFlag` / `appendFlag`), with one structural improvement: the original
//! resolves the parent directory by path (`lstat` → optionally `realpath`),
//! then does a second, unrelated path-based `open` for the temp file. That
//! leaves a TOCTOU window between "we verified this directory" and "we wrote
//! into it" — an attacker who can swap directory entries in that window can
//! still redirect the write. Here, once the parent is verified, every
//! following operation (`openat`/`renameat`) is anchored to that directory's
//! *file descriptor*, not its path, so there is nothing left to swap.

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rustix::fd::OwnedFd;
use rustix::fs::{self, AtFlags, CWD, FileType, Mode, OFlags};

use crate::error::{Result, SafeIoError};

fn verified_dir_flags() -> OFlags {
    OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC
}

fn append_existing_flags() -> OFlags {
    OFlags::WRONLY | OFlags::APPEND | OFlags::NOFOLLOW | OFlags::CLOEXEC
}

fn append_create_flags() -> OFlags {
    OFlags::WRONLY
        | OFlags::CREATE
        | OFlags::EXCL
        | OFlags::APPEND
        | OFlags::NOFOLLOW
        | OFlags::CLOEXEC
}

fn create_write_flags() -> OFlags {
    OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC
}

fn read_flags() -> OFlags {
    OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC
}

/// Verify `dir` and open it as an fd every following operation anchors to.
///
/// `dir` may itself be a symlink — the legitimate "`~/.claude` symlinked to a
/// dotfiles repo or shared config volume" pattern — in which case we resolve
/// it and require the real target be a directory owned by the current uid.
/// A symlink pointing at a directory owned by someone else is refused.
fn open_verified_dir_inner(dir: &Path, create: bool) -> Result<OwnedFd> {
    if create {
        std::fs::create_dir_all(dir)?;
    }

    let lst = std::fs::symlink_metadata(dir)?;
    let real_dir: PathBuf = if lst.file_type().is_symlink() {
        std::fs::canonicalize(dir)?
    } else {
        dir.to_path_buf()
    };

    // OFlags::NOFOLLOW here is defense in depth against a race between the
    // canonicalize above and this open: if something swapped the resolved
    // path back into a symlink in between, this call fails closed (ELOOP)
    // instead of silently following it.
    let dirfd = fs::openat(CWD, &real_dir, verified_dir_flags(), Mode::empty())
        .map_err(|_| SafeIoError::SymlinkTargetNotDir)?;

    let st = fs::fstat(&dirfd)?;
    if FileType::from_raw_mode(st.st_mode) != FileType::Directory {
        return Err(SafeIoError::SymlinkTargetNotDir);
    }
    let current_uid = rustix::process::getuid().as_raw();
    if st.st_uid != current_uid {
        return Err(SafeIoError::SymlinkTargetWrongOwner);
    }

    Ok(dirfd)
}

fn open_verified_dir(dir: &Path) -> Result<OwnedFd> {
    open_verified_dir_inner(dir, true)
}

/// Read-only callers must not create directories as a side effect of a
/// missing log/config path. They still use the exact same ownership and
/// no-follow checks as writers, so a symlink escape cannot turn a harmless
/// read into an arbitrary-file read.
fn open_existing_verified_dir(dir: &Path) -> Result<OwnedFd> {
    open_verified_dir_inner(dir, false)
}

pub fn ensure_dir(path: &Path) -> Result<()> {
    let _ = open_verified_dir(path)?;
    Ok(())
}

/// Open `name` inside `dirfd` for append, creating it if missing.
///
/// This is deliberately *not* a single `openat(..., O_CREAT | O_NOFOLLOW,
/// ...)` call. Under concurrent creation of the same not-yet-existing path,
/// that combination was observed to spuriously fail with `ENOENT` on macOS
/// (reproducible via 16 threads racing to create the same log file — see
/// `concurrent_appends_yield_well_formed_lines`); `O_CREAT` without
/// `O_EXCL` is not required to be atomic the way exclusive creation is, and
/// XNU's symlink-check lookup for `O_NOFOLLOW` appears to race with a
/// concurrent creator rather than transparently retrying.
///
/// The fix is the standard portable idiom: try opening the file assuming it
/// exists; if that's `ENOENT`, race to create it exclusively; if we lose
/// that race (`EEXIST`, someone else just created it), fall back to
/// opening it as already-existing. Every branch keeps `O_NOFOLLOW`.
fn open_append_create(dirfd: &OwnedFd, name: &OsStr) -> Result<rustix::fd::OwnedFd> {
    let open_existing = || fs::openat(dirfd, name, append_existing_flags(), Mode::empty());

    match open_existing() {
        Ok(fd) => return Ok(fd),
        Err(e) if e == rustix::io::Errno::NOENT => {}
        Err(e) => return Err(e.into()),
    }

    match fs::openat(
        dirfd,
        name,
        append_create_flags(),
        Mode::from_raw_mode(0o600),
    ) {
        Ok(fd) => Ok(fd),
        Err(e) if e == rustix::io::Errno::EXIST => Ok(open_existing()?),
        Err(e) => Err(e.into()),
    }
}

/// Refuse if `name` inside `dirfd` currently exists and is a symlink. Mirrors
/// the original's explicit refusal to write through a flag-path symlink —
/// note that `renameat` replacing a symlink destination would actually be
/// safe on its own (rename never dereferences its target), but we still
/// refuse outright rather than silently deleting a symlink a user may have
/// placed there deliberately.
fn refuse_if_symlink(dirfd: &OwnedFd, name: &OsStr) -> Result<()> {
    match fs::statat(dirfd, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(st) if FileType::from_raw_mode(st.st_mode) == FileType::Symlink => {
            Err(SafeIoError::IsSymlink)
        }
        Ok(_) => Ok(()), // exists-and-not-a-symlink: proceed
        Err(e) if e == rustix::io::Errno::NOENT => Ok(()),
        Err(e) => Err(e.into()),
    }
}

pub fn write_flag_atomic(flag_path: &Path, content: &str) -> Result<()> {
    let dir = flag_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = flag_path.file_name().ok_or_else(|| {
        SafeIoError::Io(io::Error::new(io::ErrorKind::InvalidInput, "no filename"))
    })?;

    let dirfd = open_verified_dir(dir)?;
    refuse_if_symlink(&dirfd, name)?;

    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let tmp_name = format!(".{}.{pid}.{nanos}.tmp", name.to_string_lossy());

    let tmp_fd = fs::openat(
        &dirfd,
        tmp_name.as_str(),
        create_write_flags(),
        Mode::from_raw_mode(0o600),
    )?;

    let write_result = (|| -> Result<()> {
        let mut buf = content.as_bytes();
        while !buf.is_empty() {
            let n = rustix::io::write(&tmp_fd, buf)?;
            buf = &buf[n..];
        }
        fs::fchmod(&tmp_fd, Mode::from_raw_mode(0o600))?;
        Ok(())
    })();
    drop(tmp_fd);

    if let Err(e) = write_result {
        let _ = fs::unlinkat(&dirfd, tmp_name.as_str(), AtFlags::empty());
        return Err(e);
    }

    // Re-check immediately before the rename: closes the window between the
    // symlink refusal above and the write completing.
    refuse_if_symlink(&dirfd, name)?;
    fs::renameat(&dirfd, tmp_name.as_str(), &dirfd, name)?;
    Ok(())
}

pub fn read_flag_raw(flag_path: &Path, max_bytes: usize) -> Result<String> {
    let dir = flag_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = flag_path.file_name().ok_or_else(|| {
        SafeIoError::Io(io::Error::new(io::ErrorKind::InvalidInput, "no filename"))
    })?;

    let dirfd = open_existing_verified_dir(dir)?;

    let st = fs::statat(&dirfd, name, AtFlags::SYMLINK_NOFOLLOW)?;
    let ft = FileType::from_raw_mode(st.st_mode);
    if ft == FileType::Symlink {
        return Err(SafeIoError::IsSymlink);
    }
    if ft != FileType::RegularFile {
        return Err(SafeIoError::NotAFile);
    }
    if st.st_size as u64 > max_bytes as u64 {
        return Err(SafeIoError::TooLarge(max_bytes));
    }

    let fd = fs::openat(&dirfd, name, read_flags(), Mode::empty())?;
    let capacity = max_bytes.saturating_add(1);
    let mut buf = vec![0u8; capacity];
    let mut total = 0;
    while total < capacity {
        let n = rustix::io::read(&fd, &mut buf[total..])?;
        if n == 0 {
            break;
        }
        total += n;
    }
    if total > max_bytes {
        return Err(SafeIoError::TooLarge(max_bytes));
    }
    buf.truncate(total);
    String::from_utf8(buf)
        .map_err(|e| SafeIoError::Io(io::Error::new(io::ErrorKind::InvalidData, e)))
}

pub fn append_line(path: &Path, line: &str) -> Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path.file_name().ok_or_else(|| {
        SafeIoError::Io(io::Error::new(io::ErrorKind::InvalidInput, "no filename"))
    })?;

    let dirfd = open_verified_dir(dir)?;
    refuse_if_symlink(&dirfd, name)?;

    let fd = open_append_create(&dirfd, name)?;
    fs::fchmod(&fd, Mode::from_raw_mode(0o600))?;

    let trimmed = line.strip_suffix('\n').unwrap_or(line);
    let mut out = String::with_capacity(trimmed.len() + 1);
    out.push_str(trimmed);
    out.push('\n');

    let mut buf = out.as_bytes();
    while !buf.is_empty() {
        let n = rustix::io::write(&fd, buf)?;
        buf = &buf[n..];
    }
    Ok(())
}

pub fn remove_file_if_contains(path: &Path, marker: &str) -> Result<bool> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path.file_name().ok_or_else(|| {
        SafeIoError::Io(io::Error::new(io::ErrorKind::InvalidInput, "no filename"))
    })?;
    let dirfd = match open_existing_verified_dir(dir) {
        Ok(fd) => fd,
        Err(SafeIoError::Io(error)) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    let stat = match fs::statat(&dirfd, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => stat,
        Err(error) if error == rustix::io::Errno::NOENT => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let file_type = FileType::from_raw_mode(stat.st_mode);
    if file_type == FileType::Symlink {
        return Err(SafeIoError::IsSymlink);
    }
    if file_type != FileType::RegularFile {
        return Err(SafeIoError::NotAFile);
    }
    if stat.st_size as u64 > crate::MAX_CONFIG_BYTES as u64 {
        return Err(SafeIoError::TooLarge(crate::MAX_CONFIG_BYTES));
    }
    let fd = fs::openat(&dirfd, name, read_flags(), Mode::empty())?;
    let mut content = Vec::with_capacity(stat.st_size as usize);
    let mut chunk = [0u8; 8192];
    loop {
        match rustix::io::read(&fd, &mut chunk) {
            Ok(0) => break,
            Ok(n) => content.extend_from_slice(&chunk[..n]),
            Err(error) => return Err(error.into()),
        }
    }
    if !String::from_utf8_lossy(&content).contains(marker) {
        return Ok(false);
    }
    fs::unlinkat(&dirfd, name, AtFlags::empty())?;
    Ok(true)
}

pub fn remove_file(path: &Path) -> Result<bool> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path.file_name().ok_or_else(|| {
        SafeIoError::Io(io::Error::new(io::ErrorKind::InvalidInput, "no filename"))
    })?;
    let dirfd = match open_existing_verified_dir(dir) {
        Ok(fd) => fd,
        Err(SafeIoError::Io(error)) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    let stat = match fs::statat(&dirfd, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => stat,
        Err(error) if error == rustix::io::Errno::NOENT => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let file_type = FileType::from_raw_mode(stat.st_mode);
    if file_type == FileType::Symlink {
        return Err(SafeIoError::IsSymlink);
    }
    if file_type != FileType::RegularFile {
        return Err(SafeIoError::NotAFile);
    }
    fs::unlinkat(&dirfd, name, AtFlags::empty())?;
    Ok(true)
}

pub fn read_lines(path: &Path) -> Vec<String> {
    let Ok(lst) = std::fs::symlink_metadata(path) else {
        return Vec::new();
    };
    if lst.file_type().is_symlink() || !lst.is_file() {
        return Vec::new();
    }
    let Ok(dir) = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or(())
    else {
        return Vec::new();
    };
    let Some(name) = path.file_name() else {
        return Vec::new();
    };
    let Ok(dirfd) = open_existing_verified_dir(dir) else {
        return Vec::new();
    };
    let Ok(st) = fs::statat(&dirfd, name, AtFlags::SYMLINK_NOFOLLOW) else {
        return Vec::new();
    };
    let ft = FileType::from_raw_mode(st.st_mode);
    if ft == FileType::Symlink || ft != FileType::RegularFile {
        return Vec::new();
    }
    let Ok(fd) = fs::openat(&dirfd, name, read_flags(), Mode::empty()) else {
        return Vec::new();
    };

    let mut raw = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match rustix::io::read(&fd, &mut chunk) {
            Ok(0) => break,
            Ok(n) => raw.extend_from_slice(&chunk[..n]),
            Err(_) => return Vec::new(),
        }
    }
    let text = String::from_utf8_lossy(&raw);
    text.split('\n')
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_open_mask_retains_its_required_security_and_access_bits() {
        let dir = verified_dir_flags();
        assert!(dir.contains(OFlags::DIRECTORY));
        assert!(dir.contains(OFlags::NOFOLLOW));
        assert!(dir.contains(OFlags::CLOEXEC));

        let existing = append_existing_flags();
        assert!(existing.contains(OFlags::WRONLY));
        assert!(existing.contains(OFlags::APPEND));
        assert!(existing.contains(OFlags::NOFOLLOW));
        assert!(existing.contains(OFlags::CLOEXEC));

        let create = append_create_flags();
        assert!(create.contains(OFlags::WRONLY));
        assert!(create.contains(OFlags::CREATE));
        assert!(create.contains(OFlags::EXCL));
        assert!(create.contains(OFlags::APPEND));
        assert!(create.contains(OFlags::NOFOLLOW));
        assert!(create.contains(OFlags::CLOEXEC));

        let write = create_write_flags();
        assert!(write.contains(OFlags::WRONLY));
        assert!(write.contains(OFlags::CREATE));
        assert!(write.contains(OFlags::EXCL));
        assert!(write.contains(OFlags::NOFOLLOW));
        assert!(write.contains(OFlags::CLOEXEC));

        let read = read_flags();
        assert!(read.contains(OFlags::RDONLY));
        assert!(read.contains(OFlags::NOFOLLOW));
        assert!(read.contains(OFlags::CLOEXEC));
    }
}
