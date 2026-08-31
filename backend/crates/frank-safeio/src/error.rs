use std::io;

/// Every variant here corresponds to a refusal, not necessarily a bug —
/// callers on hook paths are expected to discard the `Result` (`.ok()`) so a
/// hook always exits 0. The type exists so the *library* stays testable:
/// tests assert on which refusal fired instead of only "it silently did
/// nothing," which is how the original's equivalent bugs went unnoticed.
#[derive(Debug, thiserror::Error)]
pub enum SafeIoError {
    #[error("path is a symlink, refusing to follow")]
    IsSymlink,

    #[error("symlinked parent does not resolve to a directory")]
    SymlinkTargetNotDir,

    #[error("symlinked parent is owned by a different user")]
    SymlinkTargetWrongOwner,

    #[error("symlinked parent resolves outside the home directory")]
    SymlinkTargetOutsideHome,

    #[error("target is not a regular file")]
    NotAFile,

    #[error("file exceeds the {0}-byte safety cap")]
    TooLarge(usize),

    #[error("value is not on the caller-supplied whitelist")]
    NotWhitelisted,

    #[error(transparent)]
    Io(#[from] io::Error),
}

#[cfg(unix)]
impl From<rustix::io::Errno> for SafeIoError {
    fn from(e: rustix::io::Errno) -> Self {
        SafeIoError::Io(io::Error::from(e))
    }
}

pub type Result<T> = std::result::Result<T, SafeIoError>;
