//! Shell and terminal tool domain.

use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use frank_protocol::MAX_COMMAND_BODY_BYTES;
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command as AsyncCommand;

use super::{ToolExecutionContext, required_string};
use crate::{safe_terminal_environment, validate_terminal_command};

const SHELL_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) async fn dispatch(
    context: &ToolExecutionContext<'_>,
    name: &str,
    input: &Value,
) -> Result<Option<Value>, String> {
    let command = required_string(input, "command")?;
    let result = match name {
        "shell_exec" => execute_shell(&command, context.workspace_root).await?,
        "terminal_execute" => {
            let worktree = context
                .task
                .worktree
                .as_deref()
                .ok_or_else(|| "terminal execution requires a task worktree".to_string())?;
            let worktree = std::fs::canonicalize(worktree)
                .map_err(|error| format!("task worktree is unavailable: {error}"))?;
            validate_terminal_command(&command).map_err(|error| error.to_string())?;
            execute_terminal(&command, &worktree).await?
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}

pub(crate) async fn execute_shell(command: &str, cwd: &Path) -> Result<Value, String> {
    if command.len() > MAX_COMMAND_BODY_BYTES || command.contains('\0') {
        return Err("shell command is empty or too large".into());
    }
    execute_process(command, cwd, "shell").await
}

async fn execute_terminal(command: &str, cwd: &Path) -> Result<Value, String> {
    execute_process(command, cwd, "terminal").await
}

async fn execute_process(command: &str, cwd: &Path, label: &str) -> Result<Value, String> {
    let mut process = if cfg!(windows) {
        let mut process = AsyncCommand::new("cmd");
        process.args(["/C", command]);
        process
    } else {
        let mut process = AsyncCommand::new("/bin/sh");
        process.args(["-c", command]);
        process
    };
    process
        .current_dir(cwd)
        .env_clear()
        .envs(safe_terminal_environment())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = process.spawn().map_err(|error| error.to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("{label} stdout is unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("{label} stderr is unavailable"))?;
    let stdout_task = tokio::spawn(read_pipe_capped(stdout));
    let stderr_task = tokio::spawn(read_pipe_capped(stderr));
    let started = Instant::now();
    let status = match tokio::time::timeout(SHELL_TIMEOUT, child.wait()).await {
        Ok(result) => result.map_err(|error| error.to_string())?,
        Err(_) => {
            let _ = child.kill().await;
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            return Err(format!("{label} command timed out after 30 seconds"));
        }
    };
    let stdout = stdout_task
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
    let stderr = stderr_task
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
    Ok(json!({
        "status": status.code(),
        "success": status.success(),
        "stdout": String::from_utf8_lossy(&stdout.0),
        "stderr": String::from_utf8_lossy(&stderr.0),
        "truncated": stdout.1 || stderr.1,
        "duration_ms": started.elapsed().as_millis(),
    }))
}

pub(crate) async fn read_pipe_capped<R>(mut reader: R) -> std::io::Result<(Vec<u8>, bool)>
where
    R: AsyncRead + Unpin,
{
    let mut output = Vec::new();
    let mut truncated = false;
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        if output.len() < MAX_COMMAND_BODY_BYTES {
            let keep = read.min(MAX_COMMAND_BODY_BYTES - output.len());
            output.extend_from_slice(&buffer[..keep]);
            if keep < read {
                truncated = true;
            }
        } else {
            truncated = true;
        }
    }
    Ok((output, truncated))
}
