//! Enforcement of the budget ceilings that budget.rs accounts against.
//!
//! Time budgets are the one dimension not durable in the snapshot: elapsed
//! wall-clock is anchored in memory and only used for a live hard stop, while
//! token, cost and turn totals survive restart.

use frank_protocol::*;

use crate::*;

impl Orchestrator {
    /// Enforce wall-clock budgets even when a provider has not emitted a
    /// usage frame.  The start time is persisted in `budget_clocks`, so a
    /// daemon restart cannot reset an active task's deadline.  Token and
    /// cost budgets remain telemetry-driven; this watchdog only handles the
    /// explicit `time_seconds` limit.
    pub(crate) async fn enforce_time_budgets(&self) -> Result<()> {
        let snapshot = self.store.snapshot().await?;
        let mut expired_missions = HashSet::new();
        for mission in snapshot
            .missions
            .iter()
            .filter(|mission| mission.status == MissionStatus::Active)
        {
            if self
                .scope_time_expired(&mission.id.to_string(), &mission.budget)
                .await?
            {
                expired_missions.insert(mission.id);
            }
        }

        // A task is blocked once for the first expired scope.  Mission scope
        // wins over task/agent scope so all workers of an expired mission are
        // stopped consistently and the mission itself is paused exactly once.
        let mut expired_tasks = HashMap::<TaskId, (BudgetScope, String)>::new();
        for task in snapshot
            .tasks
            .iter()
            .filter(|task| task.status == TaskStatus::Running)
        {
            if expired_missions.contains(&task.mission_id) {
                expired_tasks.insert(
                    task.id,
                    (
                        BudgetScope::Mission,
                        "mission wall-clock budget exceeded".to_string(),
                    ),
                );
                continue;
            }
            if self
                .scope_time_expired(&task.id.to_string(), &task.budget)
                .await?
            {
                expired_tasks.insert(
                    task.id,
                    (
                        BudgetScope::Task,
                        "task wall-clock budget exceeded".to_string(),
                    ),
                );
                continue;
            }
            let Some(agent_id) = task.assigned_agent else {
                continue;
            };
            let Some(agent) = snapshot.agents.iter().find(|agent| agent.id == agent_id) else {
                continue;
            };
            if self
                .scope_time_expired(&agent.id.to_string(), &agent.budget)
                .await?
            {
                expired_tasks.insert(
                    task.id,
                    (
                        BudgetScope::Agent,
                        "agent wall-clock budget exceeded".to_string(),
                    ),
                );
            }
        }

        for mission_id in expired_missions {
            let task_ids = expired_tasks
                .iter()
                .filter_map(|(task_id, (scope, _))| {
                    (*scope == BudgetScope::Mission).then_some(*task_id)
                })
                .collect::<Vec<_>>();
            for task_id in task_ids {
                self.stop_task_for_budget(task_id).await?;
            }
            let response = self
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
            if let Some(error) = response.error
                && !matches!(error.code, ErrorCode::Conflict | ErrorCode::StaleRevision)
            {
                return Err(OrchestratorError::Validation(error.message));
            }
            self.emit_budget_pause(
                BudgetScope::Mission,
                "mission wall-clock budget exceeded; mission paused",
            )
            .await?;
        }

        let mut emitted = HashSet::<String>::new();
        for (task_id, (scope, reason)) in expired_tasks {
            // Mission-scoped tasks were already stopped and audited above.
            if scope == BudgetScope::Mission {
                continue;
            }
            self.stop_task_for_budget(task_id).await?;
            if emitted.insert(format!("{scope:?}")) {
                self.emit_budget_pause(scope, &reason).await?;
            }
        }
        Ok(())
    }

    pub(crate) async fn scope_time_expired(&self, scope_id: &str, budget: &Budget) -> Result<bool> {
        let Some(limit) = budget.time_seconds else {
            return Ok(false);
        };
        let Some((started_at, persisted_deadline)) = self.store.budget_clock(scope_id).await?
        else {
            return Ok(false);
        };
        // Recompute from the current budget as well as the persisted deadline:
        // an explicit budget adjustment may shorten or extend an existing
        // clock without resetting the original start time.
        let deadline = started_at.saturating_add(limit);
        let deadline = persisted_deadline.map_or(deadline, |stored| {
            // Keep a deadline written by an older daemon only when it is
            // stricter.  This prevents a restart during a budget adjustment
            // from accidentally widening a limit that was already active.
            stored.min(deadline)
        });
        Ok(epoch_seconds() >= deadline)
    }

    /// Stop one running worker before transitioning its card to `Blocked`.
    /// Provider/session cleanup is deliberately best-effort after the durable
    /// budget decision; the task state is still blocked if a child has
    /// already crashed or disappeared.
    pub(crate) async fn stop_task_for_budget(&self, task_id: TaskId) -> Result<()> {
        let snapshot = self.store.snapshot().await?;
        let Some(task) = snapshot
            .tasks
            .iter()
            .find(|task| task.id == task_id && task.status == TaskStatus::Running)
            .cloned()
        else {
            return Ok(());
        };
        if let Some(agent_id) = task.assigned_agent {
            if let Some(agent) = snapshot.agents.iter().find(|agent| agent.id == agent_id) {
                if let Some(session) = self.sessions.lock().await.remove(&task_id) {
                    let _ = session.graceful_stop().await;
                }
                self.scheduler.lock().await.finish(task_id, agent.provider);
            }
            let capabilities = self
                .agent_capabilities
                .lock()
                .await
                .iter()
                .filter(|(_, capability)| capability.task_id == task_id)
                .map(|(token, _)| token.clone())
                .collect::<Vec<_>>();
            for token in capabilities {
                self.revoke_agent_capability(&token).await;
            }
            self.clear_agent_session(agent_id, AgentStatus::Paused)
                .await?;
        }
        // A concurrent manual move/cancel may have won the race. In that
        // case the authoritative state is already safe and no extra error is
        // needed from the reconciliation loop.
        let response = self
            .execute(
                CommandEnvelope {
                    protocol_version: PROTOCOL_VERSION,
                    command_id: CommandId::new(),
                    expected_revision: None,
                    command: Command::SetTaskStatus {
                        task_id,
                        status: TaskStatus::Blocked,
                    },
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
        Ok(())
    }

    pub(crate) async fn emit_budget_pause(&self, scope: BudgetScope, reason: &str) -> Result<()> {
        let command_id = CommandId::new();
        for _ in 0..4 {
            let snapshot = self.store.snapshot().await?;
            match self
                .store
                .commit_command(
                    command_id,
                    Some(snapshot.revision),
                    ActorRef::system(),
                    Event::BudgetPaused {
                        scope,
                        reason: reason.to_string(),
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

    pub(crate) async fn start_scope_clock(&self, scope_id: String) {
        let now = epoch_seconds();
        // Keeping the first start time for a scope makes a mission budget
        // cover all of its parallel tasks instead of resetting on every
        // worker frame. A subsequent task in the same mission therefore
        // cannot evade a time limit by restarting its provider session.
        let persisted = self
            .store
            .budget_clock(&scope_id)
            .await
            .ok()
            .flatten()
            .map(|(started_at, _)| started_at);
        let mut clocks = self.scope_started_at.lock().await;
        let started = persisted.unwrap_or(now);
        let is_new = !clocks.contains_key(&scope_id);
        clocks.entry(scope_id.clone()).or_insert(started);
        drop(clocks);
        if is_new {
            let _ = self
                .store
                .upsert_budget_clock(&scope_id, started, None)
                .await;
        }
    }

    pub(crate) async fn start_scope_clock_with_budget(&self, scope_id: String, budget: &Budget) {
        self.start_scope_clock(scope_id.clone()).await;
        let Some(limit) = budget.time_seconds else {
            return;
        };
        let Some((started, _)) = self.store.budget_clock(&scope_id).await.ok().flatten() else {
            return;
        };
        let _ = self
            .store
            .upsert_budget_clock(&scope_id, started, Some(started.saturating_add(limit)))
            .await;
    }

    pub(crate) async fn scope_elapsed_seconds(&self, scope_id: &str) -> u64 {
        let now = epoch_seconds();
        let mut clocks = self.scope_started_at.lock().await;
        let started = *clocks.entry(scope_id.to_string()).or_insert(now);
        now.saturating_sub(started)
    }

    pub async fn record_usage(
        &self,
        usage: UsageView,
        measured_budget: Option<&Budget>,
    ) -> Result<EventEnvelope> {
        let telemetry = UsageTelemetry {
            measured_input_tokens: usage.measured_input_tokens,
            measured_output_tokens: usage.measured_output_tokens,
            estimated_input_tokens: usage.estimated_input_tokens,
            estimated_output_tokens: usage.estimated_output_tokens,
            cost_micros: usage.cost_micros,
        };
        let exceeded = {
            let mut budgets = self.budgets.lock().await;
            budgets.record(usage.scope_id.clone(), &telemetry);
            measured_budget
                .filter(|budget| !budgets.allows_measured(&usage.scope_id, budget))
                .is_some()
        };
        let event = self.commit_usage_event(usage).await?;
        if exceeded {
            return Err(OrchestratorError::BudgetExceeded);
        }
        Ok(event)
    }

    /// Record one provider telemetry frame and enforce task, mission, and
    /// agent budgets against measured quantities. Estimates are retained for
    /// warning/ledger display but never cause a hard stop.
    pub(crate) async fn record_runtime_usage(
        &self,
        usage: UsageView,
        mission_id: MissionId,
        agent_id: AgentId,
    ) -> Result<Option<BudgetScope>> {
        let telemetry = UsageTelemetry {
            measured_input_tokens: usage.measured_input_tokens,
            measured_output_tokens: usage.measured_output_tokens,
            estimated_input_tokens: usage.estimated_input_tokens,
            estimated_output_tokens: usage.estimated_output_tokens,
            cost_micros: usage.cost_micros,
        };
        let snapshot = self.store.snapshot().await?;
        let task_budget = snapshot
            .tasks
            .iter()
            .find(|task| task.id.to_string() == usage.scope_id)
            .map(|task| task.budget.clone())
            .ok_or(OrchestratorError::NotFound)?;
        let mission_budget = snapshot
            .missions
            .iter()
            .find(|mission| mission.id == mission_id)
            .map(|mission| mission.budget.clone())
            .ok_or(OrchestratorError::NotFound)?;
        let agent_budget = snapshot
            .agents
            .iter()
            .find(|agent| agent.id == agent_id)
            .map(|agent| agent.budget.clone())
            .ok_or(OrchestratorError::NotFound)?;
        let task_id = usage.scope_id.clone();
        let mission_id_text = mission_id.to_string();
        let agent_id_text = agent_id.to_string();
        // A task normally gets a clock when its provider session starts. If
        // telemetry arrives during crash recovery before that hook runs,
        // initialize the three scopes here rather than silently treating the
        // first frame as infinitely old.
        self.start_scope_clock(task_id.clone()).await;
        self.start_scope_clock(mission_id_text.clone()).await;
        self.start_scope_clock(agent_id_text.clone()).await;
        let task_elapsed = self.scope_elapsed_seconds(&task_id).await;
        let mission_elapsed = self.scope_elapsed_seconds(&mission_id_text).await;
        let agent_elapsed = self.scope_elapsed_seconds(&agent_id_text).await;
        let exceeded = {
            let mut budgets = self.budgets.lock().await;
            budgets.rebuild_from_snapshot(&snapshot);
            budgets.record(task_id.clone(), &telemetry);
            budgets.record(mission_id_text.clone(), &telemetry);
            budgets.record(agent_id_text.clone(), &telemetry);
            [
                (BudgetScope::Task, task_id, task_budget, task_elapsed),
                (
                    BudgetScope::Mission,
                    mission_id_text,
                    mission_budget,
                    mission_elapsed,
                ),
                (
                    BudgetScope::Agent,
                    agent_id_text,
                    agent_budget,
                    agent_elapsed,
                ),
            ]
            .into_iter()
            .find_map(|(scope, scope_id, budget, elapsed)| {
                let time_exceeded = budget.time_seconds.is_some_and(|limit| elapsed >= limit);
                (time_exceeded || !budgets.allows_measured(&scope_id, &budget)).then_some(scope)
            })
        };
        self.commit_usage_event(usage).await?;
        if let Some(scope) = exceeded {
            let snapshot = self.store.snapshot().await?;
            self.store
                .commit_command(
                    CommandId::new(),
                    Some(snapshot.revision),
                    ActorRef::system(),
                    Event::BudgetPaused {
                        scope,
                        reason: "measured budget exceeded; task paused".into(),
                    },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await?;
        }
        Ok(exceeded)
    }

    pub(crate) async fn commit_usage_event(&self, usage: UsageView) -> Result<EventEnvelope> {
        let command_id = CommandId::new();
        for _ in 0..3 {
            let mut snapshot = self.store.snapshot().await?;
            // Event projections are durable, but reconnecting clients load
            // this snapshot first. Include the row in the authoritative
            // snapshot so a restart cannot make the ledger appear empty until
            // a live event happens to arrive.
            snapshot.usage.push(usage.clone());
            match self
                .store
                .commit_command(
                    command_id,
                    Some(snapshot.revision),
                    ActorRef::system(),
                    Event::UsageRecorded {
                        usage: usage.clone(),
                    },
                    snapshot,
                    CommandResult::Accepted,
                )
                .await
            {
                Ok(commit) => return Ok(commit.event),
                Err(StoreError::StaleRevision { .. }) => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(OrchestratorError::Store(StoreError::StaleRevision {
            current: self.store.current_revision().await?,
        }))
    }
}
