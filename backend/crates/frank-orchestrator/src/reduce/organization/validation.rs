//! Organization graph and connector validation helpers.

use std::collections::HashSet;
use std::path::Path;

use frank_protocol::*;
use serde_json::Value;

use crate::validate::valid_agent_text;
use crate::{OrchestratorError, Result};

pub(crate) fn validate_graph(
    snapshot: &Snapshot,
    graph: &OrganizationGraph,
    publish: bool,
) -> Result<()> {
    if graph.nodes.len() > 2_000 || graph.relations.len() > 4_000 || graph.groups.len() > 512 {
        return Err(OrchestratorError::Validation(
            "organization graph exceeds the configured size limit".into(),
        ));
    }
    let mut node_ids = HashSet::new();
    let mut agent_ids = HashSet::new();
    // A graph is v2 when it declares either a v2 node or a v2 routing edge.
    // Checking edges as well prevents a malformed legacy-node graph from
    // smuggling Pickup/Drop/Rework relations through the v1 validator.
    let v2 = graph.nodes.iter().any(|node| {
        matches!(
            node.kind,
            OrganizationNodeKind::Role
                | OrganizationNodeKind::Taskboard
                | OrganizationNodeKind::ChildWorkflow
        )
    }) || graph.relations.iter().any(|relation| {
        matches!(
            relation.kind,
            OrganizationRelationKind::Pickup
                | OrganizationRelationKind::Drop
                | OrganizationRelationKind::Rework
        )
    });
    let mut group_ids = HashSet::new();
    for group in &graph.groups {
        if group.id.trim().is_empty()
            || !group_ids.insert(group.id.clone())
            || !valid_agent_text(&group.label, 256)
            || group.tone.len() > 64
            || group.tone.chars().any(char::is_control)
        {
            return Err(OrchestratorError::Validation(
                "organization group ids must be unique and non-empty".into(),
            ));
        }
        if !group.position.x.is_finite()
            || !group.position.y.is_finite()
            || !group.size.width.is_finite()
            || !group.size.height.is_finite()
            || group.size.width <= 0.0
            || group.size.height <= 0.0
        {
            return Err(OrchestratorError::Validation(
                "organization group geometry is invalid".into(),
            ));
        }
    }
    if !graph.viewport.x.is_finite()
        || !graph.viewport.y.is_finite()
        || !graph.viewport.zoom.is_finite()
        || graph.viewport.zoom <= 0.0
    {
        return Err(OrchestratorError::Validation(
            "organization viewport is invalid".into(),
        ));
    }
    for node in &graph.nodes {
        if node.id.trim().is_empty()
            || !node_ids.insert(node.id.clone())
            || !valid_agent_text(&node.label, 256)
        {
            return Err(OrchestratorError::Validation(
                "organization node ids must be unique and non-empty".into(),
            ));
        }
        if !node.position.x.is_finite() || !node.position.y.is_finite() {
            return Err(OrchestratorError::Validation(
                "organization node coordinates must be finite".into(),
            ));
        }
        if let Some(group_id) = node.group_id.as_deref()
            && !group_ids.contains(group_id)
        {
            return Err(OrchestratorError::Validation(
                "organization node references an unknown group".into(),
            ));
        }
        match node.kind {
            OrganizationNodeKind::Staff => {
                if node.capability.is_some()
                    || node.connector_profile_id.is_some()
                    || node.profile_ref.is_some()
                    || node.role_id.is_some()
                    || node.taskboard_id.is_some()
                    || node.child_workflow_id.is_some()
                {
                    return Err(OrchestratorError::Validation(
                        "staff organization nodes may reference only an agent".into(),
                    ));
                }
                let agent_id = node.agent_id.ok_or_else(|| {
                    OrchestratorError::Validation(
                        "staff organization nodes require an agent".into(),
                    )
                })?;
                if !agent_ids.insert(agent_id) {
                    return Err(OrchestratorError::Validation(
                        "an agent may appear only once in Organization".into(),
                    ));
                }
                if !snapshot
                    .agents
                    .iter()
                    .any(|agent| agent.id == agent_id && !agent.archived)
                {
                    return Err(OrchestratorError::NotFound);
                }
            }
            OrganizationNodeKind::Capability => {
                if node.agent_id.is_some()
                    || node.role_id.is_some()
                    || node.taskboard_id.is_some()
                    || node.child_workflow_id.is_some()
                    || node.profile_ref.is_some()
                {
                    return Err(OrchestratorError::Validation(
                        "capability organization nodes may reference only a capability and connector profile".into(),
                    ));
                }
                let capability = node.capability.ok_or_else(|| {
                    OrchestratorError::Validation(
                        "capability organization nodes require a capability kind".into(),
                    )
                })?;
                let Some(profile_id) = node.connector_profile_id else {
                    if publish {
                        return Err(OrchestratorError::Validation(
                            "published capabilities require a connector profile".into(),
                        ));
                    }
                    continue;
                };
                let profile = snapshot
                    .organization
                    .connector_profiles
                    .iter()
                    .find(|profile| profile.id == profile_id && !profile.archived)
                    .ok_or(OrchestratorError::NotFound)?;
                if publish && profile.health == ConnectorHealth::Unhealthy {
                    return Err(OrchestratorError::Validation(
                        "published Organization cannot reference an unhealthy connector profile"
                            .into(),
                    ));
                }
                if !profile_compatible(capability, profile.kind) {
                    return Err(OrchestratorError::Validation(
                        "connector profile is incompatible with capability".into(),
                    ));
                }
            }
            OrganizationNodeKind::Approval => {
                return Err(OrchestratorError::Validation(
                    "Approval nodes are retired; use a Taskboard question/review".into(),
                ));
            }
            OrganizationNodeKind::Role => {
                if node.agent_id.is_some()
                    || node.capability.is_some()
                    || node.connector_profile_id.is_some()
                    || node.profile_ref.is_some()
                    || node.taskboard_id.is_some()
                    || node.child_workflow_id.is_some()
                {
                    return Err(OrchestratorError::Validation(
                        "role organization nodes may reference a role only".into(),
                    ));
                }
                let role_id = node.role_id.ok_or_else(|| {
                    OrchestratorError::Validation("role organization nodes require a role".into())
                })?;
                if !snapshot
                    .roles
                    .iter()
                    .any(|role| role.id == role_id && !role.archived)
                {
                    return Err(OrchestratorError::NotFound);
                }
            }
            OrganizationNodeKind::Taskboard => {
                if node.agent_id.is_some()
                    || node.capability.is_some()
                    || node.role_id.is_some()
                    || node.child_workflow_id.is_some()
                    || node.connector_profile_id.is_some()
                    || node.profile_ref.is_some()
                {
                    return Err(OrchestratorError::Validation(
                        "taskboard organization nodes may reference only an internal board".into(),
                    ));
                }
                let board_id = node.taskboard_id.ok_or_else(|| {
                    OrchestratorError::Validation(
                        "taskboard organization nodes require a taskboard".into(),
                    )
                })?;
                if !snapshot
                    .taskboards
                    .iter()
                    .any(|board| board.id == board_id && !board.archived)
                {
                    return Err(OrchestratorError::NotFound);
                }
            }
            OrganizationNodeKind::ChildWorkflow => {
                if node.agent_id.is_some()
                    || node.capability.is_some()
                    || node.connector_profile_id.is_some()
                    || node.profile_ref.is_some()
                    || node.role_id.is_some()
                    || node.taskboard_id.is_some()
                {
                    return Err(OrchestratorError::Validation(
                        "child workflow nodes may reference a workflow only".into(),
                    ));
                }
                if node.child_workflow_id.is_none() {
                    return Err(OrchestratorError::Validation(
                        "child workflow nodes require a workflow".into(),
                    ));
                }
            }
        }
    }
    if v2
        && graph
            .nodes
            .iter()
            .any(|node| node.kind == OrganizationNodeKind::Approval)
    {
        return Err(OrchestratorError::Validation(
            "Approval nodes are retired in Organization v2; use a Taskboard question/review".into(),
        ));
    }
    let mut relation_ids = HashSet::new();
    for relation in &graph.relations {
        if relation.id.trim().is_empty() || !relation_ids.insert(relation.id.clone()) {
            return Err(OrchestratorError::Validation(
                "organization relation ids must be unique and non-empty".into(),
            ));
        }
        if relation.contract.input_summary.len() > 4_096
            || relation.contract.expected_output.len() > 4_096
            || relation
                .contract
                .input_summary
                .chars()
                .any(char::is_control)
            || relation
                .contract
                .expected_output
                .chars()
                .any(char::is_control)
        {
            return Err(OrchestratorError::Validation(
                "organization handoff contract is invalid".into(),
            ));
        }
        let source = graph
            .nodes
            .iter()
            .find(|node| node.id == relation.source_node_id)
            .ok_or_else(|| OrchestratorError::Validation("relation source is missing".into()))?;
        let target = graph
            .nodes
            .iter()
            .find(|node| node.id == relation.target_node_id)
            .ok_or_else(|| OrchestratorError::Validation("relation target is missing".into()))?;
        if source.id == target.id {
            return Err(OrchestratorError::Validation(
                "organization relations cannot self-connect".into(),
            ));
        }
        match relation.kind {
            OrganizationRelationKind::Handoff | OrganizationRelationKind::Review
                if v2
                    || source.kind != OrganizationNodeKind::Staff
                    || target.kind != OrganizationNodeKind::Staff =>
            {
                return Err(OrchestratorError::Validation(
                    "handoff and review relations require staff endpoints".into(),
                ));
            }
            OrganizationRelationKind::ToolAccess
                if (!v2 && source.kind != OrganizationNodeKind::Staff)
                    || (v2 && source.kind != OrganizationNodeKind::Role)
                    || target.kind != OrganizationNodeKind::Capability =>
            {
                return Err(OrchestratorError::Validation(
                    "tool access relations require staff to capability endpoints".into(),
                ));
            }
            OrganizationRelationKind::ToolAccess => {
                let capability = target.capability.ok_or_else(|| {
                    OrchestratorError::Validation("tool target has no capability".into())
                })?;
                if relation.permissions.is_empty()
                    || relation
                        .permissions
                        .iter()
                        .any(|permission| !capability.permissions().contains(&permission.as_str()))
                    || relation.permissions.len()
                        != relation.permissions.iter().collect::<HashSet<_>>().len()
                {
                    return Err(OrchestratorError::Validation(
                        "tool access contains an unsupported permission".into(),
                    ));
                }
            }
            OrganizationRelationKind::Drop => {
                if v2
                    && !matches!(
                        (source.kind, target.kind),
                        (
                            OrganizationNodeKind::Role | OrganizationNodeKind::ChildWorkflow,
                            OrganizationNodeKind::Taskboard
                        )
                    )
                {
                    return Err(OrchestratorError::Validation(
                        "Drop routes require a role/child workflow to taskboard endpoint".into(),
                    ));
                }
            }
            OrganizationRelationKind::Pickup => {
                if v2
                    && !matches!(
                        (source.kind, target.kind),
                        (OrganizationNodeKind::Taskboard, OrganizationNodeKind::Role)
                    )
                {
                    return Err(OrchestratorError::Validation(
                        "Pickup routes require a taskboard to role endpoint".into(),
                    ));
                }
            }
            OrganizationRelationKind::Rework => {
                if v2
                    && (!matches!(source.kind, OrganizationNodeKind::Taskboard)
                        || !matches!(target.kind, OrganizationNodeKind::Taskboard))
                {
                    return Err(OrchestratorError::Validation(
                        "Rework routes require taskboard endpoints".into(),
                    ));
                }
                if relation
                    .permissions
                    .iter()
                    .any(|permission| permission.trim().is_empty())
                {
                    return Err(OrchestratorError::Validation(
                        "rework route contains an invalid permission".into(),
                    ));
                }
            }
            _ => {}
        }
    }
    if publish
        && ((v2
            && graph
                .nodes
                .iter()
                .all(|node| node.kind != OrganizationNodeKind::Role))
            || (!v2
                && graph
                    .nodes
                    .iter()
                    .all(|node| node.kind != OrganizationNodeKind::Staff)))
    {
        return Err(OrchestratorError::Validation(
            "published Organization requires at least one worker role".into(),
        ));
    }
    Ok(())
}

pub(crate) fn graph_references_profile(
    graph: &OrganizationGraph,
    profile_id: ConnectorProfileId,
) -> bool {
    graph
        .nodes
        .iter()
        .any(|node| node.connector_profile_id == Some(profile_id))
}

pub(crate) fn valid_name(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && trimmed.len() <= 128 && trimmed.chars().all(|c| !c.is_control())
}

pub(crate) fn validate_profile_spec(spec: &ConnectorProfileSpec) -> Result<()> {
    if !valid_name(&spec.name) {
        return Err(OrchestratorError::Validation(
            "connector profile name is invalid".into(),
        ));
    }
    validate_profile_config_for_kind(&spec.config, spec.kind)
}

pub(crate) fn validate_profile_config(config: &Value) -> Result<()> {
    if config.to_string().len() > 32 * 1024 {
        return Err(OrchestratorError::Validation(
            "connector profile configuration is too large".into(),
        ));
    }
    if contains_secret_key(config) || contains_secret_value(config) {
        return Err(OrchestratorError::Validation(
            "connector secrets must be stored through the credential API".into(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_profile_config_for_kind(config: &Value, kind: ConnectorKind) -> Result<()> {
    validate_profile_config(config)?;
    let object = config.as_object().ok_or_else(|| {
        OrchestratorError::Validation("connector profile config must be an object".into())
    })?;
    match kind {
        ConnectorKind::Postgres => {
            crate::validate_postgres_profile_config(config)
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            if object
                .get("schema")
                .and_then(Value::as_str)
                .is_some_and(|schema| schema.trim().is_empty())
            {
                return Err(OrchestratorError::Validation(
                    "PostgreSQL schema cannot be empty".into(),
                ));
            }
        }
        ConnectorKind::Sqlite => {
            let path = object
                .get("path")
                .or_else(|| object.get("database_path"))
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    OrchestratorError::Validation(
                        "SQLite connector requires a non-secret path".into(),
                    )
                })?;
            if path.trim().is_empty() || path.chars().any(char::is_control) {
                return Err(OrchestratorError::Validation(
                    "SQLite connector path is invalid".into(),
                ));
            }
        }
        ConnectorKind::Browser => {
            let valid = object
                .get("allowed_domains")
                .and_then(Value::as_array)
                .is_some_and(|values| {
                    !values.is_empty()
                        && values.iter().all(|value| {
                            value.as_str().is_some_and(|domain| {
                                let domain = domain.trim();
                                !domain.is_empty()
                                    && domain.len() <= 253
                                    && !domain.chars().any(char::is_control)
                                    && !domain.contains('/')
                                    && !domain.contains(':')
                            })
                        })
                });
            if !valid {
                return Err(OrchestratorError::Validation(
                    "browser allowed_domains must contain at least one valid host name".into(),
                ));
            }
            for key in ["max_response_bytes", "max_download_bytes"] {
                if object
                    .get(key)
                    .is_some_and(|value| value.as_u64().is_none_or(|limit| limit == 0))
                {
                    return Err(OrchestratorError::Validation(
                        "browser payload limits must be positive integers".into(),
                    ));
                }
            }
            for key in ["browser_executable", "executable"] {
                if let Some(value) = object.get(key) {
                    let Some(path) = value.as_str() else {
                        return Err(OrchestratorError::Validation(
                            "browser executable path must be a string".into(),
                        ));
                    };
                    if path.trim().is_empty()
                        || path.chars().any(char::is_control)
                        || !Path::new(path).is_absolute()
                    {
                        return Err(OrchestratorError::Validation(
                            "browser executable path must be an absolute, non-control path".into(),
                        ));
                    }
                }
            }
        }
        ConnectorKind::GoogleWorkspace => {
            for key in ["client_id", "redirect_uri"] {
                if object
                    .get(key)
                    .is_some_and(|value| value.as_str().is_none_or(|text| text.trim().is_empty()))
                {
                    return Err(OrchestratorError::Validation(
                        "Google Workspace OAuth client metadata is invalid".into(),
                    ));
                }
            }
        }
        ConnectorKind::Terminal => {}
    }
    Ok(())
}

pub(crate) fn contains_secret_key(value: &Value) -> bool {
    match value {
        Value::Object(object) => object.iter().any(|(key, value)| {
            let key = key.to_ascii_lowercase();
            key.contains("credential")
                || key.contains("token")
                || key.contains("password")
                || key == "dsn"
                || key.contains("cookie")
                || key.contains("secret")
                || key.contains("api_key")
                || key.contains("private_key")
                || key.contains("access_key")
                || key == "authorization"
                || contains_secret_key(value)
        }),
        Value::Array(values) => values.iter().any(contains_secret_key),
        _ => false,
    }
}

/// Keys are the normal redaction boundary, but accepting a DSN or bearer
/// token under an innocuous key would still leak credentials into the event
/// log. Reject the common wire representations as well; the credential
/// endpoint is the only place that may receive these values.
pub(crate) fn contains_secret_value(value: &Value) -> bool {
    match value {
        Value::String(raw) => {
            let value = raw.trim().to_ascii_lowercase();
            value.starts_with("postgres://")
                || value.starts_with("postgresql://")
                || value.starts_with("mysql://")
                || value.starts_with("mongodb://")
                || value.starts_with("redis://")
                || value.starts_with("bearer ")
                || value.contains("access_token=")
                || value.contains("refresh_token=")
                || value.contains("client_secret=")
                || value.contains("password=")
                || value.contains("-----begin ")
        }
        Value::Object(object) => object.values().any(contains_secret_value),
        Value::Array(values) => values.iter().any(contains_secret_value),
        _ => false,
    }
}

pub(crate) fn sanitize_profile_config(config: Value) -> Value {
    config
}

pub(crate) fn profile_compatible(
    capability: OrganizationCapabilityKind,
    kind: ConnectorKind,
) -> bool {
    matches!(
        (capability, kind),
        (
            OrganizationCapabilityKind::Email,
            ConnectorKind::GoogleWorkspace
        ) | (
            OrganizationCapabilityKind::Calendar,
            ConnectorKind::GoogleWorkspace
        ) | (
            OrganizationCapabilityKind::Drive,
            ConnectorKind::GoogleWorkspace
        ) | (OrganizationCapabilityKind::Browser, ConnectorKind::Browser)
            | (
                OrganizationCapabilityKind::Terminal,
                ConnectorKind::Terminal
            )
            | (
                OrganizationCapabilityKind::Database,
                ConnectorKind::Postgres
            )
            | (OrganizationCapabilityKind::Database, ConnectorKind::Sqlite)
    )
}
