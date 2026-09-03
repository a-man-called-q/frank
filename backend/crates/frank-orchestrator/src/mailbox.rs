//! In-memory delivery queue for agent-to-agent messages.
//!
//! Durable state lives in the snapshot; this only tracks what is in flight.

use std::collections::HashSet;

use frank_protocol::*;

use crate::DEFAULT_MESSAGE_HOP_LIMIT;

#[derive(Debug, Clone)]
pub struct Mailbox {
    pub hop_limit: u8,
    seen: HashSet<MessageId>,
}

impl Default for Mailbox {
    fn default() -> Self {
        Self {
            hop_limit: DEFAULT_MESSAGE_HOP_LIMIT,
            seen: HashSet::new(),
        }
    }
}

impl Mailbox {
    pub fn accept(&mut self, message_id: MessageId, hop: u8) -> bool {
        hop <= self.hop_limit && self.seen.insert(message_id)
    }
}
