use serde::{Deserialize, Serialize};

use crate::*;

pub const MAX_COMMAND_BODY_BYTES: usize = 256 * 1024;
pub const MAX_MESSAGE_BODY_BYTES: usize = 64 * 1024;
pub const MAX_TERMINAL_FRAME_BYTES: usize = 256 * 1024;
pub const MAX_ARTIFACT_BYTES: u64 = 256 * 1024 * 1024;
pub const LOCAL_AUTH_FEATURE: &str = "local-password-auth";

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
