//! Evaluates a [`TargetManifest`]'s `[[detect]]` clauses against a
//! [`ProbeEnv`]. All 8 probe kinds the archive's DSL supported
//! (`archive/bin/install.js:279-334`), each a typed field instead of a
//! string tag — see `manifest.rs`'s module docs for why that matters.

use std::path::Path;

use regex::Regex;

use crate::manifest::{DetectClause, TargetManifest};
use crate::plan::{Detection, ProbeEnv};

/// Recursive extension-directory scan, depth-capped the same way the
/// archive's `walkDir` was (`archive/bin/install.js:324-334`) — this
/// exists to defend against symlink cycles under a plugin directory, not
/// because 4 levels has any significance beyond "deeper than any real
/// extension ever nests".
fn walk_basenames(root: &Path, depth: u32, out: &mut Vec<String>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            out.push(name.to_string());
        }
        if path.is_dir() {
            walk_basenames(&path, depth - 1, out);
        }
    }
}

fn vscode_ext_roots(env: &ProbeEnv) -> Vec<std::path::PathBuf> {
    [".vscode/extensions", ".vscode-server/extensions", ".cursor/extensions", ".windsurf/extensions"]
        .iter()
        .filter_map(|rel| env.home.as_ref().map(|h| h.join(rel)))
        .collect()
}

fn regex_matches_any_basename(re: &Regex, dirs: &[std::path::PathBuf]) -> bool {
    dirs.iter().any(|dir| {
        std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_str().is_some_and(|n| re.is_match(n)))
    })
}

fn eval_clause(clause: &DetectClause, env: &ProbeEnv) -> bool {
    // AND semantics: every field present in this clause must match. A
    // clause with no fields set at all matches nothing (defensive; the
    // manifest schema shouldn't produce this, but it must not silently
    // read as "always true").
    let mut any_field = false;
    let mut all_matched = true;

    if let Some(cmd) = &clause.command {
        any_field = true;
        all_matched &= env.has_command(cmd);
    }
    if let Some(dir) = &clause.dir {
        any_field = true;
        all_matched &= env.expand(dir).is_some_and(|p| p.is_dir());
    }
    if let Some(file) = &clause.file {
        any_field = true;
        all_matched &= env.expand(file).is_some_and(|p| p.is_file());
    }
    if let Some(app) = &clause.macapp {
        any_field = true;
        let found = env.is_macos
            && env.home.as_ref().is_some_and(|h| {
                Path::new("/Applications").join(format!("{app}.app")).is_dir()
                    || h.join("Applications").join(format!("{app}.app")).is_dir()
            });
        all_matched &= found;
    }
    if let Some(pattern) = &clause.vscode_ext {
        any_field = true;
        let found = Regex::new(&format!("(?i){pattern}"))
            .is_ok_and(|re| regex_matches_any_basename(&re, &vscode_ext_roots(env)));
        all_matched &= found;
    }
    if let Some(pattern) = &clause.cursor_ext {
        any_field = true;
        let dirs: Vec<_> = env.home.iter().map(|h| h.join(".cursor/extensions")).collect();
        let found = Regex::new(&format!("(?i){pattern}")).is_ok_and(|re| regex_matches_any_basename(&re, &dirs));
        all_matched &= found;
    }
    if let Some(true) = clause.jetbrains_config {
        any_field = true;
        let found = env.home.as_ref().is_some_and(|h| {
            h.join("Library/Application Support/JetBrains").is_dir() || h.join(".config/JetBrains").is_dir()
        });
        all_matched &= found;
    }
    if let Some(pattern) = &clause.jetbrains_plugin {
        any_field = true;
        let mut names = Vec::new();
        if let Some(h) = &env.home {
            for root in [h.join("Library/Application Support/JetBrains"), h.join(".config/JetBrains")] {
                walk_basenames(&root, 4, &mut names);
            }
        }
        let found = Regex::new(&format!("(?i){pattern}")).is_ok_and(|re| names.iter().any(|n| re.is_match(n)));
        all_matched &= found;
    }
    if let Some(cv) = &clause.command_version {
        any_field = true;
        let found = std::process::Command::new(&cv.bin)
            .args(&cv.args)
            .output()
            .ok()
            .and_then(|o| {
                let combined = format!(
                    "{}{}",
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                );
                Regex::new(&cv.matches).ok().map(|re| re.is_match(&combined))
            })
            .unwrap_or(false);
        all_matched &= found;
    }

    any_field && all_matched
}

pub fn detect(manifest: &TargetManifest, env: &ProbeEnv) -> Detection {
    if manifest.detect.iter().any(|clause| eval_clause(clause, env)) {
        Detection::Detected
    } else {
        Detection::NotDetected
    }
}
