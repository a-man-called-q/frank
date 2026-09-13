//! Store sessions persistence methods.

use super::*;

impl Store {
    pub async fn append_provider_session_item(
        &self,
        session_id: &str,
        item_id: &str,
        value: &Value,
    ) -> Result<u64> {
        if session_id.trim().is_empty() || item_id.trim().is_empty() {
            return Err(StoreError::Validation(
                "provider transcript identifiers are required".into(),
            ));
        }
        let value_json = serde_json::to_string(value)?;
        if value_json.len() > frank_protocol::MAX_COMMAND_BODY_BYTES {
            return Err(StoreError::Validation(
                "provider transcript item exceeds the configured size cap".into(),
            ));
        }
        let mut tx = self.pool.begin().await?;
        if let Some(sequence) = sqlx::query_scalar::<_, i64>(
            "SELECT sequence FROM provider_session_items WHERE session_id = ? AND item_id = ?",
        )
        .bind(session_id)
        .bind(item_id)
        .fetch_optional(&mut *tx)
        .await?
        {
            tx.commit().await?;
            return Ok(sequence.max(0) as u64);
        }
        let latest: i64 = sqlx::query_scalar(
            "SELECT COALESCE(MAX(sequence), 0) FROM provider_session_items WHERE session_id = ?",
        )
        .bind(session_id)
        .fetch_one(&mut *tx)
        .await?;
        let sequence = latest.max(0) as u64 + 1;
        sqlx::query("INSERT INTO provider_session_items (session_id, item_id, sequence, value_json, created_at) VALUES (?, ?, ?, ?, ?)")
            .bind(session_id)
            .bind(item_id)
            .bind(sequence as i64)
            .bind(value_json)
            .bind(timestamp_now())
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(sequence)
    }

    pub async fn provider_session_items(
        &self,
        session_id: &str,
    ) -> Result<Vec<StoredProviderSessionItem>> {
        let rows = sqlx::query("SELECT session_id, item_id, sequence, value_json, created_at FROM provider_session_items WHERE session_id = ? ORDER BY sequence ASC")
            .bind(session_id)
            .fetch_all(&self.pool)
            .await?;
        rows.into_iter()
            .map(|row| {
                let sequence: i64 = row.try_get("sequence")?;
                let value_json: String = row.try_get("value_json")?;
                Ok(StoredProviderSessionItem {
                    session_id: row.try_get("session_id")?,
                    item_id: row.try_get("item_id")?,
                    sequence: sequence.max(0) as u64,
                    value: serde_json::from_str(&value_json)?,
                    created_at: row.try_get("created_at")?,
                })
            })
            .collect()
    }
}
