//! Evaluates a [`TargetManifest`]'s `[[detect]]` clauses against a
//! [`ProbeEnv`]. All 8 probe kinds the archive's DSL supported
//! (`archive/bin/install.js:279-334`), each a typed field instead of a
//! string tag — see `manifest.rs`'s module docs for why that matters.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use regex::Regex;

use crate::manifest::{DetectClause, TargetManifest};
use crate::plan::{Detection, ProbeEnv};

/// A target manifest is user/config input, so a command-version probe must
/// not be able to wedge the GUI's two-second refresh loop (or a CLI status
/// command) forever. The child is killed on timeout and a timeout is simply a
/// non-match, preserving detection's fail-closed behavior.
const COMMAND_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Recursive extension-directory scan, depth-capped the same way the
/// archive's `walkDir` was (`archive/bin/install.js:324-334`) — this
/// exists to defend against symlink cycles under a plugin directory, not
/// because 4 levels has any significance beyond "deeper than any real
/// extension ever nests".
fn walk_basenames(root: &Path, depth: u32, out: &mut Vec<String>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path).ok();
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            out.push(name.to_string());
        }
        if metadata.is_some_and(|metadata| metadata.is_dir()) {
            walk_basenames(&path, depth - 1, out);
        }
    }
}

fn vscode_ext_roots(env: &ProbeEnv) -> Vec<std::path::PathBuf> {
    [
        ".vscode/extensions",
        ".vscode-server/extensions",
        ".cursor/extensions",
        ".windsurf/extensions",
    ]
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

fn command_version_output(bin: &str, args: &[String]) -> Option<String> {
    let mut child = Command::new(bin)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + COMMAND_PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                let output = child.wait_with_output().ok()?;
                return Some(format!(
                    "{}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(_) => return None,
        }
    }
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
                Path::new("/Applications")
                    .join(format!("{app}.app"))
                    .is_dir()
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
        let dirs: Vec<_> = env
            .home
            .iter()
            .map(|h| h.join(".cursor/extensions"))
            .collect();
        let found = Regex::new(&format!("(?i){pattern}"))
            .is_ok_and(|re| regex_matches_any_basename(&re, &dirs));
        all_matched &= found;
    }
    if let Some(true) = clause.jetbrains_config {
        any_field = true;
        let found = env.home.as_ref().is_some_and(|h| {
            h.join("Library/Application Support/JetBrains").is_dir()
                || h.join(".config/JetBrains").is_dir()
        });
        all_matched &= found;
    }
    if let Some(pattern) = &clause.jetbrains_plugin {
        any_field = true;
        let mut names = Vec::new();
        if let Some(h) = &env.home {
            for root in [
                h.join("Library/Application Support/JetBrains"),
                h.join(".config/JetBrains"),
            ] {
                walk_basenames(&root, 4, &mut names);
            }
        }
        let found = Regex::new(&format!("(?i){pattern}"))
            .is_ok_and(|re| names.iter().any(|n| re.is_match(n)));
        all_matched &= found;
    }
    if let Some(cv) = &clause.command_version {
        any_field = true;
        let found = command_version_output(&cv.bin, &cv.args)
            .and_then(|combined| {
                Regex::new(&cv.matches)
                    .ok()
                    .map(|re| re.is_match(&combined))
            })
            .unwrap_or(false);
        all_matched &= found;
    }

    any_field && all_matched
}

pub fn detect(manifest: &TargetManifest, env: &ProbeEnv) -> Detection {
    if manifest
        .detect
        .iter()
        .any(|clause| eval_clause(clause, env))
    {
        Detection::Detected
    } else {
        Detection::NotDetected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::{CommandVersionProbe, DetectClause, TargetManifest, TargetMeta};
    use std::fs;
    use tempfile::tempdir;

    fn manifest(clauses: Vec<DetectClause>) -> TargetManifest {
        TargetManifest {
            schema: 1,
            target: TargetMeta {
                id: "test".into(),
                label: "Test".into(),
                kind: "generic".into(),
                verified: true,
                soft: false,
            },
            detect: clauses,
            install: crate::manifest::InstallSpec::Spawn {
                steps: vec![],
                uninstall: None,
            },
        }
    }

    fn env(home: Option<std::path::PathBuf>) -> ProbeEnv {
        ProbeEnv {
            path_dirs: vec![],
            home,
            extra_dirs: vec![],
            is_macos: false,
        }
    }

    #[test]
    fn empty_clauses_and_invalid_regex_never_detect() {
        let empty = manifest(vec![DetectClause::default()]);
        assert_eq!(detect(&empty, &env(None)), Detection::NotDetected);
        let invalid = manifest(vec![DetectClause {
            vscode_ext: Some("[".into()),
            ..Default::default()
        }]);
        assert_eq!(detect(&invalid, &env(None)), Detection::NotDetected);
    }

    #[test]
    fn file_and_home_expansion_probes_are_anded() {
        let tmp = tempdir().unwrap();
        let marker = tmp.path().join("marker");
        fs::write(&marker, "ok").unwrap();
        let matching = manifest(vec![DetectClause {
            file: Some(marker.to_string_lossy().into_owned()),
            ..Default::default()
        }]);
        assert_eq!(detect(&matching, &env(None)), Detection::Detected);

        let home = tmp.path().join("home");
        fs::create_dir_all(home.join(".frank")).unwrap();
        let home_probe = manifest(vec![DetectClause {
            dir: Some("$HOME/.frank".into()),
            file: Some("$HOME/.frank/missing".into()),
            ..Default::default()
        }]);
        assert_eq!(
            detect(&home_probe, &env(Some(home))),
            Detection::NotDetected
        );
    }

    #[test]
    fn vscode_cursor_and_jetbrains_probes_scan_expected_roots() {
        let tmp = tempdir().unwrap();
        fs::create_dir_all(tmp.path().join(".vscode/extensions")).unwrap();
        fs::create_dir_all(tmp.path().join(".cursor/extensions")).unwrap();
        fs::create_dir_all(tmp.path().join(".config/JetBrains/plugins")).unwrap();
        fs::write(tmp.path().join(".vscode/extensions/acme-frank-1.0.0"), "").unwrap();
        fs::write(tmp.path().join(".cursor/extensions/acme-cursor"), "").unwrap();
        fs::write(
            tmp.path().join(".config/JetBrains/plugins/frank-plugin"),
            "",
        )
        .unwrap();

        let vscode = manifest(vec![DetectClause {
            vscode_ext: Some("acme-frank".into()),
            ..Default::default()
        }]);
        assert_eq!(
            detect(&vscode, &env(Some(tmp.path().into()))),
            Detection::Detected
        );
        let cursor = manifest(vec![DetectClause {
            cursor_ext: Some("acme-cursor".into()),
            ..Default::default()
        }]);
        assert_eq!(
            detect(&cursor, &env(Some(tmp.path().into()))),
            Detection::Detected
        );
        let jetbrains = manifest(vec![DetectClause {
            jetbrains_config: Some(true),
            jetbrains_plugin: Some("frank-plugin".into()),
            ..Default::default()
        }]);
        assert_eq!(
            detect(&jetbrains, &env(Some(tmp.path().into()))),
            Detection::Detected
        );
    }

    #[test]
    fn command_version_probe_is_checked_with_stdout_and_stderr() {
        let tmp = tempdir().unwrap();
        let command = tmp.path().join("versioned");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::write(&command, "#!/bin/sh\nprintf 'frank 1.2.3\\n'\n").unwrap();
            let mut permissions = fs::metadata(&command).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&command, permissions).unwrap();
        }
        #[cfg(windows)]
        {
            fs::write(&command, "frank 1.2.3\n").unwrap();
        }
        let versioned = manifest(vec![DetectClause {
            command_version: Some(CommandVersionProbe {
                bin: command.to_string_lossy().into_owned(),
                args: vec![],
                matches: "1\\.2\\.3".into(),
            }),
            ..Default::default()
        }]);
        #[cfg(unix)]
        assert_eq!(detect(&versioned, &env(None)), Detection::Detected);
        #[cfg(windows)]
        assert_eq!(detect(&versioned, &env(None)), Detection::NotDetected);
    }

    #[test]
    #[cfg(unix)]
    fn command_version_probe_times_out_and_fails_closed() {
        let tmp = tempdir().unwrap();
        let command = tmp.path().join("hangs");
        fs::write(&command, "#!/bin/sh\nsleep 3\nprintf 'frank 1.2.3\\n'\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&command).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&command, permissions).unwrap();
        let manifest = manifest(vec![DetectClause {
            command_version: Some(CommandVersionProbe {
                bin: command.to_string_lossy().into_owned(),
                args: vec![],
                matches: "1\\.2\\.3".into(),
            }),
            ..Default::default()
        }]);
        assert_eq!(detect(&manifest, &env(None)), Detection::NotDetected);
    }

    #[test]
    fn macapp_probe_is_false_when_not_running_on_macos() {
        let manifest = manifest(vec![DetectClause {
            macapp: Some("Frank".into()),
            ..Default::default()
        }]);
        assert_eq!(detect(&manifest, &env(None)), Detection::NotDetected);
    }
}
