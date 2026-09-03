//! Client-to-server commands and their results.

use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandEnvelope {
    pub protocol_version: u16,
    pub command_id: CommandId,
    pub expected_revision: Option<u64>,
    pub command: Command,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Command {
    Pair(PairingRequest),
    UpdateSettings {
        patch: SettingsPatch,
    },
    CreateProject(ProjectSpec),
    CloneProject {
        url: String,
        destination: String,
    },
    ArchiveProject {
        project_id: ProjectId,
    },
    CreateAgent(AgentSpec),
    UpdateAgent {
        agent_id: AgentId,
        patch: AgentPatch,
    },
    ArchiveAgent {
        agent_id: AgentId,
    },
    CreateMission {
        project_id: ProjectId,
        objective: String,
    },
    SetMissionStatus {
        mission_id: MissionId,
        status: MissionStatus,
    },
    CreateTask(TaskSpec),
    UpdateTask {
        task_id: TaskId,
        patch: TaskPatch,
    },
    SetTaskStatus {
        task_id: TaskId,
        status: TaskStatus,
    },
    AssignTask {
        task_id: TaskId,
        agent_id: AgentId,
    },
    SendMessage(MessageSpec),
    AckMessage {
        message_id: MessageId,
    },
    CompleteMessage {
        message_id: MessageId,
        success: bool,
    },
    RequestApproval(ApprovalSpec),
    DecideApproval {
        approval_id: ApprovalId,
        decision: ApprovalDecision,
    },
    AdjustBudget {
        scope: BudgetScope,
        scope_id: String,
        budget: Budget,
    },
    ProposeMemory {
        agent_id: AgentId,
        path: String,
        content: String,
    },
    ReadMemory {
        agent_id: AgentId,
        path: String,
    },
    PublishArtifact(ArtifactSpec),
    BeginArtifactUpload(ArtifactUploadSpec),
    FinalizeArtifactUpload {
        upload_id: UploadId,
        sha256: String,
        size: u64,
    },
    TaskAccept {
        task_id: TaskId,
    },
    SubmitSupervisorPlan {
        mission_id: MissionId,
        proposal: SupervisorPlanProposal,
    },
    CancelOperation {
        operation_id: OperationId,
    },
    RetryOperation {
        operation_id: OperationId,
    },
    CheckForUpdate,
    PrepareUpdate {
        version: String,
    },
    ApplyUpdate {
        update_id: UpdateId,
    },
    RollbackUpdate,
    DeliverMission {
        mission_id: MissionId,
    },
    OpenTerminal {
        task_id: TaskId,
        cols: u16,
        rows: u16,
    },
    TakeControl {
        session_id: TerminalSessionId,
    },
    RenewControl {
        session_id: TerminalSessionId,
        lease_id: String,
    },
    ReleaseControl {
        session_id: TerminalSessionId,
        lease_id: String,
    },
    CloseTerminal {
        session_id: TerminalSessionId,
    },
    PauseMission {
        mission_id: MissionId,
    },
    ResumeMission {
        mission_id: MissionId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandResponse {
    pub command_id: CommandId,
    pub revision: u64,
    pub result: Option<CommandResult>,
    pub error: Option<ApiError>,
}

impl CommandResponse {
    pub fn ok(command_id: CommandId, revision: u64, result: CommandResult) -> Self {
        Self {
            command_id,
            revision,
            result: Some(result),
            error: None,
        }
    }

    pub fn failed(command_id: CommandId, revision: u64, error: ApiError) -> Self {
        Self {
            command_id,
            revision,
            result: None,
            error: Some(error),
        }
    }
}

// Keep common command results allocation-free; the protocol is versioned and
// this layout is part of the stable DTO contract.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum CommandResult {
    Accepted,
    Snapshot(Snapshot),
    Pairing(PairingResponse),
    Created { id: String },
    Memory { path: String, content: String },
    Terminal(TerminalSessionView),
    Operation(OperationView),
    Upload(ArtifactUploadView),
    PlanAccepted { task_ids: Vec<TaskId> },
    Update(UpdateView),
}
