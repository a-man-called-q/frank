//! Frank's server-owned mission, task, mailbox, approval, budget, and Git
//! workflow state machines.
//!
//! The orchestrator is deliberately headless.  It receives typed commands,
//! validates authorization and transitions, updates a snapshot, and commits a
//! single event through `frank-store`.  GUI clients only observe the resulting
//! event stream; they never mutate projections directly.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine;
mod budget;
pub mod git;
mod helpers;
mod mailbox;
mod scheduler;
mod validate;

pub use budget::{BudgetLedger, UsageTotals};
use helpers::*;
pub use mailbox::Mailbox;
pub use scheduler::{Scheduler, SchedulerLimits};
pub use validate::retry_after_failure;
use validate::*;

pub mod supervisor;

use crate::git::{GitWorkflow, WorktreePlan};
use frank_agent::{ProviderMessage, RuntimeEvent, RuntimeManager, StartRequest, UsageTelemetry};
use frank_protocol::*;
use frank_store::{Store, StoreError};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::process::Command as AsyncCommand;
use tokio::sync::Mutex;

pub const DEFAULT_MAX_ATTEMPTS: u8 = 2;
pub const DEFAULT_MESSAGE_HOP_LIMIT: u8 = 6;
pub const DEFAULT_MAX_CONCURRENCY: usize = 4;
pub const DEFAULT_MAX_PROVIDER_CONCURRENCY: usize = 2;

#[derive(Debug, Clone)]
struct AgentCapability {
    agent_id: AgentId,
    task_id: TaskId,
    issued_at: u64,
    expires_at: u64,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct CloneProjectOperation {
    url: String,
    destination: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct CreateWorktreeOperation {
    project_id: ProjectId,
    mission_id: MissionId,
    mission_branch: String,
    mission_base: String,
    mission_path: String,
    task_id: TaskId,
    task_branch: String,
    task_base: String,
    task_path: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct WriteMemoryOperation {
    agent_id: AgentId,
    path: String,
    content: String,
}

/// Metadata for a host update operation.  The operation deliberately stores
/// only stable identifiers and an action; filesystem locations are resolved
/// from the daemon's environment at execution time so snapshots never leak
/// updater paths to remote clients.  Keeping the action in the journal makes
/// apply and rollback idempotent across a daemon restart.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct HostUpdateOperation {
    update_id: UpdateId,
    action: HostUpdateAction,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct StageUpdateOperation {
    update_id: UpdateId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum HostUpdateAction {
    Apply,
    Rollback,
}

#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("store error: {0}")]
    Store(#[from] StoreError),
    #[error("invalid state transition: {0}")]
    InvalidTransition(String),
    #[error("validation failed: {0}")]
    Validation(String),
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
    pub store: Store,
    pub runtime: RuntimeManager,
    pub scheduler: Arc<Mutex<Scheduler>>,
    pub mailbox: Arc<Mutex<Mailbox>>,
    pub budgets: Arc<Mutex<BudgetLedger>>,
    pub max_message_bytes: usize,
    pub memory_root: PathBuf,
    /// Live provider sessions are owned by the daemon and keyed by task. The
    /// map is deliberately not part of the persisted snapshot; provider
    /// session IDs in the agent projection are used for resume after restart.
    pub sessions: Arc<Mutex<HashMap<TaskId, Arc<frank_agent::RuntimeSession>>>>,
    /// Live supervisor processes are keyed by mission. The stable provider
    /// session ID is persisted on `MissionView`; this map only prevents two
    /// daemon reconciler ticks from spawning duplicate supervisors.
    pub supervisor_sessions: Arc<Mutex<HashMap<MissionId, Arc<frank_agent::RuntimeSession>>>>,
    /// Monotonic-enough wall-clock anchors for active budget scopes. These
    /// are kept outside the wire snapshot because timestamps are only used
    /// for a live hard-stop; token/cost/turn accounting remains durable in
    /// `Snapshot::usage` and is rebuilt on restart.
    scope_started_at: Arc<Mutex<HashMap<String, u64>>>,
    agent_capabilities: Arc<Mutex<HashMap<String, AgentCapability>>>,
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
        }
    }

    pub fn with_runtime(store: Store, runtime: RuntimeManager) -> Self {
        Self {
            runtime,
            ..Self::new(store)
        }
    }

    pub async fn snapshot(&self) -> Result<Snapshot> {
        Ok(self.store.snapshot().await?)
    }

    /// Seed the persistent Frank supervisor profile.  The profile is always
    /// present, but `server.supervisor_provider` remains `None` until
    /// onboarding explicitly chooses Codex or Claude for future missions.
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
                display_name: "Frank supervisor".to_string(),
                template: AgentTemplate::Generalist,
                provider: Provider::Codex,
                model: None,
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
                status: AgentStatus::Idle,
                provider_session_id: None,
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
        // Validate optimistic concurrency before reducing the command. Some
        // reductions allocate worktrees or prepare delivery state as
        // daemon-owned side effects, so a stale command must not leave those
        // resources behind. The transactional store repeats this check at
        // commit time for concurrent races.
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

    async fn respond_to_provider_approval(
        &self,
        approval_id: ApprovalId,
        decision: ApprovalDecision,
    ) {
        let Ok(snapshot) = self.store.snapshot().await else {
            return;
        };
        let Some(approval) = snapshot
            .approvals
            .iter()
            .find(|approval| approval.id == approval_id)
            .cloned()
        else {
            return;
        };
        if !matches!(
            (approval.status, decision),
            (ApprovalStatus::Approved, ApprovalDecision::AllowOnce)
                | (ApprovalStatus::Denied, ApprovalDecision::DenyOnce)
        ) {
            return;
        }
        let session = self.sessions.lock().await.get(&approval.task_id).cloned();
        if let Some(session) = session {
            let _ = session
                .respond_to_approval(&approval.operation, decision)
                .await;
        }
    }

    async fn pause_for_terminal(&self, session_id: TerminalSessionId) -> Result<()> {
        let snapshot = self.store.snapshot().await?;
        let Some(session) = snapshot
            .terminals
            .iter()
            .find(|session| session.id == session_id)
        else {
            return Ok(());
        };
        let task_id = session.task_id;
        let Some(task) = snapshot.tasks.iter().find(|task| task.id == task_id) else {
            return Ok(());
        };
        if let Some(provider) = task
            .assigned_agent
            .and_then(|agent_id| snapshot.agents.iter().find(|agent| agent.id == agent_id))
            .map(|agent| agent.provider)
        {
            let provider_session = { self.sessions.lock().await.remove(&task_id) };
            if let Some(provider_session) = provider_session {
                let _ = provider_session.graceful_stop().await;
            }
            self.scheduler.lock().await.finish(task_id, provider);
        }
        if let Some(capability) = self
            .agent_capabilities
            .lock()
            .await
            .iter()
            .find(|(_, capability)| capability.task_id == task_id)
            .map(|(token, _)| token.clone())
        {
            self.revoke_agent_capability(&capability).await;
        }
        if let Some(agent_id) = task.assigned_agent {
            let _ = self
                .clear_agent_session(agent_id, AgentStatus::Paused)
                .await;
        }
        Ok(())
    }

    async fn resume_after_terminal(&self, session_id: TerminalSessionId) -> Result<()> {
        let snapshot = self.store.snapshot().await?;
        let Some(session) = snapshot
            .terminals
            .iter()
            .find(|session| session.id == session_id)
        else {
            return Ok(());
        };
        let Some(task) = snapshot
            .tasks
            .iter()
            .find(|task| task.id == session.task_id)
        else {
            return Ok(());
        };
        if let Some(agent_id) = task.assigned_agent {
            let _ = self.set_agent(agent_id, AgentStatus::Idle, None).await;
        }
        // The daemon reconciler observes the still-running task on its next
        // tick and starts a fresh provider session after the lease is
        // released. Avoid recursively calling reconcile from execute.
        Ok(())
    }

    pub async fn plan_mission(
        &self,
        mission_id: MissionId,
        objective: &str,
    ) -> Result<Vec<TaskId>> {
        // CreateMission is idempotent at the storage layer, but its
        // deterministic task expansion happens after that commit. A client
        // retry must therefore short-circuit here as well or it would append
        // a second copy of the same DAG after the original response was
        // already durably recorded.
        let existing = self
            .store
            .snapshot()
            .await?
            .tasks
            .into_iter()
            .filter(|task| task.mission_id == mission_id)
            .map(|task| task.id)
            .collect::<Vec<_>>();
        if !existing.is_empty() {
            return Ok(existing);
        }
        let plan = if std::env::var_os("FRANK_DETERMINISTIC_SUPERVISOR").is_some() {
            // This switch exists for deterministic CI/fake-provider runs only.
            // A normal daemon must receive a structured proposal from the
            // selected Codex or Claude supervisor.
            supervisor::decompose_objective(mission_id, objective, 32)
        } else {
            self.request_supervisor_plan(mission_id, objective).await?
        };
        self.materialize_supervisor_plan(plan).await
    }

    async fn materialize_supervisor_plan(
        &self,
        plan: supervisor::SupervisorPlan,
    ) -> Result<Vec<TaskId>> {
        let mut ids = Vec::with_capacity(plan.tasks.len());
        let mut root_tasks = Vec::new();
        let mut ids_by_key = HashMap::<String, TaskId>::new();
        for (index, mut spec) in plan.tasks.into_iter().enumerate() {
            let key = plan
                .task_keys
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("task-{index}"));
            let dependency_keys = plan.dependency_keys.get(index).cloned().unwrap_or_default();
            spec.dependencies = dependency_keys
                .iter()
                .map(|dependency| {
                    ids_by_key.get(dependency).copied().ok_or_else(|| {
                        OrchestratorError::Validation(
                            "supervisor plan dependency was not materialized".into(),
                        )
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let is_root = spec.dependencies.is_empty();
            // Use the public daemon command path so every generated task is
            // durably committed. The old planner called `apply()` directly,
            // which built an in-memory card but never persisted it.
            let response = self
                .commit_reduced(
                    CommandEnvelope {
                        protocol_version: PROTOCOL_VERSION,
                        command_id: CommandId::new(),
                        expected_revision: None,
                        command: Command::CreateTask(spec),
                    },
                    ActorRef::system(),
                )
                .await?;
            if let Some(CommandResult::Created { id }) = response.result {
                if let Ok(id) = TaskId::parse(&id) {
                    if is_root {
                        root_tasks.push(id);
                    }
                    ids.push(id);
                    ids_by_key.insert(key, id);
                }
            } else if let Some(error) = response.error {
                return Err(match error.code {
                    ErrorCode::NotFound => OrchestratorError::NotFound,
                    _ => OrchestratorError::Validation(error.message),
                });
            }
        }
        // A generated supervisor DAG starts at Ready for dependency-free
        // tasks. The reconciler will assign a worker and move it to Running
        // once its mission is active; manually-created cards remain Backlog.
        for task_id in root_tasks {
            let _ = self
                .commit_reduced(
                    CommandEnvelope {
                        protocol_version: PROTOCOL_VERSION,
                        command_id: CommandId::new(),
                        expected_revision: None,
                        command: Command::SetTaskStatus {
                            task_id,
                            status: TaskStatus::Ready,
                        },
                    },
                    ActorRef::system(),
                )
                .await?;
        }
        Ok(ids)
    }

    /// Commit a reducer result for an internal plan materialization step.
    /// This deliberately does not call `execute`, avoiding recursive async
    /// futures through the CreateMission post-hook.
    async fn commit_reduced(
        &self,
        envelope: CommandEnvelope,
        actor: ActorRef,
    ) -> Result<CommandResponse> {
        let command_id = envelope.command_id;
        // Internal daemon transitions normally omit an expected revision. A
        // concurrent client/event can still advance SQLite between the read
        // and commit, so bind the reduction to the revision we observed and
        // retry a bounded number of times. Reusing the same command id keeps
        // a successful attempt idempotent if the caller is interrupted after
        // commit but before receiving the response.
        for _ in 0..4 {
            let snapshot = self.store.snapshot().await?;
            let expected_revision = envelope.expected_revision.or(Some(snapshot.revision));
            let (next, event, result) = self
                .reduce(snapshot, envelope.command.clone(), &actor)
                .await?;
            match self
                .store
                .commit_command(
                    command_id,
                    expected_revision,
                    actor.clone(),
                    event,
                    next,
                    result,
                )
                .await
            {
                Ok(commit) => return Ok(commit.response),
                Err(StoreError::StaleRevision { .. }) if envelope.expected_revision.is_none() => {
                    continue;
                }
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }

    /// Start the selected provider as a persistent mission supervisor and
    /// require a structured plan proposal.  The daemon still validates the
    /// proposal before it becomes a task DAG; provider text is never treated
    /// as an implicit mutation.
    async fn request_supervisor_plan(
        &self,
        mission_id: MissionId,
        objective: &str,
    ) -> Result<supervisor::SupervisorPlan> {
        let snapshot = self.store.snapshot().await?;
        let mission = snapshot
            .missions
            .iter()
            .find(|mission| mission.id == mission_id)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        let project = snapshot
            .projects
            .iter()
            .find(|project| project.id == mission.project_id && !project.archived)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        let supervisor = snapshot
            .agents
            .iter()
            .find(|agent| agent.display_name == "Frank supervisor" && !agent.archived)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        let request = StartRequest {
            agent_id: supervisor.id.to_string(),
            task_id: None,
            cwd: project.path,
            instructions: supervisor.instructions.clone(),
            policy: supervisor.policy.clone(),
            model: supervisor.model.clone(),
            resume_session_id: mission.supervisor_session_id.clone(),
            server_url: local_server_url(&snapshot.server),
            server_certificate_fingerprint: (!snapshot.server.tls_fingerprint.is_empty())
                .then(|| snapshot.server.tls_fingerprint.clone()),
            session_capability: None,
        };
        let session = if let Some(session) = self.supervisor_sessions.lock().await.get(&mission_id)
        {
            session.clone()
        } else {
            let session = if request.resume_session_id.is_some() {
                self.runtime
                    .resume(
                        mission.supervisor_provider,
                        request.clone(),
                        request.resume_session_id.as_deref().unwrap_or_default(),
                    )
                    .await
            } else {
                self.runtime
                    .start(mission.supervisor_provider, request.clone())
                    .await
            }
            .map_err(|error| OrchestratorError::ProviderUnavailable(error.to_string()))?;
            let session = Arc::new(session);
            self.supervisor_sessions
                .lock()
                .await
                .insert(mission_id, session.clone());
            session
        };
        let mut events = session
            .events()
            .await
            .map_err(|error| OrchestratorError::ProviderUnavailable(error.to_string()))?;
        let prompt = format!(
            "You are Frank's mission supervisor. Decompose this objective into a safe DAG.\n\
             Return ONLY one JSON object matching SupervisorPlanProposal: \
             {{\"mission_id\":\"{mission_id}\",\"summary\":\"...\",\"tasks\":[{{\"client_key\":\"stable-key\",\"title\":\"...\",\"objective\":\"...\",\"dependencies\":[],\"priority\":0,\"candidate_agents\":[],\"assigned_agent\":null,\"policy_requirement\":null,\"budget\":{{\"time_seconds\":null,\"turns\":null,\"measured_tokens\":null,\"cost_micros\":null}}}}]}}.\n\
             Maximum 32 tasks. Use dependency client_key values only. Objective:\n{objective}",
        );
        session
            .send(&ProviderMessage {
                role: "user".into(),
                content: prompt,
                correlation_id: Some(mission_id.to_string()),
            })
            .await
            .map_err(|error| OrchestratorError::ProviderUnavailable(error.to_string()))?;

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(45);
        let mut text = String::new();
        let mut proposal = None;
        while tokio::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let next = tokio::time::timeout(
                remaining.min(std::time::Duration::from_secs(2)),
                events.recv(),
            )
            .await;
            let Some(event) = (match next {
                Ok(event) => event,
                Err(_) => continue,
            }) else {
                break;
            };
            match event {
                RuntimeEvent::Ready {
                    provider_session_id,
                } => {
                    self.persist_supervisor_session_id(mission_id, Some(provider_session_id))
                        .await?;
                }
                RuntimeEvent::Text { text: chunk } => {
                    if text.len().saturating_add(chunk.len()) <= MAX_MESSAGE_BODY_BYTES {
                        text.push_str(&chunk);
                    }
                    if let Some(parsed) = parse_supervisor_json(&text, mission_id) {
                        proposal = Some(parsed);
                        break;
                    }
                }
                RuntimeEvent::ToolCall { input, .. } => {
                    if let Ok(parsed) = serde_json::from_value::<SupervisorPlanProposal>(input)
                        && parsed.mission_id == mission_id
                    {
                        proposal = Some(parsed);
                        break;
                    }
                }
                RuntimeEvent::Error { message } => {
                    return Err(OrchestratorError::ProviderUnavailable(message));
                }
                RuntimeEvent::Stopped { code } => {
                    return Err(OrchestratorError::ProviderUnavailable(format!(
                        "supervisor exited before returning a plan ({code:?})"
                    )));
                }
                RuntimeEvent::Usage(_)
                | RuntimeEvent::Raw(_)
                | RuntimeEvent::ApprovalRequest { .. } => {}
            }
        }
        let plan = proposal
            .ok_or_else(|| {
                OrchestratorError::ProviderUnavailable(
                    "supervisor did not return a valid structured plan".into(),
                )
            })
            .and_then(|proposal| {
                validate_supervisor_proposal(&snapshot, mission_id, &proposal)?;
                supervisor::proposal_to_plan(proposal).map_err(OrchestratorError::Validation)
            })?;

        // Keep consuming the persistent supervisor stream after the initial
        // plan so EOF/crash is observable and the child is removed from the
        // live map. Subsequent user chat can be routed through the same map.
        let supervisors = self.supervisor_sessions.clone();
        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                if matches!(event, RuntimeEvent::Stopped { .. }) {
                    break;
                }
            }
            supervisors.lock().await.remove(&mission_id);
        });
        Ok(plan)
    }

    async fn persist_supervisor_session_id(
        &self,
        mission_id: MissionId,
        provider_session_id: Option<String>,
    ) -> Result<()> {
        // Session readiness is emitted by a provider reader task while GUI
        // commands may update the same mission concurrently. Bind the
        // projection write to the revision we observed and retry a bounded
        // number of times; committing an unguarded stale snapshot here could
        // silently erase a just-created task or budget edit.
        for _ in 0..4 {
            let mut snapshot = self.store.snapshot().await?;
            let expected_revision = snapshot.revision;
            let mission = snapshot
                .missions
                .iter_mut()
                .find(|mission| mission.id == mission_id)
                .ok_or(OrchestratorError::NotFound)?;
            mission.supervisor_session_id = provider_session_id.clone();
            mission.updated_at = timestamp_now();
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(expected_revision),
                    ActorRef::system(),
                    Event::MissionSupervisorSessionChanged {
                        mission_id,
                        provider_session_id: provider_session_id.clone(),
                    },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await
            {
                Ok(_) => return Ok(()),
                Err(StoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }

    /// Issue a short-lived capability for a provider child. The server uses
    /// the same registry to authorize the MCP bridge, so a copied device
    /// token cannot be used to mutate another task through a provider tool.
    pub async fn issue_agent_capability(&self, agent_id: AgentId, task_id: TaskId) -> String {
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
        let issued_at = epoch_seconds();
        let expires_at = issued_at.saturating_add(900);
        self.agent_capabilities.lock().await.insert(
            token.clone(),
            AgentCapability {
                agent_id,
                task_id,
                issued_at,
                expires_at,
            },
        );
        let capability_hash = hash_capability(&token);
        let _ = self
            .store
            .upsert_agent_capability(&frank_store::StoredAgentCapability {
                capability_hash,
                agent_id,
                task_id,
                issued_at,
                expires_at,
                revoked: false,
            })
            .await;
        token
    }

    pub async fn validate_agent_capability(
        &self,
        token: &str,
        agent_id: AgentId,
        task_id: TaskId,
    ) -> bool {
        let expired = {
            let mut capabilities = self.agent_capabilities.lock().await;
            let Some(capability) = capabilities.get(token) else {
                return false;
            };
            if capability.expires_at < epoch_seconds() {
                capabilities.remove(token);
                true
            } else {
                return capability.agent_id == agent_id && capability.task_id == task_id;
            }
        };
        if expired {
            let _ = self
                .store
                .revoke_agent_capability(&hash_capability(token))
                .await;
        }
        false
    }

    pub async fn agent_capability_actor(&self, token: &str) -> Option<(AgentId, TaskId)> {
        let (actor, expired) = {
            let mut capabilities = self.agent_capabilities.lock().await;
            let capability = capabilities.get(token)?;
            if capability.expires_at < epoch_seconds() {
                capabilities.remove(token);
                (None, true)
            } else {
                (Some((capability.agent_id, capability.task_id)), false)
            }
        };
        if expired {
            let _ = self
                .store
                .revoke_agent_capability(&hash_capability(token))
                .await;
        }
        actor
    }

    pub async fn revoke_agent_capability(&self, token: &str) {
        self.agent_capabilities.lock().await.remove(token);
        let _ = self
            .store
            .revoke_agent_capability(&hash_capability(token))
            .await;
    }

    /// Extend capabilities for sessions that are still alive. The bearer
    /// token never changes, so a provider's MCP process can keep operating
    /// while the daemon renews the durable expiry metadata. Capabilities are
    /// revoked by the existing task/session cleanup paths and on expiry.
    async fn renew_agent_capabilities(&self) {
        let now = epoch_seconds();
        let renew_before = now.saturating_add(300);
        let renewals = {
            let mut capabilities = self.agent_capabilities.lock().await;
            let mut renewals = Vec::new();
            for (token, capability) in capabilities.iter_mut() {
                if capability.expires_at <= renew_before {
                    capability.issued_at = now;
                    capability.expires_at = now.saturating_add(900);
                    renewals.push((
                        hash_capability(token),
                        capability.agent_id,
                        capability.task_id,
                        capability.issued_at,
                        capability.expires_at,
                    ));
                }
            }
            renewals
        };
        for (capability_hash, agent_id, task_id, issued_at, expires_at) in renewals {
            let _ = self
                .store
                .upsert_agent_capability(&frank_store::StoredAgentCapability {
                    capability_hash,
                    agent_id,
                    task_id,
                    issued_at,
                    expires_at,
                    revoked: false,
                })
                .await;
        }
    }

    /// Enforce wall-clock budgets even when a provider has not emitted a
    /// usage frame.  The start time is persisted in `budget_clocks`, so a
    /// daemon restart cannot reset an active task's deadline.  Token and
    /// cost budgets remain telemetry-driven; this watchdog only handles the
    /// explicit `time_seconds` limit.
    async fn enforce_time_budgets(&self) -> Result<()> {
        let snapshot = self.store.snapshot().await?;
        let mut expired_missions = HashSet::new();
        for mission in snapshot
            .missions
            .iter()
            .filter(|mission| mission.status == MissionStatus::Active)
        {
            if self
                .scope_time_expired(&mission.id.to_string(), &mission.budget)
                .await?
            {
                expired_missions.insert(mission.id);
            }
        }

        // A task is blocked once for the first expired scope.  Mission scope
        // wins over task/agent scope so all workers of an expired mission are
        // stopped consistently and the mission itself is paused exactly once.
        let mut expired_tasks = HashMap::<TaskId, (BudgetScope, String)>::new();
        for task in snapshot
            .tasks
            .iter()
            .filter(|task| task.status == TaskStatus::Running)
        {
            if expired_missions.contains(&task.mission_id) {
                expired_tasks.insert(
                    task.id,
                    (
                        BudgetScope::Mission,
                        "mission wall-clock budget exceeded".to_string(),
                    ),
                );
                continue;
            }
            if self
                .scope_time_expired(&task.id.to_string(), &task.budget)
                .await?
            {
                expired_tasks.insert(
                    task.id,
                    (
                        BudgetScope::Task,
                        "task wall-clock budget exceeded".to_string(),
                    ),
                );
                continue;
            }
            let Some(agent_id) = task.assigned_agent else {
                continue;
            };
            let Some(agent) = snapshot.agents.iter().find(|agent| agent.id == agent_id) else {
                continue;
            };
            if self
                .scope_time_expired(&agent.id.to_string(), &agent.budget)
                .await?
            {
                expired_tasks.insert(
                    task.id,
                    (
                        BudgetScope::Agent,
                        "agent wall-clock budget exceeded".to_string(),
                    ),
                );
            }
        }

        for mission_id in expired_missions {
            let task_ids = expired_tasks
                .iter()
                .filter_map(|(task_id, (scope, _))| {
                    (*scope == BudgetScope::Mission).then_some(*task_id)
                })
                .collect::<Vec<_>>();
            for task_id in task_ids {
                self.stop_task_for_budget(task_id).await?;
            }
            let response = self
                .execute(
                    CommandEnvelope {
                        protocol_version: PROTOCOL_VERSION,
                        command_id: CommandId::new(),
                        expected_revision: None,
                        command: Command::PauseMission { mission_id },
                    },
                    ActorRef::system(),
                    DeviceRole::Owner,
                )
                .await;
            if let Some(error) = response.error
                && !matches!(error.code, ErrorCode::Conflict | ErrorCode::StaleRevision)
            {
                return Err(OrchestratorError::Validation(error.message));
            }
            self.emit_budget_pause(
                BudgetScope::Mission,
                "mission wall-clock budget exceeded; mission paused",
            )
            .await?;
        }

        let mut emitted = HashSet::<String>::new();
        for (task_id, (scope, reason)) in expired_tasks {
            // Mission-scoped tasks were already stopped and audited above.
            if scope == BudgetScope::Mission {
                continue;
            }
            self.stop_task_for_budget(task_id).await?;
            if emitted.insert(format!("{scope:?}")) {
                self.emit_budget_pause(scope, &reason).await?;
            }
        }
        Ok(())
    }

    async fn scope_time_expired(&self, scope_id: &str, budget: &Budget) -> Result<bool> {
        let Some(limit) = budget.time_seconds else {
            return Ok(false);
        };
        let Some((started_at, persisted_deadline)) = self.store.budget_clock(scope_id).await?
        else {
            return Ok(false);
        };
        // Recompute from the current budget as well as the persisted deadline:
        // an explicit budget adjustment may shorten or extend an existing
        // clock without resetting the original start time.
        let deadline = started_at.saturating_add(limit);
        let deadline = persisted_deadline.map_or(deadline, |stored| {
            // Keep a deadline written by an older daemon only when it is
            // stricter.  This prevents a restart during a budget adjustment
            // from accidentally widening a limit that was already active.
            stored.min(deadline)
        });
        Ok(epoch_seconds() >= deadline)
    }

    /// Stop one running worker before transitioning its card to `Blocked`.
    /// Provider/session cleanup is deliberately best-effort after the durable
    /// budget decision; the task state is still blocked if a child has
    /// already crashed or disappeared.
    async fn stop_task_for_budget(&self, task_id: TaskId) -> Result<()> {
        let snapshot = self.store.snapshot().await?;
        let Some(task) = snapshot
            .tasks
            .iter()
            .find(|task| task.id == task_id && task.status == TaskStatus::Running)
            .cloned()
        else {
            return Ok(());
        };
        if let Some(agent_id) = task.assigned_agent {
            if let Some(agent) = snapshot.agents.iter().find(|agent| agent.id == agent_id) {
                if let Some(session) = self.sessions.lock().await.remove(&task_id) {
                    let _ = session.graceful_stop().await;
                }
                self.scheduler.lock().await.finish(task_id, agent.provider);
            }
            let capabilities = self
                .agent_capabilities
                .lock()
                .await
                .iter()
                .filter(|(_, capability)| capability.task_id == task_id)
                .map(|(token, _)| token.clone())
                .collect::<Vec<_>>();
            for token in capabilities {
                self.revoke_agent_capability(&token).await;
            }
            self.clear_agent_session(agent_id, AgentStatus::Paused)
                .await?;
        }
        // A concurrent manual move/cancel may have won the race. In that
        // case the authoritative state is already safe and no extra error is
        // needed from the reconciliation loop.
        let response = self
            .execute(
                CommandEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    command_id: CommandId::new(),
                    expected_revision: None,
                    command: Command::SetTaskStatus {
                        task_id,
                        status: TaskStatus::Blocked,
                    },
                },
                ActorRef::system(),
                DeviceRole::Owner,
            )
            .await;
        if let Some(error) = response.error
            && !matches!(error.code, ErrorCode::Conflict | ErrorCode::StaleRevision)
        {
            return Err(OrchestratorError::Validation(error.message));
        }
        Ok(())
    }

    async fn emit_budget_pause(&self, scope: BudgetScope, reason: &str) -> Result<()> {
        let command_id = CommandId::new();
        for _ in 0..4 {
            let snapshot = self.store.snapshot().await?;
            match self
                .store
                .commit_command(
                    command_id,
                    Some(snapshot.revision),
                    ActorRef::system(),
                    Event::BudgetPaused {
                        scope,
                        reason: reason.to_string(),
                    },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await
            {
                Ok(_) => return Ok(()),
                Err(StoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }

    /// Reconcile durable task state with daemon-owned provider sessions. This
    /// loop is safe to run repeatedly and after a frankd restart: projections
    /// are authoritative, while the in-memory session map only prevents
    /// duplicate child processes during the current daemon lifetime.
    pub async fn reconcile(&self) -> Result<()> {
        self.renew_agent_capabilities().await;
        let snapshot = self.store.snapshot().await?;
        // A daemon can be interrupted after committing MessageQueued but
        // before the in-process broker advances it to Delivered.  Rebuild the
        // delivery work from the authoritative snapshot so queued messages
        // never depend on the lifetime of the HTTP request that created them.
        self.deliver_queued_messages(&snapshot).await?;
        self.expire_approvals(&snapshot).await?;
        self.expire_terminal_leases(&snapshot).await?;
        self.expire_pending_messages(&snapshot).await?;
        // Time limits are independent of provider telemetry.  A provider can
        // be quiet (or crash before emitting a usage frame), so the durable
        // SQLite clock must still stop work when its deadline elapses.
        self.enforce_time_budgets().await?;
        self.recover_running_worktree_intents().await?;
        self.reconcile_operations().await?;
        let snapshot = self.store.snapshot().await?;
        self.budgets.lock().await.rebuild_from_snapshot(&snapshot);
        {
            let mut scheduler = self.scheduler.lock().await;
            scheduler.limits.max_concurrency = snapshot.server.max_concurrency.max(1) as usize;
            scheduler.limits.max_provider_concurrency =
                snapshot.server.max_provider_concurrency.max(1) as usize;
        }

        // CreateMission commits before supervisor planning so the HTTP
        // command remains idempotent.  If frankd is killed in that window,
        // the mission is durable but has no DAG.  Re-run planning on startup
        // for those draft missions; plan_mission is itself idempotent and
        // will never append a second DAG when a prior attempt completed.
        let unplanned_drafts = snapshot
            .missions
            .iter()
            .filter(|mission| {
                mission.status == MissionStatus::Draft
                    && !snapshot
                        .tasks
                        .iter()
                        .any(|task| task.mission_id == mission.id)
            })
            .map(|mission| (mission.id, mission.objective.clone()))
            .collect::<Vec<_>>();
        for (mission_id, objective) in unplanned_drafts {
            if let Err(error) = self.plan_mission(mission_id, &objective).await {
                // Draft has no legal blocked transition. Keep the mission
                // visible so the owner can retry after fixing provider or
                // runtime health. The provider error is intentionally not
                // copied into the wire snapshot because it may contain a
                // local executable path; the next doctor/mission attempt
                // exposes a sanitized diagnostic.
                let _ = error;
            }
        }
        let snapshot = self.store.snapshot().await?;
        let active_missions = snapshot
            .missions
            .iter()
            .filter(|mission| mission.status == MissionStatus::Active)
            .map(|mission| mission.id)
            .collect::<HashSet<_>>();

        // Promote dependency-complete cards to Ready. A supervisor-generated
        // DAG already does this for roots; this branch covers later children
        // and cards created manually in the Board.
        for task in snapshot.tasks.iter().filter(|task| {
            task.status == TaskStatus::Backlog
                && active_missions.contains(&task.mission_id)
                && task.dependencies.iter().all(|dependency| {
                    snapshot
                        .tasks
                        .iter()
                        .find(|candidate| candidate.id == *dependency)
                        .is_some_and(|candidate| candidate.status == TaskStatus::Done)
                })
        }) {
            let _ = self
                .execute(
                    CommandEnvelope {
                        protocol_version: PROTOCOL_VERSION,
                        command_id: CommandId::new(),
                        expected_revision: None,
                        command: Command::SetTaskStatus {
                            task_id: task.id,
                            status: TaskStatus::Ready,
                        },
                    },
                    ActorRef::system(),
                    DeviceRole::Owner,
                )
                .await;
        }

        // A conflict-resolution card is a real task. Once its worker is
        // accepted, retry the original review task through the same durable
        // TaskAccept -> CommitTask saga. This keeps a conflict retry from
        // depending on a GUI reconnect and avoids silently marking the parent
        // done merely because the child card completed.
        self.reconcile_conflict_resolutions().await?;

        let snapshot = self.store.snapshot().await?;
        for task in snapshot.tasks.iter().filter(|task| {
            task.status == TaskStatus::Ready && active_missions.contains(&task.mission_id)
        }) {
            let task = match self.ensure_task_assignment(task.id).await {
                Ok(Some(task)) => task,
                Ok(None) => continue,
                Err(_) => continue,
            };
            let response = self
                .execute(
                    CommandEnvelope {
                        protocol_version: PROTOCOL_VERSION,
                        command_id: CommandId::new(),
                        expected_revision: None,
                        command: Command::SetTaskStatus {
                            task_id: task.id,
                            status: TaskStatus::Running,
                        },
                    },
                    ActorRef::system(),
                    DeviceRole::Owner,
                )
                .await;
            if response.error.is_none() {
                let _ = self.start_task_session(task.id).await;
            }
        }

        // A process may have disappeared while the SQLite task remained
        // Running. Attempt a resume using the stable provider session ID.
        let snapshot = self.store.snapshot().await?;
        for task in snapshot
            .tasks
            .iter()
            .filter(|task| task.status == TaskStatus::Running && task.assigned_agent.is_some())
        {
            if !self.sessions.lock().await.contains_key(&task.id) {
                let _ = self.start_task_session(task.id).await;
            }
        }
        Ok(())
    }

    async fn deliver_queued_messages(&self, snapshot: &Snapshot) -> Result<()> {
        let queued = snapshot
            .messages
            .iter()
            .filter(|message| message.delivery == DeliveryStatus::Queued)
            .map(|message| message.id)
            .collect::<Vec<_>>();
        for message_id in queued {
            // A transient provider wake-up failure must not prevent the
            // durable Delivered transition. `deliver_message` itself only
            // returns an error when persistence cannot complete; leave that
            // row queued and let the next reconciler tick retry it.
            if let Err(error) = self.deliver_message(message_id).await
                && !matches!(error, OrchestratorError::Store(_))
            {
                return Err(error);
            }
        }
        Ok(())
    }

    /// Required-reply messages are not allowed to remain acknowledged forever
    /// after a provider crash.  The deadline is derived from the durable
    /// creation timestamp, so a restart cannot reset it.  `Inform` and other
    /// fire-and-forget acts are intentionally excluded.
    async fn expire_pending_messages(&self, snapshot: &Snapshot) -> Result<()> {
        const REPLY_TIMEOUT_SECONDS: u64 = 600;
        let now = epoch_seconds();
        let expired = snapshot
            .messages
            .iter()
            .filter(|message| {
                message.act.requires_reply()
                    && matches!(
                        message.delivery,
                        DeliveryStatus::Delivered | DeliveryStatus::Acknowledged
                    )
                    && message
                        .created_at
                        .parse::<u64>()
                        .ok()
                        .map(|millis| now.saturating_sub(millis / 1_000) >= REPLY_TIMEOUT_SECONDS)
                        .unwrap_or(false)
            })
            .map(|message| message.id)
            .collect::<Vec<_>>();
        for message_id in expired {
            let mut latest = self.store.snapshot().await?;
            let Some(message) = latest
                .messages
                .iter_mut()
                .find(|message| message.id == message_id)
            else {
                continue;
            };
            if !message.act.requires_reply()
                || !matches!(
                    message.delivery,
                    DeliveryStatus::Delivered | DeliveryStatus::Acknowledged
                )
            {
                continue;
            }
            message.delivery = DeliveryStatus::Failed;
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(latest.revision),
                    ActorRef::system(),
                    Event::MessageFailed { message_id },
                    latest,
                    CommandResult::Accepted,
                )
                .await
            {
                Ok(_) | Err(StoreError::StaleRevision { .. }) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    async fn reconcile_conflict_resolutions(&self) -> Result<()> {
        let snapshot = self.store.snapshot().await?;
        let mut parents = Vec::new();
        for child in snapshot.tasks.iter().filter(|task| {
            task.status == TaskStatus::Done
                && task.title.starts_with("Resolve merge conflict for task ")
        }) {
            let prefix = child
                .title
                .strip_prefix("Resolve merge conflict for task ")
                .and_then(|value| value.split_whitespace().next())
                .unwrap_or_default();
            let Ok(parent_id) = TaskId::parse(prefix) else {
                continue;
            };
            let Some(parent) = snapshot.tasks.iter().find(|task| task.id == parent_id) else {
                continue;
            };
            if parent.status != TaskStatus::Review || parent.worktree.is_none() {
                continue;
            }
            let active_operation = snapshot.operations.iter().any(|operation| {
                operation.kind == OperationKind::CommitTask
                    && operation.resource == parent_id.to_string()
                    && matches!(
                        operation.status,
                        OperationStatus::Queued
                            | OperationStatus::Running
                            | OperationStatus::Waiting
                            | OperationStatus::Recovering
                    )
            });
            if !active_operation {
                parents.push(parent_id);
            }
        }
        parents.sort_unstable_by_key(|id| id.to_string());
        parents.dedup();
        for task_id in parents {
            let _ = self
                .execute(
                    CommandEnvelope {
                        protocol_version: PROTOCOL_VERSION,
                        command_id: CommandId::new(),
                        expected_revision: None,
                        command: Command::TaskAccept { task_id },
                    },
                    ActorRef::system(),
                    DeviceRole::Owner,
                )
                .await;
        }
        Ok(())
    }

    /// Older development snapshots could contain a Running card created by
    /// the pre-journal implementation. Reconstruct only the missing intent;
    /// never infer completion from a directory that happens to exist. The
    /// resulting event is the same atomic task+operation projection used for
    /// new transitions, so subsequent reconciliation can safely resume it.
    async fn recover_running_worktree_intents(&self) -> Result<()> {
        let snapshot = self.store.snapshot().await?;
        let missing = snapshot
            .tasks
            .iter()
            .filter(|task| {
                task.status == TaskStatus::Running
                    && !snapshot
                        .operations
                        .iter()
                        .any(|operation| worktree_operation_matches_task(operation, task.id))
            })
            .map(|task| task.id)
            .collect::<Vec<_>>();
        for task_id in missing {
            let mut latest = self.store.snapshot().await?;
            let Some(task_index) = latest.tasks.iter().position(|task| task.id == task_id) else {
                continue;
            };
            if latest.tasks[task_index].status != TaskStatus::Running
                || latest
                    .operations
                    .iter()
                    .any(|operation| worktree_operation_matches_task(operation, task_id))
            {
                continue;
            }
            let task = latest.tasks[task_index].clone();
            let mission = latest
                .missions
                .iter()
                .find(|mission| mission.id == task.mission_id)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            let project = latest
                .projects
                .iter()
                .find(|project| project.id == mission.project_id && !project.archived)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            let workflow = GitWorkflow::new(
                project,
                latest
                    .server
                    .allowed_project_roots
                    .iter()
                    .map(PathBuf::from)
                    .collect(),
            )
            .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            let mission_plan = workflow.branch_plan(&mission.branch, &workflow.project.base_branch);
            let task_plan = workflow.task_plan_from_base(task.id, &mission.branch);
            latest.tasks[task_index].worktree = Some(task_plan.path.to_string_lossy().into_owned());
            latest.tasks[task_index].branch = Some(task_plan.branch.clone());
            let now = timestamp_now();
            let operation = OperationView {
                id: OperationId::new(),
                kind: OperationKind::CreateWorktree,
                status: OperationStatus::Queued,
                resource: serde_json::to_string(&CreateWorktreeOperation {
                    project_id: mission.project_id,
                    mission_id: mission.id,
                    mission_branch: mission_plan.branch,
                    mission_base: mission_plan.base,
                    mission_path: mission_plan.path.to_string_lossy().into_owned(),
                    task_id,
                    task_branch: task_plan.branch,
                    task_base: task_plan.base,
                    task_path: task_plan.path.to_string_lossy().into_owned(),
                })
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?,
                phase: "recovery-queued".into(),
                attempt: 0,
                error: None,
                created_at: now.clone(),
                updated_at: now,
            };
            latest.operations.push(operation.clone());
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(latest.revision),
                    ActorRef::system(),
                    Event::TaskWorktreeProvisioning {
                        task: latest.tasks[task_index].clone(),
                        operation,
                    },
                    latest,
                    CommandResult::Accepted,
                )
                .await
            {
                Ok(_) | Err(StoreError::StaleRevision { .. }) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    async fn reconcile_operations(&self) -> Result<()> {
        let snapshot = self.store.snapshot().await?;
        let operations = snapshot
            .operations
            .iter()
            .filter(|operation| {
                matches!(
                    operation.status,
                    OperationStatus::Queued
                        | OperationStatus::Running
                        | OperationStatus::Recovering
                ) || (operation.status == OperationStatus::Waiting
                    && operation.phase != "awaiting-helper")
            })
            .cloned()
            .collect::<Vec<_>>();
        for operation in operations {
            let lock_resource = operation_lock_resource(&operation);
            let operation_id = operation.id;
            if !self
                .store
                .acquire_operation_lock(&lock_resource, operation_id)
                .await?
            {
                continue;
            }
            let result = match operation.kind {
                OperationKind::CloneProject => self.run_clone_project_operation(operation).await,
                OperationKind::CreateWorktree => {
                    self.run_create_worktree_operation(operation).await
                }
                OperationKind::RunChecks => self.run_checks_operation(operation).await,
                OperationKind::CommitTask => self.run_commit_task_operation(operation).await,
                OperationKind::PushMission => self.run_push_mission_operation(operation).await,
                OperationKind::OpenDraftPullRequest => {
                    self.run_open_draft_pr_operation(operation).await
                }
                OperationKind::WriteMemory => self.run_write_memory_operation(operation).await,
                OperationKind::HostUpdate => self.run_host_update_operation(operation).await,
                OperationKind::StageUpdate => self.run_stage_update_operation(operation).await,
                // These kinds are persisted now so the protocol can report a
                // durable operation, even though their concrete workers are
                // added by their respective checkpoint. Leaving them queued
                // would create an infinite retry loop, so mark the operation
                // explicitly unsupported and surface it in the audit stream.
                _ => {
                    self.finish_operation(
                        operation.id,
                        OperationStatus::Failed,
                        "unsupported operation worker".into(),
                    )
                    .await
                }
            };
            self.store
                .release_operation_lock(&lock_resource, operation_id)
                .await?;
            result?;
        }
        Ok(())
    }

    async fn run_create_worktree_operation(&self, operation: OperationView) -> Result<()> {
        let request: CreateWorktreeOperation =
            serde_json::from_str(&operation.resource).map_err(|_| {
                OrchestratorError::Validation("worktree operation metadata is invalid".into())
            })?;
        let task_id = request.task_id;
        let result = async {
            self.update_operation(
                &operation,
                OperationStatus::Running,
                "mission-worktree",
                None,
            )
            .await?;
            let snapshot = self.store.snapshot().await?;
            let project = snapshot
                .projects
                .iter()
                .find(|project| project.id == request.project_id && !project.archived)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            let workflow = GitWorkflow::new(
                project,
                snapshot
                    .server
                    .allowed_project_roots
                    .iter()
                    .map(PathBuf::from)
                    .collect(),
            )
            .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            workflow
                .create_worktree(&WorktreePlan {
                    branch: request.mission_branch,
                    path: PathBuf::from(request.mission_path),
                    base: request.mission_base,
                })
                .await
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            self.update_operation(&operation, OperationStatus::Running, "task-worktree", None)
                .await?;
            workflow
                .create_worktree(&WorktreePlan {
                    branch: request.task_branch,
                    path: PathBuf::from(request.task_path),
                    base: request.task_base,
                })
                .await
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            Ok::<(), OrchestratorError>(())
        }
        .await;
        match result {
            Ok(()) => {
                self.finish_operation(operation.id, OperationStatus::Succeeded, String::new())
                    .await?;
            }
            Err(error) => {
                let reason = error.to_string();
                self.finish_operation(operation.id, OperationStatus::Failed, reason)
                    .await?;
                // Do not leave a card permanently Running when its durable
                // worktree intent cannot be fulfilled. The failed operation
                // remains available for diagnostics; the task is blocked so
                // an operator can fix the path/repository and explicitly
                // retry it without the scheduler spawning a provider into a
                // missing directory.
                let _ = self.transition_task(task_id, TaskStatus::Blocked).await;
            }
        }
        Ok(())
    }

    async fn run_push_mission_operation(&self, operation: OperationView) -> Result<()> {
        let result = async {
            let mission_id = MissionId::parse(&operation.resource).map_err(|_| {
                OrchestratorError::Validation("delivery operation mission id is invalid".into())
            })?;
            self.update_operation(
                &operation,
                OperationStatus::Running,
                "delivery-started",
                None,
            )
            .await?;
            let snapshot = self.store.snapshot().await?;
            let mission = snapshot
                .missions
                .iter()
                .find(|mission| mission.id == mission_id)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            if mission.status != MissionStatus::Completed
                || snapshot
                    .tasks
                    .iter()
                    .any(|task| task.mission_id == mission_id && task.status != TaskStatus::Done)
            {
                return Err(OrchestratorError::Validation(
                    "all mission tasks must be accepted before delivery".into(),
                ));
            }

            // This event is informational; the operation row remains the
            // idempotency authority. Re-emitting it after a crash is safe,
            // while Git push/PR workers below explicitly detect work already
            // performed before retrying.
            self.store
                .commit_command(
                    CommandId::new(),
                    Some(snapshot.revision),
                    ActorRef::system(),
                    Event::DeliveryStarted { mission_id },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await?;
            self.update_operation(&operation, OperationStatus::Running, "push", None)
                .await?;
            self.complete_delivery(mission_id).await?;
            let snapshot = self.store.snapshot().await?;
            let project = snapshot
                .projects
                .iter()
                .find(|project| project.id == mission.project_id && !project.archived)
                .ok_or(OrchestratorError::NotFound)?;
            if project.pr_policy == PrPolicy::Draft {
                // Queue the PR as a second durable saga before the push
                // operation is marked complete. If frankd dies between these
                // two commits, the retry sees the same PR operation and does
                // not enqueue a duplicate.
                self.queue_draft_pr_operation(mission_id).await?;
            } else {
                self.commit_delivery_completed(mission_id, None).await?;
            }
            Ok::<(), OrchestratorError>(())
        }
        .await;
        match result {
            Ok(()) => {
                self.finish_operation(operation.id, OperationStatus::Succeeded, String::new())
                    .await?;
            }
            Err(error) => {
                let reason = error.to_string();
                // Delivery failures do not discard the local integration
                // worktree. Persist the blocked event before marking the
                // operation failed so the next retry has an explicit audit
                // trail and can be initiated from the UI/CLI.
                if let Some(mission_id) = MissionId::parse(&operation.resource).ok()
                    && let Ok(snapshot) = self.store.snapshot().await
                {
                    let _ = self
                        .store
                        .commit_command(
                            CommandId::new(),
                            Some(snapshot.revision),
                            ActorRef::system(),
                            Event::DeliveryBlocked {
                                mission_id,
                                reason: "push or draft PR delivery failed; local mission work remains intact".into(),
                            },
                            snapshot,
                            CommandResult::Accepted,
                        )
                        .await;
                }
                self.finish_operation(operation.id, OperationStatus::Failed, reason)
                    .await?;
            }
        }
        Ok(())
    }

    async fn run_open_draft_pr_operation(&self, operation: OperationView) -> Result<()> {
        let result = async {
            let mission_id = MissionId::parse(&operation.resource).map_err(|_| {
                OrchestratorError::Validation("draft PR operation mission id is invalid".into())
            })?;
            self.update_operation(&operation, OperationStatus::Running, "draft-pr", None)
                .await?;
            let snapshot = self.store.snapshot().await?;
            let mission = snapshot
                .missions
                .iter()
                .find(|mission| mission.id == mission_id)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            if mission.status != MissionStatus::Completed
                || snapshot
                    .tasks
                    .iter()
                    .any(|task| task.mission_id == mission_id && task.status != TaskStatus::Done)
            {
                return Err(OrchestratorError::Validation(
                    "all mission tasks must be accepted before opening a draft PR".into(),
                ));
            }
            let project = snapshot
                .projects
                .iter()
                .find(|project| project.id == mission.project_id && !project.archived)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            if project.pr_policy != PrPolicy::Draft {
                return Err(OrchestratorError::Validation(
                    "project draft PR policy is disabled".into(),
                ));
            }
            let workflow = GitWorkflow::new(
                project,
                snapshot
                    .server
                    .allowed_project_roots
                    .iter()
                    .map(PathBuf::from)
                    .collect(),
            )
            .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            let draft_pr_url = workflow
                .open_draft_pr(&mission.branch)
                .await
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            self.commit_delivery_completed(mission_id, draft_pr_url)
                .await?;
            Ok::<(), OrchestratorError>(())
        }
        .await;
        match result {
            Ok(()) => {
                self.finish_operation(operation.id, OperationStatus::Succeeded, String::new())
                    .await?;
            }
            Err(error) => {
                let reason = error.to_string();
                if let Some(mission_id) = MissionId::parse(&operation.resource).ok()
                    && let Ok(snapshot) = self.store.snapshot().await
                {
                    let _ = self
                        .store
                        .commit_command(
                            CommandId::new(),
                            Some(snapshot.revision),
                            ActorRef::system(),
                            Event::DeliveryBlocked {
                                mission_id,
                                reason:
                                    "draft PR delivery failed; local mission work remains intact"
                                        .into(),
                            },
                            snapshot,
                            CommandResult::Accepted,
                        )
                        .await;
                }
                self.finish_operation(operation.id, OperationStatus::Failed, reason)
                    .await?;
            }
        }
        Ok(())
    }

    async fn queue_draft_pr_operation(&self, mission_id: MissionId) -> Result<OperationView> {
        for _ in 0..4 {
            let mut snapshot = self.store.snapshot().await?;
            if let Some(operation) = snapshot.operations.iter().find(|operation| {
                operation.kind == OperationKind::OpenDraftPullRequest
                    && operation.resource == mission_id.to_string()
                    && !matches!(operation.status, OperationStatus::Cancelled)
            }) {
                return Ok(operation.clone());
            }
            let now = timestamp_now();
            let operation = OperationView {
                id: OperationId::new(),
                kind: OperationKind::OpenDraftPullRequest,
                status: OperationStatus::Queued,
                resource: mission_id.to_string(),
                phase: "queued".into(),
                attempt: 0,
                error: None,
                created_at: now.clone(),
                updated_at: now,
            };
            snapshot.operations.push(operation.clone());
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(snapshot.revision),
                    ActorRef::system(),
                    Event::OperationChanged {
                        operation: operation.clone(),
                    },
                    snapshot,
                    CommandResult::Operation(operation.clone()),
                )
                .await
            {
                Ok(_) => return Ok(operation),
                Err(StoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }

    async fn commit_delivery_completed(
        &self,
        mission_id: MissionId,
        draft_pr_url: Option<String>,
    ) -> Result<()> {
        for _ in 0..4 {
            let snapshot = self.store.snapshot().await?;
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(snapshot.revision),
                    ActorRef::system(),
                    Event::DeliveryCompleted {
                        mission_id,
                        draft_pr_url: draft_pr_url.clone(),
                    },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await
            {
                Ok(_) => return Ok(()),
                Err(StoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }

    /// Download, verify, and stage a release payload as a durable operation.
    /// The command that requests preparation only records this intent; the
    /// side effect happens after the transaction commits and can be resumed
    /// idempotently by the startup reconciler.
    async fn run_stage_update_operation(&self, operation: OperationView) -> Result<()> {
        let request: StageUpdateOperation =
            serde_json::from_str(&operation.resource).map_err(|_| {
                OrchestratorError::Validation("stage update metadata is invalid".into())
            })?;
        let result = async {
            let update = self
                .store
                .snapshot()
                .await?
                .update
                .filter(|update| update.id == request.update_id)
                .ok_or(OrchestratorError::NotFound)?;
            if !matches!(update.state, UpdateState::Available | UpdateState::Failed) {
                if update.state == UpdateState::Staged {
                    return Ok::<(), OrchestratorError>(());
                }
                return Err(OrchestratorError::InvalidTransition(
                    "update is not available for staging".into(),
                ));
            }
            self.update_operation(&operation, OperationStatus::Running, "manifest", None)
                .await?;
            let manifest = fetch_update_manifest()
                .await
                .map_err(OrchestratorError::Validation)?;
            if manifest.frank_version != update.version
                || !manifest.accepts_protocol(PROTOCOL_VERSION)
                || manifest
                    .rejects_downgrade_from(env!("CARGO_PKG_VERSION"))
                    .is_err()
            {
                return Err(OrchestratorError::Validation(
                    "verified update manifest no longer matches the selected update".into(),
                ));
            }
            let target =
                std::env::var("FRANK_UPDATE_TARGET").unwrap_or_else(|_| current_update_target());
            let package_kind = std::env::var("FRANK_UPDATE_PACKAGE_KIND")
                .unwrap_or_else(|_| current_update_package_kind());
            let artifact =
                select_update_artifact(&manifest, &target, &package_kind).ok_or_else(|| {
                    OrchestratorError::Validation(
                        "update artifact is not available for this host".into(),
                    )
                })?;
            if artifact.size != update.size || !artifact.sha256.eq_ignore_ascii_case(&update.sha256)
            {
                return Err(OrchestratorError::Validation(
                    "update artifact metadata changed since the check".into(),
                ));
            }
            self.update_operation(&operation, OperationStatus::Running, "download", None)
                .await?;
            let bytes = download_update_artifact(artifact).await?;
            self.update_operation(&operation, OperationStatus::Running, "stage", None)
                .await?;
            let staging_root = update_staging_root(&self.store);
            let staged = frank_update::stage_artifact(artifact, &bytes, &staging_root)
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            if !staged.is_file() {
                return Err(OrchestratorError::Validation(
                    "staged update payload is missing".into(),
                ));
            }
            self.set_update_state(request.update_id, UpdateState::Staged, None)
                .await?;
            Ok::<(), OrchestratorError>(())
        }
        .await;
        match result {
            Ok(()) => {
                self.finish_operation(operation.id, OperationStatus::Succeeded, String::new())
                    .await?;
            }
            Err(error) => {
                let reason = error.to_string();
                self.set_update_state(request.update_id, UpdateState::Failed, Some(reason.clone()))
                    .await?;
                self.finish_operation(operation.id, OperationStatus::Failed, reason)
                    .await?;
            }
        }
        Ok(())
    }

    /// Execute a staged host update through the detached helper binary.  The
    /// daemon never replaces its own files directly: the helper owns the
    /// directory-level swap and keeps the previous bundle for rollback.  All
    /// paths come from the host service environment rather than the wire
    /// snapshot, and missing helper configuration is a durable waiting state
    /// that an owner can resume with `RetryOperation` after configuring it.
    async fn run_host_update_operation(&self, operation: OperationView) -> Result<()> {
        let request: HostUpdateOperation = serde_json::from_str(&operation.resource)
            .map_err(|_| OrchestratorError::Validation("host update metadata is invalid".into()))?;
        let update = self
            .store
            .snapshot()
            .await?
            .update
            .filter(|update| update.id == request.update_id)
            .ok_or(OrchestratorError::NotFound)?;
        let missing = ["FRANK_UPDATE_CURRENT", "FRANK_UPDATE_PREVIOUS"]
            .iter()
            .filter(|name| std::env::var_os(name).is_none())
            .copied()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            self.update_operation(
                &operation,
                OperationStatus::Waiting,
                "awaiting-helper",
                Some(format!(
                    "configure updater paths before retrying ({})",
                    missing.join(", ")
                )),
            )
            .await?;
            return Ok(());
        }
        // Keep a consistent SQLite recovery point before the helper can swap
        // the host bundle. The backup path is deterministic per update so a
        // daemon restart/retry reuses the already-verified copy instead of
        // creating a second snapshot or refusing an otherwise safe retry.
        if let Err(error) = self.backup_before_host_update(request.update_id).await {
            return self
                .finish_host_update_failure(
                    &operation,
                    request.update_id,
                    &format!("database backup before update failed: {error}"),
                )
                .await;
        }
        self.update_operation(&operation, OperationStatus::Running, "verify", None)
            .await?;
        let helper = std::env::var_os("FRANK_UPDATER_BIN")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("frank-updater"));
        if helper.components().count() > 1
            && std::fs::symlink_metadata(&helper)
                .map(|metadata| metadata.file_type().is_symlink() || !metadata.is_file())
                .unwrap_or(true)
        {
            return self
                .finish_host_update_failure(
                    &operation,
                    request.update_id,
                    "configured updater helper is not a regular file",
                )
                .await;
        }
        let current = std::env::var_os("FRANK_UPDATE_CURRENT")
            .map(PathBuf::from)
            .ok_or_else(|| {
                OrchestratorError::Validation("FRANK_UPDATE_CURRENT is missing".into())
            })?;
        let previous = std::env::var_os("FRANK_UPDATE_PREVIOUS")
            .map(PathBuf::from)
            .ok_or_else(|| {
                OrchestratorError::Validation("FRANK_UPDATE_PREVIOUS is missing".into())
            })?;
        let mut command = AsyncCommand::new(helper);
        match request.action {
            HostUpdateAction::Apply => {
                // Prefer an explicitly configured path for package managers
                // that stage bundles outside Frank's data root. Otherwise
                // derive the same deterministic path used by the staging
                // operation, so a daemon restart can resume without a
                // server-path value ever crossing the wire.
                let staged = if let Some(path) = std::env::var_os("FRANK_UPDATE_STAGED") {
                    PathBuf::from(path)
                } else {
                    let artifact = frank_update::UpdateArtifact {
                        target: update.target.clone(),
                        package_kind: current_update_package_kind(),
                        url: "https://invalid.local/frank-update".into(),
                        size: update.size,
                        sha256: update.sha256.clone(),
                    };
                    frank_update::staged_artifact_path(&artifact, update_staging_root(&self.store))
                };
                command.args([
                    "swap",
                    "--staged",
                    &staged.to_string_lossy(),
                    "--current",
                    &current.to_string_lossy(),
                    "--previous",
                    &previous.to_string_lossy(),
                ]);
            }
            HostUpdateAction::Rollback => {
                command.args([
                    "rollback",
                    "--current",
                    &current.to_string_lossy(),
                    "--previous",
                    &previous.to_string_lossy(),
                ]);
            }
        }
        self.update_operation(&operation, OperationStatus::Running, "swap", None)
            .await?;
        command.kill_on_drop(true);
        let output = match tokio::time::timeout(
            std::time::Duration::from_secs(60),
            command.output(),
        )
        .await
        {
            Ok(Ok(output)) => output,
            Ok(Err(error)) => {
                return self
                    .finish_host_update_failure(
                        &operation,
                        request.update_id,
                        &format!("updater helper failed to start: {error}"),
                    )
                    .await;
            }
            Err(_) => {
                return self
                    .finish_host_update_failure(
                        &operation,
                        request.update_id,
                        "updater helper timed out",
                    )
                    .await;
            }
        };
        if output.stdout.len() > MAX_COMMAND_BODY_BYTES
            || output.stderr.len() > MAX_COMMAND_BODY_BYTES
        {
            return self
                .finish_host_update_failure(
                    &operation,
                    request.update_id,
                    "updater helper output exceeded the configured limit",
                )
                .await;
        }
        if !output.status.success() {
            let diagnostic = bounded_text(&[output.stdout, output.stderr].concat());
            return self
                .finish_host_update_failure(
                    &operation,
                    request.update_id,
                    &format!("updater helper exited unsuccessfully: {diagnostic}"),
                )
                .await;
        }
        let state = match request.action {
            HostUpdateAction::Apply => UpdateState::Succeeded,
            HostUpdateAction::Rollback => UpdateState::RolledBack,
        };
        self.set_update_state(request.update_id, state, None)
            .await?;
        self.finish_operation(operation.id, OperationStatus::Succeeded, String::new())
            .await
    }

    async fn finish_host_update_failure(
        &self,
        operation: &OperationView,
        update_id: UpdateId,
        reason: &str,
    ) -> Result<()> {
        let reason = bounded_text(reason.as_bytes());
        self.set_update_state(update_id, UpdateState::Failed, Some(reason.clone()))
            .await?;
        self.finish_operation(operation.id, OperationStatus::Failed, reason)
            .await
    }

    async fn backup_before_host_update(&self, update_id: UpdateId) -> Result<()> {
        let Some(database) = self.store.database_path().map(Path::to_path_buf) else {
            // In-memory stores are used by deterministic unit/fake-provider
            // tests and have no durable file to back up.
            return Ok(());
        };
        let parent = database
            .parent()
            .ok_or_else(|| OrchestratorError::Validation("database path has no parent".into()))?;
        let file_name = database
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| OrchestratorError::Validation("database file name is invalid".into()))?;
        let destination = parent.join(format!(".{file_name}.update-{update_id}.backup"));
        match std::fs::symlink_metadata(&destination) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
                return Err(OrchestratorError::Validation(
                    "database update backup path is not a regular file".into(),
                ));
            }
            Ok(_) => {
                // Validate an existing recovery point before reusing it. A
                // truncated/foreign file must never be treated as a valid
                // backup just because its name matches the update id.
                let backup = Store::open(&destination).await?;
                backup.integrity_check().await?;
                return Ok(());
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(OrchestratorError::Validation(format!(
                    "could not inspect database update backup path: {error}"
                )));
            }
        }
        self.store.backup_to(&destination).await?;
        Ok(())
    }

    async fn set_update_state(
        &self,
        update_id: UpdateId,
        state: UpdateState,
        error: Option<String>,
    ) -> Result<()> {
        for _ in 0..3 {
            let mut snapshot = self.store.snapshot().await?;
            let Some(update) = snapshot
                .update
                .as_mut()
                .filter(|update| update.id == update_id)
            else {
                return Err(OrchestratorError::NotFound);
            };
            update.state = state.clone();
            update.error = error.clone();
            update.checked_at = timestamp_now();
            let update = update.clone();
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(snapshot.revision),
                    ActorRef::system(),
                    Event::UpdateStateChanged { update },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await
            {
                Ok(_) => return Ok(()),
                Err(StoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }

    async fn run_write_memory_operation(&self, operation: OperationView) -> Result<()> {
        let result = async {
            let request: WriteMemoryOperation = serde_json::from_str(&operation.resource)
                .map_err(|_| OrchestratorError::Validation("memory operation is invalid".into()))?;
            self.update_operation(&operation, OperationStatus::Running, "write", None)
                .await?;
            frank_store::memory::MemoryRepository::new(&self.memory_root)
                .propose(request.agent_id, &request.path, &request.content)
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            let snapshot = self.store.snapshot().await?;
            self.store
                .commit_command(
                    CommandId::new(),
                    Some(snapshot.revision),
                    ActorRef::system(),
                    Event::MemoryProposed {
                        agent_id: request.agent_id,
                        path: request.path,
                    },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await?;
            Ok::<(), OrchestratorError>(())
        }
        .await;
        match result {
            Ok(()) => {
                self.finish_operation(operation.id, OperationStatus::Succeeded, String::new())
                    .await?;
            }
            Err(error) => {
                self.finish_operation(operation.id, OperationStatus::Failed, error.to_string())
                    .await?;
            }
        }
        Ok(())
    }

    async fn run_clone_project_operation(&self, operation: OperationView) -> Result<()> {
        let payload: CloneProjectOperation =
            serde_json::from_str(&operation.resource).map_err(|_| {
                OrchestratorError::Validation("clone operation metadata is invalid".into())
            })?;
        let snapshot = self.store.snapshot().await?;
        let allowed_roots = snapshot
            .server
            .allowed_project_roots
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        self.update_operation(&operation, OperationStatus::Running, "clone", None)
            .await?;
        let clone_result = async {
            let destination = Path::new(&payload.destination);
            if destination.exists() {
                // A crash after `git clone` but before projection commit is
                // recovered by validating and registering the existing repo,
                // never by running clone a second time.
                let probe = ProjectView {
                    id: ProjectId::nil(),
                    name: "clone-probe".into(),
                    path: payload.destination.clone(),
                    base_branch: "main".into(),
                    remote: Some("origin".into()),
                    check_commands: Vec::new(),
                    worktree_root: snapshot.server.worktree_root.clone(),
                    push_policy: PushPolicy::MissionBranch,
                    pr_policy: PrPolicy::Draft,
                    archived: false,
                };
                GitWorkflow::new(probe, allowed_roots.clone())
                    .map_err(|error| OrchestratorError::Validation(error.to_string()))?
                    .validate_repository()
                    .await
                    .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            } else {
                GitWorkflow::clone_project(&payload.url, destination, &allowed_roots)
                    .await
                    .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            }
            let probe = ProjectView {
                id: ProjectId::nil(),
                name: "clone-probe".into(),
                path: payload.destination.clone(),
                base_branch: "main".into(),
                remote: Some("origin".into()),
                check_commands: Vec::new(),
                worktree_root: snapshot.server.worktree_root.clone(),
                push_policy: PushPolicy::MissionBranch,
                pr_policy: PrPolicy::Draft,
                archived: false,
            };
            let base_branch = GitWorkflow::new(probe, allowed_roots)
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?
                .current_branch()
                .await
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            // A daemon crash can happen after the project projection commits
            // but before the clone operation is marked complete.  Reconcile
            // that window by treating an already-registered destination as
            // success; never append a second ProjectCreated event for the
            // same checkout.
            let registered = self
                .store
                .snapshot()
                .await?
                .projects
                .iter()
                .any(|project| !project.archived && project.path == payload.destination);
            if !registered {
                self.commit_reduced(
                    CommandEnvelope {
                        protocol_version: PROTOCOL_VERSION,
                        command_id: CommandId::new(),
                        expected_revision: None,
                        command: Command::CreateProject(ProjectSpec {
                            name: Path::new(&payload.destination)
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("project")
                                .to_string(),
                            path: Some(payload.destination.clone()),
                            clone_url: Some(payload.url.clone()),
                            base_branch,
                            remote: Some("origin".into()),
                            check_commands: Vec::new(),
                            worktree_root: Some(snapshot.server.worktree_root.clone()),
                            push_policy: PushPolicy::MissionBranch,
                            pr_policy: PrPolicy::Draft,
                        }),
                    },
                    ActorRef::system(),
                )
                .await?;
            }
            Ok::<(), OrchestratorError>(())
        }
        .await;
        match clone_result {
            Ok(()) => {
                self.finish_operation(operation.id, OperationStatus::Succeeded, String::new())
                    .await?
            }
            Err(error) => {
                self.finish_operation(operation.id, OperationStatus::Failed, error.to_string())
                    .await?
            }
        }
        Ok(())
    }

    /// Execute a standalone, daemon-owned check operation. Task acceptance
    /// normally uses the larger `CommitTask` saga (which runs checks before
    /// and after the commit), but keeping this worker real prevents a
    /// recovered `RunChecks` row from being mislabeled as an unsupported
    /// operation and makes project check runs observable in the ledger.
    async fn run_checks_operation(&self, operation: OperationView) -> Result<()> {
        let task_id = TaskId::parse(&operation.resource).map_err(|_| {
            OrchestratorError::Validation("check operation task id is invalid".into())
        })?;
        let result = async {
            self.update_operation(&operation, OperationStatus::Running, "checks", None)
                .await?;
            let snapshot = self.store.snapshot().await?;
            let task = snapshot
                .tasks
                .iter()
                .find(|task| task.id == task_id)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            let mission = snapshot
                .missions
                .iter()
                .find(|mission| mission.id == task.mission_id)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            let project = snapshot
                .projects
                .iter()
                .find(|project| project.id == mission.project_id && !project.archived)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            let worktree = task.worktree.clone().ok_or_else(|| {
                OrchestratorError::Validation("check operation worktree is unavailable".into())
            })?;
            let workflow = GitWorkflow::new(
                project,
                snapshot
                    .server
                    .allowed_project_roots
                    .iter()
                    .map(PathBuf::from)
                    .collect(),
            )
            .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            let checks = workflow
                .run_checks(Path::new(&worktree))
                .await
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            if checks.iter().any(|check| !check.success) {
                return Err(OrchestratorError::Validation(
                    "required project checks failed".into(),
                ));
            }
            Ok::<(), OrchestratorError>(())
        }
        .await;
        match result {
            Ok(()) => {
                self.finish_operation(operation.id, OperationStatus::Succeeded, String::new())
                    .await?;
            }
            Err(error) => {
                self.finish_operation(operation.id, OperationStatus::Failed, error.to_string())
                    .await?;
            }
        }
        Ok(())
    }

    async fn run_commit_task_operation(&self, operation: OperationView) -> Result<()> {
        let task_id = TaskId::parse(&operation.resource)
            .map_err(|_| OrchestratorError::Validation("operation task id is invalid".into()))?;
        let result = async {
            self.update_operation(&operation, OperationStatus::Running, "checks", None)
                .await?;
            let snapshot = self.store.snapshot().await?;
            let task = snapshot
                .tasks
                .iter()
                .find(|task| task.id == task_id)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            let mission = snapshot
                .missions
                .iter()
                .find(|mission| mission.id == task.mission_id)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            let project = snapshot
                .projects
                .iter()
                .find(|project| project.id == mission.project_id && !project.archived)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            let worktree = task.worktree.clone().ok_or_else(|| {
                OrchestratorError::Validation("task worktree is unavailable".into())
            })?;
            let workflow = GitWorkflow::new(
                project,
                snapshot
                    .server
                    .allowed_project_roots
                    .iter()
                    .map(PathBuf::from)
                    .collect(),
            )
            .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            let checks = workflow
                .run_checks(Path::new(&worktree))
                .await
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            if checks.iter().any(|check| !check.success) {
                return Err(OrchestratorError::Validation(
                    "required project checks failed; task remains in review".into(),
                ));
            }
            self.update_operation(&operation, OperationStatus::Running, "capture-diff", None)
                .await?;
            let diff = workflow
                .capture_diff(Path::new(&worktree))
                .await
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            if !diff.is_empty() {
                // Capture the review payload before mutating either branch.
                // The command id is derived from the task id, so a daemon
                // crash after the artifact commit but before the operation
                // advances can replay the same publication without creating
                // duplicate artifacts.
                self.publish_task_diff(&task, &diff).await?;
            }
            let mission_plan = workflow.branch_plan(&mission.branch, &workflow.project.base_branch);
            self.update_operation(
                &operation,
                OperationStatus::Running,
                "mission-worktree",
                None,
            )
            .await?;
            workflow
                .create_worktree(&mission_plan)
                .await
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            let task_branch = task
                .branch
                .clone()
                .unwrap_or_else(|| format!("frank/task-{task_id}"));
            // Commit subjects are daemon-generated and task-derived. Keeping
            // the identifier in the subject gives the durable Git saga a
            // stable marker to detect a squash that completed immediately
            // before a crash, even when another task has since merged too.
            let commit_message = stable_task_commit_message(task_id, &task.title);
            self.update_operation(&operation, OperationStatus::Running, "commit", None)
                .await?;
            workflow
                .commit_worktree(Path::new(&worktree), &commit_message)
                .await
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            // `changed == false` is a valid retry state: the daemon may have
            // committed just before a crash. Re-run checks and the idempotent
            // squash step regardless, so a restart cannot mark a task done
            // while its commit is still stranded on the task branch.
            let post_commit_checks = workflow
                .run_checks(Path::new(&worktree))
                .await
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            if post_commit_checks.iter().any(|check| !check.success) {
                return Err(OrchestratorError::Validation(
                    "required project checks failed after the daemon commit".into(),
                ));
            }
            self.update_operation(&operation, OperationStatus::Running, "squash-merge", None)
                .await?;
            workflow
                .squash_merge(&task_branch, &mission.branch, &commit_message)
                .await
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            self.transition_task(task_id, TaskStatus::Done).await?;
            Ok::<(), OrchestratorError>(())
        }
        .await;
        match result {
            Ok(()) => {
                self.finish_operation(operation.id, OperationStatus::Succeeded, String::new())
                    .await?;
            }
            Err(error) => {
                let message = error.to_string();
                // A squash merge conflict is a recoverable orchestration
                // problem, not a provider failure.  Keep the original task
                // in review and create a daemon-owned resolution card so a
                // worker can resolve it against the latest mission branch.
                // The helper is idempotent: a retry after a crash discovers
                // the existing child instead of appending another card.
                if is_merge_conflict_error(&message) {
                    self.handle_merge_conflict(task_id, &message).await?;
                }
                self.finish_operation(operation.id, OperationStatus::Failed, message)
                    .await?;
            }
        }
        Ok(())
    }

    async fn publish_task_diff(&self, task: &TaskView, diff: &[u8]) -> Result<()> {
        let mission_id = task.mission_id;
        let response = self
            .execute(
                CommandEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    command_id: CommandId::from(task.id.0),
                    expected_revision: None,
                    command: Command::PublishArtifact(ArtifactSpec {
                        mission_id,
                        task_id: Some(task.id),
                        name: format!("task-{}-diff.patch", task.id),
                        mime_type: "text/x-diff".into(),
                        bytes: diff.to_vec(),
                    }),
                },
                ActorRef::system(),
                DeviceRole::Owner,
            )
            .await;
        if let Some(error) = response.error {
            return Err(OrchestratorError::Validation(format!(
                "task diff artifact could not be published: {}",
                error.message
            )));
        }
        Ok(())
    }

    async fn handle_merge_conflict(&self, task_id: TaskId, detail: &str) -> Result<()> {
        let snapshot = self.store.snapshot().await?;
        let task = snapshot
            .tasks
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        let mission = snapshot
            .missions
            .iter()
            .find(|mission| mission.id == task.mission_id)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;

        // A conflict child is identified by the stable parent task ID in its
        // title/objective. This survives operation retries and daemon
        // restarts without introducing a second parent-task relation in the
        // v1 wire schema. A still-running child makes a retry idempotent;
        // completed children permit the next bounded attempt.
        let marker = format!("Resolve merge conflict for task {task_id}");
        if snapshot.tasks.iter().any(|candidate| {
            candidate.mission_id == task.mission_id
                && candidate.title.starts_with(&marker)
                && !matches!(
                    candidate.status,
                    TaskStatus::Done | TaskStatus::Cancelled | TaskStatus::Blocked
                )
        }) {
            return Ok(());
        }

        let conflict_attempts = snapshot
            .tasks
            .iter()
            .filter(|candidate| {
                candidate.mission_id == task.mission_id
                    && candidate
                        .title
                        .starts_with("Resolve merge conflict for task ")
            })
            .count();
        if conflict_attempts >= 2 {
            self.block_mission(
                mission.id,
                "merge conflict could not be resolved after two attempts".into(),
            )
            .await?;
            return Ok(());
        }

        // Keep dependencies aligned with the work that was already merged
        // into the mission branch. Depending on the conflicting review task
        // itself would deadlock the child because that task cannot become
        // Done until its merge succeeds.
        let objective_detail = detail
            .chars()
            .filter(|character| !character.is_control())
            .take(2_000)
            .collect::<String>();
        let title = if conflict_attempts == 0 {
            marker
        } else {
            format!("{marker} (attempt {})", conflict_attempts + 1)
        };
        let spec = TaskSpec {
            mission_id: mission.id,
            title,
            objective: format!(
                "Resolve the Git merge conflict for `{}` against mission branch `{}`.\n\nDaemon diagnostic:\n{}",
                task.title, mission.branch, objective_detail
            ),
            dependencies: task.dependencies,
            priority: task.priority.saturating_add(1),
            assigned_agent: None,
            budget: task.budget,
        };
        let response = self
            .commit_reduced(
                CommandEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    command_id: CommandId::new(),
                    expected_revision: None,
                    command: Command::CreateTask(spec),
                },
                ActorRef::system(),
            )
            .await?;
        let child_id = match response.result {
            Some(CommandResult::Created { id }) => TaskId::parse(&id).map_err(|_| {
                OrchestratorError::Validation("conflict child task id is invalid".into())
            })?,
            _ => {
                return Err(OrchestratorError::Validation(
                    "conflict child task was not created".into(),
                ));
            }
        };
        // The child is immediately runnable once the mission is active. Its
        // worktree operation is still created only by SetTaskStatus::Running,
        // preserving the same durable saga as every other task.
        self.commit_reduced(
            CommandEnvelope {
                protocol_version: PROTOCOL_VERSION,
                command_id: CommandId::new(),
                expected_revision: None,
                command: Command::SetTaskStatus {
                    task_id: child_id,
                    status: TaskStatus::Ready,
                },
            },
            ActorRef::system(),
        )
        .await?;
        Ok(())
    }

    async fn update_operation(
        &self,
        original: &OperationView,
        status: OperationStatus,
        phase: &str,
        error: Option<String>,
    ) -> Result<()> {
        for _ in 0..4 {
            let mut snapshot = self.store.snapshot().await?;
            let expected_revision = snapshot.revision;
            let operation = snapshot
                .operations
                .iter_mut()
                .find(|candidate| candidate.id == original.id)
                .ok_or(OrchestratorError::NotFound)?;
            operation.status = status;
            operation.phase = phase.to_string();
            operation.attempt = operation.attempt.saturating_add(1);
            operation.error = error.clone();
            operation.updated_at = timestamp_now();
            let operation_value = operation.clone();
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(expected_revision),
                    ActorRef::system(),
                    Event::OperationChanged {
                        operation: operation_value,
                    },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await
            {
                Ok(_) => return Ok(()),
                Err(StoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }

    async fn finish_operation(
        &self,
        operation_id: OperationId,
        status: OperationStatus,
        error: String,
    ) -> Result<()> {
        for _ in 0..4 {
            let mut snapshot = self.store.snapshot().await?;
            let expected_revision = snapshot.revision;
            let operation = snapshot
                .operations
                .iter_mut()
                .find(|operation| operation.id == operation_id)
                .ok_or(OrchestratorError::NotFound)?;
            operation.status = status;
            operation.phase = if status == OperationStatus::Succeeded {
                "complete".into()
            } else {
                "failed".into()
            };
            operation.error = (!error.is_empty()).then_some(error.clone());
            operation.updated_at = timestamp_now();
            let operation_value = operation.clone();
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(expected_revision),
                    ActorRef::system(),
                    Event::OperationChanged {
                        operation: operation_value,
                    },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await
            {
                Ok(_) => return Ok(()),
                Err(StoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }

    /// Expire pending approvals as a daemon-owned state transition. Provider
    /// sessions cannot be allowed to keep waiting forever on a stale prompt;
    /// expiry blocks the affected task and tears down its scoped runtime so a
    /// later reconcile cannot accidentally continue work without a decision.
    async fn expire_approvals(&self, snapshot: &Snapshot) -> Result<()> {
        let now = epoch_seconds() as u128;
        let expired = snapshot
            .approvals
            .iter()
            .filter(|approval| {
                approval.status == ApprovalStatus::Pending
                    && approval
                        .expires_at
                        .parse::<u128>()
                        .is_ok_and(|expires_at| expires_at <= now)
            })
            .map(|approval| (approval.id, approval.task_id))
            .collect::<Vec<_>>();
        for (approval_id, task_id) in expired {
            let mut latest = self.store.snapshot().await?;
            let Some(approval) = latest
                .approvals
                .iter_mut()
                .find(|approval| approval.id == approval_id)
            else {
                continue;
            };
            if approval.status != ApprovalStatus::Pending {
                continue;
            }
            let Ok(expires_at) = approval.expires_at.parse::<u128>() else {
                continue;
            };
            if expires_at > now {
                continue;
            }
            approval.status = ApprovalStatus::Expired;
            self.store
                .commit_command(
                    CommandId::new(),
                    Some(latest.revision),
                    ActorRef::system(),
                    Event::ApprovalExpired { approval_id },
                    latest,
                    CommandResult::Accepted,
                )
                .await?;

            let latest = self.store.snapshot().await?;
            let Some(task) = latest.tasks.iter().find(|task| task.id == task_id).cloned() else {
                continue;
            };
            if !matches!(task.status, TaskStatus::Running | TaskStatus::Ready) {
                continue;
            }
            if let Some(agent_id) = task.assigned_agent
                && let Some(agent) = latest.agents.iter().find(|agent| agent.id == agent_id)
            {
                if let Some(session) = self.sessions.lock().await.remove(&task_id) {
                    let _ = session.graceful_stop().await;
                }
                self.scheduler.lock().await.finish(task_id, agent.provider);
                if let Some(capability) = self
                    .agent_capabilities
                    .lock()
                    .await
                    .iter()
                    .find(|(_, capability)| capability.task_id == task_id)
                    .map(|(token, _)| token.clone())
                {
                    self.revoke_agent_capability(&capability).await;
                }
                let _ = self
                    .clear_agent_session(agent_id, AgentStatus::Paused)
                    .await;
            }
            let _ = self.transition_task(task_id, TaskStatus::Blocked).await;
        }
        Ok(())
    }

    /// Revoke terminal control leases whose heartbeat window elapsed while
    /// the owner was disconnected.  Lease expiry is a server-owned state
    /// transition rather than a client-side hint: clearing the durable lease
    /// emits an event, removes the SQLite control-lease projection, and lets
    /// the normal reconciler resume the paused provider on its next tick.
    async fn expire_terminal_leases(&self, snapshot: &Snapshot) -> Result<()> {
        let now = epoch_seconds() as u128;
        let expired = snapshot
            .terminals
            .iter()
            .filter_map(|session| {
                let lease = session.lease.as_ref()?;
                let expires_at = lease.expires_at.parse::<u128>().ok()?;
                (expires_at <= now).then_some((session.id, lease.clone()))
            })
            .collect::<Vec<_>>();

        for (session_id, previous_lease) in expired {
            // A concurrent renew/release wins over this stale reconciliation
            // pass. Retry the next 500 ms tick rather than clearing a fresh
            // lease with an old event.
            let mut latest = self.store.snapshot().await?;
            let Some(session) = latest
                .terminals
                .iter_mut()
                .find(|session| session.id == session_id)
            else {
                continue;
            };
            let still_expired = session.lease.as_ref().is_some_and(|lease| {
                lease.lease_id == previous_lease.lease_id
                    && lease
                        .expires_at
                        .parse::<u128>()
                        .is_ok_and(|expires_at| expires_at <= now)
            });
            if !still_expired {
                continue;
            }
            session.lease = None;
            let released = ControlLeaseView {
                lease_id: previous_lease.lease_id,
                session_id,
                actor: previous_lease.actor,
                expires_at: "released".into(),
            };
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(latest.revision),
                    ActorRef::system(),
                    Event::TerminalLeaseChanged { lease: released },
                    latest,
                    CommandResult::Accepted,
                )
                .await
            {
                Ok(_) => {
                    // `pause_for_terminal` already stopped the provider when
                    // control was acquired. Restore the agent's idle marker
                    // now that disconnect grace has expired; the scheduler
                    // will start/resume the task on its next reconciliation.
                    self.resume_after_terminal(session_id).await?;
                }
                Err(StoreError::StaleRevision { .. }) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    async fn ensure_task_assignment(&self, task_id: TaskId) -> Result<Option<TaskView>> {
        let snapshot = self.store.snapshot().await?;
        let Some(task) = snapshot
            .tasks
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
        else {
            return Ok(None);
        };
        if task.assigned_agent.is_some() {
            return Ok(Some(task));
        }
        let agent = snapshot
            .agents
            .iter()
            .find(|agent| {
                !agent.archived
                    && agent.display_name != "Frank supervisor"
                    && matches!(agent.status, AgentStatus::Offline | AgentStatus::Idle)
            })
            .cloned();
        let Some(agent) = agent else {
            return Ok(None);
        };
        let response = self
            .execute(
                CommandEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    command_id: CommandId::new(),
                    expected_revision: None,
                    command: Command::AssignTask {
                        task_id,
                        agent_id: agent.id,
                    },
                },
                ActorRef::system(),
                DeviceRole::Owner,
            )
            .await;
        if response.error.is_some() {
            return Ok(None);
        }
        Ok(self
            .store
            .snapshot()
            .await?
            .tasks
            .into_iter()
            .find(|task| task.id == task_id))
    }

    async fn start_task_session(&self, task_id: TaskId) -> Result<()> {
        if self.sessions.lock().await.contains_key(&task_id) {
            return Ok(());
        }
        let snapshot = self.store.snapshot().await?;
        let task = snapshot
            .tasks
            .iter()
            .find(|task| task.id == task_id && task.status == TaskStatus::Running)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        let agent_id = task.assigned_agent.ok_or_else(|| {
            OrchestratorError::Validation("a running task must have an assigned agent".into())
        })?;
        let agent = snapshot
            .agents
            .iter()
            .find(|agent| agent.id == agent_id && !agent.archived)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        let mission = snapshot
            .missions
            .iter()
            .find(|mission| mission.id == task.mission_id)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        let provider = agent.provider;
        let scheduler_started = self.scheduler.lock().await.start(task.id, provider);
        if !scheduler_started {
            return Ok(());
        }
        self.start_scope_clock_with_budget(task.id.to_string(), &task.budget)
            .await;
        self.start_scope_clock_with_budget(task.mission_id.to_string(), &mission.budget)
            .await;
        self.start_scope_clock_with_budget(agent.id.to_string(), &agent.budget)
            .await;
        let Some(cwd) = task.worktree.clone() else {
            self.scheduler.lock().await.finish(task.id, provider);
            return Err(OrchestratorError::Validation(
                "running task has no worktree".into(),
            ));
        };
        if cwd.trim().is_empty() {
            self.scheduler.lock().await.finish(task.id, provider);
            return Err(OrchestratorError::Validation(
                "running task worktree is unavailable".into(),
            ));
        }
        let workflow = GitWorkflow::new(
            snapshot
                .projects
                .iter()
                .find(|project| project.id == mission.project_id && !project.archived)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?,
            snapshot
                .server
                .allowed_project_roots
                .iter()
                .map(PathBuf::from)
                .collect(),
        )
        .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
        let _mission_plan = workflow.branch_plan(&mission.branch, &workflow.project.base_branch);
        let _task_plan = workflow.task_plan_from_base(task.id, &mission.branch);
        // Worktree creation is a durable operation committed together with
        // the Running transition. The reconciler performs the filesystem
        // effect before this method is called. Refuse to launch a provider
        // while that operation is still queued/recovering; doing the Git
        // mutation here would reintroduce a crash window with no journal row.
        let worktree_ready = snapshot.operations.iter().any(|operation| {
            operation.kind == OperationKind::CreateWorktree
                && worktree_operation_matches_task(operation, task.id)
                && operation.status == OperationStatus::Succeeded
        });
        if !worktree_ready {
            self.scheduler.lock().await.finish(task.id, provider);
            return Ok(());
        }
        if !Path::new(&cwd).is_dir() {
            self.scheduler.lock().await.finish(task.id, provider);
            return Err(OrchestratorError::Validation(
                "running task worktree is unavailable".into(),
            ));
        }
        let session_capability = self.issue_agent_capability(agent.id, task.id).await;
        let server_url = local_server_url(&snapshot.server);
        let request = StartRequest {
            agent_id: agent.id.to_string(),
            task_id: Some(task.id.to_string()),
            cwd,
            instructions: format!(
                "{}\n\nAgent profile:\n{}",
                task.objective, agent.instructions
            ),
            policy: agent.policy.clone(),
            model: agent.model.clone(),
            resume_session_id: agent.provider_session_id.clone(),
            server_url,
            server_certificate_fingerprint: (!snapshot.server.tls_fingerprint.is_empty())
                .then(|| snapshot.server.tls_fingerprint.clone()),
            session_capability: Some(session_capability.clone()),
        };
        let session = match if request.resume_session_id.is_some() {
            self.runtime
                .resume(
                    provider,
                    request.clone(),
                    request.resume_session_id.as_deref().unwrap(),
                )
                .await
        } else {
            self.runtime.start(provider, request.clone()).await
        } {
            Ok(session) => session,
            Err(error) => {
                self.revoke_agent_capability(&session_capability).await;
                self.scheduler.lock().await.finish(task.id, provider);
                self.mark_task_failure(task.id, agent.id, provider, error.to_string())
                    .await?;
                return Err(OrchestratorError::ProviderUnavailable(error.to_string()));
            }
        };
        let session = Arc::new(session);
        let mut events = match session.events().await {
            Ok(events) => events,
            Err(error) => {
                self.revoke_agent_capability(&session_capability).await;
                self.scheduler.lock().await.finish(task.id, provider);
                self.mark_task_failure(task.id, agent.id, provider, error.to_string())
                    .await?;
                return Err(OrchestratorError::ProviderUnavailable(error.to_string()));
            }
        };
        self.sessions.lock().await.insert(task.id, session.clone());
        self.set_agent(agent.id, AgentStatus::Starting, None)
            .await?;
        if let Err(error) = session
            .send(&ProviderMessage {
                role: "user".into(),
                // Preserve persona/profile guidance from the Add Agent
                // wizard; sending only the objective silently discarded it.
                content: request.instructions.clone(),
                correlation_id: Some(task.id.to_string()),
            })
            .await
        {
            self.revoke_agent_capability(&session_capability).await;
            self.sessions.lock().await.remove(&task.id);
            self.scheduler.lock().await.finish(task.id, provider);
            self.mark_task_failure(task.id, agent.id, provider, error.to_string())
                .await?;
            return Err(OrchestratorError::ProviderUnavailable(error.to_string()));
        }

        let orchestrator = self.clone();
        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                let _ = orchestrator
                    .handle_runtime_event(task.id, agent.id, mission.id, provider, event)
                    .await;
            }
        });
        Ok(())
    }

    async fn start_scope_clock(&self, scope_id: String) {
        let now = epoch_seconds();
        // Keeping the first start time for a scope makes a mission budget
        // cover all of its parallel tasks instead of resetting on every
        // worker frame. A subsequent task in the same mission therefore
        // cannot evade a time limit by restarting its provider session.
        let persisted = self
            .store
            .budget_clock(&scope_id)
            .await
            .ok()
            .flatten()
            .map(|(started_at, _)| started_at);
        let mut clocks = self.scope_started_at.lock().await;
        let started = persisted.unwrap_or(now);
        let is_new = !clocks.contains_key(&scope_id);
        clocks.entry(scope_id.clone()).or_insert(started);
        drop(clocks);
        if is_new {
            let _ = self
                .store
                .upsert_budget_clock(&scope_id, started, None)
                .await;
        }
    }

    async fn start_scope_clock_with_budget(&self, scope_id: String, budget: &Budget) {
        self.start_scope_clock(scope_id.clone()).await;
        let Some(limit) = budget.time_seconds else {
            return;
        };
        let Some((started, _)) = self.store.budget_clock(&scope_id).await.ok().flatten() else {
            return;
        };
        let _ = self
            .store
            .upsert_budget_clock(&scope_id, started, Some(started.saturating_add(limit)))
            .await;
    }

    async fn scope_elapsed_seconds(&self, scope_id: &str) -> u64 {
        let now = epoch_seconds();
        let mut clocks = self.scope_started_at.lock().await;
        let started = *clocks.entry(scope_id.to_string()).or_insert(now);
        now.saturating_sub(started)
    }

    async fn handle_runtime_event(
        &self,
        task_id: TaskId,
        agent_id: AgentId,
        mission_id: MissionId,
        provider: Provider,
        event: RuntimeEvent,
    ) -> Result<()> {
        // A killed provider can still have one buffered frame in the reader
        // task. Once the daemon removed the live session, that stale frame
        // must not resurrect an agent or append usage to a retried/blocked
        // task.
        if !self.sessions.lock().await.contains_key(&task_id) {
            return Ok(());
        }
        match event {
            RuntimeEvent::Ready {
                provider_session_id,
            } => {
                self.set_agent(agent_id, AgentStatus::Working, Some(provider_session_id))
                    .await?
            }
            RuntimeEvent::Text { text } => {
                let body = text
                    .chars()
                    .take(self.max_message_bytes)
                    .collect::<String>();
                if !body.trim().is_empty() {
                    let _ = self
                        .execute(
                            CommandEnvelope {
                                protocol_version: PROTOCOL_VERSION,
                                command_id: CommandId::new(),
                                expected_revision: None,
                                command: Command::SendMessage(MessageSpec {
                                    message_id: None,
                                    mission_id,
                                    task_id: Some(task_id),
                                    recipient: ActorRef::supervisor(),
                                    act: MessageAct::Inform,
                                    body,
                                    artifact_ids: Vec::new(),
                                    reply_to: None,
                                    hop: 0,
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
                }
            }
            RuntimeEvent::ApprovalRequest {
                operation,
                reason,
                cwd,
            } => {
                let _ = self
                    .execute(
                        CommandEnvelope {
                            protocol_version: PROTOCOL_VERSION,
                            command_id: CommandId::new(),
                            expected_revision: None,
                            command: Command::RequestApproval(ApprovalSpec {
                                agent_id,
                                task_id,
                                operation,
                                cwd: cwd.unwrap_or_default(),
                                project: format!("mission:{mission_id}"),
                                reason,
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
                self.set_agent(agent_id, AgentStatus::NeedsApproval, None)
                    .await?;
            }
            RuntimeEvent::Usage(usage) => {
                // Usage is recorded once under the task scope, while the
                // in-memory ledger attributes the same measured telemetry to
                // its mission and agent parents. This keeps the GUI ledger
                // from double-counting rows and still enforces all limits.
                let usage_view = UsageView {
                    id: AttemptId::new(),
                    scope: BudgetScope::Task,
                    scope_id: task_id.to_string(),
                    provider,
                    measured_input_tokens: usage.measured_input_tokens,
                    measured_output_tokens: usage.measured_output_tokens,
                    estimated_input_tokens: usage.estimated_input_tokens,
                    estimated_output_tokens: usage.estimated_output_tokens,
                    cost_micros: usage.cost_micros,
                    recorded_at: timestamp_now(),
                };
                let budget_result = self
                    .record_runtime_usage(usage_view, mission_id, agent_id)
                    .await;
                let exceeded_scope = match budget_result {
                    Ok(scope) => scope,
                    // A failed usage write must fail closed: stopping the
                    // provider is safer than allowing work to continue with
                    // an unknown budget projection. Treat it as task scoped
                    // because that is the smallest scope we can safely stop.
                    Err(_) => Some(BudgetScope::Task),
                };
                if let Some(scope) = exceeded_scope {
                    let session = { self.sessions.lock().await.remove(&task_id) };
                    if let Some(session) = session {
                        let _ = session.graceful_stop().await;
                    }
                    self.scheduler.lock().await.finish(task_id, provider);
                    if let Some(capability) = self
                        .agent_capabilities
                        .lock()
                        .await
                        .iter()
                        .find(|(_, capability)| capability.task_id == task_id)
                        .map(|(token, _)| token.clone())
                    {
                        self.revoke_agent_capability(&capability).await;
                    }
                    let _ = self.transition_task(task_id, TaskStatus::Blocked).await;
                    // Mission budgets apply across all child tasks. Pausing
                    // the mission as well as blocking the current task keeps
                    // the scheduler from immediately starting another worker
                    // and spending past the same hard limit. Task/agent limits
                    // only stop the affected attempt.
                    if scope == BudgetScope::Mission {
                        let _ = self
                            .execute(
                                CommandEnvelope {
                                    protocol_version: PROTOCOL_VERSION,
                                    command_id: CommandId::new(),
                                    expected_revision: None,
                                    command: Command::PauseMission { mission_id },
                                },
                                ActorRef::system(),
                                DeviceRole::Owner,
                            )
                            .await;
                    }
                    self.clear_agent_session(agent_id, AgentStatus::Paused)
                        .await?;
                }
            }
            RuntimeEvent::Stopped { code } => {
                // Error/budget paths remove the live session before killing
                // the child. Its reader may still emit EOF/Stopped; ignore
                // that stale terminal event so it cannot overwrite a retry or
                // a budget-blocked task.
                if code.is_some_and(|code| code != 0) {
                    self.mark_task_failure(
                        task_id,
                        agent_id,
                        provider,
                        format!("provider exited with status {code:?}"),
                    )
                    .await?;
                    return Ok(());
                }
                self.scheduler.lock().await.finish(task_id, provider);
                let _ = self.transition_task(task_id, TaskStatus::Review).await;
                self.clear_agent_session(agent_id, AgentStatus::Idle)
                    .await?;
                self.sessions.lock().await.remove(&task_id);
                if let Some(capability) = self
                    .agent_capabilities
                    .lock()
                    .await
                    .iter()
                    .find(|(_, capability)| capability.task_id == task_id)
                    .map(|(token, _)| token.clone())
                {
                    self.revoke_agent_capability(&capability).await;
                }
            }
            RuntimeEvent::Error { message } => {
                let session = self.sessions.lock().await.get(&task_id).cloned();
                self.mark_task_failure(task_id, agent_id, provider, message)
                    .await?;
                if let Some(session) = session {
                    let _ = session.graceful_stop().await;
                }
            }
            RuntimeEvent::Raw(_) | RuntimeEvent::ToolCall { .. } => {}
        }
        Ok(())
    }

    async fn set_agent(
        &self,
        agent_id: AgentId,
        status: AgentStatus,
        provider_session_id: Option<String>,
    ) -> Result<()> {
        for _ in 0..4 {
            let mut snapshot = self.store.snapshot().await?;
            let expected_revision = snapshot.revision;
            let agent = snapshot
                .agents
                .iter_mut()
                .find(|agent| agent.id == agent_id)
                .ok_or(OrchestratorError::NotFound)?;
            agent.status = status;
            if provider_session_id.is_some() {
                agent.provider_session_id = provider_session_id.clone();
            }
            let agent = agent.clone();
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(expected_revision),
                    ActorRef::system(),
                    Event::AgentUpserted { agent },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await
            {
                Ok(_) => return Ok(()),
                Err(StoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }

    /// Clear a completed or failed provider session from the persistent
    /// profile. Otherwise the next task assigned to the same agent would try
    /// to resume an unrelated provider conversation.
    async fn clear_agent_session(&self, agent_id: AgentId, status: AgentStatus) -> Result<()> {
        for _ in 0..4 {
            let mut snapshot = self.store.snapshot().await?;
            let expected_revision = snapshot.revision;
            let agent = snapshot
                .agents
                .iter_mut()
                .find(|agent| agent.id == agent_id)
                .ok_or(OrchestratorError::NotFound)?;
            agent.status = status;
            agent.provider_session_id = None;
            let agent = agent.clone();
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(expected_revision),
                    ActorRef::system(),
                    Event::AgentUpserted { agent },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await
            {
                Ok(_) => return Ok(()),
                Err(StoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }

    async fn transition_task(&self, task_id: TaskId, status: TaskStatus) -> Result<()> {
        let response = self
            .execute(
                CommandEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    command_id: CommandId::new(),
                    expected_revision: None,
                    command: Command::SetTaskStatus { task_id, status },
                },
                ActorRef::system(),
                DeviceRole::Owner,
            )
            .await;
        if let Some(error) = response.error {
            return Err(OrchestratorError::Validation(error.message));
        }
        Ok(())
    }

    async fn block_mission(&self, mission_id: MissionId, reason: String) -> Result<()> {
        for _ in 0..4 {
            let mut snapshot = self.store.snapshot().await?;
            let expected_revision = snapshot.revision;
            let mission = snapshot
                .missions
                .iter_mut()
                .find(|mission| mission.id == mission_id)
                .ok_or(OrchestratorError::NotFound)?;
            if !mission.status.can_transition_to(MissionStatus::Blocked) {
                return Ok(());
            }
            mission.status = MissionStatus::Blocked;
            mission.updated_at = timestamp_now();
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(expected_revision),
                    ActorRef::system(),
                    Event::MissionBlocked {
                        mission_id,
                        reason: reason.clone(),
                    },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await
            {
                Ok(_) => return Ok(()),
                Err(StoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }

    async fn mark_task_failure(
        &self,
        task_id: TaskId,
        agent_id: AgentId,
        provider: Provider,
        message: String,
    ) -> Result<()> {
        self.scheduler.lock().await.finish(task_id, provider);
        let status = 'commit: {
            for _ in 0..4 {
                let mut snapshot = self.store.snapshot().await?;
                let expected_revision = snapshot.revision;
                let task = snapshot
                    .tasks
                    .iter_mut()
                    .find(|task| task.id == task_id)
                    .ok_or(OrchestratorError::NotFound)?;
                let status = retry_after_failure(task);
                task.status = status;
                let task_value = task.clone();
                match self
                    .store
                    .commit_command(
                        CommandId::new(),
                        Some(expected_revision),
                        ActorRef::system(),
                        Event::TaskUpdated { task: task_value },
                        snapshot,
                        CommandResult::Accepted,
                    )
                    .await
                {
                    Ok(_) => break 'commit status,
                    Err(StoreError::StaleRevision { .. }) => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            return Err(OrchestratorError::Store(StoreError::StaleRevision {
                current: self.store.current_revision().await?,
            }));
        };
        // Keep a worker reusable when the retry policy returns the task to
        // Ready. Once attempts are exhausted, surface Failed and leave the
        // task Blocked for explicit intervention.
        let agent_status = if status == TaskStatus::Blocked {
            AgentStatus::Failed
        } else {
            AgentStatus::Idle
        };
        let _ = self.clear_agent_session(agent_id, agent_status).await;
        let _ = self
            .execute(
                CommandEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    command_id: CommandId::new(),
                    expected_revision: None,
                    command: Command::SendMessage(MessageSpec {
                        message_id: None,
                        mission_id: self
                            .store
                            .snapshot()
                            .await?
                            .tasks
                            .iter()
                            .find(|task| task.id == task_id)
                            .map(|task| task.mission_id)
                            .unwrap_or(MissionId::nil()),
                        task_id: Some(task_id),
                        recipient: ActorRef::supervisor(),
                        act: MessageAct::Inform,
                        body: format!("worker failed: {message}"),
                        artifact_ids: Vec::new(),
                        reply_to: None,
                        hop: 0,
                    }),
                },
                ActorRef::system(),
                DeviceRole::Owner,
            )
            .await;
        self.sessions.lock().await.remove(&task_id);
        let capability = self
            .agent_capabilities
            .lock()
            .await
            .iter()
            .find(|(_, capability)| capability.task_id == task_id)
            .map(|(token, _)| token.clone());
        if let Some(capability) = capability {
            self.revoke_agent_capability(&capability).await;
        }
        Ok(())
    }

    async fn error_response(
        &self,
        command_id: CommandId,
        error: OrchestratorError,
    ) -> CommandResponse {
        let revision = self.store.current_revision().await.unwrap_or_default();
        let mut api = match &error {
            OrchestratorError::Forbidden => {
                ApiError::new(ErrorCode::Forbidden, "permission denied")
            }
            OrchestratorError::NotFound => ApiError::new(ErrorCode::NotFound, "resource not found"),
            OrchestratorError::InvalidTransition(message) => {
                ApiError::new(ErrorCode::Conflict, message)
            }
            OrchestratorError::Validation(message) => ApiError::new(ErrorCode::Validation, message),
            OrchestratorError::BudgetExceeded => ApiError::new(
                ErrorCode::BudgetExceeded,
                "measured budget exceeded; adjust the budget or cancel the work",
            ),
            OrchestratorError::ProviderUnavailable(message) => {
                ApiError::new(ErrorCode::ProviderUnavailable, message)
            }
            OrchestratorError::Store(StoreError::StaleRevision { .. }) => {
                match self.store.snapshot().await {
                    Ok(snapshot) => ApiError::conflict(snapshot),
                    Err(_) => ApiError::new(ErrorCode::StaleRevision, "stale revision"),
                }
            }
            OrchestratorError::Store(_) => ApiError::new(ErrorCode::Internal, "persistence failed"),
        };
        if matches!(api.code, ErrorCode::Conflict | ErrorCode::StaleRevision) {
            api.retryable = true;
        }
        CommandResponse::failed(command_id, revision, api)
    }

    async fn apply(&self, envelope: CommandEnvelope, actor: ActorRef) -> Result<CommandResponse> {
        let current = self.store.snapshot().await?;
        let current_revision = current.revision;
        let existing_message_id = match &envelope.command {
            Command::SendMessage(spec) => spec.message_id,
            _ => None,
        }
        .filter(|message_id| {
            current
                .messages
                .iter()
                .any(|message| message.id == *message_id)
        });
        let command_id = envelope.command_id;
        let expected_revision = envelope.expected_revision;
        let artifact_command = match &envelope.command {
            Command::PublishArtifact(spec) => Some(spec.clone()),
            _ => None,
        };
        let upload_bytes = match &envelope.command {
            Command::FinalizeArtifactUpload { upload_id, .. } => self
                .store
                .artifact_upload_bytes(*upload_id)
                .await
                .ok()
                .flatten(),
            _ => None,
        };
        let message_command = matches!(&envelope.command, Command::SendMessage(_));
        let (next, event, result) = self.reduce(current, envelope.command, &actor).await?;
        // A caller may retry a broker send with the same message id but a
        // fresh command id after losing its HTTP response. `reduce` validates
        // that request and returns the already stored message; do not emit a
        // second MessageQueued event or advance the global revision for that
        // logical no-op.
        if let Event::MessageQueued { message } = &event
            && existing_message_id == Some(message.id)
        {
            return Ok(CommandResponse::ok(command_id, current_revision, result));
        }
        let commit = if let Some(spec) = artifact_command.as_ref() {
            self.store
                .commit_command_with_artifact(
                    command_id,
                    expected_revision,
                    actor,
                    event,
                    next,
                    result,
                    Some(spec.bytes.as_slice()),
                )
                .await?
        } else if let Some(bytes) = upload_bytes.as_deref() {
            self.store
                .commit_command_with_artifact(
                    command_id,
                    expected_revision,
                    actor,
                    event,
                    next,
                    result,
                    Some(bytes),
                )
                .await?
        } else {
            self.store
                .commit_command(command_id, expected_revision, actor, event, next, result)
                .await?
        };
        if message_command
            && let Some(CommandResult::Created { id }) = commit.response.result.clone()
            && let Ok(message_id) = MessageId::parse(&id)
        {
            // Queueing and delivery are separate durable transitions. The
            // queue event is committed first, then this broker step marks
            // the message delivered without making a lost HTTP response
            // cause a second provider wake-up or duplicate message body.
            let _ = self.deliver_message(message_id).await;
        }
        Ok(commit.response)
    }

    /// Advance a queued broker message to Delivered.  Recipient wake-up is
    /// intentionally best-effort: the durable event is authoritative and
    /// the reconciler will start/resume an inactive worker on its next tick.
    /// Keeping this transition separate also lets supervisor and worker
    /// messages share exactly the same delivery semantics.
    async fn deliver_message(&self, message_id: MessageId) -> Result<()> {
        for _ in 0..3 {
            let mut snapshot = self.store.snapshot().await?;
            let Some(message) = snapshot
                .messages
                .iter_mut()
                .find(|message| message.id == message_id)
            else {
                return Ok(());
            };
            if message.delivery != DeliveryStatus::Queued {
                return Ok(());
            }
            message.delivery = DeliveryStatus::Delivered;
            let delivered_message = message.clone();
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(snapshot.revision),
                    ActorRef::system(),
                    Event::MessageDelivered { message_id },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await
            {
                Ok(_) => {
                    self.wake_recipient(&delivered_message).await;
                    return Ok(());
                }
                Err(StoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }

    /// Wake a live provider session after the broker has durably delivered a
    /// message.  A worker may be offline when a message arrives; the daemon's
    /// normal reconciler observes the durable delivery and starts/resumes the
    /// assigned task without making the broker depend on a recursive scheduler
    /// call.
    async fn wake_recipient(&self, message: &MessageView) {
        if message.recipient.kind == ActorKind::Supervisor {
            self.wake_supervisor(message).await;
            return;
        }
        let Some(recipient_id) = message
            .recipient
            .id
            .as_deref()
            .and_then(|id| AgentId::parse(id).ok())
        else {
            return;
        };
        let snapshot = match self.store.snapshot().await {
            Ok(snapshot) => snapshot,
            Err(_) => return,
        };
        let task_id = message.task_id.or_else(|| {
            snapshot.tasks.iter().find_map(|task| {
                (task.mission_id == message.mission_id && task.assigned_agent == Some(recipient_id))
                    .then_some(task.id)
            })
        });
        let Some(task_id) = task_id else {
            return;
        };
        if let Some(session) = self.sessions.lock().await.get(&task_id).cloned() {
            let _ = session
                .send(&ProviderMessage {
                    role: "user".into(),
                    content: format!(
                        "[Frank broker message from {}]\n{}",
                        message.sender.display_name.as_deref().unwrap_or("agent"),
                        message.body
                    ),
                    correlation_id: Some(message.id.to_string()),
                })
                .await;
        }
        // Inactive recipients are picked up by the daemon reconciler's
        // existing half-second loop. Avoid recursively spawning that loop
        // from inside `reconcile` itself; the durable Delivered event is the
        // wake signal and remains safe across a process restart.
    }

    /// Route messages addressed to the persistent supervisor through the same
    /// broker used by workers. A supervisor process is created lazily after a
    /// daemon restart, while the message remains durable in the mailbox.
    async fn wake_supervisor(&self, message: &MessageView) {
        let session = match self.ensure_supervisor_session(message.mission_id).await {
            Ok(session) => session,
            Err(_) => return,
        };
        let sender = message.sender.display_name.as_deref().unwrap_or("operator");
        let content = format!(
            "[Frank broker message from {sender}; act={:?}; message_id={}]:\n{}",
            message.act, message.id, message.body
        );
        let _ = session
            .send(&ProviderMessage {
                role: "user".into(),
                content,
                correlation_id: Some(message.id.to_string()),
            })
            .await;
    }

    async fn ensure_supervisor_session(
        &self,
        mission_id: MissionId,
    ) -> Result<Arc<frank_agent::RuntimeSession>> {
        if let Some(session) = self
            .supervisor_sessions
            .lock()
            .await
            .get(&mission_id)
            .cloned()
        {
            return Ok(session);
        }
        let snapshot = self.store.snapshot().await?;
        let mission = snapshot
            .missions
            .iter()
            .find(|mission| mission.id == mission_id)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        let project = snapshot
            .projects
            .iter()
            .find(|project| project.id == mission.project_id && !project.archived)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        let supervisor = snapshot
            .agents
            .iter()
            .find(|agent| agent.display_name == "Frank supervisor" && !agent.archived)
            .cloned()
            .ok_or_else(|| {
                OrchestratorError::ProviderUnavailable("Frank supervisor profile is missing".into())
            })?;
        let request = StartRequest {
            agent_id: supervisor.id.to_string(),
            task_id: None,
            cwd: project.path,
            instructions: supervisor.instructions,
            policy: supervisor.policy,
            model: supervisor.model,
            resume_session_id: mission.supervisor_session_id.clone(),
            server_url: local_server_url(&snapshot.server),
            server_certificate_fingerprint: (!snapshot.server.tls_fingerprint.is_empty())
                .then(|| snapshot.server.tls_fingerprint.clone()),
            session_capability: None,
        };
        let session = if let Some(provider_session_id) = request.resume_session_id.as_deref() {
            self.runtime
                .resume(
                    mission.supervisor_provider,
                    request.clone(),
                    provider_session_id,
                )
                .await
        } else {
            self.runtime
                .start(mission.supervisor_provider, request.clone())
                .await
        }
        .map_err(|error| OrchestratorError::ProviderUnavailable(error.to_string()))?;
        let session = Arc::new(session);
        let mut events = session
            .events()
            .await
            .map_err(|error| OrchestratorError::ProviderUnavailable(error.to_string()))?;
        self.supervisor_sessions
            .lock()
            .await
            .insert(mission_id, session.clone());
        let supervisors = self.supervisor_sessions.clone();
        let orchestrator = self.clone();
        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                match event {
                    RuntimeEvent::Ready {
                        provider_session_id,
                    } => {
                        let _ = orchestrator
                            .persist_supervisor_session_id(mission_id, Some(provider_session_id))
                            .await;
                    }
                    RuntimeEvent::Stopped { .. } | RuntimeEvent::Error { .. } => break,
                    _ => {}
                }
            }
            supervisors.lock().await.remove(&mission_id);
        });
        Ok(session)
    }

    async fn complete_delivery(&self, mission_id: MissionId) -> Result<git::DeliveryResult> {
        let snapshot = self.store.snapshot().await?;
        let mission = snapshot
            .missions
            .iter()
            .find(|mission| mission.id == mission_id)
            .ok_or(OrchestratorError::NotFound)?;
        let project = snapshot
            .projects
            .iter()
            .find(|project| project.id == mission.project_id && !project.archived)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        let workflow = GitWorkflow::new(
            project,
            snapshot
                .server
                .allowed_project_roots
                .iter()
                .map(PathBuf::from)
                .collect(),
        )
        .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
        let mission_plan = workflow.branch_plan(&mission.branch, &workflow.project.base_branch);
        workflow
            .create_worktree(&mission_plan)
            .await
            .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
        let checks = workflow
            .run_checks(&mission_plan.path)
            .await
            .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
        if checks.iter().any(|check| !check.success) {
            return Err(OrchestratorError::Validation(
                "required project checks failed on the mission branch".into(),
            ));
        }
        workflow
            .push_mission(&mission.branch)
            .await
            .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
        Ok(git::DeliveryResult {
            branch: mission.branch.clone(),
            draft_pr_url: None,
        })
    }

    async fn reduce(
        &self,
        mut snapshot: Snapshot,
        command: Command,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        match command {
            Command::Pair(_) => Err(OrchestratorError::Forbidden),
            Command::UpdateSettings { patch } => {
                if let Some(Some(provider)) = patch.supervisor_provider {
                    let probe = self
                        .runtime
                        .doctor()
                        .await
                        .into_iter()
                        .find(|probe| probe.capability.provider == provider);
                    if !probe.as_ref().is_some_and(|probe| {
                        probe.capability.available && probe.capability.logged_in
                    }) {
                        let detail = probe
                            .and_then(|probe| probe.capability.diagnostic)
                            .unwrap_or_else(|| format!("{provider} is not available"));
                        return Err(OrchestratorError::ProviderUnavailable(detail));
                    }
                }
                apply_settings_patch(&mut snapshot.server, patch)?;
                let settings = snapshot.server.clone();
                Ok((
                    snapshot,
                    Event::SettingsChanged { settings },
                    CommandResult::Accepted,
                ))
            }
            Command::CreateProject(spec) => {
                validate_project_spec(&spec, &snapshot.server)?;
                let Some(path) = spec.path.as_ref() else {
                    return Err(OrchestratorError::Validation(
                        "clone projects must use the clone flow with a destination".into(),
                    ));
                };
                let project_probe = ProjectView {
                    id: ProjectId::nil(),
                    name: spec.name.clone(),
                    path: path.clone(),
                    base_branch: spec.base_branch.clone(),
                    remote: spec.remote.clone(),
                    check_commands: spec.check_commands.clone(),
                    worktree_root: spec
                        .worktree_root
                        .clone()
                        .unwrap_or_else(|| snapshot.server.worktree_root.clone()),
                    push_policy: spec.push_policy,
                    pr_policy: spec.pr_policy,
                    archived: false,
                };
                let workflow = GitWorkflow::new(
                    project_probe,
                    snapshot
                        .server
                        .allowed_project_roots
                        .iter()
                        .map(PathBuf::from)
                        .collect(),
                )
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
                workflow
                    .validate_repository()
                    .await
                    .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
                let id = ProjectId::new();
                let project = ProjectView {
                    id,
                    name: spec.name,
                    path: path.clone(),
                    base_branch: spec.base_branch,
                    remote: spec.remote,
                    check_commands: spec.check_commands,
                    worktree_root: spec
                        .worktree_root
                        .unwrap_or_else(|| snapshot.server.worktree_root.clone()),
                    push_policy: spec.push_policy,
                    pr_policy: spec.pr_policy,
                    archived: false,
                };
                snapshot.projects.push(project.clone());
                Ok((
                    snapshot,
                    Event::ProjectUpserted { project },
                    CommandResult::Created { id: id.to_string() },
                ))
            }
            Command::CloneProject { url, destination } => {
                if url.trim().is_empty() || destination.trim().is_empty() {
                    return Err(OrchestratorError::Validation(
                        "clone URL and destination are required".into(),
                    ));
                }
                if !is_allowed_path(&destination, &snapshot.server.allowed_project_roots) {
                    return Err(OrchestratorError::Validation(
                        "destination is outside an allowed project root".into(),
                    ));
                }
                // Cloning is a filesystem/network side effect. Persist the
                // intent first; the operation reconciler can safely resume a
                // clone that completed before a daemon crash.
                let operation = OperationView {
                    id: OperationId::new(),
                    kind: OperationKind::CloneProject,
                    status: OperationStatus::Queued,
                    resource: serde_json::json!({
                        "url": url,
                        "destination": destination,
                    })
                    .to_string(),
                    phase: "clone".into(),
                    attempt: 0,
                    error: None,
                    created_at: timestamp_now(),
                    updated_at: timestamp_now(),
                };
                snapshot.operations.push(operation.clone());
                Ok((
                    snapshot,
                    Event::OperationChanged {
                        operation: operation.clone(),
                    },
                    CommandResult::Operation(operation),
                ))
            }
            Command::ArchiveProject { project_id } => {
                let project = snapshot
                    .projects
                    .iter_mut()
                    .find(|p| p.id == project_id)
                    .ok_or(OrchestratorError::NotFound)?;
                project.archived = true;
                Ok((
                    snapshot,
                    Event::ProjectArchived { project_id },
                    CommandResult::Accepted,
                ))
            }
            Command::CreateAgent(spec) => {
                if !valid_agent_text(&spec.display_name, 128)
                    || spec.instructions.len() > MAX_MESSAGE_BODY_BYTES
                    || !valid_optional_agent_text(spec.model.as_deref(), 256)
                    || !valid_optional_agent_text(spec.pack_id.as_deref(), 128)
                    || !valid_optional_agent_text(spec.pack_level.as_deref(), 128)
                {
                    return Err(OrchestratorError::Validation(
                        "agent identity or instructions are invalid".into(),
                    ));
                }
                if snapshot.agents.iter().any(|agent| {
                    !agent.archived && agent.display_name.eq_ignore_ascii_case(&spec.display_name)
                }) {
                    return Err(OrchestratorError::Validation(
                        "agent display name must be unique".into(),
                    ));
                }
                let probe = self
                    .runtime
                    .doctor()
                    .await
                    .into_iter()
                    .find(|probe| probe.capability.provider == spec.provider);
                if !probe
                    .as_ref()
                    .is_some_and(|probe| probe.capability.available && probe.capability.logged_in)
                {
                    let detail = probe
                        .and_then(|probe| probe.capability.diagnostic)
                        .unwrap_or_else(|| format!("{} is not available", spec.provider));
                    return Err(OrchestratorError::ProviderUnavailable(detail));
                }
                let id = AgentId::new();
                let agent = AgentView {
                    id,
                    display_name: spec.display_name,
                    template: spec.template,
                    provider: spec.provider,
                    model: spec.model,
                    pack_id: spec.pack_id,
                    pack_level: spec.pack_level,
                    instructions: spec.instructions,
                    policy: spec.policy,
                    budget: spec.budget,
                    avatar: spec.avatar,
                    status: AgentStatus::Offline,
                    provider_session_id: None,
                    archived: false,
                };
                snapshot.agents.push(agent.clone());
                Ok((
                    snapshot,
                    Event::AgentUpserted { agent },
                    CommandResult::Created { id: id.to_string() },
                ))
            }
            Command::UpdateAgent { agent_id, patch } => {
                let updated = {
                    let agent = snapshot
                        .agents
                        .iter_mut()
                        .find(|agent| agent.id == agent_id)
                        .ok_or(OrchestratorError::NotFound)?;
                    apply_agent_patch(agent, patch);
                    agent.clone()
                };
                if !valid_agent_text(&updated.display_name, 128)
                    || updated.instructions.len() > MAX_MESSAGE_BODY_BYTES
                    || !valid_optional_agent_text(updated.model.as_deref(), 256)
                    || !valid_optional_agent_text(updated.pack_id.as_deref(), 128)
                    || !valid_optional_agent_text(updated.pack_level.as_deref(), 128)
                    || snapshot.agents.iter().any(|candidate| {
                        candidate.id != agent_id
                            && !candidate.archived
                            && candidate
                                .display_name
                                .eq_ignore_ascii_case(&updated.display_name)
                    })
                {
                    return Err(OrchestratorError::Validation(
                        "agent identity or instructions are invalid".into(),
                    ));
                }
                Ok((
                    snapshot,
                    Event::AgentUpserted { agent: updated },
                    CommandResult::Accepted,
                ))
            }
            Command::ArchiveAgent { agent_id } => {
                let agent = snapshot
                    .agents
                    .iter_mut()
                    .find(|agent| agent.id == agent_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if agent.display_name == "Frank supervisor" {
                    return Err(OrchestratorError::Validation(
                        "the built-in Frank supervisor cannot be archived".into(),
                    ));
                }
                if matches!(
                    agent.status,
                    AgentStatus::Working | AgentStatus::Starting | AgentStatus::Thinking
                ) {
                    return Err(OrchestratorError::Validation(
                        "move or cancel assigned work before archiving an active agent".into(),
                    ));
                }
                agent.archived = true;
                Ok((
                    snapshot,
                    Event::AgentArchived { agent_id },
                    CommandResult::Accepted,
                ))
            }
            Command::CreateMission {
                project_id,
                objective,
            } => {
                let project = snapshot
                    .projects
                    .iter()
                    .find(|project| project.id == project_id && !project.archived)
                    .ok_or(OrchestratorError::NotFound)?;
                let provider = snapshot.server.supervisor_provider.ok_or_else(|| {
                    OrchestratorError::Validation(
                        "choose Codex or Claude as supervisor before creating a mission".into(),
                    )
                })?;
                if objective.trim().is_empty() || objective.len() > MAX_MESSAGE_BODY_BYTES {
                    return Err(OrchestratorError::Validation(
                        "mission objective is empty or too large".into(),
                    ));
                }
                let id = MissionId::new();
                let branch = format!(
                    "frank/mission-{}-{}",
                    slug(&objective),
                    &id.to_string().replace('-', "")[..8]
                );
                let mission = MissionView {
                    id,
                    project_id: project.id,
                    objective,
                    status: MissionStatus::Draft,
                    supervisor_provider: provider,
                    supervisor_session_id: None,
                    branch,
                    budget: snapshot.server.default_budget.clone(),
                    created_at: timestamp_now(),
                    updated_at: timestamp_now(),
                };
                snapshot.missions.push(mission.clone());
                Ok((
                    snapshot,
                    Event::MissionCreated { mission },
                    CommandResult::Created { id: id.to_string() },
                ))
            }
            Command::SetMissionStatus { mission_id, status } => {
                let mission_index = snapshot
                    .missions
                    .iter()
                    .position(|mission| mission.id == mission_id)
                    .ok_or(OrchestratorError::NotFound)?;
                let current_status = snapshot.missions[mission_index].status;
                if !current_status.can_transition_to(status) {
                    return Err(OrchestratorError::InvalidTransition(format!(
                        "mission cannot transition from {:?} to {:?}",
                        current_status, status
                    )));
                }
                if status == MissionStatus::Completed
                    && snapshot.tasks.iter().any(|task| {
                        task.mission_id == mission_id && task.status != TaskStatus::Done
                    })
                {
                    return Err(OrchestratorError::Validation(
                        "all mission tasks must be done before completing the mission".into(),
                    ));
                }
                if status == MissionStatus::Active {
                    let mission = snapshot.missions[mission_index].clone();
                    let project = snapshot
                        .projects
                        .iter()
                        .find(|project| project.id == mission.project_id && !project.archived)
                        .cloned()
                        .ok_or(OrchestratorError::NotFound)?;
                    let workflow = GitWorkflow::new(
                        project,
                        snapshot
                            .server
                            .allowed_project_roots
                            .iter()
                            .map(PathBuf::from)
                            .collect(),
                    )
                    .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
                    workflow
                        .validate_repository()
                        .await
                        .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
                }
                let mission = &mut snapshot.missions[mission_index];
                mission.status = status;
                mission.updated_at = timestamp_now();
                Ok((
                    snapshot,
                    Event::MissionStatusChanged { mission_id, status },
                    CommandResult::Accepted,
                ))
            }
            Command::PauseMission { mission_id } => {
                self.pause_or_resume(snapshot, mission_id, MissionStatus::Paused)
                    .await
            }
            Command::ResumeMission { mission_id } => {
                self.pause_or_resume(snapshot, mission_id, MissionStatus::Active)
                    .await
            }
            Command::CreateTask(spec) => {
                let mission = snapshot
                    .missions
                    .iter()
                    .find(|mission| mission.id == spec.mission_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if matches!(
                    mission.status,
                    MissionStatus::Completed | MissionStatus::Failed | MissionStatus::Cancelled
                ) {
                    return Err(OrchestratorError::Validation(
                        "cannot add a task to a final mission".into(),
                    ));
                }
                if let Some(agent_id) = spec.assigned_agent
                    && !snapshot
                        .agents
                        .iter()
                        .any(|agent| agent.id == agent_id && !agent.archived)
                {
                    return Err(OrchestratorError::NotFound);
                }
                validate_task_spec(&spec, &snapshot.tasks)?;
                let id = TaskId::new();
                let task = TaskView {
                    id,
                    mission_id: spec.mission_id,
                    title: spec.title,
                    objective: spec.objective,
                    dependencies: spec.dependencies,
                    priority: spec.priority,
                    budget: spec.budget,
                    status: TaskStatus::Backlog,
                    assigned_agent: spec.assigned_agent,
                    attempt: 0,
                    max_attempts: DEFAULT_MAX_ATTEMPTS,
                    worktree: None,
                    branch: Some(format!("frank/task-{id}")),
                    result_artifact: None,
                };
                snapshot.tasks.push(task.clone());
                Ok((
                    snapshot,
                    Event::TaskCreated { task },
                    CommandResult::Created { id: id.to_string() },
                ))
            }
            Command::UpdateTask { task_id, patch } => {
                let mut candidate = snapshot
                    .tasks
                    .iter()
                    .find(|task| task.id == task_id)
                    .cloned()
                    .ok_or(OrchestratorError::NotFound)?;
                apply_task_patch(&mut candidate, patch);
                if let Some(agent_id) = candidate.assigned_agent
                    && !snapshot
                        .agents
                        .iter()
                        .any(|agent| agent.id == agent_id && !agent.archived)
                {
                    return Err(OrchestratorError::NotFound);
                }
                validate_task_view(&candidate, &snapshot.tasks)?;
                let task = snapshot
                    .tasks
                    .iter_mut()
                    .find(|task| task.id == task_id)
                    .expect("task checked");
                *task = candidate.clone();
                Ok((
                    snapshot,
                    Event::TaskUpdated { task: candidate },
                    CommandResult::Accepted,
                ))
            }
            Command::SetTaskStatus { task_id, status } => {
                let task_index = snapshot
                    .tasks
                    .iter()
                    .position(|task| task.id == task_id)
                    .ok_or(OrchestratorError::NotFound)?;
                let current_status = snapshot.tasks[task_index].status;
                if !current_status.can_transition_to(status) {
                    return Err(OrchestratorError::InvalidTransition(format!(
                        "task cannot transition from {:?} to {:?}",
                        current_status, status
                    )));
                }
                if matches!(status, TaskStatus::Ready | TaskStatus::Running)
                    && snapshot.tasks[task_index]
                        .dependencies
                        .iter()
                        .any(|dependency| {
                            snapshot
                                .tasks
                                .iter()
                                .find(|candidate| candidate.id == *dependency)
                                .is_none_or(|candidate| candidate.status != TaskStatus::Done)
                        })
                {
                    return Err(OrchestratorError::Validation(
                        "task dependencies must be done before starting this task".into(),
                    ));
                }
                if status == TaskStatus::Running {
                    let assigned_agent = snapshot.tasks[task_index].assigned_agent;
                    if assigned_agent.is_none_or(|agent_id| {
                        !snapshot
                            .agents
                            .iter()
                            .any(|agent| agent.id == agent_id && !agent.archived)
                    }) {
                        return Err(OrchestratorError::Validation(
                            "assign an active agent before running this task".into(),
                        ));
                    }
                }
                let mut worktree_operation = None;
                if status == TaskStatus::Running {
                    let task = snapshot.tasks[task_index].clone();
                    let mission = snapshot
                        .missions
                        .iter()
                        .find(|mission| mission.id == task.mission_id)
                        .cloned()
                        .ok_or(OrchestratorError::NotFound)?;
                    if mission.status != MissionStatus::Active {
                        return Err(OrchestratorError::Validation(
                            "a task can run only while its mission is active".into(),
                        ));
                    }
                    let project = snapshot
                        .projects
                        .iter()
                        .find(|project| project.id == mission.project_id && !project.archived)
                        .cloned()
                        .ok_or(OrchestratorError::NotFound)?;
                    let workflow = GitWorkflow::new(
                        project,
                        snapshot
                            .server
                            .allowed_project_roots
                            .iter()
                            .map(PathBuf::from)
                            .collect(),
                    )
                    .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
                    // Worktree creation is a filesystem mutation and is
                    // intentionally delayed until after this state
                    // transition commits. `start_task_session` performs the
                    // idempotent create/reuse step before launching a
                    // provider, so a crash cannot leave a Running task with
                    // no durable intent.
                    let mission_plan =
                        workflow.branch_plan(&mission.branch, &workflow.project.base_branch);
                    let task_plan = workflow.task_plan_from_base(task.id, &mission.branch);
                    let task = &mut snapshot.tasks[task_index];
                    task.worktree = Some(task_plan.path.to_string_lossy().into_owned());
                    task.branch = Some(task_plan.branch.clone());
                    let has_pending_worktree = snapshot.operations.iter().any(|operation| {
                        operation.kind == OperationKind::CreateWorktree
                            && worktree_operation_matches_task(operation, task_id)
                            && !matches!(
                                operation.status,
                                OperationStatus::Succeeded
                                    | OperationStatus::Cancelled
                                    | OperationStatus::Failed
                            )
                    });
                    if !has_pending_worktree {
                        let now = timestamp_now();
                        let operation = OperationView {
                            id: OperationId::new(),
                            kind: OperationKind::CreateWorktree,
                            status: OperationStatus::Queued,
                            resource: task_id.to_string(),
                            phase: "queued".into(),
                            attempt: 0,
                            error: None,
                            created_at: now.clone(),
                            updated_at: now,
                        };
                        let operation_request = CreateWorktreeOperation {
                            project_id: mission.project_id,
                            mission_id: mission.id,
                            mission_branch: mission_plan.branch,
                            mission_base: mission_plan.base,
                            mission_path: mission_plan.path.to_string_lossy().into_owned(),
                            task_id,
                            task_branch: task_plan.branch,
                            task_base: task_plan.base,
                            task_path: task_plan.path.to_string_lossy().into_owned(),
                        };
                        let resource = serde_json::to_string(&operation_request)
                            .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
                        let mut operation = operation;
                        operation.resource = resource;
                        snapshot.operations.push(operation);
                        worktree_operation = Some(());
                    }
                }
                let task = &mut snapshot.tasks[task_index];
                task.status = status;
                let task_value = task.clone();
                if worktree_operation.is_some() {
                    let operation = snapshot
                        .operations
                        .iter()
                        .find(|operation| {
                            operation.kind == OperationKind::CreateWorktree
                                && worktree_operation_matches_task(operation, task_id)
                                && operation.status == OperationStatus::Queued
                        })
                        .cloned()
                        .ok_or_else(|| {
                            OrchestratorError::Validation(
                                "worktree operation disappeared before commit".into(),
                            )
                        })?;
                    Ok((
                        snapshot,
                        Event::TaskWorktreeProvisioning {
                            task: task_value,
                            operation: operation.clone(),
                        },
                        CommandResult::Operation(operation),
                    ))
                } else {
                    Ok((
                        snapshot,
                        Event::TaskStatusChanged { task_id, status },
                        CommandResult::Accepted,
                    ))
                }
            }
            Command::AssignTask { task_id, agent_id } => {
                let agent = snapshot
                    .agents
                    .iter()
                    .find(|agent| agent.id == agent_id && !agent.archived)
                    .ok_or(OrchestratorError::NotFound)?;
                if matches!(agent.status, AgentStatus::Failed | AgentStatus::Stopping) {
                    return Err(OrchestratorError::Validation(
                        "cannot assign work to a failed or stopping agent".into(),
                    ));
                }
                let task = snapshot
                    .tasks
                    .iter_mut()
                    .find(|task| task.id == task_id)
                    .ok_or(OrchestratorError::NotFound)?;
                let mission = snapshot
                    .missions
                    .iter()
                    .find(|mission| mission.id == task.mission_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if matches!(
                    mission.status,
                    MissionStatus::Completed | MissionStatus::Failed | MissionStatus::Cancelled
                ) {
                    return Err(OrchestratorError::Validation(
                        "cannot assign work on a final mission".into(),
                    ));
                }
                task.assigned_agent = Some(agent_id);
                Ok((
                    snapshot,
                    Event::TaskAssigned { task_id, agent_id },
                    CommandResult::Accepted,
                ))
            }
            Command::SendMessage(spec) => {
                if spec.body.len() > self.max_message_bytes {
                    return Err(OrchestratorError::Validation(
                        "message body exceeds 64 KiB; publish an artifact instead".into(),
                    ));
                }
                let mission = snapshot
                    .missions
                    .iter()
                    .find(|mission| mission.id == spec.mission_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if let Some(task_id) = spec.task_id
                    && !snapshot
                        .tasks
                        .iter()
                        .any(|task| task.id == task_id && task.mission_id == mission.id)
                {
                    return Err(OrchestratorError::NotFound);
                }
                if spec.artifact_ids.len() > 256
                    || spec.artifact_ids.iter().any(|artifact_id| {
                        !snapshot.artifacts.iter().any(|artifact| {
                            artifact.id == *artifact_id
                                && artifact.mission_id == mission.id
                                && (spec.task_id.is_none() || artifact.task_id == spec.task_id)
                        })
                    })
                {
                    return Err(OrchestratorError::NotFound);
                }
                if actor.kind == ActorKind::Agent {
                    let actor_id = actor.id.as_deref().unwrap_or_default();
                    let Some(task_id) = spec.task_id else {
                        return Err(OrchestratorError::Forbidden);
                    };
                    let assigned = snapshot
                        .tasks
                        .iter()
                        .find(|task| task.id == task_id)
                        .and_then(|task| task.assigned_agent)
                        .is_some_and(|assigned| assigned.to_string() == actor_id);
                    if !assigned {
                        return Err(OrchestratorError::Forbidden);
                    }
                    // Provider sessions may talk to the supervisor or to a
                    // worker participating in the same mission. They never
                    // get a general device/system messaging primitive through
                    // the broker, even when their task capability is valid.
                    let recipient_allowed = match spec.recipient.kind {
                        ActorKind::Supervisor => true,
                        ActorKind::Agent => spec
                            .recipient
                            .id
                            .as_deref()
                            .and_then(|id| AgentId::parse(id).ok())
                            .is_some_and(|recipient_id| {
                                snapshot.agents.iter().any(|agent| {
                                    agent.id == recipient_id
                                        && !agent.archived
                                        && snapshot.tasks.iter().any(|candidate| {
                                            candidate.mission_id == mission.id
                                                && candidate.assigned_agent == Some(recipient_id)
                                        })
                                })
                            }),
                        ActorKind::Device | ActorKind::System => false,
                    };
                    if !recipient_allowed {
                        return Err(OrchestratorError::Forbidden);
                    }
                }
                if spec.body.trim().is_empty()
                    || spec
                        .recipient
                        .id
                        .as_deref()
                        .is_some_and(|id| id.len() > 128)
                    || spec.hop > DEFAULT_MESSAGE_HOP_LIMIT
                {
                    return Err(OrchestratorError::Validation(
                        "message recipient, body, or hop is invalid".into(),
                    ));
                }
                if actor.kind == ActorKind::Device && spec.recipient.kind != ActorKind::Supervisor {
                    return Err(OrchestratorError::Forbidden);
                }
                let id = spec.message_id.unwrap_or_else(MessageId::new);
                // A provider bridge may retry after losing its HTTP response.
                // Treat the durable message id as a state-level no-op instead
                // of creating a second delivery. The commit still records the
                // command id normally, while the projection remains exactly
                // unchanged because this event carries the existing message.
                if let Some(existing) = snapshot
                    .messages
                    .iter()
                    .find(|message| message.id == id)
                    .cloned()
                {
                    return Ok((
                        snapshot,
                        Event::MessageQueued { message: existing },
                        CommandResult::Created { id: id.to_string() },
                    ));
                }
                if !self.mailbox.lock().await.accept(id, spec.hop) {
                    return Err(OrchestratorError::Validation(
                        "message hop limit exceeded or duplicate message".into(),
                    ));
                }
                let message = MessageView {
                    id,
                    mission_id: spec.mission_id,
                    task_id: spec.task_id,
                    sender: actor.clone(),
                    recipient: spec.recipient,
                    act: spec.act,
                    body: spec.body,
                    artifact_ids: spec.artifact_ids,
                    reply_to: spec.reply_to,
                    hop: spec.hop,
                    delivery: DeliveryStatus::Queued,
                    created_at: timestamp_now(),
                };
                snapshot.messages.push(message.clone());
                Ok((
                    snapshot,
                    Event::MessageQueued { message },
                    CommandResult::Created { id: id.to_string() },
                ))
            }
            Command::AckMessage { message_id } => {
                let message = snapshot
                    .messages
                    .iter_mut()
                    .find(|message| message.id == message_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if actor.kind == ActorKind::Agent
                    && actor.id.as_deref() != message.recipient.id.as_deref()
                {
                    return Err(OrchestratorError::Forbidden);
                }
                if message.delivery != DeliveryStatus::Delivered {
                    return Err(OrchestratorError::InvalidTransition(
                        "only a delivered message can be acknowledged".into(),
                    ));
                }
                message.delivery = DeliveryStatus::Acknowledged;
                Ok((
                    snapshot,
                    Event::MessageAcknowledged { message_id },
                    CommandResult::Accepted,
                ))
            }
            Command::CompleteMessage {
                message_id,
                success,
            } => {
                let message = snapshot
                    .messages
                    .iter_mut()
                    .find(|message| message.id == message_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if actor.kind == ActorKind::Agent
                    && actor.id.as_deref() != message.recipient.id.as_deref()
                {
                    return Err(OrchestratorError::Forbidden);
                }
                if !matches!(
                    message.delivery,
                    DeliveryStatus::Delivered | DeliveryStatus::Acknowledged
                ) {
                    return Err(OrchestratorError::InvalidTransition(
                        "message is not awaiting completion".into(),
                    ));
                }
                message.delivery = if success {
                    DeliveryStatus::Completed
                } else {
                    DeliveryStatus::Failed
                };
                Ok((
                    snapshot,
                    if success {
                        Event::MessageCompleted { message_id }
                    } else {
                        Event::MessageFailed { message_id }
                    },
                    CommandResult::Accepted,
                ))
            }
            Command::RequestApproval(spec) => {
                if spec.operation.trim().is_empty()
                    || spec.reason.trim().is_empty()
                    || spec.operation.len() > 4096
                    || spec.reason.len() > MAX_MESSAGE_BODY_BYTES
                    || spec
                        .operation
                        .chars()
                        .any(|character| character.is_control())
                    || spec.reason.chars().any(|character| character.is_control())
                    || spec.cwd.len() > 4_096
                    || spec.project.len() > 4_096
                    || spec.cwd.chars().any(|character| character.is_control())
                    || spec.project.chars().any(|character| character.is_control())
                {
                    return Err(OrchestratorError::Validation(
                        "approval operation or reason is invalid".into(),
                    ));
                }
                let task = snapshot
                    .tasks
                    .iter()
                    .find(|task| task.id == spec.task_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if !snapshot
                    .agents
                    .iter()
                    .any(|agent| agent.id == spec.agent_id && !agent.archived)
                    || !snapshot
                        .missions
                        .iter()
                        .any(|mission| mission.id == task.mission_id)
                {
                    return Err(OrchestratorError::NotFound);
                }
                if actor.kind == ActorKind::Agent {
                    let expected_actor_id = spec.agent_id.to_string();
                    if actor.id.as_deref() != Some(expected_actor_id.as_str()) {
                        return Err(OrchestratorError::Forbidden);
                    }
                }
                let approval = ApprovalView {
                    id: ApprovalId::new(),
                    agent_id: spec.agent_id,
                    task_id: spec.task_id,
                    operation: spec.operation,
                    cwd: spec.cwd,
                    project: spec.project,
                    reason: spec.reason,
                    status: ApprovalStatus::Pending,
                    expires_at: now_plus_seconds(snapshot.server.approval_ttl_seconds).to_string(),
                };
                snapshot.approvals.push(approval.clone());
                Ok((
                    snapshot,
                    Event::ApprovalRequested {
                        approval: approval.clone(),
                    },
                    CommandResult::Created {
                        id: approval.id.to_string(),
                    },
                ))
            }
            Command::DecideApproval {
                approval_id,
                decision,
            } => {
                let approval = snapshot
                    .approvals
                    .iter_mut()
                    .find(|approval| approval.id == approval_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if !matches!(approval.status, ApprovalStatus::Pending) {
                    return Err(OrchestratorError::InvalidTransition(
                        "approval is no longer pending".into(),
                    ));
                }
                if approval.expires_at.parse::<u128>().unwrap_or_default() <= now_plus_seconds(0) {
                    approval.status = ApprovalStatus::Expired;
                    let approval_id = approval.id;
                    return Ok((
                        snapshot,
                        Event::ApprovalExpired { approval_id },
                        CommandResult::Accepted,
                    ));
                }
                approval.status = match decision {
                    ApprovalDecision::AllowOnce => ApprovalStatus::Approved,
                    ApprovalDecision::DenyOnce => ApprovalStatus::Denied,
                };
                Ok((
                    snapshot,
                    Event::ApprovalDecided {
                        approval_id,
                        decision,
                    },
                    CommandResult::Accepted,
                ))
            }
            Command::AdjustBudget {
                scope,
                scope_id,
                budget,
            } => {
                if scope_id.trim().is_empty() {
                    return Err(OrchestratorError::Validation(
                        "budget scope id is required".into(),
                    ));
                }
                match scope {
                    BudgetScope::Mission => {
                        let target_id =
                            MissionId::parse(&scope_id).map_err(|_| OrchestratorError::NotFound)?;
                        let target = snapshot
                            .missions
                            .iter_mut()
                            .find(|mission| mission.id == target_id)
                            .ok_or(OrchestratorError::NotFound)?;
                        target.budget = budget;
                    }
                    BudgetScope::Agent => {
                        let target_id =
                            AgentId::parse(&scope_id).map_err(|_| OrchestratorError::NotFound)?;
                        let target = snapshot
                            .agents
                            .iter_mut()
                            .find(|agent| agent.id == target_id)
                            .ok_or(OrchestratorError::NotFound)?;
                        target.budget = budget;
                    }
                    BudgetScope::Task => {
                        let target_id =
                            TaskId::parse(&scope_id).map_err(|_| OrchestratorError::NotFound)?;
                        let target = snapshot
                            .tasks
                            .iter_mut()
                            .find(|task| task.id == target_id)
                            .ok_or(OrchestratorError::NotFound)?;
                        target.budget = budget;
                    }
                }
                Ok((
                    snapshot,
                    Event::BudgetPaused {
                        scope,
                        reason: format!("budget updated for {scope_id}"),
                    },
                    CommandResult::Accepted,
                ))
            }
            Command::ProposeMemory {
                agent_id,
                path,
                content,
            } => {
                if content.len() > MAX_MESSAGE_BODY_BYTES
                    || path.contains("..")
                    || !path.ends_with("memory.md")
                {
                    return Err(OrchestratorError::Validation(
                        "memory proposal path or size is invalid".into(),
                    ));
                }
                if !snapshot.agents.iter().any(|agent| agent.id == agent_id) {
                    return Err(OrchestratorError::NotFound);
                }
                if actor.kind == ActorKind::Agent
                    && actor.id.as_deref().and_then(|id| AgentId::parse(id).ok()) != Some(agent_id)
                {
                    return Err(OrchestratorError::Forbidden);
                }
                let resource = serde_json::to_string(&WriteMemoryOperation {
                    agent_id,
                    path,
                    content,
                })
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
                let now = timestamp_now();
                let operation = OperationView {
                    id: OperationId::new(),
                    kind: OperationKind::WriteMemory,
                    status: OperationStatus::Queued,
                    resource,
                    phase: "queued".into(),
                    attempt: 0,
                    error: None,
                    created_at: now.clone(),
                    updated_at: now,
                };
                snapshot.operations.push(operation.clone());
                Ok((
                    snapshot,
                    Event::OperationChanged {
                        operation: operation.clone(),
                    },
                    CommandResult::Operation(operation),
                ))
            }
            Command::ReadMemory { agent_id, path } => {
                if path.contains("..") || !path.ends_with("memory.md") {
                    return Err(OrchestratorError::Validation(
                        "memory path is invalid".into(),
                    ));
                }
                if !snapshot.agents.iter().any(|agent| agent.id == agent_id) {
                    return Err(OrchestratorError::NotFound);
                }
                if actor.kind == ActorKind::Agent
                    && actor.id.as_deref().and_then(|id| AgentId::parse(id).ok()) != Some(agent_id)
                {
                    return Err(OrchestratorError::Forbidden);
                }
                let content = frank_store::memory::MemoryRepository::new(&self.memory_root)
                    .read(agent_id, &path)
                    .map_err(|error| OrchestratorError::Validation(error.to_string()))?
                    .unwrap_or_default();
                Ok((
                    snapshot,
                    Event::MemoryRead {
                        agent_id,
                        path: path.clone(),
                    },
                    CommandResult::Memory { path, content },
                ))
            }
            Command::PublishArtifact(spec) => {
                if spec.bytes.len() as u64 > MAX_ARTIFACT_BYTES {
                    return Err(OrchestratorError::Validation(
                        "artifact exceeds the configured size cap".into(),
                    ));
                }
                if spec.name.trim().is_empty()
                    || spec.name.len() > 256
                    || spec.name.chars().any(|character| character.is_control())
                    || spec.mime_type.trim().is_empty()
                    || spec.mime_type.len() > 256
                    || spec
                        .mime_type
                        .chars()
                        .any(|character| character.is_control())
                {
                    return Err(OrchestratorError::Validation(
                        "artifact name or MIME type is invalid".into(),
                    ));
                }
                let mission = snapshot
                    .missions
                    .iter()
                    .find(|mission| mission.id == spec.mission_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if let Some(task_id) = spec.task_id
                    && !snapshot
                        .tasks
                        .iter()
                        .any(|task| task.id == task_id && task.mission_id == mission.id)
                {
                    return Err(OrchestratorError::NotFound);
                }
                let id = ArtifactId::new();
                let mut digest = Sha256::new();
                digest.update(&spec.bytes);
                let artifact = ArtifactView {
                    id,
                    mission_id: spec.mission_id,
                    task_id: spec.task_id,
                    upload_id: None,
                    name: spec.name,
                    mime_type: spec.mime_type,
                    size: spec.bytes.len() as u64,
                    sha256: hex::encode(digest.finalize()),
                    pinned: false,
                    created_at: timestamp_now(),
                };
                snapshot.artifacts.push(artifact.clone());
                Ok((
                    snapshot,
                    Event::ArtifactPublished { artifact },
                    CommandResult::Created { id: id.to_string() },
                ))
            }
            Command::TaskAccept { task_id } => {
                let task = snapshot
                    .tasks
                    .iter()
                    .find(|task| task.id == task_id)
                    .cloned()
                    .ok_or(OrchestratorError::NotFound)?;
                if task.status != TaskStatus::Review {
                    return Err(OrchestratorError::InvalidTransition(
                        "only a task in review can be accepted".into(),
                    ));
                }
                // A task without a worktree is a manual/fake-provider card
                // and can be accepted immediately. A real worker task is
                // represented by a durable Git operation; the operation
                // worker performs checks, captures the diff, commits, and
                // squash-merges after this transaction has committed.
                if let Some(worktree) = task.worktree.as_deref() {
                    if !is_allowed_path(worktree, &snapshot.server.allowed_project_roots)
                        || !Path::new(worktree).is_dir()
                    {
                        return Err(OrchestratorError::Validation(
                            "task worktree is outside the allowed project roots".into(),
                        ));
                    }
                    if snapshot.operations.iter().any(|operation| {
                        operation.kind == OperationKind::CommitTask
                            && operation.resource == task_id.to_string()
                            && matches!(
                                operation.status,
                                OperationStatus::Queued
                                    | OperationStatus::Running
                                    | OperationStatus::Waiting
                                    | OperationStatus::Recovering
                            )
                    }) {
                        return Err(OrchestratorError::InvalidTransition(
                            "task acceptance is already queued".into(),
                        ));
                    }
                    let now = timestamp_now();
                    let operation = OperationView {
                        id: OperationId::new(),
                        kind: OperationKind::CommitTask,
                        status: OperationStatus::Queued,
                        resource: task_id.to_string(),
                        phase: "validate".into(),
                        attempt: 0,
                        error: None,
                        created_at: now.clone(),
                        updated_at: now,
                    };
                    snapshot.operations.push(operation.clone());
                    return Ok((
                        snapshot,
                        Event::OperationChanged {
                            operation: operation.clone(),
                        },
                        CommandResult::Operation(operation),
                    ));
                }
                let task = snapshot
                    .tasks
                    .iter_mut()
                    .find(|task| task.id == task_id)
                    .expect("task checked");
                task.status = TaskStatus::Done;
                Ok((
                    snapshot,
                    Event::TaskStatusChanged {
                        task_id,
                        status: TaskStatus::Done,
                    },
                    CommandResult::Accepted,
                ))
            }
            Command::DeliverMission { mission_id } => {
                let mission = snapshot
                    .missions
                    .iter()
                    .find(|mission| mission.id == mission_id)
                    .cloned()
                    .ok_or(OrchestratorError::NotFound)?;
                if !matches!(mission.status, MissionStatus::Completed)
                    || snapshot.tasks.iter().any(|task| {
                        task.mission_id == mission_id && task.status != TaskStatus::Done
                    })
                {
                    return Err(OrchestratorError::Validation(
                        "all mission tasks must be accepted before delivery".into(),
                    ));
                }
                if let Some(operation) = snapshot
                    .operations
                    .iter()
                    .find(|operation| {
                        matches!(
                            operation.kind,
                            OperationKind::PushMission | OperationKind::OpenDraftPullRequest
                        ) && operation.resource == mission_id.to_string()
                            && !matches!(
                                operation.status,
                                OperationStatus::Cancelled | OperationStatus::Failed
                            )
                    })
                    .cloned()
                {
                    return Ok((
                        snapshot,
                        Event::OperationChanged {
                            operation: operation.clone(),
                        },
                        CommandResult::Operation(operation),
                    ));
                }
                let now = timestamp_now();
                let operation = OperationView {
                    id: OperationId::new(),
                    kind: OperationKind::PushMission,
                    status: OperationStatus::Queued,
                    resource: mission_id.to_string(),
                    phase: "queued".into(),
                    attempt: 0,
                    error: None,
                    created_at: now.clone(),
                    updated_at: now,
                };
                snapshot.operations.push(operation.clone());
                Ok((
                    snapshot,
                    Event::OperationChanged {
                        operation: operation.clone(),
                    },
                    CommandResult::Operation(operation),
                ))
            }
            Command::OpenTerminal {
                task_id,
                cols,
                rows,
            } => {
                let task = snapshot
                    .tasks
                    .iter()
                    .find(|task| task.id == task_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if !matches!(task.status, TaskStatus::Running | TaskStatus::Review) {
                    return Err(OrchestratorError::Validation(
                        "terminal is available only for an active task".into(),
                    ));
                }
                let cwd = task
                    .worktree
                    .clone()
                    .filter(|path| !path.trim().is_empty())
                    .ok_or_else(|| {
                        OrchestratorError::Validation("terminal requires a task worktree".into())
                    })?;
                if !Path::new(&cwd).is_dir()
                    || !is_allowed_path(&cwd, &snapshot.server.allowed_project_roots)
                {
                    return Err(OrchestratorError::Validation(
                        "terminal worktree is outside the allowed project roots".into(),
                    ));
                }
                let session = TerminalSessionView {
                    id: TerminalSessionId::new(),
                    task_id,
                    cwd,
                    cols: cols.clamp(20, 400),
                    rows: rows.clamp(5, 200),
                    active: true,
                    lease: None,
                };
                snapshot.terminals.push(session.clone());
                Ok((
                    snapshot,
                    Event::TerminalOpened {
                        session: session.clone(),
                    },
                    CommandResult::Terminal(session),
                ))
            }
            Command::TakeControl { session_id } => {
                let session = snapshot
                    .terminals
                    .iter_mut()
                    .find(|session| session.id == session_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if session.lease.as_ref().is_some_and(|lease| {
                    lease.expires_at.parse::<u128>().unwrap_or_default() > now_plus_seconds(0)
                }) {
                    return Err(OrchestratorError::Validation(
                        "terminal is already controlled by another device".into(),
                    ));
                }
                session.lease = None;
                let lease = ControlLeaseView {
                    lease_id: uuid::Uuid::new_v4().to_string(),
                    session_id,
                    actor: actor.clone(),
                    expires_at: format!("{}", now_plus_seconds(30)),
                };
                session.lease = Some(lease.clone());
                Ok((
                    snapshot,
                    Event::TerminalLeaseChanged { lease },
                    CommandResult::Accepted,
                ))
            }
            Command::RenewControl {
                session_id,
                lease_id,
            } => {
                let session = snapshot
                    .terminals
                    .iter_mut()
                    .find(|session| session.id == session_id)
                    .ok_or(OrchestratorError::NotFound)?;
                let lease = session
                    .lease
                    .as_mut()
                    .filter(|lease| lease.lease_id == lease_id && same_actor(&lease.actor, actor))
                    .ok_or(OrchestratorError::Forbidden)?;
                lease.expires_at = format!("{}", now_plus_seconds(30));
                let renewed = lease.clone();
                Ok((
                    snapshot,
                    Event::TerminalLeaseChanged { lease: renewed },
                    CommandResult::Accepted,
                ))
            }
            Command::ReleaseControl {
                session_id,
                lease_id,
            } => {
                let session = snapshot
                    .terminals
                    .iter_mut()
                    .find(|session| session.id == session_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if session.lease.as_ref().is_none_or(|lease| {
                    lease.lease_id != lease_id || !same_actor(&lease.actor, actor)
                }) {
                    return Err(OrchestratorError::Forbidden);
                }
                session.lease = None;
                let lease = ControlLeaseView {
                    lease_id,
                    session_id,
                    actor: actor.clone(),
                    expires_at: "released".into(),
                };
                Ok((
                    snapshot,
                    Event::TerminalLeaseChanged { lease },
                    CommandResult::Accepted,
                ))
            }
            Command::BeginArtifactUpload(spec) => {
                if spec.size > MAX_ARTIFACT_BYTES
                    || spec.name.trim().is_empty()
                    || spec.name.len() > 256
                    || spec.mime_type.trim().is_empty()
                    || spec.mime_type.len() > 256
                    || spec.sha256.len() != 64
                    || !spec.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    return Err(OrchestratorError::Validation(
                        "artifact upload metadata is invalid".into(),
                    ));
                }
                let mission = snapshot
                    .missions
                    .iter()
                    .find(|mission| mission.id == spec.mission_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if let Some(task_id) = spec.task_id
                    && !snapshot
                        .tasks
                        .iter()
                        .any(|task| task.id == task_id && task.mission_id == mission.id)
                {
                    return Err(OrchestratorError::NotFound);
                }
                let upload = ArtifactUploadView {
                    id: UploadId::new(),
                    spec,
                    received: 0,
                    completed: false,
                    created_at: timestamp_now(),
                    expires_at: format!("{}", now_plus_seconds(600)),
                };
                snapshot.uploads.push(upload.clone());
                Ok((
                    snapshot,
                    Event::ArtifactUploadStarted {
                        upload: upload.clone(),
                    },
                    CommandResult::Upload(upload),
                ))
            }
            Command::FinalizeArtifactUpload {
                upload_id,
                sha256,
                size,
            } => {
                let upload = snapshot
                    .uploads
                    .iter_mut()
                    .find(|upload| upload.id == upload_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if upload.completed {
                    return Err(OrchestratorError::InvalidTransition(
                        "artifact upload is already complete".into(),
                    ));
                }
                let expires_at = upload.expires_at.parse::<u128>().map_err(|_| {
                    OrchestratorError::Validation(
                        "artifact upload expiry metadata is invalid".into(),
                    )
                })?;
                if expires_at <= epoch_seconds() as u128 {
                    return Err(OrchestratorError::InvalidTransition(
                        "artifact upload has expired; start a new upload".into(),
                    ));
                }
                if upload.spec.size != size || !sha256.eq_ignore_ascii_case(&upload.spec.sha256) {
                    return Err(OrchestratorError::Validation(
                        "artifact upload is incomplete or has the wrong digest".into(),
                    ));
                }
                let bytes = self
                    .store
                    .artifact_upload_bytes(upload_id)
                    .await?
                    .ok_or_else(|| {
                        OrchestratorError::Validation(
                            "artifact upload bytes are missing; retry the upload".into(),
                        )
                    })?;
                if bytes.len() as u64 != size {
                    return Err(OrchestratorError::Validation(
                        "artifact upload byte count does not match metadata".into(),
                    ));
                }
                // Chunk writes live in the upload table and intentionally do
                // not emit one event per network packet. Refresh the
                // projection's received count from those durable bytes at
                // finalize time so a reconnect or a stale snapshot cannot
                // make a complete upload appear incomplete.
                upload.received = bytes.len() as u64;
                let mut digest = Sha256::new();
                digest.update(&bytes);
                let actual_sha256 = hex::encode(digest.finalize());
                if !actual_sha256.eq_ignore_ascii_case(&upload.spec.sha256)
                    || !actual_sha256.eq_ignore_ascii_case(&sha256)
                {
                    return Err(OrchestratorError::Validation(
                        "artifact upload digest verification failed".into(),
                    ));
                }
                let artifact = ArtifactView {
                    id: ArtifactId::from(upload_id.0),
                    mission_id: upload.spec.mission_id,
                    task_id: upload.spec.task_id,
                    upload_id: Some(upload_id),
                    name: upload.spec.name.clone(),
                    mime_type: upload.spec.mime_type.clone(),
                    size,
                    sha256: actual_sha256,
                    pinned: false,
                    created_at: timestamp_now(),
                };
                upload.completed = true;
                if !snapshot
                    .artifacts
                    .iter()
                    .any(|candidate| candidate.id == artifact.id)
                {
                    snapshot.artifacts.push(artifact.clone());
                }
                Ok((
                    snapshot,
                    Event::ArtifactPublished {
                        artifact: artifact.clone(),
                    },
                    CommandResult::Created {
                        id: artifact.id.to_string(),
                    },
                ))
            }
            Command::SubmitSupervisorPlan {
                mission_id,
                proposal,
            } => {
                validate_supervisor_proposal(&snapshot, mission_id, &proposal)?;
                Ok((
                    snapshot,
                    Event::SupervisorPlanProposed {
                        mission_id,
                        proposal,
                    },
                    CommandResult::Accepted,
                ))
            }
            Command::CancelOperation { operation_id } => {
                let index = snapshot
                    .operations
                    .iter()
                    .find(|operation| operation.id == operation_id)
                    .map(|operation| {
                        snapshot
                            .operations
                            .iter()
                            .position(|candidate| candidate.id == operation.id)
                            .unwrap_or_default()
                    })
                    .ok_or(OrchestratorError::NotFound)?;
                let mut operation = snapshot.operations[index].clone();
                if matches!(
                    operation.status,
                    OperationStatus::Succeeded | OperationStatus::Cancelled
                ) {
                    return Ok((
                        snapshot,
                        Event::OperationChanged {
                            operation: operation.clone(),
                        },
                        CommandResult::Operation(operation.clone()),
                    ));
                }
                operation.status = OperationStatus::Cancelled;
                operation.updated_at = timestamp_now();
                operation.error = Some("cancelled by actor".into());
                snapshot.operations[index] = operation.clone();
                Ok((
                    snapshot,
                    Event::OperationChanged {
                        operation: operation.clone(),
                    },
                    CommandResult::Operation(operation.clone()),
                ))
            }
            Command::RetryOperation { operation_id } => {
                let index = snapshot
                    .operations
                    .iter()
                    .position(|operation| operation.id == operation_id)
                    .ok_or(OrchestratorError::NotFound)?;
                let operation = &mut snapshot.operations[index];
                if operation.status != OperationStatus::Failed
                    && !(operation.status == OperationStatus::Waiting
                        && operation.phase == "awaiting-helper")
                {
                    return Err(OrchestratorError::InvalidTransition(
                        "only a failed or helper-waiting operation can be retried".into(),
                    ));
                }
                operation.status = OperationStatus::Queued;
                operation.phase = "retry-queued".into();
                operation.error = None;
                operation.updated_at = timestamp_now();
                let operation = operation.clone();
                Ok((
                    snapshot,
                    Event::OperationChanged {
                        operation: operation.clone(),
                    },
                    CommandResult::Operation(operation),
                ))
            }
            Command::CheckForUpdate => {
                let mut update = snapshot.update.clone().unwrap_or(UpdateView {
                    id: UpdateId::new(),
                    version: env!("CARGO_PKG_VERSION").into(),
                    state: UpdateState::Idle,
                    target: String::new(),
                    size: 0,
                    sha256: String::new(),
                    error: None,
                    checked_at: timestamp_now(),
                });
                // Tests and air-gapped operators can point the daemon at a
                // local, already-downloaded manifest/signature pair. Normal
                // production checks fetch Frank's HTTPS feed at the server
                // edge. In both cases signature verification happens before
                // any manifest fields influence state.
                let result = if let (Some(manifest_path), Some(signature_path)) = (
                    std::env::var_os("FRANK_UPDATE_MANIFEST"),
                    std::env::var_os("FRANK_UPDATE_SIGNATURE"),
                ) {
                    (|| -> std::result::Result<_, String> {
                        let manifest =
                            std::fs::read(manifest_path).map_err(|error| error.to_string())?;
                        let signature = std::fs::read_to_string(signature_path)
                            .map_err(|error| error.to_string())?;
                        frank_update::parse_verified_manifest(
                            &manifest,
                            &signature,
                            &base64::engine::general_purpose::STANDARD
                                .decode(frank_update::EMBEDDED_PUBLIC_KEY_B64)
                                .map_err(|error| error.to_string())?,
                        )
                        .map_err(|error| error.to_string())
                    })()
                } else {
                    fetch_update_manifest()
                        .await
                        .map_err(|error| error.to_string())
                };
                match result {
                    Ok(manifest) => {
                        let current = env!("CARGO_PKG_VERSION");
                        let target = std::env::var("FRANK_UPDATE_TARGET")
                            .unwrap_or_else(|_| current_update_target());
                        let package_kind = std::env::var("FRANK_UPDATE_PACKAGE_KIND")
                            .unwrap_or_else(|_| current_update_package_kind());
                        // A signed manifest may carry entries for every
                        // release target, but a host must never silently
                        // fall back to the first entry when its target or
                        // package kind is absent. Doing so could stage an
                        // artifact for another architecture/OS and turn a
                        // wrong-target release into a destructive swap.
                        let artifact = select_update_artifact(&manifest, &target, &package_kind);
                        let compatible = manifest.accepts_protocol(PROTOCOL_VERSION)
                            && manifest.rejects_downgrade_from(current).is_ok()
                            && artifact.is_some();
                        if !compatible {
                            update.state = UpdateState::Failed;
                            update.error =
                                Some("update manifest is incompatible with this host".into());
                        } else if let Some(artifact) = artifact {
                            update.version = manifest.frank_version.clone();
                            update.target = artifact.target.clone();
                            update.size = artifact.size;
                            update.sha256 = artifact.sha256.clone();
                            update.state = UpdateState::Available;
                            update.error = None;
                        }
                        update.checked_at = timestamp_now();
                    }
                    Err(_error) => {
                        update.state = UpdateState::Failed;
                        update.error = Some("update manifest verification failed".into());
                        // Keep detailed diagnostics out of the wire snapshot;
                        // they may contain a local path, URL, or proxy data.
                        update.checked_at = timestamp_now();
                    }
                }
                snapshot.update = Some(update.clone());
                Ok((
                    snapshot,
                    Event::UpdateStateChanged {
                        update: update.clone(),
                    },
                    CommandResult::Update(update),
                ))
            }
            Command::PrepareUpdate { version } => {
                if version.trim().is_empty() || version.len() > 64 {
                    return Err(OrchestratorError::Validation(
                        "update version is invalid".into(),
                    ));
                }
                let update = snapshot
                    .update
                    .as_ref()
                    .filter(|update| update.version == version)
                    .ok_or_else(|| {
                        OrchestratorError::Validation(
                            "no verified update manifest is available for this version".into(),
                        )
                    })?;
                if !matches!(update.state, UpdateState::Available | UpdateState::Failed) {
                    if update.state == UpdateState::Staged
                        && let Some(operation) = snapshot
                            .operations
                            .iter()
                            .find(|operation| {
                                operation.kind == OperationKind::StageUpdate
                                    && serde_json::from_str::<StageUpdateOperation>(
                                        &operation.resource,
                                    )
                                    .is_ok_and(|request| request.update_id == update.id)
                                    && operation.status == OperationStatus::Succeeded
                            })
                            .cloned()
                    {
                        return Ok((
                            snapshot,
                            Event::OperationChanged {
                                operation: operation.clone(),
                            },
                            CommandResult::Operation(operation),
                        ));
                    }
                    return Err(OrchestratorError::InvalidTransition(
                        "update is not available for staging".into(),
                    ));
                }
                if let Some(operation) = snapshot
                    .operations
                    .iter()
                    .find(|operation| {
                        operation.kind == OperationKind::StageUpdate
                            && serde_json::from_str::<StageUpdateOperation>(&operation.resource)
                                .is_ok_and(|request| request.update_id == update.id)
                            && !matches!(
                                operation.status,
                                OperationStatus::Failed | OperationStatus::Cancelled
                            )
                    })
                    .cloned()
                {
                    return Ok((
                        snapshot,
                        Event::OperationChanged {
                            operation: operation.clone(),
                        },
                        CommandResult::Operation(operation),
                    ));
                }
                let now = timestamp_now();
                let operation = OperationView {
                    id: OperationId::new(),
                    kind: OperationKind::StageUpdate,
                    status: OperationStatus::Queued,
                    resource: serde_json::to_string(&StageUpdateOperation {
                        update_id: update.id,
                    })
                    .map_err(|error| OrchestratorError::Validation(error.to_string()))?,
                    phase: "queued".into(),
                    attempt: 0,
                    error: None,
                    created_at: now.clone(),
                    updated_at: now,
                };
                snapshot.operations.push(operation.clone());
                Ok((
                    snapshot,
                    Event::OperationChanged {
                        operation: operation.clone(),
                    },
                    CommandResult::Operation(operation),
                ))
            }
            Command::ApplyUpdate { update_id } => {
                let update = snapshot
                    .update
                    .as_mut()
                    .filter(|update| update.id == update_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if update.state != UpdateState::Staged {
                    return Err(OrchestratorError::InvalidTransition(
                        "only a staged update can be applied".into(),
                    ));
                }
                update.state = UpdateState::Applying;
                update.checked_at = timestamp_now();
                let update = update.clone();
                // Applying a bundle is a host side effect. Persist its
                // operation in the same transaction as the state transition
                // so a response loss or daemon crash cannot strand an update
                // in `Applying` without a retryable journal entry.
                let operation = snapshot
                    .operations
                    .iter()
                    .find(|operation| {
                        operation.kind == OperationKind::HostUpdate
                            && serde_json::from_str::<HostUpdateOperation>(&operation.resource)
                                .is_ok_and(|request| {
                                    request.update_id == update_id
                                        && request.action == HostUpdateAction::Apply
                                })
                            && !matches!(
                                operation.status,
                                OperationStatus::Succeeded
                                    | OperationStatus::Cancelled
                                    | OperationStatus::Failed
                            )
                    })
                    .cloned()
                    .unwrap_or_else(|| {
                        let now = timestamp_now();
                        let operation = OperationView {
                            id: OperationId::new(),
                            kind: OperationKind::HostUpdate,
                            status: OperationStatus::Queued,
                            resource: serde_json::to_string(&HostUpdateOperation {
                                update_id,
                                action: HostUpdateAction::Apply,
                            })
                            .unwrap_or_else(|_| update_id.to_string()),
                            phase: "queued".into(),
                            attempt: 0,
                            error: None,
                            created_at: now.clone(),
                            updated_at: now,
                        };
                        snapshot.operations.push(operation.clone());
                        operation
                    });
                Ok((
                    snapshot,
                    Event::UpdateStateChanged {
                        update: update.clone(),
                    },
                    CommandResult::Operation(operation),
                ))
            }
            Command::RollbackUpdate => {
                let update = snapshot
                    .update
                    .as_mut()
                    .ok_or(OrchestratorError::NotFound)?;
                if !matches!(update.state, UpdateState::Applying | UpdateState::Failed) {
                    return Err(OrchestratorError::InvalidTransition(
                        "no failed or applying update can be rolled back".into(),
                    ));
                }
                // Rollback is also a host side effect. Keep the update in an
                // applying state until frank-updater confirms the swap, and
                // persist a separate journal row for idempotent recovery.
                update.state = UpdateState::Applying;
                update.error = Some("rollback queued".into());
                update.checked_at = timestamp_now();
                let update = update.clone();
                let operation = snapshot
                    .operations
                    .iter()
                    .find(|operation| {
                        operation.kind == OperationKind::HostUpdate
                            && serde_json::from_str::<HostUpdateOperation>(&operation.resource)
                                .is_ok_and(|request| {
                                    request.update_id == update.id
                                        && request.action == HostUpdateAction::Rollback
                                })
                            && !matches!(
                                operation.status,
                                OperationStatus::Succeeded
                                    | OperationStatus::Cancelled
                                    | OperationStatus::Failed
                            )
                    })
                    .cloned()
                    .unwrap_or_else(|| {
                        let now = timestamp_now();
                        let operation = OperationView {
                            id: OperationId::new(),
                            kind: OperationKind::HostUpdate,
                            status: OperationStatus::Queued,
                            resource: serde_json::to_string(&HostUpdateOperation {
                                update_id: update.id,
                                action: HostUpdateAction::Rollback,
                            })
                            .unwrap_or_else(|_| update.id.to_string()),
                            phase: "rollback-queued".into(),
                            attempt: 0,
                            error: None,
                            created_at: now.clone(),
                            updated_at: now,
                        };
                        snapshot.operations.push(operation.clone());
                        operation
                    });
                Ok((
                    snapshot,
                    Event::UpdateStateChanged {
                        update: update.clone(),
                    },
                    CommandResult::Operation(operation),
                ))
            }
            Command::CloseTerminal { session_id } => {
                let session = snapshot
                    .terminals
                    .iter_mut()
                    .find(|session| session.id == session_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if let Some(lease) = &session.lease
                    && !same_actor(&lease.actor, actor)
                    && lease.expires_at.parse::<u128>().unwrap_or_default() > now_plus_seconds(0)
                {
                    return Err(OrchestratorError::Forbidden);
                }
                session.active = false;
                session.lease = None;
                Ok((
                    snapshot,
                    Event::TerminalClosed { session_id },
                    CommandResult::Accepted,
                ))
            }
        }
    }

    async fn pause_or_resume(
        &self,
        mut snapshot: Snapshot,
        mission_id: MissionId,
        status: MissionStatus,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        let mission = snapshot
            .missions
            .iter_mut()
            .find(|mission| mission.id == mission_id)
            .ok_or(OrchestratorError::NotFound)?;
        if !mission.status.can_transition_to(status) {
            return Err(OrchestratorError::InvalidTransition(format!(
                "mission cannot transition from {:?} to {:?}",
                mission.status, status
            )));
        }
        mission.status = status;
        mission.updated_at = timestamp_now();
        Ok((
            snapshot,
            Event::MissionStatusChanged { mission_id, status },
            CommandResult::Accepted,
        ))
    }

    pub async fn record_usage(
        &self,
        usage: UsageView,
        measured_budget: Option<&Budget>,
    ) -> Result<EventEnvelope> {
        let telemetry = UsageTelemetry {
            measured_input_tokens: usage.measured_input_tokens,
            measured_output_tokens: usage.measured_output_tokens,
            estimated_input_tokens: usage.estimated_input_tokens,
            estimated_output_tokens: usage.estimated_output_tokens,
            cost_micros: usage.cost_micros,
        };
        let exceeded = {
            let mut budgets = self.budgets.lock().await;
            budgets.record(usage.scope_id.clone(), &telemetry);
            measured_budget
                .filter(|budget| !budgets.allows_measured(&usage.scope_id, budget))
                .is_some()
        };
        let event = self.commit_usage_event(usage).await?;
        if exceeded {
            return Err(OrchestratorError::BudgetExceeded);
        }
        Ok(event)
    }

    /// Record one provider telemetry frame and enforce task, mission, and
    /// agent budgets against measured quantities. Estimates are retained for
    /// warning/ledger display but never cause a hard stop.
    async fn record_runtime_usage(
        &self,
        usage: UsageView,
        mission_id: MissionId,
        agent_id: AgentId,
    ) -> Result<Option<BudgetScope>> {
        let telemetry = UsageTelemetry {
            measured_input_tokens: usage.measured_input_tokens,
            measured_output_tokens: usage.measured_output_tokens,
            estimated_input_tokens: usage.estimated_input_tokens,
            estimated_output_tokens: usage.estimated_output_tokens,
            cost_micros: usage.cost_micros,
        };
        let snapshot = self.store.snapshot().await?;
        let task_budget = snapshot
            .tasks
            .iter()
            .find(|task| task.id.to_string() == usage.scope_id)
            .map(|task| task.budget.clone())
            .ok_or(OrchestratorError::NotFound)?;
        let mission_budget = snapshot
            .missions
            .iter()
            .find(|mission| mission.id == mission_id)
            .map(|mission| mission.budget.clone())
            .ok_or(OrchestratorError::NotFound)?;
        let agent_budget = snapshot
            .agents
            .iter()
            .find(|agent| agent.id == agent_id)
            .map(|agent| agent.budget.clone())
            .ok_or(OrchestratorError::NotFound)?;
        let task_id = usage.scope_id.clone();
        let mission_id_text = mission_id.to_string();
        let agent_id_text = agent_id.to_string();
        // A task normally gets a clock when its provider session starts. If
        // telemetry arrives during crash recovery before that hook runs,
        // initialize the three scopes here rather than silently treating the
        // first frame as infinitely old.
        self.start_scope_clock(task_id.clone()).await;
        self.start_scope_clock(mission_id_text.clone()).await;
        self.start_scope_clock(agent_id_text.clone()).await;
        let task_elapsed = self.scope_elapsed_seconds(&task_id).await;
        let mission_elapsed = self.scope_elapsed_seconds(&mission_id_text).await;
        let agent_elapsed = self.scope_elapsed_seconds(&agent_id_text).await;
        let exceeded = {
            let mut budgets = self.budgets.lock().await;
            budgets.rebuild_from_snapshot(&snapshot);
            budgets.record(task_id.clone(), &telemetry);
            budgets.record(mission_id_text.clone(), &telemetry);
            budgets.record(agent_id_text.clone(), &telemetry);
            [
                (BudgetScope::Task, task_id, task_budget, task_elapsed),
                (
                    BudgetScope::Mission,
                    mission_id_text,
                    mission_budget,
                    mission_elapsed,
                ),
                (
                    BudgetScope::Agent,
                    agent_id_text,
                    agent_budget,
                    agent_elapsed,
                ),
            ]
            .into_iter()
            .find_map(|(scope, scope_id, budget, elapsed)| {
                let time_exceeded = budget.time_seconds.is_some_and(|limit| elapsed >= limit);
                (time_exceeded || !budgets.allows_measured(&scope_id, &budget)).then_some(scope)
            })
        };
        self.commit_usage_event(usage).await?;
        if let Some(scope) = exceeded {
            let snapshot = self.store.snapshot().await?;
            self.store
                .commit_command(
                    CommandId::new(),
                    Some(snapshot.revision),
                    ActorRef::system(),
                    Event::BudgetPaused {
                        scope,
                        reason: "measured budget exceeded; task paused".into(),
                    },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await?;
        }
        Ok(exceeded)
    }

    async fn commit_usage_event(&self, usage: UsageView) -> Result<EventEnvelope> {
        let command_id = CommandId::new();
        for _ in 0..3 {
            let mut snapshot = self.store.snapshot().await?;
            // Event projections are durable, but reconnecting clients load
            // this snapshot first. Include the row in the authoritative
            // snapshot so a restart cannot make the ledger appear empty until
            // a live event happens to arrive.
            snapshot.usage.push(usage.clone());
            match self
                .store
                .commit_command(
                    command_id,
                    Some(snapshot.revision),
                    ActorRef::system(),
                    Event::UsageRecorded {
                        usage: usage.clone(),
                    },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await
            {
                Ok(commit) => return Ok(commit.event),
                Err(StoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }
}

fn current_update_target() -> String {
    let arch = std::env::consts::ARCH;
    let os = match std::env::consts::OS {
        "macos" => "apple-darwin",
        "windows" => "pc-windows-msvc",
        "linux" => "unknown-linux-gnu",
        other => other,
    };
    format!("{arch}-{os}")
}

/// Download and verify the signed release feed. The response advertises and
/// enforces a hard body cap before parsing so an endpoint cannot make the
/// daemon retain an unbounded manifest. Signature verification happens before
/// the JSON manifest is deserialized or any artifact metadata is used.
async fn fetch_update_manifest() -> std::result::Result<frank_update::UpdateManifest, String> {
    const MAX_MANIFEST_BYTES: usize = 256 * 1024;
    const MAX_SIGNATURE_BYTES: usize = 64 * 1024;
    if std::env::var_os("FRANK_UPDATE_DISABLE_NETWORK").is_some() {
        return Err("network update checks are disabled".into());
    }
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(20))
        .user_agent(format!("frank/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|error| error.to_string())?;
    let manifest_response = client
        .get(frank_update::UPDATE_FEED_URL)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let manifest = read_bounded_http_body(manifest_response, MAX_MANIFEST_BYTES).await?;
    let signature_response = client
        .get(frank_update::UPDATE_SIGNATURE_URL)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let signature = read_bounded_http_body(signature_response, MAX_SIGNATURE_BYTES).await?;
    let signature = std::str::from_utf8(&signature).map_err(|error| error.to_string())?;
    let public_key = base64::engine::general_purpose::STANDARD
        .decode(frank_update::EMBEDDED_PUBLIC_KEY_B64)
        .map_err(|error| error.to_string())?;
    frank_update::parse_verified_manifest(&manifest, signature, &public_key)
        .map_err(|error| error.to_string())
}

/// Fetch one manifest-selected payload with the same bounded streaming rules
/// used for the signed feed. Tests and air-gapped operators may provide a
/// local file through `FRANK_UPDATE_ARTIFACT`; that path is checked as a
/// regular file and still passes through the exact digest/size verifier.
async fn download_update_artifact(artifact: &frank_update::UpdateArtifact) -> Result<Vec<u8>> {
    let bytes = if let Some(path) = std::env::var_os("FRANK_UPDATE_ARTIFACT") {
        let path = PathBuf::from(path);
        let metadata = std::fs::symlink_metadata(&path).map_err(|error| {
            OrchestratorError::Validation(format!(
                "update artifact could not be inspected: {error}"
            ))
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(OrchestratorError::Validation(
                "update artifact path is not a regular file".into(),
            ));
        }
        if metadata.len() > frank_update::MAX_UPDATE_ARTIFACT_BYTES {
            return Err(OrchestratorError::Validation(
                "update artifact exceeds the configured size cap".into(),
            ));
        }
        let file = std::fs::File::open(&path).map_err(|error| {
            OrchestratorError::Validation(format!("update artifact could not be opened: {error}"))
        })?;
        let mut bytes = Vec::new();
        let mut limited = file.take(frank_update::MAX_UPDATE_ARTIFACT_BYTES.saturating_add(1));
        limited.read_to_end(&mut bytes).map_err(|error| {
            OrchestratorError::Validation(format!("update artifact could not be read: {error}"))
        })?;
        bytes
    } else {
        let client = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(300))
            .user_agent(format!("frank/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
        let response = client
            .get(&artifact.url)
            .send()
            .await
            .map_err(|error| OrchestratorError::Validation(error.to_string()))?
            .error_for_status()
            .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
        let cap = usize::try_from(frank_update::MAX_UPDATE_ARTIFACT_BYTES).unwrap_or(usize::MAX);
        read_bounded_http_body(response, cap)
            .await
            .map_err(OrchestratorError::Validation)?
    };
    frank_update::verify_artifact_bytes(artifact, &bytes)
        .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
    Ok(bytes)
}

/// Resolve the daemon-owned staging root without exposing it through a
/// snapshot. Package/service deployments can set an explicit root; local
/// development falls back to a sibling of the current bundle, then the v1
/// database directory, and finally a private temporary root for in-memory
/// tests.
fn update_staging_root(store: &Store) -> PathBuf {
    if let Some(path) = std::env::var_os("FRANK_UPDATE_STAGING") {
        return PathBuf::from(path);
    }
    if let Some(current) = std::env::var_os("FRANK_UPDATE_CURRENT")
        && let Some(parent) = Path::new(&current).parent()
    {
        return parent.join("staging");
    }
    if let Some(database) = store.database_path()
        && let Some(parent) = database.parent()
    {
        return parent.join("updates").join("staging");
    }
    std::env::temp_dir()
        .join("frank")
        .join("v1")
        .join("updates")
        .join("staging")
}

fn current_update_package_kind() -> String {
    match std::env::consts::OS {
        "macos" => "dmg",
        "windows" => "msi",
        _ => "tar.gz",
    }
    .into()
}

fn select_update_artifact<'a>(
    manifest: &'a frank_update::UpdateManifest,
    target: &str,
    package_kind: &str,
) -> Option<&'a frank_update::UpdateArtifact> {
    manifest
        .artifacts
        .iter()
        .find(|artifact| artifact.target == target && artifact.package_kind == package_kind)
}

async fn read_bounded_http_body(
    mut response: reqwest::Response,
    cap: usize,
) -> std::result::Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|length| length > cap as u64)
    {
        return Err("update response exceeds the configured size cap".into());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
        if chunk.len() > cap || body.len().saturating_add(chunk.len()) > cap {
            return Err("update response exceeds the configured size cap".into());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
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
            priority: 0,
            budget: Budget::unlimited(),
            status: TaskStatus::Backlog,
            assigned_agent: None,
            attempt: 0,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            worktree: None,
            branch: None,
            result_artifact: None,
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
            priority: 0,
            assigned_agent: None,
            budget: Budget::unlimited(),
        };
        let tasks = vec![task(dependency, vec![])];
        assert!(validate_task_spec(&spec, &tasks).is_err());
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
    fn scheduler_enforces_total_and_provider_caps() {
        let mut scheduler = Scheduler {
            limits: SchedulerLimits {
                max_concurrency: 1,
                max_provider_concurrency: 1,
            },
            ..Default::default()
        };
        assert!(scheduler.start(TaskId::new(), Provider::Codex));
        assert!(!scheduler.can_start(Provider::Claude));
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
            supervisor_provider: Provider::Codex,
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
    async fn usage_event_is_retained_in_the_authoritative_snapshot() {
        let store = Store::open_in_memory().await.unwrap();
        let orchestrator = Orchestrator::new(store.clone());
        let usage = UsageView {
            id: AttemptId::new(),
            scope: BudgetScope::Task,
            scope_id: TaskId::new().to_string(),
            provider: Provider::Codex,
            measured_input_tokens: Some(4),
            measured_output_tokens: Some(6),
            estimated_input_tokens: None,
            estimated_output_tokens: None,
            cost_micros: Some(12),
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
            supervisor_provider: Provider::Codex,
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
            supervisor_provider: Provider::Codex,
            supervisor_session_id: None,
            branch: "frank/mission-lease".into(),
            budget: Budget::unlimited(),
            created_at: timestamp_now(),
            updated_at: timestamp_now(),
        });
        snapshot.agents.push(AgentView {
            id: agent_id,
            display_name: "worker".into(),
            template: AgentTemplate::Builder,
            provider: Provider::Codex,
            model: None,
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
            archived: false,
        });
        snapshot.tasks.push(TaskView {
            id: task_id,
            mission_id,
            title: "running task".into(),
            objective: "continue".into(),
            dependencies: Vec::new(),
            priority: 0,
            budget: Budget::unlimited(),
            status: TaskStatus::Running,
            assigned_agent: Some(agent_id),
            attempt: 0,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            worktree: None,
            branch: None,
            result_artifact: None,
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
