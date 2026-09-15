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
                let approval_index = snapshot
                    .approvals
                    .iter()
                    .position(|approval| approval.id == approval_id)
                    .ok_or(OrchestratorError::NotFound)?;
                let approval = snapshot.approvals[approval_index].clone();
                if !matches!(approval.status, ApprovalStatus::Pending) {
                    return Err(OrchestratorError::InvalidTransition(
                        "approval is no longer pending".into(),
                    ));
                }
                if approval.expires_at.parse::<u128>().unwrap_or_default() <= now_plus_seconds(0) {
                    snapshot.approvals[approval_index].status = ApprovalStatus::Expired;
                    return Ok((
                        snapshot,
                        Event::ApprovalExpired {
                            approval_id: approval.id,
                        },
                        CommandResult::Accepted,
                    ));
                }
                let status = match decision {
                    ApprovalDecision::AllowOnce | ApprovalDecision::AllowForTask => {
                        ApprovalStatus::Approved
                    }
                    ApprovalDecision::DenyOnce => ApprovalStatus::Denied,
                };
                snapshot.approvals[approval_index].status = status;
                if decision == ApprovalDecision::AllowForTask {
                    let effect = task_grant_effect(&approval.operation).ok_or_else(|| {
                        OrchestratorError::Validation(
                            "only workspace writes and non-network checks can be granted for a task".into(),
                        )
                    })?;
                    let task = snapshot
                        .tasks
                        .iter()
                        .find(|task| task.id == approval.task_id)
                        .cloned()
                        .ok_or(OrchestratorError::NotFound)?;
                    if task.assigned_agent != Some(approval.agent_id)
                        || !matches!(task.status, TaskStatus::Running | TaskStatus::Review)
                    {
                        return Err(OrchestratorError::Validation(
                            "a task grant must match the active assigned task".into(),
                        ));
                    }
                    let task_worktree = task.worktree.as_deref().ok_or_else(|| {
                        OrchestratorError::Validation("task has no canonical worktree".into())
                    })?;
                    let canonical_task_worktree =
                        std::fs::canonicalize(task_worktree).map_err(|_| {
                            OrchestratorError::Validation("task worktree is unavailable".into())
                        })?;
                    let canonical_approval_worktree = std::fs::canonicalize(&approval.cwd)
                        .map_err(|_| {
                            OrchestratorError::Validation("approval worktree is unavailable".into())
                        })?;
                    if canonical_task_worktree != canonical_approval_worktree
                        || !canonical_task_worktree.is_dir()
                    {
                        return Err(OrchestratorError::Validation(
                            "approval worktree does not match the task worktree".into(),
                        ));
                    }
                    let grant = TaskGrantView {
                        id: uuid::Uuid::new_v4().to_string(),
                        task_id: approval.task_id,
                        agent_id: approval.agent_id,
                        worktree: canonical_task_worktree.to_string_lossy().into_owned(),
                        effect,
                        expires_at: approval.expires_at.clone(),
                        revoked: false,
                        source_approval_id: Some(approval.id),
                    };
                    snapshot.task_grants.push(grant.clone());
                    return Ok((
                        snapshot,
                        Event::TaskGrantCreated { grant },
                        CommandResult::Accepted,
                    ));
                }
                Ok((
                    snapshot,
                    Event::ApprovalDecided {
                        approval_id,
                        decision,
                    },
                    CommandResult::Accepted,
                ))
            }
            Command::GrantTaskAccess {
                task_id,
                agent_id,
                worktree,
                effect,
                expires_at,
            } => {
                if actor.kind == ActorKind::Agent {
                    return Err(OrchestratorError::Forbidden);
                }
                if !snapshot
                    .agents
                    .iter()
                    .any(|agent| agent.id == agent_id && !agent.archived)
                {
                    return Err(OrchestratorError::NotFound);
                }
                let task = snapshot
                    .tasks
                    .iter()
                    .find(|task| task.id == task_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if task.assigned_agent != Some(agent_id) || worktree.trim().is_empty() {
                    return Err(OrchestratorError::Validation(
                        "a task grant must match the assigned agent and a worktree".into(),
                    ));
                }
                if !matches!(task.status, TaskStatus::Running | TaskStatus::Review) {
                    return Err(OrchestratorError::Validation(
                        "task grants are valid only for active or review tasks".into(),
                    ));
                }
                let task_worktree = task.worktree.as_deref().ok_or_else(|| {
                    OrchestratorError::Validation("task has no canonical worktree".into())
                })?;
                let canonical_task_worktree =
                    std::fs::canonicalize(task_worktree).map_err(|_| {
                        OrchestratorError::Validation("task worktree is unavailable".into())
                    })?;
                let canonical_grant_worktree = std::fs::canonicalize(&worktree).map_err(|_| {
                    OrchestratorError::Validation("grant worktree is unavailable".into())
                })?;
                if canonical_grant_worktree != canonical_task_worktree
                    || !canonical_grant_worktree.is_dir()
                    || expires_at.parse::<u128>().unwrap_or_default() <= now_plus_seconds(0)
                {
                    return Err(OrchestratorError::Validation(
                        "task grant worktree or expiry is invalid".into(),
                    ));
                }
                let grant = TaskGrantView {
                    id: uuid::Uuid::new_v4().to_string(),
                    task_id,
                    agent_id,
                    worktree: canonical_grant_worktree.to_string_lossy().into_owned(),
                    effect,
                    expires_at,
                    revoked: false,
                    source_approval_id: None,
                };
                let grant_id = grant.id.clone();
                snapshot.task_grants.push(grant.clone());
                Ok((
                    snapshot,
                    Event::TaskGrantCreated { grant },
                    CommandResult::Created { id: grant_id },
                ))
            }
            Command::RevokeTaskGrant { grant_id } => {
                let grant = snapshot
                    .task_grants
                    .iter_mut()
                    .find(|grant| grant.id == grant_id)
                    .ok_or(OrchestratorError::NotFound)?;
                grant.revoked = true;
                Ok((
                    snapshot,
                    Event::TaskGrantRevoked { grant_id },
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
            _ => super::misrouted(),
        }
    }
}

fn task_grant_effect(operation: &str) -> Option<TaskGrantEffect> {
    let operation = operation.to_ascii_lowercase();
    if operation.contains("network")
        || operation.contains("credential")
        || operation.contains("sudo")
        || operation.contains("privileged")
        || operation.contains("external")
    {
        return None;
    }
    if operation.contains("check") || operation.contains("test") || operation.contains("build") {
        Some(TaskGrantEffect::Check)
    } else if operation.contains("workspace")
        || operation.contains("write")
        || operation.contains("mcp-tool")
    {
        Some(TaskGrantEffect::WorkspaceWrite)
    } else {
        None
    }
}
