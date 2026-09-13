//! The wire error envelope and its closed code set.

use serde::{Deserialize, Serialize};

use crate::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiError {
    pub code: ErrorCode,
    pub message: String,
    pub retryable: bool,
    pub latest_snapshot: Option<Box<Snapshot>>,
}

impl ApiError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: false,
            latest_snapshot: None,
        }
    }

    pub fn conflict(snapshot: Snapshot) -> Self {
        Self {
            code: ErrorCode::StaleRevision,
            message: "the server state changed; refresh and retry".to_string(),
            retryable: true,
            latest_snapshot: Some(Box::new(snapshot)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorCode {
    Unauthorized,
    Forbidden,
    Validation,
    NotFound,
    Conflict,
    StaleRevision,
    /// A draft or published Organization revision is stale. This is kept
    /// separate from the global snapshot revision because unrelated task
    /// events must not make an autosave fail.
    OrganizationRevisionConflict,
    BudgetExceeded,
    ProviderUnavailable,
    ResyncRequired,
    VersionMismatch,
    Internal,
    PayloadTooLarge,
    PairingExpired,
    PairingReused,
    CertificateMismatch,
    LeaseUnavailable,
    RateLimited,
    PairingDisabled,
}
