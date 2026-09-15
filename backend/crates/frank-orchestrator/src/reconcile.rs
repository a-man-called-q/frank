//! Orchestrator reconcile flow.

use super::*;

impl Orchestrator {
    pub async fn reconcile(&self) -> Result<()> {
        self.renew_agent_capabilities().await;
        let mut snapshot = self.store.snapshot().await?;
        // A daemon can be interrupted after committing MessageQueued but
        // before the in-process broker advances it to Delivered.  Rebuild the
        // delivery work from the authoritative snapshot so queued messages
        // never depend on the lifetime of the HTTP request that created them.
        self.deliver_queued_messages(&snapshot).await?;
        self.expire_approvals(&snapshot).await?;
        self.expire_task_grants(&snapshot).await?;
        self.expire_terminal_leases(&snapshot).await?;
        self.expire_pending_messages(&snapshot).await?;
        // Time limits are independent of provider telemetry.  A provider can
        // be quiet (or crash before emitting a usage frame), so the durable
        // SQLite clock must still stop work when its deadline elapses.
        self.enforce_time_budgets().await?;
        // Time-budget enforcement may commit task and mission transitions.
        snapshot = self.store.snapshot().await?;
        self.repair_legacy_parent_dependencies(&snapshot).await?;
        snapshot = self.store.snapshot().await?;
        self.recover_running_worktree_intents(&snapshot).await?;
        self.reconcile_operations().await?;
        // Operation workers may commit task, operation, or delivery state.
        snapshot = self.store.snapshot().await?;
        self.budgets.lock().await.rebuild_from_snapshot(&snapshot);
        {
            let mut scheduler = self.scheduler.lock().await;
            scheduler.limits.max_concurrency = snapshot.server.max_concurrency.max(1) as usize;
        }

        // CreateMission commits before supervisor planning so the HTTP
        // command remains idempotent.  If frankd is killed in that window,
        // the mission is durable but has no DAG.  Re-run planning on startup
        // for those draft missions; plan_mission is itself idempotent and
        // will never append a second DAG when a prior attempt completed.
        let unplanned_drafts = snapshot
            .missions
            .iter()
            .filter(|mission| {
                mission.status == MissionStatus::Draft
                    && !snapshot
                        .tasks
                        .iter()
                        .any(|task| task.mission_id == mission.id)
            })
            .map(|mission| (mission.id, mission.objective.clone()))
            .collect::<Vec<_>>();
        if !unplanned_drafts.is_empty()
            && snapshot.organization_runtime.status != OrganizationDrainStatus::Draining
        {
            for (mission_id, objective) in unplanned_drafts {
                if let Err(error) = self.plan_mission(mission_id, &objective).await {
                    // Draft has no legal blocked transition. Keep the mission
                    // visible so the owner can retry after fixing provider or
                    // runtime health. The provider error is intentionally not
                    // copied into the wire snapshot because it may contain a
                    // local executable path; the next doctor/mission attempt
                    // exposes a sanitized diagnostic.
                    let _ = error;
                }
            }
            // Planning may commit several idempotent task transitions, even
            // when a later provider attempt fails partway through the plan.
            snapshot = self.store.snapshot().await?;
        }
        let active_missions = snapshot
            .missions
            .iter()
            .filter(|mission| mission.status == MissionStatus::Active)
            .map(|mission| mission.id)
            .collect::<HashSet<_>>();

        // A published graph upgrade drains the old office: no new cards are
        // promoted, offered, or started while existing Running/Review work
        // finishes. Once the last active card settles, persist a paused
        // runtime boundary; the owner must explicitly publish/resume under
        // the new revision (and relocate cards first when a board vanished).
        if snapshot.organization_runtime.status == OrganizationDrainStatus::Draining {
            let target_revision = snapshot
                .organization_runtime
                .drain_requested_revision
                .unwrap_or_default();
            let has_active_work = snapshot.tasks.iter().any(|task| {
                active_missions.contains(&task.mission_id)
                    && matches!(task.status, TaskStatus::Running | TaskStatus::Review)
            });
            if !has_active_work
                && snapshot.organization_runtime.pending_relocation_count == 0
                && target_revision > 0
            {
                let _ = self
                    .commit_reduced(
                        CommandEnvelope {
                            protocol_version: PROTOCOL_VERSION,
                            command_id: CommandId::new(),
                            expected_revision: None,
                            command: Command::CompleteOrganizationDrain {
                                revision: target_revision,
                            },
                        },
                        ActorRef::system(),
                    )
                    .await?;
            }
            return Ok(());
        }

        // Promote dependency-complete cards to Ready. A supervisor-generated
        // DAG already does this for roots; this branch covers later children
        // and cards created manually in the Board.
        let task_status_by_id = snapshot
            .tasks
            .iter()
            .map(|task| (task.id, task.status))
            .collect::<HashMap<_, _>>();
        let backlog_task_ids = snapshot
            .tasks
            .iter()
            .filter(|task| {
                task.status == TaskStatus::Backlog
                    && active_missions.contains(&task.mission_id)
                    && task.dependencies.iter().all(|dependency| {
                        task_status_by_id.get(dependency) == Some(&TaskStatus::Done)
                    })
            })
            .map(|task| task.id)
            .collect::<Vec<_>>();
        for task_id in &backlog_task_ids {
            let _ = self
                .execute(
                    CommandEnvelope {
                        protocol_version: PROTOCOL_VERSION,
                        command_id: CommandId::new(),
                        expected_revision: None,
                        command: Command::SetTaskStatus {
                            task_id: *task_id,
                            status: TaskStatus::Ready,
                        },
                    },
                    ActorRef::system(),
                    DeviceRole::Owner,
                )
                .await;
        }
        if !backlog_task_ids.is_empty() {
            // A successful promotion changes the authoritative task status;
            // reload once before the next reconciliation phase rather than
            // reading the whole snapshot for every dependency card.
            snapshot = self.store.snapshot().await?;
        }

        // A conflict-resolution card is a real task. Once its worker is
        // accepted, retry the original review task through the same durable
        // TaskAccept -> CommitTask saga. This keeps a conflict retry from
        // depending on a GUI reconnect and avoids silently marking the parent
        // done merely because the child card completed.
        if self.reconcile_conflict_resolutions(&snapshot).await? {
            // TaskAccept may queue durable commit work for the parent task.
            snapshot = self.store.snapshot().await?;
        }

        let mut ready_task_ids = snapshot
            .tasks
            .iter()
            .filter(|task| {
                task.status == TaskStatus::Ready && active_missions.contains(&task.mission_id)
            })
            .map(|task| (task.priority, task.id))
            .collect::<Vec<_>>();
        // Stable priority ordering makes fan-out scheduling reproducible and
        // ensures a high-value card is not delayed by snapshot iteration order.
        ready_task_ids.sort_by(|left, right| {
            right
                .0
                .cmp(&left.0)
                .then_with(|| left.1.to_string().cmp(&right.1.to_string()))
        });
        for (_, task_id) in ready_task_ids {
            let (task, claimed) = match self.ensure_task_assignment(&snapshot, task_id).await {
                Ok(Some(task)) => task,
                Ok(None) => continue,
                Err(_) => continue,
            };
            if claimed {
                // ClaimTask may have committed an assignment; keep the next
                // status transition and subsequent candidate selection based
                // on the authoritative projection.
                snapshot = self.store.snapshot().await?;
            }
            let response = self
                .execute(
                    CommandEnvelope {
                        protocol_version: PROTOCOL_VERSION,
                        command_id: CommandId::new(),
                        expected_revision: None,
                        command: Command::SetTaskStatus {
                            task_id: task.id,
                            status: TaskStatus::Running,
                        },
                    },
                    ActorRef::system(),
                    DeviceRole::Owner,
                )
                .await;
            if response.error.is_none() {
                // SetTaskStatus commits before the provider is launched. Do
                // not let start_task_session observe the pre-running snapshot.
                snapshot = self.store.snapshot().await?;
                let _ = self.start_task_session(&snapshot, task.id).await;
                // Starting a session can commit agent state or failure/retry
                // state; make the next task decision from the latest view.
                snapshot = self.store.snapshot().await?;
            }
        }

        // A process may have disappeared while the SQLite task remained
        // Running. Attempt a resume using the stable provider session ID.
        let running_task_ids = snapshot
            .tasks
            .iter()
            .filter(|task| task.status == TaskStatus::Running && task.assigned_agent.is_some())
            .map(|task| task.id)
            .collect::<Vec<_>>();
        for task_id in running_task_ids {
            if !self.sessions.lock().await.contains_key(&task_id) {
                let _ = self.start_task_session(&snapshot, task_id).await;
                // Startup may persist Starting, retry, or failure state.
                snapshot = self.store.snapshot().await?;
            }
        }
        Ok(())
    }

    /// Task grants are capabilities in their own right. Expiry must be a
    /// durable revocation event rather than a UI-only check, so a restarted
    /// daemon cannot accidentally honor an old approval.
    async fn expire_task_grants(&self, snapshot: &Snapshot) -> Result<()> {
        let now = epoch_seconds() as u128;
        let expired = snapshot
            .task_grants
            .iter()
            .filter(|grant| {
                !grant.revoked
                    && grant
                        .expires_at
                        .parse::<u128>()
                        .is_ok_and(|expires_at| expires_at <= now)
            })
            .map(|grant| grant.id.clone())
            .collect::<Vec<_>>();
        for grant_id in expired {
            let response = self
                .execute(
                    CommandEnvelope {
                        protocol_version: PROTOCOL_VERSION,
                        command_id: CommandId::new(),
                        expected_revision: None,
                        command: Command::RevokeTaskGrant { grant_id },
                    },
                    ActorRef::system(),
                    DeviceRole::Owner,
                )
                .await;
            if let Some(error) = response.error
                && !matches!(error.code, ErrorCode::Conflict | ErrorCode::StaleRevision)
            {
                return Err(OrchestratorError::Validation(error.message));
            }
        }
        Ok(())
    }

    /// Development snapshots created before parent/child joins were
    /// structural added the parent as a normal dependency. Remove only that
    /// known-invalid edge; the normal UpdateTask reducer records the repair
    /// in the event log, feed, audit JSONL, and Journal projection.
    async fn repair_legacy_parent_dependencies(&self, snapshot: &Snapshot) -> Result<()> {
        let repairs = snapshot
            .tasks
            .iter()
            .filter_map(|task| {
                let parent_id = task.parent_task_id?;
                task.dependencies.contains(&parent_id).then(|| {
                    let dependencies = task
                        .dependencies
                        .iter()
                        .copied()
                        .filter(|dependency| *dependency != parent_id)
                        .collect::<Vec<_>>();
                    (task.id, dependencies)
                })
            })
            .collect::<Vec<_>>();
        for (task_id, dependencies) in repairs {
            let _ = self
                .execute(
                    CommandEnvelope {
                        protocol_version: PROTOCOL_VERSION,
                        command_id: CommandId::new(),
                        expected_revision: None,
                        command: Command::UpdateTask {
                            task_id,
                            patch: TaskPatch {
                                dependencies: Some(dependencies),
                                ..TaskPatch::default()
                            },
                        },
                    },
                    ActorRef::system(),
                    DeviceRole::Owner,
                )
                .await;
        }
        Ok(())
    }

    pub(crate) async fn reconcile_conflict_resolutions(&self, snapshot: &Snapshot) -> Result<bool> {
        let tasks_by_id = snapshot
            .tasks
            .iter()
            .map(|task| (task.id, task))
            .collect::<HashMap<_, _>>();
        let mut parents = Vec::new();
        for child in snapshot.tasks.iter().filter(|task| {
            task.status == TaskStatus::Done
                && task.title.starts_with("Resolve merge conflict for task ")
        }) {
            let prefix = child
                .title
                .strip_prefix("Resolve merge conflict for task ")
                .and_then(|value| value.split_whitespace().next())
                .unwrap_or_default();
            let Ok(parent_id) = TaskId::parse(prefix) else {
                continue;
            };
            let Some(parent) = tasks_by_id.get(&parent_id) else {
                continue;
            };
            if parent.status != TaskStatus::Review || parent.worktree.is_none() {
                continue;
            }
            let active_operation = snapshot.operations.iter().any(|operation| {
                operation.kind == OperationKind::CommitTask
                    && operation.resource == parent_id.to_string()
                    && matches!(
                        operation.status,
                        OperationStatus::Queued
                            | OperationStatus::Running
                            | OperationStatus::Waiting
                            | OperationStatus::Recovering
                    )
            });
            if !active_operation {
                parents.push(parent_id);
            }
        }
        parents.sort_unstable_by_key(|id| id.to_string());
        parents.dedup();
        let has_parents = !parents.is_empty();
        for task_id in parents {
            let _ = self
                .execute(
                    CommandEnvelope {
                        protocol_version: PROTOCOL_VERSION,
                        command_id: CommandId::new(),
                        expected_revision: None,
                        command: Command::TaskAccept { task_id },
                    },
                    ActorRef::system(),
                    DeviceRole::Owner,
                )
                .await;
        }
        Ok(has_parents)
    }

    /// Older development snapshots could contain a Running card created by
    /// the pre-journal implementation. Reconstruct only the missing intent;
    /// never infer completion from a directory that happens to exist. The
    /// resulting event is the same atomic task+operation projection used for
    /// new transitions, so subsequent reconciliation can safely resume it.
    pub(crate) async fn recover_running_worktree_intents(&self, snapshot: &Snapshot) -> Result<()> {
        let missing = snapshot
            .tasks
            .iter()
            .filter(|task| {
                task.status == TaskStatus::Running
                    && !snapshot
                        .operations
                        .iter()
                        .any(|operation| worktree_operation_matches_task(operation, task.id))
            })
            .map(|task| task.id)
            .collect::<Vec<_>>();
        for task_id in missing {
            let mut latest = self.store.snapshot().await?;
            let Some(task_index) = latest.tasks.iter().position(|task| task.id == task_id) else {
                continue;
            };
            if latest.tasks[task_index].status != TaskStatus::Running
                || latest
                    .operations
                    .iter()
                    .any(|operation| worktree_operation_matches_task(operation, task_id))
            {
                continue;
            }
            let task = latest.tasks[task_index].clone();
            let mission = latest
                .missions
                .iter()
                .find(|mission| mission.id == task.mission_id)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            let project = latest
                .projects
                .iter()
                .find(|project| project.id == mission.project_id && !project.archived)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?;
            let workflow = GitWorkflow::new(
                project,
                latest
                    .server
                    .allowed_project_roots
                    .iter()
                    .map(PathBuf::from)
                    .collect(),
            )
            .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
            let mission_plan = workflow.branch_plan(&mission.branch, &workflow.project.base_branch);
            let task_plan = workflow.task_plan_from_base(task.id, &mission.branch);
            latest.tasks[task_index].worktree = Some(task_plan.path.to_string_lossy().into_owned());
            latest.tasks[task_index].branch = Some(task_plan.branch.clone());
            let now = timestamp_now();
            let operation = OperationView {
                id: OperationId::new(),
                kind: OperationKind::CreateWorktree,
                status: OperationStatus::Queued,
                resource: serde_json::to_string(&CreateWorktreeOperation {
                    project_id: mission.project_id,
                    mission_id: mission.id,
                    mission_branch: mission_plan.branch,
                    mission_base: mission_plan.base,
                    mission_path: mission_plan.path.to_string_lossy().into_owned(),
                    task_id,
                    task_branch: task_plan.branch,
                    task_base: task_plan.base,
                    task_path: task_plan.path.to_string_lossy().into_owned(),
                })
                .map_err(|error| OrchestratorError::Validation(error.to_string()))?,
                phase: "recovery-queued".into(),
                attempt: 0,
                error: None,
                created_at: now.clone(),
                updated_at: now,
            };
            latest.operations.push(operation.clone());
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(latest.revision),
                    ActorRef::system(),
                    Event::TaskWorktreeProvisioning {
                        task: latest.tasks[task_index].clone(),
                        operation,
                    },
                    latest,
                    CommandResult::Accepted,
                )
                .await
            {
                Ok(_) | Err(StoreError::StaleRevision { .. }) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    /// Revoke terminal control leases whose heartbeat window elapsed while
    /// the owner was disconnected. Lease expiry is a server-owned state
    /// transition rather than a client-side hint: clearing the durable lease
    /// emits an event, removes the SQLite control-lease projection, and lets
    /// the normal reconciler resume the paused provider on its next tick.
    pub(crate) async fn expire_terminal_leases(&self, snapshot: &Snapshot) -> Result<()> {
        let now = epoch_seconds() as u128;
        let expired = snapshot
            .terminals
            .iter()
            .filter_map(|session| {
                let lease = session.lease.as_ref()?;
                let expires_at = lease.expires_at.parse::<u128>().ok()?;
                (expires_at <= now).then_some((session.id, lease.clone()))
            })
            .collect::<Vec<_>>();

        for (session_id, previous_lease) in expired {
            // A concurrent renew/release wins over this stale reconciliation
            // pass. Retry the next 500 ms tick rather than clearing a fresh
            // lease with an old event.
            let mut latest = self.store.snapshot().await?;
            let Some(session) = latest
                .terminals
                .iter_mut()
                .find(|session| session.id == session_id)
            else {
                continue;
            };
            let still_expired = session.lease.as_ref().is_some_and(|lease| {
                lease.lease_id == previous_lease.lease_id
                    && lease
                        .expires_at
                        .parse::<u128>()
                        .is_ok_and(|expires_at| expires_at <= now)
            });
            if !still_expired {
                continue;
            }
            session.lease = None;
            let released = ControlLeaseView {
                lease_id: previous_lease.lease_id,
                session_id,
                actor: previous_lease.actor,
                expires_at: "released".into(),
            };
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(latest.revision),
                    ActorRef::system(),
                    Event::TerminalLeaseChanged { lease: released },
                    latest,
                    CommandResult::Accepted,
                )
                .await
            {
                Ok(_) => {
                    // `pause_for_terminal` already stopped the provider when
                    // control was acquired. Restore the agent's idle marker
                    // now that disconnect grace has expired; the scheduler
                    // will start/resume the task on its next reconciliation.
                    self.resume_after_terminal(session_id).await?;
                }
                Err(StoreError::StaleRevision { .. }) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    pub(crate) async fn ensure_task_assignment(
        &self,
        snapshot: &Snapshot,
        task_id: TaskId,
    ) -> Result<Option<(TaskView, bool)>> {
        let Some(mut task) = snapshot
            .tasks
            .iter()
            .find(|task| task.id == task_id)
            .cloned()
        else {
            return Ok(None);
        };
        if task.assigned_agent.is_some() {
            return Ok(Some((task, false)));
        }
        let active_task_agents = snapshot
            .tasks
            .iter()
            .filter(|candidate| {
                candidate.id != task_id
                    && candidate.assigned_agent.is_some()
                    && matches!(
                        candidate.status,
                        TaskStatus::Backlog
                            | TaskStatus::Ready
                            | TaskStatus::Running
                            | TaskStatus::Review
                            | TaskStatus::Blocked
                    )
            })
            .filter_map(|candidate| candidate.assigned_agent)
            .collect::<HashSet<_>>();
        let mut candidates = snapshot
            .agents
            .iter()
            .filter(|agent| {
                !agent.archived
                    && agent.display_name != "Frank supervisor"
                    && matches!(agent.status, AgentStatus::Offline | AgentStatus::Idle)
                    && !active_task_agents.contains(&agent.id)
                    // After the first Organization publish, automatic
                    // assignment must never select a legacy/non-staff agent
                    // and then rely on the reducer to reject it. Filtering
                    // here keeps the scheduler progressing to the next
                    // eligible staff member deterministically.
                    && organization_allows_agent(snapshot, agent.id)
                    && task
                        .required_role_id
                        .is_none_or(|role_id| agent.role_id == Some(role_id))
            })
            .cloned()
            .collect::<Vec<_>>();
        // Deterministic least-recently-claimed fairness. Missing timestamps
        // win first, then the oldest numeric timestamp, with the stable UUID
        // as a final tie-breaker so two daemon ticks cannot disagree.
        candidates.sort_by(|left, right| {
            let left_claim = left
                .last_claimed_at
                .as_deref()
                .and_then(|value| value.parse::<u128>().ok());
            let right_claim = right
                .last_claimed_at
                .as_deref()
                .and_then(|value| value.parse::<u128>().ok());
            left_claim
                .cmp(&right_claim)
                .then_with(|| left.id.to_string().cmp(&right.id.to_string()))
        });
        let agent = candidates.into_iter().next();
        let Some(agent) = agent else {
            return Ok(None);
        };
        let response = self
            .execute(
                CommandEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    command_id: CommandId::new(),
                    expected_revision: None,
                    command: Command::ClaimTask {
                        task_id,
                        agent_id: agent.id,
                        source: TaskClaimSource::Automatic,
                    },
                },
                ActorRef::system(),
                DeviceRole::Owner,
            )
            .await;
        if response.error.is_some() {
            return Ok(None);
        }
        task.assigned_agent = Some(agent.id);
        if task.status == TaskStatus::Backlog {
            task.status = TaskStatus::Ready;
        }
        Ok(Some((task, true)))
    }
}
