//! Host-side executor for toolchain checks.
//!
//! `frankd` sends a durable job to this process over an outbound connection.
//! The runner maps only configured daemon paths, executes an argv vector
//! without a shell, caps output, and never needs Docker, sudo, or agent
//! credentials.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use std::{
    fs::OpenOptions,
    io::{Read, Write},
};

use frank_protocol::{
    CheckRunStatus, RunnerId, RunnerInstallJob, RunnerInstallResult, RunnerPathMapping,
    ToolchainArtifact, ToolchainArtifactSource, ToolchainCheck, ToolchainRequirementStatus,
};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::Message;

pub const MAX_OUTPUT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Error)]
pub enum RunnerError {
    #[error("runner path is outside the configured mapping")]
    PathDenied,
    #[error("runner path does not exist: {0}")]
    MissingPath(String),
    #[error("runner process failed: {0}")]
    Process(String),
    #[error("runner protocol failed: {0}")]
    Protocol(String),
    #[error("runner IO failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("runner JSON failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("runner artifact install failed: {0}")]
    Install(String),
}

pub type Result<T> = std::result::Result<T, RunnerError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerConfig {
    pub runner_id: RunnerId,
    pub name: String,
    pub daemon_url: String,
    pub one_time_token: String,
    /// Set after the first successful pairing. This credential is distinct
    /// from the short-lived one-time pairing token and is safe to reuse for
    /// reconnects until an owner revokes the runner.
    #[serde(default)]
    pub runner_credential: Option<String>,
    pub mappings: Vec<RunnerPathMapping>,
    /// Optional user-local root for downloaded toolchains. When configured,
    /// install destinations must remain below this canonical directory.
    #[serde(default)]
    pub install_root: Option<PathBuf>,
    /// Exact commands accepted by this runner. An empty list is retained for
    /// backwards-compatible test/config loading. Production pairing must
    /// populate it from the daemon's validated local/built-in manifests.
    #[serde(default)]
    pub allowed_checks: Vec<ToolchainCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerJob {
    pub id: String,
    pub runner_id: RunnerId,
    pub project_id: frank_protocol::ProjectId,
    #[serde(default)]
    pub task_id: Option<frank_protocol::TaskId>,
    pub daemon_worktree: String,
    pub check: ToolchainCheck,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerJobResult {
    pub id: String,
    pub runner_id: RunnerId,
    pub status: CheckRunStatus,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum RunnerFrame {
    Hello {
        runner_id: RunnerId,
        name: String,
        token: String,
    },
    Welcome {
        credential: String,
    },
    Run(RunnerJob),
    Result(RunnerJobResult),
    Install(RunnerInstallJob),
    InstallResult(RunnerInstallResult),
    Error {
        message: String,
    },
}

#[derive(Debug, Clone)]
pub struct HostRunner {
    config: RunnerConfig,
}

impl HostRunner {
    pub fn new(config: RunnerConfig) -> Result<Self> {
        if config.one_time_token.trim().is_empty()
            && config
                .runner_credential
                .as_deref()
                .is_none_or(str::is_empty)
        {
            return Err(RunnerError::Protocol("runner token is empty".into()));
        }
        if config.mappings.is_empty() {
            return Err(RunnerError::Protocol(
                "at least one daemon-to-host path mapping is required".into(),
            ));
        }
        for mapping in &config.mappings {
            validate_mapping(mapping)?;
        }
        for check in &config.allowed_checks {
            validate_check(check)?;
        }
        Ok(Self { config })
    }

    pub fn config(&self) -> &RunnerConfig {
        &self.config
    }

    pub fn map_worktree(&self, daemon_path: impl AsRef<Path>) -> Result<PathBuf> {
        self.map_worktree_for_project(daemon_path, None)
    }

    pub fn map_worktree_for_project(
        &self,
        daemon_path: impl AsRef<Path>,
        project_id: Option<frank_protocol::ProjectId>,
    ) -> Result<PathBuf> {
        let daemon_path = normalize_absolute(daemon_path.as_ref())?;
        for mapping in &self.config.mappings {
            if mapping.project_id.is_some() && mapping.project_id != project_id {
                continue;
            }
            let daemon_root = normalize_absolute(Path::new(&mapping.daemon_root))?;
            if let Ok(relative) = daemon_path.strip_prefix(&daemon_root) {
                let host_root = canonicalize_existing(Path::new(&mapping.host_root))?;
                let candidate = host_root.join(relative);
                let canonical_candidate = canonicalize_existing(&candidate)?;
                if !canonical_candidate.starts_with(&host_root) {
                    return Err(RunnerError::PathDenied);
                }
                if !canonical_candidate.is_dir() {
                    return Err(RunnerError::MissingPath(
                        canonical_candidate.display().to_string(),
                    ));
                }
                return Ok(canonical_candidate);
            }
        }
        Err(RunnerError::PathDenied)
    }

    pub async fn run_check(&self, job: &RunnerJob) -> Result<RunnerJobResult> {
        if job.runner_id != self.config.runner_id {
            return Err(RunnerError::Protocol("job is for another runner".into()));
        }
        let cwd = self.map_worktree_for_project(&job.daemon_worktree, Some(job.project_id))?;
        if !self.check_allowed_for_worktree(&job.check, &cwd) {
            return Err(RunnerError::Protocol(format!(
                "check '{}' is not allowed by this runner",
                job.check.id
            )));
        }
        let started = Instant::now();
        let mut command = Command::new(&job.check.program);
        command
            .env_clear()
            .args(&job.check.args)
            .current_dir(cwd)
            .envs(&job.check.environment)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        if let Some(path) = std::env::var_os("PATH") {
            command.env("PATH", path);
        }
        let mut child = command.spawn().map_err(|error| {
            RunnerError::Process(format!("could not start {}: {error}", job.check.program))
        })?;
        let stdout = child
            .stdout
            .take()
            .map(|reader| tokio::spawn(read_output(reader)));
        let stderr = child
            .stderr
            .take()
            .map(|reader| tokio::spawn(read_output(reader)));
        let process_status =
            timeout(Duration::from_secs(job.check.timeout_seconds), child.wait()).await;
        let (status, exit_code, stdout, stderr) = match process_status {
            Ok(result) => {
                let status = result?;
                (
                    if status.success() {
                        CheckRunStatus::Passed
                    } else {
                        CheckRunStatus::Failed
                    },
                    status.code(),
                    sanitize_runner_output(&join_output(stdout).await),
                    sanitize_runner_output(&join_output(stderr).await),
                )
            }
            Err(_) => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                (
                    CheckRunStatus::TimedOut,
                    None,
                    sanitize_runner_output(&join_output(stdout).await),
                    "check timed out".into(),
                )
            }
        };
        Ok(RunnerJobResult {
            id: job.id.clone(),
            runner_id: self.config.runner_id,
            status,
            exit_code,
            stdout,
            stderr,
            duration_ms: started.elapsed().as_millis() as u64,
        })
    }

    /// Built-ins are pinned into the runner configuration. Project-local
    /// manifests are loaded only after the daemon path has been mapped to a
    /// canonical host worktree, so an arbitrary daemon payload cannot widen
    /// the executable allowlist. A malformed local manifest fails closed.
    fn check_allowed_for_worktree(&self, check: &ToolchainCheck, cwd: &Path) -> bool {
        if self
            .config
            .allowed_checks
            .iter()
            .any(|allowed| allowed == check)
        {
            return true;
        }
        let directory = cwd.join(".frank").join("toolchains");
        let Ok(metadata) = std::fs::symlink_metadata(&directory) else {
            return false;
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return false;
        }
        frank_toolchain::load_local_manifests(directory)
            .ok()
            .is_some_and(|manifests| {
                manifests
                    .iter()
                    .flat_map(|manifest| manifest.checks.iter())
                    .any(|allowed| allowed == check)
            })
    }

    /// Install an exact artifact into a user-owned directory. URL artifacts
    /// are downloaded only after the caller has obtained the task grant and
    /// approval; host artifacts merely verify that the executable is present.
    /// Archives are kept as downloaded bytes in v1 so extraction policy stays
    /// explicit instead of becoming an implicit shell operation.
    pub async fn install_artifact(
        &self,
        artifact: &ToolchainArtifact,
        destination: impl AsRef<Path>,
    ) -> Result<PathBuf> {
        let destination = validate_install_destination(destination.as_ref())?;
        if let Some(root) = &self.config.install_root {
            let root = canonicalize_or_create_directory(root)?;
            if !destination.starts_with(&root) {
                return Err(RunnerError::PathDenied);
            }
        }
        match &artifact.source {
            ToolchainArtifactSource::Host { executable } => {
                verify_host_executable(executable, None, None).await
            }
            ToolchainArtifactSource::Url { url } => {
                if !url.starts_with("https://") {
                    return Err(RunnerError::Install(
                        "toolchain artifact URLs must use HTTPS".into(),
                    ));
                }
                let file_name = destination
                    .file_name()
                    .and_then(|value| value.to_str())
                    .ok_or_else(|| RunnerError::Install("destination has no file name".into()))?;
                let parent = destination
                    .parent()
                    .ok_or_else(|| RunnerError::Install("destination has no parent".into()))?;
                std::fs::create_dir_all(parent)?;
                if let Ok(metadata) = std::fs::symlink_metadata(&destination) {
                    if metadata.file_type().is_symlink() || !metadata.is_file() {
                        return Err(RunnerError::Install(
                            "install destination is not a regular file".into(),
                        ));
                    }
                    verify_artifact_file(&destination, artifact)?;
                    return Ok(destination);
                }
                let response = reqwest::Client::new()
                    .get(url)
                    .timeout(Duration::from_secs(15 * 60))
                    .send()
                    .await
                    .map_err(|error| RunnerError::Install(error.to_string()))?
                    .error_for_status()
                    .map_err(|error| RunnerError::Install(error.to_string()))?;
                if response
                    .content_length()
                    .is_some_and(|length| length != artifact.size_bytes)
                {
                    return Err(RunnerError::Install(
                        "toolchain response size does not match the manifest".into(),
                    ));
                }
                let part = parent.join(format!(".{file_name}.part"));
                let mut part_file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&part)
                    .map_err(|error| {
                        RunnerError::Install(format!(
                            "could not create an exclusive temporary artifact: {error}"
                        ))
                    })?;
                let mut stream = response.bytes_stream();
                let mut digest = Sha256::new();
                let mut total = 0_u64;
                while let Some(chunk) = stream.next().await {
                    let chunk = chunk.map_err(|error| RunnerError::Install(error.to_string()))?;
                    total = total.saturating_add(chunk.len() as u64);
                    if total > artifact.size_bytes {
                        let _ = std::fs::remove_file(&part);
                        return Err(RunnerError::Install(
                            "toolchain response exceeds the manifest size".into(),
                        ));
                    }
                    part_file.write_all(&chunk)?;
                    digest.update(&chunk);
                }
                if total != artifact.size_bytes
                    || !hex::encode(digest.finalize()).eq_ignore_ascii_case(&artifact.sha256)
                {
                    let _ = std::fs::remove_file(&part);
                    return Err(RunnerError::Install(
                        "downloaded toolchain artifact failed size or checksum verification".into(),
                    ));
                }
                part_file.sync_all()?;
                drop(part_file);
                if std::fs::symlink_metadata(&destination).is_ok() {
                    let _ = std::fs::remove_file(&part);
                    return Err(RunnerError::Install(
                        "install destination appeared during download".into(),
                    ));
                }
                std::fs::rename(&part, &destination)?;
                Ok(destination)
            }
        }
    }

    pub async fn install_job(&self, job: &RunnerInstallJob) -> Result<RunnerInstallResult> {
        if job.runner_id != self.config.runner_id {
            return Err(RunnerError::Protocol(
                "install job is for another runner".into(),
            ));
        }
        if job.manifest_id.trim().is_empty()
            || job.version.trim().is_empty()
            || job.daemon_worktree.trim().is_empty()
            || !safe_relative_install_path(&job.install_relative_path)
        {
            return Err(RunnerError::PathDenied);
        }
        let worktree = self.map_worktree_for_project(&job.daemon_worktree, Some(job.project_id))?;
        let install_root = self
            .config
            .install_root
            .as_deref()
            .ok_or_else(|| RunnerError::Install("runner has no install root".into()))?;
        let install_root = canonicalize_or_create_directory(install_root)?;
        let destination = install_root.join(&job.install_relative_path);
        let path = match &job.artifact.source {
            ToolchainArtifactSource::Host { executable } => {
                verify_host_executable(executable, Some(&worktree), Some(&job.version)).await?
            }
            ToolchainArtifactSource::Url { .. } => {
                self.install_artifact(&job.artifact, destination).await?
            }
        };
        Ok(RunnerInstallResult {
            id: job.id.clone(),
            runner_id: self.config.runner_id,
            status: ToolchainRequirementStatus::Ready,
            installed_path: Some(path.to_string_lossy().into_owned()),
            diagnostic: None,
        })
    }

    /// Keep an outbound WebSocket connected. The one-time token is exchanged
    /// for a durable runner credential on the first connection; subsequent
    /// reconnects never reuse the pairing secret.
    pub async fn serve_websocket(self, url: &str) -> Result<()> {
        let credential = self.config.runner_credential.clone();
        self.serve_websocket_loop(url, credential, None).await
    }

    /// Same reconnect loop, with the durable credential stored in a private
    /// user-local file after pairing. Environment variables remain useful for
    /// one-shot/container launches, while a host service survives restarts
    /// without asking the owner to pair again.
    pub async fn serve_websocket_with_credential_file(
        self,
        url: &str,
        credential_file: impl AsRef<Path>,
    ) -> Result<()> {
        let credential_file = credential_file.as_ref().to_path_buf();
        let credential = self
            .config
            .runner_credential
            .clone()
            .or(read_secret_file(&credential_file)?);
        self.serve_websocket_loop(url, credential, Some(credential_file))
            .await
    }

    async fn serve_websocket_loop(
        self,
        url: &str,
        mut credential: Option<String>,
        credential_file: Option<PathBuf>,
    ) -> Result<()> {
        let mut backoff_seconds = 1_u64;
        loop {
            match self.serve_websocket_once(url, credential.as_deref()).await {
                Ok(Some(next_credential)) => {
                    if let Some(path) = &credential_file
                        && read_secret_file(path)?.as_deref() != Some(next_credential.as_str())
                    {
                        write_secret_file(path, &next_credential)?;
                    }
                    credential = Some(next_credential);
                    backoff_seconds = 1;
                }
                Ok(None) => {
                    backoff_seconds = 1;
                }
                Err(error) => {
                    // A network interruption is recoverable. Protocol and
                    // process errors still use the same bounded reconnect
                    // loop so the service can survive a daemon restart.
                    let _ = error;
                }
            }
            tokio::time::sleep(Duration::from_secs(backoff_seconds)).await;
            backoff_seconds = backoff_seconds.saturating_mul(2).min(30);
        }
    }

    /// Run one connection, primarily useful for deterministic integration
    /// tests and for embedders that own their own retry policy.
    pub async fn serve_websocket_once(
        &self,
        url: &str,
        credential: Option<&str>,
    ) -> Result<Option<String>> {
        let (mut socket, _) = tokio_tungstenite::connect_async(url)
            .await
            .map_err(|error| RunnerError::Protocol(error.to_string()))?;
        let token = credential.unwrap_or(&self.config.one_time_token);
        if token.trim().is_empty() {
            return Err(RunnerError::Protocol(
                "runner has neither a pairing token nor a durable credential".into(),
            ));
        }
        send_frame(
            &mut socket,
            &RunnerFrame::Hello {
                runner_id: self.config.runner_id,
                name: self.config.name.clone(),
                token: token.to_string(),
            },
        )
        .await?;
        let mut next_credential = None;
        while let Some(frame) = socket.next().await {
            let frame = frame.map_err(|error| RunnerError::Protocol(error.to_string()))?;
            let Message::Text(payload) = frame else {
                continue;
            };
            let frame: RunnerFrame = serde_json::from_str(&payload)?;
            match frame {
                RunnerFrame::Welcome { credential } => {
                    next_credential = Some(credential);
                }
                RunnerFrame::Run(job) => {
                    let result = match self.run_check(&job).await {
                        Ok(result) => result,
                        Err(error) => RunnerJobResult {
                            id: job.id,
                            runner_id: self.config.runner_id,
                            status: CheckRunStatus::Failed,
                            exit_code: None,
                            stdout: String::new(),
                            stderr: error.to_string(),
                            duration_ms: 0,
                        },
                    };
                    send_frame(&mut socket, &RunnerFrame::Result(result)).await?;
                }
                RunnerFrame::Install(job) => {
                    let result = match self.install_job(&job).await {
                        Ok(result) => result,
                        Err(error) => RunnerInstallResult {
                            id: job.id,
                            runner_id: self.config.runner_id,
                            status: ToolchainRequirementStatus::Failed,
                            installed_path: None,
                            diagnostic: Some(error.to_string()),
                        },
                    };
                    send_frame(&mut socket, &RunnerFrame::InstallResult(result)).await?;
                }
                RunnerFrame::Error { message } => {
                    return Err(RunnerError::Protocol(message));
                }
                RunnerFrame::Hello { .. }
                | RunnerFrame::Result(_)
                | RunnerFrame::InstallResult(_) => {}
            }
        }
        Ok(next_credential)
    }
}

async fn verify_host_executable(
    executable: &str,
    cwd: Option<&Path>,
    expected_version: Option<&str>,
) -> Result<PathBuf> {
    let mut command = Command::new(executable);
    command.env_clear();
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    let output = command
        .arg("--version")
        .output()
        .await
        .map_err(|error| RunnerError::Install(format!("{executable}: {error}")))?;
    if !output.status.success() {
        return Err(RunnerError::Install(format!(
            "host executable '{executable}' is not usable"
        )));
    }
    if let Some(expected) = expected_version.filter(|version| {
        version
            .chars()
            .next()
            .is_some_and(|character| character.is_ascii_digit())
    }) {
        let version_output = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if !version_output.contains(expected) {
            return Err(RunnerError::Install(format!(
                "host executable '{executable}' did not report required version '{expected}'"
            )));
        }
    }
    Ok(PathBuf::from(executable))
}

async fn send_frame<S>(socket: &mut S, frame: &RunnerFrame) -> Result<()>
where
    S: SinkExt<Message> + Unpin,
    S::Error: std::fmt::Display,
{
    socket
        .send(Message::Text(serde_json::to_string(frame)?.into()))
        .await
        .map_err(|error| RunnerError::Protocol(error.to_string()))
}

/// Read a bounded, symlink-free runner state value.
pub fn read_secret_file(path: &Path) -> Result<Option<String>> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(RunnerError::Io(error)),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 4096 {
        return Err(RunnerError::PathDenied);
    }
    let value = std::fs::read_to_string(path)?.trim().to_string();
    Ok((!value.is_empty()).then_some(value))
}

/// Atomically persist a bounded runner state value with owner-only mode on
/// Unix hosts.
pub fn write_secret_file(path: &Path, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(RunnerError::Protocol("runner credential is empty".into()));
    }
    let path = validate_install_destination(path)?;
    let parent = path.parent().ok_or(RunnerError::PathDenied)?;
    std::fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(RunnerError::PathDenied)?;
    let part = parent.join(format!(".{file_name}.part"));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&part)?;
    file.write_all(value.trim().as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(part, path)?;
    Ok(())
}

async fn read_output(mut reader: impl AsyncRead + Unpin) -> String {
    const TRUNCATION_MARKER: &str = "\n[output truncated by frank-runner]";
    let max_payload = MAX_OUTPUT_BYTES.saturating_sub(TRUNCATION_MARKER.len());
    let mut output = Vec::with_capacity(MAX_OUTPUT_BYTES.min(64 * 1024));
    let mut buffer = [0_u8; 16 * 1024];
    let mut truncated = false;
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(length) => {
                let remaining = max_payload.saturating_sub(output.len());
                if length > remaining {
                    output.extend_from_slice(&buffer[..remaining]);
                    truncated = true;
                } else {
                    output.extend_from_slice(&buffer[..length]);
                }
            }
            Err(_) => break,
        }
    }
    let mut output = String::from_utf8_lossy(&output).into_owned();
    if truncated {
        output.push_str(TRUNCATION_MARKER);
    }
    output
}

async fn join_output(reader: Option<tokio::task::JoinHandle<String>>) -> String {
    match reader {
        Some(reader) => reader.await.unwrap_or_default(),
        None => String::new(),
    }
}

/// Keep command output useful for the Journal without allowing common bearer
/// and API-key-shaped values to be copied into the durable check projection.
fn sanitize_runner_output(value: &str) -> String {
    let mut output = value.to_string();
    let mut search_from = 0;
    while let Some(relative) = output[search_from..].find("Bearer ") {
        let prefix_start = search_from + relative;
        let token_start = prefix_start + "Bearer ".len();
        let token_end = output[token_start..]
            .find(char::is_whitespace)
            .map(|offset| token_start + offset)
            .unwrap_or(output.len());
        output.replace_range(token_start..token_end, "[redacted]");
        search_from = token_start + "[redacted]".len();
    }
    for prefix in ["sk-", "rk-"] {
        while let Some(start) = output.find(prefix) {
            let end = output[start..]
                .find(char::is_whitespace)
                .map(|offset| start + offset)
                .unwrap_or(output.len());
            output.replace_range(start..end, "[redacted]");
        }
    }
    for key in ["api_key=", "apikey=", "token=", "secret=", "password="] {
        let mut search_from = 0;
        while let Some(relative) = output[search_from..].find(key) {
            let start = search_from + relative + key.len();
            let end = output[start..]
                .find(char::is_whitespace)
                .map(|offset| start + offset)
                .unwrap_or(output.len());
            output.replace_range(start..end, "[redacted]");
            search_from = start + "[redacted]".len();
        }
    }
    output.chars().take(MAX_OUTPUT_BYTES).collect()
}

fn validate_mapping(mapping: &RunnerPathMapping) -> Result<()> {
    let daemon = Path::new(&mapping.daemon_root);
    let host = Path::new(&mapping.host_root);
    if !daemon.is_absolute() || !host.is_absolute() {
        return Err(RunnerError::Protocol(
            "runner mappings must use absolute paths".into(),
        ));
    }
    if daemon_root_has_parent_escape(daemon) || daemon_root_has_parent_escape(host) {
        return Err(RunnerError::PathDenied);
    }
    Ok(())
}

fn validate_check(check: &ToolchainCheck) -> Result<()> {
    if check.id.trim().is_empty()
        || check.program.trim().is_empty()
        || check.program.contains('/')
        || check.program.contains('\\')
        || check.program.contains('\0')
        || check.timeout_seconds == 0
        || check.timeout_seconds > 86_400
        || check.args.iter().any(|arg| arg.contains('\0'))
        || check.environment.iter().any(|(key, value)| {
            key.is_empty()
                || key.contains('=')
                || key.contains('\0')
                || value.contains('\0')
                || is_sensitive_environment_key(key)
        })
        || is_forbidden_program(&check.program)
    {
        return Err(RunnerError::Protocol(format!(
            "invalid check specification '{}'",
            check.id
        )));
    }
    Ok(())
}

fn is_forbidden_program(program: &str) -> bool {
    matches!(
        program.to_ascii_lowercase().as_str(),
        "sh" | "bash"
            | "zsh"
            | "fish"
            | "dash"
            | "cmd"
            | "powershell"
            | "pwsh"
            | "sudo"
            | "doas"
            | "pkexec"
    )
}

fn is_sensitive_environment_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "token",
        "secret",
        "password",
        "api_key",
        "apikey",
        "credential",
        "authorization",
        "private_key",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

fn validate_install_destination(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(RunnerError::PathDenied);
    }
    let mut current = PathBuf::from(std::path::MAIN_SEPARATOR.to_string());
    let components = path.components().collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        if matches!(component, std::path::Component::RootDir) {
            continue;
        }
        current.push(component.as_os_str());
        if let Ok(metadata) = std::fs::symlink_metadata(&current)
            && (metadata.file_type().is_symlink()
                || (index + 1 < components.len() && !metadata.is_dir()))
        {
            return Err(RunnerError::PathDenied);
        }
    }
    Ok(path.to_path_buf())
}

fn safe_relative_install_path(path: &str) -> bool {
    let candidate = Path::new(path);
    !path.trim().is_empty()
        && !candidate.is_absolute()
        && !path.contains('\0')
        && !candidate
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
}

fn verify_artifact_file(path: &Path, artifact: &ToolchainArtifact) -> Result<()> {
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() != artifact.size_bytes
    {
        return Err(RunnerError::Install(
            "existing toolchain artifact failed size verification".into(),
        ));
    }
    let mut file = std::fs::File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let length = file.read(&mut buffer)?;
        if length == 0 {
            break;
        }
        digest.update(&buffer[..length]);
    }
    if !hex::encode(digest.finalize()).eq_ignore_ascii_case(&artifact.sha256) {
        return Err(RunnerError::Install(
            "existing toolchain artifact failed checksum verification".into(),
        ));
    }
    Ok(())
}

fn canonicalize_or_create_directory(path: &Path) -> Result<PathBuf> {
    validate_install_destination(path)?;
    std::fs::create_dir_all(path)?;
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(RunnerError::PathDenied);
    }
    Ok(std::fs::canonicalize(path)?)
}

fn daemon_root_has_parent_escape(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
}

fn canonicalize_existing(path: &Path) -> Result<PathBuf> {
    if !path.exists() {
        return Err(RunnerError::MissingPath(path.display().to_string()));
    }
    Ok(std::fs::canonicalize(path)?)
}

fn normalize_absolute(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err(RunnerError::PathDenied);
    }
    let mut normalized = PathBuf::from(std::path::MAIN_SEPARATOR.to_string());
    for component in path.components() {
        match component {
            std::path::Component::RootDir => {}
            std::path::Component::Normal(value) => normalized.push(value),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => return Err(RunnerError::PathDenied),
            std::path::Component::Prefix(_) => return Err(RunnerError::PathDenied),
        }
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn runner(root: &Path) -> HostRunner {
        HostRunner::new(RunnerConfig {
            runner_id: RunnerId::new(),
            name: "test".into(),
            daemon_url: "ws://localhost".into(),
            one_time_token: "token".into(),
            runner_credential: None,
            mappings: vec![RunnerPathMapping {
                daemon_root: "/srv/project".into(),
                host_root: root.to_string_lossy().into_owned(),
                project_id: None,
            }],
            install_root: None,
            allowed_checks: vec![ToolchainCheck {
                id: "echo".into(),
                program: "printf".into(),
                args: vec!["ok".into()],
                environment: BTreeMap::new(),
                timeout_seconds: 10,
            }],
        })
        .unwrap()
    }

    #[tokio::test]
    async fn maps_only_inside_canonical_root_and_runs_argv_without_shell() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("worktree");
        std::fs::create_dir(&project).unwrap();
        let host = runner(root.path());
        let mapped = host.map_worktree("/srv/project/worktree").unwrap();
        assert_eq!(mapped, project.canonicalize().unwrap());
        assert!(matches!(
            host.map_worktree("/tmp/outside"),
            Err(RunnerError::PathDenied)
        ));

        let job = RunnerJob {
            id: "job".into(),
            runner_id: host.config().runner_id,
            project_id: frank_protocol::ProjectId::new(),
            task_id: None,
            daemon_worktree: "/srv/project/worktree".into(),
            check: ToolchainCheck {
                id: "echo".into(),
                program: "printf".into(),
                args: vec!["ok".into()],
                environment: BTreeMap::new(),
                timeout_seconds: 10,
            },
        };
        let result = host.run_check(&job).await.unwrap();
        assert_eq!(result.status, CheckRunStatus::Passed);
        assert_eq!(result.stdout, "ok");
    }

    #[tokio::test]
    async fn host_artifact_install_verifies_the_pinned_host_executable() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().canonicalize().unwrap();
        let host = runner(&root_path);
        let artifact = ToolchainArtifact {
            platform: "any".into(),
            source: ToolchainArtifactSource::Host {
                executable: "rustc".into(),
            },
            sha256: "0".repeat(64),
            size_bytes: 1,
            archive: None,
        };
        let destination = root_path.join("toolchains").join("rustc");
        let resolved = host
            .install_artifact(&artifact, &destination)
            .await
            .unwrap();
        assert_eq!(resolved, PathBuf::from("rustc"));
        assert!(!destination.exists());
    }

    #[tokio::test]
    async fn artifact_install_rejects_insecure_urls_and_unsafe_destinations() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().canonicalize().unwrap();
        let host = runner(&root_path);
        let artifact = ToolchainArtifact {
            platform: "any".into(),
            source: ToolchainArtifactSource::Url {
                url: "http://example.invalid/toolchain.tar.gz".into(),
            },
            sha256: "ab".repeat(32),
            size_bytes: 4,
            archive: Some("tar.gz".into()),
        };
        assert!(matches!(
            host.install_artifact(&artifact, root_path.join("artifact")).await,
            Err(RunnerError::Install(message)) if message.contains("HTTPS")
        ));
        assert!(matches!(
            host.install_artifact(&artifact, Path::new("relative/artifact"))
                .await,
            Err(RunnerError::PathDenied)
        ));
    }

    #[test]
    fn invalid_runner_checks_fail_closed_before_process_execution() {
        let root = tempfile::tempdir().unwrap();
        let mut config = runner(root.path()).config().clone();
        config.allowed_checks = vec![ToolchainCheck {
            id: "shell".into(),
            program: "sh".into(),
            args: Vec::new(),
            environment: BTreeMap::new(),
            timeout_seconds: 10,
        }];
        assert!(matches!(
            HostRunner::new(config),
            Err(RunnerError::Protocol(message)) if message.contains("invalid check")
        ));
    }

    #[test]
    fn runner_output_redacts_common_secret_shapes() {
        let output = sanitize_runner_output(
            "Bearer super-secret sk-live-value token=abc password=hunter2 visible",
        );
        assert!(!output.contains("super-secret"));
        assert!(!output.contains("sk-live-value"));
        assert!(!output.contains("abc"));
        assert!(!output.contains("hunter2"));
        assert!(output.contains("visible"));
    }

    #[test]
    fn project_bound_mapping_rejects_a_different_project() {
        let root = tempfile::tempdir().unwrap();
        let project = root.path().join("worktree");
        std::fs::create_dir(&project).unwrap();
        let project_id = frank_protocol::ProjectId::new();
        let host = HostRunner::new(RunnerConfig {
            runner_id: RunnerId::new(),
            name: "scoped".into(),
            daemon_url: "ws://localhost".into(),
            one_time_token: "token".into(),
            runner_credential: None,
            mappings: vec![RunnerPathMapping {
                daemon_root: "/srv/project".into(),
                host_root: root.path().to_string_lossy().into_owned(),
                project_id: Some(project_id),
            }],
            install_root: None,
            allowed_checks: Vec::new(),
        })
        .unwrap();
        assert!(matches!(
            host.map_worktree_for_project(
                "/srv/project/worktree",
                Some(frank_protocol::ProjectId::new())
            ),
            Err(RunnerError::PathDenied)
        ));
        assert_eq!(
            host.map_worktree_for_project("/srv/project/worktree", Some(project_id))
                .unwrap(),
            project.canonicalize().unwrap()
        );
    }

    #[test]
    fn runner_state_values_are_private_and_symlink_free() {
        let root = tempfile::tempdir().unwrap();
        // macOS exposes the temporary directory through `/var`, which is a
        // system symlink. Use its canonical spelling so the test exercises
        // the application-owned path checks rather than rejecting that alias.
        let root_path = root.path().canonicalize().unwrap();
        let path = root_path.join("state").join("credential");
        write_secret_file(&path, "runner-secret").unwrap();
        assert_eq!(
            read_secret_file(&path).unwrap().as_deref(),
            Some("runner-secret")
        );
        assert!(matches!(
            write_secret_file(&path, ""),
            Err(RunnerError::Protocol(message)) if message.contains("empty")
        ));
        #[cfg(unix)]
        use std::os::unix::fs::PermissionsExt;
        #[cfg(unix)]
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn rejects_parent_components_in_mapping() {
        let result = HostRunner::new(RunnerConfig {
            runner_id: RunnerId::new(),
            name: "bad".into(),
            daemon_url: "ws://localhost".into(),
            one_time_token: "token".into(),
            runner_credential: None,
            mappings: vec![RunnerPathMapping {
                daemon_root: "/srv/../etc".into(),
                host_root: "/tmp".into(),
                project_id: None,
            }],
            install_root: None,
            allowed_checks: Vec::new(),
        });
        assert!(matches!(result, Err(RunnerError::PathDenied)));
    }
}
