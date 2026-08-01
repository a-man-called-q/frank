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
    let name = flag_path
        .file_name()
        .ok_or_else(|| SafeIoError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, "no filename")))?;
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
    let mut buf = vec![0u8; max_bytes];
    let n = f.read(&mut buf)?;
    buf.truncate(n);
    String::from_utf8(buf)
        .map_err(|e| SafeIoError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
}

pub fn append_line(path: &Path, line: &str) -> Result<()> {
    let dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let real_dir = verify_dir(dir)?;
    let name = path
        .file_name()
        .ok_or_else(|| SafeIoError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, "no filename")))?;
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
