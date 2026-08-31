//! Small builder around Sha256 for the prepare/apply staleness
//! fingerprints. `state_fingerprint`, `pack_state_fingerprint`,
//! `current_fingerprint`, and plan-id derivation each used to build a
//! `Sha256` by hand and thread `path.to_string_lossy()` +
//! `fingerprint_path(path)` calls one at a time; this collects that
//! pattern into a single builder so every fingerprint site folds a path's
//! identity and content digest together the same way.

use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

pub(crate) struct Fingerprint(Sha256);

impl Fingerprint {
    pub(crate) fn new() -> Self {
        Fingerprint(Sha256::new())
    }

    pub(crate) fn field(mut self, bytes: impl AsRef<[u8]>) -> Self {
        self.0.update(bytes.as_ref());
        self
    }

    /// Fold in a path's own identity together with its symlink-safe,
    /// size/mtime/bounded-content change-detection digest. The two are
    /// always folded together — a renamed-but-otherwise-identical file
    /// must still change the fingerprint.
    pub(crate) fn path(self, path: &Path) -> Self {
        let digest = fingerprint_path(path);
        self.field(path.to_string_lossy().as_bytes()).field(digest)
    }

    pub(crate) fn finish(self) -> String {
        format!("{:x}", self.0.finalize())
    }
}

/// Symlink-safe, size/mtime/bounded-content change-detection digest for a
/// single path, independent of the path's own name.
fn fingerprint_path(path: &Path) -> String {
    let mut h = Sha256::new();
    match std::fs::symlink_metadata(path) {
        Ok(m) => {
            h.update([1, m.file_type().is_symlink() as u8]);
            h.update(m.len().to_le_bytes());
            h.update(
                m.modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
                    .to_le_bytes(),
            );
            if m.file_type().is_symlink() {
                if let Ok(target) = std::fs::read_link(path) {
                    h.update(target.as_os_str().to_string_lossy().as_bytes());
                }
            } else if m.is_file() {
                // Metadata alone is not enough: an external edit can
                // preserve size and, on coarse filesystems, timestamps.
                // Hash bounded file contents as part of the stale-plan
                // guard, while avoiding an unbounded read of a hostile path.
                h.update(fingerprint_file_contents(path).as_bytes());
            }
        }
        Err(_) => h.update([0]),
    }
    format!("{:x}", h.finalize())
}

/// Bounded digest of a regular file's contents: the caller already
/// resolved that `path` is a file, not a symlink or directory. Reads at
/// most `MAX_CONFIG_BYTES + 1` bytes so a hostile path can't force an
/// unbounded read, and folds truncation/read-error sentinels into the
/// digest so "read failed" and "read succeeded but was capped" are both
/// distinguishable from a real empty-file digest.
pub(crate) fn fingerprint_file_contents(path: &Path) -> String {
    let mut file_digest = Sha256::new();
    if let Ok(file) = std::fs::File::open(path) {
        let mut contents = Vec::new();
        let read_result = file
            .take(frank_safeio::MAX_CONFIG_BYTES.saturating_add(1) as u64)
            .read_to_end(&mut contents);
        let amount = contents.len().min(frank_safeio::MAX_CONFIG_BYTES);
        file_digest.update(&contents[..amount]);
        if read_result.is_err() {
            file_digest.update([0xff]);
        }
        if amount == frank_safeio::MAX_CONFIG_BYTES {
            file_digest.update([0xfe]);
        }
    } else {
        file_digest.update([0xfd]);
    }
    format!("{:x}", file_digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn bounded_file_fingerprints_include_content_and_error_sentinels() {
        let tmp = tempdir().unwrap();
        let first = tmp.path().join("first");
        let second = tmp.path().join("second");
        std::fs::write(&first, "same-size-a").unwrap();
        std::fs::write(&second, "same-size-b").unwrap();
        assert_ne!(
            fingerprint_file_contents(&first),
            fingerprint_file_contents(&second)
        );
        assert_ne!(
            fingerprint_file_contents(&first),
            fingerprint_file_contents(&tmp.path().join("missing"))
        );

        let exact = tmp.path().join("exact");
        let content = vec![b'x'; frank_safeio::MAX_CONFIG_BYTES];
        std::fs::write(&exact, &content).unwrap();
        let mut expected = Sha256::new();
        expected.update(&content);
        expected.update([0xfe]);
        assert_eq!(
            fingerprint_file_contents(&exact),
            format!("{:x}", expected.finalize())
        );
    }
}
