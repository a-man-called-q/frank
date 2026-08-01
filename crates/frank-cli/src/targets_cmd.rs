//! `frank targets` and the manifest lookup shared with `frank install
//! --only <id>` / `frank uninstall --only <id>` for generic (declarative)
//! targets.

use std::path::PathBuf;

use frank_target::manifest::TargetManifest;
use frank_target::plan::{Detection, ProbeEnv};

/// Where declarative target manifests live: the user's installed
/// manifests, plus `./targets` for local development (this repo's own
/// `targets/*.toml`, so `cargo run -p frank-cli -- targets` works without
/// an install step).
pub fn targets_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        dirs.push(PathBuf::from(xdg).join("frank/targets"));
    } else if let Some(home) = frank_safeio::home_dir() {
        dirs.push(home.join(".config/frank/targets"));
    }
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join("targets"));
    }
    dirs
}

pub fn load_all_manifests() -> Vec<(PathBuf, Result<TargetManifest, String>)> {
    let mut out = Vec::new();
    let mut seen_ids = std::collections::HashSet::new();
    for dir in targets_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let parsed = std::fs::read_to_string(&path)
                .map_err(|e| e.to_string())
                .and_then(|raw| toml::from_str::<TargetManifest>(&raw).map_err(|e| e.to_string()));
            if let Ok(m) = &parsed {
                // First directory wins for a given id (user-installed
                // manifests shadow the dev-mode ./targets copy).
                if !seen_ids.insert(m.target.id.clone()) {
                    continue;
                }
            }
            out.push((path, parsed));
        }
    }
    out
}

pub fn find_manifest(id: &str) -> Option<TargetManifest> {
    load_all_manifests()
        .into_iter()
        .find_map(|(_, parsed)| match parsed {
            Ok(m) if m.target.id == id => Some(m),
            _ => None,
        })
}

pub fn run(detected_only: bool, json: bool) -> i32 {
    let env = ProbeEnv::from_process();
    let manifests = load_all_manifests();

    #[derive(serde::Serialize)]
    struct Row {
        id: String,
        label: String,
        kind: String,
        verified: bool,
        soft: bool,
        detected: bool,
        source: String,
    }

    let mut rows = vec![Row {
        id: "claude-code".to_string(),
        label: "Claude Code".to_string(),
        kind: "native".to_string(),
        verified: true,
        soft: false,
        detected: frank_target::claude_code::ClaudeCodeTarget::detect(&env) == Detection::Detected,
        source: "built-in".to_string(),
    }];

    let mut parse_errors = Vec::new();
    for (path, parsed) in manifests {
        match parsed {
            Ok(m) => {
                let detected = frank_target::detect::detect(&m, &env) == Detection::Detected;
                rows.push(Row {
                    id: m.target.id,
                    label: m.target.label,
                    kind: m.target.kind,
                    verified: m.target.verified,
                    soft: m.target.soft,
                    detected,
                    source: path.display().to_string(),
                });
            }
            Err(e) => parse_errors.push(format!("{}: {e}", path.display())),
        }
    }

    if detected_only {
        rows.retain(|r| r.detected);
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&rows).unwrap());
    } else {
        for r in &rows {
            let mark = if r.detected { "\u{2713}" } else { " " };
            let ver = if r.verified { "verified" } else { "unverified" };
            let soft = if r.soft { ", soft" } else { "" };
            println!(
                "[{mark}] {:<14} {:<20} {} ({ver}{soft})",
                r.id, r.label, r.kind
            );
        }
    }
    for e in &parse_errors {
        eprintln!("frank: failed to parse target manifest {e}");
    }

    if parse_errors.is_empty() { 0 } else { 1 }
}
