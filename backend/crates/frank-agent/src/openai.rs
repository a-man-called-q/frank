//! Native OpenAI Responses API runtime.
//!
//! OpenAI is intentionally a separate adapter from OpenRouter. The two
//! providers share Frank's provider-neutral events and tool catalog, but the
//! native OpenAI path uses Responses items, SSE events, response ids, and
//! `previous_response_id` for efficient continuation. Frank's durable
//! transcript remains the recovery source when a response id is no longer
//! usable.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use frank_protocol::{ModelDescriptor, OpenRouterConnectionView, Timestamp};
use futures_util::StreamExt;
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::{
    CredentialResolver, OpenRouterAdapter, OpenRouterCommand, OpenRouterRuntime, ProviderError,
    Result, RuntimeAdapter, RuntimeEvent, RuntimeProbe, RuntimeSession, RuntimeToolCall,
    StartRequest, UsageTelemetry,
};

pub const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const MAX_SSE_FRAME_BYTES: usize = frank_protocol::MAX_COMMAND_BODY_BYTES;
const MAX_TURN_ATTEMPTS: usize = 3;

#[derive(Clone)]
pub struct OpenAiAdapter {
    // Reuse the hardened HTTP client, credential resolver, model cache, and
    // catalog normalization. Generation itself never goes through the
    // OpenRouter Chat Completions implementation.
    inner: OpenRouterAdapter,
}

impl std::fmt::Debug for OpenAiAdapter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenAiAdapter")
            .field("base_url", &self.inner.base_url)
            .finish_non_exhaustive()
    }
}

impl OpenAiAdapter {
    pub fn new(credentials: Arc<dyn CredentialResolver>) -> Result<Self> {
        let inner = OpenRouterAdapter::new(credentials)?
            .with_base_url(DEFAULT_BASE_URL)
            .with_provider_name("OpenAI")
            .with_catalog_requires_tools(false)
            .with_model_namespace("openai");
        Ok(Self { inner })
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.inner = self.inner.with_base_url(base_url);
        self
    }

    pub async fn connection(&self) -> OpenRouterConnectionView {
        self.inner.connection().await
    }

    pub async fn models_with_status(
        &self,
        refresh: bool,
    ) -> Result<(Vec<ModelDescriptor>, Option<Timestamp>, bool)> {
        self.inner.models_with_status(refresh).await
    }

    pub async fn models(&self, refresh: bool) -> Result<Vec<ModelDescriptor>> {
        self.inner.models(refresh).await
    }

    async fn key(&self) -> Result<String> {
        self.inner
            .credentials
            .api_key()
            .await?
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                ProviderError::Unavailable("OpenAI API key is not configured".to_string())
            })
    }
}

#[async_trait]
impl RuntimeAdapter for OpenAiAdapter {
    async fn probe(&self) -> RuntimeProbe {
        let mut probe = self.inner.probe().await;
        if probe.capability.available {
            probe
                .capability
                .capabilities
                .extend(["responses-api".into(), "previous-response-id".into()]);
        }
        probe
    }

    async fn start(&self, request: StartRequest) -> Result<RuntimeSession> {
        let key = self.key().await?;
        let Some(requested_model) = request.model.as_deref().filter(|value| !value.is_empty())
        else {
            return Err(ProviderError::Unavailable(
                "an OpenAI model must be selected before starting an agent".into(),
            ));
        };
        let (models, _, _) = self.models_with_status(false).await?;
        if !models.iter().any(|model| {
            model.canonical_slug.as_deref().unwrap_or(model.id.as_str()) == requested_model
        }) {
            return Err(ProviderError::Unavailable(format!(
                "OpenAI model '{requested_model}' is not in the current catalog"
            )));
        }
        let provider_model = requested_model
            .strip_prefix("openai/")
            .unwrap_or(requested_model)
            .to_string();
        let mut request = request;
        request.model = Some(provider_model);

        let session_id = request
            .resume_session_id
            .clone()
            .unwrap_or_else(|| format!("openai-{}", uuid::Uuid::new_v4()));
        let provider_session_id = Arc::new(tokio::sync::Mutex::new(
            request
                .resume_session_id
                .clone()
                .or_else(|| Some(session_id.clone())),
        ));
        let (command_tx, command_rx) = mpsc::channel(32);
        let (events, _) = tokio::sync::broadcast::channel(256);
        let runtime = OpenRouterRuntime {
            command_tx,
            stopped: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        };
        let output_events = events.clone();
        let provider_session_id_for_task = provider_session_id.clone();
        let client = self.inner.client.clone();
        let base_url = self.inner.base_url.clone();
        let session_request = request.clone();
        tokio::spawn(async move {
            run_responses_session(
                client,
                base_url,
                key,
                request,
                command_rx,
                output_events,
                provider_session_id_for_task,
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

async fn run_responses_session(
    client: Client,
    base_url: String,
    key: String,
    request: StartRequest,
    mut commands: mpsc::Receiver<OpenRouterCommand>,
    events: tokio::sync::broadcast::Sender<RuntimeEvent>,
    provider_session_id: Arc<tokio::sync::Mutex<Option<String>>>,
) {
    let mut previous_response_id = request
        .resume_session_id
        .clone()
        .filter(|value| value.starts_with("resp_"));
    let mut initial_input = response_input_from_transcript(&request.initial_transcript);
    let mut pending: HashMap<String, RuntimeToolCall> = HashMap::new();
    let mut queued_outputs = Vec::new();

    while let Some(command) = commands.recv().await {
        match command {
            OpenRouterCommand::Message(message) => {
                if !pending.is_empty() {
                    let _ = events.send(RuntimeEvent::Error {
                        message:
                            "OpenAI is waiting for tool results before accepting another message"
                                .into(),
                    });
                    continue;
                }
                let message_input = response_message(&message);
                if previous_response_id.is_none() && !initial_input.is_empty() {
                    initial_input.push(message_input);
                } else {
                    initial_input = vec![message_input];
                }
                match complete_response_with_retry(
                    &client,
                    &base_url,
                    &key,
                    &request,
                    previous_response_id.as_deref(),
                    &initial_input,
                    &events,
                )
                .await
                {
                    Ok(turn) => {
                        previous_response_id = Some(turn.response_id.clone());
                        *provider_session_id.lock().await = Some(turn.response_id);
                        pending = turn
                            .tool_calls
                            .iter()
                            .cloned()
                            .map(|call| (call.call_id.clone(), call))
                            .collect();
                        initial_input.clear();
                    }
                    Err(error) => {
                        let _ = events.send(RuntimeEvent::Error {
                            message: error.to_string(),
                        });
                    }
                }
            }
            OpenRouterCommand::ToolResult { call_id, output } => {
                if pending.remove(&call_id).is_none() {
                    let _ = events.send(RuntimeEvent::Error {
                        message: format!("OpenAI received an unexpected tool result for {call_id}"),
                    });
                    continue;
                }
                queued_outputs.push(json!({
                    "type": "function_call_output",
                    "call_id": call_id,
                    "output": output,
                }));
                if !pending.is_empty() {
                    continue;
                }
                let input = std::mem::take(&mut queued_outputs);
                match complete_response_with_retry(
                    &client,
                    &base_url,
                    &key,
                    &request,
                    previous_response_id.as_deref(),
                    &input,
                    &events,
                )
                .await
                {
                    Ok(turn) => {
                        previous_response_id = Some(turn.response_id.clone());
                        *provider_session_id.lock().await = Some(turn.response_id);
                        pending = turn
                            .tool_calls
                            .iter()
                            .cloned()
                            .map(|call| (call.call_id.clone(), call))
                            .collect();
                    }
                    Err(error) => {
                        let _ = events.send(RuntimeEvent::Error {
                            message: error.to_string(),
                        });
                    }
                }
            }
            OpenRouterCommand::Stop => break,
        }
    }
    let _ = events.send(RuntimeEvent::Stopped { code: Some(0) });
}

#[derive(Debug, Clone)]
struct ResponseTurn {
    response_id: String,
    tool_calls: Vec<RuntimeToolCall>,
}

async fn complete_response_with_retry(
    client: &Client,
    base_url: &str,
    key: &str,
    request: &StartRequest,
    previous_response_id: Option<&str>,
    input: &[Value],
    events: &tokio::sync::broadcast::Sender<RuntimeEvent>,
) -> Result<ResponseTurn> {
    let mut last_error = None;
    for attempt in 0..MAX_TURN_ATTEMPTS {
        match complete_response_once(
            client,
            base_url,
            key,
            request,
            previous_response_id,
            input,
            events,
        )
        .await
        {
            Ok(turn) => return Ok(turn),
            Err((error, retryable)) => {
                last_error = Some(error);
                if !retryable || attempt + 1 == MAX_TURN_ATTEMPTS {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(
                    250 * 2_u64.saturating_pow(attempt as u32),
                ))
                .await;
            }
        }
    }
    Err(last_error.unwrap_or_else(|| ProviderError::Process("OpenAI request failed".into())))
}

async fn complete_response_once(
    client: &Client,
    base_url: &str,
    key: &str,
    request: &StartRequest,
    previous_response_id: Option<&str>,
    input: &[Value],
    events: &tokio::sync::broadcast::Sender<RuntimeEvent>,
) -> std::result::Result<ResponseTurn, (ProviderError, bool)> {
    let mut body = json!({
        "model": request.model.as_deref().unwrap_or_default(),
        "instructions": request.instructions,
        "input": input,
        "stream": true,
        "store": true,
        "parallel_tool_calls": true,
        "tools": response_tool_definitions(),
        "reasoning": {
            "effort": request
                .reasoning_effort
                .unwrap_or(frank_protocol::ReasoningEffort::Max)
                .as_str()
        }
    });
    if let Some(previous_response_id) = previous_response_id {
        body["previous_response_id"] = Value::String(previous_response_id.to_string());
    }
    let response = client
        .post(format!("{base_url}/responses"))
        .bearer_auth(key)
        .header("X-Title", "Frank")
        .timeout(Duration::from_secs(10 * 60))
        .json(&body)
        .send()
        .await
        .map_err(|error| {
            (
                ProviderError::Process(sanitize_error(&error.to_string(), Some(key))),
                error.is_timeout(),
            )
        })?;
    let status = response.status();
    if !status.is_success() {
        return Err((
            http_error_for(status, response.text().await.unwrap_or_default(), key),
            status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error(),
        ));
    }

    let mut stream = response.bytes_stream();
    let mut buffer = Vec::new();
    let mut state = ResponseParseState::default();
    let stream_result: Result<()> = async {
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| {
                ProviderError::Process(sanitize_error(&error.to_string(), Some(key)))
            })?;
            buffer.extend_from_slice(&chunk);
            while let Some(index) = buffer.iter().position(|byte| *byte == b'\n') {
                if index.saturating_add(1) > MAX_SSE_FRAME_BYTES {
                    return Err(ProviderError::Malformed(
                        "OpenAI Responses SSE frame exceeds the configured size cap".into(),
                    ));
                }
                let line = buffer.drain(..=index).collect::<Vec<_>>();
                let line = std::str::from_utf8(&line)
                    .map_err(|_| ProviderError::Malformed("OpenAI SSE is not UTF-8".into()))?
                    .trim();
                let Some(data) = line.strip_prefix("data:").map(str::trim) else {
                    continue;
                };
                if data == "[DONE]" {
                    continue;
                }
                let frame: Value = serde_json::from_str(data)
                    .map_err(|error| ProviderError::Malformed(error.to_string()))?;
                parse_response_event(&frame, &mut state, Some(key), events)?;
            }
            if buffer.len() > MAX_SSE_FRAME_BYTES {
                return Err(ProviderError::Malformed(
                    "OpenAI Responses SSE frame exceeds the configured size cap".into(),
                ));
            }
        }
        if !buffer.iter().all(|byte| byte.is_ascii_whitespace()) {
            let line = std::str::from_utf8(&buffer)
                .map_err(|_| ProviderError::Malformed("OpenAI SSE is not UTF-8".into()))?
                .trim();
            let Some(data) = line.strip_prefix("data:").map(str::trim) else {
                return Err(ProviderError::Malformed(
                    "OpenAI Responses stream ended with an invalid SSE frame".into(),
                ));
            };
            if data != "[DONE]" {
                let frame: Value = serde_json::from_str(data)
                    .map_err(|error| ProviderError::Malformed(error.to_string()))?;
                parse_response_event(&frame, &mut state, Some(key), events)?;
            }
        }
        Ok(())
    }
    .await;
    if let Err(error) = stream_result {
        return Err((error, false));
    }
    let Some(response_id) = state.response_id else {
        return Err((
            ProviderError::Malformed(
                "OpenAI Responses stream did not include a response id".into(),
            ),
            false,
        ));
    };
    if !state.saw_completed {
        return Err((
            ProviderError::Malformed("OpenAI Responses stream did not complete".into()),
            false,
        ));
    }

    let mut tool_calls = Vec::with_capacity(state.function_calls.len());
    for (item_id, accumulator) in state.function_calls {
        let call_id = if accumulator.call_id.is_empty() {
            item_id
        } else {
            accumulator.call_id
        };
        let input = serde_json::from_str(&accumulator.arguments).map_err(|error| {
            (
                ProviderError::Malformed(format!(
                    "OpenAI function arguments are not JSON: {error}"
                )),
                false,
            )
        })?;
        if accumulator.name.trim().is_empty() {
            return Err((
                ProviderError::Malformed("OpenAI function call has no name".into()),
                false,
            ));
        }
        tool_calls.push(RuntimeToolCall {
            call_id,
            name: accumulator.name,
            input,
        });
    }
    tool_calls.sort_by(|left, right| left.call_id.cmp(&right.call_id));

    let turn_id = uuid::Uuid::new_v4().to_string();
    let _ = events.send(RuntimeEvent::Ready {
        provider_session_id: response_id.clone(),
    });
    let _ = events.send(RuntimeEvent::AssistantMessage {
        turn_id: turn_id.clone(),
        content: state.text,
        tool_calls: tool_calls.clone(),
    });
    for call in &tool_calls {
        let _ = events.send(RuntimeEvent::ToolCall {
            call_id: call.call_id.clone(),
            name: call.name.clone(),
            input: call.input.clone(),
        });
    }
    if let Some(usage) = state.usage {
        let _ = events.send(RuntimeEvent::Usage(usage));
    }
    // Usage must be observed before the terminal turn boundary. The
    // orchestrator finalizes/removes the live session at TurnCompleted, so a
    // later usage frame would otherwise be discarded as a stale event.
    if tool_calls.is_empty() {
        let _ = events.send(RuntimeEvent::TurnCompleted { turn_id });
    }
    Ok(ResponseTurn {
        response_id,
        tool_calls,
    })
}

#[derive(Debug, Default)]
struct FunctionAccumulator {
    call_id: String,
    name: String,
    arguments: String,
}

#[derive(Debug, Default)]
struct ResponseParseState {
    response_id: Option<String>,
    text: String,
    function_calls: HashMap<String, FunctionAccumulator>,
    usage: Option<UsageTelemetry>,
    saw_completed: bool,
}

fn parse_response_event(
    frame: &Value,
    state: &mut ResponseParseState,
    secret: Option<&str>,
    events: &tokio::sync::broadcast::Sender<RuntimeEvent>,
) -> Result<()> {
    if let Some(error) = frame.get("error") {
        return Err(ProviderError::Malformed(sanitize_error(
            &error.to_string(),
            secret,
        )));
    }
    let event_type = frame
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match event_type {
        "response.created" | "response.in_progress" => {
            set_response_id(&mut state.response_id, frame.get("response"));
        }
        "response.output_text.delta" => {
            if let Some(delta) = frame.get("delta").and_then(Value::as_str) {
                if state.text.len().saturating_add(delta.len())
                    > frank_protocol::MAX_MESSAGE_BODY_BYTES
                {
                    return Err(ProviderError::Malformed(
                        "OpenAI assistant content exceeds the configured size cap".into(),
                    ));
                }
                state.text.push_str(delta);
                let _ = events.send(RuntimeEvent::Text {
                    text: delta.to_string(),
                });
            }
        }
        "response.output_text.done" => {
            // Deltas are canonical. A provider may omit them in a short
            // response and send only the done payload, so fill text once.
            if state.text.is_empty()
                && let Some(value) = frame.get("text").and_then(Value::as_str)
            {
                if value.len() > frank_protocol::MAX_MESSAGE_BODY_BYTES {
                    return Err(ProviderError::Malformed(
                        "OpenAI assistant content exceeds the configured size cap".into(),
                    ));
                }
                state.text.push_str(value);
                let _ = events.send(RuntimeEvent::Text {
                    text: value.to_string(),
                });
            }
        }
        "response.output_item.added" | "response.output_item.done" => {
            if let Some(item) = frame.get("item")
                && item.get("type").and_then(Value::as_str) == Some("function_call")
            {
                let item_id = item
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let accumulator = state.function_calls.entry(item_id).or_default();
                if let Some(call_id) = item.get("call_id").and_then(Value::as_str) {
                    accumulator.call_id = call_id.to_string();
                }
                if let Some(name) = item.get("name").and_then(Value::as_str) {
                    accumulator.name = name.to_string();
                }
                if let Some(arguments) = item.get("arguments").and_then(Value::as_str) {
                    accumulator.arguments = arguments.to_string();
                }
            }
        }
        "response.function_call_arguments.delta" => {
            let item_id = frame
                .get("item_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let accumulator = state.function_calls.entry(item_id).or_default();
            if let Some(delta) = frame.get("delta").and_then(Value::as_str) {
                if accumulator.arguments.len().saturating_add(delta.len())
                    > frank_protocol::MAX_COMMAND_BODY_BYTES
                {
                    return Err(ProviderError::Malformed(
                        "OpenAI function arguments exceed the configured size cap".into(),
                    ));
                }
                accumulator.arguments.push_str(delta);
            }
        }
        "response.function_call_arguments.done" => {
            let item_id = frame
                .get("item_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let accumulator = state.function_calls.entry(item_id).or_default();
            if let Some(arguments) = frame.get("arguments").and_then(Value::as_str) {
                if arguments.len() > frank_protocol::MAX_COMMAND_BODY_BYTES {
                    return Err(ProviderError::Malformed(
                        "OpenAI function arguments exceed the configured size cap".into(),
                    ));
                }
                accumulator.arguments = arguments.to_string();
            }
        }
        "response.completed" => {
            set_response_id(&mut state.response_id, frame.get("response"));
            if let Some(response) = frame.get("response") {
                if response.get("status").and_then(Value::as_str) == Some("failed") {
                    return Err(ProviderError::Process(
                        response
                            .get("error")
                            .map(|value| sanitize_error(&value.to_string(), secret))
                            .unwrap_or_else(|| "OpenAI response failed".into()),
                    ));
                }
                if let Some(value) = response.get("usage") {
                    state.usage = Some(response_usage(value));
                }
                if let Some(output) = response.get("output").and_then(Value::as_array) {
                    merge_completed_function_calls(&mut state.function_calls, output);
                    if state.text.is_empty() {
                        for item in output {
                            if item.get("type").and_then(Value::as_str) == Some("message")
                                && let Some(content) = item.get("content").and_then(Value::as_array)
                            {
                                for part in content {
                                    if let Some(value) = part.get("text").and_then(Value::as_str) {
                                        if state.text.len().saturating_add(value.len())
                                            > frank_protocol::MAX_MESSAGE_BODY_BYTES
                                        {
                                            return Err(ProviderError::Malformed(
                                                "OpenAI assistant content exceeds the configured size cap".into(),
                                            ));
                                        }
                                        state.text.push_str(value);
                                        let _ = events.send(RuntimeEvent::Text {
                                            text: value.to_string(),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            state.saw_completed = true;
        }
        "response.failed" | "response.incomplete" => {
            return Err(ProviderError::Process(
                frame
                    .get("response")
                    .and_then(|response| response.get("error"))
                    .map(|value| sanitize_error(&value.to_string(), secret))
                    .unwrap_or_else(|| format!("OpenAI response {event_type}")),
            ));
        }
        _ => {}
    }
    Ok(())
}

fn set_response_id(response_id: &mut Option<String>, response: Option<&Value>) {
    if let Some(id) = response
        .and_then(|value| value.get("id"))
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty())
    {
        *response_id = Some(id.to_string());
    }
}

fn merge_completed_function_calls(
    function_calls: &mut HashMap<String, FunctionAccumulator>,
    output: &[Value],
) {
    for item in output {
        if item.get("type").and_then(Value::as_str) != Some("function_call") {
            continue;
        }
        let item_id = item
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let accumulator = function_calls.entry(item_id).or_default();
        if let Some(value) = item.get("call_id").and_then(Value::as_str) {
            accumulator.call_id = value.to_string();
        }
        if let Some(value) = item.get("name").and_then(Value::as_str) {
            accumulator.name = value.to_string();
        }
        if let Some(value) = item.get("arguments").and_then(Value::as_str) {
            accumulator.arguments = value.to_string();
        }
    }
}

fn response_usage(value: &Value) -> UsageTelemetry {
    UsageTelemetry {
        measured_input_tokens: value.get("input_tokens").and_then(Value::as_u64),
        measured_output_tokens: value.get("output_tokens").and_then(Value::as_u64),
        estimated_input_tokens: None,
        estimated_output_tokens: None,
        cost_micros: None,
        cached_input_tokens: value
            .get("input_tokens_details")
            .and_then(|details| details.get("cached_tokens"))
            .and_then(Value::as_u64),
        reasoning_tokens: value
            .get("output_tokens_details")
            .and_then(|details| details.get("reasoning_tokens"))
            .and_then(Value::as_u64),
    }
}

fn response_message(message: &crate::ProviderMessage) -> Value {
    json!({
        "role": if message.role.trim().is_empty() { "user" } else { message.role.as_str() },
        "content": [{"type": "input_text", "text": message.content}],
    })
}

fn response_input_from_transcript(transcript: &[Value]) -> Vec<Value> {
    transcript
        .iter()
        .filter_map(|message| {
            let role = message.get("role").and_then(Value::as_str)?;
            match role {
                "user" => Some(json!({
                    "role": "user",
                    "content": [{"type": "input_text", "text": message.get("content")?.as_str()?}],
                })),
                "assistant" => Some(json!({
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": message.get("content")?.as_str()?}],
                })),
                "tool" => Some(json!({
                    "type": "function_call_output",
                    "call_id": message.get("tool_call_id")?.as_str()?,
                    "output": message.get("content")?.as_str()?,
                })),
                _ => None,
            }
        })
        .collect()
}

/// Render Frank's canonical catalog into the native Responses function shape.
pub fn response_tool_definitions() -> Vec<Value> {
    frank_tool_catalog::openrouter_definitions()
        .into_iter()
        .filter_map(|definition| {
            let function = definition.get("function")?;
            Some(json!({
                "type": "function",
                "name": function.get("name")?,
                "description": function.get("description")?,
                "parameters": function.get("parameters")?,
                "strict": false,
            }))
        })
        .collect()
}

fn http_error_for(status: StatusCode, body: String, secret: &str) -> ProviderError {
    let detail = body
        .chars()
        .take(512)
        .collect::<String>()
        .replace(['\n', '\r'], " ");
    let detail = detail.replace(secret, "[redacted]");
    if detail.trim().is_empty() {
        ProviderError::Process(format!("OpenAI returned HTTP {status}"))
    } else {
        ProviderError::Process(format!("OpenAI returned HTTP {status}: {detail}"))
    }
}

fn sanitize_error(error: &str, secret: Option<&str>) -> String {
    let value = secret.filter(|secret| !secret.is_empty()).map_or_else(
        || error.to_string(),
        |secret| error.replace(secret, "[redacted]"),
    );
    value
        .replace("OPENAI_API_KEY", "provider credential")
        .chars()
        .take(512)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_tools_are_flat_function_definitions() {
        let definitions = response_tool_definitions();
        assert!(!definitions.is_empty());
        assert!(definitions.iter().all(|definition| {
            definition["type"] == "function"
                && definition.get("function").is_none()
                && definition["strict"] == false
                && definition["parameters"]["additionalProperties"] == false
        }));
    }

    #[test]
    fn transcript_maps_tool_items_to_function_outputs() {
        let input = response_input_from_transcript(&[
            json!({"role":"user","content":"hello"}),
            json!({"role":"tool","tool_call_id":"call-1","content":"done"}),
        ]);
        assert_eq!(input[0]["content"][0]["type"], "input_text");
        assert_eq!(input[1]["type"], "function_call_output");
        assert_eq!(input[1]["call_id"], "call-1");
    }

    #[test]
    fn usage_keeps_cached_and_reasoning_tokens_separate() {
        let usage = response_usage(&json!({
            "input_tokens": 12,
            "output_tokens": 8,
            "input_tokens_details": {"cached_tokens": 3},
            "output_tokens_details": {"reasoning_tokens": 4}
        }));
        assert_eq!(usage.measured_tokens(), Some(20));
        assert_eq!(usage.cached_input_tokens, Some(3));
        assert_eq!(usage.reasoning_tokens, Some(4));
    }

    #[test]
    fn fake_sse_events_reassemble_text_parallel_tools_and_usage() {
        let (events, mut received) = tokio::sync::broadcast::channel(32);
        let mut state = ResponseParseState::default();
        let frames = [
            json!({
                "type": "response.created",
                "response": {"id": "resp_fake_1", "status": "in_progress"}
            }),
            json!({"type": "response.output_text.delta", "delta": "ship "}),
            json!({"type": "response.output_text.delta", "delta": "it"}),
            json!({
                "type": "response.output_item.added",
                "item": {
                    "type": "function_call",
                    "id": "fc_item_a",
                    "call_id": "call_a",
                    "name": "task_get"
                }
            }),
            json!({
                "type": "response.output_item.added",
                "item": {
                    "type": "function_call",
                    "id": "fc_item_b",
                    "call_id": "call_b",
                    "name": "work_item_get"
                }
            }),
            json!({
                "type": "response.function_call_arguments.delta",
                "item_id": "fc_item_b",
                "delta": "{}"
            }),
            json!({
                "type": "response.function_call_arguments.delta",
                "item_id": "fc_item_a",
                "delta": "{\"task_id\":\"t1\"}"
            }),
            json!({
                "type": "response.completed",
                "response": {
                    "id": "resp_fake_1",
                    "status": "completed",
                    "usage": {
                        "input_tokens": 12,
                        "output_tokens": 9,
                        "input_tokens_details": {"cached_tokens": 2},
                        "output_tokens_details": {"reasoning_tokens": 4}
                    },
                    "output": []
                }
            }),
        ];
        for frame in frames {
            parse_response_event(&frame, &mut state, Some("sk-test"), &events).unwrap();
        }

        assert_eq!(state.response_id.as_deref(), Some("resp_fake_1"));
        assert_eq!(state.text, "ship it");
        assert!(state.saw_completed);
        assert_eq!(state.function_calls.len(), 2);
        assert_eq!(state.function_calls["fc_item_a"].call_id, "call_a");
        assert_eq!(state.function_calls["fc_item_b"].arguments, "{}");
        assert_eq!(state.usage.as_ref().unwrap().measured_tokens(), Some(21));
        assert_eq!(state.usage.as_ref().unwrap().reasoning_tokens, Some(4));

        let mut emitted = Vec::new();
        while let Ok(event) = received.try_recv() {
            emitted.push(event);
        }
        assert!(emitted.iter().any(|event| matches!(
            event,
            RuntimeEvent::Text { text } if text == "ship "
        )));
        assert!(emitted.iter().any(|event| matches!(
            event,
            RuntimeEvent::Text { text } if text == "it"
        )));
    }
}
