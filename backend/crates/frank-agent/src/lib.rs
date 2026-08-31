//! Provider and terminal runtime adapters.
//!
//! Frank controls providers through structured streams.  The adapters never
//! scrape a provider TUI and never expose the provider's app-server to the
//! network.  A provider process is owned by `frankd`, scoped to one agent/task,
//! and cleaned up when its session is stopped or dropped.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use frank_protocol::{AgentPolicy, ApprovalDecision, Provider, ProviderCapability, TerminalFrame};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{Mutex, Notify, broadcast, mpsc};

pub mod terminal;

#[derive(Debug, Error)]
pub enum ProviderError {
    #[error("provider executable is not available: {0}")]
    Unavailable(String),
    #[error("provider protocol is unsupported: {0}")]
    Unsupported(String),
    #[error("provider process failed: {0}")]
    Process(String),
    #[error("provider session is closed")]
    Closed,
    #[error("provider emitted malformed structured data: {0}")]
    Malformed(String),
    #[error("provider operation timed out")]
    Timeout,
    #[error("policy denied provider operation: {0}")]
    PolicyDenied(String),
}

pub type Result<T> = std::result::Result<T, ProviderError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeProbe {
    pub capability: ProviderCapability,
    pub executable_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartRequest {
    pub agent_id: String,
    pub task_id: Option<String>,
    pub cwd: String,
    pub instructions: String,
    pub policy: AgentPolicy,
    pub model: Option<String>,
    pub resume_session_id: Option<String>,
    /// Local API address and certificate pin used by the scoped MCP bridge.
    /// These values are transport metadata, never prompt content or provider
    /// credentials.
    pub server_url: Option<String>,
    pub server_certificate_fingerprint: Option<String>,
    /// Issued by frankd and forwarded only to the local MCP bridge. It is
    /// never included in provider prompts or exposed on the network.
    pub session_capability: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderMessage {
    pub role: String,
    pub content: String,
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum RuntimeEvent {
    Ready {
        provider_session_id: String,
    },
    Text {
        text: String,
    },
    ToolCall {
        name: String,
        input: Value,
    },
    ApprovalRequest {
        operation: String,
        reason: String,
        cwd: Option<String>,
    },
    Usage(UsageTelemetry),
    Stopped {
        code: Option<i32>,
    },
    Error {
        message: String,
    },
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageTelemetry {
    pub measured_input_tokens: Option<u64>,
    pub measured_output_tokens: Option<u64>,
    pub estimated_input_tokens: Option<u64>,
    pub estimated_output_tokens: Option<u64>,
    pub cost_micros: Option<u64>,
}

impl UsageTelemetry {
    pub fn measured(input: Option<u64>, output: Option<u64>, cost_micros: Option<u64>) -> Self {
        Self {
            measured_input_tokens: input,
            measured_output_tokens: output,
            estimated_input_tokens: None,
            estimated_output_tokens: None,
            cost_micros,
        }
    }

    pub fn estimated(input: Option<u64>, output: Option<u64>) -> Self {
        Self {
            measured_input_tokens: None,
            measured_output_tokens: None,
            estimated_input_tokens: input,
            estimated_output_tokens: output,
            cost_micros: None,
        }
    }

    /// A measured budget can only be enforced from measured telemetry.  This
    /// intentionally ignores estimates so a warning cannot become a hard
    /// stop by accident.
    pub fn measured_tokens(&self) -> Option<u64> {
        self.measured_input_tokens
            .zip(self.measured_output_tokens)
            .map(|(input, output)| input.saturating_add(output))
    }
}

#[derive(Debug)]
pub struct RuntimeSession {
    pub provider: Provider,
    pub stable_session_id: String,
    pub provider_session_id: Arc<Mutex<Option<String>>>,
    child: Arc<Mutex<Option<Child>>>,
    stdin: Arc<Mutex<Option<ChildStdin>>>,
    stdout: Arc<Mutex<Option<ChildStdout>>>,
    request: StartRequest,
    codex_initialized: Arc<Mutex<bool>>,
    codex_ready: Arc<Notify>,
    event_broadcast: broadcast::Sender<RuntimeEvent>,
    event_started: AtomicBool,
    /// Codex server requests carry a JSON-RPC id that is not part of Frank's
    /// normalized event. Keep the id in a short FIFO so an operator decision
    /// can answer the exact provider callback after the durable approval row
    /// is updated.
    pending_approvals: Arc<Mutex<VecDeque<PendingApproval>>>,
}

#[derive(Debug, Clone)]
struct PendingApproval {
    operation: String,
    request_id: Value,
}

impl RuntimeSession {
    fn new(
        provider: Provider,
        stable_session_id: String,
        child: Child,
        request: StartRequest,
    ) -> Self {
        let mut child = child;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let (event_broadcast, _) = broadcast::channel(256);
        Self {
            provider,
            stable_session_id,
            provider_session_id: Arc::new(Mutex::new(request.resume_session_id.clone())),
            child: Arc::new(Mutex::new(Some(child))),
            stdin: Arc::new(Mutex::new(stdin)),
            stdout: Arc::new(Mutex::new(stdout)),
            request,
            codex_initialized: Arc::new(Mutex::new(false)),
            codex_ready: Arc::new(Notify::new()),
            event_broadcast,
            event_started: AtomicBool::new(false),
            pending_approvals: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub async fn send(&self, message: &ProviderMessage) -> Result<()> {
        if message.content.len() > frank_protocol::MAX_MESSAGE_BODY_BYTES {
            return Err(ProviderError::Malformed(
                "provider message exceeds the configured size cap".into(),
            ));
        }
        if self.provider == Provider::Codex {
            self.ensure_codex_session().await?;
        }
        let payload = match self.provider {
            Provider::Claude => serde_json::json!({
                "type": "user",
                "message": {
                    "role": if message.role.trim().is_empty() { "user" } else { message.role.as_str() },
                    "content": [{"type": "text", "text": message.content}],
                },
            }),
            Provider::Codex => {
                let thread_id = self
                    .provider_session_id
                    .lock()
                    .await
                    .clone()
                    .ok_or_else(|| {
                        ProviderError::Process(
                            "Codex app-server did not return a thread id after startup".into(),
                        )
                    })?;
                serde_json::json!({
                    "id": uuid::Uuid::new_v4().to_string(),
                    "method": "turn/start",
                    "params": {
                        "threadId": thread_id,
                        "input": [{"type": "text", "text": message.content}],
                    },
                })
            }
        };
        let json = serde_json::to_vec(&payload)
            .map_err(|error| ProviderError::Malformed(error.to_string()))?;
        self.write_json_line(&json).await
    }

    async fn ensure_codex_session(&self) -> Result<()> {
        let should_initialize = {
            let mut initialized = self.codex_initialized.lock().await;
            if *initialized {
                false
            } else {
                *initialized = true;
                true
            }
        };
        if should_initialize {
            let initialize = serde_json::json!({
                "id": uuid::Uuid::new_v4().to_string(),
                "method": "initialize",
                "params": {
                    "clientInfo": {
                        "name": "frank",
                        "title": "Frank orchestrator",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                    "capabilities": {},
                },
            });
            let initialized = serde_json::json!({
                "method": "initialized",
                "params": {},
            });
            let thread_method = if self.request.resume_session_id.is_some() {
                "thread/resume"
            } else {
                "thread/start"
            };
            let mut params = serde_json::Map::new();
            if let Some(thread_id) = &self.request.resume_session_id {
                params.insert("threadId".into(), Value::String(thread_id.clone()));
            } else {
                params.insert("cwd".into(), Value::String(self.request.cwd.clone()));
                if let Some(model) = &self.request.model {
                    params.insert("model".into(), Value::String(model.clone()));
                }
            }
            let thread = serde_json::json!({
                "id": uuid::Uuid::new_v4().to_string(),
                "method": thread_method,
                "params": Value::Object(params),
            });
            for payload in [initialize, initialized, thread] {
                let json = serde_json::to_vec(&payload)
                    .map_err(|error| ProviderError::Malformed(error.to_string()))?;
                if let Err(error) = self.write_json_line(&json).await {
                    *self.codex_initialized.lock().await = false;
                    return Err(error);
                }
            }
        }

        // A new thread receives its real provider id asynchronously in the
        // `thread/started` notification or the `thread/start` response.  Do
        // not invent an id for `turn/start`: wait briefly for that structured
        // handshake and fail closed if the provider does not implement it.
        if self.provider_session_id.lock().await.is_none() {
            let deadline = std::time::Duration::from_secs(10);
            let wait = async {
                loop {
                    if self.provider_session_id.lock().await.is_some() {
                        return;
                    }
                    self.codex_ready.notified().await;
                }
            };
            if tokio::time::timeout(deadline, wait).await.is_err() {
                *self.codex_initialized.lock().await = false;
                return Err(ProviderError::Timeout);
            }
        }
        Ok(())
    }

    async fn write_json_line(&self, json: &[u8]) -> Result<()> {
        let mut stdin = self.stdin.lock().await;
        let writer = stdin.as_mut().ok_or(ProviderError::Closed)?;
        writer
            .write_all(json)
            .await
            .map_err(|error| ProviderError::Process(error.to_string()))?;
        writer
            .write_all(b"\n")
            .await
            .map_err(|error| ProviderError::Process(error.to_string()))?;
        writer
            .flush()
            .await
            .map_err(|error| ProviderError::Process(error.to_string()))?;
        Ok(())
    }

    /// Consume the structured stdout stream.  The adapter emits normalized
    /// events while preserving unknown provider frames as `Raw` for audit.
    pub async fn events(&self) -> Result<mpsc::Receiver<RuntimeEvent>> {
        let receiver = self.event_broadcast.subscribe();
        if !self.event_started.swap(true, Ordering::AcqRel) {
            let stdout = self.stdout.lock().await.take().ok_or_else(|| {
                self.event_started.store(false, Ordering::Release);
                ProviderError::Closed
            })?;
            let provider = self.provider;
            let provider_session_id = self.provider_session_id.clone();
            let codex_ready = self.codex_ready.clone();
            let child = self.child.clone();
            let broadcast = self.event_broadcast.clone();
            let pending_approvals = self.pending_approvals.clone();
            tokio::spawn(async move {
                read_structured_events(
                    provider,
                    stdout,
                    broadcast,
                    provider_session_id,
                    codex_ready,
                    child,
                    pending_approvals,
                )
                .await;
            });
        }
        // Preserve the existing mpsc receiver API while allowing multiple
        // orchestrator consumers (plan request, broker wake-up, and audit
        // lifecycle) to subscribe to the same structured provider stream.
        let (tx, rx) = mpsc::channel(128);
        tokio::spawn(async move {
            let mut receiver = receiver;
            loop {
                match receiver.recv().await {
                    Ok(event) => {
                        if tx.send(event).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        Ok(rx)
    }

    /// Resolve the oldest provider approval for `operation`. Codex expects a
    /// JSON-RPC response carrying the original request id; updating only the
    /// durable Frank approval row would otherwise leave the provider blocked.
    /// Claude approvals are routed through its MCP permission tool, so this
    /// is intentionally a no-op for that provider.
    pub async fn respond_to_approval(
        &self,
        operation: &str,
        decision: ApprovalDecision,
    ) -> Result<()> {
        if self.provider != Provider::Codex {
            return Ok(());
        }
        let request_id = {
            let mut pending = self.pending_approvals.lock().await;
            let position = pending
                .iter()
                .position(|approval| approval.operation == operation);
            position.and_then(|index| pending.remove(index).map(|approval| approval.request_id))
        };
        let Some(request_id) = request_id else {
            return Err(ProviderError::Process(format!(
                "no pending Codex approval matches operation {operation}"
            )));
        };
        let decision = match decision {
            ApprovalDecision::AllowOnce => "accept",
            ApprovalDecision::DenyOnce => "decline",
        };
        let payload = serde_json::json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "result": {"decision": decision},
        });
        let json = serde_json::to_vec(&payload)
            .map_err(|error| ProviderError::Malformed(error.to_string()))?;
        self.write_json_line(&json).await
    }

    pub async fn stop(&self) -> Result<()> {
        self.graceful_stop().await
    }

    /// Ask a provider to finish its current turn and then reap it.  The
    /// structured protocols do not share one shutdown verb, so the adapter
    /// first closes stdin and gives the child a short grace window before the
    /// bounded force-kill path. This is idempotent across crashes.
    pub async fn graceful_stop(&self) -> Result<()> {
        {
            let mut stdin = self.stdin.lock().await;
            stdin.take();
        }
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let exited = {
                let mut guard = self.child.lock().await;
                let Some(child) = guard.as_mut() else {
                    return Ok(());
                };
                child
                    .try_wait()
                    .map_err(|error| ProviderError::Process(error.to_string()))?
                    .is_some()
            };
            if exited {
                let mut guard = self.child.lock().await;
                if let Some(child) = guard.as_mut() {
                    let _ = child.wait().await;
                }
                *guard = None;
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                return self.force_kill().await;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    /// Force-kill and reap a provider process.  This is the only path used
    /// after a bounded graceful-stop timeout, keeping crash cleanup explicit.
    pub async fn force_kill(&self) -> Result<()> {
        let mut guard = self.child.lock().await;
        let Some(child) = guard.as_mut() else {
            return Ok(());
        };
        let already_exited = child
            .try_wait()
            .map_err(|error| ProviderError::Process(error.to_string()))?
            .is_some();
        if !already_exited {
            // The process may exit between try_wait and start_kill. Treat that
            // race as a normal idempotent stop and let wait reap it below.
            let _ = child.start_kill();
        }
        let _ = child.wait().await;
        *guard = None;
        Ok(())
    }

    pub async fn health(&self) -> Result<bool> {
        let mut guard = self.child.lock().await;
        let Some(child) = guard.as_mut() else {
            return Ok(false);
        };
        Ok(child
            .try_wait()
            .map_err(|error| ProviderError::Process(error.to_string()))?
            .is_none())
    }
}

impl Drop for RuntimeSession {
    fn drop(&mut self) {
        if let Ok(mut child) = self.child.try_lock()
            && let Some(child) = child.as_mut()
        {
            let _ = child.start_kill();
        }
    }
}

#[async_trait]
pub trait RuntimeAdapter: Send + Sync {
    fn provider(&self) -> Provider;
    async fn probe(&self) -> RuntimeProbe;
    async fn start(&self, request: StartRequest) -> Result<RuntimeSession>;
    async fn resume(
        &self,
        mut request: StartRequest,
        provider_session_id: &str,
    ) -> Result<RuntimeSession> {
        request.resume_session_id = Some(provider_session_id.to_string());
        self.start(request).await
    }

    async fn health(&self) -> RuntimeProbe {
        self.probe().await
    }
}

#[derive(Debug, Clone)]
pub struct CodexAdapter {
    pub executable: String,
}

impl Default for CodexAdapter {
    fn default() -> Self {
        Self {
            executable: "codex".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClaudeAdapter {
    pub executable: String,
}

impl Default for ClaudeAdapter {
    fn default() -> Self {
        Self {
            executable: "claude".to_string(),
        }
    }
}

async fn probe_executable(provider: Provider, executable: &str) -> RuntimeProbe {
    let version_output = match run_probe_command(executable, &["--version"]).await {
        Ok(output) => output,
        Err(error) => return unavailable_probe(provider, executable, error),
    };
    if !version_output.success {
        return unavailable_probe(
            provider,
            executable,
            format!("executable exited with {}", version_output.status),
        );
    }

    // Both supported CLIs expose an explicit auth-status command.  A binary
    // that merely exists but is not logged in must not appear selectable in
    // onboarding: provider login remains owned by the user and Frank never
    // stores provider API keys.
    let auth_args: &[&str] = match provider {
        Provider::Codex => &["login", "status"],
        Provider::Claude => &["auth", "status"],
    };
    let auth = run_probe_command(executable, auth_args).await;
    let logged_in = auth.as_ref().is_ok_and(|output| {
        if !output.success {
            return false;
        }
        serde_json::from_str::<Value>(&output.stdout)
            .ok()
            .and_then(|value| {
                value
                    .get("loggedIn")
                    .or_else(|| value.get("logged_in"))
                    .and_then(Value::as_bool)
            })
            .unwrap_or(true)
    });
    let diagnostic = if logged_in {
        None
    } else {
        Some(
            auth.ok()
                .map(|output| {
                    if output.stderr.trim().is_empty() {
                        "provider login is not available; run the provider login command"
                            .to_string()
                    } else {
                        output.stderr.trim().to_string()
                    }
                })
                .unwrap_or_else(|| "provider login status probe timed out".to_string()),
        )
    };
    let version = version_output.stdout.trim().to_string();
    RuntimeProbe {
        capability: ProviderCapability {
            provider,
            executable: Some(executable.to_string()),
            version: (!version.is_empty()).then_some(version),
            logged_in,
            available: logged_in,
            capabilities: if logged_in {
                vec![
                    "structured-stream".to_string(),
                    "resume".to_string(),
                    "approval-events".to_string(),
                    "usage-telemetry".to_string(),
                ]
            } else {
                Vec::new()
            },
            diagnostic,
        },
        executable_path: Some(executable.to_string()),
    }
}

#[derive(Debug)]
struct ProbeOutput {
    success: bool,
    status: String,
    stdout: String,
    stderr: String,
}

async fn run_probe_command(
    executable: &str,
    args: &[&str],
) -> std::result::Result<ProbeOutput, String> {
    let mut command = Command::new(executable);
    command.args(args).kill_on_drop(true);
    let output = tokio::time::timeout(std::time::Duration::from_secs(5), command.output())
        .await
        .map_err(|_| "provider probe timed out".to_string())?
        .map_err(|error| error.to_string())?;
    if output.stdout.len() > frank_protocol::MAX_COMMAND_BODY_BYTES
        || output.stderr.len() > frank_protocol::MAX_COMMAND_BODY_BYTES
    {
        return Err("provider probe output exceeds the configured size cap".into());
    }
    Ok(ProbeOutput {
        success: output.status.success(),
        status: output.status.to_string(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
    })
}

fn unavailable_probe(provider: Provider, executable: &str, diagnostic: String) -> RuntimeProbe {
    RuntimeProbe {
        capability: ProviderCapability {
            provider,
            executable: Some(executable.to_string()),
            version: None,
            logged_in: false,
            available: false,
            capabilities: Vec::new(),
            diagnostic: Some(diagnostic),
        },
        executable_path: None,
    }
}

async fn spawn_provider(
    provider: Provider,
    executable: &str,
    request: &StartRequest,
) -> Result<RuntimeSession> {
    if matches!(
        request.policy.filesystem,
        frank_protocol::FilesystemPolicy::ReadOnly
    ) && request.cwd.is_empty()
    {
        return Err(ProviderError::PolicyDenied(
            "a read-only session still requires a project cwd".to_string(),
        ));
    }
    let mut command = Command::new(executable);
    command.current_dir(&request.cwd);
    if let Some(server_url) = &request.server_url {
        command.env("FRANK_SERVER", server_url);
    }
    if let Some(fingerprint) = &request.server_certificate_fingerprint {
        command.env("FRANK_CERTIFICATE_FINGERPRINT", fingerprint);
    }
    command.env("FRANK_AGENT_ID", &request.agent_id);
    if let Some(capability) = &request.session_capability {
        command.env("FRANK_AGENT_SESSION_TOKEN", capability);
    }
    if let Some(task_id) = &request.task_id {
        command.env("FRANK_TASK_ID", task_id);
    }
    match provider {
        Provider::Codex => {
            // Codex's app-server speaks structured JSON-RPC over stdio.  It
            // is intentionally kept local to this child process.
            command.args(["app-server", "--listen", "stdio://"]);
            if let Some(model) = &request.model {
                command.arg("--model").arg(model);
            }
        }
        Provider::Claude => {
            // Claude Code's documented stream-json interface is the only
            // supported transport; no raw TUI fallback is provided.
            command.args([
                "-p",
                "--output-format",
                "stream-json",
                "--input-format",
                "stream-json",
                "--permission-prompt-tool",
                "mcp_auth_tool",
            ]);
            // The bridge is a local stdio MCP server.  It is never bound to a
            // network socket; Claude only sees it as an in-process child and
            // all mutations still return through frankd's authenticated API.
            let mcp_command = std::env::var("FRANK_AGENT_MCP_BIN")
                .unwrap_or_else(|_| "frank-agent-mcp".to_string());
            let mcp_config = serde_json::json!({
                "mcpServers": {
                    "frank": {
                        "type": "stdio",
                        "command": mcp_command,
                    }
                }
            });
            command.arg("--mcp-config").arg(mcp_config.to_string());
            if let Some(model) = &request.model {
                command.arg("--model").arg(model);
            }
            if let Some(resume) = &request.resume_session_id {
                command.arg("--resume").arg(resume);
            }
        }
    }
    command.stdin(std::process::Stdio::piped());
    command.stdout(std::process::Stdio::piped());
    // A provider's stderr is diagnostic-only. Do not pipe it without a drain
    // task: a noisy CLI could fill the pipe and deadlock stdout processing.
    command.stderr(std::process::Stdio::null());
    command.kill_on_drop(true);
    let child = command
        .spawn()
        .map_err(|error| ProviderError::Process(error.to_string()))?;
    Ok(RuntimeSession::new(
        provider,
        format!("frank-{}", uuid::Uuid::new_v4()),
        child,
        request.clone(),
    ))
}

#[async_trait]
impl RuntimeAdapter for CodexAdapter {
    fn provider(&self) -> Provider {
        Provider::Codex
    }

    async fn probe(&self) -> RuntimeProbe {
        probe_executable(Provider::Codex, &self.executable).await
    }

    async fn start(&self, request: StartRequest) -> Result<RuntimeSession> {
        spawn_provider(Provider::Codex, &self.executable, &request).await
    }
}

#[async_trait]
impl RuntimeAdapter for ClaudeAdapter {
    fn provider(&self) -> Provider {
        Provider::Claude
    }

    async fn probe(&self) -> RuntimeProbe {
        probe_executable(Provider::Claude, &self.executable).await
    }

    async fn start(&self, request: StartRequest) -> Result<RuntimeSession> {
        spawn_provider(Provider::Claude, &self.executable, &request).await
    }
}

#[derive(Clone)]
pub struct RuntimeManager {
    adapters: Arc<HashMap<Provider, Arc<dyn RuntimeAdapter>>>,
}

impl RuntimeManager {
    pub fn new() -> Self {
        let mut adapters: HashMap<Provider, Arc<dyn RuntimeAdapter>> = HashMap::new();
        adapters.insert(Provider::Codex, Arc::new(CodexAdapter::default()));
        adapters.insert(Provider::Claude, Arc::new(ClaudeAdapter::default()));
        Self {
            adapters: Arc::new(adapters),
        }
    }

    pub fn with_adapters(adapters: Vec<Arc<dyn RuntimeAdapter>>) -> Self {
        let mut map = HashMap::new();
        for adapter in adapters {
            map.insert(adapter.provider(), adapter);
        }
        Self {
            adapters: Arc::new(map),
        }
    }

    pub async fn doctor(&self) -> Vec<RuntimeProbe> {
        let mut providers: Vec<_> = self.adapters.values().cloned().collect();
        providers.sort_by_key(|adapter| adapter.provider().to_string());
        let mut result = Vec::with_capacity(providers.len());
        for adapter in providers {
            result.push(adapter.probe().await);
        }
        result
    }

    pub async fn start(&self, provider: Provider, request: StartRequest) -> Result<RuntimeSession> {
        let adapter = self
            .adapters
            .get(&provider)
            .ok_or_else(|| ProviderError::Unavailable(provider.to_string()))?;
        let probe = adapter.probe().await;
        if !probe.capability.available || !probe.capability.logged_in {
            return Err(ProviderError::Unavailable(
                probe
                    .capability
                    .diagnostic
                    .unwrap_or_else(|| provider.to_string()),
            ));
        }
        adapter.start(request).await
    }

    pub async fn resume(
        &self,
        provider: Provider,
        request: StartRequest,
        provider_session_id: &str,
    ) -> Result<RuntimeSession> {
        let adapter = self
            .adapters
            .get(&provider)
            .ok_or_else(|| ProviderError::Unavailable(provider.to_string()))?;
        let probe = adapter.probe().await;
        if !probe.capability.available || !probe.capability.logged_in {
            return Err(ProviderError::Unavailable(
                probe
                    .capability
                    .diagnostic
                    .unwrap_or_else(|| provider.to_string()),
            ));
        }
        adapter.resume(request, provider_session_id).await
    }

    pub async fn health(&self) -> Vec<RuntimeProbe> {
        self.doctor().await
    }
}

impl Default for RuntimeManager {
    fn default() -> Self {
        Self::new()
    }
}

async fn read_structured_events(
    provider: Provider,
    stdout: ChildStdout,
    tx: broadcast::Sender<RuntimeEvent>,
    provider_session_id: Arc<Mutex<Option<String>>>,
    codex_ready: Arc<Notify>,
    child: Arc<Mutex<Option<Child>>>,
    pending_approvals: Arc<Mutex<VecDeque<PendingApproval>>>,
) {
    let mut reader = BufReader::new(stdout);
    loop {
        let Some((line, too_large)) =
            (match read_bounded_line(&mut reader, frank_protocol::MAX_COMMAND_BODY_BYTES).await {
                Ok(line) => line,
                Err(error) => {
                    let _ = tx.send(RuntimeEvent::Error {
                        message: format!("provider stdout could not be read: {error}"),
                    });
                    break;
                }
            })
        else {
            break;
        };
        if too_large {
            let _ = tx.send(RuntimeEvent::Error {
                message: "provider structured frame exceeds the configured size cap".into(),
            });
            break;
        }
        let parsed = serde_json::from_slice::<Value>(&line);
        let events = match parsed {
            Ok(value) => {
                if provider == Provider::Codex {
                    capture_codex_approval(&value, &pending_approvals).await;
                }
                normalize_events(provider, value)
            }
            Err(error) => vec![RuntimeEvent::Error {
                message: format!("malformed structured frame: {error}"),
            }],
        };
        for event in events {
            if let RuntimeEvent::Ready {
                provider_session_id: id,
            } = &event
            {
                *provider_session_id.lock().await = Some(id.clone());
                codex_ready.notify_one();
            }
            let _ = tx.send(event);
        }
    }
    // EOF is a structured lifecycle boundary too. A provider may exit
    // without emitting a final result (crash, user cancellation, or a fake
    // adapter in tests); the orchestrator must still release its scheduler
    // slot and move the task out of Running.
    let code = child
        .lock()
        .await
        .as_mut()
        .and_then(|child| child.try_wait().ok().flatten())
        .and_then(|status| status.code());
    let _ = tx.send(RuntimeEvent::Stopped { code });
}

/// Read one provider frame with a hard byte cap. A provider is an external
/// process, so using `read_line` directly would allow one unterminated frame
/// to grow without bound before the caller could reject it.
async fn read_bounded_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    cap: usize,
) -> std::io::Result<Option<(Vec<u8>, bool)>> {
    let mut line = Vec::with_capacity(cap.min(4096));
    let mut too_large = false;
    loop {
        let buffer = reader.fill_buf().await?;
        if buffer.is_empty() {
            if line.is_empty() && !too_large {
                return Ok(None);
            }
            return Ok(Some((line, too_large)));
        }
        let end = buffer
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(buffer.len(), |index| index + 1);
        let terminated = buffer[end.saturating_sub(1)] == b'\n';
        if !too_large {
            if line.len().saturating_add(end) > cap {
                too_large = true;
                line.clear();
            } else {
                line.extend_from_slice(&buffer[..end]);
            }
        }
        reader.consume(end);
        if terminated {
            return Ok(Some((line, too_large)));
        }
    }
}

fn normalize_events(provider: Provider, value: Value) -> Vec<RuntimeEvent> {
    let kind = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut events = Vec::new();
    match provider {
        Provider::Claude => normalize_claude_event(kind, &value, &mut events),
        Provider::Codex => normalize_codex_event(kind, &value, &mut events),
    }
    if events.is_empty() {
        events.push(RuntimeEvent::Raw(value));
    }
    events
}

/// Normalize one checked provider frame without starting a process. This is
/// used by fixture/golden tests and by doctor tooling when validating a newly
/// installed adapter schema.
pub fn normalize_provider_frame(provider: Provider, value: Value) -> Vec<RuntimeEvent> {
    normalize_events(provider, value)
}

fn normalize_claude_event(kind: &str, value: &Value, events: &mut Vec<RuntimeEvent>) {
    match kind {
        "system" => {
            if value.get("subtype").and_then(Value::as_str) == Some("init")
                && let Some(id) = value.get("session_id").and_then(Value::as_str)
            {
                events.push(RuntimeEvent::Ready {
                    provider_session_id: id.to_string(),
                });
            }
        }
        "session" | "ready" => {
            if let Some(id) = value
                .get("session_id")
                .or_else(|| value.get("sessionId"))
                .and_then(Value::as_str)
            {
                events.push(RuntimeEvent::Ready {
                    provider_session_id: id.to_string(),
                });
            }
        }
        "assistant" => {
            if let Some(message) = value.get("message") {
                normalize_content_blocks(message, events);
                if let Some(usage) = message.get("usage") {
                    events.push(usage_telemetry(usage));
                }
            } else if let Some(text) = value.get("text").and_then(Value::as_str) {
                events.push(RuntimeEvent::Text {
                    text: text.to_string(),
                });
            }
        }
        "stream_event" => {
            if let Some(delta) = value.get("event").and_then(|event| event.get("delta"))
                && delta.get("type").and_then(Value::as_str) == Some("text_delta")
                && let Some(text) = delta.get("text").and_then(Value::as_str)
            {
                events.push(RuntimeEvent::Text {
                    text: text.to_string(),
                });
            }
        }
        "result" => {
            if let Some(id) = value.get("session_id").and_then(Value::as_str) {
                events.push(RuntimeEvent::Ready {
                    provider_session_id: id.to_string(),
                });
            }
            if let Some(usage) = value.get("usage") {
                events.push(usage_telemetry_with_fallback(usage, value));
            } else {
                events.push(usage_telemetry(value));
            }
            if let Some(text) = value.get("result").and_then(Value::as_str) {
                events.push(RuntimeEvent::Text {
                    text: text.to_string(),
                });
            }
        }
        "usage" => events.push(usage_telemetry(value)),
        "permission" | "approval" => events.push(approval_event(value)),
        "error" => events.push(error_event(value)),
        _ => {}
    }
}

fn normalize_codex_event(kind: &str, value: &Value, events: &mut Vec<RuntimeEvent>) {
    // Codex app-server notifications are JSON-RPC objects. Keep matching
    // method names explicit; unknown protocol versions remain Raw instead of
    // being interpreted as terminal text.
    let method = value.get("method").and_then(Value::as_str).unwrap_or(kind);
    let params = value.get("params").unwrap_or(value);
    // Requests such as `thread/start` and `thread/resume` return a JSON-RPC
    // result rather than a notification. Normalize both response shapes so
    // the session can use the provider-assigned id before sending its first
    // turn.
    if value.get("error").is_some() {
        events.push(error_event(value));
        return;
    }
    if let Some(result) = value.get("result")
        && let Some(id) = result
            .get("thread")
            .and_then(|thread| thread.get("id"))
            .and_then(Value::as_str)
            .or_else(|| result.get("threadId").and_then(Value::as_str))
            .or_else(|| result.get("thread_id").and_then(Value::as_str))
    {
        events.push(RuntimeEvent::Ready {
            provider_session_id: id.to_string(),
        });
        return;
    }
    match method {
        "thread/started" | "thread/created" | "session" | "ready" => {
            if let Some(id) = params
                .get("thread")
                .and_then(|thread| thread.get("id"))
                .and_then(Value::as_str)
                .or_else(|| params.get("thread_id").and_then(Value::as_str))
                .or_else(|| params.get("session_id").and_then(Value::as_str))
            {
                events.push(RuntimeEvent::Ready {
                    provider_session_id: id.to_string(),
                });
            }
        }
        "item/agentMessage/delta" | "turn/assistant_delta" | "text" => {
            if let Some(text) = params
                .get("delta")
                .or_else(|| params.get("text"))
                .and_then(Value::as_str)
            {
                events.push(RuntimeEvent::Text {
                    text: text.to_string(),
                });
            }
        }
        "item/commandExecution/requestApproval"
        | "item/fileChange/requestApproval"
        | "item/permissions/requestApproval"
        | "approval"
        | "permission" => {
            events.push(approval_event(params));
        }
        "thread/tokenUsage/updated" | "turn/tokenUsage/updated" => {
            let usage = params
                .get("tokenUsage")
                .or_else(|| params.get("token_usage"))
                .unwrap_or(params);
            let last = usage.get("last").unwrap_or(usage);
            events.push(usage_telemetry(last));
        }
        "turn/completed" | "usage" => {
            // Older app-server fixtures exposed usage directly on params;
            // newer versions may nest it under `turn` or `usage`. Check all
            // supported shapes without inventing estimated values.
            let usage = params
                .get("usage")
                .or_else(|| params.get("turn").and_then(|turn| turn.get("usage")))
                .unwrap_or(params);
            events.push(usage_telemetry(usage));
        }
        "error" => events.push(error_event(params)),
        _ => {}
    }
}

fn normalize_content_blocks(message: &Value, events: &mut Vec<RuntimeEvent>) {
    let Some(content) = message.get("content").and_then(Value::as_array) else {
        return;
    };
    for block in content {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    events.push(RuntimeEvent::Text {
                        text: text.to_string(),
                    });
                }
            }
            Some("tool_use") => {
                if let Some(name) = block.get("name").and_then(Value::as_str) {
                    events.push(RuntimeEvent::ToolCall {
                        name: name.to_string(),
                        input: block.get("input").cloned().unwrap_or(Value::Null),
                    });
                }
            }
            _ => {}
        }
    }
}

fn usage_telemetry(value: &Value) -> RuntimeEvent {
    let input = value
        .get("input_tokens")
        .or_else(|| value.get("inputTokens"))
        .and_then(Value::as_u64);
    let output = value
        .get("output_tokens")
        .or_else(|| value.get("outputTokens"))
        .and_then(Value::as_u64);
    let cost_micros = value
        .get("cost_micros")
        .or_else(|| value.get("costMicros"))
        .and_then(Value::as_u64)
        .or_else(|| {
            value
                .get("total_cost_usd")
                .or_else(|| value.get("totalCostUsd"))
                .and_then(Value::as_f64)
                .filter(|cost| cost.is_finite() && *cost >= 0.0)
                .map(|cost| (cost * 1_000_000.0).round() as u64)
        });
    RuntimeEvent::Usage(UsageTelemetry {
        measured_input_tokens: input,
        measured_output_tokens: output,
        estimated_input_tokens: None,
        estimated_output_tokens: None,
        cost_micros,
    })
}

fn usage_telemetry_with_fallback(primary: &Value, fallback: &Value) -> RuntimeEvent {
    let primary_event = usage_telemetry(primary);
    let RuntimeEvent::Usage(mut telemetry) = primary_event else {
        unreachable!("usage_telemetry always returns Usage")
    };
    let RuntimeEvent::Usage(fallback_telemetry) = usage_telemetry(fallback) else {
        unreachable!("usage_telemetry always returns Usage")
    };
    telemetry.measured_input_tokens = telemetry
        .measured_input_tokens
        .or(fallback_telemetry.measured_input_tokens);
    telemetry.measured_output_tokens = telemetry
        .measured_output_tokens
        .or(fallback_telemetry.measured_output_tokens);
    telemetry.cost_micros = telemetry.cost_micros.or(fallback_telemetry.cost_micros);
    RuntimeEvent::Usage(telemetry)
}

fn approval_event(value: &Value) -> RuntimeEvent {
    RuntimeEvent::ApprovalRequest {
        operation: provider_approval_operation(value),
        reason: value
            .get("reason")
            .or_else(|| value.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("provider requested permission")
            .chars()
            .filter(|character| !character.is_control())
            .take(frank_protocol::MAX_MESSAGE_BODY_BYTES)
            .collect(),
        cwd: value.get("cwd").and_then(Value::as_str).map(|cwd| {
            cwd.chars()
                .filter(|character| !character.is_control())
                .collect()
        }),
    }
}

fn provider_approval_operation(value: &Value) -> String {
    value
        .get("operation")
        .or_else(|| value.get("tool_name"))
        .or_else(|| value.get("command"))
        .or_else(|| value.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or("provider operation")
        .chars()
        .filter(|character| !character.is_control())
        .take(4_096)
        .collect()
}

async fn capture_codex_approval(
    value: &Value,
    pending_approvals: &Arc<Mutex<VecDeque<PendingApproval>>>,
) {
    let method = value
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if method != "item/commandExecution/requestApproval"
        && method != "item/fileChange/requestApproval"
        && method != "item/permissions/requestApproval"
    {
        return;
    }
    let Some(request_id) = value.get("id").cloned() else {
        return;
    };
    let params = value.get("params").unwrap_or(value);
    let operation = provider_approval_operation(params);
    let mut pending = pending_approvals.lock().await;
    // A provider retry can repeat the same JSON-RPC request id. Keeping one
    // entry per id makes duplicate frames harmless and avoids responding
    // twice to one approval callback.
    if pending
        .iter()
        .any(|approval| approval.request_id == request_id)
    {
        return;
    }
    pending.push_back(PendingApproval {
        operation,
        request_id,
    });
}

fn error_event(value: &Value) -> RuntimeEvent {
    let message = value
        .get("message")
        .or_else(|| value.get("error").and_then(|error| error.get("message")))
        .and_then(Value::as_str)
        .or_else(|| value.get("error").and_then(Value::as_str))
        .map(str::to_string)
        .or_else(|| {
            value
                .get("error")
                .filter(|error| !error.is_null())
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| "provider error".to_string());
    RuntimeEvent::Error { message }
}

/// Cross-platform shell specification used by the server terminal service.
/// The actual PTY implementation lives behind this boundary so the native
/// GUI can render ANSI state without ever owning a provider process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellSpec {
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
    pub shell: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellOutput {
    pub frame: TerminalFrame,
}

#[cfg(test)]
mod tests {
    use super::*;
    use frank_protocol::AgentPolicy;

    #[test]
    fn measured_and_estimated_usage_never_mix() {
        let measured = UsageTelemetry::measured(Some(4), Some(6), None);
        assert_eq!(measured.measured_tokens(), Some(10));
        let estimated = UsageTelemetry::estimated(Some(100), Some(100));
        assert_eq!(estimated.measured_tokens(), None);
    }

    #[tokio::test]
    async fn missing_provider_is_reported_by_doctor_not_panicked() {
        let manager = RuntimeManager::new();
        let probes = manager.doctor().await;
        assert_eq!(probes.len(), 2);
    }

    #[test]
    fn start_request_carries_intersection_policy() {
        let request = StartRequest {
            agent_id: "a".into(),
            task_id: None,
            cwd: "/tmp/project".into(),
            instructions: "work".into(),
            policy: AgentPolicy::default(),
            model: None,
            resume_session_id: None,
            server_url: None,
            server_certificate_fingerprint: None,
            session_capability: None,
        };
        assert_eq!(
            request.policy.filesystem,
            frank_protocol::FilesystemPolicy::WorkspaceWrite
        );
    }

    #[test]
    fn claude_stream_frames_normalize_text_tools_usage_and_session() {
        let value = serde_json::json!({
            "type": "assistant",
            "session_id": "claude-session",
            "message": {
                "content": [
                    {"type": "text", "text": "hello"},
                    {"type": "tool_use", "name": "Read", "input": {"file": "a"}}
                ],
                "usage": {"input_tokens": 3, "output_tokens": 2}
            }
        });
        let events = normalize_events(Provider::Claude, value);
        assert!(
            events
                .iter()
                .any(|event| matches!(event, RuntimeEvent::Text { text } if text == "hello"))
        );
        assert!(
            events.iter().any(
                |event| matches!(event, RuntimeEvent::ToolCall { name, .. } if name == "Read")
            )
        );
        assert!(events.iter().any(|event| matches!(event, RuntimeEvent::Usage(usage) if usage.measured_input_tokens == Some(3))));
    }

    #[test]
    fn codex_jsonrpc_frames_do_not_fall_back_to_tui_scraping() {
        let value = serde_json::json!({
            "method": "item/agentMessage/delta",
            "params": {"delta": "structured"}
        });
        let events = normalize_events(Provider::Codex, value);
        assert!(matches!(events.as_slice(), [RuntimeEvent::Text { text }] if text == "structured"));
    }

    #[test]
    fn codex_unknown_jsonrpc_result_does_not_become_provider_session() {
        // JSON-RPC responses for turns and initialization can contain an
        // unrelated `id`.  Only an explicit thread-shaped result may advance
        // the session handshake; unknown responses stay auditable Raw frames.
        let value = serde_json::json!({
            "id": "turn-42",
            "result": {"id": "turn-42"}
        });
        let events = normalize_events(Provider::Codex, value.clone());
        assert!(matches!(events.as_slice(), [RuntimeEvent::Raw(raw)] if raw == &value));
    }

    #[tokio::test]
    async fn codex_approval_decision_answers_original_jsonrpc_request() {
        let mut command = Command::new("cat");
        command.stdin(std::process::Stdio::piped());
        command.stdout(std::process::Stdio::piped());
        let child = command.spawn().expect("cat is available on test hosts");
        let request = StartRequest {
            agent_id: "agent".into(),
            task_id: Some("task".into()),
            cwd: "/tmp".into(),
            instructions: String::new(),
            policy: AgentPolicy::default(),
            model: None,
            resume_session_id: None,
            server_url: None,
            server_certificate_fingerprint: None,
            session_capability: None,
        };
        let session = RuntimeSession::new(Provider::Codex, "stable".into(), child, request);
        session
            .pending_approvals
            .lock()
            .await
            .push_back(PendingApproval {
                operation: "shell".into(),
                request_id: serde_json::json!(42),
            });
        session
            .respond_to_approval("shell", ApprovalDecision::AllowOnce)
            .await
            .expect("provider response is written");
        let stdout = session.stdout.lock().await.take().expect("stdout");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            reader.read_line(&mut line),
        )
        .await
        .expect("cat echoed the response")
        .expect("read response");
        let response: Value = serde_json::from_str(line.trim()).expect("valid JSON-RPC");
        assert_eq!(response.get("id"), Some(&serde_json::json!(42)));
        assert_eq!(response["result"]["decision"], "accept");
        session.force_kill().await.expect("reap cat");
    }
}
