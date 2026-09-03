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

pub(crate) async fn pair_prepare(
    State(state): State<ServerState>,
    ConnectInfo(address): ConnectInfo<SocketAddr>,
    Json(role): Json<DeviceRole>,
) -> impl IntoResponse {
    // A pairing ticket is an authority bootstrap operation.  It may be
    // requested by the local CLI on a server bound to a LAN address, but a
    // remote peer must never be able to mint tickets without an existing
    // device token. The production listener installs the concrete peer
    // address below, and the router uses the same extractor in tests.
    if !address.ip().is_loopback() {
        return api_error_response(
            StatusCode::FORBIDDEN,
            ApiError::new(
                ErrorCode::Forbidden,
                "pairing tickets can only be minted by the server host",
            ),
        );
    }
    let ticket = match state.pairing.prepare_durable(role).await {
        Ok(ticket) => ticket,
        Err(_) => {
            return api_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                ApiError::new(ErrorCode::Internal, "pairing ticket could not be persisted"),
            );
        }
    };
    Json(serde_json::json!({
        "secret": ticket.secret,
        "role": ticket.role,
        "certificate_fingerprint": ticket.certificate_fingerprint,
        "expires_at": ticket.expires_at.to_string(),
    }))
    .into_response()
}

pub(crate) async fn pair(
    State(state): State<ServerState>,
    Json(request): Json<PairingRequest>,
) -> impl IntoResponse {
    match state.pairing.pair(&request).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error) => api_error_response(status_for_error(error.code), error),
    }
}
