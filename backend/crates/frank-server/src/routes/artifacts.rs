//! Artifact download and resumable chunked upload.

use crate::auth::*;
use crate::*;

pub(crate) async fn artifact(
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
pub(crate) struct ArtifactUploadChunkResponse {
    upload_id: UploadId,
    received: u64,
}

/// Receive one contiguous, authenticated artifact chunk.  The upload row is
/// created by `BeginArtifactUpload`; this endpoint never trusts a client
/// supplied size or path and only appends at the durable offset recorded by
/// SQLite.
pub(crate) async fn artifact_upload_chunk(
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
pub(crate) fn validate_content_range(
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

pub(crate) async fn agent_task_matches(
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
