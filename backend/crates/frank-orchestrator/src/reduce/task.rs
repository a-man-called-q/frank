//! Task creation, status transitions, assignment and acceptance.

use frank_protocol::*;

use crate::*;

impl Orchestrator {
    pub(crate) async fn reduce_task(
        &self,
        mut snapshot: Snapshot,
        command: Command,
        _actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        match command {
            Command::CreateTask(spec) => {
                let mission = snapshot
                    .missions
                    .iter()
                    .find(|mission| mission.id == spec.mission_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if matches!(
                    mission.status,
                    MissionStatus::Completed | MissionStatus::Failed | MissionStatus::Cancelled
                ) {
                    return Err(OrchestratorError::Validation(
                        "cannot add a task to a final mission".into(),
                    ));
                }
                if let Some(agent_id) = spec.assigned_agent
                    && !snapshot
                        .agents
                        .iter()
                        .any(|agent| agent.id == agent_id && !agent.archived)
                {
                    return Err(OrchestratorError::NotFound);
                }
                validate_task_spec(&spec, &snapshot.tasks)?;
                let id = TaskId::new();
                let task = TaskView {
                    id,
                    mission_id: spec.mission_id,
                    title: spec.title,
                    objective: spec.objective,
                    dependencies: spec.dependencies,
                    priority: spec.priority,
                    budget: spec.budget,
                    status: TaskStatus::Backlog,
                    assigned_agent: spec.assigned_agent,
                    attempt: 0,
                    max_attempts: DEFAULT_MAX_ATTEMPTS,
                    worktree: None,
                    branch: Some(format!("frank/task-{id}")),
                    result_artifact: None,
                };
                snapshot.tasks.push(task.clone());
                Ok((
                    snapshot,
                    Event::TaskCreated { task },
                    CommandResult::Created { id: id.to_string() },
                ))
            }
            Command::UpdateTask { task_id, patch } => {
                let mut candidate = snapshot
                    .tasks
                    .iter()
                    .find(|task| task.id == task_id)
                    .cloned()
                    .ok_or(OrchestratorError::NotFound)?;
                apply_task_patch(&mut candidate, patch);
                if let Some(agent_id) = candidate.assigned_agent
                    && !snapshot
                        .agents
                        .iter()
                        .any(|agent| agent.id == agent_id && !agent.archived)
                {
                    return Err(OrchestratorError::NotFound);
                }
                validate_task_view(&candidate, &snapshot.tasks)?;
                let task = snapshot
                    .tasks
                    .iter_mut()
                    .find(|task| task.id == task_id)
                    .expect("task checked");
                *task = candidate.clone();
                Ok((
                    snapshot,
                    Event::TaskUpdated { task: candidate },
                    CommandResult::Accepted,
                ))
            }
            Command::SetTaskStatus { task_id, status } => {
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
                    let assigned_agent = snapshot.tasks[task_index].assigned_agent;
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
                    let mission_plan =
                        workflow.branch_plan(&mission.branch, &workflow.project.base_branch);
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
                let task_value = task.clone();
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
                    Ok((
                        snapshot,
                        Event::TaskStatusChanged { task_id, status },
                        CommandResult::Accepted,
                    ))
                }
            }
            Command::AssignTask { task_id, agent_id } => {
                let agent = snapshot
                    .agents
                    .iter()
                    .find(|agent| agent.id == agent_id && !agent.archived)
                    .ok_or(OrchestratorError::NotFound)?;
                if matches!(agent.status, AgentStatus::Failed | AgentStatus::Stopping) {
                    return Err(OrchestratorError::Validation(
                        "cannot assign work to a failed or stopping agent".into(),
                    ));
                }
                let task = snapshot
                    .tasks
                    .iter_mut()
                    .find(|task| task.id == task_id)
                    .ok_or(OrchestratorError::NotFound)?;
                let mission = snapshot
                    .missions
                    .iter()
                    .find(|mission| mission.id == task.mission_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if matches!(
                    mission.status,
                    MissionStatus::Completed | MissionStatus::Failed | MissionStatus::Cancelled
                ) {
                    return Err(OrchestratorError::Validation(
                        "cannot assign work on a final mission".into(),
                    ));
                }
                task.assigned_agent = Some(agent_id);
                Ok((
                    snapshot,
                    Event::TaskAssigned { task_id, agent_id },
                    CommandResult::Accepted,
                ))
            }
            Command::TaskAccept { task_id } => {
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
                    return Ok((
                        snapshot,
                        Event::OperationChanged {
                            operation: operation.clone(),
                        },
                        CommandResult::Operation(operation),
                    ));
                }
                let task = snapshot
                    .tasks
                    .iter_mut()
                    .find(|task| task.id == task_id)
                    .expect("task checked");
                task.status = TaskStatus::Done;
                Ok((
                    snapshot,
                    Event::TaskStatusChanged {
                        task_id,
                        status: TaskStatus::Done,
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
