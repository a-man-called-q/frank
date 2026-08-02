//! Flag file path resolution.
//!
//! M0 hardcodes the Claude Code convention (`$CLAUDE_CONFIG_DIR` or
//! `~/.claude`) since that's the only target this milestone wires up end to
//! end (see AGENTS.md's milestone table — M2 is "Claude Code only"). M1
//! promotes this into `frank-state` alongside the full config-precedence
//! chain (env var > repo-local > user config > pack default) ported from
//! `archive/src/hooks/caveman-config.js:1-110`; M2's installer will need the
//! same resolution for other targets, at which point this becomes
//! target-aware rather than Claude-Code-only.

use std::path::PathBuf;

use frank_app::FrankPaths;

pub fn config_dir() -> PathBuf {
    FrankPaths::from_process().config_dir
}

/// Named `.frank-active`, not `.caveman-active` — this is Frank's own flag,
/// not a caveman-specific one. Any pack's active level lives here.
pub fn path() -> PathBuf {
    config_dir().join(".frank-active")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_paths_are_derived_from_the_same_process_configuration() {
        let paths = FrankPaths::from_process();
        assert_eq!(config_dir(), paths.config_dir);
        assert_eq!(path(), paths.config_dir.join(".frank-active"));
    }
}
