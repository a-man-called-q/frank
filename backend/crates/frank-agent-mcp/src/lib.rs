//! Local, authenticated MCP bridge for provider sessions.
//!
//! The bridge is intentionally scoped to one agent/task capability.  It does
//! not expose SQLite, Git delivery, settings, or another agent's profile.

#![allow(clippy::result_large_err)]

use std::time::{SystemTime, UNIX_EPOCH};

use frank_client::{ClientError, RemoteClient};
use frank_protocol::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use thiserror::Error;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};

#[derive(Debug, Error)]
pub enum McpError {
    #[error("invalid session capability")]
    InvalidCapability,
    #[error("tool is outside the session scope")]
    OutOfScope,
    #[error("unknown MCP tool")]
    UnknownTool,
    #[error("client error: {0}")]
    Client(#[from] ClientError),
    #[error("invalid tool arguments: {0}")]
    Arguments(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("stdio error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, McpError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionCapability {
    pub token: String,
    pub agent_id: AgentId,
    pub task_id: TaskId,
    pub expires_at: u64,
}

impl SessionCapability {
    pub fn from_token(token: impl Into<String>, agent_id: AgentId, task_id: TaskId) -> Self {
        Self {
            token: token.into(),
            agent_id,
            task_id,
            expires_at: now().saturating_add(900),
        }
    }

    pub fn issue(agent_id: AgentId, task_id: TaskId) -> Self {
        let mut bytes = [0_u8; 32];
        let token = if getrandom::fill(&mut bytes).is_ok() {
            hex::encode(bytes)
        } else {
            format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            )
        };
        Self::from_token(token, agent_id, task_id)
    }

    pub fn is_valid(&self, token: &str, agent_id: AgentId, task_id: TaskId) -> bool {
        self.token == token
            && self.agent_id == agent_id
            && self.task_id == task_id
            && self.expires_at >= now()
    }
}

#[derive(Clone)]
pub struct ScopedBridge {
    pub client: RemoteClient,
    pub capability: SessionCapability,
}

impl ScopedBridge {
    pub fn authorize(&self, token: &str) -> Result<()> {
        // The daemon owns the authoritative expiry/revocation clock and may
        // renew a healthy provider session without changing its bearer. Keep
        // this local check to the immutable token binding; every request is
        // still rejected by frankd once the durable capability expires or is
        // revoked. This avoids a stale 15-minute copy in the MCP child
        // short-circuiting a capability that the daemon has renewed.
        if self.capability.token == token {
            Ok(())
        } else {
            Err(McpError::InvalidCapability)
        }
    }

    pub async fn call(&self, token: &str, name: &str, args: Value) -> Result<Value> {
        self.authorize(token)?;
        let descriptor = name
            .parse::<frank_tool_catalog::ToolId>()
            .map_err(|_| McpError::UnknownTool)?;
        if !matches!(
            frank_tool_catalog::descriptor(descriptor.as_str()).map(|item| item.exposure),
            Some(frank_tool_catalog::Exposure::Mcp | frank_tool_catalog::Exposure::Both)
        ) {
            return Err(McpError::UnknownTool);
        }
        match name {
            "task_get" | "work_item_get" => {
                let snapshot = self.client.snapshot().await?;
                let task = snapshot
                    .tasks
                    .into_iter()
                    .find(|task| task.id == self.capability.task_id)
                    .ok_or(McpError::OutOfScope)?;
                Ok(serde_json::to_value(task)?)
            }
            "task_update" => {
                let patch: TaskPatch = serde_json::from_value(args)
                    .map_err(|error| McpError::Arguments(error.to_string()))?;
                let snapshot = snapshot_for_task(&self.client, self.capability.task_id).await?;
                let response = self
                    .client
                    .command(
                        Command::UpdateTask {
                            task_id: self.capability.task_id,
                            patch,
                        },
                        Some(snapshot.revision),
                    )
                    .await?;
                Ok(serde_json::to_value(response)?)
            }
            "task_create_child" => {
                let mut spec: TaskSpec = serde_json::from_value(args)
                    .map_err(|error| McpError::Arguments(error.to_string()))?;
                let snapshot = snapshot_for_task(&self.client, self.capability.task_id).await?;
                let mission_id = snapshot
                    .tasks
                    .iter()
                    .find(|task| task.id == self.capability.task_id)
                    .map(|task| task.mission_id)
                    .ok_or(McpError::OutOfScope)?;
                spec.mission_id = mission_id;
                spec.dependencies.push(self.capability.task_id);
                let response = self
                    .client
                    .command(Command::CreateTask(spec), Some(snapshot.revision))
                    .await?;
                Ok(serde_json::to_value(response)?)
            }
            "work_item_list_board" => {
                let snapshot = snapshot_for_task(&self.client, self.capability.task_id).await?;
                let current = snapshot
                    .tasks
                    .iter()
                    .find(|task| task.id == self.capability.task_id)
                    .cloned()
                    .ok_or(McpError::OutOfScope)?;
                Ok(json!({
                    "tasks": snapshot
                        .tasks
                        .into_iter()
                        .filter(|task| {
                            task.mission_id == current.mission_id
                                && task.taskboard_id == current.taskboard_id
                        })
                        .collect::<Vec<_>>()
                }))
            }
            "work_item_drop" => {
                let taskboard_id: TaskboardId = serde_json::from_value(
                    args.get("taskboard_id")
                        .cloned()
                        .ok_or_else(|| McpError::Arguments("taskboard_id is required".into()))?,
                )
                .map_err(|error| McpError::Arguments(error.to_string()))?;
                let role_id = args
                    .get("role_id")
                    .filter(|value| !value.is_null())
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|error| McpError::Arguments(error.to_string()))?;
                let snapshot = snapshot_for_task(&self.client, self.capability.task_id).await?;
                let response = self
                    .client
                    .command(
                        Command::DropWorkItem {
                            task_id: self.capability.task_id,
                            taskboard_id,
                            role_id,
                        },
                        Some(snapshot.revision),
                    )
                    .await?;
                Ok(serde_json::to_value(response)?)
            }
            "work_item_spawn_children" => {
                let children = args
                    .get("children")
                    .cloned()
                    .ok_or_else(|| McpError::Arguments("children is required".into()))?;
                let mut specs: Vec<WorkItemSpec> = serde_json::from_value(children)
                    .map_err(|error| McpError::Arguments(error.to_string()))?;
                let snapshot = snapshot_for_task(&self.client, self.capability.task_id).await?;
                let current = snapshot
                    .tasks
                    .iter()
                    .find(|task| task.id == self.capability.task_id)
                    .ok_or(McpError::OutOfScope)?;
                for spec in &mut specs {
                    spec.mission_id = Some(current.mission_id);
                    spec.parent_task_id = Some(self.capability.task_id);
                    if spec.taskboard_id == TaskboardId::nil() {
                        spec.taskboard_id = current.taskboard_id.ok_or_else(|| {
                            McpError::Arguments("current work item has no board".into())
                        })?;
                    }
                    if !spec.dependencies.contains(&self.capability.task_id) {
                        spec.dependencies.push(self.capability.task_id);
                    }
                }
                let response = self
                    .client
                    .command(
                        Command::SpawnChildWorkItems {
                            parent_task_id: self.capability.task_id,
                            children: specs,
                        },
                        Some(snapshot.revision),
                    )
                    .await?;
                Ok(serde_json::to_value(response)?)
            }
            "work_item_complete" => {
                let snapshot = snapshot_for_task(&self.client, self.capability.task_id).await?;
                let response = self
                    .client
                    .command(
                        Command::SetTaskStatus {
                            task_id: self.capability.task_id,
                            status: TaskStatus::Done,
                        },
                        Some(snapshot.revision),
                    )
                    .await?;
                Ok(serde_json::to_value(response)?)
            }
            "work_item_request_human_input" => {
                let kind: HumanInputKind = serde_json::from_value(
                    args.get("kind")
                        .cloned()
                        .unwrap_or_else(|| json!("question")),
                )
                .map_err(|error| McpError::Arguments(error.to_string()))?;
                let prompt = args
                    .get("prompt")
                    .and_then(Value::as_str)
                    .ok_or_else(|| McpError::Arguments("prompt is required".into()))?;
                let snapshot = snapshot_for_task(&self.client, self.capability.task_id).await?;
                let response = self
                    .client
                    .command(
                        Command::RequestHumanInput {
                            task_id: self.capability.task_id,
                            kind,
                            prompt: prompt.to_owned(),
                        },
                        Some(snapshot.revision),
                    )
                    .await?;
                Ok(serde_json::to_value(response)?)
            }
            "work_item_rework" => {
                let reason = args
                    .get("reason")
                    .and_then(Value::as_str)
                    .ok_or_else(|| McpError::Arguments("reason is required".into()))?;
                let snapshot = snapshot_for_task(&self.client, self.capability.task_id).await?;
                let response = self
                    .client
                    .command(
                        Command::RequestTaskRework {
                            task_id: self.capability.task_id,
                            reason: reason.to_owned(),
                        },
                        Some(snapshot.revision),
                    )
                    .await?;
                Ok(serde_json::to_value(response)?)
            }
            "work_item_offer_respond" => {
                let offer_id: WorkOfferId = serde_json::from_value(
                    args.get("offer_id")
                        .cloned()
                        .ok_or_else(|| McpError::Arguments("offer_id is required".into()))?,
                )
                .map_err(|error| McpError::Arguments(error.to_string()))?;
                let accept = args
                    .get("accept")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| McpError::Arguments("accept is required".into()))?;
                let snapshot = self.client.snapshot().await?;
                let response = self
                    .client
                    .command(
                        Command::RespondWorkOffer { offer_id, accept },
                        Some(snapshot.revision),
                    )
                    .await?;
                Ok(serde_json::to_value(response)?)
            }
            "taskboard_read" => {
                let snapshot = snapshot_for_task(&self.client, self.capability.task_id).await?;
                let mission_id = snapshot
                    .tasks
                    .iter()
                    .find(|task| task.id == self.capability.task_id)
                    .map(|task| task.mission_id)
                    .ok_or(McpError::OutOfScope)?;
                Ok(json!({
                    "tasks": snapshot
                        .tasks
                        .into_iter()
                        .filter(|task| task.mission_id == mission_id)
                        .collect::<Vec<_>>()
                }))
            }
            "taskboard_create" => {
                let mut spec: TaskSpec = serde_json::from_value(args)
                    .map_err(|error| McpError::Arguments(error.to_string()))?;
                let snapshot = snapshot_for_task(&self.client, self.capability.task_id).await?;
                let mission_id = snapshot
                    .tasks
                    .iter()
                    .find(|task| task.id == self.capability.task_id)
                    .map(|task| task.mission_id)
                    .ok_or(McpError::OutOfScope)?;
                spec.mission_id = mission_id;
                if !spec.dependencies.contains(&self.capability.task_id) {
                    spec.dependencies.push(self.capability.task_id);
                }
                let response = self
                    .client
                    .command(Command::CreateTask(spec), Some(snapshot.revision))
                    .await?;
                Ok(serde_json::to_value(response)?)
            }
            "taskboard_update" => {
                let task_id: TaskId = serde_json::from_value(
                    args.get("task_id")
                        .cloned()
                        .ok_or_else(|| McpError::Arguments("task_id is required".into()))?,
                )
                .map_err(|error| McpError::Arguments(error.to_string()))?;
                let mut patch = args.clone();
                if let Some(object) = patch.as_object_mut() {
                    object.remove("task_id");
                }
                let patch: TaskPatch = serde_json::from_value(patch)
                    .map_err(|error| McpError::Arguments(error.to_string()))?;
                let snapshot = snapshot_for_task(&self.client, self.capability.task_id).await?;
                if !snapshot.tasks.iter().any(|task| {
                    task.id == task_id && task.mission_id == self.task_mission(&snapshot)
                }) {
                    return Err(McpError::OutOfScope);
                }
                let response = self
                    .client
                    .command(
                        Command::UpdateTask { task_id, patch },
                        Some(snapshot.revision),
                    )
                    .await?;
                Ok(serde_json::to_value(response)?)
            }
            "taskboard_assign" => {
                let task_id: TaskId = serde_json::from_value(
                    args.get("task_id")
                        .cloned()
                        .ok_or_else(|| McpError::Arguments("task_id is required".into()))?,
                )
                .map_err(|error| McpError::Arguments(error.to_string()))?;
                let agent_id: AgentId = serde_json::from_value(
                    args.get("agent_id")
                        .cloned()
                        .ok_or_else(|| McpError::Arguments("agent_id is required".into()))?,
                )
                .map_err(|error| McpError::Arguments(error.to_string()))?;
                let snapshot = snapshot_for_task(&self.client, self.capability.task_id).await?;
                let response = self
                    .client
                    .command(
                        Command::AssignTask { task_id, agent_id },
                        Some(snapshot.revision),
                    )
                    .await?;
                Ok(serde_json::to_value(response)?)
            }
            "message_send" => {
                let mut spec: MessageSpec = serde_json::from_value(args)
                    .map_err(|error| McpError::Arguments(error.to_string()))?;
                let snapshot = snapshot_for_task(&self.client, self.capability.task_id).await?;
                let mission_id = snapshot
                    .tasks
                    .iter()
                    .find(|task| task.id == self.capability.task_id)
                    .map(|task| task.mission_id)
                    .ok_or(McpError::OutOfScope)?;
                spec.mission_id = mission_id;
                spec.task_id = Some(self.capability.task_id);
                spec.hop = spec.hop.saturating_add(1);
                let response = self
                    .client
                    .command(Command::SendMessage(spec), Some(snapshot.revision))
                    .await?;
                Ok(serde_json::to_value(response)?)
            }
            "message_ack" => {
                let message_id: MessageId = serde_json::from_value(
                    args.get("message_id")
                        .cloned()
                        .ok_or_else(|| McpError::Arguments("message_id is required".into()))?,
                )
                .map_err(|error| McpError::Arguments(error.to_string()))?;
                let snapshot = self.client.snapshot().await?;
                if !snapshot.messages.iter().any(|message| {
                    message.id == message_id && message.task_id == Some(self.capability.task_id)
                }) {
                    return Err(McpError::OutOfScope);
                }
                let response = self
                    .client
                    .command(Command::AckMessage { message_id }, Some(snapshot.revision))
                    .await?;
                Ok(serde_json::to_value(response)?)
            }
            "artifact_publish" => {
                let mut spec: ArtifactSpec = serde_json::from_value(args)
                    .map_err(|error| McpError::Arguments(error.to_string()))?;
                let snapshot = snapshot_for_task(&self.client, self.capability.task_id).await?;
                let mission_id = snapshot
                    .tasks
                    .iter()
                    .find(|task| task.id == self.capability.task_id)
                    .map(|task| task.mission_id)
                    .ok_or(McpError::OutOfScope)?;
                spec.mission_id = mission_id;
                spec.task_id = Some(self.capability.task_id);
                let response = self
                    .client
                    .command(Command::PublishArtifact(spec), Some(snapshot.revision))
                    .await?;
                Ok(serde_json::to_value(response)?)
            }
            "memory_read" => {
                let path = args
                    .get("path")
                    .and_then(Value::as_str)
                    .unwrap_or("memory.md");
                if path.contains("..") || !path.ends_with("memory.md") {
                    return Err(McpError::OutOfScope);
                }
                let snapshot = snapshot_for_task(&self.client, self.capability.task_id).await?;
                let response = self
                    .client
                    .command(
                        Command::ReadMemory {
                            agent_id: self.capability.agent_id,
                            path: path.to_string(),
                        },
                        Some(snapshot.revision),
                    )
                    .await?;
                Ok(serde_json::to_value(response)?)
            }
            "memory_propose" => {
                let path = args
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or_else(|| McpError::Arguments("path is required".into()))?;
                let content = args
                    .get("content")
                    .and_then(Value::as_str)
                    .ok_or_else(|| McpError::Arguments("content is required".into()))?;
                let snapshot = snapshot_for_task(&self.client, self.capability.task_id).await?;
                let response = self
                    .client
                    .command(
                        Command::ProposeMemory {
                            agent_id: self.capability.agent_id,
                            path: path.to_string(),
                            content: content.to_string(),
                        },
                        Some(snapshot.revision),
                    )
                    .await?;
                Ok(serde_json::to_value(response)?)
            }
            "approval_status" => {
                let snapshot = self.client.snapshot().await?;
                let approvals = snapshot
                    .approvals
                    .into_iter()
                    .filter(|approval| approval.task_id == self.capability.task_id)
                    .collect::<Vec<_>>();
                Ok(serde_json::to_value(approvals)?)
            }
            "review_decide" => {
                let review_item_id: ReviewWorkItemId = serde_json::from_value(
                    args.get("review_item_id")
                        .cloned()
                        .ok_or_else(|| McpError::Arguments("review_item_id is required".into()))?,
                )
                .map_err(|error| McpError::Arguments(error.to_string()))?;
                let decision: ReviewDecision = serde_json::from_value(
                    args.get("decision")
                        .cloned()
                        .ok_or_else(|| McpError::Arguments("decision is required".into()))?,
                )
                .map_err(|error| McpError::Arguments(error.to_string()))?;
                let reason = args
                    .get("reason")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                let snapshot = self.client.snapshot().await?;
                let review = snapshot
                    .review_items
                    .iter()
                    .find(|review| review.id == review_item_id)
                    .ok_or(McpError::OutOfScope)?;
                if review.reviewer_agent != self.capability.agent_id
                    || review.source_task_id != self.capability.task_id
                    || review.status != ReviewWorkItemStatus::Pending
                {
                    return Err(McpError::OutOfScope);
                }
                let response = self
                    .client
                    .command(
                        Command::DecideReview {
                            review_item_id,
                            decision,
                            reason,
                        },
                        Some(snapshot.revision),
                    )
                    .await?;
                Ok(serde_json::to_value(response)?)
            }
            "mcp_auth_tool" => self.permission_prompt(args).await,
            name @ ("email_search" | "email_read" | "email_send" | "calendar_list"
            | "calendar_create" | "calendar_update" | "drive_search" | "drive_read"
            | "drive_write" | "drive_share" | "browser_browse" | "browser_extract"
            | "browser_download" | "terminal_execute" | "database_inspect"
            | "database_read" | "database_write") => {
                // frankd owns the Organization grant, connector credential,
                // and durable approval wait. The child only forwards the
                // task-scoped request with its short-lived capability.
                self.client.agent_tool(name, args).await.map_err(Into::into)
            }
            _ => Err(McpError::UnknownTool),
        }
    }

    fn task_mission(&self, snapshot: &Snapshot) -> MissionId {
        snapshot
            .tasks
            .iter()
            .find(|task| task.id == self.capability.task_id)
            .map(|task| task.mission_id)
            .unwrap_or(MissionId::nil())
    }

    /// Claude's non-interactive stream requires an MCP permission callback.
    /// Turn that callback into Frank's durable approval queue and wait for an
    /// explicit allow/deny decision.  Expiry is fail-closed and never becomes
    /// an implicit approval.
    async fn permission_prompt(&self, args: Value) -> Result<Value> {
        let operation = args
            .get("tool_name")
            .or_else(|| args.get("operation"))
            .and_then(Value::as_str)
            .unwrap_or("provider operation")
            .chars()
            .take(4_096)
            .collect::<String>();
        let cwd = args
            .get("cwd")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .chars()
            .take(4_096)
            .collect::<String>();
        let reason = args
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("provider requested permission")
            .chars()
            .take(MAX_MESSAGE_BODY_BYTES)
            .collect::<String>();
        let snapshot = snapshot_for_task(&self.client, self.capability.task_id).await?;
        let mission_id = snapshot
            .tasks
            .iter()
            .find(|task| task.id == self.capability.task_id)
            .map(|task| task.mission_id)
            .ok_or(McpError::OutOfScope)?;
        let response = self
            .client
            .command(
                Command::RequestApproval(ApprovalSpec {
                    agent_id: self.capability.agent_id,
                    task_id: self.capability.task_id,
                    operation,
                    cwd,
                    project: format!("mission:{mission_id}"),
                    reason,
                }),
                Some(snapshot.revision),
            )
            .await?;
        let Some(CommandResult::Created { id }) = response.result else {
            return Ok(json!({
                "behavior": "deny",
                "message": "Frank could not create an approval request"
            }));
        };
        let approval_id =
            ApprovalId::parse(&id).map_err(|error| McpError::Arguments(error.to_string()))?;
        let deadline = now().saturating_add(600);
        loop {
            let snapshot = self.client.snapshot().await?;
            if let Some(approval) = snapshot
                .approvals
                .iter()
                .find(|approval| approval.id == approval_id)
            {
                match approval.status {
                    ApprovalStatus::Approved => return Ok(json!({"behavior": "allow"})),
                    ApprovalStatus::Denied => {
                        return Ok(
                            json!({"behavior": "deny", "message": "Frank denied this operation"}),
                        );
                    }
                    ApprovalStatus::Expired => {
                        return Ok(
                            json!({"behavior": "deny", "message": "Frank approval expired"}),
                        );
                    }
                    ApprovalStatus::Pending => {}
                }
            } else {
                return Ok(json!({"behavior": "deny", "message": "Frank approval disappeared"}));
            }
            if now() >= deadline {
                return Ok(json!({"behavior": "deny", "message": "Frank approval timed out"}));
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Value,
    pub result: Option<Value>,
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

pub async fn run_stdio(bridge: ScopedBridge) -> Result<()> {
    let mut stdin = BufReader::new(tokio::io::stdin());
    let mut stdout = tokio::io::BufWriter::new(tokio::io::stdout());
    while let Some((line, too_large)) =
        read_bounded_line(&mut stdin, MAX_COMMAND_BODY_BYTES).await?
    {
        if too_large {
            let response = JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: Value::Null,
                result: None,
                error: Some(JsonRpcError {
                    code: -32001,
                    message: "MCP request exceeds the configured size cap".into(),
                }),
            };
            stdout
                .write_all(serde_json::to_string(&response)?.as_bytes())
                .await?;
            stdout.write_all(b"\n").await?;
            stdout.flush().await?;
            continue;
        }
        let request: JsonRpcRequest = match serde_json::from_slice(&line) {
            Ok(request) => request,
            Err(error) => {
                let response = JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id: Value::Null,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32700,
                        message: format!("invalid JSON request: {error}"),
                    }),
                };
                stdout
                    .write_all(serde_json::to_string(&response)?.as_bytes())
                    .await?;
                stdout.write_all(b"\n").await?;
                stdout.flush().await?;
                continue;
            }
        };
        let response = if request.method == "tools/list" {
            JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: request.id,
                result: Some(json!({"tools": tool_descriptors()})),
                error: None,
            }
        } else if request.method == "tools/call" {
            let name = request
                .params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let arguments = request
                .params
                .get("arguments")
                .cloned()
                .unwrap_or(Value::Null);
            match bridge.call(&bridge.capability.token, name, arguments).await {
                Ok(result) => JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id: request.id,
                    result: Some(json!({"content":[{"type":"json","json":result}]})),
                    error: None,
                },
                Err(error) => JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id: request.id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32000,
                        message: error.to_string(),
                    }),
                },
            }
        } else {
            JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: request.id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: "method not found".into(),
                }),
            }
        };
        stdout
            .write_all(serde_json::to_string(&response)?.as_bytes())
            .await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }
    Ok(())
}

/// Read exactly one newline-delimited frame without allowing an attacker to
/// make Tokio's line helper allocate an unbounded buffer. The boolean marks a
/// frame that exceeded `cap`; bytes are discarded through its newline before
/// the next request is parsed.
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

fn tool_descriptors() -> Vec<Value> {
    frank_tool_catalog::mcp_definitions()
}

async fn snapshot_for_task(client: &RemoteClient, task_id: TaskId) -> Result<Snapshot> {
    let snapshot = client.snapshot().await?;
    snapshot
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .ok_or(McpError::OutOfScope)?;
    Ok(snapshot)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_is_scoped_and_expires() {
        let capability = SessionCapability::issue(AgentId::nil(), TaskId::nil());
        assert!(capability.is_valid(&capability.token, AgentId::nil(), TaskId::nil()));
        assert!(!capability.is_valid(&capability.token, AgentId::new(), TaskId::nil()));
    }

    #[test]
    fn tool_list_has_no_git_or_settings_mutation() {
        let tools = tool_descriptors();
        assert!(tools.iter().any(|tool| tool["name"] == "task_get"));
        assert!(!tools.iter().any(|tool| tool["name"] == "git_push"));
    }
}
