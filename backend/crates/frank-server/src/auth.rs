//! Local owner authentication, bearer sessions, agent capabilities, and the
//! scoping that decides how much of a snapshot a provider session may see.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use axum::Json;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use thiserror::Error;
use tokio::sync::{Mutex, Semaphore};

use crate::*;

const SESSION_TTL_SECONDS: u64 = 30 * 24 * 60 * 60;
const RATE_WINDOW_SECONDS: u64 = 60;
const PER_IP_ATTEMPT_LIMIT: u32 = 10;
const GLOBAL_ATTEMPT_LIMIT: u32 = 100;
const HASH_CONCURRENCY: usize = 2;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("store error: {0}")]
    Store(#[from] frank_store::StoreError),
    #[error("username must be 3-32 ASCII characters using letters, numbers, '.', '_' or '-'")]
    InvalidUsername,
    #[error("password must be between 15 and 128 characters")]
    InvalidPassword,
    #[error("invalid username or password")]
    InvalidCredentials,
    #[error("local owner account has not been initialized")]
    NotConfigured,
    #[error("local owner account is already initialized")]
    AlreadyConfigured,
    #[error("password operation failed: {0}")]
    Password(String),
    #[error("authentication is temporarily rate limited")]
    RateLimited { retry_after: u64 },
    #[error("authentication service is unavailable")]
    Internal,
}

type AuthResult<T> = std::result::Result<T, AuthError>;

#[derive(Debug, Clone, Copy, Default)]
struct RateWindow {
    started_at: u64,
    attempts: u32,
}

#[derive(Debug, Default)]
struct RateState {
    by_ip: HashMap<String, RateWindow>,
    global: RateWindow,
}

#[derive(Clone)]
pub struct AuthManager {
    store: frank_store::Store,
    rate: Arc<Mutex<RateState>>,
    hash_slots: Arc<Semaphore>,
}

impl AuthManager {
    pub fn new(store: frank_store::Store) -> Self {
        Self {
            store,
            rate: Arc::new(Mutex::new(RateState::default())),
            hash_slots: Arc::new(Semaphore::new(HASH_CONCURRENCY)),
        }
    }

    pub async fn status(&self, server_id: ServerId) -> AuthResult<AuthStatusResponse> {
        Ok(AuthStatusResponse {
            configured: self.store.owner().await?.is_some(),
            auth_method: "local-password".into(),
            server_id,
        })
    }

    pub async fn initialize_owner(&self, username: &str, password: &str) -> AuthResult<OwnerView> {
        let username = normalize_username(username)?;
        let password_hash = self.hash_password(password).await?;
        let owner_id = UserId::new();
        if !self
            .store
            .create_owner(owner_id, &username, &password_hash, now())
            .await?
        {
            return Err(AuthError::AlreadyConfigured);
        }
        Ok(OwnerView {
            id: owner_id,
            username,
        })
    }

    pub async fn reset_owner_password(&self, password: &str) -> AuthResult<()> {
        let owner = self.store.owner().await?.ok_or(AuthError::NotConfigured)?;
        let password_hash = self.hash_password(password).await?;
        if !self
            .store
            .update_owner_password(owner.owner_id, &password_hash, now())
            .await?
        {
            return Err(AuthError::NotConfigured);
        }
        self.store.revoke_auth_sessions(owner.owner_id).await?;
        Ok(())
    }

    pub async fn hash_password(&self, password: &str) -> AuthResult<String> {
        validate_password(password)?;
        let password = password.to_owned();
        let permit = self
            .hash_slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AuthError::Internal)?;
        let result = tokio::task::spawn_blocking(move || hash_password_sync(&password))
            .await
            .map_err(|_| AuthError::Internal)?;
        drop(permit);
        result
    }

    async fn verify_password(&self, password: &str, encoded: &str) -> AuthResult<bool> {
        let password = password.to_owned();
        let encoded = encoded.to_owned();
        let permit = self
            .hash_slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| AuthError::Internal)?;
        let result = tokio::task::spawn_blocking(move || verify_password_sync(&password, &encoded))
            .await
            .map_err(|_| AuthError::Internal)?;
        drop(permit);
        Ok(result)
    }

    async fn allow_attempt(&self, remote_ip: IpAddr) -> AuthResult<()> {
        let now = now();
        let mut state = self.rate.lock().await;
        let ip = remote_ip.to_string();
        let (ip_started_at, ip_attempts) = {
            let ip_window = state.by_ip.entry(ip.clone()).or_default();
            if now.saturating_sub(ip_window.started_at) >= RATE_WINDOW_SECONDS {
                *ip_window = RateWindow {
                    started_at: now,
                    attempts: 0,
                };
            }
            if ip_window.started_at == 0 {
                ip_window.started_at = now;
            }
            (ip_window.started_at, ip_window.attempts)
        };
        if now.saturating_sub(state.global.started_at) >= RATE_WINDOW_SECONDS {
            state.global = RateWindow {
                started_at: now,
                attempts: 0,
            };
        }
        if state.global.started_at == 0 {
            state.global.started_at = now;
        }
        if ip_attempts >= PER_IP_ATTEMPT_LIMIT || state.global.attempts >= GLOBAL_ATTEMPT_LIMIT {
            let ip_retry = RATE_WINDOW_SECONDS.saturating_sub(now.saturating_sub(ip_started_at));
            let global_retry =
                RATE_WINDOW_SECONDS.saturating_sub(now.saturating_sub(state.global.started_at));
            return Err(AuthError::RateLimited {
                retry_after: ip_retry.max(global_retry).max(1),
            });
        }
        state
            .by_ip
            .get_mut(&ip)
            .expect("rate-limit entry is present")
            .attempts = ip_attempts.saturating_add(1);
        state.global.attempts = state.global.attempts.saturating_add(1);
        // Keep the map bounded in a long-running daemon even when clients
        // rotate source addresses.
        state
            .by_ip
            .retain(|_, window| now.saturating_sub(window.started_at) < RATE_WINDOW_SECONDS * 2);
        Ok(())
    }

    pub async fn login(
        &self,
        request: AuthLoginRequest,
        remote_ip: IpAddr,
        server_id: ServerId,
    ) -> AuthResult<AuthLoginResponse> {
        self.allow_attempt(remote_ip).await?;
        let username = normalize_username(&request.username)?;
        validate_password(&request.password)?;
        let owner = self.store.owner().await?.ok_or(AuthError::NotConfigured)?;
        // Keep the response deliberately generic for both unknown users and
        // wrong passwords. The server has exactly one local owner account.
        // Verify against the stored hash even when the username does not
        // match.  That keeps unknown-user and wrong-password requests on the
        // same expensive Argon2 path as far as the endpoint can observe.
        let password_matches = self
            .verify_password(&request.password, &owner.password_hash)
            .await?;
        if owner.username != username || !password_matches {
            return Err(AuthError::InvalidCredentials);
        }
        let device_name = request
            .device_name
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "Frank desktop".into());
        if device_name.chars().count() > 128 || device_name.chars().any(char::is_control) {
            return Err(AuthError::InvalidCredentials);
        }
        let created_at = now();
        let expires_at = created_at.saturating_add(SESSION_TTL_SECONDS);
        let session_id = SessionId::new();
        let device_id = DeviceId::new();
        let access_token = PairingSecret::generate();
        let token_hash = hex::encode(hash(&access_token));
        let inserted = self
            .store
            .insert_auth_session_if_owner_hash(
                owner.owner_id,
                &owner.password_hash,
                &frank_store::StoredAuthSession {
                    session_id,
                    owner_id: owner.owner_id,
                    device_id,
                    token_hash,
                    created_at,
                    expires_at,
                    last_seen_at: created_at,
                    revoked: false,
                },
            )
            .await?;
        if !inserted {
            return Err(AuthError::InvalidCredentials);
        }
        let session = AuthSessionView {
            session_id,
            device_id,
            expires_at: expires_at.to_string(),
        };
        Ok(AuthLoginResponse {
            access_token,
            expires_at: session.expires_at.clone(),
            server_id,
            device_id,
            owner: OwnerView {
                id: owner.owner_id,
                username: owner.username,
            },
            session,
        })
    }

    pub async fn session(&self, token: &str) -> AuthResult<Option<AuthMeResponse>> {
        let digest = hex::encode(hash(token));
        let Some(session) = self.store.auth_session(&digest, now()).await? else {
            return Ok(None);
        };
        let Some(owner) = self.store.owner().await? else {
            return Ok(None);
        };
        if owner.owner_id != session.owner_id {
            return Ok(None);
        }
        Ok(Some(AuthMeResponse {
            owner: OwnerView {
                id: owner.owner_id,
                username: owner.username,
            },
            session: AuthSessionView {
                session_id: session.session_id,
                device_id: session.device_id,
                expires_at: session.expires_at.to_string(),
            },
        }))
    }

    pub async fn authenticate(&self, token: &str) -> AuthResult<Option<DeviceAuth>> {
        let digest = hex::encode(hash(token));
        let Some(session) = self.store.auth_session(&digest, now()).await? else {
            return Ok(None);
        };
        let Some(owner) = self.store.owner().await? else {
            return Ok(None);
        };
        if owner.owner_id != session.owner_id {
            return Ok(None);
        }
        Ok(Some(DeviceAuth {
            device_id: session.device_id,
            role: DeviceRole::Owner,
            name: owner.username,
        }))
    }

    pub async fn logout(&self, token: &str) -> AuthResult<bool> {
        let digest = hex::encode(hash(token));
        let Some(session) = self.store.auth_session(&digest, now()).await? else {
            return Ok(false);
        };
        Ok(self.store.revoke_auth_session(session.session_id).await?)
    }

    pub async fn logout_all(&self, token: &str) -> AuthResult<bool> {
        let digest = hex::encode(hash(token));
        let Some(session) = self.store.auth_session(&digest, now()).await? else {
            return Ok(false);
        };
        self.store.revoke_auth_sessions(session.owner_id).await?;
        Ok(true)
    }

    pub async fn change_password(
        &self,
        token: &str,
        request: AuthPasswordRequest,
    ) -> AuthResult<bool> {
        let digest = hex::encode(hash(token));
        let Some(session) = self.store.auth_session(&digest, now()).await? else {
            return Err(AuthError::InvalidCredentials);
        };
        let Some(owner) = self.store.owner().await? else {
            return Err(AuthError::NotConfigured);
        };
        if owner.owner_id != session.owner_id
            || !self
                .verify_password(&request.current_password, &owner.password_hash)
                .await?
        {
            return Err(AuthError::InvalidCredentials);
        }
        let new_hash = self.hash_password(&request.new_password).await?;
        if !self
            .store
            .update_owner_password(owner.owner_id, &new_hash, now())
            .await?
        {
            return Err(AuthError::NotConfigured);
        }
        self.store.revoke_auth_sessions(owner.owner_id).await?;
        Ok(true)
    }

    pub async fn revoke_sessions_for_owner(&self) -> AuthResult<u64> {
        let Some(owner) = self.store.owner().await? else {
            return Ok(0);
        };
        Ok(self.store.revoke_auth_sessions(owner.owner_id).await?)
    }
}

pub fn normalize_username(value: &str) -> AuthResult<String> {
    if !(3..=32).contains(&value.len())
        || !value.is_ascii()
        || !value.bytes().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, b'.' | b'_' | b'-')
        })
    {
        return Err(AuthError::InvalidUsername);
    }
    Ok(value.to_ascii_lowercase())
}

pub fn validate_password(value: &str) -> AuthResult<()> {
    let length = value.chars().count();
    if !(15..=128).contains(&length) {
        return Err(AuthError::InvalidPassword);
    }
    Ok(())
}

fn argon2() -> AuthResult<Argon2<'static>> {
    let params = Params::new(19 * 1024, 2, 1, None)
        .map_err(|error| AuthError::Password(error.to_string()))?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

fn hash_password_sync(password: &str) -> AuthResult<String> {
    let mut salt_bytes = [0_u8; 16];
    getrandom::fill(&mut salt_bytes).map_err(|error| AuthError::Password(error.to_string()))?;
    let salt = SaltString::encode_b64(&salt_bytes)
        .map_err(|error| AuthError::Password(error.to_string()))?;
    argon2()?
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| AuthError::Password(error.to_string()))
}

fn verify_password_sync(password: &str, encoded: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(encoded) else {
        return false;
    };
    argon2()
        .map(|argon| argon.verify_password(password.as_bytes(), &parsed).is_ok())
        .unwrap_or(false)
}

fn auth_error_response(error: AuthError) -> axum::response::Response {
    let (status, mut api_error) = match error {
        AuthError::Store(_) | AuthError::Internal | AuthError::Password(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::new(ErrorCode::Internal, "authentication service is unavailable"),
        ),
        AuthError::InvalidUsername | AuthError::InvalidPassword => (
            StatusCode::BAD_REQUEST,
            ApiError::new(
                ErrorCode::Validation,
                "username or password format is invalid",
            ),
        ),
        AuthError::InvalidCredentials => (
            StatusCode::UNAUTHORIZED,
            ApiError::new(ErrorCode::Unauthorized, "invalid username or password"),
        ),
        AuthError::NotConfigured => (
            StatusCode::SERVICE_UNAVAILABLE,
            ApiError::new(
                ErrorCode::Unauthorized,
                "owner account has not been initialized",
            ),
        ),
        AuthError::AlreadyConfigured => (
            StatusCode::CONFLICT,
            ApiError::new(ErrorCode::Conflict, "owner account is already initialized"),
        ),
        AuthError::RateLimited { retry_after } => {
            let mut api_error = ApiError::new(
                ErrorCode::RateLimited,
                "too many login attempts; try again later",
            );
            api_error.retryable = true;
            let mut response = (StatusCode::TOO_MANY_REQUESTS, Json(api_error)).into_response();
            if let Ok(value) = retry_after.to_string().parse() {
                response.headers_mut().insert("retry-after", value);
            }
            return response;
        }
    };
    api_error.retryable = status.is_server_error();
    (status, Json(api_error)).into_response()
}

pub(crate) async fn status(State(state): State<ServerState>) -> impl IntoResponse {
    match state.auth.status(state.store.server_id()).await {
        Ok(status) => (StatusCode::OK, Json(status)).into_response(),
        Err(error) => auth_error_response(error),
    }
}

pub(crate) async fn login(
    State(state): State<ServerState>,
    ConnectInfo(address): ConnectInfo<std::net::SocketAddr>,
    Json(request): Json<AuthLoginRequest>,
) -> impl IntoResponse {
    match state
        .auth
        .login(request, address.ip(), state.store.server_id())
        .await
    {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        // A malformed username/password is still an unsuccessful login. Keep
        // the public response indistinguishable from an unknown account or a
        // wrong password so the endpoint cannot become a username oracle.
        Err(AuthError::InvalidUsername | AuthError::InvalidPassword) => {
            auth_error_response(AuthError::InvalidCredentials)
        }
        Err(error) => auth_error_response(error),
    }
}

pub(crate) async fn me(State(state): State<ServerState>, headers: HeaderMap) -> impl IntoResponse {
    let Some(token) = bearer(&headers) else {
        return auth_error_response(AuthError::InvalidCredentials);
    };
    match state.auth.session(token).await {
        Ok(Some(response)) => (StatusCode::OK, Json(response)).into_response(),
        Ok(None) => auth_error_response(AuthError::InvalidCredentials),
        Err(error) => auth_error_response(error),
    }
}

pub(crate) async fn logout(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let Some(token) = bearer(&headers) else {
        return auth_error_response(AuthError::InvalidCredentials);
    };
    match state.auth.logout(token).await {
        Ok(true) => (StatusCode::OK, Json(AuthMutationResponse { success: true })).into_response(),
        Ok(false) => auth_error_response(AuthError::InvalidCredentials),
        Err(error) => auth_error_response(error),
    }
}

pub(crate) async fn logout_all(
    State(state): State<ServerState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    let Some(token) = bearer(&headers) else {
        return auth_error_response(AuthError::InvalidCredentials);
    };
    match state.auth.logout_all(token).await {
        Ok(true) => (StatusCode::OK, Json(AuthMutationResponse { success: true })).into_response(),
        Ok(false) => auth_error_response(AuthError::InvalidCredentials),
        Err(error) => auth_error_response(error),
    }
}

pub(crate) async fn password(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<AuthPasswordRequest>,
) -> impl IntoResponse {
    let Some(token) = bearer(&headers) else {
        return auth_error_response(AuthError::InvalidCredentials);
    };
    match state.auth.change_password(token, request).await {
        Ok(true) => (StatusCode::OK, Json(AuthMutationResponse { success: true })).into_response(),
        Ok(false) => auth_error_response(AuthError::InvalidCredentials),
        Err(error) => auth_error_response(error),
    }
}

pub(crate) async fn authenticate(state: &ServerState, headers: &HeaderMap) -> Option<DeviceAuth> {
    let token = bearer(headers)?;
    state.auth.authenticate(token).await.ok().flatten()
}

pub(crate) async fn agent_capability_from_headers(
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
pub(crate) async fn scoped_agent_snapshot(
    state: &ServerState,
    agent_id: AgentId,
    task_id: TaskId,
) -> Option<Snapshot> {
    let snapshot = state.store.snapshot().await.ok()?;
    let task = snapshot
        .tasks
        .iter()
        .find(|task| task.id == task_id)?
        .clone();
    let is_assigned_worker = task.assigned_agent == Some(agent_id);
    let is_reviewer = snapshot.review_items.iter().any(|review| {
        review.source_task_id == task_id
            && review.reviewer_agent == agent_id
            && review.status == ReviewWorkItemStatus::Pending
    });
    if !is_assigned_worker && !is_reviewer {
        return None;
    }
    snapshot.scoped_to_task(task_id, agent_id)
}

/// Enforce the second half of agent-session authorization at the transport
/// boundary.  The orchestrator still performs the authoritative state and
/// transition checks, but a short-lived capability must never become a
/// general-purpose operator token merely because the caller also possesses a
/// valid device bearer.  Every provider mutation is therefore tied to the
/// task encoded in the capability and, where applicable, to the agent that is
/// assigned to that task.
pub(crate) async fn agent_command_is_scoped(
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

    // A reviewer is not the source task's worker, so its durable review
    // decision is authorized against the review work item before the normal
    // source-agent ownership check below.
    if let Command::DecideReview { review_item_id, .. } = command {
        return snapshot.review_items.iter().any(|review| {
            review.id == *review_item_id
                && review.status == ReviewWorkItemStatus::Pending
                && review.source_task_id == task_id
                && review.mission_id == mission_id
                && review.reviewer_agent == agent_id
        });
    }
    // Pull-mode offer responses are the one scoped mutation that may arrive
    // before the card is assigned. The durable offer itself is the authority:
    // only its target agent can accept or decline it.
    if let Command::RespondWorkOffer { offer_id, .. } = command {
        return snapshot.work_offers.iter().any(|offer| {
            offer.id == *offer_id
                && offer.task_id == task_id
                && offer.agent_id == agent_id
                && offer.status == WorkOfferStatus::Pending
        });
    }
    if scope_task.assigned_agent != Some(agent_id) {
        return false;
    }

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
            // Role and dependency edges are queue policy, not provider-owned
            // metadata. Keeping them operator-owned prevents a scoped worker
            // from removing the role gate or reshaping the DAG through its
            // otherwise valid task-session capability.
            if patch.required_role_id.is_some() || patch.dependencies.is_some() {
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
        } => {
            *candidate == task_id
                && matches!(
                    status,
                    TaskStatus::Review | TaskStatus::Blocked | TaskStatus::Done
                )
        }
        Command::AssignTask {
            task_id: candidate, ..
        } => snapshot
            .tasks
            .iter()
            .any(|task| task.id == *candidate && task.mission_id == mission_id),
        Command::ClaimTask {
            task_id: candidate,
            agent_id: candidate_agent,
            ..
        } => *candidate == task_id && *candidate_agent == agent_id,
        Command::ReleaseTask { task_id: candidate } => *candidate == task_id,
        Command::TaskAccept { task_id: candidate } => *candidate == task_id,
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
        Command::CreateWorkItem(spec) => {
            spec.mission_id.is_none_or(|mission| mission == mission_id)
                && spec
                    .parent_task_id
                    .is_none_or(|parent| parent == task_id || task_is_owned(parent))
                && spec
                    .dependencies
                    .iter()
                    .all(|dependency| *dependency == task_id || task_is_owned(*dependency))
        }
        Command::DropWorkItem {
            task_id: candidate,
            role_id,
            ..
        } => {
            *candidate == task_id
                && role_id.is_none_or(|role| {
                    snapshot
                        .roles
                        .iter()
                        .any(|candidate| candidate.id == role && !candidate.archived)
                })
        }
        Command::SpawnChildWorkItems {
            parent_task_id,
            children,
        } => {
            *parent_task_id == task_id
                && !children.is_empty()
                && children.iter().all(|child| {
                    child.mission_id.is_none_or(|mission| mission == mission_id)
                        && child
                            .dependencies
                            .iter()
                            .all(|dependency| *dependency == task_id || task_is_owned(*dependency))
                })
        }
        Command::RequestHumanInput {
            task_id: candidate, ..
        }
        | Command::RequestTaskRework {
            task_id: candidate, ..
        } => *candidate == task_id,
        Command::SendMessage(spec) => {
            spec.mission_id == mission_id && spec.task_id == Some(task_id)
        }
        Command::AddTaskComment {
            task_id: candidate, ..
        } => *candidate == task_id,
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

pub(crate) fn bearer(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get("authorization")?.to_str().ok()?;
    let (scheme, token) = value.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then_some(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    fn request(username: &str, password: &str) -> AuthLoginRequest {
        AuthLoginRequest {
            username: username.into(),
            password: password.into(),
            device_name: None,
        }
    }

    #[tokio::test]
    async fn owner_login_persists_only_a_password_hash_and_session_digest() {
        let store = frank_store::Store::open_in_memory().await.unwrap();
        let auth = AuthManager::new(store.clone());
        let owner = auth
            .initialize_owner("Alice", "correct horse 1")
            .await
            .unwrap();
        assert_eq!(owner.username, "alice");
        let stored_owner = store.owner().await.unwrap().unwrap();
        assert_ne!(stored_owner.password_hash, "correct horse 1");
        assert!(stored_owner.password_hash.starts_with("$argon2id$"));

        let response = auth
            .login(
                request("ALICE", "correct horse 1"),
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                store.server_id(),
            )
            .await
            .unwrap();
        assert_eq!(response.owner, owner);
        assert_eq!(response.access_token.len(), 64);
        assert_eq!(response.device_id, response.session.device_id);
        // Looking up the raw bearer as a digest must fail, while its SHA-256
        // representation authenticates successfully. This guards against a
        // future store implementation accidentally persisting plaintext.
        assert!(
            store
                .auth_session(&response.access_token, now())
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            store
                .auth_session(&hex::encode(hash(&response.access_token)), now())
                .await
                .unwrap()
                .is_some()
        );

        let authenticated = auth.authenticate(&response.access_token).await.unwrap();
        assert_eq!(authenticated.unwrap().device_id, response.device_id);
        assert!(auth.logout(&response.access_token).await.unwrap());
        assert!(
            auth.authenticate(&response.access_token)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn password_change_revokes_existing_sessions_and_new_password_works() {
        let store = frank_store::Store::open_in_memory().await.unwrap();
        let auth = AuthManager::new(store.clone());
        auth.initialize_owner("owner", "old password 11")
            .await
            .unwrap();
        let first = auth
            .login(
                request("owner", "old password 11"),
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                store.server_id(),
            )
            .await
            .unwrap();
        let second = auth
            .login(
                request("owner", "old password 11"),
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                store.server_id(),
            )
            .await
            .unwrap();
        assert!(
            auth.change_password(
                &first.access_token,
                AuthPasswordRequest {
                    current_password: "old password 11".into(),
                    new_password: "new password 22".into(),
                },
            )
            .await
            .unwrap()
        );
        assert!(auth.session(&first.access_token).await.unwrap().is_none());
        assert!(auth.session(&second.access_token).await.unwrap().is_none());
        assert!(matches!(
            auth.login(
                request("owner", "old password 11"),
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                store.server_id(),
            )
            .await,
            Err(AuthError::InvalidCredentials)
        ));
        assert!(
            auth.login(
                request("OWNER", "new password 22"),
                IpAddr::V4(Ipv4Addr::LOCALHOST),
                store.server_id(),
            )
            .await
            .is_ok()
        );
    }

    #[tokio::test]
    async fn concurrent_owner_bootstrap_wins_once() {
        let store = frank_store::Store::open_in_memory().await.unwrap();
        let left = AuthManager::new(store.clone());
        let right = AuthManager::new(store.clone());
        let (left_result, right_result) = tokio::join!(
            left.initialize_owner("owner", "left password 123"),
            right.initialize_owner("other", "right password 456"),
        );

        assert_eq!(left_result.is_ok() as u8 + right_result.is_ok() as u8, 1);
        assert_eq!(
            left_result
                .as_ref()
                .err()
                .map(ToString::to_string)
                .or_else(|| right_result.as_ref().err().map(ToString::to_string)),
            Some("local owner account is already initialized".into())
        );
        let winning_username = store.owner().await.unwrap().unwrap().username;
        assert!(matches!(winning_username.as_str(), "owner" | "other"));
        assert_eq!(
            winning_username,
            if left_result.is_ok() {
                "owner"
            } else {
                "other"
            }
        );
    }

    #[test]
    fn username_and_password_validation_matches_the_bootstrap_contract() {
        assert_eq!(normalize_username("AbC_1").unwrap(), "abc_1");
        assert!(normalize_username("ab").is_err());
        assert!(normalize_username("owner name").is_err());
        assert!(validate_password("a b c d e f g").is_err());
        assert!(validate_password("spasi unicode 😀 123").is_ok());
    }
}
