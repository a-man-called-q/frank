//! `frank compress` — deterministic, offline (no API key, no `claude`
//! CLI). Sensitive-path refusal, out-of-tree backup, byte-verified backup
//! before the source is ever touched, and validation before write. See
//! `frank-compress`'s crate docs for what was ported from `compress.py`
//! versus dropped (the LLM orchestration).

use std::path::{Path, PathBuf};

pub struct CompressArgs {
    pub paths: Vec<PathBuf>,
    pub check: bool,
    pub dry_run: bool,
    pub restore: bool,
}

pub fn run(args: CompressArgs) -> i32 {
    if args.paths.is_empty() {
        eprintln!("frank compress: no paths given");
        return 2;
    }
    if args.restore {
        return restore_all(&args.paths);
    }
    let mut exit = 0;
    for path in &args.paths {
        exit = exit.max(compress_one(path, args.check, args.dry_run));
    }
    exit
}

fn compress_one(path: &Path, check: bool, dry_run: bool) -> i32 {
    if frank_compress::is_sensitive_path(path) {
        eprintln!(
            "frank: refusing {} — looks like it may hold secrets",
            path.display()
        );
        return 1;
    }
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        eprintln!("frank: cannot read {}", path.display());
        return 1;
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        eprintln!(
            "frank: refusing non-regular or symlinked path {}",
            path.display()
        );
        return 1;
    }
    if !frank_compress::should_compress(path) {
        println!("frank: skipping {} (not natural language)", path.display());
        return 0;
    }
    let Ok(text) = frank_safeio::read_text_capped(path, frank_safeio::MAX_SESSION_BYTES) else {
        eprintln!("frank: cannot read {}", path.display());
        return 1;
    };
    if text.trim().is_empty() {
        println!("frank: skipping {} (empty)", path.display());
        return 0;
    }

    let (frontmatter, body) = frank_compress::split_frontmatter(&text);
    if body.trim().is_empty() {
        println!(
            "frank: skipping {} (empty after frontmatter)",
            path.display()
        );
        return 0;
    }
    let compressed_body = frank_compress::compress(body).compressed;
    if compressed_body.trim() == body.trim() {
        println!(
            "frank: {} unchanged (already compressed or no compressible prose)",
            path.display()
        );
        return 0;
    }
    let result_text = format!("{frontmatter}{compressed_body}");

    let validation = frank_compress::validate(&text, &result_text);
    for f in validation.warnings() {
        eprintln!("frank: warning for {}: {}", path.display(), f.message);
    }
    if !validation.is_valid() {
        eprintln!(
            "frank: refusing to write {} — validation failed:",
            path.display()
        );
        for f in validation.errors() {
            eprintln!("  - {}", f.message);
        }
        return 1;
    }

    let before = text.len();
    let after = result_text.len();
    let pct = 100.0 * (1.0 - after as f64 / before as f64);

    if check {
        println!(
            "frank: {} would shrink {before}B -> {after}B ({pct:.1}% smaller)",
            path.display()
        );
        return 0;
    }

    let backup_path = frank_compress::backup_path_for(path);
    match std::fs::symlink_metadata(&backup_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            eprintln!(
                "frank: refusing symlinked backup at {}",
                backup_path.display()
            );
            return 1;
        }
        Ok(metadata) if !metadata.is_file() => {
            eprintln!(
                "frank: refusing non-file backup at {}",
                backup_path.display()
            );
            return 1;
        }
        Ok(_) => {
            eprintln!(
                "frank: refusing to overwrite existing backup at {} — run with --restore first if you want to recompress",
                backup_path.display()
            );
            return 1;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            eprintln!(
                "frank: cannot inspect backup {}: {error}",
                backup_path.display()
            );
            return 1;
        }
    }

    if dry_run {
        println!(
            "frank: would back up {} -> {}",
            path.display(),
            backup_path.display()
        );
        println!(
            "frank: would write compressed {} ({before}B -> {after}B, {pct:.1}% smaller)",
            path.display()
        );
        return 0;
    }

    if let Some(dir) = backup_path.parent() {
        if let Err(e) = frank_safeio::ensure_dir(dir) {
            eprintln!("frank: failed to create backup directory: {e}");
            return 1;
        }
    }
    if let Err(e) = frank_safeio::write_flag_atomic(&backup_path, &text) {
        eprintln!("frank: failed to write backup: {e}");
        return 1;
    }
    // Read the backup back and byte-compare before ever touching the
    // source — a failed backup must never be discovered after the
    // original is already gone.
    if !backup_matches(&backup_path, &text) {
        eprintln!(
            "frank: backup verification failed, aborting before touching {}",
            path.display()
        );
        let _ = frank_safeio::remove_file(&backup_path);
        return 1;
    }

    if let Err(e) = frank_safeio::write_flag_atomic(path, &result_text) {
        eprintln!("frank: failed to write compressed file: {e}");
        return 1;
    }

    println!(
        "frank: compressed {} ({before}B -> {after}B, {pct:.1}% smaller)",
        path.display()
    );
    println!("  backup: {}", backup_path.display());
    0
}

fn restore_all(paths: &[PathBuf]) -> i32 {
    let mut exit = 0;
    for path in paths {
        let backup_path = frank_compress::backup_path_for(path);
        let backup_metadata = match std::fs::symlink_metadata(&backup_path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                eprintln!("frank: refusing symlinked backup for {}", path.display());
                exit = 1;
                continue;
            }
            Ok(metadata) if !metadata.is_file() => {
                eprintln!("frank: backup is not a regular file for {}", path.display());
                exit = 1;
                continue;
            }
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                eprintln!(
                    "frank: cannot inspect backup for {}: {error}",
                    path.display()
                );
                exit = 1;
                continue;
            }
        };
        if backup_metadata.is_none() {
            eprintln!(
                "frank: no backup found for {} (looked at {})",
                path.display(),
                backup_path.display()
            );
            exit = 1;
            continue;
        }
        match frank_safeio::read_text_capped(&backup_path, frank_safeio::MAX_SESSION_BYTES) {
            Ok(original) => match frank_safeio::write_flag_atomic(path, &original) {
                Ok(()) => {
                    println!("frank: restored {} from backup", path.display());
                    let _ = frank_safeio::remove_file(&backup_path);
                }
                Err(e) => {
                    eprintln!("frank: failed to restore {}: {e}", path.display());
                    exit = 1;
                }
            },
            Err(e) => {
                eprintln!("frank: failed to read backup for {}: {e}", path.display());
                exit = 1;
            }
        }
    }
    exit
}

fn backup_matches(path: &Path, expected: &str) -> bool {
    matches!(
        frank_safeio::read_text_capped(path, frank_safeio::MAX_SESSION_BYTES),
        Ok(read_back) if read_back == expected
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn empty_compress_invocation_is_usage_error() {
        assert_eq!(
            run(CompressArgs {
                paths: vec![],
                check: false,
                dry_run: false,
                restore: false,
            }),
            2
        );
    }

    #[test]
    fn directories_and_symlinks_fail_closed_before_reading_content() {
        let tmp = tempdir().unwrap();
        let directory = tmp.path().join("notes.md");
        std::fs::create_dir(&directory).unwrap();
        assert_eq!(compress_one(&directory, false, false), 1);

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let real = tmp.path().join("real.md");
            std::fs::write(&real, "A useful paragraph that should stay intact.").unwrap();
            let link = tmp.path().join("link.md");
            symlink(&real, &link).unwrap();
            assert_eq!(compress_one(&link, false, false), 1);
            assert_eq!(
                std::fs::read_to_string(real).unwrap(),
                "A useful paragraph that should stay intact."
            );
        }
    }

    #[test]
    fn restore_missing_backup_is_a_failure_without_touching_source() {
        let tmp = tempdir().unwrap();
        let source = tmp.path().join("notes.md");
        std::fs::write(&source, "original").unwrap();
        assert_eq!(restore_all(std::slice::from_ref(&source)), 1);
        assert_eq!(std::fs::read_to_string(source).unwrap(), "original");
    }

    #[test]
    fn backup_verification_requires_an_exact_readback() {
        let tmp = tempdir().unwrap();
        let backup = tmp.path().join("backup.md");
        std::fs::write(&backup, "different").unwrap();
        assert!(!backup_matches(&backup, "expected"));
        std::fs::write(&backup, "expected").unwrap();
        assert!(backup_matches(&backup, "expected"));
        assert!(!backup_matches(&tmp.path().join("missing.md"), "expected"));
    }
}
