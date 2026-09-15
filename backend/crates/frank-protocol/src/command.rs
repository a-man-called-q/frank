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
    CreateRole(RoleSpec),
    UpdateRole {
        role_id: RoleId,
        patch: RolePatch,
    },
    ArchiveRole {
        role_id: RoleId,
    },
    SetAgentRole {
        agent_id: AgentId,
        role_id: RoleId,
    },
    /// Create a shared, fixed-lane taskboard. Boards may be global or scoped
    /// to a project/workflow and remain visible in the All Work view.
    CreateTaskboard(TaskboardSpec),
    UpdateTaskboard {
        taskboard_id: TaskboardId,
        patch: TaskboardPatch,
    },
    ArchiveTaskboard {
        taskboard_id: TaskboardId,
    },
    /// Persist a server-wide Organization draft without changing runtime
    /// enforcement.  The graph carries its own draft revision so unrelated
    /// task events do not cause autosave conflicts.
    SaveOrganizationDraft {
        graph: OrganizationGraph,
        expected_draft_revision: u64,
    },
    /// Atomically activate a previously saved graph.  The expected published
    /// revision prevents an owner from silently overwriting another owner's
    /// published configuration.
    PublishOrganization {
        expected_published_revision: u64,
    },
    CreateConnectorProfile(ConnectorProfileSpec),
    UpdateConnectorProfile {
        profile_id: ConnectorProfileId,
        patch: ConnectorProfilePatch,
    },
    ArchiveConnectorProfile {
        profile_id: ConnectorProfileId,
    },
    CreateMission {
        project_id: ProjectId,
        objective: String,
    },
    SetMissionStatus {
        mission_id: MissionId,
        status: MissionStatus,
    },
    /// Retry supervisor planning for a draft or blocked mission.  The
    /// command is idempotent: an existing task plan is never duplicated.
    RetryMissionPlan {
        mission_id: MissionId,
    },
    CreateTask(TaskSpec),
    /// Create a raw card directly on a board. This is the v2 entry point for
    /// an AE/operator/agent and deliberately does not require a supervisor
    /// generated DAG.
    CreateWorkItem(WorkItemSpec),
    /// Move a stable card between boards. Ownership changes are represented by
    /// this durable drop, never by a direct agent-to-agent handoff.
    DropWorkItem {
        task_id: TaskId,
        taskboard_id: TaskboardId,
        #[serde(default)]
        role_id: Option<RoleId>,
    },
    /// Fan a card out into one level of child cards. The parent remains
    /// blocked until every child reaches Done.
    SpawnChildWorkItems {
        parent_task_id: TaskId,
        children: Vec<WorkItemSpec>,
    },
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
    /// Claim is a separate verb from assignment so automatic worker pickup
    /// and operator force-assignment remain distinguishable in the feed.
    ClaimTask {
        task_id: TaskId,
        agent_id: AgentId,
        source: TaskClaimSource,
    },
    ReleaseTask {
        task_id: TaskId,
    },
    /// Create a pull-mode offer without starting a provider session.
    CreateWorkOffer {
        task_id: TaskId,
        agent_id: AgentId,
    },
    /// Respond to an outstanding pull-mode offer. Accepting is the provider
    /// start boundary; declining leaves the card available for another offer.
    RespondWorkOffer {
        offer_id: WorkOfferId,
        accept: bool,
    },
    AddTaskComment {
        task_id: TaskId,
        body: String,
        #[serde(default)]
        artifact_ids: Vec<ArtifactId>,
    },
    RequestHumanInput {
        task_id: TaskId,
        kind: HumanInputKind,
        prompt: String,
    },
    ResolveHumanInput {
        human_input_id: HumanInputId,
        answer: String,
    },
    RequestTaskRework {
        task_id: TaskId,
        reason: String,
    },
    RequestOrganizationDrain {
        target_revision: u64,
    },
    /// Internal daemon transition emitted after the last old-revision worker
    /// has finished. Clients request a drain; only the reconciler completes it.
    CompleteOrganizationDrain {
        revision: u64,
    },
    /// Explicitly resume a drained Organization after relocation checks.
    ResumeOrganization {
        revision: u64,
    },
    RelocateWorkItems {
        from_board_id: TaskboardId,
        to_board_id: TaskboardId,
        task_ids: Vec<TaskId>,
        #[serde(default)]
        reason: Option<String>,
    },
    /// Fire/archive a member while preserving active work as blocked cards
    /// for handoff assessment.
    FireAgent {
        agent_id: AgentId,
        #[serde(default)]
        reason: Option<String>,
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
    GrantTaskAccess {
        task_id: TaskId,
        agent_id: AgentId,
        worktree: String,
        effect: TaskGrantEffect,
        expires_at: Timestamp,
    },
    RevokeTaskGrant {
        grant_id: String,
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
    /// Approve or reject a durable review work item.  Approval completes the
    /// source task; rejection returns it to Running.
    DecideReview {
        review_item_id: ReviewWorkItemId,
        decision: ReviewDecision,
        #[serde(default)]
        reason: Option<String>,
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
    WorkItems { task_ids: Vec<TaskId> },
    WorkOffer(WorkOfferView),
    HumanInput(HumanInputView),
    Update(UpdateView),
}
