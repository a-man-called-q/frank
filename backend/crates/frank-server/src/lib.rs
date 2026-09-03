//! Authenticated HTTPS/WebSocket server for Frank.
//!
//! `frankd` is the only process allowed to own SQLite, provider processes,
//! PTYs, and Git mutations.  The router is intentionally thin: authorization
//! and state transitions live in `frank-orchestrator`, while this crate owns
//! transport, pairing, reconnect cursors, and artifact/event streaming.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::{Body, Bytes};
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{
    ConnectInfo, DefaultBodyLimit, Path as AxumPath, Query, State, WebSocketUpgrade,
};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use frank_agent::ShellSpec;
use frank_agent::terminal::PtySession;
use frank_protocol::*;
use frank_store::Store;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::{Mutex, broadcast};

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("store error: {0}")]
    Store(#[from] frank_store::StoreError),
    #[error("orchestrator error: {0}")]
    Orchestrator(#[from] frank_orchestrator::OrchestratorError),
    #[error("server IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TLS configuration error: {0}")]
    Tls(String),
    #[error("remote plaintext bind is disabled; configure TLS before binding non-loopback")]
    PlaintextRemote,
}

pub type Result<T> = std::result::Result<T, ServerError>;

mod auth;
mod pairing;
mod routes;
mod tls;

use routes::artifacts::*;
use routes::browse::*;
use routes::commands::*;
use routes::devices::*;
use routes::diagnostics::*;
use routes::events::*;
use routes::pair::*;
use routes::terminals::*;

pub use pairing::{DeviceAuth, PairingManager, PairingSecret, PairingTicket};
pub use tls::TlsIdentity;
use tls::{install_crypto_provider, load_or_create_local_identity};

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind: SocketAddr,
    pub tls: Option<TlsIdentity>,
    pub server_version: String,
    pub max_event_batch: u32,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: SocketAddr::from(([127, 0, 0, 1], 37_465)),
            tls: None,
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            max_event_batch: 128,
        }
    }
}

impl ServerConfig {
    pub fn validate(&self) -> Result<()> {
        let tls_ready = self
            .tls
            .as_ref()
            .is_some_and(|tls| !tls.certificate_pem.is_empty() && !tls.private_key_pem.is_empty());
        if !self.bind.ip().is_loopback() && !tls_ready {
            return Err(ServerError::PlaintextRemote);
        }
        Ok(())
    }

    pub fn fingerprint(&self) -> String {
        self.tls
            .as_ref()
            .map(|identity| {
                if identity.fingerprint.is_empty() {
                    TlsIdentity::fingerprint_for_pem(&identity.certificate_pem)
                } else {
                    identity.fingerprint.clone()
                }
            })
            .unwrap_or_default()
    }
}

#[derive(Clone)]
pub struct ServerState {
    pub store: Store,
    pub orchestrator: Arc<frank_orchestrator::Orchestrator>,
    pub pairing: PairingManager,
    pub config: ServerConfig,
    pub terminal_sessions: Arc<Mutex<HashMap<TerminalSessionId, Arc<Mutex<PtySession>>>>>,
    /// A single daemon-owned reader pumps each PTY into this broadcast
    /// stream.  Viewers may disconnect without consuming output; the pump
    /// still appends every chunk to the durable transcript for replay.
    pub terminal_streams: Arc<Mutex<HashMap<TerminalSessionId, broadcast::Sender<TerminalFrame>>>>,
}

impl ServerState {
    pub async fn new(store: Store, mut config: ServerConfig) -> Result<Self> {
        install_crypto_provider();
        if config.tls.is_none() && config.bind.ip().is_loopback() {
            config.tls = Some(load_or_create_local_identity(&store)?);
        }
        let fingerprint = config.fingerprint();
        if store.certificate_fingerprint().await? != fingerprint {
            store.set_certificate_fingerprint(&fingerprint).await?;
        }
        let mut boot_snapshot = store.snapshot().await?;
        boot_snapshot.server.tls_fingerprint = fingerprint.clone();
        boot_snapshot.server.bind_address = config.bind.ip().to_string();
        boot_snapshot.server.port = config.bind.port();
        store.replace_snapshot(&boot_snapshot).await?;
        let orchestrator = Arc::new(frank_orchestrator::Orchestrator::new(store.clone()));
        orchestrator.ensure_builtin_supervisor().await?;
        let certificate_pem = config
            .tls
            .as_ref()
            .and_then(|tls| String::from_utf8(tls.certificate_pem.clone()).ok());
        let pairing = PairingManager::new_with_store(
            store.server_id(),
            fingerprint,
            certificate_pem,
            store.clone(),
        );
        pairing.load_persisted().await?;
        Ok(Self {
            store: store.clone(),
            orchestrator,
            pairing,
            config,
            terminal_sessions: Arc::new(Mutex::new(HashMap::new())),
            terminal_streams: Arc::new(Mutex::new(HashMap::new())),
        })
    }
}

pub fn build_router(state: ServerState) -> Router {
    Router::new()
        .route("/v1/health", get(health))
        .route("/v1/diagnostics", get(diagnostics))
        .route("/v1/handshake", post(handshake))
        .route("/v1/capabilities", get(capabilities))
        .route("/v1/pair", post(pair))
        .route("/v1/pair/prepare", post(pair_prepare))
        .route("/v1/snapshot", get(snapshot))
        .route("/v1/commands", post(commands))
        .route("/v1/artifacts/{id}", get(artifact))
        .route(
            "/v1/artifact-uploads/{id}",
            put(artifact_upload_chunk).layer(DefaultBodyLimit::max(MAX_TERMINAL_FRAME_BYTES * 4)),
        )
        .route("/v1/projects/browse", get(browse_projects))
        .route("/v1/devices", get(devices))
        .route("/v1/devices/{id}/revoke", post(revoke_device))
        .route("/v1/events", get(events))
        .route("/v1/terminals/{session_id}", get(terminals))
        // Command and pairing JSON is deliberately much smaller than the
        // artifact retention cap. Large outputs should be published through
        // an artifact transfer in a future protocol revision, not embedded
        // in a mutation envelope.
        .layer(DefaultBodyLimit::max(MAX_COMMAND_BODY_BYTES))
        .with_state(state)
}

fn status_for_error(code: ErrorCode) -> StatusCode {
    match code {
        ErrorCode::Unauthorized
        | ErrorCode::PairingExpired
        | ErrorCode::PairingReused
        | ErrorCode::CertificateMismatch => StatusCode::UNAUTHORIZED,
        ErrorCode::Forbidden => StatusCode::FORBIDDEN,
        ErrorCode::Validation | ErrorCode::PayloadTooLarge => StatusCode::BAD_REQUEST,
        ErrorCode::NotFound => StatusCode::NOT_FOUND,
        ErrorCode::Conflict | ErrorCode::StaleRevision | ErrorCode::LeaseUnavailable => {
            StatusCode::CONFLICT
        }
        ErrorCode::ResyncRequired | ErrorCode::VersionMismatch => StatusCode::UPGRADE_REQUIRED,
        ErrorCode::BudgetExceeded => StatusCode::UNPROCESSABLE_ENTITY,
        ErrorCode::ProviderUnavailable => StatusCode::SERVICE_UNAVAILABLE,
        ErrorCode::Internal => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn api_error_response(status: StatusCode, error: ApiError) -> axum::response::Response {
    (status, Json(error)).into_response()
}

fn hash(value: &str) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(value.as_bytes());
    digest.finalize().into()
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Run the server with TLS when configured.  A loopback-only plaintext mode
/// remains available for deterministic local tests; `validate()` rejects it
/// for any LAN/VPN/Tailscale address.
pub async fn run(state: ServerState) -> Result<()> {
    state.config.validate()?;
    spawn_background_exporter(&state);
    spawn_runtime_reconciler(&state);
    let app = build_router(state.clone());
    if let Some(tls) = &state.config.tls
        && !tls.certificate_pem.is_empty()
        && !tls.private_key_pem.is_empty()
    {
        let tls_config = axum_server::tls_rustls::RustlsConfig::from_pem(
            tls.certificate_pem.clone(),
            tls.private_key_pem.clone(),
        )
        .await
        .map_err(|error| ServerError::Tls(error.to_string()))?;
        axum_server::bind_rustls(state.config.bind, tls_config)
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await
            .map_err(|error| ServerError::Io(std::io::Error::other(error.to_string())))?;
        return Ok(());
    }
    let listener = tokio::net::TcpListener::bind(state.config.bind).await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .map_err(ServerError::Io)
}

fn canonicalize_allow_missing(path: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    let mut missing = Vec::new();
    let mut current = path;
    while std::fs::symlink_metadata(current).is_err() {
        let Some(name) = current.file_name() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "path has no existing ancestor",
            ));
        };
        missing.push(name.to_os_string());
        current = current.parent().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "path has no parent")
        })?;
    }
    let mut resolved = std::fs::canonicalize(current)?;
    for component in missing.iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

fn spawn_background_exporter(state: &ServerState) {
    let Some(database) = state
        .store
        .database_path()
        .map(std::path::Path::to_path_buf)
    else {
        return;
    };
    let audit = database
        .parent()
        .map(|parent| parent.join("audit.jsonl"))
        .unwrap_or_else(|| std::path::PathBuf::from("audit.jsonl"));
    let store = state.store.clone();
    tokio::spawn(async move {
        loop {
            let _ = store.export_audit_once(&audit, 256).await;
            // SQLite is authoritative, but structured events are deliberately
            // retained only for the configured reconnect window. Export first
            // so pruning can never discard an audit row that has not reached
            // JSONL yet; the snapshot remains available for resync after the
            // cursor falls outside this window.
            if let Ok(snapshot) = store.snapshot().await {
                let retention_days = snapshot.server.event_retention_days.max(1) as u64;
                let retention_ms = retention_days.saturating_mul(86_400_000);
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                let cutoff = now_ms.saturating_sub(retention_ms).to_string();
                let _ = store.prune_events_before(&cutoff).await;
                let terminal_days = snapshot.server.terminal_retention_days.max(1) as u64;
                let terminal_cutoff = now_ms
                    .saturating_sub(terminal_days.saturating_mul(86_400_000))
                    .to_string();
                let _ = store
                    .prune_terminal_transcript_before(&terminal_cutoff)
                    .await;
                let artifact_days = snapshot.server.artifact_retention_days.max(1) as u64;
                let artifact_cutoff = now_ms
                    .saturating_sub(artifact_days.saturating_mul(86_400_000))
                    .to_string();
                let _ = store.prune_artifacts_before(&artifact_cutoff).await;
                // Keep terminal operation history bounded while retaining all
                // queued/running/recovering rows needed by the reconciler.
                let operation_cutoff = now_ms
                    .saturating_sub(retention_ms.max(86_400_000))
                    .to_string();
                let _ = store.prune_operations_before(&operation_cutoff).await;
            }
            let _ = store
                .prune_expired_uploads(
                    &std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                        .to_string(),
                )
                .await;
            let _ = store
                .prune_agent_capabilities(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                )
                .await;
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
}

fn spawn_runtime_reconciler(state: &ServerState) {
    let orchestrator = state.orchestrator.clone();
    tokio::spawn(async move {
        // The daemon owns this loop; no GUI connection is involved. Bounded
        // polling keeps crash recovery deterministic while SQLite remains the
        // source of truth for every transition.
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(500));
        loop {
            ticker.tick().await;
            let _ = orchestrator.reconcile().await;
        }
    });
}

#[cfg(test)]
mod tests {
    use std::net::IpAddr;

    use super::*;

    #[tokio::test]
    async fn pairing_is_single_use_and_first_device_is_owner() {
        let manager = PairingManager::new(ServerId::nil(), "fingerprint".into());
        let observer = manager.prepare(DeviceRole::Observer).await;
        let request = PairingRequest {
            protocol_version: PROTOCOL_VERSION,
            secret: observer.secret,
            certificate_fingerprint: "fingerprint".into(),
            requested_role: DeviceRole::Observer,
            device_name: "laptop".into(),
        };
        assert!(matches!(
            manager.pair(&request).await,
            Err(ApiError {
                code: ErrorCode::Forbidden,
                ..
            })
        ));

        let owner = manager.prepare(DeviceRole::Owner).await;
        let request = PairingRequest {
            protocol_version: PROTOCOL_VERSION,
            secret: owner.secret,
            certificate_fingerprint: "fingerprint".into(),
            requested_role: DeviceRole::Owner,
            device_name: "desktop".into(),
        };
        let response = manager.pair(&request).await.unwrap();
        assert_eq!(response.role, DeviceRole::Owner);
        assert!(matches!(
            manager.pair(&request).await,
            Err(ApiError {
                code: ErrorCode::PairingReused,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn multiple_owner_devices_can_be_paired_after_bootstrap() {
        let manager = PairingManager::new(ServerId::nil(), "fingerprint".into());
        let first = manager.prepare(DeviceRole::Owner).await;
        manager
            .pair(&PairingRequest {
                protocol_version: PROTOCOL_VERSION,
                secret: first.secret,
                certificate_fingerprint: "fingerprint".into(),
                requested_role: DeviceRole::Owner,
                device_name: "desktop".into(),
            })
            .await
            .unwrap();

        let second = manager.prepare(DeviceRole::Owner).await;
        let response = manager
            .pair(&PairingRequest {
                protocol_version: PROTOCOL_VERSION,
                secret: second.secret,
                certificate_fingerprint: "fingerprint".into(),
                requested_role: DeviceRole::Owner,
                device_name: "laptop".into(),
            })
            .await
            .unwrap();
        assert_eq!(response.role, DeviceRole::Owner);
        assert_eq!(manager.devices().await.len(), 2);
    }

    #[tokio::test]
    async fn persisted_pairing_survives_manager_reload() {
        let store = Store::open_in_memory().await.unwrap();
        let manager = PairingManager::new_with_store(
            store.clone().server_id(),
            "fingerprint".into(),
            None,
            store.clone(),
        );
        let ticket = manager.prepare(DeviceRole::Owner).await;
        let request = PairingRequest {
            protocol_version: PROTOCOL_VERSION,
            secret: ticket.secret,
            certificate_fingerprint: "fingerprint".into(),
            requested_role: DeviceRole::Owner,
            device_name: "desktop".into(),
        };
        manager.pair(&request).await.unwrap();

        let reloaded =
            PairingManager::new_with_store(store.server_id(), "fingerprint".into(), None, store);
        reloaded.load_persisted().await.unwrap();
        assert_eq!(reloaded.devices().await.len(), 1);
    }

    #[tokio::test]
    async fn persisted_pairing_reuse_survives_manager_reload() {
        let store = Store::open_in_memory().await.unwrap();
        let manager = PairingManager::new_with_store(
            store.clone().server_id(),
            "fingerprint".into(),
            None,
            store.clone(),
        );
        let ticket = manager.prepare(DeviceRole::Owner).await;
        let request = PairingRequest {
            protocol_version: PROTOCOL_VERSION,
            secret: ticket.secret,
            certificate_fingerprint: "fingerprint".into(),
            requested_role: DeviceRole::Owner,
            device_name: "desktop".into(),
        };
        manager.pair(&request).await.unwrap();

        let reloaded =
            PairingManager::new_with_store(store.server_id(), "fingerprint".into(), None, store);
        reloaded.load_persisted().await.unwrap();
        assert!(matches!(
            reloaded.pair(&request).await,
            Err(ApiError {
                code: ErrorCode::PairingReused,
                ..
            })
        ));
    }

    #[tokio::test]
    async fn loopback_tls_identity_survives_daemon_restart() {
        let root = tempfile::tempdir().unwrap();
        let database = root.path().join("frank.sqlite3");
        let first_store = Store::open(&database).await.unwrap();
        let first = ServerState::new(first_store, ServerConfig::default())
            .await
            .unwrap();
        let first_fingerprint = first.config.fingerprint();
        assert!(root.path().join("server-cert.pem").is_file());
        assert!(root.path().join("server-key.pem").is_file());
        drop(first);

        let second_store = Store::open(&database).await.unwrap();
        let second = ServerState::new(second_store, ServerConfig::default())
            .await
            .unwrap();
        assert_eq!(second.config.fingerprint(), first_fingerprint);
    }

    #[test]
    fn remote_plaintext_is_rejected() {
        let config = ServerConfig {
            bind: SocketAddr::new(IpAddr::from([192, 168, 1, 10]), 37_465),
            tls: None,
            ..Default::default()
        };
        assert!(matches!(
            config.validate(),
            Err(ServerError::PlaintextRemote)
        ));
    }

    #[tokio::test]
    async fn handshake_rejects_a_protocol_gap_before_running_provider_probes() {
        let state = ServerState::new(
            Store::open_in_memory().await.unwrap(),
            ServerConfig::default(),
        )
        .await
        .unwrap();
        let response = handshake(
            axum::extract::State(state),
            Json(HandshakeRequest {
                protocol_version: 2,
                client_version: "2.0.0".into(),
                client_kind: "test".into(),
                supported_versions: VersionRange { min: 2, max: 3 },
            }),
        )
        .await
        .into_response();
        assert_eq!(response.status(), StatusCode::UPGRADE_REQUIRED);
    }

    #[test]
    fn content_range_must_match_contiguous_upload_chunk() {
        assert!(validate_content_range("bytes 0-3/8", 0, 4, 8).is_ok());
        assert!(validate_content_range("bytes 4-7/8", 4, 4, 8).is_ok());
        assert!(validate_content_range("bytes 1-4/8", 0, 4, 8).is_err());
        assert!(validate_content_range("bytes 0-3/9", 0, 4, 8).is_err());
    }
}
