//! Reconnecting native client for Frank's versioned HTTPS/WebSocket API.
//!
//! This crate contains no GUI dependencies and no `frank-app` dependency.  A
//! local GUI uses the exact same HTTPS path as a laptop connecting over LAN or
//! VPN; there is no direct in-process service shortcut.

// ClientError keeps the underlying reqwest/WebSocket sources intact so callers
// get useful diagnostics. Boxing every source would make the public error
// conversion noisy without changing the transport contract.
#![allow(clippy::result_large_err)]

use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use frank_protocol::*;
use futures_util::{SinkExt, StreamExt};
use reqwest::{Client, RequestBuilder, StatusCode, Url};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use serde::Deserialize;
use sha2::Digest;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};
use tokio_tungstenite::{
    Connector, connect_async_tls_with_config,
    tungstenite::{
        Message,
        client::IntoClientRequest,
        http::{HeaderValue, Request},
    },
};

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

/// Credential persistence boundary. Native shells can implement this with a
/// platform keychain; the bundled fallback is deliberately explicit and
/// symlink-safe instead of silently writing a world-readable token file.
pub trait CredentialStore: Send + Sync {
    fn save(&self, reference: &str, token: &str) -> Result<()>;
    fn load(&self, reference: &str) -> Result<Option<String>>;
    fn delete(&self, reference: &str) -> Result<()>;
    fn warning(&self) -> Option<&'static str>;
}

#[derive(Debug, Clone)]
pub struct FileCredentialStore {
    pub root: PathBuf,
}

impl FileCredentialStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path(&self, reference: &str) -> PathBuf {
        self.root
            .join(format!("{}.token", sanitize_reference(reference)))
    }
}

impl CredentialStore for FileCredentialStore {
    fn save(&self, reference: &str, token: &str) -> Result<()> {
        frank_safeio::ensure_dir(&self.root)
            .map_err(|error| ClientError::InvalidAddress(error.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.root, std::fs::Permissions::from_mode(0o700))
                .map_err(|error| ClientError::InvalidAddress(error.to_string()))?;
        }
        let path = self.path(reference);
        if std::fs::symlink_metadata(&path).is_ok() {
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|error| ClientError::InvalidAddress(error.to_string()))?;
            if metadata.file_type().is_symlink() {
                return Err(ClientError::InvalidAddress(
                    "credential path is a symlink".into(),
                ));
            }
        }
        frank_safeio::write_text_atomic(&path, token, 64 * 1024)
            .map_err(|error| ClientError::InvalidAddress(error.to_string()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                .map_err(|error| ClientError::InvalidAddress(error.to_string()))?;
        }
        Ok(())
    }

    fn load(&self, reference: &str) -> Result<Option<String>> {
        let path = self.path(reference);
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            return Ok(None);
        };
        if metadata.file_type().is_symlink() {
            return Err(ClientError::InvalidAddress(
                "credential path is a symlink".into(),
            ));
        }
        frank_safeio::read_text_capped(&path, 64 * 1024)
            .map(Some)
            .map_err(|error| ClientError::InvalidAddress(error.to_string()))
    }

    fn delete(&self, reference: &str) -> Result<()> {
        let path = self.path(reference);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => Err(ClientError::InvalidAddress(
                "credential path is a symlink".into(),
            )),
            Ok(_) => std::fs::remove_file(path)
                .map_err(|error| ClientError::InvalidAddress(error.to_string())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(ClientError::InvalidAddress(error.to_string())),
        }
    }

    fn warning(&self) -> Option<&'static str> {
        Some(
            "OS Secret Service/keychain unavailable; using a symlink-safe mode-0600 token fallback",
        )
    }
}

/// Native credential-store adapter used by the desktop shell.  The process
/// never receives a provider/API secret: only the paired Frank device token
/// crosses this boundary. If the platform helper is unavailable, operations
/// fall back to [`FileCredentialStore`], whose warning is surfaced by the GUI
/// so users can explicitly acknowledge the mode-0600 alternative.
#[derive(Debug, Clone)]
pub struct NativeCredentialStore {
    pub service: String,
    pub fallback: FileCredentialStore,
}

impl NativeCredentialStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            service: "dev.frank.desktop".into(),
            fallback: FileCredentialStore::new(root),
        }
    }

    fn key(&self, reference: &str) -> String {
        format!("{}:{}", self.service, sanitize_reference(reference))
    }

    #[allow(clippy::needless_return)]
    fn native_save(&self, reference: &str, token: &str) -> Option<Result<()>> {
        #[cfg(target_os = "macos")]
        {
            let output = std::process::Command::new("security")
                .args([
                    "add-generic-password",
                    "-a",
                    "frank",
                    "-s",
                    &self.key(reference),
                    "-w",
                    token,
                    "-U",
                ])
                .output();
            return Some(match output {
                Ok(output) if output.status.success() => Ok(()),
                Ok(_) | Err(_) => Err(ClientError::Tls("macOS Keychain unavailable".into())),
            });
        }
        #[cfg(target_os = "linux")]
        {
            let mut command = std::process::Command::new("secret-tool");
            command.args([
                "store",
                "--label=Frank device token",
                "service",
                &self.service,
                "account",
                &sanitize_reference(reference),
            ]);
            command.stdin(std::process::Stdio::piped());
            return Some(match command.spawn() {
                Ok(mut child) => {
                    let result = child
                        .stdin
                        .take()
                        .map(|mut stdin| std::io::Write::write_all(&mut stdin, token.as_bytes()));
                    let status = child.wait();
                    if result.is_ok() && status.is_ok_and(|status| status.success()) {
                        Ok(())
                    } else {
                        Err(ClientError::Tls("Linux Secret Service unavailable".into()))
                    }
                }
                Err(_) => Err(ClientError::Tls("Linux Secret Service unavailable".into())),
            });
        }
        #[cfg(target_os = "windows")]
        {
            // Credential Manager does not provide a stable, dependency-free
            // shell fallback; use the native Win32 Credential Manager API.
            Some(windows_credentials::save(&self.key(reference), token))
        }
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            let _ = (reference, token);
            None
        }
    }

    #[allow(unreachable_code)]
    fn native_load(&self, reference: &str) -> Option<Result<Option<String>>> {
        #[cfg(target_os = "macos")]
        {
            let output = std::process::Command::new("security")
                .args([
                    "find-generic-password",
                    "-a",
                    "frank",
                    "-s",
                    &self.key(reference),
                    "-w",
                ])
                .output();
            return Some(match output {
                Ok(output) if output.status.success() => Ok(Some(
                    String::from_utf8_lossy(&output.stdout).trim().to_string(),
                )),
                Ok(output) if output.status.code() == Some(44) => Ok(None),
                Ok(_) | Err(_) => Err(ClientError::Tls("macOS Keychain unavailable".into())),
            });
        }
        #[cfg(target_os = "linux")]
        {
            let output = std::process::Command::new("secret-tool")
                .args([
                    "lookup",
                    "service",
                    &self.service,
                    "account",
                    &sanitize_reference(reference),
                ])
                .output();
            return Some(match output {
                Ok(output) if output.status.success() => Ok(Some(
                    String::from_utf8_lossy(&output.stdout).trim().to_string(),
                )),
                Ok(output) if output.status.code() == Some(1) => Ok(None),
                Ok(_) | Err(_) => Err(ClientError::Tls("Linux Secret Service unavailable".into())),
            });
        }
        #[cfg(target_os = "windows")]
        {
            return Some(windows_credentials::load(&self.key(reference)));
        }
        None
    }

    #[allow(unreachable_code)]
    fn native_delete(&self, reference: &str) -> Option<Result<()>> {
        #[cfg(target_os = "macos")]
        {
            let output = std::process::Command::new("security")
                .args([
                    "delete-generic-password",
                    "-a",
                    "frank",
                    "-s",
                    &self.key(reference),
                ])
                .output();
            return Some(match output {
                Ok(output) if output.status.success() || output.status.code() == Some(44) => Ok(()),
                Ok(_) | Err(_) => Err(ClientError::Tls("macOS Keychain unavailable".into())),
            });
        }
        #[cfg(target_os = "linux")]
        {
            let output = std::process::Command::new("secret-tool")
                .args([
                    "clear",
                    "service",
                    &self.service,
                    "account",
                    &sanitize_reference(reference),
                ])
                .output();
            return Some(match output {
                Ok(output) if output.status.success() || output.status.code() == Some(1) => Ok(()),
                Ok(_) | Err(_) => Err(ClientError::Tls("Linux Secret Service unavailable".into())),
            });
        }
        #[cfg(target_os = "windows")]
        {
            return Some(windows_credentials::delete(&self.key(reference)));
        }
        None
    }
}

impl CredentialStore for NativeCredentialStore {
    fn save(&self, reference: &str, token: &str) -> Result<()> {
        if let Some(result) = self.native_save(reference, token)
            && result.is_ok()
        {
            return result;
        }
        self.fallback.save(reference, token)
    }

    fn load(&self, reference: &str) -> Result<Option<String>> {
        if let Some(Ok(Some(value))) = self.native_load(reference) {
            return Ok(Some(value));
        }
        self.fallback.load(reference)
    }

    fn delete(&self, reference: &str) -> Result<()> {
        if let Some(result) = self.native_delete(reference)
            && result.is_ok()
        {
            self.fallback.delete(reference)?;
            return Ok(());
        }
        self.fallback.delete(reference)
    }

    fn warning(&self) -> Option<&'static str> {
        self.fallback.warning()
    }
}

#[cfg(target_os = "windows")]
#[allow(non_snake_case)]
mod windows_credentials {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    use std::slice;

    use super::{ClientError, Result};

    const CRED_TYPE_GENERIC: u32 = 1;
    const CRED_PERSIST_LOCAL: u32 = 2;
    const ERROR_NOT_FOUND: u32 = 1168;

    #[repr(C)]
    struct FileTime {
        dwLowDateTime: u32,
        dwHighDateTime: u32,
    }

    #[repr(C)]
    struct CredentialAttributeW {
        Keyword: *mut u16,
        Flags: u32,
        ValueSize: u32,
        Value: *mut u8,
    }

    #[repr(C)]
    struct CredentialW {
        Flags: u32,
        Type: u32,
        TargetName: *mut u16,
        Comment: *mut u16,
        LastWritten: FileTime,
        CredentialBlobSize: u32,
        CredentialBlob: *mut u8,
        Persist: u32,
        AttributeCount: u32,
        Attributes: *mut CredentialAttributeW,
        TargetAlias: *mut u16,
        UserName: *mut u16,
    }

    #[link(name = "Advapi32")]
    unsafe extern "system" {
        fn CredWriteW(credential: *const CredentialW, flags: u32) -> i32;
        fn CredReadW(
            target_name: *const u16,
            credential_type: u32,
            flags: u32,
            credential: *mut *mut CredentialW,
        ) -> i32;
        fn CredDeleteW(target_name: *const u16, credential_type: u32, flags: u32) -> i32;
        fn CredFree(buffer: *mut c_void);
    }

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn GetLastError() -> u32;
    }

    fn wide(value: &str) -> Vec<u16> {
        std::ffi::OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    pub fn save(target: &str, token: &str) -> Result<()> {
        let target = wide(target);
        let mut blob = token.as_bytes().to_vec();
        let credential = CredentialW {
            Flags: 0,
            Type: CRED_TYPE_GENERIC,
            TargetName: target.as_ptr().cast_mut(),
            Comment: ptr::null_mut(),
            LastWritten: FileTime {
                dwLowDateTime: 0,
                dwHighDateTime: 0,
            },
            CredentialBlobSize: blob.len() as u32,
            CredentialBlob: blob.as_mut_ptr(),
            Persist: CRED_PERSIST_LOCAL,
            AttributeCount: 0,
            Attributes: ptr::null_mut(),
            TargetAlias: ptr::null_mut(),
            UserName: ptr::null_mut(),
        };
        let ok = unsafe { CredWriteW(&credential, 0) };
        if ok == 0 {
            return Err(ClientError::Tls(format!(
                "Windows Credential Manager write failed ({})",
                unsafe { GetLastError() }
            )));
        }
        Ok(())
    }

    pub fn load(target: &str) -> Result<Option<String>> {
        let target = wide(target);
        let mut credential = ptr::null_mut();
        let ok = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) };
        if ok == 0 {
            let error = unsafe { GetLastError() };
            return if error == ERROR_NOT_FOUND {
                Ok(None)
            } else {
                Err(ClientError::Tls(format!(
                    "Windows Credential Manager read failed ({error})"
                )))
            };
        }
        if credential.is_null() {
            return Err(ClientError::Tls(
                "Windows Credential Manager returned an empty credential".into(),
            ));
        }
        let result = unsafe {
            let value = slice::from_raw_parts(
                (*credential).CredentialBlob,
                (*credential).CredentialBlobSize as usize,
            );
            String::from_utf8(value.to_vec())
                .map_err(|_| ClientError::Tls("Windows credential is not UTF-8".into()))
        };
        unsafe { CredFree(credential.cast()) };
        result.map(Some)
    }

    pub fn delete(target: &str) -> Result<()> {
        let target = wide(target);
        let ok = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
        if ok != 0 || unsafe { GetLastError() } == ERROR_NOT_FOUND {
            Ok(())
        } else {
            Err(ClientError::Tls(
                "Windows Credential Manager delete failed".into(),
            ))
        }
    }
}

fn sanitize_reference(reference: &str) -> String {
    let mut value = reference
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || *character == '-' || *character == '_'
        })
        .collect::<String>();
    if value.is_empty() {
        value = "default".into();
    }
    value.chars().take(96).collect()
}

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

#[derive(Debug, Clone)]
pub struct RemoteClient {
    config: ClientConfig,
    http: Client,
    base: Url,
}

impl RemoteClient {
    pub fn new(config: ClientConfig) -> Result<Self> {
        install_crypto_provider();
        let base = Url::parse(&config.base_url)
            .map_err(|error| ClientError::InvalidAddress(error.to_string()))?;
        let secure = matches!(base.scheme(), "https");
        let loopback = base
            .host_str()
            .map(|host| matches!(host, "localhost" | "127.0.0.1" | "::1"))
            .unwrap_or(false);
        if !secure && !(loopback && config.allow_insecure_local) {
            return Err(ClientError::InsecureTransport);
        }
        let mut builder = Client::builder().timeout(config.request_timeout);
        if let Some(pin) = &config.pinned_certificate_fingerprint {
            let tls = pinned_tls_config(pin)?;
            builder = builder.use_preconfigured_tls(tls);
        } else if let Some(pem) = &config.ca_certificate_pem {
            let certificate = reqwest::Certificate::from_pem(pem)?;
            builder = builder.add_root_certificate(certificate);
        }
        let http = builder.build()?;
        Ok(Self { config, http, base })
    }

    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    pub async fn health(&self) -> Result<serde_json::Value> {
        let response = self.http.get(self.endpoint("/v1/health")).send().await?;
        self.decode_json(response).await
    }

    /// Fetch the owner-only sanitized diagnostic snapshot.  Unlike health,
    /// this endpoint deliberately requires the paired device bearer and is
    /// intended for the Settings/doctor surfaces rather than liveness probes.
    pub async fn diagnostics(&self) -> Result<DiagnosticSnapshot> {
        let response = self
            .authorized(self.http.get(self.endpoint("/v1/diagnostics")))
            .send()
            .await?;
        self.decode_json(response).await
    }

    pub async fn capabilities(&self) -> Result<Capabilities> {
        let response = self
            .authorized(self.http.get(self.endpoint("/v1/capabilities")))
            .send()
            .await?;
        let capabilities: Capabilities = self.decode_json(response).await?;
        if !capabilities.supported_versions.accepts(PROTOCOL_VERSION)
            || capabilities.minimum_compatible_client > PROTOCOL_VERSION
        {
            return Err(ClientError::Api(ApiError::new(
                ErrorCode::VersionMismatch,
                "this client is not compatible with the server protocol",
            )));
        }
        self.verify_pin(&capabilities, Some(&capabilities.certificate_fingerprint))?;
        Ok(capabilities)
    }

    /// Negotiate the wire version before subscribing to events. Capabilities
    /// remain available as a standalone bootstrap endpoint, but the explicit
    /// handshake gives onboarding one atomic compatibility decision and server
    /// identity document for reconnects.
    pub async fn handshake(&self, request: HandshakeRequest) -> Result<HandshakeResponse> {
        let response = self
            .http
            .post(self.endpoint("/v1/handshake"))
            .json(&request)
            .send()
            .await?;
        let handshake: HandshakeResponse = self.decode_json(response).await?;
        if handshake.negotiated_version != PROTOCOL_VERSION
            || !handshake
                .capabilities
                .supported_versions
                .accepts(PROTOCOL_VERSION)
            || handshake.capabilities.minimum_compatible_client > PROTOCOL_VERSION
        {
            return Err(ClientError::Api(ApiError::new(
                ErrorCode::VersionMismatch,
                "this client is not compatible with the server protocol",
            )));
        }
        self.verify_pin(
            &handshake.capabilities,
            Some(&handshake.capabilities.certificate_fingerprint),
        )?;
        Ok(handshake)
    }

    pub async fn pair(&self, request: PairingRequest) -> Result<PairingResponse> {
        let response = self
            .http
            .post(self.endpoint("/v1/pair"))
            .json(&request)
            .send()
            .await?;
        let pairing: PairingResponse = self.decode_json(response).await?;
        self.verify_pin(
            &Capabilities {
                protocol_version: PROTOCOL_VERSION,
                supported_versions: VersionRange::current(),
                minimum_compatible_client: MIN_COMPATIBLE_CLIENT,
                server_id: pairing.server_id,
                certificate_fingerprint: pairing.certificate_fingerprint.clone(),
                server_version: String::new(),
                features: Vec::new(),
                providers: Vec::new(),
                limits: CapabilityLimits::default(),
            },
            Some(&pairing.certificate_fingerprint),
        )?;
        Ok(pairing)
    }

    pub async fn prepare_pairing(&self, role: DeviceRole) -> Result<serde_json::Value> {
        let response = self
            .http
            .post(self.endpoint("/v1/pair/prepare"))
            .json(&role)
            .send()
            .await?;
        self.decode_json(response).await
    }

    pub async fn snapshot(&self) -> Result<Snapshot> {
        let response = self
            .authorized(self.http.get(self.endpoint("/v1/snapshot")))
            .send()
            .await?;
        let snapshot: Snapshot = self.decode_json(response).await?;
        self.verify_pin(
            &Capabilities {
                protocol_version: PROTOCOL_VERSION,
                supported_versions: VersionRange::current(),
                minimum_compatible_client: MIN_COMPATIBLE_CLIENT,
                server_id: snapshot.server_id,
                certificate_fingerprint: snapshot.server.tls_fingerprint.clone(),
                server_version: String::new(),
                features: Vec::new(),
                providers: Vec::new(),
                limits: CapabilityLimits::default(),
            },
            Some(&snapshot.server.tls_fingerprint),
        )?;
        Ok(snapshot)
    }

    /// Fetch an artifact through the authenticated server boundary. Artifact
    /// payloads are intentionally not decoded as JSON and use the larger
    /// protocol cap; metadata remains in snapshots/events.
    pub async fn artifact_bytes(&self, artifact_id: ArtifactId) -> Result<(String, Vec<u8>)> {
        let mut response = self
            .authorized(
                self.http
                    .get(self.endpoint(&format!("/v1/artifacts/{artifact_id}"))),
            )
            .send()
            .await?;
        if response
            .content_length()
            .is_some_and(|length| length > MAX_ARTIFACT_BYTES)
        {
            return Err(ClientError::Api(ApiError::new(
                ErrorCode::PayloadTooLarge,
                "artifact response exceeded the client limit",
            )));
        }
        let status = response.status();
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_string();
        if !status.is_success() {
            // Error envelopes are still bounded independently of the
            // artifact cap. A compromised server must not turn a failed
            // download into an unbounded allocation before the JSON error
            // can be decoded.
            let mut body = Vec::new();
            while let Some(chunk) = response.chunk().await? {
                if chunk.len() > MAX_COMMAND_BODY_BYTES
                    || body.len().saturating_add(chunk.len()) > MAX_COMMAND_BODY_BYTES
                {
                    return Err(ClientError::Api(ApiError::new(
                        ErrorCode::PayloadTooLarge,
                        "artifact error response exceeded the client limit",
                    )));
                }
                body.extend_from_slice(&chunk);
            }
            if let Ok(error) = serde_json::from_slice::<ApiError>(&body) {
                return Err(ClientError::Api(error));
            }
            return Err(ClientError::Api(ApiError::new(
                ErrorCode::NotFound,
                "artifact not found",
            )));
        }
        // Use reqwest's frame-by-frame `chunk()` API instead of `bytes()` so
        // the client applies the same hard cap while data is arriving. The
        // public convenience method still returns a Vec, but no response can
        // force allocation beyond the negotiated artifact limit.
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if chunk.len() > MAX_TERMINAL_FRAME_BYTES * 4
                || (bytes.len() as u64).saturating_add(chunk.len() as u64) > MAX_ARTIFACT_BYTES
            {
                return Err(ClientError::Api(ApiError::new(
                    ErrorCode::PayloadTooLarge,
                    "artifact response exceeded the client limit",
                )));
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok((content_type, bytes))
    }

    /// Append one bounded chunk to a server-side upload.  The offset is sent
    /// explicitly so retries cannot silently reorder bytes; the server is the
    /// authority on the returned contiguous byte count.
    pub async fn upload_artifact_chunk(
        &self,
        upload_id: UploadId,
        offset: u64,
        bytes: Vec<u8>,
    ) -> Result<ArtifactUploadChunkResponse> {
        if bytes.len() > MAX_TERMINAL_FRAME_BYTES * 4 {
            return Err(ClientError::Api(ApiError::new(
                ErrorCode::PayloadTooLarge,
                "artifact chunk is too large",
            )));
        }
        let response = self
            .authorized(
                self.http
                    .put(self.endpoint(&format!("/v1/artifact-uploads/{upload_id}")))
                    .header("x-frank-offset", offset.to_string())
                    .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
                    .body(bytes),
            )
            .send()
            .await?;
        self.decode_json(response).await
    }

    pub async fn browse_projects(&self, path: Option<&str>) -> Result<BrowseResponse> {
        let mut endpoint = self.endpoint("/v1/projects/browse");
        if let Some(path) = path {
            endpoint.query_pairs_mut().append_pair("path", path);
        }
        let response = self.authorized(self.http.get(endpoint)).send().await?;
        self.decode_json(response).await
    }

    pub async fn devices(&self) -> Result<Vec<DeviceSummary>> {
        let response = self
            .authorized(self.http.get(self.endpoint("/v1/devices")))
            .send()
            .await?;
        self.decode_json(response).await
    }

    pub async fn revoke_device(&self, device_id: DeviceId) -> Result<serde_json::Value> {
        let response = self
            .authorized(
                self.http
                    .post(self.endpoint(&format!("/v1/devices/{device_id}/revoke"))),
            )
            .send()
            .await?;
        self.decode_json(response).await
    }

    pub async fn command(
        &self,
        command: Command,
        expected_revision: Option<u64>,
    ) -> Result<CommandResponse> {
        let envelope = CommandEnvelope {
            protocol_version: PROTOCOL_VERSION,
            command_id: CommandId::new(),
            expected_revision,
            command,
        };
        let response = self.command_envelope(envelope).await?;
        if let Some(error) = response.error.clone() {
            return Err(ClientError::Api(error));
        }
        Ok(response)
    }

    pub async fn command_envelope(&self, envelope: CommandEnvelope) -> Result<CommandResponse> {
        let command_id = envelope.command_id;
        let response = self
            .authorized(self.http.post(self.endpoint("/v1/commands")))
            .json(&envelope)
            .send()
            .await?;
        let status = response.status();
        // The server uses an HTTP error status for rejected commands but
        // returns the same CommandResponse envelope so a stale-revision error
        // can carry its latest snapshot. Keep that structured payload for the
        // GUI reducer instead of collapsing it into a string-only transport
        // error. Non-command API errors still use the normal decode path.
        let body = response.bytes().await?;
        if body.len() > MAX_COMMAND_BODY_BYTES {
            return Err(ClientError::Api(ApiError::new(
                ErrorCode::PayloadTooLarge,
                "server response exceeded the client limit",
            )));
        }
        let value = match serde_json::from_slice::<CommandResponse>(&body) {
            Ok(value) => value,
            Err(_) if !status.is_success() => {
                if let Ok(error) = serde_json::from_slice::<ApiError>(&body) {
                    let revision = error
                        .latest_snapshot
                        .as_deref()
                        .map(|snapshot| snapshot.revision)
                        .unwrap_or_default();
                    return Ok(CommandResponse::failed(command_id, revision, error));
                }
                return Err(ClientError::Api(ApiError::new(
                    ErrorCode::Internal,
                    "server request failed",
                )));
            }
            Err(error) => return Err(ClientError::Decode(error)),
        };
        if (status.is_client_error() || status.is_server_error()) && value.error.is_some() {
            // Preserve the command id even when an older server omitted it
            // from a rejected envelope. The response itself remains the
            // authoritative error payload for callers that need a resync.
            return Ok(CommandResponse {
                command_id: if value.command_id == CommandId::nil() {
                    command_id
                } else {
                    value.command_id
                },
                ..value
            });
        }
        Ok(value)
    }

    pub async fn event_stream(&self, after: u64) -> Result<EventStream> {
        let base = self
            .base
            .join("/v1/events")
            .map_err(|error| ClientError::InvalidAddress(error.to_string()))?;
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
        let mut url = self
            .base
            .join(&format!("/v1/terminals/{session_id}"))
            .map_err(|error| ClientError::InvalidAddress(error.to_string()))?;
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

    pub async fn reconnecting_events(&self, after: u64) -> ReconnectingEvents {
        ReconnectingEvents {
            client: self.clone(),
            cursor: after,
            backoff: Duration::from_millis(250),
            stream: None,
        }
    }

    fn endpoint(&self, path: &str) -> Url {
        self.base.join(path).unwrap_or_else(|_| self.base.clone())
    }

    fn websocket_connector(&self) -> Result<Option<Connector>> {
        if let Some(pin) = &self.config.pinned_certificate_fingerprint {
            return Ok(Some(Connector::Rustls(Arc::new(pinned_tls_config(pin)?))));
        }
        let Some(pem) = &self.config.ca_certificate_pem else {
            return Ok(None);
        };
        let mut reader = Cursor::new(pem);
        let certificates = rustls_pemfile::certs(&mut reader)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|error| ClientError::Tls(error.to_string()))?;
        if certificates.is_empty() {
            return Err(ClientError::Tls(
                "CA PEM did not contain a certificate".into(),
            ));
        }
        let mut roots = rustls::RootCertStore::empty();
        for certificate in certificates {
            roots
                .add(certificate)
                .map_err(|error| ClientError::Tls(error.to_string()))?;
        }
        let config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        Ok(Some(Connector::Rustls(Arc::new(config))))
    }

    fn websocket_request(&self, url: Url) -> Result<Request<()>> {
        // Let tungstenite create the complete RFC 6455 handshake first. A
        // hand-built request is subtly incomplete: without the
        // `Sec-WebSocket-Key`/Upgrade headers Axum rejects it before the
        // event stream handler runs, which sends the actor into reconnect
        // backoff forever.
        let mut request = url.as_str().into_client_request()?;
        if let Some(token) = &self.config.device_token {
            // Keep bearer credentials in the TLS-protected header rather than
            // the URL.  This prevents accidental leakage through proxy/access
            // logs while retaining compatibility with the server's header
            // authentication path.
            request.headers_mut().insert(
                "authorization",
                HeaderValue::from_str(&format!("Bearer {token}"))
                    .map_err(|error| ClientError::InvalidAddress(error.to_string()))?,
            );
        }
        if let Some(token) = &self.config.agent_session_token {
            request.headers_mut().insert(
                "x-frank-agent-token",
                HeaderValue::from_str(token)
                    .map_err(|error| ClientError::InvalidAddress(error.to_string()))?,
            );
        }
        Ok(request)
    }

    fn authorized(&self, request: RequestBuilder) -> RequestBuilder {
        let request = match &self.config.device_token {
            Some(token) => request.bearer_auth(token),
            None => request,
        };
        match &self.config.agent_session_token {
            Some(token) => request.header("x-frank-agent-token", token),
            None => request,
        }
    }

    async fn decode_json<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T> {
        let status = response.status();
        let body = response.bytes().await?;
        if body.len() > MAX_COMMAND_BODY_BYTES {
            return Err(ClientError::Api(ApiError::new(
                ErrorCode::PayloadTooLarge,
                "server response exceeded the client limit",
            )));
        }
        if status == StatusCode::UPGRADE_REQUIRED {
            return Err(ClientError::Api(ApiError::new(
                ErrorCode::VersionMismatch,
                "client update required",
            )));
        }
        if !status.is_success() {
            if let Ok(error) = serde_json::from_slice::<ApiError>(&body) {
                return Err(ClientError::Api(error));
            }
            return Err(ClientError::Api(ApiError::new(
                ErrorCode::Internal,
                "server request failed",
            )));
        }
        Ok(serde_json::from_slice(&body)?)
    }

    fn verify_pin(
        &self,
        _capabilities: &Capabilities,
        snapshot_fingerprint: Option<&str>,
    ) -> Result<()> {
        let Some(expected) = &self.config.pinned_certificate_fingerprint else {
            return Ok(());
        };
        let observed = snapshot_fingerprint
            .filter(|fingerprint| !fingerprint.is_empty())
            .map(str::to_string);
        if observed.as_deref() == Some(expected.as_str()) {
            Ok(())
        } else if observed.is_none() {
            // The TLS connector already verified the CA/hostname.  The
            // capabilities endpoint has no certificate field in older v1
            // servers, so retain the pin until snapshot supplies one.
            Ok(())
        } else {
            Err(ClientError::CertificatePinMismatch)
        }
    }
}

/// Rustls 0.23 cannot choose a process-wide backend when both `ring` and
/// `aws-lc-rs` are linked through the workspace.  Install the same backend as
/// `frankd` before constructing reqwest or a pinned WebSocket connector.
fn install_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

/// Build a TLS client that trusts exactly one certificate leaf.  The pin is
/// checked before any HTTP/WebSocket bytes are accepted, while Rustls still
/// validates the server's handshake signatures through its normal crypto
/// provider.  This avoids the insecure `danger_accept_invalid_certs` escape
/// hatch for generated, self-signed Frank identities.
fn pinned_tls_config(fingerprint: &str) -> Result<rustls::ClientConfig> {
    let expected = fingerprint.trim().to_ascii_lowercase();
    let valid_shape = expected
        .strip_prefix("sha256:")
        .is_some_and(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()));
    if !valid_shape {
        return Err(ClientError::Tls(
            "certificate fingerprint must be sha256:<64 lowercase hexadecimal characters>".into(),
        ));
    }
    let verifier = PinnedCertificateVerifier::new(expected);
    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    Ok(config)
}

#[derive(Debug)]
struct PinnedCertificateVerifier {
    expected: String,
    algorithms: rustls::crypto::WebPkiSupportedAlgorithms,
}

impl PinnedCertificateVerifier {
    fn new(expected: String) -> Self {
        let provider = rustls::crypto::CryptoProvider::get_default()
            .expect("rustls crypto provider must be installed")
            .clone();
        Self {
            expected,
            algorithms: provider.signature_verification_algorithms,
        }
    }
}

impl ServerCertVerifier for PinnedCertificateVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> std::result::Result<ServerCertVerified, rustls::Error> {
        let digest = sha2::Sha256::digest(end_entity.as_ref());
        let observed = format!("sha256:{}", hex::encode(digest));
        if observed == self.expected {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "server certificate fingerprint does not match the pinned identity".into(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> std::result::Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

pub struct EventStream {
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
}

pub struct TerminalStream {
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
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
                "wss://127.0.0.1:37465/v1/events?after=0"
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
