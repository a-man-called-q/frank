use super::*;
use frank_protocol::*;
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::{connect_async_tls_with_config, tungstenite::Message};

pub struct EventStream {
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
}

#[derive(Debug, Deserialize)]
struct WireError {
    error: ApiError,
}

pub struct ReconnectingEvents {
    client: RemoteClient,
    cursor: u64,
    backoff: Duration,
    stream: Option<EventStream>,
}

impl EventStream {
    pub async fn next(&mut self) -> Result<Option<EventEnvelope>> {
        while let Some(message) = self.socket.next().await {
            match message? {
                Message::Text(text) => {
                    if text.len() > MAX_COMMAND_BODY_BYTES {
                        return Err(ClientError::Api(ApiError::new(
                            ErrorCode::PayloadTooLarge,
                            "event frame exceeds the client limit",
                        )));
                    }
                    if let Ok(error) = serde_json::from_str::<WireError>(&text) {
                        if error.error.code == ErrorCode::ResyncRequired {
                            return Err(ClientError::ResyncRequired);
                        }
                        return Err(ClientError::Api(error.error));
                    }
                    return Ok(Some(serde_json::from_str(&text)?));
                }
                Message::Close(_) => return Ok(None),
                Message::Ping(payload) => {
                    self.socket.send(Message::Pong(payload)).await?;
                }
                _ => {}
            }
        }
        Ok(None)
    }
}

/// Notifications emitted by the single connection actor.  Keeping the
/// transport lifecycle in one task prevents GUI screens from opening
/// competing WebSockets and makes server switching an explicit cancellation
/// boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionEvent {
    Connected {
        cursor: u64,
    },
    Event(Box<EventEnvelope>),
    /// A retention gap was repaired by fetching a fresh authoritative
    /// snapshot. The actor remains alive and resumes the event stream from
    /// this snapshot sequence, so callers do not need to race a second
    /// connection actor against the first one.
    Snapshot(Box<Snapshot>),
    ResyncRequired,
    TransportError {
        message: String,
    },
    Disconnected,
}

enum ActorCommand {
    Command {
        envelope: Box<CommandEnvelope>,
        response: oneshot::Sender<Result<CommandResponse>>,
    },
    Stop,
}

/// Handle for a running [`ConnectionActor`].  Commands are serialized through
/// one queue while the event stream remains independently ordered.
#[derive(Clone)]
pub struct ConnectionHandle {
    commands: mpsc::Sender<ActorCommand>,
    stopped: Arc<AtomicBool>,
}

impl ConnectionHandle {
    /// Return whether the actor has stopped and will not accept another
    /// command. GUI reconnect logic uses this to distinguish a live actor
    /// (which may already be repairing a WebSocket) from a completed actor
    /// whose handle is still held for lifecycle bookkeeping.
    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }

    pub async fn command_envelope(&self, envelope: CommandEnvelope) -> Result<CommandResponse> {
        if self.stopped.load(Ordering::Acquire) {
            return Err(ClientError::Api(ApiError::new(
                ErrorCode::Internal,
                "connection actor is stopped",
            )));
        }
        let (response, receiver) = oneshot::channel();
        self.commands
            .send(ActorCommand::Command {
                envelope: Box::new(envelope),
                response,
            })
            .await
            .map_err(|_| {
                ClientError::Api(ApiError::new(
                    ErrorCode::Internal,
                    "connection actor is unavailable",
                ))
            })?;
        receiver.await.map_err(|_| {
            ClientError::Api(ApiError::new(
                ErrorCode::Internal,
                "connection actor command was cancelled",
            ))
        })?
    }

    pub async fn command(
        &self,
        command: Command,
        expected_revision: Option<u64>,
    ) -> Result<CommandResponse> {
        self.command_envelope(CommandEnvelope {
            protocol_version: PROTOCOL_VERSION,
            command_id: CommandId::new(),
            expected_revision,
            command,
        })
        .await
    }

    /// Stop reconnect attempts and close the event stream.  Dropping the
    /// receiver is also safe: the actor notices the closed channel and exits.
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        let _ = self.commands.try_send(ActorCommand::Stop);
    }
}

impl RemoteClient {
    pub async fn event_stream(&self, after: u64) -> Result<EventStream> {
        let base = self.endpoint("/events");
        let mut url = base;
        url.set_scheme(if self.base.scheme() == "https" {
            "wss"
        } else {
            "ws"
        })
        .map_err(|_| ClientError::InvalidAddress("invalid websocket scheme".into()))?;
        url.query_pairs_mut()
            .append_pair("after", &after.to_string());
        let connector = self.websocket_connector()?;
        let request = self.websocket_request(url)?;
        let (socket, _) = connect_async_tls_with_config(request, None, false, connector).await?;
        Ok(EventStream { socket })
    }

    pub async fn reconnecting_events(&self, after: u64) -> ReconnectingEvents {
        ReconnectingEvents {
            client: self.clone(),
            cursor: after,
            backoff: Duration::from_millis(250),
            stream: None,
        }
    }
}

impl RemoteClient {
    /// Spawn the one background connection actor used by native clients.  It
    /// reconnects with bounded exponential backoff, resumes after `after`,
    /// and serializes all command requests through the same owned queue.
    pub fn spawn_connection_actor(
        &self,
        after: u64,
    ) -> (ConnectionHandle, mpsc::Receiver<ConnectionEvent>) {
        let client = self.clone();
        let stopped = Arc::new(AtomicBool::new(false));
        let actor_stopped = stopped.clone();
        let (commands, mut command_rx) = mpsc::channel(64);
        let (events_tx, events_rx) = mpsc::channel(256);
        tokio::spawn(async move {
            let mut cursor = after;
            let mut backoff = Duration::from_millis(250);
            let mut stream: Option<EventStream> = None;
            'actor: loop {
                if actor_stopped.load(Ordering::Acquire) {
                    break;
                }
                if stream.is_none() {
                    match client.event_stream(cursor).await {
                        Ok(next_stream) => {
                            stream = Some(next_stream);
                            backoff = Duration::from_millis(250);
                            if !try_emit_connection_event(
                                &events_tx,
                                ConnectionEvent::Connected { cursor },
                            ) {
                                break;
                            }
                        }
                        Err(error) => {
                            if !try_emit_connection_event(
                                &events_tx,
                                ConnectionEvent::TransportError {
                                    message: error.to_string(),
                                },
                            ) {
                                break;
                            }
                            tokio::select! {
                                _ = tokio::time::sleep(backoff) => {
                                    backoff = (backoff * 2).min(Duration::from_secs(30));
                                }
                                command = command_rx.recv() => {
                                    if !handle_actor_command(&client, command, &events_tx).await {
                                        break 'actor;
                                    }
                                }
                            }
                            continue;
                        }
                    }
                }

                let Some(active_stream) = stream.as_mut() else {
                    continue;
                };
                tokio::select! {
                    message = active_stream.next() => {
                        match message {
                            Ok(Some(event)) => {
                                cursor = event.seq;
                                if !try_emit_connection_event(
                                    &events_tx,
                                    ConnectionEvent::Event(Box::new(event)),
                                ) {
                                    break;
                                }
                            }
                            Ok(None) => {
                                stream = None;
                                if !try_emit_connection_event(&events_tx, ConnectionEvent::Disconnected) {
                                    break;
                                }
                                tokio::select! {
                                    _ = tokio::time::sleep(backoff) => {
                                        backoff = (backoff * 2).min(Duration::from_secs(30));
                                    }
                                    command = command_rx.recv() => {
                                        if !handle_actor_command(&client, command, &events_tx).await {
                                            break 'actor;
                                        }
                                    }
                                }
                            }
                            Err(ClientError::ResyncRequired) => {
                                stream = None;
                                if !try_emit_connection_event(
                                    &events_tx,
                                    ConnectionEvent::ResyncRequired,
                                ) {
                                    break;
                                }
                                match client.snapshot().await {
                                    Ok(snapshot) => {
                                        cursor = snapshot.event_seq;
                                        if !try_emit_connection_event(
                                            &events_tx,
                                            ConnectionEvent::Snapshot(Box::new(snapshot)),
                                        ) {
                                            break;
                                        }
                                        backoff = Duration::from_millis(250);
                                    }
                                    Err(error) => {
                                        if !try_emit_connection_event(
                                            &events_tx,
                                            ConnectionEvent::TransportError {
                                                message: error.to_string(),
                                            },
                                        ) {
                                            break;
                                        }
                                        tokio::select! {
                                            _ = tokio::time::sleep(backoff) => {
                                                backoff = (backoff * 2).min(Duration::from_secs(30));
                                            }
                                            command = command_rx.recv() => {
                                                if !handle_actor_command(&client, command, &events_tx).await {
                                                    break 'actor;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            Err(error) => {
                                stream = None;
                                if !try_emit_connection_event(
                                    &events_tx,
                                    ConnectionEvent::TransportError { message: error.to_string() },
                                ) {
                                    break;
                                }
                                // Do not spin a reconnect loop when the
                                // daemon is down or a VPN is flapping.
                                tokio::select! {
                                    _ = tokio::time::sleep(backoff) => {
                                        backoff = (backoff * 2).min(Duration::from_secs(30));
                                    }
                                    command = command_rx.recv() => {
                                        if !handle_actor_command(&client, command, &events_tx).await {
                                            break 'actor;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    command = command_rx.recv() => {
                        if !handle_actor_command(&client, command, &events_tx).await {
                            break;
                        }
                    }
                }
            }
            actor_stopped.store(true, Ordering::Release);
            let _ = try_emit_connection_event(&events_tx, ConnectionEvent::Disconnected);
        });
        (ConnectionHandle { commands, stopped }, events_rx)
    }
}
/// Deliver lifecycle notifications without allowing a slow GUI to stall the
/// event reader. Once the bounded queue fills, a resync marker is attempted
/// and the actor stops; the client must fetch a fresh snapshot before it can
/// safely consume more events.
fn try_emit_connection_event(
    events: &mpsc::Sender<ConnectionEvent>,
    event: ConnectionEvent,
) -> bool {
    match events.try_send(event) {
        Ok(()) => true,
        Err(mpsc::error::TrySendError::Full(_)) => {
            let _ = events.try_send(ConnectionEvent::ResyncRequired);
            false
        }
        Err(mpsc::error::TrySendError::Closed(_)) => false,
    }
}

async fn handle_actor_command(
    client: &RemoteClient,
    command: Option<ActorCommand>,
    events: &mpsc::Sender<ConnectionEvent>,
) -> bool {
    let Some(command) = command else {
        return false;
    };
    match command {
        ActorCommand::Stop => false,
        ActorCommand::Command { envelope, response } => {
            let result = client.command_envelope(*envelope).await;
            let _ = response.send(result);
            !events.is_closed()
        }
    }
}

impl ReconnectingEvents {
    pub async fn next(&mut self) -> Result<Option<EventEnvelope>> {
        loop {
            if self.stream.is_none() {
                match self.client.event_stream(self.cursor).await {
                    Ok(stream) => {
                        self.stream = Some(stream);
                        self.backoff = Duration::from_millis(250);
                    }
                    Err(error) => {
                        tokio::time::sleep(self.backoff).await;
                        self.backoff = (self.backoff * 2).min(Duration::from_secs(30));
                        if matches!(
                            error,
                            ClientError::InsecureTransport | ClientError::CertificatePinMismatch
                        ) {
                            return Err(error);
                        }
                        continue;
                    }
                }
            }
            let stream = self.stream.as_mut().expect("stream initialized");
            match stream.next().await {
                Ok(Some(event)) => {
                    self.cursor = event.seq;
                    return Ok(Some(event));
                }
                Ok(None) => self.stream = None,
                Err(ClientError::ResyncRequired) => return Err(ClientError::ResyncRequired),
                Err(error) => {
                    self.stream = None;
                    if matches!(error, ClientError::CertificatePinMismatch) {
                        return Err(error);
                    }
                }
            }
        }
    }

    pub fn cursor(&self) -> u64 {
        self.cursor
    }
}
