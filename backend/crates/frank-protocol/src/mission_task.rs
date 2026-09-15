use serde::{Deserialize, Serialize};

use crate::*;

fn is_default_work_item_kind(value: &WorkItemKind) -> bool {
    *value == WorkItemKind::Task
}

fn is_default_rework_limit(value: &u8) -> bool {
    *value == DEFAULT_REWORK_LIMIT
}

fn default_rework_limit() -> u8 {
    DEFAULT_REWORK_LIMIT
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissionView {
    pub id: MissionId,
    pub project_id: ProjectId,
    pub objective: String,
    pub status: MissionStatus,
    pub supervisor_session_id: Option<String>,
    pub branch: String,
    pub budget: Budget,
    /// A sanitized, user-facing diagnostic from the last failed planning
    /// attempt.  Older snapshots do not contain this field and deserialize
    /// with no error.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
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
            (Draft, Active | Blocked | Cancelled)
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
    /// Required primary role.  Legacy callers may omit this during the
    /// migration window; role-aware clients always set it.
    #[serde(default)]
    pub required_role_id: Option<RoleId>,
    pub priority: i32,
    pub assigned_agent: Option<AgentId>,
    pub budget: Budget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taskboard_id: Option<TaskboardId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<WorkflowId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_task_id: Option<TaskId>,
    #[serde(default, skip_serializing_if = "is_default_work_item_kind")]
    pub kind: WorkItemKind,
    #[serde(
        default = "default_rework_limit",
        skip_serializing_if = "is_default_rework_limit"
    )]
    pub rework_limit: u8,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskPatch {
    pub title: Option<String>,
    pub objective: Option<String>,
    pub dependencies: Option<Vec<TaskId>>,
    #[serde(default)]
    pub required_role_id: Option<Option<RoleId>>,
    pub priority: Option<i32>,
    pub assigned_agent: Option<Option<AgentId>>,
    pub budget: Option<Budget>,
}

/// Typed completion intent accepted by worker-facing `task_update`. A worker
/// can submit completion, but the reducer opens Review; only the configured
/// reviewer can later promote it to Done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskCompletionIntent {
    Completed,
    Rework,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskUpdateInput {
    #[serde(default)]
    pub status: Option<TaskCompletionIntent>,
    #[serde(flatten)]
    pub patch: TaskPatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskView {
    pub id: TaskId,
    pub mission_id: MissionId,
    pub title: String,
    pub objective: String,
    pub dependencies: Vec<TaskId>,
    #[serde(default)]
    pub required_role_id: Option<RoleId>,
    pub priority: i32,
    pub budget: Budget,
    pub status: TaskStatus,
    pub assigned_agent: Option<AgentId>,
    /// The staff member that owns the current review work item. This is
    /// populated atomically when a running task enters `Review`; keeping the
    /// target on the source task prevents a reviewer from accepting an
    /// unrelated review card after a reconnect.
    #[serde(default)]
    pub reviewer_agent: Option<AgentId>,
    #[serde(default)]
    pub claimed_at: Option<Timestamp>,
    #[serde(default)]
    pub claim_source: Option<TaskClaimSource>,
    pub attempt: u8,
    pub max_attempts: u8,
    pub worktree: Option<String>,
    pub branch: Option<String>,
    pub result_artifact: Option<ArtifactId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taskboard_id: Option<TaskboardId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<WorkflowId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_task_id: Option<TaskId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub child_task_ids: Vec<TaskId>,
    #[serde(default, skip_serializing_if = "is_default_work_item_kind")]
    pub kind: WorkItemKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_role_node_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organization_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "is_default_rework_limit")]
    pub rework_limit: u8,
    #[serde(default)]
    pub rework_count: u8,
}
