use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceRole {
    Owner,
    Operator,
    Observer,
}

impl DeviceRole {
    pub fn can_mutate(self) -> bool {
        !matches!(self, Self::Observer)
    }

    pub fn can_admin(self) -> bool {
        matches!(self, Self::Owner)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActorKind {
    Device,
    Agent,
    Supervisor,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActorRef {
    pub kind: ActorKind,
    pub id: Option<String>,
    pub display_name: Option<String>,
}

impl ActorRef {
    pub fn system() -> Self {
        Self {
            kind: ActorKind::System,
            id: None,
            display_name: Some("frankd".to_string()),
        }
    }

    pub fn supervisor() -> Self {
        Self {
            kind: ActorKind::Supervisor,
            id: None,
            display_name: Some("Frank supervisor".to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionRange {
    pub min: u16,
    pub max: u16,
}

impl VersionRange {
    pub const fn current() -> Self {
        Self {
            min: MIN_COMPATIBLE_CLIENT,
            max: PROTOCOL_VERSION,
        }
    }

    pub const fn accepts(&self, version: u16) -> bool {
        version >= self.min && version <= self.max
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub protocol_version: u16,
    pub supported_versions: VersionRange,
    pub minimum_compatible_client: u16,
    pub server_id: ServerId,
    #[serde(default)]
    pub certificate_fingerprint: String,
    pub server_version: String,
    pub features: Vec<String>,
    pub openrouter: OpenRouterCapability,
    pub limits: CapabilityLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityLimits {
    pub max_message_bytes: usize,
    pub max_command_bytes: usize,
    pub max_terminal_frame_bytes: usize,
    pub max_artifact_bytes: u64,
    pub max_concurrency: u16,
}

impl Default for CapabilityLimits {
    fn default() -> Self {
        Self {
            max_message_bytes: MAX_MESSAGE_BODY_BYTES,
            max_command_bytes: MAX_COMMAND_BODY_BYTES,
            max_terminal_frame_bytes: MAX_TERMINAL_FRAME_BYTES,
            max_artifact_bytes: MAX_ARTIFACT_BYTES,
            max_concurrency: 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OpenRouterCapability {
    #[serde(default)]
    pub configured: bool,
    pub version: Option<String>,
    pub logged_in: bool,
    pub available: bool,
    pub capabilities: Vec<String>,
    pub diagnostic: Option<String>,
    /// Redacted credential origin for API-backed providers.  This is never a
    /// secret value and is safe to expose in the owner-facing settings UI.
    #[serde(default)]
    pub credential_source: Option<String>,
    /// Timestamp of the last successful model-catalog refresh.
    #[serde(default)]
    pub catalog_refreshed_at: Option<Timestamp>,
}

/// Normalized status values used by the owner-facing health/doctor screens.
/// They deliberately avoid carrying raw command output or filesystem paths;
/// diagnostics returned over the wire are sanitized by the daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceStatusView {
    pub service_name: String,
    pub installed: bool,
    pub running: bool,
    pub pid: Option<u32>,
    pub descriptor_path: Option<String>,
    pub health: HealthStatus,
    pub detail: Option<String>,
    pub checked_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeDoctorCheck {
    pub component: String,
    pub status: HealthStatus,
    pub version: Option<String>,
    pub detail: Option<String>,
    pub remediation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeDoctorView {
    pub healthy: bool,
    pub checks: Vec<RuntimeDoctorCheck>,
    pub checked_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkPreview {
    pub current_bind: String,
    pub proposed_bind: String,
    pub tls_required: bool,
    pub tls_fingerprint: Option<String>,
    pub restart_required: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionView {
    pub event_days: u16,
    pub terminal_days: u16,
    pub artifact_days: u16,
    pub terminal_max_bytes: u64,
    pub pending_uploads: u64,
    pub pending_operations: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticSnapshot {
    pub generated_at: Timestamp,
    pub database: HealthStatus,
    pub audit_exporter: HealthStatus,
    pub git: HealthStatus,
    pub openrouter: Vec<RuntimeDoctorCheck>,
    pub service: Option<ServiceStatusView>,
    pub retention: RetentionView,
    pub operation_backlog: u64,
    pub disk_free_bytes: Option<u64>,
    pub redactions: Vec<String>,
}

/// Opaque provider identifier stored on usage/audit records.
///
/// Runtime configuration is OpenRouter-only, but old ledger rows may still
/// carry `codex` or `claude`. Keeping this as a string prevents historical
/// telemetry from being rewritten or accidentally becoming a runtime choice.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UsageProviderId(pub String);

impl UsageProviderId {
    pub fn openrouter() -> Self {
        Self("openrouter".to_string())
    }
}

impl Default for UsageProviderId {
    fn default() -> Self {
        Self::openrouter()
    }
}

impl fmt::Display for UsageProviderId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelDescriptor {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub canonical_slug: Option<String>,
    #[serde(default)]
    pub context_length: Option<u64>,
    #[serde(default)]
    pub input_price_per_token: Option<String>,
    #[serde(default)]
    pub output_price_per_token: Option<String>,
    #[serde(default)]
    pub supported_parameters: Vec<String>,
    #[serde(default)]
    pub deprecated_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModelSource {
    #[default]
    Role,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenRouterConnectionView {
    pub configured: bool,
    #[serde(default)]
    pub credential_source: Option<String>,
    #[serde(default)]
    pub checked_at: Option<Timestamp>,
    #[serde(default)]
    pub catalog_refreshed_at: Option<Timestamp>,
    #[serde(default)]
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandshakeRequest {
    pub protocol_version: u16,
    pub client_version: String,
    pub client_kind: String,
    pub supported_versions: VersionRange,
}

/// The first request a remote GUI/CLI makes after opening HTTPS. Keeping a
/// request/response handshake separate from the capability document lets the
/// daemon reject an incompatible client before it accepts mutations, while
/// newer additive capabilities remain safe to ignore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandshakeResponse {
    pub negotiated_version: u16,
    pub server_id: ServerId,
    pub server_version: String,
    pub capabilities: Capabilities,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingRequest {
    pub protocol_version: u16,
    pub secret: String,
    pub certificate_fingerprint: String,
    pub requested_role: DeviceRole,
    pub device_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingResponse {
    pub server_id: ServerId,
    pub device_id: DeviceId,
    pub role: DeviceRole,
    pub device_token: String,
    pub certificate_fingerprint: String,
    /// PEM is returned only during pairing so a client can pin the same
    /// self-signed identity for subsequent HTTPS and WebSocket reconnects.
    /// It is optional for servers using a public/OS-trusted certificate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub certificate_pem: Option<String>,
    pub expires_at: Timestamp,
}

/// The one local account created during first-run bootstrap.  The password
/// never crosses this boundary; only the stable identifier and public login
/// name are sent to clients.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerView {
    pub id: UserId,
    pub username: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthStatusResponse {
    pub configured: bool,
    pub auth_method: String,
    pub server_id: ServerId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthLoginRequest {
    pub username: String,
    pub password: String,
    /// Optional display name for the session's device actor.  Desktop clients
    /// may omit this and the server uses a stable default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthSessionView {
    pub session_id: SessionId,
    pub device_id: DeviceId,
    pub expires_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthLoginResponse {
    #[serde(alias = "token")]
    pub access_token: String,
    pub expires_at: Timestamp,
    pub server_id: ServerId,
    pub device_id: DeviceId,
    pub owner: OwnerView,
    pub session: AuthSessionView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthMeResponse {
    pub owner: OwnerView,
    pub session: AuthSessionView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthPasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthMutationResponse {
    pub success: bool,
}
