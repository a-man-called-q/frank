use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskClaimSource {
    Automatic,
    Manual,
    Reclaim,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskFeedKind {
    Comment,
    Created,
    StatusChanged,
    Assigned,
    Claimed,
    Released,
    AgentDeparted,
    DependencyLocked,
    DependencyUnlocked,
    AttachmentAdded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskFeedEntry {
    pub id: TaskFeedId,
    pub task_id: TaskId,
    pub actor: ActorRef,
    pub kind: TaskFeedKind,
    pub body: String,
    #[serde(default)]
    pub artifact_ids: Vec<ArtifactId>,
    pub created_at: Timestamp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewWorkItemStatus {
    Pending,
    Approved,
    Rejected,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDecision {
    Approve,
    Reject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewWorkItemView {
    pub id: ReviewWorkItemId,
    pub mission_id: MissionId,
    pub source_task_id: TaskId,
    pub source_agent: AgentId,
    pub reviewer_agent: AgentId,
    /// The exact Organization relation that created this review item.  It is
    /// retained so a later graph edit cannot silently change an in-flight
    /// review contract.
    pub relation_id: String,
    pub contract: OrganizationHandoffContract,
    pub status: ReviewWorkItemStatus,
    #[serde(default)]
    pub decision_reason: Option<String>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
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
    /// Persist a task-scoped write/check grant after the owner confirms the
    /// exact preview. Network, credentials, and privileged effects never use
    /// this decision.
    AllowForTask,
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
    pub provider: UsageProviderId,
    #[serde(default)]
    pub model: Option<String>,
    pub measured_input_tokens: Option<u64>,
    pub measured_output_tokens: Option<u64>,
    pub estimated_input_tokens: Option<u64>,
    pub estimated_output_tokens: Option<u64>,
    pub cost_micros: Option<u64>,
    #[serde(default)]
    pub cached_input_tokens: Option<u64>,
    #[serde(default)]
    pub reasoning_tokens: Option<u64>,
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
