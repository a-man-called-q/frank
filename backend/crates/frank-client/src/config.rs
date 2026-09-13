use frank_protocol::*;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ClientConfig {
    pub base_url: String,
    pub device_token: Option<String>,
    pub pinned_certificate_fingerprint: Option<String>,
    pub ca_certificate_pem: Option<Vec<u8>>,
    /// Short-lived provider-session capability. This is separate from the
    /// device bearer token: an MCP bridge may carry both, while frankd uses
    /// this header to scope mutations to one agent/task.
    pub agent_session_token: Option<String>,
    pub allow_insecure_local: bool,
    pub request_timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BrowseEntry {
    pub name: String,
    pub directory: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ArtifactUploadChunkResponse {
    pub upload_id: UploadId,
    pub received: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BrowseResponse {
    pub path: String,
    pub entries: Vec<BrowseEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeviceSummary {
    pub device_id: DeviceId,
    pub name: String,
    pub role: DeviceRole,
    pub revoked: bool,
    pub last_seen_at: u64,
}

impl ClientConfig {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            device_token: None,
            pinned_certificate_fingerprint: None,
            ca_certificate_pem: None,
            agent_session_token: None,
            allow_insecure_local: false,
            request_timeout: Duration::from_secs(30),
        }
    }

    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.device_token = Some(token.into());
        self
    }

    pub fn with_pin(mut self, fingerprint: impl Into<String>) -> Self {
        self.pinned_certificate_fingerprint = Some(fingerprint.into());
        self
    }

    pub fn with_ca_certificate_pem(mut self, pem: impl Into<Vec<u8>>) -> Self {
        self.ca_certificate_pem = Some(pem.into());
        self
    }

    pub fn with_agent_session_token(mut self, token: impl Into<String>) -> Self {
        self.agent_session_token = Some(token.into());
        self
    }

    /// Only explicit test/development callers may opt into loopback HTTP.
    /// Production GUI profiles keep this false, including when server and
    /// GUI run on the same machine.
    pub fn allow_insecure_local(mut self) -> Self {
        self.allow_insecure_local = true;
        self
    }
}
