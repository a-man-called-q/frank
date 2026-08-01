//! Frank's own record of what it injected, byte-exactly — the piece the
//! archive never had at all. `caveman-stats.js` logged mode *transitions*
//! but never the size of what `caveman-activate.js` / the mode-tracker
//! actually wrote into the model's context, so there was no way to check
//! the archive's own "~1-1.5k tokens per turn" claim against real data.
//!
//! One line per hook invocation in `$CLAUDE_CONFIG_DIR/.frank-ledger.jsonl`:
//! `{"ts":…,"kind":"activate"|"reinforce","session":…,"level":…,"inject_bytes":…}`.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectionEntry {
    pub ts: i64,
    pub kind: String,
    #[serde(default)]
    pub session: Option<String>,
    #[serde(default)]
    pub level: Option<String>,
    pub inject_bytes: usize,
}

pub fn append(path: &Path, entry: &InjectionEntry) {
    if let Ok(line) = serde_json::to_string(entry) {
        let _ = frank_safeio::append_line(path, &line);
    }
}

pub fn read_all(path: &Path) -> Vec<InjectionEntry> {
    frank_safeio::read_lines(path)
        .iter()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

pub fn totals_for_session(entries: &[InjectionEntry], session_id: &str) -> (usize, usize) {
    let mut activate = 0usize;
    let mut reinforce = 0usize;
    for e in entries {
        if e.session.as_deref() != Some(session_id) {
            continue;
        }
        match e.kind.as_str() {
            "activate" => activate += e.inject_bytes,
            "reinforce" => reinforce += e.inject_bytes,
            _ => {}
        }
    }
    (activate, reinforce)
}
