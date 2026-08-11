use super::*;
#[cfg(unix)]
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::Mutex;

#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use tempfile::{TempDir, tempdir};

static REPO_ROOT_LOCK: Mutex<()> = Mutex::new(());

fn assert_err<T>(result: Result<T>, message: &str) {
    let error = match result {
        Ok(_) => panic!("expected an error containing {message:?}"),
        Err(error) => error,
    };
    assert!(
        error.to_string().contains(message),
        "expected error containing {message:?}, got {error:#}"
    );
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

fn simple_git_workspace(version: &str) -> (TempDir, PathBuf) {
    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    write_workspace_manifest(&root, version);
    init_git_repo(&root);
    commit_all(&root, "initial");
    (dir, root)
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(mode);
    fs::set_permissions(path, permissions).unwrap();
}

#[test]
fn test_parse_semver() {
    assert_eq!(parse_semver("0.1.0"), Some((0, 1, 0)));
    assert_eq!(parse_semver("v1.2.3"), Some((1, 2, 3)));
    assert_eq!(parse_semver("10.20.30"), Some((10, 20, 30)));
    assert_eq!(parse_semver("invalid"), None);
    assert_eq!(parse_semver("1.2"), None);
    assert_eq!(parse_semver("x.2.3"), None);
    assert_eq!(parse_semver("1.x.3"), None);
    assert_eq!(parse_semver("1.2.x"), None);
}

#[cfg(unix)]
#[test]
fn release_file_name_rejects_non_utf8_names() {
    let name = OsString::from_vec(b"artifact-\xff.zip".to_vec());
    assert_err(release_file_name(Path::new(&name)), "Invalid filename");
}

#[test]
fn version_and_manifest_helpers_cover_success_and_rejection_paths() {
    let _repo_root_guard = REPO_ROOT_LOCK.lock().unwrap();
    let dir = tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "1.2.3");

    assert!(repo_root().unwrap().join("Cargo.toml").is_file());
    assert!(is_workspace_root(&root.join("Cargo.toml")));
    assert!(!is_workspace_root(&root.join("missing.toml")));
    fs::write(root.join("invalid.toml"), "not = [valid").unwrap();
    assert!(!is_workspace_root(&root.join("invalid.toml")));
    fs::write(
        root.join("not-a-workspace.toml"),
        "[package]\nname = \"x\"\n",
    )
    .unwrap();
    assert!(!is_workspace_root(&root.join("not-a-workspace.toml")));

    assert_eq!(get_version_info(root).unwrap().cargo, "1.2.3");
    assert_eq!(resolve_version("1.2.3", None).unwrap(), "1.2.3");
    assert_eq!(resolve_version("1.2.3", Some("PATCH")).unwrap(), "1.2.4");
    assert_eq!(resolve_version("1.2.3", Some("minor")).unwrap(), "1.3.0");
    assert_eq!(resolve_version("1.2.3", Some("major")).unwrap(), "2.0.0");
    assert_eq!(resolve_version("1.2.3", Some("v9.8.7")).unwrap(), "9.8.7");
    assert_err(resolve_version("not-semver", None), "not valid semver");
    assert_err(
        resolve_version("1.2.3", Some("not-semver")),
        "Invalid target version",
    );

    let missing = tempdir().unwrap();
    assert_err(get_version_info(missing.path()), "reading");
    fs::write(missing.path().join("Cargo.toml"), "[workspace\n").unwrap();
    assert_err(get_version_info(missing.path()), "TOML parse error");
    fs::write(
        missing.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\n",
    )
    .unwrap();
    assert_err(
        get_version_info(missing.path()),
        "Missing workspace.package.version",
    );
    assert_err(status(missing.path()), "Missing workspace.package.version");
    assert_err(verify(missing.path()), "Missing workspace.package.version");
    assert_err(
        bump(missing.path(), "patch"),
        "Missing workspace.package.version",
    );

    fs::create_dir_all(root.join("crates/a")).unwrap();
    fs::create_dir_all(root.join("crates/b")).unwrap();
    fs::create_dir_all(root.join("crates/no-manifest")).unwrap();
    fs::write(
        root.join("crates/a/Cargo.toml"),
        "[package]\nname = \"a\"\nversion = \"1.2.3\"\n\n[dependencies]\nb = { path = \"../b\", version = \"1.2.3\" }\nexternal = { path = \"../missing\", version = \"1.2.3\" }\nplain = \"1\"\n\n[dev-dependencies]\nwithout-version = { path = \"../b\" }\n\n[build-dependencies]\nbuild-b = { path = \"../b\", version = \"1.2.3\" }\n",
    )
    .unwrap();
    fs::write(
        root.join("crates/b/Cargo.toml"),
        "[package]\nname = \"b\"\nversion = \"1.2.3\"\n",
    )
    .unwrap();
    fs::create_dir_all(root.join("xtask")).unwrap();
    fs::write(
        root.join("xtask/Cargo.toml"),
        "[package]\nname = \"xtask\"\n",
    )
    .unwrap();

    let manifests = workspace_member_manifests(root).unwrap();
    assert_eq!(manifests.len(), 3);
    assert_eq!(workspace_member_dirs(root).unwrap().len(), 3);
    let a_manifest = root.join("crates/a/Cargo.toml");
    assert!(is_workspace_path(root, &a_manifest, "../b"));
    assert!(!is_workspace_path(root, &a_manifest, "../missing"));
    assert!(!is_workspace_path(
        root,
        &PathBuf::from("Cargo.toml"),
        "crates/b"
    ));
    assert!(!is_workspace_path(root, Path::new("/"), "crates/b"));

    let mut document = fs::read_to_string(&a_manifest)
        .unwrap()
        .parse::<DocumentMut>()
        .unwrap();
    assert!(update_path_dependency_versions(
        root,
        &a_manifest,
        &mut document,
        "2.0.0"
    ));
    assert_eq!(
        document["dependencies"]["b"]["version"].as_str(),
        Some("2.0.0")
    );
    assert_eq!(
        document["build-dependencies"]["build-b"]["version"].as_str(),
        Some("2.0.0")
    );

    let changed = write_version_manifests(root, "2.0.0").unwrap();
    assert!(changed.iter().any(|path| path.ends_with("Cargo.toml")));
    assert!(write_version_manifests(root, "2.0.0").unwrap().is_empty());

    let missing_manifest = tempdir().unwrap();
    assert_err(
        write_version_manifests(missing_manifest.path(), "2.0.0"),
        "No such file",
    );
    write_workspace_manifest(missing_manifest.path(), "1.0.0");
    fs::write(missing_manifest.path().join("Cargo.toml"), "[workspace\n").unwrap();
    assert_err(
        write_version_manifests(missing_manifest.path(), "2.0.0"),
        "TOML parse error",
    );

    let invalid_member = tempdir().unwrap();
    write_workspace_manifest(invalid_member.path(), "1.0.0");
    fs::create_dir_all(invalid_member.path().join("crates/bad")).unwrap();
    fs::write(
        invalid_member.path().join("crates/bad/Cargo.toml"),
        "[package\n",
    )
    .unwrap();
    assert_err(
        write_version_manifests(invalid_member.path(), "2.0.0"),
        "TOML parse error",
    );
}

#[cfg(unix)]
#[test]
fn repo_root_reports_a_missing_current_directory() {
    let _repo_root_guard = REPO_ROOT_LOCK.lock().unwrap();
    let previous_manifest_dir = std::env::var_os("CARGO_MANIFEST_DIR");
    // This test must force the fallback path; the lock prevents other unit
    // tests in this binary from observing the temporary environment change.
    unsafe { std::env::remove_var("CARGO_MANIFEST_DIR") };
    let previous_cwd = std::env::current_dir().unwrap();
    let missing_cwd = tempdir().unwrap();
    let missing_cwd_path = missing_cwd.path().to_path_buf();
    std::env::set_current_dir(&missing_cwd_path).unwrap();
    std::fs::remove_dir(&missing_cwd_path).unwrap();

    let result = repo_root();

    std::env::set_current_dir(previous_cwd).unwrap();
    if let Some(manifest_dir) = previous_manifest_dir {
        unsafe { std::env::set_var("CARGO_MANIFEST_DIR", manifest_dir) };
    }
    assert_err(result, "No such file");
}

#[cfg(unix)]
#[test]
fn manifest_filesystem_errors_are_reported() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write_workspace_manifest(root, "1.0.0");
    let crates_dir = root.join("crates");
    fs::create_dir(&crates_dir).unwrap();
    assert_err(
        workspace_member_manifests_from(root, |_: &Path| -> std::io::Result<std::fs::ReadDir> {
            Err(std::io::Error::other("read directory failure"))
        }),
        "read directory failure",
    );
    assert_err(
        workspace_member_manifests_from(root, |_| {
            Ok(std::iter::once(Err(std::io::Error::other(
                "directory entry failure",
            ))))
        }),
        "directory entry failure",
    );
    set_mode(&crates_dir, 0o000);
    assert_err(workspace_member_manifests(root), "Permission denied");
    assert_err(workspace_member_dirs(root), "Permission denied");
    set_mode(&crates_dir, 0o755);

    let root_write = tempdir().unwrap();
    write_workspace_manifest(root_write.path(), "1.0.0");
    let cargo_path = root_write.path().join("Cargo.toml");
    set_mode(&cargo_path, 0o444);
    assert_err(bump(root_write.path(), "patch"), "Permission denied");
    assert_err(
        write_version_manifests(root_write.path(), "2.0.0"),
        "Permission denied",
    );
    set_mode(&cargo_path, 0o644);

    let member_read = tempdir().unwrap();
    write_workspace_manifest(member_read.path(), "1.0.0");
    fs::create_dir_all(member_read.path().join("crates/a")).unwrap();
    let member_manifest = member_read.path().join("crates/a/Cargo.toml");
    fs::write(
        &member_manifest,
        "[package]\nname = \"a\"\nversion = \"1.0.0\"\n",
    )
    .unwrap();
    set_mode(&member_manifest, 0o000);
    assert_err(
        write_version_manifests(member_read.path(), "2.0.0"),
        "Permission denied",
    );
    set_mode(&member_manifest, 0o644);

    let member_write = tempdir().unwrap();
    write_workspace_manifest(member_write.path(), "1.0.0");
    fs::create_dir_all(member_write.path().join("crates/a")).unwrap();
    fs::create_dir_all(member_write.path().join("crates/b")).unwrap();
    let a_manifest = member_write.path().join("crates/a/Cargo.toml");
    fs::write(
        &a_manifest,
        "[package]\nname = \"a\"\nversion = \"1.0.0\"\n\n[dependencies]\nb = { path = \"../b\", version = \"1.0.0\" }\n",
    )
    .unwrap();
    fs::write(
        member_write.path().join("crates/b/Cargo.toml"),
        "[package]\nname = \"b\"\nversion = \"1.0.0\"\n",
    )
    .unwrap();
    set_mode(&a_manifest, 0o444);
    assert_err(
        write_version_manifests(member_write.path(), "2.0.0"),
        "Permission denied",
    );
    set_mode(&a_manifest, 0o644);

    let unreadable_crates = tempdir().unwrap();
    write_workspace_manifest(unreadable_crates.path(), "1.0.0");
    let unreadable_crates_dir = unreadable_crates.path().join("crates");
    fs::create_dir(&unreadable_crates_dir).unwrap();
    set_mode(&unreadable_crates_dir, 0o000);
    assert_err(
        write_version_manifests(unreadable_crates.path(), "2.0.0"),
        "Permission denied",
    );
    set_mode(&unreadable_crates_dir, 0o755);

    let (_publish_dir, publish_root) = simple_git_workspace("1.0.0");
    let cargo_path = publish_root.join("Cargo.toml");
    set_mode(&cargo_path, 0o444);
    assert_err(
        publish_release(&publish_root, Some("patch"), false, true),
        "Permission denied",
    );
    set_mode(&cargo_path, 0o644);

    let checksum_dir = tempdir().unwrap();
    write_workspace_manifest(checksum_dir.path(), "1.0.0");
    let dist = checksum_dir.path().join("dist");
    fs::create_dir_all(&dist).unwrap();
    set_mode(&dist, 0o000);
    assert_err(checksums(checksum_dir.path()), "Permission denied");
    set_mode(&dist, 0o755);

    assert_err(
        checksums_from_entries(
            Path::new("/tmp"),
            std::iter::once(Err(std::io::Error::other("checksum entry failure"))),
        ),
        "checksum entry failure",
    );
}

#[test]
fn git_helpers_cover_status_tags_branches_remotes_and_pushes() {
    let (_dir, root) = simple_git_workspace("1.2.3");

    assert!(git_status_paths(&root).unwrap().is_empty());
    assert!(!is_git_dirty(&root));
    let empty = tempdir().unwrap();
    assert_err(git_status_paths(empty.path()), "git status failed");
    fs::write(root.join("untracked.txt"), "untracked").unwrap();
    fs::write(root.join("Cargo.toml"), "[workspace]\nmembers = []\n").unwrap();
    let paths = git_status_paths(&root).unwrap();
    assert!(paths.iter().any(|path| path == "untracked.txt"));
    assert!(paths.iter().any(|path| path == "Cargo.toml"));
    assert!(is_git_dirty(&root));
    assert_err(
        git_status_paths(Path::new("/definitely/not/a/repository")),
        "git status",
    );
    assert!(is_git_dirty(Path::new("/definitely/not/a/repository")));
    let missing_root = Path::new("/definitely/not/a/repository");
    assert_err(
        git_current_branch(missing_root),
        "reading current git branch",
    );
    assert_err(git_remote_url(missing_root, "origin"), "reading git remote");
    assert_err(
        local_tag_exists(missing_root, "v1.2.3"),
        "checking existing git tag",
    );
    assert_err(refresh_lockfile(missing_root), "refreshing Cargo.lock");
    assert_err(
        commit_paths(missing_root, &["file".to_string()], "commit"),
        "staging release files",
    );
    assert_err(create_tag(missing_root, "v1.2.3"), "creating release tag");
    assert_err(
        push_ref(missing_root, "origin", "HEAD", "ref"),
        "pushing ref",
    );
    assert_err(get_latest_git_tag(missing_root), "No such file");

    commit_all(&root, "changes");
    let branch = git_current_branch(&root).unwrap();
    assert!(!branch.is_empty());
    git(&root, &["checkout", "--quiet", "--detach"]);
    assert_err(git_current_branch(&root), "detached HEAD");
    git(&root, &["checkout", "--quiet", "-"]);

    assert_err(git_remote_url(&root, "origin"), "not configured");
    let remote = tempdir().unwrap();
    assert!(
        git(remote.path(), &["init", "--bare", "--quiet"])
            .status
            .success()
    );
    assert!(
        git(
            &root,
            &["remote", "add", "origin", remote.path().to_str().unwrap()]
        )
        .status
        .success()
    );
    assert_eq!(
        git_remote_url(&root, "origin").unwrap(),
        remote.path().to_str().unwrap()
    );

    assert!(!local_tag_exists(&root, "v1.2.3").unwrap());
    assert_err(get_latest_git_tag(&root), "No git tags found");
    create_tag(&root, "v1.2.3").unwrap();
    assert!(local_tag_exists(&root, "v1.2.3").unwrap());
    assert_eq!(get_latest_git_tag(&root).unwrap(), "v1.2.3");
    assert_err(create_tag(&root, "v1.2.3"), "failed to create release tag");

    assert_err(
        commit_paths(&root, &["missing.txt".to_string()], "missing file"),
        "failed to stage release files",
    );
    assert_err(
        commit_paths(&root, &["Cargo.toml".to_string()], "empty commit"),
        "failed to create release commit",
    );

    let branch = git_current_branch(&root).unwrap();
    push_ref(&root, "origin", &format!("HEAD:{branch}"), "test branch").unwrap();
    assert_err(
        push_ref(&root, "missing", "HEAD", "test ref"),
        "failed to push",
    );

    assert_eq!(
        github_release_url("https://github.com/acme/frank.git", "v1.2.3"),
        Some("https://github.com/acme/frank/releases/tag/v1.2.3".to_string())
    );
    assert_eq!(
        github_release_url("http://github.com/acme/frank.git/", "v1.2.3"),
        Some("https://github.com/acme/frank/releases/tag/v1.2.3".to_string())
    );
    assert_eq!(
        github_release_url("git@github.com:acme/frank.git", "v1.2.3"),
        Some("https://github.com/acme/frank/releases/tag/v1.2.3".to_string())
    );
    assert_eq!(
        github_release_url("ssh://git@github.com/acme/frank.git", "v1.2.3"),
        Some("https://github.com/acme/frank/releases/tag/v1.2.3".to_string())
    );
    for remote in [
        "https://example.com/acme/frank.git",
        "https://github.com/",
        "https://github.com//frank.git",
        "https://github.com/acme/",
        "https://github.com/acme/frank/extra.git",
    ] {
        assert_eq!(github_release_url(remote, "v1.2.3"), None, "{remote}");
    }
}

#[test]
fn publish_release_covers_dry_run_local_commit_existing_tag_and_remote_guards() {
    let (_dir, root) = simple_git_workspace("1.2.3");
    publish_release(&root, None, true, true).unwrap();

    assert!(
        git(
            &root,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/acme/frank.git"
            ]
        )
        .status
        .success()
    );
    publish_release(&root, Some("patch"), true, false).unwrap();

    let dir = tempdir().unwrap();
    let root = dir.path().to_path_buf();
    write_cargo_workspace(&root, "1.2.3");
    init_git_repo(&root);
    commit_all(&root, "initial");
    assert!(
        git(
            &root,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/acme/frank.git"
            ]
        )
        .status
        .success()
    );
    publish_release(&root, Some("patch"), false, true).unwrap();
    assert!(local_tag_exists(&root, "v1.2.4").unwrap());
    assert_err(publish_release(&root, None, true, true), "already exists");

    let (_dir, root) = simple_git_workspace("1.2.3");
    let remote = tempdir().unwrap();
    assert!(
        git(remote.path(), &["init", "--bare", "--quiet"])
            .status
            .success()
    );
    assert!(
        git(
            &root,
            &["remote", "add", "origin", remote.path().to_str().unwrap()]
        )
        .status
        .success()
    );
    assert_err(
        publish_release(&root, Some("patch"), false, false),
        "origin must point to github.com",
    );

    let (_dir, root) = simple_git_workspace("1.2.3");
    fs::write(root.join("dirty.txt"), "dirty").unwrap();
    assert_err(
        publish_release(&root, Some("patch"), true, true),
        "clean git working tree",
    );
}

#[test]
fn publish_release_handles_a_preexisting_lockfile_without_a_release_commit() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write_cargo_workspace(root, "1.2.3");
    refresh_lockfile(root).unwrap();
    init_git_repo(root);
    commit_all(root, "initial");
    assert!(
        git(
            root,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/acme/frank.git"
            ]
        )
        .status
        .success()
    );

    publish_release(root, None, false, true).unwrap();
    assert!(local_tag_exists(root, "v1.2.3").unwrap());
    assert_eq!(
        String::from_utf8_lossy(&git(root, &["log", "-1", "--pretty=%s"]).stdout).trim(),
        "initial"
    );
}

#[test]
fn publish_release_pushes_the_branch_and_tag_to_a_push_url() {
    let dir = tempdir().unwrap();
    let root = dir.path();
    write_cargo_workspace(root, "1.2.3");
    refresh_lockfile(root).unwrap();
    init_git_repo(root);
    commit_all(root, "initial");

    let remote = tempdir().unwrap();
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

    publish_release(root, Some("patch"), false, false).unwrap();
    assert_eq!(
        String::from_utf8_lossy(&git(remote.path(), &["tag", "-l"]).stdout).trim(),
        "v1.2.4"
    );
}

#[test]
fn tag_and_checksum_helpers_cover_direct_paths() {
    let (_dir, root) = simple_git_workspace("0.5.0");
    tag(&root, false, true, true).unwrap();
    assert!(!local_tag_exists(&root, "v0.5.0").unwrap());
    tag(&root, false, false, false).unwrap();
    assert!(local_tag_exists(&root, "v0.5.0").unwrap());
    assert_err(tag(&root, false, false, false), "Failed to create git tag");

    let (_dir, root) = simple_git_workspace("0.6.0");
    assert_err(tag(&root, false, false, true), "Failed to push git tag");

    let invalid_metadata = tempdir().unwrap();
    write_workspace_manifest(invalid_metadata.path(), "1.0.0");
    assert_err(
        refresh_lockfile(invalid_metadata.path()),
        "cargo metadata failed",
    );

    let checksums_root = tempdir().unwrap();
    write_workspace_manifest(checksums_root.path(), "1.0.0");
    let dist = checksums_root.path().join("dist");
    fs::create_dir_all(&dist).unwrap();
    fs::create_dir_all(dist.join("nested")).unwrap();
    fs::write(dist.join("artifact.zip"), b"artifact").unwrap();
    fs::write(dist.join("ignored.txt"), b"ignored").unwrap();
    fs::write(dist.join("no-extension"), b"ignored").unwrap();
    checksums(checksums_root.path()).unwrap();
    let sums = fs::read_to_string(dist.join("SHA256SUMS")).unwrap();
    assert!(sums.contains("artifact.zip"));
    assert!(!sums.contains("ignored.txt"));
}
