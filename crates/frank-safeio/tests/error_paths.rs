use frank_safeio::{
    MAX_CONFIG_BYTES, SafeIoError, read_lines, remove_file, remove_file_if_contains,
    write_flag_atomic, write_text_atomic,
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
    assert!(matches!(
        remove_file(&directory),
        Err(SafeIoError::NotAFile)
    ));
}

#[test]
fn config_cap_constant_is_large_enough_for_documents() {
    assert_eq!(MAX_CONFIG_BYTES, 64 * 1024);
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
