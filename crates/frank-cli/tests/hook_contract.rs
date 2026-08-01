use std::process::{Command, Stdio};

fn frank() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_frank"));
    command.env_remove("FRANK_DEFAULT_LEVEL");
    command
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
    assert!(String::from_utf8_lossy(&output.stdout).starts_with("frank 0.1.0"));
}
