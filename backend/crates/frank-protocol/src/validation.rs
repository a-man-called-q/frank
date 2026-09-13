use crate::*;

pub fn negotiate_versions(client: &VersionRange, server: &VersionRange) -> Result<u16, ApiError> {
    let min = client.min.max(server.min);
    let max = client.max.min(server.max);
    if min > max {
        return Err(ApiError::new(
            ErrorCode::VersionMismatch,
            "client and server protocol versions do not overlap",
        ));
    }
    Ok(max)
}

/// Negotiate feature capabilities without making unknown server extensions a
/// compatibility failure. A caller lists only the features it actually
/// needs; unsupported required features receive a stable validation error,
/// while additional server features are ignored safely.
pub fn require_capabilities(
    server: &Capabilities,
    required: &[&str],
) -> Result<Vec<String>, ApiError> {
    let mut accepted = Vec::with_capacity(required.len());
    for feature in required {
        if !server.features.iter().any(|candidate| candidate == feature) {
            return Err(ApiError::new(
                ErrorCode::Validation,
                format!("server does not support required capability: {feature}"),
            ));
        }
        accepted.push((*feature).to_string());
    }
    Ok(accepted)
}

pub fn validate_command_size(command: &CommandEnvelope) -> Result<(), ApiError> {
    let encoded = serde_json::to_vec(command).map_err(|_| {
        ApiError::new(
            ErrorCode::Validation,
            "command could not be serialized for validation",
        )
    })?;
    if encoded.len() > MAX_COMMAND_BODY_BYTES {
        return Err(ApiError::new(
            ErrorCode::PayloadTooLarge,
            "command payload exceeds the server limit",
        ));
    }
    Ok(())
}
