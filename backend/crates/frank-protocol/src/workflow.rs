//! Additive protocol types for Frank's executable Organization workflow.
//!
//! The original v1 contract called the durable card a `TaskView`.  The
//! workflow model intentionally keeps that wire name and `TaskId` so existing
//! clients, event logs, and artifacts remain readable.  These types describe
//! the new board/offer/control-plane records that hang off the same stable
//! card id.

use serde::{Deserialize, Serialize};

use crate::{
    AgentId, Budget, HumanInputId, MissionId, OrganizationContextPolicy,
    OrganizationHandoffContract, ProjectId, RelocationId, RoleId, TaskId, TaskboardId, Timestamp,
    WorkOfferId, WorkflowId,
};

pub const ORGANIZATION_WORKFLOW_SCHEMA_VERSION: u16 = 2;
pub const DEFAULT_REWORK_LIMIT: u8 = 3;
pub const MAX_CHILD_WORK_ITEMS: usize = 64;

/// Fixed runtime lanes. UI may style these differently, but it cannot create
/// a second set of statuses that the scheduler does not understand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskboardLane {
    Backlog,
    Ready,
    Running,
    Blocked,
    Review,
    Done,
    Cancelled,
}

impl TaskboardLane {
    pub const ALL: [Self; 7] = [
        Self::Backlog,
        Self::Ready,
        Self::Running,
        Self::Blocked,
        Self::Review,
        Self::Done,
        Self::Cancelled,
    ];

    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Done | Self::Cancelled)
    }
}

impl From<crate::TaskStatus> for TaskboardLane {
    fn from(value: crate::TaskStatus) -> Self {
        match value {
            crate::TaskStatus::Backlog => Self::Backlog,
            crate::TaskStatus::Ready => Self::Ready,
            crate::TaskStatus::Running => Self::Running,
            crate::TaskStatus::Blocked => Self::Blocked,
            crate::TaskStatus::Review => Self::Review,
            crate::TaskStatus::Done => Self::Done,
            crate::TaskStatus::Cancelled => Self::Cancelled,
        }
    }
}

impl From<TaskboardLane> for crate::TaskStatus {
    fn from(value: TaskboardLane) -> Self {
        match value {
            TaskboardLane::Backlog => Self::Backlog,
            TaskboardLane::Ready => Self::Ready,
            TaskboardLane::Running => Self::Running,
            TaskboardLane::Blocked => Self::Blocked,
            TaskboardLane::Review => Self::Review,
            TaskboardLane::Done => Self::Done,
            TaskboardLane::Cancelled => Self::Cancelled,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TaskboardDispatchMode {
    /// The daemon assigns a ready card deterministically to an eligible role
    /// member and starts it after the claim commits.
    Auto,
    /// The daemon creates one short-lived offer for one eligible agent. The
    /// provider is not started until that agent accepts the offer.
    #[default]
    Pull,
    /// An operator/agent explicitly drops a card onto this board's role.
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemKind {
    #[default]
    Task,
    HumanQuestion,
    Idea,
    Note,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WorkOfferStatus {
    #[default]
    Pending,
    Accepted,
    Declined,
    Expired,
    Cancelled,
}

impl WorkOfferStatus {
    pub const fn is_open(self) -> bool {
        matches!(self, Self::Pending)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HumanInputStatus {
    #[default]
    Pending,
    Answered,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum HumanInputKind {
    #[default]
    Question,
    Approval,
    Clarification,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationDrainStatus {
    #[default]
    Running,
    Draining,
    Paused,
}

/// A shared board is a routeable surface. It can be global, or scoped to one
/// project/workflow while still appearing in the global All Work view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskboardSpec {
    pub name: String,
    #[serde(default)]
    pub project_id: Option<ProjectId>,
    #[serde(default)]
    pub workflow_id: Option<WorkflowId>,
    #[serde(default)]
    pub dispatch_mode: TaskboardDispatchMode,
    #[serde(default)]
    pub default_role_id: Option<RoleId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskboardView {
    pub id: TaskboardId,
    pub name: String,
    #[serde(default)]
    pub project_id: Option<ProjectId>,
    #[serde(default)]
    pub workflow_id: Option<WorkflowId>,
    #[serde(default)]
    pub dispatch_mode: TaskboardDispatchMode,
    #[serde(default)]
    pub default_role_id: Option<RoleId>,
    #[serde(default)]
    pub archived: bool,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskboardPatch {
    pub name: Option<String>,
    #[serde(default)]
    pub project_id: Option<Option<ProjectId>>,
    #[serde(default)]
    pub workflow_id: Option<Option<WorkflowId>>,
    pub dispatch_mode: Option<TaskboardDispatchMode>,
    #[serde(default)]
    pub default_role_id: Option<Option<RoleId>>,
}

/// Command input for creating a card without making the supervisor invent a
/// DAG up front. `parent_task_id` is optional; all children still retain one
/// stable `TaskId` and the parent waits on them explicitly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkItemSpec {
    #[serde(default)]
    pub mission_id: Option<MissionId>,
    pub title: String,
    pub objective: String,
    #[serde(default)]
    pub kind: WorkItemKind,
    #[serde(default = "TaskboardId::nil")]
    pub taskboard_id: TaskboardId,
    #[serde(default)]
    pub workflow_id: Option<WorkflowId>,
    #[serde(default)]
    pub parent_task_id: Option<TaskId>,
    #[serde(default)]
    pub dependencies: Vec<TaskId>,
    #[serde(default)]
    pub required_role_id: Option<RoleId>,
    #[serde(default)]
    pub priority: i32,
    #[serde(default = "Budget::unlimited")]
    pub budget: Budget,
    #[serde(default = "default_rework_limit")]
    pub rework_limit: u8,
}

fn default_rework_limit() -> u8 {
    DEFAULT_REWORK_LIMIT
}

/// The offer is a durable broker record. It is intentionally not tied to a
/// provider session; accepting it is the boundary at which a provider may be
/// started.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkOfferView {
    pub id: WorkOfferId,
    pub task_id: TaskId,
    pub taskboard_id: TaskboardId,
    pub agent_id: AgentId,
    pub role_id: Option<RoleId>,
    pub status: WorkOfferStatus,
    pub attempt: u8,
    pub created_at: Timestamp,
    pub expires_at: Timestamp,
    #[serde(default)]
    pub responded_at: Option<Timestamp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanInputView {
    pub id: HumanInputId,
    pub task_id: TaskId,
    pub mission_id: Option<MissionId>,
    pub requested_by: Option<AgentId>,
    pub kind: HumanInputKind,
    pub prompt: String,
    pub status: HumanInputStatus,
    #[serde(default)]
    pub answer: Option<String>,
    #[serde(default)]
    pub answered_by: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationRelocationView {
    pub id: RelocationId,
    pub from_revision: u64,
    pub to_revision: u64,
    pub from_board_id: TaskboardId,
    pub to_board_id: TaskboardId,
    pub task_ids: Vec<TaskId>,
    #[serde(default)]
    pub reason: Option<String>,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationRuntimeView {
    pub active_revision: u64,
    pub status: OrganizationDrainStatus,
    #[serde(default)]
    pub drain_requested_revision: Option<u64>,
    #[serde(default)]
    pub paused_revision: Option<u64>,
    #[serde(default)]
    pub pending_relocation_count: u32,
}

impl Default for OrganizationRuntimeView {
    fn default() -> Self {
        Self {
            active_revision: 0,
            status: OrganizationDrainStatus::Running,
            drain_requested_revision: None,
            paused_revision: None,
            pending_relocation_count: 0,
        }
    }
}

/// Review/handoff contracts remain useful on explicit board drops while the
/// old direct staff-to-staff graph is retired for v2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardRouteView {
    pub source_board_id: TaskboardId,
    pub target_board_id: TaskboardId,
    #[serde(default)]
    pub role_id: Option<RoleId>,
    #[serde(default)]
    pub contract: OrganizationHandoffContract,
    #[serde(default)]
    pub context_policy: OrganizationContextPolicy,
}
