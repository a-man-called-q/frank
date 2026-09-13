//! The event cursor endpoint and its WebSocket fan-out.

use crate::auth::*;
use crate::*;
use futures_util::StreamExt;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);
const AUTH_RECHECK_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Deserialize)]
pub(crate) struct EventQuery {
    after: Option<u64>,
}

pub(crate) async fn events(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Query(query): Query<EventQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    // Credentials stay in the TLS-protected header; accepting a token in the
    // query string would leak it through proxy/access logs and browser history.
    let Some(token) = bearer(&headers).map(str::to_owned) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if authenticate(&state, &headers).await.is_none() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let after = query.after.unwrap_or_default();
    ws.on_upgrade(move |socket| event_socket(socket, state, after, token))
        .into_response()
}

pub(crate) async fn event_socket(
    mut socket: WebSocket,
    state: ServerState,
    mut cursor: u64,
    token: String,
) {
    // Subscribe before the initial replay. If a commit races the replay, the
    // durable query observes it; if it commits just after the query, this
    // receiver observes the wakeup rather than relying on a polling delay.
    let mut event_wakeups = state.event_wakeups.subscribe();
    let mut keepalive = tokio::time::interval(KEEPALIVE_INTERVAL);
    let mut auth_recheck = tokio::time::interval(AUTH_RECHECK_INTERVAL);
    // Tokio intervals tick immediately on their first poll. The initial replay
    // below owns the first ping, so consume those initial ticks here.
    keepalive.tick().await;
    auth_recheck.tick().await;

    let mut last_auth_check = Instant::now();
    let mut replay = true;
    let batch_size = state.config.max_event_batch.max(1) as usize;

    loop {
        if replay {
            if last_auth_check.elapsed() >= AUTH_RECHECK_INTERVAL
                && !authenticated(&state, &token).await
            {
                let _ = socket.send(Message::Close(None)).await;
                break;
            }
            if last_auth_check.elapsed() >= AUTH_RECHECK_INTERVAL {
                last_auth_check = Instant::now();
            }

            let page = match state
                .store
                .events_after(cursor, state.config.max_event_batch)
                .await
            {
                Ok(page) => page,
                Err(_) => break,
            };
            if page.resync_required {
                let payload = serde_json::json!({
                    "error": ApiError::new(ErrorCode::ResyncRequired, "event cursor is outside retention; fetch a snapshot")
                });
                if socket
                    .send(Message::Text(payload.to_string().into()))
                    .await
                    .is_err()
                {
                    break;
                }
                break;
            }
            let page_size = page.events.len();
            for event in page.events {
                cursor = event.seq;
                let Ok(json) = serde_json::to_string(&event) else {
                    continue;
                };
                if socket.send(Message::Text(json.into())).await.is_err() {
                    return;
                }
            }

            // A full page may mean that more retained events are waiting. Keep
            // draining in sequence before sleeping for the next daemon wakeup.
            if page_size >= batch_size {
                continue;
            }
            replay = false;
            if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                break;
            }
        }

        tokio::select! {
            wakeup = event_wakeups.recv() => {
                match wakeup {
                    Ok(()) | Err(broadcast::error::RecvError::Lagged(_)) => replay = true,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            _ = keepalive.tick() => {
                if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
                    break;
                }
            }
            _ = auth_recheck.tick() => {
                if !authenticated(&state, &token).await {
                    let _ = socket.send(Message::Close(None)).await;
                    break;
                }
                last_auth_check = Instant::now();
            }
            message = socket.next() => {
                match message {
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Pong(_)))
                    | Some(Ok(Message::Text(_)))
                    | Some(Ok(Message::Binary(_))) => {}
                }
            }
        }
    }
}

async fn authenticated(state: &ServerState, token: &str) -> bool {
    state
        .auth
        .authenticate(token)
        .await
        .ok()
        .flatten()
        .is_some()
}
