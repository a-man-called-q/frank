//! Frank 1.0 wire contract.
//!
//! This crate intentionally contains data transfer objects only.  It has no
//! filesystem, provider, GUI, or database dependency, which makes the same
//! contract safe to use from `frankd`, the native client, the CLI, and the
//! scoped agent MCP bridge.

use std::fmt;

use serde::{Deserialize, Serialize};

mod command;
mod error;
mod event;
mod ids;

pub use command::*;
pub use error::*;
pub use event::*;
pub use ids::*;

pub const PROTOCOL_VERSION: u16 = 1;
pub const MIN_COMPATIBLE_CLIENT: u16 = 1;
pub const MAX_COMMAND_BODY_BYTES: usize = 256 * 1024;
pub const MAX_MESSAGE_BODY_BYTES: usize = 64 * 1024;
pub const MAX_TERMINAL_FRAME_BYTES: usize = 256 * 1024;
pub const MAX_ARTIFACT_BYTES: u64 = 256 * 1024 * 1024;

/// Monotonic sequence attached to a terminal frame.  Keeping it distinct
/// from an event sequence prevents a reconnecting terminal from accidentally
/// treating a server event as terminal output (or vice versa).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TerminalSequence(pub u64);

impl TerminalSequence {
    pub const ZERO: Self = Self(0);

    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl Default for TerminalSequence {
    fn default() -> Self {
        Self::ZERO
    }
}

impl fmt::Display for TerminalSequence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationStatus {
    Queued,
    Running,
    Waiting,
    Succeeded,
    Failed,
    Cancelled,
    Recovering,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    CloneProject,
    CreateWorktree,
    RunChecks,
    CommitTask,
    MergeTask,
    PushMission,
    OpenDraftPullRequest,
    ArtifactUpload,
    ServiceApply,
    NetworkApply,
    HostUpdate,
    StageUpdate,
    ArtifactPrune,
    WriteMemory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationView {
    pub id: OperationId,
    pub kind: OperationKind,
    pub status: OperationStatus,
    pub resource: String,
    pub phase: String,
    pub attempt: u16,
    pub error: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupervisorTaskProposal {
    pub client_key: String,
    pub title: String,
    pub objective: String,
    pub dependencies: Vec<String>,
    pub priority: i32,
    /// Optional ordered candidates supplied by the supervisor. The daemon
    /// still validates every id and may leave the task unassigned when no
    /// candidate is currently eligible.
    #[serde(default)]
    pub candidate_agents: Vec<AgentId>,
    pub assigned_agent: Option<AgentId>,
    /// The narrowest policy the task requires. A worker's effective policy is
    /// always the intersection of server/project/mission/profile policy; the
    /// daemon rejects a proposal that asks for capabilities the selected
    /// profile cannot provide.
    #[serde(default)]
    pub policy_requirement: Option<AgentPolicy>,
    pub budget: Budget,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupervisorPlanProposal {
    pub mission_id: MissionId,
    pub summary: String,
    pub tasks: Vec<SupervisorTaskProposal>,
    pub reasoning: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactUploadSpec {
    pub mission_id: MissionId,
    pub task_id: Option<TaskId>,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactUploadView {
    pub id: UploadId,
    pub spec: ArtifactUploadSpec,
    pub received: u64,
    pub completed: bool,
    pub created_at: Timestamp,
    pub expires_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateState {
    Idle,
    Available,
    Staged,
    Applying,
    Succeeded,
    Failed,
    RolledBack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateView {
    pub id: UpdateId,
    pub version: String,
    pub state: UpdateState,
    pub target: String,
    pub size: u64,
    pub sha256: String,
    pub error: Option<String>,
    pub checked_at: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceRole {
    Owner,
    Operator,
    Observer,
}

impl DeviceRole {
    pub fn can_mutate(self) -> bool {
        !matches!(self, Self::Observer)
    }

    pub fn can_admin(self) -> bool {
        matches!(self, Self::Owner)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActorKind {
    Device,
    Agent,
    Supervisor,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorRef {
    pub kind: ActorKind,
    pub id: Option<String>,
    pub display_name: Option<String>,
}

impl ActorRef {
    pub fn system() -> Self {
        Self {
            kind: ActorKind::System,
            id: None,
            display_name: Some("frankd".to_string()),
        }
    }

    pub fn supervisor() -> Self {
        Self {
            kind: ActorKind::Supervisor,
            id: None,
            display_name: Some("Frank supervisor".to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionRange {
    pub min: u16,
    pub max: u16,
}

impl VersionRange {
    pub const fn current() -> Self {
        Self {
            min: MIN_COMPATIBLE_CLIENT,
            max: PROTOCOL_VERSION,
        }
    }

    pub const fn accepts(&self, version: u16) -> bool {
        version >= self.min && version <= self.max
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub protocol_version: u16,
    pub supported_versions: VersionRange,
    pub minimum_compatible_client: u16,
    pub server_id: ServerId,
    #[serde(default)]
    pub certificate_fingerprint: String,
    pub server_version: String,
    pub features: Vec<String>,
    pub providers: Vec<ProviderCapability>,
    pub limits: CapabilityLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityLimits {
    pub max_message_bytes: usize,
    pub max_command_bytes: usize,
    pub max_terminal_frame_bytes: usize,
    pub max_artifact_bytes: u64,
    pub max_concurrency: u16,
    pub max_provider_concurrency: u16,
}

impl Default for CapabilityLimits {
    fn default() -> Self {
        Self {
            max_message_bytes: MAX_MESSAGE_BODY_BYTES,
            max_command_bytes: MAX_COMMAND_BODY_BYTES,
            max_terminal_frame_bytes: MAX_TERMINAL_FRAME_BYTES,
            max_artifact_bytes: MAX_ARTIFACT_BYTES,
            max_concurrency: 4,
            max_provider_concurrency: 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCapability {
    pub provider: Provider,
    pub executable: Option<String>,
    pub version: Option<String>,
    pub logged_in: bool,
    pub available: bool,
    pub capabilities: Vec<String>,
    pub diagnostic: Option<String>,
}

/// Normalized status values used by the owner-facing health/doctor screens.
/// They deliberately avoid carrying raw command output or filesystem paths;
/// diagnostics returned over the wire are sanitized by the daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceStatusView {
    pub service_name: String,
    pub installed: bool,
    pub running: bool,
    pub pid: Option<u32>,
    pub descriptor_path: Option<String>,
    pub health: HealthStatus,
    pub detail: Option<String>,
    pub checked_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeDoctorCheck {
    pub component: String,
    pub status: HealthStatus,
    pub version: Option<String>,
    pub detail: Option<String>,
    pub remediation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeDoctorView {
    pub healthy: bool,
    pub checks: Vec<RuntimeDoctorCheck>,
    pub checked_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkPreview {
    pub current_bind: String,
    pub proposed_bind: String,
    pub tls_required: bool,
    pub tls_fingerprint: Option<String>,
    pub restart_required: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionView {
    pub event_days: u16,
    pub terminal_days: u16,
    pub artifact_days: u16,
    pub terminal_max_bytes: u64,
    pub pending_uploads: u64,
    pub pending_operations: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticSnapshot {
    pub generated_at: Timestamp,
    pub database: HealthStatus,
    pub audit_exporter: HealthStatus,
    pub git: HealthStatus,
    pub providers: Vec<RuntimeDoctorCheck>,
    pub service: Option<ServiceStatusView>,
    pub retention: RetentionView,
    pub operation_backlog: u64,
    pub disk_free_bytes: Option<u64>,
    pub redactions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Codex,
    Claude,
}

impl fmt::Display for Provider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Codex => f.write_str("codex"),
            Self::Claude => f.write_str("claude"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandshakeRequest {
    pub protocol_version: u16,
    pub client_version: String,
    pub client_kind: String,
    pub supported_versions: VersionRange,
}

/// The first request a remote GUI/CLI makes after opening HTTPS. Keeping a
/// request/response handshake separate from the capability document lets the
/// daemon reject an incompatible client before it accepts mutations, while
/// newer additive capabilities remain safe to ignore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandshakeResponse {
    pub negotiated_version: u16,
    pub server_id: ServerId,
    pub server_version: String,
    pub capabilities: Capabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingRequest {
    pub protocol_version: u16,
    pub secret: String,
    pub certificate_fingerprint: String,
    pub requested_role: DeviceRole,
    pub device_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingResponse {
    pub server_id: ServerId,
    pub device_id: DeviceId,
    pub role: DeviceRole,
    pub device_token: String,
    pub certificate_fingerprint: String,
    /// PEM is returned only during pairing so a client can pin the same
    /// self-signed identity for subsequent HTTPS and WebSocket reconnects.
    /// It is optional for servers using a public/OS-trusted certificate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate_pem: Option<String>,
    pub expires_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub server_id: ServerId,
    pub revision: u64,
    pub event_seq: u64,
    pub server: ServerSettings,
    pub projects: Vec<ProjectView>,
    pub agents: Vec<AgentView>,
    pub missions: Vec<MissionView>,
    pub tasks: Vec<TaskView>,
    pub messages: Vec<MessageView>,
    pub approvals: Vec<ApprovalView>,
    pub artifacts: Vec<ArtifactView>,
    #[serde(default)]
    pub operations: Vec<OperationView>,
    #[serde(default)]
    pub uploads: Vec<ArtifactUploadView>,
    #[serde(default)]
    pub update: Option<UpdateView>,
    pub usage: Vec<UsageView>,
    pub terminals: Vec<TerminalSessionView>,
}

impl Snapshot {
    pub fn empty(server_id: ServerId) -> Self {
        Self {
            server_id,
            revision: 0,
            event_seq: 0,
            server: ServerSettings::default(),
            projects: Vec::new(),
            agents: Vec::new(),
            missions: Vec::new(),
            tasks: Vec::new(),
            messages: Vec::new(),
            approvals: Vec::new(),
            artifacts: Vec::new(),
            operations: Vec::new(),
            uploads: Vec::new(),
            update: None,
            usage: Vec::new(),
            terminals: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerSettings {
    pub name: String,
    pub bind_address: String,
    pub port: u16,
    pub allowed_project_roots: Vec<String>,
    pub worktree_root: String,
    pub max_concurrency: u16,
    pub max_provider_concurrency: u16,
    pub default_budget: Budget,
    pub approval_ttl_seconds: u64,
    pub event_retention_days: u16,
    pub terminal_retention_days: u16,
    pub artifact_retention_days: u16,
    pub supervisor_provider: Option<Provider>,
    pub tls_fingerprint: String,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            name: "Frank server".to_string(),
            bind_address: "127.0.0.1".to_string(),
            port: 37_465,
            allowed_project_roots: Vec::new(),
            worktree_root: String::new(),
            max_concurrency: 4,
            max_provider_concurrency: 2,
            default_budget: Budget::unlimited(),
            approval_ttl_seconds: 600,
            event_retention_days: 30,
            terminal_retention_days: 7,
            artifact_retention_days: 30,
            supervisor_provider: None,
            tls_fingerprint: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SettingsPatch {
    pub name: Option<String>,
    pub bind_address: Option<String>,
    pub port: Option<u16>,
    pub allowed_project_roots: Option<Vec<String>>,
    pub worktree_root: Option<String>,
    pub max_concurrency: Option<u16>,
    pub max_provider_concurrency: Option<u16>,
    pub default_budget: Option<Budget>,
    pub approval_ttl_seconds: Option<u64>,
    #[serde(default)]
    pub event_retention_days: Option<u16>,
    #[serde(default)]
    pub terminal_retention_days: Option<u16>,
    #[serde(default)]
    pub artifact_retention_days: Option<u16>,
    pub supervisor_provider: Option<Option<Provider>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSpec {
    pub name: String,
    pub path: Option<String>,
    pub clone_url: Option<String>,
    pub base_branch: String,
    pub remote: Option<String>,
    pub check_commands: Vec<String>,
    pub worktree_root: Option<String>,
    pub push_policy: PushPolicy,
    pub pr_policy: PrPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectView {
    pub id: ProjectId,
    pub name: String,
    pub path: String,
    pub base_branch: String,
    pub remote: Option<String>,
    pub check_commands: Vec<String>,
    pub worktree_root: String,
    pub push_policy: PushPolicy,
    pub pr_policy: PrPolicy,
    pub archived: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PushPolicy {
    Disabled,
    MissionBranch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrPolicy {
    Disabled,
    Draft,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSpec {
    pub display_name: String,
    pub template: AgentTemplate,
    pub provider: Provider,
    pub model: Option<String>,
    pub pack_id: Option<String>,
    pub pack_level: Option<String>,
    pub instructions: String,
    pub policy: AgentPolicy,
    pub budget: Budget,
    pub avatar: AvatarSpec,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPatch {
    pub display_name: Option<String>,
    pub model: Option<Option<String>>,
    pub pack_id: Option<Option<String>>,
    pub pack_level: Option<Option<String>>,
    pub instructions: Option<String>,
    pub policy: Option<AgentPolicy>,
    pub budget: Option<Budget>,
    pub avatar: Option<AvatarSpec>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentTemplate {
    Generalist,
    Researcher,
    Builder,
    Reviewer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AvatarSpec {
    pub palette: String,
    pub seed: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPolicy {
    pub filesystem: FilesystemPolicy,
    pub shell: ShellPolicy,
    pub network: NetworkPolicy,
    pub approval: ApprovalPolicy,
}

impl Default for AgentPolicy {
    fn default() -> Self {
        Self {
            filesystem: FilesystemPolicy::WorkspaceWrite,
            shell: ShellPolicy::Ask,
            network: NetworkPolicy::Ask,
            approval: ApprovalPolicy::Ask,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FilesystemPolicy {
    ReadOnly,
    WorkspaceWrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShellPolicy {
    Deny,
    Ask,
    Allow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NetworkPolicy {
    Deny,
    Ask,
    Allow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApprovalPolicy {
    Never,
    Ask,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentView {
    pub id: AgentId,
    pub display_name: String,
    pub template: AgentTemplate,
    pub provider: Provider,
    pub model: Option<String>,
    pub pack_id: Option<String>,
    pub pack_level: Option<String>,
    pub instructions: String,
    pub policy: AgentPolicy,
    pub budget: Budget,
    pub avatar: AvatarSpec,
    pub status: AgentStatus,
    pub provider_session_id: Option<String>,
    pub archived: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentStatus {
    Offline,
    Idle,
    Starting,
    Thinking,
    Working,
    Waiting,
    NeedsApproval,
    Paused,
    Failed,
    Stopping,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissionView {
    pub id: MissionId,
    pub project_id: ProjectId,
    pub objective: String,
    pub status: MissionStatus,
    pub supervisor_provider: Provider,
    pub supervisor_session_id: Option<String>,
    pub branch: String,
    pub budget: Budget,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MissionStatus {
    Draft,
    Active,
    Paused,
    Blocked,
    Completed,
    Failed,
    Cancelled,
}

impl MissionStatus {
    pub fn can_transition_to(self, next: Self) -> bool {
        use MissionStatus::*;
        matches!(
            (self, next),
            (Draft, Active | Cancelled)
                | (Active, Paused | Blocked | Completed | Failed | Cancelled)
                | (Paused, Active | Cancelled)
                | (Blocked, Active | Cancelled)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskSpec {
    pub mission_id: MissionId,
    pub title: String,
    pub objective: String,
    pub dependencies: Vec<TaskId>,
    pub priority: i32,
    pub assigned_agent: Option<AgentId>,
    pub budget: Budget,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub objective: Option<String>,
    pub dependencies: Option<Vec<TaskId>>,
    pub priority: Option<i32>,
    pub assigned_agent: Option<Option<AgentId>>,
    pub budget: Option<Budget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskView {
    pub id: TaskId,
    pub mission_id: MissionId,
    pub title: String,
    pub objective: String,
    pub dependencies: Vec<TaskId>,
    pub priority: i32,
    pub budget: Budget,
    pub status: TaskStatus,
    pub assigned_agent: Option<AgentId>,
    pub attempt: u8,
    pub max_attempts: u8,
    pub worktree: Option<String>,
    pub branch: Option<String>,
    pub result_artifact: Option<ArtifactId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Backlog,
    Ready,
    Running,
    Review,
    Done,
    Blocked,
    Cancelled,
}

impl TaskStatus {
    pub fn can_transition_to(self, next: Self) -> bool {
        use TaskStatus::*;
        matches!(
            (self, next),
            (Backlog, Ready | Cancelled)
                | (Ready, Running | Blocked | Cancelled)
                | (Running, Review | Blocked | Cancelled)
                | (Review, Done | Running | Blocked | Cancelled)
                | (Blocked, Ready | Cancelled)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageAct {
    Request,
    Inform,
    Propose,
    Query,
    Agree,
    Refuse,
    Done,
    Handoff,
}

impl MessageAct {
    /// Only conversational acts that ask for work or information require a
    /// durable reply acknowledgement from the recipient. Informational and
    /// terminal acts remain fire-and-forget while still being auditable.
    pub const fn requires_reply(self) -> bool {
        matches!(self, Self::Request | Self::Query | Self::Propose)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeliveryStatus {
    Queued,
    Delivered,
    Acknowledged,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageSpec {
    /// Optional caller-supplied id used for durable idempotency.  Provider
    /// bridges use this when retrying a send after a disconnect; a missing id
    /// is filled by `frankd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<MessageId>,
    pub mission_id: MissionId,
    pub task_id: Option<TaskId>,
    pub recipient: ActorRef,
    pub act: MessageAct,
    pub body: String,
    pub artifact_ids: Vec<ArtifactId>,
    pub reply_to: Option<MessageId>,
    pub hop: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageView {
    pub id: MessageId,
    pub mission_id: MissionId,
    pub task_id: Option<TaskId>,
    pub sender: ActorRef,
    pub recipient: ActorRef,
    pub act: MessageAct,
    pub body: String,
    pub artifact_ids: Vec<ArtifactId>,
    pub reply_to: Option<MessageId>,
    pub hop: u8,
    pub delivery: DeliveryStatus,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Denied,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalDecision {
    AllowOnce,
    DenyOnce,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalView {
    pub id: ApprovalId,
    pub agent_id: AgentId,
    pub task_id: TaskId,
    pub operation: String,
    pub cwd: String,
    pub project: String,
    pub reason: String,
    pub status: ApprovalStatus,
    pub expires_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalSpec {
    pub agent_id: AgentId,
    pub task_id: TaskId,
    pub operation: String,
    pub cwd: String,
    pub project: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BudgetScope {
    Mission,
    Agent,
    Task,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budget {
    pub time_seconds: Option<u64>,
    pub turns: Option<u32>,
    pub measured_tokens: Option<u64>,
    pub cost_micros: Option<u64>,
}

impl Budget {
    pub const fn unlimited() -> Self {
        Self {
            time_seconds: None,
            turns: None,
            measured_tokens: None,
            cost_micros: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageView {
    pub id: AttemptId,
    pub scope: BudgetScope,
    pub scope_id: String,
    pub provider: Provider,
    pub measured_input_tokens: Option<u64>,
    pub measured_output_tokens: Option<u64>,
    pub estimated_input_tokens: Option<u64>,
    pub estimated_output_tokens: Option<u64>,
    pub cost_micros: Option<u64>,
    pub recorded_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactSpec {
    pub mission_id: MissionId,
    pub task_id: Option<TaskId>,
    pub name: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactView {
    pub id: ArtifactId,
    pub mission_id: MissionId,
    pub task_id: Option<TaskId>,
    /// Present when the artifact was finalized from a streamed upload.  The
    /// stable link lets the store mark the upload complete in the same
    /// projection transaction as `ArtifactPublished`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upload_id: Option<UploadId>,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
    pub sha256: String,
    pub pinned: bool,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalSessionView {
    pub id: TerminalSessionId,
    pub task_id: TaskId,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
    pub active: bool,
    pub lease: Option<ControlLeaseView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlLeaseView {
    pub lease_id: String,
    pub session_id: TerminalSessionId,
    pub actor: ActorRef,
    pub expires_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum TerminalFrame {
    Hello {
        session_id: TerminalSessionId,
        next_sequence: TerminalSequence,
        replay_from: Option<TerminalSequence>,
    },
    Replay {
        sequence: TerminalSequence,
        bytes: Vec<u8>,
    },
    ResyncRequired {
        oldest_sequence: TerminalSequence,
    },
    Lease {
        lease: Option<ControlLeaseView>,
    },
    SequencedOutput {
        sequence: TerminalSequence,
        bytes: Vec<u8>,
    },
    Output {
        bytes: Vec<u8>,
    },
    Input {
        bytes: Vec<u8>,
    },
    Resize {
        cols: u16,
        rows: u16,
    },
    Exit {
        code: i32,
    },
    Error {
        message: String,
    },
    Heartbeat,
}

pub fn negotiate_versions(client: &VersionRange, server: &VersionRange) -> Result<u16, ApiError> {
    let min = client.min.max(server.min);
    let max = client.max.min(server.max);
    if min > max {
        return Err(ApiError::new(
            ErrorCode::VersionMismatch,
            "client and server protocol versions do not overlap",
        ));
    }
    Ok(max)
}

/// Negotiate feature capabilities without making unknown server extensions a
/// compatibility failure. A caller lists only the features it actually
/// needs; unsupported required features receive a stable validation error,
/// while additional server features are ignored safely.
pub fn require_capabilities(
    server: &Capabilities,
    required: &[&str],
) -> Result<Vec<String>, ApiError> {
    let mut accepted = Vec::with_capacity(required.len());
    for feature in required {
        if !server.features.iter().any(|candidate| candidate == feature) {
            return Err(ApiError::new(
                ErrorCode::Validation,
                format!("server does not support required capability: {feature}"),
            ));
        }
        accepted.push((*feature).to_string());
    }
    Ok(accepted)
}

pub fn validate_command_size(command: &CommandEnvelope) -> Result<(), ApiError> {
    let encoded = serde_json::to_vec(command).map_err(|_| {
        ApiError::new(
            ErrorCode::Validation,
            "command could not be serialized for validation",
        )
    })?;
    if encoded.len() > MAX_COMMAND_BODY_BYTES {
        return Err(ApiError::new(
            ErrorCode::PayloadTooLarge,
            "command payload exceeds the server limit",
        ));
    }
    Ok(())
}

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
            r#"{"protocol_version":1,"command_id":"00000000-0000-0000-0000-000000000000","expected_revision":4,"command":{"type":"set_task_status","data":{"task_id":"00000000-0000-0000-0000-000000000000","status":"running"}}}"#
        );
    }

    #[test]
    fn version_negotiation_rejects_a_gap() {
        let result = negotiate_versions(&VersionRange { min: 2, max: 3 }, &VersionRange::current());
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
            protocol_version: 1,
            supported_versions: VersionRange::current(),
            minimum_compatible_client: 1,
            server_id: ServerId::nil(),
            certificate_fingerprint: String::new(),
            server_version: "1.0.0".into(),
            features: vec!["terminal-replay".into(), "future-extension".into()],
            providers: Vec::new(),
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
                providers: Vec::new(),
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
}
