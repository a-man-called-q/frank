//! Work offer and human-input handlers.

use frank_protocol::*;

use super::{append_task_feed, ensure_eligible_agent};
use crate::*;

impl Orchestrator {
    pub(super) async fn reduce_create_work_offer(
        &self,
        mut snapshot: Snapshot,
        task_id: TaskId,
        agent_id: AgentId,
        _actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        if snapshot.organization_runtime.status == OrganizationDrainStatus::Draining {
            return Err(OrchestratorError::Validation(
                "Organization is draining; no new work may be offered".into(),
            ));
        }
        let task = snapshot
            .tasks
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        let board_id = task.taskboard_id.ok_or_else(|| {
            OrchestratorError::Validation("work item is not on a taskboard".into())
        })?;
        let board = snapshot
            .taskboards
            .iter()
            .find(|board| board.id == board_id && !board.archived)
            .ok_or(OrchestratorError::NotFound)?;
        if board.dispatch_mode != TaskboardDispatchMode::Pull {
            return Err(OrchestratorError::Validation(
                "work offers are only valid on a pull-mode board".into(),
            ));
        }
        if !matches!(task.status, TaskStatus::Backlog | TaskStatus::Ready) {
            return Err(OrchestratorError::Validation(
                "only backlog or ready work items can receive an offer".into(),
            ));
        }
        ensure_eligible_agent(&snapshot, &task, agent_id)?;
        if snapshot
            .work_offers
            .iter()
            .any(|offer| offer.task_id == task_id && offer.status.is_open())
        {
            return Err(OrchestratorError::Validation(
                "work item already has an open offer".into(),
            ));
        }
        let now = timestamp_now();
        let expires_at = now
            .parse::<u64>()
            .unwrap_or_default()
            .saturating_add(300_000)
            .to_string();
        let offer = WorkOfferView {
            id: WorkOfferId::new(),
            task_id,
            taskboard_id: board_id,
            agent_id,
            role_id: task.required_role_id,
            status: WorkOfferStatus::Pending,
            attempt: 1,
            created_at: now,
            expires_at,
            responded_at: None,
        };
        snapshot.work_offers.push(offer.clone());
        Ok((
            snapshot,
            Event::WorkOfferCreated {
                offer: offer.clone(),
            },
            CommandResult::WorkOffer(offer),
        ))
    }

    pub(super) async fn reduce_respond_work_offer(
        &self,
        mut snapshot: Snapshot,
        offer_id: WorkOfferId,
        accept: bool,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        let index = snapshot
            .work_offers
            .iter()
            .position(|offer| offer.id == offer_id && offer.status.is_open())
            .ok_or(OrchestratorError::NotFound)?;
        let mut offer = snapshot.work_offers[index].clone();
        if actor.kind == ActorKind::Agent {
            let actor_id = actor
                .id
                .as_deref()
                .and_then(|value| AgentId::parse(value).ok())
                .ok_or(OrchestratorError::Forbidden)?;
            if actor_id != offer.agent_id {
                return Err(OrchestratorError::Forbidden);
            }
        }
        let now = timestamp_now();
        if offer.expires_at.parse::<u64>().unwrap_or_default()
            <= now.parse::<u64>().unwrap_or_default()
        {
            offer.status = WorkOfferStatus::Expired;
        } else if accept {
            let task = snapshot
                .tasks
                .iter()
                .find(|task| task.id == offer.task_id)
                .ok_or(OrchestratorError::NotFound)?;
            if !matches!(task.status, TaskStatus::Backlog | TaskStatus::Ready)
                || task.taskboard_id != Some(offer.taskboard_id)
                || task
                    .assigned_agent
                    .is_some_and(|assigned| assigned != offer.agent_id)
            {
                return Err(OrchestratorError::InvalidTransition(
                    "work offer no longer matches the card state".into(),
                ));
            }
            offer.status = WorkOfferStatus::Accepted;
            let task = snapshot
                .tasks
                .iter_mut()
                .find(|task| task.id == offer.task_id)
                .ok_or(OrchestratorError::NotFound)?;
            task.assigned_agent = Some(offer.agent_id);
            task.claimed_at = Some(now.clone());
            task.claim_source = Some(TaskClaimSource::Manual);
            if task.status == TaskStatus::Backlog {
                task.status = TaskStatus::Ready;
            }
            append_task_feed(
                &mut snapshot,
                offer.task_id,
                actor,
                TaskFeedKind::Claimed,
                "Work offer accepted".into(),
                Vec::new(),
            );
        } else {
            offer.status = WorkOfferStatus::Declined;
            append_task_feed(
                &mut snapshot,
                offer.task_id,
                actor,
                TaskFeedKind::Released,
                "Work offer declined".into(),
                Vec::new(),
            );
        }
        offer.responded_at = Some(now);
        snapshot.work_offers[index] = offer.clone();
        Ok((
            snapshot,
            Event::WorkOfferResponded {
                offer: offer.clone(),
            },
            CommandResult::WorkOffer(offer),
        ))
    }

    pub(super) async fn reduce_request_human_input(
        &self,
        mut snapshot: Snapshot,
        task_id: TaskId,
        kind: HumanInputKind,
        prompt: String,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        if prompt.trim().is_empty() || prompt.len() > MAX_MESSAGE_BODY_BYTES {
            return Err(OrchestratorError::Validation(
                "human-input prompt is empty or too large".into(),
            ));
        }
        let index = snapshot
            .tasks
            .iter()
            .position(|task| task.id == task_id)
            .ok_or(OrchestratorError::NotFound)?;
        if matches!(
            snapshot.tasks[index].status,
            TaskStatus::Done | TaskStatus::Cancelled
        ) {
            return Err(OrchestratorError::Validation(
                "terminal work items cannot request human input".into(),
            ));
        }
        if snapshot
            .human_inputs
            .iter()
            .any(|input| input.task_id == task_id && input.status == HumanInputStatus::Pending)
        {
            return Err(OrchestratorError::Validation(
                "work item already waits for human input".into(),
            ));
        }
        let now = timestamp_now();
        let requested_by = actor.id.as_deref().and_then(|id| AgentId::parse(id).ok());
        let input = HumanInputView {
            id: HumanInputId::new(),
            task_id,
            mission_id: Some(snapshot.tasks[index].mission_id),
            requested_by,
            kind,
            prompt,
            status: HumanInputStatus::Pending,
            answer: None,
            answered_by: None,
            created_at: now.clone(),
            updated_at: now,
        };
        snapshot.human_inputs.push(input.clone());
        snapshot.tasks[index].status = TaskStatus::Blocked;
        let task = snapshot.tasks[index].clone();
        append_task_feed(
            &mut snapshot,
            task_id,
            actor,
            TaskFeedKind::DependencyLocked,
            "Work item is waiting for human input".into(),
            Vec::new(),
        );
        Ok((
            snapshot,
            Event::HumanInputRequested {
                task,
                input: input.clone(),
            },
            CommandResult::HumanInput(input),
        ))
    }

    pub(super) async fn reduce_resolve_human_input(
        &self,
        mut snapshot: Snapshot,
        human_input_id: HumanInputId,
        answer: String,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        if answer.trim().is_empty() || answer.len() > MAX_MESSAGE_BODY_BYTES {
            return Err(OrchestratorError::Validation(
                "human-input answer is empty or too large".into(),
            ));
        }
        let index = snapshot
            .human_inputs
            .iter()
            .position(|input| {
                input.id == human_input_id && input.status == HumanInputStatus::Pending
            })
            .ok_or(OrchestratorError::NotFound)?;
        let mut input = snapshot.human_inputs[index].clone();
        let now = timestamp_now();
        input.status = HumanInputStatus::Answered;
        input.answer = Some(answer);
        input.answered_by = actor.id.clone();
        input.updated_at = now;
        let task_index = snapshot
            .tasks
            .iter()
            .position(|task| task.id == input.task_id)
            .ok_or(OrchestratorError::NotFound)?;
        let assigned_unavailable =
            snapshot.tasks[task_index]
                .assigned_agent
                .is_some_and(|agent_id| {
                    !snapshot
                        .agents
                        .iter()
                        .any(|agent| agent.id == agent_id && !agent.archived)
                });
        if assigned_unavailable {
            // Same-agent resume is preferred; a later reconcile picks
            // a role-compatible fallback when that member departed.
            snapshot.tasks[task_index].assigned_agent = None;
            snapshot.tasks[task_index].claimed_at = None;
            snapshot.tasks[task_index].claim_source = None;
        }
        // A human answer only removes the human-input block. A parent
        // that is also waiting on child cards must remain blocked until
        // the structural fan-out dependency is complete.
        let children_pending = snapshot.tasks[task_index]
            .child_task_ids
            .iter()
            .any(|child_id| {
                snapshot
                    .tasks
                    .iter()
                    .find(|candidate| candidate.id == *child_id)
                    .is_none_or(|child| child.status != TaskStatus::Done)
            });
        snapshot.tasks[task_index].status = if children_pending {
            TaskStatus::Blocked
        } else {
            TaskStatus::Ready
        };
        snapshot.human_inputs[index] = input.clone();
        let task = snapshot.tasks[task_index].clone();
        append_task_feed(
            &mut snapshot,
            task.id,
            actor,
            TaskFeedKind::DependencyUnlocked,
            "Human input answered; work item is ready to resume".into(),
            Vec::new(),
        );
        Ok((
            snapshot,
            Event::HumanInputResolved {
                task,
                input: input.clone(),
            },
            CommandResult::HumanInput(input),
        ))
    }

    pub(super) async fn reduce_request_task_rework(
        &self,
        mut snapshot: Snapshot,
        task_id: TaskId,
        reason: String,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        if reason.trim().is_empty() || reason.len() > MAX_MESSAGE_BODY_BYTES {
            return Err(OrchestratorError::Validation(
                "rework reason is empty or too large".into(),
            ));
        }
        let index = snapshot
            .tasks
            .iter()
            .position(|task| task.id == task_id)
            .ok_or(OrchestratorError::NotFound)?;
        let task = &mut snapshot.tasks[index];
        if matches!(task.status, TaskStatus::Cancelled) {
            return Err(OrchestratorError::InvalidTransition(
                "cancelled work item cannot be reworked".into(),
            ));
        }
        let limit = if task.rework_limit == 0 {
            DEFAULT_REWORK_LIMIT
        } else {
            task.rework_limit
        };
        if task.rework_count >= limit {
            return Err(OrchestratorError::Validation(format!(
                "rework limit of {limit} has been reached"
            )));
        }
        task.rework_count = task.rework_count.saturating_add(1);
        task.status = TaskStatus::Ready;
        task.assigned_agent = None;
        task.claimed_at = None;
        task.claim_source = None;
        let count = task.rework_count;
        let task = task.clone();
        append_task_feed(
            &mut snapshot,
            task_id,
            actor,
            TaskFeedKind::StatusChanged,
            format!("Rework requested ({count}/{limit}): {reason}"),
            Vec::new(),
        );
        Ok((
            snapshot,
            Event::TaskReworkRequested {
                task,
                reason,
                count,
            },
            CommandResult::Accepted,
        ))
    }
}
