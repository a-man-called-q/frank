//! Row types returned by the store.
//!
//! Deliberately separate from the protocol DTOs: what is persisted and what is
//! put on the wire are allowed to diverge, and hashes stored here (device
//! tokens, ticket secrets) must never be handed to a protocol type.

use frank_protocol::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub response: CommandResponse,
    pub event: EventEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventPage {
    pub events: Vec<EventEnvelope>,
    pub oldest_seq: Option<u64>,
    pub latest_seq: u64,
    pub resync_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredDevice {
    pub device_id: DeviceId,
    pub name: String,
    pub role: DeviceRole,
    pub token_hash: String,
    pub certificate_fingerprint: String,
    pub revoked: bool,
    pub last_seen_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredPairingTicket {
    pub ticket_id: String,
    pub secret_hash: String,
    pub role: DeviceRole,
    pub certificate_fingerprint: String,
    pub expires_at: u64,
    pub used: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredAgentCapability {
    pub capability_hash: String,
    pub agent_id: frank_protocol::AgentId,
    pub task_id: frank_protocol::TaskId,
    pub issued_at: u64,
    pub expires_at: u64,
    pub revoked: bool,
}
