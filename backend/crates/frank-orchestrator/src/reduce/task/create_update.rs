//! Task creation and patching handlers.

use frank_protocol::*;

use super::append_task_feed;
use crate::*;

impl Orchestrator {
    pub(super) async fn reduce_create_task(
        &self,
        mut snapshot: Snapshot,
        spec: TaskSpec,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        let mission = snapshot
            .missions
            .iter()
            .find(|mission| mission.id == spec.mission_id)
            .ok_or(OrchestratorError::NotFound)?;
        if matches!(
            mission.status,
            MissionStatus::Completed | MissionStatus::Failed | MissionStatus::Cancelled
        ) {
            return Err(OrchestratorError::Validation(
                "cannot add a task to a final mission".into(),
            ));
        }
        if let Some(agent_id) = spec.assigned_agent
            && !snapshot
                .agents
                .iter()
                .any(|agent| agent.id == agent_id && !agent.archived)
        {
            return Err(OrchestratorError::NotFound);
        }
        if let Some(agent_id) = spec.assigned_agent
            && !organization_allows_agent(&snapshot, agent_id)
        {
            return Err(OrchestratorError::Validation(
                "assigned agent is not an active Organization staff member".into(),
            ));
        }
        if let Some(role_id) = spec.required_role_id {
            if !snapshot
                .roles
                .iter()
                .any(|role| role.id == role_id && !role.archived)
            {
                return Err(OrchestratorError::NotFound);
            }
            if let Some(agent_id) = spec.assigned_agent
                && snapshot
                    .agents
                    .iter()
                    .find(|agent| agent.id == agent_id)
                    .and_then(|agent| agent.role_id)
                    != Some(role_id)
            {
                return Err(OrchestratorError::Validation(
                    "assigned agent does not belong to the task role".into(),
                ));
            }
        }
        validate_task_spec(&spec, &snapshot.tasks)?;
        let id = TaskId::new();
        let task = TaskView {
            id,
            mission_id: spec.mission_id,
            title: spec.title,
            objective: spec.objective,
            dependencies: spec.dependencies,
            required_role_id: spec.required_role_id,
            priority: spec.priority,
            budget: spec.budget,
            status: TaskStatus::Backlog,
            assigned_agent: spec.assigned_agent,
            reviewer_agent: None,
            claimed_at: None,
            claim_source: None,
            attempt: 0,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            worktree: None,
            branch: Some(format!("frank/task-{id}")),
            result_artifact: None,
            taskboard_id: spec.taskboard_id,
            workflow_id: spec.workflow_id,
            parent_task_id: spec.parent_task_id,
            child_task_ids: Vec::new(),
            kind: spec.kind,
            active_role_node_id: None,
            organization_revision: snapshot
                .organization
                .published
                .as_ref()
                .map(|graph| graph.published_revision),
            rework_limit: spec.rework_limit,
            rework_count: 0,
        };
        snapshot.tasks.push(task.clone());
        append_task_feed(
            &mut snapshot,
            id,
            actor,
            TaskFeedKind::Created,
            format!("Task created: {}", task.title),
            Vec::new(),
        );
        Ok((
            snapshot,
            Event::TaskCreated { task },
            CommandResult::Created { id: id.to_string() },
        ))
    }

    pub(super) async fn reduce_update_task(
        &self,
        mut snapshot: Snapshot,
        task_id: TaskId,
        patch: TaskPatch,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        let existing = snapshot
            .tasks
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        let mut candidate = existing.clone();
        apply_task_patch(&mut candidate, patch);
        if candidate.assigned_agent != existing.assigned_agent {
            return Err(OrchestratorError::Validation(
                "use claim, assign, or release commands to change task ownership".into(),
            ));
        }
        if let Some(agent_id) = candidate.assigned_agent
            && !snapshot
                .agents
                .iter()
                .any(|agent| agent.id == agent_id && !agent.archived)
        {
            return Err(OrchestratorError::NotFound);
        }
        if let Some(agent_id) = candidate.assigned_agent
            && !organization_allows_agent(&snapshot, agent_id)
        {
            return Err(OrchestratorError::Validation(
                "assigned agent is not an active Organization staff member".into(),
            ));
        }
        validate_task_view(&candidate, &snapshot.tasks)?;
        if let Some(role_id) = candidate.required_role_id
            && !snapshot
                .roles
                .iter()
                .any(|role| role.id == role_id && !role.archived)
        {
            return Err(OrchestratorError::NotFound);
        }
        if let (Some(role_id), Some(agent_id)) =
            (candidate.required_role_id, candidate.assigned_agent)
            && snapshot
                .agents
                .iter()
                .find(|agent| agent.id == agent_id)
                .and_then(|agent| agent.role_id)
                != Some(role_id)
        {
            return Err(OrchestratorError::Validation(
                "assigned agent does not belong to the task role".into(),
            ));
        }
        let dependencies_ready = candidate.dependencies.iter().all(|dependency| {
            snapshot
                .tasks
                .iter()
                .find(|task| task.id == *dependency)
                .is_some_and(|task| task.status == TaskStatus::Done)
        });
        if candidate.dependencies != existing.dependencies {
            if matches!(candidate.status, TaskStatus::Running | TaskStatus::Review) {
                return Err(OrchestratorError::Validation(
                    "dependencies cannot change after a task starts".into(),
                ));
            }
            if candidate.status == TaskStatus::Ready && !dependencies_ready {
                candidate.status = TaskStatus::Blocked;
            } else if candidate.status == TaskStatus::Blocked && dependencies_ready {
                candidate.status = TaskStatus::Ready;
            }
        }
        let task = snapshot
            .tasks
            .iter_mut()
            .find(|task| task.id == task_id)
            .expect("task checked");
        *task = candidate.clone();
        if candidate.status != existing.status {
            let kind = if candidate.status == TaskStatus::Ready {
                TaskFeedKind::DependencyUnlocked
            } else {
                TaskFeedKind::DependencyLocked
            };
            let body = if candidate.status == TaskStatus::Ready {
                "Dependency update completed; task is now available"
            } else {
                "Task unavailable until its dependency is done"
            };
            append_task_feed(&mut snapshot, task_id, actor, kind, body.into(), Vec::new());
        }
        append_task_feed(
            &mut snapshot,
            task_id,
            actor,
            TaskFeedKind::Comment,
            "Task details updated".into(),
            Vec::new(),
        );
        Ok((
            snapshot,
            Event::TaskUpdated { task: candidate },
            CommandResult::Accepted,
        ))
    }
}
