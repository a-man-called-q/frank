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
    let raw = frank_safeio::read_text_capped(path, frank_safeio::MAX_CONFIG_BYTES).ok()?;
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
    resolve_default_level_with_user_dir(pack, cwd, env_var, user_config_dir().as_deref())
}

/// Resolve the same precedence chain while allowing an application service to
/// supply its already-resolved user config root. This keeps tests and frontends
/// that use an explicit `FrankPaths` value on exactly the same path contract as
/// the process-level CLI/hooks.
pub fn resolve_default_level_with_user_dir(
    pack: &CompiledPack,
    cwd: &Path,
    env_var: &str,
    user_dir: Option<&Path>,
) -> String {
    if let Some(v) = env_default(pack, env_var) {
        return v;
    }
    if let Some(repo_config) = find_repo_config_path(cwd) {
        if let Some(v) = read_mode_from_file(pack, &repo_config) {
            return v;
        }
    }
    if let Some(dir) = user_dir {
        if let Some(v) = read_mode_from_file(pack, &dir.join("config.toml")) {
            return v;
        }
    }
    pack.default_level.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use frank_pack::{CompiledActivation, CompiledLevel, CompiledOneshot};
    use std::collections::BTreeMap;
    use tempfile::tempdir;

    fn fixture_pack() -> CompiledPack {
        let mut levels = BTreeMap::new();
        levels.insert(
            "full".to_string(),
            CompiledLevel {
                id: "full".to_string(),
                title: None,
                aliases: vec!["classic".to_string()],
                activation_prompt: "full prompt".to_string(),
                reinforce: "full reinforcement".to_string(),
            },
        );
        levels.insert(
            "ultra".to_string(),
            CompiledLevel {
                id: "ultra".to_string(),
                title: None,
                aliases: Vec::new(),
                activation_prompt: "ultra prompt".to_string(),
                reinforce: "ultra reinforcement".to_string(),
            },
        );

        let mut aliases = BTreeMap::new();
        aliases.insert("classic".to_string(), "full".to_string());

        CompiledPack {
            id: "fixture".to_string(),
            version: "0.0.0".to_string(),
            default_level: "full".to_string(),
            levels,
            aliases,
            oneshots: BTreeMap::<String, CompiledOneshot>::new(),
            activation: CompiledActivation::default(),
            benchmark: BTreeMap::new(),
        }
    }

    #[test]
    fn malformed_or_unsafe_config_files_are_ignored() {
        let tmp = tempdir().unwrap();
        let pack = fixture_pack();

        let malformed = tmp.path().join("malformed.toml");
        std::fs::write(&malformed, "default_level = [").unwrap();
        assert_eq!(read_mode_from_file(&pack, &malformed), None);

        let directory = tmp.path().join("directory.toml");
        std::fs::create_dir(&directory).unwrap();
        assert_eq!(read_mode_from_file(&pack, &directory), None);

        let missing_default = tmp.path().join("missing-default.toml");
        std::fs::write(&missing_default, "other = true\n").unwrap();
        assert_eq!(read_mode_from_file(&pack, &missing_default), None);

        let oversized = tmp.path().join("oversized.toml");
        std::fs::write(
            &oversized,
            format!(
                "default_level = \"{}\"\n",
                "x".repeat(frank_safeio::MAX_CONFIG_BYTES)
            ),
        )
        .unwrap();
        assert_eq!(read_mode_from_file(&pack, &oversized), None);

        #[cfg(unix)]
        {
            let target = tmp.path().join("target.toml");
            std::fs::write(&target, "default_level = \"ultra\"\n").unwrap();
            let link = tmp.path().join("link.toml");
            std::os::unix::fs::symlink(&target, &link).unwrap();
            assert_eq!(read_mode_from_file(&pack, &link), None);
        }
    }

    #[test]
    fn valid_alias_and_off_values_are_accepted() {
        let tmp = tempdir().unwrap();
        let pack = fixture_pack();

        let alias = tmp.path().join("alias.toml");
        std::fs::write(&alias, "default_level = \"CLASSIC\"\n").unwrap();
        assert_eq!(
            read_mode_from_file(&pack, &alias).as_deref(),
            Some("classic")
        );

        let off = tmp.path().join("off.toml");
        std::fs::write(&off, "default_level = \" off \"\n").unwrap();
        assert_eq!(read_mode_from_file(&pack, &off).as_deref(), Some("off"));
    }

    #[test]
    fn repo_search_finds_nested_config_and_refuses_symlink_candidates() {
        let tmp = tempdir().unwrap();
        let nested = tmp.path().join("one/two");
        std::fs::create_dir_all(&nested).unwrap();
        assert_eq!(find_repo_config_path(&nested), None);

        let repo_dir = tmp.path().join(".frank");
        std::fs::create_dir(&repo_dir).unwrap();
        let config = repo_dir.join("config.toml");
        std::fs::write(&config, "default_level = \"ultra\"\n").unwrap();
        assert_eq!(find_repo_config_path(&nested), Some(config.clone()));

        std::fs::remove_file(repo_dir.join("config.toml")).unwrap();
        let target = tmp.path().join("secret.toml");
        std::fs::write(&target, "default_level = \"ultra\"\n").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &config).unwrap();
        assert_eq!(find_repo_config_path(&nested), None);
    }

    #[test]
    fn repo_search_ignores_directory_candidates() {
        let tmp = tempdir().unwrap();
        let nested = tmp.path().join("one/two");
        std::fs::create_dir_all(nested.join(".frank/config.toml")).unwrap();
        assert_eq!(find_repo_config_path(&nested), None);
    }

    #[test]
    fn repo_search_stops_at_the_walk_limit() {
        let tmp = tempdir().unwrap();
        let mut nested = tmp.path().to_path_buf();
        for segment in 0..=MAX_WALK_LEVELS {
            nested.push(format!("level-{segment}"));
        }
        std::fs::create_dir_all(&nested).unwrap();

        assert_eq!(find_repo_config_path(&nested), None);
    }

    #[test]
    fn xdg_config_home_is_used_when_present() {
        let tmp = tempdir().unwrap();
        let previous = std::env::var_os("XDG_CONFIG_HOME");
        unsafe { std::env::set_var("XDG_CONFIG_HOME", tmp.path()) };
        let actual = user_config_dir();
        restore_xdg_config_home(previous.clone());
        restore_xdg_config_home(Some(tmp.path().join("branch").into_os_string()));
        restore_xdg_config_home(None);
        restore_xdg_config_home(previous);

        assert_eq!(actual, Some(tmp.path().join("frank")));
    }

    fn restore_xdg_config_home(value: Option<std::ffi::OsString>) {
        match value {
            Some(value) => unsafe { std::env::set_var("XDG_CONFIG_HOME", value) },
            None => unsafe { std::env::remove_var("XDG_CONFIG_HOME") },
        }
    }

    #[test]
    fn invalid_repo_config_falls_through_to_user_config() {
        let tmp = tempdir().unwrap();
        let user = tmp.path().join("user");
        std::fs::create_dir_all(&user).unwrap();
        std::fs::write(
            tmp.path().join(".frank.toml"),
            "default_level = \"bogus\"\n",
        )
        .unwrap();
        std::fs::write(user.join("config.toml"), "default_level = \"ultra\"\n").unwrap();

        let resolved = resolve_default_level_with_user_dir(
            &fixture_pack(),
            tmp.path(),
            "FRANK_TEST_DEFAULT_LEVEL_UNSET_CONFIG",
            Some(&user),
        );
        assert_eq!(resolved, "ultra");

        std::fs::write(user.join("config.toml"), "default_level = \"bogus\"\n").unwrap();
        let fallback = resolve_default_level_with_user_dir(
            &fixture_pack(),
            tmp.path(),
            "FRANK_TEST_DEFAULT_LEVEL_UNSET_CONFIG",
            Some(&user),
        );
        assert_eq!(fallback, "full");
    }

    #[test]
    fn missing_user_config_dir_falls_back_to_pack_default() {
        let tmp = tempdir().unwrap();
        assert_eq!(
            resolve_default_level_with_user_dir(
                &fixture_pack(),
                tmp.path(),
                "FRANK_TEST_DEFAULT_LEVEL_UNSET_NO_USER_DIR",
                None,
            ),
            "full"
        );
    }
}
