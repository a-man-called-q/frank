//! Orchestrator runtime flow flow.

use super::*;

impl Orchestrator {
    pub(crate) async fn pause_for_terminal(&self, session_id: TerminalSessionId) -> Result<()> {
        let snapshot = self.store.snapshot().await?;
        let Some(session) = snapshot
            .terminals
            .iter()
            .find(|session| session.id == session_id)
        else {
            return Ok(());
        };
        let task_id = session.task_id;
        let Some(task) = snapshot.tasks.iter().find(|task| task.id == task_id) else {
            return Ok(());
        };
        let provider_session = { self.sessions.lock().await.remove(&task_id) };
        if let Some(provider_session) = provider_session {
            let _ = provider_session.graceful_stop().await;
            self.scheduler.lock().await.finish(task_id);
        }
        if let Some(capability) = self
            .agent_capabilities
            .lock()
            .await
            .iter()
            .find(|(_, capability)| capability.task_id == task_id)
            .map(|(token, _)| token.clone())
        {
            self.revoke_agent_capability(&capability).await;
        }
        if let Some(agent_id) = task.assigned_agent {
            let _ = self
                .clear_agent_session(agent_id, AgentStatus::Paused)
                .await;
        }
        Ok(())
    }

    pub(crate) async fn resume_after_terminal(&self, session_id: TerminalSessionId) -> Result<()> {
        let snapshot = self.store.snapshot().await?;
        let Some(session) = snapshot
            .terminals
            .iter()
            .find(|session| session.id == session_id)
        else {
            return Ok(());
        };
        let Some(task) = snapshot
            .tasks
            .iter()
            .find(|task| task.id == session.task_id)
        else {
            return Ok(());
        };
        if let Some(agent_id) = task.assigned_agent {
            let _ = self.set_agent(agent_id, AgentStatus::Idle, None).await;
        }
        // The daemon reconciler observes the still-running task on its next
        // tick and starts a fresh provider session after the lease is
        // released. Avoid recursively calling reconcile from execute.
        Ok(())
    }

    pub(crate) async fn start_task_session(
        &self,
        snapshot: &Snapshot,
        task_id: TaskId,
    ) -> Result<()> {
        if self.sessions.lock().await.contains_key(&task_id) {
            return Ok(());
        }
        let task = snapshot
            .tasks
            .iter()
            .find(|task| task.id == task_id && task.status == TaskStatus::Running)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        let agent_id = task.assigned_agent.ok_or_else(|| {
            OrchestratorError::Validation("a running task must have an assigned agent".into())
        })?;
        let agent = snapshot
            .agents
            .iter()
            .find(|agent| agent.id == agent_id && !agent.archived)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        let mission = snapshot
            .missions
            .iter()
            .find(|mission| mission.id == task.mission_id)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        let scheduler_started = self.scheduler.lock().await.start(task.id);
        if !scheduler_started {
            return Ok(());
        }
        self.start_scope_clock_with_budget(task.id.to_string(), &task.budget)
            .await;
        self.start_scope_clock_with_budget(task.mission_id.to_string(), &mission.budget)
            .await;
        self.start_scope_clock_with_budget(agent.id.to_string(), &agent.budget)
            .await;
        let Some(cwd) = task.worktree.clone() else {
            self.scheduler.lock().await.finish(task.id);
            return Err(OrchestratorError::Validation(
                "running task has no worktree".into(),
            ));
        };
        if cwd.trim().is_empty() {
            self.scheduler.lock().await.finish(task.id);
            return Err(OrchestratorError::Validation(
                "running task worktree is unavailable".into(),
            ));
        }
        let workflow = GitWorkflow::new(
            snapshot
                .projects
                .iter()
                .find(|project| project.id == mission.project_id && !project.archived)
                .cloned()
                .ok_or(OrchestratorError::NotFound)?,
            snapshot
                .server
                .allowed_project_roots
                .iter()
                .map(PathBuf::from)
                .collect(),
        )
        .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
        let _mission_plan = workflow.branch_plan(&mission.branch, &workflow.project.base_branch);
        let _task_plan = workflow.task_plan_from_base(task.id, &mission.branch);
        // Worktree creation is a durable operation committed together with
        // the Running transition. The reconciler performs the filesystem
        // effect before this method is called. Refuse to launch a provider
        // while that operation is still queued/recovering; doing the Git
        // mutation here would reintroduce a crash window with no journal row.
        let worktree_ready = snapshot.operations.iter().any(|operation| {
            operation.kind == OperationKind::CreateWorktree
                && worktree_operation_matches_task(operation, task.id)
                && operation.status == OperationStatus::Succeeded
        });
        if !worktree_ready {
            self.scheduler.lock().await.finish(task.id);
            return Ok(());
        }
        if !Path::new(&cwd).is_dir() {
            self.scheduler.lock().await.finish(task.id);
            return Err(OrchestratorError::Validation(
                "running task worktree is unavailable".into(),
            ));
        }
        let session_capability = self.issue_agent_capability(agent.id, task.id).await;
        let server_url = local_server_url(&snapshot.server);
        let request = StartRequest {
            agent_id: agent.id.to_string(),
            task_id: Some(task.id.to_string()),
            cwd,
            instructions: format!(
                "{}\n\nAgent profile:\n{}",
                task.objective, agent.instructions
            ),
            policy: agent.policy.clone(),
            model: agent.model.clone(),
            resume_session_id: agent.provider_session_id.clone(),
            server_url,
            server_certificate_fingerprint: (!snapshot.server.tls_fingerprint.is_empty())
                .then(|| snapshot.server.tls_fingerprint.clone()),
            session_capability: Some(session_capability.clone()),
            initial_transcript: provider_transcript(
                self.store
                    .provider_session_items(
                        agent.provider_session_id.as_deref().unwrap_or_default(),
                    )
                    .await?,
            ),
        };
        let session = match if request.resume_session_id.is_some() {
            self.runtime
                .resume(
                    request.clone(),
                    request.resume_session_id.as_deref().unwrap(),
                )
                .await
        } else {
            self.runtime.start(request.clone()).await
        } {
            Ok(session) => session,
            Err(error) => {
                self.revoke_agent_capability(&session_capability).await;
                self.scheduler.lock().await.finish(task.id);
                self.mark_task_failure(task.id, agent.id, error.to_string())
                    .await?;
                return Err(OrchestratorError::ProviderUnavailable(error.to_string()));
            }
        };
        let session = Arc::new(session);
        let mut events = match session.events().await {
            Ok(events) => events,
            Err(error) => {
                self.revoke_agent_capability(&session_capability).await;
                self.scheduler.lock().await.finish(task.id);
                self.mark_task_failure(task.id, agent.id, error.to_string())
                    .await?;
                return Err(OrchestratorError::ProviderUnavailable(error.to_string()));
            }
        };
        self.sessions.lock().await.insert(task.id, session.clone());
        self.set_agent(agent.id, AgentStatus::Starting, None)
            .await?;
        let send_initial_turn = request.resume_session_id.is_none();
        if send_initial_turn {
            let provider_session_id = session
                .provider_session_id
                .lock()
                .await
                .clone()
                .ok_or_else(|| {
                    OrchestratorError::ProviderUnavailable(
                        "OpenRouter session did not expose a Frank session id".into(),
                    )
                })?;
            self.store
                .append_provider_session_item(
                    &provider_session_id,
                    &format!("user-{}", task.id),
                    &serde_json::json!({
                        "message": {"role": "user", "content": request.instructions.clone()}
                    }),
                )
                .await?;
        }
        if !send_initial_turn {
            let provider_session_id = session
                .provider_session_id
                .lock()
                .await
                .clone()
                .ok_or_else(|| {
                    OrchestratorError::ProviderUnavailable(
                        "OpenRouter resumed session did not expose a Frank session id".into(),
                    )
                })?;
            let continuation = "Frank daemon resumed this task from its durable transcript and current worktree. Continue from the persisted state; do not repeat completed side effects. Treat any interrupted tool call as incomplete and request approval again when policy requires it.";
            // The item id is stable for the session, while the message is
            // sent once per new RuntimeSession. This permits a retry if the
            // daemon died after persisting but before the HTTP request landed.
            self.store
                .append_provider_session_item(
                    &provider_session_id,
                    &format!("resume-{}", task.id),
                    &serde_json::json!({
                        "message": {"role": "user", "content": continuation}
                    }),
                )
                .await?;
            if let Err(error) = session
                .send(&ProviderMessage {
                    role: "user".into(),
                    content: continuation.into(),
                    correlation_id: Some(format!("resume-{}", task.id)),
                })
                .await
            {
                self.revoke_agent_capability(&session_capability).await;
                self.sessions.lock().await.remove(&task.id);
                self.scheduler.lock().await.finish(task.id);
                self.mark_task_failure(task.id, agent.id, error.to_string())
                    .await?;
                return Err(OrchestratorError::ProviderUnavailable(error.to_string()));
            }
        }
        if send_initial_turn
            && let Err(error) = session
                .send(&ProviderMessage {
                    role: "user".into(),
                    // Preserve persona/profile guidance from the Add Agent
                    // wizard; sending only the objective silently discarded it.
                    content: request.instructions.clone(),
                    correlation_id: Some(task.id.to_string()),
                })
                .await
        {
            self.revoke_agent_capability(&session_capability).await;
            self.sessions.lock().await.remove(&task.id);
            self.scheduler.lock().await.finish(task.id);
            self.mark_task_failure(task.id, agent.id, error.to_string())
                .await?;
            return Err(OrchestratorError::ProviderUnavailable(error.to_string()));
        }

        let orchestrator = self.clone();
        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                if let Err(error) = orchestrator
                    .handle_runtime_event(task.id, agent.id, mission.id, event)
                    .await
                {
                    let message = error.to_string();
                    if let Err(cleanup_error) = orchestrator
                        .fail_runtime_task(task.id, agent.id, message.clone())
                        .await
                    {
                        eprintln!(
                            "frank-orchestrator: runtime event failed for task {}: {}; cleanup failed: {}",
                            task.id, message, cleanup_error
                        );
                    }
                    break;
                }
            }
        });
        Ok(())
    }

    pub(crate) async fn handle_runtime_event(
        &self,
        task_id: TaskId,
        agent_id: AgentId,
        mission_id: MissionId,
        event: RuntimeEvent,
    ) -> Result<()> {
        // A killed provider can still have one buffered frame in the reader
        // task. Once the daemon removed the live session, that stale frame
        // must not resurrect an agent or append usage to a retried/blocked
        // task.
        if !self.sessions.lock().await.contains_key(&task_id) {
            return Ok(());
        }
        match event {
            RuntimeEvent::Ready {
                provider_session_id,
            } => {
                self.set_agent(agent_id, AgentStatus::Working, Some(provider_session_id))
                    .await?
            }
            RuntimeEvent::Text { text } => {
                let body = text
                    .chars()
                    .take(self.max_message_bytes)
                    .collect::<String>();
                if !body.trim().is_empty() {
                    let _ = self
                        .execute(
                            CommandEnvelope {
                                protocol_version: PROTOCOL_VERSION,
                                command_id: CommandId::new(),
                                expected_revision: None,
                                command: Command::SendMessage(MessageSpec {
                                    message_id: None,
                                    mission_id,
                                    task_id: Some(task_id),
                                    recipient: ActorRef::supervisor(),
                                    act: MessageAct::Inform,
                                    body,
                                    artifact_ids: Vec::new(),
                                    reply_to: None,
                                    hop: 0,
                                }),
                            },
                            ActorRef {
                                kind: ActorKind::Agent,
                                id: Some(agent_id.to_string()),
                                display_name: None,
                            },
                            DeviceRole::Operator,
                        )
                        .await;
                }
            }
            RuntimeEvent::ApprovalRequest {
                operation,
                reason,
                cwd,
            } => {
                let response = self
                    .execute(
                        CommandEnvelope {
                            protocol_version: PROTOCOL_VERSION,
                            command_id: CommandId::new(),
                            expected_revision: None,
                            command: Command::RequestApproval(ApprovalSpec {
                                agent_id,
                                task_id,
                                operation,
                                cwd: cwd.unwrap_or_default(),
                                project: format!("mission:{mission_id}"),
                                reason,
                            }),
                        },
                        ActorRef {
                            kind: ActorKind::Agent,
                            id: Some(agent_id.to_string()),
                            display_name: None,
                        },
                        DeviceRole::Operator,
                    )
                    .await;
                if response.error.is_some()
                    || !matches!(response.result, Some(CommandResult::Created { .. }))
                {
                    let message = response
                        .error
                        .map(|error| error.message)
                        .unwrap_or_else(|| {
                            "Frank did not durably create the approval request".into()
                        });
                    self.fail_runtime_task(
                        task_id,
                        agent_id,
                        format!("approval request failed: {message}"),
                    )
                    .await?;
                    return Ok(());
                }
                self.set_agent(agent_id, AgentStatus::NeedsApproval, None)
                    .await?;
            }
            RuntimeEvent::Usage(usage) => {
                // Usage is recorded once under the task scope, while the
                // in-memory ledger attributes the same measured telemetry to
                // its mission and agent parents. This keeps the GUI ledger
                // from double-counting rows and still enforces all limits.
                let model = self
                    .store
                    .snapshot()
                    .await?
                    .agents
                    .into_iter()
                    .find(|agent| agent.id == agent_id)
                    .and_then(|agent| agent.effective_model.or(agent.model));
                let usage_view = UsageView {
                    id: AttemptId::new(),
                    scope: BudgetScope::Task,
                    scope_id: task_id.to_string(),
                    provider: UsageProviderId::openrouter(),
                    model,
                    measured_input_tokens: usage.measured_input_tokens,
                    measured_output_tokens: usage.measured_output_tokens,
                    estimated_input_tokens: usage.estimated_input_tokens,
                    estimated_output_tokens: usage.estimated_output_tokens,
                    cost_micros: usage.cost_micros,
                    cached_input_tokens: usage.cached_input_tokens,
                    reasoning_tokens: usage.reasoning_tokens,
                    recorded_at: timestamp_now(),
                };
                let budget_result = self
                    .record_runtime_usage(usage_view, mission_id, agent_id)
                    .await;
                let exceeded_scope = match budget_result {
                    Ok(scope) => scope,
                    // A failed usage write must fail closed: stopping the
                    // provider is safer than allowing work to continue with
                    // an unknown budget projection. Treat it as task scoped
                    // because that is the smallest scope we can safely stop.
                    Err(_) => Some(BudgetScope::Task),
                };
                if let Some(scope) = exceeded_scope {
                    self.clear_pending_tool_approvals(task_id).await;
                    let session = { self.sessions.lock().await.remove(&task_id) };
                    if let Some(session) = session {
                        let _ = session.graceful_stop().await;
                    }
                    self.scheduler.lock().await.finish(task_id);
                    if let Some(capability) = self
                        .agent_capabilities
                        .lock()
                        .await
                        .iter()
                        .find(|(_, capability)| capability.task_id == task_id)
                        .map(|(token, _)| token.clone())
                    {
                        self.revoke_agent_capability(&capability).await;
                    }
                    let _ = self.transition_task(task_id, TaskStatus::Blocked).await;
                    // Mission budgets apply across all child tasks. Pausing
                    // the mission as well as blocking the current task keeps
                    // the scheduler from immediately starting another worker
                    // and spending past the same hard limit. Task/agent limits
                    // only stop the affected attempt.
                    if scope == BudgetScope::Mission {
                        let _ = self
                            .execute(
                                CommandEnvelope {
                                    protocol_version: PROTOCOL_VERSION,
                                    command_id: CommandId::new(),
                                    expected_revision: None,
                                    command: Command::PauseMission { mission_id },
                                },
                                ActorRef::system(),
                                DeviceRole::Owner,
                            )
                            .await;
                    }
                    self.clear_agent_session(agent_id, AgentStatus::Paused)
                        .await?;
                }
            }
            RuntimeEvent::Stopped { code } => {
                // Error/budget paths remove the live session before killing
                // the child. Its reader may still emit EOF/Stopped; ignore
                // that stale terminal event so it cannot overwrite a retry or
                // a budget-blocked task.
                if code.is_some_and(|code| code != 0) {
                    self.mark_task_failure(
                        task_id,
                        agent_id,
                        format!("provider exited with status {code:?}"),
                    )
                    .await?;
                    return Ok(());
                }
                self.clear_pending_tool_approvals(task_id).await;
                self.scheduler.lock().await.finish(task_id);
                let _ = self.transition_task(task_id, TaskStatus::Review).await;
                self.clear_agent_session(agent_id, AgentStatus::Idle)
                    .await?;
                self.sessions.lock().await.remove(&task_id);
                if let Some(capability) = self
                    .agent_capabilities
                    .lock()
                    .await
                    .iter()
                    .find(|(_, capability)| capability.task_id == task_id)
                    .map(|(token, _)| token.clone())
                {
                    self.revoke_agent_capability(&capability).await;
                }
            }
            RuntimeEvent::Error { message } => {
                self.fail_runtime_task(task_id, agent_id, message).await?;
            }
            RuntimeEvent::ToolCall {
                call_id,
                name,
                input,
            } => {
                self.handle_openrouter_tool_call(task_id, agent_id, call_id, name, input)
                    .await?;
            }
            RuntimeEvent::AssistantMessage {
                turn_id,
                content,
                tool_calls,
            } => {
                if let Some(session) = self.sessions.lock().await.get(&task_id).cloned()
                    && let Some(session_id) = session.provider_session_id.lock().await.clone()
                {
                    let persisted_tool_calls = tool_calls
                        .iter()
                        .map(|call| {
                            serde_json::json!({
                                "id": call.call_id,
                                "type": "function",
                                "function": {
                                    "name": call.name,
                                    "arguments": serde_json::to_string(&call.input)
                                        .unwrap_or_else(|_| "{}".into()),
                                },
                            })
                        })
                        .collect::<Vec<_>>();
                    let message = serde_json::json!({
                        "role": "assistant",
                        "content": content,
                        "tool_calls": persisted_tool_calls,
                    });
                    self.store
                        .append_provider_session_item(
                            &session_id,
                            &format!("assistant-{turn_id}"),
                            &serde_json::json!({"message": message}),
                        )
                        .await?;
                }
            }
            RuntimeEvent::Raw(_) => {}
        }
        Ok(())
    }

    pub(crate) async fn set_agent(
        &self,
        agent_id: AgentId,
        status: AgentStatus,
        provider_session_id: Option<String>,
    ) -> Result<()> {
        for _ in 0..4 {
            let mut snapshot = self.store.snapshot().await?;
            let expected_revision = snapshot.revision;
            let agent = snapshot
                .agents
                .iter_mut()
                .find(|agent| agent.id == agent_id)
                .ok_or(OrchestratorError::NotFound)?;
            agent.status = status;
            if provider_session_id.is_some() {
                agent.provider_session_id = provider_session_id.clone();
            }
            // Role templates are frozen for a live provider session. The
            // transition to Idle/Offline is the safe boundary at which the
            // latest role revision becomes effective.
            let applies_pending_override =
                if matches!(status, AgentStatus::Idle | AgentStatus::Offline) {
                    agent
                        .pending_model_override
                        .take()
                        .map(|model_override| {
                            agent.model_override = model_override;
                            agent.pending_model_change = false;
                            true
                        })
                        .unwrap_or(false)
                } else {
                    false
                };
            if matches!(status, AgentStatus::Idle | AgentStatus::Offline)
                && let Some(role_id) = agent.role_id
                && let Some(role) = snapshot
                    .roles
                    .iter()
                    .find(|role| role.id == role_id && !role.archived)
                    .cloned()
                && (role.revision > agent.role_revision || applies_pending_override)
            {
                materialize_role(agent, &role);
            } else if applies_pending_override {
                agent.model = agent.model_override.clone();
            }
            if applies_pending_override {
                agent.effective_model = agent.model.clone();
                agent.model_source = if agent.model_override.is_some() {
                    ModelSource::Agent
                } else {
                    ModelSource::Role
                };
            }
            let agent = agent.clone();
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(expected_revision),
                    ActorRef::system(),
                    Event::AgentUpserted { agent },
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

    /// Clear a completed or failed provider session from the persistent
    /// profile. Otherwise the next task assigned to the same agent would try
    /// to resume an unrelated provider conversation.
    pub(crate) async fn clear_agent_session(
        &self,
        agent_id: AgentId,
        status: AgentStatus,
    ) -> Result<()> {
        for _ in 0..4 {
            let mut snapshot = self.store.snapshot().await?;
            let expected_revision = snapshot.revision;
            let agent = snapshot
                .agents
                .iter_mut()
                .find(|agent| agent.id == agent_id)
                .ok_or(OrchestratorError::NotFound)?;
            agent.status = status;
            agent.provider_session_id = None;
            // Clearing a provider session is also an idle boundary when the
            // worker becomes reusable. Apply the latest role revision here so
            // the next claim never observes a stale materialized template.
            let applies_pending_override = if status == AgentStatus::Idle {
                agent
                    .pending_model_override
                    .take()
                    .map(|model_override| {
                        agent.model_override = model_override;
                        agent.pending_model_change = false;
                        true
                    })
                    .unwrap_or(false)
            } else {
                false
            };
            if status == AgentStatus::Idle
                && let Some(role_id) = agent.role_id
                && let Some(role) = snapshot
                    .roles
                    .iter()
                    .find(|role| role.id == role_id && !role.archived)
                    .cloned()
                && (role.revision > agent.role_revision || applies_pending_override)
            {
                materialize_role(agent, &role);
            } else if applies_pending_override {
                agent.model = agent.model_override.clone();
            }
            if applies_pending_override {
                agent.effective_model = agent.model.clone();
                agent.model_source = if agent.model_override.is_some() {
                    ModelSource::Agent
                } else {
                    ModelSource::Role
                };
            }
            let agent = agent.clone();
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(expected_revision),
                    ActorRef::system(),
                    Event::AgentUpserted { agent },
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

    pub(crate) async fn transition_task(&self, task_id: TaskId, status: TaskStatus) -> Result<()> {
        let response = self
            .execute(
                CommandEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    command_id: CommandId::new(),
                    expected_revision: None,
                    command: Command::SetTaskStatus { task_id, status },
                },
                ActorRef::system(),
                DeviceRole::Owner,
            )
            .await;
        if let Some(error) = response.error {
            return Err(OrchestratorError::Validation(error.message));
        }
        Ok(())
    }

    pub(crate) async fn block_mission(&self, mission_id: MissionId, reason: String) -> Result<()> {
        for _ in 0..4 {
            let mut snapshot = self.store.snapshot().await?;
            let expected_revision = snapshot.revision;
            let mission = snapshot
                .missions
                .iter_mut()
                .find(|mission| mission.id == mission_id)
                .ok_or(OrchestratorError::NotFound)?;
            if !mission.status.can_transition_to(MissionStatus::Blocked) {
                return Ok(());
            }
            mission.status = MissionStatus::Blocked;
            mission.updated_at = timestamp_now();
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(expected_revision),
                    ActorRef::system(),
                    Event::MissionBlocked {
                        mission_id,
                        reason: reason.clone(),
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

    pub(crate) async fn mark_task_failure(
        &self,
        task_id: TaskId,
        agent_id: AgentId,
        message: String,
    ) -> Result<()> {
        self.clear_pending_tool_approvals(task_id).await;
        self.scheduler.lock().await.finish(task_id);
        let status = 'commit: {
            for _ in 0..4 {
                let mut snapshot = self.store.snapshot().await?;
                let expected_revision = snapshot.revision;
                let task = snapshot
                    .tasks
                    .iter_mut()
                    .find(|task| task.id == task_id)
                    .ok_or(OrchestratorError::NotFound)?;
                let status = retry_after_failure(task);
                task.status = status;
                let task_value = task.clone();
                let actor = ActorRef::system();
                crate::reduce::append_task_feed(
                    &mut snapshot,
                    task_id,
                    &actor,
                    TaskFeedKind::StatusChanged,
                    format!("Worker attempt failed; status changed to {:?}", status),
                    Vec::new(),
                );
                crate::reduce::update_dependency_locks(&mut snapshot, task_id, &actor, status);
                match self
                    .store
                    .commit_command(
                        CommandId::new(),
                        Some(expected_revision),
                        actor,
                        Event::TaskUpdated { task: task_value },
                        snapshot,
                        CommandResult::Accepted,
                    )
                    .await
                {
                    Ok(_) => break 'commit status,
                    Err(StoreError::StaleRevision { .. }) => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            return Err(OrchestratorError::Store(StoreError::StaleRevision {
                current: self.store.current_revision().await?,
            }));
        };
        // Keep a worker reusable when the retry policy returns the task to
        // Ready. Once attempts are exhausted, surface Failed and leave the
        // task Blocked for explicit intervention.
        let agent_status = if status == TaskStatus::Blocked {
            AgentStatus::Failed
        } else {
            AgentStatus::Idle
        };
        let _ = self.clear_agent_session(agent_id, agent_status).await;
        let _ = self
            .execute(
                CommandEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    command_id: CommandId::new(),
                    expected_revision: None,
                    command: Command::SendMessage(MessageSpec {
                        message_id: None,
                        mission_id: self
                            .store
                            .snapshot()
                            .await?
                            .tasks
                            .iter()
                            .find(|task| task.id == task_id)
                            .map(|task| task.mission_id)
                            .unwrap_or(MissionId::nil()),
                        task_id: Some(task_id),
                        recipient: ActorRef::supervisor(),
                        act: MessageAct::Inform,
                        body: format!("worker failed: {message}"),
                        artifact_ids: Vec::new(),
                        reply_to: None,
                        hop: 0,
                    }),
                },
                ActorRef::system(),
                DeviceRole::Owner,
            )
            .await;
        self.sessions.lock().await.remove(&task_id);
        let capability = self
            .agent_capabilities
            .lock()
            .await
            .iter()
            .find(|(_, capability)| capability.task_id == task_id)
            .map(|(token, _)| token.clone());
        if let Some(capability) = capability {
            self.revoke_agent_capability(&capability).await;
        }
        Ok(())
    }

    pub(crate) async fn clear_pending_tool_approvals(&self, task_id: TaskId) {
        self.tool_approvals
            .lock()
            .await
            .retain(|_, pending| pending.task_id != task_id);
    }

    pub(crate) async fn fail_runtime_task(
        &self,
        task_id: TaskId,
        agent_id: AgentId,
        message: String,
    ) -> Result<()> {
        let session = self.sessions.lock().await.remove(&task_id);
        let failure = self.mark_task_failure(task_id, agent_id, message).await;
        if let Some(session) = session {
            let _ = session.graceful_stop().await;
        }
        failure
    }

    pub(crate) async fn error_response(
        &self,
        command_id: CommandId,
        error: OrchestratorError,
    ) -> CommandResponse {
        let revision = self.store.current_revision().await.unwrap_or_default();
        let mut api = match &error {
            OrchestratorError::Forbidden => {
                ApiError::new(ErrorCode::Forbidden, "permission denied")
            }
            OrchestratorError::NotFound => ApiError::new(ErrorCode::NotFound, "resource not found"),
            OrchestratorError::InvalidTransition(message) => {
                ApiError::new(ErrorCode::Conflict, message)
            }
            OrchestratorError::Validation(message) => ApiError::new(ErrorCode::Validation, message),
            OrchestratorError::OrganizationRevisionConflict { expected, actual } => {
                let mut api = ApiError::new(
                    ErrorCode::OrganizationRevisionConflict,
                    format!(
                        "Organization revision changed elsewhere (expected {expected}, actual {actual})"
                    ),
                );
                api.retryable = true;
                api.latest_snapshot = self.store.snapshot().await.ok().map(Box::new);
                api
            }
            OrchestratorError::BudgetExceeded => ApiError::new(
                ErrorCode::BudgetExceeded,
                "measured budget exceeded; adjust the budget or cancel the work",
            ),
            OrchestratorError::ProviderUnavailable(message) => {
                ApiError::new(ErrorCode::ProviderUnavailable, message)
            }
            OrchestratorError::Store(StoreError::StaleRevision { .. }) => {
                match self.store.snapshot().await {
                    Ok(snapshot) => ApiError::conflict(snapshot),
                    Err(_) => ApiError::new(ErrorCode::StaleRevision, "stale revision"),
                }
            }
            OrchestratorError::Store(_) => ApiError::new(ErrorCode::Internal, "persistence failed"),
        };
        if matches!(api.code, ErrorCode::Conflict | ErrorCode::StaleRevision) {
            api.retryable = true;
        }
        CommandResponse::failed(command_id, revision, api)
    }
}
