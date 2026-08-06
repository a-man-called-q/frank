use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

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

fn run_with_stdin(root: &Path, args: &[&str], input: &str) -> Output {
    let mut child = frank(root)
        .args(args)
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
fn doctor_returns_success_after_a_real_install() {
    let root = tempfile::tempdir().unwrap();
    let output = run(root.path(), &["install"]);
    assert_success(&output);
    let output = run(root.path(), &["doctor"]);
    assert_success(&output);
    let output = run(root.path(), &["uninstall"]);
    assert_success(&output);
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

    let zero_output = root.path().join("zero-output.jsonl");
    fs::write(
        &zero_output,
        r#"{"type":"assistant","timestamp":"2099-01-01T00:00:00.000Z","message":{"model":"claude-3-5-sonnet","usage":{"output_tokens":0,"input_tokens":8}}}"#,
    )
    .unwrap();
    let output = run(
        root.path(),
        &[
            "stats",
            "--session",
            zero_output.to_str().unwrap(),
            "--share",
        ],
    );
    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("no turns"));

    let output = run(root.path(), &["on", "full"]);
    assert_success(&output);
    let session = root.path().join("session.jsonl");
    fs::write(
        &session,
        r#"{"type":"assistant","timestamp":"2099-01-01T00:00:00.000Z","message":{"model":"claude-3-5-sonnet","usage":{"output_tokens":12,"input_tokens":8}}}"#,
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

    let output = run(
        root.path(),
        &["stats", "--session", session.to_str().unwrap(), "--share"],
    );
    assert_success(&output);
    let share = String::from_utf8_lossy(&output.stdout);
    assert!(share.contains("Saved ~22 output tokens"));
    assert!(share.contains("(~$0.0003)"), "{share}");
}

#[test]
fn pack_add_accepts_the_typed_yes_confirmation_via_stdin() {
    let root = tempfile::tempdir().unwrap();
    let local = root.path().join("local-pack");
    fs::create_dir_all(local.join("levels")).unwrap();
    fs::write(
        local.join("pack.toml"),
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
    fs::write(local.join("levels/full.md"), "Respond with short answers.").unwrap();
    let local_source = local.to_string_lossy().into_owned();

    let output = run_with_stdin(root.path(), &["pack", "add", &local_source], "yes\n");
    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("installed local"));

    let output = run(root.path(), &["pack", "list"]);
    assert!(String::from_utf8_lossy(&output.stdout).contains("local 1.0.0"));
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
    assert!(
        String::from_utf8_lossy(&output.stdout)
            .contains("would shrink 124B -> 93B (25.0% smaller)")
    );

    let output = run(
        root.path(),
        &["compress", "--dry-run", path.to_str().unwrap()],
    );
    assert_success(&output);
    assert!(!backup.exists());

    fs::create_dir_all(backup.parent().unwrap()).unwrap();
    fs::create_dir(&backup).unwrap();
    let output = run(root.path(), &["compress", path.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("non-file backup"));
    fs::remove_dir(&backup).unwrap();

    fs::write(&backup, "existing backup").unwrap();
    let output = run(root.path(), &["compress", path.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("overwrite existing backup"));
    fs::remove_file(&backup).unwrap();

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&path, &backup).unwrap();
        let output = run(root.path(), &["compress", path.to_str().unwrap()]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("symlinked backup"));
        fs::remove_file(&backup).unwrap();
    }

    let backup_parent = backup.parent().unwrap();
    fs::remove_dir_all(backup_parent).unwrap();
    fs::write(backup_parent, "backup path blocker").unwrap();
    let output = run(root.path(), &["compress", path.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot inspect backup"));
    fs::remove_file(backup_parent).unwrap();

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

    let backup_contents = fs::read_to_string(&backup).unwrap();
    fs::remove_file(&backup).unwrap();
    #[cfg(unix)]
    {
        let decoy = root.path().join("decoy.md");
        fs::write(&decoy, original).unwrap();
        std::os::unix::fs::symlink(&decoy, &backup).unwrap();
        let output = run(
            root.path(),
            &["compress", "--restore", path.to_str().unwrap()],
        );
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("symlinked backup"));
        fs::remove_file(&backup).unwrap();
    }
    fs::create_dir(&backup).unwrap();
    let output = run(
        root.path(),
        &["compress", "--restore", path.to_str().unwrap()],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("backup is not a regular file"));
    fs::remove_dir(&backup).unwrap();

    let backup_parent = backup.parent().unwrap();
    fs::remove_dir_all(backup_parent).unwrap();
    fs::write(backup_parent, "restore path blocker").unwrap();
    let output = run(
        root.path(),
        &["compress", "--restore", path.to_str().unwrap()],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot inspect backup"));
    fs::remove_file(backup_parent).unwrap();
    fs::create_dir_all(backup_parent).unwrap();
    fs::write(&backup, backup_contents).unwrap();

    let output = run(
        root.path(),
        &["compress", "--restore", path.to_str().unwrap()],
    );
    assert_success(&output);
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    assert!(!backup.exists());

    let output = run(
        root.path(),
        &["compress", "--restore", path.to_str().unwrap()],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no backup found"));

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

    let output = run(root.path(), &["pack", "add", &source, "--yes"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("reserved"));

    let local = root.path().join("local-pack");
    fs::create_dir_all(local.join("levels")).unwrap();
    fs::write(
        local.join("pack.toml"),
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
    fs::write(local.join("levels/full.md"), "Respond with short answers.").unwrap();
    let local_source = local.to_string_lossy().into_owned();
    let output = run_with_stdin(root.path(), &["pack", "add", &local_source], "no\n");
    assert!(!output.status.success());
    let output_text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output_text.contains("not installed"));
    let output = run(root.path(), &["pack", "add", &local_source, "--yes"]);
    assert_success(&output);
    let output = run(root.path(), &["pack", "list"]);
    assert!(String::from_utf8_lossy(&output.stdout).contains("local 1.0.0"));
    let output = run(root.path(), &["levels"]);
    let levels = String::from_utf8_lossy(&output.stdout);
    assert!(levels.contains("  full [default]"), "{levels}");
    assert!(!levels.contains("  full\n"), "{levels}");

    let output = run(root.path(), &["pack", "use", "caveman"]);
    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("using built-in pack"));
    let output = run(root.path(), &["pack", "use", "caveman@2.0.0"]);
    assert_success(&output);
    assert!(String::from_utf8_lossy(&output.stdout).contains("using built-in pack"));
    let output = run(root.path(), &["pack", "use", "missing"]);
    assert!(!output.status.success());
    let output = run(root.path(), &["pack", "use", "caveman@9.9.9"]);
    assert!(!output.status.success());
    let output = run(root.path(), &["pack", "remove", "caveman"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("the built-in caveman pack cannot be removed")
    );
    let output = run(root.path(), &["pack", "remove", "caveman@9.9.9"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot be removed"));

    let output = run(root.path(), &["doctor"]);
    assert_eq!(output.status.code(), Some(1));

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
