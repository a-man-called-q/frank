//! OpenRouter's OpenAI-compatible streaming runtime.
//!
//! The adapter deliberately owns no UI or persistence concerns.  It turns
//! OpenRouter SSE frames into Frank runtime events; the orchestrator remains
//! responsible for durable state, approvals, budgets, and tool execution.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use frank_protocol::{ModelDescriptor, OpenRouterCapability, OpenRouterConnectionView, Timestamp};
use futures_util::StreamExt;
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use tokio::sync::{RwLock, mpsc};

use crate::{
    OpenRouterCommand, OpenRouterRuntime, ProviderError, Result, RuntimeAdapter, RuntimeEvent,
    RuntimeProbe, RuntimeSession, RuntimeToolCall, StartRequest, UsageTelemetry,
};

pub const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api/v1";
const MODEL_CACHE_TTL: Duration = Duration::from_secs(15 * 60);
const MAX_SSE_FRAME_BYTES: usize = frank_protocol::MAX_COMMAND_BODY_BYTES;
const MAX_TURN_ATTEMPTS: usize = 3;
const CONTROL_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

#[async_trait]
pub trait CredentialResolver: Send + Sync {
    async fn api_key(&self) -> Result<Option<String>>;

    fn credential_source(&self) -> Option<String>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct EnvironmentCredentialResolver;

#[async_trait]
impl CredentialResolver for EnvironmentCredentialResolver {
    async fn api_key(&self) -> Result<Option<String>> {
        Ok(std::env::var("OPENROUTER_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty()))
    }

    fn credential_source(&self) -> Option<String> {
        std::env::var("OPENROUTER_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|_| "environment".to_string())
    }
}

/// Environment-backed resolver for the native OpenAI adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct EnvironmentOpenAiCredentialResolver;

#[async_trait]
impl CredentialResolver for EnvironmentOpenAiCredentialResolver {
    async fn api_key(&self) -> Result<Option<String>> {
        Ok(std::env::var("OPENAI_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty()))
    }

    fn credential_source(&self) -> Option<String> {
        std::env::var("OPENAI_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|_| "environment".to_string())
    }
}

#[derive(Clone)]
pub struct OpenRouterAdapter {
    pub base_url: String,
    pub client: Client,
    pub credentials: Arc<dyn CredentialResolver>,
    pub provider_name: String,
    catalog_requires_tools: bool,
    model_namespace: Option<String>,
    catalog: Arc<RwLock<Option<CatalogCache>>>,
}

#[derive(Debug, Clone)]
struct CatalogCache {
    fetched_at: Instant,
    refreshed_at: Timestamp,
    models: Vec<ModelDescriptor>,
}

impl std::fmt::Debug for OpenRouterAdapter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenRouterAdapter")
            .field("base_url", &self.base_url)
            .finish_non_exhaustive()
    }
}

impl OpenRouterAdapter {
    pub fn new(credentials: Arc<dyn CredentialResolver>) -> Result<Self> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(10 * 60))
            .build()
            .map_err(|error| ProviderError::Process(error.to_string()))?;
        Ok(Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            client,
            credentials,
            provider_name: "OpenRouter".to_string(),
            catalog_requires_tools: true,
            model_namespace: None,
            catalog: Arc::new(RwLock::new(None)),
        })
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into().trim_end_matches('/').to_string();
        self
    }

    pub fn with_provider_name(mut self, provider_name: impl Into<String>) -> Self {
        self.provider_name = provider_name.into();
        self
    }

    pub fn with_catalog_requires_tools(mut self, required: bool) -> Self {
        self.catalog_requires_tools = required;
        self
    }

    pub fn with_model_namespace(mut self, namespace: impl Into<String>) -> Self {
        self.model_namespace = Some(namespace.into());
        self
    }

    async fn key(&self) -> Result<String> {
        self.credentials
            .api_key()
            .await?
            .filter(|key| !key.trim().is_empty())
            .ok_or_else(|| {
                ProviderError::Unavailable(format!(
                    "{} API key is not configured",
                    self.provider_name
                ))
            })
    }

    pub async fn connection(&self) -> OpenRouterConnectionView {
        let key = self.credentials.api_key().await.ok().flatten();
        let Some(key) = key.filter(|key| !key.trim().is_empty()) else {
            return OpenRouterConnectionView {
                configured: false,
                credential_source: self.credentials.credential_source(),
                checked_at: Some(frank_protocol::timestamp_now()),
                catalog_refreshed_at: self.catalog_refreshed_at().await,
                diagnostic: Some(format!("{} API key is not configured", self.provider_name)),
            };
        };
        let models_url = if self.catalog_requires_tools {
            format!("{}/models?limit=1", self.base_url)
        } else {
            format!("{}/models", self.base_url)
        };
        let response = self
            .client
            .get(models_url)
            .timeout(CONTROL_REQUEST_TIMEOUT)
            .bearer_auth(&key)
            .send()
            .await;
        let diagnostic = match response {
            Ok(response) if response.status().is_success() => None,
            Ok(response) => Some(
                http_error_for(
                    &self.provider_name,
                    response.status(),
                    response.text().await.unwrap_or_default(),
                    Some(&key),
                )
                .to_string(),
            ),
            Err(error) => Some(sanitize_error(&error.to_string(), Some(&key))),
        };
        OpenRouterConnectionView {
            configured: true,
            credential_source: self.credentials.credential_source(),
            checked_at: Some(frank_protocol::timestamp_now()),
            catalog_refreshed_at: self.catalog_refreshed_at().await,
            diagnostic,
        }
    }

    pub async fn models(&self, refresh: bool) -> Result<Vec<ModelDescriptor>> {
        Ok(self.models_with_status(refresh).await?.0)
    }

    pub async fn models_with_status(
        &self,
        refresh: bool,
    ) -> Result<(Vec<ModelDescriptor>, Option<Timestamp>, bool)> {
        if !refresh
            && let Some(cache) = self.catalog.read().await.as_ref()
            && cache.fetched_at.elapsed() < MODEL_CACHE_TTL
        {
            return Ok((
                cache.models.clone(),
                Some(cache.refreshed_at.clone()),
                false,
            ));
        }
        let key = self.key().await?;
        let models_url = if self.catalog_requires_tools {
            format!(
                "{}/models?output_modalities=text&supported_parameters=tools",
                self.base_url
            )
        } else {
            format!("{}/models", self.base_url)
        };
        let response = self
            .client
            .get(models_url)
            .timeout(CONTROL_REQUEST_TIMEOUT)
            .bearer_auth(&key)
            .send()
            .await
            .map_err(|error| {
                ProviderError::Process(sanitize_error(&error.to_string(), Some(&key)))
            });
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                if let Some(cache) = self.catalog.read().await.as_ref() {
                    return Ok((cache.models.clone(), Some(cache.refreshed_at.clone()), true));
                }
                return Err(error);
            }
        };
        let status = response.status();
        if !status.is_success() {
            let error = http_error_for(
                &self.provider_name,
                status,
                response.text().await.unwrap_or_default(),
                Some(&key),
            );
            // An authenticated failure is actionable and must not be hidden
            // behind stale metadata. Last-known-good is only a resilience
            // path for rate limiting and provider/server availability errors.
            if (status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                && let Some(cache) = self.catalog.read().await.as_ref()
            {
                return Ok((cache.models.clone(), Some(cache.refreshed_at.clone()), true));
            }
            return Err(error);
        }
        let payload: Value = match response.json().await {
            Ok(payload) => payload,
            Err(error) => {
                if let Some(cache) = self.catalog.read().await.as_ref() {
                    return Ok((cache.models.clone(), Some(cache.refreshed_at.clone()), true));
                }
                return Err(ProviderError::Malformed(sanitize_error(
                    &error.to_string(),
                    Some(&key),
                )));
            }
        };
        let models = match payload.get("data").and_then(Value::as_array) {
            Some(data) => data
                .iter()
                .filter_map(|value| model_descriptor(value, self.model_namespace.as_deref()))
                .filter(|model| {
                    model_is_eligible(
                        model,
                        self.catalog_requires_tools,
                        self.model_namespace.as_deref(),
                    )
                })
                .collect::<Vec<_>>(),
            None => {
                if let Some(cache) = self.catalog.read().await.as_ref() {
                    return Ok((cache.models.clone(), Some(cache.refreshed_at.clone()), true));
                }
                return Err(ProviderError::Malformed(format!(
                    "{} model response has no data",
                    self.provider_name
                )));
            }
        };
        let refreshed_at = frank_protocol::timestamp_now();
        *self.catalog.write().await = Some(CatalogCache {
            fetched_at: Instant::now(),
            refreshed_at: refreshed_at.clone(),
            models: models.clone(),
        });
        Ok((models, Some(refreshed_at), false))
    }

    pub async fn catalog_refreshed_at(&self) -> Option<Timestamp> {
        self.catalog
            .read()
            .await
            .as_ref()
            .map(|cache| cache.refreshed_at.clone())
    }
}

#[async_trait]
impl RuntimeAdapter for OpenRouterAdapter {
    async fn probe(&self) -> RuntimeProbe {
        let configured = self.credentials.api_key().await.ok().flatten();
        let Some(key) = configured.filter(|key| !key.trim().is_empty()) else {
            return RuntimeProbe {
                provider: self.provider_name.clone(),
                capability: OpenRouterCapability {
                    configured: false,
                    version: None,
                    logged_in: false,
                    available: false,
                    capabilities: Vec::new(),
                    diagnostic: Some(format!("{} API key is not configured", self.provider_name)),
                    credential_source: self.credentials.credential_source(),
                    catalog_refreshed_at: self.catalog_refreshed_at().await,
                },
            };
        };
        let models_url = if self.catalog_requires_tools {
            format!("{}/models?limit=1", self.base_url)
        } else {
            format!("{}/models", self.base_url)
        };
        let response = self
            .client
            .get(models_url)
            .timeout(CONTROL_REQUEST_TIMEOUT)
            .bearer_auth(&key)
            .send()
            .await;
        match response {
            Ok(response) if response.status().is_success() => RuntimeProbe {
                provider: self.provider_name.clone(),
                capability: OpenRouterCapability {
                    configured: true,
                    version: None,
                    logged_in: true,
                    available: true,
                    capabilities: vec![
                        "streaming".into(),
                        "tool-calling".into(),
                        "model-catalog".into(),
                        "usage-telemetry".into(),
                        "resume-from-transcript".into(),
                    ],
                    diagnostic: None,
                    credential_source: self.credentials.credential_source(),
                    catalog_refreshed_at: self.catalog_refreshed_at().await,
                },
            },
            Ok(response) => RuntimeProbe {
                provider: self.provider_name.clone(),
                capability: OpenRouterCapability {
                    configured: true,
                    version: None,
                    logged_in: false,
                    available: false,
                    capabilities: Vec::new(),
                    diagnostic: Some(
                        http_error_for(
                            &self.provider_name,
                            response.status(),
                            response.text().await.unwrap_or_default(),
                            Some(&key),
                        )
                        .to_string(),
                    ),
                    credential_source: self.credentials.credential_source(),
                    catalog_refreshed_at: self.catalog_refreshed_at().await,
                },
            },
            Err(error) => RuntimeProbe {
                provider: self.provider_name.clone(),
                capability: OpenRouterCapability {
                    configured: true,
                    version: None,
                    logged_in: false,
                    available: false,
                    capabilities: Vec::new(),
                    diagnostic: Some(sanitize_error(&error.to_string(), Some(&key))),
                    credential_source: self.credentials.credential_source(),
                    catalog_refreshed_at: self.catalog_refreshed_at().await,
                },
            },
        }
    }

    async fn start(&self, request: StartRequest) -> Result<RuntimeSession> {
        let key = self.key().await?;
        let requested_model = request.model.as_deref().filter(|model| !model.is_empty());
        if requested_model.is_none() {
            return Err(ProviderError::Unavailable(format!(
                "a {} model must be selected before starting an agent",
                self.provider_name
            )));
        }
        let (models, _, _) = self.models_with_status(false).await?;
        if !models.iter().any(|model| {
            model.canonical_slug.as_deref().unwrap_or(model.id.as_str()) == requested_model.unwrap()
        }) {
            return Err(ProviderError::Unavailable(format!(
                "{} model '{}' is not in the current tool-capable catalog",
                self.provider_name,
                requested_model.unwrap()
            )));
        }
        let provider_model = self
            .model_namespace
            .as_deref()
            .and_then(|namespace| {
                requested_model
                    .unwrap()
                    .strip_prefix(&format!("{namespace}/"))
            })
            .unwrap_or(requested_model.unwrap())
            .to_string();
        let mut request = request;
        request.model = Some(provider_model);
        // Chat Completions has no provider-side resumable thread. The UUID is
        // Frank's durable transcript key; reuse it across daemon restarts and
        // rebuild the request from provider_session_items instead of inventing
        // a second conversation.
        let session_id = request.resume_session_id.clone().unwrap_or_else(|| {
            format!(
                "{}-{}",
                self.provider_name.to_ascii_lowercase(),
                uuid::Uuid::new_v4()
            )
        });
        let (command_tx, command_rx) = mpsc::channel(32);
        let (events, _) = tokio::sync::broadcast::channel(256);
        let provider_session_id = Arc::new(tokio::sync::Mutex::new(Some(session_id.clone())));
        let runtime = OpenRouterRuntime {
            command_tx,
            stopped: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        let client = self.client.clone();
        let base_url = self.base_url.clone();
        let output_events = events.clone();
        let session_request = request.clone();
        let provider_name = self.provider_name.clone();
        tokio::spawn(async move {
            run_session(
                client,
                base_url,
                key,
                request,
                command_rx,
                output_events,
                provider_name,
            )
            .await;
        });
        Ok(RuntimeSession::new_openrouter(
            session_id,
            provider_session_id,
            runtime,
            events,
            session_request,
        ))
    }

    async fn model_catalog(&self, refresh: bool) -> Result<Vec<ModelDescriptor>> {
        self.models(refresh).await
    }
}

async fn run_session(
    client: Client,
    base_url: String,
    key: String,
    request: StartRequest,
    mut commands: mpsc::Receiver<OpenRouterCommand>,
    events: tokio::sync::broadcast::Sender<RuntimeEvent>,
    provider_name: String,
) {
    let mut messages = vec![json!({
        "role": "system",
        "content": request.instructions,
    })];
    messages.extend(
        request
            .initial_transcript
            .into_iter()
            .filter(valid_transcript_message),
    );
    let tools = tool_definitions();
    let mut pending_tool_calls = HashSet::new();
    let mut completed_tool_calls = messages
        .iter()
        .filter(|message| message.get("role").and_then(Value::as_str) == Some("tool"))
        .filter_map(|message| message.get("tool_call_id").and_then(Value::as_str))
        .map(str::to_string)
        .collect::<HashSet<_>>();
    while let Some(command) = commands.recv().await {
        match command {
            OpenRouterCommand::Message(message) => {
                if !pending_tool_calls.is_empty() {
                    let _ = events.send(RuntimeEvent::Error {
                        message: format!(
                            "{provider_name} is waiting for tool results before accepting another message"
                        ),
                    });
                    continue;
                }
                messages.push(json!({
                    "role": if message.role.trim().is_empty() { "user" } else { message.role.as_str() },
                    "content": message.content,
                }));
                if let Err(error) = complete_turn(
                    &client,
                    &base_url,
                    &key,
                    request.model.as_deref().unwrap_or_default(),
                    &mut messages,
                    &tools,
                    &events,
                    &provider_name,
                )
                .await
                {
                    pending_tool_calls.clear();
                    let _ = events.send(RuntimeEvent::Error {
                        message: error.to_string(),
                    });
                } else {
                    pending_tool_calls = tool_call_ids(&messages);
                }
            }
            OpenRouterCommand::ToolResult { call_id, output } => {
                if completed_tool_calls.contains(&call_id) {
                    // Durable tool results make the command idempotent. A
                    // duplicated provider event or a reconnect replay must
                    // never trigger a second effect or a second turn.
                    continue;
                }
                if !pending_tool_calls.remove(&call_id) {
                    let _ = events.send(RuntimeEvent::Error {
                        message: format!(
                            "{provider_name} received an unexpected tool result for {call_id}"
                        ),
                    });
                    continue;
                }
                completed_tool_calls.insert(call_id.clone());
                messages.push(json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": output,
                }));
                if !pending_tool_calls.is_empty() {
                    continue;
                }
                if let Err(error) = complete_turn(
                    &client,
                    &base_url,
                    &key,
                    request.model.as_deref().unwrap_or_default(),
                    &mut messages,
                    &tools,
                    &events,
                    &provider_name,
                )
                .await
                {
                    pending_tool_calls.clear();
                    let _ = events.send(RuntimeEvent::Error {
                        message: error.to_string(),
                    });
                } else {
                    pending_tool_calls = tool_call_ids(&messages);
                }
            }
            OpenRouterCommand::Stop => break,
        }
    }
    let _ = events.send(RuntimeEvent::Stopped { code: Some(0) });
}

fn tool_call_ids(messages: &[Value]) -> HashSet<String> {
    messages
        .last()
        .and_then(|message| message.get("tool_calls"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|call| {
            call.get("id")
                .and_then(Value::as_str)
                .filter(|id| !id.trim().is_empty())
                .map(str::to_string)
        })
        .collect()
}

#[derive(Debug)]
struct TurnFailure {
    error: ProviderError,
    retryable: bool,
}

#[allow(clippy::too_many_arguments)]
async fn complete_turn(
    client: &Client,
    base_url: &str,
    key: &str,
    model: &str,
    messages: &mut Vec<Value>,
    tools: &[Value],
    events: &tokio::sync::broadcast::Sender<RuntimeEvent>,
    provider_name: &str,
) -> Result<()> {
    for attempt in 0..MAX_TURN_ATTEMPTS {
        match complete_turn_once(
            client,
            base_url,
            key,
            model,
            messages,
            tools,
            events,
            provider_name,
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(failure) if failure.retryable && attempt + 1 < MAX_TURN_ATTEMPTS => {
                let delay = Duration::from_millis(250 * 2_u64.saturating_pow(attempt as u32));
                tokio::time::sleep(delay).await;
            }
            Err(failure) => return Err(failure.error),
        }
    }
    Err(ProviderError::Process(format!(
        "{provider_name} request exhausted its retry budget"
    )))
}

#[allow(clippy::too_many_arguments)]
async fn complete_turn_once(
    client: &Client,
    base_url: &str,
    key: &str,
    model: &str,
    messages: &mut Vec<Value>,
    tools: &[Value],
    events: &tokio::sync::broadcast::Sender<RuntimeEvent>,
    provider_name: &str,
) -> std::result::Result<(), TurnFailure> {
    let mut body = json!({
        "model": model,
        "messages": messages,
        "stream": true,
        "stream_options": {"include_usage": true},
        "tools": tools,
    });
    // OpenAI's reasoning models reject function tools at their default
    // reasoning effort on the Chat Completions endpoint.  Keep the native
    // OpenAI Luna path on the same stable streaming/tool protocol as the
    // OpenRouter adapter, while explicitly selecting the supported effort.
    if provider_name == "OpenAI" {
        body["reasoning_effort"] = json!("none");
    }
    let response = client
        .post(format!("{base_url}/chat/completions"))
        .bearer_auth(key)
        .header("HTTP-Referer", "https://github.com/a-man-called-q/frank")
        .header("X-Title", "Frank")
        .json(&body)
        .send()
        .await
        .map_err(|error| TurnFailure {
            retryable: error.is_timeout(),
            error: ProviderError::Process(sanitize_error(&error.to_string(), Some(key))),
        })?;
    let status = response.status();
    if !status.is_success() {
        return Err(TurnFailure {
            retryable: status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error(),
            error: http_error_for(
                provider_name,
                status,
                response.text().await.unwrap_or_default(),
                Some(key),
            ),
        });
    }
    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    let mut text = String::new();
    let mut tool_calls: HashMap<usize, ToolAccumulator> = HashMap::new();
    let mut usage = None;
    let mut stream_started = false;
    let mut received_output = false;
    let mut retryable_stream_error = false;
    let mut done_received = false;
    let stream_result: Result<()> = async {
        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(chunk) => chunk,
                Err(error) => {
                    retryable_stream_error = error.is_timeout() && !received_output;
                    return Err(ProviderError::Process(sanitize_error(
                        &error.to_string(),
                        Some(key),
                    )));
                }
            };
            stream_started = true;
            buffer.extend_from_slice(&chunk);
            while let Some(index) = buffer.iter().position(|byte| *byte == b'\n') {
                if index.saturating_add(1) > MAX_SSE_FRAME_BYTES {
                    return Err(ProviderError::Malformed(format!(
                        "{provider_name} SSE frame exceeds the configured size cap"
                    )));
                }
                let line = buffer.drain(..=index).collect::<Vec<_>>();
                let line = std::str::from_utf8(&line)
                    .map_err(|_| {
                        ProviderError::Malformed(format!("{provider_name} SSE is not UTF-8"))
                    })?
                    .trim();
                if let Some(data) = line.strip_prefix("data:") {
                    let data = data.trim();
                    if data == "[DONE]" {
                        done_received = true;
                        continue;
                    }
                    stream_started = true;
                    let frame: Value = serde_json::from_str(data)
                        .map_err(|error| ProviderError::Malformed(error.to_string()))?;
                    if frame.get("error").is_some() {
                        return Err(ProviderError::Malformed(
                            frame
                                .get("error")
                                .map(|value| sanitize_error(&value.to_string(), Some(key)))
                                .unwrap_or_else(|| {
                                    format!("{provider_name} returned a stream error")
                                }),
                        ));
                    }
                    if frame
                        .get("choices")
                        .and_then(Value::as_array)
                        .is_some_and(|choices| {
                            choices.iter().any(|choice| {
                                choice
                                    .get("delta")
                                    .and_then(|delta| delta.get("content"))
                                    .and_then(Value::as_str)
                                    .is_some_and(|value| !value.is_empty())
                                    || choice
                                        .get("delta")
                                        .and_then(|delta| delta.get("tool_calls"))
                                        .and_then(Value::as_array)
                                        .is_some_and(|calls| !calls.is_empty())
                            })
                        })
                    {
                        received_output = true;
                    }
                    parse_chunk(
                        &frame,
                        &mut text,
                        &mut tool_calls,
                        &mut usage,
                        events,
                        provider_name,
                    )?;
                }
            }
            // Drain complete SSE lines before applying the cap. One network
            // chunk may contain many valid frames; the cap applies to the
            // incomplete frame retained between chunks, not to the aggregate
            // size of already-consumed lines.
            if buffer.len() > MAX_SSE_FRAME_BYTES {
                return Err(ProviderError::Malformed(format!(
                    "{provider_name} SSE frame exceeds the configured size cap"
                )));
            }
        }
        if !buffer.is_empty() {
            return Err(ProviderError::Malformed(format!(
                "{provider_name} stream ended mid-frame"
            )));
        }
        let mut runtime_calls = Vec::new();
        let mut serialized_calls = Vec::new();
        if !tool_calls.is_empty() {
            let mut calls = tool_calls.into_iter().collect::<Vec<_>>();
            calls.sort_by_key(|(index, _)| *index);
            for (_, call) in calls {
                if call.id.trim().is_empty() || call.name.trim().is_empty() {
                    return Err(ProviderError::Malformed(format!(
                        "{provider_name} tool call is missing an id or function name"
                    )));
                }
                let input = serde_json::from_str::<Value>(&call.arguments)
                    .unwrap_or_else(|_| json!({"raw_arguments": call.arguments}));
                runtime_calls.push(RuntimeToolCall {
                    call_id: call.id.clone(),
                    name: call.name.clone(),
                    input: input.clone(),
                });
                serialized_calls.push(json!({
                    "id": call.id,
                    "type": "function",
                    "function": {"name": call.name, "arguments": call.arguments},
                }));
            }
        }
        let mut turn_id = None;
        if !text.is_empty() || !runtime_calls.is_empty() {
            let mut assistant = json!({
                "role": "assistant",
                "content": if text.is_empty() { Value::Null } else { Value::String(text.clone()) },
            });
            if !serialized_calls.is_empty() {
                assistant["tool_calls"] = Value::Array(serialized_calls);
            }
            if serde_json::to_vec(&assistant)
                .is_ok_and(|value| value.len() > frank_protocol::MAX_MESSAGE_BODY_BYTES)
            {
                return Err(ProviderError::Malformed(format!(
                    "{provider_name} assistant turn exceeds the configured size cap"
                )));
            }
            messages.push(assistant);
            let current_turn_id = uuid::Uuid::new_v4().to_string();
            turn_id = Some(current_turn_id.clone());
            let _ = events.send(RuntimeEvent::AssistantMessage {
                turn_id: current_turn_id,
                content: text.clone(),
                tool_calls: runtime_calls.clone(),
            });
            for call in &runtime_calls {
                let _ = events.send(RuntimeEvent::ToolCall {
                    call_id: call.call_id.clone(),
                    name: call.name.clone(),
                    input: call.input.clone(),
                });
            }
        }
        if runtime_calls.is_empty() && turn_id.is_none() {
            // A successful provider response may contain no visible text
            // (for example a filtered/empty answer) while still being a
            // complete turn. Emit the same terminal boundary so the agent
            // cannot remain Working forever waiting for a token that never
            // arrives.
            let current_turn_id = uuid::Uuid::new_v4().to_string();
            turn_id = Some(current_turn_id.clone());
            let _ = events.send(RuntimeEvent::AssistantMessage {
                turn_id: current_turn_id,
                content: String::new(),
                tool_calls: Vec::new(),
            });
        }
        if let Some(usage) = usage.clone() {
            let _ = events.send(RuntimeEvent::Usage(usage));
        }
        // TurnCompleted causes the orchestrator to finalize and remove the
        // live session. Emit usage first so the final turn is still recorded
        // and budgeted instead of being treated as a stale post-finalization
        // frame.
        if runtime_calls.is_empty()
            && let Some(turn_id) = turn_id
        {
            let _ = events.send(RuntimeEvent::TurnCompleted { turn_id });
        }
        Ok(())
    }
    .await;
    match stream_result {
        Ok(()) => Ok(()),
        Err(error) => {
            if stream_started && (usage.is_none() || !done_received) {
                let estimated_input = serde_json::to_string(&body)
                    .ok()
                    .map(|value| ((value.len() as u64).saturating_add(3)) / 4);
                let estimated_output = Some(((text.len() as u64).saturating_add(3)) / 4);
                let _ = events.send(RuntimeEvent::Usage(UsageTelemetry::estimated(
                    estimated_input,
                    estimated_output,
                )));
            }
            Err(TurnFailure {
                retryable: retryable_stream_error,
                error,
            })
        }
    }
}

#[derive(Debug, Default)]
struct ToolAccumulator {
    id: String,
    name: String,
    arguments: String,
}

fn parse_chunk(
    frame: &Value,
    text: &mut String,
    tool_calls: &mut HashMap<usize, ToolAccumulator>,
    usage: &mut Option<UsageTelemetry>,
    events: &tokio::sync::broadcast::Sender<RuntimeEvent>,
    provider_name: &str,
) -> Result<()> {
    if let Some(value) = frame.get("usage") {
        let input = value.get("prompt_tokens").and_then(Value::as_u64);
        let output = value.get("completion_tokens").and_then(Value::as_u64);
        let cached = value
            .get("prompt_tokens_details")
            .and_then(|details| details.get("cached_tokens"))
            .and_then(Value::as_u64);
        let reasoning = value
            .get("completion_tokens_details")
            .and_then(|details| details.get("reasoning_tokens"))
            .and_then(Value::as_u64);
        let cost_micros = value.get("cost").and_then(cost_micros);
        *usage = Some(UsageTelemetry {
            measured_input_tokens: input,
            measured_output_tokens: output,
            estimated_input_tokens: None,
            estimated_output_tokens: None,
            cost_micros,
            cached_input_tokens: cached,
            reasoning_tokens: reasoning,
        });
    }
    let Some(choice) = frame
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
    else {
        return Ok(());
    };
    let delta = choice.get("delta").cloned().unwrap_or(Value::Null);
    if let Some(value) = delta.get("content").and_then(Value::as_str) {
        if text.len().saturating_add(value.len()) > frank_protocol::MAX_MESSAGE_BODY_BYTES {
            return Err(ProviderError::Malformed(format!(
                "{provider_name} assistant content exceeds the configured size cap"
            )));
        }
        text.push_str(value);
        let _ = events.send(RuntimeEvent::Text { text: value.into() });
    }
    if let Some(calls) = delta.get("tool_calls").and_then(Value::as_array) {
        for call in calls {
            let index = call
                .get("index")
                .and_then(Value::as_u64)
                .unwrap_or_default() as usize;
            let accumulator = tool_calls.entry(index).or_default();
            if let Some(id) = call.get("id").and_then(Value::as_str) {
                if id.len() > frank_protocol::MAX_MESSAGE_BODY_BYTES {
                    return Err(ProviderError::Malformed(format!(
                        "{provider_name} tool call id exceeds the configured size cap"
                    )));
                }
                accumulator.id = id.to_string();
            }
            if let Some(function) = call.get("function") {
                if let Some(name) = function.get("name").and_then(Value::as_str) {
                    if accumulator.name.len().saturating_add(name.len())
                        > frank_protocol::MAX_MESSAGE_BODY_BYTES
                    {
                        return Err(ProviderError::Malformed(format!(
                            "{provider_name} tool name exceeds the configured size cap"
                        )));
                    }
                    accumulator.name.push_str(name);
                }
                if let Some(arguments) = function.get("arguments").and_then(Value::as_str) {
                    if accumulator.arguments.len().saturating_add(arguments.len())
                        > frank_protocol::MAX_MESSAGE_BODY_BYTES
                    {
                        return Err(ProviderError::Malformed(format!(
                            "{provider_name} tool arguments exceed the configured size cap"
                        )));
                    }
                    accumulator.arguments.push_str(arguments);
                }
            }
        }
    }
    Ok(())
}

pub fn tool_definitions() -> Vec<Value> {
    frank_tool_catalog::openrouter_definitions()
}

fn model_is_eligible(
    model: &ModelDescriptor,
    catalog_requires_tools: bool,
    namespace: Option<&str>,
) -> bool {
    if catalog_requires_tools {
        return model
            .supported_parameters
            .iter()
            .any(|parameter| parameter == "tools");
    }
    if namespace == Some("openai") {
        // OpenAI's `/models` response does not expose `supported_parameters`.
        // Keep only known chat/tool families so embeddings, image, and audio
        // models do not appear as runnable worker choices.
        return openai_model_supports_tools(&model.id);
    }
    true
}

fn model_descriptor(value: &Value, namespace: Option<&str>) -> Option<ModelDescriptor> {
    let id = value.get("id")?.as_str()?.to_string();
    let provider_canonical_slug = value
        .get("canonical_slug")
        .and_then(Value::as_str)
        .map(str::to_string);
    // OpenRouter currently gives paid and `:free` variants the same dated
    // canonical slug.  Keeping that slug would collapse the two entries in
    // the desktop picker and could silently send a Free selection through a
    // paid route.  The provider id is the only unambiguous invocation key for
    // free variants, so preserve it as Frank's canonical selection value.
    let canonical_slug = if id.ends_with(":free") {
        id.clone()
    } else {
        provider_canonical_slug.unwrap_or_else(|| id.clone())
    };
    let pricing = value.get("pricing");
    let mut supported_parameters: Vec<String> = value
        .get("supported_parameters")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if namespace == Some("openai")
        && openai_model_supports_tools(&id)
        && !supported_parameters
            .iter()
            .any(|parameter| parameter == "tools")
    {
        supported_parameters.push("tools".to_string());
    }
    let canonical_slug = match namespace {
        Some(namespace) if !canonical_slug.starts_with(&format!("{namespace}/")) => {
            format!("{namespace}/{canonical_slug}")
        }
        _ => canonical_slug,
    };
    Some(ModelDescriptor {
        id,
        name: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        canonical_slug: Some(canonical_slug),
        context_length: value.get("context_length").and_then(Value::as_u64),
        input_price_per_token: pricing
            .and_then(|pricing| pricing.get("prompt"))
            .and_then(Value::as_str)
            .map(str::to_string),
        output_price_per_token: pricing
            .and_then(|pricing| pricing.get("completion"))
            .and_then(Value::as_str)
            .map(str::to_string),
        supported_parameters,
        deprecated_at: value
            .get("deprecated_at")
            .or_else(|| value.get("expiration_date"))
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

fn openai_model_supports_tools(id: &str) -> bool {
    let normalized = id.to_ascii_lowercase();
    normalized.starts_with("gpt-4")
        || normalized.starts_with("gpt-5")
        || normalized.starts_with("o1")
        || normalized.starts_with("o3")
        || normalized.starts_with("o4")
        || normalized.starts_with("chatgpt-")
}

fn http_error_for(
    provider: &str,
    status: StatusCode,
    body: String,
    secret: Option<&str>,
) -> ProviderError {
    let detail = body
        .chars()
        .take(512)
        .collect::<String>()
        .replace(['\n', '\r'], " ");
    let detail = secret
        .filter(|secret| !secret.is_empty())
        .map_or(detail.clone(), |secret| {
            detail.replace(secret, "[redacted]")
        });
    ProviderError::Process(if detail.trim().is_empty() {
        format!("{provider} returned HTTP {status}")
    } else {
        format!("{provider} returned HTTP {status}: {detail}")
    })
}

fn sanitize_error(error: &str, secret: Option<&str>) -> String {
    let error = secret.filter(|secret| !secret.is_empty()).map_or_else(
        || error.to_string(),
        |secret| error.replace(secret, "[redacted]"),
    );
    error
        .replace("OPENROUTER_API_KEY", "provider credential")
        .chars()
        .take(512)
        .collect()
}

fn cost_micros(value: &Value) -> Option<u64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|value| value.parse::<f64>().ok()))
        .filter(|cost| cost.is_finite() && *cost >= 0.0)
        .map(|cost| (cost * 1_000_000.0).round() as u64)
}

fn valid_transcript_message(value: &Value) -> bool {
    matches!(
        value.get("role").and_then(Value::as_str),
        Some("user" | "assistant" | "tool")
    )
}

#[allow(dead_code)]
fn _cache_ttl() -> Duration {
    MODEL_CACHE_TTL
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use async_trait::async_trait;
    use axum::body::Body;
    use axum::extract::State;
    use axum::http::{HeaderMap, StatusCode, header};
    use axum::response::{IntoResponse, Response};
    use axum::{
        Json, Router,
        routing::{get, post},
    };
    use tokio::net::TcpListener;
    use tokio::sync::Mutex;

    use super::*;

    const TEST_KEY: &str = "test-openrouter-secret";

    #[derive(Clone)]
    struct FixedCredentials;

    #[async_trait]
    impl CredentialResolver for FixedCredentials {
        async fn api_key(&self) -> Result<Option<String>> {
            Ok(Some(TEST_KEY.into()))
        }

        fn credential_source(&self) -> Option<String> {
            Some("test".into())
        }
    }

    #[derive(Clone)]
    struct FakeResponse {
        status: u16,
        body: String,
        stream: bool,
        delay: Duration,
    }

    impl FakeResponse {
        fn json(status: StatusCode, body: Value) -> Self {
            Self {
                status: status.as_u16(),
                body: body.to_string(),
                stream: false,
                delay: Duration::ZERO,
            }
        }

        fn sse(body: String) -> Self {
            Self {
                status: StatusCode::OK.as_u16(),
                body,
                stream: true,
                delay: Duration::ZERO,
            }
        }

        fn status(status: StatusCode, body: &str) -> Self {
            Self {
                status: status.as_u16(),
                body: body.into(),
                stream: false,
                delay: Duration::ZERO,
            }
        }

        fn delayed(mut self, delay: Duration) -> Self {
            self.delay = delay;
            self
        }
    }

    #[derive(Clone)]
    struct FakeState {
        model_responses: Arc<Mutex<VecDeque<FakeResponse>>>,
        chat_responses: Arc<Mutex<VecDeque<FakeResponse>>>,
        chat_calls: Arc<AtomicUsize>,
        authorization: Arc<Mutex<Vec<String>>>,
        request_bodies: Arc<Mutex<Vec<Value>>>,
    }

    struct FakeServer {
        base_url: String,
        state: FakeState,
        task: tokio::task::JoinHandle<()>,
    }

    impl Drop for FakeServer {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    fn catalog_payload() -> Value {
        json!({
            "data": [
                {
                    "id": "provider/internal-name",
                    "name": "Tool Model",
                    "canonical_slug": "test/model",
                    "context_length": 32768,
                    "pricing": {"prompt": "0.000001", "completion": "0.000002"},
                    "supported_parameters": ["tools", "temperature"]
                },
                {
                    "id": "provider/no-tools",
                    "name": "Text Only",
                    "context_length": 4096,
                    "pricing": {"prompt": "0", "completion": "0"},
                    "supported_parameters": ["temperature"]
                }
            ]
        })
    }

    fn sse(frames: &[Value]) -> String {
        let mut body = frames
            .iter()
            .map(|frame| format!("data: {}\n\n", frame))
            .collect::<String>();
        body.push_str("data: [DONE]\n\n");
        body
    }

    async fn fake_models(State(state): State<FakeState>, headers: HeaderMap) -> Response {
        record_authorization(&state, &headers).await;
        let response = state
            .model_responses
            .lock()
            .await
            .pop_front()
            .unwrap_or_else(|| FakeResponse::json(StatusCode::OK, catalog_payload()));
        fake_response(response).await
    }

    async fn fake_chat(
        State(state): State<FakeState>,
        headers: HeaderMap,
        Json(body): Json<Value>,
    ) -> Response {
        record_authorization(&state, &headers).await;
        state.request_bodies.lock().await.push(body);
        state.chat_calls.fetch_add(1, Ordering::SeqCst);
        let response = state
            .chat_responses
            .lock()
            .await
            .pop_front()
            .unwrap_or_else(|| {
                FakeResponse::sse(sse(&[json!({
                    "choices": [{"delta": {"content": "ok"}}]
                })]))
            });
        fake_response(response).await
    }

    async fn record_authorization(state: &FakeState, headers: &HeaderMap) {
        if let Some(value) = headers
            .get(header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
        {
            state.authorization.lock().await.push(value.into());
        }
    }

    async fn fake_response(response: FakeResponse) -> Response {
        if !response.delay.is_zero() {
            tokio::time::sleep(response.delay).await;
        }
        let status = StatusCode::from_u16(response.status).unwrap();
        let content_type = if response.stream {
            "text/event-stream"
        } else {
            "application/json"
        };
        Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, content_type)
            .body(Body::from(response.body))
            .unwrap()
            .into_response()
    }

    async fn fake_server() -> FakeServer {
        let state = FakeState {
            model_responses: Arc::new(Mutex::new(VecDeque::new())),
            chat_responses: Arc::new(Mutex::new(VecDeque::new())),
            chat_calls: Arc::new(AtomicUsize::new(0)),
            authorization: Arc::new(Mutex::new(Vec::new())),
            request_bodies: Arc::new(Mutex::new(Vec::new())),
        };
        let app = Router::new()
            .route("/api/v1/models", get(fake_models))
            .route("/api/v1/chat/completions", post(fake_chat))
            .with_state(state.clone());
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        FakeServer {
            base_url: format!("http://{address}/api/v1"),
            state,
            task,
        }
    }

    fn adapter(server: &FakeServer) -> OpenRouterAdapter {
        OpenRouterAdapter::new(Arc::new(FixedCredentials))
            .unwrap()
            .with_base_url(&server.base_url)
    }

    fn text_response(text: &str) -> FakeResponse {
        FakeResponse::sse(sse(&[
            json!({"choices": [{"delta": {"role": "assistant", "content": text}}]}),
            json!({
                "choices": [],
                "usage": {
                    "prompt_tokens": 10,
                    "completion_tokens": 5,
                    "prompt_tokens_details": {"cached_tokens": 2},
                    "completion_tokens_details": {"reasoning_tokens": 3},
                    "cost": "0.000012"
                }
            }),
        ]))
    }

    async fn complete_for_test(
        server: &FakeServer,
        response: FakeResponse,
    ) -> (Result<()>, Vec<RuntimeEvent>, Vec<Value>) {
        server.state.chat_responses.lock().await.push_back(response);
        let mut messages = vec![json!({"role": "user", "content": "hello"})];
        let (events, _) = tokio::sync::broadcast::channel(128);
        let mut receiver = events.subscribe();
        let result = complete_turn(
            &adapter(server).client,
            &server.base_url,
            TEST_KEY,
            "test/model",
            &mut messages,
            &tool_definitions(),
            &events,
            "OpenRouter",
        )
        .await;
        let mut seen = Vec::new();
        while let Ok(event) = receiver.try_recv() {
            seen.push(event);
        }
        (result, seen, messages)
    }

    #[tokio::test]
    async fn catalog_filters_non_tool_models_and_preserves_last_known_good() {
        let server = fake_server().await;
        let adapter = adapter(&server);
        let (models, _, stale) = adapter.models_with_status(true).await.unwrap();
        assert!(!stale);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].canonical_slug.as_deref(), Some("test/model"));
        assert_eq!(models[0].id, "provider/internal-name");

        server
            .state
            .model_responses
            .lock()
            .await
            .push_back(FakeResponse::status(
                StatusCode::SERVICE_UNAVAILABLE,
                &format!("provider failed with {TEST_KEY}"),
            ));
        let (cached, refreshed_at, stale) = adapter.models_with_status(true).await.unwrap();
        assert!(stale);
        assert_eq!(cached, models);
        assert!(refreshed_at.is_some());
    }

    #[tokio::test]
    async fn streaming_text_records_measured_usage_and_redacts_authorization() {
        let server = fake_server().await;
        let (result, events, messages) = complete_for_test(&server, text_response("hello")).await;
        result.unwrap();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, RuntimeEvent::Text { text } if text == "hello"))
        );
        assert!(
            events
                .iter()
                .any(|event| matches!(event, RuntimeEvent::Usage(usage)
            if usage.measured_input_tokens == Some(10)
                && usage.measured_output_tokens == Some(5)
                && usage.cached_input_tokens == Some(2)
                && usage.reasoning_tokens == Some(3)
                && usage.cost_micros == Some(12)))
        );
        assert_eq!(messages.last().unwrap()["role"], "assistant");
        assert_eq!(
            server.state.authorization.lock().await[0],
            format!("Bearer {TEST_KEY}")
        );
    }

    #[tokio::test]
    async fn parallel_tool_deltas_become_ordered_idempotent_calls() {
        let server = fake_server().await;
        let response = FakeResponse::sse(sse(&[
            json!({"choices": [{"delta": {"tool_calls": [
                {"index": 0, "id": "call-a", "function": {"name": "workspace_read", "arguments": r#"{"path":""#}},
                {"index": 1, "id": "call-b", "function": {"name": "workspace_list", "arguments": "{}"}}
            ]}}]}),
            json!({"choices": [{"delta": {"tool_calls": [
                {"index": 0, "function": {"arguments": r#"README.md"}"#}}
            ]}}]}),
        ]));
        let (result, events, messages) = complete_for_test(&server, response).await;
        result.unwrap();
        let calls = events
            .iter()
            .find_map(|event| match event {
                RuntimeEvent::AssistantMessage { tool_calls, .. } => Some(tool_calls.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].call_id, "call-a");
        assert_eq!(calls[0].name, "workspace_read");
        assert_eq!(calls[1].call_id, "call-b");
        assert_eq!(messages.last().unwrap()["tool_calls"][0]["id"], "call-a");
    }

    #[tokio::test]
    async fn runtime_waits_for_all_parallel_tool_results_before_resuming() {
        let server = fake_server().await;
        let first_turn = FakeResponse::sse(sse(&[json!({
            "choices": [{"delta": {"tool_calls": [
                {"index": 0, "id": "call-a", "function": {"name": "workspace_read", "arguments": "{}"}},
                {"index": 1, "id": "call-b", "function": {"name": "workspace_list", "arguments": "{}"}}
            ]}}]
        })]));
        server
            .state
            .chat_responses
            .lock()
            .await
            .extend([first_turn, text_response("both results received")]);

        let request = StartRequest {
            agent_id: "agent-a".into(),
            task_id: Some("task-a".into()),
            cwd: "/tmp".into(),
            instructions: "test".into(),
            policy: frank_protocol::AgentPolicy::default(),
            model: Some("test/model".into()),
            resume_session_id: None,
            server_url: None,
            server_certificate_fingerprint: None,
            session_capability: None,
            initial_transcript: Vec::new(),
            reasoning_effort: Some(frank_protocol::ReasoningEffort::Max),
        };
        let session = adapter(&server).start(request).await.unwrap();
        let mut events = session.events().await.unwrap();
        session
            .send(&crate::ProviderMessage {
                role: "user".into(),
                content: "make the change".into(),
                correlation_id: None,
            })
            .await
            .unwrap();

        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        let mut calls = Vec::new();
        while calls.len() < 2 {
            let event = tokio::time::timeout_at(deadline, events.recv())
                .await
                .unwrap()
                .unwrap();
            if let RuntimeEvent::ToolCall { call_id, .. } = event {
                calls.push(call_id);
            }
        }
        assert_eq!(server.state.chat_calls.load(Ordering::SeqCst), 1);

        session
            .submit_tool_result(&calls[0], "first")
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(server.state.chat_calls.load(Ordering::SeqCst), 1);

        session
            .submit_tool_result(&calls[1], "second")
            .await
            .unwrap();
        loop {
            let event = tokio::time::timeout_at(deadline, events.recv())
                .await
                .unwrap()
                .unwrap();
            if matches!(event, RuntimeEvent::Text { ref text } if text == "both results received") {
                break;
            }
        }
        assert_eq!(server.state.chat_calls.load(Ordering::SeqCst), 2);
        let bodies = server.state.request_bodies.lock().await;
        let messages = bodies[1]["messages"].as_array().unwrap();
        assert_eq!(
            messages
                .iter()
                .filter(|message| message["role"] == "tool")
                .count(),
            2
        );
        drop(bodies);
        session
            .submit_tool_result(&calls[0], "duplicate must be ignored")
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(server.state.chat_calls.load(Ordering::SeqCst), 2);
        session.graceful_stop().await.unwrap();
    }

    #[tokio::test]
    async fn retryable_statuses_retry_but_partial_stream_never_replays() {
        let server = fake_server().await;
        {
            let mut responses = server.state.chat_responses.lock().await;
            responses.push_back(FakeResponse::status(StatusCode::TOO_MANY_REQUESTS, "busy"));
            responses.push_back(FakeResponse::status(StatusCode::BAD_GATEWAY, "upstream"));
            responses.push_back(text_response("recovered"));
        }
        let (result, events, _) = complete_for_test(
            &server,
            FakeResponse::status(StatusCode::IM_A_TEAPOT, "unused"),
        )
        .await;
        result.unwrap();
        assert!(
            events
                .iter()
                .any(|event| matches!(event, RuntimeEvent::Text { text } if text == "recovered"))
        );
        assert_eq!(server.state.chat_calls.load(Ordering::SeqCst), 3);

        let server = fake_server().await;
        let partial = FakeResponse::sse(
            "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\ndata: {malformed}\n\n"
                .into(),
        );
        server
            .state
            .chat_responses
            .lock()
            .await
            .extend([partial, text_response("must-not-replay")]);
        let (result, events, _) = complete_for_test(
            &server,
            FakeResponse::status(StatusCode::IM_A_TEAPOT, "unused"),
        )
        .await;
        assert!(result.is_err());
        assert!(
            events
                .iter()
                .any(|event| matches!(event, RuntimeEvent::Text { text } if text == "partial"))
        );
        assert_eq!(server.state.chat_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn unauthorized_timeout_malformed_and_oversized_frames_fail_closed() {
        let server = fake_server().await;
        let (result, _, _) = complete_for_test(
            &server,
            FakeResponse::status(StatusCode::UNAUTHORIZED, &format!("invalid key {TEST_KEY}")),
        )
        .await;
        let error = result.unwrap_err().to_string();
        assert!(!error.contains(TEST_KEY));
        assert_eq!(server.state.chat_calls.load(Ordering::SeqCst), 1);

        let server = fake_server().await;
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(1))
            .timeout(Duration::from_millis(10))
            .build()
            .unwrap();
        server.state.chat_responses.lock().await.extend([
            text_response("too late").delayed(Duration::from_millis(100)),
            text_response("too late").delayed(Duration::from_millis(100)),
            text_response("too late").delayed(Duration::from_millis(100)),
        ]);
        let (events, _) = tokio::sync::broadcast::channel(128);
        let mut messages = vec![json!({"role": "user", "content": "hello"})];
        let result = complete_turn(
            &client,
            &server.base_url,
            TEST_KEY,
            "test/model",
            &mut messages,
            &tool_definitions(),
            &events,
            "OpenRouter",
        )
        .await;
        assert!(result.is_err());

        let server = fake_server().await;
        let (result, events, _) =
            complete_for_test(&server, FakeResponse::sse("data: {broken}\n\n".into())).await;
        assert!(matches!(result, Err(ProviderError::Malformed(_))));
        assert!(
            events
                .iter()
                .any(|event| matches!(event, RuntimeEvent::Usage(usage)
            if usage.measured_input_tokens.is_none()
                && usage.estimated_input_tokens.is_some()
                && usage.cost_micros.is_none()))
        );

        let server = fake_server().await;
        let oversized = format!("data: {}\n\n", "x".repeat(MAX_SSE_FRAME_BYTES));
        let (result, _, _) = complete_for_test(&server, FakeResponse::sse(oversized)).await;
        assert!(
            matches!(result, Err(ProviderError::Malformed(message)) if message.contains("size cap"))
        );
    }

    #[tokio::test]
    async fn probe_reports_api_failure_without_exposing_the_key() {
        let server = fake_server().await;
        server
            .state
            .model_responses
            .lock()
            .await
            .push_back(FakeResponse::status(
                StatusCode::UNAUTHORIZED,
                &format!("bad {TEST_KEY}"),
            ));
        let probe = adapter(&server).probe().await;
        assert!(probe.capability.configured);
        assert!(!probe.capability.available);
        let diagnostic = probe.capability.diagnostic.unwrap();
        assert!(!diagnostic.contains(TEST_KEY));
    }

    #[test]
    fn tool_schemas_are_closed_and_model_slug_is_canonical() {
        let definitions = tool_definitions();
        assert!(definitions.iter().all(|definition| {
            definition["function"]["parameters"]["additionalProperties"] == false
        }));
        let descriptor = model_descriptor(
            &json!({"id": "provider/name", "name": "Name", "supported_parameters": ["tools"]}),
            None,
        )
        .unwrap();
        assert_eq!(descriptor.canonical_slug.as_deref(), Some("provider/name"));

        let free_descriptor = model_descriptor(
            &json!({
                "id": "provider/name:free",
                "canonical_slug": "provider/name-20260914",
                "name": "Name (free)",
                "pricing": {"prompt": "0", "completion": "0"},
                "supported_parameters": ["tools"],
            }),
            None,
        )
        .unwrap();
        assert_eq!(
            free_descriptor.canonical_slug.as_deref(),
            Some("provider/name:free")
        );

        let openai_descriptor = model_descriptor(
            &json!({"id": "gpt-4o-mini", "name": "GPT-4o mini"}),
            Some("openai"),
        )
        .unwrap();
        assert_eq!(
            openai_descriptor.canonical_slug.as_deref(),
            Some("openai/gpt-4o-mini")
        );
        assert!(
            openai_descriptor
                .supported_parameters
                .iter()
                .any(|parameter| parameter == "tools")
        );
        assert!(model_is_eligible(&openai_descriptor, false, Some("openai")));
        let embedding_descriptor = model_descriptor(
            &json!({"id": "text-embedding-3-small", "name": "Embedding"}),
            Some("openai"),
        )
        .unwrap();
        assert!(!model_is_eligible(
            &embedding_descriptor,
            false,
            Some("openai")
        ));
    }
}
