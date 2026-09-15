//! Owner-only OpenRouter credential and model-catalog routes.
//!
//! Provider credentials intentionally do not use the event-sourced command
//! endpoint.  A command envelope and its response are durable, so accepting a
//! key there would make accidental secret persistence very easy.

use std::path::PathBuf;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::{
    Json, Router,
    routing::{get, post, put},
};
use frank_agent::{
    CredentialResolver, EnvironmentCredentialResolver, EnvironmentOpenAiCredentialResolver,
};
use frank_credential::{CredentialStore, NativeCredentialStore};
use frank_protocol::ModelDescriptor;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::auth::authenticate;
use crate::{ServerState, api_error_response};

const OPENROUTER_REFERENCE: &str = "openrouter";
const OPENAI_REFERENCE: &str = "openai";
const MAX_PROVIDER_KEY_BYTES: usize = 8 * 1024;

#[derive(Clone)]
pub struct OpenRouterCredentialStore {
    native: NativeCredentialStore,
}

impl std::fmt::Debug for OpenRouterCredentialStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenRouterCredentialStore")
            .finish_non_exhaustive()
    }
}

impl OpenRouterCredentialStore {
    pub fn new(root: PathBuf) -> Self {
        Self {
            native: NativeCredentialStore::daemon(root),
        }
    }

    #[allow(clippy::result_large_err)]
    fn persisted(&self) -> frank_credential::Result<Option<String>> {
        self.native.load(OPENROUTER_REFERENCE)
    }

    #[allow(clippy::result_large_err)]
    pub fn save(&self, key: &str) -> frank_credential::Result<()> {
        self.native.save(OPENROUTER_REFERENCE, key)
    }

    #[allow(clippy::result_large_err)]
    pub fn delete(&self) -> frank_credential::Result<()> {
        self.native.delete(OPENROUTER_REFERENCE)
    }

    pub fn source(&self) -> Option<String> {
        if let Some(source) = self.native.source(OPENROUTER_REFERENCE) {
            Some(source.into())
        } else if std::env::var("OPENROUTER_API_KEY")
            .ok()
            .is_some_and(|value| !value.trim().is_empty())
        {
            Some("environment".into())
        } else {
            None
        }
    }
}

#[async_trait::async_trait]
impl CredentialResolver for OpenRouterCredentialStore {
    async fn api_key(&self) -> frank_agent::Result<Option<String>> {
        if let Some(value) = self
            .persisted()
            .map_err(|error| frank_agent::ProviderError::Process(error.to_string()))?
            .filter(|value| !value.trim().is_empty())
        {
            return Ok(Some(value));
        }
        EnvironmentCredentialResolver.api_key().await
    }

    fn credential_source(&self) -> Option<String> {
        self.source()
    }
}

#[derive(Clone)]
pub struct OpenAiCredentialStore {
    native: NativeCredentialStore,
}

impl std::fmt::Debug for OpenAiCredentialStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OpenAiCredentialStore")
            .finish_non_exhaustive()
    }
}

impl OpenAiCredentialStore {
    pub fn new(root: PathBuf) -> Self {
        Self {
            native: NativeCredentialStore::daemon(root),
        }
    }

    #[allow(clippy::result_large_err)]
    fn persisted(&self) -> frank_credential::Result<Option<String>> {
        self.native.load(OPENAI_REFERENCE)
    }

    #[allow(clippy::result_large_err)]
    pub fn save(&self, key: &str) -> frank_credential::Result<()> {
        self.native.save(OPENAI_REFERENCE, key)
    }

    #[allow(clippy::result_large_err)]
    pub fn delete(&self) -> frank_credential::Result<()> {
        self.native.delete(OPENAI_REFERENCE)
    }

    pub fn source(&self) -> Option<String> {
        if let Some(source) = self.native.source(OPENAI_REFERENCE) {
            Some(source.into())
        } else if std::env::var("OPENAI_API_KEY")
            .ok()
            .is_some_and(|value| !value.trim().is_empty())
        {
            Some("environment".into())
        } else {
            None
        }
    }
}

#[async_trait::async_trait]
impl CredentialResolver for OpenAiCredentialStore {
    async fn api_key(&self) -> frank_agent::Result<Option<String>> {
        if let Some(value) = self
            .persisted()
            .map_err(|error| frank_agent::ProviderError::Process(error.to_string()))?
            .filter(|value| !value.trim().is_empty())
        {
            return Ok(Some(value));
        }
        EnvironmentOpenAiCredentialResolver.api_key().await
    }

    fn credential_source(&self) -> Option<String> {
        self.source()
    }
}

#[derive(Debug, Deserialize)]
pub struct CredentialRequest {
    pub api_key: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct ModelQuery {
    #[serde(default)]
    pub refresh: bool,
}

#[derive(Debug, Serialize)]
pub struct ModelResponse {
    pub models: Vec<ModelDescriptor>,
    pub refreshed_at: String,
    pub stale: bool,
}

pub fn router() -> Router<ServerState> {
    Router::new()
        .route(&crate::api_path("/providers/openrouter"), get(status))
        .route(
            &crate::api_path("/providers/openrouter/credential"),
            put(save_credential).delete(delete_credential),
        )
        .route(
            &crate::api_path("/providers/openrouter/test"),
            post(test_connection),
        )
        .route(
            &crate::api_path("/providers/openrouter/models"),
            get(models),
        )
        .route(
            &crate::api_path("/providers/openrouter/models/refresh"),
            post(refresh_models),
        )
        .route(&crate::api_path("/providers/openai"), get(openai_status))
        .route(
            &crate::api_path("/providers/openai/credential"),
            put(save_openai_credential).delete(delete_openai_credential),
        )
        .route(
            &crate::api_path("/providers/openai/test"),
            post(test_openai_connection),
        )
        .route(
            &crate::api_path("/providers/openai/models"),
            get(openai_models),
        )
        .route(
            &crate::api_path("/providers/openai/models/refresh"),
            post(refresh_openai_models),
        )
}

pub(crate) async fn status(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(response) = require_owner(&state, &headers).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(state.openrouter_adapter.connection().await),
    )
        .into_response()
}

async fn save_credential(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(payload): Json<CredentialRequest>,
) -> impl IntoResponse {
    if let Err(response) = require_owner(&state, &headers).await {
        return response;
    }
    let key = payload.api_key.trim();
    if key.is_empty() || key.len() > MAX_PROVIDER_KEY_BYTES {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Validation,
                "OpenRouter API key is empty or too large",
            ),
        )
        .into_response();
    }
    match state.openrouter_credentials.save(key) {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({
                "saved": true,
                "source": state.openrouter_credentials.source(),
            })),
        )
            .into_response(),
        Err(error) => api_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Internal,
                sanitize(&error.to_string()),
            ),
        )
        .into_response(),
    }
}

async fn delete_credential(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(response) = require_owner(&state, &headers).await {
        return response;
    }
    match state.openrouter_credentials.delete() {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({
                "deleted": true,
                "source": state.openrouter_credentials.source(),
            })),
        )
            .into_response(),
        Err(error) => api_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Internal,
                sanitize(&error.to_string()),
            ),
        )
        .into_response(),
    }
}

async fn test_connection(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(response) = require_owner(&state, &headers).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(state.openrouter_adapter.connection().await),
    )
        .into_response()
}

async fn models(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Query(query): Query<ModelQuery>,
) -> impl IntoResponse {
    model_response(state, headers, query.refresh).await
}

async fn refresh_models(State(state): State<ServerState>, headers: HeaderMap) -> impl IntoResponse {
    model_response(state, headers, true).await
}

async fn openai_status(State(state): State<ServerState>, headers: HeaderMap) -> impl IntoResponse {
    if let Err(response) = require_owner(&state, &headers).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(state.openai_adapter.connection().await),
    )
        .into_response()
}

async fn save_openai_credential(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(payload): Json<CredentialRequest>,
) -> impl IntoResponse {
    if let Err(response) = require_owner(&state, &headers).await {
        return response;
    }
    let key = payload.api_key.trim();
    if key.is_empty() || key.len() > MAX_PROVIDER_KEY_BYTES {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Validation,
                "OpenAI API key is empty or too large",
            ),
        )
        .into_response();
    }
    match state.openai_credentials.save(key) {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({
                "saved": true,
                "source": state.openai_credentials.source(),
            })),
        )
            .into_response(),
        Err(error) => api_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Internal,
                sanitize(&error.to_string()),
            ),
        )
        .into_response(),
    }
}

async fn delete_openai_credential(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(response) = require_owner(&state, &headers).await {
        return response;
    }
    match state.openai_credentials.delete() {
        Ok(()) => (
            StatusCode::OK,
            Json(json!({
                "deleted": true,
                "source": state.openai_credentials.source(),
            })),
        )
            .into_response(),
        Err(error) => api_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Internal,
                sanitize(&error.to_string()),
            ),
        )
        .into_response(),
    }
}

async fn test_openai_connection(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if let Err(response) = require_owner(&state, &headers).await {
        return response;
    }
    (
        StatusCode::OK,
        Json(state.openai_adapter.connection().await),
    )
        .into_response()
}

async fn openai_models(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Query(query): Query<ModelQuery>,
) -> impl IntoResponse {
    openai_model_response(state, headers, query.refresh).await
}

async fn refresh_openai_models(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    openai_model_response(state, headers, true).await
}

async fn openai_model_response(
    state: ServerState,
    headers: HeaderMap,
    refresh: bool,
) -> axum::response::Response {
    if let Err(response) = require_owner(&state, &headers).await {
        return response;
    }
    match state.openai_adapter.models_with_status(refresh).await {
        Ok((models, refreshed_at, stale)) => (
            StatusCode::OK,
            Json(ModelResponse {
                models,
                refreshed_at: refreshed_at
                    .unwrap_or_else(frank_protocol::timestamp_now)
                    .to_string(),
                stale,
            }),
        )
            .into_response(),
        Err(error) => crate::api_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::ProviderUnavailable,
                sanitize(&error.to_string()),
            ),
        )
        .into_response(),
    }
}

async fn model_response(
    state: ServerState,
    headers: HeaderMap,
    refresh: bool,
) -> axum::response::Response {
    if let Err(response) = require_owner(&state, &headers).await {
        return response;
    }
    match state.openrouter_adapter.models_with_status(refresh).await {
        Ok((models, refreshed_at, stale)) => (
            StatusCode::OK,
            Json(ModelResponse {
                models,
                refreshed_at: refreshed_at.unwrap_or_else(frank_protocol::timestamp_now),
                stale,
            }),
        )
            .into_response(),
        Err(error) => api_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::ProviderUnavailable,
                sanitize(&error.to_string()),
            ),
        )
        .into_response(),
    }
}

async fn require_owner(
    state: &ServerState,
    headers: &HeaderMap,
) -> Result<(), axum::response::Response> {
    let Some(auth) = authenticate(state, headers).await else {
        return Err(api_error_response(
            StatusCode::UNAUTHORIZED,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Unauthorized,
                "device authentication required",
            ),
        )
        .into_response());
    };
    if !auth.role.can_admin() {
        return Err(api_error_response(
            StatusCode::FORBIDDEN,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Forbidden,
                "owner role required",
            ),
        )
        .into_response());
    }
    Ok(())
}

fn sanitize(value: &str) -> String {
    value
        .replace("OPENROUTER_API_KEY", "provider credential")
        .replace("OPENAI_API_KEY", "provider credential")
        .chars()
        .take(512)
        .collect()
}
