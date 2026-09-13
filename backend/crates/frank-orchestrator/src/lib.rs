//! Frank's server-owned mission, task, mailbox, approval, budget, and Git
//! workflow state machines.
//!
//! The orchestrator is deliberately headless.  It receives typed commands,
//! validates authorization and transitions, updates a snapshot, and commits a
//! single event through `frank-store`.  GUI clients only observe the resulting
//! event stream; they never mutate projections directly.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

mod approval_flow;
mod budget;
mod budget_enforce;
mod capability;
mod connectors;
pub mod git;
mod helpers;
mod mailbox;
mod messaging;
mod mission_flow;
mod openrouter_tools;
mod reconcile;
mod runtime_flow;

mod operations;
mod reduce;
mod scheduler;
mod tools;
mod update_flow;
mod validate;

pub use budget::{BudgetLedger, UsageTotals};
use capability::*;
pub use connectors::{
    BrowserCdpSession, BrowserLaunchConfig, BrowserPolicy, ConnectorError, ConnectorSecretResolver,
    DatabaseStatementClass, GOOGLE_WORKSPACE_SCOPES, GoogleOAuthClient, GooglePkceChallenge,
    classify_database_statement, safe_terminal_environment, validate_postgres_profile_config,
    validate_sqlite_path, validate_terminal_command,
};
use helpers::*;
pub use mailbox::Mailbox;
use operations::*;
pub use scheduler::{Scheduler, SchedulerLimits};
use update_flow::*;
pub use validate::retry_after_failure;
use validate::*;

pub mod supervisor;

use crate::git::GitWorkflow;
use crate::reduce::materialize_role;
use frank_agent::{ProviderMessage, RuntimeEvent, RuntimeManager, StartRequest, UsageTelemetry};
use frank_protocol::*;
use frank_store::{Store, StoreError};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::process::Command as AsyncCommand;
use tokio::sync::Mutex;

fn provider_transcript(items: Vec<frank_store::StoredProviderSessionItem>) -> Vec<Value> {
    let completed_calls = items
        .iter()
        .filter_map(|item| {
            item.value
                .get("tool_result")
                .and_then(|result| result.get("call_id"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .collect::<HashSet<_>>();
    items
        .into_iter()
        .filter_map(|item| {
            if let Some(message) = item.value.get("message").cloned()
                && matches!(
                    message.get("role").and_then(Value::as_str),
                    Some("user" | "assistant" | "tool")
                )
            {
                return Some(message);
            }
            if let Some(tool_result) = item.value.get("tool_result") {
                let call_id = tool_result.get("call_id").and_then(Value::as_str)?;
                let content = tool_result.get("content").and_then(Value::as_str)?;
                return Some(serde_json::json!({
                    "role": "tool",
                    "tool_call_id": call_id,
                    "content": content,
                }));
            }
            let pending = item.value.get("tool_call")?;
            let call_id = pending.get("call_id").and_then(Value::as_str)?;
            if completed_calls.contains(call_id) {
                return None;
            }
            Some(serde_json::json!({
                "role": "tool",
                "tool_call_id": call_id,
                "content": "{\"ok\":false,\"error\":\"Frank daemon restarted before approval completed\"}",
            }))
        })
        .collect()
}

pub const DEFAULT_MAX_ATTEMPTS: u8 = 2;
pub const DEFAULT_MESSAGE_HOP_LIMIT: u8 = 6;
pub const DEFAULT_MAX_CONCURRENCY: usize = 4;

#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("store error: {0}")]
    Store(#[from] StoreError),
    #[error("invalid state transition: {0}")]
    InvalidTransition(String),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("organization revision conflict (expected {expected}, actual {actual})")]
    OrganizationRevisionConflict { expected: u64, actual: u64 },
    #[error("permission denied")]
    Forbidden,
    #[error("resource not found")]
    NotFound,
    #[error("budget exceeded")]
    BudgetExceeded,
    #[error("provider unavailable: {0}")]
    ProviderUnavailable(String),
}

pub type Result<T> = std::result::Result<T, OrchestratorError>;

#[derive(Clone)]
pub struct Orchestrator {
    store: Store,
    runtime: RuntimeManager,
    scheduler: Arc<Mutex<Scheduler>>,
    mailbox: Arc<Mutex<Mailbox>>,
    budgets: Arc<Mutex<BudgetLedger>>,
    max_message_bytes: usize,
    memory_root: PathBuf,
    /// Live provider sessions are owned by the daemon and keyed by task. The
    /// map is deliberately not part of the persisted snapshot; provider
    /// session IDs in the agent projection are used for resume after restart.
    sessions: Arc<Mutex<HashMap<TaskId, Arc<frank_agent::RuntimeSession>>>>,
    /// Live supervisor processes are keyed by mission. The stable provider
    /// session ID is persisted on `MissionView`; this map only prevents two
    /// daemon reconciler ticks from spawning duplicate supervisors.
    supervisor_sessions: Arc<Mutex<HashMap<MissionId, Arc<frank_agent::RuntimeSession>>>>,
    /// Monotonic-enough wall-clock anchors for active budget scopes. These
    /// are kept outside the wire snapshot because timestamps are only used
    /// for a live hard-stop; token/cost/turn accounting remains durable in
    /// `Snapshot::usage` and is rebuilt on restart.
    scope_started_at: Arc<Mutex<HashMap<String, u64>>>,
    agent_capabilities: Arc<Mutex<HashMap<String, AgentCapability>>>,
    /// Tool calls that are waiting on an explicit owner decision. The
    /// approval row is durable; this in-memory entry only holds the
    /// task-scoped continuation needed to resume the live OpenRouter turn.
    tool_approvals: Arc<Mutex<HashMap<ApprovalId, PendingToolCall>>>,
    /// Connector credentials are resolved only inside the daemon. `None` is
    /// the safe default used by standalone orchestrator tests and makes every
    /// external connector fail closed until frankd injects its keychain
    /// boundary.
    connector_secrets: Option<Arc<dyn ConnectorSecretResolver>>,
    update_source: Arc<dyn frank_update::UpdateSource>,
}

#[derive(Debug, Clone)]
struct PendingToolCall {
    task_id: TaskId,
    agent_id: AgentId,
    call_id: String,
    name: String,
    input: Value,
}

impl Orchestrator {
    pub fn new(store: Store) -> Self {
        let memory_root = store
            .database_path()
            .and_then(Path::parent)
            .map(|parent| parent.join("memory"))
            .unwrap_or_else(|| PathBuf::from("memory"));
        Self {
            store,
            runtime: RuntimeManager::new(),
            scheduler: Arc::new(Mutex::new(Scheduler::default())),
            mailbox: Arc::new(Mutex::new(Mailbox::default())),
            budgets: Arc::new(Mutex::new(BudgetLedger::default())),
            max_message_bytes: MAX_MESSAGE_BODY_BYTES,
            memory_root,
            sessions: Arc::new(Mutex::new(HashMap::new())),
            supervisor_sessions: Arc::new(Mutex::new(HashMap::new())),
            scope_started_at: Arc::new(Mutex::new(HashMap::new())),
            agent_capabilities: Arc::new(Mutex::new(HashMap::new())),
            tool_approvals: Arc::new(Mutex::new(HashMap::new())),
            connector_secrets: None,
            update_source: Arc::new(frank_update::HttpUpdateSource::default()),
        }
    }

    pub fn with_runtime(store: Store, runtime: RuntimeManager) -> Self {
        Self {
            runtime,
            ..Self::new(store)
        }
    }

    /// Narrow health boundary used by the HTTP diagnostics surface. Runtime
    /// internals stay owned by the orchestrator rather than becoming a server
    /// dependency.
    pub async fn runtime_doctor(&self) -> Vec<frank_agent::RuntimeProbe> {
        self.runtime.doctor().await
    }

    pub fn with_connector_secrets(mut self, resolver: Arc<dyn ConnectorSecretResolver>) -> Self {
        self.connector_secrets = Some(resolver);
        self
    }

    /// Inject the verified update boundary for air-gapped and deterministic
    /// tests. Durable operation state remains owned by the orchestrator.
    pub fn with_update_source(mut self, source: Arc<dyn frank_update::UpdateSource>) -> Self {
        self.update_source = source;
        self
    }

    pub(crate) async fn connector_secret(
        &self,
        profile_id: ConnectorProfileId,
    ) -> std::result::Result<Option<String>, String> {
        let Some(resolver) = self.connector_secrets.as_ref() else {
            return Ok(None);
        };
        resolver
            .secret(profile_id)
            .await
            .map_err(|error| error.to_string())
    }

    pub async fn snapshot(&self) -> Result<Snapshot> {
        Ok(self.store.snapshot().await?)
    }

    /// Validate an exact OpenRouter canonical model slug against the current
    /// tool-capable catalog. A missing model is allowed for dormant profiles;
    /// attempting to start one without a model fails at the runtime boundary.
    pub(crate) async fn validate_openrouter_model(&self, model: Option<&str>) -> Result<()> {
        let Some(model) = model else {
            return Ok(());
        };
        if model.trim().is_empty() {
            return Err(OrchestratorError::Validation(
                "OpenRouter model cannot be empty".into(),
            ));
        }
        let catalog = self
            .runtime
            .model_catalog(false)
            .await
            .map_err(|error| OrchestratorError::ProviderUnavailable(error.to_string()))?;
        if !catalog.iter().any(|descriptor| {
            descriptor
                .canonical_slug
                .as_deref()
                .unwrap_or(&descriptor.id)
                == model
        }) {
            return Err(OrchestratorError::Validation(format!(
                "OpenRouter model '{model}' is not available in the current catalog"
            )));
        }
        Ok(())
    }

    /// Seed the default worker role without probing or starting a provider.
    /// Roles are dormant templates, so first-run bootstrap stays usable even
    /// before the owner configures an OpenRouter credential and model.
    pub async fn ensure_builtin_role(&self) -> Result<RoleId> {
        for _ in 0..4 {
            let snapshot = self.store.snapshot().await?;
            if let Some(role) = snapshot
                .roles
                .iter()
                .find(|role| role.name.eq_ignore_ascii_case("Generalist") && !role.archived)
            {
                return Ok(role.id);
            }
            let role = RoleView {
                id: RoleId::new(),
                name: "Generalist".into(),
                description: "A dependable general-purpose worker role.".into(),
                template: AgentTemplate::Generalist,
                model: None,
                pack_id: Some("caveman".into()),
                pack_level: Some("full".into()),
                instructions:
                    "Complete the assigned task and document every handoff on its taskboard card."
                        .into(),
                policy: AgentPolicy::default(),
                budget: Budget::unlimited(),
                avatar: AvatarSpec {
                    palette: "frank-generalist".into(),
                    seed: 1,
                },
                revision: 1,
                archived: false,
            };
            let mut next = snapshot.clone();
            next.roles.push(role.clone());
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(snapshot.revision),
                    ActorRef::system(),
                    Event::RoleUpserted { role: role.clone() },
                    next,
                    CommandResult::Created {
                        id: role.id.to_string(),
                    },
                )
                .await
            {
                Ok(_) => return Ok(role.id),
                Err(StoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }

    /// Seed the persistent Frank supervisor profile.  The profile is always
    /// present, but the supervisor remains unconfigured until the owner
    /// selects an OpenRouter model.
    pub async fn ensure_builtin_supervisor(&self) -> Result<AgentId> {
        for _ in 0..4 {
            let mut snapshot = self.store.snapshot().await?;
            if let Some(agent) = snapshot
                .agents
                .iter()
                .find(|agent| agent.display_name == "Frank supervisor")
            {
                return Ok(agent.id);
            }
            let supervisor = AgentView {
                // The supervisor is persistent like any other profile, but
                // its identity is still generated on first boot rather than
                // using a sentinel UUID that could collide with imported
                // data.
                id: AgentId::new(),
                // The built-in supervisor is a coordination process, not a
                // worker slot, and intentionally has no primary worker role.
                role_id: None,
                role_revision: 0,
                display_name: "Frank supervisor".to_string(),
                template: AgentTemplate::Generalist,
                model: None,
                effective_model: None,
                model_source: ModelSource::Role,
                model_override: None,
                pending_model_override: None,
                pending_model_change: false,
                pack_id: Some("caveman".to_string()),
                pack_level: Some("full".to_string()),
                instructions:
                    "Decompose missions, coordinate workers, review results, and deliver safely."
                        .to_string(),
                policy: AgentPolicy {
                    filesystem: FilesystemPolicy::ReadOnly,
                    shell: ShellPolicy::Deny,
                    network: NetworkPolicy::Deny,
                    approval: ApprovalPolicy::Never,
                },
                budget: Budget::unlimited(),
                avatar: AvatarSpec {
                    palette: "frank-supervisor".to_string(),
                    seed: 1,
                },
                status: AgentStatus::Offline,
                provider_session_id: None,
                last_claimed_at: None,
                archived: false,
            };
            let expected_revision = snapshot.revision;
            snapshot.agents.push(supervisor.clone());
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(expected_revision),
                    ActorRef::system(),
                    Event::AgentUpserted {
                        agent: supervisor.clone(),
                    },
                    snapshot,
                    CommandResult::Created {
                        id: supervisor.id.to_string(),
                    },
                )
                .await
            {
                Ok(_) => return Ok(supervisor.id),
                Err(StoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }

    /// Execute one command.  Errors are returned as protocol responses so a
    /// network client always receives a stable error code and, for conflicts,
    /// the latest snapshot.
    pub async fn execute(
        &self,
        envelope: CommandEnvelope,
        actor: ActorRef,
        role: DeviceRole,
    ) -> CommandResponse {
        let mut envelope = envelope;
        let command_id = envelope.command_id;
        let mission_to_plan = match &envelope.command {
            Command::CreateMission {
                project_id: _,
                objective,
            } => Some(objective.clone()),
            _ => None,
        };
        let submitted_supervisor_plan = match &envelope.command {
            Command::SubmitSupervisorPlan { proposal, .. } => Some(proposal.clone()),
            _ => None,
        };
        let approval_action = match &envelope.command {
            Command::DecideApproval {
                approval_id,
                decision,
            } => Some((*approval_id, *decision)),
            _ => None,
        };
        let terminal_action = match &envelope.command {
            Command::TakeControl { session_id } => Some((*session_id, true)),
            Command::ReleaseControl { session_id, .. } | Command::CloseTerminal { session_id } => {
                Some((*session_id, false))
            }
            _ => None,
        };
        let current_revision = self.store.current_revision().await.unwrap_or_default();
        if envelope.protocol_version != PROTOCOL_VERSION {
            return CommandResponse::failed(
                command_id,
                current_revision,
                ApiError::new(
                    ErrorCode::VersionMismatch,
                    "command protocol version is not supported",
                ),
            );
        }
        if let Err(error) = validate_command_size(&envelope) {
            return CommandResponse::failed(command_id, current_revision, error);
        }
        if let Err(error) = authorize(&envelope.command, actor.kind, role) {
            return CommandResponse::failed(
                command_id,
                current_revision,
                ApiError::new(ErrorCode::Forbidden, error.to_string()),
            );
        }
        // Replay the durable response before reducing the command again.
        // `commit_command` already protects the projection itself, but some
        // commands intentionally perform daemon-side work after the commit
        // (mission planning, clone/delivery, PTY setup). Re-running the
        // reducer for a lost HTTP response could otherwise duplicate those
        // side effects even though the database mutation is idempotent.
        match self.store.idempotent_response(command_id).await {
            Ok(Some(response)) => return response,
            Ok(None) => {}
            Err(_) => {
                return CommandResponse::failed(
                    command_id,
                    current_revision,
                    ApiError::new(ErrorCode::Internal, "command idempotency state unavailable"),
                );
            }
        }
        // Organization autosave uses its own draft/published revision.  It
        // must not conflict merely because an unrelated task event advanced
        // the global snapshot between the desktop read and this command.
        // Re-reduce against the newest snapshot and commit with the current
        // global revision so unrelated task state is never overwritten.
        if envelope.expected_revision.is_none()
            && matches!(
                &envelope.command,
                Command::SaveOrganizationDraft { .. }
                    | Command::PublishOrganization { .. }
                    | Command::CreateConnectorProfile(_)
                    | Command::UpdateConnectorProfile { .. }
                    | Command::ArchiveConnectorProfile { .. }
            )
        {
            return match self.commit_organization_with_retry(envelope, actor).await {
                Ok(response) => response,
                Err(error) => self.error_response(command_id, error).await,
            };
        }
        // Every mutation is bound to the revision observed at the start of
        // this request, even when a legacy client omits an explicit
        // `expected_revision`. Without this default, two concurrent claims or
        // comments could both reduce the same snapshot and the later commit
        // would silently erase the first feed entry. The transactional store
        // repeats the check at commit time for the final race window.
        if envelope.expected_revision.is_none() {
            envelope.expected_revision = Some(current_revision);
        }
        if let Some(expected_revision) = envelope.expected_revision
            && expected_revision != current_revision
        {
            let error = self
                .store
                .snapshot()
                .await
                .map(ApiError::conflict)
                .unwrap_or_else(|_| ApiError::new(ErrorCode::StaleRevision, "stale revision"));
            return CommandResponse::failed(command_id, current_revision, error);
        }
        match self.apply(envelope, actor).await {
            Ok(response) => {
                if let Some((approval_id, decision)) = approval_action {
                    // The approval transition is authoritative in SQLite;
                    // answering a live provider callback is a best-effort
                    // side effect. A provider may have crashed or been
                    // replaced between the request and the operator click,
                    // in which case reconciliation will resume it from its
                    // durable session metadata.
                    self.respond_to_provider_approval(approval_id, decision)
                        .await;
                }
                if response.error.is_none()
                    && let Some((session_id, pause)) = terminal_action
                {
                    // Lease state is committed first. Runtime control is a
                    // daemon side effect: stop/restart the provider session
                    // so a human shell cannot race worker scheduling.
                    let _ = if pause {
                        self.pause_for_terminal(session_id).await
                    } else {
                        self.resume_after_terminal(session_id).await
                    };
                }
                if let (Some(objective), Some(CommandResult::Created { id })) =
                    (mission_to_plan, response.result.clone())
                    && let Ok(mission_id) = MissionId::parse(&id)
                    && let Err(error) = self.plan_mission(mission_id, &objective).await
                {
                    let _ = self.block_mission(mission_id, error.to_string()).await;
                }
                if let Some(proposal) = submitted_supervisor_plan
                    && let Ok(plan) = supervisor::proposal_to_plan(proposal)
                {
                    let _ = self.materialize_supervisor_plan(plan).await;
                }
                response
            }
            Err(error) => self.error_response(command_id, error).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: TaskId, deps: Vec<TaskId>) -> TaskView {
        TaskView {
            id,
            mission_id: MissionId::nil(),
            title: "task".into(),
            objective: "objective".into(),
            dependencies: deps,
            required_role_id: None,
            priority: 0,
            budget: Budget::unlimited(),
            status: TaskStatus::Backlog,
            assigned_agent: None,
            reviewer_agent: None,
            claimed_at: None,
            claim_source: None,
            attempt: 0,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            worktree: None,
            branch: None,
            result_artifact: None,
            taskboard_id: None,
            workflow_id: None,
            parent_task_id: None,
            child_task_ids: Vec::new(),
            kind: WorkItemKind::Task,
            active_role_node_id: None,
            organization_revision: None,
            rework_limit: DEFAULT_REWORK_LIMIT,
            rework_count: 0,
        }
    }

    fn review_agent(id: AgentId, name: &str) -> AgentView {
        AgentView {
            id,
            role_id: None,
            role_revision: 0,
            display_name: name.into(),
            template: AgentTemplate::Reviewer,
            model: None,
            effective_model: None,
            model_source: ModelSource::Role,
            model_override: None,
            pending_model_override: None,
            pending_model_change: false,
            pack_id: None,
            pack_level: None,
            instructions: String::new(),
            policy: AgentPolicy::default(),
            budget: Budget::unlimited(),
            avatar: AvatarSpec {
                palette: "test".into(),
                seed: 1,
            },
            status: AgentStatus::Idle,
            provider_session_id: None,
            last_claimed_at: None,
            archived: false,
        }
    }

    #[test]
    fn dag_cycle_is_rejected() {
        let a = TaskId::new();
        let b = TaskId::new();
        let tasks = vec![task(a, vec![b]), task(b, vec![a])];
        assert!(validate_task_view(&task(TaskId::nil(), vec![]), &tasks).is_err());
    }

    #[test]
    fn merge_conflicts_are_distinguished_from_other_git_failures() {
        assert!(is_merge_conflict_error(
            "git merge --squash failed: CONFLICT (content): Merge conflict in src/lib.rs"
        ));
        assert!(is_merge_conflict_error(
            "Automatic merge failed; fix conflicts"
        ));
        assert!(!is_merge_conflict_error(
            "git push failed: permission denied"
        ));
    }

    #[test]
    fn update_artifact_selection_never_falls_back_to_another_target() {
        let manifest = frank_update::UpdateManifest {
            schema_version: frank_update::MANIFEST_SCHEMA_VERSION,
            frank_version: "1.0.1".into(),
            release_timestamp: "0".into(),
            protocol_min: 1,
            protocol_max: 1,
            minimum_rollback_version: "1.0.0".into(),
            artifacts: vec![frank_update::UpdateArtifact {
                target: "aarch64-apple-darwin".into(),
                package_kind: "dmg".into(),
                url: "https://example.invalid/frank.dmg".into(),
                size: 1,
                sha256: "0".repeat(64),
            }],
            release_notes_url: "https://example.invalid/release".into(),
            key_id: "test".into(),
        };
        assert!(select_update_artifact(&manifest, "x86_64-apple-darwin", "dmg").is_none());
        assert!(select_update_artifact(&manifest, "aarch64-apple-darwin", "dmg").is_some());
    }

    #[test]
    fn duplicate_task_dependencies_are_rejected() {
        let dependency = TaskId::new();
        let mission = MissionId::nil();
        let spec = TaskSpec {
            mission_id: mission,
            title: "task".into(),
            objective: "objective".into(),
            dependencies: vec![dependency, dependency],
            required_role_id: None,
            priority: 0,
            assigned_agent: None,
            budget: Budget::unlimited(),
            taskboard_id: None,
            workflow_id: None,
            parent_task_id: None,
            kind: WorkItemKind::Task,
            rework_limit: DEFAULT_REWORK_LIMIT,
        };
        let tasks = vec![task(dependency, vec![])];
        assert!(validate_task_spec(&spec, &tasks).is_err());
    }

    #[tokio::test]
    async fn published_review_creates_and_decides_a_durable_work_item() {
        let store = Store::open_in_memory().await.unwrap();
        let source = AgentId::new();
        let reviewer = AgentId::new();
        let mission_id = MissionId::new();
        let task_id = TaskId::new();
        let mut snapshot = store.snapshot().await.unwrap();
        snapshot.agents.push(review_agent(source, "Builder"));
        snapshot.agents.push(review_agent(reviewer, "Reviewer"));
        snapshot.missions.push(MissionView {
            id: mission_id,
            project_id: ProjectId::new(),
            objective: "review test".into(),
            status: MissionStatus::Active,
            supervisor_session_id: None,
            branch: "frank/review-test".into(),
            budget: Budget::unlimited(),
            created_at: timestamp_now(),
            updated_at: timestamp_now(),
        });
        let mut running = task(task_id, Vec::new());
        running.mission_id = mission_id;
        running.status = TaskStatus::Running;
        running.assigned_agent = Some(source);
        snapshot.tasks.push(running);
        snapshot.organization.published = Some(OrganizationGraph {
            id: OrganizationId::new(),
            draft_revision: 1,
            published_revision: 1,
            nodes: vec![
                OrganizationNode {
                    id: "source".into(),
                    kind: OrganizationNodeKind::Staff,
                    label: "Builder".into(),
                    position: OrganizationPoint::default(),
                    group_id: None,
                    agent_id: Some(source),
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
                    id: "reviewer".into(),
                    kind: OrganizationNodeKind::Staff,
                    label: "Reviewer".into(),
                    position: OrganizationPoint::default(),
                    group_id: None,
                    agent_id: Some(reviewer),
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
            ],
            relations: vec![OrganizationRelation {
                id: "review-edge".into(),
                kind: OrganizationRelationKind::Review,
                source_node_id: "source".into(),
                target_node_id: "reviewer".into(),
                contract: OrganizationHandoffContract::default(),
                permissions: Vec::new(),
            }],
            groups: Vec::new(),
            viewport: OrganizationViewport::default(),
        });
        store.replace_snapshot(&snapshot).await.unwrap();
        let orchestrator = Orchestrator::new(store.clone());
        let to_review = orchestrator
            .execute(
                CommandEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    command_id: CommandId::new(),
                    expected_revision: Some(0),
                    command: Command::SetTaskStatus {
                        task_id,
                        status: TaskStatus::Review,
                    },
                },
                ActorRef {
                    kind: ActorKind::Agent,
                    id: Some(source.to_string()),
                    display_name: None,
                },
                DeviceRole::Operator,
            )
            .await;
        assert!(to_review.error.is_none());
        let review = store.snapshot().await.unwrap().review_items[0].clone();
        assert_eq!(review.reviewer_agent, reviewer);
        assert_eq!(review.status, ReviewWorkItemStatus::Pending);
        let decided = orchestrator
            .execute(
                CommandEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    command_id: CommandId::new(),
                    expected_revision: Some(to_review.revision),
                    command: Command::DecideReview {
                        review_item_id: review.id,
                        decision: ReviewDecision::Approve,
                        reason: Some("looks good".into()),
                    },
                },
                ActorRef {
                    kind: ActorKind::Agent,
                    id: Some(reviewer.to_string()),
                    display_name: None,
                },
                DeviceRole::Operator,
            )
            .await;
        assert!(decided.error.is_none());
        let snapshot = store.snapshot().await.unwrap();
        assert_eq!(snapshot.tasks[0].status, TaskStatus::Done);
        assert_eq!(
            snapshot.review_items[0].status,
            ReviewWorkItemStatus::Approved
        );
        assert_eq!(
            snapshot.review_items[0].decision_reason.as_deref(),
            Some("looks good")
        );
    }

    #[test]
    fn mailbox_deduplicates_and_caps_hops() {
        let mut mailbox = Mailbox::default();
        let id = MessageId::new();
        assert!(mailbox.accept(id, 1));
        assert!(!mailbox.accept(id, 1));
        assert!(!mailbox.accept(MessageId::new(), DEFAULT_MESSAGE_HOP_LIMIT + 1));
    }

    #[test]
    fn retry_policy_allows_two_attempts_then_blocks() {
        let mut value = task(TaskId::new(), Vec::new());
        assert_eq!(retry_after_failure(&mut value), TaskStatus::Ready);
        assert_eq!(value.attempt, 1);
        assert_eq!(retry_after_failure(&mut value), TaskStatus::Blocked);
        assert_eq!(value.attempt, 2);
    }

    #[test]
    fn scheduler_enforces_total_cap() {
        let mut scheduler = Scheduler {
            limits: SchedulerLimits { max_concurrency: 1 },
            ..Default::default()
        };
        assert!(scheduler.start(TaskId::new()));
        assert!(!scheduler.can_start());
    }

    #[test]
    fn budget_uses_measured_tokens_only() {
        let mut ledger = BudgetLedger::default();
        ledger.record("mission", &UsageTelemetry::estimated(Some(100), Some(100)));
        assert!(ledger.allows_measured(
            "mission",
            &Budget {
                measured_tokens: Some(1),
                ..Budget::unlimited()
            }
        ));
    }

    #[test]
    fn turn_budget_is_hard_limited_independently_of_token_measurement() {
        let mut ledger = BudgetLedger::default();
        let budget = Budget {
            turns: Some(1),
            ..Budget::unlimited()
        };
        ledger.record("task", &UsageTelemetry::estimated(Some(100), Some(100)));
        assert!(ledger.allows_measured("task", &budget));
        ledger.record("task", &UsageTelemetry::estimated(Some(1), Some(1)));
        assert!(!ledger.allows_measured("task", &budget));
        assert_eq!(ledger.turns("task"), 2);
    }

    #[tokio::test]
    async fn broker_advances_queued_messages_to_delivered() {
        let store = Store::open_in_memory().await.unwrap();
        let mission_id = MissionId::new();
        let mut snapshot = store.snapshot().await.unwrap();
        snapshot.missions.push(MissionView {
            id: mission_id,
            project_id: ProjectId::new(),
            objective: "message test".into(),
            status: MissionStatus::Active,
            supervisor_session_id: None,
            branch: "frank/mission-message-test".into(),
            budget: Budget::unlimited(),
            created_at: timestamp_now(),
            updated_at: timestamp_now(),
        });
        store.replace_snapshot(&snapshot).await.unwrap();
        let orchestrator = Orchestrator::new(store.clone());
        let message_id = MessageId::new();
        let response = orchestrator
            .execute(
                CommandEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    command_id: CommandId::new(),
                    expected_revision: Some(0),
                    command: Command::SendMessage(MessageSpec {
                        message_id: Some(message_id),
                        mission_id,
                        task_id: None,
                        recipient: ActorRef::supervisor(),
                        act: MessageAct::Inform,
                        body: "hello supervisor".into(),
                        artifact_ids: Vec::new(),
                        reply_to: None,
                        hop: 0,
                    }),
                },
                ActorRef {
                    kind: ActorKind::Device,
                    id: Some("device".into()),
                    display_name: Some("test".into()),
                },
                DeviceRole::Operator,
            )
            .await;
        assert!(response.error.is_none());
        assert_eq!(
            store.snapshot().await.unwrap().messages[0].delivery,
            DeliveryStatus::Delivered
        );
        assert_eq!(store.events_after(0, 10).await.unwrap().events.len(), 2);
        let revision = store.current_revision().await.unwrap();
        let duplicate = orchestrator
            .execute(
                CommandEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    command_id: CommandId::new(),
                    expected_revision: Some(revision),
                    command: Command::SendMessage(MessageSpec {
                        message_id: Some(message_id),
                        mission_id,
                        task_id: None,
                        recipient: ActorRef::supervisor(),
                        act: MessageAct::Inform,
                        body: "hello supervisor".into(),
                        artifact_ids: Vec::new(),
                        reply_to: None,
                        hop: 0,
                    }),
                },
                ActorRef {
                    kind: ActorKind::Device,
                    id: Some("device".into()),
                    display_name: Some("test".into()),
                },
                DeviceRole::Operator,
            )
            .await;
        assert!(duplicate.error.is_none());
        assert_eq!(duplicate.revision, revision);
        assert_eq!(store.current_revision().await.unwrap(), revision);
        assert_eq!(store.events_after(0, 10).await.unwrap().events.len(), 2);
    }

    #[test]
    fn retry_policy_blocks_after_the_configured_attempts() {
        let mut task = task(TaskId::nil(), vec![]);
        assert_eq!(retry_after_failure(&mut task), TaskStatus::Ready);
        assert_eq!(retry_after_failure(&mut task), TaskStatus::Blocked);
        assert_eq!(task.attempt, 2);
    }

    #[test]
    fn task_commit_subject_is_stable_and_bounded() {
        let task_id = TaskId::nil();
        let message = stable_task_commit_message(task_id, "ship\nfeature\0");
        assert_eq!(message, format!("Frank task {task_id}: shipfeature"));
        assert!(message.len() <= 4_096);
    }

    #[tokio::test]
    async fn runtime_failure_cleanup_releases_worker_after_approval_failure() {
        let store = Store::open_in_memory().await.unwrap();
        let orchestrator = Orchestrator::new(store.clone());
        let mission_id = MissionId::new();
        let task_id = TaskId::new();
        let agent_id = AgentId::new();
        let mut snapshot = store.snapshot().await.unwrap();
        snapshot.missions.push(MissionView {
            id: mission_id,
            project_id: ProjectId::new(),
            objective: "approval cleanup test".into(),
            status: MissionStatus::Active,
            supervisor_session_id: None,
            branch: "frank/approval-cleanup-test".into(),
            budget: Budget::unlimited(),
            created_at: timestamp_now(),
            updated_at: timestamp_now(),
        });
        snapshot.agents.push(review_agent(agent_id, "worker"));
        let mut running = task(task_id, Vec::new());
        running.mission_id = mission_id;
        running.status = TaskStatus::Running;
        running.assigned_agent = Some(agent_id);
        snapshot.tasks.push(running);
        store.replace_snapshot(&snapshot).await.unwrap();

        orchestrator
            .fail_runtime_task(task_id, agent_id, "approval request failed".into())
            .await
            .unwrap();

        let after = store.snapshot().await.unwrap();
        assert_eq!(after.tasks[0].status, TaskStatus::Ready);
        assert_eq!(after.agents[0].status, AgentStatus::Idle);
        assert!(orchestrator.sessions.lock().await.is_empty());
        assert!(
            after
                .messages
                .iter()
                .any(|message| message.body == "worker failed: approval request failed")
        );
    }

    #[tokio::test]
    async fn usage_event_is_retained_in_the_authoritative_snapshot() {
        let store = Store::open_in_memory().await.unwrap();
        let orchestrator = Orchestrator::new(store.clone());
        let usage = UsageView {
            id: AttemptId::new(),
            scope: BudgetScope::Task,
            scope_id: TaskId::new().to_string(),
            provider: UsageProviderId("codex".into()),
            model: None,
            measured_input_tokens: Some(4),
            measured_output_tokens: Some(6),
            estimated_input_tokens: None,
            estimated_output_tokens: None,
            cost_micros: Some(12),
            cached_input_tokens: None,
            reasoning_tokens: None,
            recorded_at: timestamp_now(),
        };
        orchestrator
            .record_usage(usage.clone(), None)
            .await
            .unwrap();
        let snapshot = store.snapshot().await.unwrap();
        assert_eq!(snapshot.usage, vec![usage]);
        assert_eq!(snapshot.event_seq, 1);
    }

    #[tokio::test]
    async fn reconcile_blocks_a_silent_task_after_persisted_time_deadline() {
        let store = Store::open_in_memory().await.unwrap();
        let mission_id = MissionId::new();
        let task_id = TaskId::new();
        let now = epoch_seconds();
        let mut snapshot = store.snapshot().await.unwrap();
        snapshot.missions.push(MissionView {
            id: mission_id,
            project_id: ProjectId::new(),
            objective: "time budget test".into(),
            status: MissionStatus::Active,
            supervisor_session_id: None,
            branch: "frank/mission-time-budget".into(),
            budget: Budget::unlimited(),
            created_at: timestamp_now(),
            updated_at: timestamp_now(),
        });
        let mut running = task(task_id, Vec::new());
        running.mission_id = mission_id;
        running.status = TaskStatus::Running;
        running.budget = Budget {
            time_seconds: Some(1),
            ..Budget::unlimited()
        };
        snapshot.tasks.push(running);
        store.replace_snapshot(&snapshot).await.unwrap();
        store
            .upsert_budget_clock(
                &task_id.to_string(),
                now.saturating_sub(10),
                Some(now.saturating_sub(9)),
            )
            .await
            .unwrap();

        let orchestrator = Orchestrator::new(store.clone());
        orchestrator.enforce_time_budgets().await.unwrap();

        assert_eq!(
            store
                .snapshot()
                .await
                .unwrap()
                .tasks
                .iter()
                .find(|task| task.id == task_id)
                .unwrap()
                .status,
            TaskStatus::Blocked
        );
        assert!(
            store
                .events_after(0, 16)
                .await
                .unwrap()
                .events
                .iter()
                .any(|event| matches!(
                    event.event,
                    Event::BudgetPaused {
                        scope: BudgetScope::Task,
                        ..
                    }
                ))
        );
    }

    #[tokio::test]
    async fn apply_update_persists_host_operation_with_state_transition() {
        let store = Store::open_in_memory().await.unwrap();
        let orchestrator = Orchestrator::new(store.clone());
        let update_id = UpdateId::new();
        let mut snapshot = store.snapshot().await.unwrap();
        snapshot.update = Some(UpdateView {
            id: update_id,
            version: "1.0.1".into(),
            state: UpdateState::Staged,
            target: "aarch64-apple-darwin".into(),
            size: 1,
            sha256: "a".repeat(64),
            error: None,
            checked_at: timestamp_now(),
        });
        store.replace_snapshot(&snapshot).await.unwrap();

        let response = orchestrator
            .execute(
                CommandEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    command_id: CommandId::new(),
                    expected_revision: Some(0),
                    command: Command::ApplyUpdate { update_id },
                },
                ActorRef {
                    kind: ActorKind::Device,
                    id: Some("owner".into()),
                    display_name: Some("owner".into()),
                },
                DeviceRole::Owner,
            )
            .await;
        assert!(response.error.is_none());
        assert!(matches!(response.result, Some(CommandResult::Operation(_))));
        let snapshot = store.snapshot().await.unwrap();
        assert_eq!(snapshot.update.unwrap().state, UpdateState::Applying);
        assert_eq!(snapshot.operations.len(), 1);
        assert_eq!(snapshot.operations[0].kind, OperationKind::HostUpdate);
        assert!(
            store
                .operation(snapshot.operations[0].id)
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn expired_terminal_lease_is_released_and_worker_can_resume() {
        let store = Store::open_in_memory().await.unwrap();
        let orchestrator = Orchestrator::new(store.clone());
        let mission_id = MissionId::new();
        let task_id = TaskId::new();
        let agent_id = AgentId::new();
        let session_id = TerminalSessionId::new();
        let mut snapshot = store.snapshot().await.unwrap();
        snapshot.missions.push(MissionView {
            id: mission_id,
            project_id: ProjectId::new(),
            objective: "resume after lease expiry".into(),
            status: MissionStatus::Active,
            supervisor_session_id: None,
            branch: "frank/mission-lease".into(),
            budget: Budget::unlimited(),
            created_at: timestamp_now(),
            updated_at: timestamp_now(),
        });
        snapshot.agents.push(AgentView {
            id: agent_id,
            role_id: None,
            role_revision: 0,
            display_name: "worker".into(),
            template: AgentTemplate::Builder,
            model: None,
            effective_model: None,
            model_source: ModelSource::Role,
            model_override: None,
            pending_model_override: None,
            pending_model_change: false,
            pack_id: None,
            pack_level: None,
            instructions: String::new(),
            policy: AgentPolicy::default(),
            budget: Budget::unlimited(),
            avatar: AvatarSpec {
                palette: "worker".into(),
                seed: 7,
            },
            status: AgentStatus::Paused,
            provider_session_id: None,
            last_claimed_at: None,
            archived: false,
        });
        snapshot.tasks.push(TaskView {
            id: task_id,
            mission_id,
            title: "running task".into(),
            objective: "continue".into(),
            dependencies: Vec::new(),
            required_role_id: None,
            priority: 0,
            budget: Budget::unlimited(),
            status: TaskStatus::Running,
            assigned_agent: Some(agent_id),
            reviewer_agent: None,
            claimed_at: None,
            claim_source: None,
            attempt: 0,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            worktree: None,
            branch: None,
            result_artifact: None,
            taskboard_id: None,
            workflow_id: None,
            parent_task_id: None,
            child_task_ids: Vec::new(),
            kind: WorkItemKind::Task,
            active_role_node_id: None,
            organization_revision: None,
            rework_limit: DEFAULT_REWORK_LIMIT,
            rework_count: 0,
        });
        snapshot.terminals.push(TerminalSessionView {
            id: session_id,
            task_id,
            cwd: "/tmp".into(),
            cols: 80,
            rows: 24,
            active: true,
            lease: Some(ControlLeaseView {
                lease_id: "expired-lease".into(),
                session_id,
                actor: ActorRef {
                    kind: ActorKind::Device,
                    id: Some("device".into()),
                    display_name: Some("laptop".into()),
                },
                expires_at: "1".into(),
            }),
        });
        store.replace_snapshot(&snapshot).await.unwrap();

        orchestrator
            .expire_terminal_leases(&snapshot)
            .await
            .unwrap();

        let after = store.snapshot().await.unwrap();
        assert!(after.terminals[0].lease.is_none());
        assert_eq!(after.agents[0].status, AgentStatus::Idle);
        assert!(
            store
                .events_after(0, 10)
                .await
                .unwrap()
                .events
                .iter()
                .any(|event| matches!(
                    &event.event,
                    Event::TerminalLeaseChanged { lease } if lease.expires_at == "released"
                ))
        );
    }
}
