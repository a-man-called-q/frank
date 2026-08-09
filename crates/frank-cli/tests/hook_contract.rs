use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

fn frank() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_frank"));
    command.env_remove("FRANK_DEFAULT_LEVEL");
    command
}

fn configured(root: &Path) -> Command {
    let mut command = frank();
    command
        .env("CLAUDE_CONFIG_DIR", root.join("claude"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"));
    command
}

fn hook_with_input(root: &Path, name: &str, input: &str) -> std::process::Output {
    let mut child = configured(root)
        .args(["hook", name])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn malformed_hook_stdin_is_a_successful_noop() {
    let root = tempfile::tempdir().unwrap();
    let output = frank()
        .env("CLAUDE_CONFIG_DIR", root.path().join("claude"))
        .env("XDG_CONFIG_HOME", root.path().join("config"))
        .env("XDG_DATA_HOME", root.path().join("data"))
        .args(["hook", "user-prompt-submit"])
        .stdin(Stdio::piped())
        .output()
        .unwrap();
    assert!(output.status.success());
}

#[test]
fn every_hook_entrypoint_returns_zero_for_broken_environment_input() {
    let root = tempfile::tempdir().unwrap();
    for name in ["session-start", "user-prompt-submit", "statusline"] {
        let output = frank()
            .env("CLAUDE_CONFIG_DIR", root.path().join(name).join("claude"))
            .env("XDG_CONFIG_HOME", root.path().join(name).join("config"))
            .env("XDG_DATA_HOME", root.path().join(name).join("data"))
            .args(["hook", name])
            .stdin(Stdio::piped())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "hook {name} failed: {:?}",
            output.status
        );
    }
}

#[test]
fn valid_hooks_emit_context_statusline_stats_and_timestamped_ledger_entries() {
    let root = tempfile::tempdir().unwrap();
    let output = configured(root.path())
        .args(["on", "full"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let output = configured(root.path())
        .args(["hook", "session-start"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!output.stdout.is_empty());

    let output = configured(root.path())
        .args(["hook", "statusline"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let statusline = String::from_utf8_lossy(&output.stdout);
    assert!(statusline.contains("[CAVEMAN]"));
    assert!(!statusline.contains(":FULL]"));

    let output = configured(root.path())
        .args(["on", "lite"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let output = configured(root.path())
        .args(["hook", "statusline"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).contains(":LITE]"));

    let transcript = root.path().join("sessions").join("session-1.jsonl");
    let input = format!(
        r#"{{"prompt":"fix the auth bug","transcript_path":"{}"}}"#,
        transcript.display()
    );
    let output = hook_with_input(root.path(), "user-prompt-submit", &input);
    assert!(output.status.success());
    let context: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        context["hookSpecificOutput"]["hookEventName"],
        "UserPromptSubmit"
    );

    let ledger = fs::read_to_string(root.path().join("claude/.frank-ledger.jsonl")).unwrap();
    let entries: Vec<serde_json::Value> = ledger
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(entries.len() >= 2);
    assert!(
        entries
            .iter()
            .all(|entry| entry["ts"].as_i64().unwrap_or(0) > 1_000_000_000_000)
    );
    assert!(entries.iter().any(|entry| entry["kind"] == "activate"));
    assert!(entries.iter().any(|entry| entry["kind"] == "reinforce"));

    for args in ["/caveman-stats --share", "/caveman-stats --all"] {
        let input = format!(r#"{{"prompt":"{args}"}}"#);
        let output = hook_with_input(root.path(), "user-prompt-submit", &input);
        assert!(output.status.success());
        let response: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let reason = response["reason"].as_str().unwrap();
        if args.ends_with("--share") {
            assert!(reason.contains("no turns"));
        } else {
            assert!(reason.contains("Not enough data"));
        }
    }
}

#[test]
fn user_prompt_submit_handles_the_off_trigger_without_reinforcing() {
    let root = tempfile::tempdir().unwrap();
    let output = configured(root.path())
        .args(["on", "lite"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let output = hook_with_input(
        root.path(),
        "user-prompt-submit",
        r#"{"prompt":"turn off caveman"}"#,
    );
    assert!(output.status.success());
    assert!(output.stdout.is_empty());

    let output = configured(root.path()).args(["status"]).output().unwrap();
    assert!(String::from_utf8_lossy(&output.stdout).contains("frank: off"));
}

#[test]
fn hook_dispatch_happens_before_clap_construction() {
    let output = frank()
        .args(["hook", "not-a-real-hook", "--this-would-fail-clap"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
fn version_is_available_from_the_binary_adapter() {
    let output = frank().arg("--version").output().unwrap();
    assert!(output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .starts_with(&format!("frank {}", env!("CARGO_PKG_VERSION")))
    );
}
