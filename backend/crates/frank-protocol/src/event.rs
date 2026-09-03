//! Server-to-client events. Ordered by the global event sequence.

use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub seq: u64,
    pub occurred_at: Timestamp,
    pub actor: ActorRef,
    pub correlation_id: Option<CorrelationId>,
    pub event: Event,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Event {
    SnapshotReplaced {
        snapshot: Snapshot,
    },
    SettingsChanged {
        settings: ServerSettings,
    },
    ProjectUpserted {
        project: ProjectView,
    },
    ProjectArchived {
        project_id: ProjectId,
    },
    AgentUpserted {
        agent: AgentView,
    },
    AgentArchived {
        agent_id: AgentId,
    },
    AgentStatusChanged {
        agent_id: AgentId,
        status: AgentStatus,
    },
    MissionCreated {
        mission: MissionView,
    },
    MissionStatusChanged {
        mission_id: MissionId,
        status: MissionStatus,
    },
    MissionSupervisorSessionChanged {
        mission_id: MissionId,
        provider_session_id: Option<String>,
    },
    MissionBlocked {
        mission_id: MissionId,
        reason: String,
    },
    TaskCreated {
        task: TaskView,
    },
    TaskUpdated {
        task: TaskView,
    },
    TaskStatusChanged {
        task_id: TaskId,
        status: TaskStatus,
    },
    /// A task can become runnable only after its daemon-owned worktree
    /// operation has been durably recorded. Keeping the card and operation
    /// in one event means a crash cannot leave a `running` task with no
    /// recovery intent.
    TaskWorktreeProvisioning {
        task: TaskView,
        operation: OperationView,
    },
    TaskAssigned {
        task_id: TaskId,
        agent_id: AgentId,
    },
    MessageQueued {
        message: MessageView,
    },
    MessageDelivered {
        message_id: MessageId,
    },
    MessageAcknowledged {
        message_id: MessageId,
    },
    MessageCompleted {
        message_id: MessageId,
    },
    MessageFailed {
        message_id: MessageId,
    },
    ApprovalRequested {
        approval: ApprovalView,
    },
    ApprovalDecided {
        approval_id: ApprovalId,
        decision: ApprovalDecision,
    },
    ApprovalExpired {
        approval_id: ApprovalId,
    },
    BudgetPaused {
        scope: BudgetScope,
        reason: String,
    },
    MemoryProposed {
        agent_id: AgentId,
        path: String,
    },
    MemoryRead {
        agent_id: AgentId,
        path: String,
    },
    ArtifactPublished {
        artifact: ArtifactView,
    },
    TerminalOpened {
        session: TerminalSessionView,
    },
    TerminalLeaseChanged {
        lease: ControlLeaseView,
    },
    TerminalClosed {
        session_id: TerminalSessionId,
    },
    UsageRecorded {
        usage: UsageView,
    },
    DeliveryStarted {
        mission_id: MissionId,
    },
    DeliveryCompleted {
        mission_id: MissionId,
        draft_pr_url: Option<String>,
    },
    DeliveryBlocked {
        mission_id: MissionId,
        reason: String,
    },
    OperationChanged {
        operation: OperationView,
    },
    SupervisorPlanProposed {
        mission_id: MissionId,
        proposal: SupervisorPlanProposal,
    },
    ArtifactUploadStarted {
        upload: ArtifactUploadView,
    },
    UpdateStateChanged {
        update: UpdateView,
    },
}
