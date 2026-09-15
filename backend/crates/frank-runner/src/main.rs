use std::path::PathBuf;

use frank_protocol::{RunnerId, RunnerPathMapping};
use frank_runner::{HostRunner, RunnerConfig, read_secret_file, write_secret_file};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let daemon_url = args
        .next()
        .unwrap_or_else(|| "ws://127.0.0.1:37465/v2/runners/connect".into());
    let state_directory = runner_state_directory();
    if !state_directory.is_absolute() {
        return Err("runner state directory must be absolute".into());
    }
    std::fs::create_dir_all(&state_directory)?;
    let state_metadata = std::fs::symlink_metadata(&state_directory)?;
    if state_metadata.file_type().is_symlink() || !state_metadata.is_dir() {
        return Err("runner state directory must be a real directory".into());
    }
    let runner_id_path = state_directory.join("runner-id");
    let runner_id_value = std::env::var("FRANK_RUNNER_ID")
        .ok()
        .or(read_secret_file(&runner_id_path)?)
        .filter(|value| !value.trim().is_empty());
    let runner_id = match runner_id_value {
        Some(value) => RunnerId::parse(value.trim())?,
        None => {
            let id = RunnerId::new();
            write_secret_file(&runner_id_path, &id.to_string())?;
            id
        }
    };
    let token = std::env::var("FRANK_RUNNER_TOKEN").unwrap_or_default();
    let credential_file = std::env::var_os("FRANK_RUNNER_CREDENTIAL_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| state_directory.join("runner-credential"));
    let credential_file = if credential_file.is_absolute() {
        credential_file
    } else {
        state_directory.join(credential_file)
    };
    let runner_credential = std::env::var("FRANK_RUNNER_CREDENTIAL")
        .ok()
        .or(read_secret_file(&credential_file)?);
    let daemon_root =
        std::env::var("FRANK_DAEMON_PROJECT_ROOT").unwrap_or_else(|_| "/srv/frank/projects".into());
    let host_root = std::env::var("FRANK_HOST_PROJECT_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let mut mappings = Vec::with_capacity(2);
    match (
        std::env::var("FRANK_DAEMON_WORKTREE_ROOT"),
        std::env::var("FRANK_HOST_WORKTREE_ROOT"),
    ) {
        (Ok(daemon_worktree_root), Ok(host_worktree_root)) => mappings.push(RunnerPathMapping {
            daemon_root: daemon_worktree_root,
            host_root: host_worktree_root,
            project_id: None,
        }),
        (Err(_), Err(_)) => {}
        _ => {
            return Err(
                "FRANK_DAEMON_WORKTREE_ROOT and FRANK_HOST_WORKTREE_ROOT must be supplied together"
                    .into(),
            );
        }
    }
    // Worktree mappings precede the project mapping so a shared project root
    // cannot accidentally reinterpret a daemon worktree as a child of the
    // host checkout. Both roots are still canonicalized and constrained by
    // HostRunner before any job executes.
    mappings.push(RunnerPathMapping {
        daemon_root,
        host_root: host_root.to_string_lossy().into_owned(),
        project_id: None,
    });
    let runner = HostRunner::new(RunnerConfig {
        runner_id,
        name: std::env::var("FRANK_RUNNER_NAME").unwrap_or_else(|_| "Frank host runner".into()),
        daemon_url: daemon_url.clone(),
        one_time_token: token,
        runner_credential,
        mappings,
        install_root: Some(toolchain_root()),
        allowed_checks: frank_toolchain::builtin_manifests()
            .into_iter()
            .flat_map(|manifest| manifest.checks)
            .collect(),
    })?;
    runner
        .serve_websocket_with_credential_file(&daemon_url, credential_file)
        .await?;
    Ok(())
}

fn toolchain_root() -> PathBuf {
    if let Some(path) = std::env::var_os("FRANK_TOOLCHAIN_DIR") {
        return PathBuf::from(path);
    }
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join("Library/Application Support/Frank/toolchains");
    }
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(data_home).join("frank/toolchains");
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local/share/frank/toolchains")
}

fn runner_state_directory() -> PathBuf {
    if let Some(path) = std::env::var_os("FRANK_RUNNER_STATE_DIR") {
        return PathBuf::from(path);
    }
    toolchain_root()
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}
