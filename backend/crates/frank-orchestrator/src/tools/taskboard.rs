//! Taskboard and workflow tool domain.

use frank_protocol::*;
use serde_json::{Value, json};

use super::{ToolExecutionContext, required_string};

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
        "work_item_drop" => {
            #[derive(serde::Deserialize)]
            struct Args {
                taskboard_id: TaskboardId,
                #[serde(default)]
                role_id: Option<RoleId>,
            }
            let args: Args = serde_json::from_value(input.clone())
                .map_err(|error| format!("invalid work item drop: {error}"))?;
            orchestrator
                .command_from_agent(
                    agent_id,
                    Command::DropWorkItem {
                        task_id,
                        taskboard_id: args.taskboard_id,
                        role_id: args.role_id,
                    },
                )
                .await?
        }
        "work_item_complete" => {
            orchestrator
                .command_from_agent(
                    agent_id,
                    Command::SetTaskStatus {
                        task_id,
                        status: TaskStatus::Review,
                    },
                )
                .await?
        }
        "taskboard_read" => json!({
            "tasks": snapshot
                .tasks
                .iter()
                .filter(|candidate| candidate.mission_id == task.mission_id)
                .collect::<Vec<_>>()
        }),
        "work_item_list_board" => json!({
            "tasks": snapshot
                .tasks
                .iter()
                .filter(|candidate| {
                    candidate.mission_id == task.mission_id
                        && candidate.taskboard_id == task.taskboard_id
                })
                .collect::<Vec<_>>()
        }),
        "taskboard_create" => {
            let title = required_string(input, "title")?;
            let objective = input
                .get("objective")
                .and_then(Value::as_str)
                .unwrap_or(title.as_str())
                .to_owned();
            orchestrator
                .command_from_agent(
                    agent_id,
                    Command::CreateTask(TaskSpec {
                        mission_id: task.mission_id,
                        title,
                        objective,
                        dependencies: Vec::new(),
                        required_role_id: None,
                        priority: input.get("priority").and_then(Value::as_i64).unwrap_or(0) as i32,
                        assigned_agent: None,
                        budget: Budget::unlimited(),
                        taskboard_id: task.taskboard_id,
                        workflow_id: task.workflow_id,
                        parent_task_id: Some(task_id),
                        kind: WorkItemKind::Task,
                        rework_limit: DEFAULT_REWORK_LIMIT,
                    }),
                )
                .await?
        }
        "taskboard_update" => {
            let id = TaskId::parse(&required_string(input, "task_id")?)
                .map_err(|error| format!("invalid task_id: {error}"))?;
            if !snapshot
                .tasks
                .iter()
                .any(|candidate| candidate.id == id && candidate.mission_id == task.mission_id)
            {
                return Err("task is outside the session scope".into());
            }
            let mut patch = input.clone();
            if let Some(object) = patch.as_object_mut() {
                object.remove("task_id");
            }
            let update: TaskUpdateInput = serde_json::from_value(patch)
                .map_err(|error| format!("invalid task update: {error}"))?;
            let command = match update.status {
                Some(TaskCompletionIntent::Completed) => Command::SetTaskStatus {
                    task_id: id,
                    status: TaskStatus::Review,
                },
                Some(TaskCompletionIntent::Rework) => Command::RequestTaskRework {
                    task_id: id,
                    reason: "worker requested rework".into(),
                },
                None => Command::UpdateTask {
                    task_id: id,
                    patch: update.patch,
                },
            };
            orchestrator.command_from_agent(agent_id, command).await?
        }
        "taskboard_assign" => {
            let id = TaskId::parse(&required_string(input, "task_id")?)
                .map_err(|error| format!("invalid task_id: {error}"))?;
            let assigned_agent = AgentId::parse(&required_string(input, "agent_id")?)
                .map_err(|error| format!("invalid agent_id: {error}"))?;
            if !snapshot
                .tasks
                .iter()
                .any(|candidate| candidate.id == id && candidate.mission_id == task.mission_id)
            {
                return Err("task is outside the session scope".into());
            }
            orchestrator
                .command_from_agent(
                    agent_id,
                    Command::AssignTask {
                        task_id: id,
                        agent_id: assigned_agent,
                    },
                )
                .await?
        }
        _ => return Ok(None),
    };

    Ok(Some(result))
}
