//! Store events persistence methods.

use super::*;

impl Store {
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
        sqlx::query("DELETE FROM journal_entries WHERE occurred_at < ?")
            .bind(cutoff)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }
}
