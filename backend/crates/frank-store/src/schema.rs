//! The SQLite schema, applied in order on open.
//!
//! Append-only: every statement is CREATE ... IF NOT EXISTS, so an existing
//! database is migrated by adding to the end of this list, never by editing a
//! statement already in it.

pub(crate) const SCHEMA: &[&str] = &[
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
