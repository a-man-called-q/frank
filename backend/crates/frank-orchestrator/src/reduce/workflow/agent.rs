//! Agent departure handler for Organization workflow.

use frank_protocol::*;

use super::append_task_feed;
use crate::*;

impl Orchestrator {
    pub(super) async fn reduce_fire_agent(
        &self,
        mut snapshot: Snapshot,
        agent_id: AgentId,
        reason: Option<String>,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        let agent_index = snapshot
            .agents
            .iter()
            .position(|agent| agent.id == agent_id && !agent.archived)
            .ok_or(OrchestratorError::NotFound)?;
        if snapshot.agents[agent_index].display_name == "Frank supervisor" {
            return Err(OrchestratorError::Validation(
                "the built-in Frank supervisor cannot be fired".into(),
            ));
        }
        let blocked_task_ids = snapshot
            .tasks
            .iter()
            .filter(|task| {
                task.assigned_agent == Some(agent_id)
                    && !matches!(task.status, TaskStatus::Done | TaskStatus::Cancelled)
            })
            .map(|task| task.id)
            .collect::<Vec<_>>();
        for task_id in &blocked_task_ids {
            if let Some(task) = snapshot.tasks.iter_mut().find(|task| task.id == *task_id) {
                task.assigned_agent = None;
                task.claimed_at = None;
                task.claim_source = None;
                task.status = TaskStatus::Blocked;
            }
            append_task_feed(
                &mut snapshot,
                *task_id,
                actor,
                TaskFeedKind::AgentDeparted,
                format!("Agent {agent_id} departed; card is blocked for handoff assessment"),
                Vec::new(),
            );
        }
        let agent = &mut snapshot.agents[agent_index];
        agent.archived = true;
        agent.status = AgentStatus::Offline;
        agent.provider_session_id = None;
        Ok((
            snapshot,
            Event::AgentDeparted {
                agent_id,
                reason,
                blocked_task_ids,
            },
            CommandResult::Accepted,
        ))
    }
}
