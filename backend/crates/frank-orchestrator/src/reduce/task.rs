//! Task creation, status transitions, assignment and acceptance.

use frank_protocol::*;

use crate::reduce::materialize_role;
use crate::*;

mod assignment;
mod create_update;
mod status_review;

impl Orchestrator {
    pub(crate) async fn reduce_task(
        &self,
        snapshot: Snapshot,
        command: Command,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        match command {
            Command::CreateTask(spec) => self.reduce_create_task(snapshot, spec, actor).await,
            Command::UpdateTask { task_id, patch } => {
                self.reduce_update_task(snapshot, task_id, patch, actor)
                    .await
            }
            Command::SetTaskStatus { task_id, status } => {
                self.reduce_set_task_status(snapshot, task_id, status, actor)
                    .await
            }
            Command::AssignTask { task_id, agent_id } => {
                self.reduce_assign_task(snapshot, task_id, agent_id, actor)
                    .await
            }
            Command::ClaimTask {
                task_id,
                agent_id,
                source,
            } => {
                self.reduce_claim_task(snapshot, task_id, agent_id, source, actor)
                    .await
            }
            Command::ReleaseTask { task_id } => {
                self.reduce_release_task(snapshot, task_id, actor).await
            }
            Command::AddTaskComment {
                task_id,
                body,
                artifact_ids,
            } => {
                self.reduce_add_task_comment(snapshot, task_id, body, artifact_ids, actor)
                    .await
            }
            Command::TaskAccept { task_id } => {
                self.reduce_task_accept(snapshot, task_id, actor).await
            }
            Command::DecideReview {
                review_item_id,
                decision,
                reason,
            } => {
                self.reduce_decide_review(snapshot, review_item_id, decision, reason, actor)
                    .await
            }
            _ => super::misrouted(),
        }
    }
}

fn assign_task(
    snapshot: &mut Snapshot,
    task_id: TaskId,
    agent_id: AgentId,
    source: TaskClaimSource,
    feed_kind: TaskFeedKind,
    actor: &ActorRef,
) -> Result<()> {
    if !organization_allows_agent(snapshot, agent_id) {
        return Err(OrchestratorError::Validation(
            "agent is not an active Organization staff member".into(),
        ));
    }
    let agent = snapshot
        .agents
        .iter()
        .find(|agent| agent.id == agent_id && !agent.archived)
        .cloned()
        .ok_or(OrchestratorError::NotFound)?;
    if actor.kind == ActorKind::Agent {
        let source = actor
            .id
            .as_deref()
            .and_then(|id| AgentId::parse(id).ok())
            .ok_or(OrchestratorError::Forbidden)?;
        if !organization_allows_handoff(snapshot, source, agent_id) {
            return Err(OrchestratorError::Forbidden);
        }
    }
    if !matches!(agent.status, AgentStatus::Offline | AgentStatus::Idle) {
        return Err(OrchestratorError::Validation(
            "only an idle agent can claim a task".into(),
        ));
    }
    let task = snapshot
        .tasks
        .iter()
        .find(|task| task.id == task_id)
        .cloned()
        .ok_or(OrchestratorError::NotFound)?;
    if snapshot.organization_runtime.status == OrganizationDrainStatus::Draining {
        return Err(OrchestratorError::Validation(
            "Organization is draining; no new work may be claimed".into(),
        ));
    }
    if snapshot.tasks.iter().any(|candidate| {
        candidate.id != task_id
            && candidate.assigned_agent == Some(agent_id)
            && !matches!(candidate.status, TaskStatus::Done | TaskStatus::Cancelled)
    }) {
        return Err(OrchestratorError::Validation(
            "an agent can claim only one unfinished task at a time".into(),
        ));
    }
    if matches!(
        task.status,
        TaskStatus::Done | TaskStatus::Cancelled | TaskStatus::Running | TaskStatus::Review
    ) {
        return Err(OrchestratorError::Validation(
            "only an available task can be claimed".into(),
        ));
    }
    if let Some(role_id) = task.required_role_id
        && agent.role_id != Some(role_id)
    {
        return Err(OrchestratorError::Validation(
            "agent does not belong to the task's required role".into(),
        ));
    }
    let mission = snapshot
        .missions
        .iter()
        .find(|mission| mission.id == task.mission_id)
        .ok_or(OrchestratorError::NotFound)?;
    if matches!(
        mission.status,
        MissionStatus::Completed | MissionStatus::Failed | MissionStatus::Cancelled
    ) {
        return Err(OrchestratorError::Validation(
            "cannot claim work on a final mission".into(),
        ));
    }
    if task.dependencies.iter().any(|dependency| {
        snapshot
            .tasks
            .iter()
            .find(|candidate| candidate.id == *dependency)
            .is_none_or(|candidate| candidate.status != TaskStatus::Done)
    }) {
        return Err(OrchestratorError::Validation(
            "task dependencies must be done before claiming this task".into(),
        ));
    }
    let claimed_at = timestamp_now();
    let claimed_at_value = {
        let task = snapshot
            .tasks
            .iter_mut()
            .find(|task| task.id == task_id)
            .expect("task checked");
        if task
            .assigned_agent
            .is_some_and(|id| id != agent_id || !matches!(source, TaskClaimSource::Reclaim))
        {
            return Err(OrchestratorError::Validation(
                "task is already claimed; release it before claiming again".into(),
            ));
        }
        task.assigned_agent = Some(agent_id);
        task.claimed_at = Some(claimed_at);
        task.claim_source = Some(source);
        if task.status == TaskStatus::Backlog {
            task.status = TaskStatus::Ready;
        }
        task.claimed_at.clone()
    };
    // A new owner must never inherit the previous owner's task-scoped grant.
    for grant in &mut snapshot.task_grants {
        if grant.task_id == task_id && grant.agent_id != agent_id {
            grant.revoked = true;
        }
    }
    let agent_name = {
        let agent = snapshot
            .agents
            .iter_mut()
            .find(|agent| agent.id == agent_id)
            .expect("agent checked");
        if let Some(role_id) = agent.role_id
            && let Some(role) = snapshot
                .roles
                .iter()
                .find(|role| role.id == role_id && !role.archived)
                .cloned()
            && role.revision > agent.role_revision
        {
            materialize_role(agent, &role);
        }
        agent.last_claimed_at = claimed_at_value;
        agent.display_name.clone()
    };
    let feed_body = if feed_kind == TaskFeedKind::Assigned {
        format!("Task assigned to {agent_name}")
    } else {
        format!("Task claimed by {agent_name}")
    };
    append_task_feed(snapshot, task_id, actor, feed_kind, feed_body, Vec::new());
    Ok(())
}

pub(crate) fn update_dependency_locks(
    snapshot: &mut Snapshot,
    dependency_id: TaskId,
    actor: &ActorRef,
    dependency_status: TaskStatus,
) {
    let dependent_ids = snapshot
        .tasks
        .iter()
        .filter(|task| task.dependencies.contains(&dependency_id))
        .map(|task| task.id)
        .collect::<Vec<_>>();
    for dependent_id in dependent_ids {
        let Some(index) = snapshot
            .tasks
            .iter()
            .position(|task| task.id == dependent_id)
        else {
            continue;
        };
        let unblocked = snapshot.tasks[index].dependencies.iter().all(|dependency| {
            snapshot
                .tasks
                .iter()
                .find(|task| task.id == *dependency)
                .is_some_and(|task| task.status == TaskStatus::Done)
        });
        if dependency_status == TaskStatus::Done
            && unblocked
            && snapshot.tasks[index].status == TaskStatus::Blocked
        {
            snapshot.tasks[index].status = TaskStatus::Ready;
            append_task_feed(
                snapshot,
                dependent_id,
                actor,
                TaskFeedKind::DependencyUnlocked,
                "Dependency completed; task is now available".into(),
                Vec::new(),
            );
        } else if dependency_status != TaskStatus::Done
            && snapshot.tasks[index].status == TaskStatus::Ready
        {
            snapshot.tasks[index].status = TaskStatus::Blocked;
            append_task_feed(
                snapshot,
                dependent_id,
                actor,
                TaskFeedKind::DependencyLocked,
                "Task unavailable until its dependency is done".into(),
                Vec::new(),
            );
        }
    }
}

/// Child fan-out is a structural dependency in addition to the explicit DAG
/// dependencies. A parent stays blocked while any child is unfinished and is
/// promoted once the final child reaches Done.
pub(crate) fn update_parent_child_locks(
    snapshot: &mut Snapshot,
    child_id: TaskId,
    actor: &ActorRef,
) {
    let Some(parent_id) = snapshot
        .tasks
        .iter()
        .find(|task| task.id == child_id)
        .and_then(|task| task.parent_task_id)
    else {
        return;
    };
    let Some(parent_index) = snapshot.tasks.iter().position(|task| task.id == parent_id) else {
        return;
    };
    let all_done = snapshot.tasks[parent_index]
        .child_task_ids
        .iter()
        .all(|child| {
            snapshot
                .tasks
                .iter()
                .find(|task| task.id == *child)
                .is_some_and(|task| task.status == TaskStatus::Done)
        });
    if all_done && snapshot.tasks[parent_index].status == TaskStatus::Blocked {
        snapshot.tasks[parent_index].status = TaskStatus::Ready;
        append_task_feed(
            snapshot,
            parent_id,
            actor,
            TaskFeedKind::DependencyUnlocked,
            "All child work items completed; parent is ready".into(),
            Vec::new(),
        );
    }
}

pub(crate) fn append_task_feed(
    snapshot: &mut Snapshot,
    task_id: TaskId,
    actor: &ActorRef,
    kind: TaskFeedKind,
    body: String,
    artifact_ids: Vec<ArtifactId>,
) -> TaskFeedEntry {
    let entry = TaskFeedEntry {
        id: TaskFeedId::new(),
        task_id,
        actor: actor.clone(),
        kind,
        body,
        artifact_ids,
        created_at: timestamp_now(),
    };
    snapshot.task_feed.insert(0, entry.clone());
    // Keep the wire snapshot bounded. The SQLite projection is likewise
    // capped during replay, while older events remain available in the
    // append-only event/audit log when an operator needs historical evidence.
    snapshot.task_feed.truncate(2_048);
    entry
}
