//! Default-level resolution precedence.
//!
//! Ported from `archive/src/hooks/caveman-config.js`'s `getDefaultMode` /
//! `findRepoConfigPath` / `readModeFromConfigFile`, with two adaptations:
//! TOML instead of JSON (consistent with the rest of Frank's config
//! surface), and validation against the *active pack's* level ids/aliases
//! (plus the `"off"` sentinel) instead of a hardcoded `VALID_MODES` list —
//! a third-party pack's levels are valid defaults too.
//!
//! Precedence, highest to lowest:
//! 1. `$FRANK_DEFAULT_LEVEL` environment variable
//! 2. Repo-local config: walk up from the given directory looking for
//!    `.frank/config.toml` or `.frank.toml` (first match wins), bounded to
//!    64 levels and refusing symlinks — symmetric with `frank-safeio`'s
//!    flag-file policy.
//! 3. User config: `$XDG_CONFIG_HOME/frank/config.toml`, falling back per
//!    platform.
//! 4. The active pack's `default_level`.

use std::path::{Path, PathBuf};

use frank_pack::CompiledPack;
use serde::Deserialize;

const MAX_WALK_LEVELS: usize = 64;
const REPO_CANDIDATES: &[&str] = &[".frank/config.toml", ".frank.toml"];

#[derive(Deserialize)]
struct ConfigFile {
    default_level: Option<String>,
}

/// Is `value` a legal default: a canonical level id, a level alias, or the
/// literal `"off"` sentinel?
fn is_valid_default(pack: &CompiledPack, value: &str) -> bool {
    value == "off" || pack.resolve_level(value).is_some()
}

fn env_default(pack: &CompiledPack, env_var: &str) -> Option<String> {
    let raw = std::env::var(env_var).ok()?;
    let lower = raw.trim().to_lowercase();
    is_valid_default(pack, &lower).then_some(lower)
}

/// Refuses symlinked config files, matching `readFlag`'s policy — a
/// predictable repo-local config path is exactly the kind of place a local
/// attacker could plant a symlink pointing at a file whose *content* would
/// then be parsed as this project's default level.
fn read_mode_from_file(pack: &CompiledPack, path: &Path) -> Option<String> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    if meta.file_type().is_symlink() || !meta.is_file() {
        return None;
    }
    let raw = std::fs::read_to_string(path).ok()?;
    let parsed: ConfigFile = toml::from_str(&raw).ok()?;
    let candidate = parsed.default_level?.trim().to_lowercase();
    is_valid_default(pack, &candidate).then_some(candidate)
}

fn find_repo_config_path(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    for _ in 0..MAX_WALK_LEVELS {
        for rel in REPO_CANDIDATES {
            let candidate = dir.join(rel);
            if let Ok(meta) = std::fs::symlink_metadata(&candidate) {
                if !meta.file_type().is_symlink() && meta.is_file() {
                    return Some(candidate);
                }
            }
        }
        match dir.parent() {
            Some(parent) if parent != dir => dir = parent.to_path_buf(),
            _ => return None,
        }
    }
    None
}

fn user_config_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg).join("frank"));
    }
    #[cfg(windows)]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return Some(PathBuf::from(appdata).join("frank"));
        }
    }
    frank_safeio::home_dir().map(|h| h.join(".config").join("frank"))
}

/// Resolve the effective default level for `cwd`, following the precedence
/// chain above. `env_var` is normally `"FRANK_DEFAULT_LEVEL"` — parameterized
/// for tests.
pub fn resolve_default_level(pack: &CompiledPack, cwd: &Path, env_var: &str) -> String {
    if let Some(v) = env_default(pack, env_var) {
        return v;
    }
    if let Some(repo_config) = find_repo_config_path(cwd) {
        if let Some(v) = read_mode_from_file(pack, &repo_config) {
            return v;
        }
    }
    if let Some(dir) = user_config_dir() {
        if let Some(v) = read_mode_from_file(pack, &dir.join("config.toml")) {
            return v;
        }
    }
    pack.default_level.clone()
}
