//! Small shared utilities: path checks, string shaping, clocks and hashes.

use std::path::{Path, PathBuf};

use frank_protocol::*;
use sha2::{Digest, Sha256};

use crate::*;

pub(crate) fn worktree_operation_matches_task(operation: &OperationView, task_id: TaskId) -> bool {
    if operation.kind != OperationKind::CreateWorktree {
        return false;
    }
    serde_json::from_str::<CreateWorktreeOperation>(&operation.resource)
        .map(|request| request.task_id == task_id)
        .unwrap_or_else(|_| operation.resource == task_id.to_string())
}

pub(crate) fn operation_lock_resource(operation: &OperationView) -> String {
    match operation.kind {
        OperationKind::CreateWorktree => {
            serde_json::from_str::<CreateWorktreeOperation>(&operation.resource)
                .map(|request| format!("git:project:{}", request.project_id))
                .unwrap_or_else(|_| format!("operation:{}", operation.id))
        }
        OperationKind::CloneProject => {
            serde_json::from_str::<CloneProjectOperation>(&operation.resource)
                .map(|request| format!("git:clone:{}", request.destination))
                .unwrap_or_else(|_| format!("operation:{}", operation.id))
        }
        OperationKind::CommitTask => format!("git:task:{}", operation.resource),
        OperationKind::PushMission | OperationKind::OpenDraftPullRequest => {
            format!("git:mission:{}", operation.resource)
        }
        OperationKind::WriteMemory => format!("memory:{}", operation.resource),
        OperationKind::HostUpdate => {
            serde_json::from_str::<HostUpdateOperation>(&operation.resource)
                .map(|request| format!("host-update:{}", request.update_id))
                .unwrap_or_else(|_| format!("operation:{}", operation.id))
        }
        OperationKind::StageUpdate => {
            serde_json::from_str::<StageUpdateOperation>(&operation.resource)
                .map(|request| format!("host-update:{}", request.update_id))
                .unwrap_or_else(|_| format!("operation:{}", operation.id))
        }
        _ => format!("operation:{}", operation.id),
    }
}

pub(crate) fn is_merge_conflict_error(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("conflict")
        || message.contains("automatic merge failed")
        || (message.contains("merge --squash") && message.contains("unmerged"))
}

pub(crate) fn is_allowed_path(path: &str, roots: &[String]) -> bool {
    if path.trim().is_empty()
        || path.len() > 4_096
        || path.chars().any(|character| character.is_control())
    {
        return false;
    }
    if roots.is_empty() {
        return true;
    }
    let Ok(candidate) = canonicalize_allow_missing(Path::new(path)) else {
        return false;
    };
    roots.iter().any(|root| {
        canonicalize_allow_missing(Path::new(root)).is_ok_and(|root| candidate.starts_with(root))
    })
}

pub(crate) fn canonicalize_allow_missing(path: &Path) -> std::io::Result<PathBuf> {
    let mut missing = Vec::new();
    let mut current = path;
    while std::fs::symlink_metadata(current).is_err() {
        let Some(name) = current.file_name() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "path has no existing ancestor",
            ));
        };
        missing.push(name.to_os_string());
        current = current.parent().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "path has no parent")
        })?;
    }
    let mut resolved = std::fs::canonicalize(current)?;
    for component in missing.iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

/// Bound helper diagnostics before they are persisted in an operation row or
/// sent to a remote client. Provider and updater output is untrusted input;
/// never let a noisy child process exhaust daemon memory or enlarge a command
/// response indefinitely.
/// Truncate captured process output to the protocol's command-body ceiling.
///
/// The bound is [`MAX_COMMAND_BODY_BYTES`] rather than a local constant so it
/// cannot drift from the limit the wire actually enforces.
pub(crate) fn bounded_text(bytes: &[u8]) -> String {
    let end = bytes.len().min(MAX_COMMAND_BODY_BYTES);
    let mut text = String::from_utf8_lossy(&bytes[..end]).into_owned();
    if bytes.len() > MAX_COMMAND_BODY_BYTES {
        text.push_str("\n[output truncated]");
    }
    text
}

pub(crate) fn slug(value: &str) -> String {
    let mut result = String::new();
    for character in value.chars().take(32) {
        if character.is_ascii_alphanumeric() {
            result.push(character.to_ascii_lowercase());
        } else if !result.ends_with('-') {
            result.push('-');
        }
    }
    result
        .trim_matches('-')
        .to_string()
        .if_empty_then(|| "mission".to_string())
}

/// Return a bounded, single-line commit subject owned by the daemon. User
/// supplied task titles are retained for readability, but control characters
/// and excess length are removed so the subject is safe to pass as one Git
/// argument and remains a deterministic idempotency marker.
pub(crate) fn stable_task_commit_message(task_id: TaskId, title: &str) -> String {
    let title = title
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>();
    let title = title.trim();
    let prefix = format!("Frank task {task_id}: ");
    let available = 4_096usize.saturating_sub(prefix.len());
    let suffix = title.chars().take(available).collect::<String>();
    if suffix.is_empty() {
        format!("Frank task {task_id}")
    } else {
        format!("{prefix}{suffix}")
    }
}

pub(crate) fn now_plus_seconds(seconds: u64) -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u128
        + seconds as u128
}

pub(crate) fn local_server_url(settings: &ServerSettings) -> Option<String> {
    if settings.port == 0 {
        return None;
    }
    let bind = settings.bind_address.trim();
    let host = match bind {
        "" | "0.0.0.0" => "127.0.0.1".to_string(),
        "::" => "[::1]".to_string(),
        value if value.contains(':') && !value.starts_with('[') => format!("[{value}]"),
        value => value.to_string(),
    };
    Some(format!("https://{host}:{}", settings.port))
}

pub(crate) fn epoch_seconds() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn hash_capability(token: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(token.as_bytes());
    hex::encode(digest.finalize())
}

pub(crate) fn same_actor(left: &ActorRef, right: &ActorRef) -> bool {
    left.kind == right.kind && left.id == right.id
}

pub(crate) trait EmptyThen {
    fn if_empty_then(self, fallback: impl FnOnce() -> String) -> String;
}

impl EmptyThen for String {
    fn if_empty_then(self, fallback: impl FnOnce() -> String) -> String {
        if self.is_empty() { fallback() } else { self }
    }
}
