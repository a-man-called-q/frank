//! Single source of truth for Frank tool identity, schemas, and policy
//! metadata. Transport adapters render this catalog into their wire shape;
//! execution remains owned by the orchestrator or the scoped MCP bridge.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ToolId(String);

impl ToolId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ToolId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

impl FromStr for ToolId {
    type Err = CatalogError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.trim().is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
            return Err(CatalogError::UnknownTool(value.to_string()));
        }
        if descriptor(value).is_none() {
            return Err(CatalogError::UnknownTool(value.to_string()));
        }
        Ok(Self(value.to_string()))
    }
}

impl fmt::Display for ToolId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Exposure {
    OpenRouter,
    Mcp,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionOwner {
    Orchestrator,
    McpBridge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolDomain {
    Coordination,
    Workspace,
    Connector,
    Browser,
    Terminal,
    Database,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolEffect {
    Read,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDescriptor {
    pub id: &'static str,
    pub description: &'static str,
    pub exposure: Exposure,
    pub execution_owner: ExecutionOwner,
    pub domain: ToolDomain,
    pub effect: ToolEffect,
    pub requires_network: bool,
    pub requires_approval: bool,
    pub capability: &'static str,
    pub permission: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogError {
    UnknownTool(String),
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTool(name) => write!(f, "unknown tool: {name}"),
        }
    }
}

impl std::error::Error for CatalogError {}

const COORD: &[ToolDescriptor] = &[
    d(
        "task_get",
        "Read the current task",
        ToolDomain::Coordination,
        ToolEffect::Read,
        false,
        false,
    ),
    d(
        "task_update",
        "Update the current task",
        ToolDomain::Coordination,
        ToolEffect::Write,
        false,
        true,
    ),
    d(
        "task_create_child",
        "Create a child task",
        ToolDomain::Coordination,
        ToolEffect::Write,
        false,
        true,
    ),
    d(
        "work_item_get",
        "Read the current shared work item",
        ToolDomain::Coordination,
        ToolEffect::Read,
        false,
        false,
    ),
    d(
        "work_item_list_board",
        "List work items on the current shared board",
        ToolDomain::Coordination,
        ToolEffect::Read,
        false,
        false,
    ),
    d(
        "work_item_drop",
        "Drop the current work item on another board",
        ToolDomain::Coordination,
        ToolEffect::Write,
        false,
        true,
    ),
    d(
        "work_item_spawn_children",
        "Split the current work item into child cards",
        ToolDomain::Coordination,
        ToolEffect::Write,
        false,
        true,
    ),
    d(
        "work_item_complete",
        "Mark the current work item complete",
        ToolDomain::Coordination,
        ToolEffect::Write,
        false,
        true,
    ),
    d(
        "work_item_request_human_input",
        "Pause the current work item for a human answer",
        ToolDomain::Coordination,
        ToolEffect::Write,
        false,
        true,
    ),
    d(
        "work_item_rework",
        "Request bounded rework for the current card",
        ToolDomain::Coordination,
        ToolEffect::Write,
        false,
        true,
    ),
    d(
        "work_item_offer_respond",
        "Accept or decline a pull-mode work offer",
        ToolDomain::Coordination,
        ToolEffect::Write,
        false,
        true,
    ),
    d(
        "message_send",
        "Send a brokered message",
        ToolDomain::Coordination,
        ToolEffect::Write,
        false,
        true,
    ),
    d(
        "message_ack",
        "Acknowledge a message",
        ToolDomain::Coordination,
        ToolEffect::Write,
        false,
        false,
    ),
    d(
        "artifact_publish",
        "Publish an artifact",
        ToolDomain::Coordination,
        ToolEffect::Write,
        false,
        true,
    ),
    d(
        "memory_read",
        "Read scoped memory",
        ToolDomain::Coordination,
        ToolEffect::Read,
        false,
        false,
    ),
    d(
        "memory_propose",
        "Propose a memory update",
        ToolDomain::Coordination,
        ToolEffect::Write,
        false,
        true,
    ),
    d(
        "approval_status",
        "Read approvals for this task",
        ToolDomain::Coordination,
        ToolEffect::Read,
        false,
        false,
    ),
    d(
        "review_decide",
        "Approve or reject a durable review work item",
        ToolDomain::Coordination,
        ToolEffect::Write,
        false,
        true,
    ),
    d(
        "taskboard_read",
        "Read Frank taskboard data",
        ToolDomain::Coordination,
        ToolEffect::Read,
        false,
        false,
    ),
    d(
        "taskboard_create",
        "Create a Frank taskboard item",
        ToolDomain::Coordination,
        ToolEffect::Write,
        false,
        true,
    ),
    d(
        "taskboard_update",
        "Update a Frank taskboard item",
        ToolDomain::Coordination,
        ToolEffect::Write,
        false,
        true,
    ),
    d(
        "taskboard_assign",
        "Assign a Frank taskboard item",
        ToolDomain::Coordination,
        ToolEffect::Write,
        false,
        true,
    ),
];

const WORKSPACE: &[ToolDescriptor] = &[
    d(
        "workspace_list",
        "List files in the current task worktree",
        ToolDomain::Workspace,
        ToolEffect::Read,
        false,
        false,
    ),
    d(
        "workspace_read",
        "Read a bounded file from the current task worktree",
        ToolDomain::Workspace,
        ToolEffect::Read,
        false,
        false,
    ),
    d(
        "workspace_search",
        "Search text in the current task worktree",
        ToolDomain::Workspace,
        ToolEffect::Read,
        false,
        false,
    ),
    d(
        "workspace_apply_patch",
        "Apply a patch inside the current task worktree",
        ToolDomain::Workspace,
        ToolEffect::Write,
        false,
        true,
    ),
    d(
        "shell_exec",
        "Run a bounded shell command after Frank approval",
        ToolDomain::Workspace,
        ToolEffect::Write,
        false,
        true,
    ),
];

const CONNECTOR: &[ToolDescriptor] = &[
    d(
        "email_search",
        "Search the task-scoped Gmail profile",
        ToolDomain::Connector,
        ToolEffect::Read,
        true,
        false,
    ),
    d(
        "email_read",
        "Read a message from the task-scoped Gmail profile",
        ToolDomain::Connector,
        ToolEffect::Read,
        true,
        false,
    ),
    d(
        "email_send",
        "Send an email after explicit Frank approval",
        ToolDomain::Connector,
        ToolEffect::Write,
        true,
        true,
    ),
    d(
        "calendar_list",
        "List events from the task-scoped calendar",
        ToolDomain::Connector,
        ToolEffect::Read,
        true,
        false,
    ),
    d(
        "calendar_create",
        "Create a calendar event after approval",
        ToolDomain::Connector,
        ToolEffect::Write,
        true,
        true,
    ),
    d(
        "calendar_update",
        "Update a calendar event after approval",
        ToolDomain::Connector,
        ToolEffect::Write,
        true,
        true,
    ),
    d(
        "drive_search",
        "Search the task-scoped Drive profile",
        ToolDomain::Connector,
        ToolEffect::Read,
        true,
        false,
    ),
    d(
        "drive_read",
        "Read a Drive file",
        ToolDomain::Connector,
        ToolEffect::Read,
        true,
        false,
    ),
    d(
        "drive_write",
        "Write a Drive file after approval",
        ToolDomain::Connector,
        ToolEffect::Write,
        true,
        true,
    ),
    d(
        "drive_share",
        "Share a Drive file after approval",
        ToolDomain::Connector,
        ToolEffect::Write,
        true,
        true,
    ),
];

const BROWSER: &[ToolDescriptor] = &[
    d(
        "browser_browse",
        "Browse an allowlisted domain",
        ToolDomain::Browser,
        ToolEffect::Read,
        true,
        false,
    ),
    d(
        "browser_extract",
        "Extract content from an allowlisted page",
        ToolDomain::Browser,
        ToolEffect::Read,
        true,
        false,
    ),
    d(
        "browser_download",
        "Download an artifact after explicit approval",
        ToolDomain::Browser,
        ToolEffect::Write,
        true,
        true,
    ),
];

const TERMINAL: &[ToolDescriptor] = &[d(
    "terminal_execute",
    "Execute in the task worktree after approval",
    ToolDomain::Terminal,
    ToolEffect::Write,
    false,
    true,
)];
const DATABASE: &[ToolDescriptor] = &[
    d(
        "database_inspect",
        "Inspect a read-only database schema",
        ToolDomain::Database,
        ToolEffect::Read,
        false,
        false,
    ),
    d(
        "database_read",
        "Read from a task database",
        ToolDomain::Database,
        ToolEffect::Read,
        false,
        false,
    ),
    d(
        "database_write",
        "Write to a task database",
        ToolDomain::Database,
        ToolEffect::Write,
        false,
        true,
    ),
];
const MCP_AUTH: ToolDescriptor = ToolDescriptor {
    id: "mcp_auth_tool",
    description: "Request and wait for a Frank permission decision",
    exposure: Exposure::Mcp,
    execution_owner: ExecutionOwner::McpBridge,
    domain: ToolDomain::Coordination,
    effect: ToolEffect::Write,
    requires_network: false,
    requires_approval: true,
    capability: "task_tools",
    permission: "agent",
};

const fn d(
    id: &'static str,
    description: &'static str,
    domain: ToolDomain,
    effect: ToolEffect,
    network: bool,
    approval: bool,
) -> ToolDescriptor {
    ToolDescriptor {
        id,
        description,
        exposure: Exposure::Both,
        execution_owner: ExecutionOwner::Orchestrator,
        domain,
        effect,
        requires_network: network,
        requires_approval: approval,
        capability: "task_tools",
        permission: "agent",
    }
}

pub fn all() -> impl Iterator<Item = &'static ToolDescriptor> {
    COORD
        .iter()
        .chain(WORKSPACE)
        .chain(CONNECTOR)
        .chain(BROWSER)
        .chain(TERMINAL)
        .chain(DATABASE)
        .chain(std::iter::once(&MCP_AUTH))
}

pub fn descriptor(name: &str) -> Option<&'static ToolDescriptor> {
    all().find(|item| item.id == name)
}

/// Organization connector grant required by a tool. Keeping this mapping in
/// the catalog prevents transport and execution layers from growing separate
/// string lists for the same permission boundary.
pub fn organization_permission(name: &str) -> Option<(&'static str, &'static str)> {
    Some(match name {
        "email_search" | "email_read" => ("email", "read"),
        "email_send" => ("email", "send"),
        "calendar_list" => ("calendar", "read"),
        "calendar_create" => ("calendar", "create"),
        "calendar_update" => ("calendar", "update"),
        "drive_search" | "drive_read" => ("drive", "read"),
        "drive_write" => ("drive", "write"),
        "drive_share" => ("drive", "share"),
        "taskboard_read" => ("taskboard", "read"),
        "taskboard_create" => ("taskboard", "create"),
        "taskboard_update" => ("taskboard", "update"),
        "taskboard_assign" => ("taskboard", "assign"),
        "browser_browse" | "browser_extract" => ("browser", "browse"),
        "browser_download" => ("browser", "download"),
        "terminal_execute" | "shell_exec" => ("terminal", "execute"),
        "database_inspect" => ("database", "inspect"),
        "database_read" => ("database", "read"),
        "database_write" => ("database", "write"),
        _ => return None,
    })
}

/// The JSON Schema sent to transports. Every descriptor gets a real object
/// schema, with the fields used by the corresponding typed handler.
pub fn input_schema(name: &str) -> Value {
    let properties = match name {
        "shell_exec" | "terminal_execute" => json!({"command": {"type": "string"}}),
        "workspace_read" => json!({"path": {"type": "string"}}),
        "memory_read" | "email_read" | "drive_read" | "calendar_update" => {
            json!({"id": {"type": "string"}})
        }
        "workspace_search" | "email_search" | "drive_search" | "browser_browse"
        | "browser_extract" => json!({"query": {"type": "string"}}),
        "workspace_apply_patch" => {
            json!({"path": {"type": "string"}, "content": {"type": "string"}, "old": {"type": "string"}, "new": {"type": "string"}})
        }
        "browser_download" => json!({"url": {"type": "string"}}),
        "database_inspect" | "database_read" | "database_write" => {
            json!({"statement": {"type": "string"}})
        }
        "message_ack" => json!({"message_id": {"type": "string"}}),
        "work_item_rework" => json!({"reason": {"type": "string"}}),
        "work_item_offer_respond" => {
            json!({"offer_id": {"type": "string"}, "accept": {"type": "boolean"}})
        }
        "memory_propose" => json!({"path": {"type": "string"}, "content": {"type": "string"}}),
        "task_create_child" => {
            json!({"title": {"type": "string"}, "objective": {"type": "string"}, "dependencies": {"type": "array", "items": {"type": "string"}}})
        }
        "task_update" => {
            json!({"status": {"type": "string"}, "title": {"type": "string"}, "objective": {"type": "string"}})
        }
        "email_send" => {
            json!({"to": {"type": "array", "items": {"type": "string"}}, "subject": {"type": "string"}, "body": {"type": "string"}})
        }
        "calendar_create" => {
            json!({"title": {"type": "string"}, "start": {"type": "string"}, "end": {"type": "string"}})
        }
        "drive_write" => json!({"id": {"type": "string"}, "content": {"type": "string"}}),
        "drive_share" => {
            json!({"id": {"type": "string"}, "principal": {"type": "string"}, "role": {"type": "string"}})
        }
        "artifact_publish" => {
            json!({"name": {"type": "string"}, "mime_type": {"type": "string"}, "bytes": {"type": "array", "items": {"type": "integer"}}})
        }
        _ => json!({}),
    };
    json!({"type": "object", "properties": properties, "additionalProperties": false})
}

pub fn openrouter_definitions() -> Vec<Value> {
    all()
        .filter(|item| matches!(item.exposure, Exposure::OpenRouter | Exposure::Both))
        .map(|item| {
            json!({
                "type": "function",
                "function": {
                    "name": item.id,
                    "description": item.description,
                    "parameters": input_schema(item.id)
                }
            })
        })
        .collect()
}

pub fn mcp_definitions() -> Vec<Value> {
    all().filter(|item| matches!(item.exposure, Exposure::Mcp | Exposure::Both)).map(|item| json!({"name": item.id, "description": item.description, "inputSchema": input_schema(item.id)})).collect()
}

impl fmt::Display for ExecutionOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_are_unique_and_schemas_are_objects() {
        let mut names = std::collections::HashSet::new();
        for item in all() {
            assert!(names.insert(item.id));
            assert_eq!(input_schema(item.id)["type"], "object");
            assert!(!item.execution_owner.to_string().is_empty());
        }
    }

    #[test]
    fn transport_exposure_is_enforced() {
        let openrouter = openrouter_definitions();
        let mcp = mcp_definitions();
        assert!(
            openrouter
                .iter()
                .any(|tool| tool["function"]["name"] == "task_get")
        );
        assert!(
            !openrouter
                .iter()
                .any(|tool| tool["function"]["name"] == "mcp_auth_tool")
        );
        assert!(mcp.iter().any(|tool| tool["name"] == "task_get"));
        assert!(mcp.iter().any(|tool| tool["name"] == "mcp_auth_tool"));
    }
}
