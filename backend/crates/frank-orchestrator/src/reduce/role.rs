//! Team role templates and their idle-boundary application policy.

use frank_protocol::*;

use crate::*;

fn validate_role(role: &RoleView) -> Result<()> {
    if !valid_agent_text(&role.name, 128)
        || role.instructions.len() > MAX_MESSAGE_BODY_BYTES
        || !valid_optional_agent_text(role.model.as_deref(), 256)
        || role.model.is_none()
    {
        return Err(OrchestratorError::Validation(
            "role name and canonical model are required".into(),
        ));
    }
    Ok(())
}

pub(crate) fn apply_role_patch(role: &mut RoleView, patch: RolePatch) {
    if let Some(value) = patch.name {
        role.name = value;
    }
    if patch.clear_model {
        role.model = None;
    } else if let Some(value) = patch.model {
        role.model = value;
    }
    if let Some(value) = patch.instructions {
        role.instructions = value;
    }
    if let Some(value) = patch.policy {
        role.policy = value;
    }
    if let Some(value) = patch.budget {
        role.budget = value;
    }
}

/// Materialize a role's full template into an agent. This is deliberately
/// called only at creation, assignment, or an idle boundary; a working
/// provider session therefore keeps the exact configuration it started with.
pub(crate) fn materialize_role(agent: &mut AgentView, role: &RoleView) {
    agent.role_id = Some(role.id);
    agent.role_revision = role.revision;
    agent.model = agent.model_override.clone().or_else(|| role.model.clone());
    agent.effective_model = agent.model.clone();
    agent.model_source = if agent.model_override.is_some() {
        ModelSource::Agent
    } else {
        ModelSource::Role
    };
    agent.instructions = role.instructions.clone();
    agent.policy = role.policy.clone();
    agent.budget = role.budget.clone();
}

impl Orchestrator {
    pub(crate) async fn reduce_role(
        &self,
        mut snapshot: Snapshot,
        command: Command,
        _actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        match command {
            Command::CreateRole(spec) => {
                if !valid_agent_text(&spec.name, 128)
                    || spec.instructions.len() > MAX_MESSAGE_BODY_BYTES
                    || !valid_optional_agent_text(spec.model.as_deref(), 256)
                    || spec.model.is_none()
                {
                    return Err(OrchestratorError::Validation(
                        "role name and canonical model are required".into(),
                    ));
                }
                if let Some(model) = spec.model.as_deref() {
                    self.validate_openrouter_model(Some(model)).await?;
                }
                if snapshot
                    .roles
                    .iter()
                    .any(|role| !role.archived && role.name.eq_ignore_ascii_case(&spec.name))
                {
                    return Err(OrchestratorError::Validation(
                        "role name must be unique".into(),
                    ));
                }
                let id = RoleId::new();
                let role = RoleView {
                    id,
                    name: spec.name,
                    model: spec.model,
                    instructions: spec.instructions,
                    policy: spec.policy,
                    budget: spec.budget,
                    revision: 1,
                    archived: false,
                };
                validate_role(&role)?;
                snapshot.roles.push(role.clone());
                Ok((
                    snapshot,
                    Event::RoleUpserted { role },
                    CommandResult::Created { id: id.to_string() },
                ))
            }
            Command::UpdateRole { role_id, patch } => {
                if !patch.clear_model
                    && let Some(Some(model)) = patch.model.as_ref()
                {
                    self.validate_openrouter_model(Some(model)).await?;
                }
                if patch.clear_model || matches!(patch.model, Some(None)) {
                    return Err(OrchestratorError::Validation(
                        "a role must keep a canonical model".into(),
                    ));
                }
                let role = snapshot
                    .roles
                    .iter_mut()
                    .find(|role| role.id == role_id && !role.archived)
                    .ok_or(OrchestratorError::NotFound)?;
                apply_role_patch(role, patch);
                role.revision = role.revision.saturating_add(1);
                let updated = role.clone();
                if snapshot.roles.iter().any(|candidate| {
                    candidate.id != role_id
                        && !candidate.archived
                        && candidate.name.eq_ignore_ascii_case(&updated.name)
                }) {
                    return Err(OrchestratorError::Validation(
                        "role name must be unique".into(),
                    ));
                }
                validate_role(&updated)?;
                Ok((
                    snapshot,
                    Event::RoleUpserted { role: updated },
                    CommandResult::Accepted,
                ))
            }
            Command::ArchiveRole { role_id } => {
                let role = snapshot
                    .roles
                    .iter_mut()
                    .find(|role| role.id == role_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if snapshot
                    .agents
                    .iter()
                    .any(|agent| agent.role_id == Some(role_id) && !agent.archived)
                    || snapshot
                        .tasks
                        .iter()
                        .any(|task| task.required_role_id == Some(role_id))
                {
                    return Err(OrchestratorError::Validation(
                        "a role cannot be archived while agents or tasks use it".into(),
                    ));
                }
                role.archived = true;
                Ok((
                    snapshot,
                    Event::RoleArchived { role_id },
                    CommandResult::Accepted,
                ))
            }
            Command::SetAgentRole { agent_id, role_id } => {
                let role = snapshot
                    .roles
                    .iter()
                    .find(|role| role.id == role_id && !role.archived)
                    .cloned()
                    .ok_or(OrchestratorError::NotFound)?;
                let agent = snapshot
                    .agents
                    .iter_mut()
                    .find(|agent| agent.id == agent_id && !agent.archived)
                    .ok_or(OrchestratorError::NotFound)?;
                if !matches!(agent.status, AgentStatus::Offline | AgentStatus::Idle) {
                    return Err(OrchestratorError::Validation(
                        "an agent's role can change only while it is idle".into(),
                    ));
                }
                materialize_role(agent, &role);
                let updated = agent.clone();
                Ok((
                    snapshot,
                    Event::AgentUpserted { agent: updated },
                    CommandResult::Accepted,
                ))
            }
            _ => super::misrouted(),
        }
    }
}
