//! Bearer-token and agent-capability authentication, and the scoping that
//! decides how much of a snapshot a provider session is allowed to see.

use crate::*;

pub(crate) async fn authenticate(state: &ServerState, headers: &HeaderMap) -> Option<DeviceAuth> {
    let token = bearer(headers)?;
    state.pairing.authenticate(token).await
}

pub(crate) async fn agent_capability_from_headers(
    state: &ServerState,
    headers: &HeaderMap,
) -> Option<(AgentId, TaskId)> {
    let token = headers
        .get("x-frank-agent-token")
        .and_then(|value| value.to_str().ok())?;
    state.orchestrator.agent_capability_actor(token).await
}

/// Return the minimum projection a provider session needs for its MCP bridge.
/// Device clients continue to receive the complete snapshot; an agent token
/// never grants a read-all view of other missions, projects, or workers.
pub(crate) async fn scoped_agent_snapshot(
    state: &ServerState,
    agent_id: AgentId,
    task_id: TaskId,
) -> Option<Snapshot> {
    let mut snapshot = state.store.snapshot().await.ok()?;
    let task = snapshot
        .tasks
        .iter()
        .find(|task| task.id == task_id)?
        .clone();
    if task.assigned_agent != Some(agent_id) {
        return None;
    }
    let project_id = snapshot
        .missions
        .iter()
        .find(|mission| mission.id == task.mission_id)
        .map(|mission| mission.project_id)?;
    snapshot
        .agents
        .retain(|agent| agent.id == agent_id || agent.display_name == "Frank supervisor");
    snapshot
        .missions
        .retain(|mission| mission.id == task.mission_id);
    snapshot.projects.retain(|project| project.id == project_id);
    snapshot.tasks.retain(|candidate| candidate.id == task_id);
    snapshot
        .messages
        .retain(|message| message.task_id == Some(task_id));
    snapshot
        .approvals
        .retain(|approval| approval.task_id == task_id);
    snapshot
        .artifacts
        .retain(|artifact| artifact.task_id == Some(task_id));
    snapshot
        .usage
        .retain(|usage| usage.scope == BudgetScope::Task && usage.scope_id == task_id.to_string());
    snapshot
        .terminals
        .retain(|terminal| terminal.task_id == task_id);
    // Do not expose server filesystem policy or worktree roots through the
    // agent-facing projection. The task worktree is already carried by the
    // scoped TaskView and checked again by the daemon.
    snapshot.server.allowed_project_roots.clear();
    snapshot.server.worktree_root.clear();
    Some(snapshot)
}

/// Enforce the second half of agent-session authorization at the transport
/// boundary.  The orchestrator still performs the authoritative state and
/// transition checks, but a short-lived capability must never become a
/// general-purpose operator token merely because the caller also possesses a
/// valid device bearer.  Every provider mutation is therefore tied to the
/// task encoded in the capability and, where applicable, to the agent that is
/// assigned to that task.
pub(crate) async fn agent_command_is_scoped(
    state: &ServerState,
    command: &Command,
    agent_id: AgentId,
    task_id: TaskId,
) -> bool {
    let Ok(snapshot) = state.store.snapshot().await else {
        return false;
    };
    let Some(scope_task) = snapshot.tasks.iter().find(|task| task.id == task_id) else {
        return false;
    };
    if scope_task.assigned_agent != Some(agent_id) {
        return false;
    }
    let mission_id = scope_task.mission_id;
    let agent_text = agent_id.to_string();
    let task_is_owned = |candidate: TaskId| {
        snapshot
            .tasks
            .iter()
            .find(|task| task.id == candidate)
            .is_some_and(|task| {
                task.mission_id == mission_id && task.assigned_agent == Some(agent_id)
            })
    };

    match command {
        Command::UpdateTask {
            task_id: candidate,
            patch,
        } => {
            if *candidate != task_id {
                return false;
            }
            // A worker can report task metadata and status, but it cannot
            // raise or remove its own hard budget. Budget changes remain an
            // operator/supervisor action even when the provider has a valid
            // scoped session capability.
            if patch.budget.is_some() {
                return false;
            }
            // A worker may update its own task metadata, but cannot detach the
            // task or hand it to another agent through the generic patch.
            match patch.assigned_agent {
                None => true,
                Some(Some(id)) => id == agent_id,
                Some(None) => false,
            }
        }
        Command::SetTaskStatus {
            task_id: candidate,
            status,
        } => *candidate == task_id && matches!(status, TaskStatus::Review | TaskStatus::Blocked),
        Command::CreateTask(spec) => {
            spec.mission_id == mission_id
                && spec
                    .assigned_agent
                    .is_none_or(|assigned| assigned == agent_id)
                && spec
                    .dependencies
                    .iter()
                    .all(|dependency| *dependency == task_id || task_is_owned(*dependency))
        }
        Command::SendMessage(spec) => {
            spec.mission_id == mission_id && spec.task_id == Some(task_id)
        }
        Command::AckMessage { message_id } => snapshot.messages.iter().any(|message| {
            message.id == *message_id
                && message.mission_id == mission_id
                && message.task_id == Some(task_id)
                && message.recipient.kind == ActorKind::Agent
                && message.recipient.id.as_deref() == Some(agent_text.as_str())
        }),
        Command::CompleteMessage { message_id, .. } => snapshot.messages.iter().any(|message| {
            message.id == *message_id
                && message.mission_id == mission_id
                && message.task_id == Some(task_id)
                && message.recipient.kind == ActorKind::Agent
                && message.recipient.id.as_deref() == Some(agent_text.as_str())
        }),
        Command::RequestApproval(spec) => spec.agent_id == agent_id && spec.task_id == task_id,
        Command::PublishArtifact(spec) => {
            spec.mission_id == mission_id && spec.task_id == Some(task_id)
        }
        Command::BeginArtifactUpload(spec) => {
            spec.mission_id == mission_id && spec.task_id == Some(task_id)
        }
        Command::FinalizeArtifactUpload { upload_id, .. } => snapshot
            .uploads
            .iter()
            .any(|upload| upload.id == *upload_id && upload.spec.task_id == Some(task_id)),
        Command::ProposeMemory {
            agent_id: candidate,
            ..
        }
        | Command::ReadMemory {
            agent_id: candidate,
            ..
        } => *candidate == agent_id,
        _ => false,
    }
}

pub(crate) fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get("authorization")?.to_str().ok()?;
    value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
}
