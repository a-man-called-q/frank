//! Typed tool routing boundary.
//!
//! The concrete handlers remain private to the orchestrator, but all entry
//! points pass through the catalog before a domain handler is selected. This
//! keeps OpenRouter and MCP on the same policy path and gives new tools one
//! stable place to declare their ownership, effect, and transport exposure.

pub(crate) mod browser;
pub(crate) mod coordination;
pub(crate) mod database;
pub(crate) mod google;
pub(crate) mod taskboard;
pub(crate) mod terminal;
pub(crate) mod workspace;

use std::path::Path;
use std::time::Duration;

use frank_protocol::*;
use frank_tool_catalog::{ExecutionOwner, ToolDescriptor, ToolDomain, ToolId};
use serde_json::Value;

use crate::Orchestrator;

pub(crate) const CONNECTOR_TIMEOUT: Duration = Duration::from_secs(45);
pub(crate) const CONNECTOR_RESPONSE_CAP: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
pub(crate) struct ToolRoute {
    pub(crate) id: ToolId,
    pub(crate) descriptor: &'static ToolDescriptor,
}

pub(crate) fn parse_route(name: &str) -> Result<ToolRoute, String> {
    let id: ToolId = name
        .parse()
        .map_err(|error: frank_tool_catalog::CatalogError| error.to_string())?;
    let descriptor = frank_tool_catalog::descriptor(id.as_str())
        .ok_or_else(|| format!("unknown tool: {name}"))?;
    Ok(ToolRoute { id, descriptor })
}

pub(crate) fn owner_is_orchestrator(route: &ToolRoute) -> bool {
    route.descriptor.execution_owner == ExecutionOwner::Orchestrator
}

/// Narrow context shared by every daemon-owned tool handler.  Keeping the
/// context explicit prevents a domain module from reaching through the
/// orchestrator for unrelated state, while still allowing handlers to call
/// the same command and connector boundaries as the legacy dispatcher.
pub(crate) struct ToolExecutionContext<'a> {
    pub(crate) orchestrator: &'a Orchestrator,
    pub(crate) task_id: TaskId,
    pub(crate) agent_id: AgentId,
    pub(crate) workspace_root: &'a Path,
    pub(crate) task: &'a TaskView,
    pub(crate) agent: &'a AgentView,
    pub(crate) snapshot: &'a Snapshot,
}

pub(crate) async fn dispatch(
    route: &ToolRoute,
    context: &ToolExecutionContext<'_>,
    input: &Value,
) -> Result<Option<Value>, String> {
    let name = route.id.as_str();
    // Taskboard/work-item tools are coordination records on the wire, but
    // they have their own command family and handler boundary.
    if name.starts_with("taskboard_") || name.starts_with("work_item_") {
        return taskboard::dispatch(context, name, input).await;
    }
    // Both terminal tools execute through the terminal boundary.  `shell_exec`
    // is retained as the legacy name used by existing sessions, even though
    // the catalog groups it under the Workspace domain for transport policy.
    if matches!(name, "shell_exec" | "terminal_execute") {
        return terminal::dispatch(context, name, input).await;
    }
    match route.descriptor.domain {
        ToolDomain::Coordination => coordination::dispatch(context, name, input).await,
        ToolDomain::Workspace => workspace::dispatch(context, name, input).await,
        ToolDomain::Connector => google::dispatch(context, name, input).await,
        ToolDomain::Browser => browser::dispatch(context, name, input).await,
        ToolDomain::Terminal => terminal::dispatch(context, name, input).await,
        ToolDomain::Database => database::dispatch(context, name, input).await,
    }
}

pub(crate) fn required_string(input: &Value, key: &str) -> Result<String, String> {
    let value = input
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key} is required"))?;
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(format!("{key} is empty or invalid"));
    }
    Ok(value.to_owned())
}

/// Read a user/model-provided text payload. Unlike identifiers, commands, and
/// paths, file contents and replacement blocks are allowed to contain normal
/// newlines and tabs. NUL remains rejected because it cannot be represented
/// safely by the downstream filesystem/process boundaries.
pub(crate) fn required_text(input: &Value, key: &str) -> Result<String, String> {
    let value = input
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key} is required"))?;
    if value.trim().is_empty() || value.contains('\0') {
        return Err(format!("{key} is empty or invalid"));
    }
    Ok(value.to_owned())
}
