//! Agent-to-agent delivery: queueing, expiry, waking a recipient, and the
//! supervisor session that handles anything addressed to it.

use frank_protocol::*;

use crate::*;

impl Orchestrator {
    pub(crate) async fn deliver_queued_messages(&self, snapshot: &Snapshot) -> Result<()> {
        let queued = snapshot
            .messages
            .iter()
            .filter(|message| message.delivery == DeliveryStatus::Queued)
            .map(|message| message.id)
            .collect::<Vec<_>>();
        for message_id in queued {
            // A transient provider wake-up failure must not prevent the
            // durable Delivered transition. `deliver_message` itself only
            // returns an error when persistence cannot complete; leave that
            // row queued and let the next reconciler tick retry it.
            if let Err(error) = self.deliver_message(message_id).await
                && !matches!(error, OrchestratorError::Store(_))
            {
                return Err(error);
            }
        }
        Ok(())
    }

    /// Required-reply messages are not allowed to remain acknowledged forever
    /// after a provider crash.  The deadline is derived from the durable
    /// creation timestamp, so a restart cannot reset it.  `Inform` and other
    /// fire-and-forget acts are intentionally excluded.
    pub(crate) async fn expire_pending_messages(&self, snapshot: &Snapshot) -> Result<()> {
        const REPLY_TIMEOUT_SECONDS: u64 = 600;
        let now = epoch_seconds();
        let expired = snapshot
            .messages
            .iter()
            .filter(|message| {
                message.act.requires_reply()
                    && matches!(
                        message.delivery,
                        DeliveryStatus::Delivered | DeliveryStatus::Acknowledged
                    )
                    && message
                        .created_at
                        .parse::<u64>()
                        .ok()
                        .map(|millis| now.saturating_sub(millis / 1_000) >= REPLY_TIMEOUT_SECONDS)
                        .unwrap_or(false)
            })
            .map(|message| message.id)
            .collect::<Vec<_>>();
        for message_id in expired {
            let mut latest = self.store.snapshot().await?;
            let Some(message) = latest
                .messages
                .iter_mut()
                .find(|message| message.id == message_id)
            else {
                continue;
            };
            if !message.act.requires_reply()
                || !matches!(
                    message.delivery,
                    DeliveryStatus::Delivered | DeliveryStatus::Acknowledged
                )
            {
                continue;
            }
            message.delivery = DeliveryStatus::Failed;
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(latest.revision),
                    ActorRef::system(),
                    Event::MessageFailed { message_id },
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

    /// Advance a queued broker message to Delivered.  Recipient wake-up is
    /// intentionally best-effort: the durable event is authoritative and
    /// the reconciler will start/resume an inactive worker on its next tick.
    /// Keeping this transition separate also lets supervisor and worker
    /// messages share exactly the same delivery semantics.
    pub(crate) async fn deliver_message(&self, message_id: MessageId) -> Result<()> {
        for _ in 0..3 {
            let mut snapshot = self.store.snapshot().await?;
            let Some(message) = snapshot
                .messages
                .iter_mut()
                .find(|message| message.id == message_id)
            else {
                return Ok(());
            };
            if message.delivery != DeliveryStatus::Queued {
                return Ok(());
            }
            message.delivery = DeliveryStatus::Delivered;
            let delivered_message = message.clone();
            match self
                .store
                .commit_command(
                    CommandId::new(),
                    Some(snapshot.revision),
                    ActorRef::system(),
                    Event::MessageDelivered { message_id },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await
            {
                Ok(_) => {
                    self.wake_recipient(&delivered_message).await;
                    return Ok(());
                }
                Err(StoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }

    /// Wake a live provider session after the broker has durably delivered a
    /// message.  A worker may be offline when a message arrives; the daemon's
    /// normal reconciler observes the durable delivery and starts/resumes the
    /// assigned task without making the broker depend on a recursive scheduler
    /// call.
    pub(crate) async fn wake_recipient(&self, message: &MessageView) {
        if message.recipient.kind == ActorKind::Supervisor {
            self.wake_supervisor(message).await;
            return;
        }
        let Some(recipient_id) = message
            .recipient
            .id
            .as_deref()
            .and_then(|id| AgentId::parse(id).ok())
        else {
            return;
        };
        let snapshot = match self.store.snapshot().await {
            Ok(snapshot) => snapshot,
            Err(_) => return,
        };
        let task_id = message.task_id.or_else(|| {
            snapshot.tasks.iter().find_map(|task| {
                (task.mission_id == message.mission_id && task.assigned_agent == Some(recipient_id))
                    .then_some(task.id)
            })
        });
        let Some(task_id) = task_id else {
            return;
        };
        if let Some(session) = self.sessions.lock().await.get(&task_id).cloned() {
            let _ = session
                .send(&ProviderMessage {
                    role: "user".into(),
                    content: format!(
                        "[Frank broker message from {}]\n{}",
                        message.sender.display_name.as_deref().unwrap_or("agent"),
                        message.body
                    ),
                    correlation_id: Some(message.id.to_string()),
                })
                .await;
        }
        // Inactive recipients are picked up by the daemon reconciler's
        // existing half-second loop. Avoid recursively spawning that loop
        // from inside `reconcile` itself; the durable Delivered event is the
        // wake signal and remains safe across a process restart.
    }

    /// Route messages addressed to the persistent supervisor through the same
    /// broker used by workers. A supervisor process is created lazily after a
    /// daemon restart, while the message remains durable in the mailbox.
    pub(crate) async fn wake_supervisor(&self, message: &MessageView) {
        let session = match self.ensure_supervisor_session(message.mission_id).await {
            Ok(session) => session,
            Err(_) => return,
        };
        let sender = message.sender.display_name.as_deref().unwrap_or("operator");
        let content = format!(
            "[Frank broker message from {sender}; act={:?}; message_id={}]:\n{}",
            message.act, message.id, message.body
        );
        let _ = session
            .send(&ProviderMessage {
                role: "user".into(),
                content,
                correlation_id: Some(message.id.to_string()),
            })
            .await;
    }

    pub(crate) async fn ensure_supervisor_session(
        &self,
        mission_id: MissionId,
    ) -> Result<Arc<frank_agent::RuntimeSession>> {
        if let Some(session) = self
            .supervisor_sessions
            .lock()
            .await
            .get(&mission_id)
            .cloned()
        {
            return Ok(session);
        }
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
            .ok_or_else(|| {
                OrchestratorError::ProviderUnavailable("Frank supervisor profile is missing".into())
            })?;
        let request = StartRequest {
            agent_id: supervisor.id.to_string(),
            task_id: None,
            cwd: project.path,
            instructions: supervisor.instructions,
            policy: supervisor.policy,
            model: supervisor.model,
            resume_session_id: mission.supervisor_session_id.clone(),
            server_url: local_server_url(&snapshot.server),
            server_certificate_fingerprint: (!snapshot.server.tls_fingerprint.is_empty())
                .then(|| snapshot.server.tls_fingerprint.clone()),
            session_capability: None,
        };
        let session = if let Some(provider_session_id) = request.resume_session_id.as_deref() {
            self.runtime
                .resume(
                    mission.supervisor_provider,
                    request.clone(),
                    provider_session_id,
                )
                .await
        } else {
            self.runtime
                .start(mission.supervisor_provider, request.clone())
                .await
        }
        .map_err(|error| OrchestratorError::ProviderUnavailable(error.to_string()))?;
        let session = Arc::new(session);
        let mut events = session
            .events()
            .await
            .map_err(|error| OrchestratorError::ProviderUnavailable(error.to_string()))?;
        self.supervisor_sessions
            .lock()
            .await
            .insert(mission_id, session.clone());
        let supervisors = self.supervisor_sessions.clone();
        let orchestrator = self.clone();
        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                match event {
                    RuntimeEvent::Ready {
                        provider_session_id,
                    } => {
                        let _ = orchestrator
                            .persist_supervisor_session_id(mission_id, Some(provider_session_id))
                            .await;
                    }
                    RuntimeEvent::Stopped { .. } | RuntimeEvent::Error { .. } => break,
                    _ => {}
                }
            }
            supervisors.lock().await.remove(&mission_id);
        });
        Ok(session)
    }

    pub(crate) async fn complete_delivery(
        &self,
        mission_id: MissionId,
    ) -> Result<git::DeliveryResult> {
        let snapshot = self.store.snapshot().await?;
        let mission = snapshot
            .missions
            .iter()
            .find(|mission| mission.id == mission_id)
            .ok_or(OrchestratorError::NotFound)?;
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
        let mission_plan = workflow.branch_plan(&mission.branch, &workflow.project.base_branch);
        workflow
            .create_worktree(&mission_plan)
            .await
            .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
        let checks = workflow
            .run_checks(&mission_plan.path)
            .await
            .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
        if checks.iter().any(|check| !check.success) {
            return Err(OrchestratorError::Validation(
                "required project checks failed on the mission branch".into(),
            ));
        }
        workflow
            .push_mission(&mission.branch)
            .await
            .map_err(|error| OrchestratorError::Validation(error.to_string()))?;
        Ok(git::DeliveryResult {
            branch: mission.branch.clone(),
            draft_pr_url: None,
        })
    }
}
