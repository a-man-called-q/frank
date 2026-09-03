//! Short-lived capabilities issued to provider sessions.
//!
//! A capability is the only credential an agent process ever holds, and it is
//! scoped to one task. Tokens are stored hashed and compared by hash, so a
//! leaked snapshot cannot be replayed against the daemon.

use frank_protocol::*;

use crate::*;

#[derive(Debug, Clone)]
pub(crate) struct AgentCapability {
    pub(crate) agent_id: AgentId,
    pub(crate) task_id: TaskId,
    pub(crate) issued_at: u64,
    pub(crate) expires_at: u64,
}

impl Orchestrator {
    /// Issue a short-lived capability for a provider child. The server uses
    /// the same registry to authorize the MCP bridge, so a copied device
    /// token cannot be used to mutate another task through a provider tool.
    pub async fn issue_agent_capability(&self, agent_id: AgentId, task_id: TaskId) -> String {
        let mut bytes = [0_u8; 32];
        let token = if getrandom::fill(&mut bytes).is_ok() {
            hex::encode(bytes)
        } else {
            format!(
                "{}{}",
                uuid::Uuid::new_v4().simple(),
                uuid::Uuid::new_v4().simple()
            )
        };
        let issued_at = epoch_seconds();
        let expires_at = issued_at.saturating_add(900);
        self.agent_capabilities.lock().await.insert(
            token.clone(),
            AgentCapability {
                agent_id,
                task_id,
                issued_at,
                expires_at,
            },
        );
        let capability_hash = hash_capability(&token);
        let _ = self
            .store
            .upsert_agent_capability(&frank_store::StoredAgentCapability {
                capability_hash,
                agent_id,
                task_id,
                issued_at,
                expires_at,
                revoked: false,
            })
            .await;
        token
    }

    pub async fn validate_agent_capability(
        &self,
        token: &str,
        agent_id: AgentId,
        task_id: TaskId,
    ) -> bool {
        let expired = {
            let mut capabilities = self.agent_capabilities.lock().await;
            let Some(capability) = capabilities.get(token) else {
                return false;
            };
            if capability.expires_at < epoch_seconds() {
                capabilities.remove(token);
                true
            } else {
                return capability.agent_id == agent_id && capability.task_id == task_id;
            }
        };
        if expired {
            let _ = self
                .store
                .revoke_agent_capability(&hash_capability(token))
                .await;
        }
        false
    }

    pub async fn agent_capability_actor(&self, token: &str) -> Option<(AgentId, TaskId)> {
        let (actor, expired) = {
            let mut capabilities = self.agent_capabilities.lock().await;
            let capability = capabilities.get(token)?;
            if capability.expires_at < epoch_seconds() {
                capabilities.remove(token);
                (None, true)
            } else {
                (Some((capability.agent_id, capability.task_id)), false)
            }
        };
        if expired {
            let _ = self
                .store
                .revoke_agent_capability(&hash_capability(token))
                .await;
        }
        actor
    }

    pub async fn revoke_agent_capability(&self, token: &str) {
        self.agent_capabilities.lock().await.remove(token);
        let _ = self
            .store
            .revoke_agent_capability(&hash_capability(token))
            .await;
    }

    /// Extend capabilities for sessions that are still alive. The bearer
    /// token never changes, so a provider's MCP process can keep operating
    /// while the daemon renews the durable expiry metadata. Capabilities are
    /// revoked by the existing task/session cleanup paths and on expiry.
    pub(crate) async fn renew_agent_capabilities(&self) {
        let now = epoch_seconds();
        let renew_before = now.saturating_add(300);
        let renewals = {
            let mut capabilities = self.agent_capabilities.lock().await;
            let mut renewals = Vec::new();
            for (token, capability) in capabilities.iter_mut() {
                if capability.expires_at <= renew_before {
                    capability.issued_at = now;
                    capability.expires_at = now.saturating_add(900);
                    renewals.push((
                        hash_capability(token),
                        capability.agent_id,
                        capability.task_id,
                        capability.issued_at,
                        capability.expires_at,
                    ));
                }
            }
            renewals
        };
        for (capability_hash, agent_id, task_id, issued_at, expires_at) in renewals {
            let _ = self
                .store
                .upsert_agent_capability(&frank_store::StoredAgentCapability {
                    capability_hash,
                    agent_id,
                    task_id,
                    issued_at,
                    expires_at,
                    revoked: false,
                })
                .await;
        }
    }
}
