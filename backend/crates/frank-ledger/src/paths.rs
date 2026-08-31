use std::path::{Path, PathBuf};

/// The two ledger files under `$CLAUDE_CONFIG_DIR`: the per-turn injection
/// log (`build_session_report`'s measured-bytes input) and the lifetime
/// per-session history (`aggregate_history`'s input). Naming them once here
/// keeps the CLI, the hook entry points, and the desktop adapter from
/// re-joining the same literals and drifting apart.
pub struct LedgerPaths {
    pub ledger: PathBuf,
    pub history: PathBuf,
}

impl LedgerPaths {
    pub fn under(config_dir: &Path) -> Self {
        LedgerPaths {
            ledger: config_dir.join(".frank-ledger.jsonl"),
            history: config_dir.join(".frank-history.jsonl"),
        }
    }
}
