use serde::{Deserialize, Serialize};

use crate::*;

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
    /// Preferred role for automatic pickup. An explicit agent remains a
    /// manual override, but must belong to this role when both are present.
    #[serde(default)]
    pub target_role_id: Option<RoleId>,
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
