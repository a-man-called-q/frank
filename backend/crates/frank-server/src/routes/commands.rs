//! Snapshot reads and the single command-submission endpoint.

use crate::auth::*;
use crate::*;

pub(crate) async fn snapshot(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if headers.contains_key("x-frank-agent-token") {
        let Some((agent_id, task_id)) = agent_capability_from_headers(&state, &headers).await
        else {
            return api_error_response(
                StatusCode::UNAUTHORIZED,
                ApiError::new(
                    ErrorCode::Unauthorized,
                    "agent session capability is invalid",
                ),
            );
        };
        return match scoped_agent_snapshot(&state, agent_id, task_id).await {
            Some(mut snapshot) => {
                refresh_connector_status(&state, &mut snapshot);
                (StatusCode::OK, Json(snapshot)).into_response()
            }
            None => api_error_response(
                StatusCode::FORBIDDEN,
                ApiError::new(
                    ErrorCode::Forbidden,
                    "agent task scope is no longer available",
                ),
            ),
        };
    }
    let Some(_auth) = authenticate(&state, &headers).await else {
        return api_error_response(
            StatusCode::UNAUTHORIZED,
            ApiError::new(ErrorCode::Unauthorized, "device authentication required"),
        );
    };
    match state.store.snapshot().await {
        Ok(mut snapshot) => {
            refresh_connector_status(&state, &mut snapshot);
            (StatusCode::OK, Json(snapshot)).into_response()
        }
        Err(_) => api_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::new(ErrorCode::Internal, "snapshot unavailable"),
        ),
    }
}

fn refresh_connector_status(state: &ServerState, snapshot: &mut Snapshot) {
    for profile in &mut snapshot.organization.connector_profiles {
        if profile.kind == ConnectorKind::Taskboard {
            // Taskboard is a daemon-owned adapter and has no external
            // credential or connection test.
            profile.configured = true;
            profile.health = ConnectorHealth::Healthy;
            profile.diagnostic = None;
        } else {
            profile.configured = state.connector_credentials.configured(profile.id);
            if !profile.configured {
                profile.health = ConnectorHealth::Unknown;
                profile.diagnostic = Some("credential is not configured".into());
            }
        }
    }
}

pub(crate) async fn commands(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(command): Json<CommandEnvelope>,
) -> impl IntoResponse {
    let (actor, role) = if headers.contains_key("x-frank-agent-token") {
        let Some((agent_id, task_id)) = agent_capability_from_headers(&state, &headers).await
        else {
            return api_error_response(
                StatusCode::UNAUTHORIZED,
                ApiError::new(
                    ErrorCode::Unauthorized,
                    "agent session capability is invalid",
                ),
            );
        };
        if !agent_command_is_scoped(&state, &command.command, agent_id, task_id).await {
            return api_error_response(
                StatusCode::FORBIDDEN,
                ApiError::new(
                    ErrorCode::Forbidden,
                    "agent session capability is outside this task scope",
                ),
            );
        }
        (
            ActorRef {
                kind: ActorKind::Agent,
                id: Some(agent_id.to_string()),
                display_name: None,
            },
            DeviceRole::Operator,
        )
    } else {
        let Some(auth) = authenticate(&state, &headers).await else {
            return api_error_response(
                StatusCode::UNAUTHORIZED,
                ApiError::new(ErrorCode::Unauthorized, "device authentication required"),
            );
        };
        (
            ActorRef {
                kind: ActorKind::Device,
                id: Some(auth.device_id.to_string()),
                display_name: Some(auth.name),
            },
            auth.role,
        )
    };
    let terminal_cleanup = match &command.command {
        Command::ReleaseControl { session_id, .. } | Command::CloseTerminal { session_id } => {
            Some(*session_id)
        }
        _ => None,
    };
    let response = state.orchestrator.execute(command, actor, role).await;
    if let Some(CommandResult::Terminal(session)) = response.result.clone() {
        // A replayed OpenTerminal returns the original command response from
        // SQLite.  Do not replace a live PTY (or resurrect a session that was
        // closed after the original command) as a side effect of that replay.
        // After a daemon restart the in-memory map is empty, so an active
        // durable session is spawned again exactly once.
        let durable_active = state
            .store
            .snapshot()
            .await
            .ok()
            .and_then(|snapshot| {
                snapshot
                    .terminals
                    .iter()
                    .find(|candidate| candidate.id == session.id)
                    .map(|candidate| candidate.active)
            })
            .unwrap_or(false);
        let already_spawned = state
            .terminal_sessions
            .lock()
            .await
            .contains_key(&session.id);
        if !durable_active || already_spawned {
            if let Some(session_id) = terminal_cleanup {
                state.terminal_streams.lock().await.remove(&session_id);
                if let Some(pty) = state.terminal_sessions.lock().await.remove(&session_id) {
                    let _ = pty.lock().await.kill();
                }
            }
            return (StatusCode::OK, Json(response)).into_response();
        }
        let spec = ShellSpec {
            cwd: session.cwd.clone(),
            cols: session.cols,
            rows: session.rows,
            shell: None,
        };
        match PtySession::spawn(session.id.to_string(), &spec) {
            Ok(pty) => {
                let pty = Arc::new(Mutex::new(pty));
                state
                    .terminal_sessions
                    .lock()
                    .await
                    .insert(session.id, pty.clone());
                ensure_terminal_pump(&state, session.id, pty).await;
            }
            Err(error) => {
                // OpenTerminal is persisted before the OS PTY is spawned. If
                // the spawn fails, close the durable session immediately so
                // reconnecting clients do not see a terminal that can never
                // be attached.
                let _ = state
                    .orchestrator
                    .execute(
                        CommandEnvelope {
                            protocol_version: PROTOCOL_VERSION,
                            command_id: CommandId::new(),
                            expected_revision: Some(response.revision),
                            command: Command::CloseTerminal {
                                session_id: session.id,
                            },
                        },
                        ActorRef::system(),
                        DeviceRole::Owner,
                    )
                    .await;
                return api_error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ApiError::new(
                        ErrorCode::Internal,
                        format!("terminal could not start: {error}"),
                    ),
                );
            }
        }
    }
    if let Some(error) = response.error.clone() {
        return api_error_response(status_for_error(error.code), error);
    }
    if let Some(session_id) = terminal_cleanup {
        state.terminal_streams.lock().await.remove(&session_id);
        if let Some(pty) = state.terminal_sessions.lock().await.remove(&session_id) {
            let _ = pty.lock().await.kill();
        }
    }
    (StatusCode::OK, Json(response)).into_response()
}
