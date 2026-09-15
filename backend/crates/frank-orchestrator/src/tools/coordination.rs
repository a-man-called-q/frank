//! Coordination tools: tasks, messages, memory, approvals, and reviews.

use frank_protocol::*;
use serde::Deserialize;
use serde_json::Value;

use super::{ToolExecutionContext, required_string};

#[derive(Debug, Deserialize)]
struct ChildTaskArgs {
    title: String,
    objective: String,
    #[serde(default)]
    dependencies: Vec<TaskId>,
    #[serde(default)]
    required_role_id: Option<RoleId>,
    #[serde(default)]
    priority: i32,
    #[serde(default)]
    budget: Option<Budget>,
    #[serde(default)]
    taskboard_id: Option<TaskboardId>,
}

#[derive(Debug, Deserialize)]
struct SpawnChildrenArgs {
    children: Vec<ChildTaskArgs>,
}

#[derive(Debug, Deserialize)]
struct HumanInputArgs {
    kind: HumanInputKind,
    prompt: String,
}

#[derive(Debug, Deserialize)]
struct MessageArgs {
    body: String,
    #[serde(default)]
    recipient: Option<ActorRef>,
    #[serde(default)]
    act: Option<MessageAct>,
    #[serde(default)]
    artifact_ids: Vec<ArtifactId>,
    #[serde(default)]
    reply_to: Option<MessageId>,
}

#[derive(Debug, Deserialize)]
struct ArtifactArgs {
    name: String,
    #[serde(default = "default_artifact_mime")]
    mime_type: String,
    #[serde(default)]
    bytes: Vec<u8>,
}

fn default_artifact_mime() -> String {
    "application/octet-stream".into()
}

pub(crate) async fn dispatch(
    context: &ToolExecutionContext<'_>,
    name: &str,
    input: &Value,
) -> Result<Option<Value>, String> {
    let orchestrator = context.orchestrator;
    let task = context.task;
    let agent_id = context.agent_id;
    let task_id = context.task_id;
    let snapshot = context.snapshot;

    let result = match name {
        "task_get" | "work_item_get" => {
            serde_json::to_value(task).map_err(|error| error.to_string())?
        }
        "task_update" => {
            let update: TaskUpdateInput = serde_json::from_value(input.clone())
                .map_err(|error| format!("invalid task update: {error}"))?;
            let command = match update.status {
                Some(TaskCompletionIntent::Completed) => Command::SetTaskStatus {
                    task_id,
                    status: TaskStatus::Review,
                },
                Some(TaskCompletionIntent::Rework) => Command::RequestTaskRework {
                    task_id,
                    reason: "worker requested rework".into(),
                },
                None => Command::UpdateTask {
                    task_id,
                    patch: update.patch,
                },
            };
            orchestrator.command_from_agent(agent_id, command).await?
        }
        "task_create_child" => {
            let args: ChildTaskArgs = serde_json::from_value(input.clone())
                .map_err(|error| format!("invalid child task: {error}"))?;
            if args.title.trim().is_empty() || args.objective.trim().is_empty() {
                return Err("child task title and objective are required".into());
            }
            let dependencies = args.dependencies;
            orchestrator
                .command_from_agent(
                    agent_id,
                    Command::CreateTask(TaskSpec {
                        mission_id: task.mission_id,
                        title: args.title,
                        objective: args.objective,
                        dependencies,
                        required_role_id: args.required_role_id,
                        priority: args.priority,
                        assigned_agent: None,
                        budget: args.budget.unwrap_or_else(Budget::unlimited),
                        taskboard_id: task.taskboard_id,
                        workflow_id: task.workflow_id,
                        parent_task_id: Some(task_id),
                        kind: WorkItemKind::Task,
                        rework_limit: DEFAULT_REWORK_LIMIT,
                    }),
                )
                .await?
        }
        "work_item_spawn_children" => {
            let args: SpawnChildrenArgs = serde_json::from_value(input.clone())
                .map_err(|error| format!("invalid child work items: {error}"))?;
            if args.children.is_empty() {
                return Err("at least one child work item is required".into());
            }
            let default_board = task
                .taskboard_id
                .ok_or_else(|| "current work item is not on a taskboard".to_string())?;
            let children = args
                .children
                .into_iter()
                .map(|child| WorkItemSpec {
                    mission_id: Some(task.mission_id),
                    title: child.title,
                    objective: child.objective,
                    kind: WorkItemKind::Task,
                    taskboard_id: child.taskboard_id.unwrap_or(default_board),
                    workflow_id: task.workflow_id,
                    parent_task_id: Some(task_id),
                    dependencies: child.dependencies,
                    required_role_id: child.required_role_id,
                    priority: child.priority,
                    budget: child.budget.unwrap_or_else(Budget::unlimited),
                    rework_limit: DEFAULT_REWORK_LIMIT,
                })
                .collect::<Vec<_>>();
            orchestrator
                .command_from_agent(
                    agent_id,
                    Command::SpawnChildWorkItems {
                        parent_task_id: task_id,
                        children,
                    },
                )
                .await?
        }
        "work_item_request_human_input" => {
            let args: HumanInputArgs = serde_json::from_value(input.clone())
                .map_err(|error| format!("invalid human input request: {error}"))?;
            orchestrator
                .command_from_agent(
                    agent_id,
                    Command::RequestHumanInput {
                        task_id,
                        kind: args.kind,
                        prompt: args.prompt,
                    },
                )
                .await?
        }
        "work_item_rework" => {
            let reason = required_string(input, "reason")?;
            orchestrator
                .command_from_agent(agent_id, Command::RequestTaskRework { task_id, reason })
                .await?
        }
        "work_item_offer_respond" => {
            let offer_id = WorkOfferId::parse(&required_string(input, "offer_id")?)
                .map_err(|error| format!("invalid offer_id: {error}"))?;
            let accept = input
                .get("accept")
                .and_then(Value::as_bool)
                .ok_or_else(|| "accept is required".to_string())?;
            orchestrator
                .command_from_agent(agent_id, Command::RespondWorkOffer { offer_id, accept })
                .await?
        }
        "message_send" => {
            let args: MessageArgs = serde_json::from_value(input.clone())
                .map_err(|error| format!("invalid message: {error}"))?;
            if args.body.trim().is_empty() {
                return Err("message body is required".into());
            }
            orchestrator
                .command_from_agent(
                    agent_id,
                    Command::SendMessage(MessageSpec {
                        message_id: None,
                        mission_id: task.mission_id,
                        task_id: Some(task_id),
                        recipient: args.recipient.unwrap_or_else(ActorRef::supervisor),
                        act: args.act.unwrap_or(MessageAct::Inform),
                        body: args.body,
                        artifact_ids: args.artifact_ids,
                        reply_to: args.reply_to,
                        hop: 1,
                    }),
                )
                .await?
        }
        "message_ack" => {
            let message_id = MessageId::parse(&required_string(input, "message_id")?)
                .map_err(|error| format!("invalid message_id: {error}"))?;
            if !snapshot
                .messages
                .iter()
                .any(|message| message.id == message_id && message.task_id == Some(task_id))
            {
                return Err("message is outside the session scope".into());
            }
            orchestrator
                .command_from_agent(agent_id, Command::AckMessage { message_id })
                .await?
        }
        "artifact_publish" => {
            let args: ArtifactArgs = serde_json::from_value(input.clone())
                .map_err(|error| format!("invalid artifact: {error}"))?;
            orchestrator
                .command_from_agent(
                    agent_id,
                    Command::PublishArtifact(ArtifactSpec {
                        mission_id: task.mission_id,
                        task_id: Some(task_id),
                        name: args.name,
                        mime_type: args.mime_type,
                        bytes: args.bytes,
                    }),
                )
                .await?
        }
        "memory_read" => {
            let path = input
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("memory.md")
                .to_owned();
            orchestrator
                .command_from_agent(agent_id, Command::ReadMemory { agent_id, path })
                .await?
        }
        "memory_propose" => {
            let path = required_string(input, "path")?;
            let content = required_string(input, "content")?;
            orchestrator
                .command_from_agent(
                    agent_id,
                    Command::ProposeMemory {
                        agent_id,
                        path,
                        content,
                    },
                )
                .await?
        }
        "approval_status" => serde_json::to_value(
            snapshot
                .approvals
                .iter()
                .filter(|approval| approval.task_id == task_id)
                .cloned()
                .collect::<Vec<_>>(),
        )
        .map_err(|error| error.to_string())?,
        "review_decide" => {
            let review_item_id =
                ReviewWorkItemId::parse(&required_string(input, "review_item_id")?)
                    .map_err(|error| format!("invalid review_item_id: {error}"))?;
            let decision: ReviewDecision = serde_json::from_value(
                input
                    .get("decision")
                    .cloned()
                    .ok_or_else(|| "decision is required".to_string())?,
            )
            .map_err(|error| format!("invalid review decision: {error}"))?;
            let reason = input
                .get("reason")
                .and_then(Value::as_str)
                .map(str::to_owned);
            let review = snapshot
                .review_items
                .iter()
                .find(|review| review.id == review_item_id)
                .ok_or_else(|| "review item is outside the session scope".to_string())?;
            if review.reviewer_agent != agent_id
                || review.source_task_id != task_id
                || review.status != ReviewWorkItemStatus::Pending
            {
                return Err("review item is outside the session scope".into());
            }
            orchestrator
                .command_from_agent(
                    agent_id,
                    Command::DecideReview {
                        review_item_id,
                        decision,
                        reason,
                    },
                )
                .await?
        }
        _ => return Ok(None),
    };

    Ok(Some(result))
}
