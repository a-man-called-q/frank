//! Owner-only connector credential boundary.
//!
//! Profile metadata is event-sourced; OAuth refresh tokens, database DSNs and
//! browser cookies are stored in the daemon credential store instead. These
//! handlers never echo a secret and never construct a command envelope.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::{
    Json, Router,
    routing::{post, put},
};
use frank_credential::{CredentialStore, NativeCredentialStore};
use frank_orchestrator::{
    BrowserPolicy, GOOGLE_WORKSPACE_SCOPES, GoogleOAuthClient, GooglePkceChallenge,
    validate_postgres_profile_config, validate_sqlite_path,
};
use frank_protocol::{
    ApiError, ConnectorHealth, ConnectorKind, ConnectorProfileId, ConnectorProfileView, ErrorCode,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::Mutex;

use crate::auth::authenticate;
use crate::{ServerState, api_error_response};

const MAX_SECRET_BYTES: usize = 256 * 1024;
const GOOGLE_OAUTH_TTL_SECS: u64 = 10 * 60;

/// One-time in-memory OAuth state.  The verifier and nonce never enter the
/// event log, snapshot, or audit export; they disappear after callback (or
/// expiry) and are bound to one connector profile.
#[derive(Debug, Clone)]
pub struct GoogleOAuthPending {
    pub challenge: GooglePkceChallenge,
    pub client_id: String,
    pub redirect_uri: String,
    pub expires_at: u64,
}

pub type GoogleOAuthPendingStore = Arc<Mutex<HashMap<ConnectorProfileId, GoogleOAuthPending>>>;

#[derive(Clone)]
pub struct ConnectorCredentialStore {
    native: NativeCredentialStore,
}

impl std::fmt::Debug for ConnectorCredentialStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConnectorCredentialStore")
            .finish_non_exhaustive()
    }
}

impl ConnectorCredentialStore {
    pub fn new(root: PathBuf) -> Self {
        Self {
            native: NativeCredentialStore::daemon(root),
        }
    }

    fn reference(profile_id: ConnectorProfileId) -> String {
        format!("connector:{}", profile_id)
    }

    #[allow(clippy::result_large_err)]
    pub fn save(
        &self,
        profile_id: ConnectorProfileId,
        secret: &str,
    ) -> frank_credential::Result<()> {
        self.native.save(&Self::reference(profile_id), secret)
    }

    #[allow(clippy::result_large_err)]
    pub fn delete(&self, profile_id: ConnectorProfileId) -> frank_credential::Result<()> {
        self.native.delete(&Self::reference(profile_id))
    }

    pub fn configured(&self, profile_id: ConnectorProfileId) -> bool {
        self.native.source(&Self::reference(profile_id)).is_some()
    }

    #[allow(clippy::result_large_err)]
    pub fn load(&self, profile_id: ConnectorProfileId) -> frank_credential::Result<Option<String>> {
        self.native.load(&Self::reference(profile_id))
    }
}

#[async_trait::async_trait]
impl frank_orchestrator::ConnectorSecretResolver for ConnectorCredentialStore {
    async fn secret(
        &self,
        profile_id: ConnectorProfileId,
    ) -> Result<Option<String>, frank_orchestrator::ConnectorError> {
        self.load(profile_id)
            .map_err(|error| frank_orchestrator::ConnectorError::Invalid(error.to_string()))
    }
}

#[derive(Debug, Deserialize)]
struct SecretRequest {
    secret: String,
}

#[derive(Debug, Deserialize)]
struct GoogleOAuthCallbackRequest {
    code: String,
    state: String,
    nonce: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GoogleOAuthCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    nonce: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct GoogleOAuthStartResponse {
    authorization_url: String,
    expires_at: u64,
}

#[derive(Debug, Serialize)]
struct GoogleOAuthStatusResponse {
    configured: bool,
    health: ConnectorHealth,
}

pub fn router() -> Router<ServerState> {
    Router::new()
        .route(
            &crate::api_path("/connectors/{profile_id}/credential"),
            put(save).delete(remove),
        )
        .route(
            &crate::api_path("/connectors/{profile_id}/test"),
            post(test_connection),
        )
        .route(
            &crate::api_path("/connectors/{profile_id}/google/oauth/start"),
            post(google_oauth_start),
        )
        .route(
            &crate::api_path("/connectors/{profile_id}/google/oauth/reconnect"),
            post(google_oauth_start),
        )
        .route(
            &crate::api_path("/connectors/{profile_id}/google/oauth/callback"),
            post(google_oauth_callback).get(google_oauth_callback_get),
        )
        .route(
            &crate::api_path("/connectors/{profile_id}/google/oauth/refresh"),
            post(google_oauth_refresh),
        )
        .route(
            &crate::api_path("/connectors/{profile_id}/google/oauth/revoke"),
            post(google_oauth_revoke),
        )
}

async fn google_oauth_callback_get(
    State(state): State<ServerState>,
    headers: HeaderMap,
    AxumPath(profile_id): AxumPath<String>,
    Query(query): Query<GoogleOAuthCallbackQuery>,
) -> axum::response::Response {
    if let Some(error) = query.error {
        return google_error(
            StatusCode::BAD_REQUEST,
            format!("Google OAuth was denied: {error}"),
        );
    }
    let (Some(code), Some(state_value)) = (query.code, query.state) else {
        return google_error(
            StatusCode::BAD_REQUEST,
            "Google OAuth callback is missing code or state",
        );
    };
    google_oauth_callback(
        State(state),
        headers,
        AxumPath(profile_id),
        Json(GoogleOAuthCallbackRequest {
            code,
            state: state_value,
            nonce: query.nonce,
        }),
    )
    .await
    .into_response()
}

async fn google_profile(
    state: &ServerState,
    profile_id: ConnectorProfileId,
) -> std::result::Result<ConnectorProfileView, String> {
    let snapshot = state
        .store
        .snapshot()
        .await
        .map_err(|_| "snapshot unavailable".to_string())?;
    let profile = snapshot
        .organization
        .connector_profiles
        .into_iter()
        .find(|profile| profile.id == profile_id && !profile.archived)
        .ok_or_else(|| "connector profile not found".to_string())?;
    if profile.kind != ConnectorKind::GoogleWorkspace {
        return Err("connector profile is not a Google Workspace profile".into());
    }
    Ok(profile)
}

fn google_client_metadata(
    profile: &ConnectorProfileView,
) -> std::result::Result<(String, String), String> {
    let object = profile
        .config
        .as_object()
        .ok_or_else(|| "Google Workspace profile config is invalid".to_string())?;
    let client_id = object
        .get("client_id")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Google OAuth client_id is missing".to_string())?
        .to_owned();
    let redirect_uri = object
        .get("redirect_uri")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Google OAuth redirect_uri is missing".to_string())?
        .to_owned();
    Ok((client_id, redirect_uri))
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn google_error(status: StatusCode, message: impl Into<String>) -> axum::response::Response {
    api_error_response(status, ApiError::new(ErrorCode::Validation, message.into()))
}

async fn require_google_owner(
    state: &ServerState,
    headers: &HeaderMap,
    profile_id: &str,
) -> std::result::Result<ConnectorProfileId, axum::response::Response> {
    let Some(auth) = authenticate(state, headers).await else {
        return Err(api_error_response(
            StatusCode::UNAUTHORIZED,
            ApiError::new(ErrorCode::Unauthorized, "device authentication required"),
        ));
    };
    if !auth.role.can_admin() {
        return Err(api_error_response(
            StatusCode::FORBIDDEN,
            ApiError::new(ErrorCode::Forbidden, "owner role required"),
        ));
    }
    ConnectorProfileId::parse(profile_id).map_err(|_| {
        api_error_response(
            StatusCode::BAD_REQUEST,
            ApiError::new(ErrorCode::Validation, "invalid connector profile id"),
        )
    })
}

async fn google_oauth_start(
    State(state): State<ServerState>,
    headers: HeaderMap,
    AxumPath(profile_id): AxumPath<String>,
) -> impl IntoResponse {
    let profile_id = match require_google_owner(&state, &headers, &profile_id).await {
        Ok(profile_id) => profile_id,
        Err(response) => return response,
    };
    let profile = match google_profile(&state, profile_id).await {
        Ok(profile) => profile,
        Err(error) => return google_error(StatusCode::NOT_FOUND, error),
    };
    let (client_id, redirect_uri) = match google_client_metadata(&profile) {
        Ok(metadata) => metadata,
        Err(error) => return google_error(StatusCode::BAD_REQUEST, error),
    };
    let challenge = match GooglePkceChallenge::generate() {
        Ok(challenge) => challenge,
        Err(error) => return google_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };
    let authorization_url =
        match challenge.authorization_url(&client_id, &redirect_uri, GOOGLE_WORKSPACE_SCOPES) {
            Ok(url) => url.to_string(),
            Err(error) => return google_error(StatusCode::BAD_REQUEST, error.to_string()),
        };
    let expires_at = now_unix().saturating_add(GOOGLE_OAUTH_TTL_SECS);
    let pending = GoogleOAuthPending {
        challenge,
        client_id,
        redirect_uri,
        expires_at,
    };
    let mut flows = state.google_oauth_pending.lock().await;
    flows.retain(|_, value| value.expires_at > now_unix());
    flows.insert(profile_id, pending);
    (
        StatusCode::OK,
        Json(GoogleOAuthStartResponse {
            authorization_url,
            expires_at,
        }),
    )
        .into_response()
}

async fn google_oauth_callback(
    State(state): State<ServerState>,
    _headers: HeaderMap,
    AxumPath(profile_id): AxumPath<String>,
    Json(request): Json<GoogleOAuthCallbackRequest>,
) -> impl IntoResponse {
    // The provider redirects a browser without Frank's bearer header.  The
    // one-time state stored by an owner-authenticated start request is the
    // callback proof; when an OIDC nonce is returned we bind that too.
    // Refresh/revoke and every other credential mutation still require the
    // owner header.
    let profile_id = match ConnectorProfileId::parse(&profile_id) {
        Ok(profile_id) => profile_id,
        Err(_) => {
            return google_error(StatusCode::BAD_REQUEST, "invalid connector profile id");
        }
    };
    if let Err(error) = google_profile(&state, profile_id).await {
        return google_error(StatusCode::NOT_FOUND, error);
    }
    if request.code.trim().is_empty() {
        return google_error(StatusCode::BAD_REQUEST, "Google OAuth code is required");
    }
    let pending = state.google_oauth_pending.lock().await.remove(&profile_id);
    let Some(pending) = pending else {
        return google_error(
            StatusCode::BAD_REQUEST,
            "Google OAuth flow is missing or has expired",
        );
    };
    if pending.expires_at <= now_unix() {
        return google_error(StatusCode::BAD_REQUEST, "Google OAuth flow has expired");
    }
    let callback_check = match request.nonce.as_deref() {
        Some(nonce) => pending
            .challenge
            .validate_callback(&request.state, Some(nonce)),
        // Google code-only redirects normally return state but not nonce. The
        // state is still a high-entropy one-time binder; verify nonce whenever
        // an OIDC-capable client sends it.
        None => pending.challenge.validate_state(&request.state),
    };
    if let Err(error) = callback_check {
        return google_error(StatusCode::BAD_REQUEST, error.to_string());
    }
    let token_response = match GoogleOAuthClient::default()
        .exchange_code(
            &pending.client_id,
            None,
            &request.code,
            &pending.challenge.verifier,
            &pending.redirect_uri,
        )
        .await
    {
        Ok(response) => response,
        Err(error) => return google_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let bundle = match token_bundle(&token_response, None, true) {
        Ok(bundle) => bundle,
        Err(error) => return google_error(StatusCode::BAD_GATEWAY, error),
    };
    if let Err(error) = state.connector_credentials.save(profile_id, &bundle) {
        return google_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string());
    }
    (
        StatusCode::OK,
        Json(GoogleOAuthStatusResponse {
            configured: true,
            health: ConnectorHealth::Healthy,
        }),
    )
        .into_response()
}

async fn google_oauth_refresh(
    State(state): State<ServerState>,
    headers: HeaderMap,
    AxumPath(profile_id): AxumPath<String>,
) -> impl IntoResponse {
    let profile_id = match require_google_owner(&state, &headers, &profile_id).await {
        Ok(profile_id) => profile_id,
        Err(response) => return response,
    };
    let profile = match google_profile(&state, profile_id).await {
        Ok(profile) => profile,
        Err(error) => return google_error(StatusCode::NOT_FOUND, error),
    };
    let (client_id, _) = match google_client_metadata(&profile) {
        Ok(metadata) => metadata,
        Err(error) => return google_error(StatusCode::BAD_REQUEST, error),
    };
    let existing = match state.connector_credentials.load(profile_id) {
        Ok(Some(value)) => value,
        Ok(None) => {
            return google_error(StatusCode::NOT_FOUND, "Google credential is not configured");
        }
        Err(error) => return google_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };
    let refresh_token = serde_json::from_str::<serde_json::Value>(&existing)
        .ok()
        .and_then(|value| {
            value
                .get("refresh_token")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .filter(|value| !value.trim().is_empty());
    let Some(refresh_token) = refresh_token else {
        return google_error(StatusCode::BAD_REQUEST, "Google refresh token is missing");
    };
    let token_response = match GoogleOAuthClient::default()
        .refresh(&client_id, None, &refresh_token)
        .await
    {
        Ok(response) => response,
        Err(error) => return google_error(StatusCode::BAD_GATEWAY, error.to_string()),
    };
    let bundle = match token_bundle(&token_response, Some(&refresh_token), false) {
        Ok(bundle) => bundle,
        Err(error) => return google_error(StatusCode::BAD_GATEWAY, error),
    };
    if let Err(error) = state.connector_credentials.save(profile_id, &bundle) {
        return google_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string());
    }
    (
        StatusCode::OK,
        Json(GoogleOAuthStatusResponse {
            configured: true,
            health: ConnectorHealth::Healthy,
        }),
    )
        .into_response()
}

async fn google_oauth_revoke(
    State(state): State<ServerState>,
    headers: HeaderMap,
    AxumPath(profile_id): AxumPath<String>,
) -> impl IntoResponse {
    let profile_id = match require_google_owner(&state, &headers, &profile_id).await {
        Ok(profile_id) => profile_id,
        Err(response) => return response,
    };
    if let Err(error) = google_profile(&state, profile_id).await {
        return google_error(StatusCode::NOT_FOUND, error);
    }
    let existing = match state.connector_credentials.load(profile_id) {
        Ok(Some(value)) => value,
        Ok(None) => {
            return (
                StatusCode::OK,
                Json(GoogleOAuthStatusResponse {
                    configured: false,
                    health: ConnectorHealth::Unknown,
                }),
            )
                .into_response();
        }
        Err(error) => return google_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    };
    let token = serde_json::from_str::<serde_json::Value>(&existing)
        .ok()
        .and_then(|value| {
            value
                .get("refresh_token")
                .or_else(|| value.get("access_token"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .filter(|value| !value.trim().is_empty());
    if let Some(token) = token
        && let Err(error) = GoogleOAuthClient::default().revoke(&token).await
    {
        return google_error(StatusCode::BAD_GATEWAY, error.to_string());
    }
    if let Err(error) = state.connector_credentials.delete(profile_id) {
        return google_error(StatusCode::INTERNAL_SERVER_ERROR, error.to_string());
    }
    (
        StatusCode::OK,
        Json(GoogleOAuthStatusResponse {
            configured: false,
            health: ConnectorHealth::Unknown,
        }),
    )
        .into_response()
}

fn token_bundle(
    response: &serde_json::Value,
    fallback_refresh_token: Option<&str>,
    require_refresh_token: bool,
) -> std::result::Result<String, String> {
    let access_token = response
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Google OAuth response did not contain an access token".to_string())?;
    let refresh_token = response
        .get("refresh_token")
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .or(fallback_refresh_token.filter(|value| !value.trim().is_empty()));
    if require_refresh_token && refresh_token.is_none() {
        return Err("Google OAuth response did not contain a refresh token".into());
    }
    let mut bundle = serde_json::Map::new();
    bundle.insert("access_token".into(), json!(access_token));
    if let Some(refresh_token) = refresh_token {
        bundle.insert("refresh_token".into(), json!(refresh_token));
    }
    if let Some(expires_in) = response
        .get("expires_in")
        .and_then(serde_json::Value::as_u64)
    {
        bundle.insert("expires_in".into(), json!(expires_in));
        bundle.insert(
            "expires_at".into(),
            json!(now_unix().saturating_add(expires_in)),
        );
    }
    if let Some(token_type) = response
        .get("token_type")
        .and_then(serde_json::Value::as_str)
    {
        bundle.insert("token_type".into(), json!(token_type));
    }
    bundle.insert("issued_at".into(), json!(now_unix()));
    serde_json::to_string(&bundle).map_err(|error| error.to_string())
}

/// Run a non-mutating profile/credential boundary check. Provider calls stay
/// in the daemon adapter; this endpoint intentionally returns only a health
/// enum and a short diagnostic, never the credential or a DSN.
async fn test_connection(
    State(state): State<ServerState>,
    headers: HeaderMap,
    AxumPath(profile_id): AxumPath<String>,
) -> impl IntoResponse {
    let Some(auth) = authenticate(&state, &headers).await else {
        return api_error_response(
            StatusCode::UNAUTHORIZED,
            ApiError::new(ErrorCode::Unauthorized, "device authentication required"),
        )
        .into_response();
    };
    if !auth.role.can_admin() {
        return api_error_response(
            StatusCode::FORBIDDEN,
            ApiError::new(ErrorCode::Forbidden, "owner role required"),
        )
        .into_response();
    }
    let Ok(profile_id) = ConnectorProfileId::parse(&profile_id) else {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            ApiError::new(ErrorCode::Validation, "invalid connector profile id"),
        )
        .into_response();
    };
    let Ok(snapshot) = state.store.snapshot().await else {
        return api_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::new(ErrorCode::Internal, "snapshot unavailable"),
        )
        .into_response();
    };
    let Some(profile) = snapshot
        .organization
        .connector_profiles
        .into_iter()
        .find(|profile| profile.id == profile_id && !profile.archived)
    else {
        return api_error_response(
            StatusCode::NOT_FOUND,
            ApiError::new(ErrorCode::NotFound, "connector profile not found"),
        )
        .into_response();
    };

    let (configured, health, diagnostic) = match profile.kind {
        ConnectorKind::Taskboard | ConnectorKind::Terminal => {
            (true, ConnectorHealth::Healthy, None)
        }
        ConnectorKind::Browser => {
            let result = profile
                .config
                .as_object()
                .ok_or_else(|| "browser connector profile config is invalid".to_string())
                .and_then(|config| {
                    let domains = config
                        .get("allowed_domains")
                        .and_then(serde_json::Value::as_array)
                        .ok_or_else(|| "browser domain allowlist is missing".to_string())?;
                    if domains.is_empty() {
                        return Err("browser domain allowlist is empty".into());
                    }
                    let allowed_domains = domains
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<_>>();
                    if allowed_domains.len() != domains.len() {
                        return Err("browser domain allowlist contains an invalid host".into());
                    }
                    let policy = BrowserPolicy {
                        allowed_domains,
                        max_response_bytes: config
                            .get("max_response_bytes")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(BrowserPolicy::default().max_response_bytes as u64)
                            as usize,
                        max_download_bytes: config
                            .get("max_download_bytes")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(BrowserPolicy::default().max_download_bytes as u64)
                            as usize,
                    };
                    (policy.max_response_bytes > 0 && policy.max_download_bytes > 0)
                        .then_some(())
                        .ok_or_else(|| "browser payload limits must be positive".to_string())
                });
            match result {
                Ok(()) => (true, ConnectorHealth::Healthy, None),
                Err(error) => (false, ConnectorHealth::Unhealthy, Some(error)),
            }
        }
        ConnectorKind::Sqlite => {
            let path = profile
                .config
                .get("path")
                .or_else(|| profile.config.get("database_path"))
                .and_then(serde_json::Value::as_str);
            let result = path
                .ok_or_else(|| "SQLite path is not configured".to_string())
                .and_then(|path| {
                    validate_sqlite_path(
                        path,
                        &snapshot
                            .server
                            .allowed_project_roots
                            .iter()
                            .map(std::path::PathBuf::from)
                            .collect::<Vec<_>>(),
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
                });
            match result {
                Ok(()) => (true, ConnectorHealth::Healthy, None),
                Err(error) => (false, ConnectorHealth::Unhealthy, Some(error)),
            }
        }
        ConnectorKind::Postgres => {
            let metadata = validate_postgres_profile_config(&profile.config)
                .map_err(|error| error.to_string());
            let secret = state.connector_credentials.load(profile.id).ok().flatten();
            let result = metadata.and_then(|_| {
                let secret = secret
                    .filter(|value| !value.trim().is_empty())
                    .ok_or_else(|| "PostgreSQL credential is not configured".to_string())?;
                let dsn = serde_json::from_str::<serde_json::Value>(&secret)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("dsn")
                            .and_then(serde_json::Value::as_str)
                            .map(str::to_owned)
                    })
                    .unwrap_or(secret);
                (dsn.starts_with("postgres://") || dsn.starts_with("postgresql://"))
                    .then_some(())
                    .ok_or_else(|| "PostgreSQL credential must be a postgres DSN".to_string())
            });
            match result {
                Ok(()) => (true, ConnectorHealth::Healthy, None),
                Err(error) => (false, ConnectorHealth::Unhealthy, Some(error)),
            }
        }
        ConnectorKind::GoogleWorkspace => {
            let configured = state
                .connector_credentials
                .load(profile.id)
                .ok()
                .flatten()
                .is_some_and(|value| !value.trim().is_empty());
            if configured {
                (true, ConnectorHealth::Healthy, None)
            } else {
                (
                    false,
                    ConnectorHealth::Unknown,
                    Some("Google Workspace credential is not configured".into()),
                )
            }
        }
    };
    (
        StatusCode::OK,
        Json(json!({
            "profile_id": profile_id.to_string(),
            "configured": configured,
            "health": health,
            "diagnostic": diagnostic,
        })),
    )
        .into_response()
}

async fn save(
    State(state): State<ServerState>,
    headers: HeaderMap,
    AxumPath(profile_id): AxumPath<String>,
    Json(payload): Json<SecretRequest>,
) -> impl IntoResponse {
    let Some(auth) = authenticate(&state, &headers).await else {
        return api_error_response(
            StatusCode::UNAUTHORIZED,
            ApiError::new(ErrorCode::Unauthorized, "device authentication required"),
        )
        .into_response();
    };
    if !auth.role.can_admin() {
        return api_error_response(
            StatusCode::FORBIDDEN,
            ApiError::new(ErrorCode::Forbidden, "owner role required"),
        )
        .into_response();
    }
    let Ok(profile_id) = ConnectorProfileId::parse(&profile_id) else {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            ApiError::new(ErrorCode::Validation, "invalid connector profile id"),
        )
        .into_response();
    };
    let profile = match state.store.snapshot().await {
        Ok(snapshot) => snapshot
            .organization
            .connector_profiles
            .into_iter()
            .find(|profile| profile.id == profile_id && !profile.archived),
        Err(_) => None,
    };
    let Some(profile) = profile else {
        return api_error_response(
            StatusCode::NOT_FOUND,
            ApiError::new(ErrorCode::NotFound, "connector profile not found"),
        )
        .into_response();
    };
    if profile.kind == ConnectorKind::Taskboard {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            ApiError::new(
                ErrorCode::Validation,
                "Taskboard is internal and does not accept external credentials",
            ),
        )
        .into_response();
    }
    let secret = payload.secret.trim();
    if secret.is_empty() || secret.len() > MAX_SECRET_BYTES {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            ApiError::new(
                ErrorCode::Validation,
                "connector credential is empty or too large",
            ),
        )
        .into_response();
    }
    match state.connector_credentials.save(profile_id, secret) {
        Ok(()) => (StatusCode::OK, Json(json!({"configured": true}))).into_response(),
        Err(_error) => api_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::new(
                ErrorCode::Internal,
                "connector credential could not be stored",
            ),
        )
        .into_response(),
    }
}

async fn remove(
    State(state): State<ServerState>,
    headers: HeaderMap,
    AxumPath(profile_id): AxumPath<String>,
) -> impl IntoResponse {
    let Some(auth) = authenticate(&state, &headers).await else {
        return api_error_response(
            StatusCode::UNAUTHORIZED,
            ApiError::new(ErrorCode::Unauthorized, "device authentication required"),
        )
        .into_response();
    };
    if !auth.role.can_admin() {
        return api_error_response(
            StatusCode::FORBIDDEN,
            ApiError::new(ErrorCode::Forbidden, "owner role required"),
        )
        .into_response();
    }
    let Ok(profile_id) = ConnectorProfileId::parse(&profile_id) else {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            ApiError::new(ErrorCode::Validation, "invalid connector profile id"),
        )
        .into_response();
    };
    let profile_exists = state.store.snapshot().await.ok().is_some_and(|snapshot| {
        snapshot
            .organization
            .connector_profiles
            .iter()
            .any(|profile| profile.id == profile_id && !profile.archived)
    });
    if !profile_exists {
        return api_error_response(
            StatusCode::NOT_FOUND,
            ApiError::new(ErrorCode::NotFound, "connector profile not found"),
        )
        .into_response();
    }
    match state.connector_credentials.delete(profile_id) {
        Ok(()) => (StatusCode::OK, Json(json!({"configured": false}))).into_response(),
        Err(_) => api_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::new(
                ErrorCode::Internal,
                "connector credential could not be removed",
            ),
        )
        .into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_token_bundle_rotates_access_token_without_leaking_raw_response() {
        let bundle = token_bundle(
            &json!({
                "access_token": "access-1",
                "expires_in": 3600,
                "token_type": "Bearer",
                "unexpected": "must-not-be-persisted"
            }),
            Some("refresh-1"),
            false,
        )
        .expect("token bundle should be valid");
        let value: serde_json::Value = serde_json::from_str(&bundle).unwrap();
        assert_eq!(value["access_token"], "access-1");
        assert_eq!(value["refresh_token"], "refresh-1");
        assert!(value.get("client_secret").is_none());
        assert!(value.get("unexpected").is_none());
    }

    #[test]
    fn google_initial_exchange_requires_refresh_token() {
        assert!(token_bundle(&json!({"access_token": "access-1"}), None, true).is_err());
    }
}
