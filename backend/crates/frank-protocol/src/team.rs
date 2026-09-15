use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub server_id: ServerId,
    pub revision: u64,
    pub event_seq: u64,
    pub server: ServerSettings,
    pub projects: Vec<ProjectView>,
    /// Role templates are the source of truth for worker configuration.  The
    /// field is additive so snapshots written before Team support remain
    /// readable by a newer daemon.
    #[serde(default)]
    pub roles: Vec<RoleView>,
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
    /// Flat, append-only task activity.  Comments and system transitions use
    /// the same feed so a worker handoff is one auditable timeline.
    #[serde(default)]
    pub task_feed: Vec<TaskFeedEntry>,
    /// The server-wide Team/Organization document.  This field is additive so
    /// snapshots written before Organization support remain readable.
    #[serde(default)]
    pub organization: OrganizationStateView,
    /// Durable review work items created when a published Organization
    /// routes a task through a review relation.  The field is additive so
    /// snapshots written before review items remain readable.
    #[serde(default)]
    pub review_items: Vec<ReviewWorkItemView>,
    /// Shared Taskboard records. Additive to keep pre-workflow snapshots
    /// readable by a newer daemon.
    #[serde(default)]
    pub taskboards: Vec<TaskboardView>,
    /// Pull-mode offers are durable records and are never inferred from an
    /// idle provider process.
    #[serde(default)]
    pub work_offers: Vec<WorkOfferView>,
    /// Human questions use the same card and feed; this list stores the
    /// pending/resolution metadata for reconnect and audit.
    #[serde(default)]
    pub human_inputs: Vec<HumanInputView>,
    #[serde(default)]
    pub organization_runtime: OrganizationRuntimeView,
    #[serde(default)]
    pub organization_relocations: Vec<OrganizationRelocationView>,
    /// Task-scoped host permissions. Grants are intentionally separate from
    /// one-shot approval rows so they can be revoked and audited directly.
    #[serde(default)]
    pub task_grants: Vec<TaskGrantView>,
}

impl Snapshot {
    pub fn empty(server_id: ServerId) -> Self {
        Self {
            server_id,
            revision: 0,
            event_seq: 0,
            server: ServerSettings::default(),
            projects: Vec::new(),
            roles: Vec::new(),
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
            task_feed: Vec::new(),
            organization: OrganizationStateView::default(),
            review_items: Vec::new(),
            taskboards: Vec::new(),
            work_offers: Vec::new(),
            human_inputs: Vec::new(),
            organization_runtime: OrganizationRuntimeView::default(),
            organization_relocations: Vec::new(),
            task_grants: Vec::new(),
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
    pub default_budget: Budget,
    pub approval_ttl_seconds: u64,
    pub event_retention_days: u16,
    pub terminal_retention_days: u16,
    pub artifact_retention_days: u16,
    #[serde(default)]
    pub supervisor_model: Option<String>,
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
            default_budget: Budget::unlimited(),
            approval_ttl_seconds: 600,
            event_retention_days: 30,
            terminal_retention_days: 7,
            artifact_retention_days: 30,
            supervisor_model: None,
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
    pub default_budget: Option<Budget>,
    pub approval_ttl_seconds: Option<u64>,
    #[serde(default)]
    pub event_retention_days: Option<u16>,
    #[serde(default)]
    pub terminal_retention_days: Option<u16>,
    #[serde(default)]
    pub artifact_retention_days: Option<u16>,
    #[serde(default)]
    pub supervisor_model: Option<Option<String>>,
    #[serde(default)]
    pub clear_supervisor_model: bool,
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

/// The persisted server-wide organization board.  Identifiers inside the
/// graph are deliberately strings: the desktop editor creates stable node,
/// relation, and group ids while AgentId/ConnectorProfileId remain the typed
/// references that give the graph runtime meaning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationGraph {
    pub id: OrganizationId,
    pub draft_revision: u64,
    pub published_revision: u64,
    #[serde(default)]
    pub nodes: Vec<OrganizationNode>,
    #[serde(default)]
    pub relations: Vec<OrganizationRelation>,
    #[serde(default)]
    pub groups: Vec<OrganizationGroup>,
    #[serde(default)]
    pub viewport: OrganizationViewport,
}

impl OrganizationGraph {
    pub fn empty() -> Self {
        Self {
            id: OrganizationId::new(),
            draft_revision: 0,
            published_revision: 0,
            nodes: Vec::new(),
            relations: Vec::new(),
            groups: Vec::new(),
            viewport: OrganizationViewport::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationStateView {
    pub draft: OrganizationGraph,
    #[serde(default)]
    pub published: Option<OrganizationGraph>,
    #[serde(default)]
    pub connector_profiles: Vec<ConnectorProfileView>,
}

impl Default for OrganizationStateView {
    fn default() -> Self {
        Self {
            draft: OrganizationGraph::empty(),
            published: None,
            connector_profiles: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationNodeKind {
    Staff,
    Capability,
    Approval,
    /// v2 executable worker node. It references a Team role rather than a
    /// concrete agent.
    Role,
    /// v2 durable handoff surface.
    Taskboard,
    /// v2 one-level child workflow composition node.
    ChildWorkflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationCapabilityKind {
    Email,
    Calendar,
    Drive,
    Browser,
    Terminal,
    Database,
}

impl OrganizationCapabilityKind {
    pub const fn permissions(self) -> &'static [&'static str] {
        match self {
            Self::Email => &["read", "send"],
            Self::Calendar => &["read", "create", "update"],
            Self::Drive => &["read", "write", "share"],
            Self::Browser => &["browse", "download"],
            Self::Terminal => &["execute"],
            Self::Database => &["inspect", "read", "write"],
        }
    }

    pub const fn is_sensitive(self) -> bool {
        matches!(self, Self::Terminal | Self::Database)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationRelationKind {
    Handoff,
    ToolAccess,
    Review,
    /// Taskboard -> Role pickup route.
    Pickup,
    /// Role/child workflow -> Taskboard drop route.
    Drop,
    /// Explicit bounded revision loop.
    Rework,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationContextPolicy {
    #[default]
    MinimumRequired,
    SummaryAndArtifacts,
    FullContext,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationPoint {
    pub x: f64,
    pub y: f64,
}

impl Default for OrganizationPoint {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0 }
    }
}

impl PartialEq for OrganizationPoint {
    fn eq(&self, other: &Self) -> bool {
        self.x.to_bits() == other.x.to_bits() && self.y.to_bits() == other.y.to_bits()
    }
}

impl Eq for OrganizationPoint {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationSize {
    pub width: f64,
    pub height: f64,
}

impl Default for OrganizationSize {
    fn default() -> Self {
        Self {
            width: 420.0,
            height: 300.0,
        }
    }
}

impl PartialEq for OrganizationSize {
    fn eq(&self, other: &Self) -> bool {
        self.width.to_bits() == other.width.to_bits()
            && self.height.to_bits() == other.height.to_bits()
    }
}

impl Eq for OrganizationSize {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizationViewport {
    pub x: f64,
    pub y: f64,
    pub zoom: f64,
}

impl Default for OrganizationViewport {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
        }
    }
}

impl PartialEq for OrganizationViewport {
    fn eq(&self, other: &Self) -> bool {
        self.x.to_bits() == other.x.to_bits()
            && self.y.to_bits() == other.y.to_bits()
            && self.zoom.to_bits() == other.zoom.to_bits()
    }
}

impl Eq for OrganizationViewport {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationGroup {
    pub id: String,
    pub label: String,
    pub position: OrganizationPoint,
    pub size: OrganizationSize,
    #[serde(default)]
    pub tone: String,
    #[serde(default)]
    pub locked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationNode {
    pub id: String,
    pub kind: OrganizationNodeKind,
    pub label: String,
    pub position: OrganizationPoint,
    #[serde(default)]
    pub group_id: Option<String>,
    #[serde(default)]
    pub agent_id: Option<AgentId>,
    #[serde(default)]
    pub capability: Option<OrganizationCapabilityKind>,
    #[serde(default)]
    pub connector_profile_id: Option<ConnectorProfileId>,
    #[serde(default)]
    pub profile_ref: Option<String>,
    #[serde(default)]
    pub configured: bool,
    #[serde(default)]
    pub approval_required: bool,
    /// v2 Role node reference. Legacy Staff nodes continue to use `agent_id`
    /// during migration, but new workflows target roles rather than people.
    #[serde(default)]
    pub role_id: Option<RoleId>,
    /// v2 Taskboard node reference.
    #[serde(default)]
    pub taskboard_id: Option<TaskboardId>,
    /// v2 one-level child workflow composition reference.
    #[serde(default)]
    pub child_workflow_id: Option<WorkflowId>,
    /// Optional stable input/output port names for child-workflow routing.
    #[serde(default)]
    pub input_port: Option<String>,
    #[serde(default)]
    pub output_port: Option<String>,
    /// Rework visit limit for explicit Rework edges.
    #[serde(default)]
    pub rework_limit: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationHandoffContract {
    #[serde(default)]
    pub input_summary: String,
    #[serde(default)]
    pub expected_output: String,
    #[serde(default)]
    pub context_policy: OrganizationContextPolicy,
}

impl Default for OrganizationHandoffContract {
    fn default() -> Self {
        Self {
            input_summary: "Task brief and relevant artifacts".into(),
            expected_output: "A concise, reviewable deliverable".into(),
            context_policy: OrganizationContextPolicy::MinimumRequired,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationRelation {
    pub id: String,
    pub kind: OrganizationRelationKind,
    pub source_node_id: String,
    pub target_node_id: String,
    #[serde(default)]
    pub contract: OrganizationHandoffContract,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorKind {
    GoogleWorkspace,
    Browser,
    Terminal,
    Postgres,
    Sqlite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorHealth {
    #[default]
    Unknown,
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorProfileSpec {
    pub name: String,
    pub kind: ConnectorKind,
    /// Non-secret configuration only.  Secrets are held by the daemon's
    /// credential store and referenced by this profile's id.
    #[serde(default)]
    pub config: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorProfilePatch {
    pub name: Option<String>,
    pub kind: Option<ConnectorKind>,
    #[serde(default)]
    pub config: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectorProfileView {
    pub id: ConnectorProfileId,
    pub name: String,
    pub kind: ConnectorKind,
    #[serde(default)]
    pub config: Value,
    #[serde(default)]
    pub health: ConnectorHealth,
    /// Whether the daemon has a credential/keychain entry for this profile.
    /// The credential value itself is never serialized.
    #[serde(default)]
    pub configured: bool,
    #[serde(default)]
    pub diagnostic: Option<String>,
    #[serde(default)]
    pub checked_at: Option<Timestamp>,
    pub archived: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSpec {
    /// The primary role whose full template is materialized into this agent.
    /// A member cannot be created without a role in the production contract.
    pub role_id: RoleId,
    pub display_name: String,
    #[serde(default)]
    pub model_override: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPatch {
    #[serde(default)]
    pub role_id: Option<Option<RoleId>>,
    pub display_name: Option<String>,
    #[serde(default)]
    pub model_override: Option<Option<String>>,
    /// JSON cannot distinguish an omitted `Option<Option<String>>` from a
    /// deliberate null. This explicit bit lets the UI clear an active
    /// override without making model selection stringly typed.
    #[serde(default)]
    pub clear_model_override: bool,
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
    #[serde(default)]
    pub role_id: Option<RoleId>,
    /// Revision of the role template currently materialized into the agent.
    /// While an agent is working this remains frozen; the next idle boundary
    /// applies the latest role revision.
    #[serde(default)]
    pub role_revision: u64,
    pub display_name: String,
    pub model: Option<String>,
    #[serde(default)]
    pub effective_model: Option<String>,
    #[serde(default)]
    pub model_source: ModelSource,
    #[serde(default)]
    pub model_override: Option<String>,
    #[serde(default)]
    pub pending_model_override: Option<Option<String>>,
    #[serde(default)]
    pub pending_model_change: bool,
    pub instructions: String,
    pub policy: AgentPolicy,
    pub budget: Budget,
    pub status: AgentStatus,
    pub provider_session_id: Option<String>,
    #[serde(default)]
    pub last_claimed_at: Option<Timestamp>,
    pub archived: bool,
}

/// A role is a complete worker template.  Runtime/provider settings live here
/// rather than being independently edited on every member.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleSpec {
    pub name: String,
    #[serde(rename = "default_model", alias = "model")]
    pub model: Option<String>,
    pub instructions: String,
    pub policy: AgentPolicy,
    pub budget: Budget,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RolePatch {
    pub name: Option<String>,
    #[serde(rename = "default_model", alias = "model")]
    pub model: Option<Option<String>>,
    #[serde(default)]
    pub clear_model: bool,
    pub instructions: Option<String>,
    pub policy: Option<AgentPolicy>,
    pub budget: Option<Budget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleView {
    pub id: RoleId,
    pub name: String,
    #[serde(rename = "default_model", alias = "model")]
    pub model: Option<String>,
    pub instructions: String,
    pub policy: AgentPolicy,
    pub budget: Budget,
    pub revision: u64,
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
