//! Approval requests, decisions and budget adjustments.

use frank_protocol::*;

use crate::*;

impl Orchestrator {
    pub(crate) async fn reduce_approval(
        &self,
        mut snapshot: Snapshot,
        command: Command,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        match command {
            Command::RequestApproval(spec) => {
                if spec.operation.trim().is_empty()
                    || spec.reason.trim().is_empty()
                    || spec.operation.len() > 4096
                    || spec.reason.len() > MAX_MESSAGE_BODY_BYTES
                    || spec
                        .operation
                        .chars()
                        .any(|character| character.is_control())
                    || spec.reason.chars().any(|character| character.is_control())
                    || spec.cwd.len() > 4_096
                    || spec.project.len() > 4_096
                    || spec.cwd.chars().any(|character| character.is_control())
                    || spec.project.chars().any(|character| character.is_control())
                {
                    return Err(OrchestratorError::Validation(
                        "approval operation or reason is invalid".into(),
                    ));
                }
                let task = snapshot
                    .tasks
                    .iter()
                    .find(|task| task.id == spec.task_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if !snapshot
                    .agents
                    .iter()
                    .any(|agent| agent.id == spec.agent_id && !agent.archived)
                    || !snapshot
                        .missions
                        .iter()
                        .any(|mission| mission.id == task.mission_id)
                {
                    return Err(OrchestratorError::NotFound);
                }
                if actor.kind == ActorKind::Agent {
                    let expected_actor_id = spec.agent_id.to_string();
                    if actor.id.as_deref() != Some(expected_actor_id.as_str()) {
                        return Err(OrchestratorError::Forbidden);
                    }
                }
                let approval = ApprovalView {
                    id: ApprovalId::new(),
                    agent_id: spec.agent_id,
                    task_id: spec.task_id,
                    operation: spec.operation,
                    cwd: spec.cwd,
                    project: spec.project,
                    reason: spec.reason,
                    status: ApprovalStatus::Pending,
                    expires_at: now_plus_seconds(snapshot.server.approval_ttl_seconds).to_string(),
                };
                snapshot.approvals.push(approval.clone());
                Ok((
                    snapshot,
                    Event::ApprovalRequested {
                        approval: approval.clone(),
                    },
                    CommandResult::Created {
                        id: approval.id.to_string(),
                    },
                ))
            }
            Command::DecideApproval {
                approval_id,
                decision,
            } => {
                let approval = snapshot
                    .approvals
                    .iter_mut()
                    .find(|approval| approval.id == approval_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if !matches!(approval.status, ApprovalStatus::Pending) {
                    return Err(OrchestratorError::InvalidTransition(
                        "approval is no longer pending".into(),
                    ));
                }
                if approval.expires_at.parse::<u128>().unwrap_or_default() <= now_plus_seconds(0) {
                    approval.status = ApprovalStatus::Expired;
                    let approval_id = approval.id;
                    return Ok((
                        snapshot,
                        Event::ApprovalExpired { approval_id },
                        CommandResult::Accepted,
                    ));
                }
                approval.status = match decision {
                    ApprovalDecision::AllowOnce => ApprovalStatus::Approved,
                    ApprovalDecision::DenyOnce => ApprovalStatus::Denied,
                };
                Ok((
                    snapshot,
                    Event::ApprovalDecided {
                        approval_id,
                        decision,
                    },
                    CommandResult::Accepted,
                ))
            }
            Command::AdjustBudget {
                scope,
                scope_id,
                budget,
            } => {
                if scope_id.trim().is_empty() {
                    return Err(OrchestratorError::Validation(
                        "budget scope id is required".into(),
                    ));
                }
                match scope {
                    BudgetScope::Mission => {
                        let target_id =
                            MissionId::parse(&scope_id).map_err(|_| OrchestratorError::NotFound)?;
                        let target = snapshot
                            .missions
                            .iter_mut()
                            .find(|mission| mission.id == target_id)
                            .ok_or(OrchestratorError::NotFound)?;
                        target.budget = budget;
                    }
                    BudgetScope::Agent => {
                        let target_id =
                            AgentId::parse(&scope_id).map_err(|_| OrchestratorError::NotFound)?;
                        let target = snapshot
                            .agents
                            .iter_mut()
                            .find(|agent| agent.id == target_id)
                            .ok_or(OrchestratorError::NotFound)?;
                        target.budget = budget;
                    }
                    BudgetScope::Task => {
                        let target_id =
                            TaskId::parse(&scope_id).map_err(|_| OrchestratorError::NotFound)?;
                        let target = snapshot
                            .tasks
                            .iter_mut()
                            .find(|task| task.id == target_id)
                            .ok_or(OrchestratorError::NotFound)?;
                        target.budget = budget;
                    }
                }
                Ok((
                    snapshot,
                    Event::BudgetPaused {
                        scope,
                        reason: format!("budget updated for {scope_id}"),
                    },
                    CommandResult::Accepted,
                ))
            }
            _ => Err(OrchestratorError::Validation(
                "command was routed to the wrong reducer".into(),
            )),
        }
    }
}
