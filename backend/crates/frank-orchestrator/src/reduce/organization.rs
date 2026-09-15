//! Server-owned Organization and connector profile reductions.
//!
//! The desktop graph is only a draft until the owner publishes it.  Runtime
//! code reads `snapshot.organization.published`, which gives configuration
//! edits the same event-sourced and idempotent boundary as every other
//! daemon mutation.

use frank_protocol::*;

use crate::{Orchestrator, Result};

mod connector;
mod graph;
mod validation;

impl Orchestrator {
    pub(crate) async fn reduce_organization(
        &self,
        snapshot: Snapshot,
        command: Command,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        match command {
            Command::SaveOrganizationDraft {
                graph,
                expected_draft_revision,
            } => {
                self.reduce_save_organization_draft(snapshot, graph, expected_draft_revision, actor)
                    .await
            }
            Command::PublishOrganization {
                expected_published_revision,
            } => {
                self.reduce_publish_organization(snapshot, expected_published_revision, actor)
                    .await
            }
            Command::CreateConnectorProfile(spec) => {
                self.reduce_create_connector_profile(snapshot, spec, actor)
                    .await
            }
            Command::UpdateConnectorProfile { profile_id, patch } => {
                self.reduce_update_connector_profile(snapshot, profile_id, patch, actor)
                    .await
            }
            Command::ArchiveConnectorProfile { profile_id } => {
                self.reduce_archive_connector_profile(snapshot, profile_id, actor)
                    .await
            }
            _ => super::misrouted(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::validation::{
        validate_graph, validate_profile_config, validate_profile_config_for_kind,
    };
    use super::*;
    use crate::validate::organization_tool_denial;
    use serde_json::Value;

    fn agent(id: AgentId) -> AgentView {
        AgentView {
            id,
            role_id: None,
            role_revision: 0,
            display_name: "Worker".into(),
            model: None,
            effective_model: None,
            model_source: ModelSource::Role,
            model_override: None,
            pending_model_override: None,
            pending_model_change: false,
            instructions: String::new(),
            policy: AgentPolicy::default(),
            budget: Budget::unlimited(),
            status: AgentStatus::Idle,
            provider_session_id: None,
            last_claimed_at: None,
            archived: false,
        }
    }

    #[test]
    fn publish_requires_profile_for_every_capability() {
        let agent_id = AgentId::new();
        let mut snapshot = Snapshot::empty(ServerId::new());
        snapshot.agents.push(agent(agent_id));
        let graph = OrganizationGraph {
            id: snapshot.organization.draft.id,
            draft_revision: 0,
            published_revision: 0,
            nodes: vec![
                OrganizationNode {
                    id: "staff".into(),
                    kind: OrganizationNodeKind::Staff,
                    label: "Worker".into(),
                    position: OrganizationPoint::default(),
                    group_id: None,
                    agent_id: Some(agent_id),
                    capability: None,
                    connector_profile_id: None,
                    profile_ref: None,
                    configured: true,
                    approval_required: false,
                    role_id: None,
                    taskboard_id: None,
                    child_workflow_id: None,
                    input_port: None,
                    output_port: None,
                    rework_limit: None,
                },
                OrganizationNode {
                    id: "mail".into(),
                    kind: OrganizationNodeKind::Capability,
                    label: "Email".into(),
                    position: OrganizationPoint::default(),
                    group_id: None,
                    agent_id: None,
                    capability: Some(OrganizationCapabilityKind::Email),
                    connector_profile_id: None,
                    profile_ref: None,
                    configured: false,
                    approval_required: true,
                    role_id: None,
                    taskboard_id: None,
                    child_workflow_id: None,
                    input_port: None,
                    output_port: None,
                    rework_limit: None,
                },
            ],
            relations: Vec::new(),
            groups: Vec::new(),
            viewport: OrganizationViewport::default(),
        };
        assert!(validate_graph(&snapshot, &graph, false).is_ok());
        assert!(validate_graph(&snapshot, &graph, true).is_err());
    }

    #[test]
    fn connector_config_rejects_nested_secrets() {
        assert!(
            validate_profile_config(&serde_json::json!({
                "oauth": {"refresh_token": "secret"}
            }))
            .is_err()
        );
        assert!(validate_profile_config(&serde_json::json!({"api_key": "secret"})).is_err());
        assert!(
            validate_profile_config(&serde_json::json!({
                "endpoint": "postgres://user:password@db.example/app"
            }))
            .is_err()
        );
        assert!(
            validate_profile_config(&serde_json::json!({
                "header": "Bearer very-secret-token"
            }))
            .is_err()
        );
        assert!(
            validate_profile_config(&serde_json::json!({
                "allowed_domains": ["example.com"],
                "schema": "public"
            }))
            .is_ok()
        );
        assert!(
            validate_profile_config_for_kind(
                &serde_json::json!({"path": "/projects/app.sqlite3"}),
                ConnectorKind::Sqlite,
            )
            .is_ok()
        );
        assert!(
            validate_profile_config_for_kind(&serde_json::json!({}), ConnectorKind::Sqlite,)
                .is_err()
        );
        assert!(
            validate_profile_config_for_kind(
                &serde_json::json!({"allowed_domains": ["example.com"]}),
                ConnectorKind::Browser,
            )
            .is_ok()
        );
        assert!(
            validate_profile_config_for_kind(&serde_json::json!({}), ConnectorKind::Browser,)
                .is_err()
        );
        assert!(
            validate_profile_config_for_kind(
                &serde_json::json!({"allowed_domains": ["https://example.com"]}),
                ConnectorKind::Browser,
            )
            .is_err()
        );
    }

    #[test]
    fn tool_access_requires_a_published_grant() {
        let agent_id = AgentId::new();
        let profile_id = ConnectorProfileId::new();
        let staff_id = "staff".to_string();
        let capability_id = "terminal".to_string();
        let mut snapshot = Snapshot::empty(ServerId::new());
        snapshot.agents.push(agent(agent_id));
        snapshot
            .organization
            .connector_profiles
            .push(ConnectorProfileView {
                id: profile_id,
                name: "Terminal".into(),
                kind: ConnectorKind::Terminal,
                config: Value::Object(Default::default()),
                health: ConnectorHealth::Healthy,
                configured: true,
                diagnostic: None,
                checked_at: None,
                archived: false,
            });
        snapshot.organization.published = Some(OrganizationGraph {
            id: snapshot.organization.draft.id,
            draft_revision: 1,
            published_revision: 1,
            nodes: vec![
                OrganizationNode {
                    id: staff_id.clone(),
                    kind: OrganizationNodeKind::Staff,
                    label: "Worker".into(),
                    position: OrganizationPoint::default(),
                    group_id: None,
                    agent_id: Some(agent_id),
                    capability: None,
                    connector_profile_id: None,
                    profile_ref: None,
                    configured: true,
                    approval_required: false,
                    role_id: None,
                    taskboard_id: None,
                    child_workflow_id: None,
                    input_port: None,
                    output_port: None,
                    rework_limit: None,
                },
                OrganizationNode {
                    id: capability_id.clone(),
                    kind: OrganizationNodeKind::Capability,
                    label: "Terminal".into(),
                    position: OrganizationPoint::default(),
                    group_id: None,
                    agent_id: None,
                    capability: Some(OrganizationCapabilityKind::Terminal),
                    connector_profile_id: Some(profile_id),
                    profile_ref: None,
                    configured: true,
                    approval_required: true,
                    role_id: None,
                    taskboard_id: None,
                    child_workflow_id: None,
                    input_port: None,
                    output_port: None,
                    rework_limit: None,
                },
            ],
            relations: vec![OrganizationRelation {
                id: "grant".into(),
                kind: OrganizationRelationKind::ToolAccess,
                source_node_id: staff_id,
                target_node_id: capability_id,
                contract: OrganizationHandoffContract::default(),
                permissions: vec!["execute".into()],
            }],
            groups: Vec::new(),
            viewport: OrganizationViewport::default(),
        });
        assert!(organization_tool_denial(&snapshot, agent_id, "shell_exec").is_none());
        assert!(organization_tool_denial(&snapshot, agent_id, "database_read").is_some());
    }

    #[test]
    fn taskboard_publishes_without_a_connector_and_rejects_foreign_fields() {
        let role_id = RoleId::new();
        let taskboard_id = TaskboardId::new();
        let mut snapshot = Snapshot::empty(ServerId::new());
        snapshot.roles.push(RoleView {
            id: role_id,
            name: "Researcher".into(),
            model: None,
            instructions: String::new(),
            policy: AgentPolicy::default(),
            budget: Budget::unlimited(),
            revision: 1,
            archived: false,
        });
        let now = timestamp_now();
        snapshot.taskboards.push(TaskboardView {
            id: taskboard_id,
            name: "Inbox".into(),
            project_id: None,
            workflow_id: None,
            dispatch_mode: TaskboardDispatchMode::Pull,
            default_role_id: Some(role_id),
            archived: false,
            created_at: now.clone(),
            updated_at: now,
        });

        let graph = OrganizationGraph {
            id: snapshot.organization.draft.id,
            draft_revision: 0,
            published_revision: 0,
            nodes: vec![
                OrganizationNode {
                    id: "role".into(),
                    kind: OrganizationNodeKind::Role,
                    label: "Researcher".into(),
                    position: OrganizationPoint::default(),
                    group_id: None,
                    agent_id: None,
                    capability: None,
                    connector_profile_id: None,
                    profile_ref: None,
                    configured: false,
                    approval_required: false,
                    role_id: Some(role_id),
                    taskboard_id: None,
                    child_workflow_id: None,
                    input_port: None,
                    output_port: None,
                    rework_limit: None,
                },
                OrganizationNode {
                    id: "inbox".into(),
                    kind: OrganizationNodeKind::Taskboard,
                    label: "Inbox".into(),
                    position: OrganizationPoint::default(),
                    group_id: None,
                    agent_id: None,
                    capability: None,
                    connector_profile_id: None,
                    profile_ref: None,
                    configured: false,
                    approval_required: false,
                    role_id: None,
                    taskboard_id: Some(taskboard_id),
                    child_workflow_id: None,
                    input_port: None,
                    output_port: None,
                    rework_limit: None,
                },
            ],
            relations: vec![OrganizationRelation {
                id: "drop".into(),
                kind: OrganizationRelationKind::Drop,
                source_node_id: "role".into(),
                target_node_id: "inbox".into(),
                contract: OrganizationHandoffContract::default(),
                permissions: Vec::new(),
            }],
            groups: Vec::new(),
            viewport: OrganizationViewport::default(),
        };
        assert!(validate_graph(&snapshot, &graph, true).is_ok());

        let mut malformed = graph;
        malformed.nodes[1].connector_profile_id = Some(ConnectorProfileId::new());
        assert!(validate_graph(&snapshot, &malformed, false).is_err());
    }
}
