//! Owner pairing and host-runner job transport.

use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path as AxumPath, State, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use frank_protocol::{
    ActorRef, ApiError, CheckRunView, CommandId, CommandResult, Event, RunnerId, RunnerInstallJob,
    RunnerInstallResult, RunnerJobKind, RunnerJobStatus, RunnerJobView, RunnerPathMapping,
    RunnerStatus, RunnerView, Snapshot, TaskGrantEffect, TaskStatus, timestamp_now,
};
use frank_runner::{RunnerFrame, RunnerJob, RunnerJobResult};
use frank_store::Store;
use futures_util::StreamExt;
use serde::Deserialize;
use tokio::sync::{Mutex, mpsc};

use crate::auth::authenticate;
use crate::{RunnerRegistration, ServerState, api_error_response};

#[derive(Debug, Deserialize, Default)]
pub(crate) struct PairRunnerRequest {
    pub name: Option<String>,
    #[serde(default)]
    pub mappings: Vec<RunnerPathMapping>,
}

#[derive(Debug, serde::Serialize)]
struct PairRunnerResponse {
    runner_id: RunnerId,
    token: String,
    connect_path: String,
    view: RunnerView,
}

#[derive(Debug, serde::Serialize)]
struct RunnerListResponse {
    runners: Vec<RunnerView>,
}

#[derive(Clone)]
pub(crate) struct HostRunnerCheckExecutor {
    store: Store,
    registry: Arc<Mutex<crate::RunnerRegistryState>>,
}

impl HostRunnerCheckExecutor {
    pub(crate) fn new(store: Store, registry: Arc<Mutex<crate::RunnerRegistryState>>) -> Self {
        Self { store, registry }
    }
}

#[async_trait::async_trait]
impl frank_orchestrator::HostCheckExecutor for HostRunnerCheckExecutor {
    async fn execute(
        &self,
        request: frank_orchestrator::HostCheckRequest,
    ) -> std::result::Result<CheckRunView, String> {
        let runner_id = select_runner_for_project(&self.store, request.project_id)
            .await
            .map_err(|(_, error)| error.message)?;
        let job = RunnerJob {
            id: uuid::Uuid::new_v4().to_string(),
            runner_id,
            project_id: request.project_id,
            task_id: request.task_id,
            daemon_worktree: request.worktree.to_string_lossy().into_owned(),
            check: request.check,
        };
        dispatch_check_job(&self.store, &self.registry, job, false)
            .await
            .map(|outcome| outcome.check)
            .map_err(|(_, error)| error.message)
    }
}

async fn select_runner_for_project(
    store: &Store,
    project_id: frank_protocol::ProjectId,
) -> std::result::Result<RunnerId, (StatusCode, ApiError)> {
    let runners = store.runner_views().await.map_err(|error| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            frank_store::api_error(&error),
        )
    })?;
    runners
        .into_iter()
        .filter(|runner| matches!(runner.status, RunnerStatus::Idle | RunnerStatus::Busy))
        .find(|runner| {
            runner.path_mappings.iter().any(|mapping| {
                mapping.project_id.is_none() || mapping.project_id == Some(project_id)
            })
        })
        .map(|runner| runner.id)
        .ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                ApiError::new(
                    frank_protocol::ErrorCode::ProviderUnavailable,
                    "no connected host runner is mapped to this project",
                ),
            )
        })
}

pub(crate) async fn runners(State(state): State<ServerState>, headers: HeaderMap) -> Response {
    if !owner(&state, &headers).await {
        return unauthorized_or_forbidden(&state, &headers).await;
    }
    match state.store.runner_views().await {
        Ok(views) => (StatusCode::OK, Json(RunnerListResponse { runners: views })).into_response(),
        Err(error) => api_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            frank_store::api_error(&error),
        ),
    }
}

pub(crate) async fn pair_runner(
    State(state): State<ServerState>,
    headers: HeaderMap,
    Json(request): Json<PairRunnerRequest>,
) -> Response {
    if !owner(&state, &headers).await {
        return unauthorized_or_forbidden(&state, &headers).await;
    }
    let name = request
        .name
        .unwrap_or_else(|| "Frank host runner".into())
        .trim()
        .to_string();
    if name.is_empty() || name.len() > 128 || name.chars().any(char::is_control) {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Validation,
                "runner name is invalid",
            ),
        );
    }
    if request.mappings.is_empty() || request.mappings.len() > 32 {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Validation,
                "at least one runner path mapping is required",
            ),
        );
    }
    if request
        .mappings
        .iter()
        .any(|mapping| !valid_mapping(mapping))
    {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Validation,
                "runner mappings must be absolute and cannot contain parent traversal",
            ),
        );
    }
    let runner_id = RunnerId::new();
    let token = crate::PairingSecret::generate();
    let view = RunnerView {
        id: runner_id,
        name: name.clone(),
        host: "pending host connection".into(),
        status: RunnerStatus::Pairing,
        last_seen_at: Some(timestamp_now()),
        toolchains: vec![
            "flutter".into(),
            "odin".into(),
            "dotnet".into(),
            "cpp".into(),
        ],
        path_mappings: request.mappings.clone(),
    };
    if let Err(error) = state.store.upsert_runner_view(&view).await {
        return api_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            frank_store::api_error(&error),
        );
    }
    state.runner_registry.lock().await.pending.insert(
        token.clone(),
        RunnerRegistration {
            id: runner_id,
            name: name.clone(),
            mappings: request.mappings,
        },
    );
    (
        StatusCode::OK,
        Json(PairRunnerResponse {
            runner_id,
            token,
            connect_path: crate::api_path("/runners/connect"),
            view,
        }),
    )
        .into_response()
}

pub(crate) async fn runner_connect(
    State(state): State<ServerState>,
    ws: WebSocketUpgrade,
) -> Response {
    ws.on_upgrade(move |socket| runner_socket(state, socket))
        .into_response()
}

async fn runner_socket(state: ServerState, mut socket: WebSocket) {
    let Some(Ok(Message::Text(payload))) = socket.next().await else {
        return;
    };
    let Ok(RunnerFrame::Hello {
        runner_id,
        name: _,
        token,
    }) = serde_json::from_str::<RunnerFrame>(&payload)
    else {
        let _ = socket
            .send(Message::Text(
                serde_json::to_string(&RunnerFrame::Error {
                    message: "runner hello is invalid".into(),
                })
                .unwrap_or_default()
                .into(),
            ))
            .await;
        return;
    };
    let pending_registration = {
        let mut registry = state.runner_registry.lock().await;
        registry.pending.remove(&token)
    };
    let (registration, credential) = if let Some(registration) = pending_registration {
        if registration.id != runner_id {
            send_runner_error(&mut socket, "runner pairing identity mismatch").await;
            return;
        }
        let credential = crate::PairingSecret::generate();
        if state
            .store
            .upsert_runner_credential_hash(runner_id, &hex::encode(crate::hash(&credential)))
            .await
            .is_err()
        {
            send_runner_error(&mut socket, "runner credential could not be persisted").await;
            return;
        }
        (registration, credential)
    } else {
        let Some(view) = state.store.runner_view(runner_id).await.ok().flatten() else {
            send_runner_error(&mut socket, "runner is not paired").await;
            return;
        };
        let matches = state
            .store
            .runner_credential_matches(runner_id, &hex::encode(crate::hash(&token)))
            .await
            .unwrap_or(false);
        if !matches {
            send_runner_error(&mut socket, "runner credential is invalid").await;
            return;
        }
        (
            RunnerRegistration {
                id: view.id,
                name: view.name,
                mappings: view.path_mappings,
            },
            token.clone(),
        )
    };
    let view = RunnerView {
        id: registration.id,
        name: registration.name.clone(),
        host: "connected host runner".into(),
        status: RunnerStatus::Idle,
        last_seen_at: Some(timestamp_now()),
        toolchains: vec![
            "flutter".into(),
            "odin".into(),
            "dotnet".into(),
            "cpp".into(),
        ],
        path_mappings: registration.mappings.clone(),
    };
    let _ = state.store.upsert_runner_view(&view).await;
    let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<RunnerFrame>(32);
    state
        .runner_registry
        .lock()
        .await
        .connections
        .insert(registration.id, outgoing_tx);
    let _ = socket
        .send(Message::Text(
            serde_json::to_string(&RunnerFrame::Welcome { credential })
                .unwrap_or_default()
                .into(),
        ))
        .await;

    loop {
        tokio::select! {
            Some(frame) = outgoing_rx.recv() => {
                let Ok(payload) = serde_json::to_string(&frame) else { break; };
                if socket.send(Message::Text(payload.into())).await.is_err() { break; }
            }
            message = socket.next() => {
                match message {
                    Some(Ok(Message::Text(payload))) => {
                        if let Ok(frame) = serde_json::from_str::<RunnerFrame>(&payload) {
                            let (result_id, result_runner_id) = match &frame {
                                RunnerFrame::Result(result) => (result.id.clone(), result.runner_id),
                                RunnerFrame::InstallResult(result) => {
                                    (result.id.clone(), result.runner_id)
                                }
                                _ => continue,
                            };
                            if result_runner_id == registration.id
                                && let Some(waiter) = state
                                    .runner_registry
                                    .lock()
                                    .await
                                    .pending_jobs
                                    .remove(&result_id)
                            {
                                let _ = waiter.send(frame);
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() { break; }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(Message::Pong(_))) | Some(Ok(Message::Binary(_))) => {}
                }
            }
        }
    }
    let mut registry = state.runner_registry.lock().await;
    registry.connections.remove(&registration.id);
    drop(registry);
    let mut offline = view;
    offline.status = RunnerStatus::Offline;
    offline.last_seen_at = Some(timestamp_now());
    let _ = state.store.upsert_runner_view(&offline).await;
}

async fn send_runner_error(socket: &mut WebSocket, message: &str) {
    let _ = socket
        .send(Message::Text(
            serde_json::to_string(&RunnerFrame::Error {
                message: message.into(),
            })
            .unwrap_or_default()
            .into(),
        ))
        .await;
}

pub(crate) async fn run_check(
    State(state): State<ServerState>,
    headers: HeaderMap,
    AxumPath(path_runner_id): AxumPath<RunnerId>,
    Json(job): Json<RunnerJob>,
) -> Response {
    if !owner(&state, &headers).await {
        return unauthorized_or_forbidden(&state, &headers).await;
    }
    if job.runner_id != path_runner_id {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Validation,
                "runner id in path and job do not match",
            ),
        );
    }
    match dispatch_check_job(&state.store, &state.runner_registry, job, true).await {
        Ok(outcome) => (StatusCode::OK, Json(outcome.result)).into_response(),
        Err((status, error)) => api_error_response(status, error),
    }
}

#[derive(Debug)]
struct CheckDispatchOutcome {
    result: RunnerJobResult,
    check: CheckRunView,
}

type CheckDispatchResult = std::result::Result<CheckDispatchOutcome, (StatusCode, ApiError)>;

async fn dispatch_check_job(
    store: &Store,
    registry: &Arc<Mutex<crate::RunnerRegistryState>>,
    mut job: RunnerJob,
    enforce_task_grant: bool,
) -> CheckDispatchResult {
    if job.id.trim().is_empty() {
        job.id = uuid::Uuid::new_v4().to_string();
    }
    let runner_view = store
        .runner_view(job.runner_id)
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                frank_store::api_error(&error),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                ApiError::new(frank_protocol::ErrorCode::NotFound, "runner is not paired"),
            )
        })?;
    if enforce_task_grant && let Some(task_id) = job.task_id {
        let snapshot = store.snapshot().await.map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                frank_store::api_error(&error),
            )
        })?;
        if !task_grant_allows_check(&snapshot, task_id, &job.daemon_worktree) {
            return Err((
                StatusCode::FORBIDDEN,
                ApiError::new(
                    frank_protocol::ErrorCode::Forbidden,
                    "a non-network task check grant is required for this worktree",
                ),
            ));
        }
    }
    if runner_view
        .path_mappings
        .iter()
        .all(|mapping| mapping.project_id.is_some() && mapping.project_id != Some(job.project_id))
    {
        return Err((
            StatusCode::FORBIDDEN,
            ApiError::new(
                frank_protocol::ErrorCode::Forbidden,
                "runner has no path mapping for this project",
            ),
        ));
    }
    let created_at = timestamp_now();
    let mut durable_job = RunnerJobView {
        id: job.id.clone(),
        runner_id: job.runner_id,
        project_id: job.project_id,
        task_id: job.task_id,
        check_id: job.check.id.clone(),
        kind: RunnerJobKind::Check,
        status: RunnerJobStatus::Queued,
        created_at: created_at.clone(),
        updated_at: created_at,
        result: None,
        install_result: None,
    };
    store
        .upsert_runner_job(&durable_job)
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                frank_store::api_error(&error),
            )
        })?;
    let (waiter, receiver) = tokio::sync::oneshot::channel();
    let sender = {
        let registry = registry.lock().await;
        registry.connections.get(&job.runner_id).cloned()
    };
    let Some(sender) = sender else {
        durable_job.status = RunnerJobStatus::Failed;
        durable_job.updated_at = timestamp_now();
        let _ = store.upsert_runner_job(&durable_job).await;
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            ApiError::new(
                frank_protocol::ErrorCode::ProviderUnavailable,
                "host runner is offline",
            ),
        ));
    };
    registry
        .lock()
        .await
        .pending_jobs
        .insert(job.id.clone(), waiter);
    if sender.send(RunnerFrame::Run(job.clone())).await.is_err() {
        registry.lock().await.pending_jobs.remove(&job.id);
        durable_job.status = RunnerJobStatus::Failed;
        durable_job.updated_at = timestamp_now();
        let _ = store.upsert_runner_job(&durable_job).await;
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            ApiError::new(
                frank_protocol::ErrorCode::ProviderUnavailable,
                "host runner connection closed",
            ),
        ));
    }
    let timeout_seconds = job.check.timeout_seconds.saturating_add(30).min(86_400);
    durable_job.status = RunnerJobStatus::Running;
    durable_job.updated_at = timestamp_now();
    let _ = store.upsert_runner_job(&durable_job).await;
    let frame = match tokio::time::timeout(Duration::from_secs(timeout_seconds), receiver).await {
        Ok(Ok(frame)) => frame,
        _ => {
            registry.lock().await.pending_jobs.remove(&job.id);
            durable_job.status = RunnerJobStatus::TimedOut;
            durable_job.updated_at = timestamp_now();
            let _ = store.upsert_runner_job(&durable_job).await;
            return Err((
                StatusCode::GATEWAY_TIMEOUT,
                ApiError::new(
                    frank_protocol::ErrorCode::RateLimited,
                    "host runner check timed out",
                ),
            ));
        }
    };
    let RunnerFrame::Result(result) = frame else {
        durable_job.status = RunnerJobStatus::Failed;
        durable_job.updated_at = timestamp_now();
        let _ = store.upsert_runner_job(&durable_job).await;
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::new(
                frank_protocol::ErrorCode::Internal,
                "host runner returned an invalid result",
            ),
        ));
    };
    if result.id != job.id || result.runner_id != job.runner_id {
        durable_job.status = RunnerJobStatus::Failed;
        durable_job.updated_at = timestamp_now();
        let _ = store.upsert_runner_job(&durable_job).await;
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            ApiError::new(
                frank_protocol::ErrorCode::Internal,
                "host runner result does not match the durable check job",
            ),
        ));
    }
    let check = record_check_run(store, &job, &result)
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                frank_store::api_error(&error),
            )
        })?;
    durable_job.status = match result.status {
        frank_protocol::CheckRunStatus::Passed => RunnerJobStatus::Passed,
        frank_protocol::CheckRunStatus::TimedOut => RunnerJobStatus::TimedOut,
        frank_protocol::CheckRunStatus::Cancelled => RunnerJobStatus::Cancelled,
        _ => RunnerJobStatus::Failed,
    };
    durable_job.updated_at = timestamp_now();
    durable_job.result = Some(check.clone());
    store
        .upsert_runner_job(&durable_job)
        .await
        .map_err(|error| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                frank_store::api_error(&error),
            )
        })?;
    Ok(CheckDispatchOutcome { result, check })
}

#[derive(Debug, Deserialize)]
pub(crate) struct InstallToolchainRequest {
    pub project_id: frank_protocol::ProjectId,
    pub task_id: frank_protocol::TaskId,
    pub manifest_id: String,
    pub version: String,
    pub project_path: String,
    pub approval_id: frank_protocol::ApprovalId,
}

pub(crate) async fn install_toolchain(
    State(state): State<ServerState>,
    headers: HeaderMap,
    AxumPath(path_runner_id): AxumPath<RunnerId>,
    Json(request): Json<InstallToolchainRequest>,
) -> Response {
    if !owner(&state, &headers).await {
        return unauthorized_or_forbidden(&state, &headers).await;
    }
    if request.manifest_id.trim().is_empty()
        || request.version.trim().is_empty()
        || request.project_path.trim().is_empty()
    {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Validation,
                "toolchain install request is incomplete",
            ),
        );
    }
    let Some(project) = state.store.snapshot().await.ok().and_then(|snapshot| {
        snapshot
            .projects
            .into_iter()
            .find(|project| project.id == request.project_id && !project.archived)
    }) else {
        return api_error_response(
            StatusCode::NOT_FOUND,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::NotFound,
                "project is not registered",
            ),
        );
    };
    let Ok(project_path) = std::fs::canonicalize(&request.project_path) else {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Validation,
                "project path is not available",
            ),
        );
    };
    let Ok(registered_path) = std::fs::canonicalize(&project.path) else {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Validation,
                "registered project path is not available",
            ),
        );
    };
    if project_path != registered_path {
        return api_error_response(
            StatusCode::FORBIDDEN,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Forbidden,
                "toolchain install path does not match the registered project",
            ),
        );
    }
    let snapshot = match state.store.snapshot().await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            return api_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                frank_store::api_error(&error),
            );
        }
    };
    let Some(task) = snapshot
        .tasks
        .iter()
        .find(|task| task.id == request.task_id)
    else {
        return api_error_response(
            StatusCode::NOT_FOUND,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::NotFound,
                "install task is not found",
            ),
        );
    };
    let Some(approval) = snapshot
        .approvals
        .iter()
        .find(|approval| approval.id == request.approval_id)
    else {
        return api_error_response(
            StatusCode::NOT_FOUND,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::NotFound,
                "toolchain install approval is not found",
            ),
        );
    };
    let expected_operation = format!(
        "toolchain-install:{}:{}",
        request.manifest_id, request.version
    );
    let task_project_matches = snapshot
        .missions
        .iter()
        .any(|mission| mission.id == task.mission_id && mission.project_id == request.project_id);
    let assigned_agent_is_active = task.assigned_agent.is_some_and(|agent_id| {
        snapshot
            .agents
            .iter()
            .any(|agent| agent.id == agent_id && !agent.archived)
    });
    if approval.task_id != request.task_id
        || approval.operation != expected_operation
        || approval.status != frank_protocol::ApprovalStatus::Approved
        || !task_project_matches
        || task.assigned_agent != Some(approval.agent_id)
        || !assigned_agent_is_active
        || approval.expires_at.parse::<u128>().unwrap_or_default()
            <= frank_protocol::timestamp_now()
                .parse::<u128>()
                .unwrap_or_default()
        || !matches!(
            task.status,
            frank_protocol::TaskStatus::Running | frank_protocol::TaskStatus::Review
        )
    {
        return api_error_response(
            StatusCode::FORBIDDEN,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Forbidden,
                "an unexpired, task-scoped toolchain approval is required",
            ),
        );
    }
    let manifests = match crate::routes::toolchains::manifests_for_project(&project_path) {
        Ok(manifests) => manifests,
        Err(error) => {
            return api_error_response(
                StatusCode::BAD_REQUEST,
                frank_protocol::ApiError::new(frank_protocol::ErrorCode::Validation, error),
            );
        }
    };
    let Some(manifest) = manifests
        .into_iter()
        .find(|manifest| manifest.id == request.manifest_id && manifest.version == request.version)
    else {
        return api_error_response(
            StatusCode::NOT_FOUND,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::NotFound,
                "toolchain manifest/version is not available",
            ),
        );
    };
    let data_directory = state
        .store
        .database_path()
        .and_then(|path| path.parent().map(std::path::PathBuf::from))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let plan = match frank_toolchain::resolve_install_plan(
        &manifest,
        &frank_toolchain::platform_key(),
        &data_directory,
    ) {
        Ok(plan) => plan,
        Err(error) => {
            return api_error_response(
                StatusCode::BAD_REQUEST,
                frank_protocol::ApiError::new(
                    frank_protocol::ErrorCode::Validation,
                    error.to_string(),
                ),
            );
        }
    };
    let Some(artifact) = plan.artifact else {
        return api_error_response(
            StatusCode::BAD_REQUEST,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Validation,
                "toolchain install preview has no immutable artifact",
            ),
        );
    };
    let Some(runner_view) = state.store.runner_view(path_runner_id).await.ok().flatten() else {
        return api_error_response(
            StatusCode::NOT_FOUND,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::NotFound,
                "runner is not paired",
            ),
        );
    };
    if runner_view.path_mappings.iter().all(|mapping| {
        mapping.project_id.is_some() && mapping.project_id != Some(request.project_id)
    }) {
        return api_error_response(
            StatusCode::FORBIDDEN,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Forbidden,
                "runner has no path mapping for this project",
            ),
        );
    }
    let id = uuid::Uuid::new_v4().to_string();
    let install_job = RunnerInstallJob {
        id: id.clone(),
        runner_id: path_runner_id,
        project_id: request.project_id,
        task_id: Some(request.task_id),
        daemon_worktree: request.project_path.clone(),
        manifest_id: manifest.id.clone(),
        version: manifest.version.clone(),
        artifact,
        install_relative_path: format!("{}/{}/artifact", manifest.id, manifest.version),
    };
    let created_at = timestamp_now();
    let mut durable_job = RunnerJobView {
        id: id.clone(),
        runner_id: path_runner_id,
        project_id: request.project_id,
        task_id: Some(request.task_id),
        check_id: format!("toolchain-install:{}:{}", manifest.id, manifest.version),
        kind: RunnerJobKind::ToolchainInstall,
        status: RunnerJobStatus::Queued,
        created_at: created_at.clone(),
        updated_at: created_at,
        result: None,
        install_result: None,
    };
    if let Err(error) = state.store.upsert_runner_job(&durable_job).await {
        return api_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            frank_store::api_error(&error),
        );
    }
    let sender = {
        let registry = state.runner_registry.lock().await;
        registry.connections.get(&path_runner_id).cloned()
    };
    let Some(sender) = sender else {
        durable_job.status = RunnerJobStatus::Failed;
        durable_job.updated_at = timestamp_now();
        let _ = state.store.upsert_runner_job(&durable_job).await;
        return api_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::ProviderUnavailable,
                "host runner is offline",
            ),
        );
    };
    let (waiter, receiver) = tokio::sync::oneshot::channel();
    state
        .runner_registry
        .lock()
        .await
        .pending_jobs
        .insert(id.clone(), waiter);
    if sender
        .send(RunnerFrame::Install(install_job))
        .await
        .is_err()
    {
        state.runner_registry.lock().await.pending_jobs.remove(&id);
        durable_job.status = RunnerJobStatus::Failed;
        durable_job.updated_at = timestamp_now();
        let _ = state.store.upsert_runner_job(&durable_job).await;
        return api_error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::ProviderUnavailable,
                "host runner connection closed",
            ),
        );
    }
    durable_job.status = RunnerJobStatus::Running;
    durable_job.updated_at = timestamp_now();
    let _ = state.store.upsert_runner_job(&durable_job).await;
    let (result, timed_out) =
        match tokio::time::timeout(Duration::from_secs(15 * 60), receiver).await {
            Ok(Ok(RunnerFrame::InstallResult(result))) => (result, false),
            _ => {
                state.runner_registry.lock().await.pending_jobs.remove(&id);
                (
                    RunnerInstallResult {
                        id: id.clone(),
                        runner_id: path_runner_id,
                        status: frank_protocol::ToolchainRequirementStatus::Failed,
                        installed_path: None,
                        diagnostic: Some("host runner install timed out".into()),
                    },
                    true,
                )
            }
        };
    durable_job.status = if timed_out {
        RunnerJobStatus::TimedOut
    } else if result.status == frank_protocol::ToolchainRequirementStatus::Ready {
        RunnerJobStatus::Passed
    } else {
        RunnerJobStatus::Failed
    };
    durable_job.updated_at = timestamp_now();
    durable_job.install_result = Some(result.clone());
    if let Err(error) = state.store.upsert_runner_job(&durable_job).await {
        return api_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            frank_store::api_error(&error),
        );
    }
    let snapshot = match state.store.snapshot().await {
        Ok(snapshot) => snapshot,
        Err(error) => {
            return api_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                frank_store::api_error(&error),
            );
        }
    };
    if let Err(error) = state
        .store
        .commit_command(
            CommandId::new(),
            None,
            ActorRef::system(),
            Event::ToolchainInstallationRecorded {
                manifest_id: manifest.id,
                version: manifest.version,
                runner_id: path_runner_id,
                status: result.status.clone(),
                project_id: Some(request.project_id),
                task_id: Some(request.task_id),
                install_path: result.installed_path.clone(),
            },
            snapshot,
            CommandResult::Accepted,
        )
        .await
    {
        return api_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            frank_store::api_error(&error),
        );
    }
    (StatusCode::OK, Json(result)).into_response()
}

async fn record_check_run(
    store: &Store,
    job: &RunnerJob,
    result: &RunnerJobResult,
) -> frank_store::Result<CheckRunView> {
    let finished_at = timestamp_now();
    let finished_millis = finished_at.parse::<u128>().unwrap_or_default();
    let started_at = finished_millis
        .saturating_sub(result.duration_ms as u128)
        .to_string();
    let check = CheckRunView {
        id: result.id.clone(),
        runner_id: result.runner_id,
        project_id: job.project_id,
        task_id: job.task_id,
        check_id: job.check.id.clone(),
        status: result.status,
        exit_code: result.exit_code,
        stdout: result.stdout.clone(),
        stderr: result.stderr.clone(),
        duration_ms: result.duration_ms,
        started_at,
        finished_at: Some(finished_at),
    };
    let snapshot = store.snapshot().await?;
    store
        .commit_command(
            CommandId::new(),
            None,
            ActorRef::system(),
            Event::CheckRunRecorded {
                check: check.clone(),
            },
            snapshot,
            CommandResult::Accepted,
        )
        .await?;
    Ok(check)
}

async fn owner(state: &ServerState, headers: &HeaderMap) -> bool {
    authenticate(state, headers)
        .await
        .is_some_and(|auth| auth.role.can_admin())
}

async fn unauthorized_or_forbidden(state: &ServerState, headers: &HeaderMap) -> Response {
    match authenticate(state, headers).await {
        None => api_error_response(
            StatusCode::UNAUTHORIZED,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Unauthorized,
                "device authentication required",
            ),
        ),
        Some(_) => api_error_response(
            StatusCode::FORBIDDEN,
            frank_protocol::ApiError::new(
                frank_protocol::ErrorCode::Forbidden,
                "owner role required",
            ),
        ),
    }
}

fn valid_mapping(mapping: &RunnerPathMapping) -> bool {
    [mapping.daemon_root.as_str(), mapping.host_root.as_str()]
        .into_iter()
        .all(|path| {
            path.starts_with('/')
                && !path.chars().any(|character| character.is_control())
                && !path.split('/').any(|part| part == "..")
        })
}

fn task_grant_allows_check(
    snapshot: &Snapshot,
    task_id: frank_protocol::TaskId,
    worktree: &str,
) -> bool {
    let Some(task) = snapshot.tasks.iter().find(|task| task.id == task_id) else {
        return false;
    };
    if !matches!(task.status, TaskStatus::Running | TaskStatus::Review) {
        return false;
    }
    let Some(task_worktree) = task.worktree.as_deref() else {
        return false;
    };
    let (Ok(task_worktree), Ok(worktree)) = (
        std::fs::canonicalize(task_worktree),
        std::fs::canonicalize(worktree),
    ) else {
        return false;
    };
    if task_worktree != worktree {
        return false;
    }
    let now = timestamp_now().parse::<u128>().unwrap_or_default();
    snapshot.task_grants.iter().any(|grant| {
        grant.task_id == task_id
            && grant.effect == TaskGrantEffect::Check
            && !grant.revoked
            && grant
                .expires_at
                .parse::<u128>()
                .is_ok_and(|expires_at| expires_at > now)
            && std::fs::canonicalize(&grant.worktree).is_ok_and(|path| path == task_worktree)
    })
}
