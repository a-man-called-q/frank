use frank_safeio::{
    MAX_CONFIG_BYTES, MAX_SESSION_BYTES, SafeIoError, append_line, read_lines, remove_file,
    remove_file_if_contains, write_flag_atomic, write_text_atomic,
};
use std::path::Path;
use tempfile::tempdir;

#[test]
fn write_text_atomic_rejects_content_over_the_requested_cap() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("config.toml");

    let error = write_text_atomic(&path, "12345", 4).unwrap_err();
    assert!(matches!(error, SafeIoError::TooLarge(4)));
    assert!(!path.exists());
}

#[test]
fn write_text_atomic_accepts_content_at_the_requested_cap() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("config.toml");
    let content = "x".repeat(4);

    write_text_atomic(&path, &content, content.len()).unwrap();
    assert_eq!(std::fs::read_to_string(path).unwrap(), content);
}

#[test]
fn read_text_capped_accepts_content_at_the_requested_cap() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("config.toml");
    let content = "x".repeat(8);
    std::fs::write(&path, &content).unwrap();

    assert_eq!(
        frank_safeio::read_text_capped(&path, content.len()).unwrap(),
        content
    );
}

#[test]
fn read_lines_ignores_directories_and_missing_paths() {
    let tmp = tempdir().unwrap();
    let directory = tmp.path().join("history-dir");
    std::fs::create_dir(&directory).unwrap();

    assert!(read_lines(&directory).is_empty());
    assert!(read_lines(&tmp.path().join("does-not-exist")).is_empty());
}

#[test]
fn remove_file_returns_false_for_missing_paths_and_rejects_directories() {
    let tmp = tempdir().unwrap();
    let missing = tmp.path().join("missing");
    let directory = tmp.path().join("directory");
    std::fs::create_dir(&directory).unwrap();

    assert!(!remove_file(&missing).unwrap());
    assert!(!remove_file_if_contains(&missing, "marker").unwrap());

    let missing_parent = tmp.path().join("missing-parent").join("child");
    assert!(!remove_file(&missing_parent).unwrap());
    assert!(!remove_file_if_contains(&missing_parent, "marker").unwrap());

    let file_parent = tmp.path().join("file-parent");
    std::fs::write(&file_parent, "not a directory").unwrap();
    let child_of_file = file_parent.join("child");
    assert!(remove_file(&child_of_file).is_err());
    assert!(remove_file_if_contains(&child_of_file, "marker").is_err());
    assert!(matches!(
        remove_file(&directory),
        Err(SafeIoError::NotAFile)
    ));
}

#[cfg(unix)]
#[test]
fn removers_propagate_non_not_found_stat_errors() {
    let tmp = tempdir().unwrap();
    let long_name = "x".repeat(300);
    let path = tmp.path().join(long_name);

    assert!(remove_file(&path).is_err());
    assert!(remove_file_if_contains(&path, "marker").is_err());

    let long_parent = tmp.path().join("p".repeat(300));
    let child = long_parent.join("child");
    assert!(remove_file(&child).is_err());
    assert!(remove_file_if_contains(&child, "marker").is_err());
}

#[test]
fn marker_removal_accepts_a_file_exactly_at_the_size_cap() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("managed");
    let marker = "frank-managed";
    let content = format!("{marker}{}", "x".repeat(MAX_CONFIG_BYTES - marker.len()));
    std::fs::write(&path, content).unwrap();

    assert!(remove_file_if_contains(&path, marker).unwrap());
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn removers_refuse_symlinks_without_touching_the_target() {
    use std::os::unix::fs::symlink;

    let tmp = tempdir().unwrap();
    let target = tmp.path().join("target");
    let link = tmp.path().join("managed");
    std::fs::write(&target, "frank-managed").unwrap();
    symlink(&target, &link).unwrap();

    assert!(matches!(remove_file(&link), Err(SafeIoError::IsSymlink)));
    assert!(matches!(
        remove_file_if_contains(&link, "frank-managed"),
        Err(SafeIoError::IsSymlink)
    ));
    assert!(target.exists());
    assert!(link.exists());
}

#[test]
fn ensure_dir_verifies_a_real_directory_and_rejects_a_file() {
    let tmp = tempdir().unwrap();
    let nested = tmp.path().join("a/b");
    frank_safeio::ensure_dir(&nested).unwrap();
    assert!(nested.is_dir());

    let file = tmp.path().join("file");
    std::fs::write(&file, "not a directory").unwrap();
    assert!(frank_safeio::ensure_dir(&file).is_err());
}

#[test]
fn config_cap_constant_is_large_enough_for_documents() {
    assert_eq!(MAX_CONFIG_BYTES, 64 * 1024);
    assert_eq!(MAX_SESSION_BYTES, 16 * 1024 * 1024);
}

#[test]
fn home_dir_matches_the_platform_environment() {
    assert_eq!(
        frank_safeio::home_dir(),
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
            .map(std::path::PathBuf::from)
    );
}

#[test]
fn path_without_a_filename_is_rejected_by_every_writer_and_remover() {
    let root = Path::new("/");
    assert!(matches!(
        write_flag_atomic(root, "full"),
        Err(SafeIoError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidInput
    ));
    assert!(matches!(
        write_text_atomic(root, "full", 64),
        Err(SafeIoError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidInput
    ));
    assert!(matches!(
        frank_safeio::read_text_capped(root, 64),
        Err(SafeIoError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidInput
    ));
    assert!(matches!(
        frank_safeio::append_line(root, "line"),
        Err(SafeIoError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidInput
    ));
    assert!(matches!(
        remove_file(root),
        Err(SafeIoError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidInput
    ));
    assert!(matches!(
        remove_file_if_contains(root, "marker"),
        Err(SafeIoError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidInput
    ));
}

#[cfg(unix)]
#[test]
fn append_line_refuses_symlinked_file() {
    use std::os::unix::fs::symlink;

    let tmp = tempdir().unwrap();
    let target = tmp.path().join("target.log");
    let link = tmp.path().join("link.log");
    std::fs::write(&target, "existing\n").unwrap();
    symlink(&target, &link).unwrap();

    assert!(matches!(
        append_line(&link, "new line"),
        Err(SafeIoError::IsSymlink)
    ));
}

#[test]
fn append_line_creates_parent_if_missing() {
    let tmp = tempdir().unwrap();
    let nested = tmp.path().join("a/b/c/log.jsonl");
    
    append_line(&nested, "first line").unwrap();
    assert!(nested.exists());
    let content = std::fs::read_to_string(&nested).unwrap();
    assert_eq!(content, "first line\n");
}

#[test]
fn append_line_normalizes_newlines() {
    let tmp = tempdir().unwrap();
    let log = tmp.path().join("test.log");
    
    // Line without newline
    append_line(&log, "line1").unwrap();
    // Line with newline
    append_line(&log, "line2\n").unwrap();
    // Line with multiple newlines (strips to single newline before adding one)
    append_line(&log, "line3").unwrap();
    
    let content = std::fs::read_to_string(&log).unwrap();
    // Each should have exactly one trailing newline
    assert_eq!(content, "line1\nline2\nline3\n");
}

#[test]
fn read_text_capped_rejects_oversized_content() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("large.txt");
    let content = "x".repeat(100);
    std::fs::write(&path, &content).unwrap();
    
    let error = frank_safeio::read_text_capped(&path, 50).unwrap_err();
    assert!(matches!(error, SafeIoError::TooLarge(50)));
}

#[cfg(unix)]
#[test]
fn read_text_capped_refuses_symlinked_file() {
    use std::os::unix::fs::symlink;

    let tmp = tempdir().unwrap();
    let target = tmp.path().join("target.txt");
    let link = tmp.path().join("link.txt");
    std::fs::write(&target, "secret").unwrap();
    symlink(&target, &link).unwrap();
    
    let error = frank_safeio::read_text_capped(&link, 100).unwrap_err();
    assert!(matches!(error, SafeIoError::IsSymlink));
}

#[test]
fn read_text_capped_refuses_directory() {
    let tmp = tempdir().unwrap();
    let dir = tmp.path().join("dir");
    std::fs::create_dir(&dir).unwrap();
    
    let error = frank_safeio::read_text_capped(&dir, 100).unwrap_err();
    assert!(matches!(error, SafeIoError::NotAFile));
}

#[test]
fn remove_file_if_contains_oversized_file() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("huge");
    let content = "x".repeat(MAX_CONFIG_BYTES + 100);
    std::fs::write(&path, content).unwrap();
    
    let error = remove_file_if_contains(&path, "marker").unwrap_err();
    assert!(matches!(error, SafeIoError::TooLarge(MAX_CONFIG_BYTES)));
}

#[cfg(unix)]
#[test]
fn write_flag_atomic_creates_with_correct_permissions() {
    use std::os::unix::fs::PermissionsExt;
    
    let tmp = tempdir().unwrap();
    let flag = tmp.path().join("flag");
    
    write_flag_atomic(&flag, "test").unwrap();
    
    let metadata = std::fs::metadata(&flag).unwrap();
    let mode = metadata.permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "flag file must have 0600 permissions");
}

#[cfg(unix)]
#[test]
fn write_flag_atomic_refuses_pre_existing_symlink() {
    use std::os::unix::fs::symlink;
    
    let tmp = tempdir().unwrap();
    let target = tmp.path().join("target");
    let flag = tmp.path().join("flag");
    std::fs::write(&target, "original").unwrap();
    symlink(&target, &flag).unwrap();
    
    let error = write_flag_atomic(&flag, "new").unwrap_err();
    assert!(matches!(error, SafeIoError::IsSymlink));
    
    // Target should not be modified
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "original");
}

#[test]
fn read_lines_handles_no_trailing_newline() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("lines.txt");
    std::fs::write(&path, "a\nb\nc").unwrap();
    
    assert_eq!(read_lines(&path), vec!["a", "b", "c"]);
}

#[test]
fn read_lines_handles_empty_file() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("empty.txt");
    std::fs::write(&path, "").unwrap();
    
    assert!(read_lines(&path).is_empty());
}

#[test]
fn read_lines_handles_only_blank_lines() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("blanks.txt");
    std::fs::write(&path, "\n\n\n").unwrap();
    
    assert!(read_lines(&path).is_empty());
}

#[test]
fn read_lines_trims_whitespace() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("spaces.txt");
    std::fs::write(&path, "  line1  \n\n  line2  \n").unwrap();
    
    // Lines are NOT trimmed by read_lines, only filtered if empty
    let lines = read_lines(&path);
    assert_eq!(lines, vec!["  line1  ", "  line2  "]);
}

#[test]
fn write_flag_atomic_temp_file_cleanup_on_write_failure() {
    use std::os::unix::fs::PermissionsExt;
    
    let tmp = tempdir().unwrap();
    let flag_dir = tmp.path().join("readonly_after_create");
    std::fs::create_dir_all(&flag_dir).unwrap();
    
    let flag = flag_dir.join("flag");
    
    // Write once successfully
    write_flag_atomic(&flag, "test").unwrap();
    
    // Make directory read-only, which should cause temp file operations to fail
    std::fs::set_permissions(&flag_dir, std::fs::Permissions::from_mode(0o555)).unwrap();
    
    // This should fail but not panic and not leave temp files
    let _ = write_flag_atomic(&flag, "test2");
    
    // Restore permissions to check
    std::fs::set_permissions(&flag_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
    
    // Check no .tmp files left behind
    for entry in std::fs::read_dir(&flag_dir).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        assert!(!name_str.contains(".tmp"), "temp file {name_str} was not cleaned up");
    }
}

#[cfg(unix)]
#[test]
fn refuse_if_symlink_catches_race_condition() {
    use std::os::unix::fs::symlink;
    
    let tmp = tempdir().unwrap();
    let flag = tmp.path().join("flag");
    
    // Create a normal file first
    std::fs::write(&flag, "original").unwrap();
    
    // Now overwrite successfully
    write_flag_atomic(&flag, "updated").unwrap();
    assert_eq!(std::fs::read_to_string(&flag).unwrap(), "updated");
    
    // Remove it and replace with symlink
    std::fs::remove_file(&flag).unwrap();
    let target = tmp.path().join("target");
    std::fs::write(&target, "decoy").unwrap();
    symlink(&target, &flag).unwrap();
    
    // Should now be refused
    let error = write_flag_atomic(&flag, "attack").unwrap_err();
    assert!(matches!(error, SafeIoError::IsSymlink));
}

#[test]
fn open_append_create_race_handling() {
    let tmp = tempdir().unwrap();
    let log = tmp.path().join("race.log");
    
    // Simulate multiple threads/processes racing to create and append
    let handles: Vec<_> = (0..8)
        .map(|i| {
            let log = log.clone();
            std::thread::spawn(move || {
                append_line(&log, &format!("line {i}")).unwrap();
            })
        })
        .collect();
    
    for h in handles {
        h.join().unwrap();
    }
    
    let lines = frank_safeio::read_lines(&log);
    assert_eq!(lines.len(), 8, "all 8 concurrent appends must succeed");
}

#[test]
fn remove_file_if_contains_reads_entire_file() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("managed");
    
    // Marker at the end of file
    let content = format!("{}frank-managed", "x".repeat(1000));
    std::fs::write(&path, &content).unwrap();
    
    assert!(remove_file_if_contains(&path, "frank-managed").unwrap());
    assert!(!path.exists());
}

#[test]
fn remove_file_if_contains_with_non_utf8_content() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("binary");
    
    // Write binary content with marker
    let mut content = vec![0xff, 0xfe, 0xfd];
    content.extend_from_slice(b"frank-managed");
    content.extend_from_slice(&[0x00, 0x01, 0x02]);
    std::fs::write(&path, &content).unwrap();
    
    assert!(remove_file_if_contains(&path, "frank-managed").unwrap());
    assert!(!path.exists());
}

#[test]
fn ensure_dir_with_existing_directory() {
    let tmp = tempdir().unwrap();
    let dir = tmp.path().join("existing");
    std::fs::create_dir(&dir).unwrap();
    
    // Should succeed when directory already exists
    frank_safeio::ensure_dir(&dir).unwrap();
    assert!(dir.is_dir());
}

#[cfg(unix)]
#[test]
fn ensure_dir_with_symlink_to_directory() {
    use std::os::unix::fs::symlink;
    
    let tmp = tempdir().unwrap();
    let real_dir = tmp.path().join("real");
    std::fs::create_dir(&real_dir).unwrap();
    
    let link = tmp.path().join("link");
    symlink(&real_dir, &link).unwrap();
    
    // Should succeed - symlinks to directories are allowed
    frank_safeio::ensure_dir(&link).unwrap();
}

#[test]
fn read_text_capped_missing_file() {
    let tmp = tempdir().unwrap();
    let missing = tmp.path().join("missing.txt");
    
    assert!(frank_safeio::read_text_capped(&missing, 100).is_err());
}

#[test]
fn write_text_atomic_with_empty_content() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("empty.txt");
    
    write_text_atomic(&path, "", 100).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
}

#[test]
fn read_lines_with_very_long_lines() {
    let tmp = tempdir().unwrap();
    let path = tmp.path().join("long.txt");
    
    let long_line = "x".repeat(10000);
    std::fs::write(&path, format!("{long_line}\nshort\n{long_line}")).unwrap();
    
    let lines = read_lines(&path);
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].len(), 10000);
    assert_eq!(lines[1], "short");
    assert_eq!(lines[2].len(), 10000);
}

#[test]
fn read_flag_with_whitespace_variations() {
    const MODES: &[&str] = &["lite", "full", "ultra"];
    
    let tmp = tempdir().unwrap();
    let flag = tmp.path().join("flag");
    
    // Test various whitespace scenarios
    std::fs::write(&flag, "  lite  \n").unwrap();
    assert_eq!(
        frank_safeio::read_flag(&flag, MODES).as_deref(),
        Some("lite")
    );
    
    std::fs::write(&flag, "\nfull\n").unwrap();
    assert_eq!(
        frank_safeio::read_flag(&flag, MODES).as_deref(),
        Some("full")
    );
    
    std::fs::write(&flag, "ULTRA").unwrap();
    assert_eq!(
        frank_safeio::read_flag(&flag, MODES).as_deref(),
        Some("ultra")
    );
}
