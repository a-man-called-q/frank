//! Agent lifecycle.

use frank_protocol::*;

use crate::*;

impl Orchestrator {
    pub(crate) async fn reduce_agent(
        &self,
        mut snapshot: Snapshot,
        command: Command,
        _actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        match command {
            Command::CreateAgent(spec) => {
                if !valid_agent_text(&spec.display_name, 128)
                    || spec.instructions.len() > MAX_MESSAGE_BODY_BYTES
                    || !valid_optional_agent_text(spec.model.as_deref(), 256)
                    || !valid_optional_agent_text(spec.pack_id.as_deref(), 128)
                    || !valid_optional_agent_text(spec.pack_level.as_deref(), 128)
                {
                    return Err(OrchestratorError::Validation(
                        "agent identity or instructions are invalid".into(),
                    ));
                }
                if snapshot.agents.iter().any(|agent| {
                    !agent.archived && agent.display_name.eq_ignore_ascii_case(&spec.display_name)
                }) {
                    return Err(OrchestratorError::Validation(
                        "agent display name must be unique".into(),
                    ));
                }
                let probe = self
                    .runtime
                    .doctor()
                    .await
                    .into_iter()
                    .find(|probe| probe.capability.provider == spec.provider);
                if !probe
                    .as_ref()
                    .is_some_and(|probe| probe.capability.available && probe.capability.logged_in)
                {
                    let detail = probe
                        .and_then(|probe| probe.capability.diagnostic)
                        .unwrap_or_else(|| format!("{} is not available", spec.provider));
                    return Err(OrchestratorError::ProviderUnavailable(detail));
                }
                let id = AgentId::new();
                let agent = AgentView {
                    id,
                    display_name: spec.display_name,
                    template: spec.template,
                    provider: spec.provider,
                    model: spec.model,
                    pack_id: spec.pack_id,
                    pack_level: spec.pack_level,
                    instructions: spec.instructions,
                    policy: spec.policy,
                    budget: spec.budget,
                    avatar: spec.avatar,
                    status: AgentStatus::Offline,
                    provider_session_id: None,
                    archived: false,
                };
                snapshot.agents.push(agent.clone());
                Ok((
                    snapshot,
                    Event::AgentUpserted { agent },
                    CommandResult::Created { id: id.to_string() },
                ))
            }
            Command::UpdateAgent { agent_id, patch } => {
                let updated = {
                    let agent = snapshot
                        .agents
                        .iter_mut()
                        .find(|agent| agent.id == agent_id)
                        .ok_or(OrchestratorError::NotFound)?;
                    apply_agent_patch(agent, patch);
                    agent.clone()
                };
                if !valid_agent_text(&updated.display_name, 128)
                    || updated.instructions.len() > MAX_MESSAGE_BODY_BYTES
                    || !valid_optional_agent_text(updated.model.as_deref(), 256)
                    || !valid_optional_agent_text(updated.pack_id.as_deref(), 128)
                    || !valid_optional_agent_text(updated.pack_level.as_deref(), 128)
                    || snapshot.agents.iter().any(|candidate| {
                        candidate.id != agent_id
                            && !candidate.archived
                            && candidate
                                .display_name
                                .eq_ignore_ascii_case(&updated.display_name)
                    })
                {
                    return Err(OrchestratorError::Validation(
                        "agent identity or instructions are invalid".into(),
                    ));
                }
                Ok((
                    snapshot,
                    Event::AgentUpserted { agent: updated },
                    CommandResult::Accepted,
                ))
            }
            Command::ArchiveAgent { agent_id } => {
                let agent = snapshot
                    .agents
                    .iter_mut()
                    .find(|agent| agent.id == agent_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if agent.display_name == "Frank supervisor" {
                    return Err(OrchestratorError::Validation(
                        "the built-in Frank supervisor cannot be archived".into(),
                    ));
                }
                if matches!(
                    agent.status,
                    AgentStatus::Working | AgentStatus::Starting | AgentStatus::Thinking
                ) {
                    return Err(OrchestratorError::Validation(
                        "move or cancel assigned work before archiving an active agent".into(),
                    ));
                }
                agent.archived = true;
                Ok((
                    snapshot,
                    Event::AgentArchived { agent_id },
                    CommandResult::Accepted,
                ))
            }
            _ => Err(OrchestratorError::Validation(
                "command was routed to the wrong reducer".into(),
            )),
        }
    }
}
