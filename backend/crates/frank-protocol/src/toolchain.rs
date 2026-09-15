//! Versioned toolchain, runner, check, journal, and task-grant contracts.
//!
//! These types are deliberately transport-only.  A daemon can advertise a
//! manifest and an install preview without giving a provider session access
//! to the host filesystem or to an arbitrary shell.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolchainManifest {
    pub schema_version: u16,
    pub id: String,
    pub label: String,
    pub version: String,
    pub detect: ToolchainDetection,
    #[serde(default)]
    pub artifacts: Vec<ToolchainArtifact>,
    pub install: ToolchainInstallSpec,
    #[serde(default)]
    pub checks: Vec<ToolchainCheck>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ToolchainDetection {
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub probe: Option<ToolchainProbe>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolchainProbe {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default = "default_version_arg")]
    pub version_arg: String,
}

fn default_version_arg() -> String {
    "--version".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolchainArtifact {
    pub platform: String,
    pub source: ToolchainArtifactSource,
    pub sha256: String,
    pub size_bytes: u64,
    #[serde(default)]
    pub archive: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum ToolchainArtifactSource {
    Url { url: String },
    Host { executable: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolchainInstallSpec {
    pub directory: String,
    #[serde(default)]
    pub executable: Option<String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolchainCheck {
    pub id: String,
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
    #[serde(default = "default_check_timeout")]
    pub timeout_seconds: u64,
}

fn default_check_timeout() -> u64 {
    600
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolchainRequirementStatus {
    Missing,
    NeedsApproval,
    Installing,
    Ready,
    ManualRequirement,
    Failed,
    Incompatible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolchainRequirementView {
    pub manifest_id: String,
    pub label: String,
    pub required_version: String,
    pub status: ToolchainRequirementStatus,
    #[serde(default)]
    pub detected_version: Option<String>,
    #[serde(default)]
    pub diagnostic: Option<String>,
    #[serde(default)]
    pub install_plan: Option<ToolchainInstallPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolchainInstallPlan {
    pub manifest_id: String,
    pub version: String,
    pub source: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub install_path: String,
    pub checks: Vec<String>,
    pub approval_scope: String,
    /// Exact artifact selected for this platform. Older clients may omit it;
    /// new runners use it to verify that the preview and install request are
    /// the same immutable artifact.
    #[serde(default)]
    pub artifact: Option<ToolchainArtifact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerStatus {
    Pairing,
    Idle,
    Busy,
    Offline,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerView {
    pub id: RunnerId,
    pub name: String,
    pub host: String,
    pub status: RunnerStatus,
    pub last_seen_at: Option<Timestamp>,
    #[serde(default)]
    pub toolchains: Vec<String>,
    #[serde(default)]
    pub path_mappings: Vec<RunnerPathMapping>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerPathMapping {
    pub daemon_root: String,
    pub host_root: String,
    #[serde(default)]
    pub project_id: Option<ProjectId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckRunView {
    pub id: String,
    pub runner_id: RunnerId,
    pub project_id: ProjectId,
    #[serde(default)]
    pub task_id: Option<TaskId>,
    pub check_id: String,
    pub status: CheckRunStatus,
    #[serde(default)]
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub started_at: Timestamp,
    pub finished_at: Option<Timestamp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunnerJobStatus {
    Queued,
    Running,
    Passed,
    Failed,
    TimedOut,
    Cancelled,
}

/// The durable job table is shared by host checks and installations. Keeping
/// the kind explicit prevents an install result from being mistaken for a
/// check result when a daemon recovers a row after a restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RunnerJobKind {
    #[default]
    Check,
    ToolchainInstall,
}

/// Durable runner job projection. The live WebSocket is only a transport;
/// this row lets the daemon recover the outcome of a job after reconnects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerJobView {
    pub id: String,
    pub runner_id: RunnerId,
    pub project_id: ProjectId,
    #[serde(default)]
    pub task_id: Option<TaskId>,
    pub check_id: String,
    #[serde(default)]
    pub kind: RunnerJobKind,
    pub status: RunnerJobStatus,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    #[serde(default)]
    pub result: Option<CheckRunView>,
    #[serde(default)]
    pub install_result: Option<RunnerInstallResult>,
}

/// Exact host-side installation request. The daemon sends a relative path;
/// the runner resolves it below its configured user-local install root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerInstallJob {
    pub id: String,
    pub runner_id: RunnerId,
    pub project_id: ProjectId,
    #[serde(default)]
    pub task_id: Option<TaskId>,
    /// Daemon-side task/worktree path. The runner must map and canonicalize it
    /// before using it as the cwd for a host-version check.
    pub daemon_worktree: String,
    pub manifest_id: String,
    pub version: String,
    pub artifact: ToolchainArtifact,
    pub install_relative_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunnerInstallResult {
    pub id: String,
    pub runner_id: RunnerId,
    pub status: ToolchainRequirementStatus,
    #[serde(default)]
    pub installed_path: Option<String>,
    #[serde(default)]
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckRunStatus {
    Queued,
    Running,
    Passed,
    Failed,
    Cancelled,
    TimedOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalEntryKind {
    Task,
    Comment,
    Approval,
    AgentLifecycle,
    Usage,
    Toolchain,
    Check,
    Build,
    Event,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JournalOutcome {
    Info,
    Pending,
    Success,
    Failure,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalEntryView {
    pub sequence: u64,
    pub occurred_at: Timestamp,
    pub actor: ActorRef,
    pub kind: JournalEntryKind,
    pub outcome: JournalOutcome,
    pub summary: String,
    #[serde(default)]
    pub detail: Option<serde_json::Value>,
    #[serde(default)]
    pub project_id: Option<ProjectId>,
    #[serde(default)]
    pub mission_id: Option<MissionId>,
    #[serde(default)]
    pub task_id: Option<TaskId>,
    #[serde(default)]
    pub agent_id: Option<AgentId>,
    #[serde(default)]
    pub check_run_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct JournalFilter {
    #[serde(default)]
    pub before_sequence: Option<u64>,
    #[serde(default)]
    pub limit: Option<u32>,
    #[serde(default)]
    pub project_id: Option<ProjectId>,
    #[serde(default)]
    pub mission_id: Option<MissionId>,
    #[serde(default)]
    pub task_id: Option<TaskId>,
    #[serde(default)]
    pub agent_id: Option<AgentId>,
    #[serde(default)]
    pub kinds: Vec<JournalEntryKind>,
    #[serde(default)]
    pub outcomes: Vec<JournalOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct JournalPage {
    pub entries: Vec<JournalEntryView>,
    #[serde(default)]
    pub next_before_sequence: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskGrantEffect {
    WorkspaceWrite,
    Check,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskGrantView {
    pub id: String,
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub worktree: String,
    pub effect: TaskGrantEffect,
    pub expires_at: Timestamp,
    #[serde(default)]
    pub revoked: bool,
    /// The approval that created this durable grant, when the grant came
    /// from `Allow for this task`.  Keeping the link makes the projection
    /// repairable without exposing any approval secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_approval_id: Option<ApprovalId>,
}
