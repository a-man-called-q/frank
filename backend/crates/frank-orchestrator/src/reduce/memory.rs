//! Agent memory proposals and reads.

use frank_protocol::*;

use crate::*;

impl Orchestrator {
    pub(crate) async fn reduce_memory(
        &self,
        mut snapshot: Snapshot,
        command: Command,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        match command {
            Command::ProposeMemory {
                agent_id,
                path,
                content,
            } => {
                if content.len() > MAX_MESSAGE_BODY_BYTES
                    || path.contains("..")
                    || !path.ends_with("memory.md")
                {
                    return Err(OrchestratorError::Validation(
                        "memory proposal path or size is invalid".into(),
                    ));
                }
                if !snapshot.agents.iter().any(|agent| agent.id == agent_id) {
                    return Err(OrchestratorError::NotFound);
                }
                if actor.kind == ActorKind::Agent
                    && actor.id.as_deref().and_then(|id| AgentId::parse(id).ok()) != Some(agent_id)
                {
                    return Err(OrchestratorError::Forbidden);
                }
                let resource = serde_json::to_string(&WriteMemoryOperation {
                    agent_id,
                    path,
                    content,
                })
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
                let now = timestamp_now();
                let operation = OperationView {
                    id: OperationId::new(),
                    kind: OperationKind::WriteMemory,
                    status: OperationStatus::Queued,
                    resource,
                    phase: "queued".into(),
                    attempt: 0,
                    error: None,
                    created_at: now.clone(),
                    updated_at: now,
                };
                snapshot.operations.push(operation.clone());
                Ok((
                    snapshot,
                    Event::OperationChanged {
                        operation: operation.clone(),
                    },
                    CommandResult::Operation(operation),
                ))
            }
            Command::ReadMemory { agent_id, path } => {
                if path.contains("..") || !path.ends_with("memory.md") {
                    return Err(OrchestratorError::Validation(
                        "memory path is invalid".into(),
                    ));
                }
                if !snapshot.agents.iter().any(|agent| agent.id == agent_id) {
                    return Err(OrchestratorError::NotFound);
                }
                if actor.kind == ActorKind::Agent
                    && actor.id.as_deref().and_then(|id| AgentId::parse(id).ok()) != Some(agent_id)
                {
                    return Err(OrchestratorError::Forbidden);
                }
                let content = frank_store::memory::MemoryRepository::new(&self.memory_root)
                    .read(agent_id, &path)
                    .map_err(|error| OrchestratorError::Validation(error.to_string()))?
                    .unwrap_or_default();
                Ok((
                    snapshot,
                    Event::MemoryRead {
                        agent_id,
                        path: path.clone(),
                    },
                    CommandResult::Memory { path, content },
                ))
            }
            _ => super::misrouted(),
        }
    }
}
