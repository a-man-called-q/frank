//! Built-in and user-installed pack selection.
//!
//! The built-in caveman pack remains embedded for a fast, dependency-free
//! default. M7 adds a data-only local pack store: selected packs are loaded
//! from the lockfile, digest-checked, and compiled at runtime. Hooks fail
//! closed (no prompt injection) if a selected pack is missing or modified.

use std::io::BufRead;
use std::path::{Path, PathBuf};

use frank_app::{FrankPaths, FrankService};

/// The CLI is an adapter. Pack loading, selection, and process/user paths are
/// owned by `frank-app` so hooks, GUI, and commands cannot silently diverge.
pub fn service() -> FrankService {
    FrankService::new(FrankPaths::from_process())
}

pub fn builtin() -> frank_pack::CompiledPack {
    frank_app::builtin_pack()
}

pub fn store() -> frank_pack::PackStore {
    frank_pack::PackStore::new(service().paths().data_root.clone())
}

/// Return the selected pack, or the embedded pack when no user pack is active.
/// A corrupt selected pack remains an error: falling back would silently change
/// a user's explicit persona selection.
pub fn current() -> Result<frank_pack::CompiledPack, frank_app::AppError> {
    service().current_pack()
}

/// Stats can explain an old session if a selected pack was removed; hook and
/// command paths use `current()` and fail closed instead.
pub fn current_or_builtin() -> frank_pack::CompiledPack {
    current().unwrap_or_else(|_| builtin())
}

pub fn valid_flag_values(pack: &frank_pack::CompiledPack) -> Vec<String> {
    let mut values = pack.levels.keys().cloned().collect::<Vec<_>>();
    // One-shot modes are written to the same active flag by the state
    // machine.  Hooks must accept them as valid while the one-shot is in
    // flight; otherwise a statusline/session-start read would silently
    // collapse the state to `off` before the restore turn can run.
    values.extend(pack.oneshots.keys().cloned());
    values.push("off".to_string());
    values
}

pub fn level_by_id<'a>(
    pack: &'a frank_pack::CompiledPack,
    id: &str,
) -> Option<&'a frank_pack::CompiledLevel> {
    pack.levels.get(id)
}

const BUILTIN_PACK_ID: &str = "caveman";

pub fn build(path: &Path) -> anyhow::Result<frank_pack::CompiledPack> {
    let source = frank_pack::PackSource::load(path)?;
    Ok(frank_pack::compile(&source)?)
}

pub fn add(source: &str, expected_sha256: Option<&str>, yes: bool) -> i32 {
    if is_remote_source(source) {
        eprintln!(
            "frank pack add: HOLD(M7) remote pack sources ({source}) are not implemented; use a local directory containing pack.toml"
        );
        return 2;
    }

    let path = PathBuf::from(source);
    let preview = match build(&path) {
        Ok(pack) => pack,
        Err(e) => {
            eprintln!("frank pack add: {e}");
            return 1;
        }
    };
    if preview.id == BUILTIN_PACK_ID {
        eprintln!(
            "frank pack add: '{}' is reserved for the embedded built-in pack; choose another pack.id",
            BUILTIN_PACK_ID
        );
        return 1;
    }
    let Some(level) = preview.resolve_level(&preview.default_level) else {
        eprintln!("frank pack add: compiled pack has no default level");
        return 1;
    };
    println!("Pack {} v{}", preview.id, preview.version);
    println!(
        "Default activation prompt ({} bytes):",
        level.activation_prompt.len()
    );
    println!("---");
    println!("{}", level.activation_prompt);
    println!("---");

    if !yes {
        eprint!("Install this pack? Type 'yes' to continue: ");
        let mut answer = String::new();
        if std::io::stdin().lock().read_line(&mut answer).is_err() || answer.trim() != "yes" {
            eprintln!("frank pack add: not installed");
            return 1;
        }
    }

    match service().add_local_pack(&path, expected_sha256) {
        Ok(pack) => {
            println!("frank: installed {} v{}", pack.id, pack.version);
            println!("Use `frank pack use {}` to activate it.", pack.id);
            0
        }
        Err(e) => {
            eprintln!("frank pack add: {e}");
            1
        }
    }
}

pub fn list() -> i32 {
    match service().list_packs() {
        Ok(packs) => {
            for pack in packs {
                let state = if pack.active {
                    " [active]"
                } else if pack.builtin {
                    " [built-in]"
                } else {
                    ""
                };
                println!("{} {}{state}", pack.id, pack.version);
            }
            0
        }
        Err(e) => {
            eprintln!("frank pack list: {e}");
            1
        }
    }
}

pub fn use_pack(selector: &str) -> i32 {
    let builtin = builtin();
    if selector == BUILTIN_PACK_ID || selector == format!("{}@{}", BUILTIN_PACK_ID, builtin.version)
    {
        return match service().use_pack(selector) {
            Ok(()) => {
                println!("frank: using built-in pack {}", BUILTIN_PACK_ID);
                0
            }
            Err(e) => {
                eprintln!("frank pack use: {e}");
                1
            }
        };
    }

    match service().use_pack(selector) {
        Ok(()) => {
            println!("frank: using pack {selector}");
            0
        }
        Err(e) => {
            eprintln!("frank pack use: {e}");
            1
        }
    }
}

pub fn remove(selector: &str) -> i32 {
    if selector == BUILTIN_PACK_ID || selector.starts_with("caveman@") {
        eprintln!("frank pack remove: the built-in caveman pack cannot be removed");
        return 1;
    }
    match service().remove_pack(selector) {
        Ok(()) => {
            println!("frank: removed {selector}");
            0
        }
        Err(e) => {
            eprintln!("frank pack remove: {e}");
            1
        }
    }
}

pub fn show(selector: Option<&str>) -> i32 {
    let store = store();
    let pack = match selector {
        None | Some("caveman") => builtin(),
        Some(selector) => match store
            .find(selector)
            .and_then(|p| store.compile_installed(&p))
        {
            Ok(pack) => pack,
            Err(e) => {
                eprintln!("frank pack show: {e}");
                return 1;
            }
        },
    };
    println!("{} v{}", pack.id, pack.version);
    println!("default: {}", pack.default_level);
    for level in pack.levels.values() {
        println!(
            "  {} ({} activation bytes)",
            level.id,
            level.activation_prompt.len()
        );
    }
    0
}

fn is_remote_source(source: &str) -> bool {
    source.starts_with("github:") || source.starts_with("http://") || source.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_flag_values_include_levels_oneshots_and_off() {
        let pack = builtin();
        let values = valid_flag_values(&pack);
        for level in pack.levels.keys() {
            assert!(values.contains(level));
        }
        for oneshot in pack.oneshots.keys() {
            assert!(values.contains(oneshot));
        }
        assert!(values.iter().any(|value| value == "off"));
    }

    #[test]
    fn remote_sources_are_rejected_before_filesystem_access() {
        for source in [
            "github:org/pack",
            "http://example.test/pack",
            "https://example.test/pack",
        ] {
            assert!(is_remote_source(source));
        }
        assert!(!is_remote_source("./local-pack"));
    }
}
