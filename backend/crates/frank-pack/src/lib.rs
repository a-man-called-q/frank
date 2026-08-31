//! Pack manifest schema, fragment composition, and prompt compiler.
//!
//! A "pack" is a persona: caveman is the built-in one, but the schema here
//! is what makes Frank an engine rather than a caveman-shaped tool. See
//! `manifest.rs` for the `pack.toml` schema and `compiler.rs` for how a
//! [`compiler::PackSource`] on disk becomes a [`compiler::CompiledPack`].

mod compiler;
mod error;
mod manifest;
mod store;

pub use compiler::{
    CompiledActivation, CompiledLevel, CompiledOneshot, CompiledPack, PackSource, compile,
};
pub use error::{PackError, Result};
pub use manifest::{
    ActivationDef, BenchmarkDef, Budget, Example, Fragment, LevelDef, OneshotDef, PackManifest,
    PackMeta, ReductionStat,
};
pub use store::{
    InstalledPack, PackLock, PackRef, PackStore, PackStoreError, StoreResult, directory_sha256,
};

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use std::fs;
    use tempfile::tempdir;

    fn write(dir: &std::path::Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn minimal_pack(dir: &std::path::Path) {
        write(
            dir,
            "pack.toml",
            r#"
schema = 1

[pack]
id = "test"
version = "0.1.0"
default_level = "full"

[pack.budget]
max_activation_bytes = 900
max_reinforce_bytes = 220

[fragments]
core = { file = "shared/core.md" }

[[level]]
id = "full"
aliases = ["classic"]
compose = ["core", "@rules", "@examples"]
rules = "levels/full.md"
examples = [{ q = "Why slow?", a = "N+1 query. Add index." }]
reinforce = "STAY TERSE."

[[level]]
id = "ultra"
inherits = "full"
rules = "levels/ultra.md"

[activation]
on = ['\btalk like\b[^.]{0,40}\bcaveman\b']
off = ['\bstop\s+caveman\b']
question_guard = '^(what|how)\b'
command_prefix = "caveman"
"#,
        );
        write(dir, "shared/core.md", "Respond terse. Keep substance.");
        write(dir, "levels/full.md", "Drop articles and filler.");
        write(dir, "levels/ultra.md", "Drop everything not load-bearing.");
    }

    #[test]
    fn compiles_minimal_pack() {
        let tmp = tempdir().unwrap();
        minimal_pack(tmp.path());

        let source = PackSource::load(tmp.path()).unwrap();
        let pack = compile(&source).unwrap();

        assert_eq!(pack.id, "test");
        assert_eq!(pack.default_level, "full");
        assert!(pack.levels.contains_key("full"));
        assert!(pack.levels.contains_key("ultra"));

        let full = pack.resolve_level("full").unwrap();
        assert!(full.activation_prompt.contains("Respond terse"));
        assert!(full.activation_prompt.contains("Drop articles"));
        assert!(full.activation_prompt.contains("Ex: Why slow?"));
        assert_eq!(full.reinforce, "STAY TERSE.");
    }

    #[test]
    fn valid_flag_values_include_levels_oneshots_and_off() {
        let tmp = tempdir().unwrap();
        minimal_pack(tmp.path());
        let pack = compile(&PackSource::load(tmp.path()).unwrap()).unwrap();

        let values = pack.valid_flag_values();
        for level in pack.levels.keys() {
            assert!(values.contains(&level.as_str()));
        }
        for oneshot in pack.oneshots.keys() {
            assert!(values.contains(&oneshot.as_str()));
        }
        assert!(values.contains(&"off"));
    }

    #[test]
    fn alias_resolves_to_canonical_level() {
        let tmp = tempdir().unwrap();
        minimal_pack(tmp.path());
        let pack = compile(&PackSource::load(tmp.path()).unwrap()).unwrap();

        let via_alias = pack.resolve_level("classic").unwrap();
        let via_id = pack.resolve_level("full").unwrap();
        assert_eq!(via_alias.activation_prompt, via_id.activation_prompt);
    }

    #[test]
    fn inherited_level_reuses_parent_compose_and_reinforce() {
        let tmp = tempdir().unwrap();
        minimal_pack(tmp.path());
        let pack = compile(&PackSource::load(tmp.path()).unwrap()).unwrap();

        let ultra = pack.resolve_level("ultra").unwrap();
        // compose = ["core", "@rules"] inherited from "full", but @rules
        // resolves to ultra's own rules file.
        assert!(ultra.activation_prompt.contains("Respond terse"));
        assert!(ultra.activation_prompt.contains("Drop everything"));
        assert!(!ultra.activation_prompt.contains("Drop articles"));
        // reinforce inherited verbatim since ultra doesn't override it.
        assert_eq!(ultra.reinforce, "STAY TERSE.");
    }

    #[test]
    fn budget_violation_is_a_hard_error() {
        let tmp = tempdir().unwrap();
        minimal_pack(tmp.path());
        // Blow the 900-byte activation budget.
        write(tmp.path(), "levels/full.md", &"x".repeat(2000));

        let source = PackSource::load(tmp.path()).unwrap();
        let err = compile(&source).unwrap_err();
        assert!(matches!(err, PackError::BudgetExceeded { .. }), "{err:?}");
    }

    #[test]
    fn budget_boundary_is_inclusive() {
        let tmp = tempdir().unwrap();
        write(
            tmp.path(),
            "pack.toml",
            r#"
schema = 1
[pack]
id = "boundary"
version = "0.1.0"
default_level = "full"
[pack.budget]
max_activation_bytes = 1000
max_reinforce_bytes = 1000
[fragments]
core = { file = "core.md" }
[[level]]
id = "full"
compose = ["core"]
reinforce = "keep"
"#,
        );
        write(tmp.path(), "core.md", "exact boundary");

        let source = PackSource::load(tmp.path()).unwrap();
        let prompt_len = compile(&source)
            .unwrap()
            .resolve_level("full")
            .unwrap()
            .activation_prompt
            .len();
        let reinforce_len = compile(&source)
            .unwrap()
            .resolve_level("full")
            .unwrap()
            .reinforce
            .len();
        let manifest = std::fs::read_to_string(tmp.path().join("pack.toml")).unwrap();
        write(
            tmp.path(),
            "pack.toml",
            &manifest
                .replace(
                    "max_activation_bytes = 1000",
                    &format!("max_activation_bytes = {prompt_len}"),
                )
                .replace(
                    "max_reinforce_bytes = 1000",
                    &format!("max_reinforce_bytes = {reinforce_len}"),
                ),
        );

        assert!(compile(&PackSource::load(tmp.path()).unwrap()).is_ok());
    }

    #[test]
    fn unknown_fragment_token_is_an_error() {
        let tmp = tempdir().unwrap();
        minimal_pack(tmp.path());
        write(
            tmp.path(),
            "pack.toml",
            &fs::read_to_string(tmp.path().join("pack.toml"))
                .unwrap()
                .replace(
                    r#"compose = ["core", "@rules", "@examples"]"#,
                    r#"compose = ["nope"]"#,
                ),
        );

        let source = PackSource::load(tmp.path()).unwrap();
        let err = compile(&source).unwrap_err();
        assert!(matches!(err, PackError::UnknownFragment(_, _)), "{err:?}");
    }

    #[test]
    fn inheritance_cycle_is_detected() {
        let tmp = tempdir().unwrap();
        minimal_pack(tmp.path());
        write(
            tmp.path(),
            "pack.toml",
            &fs::read_to_string(tmp.path().join("pack.toml"))
                .unwrap()
                .replace(
                    r#"id = "full""#,
                    r#"id = "full"
inherits = "ultra""#,
                ),
        );

        let source = PackSource::load(tmp.path()).unwrap();
        let err = compile(&source).unwrap_err();
        assert!(matches!(err, PackError::InheritanceCycle(_)), "{err:?}");
    }

    #[test]
    fn invalid_activation_regex_is_rejected() {
        let tmp = tempdir().unwrap();
        minimal_pack(tmp.path());
        write(
            tmp.path(),
            "pack.toml",
            &fs::read_to_string(tmp.path().join("pack.toml"))
                .unwrap()
                .replace(r"'\bstop\s+caveman\b'", r"'\bstop\s+caveman\b('"),
        );

        let source = PackSource::load(tmp.path()).unwrap();
        let err = compile(&source).unwrap_err();
        assert!(matches!(err, PackError::InvalidRegex(_, _)), "{err:?}");
    }

    #[test]
    fn html_comments_and_headings_are_stripped() {
        let tmp = tempdir().unwrap();
        minimal_pack(tmp.path());
        write(
            tmp.path(),
            "shared/core.md",
            "## Persistence\n\n<!-- authoring note, not for the model -->\nRespond terse.\n",
        );

        let pack = compile(&PackSource::load(tmp.path()).unwrap()).unwrap();
        let full = pack.resolve_level("full").unwrap();
        assert!(!full.activation_prompt.contains('#'));
        assert!(!full.activation_prompt.contains("authoring note"));
        assert!(full.activation_prompt.contains("Persistence"));
    }

    #[test]
    fn duplicate_alias_is_rejected() {
        let tmp = tempdir().unwrap();
        minimal_pack(tmp.path());
        write(
            tmp.path(),
            "pack.toml",
            &fs::read_to_string(tmp.path().join("pack.toml"))
                .unwrap()
                .replace(
                    r#"id = "ultra"
inherits = "full""#,
                    r#"id = "ultra"
aliases = ["classic"]
inherits = "full""#,
                ),
        );

        let source = PackSource::load(tmp.path()).unwrap();
        let err = compile(&source).unwrap_err();
        assert!(matches!(err, PackError::DuplicateAlias(_, _)), "{err:?}");
    }

    #[test]
    fn path_traversal_in_a_pack_reference_is_rejected() {
        let tmp = tempdir().unwrap();
        minimal_pack(tmp.path());
        write(
            tmp.path(),
            "pack.toml",
            &fs::read_to_string(tmp.path().join("pack.toml"))
                .unwrap()
                .replace("rules = \"levels/full.md\"", "rules = \"../outside.md\""),
        );

        let err = compile(&PackSource::load(tmp.path()).unwrap()).unwrap_err();
        assert!(matches!(err, PackError::UnsafePath(_)), "{err:?}");
    }

    #[cfg(unix)]
    #[test]
    fn loading_a_symlinked_pack_root_is_rejected() {
        use std::os::unix::fs::symlink;

        let tmp = tempdir().unwrap();
        let real = tmp.path().join("real");
        minimal_pack(&real);
        let link = tmp.path().join("link");
        symlink(&real, &link).unwrap();

        assert!(matches!(
            PackSource::load(&link),
            Err(PackError::UnsafePath(_))
        ));
    }

    #[test]
    fn loading_a_regular_file_as_a_pack_root_is_rejected() {
        let tmp = tempdir().unwrap();
        let file = tmp.path().join("not-a-pack");
        fs::write(&file, "pack.toml").unwrap();
        assert!(matches!(
            PackSource::load(&file),
            Err(PackError::UnsafePath(_))
        ));
    }

    proptest! {
        #[test]
        fn malformed_manifest_text_never_panics(raw in any::<String>()) {
            let _: std::result::Result<PackManifest, _> = toml::from_str(&raw);
        }

        #[test]
        fn generated_aliases_resolve_to_the_same_level(alias in "[a-z][a-z0-9_-]{0,10}") {
            let tmp = tempdir().unwrap();
            minimal_pack(tmp.path());
            let manifest_path = tmp.path().join("pack.toml");
            let raw = fs::read_to_string(&manifest_path).unwrap().replace(
                "aliases = [\"classic\"]",
                &format!("aliases = [\"{alias}\"]"),
            );
            fs::write(manifest_path, raw).unwrap();
            let pack = compile(&PackSource::load(tmp.path()).unwrap()).unwrap();
            prop_assert_eq!(&pack.resolve_level(&alias).unwrap().id, "full");
        }
    }
}
