#[cfg(target_os = "linux")]
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

#[cfg(target_os = "linux")]
use std::os::unix::ffi::OsStringExt;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn frank_release(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_frank-release"));
    command
        .current_dir(root)
        // `cargo test` sets this in the test binary's own environment, and a
        // child process inherits it by default. `repo_root()` checks it
        // first and, left alone, would resolve to this workspace's real
        // root instead of the tempdir fixture below -- silently operating on
        // the real repo instead of the isolated one under test.
        .env_remove("CARGO_MANIFEST_DIR");
    command
}

fn run(root: &Path, args: &[&str]) -> Output {
    frank_release(root).args(args).output().unwrap()
}

#[cfg(unix)]
fn run_with_path(root: &Path, args: &[&str], bin_dir: &Path) -> Output {
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let path = format!("{}:{}", bin_dir.display(), inherited.to_string_lossy());
    frank_release(root)
        .env("PATH", path)
        .args(args)
        .output()
        .unwrap()
}

#[cfg(unix)]
fn run_with_exact_path(root: &Path, args: &[&str], bin_dir: &Path) -> Output {
    frank_release(root)
        .env("PATH", bin_dir)
        .args(args)
        .output()
        .unwrap()
}

#[cfg(unix)]
fn run_with_manifest_dir(root: &Path, args: &[&str], manifest_dir: &Path) -> Output {
    frank_release(root)
        .env("CARGO_MANIFEST_DIR", manifest_dir)
        .args(args)
        .output()
        .unwrap()
}

#[cfg(unix)]
fn fake_tool(name: &str, script: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(name);
    write_executable(&path, script);
    (dir, path)
}

#[cfg(unix)]
fn write_executable(path: &Path, script: &str) {
    fs::write(path, format!("#!/bin/sh\n{script}\n")).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(mode);
    fs::set_permissions(path, permissions).unwrap();
}

fn assert_success(output: &Output) -> String {
    assert!(
        output.status.success(),
        "command failed: {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn assert_failure(output: &Output) -> String {
    assert!(
        !output.status.success(),
        "expected failure but command succeeded:\nstdout:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn write_workspace_manifest(root: &Path, version: &str) {
    fs::write(
        root.join("Cargo.toml"),
        format!("[workspace]\nmembers = []\n\n[workspace.package]\nversion = \"{version}\"\n"),
    )
    .unwrap();
}

fn write_cargo_workspace(root: &Path, version: &str) {
    fs::create_dir_all(root.join("crates/fixture/src")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        format!(
            "[workspace]\nresolver = \"2\"\nmembers = [\"crates/fixture\"]\n\n[workspace.package]\nversion = \"{version}\"\nedition = \"2021\"\n"
        ),
    )
    .unwrap();
    fs::write(
        root.join("crates/fixture/Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion.workspace = true\nedition.workspace = true\n",
    )
    .unwrap();
    fs::write(
        root.join("crates/fixture/src/lib.rs"),
        "pub fn fixture() {}\n",
    )
    .unwrap();
}

fn git(root: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap()
}

fn init_git_repo(root: &Path) {
    assert!(git(root, &["init", "--quiet"]).status.success());
    assert!(
        git(
            root,
            &["config", "user.email", "frank-release-test@example.com"]
        )
        .status
        .success()
    );
    assert!(
        git(root, &["config", "user.name", "Frank Release Test"])
            .status
            .success()
    );
}

fn commit_all(root: &Path, message: &str) {
    assert!(git(root, &["add", "-A"]).status.success());
    assert!(
        git(root, &["commit", "--quiet", "-m", message])
            .status
            .success()
    );
}

/// A `Cargo.toml`-less subdirectory of `root`. Running commands from here
/// (instead of `root` itself) is what actually exercises `repo_root`'s
/// walk-up: relative-path fallbacks or a short-circuiting `is_workspace_root`
/// would resolve *something* when the process cwd already equals `root`, but
/// only genuinely walking up from a location with no manifest of its own
/// finds the real workspace root.
fn nested_workdir(root: &Path) -> PathBuf {
    let dir = root.join("crates").join("somecrate");
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn status_walks_up_to_the_workspace_root_and_reports_clean_git_state() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "1.2.3");
    init_git_repo(root);
    commit_all(root, "initial");

    let sub = nested_workdir(root);
    let output = run(&sub, &["status"]);
    let stdout = assert_success(&output);
    assert!(
        stdout.contains("Cargo Workspace Version: 1.2.3"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Git Tree State:          CLEAN"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Latest Git Tag:          (none)"),
        "{stdout}"
    );

    let (git_dir, _) = fake_tool(
        "git",
        r#"
case "$1" in
  status|describe) exit 0 ;;
  *) exit 1 ;;
esac
"#,
    );
    let stdout = assert_success(&run_with_path(root, &["status"], git_dir.path()));
    assert!(
        stdout.contains("Latest Git Tag:          (none)"),
        "{stdout}"
    );
}

#[test]
fn status_reports_dirty_tree_after_an_uncommitted_change() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "1.2.3");
    init_git_repo(root);
    commit_all(root, "initial");
    fs::write(root.join("dirty.txt"), "uncommitted").unwrap();

    let output = run(root, &["status"]);
    let stdout = assert_success(&output);
    assert!(
        stdout.contains("Git Tree State:          DIRTY (uncommitted changes)"),
        "{stdout}"
    );
}

#[test]
fn status_reports_the_latest_git_tag_when_one_exists() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "1.2.3");
    init_git_repo(root);
    commit_all(root, "initial");
    assert!(
        git(root, &["tag", "-a", "v0.9.0", "-m", "prior release"])
            .status
            .success()
    );

    let output = run(root, &["status"]);
    let stdout = assert_success(&output);
    assert!(
        stdout.contains("Latest Git Tag:          v0.9.0"),
        "{stdout}"
    );
}

#[cfg(unix)]
#[test]
fn status_handles_invalid_and_malformed_git_output() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "1.2.3");

    let (git_dir, _) = fake_tool(
        "git",
        r#"
case "$1" in
  status) printf '\377'; exit 0 ;;
  describe) exit 1 ;;
  *) exit 1 ;;
esac
"#,
    );
    let stdout = assert_success(&run_with_path(root, &["status"], git_dir.path()));
    assert!(stdout.contains("DIRTY (uncommitted changes)"), "{stdout}");

    let (git_dir, _) = fake_tool(
        "git",
        r#"
case "$1" in
  status) printf '??'; exit 0 ;;
  describe) printf '\377'; exit 0 ;;
  *) exit 1 ;;
esac
"#,
    );
    let stdout = assert_success(&run_with_path(root, &["status"], git_dir.path()));
    assert!(
        stdout.contains("Latest Git Tag:          (none)"),
        "{stdout}"
    );
}

#[cfg(unix)]
#[test]
fn repo_root_covers_manifest_override_and_missing_root_paths() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "1.2.3");

    let fake_manifest = root.join("not/a/real/manifest");
    let stdout = assert_success(&run_with_manifest_dir(root, &["status"], &fake_manifest));
    assert!(
        stdout.contains("Cargo Workspace Version: 1.2.3"),
        "{stdout}"
    );

    let stdout = assert_success(&run_with_manifest_dir(
        root,
        &["status"],
        Path::new(env!("CARGO_MANIFEST_DIR")),
    ));
    assert!(stdout.contains("Cargo Workspace Version:"), "{stdout}");

    let stdout = assert_success(&run_with_manifest_dir(root, &["status"], Path::new("/")));
    assert!(
        stdout.contains("Cargo Workspace Version: 1.2.3"),
        "{stdout}"
    );

    let no_root = tempfile::tempdir().unwrap();
    let output = run_with_manifest_dir(
        no_root.path(),
        &["status"],
        &no_root.path().join("not/a/manifest"),
    );
    let stderr = assert_failure(&output);
    assert!(
        stderr.contains("Could not locate repository root"),
        "{stderr}"
    );
}

#[cfg(unix)]
#[test]
fn publish_fails_closed_when_cargo_cannot_validate_the_lockfile() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_cargo_workspace(root, "1.2.3");
    init_git_repo(root);
    commit_all(root, "initial");
    assert!(
        git(
            root,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/acme/frank.git",
            ]
        )
        .status
        .success()
    );

    let (cargo_dir, _) = fake_tool(
        "cargo",
        r#"
if [ "$2" = "--locked" ]; then exit 1; fi
exit 0
"#,
    );
    let output = run_with_path(
        root,
        &["bump", "--publish", "--no-push", "patch"],
        cargo_dir.path(),
    );
    let stderr = assert_failure(&output);
    assert!(
        stderr.contains("Cargo.lock is not synchronized"),
        "{stderr}"
    );
}

#[cfg(unix)]
#[test]
fn publish_rejects_files_created_during_release_preparation() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_cargo_workspace(root, "1.2.3");
    init_git_repo(root);
    commit_all(root, "initial");
    assert!(
        git(
            root,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/acme/frank.git",
            ]
        )
        .status
        .success()
    );

    let (cargo_dir, _) = fake_tool("cargo", "touch unexpected.txt\nexit 0");
    let output = run_with_path(
        root,
        &["bump", "--publish", "--no-push", "patch"],
        cargo_dir.path(),
    );
    let stderr = assert_failure(&output);
    assert!(stderr.contains("unexpected files"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn publish_reports_invalid_git_branch_and_remote_output() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "1.2.3");

    let (git_dir, _) = fake_tool(
        "git",
        r#"
case "$1" in
  status) exit 0 ;;
  rev-parse) exit 1 ;;
  symbolic-ref) exit 0 ;;
  *) exit 1 ;;
esac
"#,
    );
    let output = run_with_path(root, &["bump", "--publish", "--dry-run"], git_dir.path());
    let stderr = assert_failure(&output);
    assert!(stderr.contains("non-empty current git branch"), "{stderr}");

    let (git_dir, _) = fake_tool(
        "git",
        r#"
case "$1" in
  status) exit 0 ;;
  rev-parse) exit 1 ;;
  symbolic-ref) printf 'main'; exit 0 ;;
  remote) printf '\377'; exit 0 ;;
  *) exit 1 ;;
esac
"#,
    );
    let stdout = assert_success(&run_with_path(
        root,
        &["bump", "--publish", "--dry-run"],
        git_dir.path(),
    ));
    assert!(stdout.contains("URL unavailable from origin"), "{stdout}");

    let (git_dir, _) = fake_tool(
        "git",
        r#"
case "$1" in
  status) exit 0 ;;
  rev-parse) exit 1 ;;
  symbolic-ref) printf '\377'; exit 0 ;;
  *) exit 1 ;;
esac
"#,
    );
    let output = run_with_path(root, &["bump", "--publish", "--dry-run"], git_dir.path());
    let stderr = assert_failure(&output);
    assert!(stderr.contains("invalid utf-8"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn publish_reports_version_and_tag_lookup_errors() {
    let git_script = r#"
case "$1" in
  status) exit 0 ;;
  rev-parse) exit 1 ;;
  *) exit 1 ;;
esac
"#;

    let missing_version = tempfile::tempdir().unwrap();
    fs::write(
        missing_version.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\n",
    )
    .unwrap();
    let (git_dir, _) = fake_tool("git", git_script);
    let output = run_with_path(
        missing_version.path(),
        &["bump", "--publish", "--dry-run"],
        git_dir.path(),
    );
    let stderr = assert_failure(&output);
    assert!(
        stderr.contains("Missing workspace.package.version"),
        "{stderr}"
    );

    let invalid_version = tempfile::tempdir().unwrap();
    write_workspace_manifest(invalid_version.path(), "not-semver");
    let (git_dir, _) = fake_tool("git", git_script);
    let output = run_with_path(
        invalid_version.path(),
        &["bump", "--publish", "--dry-run"],
        git_dir.path(),
    );
    let stderr = assert_failure(&output);
    assert!(stderr.contains("not valid semver"), "{stderr}");

    let missing_git = tempfile::tempdir().unwrap();
    write_workspace_manifest(missing_git.path(), "1.2.3");
    let (git_dir, _) = fake_tool(
        "git",
        r#"
case "$1" in
  status) /bin/rm "$0"; exit 0 ;;
  *) exit 1 ;;
esac
"#,
    );
    let output = run_with_exact_path(
        missing_git.path(),
        &["bump", "--publish", "--dry-run"],
        git_dir.path(),
    );
    let stderr = assert_failure(&output);
    assert!(stderr.contains("checking existing git tag"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn publish_reports_preparation_and_commit_process_errors() {
    let preparation = tempfile::tempdir().unwrap();
    write_workspace_manifest(preparation.path(), "1.2.3");
    let bin = tempfile::tempdir().unwrap();
    write_executable(
        &bin.path().join("git"),
        r#"
case "$1" in
  status) exit 0 ;;
  rev-parse) exit 1 ;;
  symbolic-ref) printf 'main'; exit 0 ;;
  remote) /bin/rm "$0"; exit 1 ;;
  *) exit 0 ;;
esac
"#,
    );
    write_executable(&bin.path().join("cargo"), "exit 0");
    let output = run_with_exact_path(
        preparation.path(),
        &["bump", "--publish", "--no-push", "patch"],
        bin.path(),
    );
    let stderr = assert_failure(&output);
    assert!(stderr.contains("running git status"), "{stderr}");

    let stage_failure = tempfile::tempdir().unwrap();
    write_workspace_manifest(stage_failure.path(), "1.2.3");
    let bin = tempfile::tempdir().unwrap();
    write_executable(
        &bin.path().join("git"),
        r#"
case "$1" in
  status)
    if [ ! -e "$0.state" ]; then : > "$0.state"; exit 0; fi
    printf ' M Cargo.toml'; exit 0 ;;
  rev-parse) exit 1 ;;
  symbolic-ref) printf 'main'; exit 0 ;;
  remote) exit 1 ;;
  add) exit 1 ;;
  *) exit 0 ;;
esac
"#,
    );
    write_executable(&bin.path().join("cargo"), "exit 0");
    let output = run_with_exact_path(
        stage_failure.path(),
        &["bump", "--publish", "--no-push", "patch"],
        bin.path(),
    );
    let stderr = assert_failure(&output);
    assert!(stderr.contains("failed to stage release files"), "{stderr}");

    let commit_spawn = tempfile::tempdir().unwrap();
    write_workspace_manifest(commit_spawn.path(), "1.2.3");
    let bin = tempfile::tempdir().unwrap();
    write_executable(
        &bin.path().join("git"),
        r#"
case "$1" in
  status)
    if [ ! -e "$0.state" ]; then : > "$0.state"; exit 0; fi
    printf ' M Cargo.toml'; exit 0 ;;
  rev-parse) exit 1 ;;
  symbolic-ref) printf 'main'; exit 0 ;;
  remote) exit 1 ;;
  add) /bin/rm "$0"; exit 0 ;;
  *) exit 0 ;;
esac
"#,
    );
    write_executable(&bin.path().join("cargo"), "exit 0");
    let output = run_with_exact_path(
        commit_spawn.path(),
        &["bump", "--publish", "--no-push", "patch"],
        bin.path(),
    );
    let stderr = assert_failure(&output);
    assert!(stderr.contains("creating release commit"), "{stderr}");

    let tag_failure = tempfile::tempdir().unwrap();
    write_workspace_manifest(tag_failure.path(), "1.2.3");
    let bin = tempfile::tempdir().unwrap();
    write_executable(
        &bin.path().join("git"),
        r#"
case "$1" in
  status) exit 0 ;;
  rev-parse) exit 1 ;;
  symbolic-ref) printf 'main'; exit 0 ;;
  remote) exit 1 ;;
  tag) exit 1 ;;
  *) exit 0 ;;
esac
"#,
    );
    write_executable(&bin.path().join("cargo"), "exit 0");
    let output = run_with_exact_path(
        tag_failure.path(),
        &["bump", "--publish", "--no-push", "patch"],
        bin.path(),
    );
    let stderr = assert_failure(&output);
    assert!(stderr.contains("failed to create release tag"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn publish_reports_cargo_validation_spawn_errors() {
    let dir = tempfile::tempdir().unwrap();
    write_workspace_manifest(dir.path(), "1.2.3");
    let bin = tempfile::tempdir().unwrap();
    write_executable(
        &bin.path().join("git"),
        r#"
case "$1" in
  status) exit 0 ;;
  rev-parse) exit 1 ;;
  symbolic-ref) printf 'main'; exit 0 ;;
  remote) exit 1 ;;
  *) exit 0 ;;
esac
"#,
    );
    write_executable(&bin.path().join("cargo"), "/bin/rm \"$0\"; exit 0");

    let output = run_with_exact_path(
        dir.path(),
        &["bump", "--publish", "--no-push", "patch"],
        bin.path(),
    );
    let stderr = assert_failure(&output);
    assert!(stderr.contains("validating Cargo.lock"), "{stderr}");
}

#[test]
fn verify_accepts_valid_semver_and_rejects_invalid_semver() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "2.4.6");
    let output = run(root, &["verify"]);
    let stdout = assert_success(&output);
    assert!(
        stdout.contains("Verification passed: version 2.4.6 is valid."),
        "{stdout}"
    );

    let dir2 = tempfile::tempdir().unwrap();
    let root2 = dir2.path();
    write_workspace_manifest(root2, "not-a-version");
    let output = run(root2, &["verify"]);
    let stderr = assert_failure(&output);
    assert!(stderr.contains("is not valid semver"), "{stderr}");
}

#[test]
fn bump_accepts_an_explicit_target_and_the_patch_minor_major_keywords() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "1.2.3");

    let output = run(root, &["bump", "9.9.9"]);
    let stdout = assert_success(&output);
    assert!(
        stdout.contains("Bumping version: 1.2.3 -> 9.9.9"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Version successfully bumped to 9.9.9!"),
        "{stdout}"
    );
    assert!(
        fs::read_to_string(root.join("Cargo.toml"))
            .unwrap()
            .contains("version = \"9.9.9\"")
    );

    assert_success(&run(root, &["bump", "patch"]));
    assert!(
        fs::read_to_string(root.join("Cargo.toml"))
            .unwrap()
            .contains("version = \"9.9.10\"")
    );

    assert_success(&run(root, &["bump", "minor"]));
    assert!(
        fs::read_to_string(root.join("Cargo.toml"))
            .unwrap()
            .contains("version = \"9.10.0\"")
    );

    assert_success(&run(root, &["bump", "major"]));
    assert!(
        fs::read_to_string(root.join("Cargo.toml"))
            .unwrap()
            .contains("version = \"10.0.0\"")
    );
}

#[test]
fn bump_rejects_an_invalid_target() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "1.0.0");
    let output = run(root, &["bump", "not-a-version"]);
    let stderr = assert_failure(&output);
    assert!(stderr.contains("Invalid target version"), "{stderr}");
    assert!(
        fs::read_to_string(root.join("Cargo.toml"))
            .unwrap()
            .contains("version = \"1.0.0\""),
        "a rejected bump must not touch the manifest"
    );
}

#[test]
fn bump_rejects_publish_only_flags_without_publish() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "1.0.0");

    let output = run(root, &["bump", "--dry-run"]);
    let stderr = assert_failure(&output);
    assert!(stderr.contains("require --publish"), "{stderr}");

    let output = run(root, &["bump"]);
    let stderr = assert_failure(&output);
    assert!(stderr.contains("bump requires a target"), "{stderr}");
}

#[test]
fn bump_keeps_internal_path_dependency_constraints_in_lockstep() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "1.2.3");
    fs::create_dir_all(root.join("crates/a")).unwrap();
    fs::create_dir_all(root.join("crates/b")).unwrap();
    fs::write(
        root.join("crates/a/Cargo.toml"),
        "[package]\nname = \"a\"\nversion.workspace = true\n\n[dependencies]\nb = { path = \"../b\", version = \"1.2.3\" }\n",
    )
    .unwrap();
    fs::write(
        root.join("crates/b/Cargo.toml"),
        "[package]\nname = \"b\"\nversion.workspace = true\n",
    )
    .unwrap();

    assert_success(&run(root, &["bump", "2.0.0"]));
    let dependency_manifest = fs::read_to_string(root.join("crates/a/Cargo.toml")).unwrap();
    assert!(dependency_manifest.contains("version = \"2.0.0\""));
}

#[test]
fn publish_dry_run_uses_the_current_version_without_changing_git() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "1.2.3");
    init_git_repo(root);
    commit_all(root, "initial");

    let output = run(root, &["bump", "--publish", "--dry-run"]);
    let stdout = assert_success(&output);
    assert!(
        stdout.contains("Version:                 1.2.3 -> 1.2.3"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Tag:                     v1.2.3"),
        "{stdout}"
    );
    assert!(stdout.contains("[DRY RUN] No files"), "{stdout}");
    assert!(
        String::from_utf8_lossy(&git(root, &["tag", "-l"]).stdout)
            .trim()
            .is_empty()
    );
    assert!(git(root, &["status", "--porcelain"]).stdout.is_empty());
}

#[test]
fn publish_without_push_commits_and_tags_the_release_locally() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "1.2.3");
    fs::create_dir_all(root.join("crates/fixture")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/fixture\"]\n\n[workspace.package]\nversion = \"1.2.3\"\n",
    )
    .unwrap();
    fs::write(
        root.join("crates/fixture/Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion.workspace = true\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("crates/fixture/src")).unwrap();
    fs::write(
        root.join("crates/fixture/src/lib.rs"),
        "pub fn fixture() {}\n",
    )
    .unwrap();
    init_git_repo(root);
    commit_all(root, "initial");

    let output = run(root, &["bump", "--publish", "--no-push", "patch"]);
    let stdout = assert_success(&output);
    assert!(
        stdout.contains("Created release commit for v1.2.4"),
        "{stdout}"
    );
    assert!(stdout.contains("Created release tag v1.2.4"), "{stdout}");
    assert!(stdout.contains("prepared locally"), "{stdout}");
    assert_eq!(
        String::from_utf8_lossy(&git(root, &["tag", "-l"]).stdout).trim(),
        "v1.2.4"
    );
    assert!(git(root, &["status", "--porcelain"]).stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&git(root, &["log", "-1", "--pretty=%s"]).stdout)
            .contains("chore(release): v1.2.4")
    );
}

#[test]
fn publish_pushes_the_branch_and_tag_to_a_configured_push_url() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir_all(root.join("crates/fixture/src")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nresolver = \"2\"\nmembers = [\"crates/fixture\"]\n\n[workspace.package]\nversion = \"1.2.3\"\nedition = \"2021\"\n",
    )
    .unwrap();
    fs::write(
        root.join("crates/fixture/Cargo.toml"),
        "[package]\nname = \"fixture\"\nversion.workspace = true\nedition.workspace = true\n",
    )
    .unwrap();
    fs::write(
        root.join("crates/fixture/src/lib.rs"),
        "pub fn fixture() {}\n",
    )
    .unwrap();
    init_git_repo(root);
    commit_all(root, "initial");

    let remote = tempfile::tempdir().unwrap();
    assert!(
        git(remote.path(), &["init", "--bare", "--quiet"])
            .status
            .success()
    );
    assert!(
        git(
            root,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/acme/frank.git",
            ]
        )
        .status
        .success()
    );
    assert!(
        git(
            root,
            &[
                "remote",
                "set-url",
                "--push",
                "origin",
                remote.path().to_str().unwrap(),
            ]
        )
        .status
        .success()
    );

    let output = run(root, &["bump", "--publish", "patch"]);
    let stdout = assert_success(&output);
    assert!(stdout.contains("Pushed"), "{stdout}");
    assert_eq!(
        String::from_utf8_lossy(&git(remote.path(), &["tag", "-l"]).stdout).trim(),
        "v1.2.4"
    );
}

#[test]
fn tag_dry_run_reports_without_creating_a_tag() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "0.5.0");
    init_git_repo(root);
    commit_all(root, "initial");

    let output = run(root, &["tag", "--dry-run"]);
    let stdout = assert_success(&output);
    assert!(
        stdout.contains("[DRY RUN] Would create git tag: v0.5.0"),
        "{stdout}"
    );
    let tags = git(root, &["tag", "-l"]);
    assert!(
        String::from_utf8_lossy(&tags.stdout).trim().is_empty(),
        "dry-run must not create a tag"
    );
}

#[test]
fn tag_creates_an_annotated_tag_on_a_clean_tree() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "0.5.0");
    init_git_repo(root);
    commit_all(root, "initial");

    let output = run(root, &["tag"]);
    let stdout = assert_success(&output);
    assert!(
        stdout.contains("Successfully created git tag 'v0.5.0'"),
        "{stdout}"
    );
    let tags = git(root, &["tag", "-l"]);
    assert_eq!(String::from_utf8_lossy(&tags.stdout).trim(), "v0.5.0");
}

#[test]
fn tag_refuses_a_dirty_tree_without_allow_dirty() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "0.5.0");
    init_git_repo(root);
    commit_all(root, "initial");
    fs::write(root.join("dirty.txt"), "uncommitted").unwrap();

    let output = run(root, &["tag"]);
    let stderr = assert_failure(&output);
    assert!(stderr.contains("uncommitted changes"), "{stderr}");
    let tags = git(root, &["tag", "-l"]);
    assert!(
        String::from_utf8_lossy(&tags.stdout).trim().is_empty(),
        "must not tag a dirty tree"
    );
}

#[test]
fn tag_reports_version_validation_errors() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();

    let output = run(root, &["tag"]);
    let stderr = assert_failure(&output);
    assert!(
        stderr.contains("Missing workspace.package.version"),
        "{stderr}"
    );
}

#[test]
fn tag_allows_a_dirty_tree_with_allow_dirty() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "0.5.0");
    init_git_repo(root);
    commit_all(root, "initial");
    fs::write(root.join("dirty.txt"), "uncommitted").unwrap();

    let output = run(root, &["tag", "--allow-dirty"]);
    let stdout = assert_success(&output);
    assert!(
        stdout.contains("Successfully created git tag 'v0.5.0'"),
        "{stdout}"
    );
}

#[test]
fn tag_push_sends_the_tag_to_the_configured_remote() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "0.7.0");
    init_git_repo(root);
    commit_all(root, "initial");

    let remote_dir = tempfile::tempdir().unwrap();
    assert!(
        git(remote_dir.path(), &["init", "--bare", "--quiet"])
            .status
            .success()
    );
    assert!(
        git(
            root,
            &[
                "remote",
                "add",
                "origin",
                remote_dir.path().to_str().unwrap(),
            ]
        )
        .status
        .success()
    );

    let output = run(root, &["tag", "--push"]);
    let stdout = assert_success(&output);
    assert!(
        stdout.contains("Successfully created git tag 'v0.7.0'"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Successfully pushed tag 'v0.7.0' to origin"),
        "{stdout}"
    );

    let remote_tags = git(remote_dir.path(), &["tag", "-l"]);
    assert_eq!(
        String::from_utf8_lossy(&remote_tags.stdout).trim(),
        "v0.7.0"
    );
}

#[cfg(unix)]
#[test]
fn tag_reports_git_spawn_errors_after_validation() {
    let dir = tempfile::tempdir().unwrap();
    write_workspace_manifest(dir.path(), "0.8.0");
    let bin = tempfile::tempdir().unwrap();
    write_executable(
        &bin.path().join("git"),
        "case \"$1\" in status) /bin/rm \"$0\"; exit 0 ;; *) exit 0 ;; esac",
    );
    let output = run_with_exact_path(dir.path(), &["tag", "--allow-dirty"], bin.path());
    let stderr = assert_failure(&output);
    assert!(
        stderr.contains("No such file") || stderr.contains("not found"),
        "{stderr}"
    );

    let dir = tempfile::tempdir().unwrap();
    write_workspace_manifest(dir.path(), "0.8.1");
    let bin = tempfile::tempdir().unwrap();
    write_executable(
        &bin.path().join("git"),
        "case \"$1\" in status) exit 0 ;; tag) /bin/rm \"$0\"; exit 0 ;; *) exit 0 ;; esac",
    );
    let output = run_with_exact_path(dir.path(), &["tag", "--push"], bin.path());
    let stderr = assert_failure(&output);
    assert!(
        stderr.contains("No such file") || stderr.contains("not found"),
        "{stderr}"
    );
}

#[cfg(unix)]
#[test]
fn publish_reports_branch_and_tag_push_errors() {
    let branch_failure = tempfile::tempdir().unwrap();
    write_workspace_manifest(branch_failure.path(), "1.2.3");
    let bin = tempfile::tempdir().unwrap();
    write_executable(
        &bin.path().join("git"),
        r#"
case "$1" in
  status) exit 0 ;;
  rev-parse) exit 1 ;;
  symbolic-ref) printf 'main'; exit 0 ;;
  remote) printf 'https://github.com/acme/frank.git'; exit 0 ;;
  push) exit 1 ;;
  *) exit 0 ;;
esac
"#,
    );
    write_executable(&bin.path().join("cargo"), "exit 0");
    let output = run_with_exact_path(
        branch_failure.path(),
        &["bump", "--publish", "patch"],
        bin.path(),
    );
    let stderr = assert_failure(&output);
    assert!(stderr.contains("failed to push branch main"), "{stderr}");

    let tag_failure = tempfile::tempdir().unwrap();
    write_workspace_manifest(tag_failure.path(), "1.2.3");
    let bin = tempfile::tempdir().unwrap();
    write_executable(
        &bin.path().join("git"),
        r#"
case "$1" in
  status) exit 0 ;;
  rev-parse) exit 1 ;;
  symbolic-ref) printf 'main'; exit 0 ;;
  remote) printf 'https://github.com/acme/frank.git'; exit 0 ;;
  push)
    if [ ! -e "$0.push" ]; then : > "$0.push"; exit 0; fi
    exit 1 ;;
  *) exit 0 ;;
esac
"#,
    );
    write_executable(&bin.path().join("cargo"), "exit 0");
    let output = run_with_exact_path(
        tag_failure.path(),
        &["bump", "--publish", "patch"],
        bin.path(),
    );
    let stderr = assert_failure(&output);
    assert!(stderr.contains("failed to push tag v1.2.4"), "{stderr}");
}

#[test]
fn checksums_writes_sha256sums_for_recognized_dist_artifacts_only() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "1.0.0");
    let dist = root.join("dist");
    fs::create_dir_all(&dist).unwrap();
    fs::write(dist.join("frank-x86_64.tar.gz"), b"artifact-bytes").unwrap();
    fs::write(dist.join("notes.txt"), b"not a release artifact").unwrap();

    let output = run(root, &["checksums"]);
    let stdout = assert_success(&output);
    assert!(stdout.contains("Wrote checksums to"), "{stdout}");

    let sums = fs::read_to_string(dist.join("SHA256SUMS")).unwrap();
    assert!(sums.contains("frank-x86_64.tar.gz"), "{sums}");
    assert!(!sums.contains("notes.txt"), "{sums}");

    let expected_hash = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"artifact-bytes");
        format!("{:x}", hasher.finalize())
    };
    assert!(sums.contains(&expected_hash), "{sums}");
}

#[test]
fn checksums_fails_closed_without_a_dist_directory() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "1.0.0");
    let output = run(root, &["checksums"]);
    let stderr = assert_failure(&output);
    assert!(stderr.contains("does not exist"), "{stderr}");
}

#[test]
fn checksums_fails_closed_with_no_matching_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "1.0.0");
    fs::create_dir_all(root.join("dist")).unwrap();
    fs::write(root.join("dist/readme.txt"), "not an artifact").unwrap();

    let output = run(root, &["checksums"]);
    let stderr = assert_failure(&output);
    assert!(stderr.contains("No release artifacts"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn checksums_reports_artifact_read_filename_and_output_errors() {
    let unreadable = tempfile::tempdir().unwrap();
    write_workspace_manifest(unreadable.path(), "1.0.0");
    let dist = unreadable.path().join("dist");
    fs::create_dir_all(&dist).unwrap();
    let artifact = dist.join("artifact.zip");
    fs::write(&artifact, b"artifact").unwrap();
    set_mode(&artifact, 0o000);
    let output = run(unreadable.path(), &["checksums"]);
    let stderr = assert_failure(&output);
    assert!(stderr.contains("Permission denied"), "{stderr}");
    set_mode(&artifact, 0o644);

    let blocked_output = tempfile::tempdir().unwrap();
    write_workspace_manifest(blocked_output.path(), "1.0.0");
    let dist = blocked_output.path().join("dist");
    fs::create_dir_all(&dist).unwrap();
    fs::write(dist.join("artifact.zip"), b"artifact").unwrap();
    fs::create_dir(dist.join("SHA256SUMS")).unwrap();
    let output = run(blocked_output.path(), &["checksums"]);
    let stderr = assert_failure(&output);
    assert!(stderr.contains("Is a directory"), "{stderr}");
}

#[cfg(target_os = "linux")]
#[test]
fn checksums_rejects_a_non_utf8_artifact_name() {
    let dir = tempfile::tempdir().unwrap();
    write_workspace_manifest(dir.path(), "1.0.0");
    let dist = dir.path().join("dist");
    fs::create_dir_all(&dist).unwrap();
    let name = OsString::from_vec(b"artifact-\xff.zip".to_vec());
    fs::write(dist.join(name), b"artifact").unwrap();

    let output = run(dir.path(), &["checksums"]);
    let stderr = assert_failure(&output);
    assert!(stderr.contains("Invalid filename"), "{stderr}");
}
