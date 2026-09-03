//! Typed identifiers and the wire timestamp.
//!
//! Every id is a newtype over Uuid so a MissionId can never be passed where a
//! TaskId is expected; the macro keeps the twenty of them from drifting apart.

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// RFC3339-like UTC timestamp used on the wire.  Keeping this as a string
/// avoids coupling every consumer to a date/time crate while preserving a
/// stable, sortable representation.
pub type Timestamp = String;

pub fn timestamp_now() -> Timestamp {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{millis}")
}

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            pub const fn nil() -> Self {
                Self(Uuid::nil())
            }

            pub fn parse(value: &str) -> Result<Self, uuid::Error> {
                Ok(Self(Uuid::parse_str(value)?))
            }

            pub const fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl From<Uuid> for $name {
            fn from(value: Uuid) -> Self {
                Self(value)
            }
        }

        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

id_type!(ServerId);
id_type!(DeviceId);
id_type!(ProjectId);
id_type!(AgentId);
id_type!(MissionId);
id_type!(TaskId);
id_type!(AttemptId);
id_type!(MessageId);
id_type!(ApprovalId);
id_type!(ArtifactId);
id_type!(TerminalSessionId);
id_type!(CommandId);
id_type!(CorrelationId);
id_type!(OperationId);
id_type!(UploadId);
id_type!(UpdateId);
