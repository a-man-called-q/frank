use std::path::PathBuf;

use crate::{FrankPaths, LevelSummary, PackSummary};

pub(super) fn pack_summary(
    pack: &frank_pack::CompiledPack,
    active: bool,
    builtin: bool,
) -> PackSummary {
    PackSummary {
        id: pack.id.clone(),
        version: pack.version.clone(),
        active,
        builtin,
        levels: pack
            .levels
            .values()
            .map(|level| LevelSummary {
                id: level.id.clone(),
                title: level.title.clone(),
                aliases: level.aliases.clone(),
            })
            .collect(),
    }
}

pub(super) fn load_manifests(
    paths: &FrankPaths,
) -> Vec<(PathBuf, Result<frank_target::TargetManifest, String>)> {
    let mut dirs = vec![paths.user_config_dir().join("targets")];
    dirs.push(paths.cwd.join("targets"));
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("toml") {
                continue;
            }
            let parsed = frank_safeio::read_text_capped(&path, frank_safeio::MAX_CONFIG_BYTES)
                .map_err(|e| e.to_string())
                .and_then(|raw| {
                    toml::from_str::<frank_target::TargetManifest>(&raw).map_err(|e| e.to_string())
                });
            if let Ok(manifest) = &parsed {
                if !seen.insert(manifest.target.id.clone()) {
                    continue;
                }
            }
            out.push((path, parsed));
        }
    }
    out
}
