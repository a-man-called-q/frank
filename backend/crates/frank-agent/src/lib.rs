//! Provider and terminal runtime adapters.
//!
//! Frank controls providers through structured streams.  The adapters never
//! scrape a provider TUI and never expose the provider's app-server to the
//! network.  A provider process is owned by `frankd`, scoped to one agent/task,
//! and cleaned up when its session is stopped or dropped.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use frank_protocol::{
    AgentPolicy, ApprovalDecision, ModelDescriptor, OpenRouterCapability, TerminalFrame,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::sync::{Mutex, broadcast, mpsc};

pub mod openrouter;
pub mod terminal;

pub use openrouter::{CredentialResolver, EnvironmentCredentialResolver, OpenRouterAdapter};

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
}

pub type Result<T> = std::result::Result<T, ProviderError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeProbe {
    pub capability: OpenRouterCapability,
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
    /// Durable local Chat Completions transcript used to resume a stateless
    /// OpenRouter session after frankd restarts.
    #[serde(default)]
    pub initial_transcript: Vec<Value>,
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
        #[serde(default)]
        call_id: String,
        name: String,
        input: Value,
    },
    /// One complete assistant turn, emitted after all streamed deltas have
    /// been assembled. The orchestrator persists this before executing any
    /// accompanying tool calls.
    AssistantMessage {
        turn_id: String,
        content: String,
        #[serde(default)]
        tool_calls: Vec<RuntimeToolCall>,
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
pub struct RuntimeToolCall {
    pub call_id: String,
    pub name: String,
    pub input: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageTelemetry {
    pub measured_input_tokens: Option<u64>,
    pub measured_output_tokens: Option<u64>,
    pub estimated_input_tokens: Option<u64>,
    pub estimated_output_tokens: Option<u64>,
    pub cost_micros: Option<u64>,
    #[serde(default)]
    pub cached_input_tokens: Option<u64>,
    #[serde(default)]
    pub reasoning_tokens: Option<u64>,
}

#[derive(Debug)]
pub(crate) enum OpenRouterCommand {
    Message(ProviderMessage),
    ToolResult { call_id: String, output: String },
    Stop,
}

#[derive(Debug, Clone)]
pub(crate) struct OpenRouterRuntime {
    pub command_tx: mpsc::Sender<OpenRouterCommand>,
    pub stopped: Arc<AtomicBool>,
}

impl UsageTelemetry {
    pub fn measured(input: Option<u64>, output: Option<u64>, cost_micros: Option<u64>) -> Self {
        Self {
            measured_input_tokens: input,
            measured_output_tokens: output,
            estimated_input_tokens: None,
            estimated_output_tokens: None,
            cost_micros,
            cached_input_tokens: None,
            reasoning_tokens: None,
        }
    }

    pub fn estimated(input: Option<u64>, output: Option<u64>) -> Self {
        Self {
            measured_input_tokens: None,
            measured_output_tokens: None,
            estimated_input_tokens: input,
            estimated_output_tokens: output,
            cost_micros: None,
            cached_input_tokens: None,
            reasoning_tokens: None,
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
    pub stable_session_id: String,
    pub provider_session_id: Arc<Mutex<Option<String>>>,
    request: StartRequest,
    event_broadcast: broadcast::Sender<RuntimeEvent>,
    openrouter: OpenRouterRuntime,
}

impl RuntimeSession {
    pub(crate) fn new_openrouter(
        stable_session_id: String,
        provider_session_id: Arc<Mutex<Option<String>>>,
        openrouter: OpenRouterRuntime,
        event_broadcast: broadcast::Sender<RuntimeEvent>,
        request: StartRequest,
    ) -> Self {
        Self {
            stable_session_id,
            provider_session_id,
            request,
            event_broadcast,
            openrouter,
        }
    }

    pub fn request(&self) -> &StartRequest {
        &self.request
    }

    pub async fn send(&self, message: &ProviderMessage) -> Result<()> {
        if message.content.len() > frank_protocol::MAX_MESSAGE_BODY_BYTES {
            return Err(ProviderError::Malformed(
                "provider message exceeds the configured size cap".into(),
            ));
        }
        self.openrouter
            .command_tx
            .send(OpenRouterCommand::Message(message.clone()))
            .await
            .map_err(|_| ProviderError::Closed)
    }

    pub async fn events(&self) -> Result<mpsc::Receiver<RuntimeEvent>> {
        let mut receiver = self.event_broadcast.subscribe();
        let (tx, rx) = mpsc::channel(128);
        let provider_session_id = self
            .provider_session_id
            .lock()
            .await
            .clone()
            .unwrap_or_else(|| self.stable_session_id.clone());
        let _ = tx
            .send(RuntimeEvent::Ready {
                provider_session_id,
            })
            .await;
        tokio::spawn(async move {
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

    pub async fn respond_to_approval(
        &self,
        _operation: &str,
        _decision: ApprovalDecision,
    ) -> Result<()> {
        // OpenRouter tool approval is resolved before the tool result is
        // submitted. No provider-side callback exists to answer here.
        Ok(())
    }

    pub async fn submit_tool_result(&self, call_id: &str, output: &str) -> Result<()> {
        if output.len() > frank_protocol::MAX_COMMAND_BODY_BYTES {
            return Err(ProviderError::Malformed(
                "tool result exceeds the configured size cap".into(),
            ));
        }
        self.openrouter
            .command_tx
            .send(OpenRouterCommand::ToolResult {
                call_id: call_id.to_string(),
                output: output.to_string(),
            })
            .await
            .map_err(|_| ProviderError::Closed)
    }

    pub async fn stop(&self) -> Result<()> {
        self.graceful_stop().await
    }

    pub async fn graceful_stop(&self) -> Result<()> {
        if !self.openrouter.stopped.swap(true, Ordering::AcqRel) {
            let _ = self
                .openrouter
                .command_tx
                .send(OpenRouterCommand::Stop)
                .await;
        }
        Ok(())
    }

    pub async fn force_kill(&self) -> Result<()> {
        self.graceful_stop().await
    }

    pub async fn health(&self) -> Result<bool> {
        Ok(!self.openrouter.stopped.load(Ordering::Acquire))
    }
}

#[async_trait]
pub trait RuntimeAdapter: Send + Sync {
    async fn probe(&self) -> RuntimeProbe;
    async fn start(&self, request: StartRequest) -> Result<RuntimeSession>;
    async fn model_catalog(&self, _refresh: bool) -> Result<Vec<ModelDescriptor>> {
        Err(ProviderError::Unsupported(
            "this runtime does not expose a model catalog".into(),
        ))
    }
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

#[derive(Clone)]
pub struct RuntimeManager {
    /// The runtime has one provider boundary: OpenRouter. Keeping the
    /// adapter singular makes accidental provider branching impossible in
    /// production paths.
    adapter: Arc<dyn RuntimeAdapter>,
}

impl RuntimeManager {
    pub fn new() -> Self {
        // The environment resolver keeps the CLI/server bootstrap usable for
        // headless deployments.  The daemon injects its keychain-aware
        // resolver through `with_openrouter_credentials`.
        if let Ok(adapter) =
            OpenRouterAdapter::new(Arc::new(openrouter::EnvironmentCredentialResolver))
        {
            return Self {
                adapter: Arc::new(adapter),
            };
        }
        // OpenRouterAdapter construction only fails when the reqwest client
        // cannot be built. Keep bootstrap deterministic and let health report
        // the unconfigured credential state rather than probing executables.
        let adapter = OpenRouterAdapter::new(Arc::new(openrouter::EnvironmentCredentialResolver))
            .expect("OpenRouter HTTP client must be constructible");
        Self {
            adapter: Arc::new(adapter),
        }
    }

    pub fn with_openrouter_credentials(credentials: Arc<dyn CredentialResolver>) -> Result<Self> {
        let adapter = Arc::new(OpenRouterAdapter::new(credentials)?);
        Ok(Self::with_openrouter_adapter(adapter))
    }

    pub fn with_openrouter_adapter(adapter: Arc<OpenRouterAdapter>) -> Self {
        Self {
            adapter: adapter as Arc<dyn RuntimeAdapter>,
        }
    }

    pub async fn doctor(&self) -> Vec<RuntimeProbe> {
        vec![self.adapter.probe().await]
    }

    pub async fn start(&self, request: StartRequest) -> Result<RuntimeSession> {
        let adapter = &self.adapter;
        let probe = adapter.probe().await;
        if !probe.capability.available || !probe.capability.configured {
            return Err(ProviderError::Unavailable(
                probe
                    .capability
                    .diagnostic
                    .unwrap_or_else(|| "OpenRouter is unavailable".into()),
            ));
        }
        adapter.start(request).await
    }

    pub async fn resume(
        &self,
        request: StartRequest,
        provider_session_id: &str,
    ) -> Result<RuntimeSession> {
        let adapter = &self.adapter;
        let probe = adapter.probe().await;
        if !probe.capability.available || !probe.capability.configured {
            return Err(ProviderError::Unavailable(
                probe
                    .capability
                    .diagnostic
                    .unwrap_or_else(|| "OpenRouter is unavailable".into()),
            ));
        }
        adapter.resume(request, provider_session_id).await
    }

    pub async fn health(&self) -> Vec<RuntimeProbe> {
        self.doctor().await
    }

    pub async fn model_catalog(&self, refresh: bool) -> Result<Vec<ModelDescriptor>> {
        self.adapter.model_catalog(refresh).await
    }
}

impl Default for RuntimeManager {
    fn default() -> Self {
        Self::new()
    }
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
        assert_eq!(probes.len(), 1);
        assert!(probes[0].capability.available || !probes[0].capability.configured);
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
            initial_transcript: Vec::new(),
        };
        assert_eq!(
            request.policy.filesystem,
            frank_protocol::FilesystemPolicy::WorkspaceWrite
        );
    }
}
