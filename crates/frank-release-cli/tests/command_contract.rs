use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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
