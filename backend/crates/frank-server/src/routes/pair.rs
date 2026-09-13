//! The pairing handshake endpoints.

use crate::*;

pub(crate) async fn handshake(
    State(state): State<ServerState>,
    Json(request): Json<HandshakeRequest>,
) -> impl IntoResponse {
    let negotiated = match negotiate_versions(&request.supported_versions, &VersionRange::current())
    {
        Ok(version) if request.protocol_version == version => version,
        Ok(_) => {
            return api_error_response(
                StatusCode::UPGRADE_REQUIRED,
                ApiError::new(
                    ErrorCode::VersionMismatch,
                    "client protocol version is outside the negotiated range",
                ),
            );
        }
        Err(error) => {
            return api_error_response(StatusCode::UPGRADE_REQUIRED, error);
        }
    };
    let capabilities = capability_document(&state).await;
    (
        StatusCode::OK,
        Json(HandshakeResponse {
            negotiated_version: negotiated,
            server_id: capabilities.server_id,
            server_version: capabilities.server_version.clone(),
            capabilities,
        }),
    )
        .into_response()
}

pub(crate) async fn pair_prepare(State(_state): State<ServerState>) -> impl IntoResponse {
    api_error_response(
        StatusCode::GONE,
        ApiError::new(
            ErrorCode::PairingDisabled,
            "device pairing is disabled; initialize and use a local owner account",
        ),
    )
}

pub(crate) async fn pair(State(_state): State<ServerState>) -> impl IntoResponse {
    api_error_response(
        StatusCode::GONE,
        ApiError::new(
            ErrorCode::PairingDisabled,
            "device pairing is disabled; login with the local owner account",
        ),
    )
}
