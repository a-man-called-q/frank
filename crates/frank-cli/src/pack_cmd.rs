//! `frank pack` — presentation layer only. Every decision (built-in
//! selector matching, digest checks, install/use/remove validation) is
//! made by `frank_app::FrankService`; this module formats output and runs
//! the interactive install confirmation prompt.
//!
//! The built-in caveman pack remains embedded for a fast, dependency-free
//! default. M7 adds a data-only local pack store: selected packs are loaded
//! from the lockfile, digest-checked, and compiled at runtime. Hooks fail
//! closed (no prompt injection) if a selected pack is missing or modified.

use std::io::BufRead;
use std::path::{Path, PathBuf};

use frank_app::FrankService;

use crate::error::CliError;

/// Compile a source pack without installing it, for `frank pack build` and
/// as the preview step of `frank pack add`. Unlike `FrankService`'s
/// pack-admission checks, this deliberately does not reject the reserved
/// built-in id — `frank pack build packs/caveman` must succeed.
pub fn build(path: &Path) -> anyhow::Result<frank_pack::CompiledPack> {
    let source = frank_pack::PackSource::load(path)?;
    Ok(frank_pack::compile(&source)?)
}

pub fn add(
    svc: &FrankService,
    source: &str,
    expected_sha256: Option<&str>,
    yes: bool,
) -> Result<(), CliError> {
    if is_remote_source(source) {
        return Err(CliError::new(
            "pack add",
            format!(
                "HOLD(M7) remote pack sources ({source}) are not implemented; use a local directory containing pack.toml"
            ),
            2,
        ));
    }

    let path = PathBuf::from(source);
    let preview = build(&path).map_err(|e| CliError::scoped("pack add", e))?;
    let builtin_id = frank_app::builtin_pack().id;
    if preview.id == builtin_id {
        return Err(CliError::scoped(
            "pack add",
            format!(
                "'{builtin_id}' is reserved for the embedded built-in pack; choose another pack.id"
            ),
        ));
    }
    let Some(level) = preview.resolve_level(&preview.default_level) else {
        return Err(CliError::scoped(
            "pack add",
            "compiled pack has no default level",
        ));
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
            return Err(CliError::scoped("pack add", "not installed"));
        }
    }

    let pack = svc
        .add_local_pack(&path, expected_sha256)
        .map_err(|e| CliError::scoped("pack add", e))?;
    println!("frank: installed {} v{}", pack.id, pack.version);
    println!("Use `frank pack use {}` to activate it.", pack.id);
    Ok(())
}

pub fn list(svc: &FrankService) -> Result<(), CliError> {
    let packs = svc
        .list_packs()
        .map_err(|e| CliError::scoped("pack list", e))?;
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
    Ok(())
}

pub fn use_pack(svc: &FrankService, selector: &str) -> Result<(), CliError> {
    let builtin = frank_app::builtin_pack();
    let is_builtin =
        selector == builtin.id || selector == format!("{}@{}", builtin.id, builtin.version);
    svc.use_pack(selector)
        .map_err(|e| CliError::scoped("pack use", e))?;
    if is_builtin {
        println!("frank: using built-in pack {}", builtin.id);
    } else {
        println!("frank: using pack {selector}");
    }
    Ok(())
}

pub fn remove(svc: &FrankService, selector: &str) -> Result<(), CliError> {
    svc.remove_pack(selector)
        .map_err(|e| CliError::scoped("pack remove", e))?;
    println!("frank: removed {selector}");
    Ok(())
}

pub fn show(svc: &FrankService, selector: Option<&str>) -> Result<(), CliError> {
    let pack = svc
        .pack_for_selector(selector)
        .map_err(|e| CliError::scoped("pack show", e))?;
    println!("{} v{}", pack.id, pack.version);
    println!("default: {}", pack.default_level);
    for level in pack.levels.values() {
        println!(
            "  {} ({} activation bytes)",
            level.id,
            level.activation_prompt.len()
        );
    }
    Ok(())
}

fn is_remote_source(source: &str) -> bool {
    source.starts_with("github:") || source.starts_with("http://") || source.starts_with("https://")
}

#[cfg(test)]
mod tests {
    use super::*;
    use frank_app::FrankPaths;
    use tempfile::tempdir;

    fn service(root: &Path) -> FrankService {
        FrankService::new(FrankPaths {
            config_dir: root.join("claude"),
            data_root: root.join("data"),
            user_config_dir: root.join("config"),
            cwd: root.to_path_buf(),
            frank_bin: root.join("bin/frank"),
        })
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

    // Exercises the presentation layer directly against a tempdir-backed
    // `FrankService`, the seam `main()`'s injection unlocked.

    #[test]
    fn list_and_show_succeed_against_the_builtin_pack_with_no_local_packs_installed() {
        let tmp = tempdir().unwrap();
        let svc = service(tmp.path());
        assert!(list(&svc).is_ok());
        assert!(show(&svc, None).is_ok());
        assert!(show(&svc, Some("caveman")).is_ok());
    }

    #[test]
    fn use_pack_selects_the_builtin_pack_and_rejects_an_unknown_selector() {
        let tmp = tempdir().unwrap();
        let svc = service(tmp.path());
        assert!(use_pack(&svc, "caveman").is_ok());
        let err = use_pack(&svc, "does-not-exist").unwrap_err();
        assert_eq!(err.code, 1);
    }

    #[test]
    fn remove_refuses_the_builtin_pack_at_any_version() {
        let tmp = tempdir().unwrap();
        let svc = service(tmp.path());
        let err = remove(&svc, "caveman").unwrap_err();
        assert!(format!("{err}").contains("cannot be removed"));
        let err = remove(&svc, "caveman@9.9.9").unwrap_err();
        assert!(format!("{err}").contains("cannot be removed"));
    }

    #[test]
    fn add_installs_a_local_pack_with_confirmation_skipped() {
        let tmp = tempdir().unwrap();
        let svc = service(tmp.path());
        let source = tmp.path().join("pack");
        std::fs::create_dir_all(source.join("levels")).unwrap();
        std::fs::write(
            source.join("pack.toml"),
            r#"schema = 1
[pack]
id = "local"
version = "1.0.0"
default_level = "full"
[pack.budget]
max_activation_bytes = 1000
max_reinforce_bytes = 1000
[[level]]
id = "full"
compose = ["@rules"]
rules = "levels/full.md"
"#,
        )
        .unwrap();
        std::fs::write(source.join("levels/full.md"), "Be concise.").unwrap();

        assert!(add(&svc, source.to_str().unwrap(), None, true).is_ok());
        assert!(use_pack(&svc, "local").is_ok());
    }

    #[test]
    fn add_reports_a_scoped_error_for_a_missing_source_path() {
        let tmp = tempdir().unwrap();
        let svc = service(tmp.path());
        let missing = tmp.path().join("does-not-exist");
        let err = add(&svc, missing.to_str().unwrap(), None, true).unwrap_err();
        assert!(format!("{err}").starts_with("frank pack add:"), "{err}");
    }

    #[test]
    fn use_pack_then_list_shows_the_builtin_as_built_in_and_the_new_pack_as_active() {
        let tmp = tempdir().unwrap();
        let svc = service(tmp.path());
        let source = tmp.path().join("pack");
        std::fs::create_dir_all(source.join("levels")).unwrap();
        std::fs::write(
            source.join("pack.toml"),
            r#"schema = 1
[pack]
id = "local"
version = "1.0.0"
default_level = "full"
[pack.budget]
max_activation_bytes = 1000
max_reinforce_bytes = 1000
[[level]]
id = "full"
compose = ["@rules"]
rules = "levels/full.md"
"#,
        )
        .unwrap();
        std::fs::write(source.join("levels/full.md"), "Be concise.").unwrap();
        assert!(add(&svc, source.to_str().unwrap(), None, true).is_ok());
        assert!(use_pack(&svc, "local").is_ok());

        assert!(list(&svc).is_ok());
    }

    #[test]
    fn remove_deletes_a_non_builtin_installed_pack() {
        let tmp = tempdir().unwrap();
        let svc = service(tmp.path());
        let source = tmp.path().join("pack");
        std::fs::create_dir_all(source.join("levels")).unwrap();
        std::fs::write(
            source.join("pack.toml"),
            r#"schema = 1
[pack]
id = "local"
version = "1.0.0"
default_level = "full"
[pack.budget]
max_activation_bytes = 1000
max_reinforce_bytes = 1000
[[level]]
id = "full"
compose = ["@rules"]
rules = "levels/full.md"
"#,
        )
        .unwrap();
        std::fs::write(source.join("levels/full.md"), "Be concise.").unwrap();
        assert!(add(&svc, source.to_str().unwrap(), None, true).is_ok());

        assert!(remove(&svc, "local").is_ok());
        let err = use_pack(&svc, "local").unwrap_err();
        assert_eq!(err.code, 1);
    }
}
