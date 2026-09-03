//! The event cursor endpoint and its WebSocket fan-out.

use crate::auth::*;
use crate::*;

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
    let token = bearer(&headers);
    let Some(token) = token else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    if state.pairing.authenticate(token).await.is_none() {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let after = query.after.unwrap_or_default();
    ws.on_upgrade(move |socket| event_socket(socket, state, after))
        .into_response()
}

pub(crate) async fn event_socket(mut socket: WebSocket, state: ServerState, mut cursor: u64) {
    loop {
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
        for event in page.events {
            cursor = event.seq;
            let Ok(json) = serde_json::to_string(&event) else {
                continue;
            };
            if socket.send(Message::Text(json.into())).await.is_err() {
                return;
            }
        }
        if socket.send(Message::Ping(Vec::new().into())).await.is_err() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}
