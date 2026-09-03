//! Paired-device listing and revocation.

use crate::auth::*;
use crate::*;

#[derive(Debug, Serialize)]
pub(crate) struct DeviceSummary {
    device_id: DeviceId,
    name: String,
    role: DeviceRole,
    revoked: bool,
    last_seen_at: u64,
}

pub(crate) async fn devices(
    State(state): State<ServerState>,
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

pub(crate) async fn revoke_device(
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
