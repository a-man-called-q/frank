//! Small privileged-free updater helper.
//!
//! The helper is intentionally separate from the GUI/daemon: it verifies a
//! staged payload, swaps two sibling bundle directories atomically where the
//! host filesystem permits it, and leaves rollback metadata behind. Service
//! stop/start and health checks are delegated to the caller's per-user service
//! controller so this binary remains portable across macOS, Linux, and Windows.

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use frank_update::{UpdateArtifact, verify_artifact_bytes};

#[derive(Debug, Parser)]
#[command(name = "frank-updater", version, about = "Frank signed update helper")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Verify a downloaded payload against a manifest artifact entry.
    Verify {
        #[arg(long)]
        artifact_json: PathBuf,
        #[arg(long)]
        file: PathBuf,
    },
    /// Atomically move a staged bundle into place and keep one rollback copy.
    Swap {
        #[arg(long)]
        staged: PathBuf,
        #[arg(long)]
        current: PathBuf,
        #[arg(long)]
        previous: PathBuf,
    },
    /// Restore the previous known-good bundle and keep the failed bundle for
    /// a future diagnostic or retry.
    Rollback {
        #[arg(long)]
        current: PathBuf,
        #[arg(long)]
        previous: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Verify {
            artifact_json,
            file,
        } => {
            let artifact: UpdateArtifact = serde_json::from_slice(&std::fs::read(artifact_json)?)?;
            let bytes = std::fs::read(file)?;
            verify_artifact_bytes(&artifact, &bytes)?;
            println!("verified");
        }
        Command::Swap {
            staged,
            current,
            previous,
        } => {
            swap_bundles(&staged, &current, &previous)?;
            println!("swapped");
        }
        Command::Rollback { current, previous } => {
            rollback_bundles(&current, &previous)?;
            println!("rolled back");
        }
    }
    Ok(())
}

fn swap_bundles(
    staged: &std::path::Path,
    current: &std::path::Path,
    previous: &std::path::Path,
) -> anyhow::Result<()> {
    if !staged.is_dir() || current == staged || previous == staged || current == previous {
        anyhow::bail!("staged bundle and swap paths are invalid");
    }
    reject_symlink_components(staged)?;
    reject_symlink_components(current)?;
    reject_symlink_components(previous)?;
    reject_symlink(staged)?;
    reject_symlink_if_present(current)?;
    reject_symlink_if_present(previous)?;
    let backup = unique_sibling(previous);
    if backup.exists() {
        remove_real_directory(&backup)?;
    }
    let mut previous_backed_up = false;
    let mut current_moved = false;
    if previous.exists() {
        std::fs::rename(previous, &backup)?;
        previous_backed_up = true;
    }
    let result = (|| -> std::io::Result<()> {
        if current.exists() {
            std::fs::rename(current, previous)?;
            current_moved = true;
        }
        std::fs::rename(staged, current)
    })();
    if let Err(error) = result {
        if current_moved && !current.exists() && previous.exists() {
            let _ = std::fs::rename(previous, current);
        }
        if previous_backed_up && !previous.exists() && backup.exists() {
            let _ = std::fs::rename(&backup, previous);
        }
        return Err(error.into());
    }
    if previous_backed_up {
        if current_moved {
            std::fs::remove_dir_all(&backup)?;
        } else {
            std::fs::rename(&backup, previous)?;
        }
    }
    Ok(())
}

fn rollback_bundles(current: &std::path::Path, previous: &std::path::Path) -> anyhow::Result<()> {
    if current == previous {
        anyhow::bail!("current and previous bundle paths are identical");
    }
    reject_symlink_components(current)?;
    reject_symlink_components(previous)?;
    reject_symlink(current)?;
    reject_symlink(previous)?;
    let displaced = unique_sibling(current);
    if displaced.exists() {
        remove_real_directory(&displaced)?;
    }
    std::fs::rename(current, &displaced)?;
    if let Err(error) = std::fs::rename(previous, current) {
        let _ = std::fs::rename(&displaced, current);
        return Err(error.into());
    }
    if let Err(error) = std::fs::rename(&displaced, previous) {
        // Restore the original layout if the second half fails.  The
        // previous bundle is still at `current`; move it out of the way,
        // restore the displaced current, then put it back at `current`.
        let _ = std::fs::rename(current, previous);
        let _ = std::fs::rename(&displaced, current);
        return Err(error.into());
    }
    Ok(())
}

fn reject_symlink(path: &std::path::Path) -> anyhow::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        anyhow::bail!("swap path must be a real directory");
    }
    Ok(())
}

/// Check the bundle leaf and its first existing parent.  Directory-level
/// swaps are security-sensitive: a symlinked parent could redirect an
/// otherwise verified bundle into an unrelated location between validation
/// and rename. Missing components are allowed for destination paths, while a
/// normal ancestor stops the walk so platform aliases such as macOS `/var`
/// remain usable. Metadata errors fail closed.
fn reject_symlink_components(path: &std::path::Path) -> anyhow::Result<()> {
    let mut current = path;
    loop {
        match std::fs::symlink_metadata(current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                anyhow::bail!("swap path may not contain symlink components");
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let Some(parent) = current.parent() else {
            break;
        };
        match std::fs::symlink_metadata(parent) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                anyhow::bail!("swap path may not contain symlink components");
            }
            Ok(_) => break,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => current = parent,
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn reject_symlink_if_present(path: &std::path::Path) -> anyhow::Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
            anyhow::bail!("swap path must be a real directory")
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn remove_real_directory(path: &std::path::Path) -> anyhow::Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        anyhow::bail!("refusing to remove a non-directory swap path");
    }
    std::fs::remove_dir_all(path)?;
    Ok(())
}

fn unique_sibling(path: &std::path::Path) -> PathBuf {
    path.with_file_name(format!(
        ".{}.swap-{}-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("bundle"),
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swap_keeps_previous_and_is_repeatable() {
        let root = tempfile::tempdir().unwrap();
        let staged = root.path().join("staged");
        let current = root.path().join("current");
        let previous = root.path().join("previous");
        std::fs::create_dir_all(&staged).unwrap();
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&previous).unwrap();
        std::fs::write(staged.join("version"), "new").unwrap();
        std::fs::write(current.join("version"), "current").unwrap();
        std::fs::write(previous.join("version"), "old").unwrap();

        swap_bundles(&staged, &current, &previous).unwrap();
        assert_eq!(
            std::fs::read_to_string(current.join("version")).unwrap(),
            "new"
        );
        assert_eq!(
            std::fs::read_to_string(previous.join("version")).unwrap(),
            "current"
        );
        assert!(!staged.exists());

        let next = root.path().join("next");
        std::fs::create_dir_all(&next).unwrap();
        std::fs::write(next.join("version"), "next").unwrap();
        swap_bundles(&next, &current, &previous).unwrap();
        assert_eq!(
            std::fs::read_to_string(current.join("version")).unwrap(),
            "next"
        );
        assert_eq!(
            std::fs::read_to_string(previous.join("version")).unwrap(),
            "new"
        );
    }

    #[test]
    fn rollback_exchanges_current_and_previous() {
        let root = tempfile::tempdir().unwrap();
        let current = root.path().join("current");
        let previous = root.path().join("previous");
        std::fs::create_dir_all(&current).unwrap();
        std::fs::create_dir_all(&previous).unwrap();
        std::fs::write(current.join("version"), "bad").unwrap();
        std::fs::write(previous.join("version"), "good").unwrap();

        rollback_bundles(&current, &previous).unwrap();
        assert_eq!(
            std::fs::read_to_string(current.join("version")).unwrap(),
            "good"
        );
        assert_eq!(
            std::fs::read_to_string(previous.join("version")).unwrap(),
            "bad"
        );
    }

    #[cfg(unix)]
    #[test]
    fn swap_rejects_a_symlinked_parent() {
        use std::os::unix::fs::symlink;

        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("real");
        let link = root.path().join("link");
        std::fs::create_dir(&real).unwrap();
        symlink(&real, &link).unwrap();
        let staged = link.join("staged");
        let current = link.join("current");
        let previous = link.join("previous");
        std::fs::create_dir(&staged).unwrap();
        let result = swap_bundles(&staged, &current, &previous);
        assert!(result.is_err());
        assert!(staged.is_dir());
    }
}
