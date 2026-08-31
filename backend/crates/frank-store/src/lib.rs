//! SQLite/WAL persistence for Frank 1.0.
//!
//! SQLite is authoritative.  The JSONL exporter is deliberately a separate
//! append-only audit/recovery aid and is never read to reconstruct runtime
//! state.  Every event and its audit outbox row are committed in the same
//! transaction as the revision increment.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use frank_protocol::{
    ActorRef, ApiError, ArtifactId, ArtifactView, CommandId, CommandResponse, DeviceId, DeviceRole,
    Event, EventEnvelope, ServerId, Snapshot, Timestamp, timestamp_now,
};
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use thiserror::Error;

pub mod memory;

const SCHEMA: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS server_identity (id INTEGER PRIMARY KEY CHECK (id = 1), server_id TEXT NOT NULL, certificate_fingerprint TEXT NOT NULL, created_at TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS server_state (id INTEGER PRIMARY KEY CHECK (id = 1), revision INTEGER NOT NULL DEFAULT 0, event_seq INTEGER NOT NULL DEFAULT 0, snapshot_json TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS devices (device_id TEXT PRIMARY KEY, name TEXT NOT NULL, role TEXT NOT NULL, token_hash TEXT NOT NULL UNIQUE, certificate_fingerprint TEXT NOT NULL, created_at TEXT NOT NULL, last_seen_at TEXT NOT NULL, revoked INTEGER NOT NULL DEFAULT 0)",
    "CREATE TABLE IF NOT EXISTS pairing_tickets (ticket_id TEXT PRIMARY KEY, secret_hash TEXT NOT NULL UNIQUE, role TEXT NOT NULL, certificate_fingerprint TEXT NOT NULL, expires_at INTEGER NOT NULL, used INTEGER NOT NULL DEFAULT 0)",
    "CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value_json TEXT NOT NULL, revision INTEGER NOT NULL)",
    "CREATE TABLE IF NOT EXISTS projects (project_id TEXT PRIMARY KEY, value_json TEXT NOT NULL, archived INTEGER NOT NULL DEFAULT 0)",
    "CREATE TABLE IF NOT EXISTS agent_profiles (agent_id TEXT PRIMARY KEY, value_json TEXT NOT NULL, archived INTEGER NOT NULL DEFAULT 0)",
    "CREATE TABLE IF NOT EXISTS missions (mission_id TEXT PRIMARY KEY, value_json TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS tasks (task_id TEXT PRIMARY KEY, mission_id TEXT NOT NULL, value_json TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS task_dependencies (task_id TEXT NOT NULL, depends_on TEXT NOT NULL, PRIMARY KEY (task_id, depends_on))",
    "CREATE TABLE IF NOT EXISTS task_attempts (attempt_id TEXT PRIMARY KEY, task_id TEXT NOT NULL, value_json TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS provider_sessions (session_id TEXT PRIMARY KEY, provider TEXT NOT NULL, agent_id TEXT NOT NULL, task_id TEXT, value_json TEXT NOT NULL, updated_at TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS conversations (conversation_id TEXT PRIMARY KEY, mission_id TEXT NOT NULL, value_json TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS messages (message_id TEXT PRIMARY KEY, mission_id TEXT NOT NULL, value_json TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS approvals (approval_id TEXT PRIMARY KEY, task_id TEXT NOT NULL, value_json TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS artifacts (artifact_id TEXT PRIMARY KEY, mission_id TEXT NOT NULL, value_json TEXT NOT NULL, bytes BLOB NOT NULL, pinned INTEGER NOT NULL DEFAULT 0)",
    "CREATE TABLE IF NOT EXISTS usage_records (attempt_id TEXT PRIMARY KEY, scope_id TEXT NOT NULL, value_json TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS terminal_sessions (session_id TEXT PRIMARY KEY, task_id TEXT NOT NULL, value_json TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS control_leases (lease_id TEXT PRIMARY KEY, session_id TEXT NOT NULL UNIQUE, value_json TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS operations (operation_id TEXT PRIMARY KEY, kind TEXT NOT NULL, status TEXT NOT NULL, resource TEXT NOT NULL, phase TEXT NOT NULL, attempt INTEGER NOT NULL DEFAULT 0, error TEXT, value_json TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS artifact_uploads (upload_id TEXT PRIMARY KEY, mission_id TEXT NOT NULL, task_id TEXT, name TEXT NOT NULL, mime_type TEXT NOT NULL, expected_size INTEGER NOT NULL, expected_sha256 TEXT NOT NULL, received_size INTEGER NOT NULL DEFAULT 0, bytes BLOB NOT NULL, completed INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, expires_at TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS operation_locks (resource TEXT PRIMARY KEY, operation_id TEXT NOT NULL, acquired_at TEXT NOT NULL)",
    // Capability rows intentionally contain only a digest.  The bearer value
    // is held in frankd memory for the lifetime of a provider session and is
    // never recoverable from SQLite after a restart.
    "CREATE TABLE IF NOT EXISTS agent_capabilities (capability_hash TEXT PRIMARY KEY, agent_id TEXT NOT NULL, task_id TEXT NOT NULL, issued_at INTEGER NOT NULL, expires_at INTEGER NOT NULL, revoked INTEGER NOT NULL DEFAULT 0)",
    "CREATE INDEX IF NOT EXISTS agent_capabilities_expiry_idx ON agent_capabilities (expires_at, revoked)",
    "CREATE TABLE IF NOT EXISTS budget_clocks (scope_id TEXT PRIMARY KEY, started_at INTEGER NOT NULL, deadline INTEGER)",
    "CREATE TABLE IF NOT EXISTS terminal_transcript_chunks (session_id TEXT NOT NULL, sequence INTEGER NOT NULL, bytes BLOB NOT NULL, created_at TEXT NOT NULL, PRIMARY KEY (session_id, sequence))",
    "CREATE TABLE IF NOT EXISTS update_history (update_id TEXT PRIMARY KEY, version TEXT NOT NULL, target TEXT NOT NULL, state TEXT NOT NULL, error TEXT, changed_at TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS migration_backups (backup_id TEXT PRIMARY KEY, source_path TEXT NOT NULL, backup_path TEXT NOT NULL, schema_version INTEGER NOT NULL, created_at TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS events (seq INTEGER PRIMARY KEY, revision INTEGER NOT NULL, occurred_at TEXT NOT NULL, actor_json TEXT NOT NULL, correlation_id TEXT, event_json TEXT NOT NULL)",
    "CREATE TABLE IF NOT EXISTS audit_outbox (seq INTEGER PRIMARY KEY, event_json TEXT NOT NULL, exported INTEGER NOT NULL DEFAULT 0)",
    "CREATE TABLE IF NOT EXISTS idempotency_commands (command_id TEXT PRIMARY KEY, response_json TEXT NOT NULL, event_json TEXT, created_at TEXT NOT NULL)",
    "CREATE INDEX IF NOT EXISTS events_occurred_at_idx ON events (occurred_at)",
    "CREATE INDEX IF NOT EXISTS audit_outbox_exported_idx ON audit_outbox (exported, seq)",
    "CREATE INDEX IF NOT EXISTS provider_sessions_task_idx ON provider_sessions (task_id)",
    "CREATE INDEX IF NOT EXISTS conversations_mission_idx ON conversations (mission_id)",
    "CREATE INDEX IF NOT EXISTS operations_status_idx ON operations (status, updated_at)",
    "CREATE INDEX IF NOT EXISTS artifact_uploads_expiry_idx ON artifact_uploads (completed, expires_at)",
];

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("safe audit IO error: {0}")]
    SafeIo(#[from] frank_safeio::SafeIoError),
    #[error("state revision is stale (current revision: {current})")]
    StaleRevision { current: u64 },
    #[error("stored snapshot is corrupt")]
    CorruptSnapshot,
    #[error("event sequence is older than the retained window")]
    ResyncRequired,
    #[error("validation failed: {0}")]
    Validation(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Debug, Clone)]
pub struct Store {
    pool: SqlitePool,
    database_path: Option<PathBuf>,
    server_id: ServerId,
}

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

impl Store {
    pub async fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            frank_safeio::ensure_dir(parent)?;
        }
        if let Ok(metadata) = std::fs::symlink_metadata(&path)
            && metadata.file_type().is_symlink()
        {
            return Err(StoreError::Validation(
                "database path may not be a symlink".into(),
            ));
        }
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await?;
        Self::initialize(pool, Some(path)).await
    }

    pub async fn open_in_memory() -> Result<Self> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await?;
        Self::initialize(pool, None).await
    }

    async fn initialize(pool: SqlitePool, database_path: Option<PathBuf>) -> Result<Self> {
        for statement in SCHEMA {
            sqlx::query(statement).execute(&pool).await?;
        }
        // v1 is a clean-break data root, but a daemon may have been stopped
        // during an early development boot after creating the table without
        // the durable event column.  This additive repair is safe to retry.
        let _ = sqlx::query("ALTER TABLE idempotency_commands ADD COLUMN event_json TEXT")
            .execute(&pool)
            .await;
        sqlx::query("INSERT OR IGNORE INTO schema_migrations (version, applied_at) VALUES (1, ?)")
            .bind(timestamp_now())
            .execute(&pool)
            .await?;
        // The v1 schema is intentionally embedded instead of relying on an
        // external migrations directory. Version 2 records the provider
        // session/conversation projections added to the clean-break schema;
        // all statements above are idempotent for databases created by an
        // earlier development build.
        sqlx::query("INSERT OR IGNORE INTO schema_migrations (version, applied_at) VALUES (2, ?)")
            .bind(timestamp_now())
            .execute(&pool)
            .await?;
        sqlx::query("INSERT OR IGNORE INTO schema_migrations (version, applied_at) VALUES (3, ?)")
            .bind(timestamp_now())
            .execute(&pool)
            .await?;
        // Locks are process-local coordination records. Any row left by a
        // crashed daemon must be reclaimed before the startup reconciler can
        // safely resume its durable operation.
        sqlx::query("DELETE FROM operation_locks")
            .execute(&pool)
            .await?;
        let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(&pool)
            .await?;
        if integrity.trim() != "ok" {
            return Err(StoreError::CorruptSnapshot);
        }
        let existing = sqlx::query("SELECT server_id FROM server_identity WHERE id = 1")
            .fetch_optional(&pool)
            .await?;
        let server_id = if let Some(row) = existing {
            let raw: String = row.try_get("server_id")?;
            ServerId::parse(&raw).map_err(|error| StoreError::Validation(error.to_string()))?
        } else {
            let server_id = ServerId::new();
            sqlx::query("INSERT INTO server_identity (id, server_id, certificate_fingerprint, created_at) VALUES (1, ?, '', ?)")
                .bind(server_id.to_string())
                .bind(timestamp_now())
                .execute(&pool)
                .await?;
            let snapshot = Snapshot::empty(server_id);
            sqlx::query("INSERT INTO server_state (id, revision, event_seq, snapshot_json) VALUES (1, 0, 0, ?)")
                .bind(serde_json::to_string(&snapshot)?)
                .execute(&pool)
                .await?;
            server_id
        };
        // A partially-created database from an interrupted first boot may
        // have identity but no state row.  Repair that row without touching
        // any 0.2.x files.
        let state_exists = sqlx::query("SELECT 1 FROM server_state WHERE id = 1")
            .fetch_optional(&pool)
            .await?
            .is_some();
        if !state_exists {
            let snapshot = Snapshot::empty(server_id);
            sqlx::query("INSERT INTO server_state (id, revision, event_seq, snapshot_json) VALUES (1, 0, 0, ?)")
                .bind(serde_json::to_string(&snapshot)?)
                .execute(&pool)
                .await?;
        }
        Ok(Self {
            pool,
            database_path,
            server_id,
        })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn server_id(&self) -> ServerId {
        self.server_id
    }

    pub fn database_path(&self) -> Option<&Path> {
        self.database_path.as_deref()
    }

    /// Run SQLite's built-in integrity checker and fail closed on anything
    /// other than the exact `ok` result.  Callers can surface the diagnostic
    /// without replacing the original database file.
    pub async fn integrity_check(&self) -> Result<()> {
        let result: String = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(&self.pool)
            .await?;
        if result.trim() == "ok" {
            Ok(())
        } else {
            Err(StoreError::CorruptSnapshot)
        }
    }

    /// Create a consistent SQLite backup before a migration or update.  The
    /// destination must not already exist; refusing overwrite makes a failed
    /// backup impossible to mistake for a fresh recovery point.
    pub async fn backup_to(&self, destination: impl AsRef<Path>) -> Result<PathBuf> {
        self.integrity_check().await?;
        let destination = destination.as_ref().to_path_buf();
        if destination.exists() {
            return Err(StoreError::Validation(
                "backup destination already exists".into(),
            ));
        }
        if let Some(parent) = destination.parent() {
            frank_safeio::ensure_dir(parent)?;
        }
        if let Ok(metadata) = std::fs::symlink_metadata(&destination)
            && metadata.file_type().is_symlink()
        {
            return Err(StoreError::Validation(
                "backup destination may not be a symlink".into(),
            ));
        }
        // SQLite accepts a bound string expression for VACUUM INTO. This
        // copies WAL state atomically without relying on a raw filesystem
        // copy of the database plus an unrelated -wal file.
        let path = destination.to_string_lossy().to_string();
        sqlx::query("VACUUM INTO ?")
            .bind(path)
            .execute(&self.pool)
            .await?;
        Ok(destination)
    }

    pub async fn certificate_fingerprint(&self) -> Result<String> {
        let row = sqlx::query("SELECT certificate_fingerprint FROM server_identity WHERE id = 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.try_get("certificate_fingerprint")?)
    }

    pub async fn set_certificate_fingerprint(&self, fingerprint: &str) -> Result<()> {
        sqlx::query("UPDATE server_identity SET certificate_fingerprint = ? WHERE id = 1")
            .bind(fingerprint)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn devices(&self) -> Result<Vec<StoredDevice>> {
        let rows = sqlx::query(
            "SELECT device_id, name, role, token_hash, certificate_fingerprint, revoked, last_seen_at FROM devices",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut devices = Vec::with_capacity(rows.len());
        for row in rows {
            let device_id: String = row.try_get("device_id")?;
            let role: String = row.try_get("role")?;
            let role = match role.as_str() {
                "owner" => DeviceRole::Owner,
                "operator" => DeviceRole::Operator,
                "observer" => DeviceRole::Observer,
                _ => {
                    return Err(StoreError::Validation(
                        "stored device role is invalid".into(),
                    ));
                }
            };
            let revoked = row.try_get::<i64, _>("revoked")? != 0;
            let last_seen_at = row
                .try_get::<String, _>("last_seen_at")?
                .parse::<u64>()
                .unwrap_or_default();
            devices.push(StoredDevice {
                device_id: DeviceId::parse(&device_id)
                    .map_err(|error| StoreError::Validation(error.to_string()))?,
                name: row.try_get("name")?,
                role,
                token_hash: row.try_get("token_hash")?,
                certificate_fingerprint: row.try_get("certificate_fingerprint")?,
                revoked,
                last_seen_at,
            });
        }
        Ok(devices)
    }

    pub async fn upsert_device(&self, device: &StoredDevice) -> Result<()> {
        sqlx::query(
            "INSERT INTO devices (device_id, name, role, token_hash, certificate_fingerprint, created_at, last_seen_at, revoked) VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(device_id) DO UPDATE SET name = excluded.name, role = excluded.role, token_hash = excluded.token_hash, certificate_fingerprint = excluded.certificate_fingerprint, last_seen_at = excluded.last_seen_at, revoked = excluded.revoked",
        )
        .bind(device.device_id.to_string())
        .bind(&device.name)
        .bind(match device.role {
            DeviceRole::Owner => "owner",
            DeviceRole::Operator => "operator",
            DeviceRole::Observer => "observer",
        })
        .bind(&device.token_hash)
        .bind(&device.certificate_fingerprint)
        .bind(timestamp_now())
        .bind(device.last_seen_at.to_string())
        .bind(if device.revoked { 1_i64 } else { 0_i64 })
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn revoke_device(&self, device_id: DeviceId) -> Result<bool> {
        let result = sqlx::query("UPDATE devices SET revoked = 1 WHERE device_id = ?")
            .bind(device_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn upsert_pairing_ticket(&self, ticket: &StoredPairingTicket) -> Result<()> {
        sqlx::query(
            "INSERT INTO pairing_tickets (ticket_id, secret_hash, role, certificate_fingerprint, expires_at, used) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(ticket_id) DO UPDATE SET secret_hash = excluded.secret_hash, role = excluded.role, certificate_fingerprint = excluded.certificate_fingerprint, expires_at = excluded.expires_at, used = excluded.used",
        )
        .bind(&ticket.ticket_id)
        .bind(&ticket.secret_hash)
        .bind(match ticket.role {
            DeviceRole::Owner => "owner",
            DeviceRole::Operator => "operator",
            DeviceRole::Observer => "observer",
        })
        .bind(&ticket.certificate_fingerprint)
        .bind(ticket.expires_at as i64)
        .bind(if ticket.used { 1_i64 } else { 0_i64 })
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn pairing_tickets(&self) -> Result<Vec<StoredPairingTicket>> {
        let rows = sqlx::query("SELECT ticket_id, secret_hash, role, certificate_fingerprint, expires_at, used FROM pairing_tickets")
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|row| {
                let role: String = row.try_get("role")?;
                let role = match role.as_str() {
                    "owner" => DeviceRole::Owner,
                    "operator" => DeviceRole::Operator,
                    "observer" => DeviceRole::Observer,
                    _ => {
                        return Err(StoreError::Validation(
                            "stored pairing role is invalid".into(),
                        ));
                    }
                };
                Ok(StoredPairingTicket {
                    ticket_id: row.try_get("ticket_id")?,
                    secret_hash: row.try_get("secret_hash")?,
                    role,
                    certificate_fingerprint: row.try_get("certificate_fingerprint")?,
                    expires_at: row.try_get::<i64, _>("expires_at")?.max(0) as u64,
                    used: row.try_get::<i64, _>("used")? != 0,
                })
            })
            .collect()
    }

    pub async fn mark_pairing_ticket_used(&self, ticket_id: &str) -> Result<bool> {
        let result =
            sqlx::query("UPDATE pairing_tickets SET used = 1 WHERE ticket_id = ? AND used = 0")
                .bind(ticket_id)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Persist a newly paired device and consume its one-time ticket in one
    /// transaction. A crash or constraint failure therefore cannot leave an
    /// owner device durable while the ticket remains redeemable (or consume a
    /// ticket without the device token being stored).
    pub async fn complete_pairing(&self, ticket_id: &str, device: &StoredDevice) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO devices (device_id, name, role, token_hash, certificate_fingerprint, created_at, last_seen_at, revoked) VALUES (?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(device_id) DO UPDATE SET name = excluded.name, role = excluded.role, token_hash = excluded.token_hash, certificate_fingerprint = excluded.certificate_fingerprint, last_seen_at = excluded.last_seen_at, revoked = excluded.revoked",
        )
        .bind(device.device_id.to_string())
        .bind(&device.name)
        .bind(match device.role {
            DeviceRole::Owner => "owner",
            DeviceRole::Operator => "operator",
            DeviceRole::Observer => "observer",
        })
        .bind(&device.token_hash)
        .bind(&device.certificate_fingerprint)
        .bind(timestamp_now())
        .bind(device.last_seen_at.to_string())
        .bind(if device.revoked { 1_i64 } else { 0_i64 })
        .execute(&mut *tx)
        .await?;
        let consumed =
            sqlx::query("UPDATE pairing_tickets SET used = 1 WHERE ticket_id = ? AND used = 0")
                .bind(ticket_id)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                == 1;
        if !consumed {
            return Ok(false);
        }
        tx.commit().await?;
        Ok(true)
    }

    /// Persist capability issuance metadata without persisting the bearer
    /// token itself.  A daemon restart revokes the in-memory capability map;
    /// the row remains useful for audit/doctor output and expiry cleanup.
    pub async fn upsert_agent_capability(&self, capability: &StoredAgentCapability) -> Result<()> {
        if capability.capability_hash.trim().is_empty() {
            return Err(StoreError::Validation(
                "agent capability hash is empty".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO agent_capabilities (capability_hash, agent_id, task_id, issued_at, expires_at, revoked) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(capability_hash) DO UPDATE SET agent_id = excluded.agent_id, task_id = excluded.task_id, issued_at = excluded.issued_at, expires_at = excluded.expires_at, revoked = excluded.revoked",
        )
        .bind(&capability.capability_hash)
        .bind(capability.agent_id.to_string())
        .bind(capability.task_id.to_string())
        .bind(capability.issued_at as i64)
        .bind(capability.expires_at as i64)
        .bind(if capability.revoked { 1_i64 } else { 0_i64 })
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn revoke_agent_capability(&self, capability_hash: &str) -> Result<bool> {
        let result = sqlx::query(
            "UPDATE agent_capabilities SET revoked = 1 WHERE capability_hash = ? AND revoked = 0",
        )
        .bind(capability_hash)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn prune_agent_capabilities(&self, now: u64) -> Result<u64> {
        let result =
            sqlx::query("DELETE FROM agent_capabilities WHERE expires_at < ? OR revoked = 1")
                .bind(now as i64)
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected())
    }

    /// Persist an idempotent daemon-owned side effect.  The operation row is
    /// deliberately independent from the event stream: a process may crash
    /// after committing the command and before touching Git, a provider, or a
    /// service descriptor.  Startup reconciliation can therefore inspect this
    /// row and resume the effect without replaying the user command.
    pub async fn upsert_operation(&self, operation: &frank_protocol::OperationView) -> Result<()> {
        let value_json = serde_json::to_string(operation)?;
        let kind = serde_json::to_value(operation.kind)?
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        let status = serde_json::to_value(operation.status)?
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        sqlx::query(
            "INSERT INTO operations (operation_id, kind, status, resource, phase, attempt, error, value_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(operation_id) DO UPDATE SET kind = excluded.kind, status = excluded.status, resource = excluded.resource, phase = excluded.phase, attempt = excluded.attempt, error = excluded.error, value_json = excluded.value_json, updated_at = excluded.updated_at",
        )
        .bind(operation.id.to_string())
        .bind(kind)
        .bind(status)
        .bind(&operation.resource)
        .bind(&operation.phase)
        .bind(i64::from(operation.attempt))
        .bind(&operation.error)
        .bind(value_json)
        .bind(&operation.created_at)
        .bind(&operation.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn operation(
        &self,
        operation_id: frank_protocol::OperationId,
    ) -> Result<Option<frank_protocol::OperationView>> {
        let row = sqlx::query("SELECT value_json FROM operations WHERE operation_id = ?")
            .bind(operation_id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        row.map(|row| {
            let value: String = row.try_get("value_json")?;
            Ok(serde_json::from_str(&value)?)
        })
        .transpose()
    }

    pub async fn operations(&self) -> Result<Vec<frank_protocol::OperationView>> {
        let rows = sqlx::query(
            "SELECT value_json FROM operations ORDER BY created_at ASC, operation_id ASC",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let value: String = row.try_get("value_json")?;
                Ok(serde_json::from_str(&value)?)
            })
            .collect()
    }

    /// Number of audit rows that have been committed but not exported yet.
    /// This is intentionally a count rather than the event payload so the
    /// owner-facing diagnostics endpoint cannot accidentally expose secrets.
    pub async fn audit_backlog_count(&self) -> Result<u64> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_outbox WHERE exported = 0")
            .fetch_one(&self.pool)
            .await?;
        Ok(count.max(0) as u64)
    }

    /// Acquire a per-resource operation lock atomically.  The lock is a
    /// recovery guard, not the source of truth: callers still inspect the
    /// operation row and release the lock after a terminal state.
    pub async fn acquire_operation_lock(
        &self,
        resource: &str,
        operation_id: frank_protocol::OperationId,
    ) -> Result<bool> {
        if resource.trim().is_empty() || resource.len() > 4_096 {
            return Err(StoreError::Validation(
                "operation resource is invalid".into(),
            ));
        }
        let result = sqlx::query(
            "INSERT OR IGNORE INTO operation_locks (resource, operation_id, acquired_at) VALUES (?, ?, ?)",
        )
        .bind(resource)
        .bind(operation_id.to_string())
        .bind(timestamp_now())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn release_operation_lock(
        &self,
        resource: &str,
        operation_id: frank_protocol::OperationId,
    ) -> Result<bool> {
        let result =
            sqlx::query("DELETE FROM operation_locks WHERE resource = ? AND operation_id = ?")
                .bind(resource)
                .bind(operation_id.to_string())
                .execute(&self.pool)
                .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn upsert_budget_clock(
        &self,
        scope_id: &str,
        started_at: u64,
        deadline: Option<u64>,
    ) -> Result<()> {
        sqlx::query(
            "INSERT INTO budget_clocks (scope_id, started_at, deadline) VALUES (?, ?, ?) ON CONFLICT(scope_id) DO UPDATE SET started_at = excluded.started_at, deadline = excluded.deadline",
        )
        .bind(scope_id)
        .bind(started_at as i64)
        .bind(deadline.map(|value| value as i64))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn budget_clock(&self, scope_id: &str) -> Result<Option<(u64, Option<u64>)>> {
        let row = sqlx::query("SELECT started_at, deadline FROM budget_clocks WHERE scope_id = ?")
            .bind(scope_id)
            .fetch_optional(&self.pool)
            .await?;
        row.map(|row| {
            let started_at = row.try_get::<i64, _>("started_at")?.max(0) as u64;
            let deadline = row
                .try_get::<Option<i64>, _>("deadline")?
                .map(|value| value.max(0) as u64);
            Ok((started_at, deadline))
        })
        .transpose()
    }

    /// Append bounded terminal output. Raw input is deliberately never
    /// persisted; only output bytes needed for reconnect replay are retained.
    pub async fn append_terminal_transcript(
        &self,
        session_id: frank_protocol::TerminalSessionId,
        sequence: frank_protocol::TerminalSequence,
        bytes: &[u8],
    ) -> Result<()> {
        if bytes.len() > frank_protocol::MAX_TERMINAL_FRAME_BYTES {
            return Err(StoreError::Validation(
                "terminal transcript frame is too large".into(),
            ));
        }
        sqlx::query(
            "INSERT OR IGNORE INTO terminal_transcript_chunks (session_id, sequence, bytes, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(session_id.to_string())
        .bind(sequence.0 as i64)
        .bind(bytes)
        .bind(timestamp_now())
        .execute(&self.pool)
        .await?;
        // Keep at most 10 MiB per terminal. Delete whole chunks from the
        // oldest end so replay never begins with a partial UTF-8/ANSI frame.
        loop {
            let total: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(length(bytes)), 0) FROM terminal_transcript_chunks WHERE session_id = ?",
            )
            .bind(session_id.to_string())
            .fetch_one(&self.pool)
            .await?;
            if total <= 10 * 1024 * 1024 {
                break;
            }
            let oldest: Option<i64> = sqlx::query_scalar(
                "SELECT sequence FROM terminal_transcript_chunks WHERE session_id = ? ORDER BY sequence ASC LIMIT 1",
            )
            .bind(session_id.to_string())
            .fetch_optional(&self.pool)
            .await?;
            let Some(oldest) = oldest else { break };
            sqlx::query(
                "DELETE FROM terminal_transcript_chunks WHERE session_id = ? AND sequence = ?",
            )
            .bind(session_id.to_string())
            .bind(oldest)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    /// Append output using the next sequence allocated by SQLite. This is the
    /// multi-viewer safe variant: two terminal WebSockets cannot each invent
    /// the same local sequence after a reconnect. The write transaction is
    /// the serialization point and the returned sequence is the one sent to
    /// the client.
    pub async fn append_terminal_transcript_next(
        &self,
        session_id: frank_protocol::TerminalSessionId,
        bytes: &[u8],
    ) -> Result<frank_protocol::TerminalSequence> {
        if bytes.len() > frank_protocol::MAX_TERMINAL_FRAME_BYTES {
            return Err(StoreError::Validation(
                "terminal transcript frame is too large".into(),
            ));
        }
        let mut tx = self.pool.begin().await?;
        let latest: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) FROM terminal_transcript_chunks WHERE session_id = ?",
        )
        .bind(session_id.to_string())
        .fetch_one(&mut *tx)
        .await?;
        let sequence = latest.max(0) as u64 + 1;
        sqlx::query(
            "INSERT INTO terminal_transcript_chunks (session_id, sequence, bytes, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(session_id.to_string())
        .bind(sequence as i64)
        .bind(bytes)
        .bind(timestamp_now())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.trim_terminal_transcript(session_id).await?;
        Ok(frank_protocol::TerminalSequence(sequence))
    }

    async fn trim_terminal_transcript(
        &self,
        session_id: frank_protocol::TerminalSessionId,
    ) -> Result<()> {
        // Keep at most 10 MiB per terminal. Delete whole chunks from the
        // oldest end so replay never begins with a partial UTF-8/ANSI frame.
        loop {
            let total: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(length(bytes)), 0) FROM terminal_transcript_chunks WHERE session_id = ?",
            )
            .bind(session_id.to_string())
            .fetch_one(&self.pool)
            .await?;
            if total <= 10 * 1024 * 1024 {
                break;
            }
            let oldest: Option<i64> = sqlx::query_scalar(
                "SELECT sequence FROM terminal_transcript_chunks WHERE session_id = ? ORDER BY sequence ASC LIMIT 1",
            )
            .bind(session_id.to_string())
            .fetch_optional(&self.pool)
            .await?;
            let Some(oldest) = oldest else { break };
            sqlx::query(
                "DELETE FROM terminal_transcript_chunks WHERE session_id = ? AND sequence = ?",
            )
            .bind(session_id.to_string())
            .bind(oldest)
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }

    pub async fn terminal_replay(
        &self,
        session_id: frank_protocol::TerminalSessionId,
        after: Option<frank_protocol::TerminalSequence>,
        limit: u32,
    ) -> Result<(
        Vec<(frank_protocol::TerminalSequence, Vec<u8>)>,
        Option<frank_protocol::TerminalSequence>,
        frank_protocol::TerminalSequence,
    )> {
        let oldest: Option<i64> = sqlx::query_scalar(
            "SELECT MIN(sequence) FROM terminal_transcript_chunks WHERE session_id = ?",
        )
        .bind(session_id.to_string())
        .fetch_one(&self.pool)
        .await?;
        let latest: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(sequence) FROM terminal_transcript_chunks WHERE session_id = ?",
        )
        .bind(session_id.to_string())
        .fetch_one(&self.pool)
        .await?;
        let latest = frank_protocol::TerminalSequence(latest.unwrap_or_default().max(0) as u64);
        let after_value = after.map(|sequence| sequence.0 as i64).unwrap_or(0);
        let rows = sqlx::query(
            "SELECT sequence, bytes FROM terminal_transcript_chunks WHERE session_id = ? AND sequence > ? ORDER BY sequence ASC LIMIT ?",
        )
        .bind(session_id.to_string())
        .bind(after_value)
        .bind(limit.max(1) as i64)
        .fetch_all(&self.pool)
        .await?;
        let chunks = rows
            .into_iter()
            .map(|row| {
                Ok((
                    frank_protocol::TerminalSequence(
                        row.try_get::<i64, _>("sequence")?.max(0) as u64
                    ),
                    row.try_get::<Vec<u8>, _>("bytes")?,
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok((
            chunks,
            oldest.map(|sequence| frank_protocol::TerminalSequence(sequence.max(0) as u64)),
            latest,
        ))
    }

    pub async fn upsert_artifact_upload(
        &self,
        upload: &frank_protocol::ArtifactUploadView,
    ) -> Result<()> {
        if upload.spec.size > frank_protocol::MAX_ARTIFACT_BYTES {
            return Err(StoreError::Validation(
                "artifact upload exceeds the configured size cap".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO artifact_uploads (upload_id, mission_id, task_id, name, mime_type, expected_size, expected_sha256, received_size, bytes, completed, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, X'', ?, ?, ?) ON CONFLICT(upload_id) DO UPDATE SET received_size = excluded.received_size, completed = excluded.completed, expires_at = excluded.expires_at",
        )
        .bind(upload.id.to_string())
        .bind(upload.spec.mission_id.to_string())
        .bind(upload.spec.task_id.map(|id| id.to_string()))
        .bind(&upload.spec.name)
        .bind(&upload.spec.mime_type)
        .bind(upload.spec.size as i64)
        .bind(&upload.spec.sha256)
        .bind(upload.received as i64)
        .bind(if upload.completed { 1_i64 } else { 0_i64 })
        .bind(&upload.created_at)
        .bind(&upload.expires_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Append one contiguous upload chunk.  The offset is checked inside the
    /// transaction so two clients cannot overwrite or reorder artifact bytes.
    pub async fn append_artifact_upload_chunk(
        &self,
        upload_id: frank_protocol::UploadId,
        offset: u64,
        bytes: &[u8],
    ) -> Result<u64> {
        if bytes.len() as u64 > frank_protocol::MAX_TERMINAL_FRAME_BYTES as u64 * 4 {
            return Err(StoreError::Validation("artifact chunk is too large".into()));
        }
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT expected_size, received_size, completed, expires_at FROM artifact_uploads WHERE upload_id = ?",
        )
        .bind(upload_id.to_string())
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| StoreError::Validation("artifact upload not found".into()))?;
        let expected: i64 = row.try_get("expected_size")?;
        let received: i64 = row.try_get("received_size")?;
        let completed: i64 = row.try_get("completed")?;
        let expires_at: String = row.try_get("expires_at")?;
        if completed != 0 {
            return Err(StoreError::Validation(
                "artifact upload is already complete".into(),
            ));
        }
        let expires_at = expires_at.parse::<u64>().map_err(|_| {
            StoreError::Validation("artifact upload expiry metadata is invalid".into())
        })?;
        if expires_at <= unix_seconds() {
            return Err(StoreError::Validation("artifact upload has expired".into()));
        }
        if offset != received.max(0) as u64 {
            return Err(StoreError::Validation(
                "artifact chunk offset is not contiguous".into(),
            ));
        }
        let new_size = offset.saturating_add(bytes.len() as u64);
        if new_size > expected.max(0) as u64 || new_size > frank_protocol::MAX_ARTIFACT_BYTES {
            return Err(StoreError::Validation(
                "artifact upload exceeds its declared size".into(),
            ));
        }
        // Repeat the offset/completion predicate in the write itself.  The
        // read above is only a validation aid; another client may have
        // appended a chunk after that read but before this UPDATE obtains
        // SQLite's write lock.  A conditional update turns that race into a
        // deterministic validation failure instead of silently appending an
        // out-of-order or duplicate chunk.
        let updated = sqlx::query(
            "UPDATE artifact_uploads SET bytes = bytes || ?, received_size = ? WHERE upload_id = ? AND received_size = ? AND completed = 0",
        )
        .bind(bytes)
        .bind(new_size as i64)
        .bind(upload_id.to_string())
        .bind(received)
        .execute(&mut *tx)
        .await?;
        if updated.rows_affected() != 1 {
            return Err(StoreError::Validation(
                "artifact upload changed while the chunk was being appended".into(),
            ));
        }
        tx.commit().await?;
        Ok(new_size)
    }

    pub async fn artifact_upload_bytes(
        &self,
        upload_id: frank_protocol::UploadId,
    ) -> Result<Option<Vec<u8>>> {
        let row = sqlx::query("SELECT bytes FROM artifact_uploads WHERE upload_id = ?")
            .bind(upload_id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        row.map(|row| row.try_get("bytes").map_err(StoreError::from))
            .transpose()
    }

    pub async fn mark_artifact_upload_complete(
        &self,
        upload_id: frank_protocol::UploadId,
    ) -> Result<bool> {
        // Finalization is a state transition, so check the expiry in the
        // same write transaction as the conditional update.  The
        // orchestrator performs the equivalent validation before publishing
        // the artifact, but this store-level guard keeps direct callers and
        // recovery paths from completing an upload after its lease elapsed.
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            "SELECT expected_size, received_size, completed, expires_at FROM artifact_uploads WHERE upload_id = ?",
        )
        .bind(upload_id.to_string())
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = row else {
            return Ok(false);
        };
        let expected: i64 = row.try_get("expected_size")?;
        let received: i64 = row.try_get("received_size")?;
        let completed: i64 = row.try_get("completed")?;
        if completed != 0 || received != expected {
            return Ok(false);
        }
        let expires_at: String = row.try_get("expires_at")?;
        let expires_at = expires_at.parse::<u64>().map_err(|_| {
            StoreError::Validation("artifact upload expiry metadata is invalid".into())
        })?;
        if expires_at <= unix_seconds() {
            return Err(StoreError::Validation("artifact upload has expired".into()));
        }
        let result = sqlx::query(
            "UPDATE artifact_uploads SET completed = 1 WHERE upload_id = ? AND received_size = expected_size AND completed = 0 AND CAST(expires_at AS INTEGER) > ?",
        )
        .bind(upload_id.to_string())
        .bind(unix_seconds() as i64)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() == 1 {
            tx.commit().await?;
            Ok(true)
        } else {
            // A concurrent finalizer can win between the read and UPDATE.
            // There is no state to commit in that case, and returning false
            // preserves the method's idempotent boolean contract.
            Ok(false)
        }
    }

    pub async fn prune_expired_uploads(&self, cutoff: &Timestamp) -> Result<u64> {
        let cutoff = cutoff.parse::<u64>().map_err(|_| {
            StoreError::Validation("artifact upload expiry cutoff is invalid".into())
        })?;
        let result = sqlx::query(
            "DELETE FROM artifact_uploads WHERE completed = 0 AND CAST(expires_at AS INTEGER) <= ?",
        )
        .bind(cutoff as i64)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn current_revision(&self) -> Result<u64> {
        let row = sqlx::query("SELECT revision FROM server_state WHERE id = 1")
            .fetch_one(&self.pool)
            .await?;
        let revision: i64 = row.try_get("revision")?;
        Ok(revision.max(0) as u64)
    }

    pub async fn snapshot(&self) -> Result<Snapshot> {
        let row = sqlx::query("SELECT snapshot_json FROM server_state WHERE id = 1")
            .fetch_one(&self.pool)
            .await?;
        let json: String = row.try_get("snapshot_json")?;
        serde_json::from_str(&json).map_err(|_| StoreError::CorruptSnapshot)
    }

    /// Return a previously committed command response, if any.
    ///
    /// The command id is the durable idempotency key.  Callers that perform
    /// post-commit side effects (for example delivery or mission planning)
    /// must check this before reducing the command again; replaying the
    /// response from inside `commit_command` alone would still run those
    /// side effects a second time.
    pub async fn idempotent_response(
        &self,
        command_id: CommandId,
    ) -> Result<Option<CommandResponse>> {
        let row =
            sqlx::query("SELECT response_json FROM idempotency_commands WHERE command_id = ?")
                .bind(command_id.to_string())
                .fetch_optional(&self.pool)
                .await?;
        row.map(|row| {
            let json: String = row.try_get("response_json")?;
            Ok(serde_json::from_str(&json)?)
        })
        .transpose()
    }

    /// Update boot-time server metadata without creating a user-visible
    /// mutation event. Runtime changes must still use `commit_command`.
    pub async fn replace_snapshot(&self, snapshot: &Snapshot) -> Result<()> {
        sqlx::query(
            "UPDATE server_state SET snapshot_json = ?, revision = ?, event_seq = ? WHERE id = 1",
        )
        .bind(serde_json::to_string(snapshot)?)
        .bind(snapshot.revision as i64)
        .bind(snapshot.event_seq as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Commit one authoritative event, revision, audit row, snapshot, and
    /// idempotency response atomically.
    pub async fn commit_command(
        &self,
        command_id: CommandId,
        expected_revision: Option<u64>,
        actor: ActorRef,
        event: Event,
        snapshot: Snapshot,
        result: frank_protocol::CommandResult,
    ) -> Result<Commit> {
        self.commit_command_with_artifact(
            command_id,
            expected_revision,
            actor,
            event,
            snapshot,
            result,
            None,
        )
        .await
    }

    /// Commit a command and its artifact bytes in the same SQLite transaction.
    /// Keeping the payload write inside the command transaction means a
    /// successful `ArtifactPublished` event can never be observed without its
    /// corresponding bytes.
    #[allow(clippy::too_many_arguments)]
    pub async fn commit_command_with_artifact(
        &self,
        command_id: CommandId,
        expected_revision: Option<u64>,
        actor: ActorRef,
        event: Event,
        mut snapshot: Snapshot,
        result: frank_protocol::CommandResult,
        artifact_bytes: Option<&[u8]>,
    ) -> Result<Commit> {
        if let Some(bytes) = artifact_bytes {
            if bytes.len() as u64 > frank_protocol::MAX_ARTIFACT_BYTES {
                return Err(StoreError::Validation(
                    "artifact exceeds the configured size cap".into(),
                ));
            }
            if !matches!(&event, Event::ArtifactPublished { .. }) {
                return Err(StoreError::Validation(
                    "artifact bytes require an ArtifactPublished event".into(),
                ));
            }
        }
        let mut tx = self.pool.begin().await?;

        if let Some(row) = sqlx::query(
            "SELECT response_json, event_json FROM idempotency_commands WHERE command_id = ?",
        )
        .bind(command_id.to_string())
        .fetch_optional(&mut *tx)
        .await?
        {
            let raw: String = row.try_get("response_json")?;
            let response: CommandResponse = serde_json::from_str(&raw)?;
            let event = row
                .try_get::<Option<String>, _>("event_json")?
                .and_then(|raw| serde_json::from_str::<EventEnvelope>(&raw).ok())
                .unwrap_or_else(|| EventEnvelope {
                    seq: snapshot.event_seq,
                    occurred_at: timestamp_now(),
                    actor,
                    correlation_id: None,
                    event: Event::SnapshotReplaced {
                        snapshot: snapshot.clone(),
                    },
                });
            return Ok(Commit { response, event });
        }

        let state = sqlx::query("SELECT revision, event_seq FROM server_state WHERE id = 1")
            .fetch_one(&mut *tx)
            .await?;
        let current_revision: i64 = state.try_get("revision")?;
        let current_seq: i64 = state.try_get("event_seq")?;
        let current_revision = current_revision.max(0) as u64;
        let current_seq = current_seq.max(0) as u64;
        if expected_revision.is_some() && expected_revision != Some(current_revision) {
            return Err(StoreError::StaleRevision {
                current: current_revision,
            });
        }

        let revision = current_revision.saturating_add(1);
        let seq = current_seq.saturating_add(1);
        let occurred_at = timestamp_now();
        let envelope = EventEnvelope {
            seq,
            occurred_at: occurred_at.clone(),
            actor,
            correlation_id: Some(frank_protocol::CorrelationId::from(command_id.0)),
            event,
        };
        snapshot.revision = revision;
        snapshot.event_seq = seq;
        let event_json = serde_json::to_string(&envelope.event)?;
        let actor_json = serde_json::to_string(&envelope.actor)?;
        let snapshot_json = serde_json::to_string(&snapshot)?;
        sqlx::query(
            "UPDATE server_state SET revision = ?, event_seq = ?, snapshot_json = ? WHERE id = 1",
        )
        .bind(revision as i64)
        .bind(seq as i64)
        .bind(snapshot_json)
        .execute(&mut *tx)
        .await?;
        // Projection rows are part of the same transaction as the event and
        // snapshot.  The snapshot remains the canonical read model, but these
        // narrow tables make queries/indexes and future migrations durable
        // without leaving a committed event whose projection failed later.
        apply_projection_tx(&mut tx, &envelope.event, &snapshot).await?;
        sqlx::query("INSERT INTO events (seq, revision, occurred_at, actor_json, event_json) VALUES (?, ?, ?, ?, ?)")
            .bind(seq as i64)
            .bind(revision as i64)
            .bind(&occurred_at)
            .bind(actor_json)
            .bind(&event_json)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO audit_outbox (seq, event_json, exported) VALUES (?, ?, 0)")
            .bind(seq as i64)
            .bind(serde_json::to_string(&envelope)?)
            .execute(&mut *tx)
            .await?;
        let response = CommandResponse::ok(command_id, revision, result);
        sqlx::query("INSERT INTO idempotency_commands (command_id, response_json, event_json, created_at) VALUES (?, ?, ?, ?)")
            .bind(command_id.to_string())
            .bind(serde_json::to_string(&response)?)
            .bind(serde_json::to_string(&envelope)?)
            .bind(&occurred_at)
            .execute(&mut *tx)
            .await?;
        if let Some(bytes) = artifact_bytes {
            let Event::ArtifactPublished { artifact } = &envelope.event else {
                unreachable!("artifact event was validated before opening the transaction");
            };
            let updated = sqlx::query("UPDATE artifacts SET bytes = ? WHERE artifact_id = ?")
                .bind(bytes)
                .bind(artifact.id.to_string())
                .execute(&mut *tx)
                .await?;
            if updated.rows_affected() != 1 {
                return Err(StoreError::Validation(
                    "published artifact projection was not created".into(),
                ));
            }
        }
        tx.commit().await?;
        Ok(Commit {
            response,
            event: envelope,
        })
    }

    pub async fn events_after(&self, after: u64, limit: u32) -> Result<EventPage> {
        let latest_row = sqlx::query("SELECT event_seq FROM server_state WHERE id = 1")
            .fetch_one(&self.pool)
            .await?;
        let latest_seq = (latest_row.try_get::<i64, _>("event_seq")?).max(0) as u64;
        let oldest = sqlx::query("SELECT MIN(seq) AS oldest FROM events")
            .fetch_one(&self.pool)
            .await?;
        let oldest_seq = oldest
            .try_get::<Option<i64>, _>("oldest")?
            .map(|value| value.max(0) as u64);
        let resync_required = match oldest_seq {
            Some(first) => after.saturating_add(1) < first,
            // If retention removed every structured event while the server
            // still has a later snapshot sequence, a reconnecting client
            // cannot safely infer the missing history and must resync.
            None => latest_seq > after,
        };
        if resync_required {
            return Ok(EventPage {
                events: Vec::new(),
                oldest_seq,
                latest_seq,
                resync_required,
            });
        }
        let rows = sqlx::query("SELECT seq, occurred_at, actor_json, correlation_id, event_json FROM events WHERE seq > ? ORDER BY seq ASC LIMIT ?")
            .bind(after as i64)
            .bind(limit.max(1) as i64)
            .fetch_all(&self.pool)
            .await?;
        let mut events = Vec::with_capacity(rows.len());
        for row in rows {
            let seq: i64 = row.try_get("seq")?;
            let occurred_at: String = row.try_get("occurred_at")?;
            let actor_json: String = row.try_get("actor_json")?;
            let event_json: String = row.try_get("event_json")?;
            let correlation_id = row
                .try_get::<Option<String>, _>("correlation_id")?
                .and_then(|raw| frank_protocol::CorrelationId::parse(&raw).ok());
            events.push(EventEnvelope {
                seq: seq.max(0) as u64,
                occurred_at,
                actor: serde_json::from_str(&actor_json)?,
                correlation_id,
                event: serde_json::from_str(&event_json)?,
            });
        }
        Ok(EventPage {
            events,
            oldest_seq,
            latest_seq,
            resync_required: false,
        })
    }

    pub async fn export_audit_once(&self, path: impl AsRef<Path>, max_rows: u32) -> Result<u32> {
        let path = path.as_ref();
        let rows = sqlx::query(
            "SELECT seq, event_json FROM audit_outbox WHERE exported = 0 ORDER BY seq ASC LIMIT ?",
        )
        .bind(max_rows.max(1) as i64)
        .fetch_all(&self.pool)
        .await?;
        if rows.is_empty() {
            return Ok(0);
        }
        // The exporter can be interrupted after the append and before the
        // outbox update. Read existing sequence numbers first so a restart
        // marks those rows exported without duplicating audit lines.
        let already_written = frank_safeio::read_lines(path)
            .into_iter()
            .filter_map(|line| {
                serde_json::from_str::<EventEnvelope>(&line)
                    .ok()
                    .map(|event| event.seq)
            })
            .collect::<HashSet<_>>();
        let mut exported = Vec::with_capacity(rows.len());
        for row in &rows {
            let line: String = row.try_get("event_json")?;
            let seq = row.try_get::<i64, _>("seq")?;
            if !already_written.contains(&(seq.max(0) as u64)) {
                frank_safeio::append_line(path, &line)?;
            }
            exported.push(seq);
        }
        let mut tx = self.pool.begin().await?;
        for seq in exported {
            sqlx::query("UPDATE audit_outbox SET exported = 1 WHERE seq = ?")
                .bind(seq)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(rows.len() as u32)
    }

    pub async fn prune_events_before(&self, cutoff: &Timestamp) -> Result<u64> {
        let result = sqlx::query("DELETE FROM events WHERE occurred_at < ?")
            .bind(cutoff)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Remove terminal transcript chunks older than the configured replay
    /// window.  The byte cap is enforced at append time; this age-based pass
    /// is what keeps long-lived servers from retaining inactive shell output
    /// forever. Only whole chunks are deleted, so a remaining replay always
    /// starts at a valid ANSI frame boundary.
    pub async fn prune_terminal_transcript_before(&self, cutoff: &Timestamp) -> Result<u64> {
        let result = sqlx::query("DELETE FROM terminal_transcript_chunks WHERE created_at < ?")
            .bind(cutoff)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }

    /// Prune unpinned artifacts belonging to completed missions while keeping
    /// the authoritative snapshot in sync. Retention is maintenance rather
    /// than a user mutation, so it deliberately does not advance the event
    /// sequence; a reconnecting client receives the compacted state on its
    /// next snapshot/resync.
    pub async fn prune_artifacts_before(&self, cutoff: &Timestamp) -> Result<u64> {
        let snapshot = self.snapshot().await?;
        let completed_missions = snapshot
            .missions
            .iter()
            .filter(|mission| mission.status == frank_protocol::MissionStatus::Completed)
            .map(|mission| mission.id)
            .collect::<HashSet<_>>();
        let candidates = snapshot
            .artifacts
            .iter()
            .filter(|artifact| {
                completed_missions.contains(&artifact.mission_id)
                    && !artifact.pinned
                    && artifact.created_at < *cutoff
            })
            .map(|artifact| artifact.id)
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Ok(0);
        }
        let mut tx = self.pool.begin().await?;
        for artifact_id in &candidates {
            sqlx::query("DELETE FROM artifacts WHERE artifact_id = ? AND pinned = 0")
                .bind(artifact_id.to_string())
                .execute(&mut *tx)
                .await?;
        }
        let mut compacted = snapshot;
        compacted
            .artifacts
            .retain(|artifact| !candidates.contains(&artifact.id));
        sqlx::query("UPDATE server_state SET snapshot_json = ? WHERE id = 1")
            .bind(serde_json::to_string(&compacted)?)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(candidates.len() as u64)
    }

    /// Remove terminal durable operation rows after their retention window.
    /// Non-terminal rows are never deleted: startup reconciliation must still
    /// be able to resume or explicitly fail them after a crash.
    pub async fn prune_operations_before(&self, cutoff: &Timestamp) -> Result<u64> {
        let operations = self.operations().await?;
        let candidates = operations
            .iter()
            .filter(|operation| {
                operation.updated_at < *cutoff
                    && matches!(
                        operation.status,
                        frank_protocol::OperationStatus::Succeeded
                            | frank_protocol::OperationStatus::Failed
                            | frank_protocol::OperationStatus::Cancelled
                    )
            })
            .map(|operation| operation.id)
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Ok(0);
        }
        let mut snapshot = self.snapshot().await?;
        let mut tx = self.pool.begin().await?;
        for operation_id in &candidates {
            sqlx::query("DELETE FROM operations WHERE operation_id = ?")
                .bind(operation_id.to_string())
                .execute(&mut *tx)
                .await?;
        }
        snapshot
            .operations
            .retain(|operation| !candidates.contains(&operation.id));
        sqlx::query("UPDATE server_state SET snapshot_json = ? WHERE id = 1")
            .bind(serde_json::to_string(&snapshot)?)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(candidates.len() as u64)
    }

    pub async fn upsert_projection(
        &self,
        table: ProjectionTable,
        id: &str,
        value: &Value,
    ) -> Result<()> {
        let json = serde_json::to_string(value)?;
        match table {
            ProjectionTable::Project => {
                sqlx::query("INSERT INTO projects (project_id, value_json) VALUES (?, ?) ON CONFLICT(project_id) DO UPDATE SET value_json = excluded.value_json")
                    .bind(id).bind(json).execute(&self.pool).await?;
            }
            ProjectionTable::Agent => {
                sqlx::query("INSERT INTO agent_profiles (agent_id, value_json) VALUES (?, ?) ON CONFLICT(agent_id) DO UPDATE SET value_json = excluded.value_json")
                    .bind(id).bind(json).execute(&self.pool).await?;
            }
            ProjectionTable::Mission => {
                sqlx::query("INSERT INTO missions (mission_id, value_json) VALUES (?, ?) ON CONFLICT(mission_id) DO UPDATE SET value_json = excluded.value_json")
                    .bind(id).bind(json).execute(&self.pool).await?;
            }
            ProjectionTable::Task => {
                let mission_id =
                    value
                        .get("mission_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            StoreError::Validation("task projection is missing mission_id".into())
                        })?;
                sqlx::query("INSERT INTO tasks (task_id, mission_id, value_json) VALUES (?, ?, ?) ON CONFLICT(task_id) DO UPDATE SET mission_id = excluded.mission_id, value_json = excluded.value_json")
                    .bind(id).bind(mission_id).bind(json).execute(&self.pool).await?;
            }
            ProjectionTable::Message => {
                let mission_id =
                    value
                        .get("mission_id")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            StoreError::Validation(
                                "message projection is missing mission_id".into(),
                            )
                        })?;
                sqlx::query("INSERT INTO messages (message_id, mission_id, value_json) VALUES (?, ?, ?) ON CONFLICT(message_id) DO UPDATE SET mission_id = excluded.mission_id, value_json = excluded.value_json")
                    .bind(id).bind(mission_id).bind(json).execute(&self.pool).await?;
            }
            ProjectionTable::Approval => {
                let task_id = value
                    .get("task_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        StoreError::Validation("approval projection is missing task_id".into())
                    })?;
                sqlx::query("INSERT INTO approvals (approval_id, task_id, value_json) VALUES (?, ?, ?) ON CONFLICT(approval_id) DO UPDATE SET task_id = excluded.task_id, value_json = excluded.value_json")
                    .bind(id).bind(task_id).bind(json).execute(&self.pool).await?;
            }
        }
        Ok(())
    }

    pub async fn put_artifact(&self, artifact: &ArtifactView, bytes: &[u8]) -> Result<()> {
        if bytes.len() as u64 > frank_protocol::MAX_ARTIFACT_BYTES {
            return Err(StoreError::Validation(
                "artifact exceeds the configured size cap".into(),
            ));
        }
        sqlx::query("INSERT INTO artifacts (artifact_id, mission_id, value_json, bytes, pinned) VALUES (?, ?, ?, ?, ?) ON CONFLICT(artifact_id) DO UPDATE SET value_json = excluded.value_json, bytes = excluded.bytes, pinned = excluded.pinned")
            .bind(artifact.id.to_string())
            .bind(artifact.mission_id.to_string())
            .bind(serde_json::to_string(artifact)?)
            .bind(bytes)
            .bind(if artifact.pinned { 1_i64 } else { 0_i64 })
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn artifact_bytes(&self, id: ArtifactId) -> Result<Option<(String, Vec<u8>)>> {
        let row = sqlx::query("SELECT value_json, bytes FROM artifacts WHERE artifact_id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let value_json: String = row.try_get("value_json")?;
        let artifact: ArtifactView = serde_json::from_str(&value_json)?;
        let bytes: Vec<u8> = row.try_get("bytes")?;
        Ok(Some((artifact.mime_type, bytes)))
    }

    /// Read a bounded slice of an artifact without materializing the complete
    /// BLOB. SQLite's `substr` keeps the authoritative projection in one
    /// transaction-safe source while allowing the HTTP server to stream large
    /// artifacts with a fixed memory budget.
    pub async fn artifact_chunk(
        &self,
        id: ArtifactId,
        offset: u64,
        length: u64,
    ) -> Result<Option<Vec<u8>>> {
        if length == 0 || length > frank_protocol::MAX_TERMINAL_FRAME_BYTES as u64 * 4 {
            return Err(StoreError::Validation(
                "artifact chunk length is outside the configured limit".into(),
            ));
        }
        if offset > frank_protocol::MAX_ARTIFACT_BYTES {
            return Err(StoreError::Validation(
                "artifact chunk offset is outside the configured limit".into(),
            ));
        }
        let row =
            sqlx::query("SELECT substr(bytes, ?, ?) AS chunk FROM artifacts WHERE artifact_id = ?")
                .bind(offset.saturating_add(1) as i64)
                .bind(length as i64)
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(row.try_get("chunk")?))
    }

    /// Expose a transaction helper for server-side projection updates that
    /// need to accompany a command.  Callers must commit or roll back the
    /// returned transaction; this API is intentionally not used by clients.
    pub async fn begin(&self) -> Result<Transaction<'_, Sqlite>> {
        Ok(self.pool.begin().await?)
    }
}

async fn apply_projection_tx(
    tx: &mut Transaction<'_, Sqlite>,
    event: &frank_protocol::Event,
    snapshot: &Snapshot,
) -> Result<()> {
    use frank_protocol::Event;

    async fn upsert(
        tx: &mut Transaction<'_, Sqlite>,
        table: &str,
        id_column: &str,
        id: &str,
        value: Value,
    ) -> Result<()> {
        // Table and column names come only from the fixed match arms below;
        // values are always bound parameters.
        let sql = format!(
            "INSERT INTO {table} ({id_column}, value_json) VALUES (?, ?) ON CONFLICT({id_column}) DO UPDATE SET value_json = excluded.value_json"
        );
        sqlx::query(&sql)
            .bind(id)
            .bind(serde_json::to_string(&value)?)
            .execute(&mut **tx)
            .await?;
        Ok(())
    }

    match event {
        Event::SettingsChanged { settings } => {
            sqlx::query("INSERT INTO settings (key, value_json, revision) VALUES ('server', ?, ?) ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, revision = excluded.revision")
                .bind(serde_json::to_string(settings)?)
                .bind(snapshot.revision as i64)
                .execute(&mut **tx)
                .await?;
        }
        Event::ProjectUpserted { project } => {
            upsert(
                tx,
                "projects",
                "project_id",
                &project.id.to_string(),
                serde_json::to_value(project)?,
            )
            .await?;
        }
        Event::ProjectArchived { project_id } => {
            if let Some(project) = snapshot
                .projects
                .iter()
                .find(|project| project.id == *project_id)
            {
                upsert(
                    tx,
                    "projects",
                    "project_id",
                    &project.id.to_string(),
                    serde_json::to_value(project)?,
                )
                .await?;
            }
        }
        Event::AgentUpserted { agent } => {
            upsert(
                tx,
                "agent_profiles",
                "agent_id",
                &agent.id.to_string(),
                serde_json::to_value(agent)?,
            )
            .await?;
            upsert_provider_session_projection(tx, agent, snapshot).await?;
        }
        Event::AgentArchived { agent_id } | Event::AgentStatusChanged { agent_id, .. } => {
            if let Some(agent) = snapshot.agents.iter().find(|agent| agent.id == *agent_id) {
                upsert(
                    tx,
                    "agent_profiles",
                    "agent_id",
                    &agent.id.to_string(),
                    serde_json::to_value(agent)?,
                )
                .await?;
                upsert_provider_session_projection(tx, agent, snapshot).await?;
            }
        }
        Event::MissionCreated { mission } => {
            upsert(
                tx,
                "missions",
                "mission_id",
                &mission.id.to_string(),
                serde_json::to_value(mission)?,
            )
            .await?;
        }
        Event::MissionStatusChanged { mission_id, .. }
        | Event::MissionSupervisorSessionChanged { mission_id, .. }
        | Event::MissionBlocked { mission_id, .. } => {
            if let Some(mission) = snapshot
                .missions
                .iter()
                .find(|mission| mission.id == *mission_id)
            {
                upsert(
                    tx,
                    "missions",
                    "mission_id",
                    &mission.id.to_string(),
                    serde_json::to_value(mission)?,
                )
                .await?;
            }
        }
        Event::TaskCreated { task } | Event::TaskUpdated { task } => {
            upsert_task_projection(tx, task).await?;
        }
        Event::TaskStatusChanged { task_id, .. } | Event::TaskAssigned { task_id, .. } => {
            if let Some(task) = snapshot.tasks.iter().find(|task| task.id == *task_id) {
                upsert_task_projection(tx, task).await?;
            }
        }
        Event::TaskWorktreeProvisioning { task, operation } => {
            upsert_task_projection(tx, task).await?;
            let value_json = serde_json::to_string(operation)?;
            let kind = serde_json::to_value(operation.kind)?
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            let status = serde_json::to_value(operation.status)?
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            sqlx::query("INSERT INTO operations (operation_id, kind, status, resource, phase, attempt, error, value_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(operation_id) DO UPDATE SET kind = excluded.kind, status = excluded.status, resource = excluded.resource, phase = excluded.phase, attempt = excluded.attempt, error = excluded.error, value_json = excluded.value_json, updated_at = excluded.updated_at")
                .bind(operation.id.to_string())
                .bind(kind)
                .bind(status)
                .bind(&operation.resource)
                .bind(&operation.phase)
                .bind(i64::from(operation.attempt))
                .bind(&operation.error)
                .bind(value_json)
                .bind(&operation.created_at)
                .bind(&operation.updated_at)
                .execute(&mut **tx)
                .await?;
        }
        Event::MessageQueued { message } => {
            sqlx::query("INSERT INTO messages (message_id, mission_id, value_json) VALUES (?, ?, ?) ON CONFLICT(message_id) DO UPDATE SET mission_id = excluded.mission_id, value_json = excluded.value_json")
                .bind(message.id.to_string())
                .bind(message.mission_id.to_string())
                .bind(serde_json::to_string(message)?)
                .execute(&mut **tx)
                .await?;
        }
        Event::MessageDelivered { message_id }
        | Event::MessageAcknowledged { message_id }
        | Event::MessageCompleted { message_id }
        | Event::MessageFailed { message_id } => {
            if let Some(message) = snapshot
                .messages
                .iter()
                .find(|message| message.id == *message_id)
            {
                sqlx::query("INSERT INTO messages (message_id, mission_id, value_json) VALUES (?, ?, ?) ON CONFLICT(message_id) DO UPDATE SET mission_id = excluded.mission_id, value_json = excluded.value_json")
                    .bind(message.id.to_string())
                    .bind(message.mission_id.to_string())
                    .bind(serde_json::to_string(message)?)
                    .execute(&mut **tx)
                    .await?;
            }
        }
        Event::ApprovalRequested { approval } => {
            sqlx::query("INSERT INTO approvals (approval_id, task_id, value_json) VALUES (?, ?, ?) ON CONFLICT(approval_id) DO UPDATE SET task_id = excluded.task_id, value_json = excluded.value_json")
                .bind(approval.id.to_string())
                .bind(approval.task_id.to_string())
                .bind(serde_json::to_string(approval)?)
                .execute(&mut **tx)
                .await?;
        }
        Event::ApprovalDecided { approval_id, .. } | Event::ApprovalExpired { approval_id } => {
            if let Some(approval) = snapshot
                .approvals
                .iter()
                .find(|approval| approval.id == *approval_id)
            {
                sqlx::query("INSERT INTO approvals (approval_id, task_id, value_json) VALUES (?, ?, ?) ON CONFLICT(approval_id) DO UPDATE SET task_id = excluded.task_id, value_json = excluded.value_json")
                    .bind(approval.id.to_string())
                    .bind(approval.task_id.to_string())
                    .bind(serde_json::to_string(approval)?)
                    .execute(&mut **tx)
                    .await?;
            }
        }
        Event::ArtifactPublished { artifact } => {
            sqlx::query("INSERT INTO artifacts (artifact_id, mission_id, value_json, bytes, pinned) VALUES (?, ?, ?, X'', ?) ON CONFLICT(artifact_id) DO UPDATE SET value_json = excluded.value_json, pinned = excluded.pinned")
                .bind(artifact.id.to_string())
                .bind(artifact.mission_id.to_string())
                .bind(serde_json::to_string(artifact)?)
                .bind(if artifact.pinned { 1_i64 } else { 0_i64 })
                .execute(&mut **tx)
                .await?;
            if let Some(upload_id) = artifact.upload_id {
                // A streamed upload keeps its bytes in the upload row until
                // the final command commits. Copy those bytes into the
                // authoritative artifact row in this same transaction as the
                // publication event. Without this step a successful
                // FinalizeArtifactUpload would expose correct metadata but a
                // zero-byte download after reconnect/restart.
                let copied = sqlx::query(
                    "UPDATE artifacts SET bytes = (SELECT bytes FROM artifact_uploads WHERE upload_id = ?) WHERE artifact_id = ? AND EXISTS (SELECT 1 FROM artifact_uploads WHERE upload_id = ? AND completed = 0 AND received_size = expected_size AND CAST(expires_at AS INTEGER) > ?)",
                )
                .bind(upload_id.to_string())
                .bind(artifact.id.to_string())
                .bind(upload_id.to_string())
                .bind(unix_seconds() as i64)
                .execute(&mut **tx)
                .await?;
                if copied.rows_affected() != 1 {
                    return Err(StoreError::Validation(
                        "artifact upload bytes are unavailable or expired".into(),
                    ));
                }
                // The event projection is also guarded by the upload lease.
                // Normally the orchestrator validates expiry before emitting
                // `ArtifactPublished`, but replay/recovery must not turn a
                // stale upload into a completed one.
                sqlx::query("UPDATE artifact_uploads SET completed = 1 WHERE upload_id = ? AND received_size = expected_size AND CAST(expires_at AS INTEGER) > ?")
                    .bind(upload_id.to_string())
                    .bind(unix_seconds() as i64)
                    .execute(&mut **tx)
                    .await?;
            }
        }
        Event::TerminalOpened { session } => {
            upsert_terminal_projection(tx, session).await?;
        }
        Event::TerminalLeaseChanged { lease } => {
            if let Some(session) = snapshot
                .terminals
                .iter()
                .find(|session| session.id == lease.session_id)
            {
                upsert_terminal_projection(tx, session).await?;
            }
            if lease.expires_at == "released" {
                sqlx::query("DELETE FROM control_leases WHERE session_id = ?")
                    .bind(lease.session_id.to_string())
                    .execute(&mut **tx)
                    .await?;
            } else {
                sqlx::query("INSERT INTO control_leases (lease_id, session_id, value_json) VALUES (?, ?, ?) ON CONFLICT(session_id) DO UPDATE SET lease_id = excluded.lease_id, value_json = excluded.value_json")
                    .bind(&lease.lease_id)
                    .bind(lease.session_id.to_string())
                    .bind(serde_json::to_string(lease)?)
                    .execute(&mut **tx)
                    .await?;
            }
        }
        Event::TerminalClosed { session_id } => {
            if let Some(session) = snapshot
                .terminals
                .iter()
                .find(|session| session.id == *session_id)
            {
                upsert_terminal_projection(tx, session).await?;
            }
            sqlx::query("DELETE FROM control_leases WHERE session_id = ?")
                .bind(session_id.to_string())
                .execute(&mut **tx)
                .await?;
        }
        Event::UsageRecorded { usage } => {
            sqlx::query("INSERT INTO usage_records (attempt_id, scope_id, value_json) VALUES (?, ?, ?) ON CONFLICT(attempt_id) DO UPDATE SET scope_id = excluded.scope_id, value_json = excluded.value_json")
                .bind(usage.id.to_string())
                .bind(&usage.scope_id)
                .bind(serde_json::to_string(usage)?)
                .execute(&mut **tx)
                .await?;
        }
        Event::OperationChanged { operation } => {
            let value_json = serde_json::to_string(operation)?;
            let kind = serde_json::to_value(operation.kind)?
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            let status = serde_json::to_value(operation.status)?
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            sqlx::query("INSERT INTO operations (operation_id, kind, status, resource, phase, attempt, error, value_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(operation_id) DO UPDATE SET kind = excluded.kind, status = excluded.status, resource = excluded.resource, phase = excluded.phase, attempt = excluded.attempt, error = excluded.error, value_json = excluded.value_json, updated_at = excluded.updated_at")
                .bind(operation.id.to_string())
                .bind(kind)
                .bind(status)
                .bind(&operation.resource)
                .bind(&operation.phase)
                .bind(i64::from(operation.attempt))
                .bind(&operation.error)
                .bind(value_json)
                .bind(&operation.created_at)
                .bind(&operation.updated_at)
                .execute(&mut **tx)
                .await?;
        }
        Event::ArtifactUploadStarted { upload } => {
            sqlx::query("INSERT INTO artifact_uploads (upload_id, mission_id, task_id, name, mime_type, expected_size, expected_sha256, received_size, bytes, completed, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, X'', 0, ?, ?) ON CONFLICT(upload_id) DO UPDATE SET received_size = excluded.received_size, completed = excluded.completed, expires_at = excluded.expires_at")
                .bind(upload.id.to_string())
                .bind(upload.spec.mission_id.to_string())
                .bind(upload.spec.task_id.map(|id| id.to_string()))
                .bind(&upload.spec.name)
                .bind(&upload.spec.mime_type)
                .bind(upload.spec.size as i64)
                .bind(&upload.spec.sha256)
                .bind(upload.received as i64)
                .bind(&upload.created_at)
                .bind(&upload.expires_at)
                .execute(&mut **tx)
                .await?;
        }
        Event::UpdateStateChanged { update } => {
            let state = serde_json::to_value(update.state.clone())?
                .as_str()
                .unwrap_or("unknown")
                .to_string();
            sqlx::query("INSERT INTO update_history (update_id, version, target, state, error, changed_at) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(update_id) DO UPDATE SET version = excluded.version, target = excluded.target, state = excluded.state, error = excluded.error, changed_at = excluded.changed_at")
                .bind(update.id.to_string())
                .bind(&update.version)
                .bind(&update.target)
                .bind(state)
                .bind(&update.error)
                .bind(&update.checked_at)
                .execute(&mut **tx)
                .await?;
            // Apply/Rollback update commands carry a host operation in the
            // same snapshot. Keep the narrow operation projection in sync
            // even though the event envelope intentionally remains a single
            // UpdateStateChanged event for wire compatibility.
            for operation in snapshot
                .operations
                .iter()
                .filter(|operation| operation.kind == frank_protocol::OperationKind::HostUpdate)
            {
                let value_json = serde_json::to_string(operation)?;
                let kind = serde_json::to_value(operation.kind)?
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string();
                let status = serde_json::to_value(operation.status)?
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string();
                sqlx::query("INSERT INTO operations (operation_id, kind, status, resource, phase, attempt, error, value_json, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(operation_id) DO UPDATE SET kind = excluded.kind, status = excluded.status, resource = excluded.resource, phase = excluded.phase, attempt = excluded.attempt, error = excluded.error, value_json = excluded.value_json, updated_at = excluded.updated_at")
                    .bind(operation.id.to_string())
                    .bind(kind)
                    .bind(status)
                    .bind(&operation.resource)
                    .bind(&operation.phase)
                    .bind(i64::from(operation.attempt))
                    .bind(&operation.error)
                    .bind(value_json)
                    .bind(&operation.created_at)
                    .bind(&operation.updated_at)
                    .execute(&mut **tx)
                    .await?;
            }
        }
        Event::SnapshotReplaced { .. }
        | Event::BudgetPaused { .. }
        | Event::MemoryProposed { .. }
        | Event::MemoryRead { .. }
        | Event::DeliveryStarted { .. }
        | Event::DeliveryCompleted { .. }
        | Event::DeliveryBlocked { .. }
        | Event::SupervisorPlanProposed { .. } => {}
    }
    Ok(())
}

async fn upsert_task_projection(
    tx: &mut Transaction<'_, Sqlite>,
    task: &frank_protocol::TaskView,
) -> Result<()> {
    sqlx::query("INSERT INTO tasks (task_id, mission_id, value_json) VALUES (?, ?, ?) ON CONFLICT(task_id) DO UPDATE SET mission_id = excluded.mission_id, value_json = excluded.value_json")
        .bind(task.id.to_string())
        .bind(task.mission_id.to_string())
        .bind(serde_json::to_string(task)?)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM task_dependencies WHERE task_id = ?")
        .bind(task.id.to_string())
        .execute(&mut **tx)
        .await?;
    for dependency in &task.dependencies {
        sqlx::query("INSERT INTO task_dependencies (task_id, depends_on) VALUES (?, ?)")
            .bind(task.id.to_string())
            .bind(dependency.to_string())
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

async fn upsert_terminal_projection(
    tx: &mut Transaction<'_, Sqlite>,
    session: &frank_protocol::TerminalSessionView,
) -> Result<()> {
    sqlx::query("INSERT INTO terminal_sessions (session_id, task_id, value_json) VALUES (?, ?, ?) ON CONFLICT(session_id) DO UPDATE SET task_id = excluded.task_id, value_json = excluded.value_json")
        .bind(session.id.to_string())
        .bind(session.task_id.to_string())
        .bind(serde_json::to_string(session)?)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn upsert_provider_session_projection(
    tx: &mut Transaction<'_, Sqlite>,
    agent: &frank_protocol::AgentView,
    snapshot: &Snapshot,
) -> Result<()> {
    if let Some(session_id) = agent.provider_session_id.as_deref() {
        sqlx::query("DELETE FROM provider_sessions WHERE agent_id = ? AND session_id != ?")
            .bind(agent.id.to_string())
            .bind(session_id)
            .execute(&mut **tx)
            .await?;
        let task_id = snapshot
            .tasks
            .iter()
            .find(|task| {
                task.assigned_agent == Some(agent.id)
                    && task.status == frank_protocol::TaskStatus::Running
            })
            .map(|task| task.id.to_string());
        sqlx::query("INSERT INTO provider_sessions (session_id, provider, agent_id, task_id, value_json, updated_at) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(session_id) DO UPDATE SET provider = excluded.provider, agent_id = excluded.agent_id, task_id = excluded.task_id, value_json = excluded.value_json, updated_at = excluded.updated_at")
            .bind(session_id)
            .bind(agent.provider.to_string())
            .bind(agent.id.to_string())
            .bind(task_id)
            .bind(serde_json::to_string(agent)?)
            .bind(timestamp_now())
            .execute(&mut **tx)
            .await?;
    } else {
        sqlx::query("DELETE FROM provider_sessions WHERE agent_id = ?")
            .bind(agent.id.to_string())
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionTable {
    Project,
    Agent,
    Mission,
    Task,
    Message,
    Approval,
}

/// Map a storage error into the stable network error contract without
/// exposing SQL details or local filesystem paths.
pub fn api_error(error: &StoreError) -> ApiError {
    match error {
        StoreError::StaleRevision { .. } => ApiError::new(
            frank_protocol::ErrorCode::StaleRevision,
            "the server state changed; refresh and retry",
        ),
        StoreError::ResyncRequired => ApiError::new(
            frank_protocol::ErrorCode::ResyncRequired,
            "the event cursor is outside the retention window",
        ),
        StoreError::Validation(message) => {
            ApiError::new(frank_protocol::ErrorCode::Validation, message.clone())
        }
        _ => ApiError::new(
            frank_protocol::ErrorCode::Internal,
            "the server could not persist the request",
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use frank_protocol::{
        AgentId, ArtifactUploadSpec, ArtifactUploadView, ArtifactView, Budget, CommandResult,
        CorrelationId, Event, MissionStatus, MissionView, OperationId, OperationKind,
        OperationStatus, OperationView, Provider, TaskId, TaskStatus,
    };

    #[tokio::test]
    async fn wal_store_commits_event_snapshot_and_idempotency() {
        let store = Store::open_in_memory().await.unwrap();
        let command_id = CommandId::new();
        let snapshot = store.snapshot().await.unwrap();
        let commit = store
            .commit_command(
                command_id,
                Some(0),
                ActorRef::system(),
                Event::TaskStatusChanged {
                    task_id: frank_protocol::TaskId::nil(),
                    status: TaskStatus::Ready,
                },
                snapshot,
                CommandResult::Accepted,
            )
            .await
            .unwrap();
        assert_eq!(commit.response.revision, 1);
        assert_eq!(
            commit.event.correlation_id,
            Some(CorrelationId::from(command_id.0))
        );
        assert_eq!(store.current_revision().await.unwrap(), 1);
        assert_eq!(
            store.idempotent_response(command_id).await.unwrap(),
            Some(commit.response.clone())
        );
        let replay = store
            .commit_command(
                command_id,
                Some(0),
                ActorRef::system(),
                Event::SnapshotReplaced {
                    snapshot: store.snapshot().await.unwrap(),
                },
                store.snapshot().await.unwrap(),
                CommandResult::Accepted,
            )
            .await
            .unwrap();
        assert_eq!(replay.response, commit.response);
        assert_eq!(store.events_after(0, 50).await.unwrap().events.len(), 1);
    }

    #[tokio::test]
    async fn stale_revision_is_rejected_before_mutation() {
        let store = Store::open_in_memory().await.unwrap();
        let snapshot = store.snapshot().await.unwrap();
        store
            .commit_command(
                CommandId::new(),
                None,
                ActorRef::system(),
                Event::SnapshotReplaced {
                    snapshot: snapshot.clone(),
                },
                snapshot,
                CommandResult::Accepted,
            )
            .await
            .unwrap();
        let result = store
            .commit_command(
                CommandId::new(),
                Some(0),
                ActorRef::system(),
                Event::SnapshotReplaced {
                    snapshot: store.snapshot().await.unwrap(),
                },
                store.snapshot().await.unwrap(),
                CommandResult::Accepted,
            )
            .await;
        assert!(matches!(
            result,
            Err(StoreError::StaleRevision { current: 1 })
        ));
    }

    #[tokio::test]
    async fn device_persistence_round_trips_all_security_fields() {
        let store = Store::open_in_memory().await.unwrap();
        let device = StoredDevice {
            device_id: DeviceId::new(),
            name: "laptop".into(),
            role: DeviceRole::Operator,
            token_hash: "aa".repeat(32),
            certificate_fingerprint: "sha256:fingerprint".into(),
            revoked: false,
            last_seen_at: 42,
        };
        store.upsert_device(&device).await.unwrap();
        let devices = store.devices().await.unwrap();
        assert_eq!(devices, vec![device]);
    }

    #[tokio::test]
    async fn pairing_device_and_ticket_are_atomic() {
        let store = Store::open_in_memory().await.unwrap();
        let ticket = StoredPairingTicket {
            ticket_id: "ticket".into(),
            secret_hash: "bb".repeat(32),
            role: DeviceRole::Owner,
            certificate_fingerprint: "sha256:fingerprint".into(),
            expires_at: 100,
            used: false,
        };
        store.upsert_pairing_ticket(&ticket).await.unwrap();
        let device = StoredDevice {
            device_id: DeviceId::new(),
            name: "laptop".into(),
            role: DeviceRole::Owner,
            token_hash: "cc".repeat(32),
            certificate_fingerprint: ticket.certificate_fingerprint.clone(),
            revoked: false,
            last_seen_at: 1,
        };
        assert!(
            store
                .complete_pairing(&ticket.ticket_id, &device)
                .await
                .unwrap()
        );
        assert!(
            !store
                .complete_pairing(&ticket.ticket_id, &device)
                .await
                .unwrap()
        );
        assert_eq!(store.devices().await.unwrap(), vec![device]);
        assert!(store.pairing_tickets().await.unwrap()[0].used);
    }

    #[tokio::test]
    async fn agent_capability_metadata_never_requires_plaintext_token() {
        let store = Store::open_in_memory().await.unwrap();
        let capability = StoredAgentCapability {
            capability_hash: "sha256:capability".into(),
            agent_id: AgentId::new(),
            task_id: TaskId::new(),
            issued_at: 10,
            expires_at: 20,
            revoked: false,
        };
        store.upsert_agent_capability(&capability).await.unwrap();
        assert!(
            store
                .revoke_agent_capability(&capability.capability_hash)
                .await
                .unwrap()
        );
        assert_eq!(store.prune_agent_capabilities(20).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn artifact_event_and_bytes_commit_atomically() {
        let store = Store::open_in_memory().await.unwrap();
        let artifact = ArtifactView {
            id: ArtifactId::new(),
            mission_id: frank_protocol::MissionId::nil(),
            task_id: None,
            upload_id: None,
            name: "result.txt".into(),
            mime_type: "text/plain".into(),
            size: 5,
            sha256: String::new(),
            pinned: false,
            created_at: timestamp_now(),
        };
        let mut snapshot = store.snapshot().await.unwrap();
        snapshot.artifacts.push(artifact.clone());
        store
            .commit_command_with_artifact(
                CommandId::new(),
                Some(0),
                ActorRef::system(),
                Event::ArtifactPublished {
                    artifact: artifact.clone(),
                },
                snapshot,
                CommandResult::Created {
                    id: artifact.id.to_string(),
                },
                Some(b"hello"),
            )
            .await
            .unwrap();
        let stored = store.artifact_bytes(artifact.id).await.unwrap().unwrap();
        assert_eq!(stored.0, "text/plain");
        assert_eq!(stored.1, b"hello");
    }

    #[tokio::test]
    async fn artifact_chunks_are_bounded_and_reassemble_authoritative_bytes() {
        let store = Store::open_in_memory().await.unwrap();
        let artifact = ArtifactView {
            id: ArtifactId::new(),
            mission_id: frank_protocol::MissionId::nil(),
            task_id: None,
            upload_id: None,
            name: "chunked.txt".into(),
            mime_type: "text/plain".into(),
            size: 11,
            sha256: String::new(),
            pinned: false,
            created_at: timestamp_now(),
        };
        store.put_artifact(&artifact, b"hello world").await.unwrap();
        let first = store
            .artifact_chunk(artifact.id, 0, 5)
            .await
            .unwrap()
            .unwrap();
        let second = store
            .artifact_chunk(artifact.id, 5, 6)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first, b"hello");
        assert_eq!(second, b" world");
        assert!(matches!(
            store
                .artifact_chunk(
                    artifact.id,
                    0,
                    frank_protocol::MAX_TERMINAL_FRAME_BYTES as u64 * 4 + 1
                )
                .await,
            Err(StoreError::Validation(_))
        ));
    }

    #[tokio::test]
    async fn finalized_stream_upload_copies_bytes_into_artifact_projection() {
        let store = Store::open_in_memory().await.unwrap();
        let mission_id = frank_protocol::MissionId::new();
        let upload_id = frank_protocol::UploadId::new();
        let upload = ArtifactUploadView {
            id: upload_id,
            spec: ArtifactUploadSpec {
                mission_id,
                task_id: None,
                name: "streamed.txt".into(),
                mime_type: "text/plain".into(),
                size: 5,
                sha256: "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".into(),
            },
            received: 0,
            completed: false,
            created_at: timestamp_now(),
            // Keep the fixture well inside its lease even on a slow CI host.
            expires_at: (unix_seconds() + 600).to_string(),
        };
        store.upsert_artifact_upload(&upload).await.unwrap();
        assert_eq!(
            store
                .append_artifact_upload_chunk(upload_id, 0, b"hello")
                .await
                .unwrap(),
            5
        );

        let artifact = ArtifactView {
            id: ArtifactId::from(upload_id.0),
            mission_id,
            task_id: None,
            upload_id: Some(upload_id),
            name: upload.spec.name.clone(),
            mime_type: upload.spec.mime_type.clone(),
            size: upload.spec.size,
            sha256: upload.spec.sha256.clone(),
            pinned: false,
            created_at: timestamp_now(),
        };
        let mut snapshot = store.snapshot().await.unwrap();
        snapshot.uploads.push(ArtifactUploadView {
            received: 5,
            ..upload.clone()
        });
        snapshot.artifacts.push(artifact.clone());
        store
            .commit_command(
                CommandId::new(),
                Some(0),
                ActorRef::system(),
                Event::ArtifactPublished {
                    artifact: artifact.clone(),
                },
                snapshot,
                CommandResult::Created {
                    id: artifact.id.to_string(),
                },
            )
            .await
            .unwrap();

        let stored = store.artifact_bytes(artifact.id).await.unwrap().unwrap();
        assert_eq!(stored.0, "text/plain");
        assert_eq!(stored.1, b"hello");
    }

    #[tokio::test]
    async fn expired_artifact_upload_cannot_receive_new_chunks() {
        let store = Store::open_in_memory().await.unwrap();
        let upload = ArtifactUploadView {
            id: frank_protocol::UploadId::new(),
            spec: ArtifactUploadSpec {
                mission_id: frank_protocol::MissionId::new(),
                task_id: None,
                name: "expired.txt".into(),
                mime_type: "text/plain".into(),
                size: 1,
                sha256: "a".repeat(64),
            },
            received: 0,
            completed: false,
            created_at: "1".into(),
            expires_at: "1".into(),
        };
        store.upsert_artifact_upload(&upload).await.unwrap();
        assert!(matches!(
            store
                .append_artifact_upload_chunk(upload.id, 0, b"x")
                .await,
            Err(StoreError::Validation(message)) if message.contains("expired")
        ));
        assert_eq!(store.prune_expired_uploads(&"2".into()).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn expired_artifact_upload_cannot_be_finalized() {
        let store = Store::open_in_memory().await.unwrap();
        let upload = ArtifactUploadView {
            id: frank_protocol::UploadId::new(),
            spec: ArtifactUploadSpec {
                mission_id: frank_protocol::MissionId::new(),
                task_id: None,
                name: "expired-finalize.txt".into(),
                mime_type: "text/plain".into(),
                size: 1,
                sha256: "a".repeat(64),
            },
            // Simulate a fully received upload whose finalization was
            // interrupted until after its retention lease expired.
            received: 1,
            completed: false,
            created_at: "1".into(),
            expires_at: "1".into(),
        };
        store.upsert_artifact_upload(&upload).await.unwrap();
        assert!(matches!(
            store.mark_artifact_upload_complete(upload.id).await,
            Err(StoreError::Validation(message)) if message.contains("expired")
        ));
    }

    #[tokio::test]
    async fn operation_locks_and_budget_clocks_are_restart_safe() {
        let store = Store::open_in_memory().await.unwrap();
        let operation = frank_protocol::OperationId::new();
        assert!(
            store
                .acquire_operation_lock("mission:one", operation)
                .await
                .unwrap()
        );
        assert!(
            !store
                .acquire_operation_lock("mission:one", frank_protocol::OperationId::new())
                .await
                .unwrap()
        );
        assert!(
            store
                .release_operation_lock("mission:one", operation)
                .await
                .unwrap()
        );
        store
            .upsert_budget_clock("mission:one", 100, Some(200))
            .await
            .unwrap();
        assert_eq!(
            store.budget_clock("mission:one").await.unwrap(),
            Some((100, Some(200)))
        );
    }

    #[tokio::test]
    async fn terminal_transcript_replay_is_sequence_ordered() {
        let store = Store::open_in_memory().await.unwrap();
        let session = frank_protocol::TerminalSessionId::new();
        store
            .append_terminal_transcript(session, frank_protocol::TerminalSequence(1), b"one")
            .await
            .unwrap();
        store
            .append_terminal_transcript(session, frank_protocol::TerminalSequence(2), b"two")
            .await
            .unwrap();
        let replay = store
            .terminal_replay(session, Some(frank_protocol::TerminalSequence(1)), 10)
            .await
            .unwrap();
        assert_eq!(replay.0.len(), 1);
        assert_eq!(replay.0[0].0, frank_protocol::TerminalSequence(2));
        assert_eq!(replay.2, frank_protocol::TerminalSequence(2));
    }

    #[tokio::test]
    async fn backup_and_integrity_check_do_not_overwrite_destination() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("frank.sqlite3");
        let store = Store::open(&path).await.unwrap();
        store.integrity_check().await.unwrap();
        let backup = directory.path().join("backup.sqlite3");
        assert_eq!(store.backup_to(&backup).await.unwrap(), backup);
        assert!(backup.is_file());
        assert!(store.backup_to(&backup).await.is_err());
    }

    #[tokio::test]
    async fn retention_prunes_only_old_unpinned_completed_artifacts() {
        let store = Store::open_in_memory().await.unwrap();
        let mission_id = frank_protocol::MissionId::new();
        let old = ArtifactView {
            id: ArtifactId::new(),
            mission_id,
            task_id: None,
            upload_id: None,
            name: "old.txt".into(),
            mime_type: "text/plain".into(),
            size: 3,
            sha256: "a".repeat(64),
            pinned: false,
            created_at: "10".into(),
        };
        let pinned = ArtifactView {
            id: ArtifactId::new(),
            mission_id,
            task_id: None,
            upload_id: None,
            name: "pinned.txt".into(),
            mime_type: "text/plain".into(),
            size: 6,
            sha256: "b".repeat(64),
            pinned: true,
            created_at: "10".into(),
        };
        store.put_artifact(&old, b"old").await.unwrap();
        store.put_artifact(&pinned, b"pinned").await.unwrap();
        let mut snapshot = store.snapshot().await.unwrap();
        snapshot.missions.push(MissionView {
            id: mission_id,
            project_id: frank_protocol::ProjectId::new(),
            objective: "complete".into(),
            status: MissionStatus::Completed,
            supervisor_provider: Provider::Codex,
            supervisor_session_id: None,
            branch: "frank/mission-test".into(),
            budget: Budget::unlimited(),
            created_at: "1".into(),
            updated_at: "2".into(),
        });
        snapshot.artifacts.extend([old.clone(), pinned.clone()]);
        store.replace_snapshot(&snapshot).await.unwrap();

        assert_eq!(store.prune_artifacts_before(&"20".into()).await.unwrap(), 1);
        assert!(store.artifact_bytes(old.id).await.unwrap().is_none());
        assert!(store.artifact_bytes(pinned.id).await.unwrap().is_some());
        let remaining = store.snapshot().await.unwrap().artifacts;
        assert_eq!(remaining, vec![pinned]);
    }

    #[tokio::test]
    async fn retention_keeps_non_terminal_operations_for_recovery() {
        let store = Store::open_in_memory().await.unwrap();
        let finished = OperationView {
            id: OperationId::new(),
            kind: OperationKind::RunChecks,
            status: OperationStatus::Succeeded,
            resource: "task".into(),
            phase: "done".into(),
            attempt: 1,
            error: None,
            created_at: "10".into(),
            updated_at: "10".into(),
        };
        let recovering = OperationView {
            id: OperationId::new(),
            kind: OperationKind::RunChecks,
            status: OperationStatus::Recovering,
            resource: "task-2".into(),
            phase: "resume".into(),
            attempt: 1,
            error: None,
            created_at: "10".into(),
            updated_at: "10".into(),
        };
        store.upsert_operation(&finished).await.unwrap();
        store.upsert_operation(&recovering).await.unwrap();
        let mut snapshot = store.snapshot().await.unwrap();
        snapshot
            .operations
            .extend([finished.clone(), recovering.clone()]);
        store.replace_snapshot(&snapshot).await.unwrap();

        assert_eq!(
            store.prune_operations_before(&"20".into()).await.unwrap(),
            1
        );
        assert!(store.operation(finished.id).await.unwrap().is_none());
        assert!(store.operation(recovering.id).await.unwrap().is_some());
        assert_eq!(store.snapshot().await.unwrap().operations, vec![recovering]);
    }
}
