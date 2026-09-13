//! Agent lifecycle.

use frank_protocol::*;

use crate::reduce::materialize_role;
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
                let role_id = spec.role_id.ok_or_else(|| {
                    OrchestratorError::Validation(
                        "a role is required when creating a new agent".into(),
                    )
                })?;
                let role = Some(
                    snapshot
                        .roles
                        .iter()
                        .find(|role| role.id == role_id && !role.archived)
                        .cloned()
                        .ok_or(OrchestratorError::NotFound)?,
                );
                if !valid_agent_text(&spec.display_name, 128)
                    || spec.instructions.len() > MAX_MESSAGE_BODY_BYTES
                    || !valid_optional_agent_text(spec.model.as_deref(), 256)
                    || !valid_optional_agent_text(spec.model_override.as_deref(), 256)
                    || !valid_optional_agent_text(spec.pack_id.as_deref(), 128)
                    || !valid_optional_agent_text(spec.pack_level.as_deref(), 128)
                {
                    return Err(OrchestratorError::Validation(
                        "agent identity or instructions are invalid".into(),
                    ));
                }
                let effective_model = spec
                    .model_override
                    .as_deref()
                    .or_else(|| role.as_ref().and_then(|role| role.model.as_deref()))
                    .or(spec.model.as_deref());
                self.validate_openrouter_model(effective_model).await?;
                if snapshot.agents.iter().any(|agent| {
                    !agent.archived && agent.display_name.eq_ignore_ascii_case(&spec.display_name)
                }) {
                    return Err(OrchestratorError::Validation(
                        "agent display name must be unique".into(),
                    ));
                }
                let id = AgentId::new();
                let role_revision = role.as_ref().map(|role| role.revision).unwrap_or_default();
                let agent = AgentView {
                    id,
                    role_id: Some(role_id),
                    role_revision,
                    display_name: spec.display_name,
                    template: role.as_ref().map_or(spec.template, |role| role.template),
                    model: spec.model_override.clone().or_else(|| {
                        role.as_ref()
                            .map_or(spec.model.clone(), |role| role.model.clone())
                    }),
                    effective_model: spec.model_override.clone().or_else(|| {
                        role.as_ref()
                            .map_or(spec.model.clone(), |role| role.model.clone())
                    }),
                    model_source: if spec.model_override.is_some() {
                        ModelSource::Agent
                    } else {
                        ModelSource::Role
                    },
                    model_override: spec.model_override.clone(),
                    pending_model_override: None,
                    pending_model_change: false,
                    pack_id: role
                        .as_ref()
                        .map_or(spec.pack_id.clone(), |role| role.pack_id.clone()),
                    pack_level: role
                        .as_ref()
                        .map_or(spec.pack_level.clone(), |role| role.pack_level.clone()),
                    instructions: role
                        .as_ref()
                        .map_or(spec.instructions.clone(), |role| role.instructions.clone()),
                    policy: role
                        .as_ref()
                        .map_or(spec.policy.clone(), |role| role.policy.clone()),
                    budget: role
                        .as_ref()
                        .map_or(spec.budget.clone(), |role| role.budget.clone()),
                    avatar: role
                        .as_ref()
                        .map_or(spec.avatar.clone(), |role| role.avatar.clone()),
                    status: AgentStatus::Offline,
                    provider_session_id: None,
                    last_claimed_at: None,
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
                let existing_agent = snapshot
                    .agents
                    .iter()
                    .find(|agent| agent.id == agent_id)
                    .cloned()
                    .ok_or(OrchestratorError::NotFound)?;
                if let Some(Some(model)) = patch.model.as_ref() {
                    self.validate_openrouter_model(Some(model)).await?;
                }
                if !patch.clear_model_override
                    && let Some(Some(model)) = patch.model_override.as_ref()
                {
                    self.validate_openrouter_model(Some(model)).await?;
                }
                let requested_role = patch.role_id;
                let existing_role_id = existing_agent.role_id;
                if requested_role.is_none()
                    && existing_role_id.is_some()
                    && patch_changes_role_owned_fields(&patch)
                {
                    return Err(OrchestratorError::Validation(
                        "edit the agent's role template instead of overriding role-owned settings"
                            .into(),
                    ));
                }
                let role = match requested_role {
                    Some(Some(role_id)) => Some(
                        snapshot
                            .roles
                            .iter()
                            .find(|role| role.id == role_id && !role.archived)
                            .cloned()
                            .ok_or(OrchestratorError::NotFound)?,
                    ),
                    Some(None) | None => None,
                };
                let updated = {
                    let agent = snapshot
                        .agents
                        .iter_mut()
                        .find(|agent| agent.id == agent_id)
                        .ok_or(OrchestratorError::NotFound)?;
                    if requested_role.is_some()
                        && !matches!(agent.status, AgentStatus::Offline | AgentStatus::Idle)
                    {
                        return Err(OrchestratorError::Validation(
                            "an agent's role can change only while it is idle".into(),
                        ));
                    }
                    let model_override = if patch.clear_model_override {
                        Some(None)
                    } else {
                        patch.model_override.clone()
                    };
                    apply_agent_patch(agent, patch);
                    if let Some(model_override) = model_override {
                        if matches!(agent.status, AgentStatus::Offline | AgentStatus::Idle) {
                            agent.model_override = model_override;
                            agent.pending_model_override = None;
                            agent.pending_model_change = false;
                            if let Some(role_id) = agent.role_id
                                && let Some(role) =
                                    snapshot.roles.iter().find(|role| role.id == role_id)
                            {
                                agent.model =
                                    agent.model_override.clone().or_else(|| role.model.clone());
                            } else {
                                agent.model = agent.model_override.clone();
                            }
                            agent.effective_model = agent.model.clone();
                            agent.model_source = if agent.model_override.is_some() {
                                ModelSource::Agent
                            } else {
                                ModelSource::Role
                            };
                        } else {
                            agent.pending_model_override = Some(model_override);
                            agent.pending_model_change = true;
                        }
                    }
                    if let Some(role) = role.as_ref() {
                        materialize_role(agent, role);
                    } else if requested_role == Some(None) {
                        agent.role_id = None;
                        agent.role_revision = 0;
                    }
                    agent.effective_model = agent.model.clone();
                    agent.model_source = if agent.model_override.is_some() {
                        ModelSource::Agent
                    } else {
                        ModelSource::Role
                    };
                    agent.clone()
                };
                if !valid_agent_text(&updated.display_name, 128)
                    || updated.instructions.len() > MAX_MESSAGE_BODY_BYTES
                    || !valid_optional_agent_text(updated.model.as_deref(), 256)
                    || !valid_optional_agent_text(updated.model_override.as_deref(), 256)
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
                    .iter()
                    .find(|agent| agent.id == agent_id)
                    .cloned()
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
                if snapshot.tasks.iter().any(|task| {
                    task.assigned_agent == Some(agent_id)
                        && !matches!(task.status, TaskStatus::Done | TaskStatus::Cancelled)
                }) {
                    return Err(OrchestratorError::Validation(
                        "move or release unfinished work before archiving the agent".into(),
                    ));
                }
                if snapshot.organization.draft.nodes.iter().any(|node| {
                    node.kind == OrganizationNodeKind::Staff && node.agent_id == Some(agent_id)
                }) || snapshot
                    .organization
                    .published
                    .as_ref()
                    .is_some_and(|graph| {
                        graph.nodes.iter().any(|node| {
                            node.kind == OrganizationNodeKind::Staff
                                && node.agent_id == Some(agent_id)
                        })
                    })
                {
                    return Err(OrchestratorError::Validation(
                        "remove the agent from Organization before archiving it".into(),
                    ));
                }
                snapshot
                    .agents
                    .iter_mut()
                    .find(|candidate| candidate.id == agent_id)
                    .expect("agent checked")
                    .archived = true;
                Ok((
                    snapshot,
                    Event::AgentArchived { agent_id },
                    CommandResult::Accepted,
                ))
            }
            _ => super::misrouted(),
        }
    }
}

fn patch_changes_role_owned_fields(patch: &AgentPatch) -> bool {
    patch.model.is_some()
        || patch.clear_model_override
        || patch.pack_id.is_some()
        || patch.pack_level.is_some()
        || patch.instructions.is_some()
        || patch.policy.is_some()
        || patch.budget.is_some()
        || patch.avatar.is_some()
}
