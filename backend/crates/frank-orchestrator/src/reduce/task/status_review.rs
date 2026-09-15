//! Task status and acceptance/review handlers.

use frank_protocol::*;

use super::{append_task_feed, update_dependency_locks, update_parent_child_locks};
use crate::*;

impl Orchestrator {
    pub(super) async fn reduce_set_task_status(
        &self,
        mut snapshot: Snapshot,
        task_id: TaskId,
        status: TaskStatus,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        let task_index = snapshot
            .tasks
            .iter()
            .position(|task| task.id == task_id)
            .ok_or(OrchestratorError::NotFound)?;
        let current_status = snapshot.tasks[task_index].status;
        if !current_status.can_transition_to(status) {
            return Err(OrchestratorError::InvalidTransition(format!(
                "task cannot transition from {:?} to {:?}",
                current_status, status
            )));
        }
        let assigned_agent = snapshot.tasks[task_index].assigned_agent;
        if current_status == TaskStatus::Review
            && matches!(status, TaskStatus::Running | TaskStatus::Done)
            && actor.kind == ActorKind::Agent
        {
            let reviewer = actor
                .id
                .as_deref()
                .and_then(|id| AgentId::parse(id).ok())
                .ok_or(OrchestratorError::Forbidden)?;
            if snapshot.tasks[task_index].reviewer_agent != Some(reviewer) {
                return Err(OrchestratorError::Forbidden);
            }
        }
        let review_target = if status == TaskStatus::Review {
            let Some(source) = assigned_agent else {
                return Err(OrchestratorError::Validation(
                    "a review task must retain its source staff agent".into(),
                ));
            };
            if actor.kind == ActorKind::Agent
                && actor.id.as_deref().and_then(|id| AgentId::parse(id).ok()) != Some(source)
            {
                return Err(OrchestratorError::Forbidden);
            }
            organization_review_target(&snapshot, source)
        } else {
            None
        };
        if status == TaskStatus::Review
            && snapshot.organization.published.is_some()
            && review_target.is_none()
        {
            return Err(OrchestratorError::Validation(
                "published Organization has no review target for this staff member".into(),
            ));
        }
        if matches!(status, TaskStatus::Ready | TaskStatus::Running)
            && snapshot.tasks[task_index]
                .dependencies
                .iter()
                .any(|dependency| {
                    snapshot
                        .tasks
                        .iter()
                        .find(|candidate| candidate.id == *dependency)
                        .is_none_or(|candidate| candidate.status != TaskStatus::Done)
                })
        {
            return Err(OrchestratorError::Validation(
                "task dependencies must be done before starting this task".into(),
            ));
        }
        if status == TaskStatus::Running {
            if assigned_agent.is_none_or(|agent_id| {
                !snapshot
                    .agents
                    .iter()
                    .any(|agent| agent.id == agent_id && !agent.archived)
            }) {
                return Err(OrchestratorError::Validation(
                    "assign an active agent before running this task".into(),
                ));
            }
            if let Some(agent_id) = assigned_agent
                && !organization_allows_agent(&snapshot, agent_id)
            {
                return Err(OrchestratorError::Validation(
                    "running task agent is not an active Organization staff member".into(),
                ));
            }
            if let Some(role_id) = snapshot.tasks[task_index].required_role_id
                && snapshot
                    .agents
                    .iter()
                    .find(|agent| Some(agent.id) == assigned_agent)
                    .and_then(|agent| agent.role_id)
                    != Some(role_id)
            {
                return Err(OrchestratorError::Validation(
                    "running task must be assigned to a member of its role".into(),
                ));
            }
        }
        if status == TaskStatus::Done
            && snapshot.tasks[task_index]
                .child_task_ids
                .iter()
                .any(|child_id| {
                    snapshot
                        .tasks
                        .iter()
                        .find(|candidate| candidate.id == *child_id)
                        .is_none_or(|child| child.status != TaskStatus::Done)
                })
        {
            return Err(OrchestratorError::Validation(
                "parent work item cannot complete until every child is done".into(),
            ));
        }
        let mut worktree_operation = None;
        if status == TaskStatus::Running {
            let task = snapshot.tasks[task_index].clone();
            let mission = snapshot
                .missions
                .iter()
                .find(|mission| mission.id == task.mission_id)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            if mission.status != MissionStatus::Active {
                return Err(OrchestratorError::Validation(
                    "a task can run only while its mission is active".into(),
                ));
            }
            let project = snapshot
                .projects
                .iter()
                .find(|project| project.id == mission.project_id && !project.archived)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            let workflow = GitWorkflow::new(
                project,
                snapshot
                    .server
                    .allowed_project_roots
                    .iter()
                    .map(PathBuf::from)
                    .collect(),
            )
            .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            // Worktree creation is a filesystem mutation and is
            // intentionally delayed until after this state
            // transition commits. `start_task_session` performs the
            // idempotent create/reuse step before launching a
            // provider, so a crash cannot leave a Running task with
            // no durable intent.
            let mission_plan = workflow.branch_plan(&mission.branch, &workflow.project.base_branch);
            let task_plan = workflow.task_plan_from_base(task.id, &mission.branch);
            let task = &mut snapshot.tasks[task_index];
            task.worktree = Some(task_plan.path.to_string_lossy().into_owned());
            task.branch = Some(task_plan.branch.clone());
            let has_pending_worktree = snapshot.operations.iter().any(|operation| {
                operation.kind == OperationKind::CreateWorktree
                    && worktree_operation_matches_task(operation, task_id)
                    && !matches!(
                        operation.status,
                        OperationStatus::Succeeded
                            | OperationStatus::Cancelled
                            | OperationStatus::Failed
                    )
            });
            if !has_pending_worktree {
                let now = timestamp_now();
                let operation = OperationView {
                    id: OperationId::new(),
                    kind: OperationKind::CreateWorktree,
                    status: OperationStatus::Queued,
                    resource: task_id.to_string(),
                    phase: "queued".into(),
                    attempt: 0,
                    error: None,
                    created_at: now.clone(),
                    updated_at: now,
                };
                let operation_request = CreateWorktreeOperation {
                    project_id: mission.project_id,
                    mission_id: mission.id,
                    mission_branch: mission_plan.branch,
                    mission_base: mission_plan.base,
                    mission_path: mission_plan.path.to_string_lossy().into_owned(),
                    task_id,
                    task_branch: task_plan.branch,
                    task_base: task_plan.base,
                    task_path: task_plan.path.to_string_lossy().into_owned(),
                };
                let resource = serde_json::to_string(&operation_request)
                    .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
                let mut operation = operation;
                operation.resource = resource;
                snapshot.operations.push(operation);
                worktree_operation = Some(());
            }
        }
        let task = &mut snapshot.tasks[task_index];
        task.status = status;
        task.reviewer_agent = if status == TaskStatus::Review {
            review_target
        } else if current_status == TaskStatus::Review {
            // A rejection returns the source task to Running and
            // clears the completed review work item.
            None
        } else {
            task.reviewer_agent
        };
        if matches!(
            status,
            TaskStatus::Review | TaskStatus::Done | TaskStatus::Cancelled
        ) {
            // Grants are scoped to one worker attempt. Revoke them before the
            // review handoff so a later assignment cannot reuse the old
            // approval.
            for grant in &mut snapshot.task_grants {
                if grant.task_id == task_id {
                    grant.revoked = true;
                }
            }
        }
        let task_value = task.clone();
        let review_event = if status == TaskStatus::Review {
            // Legacy missions predate the Organization review graph. They
            // still use the durable Review lane, but without a synthetic
            // reviewer work item; only published graphs require a concrete
            // review relation.
            if snapshot.organization.published.is_none() {
                None
            } else {
                let Some(source_agent) = assigned_agent else {
                    return Err(OrchestratorError::Validation(
                        "a review task must retain its source staff agent".into(),
                    ));
                };
                let Some(reviewer_agent) = review_target else {
                    return Err(OrchestratorError::Validation(
                        "a published review task must have a reviewer".into(),
                    ));
                };
                match organization_review_contract(&snapshot, source_agent, reviewer_agent) {
                    Some((relation_id, contract)) => {
                        let now = timestamp_now();
                        let review = ReviewWorkItemView {
                            id: ReviewWorkItemId::new(),
                            mission_id: task_value.mission_id,
                            source_task_id: task_id,
                            source_agent,
                            reviewer_agent,
                            relation_id,
                            contract,
                            status: ReviewWorkItemStatus::Pending,
                            decision_reason: None,
                            created_at: now.clone(),
                            updated_at: now,
                        };
                        snapshot.review_items.push(review.clone());
                        Some(review)
                    }
                    None => {
                        return Err(OrchestratorError::Validation(
                            "published review relation is missing its contract".into(),
                        ));
                    }
                }
            }
        } else if current_status == TaskStatus::Review {
            let next_status = match status {
                TaskStatus::Done => ReviewWorkItemStatus::Approved,
                TaskStatus::Running => ReviewWorkItemStatus::Rejected,
                TaskStatus::Cancelled => ReviewWorkItemStatus::Cancelled,
                TaskStatus::Blocked => ReviewWorkItemStatus::Rejected,
                _ => ReviewWorkItemStatus::Cancelled,
            };
            snapshot
                .review_items
                .iter_mut()
                .find(|review| {
                    review.source_task_id == task_id
                        && review.status == ReviewWorkItemStatus::Pending
                })
                .map(|review| {
                    review.status = next_status;
                    review.updated_at = timestamp_now();
                    review.clone()
                })
        } else {
            None
        };
        append_task_feed(
            &mut snapshot,
            task_id,
            actor,
            TaskFeedKind::StatusChanged,
            format!("Status changed to {:?}", status),
            Vec::new(),
        );
        update_dependency_locks(&mut snapshot, task_id, actor, status);
        if status == TaskStatus::Done {
            update_parent_child_locks(&mut snapshot, task_id, actor);
        }
        if worktree_operation.is_some() {
            let operation = snapshot
                .operations
                .iter()
                .find(|operation| {
                    operation.kind == OperationKind::CreateWorktree
                        && worktree_operation_matches_task(operation, task_id)
                        && operation.status == OperationStatus::Queued
                })
                .cloned()
                .ok_or_else(|| {
                    OrchestratorError::Validation(
                        "worktree operation disappeared before commit".into(),
                    )
                })?;
            Ok((
                snapshot,
                Event::TaskWorktreeProvisioning {
                    task: task_value,
                    operation: operation.clone(),
                },
                CommandResult::Operation(operation),
            ))
        } else {
            let event = match review_event {
                Some(review) if status == TaskStatus::Review => Event::ReviewWorkItemOpened {
                    task: task_value,
                    review,
                },
                Some(review) if current_status == TaskStatus::Review => {
                    Event::ReviewWorkItemDecided {
                        task: task_value,
                        review,
                    }
                }
                _ => Event::TaskStatusChanged { task_id, status },
            };
            Ok((snapshot, event, CommandResult::Accepted))
        }
    }

    pub(super) async fn reduce_task_accept(
        &self,
        mut snapshot: Snapshot,
        task_id: TaskId,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        let task = snapshot
            .tasks
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        if task.status != TaskStatus::Review {
            return Err(OrchestratorError::InvalidTransition(
                "only a task in review can be accepted".into(),
            ));
        }
        if task.child_task_ids.iter().any(|child_id| {
            snapshot
                .tasks
                .iter()
                .find(|candidate| candidate.id == *child_id)
                .is_none_or(|child| child.status != TaskStatus::Done)
        }) {
            return Err(OrchestratorError::Validation(
                "parent work item cannot be accepted until every child is done".into(),
            ));
        }
        if actor.kind == ActorKind::Agent {
            let reviewer = actor
                .id
                .as_deref()
                .and_then(|id| AgentId::parse(id).ok())
                .ok_or(OrchestratorError::Forbidden)?;
            let source = task.assigned_agent.ok_or_else(|| {
                OrchestratorError::Validation(
                    "a review task must retain its source staff agent".into(),
                )
            })?;
            if snapshot.organization.published.is_some()
                && (task.reviewer_agent != Some(reviewer)
                    || !organization_allows_review(&snapshot, source, reviewer))
            {
                return Err(OrchestratorError::Forbidden);
            }
        }
        // A task without a worktree is a manual/fake-provider card
        // and can be accepted immediately. A real worker task is
        // represented by a durable Git operation; the operation
        // worker performs checks, captures the diff, commits, and
        // squash-merges after this transaction has committed.
        if let Some(worktree) = task.worktree.as_deref() {
            if !is_allowed_path(worktree, &snapshot.server.allowed_project_roots)
                || !Path::new(worktree).is_dir()
            {
                return Err(OrchestratorError::Validation(
                    "task worktree is outside the allowed project roots".into(),
                ));
            }
            if snapshot.operations.iter().any(|operation| {
                operation.kind == OperationKind::CommitTask
                    && operation.resource == task_id.to_string()
                    && matches!(
                        operation.status,
                        OperationStatus::Queued
                            | OperationStatus::Running
                            | OperationStatus::Waiting
                            | OperationStatus::Recovering
                    )
            }) {
                return Err(OrchestratorError::InvalidTransition(
                    "task acceptance is already queued".into(),
                ));
            }
            let now = timestamp_now();
            let operation = OperationView {
                id: OperationId::new(),
                kind: OperationKind::CommitTask,
                status: OperationStatus::Queued,
                resource: task_id.to_string(),
                phase: "validate".into(),
                attempt: 0,
                error: None,
                created_at: now.clone(),
                updated_at: now,
            };
            snapshot.operations.push(operation.clone());
            if let Some(review) = snapshot.review_items.iter_mut().find(|review| {
                review.source_task_id == task_id && review.status == ReviewWorkItemStatus::Pending
            }) {
                review.status = ReviewWorkItemStatus::Approved;
                review.updated_at = timestamp_now();
            }
            return Ok((
                snapshot,
                Event::OperationChanged {
                    operation: operation.clone(),
                },
                CommandResult::Operation(operation),
            ));
        }
        let task_value = {
            let task = snapshot
                .tasks
                .iter_mut()
                .find(|task| task.id == task_id)
                .expect("task checked");
            task.status = TaskStatus::Done;
            task.clone()
        };
        append_task_feed(
            &mut snapshot,
            task_id,
            actor,
            TaskFeedKind::StatusChanged,
            "Task accepted and marked done".into(),
            Vec::new(),
        );
        update_dependency_locks(&mut snapshot, task_id, actor, TaskStatus::Done);
        update_parent_child_locks(&mut snapshot, task_id, actor);
        if let Some(review) = snapshot.review_items.iter_mut().find(|review| {
            review.source_task_id == task_id && review.status == ReviewWorkItemStatus::Pending
        }) {
            review.status = ReviewWorkItemStatus::Approved;
            review.updated_at = timestamp_now();
        }
        let event = snapshot
            .review_items
            .iter()
            .find(|review| {
                review.source_task_id == task_id && review.status == ReviewWorkItemStatus::Approved
            })
            .cloned()
            .map(|review| Event::ReviewWorkItemDecided {
                task: task_value,
                review,
            })
            .unwrap_or(Event::TaskStatusChanged {
                task_id,
                status: TaskStatus::Done,
            });
        Ok((snapshot, event, CommandResult::Accepted))
    }

    pub(super) async fn reduce_decide_review(
        &self,
        snapshot: Snapshot,
        review_item_id: ReviewWorkItemId,
        decision: ReviewDecision,
        reason: Option<String>,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        if let Some(reason) = reason.as_deref()
            && !valid_content_text(reason, self.max_message_bytes)
        {
            return Err(OrchestratorError::Validation(
                "review decision reason is empty or exceeds the message limit".into(),
            ));
        }
        let review = snapshot
            .review_items
            .iter()
            .find(|review| review.id == review_item_id)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        if review.status != ReviewWorkItemStatus::Pending {
            return Err(OrchestratorError::InvalidTransition(
                "review work item has already been decided".into(),
            ));
        }
        if actor.kind == ActorKind::Agent {
            let reviewer = actor
                .id
                .as_deref()
                .and_then(|id| AgentId::parse(id).ok())
                .ok_or(OrchestratorError::Forbidden)?;
            if reviewer != review.reviewer_agent
                || !organization_allows_review(
                    &snapshot,
                    review.source_agent,
                    review.reviewer_agent,
                )
            {
                return Err(OrchestratorError::Forbidden);
            }
        }
        let next_command = match decision {
            ReviewDecision::Approve => Command::TaskAccept {
                task_id: review.source_task_id,
            },
            ReviewDecision::Reject => Command::SetTaskStatus {
                task_id: review.source_task_id,
                status: TaskStatus::Running,
            },
        };
        let (mut next, mut event, result) =
            Box::pin(self.reduce_task(snapshot, next_command, actor)).await?;
        if let Some(item) = next
            .review_items
            .iter_mut()
            .find(|item| item.id == review_item_id)
        {
            item.decision_reason = reason
                .filter(|reason| !reason.trim().is_empty())
                .map(|reason| reason.trim().to_owned());
            item.updated_at = timestamp_now();
            // A worktree acceptance returns an operation event. The
            // decision is still persisted in the snapshot, while a
            // lightweight status event is emitted for manual tasks.
            if matches!(event, Event::TaskStatusChanged { .. }) {
                let task = next
                    .tasks
                    .iter()
                    .find(|task| task.id == review.source_task_id)
                    .cloned()
                    .ok_or(OrchestratorError::NotFound)?;
                event = Event::ReviewWorkItemDecided {
                    task,
                    review: item.clone(),
                };
            }
        }
        Ok((next, event, result))
    }
}
