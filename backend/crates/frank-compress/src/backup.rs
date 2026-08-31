//! Out-of-tree backup path resolution — ported from `compress.py`'s
//! `backup_dir_for`. Backups live outside the source directory so a
//! skill/rule auto-loader (Claude Code, opencode, ...) doesn't re-ingest
//! the backup as live context, unlike the archive's `.original.md`
//! sibling convention (which `caveman-stats.js` then had to specially
//! recognize and skip — see the note in `frank-ledger`).

use std::path::{Path, PathBuf};

fn data_home() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                frank_safeio::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("AppData")
                    .join("Local")
            })
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                frank_safeio::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".local")
                    .join("share")
            })
    }
}

/// The source file's parent-directory name is mirrored under the base to
/// reduce cross-project collisions (two `task.md` files in different
/// repos land in different backup subdirectories).
pub fn backup_dir_for(filepath: &Path) -> PathBuf {
    let parent_name = filepath
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("");
    data_home()
        .join("frank-compress")
        .join("backups")
        .join(parent_name)
}

pub fn backup_path_for(filepath: &Path) -> PathBuf {
    let stem = filepath
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    backup_dir_for(filepath).join(format!("{stem}.original.md"))
}
