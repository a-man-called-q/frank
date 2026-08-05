use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use sha2::{Digest, Sha256};
use toml_edit::{value, DocumentMut};

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
        target: String,
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
        Command::Bump { target } => bump(&root, &target),
        Command::Tag {
            allow_dirty,
            dry_run,
            push,
        } => tag(&root, allow_dirty, dry_run, push),
        Command::Checksums => checksums(&root),
    }
}

fn repo_root() -> Result<PathBuf> {
    if let Ok(manifest_dir) = std::env::var("CARGO_MANIFEST_DIR") {
        let path = PathBuf::from(manifest_dir);
        if let Some(parent) = path.parent() {
            if let Some(root) = parent.parent() {
                if root.join("Cargo.toml").exists() {
                    return Ok(root.to_path_buf());
                }
            }
            if path.join("../../Cargo.toml").exists() {
                return Ok(path.join("../..").canonicalize()?);
            }
        }
    }

    let cwd = std::env::current_dir()?;
    let mut current = cwd.as_path();
    loop {
        if current.join("Cargo.toml").exists() && current.join("apps/frank-gui").exists() {
            return Ok(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => bail!("Could not locate repository root from {}", cwd.display()),
        }
    }
}

struct VersionInfo {
    cargo: String,
    package_json: String,
    tauri_conf: String,
    synced: bool,
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

    let pkg_path = root.join("apps/frank-gui/package.json");
    let pkg_content = fs::read_to_string(&pkg_path)
        .with_context(|| format!("reading {}", pkg_path.display()))?;
    let pkg_json: serde_json::Value = serde_json::from_str(&pkg_content)?;
    let package_json = pkg_json
        .get("version")
        .and_then(serde_json::Value::as_str)
        .context("Missing version in package.json")?
        .to_string();

    let tauri_path = root.join("apps/frank-gui/src-tauri/tauri.conf.json");
    let tauri_content = fs::read_to_string(&tauri_path)
        .with_context(|| format!("reading {}", tauri_path.display()))?;
    let tauri_json: serde_json::Value = serde_json::from_str(&tauri_content)?;
    let tauri_conf = tauri_json
        .get("version")
        .and_then(serde_json::Value::as_str)
        .context("Missing version in tauri.conf.json")?
        .to_string();

    let synced = cargo == package_json && cargo == tauri_conf;

    Ok(VersionInfo {
        cargo,
        package_json,
        tauri_conf,
        synced,
    })
}

fn status(root: &Path) -> Result<()> {
    let info = get_version_info(root)?;
    println!("=== Frank Release Status ===");
    println!("Repository Root: {}", root.display());
    println!("Cargo Workspace Version: {}", info.cargo);
    println!("package.json Version:    {}", info.package_json);
    println!("tauri.conf.json Version: {}", info.tauri_conf);

    if info.synced {
        println!("Version Sync:            OK (all match {})", info.cargo);
    } else {
        println!("Version Sync:            MISMATCH!");
    }

    let git_dirty = is_git_dirty(root);
    println!(
        "Git Tree State:          {}",
        if git_dirty { "DIRTY (uncommitted changes)" } else { "CLEAN" }
    );

    if let Ok(latest_tag) = get_latest_git_tag(root) {
        println!("Latest Git Tag:          {}", latest_tag);
    } else {
        println!("Latest Git Tag:          (none)");
    }

    Ok(())
}

fn verify(root: &Path) -> Result<()> {
    let info = get_version_info(root)?;
    if !info.synced {
        bail!(
            "Version mismatch detected!\n  Cargo.toml: {}\n  package.json: {}\n  tauri.conf.json: {}",
            info.cargo, info.package_json, info.tauri_conf
        );
    }
    println!("Verification passed: version {} is synchronized.", info.cargo);
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
    let current_version = &info.cargo;
    let (major, minor, patch) = parse_semver(current_version).with_context(|| {
        format!("Current version '{}' is not valid semver", current_version)
    })?;

    let new_version = match target.to_lowercase().as_str() {
        "patch" => format!("{}.{}.{}", major, minor, patch + 1),
        "minor" => format!("{}.{}.0", major, minor + 1),
        "major" => format!("{}.0.0", major + 1),
        other => {
            if parse_semver(other).is_none() {
                bail!(
                    "Invalid target version '{}'. Expected semver (e.g. '0.2.0') or bump keyword ('patch', 'minor', 'major').",
                    other
                );
            }
            other.strip_prefix('v').unwrap_or(other).to_string()
        }
    };

    println!("Bumping version: {} -> {}", current_version, new_version);

    // 1. Update Cargo.toml
    let cargo_path = root.join("Cargo.toml");
    let cargo_content = fs::read_to_string(&cargo_path)?;
    let mut cargo_doc = cargo_content.parse::<DocumentMut>()?;
    cargo_doc["workspace"]["package"]["version"] = value(&new_version);
    fs::write(&cargo_path, cargo_doc.to_string())?;
    println!(" Updated {}", cargo_path.display());

    // 2. Update apps/frank-gui/package.json
    let pkg_path = root.join("apps/frank-gui/package.json");
    let pkg_content = fs::read_to_string(&pkg_path)?;
    let mut pkg_json: serde_json::Value = serde_json::from_str(&pkg_content)?;
    if let Some(obj) = pkg_json.as_object_mut() {
        obj.insert("version".to_string(), serde_json::Value::String(new_version.clone()));
    }
    fs::write(&pkg_path, serde_json::to_string_pretty(&pkg_json)? + "\n")?;
    println!(" Updated {}", pkg_path.display());

    // 3. Update apps/frank-gui/src-tauri/tauri.conf.json
    let tauri_path = root.join("apps/frank-gui/src-tauri/tauri.conf.json");
    let tauri_content = fs::read_to_string(&tauri_path)?;
    let mut tauri_json: serde_json::Value = serde_json::from_str(&tauri_content)?;
    if let Some(obj) = tauri_json.as_object_mut() {
        obj.insert("version".to_string(), serde_json::Value::String(new_version.clone()));
    }
    fs::write(&tauri_path, serde_json::to_string_pretty(&tauri_json)? + "\n")?;
    println!(" Updated {}", tauri_path.display());

    println!("Version successfully bumped to {}!", new_version);
    Ok(())
}

fn is_git_dirty(root: &Path) -> bool {
    let output = ProcessCommand::new("git")
        .args(["status", "--porcelain"])
        .current_dir(root)
        .output();
    match output {
        Ok(out) => !out.stdout.is_empty(),
        Err(_) => false,
    }
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
        println!("[DRY RUN] Would create git tag: {}", tag_name);
        if push {
            println!("[DRY RUN] Would push git tag '{}' to origin", tag_name);
        }
        return Ok(());
    }

    println!("Creating git tag: {}", tag_name);
    let status = ProcessCommand::new("git")
        .args(["tag", "-a", &tag_name, "-m", &format!("Release {}", tag_name)])
        .current_dir(root)
        .status()?;

    if !status.success() {
        bail!("Failed to create git tag '{}'", tag_name);
    }

    println!("Successfully created git tag '{}'", tag_name);

    if push {
        println!("Pushing tag '{}' to origin...", tag_name);
        let push_status = ProcessCommand::new("git")
            .args(["push", "origin", &tag_name])
            .current_dir(root)
            .status()?;
        if !push_status.success() {
            bail!("Failed to push git tag '{}' to origin", tag_name);
        }
        println!("Successfully pushed tag '{}' to origin", tag_name);
    }

    Ok(())
}

fn checksums(root: &Path) -> Result<()> {
    let dist_dir = root.join("dist");
    if !dist_dir.exists() {
        bail!("Directory {} does not exist. Build release artifacts first.", dist_dir.display());
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
        lines.push(format!("{}  {}", hash, file_name));
    }

    let sums_path = dist_dir.join("SHA256SUMS");
    let content = lines.join("\n") + "\n";
    fs::write(&sums_path, content)?;

    println!("Wrote checksums to {}", sums_path.display());
    for line in lines {
        println!("  {}", line);
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

