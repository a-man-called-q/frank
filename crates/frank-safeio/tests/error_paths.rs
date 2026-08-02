use frank_safeio::{
    MAX_CONFIG_BYTES, MAX_SESSION_BYTES, SafeIoError, read_lines, remove_file,
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
