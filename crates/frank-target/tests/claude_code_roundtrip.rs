//! Install/uninstall round trips against a fake config dir — the coverage
//! the archive's own `detectMatch`, checksum verification, and quoting
//! logic notably lacked (see AGENTS.md's testing notes). Every case here
//! writes real files to a `tempdir()` and reads them back; nothing is
//! mocked.

use frank_target::{InstallCtx, claude_code::ClaudeCodeTarget, plan};
use serde_json::{Value, json};
use tempfile::tempdir;

fn ctx(config_dir: &std::path::Path) -> InstallCtx {
    InstallCtx {
        config_dir: config_dir.to_path_buf(),
        frank_bin: std::path::PathBuf::from("/usr/local/bin/frank"),
        cwd: std::path::PathBuf::from("/repo"),
    }
}

fn settings(config_dir: &std::path::Path) -> Value {
    frank_target::settings::read_settings(&config_dir.join("settings.json")).unwrap()
}

#[test]
fn fresh_install_writes_both_hooks() {
    let tmp = tempdir().unwrap();
    let c = ctx(tmp.path());

    let install_plan = ClaudeCodeTarget::plan_install(&c);
    let log = plan::apply(&install_plan).unwrap();
    assert!(log.iter().any(|l| l.contains("SessionStart")));
    assert!(log.iter().any(|l| l.contains("UserPromptSubmit")));

    let s = settings(tmp.path());
    assert!(
        s["hooks"]["SessionStart"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("hook session-start")
    );
    assert!(
        s["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("hook user-prompt-submit")
    );
}

#[test]
fn install_is_idempotent_no_duplicate_entries() {
    let tmp = tempdir().unwrap();
    let c = ctx(tmp.path());

    plan::apply(&ClaudeCodeTarget::plan_install(&c)).unwrap();
    let before = std::fs::read_to_string(tmp.path().join("settings.json")).unwrap();
    plan::apply(&ClaudeCodeTarget::plan_install(&c)).unwrap();
    let after = std::fs::read_to_string(tmp.path().join("settings.json")).unwrap();

    assert_eq!(
        before, after,
        "second install must be a byte-identical no-op"
    );
    let s = settings(tmp.path());
    assert_eq!(s["hooks"]["SessionStart"].as_array().unwrap().len(), 1);
}

#[test]
fn uninstall_removes_only_frank_hooks_and_preserves_user_hooks() {
    let tmp = tempdir().unwrap();
    let c = ctx(tmp.path());

    // A user's own pre-existing hook that must survive uninstall.
    std::fs::write(
        tmp.path().join("settings.json"),
        json!({
            "hooks": {
                "SessionStart": [
                    { "hooks": [ { "type": "command", "command": "echo 'my own startup hook'" } ] }
                ]
            }
        })
        .to_string(),
    )
    .unwrap();

    plan::apply(&ClaudeCodeTarget::plan_install(&c)).unwrap();
    assert_eq!(
        settings(tmp.path())["hooks"]["SessionStart"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    plan::apply(&ClaudeCodeTarget::plan_uninstall(&c)).unwrap();
    let s = settings(tmp.path());
    let remaining = s["hooks"]["SessionStart"].as_array().unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(
        remaining[0]["hooks"][0]["command"],
        "echo 'my own startup hook'"
    );
    // UserPromptSubmit had only our hook, so the whole event key is gone.
    assert!(s["hooks"].get("UserPromptSubmit").is_none());
}

#[test]
fn uninstall_on_a_machine_that_was_never_installed_is_a_harmless_noop() {
    let tmp = tempdir().unwrap();
    let c = ctx(tmp.path());
    // No settings.json exists at all.
    let log = plan::apply(&ClaudeCodeTarget::plan_uninstall(&c)).unwrap();
    assert!(!tmp.path().join("settings.json").exists());
    assert!(log.is_empty() || log.iter().all(|l| !l.contains("removed")));
}

#[test]
fn install_tolerates_jsonc_with_a_malformed_hook_already_present() {
    let tmp = tempdir().unwrap();
    let c = ctx(tmp.path());

    // Comments, trailing commas, AND a malformed hook entry that would
    // otherwise make Claude Code discard the whole file.
    std::fs::write(
        tmp.path().join("settings.json"),
        r#"{
          // user's existing config
          "hooks": {
            "SessionStart": [
              { "hooks": [ { "type": "command" } ] }, // malformed: no command
            ],
          },
          "statusLine": { "type": "command", "command": "true" },
        }"#,
    )
    .unwrap();

    plan::apply(&ClaudeCodeTarget::plan_install(&c)).unwrap();
    let s = settings(tmp.path());
    // The malformed entry is gone (validated away), Frank's hook is there,
    // and the unrelated statusLine key survived the round trip.
    let session_start = s["hooks"]["SessionStart"].as_array().unwrap();
    assert_eq!(session_start.len(), 1);
    assert!(
        session_start[0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("hook session-start")
    );
    assert_eq!(s["statusLine"]["command"], "true");
}

#[test]
fn install_backs_up_settings_exactly_once() {
    let tmp = tempdir().unwrap();
    let c = ctx(tmp.path());
    std::fs::write(
        tmp.path().join("settings.json"),
        json!({"a": 1}).to_string(),
    )
    .unwrap();

    plan::apply(&ClaudeCodeTarget::plan_install(&c)).unwrap();
    let backup_path = tmp.path().join("settings.json.frank-backup");
    assert!(backup_path.exists());
    let first_backup = std::fs::read_to_string(&backup_path).unwrap();
    assert_eq!(first_backup, json!({"a": 1}).to_string());

    // A second install must not clobber the backup with the now-merged file.
    plan::apply(&ClaudeCodeTarget::plan_install(&c)).unwrap();
    let second_backup = std::fs::read_to_string(&backup_path).unwrap();
    assert_eq!(first_backup, second_backup);
}

#[cfg(unix)]
#[test]
fn install_refuses_a_settings_symlink_without_touching_the_target() {
    use std::os::unix::fs::symlink;

    let tmp = tempdir().unwrap();
    let c = ctx(tmp.path());
    let outside = tmp.path().join("outside-settings.json");
    std::fs::write(&outside, "{\"untouched\":true}\n").unwrap();
    symlink(&outside, tmp.path().join("settings.json")).unwrap();

    let result = plan::apply(&ClaudeCodeTarget::plan_install(&c));
    assert!(result.is_err());
    assert_eq!(
        std::fs::read_to_string(outside).unwrap(),
        "{\"untouched\":true}\n"
    );
}

#[test]
fn install_refuses_an_oversized_settings_document() {
    let tmp = tempdir().unwrap();
    let c = ctx(tmp.path());
    std::fs::write(
        tmp.path().join("settings.json"),
        format!(
            "{{\"padding\":\"{}\"}}",
            "x".repeat(frank_safeio::MAX_CONFIG_BYTES)
        ),
    )
    .unwrap();

    assert!(plan::apply(&ClaudeCodeTarget::plan_install(&c)).is_err());
}

/// Regression test: a fresh machine has no settings.json at all before the
/// first install. The backup step has nothing to copy on install #1 — it
/// must not then treat install #2 (where settings.json exists, but now
/// contains Frank's own merged hooks) as "the first backup" and capture
/// post-merge content while calling it pristine.
#[test]
fn backup_marker_survives_when_no_settings_json_existed_before_first_install() {
    let tmp = tempdir().unwrap();
    let c = ctx(tmp.path());
    assert!(!tmp.path().join("settings.json").exists());

    plan::apply(&ClaudeCodeTarget::plan_install(&c)).unwrap();
    let backup_path = tmp.path().join("settings.json.frank-backup");
    assert!(backup_path.exists());
    let first_backup = std::fs::read_to_string(&backup_path).unwrap();
    assert!(
        !first_backup.contains("hook session-start"),
        "backup must not contain Frank's own hooks"
    );

    // Second install must not overwrite the marker with the now-merged file.
    plan::apply(&ClaudeCodeTarget::plan_install(&c)).unwrap();
    let second_backup = std::fs::read_to_string(&backup_path).unwrap();
    assert_eq!(first_backup, second_backup);
    assert!(!second_backup.contains("hook session-start"));
}

#[test]
fn doctor_reports_missing_then_present_hooks() {
    let tmp = tempdir().unwrap();
    let c = ctx(tmp.path());

    let before = ClaudeCodeTarget::doctor(&c);
    assert!(before.iter().all(|d| !d.ok));

    plan::apply(&ClaudeCodeTarget::plan_install(&c)).unwrap();
    let after = ClaudeCodeTarget::doctor(&c);
    assert!(
        after.iter().all(|d| d.ok),
        "{:?}",
        after.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn dry_run_plan_description_matches_what_apply_actually_does() {
    let tmp = tempdir().unwrap();
    let c = ctx(tmp.path());
    let install_plan = ClaudeCodeTarget::plan_install(&c);

    // The description is generated from the same Action list apply() will
    // execute -- there's no separate "preview" code path to drift.
    let description = install_plan.describe();
    assert!(description.iter().any(|l| l.contains("SessionStart")));
    assert!(description.iter().any(|l| l.contains("UserPromptSubmit")));

    // Actually applying leaves the described hooks in place.
    plan::apply(&install_plan).unwrap();
    let s = settings(tmp.path());
    assert!(
        s["hooks"]["SessionStart"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("hook session-start")
    );
}

#[test]
fn detect_via_command_on_path() {
    use frank_target::ProbeEnv;
    let tmp = tempdir().unwrap();
    std::fs::write(tmp.path().join("claude"), "#!/bin/sh\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            tmp.path().join("claude"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
    let env = ProbeEnv {
        path_dirs: vec![tmp.path().to_path_buf()],
        home: None,
        extra_dirs: vec![],
        is_macos: false,
    };
    assert_eq!(
        ClaudeCodeTarget::detect(&env),
        frank_target::Detection::Detected
    );
}

#[test]
fn detect_via_home_dot_claude_dir() {
    use frank_target::ProbeEnv;
    let tmp = tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join(".claude")).unwrap();
    let env = ProbeEnv {
        path_dirs: vec![],
        home: Some(tmp.path().to_path_buf()),
        extra_dirs: vec![],
        is_macos: false,
    };
    assert_eq!(
        ClaudeCodeTarget::detect(&env),
        frank_target::Detection::Detected
    );
}

#[test]
fn detect_absent_when_neither_signal_present() {
    use frank_target::ProbeEnv;
    let tmp = tempdir().unwrap();
    let env = ProbeEnv {
        path_dirs: vec![],
        home: Some(tmp.path().to_path_buf()),
        extra_dirs: vec![],
        is_macos: false,
    };
    assert_eq!(
        ClaudeCodeTarget::detect(&env),
        frank_target::Detection::NotDetected
    );
}
