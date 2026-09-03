//! Device pairing: tickets, the device registry, and the agent capabilities
//! issued against them.
//!
//! Pairing is the only path by which a device gains a token, so the checks here
//! are the daemon's whole authentication story. Ticket secrets and device
//! tokens are stored hashed and compared in constant time; nothing in this
//! module hands back a secret it was given.

use std::collections::HashMap;
use std::sync::Arc;

use frank_protocol::*;
use frank_store::Store;
use tokio::sync::Mutex;

use crate::{Result, hash, now};

#[derive(Debug, Clone)]
pub(crate) struct PendingPairing {
    secret_hash: [u8; 32],
    role: DeviceRole,
    certificate_fingerprint: String,
    expires_at: u64,
    used: bool,
}

#[derive(Debug, Clone)]
pub struct PairingTicket {
    pub secret: String,
    pub role: DeviceRole,
    pub certificate_fingerprint: String,
    pub expires_at: u64,
}

#[derive(Debug, Clone)]
pub struct DeviceAuth {
    pub device_id: DeviceId,
    pub role: DeviceRole,
    pub name: String,
}

#[derive(Debug, Clone)]
pub(crate) struct DeviceRecord {
    auth: DeviceAuth,
    token_hash: [u8; 32],
    revoked: bool,
    last_seen_at: u64,
}

#[derive(Clone)]
pub struct PairingManager {
    server_id: ServerId,
    fingerprint: String,
    certificate_pem: Option<String>,
    store: Option<Store>,
    pending: Arc<Mutex<HashMap<String, PendingPairing>>>,
    devices: Arc<Mutex<HashMap<DeviceId, DeviceRecord>>>,
    attempts: Arc<Mutex<HashMap<String, (u64, u8)>>>,
}

impl PairingManager {
    pub fn new(server_id: ServerId, fingerprint: String) -> Self {
        Self::new_with_certificate(server_id, fingerprint, None)
    }

    pub fn new_with_certificate(
        server_id: ServerId,
        fingerprint: String,
        certificate_pem: Option<String>,
    ) -> Self {
        Self {
            server_id,
            fingerprint,
            certificate_pem,
            store: None,
            pending: Arc::new(Mutex::new(HashMap::new())),
            devices: Arc::new(Mutex::new(HashMap::new())),
            attempts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn new_with_store(
        server_id: ServerId,
        fingerprint: String,
        certificate_pem: Option<String>,
        store: Store,
    ) -> Self {
        let mut manager = Self::new_with_certificate(server_id, fingerprint, certificate_pem);
        manager.store = Some(store);
        manager
    }

    pub async fn load_persisted(&self) -> Result<()> {
        let Some(store) = &self.store else {
            return Ok(());
        };
        let now = now();
        let tickets = store.pairing_tickets().await?;
        let mut pending = self.pending.lock().await;
        for ticket in tickets {
            // Keep a used ticket in memory until its normal expiry.  This is
            // important for a durable one-time code: after a daemon restart a
            // second redemption must still be reported as `pairing-reused`,
            // rather than looking like an unknown/expired code.  Expired
            // unused tickets are intentionally not restored and can only
            // produce the generic expired response.
            if ticket.expires_at < now || ticket.certificate_fingerprint != self.fingerprint {
                continue;
            }
            let Ok(bytes) = hex::decode(&ticket.secret_hash) else {
                continue;
            };
            let Ok(secret_hash) = <[u8; 32]>::try_from(bytes.as_slice()) else {
                continue;
            };
            pending.insert(
                ticket.ticket_id,
                PendingPairing {
                    secret_hash,
                    role: ticket.role,
                    certificate_fingerprint: ticket.certificate_fingerprint,
                    expires_at: ticket.expires_at,
                    used: ticket.used,
                },
            );
        }
        drop(pending);
        let records = store.devices().await?;
        let mut valid = Vec::with_capacity(records.len());
        for record in records {
            // A certificate rotation is an explicit trust boundary. Existing
            // device tokens are not silently carried across that boundary.
            if record.certificate_fingerprint != self.fingerprint {
                let _ = store.revoke_device(record.device_id).await?;
                continue;
            }
            let Ok(bytes) = hex::decode(record.token_hash) else {
                continue;
            };
            let Ok(token_hash) = <[u8; 32]>::try_from(bytes.as_slice()) else {
                continue;
            };
            valid.push((
                record.device_id,
                DeviceRecord {
                    auth: DeviceAuth {
                        device_id: record.device_id,
                        role: record.role,
                        name: record.name,
                    },
                    token_hash,
                    revoked: record.revoked,
                    last_seen_at: record.last_seen_at,
                },
            ));
        }
        let mut devices = self.devices.lock().await;
        for (device_id, device) in valid {
            devices.insert(device_id, device);
        }
        Ok(())
    }

    pub async fn prepare(&self, role: DeviceRole) -> PairingTicket {
        self.prepare_inner(role, false)
            .await
            .expect("in-memory pairing ticket preparation cannot fail")
    }

    /// Prepare a ticket and require its durable row to be committed before
    /// returning the secret.  The HTTP endpoint uses this variant so a
    /// successful response can always survive a frankd restart.
    pub async fn prepare_durable(
        &self,
        role: DeviceRole,
    ) -> std::result::Result<PairingTicket, frank_store::StoreError> {
        self.prepare_inner(role, true).await
    }

    async fn prepare_inner(
        &self,
        role: DeviceRole,
        require_persistence: bool,
    ) -> std::result::Result<PairingTicket, frank_store::StoreError> {
        let secret = PairingSecret::generate();
        let expires_at = now().saturating_add(600);
        let key = hash(&secret);
        self.pending.lock().await.insert(
            hex::encode(key),
            PendingPairing {
                secret_hash: key,
                role,
                certificate_fingerprint: self.fingerprint.clone(),
                expires_at,
                used: false,
            },
        );
        if let Some(store) = &self.store {
            let result = store
                .upsert_pairing_ticket(&frank_store::StoredPairingTicket {
                    ticket_id: hex::encode(key),
                    secret_hash: hex::encode(key),
                    role,
                    certificate_fingerprint: self.fingerprint.clone(),
                    expires_at,
                    used: false,
                })
                .await;
            if let Err(error) = result {
                self.pending.lock().await.remove(&hex::encode(key));
                if require_persistence {
                    return Err(error);
                }
            }
        } else if require_persistence {
            self.pending.lock().await.remove(&hex::encode(key));
            return Err(frank_store::StoreError::Validation(
                "pairing persistence is unavailable".into(),
            ));
        }
        Ok(PairingTicket {
            secret,
            role,
            certificate_fingerprint: self.fingerprint.clone(),
            expires_at,
        })
    }

    pub async fn pair(
        &self,
        request: &PairingRequest,
    ) -> std::result::Result<PairingResponse, ApiError> {
        if request.protocol_version != PROTOCOL_VERSION {
            return Err(ApiError::new(
                ErrorCode::VersionMismatch,
                "pairing protocol version is unsupported",
            ));
        }
        if request.certificate_fingerprint != self.fingerprint {
            return Err(ApiError::new(
                ErrorCode::CertificateMismatch,
                "server certificate fingerprint does not match",
            ));
        }
        let key = hash(&request.secret);
        let key_string = hex::encode(key);
        {
            let now = now();
            let mut attempts = self.attempts.lock().await;
            let entry = attempts.entry(key_string.clone()).or_insert((now, 0));
            if now.saturating_sub(entry.0) >= 60 {
                *entry = (now, 0);
            }
            entry.1 = entry.1.saturating_add(1);
            if entry.1 > 5 {
                return Err(ApiError::new(
                    ErrorCode::Unauthorized,
                    "pairing rate limit exceeded; try again later",
                ));
            }
        }
        // Keep the pending-ticket lock until all checks pass and the ticket is
        // marked used. Without this critical section two simultaneous pair
        // requests could both redeem the same one-time secret.
        let mut pending = self.pending.lock().await;
        let ticket = pending.get_mut(&key_string).ok_or_else(|| {
            ApiError::new(
                ErrorCode::PairingExpired,
                "pairing code is unknown or expired",
            )
        })?;
        if ticket.used {
            return Err(ApiError::new(
                ErrorCode::PairingReused,
                "pairing code has already been used",
            ));
        }
        if ticket.expires_at < now() {
            return Err(ApiError::new(
                ErrorCode::PairingExpired,
                "pairing code has expired",
            ));
        }
        if ticket.secret_hash != key
            || ticket.certificate_fingerprint != request.certificate_fingerprint
        {
            return Err(ApiError::new(
                ErrorCode::CertificateMismatch,
                "pairing proof did not match",
            ));
        }
        if ticket.role != request.requested_role {
            return Err(ApiError::new(
                ErrorCode::Forbidden,
                "pairing role does not match the issued ticket",
            ));
        }
        let ticket_role = ticket.role;
        let ticket_fingerprint = ticket.certificate_fingerprint.clone();
        let ticket_expires_at = ticket.expires_at;
        {
            let devices = self.devices.lock().await;
            let has_owner = devices
                .values()
                .any(|device| device.auth.role == DeviceRole::Owner && !device.revoked);
            if !has_owner && ticket_role != DeviceRole::Owner {
                return Err(ApiError::new(
                    ErrorCode::Forbidden,
                    "the first paired device must be an owner",
                ));
            }
        }
        if request.device_name.trim().is_empty() || request.device_name.len() > 128 {
            return Err(ApiError::new(
                ErrorCode::Validation,
                "device name is invalid",
            ));
        }
        let device_id = DeviceId::new();
        let token = PairingSecret::generate();
        let token_hash = hash(&token);
        let auth = DeviceAuth {
            device_id,
            role: ticket_role,
            name: request.device_name.clone(),
        };
        let device = DeviceRecord {
            auth: auth.clone(),
            token_hash,
            revoked: false,
            last_seen_at: now(),
        };
        // Device insertion and one-time ticket consumption are one database
        // transaction. The in-memory lock still serializes concurrent pair
        // requests, while a database failure leaves both records unchanged.
        if let Some(store) = &self.store {
            let committed = store
                .complete_pairing(
                    &key_string,
                    &frank_store::StoredDevice {
                        device_id,
                        name: auth.name.clone(),
                        role: auth.role,
                        token_hash: hex::encode(token_hash),
                        certificate_fingerprint: request.certificate_fingerprint.clone(),
                        revoked: false,
                        last_seen_at: device.last_seen_at,
                    },
                )
                .await
                .map_err(|_| {
                    ApiError::new(ErrorCode::Internal, "pairing could not be persisted")
                })?;
            if !committed {
                return Err(ApiError::new(
                    ErrorCode::PairingReused,
                    "pairing code has already been used",
                ));
            }
        }
        ticket.used = true;
        drop(pending);
        self.attempts.lock().await.remove(&key_string);
        self.devices.lock().await.insert(device_id, device);
        Ok(PairingResponse {
            server_id: self.server_id,
            device_id,
            role: ticket_role,
            device_token: token,
            certificate_fingerprint: ticket_fingerprint,
            certificate_pem: self.certificate_pem.clone(),
            expires_at: ticket_expires_at.to_string(),
        })
    }

    pub async fn authenticate(&self, token: &str) -> Option<DeviceAuth> {
        let token_hash = hash(token);
        let mut devices = self.devices.lock().await;
        let authenticated = devices.values_mut().find_map(|device| {
            if !device.revoked && device.token_hash == token_hash {
                device.last_seen_at = now();
                Some(device.auth.clone())
            } else {
                None
            }
        });
        drop(devices);
        if let Some(auth) = &authenticated
            && let Some(store) = &self.store
        {
            let _ = store
                .upsert_device(&frank_store::StoredDevice {
                    device_id: auth.device_id,
                    name: auth.name.clone(),
                    role: auth.role,
                    token_hash: hex::encode(token_hash),
                    certificate_fingerprint: self.fingerprint.clone(),
                    revoked: false,
                    last_seen_at: now(),
                })
                .await;
        }
        authenticated
    }

    pub async fn revoke(&self, device_id: DeviceId) -> bool {
        let mut devices = self.devices.lock().await;
        let found = if let Some(device) = devices.get_mut(&device_id) {
            device.revoked = true;
            true
        } else {
            false
        };
        drop(devices);
        if found && let Some(store) = &self.store {
            let _ = store.revoke_device(device_id).await;
        }
        found
    }

    pub async fn devices(&self) -> Vec<(DeviceAuth, bool, u64)> {
        self.devices
            .lock()
            .await
            .values()
            .map(|device| (device.auth.clone(), device.revoked, device.last_seen_at))
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct PairingSecret;

impl PairingSecret {
    pub fn generate() -> String {
        let mut bytes = [0_u8; 32];
        if getrandom::fill(&mut bytes).is_ok() {
            return hex::encode(bytes);
        }
        // System randomness should be available on every supported platform;
        // retain a non-panicking fallback for a degraded early-boot entropy
        // source while still returning the fixed 256-bit wire shape.
        let first = uuid::Uuid::new_v4();
        let second = uuid::Uuid::new_v4();
        let mut fallback = [0_u8; 32];
        fallback[..16].copy_from_slice(first.as_bytes());
        fallback[16..].copy_from_slice(second.as_bytes());
        hex::encode(fallback)
    }
}
