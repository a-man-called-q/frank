#[cfg(test)]
mod tests {
    use crate::detect::detect;
    use crate::manifest::TargetManifest;
    use crate::plan::{Detection, ProbeEnv};
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn env(home: Option<PathBuf>, path_dirs: Vec<PathBuf>) -> ProbeEnv {
        ProbeEnv {
            path_dirs,
            home,
            extra_dirs: vec![],
            is_macos: false,
        }
    }

    fn parse(toml: &str) -> TargetManifest {
        toml::from_str(toml).unwrap()
    }

    #[test]
    fn unknown_probe_key_is_a_parse_error_not_a_silent_false() {
        // This is the exact bug class the archive had: `detectMatch`'s
        // switch had no `default` arm, so a typo'd probe kind silently
        // evaluated false forever. Here it must fail to parse at all.
        let result: Result<TargetManifest, _> = toml::from_str(
            r#"
schema = 1
[target]
id = "x"
label = "X"
kind = "generic"
[[detect]]
commnad = "typo"
[install]
strategy = "spawn"
"#,
        );
        assert!(result.is_err(), "typo'd probe key must fail to parse");
    }

    #[test]
    fn command_probe_detects_via_path() {
        let tmp = tempdir().unwrap();
        std::fs::write(tmp.path().join("mytool"), "").unwrap();
        let m = parse(
            r#"
schema = 1
[target]
id = "x"
label = "X"
kind = "generic"
[[detect]]
command = "mytool"
[install]
strategy = "spawn"
"#,
        );
        let e = env(None, vec![tmp.path().to_path_buf()]);
        assert_eq!(detect(&m, &e), Detection::Detected);
        assert_eq!(detect(&m, &env(None, vec![])), Detection::NotDetected);
    }

    #[test]
    fn dir_probe_expands_home() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".myagent")).unwrap();
        let m = parse(
            r#"
schema = 1
[target]
id = "x"
label = "X"
kind = "generic"
[[detect]]
dir = "$HOME/.myagent"
[install]
strategy = "spawn"
"#,
        );
        assert_eq!(
            detect(&m, &env(Some(tmp.path().to_path_buf()), vec![])),
            Detection::Detected
        );
    }

    #[test]
    fn clauses_or_together() {
        let tmp = tempdir().unwrap();
        std::fs::write(tmp.path().join("realtool"), "").unwrap();
        let m = parse(
            r#"
schema = 1
[target]
id = "x"
label = "X"
kind = "generic"
[[detect]]
command = "doesnotexist"
[[detect]]
command = "realtool"
[install]
strategy = "spawn"
"#,
        );
        assert_eq!(
            detect(&m, &env(None, vec![tmp.path().to_path_buf()])),
            Detection::Detected
        );
    }

    #[test]
    fn keys_within_one_clause_and_together() {
        let tmp = tempdir().unwrap();
        std::fs::write(tmp.path().join("realtool"), "").unwrap();
        // dir doesn't exist -> AND fails even though command matches.
        let m = parse(
            r#"
schema = 1
[target]
id = "x"
label = "X"
kind = "generic"
[[detect]]
command = "realtool"
dir = "$HOME/.nonexistent-dir-xyz"
[install]
strategy = "spawn"
"#,
        );
        let e = env(
            Some(tmp.path().to_path_buf()),
            vec![tmp.path().to_path_buf()],
        );
        assert_eq!(detect(&m, &e), Detection::NotDetected);
    }

    #[test]
    fn soft_and_verified_flags_parse() {
        let m = parse(
            r#"
schema = 1
[target]
id = "antigravity"
label = "Antigravity"
kind = "generic"
soft = true
verified = false
[[detect]]
dir = "$HOME/.antigravity"
[install]
strategy = "spawn"
"#,
        );
        assert!(m.target.soft);
        assert!(!m.target.verified);
    }

    #[test]
    fn spawn_strategy_parses_steps_and_optional_uninstall() {
        let m = parse(
            r#"
schema = 1
[target]
id = "x"
label = "X"
kind = "generic"
[install]
strategy = "spawn"
[[install.step]]
program = "npx"
args = ["-y", "skills", "add", "repo", "-a", "x"]
success = "status_zero"
"#,
        );
        let crate::manifest::InstallSpec::Spawn { steps, uninstall } = &m.install else {
            panic!()
        };
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].program, "npx");
        assert!(uninstall.is_none());
    }

    #[test]
    fn markdown_block_strategy_parses() {
        let m = parse(
            r#"
schema = 1
[target]
id = "x"
label = "X"
kind = "generic"
[install]
strategy = "markdown-block"
[install.markdown]
path = "./AGENTS.md"
begin = "<!-- frank:begin -->"
end = "<!-- frank:end -->"
body = "pack:static_digest"
create_if_missing = true
"#,
        );
        assert!(matches!(
            m.install,
            crate::manifest::InstallSpec::MarkdownBlock { .. }
        ));
    }
}
