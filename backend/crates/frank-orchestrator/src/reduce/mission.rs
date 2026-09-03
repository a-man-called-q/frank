//! Mission lifecycle, supervisor plans and delivery.

use frank_protocol::*;

use crate::*;

impl Orchestrator {
    pub(crate) async fn reduce_mission(
        &self,
        mut snapshot: Snapshot,
        command: Command,
        _actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        match command {
            Command::CreateMission {
                project_id,
                objective,
            } => {
                let project = snapshot
                    .projects
                    .iter()
                    .find(|project| project.id == project_id && !project.archived)
                    .ok_or(OrchestratorError::NotFound)?;
                let provider = snapshot.server.supervisor_provider.ok_or_else(|| {
                    OrchestratorError::Validation(
                        "choose Codex or Claude as supervisor before creating a mission".into(),
                    )
                })?;
                if objective.trim().is_empty() || objective.len() > MAX_MESSAGE_BODY_BYTES {
                    return Err(OrchestratorError::Validation(
                        "mission objective is empty or too large".into(),
                    ));
                }
                let id = MissionId::new();
                let branch = format!(
                    "frank/mission-{}-{}",
                    slug(&objective),
                    &id.to_string().replace('-', "")[..8]
                );
                let mission = MissionView {
                    id,
                    project_id: project.id,
                    objective,
                    status: MissionStatus::Draft,
                    supervisor_provider: provider,
                    supervisor_session_id: None,
                    branch,
                    budget: snapshot.server.default_budget.clone(),
                    created_at: timestamp_now(),
                    updated_at: timestamp_now(),
                };
                snapshot.missions.push(mission.clone());
                Ok((
                    snapshot,
                    Event::MissionCreated { mission },
                    CommandResult::Created { id: id.to_string() },
                ))
            }
            Command::SetMissionStatus { mission_id, status } => {
                let mission_index = snapshot
                    .missions
                    .iter()
                    .position(|mission| mission.id == mission_id)
                    .ok_or(OrchestratorError::NotFound)?;
                let current_status = snapshot.missions[mission_index].status;
                if !current_status.can_transition_to(status) {
                    return Err(OrchestratorError::InvalidTransition(format!(
                        "mission cannot transition from {:?} to {:?}",
                        current_status, status
                    )));
                }
                if status == MissionStatus::Completed
                    && snapshot.tasks.iter().any(|task| {
                        task.mission_id == mission_id && task.status != TaskStatus::Done
                    })
                {
                    return Err(OrchestratorError::Validation(
                        "all mission tasks must be done before completing the mission".into(),
                    ));
                }
                if status == MissionStatus::Active {
                    let mission = snapshot.missions[mission_index].clone();
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
                    workflow
                        .validate_repository()
                        .await
                        .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
                }
                let mission = &mut snapshot.missions[mission_index];
                mission.status = status;
                mission.updated_at = timestamp_now();
                Ok((
                    snapshot,
                    Event::MissionStatusChanged { mission_id, status },
                    CommandResult::Accepted,
                ))
            }
            Command::PauseMission { mission_id } => {
                self.pause_or_resume(snapshot, mission_id, MissionStatus::Paused)
                    .await
            }
            Command::ResumeMission { mission_id } => {
                self.pause_or_resume(snapshot, mission_id, MissionStatus::Active)
                    .await
            }
            Command::DeliverMission { mission_id } => {
                let mission = snapshot
                    .missions
                    .iter()
                    .find(|mission| mission.id == mission_id)
                    .cloned()
                    .ok_or(OrchestratorError::NotFound)?;
                if !matches!(mission.status, MissionStatus::Completed)
                    || snapshot.tasks.iter().any(|task| {
                        task.mission_id == mission_id && task.status != TaskStatus::Done
                    })
                {
                    return Err(OrchestratorError::Validation(
                        "all mission tasks must be accepted before delivery".into(),
                    ));
                }
                if let Some(operation) = snapshot
                    .operations
                    .iter()
                    .find(|operation| {
                        matches!(
                            operation.kind,
                            OperationKind::PushMission | OperationKind::OpenDraftPullRequest
                        ) && operation.resource == mission_id.to_string()
                            && !matches!(
                                operation.status,
                                OperationStatus::Cancelled | OperationStatus::Failed
                            )
                    })
                    .cloned()
                {
                    return Ok((
                        snapshot,
                        Event::OperationChanged {
                            operation: operation.clone(),
                        },
                        CommandResult::Operation(operation),
                    ));
                }
                let now = timestamp_now();
                let operation = OperationView {
                    id: OperationId::new(),
                    kind: OperationKind::PushMission,
                    status: OperationStatus::Queued,
                    resource: mission_id.to_string(),
                    phase: "queued".into(),
                    attempt: 0,
                    error: None,
                    created_at: now.clone(),
                    updated_at: now,
                };
                snapshot.operations.push(operation.clone());
                Ok((
                    snapshot,
                    Event::OperationChanged {
                        operation: operation.clone(),
                    },
                    CommandResult::Operation(operation),
                ))
            }
            Command::SubmitSupervisorPlan {
                mission_id,
                proposal,
            } => {
                validate_supervisor_proposal(&snapshot, mission_id, &proposal)?;
                Ok((
                    snapshot,
                    Event::SupervisorPlanProposed {
                        mission_id,
                        proposal,
                    },
                    CommandResult::Accepted,
                ))
            }
            _ => super::misrouted(),
        }
    }
}
