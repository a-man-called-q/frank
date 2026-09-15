//! Workspace read/write tool domain.

use std::path::{Path, PathBuf};

use frank_protocol::FilesystemPolicy;
use serde_json::{Value, json};

use super::{ToolExecutionContext, required_string, required_text};
use crate::helpers::canonicalize_allow_missing;

pub(crate) const WORKSPACE_READ_CAP: usize = 256 * 1024;
pub(crate) const WORKSPACE_ENTRY_CAP: usize = 512;

pub(crate) async fn dispatch(
    context: &ToolExecutionContext<'_>,
    name: &str,
    input: &Value,
) -> Result<Option<Value>, String> {
    let root = context.workspace_root;
    let result = match name {
        "workspace_list" => {
            let path = input
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let directory = confined_path(root, path, true)?;
            list_workspace(&directory)?
        }
        "workspace_read" => {
            // Older model sessions used the catalog's former `id` field. Keep
            // that alias readable while advertising the canonical `path` key.
            let path = input
                .get("path")
                .and_then(Value::as_str)
                .or_else(|| input.get("id").and_then(Value::as_str))
                .ok_or_else(|| "path is required".to_string())?;
            if path.trim().is_empty() || path.chars().any(char::is_control) {
                return Err("path is empty or invalid".into());
            }
            let path = path.to_owned();
            let file = confined_path(root, &path, false)?;
            let metadata = std::fs::symlink_metadata(&file)
                .map_err(|error| format!("cannot inspect file: {error}"))?;
            if !metadata.is_file() || metadata.len() > WORKSPACE_READ_CAP as u64 {
                return Err("workspace file is not a regular file or exceeds the read cap".into());
            }
            let content = frank_safeio::read_text_capped(&file, WORKSPACE_READ_CAP)
                .map_err(|error| error.to_string())?;
            json!({"path": path, "content": content})
        }
        "workspace_search" => {
            let query = required_string(input, "query")?;
            if query.len() > 512 {
                return Err("search query is too large".into());
            }
            let path = input
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let directory = confined_path(root, path, true)?;
            search_workspace(&directory, &query)?
        }
        "workspace_apply_patch" => {
            if context.agent.policy.filesystem == FilesystemPolicy::ReadOnly {
                return Err("agent filesystem policy is read-only".into());
            }
            let path = required_string(input, "path")?;
            let file = confined_path(root, &path, true)?;
            let content = if let Some(content) = input.get("content").and_then(Value::as_str) {
                if content.contains('\0') {
                    return Err("content is empty or invalid".into());
                }
                content.to_owned()
            } else {
                let old = required_text(input, "old")?;
                let new = required_text(input, "new")?;
                let existing = frank_safeio::read_text_capped(&file, WORKSPACE_READ_CAP)
                    .map_err(|error| error.to_string())?;
                if !existing.contains(&old) {
                    return Err("workspace replacement did not match".into());
                }
                existing.replacen(&old, &new, 1)
            };
            if content.len() > WORKSPACE_READ_CAP {
                return Err("workspace write exceeds the configured size cap".into());
            }
            frank_safeio::write_text_atomic(&file, &content, WORKSPACE_READ_CAP)
                .map_err(|error| error.to_string())?;
            json!({"path": path, "bytes": content.len()})
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}

pub(crate) fn confined_path(
    root: &Path,
    relative: &str,
    allow_directory: bool,
) -> Result<PathBuf, String> {
    if relative.len() > 4_096 || relative.chars().any(char::is_control) {
        return Err("workspace path is invalid".into());
    }
    let root =
        std::fs::canonicalize(root).map_err(|error| format!("worktree is unavailable: {error}"))?;
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("workspace path must stay relative to the task worktree".into());
    }
    let candidate = if relative.is_empty() {
        root.clone()
    } else {
        root.join(relative_path)
    };
    if let Ok(metadata) = std::fs::symlink_metadata(&candidate) {
        if metadata.file_type().is_symlink() {
            return Err("workspace symlinks are not allowed".into());
        }
        if !allow_directory && !metadata.is_file() {
            return Err("workspace target is not a regular file".into());
        }
    }
    let resolved = canonicalize_allow_missing(&candidate)
        .map_err(|error| format!("workspace path cannot be resolved: {error}"))?;
    if !resolved.starts_with(&root) {
        return Err("workspace path escapes the task worktree".into());
    }
    Ok(candidate)
}

fn list_workspace(directory: &Path) -> Result<Value, String> {
    let metadata = std::fs::symlink_metadata(directory).map_err(|error| error.to_string())?;
    if !metadata.is_dir() {
        return Err("workspace list target is not a directory".into());
    }
    let mut entries = Vec::new();
    collect_workspace_entries(directory, directory, 0, &mut entries)?;
    let truncated = entries.len() >= WORKSPACE_ENTRY_CAP;
    Ok(json!({"entries": entries, "truncated": truncated}))
}

fn collect_workspace_entries(
    root: &Path,
    directory: &Path,
    depth: usize,
    entries: &mut Vec<Value>,
) -> Result<(), String> {
    if entries.len() >= WORKSPACE_ENTRY_CAP {
        return Ok(());
    }
    let mut children = std::fs::read_dir(directory)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    children.sort_by_key(|entry| entry.file_name());
    for entry in children {
        if entries.len() >= WORKSPACE_ENTRY_CAP {
            break;
        }
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .into_owned();
        let kind = if metadata.file_type().is_symlink() {
            "symlink"
        } else if metadata.is_dir() {
            "directory"
        } else {
            "file"
        };
        entries.push(json!({"path": relative, "kind": kind, "bytes": metadata.len()}));
        if metadata.is_dir() && !metadata.file_type().is_symlink() && depth < 3 {
            collect_workspace_entries(root, &path, depth + 1, entries)?;
        }
    }
    Ok(())
}

fn search_workspace(directory: &Path, query: &str) -> Result<Value, String> {
    let mut files = Vec::new();
    collect_regular_files(directory, &mut files, 0)?;
    let mut matches = Vec::new();
    let mut truncated = false;
    for file in files {
        let metadata = std::fs::symlink_metadata(&file).map_err(|error| error.to_string())?;
        if metadata.len() > WORKSPACE_READ_CAP as u64 {
            continue;
        }
        let bytes = std::fs::read(&file).map_err(|error| error.to_string())?;
        let content = String::from_utf8_lossy(&bytes);
        for (line_number, line) in content.lines().enumerate() {
            if line.contains(query) {
                matches.push(json!({
                    "path": file.to_string_lossy(),
                    "line": line_number + 1,
                    "text": line,
                }));
                if matches.len() >= 256 {
                    truncated = true;
                    break;
                }
            }
        }
        if truncated {
            break;
        }
    }
    Ok(json!({"matches": matches, "truncated": truncated}))
}

fn collect_regular_files(
    directory: &Path,
    files: &mut Vec<PathBuf>,
    depth: usize,
) -> Result<(), String> {
    if depth > 6 || files.len() >= WORKSPACE_ENTRY_CAP {
        return Ok(());
    }
    let mut children = std::fs::read_dir(directory)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    children.sort_by_key(|entry| entry.file_name());
    for entry in children {
        if files.len() >= WORKSPACE_ENTRY_CAP {
            break;
        }
        let path = entry.path();
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            collect_regular_files(&path, files, depth + 1)?;
        } else if metadata.is_file() {
            files.push(path);
        }
    }
    Ok(())
}
