//! Symlink-safe, size-capped, atomic flag and log IO. Frank's security kernel.
//!
//! Ported from `archive/src/hooks/caveman-config.js`'s `safeWriteFlag` /
//! `readFlag` / `appendFlag` / `readHistory`. See `unix.rs` for what changed
//! in the port (dirfd-anchored operations instead of path-based TOCTOU) and
//! what was kept verbatim (the 64-byte cap, whitelist-after-read, 0600 perms,
//! silent-fail-by-`Result`-discard contract).
//!
//! Every function here returns a `Result` rather than swallowing errors
//! internally, unlike the original. Hook call sites are expected to discard
//! it (`.ok()`) to preserve the "a hook always exits 0" contract — but
//! keeping the `Result` here means this crate's own test suite can assert on
//! *why* an operation was refused, which is exactly the coverage gap noted
//! in the archive (`detectMatch`, checksum verification, and friends had zero
//! tests despite deciding what runs on a user's machine).

mod error;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

pub use error::{Result, SafeIoError};

/// Hard cap on a flag file's size. The longest legitimate value in the
/// built-in caveman pack is `"wenyan-ultra"` (12 bytes); 64 leaves slack
/// without opening an exfiltration channel through an oversized flag file.
pub const MAX_FLAG_BYTES: usize = 64;

/// Maximum size for user configuration documents written through the shared
/// safe IO boundary. Configuration is intentionally much larger than a mode
/// flag, but still bounded so a corrupted or hostile file cannot force an
/// unbounded allocation in a hook-adjacent process.
pub const MAX_CONFIG_BYTES: usize = 64 * 1024;
/// Session transcripts may be larger than settings documents, but remain
/// bounded at the parser boundary so a malformed path cannot force an
/// unbounded allocation in a stats command.
pub const MAX_SESSION_BYTES: usize = 16 * 1024 * 1024;

#[cfg(unix)]
use unix as imp;
#[cfg(windows)]
use windows as imp;

use std::path::{Path, PathBuf};

/// Best-effort home directory resolution, used for path defaults (the
/// `$CLAUDE_CONFIG_DIR` fallback, etc.) and by the Windows backend's
/// "symlink target must stay under home" check. Deliberately minimal — no
/// dependency on the `dirs`/`home` crates for two environment variable reads.
pub fn home_dir() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
}

/// Write `content` to `flag_path` atomically and symlink-safely: verifies
/// (and, if the parent is itself a legitimately symlinked directory,
/// resolves and ownership-checks) the parent, refuses if the destination is
/// currently a symlink, writes to a temp file with 0600 permissions, then
/// renames into place.
pub fn write_flag_atomic(flag_path: &Path, content: &str) -> Result<()> {
    imp::write_flag_atomic(flag_path, content)
}

/// Write a bounded text document atomically using the same symlink-safe
/// implementation as flag files. This is the common write primitive for
/// Frank-owned TOML/JSON configuration; callers choose the appropriate cap
/// at the API boundary rather than bypassing this crate with `fs::write`.
pub fn write_text_atomic(path: &Path, content: &str, max_bytes: usize) -> Result<()> {
    if content.len() > max_bytes {
        return Err(SafeIoError::TooLarge(max_bytes));
    }
    imp::write_flag_atomic(path, content)
}

/// Create and verify a directory that will receive Frank-owned files. A
/// symlink to a user-owned directory is allowed (dotfiles setups commonly
/// use one); a symlink to a non-directory or, on Windows, outside the user's
/// home is rejected by the platform backend.
pub fn ensure_dir(path: &Path) -> Result<()> {
    imp::ensure_dir(path)
}

/// Read a bounded UTF-8 document without following a symlink at the file
/// entry. The platform backends perform the metadata and no-follow checks.
pub fn read_text_capped(path: &Path, max_bytes: usize) -> Result<String> {
    imp::read_flag_raw(path, max_bytes)
}

/// Read a flag file, symlink-safely and size-capped, and validate the
/// trimmed/lowercased content against `valid`. Returns `None` on any
/// anomaly — missing file, symlink, oversized, or not on the whitelist —
/// exactly mirroring the original's "return null on any anomaly" contract,
/// because callers (statusline, reinforcement) must never surface a raw
/// value that wasn't validated.
pub fn read_flag(flag_path: &Path, valid: &[&str]) -> Option<String> {
    let raw = imp::read_flag_raw(flag_path, MAX_FLAG_BYTES).ok()?;
    let candidate = raw.trim().to_lowercase();
    if valid.iter().any(|v| *v == candidate) {
        Some(candidate)
    } else {
        None
    }
}

/// Append `line` to `path`, symlink-safely, with a single trailing newline
/// normalized regardless of what `line` already had.
pub fn append_line(path: &Path, line: &str) -> Result<()> {
    imp::append_line(path, line)
}

/// Remove a regular Frank-owned file without following a symlink. The parent
/// is verified and the unlink is anchored to its directory fd; callers supply
/// an ownership marker so uninstall cannot delete an unrelated user file.
pub fn remove_file_if_contains(path: &Path, marker: &str) -> Result<bool> {
    imp::remove_file_if_contains(path, marker)
}

/// Remove a regular Frank-owned file without following a symlink. This is
/// intentionally separate from [`remove_file_if_contains`]: state flags are
/// small, validated values rather than marker-bearing scripts, but they still
/// need the same directory-anchored unlink and no-follow checks.
pub fn remove_file(path: &Path) -> Result<bool> {
    imp::remove_file(path)
}

/// Read a log/history file symlink-safely, split into non-blank lines. No
/// size cap — history is expected to grow with use, matching the original.
/// Returns an empty vec on any anomaly rather than propagating an error,
/// since a missing history file is normal (first run) rather than exceptional.
pub fn read_lines(path: &Path) -> Vec<String> {
    imp::read_lines(path)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::{PermissionsExt, symlink};
    use tempfile::tempdir;

    const MODES: &[&str] = &[
        "off",
        "lite",
        "full",
        "ultra",
        "wenyan-lite",
        "wenyan",
        "wenyan-full",
        "wenyan-ultra",
        "commit",
        "review",
        "compress",
    ];

    #[test]
    fn writes_flag_in_normal_directory() {
        let tmp = tempdir().unwrap();
        let flag_dir = tmp.path().join("claude-config");
        let flag_path = flag_dir.join(".caveman-active");

        write_flag_atomic(&flag_path, "full").unwrap();
        assert_eq!(std::fs::read_to_string(&flag_path).unwrap(), "full");
    }

    #[test]
    fn writes_flag_when_parent_is_symlink_owned_by_current_user() {
        let tmp = tempdir().unwrap();
        let real_dir = tmp.path().join("real-claude-config");
        std::fs::create_dir_all(&real_dir).unwrap();
        let symlink_dir = tmp.path().join("claude-symlink");
        symlink(&real_dir, &symlink_dir).unwrap();

        let flag_path = symlink_dir.join(".caveman-active");
        write_flag_atomic(&flag_path, "ultra").unwrap();

        let real_flag_path = real_dir.join(".caveman-active");
        assert_eq!(std::fs::read_to_string(&real_flag_path).unwrap(), "ultra");
    }

    #[test]
    fn read_flag_works_through_symlinked_parent() {
        let tmp = tempdir().unwrap();
        let real_dir = tmp.path().join("real-claude-config");
        std::fs::create_dir_all(&real_dir).unwrap();
        let symlink_dir = tmp.path().join("claude-symlink");
        symlink(&real_dir, &symlink_dir).unwrap();

        std::fs::write(real_dir.join(".caveman-active"), "lite").unwrap();

        let result = read_flag(&symlink_dir.join(".caveman-active"), MODES);
        assert_eq!(result.as_deref(), Some("lite"));
    }

    #[test]
    fn write_then_read_round_trips_through_symlink() {
        let tmp = tempdir().unwrap();
        let real_dir = tmp.path().join("real-config");
        std::fs::create_dir_all(&real_dir).unwrap();
        let symlink_dir = tmp.path().join("link-config");
        symlink(&real_dir, &symlink_dir).unwrap();

        let flag_path = symlink_dir.join(".caveman-active");
        write_flag_atomic(&flag_path, "wenyan-ultra").unwrap();

        assert_eq!(
            read_flag(&flag_path, MODES).as_deref(),
            Some("wenyan-ultra")
        );
    }

    #[test]
    fn refuses_flag_file_that_is_itself_a_symlink() {
        let tmp = tempdir().unwrap();
        let real_dir = tmp.path().join("real-config");
        std::fs::create_dir_all(&real_dir).unwrap();
        let symlink_dir = tmp.path().join("link-config");
        symlink(&real_dir, &symlink_dir).unwrap();

        let decoy = tmp.path().join("decoy.txt");
        std::fs::write(&decoy, "ATTACK").unwrap();
        let real_flag_path = real_dir.join(".caveman-active");
        symlink(&decoy, &real_flag_path).unwrap();

        let result = write_flag_atomic(&symlink_dir.join(".caveman-active"), "full");
        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(&decoy).unwrap(), "ATTACK");
    }

    #[test]
    fn read_flag_refuses_symlinked_flag_file() {
        let tmp = tempdir().unwrap();
        let real_dir = tmp.path().join("real-config");
        std::fs::create_dir_all(&real_dir).unwrap();

        let secret = tmp.path().join("secret.txt");
        std::fs::write(&secret, "SSH_PRIVATE_KEY_CONTENT").unwrap();
        symlink(&secret, real_dir.join(".caveman-active")).unwrap();

        let result = read_flag(&real_dir.join(".caveman-active"), MODES);
        assert_eq!(result, None);
    }

    #[test]
    fn flag_file_permissions_are_0600_through_symlink() {
        let tmp = tempdir().unwrap();
        let real_dir = tmp.path().join("real-config");
        std::fs::create_dir_all(&real_dir).unwrap();
        let symlink_dir = tmp.path().join("link-config");
        symlink(&real_dir, &symlink_dir).unwrap();

        write_flag_atomic(&symlink_dir.join(".caveman-active"), "full").unwrap();

        let mode = std::fs::metadata(real_dir.join(".caveman-active"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn overwrites_existing_flag_through_symlinked_parent() {
        let tmp = tempdir().unwrap();
        let real_dir = tmp.path().join("real-config");
        std::fs::create_dir_all(&real_dir).unwrap();
        let symlink_dir = tmp.path().join("link-config");
        symlink(&real_dir, &symlink_dir).unwrap();

        let flag_path = symlink_dir.join(".caveman-active");
        write_flag_atomic(&flag_path, "lite").unwrap();
        assert_eq!(read_flag(&flag_path, MODES).as_deref(), Some("lite"));

        write_flag_atomic(&flag_path, "ultra").unwrap();
        assert_eq!(read_flag(&flag_path, MODES).as_deref(), Some("ultra"));
    }

    #[test]
    fn creates_parent_directory_when_missing() {
        let tmp = tempdir().unwrap();
        let flag_path = tmp
            .path()
            .join("nonexistent")
            .join("nested")
            .join(".caveman-active");

        write_flag_atomic(&flag_path, "full").unwrap();
        assert!(flag_path.exists());
        assert_eq!(std::fs::read_to_string(&flag_path).unwrap(), "full");
    }

    #[test]
    fn symlink_to_nonexistent_target_fails_without_panicking() {
        let tmp = tempdir().unwrap();
        let symlink_dir = tmp.path().join("broken-link");
        if symlink("/nonexistent/path/that/does/not/exist", &symlink_dir).is_err() {
            return; // couldn't create the symlink; nothing to assert
        }

        let flag_path = symlink_dir.join(".caveman-active");
        let result = write_flag_atomic(&flag_path, "full");
        assert!(result.is_err());
        assert!(!flag_path.exists());
    }

    #[test]
    fn all_valid_modes_round_trip_through_symlinked_parent() {
        let tmp = tempdir().unwrap();
        let real_dir = tmp.path().join("real-config");
        std::fs::create_dir_all(&real_dir).unwrap();
        let symlink_dir = tmp.path().join("link-config");
        symlink(&real_dir, &symlink_dir).unwrap();

        let flag_path = symlink_dir.join(".caveman-active");
        for mode in MODES {
            write_flag_atomic(&flag_path, mode).unwrap();
            assert_eq!(read_flag(&flag_path, MODES).as_deref(), Some(*mode));
        }
    }

    #[test]
    fn refuses_symlinked_parent_owned_by_a_different_uid_is_untestable_here() {
        // A genuine cross-uid test needs a container/CI job that can create a
        // second user; see AGENTS.md / the plan's testing section. Left as a
        // documented gap rather than a fake positive.
    }

    #[test]
    fn read_flag_rejects_oversized_content() {
        let tmp = tempdir().unwrap();
        let flag_path = tmp.path().join(".caveman-active");
        std::fs::write(&flag_path, "x".repeat(MAX_FLAG_BYTES + 1)).unwrap();

        assert_eq!(read_flag(&flag_path, MODES), None);
    }

    #[test]
    fn read_flag_rejects_non_whitelisted_value() {
        let tmp = tempdir().unwrap();
        let flag_path = tmp.path().join(".caveman-active");
        std::fs::write(&flag_path, "not-a-real-mode").unwrap();

        assert_eq!(read_flag(&flag_path, MODES), None);
    }

    #[test]
    fn supports_non_utf8_unix_filenames_without_following_them() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join(std::ffi::OsString::from_vec(vec![
            b'f', b'r', b'a', b'n', b'k', b'-', 0xff,
        ]));

        if let Err(error) = write_flag_atomic(&path, "full") {
            // APFS on macOS rejects byte sequences that are not valid UTF-8
            // at the filesystem boundary. Linux filesystems permit them, so
            // the same test remains active there instead of being a fake
            // unconditional pass on every platform.
            if matches!(error, SafeIoError::Io(ref e) if e.kind() == std::io::ErrorKind::PermissionDenied || e.raw_os_error() == Some(92))
            {
                return;
            }
            panic!("unexpected non-UTF-8 path error: {error:?}");
        }
        assert_eq!(read_flag(&path, MODES).as_deref(), Some("full"));
    }

    #[test]
    fn reading_a_missing_path_does_not_create_its_parent() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("missing").join("flag");

        assert!(read_text_capped(&path, MAX_FLAG_BYTES).is_err());
        assert!(!tmp.path().join("missing").exists());
    }

    #[test]
    fn refuses_a_parent_symlink_that_resolves_to_a_file() {
        let tmp = tempdir().unwrap();
        let file = tmp.path().join("not-a-directory");
        std::fs::write(&file, "secret").unwrap();
        let parent = tmp.path().join("parent-link");
        symlink(&file, &parent).unwrap();

        let error = read_text_capped(&parent.join("flag"), MAX_FLAG_BYTES).unwrap_err();
        assert!(matches!(error, SafeIoError::SymlinkTargetNotDir));
    }

    #[test]
    fn removes_only_a_regular_file_with_the_requested_marker() {
        let tmp = tempdir().unwrap();
        let managed = tmp.path().join("managed");
        let user = tmp.path().join("user");
        std::fs::write(&managed, "#!/bin/sh\n# frank-managed\n").unwrap();
        std::fs::write(&user, "#!/bin/sh\n# user-owned\n").unwrap();

        assert!(remove_file_if_contains(&managed, "frank-managed").unwrap());
        assert!(!managed.exists());
        assert!(!remove_file_if_contains(&user, "frank-managed").unwrap());
        assert!(user.exists());
    }

    #[test]
    fn refuses_to_remove_a_managed_symlink() {
        let tmp = tempdir().unwrap();
        let secret = tmp.path().join("secret");
        let link = tmp.path().join("managed");
        std::fs::write(&secret, "frank-managed").unwrap();
        symlink(&secret, &link).unwrap();

        assert!(matches!(
            remove_file_if_contains(&link, "frank-managed"),
            Err(SafeIoError::IsSymlink)
        ));
        assert!(secret.exists());
    }

    #[test]
    fn append_line_writes_and_appends_with_single_trailing_newline() {
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join(".caveman-mode-log.jsonl");

        append_line(&log_path, r#"{"mode":"full"}"#).unwrap();
        // A trailing newline already present must not become two.
        append_line(&log_path, "{\"mode\":\"ultra\"}\n").unwrap();

        let content = std::fs::read_to_string(&log_path).unwrap();
        assert_eq!(content, "{\"mode\":\"full\"}\n{\"mode\":\"ultra\"}\n");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], r#"{"mode":"full"}"#);
    }

    #[test]
    fn concurrent_appends_yield_well_formed_lines() {
        let tmp = tempdir().unwrap();
        let log_path = tmp.path().join(".caveman-mode-log.jsonl");
        std::fs::create_dir_all(tmp.path()).unwrap();

        let handles: Vec<_> = (0..16)
            .map(|i| {
                let path = log_path.clone();
                std::thread::spawn(move || {
                    append_line(&path, &format!(r#"{{"n":{i}}}"#)).unwrap();
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }

        let content = std::fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 16);
        for line in lines {
            assert!(line.starts_with('{') && line.ends_with('}'));
        }
    }

    #[test]
    fn read_lines_refuses_symlinked_file() {
        let tmp = tempdir().unwrap();
        let secret = tmp.path().join("secret.txt");
        std::fs::write(&secret, "line1\nline2\n").unwrap();
        let link = tmp.path().join("history.jsonl");
        symlink(&secret, &link).unwrap();

        assert_eq!(read_lines(&link), Vec::<String>::new());
    }

    #[test]
    fn read_lines_filters_blank_lines() {
        let tmp = tempdir().unwrap();
        let path = tmp.path().join("history.jsonl");
        std::fs::write(&path, "a\n\nb\n\n\nc\n").unwrap();

        assert_eq!(read_lines(&path), vec!["a", "b", "c"]);
    }
}
