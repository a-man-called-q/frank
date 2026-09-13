use super::*;
use frank_protocol::*;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async_tls_with_config, tungstenite::Message};

pub struct TerminalStream {
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
}

impl RemoteClient {
    pub async fn terminal_stream(&self, session_id: TerminalSessionId) -> Result<TerminalStream> {
        self.terminal_stream_after(session_id, None).await
    }

    /// Open a terminal stream and request only frames after `after`. The
    /// server replies with a terminal resync frame when retention has already
    /// discarded the requested sequence.
    pub async fn terminal_stream_after(
        &self,
        session_id: TerminalSessionId,
        after: Option<TerminalSequence>,
    ) -> Result<TerminalStream> {
        let mut url = self.endpoint(&format!("/terminals/{session_id}"));
        url.set_scheme(if self.base.scheme() == "https" {
            "wss"
        } else {
            "ws"
        })
        .map_err(|_| ClientError::InvalidAddress("invalid websocket scheme".into()))?;
        if let Some(after) = after {
            url.query_pairs_mut()
                .append_pair("after", &after.0.to_string());
        }
        let connector = self.websocket_connector()?;
        let request = self.websocket_request(url)?;
        let (socket, _) = connect_async_tls_with_config(request, None, false, connector).await?;
        Ok(TerminalStream { socket })
    }
}

impl TerminalStream {
    pub async fn send(&mut self, frame: &TerminalFrame) -> Result<()> {
        let json = serde_json::to_string(frame)?;
        if json.len() > MAX_TERMINAL_FRAME_BYTES {
            return Err(ClientError::Api(ApiError::new(
                ErrorCode::PayloadTooLarge,
                "terminal frame exceeds the client limit",
            )));
        }
        self.socket.send(Message::Text(json.into())).await?;
        Ok(())
    }

    pub async fn next(&mut self) -> Result<Option<TerminalFrame>> {
        while let Some(message) = self.socket.next().await {
            match message? {
                Message::Text(text) => return Ok(Some(serde_json::from_str(&text)?)),
                Message::Close(_) => return Ok(None),
                Message::Ping(payload) => {
                    // Drive the pong explicitly. This keeps a long-lived
                    // terminal viewer alive behind proxies that enforce the
                    // WebSocket heartbeat deadline.
                    self.socket.send(Message::Pong(payload)).await?;
                }
                _ => {}
            }
        }
        Ok(None)
    }
}
