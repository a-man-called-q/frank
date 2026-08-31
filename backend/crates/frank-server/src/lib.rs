//! Authenticated HTTPS/WebSocket server for Frank.
//!
//! `frankd` is the only process allowed to own SQLite, provider processes,
//! PTYs, and Git mutations.  The router is intentionally thin: authorization
//! and state transitions live in `frank-orchestrator`, while this crate owns
//! transport, pairing, reconnect cursors, and artifact/event streaming.

use std::collections::HashMap;
use std::io::Cursor;
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

pub use frank_app::service;

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

#[derive(Debug, Clone)]
pub struct TlsIdentity {
    pub certificate_pem: Vec<u8>,
    pub private_key_pem: Vec<u8>,
    pub fingerprint: String,
}

impl TlsIdentity {
    pub fn fingerprint_for(certificate_der: &[u8]) -> String {
        let mut digest = Sha256::new();
        digest.update(certificate_der);
        format!("sha256:{}", hex::encode(digest.finalize()))
    }

    /// Compute the pin from the DER certificate contained in a PEM bundle.
    /// Falling back to the raw bytes keeps malformed custom identities
    /// diagnosable while preserving a deterministic fingerprint for them.
    pub fn fingerprint_for_pem(certificate_pem: &[u8]) -> String {
        let mut reader = Cursor::new(certificate_pem);
        if let Some(Ok(certificate)) = rustls_pemfile::certs(&mut reader).next() {
            return Self::fingerprint_for(&certificate);
        }
        Self::fingerprint_for(certificate_pem)
    }
}

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

#[derive(Debug, Clone)]
struct PendingPairing {
    secret_hash: [u8; 32],
    role: DeviceRole,
    certificate_fingerprint: String,
    expires_at: u64,
    used: bool,
}

#[derive(Debug, Clone)]
pub struct PairingTicket {
    pub secret: String,
    pub role: DeviceRole,
    pub certificate_fingerprint: String,
    pub expires_at: u64,
}

#[derive(Debug, Clone)]
pub struct DeviceAuth {
    pub device_id: DeviceId,
    pub role: DeviceRole,
    pub name: String,
}

#[derive(Debug, Clone)]
struct DeviceRecord {
    auth: DeviceAuth,
    token_hash: [u8; 32],
    revoked: bool,
    last_seen_at: u64,
}

#[derive(Clone)]
pub struct PairingManager {
    server_id: ServerId,
    fingerprint: String,
    certificate_pem: Option<String>,
    store: Option<Store>,
    pending: Arc<Mutex<HashMap<String, PendingPairing>>>,
    devices: Arc<Mutex<HashMap<DeviceId, DeviceRecord>>>,
    attempts: Arc<Mutex<HashMap<String, (u64, u8)>>>,
}

impl PairingManager {
    pub fn new(server_id: ServerId, fingerprint: String) -> Self {
        Self::new_with_certificate(server_id, fingerprint, None)
    }

    pub fn new_with_certificate(
        server_id: ServerId,
        fingerprint: String,
        certificate_pem: Option<String>,
    ) -> Self {
        Self {
            server_id,
            fingerprint,
            certificate_pem,
            store: None,
            pending: Arc::new(Mutex::new(HashMap::new())),
            devices: Arc::new(Mutex::new(HashMap::new())),
            attempts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn new_with_store(
        server_id: ServerId,
        fingerprint: String,
        certificate_pem: Option<String>,
        store: Store,
    ) -> Self {
        let mut manager = Self::new_with_certificate(server_id, fingerprint, certificate_pem);
        manager.store = Some(store);
        manager
    }

    pub async fn load_persisted(&self) -> Result<()> {
        let Some(store) = &self.store else {
            return Ok(());
        };
        let now = now();
        let tickets = store.pairing_tickets().await?;
        let mut pending = self.pending.lock().await;
        for ticket in tickets {
            // Keep a used ticket in memory until its normal expiry.  This is
            // important for a durable one-time code: after a daemon restart a
            // second redemption must still be reported as `pairing-reused`,
            // rather than looking like an unknown/expired code.  Expired
            // unused tickets are intentionally not restored and can only
            // produce the generic expired response.
            if ticket.expires_at < now || ticket.certificate_fingerprint != self.fingerprint {
                continue;
            }
            let Ok(bytes) = hex::decode(&ticket.secret_hash) else {
                continue;
            };
            let Ok(secret_hash) = <[u8; 32]>::try_from(bytes.as_slice()) else {
                continue;
            };
            pending.insert(
                ticket.ticket_id,
                PendingPairing {
                    secret_hash,
                    role: ticket.role,
                    certificate_fingerprint: ticket.certificate_fingerprint,
                    expires_at: ticket.expires_at,
                    used: ticket.used,
                },
            );
        }
        drop(pending);
        let records = store.devices().await?;
        let mut valid = Vec::with_capacity(records.len());
        for record in records {
            // A certificate rotation is an explicit trust boundary. Existing
            // device tokens are not silently carried across that boundary.
            if record.certificate_fingerprint != self.fingerprint {
                let _ = store.revoke_device(record.device_id).await?;
                continue;
            }
            let Ok(bytes) = hex::decode(record.token_hash) else {
                continue;
            };
            let Ok(token_hash) = <[u8; 32]>::try_from(bytes.as_slice()) else {
                continue;
            };
            valid.push((
                record.device_id,
                DeviceRecord {
                    auth: DeviceAuth {
                        device_id: record.device_id,
                        role: record.role,
                        name: record.name,
                    },
                    token_hash,
                    revoked: record.revoked,
                    last_seen_at: record.last_seen_at,
                },
            ));
        }
        let mut devices = self.devices.lock().await;
        for (device_id, device) in valid {
            devices.insert(device_id, device);
        }
        Ok(())
    }

    pub async fn prepare(&self, role: DeviceRole) -> PairingTicket {
        self.prepare_inner(role, false)
            .await
            .expect("in-memory pairing ticket preparation cannot fail")
    }

    /// Prepare a ticket and require its durable row to be committed before
    /// returning the secret.  The HTTP endpoint uses this variant so a
    /// successful response can always survive a frankd restart.
    pub async fn prepare_durable(
        &self,
        role: DeviceRole,
    ) -> std::result::Result<PairingTicket, frank_store::StoreError> {
        self.prepare_inner(role, true).await
    }

    async fn prepare_inner(
        &self,
        role: DeviceRole,
        require_persistence: bool,
    ) -> std::result::Result<PairingTicket, frank_store::StoreError> {
        let secret = PairingSecret::generate();
        let expires_at = now().saturating_add(600);
        let key = hash(&secret);
        self.pending.lock().await.insert(
            hex::encode(key),
            PendingPairing {
                secret_hash: key,
                role,
                certificate_fingerprint: self.fingerprint.clone(),
                expires_at,
                used: false,
            },
        );
        if let Some(store) = &self.store {
            let result = store
                .upsert_pairing_ticket(&frank_store::StoredPairingTicket {
                    ticket_id: hex::encode(key),
                    secret_hash: hex::encode(key),
                    role,
                    certificate_fingerprint: self.fingerprint.clone(),
                    expires_at,
                    used: false,
                })
                .await;
            if let Err(error) = result {
                self.pending.lock().await.remove(&hex::encode(key));
                if require_persistence {
                    return Err(error);
                }
            }
        } else if require_persistence {
            self.pending.lock().await.remove(&hex::encode(key));
            return Err(frank_store::StoreError::Validation(
                "pairing persistence is unavailable".into(),
            ));
        }
        Ok(PairingTicket {
            secret,
            role,
            certificate_fingerprint: self.fingerprint.clone(),
            expires_at,
        })
    }

    pub async fn pair(
        &self,
        request: &PairingRequest,
    ) -> std::result::Result<PairingResponse, ApiError> {
        if request.protocol_version != PROTOCOL_VERSION {
            return Err(ApiError::new(
                ErrorCode::VersionMismatch,
                "pairing protocol version is unsupported",
            ));
        }
        if request.certificate_fingerprint != self.fingerprint {
            return Err(ApiError::new(
                ErrorCode::CertificateMismatch,
                "server certificate fingerprint does not match",
            ));
        }
        let key = hash(&request.secret);
        let key_string = hex::encode(key);
        {
            let now = now();
            let mut attempts = self.attempts.lock().await;
            let entry = attempts.entry(key_string.clone()).or_insert((now, 0));
            if now.saturating_sub(entry.0) >= 60 {
                *entry = (now, 0);
            }
            entry.1 = entry.1.saturating_add(1);
            if entry.1 > 5 {
                return Err(ApiError::new(
                    ErrorCode::Unauthorized,
                    "pairing rate limit exceeded; try again later",
                ));
            }
        }
        // Keep the pending-ticket lock until all checks pass and the ticket is
        // marked used. Without this critical section two simultaneous pair
        // requests could both redeem the same one-time secret.
        let mut pending = self.pending.lock().await;
        let ticket = pending.get_mut(&key_string).ok_or_else(|| {
            ApiError::new(
                ErrorCode::PairingExpired,
                "pairing code is unknown or expired",
            )
        })?;
        if ticket.used {
            return Err(ApiError::new(
                ErrorCode::PairingReused,
                "pairing code has already been used",
            ));
        }
        if ticket.expires_at < now() {
            return Err(ApiError::new(
                ErrorCode::PairingExpired,
                "pairing code has expired",
            ));
        }
        if ticket.secret_hash != key
            || ticket.certificate_fingerprint != request.certificate_fingerprint
        {
            return Err(ApiError::new(
                ErrorCode::CertificateMismatch,
                "pairing proof did not match",
            ));
        }
        if ticket.role != request.requested_role {
            return Err(ApiError::new(
                ErrorCode::Forbidden,
                "pairing role does not match the issued ticket",
            ));
        }
        let ticket_role = ticket.role;
        let ticket_fingerprint = ticket.certificate_fingerprint.clone();
        let ticket_expires_at = ticket.expires_at;
        {
            let devices = self.devices.lock().await;
            let has_owner = devices
                .values()
                .any(|device| device.auth.role == DeviceRole::Owner && !device.revoked);
            if !has_owner && ticket_role != DeviceRole::Owner {
                return Err(ApiError::new(
                    ErrorCode::Forbidden,
                    "the first paired device must be an owner",
                ));
            }
        }
        if request.device_name.trim().is_empty() || request.device_name.len() > 128 {
            return Err(ApiError::new(
                ErrorCode::Validation,
                "device name is invalid",
            ));
        }
        let device_id = DeviceId::new();
        let token = PairingSecret::generate();
        let token_hash = hash(&token);
        let auth = DeviceAuth {
            device_id,
            role: ticket_role,
            name: request.device_name.clone(),
        };
        let device = DeviceRecord {
            auth: auth.clone(),
            token_hash,
            revoked: false,
            last_seen_at: now(),
        };
        // Device insertion and one-time ticket consumption are one database
        // transaction. The in-memory lock still serializes concurrent pair
        // requests, while a database failure leaves both records unchanged.
        if let Some(store) = &self.store {
            let committed = store
                .complete_pairing(
                    &key_string,
                    &frank_store::StoredDevice {
                        device_id,
                        name: auth.name.clone(),
                        role: auth.role,
                        token_hash: hex::encode(token_hash),
                        certificate_fingerprint: request.certificate_fingerprint.clone(),
                        revoked: false,
                        last_seen_at: device.last_seen_at,
                    },
                )
                .await
                .map_err(|_| {
                    ApiError::new(ErrorCode::Internal, "pairing could not be persisted")
                })?;
            if !committed {
                return Err(ApiError::new(
                    ErrorCode::PairingReused,
                    "pairing code has already been used",
                ));
            }
        }
        ticket.used = true;
        drop(pending);
        self.attempts.lock().await.remove(&key_string);
        self.devices.lock().await.insert(device_id, device);
        Ok(PairingResponse {
            server_id: self.server_id,
            device_id,
            role: ticket_role,
            device_token: token,
            certificate_fingerprint: ticket_fingerprint,
            certificate_pem: self.certificate_pem.clone(),
            expires_at: ticket_expires_at.to_string(),
        })
    }

    pub async fn authenticate(&self, token: &str) -> Option<DeviceAuth> {
        let token_hash = hash(token);
        let mut devices = self.devices.lock().await;
        let authenticated = devices.values_mut().find_map(|device| {
            if !device.revoked && device.token_hash == token_hash {
                device.last_seen_at = now();
                Some(device.auth.clone())
            } else {
                None
            }
        });
        drop(devices);
        if let Some(auth) = &authenticated
            && let Some(store) = &self.store
        {
            let _ = store
                .upsert_device(&frank_store::StoredDevice {
                    device_id: auth.device_id,
                    name: auth.name.clone(),
                    role: auth.role,
                    token_hash: hex::encode(token_hash),
                    certificate_fingerprint: self.fingerprint.clone(),
                    revoked: false,
                    last_seen_at: now(),
                })
                .await;
        }
        authenticated
    }

    pub async fn revoke(&self, device_id: DeviceId) -> bool {
        let mut devices = self.devices.lock().await;
        let found = if let Some(device) = devices.get_mut(&device_id) {
            device.revoked = true;
            true
        } else {
            false
        };
        drop(devices);
        if found && let Some(store) = &self.store {
            let _ = store.revoke_device(device_id).await;
        }
        found
    }

    pub async fn devices(&self) -> Vec<(DeviceAuth, bool, u64)> {
        self.devices
            .lock()
            .await
            .values()
            .map(|device| (device.auth.clone(), device.revoked, device.last_seen_at))
            .collect()
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

/// Rustls 0.23 deliberately refuses to guess when both its `ring` and
/// `aws-lc-rs` backends are present.  Frank uses the aws-lc-rs provider for every
/// HTTPS/WebSocket process; install it before axum-server or a pinned client
/// builder asks Rustls for the process-wide provider.  Calling this more than
/// once is harmless because Rustls returns an error when another thread won
/// the one-time installation race.
fn install_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

/// Load the loopback identity from the versioned server data root, creating it
/// exactly once on first boot.  The certificate fingerprint is a trust anchor
/// for paired clients, so silently generating a new certificate after every
/// restart would revoke every device and create a confusing trust reset.
fn load_or_create_local_identity(store: &Store) -> Result<TlsIdentity> {
    let Some(data_root) = store.database_path().and_then(std::path::Path::parent) else {
        return generate_local_identity();
    };
    frank_safeio::ensure_dir(data_root).map_err(|error| ServerError::Tls(error.to_string()))?;
    let certificate_path = data_root.join("server-cert.pem");
    let key_path = data_root.join("server-key.pem");
    let certificate_exists = std::fs::symlink_metadata(&certificate_path).is_ok();
    let key_exists = std::fs::symlink_metadata(&key_path).is_ok();
    match (certificate_exists, key_exists) {
        (true, true) => {
            for path in [&certificate_path, &key_path] {
                if std::fs::symlink_metadata(path)
                    .map(|metadata| metadata.file_type().is_symlink())
                    .unwrap_or(false)
                {
                    return Err(ServerError::Tls(
                        "persisted TLS identity may not be a symlink".into(),
                    ));
                }
            }
            let certificate_pem =
                frank_safeio::read_text_capped(&certificate_path, frank_safeio::MAX_CONFIG_BYTES)
                    .map_err(|error| ServerError::Tls(error.to_string()))?
                    .into_bytes();
            let private_key_pem =
                frank_safeio::read_text_capped(&key_path, frank_safeio::MAX_CONFIG_BYTES)
                    .map_err(|error| ServerError::Tls(error.to_string()))?
                    .into_bytes();
            if certificate_pem.is_empty() || private_key_pem.is_empty() {
                return Err(ServerError::Tls(
                    "persisted TLS identity is empty".to_string(),
                ));
            }
            // Harden identities created by older development builds as well
            // as newly generated keys. A key that was accidentally left
            // group/world-readable must not remain usable merely because it
            // already exists on disk.
            set_private_key_permissions(&key_path)?;
            Ok(TlsIdentity {
                fingerprint: TlsIdentity::fingerprint_for_pem(&certificate_pem),
                certificate_pem,
                private_key_pem,
            })
        }
        (false, false) => {
            let identity = generate_local_identity()?;
            frank_safeio::write_text_atomic(
                &certificate_path,
                &String::from_utf8_lossy(&identity.certificate_pem),
                frank_safeio::MAX_CONFIG_BYTES,
            )
            .map_err(|error| ServerError::Tls(error.to_string()))?;
            frank_safeio::write_text_atomic(
                &key_path,
                &String::from_utf8_lossy(&identity.private_key_pem),
                frank_safeio::MAX_CONFIG_BYTES,
            )
            .map_err(|error| ServerError::Tls(error.to_string()))?;
            set_private_key_permissions(&key_path)?;
            Ok(identity)
        }
        _ => Err(ServerError::Tls(
            "persisted TLS certificate and private key must be provided together".to_string(),
        )),
    }
}

fn generate_local_identity() -> Result<TlsIdentity> {
    let certificate = rcgen::generate_simple_self_signed(vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ])
    .map_err(|error| ServerError::Tls(error.to_string()))?;
    let certificate_pem = certificate
        .serialize_pem()
        .map_err(|error| ServerError::Tls(error.to_string()))?
        .into_bytes();
    Ok(TlsIdentity {
        fingerprint: TlsIdentity::fingerprint_for_pem(&certificate_pem),
        certificate_pem,
        private_key_pem: certificate.serialize_private_key_pem().into_bytes(),
    })
}

fn set_private_key_permissions(path: &std::path::Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .map_err(ServerError::Io)?;
    }
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
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

#[derive(Debug, Serialize)]
struct Health {
    ok: bool,
    server_id: ServerId,
    protocol_version: u16,
    revision: u64,
}

async fn health(State(state): State<ServerState>) -> impl IntoResponse {
    // Keep liveness cheap but do not report a healthy daemon when SQLite has
    // failed integrity checks.  The detailed owner-only diagnostics endpoint
    // carries the rest of the component matrix; this endpoint is suitable for
    // service managers and load balancers.
    let database_ok = state.store.integrity_check().await.is_ok();
    let revision = state.store.current_revision().await.unwrap_or_default();
    Json(Health {
        ok: database_ok,
        server_id: state.store.server_id(),
        protocol_version: PROTOCOL_VERSION,
        revision,
    })
}

/// Return the owner-only, sanitized daemon diagnostic bundle.  The ordinary
/// health endpoint remains intentionally cheap and unauthenticated; this
/// endpoint is the detailed operational surface used by the GUI/CLI doctor
/// screens and never includes raw command output or server filesystem paths.
async fn diagnostics(State(state): State<ServerState>, headers: HeaderMap) -> impl IntoResponse {
    let Some(auth) = authenticate(&state, &headers).await else {
        return api_error_response(
            StatusCode::UNAUTHORIZED,
            ApiError::new(ErrorCode::Unauthorized, "device authentication required"),
        );
    };
    if !auth.role.can_admin() {
        return api_error_response(
            StatusCode::FORBIDDEN,
            ApiError::new(ErrorCode::Forbidden, "owner role required for diagnostics"),
        );
    }

    let generated_at = timestamp_now();
    let database = match state.store.integrity_check().await {
        Ok(()) => HealthStatus::Healthy,
        Err(_) => HealthStatus::Unhealthy,
    };
    let audit_backlog = state.store.audit_backlog_count().await.unwrap_or_default();
    let audit_exporter = if audit_backlog == 0 {
        HealthStatus::Healthy
    } else {
        HealthStatus::Degraded
    };
    let snapshot = state.store.snapshot().await.ok();
    let (retention, operation_backlog, git) = if let Some(snapshot) = snapshot {
        let operation_backlog = snapshot
            .operations
            .iter()
            .filter(|operation| {
                matches!(
                    operation.status,
                    OperationStatus::Queued
                        | OperationStatus::Running
                        | OperationStatus::Waiting
                        | OperationStatus::Recovering
                )
            })
            .count() as u64;
        let pending_uploads = snapshot
            .uploads
            .iter()
            .filter(|upload| !upload.completed)
            .count() as u64;
        let roots_ok = snapshot
            .server
            .allowed_project_roots
            .iter()
            .all(|root| std::fs::metadata(root).is_ok_and(|metadata| metadata.is_dir()));
        let projects_ok = snapshot
            .projects
            .iter()
            .filter(|project| !project.archived)
            .all(|project| {
                std::fs::metadata(&project.path).is_ok_and(|metadata| metadata.is_dir())
            });
        let git = if roots_ok && projects_ok {
            HealthStatus::Healthy
        } else {
            HealthStatus::Degraded
        };
        (
            RetentionView {
                event_days: snapshot.server.event_retention_days,
                terminal_days: snapshot.server.terminal_retention_days,
                artifact_days: snapshot.server.artifact_retention_days,
                terminal_max_bytes: 10 * 1024 * 1024,
                pending_uploads,
                pending_operations: operation_backlog,
            },
            operation_backlog,
            git,
        )
    } else {
        (
            RetentionView {
                event_days: 0,
                terminal_days: 0,
                artifact_days: 0,
                terminal_max_bytes: 10 * 1024 * 1024,
                pending_uploads: 0,
                pending_operations: 0,
            },
            0,
            HealthStatus::Unknown,
        )
    };
    let providers = state
        .orchestrator
        .runtime
        .doctor()
        .await
        .into_iter()
        .map(|probe| {
            let status = if probe.capability.available && probe.capability.logged_in {
                HealthStatus::Healthy
            } else if probe.capability.available {
                HealthStatus::Degraded
            } else {
                HealthStatus::Unhealthy
            };
            let detail = if probe.capability.available {
                if probe.capability.logged_in {
                    None
                } else {
                    Some("provider login is required".to_string())
                }
            } else {
                Some("provider executable or protocol is unavailable".to_string())
            };
            RuntimeDoctorCheck {
                component: probe.capability.provider.to_string(),
                status,
                version: probe.capability.version,
                detail,
                remediation: Some("run `frank server doctor` on the daemon host".to_string()),
            }
        })
        .collect();
    let installed = service::is_installed();
    let service = Some(ServiceStatusView {
        service_name: service::SERVICE_NAME.to_string(),
        installed,
        running: true,
        pid: std::process::id().into(),
        descriptor_path: None,
        health: if installed {
            HealthStatus::Healthy
        } else {
            HealthStatus::Degraded
        },
        detail: (!installed).then(|| {
            "frankd is running without a detected per-user service descriptor".to_string()
        }),
        checked_at: generated_at.clone(),
    });
    (
        StatusCode::OK,
        Json(DiagnosticSnapshot {
            generated_at,
            database,
            audit_exporter,
            git,
            providers,
            service,
            retention,
            operation_backlog,
            disk_free_bytes: None,
            redactions: vec![
                "provider environment and credentials omitted".into(),
                "server filesystem paths omitted".into(),
                "terminal input and bearer tokens omitted".into(),
            ],
        }),
    )
        .into_response()
}

async fn handshake(
    State(state): State<ServerState>,
    Json(request): Json<HandshakeRequest>,
) -> impl IntoResponse {
    let negotiated = match negotiate_versions(&request.supported_versions, &VersionRange::current())
    {
        Ok(version) if request.protocol_version == version => version,
        Ok(_) => {
            return api_error_response(
                StatusCode::UPGRADE_REQUIRED,
                ApiError::new(
                    ErrorCode::VersionMismatch,
                    "client protocol version is outside the negotiated range",
                ),
            );
        }
        Err(error) => {
            return api_error_response(StatusCode::UPGRADE_REQUIRED, error);
        }
    };
    let capabilities = capability_document(&state).await;
    (
        StatusCode::OK,
        Json(HandshakeResponse {
            negotiated_version: negotiated,
            server_id: capabilities.server_id,
            server_version: capabilities.server_version.clone(),
            capabilities,
        }),
    )
        .into_response()
}

async fn capabilities(State(state): State<ServerState>) -> impl IntoResponse {
    Json(capability_document(&state).await)
}

async fn capability_document(state: &ServerState) -> Capabilities {
    let probes = state.orchestrator.runtime.doctor().await;
    let providers = probes
        .into_iter()
        .map(|probe| {
            let mut capability = probe.capability;
            // Capabilities are intentionally unauthenticated bootstrap data.
            // Do not expose executable paths or provider stderr to an
            // unauthenticated peer; the owner-only diagnostics endpoint has
            // the actionable, sanitized doctor result.
            capability.executable = capability.executable.and_then(|executable| {
                std::path::Path::new(&executable)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_owned)
            });
            capability.diagnostic = capability.diagnostic.map(|_| {
                "provider is unavailable or requires login; run frank server doctor".into()
            });
            capability
        })
        .collect::<Vec<_>>();
    let settings = state
        .store
        .snapshot()
        .await
        .ok()
        .map(|snapshot| snapshot.server);
    let settings = settings.unwrap_or_default();
    Capabilities {
        protocol_version: PROTOCOL_VERSION,
        supported_versions: VersionRange::current(),
        minimum_compatible_client: MIN_COMPATIBLE_CLIENT,
        server_id: state.store.server_id(),
        certificate_fingerprint: state.config.fingerprint(),
        server_version: state.config.server_version.clone(),
        features: vec![
            "missions".into(),
            "kanban".into(),
            "agent-messaging".into(),
            "approvals".into(),
            "terminal-lease".into(),
            "terminal-replay".into(),
            "artifact-upload".into(),
            "durable-operations".into(),
            "signed-updates".into(),
            "audit-jsonl".into(),
        ],
        providers,
        limits: CapabilityLimits {
            max_concurrency: settings.max_concurrency,
            max_provider_concurrency: settings.max_provider_concurrency,
            ..CapabilityLimits::default()
        },
    }
}

async fn pair_prepare(
    State(state): State<ServerState>,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
    Json(role): Json<DeviceRole>,
) -> impl IntoResponse {
    // A pairing ticket is an authority bootstrap operation.  It may be
    // requested by the local CLI on a server bound to a LAN address, but a
    // remote peer must never be able to mint tickets without an existing
    // device token. The production listener installs the concrete peer
    // address below, and the router uses the same extractor in tests.
    if !address.ip().is_loopback() {
        return api_error_response(
            StatusCode::FORBIDDEN,
            ApiError::new(
                ErrorCode::Forbidden,
                "pairing tickets can only be minted by the server host",
            ),
        );
    }
    let ticket = match state.pairing.prepare_durable(role).await {
        Ok(ticket) => ticket,
        Err(_) => {
            return api_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiError::new(ErrorCode::Internal, "pairing ticket could not be persisted"),
            );
        }
    };
    Json(serde_json::json!({
        "secret": ticket.secret,
        "role": ticket.role,
        "certificate_fingerprint": ticket.certificate_fingerprint,
        "expires_at": ticket.expires_at.to_string(),
    }))
    .into_response()
}

async fn pair(
    State(state): State<ServerState>,
    Json(request): Json<PairingRequest>,
) -> impl IntoResponse {
    match state.pairing.pair(&request).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error) => api_error_response(status_for_error(error.code), error),
    }
}

async fn snapshot(State(state): State<ServerState>, headers: HeaderMap) -> impl IntoResponse {
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
            Some(snapshot) => (StatusCode::OK, Json(snapshot)).into_response(),
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
        Ok(snapshot) => (StatusCode::OK, Json(snapshot)).into_response(),
        Err(_) => api_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::new(ErrorCode::Internal, "snapshot unavailable"),
        ),
    }
}

async fn commands(
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

async fn artifact(
    State(state): State<ServerState>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let Ok(artifact_id) = ArtifactId::parse(&id) else {
        return api_error_response(
            StatusCode::NOT_FOUND,
            ApiError::new(ErrorCode::NotFound, "artifact not found"),
        );
    };

    // Artifact bytes are project/task scoped data.  Validate the metadata
    // projection before reading the payload so a guessed UUID cannot become a
    // cross-mission oracle (and so an agent capability can never download a
    // sibling task's result).  Device roles retain read access to projects;
    // operator/owner authorization is enforced by the pairing token itself.
    let snapshot = match state.store.snapshot().await {
        Ok(snapshot) => snapshot,
        Err(_) => {
            return api_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiError::new(ErrorCode::Internal, "artifact metadata unavailable"),
            );
        }
    };
    let Some(metadata) = snapshot
        .artifacts
        .iter()
        .find(|artifact| artifact.id == artifact_id)
        .cloned()
    else {
        return api_error_response(
            StatusCode::NOT_FOUND,
            ApiError::new(ErrorCode::NotFound, "artifact not found"),
        );
    };
    if !snapshot
        .missions
        .iter()
        .any(|mission| mission.id == metadata.mission_id)
        || metadata.task_id.is_some_and(|task_id| {
            !snapshot
                .tasks
                .iter()
                .any(|task| task.id == task_id && task.mission_id == metadata.mission_id)
        })
    {
        return api_error_response(
            StatusCode::NOT_FOUND,
            ApiError::new(ErrorCode::NotFound, "artifact not found"),
        );
    }
    if let Some(token) = headers
        .get("x-frank-agent-token")
        .and_then(|value| value.to_str().ok())
    {
        let Some((agent_id, task_id)) = state.orchestrator.agent_capability_actor(token).await
        else {
            return api_error_response(
                StatusCode::UNAUTHORIZED,
                ApiError::new(
                    ErrorCode::Unauthorized,
                    "agent session capability is invalid",
                ),
            );
        };
        if metadata.task_id != Some(task_id)
            || !agent_task_matches(&state, agent_id, task_id, metadata.mission_id).await
        {
            return api_error_response(
                StatusCode::FORBIDDEN,
                ApiError::new(ErrorCode::Forbidden, "artifact is outside task scope"),
            );
        }
    } else if authenticate(&state, &headers).await.is_none() {
        return api_error_response(
            StatusCode::UNAUTHORIZED,
            ApiError::new(ErrorCode::Unauthorized, "device authentication required"),
        );
    }
    if metadata.size > MAX_ARTIFACT_BYTES {
        return api_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::new(
                ErrorCode::Internal,
                "artifact metadata exceeds the server limit",
            ),
        );
    }
    // SQLite returns one bounded slice per iteration. The stream stops only
    // after the metadata-declared size has been sent; an early/oversized slice
    // is surfaced as an IO error instead of silently returning a truncated or
    // cross-projection response.
    const CHUNK_BYTES: u64 = (MAX_TERMINAL_FRAME_BYTES as u64) * 4;
    let store = state.store.clone();
    let artifact_size = metadata.size;
    let artifact_mime = metadata.mime_type.clone();
    let stream = futures_util::stream::try_unfold(
        (store, artifact_id, 0_u64),
        move |(store, artifact_id, offset)| async move {
            if offset >= artifact_size {
                return Ok(None);
            }
            let requested = (artifact_size - offset).min(CHUNK_BYTES);
            let bytes = store
                .artifact_chunk(artifact_id, offset, requested)
                .await
                .map_err(|error| std::io::Error::other(error.to_string()))?
                .ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::NotFound, "artifact disappeared")
                })?;
            if bytes.is_empty() || bytes.len() as u64 > requested {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "artifact projection is shorter than its metadata",
                ));
            }
            let next_offset = offset.saturating_add(bytes.len() as u64);
            if next_offset > artifact_size {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "artifact projection exceeds its metadata",
                ));
            }
            Ok(Some((
                Bytes::from(bytes),
                (store, artifact_id, next_offset),
            )))
        },
    );
    let mut response = Body::from_stream(stream).into_response();
    *response.status_mut() = StatusCode::OK;
    if let Ok(value) = axum::http::HeaderValue::from_str(&artifact_mime) {
        response
            .headers_mut()
            .insert(axum::http::header::CONTENT_TYPE, value);
    }
    if let Ok(value) = axum::http::HeaderValue::from_str(&artifact_size.to_string()) {
        response
            .headers_mut()
            .insert(axum::http::header::CONTENT_LENGTH, value);
    }
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    response
}

#[derive(Debug, Serialize)]
struct ArtifactUploadChunkResponse {
    upload_id: UploadId,
    received: u64,
}

/// Receive one contiguous, authenticated artifact chunk.  The upload row is
/// created by `BeginArtifactUpload`; this endpoint never trusts a client
/// supplied size or path and only appends at the durable offset recorded by
/// SQLite.
async fn artifact_upload_chunk(
    State(state): State<ServerState>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let Ok(upload_id) = UploadId::parse(&id) else {
        return api_error_response(
            StatusCode::NOT_FOUND,
            ApiError::new(ErrorCode::NotFound, "artifact upload not found"),
        );
    };
    let offset = match headers
        .get("x-frank-offset")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
    {
        Some(offset) => offset,
        None => {
            return api_error_response(
                StatusCode::BAD_REQUEST,
                ApiError::new(ErrorCode::Validation, "x-frank-offset is required"),
            );
        }
    };
    if body.len() > MAX_TERMINAL_FRAME_BYTES * 4 {
        return api_error_response(
            StatusCode::PAYLOAD_TOO_LARGE,
            ApiError::new(ErrorCode::PayloadTooLarge, "artifact chunk is too large"),
        );
    }
    let Some(upload) = state.store.snapshot().await.ok().and_then(|snapshot| {
        snapshot
            .uploads
            .into_iter()
            .find(|upload| upload.id == upload_id)
    }) else {
        return api_error_response(
            StatusCode::NOT_FOUND,
            ApiError::new(ErrorCode::NotFound, "artifact upload not found"),
        );
    };
    if upload.completed {
        return api_error_response(
            StatusCode::CONFLICT,
            ApiError::new(ErrorCode::Conflict, "artifact upload is already complete"),
        );
    }
    if let Some(value) = headers
        .get("content-range")
        .and_then(|value| value.to_str().ok())
        && let Err(message) =
            validate_content_range(value, offset, body.len() as u64, upload.spec.size)
    {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            ApiError::new(ErrorCode::Validation, message),
        );
    }

    // A scoped provider capability may only append to an upload belonging to
    // its own task. Device uploads require a mutating device role.
    if let Some(token) = headers
        .get("x-frank-agent-token")
        .and_then(|value| value.to_str().ok())
    {
        let Some((agent_id, task_id)) = state.orchestrator.agent_capability_actor(token).await
        else {
            return api_error_response(
                StatusCode::UNAUTHORIZED,
                ApiError::new(
                    ErrorCode::Unauthorized,
                    "agent session capability is invalid",
                ),
            );
        };
        if upload.spec.task_id != Some(task_id)
            || !agent_task_matches(&state, agent_id, task_id, upload.spec.mission_id).await
        {
            return api_error_response(
                StatusCode::FORBIDDEN,
                ApiError::new(
                    ErrorCode::Forbidden,
                    "artifact upload is outside task scope",
                ),
            );
        }
    } else {
        let Some(auth) = authenticate(&state, &headers).await else {
            return api_error_response(
                StatusCode::UNAUTHORIZED,
                ApiError::new(ErrorCode::Unauthorized, "device authentication required"),
            );
        };
        if !auth.role.can_mutate() {
            return api_error_response(
                StatusCode::FORBIDDEN,
                ApiError::new(
                    ErrorCode::Forbidden,
                    "operator role required for artifact upload",
                ),
            );
        }
    }
    match state
        .store
        .append_artifact_upload_chunk(upload_id, offset, &body)
        .await
    {
        Ok(received) => (
            StatusCode::OK,
            Json(ArtifactUploadChunkResponse {
                upload_id,
                received,
            }),
        )
            .into_response(),
        Err(error) => {
            let validation = matches!(&error, frank_store::StoreError::Validation(_));
            api_error_response(
                if validation {
                    StatusCode::BAD_REQUEST
                } else {
                    StatusCode::INTERNAL_SERVER_ERROR
                },
                ApiError::new(
                    if validation {
                        ErrorCode::Validation
                    } else {
                        ErrorCode::Internal
                    },
                    error.to_string(),
                ),
            )
        }
    }
}

/// Validate an optional RFC 7233-style range sent with an upload chunk.  The
/// legacy `x-frank-offset` header remains supported for clients that do not
/// know the total size, while a supplied range is never allowed to describe a
/// different byte span than the request body.
fn validate_content_range(
    value: &str,
    offset: u64,
    body_len: u64,
    expected_size: u64,
) -> std::result::Result<(), String> {
    let value = value.trim();
    let Some(value) = value.strip_prefix("bytes ") else {
        return Err("content-range must use the bytes unit".into());
    };
    let Some((range, total)) = value.split_once('/') else {
        return Err("content-range is malformed".into());
    };
    let total = total
        .parse::<u64>()
        .map_err(|_| "content-range total is invalid".to_string())?;
    if total != expected_size {
        return Err("content-range total does not match the upload size".into());
    }
    let Some((start, end)) = range.split_once('-') else {
        return Err("content-range byte span is malformed".into());
    };
    let start = start
        .parse::<u64>()
        .map_err(|_| "content-range start is invalid".to_string())?;
    let end = end
        .parse::<u64>()
        .map_err(|_| "content-range end is invalid".to_string())?;
    if start != offset || end < start || end.saturating_sub(start).saturating_add(1) != body_len {
        return Err("content-range does not match the contiguous chunk".into());
    }
    if end >= expected_size && body_len != 0 {
        return Err("content-range exceeds the upload size".into());
    }
    Ok(())
}

async fn agent_task_matches(
    state: &ServerState,
    agent_id: AgentId,
    task_id: TaskId,
    mission_id: MissionId,
) -> bool {
    state.store.snapshot().await.ok().is_some_and(|snapshot| {
        snapshot.tasks.iter().any(|task| {
            task.id == task_id
                && task.mission_id == mission_id
                && task.assigned_agent == Some(agent_id)
        })
    })
}

#[derive(Debug, Deserialize)]
struct BrowseQuery {
    path: Option<String>,
}

#[derive(Debug, Serialize)]
struct BrowseEntry {
    name: String,
    directory: bool,
}

#[derive(Debug, Serialize)]
struct BrowseResponse {
    path: String,
    entries: Vec<BrowseEntry>,
}

async fn browse_projects(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Query(query): Query<BrowseQuery>,
) -> impl IntoResponse {
    let Some(auth) = authenticate(&state, &headers).await else {
        return api_error_response(
            StatusCode::UNAUTHORIZED,
            ApiError::new(ErrorCode::Unauthorized, "device authentication required"),
        );
    };
    if !auth.role.can_admin() {
        return api_error_response(
            StatusCode::FORBIDDEN,
            ApiError::new(
                ErrorCode::Forbidden,
                "owner role required to browse project roots",
            ),
        );
    }
    let roots = match state.store.snapshot().await {
        Ok(snapshot) => snapshot.server.allowed_project_roots,
        Err(_) => Vec::new(),
    };
    let path = query
        .path
        .unwrap_or_else(|| roots.first().cloned().unwrap_or_else(|| ".".into()));
    if path.len() > 4_096 || path.chars().any(|character| character.is_control()) {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            ApiError::new(ErrorCode::Validation, "path is invalid"),
        );
    }
    if path.split(['/', '\\']).any(|component| component == "..") {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            ApiError::new(ErrorCode::Validation, "path traversal is not allowed"),
        );
    }
    let canonical_roots = roots
        .iter()
        .filter_map(|root| std::fs::canonicalize(root).ok())
        .collect::<Vec<_>>();
    if !roots.is_empty() && canonical_roots.len() != roots.len() {
        return api_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::new(
                ErrorCode::Internal,
                "an allowed project root is unavailable",
            ),
        );
    }
    let path = match canonicalize_allow_missing(std::path::Path::new(&path)) {
        Ok(path) => path,
        Err(_) => {
            return api_error_response(
                StatusCode::NOT_FOUND,
                ApiError::new(ErrorCode::NotFound, "directory not found"),
            );
        }
    };
    if !canonical_roots.is_empty() && !canonical_roots.iter().any(|root| path.starts_with(root)) {
        return api_error_response(
            StatusCode::FORBIDDEN,
            ApiError::new(
                ErrorCode::Forbidden,
                "path is outside an allowed project root",
            ),
        );
    }
    if !path.is_dir() {
        return api_error_response(
            StatusCode::NOT_FOUND,
            ApiError::new(ErrorCode::NotFound, "directory not found"),
        );
    }
    let mut entries = Vec::new();
    let read_dir = match std::fs::read_dir(&path) {
        Ok(read_dir) => read_dir,
        Err(_) => {
            return api_error_response(
                StatusCode::NOT_FOUND,
                ApiError::new(ErrorCode::NotFound, "directory not found"),
            );
        }
    };
    for entry in read_dir.flatten().take(256) {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        // Do not follow symlinks while presenting the server-side browser.
        // A symlink target can still be opened explicitly after it has passed
        // the canonical allowed-root check, but it is never presented as a
        // traversable directory by default.
        let directory = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        entries.push(BrowseEntry { name, directory });
    }
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    (
        StatusCode::OK,
        Json(BrowseResponse {
            path: path.to_string_lossy().into_owned(),
            entries,
        }),
    )
        .into_response()
}

#[derive(Debug, Serialize)]
struct DeviceSummary {
    device_id: DeviceId,
    name: String,
    role: DeviceRole,
    revoked: bool,
    last_seen_at: u64,
}

async fn devices(State(state): State<ServerState>, headers: HeaderMap) -> impl IntoResponse {
    let Some(auth) = authenticate(&state, &headers).await else {
        return api_error_response(
            StatusCode::UNAUTHORIZED,
            ApiError::new(ErrorCode::Unauthorized, "device authentication required"),
        );
    };
    if !auth.role.can_admin() {
        return api_error_response(
            StatusCode::FORBIDDEN,
            ApiError::new(ErrorCode::Forbidden, "owner role required to list devices"),
        );
    }
    let summaries = state
        .pairing
        .devices()
        .await
        .into_iter()
        .map(|(auth, revoked, last_seen_at)| DeviceSummary {
            device_id: auth.device_id,
            name: auth.name,
            role: auth.role,
            revoked,
            last_seen_at,
        })
        .collect::<Vec<_>>();
    (StatusCode::OK, Json(summaries)).into_response()
}

async fn revoke_device(
    State(state): State<ServerState>,
    AxumPath(id): AxumPath<String>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let Some(auth) = authenticate(&state, &headers).await else {
        return api_error_response(
            StatusCode::UNAUTHORIZED,
            ApiError::new(ErrorCode::Unauthorized, "device authentication required"),
        );
    };
    if !auth.role.can_admin() {
        return api_error_response(
            StatusCode::FORBIDDEN,
            ApiError::new(
                ErrorCode::Forbidden,
                "owner role required to revoke devices",
            ),
        );
    }
    let Ok(device_id) = DeviceId::parse(&id) else {
        return api_error_response(
            StatusCode::NOT_FOUND,
            ApiError::new(ErrorCode::NotFound, "device not found"),
        );
    };
    if state.pairing.revoke(device_id).await {
        (StatusCode::OK, Json(serde_json::json!({"revoked": true}))).into_response()
    } else {
        api_error_response(
            StatusCode::NOT_FOUND,
            ApiError::new(ErrorCode::NotFound, "device not found"),
        )
    }
}

#[derive(Debug, Deserialize)]
struct EventQuery {
    after: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct TerminalQuery {
    after: Option<u64>,
}

async fn events(
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

async fn event_socket(mut socket: WebSocket, state: ServerState, mut cursor: u64) {
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

async fn terminals(
    State(state): State<ServerState>,
    AxumPath(session_id): AxumPath<String>,
    headers: HeaderMap,
    Query(query): Query<TerminalQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let token = bearer(&headers);
    let Some(token) = token else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Some(auth) = state.pairing.authenticate(token).await else {
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
            actor_device_id,
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
    actor_device_id: DeviceId,
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
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(15));
    loop {
        tokio::select! {
            _ = tick.tick() => {
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
                        let lease_active = terminal_lease_active(&state, session_id, actor_device_id).await;
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
async fn ensure_terminal_pump(
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

async fn close_terminal_after_pump_exit(state: &ServerState, session_id: TerminalSessionId) {
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

async fn terminal_lease(
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

async fn terminal_lease_active(
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

async fn authenticate(state: &ServerState, headers: &HeaderMap) -> Option<DeviceAuth> {
    let token = bearer(headers)?;
    state.pairing.authenticate(token).await
}

async fn agent_capability_from_headers(
    state: &ServerState,
    headers: &HeaderMap,
) -> Option<(AgentId, TaskId)> {
    let token = headers
        .get("x-frank-agent-token")
        .and_then(|value| value.to_str().ok())?;
    state.orchestrator.agent_capability_actor(token).await
}

/// Return the minimum projection a provider session needs for its MCP bridge.
/// Device clients continue to receive the complete snapshot; an agent token
/// never grants a read-all view of other missions, projects, or workers.
async fn scoped_agent_snapshot(
    state: &ServerState,
    agent_id: AgentId,
    task_id: TaskId,
) -> Option<Snapshot> {
    let mut snapshot = state.store.snapshot().await.ok()?;
    let task = snapshot
        .tasks
        .iter()
        .find(|task| task.id == task_id)?
        .clone();
    if task.assigned_agent != Some(agent_id) {
        return None;
    }
    let project_id = snapshot
        .missions
        .iter()
        .find(|mission| mission.id == task.mission_id)
        .map(|mission| mission.project_id)?;
    snapshot
        .agents
        .retain(|agent| agent.id == agent_id || agent.display_name == "Frank supervisor");
    snapshot
        .missions
        .retain(|mission| mission.id == task.mission_id);
    snapshot.projects.retain(|project| project.id == project_id);
    snapshot.tasks.retain(|candidate| candidate.id == task_id);
    snapshot
        .messages
        .retain(|message| message.task_id == Some(task_id));
    snapshot
        .approvals
        .retain(|approval| approval.task_id == task_id);
    snapshot
        .artifacts
        .retain(|artifact| artifact.task_id == Some(task_id));
    snapshot
        .usage
        .retain(|usage| usage.scope == BudgetScope::Task && usage.scope_id == task_id.to_string());
    snapshot
        .terminals
        .retain(|terminal| terminal.task_id == task_id);
    // Do not expose server filesystem policy or worktree roots through the
    // agent-facing projection. The task worktree is already carried by the
    // scoped TaskView and checked again by the daemon.
    snapshot.server.allowed_project_roots.clear();
    snapshot.server.worktree_root.clear();
    Some(snapshot)
}

/// Enforce the second half of agent-session authorization at the transport
/// boundary.  The orchestrator still performs the authoritative state and
/// transition checks, but a short-lived capability must never become a
/// general-purpose operator token merely because the caller also possesses a
/// valid device bearer.  Every provider mutation is therefore tied to the
/// task encoded in the capability and, where applicable, to the agent that is
/// assigned to that task.
async fn agent_command_is_scoped(
    state: &ServerState,
    command: &Command,
    agent_id: AgentId,
    task_id: TaskId,
) -> bool {
    let Ok(snapshot) = state.store.snapshot().await else {
        return false;
    };
    let Some(scope_task) = snapshot.tasks.iter().find(|task| task.id == task_id) else {
        return false;
    };
    if scope_task.assigned_agent != Some(agent_id) {
        return false;
    }
    let mission_id = scope_task.mission_id;
    let agent_text = agent_id.to_string();
    let task_is_owned = |candidate: TaskId| {
        snapshot
            .tasks
            .iter()
            .find(|task| task.id == candidate)
            .is_some_and(|task| {
                task.mission_id == mission_id && task.assigned_agent == Some(agent_id)
            })
    };

    match command {
        Command::UpdateTask {
            task_id: candidate,
            patch,
        } => {
            if *candidate != task_id {
                return false;
            }
            // A worker can report task metadata and status, but it cannot
            // raise or remove its own hard budget. Budget changes remain an
            // operator/supervisor action even when the provider has a valid
            // scoped session capability.
            if patch.budget.is_some() {
                return false;
            }
            // A worker may update its own task metadata, but cannot detach the
            // task or hand it to another agent through the generic patch.
            match patch.assigned_agent {
                None => true,
                Some(Some(id)) => id == agent_id,
                Some(None) => false,
            }
        }
        Command::SetTaskStatus {
            task_id: candidate,
            status,
        } => *candidate == task_id && matches!(status, TaskStatus::Review | TaskStatus::Blocked),
        Command::CreateTask(spec) => {
            spec.mission_id == mission_id
                && spec
                    .assigned_agent
                    .is_none_or(|assigned| assigned == agent_id)
                && spec
                    .dependencies
                    .iter()
                    .all(|dependency| *dependency == task_id || task_is_owned(*dependency))
        }
        Command::SendMessage(spec) => {
            spec.mission_id == mission_id && spec.task_id == Some(task_id)
        }
        Command::AckMessage { message_id } => snapshot.messages.iter().any(|message| {
            message.id == *message_id
                && message.mission_id == mission_id
                && message.task_id == Some(task_id)
                && message.recipient.kind == ActorKind::Agent
                && message.recipient.id.as_deref() == Some(agent_text.as_str())
        }),
        Command::CompleteMessage { message_id, .. } => snapshot.messages.iter().any(|message| {
            message.id == *message_id
                && message.mission_id == mission_id
                && message.task_id == Some(task_id)
                && message.recipient.kind == ActorKind::Agent
                && message.recipient.id.as_deref() == Some(agent_text.as_str())
        }),
        Command::RequestApproval(spec) => spec.agent_id == agent_id && spec.task_id == task_id,
        Command::PublishArtifact(spec) => {
            spec.mission_id == mission_id && spec.task_id == Some(task_id)
        }
        Command::BeginArtifactUpload(spec) => {
            spec.mission_id == mission_id && spec.task_id == Some(task_id)
        }
        Command::FinalizeArtifactUpload { upload_id, .. } => snapshot
            .uploads
            .iter()
            .any(|upload| upload.id == *upload_id && upload.spec.task_id == Some(task_id)),
        Command::ProposeMemory {
            agent_id: candidate,
            ..
        }
        | Command::ReadMemory {
            agent_id: candidate,
            ..
        } => *candidate == agent_id,
        _ => false,
    }
}

fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get("authorization")?.to_str().ok()?;
    value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
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

#[derive(Debug, Clone)]
pub struct PairingSecret;

impl PairingSecret {
    pub fn generate() -> String {
        let mut bytes = [0_u8; 32];
        if getrandom::fill(&mut bytes).is_ok() {
            return hex::encode(bytes);
        }
        // System randomness should be available on every supported platform;
        // retain a non-panicking fallback for a degraded early-boot entropy
        // source while still returning the fixed 256-bit wire shape.
        let first = uuid::Uuid::new_v4();
        let second = uuid::Uuid::new_v4();
        let mut fallback = [0_u8; 32];
        fallback[..16].copy_from_slice(first.as_bytes());
        fallback[16..].copy_from_slice(second.as_bytes());
        hex::encode(fallback)
    }
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
