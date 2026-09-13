//! Store migrations persistence methods.

use super::*;

impl Store {
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

    pub async fn migrate_openrouter_runtime(&self) -> Result<Option<PathBuf>> {
        const MIGRATION_VERSION: i64 = 6;
        let applied: i64 =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?)")
                .bind(MIGRATION_VERSION)
                .fetch_one(&self.pool)
                .await?;
        if applied != 0 {
            return Ok(None);
        }

        let mut snapshot = self.snapshot().await?;
        let provider_projection_exists: i64 =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM provider_sessions LIMIT 1)")
                .fetch_one(&self.pool)
                .await?;
        let provider_transcript_exists: i64 =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM provider_session_items LIMIT 1)")
                .fetch_one(&self.pool)
                .await?;
        let needs_reset = provider_projection_exists != 0
            || provider_transcript_exists != 0
            || !snapshot.roles.is_empty()
            || !snapshot.agents.is_empty();
        let backup = if needs_reset {
            if let Some(database_path) = self.database_path.as_ref() {
                let name = database_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("frank.sqlite3");
                let destination = database_path.with_file_name(format!(
                    "{name}.openrouter-backup-{}.sqlite3",
                    uuid::Uuid::new_v4()
                ));
                Some(self.backup_to(&destination).await?)
            } else {
                None
            }
        } else {
            None
        };

        if needs_reset {
            snapshot.roles.clear();
            snapshot.agents.clear();
            // The OpenRouter runtime reset invalidates every old agent
            // reference. Keep the Organization document unpublished until
            // the owner recreates the roster against the new ids.
            snapshot.organization = frank_protocol::OrganizationStateView::default();
            snapshot.server.supervisor_model = None;
            for mission in &mut snapshot.missions {
                if !matches!(
                    mission.status,
                    frank_protocol::MissionStatus::Completed
                        | frank_protocol::MissionStatus::Failed
                        | frank_protocol::MissionStatus::Cancelled
                ) {
                    mission.supervisor_session_id = None;
                    mission.updated_at = timestamp_now();
                }
            }
            for task in &mut snapshot.tasks {
                if matches!(
                    task.status,
                    frank_protocol::TaskStatus::Done | frank_protocol::TaskStatus::Cancelled
                ) {
                    continue;
                }
                task.required_role_id = None;
                task.assigned_agent = None;
                task.claimed_at = None;
                task.claim_source = None;
                if task.status == frank_protocol::TaskStatus::Running {
                    task.status = frank_protocol::TaskStatus::Blocked;
                    snapshot.task_feed.push(frank_protocol::TaskFeedEntry {
                        id: frank_protocol::TaskFeedId::new(),
                        task_id: task.id,
                        actor: frank_protocol::ActorRef::system(),
                        kind: frank_protocol::TaskFeedKind::Comment,
                        body: "Team runtime reset for OpenRouter".into(),
                        artifact_ids: Vec::new(),
                        created_at: timestamp_now(),
                    });
                }
            }
        }

        let mut tx = self.pool.begin().await?;
        if needs_reset {
            for table in [
                "roles",
                "agent_profiles",
                "provider_sessions",
                "provider_session_items",
                "agent_capabilities",
                "budget_clocks",
                "organization_state",
                "connector_profiles",
            ] {
                sqlx::query(&format!("DELETE FROM {table}"))
                    .execute(&mut *tx)
                    .await?;
            }
            // Keep the narrow projections aligned with the authoritative
            // snapshot after clearing claims/assignments and adding reset
            // feed entries.
            sqlx::query("DELETE FROM missions")
                .execute(&mut *tx)
                .await?;
            for mission in &snapshot.missions {
                sqlx::query("INSERT INTO missions (mission_id, value_json) VALUES (?, ?)")
                    .bind(mission.id.to_string())
                    .bind(serde_json::to_string(mission)?)
                    .execute(&mut *tx)
                    .await?;
            }
            sqlx::query("DELETE FROM tasks").execute(&mut *tx).await?;
            sqlx::query("DELETE FROM task_dependencies")
                .execute(&mut *tx)
                .await?;
            for task in &snapshot.tasks {
                sqlx::query("INSERT INTO tasks (task_id, mission_id, value_json) VALUES (?, ?, ?)")
                    .bind(task.id.to_string())
                    .bind(task.mission_id.to_string())
                    .bind(serde_json::to_string(task)?)
                    .execute(&mut *tx)
                    .await?;
                for dependency in &task.dependencies {
                    sqlx::query(
                        "INSERT INTO task_dependencies (task_id, depends_on) VALUES (?, ?)",
                    )
                    .bind(task.id.to_string())
                    .bind(dependency.to_string())
                    .execute(&mut *tx)
                    .await?;
                }
            }
            sqlx::query("DELETE FROM task_feed")
                .execute(&mut *tx)
                .await?;
            for feed in &snapshot.task_feed {
                sqlx::query("INSERT INTO task_feed (feed_id, task_id, kind, actor_json, value_json, created_at) VALUES (?, ?, ?, ?, ?, ?)")
                    .bind(feed.id.to_string())
                    .bind(feed.task_id.to_string())
                    .bind(serde_json::to_value(feed.kind)?.as_str().unwrap_or("comment"))
                    .bind(serde_json::to_string(&feed.actor)?)
                    .bind(serde_json::to_string(feed)?)
                    .bind(&feed.created_at)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        sqlx::query(
            "UPDATE server_state SET snapshot_json = ?, revision = ?, event_seq = ? WHERE id = 1",
        )
        .bind(serde_json::to_string(&snapshot)?)
        .bind(snapshot.revision as i64)
        .bind(snapshot.event_seq as i64)
        .execute(&mut *tx)
        .await?;
        if let Some(backup_path) = &backup {
            sqlx::query("INSERT INTO migration_backups (backup_id, source_path, backup_path, schema_version, created_at) VALUES (?, ?, ?, ?, ?)")
                .bind(uuid::Uuid::new_v4().to_string())
                .bind(self.database_path.as_deref().unwrap_or_else(|| Path::new(":memory:")).to_string_lossy().to_string())
                .bind(backup_path.to_string_lossy().to_string())
                .bind(MIGRATION_VERSION)
                .bind(timestamp_now())
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("INSERT OR IGNORE INTO schema_migrations (version, applied_at) VALUES (?, ?)")
            .bind(MIGRATION_VERSION)
            .bind(timestamp_now())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(backup)
    }

    pub async fn migrate_openrouter_snapshot(&self) -> Result<Option<PathBuf>> {
        const MIGRATION_VERSION: i64 = 7;
        let applied: i64 =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?)")
                .bind(MIGRATION_VERSION)
                .fetch_one(&self.pool)
                .await?;
        if applied != 0 {
            return Ok(None);
        }
        let raw: String = sqlx::query_scalar("SELECT snapshot_json FROM server_state WHERE id = 1")
            .fetch_one(&self.pool)
            .await?;
        let mut value: Value = serde_json::from_str(&raw)?;
        let mut changed = false;
        let openrouter_sessions = sqlx::query_scalar::<_, String>(
            "SELECT session_id FROM provider_sessions WHERE lower(provider) = 'openrouter'",
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .collect::<HashSet<_>>();
        if let Some(server) = value.get_mut("server").and_then(Value::as_object_mut) {
            for key in [
                "provider",
                "supervisor_provider",
                "max_provider_concurrency",
            ] {
                changed |= server.remove(key).is_some();
            }
        }
        for collection in ["roles", "agents", "missions"] {
            if let Some(items) = value.get_mut(collection).and_then(Value::as_array_mut) {
                for item in items {
                    if let Some(object) = item.as_object_mut() {
                        for key in ["provider", "supervisor_provider"] {
                            changed |= object.remove(key).is_some();
                        }
                        let session_key = if collection == "missions" {
                            "supervisor_session_id"
                        } else if collection == "agents" {
                            "provider_session_id"
                        } else {
                            ""
                        };
                        if !session_key.is_empty()
                            && object.get(session_key).and_then(Value::as_str).is_some_and(
                                |session_id| {
                                    !session_id.trim().is_empty()
                                        && !openrouter_sessions.contains(session_id)
                                },
                            )
                        {
                            object.insert(session_key.to_string(), Value::Null);
                            changed = true;
                        }
                    }
                }
            }
        }
        // Usage provider ids are intentionally not touched.
        let backup = if changed {
            if let Some(database_path) = self.database_path.as_ref() {
                let name = database_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("frank.sqlite3");
                let destination = database_path.with_file_name(format!(
                    "{name}.openrouter-v7-backup-{}.sqlite3",
                    uuid::Uuid::new_v4()
                ));
                Some(self.backup_to(&destination).await?)
            } else {
                None
            }
        } else {
            None
        };
        let snapshot: Snapshot = serde_json::from_value(value.clone())?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("UPDATE server_state SET snapshot_json = ? WHERE id = 1")
            .bind(serde_json::to_string(&value)?)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM provider_sessions WHERE lower(provider) <> 'openrouter'")
            .execute(&mut *tx)
            .await?;
        // Keep the narrow projections in lockstep with the canonical
        // snapshot. Event/audit tables are intentionally never rewritten.
        sqlx::query("DELETE FROM missions")
            .execute(&mut *tx)
            .await?;
        for mission in &snapshot.missions {
            sqlx::query("INSERT INTO missions (mission_id, value_json) VALUES (?, ?)")
                .bind(mission.id.to_string())
                .bind(serde_json::to_string(mission)?)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("DELETE FROM roles").execute(&mut *tx).await?;
        for role in &snapshot.roles {
            sqlx::query("INSERT INTO roles (role_id, value_json, archived) VALUES (?, ?, ?)")
                .bind(role.id.to_string())
                .bind(serde_json::to_string(role)?)
                .bind(if role.archived { 1_i64 } else { 0_i64 })
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("DELETE FROM agent_profiles")
            .execute(&mut *tx)
            .await?;
        for agent in &snapshot.agents {
            sqlx::query(
                "INSERT INTO agent_profiles (agent_id, value_json, archived) VALUES (?, ?, ?)",
            )
            .bind(agent.id.to_string())
            .bind(serde_json::to_string(agent)?)
            .bind(if agent.archived { 1_i64 } else { 0_i64 })
            .execute(&mut *tx)
            .await?;
        }
        if let Some(backup_path) = &backup {
            sqlx::query("INSERT INTO migration_backups (backup_id, source_path, backup_path, schema_version, created_at) VALUES (?, ?, ?, ?, ?)")
                .bind(uuid::Uuid::new_v4().to_string())
                .bind(self.database_path.as_deref().unwrap_or_else(|| Path::new(":memory:")).to_string_lossy().to_string())
                .bind(backup_path.to_string_lossy().to_string())
                .bind(MIGRATION_VERSION)
                .bind(timestamp_now())
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("INSERT OR IGNORE INTO schema_migrations (version, applied_at) VALUES (?, ?)")
            .bind(MIGRATION_VERSION)
            .bind(timestamp_now())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(backup)
    }
}
