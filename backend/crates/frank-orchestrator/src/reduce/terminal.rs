//! Terminal sessions and control leases.

use frank_protocol::*;

use crate::*;

impl Orchestrator {
    pub(crate) async fn reduce_terminal(
        &self,
        mut snapshot: Snapshot,
        command: Command,
        actor: &ActorRef,
    ) -> Result<(Snapshot, Event, CommandResult)> {
        match command {
            Command::OpenTerminal {
                task_id,
                cols,
                rows,
            } => {
                let task = snapshot
                    .tasks
                    .iter()
                    .find(|task| task.id == task_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if !matches!(task.status, TaskStatus::Running | TaskStatus::Review) {
                    return Err(OrchestratorError::Validation(
                        "terminal is available only for an active task".into(),
                    ));
                }
                let cwd = task
                    .worktree
                    .clone()
                    .filter(|path| !path.trim().is_empty())
                    .ok_or_else(|| {
                        OrchestratorError::Validation("terminal requires a task worktree".into())
                    })?;
                if !Path::new(&cwd).is_dir()
                    || !is_allowed_path(&cwd, &snapshot.server.allowed_project_roots)
                {
                    return Err(OrchestratorError::Validation(
                        "terminal worktree is outside the allowed project roots".into(),
                    ));
                }
                let session = TerminalSessionView {
                    id: TerminalSessionId::new(),
                    task_id,
                    cwd,
                    cols: cols.clamp(20, 400),
                    rows: rows.clamp(5, 200),
                    active: true,
                    lease: None,
                };
                snapshot.terminals.push(session.clone());
                Ok((
                    snapshot,
                    Event::TerminalOpened {
                        session: session.clone(),
                    },
                    CommandResult::Terminal(session),
                ))
            }
            Command::TakeControl { session_id } => {
                let session = snapshot
                    .terminals
                    .iter_mut()
                    .find(|session| session.id == session_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if session.lease.as_ref().is_some_and(|lease| {
                    lease.expires_at.parse::<u128>().unwrap_or_default() > now_plus_seconds(0)
                }) {
                    return Err(OrchestratorError::Validation(
                        "terminal is already controlled by another device".into(),
                    ));
                }
                session.lease = None;
                let lease = ControlLeaseView {
                    lease_id: uuid::Uuid::new_v4().to_string(),
                    session_id,
                    actor: actor.clone(),
                    expires_at: format!("{}", now_plus_seconds(30)),
                };
                session.lease = Some(lease.clone());
                Ok((
                    snapshot,
                    Event::TerminalLeaseChanged { lease },
                    CommandResult::Accepted,
                ))
            }
            Command::RenewControl {
                session_id,
                lease_id,
            } => {
                let session = snapshot
                    .terminals
                    .iter_mut()
                    .find(|session| session.id == session_id)
                    .ok_or(OrchestratorError::NotFound)?;
                let lease = session
                    .lease
                    .as_mut()
                    .filter(|lease| lease.lease_id == lease_id && same_actor(&lease.actor, actor))
                    .ok_or(OrchestratorError::Forbidden)?;
                lease.expires_at = format!("{}", now_plus_seconds(30));
                let renewed = lease.clone();
                Ok((
                    snapshot,
                    Event::TerminalLeaseChanged { lease: renewed },
                    CommandResult::Accepted,
                ))
            }
            Command::ReleaseControl {
                session_id,
                lease_id,
            } => {
                let session = snapshot
                    .terminals
                    .iter_mut()
                    .find(|session| session.id == session_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if session.lease.as_ref().is_none_or(|lease| {
                    lease.lease_id != lease_id || !same_actor(&lease.actor, actor)
                }) {
                    return Err(OrchestratorError::Forbidden);
                }
                session.lease = None;
                let lease = ControlLeaseView {
                    lease_id,
                    session_id,
                    actor: actor.clone(),
                    expires_at: "released".into(),
                };
                Ok((
                    snapshot,
                    Event::TerminalLeaseChanged { lease },
                    CommandResult::Accepted,
                ))
            }
            Command::CloseTerminal { session_id } => {
                let session = snapshot
                    .terminals
                    .iter_mut()
                    .find(|session| session.id == session_id)
                    .ok_or(OrchestratorError::NotFound)?;
                if let Some(lease) = &session.lease
                    && !same_actor(&lease.actor, actor)
                    && lease.expires_at.parse::<u128>().unwrap_or_default() > now_plus_seconds(0)
                {
                    return Err(OrchestratorError::Forbidden);
                }
                session.active = false;
                session.lease = None;
                Ok((
                    snapshot,
                    Event::TerminalClosed { session_id },
                    CommandResult::Accepted,
                ))
            }
            _ => super::misrouted(),
        }
    }
}
