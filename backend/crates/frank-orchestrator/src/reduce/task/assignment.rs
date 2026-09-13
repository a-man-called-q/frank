//! Task assignment, claiming, releasing, and comment handlers.

use frank_protocol::*;

use super::{append_task_feed, assign_task};
use crate::*;

impl Orchestrator {
    pub(super) async fn reduce_assign_task(
        &self,
        mut snapshot: Snapshot,
        task_id: TaskId,
        agent_id: AgentId,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        assign_task(
            &mut snapshot,
            task_id,
            agent_id,
            TaskClaimSource::Manual,
            TaskFeedKind::Assigned,
            actor,
        )?;
        Ok((
            snapshot,
            Event::TaskAssigned { task_id, agent_id },
            CommandResult::Accepted,
        ))
    }

    pub(super) async fn reduce_claim_task(
        &self,
        mut snapshot: Snapshot,
        task_id: TaskId,
        agent_id: AgentId,
        source: TaskClaimSource,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        assign_task(
            &mut snapshot,
            task_id,
            agent_id,
            source,
            TaskFeedKind::Claimed,
            actor,
        )?;
        let claimed_at = snapshot
            .tasks
            .iter()
            .find(|task| task.id == task_id)
            .and_then(|task| task.claimed_at.clone())
            .unwrap_or_else(timestamp_now);
        Ok((
            snapshot,
            Event::TaskClaimed {
                task_id,
                agent_id,
                claimed_at,
                source,
            },
            CommandResult::Accepted,
        ))
    }

    pub(super) async fn reduce_release_task(
        &self,
        mut snapshot: Snapshot,
        task_id: TaskId,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        let task = snapshot
            .tasks
            .iter_mut()
            .find(|task| task.id == task_id)
            .ok_or(OrchestratorError::NotFound)?;
        if task.status == TaskStatus::Running {
            return Err(OrchestratorError::Validation(
                "a running task must be stopped before releasing its claim".into(),
            ));
        }
        task.assigned_agent = None;
        task.claimed_at = None;
        task.claim_source = None;
        append_task_feed(
            &mut snapshot,
            task_id,
            actor,
            TaskFeedKind::Released,
            "Task claim released".into(),
            Vec::new(),
        );
        Ok((
            snapshot,
            Event::TaskReleased { task_id },
            CommandResult::Accepted,
        ))
    }

    pub(super) async fn reduce_add_task_comment(
        &self,
        mut snapshot: Snapshot,
        task_id: TaskId,
        body: String,
        artifact_ids: Vec<ArtifactId>,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        if !valid_content_text(&body, self.max_message_bytes) {
            return Err(OrchestratorError::Validation(
                "task comment is empty or exceeds the message limit".into(),
            ));
        }
        let task = snapshot
            .tasks
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        if actor.kind == ActorKind::Agent
            && task.assigned_agent.map(|id| id.to_string()).as_deref() != actor.id.as_deref()
        {
            return Err(OrchestratorError::Forbidden);
        }
        if artifact_ids.len() > 256
            || artifact_ids.iter().any(|artifact_id| {
                !snapshot.artifacts.iter().any(|artifact| {
                    artifact.id == *artifact_id
                        && artifact.mission_id == task.mission_id
                        && (artifact.task_id == Some(task_id) || artifact.task_id.is_none())
                })
            })
        {
            return Err(OrchestratorError::NotFound);
        }
        let entry = append_task_feed(
            &mut snapshot,
            task_id,
            actor,
            TaskFeedKind::Comment,
            body,
            artifact_ids,
        );
        Ok((
            snapshot,
            Event::TaskActivityAdded {
                entry: entry.clone(),
            },
            CommandResult::Created {
                id: entry.id.to_string(),
            },
        ))
    }
}
