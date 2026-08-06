//! Applies an [`Intent`] to the flag file, including the one-shot
//! commit/review/compress save-and-restore dance (#599) and the
//! mode-transition log (#601). This is the impure counterpart to
//! `intent::classify` — every filesystem operation here goes through
//! `frank-safeio`, never a raw path.
//!
//! Ported from `archive/src/hooks/caveman-mode-tracker.js`'s tail half
//! (lines ~134-197) and `caveman-config.js`'s `recordModeChange`.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use frank_pack::CompiledPack;

use crate::intent::Intent;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppliedState {
    Off,
    Level(String),
    Oneshot(String),
    /// `/<prefix>-stats` was requested; the flag was not touched at all.
    /// The caller (frank-cli's hook, eventually frank-ledger) is
    /// responsible for actually producing the stats report.
    StatsRequested(Vec<String>),
}

pub struct FlagPaths {
    pub active: PathBuf,
    pub prev: PathBuf,
    pub mode_log: PathBuf,
}

impl FlagPaths {
    pub fn under(config_dir: &Path) -> Self {
        FlagPaths {
            active: config_dir.join(".frank-active"),
            prev: config_dir.join(".frank-active.prev"),
            mode_log: config_dir.join(".frank-mode-log.jsonl"),
        }
    }
}

fn record_mode_change(paths: &FlagPaths, pack: &CompiledPack, next: Option<&str>) {
    let current = frank_safeio::read_flag(&paths.active, &pack.valid_flag_values());
    if current.as_deref() == next {
        return;
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let line = serde_json::json!({ "ts": ts, "mode": next, "prev": current }).to_string();
    let _ = frank_safeio::append_line(&paths.mode_log, &line);
}

fn deactivate(paths: &FlagPaths, pack: &CompiledPack) {
    record_mode_change(paths, pack, None);
    let _ = frank_safeio::remove_file(&paths.active);
    let _ = frank_safeio::remove_file(&paths.prev);
}

fn activate(paths: &FlagPaths, pack: &CompiledPack, level: &str) {
    record_mode_change(paths, pack, Some(level));
    let _ = frank_safeio::write_flag_atomic(&paths.active, level);
}

/// Apply `intent` to the flag state, then run the one-shot restore check
/// unconditionally (it must run every turn, not just turns where a oneshot
/// was just set — that's what lets the restore happen on the *next*
/// ordinary prompt after a one-shot command).
pub fn apply(intent: &Intent, pack: &CompiledPack, paths: &FlagPaths) -> AppliedState {
    if let Intent::Stats(args) = intent {
        return AppliedState::StatsRequested(args.clone());
    }

    let mut set_oneshot_this_turn = false;

    match intent {
        Intent::Deactivate => deactivate(paths, pack),
        Intent::Activate(level) => activate(paths, pack, level),
        Intent::Oneshot(id) => {
            let current = frank_safeio::read_flag(&paths.active, &pack.valid_flag_values());
            if let Some(cur) = current.as_deref() {
                if !pack.oneshots.contains_key(cur) {
                    let _ = frank_safeio::write_flag_atomic(&paths.prev, cur);
                }
            }
            set_oneshot_this_turn = true;
            record_mode_change(paths, pack, Some(id));
            let _ = frank_safeio::write_flag_atomic(&paths.active, id);
        }
        Intent::None | Intent::Stats(_) => {}
    }

    let mut active = frank_safeio::read_flag(&paths.active, &pack.valid_flag_values());

    if let Some(cur) = active.clone() {
        if pack.oneshots.contains_key(&cur) && !set_oneshot_this_turn {
            let prev = frank_safeio::read_flag(&paths.prev, &pack.valid_flag_values());
            let _ = frank_safeio::remove_file(&paths.prev);
            match prev.filter(|p| !pack.oneshots.contains_key(p)) {
                Some(p) => {
                    record_mode_change(paths, pack, Some(&p));
                    let _ = frank_safeio::write_flag_atomic(&paths.active, &p);
                    active = Some(p);
                }
                None => {
                    record_mode_change(paths, pack, None);
                    let _ = frank_safeio::remove_file(&paths.active);
                    active = None;
                }
            }
        }
    }

    match active {
        None => AppliedState::Off,
        Some(id) if pack.oneshots.contains_key(&id) => AppliedState::Oneshot(id),
        Some(id) => AppliedState::Level(id),
    }
}
