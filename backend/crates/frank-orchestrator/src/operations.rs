//! Long-running operations: worktrees, checks, commits, pushes, draft PRs,
//! project clones and memory writes.
//!
//! Each runner is resumable. An operation row is the durable record of intent,
//! so a runner must be safe to re-enter after a daemon restart -- which is why
//! they finish through update_operation/finish_operation rather than mutating
//! the snapshot directly.

use std::path::PathBuf;

use frank_protocol::*;

use crate::git::{GitWorkflow, WorktreePlan};
use crate::*;

#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct CloneProjectOperation {
    pub(crate) url: String,
    pub(crate) destination: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct CreateWorktreeOperation {
    pub(crate) project_id: ProjectId,
    pub(crate) mission_id: MissionId,
    pub(crate) mission_branch: String,
    pub(crate) mission_base: String,
    pub(crate) mission_path: String,
    pub(crate) task_id: TaskId,
    pub(crate) task_branch: String,
    pub(crate) task_base: String,
    pub(crate) task_path: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct WriteMemoryOperation {
    pub(crate) agent_id: AgentId,
    pub(crate) path: String,
    pub(crate) content: String,
}

impl Orchestrator {
    pub(crate) async fn reconcile_operations(&self) -> Result<()> {
        let snapshot = self.store.snapshot().await?;
        let operations = snapshot
            .operations
            .iter()
            .filter(|operation| {
                matches!(
                    operation.status,
                    OperationStatus::Queued
                        | OperationStatus::Running
                        | OperationStatus::Recovering
                ) || (operation.status == OperationStatus::Waiting
                    && operation.phase != "awaiting-helper")
            })
            .cloned()
            .collect::<Vec<_>>();
        for operation in operations {
            let lock_resource = operation_lock_resource(&operation);
            let operation_id = operation.id;
            if !self
                .store
                .acquire_operation_lock(&lock_resource, operation_id)
                .await?
            {
                continue;
            }
            let result = match operation.kind {
                OperationKind::CloneProject => self.run_clone_project_operation(operation).await,
                OperationKind::CreateWorktree => {
                    self.run_create_worktree_operation(operation).await
                }
                OperationKind::RunChecks => self.run_checks_operation(operation).await,
                OperationKind::CommitTask => self.run_commit_task_operation(operation).await,
                OperationKind::PushMission => self.run_push_mission_operation(operation).await,
                OperationKind::OpenDraftPullRequest => {
                    self.run_open_draft_pr_operation(operation).await
                }
                OperationKind::WriteMemory => self.run_write_memory_operation(operation).await,
                OperationKind::HostUpdate => self.run_host_update_operation(operation).await,
                OperationKind::StageUpdate => self.run_stage_update_operation(operation).await,
                // These kinds are persisted now so the protocol can report a
                // durable operation, even though their concrete workers are
                // added by their respective checkpoint. Leaving them queued
                // would create an infinite retry loop, so mark the operation
                // explicitly unsupported and surface it in the audit stream.
                _ => {
                    self.finish_operation(
                        operation.id,
                        OperationStatus::Failed,
                        "unsupported operation worker".into(),
                    )
                    .await
                }
            };
            self.store
                .release_operation_lock(&lock_resource, operation_id)
                .await?;
            result?;
        }
        Ok(())
    }

    pub(crate) async fn run_create_worktree_operation(
        &self,
        operation: OperationView,
    ) -> Result<()> {
        let request: CreateWorktreeOperation =
            serde_json::from_str(&operation.resource).map_err(|_| {
                OrchestratorError::Validation("worktree operation metadata is invalid".into())
            })?;
        let task_id = request.task_id;
        let result = async {
            self.update_operation(
                &operation,
                OperationStatus::Running,
                "mission-worktree",
                None,
            )
            .await?;
            let snapshot = self.store.snapshot().await?;
            let project = snapshot
                .projects
                .iter()
                .find(|project| project.id == request.project_id && !project.archived)
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
                .create_worktree(&WorktreePlan {
                    branch: request.mission_branch,
                    path: PathBuf::from(request.mission_path),
                    base: request.mission_base,
                })
                .await
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            self.update_operation(&operation, OperationStatus::Running, "task-worktree", None)
                .await?;
            workflow
                .create_worktree(&WorktreePlan {
                    branch: request.task_branch,
                    path: PathBuf::from(request.task_path),
                    base: request.task_base,
                })
                .await
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            Ok::<(), OrchestratorError>(())
        }
        .await;
        match result {
            Ok(()) => {
                self.finish_operation(operation.id, OperationStatus::Succeeded, String::new())
                    .await?;
            }
            Err(error) => {
                let reason = error.to_string();
                self.finish_operation(operation.id, OperationStatus::Failed, reason)
                    .await?;
                // Do not leave a card permanently Running when its durable
                // worktree intent cannot be fulfilled. The failed operation
                // remains available for diagnostics; the task is blocked so
                // an operator can fix the path/repository and explicitly
                // retry it without the scheduler spawning a provider into a
                // missing directory.
                let _ = self.transition_task(task_id, TaskStatus::Blocked).await;
            }
        }
        Ok(())
    }

    pub(crate) async fn run_push_mission_operation(&self, operation: OperationView) -> Result<()> {
        let result = async {
            let mission_id = MissionId::parse(&operation.resource).map_err(|_| {
                OrchestratorError::Validation("delivery operation mission id is invalid".into())
            })?;
            self.update_operation(
                &operation,
                OperationStatus::Running,
                "delivery-started",
                None,
            )
            .await?;
            let snapshot = self.store.snapshot().await?;
            let mission = snapshot
                .missions
                .iter()
                .find(|mission| mission.id == mission_id)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            if mission.status != MissionStatus::Completed
                || snapshot
                    .tasks
                    .iter()
                    .any(|task| task.mission_id == mission_id && task.status != TaskStatus::Done)
            {
                return Err(OrchestratorError::Validation(
                    "all mission tasks must be accepted before delivery".into(),
                ));
            }

            // This event is informational; the operation row remains the
            // idempotency authority. Re-emitting it after a crash is safe,
            // while Git push/PR workers below explicitly detect work already
            // performed before retrying.
            self.store
                .commit_command(
                    CommandId::new(),
                    Some(snapshot.revision),
                    ActorRef::system(),
                    Event::DeliveryStarted { mission_id },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await?;
            self.update_operation(&operation, OperationStatus::Running, "push", None)
                .await?;
            self.complete_delivery(mission_id).await?;
            let snapshot = self.store.snapshot().await?;
            let project = snapshot
                .projects
                .iter()
                .find(|project| project.id == mission.project_id && !project.archived)
                .ok_or(OrchestratorError::NotFound)?;
            if project.pr_policy == PrPolicy::Draft {
                // Queue the PR as a second durable saga before the push
                // operation is marked complete. If frankd dies between these
                // two commits, the retry sees the same PR operation and does
                // not enqueue a duplicate.
                self.queue_draft_pr_operation(mission_id).await?;
            } else {
                self.commit_delivery_completed(mission_id, None).await?;
            }
            Ok::<(), OrchestratorError>(())
        }
        .await;
        match result {
            Ok(()) => {
                self.finish_operation(operation.id, OperationStatus::Succeeded, String::new())
                    .await?;
            }
            Err(error) => {
                let reason = error.to_string();
                // Delivery failures do not discard the local integration
                // worktree. Persist the blocked event before marking the
                // operation failed so the next retry has an explicit audit
                // trail and can be initiated from the UI/CLI.
                if let Some(mission_id) = MissionId::parse(&operation.resource).ok()
                    && let Ok(snapshot) = self.store.snapshot().await
                {
                    let _ = self
                        .store
                        .commit_command(
                            CommandId::new(),
                            Some(snapshot.revision),
                            ActorRef::system(),
                            Event::DeliveryBlocked {
                                mission_id,
                                reason: "push or draft PR delivery failed; local mission work remains intact".into(),
                            },
                            snapshot,
                            CommandResult::Accepted,
                        )
                        .await;
                }
                self.finish_operation(operation.id, OperationStatus::Failed, reason)
                    .await?;
            }
        }
        Ok(())
    }

    pub(crate) async fn run_open_draft_pr_operation(&self, operation: OperationView) -> Result<()> {
        let result = async {
            let mission_id = MissionId::parse(&operation.resource).map_err(|_| {
                OrchestratorError::Validation("draft PR operation mission id is invalid".into())
            })?;
            self.update_operation(&operation, OperationStatus::Running, "draft-pr", None)
                .await?;
            let snapshot = self.store.snapshot().await?;
            let mission = snapshot
                .missions
                .iter()
                .find(|mission| mission.id == mission_id)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            if mission.status != MissionStatus::Completed
                || snapshot
                    .tasks
                    .iter()
                    .any(|task| task.mission_id == mission_id && task.status != TaskStatus::Done)
            {
                return Err(OrchestratorError::Validation(
                    "all mission tasks must be accepted before opening a draft PR".into(),
                ));
            }
            let project = snapshot
                .projects
                .iter()
                .find(|project| project.id == mission.project_id && !project.archived)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            if project.pr_policy != PrPolicy::Draft {
                return Err(OrchestratorError::Validation(
                    "project draft PR policy is disabled".into(),
                ));
            }
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
            let draft_pr_url = workflow
                .open_draft_pr(&mission.branch)
                .await
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            self.commit_delivery_completed(mission_id, draft_pr_url)
                .await?;
            Ok::<(), OrchestratorError>(())
        }
        .await;
        match result {
            Ok(()) => {
                self.finish_operation(operation.id, OperationStatus::Succeeded, String::new())
                    .await?;
            }
            Err(error) => {
                let reason = error.to_string();
                if let Some(mission_id) = MissionId::parse(&operation.resource).ok()
                    && let Ok(snapshot) = self.store.snapshot().await
                {
                    let _ = self
                        .store
                        .commit_command(
                            CommandId::new(),
                            Some(snapshot.revision),
                            ActorRef::system(),
                            Event::DeliveryBlocked {
                                mission_id,
                                reason:
                                    "draft PR delivery failed; local mission work remains intact"
                                        .into(),
                            },
                            snapshot,
                            CommandResult::Accepted,
                        )
                        .await;
                }
                self.finish_operation(operation.id, OperationStatus::Failed, reason)
                    .await?;
            }
        }
        Ok(())
    }

    pub(crate) async fn queue_draft_pr_operation(
        &self,
        mission_id: MissionId,
    ) -> Result<OperationView> {
        for _ in 0..4 {
            let mut snapshot = self.store.snapshot().await?;
            if let Some(operation) = snapshot.operations.iter().find(|operation| {
                operation.kind == OperationKind::OpenDraftPullRequest
                    && operation.resource == mission_id.to_string()
                    && !matches!(operation.status, OperationStatus::Cancelled)
            }) {
                return Ok(operation.clone());
            }
            let now = timestamp_now();
            let operation = OperationView {
                id: OperationId::new(),
                kind: OperationKind::OpenDraftPullRequest,
                status: OperationStatus::Queued,
                resource: mission_id.to_string(),
                phase: "queued".into(),
                attempt: 0,
                error: None,
                created_at: now.clone(),
                updated_at: now,
            };
            snapshot.operations.push(operation.clone());
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(snapshot.revision),
                    ActorRef::system(),
                    Event::OperationChanged {
                        operation: operation.clone(),
                    },
                    snapshot,
                    CommandResult::Operation(operation.clone()),
                )
                .await
            {
                Ok(_) => return Ok(operation),
                Err(StoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }

    pub(crate) async fn commit_delivery_completed(
        &self,
        mission_id: MissionId,
        draft_pr_url: Option<String>,
    ) -> Result<()> {
        for _ in 0..4 {
            let snapshot = self.store.snapshot().await?;
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(snapshot.revision),
                    ActorRef::system(),
                    Event::DeliveryCompleted {
                        mission_id,
                        draft_pr_url: draft_pr_url.clone(),
                    },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await
            {
                Ok(_) => return Ok(()),
                Err(StoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }

    pub(crate) async fn run_write_memory_operation(&self, operation: OperationView) -> Result<()> {
        let result = async {
            let request: WriteMemoryOperation = serde_json::from_str(&operation.resource)
                .map_err(|_| OrchestratorError::Validation("memory operation is invalid".into()))?;
            self.update_operation(&operation, OperationStatus::Running, "write", None)
                .await?;
            frank_store::memory::MemoryRepository::new(&self.memory_root)
                .propose(request.agent_id, &request.path, &request.content)
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            let snapshot = self.store.snapshot().await?;
            self.store
                .commit_command(
                    CommandId::new(),
                    Some(snapshot.revision),
                    ActorRef::system(),
                    Event::MemoryProposed {
                        agent_id: request.agent_id,
                        path: request.path,
                    },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await?;
            Ok::<(), OrchestratorError>(())
        }
        .await;
        match result {
            Ok(()) => {
                self.finish_operation(operation.id, OperationStatus::Succeeded, String::new())
                    .await?;
            }
            Err(error) => {
                self.finish_operation(operation.id, OperationStatus::Failed, error.to_string())
                    .await?;
            }
        }
        Ok(())
    }

    pub(crate) async fn run_clone_project_operation(&self, operation: OperationView) -> Result<()> {
        let payload: CloneProjectOperation =
            serde_json::from_str(&operation.resource).map_err(|_| {
                OrchestratorError::Validation("clone operation metadata is invalid".into())
            })?;
        let snapshot = self.store.snapshot().await?;
        let allowed_roots = snapshot
            .server
            .allowed_project_roots
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        self.update_operation(&operation, OperationStatus::Running, "clone", None)
            .await?;
        let clone_result = async {
            let destination = Path::new(&payload.destination);
            if destination.exists() {
                // A crash after `git clone` but before projection commit is
                // recovered by validating and registering the existing repo,
                // never by running clone a second time.
                let probe = ProjectView {
                    id: ProjectId::nil(),
                    name: "clone-probe".into(),
                    path: payload.destination.clone(),
                    base_branch: "main".into(),
                    remote: Some("origin".into()),
                    check_commands: Vec::new(),
                    worktree_root: snapshot.server.worktree_root.clone(),
                    push_policy: PushPolicy::MissionBranch,
                    pr_policy: PrPolicy::Draft,
                    archived: false,
                };
                GitWorkflow::new(probe, allowed_roots.clone())
                    .map_err(|error| OrchestratorError::Validation(error.to_string()))?
                    .validate_repository()
                    .await
                    .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            } else {
                GitWorkflow::clone_project(&payload.url, destination, &allowed_roots)
                    .await
                    .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            }
            let probe = ProjectView {
                id: ProjectId::nil(),
                name: "clone-probe".into(),
                path: payload.destination.clone(),
                base_branch: "main".into(),
                remote: Some("origin".into()),
                check_commands: Vec::new(),
                worktree_root: snapshot.server.worktree_root.clone(),
                push_policy: PushPolicy::MissionBranch,
                pr_policy: PrPolicy::Draft,
                archived: false,
            };
            let base_branch = GitWorkflow::new(probe, allowed_roots)
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?
                .current_branch()
                .await
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            // A daemon crash can happen after the project projection commits
            // but before the clone operation is marked complete.  Reconcile
            // that window by treating an already-registered destination as
            // success; never append a second ProjectCreated event for the
            // same checkout.
            let registered = self
                .store
                .snapshot()
                .await?
                .projects
                .iter()
                .any(|project| !project.archived && project.path == payload.destination);
            if !registered {
                self.commit_reduced(
                    CommandEnvelope {
                        protocol_version: PROTOCOL_VERSION,
                        command_id: CommandId::new(),
                        expected_revision: None,
                        command: Command::CreateProject(ProjectSpec {
                            name: Path::new(&payload.destination)
                                .file_name()
                                .and_then(|name| name.to_str())
                                .unwrap_or("project")
                                .to_string(),
                            path: Some(payload.destination.clone()),
                            clone_url: Some(payload.url.clone()),
                            base_branch,
                            remote: Some("origin".into()),
                            check_commands: Vec::new(),
                            worktree_root: Some(snapshot.server.worktree_root.clone()),
                            push_policy: PushPolicy::MissionBranch,
                            pr_policy: PrPolicy::Draft,
                        }),
                    },
                    ActorRef::system(),
                )
                .await?;
            }
            Ok::<(), OrchestratorError>(())
        }
        .await;
        match clone_result {
            Ok(()) => {
                self.finish_operation(operation.id, OperationStatus::Succeeded, String::new())
                    .await?
            }
            Err(error) => {
                self.finish_operation(operation.id, OperationStatus::Failed, error.to_string())
                    .await?
            }
        }
        Ok(())
    }

    /// Execute a standalone, daemon-owned check operation. Task acceptance
    /// normally uses the larger `CommitTask` saga (which runs checks before
    /// and after the commit), but keeping this worker real prevents a
    /// recovered `RunChecks` row from being mislabeled as an unsupported
    /// operation and makes project check runs observable in the ledger.
    pub(crate) async fn run_checks_operation(&self, operation: OperationView) -> Result<()> {
        let task_id = TaskId::parse(&operation.resource).map_err(|_| {
            OrchestratorError::Validation("check operation task id is invalid".into())
        })?;
        let result = async {
            self.update_operation(&operation, OperationStatus::Running, "checks", None)
                .await?;
            let snapshot = self.store.snapshot().await?;
            let task = snapshot
                .tasks
                .iter()
                .find(|task| task.id == task_id)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            let mission = snapshot
                .missions
                .iter()
                .find(|mission| mission.id == task.mission_id)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            let project = snapshot
                .projects
                .iter()
                .find(|project| project.id == mission.project_id && !project.archived)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            let worktree = task.worktree.clone().ok_or_else(|| {
                OrchestratorError::Validation("check operation worktree is unavailable".into())
            })?;
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
            let checks = workflow
                .run_checks(Path::new(&worktree))
                .await
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            if checks.iter().any(|check| !check.success) {
                return Err(OrchestratorError::Validation(
                    "required project checks failed".into(),
                ));
            }
            Ok::<(), OrchestratorError>(())
        }
        .await;
        match result {
            Ok(()) => {
                self.finish_operation(operation.id, OperationStatus::Succeeded, String::new())
                    .await?;
            }
            Err(error) => {
                self.finish_operation(operation.id, OperationStatus::Failed, error.to_string())
                    .await?;
            }
        }
        Ok(())
    }

    pub(crate) async fn run_commit_task_operation(&self, operation: OperationView) -> Result<()> {
        let task_id = TaskId::parse(&operation.resource)
            .map_err(|_| OrchestratorError::Validation("operation task id is invalid".into()))?;
        let result = async {
            self.update_operation(&operation, OperationStatus::Running, "checks", None)
                .await?;
            let snapshot = self.store.snapshot().await?;
            let task = snapshot
                .tasks
                .iter()
                .find(|task| task.id == task_id)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            let mission = snapshot
                .missions
                .iter()
                .find(|mission| mission.id == task.mission_id)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            let project = snapshot
                .projects
                .iter()
                .find(|project| project.id == mission.project_id && !project.archived)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            let worktree = task.worktree.clone().ok_or_else(|| {
                OrchestratorError::Validation("task worktree is unavailable".into())
            })?;
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
            let checks = workflow
                .run_checks(Path::new(&worktree))
                .await
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            if checks.iter().any(|check| !check.success) {
                return Err(OrchestratorError::Validation(
                    "required project checks failed; task remains in review".into(),
                ));
            }
            self.update_operation(&operation, OperationStatus::Running, "capture-diff", None)
                .await?;
            let diff = workflow
                .capture_diff(Path::new(&worktree))
                .await
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            if !diff.is_empty() {
                // Capture the review payload before mutating either branch.
                // The command id is derived from the task id, so a daemon
                // crash after the artifact commit but before the operation
                // advances can replay the same publication without creating
                // duplicate artifacts.
                self.publish_task_diff(&task, &diff).await?;
            }
            let mission_plan = workflow.branch_plan(&mission.branch, &workflow.project.base_branch);
            self.update_operation(
                &operation,
                OperationStatus::Running,
                "mission-worktree",
                None,
            )
            .await?;
            workflow
                .create_worktree(&mission_plan)
                .await
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            let task_branch = task
                .branch
                .clone()
                .unwrap_or_else(|| format!("frank/task-{task_id}"));
            // Commit subjects are daemon-generated and task-derived. Keeping
            // the identifier in the subject gives the durable Git saga a
            // stable marker to detect a squash that completed immediately
            // before a crash, even when another task has since merged too.
            let commit_message = stable_task_commit_message(task_id, &task.title);
            self.update_operation(&operation, OperationStatus::Running, "commit", None)
                .await?;
            workflow
                .commit_worktree(Path::new(&worktree), &commit_message)
                .await
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            // `changed == false` is a valid retry state: the daemon may have
            // committed just before a crash. Re-run checks and the idempotent
            // squash step regardless, so a restart cannot mark a task done
            // while its commit is still stranded on the task branch.
            let post_commit_checks = workflow
                .run_checks(Path::new(&worktree))
                .await
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            if post_commit_checks.iter().any(|check| !check.success) {
                return Err(OrchestratorError::Validation(
                    "required project checks failed after the daemon commit".into(),
                ));
            }
            self.update_operation(&operation, OperationStatus::Running, "squash-merge", None)
                .await?;
            workflow
                .squash_merge(&task_branch, &mission.branch, &commit_message)
                .await
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            self.transition_task(task_id, TaskStatus::Done).await?;
            Ok::<(), OrchestratorError>(())
        }
        .await;
        match result {
            Ok(()) => {
                self.finish_operation(operation.id, OperationStatus::Succeeded, String::new())
                    .await?;
            }
            Err(error) => {
                let message = error.to_string();
                // A squash merge conflict is a recoverable orchestration
                // problem, not a provider failure.  Keep the original task
                // in review and create a daemon-owned resolution card so a
                // worker can resolve it against the latest mission branch.
                // The helper is idempotent: a retry after a crash discovers
                // the existing child instead of appending another card.
                if is_merge_conflict_error(&message) {
                    self.handle_merge_conflict(task_id, &message).await?;
                }
                self.finish_operation(operation.id, OperationStatus::Failed, message)
                    .await?;
            }
        }
        Ok(())
    }

    pub(crate) async fn publish_task_diff(&self, task: &TaskView, diff: &[u8]) -> Result<()> {
        let mission_id = task.mission_id;
        let response = self
            .execute(
                CommandEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    command_id: CommandId::from(task.id.0),
                    expected_revision: None,
                    command: Command::PublishArtifact(ArtifactSpec {
                        mission_id,
                        task_id: Some(task.id),
                        name: format!("task-{}-diff.patch", task.id),
                        mime_type: "text/x-diff".into(),
                        bytes: diff.to_vec(),
                    }),
                },
                ActorRef::system(),
                DeviceRole::Owner,
            )
            .await;
        if let Some(error) = response.error {
            return Err(OrchestratorError::Validation(format!(
                "task diff artifact could not be published: {}",
                error.message
            )));
        }
        Ok(())
    }

    pub(crate) async fn handle_merge_conflict(&self, task_id: TaskId, detail: &str) -> Result<()> {
        let snapshot = self.store.snapshot().await?;
        let task = snapshot
            .tasks
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        let mission = snapshot
            .missions
            .iter()
            .find(|mission| mission.id == task.mission_id)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;

        // A conflict child is identified by the stable parent task ID in its
        // title/objective. This survives operation retries and daemon
        // restarts without introducing a second parent-task relation in the
        // v1 wire schema. A still-running child makes a retry idempotent;
        // completed children permit the next bounded attempt.
        let marker = format!("Resolve merge conflict for task {task_id}");
        if snapshot.tasks.iter().any(|candidate| {
            candidate.mission_id == task.mission_id
                && candidate.title.starts_with(&marker)
                && !matches!(
                    candidate.status,
                    TaskStatus::Done | TaskStatus::Cancelled | TaskStatus::Blocked
                )
        }) {
            return Ok(());
        }

        let conflict_attempts = snapshot
            .tasks
            .iter()
            .filter(|candidate| {
                candidate.mission_id == task.mission_id
                    && candidate
                        .title
                        .starts_with("Resolve merge conflict for task ")
            })
            .count();
        if conflict_attempts >= 2 {
            self.block_mission(
                mission.id,
                "merge conflict could not be resolved after two attempts".into(),
            )
            .await?;
            return Ok(());
        }

        // Keep dependencies aligned with the work that was already merged
        // into the mission branch. Depending on the conflicting review task
        // itself would deadlock the child because that task cannot become
        // Done until its merge succeeds.
        let objective_detail = detail
            .chars()
            .filter(|character| !character.is_control())
            .take(2_000)
            .collect::<String>();
        let title = if conflict_attempts == 0 {
            marker
        } else {
            format!("{marker} (attempt {})", conflict_attempts + 1)
        };
        let spec = TaskSpec {
            mission_id: mission.id,
            title,
            objective: format!(
                "Resolve the Git merge conflict for `{}` against mission branch `{}`.\n\nDaemon diagnostic:\n{}",
                task.title, mission.branch, objective_detail
            ),
            dependencies: task.dependencies,
            required_role_id: task.required_role_id,
            priority: task.priority.saturating_add(1),
            assigned_agent: None,
            budget: task.budget,
            taskboard_id: task.taskboard_id,
            workflow_id: task.workflow_id,
            parent_task_id: Some(task.id),
            kind: task.kind,
            rework_limit: task.rework_limit,
        };
        let response = self
            .commit_reduced(
                CommandEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    command_id: CommandId::new(),
                    expected_revision: None,
                    command: Command::CreateTask(spec),
                },
                ActorRef::system(),
            )
            .await?;
        let child_id = match response.result {
            Some(CommandResult::Created { id }) => TaskId::parse(&id).map_err(|_| {
                OrchestratorError::Validation("conflict child task id is invalid".into())
            })?,
            _ => {
                return Err(OrchestratorError::Validation(
                    "conflict child task was not created".into(),
                ));
            }
        };
        // The child is immediately runnable once the mission is active. Its
        // worktree operation is still created only by SetTaskStatus::Running,
        // preserving the same durable saga as every other task.
        self.commit_reduced(
            CommandEnvelope {
                protocol_version: PROTOCOL_VERSION,
                command_id: CommandId::new(),
                expected_revision: None,
                command: Command::SetTaskStatus {
                    task_id: child_id,
                    status: TaskStatus::Ready,
                },
            },
            ActorRef::system(),
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn update_operation(
        &self,
        original: &OperationView,
        status: OperationStatus,
        phase: &str,
        error: Option<String>,
    ) -> Result<()> {
        for _ in 0..4 {
            let mut snapshot = self.store.snapshot().await?;
            let expected_revision = snapshot.revision;
            let operation = snapshot
                .operations
                .iter_mut()
                .find(|candidate| candidate.id == original.id)
                .ok_or(OrchestratorError::NotFound)?;
            operation.status = status;
            operation.phase = phase.to_string();
            operation.attempt = operation.attempt.saturating_add(1);
            operation.error = error.clone();
            operation.updated_at = timestamp_now();
            let operation_value = operation.clone();
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(expected_revision),
                    ActorRef::system(),
                    Event::OperationChanged {
                        operation: operation_value,
                    },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await
            {
                Ok(_) => return Ok(()),
                Err(StoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }

    pub(crate) async fn finish_operation(
        &self,
        operation_id: OperationId,
        status: OperationStatus,
        error: String,
    ) -> Result<()> {
        for _ in 0..4 {
            let mut snapshot = self.store.snapshot().await?;
            let expected_revision = snapshot.revision;
            let operation = snapshot
                .operations
                .iter_mut()
                .find(|operation| operation.id == operation_id)
                .ok_or(OrchestratorError::NotFound)?;
            operation.status = status;
            operation.phase = if status == OperationStatus::Succeeded {
                "complete".into()
            } else {
                "failed".into()
            };
            operation.error = (!error.is_empty()).then_some(error.clone());
            operation.updated_at = timestamp_now();
            let operation_value = operation.clone();
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(expected_revision),
                    ActorRef::system(),
                    Event::OperationChanged {
                        operation: operation_value,
                    },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await
            {
                Ok(_) => return Ok(()),
                Err(StoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }
}
