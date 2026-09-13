//! Wire version and endpoint constants.

/// Current Frank daemon/client protocol version.
pub const PROTOCOL_VERSION: u16 = 2;
/// v2 deliberately has no v1 compatibility window.
pub const MIN_COMPATIBLE_CLIENT: u16 = 2;
/// Canonical HTTP/WebSocket API prefix used by every native client.
pub const API_PREFIX: &str = "/v2";
