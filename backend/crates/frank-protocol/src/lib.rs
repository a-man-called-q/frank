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
mod toolchain;
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
pub use toolchain::*;
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

    #[test]
    fn runner_toolchain_and_journal_dtos_round_trip_with_defaults() {
        let runner_id = RunnerId::new();
        let project_id = ProjectId::new();
        let task_id = TaskId::new();
        let artifact = ToolchainArtifact {
            platform: "macos-arm64".into(),
            source: ToolchainArtifactSource::Host {
                executable: "flutter".into(),
            },
            sha256: "0".repeat(64),
            size_bytes: 1,
            archive: None,
        };
        let runner = RunnerView {
            id: runner_id,
            name: "build-mac".into(),
            host: "macbook.local".into(),
            status: RunnerStatus::Idle,
            last_seen_at: Some("2026-09-15T00:00:00Z".into()),
            toolchains: vec!["flutter@3.47.1".into()],
            path_mappings: vec![RunnerPathMapping {
                daemon_root: "/worktrees".into(),
                host_root: "/Users/builder/worktrees".into(),
                project_id: Some(project_id),
            }],
        };
        let check = CheckRunView {
            id: "check-1".into(),
            runner_id,
            project_id,
            task_id: Some(task_id),
            check_id: "rust-test".into(),
            status: CheckRunStatus::Passed,
            exit_code: Some(0),
            stdout: "ok".into(),
            stderr: String::new(),
            duration_ms: 42,
            started_at: "2026-09-15T00:00:00Z".into(),
            finished_at: Some("2026-09-15T00:00:01Z".into()),
        };
        let job = RunnerJobView {
            id: "job-1".into(),
            runner_id,
            project_id,
            task_id: Some(task_id),
            check_id: "rust-test".into(),
            kind: RunnerJobKind::Check,
            status: RunnerJobStatus::Passed,
            created_at: "2026-09-15T00:00:00Z".into(),
            updated_at: "2026-09-15T00:00:01Z".into(),
            result: Some(check),
            install_result: None,
        };
        let plan = ToolchainInstallPlan {
            manifest_id: "flutter".into(),
            version: "3.47.1".into(),
            source: "host:flutter".into(),
            sha256: "0".repeat(64),
            size_bytes: 1,
            install_path: "/Users/builder/.frank/toolchains/flutter".into(),
            checks: vec!["flutter --version".into()],
            approval_scope: "host-tool".into(),
            artifact: Some(artifact.clone()),
        };
        let requirement = ToolchainRequirementView {
            manifest_id: "flutter".into(),
            label: "Flutter".into(),
            required_version: "3.47.1".into(),
            status: ToolchainRequirementStatus::ManualRequirement,
            detected_version: Some("3.47.1".into()),
            diagnostic: None,
            install_plan: Some(plan),
        };
        let install = RunnerInstallJob {
            id: "install-1".into(),
            runner_id,
            project_id,
            task_id: Some(task_id),
            daemon_worktree: "/worktrees/project".into(),
            manifest_id: "flutter".into(),
            version: "3.47.1".into(),
            artifact,
            install_relative_path: "flutter".into(),
        };
        let filter = JournalFilter {
            before_sequence: Some(100),
            limit: Some(20),
            project_id: Some(project_id),
            mission_id: Some(MissionId::new()),
            task_id: Some(task_id),
            agent_id: Some(AgentId::new()),
            kinds: vec![JournalEntryKind::Toolchain, JournalEntryKind::Check],
            outcomes: vec![JournalOutcome::Success, JournalOutcome::Failure],
        };
        let entry = JournalEntryView {
            sequence: 99,
            occurred_at: "2026-09-15T00:00:01Z".into(),
            actor: ActorRef::system(),
            kind: JournalEntryKind::Check,
            outcome: JournalOutcome::Success,
            summary: "rust-test passed".into(),
            detail: Some(serde_json::json!({"runner": runner_id.to_string()})),
            project_id: Some(project_id),
            mission_id: None,
            task_id: Some(task_id),
            agent_id: None,
            check_run_id: Some("check-1".into()),
        };
        let payload = serde_json::json!({
            "runner": runner,
            "job": job,
            "requirement": requirement,
            "install": install,
            "filter": filter,
            "page": JournalPage {
                entries: vec![entry],
                next_before_sequence: Some(98),
            },
        });
        let decoded: serde_json::Value =
            serde_json::from_slice(&serde_json::to_vec(&payload).expect("payload should encode"))
                .expect("payload should decode");
        assert_eq!(decoded, payload);

        let defaults: JournalFilter = serde_json::from_str("{}").unwrap();
        assert_eq!(defaults, JournalFilter::default());
        let old_job: RunnerJobView = serde_json::from_str(
            r#"{"id":"job-legacy","runner_id":"00000000-0000-0000-0000-000000000000","project_id":"00000000-0000-0000-0000-000000000000","check_id":"check","status":"queued","created_at":"now","updated_at":"now"}"#,
        )
        .unwrap();
        assert_eq!(old_job.kind, RunnerJobKind::Check);
        assert!(old_job.result.is_none());
        assert!(old_job.install_result.is_none());
    }

    #[test]
    fn command_and_event_extensions_keep_their_discriminators() {
        let runner_id = RunnerId::nil();
        let project_id = ProjectId::nil();
        let command = Command::RequestHumanInput {
            task_id: TaskId::nil(),
            kind: HumanInputKind::Question,
            prompt: "choose a runner".into(),
        };
        let encoded = serde_json::to_value(&command).unwrap();
        assert_eq!(encoded["type"], "request_human_input");
        assert_eq!(encoded["data"]["kind"], "question");

        let event = Event::ToolchainInstallationRecorded {
            manifest_id: "dotnet".into(),
            version: "lts-lockfile".into(),
            runner_id,
            status: ToolchainRequirementStatus::NeedsApproval,
            project_id: Some(project_id),
            task_id: None,
            install_path: None,
        };
        let value = serde_json::to_value(&event).unwrap();
        assert_eq!(value["type"], "toolchain_installation_recorded");
        assert_eq!(value["data"]["status"], "needs_approval");

        let check = CheckRunView {
            id: "check-2".into(),
            runner_id,
            project_id,
            task_id: None,
            check_id: "dotnet-format".into(),
            status: CheckRunStatus::Failed,
            exit_code: Some(1),
            stdout: String::new(),
            stderr: "formatting required".into(),
            duration_ms: 10,
            started_at: "now".into(),
            finished_at: Some("later".into()),
        };
        let event = Event::CheckRunRecorded {
            check: check.clone(),
        };
        let decoded: Event = serde_json::from_value(serde_json::to_value(event).unwrap()).unwrap();
        assert_eq!(decoded, Event::CheckRunRecorded { check });
    }

    #[test]
    fn strict_dtos_reject_unknown_fields_but_legacy_optional_fields_default() {
        let unknown = serde_json::from_str::<ToolchainManifest>(
            r#"{"schema_version":1,"id":"x","label":"X","version":"1","detect":{},"install":{"directory":"x"},"unexpected":true}"#,
        );
        assert!(unknown.is_err());
        let artifact: ToolchainArtifact = serde_json::from_str(
            r#"{"platform":"macos-arm64","source":{"kind":"host","executable":"flutter"},"sha256":"0000000000000000000000000000000000000000000000000000000000000000","size_bytes":1}"#,
        )
        .unwrap();
        assert_eq!(artifact.archive, None);
        let check: ToolchainCheck =
            serde_json::from_str(r#"{"id":"version","program":"flutter"}"#).unwrap();
        assert_eq!(check.timeout_seconds, 600);
        assert!(check.args.is_empty());
        assert!(check.environment.is_empty());
    }
}
