use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use sha2::{Digest, Sha256};
use toml_edit::{DocumentMut, value};

#[derive(Parser)]
#[command(name = "frank-release", about = "CLI tool for managing Frank releases")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show release status and verify version synchronization across manifests.
    Status,
    /// Validate that all version manifests are synchronized and report release readiness.
    Verify,
    /// Bump workspace version (e.g., 0.2.0, patch, minor, major).
    Bump {
        /// Target semver version (e.g. "0.2.0") or bump type ("patch", "minor", "major").
        ///
        /// When `--publish` is used without a target, the current workspace
        /// version is released as-is.
        target: Option<String>,
        /// Commit, tag, push, and let the tag-triggered GitHub workflow publish the release.
        #[arg(long)]
        publish: bool,
        /// Show the complete release plan without changing files or git refs.
        #[arg(long)]
        dry_run: bool,
        /// Stop after the local commit and tag; do not push to origin.
        #[arg(long)]
        no_push: bool,
    },
    /// Create a git release tag (e.g., v0.1.0) for the current workspace version.
    Tag {
        /// Allow creating a tag even if the git working tree has uncommitted changes.
        #[arg(long)]
        allow_dirty: bool,
        /// Show what would be tagged without creating the tag.
        #[arg(long)]
        dry_run: bool,
        /// Push the created tag to origin after tagging.
        #[arg(long)]
        push: bool,
    },
    /// Generate SHA256SUMS for release artifacts in dist/.
    Checksums,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let root = repo_root()?;

    match cli.command {
        Command::Status => status(&root),
        Command::Verify => verify(&root),
        Command::Bump {
            target,
            publish,
            dry_run,
            no_push,
        } => {
            if publish {
                publish_release(&root, target.as_deref(), dry_run, no_push)
            } else {
                if dry_run || no_push {
                    bail!("--dry-run and --no-push require --publish");
                }
                let target = target.context("bump requires a target unless --publish is used")?;
                bump(&root, &target)
            }
        }
        Command::Tag {
            allow_dirty,
            dry_run,
            push,
        } => tag(&root, allow_dirty, dry_run, push),
        Command::Checksums => checksums(&root),
    }
}

/// `apps/frank-gui` no longer exists (the frank-gui -> iced migration
/// removed the Tauri app it used to sentinel on -- see the plan). A
/// workspace root is, definitionally, wherever `[workspace]` lives; that is
/// the sentinel now, and it needs no GUI-specific knowledge at all.
fn repo_root() -> Result<PathBuf> {
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let path = PathBuf::from(manifest_dir);
        if let Some(parent) = path.parent() {
            if let Some(root) = parent.parent() {
                if is_workspace_root(&root.join("Cargo.toml")) {
                    return Ok(root.to_path_buf());
                }
            }
            let candidate = path.join("../..");
            if is_workspace_root(&candidate.join("Cargo.toml")) {
                return Ok(candidate.canonicalize()?);
            }
        }
    }

    let cwd = std::env::current_dir()?;
    let mut current = cwd.as_path();
    loop {
        if is_workspace_root(&current.join("Cargo.toml")) {
            return Ok(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => bail!("Could not locate repository root from {}", cwd.display()),
        }
    }
}

fn is_workspace_root(cargo_toml: &Path) -> bool {
    let Ok(content) = fs::read_to_string(cargo_toml) else {
        return false;
    };
    toml::from_str::<toml::Value>(&content)
        .ok()
        .is_some_and(|doc| doc.get("workspace").is_some())
}

struct VersionInfo {
    cargo: String,
}

fn get_version_info(root: &Path) -> Result<VersionInfo> {
    let cargo_path = root.join("Cargo.toml");
    let cargo_content = fs::read_to_string(&cargo_path)
        .with_context(|| format!("reading {}", cargo_path.display()))?;
    let cargo_toml: toml::Value = toml::from_str(&cargo_content)?;
    let cargo = cargo_toml
        .get("workspace")
        .and_then(|v| v.get("package"))
        .and_then(|v| v.get("version"))
        .and_then(toml::Value::as_str)
        .context("Missing workspace.package.version in root Cargo.toml")?
        .to_string();

    Ok(VersionInfo { cargo })
}

fn status(root: &Path) -> Result<()> {
    let info = get_version_info(root)?;
    println!("=== Frank Release Status ===");
    println!("Repository Root: {}", root.display());
    println!("Cargo Workspace Version: {}", info.cargo);

    let git_dirty = is_git_dirty(root);
    println!(
        "Git Tree State:          {}",
        if git_dirty {
            "DIRTY (uncommitted changes)"
        } else {
            "CLEAN"
        }
    );

    if let Ok(latest_tag) = get_latest_git_tag(root) {
        println!("Latest Git Tag:          {latest_tag}");
    } else {
        println!("Latest Git Tag:          (none)");
    }

    Ok(())
}

fn verify(root: &Path) -> Result<()> {
    let info = get_version_info(root)?;
    parse_semver(&info.cargo)
        .with_context(|| format!("workspace version '{}' is not valid semver", info.cargo))?;
    println!("Verification passed: version {} is valid.", info.cargo);
    Ok(())
}

fn parse_semver(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.strip_prefix('v').unwrap_or(s);
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let major = parts[0].parse::<u64>().ok()?;
    let minor = parts[1].parse::<u64>().ok()?;
    let patch = parts[2].parse::<u64>().ok()?;
    Some((major, minor, patch))
}

fn bump(root: &Path, target: &str) -> Result<()> {
    let info = get_version_info(root)?;
    let new_version = resolve_version(&info.cargo, Some(target))?;
    println!("Bumping version: {} -> {new_version}", info.cargo);

    let changed = write_version_manifests(root, &new_version)?;
    for path in changed {
        println!(" Updated {}", path.display());
    }

    println!("Version successfully bumped to {new_version}!");
    Ok(())
}

fn resolve_version(current: &str, target: Option<&str>) -> Result<String> {
    let (major, minor, patch) = parse_semver(current)
        .with_context(|| format!("Current version '{current}' is not valid semver"))?;

    let Some(target) = target else {
        return Ok(current.to_string());
    };

    match target.to_lowercase().as_str() {
        "patch" => Ok(format!("{major}.{minor}.{}", patch + 1)),
        "minor" => Ok(format!("{major}.{}.0", minor + 1)),
        "major" => Ok(format!("{}.0.0", major + 1)),
        other => {
            if parse_semver(other).is_none() {
                bail!(
                    "Invalid target version '{other}'. Expected semver (e.g. '0.2.0') or bump keyword ('patch', 'minor', 'major')."
                );
            }
            Ok(other.strip_prefix('v').unwrap_or(other).to_string())
        }
    }
}

fn workspace_member_manifests(root: &Path) -> Result<Vec<PathBuf>> {
    let mut manifests = Vec::new();
    let crates_dir = root.join("crates");
    if crates_dir.is_dir() {
        let mut entries: Vec<_> = fs::read_dir(&crates_dir)?.collect::<std::io::Result<_>>()?;
        entries.sort_by_key(std::fs::DirEntry::path);
        for entry in entries {
            let manifest = entry.path().join("Cargo.toml");
            if manifest.is_file() {
                manifests.push(manifest);
            }
        }
    }

    let xtask_manifest = root.join("xtask/Cargo.toml");
    if xtask_manifest.is_file() {
        manifests.push(xtask_manifest);
    }
    Ok(manifests)
}

fn workspace_member_dirs(root: &Path) -> Result<Vec<PathBuf>> {
    Ok(workspace_member_manifests(root)?
        .into_iter()
        .filter_map(|path| path.parent().map(Path::to_path_buf))
        .filter_map(|path| path.canonicalize().ok())
        .collect())
}

fn is_workspace_path(root: &Path, manifest_path: &Path, dependency_path: &str) -> bool {
    let Some(parent) = manifest_path.parent() else {
        return false;
    };
    let Ok(dependency_dir) = parent.join(dependency_path).canonicalize() else {
        return false;
    };

    workspace_member_dirs(root)
        .map(|members| members.into_iter().any(|member| member == dependency_dir))
        .unwrap_or(false)
}

fn update_path_dependency_versions(
    root: &Path,
    manifest_path: &Path,
    document: &mut DocumentMut,
    new_version: &str,
) -> bool {
    let mut changed = false;
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let Some(table) = document
            .get_mut(section)
            .and_then(toml_edit::Item::as_table_like_mut)
        else {
            continue;
        };

        for (_, dependency) in table.iter_mut() {
            let Some(path) = dependency
                .as_table_like()
                .and_then(|dependency| dependency.get("path"))
                .and_then(toml_edit::Item::as_str)
                .map(str::to_owned)
            else {
                continue;
            };

            if !is_workspace_path(root, manifest_path, &path) {
                continue;
            }

            let Some(dependency) = dependency.as_table_like_mut() else {
                continue;
            };
            if dependency.get("version").is_some() {
                dependency.insert("version", value(new_version));
                changed = true;
            }
        }
    }
    changed
}

fn write_version_manifests(root: &Path, new_version: &str) -> Result<Vec<PathBuf>> {
    let mut changed_paths = Vec::new();
    let cargo_path = root.join("Cargo.toml");
    let cargo_content = fs::read_to_string(&cargo_path)?;
    let mut cargo_doc = cargo_content.parse::<DocumentMut>()?;
    cargo_doc["workspace"]["package"]["version"] = value(new_version);
    let rendered = cargo_doc.to_string();
    if rendered != cargo_content {
        fs::write(&cargo_path, rendered)?;
        changed_paths.push(cargo_path);
    }

    // Workspace crates inherit their package version, but their path
    // dependencies still carry semver constraints. Keep those constraints in
    // lockstep too, otherwise a bump from 0.1.x to 0.2.x makes Cargo reject
    // the workspace before the release workflow can build it.
    for manifest_path in workspace_member_manifests(root)? {
        let content = fs::read_to_string(&manifest_path)?;
        let mut document = content.parse::<DocumentMut>()?;
        if !update_path_dependency_versions(root, &manifest_path, &mut document, new_version) {
            continue;
        }
        let rendered = document.to_string();
        if rendered != content {
            fs::write(&manifest_path, rendered)?;
            changed_paths.push(manifest_path);
        }
    }

    Ok(changed_paths)
}

fn git_status_paths(root: &Path) -> Result<Vec<String>> {
    let output = ProcessCommand::new("git")
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .current_dir(root)
        .output()
        .context("running git status")?;
    if !output.status.success() {
        bail!("git status failed");
    }

    let stdout = String::from_utf8(output.stdout).context("git status output is not UTF-8")?;
    stdout
        .lines()
        .map(|line| {
            line.get(3..)
                .map(str::trim)
                .filter(|path| !path.is_empty())
                .map(str::to_owned)
                .with_context(|| format!("unexpected git status line: {line:?}"))
        })
        .collect()
}

fn is_git_dirty(root: &Path) -> bool {
    git_status_paths(root)
        .map(|paths| !paths.is_empty())
        .unwrap_or(true)
}

fn git_current_branch(root: &Path) -> Result<String> {
    let output = ProcessCommand::new("git")
        .args(["symbolic-ref", "--quiet", "--short", "HEAD"])
        .current_dir(root)
        .output()
        .context("reading current git branch")?;
    if !output.status.success() {
        bail!("release requires a named git branch; detached HEAD is not supported");
    }
    let branch = String::from_utf8(output.stdout)?.trim().to_string();
    if branch.is_empty() {
        bail!("release requires a non-empty current git branch");
    }
    Ok(branch)
}

fn git_remote_url(root: &Path, remote: &str) -> Result<String> {
    let output = ProcessCommand::new("git")
        .args(["remote", "get-url", remote])
        .current_dir(root)
        .output()
        .with_context(|| format!("reading git remote {remote}"))?;
    if !output.status.success() {
        bail!("git remote '{remote}' is not configured");
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

fn github_release_url(remote_url: &str, tag_name: &str) -> Option<String> {
    let remote = remote_url
        .trim()
        .strip_prefix("https://github.com/")
        .or_else(|| remote_url.trim().strip_prefix("http://github.com/"))
        .or_else(|| remote_url.trim().strip_prefix("git@github.com:"))
        .or_else(|| remote_url.trim().strip_prefix("ssh://git@github.com/"))?;
    let repository = remote.trim_end_matches('/').trim_end_matches(".git");
    let mut parts = repository.split('/');
    let owner = parts.next()?;
    let name = parts.next()?;
    if owner.is_empty() || name.is_empty() || parts.next().is_some() {
        return None;
    }
    Some(format!(
        "https://github.com/{owner}/{name}/releases/tag/{tag_name}"
    ))
}

fn local_tag_exists(root: &Path, tag_name: &str) -> Result<bool> {
    let output = ProcessCommand::new("git")
        .args([
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/tags/{tag_name}"),
        ])
        .current_dir(root)
        .output()
        .context("checking existing git tag")?;
    Ok(output.status.success())
}

fn refresh_lockfile(root: &Path) -> Result<()> {
    let status = ProcessCommand::new("cargo")
        .args(["metadata", "--format-version", "1"])
        .current_dir(root)
        .stdout(Stdio::null())
        .status()
        .context("refreshing Cargo.lock with cargo metadata")?;
    if !status.success() {
        bail!("cargo metadata failed while preparing the release");
    }

    let status = ProcessCommand::new("cargo")
        .args(["metadata", "--locked", "--format-version", "1"])
        .current_dir(root)
        .stdout(Stdio::null())
        .status()
        .context("validating Cargo.lock")?;
    if !status.success() {
        bail!("Cargo.lock is not synchronized after the version bump");
    }
    Ok(())
}

fn commit_paths(root: &Path, paths: &[String], message: &str) -> Result<()> {
    let mut args = vec!["add".to_string(), "--".to_string()];
    args.extend(paths.iter().cloned());
    let status = ProcessCommand::new("git")
        .args(&args)
        .current_dir(root)
        .status()
        .context("staging release files")?;
    if !status.success() {
        bail!("failed to stage release files");
    }

    let status = ProcessCommand::new("git")
        .args(["commit", "-m", message])
        .current_dir(root)
        .status()
        .context("creating release commit")?;
    if !status.success() {
        bail!("failed to create release commit");
    }
    Ok(())
}

fn create_tag(root: &Path, tag_name: &str) -> Result<()> {
    let status = ProcessCommand::new("git")
        .args(["tag", "-a", tag_name, "-m", &format!("Release {tag_name}")])
        .current_dir(root)
        .status()
        .context("creating release tag")?;
    if !status.success() {
        bail!("failed to create release tag '{tag_name}'");
    }
    Ok(())
}

fn push_ref(root: &Path, remote: &str, reference: &str, description: &str) -> Result<()> {
    let status = ProcessCommand::new("git")
        .args(["push", remote, reference])
        .current_dir(root)
        .status()
        .with_context(|| format!("pushing {description}"))?;
    if !status.success() {
        bail!("failed to push {description} to {remote}");
    }
    Ok(())
}

fn publish_release(root: &Path, target: Option<&str>, dry_run: bool, no_push: bool) -> Result<()> {
    if is_git_dirty(root) {
        bail!("release requires a clean git working tree; commit or stash existing changes first");
    }

    let info = get_version_info(root)?;
    let new_version = resolve_version(&info.cargo, target)?;
    let tag_name = format!("v{new_version}");
    if local_tag_exists(root, &tag_name)? {
        bail!("git tag '{tag_name}' already exists");
    }

    let branch = git_current_branch(root)?;
    let remote_url = git_remote_url(root, "origin").ok();
    let release_url = remote_url
        .as_deref()
        .and_then(|url| github_release_url(url, &tag_name));

    println!("=== Frank GitHub Release ===");
    println!("Version:                 {} -> {new_version}", info.cargo);
    println!("Commit:                  chore(release): {tag_name}");
    println!("Tag:                     {tag_name}");
    println!("Branch:                  {branch}");
    if no_push {
        println!("Push:                    disabled (--no-push)");
    } else {
        println!("Push:                    origin/{branch} and {tag_name}");
    }
    if let Some(url) = &release_url {
        println!("GitHub Release:          {url}");
    } else {
        println!("GitHub Release:          URL unavailable from origin");
    }

    if dry_run {
        println!("[DRY RUN] No files, commits, tags, or pushes were changed.");
        return Ok(());
    }

    if !no_push && release_url.is_none() {
        bail!("origin must point to github.com before publishing a GitHub Release");
    }

    let changed_paths = write_version_manifests(root, &new_version)?;
    refresh_lockfile(root)?;

    let status_paths = git_status_paths(root)?;
    let mut allowed_paths: Vec<String> = changed_paths
        .iter()
        .map(|path| {
            path.strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .trim_start_matches('/')
                .to_string()
        })
        .collect();
    if status_paths.iter().any(|path| path == "Cargo.lock") {
        allowed_paths.push("Cargo.lock".to_string());
    }
    allowed_paths.sort();
    allowed_paths.dedup();

    let unexpected: Vec<_> = status_paths
        .iter()
        .filter(|path| !allowed_paths.contains(path))
        .cloned()
        .collect();
    if !unexpected.is_empty() {
        bail!(
            "release preparation changed unexpected files: {}",
            unexpected.join(", ")
        );
    }

    if !status_paths.is_empty() {
        commit_paths(root, &status_paths, &format!("chore(release): {tag_name}"))?;
        println!("Created release commit for {tag_name}");
    } else {
        println!("Workspace version already matches {new_version}; no release commit needed");
    }

    create_tag(root, &tag_name)?;
    println!("Created release tag {tag_name}");

    if no_push {
        println!("Release prepared locally. Push the branch and tag to trigger GitHub Actions.");
        return Ok(());
    }

    push_ref(
        root,
        "origin",
        &format!("HEAD:{branch}"),
        &format!("branch {branch}"),
    )?;
    push_ref(root, "origin", &tag_name, &format!("tag {tag_name}"))?;
    println!("Pushed {branch} and {tag_name} to origin");
    println!(
        "GitHub Actions will build the artifacts and publish the release at {}",
        release_url.unwrap()
    );
    Ok(())
}

fn get_latest_git_tag(root: &Path) -> Result<String> {
    let output = ProcessCommand::new("git")
        .args(["describe", "--tags", "--abbrev=0"])
        .current_dir(root)
        .output()?;
    if output.status.success() {
        let tag = String::from_utf8(output.stdout)?.trim().to_string();
        if !tag.is_empty() {
            return Ok(tag);
        }
    }
    bail!("No git tags found")
}

fn tag(root: &Path, allow_dirty: bool, dry_run: bool, push: bool) -> Result<()> {
    verify(root)?;

    let info = get_version_info(root)?;
    let tag_name = format!("v{}", info.cargo);

    if is_git_dirty(root) && !allow_dirty {
        bail!(
            "Git working tree has uncommitted changes! Commit or stash your changes, or pass --allow-dirty."
        );
    }

    if dry_run {
        println!("[DRY RUN] Would create git tag: {tag_name}");
        if push {
            println!("[DRY RUN] Would push git tag '{tag_name}' to origin");
        }
        return Ok(());
    }

    println!("Creating git tag: {tag_name}");
    let status = ProcessCommand::new("git")
        .args(["tag", "-a", &tag_name, "-m", &format!("Release {tag_name}")])
        .current_dir(root)
        .status()?;

    if !status.success() {
        bail!("Failed to create git tag '{tag_name}'");
    }

    println!("Successfully created git tag '{tag_name}'");

    if push {
        println!("Pushing tag '{tag_name}' to origin...");
        let push_status = ProcessCommand::new("git")
            .args(["push", "origin", &tag_name])
            .current_dir(root)
            .status()?;
        if !push_status.success() {
            bail!("Failed to push git tag '{tag_name}' to origin");
        }
        println!("Successfully pushed tag '{tag_name}' to origin");
    }

    Ok(())
}

fn checksums(root: &Path) -> Result<()> {
    let dist_dir = root.join("dist");
    if !dist_dir.exists() {
        bail!(
            "Directory {} does not exist. Build release artifacts first.",
            dist_dir.display()
        );
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(&dist_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if matches!(ext, "gz" | "zip" | "dmg" | "msi" | "deb" | "rpm") {
                    entries.push(path);
                }
            }
        }
    }

    if entries.is_empty() {
        bail!("No release artifacts (.tar.gz, .zip, .dmg, .msi, .deb, .rpm) found in dist/");
    }

    entries.sort();

    let mut lines = Vec::new();
    for path in &entries {
        let bytes = fs::read(path)?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let hash = format!("{:x}", hasher.finalize());
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .context("Invalid filename")?;
        lines.push(format!("{hash}  {file_name}"));
    }

    let sums_path = dist_dir.join("SHA256SUMS");
    let content = lines.join("\n") + "\n";
    fs::write(&sums_path, content)?;

    println!("Wrote checksums to {}", sums_path.display());
    for line in lines {
        println!("  {line}");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_semver() {
        assert_eq!(parse_semver("0.1.0"), Some((0, 1, 0)));
        assert_eq!(parse_semver("v1.2.3"), Some((1, 2, 3)));
        assert_eq!(parse_semver("10.20.30"), Some((10, 20, 30)));
        assert_eq!(parse_semver("invalid"), None);
        assert_eq!(parse_semver("1.2"), None);
    }
}
