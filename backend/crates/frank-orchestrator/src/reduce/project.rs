//! Project registration, cloning and archival.

use frank_protocol::*;

use crate::*;

impl Orchestrator {
    pub(crate) async fn reduce_project(
        &self,
        mut snapshot: Snapshot,
        command: Command,
        _actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        match command {
            Command::CreateProject(spec) => {
                validate_project_spec(&spec, &snapshot.server)?;
                let Some(path) = spec.path.as_ref() else {
                    return Err(OrchestratorError::Validation(
                        "clone projects must use the clone flow with a destination".into(),
                    ));
                };
                let project_probe = ProjectView {
                    id: ProjectId::nil(),
                    name: spec.name.clone(),
                    path: path.clone(),
                    base_branch: spec.base_branch.clone(),
                    remote: spec.remote.clone(),
                    check_commands: spec.check_commands.clone(),
                    worktree_root: spec
                        .worktree_root
                        .clone()
                        .unwrap_or_else(|| snapshot.server.worktree_root.clone()),
                    push_policy: spec.push_policy,
                    pr_policy: spec.pr_policy,
                    archived: false,
                };
                let workflow = GitWorkflow::new(
                    project_probe,
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
                let id = ProjectId::new();
                let project = ProjectView {
                    id,
                    name: spec.name,
                    path: path.clone(),
                    base_branch: spec.base_branch,
                    remote: spec.remote,
                    check_commands: spec.check_commands,
                    worktree_root: spec
                        .worktree_root
                        .unwrap_or_else(|| snapshot.server.worktree_root.clone()),
                    push_policy: spec.push_policy,
                    pr_policy: spec.pr_policy,
                    archived: false,
                };
                snapshot.projects.push(project.clone());
                Ok((
                    snapshot,
                    Event::ProjectUpserted { project },
                    CommandResult::Created { id: id.to_string() },
                ))
            }
            Command::CloneProject { url, destination } => {
                if url.trim().is_empty() || destination.trim().is_empty() {
                    return Err(OrchestratorError::Validation(
                        "clone URL and destination are required".into(),
                    ));
                }
                if !is_allowed_path(&destination, &snapshot.server.allowed_project_roots) {
                    return Err(OrchestratorError::Validation(
                        "destination is outside an allowed project root".into(),
                    ));
                }
                // Cloning is a filesystem/network side effect. Persist the
                // intent first; the operation reconciler can safely resume a
                // clone that completed before a daemon crash.
                let operation = OperationView {
                    id: OperationId::new(),
                    kind: OperationKind::CloneProject,
                    status: OperationStatus::Queued,
                    resource: serde_json::json!({
                        "url": url,
                        "destination": destination,
                    })
                    .to_string(),
                    phase: "clone".into(),
                    attempt: 0,
                    error: None,
                    created_at: timestamp_now(),
                    updated_at: timestamp_now(),
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
            Command::ArchiveProject { project_id } => {
                let project = snapshot
                    .projects
                    .iter_mut()
                    .find(|p| p.id == project_id)
                    .ok_or(OrchestratorError::NotFound)?;
                project.archived = true;
                Ok((
                    snapshot,
                    Event::ProjectArchived { project_id },
                    CommandResult::Accepted,
                ))
            }
            _ => Err(OrchestratorError::Validation(
                "command was routed to the wrong reducer".into(),
            )),
        }
    }
}
