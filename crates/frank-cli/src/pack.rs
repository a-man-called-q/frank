//! Built-in and user-installed pack selection.
//!
//! The built-in caveman pack remains embedded for a fast, dependency-free
//! default. M7 adds a data-only local pack store: selected packs are loaded
//! from the lockfile, digest-checked, and compiled at runtime. Hooks fail
//! closed (no prompt injection) if a selected pack is missing or modified.

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

pub mod caveman {
    #![allow(dead_code)]
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packs/caveman/compiled.rs"
    ));
}

/// Compatibility alias for the generated built-in pack. Runtime paths use
/// [`current`] so `frank pack use` can select a third-party pack.
pub use caveman as active;

static BUILTIN: OnceLock<frank_pack::CompiledPack> = OnceLock::new();

pub fn builtin() -> frank_pack::CompiledPack {
    compiled()
}

pub fn data_root() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| frank_safeio::home_dir().map(|h| h.join(".local").join("share")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("frank")
}

pub fn store() -> frank_pack::PackStore {
    frank_pack::PackStore::new(data_root())
}

/// Return the selected pack, or the embedded caveman pack when no user pack
/// is active. A corrupt selected pack is an error: falling back would make a
/// user's explicit selection silently change persona.
pub fn current() -> frank_pack::StoreResult<frank_pack::CompiledPack> {
    match store().active()? {
        Some((_, pack)) => Ok(pack),
        None => Ok(builtin()),
    }
}

/// Stats can still explain an old session if a selected pack was removed;
/// command and hook paths use `current()` and fail closed instead.
pub fn current_or_builtin() -> frank_pack::CompiledPack {
    current().unwrap_or_else(|_| builtin())
}

pub fn valid_flag_values(pack: &frank_pack::CompiledPack) -> Vec<String> {
    let mut values = pack.levels.keys().cloned().collect::<Vec<_>>();
    values.push("off".to_string());
    values
}

pub fn resolve_level(pack: &frank_pack::CompiledPack, name: &str) -> Option<String> {
    pack.resolve_level(name).map(|level| level.id.clone())
}

pub fn level_by_id<'a>(
    pack: &'a frank_pack::CompiledPack,
    id: &str,
) -> Option<&'a frank_pack::CompiledLevel> {
    pack.levels.get(id)
}

/// Build a `CompiledPack` from the flat, zero-cost embedded statics emitted
/// by `xtask build-packs`.
pub fn compiled() -> frank_pack::CompiledPack {
    BUILTIN.get_or_init(build_compiled).clone()
}

fn build_compiled() -> frank_pack::CompiledPack {
    use std::collections::BTreeMap;

    let levels = active::LEVELS
        .iter()
        .map(|l| {
            (
                l.id.to_string(),
                frank_pack::CompiledLevel {
                    id: l.id.to_string(),
                    title: l.title.map(str::to_string),
                    aliases: l.aliases.iter().map(|a| a.to_string()).collect(),
                    activation_prompt: l.activation_prompt.to_string(),
                    reinforce: l.reinforce.to_string(),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();

    let aliases = active::ALIASES
        .iter()
        .map(|a| (a.alias.to_string(), a.canonical.to_string()))
        .collect::<BTreeMap<_, _>>();

    let oneshots = active::ONESHOTS
        .iter()
        .map(|o| {
            (
                o.id.to_string(),
                frank_pack::CompiledOneshot {
                    id: o.id.to_string(),
                    prompt: o.prompt.to_string(),
                    restores_previous: o.restores_previous,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();

    let activation = frank_pack::CompiledActivation {
        on: active::ACTIVATION
            .on
            .iter()
            .map(|s| s.to_string())
            .collect(),
        off: active::ACTIVATION
            .off
            .iter()
            .map(|s| s.to_string())
            .collect(),
        question_guard: active::ACTIVATION.question_guard.map(str::to_string),
        command_prefix: active::ACTIVATION.command_prefix.map(str::to_string),
    };

    let benchmark = active::BENCHMARK
        .iter()
        .map(|r| {
            (
                r.level.to_string(),
                frank_pack::ReductionStat {
                    mean: r.mean,
                    p25: r.p25,
                    p75: r.p75,
                    n: r.n,
                    model: r.model.map(str::to_string),
                },
            )
        })
        .collect::<BTreeMap<_, _>>();

    frank_pack::CompiledPack {
        id: active::PACK_ID.to_string(),
        version: active::PACK_VERSION.to_string(),
        default_level: active::DEFAULT_LEVEL.to_string(),
        levels,
        aliases,
        oneshots,
        activation,
        benchmark,
    }
}

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
    if preview.id == active::PACK_ID {
        eprintln!(
            "frank pack add: '{}' is reserved for the embedded built-in pack; choose another pack.id",
            active::PACK_ID
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

    match store().add_local(&path, expected_sha256) {
        Ok(install) => {
            println!(
                "frank: installed {} (sha256 {})",
                install.installed.pack_ref().display_name(),
                install.installed.sha256
            );
            println!("Use `frank pack use {}` to activate it.", install.pack.id);
            0
        }
        Err(e) => {
            eprintln!("frank pack add: {e}");
            1
        }
    }
}

pub fn list() -> i32 {
    let store = store();
    let lock = match store.load_lock() {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("frank pack list: {e}");
            return 1;
        }
    };
    let builtin = builtin();
    let builtin_active = lock.active.is_none();
    println!(
        "{} {}{}",
        builtin.id,
        builtin.version,
        if builtin_active {
            " [active]"
        } else {
            " [built-in]"
        }
    );
    for installed in lock.packs {
        let active = lock.active.as_ref() == Some(&installed.pack_ref());
        println!(
            "{} {}{}",
            installed.id,
            installed.version,
            if active { " [active]" } else { "" }
        );
    }
    0
}

pub fn use_pack(selector: &str) -> i32 {
    let store = store();
    if selector == active::PACK_ID
        || selector == format!("{}@{}", active::PACK_ID, active::PACK_VERSION)
    {
        return match store.set_active(None) {
            Ok(()) => {
                println!("frank: using built-in pack {}", active::PACK_ID);
                0
            }
            Err(e) => {
                eprintln!("frank pack use: {e}");
                1
            }
        };
    }

    let installed = match store.find(selector) {
        Ok(pack) => pack,
        Err(e) => {
            eprintln!("frank pack use: {e}");
            return 1;
        }
    };
    if let Err(e) = store.compile_installed(&installed) {
        eprintln!("frank pack use: {e}");
        return 1;
    }
    match store.set_active(Some(installed.pack_ref())) {
        Ok(()) => {
            println!("frank: using pack {}", installed.pack_ref().display_name());
            0
        }
        Err(e) => {
            eprintln!("frank pack use: {e}");
            1
        }
    }
}

pub fn remove(selector: &str) -> i32 {
    if selector == active::PACK_ID || selector.starts_with("caveman@") {
        eprintln!("frank pack remove: the built-in caveman pack cannot be removed");
        return 1;
    }
    match store().remove(selector) {
        Ok(removed) => {
            println!("frank: removed {}", removed.pack_ref().display_name());
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
