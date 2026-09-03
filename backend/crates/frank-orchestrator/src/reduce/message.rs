//! The agent mailbox commands.

use frank_protocol::*;

use crate::*;

impl Orchestrator {
    pub(crate) async fn reduce_message(
        &self,
        mut snapshot: Snapshot,
        command: Command,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        match command {
            Command::SendMessage(spec) => {
                if spec.body.len() > self.max_message_bytes {
                    return Err(OrchestratorError::Validation(
                        "message body exceeds 64 KiB; publish an artifact instead".into(),
                    ));
                }
                let mission = snapshot
                    .missions
                    .iter()
                    .find(|mission| mission.id == spec.mission_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if let Some(task_id) = spec.task_id
                    && !snapshot
                        .tasks
                        .iter()
                        .any(|task| task.id == task_id && task.mission_id == mission.id)
                {
                    return Err(OrchestratorError::NotFound);
                }
                if spec.artifact_ids.len() > 256
                    || spec.artifact_ids.iter().any(|artifact_id| {
                        !snapshot.artifacts.iter().any(|artifact| {
                            artifact.id == *artifact_id
                                && artifact.mission_id == mission.id
                                && (spec.task_id.is_none() || artifact.task_id == spec.task_id)
                        })
                    })
                {
                    return Err(OrchestratorError::NotFound);
                }
                if actor.kind == ActorKind::Agent {
                    let actor_id = actor.id.as_deref().unwrap_or_default();
                    let Some(task_id) = spec.task_id else {
                        return Err(OrchestratorError::Forbidden);
                    };
                    let assigned = snapshot
                        .tasks
                        .iter()
                        .find(|task| task.id == task_id)
                        .and_then(|task| task.assigned_agent)
                        .is_some_and(|assigned| assigned.to_string() == actor_id);
                    if !assigned {
                        return Err(OrchestratorError::Forbidden);
                    }
                    // Provider sessions may talk to the supervisor or to a
                    // worker participating in the same mission. They never
                    // get a general device/system messaging primitive through
                    // the broker, even when their task capability is valid.
                    let recipient_allowed = match spec.recipient.kind {
                        ActorKind::Supervisor => true,
                        ActorKind::Agent => spec
                            .recipient
                            .id
                            .as_deref()
                            .and_then(|id| AgentId::parse(id).ok())
                            .is_some_and(|recipient_id| {
                                snapshot.agents.iter().any(|agent| {
                                    agent.id == recipient_id
                                        && !agent.archived
                                        && snapshot.tasks.iter().any(|candidate| {
                                            candidate.mission_id == mission.id
                                                && candidate.assigned_agent == Some(recipient_id)
                                        })
                                })
                            }),
                        ActorKind::Device | ActorKind::System => false,
                    };
                    if !recipient_allowed {
                        return Err(OrchestratorError::Forbidden);
                    }
                }
                if spec.body.trim().is_empty()
                    || spec
                        .recipient
                        .id
                        .as_deref()
                        .is_some_and(|id| id.len() > 128)
                    || spec.hop > DEFAULT_MESSAGE_HOP_LIMIT
                {
                    return Err(OrchestratorError::Validation(
                        "message recipient, body, or hop is invalid".into(),
                    ));
                }
                if actor.kind == ActorKind::Device && spec.recipient.kind != ActorKind::Supervisor {
                    return Err(OrchestratorError::Forbidden);
                }
                let id = spec.message_id.unwrap_or_else(MessageId::new);
                // A provider bridge may retry after losing its HTTP response.
                // Treat the durable message id as a state-level no-op instead
                // of creating a second delivery. The commit still records the
                // command id normally, while the projection remains exactly
                // unchanged because this event carries the existing message.
                if let Some(existing) = snapshot
                    .messages
                    .iter()
                    .find(|message| message.id == id)
                    .cloned()
                {
                    return Ok((
                        snapshot,
                        Event::MessageQueued { message: existing },
                        CommandResult::Created { id: id.to_string() },
                    ));
                }
                if !self.mailbox.lock().await.accept(id, spec.hop) {
                    return Err(OrchestratorError::Validation(
                        "message hop limit exceeded or duplicate message".into(),
                    ));
                }
                let message = MessageView {
                    id,
                    mission_id: spec.mission_id,
                    task_id: spec.task_id,
                    sender: actor.clone(),
                    recipient: spec.recipient,
                    act: spec.act,
                    body: spec.body,
                    artifact_ids: spec.artifact_ids,
                    reply_to: spec.reply_to,
                    hop: spec.hop,
                    delivery: DeliveryStatus::Queued,
                    created_at: timestamp_now(),
                };
                snapshot.messages.push(message.clone());
                Ok((
                    snapshot,
                    Event::MessageQueued { message },
                    CommandResult::Created { id: id.to_string() },
                ))
            }
            Command::AckMessage { message_id } => {
                let message = snapshot
                    .messages
                    .iter_mut()
                    .find(|message| message.id == message_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if actor.kind == ActorKind::Agent
                    && actor.id.as_deref() != message.recipient.id.as_deref()
                {
                    return Err(OrchestratorError::Forbidden);
                }
                if message.delivery != DeliveryStatus::Delivered {
                    return Err(OrchestratorError::InvalidTransition(
                        "only a delivered message can be acknowledged".into(),
                    ));
                }
                message.delivery = DeliveryStatus::Acknowledged;
                Ok((
                    snapshot,
                    Event::MessageAcknowledged { message_id },
                    CommandResult::Accepted,
                ))
            }
            Command::CompleteMessage {
                message_id,
                success,
            } => {
                let message = snapshot
                    .messages
                    .iter_mut()
                    .find(|message| message.id == message_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if actor.kind == ActorKind::Agent
                    && actor.id.as_deref() != message.recipient.id.as_deref()
                {
                    return Err(OrchestratorError::Forbidden);
                }
                if !matches!(
                    message.delivery,
                    DeliveryStatus::Delivered | DeliveryStatus::Acknowledged
                ) {
                    return Err(OrchestratorError::InvalidTransition(
                        "message is not awaiting completion".into(),
                    ));
                }
                message.delivery = if success {
                    DeliveryStatus::Completed
                } else {
                    DeliveryStatus::Failed
                };
                Ok((
                    snapshot,
                    if success {
                        Event::MessageCompleted { message_id }
                    } else {
                        Event::MessageFailed { message_id }
                    },
                    CommandResult::Accepted,
                ))
            }
            _ => super::misrouted(),
        }
    }
}
