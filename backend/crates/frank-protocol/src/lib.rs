//! Frank 2.0 wire contract.
//!
//! This crate intentionally contains data transfer objects only.  It has no
//! filesystem, provider, GUI, or database dependency, which makes the same
//! contract safe to use from `frankd`, the native client, the CLI, and the
//! scoped agent MCP bridge.

use std::fmt;

mod command;
mod error;
mod event;
mod ids;
mod mission_proposals;
mod mission_task;
mod operations_artifacts_terminal;
mod settings_capabilities;
mod snapshot_scope;
mod team;
mod usage;
mod validation;
mod version;
mod workflow;

pub use command::*;
pub use error::*;
pub use event::*;
pub use ids::*;
pub use mission_proposals::*;
pub use mission_task::*;
pub use operations_artifacts_terminal::*;
pub use settings_capabilities::*;
pub use snapshot_scope::*;
pub use team::*;
pub use usage::*;
pub use validation::*;
pub use version::*;
pub use workflow::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_golden_serialization_is_stable() {
        let command = CommandEnvelope {
            protocol_version: PROTOCOL_VERSION,
            command_id: CommandId::nil(),
            expected_revision: Some(4),
            command: Command::SetTaskStatus {
                task_id: TaskId::nil(),
                status: TaskStatus::Running,
            },
        };
        let json = serde_json::to_string(&command).unwrap();
        assert_eq!(
            json,
            r#"{"protocol_version":2,"command_id":"00000000-0000-0000-0000-000000000000","expected_revision":4,"command":{"type":"set_task_status","data":{"task_id":"00000000-0000-0000-0000-000000000000","status":"running"}}}"#
        );
    }

    #[test]
    fn version_negotiation_rejects_a_gap() {
        let result = negotiate_versions(&VersionRange { min: 1, max: 1 }, &VersionRange::current());
        assert!(matches!(
            result,
            Err(ApiError {
                code: ErrorCode::VersionMismatch,
                ..
            })
        ));
    }

    #[test]
    fn capability_negotiation_ignores_unknown_extensions_but_rejects_required_gaps() {
        let mut capabilities = Capabilities {
            protocol_version: 2,
            supported_versions: VersionRange::current(),
            minimum_compatible_client: 2,
            server_id: ServerId::nil(),
            certificate_fingerprint: String::new(),
            server_version: "1.0.0".into(),
            features: vec!["terminal-replay".into(), "future-extension".into()],
            openrouter: OpenRouterCapability::default(),
            limits: CapabilityLimits::default(),
        };
        assert_eq!(
            require_capabilities(&capabilities, &["terminal-replay"]).unwrap(),
            vec!["terminal-replay"]
        );
        capabilities
            .features
            .retain(|feature| feature != "terminal-replay");
        assert!(require_capabilities(&capabilities, &["terminal-replay"]).is_err());
    }

    #[test]
    fn handshake_response_round_trips_capabilities_without_losing_unknown_features() {
        let response = HandshakeResponse {
            negotiated_version: PROTOCOL_VERSION,
            server_id: ServerId::nil(),
            server_version: "1.0.0".into(),
            capabilities: Capabilities {
                protocol_version: PROTOCOL_VERSION,
                supported_versions: VersionRange::current(),
                minimum_compatible_client: MIN_COMPATIBLE_CLIENT,
                server_id: ServerId::nil(),
                certificate_fingerprint: "sha256:test".into(),
                server_version: "1.0.0".into(),
                features: vec!["future-extension".into()],
                openrouter: OpenRouterCapability::default(),
                limits: CapabilityLimits::default(),
            },
        };
        let encoded = serde_json::to_vec(&response).unwrap();
        let decoded: HandshakeResponse = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, response);
        assert!(
            decoded
                .capabilities
                .features
                .iter()
                .any(|feature| feature == "future-extension")
        );
    }

    #[test]
    fn task_and_mission_transitions_are_closed() {
        assert!(MissionStatus::Draft.can_transition_to(MissionStatus::Active));
        assert!(!MissionStatus::Completed.can_transition_to(MissionStatus::Active));
        assert!(TaskStatus::Running.can_transition_to(TaskStatus::Review));
        assert!(!TaskStatus::Done.can_transition_to(TaskStatus::Running));
    }

    #[test]
    fn organization_and_connector_dtos_round_trip() {
        let agent_id = AgentId::new();
        let profile_id = ConnectorProfileId::new();
        let graph = OrganizationGraph {
            id: OrganizationId::new(),
            draft_revision: 3,
            published_revision: 2,
            nodes: vec![
                OrganizationNode {
                    id: "staff-1".into(),
                    kind: OrganizationNodeKind::Staff,
                    label: "Builder".into(),
                    position: OrganizationPoint { x: 4.0, y: 8.0 },
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
                    id: "terminal-1".into(),
                    kind: OrganizationNodeKind::Capability,
                    label: "Terminal".into(),
                    position: OrganizationPoint { x: 90.0, y: 8.0 },
                    group_id: None,
                    agent_id: None,
                    capability: Some(OrganizationCapabilityKind::Terminal),
                    connector_profile_id: Some(profile_id),
                    profile_ref: Some("worktree".into()),
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
                id: "grant-1".into(),
                kind: OrganizationRelationKind::ToolAccess,
                source_node_id: "staff-1".into(),
                target_node_id: "terminal-1".into(),
                contract: OrganizationHandoffContract::default(),
                permissions: vec!["execute".into()],
            }],
            groups: Vec::new(),
            viewport: OrganizationViewport {
                x: 0.0,
                y: 0.0,
                zoom: 1.0,
            },
        };
        let state = OrganizationStateView {
            draft: graph.clone(),
            published: Some(graph),
            connector_profiles: vec![ConnectorProfileView {
                id: profile_id,
                name: "Worktree terminal".into(),
                kind: ConnectorKind::Terminal,
                config: serde_json::json!({"allowlist": ["cargo", "git"]}),
                health: ConnectorHealth::Healthy,
                configured: true,
                diagnostic: None,
                checked_at: Some(timestamp_now()),
                archived: false,
            }],
        };
        let encoded = serde_json::to_vec(&state).unwrap();
        let decoded: OrganizationStateView = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, state);
    }
}
