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

mod artifacts;
mod auth;
mod commit;
mod events;
pub mod memory;
mod migrations;
mod operations_store;
mod pairing;
mod projection;
mod projection_store;
mod schema;
mod sessions;
mod snapshot_store;
mod terminal_store;
mod types;

pub use projection::ProjectionTable;
use projection::apply_projection_tx;
use schema::SCHEMA;
pub use types::*;

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

/// All inputs needed for one atomic command commit. Keeping this as a named
/// request prevents callers from accidentally swapping the actor/event/result
/// arguments as the commit surface evolves.
pub struct CommitRequest<'a> {
    pub command_id: CommandId,
    pub expected_revision: Option<u64>,
    pub actor: ActorRef,
    pub event: Event,
    pub snapshot: Snapshot,
    pub result: frank_protocol::CommandResult,
    pub artifact_bytes: Option<&'a [u8]>,
}

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
        // Authentication is the v4 trust-boundary migration. Existing
        // pairing credentials are intentionally invalidated once, while the
        // project/task snapshot remains untouched. New pairings are rejected
        // by frankd's HTTP surface and can never recreate an owner session.
        let auth_migration_applied = sqlx::query_scalar::<_, i64>(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = 4)",
        )
        .fetch_one(&pool)
        .await?
            != 0;
        if !auth_migration_applied {
            let mut migration = pool.begin().await?;
            sqlx::query("UPDATE devices SET revoked = 1")
                .execute(&mut *migration)
                .await?;
            sqlx::query("UPDATE pairing_tickets SET used = 1")
                .execute(&mut *migration)
                .await?;
            sqlx::query(
                "INSERT OR IGNORE INTO schema_migrations (version, applied_at) VALUES (4, ?)",
            )
            .bind(timestamp_now())
            .execute(&mut *migration)
            .await?;
            migration.commit().await?;
        }
        // Team/taskboard projections are additive. The CREATE IF NOT EXISTS
        // statements above make this safe for both fresh and pre-Team stores;
        // the marker lets diagnostics report that the migration has run.
        sqlx::query("INSERT OR IGNORE INTO schema_migrations (version, applied_at) VALUES (5, ?)")
            .bind(timestamp_now())
            .execute(&pool)
            .await?;
        // Organization v2 keeps TaskId as the stable card identity while
        // adding shared boards, pull offers, human-input waits, and explicit
        // relocation history. All rows are additive projections; the event
        // log and snapshot remain the recovery source of truth.
        sqlx::query("INSERT OR IGNORE INTO schema_migrations (version, applied_at) VALUES (8, ?)")
            .bind(timestamp_now())
            .execute(&pool)
            .await?;
        // Keep the one-owner invariant in a separate singleton slot. This is
        // additive for databases that already have the owner table and gives
        // concurrent first-run processes a durable claim point even when they
        // choose different usernames and owner IDs.
        sqlx::query(
            "INSERT OR IGNORE INTO owner_account_slot (slot, owner_id) SELECT 1, owner_id FROM owner_accounts ORDER BY created_at ASC, owner_id ASC LIMIT 1",
        )
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

    pub fn server_id(&self) -> ServerId {
        self.server_id
    }

    pub fn database_path(&self) -> Option<&Path> {
        self.database_path.as_deref()
    }
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
        AgentId, AgentPolicy, AgentTemplate, AgentView, ArtifactUploadSpec, ArtifactUploadView,
        ArtifactView, AvatarSpec, Budget, CommandResult, CorrelationId, Event, MissionStatus,
        MissionView, ModelSource, OperationId, OperationKind, OperationStatus, OperationView,
        ProjectId, ProjectView, RoleId, RoleView, TaskId, TaskStatus, TaskView, UsageView,
    };
    use frank_protocol::{DEFAULT_REWORK_LIMIT, WorkItemKind};

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
            .commit_request(CommitRequest {
                command_id: CommandId::new(),
                expected_revision: Some(0),
                actor: ActorRef::system(),
                event: Event::ArtifactPublished {
                    artifact: artifact.clone(),
                },
                snapshot,
                result: CommandResult::Created {
                    id: artifact.id.to_string(),
                },
                artifact_bytes: Some(b"hello"),
            })
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

    #[tokio::test]
    async fn provider_session_items_are_append_only_and_idempotent() {
        let store = Store::open_in_memory().await.unwrap();
        let first = store
            .append_provider_session_item(
                "openrouter-session",
                "user-1",
                &serde_json::json!({"message": {"role": "user", "content": "hello"}}),
            )
            .await
            .unwrap();
        let replay = store
            .append_provider_session_item(
                "openrouter-session",
                "user-1",
                &serde_json::json!({"message": {"role": "user", "content": "different"}}),
            )
            .await
            .unwrap();
        let second = store
            .append_provider_session_item(
                "openrouter-session",
                "assistant-1",
                &serde_json::json!({"message": {"role": "assistant", "content": "hi"}}),
            )
            .await
            .unwrap();
        assert_eq!(first, 1);
        assert_eq!(replay, first);
        assert_eq!(second, 2);
        let items = store
            .provider_session_items("openrouter-session")
            .await
            .unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].item_id, "user-1");
        assert_eq!(items[0].value["message"]["content"], "hello");
        assert_eq!(items[1].sequence, 2);
    }

    #[tokio::test]
    async fn openrouter_migration_preserves_work_and_ledger_but_resets_team_runtime() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("frank.sqlite3");
        let store = Store::open(&database).await.unwrap();
        let project_id = ProjectId::new();
        let mission_id = frank_protocol::MissionId::new();
        let task_id = TaskId::new();
        let role_id = RoleId::new();
        let agent_id = AgentId::new();
        let mut snapshot = store.snapshot().await.unwrap();
        snapshot.projects.push(ProjectView {
            id: project_id,
            name: "Keep me".into(),
            path: directory.path().to_string_lossy().into_owned(),
            base_branch: "main".into(),
            remote: None,
            check_commands: vec!["cargo test".into()],
            worktree_root: directory.path().to_string_lossy().into_owned(),
            push_policy: frank_protocol::PushPolicy::Disabled,
            pr_policy: frank_protocol::PrPolicy::Disabled,
            archived: false,
        });
        snapshot.missions.push(MissionView {
            id: mission_id,
            project_id,
            objective: "Keep this mission".into(),
            status: MissionStatus::Active,
            supervisor_session_id: Some("legacy-session".into()),
            branch: "frank/keep-me".into(),
            budget: Budget::unlimited(),
            created_at: timestamp_now(),
            updated_at: timestamp_now(),
        });
        snapshot.roles.push(RoleView {
            id: role_id,
            name: "Legacy role".into(),
            description: "Legacy".into(),
            template: AgentTemplate::Builder,
            model: Some("legacy/model".into()),
            pack_id: None,
            pack_level: None,
            instructions: "build".into(),
            policy: AgentPolicy::default(),
            budget: Budget::unlimited(),
            avatar: AvatarSpec {
                palette: "legacy".into(),
                seed: 1,
            },
            revision: 1,
            archived: false,
        });
        snapshot.agents.push(AgentView {
            id: agent_id,
            role_id: Some(role_id),
            role_revision: 1,
            display_name: "Legacy agent".into(),
            template: AgentTemplate::Builder,
            model: Some("legacy/model".into()),
            effective_model: Some("legacy/model".into()),
            model_source: ModelSource::Role,
            model_override: None,
            pending_model_override: None,
            pending_model_change: false,
            pack_id: None,
            pack_level: None,
            instructions: "build".into(),
            policy: AgentPolicy::default(),
            budget: Budget::unlimited(),
            avatar: AvatarSpec {
                palette: "legacy".into(),
                seed: 1,
            },
            status: frank_protocol::AgentStatus::Working,
            provider_session_id: Some("legacy-session".into()),
            last_claimed_at: None,
            archived: false,
        });
        snapshot.tasks.push(TaskView {
            id: task_id,
            mission_id,
            title: "Keep task".into(),
            objective: "Keep task data".into(),
            dependencies: Vec::new(),
            required_role_id: Some(role_id),
            priority: 0,
            budget: Budget::unlimited(),
            status: TaskStatus::Running,
            assigned_agent: Some(agent_id),
            reviewer_agent: None,
            claimed_at: Some(timestamp_now()),
            claim_source: Some(frank_protocol::TaskClaimSource::Automatic),
            attempt: 1,
            max_attempts: 2,
            worktree: Some(directory.path().to_string_lossy().into_owned()),
            branch: Some("frank/keep-me".into()),
            result_artifact: None,
            taskboard_id: None,
            workflow_id: None,
            parent_task_id: None,
            child_task_ids: Vec::new(),
            kind: WorkItemKind::Task,
            active_role_node_id: None,
            organization_revision: None,
            rework_limit: DEFAULT_REWORK_LIMIT,
            rework_count: 0,
        });
        snapshot.usage.push(UsageView {
            id: frank_protocol::AttemptId::new(),
            scope: frank_protocol::BudgetScope::Task,
            scope_id: task_id.to_string(),
            provider: frank_protocol::UsageProviderId("codex".into()),
            model: Some("legacy/model".into()),
            measured_input_tokens: Some(3),
            measured_output_tokens: Some(2),
            estimated_input_tokens: None,
            estimated_output_tokens: None,
            cost_micros: Some(7),
            cached_input_tokens: None,
            reasoning_tokens: None,
            recorded_at: timestamp_now(),
        });
        store.replace_snapshot(&snapshot).await.unwrap();
        sqlx::query("INSERT INTO provider_sessions (session_id, provider, agent_id, task_id, value_json, updated_at) VALUES (?, ?, ?, ?, ?, ?)")
            .bind("legacy-session")
            .bind("codex")
            .bind(agent_id.to_string())
            .bind(task_id.to_string())
            .bind("{}")
            .bind(timestamp_now())
            .execute(&store.pool)
            .await
            .unwrap();

        let backup = store.migrate_openrouter_runtime().await.unwrap();
        assert!(backup.as_ref().is_some_and(|path| path.is_file()));
        let migrated = store.snapshot().await.unwrap();
        assert_eq!(migrated.projects.len(), 1);
        assert_eq!(migrated.projects[0].name, "Keep me");
        assert_eq!(migrated.missions[0].objective, "Keep this mission");
        assert_eq!(migrated.usage.len(), 1);
        assert!(migrated.roles.is_empty());
        assert!(migrated.agents.is_empty());
        assert!(migrated.server.supervisor_model.is_none());
        let task = &migrated.tasks[0];
        assert_eq!(task.status, TaskStatus::Blocked);
        assert!(task.assigned_agent.is_none());
        assert!(task.required_role_id.is_none());
        assert!(task.claimed_at.is_none());
        assert!(migrated.task_feed.iter().any(|feed| {
            feed.task_id == task_id && feed.body == "Team runtime reset for OpenRouter"
        }));
        let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM provider_sessions")
            .fetch_one(&store.pool)
            .await
            .unwrap();
        assert_eq!(sessions, 0);
        assert!(store.migrate_openrouter_runtime().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn openrouter_snapshot_migration_is_raw_and_keeps_historical_usage() {
        let store = Store::open_in_memory().await.unwrap();
        let mut snapshot = store.snapshot().await.unwrap();
        snapshot.usage.push(UsageView {
            id: frank_protocol::AttemptId::new(),
            scope: frank_protocol::BudgetScope::Task,
            scope_id: "legacy-task".into(),
            provider: frank_protocol::UsageProviderId("codex".into()),
            model: Some("legacy-model".into()),
            measured_input_tokens: Some(3),
            measured_output_tokens: Some(2),
            estimated_input_tokens: None,
            estimated_output_tokens: None,
            cost_micros: None,
            cached_input_tokens: None,
            reasoning_tokens: None,
            recorded_at: timestamp_now(),
        });
        let mut raw = serde_json::to_value(&snapshot).unwrap();
        let server = raw
            .get_mut("server")
            .and_then(Value::as_object_mut)
            .unwrap();
        server.insert("provider".into(), Value::String("codex".into()));
        server.insert("supervisor_provider".into(), Value::String("claude".into()));
        server.insert("max_provider_concurrency".into(), Value::from(2));
        sqlx::query("UPDATE server_state SET snapshot_json = ? WHERE id = 1")
            .bind(serde_json::to_string(&raw).unwrap())
            .execute(&store.pool)
            .await
            .unwrap();

        assert!(store.migrate_openrouter_snapshot().await.unwrap().is_none());
        let canonical: Value = serde_json::from_str(
            &sqlx::query_scalar::<_, String>("SELECT snapshot_json FROM server_state WHERE id = 1")
                .fetch_one(&store.pool)
                .await
                .unwrap(),
        )
        .unwrap();
        let canonical_server = canonical.get("server").unwrap();
        assert!(canonical_server.get("provider").is_none());
        assert!(canonical_server.get("supervisor_provider").is_none());
        assert!(canonical_server.get("max_provider_concurrency").is_none());
        assert_eq!(canonical["usage"][0]["provider"], "codex");
        assert!(store.migrate_openrouter_snapshot().await.unwrap().is_none());
    }
}
