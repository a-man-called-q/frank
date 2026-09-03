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
mod budget_enforce;
mod capability;
pub mod git;
mod helpers;
mod mailbox;
mod messaging;
mod operations;
mod reduce;
mod scheduler;
mod update_flow;
mod validate;

pub use budget::{BudgetLedger, UsageTotals};
use capability::*;
use helpers::*;
pub use mailbox::Mailbox;
use operations::*;
pub use scheduler::{Scheduler, SchedulerLimits};
use update_flow::*;
pub use validate::retry_after_failure;
use validate::*;

pub mod supervisor;

use crate::git::GitWorkflow;
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

    /// Route a command to its domain reducer.
    ///
    /// Dispatch only: matching on a reference keeps `command` intact so the
    /// reducer that handles it receives it by value, exactly as the single
    /// 1902-line match used to.
    async fn reduce(
        &self,
        snapshot: Snapshot,
        command: Command,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        match &command {
            Command::Pair(..) | Command::UpdateSettings { .. } => {
                self.reduce_settings(snapshot, command, actor).await
            }
            Command::CreateProject(..)
            | Command::CloneProject { .. }
            | Command::ArchiveProject { .. } => self.reduce_project(snapshot, command, actor).await,
            Command::CreateAgent(..)
            | Command::UpdateAgent { .. }
            | Command::ArchiveAgent { .. } => self.reduce_agent(snapshot, command, actor).await,
            Command::CreateMission { .. }
            | Command::SetMissionStatus { .. }
            | Command::PauseMission { .. }
            | Command::ResumeMission { .. }
            | Command::DeliverMission { .. }
            | Command::SubmitSupervisorPlan { .. } => {
                self.reduce_mission(snapshot, command, actor).await
            }
            Command::CreateTask(..)
            | Command::UpdateTask { .. }
            | Command::SetTaskStatus { .. }
            | Command::AssignTask { .. }
            | Command::TaskAccept { .. } => self.reduce_task(snapshot, command, actor).await,
            Command::SendMessage(..)
            | Command::AckMessage { .. }
            | Command::CompleteMessage { .. } => {
                self.reduce_message(snapshot, command, actor).await
            }
            Command::RequestApproval(..)
            | Command::DecideApproval { .. }
            | Command::AdjustBudget { .. } => self.reduce_approval(snapshot, command, actor).await,
            Command::ProposeMemory { .. } | Command::ReadMemory { .. } => {
                self.reduce_memory(snapshot, command, actor).await
            }
            Command::PublishArtifact(..)
            | Command::BeginArtifactUpload(..)
            | Command::FinalizeArtifactUpload { .. } => {
                self.reduce_artifact(snapshot, command, actor).await
            }
            Command::OpenTerminal { .. }
            | Command::TakeControl { .. }
            | Command::RenewControl { .. }
            | Command::ReleaseControl { .. }
            | Command::CloseTerminal { .. } => self.reduce_terminal(snapshot, command, actor).await,
            Command::CancelOperation { .. } | Command::RetryOperation { .. } => {
                self.reduce_operation(snapshot, command, actor).await
            }
            Command::CheckForUpdate
            | Command::PrepareUpdate { .. }
            | Command::ApplyUpdate { .. }
            | Command::RollbackUpdate => self.reduce_update(snapshot, command, actor).await,
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
