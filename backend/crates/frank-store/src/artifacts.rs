//! Store artifacts persistence methods.

use super::*;

impl Store {
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

    pub async fn prune_artifacts_before(&self, cutoff: &Timestamp) -> Result<u64> {
        let snapshot = self.snapshot().await?;
        let completed_missions = snapshot
            .missions
            .iter()
            .filter(|mission| mission.status == frank_protocol::MissionStatus::Completed)
            .map(|mission| mission.id)
            .collect::<HashSet<_>>();
        let linked_rows =
            sqlx::query_scalar::<_, String>("SELECT DISTINCT artifact_id FROM task_artifact_links")
                .fetch_all(&self.pool)
                .await?;
        let linked_artifacts = linked_rows
            .into_iter()
            .filter_map(|id| ArtifactId::parse(&id).ok())
            .collect::<HashSet<_>>();
        let candidates = snapshot
            .artifacts
            .iter()
            .filter(|artifact| {
                completed_missions.contains(&artifact.mission_id)
                    && !artifact.pinned
                    && artifact.created_at < *cutoff
                    && !linked_artifacts.contains(&artifact.id)
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
}
