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
    if !frank_compress::should_compress(path) {
        println!("frank: skipping {} (not natural language)", path.display());
        return 0;
    }

    let Ok(text) = std::fs::read_to_string(path) else {
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
    if backup_path.exists() {
        eprintln!(
            "frank: refusing to overwrite existing backup at {} — run with --restore first if you want to recompress",
            backup_path.display()
        );
        return 1;
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
        if let Err(e) = std::fs::create_dir_all(dir) {
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
    match std::fs::read_to_string(&backup_path) {
        Ok(read_back) if read_back == text => {}
        _ => {
            eprintln!(
                "frank: backup verification failed, aborting before touching {}",
                path.display()
            );
            let _ = std::fs::remove_file(&backup_path);
            return 1;
        }
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
        if !backup_path.exists() {
            eprintln!(
                "frank: no backup found for {} (looked at {})",
                path.display(),
                backup_path.display()
            );
            exit = 1;
            continue;
        }
        match std::fs::read_to_string(&backup_path) {
            Ok(original) => match frank_safeio::write_flag_atomic(path, &original) {
                Ok(()) => {
                    println!("frank: restored {} from backup", path.display());
                    let _ = std::fs::remove_file(&backup_path);
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
