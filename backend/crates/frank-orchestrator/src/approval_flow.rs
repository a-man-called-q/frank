//! Orchestrator approval flow flow.

use super::*;

impl Orchestrator {
    pub(crate) async fn respond_to_provider_approval(
        &self,
        approval_id: ApprovalId,
        decision: ApprovalDecision,
    ) {
        let Ok(snapshot) = self.store.snapshot().await else {
            return;
        };
        let Some(approval) = snapshot
            .approvals
            .iter()
            .find(|approval| approval.id == approval_id)
            .cloned()
        else {
            return;
        };
        let pending = self.tool_approvals.lock().await.remove(&approval_id);
        if let Some(pending) = pending {
            let allowed = matches!(
                (approval.status, decision),
                (ApprovalStatus::Approved, ApprovalDecision::AllowOnce)
            );
            let session = self.sessions.lock().await.get(&pending.task_id).cloned();
            if let Some(session) = session {
                let workspace_root = session.request().cwd.clone();
                let output = if allowed {
                    self.execute_openrouter_tool(
                        pending.task_id,
                        pending.agent_id,
                        &workspace_root,
                        &pending.name,
                        &pending.input,
                    )
                    .await
                } else {
                    serde_json::json!({
                        "ok": false,
                        "error": match approval.status {
                            ApprovalStatus::Expired => "Frank approval expired",
                            ApprovalStatus::Denied => "Frank denied this operation",
                            _ => "Frank approval was not granted",
                        }
                    })
                };
                let _ = self
                    .deliver_openrouter_tool_result(
                        pending.task_id,
                        &session,
                        &pending.call_id,
                        output,
                    )
                    .await;
                let has_pending_for_task = self
                    .tool_approvals
                    .lock()
                    .await
                    .values()
                    .any(|entry| entry.task_id == pending.task_id);
                let _ = self
                    .set_agent(
                        pending.agent_id,
                        if has_pending_for_task {
                            AgentStatus::NeedsApproval
                        } else {
                            AgentStatus::Working
                        },
                        None,
                    )
                    .await;
            }
            return;
        }
        if !matches!(
            (approval.status, decision),
            (ApprovalStatus::Approved, ApprovalDecision::AllowOnce)
                | (ApprovalStatus::Denied, ApprovalDecision::DenyOnce)
        ) {
            return;
        }
        let session = self.sessions.lock().await.get(&approval.task_id).cloned();
        if let Some(session) = session {
            let _ = session
                .respond_to_approval(&approval.operation, decision)
                .await;
        }
    }

    pub(crate) async fn handle_openrouter_tool_call(
        &self,
        task_id: TaskId,
        agent_id: AgentId,
        call_id: String,
        name: String,
        input: Value,
    ) -> Result<()> {
        if call_id.trim().is_empty() {
            return Err(OrchestratorError::Validation(
                "OpenRouter tool call is missing its id".into(),
            ));
        }
        let session = self
            .sessions
            .lock()
            .await
            .get(&task_id)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        let workspace_root = session.request().cwd.clone();
        let snapshot = self.store.snapshot().await?;
        let agent = snapshot
            .agents
            .iter()
            .find(|agent| agent.id == agent_id && !agent.archived)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        let session_id = session
            .provider_session_id
            .lock()
            .await
            .clone()
            .ok_or_else(|| {
                OrchestratorError::ProviderUnavailable(
                    "OpenRouter session id is unavailable".into(),
                )
            })?;
        if let Some(reason) = organization_tool_denial(&snapshot, agent_id, &name) {
            self.deliver_openrouter_tool_result(
                task_id,
                &session,
                &call_id,
                serde_json::json!({"ok": false, "error": reason}),
            )
            .await?;
            return Ok(());
        }
        let request_item_id = format!("tool-{call_id}-request");
        let result_item_id = format!("tool-{call_id}");
        let transcript = self.store.provider_session_items(&session_id).await?;
        if let Some(content) = transcript
            .iter()
            .find(|item| item.item_id == result_item_id)
            .and_then(|item| item.value.get("tool_result"))
            .and_then(|result| result.get("content"))
            .and_then(Value::as_str)
        {
            // A duplicated provider event must replay the durable result, not
            // execute the side effect a second time.
            session
                .submit_tool_result(&call_id, content)
                .await
                .map_err(|error| OrchestratorError::ProviderUnavailable(error.to_string()))?;
            return Ok(());
        }
        if transcript
            .iter()
            .any(|item| item.item_id == request_item_id)
        {
            // A request marker without a result means the previous handler
            // stopped between the marker and the effect. Fail closed: the
            // model can decide whether to issue a fresh, separately approved
            // call, but Frank never guesses that the effect is safe to repeat.
            self.deliver_openrouter_tool_result(
                task_id,
                &session,
                &call_id,
                serde_json::json!({
                    "ok": false,
                    "error": "OpenRouter tool call was interrupted before its result was durable"
                }),
            )
            .await?;
            return Ok(());
        }
        // The request marker is written before any side effect. On a daemon
        // crash, provider_transcript turns an unfinished marker into a
        // deterministic tool error instead of executing the same call twice.
        self.store
            .append_provider_session_item(
                &session_id,
                &request_item_id,
                &serde_json::json!({
                    "tool_call": {
                        "call_id": call_id,
                        "name": name,
                        "input": input,
                        "status": "pending",
                    }
                }),
            )
            .await?;
        if let Some(reason) = openrouter_tools::tool_requires_approval(&agent.policy, &name, &input)
        {
            let denied = reason.starts_with("denied:")
                || (agent.policy.approval != ApprovalPolicy::Ask
                    && openrouter_tools::tool_is_write(&name)
                    && name == "shell_exec"
                    && agent.policy.shell == ShellPolicy::Ask);
            if denied {
                self.deliver_openrouter_tool_result(
                    task_id,
                    &session,
                    &call_id,
                    serde_json::json!({"ok": false, "error": reason}),
                )
                .await?;
                return Ok(());
            }
            let response = self
                .execute(
                    CommandEnvelope {
                        protocol_version: PROTOCOL_VERSION,
                        command_id: CommandId::new(),
                        expected_revision: None,
                        command: Command::RequestApproval(ApprovalSpec {
                            agent_id,
                            task_id,
                            operation: format!("openrouter-tool:{name}:{call_id}")
                                .chars()
                                .take(4_096)
                                .collect(),
                            cwd: workspace_root,
                            project: format!(
                                "mission:{}",
                                snapshot
                                    .tasks
                                    .iter()
                                    .find(|task| task.id == task_id)
                                    .map(|task| task.mission_id)
                                    .unwrap_or_default()
                            ),
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
            if let Some(CommandResult::Created { id }) = response.result {
                let approval_id = ApprovalId::parse(&id)
                    .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
                self.tool_approvals.lock().await.insert(
                    approval_id,
                    PendingToolCall {
                        task_id,
                        agent_id,
                        call_id,
                        name,
                        input,
                    },
                );
                self.set_agent(agent_id, AgentStatus::NeedsApproval, None)
                    .await?;
            } else {
                let message = response
                    .error
                    .map(|error| error.message)
                    .unwrap_or_else(|| "Frank could not create an approval request".into());
                self.deliver_openrouter_tool_result(
                    task_id,
                    &session,
                    &call_id,
                    serde_json::json!({"ok": false, "error": message}),
                )
                .await?;
            }
            return Ok(());
        }
        let output = self
            .execute_openrouter_tool(task_id, agent_id, &workspace_root, &name, &input)
            .await;
        self.deliver_openrouter_tool_result(task_id, &session, &call_id, output)
            .await
    }

    pub(crate) async fn deliver_openrouter_tool_result(
        &self,
        task_id: TaskId,
        session: &frank_agent::RuntimeSession,
        call_id: &str,
        output: Value,
    ) -> Result<()> {
        let mut encoded = serde_json::to_string(&output)
            .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
        if encoded.len() > MAX_COMMAND_BODY_BYTES {
            encoded = serde_json::to_string(&serde_json::json!({
                "ok": false,
                "error": "tool result exceeded Frank's output cap"
            }))
            .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
        }
        let session_id = session
            .provider_session_id
            .lock()
            .await
            .clone()
            .ok_or_else(|| {
                OrchestratorError::ProviderUnavailable(
                    "OpenRouter session id is unavailable".into(),
                )
            })?;
        self.store
            .append_provider_session_item(
                &session_id,
                &format!("tool-{call_id}"),
                &serde_json::json!({
                    "tool_result": {"call_id": call_id, "content": encoded}
                }),
            )
            .await?;
        session
            .submit_tool_result(call_id, &encoded)
            .await
            .map_err(|error| OrchestratorError::ProviderUnavailable(error.to_string()))?;
        let _ = task_id;
        Ok(())
    }

    /// Expire pending approvals as a daemon-owned state transition. Provider
    /// sessions cannot be allowed to keep waiting forever on a stale prompt;
    /// expiry blocks the affected task and tears down its scoped runtime so a
    /// later reconcile cannot accidentally continue work without a decision.
    pub(crate) async fn expire_approvals(&self, snapshot: &Snapshot) -> Result<()> {
        let now = epoch_seconds() as u128;
        let expired = snapshot
            .approvals
            .iter()
            .filter(|approval| {
                approval.status == ApprovalStatus::Pending
                    && approval
                        .expires_at
                        .parse::<u128>()
                        .is_ok_and(|expires_at| expires_at <= now)
            })
            .map(|approval| (approval.id, approval.task_id))
            .collect::<Vec<_>>();
        for (approval_id, task_id) in expired {
            let mut latest = self.store.snapshot().await?;
            let Some(approval) = latest
                .approvals
                .iter_mut()
                .find(|approval| approval.id == approval_id)
            else {
                continue;
            };
            if approval.status != ApprovalStatus::Pending {
                continue;
            }
            let Ok(expires_at) = approval.expires_at.parse::<u128>() else {
                continue;
            };
            if expires_at > now {
                continue;
            }
            approval.status = ApprovalStatus::Expired;
            self.store
                .commit_command(
                    CommandId::new(),
                    Some(latest.revision),
                    ActorRef::system(),
                    Event::ApprovalExpired { approval_id },
                    latest,
                    CommandResult::Accepted,
                )
                .await?;
            self.tool_approvals.lock().await.remove(&approval_id);

            let latest = self.store.snapshot().await?;
            let Some(task) = latest.tasks.iter().find(|task| task.id == task_id).cloned() else {
                continue;
            };
            if !matches!(task.status, TaskStatus::Running | TaskStatus::Ready) {
                continue;
            }
            if let Some(agent_id) = task.assigned_agent {
                if let Some(session) = self.sessions.lock().await.remove(&task_id) {
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
                let _ = self
                    .clear_agent_session(agent_id, AgentStatus::Paused)
                    .await;
            }
            let _ = self.transition_task(task_id, TaskStatus::Blocked).await;
        }
        Ok(())
    }
    pub(crate) async fn apply(
        &self,
        envelope: CommandEnvelope,
        actor: ActorRef,
    ) -> Result<CommandResponse> {
        let current = self.store.snapshot().await?;
        let current_revision = current.revision;
        let existing_message_id = match &envelope.command {
            Command::SendMessage(spec) => spec.message_id,
            _ => None,
        }
        .filter(|message_id| {
            current
                .messages
                .iter()
                .any(|message| message.id == *message_id)
        });
        let command_id = envelope.command_id;
        let expected_revision = envelope.expected_revision;
        let artifact_command = match &envelope.command {
            Command::PublishArtifact(spec) => Some(spec.clone()),
            _ => None,
        };
        let upload_bytes = match &envelope.command {
            Command::FinalizeArtifactUpload { upload_id, .. } => self
                .store
                .artifact_upload_bytes(*upload_id)
                .await
                .ok()
                .flatten(),
            _ => None,
        };
        let message_command = matches!(&envelope.command, Command::SendMessage(_));
        let (next, event, result) = self.reduce(current, envelope.command, &actor).await?;
        // A caller may retry a broker send with the same message id but a
        // fresh command id after losing its HTTP response. `reduce` validates
        // that request and returns the already stored message; do not emit a
        // second MessageQueued event or advance the global revision for that
        // logical no-op.
        if let Event::MessageQueued { message } = &event
            && existing_message_id == Some(message.id)
        {
            return Ok(CommandResponse::ok(command_id, current_revision, result));
        }
        let commit = if let Some(spec) = artifact_command.as_ref() {
            self.store
                .commit_request(frank_store::CommitRequest {
                    command_id,
                    expected_revision,
                    actor,
                    event,
                    snapshot: next,
                    result,
                    artifact_bytes: Some(spec.bytes.as_slice()),
                })
                .await?
        } else if let Some(bytes) = upload_bytes.as_deref() {
            self.store
                .commit_request(frank_store::CommitRequest {
                    command_id,
                    expected_revision,
                    actor,
                    event,
                    snapshot: next,
                    result,
                    artifact_bytes: Some(bytes),
                })
                .await?
        } else {
            self.store
                .commit_command(command_id, expected_revision, actor, event, next, result)
                .await?
        };
        if message_command
            && let Some(CommandResult::Created { id }) = commit.response.result.clone()
            && let Ok(message_id) = MessageId::parse(&id)
        {
            // Queueing and delivery are separate durable transitions. The
            // queue event is committed first, then this broker step marks
            // the message delivered without making a lost HTTP response
            // cause a second provider wake-up or duplicate message body.
            let _ = self.deliver_message(message_id).await;
        }
        Ok(commit.response)
    }

    pub(crate) async fn commit_organization_with_retry(
        &self,
        envelope: CommandEnvelope,
        actor: ActorRef,
    ) -> Result<CommandResponse> {
        for _ in 0..4 {
            let snapshot = self.store.snapshot().await?;
            let (next, event, result) = self
                .reduce(snapshot.clone(), envelope.command.clone(), &actor)
                .await?;
            match self
                .store
                .commit_command(
                    envelope.command_id,
                    Some(snapshot.revision),
                    actor.clone(),
                    event,
                    next,
                    result,
                )
                .await
            {
                Ok(commit) => return Ok(commit.response),
                Err(StoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }

    /// Route a command to its domain reducer.
    ///
    /// Dispatch only: matching on a reference keeps `command` intact so the
    /// reducer that handles it receives it by value, exactly as the single
    /// 1902-line match used to.
    pub(crate) async fn reduce(
        &self,
        snapshot: Snapshot,
        command: Command,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        match &command {
            Command::Pair(..) | Command::UpdateSettings { .. } => {
                self.reduce_settings(snapshot, command, actor).await
            }
            Command::CreateProject(..)
            | Command::CloneProject { .. }
            | Command::ArchiveProject { .. } => self.reduce_project(snapshot, command, actor).await,
            Command::CreateAgent(..)
            | Command::UpdateAgent { .. }
            | Command::ArchiveAgent { .. } => self.reduce_agent(snapshot, command, actor).await,
            Command::FireAgent { .. } => self.reduce_workflow(snapshot, command, actor).await,
            Command::CreateRole(..)
            | Command::UpdateRole { .. }
            | Command::ArchiveRole { .. }
            | Command::SetAgentRole { .. } => self.reduce_role(snapshot, command, actor).await,
            Command::CreateTaskboard(..)
            | Command::UpdateTaskboard { .. }
            | Command::ArchiveTaskboard { .. }
            | Command::CreateWorkItem(..)
            | Command::DropWorkItem { .. }
            | Command::SpawnChildWorkItems { .. }
            | Command::CreateWorkOffer { .. }
            | Command::RespondWorkOffer { .. }
            | Command::RequestHumanInput { .. }
            | Command::ResolveHumanInput { .. }
            | Command::RequestTaskRework { .. }
            | Command::RequestOrganizationDrain { .. }
            | Command::CompleteOrganizationDrain { .. }
            | Command::ResumeOrganization { .. }
            | Command::RelocateWorkItems { .. } => {
                self.reduce_workflow(snapshot, command, actor).await
            }
            Command::SaveOrganizationDraft { .. }
            | Command::PublishOrganization { .. }
            | Command::CreateConnectorProfile(..)
            | Command::UpdateConnectorProfile { .. }
            | Command::ArchiveConnectorProfile { .. } => {
                self.reduce_organization(snapshot, command, actor).await
            }
            Command::CreateMission { .. }
            | Command::SetMissionStatus { .. }
            | Command::PauseMission { .. }
            | Command::ResumeMission { .. }
            | Command::DeliverMission { .. }
            | Command::SubmitSupervisorPlan { .. } => {
                self.reduce_mission(snapshot, command, actor).await
            }
            Command::CreateTask(..)
            | Command::UpdateTask { .. }
            | Command::SetTaskStatus { .. }
            | Command::AssignTask { .. }
            | Command::ClaimTask { .. }
            | Command::ReleaseTask { .. }
            | Command::AddTaskComment { .. }
            | Command::TaskAccept { .. }
            | Command::DecideReview { .. } => self.reduce_task(snapshot, command, actor).await,
            Command::SendMessage(..)
            | Command::AckMessage { .. }
            | Command::CompleteMessage { .. } => {
                self.reduce_message(snapshot, command, actor).await
            }
            Command::RequestApproval(..)
            | Command::DecideApproval { .. }
            | Command::AdjustBudget { .. } => self.reduce_approval(snapshot, command, actor).await,
            Command::ProposeMemory { .. } | Command::ReadMemory { .. } => {
                self.reduce_memory(snapshot, command, actor).await
            }
            Command::PublishArtifact(..)
            | Command::BeginArtifactUpload(..)
            | Command::FinalizeArtifactUpload { .. } => {
                self.reduce_artifact(snapshot, command, actor).await
            }
            Command::OpenTerminal { .. }
            | Command::TakeControl { .. }
            | Command::RenewControl { .. }
            | Command::ReleaseControl { .. }
            | Command::CloseTerminal { .. } => self.reduce_terminal(snapshot, command, actor).await,
            Command::CancelOperation { .. } | Command::RetryOperation { .. } => {
                self.reduce_operation(snapshot, command, actor).await
            }
            Command::CheckForUpdate
            | Command::PrepareUpdate { .. }
            | Command::ApplyUpdate { .. }
            | Command::RollbackUpdate => self.reduce_update(snapshot, command, actor).await,
        }
    }

    pub(crate) async fn pause_or_resume(
        &self,
        mut snapshot: Snapshot,
        mission_id: MissionId,
        status: MissionStatus,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        let mission = snapshot
            .missions
            .iter_mut()
            .find(|mission| mission.id == mission_id)
            .ok_or(OrchestratorError::NotFound)?;
        if !mission.status.can_transition_to(status) {
            return Err(OrchestratorError::InvalidTransition(format!(
                "mission cannot transition from {:?} to {:?}",
                mission.status, status
            )));
        }
        mission.status = status;
        mission.updated_at = timestamp_now();
        Ok((
            snapshot,
            Event::MissionStatusChanged { mission_id, status },
            CommandResult::Accepted,
        ))
    }
}
