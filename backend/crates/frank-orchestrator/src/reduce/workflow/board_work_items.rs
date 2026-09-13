//! Taskboard and work-item handlers.

use frank_protocol::*;

use super::{append_task_feed, create_work_item, ensure_role, validate_board_name};
use crate::*;

impl Orchestrator {
    pub(super) async fn reduce_create_taskboard(
        &self,
        mut snapshot: Snapshot,
        spec: TaskboardSpec,
        _actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        validate_board_name(&spec.name)?;
        if let Some(project_id) = spec.project_id
            && !snapshot
                .projects
                .iter()
                .any(|project| project.id == project_id && !project.archived)
        {
            return Err(OrchestratorError::NotFound);
        }
        if let Some(role_id) = spec.default_role_id {
            ensure_role(&snapshot, role_id)?;
        }
        if snapshot
            .taskboards
            .iter()
            .any(|board| !board.archived && board.name.eq_ignore_ascii_case(&spec.name))
        {
            return Err(OrchestratorError::Validation(
                "taskboard name must be unique".into(),
            ));
        }
        let id = TaskboardId::new();
        let now = timestamp_now();
        let board = TaskboardView {
            id,
            name: spec.name.trim().to_owned(),
            project_id: spec.project_id,
            workflow_id: spec.workflow_id,
            dispatch_mode: spec.dispatch_mode,
            default_role_id: spec.default_role_id,
            archived: false,
            created_at: now.clone(),
            updated_at: now,
        };
        snapshot.taskboards.push(board.clone());
        Ok((
            snapshot,
            Event::TaskboardUpserted { taskboard: board },
            CommandResult::Created { id: id.to_string() },
        ))
    }

    pub(super) async fn reduce_update_taskboard(
        &self,
        mut snapshot: Snapshot,
        taskboard_id: TaskboardId,
        patch: TaskboardPatch,
        _actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        let index = snapshot
            .taskboards
            .iter()
            .position(|board| board.id == taskboard_id && !board.archived)
            .ok_or(OrchestratorError::NotFound)?;
        let mut board = snapshot.taskboards[index].clone();
        if let Some(name) = patch.name {
            validate_board_name(&name)?;
            if snapshot.taskboards.iter().any(|candidate| {
                candidate.id != taskboard_id
                    && !candidate.archived
                    && candidate.name.eq_ignore_ascii_case(name.trim())
            }) {
                return Err(OrchestratorError::Validation(
                    "taskboard name must be unique".into(),
                ));
            }
            board.name = name.trim().to_owned();
        }
        if let Some(project_id) = patch.project_id {
            if let Some(project_id) = project_id
                && !snapshot
                    .projects
                    .iter()
                    .any(|project| project.id == project_id && !project.archived)
            {
                return Err(OrchestratorError::NotFound);
            }
            board.project_id = project_id;
        }
        if let Some(workflow_id) = patch.workflow_id {
            board.workflow_id = workflow_id;
        }
        if let Some(dispatch_mode) = patch.dispatch_mode {
            board.dispatch_mode = dispatch_mode;
        }
        if let Some(role_id) = patch.default_role_id {
            if let Some(role_id) = role_id {
                ensure_role(&snapshot, role_id)?;
            }
            board.default_role_id = role_id;
        }
        board.updated_at = timestamp_now();
        snapshot.taskboards[index] = board.clone();
        Ok((
            snapshot,
            Event::TaskboardUpserted { taskboard: board },
            CommandResult::Accepted,
        ))
    }

    pub(super) async fn reduce_archive_taskboard(
        &self,
        mut snapshot: Snapshot,
        taskboard_id: TaskboardId,
        _actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        let index = snapshot
            .taskboards
            .iter()
            .position(|board| board.id == taskboard_id && !board.archived)
            .ok_or(OrchestratorError::NotFound)?;
        if snapshot.tasks.iter().any(|task| {
            task.taskboard_id == Some(taskboard_id)
                && !matches!(task.status, TaskStatus::Done | TaskStatus::Cancelled)
        }) {
            return Err(OrchestratorError::Validation(
                "relocate unfinished work before archiving a taskboard".into(),
            ));
        }
        snapshot.taskboards[index].archived = true;
        snapshot.taskboards[index].updated_at = timestamp_now();
        Ok((
            snapshot,
            Event::TaskboardArchived { taskboard_id },
            CommandResult::Accepted,
        ))
    }

    pub(super) async fn reduce_create_work_item(
        &self,
        mut snapshot: Snapshot,
        spec: WorkItemSpec,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        let task = create_work_item(&snapshot, spec, actor).await?;
        let task_id = task.id;
        snapshot.tasks.push(task.clone());
        append_task_feed(
            &mut snapshot,
            task_id,
            actor,
            TaskFeedKind::Created,
            format!("Work item created: {}", task.title),
            Vec::new(),
        );
        Ok((
            snapshot,
            Event::TaskCreated { task },
            CommandResult::Created {
                id: task_id.to_string(),
            },
        ))
    }

    pub(super) async fn reduce_drop_work_item(
        &self,
        mut snapshot: Snapshot,
        task_id: TaskId,
        taskboard_id: TaskboardId,
        role_id: Option<RoleId>,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        let target_board = snapshot
            .taskboards
            .iter()
            .find(|board| board.id == taskboard_id && !board.archived)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        let target_role_id = role_id.or(target_board.default_role_id);
        if let Some(role_id) = target_role_id {
            ensure_role(&snapshot, role_id)?;
        }
        let index = snapshot
            .tasks
            .iter()
            .position(|task| task.id == task_id)
            .ok_or(OrchestratorError::NotFound)?;
        let from_board_id = snapshot.tasks[index].taskboard_id;
        if from_board_id == Some(taskboard_id)
            && target_role_id == snapshot.tasks[index].required_role_id
        {
            return Ok((
                snapshot.clone(),
                Event::WorkItemDropped {
                    task: snapshot.tasks[index].clone(),
                    from_board_id,
                    to_board_id: taskboard_id,
                    role_id: target_role_id,
                },
                CommandResult::Accepted,
            ));
        }
        if matches!(
            snapshot.tasks[index].status,
            TaskStatus::Done | TaskStatus::Cancelled
        ) {
            return Err(OrchestratorError::Validation(
                "terminal work items cannot be dropped".into(),
            ));
        }
        if matches!(
            snapshot.tasks[index].status,
            TaskStatus::Running | TaskStatus::Review
        ) {
            return Err(OrchestratorError::Validation(
                "active work must finish before its card is dropped".into(),
            ));
        }
        let task = &mut snapshot.tasks[index];
        task.taskboard_id = Some(taskboard_id);
        task.required_role_id = target_role_id;
        // A board drop is the ownership boundary. A previous claim is
        // cleared so the target board can offer it to its own role.
        task.assigned_agent = None;
        task.claimed_at = None;
        task.claim_source = None;
        let task = task.clone();
        append_task_feed(
            &mut snapshot,
            task_id,
            actor,
            TaskFeedKind::Released,
            format!("Work item dropped on board {taskboard_id}"),
            Vec::new(),
        );
        Ok((
            snapshot,
            Event::WorkItemDropped {
                task,
                from_board_id,
                to_board_id: taskboard_id,
                role_id: target_role_id,
            },
            CommandResult::Accepted,
        ))
    }

    pub(super) async fn reduce_spawn_child_work_items(
        &self,
        mut snapshot: Snapshot,
        parent_task_id: TaskId,
        children: Vec<WorkItemSpec>,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        let parent_index = snapshot
            .tasks
            .iter()
            .position(|task| task.id == parent_task_id)
            .ok_or(OrchestratorError::NotFound)?;
        if children.is_empty() || children.len() > MAX_CHILD_WORK_ITEMS {
            return Err(OrchestratorError::Validation(format!(
                "fan-out must contain between one and {MAX_CHILD_WORK_ITEMS} children"
            )));
        }
        if snapshot.tasks[parent_index].parent_task_id.is_some() {
            return Err(OrchestratorError::Validation(
                "child work items cannot spawn grandchildren".into(),
            ));
        }
        let parent_mission = snapshot.tasks[parent_index].mission_id;
        let mut created = Vec::with_capacity(children.len());
        for mut spec in children {
            if spec.mission_id.is_none() {
                spec.mission_id = Some(parent_mission);
            }
            if spec.mission_id != Some(parent_mission) {
                return Err(OrchestratorError::Validation(
                    "child work item must stay in the parent mission".into(),
                ));
            }
            if !spec.dependencies.contains(&parent_task_id) {
                spec.dependencies.push(parent_task_id);
            }
            spec.parent_task_id = Some(parent_task_id);
            created.push(create_work_item(&snapshot, spec, actor).await?);
        }
        let child_ids = created.iter().map(|task| task.id).collect::<Vec<_>>();
        snapshot.tasks.extend(created.iter().cloned());
        let parent = &mut snapshot.tasks[parent_index];
        parent.child_task_ids.extend(child_ids.iter().copied());
        parent
            .child_task_ids
            .sort_unstable_by_key(|id| id.to_string());
        parent.child_task_ids.dedup();
        if !matches!(parent.status, TaskStatus::Done | TaskStatus::Cancelled) {
            parent.status = TaskStatus::Blocked;
        }
        let parent = parent.clone();
        append_task_feed(
            &mut snapshot,
            parent_task_id,
            actor,
            TaskFeedKind::DependencyLocked,
            format!("Work item split into {} child cards", child_ids.len()),
            Vec::new(),
        );
        Ok((
            snapshot,
            Event::ChildWorkItemsSpawned {
                parent,
                children: created.clone(),
            },
            CommandResult::WorkItems {
                task_ids: child_ids,
            },
        ))
    }
}
