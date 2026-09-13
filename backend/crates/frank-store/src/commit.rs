//! Store commit persistence methods.

use super::*;

impl Store {
    pub async fn commit_command(
        &self,
        command_id: CommandId,
        expected_revision: Option<u64>,
        actor: ActorRef,
        event: Event,
        snapshot: Snapshot,
        result: frank_protocol::CommandResult,
    ) -> Result<Commit> {
        self.commit_request(CommitRequest {
            command_id,
            expected_revision,
            actor,
            event,
            snapshot,
            result,
            artifact_bytes: None,
        })
        .await
    }

    pub async fn commit_request(&self, request: CommitRequest<'_>) -> Result<Commit> {
        let CommitRequest {
            command_id,
            expected_revision,
            actor,
            event,
            mut snapshot,
            result,
            artifact_bytes,
        } = request;
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

    pub async fn begin(&self) -> Result<Transaction<'_, Sqlite>> {
        Ok(self.pool.begin().await?)
    }
}
