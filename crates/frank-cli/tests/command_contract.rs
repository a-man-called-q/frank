use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn frank(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_frank"));
    command
        .current_dir(root)
        .env("CLAUDE_CONFIG_DIR", root.join("claude"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env_remove("FRANK_DEFAULT_LEVEL");
    command
}

fn run(root: &Path, args: &[&str]) -> Output {
    frank(root).args(args).output().unwrap()
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "command failed: {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn built_in_pack_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packs/caveman")
}

fn backup_path(root: &Path, source: &Path) -> PathBuf {
    root.join("data")
        .join("frank-compress")
        .join("backups")
        .join(source.parent().unwrap().file_name().unwrap())
        .join("notes.original.md")
}

#[test]
fn state_commands_cover_on_status_off_levels_and_invalid_level() {
    let root = tempfile::tempdir().unwrap();

    let output = run(root.path(), &["on", "lite"]);
    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("frank: on (lite)"));

    let output = run(root.path(), &["status"]);
    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("frank: on (lite)"));

    let output = run(root.path(), &["levels"]);
    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("full"));

    let output = run(root.path(), &["on", "not-a-level"]);
    assert!(!output.status.success());

    let output = run(root.path(), &["off"]);
    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("frank: off"));
}

#[test]
fn install_and_uninstall_dry_run_cover_native_and_unknown_targets() {
    let root = tempfile::tempdir().unwrap();
    let output = run(root.path(), &["install", "--dry-run"]);
    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("Would apply to claude-code"));

    let output = run(root.path(), &["uninstall", "--dry-run"]);
    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("Would apply to claude-code"));

    let output = run(root.path(), &["install", "--dry-run", "--only", "missing"]);
    assert!(!output.status.success());
}

#[test]
fn targets_support_json_and_detected_filters() {
    let root = tempfile::tempdir().unwrap();
    let output = run(root.path(), &["targets", "--json"]);
    assert_success(&output);
    let rows: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        rows.as_array().unwrap().iter().any(|row| {
            row.get("id").and_then(serde_json::Value::as_str) == Some("claude-code")
        })
    );

    let output = run(root.path(), &["targets", "--detected", "--json"]);
    assert_success(&output);
    assert!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout)
            .unwrap()
            .is_array()
    );

    fs::create_dir_all(root.path().join("targets")).unwrap();
    fs::write(root.path().join("targets/broken.toml"), "[not valid").unwrap();
    let output = run(root.path(), &["targets", "--json"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(serde_json::from_slice::<serde_json::Value>(&output.stdout).is_ok());
    assert!(String::from_utf8_lossy(&output.stderr).contains("failed to parse target manifest"));
}

#[test]
fn stats_rendering_covers_empty_json_share_explain_and_lifetime_paths() {
    let root = tempfile::tempdir().unwrap();
    let output = run(root.path(), &["stats", "--json"]);
    assert_success(&output);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json.get("turns").and_then(serde_json::Value::as_u64),
        Some(0)
    );

    let output = run(root.path(), &["stats", "--share"]);
    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("no turns"));

    let output = run(root.path(), &["stats", "--explain"]);
    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("Explain:"));

    let output = run(root.path(), &["stats", "--all"]);
    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("Not enough data"));

    let session = root.path().join("session.jsonl");
    fs::write(
        &session,
        r#"{"type":"assistant","timestamp":"2026-08-01T00:00:00.000Z","message":{"model":"m","usage":{"output_tokens":12,"input_tokens":8}}}"#,
    )
    .unwrap();
    let output = run(
        root.path(),
        &["stats", "--session", session.to_str().unwrap(), "--json"],
    );
    assert_success(&output);
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        json.get("turns").and_then(serde_json::Value::as_u64),
        Some(1)
    );
}

#[test]
fn compression_check_dry_run_write_and_restore_are_safe() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("notes.md");
    let backup = backup_path(root.path(), &path);
    let original = "Please just review this useful project note and basically explain the important implementation details for the next change.\n";
    fs::write(&path, original).unwrap();

    let output = run(
        root.path(),
        &["compress", "--check", path.to_str().unwrap()],
    );
    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("would shrink"));

    let output = run(
        root.path(),
        &["compress", "--dry-run", path.to_str().unwrap()],
    );
    assert_success(&output);
    assert!(!backup.exists());

    let output = run(root.path(), &["compress", path.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "compress failed: {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        backup.exists(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_ne!(fs::read_to_string(&path).unwrap(), original);

    let output = run(
        root.path(),
        &["compress", "--restore", path.to_str().unwrap()],
    );
    assert_success(&output);
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert!(!backup.exists());

    let output = run(root.path(), &["compress"]);
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn pack_commands_cover_builtin_paths_and_fail_closed_errors() {
    let root = tempfile::tempdir().unwrap();
    let output = run(root.path(), &["pack", "list"]);
    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("caveman"));

    let output = run(root.path(), &["pack", "show"]);
    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("caveman v"));

    let source = built_in_pack_path().to_string_lossy().into_owned();
    let output = run(root.path(), &["pack", "build", &source]);
    assert_success(&output);

    let output = run(root.path(), &["pack", "use", "caveman"]);
    assert_success(&output);
    let output = run(root.path(), &["pack", "remove", "caveman"]);
    assert!(!output.status.success());

    let output = run(
        root.path(),
        &["pack", "add", "https://example.test/pack", "--yes"],
    );
    assert_eq!(output.status.code(), Some(2));

    let output = run(root.path(), &["pack", "show", "missing"]);
    assert!(!output.status.success());
}

#[test]
fn mcp_cli_reports_usage_and_spawn_failures_without_panicking() {
    let root = tempfile::tempdir().unwrap();
    let output = run(root.path(), &["mcp", "proxy"]);
    assert_eq!(output.status.code(), Some(2));

    let output = run(
        root.path(),
        &["mcp", "proxy", "--", "frank-command-that-does-not-exist"],
    );
    assert_eq!(output.status.code(), Some(1));
}
