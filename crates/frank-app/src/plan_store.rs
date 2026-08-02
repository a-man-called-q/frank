use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

use crate::{AppError, PLAN_TTL};

pub(super) struct Prepared<T> {
    pub(super) payload: T,
    pub(super) state_fingerprint: String,
    pub(super) created: Instant,
}

pub(super) struct PreparedStore<T> {
    entries: Mutex<HashMap<String, Prepared<T>>>,
}

impl<T> PreparedStore<T> {
    pub(super) fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    pub(super) fn insert(
        &self,
        plan_id: String,
        payload: T,
        state_fingerprint: String,
        now: Instant,
    ) -> Result<(), AppError> {
        let mut entries = self.entries.lock().map_err(|_| AppError::StalePlan)?;
        entries.retain(|_, prepared| now.saturating_duration_since(prepared.created) <= PLAN_TTL);
        entries.insert(
            plan_id,
            Prepared {
                payload,
                state_fingerprint,
                created: now,
            },
        );
        Ok(())
    }

    pub(super) fn take(&self, plan_id: &str) -> Result<Prepared<T>, AppError> {
        self.entries
            .lock()
            .map_err(|_| AppError::StalePlan)?
            .remove(plan_id)
            .ok_or(AppError::StalePlan)
    }
}
