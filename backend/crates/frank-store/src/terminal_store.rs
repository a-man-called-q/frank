//! Store terminal store persistence methods.

use super::*;

impl Store {
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

    pub async fn prune_terminal_transcript_before(&self, cutoff: &Timestamp) -> Result<u64> {
        let result = sqlx::query("DELETE FROM terminal_transcript_chunks WHERE created_at < ?")
            .bind(cutoff)
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected())
    }
}
