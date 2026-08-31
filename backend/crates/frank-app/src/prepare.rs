//! Shared prepare/apply flow for target and pack plans: fingerprint state,
//! bump a nonce, derive a stable plan id, and store it; then on apply, take
//! the plan once and reject it if it has expired or the state fingerprint
//! no longer matches. Target and pack plans used to implement this
//! algorithm independently, with differently-shaped staleness checks (a
//! bare `String` comparison for targets, an `Option` comparison via `.ok()`
//! for packs) — this is the one algorithm both now share.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use crate::fingerprint::Fingerprint;
use crate::plan_store::PreparedStore;
use crate::{AppError, Clock, PLAN_TTL};

pub(crate) struct PreparationFlow<'a, T> {
    pub(crate) store: &'a PreparedStore<T>,
    pub(crate) clock: &'a dyn Clock,
    pub(crate) nonce: &'a AtomicU64,
    pub(crate) id_prefix: &'static str,
}

impl<T> PreparationFlow<'_, T> {
    pub(crate) fn prepare(
        &self,
        payload: T,
        identity: &str,
        fingerprint: String,
    ) -> Result<String, AppError> {
        let now = self.clock.now();
        let nonce = self.nonce.fetch_add(1, Ordering::Relaxed);
        let plan_id = self.plan_id(identity, &fingerprint, now, nonce);
        self.store
            .insert(plan_id.clone(), payload, fingerprint, now)?;
        Ok(plan_id)
    }

    /// Take the prepared plan once, then reject it as stale if the TTL has
    /// elapsed or `refingerprint` no longer matches the fingerprint
    /// recorded at prepare time — including the case where the current
    /// fingerprint can no longer be computed at all (`None`).
    pub(crate) fn take_valid(
        &self,
        plan_id: &str,
        refingerprint: impl FnOnce(&T) -> Option<String>,
    ) -> Result<T, AppError> {
        let prepared = self.store.take(plan_id)?;
        let expired = self.clock.now().saturating_duration_since(prepared.created) > PLAN_TTL;
        let changed = refingerprint(&prepared.payload).as_deref()
            != Some(prepared.state_fingerprint.as_str());
        if expired || changed {
            return Err(AppError::StalePlan);
        }
        Ok(prepared.payload)
    }

    fn plan_id(&self, identity: &str, fingerprint: &str, now: Instant, nonce: u64) -> String {
        let digest = Fingerprint::new()
            .field(identity.as_bytes())
            .field(fingerprint.as_bytes())
            .field(format!("{now:?}").as_bytes())
            .field(nonce.to_le_bytes())
            .finish();
        format!("{}{digest}", self.id_prefix)
    }
}
