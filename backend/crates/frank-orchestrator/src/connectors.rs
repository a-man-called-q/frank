//! Connector boundaries shared by the runtime and task-scoped MCP bridge.
//!
//! The first implementation keeps provider credentials and network clients in
//! the daemon.  This module contains the provider-neutral registry contract
//! and the fail-closed database statement classifier used before an adapter is
//! selected.

use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use frank_protocol::ConnectorProfileId;
use futures_util::{SinkExt, StreamExt};
use reqwest::Url;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::process::{Child, Command as AsyncCommand};
use tokio::time::sleep;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async, tungstenite::Message};

/// Credential lookup is deliberately a one-way daemon boundary. The
/// resolver is injected by `frankd`; the resolver is never part of a
/// protocol snapshot, command, event, or provider child process.
#[async_trait]
pub trait ConnectorSecretResolver: Send + Sync {
    async fn secret(
        &self,
        profile_id: ConnectorProfileId,
    ) -> Result<Option<String>, ConnectorError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseStatementClass {
    ReadOnly,
    Write,
    Reject,
}

/// Minimum Google Workspace scopes used by the built-in adapter. A single
/// OAuth profile can service Gmail, Calendar, and Drive; callers select the
/// individual API operation only after Organization grants it.
pub const GOOGLE_WORKSPACE_SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/gmail.modify",
    "https://www.googleapis.com/auth/calendar",
    "https://www.googleapis.com/auth/drive.file",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GooglePkceChallenge {
    pub state: String,
    pub nonce: String,
    pub verifier: String,
    pub challenge: String,
}

impl GooglePkceChallenge {
    /// Generate an OAuth Authorization Code + PKCE tuple. The verifier and
    /// nonce are returned to the daemon caller and must remain in memory (or
    /// its credential store), never in Organization metadata.
    pub fn generate() -> Result<Self, ConnectorError> {
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes)
            .map_err(|error| ConnectorError::Invalid(format!("randomness unavailable: {error}")))?;
        let state = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
        getrandom::fill(&mut bytes)
            .map_err(|error| ConnectorError::Invalid(format!("randomness unavailable: {error}")))?;
        let nonce = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
        getrandom::fill(&mut bytes)
            .map_err(|error| ConnectorError::Invalid(format!("randomness unavailable: {error}")))?;
        let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(Sha256::digest(verifier.as_bytes()));
        Ok(Self {
            state,
            nonce,
            verifier,
            challenge,
        })
    }

    pub fn authorization_url(
        &self,
        client_id: &str,
        redirect_uri: &str,
        scopes: &[&str],
    ) -> Result<Url, ConnectorError> {
        if client_id.trim().is_empty() || redirect_uri.trim().is_empty() || scopes.is_empty() {
            return Err(ConnectorError::Invalid(
                "Google OAuth client, redirect URI, and scopes are required".into(),
            ));
        }
        let mut url = Url::parse("https://accounts.google.com/o/oauth2/v2/auth")
            .map_err(|error| ConnectorError::Invalid(error.to_string()))?;
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("client_id", client_id)
                .append_pair("redirect_uri", redirect_uri)
                .append_pair("response_type", "code")
                .append_pair("access_type", "offline")
                .append_pair("prompt", "consent")
                .append_pair("scope", &scopes.join(" "))
                .append_pair("state", &self.state)
                .append_pair("nonce", &self.nonce)
                .append_pair("code_challenge", &self.challenge)
                .append_pair("code_challenge_method", "S256");
        }
        Ok(url)
    }

    pub fn validate_callback(
        &self,
        state: &str,
        nonce: Option<&str>,
    ) -> Result<(), ConnectorError> {
        // Both values are one-time binders for the authorization response.
        // Accepting a missing nonce would turn the state check into the only
        // CSRF/replay boundary for providers that return an ID token.
        if state != self.state || nonce != Some(self.nonce.as_str()) {
            return Err(ConnectorError::Invalid(
                "Google OAuth state or nonce did not match".into(),
            ));
        }
        Ok(())
    }

    /// Validate the OAuth CSRF binder for providers that do not echo the
    /// optional OIDC nonce in a code-only redirect.  Callers that receive an
    /// ID-token nonce should prefer `validate_callback`, which checks both.
    pub fn validate_state(&self, state: &str) -> Result<(), ConnectorError> {
        if state != self.state {
            return Err(ConnectorError::Invalid(
                "Google OAuth state did not match".into(),
            ));
        }
        Ok(())
    }
}

/// Small Google OAuth code-flow client. It is intentionally provider-neutral:
/// the caller decides where the returned token bundle is stored. The bundle
/// must be handed directly to the daemon credential store and is never
/// serialized into an Organization DTO.
#[derive(Clone)]
pub struct GoogleOAuthClient {
    pub client: reqwest::Client,
    pub token_endpoint: Url,
    pub revoke_endpoint: Url,
}

impl std::fmt::Debug for GoogleOAuthClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GoogleOAuthClient")
            .field("token_endpoint", &self.token_endpoint)
            .field("revoke_endpoint", &self.revoke_endpoint)
            .finish_non_exhaustive()
    }
}

impl Default for GoogleOAuthClient {
    fn default() -> Self {
        Self {
            client: reqwest::Client::new(),
            token_endpoint: Url::parse("https://oauth2.googleapis.com/token")
                .expect("Google token endpoint is a valid URL"),
            revoke_endpoint: Url::parse("https://oauth2.googleapis.com/revoke")
                .expect("Google revoke endpoint is a valid URL"),
        }
    }
}

impl GoogleOAuthClient {
    pub async fn exchange_code(
        &self,
        client_id: &str,
        client_secret: Option<&str>,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
    ) -> Result<Value, ConnectorError> {
        if client_id.trim().is_empty()
            || code.trim().is_empty()
            || verifier.trim().is_empty()
            || redirect_uri.trim().is_empty()
        {
            return Err(ConnectorError::Invalid(
                "Google OAuth code exchange parameters are incomplete".into(),
            ));
        }
        let mut form = vec![
            ("client_id", client_id),
            ("code", code),
            ("code_verifier", verifier),
            ("grant_type", "authorization_code"),
            ("redirect_uri", redirect_uri),
        ];
        if let Some(client_secret) = client_secret.filter(|value| !value.trim().is_empty()) {
            form.push(("client_secret", client_secret));
        }
        self.post_token_form(form).await
    }

    pub async fn refresh(
        &self,
        client_id: &str,
        client_secret: Option<&str>,
        refresh_token: &str,
    ) -> Result<Value, ConnectorError> {
        if client_id.trim().is_empty() || refresh_token.trim().is_empty() {
            return Err(ConnectorError::Invalid(
                "Google OAuth refresh parameters are incomplete".into(),
            ));
        }
        let mut form = vec![
            ("client_id", client_id),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ];
        if let Some(client_secret) = client_secret.filter(|value| !value.trim().is_empty()) {
            form.push(("client_secret", client_secret));
        }
        self.post_token_form(form).await
    }

    pub async fn revoke(&self, token: &str) -> Result<(), ConnectorError> {
        if token.trim().is_empty() {
            return Err(ConnectorError::Invalid(
                "Google OAuth revoke token is required".into(),
            ));
        }
        let response = self
            .client
            .post(self.revoke_endpoint.clone())
            .form(&[("token", token)])
            .send()
            .await
            .map_err(|error| {
                ConnectorError::Invalid(sanitize_connector_error(&error.to_string(), Some(token)))
            })?;
        if response.status().is_success() || response.status() == reqwest::StatusCode::BAD_REQUEST {
            return Ok(());
        }
        Err(ConnectorError::Invalid(format!(
            "Google OAuth revoke failed with {}",
            response.status()
        )))
    }

    async fn post_token_form<'a>(
        &self,
        form: Vec<(&'a str, &'a str)>,
    ) -> Result<Value, ConnectorError> {
        let response = self
            .client
            .post(self.token_endpoint.clone())
            .form(&form)
            .send()
            .await
            .map_err(|error| ConnectorError::Invalid(error.to_string()))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| ConnectorError::Invalid(error.to_string()))?;
        if !status.is_success() {
            return Err(ConnectorError::Invalid(format!(
                "Google OAuth token endpoint returned {}",
                status
            )));
        }
        serde_json::from_str(&body).map_err(|error| {
            ConnectorError::Invalid(format!("invalid Google OAuth response: {error}"))
        })
    }
}

fn sanitize_connector_error(error: &str, secret: Option<&str>) -> String {
    let mut value = error.to_owned();
    if let Some(secret) = secret.filter(|value| !value.is_empty()) {
        value = value.replace(secret, "[redacted]");
    }
    value.chars().take(512).collect()
}

/// Browser policy is deliberately independent from Chrome/CDP process
/// management. It is shared by the CDP and compatibility adapters and
/// unit-tested here so SSRF and response/download limits cannot be bypassed by
/// a provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserPolicy {
    pub allowed_domains: Vec<String>,
    pub max_response_bytes: usize,
    pub max_download_bytes: usize,
}

impl Default for BrowserPolicy {
    fn default() -> Self {
        Self {
            allowed_domains: Vec::new(),
            max_response_bytes: 2 * 1024 * 1024,
            max_download_bytes: 32 * 1024 * 1024,
        }
    }
}

impl BrowserPolicy {
    pub fn validate_url(&self, raw: &str) -> Result<Url, ConnectorError> {
        let url = Url::parse(raw).map_err(|error| ConnectorError::Invalid(error.to_string()))?;
        if !matches!(url.scheme(), "http" | "https") || url.username() != "" {
            return Err(ConnectorError::Invalid(
                "browser URL must be HTTP(S) without embedded credentials".into(),
            ));
        }
        let host = url
            .host_str()
            .ok_or_else(|| ConnectorError::Invalid("browser URL has no host".into()))?
            .to_ascii_lowercase();
        if host == "localhost"
            || host.ends_with(".localhost")
            || host.ends_with(".local")
            || host.ends_with(".internal")
            || host.parse::<IpAddr>().is_ok_and(is_private_or_local_ip)
        {
            return Err(ConnectorError::Invalid(
                "browser URL targets a private or local address".into(),
            ));
        }
        if !self.allowed_domains.is_empty()
            && !self.allowed_domains.iter().any(|allowed| {
                let allowed = allowed.trim().trim_start_matches("*.").to_ascii_lowercase();
                host == allowed || host.ends_with(&format!(".{allowed}"))
            })
        {
            return Err(ConnectorError::Invalid(
                "browser URL is outside the Organization domain allowlist".into(),
            ));
        }
        Ok(url)
    }

    pub fn enforce_response_limit(&self, bytes: usize) -> Result<(), ConnectorError> {
        if bytes > self.max_response_bytes {
            return Err(ConnectorError::Invalid(
                "browser response exceeds the configured limit".into(),
            ));
        }
        Ok(())
    }

    pub fn enforce_download_limit(&self, bytes: usize) -> Result<(), ConnectorError> {
        if bytes > self.max_download_bytes {
            return Err(ConnectorError::Invalid(
                "browser download exceeds the configured limit".into(),
            ));
        }
        Ok(())
    }
}

/// Configuration for a daemon-owned, task-isolated Chromium process.  The
/// executable is checked before launch and the child receives a minimal
/// environment; provider input never becomes a shell command or a browser
/// flag.
#[derive(Debug, Clone)]
pub struct BrowserLaunchConfig {
    pub executable: PathBuf,
    pub user_data_dir: PathBuf,
    pub startup_timeout: Duration,
}

impl BrowserLaunchConfig {
    pub fn new(executable: impl Into<PathBuf>, user_data_dir: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            user_data_dir: user_data_dir.into(),
            startup_timeout: Duration::from_secs(10),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct ChromeVersionResponse {
    #[serde(rename = "webSocketDebuggerUrl")]
    web_socket_debugger_url: String,
}

/// A small CDP client that serializes commands on one browser websocket.  A
/// fresh browser context is created for every request and disposed before the
/// process exits, so cookies/local storage cannot leak between tasks.
pub struct BrowserCdpSession {
    child: Child,
    socket: WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>,
    next_id: u64,
    user_data_dir: PathBuf,
    cleanup_user_data_dir: bool,
}

impl std::fmt::Debug for BrowserCdpSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BrowserCdpSession")
            .field("next_id", &self.next_id)
            .field("user_data_dir", &self.user_data_dir)
            .field("cleanup_user_data_dir", &self.cleanup_user_data_dir)
            .finish_non_exhaustive()
    }
}

impl BrowserCdpSession {
    pub async fn launch(config: BrowserLaunchConfig) -> Result<Self, ConnectorError> {
        let metadata = std::fs::symlink_metadata(&config.executable).map_err(|error| {
            ConnectorError::Invalid(format!("browser executable is unavailable: {error}"))
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(ConnectorError::Invalid(
                "browser executable must be a regular non-symlink file".into(),
            ));
        }
        let cleanup_user_data_dir = !config.user_data_dir.exists();
        if config.user_data_dir.exists() {
            let metadata = std::fs::symlink_metadata(&config.user_data_dir).map_err(|error| {
                ConnectorError::Invalid(format!(
                    "browser profile directory is unavailable: {error}"
                ))
            })?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(ConnectorError::Invalid(
                    "browser profile directory must be a non-symlink directory".into(),
                ));
            }
        } else {
            std::fs::create_dir_all(&config.user_data_dir).map_err(|error| {
                ConnectorError::Invalid(format!(
                    "browser profile directory could not be created: {error}"
                ))
            })?;
        }
        let metadata = std::fs::symlink_metadata(&config.user_data_dir).map_err(|error| {
            ConnectorError::Invalid(format!("browser profile directory is unavailable: {error}"))
        })?;
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            return Err(ConnectorError::Invalid(
                "browser profile directory must be a non-symlink directory".into(),
            ));
        }

        let listener =
            std::net::TcpListener::bind((std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 0))
                .map_err(|error| {
                ConnectorError::Invalid(format!("browser debug port unavailable: {error}"))
            })?;
        let port = listener
            .local_addr()
            .map_err(|error| {
                ConnectorError::Invalid(format!("browser debug port unavailable: {error}"))
            })?
            .port();
        drop(listener);

        let mut command = AsyncCommand::new(&config.executable);
        command
            .arg("--headless=new")
            .arg("--disable-gpu")
            .arg("--disable-dev-shm-usage")
            .arg("--disable-extensions")
            .arg("--disable-sync")
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--remote-debugging-address=127.0.0.1")
            .arg(format!("--remote-debugging-port={port}"))
            .arg(format!(
                "--user-data-dir={}",
                config.user_data_dir.display()
            ))
            .arg("about:blank")
            .env_clear()
            .envs(safe_terminal_environment())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = command.spawn().map_err(|error| {
            ConnectorError::Invalid(format!("browser process could not start: {error}"))
        })?;
        let endpoint = format!("http://127.0.0.1:{port}/json/version");
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(500))
            .build()
            .map_err(|error| ConnectorError::Invalid(error.to_string()))?;
        let deadline = tokio::time::Instant::now() + config.startup_timeout;
        let version = loop {
            if tokio::time::Instant::now() >= deadline {
                let _ = child.kill().await;
                if cleanup_user_data_dir {
                    let _ = tokio::fs::remove_dir_all(&config.user_data_dir).await;
                }
                return Err(ConnectorError::Invalid(
                    "browser CDP endpoint did not become ready".into(),
                ));
            }
            match client.get(&endpoint).send().await {
                Ok(response) if response.status().is_success() => {
                    match response.json::<ChromeVersionResponse>().await {
                        Ok(version) if !version.web_socket_debugger_url.trim().is_empty() => {
                            break version;
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            sleep(Duration::from_millis(50)).await;
        };
        let (socket, _) = match connect_async(version.web_socket_debugger_url).await {
            Ok(connection) => connection,
            Err(error) => {
                let _ = child.kill().await;
                if cleanup_user_data_dir {
                    let _ = tokio::fs::remove_dir_all(&config.user_data_dir).await;
                }
                return Err(ConnectorError::Invalid(format!(
                    "browser CDP websocket could not connect: {error}"
                )));
            }
        };
        Ok(Self {
            child,
            socket,
            next_id: 1,
            user_data_dir: config.user_data_dir,
            cleanup_user_data_dir,
        })
    }

    async fn command(
        &mut self,
        method: &str,
        params: Value,
        session_id: Option<&str>,
    ) -> Result<Value, ConnectorError> {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let mut request = json!({"id": id, "method": method, "params": params});
        if let Some(session_id) = session_id {
            request["sessionId"] = json!(session_id);
        }
        self.socket
            .send(Message::Text(request.to_string().into()))
            .await
            .map_err(|error| {
                ConnectorError::Invalid(format!("browser CDP send failed: {error}"))
            })?;
        while let Some(message) = self.socket.next().await {
            let message = message.map_err(|error| {
                ConnectorError::Invalid(format!("browser CDP receive failed: {error}"))
            })?;
            let Message::Text(text) = message else {
                continue;
            };
            let value: Value = serde_json::from_str(&text).map_err(|error| {
                ConnectorError::Invalid(format!("browser CDP JSON invalid: {error}"))
            })?;
            if value.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = value.get("error") {
                return Err(ConnectorError::Invalid(format!(
                    "browser CDP command failed: {error}"
                )));
            }
            return Ok(value.get("result").cloned().unwrap_or(Value::Null));
        }
        Err(ConnectorError::Invalid(
            "browser CDP websocket closed".into(),
        ))
    }

    pub async fn browse(
        &mut self,
        url: &Url,
        name: &str,
        policy: &BrowserPolicy,
    ) -> Result<Value, ConnectorError> {
        let context = self
            .command(
                "Target.createBrowserContext",
                json!({"disposeOnDetach": true}),
                None,
            )
            .await?
            .get("browserContextId")
            .and_then(Value::as_str)
            .ok_or_else(|| ConnectorError::Invalid("browser CDP context was not created".into()))?
            .to_owned();
        let target = self
            .command(
                "Target.createTarget",
                json!({"url": "about:blank", "browserContextId": context}),
                None,
            )
            .await?
            .get("targetId")
            .and_then(Value::as_str)
            .ok_or_else(|| ConnectorError::Invalid("browser CDP target was not created".into()))?
            .to_owned();
        let session = self
            .command(
                "Target.attachToTarget",
                json!({"targetId": target, "flatten": true}),
                None,
            )
            .await?
            .get("sessionId")
            .and_then(Value::as_str)
            .ok_or_else(|| ConnectorError::Invalid("browser CDP target could not attach".into()))?
            .to_owned();
        self.command("Page.enable", json!({}), Some(&session))
            .await?;
        self.command("Runtime.enable", json!({}), Some(&session))
            .await?;
        self.command(
            "Page.navigate",
            json!({"url": url.as_str()}),
            Some(&session),
        )
        .await?;
        for _ in 0..100 {
            let state = self
                .command(
                    "Runtime.evaluate",
                    json!({"expression": "document.readyState", "returnByValue": true}),
                    Some(&session),
                )
                .await?
                .get("result")
                .and_then(|value| value.get("value"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            if matches!(state.as_str(), "interactive" | "complete") {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
        let expression = if name == "browser_download" {
            format!(
                "(async()=>{{const r=await fetch({});const b=new Uint8Array(await r.arrayBuffer());let s='';for(const x of b)s+=String.fromCharCode(x);return {{bytes_base64:btoa(s),mime_type:r.headers.get('content-type')||'application/octet-stream'}}}})()",
                serde_json::to_string(url.as_str()).unwrap_or_else(|_| "\"\"".into())
            )
        } else {
            "document.documentElement?.innerText || document.body?.innerText || \"\"".into()
        };
        let value = self
            .command(
                "Runtime.evaluate",
                json!({"expression": expression, "returnByValue": true, "awaitPromise": true}),
                Some(&session),
            )
            .await?
            .get("result")
            .and_then(|value| value.get("result"))
            .and_then(|value| value.get("value"))
            .cloned()
            .unwrap_or(Value::Null);
        let result = if name == "browser_download" {
            let encoded = value
                .get("bytes_base64")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    ConnectorError::Invalid("browser download returned no bytes".into())
                })?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|error| {
                    ConnectorError::Invalid(format!("browser download encoding failed: {error}"))
                })?;
            policy.enforce_download_limit(bytes.len())?;
            json!({
                "url": url.as_str(),
                "name": url.path_segments().and_then(|mut segments| segments.next_back()).filter(|name| !name.is_empty()).unwrap_or("browser-download.bin"),
                "mime_type": value.get("mime_type").and_then(Value::as_str).unwrap_or("application/octet-stream"),
                "bytes_base64": base64::engine::general_purpose::STANDARD.encode(bytes),
            })
        } else {
            let text = value.as_str().unwrap_or_default();
            policy.enforce_response_limit(text.len())?;
            json!({"url": url.as_str(), "status": 200, "content": text})
        };
        let _ = self
            .command(
                "Target.disposeBrowserContext",
                json!({"browserContextId": context}),
                None,
            )
            .await;
        Ok(result)
    }

    pub async fn close(mut self) {
        let _ = self.socket.close(None).await;
        let _ = self.child.kill().await;
        if self.cleanup_user_data_dir {
            let _ = tokio::fs::remove_dir_all(&self.user_data_dir).await;
        }
    }
}

fn is_private_or_local_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_unspecified()
                || ip.octets()[0] == 169 && ip.octets()[1] == 254
        }
        IpAddr::V6(ip) => {
            ip.is_loopback()
                || ip.is_unspecified()
                || ip.is_unique_local()
                || ip
                    .to_ipv4_mapped()
                    .is_some_and(|mapped| is_private_or_local_ip(IpAddr::V4(mapped)))
        }
    }
}

/// A terminal boundary that can be used by the daemon before spawning a
/// command. It never accepts an environment map from a provider and rejects
/// direct Git writes, which remain the orchestrator's responsibility.
pub fn validate_terminal_command(command: &str) -> Result<(), ConnectorError> {
    if command.trim().is_empty()
        || command.chars().any(char::is_control)
        || command.len() > 256 * 1024
    {
        return Err(ConnectorError::Invalid(
            "terminal command is invalid".into(),
        ));
    }
    let tokens = command
        .split_whitespace()
        .map(|token| token.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let forbidden = ["sudo", "su", "doas"];
    let mut git_command = false;
    let mut git_write = false;
    for token in &tokens {
        if matches!(token.as_str(), "&&" | "||" | "|" | ";") {
            git_command = false;
            continue;
        }
        if token == "git" {
            git_command = true;
            continue;
        }
        if git_command
            && matches!(
                token.as_str(),
                "add" | "commit" | "push" | "reset" | "checkout" | "switch" | "merge"
            )
        {
            git_write = true;
        }
    }
    if git_write
        || forbidden
            .iter()
            .any(|needle| tokens.iter().any(|token| token == needle))
    {
        return Err(ConnectorError::Invalid(
            "direct Git writes and privilege escalation are not allowed in terminal tool calls"
                .into(),
        ));
    }
    Ok(())
}

pub fn safe_terminal_environment() -> Vec<(&'static str, &'static str)> {
    vec![("PATH", "/usr/bin:/bin:/usr/local/bin"), ("LC_ALL", "C")]
}

pub fn validate_sqlite_path(
    path: impl AsRef<Path>,
    allowed_roots: &[PathBuf],
) -> Result<PathBuf, ConnectorError> {
    let path = path.as_ref();
    if path.as_os_str().is_empty() || allowed_roots.is_empty() {
        return Err(ConnectorError::Invalid(
            "SQLite path is not configured".into(),
        ));
    }
    let canonical = std::fs::canonicalize(path)
        .map_err(|error| ConnectorError::Invalid(format!("SQLite path is unavailable: {error}")))?;
    if allowed_roots
        .iter()
        .any(|root| path_contains_symlink_below_root(path, root))
    {
        return Err(ConnectorError::Invalid(
            "SQLite path may not traverse a symlink".into(),
        ));
    }
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| ConnectorError::Invalid(error.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ConnectorError::Invalid(
            "SQLite path must be a regular non-symlink file".into(),
        ));
    }
    if !allowed_roots.iter().any(|root| {
        std::fs::canonicalize(root)
            .ok()
            .is_some_and(|root| canonical.starts_with(root))
    }) {
        return Err(ConnectorError::Invalid(
            "SQLite path is outside the allowed project roots".into(),
        ));
    }
    Ok(canonical)
}

fn path_contains_symlink_below_root(path: &Path, root: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };
    let mut current = root.to_path_buf();
    relative.components().any(|component| {
        current.push(component.as_os_str());
        std::fs::symlink_metadata(&current)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
    })
}

pub fn validate_postgres_profile_config(config: &Value) -> Result<(), ConnectorError> {
    let object = config.as_object().ok_or_else(|| {
        ConnectorError::Invalid("PostgreSQL profile config must be an object".into())
    })?;
    if object.keys().any(|key| {
        let key = key.to_ascii_lowercase();
        key.contains("dsn")
            || key.contains("password")
            || key.contains("token")
            || key.contains("secret")
            || key.contains("credential")
    }) {
        return Err(ConnectorError::Invalid(
            "PostgreSQL DSN and credentials belong in the daemon credential store".into(),
        ));
    }
    Ok(())
}

/// Classify one SQL statement without attempting to parse or execute it.
/// Unknown syntax is rejected rather than guessed.  A caller must still pass
/// a `Write` result through the durable approval queue.
pub fn classify_database_statement(statement: &str) -> DatabaseStatementClass {
    let trimmed = statement.trim();
    let lowered_statement = trimmed.to_ascii_lowercase();
    if trimmed.is_empty()
        || trimmed.contains('\0')
        || trimmed.contains(';')
        || trimmed.contains("--")
        || trimmed.contains("/*")
        || trimmed.contains("*/")
        || lowered_statement.contains("load_extension")
        || lowered_statement.contains("attach ")
    {
        return DatabaseStatementClass::Reject;
    }
    let mut tokens = trimmed.split_whitespace();
    let Some(first) = tokens.next().map(|token| token.to_ascii_lowercase()) else {
        return DatabaseStatementClass::Reject;
    };
    match first.as_str() {
        "select" => {
            let rest = tokens
                .map(|token| token.to_ascii_lowercase())
                .collect::<Vec<_>>();
            let has_lock = rest.windows(2).any(|window| {
                window[0] == "for" && matches!(window[1].as_str(), "update" | "share" | "no")
            });
            if rest.iter().any(|token| token == "into") || has_lock {
                DatabaseStatementClass::Reject
            } else {
                DatabaseStatementClass::ReadOnly
            }
        }
        "show" | "describe" | "desc" => DatabaseStatementClass::ReadOnly,
        "explain" => {
            let rest = tokens
                .map(|token| token.to_ascii_lowercase())
                .collect::<Vec<_>>();
            if rest
                .iter()
                .any(|token| token == "analyze" || token == "analyse")
                || rest.iter().any(|token| {
                    matches!(
                        token.as_str(),
                        "insert"
                            | "update"
                            | "delete"
                            | "merge"
                            | "replace"
                            | "create"
                            | "alter"
                            | "drop"
                            | "truncate"
                    )
                })
            {
                DatabaseStatementClass::Reject
            } else {
                DatabaseStatementClass::ReadOnly
            }
        }
        "pragma" => {
            let rest = tokens.collect::<Vec<_>>().join(" ").to_ascii_lowercase();
            if rest.starts_with("table_info(")
                || rest.starts_with("table_xinfo(")
                || rest.starts_with("index_list(")
                || rest.starts_with("foreign_key_list(")
            {
                DatabaseStatementClass::ReadOnly
            } else {
                DatabaseStatementClass::Reject
            }
        }
        "insert" | "update" | "delete" | "merge" | "replace" | "create" | "alter" | "drop"
        | "truncate" | "grant" | "revoke" | "vacuum" | "reindex" => DatabaseStatementClass::Write,
        // WITH can contain INSERT/UPDATE/DELETE, so it cannot be treated as
        // read-only without a full SQL parser.
        _ => DatabaseStatementClass::Reject,
    }
}

#[derive(Debug, Error)]
pub enum ConnectorError {
    #[error("connector adapter is unavailable")]
    Unavailable,
    #[error("connector operation is not supported")]
    Unsupported,
    #[error("connector request is invalid: {0}")]
    Invalid(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_classifier_is_read_only_by_allowlist() {
        assert_eq!(
            classify_database_statement("SELECT id FROM users"),
            DatabaseStatementClass::ReadOnly
        );
        assert_eq!(
            classify_database_statement("PRAGMA table_info(users)"),
            DatabaseStatementClass::ReadOnly
        );
        assert_eq!(
            classify_database_statement("PRAGMA journal_mode=WAL"),
            DatabaseStatementClass::Reject
        );
        assert_eq!(
            classify_database_statement("SELECT 1; DELETE FROM users"),
            DatabaseStatementClass::Reject
        );
        assert_eq!(
            classify_database_statement("WITH x AS (SELECT 1) SELECT * FROM x"),
            DatabaseStatementClass::Reject
        );
        assert_eq!(
            classify_database_statement("EXPLAIN ANALYZE SELECT * FROM users"),
            DatabaseStatementClass::Reject
        );
        assert_eq!(
            classify_database_statement("SELECT id INTO archive FROM users"),
            DatabaseStatementClass::Reject
        );
        assert_eq!(
            classify_database_statement("EXPLAIN UPDATE users SET active = 0"),
            DatabaseStatementClass::Reject
        );
        assert_eq!(
            classify_database_statement("SELECT load_extension('unsafe')"),
            DatabaseStatementClass::Reject
        );
        assert_eq!(
            classify_database_statement("UPDATE users SET active = 0"),
            DatabaseStatementClass::Write
        );
        assert_eq!(
            classify_database_statement("DROP TABLE users"),
            DatabaseStatementClass::Write
        );
    }

    #[test]
    fn google_oauth_uses_pkce_state_and_nonce() {
        let challenge = GooglePkceChallenge::generate().unwrap();
        let url = challenge
            .authorization_url(
                "client-id",
                "http://127.0.0.1/callback",
                GOOGLE_WORKSPACE_SCOPES,
            )
            .unwrap();
        let query = url.query().unwrap();
        assert!(query.contains("code_challenge_method=S256"));
        assert!(query.contains("state="));
        assert!(query.contains("nonce="));
        challenge
            .validate_callback(&challenge.state, Some(&challenge.nonce))
            .unwrap();
        challenge.validate_state(&challenge.state).unwrap();
        assert!(challenge.validate_callback(&challenge.state, None).is_err());
        assert!(challenge.validate_callback("wrong", None).is_err());
    }

    #[test]
    fn browser_policy_rejects_ssrf_and_caps_payloads() {
        let policy = BrowserPolicy {
            allowed_domains: vec!["example.com".into()],
            ..BrowserPolicy::default()
        };
        assert!(policy.validate_url("https://example.com/docs").is_ok());
        assert!(policy.validate_url("http://127.0.0.1/admin").is_err());
        assert!(policy.validate_url("https://other.example").is_err());
        assert!(
            policy
                .enforce_response_limit(policy.max_response_bytes + 1)
                .is_err()
        );
        assert!(
            policy
                .enforce_download_limit(policy.max_download_bytes + 1)
                .is_err()
        );
    }

    #[test]
    fn terminal_and_database_profiles_fail_closed() {
        assert!(validate_terminal_command("cargo test").is_ok());
        assert!(validate_terminal_command("git commit -am bad").is_err());
        assert!(validate_terminal_command("git -C worktree\tcommit -am bad").is_err());
        assert!(validate_terminal_command("cargo test\nrm -rf /tmp").is_err());
        assert!(validate_terminal_command("sudo whoami").is_err());
        assert!(
            validate_postgres_profile_config(&serde_json::json!({
                "schema": "public",
                "tables": ["jobs"]
            }))
            .is_ok()
        );
        assert!(
            validate_postgres_profile_config(&serde_json::json!({
                "dsn": "postgres://user:pass@host/db"
            }))
            .is_err()
        );
    }
}
