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

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rustix::fd::OwnedFd;
use rustix::fs::{self, AtFlags, FileType, Mode, OFlags, CWD};

use crate::error::{Result, SafeIoError};

/// Verify `dir` and open it as an fd every following operation anchors to.
///
/// `dir` may itself be a symlink — the legitimate "`~/.claude` symlinked to a
/// dotfiles repo or shared config volume" pattern — in which case we resolve
/// it and require the real target be a directory owned by the current uid.
/// A symlink pointing at a directory owned by someone else is refused.
fn open_verified_dir(dir: &Path) -> Result<OwnedFd> {
    std::fs::create_dir_all(dir)?;

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
    let dirfd = fs::openat(
        CWD,
        &real_dir,
        OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
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
fn open_append_create(dirfd: &OwnedFd, name: &str) -> Result<rustix::fd::OwnedFd> {
    let open_existing = || {
        fs::openat(
            dirfd,
            name,
            OFlags::WRONLY | OFlags::APPEND | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
    };

    match open_existing() {
        Ok(fd) => return Ok(fd),
        Err(e) if e == rustix::io::Errno::NOENT => {}
        Err(e) => return Err(e.into()),
    }

    match fs::openat(
        dirfd,
        name,
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::APPEND | OFlags::NOFOLLOW | OFlags::CLOEXEC,
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
fn refuse_if_symlink(dirfd: &OwnedFd, name: &str) -> Result<()> {
    match fs::statat(dirfd, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(st) if FileType::from_raw_mode(st.st_mode) == FileType::Symlink => {
            Err(SafeIoError::IsSymlink)
        }
        Ok(_) | Err(_) => Ok(()), // missing, or exists-and-not-a-symlink: proceed
    }
}

pub fn write_flag_atomic(flag_path: &Path, content: &str) -> Result<()> {
    let dir = flag_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = flag_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| SafeIoError::Io(io::Error::new(io::ErrorKind::InvalidInput, "no filename")))?;

    let dirfd = open_verified_dir(dir)?;
    refuse_if_symlink(&dirfd, name)?;

    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let tmp_name = format!(".{name}.{pid}.{nanos}.tmp");

    let tmp_fd = fs::openat(
        &dirfd,
        tmp_name.as_str(),
        OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
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
    let name = flag_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| SafeIoError::Io(io::Error::new(io::ErrorKind::InvalidInput, "no filename")))?;

    let dirfd = fs::openat(
        CWD,
        dir,
        OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )?;

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

    let fd = fs::openat(
        &dirfd,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let mut buf = vec![0u8; max_bytes];
    let n = rustix::io::read(&fd, &mut buf)?;
    buf.truncate(n);
    String::from_utf8(buf).map_err(|e| SafeIoError::Io(io::Error::new(io::ErrorKind::InvalidData, e)))
}

pub fn append_line(path: &Path, line: &str) -> Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| SafeIoError::Io(io::Error::new(io::ErrorKind::InvalidInput, "no filename")))?;

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
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return Vec::new();
    };
    let Ok(dirfd) = fs::openat(CWD, dir, OFlags::DIRECTORY | OFlags::CLOEXEC, Mode::empty())
    else {
        return Vec::new();
    };
    let Ok(fd) = fs::openat(
        &dirfd,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    ) else {
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
