//! Reconnecting native client for Frank's versioned HTTPS/WebSocket API.
//!
//! This crate contains no GUI dependencies and no frank-app dependency. A
//! local GUI uses the exact same HTTPS path as a laptop connecting over LAN or
//! VPN; there is no direct in-process service shortcut.

// ClientError keeps the underlying reqwest/WebSocket sources intact so callers
// get useful diagnostics. Boxing every source would make the public error
// conversion noisy without changing the transport contract.
#![allow(clippy::result_large_err)]

use frank_protocol::ApiError;
use reqwest::{Client, Url};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("invalid server address: {0}")]
    InvalidAddress(String),
    #[error("insecure transport is disabled; use HTTPS")]
    InsecureTransport,
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),
    #[error("WebSocket request failed: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("protocol payload failed to decode: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("server returned an API error: {0:?}")]
    Api(ApiError),
    #[error("server certificate fingerprint mismatch")]
    CertificatePinMismatch,
    #[error("server requires a resync snapshot")]
    ResyncRequired,
    #[error("server certificate could not be loaded: {0}")]
    Tls(String),
}

pub type Result<T> = std::result::Result<T, ClientError>;

mod config;
mod events;
mod http;
mod terminal;
mod tls;

pub use config::{
    ArtifactUploadChunkResponse, BrowseEntry, BrowseResponse, ClientConfig, DeviceSummary,
};
// Kept as a compatibility re-export while credential persistence lives in a
// leaf crate that the daemon can depend on without depending on HTTP/TLS.
pub use events::{ConnectionEvent, ConnectionHandle, EventStream, ReconnectingEvents};
pub use frank_credential::{
    CredentialError, CredentialStore, DAEMON_LABEL, DAEMON_SERVICE, DESKTOP_LABEL, DESKTOP_SERVICE,
    FileCredentialStore, MAX_CREDENTIAL_BYTES, NativeCredentialBackend, NativeCredentialStore,
};
pub use terminal::TerminalStream;

#[derive(Debug, Clone)]
pub struct RemoteClient {
    config: ClientConfig,
    http: Client,
    base: Url,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plaintext_requires_explicit_loopback_opt_in() {
        assert!(matches!(
            RemoteClient::new(ClientConfig::new("http://127.0.0.1:37465")),
            Err(ClientError::InsecureTransport)
        ));
        assert!(
            RemoteClient::new(ClientConfig::new("http://127.0.0.1:37465").allow_insecure_local())
                .is_ok()
        );
    }

    #[test]
    fn remote_plaintext_is_never_allowed() {
        assert!(matches!(
            RemoteClient::new(ClientConfig::new("http://192.168.1.2:37465").allow_insecure_local()),
            Err(ClientError::InsecureTransport)
        ));
    }

    #[test]
    fn certificate_pins_are_shape_checked_before_transport() {
        let client = RemoteClient::new(
            ClientConfig::new("https://127.0.0.1:37465").with_pin("sha256:not-a-fingerprint"),
        );
        assert!(matches!(client, Err(ClientError::Tls(_))));
    }

    #[test]
    fn valid_certificate_pin_builds_http_client() {
        // Keep this regression test at the construction boundary: reqwest's
        // `use_preconfigured_tls` accepts an owned rustls ClientConfig, while
        // the WebSocket connector wraps the same config in an Arc later.
        let fingerprint = format!("sha256:{}", "ab".repeat(32));
        assert!(
            RemoteClient::new(ClientConfig::new("https://127.0.0.1:37465").with_pin(fingerprint))
                .is_ok()
        );
    }

    #[test]
    fn websocket_request_has_tungstenite_handshake_headers() {
        let client = RemoteClient::new(
            ClientConfig::new("https://127.0.0.1:37465")
                .with_pin(format!("sha256:{}", "ab".repeat(32)))
                .with_token("device-token"),
        )
        .expect("valid pin should build the client");
        let request = client
            .websocket_request(
                "wss://127.0.0.1:37465/v2/events?after=0"
                    .parse()
                    .expect("valid websocket URL"),
            )
            .expect("tungstenite should build the request");
        assert_eq!(request.method(), "GET");
        assert_eq!(request.headers()["connection"], "Upgrade");
        assert_eq!(request.headers()["upgrade"], "websocket");
        assert_eq!(request.headers()["sec-websocket-version"], "13");
        assert!(request.headers().contains_key("sec-websocket-key"));
        assert_eq!(request.headers()["authorization"], "Bearer device-token");
    }
}
