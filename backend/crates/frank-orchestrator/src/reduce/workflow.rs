//! Executable Organization v2 workflow records.
//!
//! This reducer deliberately keeps the existing `TaskView`/`TaskId` card
//! contract. A board move, offer, human question, or child fan-out is an
//! event around the same stable card rather than a second provider mailbox.

use frank_protocol::*;

use crate::reduce::append_task_feed;
use crate::{Orchestrator, OrchestratorError, Result, validate_task_view};

mod agent;
mod board_work_items;
mod offers_input;
mod organization_runtime;

impl Orchestrator {
    pub(crate) async fn reduce_workflow(
        &self,
        snapshot: Snapshot,
        command: Command,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        match command {
            Command::CreateTaskboard(spec) => {
                self.reduce_create_taskboard(snapshot, spec, actor).await
            }
            Command::UpdateTaskboard {
                taskboard_id,
                patch,
            } => {
                self.reduce_update_taskboard(snapshot, taskboard_id, patch, actor)
                    .await
            }
            Command::ArchiveTaskboard { taskboard_id } => {
                self.reduce_archive_taskboard(snapshot, taskboard_id, actor)
                    .await
            }
            Command::CreateWorkItem(spec) => {
                self.reduce_create_work_item(snapshot, spec, actor).await
            }
            Command::DropWorkItem {
                task_id,
                taskboard_id,
                role_id,
            } => {
                self.reduce_drop_work_item(snapshot, task_id, taskboard_id, role_id, actor)
                    .await
            }
            Command::SpawnChildWorkItems {
                parent_task_id,
                children,
            } => {
                self.reduce_spawn_child_work_items(snapshot, parent_task_id, children, actor)
                    .await
            }
            Command::CreateWorkOffer { task_id, agent_id } => {
                self.reduce_create_work_offer(snapshot, task_id, agent_id, actor)
                    .await
            }
            Command::RespondWorkOffer { offer_id, accept } => {
                self.reduce_respond_work_offer(snapshot, offer_id, accept, actor)
                    .await
            }
            Command::RequestHumanInput {
                task_id,
                kind,
                prompt,
            } => {
                self.reduce_request_human_input(snapshot, task_id, kind, prompt, actor)
                    .await
            }
            Command::ResolveHumanInput {
                human_input_id,
                answer,
            } => {
                self.reduce_resolve_human_input(snapshot, human_input_id, answer, actor)
                    .await
            }
            Command::RequestTaskRework { task_id, reason } => {
                self.reduce_request_task_rework(snapshot, task_id, reason, actor)
                    .await
            }
            Command::RequestOrganizationDrain { target_revision } => {
                self.reduce_request_organization_drain(snapshot, target_revision, actor)
                    .await
            }
            Command::CompleteOrganizationDrain { revision } => {
                self.reduce_complete_organization_drain(snapshot, revision, actor)
                    .await
            }
            Command::ResumeOrganization { revision } => {
                self.reduce_resume_organization(snapshot, revision, actor)
                    .await
            }
            Command::RelocateWorkItems {
                from_board_id,
                to_board_id,
                task_ids,
                reason,
            } => {
                self.reduce_relocate_work_items(
                    snapshot,
                    from_board_id,
                    to_board_id,
                    task_ids,
                    reason,
                    actor,
                )
                .await
            }
            Command::FireAgent { agent_id, reason } => {
                self.reduce_fire_agent(snapshot, agent_id, reason, actor)
                    .await
            }
            _ => super::misrouted(),
        }
    }
}

async fn create_work_item(
    snapshot: &Snapshot,
    spec: WorkItemSpec,
    _actor: &ActorRef,
) -> Result<TaskView> {
    let board = snapshot
        .taskboards
        .iter()
        .find(|board| board.id == spec.taskboard_id && !board.archived)
        .ok_or(OrchestratorError::NotFound)?;
    let required_role_id = spec.required_role_id.or(board.default_role_id);
    if let Some(role_id) = required_role_id {
        ensure_role(snapshot, role_id)?;
    }
    if let Some(parent_task_id) = spec.parent_task_id {
        let parent = snapshot
            .tasks
            .iter()
            .find(|task| task.id == parent_task_id)
            .ok_or(OrchestratorError::NotFound)?;
        if parent.parent_task_id.is_some() {
            return Err(OrchestratorError::Validation(
                "child work items cannot have grandchildren".into(),
            ));
        }
        if matches!(parent.status, TaskStatus::Done | TaskStatus::Cancelled) {
            return Err(OrchestratorError::Validation(
                "cannot add children to a final parent task".into(),
            ));
        }
        if matches!(parent.status, TaskStatus::Running | TaskStatus::Review) {
            return Err(OrchestratorError::Validation(
                "cannot add children while the parent task is active".into(),
            ));
        }
        if let Some(mission_id) = spec.mission_id
            && mission_id != parent.mission_id
        {
            return Err(OrchestratorError::Validation(
                "work item and parent must share a mission".into(),
            ));
        }
    }
    if spec.title.trim().is_empty()
        || spec.title.len() > 256
        || spec.objective.trim().is_empty()
        || spec.objective.len() > MAX_MESSAGE_BODY_BYTES
        || spec
            .dependencies
            .iter()
            .any(|dependency| !snapshot.tasks.iter().any(|task| task.id == *dependency))
    {
        return Err(OrchestratorError::Validation(
            "work item title, objective, or dependency is invalid".into(),
        ));
    }
    let mission_id = spec
        .mission_id
        .or_else(|| {
            spec.parent_task_id.and_then(|parent| {
                snapshot
                    .tasks
                    .iter()
                    .find(|task| task.id == parent)
                    .map(|task| task.mission_id)
            })
        })
        .unwrap_or_else(MissionId::nil);
    if let Some(mission) = snapshot
        .missions
        .iter()
        .find(|mission| mission.id == mission_id)
        && matches!(
            mission.status,
            MissionStatus::Completed | MissionStatus::Failed | MissionStatus::Cancelled
        )
    {
        return Err(OrchestratorError::Validation(
            "cannot add a work item to a final mission".into(),
        ));
    }
    let id = TaskId::new();
    let task = TaskView {
        id,
        mission_id,
        title: spec.title.trim().to_owned(),
        objective: spec.objective,
        dependencies: spec.dependencies,
        required_role_id,
        priority: spec.priority,
        budget: spec.budget,
        status: TaskStatus::Backlog,
        assigned_agent: None,
        reviewer_agent: None,
        claimed_at: None,
        claim_source: None,
        attempt: 0,
        max_attempts: crate::DEFAULT_MAX_ATTEMPTS,
        worktree: None,
        branch: Some(format!("frank/work-item-{id}")),
        result_artifact: None,
        taskboard_id: Some(spec.taskboard_id),
        workflow_id: spec.workflow_id,
        parent_task_id: spec.parent_task_id,
        child_task_ids: Vec::new(),
        kind: spec.kind,
        active_role_node_id: None,
        organization_revision: snapshot
            .organization
            .published
            .as_ref()
            .map(|graph| graph.published_revision),
        rework_limit: if spec.rework_limit == 0 {
            DEFAULT_REWORK_LIMIT
        } else {
            spec.rework_limit
        },
        rework_count: 0,
    };
    validate_task_view(&task, &snapshot.tasks)?;
    Ok(task)
}

fn validate_board_name(name: &str) -> Result<()> {
    if name.trim().is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
        return Err(OrchestratorError::Validation(
            "taskboard name is invalid".into(),
        ));
    }
    Ok(())
}

fn ensure_board(snapshot: &Snapshot, board_id: TaskboardId) -> Result<()> {
    if snapshot
        .taskboards
        .iter()
        .any(|board| board.id == board_id && !board.archived)
    {
        Ok(())
    } else {
        Err(OrchestratorError::NotFound)
    }
}

fn ensure_role(snapshot: &Snapshot, role_id: RoleId) -> Result<()> {
    if snapshot
        .roles
        .iter()
        .any(|role| role.id == role_id && !role.archived)
    {
        Ok(())
    } else {
        Err(OrchestratorError::NotFound)
    }
}

fn ensure_eligible_agent(snapshot: &Snapshot, task: &TaskView, agent_id: AgentId) -> Result<()> {
    let agent = snapshot
        .agents
        .iter()
        .find(|agent| agent.id == agent_id && !agent.archived)
        .ok_or(OrchestratorError::NotFound)?;
    if !matches!(agent.status, AgentStatus::Idle | AgentStatus::Offline) {
        return Err(OrchestratorError::Validation(
            "only an idle or offline agent can receive an offer".into(),
        ));
    }
    if let Some(role_id) = task.required_role_id
        && agent.role_id != Some(role_id)
    {
        return Err(OrchestratorError::Validation(
            "offer agent does not belong to the work item's role".into(),
        ));
    }
    if snapshot.tasks.iter().any(|candidate| {
        candidate.id != task.id
            && candidate.assigned_agent == Some(agent_id)
            && !matches!(candidate.status, TaskStatus::Done | TaskStatus::Cancelled)
    }) {
        return Err(OrchestratorError::Validation(
            "agent already owns unfinished work".into(),
        ));
    }
    if task.dependencies.iter().any(|dependency| {
        snapshot
            .tasks
            .iter()
            .find(|candidate| candidate.id == *dependency)
            .is_none_or(|candidate| candidate.status != TaskStatus::Done)
    }) {
        return Err(OrchestratorError::Validation(
            "work item dependencies must be done before offering it".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reduce::task::update_parent_child_locks;
    use frank_store::Store;

    fn owner_actor() -> ActorRef {
        ActorRef {
            kind: ActorKind::Device,
            id: Some(UserId::new().to_string()),
            display_name: Some("owner".into()),
        }
    }

    fn board(id: TaskboardId, mode: TaskboardDispatchMode) -> TaskboardView {
        let now = timestamp_now();
        TaskboardView {
            id,
            name: "Engineering".into(),
            project_id: None,
            workflow_id: None,
            dispatch_mode: mode,
            default_role_id: None,
            archived: false,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    fn work_item(board_id: TaskboardId, title: &str) -> WorkItemSpec {
        WorkItemSpec {
            mission_id: None,
            title: title.into(),
            objective: format!("Objective for {title}"),
            kind: WorkItemKind::Task,
            taskboard_id: board_id,
            workflow_id: None,
            parent_task_id: None,
            dependencies: Vec::new(),
            required_role_id: None,
            priority: 0,
            budget: Budget::unlimited(),
            rework_limit: DEFAULT_REWORK_LIMIT,
        }
    }

    fn worker(agent_id: AgentId) -> AgentView {
        AgentView {
            id: agent_id,
            role_id: None,
            role_revision: 0,
            display_name: "Worker".into(),
            model: None,
            effective_model: None,
            model_source: ModelSource::Role,
            model_override: None,
            pending_model_override: None,
            pending_model_change: false,
            instructions: String::new(),
            policy: AgentPolicy::default(),
            budget: Budget::unlimited(),
            status: AgentStatus::Idle,
            provider_session_id: None,
            last_claimed_at: None,
            archived: false,
        }
    }

    #[tokio::test]
    async fn board_work_items_fan_out_wait_and_rework_keep_one_card_contract() {
        let store = Store::open_in_memory().await.unwrap();
        let orchestrator = Orchestrator::new(store);
        let board_id = TaskboardId::new();
        let actor = owner_actor();
        let mut snapshot = Snapshot::empty(ServerId::new());
        snapshot
            .taskboards
            .push(board(board_id, TaskboardDispatchMode::Auto));

        let (snapshot, Event::TaskCreated { task: parent }, _) = orchestrator
            .reduce_workflow(
                snapshot,
                Command::CreateWorkItem(work_item(board_id, "Parent")),
                &actor,
            )
            .await
            .unwrap()
        else {
            panic!("work-item creation returned the wrong event");
        };
        let parent_id = parent.id;
        let child_specs = vec![
            work_item(board_id, "Child A"),
            work_item(board_id, "Child B"),
        ];
        let (snapshot, Event::ChildWorkItemsSpawned { parent, children }, _) = orchestrator
            .reduce_workflow(
                snapshot,
                Command::SpawnChildWorkItems {
                    parent_task_id: parent_id,
                    children: child_specs,
                },
                &actor,
            )
            .await
            .unwrap()
        else {
            panic!("fan-out returned the wrong event");
        };
        assert_eq!(parent.id, parent_id);
        assert_eq!(children.len(), 2);
        assert_eq!(parent.child_task_ids.len(), 2);
        assert_eq!(
            snapshot
                .tasks
                .iter()
                .find(|task| task.id == parent_id)
                .unwrap()
                .status,
            TaskStatus::Blocked
        );

        let (snapshot_after_wait, Event::HumanInputRequested { input, .. }, _) = orchestrator
            .reduce_workflow(
                snapshot,
                Command::RequestHumanInput {
                    task_id: parent_id,
                    kind: HumanInputKind::Question,
                    prompt: "Which release train?".into(),
                },
                &actor,
            )
            .await
            .unwrap()
        else {
            panic!("human-input request returned the wrong event");
        };
        let (mut snapshot, Event::HumanInputResolved { .. }, _) = orchestrator
            .reduce_workflow(
                snapshot_after_wait,
                Command::ResolveHumanInput {
                    human_input_id: input.id,
                    answer: "Stable".into(),
                },
                &actor,
            )
            .await
            .unwrap()
        else {
            panic!("human-input resolution returned the wrong event");
        };
        assert_eq!(
            snapshot
                .tasks
                .iter()
                .find(|task| task.id == parent_id)
                .unwrap()
                .status,
            TaskStatus::Blocked,
            "answering a question must not bypass unfinished child cards"
        );

        for child_id in parent.child_task_ids {
            snapshot
                .tasks
                .iter_mut()
                .find(|task| task.id == child_id)
                .unwrap()
                .status = TaskStatus::Done;
            update_parent_child_locks(&mut snapshot, child_id, &actor);
        }
        assert_eq!(
            snapshot
                .tasks
                .iter()
                .find(|task| task.id == parent_id)
                .unwrap()
                .status,
            TaskStatus::Ready
        );

        let (snapshot, Event::TaskReworkRequested { task, count, .. }, _) = orchestrator
            .reduce_workflow(
                snapshot,
                Command::RequestTaskRework {
                    task_id: parent_id,
                    reason: "Needs another pass".into(),
                },
                &actor,
            )
            .await
            .unwrap()
        else {
            panic!("rework returned the wrong event");
        };
        assert_eq!(task.id, parent_id);
        assert_eq!(count, 1);
        assert_eq!(task.rework_count, 1);
        assert_eq!(task.status, TaskStatus::Ready);
        assert_eq!(snapshot.tasks[0].id, parent_id);
    }

    #[test]
    fn fixed_lanes_are_a_closed_mapping_to_task_status() {
        assert_eq!(TaskboardLane::ALL.len(), 7);
        assert!(TaskboardLane::Done.is_terminal());
        assert!(!TaskboardLane::Running.is_terminal());
        for lane in TaskboardLane::ALL {
            let status: TaskStatus = lane.into();
            assert_eq!(TaskboardLane::from(status), lane);
        }
    }

    #[tokio::test]
    async fn pull_board_offer_is_accepted_before_provider_claim() {
        let store = Store::open_in_memory().await.unwrap();
        let orchestrator = Orchestrator::new(store);
        let board_id = TaskboardId::new();
        let agent_id = AgentId::new();
        let owner = owner_actor();
        let agent = ActorRef {
            kind: ActorKind::Agent,
            id: Some(agent_id.to_string()),
            display_name: Some("Worker".into()),
        };
        let mut snapshot = Snapshot::empty(ServerId::new());
        snapshot
            .taskboards
            .push(board(board_id, TaskboardDispatchMode::Pull));
        snapshot.agents.push(worker(agent_id));

        let (snapshot, Event::TaskCreated { task }, _) = orchestrator
            .reduce_workflow(
                snapshot,
                Command::CreateWorkItem(work_item(board_id, "Pull me")),
                &owner,
            )
            .await
            .unwrap()
        else {
            panic!("work-item creation returned the wrong event");
        };
        let (snapshot, Event::WorkOfferCreated { offer }, _) = orchestrator
            .reduce_workflow(
                snapshot,
                Command::CreateWorkOffer {
                    task_id: task.id,
                    agent_id,
                },
                &owner,
            )
            .await
            .unwrap()
        else {
            panic!("offer creation returned the wrong event");
        };
        assert_eq!(offer.status, WorkOfferStatus::Pending);
        let (snapshot, Event::WorkOfferResponded { offer }, _) = orchestrator
            .reduce_workflow(
                snapshot,
                Command::RespondWorkOffer {
                    offer_id: offer.id,
                    accept: true,
                },
                &agent,
            )
            .await
            .unwrap()
        else {
            panic!("offer response returned the wrong event");
        };
        assert_eq!(offer.status, WorkOfferStatus::Accepted);
        let claimed = snapshot
            .tasks
            .iter()
            .find(|card| card.id == task.id)
            .unwrap();
        assert_eq!(claimed.assigned_agent, Some(agent_id));
        assert_eq!(claimed.status, TaskStatus::Ready);
        assert_eq!(claimed.claim_source, Some(TaskClaimSource::Manual));
    }
}
