//! Task-scoped connector tool endpoint used by the local MCP child.
//!
//! The child never receives a connector credential.  This handler resolves
//! the capability from its short-lived agent token, routes sensitive actions
//! through the durable approval queue, and only then invokes the daemon-owned
//! adapter.

use std::time::Duration;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::{Json, Router, routing::post};
use frank_protocol::{
    ActorKind, ActorRef, ApprovalDecision, ApprovalStatus, Command, CommandEnvelope, CommandResult,
    DeviceRole, ErrorCode, PROTOCOL_VERSION,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::auth::agent_capability_from_headers;
use crate::{ServerState, api_error_response};

#[derive(Debug, Deserialize)]
struct AgentToolRequest {
    name: String,
    #[serde(default)]
    input: Value,
}

pub fn router() -> Router<ServerState> {
    Router::new().route(&crate::api_path("/agent-tools"), post(call))
}

async fn call(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<AgentToolRequest>,
) -> impl IntoResponse {
    let Some((agent_id, task_id)) = agent_capability_from_headers(&state, &headers).await else {
        return api_error_response(
            StatusCode::UNAUTHORIZED,
            frank_protocol::ApiError::new(
                ErrorCode::Unauthorized,
                "agent session capability is invalid",
            ),
        )
        .into_response();
    };
    if request.name.trim().is_empty()
        || request.name.len() > 128
        || request.name.chars().any(char::is_control)
        || serde_json::to_vec(&request.input)
            .is_ok_and(|bytes| bytes.len() > frank_protocol::MAX_COMMAND_BODY_BYTES)
    {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            frank_protocol::ApiError::new(ErrorCode::Validation, "agent tool request is invalid"),
        )
        .into_response();
    }
    let Ok(snapshot) = state.store.snapshot().await else {
        return api_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            frank_protocol::ApiError::new(ErrorCode::Internal, "snapshot unavailable"),
        )
        .into_response();
    };
    let Some(task) = snapshot
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .cloned()
    else {
        return api_error_response(
            StatusCode::FORBIDDEN,
            frank_protocol::ApiError::new(ErrorCode::Forbidden, "task scope is unavailable"),
        )
        .into_response();
    };
    if task.assigned_agent != Some(agent_id) {
        return api_error_response(
            StatusCode::FORBIDDEN,
            frank_protocol::ApiError::new(
                ErrorCode::Forbidden,
                "agent is not assigned to this task",
            ),
        )
        .into_response();
    }
    if !snapshot
        .agents
        .iter()
        .any(|agent| agent.id == agent_id && !agent.archived)
    {
        return api_error_response(
            StatusCode::FORBIDDEN,
            frank_protocol::ApiError::new(ErrorCode::Forbidden, "agent is unavailable"),
        )
        .into_response();
    }

    if sensitive_tool(&request.name) {
        let approval = state
            .orchestrator
            .execute(
                CommandEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    command_id: frank_protocol::CommandId::new(),
                    expected_revision: None,
                    command: Command::RequestApproval(frank_protocol::ApprovalSpec {
                        agent_id,
                        task_id,
                        operation: format!("mcp-tool:{}", request.name)
                            .chars()
                            .take(4_096)
                            .collect(),
                        cwd: task.worktree.clone().unwrap_or_default(),
                        project: format!("mission:{}", task.mission_id),
                        reason: format!(
                            "MCP connector tool {} requires explicit approval",
                            request.name
                        ),
                    }),
                },
                ActorRef {
                    kind: ActorKind::Agent,
                    id: Some(agent_id.to_string()),
                    display_name: None,
                },
                DeviceRole::Operator,
            )
            .await;
        let Some(CommandResult::Created { id }) = approval.result else {
            return Json(json!({"ok": false, "error": approval.error.map(|error| error.message).unwrap_or_else(|| "approval request could not be created".into())})).into_response();
        };
        let Ok(approval_id) = frank_protocol::ApprovalId::parse(&id) else {
            return Json(json!({"ok": false, "error": "approval id is invalid"})).into_response();
        };
        let decision = wait_for_approval(&state, approval_id).await;
        if !matches!(decision, Some(ApprovalDecision::AllowOnce)) {
            return Json(json!({
                "ok": false,
                "error": match decision {
                    Some(ApprovalDecision::DenyOnce) => "Frank denied this operation",
                    None => "Frank approval expired or disappeared",
                    _ => "Frank approval was not granted",
                }
            }))
            .into_response();
        }
    }

    let workspace_root = task
        .worktree
        .clone()
        .or_else(|| {
            (!snapshot.server.worktree_root.is_empty())
                .then(|| snapshot.server.worktree_root.clone())
        })
        .unwrap_or_default();
    let result = state
        .orchestrator
        .execute_openrouter_tool(
            task_id,
            agent_id,
            &workspace_root,
            &request.name,
            &request.input,
        )
        .await;
    Json(result).into_response()
}

fn sensitive_tool(name: &str) -> bool {
    matches!(
        name,
        "email_send"
            | "calendar_create"
            | "calendar_update"
            | "drive_write"
            | "drive_share"
            | "browser_download"
            | "terminal_execute"
            | "database_write"
    )
}

async fn wait_for_approval(
    state: &ServerState,
    approval_id: frank_protocol::ApprovalId,
) -> Option<ApprovalDecision> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(600);
    loop {
        let snapshot = state.store.snapshot().await.ok()?;
        let approval = snapshot
            .approvals
            .iter()
            .find(|approval| approval.id == approval_id)?;
        match approval.status {
            ApprovalStatus::Approved => return Some(ApprovalDecision::AllowOnce),
            ApprovalStatus::Denied => return Some(ApprovalDecision::DenyOnce),
            ApprovalStatus::Expired => return None,
            ApprovalStatus::Pending => {}
        }
        if tokio::time::Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}
