//! Terminal sessions: lease arbitration, the WebSocket, and the PTY pump.

use crate::auth::*;
use crate::*;

#[derive(Debug, Deserialize)]
pub(crate) struct TerminalQuery {
    after: Option<u64>,
}

#[derive(Debug, Clone)]
struct TerminalSocketAuth {
    device_id: DeviceId,
    token: String,
}

pub(crate) async fn terminals(
    State(state): State<ServerState>,
    AxumPath(session_id): AxumPath<String>,
    headers: HeaderMap,
    Query(query): Query<TerminalQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let Some(token) = bearer(&headers).map(str::to_owned) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(auth) = authenticate(&state, &headers).await else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Ok(session_id) = TerminalSessionId::parse(&session_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let snapshot = match state.store.snapshot().await {
        Ok(snapshot) => snapshot,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let Some(durable_session) = snapshot
        .terminals
        .iter()
        .find(|session| session.id == session_id && session.active)
        .cloned()
    else {
        return StatusCode::NOT_FOUND.into_response();
    };

    // PTYs are intentionally process-local, while terminal metadata is
    // durable.  Rehydrate an active shell lazily after a daemon restart (or
    // after a process-local map was evicted) instead of making reconnects
    // fail with a misleading 404.  The map insertion is checked again under
    // the lock so two simultaneous viewers cannot spawn two shells.
    let pty = if let Some(existing) = state
        .terminal_sessions
        .lock()
        .await
        .get(&session_id)
        .cloned()
    {
        existing
    } else {
        let spec = ShellSpec {
            cwd: durable_session.cwd.clone(),
            cols: durable_session.cols,
            rows: durable_session.rows,
            shell: None,
        };
        let spawned = match PtySession::spawn(session_id.to_string(), &spec) {
            Ok(pty) => Arc::new(Mutex::new(pty)),
            Err(error) => {
                return api_error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    ApiError::new(
                        ErrorCode::Internal,
                        format!("terminal could not be resumed: {error}"),
                    ),
                );
            }
        };
        let mut sessions = state.terminal_sessions.lock().await;
        sessions
            .entry(session_id)
            .or_insert_with(|| spawned.clone())
            .clone()
    };
    ensure_terminal_pump(&state, session_id, pty.clone()).await;
    let stream = state
        .terminal_streams
        .lock()
        .await
        .get(&session_id)
        .cloned();
    let Some(stream) = stream else {
        return api_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::new(ErrorCode::Internal, "terminal output stream is unavailable"),
        );
    };
    let session_state = state.clone();
    let actor_device_id = auth.device_id;
    ws.on_upgrade(move |socket| {
        terminal_socket(
            socket,
            pty,
            stream,
            session_state,
            session_id,
            TerminalSocketAuth {
                device_id: actor_device_id,
                token,
            },
            query.after.map(TerminalSequence),
        )
    })
    .into_response()
}

async fn terminal_socket(
    mut socket: WebSocket,
    pty: Arc<Mutex<PtySession>>,
    stream: broadcast::Sender<TerminalFrame>,
    state: ServerState,
    session_id: TerminalSessionId,
    auth: TerminalSocketAuth,
    replay_after: Option<TerminalSequence>,
) {
    // Viewing terminal output is deliberately independent from Take Control.
    // Only input/resize below consult the live lease, so a second operator or
    // an observer can follow progress without interrupting the worker.
    let mut output = stream.subscribe();
    let (replay, oldest, latest) = match state
        .store
        .terminal_replay(session_id, replay_after, 2_048)
        .await
    {
        Ok(value) => value,
        Err(_) => {
            let frame = TerminalFrame::Error {
                message: "terminal transcript is temporarily unavailable".into(),
            };
            let _ = socket
                .send(Message::Text(
                    serde_json::to_string(&frame).unwrap_or_default().into(),
                ))
                .await;
            return;
        }
    };
    // Tell the client which sequence immediately precedes the replay batch.
    // For a fresh viewer this is the first retained chunk minus one; sending
    // `None` would make a client initialise at `latest` and discard replay.
    let replay_from = replay
        .first()
        .map(|(sequence, _)| TerminalSequence(sequence.0.saturating_sub(1)))
        .or(replay_after);
    let hello = TerminalFrame::Hello {
        session_id,
        next_sequence: latest.next(),
        replay_from,
    };
    if socket
        .send(Message::Text(
            serde_json::to_string(&hello).unwrap_or_default().into(),
        ))
        .await
        .is_err()
    {
        return;
    }
    if let (Some(after), Some(oldest)) = (replay_after, oldest)
        && after.0.saturating_add(1) < oldest.0
    {
        let frame = TerminalFrame::ResyncRequired {
            oldest_sequence: oldest,
        };
        let _ = socket
            .send(Message::Text(
                serde_json::to_string(&frame).unwrap_or_default().into(),
            ))
            .await;
        return;
    }
    for (sequence, bytes) in replay {
        if socket
            .send(Message::Text(
                serde_json::to_string(&TerminalFrame::Replay { sequence, bytes })
                    .unwrap_or_default()
                    .into(),
            ))
            .await
            .is_err()
        {
            return;
        }
    }
    if let Some(lease) = terminal_lease(&state, session_id).await {
        let _ = socket
            .send(Message::Text(
                serde_json::to_string(&TerminalFrame::Lease { lease: Some(lease) })
                    .unwrap_or_default()
                    .into(),
            ))
            .await;
    }
    // WebSocket ping is a liveness signal, not a render loop.  Keep it
    // comfortably below typical proxy idle timeouts without generating a
    // frame every few milliseconds for every connected terminal viewer.
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));
    loop {
        tokio::select! {
            _ = tick.tick() => {
                if state
                    .auth
                    .authenticate(&auth.token)
                    .await
                    .ok()
                    .flatten()
                    .is_none()
                {
                    let _ = socket.send(Message::Close(None)).await;
                    break;
                }
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
            frame = output.recv() => {
                match frame {
                    Ok(frame) => {
                        let Ok(json) = serde_json::to_string(&frame) else { continue };
                        if socket.send(Message::Text(json.into())).await.is_err() { break; }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        let oldest = state.store.terminal_replay(session_id, None, 1).await
                            .ok()
                            .and_then(|(_, oldest, _)| oldest)
                            .unwrap_or(TerminalSequence::ZERO);
                        let frame = TerminalFrame::ResyncRequired { oldest_sequence: oldest };
                        let _ = socket.send(Message::Text(
                            serde_json::to_string(&frame).unwrap_or_default().into()
                        )).await;
                        break;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            message = socket.next() => {
                let Some(Ok(message)) = message else { break; };
                match message {
                    Message::Text(text) => {
                        if text.len() > MAX_TERMINAL_FRAME_BYTES { break; }
                        let Ok(frame) = serde_json::from_str::<TerminalFrame>(&text) else {
                            let error = TerminalFrame::Error { message: "invalid terminal frame".into() };
                            let _ = socket.send(Message::Text(serde_json::to_string(&error).unwrap_or_default().into())).await;
                            continue;
                        };
                        let lease_active =
                            terminal_lease_active(&state, session_id, auth.device_id).await;
                        let result = match frame {
                            TerminalFrame::Input { bytes } if lease_active && bytes.len() <= MAX_TERMINAL_FRAME_BYTES => {
                                let guard = pty.lock().await;
                                guard.send_input(&bytes)
                            }
                            TerminalFrame::Resize { cols, rows } if lease_active => {
                                let guard = pty.lock().await;
                                guard.resize(cols, rows)
                            }
                            TerminalFrame::Input { .. } | TerminalFrame::Resize { .. } if !lease_active => {
                                let error = TerminalFrame::Error { message: "Take Control lease is required for terminal input".into() };
                                let _ = socket.send(Message::Text(serde_json::to_string(&error).unwrap_or_default().into())).await;
                                Ok(())
                            }
                            TerminalFrame::Input { .. } => Err(frank_agent::terminal::PtyError::Io("terminal input frame is too large".into())),
                            TerminalFrame::Resize { .. } => Err(frank_agent::terminal::PtyError::Io("terminal resize is invalid".into())),
                            TerminalFrame::Heartbeat => Ok(()),
                            TerminalFrame::Output { .. }
                            | TerminalFrame::Hello { .. }
                            | TerminalFrame::Replay { .. }
                            | TerminalFrame::ResyncRequired { .. }
                            | TerminalFrame::Lease { .. }
                            | TerminalFrame::SequencedOutput { .. }
                            | TerminalFrame::Exit { .. }
                            | TerminalFrame::Error { .. } => Ok(()),
                        };
                        if let Err(error) = result {
                            let frame = TerminalFrame::Error { message: error.to_string() };
                            let _ = socket.send(Message::Text(serde_json::to_string(&frame).unwrap_or_default().into())).await;
                        }
                    }
                    Message::Close(_) => break,
                    Message::Ping(payload) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    // A viewer disconnect is not a terminal close. The PTY remains owned by
    // frankd so a reconnect can replay/continue it; explicit CloseTerminal
    // removes and kills it in the command handler.
}

/// Start exactly one reader for a PTY.  Reading in the WebSocket task would
/// lose output while every viewer is disconnected; this daemon-owned pump
/// persists transcript chunks first and then fans sequenced frames out to any
/// current viewers.  A broadcast lag is handled by the viewer with a
/// `ResyncRequired` frame and a fresh durable replay.
pub(crate) async fn ensure_terminal_pump(
    state: &ServerState,
    session_id: TerminalSessionId,
    pty: Arc<Mutex<PtySession>>,
) {
    let sender = {
        let mut streams = state.terminal_streams.lock().await;
        if streams.contains_key(&session_id) {
            return;
        }
        let (sender, _) = broadcast::channel(256);
        streams.insert(session_id, sender.clone());
        sender
    };
    let runtime = state.clone();
    tokio::spawn(async move {
        loop {
            let result = {
                let guard = pty.lock().await;
                guard.try_recv()
            };
            match result {
                Ok(Some(TerminalFrame::Output { bytes })) => {
                    let Ok(sequence) = runtime
                        .store
                        .append_terminal_transcript_next(session_id, &bytes)
                        .await
                    else {
                        let _ = sender.send(TerminalFrame::Error {
                            message: "terminal transcript could not be persisted".into(),
                        });
                        continue;
                    };
                    let _ = sender.send(TerminalFrame::SequencedOutput { sequence, bytes });
                }
                Ok(Some(frame @ TerminalFrame::Exit { .. })) => {
                    let _ = sender.send(frame);
                    close_terminal_after_pump_exit(&runtime, session_id).await;
                    break;
                }
                Ok(Some(frame @ TerminalFrame::Error { .. })) => {
                    let _ = sender.send(frame);
                    close_terminal_after_pump_exit(&runtime, session_id).await;
                    break;
                }
                Ok(Some(frame)) => {
                    let _ = sender.send(frame);
                }
                Ok(None) => {
                    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                }
                Err(error) => {
                    let _ = sender.send(TerminalFrame::Error {
                        message: error.to_string(),
                    });
                    close_terminal_after_pump_exit(&runtime, session_id).await;
                    break;
                }
            }
        }
    });
}

pub(crate) async fn close_terminal_after_pump_exit(
    state: &ServerState,
    session_id: TerminalSessionId,
) {
    let _ = state
        .orchestrator
        .execute(
            CommandEnvelope {
                protocol_version: PROTOCOL_VERSION,
                command_id: CommandId::new(),
                expected_revision: None,
                command: Command::CloseTerminal { session_id },
            },
            ActorRef::system(),
            DeviceRole::Owner,
        )
        .await;
    state.terminal_sessions.lock().await.remove(&session_id);
    state.terminal_streams.lock().await.remove(&session_id);
}

pub(crate) async fn terminal_lease(
    state: &ServerState,
    session_id: TerminalSessionId,
) -> Option<ControlLeaseView> {
    state.store.snapshot().await.ok().and_then(|snapshot| {
        snapshot
            .terminals
            .into_iter()
            .find(|session| session.id == session_id)
            .and_then(|session| session.lease)
            .filter(|lease| {
                lease
                    .expires_at
                    .parse::<u128>()
                    .is_ok_and(|expires_at| expires_at > now() as u128)
            })
    })
}

pub(crate) async fn terminal_lease_active(
    state: &ServerState,
    session_id: TerminalSessionId,
    actor_device_id: DeviceId,
) -> bool {
    let actor_id = actor_device_id.to_string();
    state
        .store
        .snapshot()
        .await
        .ok()
        .and_then(|snapshot| {
            snapshot
                .terminals
                .into_iter()
                .find(|session| session.id == session_id)
                .and_then(|session| session.lease)
        })
        .is_some_and(|lease| {
            lease.actor.kind == ActorKind::Device
                && lease.actor.id.as_deref() == Some(actor_id.as_str())
                && lease.expires_at.parse::<u128>().unwrap_or_default() > now() as u128
        })
}
