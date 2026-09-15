//! Orchestrator mission flow flow.

use super::*;

impl Orchestrator {
    pub async fn plan_mission(
        &self,
        mission_id: MissionId,
        objective: &str,
    ) -> Result<Vec<TaskId>> {
        // CreateMission is idempotent at the storage layer, but its
        // deterministic task expansion happens after that commit. A client
        // retry must therefore short-circuit here as well or it would append
        // a second copy of the same DAG after the original response was
        // already durably recorded.
        let existing = self
            .store
            .snapshot()
            .await?
            .tasks
            .into_iter()
            .filter(|task| task.mission_id == mission_id)
            .map(|task| task.id)
            .collect::<Vec<_>>();
        if !existing.is_empty() {
            return Ok(existing);
        }
        let plan = if std::env::var_os("FRANK_DETERMINISTIC_SUPERVISOR").is_some() {
            // This switch exists for deterministic CI/fake-provider runs only.
            // A normal daemon must receive a structured proposal from the
            // selected OpenRouter supervisor.
            supervisor::decompose_objective(mission_id, objective, 32)
        } else {
            self.request_supervisor_plan(mission_id, objective).await?
        };
        self.materialize_supervisor_plan(plan).await
    }

    pub(crate) async fn materialize_supervisor_plan(
        &self,
        plan: supervisor::SupervisorPlan,
    ) -> Result<Vec<TaskId>> {
        let mut ids = Vec::with_capacity(plan.tasks.len());
        let mut root_tasks = Vec::new();
        let mut ids_by_key = HashMap::<String, TaskId>::new();
        for (index, mut spec) in plan.tasks.into_iter().enumerate() {
            let key = plan
                .task_keys
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("task-{index}"));
            let dependency_keys = plan.dependency_keys.get(index).cloned().unwrap_or_default();
            spec.dependencies = dependency_keys
                .iter()
                .map(|dependency| {
                    ids_by_key.get(dependency).copied().ok_or_else(|| {
                        OrchestratorError::Validation(
                            "supervisor plan dependency was not materialized".into(),
                        )
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            let is_root = spec.dependencies.is_empty();
            // Use the public daemon command path so every generated task is
            // durably committed. The old planner called `apply()` directly,
            // which built an in-memory card but never persisted it.
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
            if let Some(CommandResult::Created { id }) = response.result {
                if let Ok(id) = TaskId::parse(&id) {
                    if is_root {
                        root_tasks.push(id);
                    }
                    ids.push(id);
                    ids_by_key.insert(key, id);
                }
            } else if let Some(error) = response.error {
                return Err(match error.code {
                    ErrorCode::NotFound => OrchestratorError::NotFound,
                    _ => OrchestratorError::Validation(error.message),
                });
            }
        }
        // A generated supervisor DAG starts at Ready for dependency-free
        // tasks. The reconciler will assign a worker and move it to Running
        // once its mission is active; manually-created cards remain Backlog.
        for task_id in root_tasks {
            let _ = self
                .commit_reduced(
                    CommandEnvelope {
                        protocol_version: PROTOCOL_VERSION,
                        command_id: CommandId::new(),
                        expected_revision: None,
                        command: Command::SetTaskStatus {
                            task_id,
                            status: TaskStatus::Ready,
                        },
                    },
                    ActorRef::system(),
                )
                .await?;
        }
        Ok(ids)
    }

    /// Commit a reducer result for an internal plan materialization step.
    /// This deliberately does not call `execute`, avoiding recursive async
    /// futures through the CreateMission post-hook.
    pub(crate) async fn commit_reduced(
        &self,
        envelope: CommandEnvelope,
        actor: ActorRef,
    ) -> Result<CommandResponse> {
        let command_id = envelope.command_id;
        // Internal daemon transitions normally omit an expected revision. A
        // concurrent client/event can still advance SQLite between the read
        // and commit, so bind the reduction to the revision we observed and
        // retry a bounded number of times. Reusing the same command id keeps
        // a successful attempt idempotent if the caller is interrupted after
        // commit but before receiving the response.
        for _ in 0..4 {
            let snapshot = self.store.snapshot().await?;
            let expected_revision = envelope.expected_revision.or(Some(snapshot.revision));
            let (next, event, result) = self
                .reduce(snapshot, envelope.command.clone(), &actor)
                .await?;
            match self
                .store
                .commit_command(
                    command_id,
                    expected_revision,
                    actor.clone(),
                    event,
                    next,
                    result,
                )
                .await
            {
                Ok(commit) => return Ok(commit.response),
                Err(StoreError::StaleRevision { .. }) if envelope.expected_revision.is_none() => {
                    continue;
                }
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }

    /// Start the selected provider as a persistent mission supervisor and
    /// require a structured plan proposal.  The daemon still validates the
    /// proposal before it becomes a task DAG; provider text is never treated
    /// as an implicit mutation.
    pub(crate) async fn request_supervisor_plan(
        &self,
        mission_id: MissionId,
        objective: &str,
    ) -> Result<supervisor::SupervisorPlan> {
        let snapshot = self.store.snapshot().await?;
        let mission = snapshot
            .missions
            .iter()
            .find(|mission| mission.id == mission_id)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        let project = snapshot
            .projects
            .iter()
            .find(|project| project.id == mission.project_id && !project.archived)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        let supervisor = snapshot
            .agents
            .iter()
            .find(|agent| agent.display_name == "Frank supervisor" && !agent.archived)
            .cloned()
            .ok_or(OrchestratorError::NotFound)?;
        let request = StartRequest {
            agent_id: supervisor.id.to_string(),
            task_id: None,
            cwd: project.path,
            instructions: supervisor.instructions.clone(),
            policy: supervisor.policy.clone(),
            model: snapshot
                .server
                .supervisor_model
                .clone()
                .or_else(|| supervisor.effective_model.clone())
                .or_else(|| supervisor.model.clone())
                .or_else(|| Some(crate::DEFAULT_LUNA_MODEL.into())),
            resume_session_id: mission.supervisor_session_id.clone(),
            server_url: local_server_url(&snapshot.server),
            server_certificate_fingerprint: (!snapshot.server.tls_fingerprint.is_empty())
                .then(|| snapshot.server.tls_fingerprint.clone()),
            session_capability: None,
            initial_transcript: provider_transcript(
                self.store
                    .provider_session_items(
                        mission.supervisor_session_id.as_deref().unwrap_or_default(),
                    )
                    .await?,
            ),
            reasoning_effort: Some(ReasoningEffort::Max),
        };
        let session = if let Some(session) = self.supervisor_sessions.lock().await.get(&mission_id)
        {
            session.clone()
        } else {
            let session = if request.resume_session_id.is_some() {
                self.runtime
                    .resume(
                        request.clone(),
                        request.resume_session_id.as_deref().unwrap_or_default(),
                    )
                    .await
            } else {
                self.runtime.start(request.clone()).await
            }
            .map_err(|error| OrchestratorError::ProviderUnavailable(error.to_string()))?;
            let session = Arc::new(session);
            self.supervisor_sessions
                .lock()
                .await
                .insert(mission_id, session.clone());
            session
        };
        let mut events = session
            .events()
            .await
            .map_err(|error| OrchestratorError::ProviderUnavailable(error.to_string()))?;
        let prompt = format!(
            "You are Frank's mission supervisor. Decompose this objective into a safe DAG.\n\
             Return ONLY one JSON object matching SupervisorPlanProposal: \
             {{\"mission_id\":\"{mission_id}\",\"summary\":\"...\",\"tasks\":[{{\"client_key\":\"stable-key\",\"title\":\"...\",\"objective\":\"...\",\"dependencies\":[],\"priority\":0,\"candidate_agents\":[],\"assigned_agent\":null,\"policy_requirement\":null,\"budget\":{{\"time_seconds\":null,\"turns\":null,\"measured_tokens\":null,\"cost_micros\":null}}}}]}}.\n\
             Maximum 32 tasks. Use dependency client_key values only. Objective:\n{objective}",
        );
        let session_id = session
            .provider_session_id
            .lock()
            .await
            .clone()
            .ok_or_else(|| {
                OrchestratorError::ProviderUnavailable(
                    "OpenRouter supervisor session id is unavailable".into(),
                )
            })?;
        let transcript_items = self.store.provider_session_items(&session_id).await?;
        let prompt_item_id = format!("supervisor-user-{mission_id}");
        let has_prompt = transcript_items
            .iter()
            .any(|item| item.item_id == prompt_item_id);
        if !has_prompt {
            self.store
                .append_provider_session_item(
                    &session_id,
                    &prompt_item_id,
                    &serde_json::json!({"message": {"role": "user", "content": prompt.clone()}}),
                )
                .await?;
            session
                .send(&ProviderMessage {
                    role: "user".into(),
                    content: prompt,
                    correlation_id: Some(mission_id.to_string()),
                })
                .await
                .map_err(|error| OrchestratorError::ProviderUnavailable(error.to_string()))?;
        } else if request.resume_session_id.is_some()
            && !transcript_items
                .iter()
                .any(|item| item.item_id == format!("supervisor-resume-{mission_id}"))
        {
            // A persisted prompt with no live supervisor session is the
            // daemon-restart case. Chat Completions is stateless, so send a
            // durable continuation once to re-open the planning turn without
            // replaying the original objective or any completed side effect.
            let continuation = "Frank daemon resumed the supervisor from its durable transcript. Continue the pending mission plan from the recorded state; do not duplicate completed work or silently substitute a model.";
            self.store
                .append_provider_session_item(
                    &session_id,
                    &format!("supervisor-resume-{mission_id}"),
                    &serde_json::json!({
                        "message": {"role": "user", "content": continuation}
                    }),
                )
                .await?;
            session
                .send(&ProviderMessage {
                    role: "user".into(),
                    content: continuation.into(),
                    correlation_id: Some(format!("supervisor-resume-{mission_id}")),
                })
                .await
                .map_err(|error| OrchestratorError::ProviderUnavailable(error.to_string()))?;
        }

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(45);
        let mut text = String::new();
        let mut proposal = None;
        while tokio::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let next = tokio::time::timeout(
                remaining.min(std::time::Duration::from_secs(2)),
                events.recv(),
            )
            .await;
            let Some(event) = (match next {
                Ok(event) => event,
                Err(_) => continue,
            }) else {
                break;
            };
            match event {
                RuntimeEvent::Ready {
                    provider_session_id,
                } => {
                    self.persist_supervisor_session_id(mission_id, Some(provider_session_id))
                        .await?;
                }
                RuntimeEvent::Text { text: chunk } => {
                    if text.len().saturating_add(chunk.len()) <= MAX_MESSAGE_BODY_BYTES {
                        text.push_str(&chunk);
                    }
                    if let Some(parsed) = parse_supervisor_json(&text, mission_id) {
                        proposal = Some(parsed);
                        break;
                    }
                }
                RuntimeEvent::ToolCall { input, .. } => {
                    if let Ok(parsed) = serde_json::from_value::<SupervisorPlanProposal>(input)
                        && parsed.mission_id == mission_id
                    {
                        proposal = Some(parsed);
                        break;
                    }
                }
                RuntimeEvent::AssistantMessage {
                    turn_id,
                    content,
                    tool_calls,
                } => {
                    let session_id = session
                        .provider_session_id
                        .lock()
                        .await
                        .clone()
                        .ok_or_else(|| {
                            OrchestratorError::ProviderUnavailable(
                                "OpenRouter supervisor session id is unavailable".into(),
                            )
                        })?;
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
                    self.store
                        .append_provider_session_item(
                            &session_id,
                            &format!("assistant-{turn_id}"),
                            &serde_json::json!({
                                "message": {
                                    "role": "assistant",
                                    "content": content,
                                    "tool_calls": persisted_tool_calls,
                                }
                            }),
                        )
                        .await?;
                }
                RuntimeEvent::Usage(usage) => {
                    self.record_usage(
                        UsageView {
                            id: AttemptId::new(),
                            scope: BudgetScope::Agent,
                            scope_id: supervisor.id.to_string(),
                            provider: UsageProviderId::for_model(request.model.as_deref()),
                            model: request.model.clone(),
                            measured_input_tokens: usage.measured_input_tokens,
                            measured_output_tokens: usage.measured_output_tokens,
                            estimated_input_tokens: usage.estimated_input_tokens,
                            estimated_output_tokens: usage.estimated_output_tokens,
                            cost_micros: usage.cost_micros,
                            cached_input_tokens: usage.cached_input_tokens,
                            reasoning_tokens: usage.reasoning_tokens,
                            recorded_at: timestamp_now(),
                        },
                        Some(&supervisor.budget),
                    )
                    .await?;
                }
                RuntimeEvent::Error { message } => {
                    return Err(OrchestratorError::ProviderUnavailable(message));
                }
                RuntimeEvent::Stopped { code } => {
                    return Err(OrchestratorError::ProviderUnavailable(format!(
                        "supervisor exited before returning a plan ({code:?})"
                    )));
                }
                RuntimeEvent::Raw(_)
                | RuntimeEvent::ApprovalRequest { .. }
                | RuntimeEvent::TurnCompleted { .. } => {}
            }
        }
        let plan = proposal
            .ok_or_else(|| {
                OrchestratorError::ProviderUnavailable(
                    "supervisor did not return a valid structured plan".into(),
                )
            })
            .and_then(|proposal| {
                validate_supervisor_proposal(&snapshot, mission_id, &proposal)?;
                supervisor::proposal_to_plan(proposal).map_err(OrchestratorError::Validation)
            })?;

        // Keep consuming the persistent supervisor stream after the initial
        // plan so EOF/crash is observable and the child is removed from the
        // live map. Subsequent user chat can be routed through the same map.
        let supervisors = self.supervisor_sessions.clone();
        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                if matches!(event, RuntimeEvent::Stopped { .. }) {
                    break;
                }
            }
            supervisors.lock().await.remove(&mission_id);
        });
        Ok(plan)
    }

    pub(crate) async fn persist_supervisor_session_id(
        &self,
        mission_id: MissionId,
        provider_session_id: Option<String>,
    ) -> Result<()> {
        // Session readiness is emitted by a provider reader task while GUI
        // commands may update the same mission concurrently. Bind the
        // projection write to the revision we observed and retry a bounded
        // number of times; committing an unguarded stale snapshot here could
        // silently erase a just-created task or budget edit.
        for _ in 0..4 {
            let mut snapshot = self.store.snapshot().await?;
            let expected_revision = snapshot.revision;
            let mission = snapshot
                .missions
                .iter_mut()
                .find(|mission| mission.id == mission_id)
                .ok_or(OrchestratorError::NotFound)?;
            mission.supervisor_session_id = provider_session_id.clone();
            mission.updated_at = timestamp_now();
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(expected_revision),
                    ActorRef::system(),
                    Event::MissionSupervisorSessionChanged {
                        mission_id,
                        provider_session_id: provider_session_id.clone(),
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
