//! Windows flag/log IO.
//!
//! Compiles and follows the same contract as the Unix backend, but is
//! **not yet exercised on real Windows** — this workstation is macOS and M0
//! only targets macOS/Linux (see the plan's milestone table). Treat this
//! module as a placeholder to keep the workspace cross-platform-shaped until
//! M6 (distribution) adds Windows CI and real verification.
//!
//! No uid concept exists on Windows, so ownership verification falls back to
//! requiring the resolved path live under the user's home directory, mirroring
//! `archive/src/hooks/caveman-config.js`'s Windows branch.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::{Result, SafeIoError};

const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;

fn verify_dir(dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let lst = std::fs::symlink_metadata(dir)?;
    let real_dir = if lst.file_type().is_symlink() {
        std::fs::canonicalize(dir)?
    } else {
        dir.to_path_buf()
    };
    if !real_dir.is_dir() {
        return Err(SafeIoError::SymlinkTargetNotDir);
    }
    if let Some(home) = crate::home_dir() {
        let home = home.canonicalize().unwrap_or(home);
        if !real_dir.starts_with(&home) {
            return Err(SafeIoError::SymlinkTargetOutsideHome);
        }
    }
    Ok(real_dir)
}

pub fn ensure_dir(path: &Path) -> Result<()> {
    let _ = verify_dir(path)?;
    Ok(())
}

fn is_symlink_at(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

pub fn write_flag_atomic(flag_path: &Path, content: &str) -> Result<()> {
    let dir = flag_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let real_dir = verify_dir(dir)?;
    let name = flag_path.file_name().ok_or_else(|| {
        SafeIoError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "no filename",
        ))
    })?;
    let real_flag_path = real_dir.join(name);

    if is_symlink_at(&real_flag_path) {
        return Err(SafeIoError::IsSymlink);
    }

    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let tmp_path = real_dir.join(format!(".{}.{pid}.{nanos}.tmp", name.to_string_lossy()));

    {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_ATTRIBUTE_HIDDEN)
            .open(&tmp_path)?;
        f.write_all(content.as_bytes())?;
    }

    if is_symlink_at(&real_flag_path) {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(SafeIoError::IsSymlink);
    }
    std::fs::rename(&tmp_path, &real_flag_path)?;
    Ok(())
}

pub fn read_flag_raw(flag_path: &Path, max_bytes: usize) -> Result<String> {
    let lst = std::fs::symlink_metadata(flag_path)?;
    if lst.file_type().is_symlink() {
        return Err(SafeIoError::IsSymlink);
    }
    if !lst.is_file() {
        return Err(SafeIoError::NotAFile);
    }
    if lst.len() > max_bytes as u64 {
        return Err(SafeIoError::TooLarge(max_bytes));
    }
    let mut f = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(flag_path)?;
    let capacity = max_bytes.saturating_add(1);
    let mut buf = vec![0u8; capacity];
    let mut total = 0;
    while total < capacity {
        let n = f.read(&mut buf[total..])?;
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
        .map_err(|e| SafeIoError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
}

pub fn append_line(path: &Path, line: &str) -> Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let real_dir = verify_dir(dir)?;
    let name = path.file_name().ok_or_else(|| {
        SafeIoError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "no filename",
        ))
    })?;
    let real_path = real_dir.join(name);

    if is_symlink_at(&real_path) {
        return Err(SafeIoError::IsSymlink);
    }

    let trimmed = line.strip_suffix('\n').unwrap_or(line);
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&real_path)?;
    f.write_all(trimmed.as_bytes())?;
    f.write_all(b"\n")?;
    Ok(())
}

pub fn remove_file_if_contains(path: &Path, marker: &str) -> Result<bool> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let real_dir = verify_dir(dir)?;
    let name = path.file_name().ok_or_else(|| {
        SafeIoError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "no filename",
        ))
    })?;
    let real_path = real_dir.join(name);
    let metadata = match std::fs::symlink_metadata(&real_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        return Err(SafeIoError::IsSymlink);
    }
    if !metadata.is_file() {
        return Err(SafeIoError::NotAFile);
    }
    if metadata.len() > crate::MAX_CONFIG_BYTES as u64 {
        return Err(SafeIoError::TooLarge(crate::MAX_CONFIG_BYTES));
    }
    let content = std::fs::read_to_string(&real_path)?;
    if !content.contains(marker) {
        return Ok(false);
    }
    std::fs::remove_file(real_path)?;
    Ok(true)
}

pub fn remove_file(path: &Path) -> Result<bool> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let real_dir = verify_dir(dir)?;
    let name = path.file_name().ok_or_else(|| {
        SafeIoError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "no filename",
        ))
    })?;
    let real_path = real_dir.join(name);
    let metadata = match std::fs::symlink_metadata(&real_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        return Err(SafeIoError::IsSymlink);
    }
    if !metadata.is_file() {
        return Err(SafeIoError::NotAFile);
    }
    std::fs::remove_file(real_path)?;
    Ok(true)
}

pub fn read_lines(path: &Path) -> Vec<String> {
    let Ok(lst) = std::fs::symlink_metadata(path) else {
        return Vec::new();
    };
    if lst.file_type().is_symlink() || !lst.is_file() {
        return Vec::new();
    }
    let Ok(mut f) = OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
    else {
        return Vec::new();
    };
    let mut raw = String::new();
    if f.read_to_string(&mut raw).is_err() {
        return Vec::new();
    }
    raw.split('\n')
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect()
}

/// Holds the locked file open for the guard's lifetime. Windows releases a
/// `LockFileEx` lock automatically when the handle is closed, so `Drop`
/// doesn't need an explicit `UnlockFileEx` call — dropping the `File` is
/// enough, same shape as the Unix `flock`-on-an-`OwnedFd` guard.
pub struct LockGuard {
    _file: std::fs::File,
}

/// Try to take an exclusive, non-blocking lock on `name` inside `dir` via
/// `LockFileEx(..., LOCKFILE_FAIL_IMMEDIATELY | LOCKFILE_EXCLUSIVE_LOCK, ...)`.
/// Returns `Ok(None)` — not an error — when another process already holds
/// it, matching the Unix backend's contract.
///
/// Compiles and cross-checks clean (`cargo check --target
/// x86_64-pc-windows-gnu`) but is **not yet exercised on real Windows** --
/// see this module's top-level doc comment.
pub fn try_lock_exclusive(dir: &Path, name: &std::ffi::OsStr) -> Result<Option<LockGuard>> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::{ERROR_IO_PENDING, ERROR_LOCK_VIOLATION};
    use windows_sys::Win32::Storage::FileSystem::{
        LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx,
    };
    use windows_sys::Win32::System::IO::OVERLAPPED;

    let real_dir = verify_dir(dir)?;
    let real_path = real_dir.join(name);
    if is_symlink_at(&real_path) {
        return Err(SafeIoError::IsSymlink);
    }

    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&real_path)?;

    let mut overlapped: OVERLAPPED = unsafe { std::mem::zeroed() };
    // SAFETY: `handle` is a valid, open `HANDLE` for the lifetime of `file`,
    // which outlives this call. `overlapped` is zero-initialized and its
    // address is valid for the duration of the (synchronous, since
    // LOCKFILE_FAIL_IMMEDIATELY never queues an async completion) call.
    let succeeded = unsafe {
        LockFileEx(
            file.as_raw_handle() as _,
            LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
            0,
            u32::MAX,
            u32::MAX,
            &mut overlapped,
        )
    };

    if succeeded != 0 {
        return Ok(Some(LockGuard { _file: file }));
    }

    let error = std::io::Error::last_os_error();
    match error.raw_os_error().map(|code| code as u32) {
        Some(ERROR_LOCK_VIOLATION) | Some(ERROR_IO_PENDING) => Ok(None),
        _ => Err(SafeIoError::Io(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn writes_and_reads_a_path_with_spaces() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("config with spaces").join(".frank-active");
        write_flag_atomic(&path, "full").unwrap();
        assert_eq!(read_flag_raw(&path, 64).unwrap(), "full");
    }

    #[test]
    fn refuses_a_reparse_point_file_without_touching_its_target() {
        use std::os::windows::fs::symlink_file;

        let tmp = tempdir().unwrap();
        let target = tmp.path().join("target.txt");
        std::fs::write(&target, "untouched").unwrap();
        let link = tmp.path().join(".frank-active");
        // Creating a symlink may be denied on an unprivileged Windows CI
        // worker; that environment still gets the real package smoke tests.
        if symlink_file(&target, &link).is_err() {
            return;
        }
        assert!(write_flag_atomic(&link, "full").is_err());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "untouched");
    }

    #[test]
    fn missing_read_does_not_create_a_directory() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("missing").join(".frank-active");
        assert!(read_flag_raw(&path, 64).is_err());
        assert!(!path.parent().unwrap().exists());
    }
}
